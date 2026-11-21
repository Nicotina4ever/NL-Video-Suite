use crate::{media, model::*, store, update_project, ApiError, AppState};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path as FsPath, PathBuf};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct TimelineRequest {
    pub timeline: Timeline,
    pub expected_updated_at: Option<chrono::DateTime<Utc>>,
    #[serde(default)]
    pub montage_titles: bool,
}

pub fn validate_timeline(project: &Project, timeline: &Timeline) -> anyhow::Result<()> {
    use std::collections::HashSet;
    let mut ids = HashSet::new();
    for track in &timeline.tracks {
        anyhow::ensure!(ids.insert(track.id), "ID traccia duplicato");
    }
    ids.clear();
    for clip in &timeline.clips {
        clip.transform.validate()?;
        anyhow::ensure!(ids.insert(clip.id), "ID clip duplicato");
        let track = timeline
            .tracks
            .iter()
            .find(|t| t.id == clip.track_id)
            .ok_or_else(|| anyhow::anyhow!("Traccia inesistente"))?;
        let asset = project
            .assets
            .iter()
            .find(|a| a.id == clip.asset_id)
            .ok_or_else(|| anyhow::anyhow!("Asset inesistente"))?;
        anyhow::ensure!(
            clip.duration_ms > 0 && clip.duration_ms <= 86_400_000,
            "Durata non valida (massimo 24 ore)"
        );
        anyhow::ensure!(
            clip.start_ms
                .checked_add(clip.duration_ms)
                .is_some_and(|end| end <= 86_400_000),
            "Fine clip fuori dai limiti"
        );
        let source_end = clip
            .trim_in_ms
            .checked_add(clip.duration_ms)
            .ok_or_else(|| anyhow::anyhow!("Trim non valido"))?;
        if asset.kind != AssetKind::Image {
            if let Some(duration) = asset.media.as_ref().and_then(|m| m.duration_ms) {
                anyhow::ensure!(
                    source_end <= duration + 50,
                    "Trim oltre la durata del media"
                );
            }
        }
        anyhow::ensure!(
            clip.volume.is_finite() && (0.0..=4.0).contains(&clip.volume),
            "Volume fuori dai limiti"
        );
        anyhow::ensure!(
            clip.opacity.is_finite() && (0.0..=1.0).contains(&clip.opacity),
            "Opacità fuori dai limiti"
        );
        anyhow::ensure!(
            match asset.kind {
                // A video with audio can live on the audio track: it is the
                // detached-audio clip Premiere-style unlink produces, and the
                // compositor renders it as audio only (audio tracks are never
                // drawn visually).
                AssetKind::Video => {
                    track.kind != TrackKind::Audio
                        || asset.media.as_ref().is_some_and(|m| m.has_audio)
                }
                AssetKind::Audio => track.kind == TrackKind::Audio,
                AssetKind::Image => track.kind != TrackKind::Audio,
                _ => false,
            },
            "Media incompatibile con la traccia"
        );
    }
    Ok(())
}

pub async fn replace_timeline(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<TimelineRequest>,
) -> Result<Json<Project>, ApiError> {
    let mut projects = state.projects.write().await;
    let project = projects
        .get_mut(&id)
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    if request
        .expected_updated_at
        .is_some_and(|revision| revision != project.updated_at)
    {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Il progetto è cambiato. Ricarica prima di salvare".into(),
        ));
    }
    validate_timeline(project, &request.timeline)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    for old_track in project.timeline.tracks.iter().filter(|t| t.locked) {
        let before = project
            .timeline
            .clips
            .iter()
            .filter(|c| c.track_id == old_track.id)
            .collect::<Vec<_>>();
        let after = request
            .timeline
            .clips
            .iter()
            .filter(|c| c.track_id == old_track.id)
            .collect::<Vec<_>>();
        if serde_json::to_value(before).ok() != serde_json::to_value(after).ok() {
            return Err(ApiError::bad_request(
                "Sblocca la traccia prima di modificare i clip",
            ));
        }
    }
    let mut next = project.clone();
    next.timeline = request.timeline;
    if request.montage_titles {
        crate::creative_montage::add_titles(
            &mut next,
            project,
            Some(&crate::store::project_dir(&state.data_dir, id)),
        )?;
    }
    // The real clip is the source of truth after approval. Keep its object's
    // timing in sync, including undo/redo, so saving a prompt cannot silently
    // restore the position from before a timeline move or trim.
    for object in &mut next.graphic_objects {
        if let Some(clip) = next
            .timeline
            .clips
            .iter()
            .find(|c| c.id == crate::graphic_objects::overlay_id(object.id))
        {
            object.start_ms = clip.start_ms;
            object.duration_ms = clip.duration_ms;
        }
    }
    next.updated_at = Utc::now();
    state.save_project(&next).await?;
    *project = next.clone();
    Ok(Json(next))
}

