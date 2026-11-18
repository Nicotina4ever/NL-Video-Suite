//! Editable service settings. Engine/model paths and provenance stay untouched.
use crate::{model::*, store, ApiError, AppState};
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};
use uuid::Uuid;

pub const LLM_ROLES: [(&str, &str); 5] = [
    ("story", "Storie e storyboard"),
    ("clipping", "Sgrossatura, clip, reel e shorts"),
    ("graphics", "Analisi e direzione delle grafiche"),
    ("longform", "Argomenti e capitoli Longform"),
    (
        "vision",
        "Analisi immagini e fotogrammi · richiede modello multimodale",
    ),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsRequest {
    comfy_url: String,
    ollama_url: String,
    llm_model: String,
    /// Partial updates: absent roles are preserved; an empty name removes an override.
    #[serde(default)]
    role_llm_models: Option<BTreeMap<String, String>>,
    #[serde(default)]
    ollama_execution: Option<OllamaExecution>,
    #[serde(default)]
    llm_provider: Option<LlmProvider>,
    #[serde(default)]
    openai_base_url: Option<String>,
}

pub fn provider_model_name(value: &str, provider: LlmProvider) -> Result<String, ApiError> {
    if provider == LlmProvider::Ollama {
        return model_name(value);
    }
    // llama.cpp's real model IDs may be GGUF paths, including spaces and Unicode.
    let name = value.trim();
    if name.is_empty() || name.len() > 512 || name.chars().any(char::is_control) {
        return Err(ApiError::bad_request("Modello LLM: identificativo non valido (massimo 512 byte, senza caratteri di controllo)"));
    }
    Ok(name.to_owned())
}

pub fn model_name(value: &str) -> Result<String, ApiError> {
    let name = value.trim();
    if name.is_empty()
        || name.len() > 160
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-/:".contains(c))
    {
        return Err(ApiError::bad_request(
            "Modello Ollama: usa un nome valido (massimo 160 caratteri, senza spazi)",
        ));
    }
    Ok(name.to_owned())
}

fn role_updates(
    current: &BTreeMap<String, String>,
    updates: &BTreeMap<String, String>,
    provider: LlmProvider,
) -> Result<BTreeMap<String, String>, ApiError> {
    let mut result = current.clone();
    for (role, name) in updates {
        if !LLM_ROLES.iter().any(|(id, _)| role == id) {
            return Err(ApiError::bad_request(format!(
                "Ruolo LLM non supportato: {role}"
            )));
        }
        if name.trim().is_empty() {
            result.remove(role);
        } else {
            result.insert(role.clone(), provider_model_name(name, provider)?);
        }
    }
    Ok(result)
}

/// Normalize only names actually returned by the connected Ollama installation.
/// Configured-but-missing models are reported separately, never inserted here.
pub fn model_catalog(value: Value, config: &AppConfig) -> Result<Value, ApiError> {
    let compatible = config.llm_provider == LlmProvider::OpenaiCompatible;
    let field = if compatible { "data" } else { "models" };
    let raw = value.get(field).and_then(Value::as_array).ok_or_else(|| {
        ApiError::bad_request(format!("Catalogo LLM non valido: campo {field} assente"))
    })?;
    let mut installed = BTreeMap::new();
    for entry in raw {
        let Some(raw_name) = entry
            .get(if compatible { "id" } else { "name" })
            .and_then(Value::as_str)
            .or_else(|| entry.get("model").and_then(Value::as_str))
        else {
            continue;
        };
        let Ok(name) = provider_model_name(raw_name, config.llm_provider) else {
            continue;
        };
        let mut normalized = json!({"name": name, "model": name});
        for field in [
            "size",
            "digest",
            "modified_at",
            "details",
            "remote_model",
            "remote_host",
            "capabilities",
            "id",
            "object",
            "created",
            "owned_by",
            "meta",
            "architecture",
            "status",
        ] {
            if let Some(metadata) = entry.get(field) {
                normalized[field] = metadata.clone();
            }
        }
        normalized["remote"] = json!(entry
            .get("remote_host")
            .and_then(Value::as_str)
            .is_some_and(|host| !host.is_empty()));
        normalized["chat_compatible"] = entry
            .get("capabilities")
            .and_then(Value::as_array)
            .map(|capabilities| {
                json!(capabilities
                    .iter()
                    .any(|capability| capability.as_str() == Some("completion")))
            })
            .unwrap_or(Value::Null);
        installed.entry(name).or_insert(normalized);
    }
    let roles = LLM_ROLES
        .iter()
        .map(|(id, label)| {
            let effective = config.llm_for_role(id);
            json!({"id": id, "label": label,
            "configured_model": config.role_llm_models.get(*id),
            "effective_model": effective,
            "installed": installed.contains_key(effective)})
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"models": installed.into_values().collect::<Vec<_>>(),
        "roles": roles, "default_model": config.llm_model,
        "provider": config.llm_provider,
        "base_url": if compatible { &config.openai_base_url } else { &config.ollama_url },
        "ollama_execution_applies": !compatible,
        "ollama_execution": config.ollama_execution,
        "execution_profiles": [
            {"id": "cpu", "label": "CPU · VRAM libera per ComfyUI", "description": "Invia num_gpu=0 a Ollama; più lento e richiede RAM sufficiente."},
            {"id": "auto", "label": "Automatico · GPU gestita da Ollama", "description": "Non impone un limite VRAM; evita generazioni esterne contemporanee."}
        ]}),
    )
}

