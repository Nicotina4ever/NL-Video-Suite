//! NL timeline agent: a sketched timeline of generation objects.
//!
//! The operator gives a subject (or a list of objects), the agent writes one visual prompt per
//! object, and each object is placed on the timeline as a **placeholder** card. The prompts are
//! editable; ComfyUI generates the media, the generated file is moved into the project folder and
//! the placeholder clip is replaced in place. Nothing is generated silently: the plan is a JSON
//! document shown in the NL Agent panel and saved beside the project as `nl/timeline-plan.json`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use filmcraft_time::Tick;
use image::{ColorType, Rgba, RgbaImage};
use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{EngineError, Result, Session};

const MAX_ENTRIES: usize = 24;
const MAX_TITLE: usize = 160;
const MAX_PROMPT: usize = 1500;
const DEFAULT_SECONDS: f64 = 4.0;
const MAX_SECONDS: f64 = 30.0;

fn bad_cmd(cmd: &str, msg: impl ToString) -> EngineError {
    bad(cmd, msg.to_string())
}

fn text(value: Option<&Value>, max: usize) -> String {
    value.and_then(Value::as_str).unwrap_or_default().trim().chars().take(max).collect()
}

fn opt_id(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64)
}

/// The empty plan the panel starts from.
pub fn empty_plan() -> Value {
    json!({"subject": "", "style": "", "source": "empty", "entries": []})
}

/// The plan of this session/project (loaded from the sidecar on first use).
pub fn load(s: &mut Session) -> Value {
    if let Some(plan) = &s.nl.agent_plan {
        return plan.clone();
    }
    let plan = plan_path(s)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .map(|value| sanitize_plan(&value))
        .unwrap_or_else(empty_plan);
    s.nl.agent_plan = Some(plan.clone());
    plan
}

fn plan_path(s: &Session) -> Option<PathBuf> {
    let project = s.path.as_deref()?;
    let dir = Path::new(project).parent()?.to_path_buf();
    Some(dir.join("nl").join("timeline-plan.json"))
}

fn save(s: &mut Session, plan: &Value) -> Result<()> {
    s.nl.agent_plan = Some(plan.clone());
    let Some(path) = plan_path(s) else { return Ok(()) };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| bad_cmd("nl.agent.plan", format!("{}: {e}", parent.display())))?;
    }
    let text = serde_json::to_vec_pretty(plan).map_err(|e| bad_cmd("nl.agent.plan", e.to_string()))?;
    std::fs::write(&path, text).map_err(|e| bad_cmd("nl.agent.plan", format!("{}: {e}", path.display())))
}

/// The project folder: beside the saved `.fcproj`.
fn project_dir(s: &Session) -> Result<PathBuf> {
    let project = s.path.as_deref().ok_or_else(|| bad_cmd("nl.agent.sketch", "save the project first: generated assets live in the project folder"))?;
    Path::new(project).parent().map(Path::to_path_buf).ok_or_else(|| bad_cmd("nl.agent.sketch", "the project has no folder"))
}

/// Keep only sane values from a model/user plan and number the objects.
pub fn sanitize_plan(value: &Value) -> Value {
    let object = value.as_object();
    let subject = text(object.and_then(|o| o.get("subject")), MAX_TITLE);
    let style = text(object.and_then(|o| o.get("style")), MAX_TITLE);
    let mut entries = Vec::new();
    for (index, entry) in object.and_then(|o| o.get("entries")).and_then(Value::as_array).into_iter().flatten().enumerate().take(MAX_ENTRIES) {
        let Some(entry) = entry.as_object() else { continue };
        let get = |key: &str| entry.get(key);
        let title = {
            let title = text(get("title"), MAX_TITLE);
            if title.is_empty() { format!("Oggetto {}", index + 1) } else { title }
        };
        let prompt = text(get("prompt"), MAX_PROMPT);
        let seconds = get("seconds").and_then(Value::as_f64).filter(|s| s.is_finite()).unwrap_or(DEFAULT_SECONDS).clamp(1.0, MAX_SECONDS);
        let kind = match get("kind").and_then(Value::as_str).unwrap_or("image") {
            "video" => "video",
            _ => "image",
        };
        let status = match get("status").and_then(Value::as_str).unwrap_or("draft") {
            "ready" => "ready",
            "generated" => "generated",
            "failed" => "failed",
            _ => "draft",
        };
        let mut row = json!({
            "id": index as u64 + 1,
            "title": title,
            "prompt": prompt,
            "seconds": seconds,
            "kind": kind,
            "status": status,
            "source": text(get("source"), 40),
        });
        if let Some(id) = opt_id(get("placeholderItem")) {
            row["placeholderItem"] = json!(id);
        }
        if let Some(id) = opt_id(get("clip")) {
            row["clip"] = json!(id);
        }
        if let Some(id) = opt_id(get("generatedItem")) {
            row["generatedItem"] = json!(id);
        }
        if let Some(asset) = get("asset").and_then(Value::as_str).filter(|a| !a.trim().is_empty()) {
            row["asset"] = json!(asset);
        }
        entries.push(row);
    }
    json!({
        "subject": subject,
        "style": style,
        "source": text(object.and_then(|o| o.get("source")), 40),
        "entries": entries,
    })
}

