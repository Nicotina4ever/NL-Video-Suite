//! Confirmed deletion of one owned project, using the existing checked file remover.
//! No source_path, model directory or remote-provider output is a deletion target.
use crate::{model::*, storage, store, ApiError, AppState};
use axum::{
    extract::{Path as RoutePath, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const AREA: &str = "maintenance/project-deletions";
fn conflict(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::CONFLICT, message.into())
}
fn pending(data: &Path, id: Uuid) -> PathBuf {
    data.join(AREA).join(format!("pending-{id}.json"))
}
pub(crate) fn ensure_ready(data: &Path, id: Uuid) -> Result<(), ApiError> {
    if pending(data, id).exists() {
        return Err(conflict("Questo progetto è in eliminazione o è già stato eliminato. Completa la rimozione da Progetti e materiali."));
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
struct Plan {
    id: Uuid,
    project_id: Uuid,
    title: String,
    updated_at: DateTime<Utc>,
    data: PathBuf,
    expires_at: DateTime<Utc>,
    files: Vec<storage::CacheFile>,
    directories: Vec<PathBuf>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteRequest {
    plan_id: Uuid,
    confirmation: String,
}

fn roots(data: &Path, id: Uuid, jobs: &[Job]) -> Vec<PathBuf> {
    let mut paths = vec![
        store::project_dir(data, id)
            .strip_prefix(data)
            .expect("project inside data")
            .to_path_buf(),
        PathBuf::from("previews").join(id.to_string()),
    ];
    for job in jobs.iter().filter(|j| j.project_id == Some(id)) {
        paths.push(PathBuf::from("logs").join(format!("{}.log", job.id)));
    }
    paths
}
fn category(relative: &Path) -> &'static str {
    let parts = relative
        .iter()
        .map(|s| s.to_string_lossy())
        .collect::<Vec<_>>();
    if parts.first().is_some_and(|s| s == "previews") {
        return "Anteprime e temporanei";
    }
    match parts.get(2).map(|s| s.as_ref()) {
        Some("assets") => "File importati e allegati",
        Some("exports") => "Video e risultati esportati",
        Some("analysis" | ".cache") => "Analisi e cache del progetto",
        Some("edit_versions" | "edit-versions" | "versions") => "Versioni del progetto",
        _ => "Dati, copioni, reference e altri file del progetto",
    }
}
fn inventory(
    data: &Path,
    id: Uuid,
    jobs: &[Job],
) -> anyhow::Result<(Vec<storage::CacheFile>, Vec<PathBuf>)> {
    let mut files = vec![];
    let mut directories = vec![];
    let mut stack = roots(data, id, jobs)
        .into_iter()
        .map(|p| data.join(p))
        .collect::<Vec<_>>();
    while let Some(path) = stack.pop() {
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        anyhow::ensure!(
            !storage::linked(&meta),
            "Collegamento/junction escluso: {}. Nessun file esterno verrà seguito.",
            path.display()
        );
        storage::reject_links(&path)?;
        let relative = path.strip_prefix(data)?.to_path_buf();
        if meta.is_dir() {
            directories.push(relative);
            for entry in fs::read_dir(&path)? {
                stack.push(entry?.path());
            }
        } else if meta.is_file() {
            let (secs, nanos) = storage::stamp(&meta)?;
            files.push(storage::CacheFile {
                category: category(&relative).into(),
                relative_path: relative,
                bytes: meta.len(),
                modified_secs: secs,
                modified_nanos: nanos,
            });
        } else {
            anyhow::bail!("File speciale non eliminabile: {}", path.display());
        }
    }
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    directories.sort();
    Ok((files, directories))
}
fn points_into(value: &Value, root: &str, url: &str) -> bool {
    match value {
        Value::String(s) => {
            let s = s.replace('\\', "/").to_lowercase();
            s.starts_with(root) || s.contains(url)
        }
        Value::Array(a) => a.iter().any(|v| points_into(v, root, url)),
        Value::Object(o) => o.values().any(|v| points_into(v, root, url)),
        _ => false,
    }
}
fn blockers(
    data: &Path,
    target: Uuid,
    all: &BTreeMap<Uuid, Project>,
    jobs: &[Job],
) -> anyhow::Result<Vec<String>> {
    let root = store::project_dir(data, target);
    let prefix = format!(
        "{}/",
        root.to_string_lossy().replace('\\', "/").to_lowercase()
    );
    let url = format!("/api/projects/{target}/");
    let mut result = vec![];
    for p in all.values().filter(|p| p.id != target) {
        let mut value = serde_json::to_value(p)?;
        for (index, a) in p.assets.iter().enumerate() {
            let path = store::project_dir(data, p.id)
                .join("assets")
                .join(&a.file_name);
            if let Ok(owned) = storage::native_canonical(&path) {
                if owned.starts_with(&root) {
                    result.push(format!(
                        "Il progetto «{}» usa un file di questo progetto.",
                        p.title
                    ));
                }
                // An imported copy owns its own path. Its source_path is provenance.
                else if owned.starts_with(store::project_dir(data, p.id))
                    && storage::reject_links(&path).is_ok()
                {
                    value["assets"][index]
                        .as_object_mut()
                        .unwrap()
                        .remove("source_path");
                }
            }
        }
        if points_into(&value, &prefix, &url) {
            result.push(format!("Il progetto «{}» contiene un riferimento diretto: importane una copia prima di eliminare questo progetto.",p.title));
        }
    }
    for job in jobs
        .iter()
        .filter(|j| matches!(j.status, JobStatus::Queued | JobStatus::Running))
    {
        if job.project_id == Some(target) {
            result.push(format!(
                "Lavoro ancora attivo: {}. Attendi il completamento o gestiscilo in Lavori.",
                job.message
            ));
        } else if points_into(&serde_json::to_value(job)?, &prefix, &url) {
            result.push("Un altro lavoro in corso usa file di questo progetto.".into());
        }
    }
    result.sort();
    result.dedup();
    Ok(result)
}

pub async fn preview(
    State(state): State<AppState>,
    RoutePath(id): RoutePath<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let all = state.projects.read().await.clone();
    let project = all
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    let jobs = state
        .jobs
        .read()
        .await
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let data = state.data_dir.as_ref().clone();
    tokio::task::spawn_blocking(move|| -> anyhow::Result<Value> {
        storage::reject_links(&data)?;let data=storage::native_canonical(&data)?;
        let reasons=blockers(&data,id,&all,&jobs)?;let (files,directories)=inventory(&data,id,&jobs)?;
        let plan=Plan{id:Uuid::new_v4(),project_id:id,title:project.title.clone(),updated_at:project.updated_at,data:data.clone(),expires_at:Utc::now()+Duration::minutes(15),files,directories};
        let directory=storage::safe_directory(&data,Path::new(AREA))?;
        store::write_json(&directory.join(format!("plan-{}.json",plan.project_id)),&plan)?;
        let mut categories=BTreeMap::<String,(usize,u64)>::new();for f in &plan.files {let entry=categories.entry(f.category.clone()).or_default();entry.0+=1;entry.1+=f.bytes;}
        Ok(json!({"plan_id":plan.id,"project_id":id,"title":project.title,"files":plan.files.len(),"bytes":plan.files.iter().map(|f|f.bytes).sum::<u64>(),"categories":categories.into_iter().map(|(name,(files,bytes))|json!({"name":name,"files":files,"bytes":bytes})).collect::<Vec<_>>(),"blocked":reasons,"can_delete":reasons.is_empty(),"pending":pending(&data,id).exists(),"external_sources_preserved":project.assets.iter().filter(|a|!a.source_path.is_empty()).count(),"expires_at":plan.expires_at,"note":"Eliminazione definitiva delle sole copie interne, risultati, versioni e cache di questo progetto. Originali esterni, altri progetti, modelli, runtime e output del servizio ComfyUI restano intatti. Byte logici: lo spazio fisico liberato può essere minore per hardlink condivisi o file ancora aperti."}))
    }).await.map_err(|e|ApiError::internal(e.into()))?.map(Json).map_err(|e|ApiError::bad_request(e.to_string()))
}

#[derive(Serialize)]
struct Removal {
    complete: bool,
    deleted_files: usize,
    deleted_bytes: u64,
    remaining: Vec<String>,
    warnings: Vec<String>,
}
fn execute(data: &Path, p: &Project, plan: &Plan, jobs: &[Job]) -> anyhow::Result<Removal> {
    anyhow::ensure!(
        plan.data == data
            && plan.project_id == p.id
            && plan.title == p.title
            && plan.updated_at == p.updated_at
            && plan.expires_at > Utc::now(),
        "Riepilogo scaduto o progetto modificato. Aggiorna il riepilogo."
    );
    let (fresh, dirs) = inventory(data, p.id, jobs)?;
    anyhow::ensure!(
        serde_json::to_vec(&fresh)? == serde_json::to_vec(&plan.files)? && dirs == plan.directories,
        "File o temporanei modificati dopo il riepilogo. Aggiornalo prima di confermare."
    );
    let marker = pending(data, p.id);
    storage::safe_directory(data, Path::new(AREA))?;
    if marker.exists() {
        storage::reject_links(&marker)?;
    }
    store::write_json(
        &marker,
        &json!({"project_id":p.id,"state":"deleting","at":Utc::now()}),
    )?;
    let project_root = store::project_dir(data, p.id);
    let record = project_root.strip_prefix(data)?.join("project.json");
    let mut result = Removal {
        complete: false,
        deleted_files: 0,
        deleted_bytes: 0,
        remaining: vec![],
        warnings: vec![],
    };
    for f in plan.files.iter().filter(|f| f.relative_path != record) {
        let path = data.join(&f.relative_path);
        if let Err(e) =
            storage::reject_links(&path).and_then(|()| storage::remove_checked(data, &path, f))
        {
            result
                .remaining
                .push(format!("{}: {e}", f.relative_path.display()));
        } else {
            result.deleted_files += 1;
            result.deleted_bytes += f.bytes;
        }
    }
    let mut sorted = plan.directories.clone();
    sorted.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for relative in &sorted {
        let path = data.join(relative);
        if path == project_root {
            continue;
        }
        if let Err(e) = storage::reject_links(&path).and_then(|()| Ok(fs::remove_dir(&path)?)) {
            result
                .remaining
                .push(format!("{}: {e}", relative.display()));
        }
    }
    if result.remaining.is_empty() {
        if let Some(f) = plan.files.iter().find(|f| f.relative_path == record) {
            if let Err(e) = storage::remove_checked(data, &data.join(&record), f) {
                result.remaining.push(e.to_string());
            } else {
                result.deleted_files += 1;
                result.deleted_bytes += f.bytes;
            }
        }
        if result.remaining.is_empty() && project_root.exists() {
            if let Err(e) = storage::reject_links(&project_root)
                .and_then(|()| Ok(fs::remove_dir(&project_root)?))
            {
                result.remaining.push(e.to_string());
            }
        }
    }
    result.complete = result.remaining.is_empty();
    if !result.complete && !data.join(&record).exists() {
        // Preserve only the registry for a visible retry; never recreate media.
        storage::safe_directory(data, record.parent().unwrap())?;
        store::write_json(&data.join(&record), p)?;
    }
    // A small UUID-only receipt also blocks late/stale work from recreating the project.
    store::write_json(
        &marker,
        &json!({"project_id":p.id,"state":if result.complete{"deleted"}else{"pending"},"at":Utc::now(),"deleted_files":result.deleted_files}),
    )?;
    if result.complete {
        let path = data
            .join(AREA)
            .join(format!("plan-{}.json", plan.project_id));
        storage::reject_links(&path)?;
        fs::remove_file(path)?;
    }
    Ok(result)
}

pub async fn delete(
    State(state): State<AppState>,
    RoutePath(id): RoutePath<Uuid>,
    Json(request): Json<DeleteRequest>,
) -> Result<Json<Value>, ApiError> {
    // Same lock order as enqueue. try_write avoids waiting on any reverse-order legacy worker.
    let mut projects = state.projects.write().await;
    let mut jobs = state
        .jobs
        .try_write()
        .map_err(|_| conflict("Coda in aggiornamento: riprova fra un momento."))?;
    let p = projects
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    if request.confirmation != p.title {
        return Err(ApiError::bad_request(
            "Conferma del progetto non corrispondente.",
        ));
    }
    let data = storage::native_canonical(&state.data_dir).map_err(ApiError::internal)?;
    let snapshot = jobs.values().cloned().collect::<Vec<_>>();
    let reasons = blockers(&data, id, &projects, &snapshot).map_err(ApiError::internal)?;
    if !reasons.is_empty() {
        return Err(conflict(reasons.join(" ")));
    }
    let path = data.join(AREA).join(format!("plan-{id}.json"));
    storage::reject_links(&path).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let plan: Plan = store::read_json(&path).map_err(|e| ApiError::bad_request(e.to_string()))?;
    if plan.id != request.plan_id {
        return Err(conflict(
            "Riepilogo sostituito: aggiornalo prima di confermare.",
        ));
    }

    let mut removed = tokio::task::spawn_blocking(move || execute(&data, &p, &plan, &snapshot))
        .await
        .map_err(|e| ApiError::internal(e.into()))?
        .map_err(|e| conflict(e.to_string()))?;
    let mut history_removed = 0;
    if removed.complete {
        projects.remove(&id);
        let before = jobs.len();
        jobs.retain(|_, j| j.project_id != Some(id));
        history_removed = before - jobs.len();
        if let Err(e) =
            store::save_jobs(&state.data_dir, &jobs.values().cloned().collect::<Vec<_>>())
        {
            removed.warnings.push(format!(
                "Progetto eliminato; impossibile aggiornare la cronologia: {e}"
            ));
        }
    }
    Ok(Json(
        json!({"project_id":id,"complete":removed.complete,"deleted_files":removed.deleted_files,"deleted_bytes":removed.deleted_bytes,"remaining":removed.remaining,"warnings":removed.warnings,"jobs_removed":history_removed,"note":"Originali esterni e file condivisi con altri progetti non eliminati. Rimane una piccola ricevuta con UUID e conteggi, senza contenuti o nomi del progetto."}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, Project, Vec<Job>) {
        let data = std::env::temp_dir().join(format!("nl-project-delete-{}", Uuid::new_v4()));
        fs::create_dir_all(&data).unwrap();
        let p = Project::new("Eliminazione QA".into(), None);
        store::save_project(&data, &p).unwrap();
        (storage::native_canonical(&data).unwrap(), p, vec![])
    }
    fn plan(data: &Path, p: &Project, jobs: &[Job]) -> Plan {
        let (files, directories) = inventory(data, p.id, jobs).unwrap();
        Plan {
            id: Uuid::new_v4(),
            project_id: p.id,
            title: p.title.clone(),
            updated_at: p.updated_at,
            data: data.into(),
            expires_at: Utc::now() + Duration::minutes(15),
            files,
            directories,
        }
    }
    fn persist(data: &Path, p: &Plan) {
        let dir = storage::safe_directory(data, Path::new(AREA)).unwrap();
        store::write_json(&dir.join(format!("plan-{}.json", p.project_id)), p).unwrap();
    }
    #[test]
    fn changed_files_refuse_before_removing_anything() {
        let (data, p, jobs) = fixture();
        let path = store::project_dir(&data, p.id).join("assets/a.bin");
        fs::write(&path, b"a").unwrap();
        let old = plan(&data, &p, &jobs);
        persist(&data, &old);
        fs::write(&path, b"changed").unwrap();
        assert!(execute(&data, &p, &old, &jobs).is_err());
        assert!(path.exists());
        assert!(!pending(&data, p.id).exists());
    }
    #[test]
    fn only_owned_names_are_removed_and_references_block() {
        let (data, p, jobs) = fixture();
        let original = data.join("original.bin");
        fs::write(&original, b"original remains").unwrap();
        let inside = store::project_dir(&data, p.id).join("assets/a.bin");
        fs::hard_link(&original, &inside).unwrap();
        let mut other = Project::new("Altro".into(), None);
        other
            .transcript_metadata
            .insert(Uuid::new_v4(), json!({"reference":inside}));
        let map = BTreeMap::from([(p.id, p.clone()), (other.id, other)]);
        assert!(!blockers(&data, p.id, &map, &jobs).unwrap().is_empty());
        let frozen = plan(&data, &p, &jobs);
        persist(&data, &frozen);
        let result = execute(&data, &p, &frozen, &jobs).unwrap();
        assert!(result.complete);
        assert_eq!(fs::read(&original).unwrap(), b"original remains");
        assert!(!store::project_dir(&data, p.id).exists());
        assert!(ensure_ready(&data, p.id).is_err());
    }
    #[test]
    fn active_jobs_block_without_touching_files() {
        let (data, p, _) = fixture();
        let job = Job::queued(JobKind::Pipeline, Some(p.id), "Lavoro attivo");
        assert!(
            !blockers(&data, p.id, &BTreeMap::from([(p.id, p.clone())]), &[job])
                .unwrap()
                .is_empty()
        );
        assert!(store::project_dir(&data, p.id)
            .join("project.json")
            .exists());
    }
}
