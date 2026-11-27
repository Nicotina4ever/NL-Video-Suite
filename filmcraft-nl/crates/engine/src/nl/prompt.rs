//! Prompt drafting with the local model (the Prompter's "Prompt AI", reduced to what the panels
//! need): a brief becomes a generation prompt (and optional variants) that are validated before
//! they reach a workflow. The model never sees or returns node ids, links or class names, and the
//! result is suggested, not applied: the panel fills the field only when the operator clicks.
//!
//! Long calls run as a background job (`wait: false`); the result lands in
//! `s.nl.results["prompt"]` and the panel offers it. The model is unloaded when it answers
//! (`keep_alive: 0`) so a ComfyUI generation that follows does not share VRAM with it.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{Result, Session};

use super::state::PendingPrompt;

const MAX_PROMPT: usize = 1500;
const MAX_VARIANTS: usize = 4;

fn bad_cmd(cmd: &str, msg: impl ToString) -> crate::EngineError {
    bad(cmd, msg.to_string())
}

fn workflow_guide(workflow: &str) -> &'static str {
    let name = workflow.to_ascii_lowercase();
    if name.contains("minimax") || name.contains("h3") {
        "Guida MiniMax H3: scrivi un prompt per un video con reference. Descrivi movimento, camera, performance e atmosfera; \
         non descrivere staticamente l'immagine. Se il brief nomina reference, usa i tag <Picture 1>, <Picture 2>… nello stesso ordine dei file."
    } else if name.contains("wan") {
        "Guida Wan i2v: l'immagine è l'ancora visiva; descrivi solo movimento, camera e ciò che cambia, non riscrivere l'immagine."
    } else if name.contains("flux") || name.contains("ideogram") || name.contains("krea") || name.contains("z-image") || name.contains("z_image") {
        "Guida immagine cinematografica: soggetto, espressione, costume, ambiente, luce, composizione e obiettivo; una sola immagine, dettagli concreti."
    } else if name.contains("music") || name.contains("ace_step") || name.contains("audio") {
        "Guida musica: genere, strumenti, atmosfera, energia, tempo/BPM e struttura; niente testo parlato se non richiesto."
    } else {
        "Guida generica: prompt visivo concreto, senza nomi di modello, risoluzioni o parametri tecnici."
    }
}

/// Optional operator guide for one workflow: `<workflow>.md`, `<workflow>.guide.md`, or
/// `guides/<workflow>.md` next to the JSON. The file is data for the model, never instructions to
/// the app; it is bounded and ignored when missing.
fn read_workflow_guide(s: &Session, workflow: &str) -> Option<String> {
    if workflow.trim().is_empty() {
        return None;
    }
    let file = super::models::find(s, workflow)?;
    let stem = file.file_stem()?.to_string_lossy().to_string();
    let parent = file.parent()?;
    for candidate in [parent.join(format!("{stem}.md")), parent.join(format!("{stem}.guide.md")), parent.join("guides").join(format!("{stem}.md"))] {
        if let Ok(text) = std::fs::read_to_string(&candidate) {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.chars().take(12_000).collect());
            }
        }
    }
    // ComfyUI's own tutorial bundle in workflows/docs, matched by the model family in the file
    // name. The operator can add or replace these guides without moving a JSON.
    let name = workflow.to_ascii_lowercase();
    let relative = if name.contains("minimax") && name.contains("h3") {
        Some("video/minimax/minimax-h3-prompt-guide.mdx")
    } else if name.contains("minimax") || name.contains("music") {
        Some("audio/minimax/minimax-music-3.mdx")
    } else if name.contains("krea") {
        Some("image/krea/krea-2.mdx")
    } else if name.contains("flux2") || name.contains("flux-2") {
        Some("flux/flux-2-dev.mdx")
    } else if name.contains("ideogram") {
        Some("image/ideogram/ideogram-v4.mdx")
    } else if name.contains("qwen") {
        Some("image/qwen/qwen-image.mdx")
    } else if name.contains("z-image") || name.contains("z_image") {
        Some("image/z-image/z-image-turbo.mdx")
    } else if name.contains("ace") {
        Some("audio/ace-step/ace-step-v1-5.mdx")
    } else if name.contains("wan") {
        Some("video/wan/wan2_2.mdx")
    } else {
        None
    };
    let relative = relative?;
    let path = super::suite_root()?.join("workflows").join("docs").join("tutorials").join(relative);
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.split("---").nth(2).unwrap_or(&text).trim();
    (!text.is_empty()).then(|| text.chars().take(8_000).collect())
}

fn system_prompt(kind: &str, workflow: &str, guide: Option<&str>) -> String {
    let medium = match kind {
        "video" | "v2v" => "video (descrivi soggetto, azione, movimento di camera e ambiente in inglese)",
        "music" => "musica (genere, strumenti, atmosfera e ritmo in inglese)",
        _ => "immagine (descrivi soggetto, azione, ambiente, luce e inquadratura in inglese)",
    };
    let extra = guide.map(|guide| format!("\nGuida dell'operatore per questo workflow:\n{guide}\n")).unwrap_or_default();
    format!(
        "Sei un prompt engineer per una generazione di {medium}. {}{extra}\
Rispondi SOLO con un oggetto JSON valido: {{\"prompt\": string, \"variants\": [string, ...]}} (massimo {MAX_VARIANTS} varianti). \
Il prompt deve essere concreto e visivo, massimo {MAX_PROMPT} caratteri, senza testo attorno. \
NON citare nomi di modelli, risoluzioni, durate, aspect ratio, node id, link o class_type. \
Mantieni la lingua inglese per il prompt, qualunque sia la lingua del brief.",
        workflow_guide(workflow)
    )
}

