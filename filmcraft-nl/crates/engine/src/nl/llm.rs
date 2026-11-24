//! The local language-model side of the NL features: reading the installed models and asking for
//! JSON answers (ranking, prompts and translation all go through here).
//!
//! Only plain `http://` endpoints are used, no model is ever pulled, and a failed call says what
//! failed rather than pretending an empty answer is a result.

use std::time::Duration;

use serde_json::Value;

use super::http;

/// How long one model listing may take.
const LIST_TIMEOUT: Duration = Duration::from_secs(3);

/// The models the local Ollama already has, newest listing first, cloud-only and embedding models
/// left out: they cannot answer offline, so offering them would be a trap.
pub fn models(base: &str) -> Result<Vec<Value>, String> {
    let value = http::get_json(base, "api/tags", LIST_TIMEOUT)?;
    let list = value.get("models").and_then(Value::as_array).ok_or_else(|| "the response has no `models` list".to_string())?;
    let mut out: Vec<Value> = list
        .iter()
        .filter_map(|m| {
            let name = m.get("name").and_then(Value::as_str)?.trim().to_string();
            if name.is_empty() || !usable(&name) {
                return None;
            }
            Some(serde_json::json!({
                "name": name,
                "family": m.get("details").and_then(|d| d.get("family")).and_then(Value::as_str),
                "parameters": m.get("details").and_then(|d| d.get("parameter_size")).and_then(Value::as_str),
                "bytes": m.get("size").and_then(Value::as_u64),
            }))
        })
        .collect();
    out.sort_by(|a, b| a["name"].as_str().unwrap_or_default().cmp(b["name"].as_str().unwrap_or_default()));
    Ok(out)
}

/// Whether a model name can answer with a local generation (no cloud service, no embeddings).
pub fn usable(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !(lower.contains("cloud") || lower.contains("embed"))
}

/// Ask the model for a JSON answer and parse it; the caller decides what the shape means.
pub fn chat_json(base: &str, model: &str, system: &str, prompt: &str, timeout: Duration) -> Result<Value, String> {
    chat_json_with(base, model, system, prompt, timeout, None)
}

/// [`chat_json`] with an explicit Ollama `keep_alive` (e.g. `Some("0")` frees the VRAM at once).
pub fn chat_json_with(base: &str, model: &str, system: &str, prompt: &str, timeout: Duration, keep_alive: Option<&str>) -> Result<Value, String> {
    let text = crate::ollama::chat_with(base, model, system, prompt, timeout, keep_alive)?;
    json_from_text(&text)
}

/// The language-model endpoint actually in use: provider, URL, model and optional bearer token.
#[derive(Clone, Debug)]
pub struct LlmConfig {
    pub provider: String,
    pub url: String,
    pub model: String,
    pub api_key: Option<String>,
}

impl LlmConfig {
    /// The configured provider with an optional model override from the command parameters.
    pub fn for_session(s: &crate::Session, model_override: Option<&str>) -> Self {
        let model = model_override.map(str::trim).filter(|model| !model.is_empty()).map(str::to_string).unwrap_or_else(|| s.prefs.nl.llm_model());
        Self { provider: s.prefs.nl.llm_provider(), url: s.prefs.nl.llm_url(), model, api_key: s.prefs.nl.llm_api_key() }
    }

    /// A text answer from this provider.
    pub fn auto_text(&self, system: &str, prompt: &str, timeout: Duration) -> Result<String, String> {
        if self.provider.eq_ignore_ascii_case("ollama") {
            crate::ollama::chat_text(&self.url, &self.model, system, prompt, timeout, None)
        } else {
            openai_chat(&self.url, &self.model, system, prompt, timeout, self.api_key.as_deref(), false)
        }
    }

    /// A JSON answer from this provider; `keep_alive` only applies to Ollama.
    pub fn auto_json(&self, system: &str, prompt: &str, timeout: Duration, keep_alive: Option<&str>) -> Result<Value, String> {
        if self.provider.eq_ignore_ascii_case("ollama") {
            let text = crate::ollama::chat_with(&self.url, &self.model, system, prompt, timeout, keep_alive)?;
            json_from_text(&text)
        } else {
            let text = openai_chat(&self.url, &self.model, system, prompt, timeout, self.api_key.as_deref(), true)?;
            json_from_text(&text)
        }
    }
}

/// Extract the outermost JSON object or array from a model answer (it often wraps JSON in prose or
/// a code fence).
pub fn json_from_text(text: &str) -> Result<Value, String> {
    let text = text.trim();
    let start = text.find(['{', '[']).ok_or_else(|| "the model answered without JSON".to_string())?;
    let end = text.rfind(['}', ']']).ok_or_else(|| "the model answered without JSON".to_string())?;
    if end <= start {
        return Err("the model answered without JSON".into());
    }
    serde_json::from_str(&text[start..=end]).map_err(|e| format!("the model's JSON does not parse: {e}"))
}

