//! Browser playback proxies. Full-quality project assets remain the export source.
use crate::{
    model::{Asset, AssetKind, Job, JobKind, JobStatus},
    runtime, store, ApiError, AppState,
};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command, sync::OnceLock};
use tokio::sync::Mutex;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

static SUBMISSION: OnceLock<Mutex<()>> = OnceLock::new();
// A bounded, low-memory CPU decoder must not hold the heavy inference worker
// for minutes before ASR starts. No GPU or external service is used here.
static PLAYBACK_WORKER: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
#[derive(Default, serde::Deserialize)]
pub struct RefreshRequest {
    #[serde(default)]
    refresh: bool,
}

async fn owned(
    state: &AppState,
    project: Uuid,
    asset: Uuid,
) -> Result<(Asset, PathBuf, PathBuf), ApiError> {
    let item = state
        .projects
        .read()
        .await
        .get(&project)
        .and_then(|p| p.assets.iter().find(|a| a.id == asset))
        .cloned()
        .ok_or_else(|| ApiError::not_found("Media assente"))?;
    if !matches!(item.kind, AssetKind::Video | AssetKind::Audio) {
        return Err(ApiError::bad_request("Seleziona un video o un audio"));
    }
    let root = store::project_dir(&state.data_dir, project)
        .join("assets")
        .canonicalize()
        .map_err(|e| ApiError::internal(e.into()))?;
    let input = root
        .join(&item.file_name)
        .canonicalize()
        .map_err(|e| ApiError::internal(e.into()))?;
    if !input.starts_with(&root) {
        return Err(ApiError::bad_request("Media fuori dal progetto"));
    }
    let output = state
        .data_dir
        .join("previews")
        .join(project.to_string())
        .join(format!("{asset}-playback-v2.mp4"));
    Ok((item, runtime::native_path(&input), output))
}
fn native_compatible(extension: &str, streams: &[Value]) -> bool {
    streams.iter().all(|s| match s["codec_type"].as_str() {
        Some("video") => matches!(
            (extension, s["codec_name"].as_str(), s["pix_fmt"].as_str()),
            (
                "mp4" | "m4v" | "mov",
                Some("h264"),
                Some("yuv420p" | "yuvj420p")
            ) | ("webm", Some("vp8" | "vp9" | "av1"), _)
        ),
        Some("audio") => match extension {
            "mp4" | "m4v" | "mov" | "m4a" => s["codec_name"] == "aac",
            "wav" => matches!(s["codec_name"].as_str(), Some("pcm_s16le" | "pcm_f32le")),
            "mp3" => s["codec_name"] == "mp3",
            "ogg" | "webm" => matches!(s["codec_name"].as_str(), Some("opus" | "vorbis")),
            "flac" => true,
            _ => false,
        },
        _ => true,
    }) && streams
        .iter()
        .any(|s| matches!(s["codec_type"].as_str(), Some("video" | "audio")))
}
async fn related_job(state: &AppState, project: Uuid, asset: Uuid) -> Option<Job> {
    state
        .jobs
        .read()
        .await
        .values()
        .filter(|j| {
            j.project_id == Some(project)
                && matches!(j.kind, JobKind::PreparePreview)
                && j.result
                    .as_ref()
                    .and_then(|r| r["preview_asset_id"].as_str())
                    .is_some_and(|s| s == asset.to_string())
        })
        .max_by_key(|j| j.created_at)
        .cloned()
}
pub async fn descriptor(
    State(state): State<AppState>,
    Path((project, asset)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, ApiError> {
    let (item, input, output) = owned(&state, project, asset).await?;
    let previous = related_job(&state, project, asset).await;
    if let Some(job) = previous.as_ref() {
        if matches!(job.status, JobStatus::Queued | JobStatus::Running) {
            return Ok(Json(
                json!({"mode":"preparing","job_id":job.id,"message":job.message}),
            ));
        }
        if matches!(job.status, JobStatus::Failed) && !output.is_file() {
            return Ok(Json(
                json!({"mode":"failed","job_id":job.id,"message":job.message}),
            ));
        }
    }
    if output.is_file() {
        return Ok(Json(
            json!({"mode":"proxy","url":format!("/api/projects/{project}/assets/{asset}/playback/file"),"message":"Anteprima H.264 leggera · export dal media originale"}),
        ));
    }
    let extension = std::path::Path::new(&item.file_name)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let compatible = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
        let result = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,codec_name,pix_fmt",
                "-of",
                "json",
            ])
            .arg(input)
            .output()?;
        anyhow::ensure!(
            result.status.success(),
            "Impossibile leggere il codec del media"
        );
        let value: Value = serde_json::from_slice(&result.stdout)?;
        Ok(native_compatible(
            &extension,
            value["streams"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default(),
        ))
    })
    .await
    .map_err(|e| ApiError::internal(e.into()))?
    .map_err(ApiError::internal)?;
    Ok(Json(if compatible {
        json!({"mode":"native","url":format!("/api/projects/{project}/assets/{asset}/file")})
    } else {
        json!({"mode":"proxy_required","message":"Formato non universale nel browser: preparazione automatica dell’anteprima compatibile"})
    }))
}
pub async fn prepare(
    State(state): State<AppState>,
    Path((project, asset)): Path<(Uuid, Uuid)>,
    Query(options): Query<RefreshRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let _gate = SUBMISSION.get_or_init(|| Mutex::new(())).lock().await;
    let (item, input, output) = owned(&state, project, asset).await?;
    if output.is_file() && !options.refresh {
        return Ok((
            StatusCode::OK,
            Json(
                json!({"mode":"proxy","url":format!("/api/projects/{project}/assets/{asset}/playback/file")}),
            ),
        ));
    }
    if let Some(job) = related_job(&state, project, asset).await {
        if matches!(job.status, JobStatus::Queued | JobStatus::Running) {
            return Ok((
                StatusCode::ACCEPTED,
                Json(json!({"mode":"preparing","job_id":job.id})),
            ));
        }
    }
    let mut job = Job::queued(
        JobKind::PreparePreview,
        Some(project),
        "Anteprima compatibile in coda (CPU, senza modelli IA)",
    );
    job.result = Some(json!({"preview_asset_id":asset}));
    let id = job.id;
    state.add_job(job).await?;
    tokio::spawn(async move {
        let _permit = PLAYBACK_WORKER
            .get_or_init(|| tokio::sync::Semaphore::new(1))
            .acquire()
            .await
            .expect("playback worker open");
        state
            .set_job(
                id,
                JobStatus::Running,
                "Preparazione anteprima H.264/AAC · CPU, massimo 2 thread",
                vec![],
            )
            .await;
        let destination = output.clone();
        let progress_state = state.clone();
        let result=async move {
            let directory=destination.parent().expect("preview directory");std::fs::create_dir_all(directory)?;
            let temporary=directory.join(format!("{asset}-{}.tmp",Uuid::new_v4()));
            let progress=directory.join(format!("{asset}-{id}-progress.txt"));
            let log_path=directory.join(format!("{asset}-{id}-playback.log"));let log=std::fs::File::create(&log_path)?;
            let mut command=tokio::process::Command::new("ffmpeg");
            #[cfg(windows)]command.creation_flags(0x08000000);
            command.args(["-nostdin","-v","error","-n","-threads","2","-filter_threads","2","-progress"]).arg(&progress).arg("-i").arg(input);
            if item.kind==AssetKind::Video {
                command.args(["-map","0:v:0","-map","0:a:0?","-vf","scale=w='max(2,trunc(iw*min(1,min(960/iw,720/ih))/2)*2)':h='max(2,trunc(ih*min(1,min(960/iw,720/ih))/2)*2)',fps=30","-c:v","libx264","-preset","veryfast","-crf","24","-pix_fmt","yuv420p","-threads","2"]);
            } else {command.args(["-vn","-map","0:a:0"]);}
            let mut child=command.args(["-c:a","aac","-b:a","128k","-movflags","+faststart","-map_metadata","-1","-f","mp4"]).arg(&temporary).stdout(std::process::Stdio::null()).stderr(log).kill_on_drop(true).spawn()?;
            let duration=item.media.as_ref().and_then(|m|m.duration_ms);let mut previous=String::new();
            let status=loop{if let Some(status)=child.try_wait()?{break status;}let text=tokio::fs::read_to_string(&progress).await.unwrap_or_default();let elapsed=text.lines().filter_map(|l|l.strip_prefix("out_time_us=").and_then(|v|v.parse::<u64>().ok())).next_back().unwrap_or(0)/1000;
                let message=if let Some(total)=duration.filter(|v|*v>0){format!("Anteprima H.264 CPU · {}% · {:.0}/{:.0}s · ASR non bloccata",(elapsed*100/total).min(99),elapsed as f64/1000.,total as f64/1000.)}else{format!("Anteprima H.264 CPU · {:.0}s convertiti",elapsed as f64/1000.)};
                if previous!=message{progress_state.set_job(id,JobStatus::Running,&message,vec![]).await;previous=message;}tokio::time::sleep(std::time::Duration::from_millis(750)).await;
            };
            if !status.success() {let _=std::fs::remove_file(&temporary);anyhow::bail!("FFmpeg: {}",std::fs::read_to_string(log_path).unwrap_or_default());}
            std::fs::rename(temporary,destination)?;let _=std::fs::remove_file(progress);let _=std::fs::remove_file(log_path);Ok::<_,anyhow::Error>(())
        }.await;
        match result {
            Ok(()) => {
                state
                    .set_job(
                        id,
                        JobStatus::Succeeded,
                        "Anteprima compatibile pronta. Export e file originale invariati.",
                        vec![runtime::native_path(&output).to_string_lossy().into_owned()],
                    )
                    .await
            }
            other => {
                state
                    .set_job(
                        id,
                        JobStatus::Failed,
                        format!("Anteprima non preparata: {other:?}"),
                        vec![],
                    )
                    .await
            }
        }
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"mode":"preparing","job_id":id})),
    ))
}
pub async fn file(
    State(state): State<AppState>,
    Path((project, asset)): Path<(Uuid, Uuid)>,
    request: Request<Body>,
) -> Result<Response, ApiError> {
    let (_, _, output) = owned(&state, project, asset).await?;
    ServeFile::new(output)
        .oneshot(request)
        .await
        .map(IntoResponse::into_response)
        .map_err(|e| ApiError::internal(e.into()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codec_policy() {
        assert!(native_compatible(
            "mp4",
            &[
                json!({"codec_type":"video","codec_name":"h264","pix_fmt":"yuv420p"}),
                json!({"codec_type":"audio","codec_name":"aac"})
            ]
        ));
        assert!(!native_compatible(
            "mp4",
            &[json!({"codec_type":"video","codec_name":"hevc","pix_fmt":"yuv420p"})]
        ));
        assert!(!native_compatible(
            "mp4",
            &[json!({"codec_type":"video","codec_name":"h264","pix_fmt":"yuv420p10le"})]
        ));
        assert!(!native_compatible(
            "mkv",
            &[json!({"codec_type":"video","codec_name":"h264","pix_fmt":"yuv420p"})]
        ));
        assert!(!native_compatible("mp4", &[]));
    }
}
