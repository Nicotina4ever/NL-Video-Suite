//! Edit versions: name a cut, keep it as a snapshot of the active sequence and come back to it
//! later. A restore saves the current state first, so trying a version cannot lose work.
//!
//! Snapshots live beside a saved project (`nl/versions/`), or in the suite data folder while the
//! project is unsaved. They are the sequence only: media stays in the project.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{EngineError, Result, Session};

const MAX_NAME: usize = 120;

fn bad_cmd(cmd: &str, msg: impl ToString) -> EngineError {
    bad(cmd, msg.to_string())
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or(0)
}

/// Where this project's versions live.
fn version_dir(s: &Session) -> Result<PathBuf> {
    // Versions live beside the project: sharing one `unsaved` folder between temporary projects
    // would let a version be restored into the wrong project, so a version needs a saved project.
    let project = s
        .path
        .as_deref()
        .map(std::path::Path::new)
        .ok_or_else(|| bad_cmd("nl.versions", "save the project first: edit versions are kept in nl/versions next to the project file"))?;
    let parent = project.parent().ok_or_else(|| bad_cmd("nl.versions", "the project has no folder"))?;
    Ok(parent.join("nl").join("versions"))
}

fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .take(MAX_NAME)
        .map(|character| if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_') { character } else { '_' })
        .collect();
    if cleaned.is_empty() { "Version".into() } else { cleaned }
}

/// A stored entry as the panel shows it (without the sequence itself).
fn entry_summary(file: &std::path::Path, value: &Value) -> Value {
    let sequence = &value["sequence"];
    let clips: usize = ["video_tracks", "audio_tracks"]
        .iter()
        .map(|key| sequence[key].as_array().map(|tracks| tracks.iter().map(|track| track["items"].as_array().map_or(0, Vec::len)).sum::<usize>()).unwrap_or(0))
        .sum();
    json!({
        "id": value["id"],
        "name": value["name"],
        "created": value["created"],
        "file": file.file_name().map(|name| name.to_string_lossy().to_string()),
        "clips": clips,
        "durationSeconds": value["durationSeconds"],
    })
}

/// `nl.versions.save` — snapshot the active sequence under a name.
fn versions_save(s: &mut Session, p: &Value) -> Result<Value> {
    let name = sanitize_name(p.get("name").and_then(Value::as_str).unwrap_or("Version"));
    let active_id = s.state.active_sequence.ok_or(EngineError::NoSequence)?;
    let sequence = s.project.sequence(active_id).ok_or(EngineError::NoSequence)?.clone();
    let dir = version_dir(s)?;
    std::fs::create_dir_all(&dir).map_err(|e| bad_cmd("nl.versions.save", format!("{}: {e}", dir.display())))?;
    let created = now_secs();
    let id = format!("{created}-{}", sanitize_name(&name).replace(' ', "-").to_lowercase());
    let value = json!({
        "id": id,
        "name": name,
        "created": created,
        "sequenceId": active_id.0,
        "durationSeconds": sequence.duration().seconds(),
        "sequence": serde_json::to_value(&sequence).unwrap_or(Value::Null),
    });
    let path = dir.join(format!("{id}.json"));
    std::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap_or_default())
        .map_err(|e| bad_cmd("nl.versions.save", format!("{}: {e}", path.display())))?;
    let clips: usize = sequence.video_tracks.iter().chain(sequence.audio_tracks.iter()).map(|track| track.items.len()).sum();
    Ok(json!({"id": id, "name": value["name"], "clips": clips, "durationSeconds": sequence.duration().seconds(), "path": path.to_string_lossy()}))
}

/// `nl.versions.list` — the saved versions, newest first.
fn versions_list(s: &mut Session, _: &Value) -> Result<Value> {
    let dir = version_dir(s)?;
    let mut entries = Vec::new();
    if let Ok(read) = std::fs::read_dir(&dir) {
        for file in read.flatten() {
            let path = file.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
            entries.push(entry_summary(&path, &value));
        }
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry["created"].as_u64().unwrap_or(0)));
    Ok(json!({"folder": dir.to_string_lossy(), "versions": entries, "count": entries.len()}))
}

