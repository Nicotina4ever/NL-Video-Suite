//! Discovery and verification of local speech models, plus the local Qwen3-ASR runner.
//!
//! The classic NL suite kept model paths and their identity (path/bytes/sha256) in
//! `asr-identities`; this module finds models in the suite, project, user folders and the legacy
//! `J:\ZLinkdaC` location the operator already uses (Qwen3-ASR GGUF + forced aligner + CLI, plus
//! Whisper CTranslate2 folders and WhisperX), checks what is really there and runs qwen3-asr.cpp
//! locally. Nothing is downloaded here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::Session;

const MAX_ENTRIES: usize = 400;
const GGUF_MAGIC: &[u8; 4] = b"GGUF";

fn exists(path: &Path) -> bool {
    path.is_file()
}

/// Folders that may hold speech models, in priority order and without duplicates.
pub fn roots(s: &Session) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut push = |path: PathBuf| {
        if path.is_dir() && !roots.iter().any(|root| root == &path) {
            roots.push(path);
        }
    };
    for part in s.prefs.nl.asr_search_dirs_list() {
        push(PathBuf::from(part));
    }
    if let Some(root) = crate::nl::suite_root() {
        push(root.join("models"));
        push(root.join("models").join("asr"));
        push(root.join("models").join("alignment"));
    }
    if let Some(project) = s.path.as_deref().map(Path::new).and_then(Path::parent) {
        push(project.join("models"));
        push(project.join("models").join("asr"));
    }
    if let Some(data) = crate::autosave::default_data_dir() {
        push(data.join("models"));
    }
    // Legacy suite locations (the operator's existing Qwen3-ASR and WhisperX installs).
    let legacy = PathBuf::from(r"J:\ZLinkdaC");
    if legacy.is_dir() {
        push(legacy.clone());
        push(legacy.join("Qwen3ASR"));
        push(legacy.join("Qwen3ASR").join("models"));
        push(legacy.join("SpeechToText"));
        push(legacy.join("SpeechToText").join("WhisperX"));
    }
    roots
}

/// Known identities from the classic suite (`asr-identities/*.json`: path/bytes/sha256), keyed by
/// lower-cased path.
fn known_identities(s: &Session) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let mut folders: Vec<PathBuf> = Vec::new();
    if let Some(root) = crate::nl::suite_root() {
        folders.push(root.join("sviluppo").join("anteprima_creativa").join("data").join("projects"));
    }
    if let Some(project) = s.path.as_deref().map(Path::new).and_then(Path::parent) {
        folders.push(project.join("asr-identities"));
        folders.push(project.to_path_buf());
    }
    fn walk(folder: &Path, depth: usize, out: &mut BTreeMap<String, Value>) {
        if depth > 5 || out.len() > 2000 {
            return;
        }
        let Ok(read) = std::fs::read_dir(folder) else { return };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, depth + 1, out);
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) == Some("asr-identities") {
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
            let Some(model_path) = value.get("path").and_then(Value::as_str) else { continue };
            if value.get("sha256").and_then(Value::as_str).is_none() {
                continue;
            }
            out.insert(model_path.to_lowercase(), value);
        }
    }
    for folder in folders {
        walk(&folder, 0, &mut out);
    }
    out
}

fn classify_gguf(path: &Path, bytes: u64, identities: &BTreeMap<String, Value>, out: &mut Vec<Value>) {
    if bytes < 10 * 1024 * 1024 {
        return;
    }
    let mut magic = [0u8; 4];
    let valid = std::fs::File::open(path).and_then(|mut file| std::io::Read::read_exact(&mut file, &mut magic)).is_ok() && &magic == GGUF_MAGIC;
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_lowercase();
    let role = if name.contains("align") { "aligner" } else { "asr" };
    let identity = identities.get(&path.to_string_lossy().to_lowercase());
    let identity_matches = identity.and_then(|value| value.get("bytes").and_then(Value::as_u64)).is_some_and(|known| known == bytes);
    out.push(json!({
        "path": path.to_string_lossy(),
        "kind": "qwen-gguf",
        "role": role,
        "bytes": bytes,
        "verified": valid && identity_matches,
        "ggufMagic": valid,
        "identity": identity,
        "identitySizeMatches": identity_matches,
        "missing": if valid { json!([]) } else { json!(["GGUF magic"]) },
    }));
}

fn classify_dir(path: &Path, identities: &BTreeMap<String, Value>, out: &mut Vec<Value>) {
    let model = path.join("model.bin");
    if model.is_file() {
        let bytes = std::fs::metadata(&model).map(|metadata| metadata.len()).unwrap_or(0);
        let mut missing = Vec::new();
        if !path.join("config.json").is_file() {
            missing.push("config.json");
        }
        out.push(json!({
            "path": path.to_string_lossy(),
            "kind": "whisper-ct2",
            "role": "asr",
            "bytes": bytes,
            "verified": missing.is_empty(),
            "missing": missing,
        }));
        return;
    }
    let Ok(read) = std::fs::read_dir(path) else { return };
    for entry in read.flatten() {
        let file = entry.path();
        if !file.is_file() {
            continue;
        }
        let name = file.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
        let lower = name.to_lowercase();
        if lower.ends_with(".gguf") {
            let bytes = std::fs::metadata(&file).map(|metadata| metadata.len()).unwrap_or(0);
            classify_gguf(&file, bytes, identities, out);
        } else if lower.contains("qwen3-asr-cli") || lower.contains("whisperx-standalone") {
            out.push(json!({
                "path": file.to_string_lossy(),
                "kind": "engine",
                "engine": if lower.contains("qwen") { "qwen3-asr" } else { "whisperx" },
                "bytes": std::fs::metadata(&file).map(|metadata| metadata.len()).unwrap_or(0),
                "verified": true,
                "missing": [],
            }));
        }
    }
}

