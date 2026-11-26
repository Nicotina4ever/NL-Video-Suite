//! Timeline analysis with a local lightweight vision model (LFM on llama.cpp or any
//! OpenAI-compatible vision endpoint). It samples frames of the sequence, asks the model for a
//! concise scene/OCR description and optionally transcribes the range with the configured ASR as a
//! reinforcing audio context. The result is a text block ready to guide new generations; nothing is
//! sent anywhere except the operator's own server.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use filmcraft_time::Tick;
use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{EngineError, Result, Session};

const MAX_FRAMES: usize = 12;
const VISION_TIMEOUT: Duration = Duration::from_secs(180);

fn bad_cmd(msg: impl ToString) -> EngineError {
    bad("nl.timeline.analyze", msg.to_string())
}

fn data_dir() -> PathBuf {
    crate::autosave::default_data_dir().unwrap_or_else(std::env::temp_dir)
}

/// The frame times (seconds) of the requested range.
fn frame_times(start: f64, end: f64, every: f64, max_frames: usize) -> Vec<f64> {
    let mut times = Vec::new();
    let every = every.max(0.5);
    let mut at = start.max(0.0);
    while at < end && times.len() < max_frames.min(MAX_FRAMES) {
        times.push((at * 1000.0).round() / 1000.0);
        at += every;
    }
    if times.is_empty() {
        times.push(start.max(0.0));
    }
    times
}

/// Ask an OpenAI-compatible vision model about one PNG.
pub(crate) fn ask_vision(endpoint: &str, model: &str, png: &[u8], prompt: &str) -> std::result::Result<String, String> {
    let image = super::vision::base64(png);
    let body = json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": prompt},
                {"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{image}")}}
            ]
        }],
        "max_tokens": 900,
        "temperature": 0.2,
    })
    .to_string();
    let reply = super::http::request(super::http::Method::Post, endpoint, Some(&body), VISION_TIMEOUT);
    let parsed = reply.and_then(|reply| serde_json::from_slice::<Value>(&reply).map_err(|e| format!("{endpoint}: response is not JSON: {e}")));
    if let Ok(value) = &parsed {
        let content = &value["choices"][0]["message"]["content"];
        if let Some(text) = content.as_str()
            && !text.trim().is_empty()
        {
            return Ok(text.trim().to_string());
        }
        if let Some(parts) = content.as_array() {
            let text = parts.iter().filter_map(|part| part["text"].as_str()).collect::<Vec<_>>().join(" ");
            if !text.trim().is_empty() {
                return Ok(text.trim().to_string());
            }
        }
        if let Some(reasoning) = value["choices"][0]["message"]["reasoning"].as_str()
            && !reasoning.trim().is_empty()
        {
            return Ok(reasoning.trim().chars().take(1200).collect());
        }
    }
    // Ollama's OpenAI-compatible endpoint rejects some image payloads; its native /api/chat takes
    // the same model and image with `images: [base64]` and is what lfm3-omni expects.
    if endpoint.contains("11434") {
        let base = endpoint.trim_end_matches('/').trim_end_matches("/v1").trim_end_matches("/chat/completions");
        let body = json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt, "images": [image]}],
            "stream": false,
            "options": {"num_predict": 900, "temperature": 0.2}
        })
        .to_string();
        let reply = super::http::request(super::http::Method::Post, &format!("{base}/api/chat"), Some(&body), VISION_TIMEOUT)?;
        let value: Value = serde_json::from_slice(&reply).map_err(|e| format!("{base}: response is not JSON: {e}"))?;
        if let Some(text) = value["message"]["content"].as_str()
            && !text.trim().is_empty()
        {
            return Ok(text.trim().to_string());
        }
        if let Some(text) = value["message"]["thinking"].as_str()
            && !text.trim().is_empty()
        {
            return Ok(text.trim().chars().take(1200).collect());
        }
        return Err(format!("{base}: Ollama returned no vision answer"));
    }
    Err(parsed.err().unwrap_or_else(|| format!("{endpoint}: the vision answer has no content")))
}

/// The model the endpoint declares, when the preference is empty.
pub(crate) fn first_model(endpoint: &str) -> std::result::Result<String, String> {
    let base = endpoint.trim_end_matches('/').trim_end_matches("/chat/completions");
    let value = super::http::get_json(base, "models", Duration::from_secs(20))?;
    value["data"]
        .as_array()
        .and_then(|list| list.first())
        .and_then(|model| model["id"].as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("{base}: no model declared"))
}

pub(crate) fn vision_endpoint(s: &Session, p: &Value) -> String {
    let base = p
        .get("visionUrl")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if s.prefs.nl.vision_url.trim().is_empty() {
                format!("{}/v1", s.prefs.nl.ollama_url().trim_end_matches('/'))
            } else {
                s.prefs.nl.vision_url()
            }
        });
    let base = base.trim_end_matches('/').to_string();
    if base.ends_with("/chat/completions") { base } else { format!("{base}/chat/completions") }
}