#[derive(Deserialize)]
pub struct CreativeBriefRequest {
    pub creative_brief: String,
    pub expected_updated_at: Option<chrono::DateTime<Utc>>,
}

pub async fn save_creative_brief(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<CreativeBriefRequest>,
) -> Result<Json<Project>, ApiError> {
    if request.creative_brief.chars().count() > 2000 {
        return Err(ApiError::bad_request(
            "Direzione visiva: massimo 2000 caratteri",
        ));
    }
    let mut projects = state.projects.write().await;
    let project = projects
        .get_mut(&id)
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    if request
        .expected_updated_at
        .is_some_and(|revision| revision != project.updated_at)
    {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Il progetto è cambiato. Ricarica prima di salvare".into(),
        ));
    }
    let mut next = project.clone();
    next.creative_brief = request.creative_brief;
    next.updated_at = Utc::now();
    state.save_project(&next).await?;
    *project = next.clone();
    Ok(Json(next))
}

pub async fn save_story(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(story): Json<Story>,
) -> Result<Json<Value>, ApiError> {
    update_project(&state, id, |p| {
        p.story = story;
        Ok(())
    })
    .await?;
    Ok(Json(json!({"saved":true})))
}

#[derive(Deserialize)]
pub struct StoryRequest {
    pub premise: String,
}
pub async fn generate_story(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<StoryRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    if request.premise.trim().is_empty() {
        return Err(ApiError::bad_request("Inserisci un'idea"));
    }
    let creative_brief = state
        .projects
        .read()
        .await
        .get(&id)
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?
        .creative_brief
        .clone();
    let config = state.config();
    let job = Job::queued(JobKind::GenerateStory, Some(id), "Storyboard in coda");
    state.add_job(job.clone()).await?;
    let job_id = job.id;
    let app = state.clone();
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(
            job_id,
            JobStatus::Running,
            "Scrittura storyboard con il provider LLM selezionato",
            vec![],
        )
        .await;
        let result=async {
            let response=crate::providers::chat(&app.http,&config,"story",json!([
                {"role":"system","content":"Scrivi in italiano. Restituisci solo un oggetto JSON: premise (string), characters e locations (array di {name,description}), scenes (array di {title,description,dialogue,image_prompt,motion_prompt,duration_sec,camera}). Copri tutta la storia, conserva i dialoghi e crea 4-8 scene dettagliate. Rispetta la direzione visiva del progetto, se fornita. Nessun testo extra."},{"role":"user","content":format!("Direzione visiva del progetto: {}\n\nSoggetto: {}",creative_brief,request.premise)}]),json!("json"),4096).await?;
            let content=response.pointer("/message/content").and_then(Value::as_str).ok_or_else(||anyhow::anyhow!("Il provider LLM non ha restituito contenuto"))?;
            let value:Value=serde_json::from_str(content)?;
            anyhow::ensure!(value.get("scenes").and_then(Value::as_array).is_some_and(|s|!s.is_empty()),"Storyboard senza scene");
            Ok::<_,anyhow::Error>(value)
        }.await;
        match result {
            Ok(value) => {
                {
                    let mut jobs = app.jobs.write().await;
                    if let Some(j) = jobs.get_mut(&job_id) {
                        j.result = Some(value);
                    }
                }
                app.set_job(
                    job_id,
                    JobStatus::Succeeded,
                    "Bozza pronta: revisiona e applica dallo storyboard",
                    vec![],
                )
                .await;
            }
            Err(e) => {
                app.set_job(job_id, JobStatus::Failed, e.to_string(), vec![])
                    .await
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}

pub async fn models(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let config = state.config();
    Ok(Json(
        crate::providers::list_models(&state.http, &config).await?,
    ))
}
pub async fn settings(State(state): State<AppState>) -> Json<AppConfig> {
    Json(state.config())
}
pub async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Job>, ApiError> {
    state
        .jobs
        .read()
        .await
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::not_found("Lavoro assente"))
}

pub(super) async fn serve_file(
    path: PathBuf,
    request: Request<Body>,
) -> Result<Response, ApiError> {
    ServeFile::new(path)
        .oneshot(request)
        .await
        .map(IntoResponse::into_response)
        .map_err(|e| ApiError::internal(e.into()))
}
pub async fn asset_file(
    State(state): State<AppState>,
    Path((id, asset_id)): Path<(Uuid, Uuid)>,
    request: Request<Body>,
) -> Result<Response, ApiError> {
    let path = {
        let projects = state.projects.read().await;
        let project = projects
            .get(&id)
            .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
        let asset = project
            .assets
            .iter()
            .find(|a| a.id == asset_id)
            .ok_or_else(|| ApiError::not_found("Asset assente"))?;
        if FsPath::new(&asset.file_name).components().count() != 1 {
            return Err(ApiError::bad_request("Nome file non valido"));
        }
        store::project_dir(&state.data_dir, id)
            .join("assets")
            .join(&asset.file_name)
    };
    serve_file(path, request).await
}
pub async fn export_file(
    State(state): State<AppState>,
    Path((id, name)): Path<(Uuid, String)>,
    request: Request<Body>,
) -> Result<Response, ApiError> {
    if FsPath::new(&name).components().count() != 1 || name.contains(['/', '\\', ':']) {
        return Err(ApiError::bad_request("Nome export non valido"));
    }
    serve_file(
        store::project_dir(&state.data_dir, id)
            .join("exports")
            .join(name),
        request,
    )
    .await
}

pub fn normalise_transcript(asset_id: Uuid, value: &Value) -> anyhow::Result<Transcript> {
    fn convert(values: &[Value]) -> anyhow::Result<Vec<TranscriptSegment>> {
        let mut out = vec![];
        for segment in values {
            let time = |ms: &str, sec: &str| -> anyhow::Result<u64> {
                if let Some(v) = segment.get(ms).and_then(Value::as_u64) {
                    return Ok(v);
                }
                let seconds = segment
                    .get(sec)
                    .and_then(Value::as_f64)
                    .ok_or_else(|| anyhow::anyhow!("Timestamp assente"))?;
                anyhow::ensure!(
                    seconds.is_finite() && (0.0..=86400.0).contains(&seconds),
                    "Timestamp non valido"
                );
                Ok((seconds * 1000.0).round() as u64)
            };
            let start_ms = time("start_ms", "start")?;
            let end_ms = time("end_ms", "end")?;
            anyhow::ensure!(end_ms > start_ms, "Segmento con durata nulla o negativa");
            let text = segment
                .get("text")
                .or_else(|| segment.get("word"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned();
            if !text.is_empty() {
                out.push(TranscriptSegment {
                    start_ms,
                    end_ms,
                    text,
                });
            }
        }
        out.sort_by_key(|s| s.start_ms);
        Ok(out)
    }
    let segments = convert(
        value
            .get("segments")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("Manca l'array segments"))?,
    )?;
    anyhow::ensure!(!segments.is_empty(), "Trascrizione vuota");
    let words = if let Some(words) = value.get("words").and_then(Value::as_array) {
        convert(words)?
    } else {
        vec![]
    };
    Ok(Transcript {
        id: Uuid::new_v4(),
        asset_id,
        language: value
            .get("language")
            .and_then(Value::as_str)
            .unwrap_or("it")
            .into(),
        segments,
        words,
        imported_at: Utc::now(),
    })
}
#[derive(Deserialize)]
pub struct TranscriptRequest {
    pub asset_id: Uuid,
    pub transcript: Value,
}
pub async fn import_transcript(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<TranscriptRequest>,
) -> Result<Json<Transcript>, ApiError> {
    let transcript = normalise_transcript(request.asset_id, &request.transcript)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    update_project(&state, id, |p| {
        if !p.assets.iter().any(|a| a.id == request.asset_id) {
            return Err(ApiError::bad_request("Asset assente"));
        }
        crate::asr::retain_raw(p, transcript.clone());
        Ok(())
    })
    .await?;
    Ok(Json(transcript))
}
pub fn remap_transcript(source: &Transcript, asset_id: Uuid, ranges: &[(u64, u64)]) -> Transcript {
    fn remap(segments: &[TranscriptSegment], ranges: &[(u64, u64)]) -> Vec<TranscriptSegment> {
        let mut out = vec![];
        let mut offset = 0;
        for &(start, end) in ranges {
            for s in segments
                .iter()
                .filter(|s| s.end_ms > start && s.start_ms < end)
            {
                let from = s.start_ms.max(start);
                let to = s.end_ms.min(end);
                if to > from {
                    out.push(TranscriptSegment {
                        start_ms: from - start + offset,
                        end_ms: to - start + offset,
                        text: s.text.clone(),
                    });
                }
            }
            offset += end - start;
        }
        out
    }
    let words = remap(&source.words, ranges);
    let segments = if words.is_empty() {
        remap(&source.segments, ranges)
    } else {
        words
            .iter()
            .map(|w| TranscriptSegment {
                start_ms: w.start_ms,
                end_ms: w.end_ms,
                text: w.text.clone(),
            })
            .collect()
    };
    Transcript {
        id: Uuid::new_v4(),
        asset_id,
        language: source.language.clone(),
        segments,
        words,
        imported_at: Utc::now(),
    }
}

pub fn srt(transcript: &Transcript) -> String {
    fn timestamp(ms: u64) -> String {
        format!(
            "{:02}:{:02}:{:02},{:03}",
            ms / 3600000,
            (ms / 60000) % 60,
            (ms / 1000) % 60,
            ms % 1000
        )
    }
    transcript
        .segments
        .iter()
        .enumerate()
        .map(|(i, s)| {
            format!(
                "{}\n{} --> {}\n{}\n\n",
                i + 1,
                timestamp(s.start_ms),
                timestamp(s.end_ms),
                s.text
            )
        })
        .collect()
}
pub async fn subtitle_file(
    State(state): State<AppState>,
    Path((id, transcript_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    let projects = state.projects.read().await;
    let project = projects
        .get(&id)
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    let transcript = project
        .transcripts
        .iter()
        .find(|t| t.id == transcript_id)
        .ok_or_else(|| ApiError::not_found("Trascrizione assente"))?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-subrip; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=transcript.srt",
            ),
        ],
        srt(transcript),
    )
        .into_response())
}

