//! The variation queue: deterministic seeds, a prepared payload shown before sending, an explicit
//! confirmation before anything reaches ComfyUI, and a controlled way to free VRAM.
//!
//! Nothing here talks to ComfyUI until `send` with `confirm: true`; `prepare` builds exactly the
//! payload that would be sent (through the same dry run the Generate panel uses) so the operator
//! sees the prompt, size, references and overrides before committing a long generation.

use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{EngineError, Result, Session};

use super::http::{self, Method};

const MAX_QUEUE: usize = 64;
const MAX_PROMPT: usize = 4000;

fn bad_cmd(cmd: &str, msg: impl ToString) -> EngineError {
    bad(cmd, msg.to_string())
}

/// A stable seed when the operator does not give one: recorded in the entry, so a variant can be
/// reproduced later.
fn fresh_seed() -> u64 {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|elapsed| elapsed.as_nanos()).unwrap_or(0);
    ((nanos ^ (nanos >> 32)) & 0xFFFF_FFFF) as u64
}

/// The parameters a prepared/sent entry hands to `graphics.generateComfy`.
fn generation_params(entry: &Value) -> Value {
    let mut params = json!({
        "prompt": entry["prompt"],
        "seed": entry["seed"],
        "kind": entry["kind"],
        "resolutionMode": entry["kind"],
    });
    if let Some(workflow) = entry["workflow"].as_str().filter(|workflow| !workflow.is_empty()) {
        params["workflow"] = json!(workflow);
    }
    if let Some(overrides) = entry["overrides"].as_array().filter(|overrides| !overrides.is_empty()) {
        params["overrides"] = json!(overrides);
    }
    params
}

/// `nl.queue.add` — enqueue variants with deterministic seeds; duplicates are reported, not added.
fn queue_add(s: &mut Session, p: &Value) -> Result<Value> {
    let prompt = p
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .ok_or_else(|| bad_cmd("nl.queue.add", "need `prompt`"))?
        .to_string();
    if prompt.chars().count() > MAX_PROMPT {
        return Err(bad_cmd("nl.queue.add", format!("`prompt` is longer than {MAX_PROMPT} characters")));
    }
    let kind = p.get("kind").and_then(Value::as_str).unwrap_or("image").to_string();
    if !matches!(kind.as_str(), "image" | "video" | "audio") {
        return Err(bad_cmd("nl.queue.add", "`kind` must be image, video or audio"));
    }
    let workflow = p.get("workflow").and_then(Value::as_str).unwrap_or_default().trim().to_string();
    let overrides = p.get("overrides").cloned().unwrap_or(Value::Null);
    if !overrides.is_null() && !overrides.is_array() {
        return Err(bad_cmd("nl.queue.add", "`overrides` must be an array of {node, field, value}"));
    }
    let count = p.get("count").and_then(Value::as_u64).unwrap_or(1).clamp(1, 32) as usize;
    let seeds: Vec<u64> = if let Some(list) = p.get("seeds").and_then(Value::as_array) {
        list.iter().take(32).filter_map(Value::as_u64).collect()
    } else {
        let base = p.get("seed").and_then(Value::as_u64).unwrap_or_else(fresh_seed);
        (0..count as u64).map(|offset| base.wrapping_add(offset)).collect()
    };
    if seeds.is_empty() {
        return Err(bad_cmd("nl.queue.add", "no seeds to use"));
    }
    let mut added = Vec::new();
    let mut duplicates = 0;
    for seed in seeds {
        if s.nl.queue.iter().any(|entry| {
            entry["prompt"].as_str() == Some(prompt.as_str())
                && entry["kind"].as_str() == Some(kind.as_str())
                && entry["workflow"].as_str().unwrap_or_default() == workflow
                && entry["seed"].as_u64() == Some(seed)
        }) {
            duplicates += 1;
            continue;
        }
        if s.nl.queue.len() >= MAX_QUEUE {
            return Err(bad_cmd("nl.queue.add", format!("the queue is full ({MAX_QUEUE} entries): remove some first")));
        }
        let id = s.nl.queue_next.max(1);
        s.nl.queue_next = id + 1;
        let entry = json!({
            "id": id,
            "prompt": prompt,
            "kind": kind,
            "workflow": workflow,
            "seed": seed,
            "overrides": overrides,
            "status": "new",
            "prepared": Value::Null,
            "job": Value::Null,
            "error": Value::Null,
        });
        s.nl.queue.push(entry);
        added.push(json!({"id": id, "seed": seed}));
    }
    Ok(json!({"added": added, "duplicates": duplicates, "queued": s.nl.queue.len()}))
}

