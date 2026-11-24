//! Workflow doctor and API verification, merged: one place that answers "will this workflow run on
//! *this* ComfyUI, with *these* models?" before a generation is queued.
//!
//! Two layers, because they fail for different reasons:
//! - **Static diagnosis** reads the API JSON: link targets that do not exist, prompt slots left
//!   empty, latent sizes off the model grid, absurd durations, known-broken AV-split wiring and
//!   empty reference inputs. It needs no server.
//! - **API verification** reads ComfyUI's own `/object_info` (read-only, the same call the Prompter
//!   uses): node classes installed, model files really present in the loader enums, reference
//!   files really present in the input enum. It never submits a prompt.
//!
//! Diagnosis never edits the original workflow. `nl.workflows.fix` writes a repaired *copy* under
//! the user data folder and says which changes it made; the operator's files stay untouched.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::Session;

use super::state::WorkflowIssue;

/// The static layer's fixes that can be applied without guessing: latent sizes rounded to the
/// model grid. Everything else is reported with what a person should change.
pub fn repair(workflow: &Value) -> (Value, Vec<String>) {
    let mut workflow = workflow.clone();
    let mut fixes = Vec::new();
    for (id, node) in workflow.as_object_mut().into_iter().flat_map(|nodes| nodes.iter_mut()) {
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default().to_string();
        if !class.to_ascii_lowercase().contains("latent") {
            continue;
        }
        if let Some(inputs) = node.get_mut("inputs").and_then(Value::as_object_mut) {
            for key in ["width", "height"] {
                let Some(value) = inputs.get(key).and_then(Value::as_u64) else { continue };
                let rounded = value.div_ceil(16) * 16;
                if rounded != value {
                    inputs.insert(key.to_string(), json!(rounded));
                    fixes.push(format!("node {id}: {key} {value} → {rounded} (multiple of 16)"));
                }
            }
        }
    }
    (workflow, fixes)
}

