//! Local ComfyUI generation: submit an API-format workflow, wait for it, download the
//! produced media and import it into the project.
//!
//! The suite already ships real workflows under `workflows/`; this module does not ship model
//! weights and does not decide a model for the user. It fills a prompt slot explicitly
//! (`{{prompt}}` or a node titled "Positive Prompt"), submits to the ComfyUI the operator runs.
#![cfg(not(target_arch = "wasm32"))]

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::{EngineError, Result, Session};

const DEFAULT_WORKFLOWS: &[&str] = &[
    "workflows/image/text-to-image/z-image-turbo.json",
    "workflows/image/text-to-image/image_z_image.json",
    "workflows/image/text-to-image/image_flux2_text_to_image_9b.json",
];
const DEFAULT_MUSIC_WORKFLOW: &str = "workflows/audio/music/audio_ace_step1_5_xl_sft.json";
const MEDIA_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "wav", "flac", "mp3", "ogg", "mp4", "webm", "mov"];

fn bad(e: impl ToString) -> EngineError {
    EngineError::BadParams { cmd: "graphics.generateComfy".into(), msg: e.to_string() }
}

/// A generation job whose downloaded files are imported when it finishes.
pub struct PendingComfy {
    job: u64,
    files: Arc<Mutex<Option<std::result::Result<Vec<String>, String>>>>,
    epoch: u64,
    /// The story scene/shot this generation belongs to, so the imported media is linked back.
    story: Option<Value>,
    /// Where to place the first imported media (a bridge gap, the playhead): passed to
    /// `timeline.place` after the import.
    place: Option<Value>,
}

const MODEL_KEYS: &[&str] = &["ckpt_name", "unet_name", "lora_name", "vae_name", "clip_name", "text_encoder", "model_name", "control_net_name"];

fn normalize_model_path(value: &str) -> String {
    value.replace('\\', "/").to_ascii_lowercase()
}

/// ComfyUI refuses a model name that is not in the list it reports for that node. A workflow
/// exported on another machine can carry the right filename with the wrong folder case
/// (`minimax-h3` vs `MiniMaxH3`), so match by full path first and then by basename, ignoring case
/// and separator. This repairs the copy in memory only; the workflow file is never edited.
fn repair_model_names(s: &mut Session, workflow: &mut Value) -> Result<usize> {
    let Some(nodes) = workflow.as_object() else { return Ok(0) };
    if nodes.values().all(|node| {
        node.get("inputs")
            .and_then(Value::as_object)
            .is_none_or(|inputs| inputs.iter().all(|(key, value)| !MODEL_KEYS.contains(&key.as_str()) || !value.is_string()))
    }) {
        return Ok(0);
    }
    let url = s.prefs.nl.comfy_url();
    let info = match s.nl.object_info.clone() {
        Some(info) => info,
        None => {
            let info = crate::nl::http::get_json(&url, "object_info", Duration::from_secs(120)).map_err(|e| bad(format!("ComfyUI {url}: {e}")))?;
            s.nl.object_info = Some(info.clone());
            info
        }
    };
    let Some(nodes) = workflow.as_object_mut() else { return Ok(0) };
    let mut repaired = 0;
    for node in nodes.values_mut() {
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default().to_string();
        let Some(required) = info.get(&class).and_then(|class| class.pointer("/input/required")).and_then(Value::as_object) else { continue };
        let Some(inputs) = node.get_mut("inputs").and_then(Value::as_object_mut) else { continue };
        for (key, value) in inputs.iter_mut() {
            if !MODEL_KEYS.contains(&key.as_str()) {
                continue;
            }
            let Value::String(current) = value else { continue };
            let Some(options) = required.get(key).and_then(Value::as_array).and_then(|spec| spec.first()).and_then(Value::as_array) else { continue };
            let names: Vec<&str> = options.iter().filter_map(Value::as_str).collect();
            if names.contains(&current.as_str()) {
                continue;
            }
            let wanted = normalize_model_path(current);
            let wanted_base = wanted.rsplit('/').next().unwrap_or(&wanted);
            let candidate = names
                .iter()
                .find(|name| normalize_model_path(name) == wanted)
                .or_else(|| names.iter().find(|name| normalize_model_path(name).rsplit('/').next().is_some_and(|base| base == wanted_base)));
            if let Some(candidate) = candidate {
                *current = candidate.to_string();
                repaired += 1;
            }
        }
    }
    Ok(repaired)
}

/// `graphics.generateComfy` — generate media with a local ComfyUI and import it.
fn generate(s: &mut Session, p: &Value) -> Result<Value> {
    let prompt = p.get("prompt").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| bad("`prompt` is required"))?.to_string();
    let url = p.get("url").and_then(Value::as_str).map(str::trim).filter(|u| !u.is_empty()).map(str::to_string).unwrap_or_else(|| s.prefs.nl.comfy_url());
    let workflow_path = resolve_workflow(s, p)?;
    let text = std::fs::read_to_string(&workflow_path).map_err(|e| bad(format!("{}: {e}", workflow_path.display())))?;
    let mut workflow: Value = serde_json::from_str(&text).map_err(|e| bad(format!("{}: not a ComfyUI API workflow: {e}", workflow_path.display())))?;
    if !workflow.is_object() {
        return Err(bad("the workflow must be a ComfyUI API-format JSON object"));
    }
    let seed = p.get("seed").and_then(Value::as_u64).unwrap_or_else(random_seed);
    let negative = p.get("negative").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let lyrics = p.get("lyrics").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    // A character-sheet workflow ships its own carefully written prompt chain (apex H3). Keep
    // those defaults: replacing them with the caller's brief is what produced the martini glass
    // in every unrelated image. Other workflows get their prompts replaced as usual.
    let keep_default_prompts =
        workflow_path.file_stem().and_then(|stem| stem.to_str()).is_some_and(|stem| stem.to_ascii_lowercase().contains("character_sheet"));
    let (replaced, filled) = if keep_default_prompts {
        (false, true)
    } else {
        let replaced = replace_tokens(&mut workflow, &prompt, negative, lyrics, seed);
        let filled = fill_prompt(&mut workflow, &prompt, negative, lyrics);
        (replaced, filled)
    };
    if !replaced && !filled {
        return Err(bad("the workflow has no prompt slot: add the literal {{prompt}} to a text input or title a CLIPTextEncode node \"Positive Prompt\""));
    }
    set_seed(&mut workflow, seed);
    if let Some(width) = p.get("width").and_then(Value::as_u64)
        && let Some(height) = p.get("height").and_then(Value::as_u64)
    {
        set_size(&mut workflow, width as u32, height as u32);
    }
    if let Some(seconds) = p.get("seconds").and_then(Value::as_u64) {
        set_duration(&mut workflow, seconds);
    }
    // The sequence is the source of truth for generated media sizes. Explicit width/height win.
    if p.get("width").is_none()
        && p.get("height").is_none()
        && let Some((width, height)) = crate::nl::size::from_params(s, p)
    {
        set_size(&mut workflow, width, height);
    }
    // Widget overrides from the parameter panel / queue: they must name an existing node and field,
    // so a typo is refused instead of silently ignored.
    let overridden = apply_overrides(&mut workflow, p.get("overrides"))?;
    let repaired = repair_model_names(s, &mut workflow)?;
    if repaired > 0 {
        s.toast(format!("ComfyUI: repaired {repaired} model name(s) to the names this ComfyUI has"));
    }
    // Reference files (image/video/audio) are uploaded and named in the workflow's loaders before
    // submitting: a workflow without the input is refused here, not left to fail in ComfyUI.
    let staged = stage_references(&url, p, &mut workflow)?;
    // Dry run: the operator (or a script) sees exactly what would be submitted — the prompt, the
    // size derived from the sequence, the reference files and the patched loaders — without
    // sending anything to ComfyUI. Useful to verify a v2v/r2v payload before a long generation.
    if p.get("dryRun").and_then(Value::as_bool).unwrap_or(false) {
        let size = crate::nl::size::from_params(s, p);
        return Ok(json!({
            "dryRun": true,
            "workflow": workflow_path.to_string_lossy(),
            "url": url,
            "prompt": prompt,
            "seed": seed,
            "size": size.map(|(w, h)| json!({"width": w, "height": h})),
            "references": staged,
            "nodes": workflow.as_object().map_or(0, serde_json::Map::len),
            "overridden": overridden,
            "modelNamesRepaired": repaired,
            "submitted": false,
        }));
    }

    let id = s.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let label = format!("ComfyUI: {}", workflow_path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "workflow".into()));
    let job = crate::Job { id, label, progress: Default::default(), result: Default::default() };
    let (prog, res) = (job.progress.clone(), job.result.clone());
    let files: Arc<Mutex<Option<std::result::Result<Vec<String>, String>>>> = Arc::default();
    let out = files.clone();
    let dir = generated_dir(id);
    let wait = p.get("wait").and_then(Value::as_bool).unwrap_or(false);
    let story_target = p.get("storyTarget").cloned();
    let place = p.get("place").cloned().filter(Value::is_object);
    let timeout = Duration::from_secs(p.get("timeoutSeconds").and_then(Value::as_u64).unwrap_or(s.prefs.nl.queue_timeout_sec as u64).clamp(30, 7200));
    let (url_run, workflow_run) = (url.clone(), workflow.clone());
    let run = move || {
        let result = submit_and_download(&url_run, &workflow_run, &dir, timeout, &prog);
        *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(result.clone());
        *res.lock().unwrap_or_else(|e| e.into_inner()) = Some(match &result {
            Ok(_) => Ok(filmcraft_export::Report::default()),
            Err(e) => Err(e.clone()),
        });
        prog.finished.store(true, Ordering::Relaxed);
    };
    s.jobs.push(job);
    s.comfy_jobs.push(PendingComfy { job: id, files, epoch: s.document_epoch, story: story_target, place });
    if wait {
        s.nl.results.remove("comfy.last");
        run();
        poll(s);
        if !s.nl.results.contains_key("comfy.last") {
            let error = s
                .jobs
                .iter()
                .find(|job| job.id == id)
                .and_then(|job| job.result.lock().ok().and_then(|result| result.clone().and_then(|r| r.err())))
                .unwrap_or_else(|| "ComfyUI finished without importing a media file".to_string());
            return Err(bad(format!("ComfyUI: {error}")));
        }
    } else {
        std::thread::Builder::new()
            .name("filmcraft-comfy".into())
            .spawn(run)
            .map_err(|e| EngineError::Other(format!("can't start the ComfyUI thread: {e}")))?;
    }
    Ok(
        json!({"job": id, "workflow": workflow_path.to_string_lossy(), "url": url, "seed": seed, "prompt": prompt, "references": staged, "overridden": overridden, "place": p.get("place").cloned()}),
    )
}