/// `nl.queue.list` — the queue as the panel shows it.
fn queue_list(s: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!({"entries": s.nl.queue.clone(), "queued": s.nl.queue.len()}))
}

/// `nl.queue.prepare` — build the exact payload of every new entry without sending it.
fn queue_prepare(s: &mut Session, p: &Value) -> Result<Value> {
    let id = p.get("id").and_then(Value::as_u64);
    let mut prepared = 0;
    let mut errors = Vec::new();
    let mut finished = Vec::new();
    for index in 0..s.nl.queue.len() {
        let entry_id = s.nl.queue[index]["id"].as_u64().unwrap_or(0);
        if id.is_some_and(|id| id != entry_id) || s.nl.queue[index]["status"].as_str() != Some("new") {
            continue;
        }
        let mut params = generation_params(&s.nl.queue[index]);
        params["dryRun"] = json!(true);
        match s.execute("graphics.generateComfy", params) {
            Ok(value) => {
                s.nl.queue[index]["prepared"] = value;
                s.nl.queue[index]["status"] = json!("prepared");
                prepared += 1;
                finished.push(entry_id);
            }
            Err(e) => {
                let message = e.to_string();
                s.nl.queue[index]["status"] = json!("error");
                s.nl.queue[index]["error"] = json!(message);
                errors.push(json!({"id": entry_id, "error": message}));
            }
        }
    }
    Ok(json!({"prepared": prepared, "entries": finished, "errors": errors}))
}

/// `nl.queue.send` — send prepared entries; refuses without `confirm: true`.
fn queue_send(s: &mut Session, p: &Value) -> Result<Value> {
    let confirm = p.get("confirm").and_then(Value::as_bool).unwrap_or(false);
    if !confirm {
        return Err(bad_cmd("nl.queue.send", "nothing is sent without `confirm: true`: prepare first, check the payloads, then confirm"));
    }
    let id = p.get("id").and_then(Value::as_u64);
    let mut sent = 0;
    let mut skipped = 0;
    let mut errors = Vec::new();
    for index in 0..s.nl.queue.len() {
        let entry_id = s.nl.queue[index]["id"].as_u64().unwrap_or(0);
        if id.is_some_and(|id| id != entry_id) {
            continue;
        }
        let status = s.nl.queue[index]["status"].as_str().unwrap_or("new");
        if status != "prepared" {
            if id.is_none() {
                skipped += 1;
            }
            continue;
        }
        let mut params = generation_params(&s.nl.queue[index]);
        if let Some(wait) = p.get("wait").and_then(Value::as_bool) {
            params["wait"] = json!(wait);
        }
        if let Some(timeout) = p.get("timeoutSeconds").and_then(Value::as_u64) {
            params["timeoutSeconds"] = json!(timeout);
        }
        match s.execute("graphics.generateComfy", params) {
            Ok(value) => {
                s.nl.queue[index]["status"] = json!("sent");
                s.nl.queue[index]["job"] = value["job"].clone();
                sent += 1;
            }
            Err(e) => {
                let message = e.to_string();
                s.nl.queue[index]["status"] = json!("error");
                s.nl.queue[index]["error"] = json!(message);
                errors.push(json!({"id": entry_id, "error": message}));
            }
        }
    }
    Ok(json!({"sent": sent, "skipped": skipped, "errors": errors}))
}

/// `nl.queue.remove` — drop one entry, or the finished ones with `finished: true`.
fn queue_remove(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(id) = p.get("id").and_then(Value::as_u64) {
        let before = s.nl.queue.len();
        s.nl.queue.retain(|entry| entry["id"].as_u64() != Some(id));
        return Ok(json!({"removed": before - s.nl.queue.len()}));
    }
    let finished = p.get("finished").and_then(Value::as_bool).unwrap_or(false);
    if !finished {
        return Err(bad_cmd("nl.queue.remove", "pass `id` or `finished: true`"));
    }
    let before = s.nl.queue.len();
    s.nl.queue.retain(|entry| !matches!(entry["status"].as_str(), Some("sent" | "error")));
    Ok(json!({"removed": before - s.nl.queue.len()}))
}

