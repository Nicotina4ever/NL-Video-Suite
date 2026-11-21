mod album;
mod alpha_render;
mod asr;
mod asr_catalog;
mod asr_store;
mod audio_mixer;
mod caption_core;
mod comfy_recovery;
mod compositor;
mod creative_montage;
mod edit_versions;
mod editor;
mod editorial_qa;
mod export_encoding;
mod fonts;
mod generation;
mod graphic_objects;
mod graphics;
mod legacy;
mod longform;
mod longform_analysis;
mod longform_preview;
mod media;
mod migration;
mod model;
mod montage_audio;
mod montage_planner;
mod motion;
mod narrative;
mod narrative_workflows;
mod pipeline;
mod pipeline_flow;
mod playback;
mod previews;
mod production_package;
mod production_preset;
mod project_index;
mod project_management;
mod providers;
mod reframe;
mod resources;
mod runtime;
mod services;
mod settings;
mod shorts_studio;
mod shot_map;
mod social;
mod storage;
mod store;
mod subtitle_alignment;
mod subtitle_history;
mod subtitle_studio;
mod thumbnail;
mod time_map;
mod transcript_assistant;
mod transcript_core;
mod waveform;
mod web;
mod workflow_controls;
mod workflow_doctor;
mod workflow_import;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use model::{
    AddTimelineClipRequest, AppConfig, Asset, AssetKind, CreateProjectRequest, ExtractClipRequest,
    ImportAssetRequest, Job, JobKind, JobStatus, Project, SplitClipRequest, TimelineClip,
    TrackKind, WorkflowScanRequest, WorkflowSummary,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    net::TcpListener,
    sync::{RwLock, Semaphore},
};
use tower_http::trace::TraceLayer;
use tracing::{error, info};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    data_dir: Arc<PathBuf>,
    config: Arc<std::sync::RwLock<AppConfig>>,
    projects: Arc<RwLock<BTreeMap<Uuid, Project>>>,
    jobs: Arc<RwLock<BTreeMap<Uuid, Job>>>,
    workflows: Arc<RwLock<Vec<WorkflowSummary>>>,
    http: reqwest::Client,
    worker: Arc<Semaphore>,
}

impl AppState {
    /// Never hold a std lock across await; each operation uses one snapshot.
    fn config(&self) -> AppConfig {
        self.config
            .read()
            .expect("configuration lock poisoned")
            .clone()
    }

    async fn add_job(&self, job: Job) -> Result<(), ApiError> {
        // Hold the project read guard through enqueue: deletion takes the write
        // guard, so a request prepared before deletion cannot resurrect work.
        let projects = self.projects.read().await;
        if let Some(id) = job.project_id {
            if !projects.contains_key(&id) {
                return Err(ApiError::not_found("Progetto eliminato o assente"));
            }
            project_management::ensure_ready(&self.data_dir, id)?;
        }
        let mut jobs = self.jobs.write().await;
        jobs.insert(job.id, job);
        let snapshot = jobs.values().cloned().collect::<Vec<_>>();
        if let Err(error) = store::save_jobs(&self.data_dir, &snapshot) {
            error!(%error, "could not persist job list");
        }
        Ok(())
    }

    async fn set_job(
        &self,
        id: Uuid,
        status: JobStatus,
        message: impl Into<String>,
        output_paths: Vec<String>,
    ) {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(&id) {
            job.status = status;
            job.message = message.into();
            job.output_paths = output_paths;
            match status {
                JobStatus::Running => {
                    job.started_at.get_or_insert_with(Utc::now);
                }
                JobStatus::Succeeded | JobStatus::Failed => job.completed_at = Some(Utc::now()),
                JobStatus::Queued => {}
            }
        }
        let snapshot = jobs.values().cloned().collect::<Vec<_>>();
        if let Err(error) = store::save_jobs(&self.data_dir, &snapshot) {
            error!(%error, "could not persist job list");
        }
    }

    async fn save_project(&self, project: &Project) -> Result<(), ApiError> {
        project_management::ensure_ready(&self.data_dir, project.id)?;
        edit_versions::ensure_ready(&self.data_dir, project.id)?;
        store::save_project(&self.data_dir, project).map_err(ApiError::internal)
    }
}

#[derive(Debug)]
struct ApiError(StatusCode, String);
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.1.fmt(f)
    }
}
impl std::error::Error for ApiError {}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, message.into())
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self(StatusCode::NOT_FOUND, message.into())
    }

    fn internal(error: anyhow::Error) -> Self {
        error!(%error, "internal API error");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal application error".to_owned(),
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}