/// The last plan, for the panel, the CLI and agents.
fn plan_get(s: &mut Session, _: &Value) -> Result<Value> {
    let plan = load(s);
    Ok(json!({
        "plan": plan,
        "path": plan_path(s).map(|p| p.to_string_lossy().to_string()),
    }))
}

/// Ask the local model for a list of objects with visual prompts. A model failure is reported in
/// `llmError` and the subject still becomes one editable draft, so the panel is never empty.
fn model_entries(s: &Session, subject: &str, style: &str, count: usize) -> (Vec<Value>, Option<String>) {
    let system = "Sei un regista e direttore della fotografia. Trasforma un soggetto in una lista \
        di oggetti visivi per una timeline. Rispondi SOLO con JSON valido: \
        {\"title\":\"...\",\"style\":\"...\",\"entries\":[{\"title\":\"...\",\"prompt\":\"...\",\"seconds\":4,\"kind\":\"image\"}]}. \
        Ogni prompt deve descrivere inquadratura, soggetto, luce, ambiente e stile, in modo autonomo. \
        Usa al massimo il numero di oggetti richiesto, durate 2-8 secondi.";
    let prompt = format!(
        "Soggetto: {subject}\nStile: {style}\nNumero di oggetti: {count}\nScrivi i prompt in italiano, \
         pronti per un generatore di immagini."
    );
    let config = super::llm::LlmConfig::for_session(s, None);
    match config.auto_json(system, &prompt, Duration::from_secs(120), Some("0")) {
        Ok(value) => {
            let entries = value.get("entries").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().take(count.max(1)).collect();
            (entries, None)
        }
        Err(error) => {
            let title = subject.chars().take(80).collect::<String>();
            (
                vec![json!({
                    "title": if title.is_empty() { "Oggetto".to_string() } else { title },
                    "prompt": subject,
                    "seconds": DEFAULT_SECONDS,
                    "kind": "image",
                })],
                Some(error),
            )
        }
    }
}

/// `nl.agent.sketch` — subject → plan → placeholder cards on the timeline.
fn sketch(s: &mut Session, p: &Value) -> Result<Value> {
    let subject = text(p.get("subject"), MAX_TITLE);
    let style = text(p.get("style"), MAX_TITLE);
    let dry_run = p.get("dryRun").and_then(Value::as_bool).unwrap_or(false);
    if subject.is_empty() && p.get("entries").and_then(Value::as_array).is_none() {
        return Err(bad_cmd("nl.agent.sketch", "serve `subject` o `entries`"));
    }
    let count = p.get("scenes").and_then(Value::as_u64).unwrap_or(6).clamp(1, MAX_ENTRIES as u64) as usize;
    let (raw_entries, llm_error) = match p.get("entries").and_then(Value::as_array) {
        Some(entries) => (entries.clone(), None),
        None => model_entries(s, &subject, &style, count),
    };
    let mut plan = sanitize_plan(&json!({
        "subject": subject,
        "style": style,
        "source": if p.get("entries").is_some() { "given" } else if llm_error.is_some() { "subject-fallback" } else { "model" },
        "entries": raw_entries,
    }));
    if let Some(error) = llm_error {
        plan["llmError"] = json!(error);
    }
    if dry_run {
        return Ok(json!({"plan": plan, "dryRun": true, "placed": 0}));
    }
    let start = p.get("startSeconds").and_then(Value::as_f64).map(Tick::from_seconds_f64).unwrap_or_else(|| s.playhead());
    let placed = place_plan(s, &mut plan, start)?;
    save(s, &plan)?;
    s.nl.results.insert("agent.plan", plan.clone());
    Ok(json!({"plan": plan, "placed": placed, "path": plan_path(s).map(|p| p.to_string_lossy().to_string())}))
}

