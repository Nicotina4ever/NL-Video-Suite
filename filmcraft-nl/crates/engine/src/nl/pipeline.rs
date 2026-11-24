//! The bridge pipeline: clipping proposals and renders, rough-cut analysis and renders.
//!
//! The heavy lifting lives in the vendored engines under `engines/clipsai`, run through
//! `sviluppo/bridge/clipsai_bridge.py` with a request/result JSON pair (the same bridge the NL ASR
//! uses). This module owns the request, the progress heartbeat, the timeout and the session state;
//! it never edits the project by itself — rendered files are imported as media when the job
//! succeeds, and a failure is reported as a failure.
#![cfg(not(target_arch = "wasm32"))]

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::{EngineError, Result, Session};

use super::state::PendingNl;

/// One bridge call: an operation, the media it reads and its own items/parameters.
pub struct BridgeCall {
    /// Bridge operation: `clips`, `reels`, `cleanup`, `render_clips`, `render_reels`,
    /// `render_cleanup`.
    pub operation: &'static str,
    pub media: String,
    pub language: String,
    /// Operation parameters (thresholds, model, flags) passed through to the engines.
    pub params: Value,
    /// Items for the render/selection operations (proposals, intervals).
    pub items: Option<Value>,
    /// Clipping mode (`1` stories, `2` reels, `3` narrative reel) when the operation takes one.
    pub mode: Option<String>,
    /// Render a vertical 9:16 copy next to the master.
    pub vertical: bool,
}

fn bad(e: impl ToString) -> EngineError {
    EngineError::BadParams { cmd: "nl.pipeline".into(), msg: e.to_string() }
}

/// Start one bridge operation in the background and return its job.
pub fn start(s: &mut Session, label: String, call: BridgeCall) -> Result<Value> {
    let root = super::suite_root().ok_or_else(|| bad("the NL suite folder was not found (set NL_SUITE_ROOT)"))?;
    let work = super::media_work_dir(s, &call.media, "pipeline");
    std::fs::create_dir_all(&work).map_err(|e| bad(format!("{}: {e}", work.display())))?;
    let id = s.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let job = crate::Job { id, label, progress: Default::default(), result: Default::default() };
    let (prog, res) = (job.progress.clone(), job.result.clone());
    let result: Arc<Mutex<Option<std::result::Result<Value, String>>>> = Arc::default();
    let out = result.clone();
    let timeout = s.prefs.nl.queue_timeout();
    let ollama_url = s.prefs.nl.ollama_url();
    let model = s.prefs.nl.ollama_model();
    let model_dir = asr_model_dir(&root, &s.prefs.nl.asr_model);
    let operation = call.operation;
    let media = call.media.clone();
    let epoch = s.document_epoch;
    let work_run = work.clone();
    s.jobs.push(job);
    s.nl.pending.push(PendingNl { job: id, operation, result, epoch });
    std::thread::Builder::new()
        .name("filmcraft-nl-pipeline".into())
        .spawn(move || {
            let heartbeat = Heartbeat::start(&work_run, prog.clone());
            let outcome = run(&root, &call, &work_run, &ollama_url, &model, &model_dir, timeout, &prog);
            heartbeat.stop();
            let value = match outcome {
                Ok(value) => {
                    *res.lock().unwrap_or_else(|e| e.into_inner()) = Some(Ok(filmcraft_export::Report::default()));
                    Ok(value)
                }
                Err(e) => {
                    *res.lock().unwrap_or_else(|e| e.into_inner()) = Some(Err(e.clone()));
                    Err(e)
                }
            };
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
            prog.done.store(1, Ordering::Relaxed);
            prog.finished.store(true, Ordering::Relaxed);
        })
        .map_err(|e| EngineError::Other(format!("can't start the NL pipeline thread: {e}")))?;
    Ok(json!({"job": id, "operation": operation, "media": media, "work": work.to_string_lossy()}))
}

