//! Automatic storage maintenance: the per-user data folder fills up with temporary files (the
//! two bridge frames of every run, logs, transcript caches). This module removes only those
//! disposable files, older than a threshold, and never touches generated media unless the operator
//! explicitly enables it in Settings ▸ NL Video Suite.
//!
//! The automatic pass runs at most once a day when the editor starts; the Storage panel can preview
//! or run it now. Every deletion is confined to the suite's data folder.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::Result;
use crate::commands::{CommandSpec, always};

/// Disposable files older than this are removed by the automatic pass.
pub const DEFAULT_TEMP_DAYS: u64 = 7;
/// Generated media is removed only when enabled, and only after this many days.
pub const DEFAULT_GENERATED_DAYS: u64 = 30;
/// At most one automatic pass per day.
const AUTO_INTERVAL: u64 = 24 * 60 * 60;

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or(0)
}

fn data_dir() -> PathBuf {
    crate::autosave::default_data_dir().unwrap_or_else(std::env::temp_dir)
}

#[derive(Default, Clone, Copy)]
struct Totals {
    files: u64,
    bytes: u64,
}

impl Totals {
    fn add(&mut self, path: &Path) {
        self.files += 1;
        self.bytes += std::fs::symlink_metadata(path).map(|metadata| metadata.len()).unwrap_or(0);
    }
}

/// State of one cleanup pass: what it deleted, what it kept and which folders it already visited.
struct Cleanup {
    dry_run: bool,
    seen: HashSet<PathBuf>,
    categories: Vec<Value>,
    removed: Totals,
    kept: Totals,
}

impl Cleanup {
    fn new(dry_run: bool) -> Self {
        Self { dry_run, seen: HashSet::new(), categories: Vec::new(), removed: Totals::default(), kept: Totals::default() }
    }

    /// Remove files under `dir` older than `cutoff`; empty folders are pruned. Symlinks are never
    /// followed or deleted through.
    fn dir(&mut self, dir: &Path, cutoff: SystemTime) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut children = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&path) else { continue };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                children.push(path);
                continue;
            }
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            if modified < cutoff {
                let deleted = self.dry_run || std::fs::remove_file(&path).is_ok();
                if deleted {
                    self.removed.add(&path);
                } else {
                    self.kept.add(&path);
                }
            } else {
                self.kept.add(&path);
            }
        }
        for child in children {
            self.dir(&child, cutoff);
            let _ = std::fs::remove_dir(&child); // only succeeds when empty
        }
    }

    fn category(&mut self, root: &Path, name: &str, relative: &str, days: u64) {
        let dir = root.join(relative);
        if dir == root || !dir.exists() {
            return;
        }
        // On case-insensitive file systems two category names can resolve to the same folder: count
        // it once.
        let identity = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        if !self.seen.insert(identity) {
            return;
        }
        let cutoff = SystemTime::now() - Duration::from_secs(days.saturating_mul(24 * 60 * 60));
        let before = (self.removed.files, self.removed.bytes, self.kept.files, self.kept.bytes);
        self.dir(&dir, cutoff);
        self.categories.push(json!({
            "category": name,
            "folder": dir.to_string_lossy(),
            "olderThanDays": days,
            "removedFiles": self.removed.files - before.0,
            "removedBytes": self.removed.bytes - before.1,
            "keptFiles": self.kept.files - before.2,
            "keptBytes": self.kept.bytes - before.3,
        }));
    }
}

/// The cleanup itself, against an explicit root (the data folder in production, a temporary folder
/// in tests).
pub fn cleanup_in(root: &Path, temp_days: u64, include_generated: bool, generated_days: u64, dry_run: bool) -> Value {
    let mut cleanup = Cleanup::new(dry_run);
    // Temporary frames of the generative bridge.
    cleanup.category(root, "bridge-frames", "nl/bridge", temp_days);
    // Old log files.
    cleanup.category(root, "logs", "Logs", temp_days);
    // Recognition/transcript caches the suite can rebuild.
    cleanup.category(root, "asr-cache", "nl/asr", temp_days);
    cleanup.category(root, "cache", "cache", temp_days);
    // Generated media: only on request, after a longer retention.
    if include_generated {
        cleanup.category(root, "generated-media", "generated", generated_days);
    }
    json!({
        "root": root.to_string_lossy(),
        "dryRun": dry_run,
        "tempDays": temp_days,
        "includeGenerated": include_generated,
        "generatedDays": generated_days,
        "removedFiles": cleanup.removed.files,
        "removedBytes": cleanup.removed.bytes,
        "keptFiles": cleanup.kept.files,
        "keptBytes": cleanup.kept.bytes,
        "categories": cleanup.categories,
    })
}

