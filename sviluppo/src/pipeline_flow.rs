//! Persistent, review-gated long-video production flow. The existing bridge
//! performs ASR/alignment before analysis; this layer keeps subsequent steps
//! together without treating an AI proposal as an editorial approval.
use crate::{editor, graphics, model::*, pipeline, store, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::OnceLock};
use tokio::sync::Mutex;
use uuid::Uuid;

static MUTATION: OnceLock<Mutex<()>> = OnceLock::new();
fn mutation() -> &'static Mutex<()> {
    MUTATION.get_or_init(|| Mutex::new(()))
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Flow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub asset_id: Uuid,
    pub operation: String,
    pub created_at: DateTime<Utc>,
    pub analysis_job_id: Uuid,
    #[serde(default)]
    pub applied_clip_ids: Vec<Uuid>,
    #[serde(default)]
    pub graphics_job_id: Option<Uuid>,
    #[serde(default)]
    pub export_job_id: Option<Uuid>,
    #[serde(default)]
    pub skipped_graphics: Vec<Uuid>,
    #[serde(default)]
    pub warning: Option<String>,
}

#[derive(Deserialize)]
pub struct StartRequest {
    pub asset_id: Uuid,
    pub operation: String,
    #[serde(default = "italian")]
    pub language: String,
    #[serde(default)]
    pub bad_takes: bool,
}
fn italian() -> String {
    "it".into()
}

#[derive(Deserialize)]
pub struct ApplyRequest {
    pub items: Vec<Value>,
    pub expected_updated_at: DateTime<Utc>,
    #[serde(default)]
    pub auto_graphics: bool,
    pub workflow_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub struct GraphicsRequest {
    pub workflow_id: Uuid,
}
#[derive(Deserialize)]
pub struct ReviewRequest {
    pub job_id: Uuid,
    #[serde(default)]
    pub skip: bool,
}

fn flow_path(state: &AppState, id: Uuid, flow: Uuid) -> PathBuf {
    store::project_dir(&state.data_dir, id)
        .join("flows")
        .join(format!("{flow}.json"))
}
fn read_flow(state: &AppState, id: Uuid, flow_id: Uuid) -> Result<Flow, ApiError> {
    let path = flow_path(state, id, flow_id);
    if !path.is_file() {
        return Err(ApiError::not_found("Percorso guidato assente"));
    }
    let flow: Flow = store::read_json(&path).map_err(ApiError::internal)?;
    if flow.project_id != id || flow.id != flow_id {
        return Err(ApiError::bad_request("Percorso di un altro progetto"));
    }
    Ok(flow)
}
fn save_flow(state: &AppState, flow: &Flow) -> Result<(), ApiError> {
    let path = flow_path(state, flow.project_id, flow.id);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| ApiError::internal(e.into()))?;
    store::write_json(&path, flow).map_err(ApiError::internal)
}

