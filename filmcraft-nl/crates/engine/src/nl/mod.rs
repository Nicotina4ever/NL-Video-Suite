//! The NL Video Suite side of the engine: the settings-derived size policy, the local model and
//! workflow inventory, and the diagnostics the NL panels show.
//!
//! The pieces that generate media or read media live beside their engine feature (`comfy.rs`,
//! `nl_asr.rs`, `shorts.rs`); this module holds what those features share and the commands the
//! NL panels call. Nothing here downloads a model or starts a service by itself.

pub mod agent;
pub mod album;
pub mod analysis;
pub mod asr_models;
pub mod assistant;
pub mod delivery;
pub mod diagnostics;
pub mod generative;
pub mod graphics;
pub mod http;
pub mod llm;
pub mod longform;
pub mod maintenance;
pub mod models;
pub mod montage;
pub mod pipeline;
pub mod prompt;
pub mod queue;
pub mod reframe;
pub mod search;
pub mod size;
pub mod state;
pub mod story;
pub mod subtitles;
pub mod titles;
pub mod translate;
pub mod versions;
pub mod vision;
pub mod workflow_schema;
pub mod workflows;

use std::path::PathBuf;
use std::time::Duration;

use filmcraft_project::{ItemId, ItemKind, MediaRef};
use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{Result, Session};

/// The media a command works on: the named item, else the Project panel selection, else the audio
/// clip under the playhead. Shared by every NL step that reads one file.
pub(crate) fn target_media(s: &Session, p: &Value) -> Option<(ItemId, String)> {
    let mut raw = p.get("item").and_then(Value::as_u64).map(ItemId).into_iter().collect::<Vec<_>>();
    if raw.is_empty() {
        raw = s.state.project_selection.clone();
    }
    if raw.is_empty()
        && let Some(q) = s.active_sequence()
    {
        // Timeline-first: the open sequence is the source. Prefer the picture, then the sound, so
        // "Find candidates", rough cut, ASR and transcription work on what the operator is
        // already editing instead of asking for another file.
        raw = q
            .video_tracks
            .iter()
            .flat_map(|t| t.items.iter())
            .filter(|i| i.enabled)
            .map(|i| i.item)
            .chain(q.audio_tracks.iter().flat_map(|t| t.items.iter()).filter(|i| i.enabled).map(|i| i.item))
            .collect();
    }
    for id in raw {
        // A selected sequence clip resolves to the media it plays; a selected Project-panel item
        // already is that media.
        let media = s.project.resolve_media(id).map(|(media, _, _)| media).or_else(|| match s.project.item(id).map(|i| &i.kind) {
            Some(ItemKind::Media(_)) => Some(id),
            _ => None,
        });
        let Some(media) = media else { continue };
        let Some(item) = s.project.item(media) else { continue };
        let ItemKind::Media(clip) = &item.kind else { continue };
        let MediaRef::File { path } = &clip.media else { continue };
        return Some((media, path.clone()));
    }
    None
}

/// The work folder of an analysis for one media: stable per file so a transcript cached by the
/// analysis is reused by the render step (which must not transcribe a second time).
pub(crate) fn media_work_dir(s: &Session, media: &str, purpose: &str) -> PathBuf {
    let configured = s.prefs.nl.asr_cache_dir.trim();
    let base = if configured.is_empty() {
        crate::autosave::default_data_dir().unwrap_or_else(std::env::temp_dir)
    } else {
        let path = PathBuf::from(configured);
        if path.is_absolute() { path } else { crate::nl::suite_root().unwrap_or_else(|| PathBuf::from(".")).join(path) }
    };
    base.join("nl").join(purpose).join(fingerprint(media))
}

/// A short stable fingerprint of a path (FNV-1a), so work folders do not leak full paths.
pub(crate) fn fingerprint(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn spec(id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>) -> CommandSpec {
    CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: always, run, journal: false, named: crate::commands::NamedKeys::NONE }
}

/// `nl.models.workflows` — the workflow JSON files the operator can pick, by kind.
fn workflows(s: &mut Session, p: &Value) -> Result<Value> {
    let kind = p.get("kind").and_then(Value::as_str).unwrap_or("all");
    let list = models::workflows(s, kind).map_err(|e| bad("nl.models.workflows", e))?;
    let selected = models::selected_workflows(s);
    Ok(json!({"workflows": list, "selected": selected}))
}

/// `nl.models.ollama` — the models the local Ollama already has (never pulls).
fn ollama_models(s: &mut Session, _: &Value) -> Result<Value> {
    let url = s.prefs.nl.ollama_url();
    match llm::models(&url) {
        Ok(list) => Ok(json!({"url": url, "reachable": true, "models": list})),
        Err(e) => Ok(json!({"url": url, "reachable": false, "error": e, "models": []})),
    }
}

/// `nl.models.llm` — the models of the configured provider (Ollama or OpenAI-compatible).
fn llm_models(s: &mut Session, _: &Value) -> Result<Value> {
    let provider = s.prefs.nl.llm_provider();
    let url = s.prefs.nl.llm_url();
    match llm::models_for(s) {
        Ok((url, list)) => Ok(json!({"provider": provider, "url": url, "reachable": true, "models": list})),
        Err(e) => Ok(json!({"provider": provider, "url": url, "reachable": false, "error": e, "models": []})),
    }
}