/// Move finished results into the session and report failures.
pub(crate) fn poll(s: &mut Session) {
    let mut i = 0;
    while i < s.nl.pending.len() {
        let finished = s.jobs.iter().find(|j| j.id == s.nl.pending[i].job).is_none_or(|j| j.progress.finished.load(Ordering::Relaxed));
        if !finished {
            i += 1;
            continue;
        }
        let pending = s.nl.pending.remove(i);
        let Some(result) = pending.result.lock().unwrap_or_else(|e| e.into_inner()).take() else { continue };
        match result {
            Ok(value) => {
                s.nl.results.insert(pending.operation, value.clone());
                let files = files_of(&value);
                if !files.is_empty() {
                    s.nl.files = files.clone();
                    let mut imported = Vec::new();
                    let mut errors = Vec::new();
                    for file in &files {
                        match crate::commands::import_streamed(s, file, None) {
                            Ok(item) => imported.push(json!({"item": item.0, "path": file})),
                            Err(e) => errors.push(format!("{file}: {e}")),
                        }
                    }
                    if errors.is_empty() {
                        s.toast(format!("NL: imported {} file(s) into the project", imported.len()));
                    } else {
                        s.error_toast("nl.pipeline", format!("Imported {} file(s); {} failed: {}", imported.len(), errors.len(), errors.join(" | ")));
                    }
                } else {
                    let count = match pending.operation {
                        "clips" => value["clips"].as_array().map_or(0, Vec::len),
                        "reels" => value["reels"].as_array().map_or(0, Vec::len),
                        "cleanup" => value["cuts"].as_array().map_or(0, Vec::len),
                        _ => 0,
                    };
                    s.toast(format!("NL {}: {count} result(s)", pending.operation));
                }
            }
            Err(e) => s.error_toast("nl.pipeline", format!("{}: {e}", pending.operation)),
        }
    }
}

/// The files a render result names.
fn files_of(value: &Value) -> Vec<String> {
    value
        .get("files")
        .and_then(Value::as_array)
        .map(|files| files.iter().filter_map(Value::as_str).map(str::to_string).filter(|f| Path::new(f).is_file()).collect())
        .unwrap_or_default()
}

/// A heartbeat over the bridge's `progress.json`, so a long transcription shows it is alive.
struct Heartbeat {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Heartbeat {
    fn start(work: &Path, progress: Arc<filmcraft_export::Progress>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let path = work.join("progress.json");
        let thread = std::thread::Builder::new()
            .name("filmcraft-nl-heartbeat".into())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    if let Ok(text) = std::fs::read_to_string(&path)
                        && let Ok(value) = serde_json::from_str::<Value>(&text)
                        && let Some(stage) = value.get("stage").and_then(Value::as_str)
                    {
                        *progress.status.lock().unwrap_or_else(|e| e.into_inner()) = stage.to_string();
                    }
                    std::thread::sleep(Duration::from_millis(700));
                }
            })
            .ok();
        Self { stop, thread }
    }

    fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Run the bridge once and return its result document.
