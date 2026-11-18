//! Named editing checkpoints, separate from Undo. No media or ASR copies.
use crate::{editor, model::*, store, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path as FsPath};
use uuid::Uuid;
mod production;
pub(crate) use production::{ensure_ready, recover_pending};

const FIELDS: &[&str] = &[
    "production_preset",
    "aspect_ratio",
    "timeline",
    "graphic_objects",
    "longform",
    "clip_origins",
    "shorts",
    "subtitles",
    "motion",
    "social",
    "montage",
    "audio",
];
const LIMIT: u64 = 16 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
pub struct Version {
    schema_version: u32,
    id: Uuid,
    project_id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
    source_updated_at: DateTime<Utc>,
    recovery: bool,
    transcript_fingerprints: BTreeMap<Uuid, Uuid>,
    asset_refs: BTreeMap<Uuid, Value>,
    editing: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    narrative: Option<crate::narrative::Production>,
    integrity: Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    name: Option<String>,
    expected_updated_at: DateTime<Utc>,
    source_version_id: Option<Uuid>,
    #[serde(default)]
    expected_narrative_revision: Option<u64>,
}
fn bad(e: impl ToString) -> ApiError {
    ApiError::bad_request(e.to_string())
}
fn fingerprint(v: &impl Serialize) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        &serde_json::to_vec(v).expect("serializable editing data"),
    )
}
fn name(v: &str) -> Result<String, ApiError> {
    let v = v.trim();
    if v.is_empty() || v.chars().count() > 100 || v.chars().any(char::is_control) {
        return Err(bad(
            "Nome richiesto: 1–100 caratteri, senza caratteri di controllo",
        ));
    }
    Ok(v.into())
}
impl Version {
    fn seal(&mut self) {
        self.integrity = self.checksum();
    }
    fn checksum(&self) -> Uuid {
        if self.schema_version == 2 {
            return fingerprint(&(
                self.project_id,
                &self.transcript_fingerprints,
                &self.asset_refs,
                &self.editing,
                &self.narrative,
            ));
        }
        fingerprint(&(
            self.project_id,
            &self.transcript_fingerprints,
            &self.asset_refs,
            &self.editing,
        ))
    }
    fn capture(p: &Project, label: String, recovery: bool) -> Self {
        let data = serde_json::to_value(p).expect("project JSON");
        let editing: Value = FIELDS
            .iter()
            .map(|&k| (k.to_owned(), data[k].clone()))
            .collect();
        fn referenced(v: &Value, id: Uuid) -> bool {
            match v {
                Value::String(s) => s == &id.to_string(),
                Value::Array(a) => a.iter().any(|v| referenced(v, id)),
                Value::Object(o) => o.values().any(|v| referenced(v, id)),
                _ => false,
            }
        }
        let asset_refs = p
            .assets
            .iter()
            .filter(|a| referenced(&editing, a.id))
            .map(|a| (a.id, json!({"file_name":a.file_name,"kind":a.kind})))
            .collect();
        let mut v = Self {
            schema_version: 1,
            id: Uuid::new_v4(),
            project_id: p.id,
            name: label,
            created_at: Utc::now(),
            source_updated_at: p.updated_at,
            recovery,
            transcript_fingerprints: p
                .transcripts
                .iter()
                .map(|t| (t.id, fingerprint(t)))
                .collect(),
            asset_refs,
            editing,
            narrative: None,
            integrity: Uuid::nil(),
        };
        v.seal();
        v
    }
    fn preview(&self, p: &Project) -> Result<Project, ApiError> {
        if ![1, 2].contains(&self.schema_version)
            || (self.schema_version == 1 && self.narrative.is_some())
            || (self.schema_version == 2 && self.narrative.is_none())
            || self.project_id != p.id
            || self.integrity != self.checksum()
        {
            return Err(bad(
                "Versione estranea, danneggiata o schema non supportato",
            ));
        }
        if self.transcript_fingerprints.iter().any(|(id, hash)| {
            p.transcripts
                .iter()
                .find(|t| t.id == *id)
                .is_none_or(|t| fingerprint(t) != *hash)
        }) {
            return Err(bad("Trascrizione sorgente cambiata: ripristino bloccato per non disallineare sottotitoli e decisioni. La versione resta conservata."));
        }
        for (id, reference) in &self.asset_refs {
            let a = p
                .assets
                .iter()
                .find(|a| a.id == *id)
                .ok_or_else(|| bad("Asset della versione non più presente nella libreria"))?;
            if json!({"file_name":a.file_name,"kind":a.kind}) != *reference {
                return Err(bad(format!(
                    "Media mancante o riferimento modificato: {}",
                    a.name
                )));
            }
        }
        let obj = self
            .editing
            .as_object()
            .ok_or_else(|| bad("Dati versione invalidi"))?;
        // Historical v1 snapshots have ten fields; the optional mixer was added
        // later. Absence means bypass, never inherit the current mix by accident.
        if obj.keys().any(|k| !FIELDS.contains(&k.as_str()))
            || FIELDS
                .iter()
                .filter(|k| **k != "audio" && **k != "production_preset")
                .any(|k| !obj.contains_key(*k))
        {
            return Err(bad("Campi versione incompleti o non consentiti"));
        }
        let mut data = serde_json::to_value(p).map_err(bad)?;
        for &key in FIELDS {
            data[key] = if key == "audio" && !obj.contains_key(key) {
                serde_json::to_value(crate::audio_mixer::Document::default()).map_err(bad)?
            } else {
                self.editing[key].clone()
            };
        }
        let mut next: Project = serde_json::from_value(data).map_err(bad)?;
        if let Some(v) = &next.production_preset {
            crate::production_preset::validate(v)?;
        }
        editor::validate_timeline(&next, &next.timeline).map_err(bad)?;
        crate::audio_mixer::validate(&next.audio, &next).map_err(bad)?;
        crate::motion::validate(&next.motion)?;
        for profile in &next.social.profiles {
            crate::social::validate(profile).map_err(bad)?;
        }
        // New revision branch. Rebase only origins which were fresh in that checkpoint.
        for origin in next.clip_origins.values_mut() {
            if origin.producer == "longform" && origin.input_revision == next.longform.revision {
                origin.input_revision = p.longform.revision.saturating_add(1);
            }
            if origin.producer.starts_with("shorts:")
                && origin.input_revision == next.shorts.revision
            {
                origin.input_revision = p.shorts.revision.saturating_add(1);
            }
        }
        next.longform.revision = p.longform.revision.saturating_add(1);
        next.shorts.revision = p.shorts.revision.saturating_add(1);
        next.subtitles.revision = p.subtitles.revision.saturating_add(1);
        next.motion.revision = p.motion.revision.saturating_add(1);
        next.subtitle_history = Default::default();
        next.updated_at = Utc::now();
        if let Some(n) = &self.narrative {
            let mut n = n.clone();
            crate::narrative::validate(&next, &mut n)?;
        }
        Ok(next)
    }
    fn summary(&self) -> Value {
        json!({"id":self.id,"name":self.name,"created_at":self.created_at,"source_updated_at":self.source_updated_at,"recovery":self.recovery,"clips":self.editing["timeline"]["clips"].as_array().map_or(0,Vec::len),"asset_references":self.asset_refs.len(),"narrative_captured":self.narrative.is_some(),"narrative_scenes":self.narrative.as_ref().map(|n|n.scenes.len())})
    }
}
fn directory(data: &FsPath, id: Uuid) -> std::path::PathBuf {
    store::project_dir(data, id).join("edit-versions")
}
fn media_available(data: &FsPath, p: &Project, v: &Version) -> Result<(), ApiError> {
    for id in v.asset_refs.keys() {
        let a = p
            .assets
            .iter()
            .find(|a| a.id == *id)
            .ok_or_else(|| bad("Asset assente"))?;
        if FsPath::new(&a.file_name).components().count() != 1
            || !store::project_dir(data, p.id)
                .join("assets")
                .join(&a.file_name)
                .is_file()
        {
            return Err(bad(format!("Copia del media non disponibile: {}", a.name)));
        }
    }
    Ok(())
}
fn read(data: &FsPath, pid: Uuid, id: Uuid) -> Result<Version, ApiError> {
    let path = directory(data, pid).join(format!("{id}.json"));
    let meta = std::fs::metadata(&path).map_err(|_| ApiError::not_found("Versione non trovata"))?;
    if meta.len() > LIMIT {
        return Err(bad("Versione oltre 16 MiB"));
    }
    let v: Version = store::read_json(&path).map_err(bad)?;
    if v.id != id || v.project_id != pid {
        return Err(bad("Identità della versione non valida"));
    }
    Ok(v)
}
fn persist(data: &FsPath, v: &Version) -> Result<(), ApiError> {
    if serde_json::to_vec_pretty(v).map_err(bad)?.len() as u64 > LIMIT {
        return Err(bad("Versione oltre 16 MiB: nessun media viene copiato"));
    }
    let dir = directory(data, v.project_id);
    std::fs::create_dir_all(&dir).map_err(bad)?;
    store::write_json(&dir.join(format!("{}.json", v.id)), v).map_err(bad)
}
fn expected(p: &Project, r: &Request) -> Result<(), ApiError> {
    if p.updated_at != r.expected_updated_at {
        Err(ApiError(
            StatusCode::CONFLICT,
            "Progetto cambiato: aggiorna il confronto prima di continuare".into(),
        ))
    } else {
        Ok(())
    }
}
async fn idle(state: &AppState, id: Uuid) -> Result<(), ApiError> {
    ensure_ready(&state.data_dir, id)?;
    if state.jobs.read().await.values().any(|j| {
        j.project_id == Some(id) && matches!(j.status, JobStatus::Queued | JobStatus::Running)
    }) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Attendi i lavori del progetto prima di gestire le versioni".into(),
        ));
    }
    Ok(())
}
pub async fn list(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let _guard = crate::narrative::production_lock().lock().await;
    if !s.projects.read().await.contains_key(&id) {
        return Err(ApiError::not_found("Progetto assente"));
    }
    let dir = directory(&s.data_dir, id);
    let mut items = vec![];
    let mut errors = vec![];
    if dir.exists() {
        for entry in std::fs::read_dir(dir).map_err(bad)? {
            let e = entry.map_err(bad)?;
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Some(vid) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| Uuid::parse_str(s).ok())
            {
                match read(&s.data_dir, id, vid) {
                    Ok(v) => items.push(v.summary()),
                    Err(_) => errors.push(vid),
                }
            }
        }
    }
    items.sort_by_key(|v| std::cmp::Reverse(v["created_at"].as_str().unwrap_or("").to_owned()));
    let projects = s.projects.read().await;
    let p = projects
        .get(&id)
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    let narrative = crate::narrative::load(&s, p)?;
    Ok(Json(
        json!({"versions":items,"unreadable":errors,"narrative_revision":narrative.revision}),
    ))
}
pub async fn create(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(r): Json<Request>,
) -> Result<Json<Value>, ApiError> {
    let _guard = crate::narrative::production_lock().lock().await;
    idle(&s, id).await?;
    let projects = s.projects.write().await;
    let p = projects
        .get(&id)
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    expected(p, &r)?;
    production::expected(&s, p, &r)?;
    let label = name(r.name.as_deref().unwrap_or(""))?;
    let v = if let Some(vid) = r.source_version_id {
        let mut v = read(&s.data_dir, id, vid)?;
        v.preview(p)?;
        v.id = Uuid::new_v4();
        v.name = label;
        v.created_at = Utc::now();
        v.recovery = false;
        v
    } else {
        Version::capture_all(&s, p, label, false)?
    };
    persist(&s.data_dir, &v)?;
    Ok(Json(v.summary()))
}
pub async fn compare(
    State(s): State<AppState>,
    Path((pid, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    let _guard = crate::narrative::production_lock().lock().await;
    let projects = s.projects.read().await;
    let p = projects
        .get(&pid)
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    let v = read(&s.data_dir, pid, id)?;
    let current = serde_json::to_value(p).map_err(bad)?;
    let mut changes: Vec<_> = FIELDS
        .iter()
        .filter(|&&key| current[key] != v.editing[key])
        .map(|&key| json!({"field":key,"current":current[key],"version":v.editing[key]}))
        .collect();
    let narrative = crate::narrative::load(&s, p)?;
    if let Some(n) = &v.narrative {
        if serde_json::to_value(&narrative).map_err(bad)? != serde_json::to_value(n).map_err(bad)? {
            changes.push(json!({"field":"narrative","current":narrative,"version":n}));
        }
    }
    let error = v
        .preview(p)
        .and_then(|_| media_available(&s.data_dir, p, &v))
        .err()
        .map(|e| e.1);
    Ok(Json(
        json!({"version":v.summary(),"expected_updated_at":p.updated_at,"expected_narrative_revision":narrative.revision,"narrative_notice":if v.narrative.is_none(){"Versione precedente al piano narrativo: il piano corrente viene conservato e l’export guidato richiederà revisione."}else{"Il piano narrativo e le decisioni vengono ripristinati insieme al montaggio; i job e i media non vengono riavvolti."},"changes":changes,"can_restore":error.is_none(),"restore_error":error}),
    ))
}
pub async fn rename(
    State(s): State<AppState>,
    Path((pid, id)): Path<(Uuid, Uuid)>,
    Json(r): Json<Request>,
) -> Result<Json<Value>, ApiError> {
    let projects = s.projects.write().await;
    let p = projects
        .get(&pid)
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    expected(p, &r)?;
    let mut v = read(&s.data_dir, pid, id)?;
    v.preview(p)?;
    v.name = name(r.name.as_deref().unwrap_or(""))?;
    persist(&s.data_dir, &v)?;
    Ok(Json(v.summary()))
}
pub async fn restore(
    State(s): State<AppState>,
    Path((pid, id)): Path<(Uuid, Uuid)>,
    Json(r): Json<Request>,
) -> Result<Json<Value>, ApiError> {
    let _guard = crate::narrative::production_lock().lock().await;
    idle(&s, pid).await?;
    let mut projects = s.projects.write().await;
    let p = projects
        .get_mut(&pid)
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    expected(p, &r)?;
    production::expected(&s, p, &r)?;
    let v = read(&s.data_dir, pid, id)?;
    let next = v.preview(p)?;
    media_available(&s.data_dir, p, &v)?;
    // Backup is durable BEFORE replacing current data. On failure it is retained, never erased.
    let backup = Version::capture_all(&s, p, format!("Prima del ripristino · {}", v.name), true)?;
    persist(&s.data_dir, &backup)?;
    let narrative = v.restore_production(&s, p, &next)?;
    production::restore_pair(&s.data_dir, &next, &narrative, backup.id).map_err(bad)?;
    *p = next.clone();
    Ok(Json(json!({"project":next,"backup_id":backup.id})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_snapshots_restore_bypass_new_snapshots_restore_mix() {
        let mut p = Project::new("compat".into(), None);
        let mut old = Version::capture(&p, "old".into(), false);
        old.editing.as_object_mut().unwrap().remove("audio");
        old.seal();
        p.audio.enabled = true;
        p.audio.master.db = -6.;
        p.audio.ducking.enabled = true;
        p.audio.ducking.depth.points = vec![crate::audio_mixer::Point {
            time_ms: 1000,
            db: -9.,
        }];
        p.audio.finalization.enabled = true;
        p.audio.finalization.limiter = true;
        p.audio.finalization.ceiling_dbtp = -3.;
        assert!(!old.preview(&p).unwrap().audio.enabled);
        let new = Version::capture(&p, "mix".into(), false);
        p.audio.master.db = 0.;
        assert_eq!(new.preview(&p).unwrap().audio.master.db, -6.);
        let restored = new.preview(&p).unwrap();
        assert!(restored.audio.ducking.enabled && restored.audio.finalization.limiter);
        assert_eq!(restored.audio.ducking.depth.points[0].db, -9.);
        assert_eq!(restored.audio.finalization.ceiling_dbtp, -3.);
    }
    #[tokio::test]
    async fn active_project_jobs_block_restore_even_when_archived() {
        use std::sync::Arc;
        use tokio::sync::{RwLock, Semaphore};
        let pid = Uuid::new_v4();
        let mut job = Job::queued(
            JobKind::ExportTimeline,
            Some(pid),
            "Test isolato, nessuna elaborazione",
        );
        job.archived = true;
        let state = AppState {
            data_dir: Arc::new(
                std::env::temp_dir().join(format!("nl-versions-unused-{}", Uuid::new_v4())),
            ),
            config: Arc::new(std::sync::RwLock::new(AppConfig::default())),
            projects: Arc::new(RwLock::new(BTreeMap::new())),
            jobs: Arc::new(RwLock::new(BTreeMap::from([(job.id, job.clone())]))),
            workflows: Arc::new(RwLock::new(vec![])),
            http: reqwest::Client::new(),
            worker: Arc::new(Semaphore::new(1)),
        };
        assert!(idle(&state, pid).await.is_err());
        assert!(idle(&state, Uuid::new_v4()).await.is_ok());
        state.jobs.write().await.get_mut(&job.id).unwrap().status = JobStatus::Running;
        assert!(idle(&state, pid).await.is_err());
        state.jobs.write().await.get_mut(&job.id).unwrap().status = JobStatus::Succeeded;
        assert!(idle(&state, pid).await.is_ok());
    }
    #[test]
    fn versions_restore_editing_not_sources_and_reject_damage() {
        let mut p = Project::new("Versioni".into(), None);
        let v = Version::capture(&p, "A".into(), false);
        p.title = "Nuovo nome".into();
        p.story.premise = "Non riavvolgere la produzione".into();
        p.aspect_ratio = "9:16".into();
        let restored = v.preview(&p).unwrap();
        assert_eq!(restored.aspect_ratio, "16:9");
        assert_eq!(restored.title, p.title);
        assert_eq!(restored.story.premise, p.story.premise);
        assert!(restored.motion.revision > p.motion.revision);
        let mut broken = v.clone();
        broken.editing["aspect_ratio"] = json!("1:1");
        assert!(broken.preview(&p).is_err());
        broken = v.clone();
        broken.project_id = Uuid::new_v4();
        assert!(broken.preview(&p).is_err());
        assert!(name("").is_err());
        assert!(name("nome\nrotto").is_err());
    }
    #[test]
    fn disk_roundtrip_and_identity_guard() {
        let dir = std::env::temp_dir().join(format!("nl-versions-unit-{}", Uuid::new_v4()));
        let p = Project::new("Test".into(), None);
        let v = Version::capture(&p, "Versione A".into(), false);
        persist(&dir, &v).unwrap();
        let loaded = read(&dir, p.id, v.id).unwrap();
        assert_eq!(loaded.editing, v.editing);
        assert!(loaded.preview(&p).is_ok());
        assert!(read(&dir, Uuid::new_v4(), v.id).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn added_transcripts_do_not_invalidate_existing_versions_but_changed_raw_does() {
        let mut p = Project::new("ASR".into(), None);
        let t = Transcript {
            id: Uuid::new_v4(),
            asset_id: Uuid::new_v4(),
            language: "it".into(),
            segments: vec![],
            words: vec![],
            imported_at: Utc::now(),
        };
        p.transcripts.push(t.clone());
        let v = Version::capture(&p, "Prima".into(), false);
        let mut other = t;
        other.id = Uuid::new_v4();
        p.transcripts.push(other);
        assert!(v.preview(&p).is_ok());
        p.transcripts[0].language = "en".into();
        assert!(v.preview(&p).is_err());
    }
}
