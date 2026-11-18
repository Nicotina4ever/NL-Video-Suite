//! Selective CPU alignment is opt-in and never replaces the original ASR.
use crate::{
    asr, model::*, store, subtitle_studio::Word, time_map::TimeRange, update_project, ApiError,
    AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
}
pub async fn align(
    State(state): State<AppState>,
    Path((id, uid)): Path<(Uuid, Uuid)>,
    Json(req): Json<Request>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let p = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    if p.updated_at != req.expected_updated_at {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Progetto modificato: salva e ricarica il cue".into(),
        ));
    }
    let unit = p
        .subtitles
        .units
        .iter()
        .find(|u| u.id == uid)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Cue assente"))?;
    if unit.translation_of.is_some() {
        return Err(ApiError::bad_request("Il livello tradotto conserva i tempi del cue: non può essere allineato a parole in un'altra lingua non pronunciate nell'audio"));
    }
    if !["it", "en", "es"].contains(&unit.language.as_str()) || unit.source.duration() > 60000 {
        return Err(ApiError::bad_request(
            "Allineamento selettivo: cue IT/EN/ES entro 60 secondi",
        ));
    }
    let config = asr::load(&state)?;
    asr::validate(&config)?;
    let runtime = state.config().python_executable;
    let asset = p
        .assets
        .iter()
        .find(|a| a.id == unit.asset_id)
        .ok_or_else(|| ApiError::not_found("Media assente"))?;
    let root = store::project_dir(&state.data_dir, id);
    let source = root.join("assets").join(&asset.file_name);
    let duration = asset
        .media
        .as_ref()
        .and_then(|m| m.duration_ms)
        .ok_or_else(|| ApiError::bad_request("Durata del media assente"))?;
    let job = Job::queued(
        JobKind::Pipeline,
        Some(id),
        "Allineamento selettivo del solo cue · Qwen CPU",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let jid = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(
            jid,
            JobStatus::Running,
            "Forced alignment locale del testo revisionato: l'ASR originale resta intatta",
            vec![],
        )
        .await;
        let input = unit.clone();
        let result=tokio::task::spawn_blocking(move||->anyhow::Result<Value>{let work=root.join("jobs").join(jid.to_string());std::fs::create_dir_all(&work)?;std::fs::write(work.join("align.py"),include_str!("../bridge/subtitle_align_bridge.py"))?;
 store::write_json(&work.join("request.json"),&json!({"config":config,"unit":input,"duration_ms":duration,"source":source,"work_dir":work,"cache_dir":root.join("analysis/selective_alignment"),"ffmpeg":std::env::current_dir()?.join("app/tools/ffmpeg.exe")}))?;
 let mut command=std::process::Command::new(runtime);#[cfg(windows)]{use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}let output=command.arg(work.join("align.py")).arg(work.join("request.json")).arg(work.join("result.json")).env("PYTHONIOENCODING","utf-8").env("PYTHONDONTWRITEBYTECODE","1").output()?;
 anyhow::ensure!(output.status.success(),"Allineamento: {}",String::from_utf8_lossy(&output.stderr));store::read_json(&work.join("result.json"))}).await;
        let value = match result {
            Ok(Ok(v)) => v,
            other => {
                let message = match other {
                    Ok(Err(e)) => e.to_string(),
                    Err(e) => e.to_string(),
                    _ => unreachable!(),
                };
                app.set_job(jid, JobStatus::Failed, message, vec![]).await;
                return;
            }
        };
        let saved = update_project(&app, id, |p| {
            let before = p.subtitles.clone();
            let current = p
                .subtitles
                .units
                .iter_mut()
                .find(|u| u.id == uid)
                .ok_or_else(|| ApiError::bad_request("Cue rimosso durante l'allineamento"))?;
            if current.text != unit.text
                || current.transcript_id != unit.transcript_id
                || current.source.start_ms != unit.source.start_ms
                || current.source.end_ms != unit.source.end_ms
                || !p.transcripts.iter().any(|t| t.id == unit.transcript_id)
            {
                return Err(ApiError(
                    StatusCode::CONFLICT,
                    "Testo o sorgente cambiati: il risultato non sostituisce la revisione corrente"
                        .into(),
                ));
            }
            let rows = value["words"]
                .as_array()
                .ok_or_else(|| ApiError::bad_request("Runner senza parole"))?;
            let mut words = vec![];
            for (i, w) in rows.iter().enumerate() {
                let from = w["start_ms"]
                    .as_u64()
                    .ok_or_else(|| ApiError::bad_request("Timing parola non valido"))?;
                let to = w["end_ms"]
                    .as_u64()
                    .ok_or_else(|| ApiError::bad_request("Timing parola non valido"))?;
                let text = w["text"]
                    .as_str()
                    .ok_or_else(|| ApiError::bad_request("Testo parola assente"))?;
                let source =
                    TimeRange::new(from, to).map_err(|e| ApiError::bad_request(e.to_string()))?;
                words.push(Word {
                    id: Uuid::new_v5(
                        &uid,
                        format!("aligned-v1:{i}:{from}:{to}:{text}").as_bytes(),
                    ),
                    text: text.into(),
                    source,
                });
            }
            if words.is_empty() {
                return Err(ApiError::bad_request("Nessuna parola allineata"));
            }
            current.source = TimeRange {
                start_ms: words.iter().map(|w| w.source.start_ms).min().unwrap_or(0),
                end_ms: words.iter().map(|w| w.source.end_ms).max().unwrap_or(0),
            };
            current.words = words;
            current.align_status = "aligned".into();
            current.alignment = Some(value["metadata"].clone());
            current.approved = false;
            p.subtitles.revision += 1;
            p.subtitle_history.record(&before, &p.subtitles);
            Ok(())
        })
        .await;
        match saved {
            Ok(()) => {
                if let Some(j) = app.jobs.write().await.get_mut(&jid) {
                    j.result = Some(
                        json!({"unit_id":uid,"cache_hit":value["cache_hit"],"metadata":value["metadata"],"raw_asr_changed":false}),
                    );
                }
                app.set_job(jid,JobStatus::Succeeded,"Cue riallineato: verifica ascolto e timing, poi approva. Nessun'altra parte è stata rianalizzata.",vec![]).await;
            }
            Err(e) => app.set_job(jid, JobStatus::Failed, e.1, vec![]).await,
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}