/// `nl.timeline.analyze` — sample the sequence with a local vision model and optional ASR.
fn analyze(s: &mut Session, p: &Value) -> Result<Value> {
    let Some(sequence) = s.active_sequence() else { return Err(EngineError::NoSequence) };
    let duration = sequence.duration().seconds();
    if duration <= 0.0 {
        return Err(bad_cmd("the sequence is empty"));
    }
    let start = p.get("startSeconds").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, duration);
    let end = p.get("endSeconds").and_then(Value::as_f64).unwrap_or(duration).clamp(start, duration);
    if end <= start {
        return Err(bad_cmd("the range is empty"));
    }
    let every = p.get("everySeconds").and_then(Value::as_f64).unwrap_or(5.0).clamp(0.5, 120.0);
    let max_frames = p.get("maxFrames").and_then(Value::as_u64).unwrap_or(6).clamp(1, MAX_FRAMES as u64) as usize;
    let include_audio = p.get("includeAudio").and_then(Value::as_bool).unwrap_or(false);
    let times = frame_times(start, end, every, max_frames);
    let endpoint = vision_endpoint(s, p);
    if !endpoint.starts_with("http://") {
        return Err(bad_cmd(format!("{endpoint}: only http:// endpoints are supported")));
    }
    let model = match p
        .get("visionModel")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
        .or_else(|| Some(s.prefs.nl.vision_model.trim().to_string()).filter(|model| !model.is_empty()))
    {
        Some(model) => model,
        None => first_model(&endpoint).map_err(bad_cmd)?,
    };
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_millis()).unwrap_or(0);
    let dir = data_dir().join("nl").join("analysis").join(stamp.to_string());
    std::fs::create_dir_all(&dir).map_err(|e| bad_cmd(format!("{}: {e}", dir.display())))?;
    if p.get("dryRun").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(json!({
            "dryRun": true,
            "submitted": false,
            "endpoint": endpoint,
            "model": model,
            "range": {"startSeconds": start, "endSeconds": end, "everySeconds": every},
            "times": times,
            "includeAudio": include_audio,
        }));
    }
    let prompt = "Analizza questo fotogramma di un video. Rispondi in italiano in massimo 3 frasi: scena e soggetti, azione, testo o scritte visibili (OCR), inquadratura/camera, luci e colori. Non inventare: se un testo non è leggibile dillo.";
    let mut frames = Vec::new();
    for (index, at) in times.iter().enumerate() {
        let png = dir.join(format!("frame-{index:02}.png"));
        s.execute("file.exportFrame", json!({"target": "program", "format": "png", "path": png.to_string_lossy(), "time": Tick::from_seconds_f64(*at).0}))?;
        let bytes = std::fs::read(&png).map_err(|e| bad_cmd(format!("{}: {e}", png.display())))?;
        let description = ask_vision(&endpoint, &model, &bytes, prompt).map_err(bad_cmd)?;
        frames.push(json!({"timeSeconds": at, "description": description, "frame": png.to_string_lossy()}));
    }
    let mut audio = Value::Null;
    if include_audio {
        let wav = dir.join("range.wav");
        let export = s.execute(
            "file.exportMedia",
            json!({
                "path": wav.to_string_lossy(),
                "format": "wav",
                "range": "custom",
                "startSeconds": start,
                "endSeconds": end,
                "wait": true,
            }),
        );
        match export {
            Ok(_) => {
                let backend = s.prefs.nl.asr_provider();
                let asr = s.execute("nl.asr.transcribe", json!({"path": wav.to_string_lossy(), "backend": backend, "language": s.prefs.nl.asr_language()}));
                match asr {
                    Ok(result) => {
                        audio = json!({"text": result["text"].clone(), "language": result["language"].clone(), "provider": result["provider"].clone()})
                    }
                    Err(e) => audio = json!({"error": e.to_string()}),
                }
            }
            Err(e) => audio = json!({"error": e.to_string()}),
        }
    }
    let mut context = format!("Analisi della timeline (vision {model}, {:.1}–{:.1} s):", start, end);
    for frame in &frames {
        context.push_str(&format!("\n- a {:.1} s: {}", frame["timeSeconds"].as_f64().unwrap_or(0.0), frame["description"].as_str().unwrap_or("")));
    }
    if let Some(text) = audio.get("text").and_then(Value::as_str) {
        context.push_str(&format!("\n- Audio: {text}"));
    }
    let result = json!({
        "range": {"startSeconds": start, "endSeconds": end, "everySeconds": every},
        "frames": frames,
        "audio": audio,
        "context": context,
        "model": model,
        "endpoint": endpoint,
        "folder": dir.to_string_lossy(),
    });
    s.nl.results.insert("analysis", result.clone());
    // Sidecar next to the project, when it is saved.
    if let Some(project) = s.path.as_deref().map(std::path::Path::new)
        && let Some(parent) = project.parent()
    {
        let sidecar = parent.join("nl");
        let _ = std::fs::create_dir_all(&sidecar);
        let _ = std::fs::write(sidecar.join("analysis.json"), serde_json::to_vec_pretty(&result).unwrap_or_default());
    }
    Ok(result)
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "nl.timeline.analyze",
        label: "Analyze Timeline with Vision",
        menu: &[],
        shortcut: None,
        params: r#"{"startSeconds":f?,"endSeconds":f?,"everySeconds":f?,"maxFrames":int?,"includeAudio":bool?,"visionUrl":str?,"visionModel":str?,"dryRun":bool?"}"#,
        enabled: always,
        run: analyze,
        journal: false,
        named: crate::commands::NamedKeys::NONE,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_times_cover_the_range_and_respect_the_cap() {
        let times = frame_times(0.0, 10.0, 4.0, 3);
        assert_eq!(times, vec![0.0, 4.0, 8.0]);
        assert_eq!(frame_times(2.0, 3.0, 10.0, 5), vec![2.0]);
        assert!(frame_times(0.0, 100.0, 1.0, 99).len() <= MAX_FRAMES);
    }
}