fn maintenance_path(root: &Path) -> PathBuf {
    root.join("nl").join("maintenance.json")
}

/// `nl.storage.cleanup` — preview or run the cleanup; `auto: true` throttles to once a day.
fn storage_cleanup(s: &mut crate::Session, p: &Value) -> Result<Value> {
    let root = data_dir();
    let auto = p.get("auto").and_then(Value::as_bool).unwrap_or(false);
    let force = p.get("force").and_then(Value::as_bool).unwrap_or(false);
    let dry_run = p.get("dryRun").and_then(Value::as_bool).unwrap_or(false);
    let stamp = maintenance_path(&root);
    let last_run = std::fs::read_to_string(&stamp)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value["lastRun"].as_u64())
        .unwrap_or(0);
    if auto && !force && last_run > 0 && now_secs().saturating_sub(last_run) < AUTO_INTERVAL {
        return Ok(json!({"auto": true, "skipped": true, "lastRun": last_run, "removedFiles": 0, "removedBytes": 0}));
    }
    let temp_days = p.get("days").and_then(Value::as_u64).unwrap_or(s.prefs.nl.cleanup_days.max(1)).clamp(1, 365);
    let include_generated = p.get("includeGenerated").and_then(Value::as_bool).unwrap_or(s.prefs.nl.cleanup_generated);
    let generated_days = p.get("generatedDays").and_then(Value::as_u64).unwrap_or(s.prefs.nl.generated_keep_days.max(1)).clamp(1, 3650);
    let mut result = cleanup_in(&root, temp_days, include_generated, generated_days, dry_run);
    result["auto"] = json!(auto);
    result["skipped"] = json!(false);
    if !dry_run {
        let _ = std::fs::create_dir_all(root.join("nl"));
        let _ = std::fs::write(&stamp, json!({"lastRun": now_secs()}).to_string());
    }
    Ok(result)
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "nl.storage.cleanup",
        label: "NL Storage Cleanup",
        menu: &[],
        shortcut: None,
        params: r#"{"dryRun":bool?,"auto":bool?,"force":bool?,"days":int?,"includeGenerated":bool?,"generatedDays":int?}"#,
        enabled: always,
        run: storage_cleanup,
        journal: false,
        named: crate::commands::NamedKeys::NONE,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn write_old(path: &Path, days: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut file = File::create(path).unwrap();
        file.write_all(b"x").unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(days * 24 * 60 * 60 + 60)).unwrap();
    }

    fn write_new(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"new").unwrap();
    }

    #[test]
    fn cleanup_removes_only_old_temporary_files_and_keeps_generated_media_by_default() {
        let root = std::env::temp_dir().join(format!("nl-maint-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        write_old(&root.join("nl/bridge/2020/outgoing-last-frame.png"), 10);
        write_new(&root.join("nl/bridge/2020/incoming-first-frame.png"));
        write_old(&root.join("generated/comfy-1/old.mp4"), 60);
        write_old(&root.join("Logs/old.log"), 40);
        let outside = root.join("project.fcproj");
        write_old(&outside, 60);
        let preview = cleanup_in(&root, 7, false, 30, true);
        assert_eq!(preview["removedFiles"], 2, "{preview:?}");
        assert!(root.join("nl/bridge/2020/outgoing-last-frame.png").is_file(), "dry run deletes nothing");
        let run = cleanup_in(&root, 7, false, 30, false);
        assert_eq!(run["removedFiles"], 2, "{run:?}");
        assert!(!root.join("nl/bridge/2020/outgoing-last-frame.png").exists());
        assert!(root.join("nl/bridge/2020/incoming-first-frame.png").is_file(), "recent frame kept");
        assert!(root.join("generated/comfy-1/old.mp4").is_file(), "generated media kept unless asked");
        assert!(outside.is_file(), "files outside the category folders are untouched");
        let with_generated = cleanup_in(&root, 7, true, 30, false);
        assert_eq!(with_generated["removedFiles"], 1, "{with_generated:?}");
        assert!(!root.join("generated/comfy-1/old.mp4").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_command_skips_a_second_automatic_run_within_a_day() {
        let mut s = crate::Session::default();
        let root = data_dir();
        let stamp = maintenance_path(&root);
        let saved = std::fs::read(&stamp).ok();
        let _ = std::fs::create_dir_all(root.join("nl"));
        let _ = std::fs::write(&stamp, json!({"lastRun": now_secs()}).to_string());
        let report = s.execute("nl.storage.cleanup", json!({"auto": true})).unwrap();
        assert_eq!(report["skipped"], true, "{report:?}");
        match saved {
            Some(bytes) => {
                let _ = std::fs::write(&stamp, bytes);
            }
            None => {
                let _ = std::fs::remove_file(&stamp);
            }
        }
    }
}