/// Remove resolution/aspect wording the models like to add: the size comes from the sequence, so a
/// prompt that says "8k" or "16:9" is contradicting the panel.
fn strip_resolution(text: &str) -> String {
    let mut out = text.to_string();
    for phrase in [
        "8k resolution",
        "4k resolution",
        "high resolution",
        "ultra high resolution",
        "aspect ratio",
        "8k",
        "6k",
        "4k",
        "2k",
        "2160p",
        "1440p",
        "1080p",
        "720p",
        "16:9",
        "9:16",
        "1:1",
    ] {
        out = out.replace(phrase, "");
    }
    // Tidy the gaps the removals leave: double spaces, space before punctuation, doubled commas.
    while out.contains("  ") {
        out = out.replace("  ", " ");
    }
    for (from, to) in [(" ,", ","), (" .", "."), (",,", ","), (",.", ".")] {
        while out.contains(from) {
            out = out.replace(from, to);
        }
    }
    out.trim().to_string()
}

/// Validate the model's answer: strings only, bounded, and no workflow internals leaked in.
fn clean(value: &Value) -> std::result::Result<Value, String> {
    let object = value.as_object().ok_or("the model did not answer a JSON object")?;
    let text = |value: Option<&Value>, max: usize| -> std::result::Result<String, String> {
        let text = value.and_then(Value::as_str).ok_or("the model answered without a prompt")?;
        let text = strip_resolution(text.trim());
        if text.is_empty() || text.chars().count() > max {
            return Err(format!("the prompt is empty or longer than {max} characters"));
        }
        let lower = text.to_ascii_lowercase();
        if lower.contains("class_type") || lower.contains("\"inputs\"") || lower.contains("node id") {
            return Err("the model leaked workflow internals into the prompt".into());
        }
        Ok(text)
    };
    let prompt = text(object.get("prompt"), MAX_PROMPT)?;
    let mut variants = Vec::new();
    for variant in object.get("variants").and_then(Value::as_array).into_iter().flatten().take(MAX_VARIANTS) {
        if let Ok(text) = text(Some(variant), MAX_PROMPT) {
            variants.push(text);
        }
    }
    Ok(json!({"prompt": prompt, "variants": variants}))
}

fn call(
    llm: &super::llm::LlmConfig,
    brief: &str,
    kind: &str,
    language: &str,
    workflow: &str,
    guide: Option<&str>,
    timeout: Duration,
) -> std::result::Result<Value, String> {
    let prompt = format!("Brief dell'operatore (lingua: {language}): {brief}\nScrivi il prompt di generazione.");
    let value = llm.auto_json(&system_prompt(kind, workflow, guide), &prompt, timeout, Some("0"))?;
    clean(&value)
}

/// `nl.prompt.enhance` — brief → validated prompt (+ variants), suggested to the operator.
fn enhance(s: &mut Session, p: &Value) -> Result<Value> {
    let brief = p
        .get("brief")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .ok_or_else(|| bad_cmd("nl.prompt.enhance", "need `brief`"))?
        .to_string();
    if brief.chars().count() > 4000 {
        return Err(bad_cmd("nl.prompt.enhance", "`brief` is too long (4000 characters max)"));
    }
    let kind = p.get("kind").and_then(Value::as_str).unwrap_or("image").to_string();
    if !matches!(kind.as_str(), "image" | "video" | "v2v" | "music") {
        return Err(bad_cmd("nl.prompt.enhance", "`kind` must be image, video, v2v or music"));
    }
    let language = p.get("language").and_then(Value::as_str).unwrap_or("it").to_string();
    let workflow = p.get("workflow").and_then(Value::as_str).unwrap_or("").to_string();
    let guide = read_workflow_guide(s, &workflow);
    let llm = super::llm::LlmConfig::for_role(s, "prompt", p.get("model").and_then(Value::as_str));
    let wait = p.get("wait").and_then(Value::as_bool).unwrap_or(false);
    if wait {
        let answer =
            call(&llm, &brief, &kind, &language, &workflow, guide.as_deref(), Duration::from_secs(180)).map_err(|e| bad_cmd("nl.prompt.enhance", e))?;
        let mut result = answer;
        result["model"] = json!(llm.model);
        result["kind"] = json!(kind);
        result["workflow"] = json!(workflow);
        result["guide"] = json!(guide.is_some());
        return Ok(result);
    }
    let result: Arc<Mutex<Option<std::result::Result<Value, String>>>> = Arc::default();
    let out = result.clone();
    let id = s.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let job = crate::Job { id, label: format!("NL prompt ({})", llm.model), progress: Default::default(), result: Default::default() };
    let progress = job.progress.clone();
    let llm_run = llm.clone();
    let kind_run = kind.clone();
    let language_run = language.clone();
    let workflow_run = workflow.clone();
    let guide_run = guide.clone();
    s.jobs.push(job);
    s.nl.prompt_pending = Some(PendingPrompt { job: id, result, epoch: s.document_epoch, key: "prompt" });
    std::thread::Builder::new()
        .name("filmcraft-nl-prompt".into())
        .spawn(move || {
            *progress.status.lock().unwrap_or_else(|e| e.into_inner()) = format!("NL prompt: writing with {}", llm_run.model);
            let value = call(&llm_run, &brief, &kind_run, &language_run, &workflow_run, guide_run.as_deref(), Duration::from_secs(180));
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
            progress.finished.store(true, std::sync::atomic::Ordering::Relaxed);
        })
        .map_err(|e| crate::EngineError::Other(format!("can't start the prompt thread: {e}")))?;
    Ok(json!({"job": id, "model": llm.model, "kind": kind}))
}