fn walk(folder: &Path, depth: usize, identities: &BTreeMap<String, Value>, models: &mut Vec<Value>, engines: &mut Vec<Value>) {
    if depth > 4 || models.len() + engines.len() >= MAX_ENTRIES {
        return;
    }
    let Ok(read) = std::fs::read_dir(folder) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else { continue };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
            if matches!(name, "_internal" | "node_modules" | "target" | ".git") {
                continue;
            }
            let mut local = Vec::new();
            classify_dir(&path, identities, &mut local);
            for entry in local {
                if entry["kind"] == "engine" {
                    engines.push(entry);
                } else {
                    models.push(entry);
                }
            }
            walk(&path, depth + 1, identities, models, engines);
            continue;
        }
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_lowercase();
        if name.ends_with(".gguf") {
            classify_gguf(&path, metadata.len(), identities, models);
        } else if metadata.is_file() && (name.contains("qwen3-asr-cli") || name.contains("whisperx-standalone")) {
            engines.push(json!({
                "path": path.to_string_lossy(),
                "kind": "engine",
                "engine": if name.contains("qwen") { "qwen3-asr" } else { "whisperx" },
                "bytes": metadata.len(),
                "verified": true,
            }));
        }
    }
}

/// `nl.asr.models` — the speech models and engines found locally, with verification facts.
pub fn discover(s: &Session) -> Value {
    let identities = known_identities(s);
    let roots = roots(s);
    let mut models: Vec<Value> = Vec::new();
    let mut engines: Vec<Value> = Vec::new();
    for root in &roots {
        walk(root, 0, &identities, &mut models, &mut engines);
        if models.len() + engines.len() >= MAX_ENTRIES {
            break;
        }
    }
    // Deduplicate by path, keeping the first (higher-priority root).
    let mut seen = std::collections::HashSet::new();
    models.retain(|entry| seen.insert(entry["path"].as_str().unwrap_or_default().to_lowercase()));
    engines.retain(|entry| seen.insert(entry["path"].as_str().unwrap_or_default().to_lowercase()));
    let verified = models.iter().filter(|model| model["verified"] == true).count();
    let qwen = models.iter().filter(|model| model["kind"] == "qwen-gguf").count();
    let whisper = models.iter().filter(|model| model["kind"] == "whisper-ct2").count();
    json!({
        "roots": roots.iter().map(|root| root.to_string_lossy()).collect::<Vec<_>>(),
        "models": models,
        "engines": engines,
        "count": verified,
        "qwenModels": qwen,
        "whisperModels": whisper,
    })
}

/// Where the qwen3-asr.cpp runner, its GGUF model, the aligner and ffmpeg are.
pub struct QwenPaths {
    pub runner: PathBuf,
    pub model: PathBuf,
    pub aligner: PathBuf,
    pub ffmpeg: PathBuf,
}

fn first_scanned(discovery: &Value, kind: &str, role: &str) -> Option<PathBuf> {
    discovery["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|model| model["kind"] == kind && model["role"] == role && model["verified"] == true)
        .filter_map(|model| model["path"].as_str().map(PathBuf::from))
        .next()
}

pub fn resolve_qwen(s: &Session, p: &Value) -> Result<QwenPaths, String> {
    let discovery = discover(s);
    let runner = p
        .get("runner")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(s.prefs.nl.asr_runner.trim())).filter(|path| !path.as_os_str().is_empty()))
        .or_else(|| {
            discovery["engines"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|engine| engine["engine"] == "qwen3-asr")
                .and_then(|engine| engine["path"].as_str())
                .map(PathBuf::from)
        })
        .ok_or_else(|| "qwen3-asr-cli.exe not found: install it or set nl.asrRunner".to_string())?;
    let model = p
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(s.prefs.nl.asr_qwen_model.trim())).filter(|path| !path.as_os_str().is_empty()))
        .or_else(|| first_scanned(&discovery, "qwen-gguf", "asr"))
        .ok_or_else(|| "Qwen3-ASR GGUF model not found: set nl.asrQwenModel or nl.asrSearchDirs".to_string())?;
    let aligner = p
        .get("aligner")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(s.prefs.nl.asr_qwen_aligner.trim())).filter(|path| !path.as_os_str().is_empty()))
        .or_else(|| first_scanned(&discovery, "qwen-gguf", "aligner"))
        .ok_or_else(|| "Qwen3 forced-aligner GGUF not found: set nl.asrQwenAligner or nl.asrSearchDirs".to_string())?;
    if !exists(&runner) {
        return Err(format!("qwen runner not found: {}", runner.display()));
    }
    if !exists(&model) {
        return Err(format!("Qwen model not found: {}", model.display()));
    }
    if !exists(&aligner) {
        return Err(format!("Qwen aligner not found: {}", aligner.display()));
    }
    let ffmpeg = crate::nl::suite_root()
        .map(|root| root.join("app").join("tools").join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("ffmpeg"));
    Ok(QwenPaths { runner, model, aligner, ffmpeg })
}