/// `nl.agent.add` — one placeholder at the playhead, with no prompt yet.
fn add(s: &mut Session, p: &Value) -> Result<Value> {
    let mut plan = load(s);
    let subject = text(p.get("prompt"), MAX_TITLE);
    let title = {
        let title = text(p.get("title"), MAX_TITLE);
        if title.is_empty() { if subject.is_empty() { "Nuovo oggetto".to_string() } else { subject.chars().take(60).collect() } } else { title }
    };
    let entry = json!({
        "title": title,
        "prompt": subject,
        "seconds": p.get("seconds").and_then(Value::as_f64).unwrap_or(DEFAULT_SECONDS),
        "kind": p.get("kind").and_then(Value::as_str).unwrap_or("image"),
        "source": "operator",
    });
    plan["entries"].as_array_mut().ok_or_else(|| bad_cmd("nl.agent.add", "plan has no entries"))?.push(entry);
    plan = sanitize_plan(&plan);
    let start = p.get("startSeconds").and_then(Value::as_f64).map(Tick::from_seconds_f64).unwrap_or_else(|| s.playhead());
    let placed = place_plan(s, &mut plan, start)?;
    save(s, &plan)?;
    s.nl.results.insert("agent.plan", plan.clone());
    Ok(json!({"plan": plan, "placed": placed}))
}