/// Move a finished prompt answer into `s.nl.results["prompt"]`.
pub(crate) fn poll(s: &mut Session) {
    let finished =
        s.nl.prompt_pending
            .as_ref()
            .is_none_or(|pending| s.jobs.iter().find(|j| j.id == pending.job).is_none_or(|j| j.progress.finished.load(std::sync::atomic::Ordering::Relaxed)));
    if !finished {
        return;
    }
    let Some(pending) = s.nl.prompt_pending.take() else { return };
    let Some(result) = pending.result.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
    if pending.epoch != s.document_epoch {
        s.error_toast("nl.prompt.enhance", "the project was replaced; the suggested prompt was discarded");
        return;
    }
    match result {
        Ok(value) => {
            s.nl.results.insert(pending.key, value.clone());
            s.toast("NL prompt: suggestion ready (apply it from the panel)");
        }
        Err(e) => s.error_toast("nl.prompt", format!("NL prompt: {e}")),
    }
}

/// H3 section headers by mode, in the order MiniMax's guide requires.
fn h3_sections(mode: &str) -> Vec<&'static str> {
    if mode == "ref" {
        vec!["subject_definitions:", "summary:", "retention_analysis:", "detailed_description:", "overall_soundscape:", "non_diegetic_music:"]
    } else {
        vec!["integrated_multimodal_description:", "overall_soundscape:", "non_diegetic_music:"]
    }
}

/// The exact alignment line a keyframe mode needs, computed here so the model cannot get it wrong.
fn h3_instruction(mode: &str, seconds: Option<f64>, final_shot: Option<u64>) -> Option<String> {
    let seconds = seconds.unwrap_or(0.0);
    match mode {
        "i2va" => Some("I2VA: For the target video, at 0.00 seconds into the target video, <Picture 1> (from [Shot 1]) is fully referenced.".into()),
        "fl2va" => Some(format!(
            "FL2VA: How the reference pictures align with the target video — Picture 1 (from Shot 1) aligns with the 0.00-second mark of the target video; Picture 2 (from Shot {}) aligns with the {seconds:.2}-second mark of the target video.",
            final_shot.unwrap_or(1)
        )),
        "l2va" => Some(format!(
            "L2VA: How the reference pictures align with the target video — <Picture 1> (from Shot {}) aligns with the {seconds:.2}-second mark of the target video.",
            final_shot.unwrap_or(1)
        )),
        _ => None,
    }
}

/// Ask the local model for an H3 prompt: base modes produce the three sections, `ref` the six
/// full-reference sections. The alignment line for i2va/fl2va/l2va is prepended by us.
fn h3_system(mode: &str) -> String {
    let base = "Regole: inizia [Shot 1] con stile visivo e composizione, senza timestamp. Gli shot successivi iniziano con un tempo di taglio crescente, per esempio '[Shot 2] At 00:03.500, the camera cuts to ...'. Usa tagli solo quando aggiungono informazione; per piccoli cambi usa il movimento di camera. Descrivi eventi visivi e sonori osservabili in ordine cronologico. Non citare nome del modello, risoluzione, aspect ratio o durata fuori dai timestamp. Metti il dialogo esatto in <d>[Lingua] parole esatte</d> e assegna (S1),(S2) agli speaker in ordine di prima voce. overall_soundscape: 1-4 frasi su ambiente, suoni fisici e suoni umani non verbali, senza ripetere il dialogo; non_diegetic_music: 1-3 frasi su strumentazione, tempo e sviluppo, oppure N/A.";
    if mode == "ref" {
        format!(
            "Scrivi un prompt MiniMax H3 in modalità full-reference (immagini, video, soggetti e audio riutilizzabili). Rispondi SOLO con il testo del prompt, senza markdown. \
Formato ESATTO, sei campi in questo ordine separati da una riga vuota, senza altro testo fuori dai campi:\nsubject_definitions:\n<Subject 1> is ... from <Picture 1>.\n\nsummary:\n[video editing] ...\n\nretention_analysis:\n<Subject 1>: fully_preserved — ...\n\ndetailed_description:\nStyle. [Shot 1] ...\n\noverall_soundscape:\n...\n\nnon_diegetic_music:\n...\n\
Definisci ogni riferimento con una riga: '<Subject N> ...', '<Picture N> ...', '<Video N> ...', '<Audio N> ...', citando la sorgente nella definizione; label stabili in tutte le sezioni. \
summary inizia con una combinazione tra parentesi quadre scelta tra: keyframe completion, reference generation, video editing, video continuation, audio reuse, audio reference (unite da +). \
retention_analysis: una riga per ogni label definito, con un solo valore: fully_preserved, partially_preserved, attribute_transfer, weak_reference per il visivo; fully_copy, partially_copy, reference, weak_reference per l'audio, spiegando cosa è conservato o trasferito. \
detailed_description: 350-500 parole, stile in 1-2 frasi prima di [Shot 1], poi le regole degli shot. {base}"
        )
    } else {
        format!(
            "Scrivi un prompt MiniMax H3 (video con audio) in modalità testo→video+audio. Rispondi SOLO con il testo del prompt, senza markdown. \
Formato ESATTO, tre campi in questo ordine separati da una riga vuota, senza altro testo fuori dai campi:\nintegrated_multimodal_description: [Shot 1] ...\n\noverall_soundscape: ...\n\nnon_diegetic_music: ...\n{base}\nEsempio di intestazioni da copiare letteralmente: integrated_multimodal_description: [Shot 1] A man enters a studio. At 00:02.000, the camera cuts to a close-up. ... / overall_soundscape: ... / non_diegetic_music: ..."
        )
    }
}

