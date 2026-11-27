//! Dynamic schema of one ComfyUI workflow: parameters classified from `/object_info`, media
//! reference slots (image/video/audio/video-audio), LoRA controls and live validity warnings.
//! This is the information the object-action panel needs to populate fields and offer every
//! compatible workflow instead of a hardcoded list.

use std::time::Duration;

use serde_json::{Value, json};

use crate::{CommandSpec, Session};

fn bad(msg: impl ToString) -> crate::EngineError {
    crate::EngineError::BadParams { cmd: "nl.workflows.schema".into(), msg: msg.to_string() }
}

fn link(value: &Value) -> Option<(&str, u64)> {
    let pair = value.as_array()?;
    if pair.len() != 2 || pair[0].is_string() == false {
        return None;
    }
    Some((pair[0].as_str()?, pair[1].as_u64().unwrap_or(0)))
}

fn spec_of<'a>(info: &'a Value, class: &str, field: &str) -> Option<&'a Value> {
    let input = info.get(class)?.get("input")?;
    input.get("required").and_then(|m| m.get(field)).or_else(|| input.get("optional").and_then(|m| m.get(field)))
}

fn describe(spec: Option<&Value>, current: &Value) -> (String, Option<Vec<String>>, Option<(f64, f64)>, Option<Value>) {
    let Some(spec) = spec else {
        let kind = match current {
            Value::Bool(_) => "bool",
            Value::Number(n) if n.is_i64() || n.is_u64() => "int",
            Value::Number(_) => "float",
            Value::String(_) => "text",
            _ => "input",
        };
        return (kind.to_string(), None, None, None);
    };
    let list = spec.as_array();
    if let Some(values) = list.and_then(|a| a.first()).and_then(Value::as_array) {
        return ("enum".into(), Some(values.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()), None, None);
    }
    let tag = list.and_then(|a| a.first()).and_then(Value::as_str).unwrap_or_default();
    let opts = list.and_then(|a| a.get(1));
    let mut enum_values = None;
    let kind = match tag {
        "INT" => "int",
        "FLOAT" => "float",
        "BOOLEAN" => "bool",
        "STRING" => "text",
        "COMBO" => {
            enum_values = opts
                .and_then(|o| o.get("options"))
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(|v| v.as_str().map(str::to_string)).collect::<Vec<_>>());
            "enum"
        }
        _ => match current {
            Value::Bool(_) => "bool",
            Value::Number(n) if n.is_i64() || n.is_u64() => "int",
            Value::Number(_) => "float",
            Value::String(_) => "text",
            _ => "input",
        },
    };
    let range = opts.and_then(|o| {
        let min = o.get("min").and_then(Value::as_f64)?;
        let max = o.get("max").and_then(Value::as_f64)?;
        Some((min, max))
    });
    let default = opts.and_then(|o| o.get("default").cloned());
    (kind.to_string(), enum_values, range, default)
}

fn role_for(class: &str, field: &str) -> (&'static str, &'static str) {
    let lower = field.to_ascii_lowercase();
    let class = class.to_ascii_lowercase();
    if lower.contains("negative") {
        return ("negative", "Prompt");
    }
    if lower == "prompt" || lower == "text" || lower == "caption" || lower == "tags" {
        return ("prompt", "Prompt");
    }
    if lower == "lyrics" {
        return ("lyrics", "Prompt secondari");
    }
    if lower.contains("seed") {
        return ("seed", "Seed");
    }
    if matches!(lower.as_str(), "length" | "duration" | "seconds" | "max_duration") {
        return ("duration", "Durata");
    }
    if matches!(lower.as_str(), "width" | "height" | "aspect_ratio" | "megapixels" | "multiple") {
        return ("resolution", "Risoluzione");
    }
    if matches!(lower.as_str(), "sampler_name" | "scheduler" | "steps" | "cfg" | "denoise") {
        return ("sampling", "Campionamento");
    }
    if lower.ends_with("_name") && (class.contains("loader") || class.contains("lora")) {
        return ("model", "Modelli");
    }
    if lower == "switch" {
        return ("switch", "Interruttori");
    }
    if class.contains("lora") {
        return ("lora", "LoRA");
    }
    ("other", "Avanzate")
}

