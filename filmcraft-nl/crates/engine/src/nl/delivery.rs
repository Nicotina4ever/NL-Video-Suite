//! Delivery checks and production package: a read-only QA pass over the sequence, a folder with the
//! project's deliverable text (captions, chapters, story, manifest, thumbnail) and a storage report.
//!
//! Nothing is uploaded and no media is moved: the package contains the small artifacts an editor
//! hands to a client or uploads by hand; generated media stays where ComfyUI wrote it and is listed
//! in the manifest.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{EngineError, Result, Session};

fn bad_cmd(cmd: &str, msg: impl ToString) -> EngineError {
    bad(cmd, msg.to_string())
}

/// Sizes of the data folder and of the project, without following anything outside them.
fn dir_size(path: &Path, depth: usize, files: &mut usize, bytes: &mut u64) {
    if depth > 6 || *files > 20_000 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else { return };
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else { continue };
        if metadata.is_dir() {
            dir_size(&entry.path(), depth + 1, files, bytes);
        } else {
            *files += 1;
            *bytes += metadata.len();
        }
    }
}

/// `nl.storage.status` — how much space the suite's data and this project take.
fn storage_status(s: &mut Session, _: &Value) -> Result<Value> {
    let data = crate::autosave::default_data_dir().unwrap_or_else(std::env::temp_dir);
    let mut data_bytes = 0;
    let mut data_files = 0;
    dir_size(&data, 0, &mut data_files, &mut data_bytes);
    let generated = data.join("generated");
    let mut generated_bytes = 0;
    let mut generated_files = 0;
    dir_size(&generated, 0, &mut generated_files, &mut generated_bytes);
    let project = s.path.as_ref().map(PathBuf::from);
    let mut project_bytes = 0;
    let mut project_files = 0;
    if let Some(path) = &project {
        if path.is_dir() {
            dir_size(path, 0, &mut project_files, &mut project_bytes);
        } else if let Ok(metadata) = std::fs::metadata(path) {
            project_files = 1;
            project_bytes = metadata.len();
        }
        if let Some(sidecar) = path.parent().map(|parent| parent.join("nl")) {
            dir_size(&sidecar, 0, &mut project_files, &mut project_bytes);
        }
    }
    Ok(json!({
        "dataDir": data.to_string_lossy(),
        "dataBytes": data_bytes,
        "dataFiles": data_files,
        "generatedBytes": generated_bytes,
        "generatedFiles": generated_files,
        "projectBytes": project_bytes,
        "projectFiles": project_files,
    }))
}

/// The QA checks: what is there and what would stop a delivery.
pub fn qa_checks(s: &Session) -> Vec<Value> {
    let mut checks = Vec::new();
    let mut push = |id: &str, status: &str, detail: String| checks.push(json!({"id": id, "status": status, "detail": detail}));
    let Some(sequence_id) = s.state.active_sequence else {
        push("sequence", "error", "No active sequence".into());
        return checks;
    };
    let Some(sequence) = s.project.sequence(sequence_id) else {
        push("sequence", "error", "The active sequence is missing".into());
        return checks;
    };
    let settings = &sequence.settings;
    let fps = settings.frame_rate.num as f64 / settings.frame_rate.den.max(1) as f64;
    push("sequence", "ok", format!("{} × {} @ {:.2} fps", settings.width, settings.height, fps));
    if settings.width >= 1280 && settings.height >= 720 {
        push("resolution", "ok", format!("{}×{} is at least 720p", settings.width, settings.height));
    } else {
        push("resolution", "warn", format!("{}×{} is below 720p", settings.width, settings.height));
    }
    let duration = sequence.duration().seconds();
    push("duration", "ok", format!("{duration:.1} s"));
    let cues: usize = sequence.caption_tracks.iter().map(|track| track.captions.len()).sum();
    if cues > 0 {
        push("captions", "ok", format!("{cues} cue(s) in {} track(s)", sequence.caption_tracks.len()));
    } else {
        push("captions", "warn", "No captions: add a subtitle track before delivering".into());
    }
    let audio_items: usize = sequence.audio_tracks.iter().map(|track| track.items.len()).sum();
    if audio_items > 0 {
        push("audio", "ok", format!("{audio_items} audio clip(s), master {:?}", settings.audio_master));
    } else {
        push("audio", "warn", "No audio clips".into());
    }
    let video_items: usize = sequence.video_tracks.iter().map(|track| track.items.len()).sum();
    push("video", if video_items > 0 { "ok" } else { "warn" }, format!("{video_items} video clip(s)"));
    let chapters = s.nl.longform.as_ref().and_then(|longform| longform["chapters"].as_array().map(Vec::len)).unwrap_or(0);
    push(
        "chapters",
        if chapters > 0 { "ok" } else { "info" },
        if chapters > 0 { format!("{chapters} chapter(s)") } else { "No longform chapters (optional)".into() },
    );
    let scenes = s.nl.story.as_ref().and_then(|story| story["scenes"].as_array().map(Vec::len)).unwrap_or(0);
    push("story", if scenes > 0 { "ok" } else { "info" }, if scenes > 0 { format!("{scenes} story scene(s)") } else { "No AI story (optional)".into() });
    let data = crate::autosave::default_data_dir().unwrap_or_else(std::env::temp_dir).join("generated");
    let mut files = 0;
    let mut bytes = 0;
    dir_size(&data, 0, &mut files, &mut bytes);
    push("generated", "info", format!("{files} generated file(s), {:.1} MB", bytes as f64 / (1024.0 * 1024.0)));
    checks
}

