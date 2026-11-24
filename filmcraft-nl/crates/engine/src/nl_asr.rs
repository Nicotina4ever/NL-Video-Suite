//! Speech recognition through the NL suite's own bridge (WhisperX, local models).
//!
//! The bridge is the same one the NL application uses: `runtime/python/python.exe` runs
//! `sviluppo/bridge/clipsai_bridge.py` with a request/result JSON pair. No model is downloaded
//! here and the source media is never modified.
#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use filmcraft_project::{ItemId, Transcript, Word};
use filmcraft_time::Tick;
use serde_json::{Value, json};

use crate::nl::http;
use crate::{EngineError, Result, Session};

/// A transcription job whose transcript is applied when it finishes.
pub struct PendingAsr {
    job: u64,
    item: ItemId,
    transcript: Arc<Mutex<Option<std::result::Result<Transcript, String>>>>,
    epoch: u64,
}

fn bad(e: impl ToString) -> EngineError {
    EngineError::BadParams { cmd: "transcript.transcribeNl".into(), msg: e.to_string() }
}

/// `transcript.transcribeNl` — WhisperX through the NL bridge.
fn transcribe(s: &mut Session, p: &Value) -> Result<Value> {
    let language =
        p.get("language").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string).unwrap_or_else(|| s.prefs.nl.asr_language());
    if !matches!(language.as_str(), "it" | "en" | "es" | "fr" | "de" | "pt" | "nl" | "pl" | "auto") {
        return Err(bad("language must be it, en, es, fr, de, pt, nl, pl or auto"));
    }
    let device =
        p.get("device").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string).unwrap_or_else(|| s.prefs.nl.asr_device());
    if !matches!(device.as_str(), "auto" | "cpu" | "cuda") {
        return Err(bad("device must be auto, cpu or cuda"));
    }
    let (item, path) = target(s, p).ok_or_else(|| bad("select a media file in the Project panel or put one on the timeline"))?;
    let root = suite_root();
    if !root.join("runtime/python/python.exe").is_file() || !root.join("sviluppo/bridge/clipsai_bridge.py").is_file() {
        return Err(bad(format!("NL runtime/bridge not found under {}", root.display())));
    }
    let model_dir = crate::nl::pipeline::asr_model_dir(&root, &s.prefs.nl.asr_model);
    if !model_dir.is_dir() {
        return Err(bad(format!(
            "the speech model folder in Settings ▸ NL Video Suite was not found: {} (nothing is downloaded automatically)",
            model_dir.display()
        )));
    }

    let id = s.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let job = crate::Job { id, label: format!("Transcribe NL ({language}, {device})"), progress: Default::default(), result: Default::default() };
    let (prog, res) = (job.progress.clone(), job.result.clone());
    let transcript: Arc<Mutex<Option<std::result::Result<Transcript, String>>>> = Arc::default();
    let out = transcript.clone();
    let work = work_dir(s, &path, &language, id);
    let wait = p.get("wait").and_then(Value::as_bool).unwrap_or(false);
    let ollama_url = s.prefs.nl.ollama_url();
    let ollama_model = s.prefs.nl.ollama_model();
    let model_name = model_dir.file_name().map(|n| n.to_string_lossy().to_string());
    let used: Arc<Mutex<Option<String>>> = Arc::default();
    let (path_run, language_run, device_run) = (path.clone(), language.clone(), device.clone());
    let used_run = used.clone();
    let run = move || {
        prog.total.store(1, Ordering::Relaxed);
        // GPU first: the portable runtime ships CUDA; when the model or alignment cannot start
        // there, retry once on CPU, say so, and keep both errors if the retry fails too.
        let (result, used_device) = if device_run == "auto" {
            let bridge = |device: &str| {
                run_bridge(
                    &BridgeJob {
                        root: &root,
                        model: &model_dir,
                        media: &path_run,
                        work: &work,
                        language: &language_run,
                        device,
                        ollama_url: &ollama_url,
                        ollama_model: &ollama_model,
                    },
                    &prog,
                )
            };
            match bridge("cuda") {
                Ok(value) => (Ok(value), "cuda"),
                Err(gpu) => {
                    *prog.status.lock().unwrap_or_else(|e| e.into_inner()) = "ASR NL: CUDA non disponibile, ripiego su CPU".into();
                    (bridge("cpu").map_err(|cpu| format!("CUDA: {gpu}; CPU: {cpu}")), "cpu")
                }
            }
        } else {
            let value = run_bridge(
                &BridgeJob {
                    root: &root,
                    model: &model_dir,
                    media: &path_run,
                    work: &work,
                    language: &language_run,
                    device: &device_run,
                    ollama_url: &ollama_url,
                    ollama_model: &ollama_model,
                },
                &prog,
            );
            (value, device_run.as_str())
        };
        *used_run.lock().unwrap_or_else(|e| e.into_inner()) = Some(used_device.to_string());
        let parsed = result.and_then(|value| parse_transcript(&value, &language_run));
        *res.lock().unwrap_or_else(|e| e.into_inner()) = Some(match &parsed {
            Ok(_) => Ok(filmcraft_export::Report::default()),
            Err(e) => Err(e.clone()),
        });
        *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(parsed);
        prog.done.store(1, Ordering::Relaxed);
        prog.finished.store(true, Ordering::Relaxed);
    };
    s.jobs.push(job);
    s.asr_jobs.push(PendingAsr { job: id, item, transcript, epoch: s.document_epoch });
    if wait {
        run();
        poll(s);
    } else {
        std::thread::Builder::new().name("filmcraft-nl-asr".into()).spawn(run).map_err(|e| EngineError::Other(format!("can't start the ASR thread: {e}")))?;
    }
    let mut response = json!({
        "job": id,
        "item": item.0,
        "language": language,
        "device": device,
        "gpuFirst": s.prefs.nl.gpu_first,
        "model": model_name,
        "media": path,
    });
    if wait && let Some(used) = used.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        response["usedDevice"] = json!(used);
    }
    Ok(response)
}