/// `nl.services.status` — whether the local AI services answer, without starting them.
fn services_status(s: &mut Session, _: &Value) -> Result<Value> {
    Ok(diagnostics::services(s))
}

/// `nl.diagnostics` — services, paths, models and tools in one report (Aiuto ▸ Diagnostics).
fn diagnostics(s: &mut Session, _: &Value) -> Result<Value> {
    Ok(diagnostics::report(s))
}

/// `nl.size.preview` — the size a generation would use for the open sequence, without generating.
fn size_preview(s: &mut Session, p: &Value) -> Result<Value> {
    let size = size::from_params(s, p).ok_or_else(|| bad("nl.size.preview", "open a sequence first: generated sizes come from it"))?;
    Ok(json!({
        "width": size.0,
        "height": size.1,
        "mode": if size::Mode::from_params(p) == size::Mode::Video { "video" } else { "image" },
        "multiple": s.prefs.nl.size_step(),
        "imageScale": s.prefs.nl.image_scale,
        "videoScale": s.prefs.nl.video_scale,
    }))
}

/// `nl.clipping.propose` — the editorial candidates for the selected video (no render yet).
fn clipping_propose(s: &mut Session, p: &Value) -> Result<Value> {
    let mode = p.get("mode").and_then(Value::as_str).unwrap_or("2").to_string();
    if !matches!(mode.as_str(), "1" | "2" | "3") {
        return Err(bad("nl.clipping.propose", "mode must be 1 (stories), 2 (reels) or 3 (narrative reel)"));
    }
    let (_, media) = target_media(s, p).ok_or_else(|| bad("nl.clipping.propose", "select a video in the Project panel or put one on the timeline"))?;
    let language = language(s, p, "nl.clipping.propose")?;
    let params = p.get("params").filter(|v| v.is_object()).cloned().unwrap_or_else(|| json!({}));
    let call = pipeline::BridgeCall {
        operation: if mode == "3" { "reels" } else { "clips" },
        media,
        language,
        params,
        items: None,
        mode: Some(mode.clone()),
        vertical: p.get("vertical").and_then(Value::as_bool).unwrap_or(true),
    };
    pipeline::start(s, format!("NL clipping (mode {mode})"), call)
}

/// `nl.clipping.render` — render only the selected candidates into the project.
fn clipping_render(s: &mut Session, p: &Value) -> Result<Value> {
    let (_, media) = target_media(s, p).ok_or_else(|| bad("nl.clipping.render", "select the source video in the Project panel"))?;
    let items = p
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .filter(|items| !items.is_empty())
        .ok_or_else(|| bad("nl.clipping.render", "pass the selected candidates in `items` (start, end, title, and segments for a narrative reel)"))?;
    let reels = items.iter().any(|item| item.get("segments").is_some());
    let language = language(s, p, "nl.clipping.render")?;
    let params = p.get("params").filter(|v| v.is_object()).cloned().unwrap_or_else(|| json!({}));
    let call = pipeline::BridgeCall {
        operation: if reels { "render_reels" } else { "render_clips" },
        media,
        language,
        params,
        items: Some(Value::Array(items)),
        mode: None,
        vertical: p.get("vertical").and_then(Value::as_bool).unwrap_or(true),
    };
    pipeline::start(s, "NL clipping render".into(), call)
}

/// `nl.sgrossa.analyze` — the cut plan (silences, breaths, fillers, retakes) without rendering.
fn sgrossa_analyze(s: &mut Session, p: &Value) -> Result<Value> {
    let (_, media) = target_media(s, p).ok_or_else(|| bad("nl.sgrossa.analyze", "select a video in the Project panel or put one on the timeline"))?;
    let language = language(s, p, "nl.sgrossa.analyze")?;
    let params = p.get("params").filter(|v| v.is_object()).cloned().unwrap_or_else(|| json!({}));
    let call = pipeline::BridgeCall { operation: "cleanup", media, language, params, items: None, mode: None, vertical: false };
    pipeline::start(s, "NL rough cut analysis".into(), call)
}

/// `nl.sgrossa.apply` — render the chosen good intervals and import them in order.
fn sgrossa_apply(s: &mut Session, p: &Value) -> Result<Value> {
    let (_, media) = target_media(s, p).ok_or_else(|| bad("nl.sgrossa.apply", "select the source video in the Project panel"))?;
    let items = p
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .filter(|items| !items.is_empty())
        .ok_or_else(|| bad("nl.sgrossa.apply", "pass the intervals to keep in `items`: [[start, end], …]"))?;
    let language = language(s, p, "nl.sgrossa.apply")?;
    let call =
        pipeline::BridgeCall { operation: "render_cleanup", media, language, params: json!({}), items: Some(Value::Array(items)), mode: None, vertical: false };
    pipeline::start(s, "NL rough cut render".into(), call)
}

/// `nl.pipeline.result` — the last result of one operation, for panels and agents.
fn pipeline_result(s: &mut Session, p: &Value) -> Result<Value> {
    let operation = p.get("operation").and_then(Value::as_str);
    match operation {
        Some(operation) => match s.nl.result(operation) {
            Some(value) => Ok(json!({"operation": operation, "result": value})),
            None => Err(bad("nl.pipeline.result", format!("no {operation} result yet: run that step first"))),
        },
        None => Ok(json!({"results": s.nl.results, "files": s.nl.files, "cover": s.nl.cover})),
    }
}