/// Move one imported file into the active project's `generated/` folder and relink it. This is the
/// common post-import step for every workflow (Generate, music, cover, agents): the AppData cache
/// is only a download folder, never the place a project asset lives.
fn adopt_into_project(s: &mut Session, item: filmcraft_project::ItemId, source: &str) -> String {
    let Some(project) = s.path.as_deref() else { return source.to_string() };
    let Some(folder) = std::path::Path::new(project).parent().map(|parent| parent.join("generated")) else { return source.to_string() };
    if std::fs::create_dir_all(&folder).is_err() {
        return source.to_string();
    }
    let source_path = std::path::Path::new(source);
    if source_path.parent() == Some(folder.as_path()) {
        return source.to_string();
    }
    let Some(name) = source_path.file_name().and_then(|n| n.to_str()) else { return source.to_string() };
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect();
    let dest = folder.join(format!("nl-{}-{safe}", item.0));
    if dest.exists() && std::fs::remove_file(&dest).is_err() {
        return source.to_string();
    }
    let moved = std::fs::rename(source_path, &dest).or_else(|_| std::fs::copy(source_path, &dest).map(|_| ()).and_then(|_| std::fs::remove_file(source_path)));
    if moved.is_err() {
        return source.to_string();
    }
    let path = dest.to_string_lossy().to_string();
    let _ = s.execute("media.relink", json!({"item": item.0, "path": path, "force": true}));
    path
}

/// Import finished jobs; failures are reported without touching the project's media.
pub(crate) fn poll(s: &mut Session) {
    let mut i = 0;
    while i < s.comfy_jobs.len() {
        let job = s.jobs.iter().find(|j| j.id == s.comfy_jobs[i].job);
        let finished = job.is_none_or(|j| j.progress.finished.load(Ordering::Relaxed));
        let cancelled = job.is_some_and(|j| j.progress.cancel.load(Ordering::Relaxed));
        if !finished {
            i += 1;
            continue;
        }
        let pending = s.comfy_jobs.remove(i);
        if cancelled {
            continue;
        }
        let Some(result) = pending.files.lock().unwrap_or_else(|e| e.into_inner()).take() else { continue };
        if pending.epoch != s.document_epoch {
            s.error_toast("graphics.generateComfy", "the project was replaced; the generated media was downloaded but not imported");
            continue;
        }
        match result {
            Ok(paths) => {
                let mut imported = Vec::new();
                let mut errors = Vec::new();
                for path in paths {
                    match crate::commands::import_streamed(s, &path, None) {
                        Ok(id) => imported.push((id, adopt_into_project(s, id, &path))),
                        Err(e) => errors.push(format!("{path}: {e}")),
                    }
                }
                if let Some(target) = &pending.story
                    && let Some((id, path)) = imported.first()
                {
                    crate::nl::story::assign(s, target, *id, path);
                }
                // A generation started from the bridge or the text-to-video command can say where
                // its result belongs; placing it here keeps the job asynchronous and the UI free.
                if let Some(place) = &pending.place
                    && let Some((id, _)) = imported.first()
                {
                    let mut params = place.clone();
                    params["item"] = json!(id.0);
                    match s.execute("timeline.place", params) {
                        Ok(mut result) => {
                            if let Some(transition) = pending.place.as_ref().and_then(|place| place.get("transition")).and_then(Value::as_str) {
                                let applied = crate::nl::generative::apply_place_transitions(s, &result, transition);
                                result["transitions"] = json!(applied);
                            }
                            *s.nl.results.entry("comfy.last_place").or_insert(Value::Null) = result;
                            s.toast("ComfyUI: generated media placed on the timeline".to_string());
                        }
                        Err(e) => s.error_toast("graphics.generateComfy", format!("generated media imported but not placed: {e}")),
                    }
                }
                // Remember what was imported last so a synchronous caller (CLI, agent) can use it
                // without guessing item ids.
                if let Some((id, path)) = imported.first() {
                    *s.nl.results.entry("comfy.last").or_insert(Value::Null) = json!({"items": imported.iter().map(|(item, path)| json!({"item": item.0, "path": path})).collect::<Vec<_>>(), "first": {"item": id.0, "path": path}});
                }
                if !errors.is_empty() {
                    s.error_toast("graphics.generateComfy", format!("Imported {} file(s); {} failed: {}", imported.len(), errors.len(), errors.join(" | ")));
                } else if imported.is_empty() {
                    s.error_toast("graphics.generateComfy", "ComfyUI finished without a media file to import");
                } else {
                    s.toast(format!("ComfyUI: imported {} media item(s) into the project", imported.len()));
                }
            }
            Err(e) => s.error_toast("graphics.generateComfy", format!("ComfyUI: {e}")),
        }
    }
}

/// The workflow for this run: the `workflow` parameter (relative to the suite root or the
/// workflow folder), else the NL preference for the kind, else a bundled default (images and music
/// only — a video workflow is never guessed).
fn resolve_workflow(s: &Session, p: &Value) -> std::result::Result<PathBuf, EngineError> {
    if let Some(path) = p.get("workflow").and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty()) {
        return find_workflow(s, path).ok_or_else(|| bad(format!("workflow not found: {path}")));
    }
    let kind =
        p.get("kind").and_then(Value::as_str).unwrap_or(if crate::nl::size::Mode::from_params(p) == crate::nl::size::Mode::Video { "video" } else { "image" });
    let preferred = match kind {
        "video" => s.prefs.nl.comfy_video_workflow.trim(),
        "audio" => s.prefs.nl.comfy_music_workflow.trim(),
        _ => s.prefs.nl.comfy_image_workflow.trim(),
    };
    if !preferred.is_empty() {
        return find_workflow(s, preferred).ok_or_else(|| bad(format!("the {kind} workflow set in Settings ▸ NL Video Suite was not found: {preferred}")));
    }
    match kind {
        "audio" => {
            let root = suite_root();
            let path = root.join(DEFAULT_MUSIC_WORKFLOW);
            if path.is_file() { Ok(path) } else { Err(bad("no music workflow found; set one in Settings ▸ NL Video Suite or pass `workflow`")) }
        }
        "video" => Err(bad("no video workflow: set one in Settings ▸ NL Video Suite or pass `workflow`")),
        _ => {
            let root = suite_root();
            for relative in DEFAULT_WORKFLOWS {
                let path = root.join(relative);
                if path.is_file() {
                    return Ok(path);
                }
            }
            Err(bad("no default image workflow found under workflows/image; pass `workflow`"))
        }
    }
}