/// `nl.delivery.qa` — the checks in one report.
fn delivery_qa(s: &mut Session, _: &Value) -> Result<Value> {
    let checks = qa_checks(s);
    let errors = checks.iter().filter(|check| check["status"] == "error").count();
    let warnings = checks.iter().filter(|check| check["status"] == "warn").count();
    Ok(json!({"checks": checks, "errors": errors, "warnings": warnings, "ready": errors == 0}))
}

/// `nl.delivery.package` — a folder with manifest, captions, chapters, story, thumbnail and the
/// project file (never the media, which stays where it is).
fn delivery_package(s: &mut Session, p: &Value) -> Result<Value> {
    let path = p
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| bad_cmd("nl.delivery.package", "need `path`: the folder to create"))?;
    let folder = PathBuf::from(path);
    if !folder.is_absolute() {
        return Err(bad_cmd("nl.delivery.package", "`path` must be absolute"));
    }
    if folder.exists() {
        return Err(bad_cmd("nl.delivery.package", format!("{} already exists: choose a new folder", folder.display())));
    }
    std::fs::create_dir_all(&folder).map_err(|e| bad_cmd("nl.delivery.package", format!("{}: {e}", folder.display())))?;
    let mut files: Vec<String> = Vec::new();
    let checks = qa_checks(s);
    fn record(files: &mut Vec<String>, name: &str, created: bool) {
        if created {
            files.push(name.to_string());
        }
    }
    // Manifest first: it names everything else.
    let manifest = json!({
        "tool": "NL Video Suite",
        "createdBy": "filmcraft-nl",
        "checks": checks,
        "sequence": s.active_sequence().map(|sequence| json!({
            "width": sequence.settings.width,
            "height": sequence.settings.height,
            "fps": sequence.settings.frame_rate.num as f64 / sequence.settings.frame_rate.den.max(1) as f64,
            "durationSeconds": sequence.duration().seconds(),
        })),
        "story": s.nl.story.is_some(),
        "chapters": s.nl.longform.as_ref().and_then(|longform| longform["chapters"].as_array().map(Vec::len)).unwrap_or(0),
    });
    record(&mut files, "manifest.json", std::fs::write(folder.join("manifest.json"), serde_json::to_vec_pretty(&manifest).unwrap_or_default()).is_ok());
    // Chapters.
    if let Some(chapters) = s.nl.longform.clone() {
        record(&mut files, "chapters.json", std::fs::write(folder.join("chapters.json"), serde_json::to_vec_pretty(&chapters).unwrap_or_default()).is_ok());
    }
    // Story.
    if let Some(story) = s.nl.story.clone() {
        record(&mut files, "story.json", std::fs::write(folder.join("story.json"), serde_json::to_vec_pretty(&story).unwrap_or_default()).is_ok());
    }
    // Captions: export the first caption track, if any.
    let track = s.active_sequence().and_then(|sequence| sequence.caption_tracks.first().map(|track| track.id.0));
    if let Some(track) = track {
        record(
            &mut files,
            "captions.srt",
            s.execute("captions.export", json!({"track": track, "path": folder.join("captions.srt").to_string_lossy()})).is_ok(),
        );
    }
    // Thumbnail from the playhead; a renderer that cannot run is noted, not fatal.
    let thumbnail = s.execute("file.exportFrame", json!({"path": folder.join("thumbnail.png").to_string_lossy(), "format": "png", "import": false})).is_ok();
    record(&mut files, "thumbnail.png", thumbnail);
    // The project file itself (small, not its media).
    if let Some(project) = s.path.as_deref().map(Path::new)
        && project.is_file()
    {
        let name = project.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_else(|| "project.fcproj".into());
        record(&mut files, &name, std::fs::copy(project, folder.join(&name)).is_ok());
    }
    // README with the human summary.
    let mut readme = String::from("# Delivery package\n\nGenerated by NL Video Suite.\n\n");
    readme.push_str(&format!("- Files: {}\n", files.len()));
    for file in &files {
        readme.push_str(&format!("- {file}\n"));
    }
    readme.push_str("\nMedia is not copied: generated files stay in the suite data folder and the project references them.\n");
    record(&mut files, "README.md", std::fs::write(folder.join("README.md"), readme).is_ok());
    Ok(json!({"path": folder.to_string_lossy(), "files": files, "checks": checks, "thumbnail": thumbnail}))
}

