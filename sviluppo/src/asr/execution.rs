//! Per-request ASR device choice; no downloads, model loading or shared unload.
use crate::{resources, ApiError, AppState};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Device {
    #[default]
    Auto,
    Cpu,
    Cuda,
}
impl Device {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cpu => "cpu",
            Self::Cuda => "cuda",
        }
    }
}
type ProbeCache = Option<(std::path::PathBuf, Instant, Value)>;
static CACHE: OnceLock<tokio::sync::Mutex<ProbeCache>> = OnceLock::new();
pub async fn probe(state: &AppState) -> anyhow::Result<Value> {
    let python = state.config().python_executable;
    let mut cache = CACHE
        .get_or_init(|| tokio::sync::Mutex::new(None))
        .lock()
        .await;
    if let Some((path, at, value)) = &*cache {
        if *path == python && at.elapsed() < Duration::from_secs(60) {
            return Ok(value.clone());
        }
    }
    let mut command = tokio::process::Command::new(&python);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(
        Duration::from_secs(40),
        command
            .arg("-c")
            .arg(include_str!("../../bridge/asr_hardware.py"))
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .kill_on_drop(true)
            .output(),
    )
    .await??;
    anyhow::ensure!(
        output.status.success(),
        "Rilevamento ASR: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout)?;
    *cache = Some((python, Instant::now(), value.clone()));
    Ok(value)
}
pub async fn hardware(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(probe(&state).await.map_err(|e| {
        ApiError::bad_request(format!(
            "Rilevamento GPU non riuscito: {e}. CPU resta selezionabile esplicitamente."
        ))
    })?))
}
pub async fn resolve(state: &AppState, requested: Device) -> Result<Device, ApiError> {
    if requested == Device::Cpu {
        return Ok(Device::Cpu);
    }
    let result = probe(state)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    if result["cuda"].as_bool() != Some(true) {
        if requested == Device::Cuda {
            return Err(ApiError::bad_request(format!("GPU ASR richiesta ma non operativa: {}. Nessun ripiego in CPU: scegli CPU o correggi il runtime.",result["reason"].as_str().unwrap_or("CUDA non disponibile"))));
        }
        return Ok(Device::Cpu);
    }
    resources::check_comfy_queue(&state.http, &state.config().comfy_url)
        .await
        .map_err(|e| ApiError::bad_request(format!("ASR GPU: {e}")))?;
    Ok(Device::Cuda)
}
pub fn metadata(requested: Device, actual: Device) -> Value {
    json!({"requested":requested.label(),"actual":actual.label(),"cpu_fallback":false})
}