fn qwen_language(code: &str) -> Option<&'static str> {
    match code {
        "it" => Some("italian"),
        "en" => Some("english"),
        "es" => Some("spanish"),
        "fr" => Some("french"),
        "de" => Some("german"),
        "pt" => Some("portuguese"),
        "nl" => Some("dutch"),
        "pl" => Some("polish"),
        _ => None,
    }
}

fn stamp() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_millis()).unwrap_or(0)
}

/// Extract 16 kHz mono WAV with ffmpeg.
fn to_wav(s: &Session, ffmpeg: &Path, source: &Path) -> Result<PathBuf, String> {
    let dir = crate::nl::media_work_dir(s, &source.to_string_lossy(), "qwen");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let wav = dir.join(format!("input-{}.wav", stamp()));
    let output = Command::new(ffmpeg)
        .arg("-nostdin")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(source)
        .arg("-vn")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000")
        .arg(&wav)
        .output()
        .map_err(|e| format!("{}: {e}", ffmpeg.display()))?;
    if !output.status.success() || !wav.is_file() {
        return Err(format!("ffmpeg failed: {}", String::from_utf8_lossy(&output.stderr).trim().chars().take(500).collect::<String>()));
    }
    Ok(wav)
}

/// Run qwen3-asr.cpp on a media file and return the transcript with word timings.
pub fn transcribe_qwen(s: &Session, source: &Path, paths: &QwenPaths, language: &str, threads: u32) -> Result<Value, String> {
    let started = SystemTime::now();
    let wav = to_wav(s, &paths.ffmpeg, source)?;
    let out = wav.with_extension("json");
    let mut command = Command::new(&paths.runner);
    command
        .arg("-m")
        .arg(&paths.model)
        .arg("--aligner-model")
        .arg(&paths.aligner)
        .arg("-f")
        .arg(&wav)
        .arg("--transcribe-align")
        .arg("-t")
        .arg(threads.clamp(1, 64).to_string())
        .arg("--max-tokens")
        .arg("1024")
        .arg("--no-timing")
        .arg("-o")
        .arg(&out);
    if let Some(name) = qwen_language(language) {
        command.arg("-l").arg(name);
    }
    if let Some(parent) = paths.runner.parent() {
        command.current_dir(parent);
    }
    let output = command.output().map_err(|e| format!("{}: {e}", paths.runner.display()))?;
    if !output.status.success() || !out.is_file() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("qwen3-asr-cli failed: {}", stderr.trim().chars().take(800).collect::<String>()));
    }
    let text = std::fs::read_to_string(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let value: Value = serde_json::from_str(&text).map_err(|e| format!("{}: not JSON: {e}", out.display()))?;
    let words = value.get("words").and_then(Value::as_array).cloned().unwrap_or_default();
    let transcript =
        words.iter().filter_map(|word| word.get("word").and_then(Value::as_str)).map(str::trim).filter(|word| !word.is_empty()).collect::<Vec<_>>().join(" ");
    if transcript.is_empty() {
        return Err("qwen3-asr-cli returned no words".into());
    }
    let detected =
        String::from_utf8_lossy(&output.stderr).lines().find_map(|line| line.split("Detected language:").nth(1)).map(str::trim).unwrap_or(language).to_string();
    Ok(json!({
        "provider": "qwen_gguf",
        "text": transcript,
        "words": words,
        "language": detected,
        "runner": paths.runner.to_string_lossy(),
        "model": paths.model.to_string_lossy(),
        "aligner": paths.aligner.to_string_lossy(),
        "elapsedSeconds": started.elapsed().map(|elapsed| elapsed.as_secs_f64().round()).unwrap_or(0.0),
        "output": out.to_string_lossy(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_names_match_the_qwen_expected_names() {
        assert_eq!(qwen_language("it"), Some("italian"));
        assert_eq!(qwen_language("en"), Some("english"));
        assert_eq!(qwen_language("auto"), None);
        assert_eq!(qwen_language("xx"), None);
    }

    #[test]
    fn a_fake_gguf_without_the_magic_is_not_verified() {
        let root = std::env::temp_dir().join(format!("nl-asr-models-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("qwen3-asr-1.7b-q8_0.gguf");
        std::fs::write(&file, vec![0u8; 11 * 1024 * 1024]).unwrap();
        let mut out = Vec::new();
        classify_gguf(&file, 11 * 1024 * 1024, &BTreeMap::new(), &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["ggufMagic"], false);
        assert_eq!(out[0]["verified"], false);
        let _ = std::fs::remove_dir_all(&root);
    }
}