/// Put the right `[Language]` tag inside the first `<d>…</d>` block: the guide requires it and a
/// local model often writes `[English]` for Italian words, or no tag at all.
fn fix_dialogue_tag(mut text: String, dialogue: Option<&str>, language: &str) -> String {
    if dialogue.is_none() {
        return text;
    }
    let label = language_label(language);
    let Some(start) = text.find("<d>") else { return text };
    let content_start = start + "<d>".len();
    let Some(end) = text[content_start..].find("</d>").map(|end| content_start + end) else { return text };
    let content = text[content_start..end].to_string();
    let trimmed = content.trim_start();
    let leading = &content[..content.len() - trimmed.len()];
    let mut rest = trimmed;
    if let Some(stripped) = trimmed.strip_prefix('[')
        && let Some(close) = stripped.find(']')
    {
        rest = &stripped[close + 1..];
    }
    // A model may also write the language as a bare word ("English Ci siamo!"): drop it.
    let first_end = rest.char_indices().find(|(_, c)| c.is_whitespace()).map(|(index, _)| index).unwrap_or(rest.len());
    let first = &rest[..first_end];
    const LANGUAGES: [&str; 9] = ["english", "italian", "italiano", "spanish", "español", "french", "français", "german", "deutsch"];
    if LANGUAGES.contains(&first.to_lowercase().as_str()) {
        rest = rest[first_end..].trim_start();
    }
    let new_content = format!("{leading}[{label}]{rest}");
    text.replace_range(content_start..end, &new_content);
    text
}

/// The exact dialogue asked for must appear in the prompt, verbatim: the guide requires it and a
/// local model easily leaves it out.
fn language_label(language: &str) -> &'static str {
    match language.to_ascii_lowercase().as_str() {
        "it" | "italiano" => "Italian",
        "es" | "español" => "Spanish",
        "fr" | "français" => "French",
        "de" | "deutsch" => "German",
        _ => "English",
    }
}