/// Chat against an OpenAI-compatible server (`/v1/chat/completions`, llama.cpp, LM Studio, vLLM,
/// Ollama's own compatibility endpoint). JSON mode is requested first; a server that refuses
/// `response_format` is retried without it.
fn openai_chat(base: &str, model: &str, system: &str, prompt: &str, timeout: Duration, api_key: Option<&str>, json_format: bool) -> Result<String, String> {
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let mut headers: Vec<(&str, String)> = Vec::new();
    if let Some(key) = api_key.filter(|key| !key.trim().is_empty()) {
        headers.push(("Authorization", format!("Bearer {}", key.trim())));
    }
    let body = |with_json: bool| -> String {
        let mut value = serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.2,
            "stream": false
        });
        if with_json {
            value["response_format"] = serde_json::json!({"type": "json_object"});
        }
        value.to_string()
    };
    let post = |text: &str| -> Result<Value, String> {
        let bytes = http::request_with_headers(http::Method::Post, &url, Some(text), &headers, timeout)?;
        serde_json::from_slice(&bytes).map_err(|e| format!("{url}: response is not JSON: {e}"))
    };
    let value = match post(&body(json_format)) {
        Ok(value) => value,
        Err(error) if json_format => post(&body(false)).map_err(|second| format!("{error}; without `response_format`: {second}"))?,
        Err(error) => return Err(error),
    };
    value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .filter(|content| !content.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{url}: no choices[0].message.content in the answer"))
}

/// The models an OpenAI-compatible server declares (`GET /models`).
fn openai_models(base: &str) -> Result<Vec<Value>, String> {
    let value = http::get_json(base, "models", LIST_TIMEOUT)?;
    let list = value.get("data").or_else(|| value.get("models")).and_then(Value::as_array).ok_or_else(|| "the response has no model list".to_string())?;
    let mut out: Vec<Value> = list
        .iter()
        .filter_map(|model| {
            let name = model.get("id").or_else(|| model.get("name")).and_then(Value::as_str)?.trim().to_string();
            if name.is_empty() || !usable(&name) {
                return None;
            }
            Some(serde_json::json!({"name": name, "family": null, "parameters": null, "bytes": null}))
        })
        .collect();
    out.sort_by(|a, b| a["name"].as_str().unwrap_or_default().cmp(b["name"].as_str().unwrap_or_default()));
    Ok(out)
}

/// Models of the configured provider, with the endpoint used.
pub fn models_for(s: &crate::Session) -> Result<(String, Vec<Value>), String> {
    let llm = LlmConfig::for_session(s, None);
    let list = if llm.provider.eq_ignore_ascii_case("ollama") { models(&llm.url)? } else { openai_models(&llm.url)? };
    Ok((llm.url, list))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_provider_defaults_to_the_local_ollama_and_its_model() {
        let s = crate::Session::default();
        let llm = LlmConfig::for_session(&s, None);
        assert_eq!(llm.provider, "ollama");
        assert_eq!(llm.url, "http://127.0.0.1:11434");
        assert!(!llm.model.is_empty());
        assert!(llm.api_key.is_none());
        let overridden = LlmConfig::for_session(&s, Some("custom-model"));
        assert_eq!(overridden.model, "custom-model");
    }

    #[test]
    fn an_openai_compatible_provider_uses_its_own_endpoint_model_and_key() {
        let mut s = crate::Session::default();
        s.prefs.nl.llm_provider = "openai_compatible".into();
        s.prefs.nl.llm_url = "http://127.0.0.1:8080/v1/".into();
        s.prefs.nl.llm_model = "Qwen3-8B-Q4_K_M".into();
        s.prefs.nl.llm_api_key = "secret-token".into();
        let llm = LlmConfig::for_session(&s, None);
        assert_eq!(llm.provider, "openai_compatible");
        assert_eq!(llm.url, "http://127.0.0.1:8080/v1", "the trailing slash is removed");
        assert_eq!(llm.model, "Qwen3-8B-Q4_K_M");
        assert_eq!(llm.api_key.as_deref(), Some("secret-token"));
    }

    #[test]
    fn cloud_and_embedding_models_are_not_offered() {
        assert!(usable("qwen3.5:9b"));
        assert!(usable("lfm2.5:latest"));
        assert!(!usable("gemma3:27b-cloud"));
        assert!(!usable("nomic-embed-text:latest"));
    }
}
