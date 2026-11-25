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

use filmcraft_project::{BinEntry, ItemId, ItemKind, MediaRef};

const MAX_ENTRIES: usize = 24;
const MAX_TITLE: usize = 160;
const MAX_TEXT: usize = 4000;
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

/// The four quality tiers the operator picks. They select a workflow whose own quality estimate is
/// closest, preferring text-to-image/text-to-video workflows over special-purpose ones.
const QUALITY_TIERS: [&str; 4] = ["bozza", "standard", "alta", "massima"];

fn quality_index(quality: &str) -> usize {
    let q = quality.trim().to_ascii_lowercase();
    QUALITY_TIERS.iter().position(|tier| *tier == q).unwrap_or(1)
}

/// A workflow's own quality estimate from its name: turbo/lightning is a draft, plain z-image/krea2
/// is standard, flux2/ideogram/qwen/klein are high, and the rest stay standard.
fn workflow_quality(name: &str) -> usize {
    let n = name.to_ascii_lowercase();
    if n.contains("turbo") || n.contains("lightning") || n.contains("_fast") || n.contains("1bg") {
        return 0;
    }
    if n.contains("flux2")
        || n.contains("flux.2")
        || n.contains("ideogram")
        || n.contains("qwen")
        || n.contains("klein")
        || n.contains("refiner")
        || n.contains("hd")
    {
        return 3;
    }
    if n.contains("krea2") || n.contains("z-image") || n.contains("wan2.2") {
        return 2;
    }
    1
}

/// Special-purpose workflows that need references, an edit input or a previous clip: useful, but
/// not the default for a text-only story/series placeholder.
fn is_special_workflow(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ["edit", "sheet", "extend", "v2v", "r2v", "reference", "character", "analisi", "old", "music", "audio"].iter().any(|word| n.contains(word))
}

fn workflow_score(name: &str, kind: &str, target: usize) -> i32 {
    let quality = workflow_quality(name);
    let mut score = 100 - (quality as i32 - target as i32).abs() * 25;
    let n = name.to_ascii_lowercase();
    if n.contains("text_to_image") || n.contains("t2i") || n.contains("text_to_video") || n.contains("t2v") || n.contains("t2i") {
        score += 15;
    }
    if kind == "image" && n.contains("image") {
        score += 5;
    }
    if kind == "video" && n.contains("video") {
        score += 5;
    }
    if is_special_workflow(name) {
        score -= 70;
    }
    // At high/max quality a turbo workflow is a worse match than a plain one.
    if quality == 0 && target >= 2 {
        score -= 15;
    }
    score
}

/// Pick the workflow for one kind and quality from the suite's own library.
pub(crate) fn choose_workflow(s: &Session, kind: &str, quality: &str) -> Option<Value> {
    let target = quality_index(quality);
    let list = super::models::workflows(s, kind).ok()?;
    let mut best: Option<(i32, Value)> = None;
    for workflow in list {
        let name = workflow["name"].as_str().unwrap_or("");
        let path = workflow["path"].as_str().unwrap_or("");
        if name.is_empty() || path.is_empty() || workflow.get("error").is_some() {
            continue;
        }
        let score = workflow_score(name, kind, target);
        let candidate = json!({
            "path": path,
            "name": name,
            "kind": kind,
            "quality": QUALITY_TIERS[workflow_quality(name)],
            "score": score,
            "reason": format!("{name}: qualità {}, {} per {}", QUALITY_TIERS[workflow_quality(name)], if is_special_workflow(name) { "specialistico" } else { "generico" }, QUALITY_TIERS[target]),
        });
        if best.as_ref().is_none_or(|(best_score, _)| score > *best_score) {
            best = Some((score, candidate));
        }
    }
    best.map(|(_, candidate)| candidate)
}

/// Fill every plan entry that has no workflow yet.
fn assign_workflows(s: &Session, plan: &mut Value, quality: &str) {
    let entries = plan["entries"].as_array().cloned().unwrap_or_default();
    for (index, entry) in entries.iter().enumerate() {
        if entry.get("workflow").and_then(Value::as_str).is_some_and(|w| !w.trim().is_empty()) {
            continue;
        }
        let kind = entry["kind"].as_str().unwrap_or("image");
        if let Some(workflow) = choose_workflow(s, kind, quality) {
            plan["entries"][index]["workflow"] = workflow["path"].clone();
            plan["entries"][index]["workflowQuality"] = workflow["quality"].clone();
            plan["entries"][index]["workflowReason"] = workflow["reason"].clone();
        }
    }
    plan["quality"] = json!(QUALITY_TIERS[quality_index(quality)]);
}