/// `nl.sgrossa.report` — write the reviewed cut plan next to the project (or at `path`).
fn sgrossa_report(s: &mut Session, p: &Value) -> Result<Value> {
    let plan = s.nl.result("cleanup").cloned().ok_or_else(|| bad("nl.sgrossa.report", "run the rough cut analysis first: there is no plan to report"))?;
    let path = match p.get("path").and_then(Value::as_str).filter(|p| !p.trim().is_empty()) {
        Some(path) => PathBuf::from(path),
        None => {
            let dir = s
                .path
                .as_deref()
                .and_then(|p| std::path::Path::new(p).parent().map(|p| p.to_path_buf()))
                .ok_or_else(|| bad("nl.sgrossa.report", "save the project first or pass `path` for the report"))?;
            dir.join("sgrossa_report.json")
        }
    };
    let report = json!({
        "generatedUnixtime": crate::autosave::unix_now(),
        "app": {"name": "NL Video Suite", "version": env!("CARGO_PKG_VERSION")},
        "plan": plan,
    });
    let text = serde_json::to_vec_pretty(&report).map_err(|e| bad("nl.sgrossa.report", e.to_string()))?;
    std::fs::write(&path, text).map_err(|e| bad("nl.sgrossa.report", format!("{}: {e}", path.display())))?;
    Ok(json!({
        "path": path.to_string_lossy(),
        "cuts": plan["cuts"].as_array().map_or(0, Vec::len),
        "pieces": plan["good"].as_array().map_or(0, Vec::len),
    }))
}

/// The transcript of a media item: stored under the media, or under a sequence clip that reads it.
fn transcript_for(s: &Session, item: ItemId) -> Option<std::sync::Arc<filmcraft_project::Transcript>> {
    if let Some(transcript) = s.project.transcripts.get(&item) {
        return Some(transcript.clone());
    }
    let q = s.active_sequence()?;
    for track in q.video_tracks.iter().chain(q.audio_tracks.iter()) {
        for clip in &track.items {
            if s.project.resolve_media(clip.item).map(|(media, _, _)| media) == Some(item)
                && let Some(transcript) = s.project.transcripts.get(&clip.item)
            {
                return Some(transcript.clone());
            }
        }
    }
    None
}

/// `nl.cover.set` — mark a generated image as the project's cover and keep the reference.
///
/// The cover is the operator's choice after reviewing the generated candidates; this command only
/// records it (the image itself already lives in the project's media). When the project is saved
/// the reference lands in `nl/cover.json` beside it, so the publishing step and later sessions can
/// find it; an unsaved project keeps it in the session and says so.
fn cover_set(s: &mut Session, p: &Value) -> Result<Value> {
    let mut candidates = p.get("item").and_then(Value::as_u64).map(ItemId).into_iter().collect::<Vec<_>>();
    if candidates.is_empty() {
        candidates = s.state.project_selection.clone();
    }
    let item = candidates
        .into_iter()
        .find(|id| s.project.item(*id).is_some_and(|i| matches!(i.kind, ItemKind::Media(_))))
        .ok_or_else(|| bad("nl.cover.set", "select the generated cover in the Project panel: the cover must be an image already in the project"))?;
    let path = s.project.item(item).and_then(|i| match &i.kind {
        ItemKind::Media(clip) => match &clip.media {
            MediaRef::File { path } => Some(path.clone()),
            _ => None,
        },
        _ => None,
    });
    let record = json!({
        "item": item.0,
        "path": path,
        "setUnixtime": crate::autosave::unix_now(),
    });
    s.nl.cover = Some(record.clone());
    let mut saved = false;
    if let Some(project) = s.path.as_deref().and_then(|p| std::path::Path::new(p).parent().map(|d| d.to_path_buf())) {
        let dir = project.join("nl");
        std::fs::create_dir_all(&dir).map_err(|e| bad("nl.cover.set", format!("{}: {e}", dir.display())))?;
        let file = dir.join("cover.json");
        let text = serde_json::to_vec_pretty(&record).map_err(|e| bad("nl.cover.set", e.to_string()))?;
        std::fs::write(&file, text).map_err(|e| bad("nl.cover.set", format!("{}: {e}", file.display())))?;
        saved = true;
    }
    s.toast("NL: cover reference set".to_string());
    Ok(json!({"cover": record, "saved": saved}))
}

/// `nl.workflows.list` — the workflows with their kind and inputs, for the pickers.
fn workflows_list(s: &mut Session, p: &Value) -> Result<Value> {
    let kind = p.get("kind").and_then(Value::as_str).unwrap_or("all");
    let list = models::workflows(s, kind).map_err(|e| bad("nl.workflows.list", e))?;
    Ok(json!({
        "workflows": list,
        "selected": models::selected_workflows(s),
        "root": models::root(s).map(|p| p.to_string_lossy().to_string()),
        "apiVerified": s.nl.object_info.is_some(),
    }))
}