/// A workflow path from an absolute path, the suite root, or the workflow folder.
fn find_workflow(s: &Session, path: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        return candidate.is_file().then_some(candidate);
    }
    let mut candidates = vec![suite_root().join(&candidate)];
    if let Some(workflows) = crate::nl::models::root(s) {
        candidates.push(workflows.join(&candidate));
        if let Some(parent) = workflows.parent() {
            candidates.push(parent.join(&candidate));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn suite_root() -> PathBuf {
    if let Ok(root) = std::env::var("NL_SUITE_ROOT")
        && !root.trim().is_empty()
    {
        return PathBuf::from(root);
    }
    let mut starts = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        starts.push(parent.to_path_buf());
    }
    if let Ok(dir) = std::env::current_dir() {
        starts.push(dir);
    }
    for start in starts {
        for ancestor in start.ancestors() {
            if ancestor.join("workflows").is_dir() {
                return ancestor.to_path_buf();
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn generated_dir(job: u64) -> PathBuf {
    let base = crate::autosave::default_data_dir().unwrap_or_else(std::env::temp_dir);
    base.join("generated").join(format!("comfy-{job}"))
}

fn random_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64 & 0x7fff_ffff).unwrap_or(1)
}

fn replace_tokens(value: &mut Value, prompt: &str, negative: Option<&str>, lyrics: Option<&str>, seed: u64) -> bool {
    match value {
        Value::String(text) => {
            let before = text.clone();
            *text =
                text.replace("{{prompt}}", prompt).replace("{{PROMPT}}", prompt).replace("{{seed}}", &seed.to_string()).replace("{{SEED}}", &seed.to_string());
            if let Some(negative) = negative {
                *text = text.replace("{{negative}}", negative).replace("{{NEGATIVE}}", negative);
            }
            if let Some(lyrics) = lyrics {
                *text = text.replace("{{lyrics}}", lyrics).replace("{{LYRICS}}", lyrics);
            }
            before != *text
        }
        Value::Array(items) => {
            let mut changed = false;
            for item in items {
                changed |= replace_tokens(item, prompt, negative, lyrics, seed);
            }
            changed
        }
        Value::Object(map) => {
            let mut changed = false;
            for item in map.values_mut() {
                changed |= replace_tokens(item, prompt, negative, lyrics, seed);
            }
            changed
        }
        _ => false,
    }
}

fn fill_prompt(workflow: &mut Value, prompt: &str, negative: Option<&str>, lyrics: Option<&str>) -> bool {
    let Some(nodes) = workflow.as_object_mut() else { return false };
    let mut positive = false;
    let mut lyrics_filled = lyrics.is_none();
    for node in nodes.values_mut() {
        let title = node.get("_meta").and_then(|m| m.get("title")).and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default().to_string();
        let is_text_encode = class == "CLIPTextEncode" || class.contains("TextEncode");
        // Some workflows keep the prompt in a primitive string node titled "Prompt" (MiniMax H3 and
        // other graph styles) instead of a CLIPTextEncode.
        let is_string_prompt = class.contains("PrimitiveString") && (title.contains("prompt") || title.contains("positive"));
        let Some(inputs) = node.get_mut("inputs").and_then(Value::as_object_mut) else { continue };
        if !lyrics_filled
            && let Some(lyrics) = lyrics
            && let Some(Value::String(value)) = inputs.get_mut("lyrics")
        {
            *value = lyrics.to_string();
            lyrics_filled = true;
        }
        let is_negative = title.contains("negative");
        if title.contains("system prompt") || title.contains("system message") {
            continue;
        }
        if is_negative {
            if let Some(negative) = negative {
                for key in ["text", "value"] {
                    if let Some(Value::String(text)) = inputs.get_mut(key) {
                        *text = negative.to_string();
                        break;
                    }
                }
            }
            continue;
        }
        // Prompt-expander workflows keep the operator's text in a "User Prompt" primitive and
        // expand it through their own system prompt. Fill that node and leave the system prompt
        // alone: filling the system node is how the martini default survived every generation.
        if title.contains("user prompt") {
            for key in ["value", "text", "prompt"] {
                if let Some(Value::String(text)) = inputs.get_mut(key) {
                    *text = prompt.to_string();
                    positive = true;
                    break;
                }
            }
        }
        // MiniMax H3 (t2v / flf2v) keeps the prompt as a widget on its own node, not in a
        // CLIPTextEncode: a string there is the positive prompt.
        let is_h3_widget_prompt = (class.contains("MiniMaxH3") || class.contains("MinimaxH3")) && inputs.get("prompt").is_some_and(Value::is_string);
        let is_positive = title.contains("positive prompt")
            || title == "clip text encode (prompt)"
            || (is_text_encode && !is_negative)
            || is_string_prompt
            || is_h3_widget_prompt;
        if !is_positive {
            continue;
        }
        for key in ["text", "tags", "caption", "value", "prompt"] {
            if let Some(Value::String(text)) = inputs.get_mut(key) {
                *text = prompt.to_string();
                positive = true;
                break;
            }
        }
    }
    if !positive {
        // NanoBanana/Qwen edit graphs keep the operator instruction in a generic multiline
        // primitive with no "prompt" title. Fill that default, never a system instruction.
        for node in nodes.values_mut() {
            let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default();
            let title = node.get("_meta").and_then(|m| m.get("title")).and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
            if !class.contains("PrimitiveString") || title.contains("system") {
                continue;
            }
            if let Some(Value::String(value)) = node.get_mut("inputs").and_then(Value::as_object_mut).and_then(|inputs| inputs.get_mut("value")) {
                if value.trim_start().to_ascii_lowercase().starts_with("you are") {
                    continue;
                }
                *value = prompt.to_string();
                positive = true;
                break;
            }
        }
    }
    positive
}

/// Apply `overrides: [{node, field, value}]` to the workflow widgets. Returns how many were
/// applied; an unknown node, missing inputs or unknown field is an error, never ignored.
fn apply_overrides(workflow: &mut Value, overrides: Option<&Value>) -> Result<usize> {
    let Some(list) = overrides.and_then(Value::as_array) else { return Ok(0) };
    let Some(nodes) = workflow.as_object_mut() else { return Err(bad("the workflow is not an object")) };
    let mut applied = 0;
    for override_value in list {
        let optional = override_value.get("optional").and_then(Value::as_bool).unwrap_or(false);
        let Some(field) = override_value.get("field").and_then(Value::as_str).filter(|field| !field.is_empty()) else {
            return Err(bad("each override needs `field`"));
        };
        let Some(value) = override_value.get("value") else {
            return Err(bad("each override needs `value`"));
        };
        // A node id pins one widget; `class` (optionally with `title`) lets the same preset be
        // copied to another workflow where the node ids differ.
        let node_id = match override_value.get("node") {
            Some(Value::String(id)) if !id.is_empty() => Some(id.clone()),
            Some(Value::Number(id)) => Some(id.to_string()),
            Some(_) => return Err(bad("`node` must be a node id")),
            None => None,
        };
        let class = override_value.get("class").and_then(Value::as_str).filter(|class| !class.is_empty());
        let title = override_value.get("title").and_then(Value::as_str).filter(|title| !title.is_empty());
        let candidates: Vec<String> = if let Some(id) = node_id {
            vec![id]
        } else if let Some(class) = class {
            nodes
                .iter()
                .filter(|(_, node)| node.get("class_type").and_then(Value::as_str) == Some(class))
                .filter(|(_, node)| title.is_none_or(|title| node.get("_meta").and_then(|meta| meta.get("title")).and_then(Value::as_str) == Some(title)))
                .filter(|(_, node)| node.get("inputs").and_then(Value::as_object).is_some_and(|inputs| inputs.contains_key(field)))
                .map(|(id, _)| id.clone())
                .collect()
        } else {
            return Err(bad("each override needs `node` or `class`"));
        };
        let mut target = None;
        for id in &candidates {
            if nodes.get(id).and_then(|node| node.get("inputs")).and_then(Value::as_object).is_some_and(|inputs| inputs.contains_key(field)) {
                target = Some(id.clone());
                break;
            }
        }
        let Some(target) = target else {
            if optional {
                continue;
            }
            let hint = class.map(|class| format!(" (class {class})")).unwrap_or_default();
            return Err(bad(format!("override: no node with a `{field}` input{hint}")));
        };
        let Some(inputs) = nodes.get_mut(&target).and_then(|node| node.get_mut("inputs")).and_then(Value::as_object_mut) else {
            return Err(bad(format!("override: no node {target} with inputs")));
        };
        inputs.insert(field.to_string(), value.clone());
        applied += 1;
    }
    Ok(applied)
}

fn set_seed(workflow: &mut Value, seed: u64) {
    let Some(nodes) = workflow.as_object_mut() else { return };
    for node in nodes.values_mut() {
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default().to_string();
        if !class.contains("KSampler") && !class.contains("RandomNoise") {
            continue;
        }
        let Some(inputs) = node.get_mut("inputs").and_then(Value::as_object_mut) else { continue };
        for key in ["seed", "noise_seed"] {
            if inputs.get(key).is_some_and(Value::is_number) {
                inputs.insert(key.to_string(), json!(seed));
            }
        }
    }
}

fn set_duration(workflow: &mut Value, seconds: u64) {
    let Some(nodes) = workflow.as_object_mut() else { return };
    for node in nodes.values_mut() {
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default().to_string();
        let title = node.get("_meta").and_then(|meta| meta.get("title")).and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
        let Some(inputs) = node.get_mut("inputs").and_then(Value::as_object_mut) else { continue };
        // Music workflows take the duration on their audio nodes.
        if class.contains("AceStep") || class.contains("Music") || class.contains("Audio") {
            for key in ["duration", "max_duration", "seconds"] {
                if inputs.get(key).is_some_and(Value::is_number) {
                    inputs.insert(key.into(), json!(seconds));
                }
            }
        }
        // Video workflows keep it in a float node titled after the duration (H3 t2v/i2v, R2V).
        if class.contains("PrimitiveFloat") && title.contains("duration") && inputs.get("value").is_some_and(Value::is_number) {
            inputs.insert("value".into(), json!(seconds));
        }
    }
}

/// `audio.generateMusic` — the same ComfyUI bridge with an audio workflow (from Settings ▸ NL
/// Video Suite, else the bundled music default).
fn generate_music(s: &mut Session, p: &Value) -> Result<Value> {
    let mut params = p.clone();
    params["kind"] = json!("audio");
    generate(s, &params)
}

/// `graphics.generateCover` — a cover (thumbnail) guided by a frame of the open sequence and,
/// when no prompt is typed, by what is being said around that frame.
///
/// The frame is exported with the same command the Frame Export panel uses and uploaded to
/// ComfyUI's input folder; the workflow's image input (`LoadImage`) receives it by name, so a
/// text-to-image workflow is refused with a clear message instead of generating an unrelated
/// picture. The generated image is imported like any other ComfyUI output.
fn generate_cover(s: &mut Session, p: &Value) -> Result<Value> {
    let url = p.get("url").and_then(Value::as_str).map(str::trim).filter(|u| !u.is_empty()).map(str::to_string).unwrap_or_else(|| s.prefs.nl.comfy_url());
    let time = match p.get("time").and_then(Value::as_i64) {
        Some(ticks) => ticks,
        None => match p.get("timeSeconds").and_then(Value::as_f64) {
            Some(seconds) if seconds.is_finite() => filmcraft_time::Tick::from_seconds_f64(seconds.max(0.0)).0,
            _ => s.playhead().0,
        },
    };
    let prompt = match p.get("prompt").and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty()) {
        Some(prompt) => prompt.to_string(),
        None => cover_prompt(s, time).ok_or_else(|| bad("write a prompt for the cover, or transcribe the sequence so it can be derived from what is said"))?,
    };
    let workflow_path = resolve_workflow(s, p)?;
    let text = std::fs::read_to_string(&workflow_path).map_err(|e| bad(format!("{}: {e}", workflow_path.display())))?;
    let mut workflow: Value = serde_json::from_str(&text).map_err(|e| bad(format!("{}: not a ComfyUI API workflow: {e}", workflow_path.display())))?;
    if !workflow.is_object() {
        return Err(bad("the workflow must be a ComfyUI API-format JSON object"));
    }
    let seed = p.get("seed").and_then(Value::as_u64).unwrap_or_else(random_seed);
    let negative = p.get("negative").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let replaced = replace_tokens(&mut workflow, &prompt, negative, None, seed);
    let filled = fill_prompt(&mut workflow, &prompt, negative, None);
    if !replaced && !filled {
        return Err(bad("the workflow has no prompt slot: add the literal {{prompt}} to a text input or title a CLIPTextEncode node \"Positive Prompt\""));
    }
    set_seed(&mut workflow, seed);
    if let Some((width, height)) = crate::nl::size::from_params(s, &json!({"resolutionMode": "image"})) {
        set_size(&mut workflow, width, height);
    }
    // The chosen frame: exported to a PNG, uploaded, and named in the workflow's image input.
    let frame_path = cover_frame_path(s, time);
    s.execute("file.exportFrame", json!({"target": "program", "path": frame_path.to_string_lossy(), "time": time}))?;
    let uploaded = upload_image(&url, &frame_path)?;
    if !set_image_input(&mut workflow, &uploaded) {
        return Err(bad("the workflow has no image input (a LoadImage node): pick an edit or img2img workflow for covers"));
    }

    let id = s.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let label = format!("ComfyUI cover: {}", workflow_path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "workflow".into()));
    let job = crate::Job { id, label, progress: Default::default(), result: Default::default() };
    let (prog, res) = (job.progress.clone(), job.result.clone());
    let files: Arc<Mutex<Option<std::result::Result<Vec<String>, String>>>> = Arc::default();
    let out = files.clone();
    let dir = generated_dir(id);
    let wait = p.get("wait").and_then(Value::as_bool).unwrap_or(false);
    let story_target = p.get("storyTarget").cloned();
    let place = p.get("place").cloned().filter(Value::is_object);
    let timeout = Duration::from_secs(p.get("timeoutSeconds").and_then(Value::as_u64).unwrap_or(s.prefs.nl.queue_timeout_sec as u64).clamp(30, 7200));
    let (url_run, workflow_run) = (url.clone(), workflow.clone());
    let run = move || {
        let result = submit_and_download(&url_run, &workflow_run, &dir, timeout, &prog);
        *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(result.clone());
        *res.lock().unwrap_or_else(|e| e.into_inner()) = Some(match &result {
            Ok(_) => Ok(filmcraft_export::Report::default()),
            Err(e) => Err(e.clone()),
        });
        prog.finished.store(true, Ordering::Relaxed);
    };
    s.jobs.push(job);
    s.comfy_jobs.push(PendingComfy { job: id, files, epoch: s.document_epoch, story: story_target, place });
    if wait {
        s.nl.results.remove("comfy.last");
        run();
        poll(s);
        if !s.nl.results.contains_key("comfy.last") {
            let error = s
                .jobs
                .iter()
                .find(|job| job.id == id)
                .and_then(|job| job.result.lock().ok().and_then(|result| result.clone().and_then(|r| r.err())))
                .unwrap_or_else(|| "ComfyUI finished without importing a media file".to_string());
            return Err(bad(format!("ComfyUI: {error}")));
        }
    } else {
        std::thread::Builder::new()
            .name("filmcraft-comfy-cover".into())
            .spawn(run)
            .map_err(|e| EngineError::Other(format!("can't start the ComfyUI thread: {e}")))?;
    }
    Ok(json!({
        "job": id,
        "workflow": workflow_path.to_string_lossy(),
        "url": url,
        "seed": seed,
        "prompt": prompt,
        "time": time,
        "frame": frame_path.to_string_lossy(),
        "uploaded": uploaded,
    }))
}

/// `nl.cover.prompt` — the prompt the cover step would derive from the speech near a frame.
fn cover_prompt_query(s: &mut Session, p: &Value) -> Result<Value> {
    let time = match p.get("time").and_then(Value::as_i64) {
        Some(ticks) => ticks,
        None => match p.get("timeSeconds").and_then(Value::as_f64) {
            Some(seconds) if seconds.is_finite() => filmcraft_time::Tick::from_seconds_f64(seconds.max(0.0)).0,
            _ => s.playhead().0,
        },
    };
    Ok(json!({"prompt": cover_prompt(s, time), "time": time}))
}

/// A prompt from the words spoken shortly before the chosen frame, so a cover is about this video
/// and not about a generic subject.
fn cover_prompt(s: &Session, time: i64) -> Option<String> {
    let words = crate::transcript::sequence_words(s);
    if words.is_empty() {
        return None;
    }
    let mut spoken: Vec<String> =
        words.iter().filter(|w| w.start.0 <= time).rev().take(40).map(|w| w.text.trim().to_string()).filter(|t| !t.is_empty()).collect();
    spoken.reverse();
    if spoken.is_empty() {
        return None;
    }
    let mut text = spoken.join(" ");
    if text.chars().count() > 400 {
        text = text.chars().take(400).collect();
    }
    Some(format!("Cover image for a video about: {text}"))
}

/// The PNG the cover step exports, under the per-user data folder (never next to user media).
fn cover_frame_path(s: &Session, time: i64) -> PathBuf {
    let base = crate::autosave::default_data_dir().unwrap_or_else(std::env::temp_dir);
    let sequence = s.state.active_sequence.map(|id| id.0).unwrap_or(0);
    base.join("covers").join(format!("frame-{sequence}-{time}.png"))
}

/// Upload one file (image, video or audio) to ComfyUI's input folder; returns the name a loader
/// node should reference. `/upload/image` stores whatever bytes it is given, and the video/audio
/// loaders (VHS_LoadVideo, LoadAudioUI) resolve the returned name inside the input folder.
fn upload_file(url: &str, path: &Path) -> Result<String> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).filter(|n| !n.is_empty()).ok_or_else(|| bad("the reference has no file name"))?;
    let bytes = std::fs::read(path).map_err(|e| bad(format!("{}: {e}", path.display())))?;
    let content_type = match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("flac") => "audio/flac",
        _ => "application/octet-stream",
    };
    let boundary = format!("----NLBoundary{}", random_seed());
    let mut body = Vec::new();
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{name}\"\r\nContent-Type: {content_type}\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(&bytes);
    body.extend_from_slice(format!("\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"overwrite\"\r\n\r\ntrue\r\n--{boundary}--\r\n").as_bytes());
    let content_type = format!("multipart/form-data; boundary={boundary}");
    let base = url.trim_end_matches('/');
    let (status, response) = request_typed(&format!("{base}/upload/image"), "POST", Some(&body), &content_type, Duration::from_secs(300))
        .map_err(|e| bad(format!("ComfyUI upload: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(bad(format!("ComfyUI upload: HTTP {status}: {}", String::from_utf8_lossy(&response).trim().chars().take(300).collect::<String>())));
    }
    let value: Value = serde_json::from_slice(&response).map_err(|e| bad(format!("ComfyUI upload returned invalid JSON: {e}")))?;
    value.get("name").and_then(Value::as_str).map(str::to_string).filter(|n| !n.is_empty()).ok_or_else(|| bad("ComfyUI upload returned no file name"))
}

/// Upload one image (the cover path).
fn upload_image(url: &str, path: &Path) -> Result<String> {
    upload_file(url, path)
}

/// Upload the reference files a generation names (`image`/`images`, `video`, `audio`) and patch
/// the workflow's loaders. A workflow without the matching input is refused with a clear message
/// instead of generating something unrelated.
fn stage_references(url: &str, p: &Value, workflow: &mut Value) -> Result<Vec<Value>> {
    let mut staged = Vec::new();
    let mut images: Vec<String> = Vec::new();
    for role in ["character", "background"] {
        if let Some(path) = p.get(role).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
            images.push(path.to_string());
        }
    }
    if let Some(list) = p.get("images").and_then(Value::as_array) {
        images.extend(list.iter().filter_map(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string));
    }
    if let Some(path) = p.get("image").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
        images.push(path.to_string());
    }
    if !images.is_empty() {
        let mut names = Vec::new();
        for path in &images {
            let file = std::path::Path::new(path);
            if !file.is_file() {
                return Err(bad(format!("image reference not found: {path}")));
            }
            names.push(upload_file(url, file)?);
        }
        let patched = patch_ordered_references(workflow, "image", &names);
        if patched == 0 {
            return Err(bad("the workflow has no image input (LoadImage): pick an edit, img2img or reference workflow"));
        }
        if patched < names.len() {
            return Err(bad(format!(
                "the workflow has only {patched} image input(s) for {} reference(s): pick a workflow with enough inputs or pass fewer images",
                names.len()
            )));
        }
        staged.push(json!({"role": "image", "files": names, "patched": patched}));
    }
    for role in ["video", "audio"] {
        let mut files: Vec<String> = Vec::new();
        let plural = if role == "video" { "videos" } else { "audios" };
        if let Some(list) = p.get(plural).and_then(Value::as_array) {
            files.extend(list.iter().filter_map(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string));
        }
        if let Some(path) = p.get(role).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
            files.push(path.to_string());
        }
        if files.is_empty() {
            continue;
        }
        let mut names = Vec::new();
        for path in &files {
            let file = std::path::Path::new(path);
            if !file.is_file() {
                return Err(bad(format!("{role} reference not found: {path}")));
            }
            names.push(upload_file(url, file)?);
        }
        let patched = patch_ordered_references(workflow, role, &names);
        if patched == 0 {
            let expected = if role == "video" { "LoadVideo or VHS_LoadVideo" } else { "LoadAudio or LoadAudioUI" };
            return Err(bad(format!("the workflow has no {role} input ({expected}): pick a workflow that declares it")));
        }
        if patched < names.len() {
            return Err(bad(format!(
                "the workflow has only {patched} {role} input(s) for {} reference(s): pick a workflow with enough inputs or pass fewer files",
                names.len()
            )));
        }
        staged.push(json!({"role": role, "files": names, "patched": patched}));
    }
    if let Some(loras) = p.get("loras").and_then(Value::as_array) {
        let mut changed = Vec::new();
        for lora in loras {
            let Some(name) = lora.get("name").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) else { continue };
            let strength = lora.get("strength").and_then(Value::as_f64).unwrap_or(1.0);
            let wanted_node = lora.get("node").and_then(Value::as_str);
            let add_new = wanted_node == Some("0");
            let Some(nodes) = workflow.as_object_mut() else { break };
            let target = if add_new {
                None
            } else {
                nodes.iter_mut().find(|(id, node)| {
                    let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
                    class.contains("lora") && wanted_node.is_none_or(|wanted| wanted == *id)
                })
            };
            if let Some((id, node)) = target {
                let inputs = node.get_mut("inputs").and_then(Value::as_object_mut).ok_or_else(|| bad("the LoRA node has no inputs"))?;
                inputs.insert("lora_name".into(), json!(name));
                if inputs.contains_key("strength_model") {
                    inputs.insert("strength_model".into(), json!(strength));
                }
                if inputs.contains_key("strength_clip") {
                    inputs.insert("strength_clip".into(), json!(strength));
                }
                changed.push(json!({"node": id, "name": name, "strength": strength}));
                continue;
            }
            if wanted_node.is_some() && !add_new {
                return Err(bad(format!("no LoRA node `{}` in this workflow", wanted_node.unwrap_or_default())));
            }
            // The workflow has no free LoRA slot: insert a model-only LoRA between a MODEL
            // consumer and its source, in memory, for this run only.
            let Some((consumer_id, source)) = nodes.iter().find_map(|(id, node)| {
                let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
                if class.contains("lora") {
                    return None;
                }
                let link = node.get("inputs").and_then(Value::as_object).and_then(|inputs| inputs.get("model")).and_then(Value::as_array)?;
                (link.len() == 2 && link[0].is_string()).then(|| (id.clone(), link.clone()))
            }) else {
                return Err(bad(format!("no MODEL link to attach LoRA `{name}` in this workflow")));
            };
            let next = nodes.keys().filter_map(|key| key.rsplit(':').next().and_then(|part| part.parse::<u64>().ok())).max().unwrap_or(0) + 1;
            let new_id = next.to_string();
            nodes.insert(
                new_id.clone(),
                json!({"class_type": "LoraLoaderModelOnly", "inputs": {"model": source, "lora_name": name, "strength_model": strength}, "_meta": {"title": "LoRA (NL)"}}),
            );
            if let Some(consumer) = nodes.get_mut(&consumer_id)
                && let Some(inputs) = consumer.get_mut("inputs").and_then(Value::as_object_mut)
            {
                inputs.insert("model".into(), json!([new_id, 0]));
            }
            changed.push(json!({"node": new_id, "name": name, "strength": strength, "added": true}));
        }
        if !changed.is_empty() {
            staged.push(json!({"role": "lora", "items": changed}));
        }
    }
    Ok(staged)
}

