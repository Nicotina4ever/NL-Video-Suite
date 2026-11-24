//! Status of the local AI services and the paths the NL features need, without starting anything.
//!
//! The panels show this so "ComfyUI non risponde" is a visible fact with its address, not a job
//! that hangs. Starting or stopping a service is an explicit operator action (P6) and never a side
//! effect of opening a panel.

use std::time::Duration;

use serde_json::{Value, json};

use super::{http, models};
use crate::Session;

/// A status check answers within this; a busy machine is reported, not waited on.
const CHECK_TIMEOUT: Duration = Duration::from_secs(2);

fn comfy_status(s: &Session) -> Value {
    let url = s.prefs.nl.comfy_url();
    match http::get_json(&url, "system_stats", CHECK_TIMEOUT) {
        Ok(value) => json!({
            "url": url,
            "reachable": true,
            "system": value.get("system").cloned().unwrap_or(Value::Null),
            "devices": value.get("devices").cloned().unwrap_or(Value::Null),
        }),
        Err(e) => json!({"url": url, "reachable": false, "error": e}),
    }
}

fn ollama_status(s: &Session) -> Value {
    let url = s.prefs.nl.ollama_url();
    match http::get_json(&url, "api/version", CHECK_TIMEOUT) {
        Ok(value) => json!({
            "url": url,
            "reachable": true,
            "version": value.get("version").cloned().unwrap_or(Value::Null),
            "model": s.prefs.nl.ollama_model(),
        }),
        Err(e) => json!({"url": url, "reachable": false, "error": e, "model": s.prefs.nl.ollama_model()}),
    }
}

/// The configured language-model provider: which one, where, and whether it answers.
fn llm_status(s: &Session) -> Value {
    let provider = s.prefs.nl.llm_provider();
    let url = s.prefs.nl.llm_url();
    let model = s.prefs.nl.llm_model();
    let key = s.prefs.nl.llm_api_key().is_some();
    let reachable =
        if provider == "ollama" { http::get_json(&url, "api/version", CHECK_TIMEOUT).is_ok() } else { http::get_json(&url, "models", CHECK_TIMEOUT).is_ok() };
    json!({"provider": provider, "url": url, "model": model, "apiKeyConfigured": key, "reachable": reachable})
}

/// `nl.services.status`: both local services with the address they were checked at, plus the
/// language-model provider actually in use.
pub fn services(s: &mut Session) -> Value {
    json!({"comfy": comfy_status(s), "ollama": ollama_status(s), "llm": llm_status(s)})
}

/// `nl.diagnostics`: services, folders, tools and counts in one report.
pub fn report(s: &mut Session) -> Value {
    let root = super::suite_root();
    let root_text = root.as_ref().map(|p| p.to_string_lossy().to_string());
    let exists = |relative: &str| root.as_ref().is_some_and(|r| r.join(relative).exists());
    let workflow_root = models::root(s);
    let workflow_count = models::workflows(s, "all").map(|l| l.len()).unwrap_or(0);
    json!({
        "app": {"name": "NL Video Suite", "version": env!("CARGO_PKG_VERSION")},
        "suiteRoot": root_text,
        "services": services(s),
        "tools": {
            "ffmpeg": exists("app/tools/ffmpeg.exe") || on_path("ffmpeg"),
            "ffprobe": exists("app/tools/ffprobe.exe") || on_path("ffprobe"),
            "python": exists("runtime/python/python.exe"),
            "asrBridge": exists("sviluppo/bridge/clipsai_bridge.py"),
            "asrModel": exists("models/asr/faster-whisper-large-v3"),
            "alignment": exists("models/alignment/italian-xlsr53"),
        },
        "workflows": {"root": workflow_root.map(|p| p.to_string_lossy().to_string()), "count": workflow_count},
        "preferences": {
            "imageScale": s.prefs.nl.image_scale,
            "videoScale": s.prefs.nl.video_scale,
            "sizeMultiple": s.prefs.nl.size_multiple,
            "asrDevice": s.prefs.nl.asr_device(),
            "asrLanguage": s.prefs.nl.asr_language(),
            "gpuFirst": s.prefs.nl.gpu_first,
            "asrCache": s.prefs.nl.asr_cache,
        },
    })
}

fn on_path(tool: &str) -> bool {
    let exe = format!("{tool}.exe");
    std::env::var_os("PATH").map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(&exe).is_file() || dir.join(tool).is_file())).unwrap_or(false)
}