fn target(s: &Session, p: &Value) -> Option<(ItemId, String)> {
    crate::nl::target_media(s, p)
}

/// Everything one bridge run needs. Grouped so the call sites stay readable.
struct BridgeJob<'a> {
    root: &'a Path,
    model: &'a Path,
    media: &'a str,
    work: &'a Path,
    language: &'a str,
    device: &'a str,
    ollama_url: &'a str,
    ollama_model: &'a str,
}

fn run_bridge(job: &BridgeJob, progress: &filmcraft_export::Progress) -> std::result::Result<Value, String> {
    let BridgeJob { root, model, media, work, language, device, ollama_url, ollama_model } = *job;
    let python = root.join("runtime/python/python.exe");
    let bridge = root.join("sviluppo/bridge/clipsai_bridge.py");
    std::fs::create_dir_all(work).map_err(|e| format!("{}: {e}", work.display()))?;
    let request = json!({
        "source_root": root.join("engines/clipsai").to_string_lossy(),
        "input_path": media,
        "work_dir": work.to_string_lossy(),
        "operation": "transcribe",
        "language": language,
        "device": device,
        "progress_path": work.join("progress.json").to_string_lossy(),
        "ollama_url": ollama_url,
        "model": ollama_model,
        "params": {}
    });
    let request_path = work.join("request.json");
    let result_path = work.join("result.json");
    std::fs::write(&request_path, serde_json::to_vec_pretty(&request).map_err(|e| e.to_string())?).map_err(|e| format!("{}: {e}", request_path.display()))?;
    let mut command = std::process::Command::new(&python);
    command
        .arg(bridge)
        .arg(&request_path)
        .arg(&result_path)
        .env("NL_ALIGNMENT_MODEL_PATH", root.join("models/alignment/italian-xlsr53"))
        .env("CLIPSAI_WHISPER_MODEL", model)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .current_dir(root);
    let mut paths = vec![root.join("app/tools")];
    if let Some(old) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&old));
    }
    if let Ok(path) = std::env::join_paths(paths) {
        command.env("PATH", path);
    }
    *progress.status.lock().unwrap_or_else(|e| e.into_inner()) = "ASR NL: caricamento del modello e trascrizione".into();
    let output = command.output().map_err(|e| format!("can't start {}: {e}", python.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = stderr.chars().rev().take(3000).collect::<String>().chars().rev().collect();
        return Err(format!("bridge exited with {}: {tail}", output.status));
    }
    let bytes = std::fs::read(&result_path).map_err(|e| format!("{}: {e}", result_path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("bridge result is not JSON: {e}"))
}

fn parse_transcript(value: &Value, language: &str) -> std::result::Result<Transcript, String> {
    let words = value.get("words").and_then(Value::as_array).ok_or_else(|| "bridge result has no words array".to_string())?;
    let mut out = Vec::with_capacity(words.len());
    for word in words {
        let text = word.get("word").and_then(Value::as_str).or_else(|| word.get("text").and_then(Value::as_str)).map(str::trim).filter(|s| !s.is_empty());
        let start = word.get("start").and_then(Value::as_f64);
        let end = word.get("end").and_then(Value::as_f64);
        let (Some(text), Some(start), Some(end)) = (text, start, end) else { continue };
        if !start.is_finite() || !end.is_finite() || end <= start {
            continue;
        }
        out.push(Word::new(text, Tick::from_seconds_f64(start.max(0.0)), Tick::from_seconds_f64(end.max(start))));
    }
    if out.is_empty() {
        return Err("the bridge returned no usable word timings (silent media?)".into());
    }
    let mut transcript = Transcript {
        language: value.get("language").and_then(Value::as_str).unwrap_or(language).to_string(),
        source: "nl-whisperx".into(),
        speakers: Vec::new(),
        words: out,
    };
    transcript.normalize();
    transcript.check()?;
    Ok(transcript)
}

/// Apply finished transcriptions (one undo step each).
pub(crate) fn poll(s: &mut Session) {
    let mut i = 0;
    while i < s.asr_jobs.len() {
        let job = s.jobs.iter().find(|j| j.id == s.asr_jobs[i].job);
        let finished = job.is_none_or(|j| j.progress.finished.load(Ordering::Relaxed));
        let cancelled = job.is_some_and(|j| j.progress.cancel.load(Ordering::Relaxed));
        if !finished {
            i += 1;
            continue;
        }
        let pending = s.asr_jobs.remove(i);
        if cancelled {
            continue;
        }
        let Some(result) = pending.transcript.lock().unwrap_or_else(|e| e.into_inner()).take() else { continue };
        if pending.epoch != s.document_epoch {
            s.error_toast("transcript.transcribeNl", "the project was replaced; the transcription was not applied");
            continue;
        }
        match result {
            Ok(transcript) => {
                let count = transcript.words.len();
                let item = pending.item;
                match s.edit("Transcribe NL", move |project, _| {
                    project.transcripts.insert(item, Arc::new(transcript));
                    Ok(())
                }) {
                    Ok(_) => s.toast(format!("ASR NL: {count} word(s) added to the transcript")),
                    Err(e) => s.error_toast("transcript.transcribeNl", e.to_string()),
                }
            }
            Err(e) => s.error_toast("transcript.transcribeNl", format!("ASR NL: {e}")),
        }
    }
}

/// The NL Python runtime, bridge and WhisperX model are present in this suite folder.
pub fn available() -> bool {
    let root = suite_root();
    root.join("runtime/python/python.exe").is_file()
        && root.join("sviluppo/bridge/clipsai_bridge.py").is_file()
        && root.join("models/asr/faster-whisper-large-v3").is_dir()
}

fn suite_root() -> PathBuf {
    crate::nl::suite_root().unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// A stable work folder per media so an unchanged file reuses its cached transcript; without the
/// cache preference every job gets its own folder.
fn work_dir(s: &crate::Session, media: &str, _language: &str, job: u64) -> PathBuf {
    let dir = crate::nl::media_work_dir(s, media, "asr");
    if s.prefs.nl.asr_cache { dir } else { dir.with_file_name(format!("job-{job}")) }
}

/// A `multipart/form-data` body for the speech endpoint.
fn multipart(boundary: &str, file_name: &str, file_bytes: &[u8], fields: &[(&str, &str)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    }
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: application/octet-stream\r\n\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(file_bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

/// `nl.asr.transcribe` — speech recognition through the configured provider. The bridge stays the
/// backend that also applies the transcript to the media (`transcript.transcribeNl`); this command
/// covers any OpenAI-compatible `/audio/transcriptions` server (whisper.cpp, faster-whisper-server,
/// vLLM whisper, LocalAI) and returns the text without touching the project.
fn provider_transcribe(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "nl.asr.transcribe";
    let bad = |msg: String| EngineError::BadParams { cmd: cmd.into(), msg };
    let provider = p.get("backend").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| s.prefs.nl.asr_provider());
    if !matches!(provider.as_str(), "bridge" | "openai_compatible" | "qwen_gguf") {
        return Err(bad("backend must be bridge, openai_compatible or qwen_gguf".into()));
    }
    if provider == "bridge" {
        return Err(bad(
            "the bridge backend is transcript.transcribeNl (it also applies the transcript); set nl.asrProvider=openai_compatible for an HTTP server".into(),
        ));
    }
    let source = match p.get("path").and_then(Value::as_str).map(str::trim).filter(|path| !path.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => target(s, p).map(|(_, path)| PathBuf::from(path)).ok_or_else(|| bad("pass `path` or select a media item / timeline clip".into()))?,
    };
    if !source.is_file() {
        return Err(bad(format!("file not found: {}", source.display())));
    }
    if provider == "qwen_gguf" {
        let paths = crate::nl::asr_models::resolve_qwen(s, p).map_err(bad)?;
        let language = p
            .get("language")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|language| !language.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| s.prefs.nl.asr_language());
        let bytes = std::fs::metadata(&source).map(|metadata| metadata.len()).unwrap_or(0);
        if p.get("dryRun").and_then(Value::as_bool).unwrap_or(false) {
            return Ok(json!({
                "dryRun": true,
                "submitted": false,
                "provider": "qwen_gguf",
                "runner": paths.runner.to_string_lossy(),
                "model": paths.model.to_string_lossy(),
                "aligner": paths.aligner.to_string_lossy(),
                "ffmpeg": paths.ffmpeg.to_string_lossy(),
                "language": language,
                "source": source.to_string_lossy(),
                "bytes": bytes,
            }));
        }
        let threads = p.get("threads").and_then(Value::as_u64).unwrap_or(8).clamp(1, 64) as u32;
        let mut result = crate::nl::asr_models::transcribe_qwen(s, &source, &paths, &language, threads).map_err(bad)?;
        result["bytes"] = json!(bytes);
        result["source"] = json!(source.to_string_lossy());
        return Ok(result);
    }
    let base = p.get("url").and_then(Value::as_str).map(str::trim).filter(|url| !url.is_empty()).map(str::to_string).unwrap_or_else(|| s.prefs.nl.asr_url());
    if !base.starts_with("http://") {
        return Err(bad(format!("{base}: only http:// endpoints are supported")));
    }
    let endpoint = if base.contains("/audio/transcriptions") { base.clone() } else { format!("{}/audio/transcriptions", base.trim_end_matches('/')) };
    let language = p
        .get("language")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|language| !language.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| s.prefs.nl.asr_language());
    let model = p
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
        .or_else(|| Some(s.prefs.nl.asr_model.clone()).filter(|model| !model.trim().is_empty() && !model.contains(['/', '\\'])))
        .unwrap_or_else(|| "whisper-1".to_string());
    let bytes = std::fs::metadata(&source).map(|metadata| metadata.len()).unwrap_or(0);
    if bytes > 512 * 1024 * 1024 {
        return Err(bad(format!("{} is larger than 512 MB: transcribe a smaller excerpt", source.display())));
    }
    if p.get("dryRun").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(json!({
            "dryRun": true,
            "submitted": false,
            "provider": provider,
            "endpoint": endpoint,
            "model": model,
            "language": language,
            "source": source.to_string_lossy(),
            "bytes": bytes,
        }));
    }
    let file = std::fs::read(&source).map_err(|e| bad(format!("{}: {e}", source.display())))?;
    let boundary = format!("----NLVideoSuite{}", SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_millis()).unwrap_or(0));
    let file_name = source.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_else(|| "audio".into());
    let mut fields: Vec<(&str, &str)> = vec![("model", model.as_str()), ("response_format", "json")];
    if !language.is_empty() && language != "auto" {
        fields.push(("language", language.as_str()));
    }
    let body = multipart(&boundary, &file_name, &file, &fields);
    let content_type = format!("multipart/form-data; boundary={boundary}");
    let mut headers: Vec<(&str, String)> = Vec::new();
    if let Some(key) = s.prefs.nl.asr_api_key().filter(|key| !key.trim().is_empty()) {
        headers.push(("Authorization", format!("Bearer {}", key.trim())));
    }
    let timeout = Duration::from_secs(p.get("timeoutSeconds").and_then(Value::as_u64).unwrap_or(600).clamp(30, 3600));
    let reply = http::post_bytes_with_headers(&endpoint, &body, &content_type, &headers, timeout).map_err(bad)?;
    let value: Value = serde_json::from_slice(&reply).map_err(|e| bad(format!("{endpoint}: response is not JSON: {e}")))?;
    let text = value
        .get("text")
        .or_else(|| value.get("transcription"))
        .or_else(|| value.get("result"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| bad(format!("{endpoint}: the answer has no text field")))?;
    Ok(json!({
        "provider": provider,
        "endpoint": endpoint,
        "model": model,
        "language": language,
        "languageDetected": value.get("language").cloned().unwrap_or(Value::Null),
        "text": text,
        "segments": value.get("segments").and_then(Value::as_array).map(Vec::len),
        "bytes": file.len(),
    }))
}
/// `nl.asr.models` — the speech models and engines found locally, with verification facts.
fn discover_models(s: &mut Session, _: &Value) -> Result<Value> {
    Ok(crate::nl::asr_models::discover(s))
}