/// Patch loader nodes of `role` in node-id order, so multi-reference workflows number their
/// subjects the way the file order says. Returns how many nodes were patched.
fn patch_ordered_references(workflow: &mut Value, role: &str, names: &[String]) -> usize {
    let Some(nodes) = workflow.as_object_mut() else { return 0 };
    let mut targets: Vec<String> = nodes
        .iter()
        .filter(|(_, node)| reference_matches(node.get("class_type").and_then(Value::as_str).unwrap_or_default(), role))
        .map(|(id, _)| id.clone())
        .collect();
    targets.sort_by(|a, b| match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        _ => a.cmp(b),
    });
    let mut patched = 0;
    for (id, name) in targets.iter().zip(names.iter()) {
        let Some(inputs) = nodes.get_mut(id).and_then(|node| node.get_mut("inputs")).and_then(Value::as_object_mut) else { continue };
        let key = match role {
            "image" => "image",
            "video" => {
                if inputs.contains_key("file") {
                    "file"
                } else {
                    "video"
                }
            }
            "audio" => {
                if inputs.contains_key("file") {
                    "file"
                } else {
                    "audio"
                }
            }
            _ => continue,
        };
        inputs.insert(key.to_string(), json!(name));
        patched += 1;
    }
    patched
}

/// Whether a loader node class belongs to a reference role.
fn reference_matches(class: &str, role: &str) -> bool {
    let class = class.to_ascii_lowercase();
    match role {
        "image" => class.contains("loadimage") || class.contains("load_image"),
        "video" => class.contains("loadvideo") || class.contains("load_video"),
        "audio" => class.contains("loadaudio") || class.contains("load_audio"),
        _ => false,
    }
}
/// Name `uploaded` in every `LoadImage` node; false when the workflow has none.
fn set_image_input(workflow: &mut Value, uploaded: &str) -> bool {
    let Some(nodes) = workflow.as_object_mut() else { return false };
    let mut changed = false;
    for node in nodes.values_mut() {
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default().to_string();
        if !class.eq_ignore_ascii_case("LoadImage") {
            continue;
        }
        if let Some(inputs) = node.get_mut("inputs").and_then(Value::as_object_mut) {
            inputs.insert("image".into(), json!(uploaded));
            changed = true;
        }
    }
    changed
}