fn check_dialogue(text: String, dialogue: Option<&str>, language: &str) -> std::result::Result<String, String> {
    let Some(dialogue) = dialogue else { return Ok(text) };
    // Compare the words, not the punctuation: a model may drop the final "!" while keeping the
    // exact words, and the guide cares about the words.
    let words = |value: &str| -> String {
        value
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    if !words(&text).contains(&words(dialogue)) {
        return Err(format!("il dialogo richiesto non compare nel prompt: {dialogue}"));
    }
    let label = language_label(language);
    let tagged = text
        .match_indices("<d>")
        .any(|(start, _)| text[start..].find("</d>").map(|end| text[start..start + end].contains(&format!("[{label}]"))).unwrap_or(false));
    if !tagged {
        return Err(format!("il dialogo deve stare dentro <d>[{label}] ...</d> per essere conforme allo schema H3"));
    }
    Ok(text)
}

/// Ask for the H3 prompt and, if the answer breaks the schema, ask once more with the error said
/// back; a small local model often misses a header on the first try.
fn request_h3(
    llm: &super::llm::LlmConfig,
    system: &str,
    user: &str,
    mode: &str,
    dialogue: Option<&str>,
    language: &str,
) -> std::result::Result<String, String> {
    let first = llm.auto_text(system, user, Duration::from_secs(300))?;
    match clean_h3(mode, &first).map(|text| fix_dialogue_tag(text, dialogue, language)).and_then(|text| check_dialogue(text, dialogue, language)) {
        Ok(text) => Ok(text),
        Err(error) => {
            let retry = format!(
                "{user}\n\nLa risposta precedente non rispettava il formato richiesto ({error}). Riscrivi il prompt rispettando ESATTAMENTE le intestazioni e i campi, senza altro testo attorno."
            );
            let second = llm.auto_text(system, &retry, Duration::from_secs(300))?;
            clean_h3(mode, &second).map(|text| fix_dialogue_tag(text, dialogue, language)).and_then(|text| check_dialogue(text, dialogue, language)).map_err(
                |second_error| {
                    let excerpt: String = second.trim().chars().take(300).collect();
                    format!("{error} (anche al secondo tentativo: {second_error}; risposta: {excerpt})")
                },
            )
        }
    }
}

/// Validate an H3 prompt: sections present in order, labels defined in `ref`, and no workflow
/// internals or settings leaked into the prose.
fn clean_h3(mode: &str, text: &str) -> std::result::Result<String, String> {
    let text = strip_resolution(text.trim());
    if text.is_empty() {
        return Err("the model answered an empty prompt".into());
    }
    if text.chars().count() > 8000 {
        return Err("the H3 prompt is longer than 8000 characters".into());
    }
    let lower = text.to_ascii_lowercase();
    if lower.contains("class_type") || lower.contains("\"inputs\"") || lower.contains("node id") {
        return Err("the model leaked workflow internals into the prompt".into());
    }
    let mut position = 0;
    for header in h3_sections(mode) {
        let Some(found) = text[position..].find(header).map(|i| i + position) else {
            return Err(format!("the H3 prompt is missing the `{header}` section"));
        };
        position = found + header.len();
    }
    if mode != "ref" {
        let start = text.find("integrated_multimodal_description:").map(|i| i + "integrated_multimodal_description:".len()).unwrap_or(0);
        let shot = text[start..].trim_start().starts_with("[Shot 1]");
        if !shot {
            return Err("the H3 timeline must start with [Shot 1]".into());
        }
    } else {
        let definitions = text.split("summary:").next().unwrap_or_default();
        let labels: Vec<&str> = ["<Subject ", "<Picture ", "<Video ", "<Audio "]
            .into_iter()
            .flat_map(|prefix| definitions.match_indices(prefix).map(|(i, _)| &definitions[i..]).collect::<Vec<_>>())
            .map(|rest| &rest[..rest.find('>').map(|end| end + 1).unwrap_or(rest.len())])
            .collect();
        let definitions_with_labels = labels.iter().filter(|label| definitions.contains(&format!("{label} is "))).count();
        if definitions_with_labels == 0 {
            return Err("the full-reference prompt defines no subject/picture/video/audio label".into());
        }
        let retention = text.split("retention_analysis:").nth(1).and_then(|rest| rest.split("detailed_description:").next()).unwrap_or_default();
        for label in labels {
            if definitions.contains(&format!("{label} is ")) && !retention.contains(label) {
                return Err(format!("{label} has no retention_analysis line"));
            }
        }
    }
    Ok(text)
}

/// `nl.prompt.h3` — an H3 prompt in the model's own schema (base, keyframe or full-reference).
fn h3(s: &mut Session, p: &Value) -> Result<Value> {
    let brief =
        p.get("brief").and_then(Value::as_str).map(str::trim).filter(|b| !b.is_empty()).ok_or_else(|| bad_cmd("nl.prompt.h3", "need `brief`"))?.to_string();
    let mode = p.get("mode").and_then(Value::as_str).unwrap_or("t2va").to_ascii_lowercase();
    if !matches!(mode.as_str(), "t2va" | "i2va" | "fl2va" | "l2va" | "ref") {
        return Err(bad_cmd("nl.prompt.h3", "`mode` must be t2va, i2va, fl2va, l2va or ref"));
    }
    let seconds = p.get("seconds").and_then(Value::as_f64).filter(|s| s.is_finite() && *s > 0.0);
    if matches!(mode.as_str(), "fl2va" | "l2va") && seconds.is_none() {
        return Err(bad_cmd("nl.prompt.h3", format!("mode {mode} needs `seconds` (the effective duration used for the alignment line)")));
    }
    let final_shot = p.get("finalShot").and_then(Value::as_u64);
    let dialogue = p.get("dialogue").and_then(Value::as_str).map(str::trim).filter(|d| !d.is_empty());
    let references = p.get("references").and_then(Value::as_array).cloned().unwrap_or_default();
    let language = p.get("language").and_then(Value::as_str).unwrap_or("it");
    let mut user = format!(
        "Brief dell'operatore: {brief}\nModalità: {mode}.\nScrivi il prompt in inglese; solo il dialogo e i testi visibili restano nella loro lingua (brief in {language})."
    );
    if let Some(seconds) = seconds {
        user.push_str(&format!("\nDurata effettiva: {seconds:.2} secondi (usala solo per i timestamp/alignment, non scriverla come impostazione)."));
    }
    if let Some(dialogue) = dialogue {
        user.push_str(&format!("\nDialogo esatto da riportare parola per parola: {dialogue}"));
    }
    if !references.is_empty() {
        user.push_str("\nRiferimenti disponibili:");
        for reference in &references {
            let role = reference.get("role").and_then(Value::as_str).unwrap_or("picture");
            let description = reference.get("description").and_then(Value::as_str).unwrap_or("reference");
            user.push_str(&format!("\n- {role}: {description}"));
        }
    }
    let system = h3_system(&mode);
    let llm = super::llm::LlmConfig::for_role(s, "prompt", p.get("model").and_then(Value::as_str));
    let instruction = h3_instruction(&mode, seconds, final_shot);
    let wait = p.get("wait").and_then(Value::as_bool).unwrap_or(false);
    if wait {
        let text = request_h3(&llm, &system, &user, &mode, dialogue, language).map_err(|e| bad_cmd("nl.prompt.h3", e))?;
        return Ok(compose_h3(&instruction, text, &mode, &llm.model, seconds));
    }
    let result: Arc<Mutex<Option<std::result::Result<Value, String>>>> = Arc::default();
    let out = result.clone();
    let id = s.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let job = crate::Job { id, label: format!("NL H3 prompt ({})", llm.model), progress: Default::default(), result: Default::default() };
    let progress = job.progress.clone();
    let llm_run = llm.clone();
    let mode_run = mode.clone();
    let instruction_run = instruction.clone();
    let dialogue_run = dialogue.map(str::to_string);
    let language_run = language.to_string();
    s.jobs.push(job);
    s.nl.prompt_pending = Some(PendingPrompt { job: id, result, epoch: s.document_epoch, key: "h3prompt" });
    std::thread::Builder::new()
        .name("filmcraft-nl-h3".into())
        .spawn(move || {
            *progress.status.lock().unwrap_or_else(|e| e.into_inner()) = format!("NL H3 prompt: writing with {}", llm_run.model);
            let value = request_h3(&llm_run, &system, &user, &mode_run, dialogue_run.as_deref(), &language_run)
                .map(|text| compose_h3(&instruction_run, text, &mode_run, &llm_run.model, seconds));
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
            progress.finished.store(true, std::sync::atomic::Ordering::Relaxed);
        })
        .map_err(|e| crate::EngineError::Other(format!("can't start the H3 prompt thread: {e}")))?;
    Ok(json!({"job": id, "model": llm.model, "mode": mode}))
}

/// The final H3 text: our computed alignment line (when the mode needs one) and the model answer.
fn compose_h3(instruction: &Option<String>, text: String, mode: &str, model: &str, seconds: Option<f64>) -> Value {
    let prompt = match instruction {
        Some(instruction) => format!("{instruction}\n\n{text}"),
        None => text,
    };
    json!({"prompt": prompt, "mode": mode, "model": model, "seconds": seconds})
}

/// `nl.prompt.fromObject` — read the selected frame with the vision model, read the spoken text
/// under it, and build the prompt from that context instead of from an empty brief.
fn from_object(s: &mut Session, p: &Value) -> Result<Value> {
    let clip = p.get("clip").and_then(Value::as_u64).ok_or_else(|| bad_cmd("nl.prompt.fromObject", "serve `clip`"))?;
    let (_, item) = s
        .active_sequence()
        .and_then(|sequence| sequence.find_item(filmcraft_project::ClipId(clip)))
        .ok_or_else(|| bad_cmd("nl.prompt.fromObject", "clip not found"))?;
    let item_id = item.item.0;
    let path = super::agent::media_path(s, item_id).ok_or_else(|| bad_cmd("nl.prompt.fromObject", "the clip has no media file"))?;
    let mut endpoint = super::analysis::vision_endpoint(s, p);
    let mut model = p
        .get("visionModel")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| (!s.prefs.nl.vision_model.trim().is_empty()).then(|| s.prefs.nl.vision_model.clone()))
        .or_else(|| super::analysis::first_model(&endpoint).ok());
    // Ollama exposes its vision models on the OpenAI-compatible endpoint too; when the configured
    // llama.cpp endpoint is empty, use it instead of refusing to analyse the frame.
    if model.is_none() {
        endpoint = "http://127.0.0.1:11434/v1".to_string();
        for candidate in ["gemma4:e4b", "gemma4:latest", "qwen3.5:9b"] {
            if super::analysis::ask_vision(&endpoint, candidate, &[], "ping").is_ok() || super::analysis::first_model(&endpoint).is_ok() {
                model = Some(candidate.to_string());
                break;
            }
        }
    }
    let model = model.ok_or_else(|| bad_cmd("nl.prompt.fromObject", "no vision model: set the NL vision model in Settings"))?;
    let bytes = std::fs::read(&path).map_err(|e| bad_cmd("nl.prompt.fromObject", format!("{path}: {e}")))?;
    let describe = "Analizza il frame: soggetto, identita, abbigliamento, ambiente, luce, camera, stile. Rispondi in inglese, concreto, massimo 4 frasi.";
    let mut used_model = model.clone();
    let mut description = match super::analysis::ask_vision(&endpoint, &model, &bytes, describe) {
        Ok(text) => text,
        Err(_) => {
            // The configured endpoint may be llama.cpp (default :8080) and refuse or drop the
            // request. Ollama on :11434 exposes vision models on the same OpenAI-compatible API:
            // retry there before giving up, using a reasoning-capable vision model.
            let ollama = "http://127.0.0.1:11434/v1".to_string();
            let mut text = String::new();
            for candidate in ["gemma4:e4b", "gemma4:latest", "qwen3.5:9b"] {
                if let Ok(value) = super::analysis::ask_vision(&ollama, candidate, &bytes, describe)
                    && !value.trim().is_empty()
                {
                    used_model = candidate.to_string();
                    text = value;
                    break;
                }
            }
            text
        }
    };
    if description.trim().is_empty() {
        description = String::new();
    }
    let start = item.start.0 as f64 / 254_016_000_000.0;
    let end = (item.start.0 + item.duration.0) as f64 / 254_016_000_000.0;
    let mut spoken = String::new();
    if let Ok(transcript) = s.execute("transcript.inspect", json!({}))
        && let Some(paragraphs) = transcript["paragraphs"].as_array()
    {
        for paragraph in paragraphs {
            let from = paragraph["start"].as_f64().unwrap_or(0.0) / 254_016_000_000.0;
            let to = paragraph["end"].as_f64().unwrap_or(0.0) / 254_016_000_000.0;
            if to > start && from < end {
                spoken.push_str(paragraph["text"].as_str().unwrap_or(""));
                spoken.push(' ');
            }
        }
    }
    let brief = format!(
        "Frame analizzato: {description}\nParlato sotto la clip ({start:.1}s-{end:.1}s): {}\nScrivi il prompt di generazione coerente con questo contenuto, senza inventare elementi assenti.",
        if spoken.is_empty() { "nessuna trascrizione disponibile" } else { spoken.trim() }
    );
    let prompt = match s.execute("nl.prompt.enhance", json!({"brief": brief, "kind": "image", "wait": true})) {
        Ok(enhanced) => enhanced["prompt"].as_str().unwrap_or("").trim().to_string(),
        // A prompt model that echoes workflow internals is rejected by the enhance guard; the
        // frame description and the spoken line are still a usable, grounded prompt.
        Err(_) => format!("{description} Context: {}", if spoken.is_empty() { "no speech transcript available" } else { spoken.trim() }),
    };
    if prompt.is_empty() {
        return Err(bad_cmd("nl.prompt.fromObject", "the model returned an empty prompt"));
    }
    if let Some(entry) = s.project.item(filmcraft_project::ItemId(item_id)).and_then(|item| item.metadata.get("nl.entry")).and_then(|id| id.parse::<u64>().ok())
    {
        let _ = s.execute("nl.agent.update", json!({"id": entry, "prompt": prompt}));
    }
    let _ = s.edit("NL Prompt from Frame", |project, _| {
        if let Some(item) = project.item_mut(filmcraft_project::ItemId(item_id)) {
            item.metadata.insert("nl.prompt".into(), prompt.clone());
            item.metadata.insert("nl.vision".into(), description.clone());
            item.metadata.insert("nl.spoken".into(), spoken.trim().to_string());
        }
        Ok(())
    });
    Ok(json!({"prompt": prompt, "vision": description, "spoken": spoken.trim(), "model": used_model, "asset": path}))
}