pub(crate) fn commands() -> Vec<crate::CommandSpec> {
    vec![
        crate::CommandSpec {
            id: "nl.asr.models",
            label: "Discover speech models…",
            menu: &[],
            shortcut: None,
            params: "{}",
            enabled: crate::commands::always,
            run: discover_models,
            journal: false,
            named: crate::commands::NamedKeys::NONE,
        },
        crate::CommandSpec {
            id: "transcript.transcribeNl",
            label: "Transcribe with NL ASR…",
            menu: &["Sequence", "Transcript"],
            shortcut: None,
            params: r#"{"item":id?,"language":str?,"device":"auto|cpu|cuda"?,"wait":bool?}"#,
            enabled: crate::commands::always,
            run: transcribe,
            journal: false,
            named: crate::commands::NamedKeys::NONE,
        },
        crate::CommandSpec {
            id: "nl.asr.transcribe",
            label: "Transcribe with ASR provider…",
            menu: &[],
            shortcut: None,
            params: r#"{"path":str?,"item":id?,"language":str?,"model":str?,"url":str?,"backend":"bridge|openai_compatible|qwen_gguf"?,"runner":str?,"aligner":str?,"threads":int?,"dryRun":bool?,"timeoutSeconds":int?}"#,
            enabled: crate::commands::always,
            run: provider_transcribe,
            journal: false,
            named: crate::commands::NamedKeys::NONE,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_multipart_body_carries_the_file_and_the_fields() {
        let body = multipart("BND", "clip.mp4", b"abc", &[("model", "whisper-1"), ("language", "it")]);
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("name=\"model\""));
        assert!(text.contains("whisper-1"));
        assert!(text.contains("filename=\"clip.mp4\""));
        assert!(text.contains("abc"));
        assert!(text.trim_end().ends_with("--BND--"));
    }

    #[test]
    fn the_provider_command_validates_and_dry_runs_without_network() {
        let mut s = crate::Session::default();
        let file = std::env::temp_dir().join(format!("nl-asr-provider-{}.wav", std::process::id()));
        std::fs::write(&file, b"RIFF").unwrap();
        // the default backend is the bridge: this command refuses and points at transcript.transcribeNl
        assert!(s.execute("nl.asr.transcribe", json!({"path": file.to_string_lossy(), "dryRun": true})).is_err());
        s.prefs.nl.asr_provider = "openai_compatible".into();
        s.prefs.nl.asr_url = "http://127.0.0.1:8000/v1".into();
        let plan = s.execute("nl.asr.transcribe", json!({"path": file.to_string_lossy(), "dryRun": true})).unwrap();
        assert_eq!(plan["submitted"], false);
        assert_eq!(plan["endpoint"], "http://127.0.0.1:8000/v1/audio/transcriptions");
        assert!(s.execute("nl.asr.transcribe", json!({"path": file.to_string_lossy(), "url": "https://x", "dryRun": true})).is_err());
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn parses_word_timings_and_skips_invalid_entries() {
        let value = json!({"language": "it", "words": [
            {"word": "ciao", "start": 0.1, "end": 0.4},
            {"word": "", "start": 0.5, "end": 0.6},
            {"word": "male", "start": 0.9, "end": 0.3},
            {"text": "fine", "start": 0.7, "end": 0.9}
        ]});
        let transcript = parse_transcript(&value, "it").unwrap();
        assert_eq!(transcript.words.len(), 2);
        assert_eq!(transcript.words[0].text, "ciao");
        assert_eq!(transcript.words[1].text, "fine");
        assert!(transcript.words[1].start < transcript.words[1].end);
    }

    #[test]
    fn a_missing_words_array_is_an_error_not_an_empty_transcript() {
        assert!(parse_transcript(&json!({"segments": []}), "it").is_err());
    }
}