#[allow(clippy::too_many_arguments)]
fn run(
    root: &Path,
    call: &BridgeCall,
    work: &Path,
    ollama_url: &str,
    model: &str,
    model_dir: &Path,
    timeout: Duration,
    progress: &filmcraft_export::Progress,
) -> std::result::Result<Value, String> {
    let python = root.join("runtime/python/python.exe");
    let bridge = root.join("sviluppo/bridge/clipsai_bridge.py");
    if !python.is_file() || !bridge.is_file() {
        return Err(format!("NL runtime/bridge not found under {}", root.display()));
    }
    let request = json!({
        "source_root": root.join("engines/clipsai").to_string_lossy(),
        "input_path": call.media,
        "work_dir": work.to_string_lossy(),
        "operation": call.operation,
        "language": call.language,
        "params": call.params,
        "items": call.items.clone().unwrap_or(Value::Null),
        "mode": call.mode.clone().unwrap_or_default(),
        "vertical": call.vertical,
        "progress_path": work.join("progress.json").to_string_lossy(),
        "ollama_url": ollama_url,
        "model": model,
        "llm_provider": "ollama",
    });
    let request_path = work.join(format!("request-{}.json", call.operation));
    let result_path = work.join(format!("result-{}.json", call.operation));
    std::fs::write(&request_path, serde_json::to_vec_pretty(&request).map_err(|e| e.to_string())?).map_err(|e| format!("{}: {e}", request_path.display()))?;
    let mut command = Command::new(&python);
    command
        .arg(&bridge)
        .arg(&request_path)
        .arg(&result_path)
        .env("NL_ALIGNMENT_MODEL_PATH", root.join("models/alignment/italian-xlsr53"))
        .env("CLIPSAI_WHISPER_MODEL", model_dir)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut paths = vec![root.join("app/tools")];
    if let Some(old) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&old));
    }
    if let Ok(path) = std::env::join_paths(paths) {
        command.env("PATH", path);
    }
    *progress.status.lock().unwrap_or_else(|e| e.into_inner()) = match call.operation {
        "cleanup" => "Rough cut: transcription, silences, breaths, fillers and retakes".into(),
        "clips" | "reels" => "Clipping: transcription and editorial selection".into(),
        _ => "Rendering the selected pieces".into(),
    };
    let mut child = command.spawn().map_err(|e| format!("can't start {}: {e}", python.display()))?;
    // Drain stdout so the bridge never blocks on a full pipe; its lines are progress chatter.
    let stdout = child.stdout.take();
    let reader = std::thread::Builder::new()
        .name("filmcraft-nl-bridge-out".into())
        .spawn(move || {
            if let Some(stdout) = stdout {
                let mut last = String::new();
                for line in std::io::BufReader::new(stdout).lines().map_while(std::result::Result::ok) {
                    if let Ok(value) = serde_json::from_str::<Value>(&line)
                        && let Some(message) = value.get("message").and_then(Value::as_str)
                        && !message.trim().is_empty()
                    {
                        last = message.to_string();
                    }
                }
                last
            } else {
                String::new()
            }
        })
        .ok();
    // Drain stderr too: a verbose engine must never block on a full pipe, and its tail is the
    // most useful part of a failure.
    let stderr = child.stderr.take();
    let err_reader = std::thread::Builder::new()
        .name("filmcraft-nl-bridge-err".into())
        .spawn(move || {
            let mut tail = String::new();
            if let Some(stderr) = stderr {
                for line in std::io::BufReader::new(stderr).lines().map_while(std::result::Result::ok) {
                    tail.push_str(&line);
                    tail.push('\n');
                    if tail.len() > 4000 {
                        tail = tail.split_off(tail.len() - 4000);
                    }
                }
            }
            tail
        })
        .ok();
    let started = web_time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let last = reader.and_then(|r| r.join().ok()).unwrap_or_default();
                let tail = err_reader.and_then(|r| r.join().ok()).unwrap_or_default();
                if !status.success() {
                    return Err(format!("bridge {last}: {status}\n{tail}").trim().to_string());
                }
                let bytes = std::fs::read(&result_path).map_err(|e| format!("{}: {e}", result_path.display()))?;
                return serde_json::from_slice(&bytes).map_err(|e| format!("bridge result is not JSON: {e}"));
            }
            Ok(None) => {}
            Err(e) => return Err(format!("bridge process: {e}")),
        }
        if progress.cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            return Err("cancelled".into());
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            return Err(format!("the bridge did not finish within {} s", timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// The speech model folder: the NL preference when it names a folder, else the portable model.
pub(crate) fn asr_model_dir(root: &Path, configured: &str) -> PathBuf {
    let configured = configured.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        let path = if path.is_absolute() { path } else { root.join(path) };
        if path.is_dir() {
            return path;
        }
    }
    root.join("models/asr/faster-whisper-large-v3")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_existing_files_are_imported() {
        let dir = crate::media_test_util::tmp_dir("nl-pipeline-files");
        let real = dir.join("clip.mp4");
        std::fs::write(&real, b"x").unwrap();
        let value = json!({"files": [real.to_string_lossy(), dir.join("missing.mp4").to_string_lossy()]});
        assert_eq!(files_of(&value), vec![real.to_string_lossy().to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_result_without_files_is_not_a_render() {
        assert!(files_of(&json!({"clips": []})).is_empty());
    }
}