fn set_size(workflow: &mut Value, width: u32, height: u32) {
    let Some(nodes) = workflow.as_object_mut() else { return };
    for node in nodes.values_mut() {
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default().to_string();
        let Some(inputs) = node.get_mut("inputs").and_then(Value::as_object_mut) else { continue };
        // A latent node may take width/height as *links* to a ResolutionSelector (Krea2/Z-image
        // workflows do). Replace the links with the sequence numbers: otherwise ComfyUI keeps the
        // selector's 16:9 default and a vertical project gets a landscape image.
        if inputs.contains_key("width") && inputs.contains_key("height") && (class.contains("Latent") || class.contains("Empty")) {
            inputs.insert("width".into(), json!(width));
            inputs.insert("height".into(), json!(height));
        }
        // MiniMax H3 video nodes often take width/height from a ResolutionSelector; replacing the
        // links with numbers follows the sequence size exactly (the selector stays for other runs).
        if class.contains("MiniMaxH3") || class.contains("MinimaxH3") {
            if inputs.contains_key("width") {
                inputs.insert("width".into(), json!(width));
            }
            if inputs.contains_key("height") {
                inputs.insert("height".into(), json!(height));
            }
        }
    }
}

fn submit_and_download(
    url: &str,
    workflow: &Value,
    dir: &Path,
    timeout: Duration,
    progress: &filmcraft_export::Progress,
) -> std::result::Result<Vec<String>, String> {
    let base = url.trim_end_matches('/');
    set_status(progress, "ComfyUI: invio del workflow");
    let submitted = post_json(&format!("{base}/prompt"), &json!({"prompt": workflow, "client_id": "filmcraft-nl"}))?;
    let prompt_id = submitted.get("prompt_id").and_then(Value::as_str).ok_or_else(|| format!("ComfyUI did not return prompt_id: {submitted}"))?.to_string();
    set_status(progress, &format!("ComfyUI: in coda · prompt {prompt_id}"));
    let start = web_time::Instant::now();
    let outputs = loop {
        if progress.cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        if start.elapsed() > timeout {
            return Err(format!("timeout after {} s; prompt {prompt_id} may still run in ComfyUI", timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_secs(1));
        let history = match get_json(&format!("{base}/history/{prompt_id}"), Duration::from_secs(30)) {
            Ok(value) => value,
            // Some builds answer 404 while the prompt is still queued.
            Err(error) if error.starts_with("HTTP 404") => continue,
            Err(error) => return Err(error),
        };
        let Some(entry) = history.get(&prompt_id) else { continue };
        if entry.pointer("/status/status_str").and_then(Value::as_str) == Some("error") {
            return Err(format!("ComfyUI error: {}", entry.get("status").cloned().unwrap_or(Value::Null)));
        }
        if let Some(outputs) = entry.get("outputs").and_then(Value::as_object)
            && !outputs.is_empty()
        {
            break outputs.clone();
        }
    };
    set_status(progress, "ComfyUI: download dei risultati");
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut paths = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for node in outputs.values() {
        for collection in node.as_object().into_iter().flat_map(|o| o.values()).filter_map(Value::as_array) {
            for file in collection {
                let Some(name) = file.get("filename").and_then(Value::as_str) else { continue };
                let Some(extension) = Path::new(name).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase) else { continue };
                if !MEDIA_EXTENSIONS.contains(&extension.as_str()) {
                    continue;
                }
                let subfolder = file.get("subfolder").and_then(Value::as_str).unwrap_or_default();
                let kind = file.get("type").and_then(Value::as_str).unwrap_or("output");
                if !seen.insert((name.to_string(), subfolder.to_string(), kind.to_string())) {
                    continue;
                }
                let safe = Path::new(name)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .filter(|n| !n.is_empty())
                    .ok_or_else(|| format!("unsafe output name: {name}"))?;
                let bytes = get_bytes(&format!("{base}/view?filename={}&subfolder={}&type={}", encode(name), encode(subfolder), encode(kind)))?;
                let destination = unique_path(dir, &safe);
                std::fs::write(&destination, &bytes).map_err(|e| format!("{}: {e}", destination.display()))?;
                paths.push(destination.to_string_lossy().to_string());
            }
        }
    }
    if paths.is_empty() {
        return Err("ComfyUI completed without a downloadable image/audio/video".into());
    }
    Ok(paths)
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    if !path.exists() {
        return path;
    }
    let stem = Path::new(name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "output".into());
    let extension = Path::new(name).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for n in 1..1000 {
        let candidate = dir.join(format!("{stem}-{n}{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-{}", random_seed()))
}

fn set_status(progress: &filmcraft_export::Progress, text: &str) {
    *progress.status.lock().unwrap_or_else(|e| e.into_inner()) = text.to_string();
}

fn post_json(url: &str, value: &Value) -> std::result::Result<Value, String> {
    let body = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    let (status, body) = request(url, "POST", Some(&body), Duration::from_secs(60))?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}: {}", String::from_utf8_lossy(&body).trim().chars().take(500).collect::<String>()));
    }
    serde_json::from_slice(&body).map_err(|e| format!("ComfyUI returned invalid JSON: {e}"))
}