/// `nl.agent.workflows` — what the suite would use for a kind and quality, with the alternatives.
fn workflows_recommend(s: &mut Session, p: &Value) -> Result<Value> {
    let kind = p.get("kind").and_then(Value::as_str).unwrap_or("image");
    if !matches!(kind, "image" | "video") {
        return Err(bad_cmd("nl.agent.workflows", "`kind` must be image or video"));
    }
    let quality = p.get("quality").and_then(Value::as_str).unwrap_or("standard");
    let target = quality_index(quality);
    let list = super::models::workflows(s, kind).map_err(|e| bad_cmd("nl.agent.workflows", e))?;
    let mut candidates: Vec<Value> = list
        .into_iter()
        .filter(|workflow| workflow.get("error").is_none())
        .map(|workflow| {
            let name = workflow["name"].as_str().unwrap_or("");
            let score = workflow_score(name, kind, target);
            json!({
                "path": workflow["path"],
                "name": name,
                "kind": kind,
                "quality": QUALITY_TIERS[workflow_quality(name)],
                "score": score,
                "special": is_special_workflow(name),
            })
        })
        .collect();
    candidates.sort_by(|a, b| b["score"].as_i64().cmp(&a["score"].as_i64()));
    let chosen = choose_workflow(s, kind, quality);
    let result = json!({
        "kind": kind,
        "quality": QUALITY_TIERS[target],
        "chosen": chosen,
        "candidates": candidates.into_iter().take(12).collect::<Vec<_>>(),
    });
    s.nl.results.insert("agent.workflows", result.clone());
    Ok(result)
}
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
            "audio" => "audio",
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
        if let Some(workflow) = get("workflow").and_then(Value::as_str).filter(|w| !w.trim().is_empty()) {
            row["workflow"] = json!(workflow);
        }
        if let Some(quality) = get("workflowQuality").and_then(Value::as_str).filter(|q| !q.is_empty()) {
            row["workflowQuality"] = json!(quality);
            row["workflowReason"] = get("workflowReason").cloned().unwrap_or(Value::Null);
        }
        if let Some(scene) = opt_id(get("scene")) {
            row["scene"] = json!(scene);
        }
        if let Some(shot) = opt_id(get("shot")) {
            row["shot"] = json!(shot);
        }
        entries.push(row);
    }
    json!({
        "subject": subject,
        "style": style,
        "source": text(object.and_then(|o| o.get("source")), 40),
        "mode": text(object.and_then(|o| o.get("mode")), 20),
        "quality": text(object.and_then(|o| o.get("quality")), 20),
        "title": text(object.and_then(|o| o.get("title")), MAX_TITLE),
        "synopsis": text(object.and_then(|o| o.get("synopsis")), MAX_TEXT),
        "reply": text(object.and_then(|o| o.get("reply")), MAX_TEXT),
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
    let quality = p.get("quality").and_then(Value::as_str).unwrap_or("standard");
    let mode = p.get("mode").and_then(Value::as_str).unwrap_or("objects");
    let (raw_entries, llm_error) = match p.get("entries").and_then(Value::as_array) {
        Some(entries) => (entries.clone(), None),
        None => model_entries(s, &subject, &style, count),
    };
    let mut plan = sanitize_plan(&json!({
        "subject": subject,
        "style": style,
        "mode": mode,
        "quality": quality,
        "source": if p.get("entries").is_some() { "given" } else if llm_error.is_some() { "subject-fallback" } else { "model" },
        "entries": raw_entries,
    }));
    assign_workflows(s, &mut plan, quality);
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
    let quality = p
        .get("quality")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| plan["quality"].as_str().map(str::to_string))
        .unwrap_or_else(|| "standard".into());
    plan["quality"] = json!(QUALITY_TIERS[quality_index(&quality)]);
    assign_workflows(s, &mut plan, &quality);
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
        let kind = entry["kind"].as_str().unwrap_or("image");
        if kind == "audio" {
            let wav = folder.join(format!("placeholder-{:02}.wav", index + 1));
            write_silent_wav(&wav, seconds)?;
            let imported = s.execute("file.import", json!({"paths": [wav.to_string_lossy()]}))?;
            let item = imported["items"]
                .as_array()
                .and_then(|items| items.first())
                .and_then(Value::as_u64)
                .ok_or_else(|| bad_cmd("nl.agent.sketch", "the audio placeholder was not imported"))?;
            let details = entry.clone();
            let _ = s.edit("NL Audio Placeholder Details", |project, _| {
                if let Some(project_item) = project.item_mut(ItemId(item)) {
                    project_item.metadata.insert("nl.role".into(), "agent-placeholder".into());
                    project_item.metadata.insert("nl.entry".into(), details["id"].as_u64().unwrap_or(0).to_string());
                    project_item.metadata.insert("nl.kind".into(), "audio".into());
                    project_item.metadata.insert("nl.prompt".into(), details["prompt"].as_str().unwrap_or("").to_string());
                }
                Ok(())
            });
            let name = format!("NL · {title} · audio");
            let _ = s.execute("clip.rename", json!({"item": item, "name": name}));
            let track = ensure_audio_track(s)?;
            let placed_result =
                s.execute("timeline.place", json!({"item": item, "track": track, "time": cursor.0, "duration": Tick::from_seconds_f64(seconds).0}))?;
            let clip = placed_result["clips"]
                .as_array()
                .and_then(|clips| clips.first())
                .and_then(Value::as_u64)
                .ok_or_else(|| bad_cmd("nl.agent.sketch", "the audio placeholder was not placed on the timeline"))?;
            let _ = s.execute("clip.rename", json!({"clips": [clip], "name": name}));
            plan["entries"][index]["placeholderItem"] = json!(item);
            plan["entries"][index]["clip"] = json!(clip);
            cursor = Tick(cursor.0 + Tick::from_seconds_f64(seconds).0);
            placed += 1;
            continue;
        }
        let png = folder.join(format!("placeholder-{:02}.png", index + 1));
        write_placeholder(&png, title, index)?;
        let imported = s.execute("file.import", json!({"paths": [png.to_string_lossy()]}))?;
        let item = imported["items"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(Value::as_u64)
            .ok_or_else(|| bad_cmd("nl.agent.sketch", "the placeholder image was not imported"))?;
        // The placeholder carries its own details: later passes and the Properties panel can see
        // what this object has to become without reading the sidecar plan.
        let details = entry.clone();
        let _ = s.edit("NL Placeholder Details", |project, _| {
            if let Some(project_item) = project.item_mut(ItemId(item)) {
                project_item.metadata.insert("nl.role".into(), "agent-placeholder".into());
                project_item.metadata.insert("nl.entry".into(), details["id"].as_u64().unwrap_or(0).to_string());
                project_item.metadata.insert("nl.title".into(), title.to_string());
                project_item.metadata.insert("nl.prompt".into(), details["prompt"].as_str().unwrap_or("").to_string());
                project_item.metadata.insert("nl.workflow".into(), details["workflow"].as_str().unwrap_or("").to_string());
                project_item.metadata.insert("nl.quality".into(), details["workflowQuality"].as_str().unwrap_or("").to_string());
                project_item.metadata.insert("nl.kind".into(), details["kind"].as_str().unwrap_or("image").to_string());
            }
            Ok(())
        });
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

fn ensure_audio_track(s: &mut Session) -> Result<String> {
    if let Some(index) = s.active_sequence().and_then(|sequence| sequence.audio_tracks.iter().rposition(|track| track.name == "NL Audio")) {
        return Ok(format!("A{}", index + 1));
    }
    s.execute("sequence.addTracks", json!({"audio": 1}))?;
    let count = s
        .active_sequence()
        .map(|sequence| sequence.audio_tracks.len())
        .ok_or_else(|| bad_cmd("nl.agent.sketch", "serve una sequenza con una traccia audio"))?;
    let name = format!("A{count}");
    let _ = s.execute("timeline.setTrack", json!({"track": name, "name": "NL Audio"}));
    Ok(name)
}

/// A silent PCM WAV placeholder so an audio object exists on the timeline before the real music
/// or voice is generated. 48 kHz, mono, 16-bit; the import and the timeline use the real duration.
fn write_silent_wav(path: &Path, seconds: f64) -> Result<()> {
    let rate = 48_000u32;
    let samples = (seconds.clamp(1.0, 300.0) * rate as f64).round() as u32;
    let data_len = samples * 2;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    bytes.resize(44 + data_len as usize, 0);
    std::fs::write(path, bytes).map_err(|e| bad_cmd("nl.agent.sketch", format!("{}: {e}", path.display())))
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
    let quality = plan["quality"].as_str().unwrap_or("standard").to_string();
    assign_workflows(s, &mut plan, &quality);
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
        if let Some(workflow) = workflow.as_deref().or_else(|| entry["workflow"].as_str()) {
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

/// The entries a story becomes on the placeholder track: one per scene, or one per shot when the
/// operator asked for video and the model returned shots.
fn story_entries(story: &Value, media: &str) -> Vec<Value> {
    let video = media == "video";
    let mut entries = Vec::new();
    for (index, scene) in story["scenes"].as_array().into_iter().flatten().enumerate() {
        let heading = scene["heading"].as_str().filter(|h| !h.is_empty()).map(str::to_string).unwrap_or_else(|| format!("Scena {}", index + 1));
        let shots = scene["shots"].as_array().cloned().unwrap_or_default();
        if video && !shots.is_empty() {
            for shot in shots {
                let prompt = shot["imagePrompt"]
                    .as_str()
                    .filter(|p| !p.is_empty())
                    .or_else(|| scene["motionPrompt"].as_str().filter(|p| !p.is_empty()))
                    .or_else(|| scene["action"].as_str())
                    .unwrap_or("")
                    .to_string();
                entries.push(json!({
                    "title": format!("{heading} · inquadratura {}", shot["id"].as_u64().unwrap_or(1)),
                    "prompt": prompt,
                    "seconds": shot["durationSec"].as_f64().unwrap_or(3.0),
                    "kind": "video",
                    "scene": index as u64 + 1,
                    "shot": shot["id"].as_u64().unwrap_or(1),
                }));
            }
        } else {
            let prompt = scene["imagePrompt"].as_str().filter(|p| !p.is_empty()).or_else(|| scene["action"].as_str()).unwrap_or("").to_string();
            entries.push(json!({
                "title": heading,
                "prompt": prompt,
                "seconds": scene["durationSec"].as_f64().unwrap_or(DEFAULT_SECONDS),
                "kind": if video { "video" } else { "image" },
                "scene": index as u64 + 1,
            }));
        }
    }
    entries
}

fn series_entries(answer: &Value, media: &str) -> Vec<Value> {
    answer["episodes"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, episode)| {
            json!({
                "title": episode["title"].as_str().filter(|t| !t.is_empty()).map(str::to_string).unwrap_or_else(|| format!("Episodio {}", index + 1)),
                "prompt": episode["prompt"].as_str().or_else(|| episode["logline"].as_str()).unwrap_or(""),
                "seconds": episode["seconds"].as_f64().unwrap_or(6.0),
                "kind": episode["kind"].as_str().unwrap_or(if media == "image" { "image" } else { "video" }),
            })
        })
        .collect()
}

fn quality_brief(quality: &str) -> &'static str {
    match quality_index(quality) {
        0 => "Bozza: pochi oggetti, prompt brevi, nessuna inquadratura secondaria.",
        1 => "Standard: 5-6 oggetti, prompt chiari con soggetto, luce e ambiente.",
        2 => "Alta: 7-8 oggetti, inquadrature curate, luce e composizione descritte.",
        _ => "Massima: fino a 12 oggetti, più inquadrature per scena, continuità e dettagli visivi.",
    }
}

/// `nl.agent.chat` — the production assistant: a message becomes a story, a series or a list of
/// objects; prompts and workflows are filled, and the result can be placed on the timeline.
fn chat(s: &mut Session, p: &Value) -> Result<Value> {
    let message = text(p.get("message").or_else(|| p.get("subject")), MAX_TEXT);
    if message.is_empty() {
        return Err(bad_cmd("nl.agent.chat", "serve un messaggio o un soggetto"));
    }
    let mode = p.get("mode").and_then(Value::as_str).unwrap_or("story");
    if !matches!(mode, "story" | "series" | "objects") {
        return Err(bad_cmd("nl.agent.chat", "`mode` deve essere story, series o objects"));
    }
    let media = p.get("media").and_then(Value::as_str).unwrap_or(if mode == "series" { "video" } else { "image" });
    let quality = p.get("quality").and_then(Value::as_str).unwrap_or("standard");
    let count = p.get("scenes").and_then(Value::as_u64).unwrap_or(6).clamp(1, MAX_ENTRIES as u64);
    let place = p.get("place").and_then(Value::as_bool).unwrap_or(false);
    let system = match mode {
        "series" => {
            "Sei un agente di produzione video. Trasforma la richiesta in una serie di episodi/clip. \
             Rispondi SOLO con JSON valido: {\"reply\":\"...\",\"title\":\"...\",\"style\":\"...\",\"episodes\":[{\"title\":\"...\",\"logline\":\"...\",\"prompt\":\"...\",\"seconds\":6,\"kind\":\"video\"}]}. \
             Ogni prompt deve essere autonomo, visivo e pronto per un generatore."
        }
        "objects" => {
            "Sei un agente di produzione video. Trasforma la richiesta in oggetti da generare. \
             Rispondi SOLO con JSON valido: {\"reply\":\"...\",\"title\":\"...\",\"style\":\"...\",\"entries\":[{\"title\":\"...\",\"prompt\":\"...\",\"seconds\":4,\"kind\":\"image\"}]}."
        }
        _ => {
            "Sei un agente di produzione video. Aiuta l'operatore a costruire una storia. \
             Rispondi SOLO con JSON valido: {\"reply\":\"...\",\"title\":\"...\",\"synopsis\":\"...\",\"style\":\"...\",\
             \"beats\":[\"...\"],\"characters\":[{\"name\":\"...\",\"description\":\"...\"}],\
             \"locations\":[{\"name\":\"...\",\"description\":\"...\"}],\
             \"scenes\":[{\"heading\":\"...\",\"action\":\"...\",\"dialogue\":\"...\",\"durationSec\":5,\
             \"imagePrompt\":\"...\",\"motionPrompt\":\"...\",\"shots\":[{\"camera\":\"...\",\"description\":\"...\",\"imagePrompt\":\"...\",\"durationSec\":3}]}]}."
        }
    };
    let prompt = format!(
        "Richiesta dell'operatore: {message}\nModalità: {mode}\nQualità: {quality}\n{}\n\
         Numero massimo di scene/oggetti: {count}\nScrivi in italiano.",
        quality_brief(quality)
    );
    let config = super::llm::LlmConfig::for_session(s, None);
    let answer =
        config.auto_json(system, &prompt, Duration::from_secs(180), Some("0")).map_err(|e| bad_cmd("nl.agent.chat", format!("LLM non disponibile: {e}")))?;
    let reply = text(answer.get("reply"), MAX_TEXT);
    let title = text(answer.get("title"), MAX_TITLE);
    let synopsis = text(answer.get("synopsis"), MAX_TEXT);
    let style = text(answer.get("style"), MAX_TITLE);
    let mut story = Value::Null;
    let entries = match mode {
        "series" => series_entries(&answer, media),
        "objects" => answer["entries"].as_array().cloned().unwrap_or_default(),
        _ => {
            story = json!({
                "title": title,
                "synopsis": synopsis,
                "style": style,
                "beats": answer.get("beats").cloned().unwrap_or(json!([])),
                "characters": answer.get("characters").cloned().unwrap_or(json!([])),
                "locations": answer.get("locations").cloned().unwrap_or(json!([])),
                "scenes": answer.get("scenes").cloned().unwrap_or(json!([])),
            });
            if let Err(error) = s.execute("nl.story.set", json!({"story": story})) {
                return Err(bad_cmd("nl.agent.chat", format!("storia non salvata: {error}")));
            }
            story_entries(&story, media)
        }
    };
    if entries.is_empty() {
        return Err(bad_cmd("nl.agent.chat", "il modello non ha restituito scene o oggetti"));
    }
    let mut plan = sanitize_plan(&json!({
        "subject": message,
        "style": style,
        "mode": mode,
        "quality": quality,
        "title": title,
        "synopsis": synopsis,
        "reply": reply,
        "source": "assistant",
        "entries": entries,
    }));
    assign_workflows(s, &mut plan, quality);
    let mut placed = 0;
    if place {
        let start = p.get("startSeconds").and_then(Value::as_f64).map(Tick::from_seconds_f64).unwrap_or_else(|| s.playhead());
        placed = place_plan(s, &mut plan, start)?;
    }
    save(s, &plan)?;
    s.nl.results.insert("agent.plan", plan.clone());
    s.nl.results.insert("agent.reply", json!({"reply": reply, "title": title, "synopsis": synopsis}));
    Ok(json!({
        "reply": reply,
        "title": title,
        "synopsis": synopsis,
        "mode": mode,
        "quality": QUALITY_TIERS[quality_index(quality)],
        "entries": plan["entries"].as_array().map_or(0, Vec::len),
        "placed": placed,
        "story": story,
        "plan": plan,
    }))
}

/// `nl.agent.prompt` — edit one object's prompt before it goes to ComfyUI.
fn prompt_set(s: &mut Session, p: &Value) -> Result<Value> {
    let id = p.get("id").and_then(Value::as_u64).ok_or_else(|| bad_cmd("nl.agent.prompt", "serve `id`"))?;
    let prompt = text(p.get("prompt"), MAX_PROMPT);
    let mut plan = load(s);
    let entries = plan["entries"].as_array_mut().ok_or_else(|| bad_cmd("nl.agent.prompt", "plan has no entries"))?;
    let entry =
        entries.iter_mut().find(|entry| entry["id"].as_u64() == Some(id)).ok_or_else(|| bad_cmd("nl.agent.prompt", format!("no object {id} in the plan")))?;
    entry["prompt"] = json!(prompt);
    entry["status"] = json!(if entry["status"] == "generated" { "generated" } else { "ready" });
    save(s, &plan)?;
    s.nl.results.insert("agent.plan", plan.clone());
    Ok(json!({"id": id, "prompt": prompt, "plan": plan}))
}

/// `nl.agent.place` — put the current plan's missing objects on the timeline.
fn place(s: &mut Session, p: &Value) -> Result<Value> {
    let mut plan = load(s);
    let quality = plan["quality"].as_str().unwrap_or("standard").to_string();
    assign_workflows(s, &mut plan, &quality);
    let start = p.get("startSeconds").and_then(Value::as_f64).map(Tick::from_seconds_f64).unwrap_or_else(|| s.playhead());
    let placed = place_plan(s, &mut plan, start)?;
    save(s, &plan)?;
    Ok(json!({"placed": placed, "plan": plan}))
}

fn media_path(s: &Session, id: u64) -> Option<String> {
    let item = s.project.item(ItemId(id))?;
    let ItemKind::Media(clip) = &item.kind else { return None };
    let MediaRef::File { path } = &clip.media else { return None };
    Some(path.clone())
}

fn find_bin_named(bin: &filmcraft_project::Bin, name: &str) -> Option<u64> {
    if bin.name == name {
        return Some(bin.id.0);
    }
    for child in &bin.children {
        if let BinEntry::Bin(sub) = child
            && let Some(found) = find_bin_named(sub, name)
        {
            return Some(found);
        }
    }
    None
}

fn footage_ai_bin(s: &mut Session) -> Result<u64> {
    if let Some(id) = find_bin_named(&s.project.root, "Footage_AI") {
        return Ok(id);
    }
    s.edit("New Footage_AI Bin", |project, _| Ok(project.add_bin("Footage_AI", None).0))
}

fn into_footage_ai(s: &mut Session, item: u64, name: &str) -> Result<u64> {
    let bin = footage_ai_bin(s)?;
    s.execute("project.moveToBin", json!({"items": [item], "bin": bin}))?;
    let _ = s.execute("clip.rename", json!({"item": item, "name": name}));
    Ok(bin)
}

/// The last ComfyUI import: its project item and the downloaded file.
fn last_comfy(s: &Session) -> Option<(u64, String)> {
    let last = s.nl.results.get("comfy.last")?;
    Some((last["first"]["item"].as_u64()?, last["first"]["path"].as_str()?.to_string()))
}

/// Move a generated file into the project and relink the item. The download folder is a cache, not
/// the project: the project must keep working if that cache is cleaned.
fn adopt_generated(s: &mut Session, item: u64, source: &str) -> Result<String> {
    let folder = project_dir(s)?.join("generated");
    std::fs::create_dir_all(&folder).map_err(|e| bad_cmd("nl.agent", format!("{}: {e}", folder.display())))?;
    let dest = move_to_project(std::path::Path::new(source), &folder, item)?;
    s.execute("media.relink", json!({"item": item, "path": dest.to_string_lossy(), "force": true}))?;
    Ok(dest.to_string_lossy().to_string())
}

fn generate_asset(s: &mut Session, prompt: &str, workflow: &str, kind: &str, extra: Value) -> Result<Value> {
    let mut params = json!({"prompt": prompt, "workflow": workflow, "kind": kind, "wait": true});
    if let Some(extra) = extra.as_object() {
        for (key, value) in extra {
            params[key] = value.clone();
        }
    }
    s.execute("graphics.generateComfy", params)?;
    let (item, source) = last_comfy(s).ok_or_else(|| bad_cmd("nl.agent", "ComfyUI returned no media"))?;
    let asset = adopt_generated(s, item, &source)?;
    Ok(json!({"item": item, "asset": asset, "workflow": workflow}))
}

/// Whether every model name in a workflow exists in this ComfyUI. A workflow that references a
/// checkpoint/VAE/LoRA the machine does not have is not a candidate for automatic selection.
fn workflow_models_available(s: &mut Session, path: &str) -> bool {
    let Some(file) = super::models::find(s, path) else { return false };
    let Ok(text) = std::fs::read_to_string(file) else { return false };
    let Ok(workflow) = serde_json::from_str::<Value>(&text) else { return false };
    let info = match s.nl.object_info.clone() {
        Some(info) => info,
        None => {
            let url = s.prefs.nl.comfy_url();
            let Ok(info) = super::http::get_json(&url, "object_info", Duration::from_secs(120)) else { return false };
            s.nl.object_info = Some(info.clone());
            info
        }
    };
    const MODEL_KEYS: [&str; 8] = ["ckpt_name", "unet_name", "lora_name", "vae_name", "clip_name", "text_encoder", "model_name", "control_net_name"];
    for node in workflow.as_object().into_iter().flat_map(|nodes| nodes.values()) {
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default();
        if info.get(class).is_none() {
            return false;
        }
        let Some(required) = info.get(class).and_then(|class| class.pointer("/input/required")).and_then(Value::as_object) else { continue };
        for (key, value) in node.get("inputs").and_then(Value::as_object).into_iter().flatten() {
            if !MODEL_KEYS.contains(&key.as_str()) {
                continue;
            }
            let Value::String(current) = value else { continue };
            let Some(options) = required.get(key).and_then(Value::as_array).and_then(|spec| spec.first()).and_then(Value::as_array) else { continue };
            let names: Vec<&str> = options.iter().filter_map(Value::as_str).collect();
            if names.contains(&current.as_str()) {
                continue;
            }
            let wanted = current.replace('\\', "/").to_ascii_lowercase();
            let wanted_base = wanted.rsplit('/').next().unwrap_or(&wanted);
            let available = names.iter().any(|name| {
                let name = name.replace('\\', "/").to_ascii_lowercase();
                name == wanted || name.rsplit('/').next().is_some_and(|base| base == wanted_base)
            });
            if !available {
                return false;
            }
        }
    }
    true
}

/// A text-to-video workflow with an image input; a T2V has none and cannot animate a still.
fn choose_i2v_workflow(s: &mut Session, quality: &str) -> Option<Value> {
    let target = quality_index(quality);
    let list = super::models::workflows(s, "video").ok()?;
    let mut best: Option<(i32, Value)> = None;
    for workflow in list {
        let name = workflow["name"].as_str().unwrap_or("");
        let path = workflow["path"].as_str().unwrap_or("");
        let images = workflow["capabilities"]["imageRefs"].as_array().map_or(0, Vec::len);
        if name.is_empty() || path.is_empty() || images == 0 || workflow.get("error").is_some() {
            continue;
        }
        if !workflow_models_available(s, path) {
            continue;
        }
        let mut score = workflow_score(name, "video", target);
        if images == 1 {
            score += 25;
        }
        if name.to_ascii_lowercase().contains("r2v") {
            score += 80;
        }
        if name.to_ascii_lowercase().contains("image2video") || name.to_ascii_lowercase().contains("i2v") {
            score += 20;
        }
        let candidate = json!({
            "path": path,
            "name": name,
            "kind": "video",
            "imageInputs": images,
            "quality": QUALITY_TIERS[workflow_quality(name)],
            "score": score,
            "reason": format!("{name}: i2v con {images} ingresso/i immagine, qualità {}", QUALITY_TIERS[workflow_quality(name)]),
        });
        if best.as_ref().is_none_or(|(best_score, _)| score > *best_score) {
            best = Some((score, candidate));
        }
    }
    best.map(|(_, candidate)| candidate)
}

/// `nl.agent.characterSheet` — a four-view character sheet, put in Footage_AI for reuse.
fn character_sheet(s: &mut Session, p: &Value) -> Result<Value> {
    let name = text(p.get("name"), MAX_TITLE);
    let description = text(p.get("description").or_else(|| p.get("prompt")), MAX_PROMPT);
    if description.is_empty() {
        return Err(bad_cmd("nl.agent.characterSheet", "serve `description` o `prompt` (come è fatto il personaggio)"));
    }
    let quality = p.get("quality").and_then(Value::as_str).unwrap_or("alta");
    let workflow = choose_workflow(s, "image", quality).ok_or_else(|| bad_cmd("nl.agent.characterSheet", "no image workflow for this quality"))?;
    let path = workflow["path"].as_str().unwrap_or("");
    let prompt = format!(
        "Character reference sheet, four views on a clean white background: front full body, side full body, back full body, and close-up headshot. \
         {description}. Same costume and proportions in every view, sharp focus, professional character design sheet."
    );
    let mut result = generate_asset(s, &prompt, path, "image", json!({"resolutionMode": "image"}))?;
    let item = result["item"].as_u64().ok_or_else(|| bad_cmd("nl.agent.characterSheet", "no item"))?;
    let label = if name.is_empty() { "Character Sheet".to_string() } else { format!("Character Sheet · {name}") };
    let bin = into_footage_ai(s, item, &label)?;
    result["item"] = json!(item);
    result["bin"] = json!(bin);
    result["label"] = json!(label);
    s.nl.results.insert("agent.characterSheet", result.clone());
    Ok(result)
}

/// `nl.agent.animate` — a still (Footage_AI item or path) becomes a video with an i2v workflow and
/// replaces the plan object's placeholder in place.
fn animate(s: &mut Session, p: &Value) -> Result<Value> {
    let entry_id = p.get("id").and_then(Value::as_u64);
    let mut plan = load(s);
    let entry = entry_id.and_then(|id| plan["entries"].as_array().and_then(|entries| entries.iter().find(|entry| entry["id"].as_u64() == Some(id))).cloned());
    let prompt = text(p.get("prompt").or_else(|| entry.as_ref().and_then(|entry| entry.get("prompt"))), MAX_PROMPT);
    if prompt.is_empty() {
        return Err(bad_cmd("nl.agent.animate", "serve `prompt` o un oggetto del piano con prompt"));
    }
    let image = p
        .get("image")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| entry.as_ref().and_then(|entry| entry["asset"].as_str()).map(str::to_string))
        .or_else(|| entry.as_ref().and_then(|entry| entry["generatedItem"].as_u64()).and_then(|item| media_path(s, item)))
        .or_else(|| p.get("item").and_then(Value::as_u64).and_then(|item| media_path(s, item)))
        .ok_or_else(|| bad_cmd("nl.agent.animate", "serve `image`, `item` (Footage_AI), o un oggetto con asset generato"))?;
    let quality = p.get("quality").and_then(Value::as_str).unwrap_or("standard");
    let workflow = match p.get("workflow").and_then(Value::as_str) {
        Some(path) => json!({"path": path, "name": path}),
        None => choose_i2v_workflow(s, quality).ok_or_else(|| bad_cmd("nl.agent.animate", "no image-to-video workflow found"))?,
    };
    let path = workflow["path"].as_str().unwrap_or("");
    let seconds = p
        .get("seconds")
        .and_then(Value::as_u64)
        .or_else(|| entry.as_ref().and_then(|entry| entry["seconds"].as_f64()).map(|seconds| seconds.round() as u64))
        .unwrap_or(4);
    if p.get("dryRun").and_then(Value::as_bool).unwrap_or(false) {
        let dry = s.execute(
            "graphics.generateComfy",
            json!({"prompt": prompt, "workflow": path, "kind": "video", "image": image, "seconds": seconds, "dryRun": true}),
        )?;
        return Ok(json!({"dryRun": true, "workflow": path, "generated": dry}));
    }
    let mut result = generate_asset(s, &prompt, path, "video", json!({"image": image, "seconds": seconds}))?;
    let item = result["item"].as_u64().ok_or_else(|| bad_cmd("nl.agent.animate", "no item"))?;
    let mut replaced = false;
    if let Some(entry) = entry {
        let id = entry["id"].as_u64().unwrap_or(0);
        if let Some(clip) = entry["clip"].as_u64() {
            s.execute("clip.replaceFromBin", json!({"clips": [clip], "item": item, "keepSourceIn": false}))?;
            let title = entry["title"].as_str().unwrap_or("NL");
            let _ = s.execute("clip.rename", json!({"clips": [clip], "name": format!("NL · {title}")}));
            replaced = true;
        }
        if let Some(index) = plan["entries"].as_array().and_then(|entries| entries.iter().position(|row| row["id"].as_u64() == Some(id))) {
            plan["entries"][index]["generatedItem"] = json!(item);
            plan["entries"][index]["status"] = json!("generated");
            plan["entries"][index]["asset"] = result["asset"].clone();
            plan["entries"][index]["workflow"] = json!(path);
            save(s, &plan)?;
        }
    }
    result["replaced"] = json!(replaced);
    result["workflow"] = json!(path);
    s.nl.results.insert("agent.animate", result.clone());
    Ok(result)
}

/// `nl.agent.squad` — a small production squad working in order. The roles are selective: a run
/// with only `prompts` does not touch the story, and `assets`/`video` only queue work for the
/// ComfyUI queue. The local LLM and ComfyUI never work at the same time: planning is synchronous,
/// render jobs are submitted later by `run`.
fn squad(s: &mut Session, p: &Value) -> Result<Value> {
    let action = p.get("action").and_then(Value::as_str).unwrap_or("plan");
    let roles: Vec<String> = p
        .get("roles")
        .and_then(Value::as_array)
        .map(|roles| roles.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_else(|| vec!["story".into(), "prompts".into()]);
    if action == "plan" {
        let brief = text(p.get("brief").or_else(|| p.get("message")), MAX_TEXT);
        let mode = p.get("mode").and_then(Value::as_str).unwrap_or("story");
        let media = p.get("media").and_then(Value::as_str).unwrap_or(if mode == "series" { "video" } else { "image" });
        let quality = p.get("quality").and_then(Value::as_str).unwrap_or("standard");
        let mut stages = Vec::new();
        if roles.iter().any(|role| role == "story") {
            if brief.is_empty() {
                return Err(bad_cmd("nl.agent.squad", "serve `brief` per lo stadio story"));
            }
            let result = chat(
                s,
                &json!({"message": brief, "mode": mode, "media": media, "quality": quality, "scenes": p.get("scenes").cloned().unwrap_or(json!(6)), "place": false}),
            )?;
            stages.push(json!({"role": "story", "status": "done", "title": result["title"], "entries": result["entries"]}));
        }
        if roles.iter().any(|role| role == "script") {
            let plan = load(s);
            let shots: usize = plan["entries"].as_array().map_or(0, |entries| entries.iter().filter(|entry| entry.get("shot").is_some()).count());
            stages.push(json!({"role": "script", "status": "done", "shots": shots, "note": "scene/shot già prodotti dallo story agent"}));
        }
        if roles.iter().any(|role| role == "prompts") {
            let result = prompts(s, &json!({}))?;
            stages.push(json!({"role": "prompts", "status": "done", "updated": result["updated"]}));
        }
        let plan = load(s);
        if roles.iter().any(|role| role == "assets" || role == "video") {
            let kinds: Vec<&str> = roles
                .iter()
                .filter_map(|role| match role.as_str() {
                    "assets" => Some("image"),
                    "video" => Some("video"),
                    _ => None,
                })
                .collect();
            let queued = plan["entries"]
                .as_array()
                .map_or(0, |entries| entries.iter().filter(|entry| kinds.contains(&entry["kind"].as_str().unwrap_or("image"))).count());
            stages.push(json!({
                "role": "assets/video",
                "status": "ready",
                "queued": queued,
                "note": "usa action=run per inviare tutto alla coda ComfyUI quando il LLM è a riposo",
            }));
        }
        let mut plan = load(s);
        plan["stages"] = json!(stages);
        plan["squadRoles"] = json!(roles);
        save(s, &plan)?;
        s.nl.results.insert("agent.plan", plan.clone());
        return Ok(json!({"action": "plan", "stages": stages, "plan": plan}));
    }
    if action == "run" {
        let mut plan = load(s);
        let kinds: Vec<&str> = roles
            .iter()
            .filter_map(|role| match role.as_str() {
                "assets" => Some("image"),
                "video" => Some("video"),
                _ => None,
            })
            .collect();
        let entries = plan["entries"].as_array().cloned().unwrap_or_default();
        let mut submitted = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            let kind = entry["kind"].as_str().unwrap_or("image");
            if !kinds.is_empty() && !kinds.contains(&kind) {
                continue;
            }
            if entry["status"].as_str() == Some("generated") || entry["prompt"].as_str().unwrap_or("").trim().is_empty() {
                continue;
            }
            let workflow = entry["workflow"].as_str().unwrap_or("");
            if workflow.is_empty() {
                continue;
            }
            let result = s.execute(
                "graphics.generateComfy",
                json!({
                    "prompt": entry["prompt"],
                    "workflow": workflow,
                    "kind": kind,
                    "wait": false,
                }),
            )?;
            let job = result["job"].as_u64().or_else(|| result["id"].as_u64()).unwrap_or(0);
            plan["entries"][index]["status"] = json!("queued");
            plan["entries"][index]["job"] = json!(job);
            submitted.push(json!({"id": entry["id"], "kind": kind, "job": job}));
        }
        save(s, &plan)?;
        return Ok(json!({"action": "run", "submitted": submitted, "plan": plan}));
    }
    let plan = load(s);
    Ok(json!({"action": "status", "stages": plan["stages"], "entries": plan["entries"]}))
}

/// `nl.agent.audio` — generate the audio for an audio placeholder and replace it in place.
fn generate_audio(s: &mut Session, p: &Value) -> Result<Value> {
    let id = p.get("id").and_then(Value::as_u64).ok_or_else(|| bad_cmd("nl.agent.audio", "serve `id` dell'oggetto audio"))?;
    let mut plan = load(s);
    let entry = plan["entries"]
        .as_array()
        .and_then(|entries| entries.iter().find(|entry| entry["id"].as_u64() == Some(id)))
        .cloned()
        .ok_or_else(|| bad_cmd("nl.agent.audio", format!("no object {id} in the plan")))?;
    if entry["kind"] != "audio" {
        return Err(bad_cmd("nl.agent.audio", "this object is not an audio placeholder"));
    }
    let prompt = text(p.get("prompt").or_else(|| entry.get("prompt")), MAX_PROMPT);
    if prompt.is_empty() {
        return Err(bad_cmd("nl.agent.audio", "serve un prompt audio"));
    }
    let seconds = entry["seconds"].as_f64().unwrap_or(10.0).round() as u64;
    if p.get("dryRun").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(json!({"dryRun": true, "prompt": prompt, "seconds": seconds}));
    }
    s.execute("audio.generateMusic", json!({"prompt": prompt, "seconds": seconds, "wait": true}))?;
    let (item, source) = last_comfy(s).ok_or_else(|| bad_cmd("nl.agent.audio", "ComfyUI returned no audio"))?;
    let asset = adopt_generated(s, item, &source)?;
    if let Some(clip) = entry["clip"].as_u64() {
        s.execute("clip.replaceFromBin", json!({"clips": [clip], "item": item, "keepSourceIn": false}))?;
        let title = entry["title"].as_str().unwrap_or("Audio");
        let _ = s.execute("clip.rename", json!({"clips": [clip], "name": format!("NL · {title} · audio")}));
    }
    if let Some(index) = plan["entries"].as_array().and_then(|entries| entries.iter().position(|entry| entry["id"].as_u64() == Some(id))) {
        plan["entries"][index]["status"] = json!("generated");
        plan["entries"][index]["asset"] = json!(asset);
        plan["entries"][index]["generatedItem"] = json!(item);
        save(s, &plan)?;
    }
    Ok(json!({"id": id, "item": item, "asset": asset, "seconds": seconds}))
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
            r#"{"subject":str?,"style":str?,"mode":"objects|story|series"?,"quality":"bozza|standard|alta|massima"?,"scenes":int?,"entries":[obj]?,"startSeconds":f?,"dryRun":bool?}"#,
            sketch,
        ),
        spec(
            "nl.agent.chat",
            "NL Production Assistant",
            r#"{"message":str?,"subject":str?,"mode":"story|series|objects"?,"media":"image|video"?,"quality":"bozza|standard|alta|massima"?,"scenes":int?,"place":bool?,"startSeconds":f?}"#,
            chat,
        ),
        spec("nl.agent.workflows", "NL Workflow Recommendation", r#"{"kind":"image|video","quality":"bozza|standard|alta|massima"?}"#, workflows_recommend),
        spec("nl.agent.place", "NL Place Agent Plan", r#"{"startSeconds":f?}"#, place),
        spec("nl.agent.add", "NL Add Generation Object", r#"{"title":str?,"prompt":str?,"seconds":f?,"kind":"image|video"?,"startSeconds":f?}"#, add),
        spec("nl.agent.prompts", "NL Fill Object Prompts", r#"{"force":bool?}"#, prompts),
        spec("nl.agent.prompt", "NL Edit Object Prompt", r#"{"id":int,"prompt":str}"#, prompt_set),
        spec("nl.agent.generate", "NL Generate Timeline Objects", r#"{"all":bool?,"id":int?,"workflow":str?,"dryRun":bool?}"#, generate),
        spec("nl.agent.collect", "NL Collect Project Assets", r#"{"dryRun":bool?}"#, collect),
        spec(
            "nl.agent.characterSheet",
            "NL Character Sheet",
            r#"{"name":str?,"description":str?,"prompt":str?,"quality":"bozza|standard|alta|massima"?}"#,
            character_sheet,
        ),
        spec(
            "nl.agent.animate",
            "NL Animate Still to Video",
            r#"{"id":int?,"item":id?,"image":str?,"prompt":str?,"workflow":str?,"quality":str?,"seconds":int?,"dryRun":bool?}"#,
            animate,
        ),
        spec("nl.agent.audio", "NL Generate Audio Placeholder", r#"{"id":int,"prompt":str?,"dryRun":bool?}"#, generate_audio),
        spec(
            "nl.agent.squad",
            "NL Production Squad",
            r#"{"action":"plan|run|status"?,"roles":["story","script","prompts","assets","video"]?,"brief":str?,"mode":"story|series|objects"?,"media":"image|video"?,"quality":str?,"scenes":int?}"#,
            squad,
        ),
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

#[test]
fn quality_selects_a_workflow_family_not_just_the_first_file() {
    assert_eq!(workflow_quality("z-image-turbo"), 0);
    assert_eq!(workflow_quality("image_flux2_text_to_image_9b"), 3);
    assert_eq!(workflow_quality("image_ideogram4_t2i_fixed"), 3);
    assert!(workflow_score("image_flux2_text_to_image_9b", "image", 3) > workflow_score("z-image-turbo", "image", 3));
    assert!(workflow_score("z-image-turbo", "image", 0) > workflow_score("image_flux2_text_to_image_9b", "image", 0));
    assert!(is_special_workflow("MiniMax-H3-Turbo-R2V-Extend-2Refs1Vid_API"));
    assert_eq!(quality_index("massima"), 3);
    assert_eq!(quality_index("unknown"), 1);
}

#[test]
fn a_story_becomes_scene_or_shot_placeholders() {
    let story = json!({"scenes": [
        {"heading": "Apertura", "action": "strada bagnata", "imagePrompt": "neon", "durationSec": 4,
         "shots": [{"id": 1, "imagePrompt": "wide", "durationSec": 2}, {"id": 2, "imagePrompt": "close", "durationSec": 3}]},
        {"heading": "Finale", "action": "skyline", "durationSec": 5}
    ]});
    let images = story_entries(&story, "image");
    assert_eq!(images.len(), 2);
    assert_eq!(images[0]["kind"], "image");
    assert_eq!(images[0]["prompt"], "neon");
    let videos = story_entries(&story, "video");
    assert_eq!(videos.len(), 3, "two shots for the first scene and one for the second");
    assert_eq!(videos[1]["kind"], "video");
    assert_eq!(videos[1]["shot"], 2);
}

#[test]
fn a_series_becomes_episode_entries() {
    let answer = json!({"episodes": [{"title": "Ep 1", "logline": "inizio", "seconds": 8}, {"title": "Ep 2", "logline": "fine"}]});
    let entries = series_entries(&answer, "video");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["kind"], "video");
    assert_eq!(entries[1]["title"], "Ep 2");
}