/// Diagnose one workflow; `object_info` is ComfyUI's `/object_info` when it has been fetched.
pub fn diagnose(workflow: &Value, object_info: Option<&Value>) -> Vec<WorkflowIssue> {
    let mut issues = Vec::new();
    let Some(nodes) = workflow.as_object() else {
        return vec![WorkflowIssue::error(None, None, "the file is not a ComfyUI API workflow object", Some("export it with Save (API Format)".into()))];
    };
    if nodes.is_empty() || !nodes.values().any(|n| n.get("class_type").is_some()) {
        return vec![WorkflowIssue::error(None, None, "the file has no nodes with a class_type", Some("export it in API Format, not as a UI graph".into()))];
    }
    let class_of = |id: &str| nodes.get(id).and_then(|n| n.get("class_type")).and_then(Value::as_str).map(str::to_string);
    let output_len = |class: &str| object_info.and_then(|info| info.get(class)).and_then(|i| i.get("output")).and_then(Value::as_array).map(Vec::len);
    for (id, node) in nodes {
        // Some exports keep UI-only marker nodes (groups, notes) without a class_type: ComfyUI
        // ignores them, so they are reported as a note rather than a defect.
        let Some(class) = node.get("class_type").and_then(Value::as_str) else {
            issues.push(WorkflowIssue::info(
                Some(id.clone()),
                None,
                "UI-only marker node (no class_type): ComfyUI skips it",
                Some("safe to leave; it carries a panel title or a note".into()),
            ));
            continue;
        };
        let class_lower = class.to_ascii_lowercase();
        let is_latent = class_lower.contains("latent");
        if let Some(info) = object_info
            && info.get(class).is_none()
        {
            issues.push(WorkflowIssue::error(
                Some(id.clone()),
                Some(class.to_string()),
                format!("node class `{class}` is not installed in this ComfyUI"),
                Some("install the custom node pack or pick another workflow".into()),
            ));
            continue;
        }
        let inputs = node.get("inputs").and_then(Value::as_object);
        let Some(inputs) = inputs else {
            issues.push(WorkflowIssue::error(Some(id.clone()), Some(class.to_string()), "node has no inputs", None));
            continue;
        };
        for (key, value) in inputs {
            // Links: [node id, output index].
            if let Some(link) = link_target(value) {
                match class_of(&link.0) {
                    None => issues.push(WorkflowIssue::error(
                        Some(id.clone()),
                        Some(class.to_string()),
                        format!("input `{key}` links to node {} which does not exist", link.0),
                        Some("reconnect the input to a real node".into()),
                    )),
                    Some(target_class) => {
                        if let Some(len) = output_len(&target_class)
                            && link.1 >= len
                        {
                            issues.push(WorkflowIssue::error(
                                Some(id.clone()),
                                Some(class.to_string()),
                                format!("input `{key}` reads output {} of `{target_class}` which has {len}", link.1),
                                Some("reconnect to an existing output".into()),
                            ));
                        }
                    }
                }
                continue;
            }
            // Empty prompt slots.
            if is_prompt_slot(key, class) {
                if let Some(text) = value.as_str()
                    && text.trim().is_empty()
                {
                    issues.push(WorkflowIssue::warning(
                        Some(id.clone()),
                        Some(class.to_string()),
                        format!("prompt slot `{key}` is empty"),
                        Some("write a prompt (the panels fill it) or pick another workflow".into()),
                    ));
                }
                continue;
            }
            // Model files and named references: verified against the server's own enum when known.
            // Workflow JSON on Windows often stores model paths with backslashes while ComfyUI's
            // enums use forward slashes, so both sides are normalised before comparing.
            if let Some(name) = value.as_str().filter(|v| !v.trim().is_empty())
                && let Some(choices) = enum_choices(object_info, class, key)
            {
                let normalised = normalize_separators(name);
                if !choices.iter().any(|c| normalize_separators(c) == normalised) {
                    let missing_reference = is_reference_key(key);
                    let what = if missing_reference { "reference file" } else { "model file" };
                    issues.push(if missing_reference {
                        WorkflowIssue::warning(
                            Some(id.clone()),
                            Some(class.to_string()),
                            format!("{what} `{name}` is not in ComfyUI's input list ({key})"),
                            Some("the panel supplies its own file when generating".into()),
                        )
                    } else {
                        WorkflowIssue::error(
                            Some(id.clone()),
                            Some(class.to_string()),
                            format!("{what} `{name}` is not available in this ComfyUI ({key})"),
                            Some("choose one that exists or install it; nothing is downloaded automatically".into()),
                        )
                    });
                }
            }
            // Geometry and duration sanity (only on latent sizes: a mask or a scale node may
            // legitimately use small numbers).
            if is_latent
                && matches!(key.as_str(), "width" | "height")
                && let Some(number) = value.as_u64()
                && !(64..=8192).contains(&number)
            {
                issues.push(WorkflowIssue::warning(
                    Some(id.clone()),
                    Some(class.to_string()),
                    format!("{key} {number} is outside 64–8192 px"),
                    Some("the panels derive the size from the sequence".into()),
                ));
            }
            if is_latent
                && matches!(key.as_str(), "width" | "height")
                && let Some(number) = value.as_u64()
                && number % 8 != 0
            {
                issues.push(WorkflowIssue::info(
                    Some(id.clone()),
                    Some(class.to_string()),
                    format!("{key} {number} is not a multiple of 8"),
                    Some("generated sizes round to 8/16 px".into()),
                ));
            }
            if matches!(key.as_str(), "length" | "duration" | "seconds" | "max_duration")
                && let Some(number) = value.as_f64()
                && (!number.is_finite() || number <= 0.0 || number > 3600.0)
            {
                issues.push(WorkflowIssue::warning(Some(id.clone()), Some(class.to_string()), format!("{key} {number} is not a usable duration"), None));
            }
        }
        // Known wiring defects that make ComfyUI abort: the AV splitter needs its combined latent
        // (or an audio latent) actually connected. `av_latent` is the input the LTX nodes use.
        if class.eq_ignore_ascii_case("LTXVSeparateAVLatent") {
            let wired = ["av_latent", "audio_latent", "samples"].iter().any(|key| inputs.get(*key).is_some_and(|value| link_target(value).is_some()));
            if !wired {
                issues.push(WorkflowIssue::error(
                    Some(id.clone()),
                    Some(class.to_string()),
                    "LTXVSeparateAVLatent has no connected latent: ComfyUI aborts with an index error",
                    Some("connect av_latent/audio_latent or remove the node".into()),
                ));
            }
        }
        if class.to_ascii_lowercase().contains("imagescaleby")
            && let Some(scale) = inputs.get("scale_by").and_then(Value::as_f64)
            && scale < 0.99
        {
            issues.push(WorkflowIssue::warning(
                Some(id.clone()),
                Some(class.to_string()),
                format!("image downscale by {scale} halves the output"),
                Some("preview-only nodes should keep scale_by at 1.0".into()),
            ));
        }
    }
    issues
}

/// A link value `["12", 0]`.
fn link_target(value: &Value) -> Option<(String, usize)> {
    let pair = value.as_array()?;
    if pair.len() != 2 {
        return None;
    }
    let id = pair[0].as_str()?.to_string();
    let index = pair[1].as_u64()? as usize;
    Some((id, index))
}

