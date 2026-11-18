//! Bounded A/B render of a proposed rough cut without applying it to the project.
use crate::{compositor, longform, model::*, store, ApiError, AppState};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    response::Response,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewRequest {
    pub asset_id: Uuid,
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub expected_revision: u64,
    #[serde(default)]
    pub at_source_ms: u64,
}
pub async fn render(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<PreviewRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let mut p = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    if p.updated_at != req.expected_updated_at || p.longform.revision != req.expected_revision {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Proposta cambiata: aggiorna il confronto prima dell'anteprima A/B".into(),
        ));
    }
    let (timeline, origins) = longform::proposal(&p, req.asset_id)?;
    let own = timeline
        .clips
        .iter()
        .filter(|c| {
            origins
                .get(&c.id)
                .is_some_and(|o| o.producer == "longform" && o.asset_id == req.asset_id)
        })
        .collect::<Vec<_>>();
    let at = own
        .iter()
        .filter(|c| c.trim_in_ms >= req.at_source_ms)
        .min_by_key(|c| c.trim_in_ms)
        .map(|c| c.start_ms)
        .or_else(|| own.last().map(|c| c.start_ms))
        .unwrap_or(0);
    let end = timeline
        .clips
        .iter()
        .map(|c| c.start_ms + c.duration_ms)
        .max()
        .unwrap_or(0);
    let window_start = at.saturating_sub(6000).min(end.saturating_sub(18000));
    let window_end = (window_start + 18000).min(end);
    let first_source = own
        .iter()
        .find(|c| c.start_ms + c.duration_ms > window_start)
        .map(|c| c.trim_in_ms + window_start.saturating_sub(c.start_ms))
        .unwrap_or(0);
    let key = Uuid::new_v5(
        &p.id,
        format!(
            "rough-preview-v3-all-motion-origin:{}:{}:{}:{}",
            p.updated_at, req.asset_id, req.expected_revision, req.at_source_ms
        )
        .as_bytes(),
    );
    p.timeline = timeline;
    p = compositor::window(&p, window_start, window_end);
    let root = store::project_dir(&state.data_dir, id);
    let directory = root.join(".cache/previews/longform");
    std::fs::create_dir_all(&directory).map_err(|e| ApiError::internal(e.into()))?;
    let filename = format!("rough-{key}.mp4");
    let destination = directory.join(&filename);
    let job = Job::queued(
        JobKind::PreparePreview,
        Some(id),
        "Anteprima proposta A/B in coda · massimo 18 secondi",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let jid = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(
            jid,
            JobStatus::Running,
            "Render anteprima del raccordo proposto",
            vec![],
        )
        .await;
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            if destination.is_file() {
                return Ok(());
            }
            let layers = compositor::manifest(&p, &root)?;
            let (w, h) = compositor::dimensions(&p.aspect_ratio)?;
            let factor = (640_f64 / f64::from(w)).min(640_f64 / f64::from(h));
            let size = (
                ((f64::from(w) * factor / 2.).round() as u32) * 2,
                ((f64::from(h) * factor / 2.).round() as u32) * 2,
            );
            let temporary = directory.join(format!("rough-{key}-{jid}.mp4"));
            compositor::render_project(
                &p,
                &root,
                &layers,
                &temporary,
                size,
                crate::social::render_options(&p),
            )?;
            std::fs::rename(temporary, destination)?;
            Ok(())
        })
        .await;
        match result {
            Ok(Ok(())) => {
                if let Some(j) = app.jobs.write().await.get_mut(&jid) {
                    j.result = Some(
                        json!({"preview_file":filename,"source_start_hint_ms":first_source,"timeline_start_ms":window_start,"duration_ms":window_end-window_start,"proposal_revision":req.expected_revision,"applied":false}),
                    );
                }
                app.set_job(
                    jid,
                    JobStatus::Succeeded,
                    "Anteprima A/B pronta: confronta sorgente e proposta prima di applicare",
                    vec![],
                )
                .await;
            }
            Ok(Err(e)) => {
                app.set_job(jid, JobStatus::Failed, e.to_string(), vec![])
                    .await
            }
            Err(e) => {
                app.set_job(jid, JobStatus::Failed, e.to_string(), vec![])
                    .await
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}
pub async fn file(
    State(state): State<AppState>,
    Path((id, name)): Path<(Uuid, String)>,
    request: Request<Body>,
) -> Result<Response, ApiError> {
    if !state.projects.read().await.contains_key(&id) {
        return Err(ApiError::not_found("Progetto assente"));
    }
    let token = name
        .strip_prefix("rough-")
        .and_then(|s| s.strip_suffix(".mp4"))
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| ApiError::bad_request("Anteprima non valida"))?;
    crate::editor::serve_file(
        store::project_dir(&state.data_dir, id)
            .join(".cache/previews/longform")
            .join(format!("rough-{token}.mp4")),
        request,
    )
    .await
}
