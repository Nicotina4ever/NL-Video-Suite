//! Quick segment analysis with a short-lived llama.cpp server. The model is loaded only for the
//! request (random loopback port), then the child process is killed so the VRAM is released; the
//! same request carries the exported frames and the audio of the exact timeline segment.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

use crate::{EngineError, Result, Session};

const DEFAULT_SERVER: &str = r"K:\AI\LlamaCPP\llama-server.exe";
const DEFAULT_MODEL: &str = r"K:\AI\LlamaCPP\Modello\LFM3\LFM3-Omni-2B-Q8_0.gguf";
const DEFAULT_MMPROJ: &str = r"K:\AI\LlamaCPP\Modello\LFM3\mmproj-LFM3-Omni-2B-Q8_0.gguf";

fn bad(msg: impl ToString) -> EngineError {
    EngineError::BadParams { cmd: "nl.analyze.segment".into(), msg: msg.to_string() }
}

struct ServerGuard(Option<Child>);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn first_path(p: &Value, key: &str, env: &str, configured: &str, default: &str) -> PathBuf {
    if let Some(path) = p.get(key).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()) {
        return PathBuf::from(path);
    }
    if !configured.trim().is_empty() {
        return PathBuf::from(configured.trim());
    }
    if let Ok(path) = std::env::var(env) {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    PathBuf::from(default)
}

fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| bad(format!("no free loopback port: {e}")))?;
    listener.local_addr().map(|addr| addr.port()).map_err(|e| bad(e.to_string()))
}

fn start_server(server: &Path, model: &Path, mmproj: &Path, port: u16, context: i64) -> Result<ServerGuard> {
    if !server.is_file() {
        return Err(bad(format!("llama-server not found: {}", server.display())));
    }
    if !model.is_file() {
        return Err(bad(format!("GGUF model not found: {}", model.display())));
    }
    let mut command = Command::new(server);
    command
        .arg("-m")
        .arg(model)
        .arg("--mmproj")
        .arg(mmproj)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--no-ui")
        .arg("-c")
        .arg(context.to_string())
        .arg("-ngl")
        .arg("99")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let child = command.spawn().map_err(|e| bad(format!("cannot start llama-server: {e}")))?;
    let guard = ServerGuard(Some(child));
    let base = format!("http://127.0.0.1:{port}");
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    while std::time::Instant::now() < deadline {
        if crate::nl::http::request(crate::nl::http::Method::Get, &format!("{base}/health"), None, Duration::from_secs(2)).is_ok() {
            return Ok(guard);
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    Err(bad("llama-server did not become ready within 120 s"))
}

fn export_frames(s: &mut Session, start: f64, end: f64, dir: &Path) -> Vec<PathBuf> {
    let _ = std::fs::create_dir_all(dir);
    let times = if end - start < 0.5 { vec![start] } else { vec![start, (start + end) / 2.0, end] };
    let mut frames = Vec::new();
    for (index, seconds) in times.iter().enumerate() {
        let path = dir.join(format!("segment-{index}.png"));
        let ticks = (*seconds * 254_016_000_000.0) as i64;
        if s.execute("file.exportFrame", json!({"target": "program", "format": "png", "path": path.to_string_lossy(), "time": ticks})).is_ok() {
            frames.push(path);
        }
    }
    frames
}

fn export_audio(s: &mut Session, start: f64, end: f64, dir: &Path) -> Option<PathBuf> {
    let path = dir.join("segment.wav");
    let result = s.execute(
        "file.exportMedia",
        json!({"path": path.to_string_lossy(), "format": "wav", "range": "custom", "startSeconds": start, "endSeconds": end, "audio": true, "wait": true}),
    );
    if result.is_ok() && path.is_file() { Some(path) } else { None }
}

fn read_base64(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(crate::nl::vision::base64(&bytes))
}

/// `nl.analyze.segment` — short image+audio analysis of the exact timeline range.
fn analyze(s: &mut Session, p: &Value) -> Result<Value> {
    let start = p.get("startSeconds").and_then(Value::as_f64).unwrap_or(0.0).max(0.0);
    let end = p.get("endSeconds").and_then(Value::as_f64).filter(|end| *end > start).unwrap_or(start + 5.0);
    let prompt = p
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("Analizza questo segmento video: descrivi le immagini, trascrivi il parlato/audio e suggerisci un prompt video coerente. Rispondi in italiano, conciso.")
        .to_string();
    let server = first_path(p, "llamaServer", "NL_LLAMA_SERVER", &s.prefs.nl.llama_server_path(), DEFAULT_SERVER);
    let model = first_path(p, "model", "NL_LLAMA_MODEL", &s.prefs.nl.llama_model_path(), DEFAULT_MODEL);
    let mmproj = first_path(p, "mmproj", "NL_LLAMA_MMPROJ", &s.prefs.nl.llama_mmproj_path(), DEFAULT_MMPROJ);
    let dir = crate::autosave::default_data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("nl")
        .join("segment-analysis")
        .join(format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)));
    let frames = export_frames(s, start, end, &dir);
    let audio = export_audio(s, start, end, &dir);
    if frames.is_empty() && audio.is_none() {
        return Err(bad("nothing to analyse: could not export a frame or the audio of the segment"));
    }
    let context = p.get("context").and_then(Value::as_i64).unwrap_or(s.prefs.nl.llama_context).clamp(2048, 131072);
    let port = free_port()?;
    let _guard = start_server(&server, &model, &mmproj, port, context)?;
    let mut parts: Vec<Value> = vec![json!({"type": "text", "text": prompt})];
    for frame in &frames {
        if let Some(data) = read_base64(frame) {
            parts.push(json!({"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{data}")}}));
        }
    }
    if let Some(audio) = &audio
        && let Some(data) = read_base64(audio)
    {
        parts.push(json!({"type": "input_audio", "input_audio": {"data": data, "format": "wav"}}));
    }
    let body = json!({
        "model": "lfm3-omni",
        "messages": [{"role": "user", "content": parts}],
        "max_tokens": 700,
        "temperature": 0.2
    })
    .to_string();
    let reply = crate::nl::http::request(
        crate::nl::http::Method::Post,
        &format!("http://127.0.0.1:{port}/v1/chat/completions"),
        Some(&body),
        Duration::from_secs(300),
    )
    .map_err(|e| bad(format!("llama-server analysis failed: {e}")))?;
    let value: Value = serde_json::from_slice(&reply).map_err(|e| bad(format!("llama-server returned invalid JSON: {e}")))?;
    let text = value["choices"][0]["message"]["content"]
        .as_str()
        .or_else(|| value["choices"][0]["message"]["reasoning"].as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| bad("llama-server returned no text"))?
        .to_string();
    Ok(json!({
        "segment": {"startSeconds": start, "endSeconds": end},
        "text": text,
        "frames": frames.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
        "audio": audio.map(|p| p.to_string_lossy().to_string()),
        "server": server.to_string_lossy(),
        "model": model.to_string_lossy(),
        "port": port
    }))
}

pub(crate) fn commands() -> Vec<crate::CommandSpec> {
    vec![crate::CommandSpec {
        id: "nl.analyze.segment",
        label: "NL Segment Analysis",
        menu: &[],
        shortcut: None,
        params: r#"{"startSeconds":f?,"endSeconds":f?,"prompt":str?,"llamaServer":str?,"model":str?,"mmproj":str?}"#,
        enabled: crate::commands::always,
        run: analyze,
        journal: false,
        named: crate::commands::NamedKeys::NONE,
    }]
}