/// ComfyUI enum entries use `/`; workflow JSON written on Windows may use `\`.
fn normalize_separators(text: &str) -> String {
    text.replace('\\', "/")
}

fn is_prompt_slot(key: &str, class: &str) -> bool {
    let key = key.to_ascii_lowercase();
    let class = class.to_ascii_lowercase();
    (class.contains("textencode") || class.contains("text_encode") || class.contains("prompt"))
        && (key.contains("text") || key.contains("prompt"))
        && !key.contains("negative")
}

fn is_reference_key(key: &str) -> bool {
    matches!(key, "image" | "video" | "audio" | "file" | "filename")
}

/// The enum of a widget input from `/object_info`, when the server publishes one.
fn enum_choices<'a>(object_info: Option<&'a Value>, class: &str, key: &str) -> Option<Vec<&'a str>> {
    let info = object_info?.get(class)?;
    for section in ["required", "optional"] {
        if let Some(spec) = info.get("input").and_then(|i| i.get(section)).and_then(|s| s.get(key))
            && let Some(values) = spec.get(0).and_then(Value::as_array)
        {
            let strings: Vec<&str> = values.iter().filter_map(Value::as_str).collect();
            if !strings.is_empty() && strings.len() == values.len() {
                return Some(strings);
            }
        }
    }
    None
}

/// The real widgets a workflow exposes, resolved against ComfyUI's `/object_info`: current value,
/// kind, choices, ranges. This is what the Prompter's parameter panel reads, reduced to a JSON
/// description the GUI can draw and an agent can inspect.
pub fn parameters(workflow: &Value, object_info: &Value, limit: usize) -> Value {
    let mut rows = Vec::new();
    let nodes = workflow.as_object().cloned().unwrap_or_default();
    let mut ids: Vec<String> = nodes.keys().cloned().collect();
    ids.sort_by(|a, b| match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        _ => a.cmp(b),
    });
    let mut missing_classes = Vec::new();
    for id in ids {
        let Some(node) = nodes.get(&id) else { continue };
        let class = node.get("class_type").and_then(Value::as_str).unwrap_or_default().to_string();
        let title = node.get("_meta").and_then(|m| m.get("title")).and_then(Value::as_str).unwrap_or_default().to_string();
        let Some(info) = object_info.get(&class) else {
            missing_classes.push(class.clone());
            continue;
        };
        let inputs = node.get("inputs").and_then(Value::as_object);
        for section in ["required", "optional"] {
            let Some(specs) = info.get("input").and_then(|i| i.get(section)).and_then(Value::as_object) else { continue };
            for (field, spec) in specs {
                let value = inputs.and_then(|inputs| inputs.get(field)).cloned();
                if value.as_ref().is_some_and(|value| value.as_array().is_some_and(|a| a.len() == 2 && a[0].is_string() && a[1].is_u64())) {
                    rows.push(json!({"node": id, "class": class, "title": title, "field": field, "linked": true}));
                    continue;
                }
                let (kind, choices, min, max) = describe_spec(spec);
                let mut row = json!({
                    "node": id, "class": class, "title": title, "field": field,
                    "value": value, "kind": kind, "linked": false,
                });
                if let Some(choices) = choices {
                    row["choices"] = json!(choices);
                }
                if let Some(min) = min {
                    row["min"] = min;
                }
                if let Some(max) = max {
                    row["max"] = max;
                }
                rows.push(row);
            }
        }
    }
    let shown = rows.len().min(limit);
    json!({
        "parameters": rows[..shown].to_vec(),
        "total": rows.len(),
        "truncated": rows.len() > shown,
        "missingClasses": missing_classes,
    })
}

/// Reads a `/object_info` input spec: kind name, enum choices, numeric bounds.
fn describe_spec(spec: &Value) -> (&'static str, Option<Vec<Value>>, Option<Value>, Option<Value>) {
    let Some(list) = spec.as_array() else { return ("unknown", None, None, None) };
    let options = list.get(1).cloned().unwrap_or(Value::Null);
    let bounds = || (options.get("min").cloned().filter(|v| !v.is_null()), options.get("max").cloned().filter(|v| !v.is_null()));
    match list.first() {
        Some(Value::Array(choices)) => ("choice", Some(choices.clone()), None, None),
        Some(Value::String(kind)) => match kind.as_str() {
            "INT" => {
                let (min, max) = bounds();
                ("int", None, min, max)
            }
            "FLOAT" => {
                let (min, max) = bounds();
                ("float", None, min, max)
            }
            "BOOLEAN" => ("bool", None, None, None),
            "STRING" => ("text", None, None, None),
            _ => ("input", None, None, None),
        },
        _ => ("unknown", None, None, None),
    }
}