pub fn openai_base_url(value: &str) -> Result<String, ApiError> {
    let mut url = reqwest::Url::parse(value.trim()).map_err(|_| {
        ApiError::bad_request(
            "Inserisci un indirizzo HTTP/HTTPS completo per il server compatibile",
        )
    })?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path().trim_end_matches('/'), "" | "/v1")
    {
        return Err(ApiError::bad_request("Server compatibile: usa protocollo, host e porta, con /v1 opzionale; niente credenziali, altri percorsi o parametri"));
    }
    url.set_path("/v1");
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn service_url(value: &str) -> Result<String, ApiError> {
    let url = reqwest::Url::parse(value.trim())
        .map_err(|_| ApiError::bad_request("Inserisci un indirizzo HTTP/HTTPS completo"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(ApiError::bad_request(
            "Usa solo protocollo, host e porta; niente credenziali, percorsi o parametri",
        ));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

/// User-selected server profiles. Settings remain canonical for all existing
/// engines; selecting a profile projects its values onto those settings.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub provider: LlmProvider,
    pub base_url: String,
    pub llm_model: String,
    #[serde(default)]
    pub role_llm_models: BTreeMap<String, String>,
    #[serde(default)]
    pub ollama_execution: OllamaExecution,
    /// External servers are never assumed to be llama.cpp solely from their API.
    #[serde(default)]
    pub launcher: ProviderLauncher,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderLauncher {
    #[default]
    External,
    Ollama,
    Llamacpp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderProfiles {
    pub active_id: String,
    pub profiles: Vec<ProviderProfile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveProviderProfileRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub provider: LlmProvider,
    pub base_url: String,
    pub llm_model: String,
    #[serde(default)]
    pub role_llm_models: BTreeMap<String, String>,
    #[serde(default)]
    pub ollama_execution: OllamaExecution,
    #[serde(default)]
    pub launcher: ProviderLauncher,
    #[serde(default)]
    pub comfy_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivateProviderProfileRequest {
    pub id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeProviderRequest {
    pub provider: LlmProvider,
    pub base_url: String,
}

fn profile_from_config(id: String, name: String, config: &AppConfig) -> ProviderProfile {
    ProviderProfile {
        id,
        name,
        provider: config.llm_provider,
        base_url: match config.llm_provider {
            LlmProvider::Ollama => config.ollama_url.clone(),
            LlmProvider::OpenaiCompatible => config.openai_base_url.clone(),
        },
        llm_model: config.llm_model.clone(),
        role_llm_models: config.role_llm_models.clone(),
        ollama_execution: config.ollama_execution,
        launcher: if config.llm_provider == LlmProvider::Ollama {
            ProviderLauncher::Ollama
        } else {
            ProviderLauncher::External
        },
    }
}

fn validate_profile(mut profile: ProviderProfile) -> Result<ProviderProfile, ApiError> {
    if profile.id.is_empty()
        || profile.id.len() > 80
        || !profile
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(ApiError::bad_request(
            "Identificativo del profilo non valido",
        ));
    }
    profile.name = profile.name.trim().to_owned();
    if profile.name.is_empty()
        || profile.name.len() > 120
        || profile.name.chars().any(char::is_control)
    {
        return Err(ApiError::bad_request(
            "Nome del profilo: da 1 a 120 byte, senza caratteri di controllo",
        ));
    }
    profile.base_url = match profile.provider {
        LlmProvider::Ollama => service_url(&profile.base_url)?,
        LlmProvider::OpenaiCompatible => openai_base_url(&profile.base_url)?,
    };
    profile.llm_model = provider_model_name(&profile.llm_model, profile.provider)?;
    profile.role_llm_models =
        role_updates(&BTreeMap::new(), &profile.role_llm_models, profile.provider)?;
    if matches!(
        (profile.provider, profile.launcher),
        (LlmProvider::Ollama, ProviderLauncher::Llamacpp)
            | (LlmProvider::OpenaiCompatible, ProviderLauncher::Ollama)
    ) {
        return Err(ApiError::bad_request(
            "Il launcher non corrisponde al protocollo del profilo",
        ));
    }
    Ok(profile)
}

fn read_profiles(data: &Path, config: &AppConfig) -> Result<ProviderProfiles, ApiError> {
    let path = data.join("provider_profiles.json");
    if !path.exists() {
        let id = if config.llm_provider == LlmProvider::Ollama {
            "ollama-default"
        } else {
            "existing-provider"
        };
        return Ok(ProviderProfiles {
            active_id: id.into(),
            profiles: vec![profile_from_config(
                id.into(),
                if config.llm_provider == LlmProvider::Ollama {
                    "Ollama locale"
                } else {
                    "Server compatibile"
                }
                .into(),
                config,
            )],
        });
    }
    if std::fs::metadata(&path)
        .map_err(|e| ApiError::internal(e.into()))?
        .len()
        > 128 * 1024
    {
        return Err(ApiError::bad_request("Elenco provider troppo grande"));
    }
    let profiles: ProviderProfiles = store::read_json(&path).map_err(ApiError::internal)?;
    if profiles.profiles.is_empty() || profiles.profiles.len() > 32 {
        return Err(ApiError::bad_request(
            "Sono supportati da 1 a 32 profili LLM",
        ));
    }
    let mut ids = std::collections::BTreeSet::new();
    for profile in &profiles.profiles {
        validate_profile(profile.clone())?;
        if !ids.insert(&profile.id) {
            return Err(ApiError::bad_request("Identificativo provider duplicato"));
        }
    }
    if !ids.contains(&profiles.active_id) {
        return Err(ApiError::bad_request(
            "Il profilo LLM attivo non è presente nell'elenco",
        ));
    }
    Ok(profiles)
}

fn sync_active_profile(profiles: &mut ProviderProfiles, config: &AppConfig) {
    if let Some(active) = profiles
        .profiles
        .iter_mut()
        .find(|p| p.id == profiles.active_id)
    {
        let launcher = active.launcher;
        let mut current = profile_from_config(active.id.clone(), active.name.clone(), config);
        if current.provider == active.provider {
            current.launcher = launcher;
        }
        *active = current;
    }
}

fn apply_provider_profile(config: &mut AppConfig, profile: &ProviderProfile) {
    config.llm_provider = profile.provider;
    match profile.provider {
        LlmProvider::Ollama => config.ollama_url = profile.base_url.clone(),
        LlmProvider::OpenaiCompatible => config.openai_base_url = profile.base_url.clone(),
    }
    config.llm_model = profile.llm_model.clone();
    config.role_llm_models = profile.role_llm_models.clone();
    config.ollama_execution = profile.ollama_execution;
}

pub async fn provider_profiles(
    State(state): State<AppState>,
) -> Result<Json<ProviderProfiles>, ApiError> {
    let config = state.config();
    let mut profiles = read_profiles(&state.data_dir, &config)?;
    sync_active_profile(&mut profiles, &config);
    Ok(Json(profiles))
}

/// Never starts a server or downloads a model. Only queries the explicitly chosen endpoint.
pub async fn probe_provider_models(
    State(state): State<AppState>,
    Json(request): Json<ProbeProviderRequest>,
) -> Result<Json<Value>, ApiError> {
    let mut config = state.config();
    config.llm_provider = request.provider;
    match request.provider {
        LlmProvider::Ollama => config.ollama_url = service_url(&request.base_url)?,
        LlmProvider::OpenaiCompatible => {
            config.openai_base_url = openai_base_url(&request.base_url)?
        }
    }
    Ok(Json(
        crate::providers::list_models(&state.http, &config).await?,
    ))
}

pub async fn save_provider_profile(
    State(state): State<AppState>,
    Json(request): Json<SaveProviderProfileRequest>,
) -> Result<Json<ProviderProfiles>, ApiError> {
    let profile = validate_profile(ProviderProfile {
        id: request.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: request.name,
        provider: request.provider,
        base_url: request.base_url,
        llm_model: request.llm_model,
        role_llm_models: request.role_llm_models,
        ollama_execution: request.ollama_execution,
        launcher: request.launcher,
    })?;
    let comfy_url = request.comfy_url.map(|url| service_url(&url)).transpose()?;
    let jobs = state.jobs.read().await;
    if jobs
        .values()
        .any(|j| matches!(j.status, JobStatus::Running | JobStatus::Queued))
    {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Attendi che la coda sia vuota prima di cambiare provider".into(),
        ));
    }
    let mut current = state.config.write().expect("configuration lock poisoned");
    let mut profiles = read_profiles(&state.data_dir, &current)?;
    sync_active_profile(&mut profiles, &current);
    if let Some(existing) = profiles.profiles.iter_mut().find(|p| p.id == profile.id) {
        *existing = profile.clone();
    } else {
        if profiles.profiles.len() >= 32 {
            return Err(ApiError::bad_request("Massimo 32 profili LLM"));
        }
        profiles.profiles.push(profile.clone());
    }
    profiles.active_id = profile.id.clone();
    let mut next = current.clone();
    apply_provider_profile(&mut next, &profile);
    if let Some(url) = comfy_url {
        next.comfy_url = url;
    }
    // The registry is metadata; settings is canonical. A failed settings write
    // never changes the live worker configuration or starts external services.
    store::write_json(&state.data_dir.join("provider_profiles.json"), &profiles)
        .map_err(ApiError::internal)?;
    store::write_json(&state.data_dir.join("settings.json"), &next).map_err(ApiError::internal)?;
    *current = next;
    Ok(Json(profiles))
}

pub async fn activate_provider_profile(
    State(state): State<AppState>,
    Json(request): Json<ActivateProviderProfileRequest>,
) -> Result<Json<ProviderProfiles>, ApiError> {
    let jobs = state.jobs.read().await;
    if jobs
        .values()
        .any(|j| matches!(j.status, JobStatus::Running | JobStatus::Queued))
    {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Attendi che la coda sia vuota prima di cambiare provider".into(),
        ));
    }
    let mut current = state.config.write().expect("configuration lock poisoned");
    let mut profiles = read_profiles(&state.data_dir, &current)?;
    sync_active_profile(&mut profiles, &current);
    let profile = profiles
        .profiles
        .iter()
        .find(|p| p.id == request.id)
        .cloned()
        .ok_or_else(|| ApiError::bad_request("Profilo LLM non trovato"))?;
    let mut next = current.clone();
    apply_provider_profile(&mut next, &profile);
    profiles.active_id = profile.id.clone();
    store::write_json(&state.data_dir.join("provider_profiles.json"), &profiles)
        .map_err(ApiError::internal)?;
    store::write_json(&state.data_dir.join("settings.json"), &next).map_err(ApiError::internal)?;
    *current = next;
    Ok(Json(profiles))
}

pub fn active_provider_launcher(
    data: &Path,
    config: &AppConfig,
) -> Result<ProviderLauncher, ApiError> {
    let profiles = read_profiles(data, config)?;
    let active = profiles
        .profiles
        .iter()
        .find(|p| p.id == profiles.active_id)
        .ok_or_else(|| ApiError::bad_request("Profilo attivo mancante"))?;
    if active.provider != config.llm_provider {
        return Ok(if config.llm_provider == LlmProvider::Ollama {
            ProviderLauncher::Ollama
        } else {
            ProviderLauncher::External
        });
    }
    Ok(active.launcher)
}

pub async fn save(
    State(state): State<AppState>,
    Json(request): Json<SettingsRequest>,
) -> Result<Json<AppConfig>, ApiError> {
    let comfy_url = service_url(&request.comfy_url)?;
    let ollama_url = service_url(&request.ollama_url)?;
    // Prevent service changes while the shared worker or its queue is occupied.
    let jobs = state.jobs.read().await;
    if jobs
        .values()
        .any(|j| matches!(j.status, JobStatus::Running | JobStatus::Queued))
    {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Attendi che la coda sia vuota prima di cambiare i servizi".into(),
        ));
    }
    let mut current = state.config.write().expect("configuration lock poisoned");
    let mut next = current.clone();
    if let Some(provider) = request.llm_provider {
        next.llm_provider = provider;
    }
    if let Some(base) = request.openai_base_url {
        next.openai_base_url = openai_base_url(&base)?;
    }
    next.comfy_url = comfy_url;
    next.ollama_url = ollama_url;
    next.llm_model = provider_model_name(&request.llm_model, next.llm_provider)?;
    if let Some(updates) = request.role_llm_models {
        next.role_llm_models = role_updates(&next.role_llm_models, &updates, next.llm_provider)?;
    }
    if let Some(execution) = request.ollama_execution {
        next.ollama_execution = execution;
    }
    store::write_json(&state.data_dir.join("settings.json"), &next).map_err(ApiError::internal)?;
    *current = next.clone();
    Ok(Json(next))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_addresses_are_explicit_bases_without_secrets() {
        assert_eq!(
            service_url(" http://localhost:8188/ ").unwrap(),
            "http://localhost:8188"
        );
        assert_eq!(
            service_url("http://[::1]:11434").unwrap(),
            "http://[::1]:11434"
        );
        for bad in [
            "file:///K:/AI",
            "http://user:secret@localhost:8188",
            "http://localhost:8188/api",
            "http://localhost:8188?token=x",
            "http://localhost:8188/#x",
            "localhost:8188",
        ] {
            assert!(service_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn roles_are_real_and_partial_updates_preserve_other_choices() {
        let current = BTreeMap::from([
            ("story".into(), "qwen2.5:14b-instruct".into()),
            ("graphics".into(), "qwen3.5:9b".into()),
        ]);
        let updates = BTreeMap::from([
            ("story".into(), "".into()),
            ("clipping".into(), " llama3.1:8b ".into()),
        ]);
        let result = role_updates(&current, &updates, LlmProvider::Ollama).unwrap();
        assert!(!result.contains_key("story"));
        assert_eq!(result["graphics"], "qwen3.5:9b");
        assert_eq!(result["clipping"], "llama3.1:8b");
        assert!(role_updates(
            &current,
            &BTreeMap::from([("script".into(), "model:latest".into())]),
            LlmProvider::Ollama
        )
        .is_err());
        for bad in ["", "model name", "name\nother", "../name?download=yes"] {
            assert!(model_name(bad).is_err());
        }
    }

    #[test]
    fn old_settings_default_to_cpu_and_role_fallback_without_reset() {
        let mut value = serde_json::to_value(AppConfig::default()).unwrap();
        value.as_object_mut().unwrap().remove("role_llm_models");
        value.as_object_mut().unwrap().remove("ollama_execution");
        value.as_object_mut().unwrap().remove("llm_provider");
        value.as_object_mut().unwrap().remove("openai_base_url");
        let config: AppConfig = serde_json::from_value(value).unwrap();
        assert_eq!(config.llm_for_role("story"), config.llm_model);
        assert_eq!(config.ollama_execution, OllamaExecution::Cpu);
        assert_eq!(config.llm_provider, LlmProvider::Ollama);
        assert_eq!(config.openai_base_url, "http://127.0.0.1:8080/v1");
        assert_eq!(config.ollama_execution.options(8192, 2048)["num_gpu"], 0);
        assert!(OllamaExecution::Auto
            .options(8192, 2048)
            .get("num_gpu")
            .is_none());
        let request: SettingsRequest = serde_json::from_value(json!({
            "comfy_url":"http://localhost:8188", "ollama_url":"http://localhost:11434", "llm_model":"test:latest"
        })).unwrap();
        assert!(request.role_llm_models.is_none());
        assert!(request.ollama_execution.is_none());
        assert!(request.llm_provider.is_none());
        assert!(request.openai_base_url.is_none());
    }

    #[test]
    fn catalog_deduplicates_installed_names_and_does_not_invent_configured_models() {
        let mut config = AppConfig::default();
        config
            .role_llm_models
            .insert("story".into(), "absent:latest".into());
        let result = model_catalog(
            json!({"models":[
                {"name":"qwen2.5:14b-instruct","size":123},
                {"model":"qwen3.5:9b","size":456},
                {"name":"qwen2.5:14b-instruct","size":999},
                {"name":"invalid name"}, {"size":111}
            ]}),
            &config,
        )
        .unwrap();
        assert_eq!(result["models"].as_array().unwrap().len(), 2);
        assert_eq!(result["roles"][0]["effective_model"], "absent:latest");
        assert_eq!(result["roles"][0]["installed"], false);
        assert_eq!(result["roles"][1]["installed"], true);
        assert!(model_catalog(json!({"error":"Unavailable"}), &config).is_err());
    }

    #[test]
    fn catalog_preserves_real_capabilities_and_flags_remote_models_without_guessing() {
        let result = model_catalog(json!({"models":[
            {"name":"chat:latest","capabilities":["completion","vision"]},
            {"name":"embed:latest","capabilities":["embedding"]},
            {"name":"unknown:latest"},
            {"name":"remote:latest","remote_host":"https://ollama.com","capabilities":["completion"]}
        ]}), &AppConfig::default()).unwrap();
        let models = result["models"].as_array().unwrap();
        assert_eq!(models[0]["chat_compatible"], true);
        assert_eq!(models[1]["chat_compatible"], false);
        assert_eq!(models[2]["remote"], true);
        assert!(models[3]["chat_compatible"].is_null());
    }

    #[test]
    fn compatible_base_and_real_gguf_ids_are_supported_without_credentials() {
        assert_eq!(
            openai_base_url("http://localhost:1234/").unwrap(),
            "http://localhost:1234/v1"
        );
        assert_eq!(
            openai_base_url("http://192.168.1.8:8080/v1/").unwrap(),
            "http://192.168.1.8:8080/v1"
        );
        for invalid in [
            "http://user:password@localhost:8080/v1",
            "file:///K:/AI",
            "http://localhost:8080/api",
            "http://localhost:8080/v1?api_key=secret",
        ] {
            assert!(openai_base_url(invalid).is_err());
        }
        assert_eq!(
            provider_model_name("../modelli/Italiano Q4.gguf", LlmProvider::OpenaiCompatible)
                .unwrap(),
            "../modelli/Italiano Q4.gguf"
        );
        assert!(provider_model_name("line\nbreak", LlmProvider::OpenaiCompatible).is_err());
        let config = AppConfig {
            llm_provider: LlmProvider::OpenaiCompatible,
            llm_model: "real.gguf".into(),
            ..AppConfig::default()
        };
        let catalog = model_catalog(
            json!({"data":[{"id":"real.gguf"},{"id":"real.gguf"}]}),
            &config,
        )
        .unwrap();
        assert_eq!(catalog["models"].as_array().unwrap().len(), 1);
        assert_eq!(catalog["roles"][0]["installed"], true);
        assert_eq!(catalog["ollama_execution_applies"], false);
    }

    #[test]
    fn profiles_keep_protocols_role_models_and_launchers_separate() {
        let ollama = profile_from_config(
            "ollama-default".into(),
            "Ollama".into(),
            &AppConfig::default(),
        );
        let compatible = validate_profile(ProviderProfile {
            id: "studio".into(),
            name: " LM Studio italiano ".into(),
            provider: LlmProvider::OpenaiCompatible,
            base_url: "http://localhost:1234".into(),
            llm_model: "modello reale.gguf".into(),
            role_llm_models: BTreeMap::from([("story".into(), "storie Q4.gguf".into())]),
            ollama_execution: OllamaExecution::Cpu,
            launcher: ProviderLauncher::External,
        })
        .unwrap();
        assert_eq!(compatible.name, "LM Studio italiano");
        assert_eq!(compatible.base_url, "http://localhost:1234/v1");
        let mut config = AppConfig::default();
        apply_provider_profile(&mut config, &compatible);
        assert_eq!(config.llm_for_role("story"), "storie Q4.gguf");
        assert_eq!(config.llm_for_role("clipping"), "modello reale.gguf");
        assert_eq!(config.ollama_url, "http://127.0.0.1:11434");
        apply_provider_profile(&mut config, &ollama);
        assert_eq!(config.llm_provider, LlmProvider::Ollama);
        assert!(config.role_llm_models.is_empty());
        assert_eq!(config.ollama_execution, OllamaExecution::Cpu);
        assert_eq!(config.openai_base_url, "http://localhost:1234/v1");
        let mut bad = compatible;
        bad.launcher = ProviderLauncher::Ollama;
        assert!(validate_profile(bad).is_err());
    }

    #[test]
    fn active_profile_preserves_settings_edits_before_switching() {
        let mut current = AppConfig::default();
        let profile = profile_from_config("ollama-default".into(), "Casa".into(), &current);
        let mut profiles = ProviderProfiles {
            active_id: profile.id.clone(),
            profiles: vec![profile],
        };
        current
            .role_llm_models
            .insert("graphics".into(), "regista:latest".into());
        current.llm_model = "nuovo:latest".into();
        sync_active_profile(&mut profiles, &current);
        assert_eq!(profiles.profiles[0].name, "Casa");
        assert_eq!(profiles.profiles[0].launcher, ProviderLauncher::Ollama);
        assert_eq!(
            profiles.profiles[0].role_llm_models["graphics"],
            "regista:latest"
        );
        assert_eq!(profiles.profiles[0].llm_model, "nuovo:latest");
        current.llm_provider = LlmProvider::OpenaiCompatible;
        current.llm_model = "manual Q4.gguf".into();
        sync_active_profile(&mut profiles, &current);
        assert_eq!(profiles.profiles[0].launcher, ProviderLauncher::External);
    }
}
