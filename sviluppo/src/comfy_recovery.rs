//! Recover a persisted prompt receipt. This module NEVER posts a new /prompt.
use crate::{media, model::*, store, update_project, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::{Path as FsPath, PathBuf},
    time::Duration,
};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub project_id: Option<Uuid>,
    pub comfy_url: String,
    pub prompt_id: String,
    pub placement: Option<GraphicPlacement>,
}
#[derive(Serialize, Deserialize)]
struct Intent {
    receipt: Receipt,
    workflow: Value,
}
pub async fn record_intent(
    state: &AppState,
    job: Uuid,
    base: &str,
    request: &ComfySubmitRequest,
    workflow: &Value,
) -> anyhow::Result<()> {
    let intent = Intent {
        receipt: Receipt {
            schema_version: 1,
            job_id: job,
            project_id: request.project_id,
            comfy_url: base.into(),
            prompt_id: String::new(),
            placement: request.placement.clone(),
        },
        workflow: workflow.clone(),
    };
    let path = receipt_path(state, job).with_file_name("prompt-intent.json");
    store::write_json(&path, &intent)?;
    if let Some(j) = state.jobs.write().await.get_mut(&job) {
        j.result = Some(
            json!({"_nl_suite":{"recoverable":true,"receipt_pending":true,"intent_schema":1}}),
        );
    }
    state.set_job(job,JobStatus::Running,"Invio ComfyUI: intenzione persistita; in caso di esito incerto recupera, non reinviare",vec![]).await;
    Ok(())
}
async fn bounded_json(state: &AppState, url: String) -> anyhow::Result<Value> {
    let mut response = state
        .http
        .get(url)
        .timeout(Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?;
    let mut bytes = vec![];
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            bytes.len() + chunk.len() <= 16 * 1024 * 1024,
            "Risposta recovery oltre16MiB: nessuna nuova generazione inviata"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&bytes)?)
}
fn matching_prompt(row: &Value, intent: &Intent) -> Option<String> {
    let job = intent.receipt.job_id.to_string();
    if row.get(2) != Some(&intent.workflow)
        || row.get(3)?.get("nl_suite_job_id")?.as_str() != Some(&job)
        || row.get(3)?.get("client_id")?.as_str() != Some(&job)
    {
        return None;
    }
    row.get(1)?
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 512)
        .map(str::to_owned)
}
async fn recover_receipt(state: &AppState, id: Uuid) -> Result<Receipt, ApiError> {
    let path = receipt_path(state, id);
    if path.is_file() {
        return store::read_json(&path).map_err(|_| {
            ApiError::bad_request("Ricevuta corrotta: conservata, nessun reinvio automatico")
        });
    }
    let job = state
        .jobs
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Job assente"))?;
    if !matches!(job.kind, JobKind::SubmitComfy) || !matches!(job.status, JobStatus::Failed) {
        return Err(ApiError::bad_request(
            "Il job non è interrotto: nessun recupero concorrente avviato",
        ));
    }
    let intent_path = path.with_file_name("prompt-intent.json");
    let intent:Intent=store::read_json(&intent_path).map_err(|_|ApiError::bad_request("Nessuna ricevuta/intenzione recuperabile. Non reinviare alla cieca un prompt che potrebbe essere attivo."))?;
    if intent.receipt.schema_version != 1
        || intent.receipt.job_id != id
        || intent.receipt.project_id != job.project_id
        || intent.receipt.comfy_url.trim_end_matches('/')
            != state.config().comfy_url.trim_end_matches('/')
    {
        return Err(ApiError::bad_request(
            "Intenzione non associata al job o al server configurato",
        ));
    }
    let base = intent.receipt.comfy_url.trim_end_matches('/');
    let queue = bounded_json(state, format!("{base}/queue"))
        .await
        .map_err(|e| {
            ApiError::bad_request(format!(
                "Verifica invio incerto non disponibile: {e}. Non reinviare."
            ))
        })?;
    let history = bounded_json(state, format!("{base}/history?max_items=200"))
        .await
        .map_err(|e| {
            ApiError::bad_request(format!(
                "History limitata non verificabile: {e}. Intenzione conservata, nessun reinvio."
            ))
        })?;
    let mut ids = HashSet::new();
    for key in ["queue_running", "queue_pending"] {
        for row in queue[key].as_array().into_iter().flatten() {
            if let Some(pid) = matching_prompt(row, &intent) {
                ids.insert(pid);
            }
        }
    }
    for entry in history.as_object().into_iter().flat_map(|o| o.values()) {
        if let Some(pid) = entry
            .get("prompt")
            .and_then(|row| matching_prompt(row, &intent))
        {
            ids.insert(pid);
        }
    }
    if ids.len() != 1 {
        return Err(ApiError::bad_request(format!("Esito invio non univoco: {} prompt verificabili in coda/ultimi200 risultati. Intenzione conservata; controlla ComfyUI, nessun reinvio automatico.",ids.len())));
    }
    let mut receipt = intent.receipt;
    receipt.prompt_id = ids.into_iter().next().expect("one receipt");
    store::write_json(&path, &receipt).map_err(ApiError::internal)?;
    Ok(receipt)
}
fn receipt_path(state: &AppState, id: Uuid) -> PathBuf {
    state
        .data_dir
        .join("generated")
        .join(id.to_string())
        .join("prompt-receipt.json")
}
pub async fn record(
    state: &AppState,
    job_id: Uuid,
    prompt_id: &str,
    base: &str,
    request: &ComfySubmitRequest,
) -> anyhow::Result<()> {
    let receipt = Receipt {
        schema_version: 1,
        job_id,
        project_id: request.project_id,
        comfy_url: base.into(),
        prompt_id: prompt_id.into(),
        placement: request.placement.clone(),
    };
    store::write_json(&receipt_path(state, job_id), &receipt)?;
    if let Some(j) = state.jobs.write().await.get_mut(&job_id) {
        j.result = Some(
            json!({"_nl_suite":{"prompt_id":prompt_id,"recoverable":true,"receipt_schema":1}}),
        );
    }
    state
        .set_job(
            job_id,
            JobStatus::Running,
            format!("ComfyUI prompt {prompt_id}: ricevuta salvata, recuperabile senza rigenerare"),
            vec![],
        )
        .await;
    Ok(())
}
pub fn file_id(job: Uuid, name: &str, sub: &str, kind: &str) -> Uuid {
    Uuid::new_v5(
        &job,
        format!("comfy-output-v1:{name}:{sub}:{kind}").as_bytes(),
    )
}
pub fn asset_path(root: &FsPath, name: &str, id: Uuid) -> anyhow::Result<PathBuf> {
    Ok(root.join("assets").join(format!(
        "{id}-{}",
        media::safe_file_name(FsPath::new(name))?
    )))
}
pub fn describe(root: &FsPath, name: &str, id: Uuid, job: Uuid) -> anyhow::Result<Asset> {
    let path = asset_path(root, name, id)?;
    let kind = media::asset_kind_for(&path);
    let info = if matches!(kind, AssetKind::Video | AssetKind::Audio | AssetKind::Image) {
        media::probe_media(&path)?
    } else {
        None
    };
    Ok(Asset {
        id,
        name: FsPath::new(name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        kind,
        file_name: path.file_name().unwrap().to_string_lossy().into(),
        source_path: format!("ComfyUI job {job}: {name}"),
        imported_at: chrono::Utc::now(),
        media: info,
    })
}
pub async fn recover(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let receipt = recover_receipt(&state, id).await?;
    if receipt.schema_version != 1 || receipt.job_id != id || receipt.prompt_id.is_empty() {
        return Err(ApiError::bad_request("Ricevuta non valida"));
    }
    if receipt.comfy_url.trim_end_matches('/') != state.config().comfy_url.trim_end_matches('/') {
        return Err(ApiError::bad_request("ComfyUI configurato diverso dal server della ricevuta: ripristina l'indirizzo prima di recuperare"));
    }
    let job = {
        let mut jobs = state.jobs.write().await;
        let j = jobs
            .get_mut(&id)
            .ok_or_else(|| ApiError::not_found("Job assente"))?;
        if matches!(j.status, JobStatus::Succeeded) {
            return Ok((StatusCode::ACCEPTED, Json(j.clone())));
        }
        if matches!(j.status, JobStatus::Queued | JobStatus::Running) {
            return Err(ApiError::bad_request(
                "Questo job è già attivo: nessun secondo recupero avviato",
            ));
        }
        if !matches!(j.kind, JobKind::SubmitComfy) || j.project_id != receipt.project_id {
            return Err(ApiError::bad_request("Ricevuta non associata al job"));
        }
        j.status = JobStatus::Queued;
        j.completed_at = None;
        j.message = "Recupero prompt esistente, nessuna nuova generazione".into();
        j.clone()
    };
    state
        .set_job(id, JobStatus::Queued, &job.message, vec![])
        .await;
    let app = state.clone();
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(
            id,
            JobStatus::Running,
            "Recupero ComfyUI: lettura history del prompt originale",
            vec![],
        )
        .await;
        let result = collect(&app, &receipt).await;
        match result {
            Ok((paths, entry)) => {
                if let Some(j) = app.jobs.write().await.get_mut(&id) {
                    j.result = Some(entry);
                }
                app.set_job(id,JobStatus::Succeeded,"Output del prompt originale recuperati; nessuna rigenerazione. Revisiona prima del placement.",paths).await;
            }
            Err(e) => {
                app.set_job(
                    id,
                    JobStatus::Failed,
                    format!("Recupero sospeso: {e}. Ricevuta conservata; non reinviare il prompt."),
                    vec![],
                )
                .await
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}
async fn collect(state: &AppState, r: &Receipt) -> anyhow::Result<(Vec<String>, Value)> {
    let base = r.comfy_url.trim_end_matches('/');
    let started = std::time::Instant::now();
    let mut entry = loop {
        anyhow::ensure!(
            started.elapsed() < Duration::from_secs(7200),
            "History ancora non completata dopo 2 ore"
        );
        let response = state
            .http
            .get(format!("{base}/history/{}", r.prompt_id))
            .timeout(Duration::from_secs(20))
            .send()
            .await;
        let history = match response {
            Ok(v) if v.status().is_server_error() => {
                anyhow::ensure!(
                    started.elapsed() < Duration::from_secs(120),
                    "History non raggiungibile"
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
            Ok(v) => v.error_for_status()?.json::<Value>().await?,
            Err(e) => {
                anyhow::ensure!(started.elapsed() < Duration::from_secs(120), "History: {e}");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        if let Some(e) = history.get(&r.prompt_id) {
            anyhow::ensure!(
                e.pointer("/status/status_str").and_then(Value::as_str) != Some("error"),
                "ComfyUI ha segnalato errore: {}",
                e["status"]
            );
            let complete = e.pointer("/status/completed").and_then(Value::as_bool) == Some(true);
            let queue = state
                .http
                .get(format!("{base}/queue"))
                .send()
                .await?
                .error_for_status()?
                .json::<Value>()
                .await?;
            let present = ["queue_running", "queue_pending"].iter().any(|k| {
                queue[*k].as_array().is_some_and(|v| {
                    v.iter()
                        .any(|row| row.get(1).and_then(Value::as_str) == Some(&r.prompt_id))
                })
            });
            if complete || (!present && e["outputs"].as_object().is_some_and(|o| !o.is_empty())) {
                break e.clone();
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    };
    let root = r
        .project_id
        .map(|id| store::project_dir(&state.data_dir, id));
    let directory = state.data_dir.join("generated").join(r.job_id.to_string());
    tokio::fs::create_dir_all(&directory).await?;
    let outputs = entry["outputs"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Prompt senza output scaricabili"))?;
    let mut seen = HashSet::new();
    let mut assets = vec![];
    let mut paths = vec![];
    for node in outputs.values() {
        for collection in node
            .as_object()
            .into_iter()
            .flat_map(|o| o.values())
            .filter_map(Value::as_array)
        {
            for f in collection {
                let Some(name) = f["filename"].as_str() else {
                    continue;
                };
                let sub = f["subfolder"].as_str().unwrap_or("");
                let kind = f["type"].as_str().unwrap_or("output");
                let aid = file_id(r.job_id, name, sub, kind);
                if !seen.insert(aid) {
                    continue;
                }
                let safe = media::safe_file_name(FsPath::new(name))?;
                let target = if let Some(root) = &root {
                    asset_path(root, name, aid)?
                } else {
                    directory.join(format!("{aid}-{safe}"))
                };
                if !target.is_file() {
                    let temporary = directory.join(format!("{aid}.part"));
                    let mut output = tokio::fs::File::create(&temporary).await?;
                    let mut response = state
                        .http
                        .get(format!("{base}/view"))
                        .query(&[("filename", name), ("subfolder", sub), ("type", kind)])
                        .timeout(Duration::from_secs(600))
                        .send()
                        .await?
                        .error_for_status()?;
                    while let Some(b) = response.chunk().await? {
                        output.write_all(&b).await?;
                    }
                    output.flush().await?;
                    drop(output);
                    if let Some(parent) = target.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                    }
                    tokio::fs::rename(temporary, &target).await?;
                }
                if let Some(root) = &root {
                    let root = root.clone();
                    let name = name.to_owned();
                    let job = r.job_id;
                    assets.push(
                        tokio::task::spawn_blocking(move || describe(&root, &name, aid, job))
                            .await??,
                    );
                }
                paths.push(target.to_string_lossy().into_owned());
            }
        }
    }
    anyhow::ensure!(!paths.is_empty(), "Output senza file media");
    if let Some(pid) = r.project_id {
        update_project(state, pid, |p| {
            for a in &assets {
                if !p.assets.iter().any(|v| v.id == a.id) {
                    p.assets.push(a.clone());
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!(e.1))?;
    }
    entry["_nl_suite"] = json!({"asset_ids":assets.iter().map(|a|a.id).collect::<Vec<_>>(),"prompt_id":r.prompt_id,"recoverable":true,"recovered":true,"regenerated":false});
    if let (Some(placement), Some(asset)) = (
        &r.placement,
        assets.iter().find(|a| a.kind == AssetKind::Image),
    ) {
        entry["_nl_suite"]["pending_placement"] = json!(placement);
        entry["_nl_suite"]["asset_id"] = json!(asset.id);
        entry["_nl_suite"]["approved"] = json!(false);
    }
    Ok((paths, entry))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uncertain_receipt_requires_owned_client_marker_and_identical_workflow() {
        let id = Uuid::new_v4();
        let intent = Intent {
            receipt: Receipt {
                schema_version: 1,
                job_id: id,
                project_id: None,
                comfy_url: "http://127.0.0.1:9".into(),
                prompt_id: String::new(),
                placement: None,
            },
            workflow: json!({"save":{"class_type":"SaveImage","inputs":{"filename_prefix":"test"}}}),
        };
        let row =
            json!([0,"original",intent.workflow,{"client_id":id,"nl_suite_job_id":id},["save"]]);
        assert_eq!(matching_prompt(&row, &intent), Some("original".into()));
        let mut foreign = row.clone();
        foreign[3]["client_id"] = json!(Uuid::new_v4());
        assert!(matching_prompt(&foreign, &intent).is_none());
        foreign = row.clone();
        foreign[2]["save"]["inputs"]["filename_prefix"] = json!("different");
        assert!(matching_prompt(&foreign, &intent).is_none());
        assert!(matching_prompt(&json!([0, "original"]), &intent).is_none());
    }
    #[test]
    fn output_identity_includes_remote_subfolder_and_is_repeatable() {
        let j = Uuid::new_v4();
        let a = file_id(j, "image.png", "a", "output");
        assert_eq!(a, file_id(j, "image.png", "a", "output"));
        assert_ne!(a, file_id(j, "image.png", "b", "output"));
    }
}
