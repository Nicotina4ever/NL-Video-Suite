//! Explicit, local service launchers. Never invoke the original projects' scripts.
use crate::{model::LlmProvider, ApiError, AppState};
use axum::{extract::State, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::Duration,
};
use uuid::Uuid;

static CONFIG_LOCK: Mutex<()> = Mutex::new(());
static START_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServiceConfig {
    pub comfy_dir: String,
    pub ollama_executable: String,
    pub ollama_models_dir: String,
    pub llamacpp_executable: String,
    pub llamacpp_model: String,
    pub comfy_sage_attention: bool,
    pub comfy_reserved_vram_gb: f32,
    pub llamacpp_gpu_layers: u32,
    pub llamacpp_context: u32,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            comfy_dir: String::new(),
            ollama_executable: String::new(),
            ollama_models_dir: String::new(),
            llamacpp_executable: String::new(),
            llamacpp_model: String::new(),
            comfy_sage_attention: false,
            comfy_reserved_vram_gb: 1.0,
            llamacpp_gpu_layers: 0,
            llamacpp_context: 8192,
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigPatch {
    pub comfy_dir: Option<String>,
    pub ollama_executable: Option<String>,
    pub ollama_models_dir: Option<String>,
    pub llamacpp_executable: Option<String>,
    pub llamacpp_model: Option<String>,
    pub comfy_sage_attention: Option<bool>,
    pub comfy_reserved_vram_gb: Option<f32>,
    pub llamacpp_gpu_layers: Option<u32>,
    pub llamacpp_context: Option<u32>,
}

#[derive(Default, Serialize)]
pub struct Discovery {
    pub comfy_dirs: Vec<String>,
    pub ollama_executables: Vec<String>,
    pub llamacpp_executables: Vec<String>,
    pub llamacpp_models: Vec<String>,
    pub ollama_model_dirs: Vec<String>,
}

#[derive(Serialize)]
pub struct ConfigResponse {
    pub config: ServiceConfig,
    pub discovered: Discovery,
    pub warnings: Vec<String>,
}

pub async fn get_config(State(state): State<AppState>) -> Result<Json<ConfigResponse>, ApiError> {
    let config = load_config(&state.data_dir).map_err(friendly)?;
    Ok(Json(config_response(config)))
}

pub async fn set_config(
    State(state): State<AppState>,
    Json(patch): Json<ConfigPatch>,
) -> Result<Json<ConfigResponse>, ApiError> {
    let _guard = CONFIG_LOCK
        .lock()
        .map_err(|_| ApiError::bad_request("Configurazione servizi temporaneamente occupata"))?;
    let config =
        apply_patch(load_config(&state.data_dir).map_err(friendly)?, patch).map_err(friendly)?;
    save_config(&state.data_dir, &config).map_err(friendly)?;
    Ok(Json(config_response(config)))
}

fn friendly(error: anyhow::Error) -> ApiError {
    ApiError::bad_request(error.to_string())
}

fn config_response(config: ServiceConfig) -> ConfigResponse {
    ConfigResponse {
        config,
        discovered: discover(),
        warnings: vec![
            "I percorsi trovati sono suggerimenti in sola lettura: vanno scelti e salvati esplicitamente. Nessun avvio automatico.".into(),
            "ComfyUI e i provider LLM restano installazioni separate. La suite non scarica modelli e non arresta processi esterni.".into(),
            "llama.cpp parte in CPU per impostazione predefinita. Aumentare i layer GPU può contendere la VRAM a ComfyUI.".into(),
            "I launcher e i log della suite sono in data/services; i servizi avviati possono usare le proprie normali cartelle di lavoro/cache.".into(),
        ],
    }
}

fn no_reparse(path: &Path) -> anyhow::Result<()> {
    for parent in path.ancestors() {
        if parent.as_os_str().is_empty() {
            continue;
        }
        let metadata = fs::symlink_metadata(parent)?;
        #[cfg(windows)]
        let reparse = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let reparse = false;
        if metadata.file_type().is_symlink() || reparse {
            anyhow::bail!(
                "Link/junction non consentito per i record della suite: {}",
                parent.display()
            );
        }
    }
    Ok(())
}

fn config_path(data: &Path) -> anyhow::Result<PathBuf> {
    no_reparse(data)?;
    let path = data.join("services.json");
    if path.exists() {
        no_reparse(&path)?;
    }
    Ok(path)
}

fn load_config(data: &Path) -> anyhow::Result<ServiceConfig> {
    let path = config_path(data)?;
    if !path.exists() {
        return Ok(ServiceConfig::default());
    }
    if fs::metadata(&path)?.len() > 64 * 1024 {
        anyhow::bail!("Configurazione servizi troppo grande");
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn save_config(data: &Path, config: &ServiceConfig) -> anyhow::Result<()> {
    let path = config_path(data)?;
    let temporary = data.join(format!("services-{}.json.tmp", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec_pretty(config)?)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, path)?;
    Ok(())
}

fn clean_path(value: String) -> anyhow::Result<String> {
    let value = value.trim().to_string();
    if value.contains(['\0', '\r', '\n']) || value.starts_with('"') || value.ends_with('"') {
        anyhow::bail!("Inserisci il percorso senza virgolette o caratteri di controllo");
    }
    Ok(value)
}

fn resolved(value: &str) -> anyhow::Result<PathBuf> {
    if value.trim().is_empty() {
        anyhow::bail!("Seleziona prima il percorso nelle impostazioni servizi");
    }
    let path = PathBuf::from(value);
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    // A chosen external install may legitimately live behind a user-owned junction.
    // Resolve once and only execute the concrete standard executable, never a script.
    Ok(crate::runtime::native_path(&path.canonicalize().map_err(
        |e| anyhow::anyhow!("Percorso non disponibile: {} ({e})", path.display()),
    )?))
}

fn executable(value: &str, expected: &str) -> anyhow::Result<PathBuf> {
    let path = resolved(value)?;
    let name = path.file_name().and_then(|p| p.to_str()).unwrap_or("");
    if !path.is_file() || !name.eq_ignore_ascii_case(expected) {
        anyhow::bail!("Seleziona l'eseguibile standard {expected}, non un batch o uno script");
    }
    Ok(path)
}

fn comfy_paths(value: &str) -> anyhow::Result<(PathBuf, PathBuf, PathBuf)> {
    let root = resolved(value)?;
    let python = root.join("python_embeded/python.exe");
    let main = root.join("ComfyUI/main.py");
    if !root.is_dir() || !python.is_file() || !main.is_file() {
        anyhow::bail!("Seleziona la root ComfyUI portable che contiene python_embeded/python.exe e ComfyUI/main.py");
    }
    let python = executable(&python.to_string_lossy(), "python.exe")?;
    Ok((
        root,
        python,
        crate::runtime::native_path(&main.canonicalize()?),
    ))
}

fn model_path(value: &str) -> anyhow::Result<PathBuf> {
    let path = resolved(value)?;
    if !path.is_file()
        || !path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("gguf"))
    {
        anyhow::bail!("Seleziona un file GGUF esistente per llama.cpp");
    }
    Ok(path)
}

fn validate(config: &ServiceConfig) -> anyhow::Result<()> {
    if !config.comfy_reserved_vram_gb.is_finite()
        || !(0.0..=32.0).contains(&config.comfy_reserved_vram_gb)
    {
        anyhow::bail!("Riserva VRAM ComfyUI: da 0 a 32 GB");
    }
    if !config.comfy_dir.is_empty() {
        comfy_paths(&config.comfy_dir)?;
    }
    if !config.ollama_executable.is_empty() {
        executable(&config.ollama_executable, "ollama.exe")?;
    }
    if !config.ollama_models_dir.is_empty() && !resolved(&config.ollama_models_dir)?.is_dir() {
        anyhow::bail!("La cartella dei modelli Ollama deve essere una directory esistente");
    }
    if !config.llamacpp_executable.is_empty() {
        executable(&config.llamacpp_executable, "llama-server.exe")?;
    }
    if !config.llamacpp_model.is_empty() {
        model_path(&config.llamacpp_model)?;
    }
    if !(512..=32_768).contains(&config.llamacpp_context) {
        anyhow::bail!("Contesto llama.cpp: da 512 a 32768 token");
    }
    if config.llamacpp_gpu_layers > 256 {
        anyhow::bail!("Layer GPU llama.cpp: da 0 a 256");
    }
    Ok(())
}

fn apply_patch(mut config: ServiceConfig, patch: ConfigPatch) -> anyhow::Result<ServiceConfig> {
    if let Some(value) = patch.comfy_dir {
        config.comfy_dir = clean_path(value)?;
    }
    if let Some(value) = patch.ollama_executable {
        config.ollama_executable = clean_path(value)?;
    }
    if let Some(value) = patch.ollama_models_dir {
        config.ollama_models_dir = clean_path(value)?;
    }
    if let Some(value) = patch.llamacpp_executable {
        config.llamacpp_executable = clean_path(value)?;
    }
    if let Some(value) = patch.llamacpp_model {
        config.llamacpp_model = clean_path(value)?;
    }
    if let Some(value) = patch.comfy_sage_attention {
        config.comfy_sage_attention = value;
    }
    if let Some(value) = patch.comfy_reserved_vram_gb {
        config.comfy_reserved_vram_gb = value;
    }
    if let Some(value) = patch.llamacpp_gpu_layers {
        config.llamacpp_gpu_layers = value;
    }
    if let Some(value) = patch.llamacpp_context {
        config.llamacpp_context = value;
    }
    validate(&config)?;
    Ok(config)
}

fn add_existing(list: &mut Vec<String>, path: PathBuf, directory: bool) {
    if (directory && path.is_dir()) || (!directory && path.is_file()) {
        if let Ok(path) = path.canonicalize() {
            let value = crate::runtime::native_path(&path)
                .to_string_lossy()
                .to_string();
            if !list.contains(&value) {
                list.push(value);
            }
        }
    }
}

fn discover() -> Discovery {
    let mut found = Discovery::default();
    let root = std::env::current_dir().unwrap_or_default();
    let adjacent = root.parent().unwrap_or(&root);
    for path in [
        root.join("services/ComfyUI"),
        root.join("external/ComfyUI"),
        adjacent.join("ComfyUI"),
        PathBuf::from(r"K:\AI\ComfyUI"),
    ] {
        if path.join("python_embeded/python.exe").is_file()
            && path.join("ComfyUI/main.py").is_file()
        {
            add_existing(&mut found.comfy_dirs, path, true);
        }
    }
    for path in [
        root.join("services/Ollama/ollama.exe"),
        adjacent.join("Ollama/ollama.exe"),
        PathBuf::from(r"K:\AI\Ollama\ollama.exe"),
    ] {
        add_existing(&mut found.ollama_executables, path, false);
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        add_existing(
            &mut found.ollama_executables,
            PathBuf::from(local).join("Programs/Ollama/ollama.exe"),
            false,
        );
    }
    if let Some(path) = std::env::var_os("PATH") {
        for folder in std::env::split_paths(&path).take(128) {
            add_existing(
                &mut found.ollama_executables,
                folder.join("ollama.exe"),
                false,
            );
        }
    }
    for path in [
        root.join("services/LlamaCPP/llama-server.exe"),
        adjacent.join("LlamaCPP/llama-server.exe"),
        PathBuf::from(r"K:\AI\LlamaCPP\llama-server.exe"),
    ] {
        add_existing(&mut found.llamacpp_executables, path, false);
    }
    add_existing(
        &mut found.llamacpp_models,
        PathBuf::from(r"K:\AI\LlamaCPP\Modello\Qwen3.8-27B-GGUF\Qwen3.8-27B-UD-IQ3_S.gguf"),
        false,
    );
    if let Some(path) = std::env::var_os("OLLAMA_MODELS") {
        add_existing(&mut found.ollama_model_dirs, PathBuf::from(path), true);
    }
    found
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartRequest {
    pub service: String,
}

#[derive(Serialize)]
pub struct StartResponse {
    pub status: &'static str,
    pub service: String,
    pub provider: Option<&'static str>,
    pub pid: Option<u32>,
    pub endpoint: String,
    pub log_paths: Vec<PathBuf>,
    pub message: String,
}

struct LocalEndpoint {
    port: u16,
    base: String,
}

fn local_endpoint(value: &str) -> anyhow::Result<LocalEndpoint> {
    let url = reqwest::Url::parse(value)?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        anyhow::bail!("Avvio consentito solo per endpoint HTTP locali 127.0.0.1/localhost, senza credenziali o query. I server remoti/HTTPS si avviano esternamente.");
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| anyhow::anyhow!("Porta servizio mancante"))?;
    if port == 0 {
        anyhow::bail!("Porta servizio non valida");
    }
    Ok(LocalEndpoint {
        port,
        base: format!("http://127.0.0.1:{port}"),
    })
}

#[derive(Debug)]
struct LaunchPlan {
    executable: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
    env: Vec<(String, String)>,
}

fn launch_plan(
    service: &str,
    provider: LlmProvider,
    config: &ServiceConfig,
    port: u16,
) -> anyhow::Result<LaunchPlan> {
    // Validate only the selected launch target, so an unavailable optional install
    // does not prevent starting a different service on a relocated PC.
    if service == "comfyui" {
        let (cwd, executable, main) = comfy_paths(&config.comfy_dir)?;
        let mut args = vec![
            "-s".into(),
            main.to_string_lossy().to_string(),
            "--windows-standalone-build".into(),
            "--listen".into(),
            "127.0.0.1".into(),
            "--port".into(),
            port.to_string(),
            "--disable-auto-launch".into(),
            "--reserve-vram".into(),
            config.comfy_reserved_vram_gb.to_string(),
        ];
        if config.comfy_sage_attention {
            args.push("--use-sage-attention".into());
        }
        return Ok(LaunchPlan {
            executable,
            args,
            cwd,
            env: vec![
                ("PYTHONIOENCODING".into(), "utf-8".into()),
                ("PYTHONDONTWRITEBYTECODE".into(), "1".into()),
            ],
        });
    }
    if service != "llm" {
        anyhow::bail!("Servizio supportato: comfyui oppure llm");
    }
    match provider {
        LlmProvider::Ollama => {
            let executable = executable(&config.ollama_executable, "ollama.exe")?;
            let mut env = vec![
                ("OLLAMA_HOST".into(), format!("127.0.0.1:{port}")),
                ("OLLAMA_NOPRUNE".into(), "1".into()),
                ("OLLAMA_NO_CLOUD".into(), "1".into()),
                ("OLLAMA_NUM_PARALLEL".into(), "1".into()),
                ("OLLAMA_MAX_LOADED_MODELS".into(), "1".into()),
            ];
            if !config.ollama_models_dir.is_empty() {
                let models = resolved(&config.ollama_models_dir)?;
                if !models.is_dir() {
                    anyhow::bail!("Cartella modelli Ollama non disponibile");
                }
                env.push((
                    "OLLAMA_MODELS".into(),
                    models.to_string_lossy().into_owned(),
                ));
            }
            let cwd = executable
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Cartella Ollama non valida"))?
                .to_path_buf();
            Ok(LaunchPlan {
                executable,
                args: vec!["serve".into()],
                cwd,
                env,
            })
        }
        LlmProvider::OpenaiCompatible => {
            if !(512..=32_768).contains(&config.llamacpp_context)
                || config.llamacpp_gpu_layers > 256
            {
                anyhow::bail!("Contesto/layer GPU llama.cpp non validi");
            }
            let executable = executable(&config.llamacpp_executable, "llama-server.exe")?;
            let model = model_path(&config.llamacpp_model)?;
            let cwd = executable
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Cartella llama.cpp non valida"))?
                .to_path_buf();
            let args = vec![
                "--model".into(),
                model.to_string_lossy().to_string(),
                "--host".into(),
                "127.0.0.1".into(),
                "--port".into(),
                port.to_string(),
                "--ctx-size".into(),
                config.llamacpp_context.to_string(),
                "--n-gpu-layers".into(),
                config.llamacpp_gpu_layers.to_string(),
                "--parallel".into(),
                "1".into(),
            ];
            Ok(LaunchPlan {
                executable,
                args,
                cwd,
                env: vec![],
            })
        }
    }
}

pub async fn start(
    State(state): State<AppState>,
    Json(request): Json<StartRequest>,
) -> Result<Json<StartResponse>, ApiError> {
    if !matches!(request.service.as_str(), "comfyui" | "llm") {
        return Err(ApiError::bad_request(
            "Servizio supportato: comfyui oppure llm",
        ));
    }
    let _lock = START_LOCK.try_lock().map_err(|_| {
        ApiError::bad_request("Un avvio servizi è già in corso: attendi e aggiorna lo stato")
    })?;
    let app = state.config();
    let (value, probe_path, provider) = if request.service == "comfyui" {
        (&app.comfy_url, "/system_stats", None)
    } else {
        match app.llm_provider {
            LlmProvider::Ollama => (&app.ollama_url, "/api/tags", Some("ollama")),
            LlmProvider::OpenaiCompatible => (
                &app.openai_base_url,
                "/v1/models",
                Some("openai_compatible"),
            ),
        }
    };
    let endpoint = local_endpoint(value).map_err(friendly)?;
    let online = state
        .http
        .get(format!("{}{probe_path}", endpoint.base))
        .timeout(Duration::from_secs(2))
        .send()
        .await;
    if let Ok(response) = online {
        if response.status().is_success() {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                let key = if request.service == "comfyui" {
                    "devices"
                } else if matches!(app.llm_provider, LlmProvider::Ollama) {
                    "models"
                } else {
                    "data"
                };
                if body.get(key).is_some_and(serde_json::Value::is_array) {
                    return Ok(Json(StartResponse {
                        status: "already_running",
                        service: request.service,
                        provider,
                        pid: None,
                        endpoint: value.clone(),
                        log_paths: vec![],
                        message: "Servizio già raggiungibile: nessun processo avviato o arrestato."
                            .into(),
                    }));
                }
            }
        }
    }
    if tokio::time::timeout(
        Duration::from_millis(500),
        tokio::net::TcpStream::connect(("127.0.0.1", endpoint.port)),
    )
    .await
    .is_ok_and(|r| r.is_ok())
    {
        return Err(ApiError::bad_request("La porta è già occupata ma il servizio non è pronto/riconoscibile. Nessun processo avviato o arrestato: controlla lo stato e i log del servizio."));
    }
    if request.service == "llm" {
        let launcher = crate::settings::active_provider_launcher(&state.data_dir, &app)?;
        if launcher == crate::settings::ProviderLauncher::External {
            return Err(ApiError::bad_request("Questo profilo usa un server gestito esternamente: avvialo dal suo programma e aggiorna i modelli. La suite non avvia un provider diverso al suo posto. Per Ollama o llama.cpp scegli esplicitamente il relativo launcher nel profilo."));
        }
    }
    let config = load_config(&state.data_dir).map_err(friendly)?;
    let plan = launch_plan(&request.service, app.llm_provider, &config, endpoint.port)
        .map_err(friendly)?;
    let data = state.data_dir.as_ref().clone();
    let service = request.service.clone();
    let (pid, log_paths) = tokio::task::spawn_blocking(move || launch(&data, &service, plan))
        .await
        .map_err(|e| ApiError::internal(e.into()))?
        .map_err(friendly)?;
    Ok(Json(StartResponse {status:"starting",service:request.service,provider,pid:Some(pid),endpoint:value.clone(),log_paths,message:"Avvio richiesto. Il caricamento può richiedere tempo: aggiorna lo stato. Nessun modello scaricato e nessun servizio esterno arrestato.".into()}))
}

#[derive(Serialize)]
struct LaunchRecord {
    service: String,
    pid: u32,
    started_at: chrono::DateTime<Utc>,
    executable: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
    stdout: PathBuf,
    stderr: PathBuf,
}

fn launch(data: &Path, service: &str, plan: LaunchPlan) -> anyhow::Result<(u32, Vec<PathBuf>)> {
    no_reparse(data)?;
    let directory = data.join("services");
    if !directory.exists() {
        fs::create_dir(&directory)?;
    }
    no_reparse(&directory)?;
    let id = Uuid::new_v4();
    let stdout = directory.join(format!("{service}-{id}.stdout.log"));
    let stderr = directory.join(format!("{service}-{id}.stderr.log"));
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stdout)?;
    let error = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stderr)?;
    // Create the record first to detect permission errors before spawning.
    let record_path = directory.join(format!("{service}-{id}.json"));
    let mut record_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(record_path)?;
    let mut command = Command::new(&plan.executable);
    command
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .envs(plan.env)
        .stdin(Stdio::null())
        .stdout(output)
        .stderr(error);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    } // CREATE_NO_WINDOW
    let mut child = command.spawn()?;
    let pid = child.id();
    let record = LaunchRecord {
        service: service.into(),
        pid,
        started_at: Utc::now(),
        executable: plan.executable,
        args: plan.args,
        cwd: plan.cwd,
        stdout: stdout.strip_prefix(data)?.to_path_buf(),
        stderr: stderr.strip_prefix(data)?.to_path_buf(),
    };
    let saved = (|| -> anyhow::Result<()> {
        serde_json::to_writer_pretty(&mut record_file, &record)?;
        record_file.flush()?;
        record_file.sync_all()?;
        Ok(())
    })();
    if let Err(error) = saved {
        // Only this freshly spawned child, never a pre-existing/external service.
        let _ = child.kill();
        let _ = child.wait();
        anyhow::bail!("Impossibile registrare l'avvio (PID {pid}); arresto del solo processo appena creato richiesto: {error}");
    }
    // Reap when it eventually exits. Dropping a Child handle does not kill it.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok((pid, vec![record.stdout, record.stderr]))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("nl-services-test-{}", Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn file(&self, name: &str) -> String {
            let path = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"MZ test fixture, never executed").unwrap();
            path.to_string_lossy().to_string()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fn clear(path: &Path) {
                let Ok(meta) = fs::symlink_metadata(path) else {
                    return;
                };
                if meta.is_dir() && !meta.file_type().is_symlink() {
                    if let Ok(entries) = fs::read_dir(path) {
                        for e in entries.flatten() {
                            clear(&e.path());
                        }
                    }
                    let _ = fs::remove_dir(path);
                } else {
                    let _ = fs::remove_file(path);
                }
            }
            clear(&self.0);
        }
    }
    #[test]
    fn endpoints_only_allow_explicit_local_http() {
        assert_eq!(local_endpoint("http://localhost:8188").unwrap().port, 8188);
        assert_eq!(
            local_endpoint("http://127.0.0.1:8080/v1").unwrap().port,
            8080
        );
        for url in [
            "https://127.0.0.1:8188",
            "http://example.com:8188",
            "http://0.0.0.0:8188",
            "http://127.0.0.1:0",
            "http://user:secret@localhost:8080",
            "http://localhost:8080/?start=1",
        ] {
            assert!(local_endpoint(url).is_err(), "{url}");
        }
    }
    #[test]
    fn patch_preserves_missing_fields_and_never_executes_scripts() {
        let f = Fixture::new();
        let exe = f.file("ollama.exe");
        let original = ServiceConfig {
            ollama_executable: exe.clone(),
            ..Default::default()
        };
        let next = apply_patch(
            original,
            ConfigPatch {
                comfy_sage_attention: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(next.ollama_executable, exe);
        assert!(next.comfy_sage_attention);
        let script = f.file("start.bat");
        assert!(executable(&script, "ollama.exe").is_err());
        assert!(apply_patch(
            next,
            ConfigPatch {
                llamacpp_context: Some(131072),
                ..Default::default()
            }
        )
        .is_err());
    }
    #[test]
    fn llama_plan_defaults_to_cpu_and_never_downloads() {
        let f = Fixture::new();
        let config = ServiceConfig {
            llamacpp_executable: f.file("llama-server.exe"),
            llamacpp_model: f.file("my model.gguf"),
            ..Default::default()
        };
        let plan = launch_plan("llm", LlmProvider::OpenaiCompatible, &config, 8080).unwrap();
        assert!(plan.args.windows(2).any(|a| a == ["--n-gpu-layers", "0"]));
        assert!(plan.args.windows(2).any(|a| a == ["--ctx-size", "8192"]));
        assert!(plan.args.windows(2).any(|a| a == ["--host", "127.0.0.1"]));
        assert!(!plan
            .args
            .iter()
            .any(|a| a.contains("hf-repo") || a.contains("http")));
    }
    #[test]
    fn ollama_uses_only_explicit_model_directory_without_global_changes() {
        let f = Fixture::new();
        f.file("models/manifests/test-model");
        let config = ServiceConfig {
            ollama_executable: f.file("ollama.exe"),
            ollama_models_dir: f.0.join("models").to_string_lossy().into_owned(),
            ..Default::default()
        };
        let plan = launch_plan("llm", LlmProvider::Ollama, &config, 11434).unwrap();
        assert!(plan.env.contains(&(
            "OLLAMA_MODELS".into(),
            resolved(&config.ollama_models_dir)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        )));
        assert_eq!(plan.args, ["serve"]);
    }
    #[test]
    fn comfy_plan_only_enables_sage_on_explicit_choice() {
        let f = Fixture::new();
        f.file("python_embeded/python.exe");
        f.file("ComfyUI/main.py");
        let mut config = ServiceConfig {
            comfy_dir: f.0.to_string_lossy().to_string(),
            ..Default::default()
        };
        assert!(!launch_plan("comfyui", LlmProvider::Ollama, &config, 8188)
            .unwrap()
            .args
            .iter()
            .any(|s| s == "--use-sage-attention"));
        assert!(launch_plan("comfyui", LlmProvider::Ollama, &config, 8188)
            .unwrap()
            .args
            .windows(2)
            .any(|a| a == ["--reserve-vram", "1"]));
        config.comfy_sage_attention = true;
        assert!(launch_plan("comfyui", LlmProvider::Ollama, &config, 8188)
            .unwrap()
            .args
            .iter()
            .any(|s| s == "--use-sage-attention"));
        let plan = launch_plan(
            "llm",
            LlmProvider::Ollama,
            &ServiceConfig {
                ollama_executable: f.file("ollama.exe"),
                ..Default::default()
            },
            11434,
        )
        .unwrap();
        assert!(plan
            .env
            .contains(&("OLLAMA_HOST".into(), "127.0.0.1:11434".into())));
        assert!(plan.env.contains(&("OLLAMA_NOPRUNE".into(), "1".into())));
        assert!(plan.env.contains(&("OLLAMA_NO_CLOUD".into(), "1".into())));
        assert_eq!(plan.args, ["serve"]);
    }
}