/// `nl.delivery.social` — the checks a platform cares about before an upload.
fn delivery_social(s: &mut Session, p: &Value) -> Result<Value> {
    let platform = p.get("platform").and_then(Value::as_str).map(str::to_ascii_lowercase).unwrap_or_else(|| "youtube".into());
    let vertical_required = matches!(platform.as_str(), "tiktok" | "reels" | "instagram");
    let max_seconds = match platform.as_str() {
        "tiktok" => Some(60.0),
        "reels" | "instagram" => Some(90.0),
        _ => None,
    };
    let Some(sequence) = s.active_sequence() else { return Err(bad_cmd("nl.delivery.social", "no active sequence")) };
    let settings = &sequence.settings;
    let mut checks = Vec::new();
    let mut push = |id: &str, ok: bool, detail: String| checks.push(json!({"id": id, "ok": ok, "detail": detail}));
    if vertical_required {
        let vertical = settings.height > settings.width;
        push("aspect", vertical, format!("{}×{} ({})", settings.width, settings.height, if vertical { "verticale" } else { "orizzontale" }));
    } else {
        push("aspect", settings.width >= 1280 && settings.height >= 720, format!("{}×{}", settings.width, settings.height));
    }
    let duration = sequence.duration().seconds();
    match max_seconds {
        Some(max) => push("duration", duration <= max, format!("{duration:.1} s (limite {max:.0} s)")),
        None => push("duration", true, format!("{duration:.1} s")),
    }
    let cues: usize = sequence.caption_tracks.iter().map(|track| track.captions.len()).sum();
    push("captions", cues > 0, if cues > 0 { format!("{cues} cue(s)") } else { "nessun sottotitolo: i social lo richiedono".into() });
    let audio: usize = sequence.audio_tracks.iter().map(|track| track.items.len()).sum();
    push("audio", audio > 0, if audio > 0 { format!("{audio} clip audio") } else { "traccia audio assente".into() });
    let ready = checks.iter().all(|check| check["ok"] == true);
    Ok(json!({"platform": platform, "checks": checks, "ready": ready}))
}
pub(crate) fn commands() -> Vec<CommandSpec> {
    let spec = |id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>| -> CommandSpec {
        CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: always, run, journal: false, named: crate::commands::NamedKeys::NONE }
    };
    vec![
        spec("nl.delivery.qa", "NL Delivery Checks", "{}", delivery_qa),
        spec("nl.delivery.package", "NL Production Package", r#"{"path":str}"#, delivery_package),
        spec("nl.delivery.social", "NL Social Checks", r#"{"platform":"youtube|tiktok|reels|instagram"}"#, delivery_social),
        spec("nl.storage.status", "NL Storage Status", "{}", storage_status),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qa_reports_sequence_and_missing_captions() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        let report = s.execute("nl.delivery.qa", json!({})).unwrap();
        let checks = report["checks"].as_array().unwrap();
        assert!(checks.iter().any(|check| check["id"] == "sequence" && check["status"] == "ok"));
        assert!(checks.iter().any(|check| check["id"] == "duration"));
        assert_eq!(report["ready"], true, "the demo is deliverable apart from optional items");
    }

    #[test]
    fn the_package_writes_a_manifest_and_readme() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        let folder = std::env::temp_dir().join(format!("nl-delivery-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let report = s.execute("nl.delivery.package", json!({"path": folder.to_string_lossy()})).unwrap();
        assert!(folder.join("manifest.json").is_file());
        assert!(folder.join("README.md").is_file());
        let files = report["files"].as_array().unwrap();
        assert!(files.iter().any(|file| file == "manifest.json"));
        // the same folder cannot be reused
        assert!(s.execute("nl.delivery.package", json!({"path": folder.to_string_lossy()})).is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn social_checks_depend_on_the_platform() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        let youtube = s.execute("nl.delivery.social", json!({"platform": "youtube"})).unwrap();
        assert!(youtube["checks"].as_array().unwrap().iter().any(|check| check["id"] == "aspect"));
        let tiktok = s.execute("nl.delivery.social", json!({"platform": "tiktok"})).unwrap();
        let aspect = tiktok["checks"].as_array().unwrap().iter().find(|check| check["id"] == "aspect").cloned().unwrap();
        assert_eq!(aspect["ok"], false, "the demo is landscape, not vertical");
        assert!(s.execute("nl.delivery.social", json!({"platform": "youtube"})).is_ok());
    }

    #[test]
    fn storage_reports_sizes() {
        let mut s = Session::default();
        let report = s.execute("nl.storage.status", json!({})).unwrap();
        assert!(report["dataBytes"].as_u64().is_some());
        assert!(report["generatedFiles"].as_u64().is_some());
    }
}