/// `nl.workflows.verify` — read ComfyUI's `/object_info` and check the installed node classes.
fn workflows_verify(s: &mut Session, _: &Value) -> Result<Value> {
    let url = s.prefs.nl.comfy_url();
    let info = http::get_json(&url, "object_info", Duration::from_secs(120)).map_err(|e| bad("nl.workflows.verify", format!("ComfyUI {url}: {e}")))?;
    let report = workflows::verify(s, &info);
    let classes = report["classes"].as_u64().unwrap_or(0);
    s.nl.object_info = Some(info);
    s.toast(format!("NL: ComfyUI verified, {classes} node classes"));
    let mut response = report;
    response["url"] = json!(url);
    response["apiVerified"] = json!(true);
    Ok(response)
}

/// `nl.workflows.diagnose` — static (and, with a cached `/object_info`, live) doctor for one
/// workflow or all of them.
fn workflows_diagnose(s: &mut Session, p: &Value) -> Result<Value> {
    // One `exec` (CLI or agent) does not keep the session between calls, so the doctor can fetch
    // the API description itself when asked: `api: true` means "include the live checks".
    if s.nl.object_info.is_none() && p.get("api").and_then(Value::as_bool).unwrap_or(false) {
        let url = s.prefs.nl.comfy_url();
        let info = http::get_json(&url, "object_info", Duration::from_secs(120)).map_err(|e| bad("nl.workflows.diagnose", format!("ComfyUI {url}: {e}")))?;
        s.nl.object_info = Some(info);
    }
    let api = s.nl.object_info.as_ref();
    if let Some(path) = p.get("workflow").and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty()) {
        let file = models::find(s, path).ok_or_else(|| bad("nl.workflows.diagnose", format!("workflow not found: {path}")))?;
        let text = std::fs::read_to_string(&file).map_err(|e| bad("nl.workflows.diagnose", format!("{}: {e}", file.display())))?;
        let workflow: Value = serde_json::from_str(&text).map_err(|e| bad("nl.workflows.diagnose", format!("{}: not JSON: {e}", file.display())))?;
        let issues = workflows::diagnose(&workflow, api);
        let counts = issue_counts(&issues);
        return Ok(json!({
            "workflow": path,
            "apiVerified": api.is_some(),
            "issues": issues.iter().map(crate::nl::state::WorkflowIssue::to_json).collect::<Vec<_>>(),
            "counts": counts,
        }));
    }
    if !p.get("all").and_then(Value::as_bool).unwrap_or(false) {
        return Err(bad("nl.workflows.diagnose", "pass `workflow` (one file) or `all: true` (every workflow)"));
    }
    let list = models::workflows(s, "all").map_err(|e| bad("nl.workflows.diagnose", e))?;
    let root = models::root(s).ok_or_else(|| bad("nl.workflows.diagnose", "the workflow folder was not found"))?;
    let mut rows = Vec::new();
    let (mut errors, mut warnings) = (0usize, 0usize);
    for entry in list.iter().take(300) {
        let Some(path) = entry["path"].as_str() else { continue };
        let file = root.join(path);
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let Ok(workflow) = serde_json::from_str::<Value>(&text) else { continue };
        let issues = workflows::diagnose(&workflow, api);
        let counts = issue_counts(&issues);
        if let Some(errors_count) = counts["error"].as_u64().filter(|c| *c > 0) {
            errors += errors_count as usize;
        }
        if let Some(warnings_count) = counts["warning"].as_u64().filter(|c| *c > 0) {
            warnings += warnings_count as usize;
        }
        if !issues.is_empty() {
            rows.push(json!({
                "path": path,
                "counts": counts,
                "issues": issues.iter().take(8).map(crate::nl::state::WorkflowIssue::to_json).collect::<Vec<_>>(),
            }));
        }
    }
    Ok(json!({
        "apiVerified": api.is_some(),
        "workflows": rows,
        "totals": {"workflowsWithIssues": rows.len(), "errors": errors, "warnings": warnings},
        "note": if api.is_some() { "live checks against /object_info included" } else { "run nl.workflows.verify first for live checks against ComfyUI" },
    }))
}

fn issue_counts(issues: &[crate::nl::state::WorkflowIssue]) -> Value {
    let count = |severity: &str| issues.iter().filter(|i| i.severity == severity).count();
    json!({"error": count("error"), "warning": count("warning"), "info": count("info")})
}

/// `nl.workflows.capabilities` — the reference slots and LoRA nodes of one workflow, so the
/// Generate panel knows which reference fields to show and never offers references to a T2V/T2I.
fn workflows_capabilities(s: &mut Session, p: &Value) -> Result<Value> {
    let path = p
        .get("workflow")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| bad("nl.workflows.capabilities", "pass `workflow`"))?;
    let file = models::find(s, path).ok_or_else(|| bad("nl.workflows.capabilities", format!("workflow not found: {path}")))?;
    let text = std::fs::read_to_string(&file).map_err(|e| bad("nl.workflows.capabilities", format!("{}: {e}", file.display())))?;
    let workflow: Value = serde_json::from_str(&text).map_err(|e| bad("nl.workflows.capabilities", format!("{}: not JSON: {e}", file.display())))?;
    let capabilities = models::capabilities(&workflow);
    let mut loras = Vec::new();
    for (id, node) in workflow.as_object().into_iter().flat_map(|nodes| nodes.iter()) {
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default();
        if !class.to_ascii_lowercase().contains("lora") {
            continue;
        }
        loras.push(json!({
            "node": id,
            "class": class,
            "name": node.get("inputs").and_then(|i| i.get("lora_name")).and_then(Value::as_str).unwrap_or_default(),
        }));
    }
    Ok(json!({
        "path": path,
        "capabilities": capabilities,
        "loras": loras,
        "imageRefs": capabilities["imageRefs"].as_array().map_or(0, Vec::len),
        "videoInputs": capabilities["videoInputs"].as_array().map_or(0, Vec::len),
        "audioInputs": capabilities["audioInputs"].as_array().map_or(0, Vec::len),
    }))
}