/// `nl.queue.free` — ask ComfyUI to unload models and free memory; only loopback, only on confirm.
fn queue_free(s: &mut Session, p: &Value) -> Result<Value> {
    if !p.get("confirm").and_then(Value::as_bool).unwrap_or(false) {
        return Err(bad_cmd("nl.queue.free", "freeing ComfyUI's VRAM needs `confirm: true`"));
    }
    let base = s.prefs.nl.comfy_url();
    let host = base.trim_start_matches("http://").split([':', '/']).next().unwrap_or_default();
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
        return Err(bad_cmd("nl.queue.free", format!("only a loopback ComfyUI can be freed (this one is {base})")));
    }
    let body = json!({"unload_models": true, "free_memory": true}).to_string();
    let response = http::request(Method::Post, &format!("{}/free", base.trim_end_matches('/')), Some(&body), std::time::Duration::from_secs(60))
        .map_err(|e| bad_cmd("nl.queue.free", e))?;
    let text = String::from_utf8_lossy(&response).trim().to_string();
    Ok(json!({"freed": true, "url": base, "response": text.chars().take(200).collect::<String>()}))
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    let spec = |id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>| -> CommandSpec {
        CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: always, run, journal: false, named: crate::commands::NamedKeys::NONE }
    };
    vec![
        spec(
            "nl.queue.add",
            "NL Queue Variants",
            r#"{"prompt":str,"kind":"image|video|audio"?,"workflow":str?,"count":int?,"seed":int?,"seeds":[int]?,"overrides":[obj]?}"#,
            queue_add,
        ),
        spec("nl.queue.list", "NL Queue", "{}", queue_list),
        spec("nl.queue.prepare", "NL Prepare Variants", r#"{"id":int?}"#, queue_prepare),
        spec("nl.queue.send", "NL Send Variants", r#"{"id":int?,"confirm":bool,"wait":bool?,"timeoutSeconds":int?}"#, queue_send),
        spec("nl.queue.remove", "NL Remove Variants", r#"{"id":int?,"finished":bool?}"#, queue_remove),
        spec("nl.queue.free", "NL Free ComfyUI VRAM", r#"{"confirm":bool}"#, queue_free),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_get_deterministic_seeds_and_duplicates_are_skipped() {
        let mut s = Session::default();
        let added = s.execute("nl.queue.add", json!({"prompt": "a studio", "count": 3, "seed": 100})).unwrap();
        assert_eq!(added["added"].as_array().unwrap().len(), 3);
        let seeds: Vec<u64> = s.nl.queue.iter().filter_map(|entry| entry["seed"].as_u64()).collect();
        assert_eq!(seeds, vec![100, 101, 102]);
        let again = s.execute("nl.queue.add", json!({"prompt": "a studio", "count": 2, "seed": 101})).unwrap();
        assert_eq!(again["added"].as_array().unwrap().len(), 0);
        assert_eq!(again["duplicates"], 2);
        assert_eq!(s.nl.queue.len(), 3);
    }

    #[test]
    fn sending_without_confirm_is_refused_and_prepare_builds_payloads() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        s.execute("nl.queue.add", json!({"prompt": "a red apple", "count": 1, "seed": 7})).unwrap();
        // no confirmation: nothing is sent
        assert!(s.execute("nl.queue.send", json!({})).is_err());
        let prepared = s.execute("nl.queue.prepare", json!({})).unwrap();
        assert_eq!(prepared["prepared"], 1);
        assert_eq!(s.nl.queue[0]["status"], "prepared");
        assert_eq!(s.nl.queue[0]["prepared"]["submitted"], false);
        assert!(s.nl.queue[0]["prepared"]["prompt"].as_str().unwrap().contains("a red apple"));
        // an unknown id prepares nothing, it does not send
        let none = s.execute("nl.queue.prepare", json!({"id": 99})).unwrap();
        assert_eq!(none["prepared"], 0);
    }

    #[test]
    fn removing_finished_entries_keeps_the_rest() {
        let mut s = Session::default();
        s.execute("nl.queue.add", json!({"prompt": "one", "count": 2, "seed": 1})).unwrap();
        s.nl.queue[0]["status"] = json!("sent");
        let removed = s.execute("nl.queue.remove", json!({"finished": true})).unwrap();
        assert_eq!(removed["removed"], 1);
        assert_eq!(s.nl.queue.len(), 1);
        assert_eq!(s.nl.queue[0]["status"], "new");
        assert!(s.execute("nl.queue.remove", json!({})).is_err());
    }

    #[test]
    fn freeing_needs_confirmation() {
        let mut s = Session::default();
        assert!(s.execute("nl.queue.free", json!({})).is_err());
    }
}
