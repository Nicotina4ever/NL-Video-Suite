//! Regenerable previews of project-owned media, never the source folders.
use crate::{model::AssetKind, store, ApiError, AppState};
use axum::{
    body::Body,
    extract::{Path, State},
    http::Request,
    response::{IntoResponse, Response},
};
use std::{process::Command, sync::OnceLock};
use tokio::sync::Semaphore;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

static PREVIEW_GATE: OnceLock<Semaphore> = OnceLock::new();

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameRequest {
    pub at_ms: u64,
}

/// Extract from the full-resolution project copy, not the browser proxy/canvas.
pub async fn capture_frame(
    State(state): State<AppState>,
    Path((project, id)): Path<(Uuid, Uuid)>,
    axum::Json(request): axum::Json<FrameRequest>,
) -> Result<axum::Json<crate::model::Asset>, ApiError> {
    let asset = state
        .projects
        .read()
        .await
        .get(&project)
        .and_then(|p| p.assets.iter().find(|a| a.id == id))
        .cloned()
        .ok_or_else(|| ApiError::not_found("Video assente"))?;
    if asset.kind != AssetKind::Video
        || asset
            .media
            .as_ref()
            .and_then(|m| m.duration_ms)
            .is_none_or(|d| request.at_ms >= d)
    {
        return Err(ApiError::bad_request(
            "Scegli un fotogramma dentro la durata del video",
        ));
    }
    let root = store::project_dir(&state.data_dir, project)
        .join("assets")
        .canonicalize()
        .map_err(|e| ApiError::internal(e.into()))?;
    let source = root
        .join(&asset.file_name)
        .canonicalize()
        .map_err(|e| ApiError::internal(e.into()))?;
    if !source.starts_with(&root) {
        return Err(ApiError::bad_request("Media fuori dal progetto"));
    }
    let _permit = PREVIEW_GATE
        .get_or_init(|| Semaphore::new(1))
        .acquire()
        .await
        .expect("preview gate open");
    let frame = tokio::task::spawn_blocking(move || -> anyhow::Result<crate::model::Asset> {
        let name = format!("frame-{}-{}.png", request.at_ms, Uuid::new_v4());
        let destination = root.join(&name);
        let mut command = Command::new("ffmpeg");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let result = command
            .args([
                "-nostdin",
                "-v",
                "error",
                "-n",
                "-threads",
                "2",
                "-ss",
                &format!("{:.3}", request.at_ms as f64 / 1000.0),
            ])
            .arg("-i")
            .arg(&source)
            .args(["-map", "0:v:0", "-frames:v", "1", "-threads", "2"])
            .arg(&destination)
            .output()?;
        anyhow::ensure!(
            result.status.success() && destination.is_file(),
            "Fotogramma: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        Ok(crate::model::Asset {
            id: Uuid::new_v4(),
            name: format!(
                "{} · frame {:.3}s",
                asset.name,
                request.at_ms as f64 / 1000.0
            ),
            kind: AssetKind::Image,
            file_name: name,
            source_path: format!("Frame di asset {id} a {} ms", request.at_ms),
            imported_at: chrono::Utc::now(),
            media: crate::media::probe_media(&destination)?,
        })
    })
    .await
    .map_err(|e| ApiError::internal(e.into()))?
    .map_err(ApiError::internal)?;
    crate::update_project(&state, project, |p| {
        p.assets.push(frame.clone());
        Ok(())
    })
    .await?;
    Ok(axum::Json(frame))
}

pub async fn thumbnail(
    State(state): State<AppState>,
    Path((project, asset)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    create(state, project, asset).await
}
pub async fn waveform(
    State(state): State<AppState>,
    Path((project, asset)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    crate::waveform::overview(state, project, asset).await
}
async fn create(state: AppState, project: Uuid, id: Uuid) -> Result<Response, ApiError> {
    let asset = state
        .projects
        .read()
        .await
        .get(&project)
        .and_then(|p| p.assets.iter().find(|a| a.id == id))
        .cloned()
        .ok_or_else(|| ApiError::not_found("Media non trovato"))?;
    if !matches!(asset.kind, AssetKind::Video | AssetKind::Image) {
        return Err(ApiError::bad_request("Il media non ha un'anteprima visiva"));
    }
    let owned = store::project_dir(&state.data_dir, project)
        .join("assets")
        .canonicalize()
        .map_err(|e| ApiError::internal(e.into()))?;
    let source = owned
        .join(&asset.file_name)
        .canonicalize()
        .map_err(|e| ApiError::internal(e.into()))?;
    if !source.starts_with(&owned) {
        return Err(ApiError::bad_request("Percorso media fuori dal progetto"));
    }
    let directory = state.data_dir.join("previews").join(project.to_string());
    let output = directory.join(format!("{id}-poster.jpg"));
    if !output.is_file() {
        let _permit = PREVIEW_GATE
            .get_or_init(|| Semaphore::new(1))
            .acquire()
            .await
            .expect("preview gate open");
        if !output.is_file() {
            let destination = output.clone();
            let duration = asset
                .media
                .as_ref()
                .and_then(|m| m.duration_ms)
                .unwrap_or(0);
            tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                std::fs::create_dir_all(&directory)?;
                let temporary=directory.join(format!("{}.tmp",Uuid::new_v4()));
                let mut command=Command::new("ffmpeg");
                command.args(["-nostdin","-v","error","-n"]);
                if asset.kind==AssetKind::Video {
                    command.args(["-ss", &format!("{:.3}",(duration as f64/1000.0*0.15).min(3.0))]);
                }
                command.arg("-i").arg(&source);
                command.args(["-vf","scale=240:160:force_original_aspect_ratio=decrease,pad=240:160:(ow-iw)/2:(oh-ih)/2:color=0x15111e","-c:v","mjpeg","-q:v","5"]);
                let result=command.args(["-frames:v","1","-f","image2","-update","1"]).arg(&temporary).output()?;
                anyhow::ensure!(result.status.success(),"Anteprima: {}",String::from_utf8_lossy(&result.stderr));
                std::fs::rename(&temporary,&destination)?;
                Ok(())
            }).await.map_err(|e|ApiError::internal(e.into()))?.map_err(ApiError::internal)?;
        }
    }
    let response = ServeFile::new(output)
        .oneshot(Request::new(Body::empty()))
        .await
        .map_err(|e| ApiError::internal(e.into()))?;
    Ok(response.into_response())
}