/// `nl.prompt.ideogram` — build the structured JSON prompt Ideogram4 expects, from an Italian or
/// English brief. The model must return the schema, never prose.
fn ideogram(s: &mut Session, p: &Value) -> std::result::Result<Value, crate::EngineError> {
    let brief = p.get("brief").and_then(Value::as_str).map(str::trim).unwrap_or("").to_string();
    if brief.is_empty() {
        return Err(bad_cmd("nl.prompt.ideogram", "serve `brief`"));
    }
    let aspect = p.get("aspect").and_then(Value::as_str).unwrap_or("9:16");
    let system = "Sei un art director per Ideogram 4. Rispondi SOLO con JSON valido con questa forma: \
{\"aspect_ratio\":\"9:16\"|\"1:1\"|\"16:9\",\"high_level_description\":string,\
\"compositional_deconstruction\":{\"background\":string,\"elements\":[{\"type\":\"text\",\"bbox\":[x1,y1,x2,y2],\"text\":string,\"desc\":string},{\"type\":\"obj\",\"bbox\":[x1,y1,x2,y2],\"desc\":string}]}}. \
bbox sono interi 0-1000. Massimo 6 elementi. Ogni testo visibile deve essere nel campo text, nella lingua richiesta, senza inventare altre scritte. Non includere markdown, commenti o testo fuori dal JSON.";
    let prompt = format!("Brief: {brief}\nAspect ratio richiesto: {aspect}. Crea una card infografica pulita con testi leggibili.");
    let llm = super::llm::LlmConfig::for_role(s, "prompt", p.get("model").and_then(Value::as_str));
    let value = llm.auto_json(system, &prompt, Duration::from_secs(240), Some("0")).map_err(|e| bad_cmd("nl.prompt.ideogram", e))?;
    if value.get("aspect_ratio").and_then(Value::as_str).is_none()
        || value.pointer("/compositional_deconstruction/elements").and_then(Value::as_array).is_none()
    {
        return Err(bad_cmd("nl.prompt.ideogram", "the model did not return the Ideogram JSON schema"));
    }
    let compact = serde_json::to_string(&value).map_err(|e| bad_cmd("nl.prompt.ideogram", e))?;
    Ok(json!({"prompt": compact, "json": value, "model": llm.model}))
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "nl.prompt.enhance",
            label: "NL Prompt AI",
            menu: &[],
            shortcut: None,
            params: r#"{"brief":str,"kind":"image|video|v2v|music"?,"workflow":str?,"language":str?,"model":str?,"wait":bool?}"#,
            enabled: always,
            run: enhance,
            journal: false,
            named: crate::commands::NamedKeys::NONE,
        },
        CommandSpec {
            id: "nl.prompt.h3",
            label: "NL H3 Prompt",
            menu: &[],
            shortcut: None,
            params: r#"{"brief":str,"mode":"t2va|i2va|fl2va|l2va|ref"?,"dialogue":str?,"seconds":f?,"finalShot":int?,"references":[obj]?,"language":str?,"model":str?,"wait":bool?}"#,
            enabled: always,
            run: h3,
            journal: false,
            named: crate::commands::NamedKeys::NONE,
        },
        CommandSpec {
            id: "nl.prompt.fromObject",
            label: "NL Prompt from Frame and Speech",
            menu: &[],
            shortcut: None,
            params: r#"{"clip":id,"visionModel":str?}"#,
            enabled: always,
            run: from_object,
            journal: false,
            named: crate::commands::NamedKeys::NONE,
        },
        CommandSpec {
            id: "nl.prompt.ideogram",
            label: "NL Ideogram JSON Prompt",
            menu: &[],
            shortcut: None,
            params: r#"{"brief":str,"aspect":str?,"model":str?}"#,
            enabled: always,
            run: ideogram,
            journal: false,
            named: crate::commands::NamedKeys::NONE,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_that_leaks_workflow_internals_is_refused() {
        let bad = json!({"prompt": "use class_type KSampler with \"inputs\""});
        assert!(clean(&bad).is_err());
    }

    #[test]
    fn prompts_and_variants_are_bounded_and_trimmed() {
        let ok = clean(&json!({"prompt": "  a studio, warm light  ", "variants": ["one", "", "two"]})).unwrap();
        assert_eq!(ok["prompt"], "a studio, warm light");
        assert_eq!(ok["variants"], json!(["one", "two"]), "empty variants are dropped");
        let long = "x".repeat(MAX_PROMPT + 1);
        assert!(clean(&json!({"prompt": long})).is_err());
    }

    #[test]
    fn dialogue_must_appear_verbatim_and_with_the_language_tag() {
        let good = "<d>[Italian] Ci siamo!</d>".to_string();
        assert!(check_dialogue(good.clone(), Some("Ci siamo!"), "it").is_ok());
        assert!(check_dialogue(good, Some("Non ci siamo"), "it").is_err(), "words must be there");
        assert!(check_dialogue("senza tag".into(), Some("senza tag"), "it").is_err(), "the tag is required");
        assert!(check_dialogue("<d>[English] senza tag</d>".into(), Some("senza tag"), "it").is_err(), "wrong language tag");
        assert_eq!(language_label("it"), "Italian");
        assert_eq!(language_label("es"), "Spanish");
    }

    #[test]
    fn the_dialogue_tag_is_repaired_when_the_model_writes_the_wrong_language() {
        let fixed = fix_dialogue_tag("<d>[English] Ci siamo!</d>".into(), Some("Ci siamo!"), "it");
        assert!(fixed.contains("<d>[Italian] Ci siamo!</d>"), "{fixed}");
        let fixed = fix_dialogue_tag("<d>English Ci siamo!</d>".into(), Some("Ci siamo!"), "it");
        assert!(fixed.contains("<d>[Italian]Ci siamo!</d>"), "{fixed}");
        let fixed = fix_dialogue_tag("<d>Ci siamo!</d>".into(), Some("Ci siamo!"), "it");
        assert!(fixed.contains("<d>[Italian]Ci siamo!</d>"), "{fixed}");
        let untouched = fix_dialogue_tag("no dialogue here".into(), None, "it");
        assert_eq!(untouched, "no dialogue here");
    }

    #[test]
    fn h3_base_sections_must_be_in_order_and_start_with_shot_1() {
        let good = "integrated_multimodal_description: [Shot 1] A studio, warm light.\n\noverall_soundscape: Room tone.\n\nnon_diegetic_music: N/A";
        assert!(clean_h3("t2va", good).is_ok());
        let wrong_order = "overall_soundscape: room tone\n\nintegrated_multimodal_description: [Shot 1] x\n\nnon_diegetic_music: N/A";
        assert!(clean_h3("t2va", wrong_order).is_err());
        let no_shot = "integrated_multimodal_description: A studio\n\noverall_soundscape: room tone\n\nnon_diegetic_music: N/A";
        assert!(clean_h3("t2va", no_shot).is_err());
    }

    #[test]
    fn h3_full_reference_needs_definitions_with_retention() {
        let good = "subject_definitions:\n<Subject 1> is the man from <Picture 1>.\n\nsummary:\n[video editing] The target video edits <Video 1>.\n\nretention_analysis:\n<Subject 1>: fully_preserved — identity kept.\n\n\noverall_soundscape: Room tone.\n\nnon_diegetic_music: N/A";
        // detailed_description missing on purpose: sections must all be present in order
        assert!(clean_h3("ref", good).is_err());
        let good = "subject_definitions:\n<Subject 1> is the man from <Picture 1>.\n\nsummary:\n[video editing] The target video edits <Video 1>.\n\nretention_analysis:\n<Subject 1>: fully_preserved — identity kept.\n\ndetailed_description:\nStyle first.\n[Shot 1] He walks.\n\noverall_soundscape: Room tone.\n\nnon_diegetic_music: N/A";
        assert!(clean_h3("ref", good).is_ok(), "{:?}", clean_h3("ref", good));
        let without_retention = good.replace("<Subject 1>: fully_preserved — identity kept.", "nothing here");
        assert!(clean_h3("ref", &without_retention).is_err());
    }

    #[test]
    fn h3_keyframe_alignment_lines_are_exact() {
        let i2va = h3_instruction("i2va", None, None).unwrap();
        assert!(i2va.starts_with("I2VA: For the target video, at 0.00 seconds"), "{i2va}");
        let fl2va = h3_instruction("fl2va", Some(5.5), Some(3)).unwrap();
        assert!(fl2va.contains("Picture 2 (from Shot 3) aligns with the 5.50-second mark"), "{fl2va}");
        let l2va = h3_instruction("l2va", Some(4.0), Some(2)).unwrap();
        assert!(l2va.contains("<Picture 1> (from Shot 2) aligns with the 4.00-second mark"), "{l2va}");
        assert!(h3_instruction("t2va", None, None).is_none());
    }

    #[test]
    fn h3_command_checks_its_parameters() {
        let mut s = crate::Session::default();
        assert!(s.execute("nl.prompt.h3", json!({})).is_err(), "brief required");
        assert!(s.execute("nl.prompt.h3", json!({"brief": "x", "mode": "nope"})).is_err());
        assert!(s.execute("nl.prompt.h3", json!({"brief": "x", "mode": "fl2va"})).is_err(), "fl2va needs seconds");
        assert!(s.execute("nl.prompt.h3", json!({"brief": "x", "mode": "l2va"})).is_err(), "l2va needs seconds");
    }

    #[test]
    fn resolution_and_aspect_wording_is_stripped() {
        let answer = clean(&json!({"prompt": "a studio, 8k resolution, cinematic, 16:9 aspect ratio"})).unwrap();
        assert!(!answer["prompt"].as_str().unwrap().to_ascii_lowercase().contains("8k"), "{answer}");
        assert!(!answer["prompt"].as_str().unwrap().contains("16:9"), "{answer}");
        assert!(answer["prompt"].as_str().unwrap().starts_with("a studio"), "{answer}");
    }

    #[test]
    fn the_brief_is_required() {
        let mut s = crate::Session::default();
        assert!(s.execute("nl.prompt.enhance", json!({})).is_err());
        assert!(s.execute("nl.prompt.enhance", json!({"brief": "  "})).is_err());
        assert!(s.execute("nl.prompt.enhance", json!({"brief": "x", "kind": "nope"})).is_err());
    }
}
