//! Local vision descriptions through Ollama, used by the bridge to steer the transition prompt from
//! the incoming frame. Sequential with ComfyUI (the description happens before the generation is
//! submitted) and the model is unloaded right after (`keep_alive: "0"`).

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::http;

/// Standard base64 with padding (no dependency: it is a small, well-tested conversion).
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Ask the configured Ollama vision model to describe one image.
pub fn describe_image(url: &str, model: &str, image: &Path, prompt: &str, timeout: Duration) -> Result<String, String> {
    let bytes = std::fs::read(image).map_err(|e| format!("{}: {e}", image.display()))?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(format!("{}: image larger than 16 MB", image.display()));
    }
    let body = json!({
        "model": model,
        "prompt": prompt,
        "images": [base64(&bytes)],
        "stream": false,
        "keep_alive": "0",
    })
    .to_string();
    let endpoint = format!("{}/api/generate", url.trim().trim_end_matches('/'));
    let reply = http::request(http::Method::Post, &endpoint, Some(&body), timeout)?;
    let value: Value = serde_json::from_slice(&reply).map_err(|e| format!("{endpoint}: response is not JSON: {e}"))?;
    value
        .get("response")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{endpoint}: the vision model answered without text"))
}

/// The default question for the first frame of the incoming shot.
pub fn describe_incoming(url: &str, model: &str, image: &Path, timeout: Duration) -> Result<String, String> {
    describe_image(
        url,
        model,
        image,
        "Describe only what is visible in this frame in one short paragraph: subject and action, framing and camera angle, light, colour palette and environment. No names, no guesses about the plot, no text of the image.",
        timeout,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0, 0, 0]), "AAAA");
    }
}