/// `nl.workflows.parameters` — the real widgets of one workflow, resolved from `/object_info`.
fn workflows_parameters(s: &mut Session, p: &Value) -> Result<Value> {
    let path = p
        .get("workflow")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| bad("nl.workflows.parameters", "pass `workflow`: the workflow whose parameters you want"))?;
    let file = models::find(s, path).ok_or_else(|| bad("nl.workflows.parameters", format!("workflow not found: {path}")))?;
    let text = std::fs::read_to_string(&file).map_err(|e| bad("nl.workflows.parameters", format!("{}: {e}", file.display())))?;
    let workflow: Value = serde_json::from_str(&text).map_err(|e| bad("nl.workflows.parameters", format!("{}: not JSON: {e}", file.display())))?;
    let fetch = p.get("api").and_then(Value::as_bool).unwrap_or(false);
    if s.nl.object_info.is_none() && fetch {
        let url = s.prefs.nl.comfy_url();
        let info = http::get_json(&url, "object_info", Duration::from_secs(120)).map_err(|e| bad("nl.workflows.parameters", format!("ComfyUI {url}: {e}")))?;
        s.nl.object_info = Some(info);
    }
    let info = s.nl.object_info.as_ref().ok_or_else(|| {
        bad("nl.workflows.parameters", "run nl.workflows.verify first (or pass `api: true`): parameters come from ComfyUI's own /object_info")
    })?;
    let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(200).clamp(1, 2000) as usize;
    let mut report = workflows::parameters(&workflow, info, limit);
    report["workflow"] = json!(file.to_string_lossy());
    report["workflow"] = json!(path);
    Ok(report)
}

/// `nl.workflows.import` — copy an API-format workflow into the library; the original is only read.
fn workflows_import(s: &mut Session, p: &Value) -> Result<Value> {
    let source = p
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| bad("nl.workflows.import", "pass `path`: the workflow JSON exported from ComfyUI in API Format"))?;
    let source = PathBuf::from(source);
    if !source.is_file() {
        return Err(bad("nl.workflows.import", format!("{}: file not found", source.display())));
    }
    let text = std::fs::read_to_string(&source).map_err(|e| bad("nl.workflows.import", format!("{}: {e}", source.display())))?;
    let workflow: Value = serde_json::from_str(&text).map_err(|e| bad("nl.workflows.import", format!("{}: not JSON: {e}", source.display())))?;
    let nodes = workflow.as_object().into_iter().flat_map(|n| n.values()).filter(|n| n.get("class_type").is_some()).count();
    if !workflow.is_object() || nodes == 0 {
        return Err(bad("nl.workflows.import", "not a ComfyUI API workflow: export it with Save (API Format)"));
    }
    let root = models::root(s).ok_or_else(|| bad("nl.workflows.import", "the workflow folder was not found"))?;
    let dir = root.join("nl-imports");
    std::fs::create_dir_all(&dir).map_err(|e| bad("nl.workflows.import", format!("{}: {e}", dir.display())))?;
    let stem: String = source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "workflow".into())
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_') { c } else { '_' })
        .collect();
    let mut destination = dir.join(format!("{stem}.json"));
    let mut n = 2;
    while destination.exists() {
        destination = dir.join(format!("{stem}-{n}.json"));
        n += 1;
    }
    std::fs::copy(&source, &destination).map_err(|e| bad("nl.workflows.import", format!("{}: {e}", destination.display())))?;
    let relative = destination.strip_prefix(&root).unwrap_or(&destination).to_string_lossy().replace('\\', "/");
    s.toast(format!("NL: workflow imported as {relative}"));
    Ok(json!({"path": relative, "absolute": destination.to_string_lossy(), "nodes": nodes}))
}

/// `nl.workflows.fix` — write a repaired copy (sizes on the model grid) under the user data folder.
fn workflows_fix(s: &mut Session, p: &Value) -> Result<Value> {
    let path = p
        .get("workflow")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| bad("nl.workflows.fix", "pass `workflow`: the workflow to repair"))?;
    let file = models::find(s, path).ok_or_else(|| bad("nl.workflows.fix", format!("workflow not found: {path}")))?;
    let text = std::fs::read_to_string(&file).map_err(|e| bad("nl.workflows.fix", format!("{}: {e}", file.display())))?;
    let workflow: Value = serde_json::from_str(&text).map_err(|e| bad("nl.workflows.fix", format!("{}: not JSON: {e}", file.display())))?;
    let (fixed, fixes) = workflows::repair(&workflow);
    if fixes.is_empty() {
        return Ok(json!({"fixed": false, "path": file.to_string_lossy(), "fixes": [], "note": "no automatic fix applies; see nl.workflows.diagnose"}));
    }
    let base = crate::autosave::default_data_dir().unwrap_or_else(std::env::temp_dir);
    let dir = base.join("nl").join("workflows").join("fixed");
    std::fs::create_dir_all(&dir).map_err(|e| bad("nl.workflows.fix", format!("{}: {e}", dir.display())))?;
    let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "workflow".into());
    let mut destination = dir.join(format!("{stem}-fixed.json"));
    let mut n = 2;
    while destination.exists() {
        destination = dir.join(format!("{stem}-fixed-{n}.json"));
        n += 1;
    }
    let bytes = serde_json::to_vec_pretty(&fixed).map_err(|e| bad("nl.workflows.fix", e.to_string()))?;
    std::fs::write(&destination, bytes).map_err(|e| bad("nl.workflows.fix", format!("{}: {e}", destination.display())))?;
    s.toast("NL: repaired workflow copy written (the original is untouched)");
    Ok(json!({"fixed": true, "original": file.to_string_lossy(), "path": destination.to_string_lossy(), "fixes": fixes}))
}