/// `nl.versions.restore` — put a saved sequence back, saving the current one first.
fn versions_restore(s: &mut Session, p: &Value) -> Result<Value> {
    let id = p
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| bad_cmd("nl.versions.restore", "need `id` from nl.versions.list"))?;
    let dir = version_dir(s)?;
    let path = dir.join(format!("{id}.json"));
    let text = std::fs::read_to_string(&path).map_err(|e| bad_cmd("nl.versions.restore", format!("{}: {e}", path.display())))?;
    let value: Value = serde_json::from_str(&text).map_err(|e| bad_cmd("nl.versions.restore", format!("{id}: {e}")))?;
    let restored: filmcraft_project::Sequence =
        serde_json::from_value(value["sequence"].clone()).map_err(|e| bad_cmd("nl.versions.restore", format!("{id}: {e}")))?;
    let active = s.state.active_sequence.ok_or(EngineError::NoSequence)?;
    if let Some(stored) = value["sequenceId"].as_u64()
        && stored != active.0
    {
        return Err(bad_cmd("nl.versions.restore", "this version belongs to another sequence"));
    }
    // Keep the state we are about to leave.
    let _ = versions_save(s, &json!({"name": format!("Auto before restore ({id})")}));
    s.edit_sequence("Restore Edit Version", |sequence, _, _| {
        *sequence = restored.clone();
        Ok(())
    })?;
    let clips: usize = restored.video_tracks.iter().chain(restored.audio_tracks.iter()).map(|track| track.items.len()).sum();
    s.toast(format!("NL versions: restored `{}`", value["name"].as_str().unwrap_or(id)));
    Ok(json!({"id": id, "name": value["name"], "clips": clips, "durationSeconds": restored.duration().seconds()}))
}

/// `nl.versions.delete` — remove a saved version.
fn versions_delete(s: &mut Session, p: &Value) -> Result<Value> {
    let id = p.get("id").and_then(Value::as_str).map(str::trim).filter(|id| !id.is_empty()).ok_or_else(|| bad_cmd("nl.versions.delete", "need `id`"))?;
    let dir = version_dir(s)?;
    let path = dir.join(format!("{id}.json"));
    std::fs::remove_file(&path).map_err(|e| bad_cmd("nl.versions.delete", format!("{}: {e}", path.display())))?;
    Ok(json!({"deleted": id}))
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    let spec = |id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>| -> CommandSpec {
        CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: always, run, journal: false, named: crate::commands::NamedKeys::NONE }
    };
    vec![
        spec("nl.versions.save", "NL Save Edit Version", r#"{"name":str?}"#, versions_save),
        spec("nl.versions.list", "NL Edit Versions", "{}", versions_list),
        spec("nl.versions.restore", "NL Restore Edit Version", r#"{"id":str}"#, versions_restore),
        spec("nl.versions.delete", "NL Delete Edit Version", r#"{"id":str}"#, versions_delete),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_round_trips_through_save_list_restore_and_delete() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        // Edit versions live beside a saved project, so the test saves one first.
        let folder = std::env::temp_dir().join(format!("nl-versions-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        s.execute("file.saveAs", json!({"path": folder.join("test.fcproj").to_string_lossy()})).unwrap();
        let saved = s.execute("nl.versions.save", json!({"name": "Director's cut"})).unwrap();
        let id = saved["id"].as_str().unwrap().to_string();
        let list = s.execute("nl.versions.list", json!({})).unwrap();
        assert_eq!(list["versions"][0]["id"], id);
        // change the sequence, then restore the saved cut
        s.execute("sequence.settings", json!({"width": 1024})).unwrap();
        assert_eq!(s.active_sequence().unwrap().settings.width, 1024);
        s.execute("nl.versions.restore", json!({"id": id})).unwrap();
        assert_eq!(s.active_sequence().unwrap().settings.width, 1920);
        // restoring saved the state it left behind
        let list = s.execute("nl.versions.list", json!({})).unwrap();
        assert!(list["versions"].as_array().unwrap().len() >= 2, "{list:?}");
        s.execute("nl.versions.delete", json!({"id": id})).unwrap();
        let list = s.execute("nl.versions.list", json!({})).unwrap();
        assert!(list["versions"].as_array().unwrap().iter().all(|version| version["id"] != id));
        assert!(s.execute("nl.versions.restore", json!({"id": "nope"})).is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
