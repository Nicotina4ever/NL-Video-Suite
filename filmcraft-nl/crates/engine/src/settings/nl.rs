//! NL Video Suite preferences: local AI endpoints, installed models and workflows, the
//! resolution profiles derived from the open sequence, speech recognition and service paths.
//!
//! Every value is a Settings key under the `nl` category (`nl.comfyUrl`, `nl.imageScale`…), so the
//! Settings dialog, `prefs.get`/`prefs.set` and the NL panels all read and write the same store
//! (`preferences.json` in the per-user data directory). Nothing here downloads a model: the values
//! name what the operator already has.
//!
//! Precedence used by the engine, highest first: an explicit command parameter, these preferences,
//! the matching `NL_*` environment variable, then the built-in default.

use serde::{Deserialize, Serialize};

/// Endpoint used when neither the preferences nor the environment name one.
pub const DEFAULT_COMFY_URL: &str = "http://127.0.0.1:8188";
/// Endpoint used when neither the preferences nor the environment name one.
pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";
/// Model used when neither the preferences nor the environment name one.
pub const DEFAULT_OLLAMA_MODEL: &str = "lfm2.5:latest";
/// Language-model provider: the local Ollama, or any OpenAI-compatible server (llama.cpp,
/// LM Studio, vLLM, a gateway). The classic suite offered the same two choices.
pub const DEFAULT_LLM_PROVIDER: &str = "ollama";
/// Default for an OpenAI-compatible server: llama.cpp's server and LM Studio both use this.
pub const DEFAULT_LLM_URL: &str = "http://127.0.0.1:8080/v1";

/// NL Video Suite preferences (Settings ▸ NL Video Suite).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NlPrefs {
    /// ComfyUI base URL (`http://host:port`).
    pub comfy_url: String,
    /// Image workflow, relative to the suite root or absolute; empty = the built-in default.
    pub comfy_image_workflow: String,
    /// Video (image-to-video) workflow; empty = the built-in default.
    pub comfy_video_workflow: String,
    /// Music workflow; empty = the built-in Ace Step default.
    pub comfy_music_workflow: String,
    /// Folder the workflow pickers list; empty = `workflows/regista` under the suite root.
    pub comfy_workflow_dir: String,
    /// Ollama base URL.
    pub ollama_url: String,
    /// Ollama model for ranking, prompts and translation.
    pub ollama_model: String,
    /// Ollama model for image analysis; empty = the main model.
    pub ollama_model_vision: String,
    /// Language-model provider: `ollama` or `openai_compatible`.
    pub llm_provider: String,
    /// Base URL of the OpenAI-compatible server (with `/v1`); empty = the built-in default.
    pub llm_url: String,
    /// Model on the OpenAI-compatible server (or an override for Ollama); empty = the Ollama model.
    pub llm_model: String,
    /// Optional bearer token for the OpenAI-compatible server (empty: none; `NL_LLM_API_KEY` works too).
    pub llm_api_key: String,
    /// Speech-recognition model name (informational) or a model folder; empty = the portable one.
    pub asr_model: String,
    /// Speech backend: `bridge` (the suite runtime) or `openai_compatible` (a whisper-compatible HTTP server).
    pub asr_provider: String,
    /// Base URL of the OpenAI-compatible speech server (`/v1` included).
    pub asr_url: String,
    /// Optional bearer token for the speech server.
    pub asr_api_key: String,
    /// Extra folders scanned for speech models (semicolon-separated).
    pub asr_search_dirs: String,
    /// qwen3-asr.cpp CLI executable (qwen_gguf backend); empty = auto-detect.
    pub asr_runner: String,
    /// qwen3-asr GGUF model (qwen_gguf backend); empty = auto-detect.
    pub asr_qwen_model: String,
    /// qwen3 forced-aligner GGUF (qwen_gguf backend); empty = auto-detect.
    pub asr_qwen_aligner: String,
    /// `auto` (GPU first, one CPU retry), `cuda` or `cpu`.
    pub asr_device: String,
    /// Language passed to the recognizer.
    pub asr_language: String,
    /// Reuse a transcript cached next to the work folder for unchanged media.
    pub asr_cache: bool,
    /// Folder for recognition work and caches; empty = the per-user data directory.
    pub asr_cache_dir: String,
    /// Image resolution profile from the sequence: `1x` or `2x`.
    pub image_scale: String,
    /// Video resolution profile from the sequence: `2/3` (a third smaller) or `1/2`.
    pub video_scale: String,
    /// Rounding of generated sizes: `8` or `16` pixels.
    pub size_multiple: String,
    /// Try the GPU first and fall back to the CPU once, saying so in the result.
    pub gpu_first: bool,
    /// How long one ComfyUI generation may wait before failing (seconds).
    pub queue_timeout_sec: u32,
    /// How often the queue checks ComfyUI (seconds).
    pub queue_poll_sec: u32,
    /// ComfyUI installation folder used by the service panel; empty = not configured.
    pub service_comfy_dir: String,
    /// Ollama executable or app path used by the service panel; empty = not configured.
    pub service_ollama_path: String,
    /// OpenAI-compatible vision endpoint for timeline analysis (llama.cpp server); empty = 127.0.0.1:8080/v1.
    pub vision_url: String,
    /// Vision model name; empty = the first model the endpoint declares.
    pub vision_model: String,
    /// Music model override; empty = whatever the workflow names.
    pub music_model: String,
    /// Remove disposable files (bridge frames, old logs, caches) when the editor starts.
    pub auto_cleanup: bool,
    /// Disposable files older than this many days are removed.
    pub cleanup_days: u64,
    /// Also remove generated media (off by default: those are the operator's outputs).
    pub cleanup_generated: bool,
    /// Generated media older than this many days is removed when `cleanupGenerated` is on.
    pub generated_keep_days: u64,
}

