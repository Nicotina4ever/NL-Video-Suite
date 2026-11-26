//! Explicit batch of NL commands: enqueue work while editing, then start it once. The variation
//! queue (`nl.queue`) is untouched; this is a separate list for internal functions and ComfyUI
//! prompts.

use serde_json::{Value, json};

use crate::{EngineError, Result, Session};

fn bad(msg: impl ToString) -> EngineError {
    EngineError::BadParams { cmd: "nl.batch".into(), msg: msg.to_string() }
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    let command = p.get("command").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).ok_or_else(|| bad("serve `command`"))?.to_string();
    let params = p.get("params").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    let label = p.get("label").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).unwrap_or(&command).to_string();
    let id = s.nl.batch_next.max(1);
    s.nl.batch_next = id + 1;
    s.nl.batch.push(json!({"id": id, "command": command, "params": params, "label": label, "status": "pending"}));
    Ok(json!({"batch": s.nl.batch, "id": id}))
}

fn list(s: &mut Session, _p: &Value) -> Result<Value> {
    Ok(json!({"batch": s.nl.batch, "pending": s.nl.batch.iter().filter(|entry| entry["status"] == "pending").count()}))
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    let id = p.get("id").and_then(Value::as_u64).ok_or_else(|| bad("serve `id`"))?;
    s.nl.batch.retain(|entry| entry["id"].as_u64() != Some(id));
    Ok(json!({"batch": s.nl.batch}))
}

fn clear(s: &mut Session, _p: &Value) -> Result<Value> {
    s.nl.batch.clear();
    Ok(json!({"batch": []}))
}

fn start(s: &mut Session, p: &Value) -> Result<Value> {
    let stop_on_error = p.get("stopOnError").and_then(Value::as_bool).unwrap_or(false);
    let entries = s.nl.batch.clone();
    let mut started = 0_u64;
    let mut failed = 0_u64;
    let mut results = Vec::new();
    for index in 0..entries.len() {
        if s.nl.batch.get(index).and_then(|entry| entry["status"].as_str()) != Some("pending") {
            continue;
        }
        let command = s.nl.batch[index]["command"].as_str().unwrap_or_default().to_string();
        let mut params = s.nl.batch[index]["params"].clone();
        // ComfyUI commands are submitted, not waited one by one: ComfyUI owns their queue.
        if command == "graphics.generateComfy" || command.starts_with("nl.bridge.") || command.starts_with("nl.agent.") || command == "graphics.upscaleVideo" {
            if let Some(object) = params.as_object_mut() {
                object.entry("wait").or_insert(json!(false));
            }
        }
        match s.execute(&command, params) {
            Ok(value) => {
                started += 1;
                s.nl.batch[index]["status"] = json!("queued");
                s.nl.batch[index]["result"] = value.clone();
                results.push(json!({"id": s.nl.batch[index]["id"], "command": command, "ok": true}));
            }
            Err(error) => {
                failed += 1;
                s.nl.batch[index]["status"] = json!("failed");
                s.nl.batch[index]["error"] = json!(error.to_string());
                results.push(json!({"id": s.nl.batch[index]["id"], "command": command, "ok": false, "error": error.to_string()}));
                if stop_on_error {
                    break;
                }
            }
        }
    }
    Ok(json!({"started": started, "failed": failed, "results": results, "batch": s.nl.batch}))
}

pub(crate) fn commands() -> Vec<crate::CommandSpec> {
    let spec = |id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>| crate::CommandSpec {
        id,
        label,
        menu: &[],
        shortcut: None,
        params,
        enabled: crate::commands::always,
        run,
        journal: false,
        named: crate::commands::NamedKeys::NONE,
    };
    vec![
        spec("nl.batch.add", "NL Batch Add", r#"{"command":str,"params":obj?,"label":str?}"#, add),
        spec("nl.batch.list", "NL Batch List", r#"{}"#, list),
        spec("nl.batch.remove", "NL Batch Remove", r#"{"id":u64}"#, remove),
        spec("nl.batch.clear", "NL Batch Clear", r#"{}"#, clear),
        spec("nl.batch.start", "NL Batch Start", r#"{"stopOnError":bool?}"#, start),
    ]
}
