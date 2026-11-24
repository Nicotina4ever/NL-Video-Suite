//! Album of generated media: what ComfyUI produced for this suite, listed read-only and insertable
//! on the timeline again. Files outside the suite's generated folder are refused, so a path from a
//! project file cannot make the tool import something arbitrary.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use filmcraft_time::Tick;
use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{EngineError, Result, Session};

fn bad_cmd(cmd: &str, msg: impl ToString) -> EngineError {
    bad(cmd, msg.to_string())
}

const MEDIA_EXTENSIONS: [&str; 11] = ["png", "jpg", "jpeg", "webp", "mp4", "webm", "mov", "mp3", "wav", "flac", "ogg"];
const MAX_ENTRIES: usize = 200;

fn generated_dir() -> PathBuf {
    crate::autosave::default_data_dir().unwrap_or_else(std::env::temp_dir).join("generated")
}

fn kind_of(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("mp4" | "webm" | "mov") => "video",
        Some("mp3" | "wav" | "flac" | "ogg") => "audio",
        _ => "image",
    }
}

fn walk(folder: &Path, depth: usize, entries: &mut Vec<Value>) {
    if depth > 3 || entries.len() >= MAX_ENTRIES {
        return;
    }
    let Ok(read) = std::fs::read_dir(folder) else { return };
    for entry in read.flatten() {
        let Ok(metadata) = entry.metadata() else { continue };
        if metadata.is_dir() {
            walk(&entry.path(), depth + 1, entries);
            continue;
        }
        let path = entry.path();
        let Some(extension) = path.extension().and_then(|extension| extension.to_str()).map(str::to_ascii_lowercase) else { continue };
        if !MEDIA_EXTENSIONS.contains(&extension.as_str()) {
            continue;
        }
        let modified = metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map(|elapsed| elapsed.as_secs()).unwrap_or(0);
        entries.push(json!({
            "path": path.to_string_lossy(),
            "name": path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default(),
            "kind": kind_of(&path),
            "bytes": metadata.len(),
            "modified": modified,
        }));
        if entries.len() >= MAX_ENTRIES {
            return;
        }
    }
}

/// `nl.album.list` — generated images, videos and audio, newest first.
fn album_list(_: &mut Session, _: &Value) -> Result<Value> {
    let folder = generated_dir();
    let mut entries = Vec::new();
    walk(&folder, 0, &mut entries);
    entries.sort_by_key(|entry| std::cmp::Reverse(entry["modified"].as_u64().unwrap_or(0)));
    entries.truncate(MAX_ENTRIES);
    let bytes: u64 = entries.iter().map(|entry| entry["bytes"].as_u64().unwrap_or(0)).sum();
    Ok(json!({"folder": folder.to_string_lossy(), "entries": entries, "count": entries.len(), "bytes": bytes}))
}

/// `nl.album.insert` — import one generated file again and put it on the timeline.
fn album_insert(s: &mut Session, p: &Value) -> Result<Value> {
    let path = p
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| bad_cmd("nl.album.insert", "need `path` (a file from the album)"))?;
    let file = Path::new(path);
    let folder = generated_dir();
    let canonical_file = file.canonicalize().map_err(|e| bad_cmd("nl.album.insert", format!("{path}: {e}")))?;
    let canonical_folder = folder.canonicalize().map_err(|e| bad_cmd("nl.album.insert", format!("{}: {e}", folder.display())))?;
    if !canonical_file.starts_with(&canonical_folder) {
        return Err(bad_cmd("nl.album.insert", "only files from the suite's generated folder can be inserted from the album"));
    }
    if !canonical_file.is_file() {
        return Err(bad_cmd("nl.album.insert", "the album entry is not a file"));
    }
    let path = canonical_file.to_string_lossy().to_string();
    let item = crate::commands::import_streamed(s, &path, None)?;
    let kind = kind_of(&canonical_file);
    let mut params = json!({"item": item.0});
    if let Some(at) = p.get("atSeconds").and_then(Value::as_f64) {
        params["time"] = json!(Tick::from_seconds_f64(at).0);
    }
    match kind {
        "audio" => {
            params["audio"] = json!(true);
            params["video"] = json!(false);
            params["audioTrack"] = json!(p.get("track").and_then(Value::as_str).unwrap_or("A1"));
        }
        _ => {
            params["track"] = json!(p.get("track").and_then(Value::as_str).unwrap_or("V1"));
            let default_seconds = if kind == "image" { 5.0 } else { 0.0 };
            if let Some(seconds) =
                p.get("seconds").and_then(Value::as_f64).filter(|seconds| *seconds > 0.0).or((default_seconds > 0.0).then_some(default_seconds))
            {
                params["duration"] = json!(Tick::from_seconds_f64(seconds).0);
            }
        }
    }
    let placed = s.execute("timeline.place", params)?;
    Ok(json!({"item": item.0, "kind": kind, "placed": placed, "path": path}))
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    let spec = |id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>| -> CommandSpec {
        CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: always, run, journal: false, named: crate::commands::NamedKeys::NONE }
    };
    vec![
        spec("nl.album.list", "NL Album", "{}", album_list),
        spec("nl.album.insert", "NL Insert from Album", r#"{"path":str,"atSeconds":f?,"track":str?,"seconds":f?}"#, album_insert),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_album_refuses_files_outside_the_generated_folder() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        assert!(s.execute("nl.album.insert", json!({"path": "C:/Windows/notepad.exe"})).is_err());
        assert!(s.execute("nl.album.insert", json!({})).is_err());
    }

    #[test]
    fn the_album_lists_with_a_folder_and_a_count() {
        let mut s = Session::default();
        let report = s.execute("nl.album.list", json!({})).unwrap();
        assert!(report["folder"].as_str().unwrap().contains("generated"));
        assert!(report["entries"].as_array().is_some());
        assert_eq!(report["count"].as_u64(), Some(report["entries"].as_array().unwrap().len() as u64));
    }
}