#[derive(Serialize)]
struct Health {
    application: &'static str,
    version: &'static str,
    ffmpeg: bool,
    comfyui: bool,
    ollama: bool,
    llm: bool,
    llm_provider: model::LlmProvider,
    installation_root: PathBuf,
    data_dir: PathBuf,
    data_warnings: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    runtime::initialize_environment()?;
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let (data_dir, port) = parse_arguments()?;
    std::fs::create_dir_all(&data_dir)?;
    let data_dir = data_dir.canonicalize()?;
    let data_lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(data_dir.join("server.lock"))?;
    data_lock.try_lock().map_err(|e| {
        anyhow::anyhow!("Le cartelle dati sono già aperte da un'altra istanza: {e}")
    })?;
    std::fs::write(data_dir.join("server.pid"), std::process::id().to_string())?;
    let config = store::load_or_create_config(&data_dir)?;
    match store::migrate_project_folders(&data_dir) {
        Ok(items) if !items.is_empty() => {
            tracing::info!(
                "{} cartelle progetto rinominate in modo leggibile (ricevuta in data/project-folders-migration.json)",
                items.len()
            );
        }
        Ok(_) => {}
        Err(error) => tracing::warn!("Migrazione cartelle progetto non completata: {error}"),
    }
    let projects = store::load_projects(&data_dir)?;
    let mut reconciled = false;
    let jobs = store::load_jobs(&data_dir)?
        .into_iter()
        .map(|mut job| {
            if matches!(job.status, JobStatus::Queued | JobStatus::Running) {
                let previous = std::mem::take(&mut job.message);
                job.status = JobStatus::Failed;
                job.completed_at = Some(Utc::now());
                job.message =
                    format!("Interrotto dal riavvio: rilancia il lavoro. Prima fase: {previous}");
                if job.result.as_ref().is_some_and(|r|r["_nl_suite"]["prompt_id"].is_string()) {job.message=format!("Interrotto dal riavvio: recupera il prompt originale, non rigenerare. Prima fase: {previous}");}
                reconciled = true;
            }
            (job.id, job)
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    if reconciled {
        // Persist immediately: the in-memory result used to be lost on the next
        // start, leaving jobs.json frozen at "running" for the whole session.
        store::save_jobs(&data_dir, &jobs.values().cloned().collect::<Vec<_>>())?;
    }
    let mut workflows = store::load_workflows(&data_dir)?;
    let localized = runtime::localize_workflows(&config, &mut workflows);
    if runtime::portable_workflow_paths(&mut workflows) || localized {
        store::save_workflows(&data_dir, &workflows)?;
    }
    if workflows.is_empty() {
        match media::scan_workflows(&default_workflow_roots(&config, &data_dir)) {
            Ok(found) => {
                workflows = found;
                store::save_workflows(&data_dir, &workflows)?;
            }
            Err(error) => {
                tracing::warn!(%error,"Catalogo iniziale non disponibile; riprova dalla UI")
            }
        }
    }
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;
    let state = AppState {
        data_dir: Arc::new(data_dir),
        config: Arc::new(std::sync::RwLock::new(config)),
        projects: Arc::new(RwLock::new(projects)),
        jobs: Arc::new(RwLock::new(jobs)),
        workflows: Arc::new(RwLock::new(workflows)),
        http,
        worker: Arc::new(Semaphore::new(1)),
    };

    // Warm the derived search indexes in the background: boot never waits for
    // this sweep, and searches rebuild on demand if it did not complete.
    {
        let data = state.data_dir.clone();
        let projects = state.projects.clone();
        tokio::spawn(async move {
            let list = projects.read().await.values().cloned().collect::<Vec<_>>();
            for project in list {
                let data = (*data).clone();
                let _ = tokio::task::spawn_blocking(move || {
                    project_index::rebuild_if_stale(&data, &project)
                })
                .await;
            }
        });
    }

    let app = Router::new()
        .route("/", get(web::index))
        .route("/api/health", get(health))
        .route(
            "/api/v1/resources",
            get(resources::view).put(resources::save),
        )
        .route("/api/integrations", get(integrations))
        .route("/api/runtime", get(runtime::status))
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/{id}",
            get(get_project).delete(project_management::delete),
        )
        .route(
            "/api/projects/{id}/title",
            axum::routing::put(rename_project),
        )
        .route(
            "/api/projects/{id}/delete-preview",
            post(project_management::preview),
        )
        .route(
            "/api/projects/{id}/edit-versions",
            get(edit_versions::list).post(edit_versions::create),
        )
        .route(
            "/api/projects/{id}/edit-versions/{version}",
            get(edit_versions::compare),
        )
        .route(
            "/api/projects/{id}/edit-versions/{version}/name",
            post(edit_versions::rename),
        )
        .route(
            "/api/projects/{id}/edit-versions/{version}/restore",
            post(edit_versions::restore),
        )
        .route(
            "/api/projects/{id}/creative-brief",
            axum::routing::put(editor::save_creative_brief),
        )
        .route("/api/projects/{id}/assets", post(import_asset))
        .route(
            "/api/projects/{id}/graphic-objects",
            post(graphic_objects::create),
        )
        .route(
            "/api/projects/{id}/graphic-objects/{object_id}",
            axum::routing::put(graphic_objects::update).delete(graphic_objects::delete),
        )
        .route(
            "/api/projects/{id}/graphic-objects/{object_id}/describe",
            post(graphic_objects::describe),
        )
        .route(
            "/api/projects/{id}/graphic-objects/{object_id}/generate",
            post(graphic_objects::generate),
        )
        .route(
            "/api/projects/{id}/graphic-objects/{object_id}/approve",
            post(graphic_objects::approve),
        )
        .route(
            "/api/projects/{id}/assets/upload",
            post(editor::upload_asset).layer(axum::extract::DefaultBodyLimit::disable()),
        )
        .route("/api/projects/{id}/legacy-import", post(migration::import))
        .route(
            "/api/projects/{id}/graphics/suggest",
            post(graphics::suggest),
        )
        .route(
            "/api/projects/{id}/graphics/generate",
            post(graphics::generate),
        )
        .route(
            "/api/projects/{id}/graphics/approve",
            post(graphics::approve),
        )
        .route("/api/projects/{id}/timeline/clips", post(add_timeline_clip))
        .route(
            "/api/projects/{id}/montage",
            axum::routing::put(creative_montage::save)
                .layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route(
            "/api/projects/{id}/montage/preview",
            post(creative_montage::preview),
        )
        .route(
            "/api/projects/{id}/montage/planner/propose",
            post(montage_planner::propose),
        )
        .route(
            "/api/projects/{id}/montage/planner/analyze",
            post(montage_planner::analyze),
        )
        .route("/api/v1/album/models", get(album::models))
        .route(
            "/api/projects/{id}/montage/album/script",
            get(album::script::view)
                .put(album::script::save)
                .layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route(
            "/api/projects/{id}/montage/album/script/propose",
            post(album::script::generate),
        )
        .route(
            "/api/projects/{id}/montage/album/analyze",
            post(album::analyze),
        )
        .route("/api/projects/{id}/montage/album/pause", post(album::pause))
        .route(
            "/api/projects/{id}/montage/planner/apply",
            post(montage_planner::apply)
                .layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route(
            "/api/projects/{id}/clips/{clip_id}/split",
            post(split_timeline_clip),
        )
        .route("/api/projects/{id}/extract", post(extract_media_clip))
        .route("/api/projects/{id}/export", post(export_timeline))
        .route("/api/jobs", get(list_jobs))
        .route(
            "/api/jobs/{id}/recover-comfy",
            post(comfy_recovery::recover),
        )
        .route("/api/jobs/archive", post(archive_jobs))
        .route(
            "/api/projects/{id}/assets/{asset}/frame",
            post(previews::capture_frame),
        )
        .route("/api/workflows", get(list_workflows))
        .route("/api/workflows/scan", post(scan_workflows))
        .route("/api/workflows/import", post(workflow_import::import))
        .route("/api/production-presets", get(production_preset::catalog))
        .route(
            "/api/projects/{id}/production-preset",
            axum::routing::put(production_preset::save),
        )
        .route("/api/workflows/diagnose", post(workflow_doctor::diagnose))
        .route("/api/workflows/repair", post(workflow_doctor::repair))
        .route("/api/comfy/submit", post(generation::submit_comfy))
        .route("/api/v1/capabilities", get(asr::capabilities))
        .route(
            "/api/v1/projects/{id}/longform/preview-render",
            post(longform_preview::render),
        )
        .route(
            "/api/v1/projects/{id}/longform/previews/{name}",
            get(longform_preview::file),
        )
        .route(
            "/api/v1/projects/{id}/longform/topics",
            post(longform_analysis::topics),
        )
        .route(
            "/api/v1/projects/{id}/longform/rough-cut",
            post(longform_analysis::rough_cut),
        )
        .route(
            "/api/v1/projects/{id}/longform/{asset}/overview",
            post(longform_analysis::overview),
        )
        .route("/api/v1/projects/{id}/search", get(project_index::view))
        .route(
            "/api/v1/projects/{id}/transcripts/{transcript}/corrections",
            post(transcript_assistant::corrections),
        )
        .route(
            "/api/v1/projects/{id}/translate",
            post(transcript_assistant::translate),
        )
        .route(
            "/api/v1/projects/{id}/transcripts/{transcript}/translations/{language}",
            get(transcript_assistant::translation_view).put(transcript_assistant::translation_save),
        )
        .route(
            "/api/v1/projects/{id}/transcripts/{transcript}/translations/{language}/srt",
            get(transcript_assistant::translation_srt),
        )
        .route(
            "/api/v1/projects/{id}/longform/chapters/{format}",
            get(longform_analysis::export),
        )
        .route(
            "/api/v1/projects/{id}/captions/proposal",
            get(caption_core::proposal),
        )
        .route(
            "/api/v1/projects/{id}/captions/sidecar/{format}",
            get(caption_core::download),
        )
        .route("/api/v1/asr/providers", get(asr::config).put(asr::save))
        .route("/api/v1/asr/discover", get(asr::discover))
        .route("/api/v1/asr/hardware", get(asr::execution::hardware))
        .route(
            "/api/v1/asr/store",
            get(asr_store::view).post(asr_store::change),
        )
        .route(
            "/api/v1/asr/catalog",
            get(asr_catalog::view).put(asr_catalog::change),
        )
        .route(
            "/api/v1/projects/{id}/asr/profile",
            post(asr_catalog::select),
        )
        .route(
            "/api/v1/projects/{id}/asr/transcripts/select",
            post(asr::select_transcript),
        )
        .route("/api/v1/projects/{id}/asr", post(asr::transcribe))
        .route(
            "/api/v1/projects/{id}/longform/{asset}",
            get(longform::view),
        )
        .route(
            "/api/v1/projects/{id}/longform",
            axum::routing::put(longform::save),
        )
        .route(
            "/api/v1/projects/{id}/longform/preview",
            post(longform::preview),
        )
        .route(
            "/api/v1/projects/{id}/longform/apply",
            post(longform::apply),
        )
        .route("/api/v1/projects/{id}/time-map", post(longform::mapping))
        .route(
            "/api/export/capabilities",
            get(export_encoding::capabilities),
        )
        .route(
            "/api/projects/{id}/timeline/{clip}/reframe",
            post(reframe::analyze),
        )
        .route(
            "/api/v1/projects/{id}/social",
            get(social::view).put(social::save),
        )
        .route(
            "/api/v1/projects/{id}/social/suggest",
            post(social::suggest),
        )
        .route("/api/v1/projects/{id}/social/batch", post(social::batch))
        .route(
            "/api/v1/projects/{id}/social/thumbnail",
            post(thumbnail::render),
        )
        .route(
            "/api/v1/projects/{id}/social/thumbnail/{key}",
            get(thumbnail::file),
        )
        .route("/api/v1/projects/{id}/social/qa", get(editorial_qa::view))
        .route(
            "/api/v1/projects/{id}/social/export/{format}",
            get(social::export),
        )
        .route(
            "/api/v1/projects/{id}/motion",
            get(motion::view)
                .put(motion::save)
                .layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route("/api/v1/fonts", get(fonts::view))
        .route(
            "/api/v1/projects/{id}/motion/idea/suggest",
            post(motion::idea::suggest),
        )
        .route(
            "/api/v1/projects/{id}/motion/idea/compile",
            post(motion::idea::compile),
        )
        .route(
            "/api/v1/projects/{id}/motion/title-design",
            post(motion::title_design::view),
        )
        .route(
            "/api/v1/motion/library/validate",
            post(motion::validate_library)
                .layer(axum::extract::DefaultBodyLimit::max(8 * 1024 * 1024)),
        )
        .route(
            "/api/v1/projects/{id}/motion/style/suggest",
            post(motion::suggest_style),
        )
        .route("/api/v1/projects/{id}/motion/export", get(motion::export))
        .route(
            "/api/v1/projects/{id}/motion/{graphic}/prepare",
            post(motion::prepare),
        )
        .route(
            "/api/v1/projects/{id}/motion/{graphic}/preview/{key}",
            get(motion::preview_file),
        )
        .route(
            "/api/v1/projects/{id}/subtitles/{unit}/align",
            post(subtitle_alignment::align),
        )
        .route(
            "/api/v1/projects/{id}/subtitles",
            get(subtitle_studio::view)
                .put(subtitle_studio::save)
                .layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route(
            "/api/v1/projects/{id}/subtitles/sync",
            post(subtitle_studio::sync),
        )
        .route(
            "/api/v1/projects/{id}/subtitles/manual",
            post(subtitle_studio::manual),
        )
        .route(
            "/api/v1/projects/{id}/subtitles/structure",
            post(subtitle_studio::structure),
        )
        .route(
            "/api/v1/projects/{id}/subtitles/export/{format}",
            get(subtitle_studio::export),
        )
        .route(
            "/api/v1/projects/{id}/subtitles/frame",
            get(subtitle_studio::frame),
        )
        .route(
            "/api/v1/projects/{id}/shorts",
            get(shorts_studio::view).put(shorts_studio::save),
        )
        .route(
            "/api/v1/projects/{id}/shorts/manual",
            post(shorts_studio::manual),
        )
        .route(
            "/api/v1/projects/{id}/shorts/suggest",
            post(shorts_studio::suggest),
        )
        .route(
            "/api/v1/projects/{id}/shorts/apply",
            post(shorts_studio::apply),
        )
        .route(
            "/api/v1/projects/{id}/shorts/batch",
            post(shorts_studio::batch),
        )
        .route(
            "/api/v1/projects/{id}/shorts/{candidate}/reframe",
            post(shorts_studio::reframe),
        )
        .route("/api/workflows/{id}", get(generation::workflow_detail))
        .route(
            "/api/workflows/{id}/controls",
            get(workflow_controls::controls),
        )
        .route("/app.js", get(web::javascript))
        .route("/style.css", get(web::stylesheet))
        .route("/appearance.js", get(web::appearance_javascript))
        .route("/appearance.css", get(web::appearance_stylesheet))
        .route("/nle.js", get(web::nle))
        .route("/nle.css", get(web::nle_stylesheet))
        .route("/system.js", get(web::system_javascript))
        .route("/flow.js", get(web::flow_javascript))
        .route("/flow.css", get(web::flow_stylesheet))
        .route("/longform.css", get(web::longform_stylesheet))
        .route("/narrative.js", get(web::narrative_javascript))
        .route("/narrative.css", get(web::narrative_stylesheet))
        .route(
            "/api/workflows/{id}/narrative-controls",
            get(narrative_workflows::controls),
        )
        .route(
            "/api/projects/{id}/narrative",
            get(narrative::get).put(narrative::save),
        )
        .route("/api/projects/{id}/narrative/plan", post(narrative::plan))
        .route(
            "/api/projects/{id}/montage/audio/analyze",
            post(montage_audio::start),
        )
        .route(
            "/api/projects/{id}/narrative/assistant/propose",
            post(narrative::assistant::propose),
        )
        .route(
            "/api/projects/{id}/narrative/assistant/apply",
            post(narrative::assistant::apply),
        )
        .route(
            "/api/projects/{id}/narrative/references/suggest",
            post(narrative::references::suggest_references),
        )
        .route(
            "/api/projects/{id}/narrative/references/approve",
            post(narrative::references::approve_reference),
        )
        .route(
            "/api/projects/{id}/narrative/references/generate",
            post(narrative::references::generate_reference),
        )
        .route(
            "/api/projects/{id}/narrative/references/analyze",
            post(narrative::references::analyze_reference),
        )
        .route(
            "/api/projects/{id}/narrative/preflight",
            post(narrative::preflight::check),
        )
        .route(
            "/api/projects/{id}/narrative/generate",
            post(narrative::generate),
        )
        .route(
            "/api/projects/{id}/narrative/approve",
            post(narrative::approve),
        )
        .route(
            "/api/projects/{id}/narrative/audio-first/approve",
            post(narrative::audio_first::approve),
        )
        .route(
            "/api/projects/{id}/production-package",
            get(production_package::inspect).post(production_package::export),
        )
        .route(
            "/api/projects/{id}/narrative/assemble",
            post(narrative::assemble),
        )
        .route(
            "/api/projects/{id}/narrative/export",
            post(narrative::export),
        )
        .route(
            "/api/projects/{id}/timeline",
            axum::routing::put(editor::replace_timeline),
        )
        .route(
            "/api/projects/{id}/audio",
            axum::routing::put(audio_mixer::save),
        )
        .route(
            "/api/projects/{id}/audio/prepared",
            get(audio_mixer::master::status),
        )
        .route(
            "/api/projects/{id}/audio/prepare",
            post(audio_mixer::master::queue),
        )
        .route(
            "/api/projects/{id}/audio/cache/{key}",
            get(audio_mixer::master::file),
        )
        .route(
            "/api/projects/{id}/story",
            axum::routing::put(editor::save_story),
        )
        .route(
            "/api/projects/{id}/story/generate",
            post(editor::generate_story),
        )
        .route("/api/projects/{id}/pipeline", post(pipeline::run_pipeline))
        .route(
            "/api/projects/{id}/assets/{asset_id}/file",
            get(editor::asset_file),
        )
        .route(
            "/api/projects/{id}/assets/{asset_id}/playback",
            get(playback::descriptor).post(playback::prepare),
        )
        .route(
            "/api/projects/{id}/assets/{asset_id}/playback/file",
            get(playback::file),
        )
        .route(
            "/api/projects/{id}/assets/{asset_id}/thumbnail",
            get(previews::thumbnail),
        )
        .route(
            "/api/projects/{id}/assets/{asset_id}/waveform",
            get(previews::waveform),
        )
        .route(
            "/api/v1/projects/{id}/assets/{asset_id}/peaks",
            get(waveform::data).post(waveform::prepare),
        )
        .route(
            "/api/v1/projects/{id}/assets/{asset_id}/shots",
            get(shot_map::view).post(shot_map::prepare),
        )
        .route(
            "/api/v1/projects/{id}/assets/{asset_id}/shots/inspect",
            post(shot_map::inspect),
        )
        .route(
            "/api/v1/projects/{id}/assets/{asset_id}/shots/review",
            axum::routing::put(shot_map::review),
        )
        .route(
            "/api/projects/{id}/exports/{name}",
            get(editor::export_file),
        )
        .route(
            "/api/projects/{id}/transcripts/import",
            post(editor::import_transcript),
        )
        .route(
            "/api/projects/{id}/transcripts/{transcript_id}/srt",
            get(editor::subtitle_file),
        )
        .route("/api/models", get(editor::models))
        .route("/api/jobs/{id}", get(editor::get_job))
        .route("/api/settings", get(editor::settings).put(settings::save))
        .route("/api/storage/preview", post(storage::preview))
        .route(
            "/api/projects/{id}/flows",
            get(pipeline_flow::list).post(pipeline_flow::start),
        )
        .route(
            "/api/projects/{id}/flows/{flow_id}/apply",
            post(pipeline_flow::apply),
        )
        .route(
            "/api/projects/{id}/flows/{flow_id}/graphics",
            post(pipeline_flow::start_graphics),
        )
        .route(
            "/api/projects/{id}/flows/{flow_id}/review",
            post(pipeline_flow::review),
        )
        .route(
            "/api/projects/{id}/flows/{flow_id}/export",
            post(pipeline_flow::export),
        )
        .route(
            "/api/providers",
            get(settings::provider_profiles).put(settings::save_provider_profile),
        )
        .route(
            "/api/providers/activate",
            post(settings::activate_provider_profile),
        )
        .route(
            "/api/providers/models",
            post(settings::probe_provider_models),
        )
        .route("/api/storage/cleanup", post(storage::cleanup))
        .route("/api/storage/models/scan", post(storage::scan_models))
        .route(
            "/api/services",
            get(services::get_config).put(services::set_config),
        )
        .route("/api/services/start", post(services::start))
        .route("/guida", get(web::guide))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(address).await?;
    info!(%address, "NL Video Suite listening");
    axum::serve(listener, app).await?;
    Ok(())
}

fn parse_arguments() -> anyhow::Result<(PathBuf, u16)> {
    let mut data_dir = std::env::current_dir()?.join("data");
    let mut port = 8088;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--data-dir" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--data-dir requires a path"))?;
                data_dir = PathBuf::from(value);
            }
            "--port" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--port requires a number"))?;
                port = value.parse::<u16>()?;
            }
            "--help" | "-h" => {
                println!(
                    "NL Video Suite\n\n  cargo run --release -- --port 8088 --data-dir ./data"
                );
                std::process::exit(0);
            }
            "--version" => {
                println!("{}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }
    Ok((data_dir, port))
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    let config = state.config();
    let ffmpeg = tokio::task::spawn_blocking(|| media::tool_available("ffmpeg"))
        .await
        .unwrap_or(false);
    let comfy_url = format!("{}/system_stats", config.comfy_url.trim_end_matches('/'));
    let model_url = providers::models_url(&config);
    let (comfyui, llm) = tokio::join!(
        async {
            state
                .http
                .get(comfy_url)
                .send()
                .await
                .map(|response| response.status().is_success())
                .unwrap_or(false)
        },
        async {
            state
                .http
                .get(model_url)
                .send()
                .await
                .map(|response| response.status().is_success())
                .unwrap_or(false)
        }
    );
    // Never query unselected LLM providers during routine health checks.
    let ollama = llm && matches!(config.llm_provider, model::LlmProvider::Ollama);
    Json(Health {
        application: "nl-video-suite",
        version: env!("CARGO_PKG_VERSION"),
        ffmpeg,
        comfyui,
        ollama,
        llm,
        llm_provider: config.llm_provider,
        installation_root: std::env::current_dir().unwrap_or_default(),
        data_dir: runtime::native_path(&state.data_dir),
        data_warnings: store::folder_issues(&state.data_dir),
    })
}

async fn integrations(State(state): State<AppState>) -> Json<Vec<model::Integration>> {
    Json(legacy::integrations(&state.config()))
}

async fn list_projects(State(state): State<AppState>) -> Json<Vec<Project>> {
    let mut projects = state
        .projects
        .read()
        .await
        .values()
        .cloned()
        .collect::<Vec<_>>();
    projects.sort_by_key(|project| std::cmp::Reverse(project.updated_at));
    Json(projects)
}

async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Project>, ApiError> {
    let id = parse_id(&id)?;
    let project = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    Ok(Json(project))
}

async fn create_project(
    State(state): State<AppState>,
    Json(request): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<Project>), ApiError> {
    let title = request.title.trim();
    if title.is_empty() || title.chars().count() > 100 {
        return Err(ApiError::bad_request("title must contain 1–100 characters"));
    }
    let project = Project::new(title.to_owned(), request.aspect_ratio);
    compositor::dimensions(&project.aspect_ratio)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    state.save_project(&project).await?;
    state
        .projects
        .write()
        .await
        .insert(project.id, project.clone());
    Ok((StatusCode::CREATED, Json(project)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameProjectRequest {
    title: String,
    expected_updated_at: chrono::DateTime<chrono::Utc>,
}

async fn rename_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<RenameProjectRequest>,
) -> Result<Json<Value>, ApiError> {
    let id = parse_id(&id)?;
    let title = request.title.trim().to_owned();
    if title.is_empty() || title.chars().count() > 100 {
        return Err(ApiError::bad_request("title must contain 1–100 characters"));
    }
    if state.jobs.read().await.values().any(|j| {
        j.project_id == Some(id) && matches!(j.status, JobStatus::Queued | JobStatus::Running)
    }) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Attendi i lavori attivi del progetto prima di rinominarlo".into(),
        ));
    }
    update_project(&state, id, |p| {
        if p.updated_at != request.expected_updated_at {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "Progetto aggiornato altrove: ricarica".into(),
            ));
        }
        p.title = title.clone();
        Ok(())
    })
    .await?;
    let project = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    let data = state.data_dir.as_ref().clone();
    let renamed =
        tokio::task::spawn_blocking(move || store::rename_project_folder(&data, &project))
            .await
            .map_err(|e| ApiError::internal(e.into()))?
            .map_err(ApiError::internal)?;
    Ok(Json(json!({
        "title": title,
        "folder_renamed": renamed.as_ref().map(|m| m.to.clone()),
        "folder_from": renamed.as_ref().map(|m| m.from.clone()),
    })))
}

async fn import_asset(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ImportAssetRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let project_id = parse_id(&id)?;
    let source = request
        .path
        .canonicalize()
        .map_err(|_| ApiError::bad_request("asset path does not exist or is not accessible"))?;
    if !source.is_file() {
        return Err(ApiError::bad_request("asset path must point to a file"));
    }
    if !state.projects.read().await.contains_key(&project_id) {
        return Err(ApiError::not_found("project not found"));
    }
    let job = Job::queued(
        JobKind::ImportAsset,
        Some(project_id),
        "waiting to import asset",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker is open");
        app.set_job(
            job.id,
            JobStatus::Running,
            "copying and probing asset",
            Vec::new(),
        )
        .await;
        let assets_dir = store::project_dir(&app.data_dir, project_id).join("assets");
        let requested_kind = request.kind;
        let result = tokio::task::spawn_blocking(move || {
            let destination = media::copy_into_project(&source, &assets_dir)?;
            let kind = requested_kind.unwrap_or_else(|| media::asset_kind_for(&destination));
            let media = match kind {
                AssetKind::Video | AssetKind::Audio | AssetKind::Image => {
                    media::probe_media(&destination)?
                }
                _ => None,
            };
            Ok::<_, anyhow::Error>((destination, kind, media, source))
        })
        .await;
        match result {
            Ok(Ok((destination, kind, media_info, original))) => {
                let asset = Asset {
                    id: Uuid::new_v4(),
                    name: destination
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("Imported asset")
                        .to_owned(),
                    kind,
                    file_name: destination
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("asset")
                        .to_owned(),
                    source_path: original.to_string_lossy().to_string(),
                    imported_at: Utc::now(),
                    media: media_info,
                };
                let outcome = update_project(&app, project_id, |project| {
                    project.assets.push(asset);
                    Ok(())
                })
                .await;
                match outcome {
                    Ok(()) => {
                        app.set_job(
                            job.id,
                            JobStatus::Succeeded,
                            "asset imported",
                            vec![destination.to_string_lossy().to_string()],
                        )
                        .await
                    }
                    Err(error) => {
                        app.set_job(job.id, JobStatus::Failed, error.1, Vec::new())
                            .await
                    }
                }
            }
            Ok(Err(error)) => {
                app.set_job(job.id, JobStatus::Failed, error.to_string(), Vec::new())
                    .await
            }
            Err(error) => {
                app.set_job(job.id, JobStatus::Failed, error.to_string(), Vec::new())
                    .await
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}

async fn add_timeline_clip(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<AddTimelineClipRequest>,
) -> Result<(StatusCode, Json<TimelineClip>), ApiError> {
    let project_id = parse_id(&id)?;
    let mut projects = state.projects.write().await;
    let project = projects
        .get_mut(&project_id)
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    let asset = project
        .assets
        .iter()
        .find(|asset| asset.id == request.asset_id)
        .cloned()
        .ok_or_else(|| ApiError::bad_request("asset does not belong to this project"))?;
    let expected_track = match asset.kind {
        AssetKind::Audio => TrackKind::Audio,
        AssetKind::Image => TrackKind::Overlay,
        _ => TrackKind::Video,
    };
    let track_id = request
        .track_id
        .or_else(|| {
            project
                .timeline
                .tracks
                .iter()
                .find(|track| track.kind == expected_track)
                .map(|track| track.id)
        })
        .ok_or_else(|| ApiError::bad_request("no compatible timeline track exists"))?;
    let track = project
        .timeline
        .tracks
        .iter()
        .find(|track| track.id == track_id)
        .ok_or_else(|| ApiError::bad_request("timeline track not found"))?;
    if track.locked {
        return Err(ApiError::bad_request("timeline track is locked"));
    }
    let trim_in_ms = request.trim_in_ms.unwrap_or(0);
    let source_duration = asset
        .media
        .as_ref()
        .and_then(|media| media.duration_ms)
        .unwrap_or(5_000);
    let source_duration = if asset.kind == AssetKind::Image {
        5_000
    } else {
        source_duration
    };
    let duration_ms = request
        .duration_ms
        .unwrap_or_else(|| source_duration.saturating_sub(trim_in_ms));
    if duration_ms == 0 {
        return Err(ApiError::bad_request(
            "clip duration must be greater than zero",
        ));
    }
    let start_ms = request.start_ms.unwrap_or_else(|| {
        project
            .timeline
            .clips
            .iter()
            .filter(|clip| clip.track_id == track_id)
            .map(|clip| clip.start_ms + clip.duration_ms)
            .max()
            .unwrap_or(0)
    });
    let clip = TimelineClip {
        id: Uuid::new_v4(),
        asset_id: asset.id,
        track_id,
        start_ms,
        trim_in_ms,
        duration_ms,
        label: request.label,
        volume: 1.0,
        opacity: 1.0,
        transform: model::VisualTransform::default(),
    };
    let mut timeline = project.timeline.clone();
    timeline.clips.push(clip.clone());
    editor::validate_timeline(project, &timeline)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    project.timeline = timeline;
    project.updated_at = Utc::now();
    let snapshot = project.clone();
    state.save_project(&snapshot).await?;
    Ok((StatusCode::CREATED, Json(clip)))
}

async fn split_timeline_clip(
    State(state): State<AppState>,
    Path((id, clip_id)): Path<(String, String)>,
    Json(request): Json<SplitClipRequest>,
) -> Result<Json<Vec<TimelineClip>>, ApiError> {
    let project_id = parse_id(&id)?;
    let clip_id = parse_id(&clip_id)?;
    let mut projects = state.projects.write().await;
    let project = projects
        .get_mut(&project_id)
        .ok_or_else(|| ApiError::not_found("project not found"))?;
    let index = project
        .timeline
        .clips
        .iter()
        .position(|clip| clip.id == clip_id)
        .ok_or_else(|| ApiError::not_found("timeline clip not found"))?;
    let original = project.timeline.clips[index].clone();
    if project
        .timeline
        .tracks
        .iter()
        .any(|t| t.id == original.track_id && t.locked)
    {
        return Err(ApiError::bad_request("Traccia bloccata"));
    }
    let clip_end = original.start_ms + original.duration_ms;
    if request.at_ms <= original.start_ms || request.at_ms >= clip_end {
        return Err(ApiError::bad_request("split point must be inside the clip"));
    }
    let offset = request.at_ms - original.start_ms;
    let first = TimelineClip {
        duration_ms: offset,
        ..original.clone()
    };
    let second = TimelineClip {
        id: Uuid::new_v4(),
        start_ms: request.at_ms,
        trim_in_ms: original.trim_in_ms + offset,
        duration_ms: original.duration_ms - offset,
        ..original
    };
    project.timeline.clips[index] = first.clone();
    project.timeline.clips.insert(index + 1, second.clone());
    project.updated_at = Utc::now();
    let snapshot = project.clone();
    state.save_project(&snapshot).await?;
    Ok(Json(vec![first, second]))
}

async fn extract_media_clip(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ExtractClipRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let project_id = parse_id(&id)?;
    if request.end_ms <= request.start_ms {
        return Err(ApiError::bad_request(
            "end_ms must be greater than start_ms",
        ));
    }
    let (input, extension) = {
        let projects = state.projects.read().await;
        let project = projects
            .get(&project_id)
            .ok_or_else(|| ApiError::not_found("project not found"))?;
        let asset = project
            .assets
            .iter()
            .find(|asset| asset.id == request.asset_id)
            .ok_or_else(|| ApiError::bad_request("asset does not belong to this project"))?;
        if asset.kind != AssetKind::Video {
            return Err(ApiError::bad_request("only a video asset can be extracted"));
        }
        (
            store::project_dir(&state.data_dir, project_id)
                .join("assets")
                .join(&asset.file_name),
            std::path::Path::new(&asset.file_name)
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("mp4")
                .to_owned(),
        )
    };
    let title = request.title.unwrap_or_else(|| "derived-clip".to_owned());
    let name = format!(
        "{}-{}.{}",
        filesystem_stem(&title),
        Uuid::new_v4(),
        extension
    );
    let output = store::project_dir(&state.data_dir, project_id)
        .join("assets")
        .join(name);
    let job = Job::queued(
        JobKind::ExtractClip,
        Some(project_id),
        "waiting to extract clip",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker is open");
        app.set_job(
            job.id,
            JobStatus::Running,
            "extracting clip with FFmpeg",
            Vec::new(),
        )
        .await;
        let output_for_task = output.clone();
        let input_for_task = input.clone();
        let result = tokio::task::spawn_blocking(move || {
            media::extract_clip(
                &input_for_task,
                &output_for_task,
                request.start_ms,
                request.end_ms,
            )?;
            let media_info = media::probe_media(&output_for_task)?;
            Ok::<_, anyhow::Error>((output_for_task, media_info))
        })
        .await;
        match result {
            Ok(Ok((output_path, media_info))) => {
                let asset = Asset {
                    id: Uuid::new_v4(),
                    name: title,
                    kind: AssetKind::Video,
                    file_name: output_path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("derived-clip.mp4")
                        .to_owned(),
                    source_path: input.to_string_lossy().to_string(),
                    imported_at: Utc::now(),
                    media: media_info,
                };
                match update_project(&app, project_id, |project| {
                    project.assets.push(asset);
                    Ok(())
                })
                .await
                {
                    Ok(()) => {
                        app.set_job(
                            job.id,
                            JobStatus::Succeeded,
                            "derived clip created",
                            vec![output_path.to_string_lossy().to_string()],
                        )
                        .await
                    }
                    Err(error) => {
                        app.set_job(job.id, JobStatus::Failed, error.1, Vec::new())
                            .await
                    }
                }
            }
            Ok(Err(error)) => {
                app.set_job(job.id, JobStatus::Failed, error.to_string(), Vec::new())
                    .await
            }
            Err(error) => {
                app.set_job(job.id, JobStatus::Failed, error.to_string(), Vec::new())
                    .await
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}

async fn export_timeline(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let project_id = parse_id(&id)?;
    let project = state
        .projects
        .read()
        .await
        .get(&project_id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    queue_export(state, project).await
}

/// Queue exactly this immutable snapshot. Narrative SRT and MP4 use the same
/// timing even when the user edits the live timeline while export is queued.
async fn queue_export(
    state: AppState,
    project: Project,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let project_id = project.id;
    subtitle_studio::validate_delivery(&project)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let (inputs, output, size) = {
        let root = store::project_dir(&state.data_dir, project_id);
        let layers = compositor::manifest(&project, &root)
            .map_err(|e| ApiError::bad_request(e.to_string()))?;
        let size = compositor::dimensions(&project.aspect_ratio)
            .map_err(|e| ApiError::bad_request(e.to_string()))?;
        let output = root.join("exports").join(format!(
            "{}-{}.mp4",
            filesystem_stem(&project.title),
            Uuid::new_v4()
        ));
        (layers, output, size)
    };
    let job = Job::queued(
        JobKind::ExportTimeline,
        Some(project_id),
        "Esportazione del montaggio in coda",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker is open");
        app.set_job(
            job.id,
            JobStatus::Running,
            "Esportazione FFmpeg · compositing CPU due thread; verifica encoder richiesto prima del render",
            Vec::new(),
        )
        .await;
        let output_for_task = output.clone();
        let duration_ms = project
            .timeline
            .clips
            .iter()
            .map(|c| c.start_ms + c.duration_ms)
            .max()
            .unwrap_or(1)
            .max(motion::duration_ms(&project));
        let project_root = store::project_dir(&app.data_dir, project_id);
        let mut task = tokio::task::spawn_blocking(move || {
            if let Some(parent) = output_for_task.parent() {
                std::fs::create_dir_all(parent)?;
            }
            compositor::render_project(
                &project,
                &project_root,
                &inputs,
                &output_for_task,
                size,
                social::render_options(&project),
            )?;
            let mut files = vec![
                output_for_task.to_string_lossy().to_string(),
                output_for_task
                    .with_extension("encoding.json")
                    .to_string_lossy()
                    .to_string(),
            ];
            files.extend(subtitle_studio::write_delivery(
                &project,
                &output_for_task,
                size,
            )?);
            files.extend(social::write_delivery(&project, &output_for_task)?);
            Ok::<_, anyhow::Error>(files)
        });
        let result = loop {
            tokio::select! {
                result=&mut task=>break result,
                _=tokio::time::sleep(Duration::from_millis(750))=>{
                    let progress=tokio::fs::read_to_string(output.with_extension("progress.txt")).await.unwrap_or_default();
                    let elapsed=progress.lines().filter_map(|l|l.strip_prefix("out_time_us=").and_then(|v|v.parse::<u64>().ok())).next_back().unwrap_or(0)/1000;
                    if elapsed>0 {app.set_job(job.id,JobStatus::Running,format!("Export FFmpeg · {}% · {:.1}/{:.1}s renderizzati · avanzamento misurato",(elapsed.saturating_mul(100)/duration_ms).min(99),elapsed as f64/1000.,duration_ms as f64/1000.),vec![]).await;}
                }
            }
        };
        match result {
            Ok(Ok(outputs)) => {
                let encoding: serde_json::Value =
                    store::read_json(&output.with_extension("encoding.json")).unwrap_or_default();
                let video = encoding
                    .pointer("/actual/streams")
                    .and_then(serde_json::Value::as_array)
                    .and_then(|s| s.iter().find(|v| v["codec_type"] == "video"));
                let message = video
                    .map(|v| {
                        format!(
                            "Video esportato · {}×{} · {} fps · {} / AAC{}",
                            v["width"],
                            v["height"],
                            v["avg_frame_rate"].as_str().unwrap_or("?"),
                            v["codec_name"].as_str().unwrap_or("?"),
                            if encoding["fallback"].is_string() {
                                " · FALLBACK CPU autorizzato (dettagli encoding.json)"
                            } else {
                                ""
                            }
                        )
                    })
                    .unwrap_or_else(|| "Video esportato".into());
                if let Some(j) = app.jobs.write().await.get_mut(&job.id) {
                    let result = j.result.get_or_insert_with(|| json!({}));
                    result["encoding"] = encoding;
                }
                app.set_job(job.id, JobStatus::Succeeded, message, outputs)
                    .await
            }
            Ok(Err(error)) => {
                app.set_job(job.id, JobStatus::Failed, error.to_string(), Vec::new())
                    .await
            }
            Err(error) => {
                app.set_job(job.id, JobStatus::Failed, error.to_string(), Vec::new())
                    .await
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}

async fn list_jobs(State(state): State<AppState>) -> Json<Vec<Job>> {
    let mut jobs = state
        .jobs
        .read()
        .await
        .values()
        .cloned()
        .collect::<Vec<_>>();
    jobs.sort_by_key(|job| std::cmp::Reverse(job.created_at));
    Json(jobs)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchiveJobsRequest {
    project_id: Option<Uuid>,
    archived: bool,
}

async fn archive_jobs(
    State(state): State<AppState>,
    Json(request): Json<ArchiveJobsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut jobs = state.jobs.write().await;
    let mut next = jobs.clone();
    let mut changed = 0;
    for job in next.values_mut() {
        if request
            .project_id
            .is_none_or(|id| job.project_id == Some(id))
            && matches!(job.status, JobStatus::Succeeded | JobStatus::Failed)
            && job.archived != request.archived
        {
            job.archived = request.archived;
            changed += 1;
        }
    }
    store::save_jobs(&state.data_dir, &next.values().cloned().collect::<Vec<_>>())
        .map_err(ApiError::internal)?;
    *jobs = next;
    Ok(Json(
        serde_json::json!({"changed":changed,"files_deleted":0}),
    ))
}

async fn list_workflows(State(state): State<AppState>) -> Json<Vec<WorkflowSummary>> {
    Json(state.workflows.read().await.clone())
}

async fn scan_workflows(
    State(state): State<AppState>,
    Json(payload): Json<WorkflowScanRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let roots = payload
        .roots
        .unwrap_or_else(|| default_workflow_roots(&state.config(), &state.data_dir));
    let job = Job::queued(JobKind::ScanWorkflows, None, "waiting to scan workflows");
    state.add_job(job.clone()).await?;
    let app = state.clone();
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker is open");
        app.set_job(
            job.id,
            JobStatus::Running,
            "scanning workflow JSON files",
            Vec::new(),
        )
        .await;
        let result = tokio::task::spawn_blocking(move || media::scan_workflows(&roots)).await;
        match result {
            Ok(Ok(mut workflows)) => {
                runtime::portable_workflow_paths(&mut workflows);
                let count = workflows.len();
                *app.workflows.write().await = workflows.clone();
                match store::save_workflows(&app.data_dir, &workflows) {
                    Ok(()) => {
                        app.set_job(
                            job.id,
                            JobStatus::Succeeded,
                            format!("catalogued {count} workflows"),
                            Vec::new(),
                        )
                        .await
                    }
                    Err(error) => {
                        app.set_job(job.id, JobStatus::Failed, error.to_string(), Vec::new())
                            .await
                    }
                }
            }
            Ok(Err(error)) => {
                app.set_job(job.id, JobStatus::Failed, error.to_string(), Vec::new())
                    .await
            }
            Err(error) => {
                app.set_job(job.id, JobStatus::Failed, error.to_string(), Vec::new())
                    .await
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}

async fn update_project<F>(state: &AppState, project_id: Uuid, mutate: F) -> Result<(), ApiError>
where
    F: FnOnce(&mut Project) -> Result<(), ApiError>,
{
    let snapshot = {
        let mut projects = state.projects.write().await;
        project_management::ensure_ready(&state.data_dir, project_id)?;
        edit_versions::ensure_ready(&state.data_dir, project_id)?;
        let project = projects
            .get_mut(&project_id)
            .ok_or_else(|| ApiError::not_found("project not found"))?;
        let mut snapshot = project.clone();
        mutate(&mut snapshot)?;
        snapshot.updated_at = Utc::now();
        state.save_project(&snapshot).await?;
        *project = snapshot.clone();
        snapshot
    };
    let _ = snapshot;
    Ok(())
}

fn default_workflow_roots(config: &AppConfig, data_dir: &std::path::Path) -> Vec<PathBuf> {
    let mut roots = if config.workflow_roots.is_empty() {
        vec![PathBuf::from("workflows")]
    } else {
        config.workflow_roots.clone()
    };
    roots.push(data_dir.join("workflows").join("repaired"));
    roots.push(data_dir.join("workflows").join("imported"));
    roots
}

fn filesystem_stem(value: &str) -> String {
    let output: String = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let output = output.trim_matches('-');
    if output.is_empty() {
        "output".to_owned()
    } else {
        output.to_owned()
    }
}

fn parse_id(value: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(value).map_err(|_| ApiError::bad_request("invalid UUID"))
}