pub async fn upload_asset(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    mut multipart: axum::extract::Multipart,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    use tokio::io::AsyncWriteExt;
    if !state.projects.read().await.contains_key(&id) {
        return Err(ApiError::not_found("Progetto assente"));
    }
    let root = store::project_dir(&state.data_dir, id);
    let mut field = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
        .ok_or_else(|| ApiError::bad_request("Seleziona un file"))?;
    let name = field
        .file_name()
        .ok_or_else(|| ApiError::bad_request("Campo file assente"))?
        .to_owned();
    let safe = media::safe_file_name(FsPath::new(&name))
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let filename = format!("{}-{safe}", Uuid::new_v4());
    let temporary = root
        .join("assets")
        .join(format!(".incoming-{}", Uuid::new_v4()));
    struct PartialUpload(PathBuf);
    impl Drop for PartialUpload {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let guard = PartialUpload(temporary.clone());
    let mut file = tokio::fs::File::create(&temporary)
        .await
        .map_err(|e| ApiError::internal(e.into()))?;
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
    {
        file.write_all(&chunk)
            .await
            .map_err(|e| ApiError::internal(e.into()))?;
    }
    file.flush()
        .await
        .map_err(|e| ApiError::internal(e.into()))?;
    drop(file);
    let destination = root.join("assets").join(&filename);
    tokio::fs::rename(&temporary, &destination)
        .await
        .map_err(|e| ApiError::internal(e.into()))?;
    drop(guard);
    let job = Job::queued(
        JobKind::ImportAsset,
        Some(id),
        "File caricato: verifica media in coda",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let job_id = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(job_id, JobStatus::Running, "Verifica file caricato", vec![])
            .await;
        let kind = media::asset_kind_for(&destination);
        let target = destination.clone();
        let result = tokio::task::spawn_blocking(move || {
            if matches!(kind, AssetKind::Video | AssetKind::Audio | AssetKind::Image) {
                media::probe_media(&target)
            } else {
                Ok(None)
            }
        })
        .await;
        match result {
            Ok(Ok(info)) => {
                let asset = Asset {
                    id: Uuid::new_v4(),
                    name,
                    kind,
                    file_name: filename,
                    source_path: "browser upload".into(),
                    imported_at: Utc::now(),
                    media: info,
                };
                match update_project(&app, id, |p| {
                    p.assets.push(asset);
                    Ok(())
                })
                .await
                {
                    Ok(()) => {
                        app.set_job(
                            job_id,
                            JobStatus::Succeeded,
                            "File importato",
                            vec![destination.to_string_lossy().into()],
                        )
                        .await
                    }
                    Err(e) => app.set_job(job_id, JobStatus::Failed, e.1, vec![]).await,
                }
            }
            Ok(Err(e)) => {
                app.set_job(job_id, JobStatus::Failed, e.to_string(), vec![])
                    .await
            }
            Err(e) => {
                app.set_job(job_id, JobStatus::Failed, e.to_string(), vec![])
                    .await
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}

pub fn import_generated(root: &FsPath, path: &FsPath) -> anyhow::Result<Asset> {
    let destination = media::copy_into_project(path, &root.join("assets"))?;
    let kind = media::asset_kind_for(&destination);
    let info = if matches!(kind, AssetKind::Video | AssetKind::Audio | AssetKind::Image) {
        media::probe_media(&destination)?
    } else {
        None
    };
    Ok(Asset {
        id: Uuid::new_v4(),
        name: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        kind,
        file_name: destination.file_name().unwrap().to_string_lossy().into(),
        source_path: path.to_string_lossy().into(),
        imported_at: Utc::now(),
        media: info,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn derived_transcript_follows_kept_ranges() {
        let original=normalise_transcript(Uuid::new_v4(),&json!({"segments":[{"start":1,"end":6,"text":"prima dopo"}],"words":[{"start":1,"end":2,"word":"prima"},{"start":5,"end":6,"word":"dopo"}]})).unwrap();
        let derived = remap_transcript(&original, Uuid::new_v4(), &[(1000, 2500), (4500, 6500)]);
        assert_eq!(derived.words[0].start_ms, 0);
        assert_eq!(derived.words[1].start_ms, 2000);
        assert_eq!(derived.words[1].end_ms, 3000);
        assert_eq!(derived.segments[1].text, "dopo");
    }
    #[test]
    fn transcript_preserves_words_and_srt_times() {
        let t=normalise_transcript(Uuid::new_v4(),&json!({"segments":[{"start":1.235,"end":2.5,"text":"Ciao mondo"}],"words":[{"start":1.235,"end":1.6,"word":"Ciao"}]})).unwrap();
        assert_eq!(t.words[0].start_ms, 1235);
        assert_eq!(srt(&t), "1\n00:00:01,235 --> 00:00:02,500\nCiao mondo\n\n");
        assert!(normalise_transcript(
            Uuid::new_v4(),
            &json!({"segments":[{"start":2,"end":1,"text":"bad"}]})
        )
        .is_err());
    }
    #[test]
    fn video_with_audio_can_be_detached_to_the_audio_track() {
        let mut p = Project::new("Test".into(), None);
        let asset = Asset {
            id: Uuid::new_v4(),
            name: "clip".into(),
            kind: AssetKind::Video,
            file_name: "clip.mp4".into(),
            source_path: String::new(),
            imported_at: Utc::now(),
            media: Some(MediaInfo {
                duration_ms: Some(10_000),
                width: Some(1920),
                height: Some(1080),
                frame_rate: Some(30.0),
                has_audio: true,
            }),
        };
        p.assets.push(asset.clone());
        let audio = p
            .timeline
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio)
            .unwrap()
            .id;
        p.timeline.clips.push(TimelineClip {
            id: Uuid::new_v4(),
            asset_id: asset.id,
            track_id: audio,
            start_ms: 0,
            trim_in_ms: 0,
            duration_ms: 2_000,
            label: None,
            volume: 1.0,
            opacity: 1.0,
            transform: VisualTransform::default(),
        });
        assert!(validate_timeline(&p, &p.timeline).is_ok());
        // A silent video has nothing to detach: the audio track must refuse it.
        p.assets[0].media.as_mut().unwrap().has_audio = false;
        assert!(validate_timeline(&p, &p.timeline).is_err());
    }
}
