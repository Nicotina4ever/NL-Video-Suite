//! The workflow inventory: what the operator has under the suite's workflow folders, classified by
//! what the workflow produces (video, image, audio) and what inputs it needs (reference images,
//! video-to-video, audio), so the panels offer only workflows that fit what is being generated.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::Session;

/// Folders under the workflow root that are experiments, not runnable workflows.
const SKIPPED: &[&str] = &["old", "archive", "_archive", "autofix", "__pycache__"];

/// The workflow folder: the NL preference, else the suite's `workflows` folder (every vendored
/// family: regista, prompter, calliope).
pub fn root(s: &Session) -> Option<PathBuf> {
    let configured = s.prefs.nl.comfy_workflow_dir.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        return if path.is_absolute() { Some(path) } else { super::suite_root().map(|root| root.join(path)) };
    }
    let root = super::suite_root()?;
    Some(root.join("workflows"))
}

/// The workflow kind from its folder under the root, for files whose content says nothing.
pub fn kind_of(relative: &Path) -> &'static str {
    match relative.components().next().and_then(|c| c.as_os_str().to_str()).unwrap_or_default() {
        "image" | "image_edit" => "image",
        "video" => "video",
        "audio" => "audio",
        "analisi" => "analysis",
        _ => "other",
    }
}

/// What a workflow produces, from the nodes that save the result (falls back to the folder).
pub fn workflow_kind(workflow: &Value, relative: &Path) -> &'static str {
    let mut video = false;
    let mut image = false;
    let mut audio = false;
    for node in workflow.as_object().into_iter().flat_map(|nodes| nodes.values()) {
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
        if class.contains("savevideo") || class.contains("vhs_videocombine") || class.contains("saveanimated") {
            video = true;
        } else if class.contains("saveaudio") || class.contains("previewaudio") {
            audio = true;
        } else if class.contains("saveimage") || class.contains("previewimage") {
            image = true;
        }
    }
    match (video, image, audio) {
        (true, _, _) => "video",
        (_, true, _) => "image",
        (_, _, true) => "audio",
        _ => kind_of(relative),
    }
}

/// Inputs a workflow offers: prompt slots, image references, a video and an audio input, and
/// whether it can extend/convert an existing video (v2v).
pub fn capabilities(workflow: &Value) -> Value {
    let mut prompt_slots = Vec::new();
    let mut image_refs = Vec::new();
    let mut video_inputs = Vec::new();
    let mut audio_inputs = Vec::new();
    let mut models = Vec::new();
    let mut v2v = false;
    for (id, node) in workflow.as_object().into_iter().flat_map(|nodes| nodes.iter()) {
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default();
        let lower = class.to_ascii_lowercase();
        let title = node.get("_meta").and_then(|m| m.get("title")).and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
        let inputs = node.get("inputs").and_then(Value::as_object);
        if lower.contains("loadimage") {
            image_refs.push(id.clone());
        } else if lower.contains("loadvideo") || lower.contains("vhs_loadvideo") || lower.contains("load_video") {
            video_inputs.push(id.clone());
        } else if lower.contains("loadaudio") || lower.contains("load_audio") || lower.contains("loadaudioui") {
            audio_inputs.push(id.clone());
        } else if lower.contains("videoxend") || lower.contains("videoextend") || lower.contains("minimaxh3videoextend") {
            v2v = true;
        }
        if (lower.contains("textencode") || lower.contains("text_encode") || lower.contains("prompt"))
            && inputs.is_some_and(|i| i.values().any(Value::is_string))
            && !title.contains("negative")
        {
            prompt_slots.push(id.clone());
        }
        if let Some(inputs) = inputs {
            for (key, value) in inputs {
                if let Some(model) = model_value(key, value) {
                    models.push(json!({"node": id, "field": key, "model": model}));
                }
            }
        }
    }
    if !video_inputs.is_empty() {
        v2v = true;
    }
    json!({
        "promptSlots": prompt_slots,
        "imageRefs": image_refs,
        "videoInputs": video_inputs,
        "audioInputs": audio_inputs,
        "v2v": v2v,
        "models": models,
    })
}

fn model_value(key: &str, value: &Value) -> Option<String> {
    const MODEL_KEYS: &[&str] = &["ckpt_name", "unet_name", "lora_name", "vae_name", "clip_name", "text_encoder", "model_name", "control_net_name"];
    if MODEL_KEYS.contains(&key) {
        return value.as_str().map(str::to_string);
    }
    // rgthree's Power Lora Loader keeps each LoRA in an object.
    if key.starts_with("lora_") && value.is_object() {
        return value.get("lora").and_then(Value::as_str).map(str::to_string);
    }
    None
}