/// Ensure the placeholder track exists and place every object that has no clip yet.
fn place_plan(s: &mut Session, plan: &mut Value, start: Tick) -> Result<usize> {
    let folder = project_dir(s)?.join("nl").join("placeholders");
    std::fs::create_dir_all(&folder).map_err(|e| bad_cmd("nl.agent.sketch", format!("{}: {e}", folder.display())))?;
    let track = ensure_track(s)?;
    let mut cursor = start;
    let mut placed = 0usize;
    let entries = plan["entries"].as_array().cloned().unwrap_or_default();
    for (index, entry) in entries.iter().enumerate() {
        if entry.get("clip").and_then(Value::as_u64).is_some() {
            continue;
        }
        let title = entry["title"].as_str().unwrap_or("Oggetto");
        let seconds = entry["seconds"].as_f64().unwrap_or(DEFAULT_SECONDS).clamp(1.0, MAX_SECONDS);
        let png = folder.join(format!("placeholder-{:02}.png", index + 1));
        write_placeholder(&png, title, index)?;
        let imported = s.execute("file.import", json!({"paths": [png.to_string_lossy()]}))?;
        let item = imported["items"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(Value::as_u64)
            .ok_or_else(|| bad_cmd("nl.agent.sketch", "the placeholder image was not imported"))?;
        let name = format!("NL · {title}");
        let _ = s.execute("clip.rename", json!({"item": item, "name": name}));
        let placed_result = s.execute(
            "timeline.place",
            json!({
                "item": item,
                "track": track,
                "time": cursor.0,
                "duration": Tick::from_seconds_f64(seconds).0,
            }),
        )?;
        let clip = placed_result["clips"]
            .as_array()
            .and_then(|clips| clips.first())
            .and_then(Value::as_u64)
            .ok_or_else(|| bad_cmd("nl.agent.sketch", "the placeholder was not placed on the timeline"))?;
        let _ = s.execute("clip.rename", json!({"clips": [clip], "name": name}));
        let _ = s.execute("clip.fitToFrame", json!({"clips": [clip]}));
        plan["entries"][index]["placeholderItem"] = json!(item);
        plan["entries"][index]["clip"] = json!(clip);
        cursor = Tick(cursor.0 + Tick::from_seconds_f64(seconds).0);
        placed += 1;
    }
    Ok(placed)
}

fn ensure_track(s: &mut Session) -> Result<String> {
    if let Some(index) = s.active_sequence().and_then(|sequence| sequence.video_tracks.iter().rposition(|track| track.name == "NL")) {
        return Ok(format!("V{}", index + 1));
    }
    s.execute("sequence.addTracks", json!({"video": 1}))?;
    let count = s
        .active_sequence()
        .map(|sequence| sequence.video_tracks.len())
        .ok_or_else(|| bad_cmd("nl.agent.sketch", "serve una sequenza con una traccia video"))?;
    let name = format!("V{count}");
    // Name the new track so the next sketch finds it again; the name is metadata, the id is what
    // `timeline.place` resolves.
    let _ = s.execute("timeline.setTrack", json!({"track": name, "name": "NL"}));
    Ok(name)
}

/// A readable placeholder card: dark field, violet border and a few stripes, no font needed.
fn write_placeholder(path: &Path, title: &str, index: usize) -> Result<()> {
    let (w, h) = (540u32, 960u32);
    let mut image = RgbaImage::from_pixel(w, h, Rgba([16, 10, 28, 255]));
    let border = Rgba([117, 49, 255, 255]);
    for x in 0..w {
        for y in 0..h {
            if x < 4 || y < 4 || x >= w - 4 || y >= h - 4 {
                image.put_pixel(x, y, border);
            }
        }
    }
    let accent = Rgba([229, 70, 156, 255]);
    let band = 8 + (index as u32 % 5) * 3;
    for y in (120..h - 120).step_by(48) {
        for x in 60..w - 60 {
            image.put_pixel(x, y, if (x / 24 + y / 48) % 2 == 0 { accent } else { border });
        }
    }
    for y in (h / 2 - band)..(h / 2 + band) {
        for x in 80..w - 80 {
            image.put_pixel(x, y, accent);
        }
    }
    // A tiny title mark: the first letter in a block, so two cards differ even without a font.
    let initial = title.chars().next().unwrap_or('N').to_ascii_uppercase();
    let glyph = initial as u32 % 8;
    for y in 60..140 {
        for x in 60..140 {
            if (x / 10 + y / 10 + glyph).is_multiple_of(2) {
                image.put_pixel(x, y, accent);
            }
        }
    }
    image::save_buffer(path, image.as_raw(), w, h, ColorType::Rgba8).map_err(|e| bad_cmd("nl.agent.sketch", format!("{}: {e}", path.display())))
}

/// `nl.agent.prompts` — fill the empty prompts from the subject and the titles.
fn prompts(s: &mut Session, p: &Value) -> Result<Value> {
    let mut plan = load(s);
    let force = p.get("force").and_then(Value::as_bool).unwrap_or(false);
    let entries = plan["entries"].as_array().cloned().unwrap_or_default();
    if entries.is_empty() {
        return Err(bad_cmd("nl.agent.prompts", "the plan is empty: create the sketch first"));
    }
    let wanted: Vec<Value> = entries
        .iter()
        .filter(|entry| force || entry["prompt"].as_str().unwrap_or("").trim().is_empty())
        .map(|entry| json!({"id": entry["id"], "title": entry["title"], "seconds": entry["seconds"]}))
        .collect();
    if wanted.is_empty() {
        return Ok(json!({"updated": 0, "plan": plan}));
    }
    let subject = plan["subject"].as_str().unwrap_or("").to_string();
    let style = plan["style"].as_str().unwrap_or("").to_string();
    let system = "Sei un regista e direttore della fotografia. Scrivi un prompt visivo per ogni \
        oggetto, in italiano, con inquadratura, soggetto, luce, ambiente e stile. Rispondi SOLO con \
        JSON valido: {\"prompts\":[{\"id\":1,\"prompt\":\"...\"}]}.";
    let prompt = format!("Soggetto: {subject}\nStile: {style}\nOggetti: {}", Value::Array(wanted.clone()));
    let config = super::llm::LlmConfig::for_session(s, None);
    let answer = config.auto_json(system, &prompt, Duration::from_secs(120), Some("0"));
    let (answered, error) = match answer {
        Ok(value) => (value, None),
        Err(error) => (json!({"prompts": []}), Some(error)),
    };
    let mut updated = 0;
    for entry in plan["entries"].as_array_mut().into_iter().flatten() {
        let id = entry["id"].as_u64().unwrap_or(0);
        let current = entry["prompt"].as_str().unwrap_or("").trim().to_string();
        if !force && !current.is_empty() {
            continue;
        }
        let generated = answered
            .get("prompts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|row| row["id"].as_u64() == Some(id))
            .and_then(|row| row["prompt"].as_str())
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string);
        let fallback = format!("{}. {}. {}{}", subject, entry["title"].as_str().unwrap_or("Oggetto"), style, if style.is_empty() { "" } else { "." });
        entry["prompt"] = json!(generated.unwrap_or(fallback));
        entry["status"] = json!(if entry["status"] == "generated" { "generated" } else { "ready" });
        entry["source"] = json!(if error.is_some() { "title-fallback" } else { "model" });
        updated += 1;
    }
    save(s, &plan)?;
    let mut result = json!({"updated": updated, "plan": plan});
    if let Some(error) = error {
        result["llmError"] = json!(error);
    }
    Ok(result)
}

/// `nl.agent.generate` — ComfyUI per object, asset moved into the project, clip replaced in place.
fn generate(s: &mut Session, p: &Value) -> Result<Value> {
    let mut plan = load(s);
    let dry_run = p.get("dryRun").and_then(Value::as_bool).unwrap_or(false);
    let only = p.get("id").and_then(Value::as_u64);
    let all = p.get("all").and_then(Value::as_bool).unwrap_or(true);
    let workflow = p.get("workflow").and_then(Value::as_str).map(str::to_string);
    let generated_dir = project_dir(s)?.join("generated");
    let entries = plan["entries"].as_array().cloned().unwrap_or_default();
    let selected: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            let status = entry["status"].as_str().unwrap_or("draft");
            status != "generated" && (only.is_none() || entry["id"].as_u64() == only) && (all || only.is_some())
        })
        .map(|(index, _)| index)
        .collect();
    if selected.is_empty() {
        return Ok(json!({"generated": 0, "failed": [], "plan": plan, "note": "no object to generate"}));
    }
    if dry_run {
        let list: Vec<Value> = selected.iter().map(|index| entries[*index].clone()).collect();
        return Ok(json!({"dryRun": true, "wouldGenerate": list, "workflow": workflow, "plan": plan}));
    }
    std::fs::create_dir_all(&generated_dir).map_err(|e| bad_cmd("nl.agent.generate", format!("{}: {e}", generated_dir.display())))?;
    let mut generated = 0;
    let mut failed = Vec::new();
    for index in selected {
        let entry = plan["entries"][index].clone();
        let id = entry["id"].as_u64().unwrap_or(0);
        let prompt = entry["prompt"].as_str().unwrap_or("").trim().to_string();
        if prompt.is_empty() {
            failed.push(json!({"id": id, "error": "empty prompt: fill it with AI or write it"}));
            plan["entries"][index]["status"] = json!("failed");
            continue;
        }
        let mut params = json!({"prompt": prompt, "wait": true});
        if let Some(workflow) = &workflow {
            params["workflow"] = json!(workflow);
        }
        if let Some(seconds) = entry["seconds"].as_f64() {
            params["seconds"] = json!(seconds.round() as u64);
        }
        if entry["kind"] == "video" {
            params["kind"] = json!("video");
        }
        match s.execute("graphics.generateComfy", params) {
            Ok(_) => {}
            Err(error) => {
                failed.push(json!({"id": id, "error": error.to_string()}));
                plan["entries"][index]["status"] = json!("failed");
                continue;
            }
        }
        let Some(last) = s.nl.results.get("comfy.last").cloned() else {
            failed.push(json!({"id": id, "error": "ComfyUI did not import a media file (job failed or returned no output)"}));
            plan["entries"][index]["status"] = json!("failed");
            continue;
        };
        let Some(item) = last["first"]["item"].as_u64() else {
            failed.push(json!({"id": id, "error": "the generated item has no project id"}));
            plan["entries"][index]["status"] = json!("failed");
            continue;
        };
        let Some(source) = last["first"]["path"].as_str().map(PathBuf::from) else {
            failed.push(json!({"id": id, "error": "the generated file has no path"}));
            plan["entries"][index]["status"] = json!("failed");
            continue;
        };
        let asset = match move_to_project(&source, &generated_dir, id) {
            Ok(path) => path,
            Err(error) => {
                failed.push(json!({"id": id, "error": error.to_string()}));
                plan["entries"][index]["status"] = json!("failed");
                continue;
            }
        };
        if let Err(error) = s.execute("media.relink", json!({"item": item, "path": asset.to_string_lossy(), "force": true})) {
            failed.push(json!({"id": id, "error": format!("asset moved but not relinked: {error}")}));
        }
        if let Some(clip) = entry["clip"].as_u64()
            && let Err(error) = s.execute("clip.replaceFromBin", json!({"clips": [clip], "item": item, "keepSourceIn": false}))
        {
            failed.push(json!({"id": id, "error": format!("generated, but the placeholder was not replaced: {error}")}));
            plan["entries"][index]["status"] = json!("failed");
            continue;
        }
        if let Some(clip) = entry["clip"].as_u64() {
            let title = entry["title"].as_str().unwrap_or("NL");
            let _ = s.execute("clip.rename", json!({"clips": [clip], "name": format!("NL · {title}")}));
        }
        plan["entries"][index]["status"] = json!("generated");
        plan["entries"][index]["generatedItem"] = json!(item);
        plan["entries"][index]["asset"] = json!(asset.to_string_lossy());
        generated += 1;
    }
    save(s, &plan)?;
    s.nl.results.insert("agent.plan", plan.clone());
    Ok(json!({"generated": generated, "failed": failed, "plan": plan}))
}