/// Fetch ComfyUI's `/object_info` (read-only) and check every workflow against it.
pub fn verify(s: &Session, object_info: &Value) -> Value {
    let Some(root) = super::models::root(s) else {
        return json!({"reachable": true, "classes": object_info.as_object().map_or(0, serde_json::Map::len), "workflows": [], "error": "the workflow folder was not found"});
    };
    let classes: std::collections::HashSet<&str> = object_info.as_object().into_iter().flat_map(|m| m.keys()).map(String::as_str).collect();
    let mut rows = Vec::new();
    let mut files = Vec::new();
    collect_json(&root, &root, 0, &mut files);
    for relative in files {
        let path = root.join(&relative);
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(workflow) = serde_json::from_str::<Value>(&text) else {
            rows.push(json!({"path": relative.to_string_lossy().replace('\\', "/"), "ok": false, "error": "not JSON"}));
            continue;
        };
        let workflow_classes: std::collections::BTreeSet<&str> =
            workflow.as_object().into_iter().flat_map(|nodes| nodes.values()).filter_map(|n| n.get("class_type")).filter_map(Value::as_str).collect();
        let missing: Vec<&str> = workflow_classes.iter().copied().filter(|c| !classes.contains(c)).collect();
        rows.push(json!({
            "path": relative.to_string_lossy().replace('\\', "/"),
            "ok": missing.is_empty(),
            "missing": missing,
        }));
    }
    let missing_workflows = rows.iter().filter(|r| r["ok"] == false).count();
    json!({"reachable": true, "classes": classes.len(), "workflows": rows, "missingWorkflows": missing_workflows})
}