fn get_json(url: &str, timeout: Duration) -> std::result::Result<Value, String> {
    let (status, body) = request(url, "GET", None, timeout)?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}: {}", String::from_utf8_lossy(&body).trim().chars().take(500).collect::<String>()));
    }
    serde_json::from_slice(&body).map_err(|e| format!("ComfyUI returned invalid JSON: {e}"))
}

fn get_bytes(url: &str) -> std::result::Result<Vec<u8>, String> {
    let (status, body) = request(url, "GET", None, Duration::from_secs(600))?;
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status} downloading {url}"));
    }
    Ok(body)
}

fn request(url: &str, method: &str, body: Option<&[u8]>, timeout: Duration) -> std::result::Result<(u16, Vec<u8>), String> {
    request_typed(url, method, body, "application/json", timeout)
}

/// A request with an explicit content type (the image upload is multipart, not JSON).
fn request_typed(url: &str, method: &str, body: Option<&[u8]>, content_type: &str, timeout: Duration) -> std::result::Result<(u16, Vec<u8>), String> {
    let rest = url.strip_prefix("http://").ok_or_else(|| "only http:// ComfyUI endpoints are supported".to_string())?;
    let (authority, path) = match rest.split_once('/') {
        Some((a, p)) => (a, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    let address = authority.to_socket_addrs().map_err(|e| format!("{authority}: {e}"))?.next().ok_or_else(|| format!("{authority} resolved to no address"))?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5)).map_err(|e| format!("connect {authority}: {e}"))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n");
    if let Some(body) = body {
        head.push_str(&format!("Content-Type: {content_type}\r\nContent-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).map_err(|e| format!("write: {e}"))?;
    if let Some(body) = body {
        stream.write_all(body).map_err(|e| format!("write: {e}"))?;
    }
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| format!("read: {e}"))?;
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| "malformed HTTP response".to_string())?;
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let status =
        head.lines().next().and_then(|l| l.split_whitespace().nth(1)).and_then(|s| s.parse::<u16>().ok()).ok_or_else(|| "malformed HTTP status".to_string())?;
    let body = &raw[split + 4..];
    let body = if head.to_ascii_lowercase().contains("transfer-encoding: chunked") { dechunk(body)? } else { body.to_vec() };
    Ok((status, body))
}

fn dechunk(mut body: &[u8]) -> std::result::Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let line = body.windows(2).position(|w| w == b"\r\n").ok_or_else(|| "malformed chunked response".to_string())?;
        let size = usize::from_str_radix(String::from_utf8_lossy(&body[..line]).trim().split(';').next().unwrap_or("0").trim(), 16)
            .map_err(|_| "bad chunk size".to_string())?;
        body = &body[line + 2..];
        if size == 0 {
            return Ok(out);
        }
        if body.len() < size + 2 {
            return Err("truncated chunked response".into());
        }
        out.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub(crate) fn commands() -> Vec<crate::CommandSpec> {
    vec![
        crate::CommandSpec {
            id: "graphics.generateComfy",
            label: "Generate with ComfyUI…",
            menu: &["Graphics and Titles"],
            shortcut: None,
            params: r#"{"prompt":str,"workflow":str?,"negative":str?,"lyrics":str?,"seed":n?,"width":n?,"height":n?,"resolutionMode":"image|video"?,"imageScale":f?,"videoScale":"2/3|1/2|1/3"?,"character":str?,"background":str?,"image":str?,"images":[str]?,"video":str?,"videos":[str]?,"audio":str?,"audios":[str]?,"loras":[{"node":str?,"name":str,"strength":f?}]?,"overrides":[{"node":str,"field":str,"value":any}]?,"dryRun":bool?,"url":str?,"wait":bool?,"timeoutSeconds":n?}"#,
            enabled: crate::commands::always,
            run: generate,
            journal: false,
            named: crate::commands::NamedKeys::NONE,
        },
        crate::CommandSpec {
            id: "audio.generateMusic",
            label: "Generate Music with ComfyUI…",
            menu: &["Clip"],
            shortcut: None,
            params: r#"{"prompt":str,"lyrics":str?,"workflow":str?,"seconds":n?,"seed":n?,"url":str?,"wait":bool?,"timeoutSeconds":n?}"#,
            enabled: crate::commands::always,
            run: generate_music,
            journal: false,
            named: crate::commands::NamedKeys::NONE,
        },
        crate::CommandSpec {
            id: "graphics.generateCover",
            label: "Generate Cover from Frame…",
            menu: &["Graphics and Titles"],
            shortcut: None,
            params: r#"{"prompt":str?,"time":ticks?,"timeSeconds":f?,"workflow":str?,"negative":str?,"seed":n?,"url":str?,"wait":bool?,"timeoutSeconds":n?}"#,
            enabled: crate::commands::always,
            run: generate_cover,
            journal: false,
            named: crate::commands::NamedKeys::NONE,
        },
        crate::CommandSpec {
            id: "nl.cover.prompt",
            label: "NL Cover Prompt",
            menu: &[],
            shortcut: None,
            params: r#"{"time":ticks?,"timeSeconds":f?}"#,
            enabled: crate::commands::always,
            run: cover_prompt_query,
            journal: false,
            named: crate::commands::NamedKeys::NONE,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_the_image_prompt_and_keeps_the_negative() {
        let mut workflow = json!({
            "1": {"class_type": "CLIPTextEncode", "_meta": {"title": "CLIP Text Encode (Positive Prompt)"}, "inputs": {"text": "old"}},
            "2": {"class_type": "CLIPTextEncode", "_meta": {"title": "CLIP Text Encode (Negative Prompt)"}, "inputs": {"text": "old"}}
        });
        assert!(fill_prompt(&mut workflow, "a new shot", Some("blurry"), None));
        assert_eq!(workflow["1"]["inputs"]["text"], "a new shot");
        assert_eq!(workflow["2"]["inputs"]["text"], "blurry");
    }

    #[test]
    fn a_prompt_expander_keeps_its_system_node_and_replaces_the_user_prompt() {
        let mut workflow = json!({
            "1": {"class_type": "PrimitiveStringMultiline", "_meta": {"title": "Text String (System Prompt)"}, "inputs": {"value": "expand the prompt"}},
            "2": {"class_type": "PrimitiveStringMultiline", "_meta": {"title": "Text String (User Prompt)"}, "inputs": {"value": "a martini glass"}},
            "3": {"class_type": "CLIPTextEncode", "_meta": {"title": "CLIP Text Encode (Prompt)"}, "inputs": {"text": ["4", 0]}}
        });
        assert!(fill_prompt(&mut workflow, "detective in neon rain", None, None));
        assert_eq!(workflow["1"]["inputs"]["value"], "expand the prompt");
        assert_eq!(workflow["2"]["inputs"]["value"], "detective in neon rain");
    }

    #[test]
    fn a_linked_latent_size_follows_the_sequence() {
        let mut workflow = json!({
            "1": {"class_type": "EmptyLatentImage", "inputs": {"width": ["2", 0], "height": ["2", 1], "batch_size": 1}},
            "2": {"class_type": "ResolutionSelector", "inputs": {"aspect_ratio": "16:9 (Widescreen)", "megapixels": 1}}
        });
        set_size(&mut workflow, 1080, 1920);
        assert_eq!(workflow["1"]["inputs"]["width"], 1080);
        assert_eq!(workflow["1"]["inputs"]["height"], 1920);
    }

    #[test]
    fn fills_audio_caption_tags_and_lyrics() {
        let mut minimax = json!({
            "1": {"class_type": "MiniMaxMusic3TextEncode", "_meta": {"title": "MiniMax Music3 Text Encode"}, "inputs": {"caption": "old", "lyrics": "old"}}
        });
        assert!(fill_prompt(&mut minimax, "warm soul", None, Some("[Verse]\nla la")));
        assert_eq!(minimax["1"]["inputs"]["caption"], "warm soul");
        assert_eq!(minimax["1"]["inputs"]["lyrics"], "[Verse]\nla la");

        let mut ace = json!({
            "1": {"class_type": "TextEncodeAceStepAudio1.5", "_meta": {"title": "TextEncodeAceStepAudio1.5"}, "inputs": {"tags": "old"}}
        });
        assert!(fill_prompt(&mut ace, "lo-fi drums", None, None));
        assert_eq!(ace["1"]["inputs"]["tags"], "lo-fi drums");
    }

    #[test]
    fn replaces_tokens_in_every_nested_string() {
        let mut workflow = json!({"a": ["{{prompt}}", {"b": "{{seed}}"}], "c": "{{lyrics}}"});
        assert!(replace_tokens(&mut workflow, "hello", Some("no"), Some("la"), 42));
        assert_eq!(workflow["a"][0], "hello");
        assert_eq!(workflow["a"][1]["b"], "42");
        assert_eq!(workflow["c"], "la");
    }

    #[test]
    fn image_and_video_sizes_follow_the_sequence_and_the_nl_preferences() {
        let mut session = crate::Session::default();
        session.execute("file.openDemoProject", json!({})).unwrap();
        let (w, h) = session.active_sequence().map(|q| (q.settings.width, q.settings.height)).unwrap();
        let step = session.prefs.nl.size_step();
        let round = |value: f64| crate::nl::size::round_to(value, step);
        assert_eq!(crate::nl::size::from_params(&session, &json!({"resolutionMode": "image"})), Some((round(w as f64), round(h as f64))));
        assert_eq!(
            crate::nl::size::from_params(&session, &json!({"resolutionMode": "image", "imageScale": 2})),
            Some((round(w as f64 * 2.0), round(h as f64 * 2.0)))
        );
        assert_eq!(
            crate::nl::size::from_params(&session, &json!({"resolutionMode": "video", "videoScale": "1/2"})),
            Some((round(w as f64 / 2.0), round(h as f64 / 2.0)))
        );
        session.prefs.nl.video_scale = "1/2".into();
        assert_eq!(
            crate::nl::size::from_params(&session, &json!({"resolutionMode": "video"})),
            Some((round(w as f64 / 2.0), round(h as f64 / 2.0))),
            "the NL preference applies when the dialog passes no override"
        );
    }

    #[test]
    fn changes_only_the_sampler_seed() {
        let mut workflow = json!({
            "1": {"class_type": "KSampler", "inputs": {"seed": 1}},
            "2": {"class_type": "EmptyLatentImage", "inputs": {"width": 512, "height": 512}}
        });
        set_seed(&mut workflow, 99);
        assert_eq!(workflow["1"]["inputs"]["seed"], 99);
        assert_eq!(workflow["2"]["inputs"]["width"], 512);
    }
}

#[cfg(test)]
mod cover_tests {
    use super::*;

    #[test]
    fn reference_nodes_are_patched_in_node_id_order() {
        let mut workflow = json!({
            "10": {"class_type": "LoadImage", "inputs": {"image": "old.png"}},
            "2": {"class_type": "LoadImage", "inputs": {"image": "old.png"}},
            "3": {"class_type": "KSampler", "inputs": {"seed": 1}}
        });
        assert_eq!(patch_ordered_references(&mut workflow, "image", &["first.png".into(), "second.png".into()]), 2);
        assert_eq!(workflow["2"]["inputs"]["image"], "first.png");
        assert_eq!(workflow["10"]["inputs"]["image"], "second.png");
        assert_eq!(workflow["3"]["inputs"]["seed"], 1);
    }

    #[test]
    fn video_and_audio_loaders_get_their_own_input_key() {
        let mut workflow = json!({
            "1": {"class_type": "VHS_LoadVideo", "inputs": {"video": "old.mp4"}},
            "2": {"class_type": "LoadVideo", "inputs": {"file": "old.mp4"}},
            "3": {"class_type": "LoadAudioUI", "inputs": {"audio": "old.wav"}},
            "4": {"class_type": "LoadAudio", "inputs": {"file": "old.wav"}}
        });
        // One video reference fills the first video loader in node-id order and leaves the other.
        assert_eq!(patch_ordered_references(&mut workflow, "video", &["new.mp4".into()]), 1);
        assert_eq!(workflow["1"]["inputs"]["video"], "new.mp4");
        assert_eq!(workflow["2"]["inputs"]["file"], "old.mp4");
        assert_eq!(patch_ordered_references(&mut workflow, "audio", &["new.wav".into()]), 1);
        assert_eq!(workflow["3"]["inputs"]["audio"], "new.wav");
        assert_eq!(workflow["4"]["inputs"]["file"], "old.wav");
    }

    #[test]
    fn a_workflow_without_the_input_reports_zero_patches() {
        let mut workflow = json!({"1": {"class_type": "SaveImage", "inputs": {}}});
        assert_eq!(patch_ordered_references(&mut workflow, "video", &["x.mp4".into()]), 0);
        assert_eq!(patch_ordered_references(&mut workflow, "image", &["x.png".into()]), 0);
    }

    #[test]
    fn the_cover_names_the_uploaded_frame_in_every_load_image() {
        let mut workflow = json!({
            "1": {"class_type": "LoadImage", "inputs": {"image": "old.png"}},
            "2": {"class_type": "KSampler", "inputs": {"seed": 1}}
        });
        assert!(set_image_input(&mut workflow, "uploaded.png"));
        assert_eq!(workflow["1"]["inputs"]["image"], "uploaded.png");
        assert_eq!(workflow["2"]["inputs"]["seed"], 1, "other nodes are untouched");
    }

    #[test]
    fn a_text_to_image_workflow_is_not_a_cover_workflow() {
        let mut workflow = json!({"1": {"class_type": "EmptyLatentImage", "inputs": {"width": 1024}}});
        assert!(!set_image_input(&mut workflow, "uploaded.png"));
    }

    #[test]
    fn overrides_change_widgets_and_unknown_ones_are_refused() {
        let mut workflow = json!({
            "1": {"class_type": "KSampler", "inputs": {"steps": 20, "cfg": 3.5}},
            "2": {"class_type": "SaveImage", "inputs": {}}
        });
        let applied = apply_overrides(
            &mut workflow,
            Some(&json!([
                {"node": "1", "field": "steps", "value": 8},
                {"node": 2, "field": "filename_prefix", "value": "x"}
            ])),
        );
        // the second override targets a field SaveImage does not declare: the whole call is refused
        assert!(applied.is_err(), "unknown field must be refused");
        let applied = apply_overrides(&mut workflow, Some(&json!([{"node": "1", "field": "steps", "value": 8}]))).unwrap();
        assert_eq!(applied, 1);
        assert_eq!(workflow["1"]["inputs"]["steps"], 8);
        assert!(apply_overrides(&mut workflow, Some(&json!([{"node": "9", "field": "steps", "value": 1}]))).is_err());
        // A preset can name the class instead of the id, so it is reusable on another workflow.
        assert!(apply_overrides(&mut workflow, Some(&json!([{"class": "KSampler", "field": "steps", "value": 12}]))).unwrap() == 1);
        assert_eq!(workflow["1"]["inputs"]["steps"], 12);
        assert!(apply_overrides(&mut workflow, Some(&json!([{"class": "LoraLoader", "field": "lora_name", "value": "x", "optional": true}]))).unwrap() == 0);
        assert!(apply_overrides(&mut workflow, Some(&json!([{"class": "LoraLoader", "field": "lora_name", "value": "x"}]))).is_err());
        assert!(apply_overrides(&mut workflow, None).unwrap() == 0);
    }

    #[test]
    fn the_cover_needs_a_prompt_or_speech() {
        let mut s = crate::Session::default();
        let e = s.execute("graphics.generateCover", json!({})).unwrap_err().to_string();
        assert!(e.contains("write a prompt"), "{e}");
    }
}