fn move_to_project(source: &Path, generated_dir: &Path, id: u64) -> Result<PathBuf> {
    let name = source.file_name().and_then(|n| n.to_str()).unwrap_or("generated.bin");
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect();
    let dest = generated_dir.join(format!("nl-{id:02}-{safe}"));
    if source == dest {
        return Ok(dest);
    }
    if dest.exists() {
        std::fs::remove_file(&dest).map_err(|e| bad_cmd("nl.agent.generate", format!("{}: {e}", dest.display())))?;
    }
    match std::fs::rename(source, &dest) {
        Ok(()) => Ok(dest),
        Err(_) => {
            std::fs::copy(source, &dest).map_err(|e| bad_cmd("nl.agent.generate", format!("{}: {e}", dest.display())))?;
            std::fs::remove_file(source).map_err(|e| bad_cmd("nl.agent.generate", format!("{}: {e}", source.display())))?;
            Ok(dest)
        }
    }
}

/// `nl.agent.collect` — move every generated asset into the project folder: the plan's generated
/// media and any project media still pointing at ComfyUI's data folder (`generated/comfy-*`).
fn collect(s: &mut Session, p: &Value) -> Result<Value> {
    let mut plan = load(s);
    let dry_run = p.get("dryRun").and_then(Value::as_bool).unwrap_or(false);
    let folder = project_dir(s)?.join("generated");
    if !dry_run {
        std::fs::create_dir_all(&folder).map_err(|e| bad_cmd("nl.agent.collect", format!("{}: {e}", folder.display())))?;
    }
    let mut moved = 0;
    let entries = plan["entries"].as_array().cloned().unwrap_or_default();
    for (index, entry) in entries.iter().enumerate() {
        let (Some(item), Some(asset)) = (entry["generatedItem"].as_u64(), entry["asset"].as_str()) else { continue };
        let source = PathBuf::from(asset);
        let inside = source.parent().is_some_and(|parent| parent == folder);
        if inside || !source.is_file() {
            continue;
        }
        if dry_run {
            moved += 1;
            continue;
        }
        let dest = move_to_project(&source, &folder, entry["id"].as_u64().unwrap_or((index + 1) as u64))?;
        if let Err(error) = s.execute("media.relink", json!({"item": item, "path": dest.to_string_lossy(), "force": true})) {
            return Err(bad_cmd("nl.agent.collect", format!("asset moved but not relinked: {error}")));
        }
        plan["entries"][index]["asset"] = json!(dest.to_string_lossy());
        moved += 1;
    }
    // Older generations (NL Generate, Cover, Music, Queue) are imported from the data folder;
    // collect them too, so “all generated assets” really means all of them.
    let items: Vec<u64> = s.project.items.keys().map(|id| id.0).collect();
    for id in items {
        let Some(project_item) = s.project.item(filmcraft_project::ItemId(id)) else { continue };
        let filmcraft_project::ItemKind::Media(clip) = &project_item.kind else { continue };
        let filmcraft_project::MediaRef::File { path } = &clip.media else { continue };
        let source = PathBuf::from(path);
        let text = path.replace('\\', "/");
        if !text.contains("generated/comfy-") || source.parent().is_some_and(|parent| parent == folder) {
            continue;
        }
        if dry_run {
            moved += 1;
            continue;
        }
        let dest = move_to_project(&source, &folder, id)?;
        s.execute("media.relink", json!({"item": id, "path": dest.to_string_lossy(), "force": true}))?;
        moved += 1;
    }
    if !dry_run {
        save(s, &plan)?;
    }
    Ok(json!({"moved": moved, "dryRun": dry_run, "plan": plan}))
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    let spec = |id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>| CommandSpec {
        id,
        label,
        menu: &[],
        shortcut: None,
        params,
        enabled: always,
        run,
        journal: false,
        named: crate::commands::NamedKeys::NONE,
    };
    vec![
        spec("nl.agent.plan", "NL Timeline Agent Plan", "{}", plan_get),
        spec(
            "nl.agent.sketch",
            "NL Sketch Timeline from Subject",
            r#"{"subject":str?,"style":str?,"scenes":int?,"entries":[obj]?,"startSeconds":f?,"dryRun":bool?}"#,
            sketch,
        ),
        spec("nl.agent.add", "NL Add Generation Object", r#"{"title":str?,"prompt":str?,"seconds":f?,"kind":"image|video"?,"startSeconds":f?}"#, add),
        spec("nl.agent.prompts", "NL Fill Object Prompts", r#"{"force":bool?}"#, prompts),
        spec("nl.agent.generate", "NL Generate Timeline Objects", r#"{"all":bool?,"id":int?,"workflow":str?,"dryRun":bool?}"#, generate),
        spec("nl.agent.collect", "NL Collect Project Assets", r#"{"dryRun":bool?}"#, collect),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_project(name: &str) -> (Session, std::path::PathBuf) {
        let dir = crate::media_test_util::tmp_dir(name);
        let mut s = Session::default();
        s.execute("file.openDemoProject", serde_json::json!({})).unwrap();
        s.path = Some(dir.join("agent.fcproj").to_string_lossy().to_string());
        (s, dir)
    }

    #[test]
    fn a_sketch_places_placeholder_cards_and_saves_the_plan() {
        let (mut s, dir) = session_with_project("nl-agent-sketch");
        let result = s
            .execute(
                "nl.agent.sketch",
                serde_json::json!({
                    "subject": "un viaggio notturno",
                    "entries": [
                        {"title": "Apertura", "prompt": "strada bagnata, neon", "seconds": 2},
                        {"title": "Volto", "prompt": "primo piano, luce viola", "seconds": 3}
                    ]
                }),
            )
            .unwrap();
        let plan = &result["plan"];
        assert_eq!(plan["entries"].as_array().unwrap().len(), 2);
        assert!(plan["entries"][0]["clip"].as_u64().is_some());
        let sequence = s.active_sequence().unwrap();
        let track = sequence.video_tracks.iter().find(|track| track.name == "NL").expect("the NL placeholder track");
        assert_eq!(track.items.len(), 2, "both placeholders are on the timeline");
        assert!(dir.join("nl").join("timeline-plan.json").is_file());
        assert!(dir.join("nl").join("placeholders").read_dir().unwrap().count() >= 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_dry_run_writes_only_the_plan() {
        let (mut s, dir) = session_with_project("nl-agent-dry");
        let result = s.execute("nl.agent.sketch", serde_json::json!({"subject": "solo", "entries": [{"title": "A", "prompt": "p"}], "dryRun": true})).unwrap();
        assert_eq!(result["placed"], 0);
        assert!(s.active_sequence().unwrap().video_tracks.iter().all(|track| track.name != "NL"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_plan_sanitises_titles_prompts_and_seconds() {
        let plan = sanitize_plan(&serde_json::json!({
            "subject": "x",
            "entries": [
                {"title": "", "prompt": "p", "seconds": 999.0, "kind": "video"},
                {"title": "B", "prompt": "", "seconds": -2.0, "kind": "other"}
            ]
        }));
        let entries = plan["entries"].as_array().unwrap();
        assert_eq!(entries[0]["title"], "Oggetto 1");
        assert_eq!(entries[0]["kind"], "video");
        assert_eq!(entries[0]["seconds"], 30.0);
        assert_eq!(entries[1]["kind"], "image");
        assert_eq!(entries[1]["seconds"], 1.0);
    }
}

#[test]
fn an_asset_is_moved_into_the_project_folder() {
    let dir = crate::media_test_util::tmp_dir("nl-agent-asset");
    let source = dir.join("comfy_00001_.png");
    std::fs::write(&source, b"png").unwrap();
    let generated = dir.join("generated");
    std::fs::create_dir_all(&generated).unwrap();
    let moved = move_to_project(&source, &generated, 7).unwrap();
    assert_eq!(moved.parent(), Some(generated.as_path()));
    assert!(moved.file_name().unwrap().to_string_lossy().starts_with("nl-07-"));
    assert!(!source.exists(), "the original download is moved, not copied");
    let _ = std::fs::remove_dir_all(&dir);
}