/// Every runnable workflow, sorted by kind then path, with its path (relative to the root), kind
/// and capabilities. An unreadable file is listed with an `error` instead of being dropped.
pub fn workflows(s: &Session, kind: &str) -> Result<Vec<Value>, String> {
    let Some(root) = root(s) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    collect(&root, &root, 0, &mut out)?;
    let mut list: Vec<Value> = Vec::new();
    for (relative, bytes) in out {
        if bytes == 0 || relative.extension().is_none_or(|e| !e.eq_ignore_ascii_case("json")) {
            continue;
        }
        let path = root.join(&relative);
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let entry = match serde_json::from_str::<Value>(&text) {
            Ok(workflow) if workflow.is_object() => {
                let workflow_kind_of = workflow_kind(&workflow, &relative);
                if kind != "all" && kind != workflow_kind_of {
                    continue;
                }
                json!({
                    "path": relative.to_string_lossy().replace('\\', "/"),
                    "name": relative.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    "kind": workflow_kind_of,
                    "bytes": bytes,
                    "capabilities": capabilities(&workflow),
                })
            }
            Ok(_) => {
                json!({"path": relative.to_string_lossy().replace('\\', "/"), "name": relative.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), "kind": "other", "bytes": bytes, "error": "not a ComfyUI API workflow object"})
            }
            Err(e) => {
                json!({"path": relative.to_string_lossy().replace('\\', "/"), "name": relative.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), "kind": "other", "bytes": bytes, "error": format!("not JSON: {e}")})
            }
        };
        list.push(entry);
    }
    list.sort_by(|a, b| {
        (a["kind"].as_str().unwrap_or_default(), a["path"].as_str().unwrap_or_default())
            .cmp(&(b["kind"].as_str().unwrap_or_default(), b["path"].as_str().unwrap_or_default()))
    });
    Ok(list)
}

/// Whether a folder under the workflow root is an experiment rather than runnable workflows.
pub(crate) fn is_skipped(name: &str) -> bool {
    SKIPPED.contains(&name)
}

fn collect(root: &Path, dir: &Path, depth: usize, out: &mut Vec<(PathBuf, u64)>) -> Result<(), String> {
    if depth > 6 {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else { continue };
        if metadata.is_dir() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if is_skipped(&name) {
                continue;
            }
            collect(root, &path, depth + 1, out)?;
        } else if metadata.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            out.push((relative, metadata.len()));
        }
    }
    Ok(())
}

/// Resolve a workflow path: absolute, relative to the suite root, to the workflow root or to a
/// parent of it (so `regista/image/x.json` and `image/x.json` both work).
pub fn find(s: &Session, path: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        return candidate.is_file().then_some(candidate);
    }
    let mut candidates = Vec::new();
    if let Some(suite) = super::suite_root() {
        candidates.push(suite.join(&candidate));
    }
    if let Some(root) = root(s) {
        candidates.push(root.join(&candidate));
        if let Some(parent) = root.parent() {
            candidates.push(parent.join(&candidate));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// The workflows the NL preferences select, for the settings and panel summaries.
pub fn selected_workflows(s: &Session) -> Value {
    let nl = &s.prefs.nl;
    json!({
        "dir": root(s).map(|p| p.to_string_lossy().to_string()),
        "image": nl.comfy_image_workflow,
        "video": nl.comfy_video_workflow,
        "music": nl.comfy_music_workflow,
        "musicModel": nl.music_model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(entries: &[(&str, &str)]) -> Value {
        let mut workflow = serde_json::Map::new();
        for (id, class) in entries {
            workflow.insert((*id).into(), json!({"class_type": class, "inputs": {}}));
        }
        Value::Object(workflow)
    }

    #[test]
    fn the_saving_node_decides_the_kind() {
        assert_eq!(workflow_kind(&api(&[("1", "SaveVideo")]), Path::new("prompter/x.json")), "video");
        assert_eq!(workflow_kind(&api(&[("1", "SaveImage")]), Path::new("prompter/x.json")), "image");
        assert_eq!(workflow_kind(&api(&[("1", "SaveAudioAdvanced")]), Path::new("prompter/x.json")), "audio");
        assert_eq!(workflow_kind(&api(&[("1", "KSampler")]), Path::new("image/x.json")), "image");
        assert_eq!(workflow_kind(&api(&[("1", "KSampler")]), Path::new("prompter/x.json")), "other");
    }

    #[test]
    fn capabilities_name_the_inputs_that_matter() {
        let mut workflow = api(&[
            ("1", "LoadImage"),
            ("2", "VHS_LoadVideo"),
            ("3", "LoadAudio"),
            ("4", "CLIPTextEncode"),
            ("5", "MiniMaxH3VideoExtendPatched"),
            ("6", "CheckpointLoaderSimple"),
        ]);
        workflow["4"]["inputs"] = json!({"text": "a shot"});
        workflow["6"]["inputs"] = json!({"ckpt_name": "flux.safetensors"});
        let caps = capabilities(&workflow);
        assert_eq!(caps["v2v"], true, "{caps}");
        assert_eq!(caps["imageRefs"], json!(["1"]));
        assert_eq!(caps["videoInputs"], json!(["2"]));
        assert_eq!(caps["audioInputs"], json!(["3"]));
        assert_eq!(caps["promptSlots"], json!(["4"]));
        assert_eq!(caps["models"][0]["model"], "flux.safetensors");
    }

    #[test]
    fn a_prompt_that_is_an_empty_string_is_still_a_prompt_slot() {
        let mut workflow = api(&[("1", "CLIPTextEncode")]);
        workflow["1"]["inputs"] = json!({"text": ""});
        assert_eq!(capabilities(&workflow)["promptSlots"], json!(["1"]));
    }
}