fn upstream_file(graph: &Value, start: &str, depth: usize) -> Option<String> {
    if depth > 8 {
        return None;
    }
    let node = graph.get(start)?;
    let inputs = node.get("inputs")?.as_object()?;
    for key in ["image", "video", "audio", "file"] {
        if let Some(value) = inputs.get(key).and_then(Value::as_str).filter(|v| !v.is_empty()) {
            return Some(value.to_string());
        }
    }
    for value in inputs.values() {
        if let Some((next, _)) = link(value)
            && let Some(found) = upstream_file(graph, next, depth + 1)
        {
            return Some(found);
        }
    }
    None
}

fn slot_limits(info: &Value, class: &str) -> Value {
    let input = info.get(class).and_then(|node| node.get("input")).and_then(|input| input.get("optional"));
    let max = |name: &str| {
        input
            .and_then(|m| m.get(name))
            .and_then(|spec| spec.get(1))
            .and_then(|opts| opts.get("template"))
            .and_then(|template| template.get("max"))
            .and_then(Value::as_u64)
    };
    json!({
        "image": max("ref_images").unwrap_or(9),
        "video": max("ref_videos").unwrap_or(3),
        "audio": max("ref_audios").unwrap_or(3),
        "videoAudio": max("ref_video_audios").unwrap_or(3)
    })
}

fn lora_names(info: &Value) -> Vec<String> {
    let mut names = Vec::new();
    for class in ["LoraLoader", "LoraLoaderModelOnly"] {
        if let Some(spec) = spec_of(info, class, "lora_name")
            && let Some(values) = spec.get(0).and_then(Value::as_array)
        {
            names.extend(values.iter().filter_map(|v| v.as_str().map(str::to_string)));
        }
    }
    names.sort();
    names.dedup();
    names
}

