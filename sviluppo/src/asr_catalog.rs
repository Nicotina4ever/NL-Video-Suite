//! Explicit local ASR profiles. Registration links existing resources, never downloads them.
use crate::{asr, model::*, store, ApiError, AppState};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    path::{Path as FsPath, PathBuf},
    sync::OnceLock,
};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub model: PathBuf,
    pub aligner: PathBuf,
    #[serde(default)]
    pub runner: PathBuf,
    #[serde(default = "threads")]
    pub threads: u8,
    #[serde(default = "chunk")]
    pub chunk_seconds: u16,
    #[serde(default)]
    pub variant: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub license_note: String,
}
fn threads() -> u8 {
    4
}
fn chunk() -> u16 {
    30
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub revision: u64,
    pub default_id: Option<String>,
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub retired_profiles: Vec<Profile>,
}
fn bad(e: impl ToString) -> ApiError {
    ApiError::bad_request(e.to_string())
}
pub(crate) fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
fn load(s: &AppState) -> Result<Catalog, ApiError> {
    let path = s.data_dir.join("asr_catalog.json");
    if path.exists() {
        store::read_json(&path).map_err(ApiError::internal)
    } else {
        Ok(Catalog::default())
    }
}
fn builtin(s: &AppState) -> Result<Vec<Profile>, ApiError> {
    let config = s.config();
    let q = asr::load(s)?;
    Ok(vec![
        Profile {
            id: "whisper-default".into(),
            name: "WhisperX · configurazione portable".into(),
            engine: "whisperx".into(),
            model: config.asr_model_path,
            aligner: config.alignment_model_path,
            runner: PathBuf::new(),
            threads: 4,
            chunk_seconds: 30,
            variant: String::new(),
            source: "Configurazione della suite".into(),
            license_note: String::new(),
        },
        Profile {
            id: "qwen-configured".into(),
            name: "Qwen GGUF · configurazione locale".into(),
            engine: "qwen_gguf".into(),
            model: q.model,
            aligner: q.aligner,
            runner: q.runner,
            threads: q.threads,
            chunk_seconds: q.chunk_seconds,
            variant: String::new(),
            source: "Configurazione Qwen esistente".into(),
            license_note: String::new(),
        },
    ])
}
pub fn resolve(s: &AppState, id: &str) -> Result<Profile, ApiError> {
    builtin(s)?.into_iter().chain(load(s)?.profiles).find(|p|p.id==id).ok_or_else(||bad("Profilo ASR non disponibile: scegli un profilo installato, nessun fallback implicito"))
}
pub fn effective(s: &AppState, p: &Project, explicit: Option<&str>) -> Result<Profile, ApiError> {
    let c = load(s)?;
    resolve(
        s,
        explicit
            .or(p.longform.asr_profile_id.as_deref())
            .or(c.default_id.as_deref())
            .unwrap_or("whisper-default"),
    )
}
impl Profile {
    pub fn cache_key(&self, source: &FsPath, language: &str) -> String {
        let stamp = |path: &FsPath| {
            let meta = std::fs::metadata(path).ok();
            json!({"path":path.canonicalize().ok(),"length":meta.as_ref().map(|m|m.len()),"modified":meta.and_then(|m|m.modified().ok()).and_then(|t|t.duration_since(std::time::UNIX_EPOCH).ok()).map(|v|v.as_nanos().to_string())})
        };
        let files = [
            self.model.clone(),
            self.aligner.clone(),
            self.model.join("config.json"),
            self.model.join("model.bin"),
            self.model.join("tokenizer.json"),
            self.aligner.join("config.json"),
            self.aligner.join("pytorch_model.bin"),
        ]
        .iter()
        .map(|p| stamp(p))
        .collect::<Vec<_>>();
        Uuid::new_v5(&Uuid::NAMESPACE_OID,json!({"adapter":"whisperx-profile-cache-v1","bridge":include_str!("../bridge/clipsai_bridge.py"),"source":stamp(source),"dependencies":files,"language":language}).to_string().as_bytes()).to_string()
    }
    pub fn qwen(&self) -> asr::Config {
        asr::Config {
            schema_version: 1,
            runner: self.runner.clone(),
            model: self.model.clone(),
            aligner: self.aligner.clone(),
            threads: self.threads,
            chunk_seconds: self.chunk_seconds,
        }
    }
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.name.trim().is_empty()
            || self.name.len() > 200
            || [&self.variant, &self.source, &self.license_note]
                .iter()
                .any(|v| v.len() > 2000)
        {
            return Err(bad("Nome richiesto e metadati ASR troppo lunghi"));
        }
        match self.engine.as_str() {
            "qwen_gguf" => asr::validate(&self.qwen()),
            "whisperx" => {
                for path in [
                    self.model.join("config.json"),
                    self.model.join("model.bin"),
                    self.model.join("tokenizer.json"),
                    self.aligner.join("config.json"),
                    self.aligner.join("pytorch_model.bin"),
                ] {
                    if !path.is_file() {
                        return Err(bad(format!(
                            "Dipendenza WhisperX mancante: {}",
                            path.display()
                        )));
                    }
                }
                Ok(())
            }
            _ => Err(bad("Nessun adapter operativo per questo backend")),
        }
    }
}
// Bounded, read-only accounting. Link/junction contents are never traversed.
fn inventory(path: &FsPath) -> Value {
    if path.as_os_str().is_empty() {
        return json!({"path":path,"present":false,"bytes":0,"complete":true});
    }
    let canonical = path
        .canonicalize()
        .ok()
        .map(|p| crate::runtime::native_path(&p));
    let mut stack = vec![path.to_owned()];
    let mut bytes = 0u64;
    let mut files = 0usize;
    let mut complete = true;
    let mut seen = BTreeSet::new();
    let started = std::time::Instant::now();
    while let Some(p) = stack.pop() {
        if seen.len() >= 20_000 || started.elapsed() > std::time::Duration::from_secs(2) {
            complete = false;
            break;
        }
        if !seen.insert(p.clone()) {
            continue;
        }
        if crate::storage::reject_links(&p).is_err() {
            complete = false;
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&p) else {
            complete = false;
            continue;
        };
        if meta.is_file() {
            bytes = bytes.saturating_add(meta.len());
            files += 1;
        } else if meta.is_dir() {
            match std::fs::read_dir(p) {
                Ok(entries) => {
                    for e in entries {
                        if seen.len() + stack.len() >= 20_000
                            || started.elapsed() > std::time::Duration::from_secs(2)
                        {
                            complete = false;
                            break;
                        }
                        if let Ok(e) = e {
                            stack.push(e.path());
                        } else {
                            complete = false;
                        }
                    }
                }
                Err(_) => complete = false,
            }
        }
    }
    json!({"path":path,"canonical_path":canonical,"present":path.exists(),"bytes":bytes,"files":files,"complete":complete,"ownership":"unknown","deletable":false,"space_kind":"logical_bytes_not_reclaimable_space","notice":"Risorsa preesistente: proprietà esclusiva non provata. Nessuna cancellazione consentita."})
}
fn reference_value(v: &Value, p: &Profile) -> bool {
    fn path_key(s: &str) -> String {
        s.trim_start_matches(r"\\?\")
            .replace('\\', "/")
            .to_lowercase()
    }
    let keys = [&p.model, &p.aligner, &p.runner]
        .into_iter()
        .filter(|v| !v.as_os_str().is_empty())
        .flat_map(|v| {
            let canonical = v
                .canonicalize()
                .map(|v| crate::runtime::native_path(&v))
                .unwrap_or_else(|_| v.clone());
            [
                path_key(&v.to_string_lossy()),
                path_key(&canonical.to_string_lossy()),
            ]
        })
        .collect::<Vec<_>>();
    fn walk(v: &Value, id: &str, keys: &[String]) -> bool {
        match v {
            Value::String(s) => {
                let value = path_key(s);
                (!id.is_empty() && s == id)
                    || keys
                        .iter()
                        .any(|k| value == *k || value.starts_with(&(k.clone() + "/")))
            }
            Value::Array(a) => a.iter().any(|v| walk(v, id, keys)),
            Value::Object(o) => o.values().any(|v| walk(v, id, keys)),
            _ => false,
        }
    }
    walk(v, &p.id, &keys)
}
pub(crate) fn historical_usage(
    s: &AppState,
    p: &Profile,
    projects: &std::collections::BTreeMap<Uuid, Project>,
) -> Value {
    let mut refs = vec![];
    let mut complete = true;
    let mut count = 0;
    let start = std::time::Instant::now();
    for project in projects.values() {
        if project.longform.asr_profile_id.as_deref() == Some(&p.id)
            || project
                .transcript_metadata
                .values()
                .any(|m| reference_value(m, p))
        {
            refs.push(json!({"project_id":project.id,"kind":"profile_or_raw_metadata","title":project.title}));
        }
        for folder in ["edit-versions", "jobs"] {
            let mut stack = vec![store::project_dir(&s.data_dir, project.id).join(folder)];
            while let Some(path) = stack.pop() {
                if !path.exists() {
                    continue;
                }
                if count >= 10_000 || start.elapsed() > std::time::Duration::from_secs(3) {
                    complete = false;
                    break;
                }
                count += 1;
                if crate::storage::reject_links(&path).is_err() {
                    complete = false;
                    continue;
                }
                if path.is_dir() {
                    if let Ok(entries) = std::fs::read_dir(path) {
                        for entry in entries {
                            if stack.len() >= 10_000 {
                                complete = false;
                                break;
                            }
                            if let Ok(entry) = entry {
                                stack.push(entry.path());
                            } else {
                                complete = false;
                            }
                        }
                    } else {
                        complete = false;
                    }
                    continue;
                }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if folder == "jobs" && !matches!(name, "request.json" | "result.json") {
                    continue;
                }
                if path.extension().is_none_or(|v| v != "json") {
                    continue;
                }
                if std::fs::metadata(&path)
                    .map(|m| m.len() > 32 * 1024 * 1024)
                    .unwrap_or(true)
                {
                    complete = false;
                    continue;
                }
                match store::read_json::<Value>(&path) {
                    Ok(v) => {
                        if reference_value(&v, p) {
                            refs.push(json!({"project_id":project.id,"kind":folder,"path":path}));
                        }
                    }
                    Err(_) => complete = false,
                }
            }
        }
    }
    json!({"references":refs,"complete":complete,"visited_entries":count,"external_usage":"not_provable_no_external_model_is_deletable"})
}
pub async fn view(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    let _guard = lock().lock().await;
    let c = load(&s)?;
    let mut profiles = builtin(&s)?;
    profiles.extend(c.profiles.clone());
    let projects = s.projects.read().await.clone();
    let busy = s
        .jobs
        .read()
        .await
        .values()
        .any(|j| matches!(j.status, JobStatus::Queued | JobStatus::Running));
    let rows=profiles.into_iter().map(|p|{
        let ready=p.validate();let users=projects.values().filter(|v|v.longform.asr_profile_id.as_deref()==Some(&p.id)).map(|v|json!({"id":v.id,"title":v.title})).collect::<Vec<_>>();
        let deps=vec![json!({"role":"asr","resource":inventory(&p.model)}),json!({"role":"aligner","resource":inventory(&p.aligner)})];
        let usage=historical_usage(&s,&p,&projects);
        json!({"profile":p,"ready":ready.is_ok(),"notice":ready.err().map(|e|e.1),"dependencies":deps,"runner":inventory(&p.runner),"project_references":users,"usage":usage,"read_only":p.id=="whisper-default"||p.id=="qwen-configured","inference_verified":false,"format":if p.engine=="qwen_gguf"{"gguf"}else{"ctranslate2+transformers_aligner"},"hardware":"cpu","variant_provenance":"user_declared_or_unspecified_not_inferred_from_filename"})
    }).collect::<Vec<_>>();
    Ok(Json(
        json!({"schema_version":1,"revision":c.revision,"default_id":c.default_id.unwrap_or_else(||"whisper-default".into()),"profiles":rows,"busy":busy,"downloads":false,"runtime":inventory(&s.config().python_executable),"execution":"WhisperX: auto/cuda/cpu; configured Qwen GGUF runner: cpu. Availability checks files, not model accuracy.","deletion_policy":"Existing/shared/external/unknown resources cannot be deleted. Unlinking only removes the catalog reference."}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub expected_revision: u64,
    pub action: String,
    pub profile: Option<Profile>,
    pub id: Option<String>,
}
pub async fn change(
    State(s): State<AppState>,
    Json(req): Json<Change>,
) -> Result<Json<Value>, ApiError> {
    {
        let _guard = lock().lock().await;
        let mut c = load(&s)?;
        if c.revision != req.expected_revision {
            return Err(ApiError(
                axum::http::StatusCode::CONFLICT,
                "Catalogo aggiornato altrove: ricarica".into(),
            ));
        }
        match req.action.as_str() {
            "register" => {
                let mut p = req.profile.ok_or_else(|| bad("Profilo richiesto"))?;
                p.validate()?;
                let root = std::env::current_dir().map_err(|e| ApiError::internal(e.into()))?;
                for path in [&mut p.model, &mut p.aligner, &mut p.runner] {
                    if path.as_os_str().is_empty() {
                        continue;
                    }
                    if let Ok(canonical) = path.canonicalize() {
                        let canonical = crate::runtime::native_path(&canonical);
                        *path = canonical
                            .strip_prefix(&root)
                            .unwrap_or(&canonical)
                            .to_owned();
                    }
                }
                p.id = Uuid::new_v4().to_string();
                c.profiles.push(p);
            }
            "default" => {
                let id = req.id.ok_or_else(|| bad("Profilo richiesto"))?;
                resolve(&s, &id)?.validate()?;
                c.default_id = Some(id);
            }
            "unlink" => {
                let id = req.id.ok_or_else(|| bad("Profilo richiesto"))?;
                if c.default_id.as_deref() == Some(&id)
                    || s.projects
                        .read()
                        .await
                        .values()
                        .any(|p| p.longform.asr_profile_id.as_deref() == Some(&id))
                    || s.jobs
                        .read()
                        .await
                        .values()
                        .any(|j| matches!(j.status, JobStatus::Queued | JobStatus::Running))
                {
                    return Err(bad("Profilo predefinito, riferito da un progetto o coda attiva: scollegamento bloccato"));
                }
                if !c.profiles.iter().any(|p| p.id == id) {
                    return Err(bad("Profilo incorporato o assente: non scollegabile"));
                }
                if let Some(index) = c.profiles.iter().position(|p| p.id == id) {
                    let p = c.profiles.remove(index);
                    c.retired_profiles.push(p);
                }
            }
            _ => return Err(bad("Operazione non supportata. Nessun modello cancellato")),
        }
        c.revision = c.revision.saturating_add(1);
        store::write_json(&s.data_dir.join("asr_catalog.json"), &c).map_err(ApiError::internal)?;
    }
    view(State(s)).await
}
pub(crate) async fn resource_usage(s: &AppState, path: &FsPath) -> Result<Value, ApiError> {
    let projects = s.projects.read().await.clone();
    let probe = Profile {
        id: String::new(),
        name: "usage".into(),
        engine: "whisperx".into(),
        model: path.to_owned(),
        aligner: PathBuf::new(),
        runner: PathBuf::new(),
        threads: 4,
        chunk_seconds: 30,
        variant: String::new(),
        source: String::new(),
        license_note: String::new(),
    };
    let mut result = historical_usage(s, &probe, &projects);
    let c = load(s)?;
    let mut profiles = builtin(s)?;
    profiles.extend(c.profiles.clone());
    for p in profiles
        .into_iter()
        .chain(c.retired_profiles.iter().cloned())
    {
        if reference_value(&json!([p.model, p.aligner, p.runner]), &probe) {
            let u = historical_usage(s, &p, &projects);
            let active = !c.retired_profiles.iter().any(|v| v.id == p.id);
            if active
                || !u["references"].as_array().is_some_and(|a| a.is_empty())
                || u["complete"] != true
            {
                result["references"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"kind":"profile_dependency","profile_id":p.id,"usage":u}));
            }
            if u["complete"] != true {
                result["complete"] = json!(false);
            }
        }
    }
    Ok(result)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub profile_id: Option<String>,
}
pub async fn select(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Selection>,
) -> Result<Json<Project>, ApiError> {
    let _guard = lock().lock().await;
    if let Some(profile) = &req.profile_id {
        resolve(&s, profile)?.validate()?;
    }
    crate::update_project(&s, id, |p| {
        if p.updated_at != req.expected_updated_at {
            return Err(ApiError(
                axum::http::StatusCode::CONFLICT,
                "Progetto aggiornato altrove".into(),
            ));
        }
        p.longform.asr_profile_id = req.profile_id;
        Ok(())
    })
    .await?;
    Ok(Json(
        s.projects
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| bad("Progetto assente"))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, Profile) {
        let root = std::env::temp_dir().join(format!("nl-asr-catalog-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("model")).unwrap();
        std::fs::create_dir_all(root.join("aligner")).unwrap();
        for file in [
            "model/config.json",
            "model/model.bin",
            "model/tokenizer.json",
            "aligner/config.json",
            "aligner/pytorch_model.bin",
            "source.wav",
        ] {
            std::fs::write(root.join(file), b"{}").unwrap();
        }
        let p = Profile {
            id: Uuid::new_v4().to_string(),
            name: "Fixture".into(),
            engine: "whisperx".into(),
            model: root.join("model"),
            aligner: root.join("aligner"),
            runner: PathBuf::new(),
            threads: 4,
            chunk_seconds: 30,
            variant: String::new(),
            source: String::new(),
            license_note: String::new(),
        };
        (root, p)
    }
    #[test]
    fn inventory_counts_logical_bytes_without_claiming_exclusive_ownership() {
        let (root, p) = fixture();
        let m = inventory(&p.model);
        assert_eq!(m["bytes"], 6);
        assert_eq!(m["files"], 3);
        assert_eq!(m["complete"], true);
        assert_eq!(m["deletable"], false);
        assert_eq!(m["ownership"], "unknown");
        assert_eq!(inventory(&root.join("absent"))["complete"], false);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cache_identity_uses_resources_language_and_input_not_profile_label() {
        let (root, mut p) = fixture();
        let input = root.join("source.wav");
        let key = p.cache_key(&input, "it");
        p.name = "Changed".into();
        p.id = Uuid::new_v4().to_string();
        assert_eq!(key, p.cache_key(&input, "it"));
        assert_ne!(key, p.cache_key(&input, "en"));
        std::fs::write(p.model.join("model.bin"), b"changed checkpoint fixture").unwrap();
        assert_ne!(key, p.cache_key(&input, "it"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unsupported_adapters_and_missing_dependencies_fail_without_fallback() {
        let (root, mut p) = fixture();
        assert!(p.validate().is_ok());
        p.engine = "vosk".into();
        assert!(p.validate().is_err());
        p.engine = "whisperx".into();
        std::fs::remove_file(p.aligner.join("pytorch_model.bin")).unwrap();
        assert!(p.validate().is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn raw_and_snapshot_references_are_not_limited_to_current_profile_selection() {
        let (root, p) = fixture();
        assert!(reference_value(
            &json!({"models":{"aligner":{"path":p.aligner}}}),
            &p
        ));
        assert!(reference_value(
            &json!({"longform":{"asr_profile_id":p.id}}),
            &p
        ));
        assert!(!reference_value(&json!({"text":"unrelated"}), &p));
        std::fs::remove_dir_all(root).unwrap();
    }
}
