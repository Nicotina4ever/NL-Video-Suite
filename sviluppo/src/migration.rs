use crate::{editor, model::*, store, update_project, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf, process::Command};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ImportRequest {
    pub system: String,
    pub path: PathBuf,
    pub source_project_id: Option<u64>,
    pub asset_id: Option<Uuid>,
}

pub async fn import(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<ImportRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    if !matches!(
        request.system.as_str(),
        "regista" | "calliope" | "zp_shorts"
    ) {
        return Err(ApiError::bad_request("Sistema non supportato"));
    }
    let config = state.config();
    let source = config.engine_root.join(&request.system);
    let python = config.python_executable.canonicalize().map_err(|_| {
        ApiError::bad_request("Runtime condiviso assente: esegui Prepara_Runtime.cmd")
    })?;
    let source = if request.system == "zp_shorts" {
        source.canonicalize().map_err(|_| {
            ApiError::bad_request("Parser ZP interno assente: esegui Prepara_Runtime.cmd")
        })?
    } else {
        source
    };
    let path = request
        .path
        .canonicalize()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let root = store::project_dir(&state.data_dir, id);
    let media = {
        let projects = state.projects.read().await;
        let p = projects
            .get(&id)
            .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
        request
            .asset_id
            .and_then(|id| p.assets.iter().find(|a| a.id == id))
            .map(|a| root.join("assets").join(&a.file_name))
    };
    if request.system == "zp_shorts" && media.is_none() {
        return Err(ApiError::bad_request(
            "Seleziona l'asset sorgente della sessione ZP",
        ));
    }
    if request.system == "calliope" && request.source_project_id.is_none() {
        return Err(ApiError::bad_request("Inserisci l'ID progetto Calliope"));
    }
    let job = Job::queued(
        JobKind::Pipeline,
        Some(id),
        format!("Importazione {}", request.system),
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let job_id = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(
            job_id,
            JobStatus::Running,
            "Importazione progetto legacy",
            vec![],
        )
        .await;
        let task_root = root.clone();
        let task=tokio::task::spawn_blocking(move||->anyhow::Result<(Vec<Asset>,Vec<TimelineClip>,Story,Value)>{
            let directory=task_root.join("jobs").join(job_id.to_string());std::fs::create_dir_all(&directory)?;
            let script=directory.join("legacy_import.py");std::fs::write(&script,include_str!("../bridge/legacy_import.py"))?;
            let input=directory.join("request.json");let output=directory.join("legacy-snapshot.json");
            store::write_json(&input,&json!({"system":request.system,"path":path,"source_project_id":request.source_project_id,"media_path":media,"source_root":source}))?;
            let result=Command::new(python).arg(script).arg(input).arg(&output).env("PYTHONIOENCODING","utf-8").output()?;
            anyhow::ensure!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));
            let value:Value=store::read_json(&output)?;
            let mut story=value.get("story").cloned().unwrap_or(json!({}));
            for key in ["characters","locations"] {if let Some(entities)=story.get_mut(key).and_then(Value::as_array_mut){for entity in entities{entity["id"]=json!(Uuid::new_v4());}}}
            let story:Story=serde_json::from_value(story)?;
            let mut assets=vec![];let mut clips=vec![];let mut imported=HashMap::new();
            for item in value.get("clips").and_then(Value::as_array).into_iter().flatten(){
                let path=PathBuf::from(item["path"].as_str().ok_or_else(||anyhow::anyhow!("Clip senza path"))?);
                let asset_id=if let Some(asset_id)=imported.get(&path){*asset_id}else{
                    let asset=editor::import_generated(&task_root,&path)?;let asset_id=asset.id;assets.push(asset);imported.insert(path,asset_id);asset_id
                };
                let milliseconds=|key:&str|->anyhow::Result<u64>{let value=item[key].as_f64().ok_or_else(||anyhow::anyhow!("Timestamp assente: {key}"))?;anyhow::ensure!(value.is_finite() && (0.0..=86400.0).contains(&value),"Timestamp fuori limite");Ok((value*1000.0).round() as u64)};
                // Track IDs are rebound to the destination model before committing.
                let track=match item["track"].as_str().unwrap_or("video"){"audio"=>1,"overlay"=>2,_=>0};
                clips.push(TimelineClip{id:Uuid::new_v4(),asset_id,track_id:Uuid::from_u128(track),start_ms:milliseconds("start")?,trim_in_ms:milliseconds("trim")?,duration_ms:milliseconds("duration")?,label:item["label"].as_str().map(str::to_owned),volume:item["volume"].as_f64().unwrap_or(1.0),opacity:item["opacity"].as_f64().unwrap_or(1.0),transform:VisualTransform::default()});
            }
            for path in value["references"].as_array().into_iter().flatten().filter_map(Value::as_str){assets.push(editor::import_generated(&task_root,std::path::Path::new(path))?);}
            Ok((assets,clips,story,value))
        }).await;
        match task {
            Ok(Ok((assets, mut clips, story, value))) => {
                let saved = update_project(&app, id, |p| {
                    let mut next = p.clone();
                    next.assets.extend(assets);
                    for clip in &mut clips {
                        let kind = match clip.track_id.as_u128() {
                            1 => TrackKind::Audio,
                            2 => TrackKind::Overlay,
                            _ => TrackKind::Video,
                        };
                        clip.track_id = next
                            .timeline
                            .tracks
                            .iter()
                            .find(|t| t.kind == kind)
                            .ok_or_else(|| ApiError::bad_request("Traccia destinazione assente"))?
                            .id;
                    }
                    next.timeline.clips.extend(clips);
                    editor::validate_timeline(&next, &next.timeline)
                        .map_err(|e| ApiError::bad_request(e.to_string()))?;
                    if !story.premise.is_empty() || !story.scenes.is_empty() {
                        next.story = story;
                    }
                    *p = next;
                    Ok(())
                })
                .await;
                match saved {
                    Ok(()) => {
                        {
                            let mut jobs = app.jobs.write().await;
                            if let Some(j) = jobs.get_mut(&job_id) {
                                j.result = Some(value.clone());
                            }
                        }
                        app.set_job(
                            job_id,
                            JobStatus::Succeeded,
                            format!("Importazione completata; snapshot originale salvato nel progetto. {}",value.get("warnings").unwrap_or(&Value::Null)),
                            vec![],
                        )
                        .await;
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