/// `nl.workflows.schema` — one workflow classified with its live ComfyUI schema.
fn schema(s: &mut Session, p: &Value) -> std::result::Result<Value, crate::EngineError> {
    let workflow_path = p.get("workflow").and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty()).ok_or_else(|| bad("pass `workflow`"))?;
    let file = super::models::find(s, workflow_path).ok_or_else(|| bad(format!("workflow not found: {workflow_path}")))?;
    let text = std::fs::read_to_string(&file).map_err(|e| bad(format!("{}: {e}", file.display())))?;
    let graph: Value = serde_json::from_str(&text).map_err(|e| bad(format!("{}: not JSON: {e}", file.display())))?;
    let Some(nodes) = graph.as_object() else { return Err(bad("not a ComfyUI API workflow")) };

    let fetch = p.get("api").and_then(Value::as_bool).unwrap_or(false);
    if s.nl.object_info.is_none() || fetch {
        let url = s.prefs.nl.comfy_url();
        let info = crate::nl::http::get_json(&url, "object_info", Duration::from_secs(120)).map_err(|e| bad(format!("ComfyUI {url}: {e}")))?;
        s.nl.object_info = Some(info);
    }
    let info = s.nl.object_info.as_ref().ok_or_else(|| bad("ComfyUI /object_info is not available"))?;

    let mut parameters = Vec::new();
    let mut slots = Vec::new();
    let mut loras = Vec::new();
    let mut warnings = Vec::new();
    let mut limits = json!({});
    let mut ref_limits_found = false;
    let mut seen_classes = std::collections::BTreeSet::new();

    for (node_id, node) in nodes {
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default();
        if !seen_classes.insert(class.to_string()) {
            // still collect parameters below
        }
        let title = node.get("_meta").and_then(|meta| meta.get("title")).and_then(Value::as_str).unwrap_or("");
        let inputs = node.get("inputs").and_then(Value::as_object);
        let Some(inputs) = inputs else { continue };

        if class.starts_with("LoraLoader") {
            let name = inputs.get("lora_name").and_then(Value::as_str).unwrap_or("");
            let strength = inputs.get("strength_model").and_then(Value::as_f64).or_else(|| inputs.get("strength_clip").and_then(Value::as_f64)).unwrap_or(0.0);
            loras.push(json!({
                "node": node_id, "name": name, "strength": strength,
                "enabled": strength != 0.0, "title": title
            }));
        }

        for (field, value) in inputs {
            let (kind, enum_values, range, default) = describe(spec_of(info, class, field), value);
            let (role, group) = role_for(class, field);
            if let Some((from, to)) = range {
                parameters.push(json!({
                    "id": format!("{node_id}.{field}"), "node": node_id, "field": field,
                    "class": class, "title": title, "role": role, "group": group,
                    "kind": kind, "enum": enum_values, "range": [from, to],
                    "default": default, "current": value, "linked": false
                }));
            } else {
                parameters.push(json!({
                    "id": format!("{node_id}.{field}"), "node": node_id, "field": field,
                    "class": class, "title": title, "role": role, "group": group,
                    "kind": kind, "enum": enum_values,
                    "default": default, "current": value, "linked": false
                }));
            }
        }
    }

    // Reference slots: keys ref_images.*, ref_videos.*, ref_video_audios.*, ref_audios.*.
    for (node_id, node) in nodes {
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default();
        let Some(inputs) = node.get("inputs").and_then(Value::as_object) else { continue };
        let mut has_refs = false;
        for (field, value) in inputs {
            let (kind, key) = if field.starts_with("ref_images.") {
                ("image", "image")
            } else if field.starts_with("ref_videos.") {
                ("video", "video")
            } else if field.starts_with("ref_video_audios.") {
                ("video_audio", "videoAudio")
            } else if field.starts_with("ref_audios.") {
                ("audio", "audio")
            } else {
                continue;
            };
            has_refs = true;
            let index =
                field.chars().rev().take_while(|c| c.is_ascii_digit()).collect::<String>().chars().rev().collect::<String>().parse::<u32>().unwrap_or(0) + 1;
            let connected = link(value);
            let file = connected.and_then(|(source, _)| upstream_file(&graph, source, 0));
            slots.push(json!({
                "kind": kind, "key": key, "index": index, "consumer": node_id,
                "connected": connected.is_some(), "node": connected.map(|(source, _)| source),
                "file": file, "field": field
            }));
        }
        if has_refs && !ref_limits_found {
            limits = slot_limits(info, class);
            ref_limits_found = true;
        }
    }

    let missing: Vec<String> = seen_classes.iter().filter(|class| !info.get(class.as_str()).is_some()).cloned().collect();
    if !missing.is_empty() {
        warnings.push(format!("Nodi non installati: {}", missing.join(", ")));
    }
    let count = |kind: &str| slots.iter().filter(|slot| slot["kind"] == kind && slot["connected"] == true).count();
    let duration = if seen_classes.iter().any(|class| class.contains("MiniMaxH3") || class.contains("MinimaxH3")) {
        [4.0, 15.0]
    } else if seen_classes.iter().any(|class| class.contains("Music")) {
        [0.04, 360.0]
    } else {
        [1.0, 30.0]
    };

    Ok(json!({
        "workflow": workflow_path.replace('\\', "/"),
        "nodeCount": nodes.len(),
        "classes": seen_classes.into_iter().collect::<Vec<_>>(),
        "parameters": parameters,
        "slots": slots,
        "slotCounts": {"image": count("image"), "video": count("video"), "audio": count("audio"), "videoAudio": count("video_audio")},
        "limits": limits,
        "loras": loras,
        "loraNames": lora_names(info),
        "durationRange": duration,
        "missingClasses": missing,
        "warnings": warnings,
        "apiVerified": true
    }))
}