/// `nl.subtitles.srt` — write subtitles for a clip from the transcript's word timings.
fn subtitle_srt(s: &mut Session, p: &Value) -> Result<Value> {
    let (item, media) = target_media(s, p).ok_or_else(|| bad("nl.subtitles.srt", "select the clip in the Project panel"))?;
    let transcript =
        transcript_for(s, item).ok_or_else(|| bad("nl.subtitles.srt", "that clip has no transcript yet: transcribe it first (NL ASR or Text ▸ Transcribe)"))?;
    let from = p.get("start").and_then(Value::as_f64).ok_or_else(|| bad("nl.subtitles.srt", "need `start` in seconds"))?;
    let to = p.get("end").and_then(Value::as_f64).ok_or_else(|| bad("nl.subtitles.srt", "need `end` in seconds"))?;
    if !from.is_finite() || !to.is_finite() || to <= from || from < 0.0 {
        return Err(bad("nl.subtitles.srt", "start/end must be finite seconds with start < end"));
    }
    let mut cues = subtitles::cues_from_words(&transcript.words, from, to, 42, 0.6);
    if cues.is_empty() {
        return Err(bad("nl.subtitles.srt", "no words in that range: nothing to write rather than empty cues"));
    }
    let variant = if p.get("hormozi").and_then(Value::as_bool).unwrap_or(false) { "hormozi" } else { "normal" };
    if variant == "hormozi" {
        cues = subtitles::hormozi(&cues);
    }
    let path = match p.get("path").and_then(Value::as_str).filter(|p| !p.trim().is_empty()) {
        Some(path) => PathBuf::from(path),
        None => default_subtitle_path(s, &media, from, to, variant)?,
    };
    let text = subtitles::to_srt(&cues);
    std::fs::write(&path, text).map_err(|e| bad("nl.subtitles.srt", format!("{}: {e}", path.display())))?;
    Ok(json!({"path": path.to_string_lossy(), "variant": variant, "cues": cues.len(), "words": transcript.words.len()}))
}

fn default_subtitle_path(s: &Session, media: &str, from: f64, to: f64, variant: &str) -> Result<PathBuf> {
    let dir = match s.path.as_deref() {
        Some(path) => PathBuf::from(path).parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from(".")),
        None => return Err(bad("nl.subtitles.srt", "save the project first or pass `path` for the subtitle file")),
    };
    let stem = std::path::Path::new(media).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "clip".into());
    Ok(dir.join(format!("{stem}_{from:.1}-{to:.1}_{variant}.srt")))
}