fn collect_json(root: &Path, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else { continue };
        if metadata.is_dir() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if super::models::is_skipped(&name) {
                continue;
            }
            collect_json(root, &path, depth + 1, out);
        } else if metadata.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
            out.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(class: &str, inputs: Value) -> Value {
        json!({"class_type": class, "inputs": inputs})
    }

    #[test]
    fn a_link_to_a_missing_node_is_an_error() {
        let workflow = json!({"1": node("KSampler", json!({"model": ["9", 0]}))});
        let issues = diagnose(&workflow, None);
        assert!(issues.iter().any(|i| i.severity == "error" && i.message.contains("node 9")), "{issues:?}");
    }

    #[test]
    fn an_installed_class_and_model_pass() {
        let workflow = json!({"1": node("CheckpointLoaderSimple", json!({"ckpt_name": "flux.safetensors"}))});
        let info =
            json!({"CheckpointLoaderSimple": {"input": {"required": {"ckpt_name": [["flux.safetensors", "other.safetensors"], {}]}}, "output": ["MODEL"]}});
        let issues = diagnose(&workflow, Some(&info));
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn a_model_that_is_not_on_the_server_is_an_error() {
        let workflow = json!({"1": node("CheckpointLoaderSimple", json!({"ckpt_name": "missing.safetensors"}))});
        let info = json!({"CheckpointLoaderSimple": {"input": {"required": {"ckpt_name": [["other.safetensors"], {}]}}, "output": ["MODEL"]}});
        let issues = diagnose(&workflow, Some(&info));
        assert!(issues.iter().any(|i| i.severity == "error" && i.message.contains("missing.safetensors")), "{issues:?}");
    }

    #[test]
    fn a_class_that_is_not_installed_is_an_error() {
        let workflow = json!({"1": node("SomeCustomNode", json!({}))});
        let info = json!({});
        let issues = diagnose(&workflow, Some(&info));
        assert!(issues.iter().any(|i| i.message.contains("SomeCustomNode")), "{issues:?}");
    }

    #[test]
    fn the_ltx_audio_split_and_empty_prompts_are_reported() {
        let workflow = json!({
            "1": node("LTXVSeparateAVLatent", json!({})),
            "2": node("CLIPTextEncode", json!({"text": "  "}))
        });
        let issues = diagnose(&workflow, None);
        assert!(issues.iter().any(|i| i.message.contains("no connected latent")), "{issues:?}");
        assert!(issues.iter().any(|i| i.message.contains("empty")), "{issues:?}");
    }

    #[test]
    fn parameters_resolve_values_choices_bounds_and_links() {
        let workflow = json!({
            "1": {"class_type": "KSampler", "inputs": {"seed": 42, "model": ["2", 0], "sampler_name": "euler", "steps": 20}},
            "2": {"class_type": "CheckpointLoaderSimple", "inputs": {"ckpt_name": "a.safetensors"}}
        });
        let info = json!({
            "KSampler": {"input": {"required": {
                "seed": ["INT", {"min": 0, "max": 100}],
                "model": [["MODEL"]],
                "sampler_name": [["euler", "dpmpp_2m"]],
                "steps": ["INT", {"min": 1, "max": 100}]
            }}},
            "CheckpointLoaderSimple": {"input": {"required": {"ckpt_name": [["a.safetensors", "b.safetensors"]]}}}
        });
        let report = parameters(&workflow, &info, 100);
        let rows = report["parameters"].as_array().unwrap();
        let row = |field: &str| rows.iter().find(|row| row["field"] == field).cloned().unwrap();
        assert_eq!(row("seed")["kind"], "int");
        assert_eq!(row("seed")["value"], 42);
        assert_eq!(row("seed")["max"], 100);
        assert_eq!(row("model")["linked"], true);
        assert_eq!(row("sampler_name")["kind"], "choice");
        assert_eq!(row("sampler_name")["choices"], json!(["euler", "dpmpp_2m"]));
        assert_eq!(row("ckpt_name")["kind"], "choice");
        assert!(report["missingClasses"].as_array().unwrap().is_empty());
    }

    #[test]
    fn parameters_report_classes_that_are_not_installed() {
        let workflow = json!({"1": {"class_type": "MissingNode", "inputs": {}}});
        let report = parameters(&workflow, &json!({}), 100);
        assert_eq!(report["missingClasses"], json!(["MissingNode"]));
        assert!(report["parameters"].as_array().unwrap().is_empty());
    }

    #[test]
    fn the_repair_rounds_latent_sizes_and_leaves_the_rest_alone() {
        let workflow = json!({"1": node("EmptyLatentImage", json!({"width": 1000, "height": 1080, "batch_size": 1}))});
        let (fixed, changes) = repair(&workflow);
        assert_eq!(fixed["1"]["inputs"]["width"], 1008);
        assert_eq!(fixed["1"]["inputs"]["height"], 1088, "1080 is on the 8 grid but not the 16 grid");
        assert_eq!(fixed["1"]["inputs"]["batch_size"], 1);
        assert_eq!(changes.len(), 2, "{changes:?}");
        assert_eq!(workflow["1"]["inputs"]["width"], 1000, "the original is untouched");
    }

    #[test]
    fn a_prompt_input_that_is_a_link_is_not_reported_as_empty() {
        let workflow = json!({"1": node("CLIPTextEncode", json!({"text": ["2", 0]})), "2": node("PrimitiveString", json!({}))});
        let issues = diagnose(&workflow, None);
        assert!(!issues.iter().any(|i| i.message.contains("empty")), "{issues:?}");
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    fn node(class: &str, inputs: Value) -> Value {
        json!({"class_type": class, "inputs": inputs})
    }

    #[test]
    fn a_windows_style_model_path_matches_the_servers_forward_slash_enum() {
        let workflow = json!({"1": node("UNETLoader", json!({"unet_name": "minimax-h3\\model.safetensors"}))});
        let info = json!({"UNETLoader": {"input": {"required": {"unet_name": [["minimax-h3/model.safetensors"], {}]}}, "output": ["MODEL"]}});
        assert!(diagnose(&workflow, Some(&info)).is_empty());
    }

    #[test]
    fn a_stale_reference_is_a_warning_because_the_panel_supplies_its_own() {
        let workflow = json!({"1": node("LoadImage", json!({"image": "old.png"}))});
        let info = json!({"LoadImage": {"input": {"required": {"image": [["new.png"], {}]}}, "output": ["IMAGE"]}});
        let issues = diagnose(&workflow, Some(&info));
        assert!(issues.iter().all(|i| i.severity == "warning"), "{issues:?}");
        assert!(issues.iter().any(|i| i.message.contains("old.png")), "{issues:?}");
    }
}

#[cfg(test)]
mod marker_tests {
    use super::*;

    #[test]
    fn a_ui_marker_node_is_a_note_not_an_error() {
        let workflow = serde_json::json!({
            "1": {"inputs": {}, "_meta": {"title": "Control Panel"}},
            "2": {"class_type": "SaveImage", "inputs": {}}
        });
        let issues = diagnose(&workflow, None);
        assert!(issues.iter().all(|i| i.severity != "error"), "{issues:?}");
        assert!(issues.iter().any(|i| i.severity == "info"), "{issues:?}");
    }
}