impl Default for NlPrefs {
    fn default() -> Self {
        Self {
            comfy_url: DEFAULT_COMFY_URL.into(),
            comfy_image_workflow: String::new(),
            comfy_video_workflow: String::new(),
            comfy_music_workflow: String::new(),
            comfy_workflow_dir: String::new(),
            ollama_url: DEFAULT_OLLAMA_URL.into(),
            ollama_model: DEFAULT_OLLAMA_MODEL.into(),
            ollama_model_vision: String::new(),
            llm_provider: DEFAULT_LLM_PROVIDER.into(),
            llm_url: String::new(),
            llm_model: String::new(),
            llm_api_key: String::new(),
            asr_model: String::new(),
            asr_provider: "bridge".into(),
            asr_search_dirs: String::new(),
            asr_runner: String::new(),
            asr_qwen_model: String::new(),
            asr_qwen_aligner: String::new(),
            asr_url: String::new(),
            asr_api_key: String::new(),
            asr_device: "auto".into(),
            asr_language: "it".into(),
            asr_cache: true,
            asr_cache_dir: String::new(),
            image_scale: "1x".into(),
            video_scale: "2/3".into(),
            size_multiple: "8".into(),
            gpu_first: true,
            queue_timeout_sec: 1800,
            queue_poll_sec: 2,
            service_comfy_dir: String::new(),
            service_ollama_path: String::new(),
            vision_url: String::new(),
            vision_model: String::new(),
            music_model: String::new(),
            auto_cleanup: true,
            cleanup_days: crate::nl::maintenance::DEFAULT_TEMP_DAYS,
            cleanup_generated: false,
            generated_keep_days: crate::nl::maintenance::DEFAULT_GENERATED_DAYS,
        }
    }
}

/// A trimmed value, or `None` when it is blank.
fn some(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(|value| some(&value))
}

impl NlPrefs {
    /// ComfyUI endpoint: preference, then `NL_COMFY_URL`, then the built-in default.
    pub fn comfy_url(&self) -> String {
        some(&self.comfy_url).or_else(|| env("NL_COMFY_URL")).unwrap_or_else(|| DEFAULT_COMFY_URL.into())
    }

    /// The extra model folders, split on ';'.
    pub fn asr_search_dirs_list(&self) -> Vec<String> {
        self.asr_search_dirs.split([';', ':']).map(str::trim).filter(|part| !part.is_empty()).map(str::to_string).collect()
    }
    /// The configured vision endpoint (frame/OCR analysis through a local OpenAI-compatible server).
    pub fn vision_url(&self) -> String {
        some(&self.vision_url).or_else(|| env("NL_VISION_URL")).unwrap_or_else(|| "http://127.0.0.1:8080/v1".into()).trim_end_matches('/').to_string()
    }

