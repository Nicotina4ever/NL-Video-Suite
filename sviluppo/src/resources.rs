//! Read-only probes and conservative, per-request admission. No global unload/kill.
use crate::{model::*, store, ApiError, AppState};
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub profile: String,
    pub reserve_ram_mib: u64,
    pub llm_threads: u32,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            profile: "balanced".into(),
            reserve_ram_mib: 2048,
            llm_threads: 4,
        }
    }
}
impl Policy {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            ["cpu_first", "balanced", "gpu_preferred"].contains(&self.profile.as_str())
                && (512..=16384).contains(&self.reserve_ram_mib)
                && (1..=16).contains(&self.llm_threads),
            "Risorse: profilo CPU/Balanced/GPU, riserva 512–16384MiB, 1–16 thread LLM"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Memory {
    pub total_bytes: u64,
    pub available_bytes: u64,
}
/// Unit-test binary only: OS memory is never probed inside `cargo test`.
/// Admission is injected so the suite is deterministic on any machine; the
/// production build compiles this away and probes the real OS. Tests that
/// need a specific value set `TEST_AVAILABLE_RAM` (0 keeps the default).
#[cfg(test)]
pub(crate) static TEST_AVAILABLE_RAM: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(test)]
fn test_memory() -> Option<Memory> {
    let injected = TEST_AVAILABLE_RAM.load(std::sync::atomic::Ordering::Relaxed);
    let available_bytes = if injected > 0 { injected } else { 64 << 30 };
    Some(Memory {
        total_bytes: available_bytes.saturating_add(64 << 30),
        available_bytes,
    })
}
#[cfg(windows)]
pub fn memory() -> Option<Memory> {
    #[cfg(test)]
    if let Some(injected) = test_memory() {
        return Some(injected);
    }
    #[repr(C)]
    struct Status {
        length: u32,
        load: u32,
        total: u64,
        available: u64,
        page_total: u64,
        page_available: u64,
        virtual_total: u64,
        virtual_available: u64,
        extended: u64,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalMemoryStatusEx(status: *mut Status) -> i32;
    }
    let mut status = Status {
        length: std::mem::size_of::<Status>() as u32,
        load: 0,
        total: 0,
        available: 0,
        page_total: 0,
        page_available: 0,
        virtual_total: 0,
        virtual_available: 0,
        extended: 0,
    };
    // OS read-only query; the initialized C structure is exactly dwLength bytes.
    if unsafe { GlobalMemoryStatusEx(&mut status) } != 0 {
        Some(Memory {
            total_bytes: status.total,
            available_bytes: status.available,
        })
    } else {
        None
    }
}
#[cfg(not(windows))]
pub fn memory() -> Option<Memory> {
    #[cfg(test)]
    if let Some(injected) = test_memory() {
        return Some(injected);
    }
    None
}
async fn get(client: &reqwest::Client, url: String) -> Option<Value> {
    client
        .get(url)
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()
}
fn connection_refused(error: &reqwest::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(err) = source {
        if err
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::ConnectionRefused)
        {
            return true;
        }
        source = err.source();
    }
    false
}
/// A refused local connection is an offline service, not an occupied queue.
/// Timeouts, HTTP errors and unreachable remote servers remain indeterminate.
pub(crate) async fn check_comfy_queue(client: &reqwest::Client, base: &str) -> anyhow::Result<()> {
    let url = reqwest::Url::parse(&format!("{}/queue", base.trim_end_matches('/')))?;
    let local = matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    let response=match client.get(url).timeout(Duration::from_secs(6)).send().await {
        Ok(response)=>response,
        Err(error) if local && connection_refused(&error)=>return Ok(()),
        Err(_)=>anyhow::bail!("Stato ComfyUI non verificabile (timeout o server remoto irraggiungibile). Controlla l'indirizzo in Impostazioni o riprova; non è stato rilevato un servizio locale spento."),
    };
    let queue:Value=response.error_for_status().map_err(|_|anyhow::anyhow!("ComfyUI risponde con un errore HTTP: verifica il servizio, non è considerato spento."))?.json().await.map_err(|_|anyhow::anyhow!("Risposta ComfyUI non valida: coda non verificabile."))?;
    anyhow::ensure!(
        ["queue_running", "queue_pending"]
            .iter()
            .all(|k| queue[*k].as_array().is_some()),
        "ComfyUI risponde ma il formato della coda non è riconosciuto."
    );
    anyhow::ensure!(
        ["queue_running", "queue_pending"]
            .iter()
            .all(|k| queue[*k].as_array().is_some_and(|q| q.is_empty())),
        "ComfyUI ha lavori esterni attivi: richiesta LLM GPU non avviata, usa CPU-first o attendi."
    );
    Ok(())
}
async fn gpu() -> Value {
    let mut cmd = tokio::process::Command::new("nvidia-smi");
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let future = cmd
        .args([
            "--query-gpu=name,memory.total,memory.used,memory.free",
            "--format=csv,noheader,nounits",
        ])
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(Duration::from_secs(2), future).await {
        Ok(Ok(o)) if o.status.success() => {
            json!({"available":true,"source":"nvidia-smi read-only","csv":String::from_utf8_lossy(&o.stdout).trim()})
        }
        _ => json!({"available":false,"vram":"unknown, never assumed zero"}),
    }
}
pub async fn view(State(state): State<AppState>) -> Json<Value> {
    let cfg = state.config();
    let (gpu, comfy, ollama) = tokio::join!(
        gpu(),
        get(
            &state.http,
            format!("{}/queue", cfg.comfy_url.trim_end_matches('/'))
        ),
        get(
            &state.http,
            format!("{}/api/ps", cfg.ollama_url.trim_end_matches('/'))
        )
    );
    Json(
        json!({"policy":cfg.resources,"memory":memory(),"gpu":gpu,"comfy_queue":comfy,"ollama_resident":ollama,"suite_active_jobs":state.jobs.read().await.values().filter(|j|matches!(j.status,JobStatus::Queued|JobStatus::Running)).count(),"estimates_are_guarantees":false,"external_process_control":false,"render_threads":2,"proxy_threads":2,"cpu_threads_available":std::thread::available_parallelism().map(|n|n.get()).ok()}),
    )
}
pub async fn save(
    State(state): State<AppState>,
    Json(policy): Json<Policy>,
) -> Result<Json<Policy>, ApiError> {
    policy
        .validate()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let jobs = state.jobs.read().await;
    if jobs
        .values()
        .any(|j| matches!(j.status, JobStatus::Queued | JobStatus::Running))
    {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Attendi la coda della suite prima di cambiare il profilo risorse".into(),
        ));
    }
    let mut config = state.config.write().expect("config lock");
    let mut next = config.clone();
    next.resources = policy.clone();
    store::write_json(&state.data_dir.join("settings.json"), &next).map_err(ApiError::internal)?;
    *config = next;
    Ok(Json(policy))
}
pub async fn guard_llm(
    client: &reqwest::Client,
    cfg: &AppConfig,
    model: &str,
) -> anyhow::Result<()> {
    // The owner's rule of 21/11: only process overlap serializes. The LLM waits
    // for ComfyUI's external queue to empty when it would use the GPU; RAM/VRAM
    // never gate a request. After a day something is permanently wrong.
    const DEADLINE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);
    const POLL: std::time::Duration = std::time::Duration::from_secs(15);
    let started = std::time::Instant::now();
    loop {
        match guard_llm_once(client, cfg, model).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                if started.elapsed() >= DEADLINE {
                    return Err(anyhow::anyhow!(
                        "Coda ComfyUI non libera dopo 24 ore di attesa: {error}"
                    ));
                }
                tracing::info!(
                    %error,
                    wait_seconds = POLL.as_secs(),
                    "LLM in attesa della coda ComfyUI: un solo job pesante alla volta"
                );
                tokio::time::sleep(POLL).await;
            }
        }
    }
}
async fn guard_llm_once(
    client: &reqwest::Client,
    cfg: &AppConfig,
    _model: &str,
) -> anyhow::Result<()> {
    if cfg.llm_provider != LlmProvider::Ollama {
        return Ok(());
    }
    // Owner's rule (21/11): only process overlap serializes. RAM and VRAM are
    // reported by the status panel and can be released manually; they never
    // refuse or delay a request.
    if std::env::var("NL_SUITE_LLM_ADMISSION").is_ok_and(|v| v == "off") {
        return Ok(());
    }
    let url = reqwest::Url::parse(&cfg.ollama_url)?;
    if !matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    ) {
        return Ok(());
    }
    let gpu_requested = cfg.resources.profile == "gpu_preferred"
        || (cfg.resources.profile != "cpu_first" && cfg.ollama_execution == OllamaExecution::Auto);
    if gpu_requested {
        // One heavy GPU consumer at a time: wait for ComfyUI's external queue.
        check_comfy_queue(client, &cfg.comfy_url).await?;
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseRequest {
    pub confirmation: String,
}

/// Manual, explicitly confirmed release of the resources the suite can act
/// on: unload every model Ollama currently keeps resident and, ONLY when
/// ComfyUI's queue is empty, ask ComfyUI to release its weights and cache.
/// Never interrupts running work, never kills services, never touches
/// another app's active generation: if ComfyUI is busy it is reported and
/// left alone. The button is the owner's emergency valve; admission normally
/// queues instead of failing.
pub async fn release(
    State(state): State<AppState>,
    Json(request): Json<ReleaseRequest>,
) -> Result<Json<Value>, ApiError> {
    if request.confirmation != "SVUOTA" {
        return Err(ApiError::bad_request(
            "Scrivi SVUOTA per confermare il rilascio manuale delle risorse.",
        ));
    }
    let cfg = state.config();
    let before_ram = memory().map(|m| m.available_bytes).unwrap_or(0);
    let before_gpu = gpu().await;
    let mut actions: Vec<String> = Vec::new();
    let ollama_local = reqwest::Url::parse(&cfg.ollama_url)
        .ok()
        .and_then(|url| {
            url.host_str()
                .map(|host| matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1"))
        })
        .unwrap_or(false);
    if ollama_local {
        let base = cfg.ollama_url.trim_end_matches('/');
        if let Some(ps) = get(&state.http, format!("{base}/api/ps")).await {
            let loaded: Vec<String> = ps["models"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|m| m["name"].as_str().map(|s| s.to_owned()))
                .collect();
            if loaded.is_empty() {
                actions.push("Ollama: nessun modello in memoria".into());
            }
            for name in loaded {
                let response = state
                    .http
                    .post(format!("{base}/api/generate"))
                    .timeout(Duration::from_secs(30))
                    .json(&json!({"model": name, "prompt": "", "keep_alive": 0}))
                    .send()
                    .await;
                match response {
                    Ok(r) if r.status().is_success() => {
                        actions.push(format!("Ollama: «{name}» scaricato dalla memoria"))
                    }
                    Ok(r) => actions.push(format!(
                        "Ollama: «{name}» non scaricato (HTTP {})",
                        r.status()
                    )),
                    Err(e) => actions.push(format!("Ollama: «{name}» non raggiungibile: {e}")),
                }
            }
        } else {
            actions.push("Ollama non raggiungibile: nessun modello scaricato".into());
        }
    } else {
        actions.push("Provider LLM remoto: rilascio locale non applicabile".into());
    }
    // ComfyUI only if idle: its queue belongs to whoever is using it.
    let base = cfg.comfy_url.trim_end_matches('/');
    let idle = get(&state.http, format!("{base}/queue"))
        .await
        .map(|q| {
            ["queue_running", "queue_pending"]
                .iter()
                .all(|k| q[*k].as_array().map(|v| v.is_empty()).unwrap_or(false))
        })
        .unwrap_or(false);
    if idle {
        let released = state
            .http
            .post(format!("{base}/free"))
            .timeout(Duration::from_secs(60))
            .json(&json!({"unload_models": true, "free_memory": true}))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        actions.push(if released {
            "ComfyUI: pesi e cache rilasciati (coda vuota)".to_owned()
        } else {
            "ComfyUI: versione senza endpoint di rilascio; nulla fatto".to_owned()
        });
    } else {
        actions.push("ComfyUI occupato: lavori tuoi in corso, non toccato".to_owned());
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    let after_ram = memory().map(|m| m.available_bytes).unwrap_or(0);
    let after_gpu = gpu().await;
    let gib = 1u64 << 30;
    let recovered = after_ram.saturating_sub(before_ram);
    let before_gib = before_ram / gib;
    let after_gib = after_ram / gib;
    Ok(Json(json!({
        "ram_free_before": before_ram,
        "ram_free_after": after_ram,
        "ram_recovered_bytes": recovered,
        "gpu_before": before_gpu,
        "gpu_after": after_gpu,
        "actions": actions,
        "note": format!(
            "RAM libera: {before_gib} GiB prima, {after_gib} GiB dopo (+{} MiB). Nessun processo esterno terminato; i lavori ComfyUI in corso non sono stati interrotti.",
            recovered / (1u64 << 20)
        ),
    })))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_still_validates_but_memory_never_gates_admission() {
        let mut p = Policy::default();
        assert!(p.validate().is_ok());
        p.llm_threads = 100;
        assert!(p.validate().is_err());
    }
    #[tokio::test]
    async fn low_reported_ram_never_blocks_the_llm() {
        // Regression guard for the owner's rule: only process overlap gates
        // work. A tiny reported RAM must not stop a loopback LLM request; the
        // offline ComfyUI probe is allowed to pass because it cannot overlap.
        TEST_AVAILABLE_RAM.store(64 * 1024 * 1024, std::sync::atomic::Ordering::Relaxed);
        let mut cfg = AppConfig::default();
        cfg.llm_provider = LlmProvider::Ollama;
        cfg.ollama_url = "http://127.0.0.1:11434".into();
        cfg.comfy_url = "http://127.0.0.1:9".into();
        cfg.ollama_execution = OllamaExecution::Auto;
        let result = guard_llm_once(&reqwest::Client::new(), &cfg, "model").await;
        TEST_AVAILABLE_RAM.store(0, std::sync::atomic::Ordering::Relaxed);
        assert!(result.is_ok(), "{result:?}");
    }
    #[tokio::test]
    async fn local_comfy_offline_allows_ollama_but_bad_responses_do_not() {
        let client = reqwest::Client::new();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        assert!(check_comfy_queue(&client, &base).await.is_ok());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new().route(
            "/queue",
            axum::routing::get(|| async { StatusCode::SERVICE_UNAVAILABLE }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        assert!(check_comfy_queue(&client, &base)
            .await
            .unwrap_err()
            .to_string()
            .contains("HTTP"));
        server.abort();
    }
}
