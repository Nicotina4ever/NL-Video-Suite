//! Minimal HTTP client for a local Ollama server (native only).
//!
//! FilmCraft talks to `127.0.0.1:11434` directly so the AI features work with the Ollama the
//! operator already runs. Only plain `http://` is supported: the endpoint is local by design.
#![cfg(not(target_arch = "wasm32"))]

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Chat with `model`; returns the assistant message text. `format` asks Ollama for JSON.
pub(crate) fn chat(base: &str, model: &str, system: &str, prompt: &str, timeout: Duration) -> Result<String, String> {
    chat_with(base, model, system, prompt, timeout, None)
}

/// Chat with `model`, optionally telling Ollama to unload it right after the answer
/// (`keep_alive: Some("0")`): used when a ComfyUI generation follows and both must not sit in VRAM
/// together.
pub(crate) fn chat_with(base: &str, model: &str, system: &str, prompt: &str, timeout: Duration, keep_alive: Option<&str>) -> Result<String, String> {
    chat_with_format(base, model, system, prompt, timeout, keep_alive, true)
}

/// Plain-text chat (no `format: json`): used for prompts whose schema is the model's own text
/// format, such as MiniMax H3's section layout.
pub(crate) fn chat_text(base: &str, model: &str, system: &str, prompt: &str, timeout: Duration, keep_alive: Option<&str>) -> Result<String, String> {
    chat_with_format(base, model, system, prompt, timeout, keep_alive, false)
}

fn chat_with_format(
    base: &str,
    model: &str,
    system: &str,
    prompt: &str,
    timeout: Duration,
    keep_alive: Option<&str>,
    json_format: bool,
) -> Result<String, String> {
    let url = if base.contains("/api/chat") { base.to_string() } else { format!("{}/api/chat", base.trim_end_matches('/')) };
    let mut body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": prompt}
        ],
        "stream": false,
        "think": false,
        // A transcript window with timestamps is much longer than Ollama's default 4k context;
        // without this the model sees only the tail of the prompt and answers nothing useful.
        "options": {"temperature": 0.2, "num_predict": 2048, "num_ctx": 16384},
        "keep_alive": keep_alive
    });
    if json_format {
        body["format"] = serde_json::json!("json");
    }
    let body = body.to_string();
    let bytes = post(&url, &body, timeout)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| format!("ollama: response is not JSON: {e}"))?;
    value
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
        .ok_or_else(|| "ollama: response has no message.content".to_string())
}

fn post(url: &str, body: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    let rest = url.strip_prefix("http://").ok_or_else(|| "ollama: only http:// endpoints are supported".to_string())?;
    let (authority, path) = match rest.split_once('/') {
        Some((a, p)) => (a, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    let addr = authority
        .to_socket_addrs()
        .map_err(|e| format!("ollama: {authority}: {e}"))?
        .next()
        .ok_or_else(|| format!("ollama: {authority} resolved to no address"))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).map_err(|e| format!("ollama: connect {authority}: {e}"))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(15)));
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).map_err(|e| format!("ollama: write: {e}"))?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| format!("ollama: read: {e}"))?;
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| "ollama: malformed HTTP response (no header end)".to_string())?;
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let status = head.lines().next().unwrap_or_default();
    if !status.contains(" 200 ") {
        let body = String::from_utf8_lossy(&raw[split + 4..]);
        return Err(format!("ollama: {status}: {}", body.trim().chars().take(600).collect::<String>()));
    }
    let body = &raw[split + 4..];
    if head.to_ascii_lowercase().contains("transfer-encoding: chunked") { dechunk(body) } else { Ok(body.to_vec()) }
}

fn dechunk(mut body: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let line = body.windows(2).position(|w| w == b"\r\n").ok_or_else(|| "ollama: malformed chunked response".to_string())?;
        let size_text = String::from_utf8_lossy(&body[..line]);
        let size = usize::from_str_radix(size_text.trim().split(';').next().unwrap_or("0").trim(), 16).map_err(|_| "ollama: bad chunk size".to_string())?;
        body = &body[line + 2..];
        if size == 0 {
            return Ok(out);
        }
        if body.len() < size + 2 {
            return Err("ollama: truncated chunked response".into());
        }
        out.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn reads_a_chunked_json_response() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf).unwrap();
            let body = b"{\"message\":{\"content\":\"ciao\"}}";
            let head = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
            let _ = s.write_all(head.as_bytes());
            let _ = s.write_all(format!("{:x}\r\n", body.len()).as_bytes());
            let _ = s.write_all(body);
            let _ = s.write_all(b"\r\n0\r\n\r\n");
        });
        let reply = chat(&format!("http://127.0.0.1:{port}"), "m", "s", "p", Duration::from_secs(5)).unwrap();
        assert_eq!(reply, "ciao");
        server.join().unwrap();
    }
}