    pub fn asr_provider(&self) -> String {
        some(&self.asr_provider).or_else(|| env("NL_ASR_PROVIDER")).unwrap_or_else(|| "bridge".into()).to_ascii_lowercase()
    }

    /// Base URL of the OpenAI-compatible speech server (default 127.0.0.1:8000/v1).
    pub fn asr_url(&self) -> String {
        some(&self.asr_url).or_else(|| env("NL_ASR_URL")).unwrap_or_else(|| "http://127.0.0.1:8000/v1".into()).trim_end_matches('/').to_string()
    }

    /// Optional bearer token for the speech server.
    pub fn asr_api_key(&self) -> Option<String> {
        some(&self.asr_api_key).or_else(|| env("NL_ASR_API_KEY"))
    }

    /// Ollama endpoint: preference, then `NL_OLLAMA_URL`, then the built-in default.
    pub fn ollama_url(&self) -> String {
        some(&self.ollama_url).or_else(|| env("NL_OLLAMA_URL")).unwrap_or_else(|| DEFAULT_OLLAMA_URL.into())
    }

    /// Language-model provider (`ollama` or `openai_compatible`): preference, then
    /// `NL_LLM_PROVIDER`, then Ollama.
    pub fn llm_provider(&self) -> String {
        some(&self.llm_provider).or_else(|| env("NL_LLM_PROVIDER")).unwrap_or_else(|| DEFAULT_LLM_PROVIDER.into()).to_ascii_lowercase()
    }

    /// Endpoint of the language-model provider: the Ollama URL for Ollama, the configured
    /// OpenAI-compatible base (default llama.cpp/LM Studio) otherwise.
    pub fn llm_url(&self) -> String {
        if self.llm_provider() == "ollama" {
            self.ollama_url()
        } else {
            some(&self.llm_url).or_else(|| env("NL_LLM_URL")).unwrap_or_else(|| DEFAULT_LLM_URL.into()).trim_end_matches('/').to_string()
        }
    }

    /// Model for prompts, stories, analysis and translation: the generic override, then
    /// `NL_LLM_MODEL`, then the Ollama model (which the classic suite also used as default).
    pub fn llm_model(&self) -> String {
        some(&self.llm_model).or_else(|| env("NL_LLM_MODEL")).unwrap_or_else(|| self.ollama_model())
    }

    /// Optional bearer token for the OpenAI-compatible server.
    pub fn llm_api_key(&self) -> Option<String> {
        some(&self.llm_api_key).or_else(|| env("NL_LLM_API_KEY"))
    }

    /// Ollama model for ranking, prompts and translation.
    pub fn ollama_model(&self) -> String {
        some(&self.ollama_model).or_else(|| env("NL_OLLAMA_MODEL")).unwrap_or_else(|| DEFAULT_OLLAMA_MODEL.into())
    }

    /// Ollama model for image analysis (falls back to the main model).
    pub fn ollama_model_vision(&self) -> String {
        some(&self.ollama_model_vision).unwrap_or_else(|| self.ollama_model())
    }

    /// Recognizer device: `auto`, `cuda` or `cpu`; `auto` with GPU-first off means `cpu`.
    pub fn asr_device(&self) -> String {
        match self.asr_device.as_str() {
            "cuda" => "cuda".into(),
            "cpu" => "cpu".into(),
            _ if self.gpu_first => "auto".into(),
            _ => "cpu".into(),
        }
    }

    /// Recognizer language.
    pub fn asr_language(&self) -> String {
        some(&self.asr_language).unwrap_or_else(|| "it".into())
    }

    /// Image multiplier from the sequence: 1.0 or 2.0.
    pub fn image_multiplier(&self) -> f64 {
        if self.image_scale == "2x" { 2.0 } else { 1.0 }
    }

    /// Video divisor from the sequence: 1.5 (2/3) or 2.0 (1/2).
    pub fn video_divisor(&self) -> f64 {
        if self.video_scale == "1/2" { 2.0 } else { 1.5 }
    }

    /// Rounding granularity of generated sizes (8 or 16 pixels).
    pub fn size_step(&self) -> u32 {
        if self.size_multiple == "16" { 16 } else { 8 }
    }

    /// How long one ComfyUI generation may wait.
    pub fn queue_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.queue_timeout_sec.clamp(30, 7200) as u64)
    }
}