/// The language of an NL step: the command's, else the NL preference; validated once here.
fn language(s: &Session, p: &Value, cmd: &str) -> Result<String> {
    let language =
        p.get("language").and_then(Value::as_str).map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).unwrap_or_else(|| s.prefs.nl.asr_language());
    if !matches!(language.as_str(), "it" | "en" | "es" | "fr" | "de" | "pt" | "nl" | "pl" | "auto") {
        return Err(bad(cmd, "language must be it, en, es, fr, de, pt, nl, pl or auto"));
    }
    Ok(language)
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec("nl.models.workflows", "NL Workflows", r#"{"kind":"image|video|audio|all"?}"#, workflows),
        spec("nl.models.ollama", "NL Ollama Models", "{}", ollama_models),
        spec("nl.models.llm", "NL LLM Models", "{}", llm_models),
        spec("nl.services.status", "NL Services Status", "{}", services_status),
        spec("nl.diagnostics", "NL Diagnostics", "{}", diagnostics),
        spec(
            "nl.size.preview",
            "Generated Size",
            r#"{"resolutionMode":"image|video"?,"imageScale":"1x|2x"|float?,"videoScale":"2/3|1/2"?,"sizeMultiple":"8|16"?}"#,
            size_preview,
        ),
        spec(
            "nl.clipping.propose",
            "NL Clipping Candidates",
            r#"{"item":id?,"mode":"1|2|3"?,"language":str?,"params":obj?,"vertical":bool?}"#,
            clipping_propose,
        ),
        spec("nl.clipping.render", "NL Render Clips", r#"{"items":[obj],"language":str?,"vertical":bool?}"#, clipping_render),
        spec("nl.sgrossa.analyze", "NL Rough Cut Plan", r#"{"item":id?,"language":str?,"params":obj?}"#, sgrossa_analyze),
        spec("nl.sgrossa.apply", "NL Render Rough Cut", r#"{"items":[[float,float]],"language":str?}"#, sgrossa_apply),
        spec("nl.sgrossa.report", "NL Rough Cut Report", r#"{"path":str?}"#, sgrossa_report),
        spec("nl.pipeline.result", "NL Last Result", r#"{"operation":str?}"#, pipeline_result),
        spec("nl.subtitles.srt", "NL Subtitles", r#"{"item":id?,"start":float,"end":float,"hormozi":bool?,"path":str?}"#, subtitle_srt),
        spec("nl.cover.set", "NL Set Cover", r#"{"item":id?}"#, cover_set),
        spec("nl.workflows.list", "NL Workflow Library", r#"{"kind":"image|video|audio|all"?}"#, workflows_list),
        spec("nl.workflows.verify", "NL Verify ComfyUI API", "{}", workflows_verify),
        spec("nl.workflows.diagnose", "NL Workflow Doctor", r#"{"workflow":str?,"all":bool?,"api":bool?}"#, workflows_diagnose),
        spec("nl.workflows.parameters", "NL Workflow Parameters", r#"{"workflow":str,"api":bool?,"limit":int?}"#, workflows_parameters),
        spec("nl.workflows.capabilities", "NL Workflow Capabilities", r#"{"workflow":str}"#, workflows_capabilities),
        spec(
            "nl.workflows.presets",
            "NL Workflow Presets",
            r#"{"action":"list|save|load|copy|delete","name":str?,"from":str?,"workflow":str?,"overrides":[any]?}"#,
            |_s, p| workflows::preset_action(p),
        ),
        spec("nl.workflows.import", "NL Import Workflow", r#"{"path":str}"#, workflows_import),
        spec("nl.workflows.fix", "NL Fix Workflow Copy", r#"{"workflow":str}"#, workflows_fix),
    ]
    .into_iter()
    .chain(story::commands())
    .chain(prompt::commands())
    .chain(longform::commands())
    .chain(search::commands())
    .chain(queue::commands())
    .chain(reframe::commands())
    .chain(translate::commands())
    .chain(assistant::commands())
    .chain(titles::commands())
    .chain(graphics::commands())
    .chain(generative::commands())
    .chain(delivery::commands())
    .chain(maintenance::commands())
    .chain(album::commands())
    .chain(analysis::commands())
    .chain(versions::commands())
    .chain(montage::commands())
    .chain(agent::commands())
    .chain(workflow_schema::commands())
    .collect()
}

/// The suite root the workflows and engines live under, or `None` when it cannot be found.
pub(crate) fn suite_root() -> Option<std::path::PathBuf> {
    if let Ok(root) = std::env::var("NL_SUITE_ROOT")
        && !root.trim().is_empty()
    {
        return Some(std::path::PathBuf::from(root));
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
            if ancestor.join("workflows/image").is_dir() || ancestor.join("runtime/python").is_dir() {
                return Some(ancestor.to_path_buf());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use crate::Session;
    use filmcraft_media::DemoScene;
    use filmcraft_project::{ItemId, Transcript, Word};
    use filmcraft_time::Tick;

    #[test]
    fn clipping_and_rough_cut_need_a_video_and_a_valid_mode() {
        let mut s = Session::default();
        let e = s.execute("nl.clipping.propose", json!({})).unwrap_err().to_string();
        assert!(e.contains("select a video"), "{e}");
        let e = s.execute("nl.sgrossa.analyze", json!({})).unwrap_err().to_string();
        assert!(e.contains("select a video"), "{e}");
        assert!(s.execute("nl.clipping.propose", json!({"mode": "9"})).is_err(), "unknown mode is refused before any work");
    }

    #[test]
    fn a_render_needs_selected_items() {
        let mut s = Session::default();
        assert!(s.execute("nl.clipping.render", json!({})).is_err());
        let e = s.execute("nl.sgrossa.apply", json!({})).unwrap_err().to_string();
        assert!(e.contains("select"), "{e}");
    }

    #[test]
    fn pipeline_results_are_explicit_before_any_step_ran() {
        let mut s = Session::default();
        let e = s.execute("nl.pipeline.result", json!({"operation": "clips"})).unwrap_err().to_string();
        assert!(e.contains("no clips result yet"), "{e}");
        let r = s.execute("nl.pipeline.result", json!({})).unwrap();
        assert!(r["results"].as_object().unwrap().is_empty());
        let e = s.execute("nl.sgrossa.report", json!({})).unwrap_err().to_string();
        assert!(e.contains("run the rough cut analysis first"), "{e}");
    }

    /// A real imported movie with an Italian transcript stored under its media.
    fn movie_with_words(directory: &std::path::Path) -> (Session, ItemId) {
        let movie = directory.join("clip.mp4");
        crate::media_test_util::make_movie(&movie, DemoScene::OceanSunset, 64, 36, 48);
        let (mut s, items, _) = crate::media_test_util::session_with(&[&movie]);
        let media = items[0];
        let mut transcript = Transcript { language: "it".into(), ..Default::default() };
        for (text, start, end) in [("Ciao", 0.2, 0.5), ("a", 0.55, 0.7), ("tutti", 1.4, 1.9)] {
            transcript.words.push(Word::new(text, Tick::from_seconds_f64(start), Tick::from_seconds_f64(end)));
        }
        transcript.normalize();
        s.edit("test transcript", move |project, _| {
            project.transcripts.insert(media, Arc::new(transcript));
            Ok(())
        })
        .unwrap();
        (s, media)
    }

    #[test]
    fn subtitles_are_written_from_word_timings() {
        let dir = crate::media_test_util::tmp_dir("nl-subtitles");
        let (mut s, _) = movie_with_words(&dir);
        let normal = dir.join("normal.srt");
        let r = s.execute("nl.subtitles.srt", json!({"start": 0.0, "end": 3.0, "path": normal.to_string_lossy()})).unwrap();
        assert_eq!(r["variant"], "normal");
        let text = std::fs::read_to_string(&normal).unwrap();
        assert!(text.contains("Ciao a"), "{text}");
        assert!(text.contains("tutti"), "{text}");
        assert!(text.contains("-->"), "an SRT has timings: {text}");

        let hormozi = dir.join("hormozi.srt");
        let r = s.execute("nl.subtitles.srt", json!({"start": 0.0, "end": 3.0, "hormozi": true, "path": hormozi.to_string_lossy()})).unwrap();
        assert_eq!(r["variant"], "hormozi");
        let text = std::fs::read_to_string(&hormozi).unwrap();
        assert!(text.contains("CIAO"), "the hormozi variant is uppercase: {text}");

        // An empty range is an error, not an empty subtitle file.
        assert!(s.execute("nl.subtitles.srt", json!({"start": 10.0, "end": 12.0, "path": normal.to_string_lossy()})).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn subtitles_need_a_transcript() {
        let dir = crate::media_test_util::tmp_dir("nl-subtitles-none");
        let movie = dir.join("clip.mp4");
        crate::media_test_util::make_movie(&movie, DemoScene::Plasma, 64, 36, 24);
        let (mut s, _, _) = crate::media_test_util::session_with(&[&movie]);
        let e = s.execute("nl.subtitles.srt", json!({"start": 0.0, "end": 2.0, "path": "x.srt"})).unwrap_err().to_string();
        assert!(e.contains("no transcript"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod cover_tests {
    use serde_json::json;

    #[test]
    fn the_cover_is_recorded_next_to_a_saved_project() {
        let dir = crate::media_test_util::tmp_dir("nl-cover");
        let movie = dir.join("shot.mp4");
        crate::media_test_util::make_movie(&movie, filmcraft_media::DemoScene::OceanSunset, 64, 36, 24);
        let (mut s, items, _) = crate::media_test_util::session_with(&[&movie]);
        s.state.project_selection = vec![items[0]];
        s.path = Some(dir.join("progetto.fcproj").to_string_lossy().to_string());
        let r = s.execute("nl.cover.set", json!({})).unwrap();
        assert_eq!(r["saved"], true);
        let side = dir.join("nl").join("cover.json");
        assert!(side.is_file(), "the reference is kept beside the project");
        let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&side).unwrap()).unwrap();
        assert_eq!(value["item"], items[0].0);
        assert_eq!(s.nl.cover.as_ref().unwrap()["item"], items[0].0);
        // A cover is a chosen image, not whatever happens to be selected.
        s.state.project_selection.clear();
        let e = s.execute("nl.cover.set", json!({})).unwrap_err().to_string();
        assert!(e.contains("select the generated cover"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unsaved_project_keeps_the_cover_in_the_session_only() {
        let dir = crate::media_test_util::tmp_dir("nl-cover-unsaved");
        let movie = dir.join("shot.mp4");
        crate::media_test_util::make_movie(&movie, filmcraft_media::DemoScene::Plasma, 64, 36, 24);
        let (mut s, items, _) = crate::media_test_util::session_with(&[&movie]);
        s.state.project_selection = vec![items[0]];
        let r = s.execute("nl.cover.set", json!({})).unwrap();
        assert_eq!(r["saved"], false);
        assert_eq!(s.nl.cover.as_ref().unwrap()["item"], items[0].0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod performance_tests {
    use serde_json::json;

    use crate::Session;

    /// Opening a project must not start AI work or copy a workflow library into memory: the app
    /// stays a fast editor until the operator asks for something.
    #[test]
    fn opening_a_project_starts_no_ai_work() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        assert!(s.jobs.is_empty(), "no background job at open");
        assert!(s.nl.pending.is_empty(), "no NL job at open");
        assert!(s.nl.results.is_empty(), "no analysis at open");
        assert!(s.nl.object_info.is_none(), "ComfyUI is not contacted at open");
        assert!(s.comfy_jobs.is_empty() && s.asr_jobs.is_empty() && s.shorts_jobs.is_empty());
    }

    #[test]
    fn the_active_timeline_is_the_default_media() {
        let dir = crate::media_test_util::tmp_dir("nl-timeline-target");
        let movie = dir.join("clip.mp4");
        crate::media_test_util::make_movie(&movie, filmcraft_media::DemoScene::Plasma, 64, 36, 24);
        let (s, items, _) = crate::media_test_util::session_with(&[&movie]);
        let (media, path) = super::target_media(&s, &json!({})).expect("the open sequence is the default media");
        assert_eq!(media, items[0]);
        assert!(!path.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