/// `nl.workflows.delete` — move one workflow out of the library into `da_eliminare/workflows`.
/// Nothing is erased: the file can be restored or inspected.
fn delete_workflow(s: &mut Session, p: &Value) -> std::result::Result<Value, crate::EngineError> {
    let workflow = p.get("workflow").and_then(Value::as_str).map(str::trim).filter(|w| !w.is_empty()).ok_or_else(|| bad("pass `workflow`"))?;
    let file = super::models::find(s, workflow).ok_or_else(|| bad(format!("workflow not found: {workflow}")))?;
    let root = crate::nl::suite_root().ok_or_else(|| bad("suite root not found"))?;
    let workflows_root = root.join("workflows");
    let canonical = file.canonicalize().map_err(|e| bad(format!("{}: {e}", file.display())))?;
    if !canonical.starts_with(workflows_root.canonicalize().unwrap_or(workflows_root.clone())) {
        return Err(bad("refusing to delete a file outside workflows/"));
    }
    let trash = root.join("da_eliminare").join("workflows");
    std::fs::create_dir_all(&trash).map_err(|e| bad(format!("{}: {e}", trash.display())))?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name = canonical.file_name().and_then(|n| n.to_str()).unwrap_or("workflow.json");
    let destination = trash.join(format!("{stamp}-{name}"));
    std::fs::rename(&canonical, &destination).map_err(|e| bad(format!("{}: {e}", canonical.display())))?;
    s.toast(format!("Workflow spostato in {}", destination.display()));
    Ok(json!({"moved": destination.to_string_lossy(), "workflow": workflow}))
}

/// `nl.workflows.suggestLora` — let the local LLM pick LoRA names from the installed catalogue
/// that fit the current prompt. Names are validated against the catalogue; nothing is applied
/// without the operator pressing Add.
fn suggest_lora(s: &mut Session, p: &Value) -> std::result::Result<Value, crate::EngineError> {
    let prompt = p.get("prompt").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).ok_or_else(|| bad("pass `prompt`"))?;
    if s.nl.object_info.is_none() {
        let url = s.prefs.nl.comfy_url();
        let info = crate::nl::http::get_json(&url, "object_info", Duration::from_secs(120)).map_err(|e| bad(format!("ComfyUI {url}: {e}")))?;
        s.nl.object_info = Some(info);
    }
    let info = s.nl.object_info.as_ref().ok_or_else(|| bad("ComfyUI /object_info is not available"))?;
    let names = lora_names(info);
    if names.is_empty() {
        return Err(bad("no LoRA names in /object_info"));
    }
    let system = "Sei un direttore della fotografia. Scegli da 1 a 3 LoRA utili per il prompt, SOLO tra i nomi elencati. Rispondi con JSON {\"loras\":[\"nome esatto\",...]} senza altro testo.";
    let brief = format!("Prompt: {prompt}\nLoRA disponibili:\n{}", names.join("\n"));
    let llm = super::llm::LlmConfig::for_role(s, "prompt", p.get("model").and_then(Value::as_str));
    let answer = llm.auto_json(system, &brief, Duration::from_secs(180), Some("0")).map_err(|e| bad(e))?;
    let chosen: Vec<String> = answer["loras"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|row| row.as_str()).filter(|name| names.iter().any(|known| known == name)).map(str::to_string).take(3).collect())
        .unwrap_or_default();
    Ok(json!({"loras": chosen, "model": llm.model, "available": names.len()}))
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "nl.workflows.schema",
            label: "NL Workflow Schema",
            menu: &[],
            shortcut: None,
            params: r#"{"workflow":str,"api":bool?}"#,
            enabled: crate::commands::always,
            run: schema,
            journal: false,
            named: crate::commands::NamedKeys::NONE,
        },
        CommandSpec {
            id: "nl.workflows.suggestLora",
            label: "NL Suggest LoRA",
            menu: &[],
            shortcut: None,
            params: r#"{"prompt":str,"model":str?}"#,
            enabled: crate::commands::always,
            run: suggest_lora,
            journal: false,
            named: crate::commands::NamedKeys::NONE,
        },
        CommandSpec {
            id: "nl.workflows.delete",
            label: "NL Delete Workflow",
            menu: &[],
            shortcut: None,
            params: r#"{"workflow":str}"#,
            enabled: crate::commands::always,
            run: delete_workflow,
            journal: false,
            named: crate::commands::NamedKeys::NONE,
        },
    ]
}