fn generation_ids(job: Option<&Job>) -> Vec<Uuid> {
    job.and_then(|j| j.result.as_ref())
        .and_then(|v| v["generation_jobs"].as_array())
        .map(|jobs| {
            jobs.iter()
                .filter_map(|j| serde_json::from_value(j["id"].clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

async fn view(state: &AppState, flow: Flow) -> Value {
    let jobs = state.jobs.read().await;
    let analysis = jobs.get(&flow.analysis_job_id);
    let graphics_job = flow.graphics_job_id.and_then(|id| jobs.get(&id));
    let generations = generation_ids(graphics_job)
        .iter()
        .filter_map(|id| jobs.get(id).cloned())
        .collect::<Vec<_>>();
    let export = flow.export_job_id.and_then(|id| jobs.get(&id));
    let (phase, message) = if let Some(job) = export {
        match job.status {
            JobStatus::Succeeded => ("complete", "Export pronto"),
            JobStatus::Failed => ("export_failed", "Export non riuscito: correggi e riprova"),
            _ => ("exporting", "Esportazione del montaggio"),
        }
    } else if let Some(job) = graphics_job {
        match job.status {
            JobStatus::Failed => (
                "graphics_failed",
                "Grafiche non riuscite: puoi riprovare o esportare senza immagini",
            ),
            JobStatus::Queued | JobStatus::Running => (
                "graphics",
                "Analisi del parlato e generazione delle grafiche",
            ),
            JobStatus::Succeeded => {
                if generations
                    .iter()
                    .any(|j| matches!(j.status, JobStatus::Queued | JobStatus::Running))
                {
                    ("graphics", "Generazione immagini in sequenza")
                } else if generations.iter().any(|j| {
                    matches!(j.status, JobStatus::Succeeded)
                        && !flow.skipped_graphics.contains(&j.id)
                        && j.result
                            .as_ref()
                            .is_some_and(|v| v.pointer("/_nl_suite/approved") != Some(&json!(true)))
                }) {
                    (
                        "review_graphics",
                        "Revisiona le immagini: approva oppure escludi",
                    )
                } else {
                    (
                        "ready",
                        "Montaggio pronto: puoi esportare o rifinire nella timeline",
                    )
                }
            }
        }
    } else if !flow.applied_clip_ids.is_empty() {
        ("ready", "Tagli approvati e inseriti; grafiche facoltative")
    } else {
        match analysis.map(|j| &j.status) {
            Some(JobStatus::Succeeded) => (
                "review_cuts",
                "Analisi terminata: revisiona i tagli prima di continuare",
            ),
            Some(JobStatus::Failed) | None => (
                "analysis_failed",
                "Analisi non riuscita: il dettaglio è qui sotto",
            ),
            _ => (
                "analyzing",
                "Trascrizione, allineamento italiano e proposte: passaggi automatici",
            ),
        }
    };
    json!({"flow":flow,"phase":phase,"message":message,"analysis_job":analysis,"graphics_job":graphics_job,"generation_jobs":generations,"export_job":export})
}

pub async fn list(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if !state.projects.read().await.contains_key(&id) {
        return Err(ApiError::not_found("Progetto assente"));
    }
    let _guard = mutation().lock().await;
    let directory = store::project_dir(&state.data_dir, id).join("flows");
    let mut flows = vec![];
    if directory.exists() {
        for entry in std::fs::read_dir(directory).map_err(|e| ApiError::internal(e.into()))? {
            let entry = entry.map_err(|e| ApiError::internal(e.into()))?;
            if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let flow: Flow = store::read_json(&entry.path()).map_err(ApiError::internal)?;
            if flow.project_id == id {
                flows.push(flow);
            }
        }
    }
    flows.sort_by_key(|flow| std::cmp::Reverse(flow.created_at));
    let mut result = Vec::new();
    for flow in flows.into_iter().take(50) {
        result.push(view(&state, flow).await);
    }
    Ok(Json(json!(result)))
}

pub async fn start(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<StartRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if !matches!(
        request.operation.as_str(),
        "cleanup" | "clips" | "reels" | "shorts"
    ) {
        return Err(ApiError::bad_request(
            "Scegli sgrossatura, clip, reel o shorts",
        ));
    }
    if !["it", "en", "es", "auto"].contains(&request.language.as_str()) {
        return Err(ApiError::bad_request(
            "Lingua non supportata dagli adapter ASR installati",
        ));
    }
    let _guard = mutation().lock().await;
    let (_, Json(job)) = pipeline::run_pipeline(
        State(state.clone()),
        Path(id),
        Json(pipeline::PipelineRequest {
            asset_id: request.asset_id,
            operation: request.operation.clone(),
            language: request.language,
            params: json!({"ENABLE_AI_BAD_TAKES":request.bad_takes}),
            items: vec![],
            burn_subs: false,
            vertical: true,
            mode: "2".into(),
        }),
    )
    .await?;
    let flow = Flow {
        id: Uuid::new_v4(),
        project_id: id,
        asset_id: request.asset_id,
        operation: request.operation,
        created_at: Utc::now(),
        analysis_job_id: job.id,
        applied_clip_ids: vec![],
        graphics_job_id: None,
        export_job_id: None,
        skipped_graphics: vec![],
        warning: None,
    };
    save_flow(&state, &flow)?;
    Ok((StatusCode::ACCEPTED, Json(view(&state, flow).await)))
}

fn ranges(items: &[Value]) -> Result<Vec<(u64, u64, String)>, ApiError> {
    if items.is_empty() || items.len() > 200 {
        return Err(ApiError::bad_request("Seleziona da 1 a 200 proposte"));
    }
    let mut result = vec![];
    for (i, item) in items.iter().enumerate() {
        let pairs = item
            .get("segments")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| vec![json!([item["start"], item["end"]])]);
        for pair in pairs {
            let a = pair[0]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0.0)
                .ok_or_else(|| ApiError::bad_request("Inizio intervallo non valido"))?;
            let b = pair[1]
                .as_f64()
                .filter(|v| v.is_finite() && *v > a && *v <= 604_800.0)
                .ok_or_else(|| ApiError::bad_request("Fine intervallo non valida"))?;
            let start = (a * 1000.0).round() as u64;
            let end = (b * 1000.0).round() as u64;
            if end <= start {
                return Err(ApiError::bad_request("Intervallo inferiore a 1 ms"));
            }
            result.push((
                start,
                end,
                item["title"]
                    .as_str()
                    .map(|s| s.chars().take(160).collect())
                    .unwrap_or_else(|| format!("Proposta {}", i + 1)),
            ));
            if result.len() > 400 {
                return Err(ApiError::bad_request(
                    "Massimo 400 intervalli per montaggio guidato",
                ));
            }
        }
    }
    if result.is_empty() {
        return Err(ApiError::bad_request(
            "Le proposte non contengono intervalli",
        ));
    }
    Ok(result)
}

async fn graphics_inner(
    state: &AppState,
    flow: &mut Flow,
    workflow_id: Uuid,
) -> Result<(), ApiError> {
    if flow.applied_clip_ids.is_empty() {
        return Err(ApiError::bad_request("Approva prima i tagli"));
    }
    if let Some(id) = flow.graphics_job_id {
        let jobs = state.jobs.read().await;
        if jobs
            .get(&id)
            .is_some_and(|j| !matches!(j.status, JobStatus::Failed))
        {
            return Err(ApiError::bad_request(
                "Analisi grafica già avviata per questo percorso",
            ));
        }
    }
    let (_, Json(job)) = graphics::suggest(
        State(state.clone()),
        Path(flow.project_id),
        Json(graphics::SuggestRequest {
            window: None,
            execution: None,
            auto_generate: true,
            workflow_id: Some(workflow_id),
            creative_brief: None,
            review_before_insert: true,
            clip_ids: flow.applied_clip_ids.clone(),
        }),
    )
    .await?;
    flow.graphics_job_id = Some(job.id);
    flow.warning = None;
    save_flow(state, flow)
}

pub async fn apply(
    State(state): State<AppState>,
    Path((id, flow_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<ApplyRequest>,
) -> Result<Json<Value>, ApiError> {
    let _guard = mutation().lock().await;
    let mut flow = read_flow(&state, id, flow_id)?;
    if !flow.applied_clip_ids.is_empty() {
        return Ok(Json(view(&state, flow).await));
    }
    if !state
        .jobs
        .read()
        .await
        .get(&flow.analysis_job_id)
        .is_some_and(|job| matches!(job.status, JobStatus::Succeeded))
    {
        return Err(ApiError::bad_request(
            "Attendi il completamento dell'analisi prima di approvare",
        ));
    }
    if request.auto_graphics && request.workflow_id.is_none() {
        return Err(ApiError::bad_request(
            "Scegli un workflow immagine oppure disattiva le grafiche",
        ));
    }
    let selected = ranges(&request.items)?;
    let mut projects = state.projects.write().await;
    let project = projects
        .get(&id)
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    if request.expected_updated_at != project.updated_at {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Il progetto è cambiato: aggiorna lo stato prima di approvare".into(),
        ));
    }
    let asset = project
        .assets
        .iter()
        .find(|a| a.id == flow.asset_id)
        .ok_or_else(|| ApiError::not_found("Media sorgente assente"))?;
    let kind = if asset.kind == AssetKind::Audio {
        TrackKind::Audio
    } else {
        TrackKind::Video
    };
    let track = project
        .timeline
        .tracks
        .iter()
        .find(|t| t.kind == kind && !t.locked)
        .ok_or_else(|| {
            ApiError::bad_request("Sblocca la traccia compatibile prima di continuare")
        })?;
    let mut next = project.clone();
    let mut cursor = next
        .timeline
        .clips
        .iter()
        .map(|c| c.start_ms.saturating_add(c.duration_ms))
        .max()
        .unwrap_or(0);
    for (i, (start, end, label)) in selected.into_iter().enumerate() {
        let clip_id = Uuid::new_v5(&flow.id, format!("cut-{i}").as_bytes());
        // Recover safely if the app stopped after saving the project but before
        // saving its flow record. Stable IDs prevent duplicate insertion.
        if let Some(old) = next.timeline.clips.iter().find(|c| c.id == clip_id) {
            if old.asset_id != flow.asset_id
                || old.trim_in_ms != start
                || old.duration_ms != end - start
            {
                return Err(ApiError::bad_request(
                    "Approvazione precedente differente: crea un nuovo percorso",
                ));
            }
            flow.applied_clip_ids.push(clip_id);
            continue;
        }
        next.timeline.clips.push(TimelineClip {
            id: clip_id,
            asset_id: flow.asset_id,
            track_id: track.id,
            start_ms: cursor,
            trim_in_ms: start,
            duration_ms: end - start,
            label: Some(label),
            volume: 1.0,
            opacity: 1.0,
            transform: VisualTransform::default(),
        });
        cursor = cursor
            .checked_add(end - start)
            .ok_or_else(|| ApiError::bad_request("Durata totale troppo grande"))?;
        flow.applied_clip_ids.push(clip_id);
    }
    editor::validate_timeline(&next, &next.timeline)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    next.updated_at = Utc::now();
    state.save_project(&next).await?;
    projects.insert(id, next);
    drop(projects);
    save_flow(&state, &flow)?;
    if request.auto_graphics {
        if let Err(error) = graphics_inner(&state, &mut flow, request.workflow_id.unwrap()).await {
            flow.warning = Some(format!("Tagli salvati. Grafiche non avviate: {}", error.1));
            save_flow(&state, &flow)?;
        }
    }
    Ok(Json(view(&state, flow).await))
}

pub async fn start_graphics(
    State(state): State<AppState>,
    Path((id, flow_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<GraphicsRequest>,
) -> Result<Json<Value>, ApiError> {
    let _guard = mutation().lock().await;
    let mut flow = read_flow(&state, id, flow_id)?;
    graphics_inner(&state, &mut flow, request.workflow_id).await?;
    Ok(Json(view(&state, flow).await))
}

pub async fn review(
    State(state): State<AppState>,
    Path((id, flow_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<ReviewRequest>,
) -> Result<Json<Value>, ApiError> {
    let _guard = mutation().lock().await;
    let mut flow = read_flow(&state, id, flow_id)?;
    let jobs = state.jobs.read().await;
    if !generation_ids(flow.graphics_job_id.and_then(|j| jobs.get(&j))).contains(&request.job_id) {
        return Err(ApiError::bad_request("Immagine estranea al percorso"));
    }
    if !jobs
        .get(&request.job_id)
        .is_some_and(|j| matches!(j.status, JobStatus::Succeeded))
    {
        return Err(ApiError::bad_request(
            "L'immagine non è pronta per la revisione",
        ));
    }
    drop(jobs);
    if request.skip {
        if !flow.skipped_graphics.contains(&request.job_id) {
            flow.skipped_graphics.push(request.job_id);
        }
    } else {
        let _ = graphics::approve(
            State(state.clone()),
            Path(id),
            Json(graphics::ApprovalRequest {
                job_id: request.job_id,
            }),
        )
        .await?;
        flow.skipped_graphics.retain(|j| *j != request.job_id);
    }
    save_flow(&state, &flow)?;
    Ok(Json(view(&state, flow).await))
}

pub async fn export(
    State(state): State<AppState>,
    Path((id, flow_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    let _guard = mutation().lock().await;
    let mut flow = read_flow(&state, id, flow_id)?;
    let current = view(&state, flow.clone()).await;
    if !matches!(
        current["phase"].as_str(),
        Some("ready" | "graphics_failed" | "export_failed" | "complete")
    ) {
        return Err(ApiError::bad_request(
            "Completa prima l'analisi e le revisioni: non vengono esportate immagini non approvate",
        ));
    }
    let (_, Json(job)) = crate::export_timeline(State(state.clone()), Path(id.to_string())).await?;
    flow.export_job_id = Some(job.id);
    save_flow(&state, &flow)?;
    Ok(Json(view(&state, flow).await))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_accept_disjoint_ranges_but_never_invalid_time() {
        let parsed = ranges(&[json!({"title":"Reel","segments":[[1.2,2.5],[4,5]]})]).unwrap();
        assert_eq!(parsed[0], (1200, 2500, "Reel".into()));
        assert_eq!(parsed[1].0, 4000);
        for item in [
            json!({"start":-1,"end":2}),
            json!({"start":3,"end":2}),
            json!({"segments":[]}),
            json!({"segments":[[1,1.0001]]}),
        ] {
            assert!(ranges(&[item]).is_err());
        }
    }
    #[test]
    fn segment_limit_is_bounded() {
        assert!(ranges(&vec![json!({"start":0,"end":1}); 201]).is_err());
        assert!(ranges(&[json!({"segments":vec![json!([0,1]);401]})]).is_err());
    }
}
