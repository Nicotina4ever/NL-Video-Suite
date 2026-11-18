//! Versioned ASR provider boundary. Qwen is opt-in and uses configured external
//! GGUF files; WhisperX remains the default. Both produce the same transcript.
pub mod execution;
use crate::{editor, model::*, pipeline, store, update_project, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path as FsPath, PathBuf},
    time::Duration,
};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub runner: PathBuf,
    pub model: PathBuf,
    pub aligner: PathBuf,
    pub threads: u8,
    pub chunk_seconds: u16,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            runner: PathBuf::new(),
            model: PathBuf::new(),
            aligner: PathBuf::new(),
            threads: 4,
            chunk_seconds: 30,
        }
    }
}
fn bad(e: impl ToString) -> ApiError {
    ApiError::bad_request(e.to_string())
}
pub(super) fn load(state: &AppState) -> Result<Config, ApiError> {
    let path = state.data_dir.join("asr_providers.json");
    if path.is_file() {
        store::read_json(&path).map_err(ApiError::internal)
    } else {
        Ok(Config::default())
    }
}
pub(super) fn validate(config: &Config) -> Result<(), ApiError> {
    if config.schema_version != 1
        || !(1..=16).contains(&config.threads)
        || !(6..=60).contains(&config.chunk_seconds)
    {
        return Err(bad(
            "Config ASR: schema 1, da 1 a 16 thread, chunk da 6 a 60 secondi",
        ));
    }
    if !config.runner.is_file()
        || config
            .runner
            .extension()
            .and_then(|v| v.to_str())
            .is_none_or(|v| !v.eq_ignore_ascii_case("exe"))
    {
        return Err(bad(
            "Seleziona il runner Windows qwen3-asr-cli.exe già installato",
        ));
    }
    use std::io::Read;
    for path in [&config.model, &config.aligner] {
        let mut header = [0_u8; 4];
        let valid = std::fs::File::open(path)
            .and_then(|mut f| f.read_exact(&mut header))
            .is_ok()
            && header == *b"GGUF";
        if !valid {
            return Err(bad(format!(
                "File GGUF assente o non valido: {}",
                path.display()
            )));
        }
    }
    Ok(())
}
pub async fn config(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let cfg = load(&state)?;
    let ready = validate(&cfg);
    Ok(Json(
        json!({"schema_version":1,"config":cfg,"ready":ready.is_ok(),"notice":ready.err().map(|e|e.1),"whisperx_default":true,"qwen_experimental":true}),
    ))
}
pub async fn save(
    State(state): State<AppState>,
    Json(cfg): Json<Config>,
) -> Result<Json<Value>, ApiError> {
    validate(&cfg)?;
    store::write_json(&state.data_dir.join("asr_providers.json"), &cfg)
        .map_err(ApiError::internal)?;
    config(State(state)).await
}
pub async fn discover() -> Json<Value> {
    let root = FsPath::new("J:/ZLinkdaC/Qwen3ASR");
    let cfg = Config {
        runner: root.join("qwen3-asr-cli.exe"),
        model: root.join("models/qwen3-asr-1.7b-q8_0.gguf"),
        aligner: root.join("models/qwen3-forced-aligner-0.6b-q4_k_m.gguf"),
        ..Config::default()
    };
    Json(json!({"available":validate(&cfg).is_ok(),"candidate":cfg,"saved":false}))
}
pub async fn capabilities() -> Json<Value> {
    let rates=[(24000,1001),(24,1),(25,1),(30000,1001),(30,1),(50,1),(60000,1001),(60,1)].into_iter().map(|(numerator,denominator)|{
        let fps=crate::time_map::FrameRate{numerator,denominator};json!({"fps":fps,"frame_at_one_hour":fps.frame_at_ms(3_600_000).ok(),"round_trip_ms":fps.frame_at_ms(3_600_000).and_then(|f|fps.ms_at_frame(f)).ok()})
    }).collect::<Vec<_>>();
    Json(
        json!({"api_version":1,"time_unit":"integer_milliseconds","intervals":"half_open","time_mapping":"plural_bidirectional","frame_rate_contract":rates,"export_frame_rate":"selected_social_profile_or_30_fps_legacy","asr_providers":[{"id":"whisperx","default":true,"local":true},{"id":"qwen_gguf","experimental":true,"execution":"cpu","requires_explicit_config":true}],"native_motion_layers":["text","rect","ellipse","path","group","svg","image"],"font_catalog":"/api/v1/fonts","downloads":false}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub asset_id: Uuid,
    pub engine: String,
    #[serde(default)]
    pub device: execution::Device,
    pub profile_id: Option<String>,
    #[serde(default = "italian")]
    pub language: String,
}
fn italian() -> String {
    "it".into()
}
/// Legacy projects keep the first raw. Selection is editorial, not a model setting.
pub fn active(p: &Project, asset: Uuid) -> Option<&Transcript> {
    if let Some(id) = p.longform.active_transcripts.get(&asset) {
        return p
            .transcripts
            .iter()
            .find(|t| t.id == *id && t.asset_id == asset);
    }
    p.transcripts.iter().find(|t| t.asset_id == asset)
}
pub fn retain_raw(p: &mut Project, transcript: Transcript) {
    if !p.transcripts.iter().any(|t| t.id == transcript.id) {
        p.transcripts.push(transcript);
    }
}
pub fn save_result(p: &mut Project, mut t: Transcript, metadata: Value) -> Transcript {
    if let Some(old) = p.transcripts.iter().find(|old| {
        old.asset_id == t.asset_id
            && old.language == t.language
            && serde_json::to_value(&old.words).ok() == serde_json::to_value(&t.words).ok()
            && serde_json::to_value(&old.segments).ok() == serde_json::to_value(&t.segments).ok()
            && p.transcript_metadata.get(&old.id).is_some_and(|m| {
                ["engine", "model", "models", "parameters", "source_sha256"]
                    .iter()
                    .all(|k| m[*k] == metadata[*k])
            })
    }) {
        t = old.clone();
    }
    p.transcript_metadata.entry(t.id).or_insert(metadata);
    retain_raw(p, t.clone());
    t
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptSelection {
    pub asset_id: Uuid,
    pub transcript_id: Uuid,
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
}
pub async fn select_transcript(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<TranscriptSelection>,
) -> Result<Json<Project>, ApiError> {
    update_project(&s, id, |p| {
        if p.updated_at != req.expected_updated_at {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "Progetto aggiornato altrove: ricarica".into(),
            ));
        }
        if !p
            .transcripts
            .iter()
            .any(|t| t.id == req.transcript_id && t.asset_id == req.asset_id)
        {
            return Err(bad("Trascrizione estranea al media"));
        }
        p.longform
            .active_transcripts
            .insert(req.asset_id, req.transcript_id);
        p.longform.revision = p.longform.revision.saturating_add(1);
        Ok(())
    })
    .await?;
    Ok(Json(
        s.projects
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| bad("Progetto assente"))?,
    ))
}
pub async fn transcribe(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Request>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let profile = req
        .profile_id
        .as_deref()
        .map(|id| crate::asr_catalog::resolve(&state, id))
        .transpose()?;
    if let Some(p) = &profile {
        p.validate()?;
        if p.engine != req.engine {
            return Err(bad(
                "Il profilo selezionato appartiene a un backend diverso",
            ));
        }
    }
    if !["it", "en", "es", "auto"].contains(&req.language.as_str()) {
        return Err(bad("Lingua ASR: it, en, es o auto"));
    }
    if req.engine == "whisperx" {
        if req.language != "it" {
            return Err(bad("WhisperX portable ha l'allineatore italiano. Per un'altra lingua scegli un backend configurato che la supporti; nessun download implicito."));
        }
        return pipeline::run_pipeline(
            State(state),
            Path(id),
            Json(pipeline::PipelineRequest {
                asset_id: req.asset_id,
                operation: "transcribe".into(),
                language: req.language,
                params: json!({"asr_profile_id":req.profile_id,"asr_engine":"whisperx","asr_device":req.device}),
                items: vec![],
                burn_subs: false,
                vertical: false,
                mode: "2".into(),
            }),
        )
        .await;
    }
    if req.engine != "qwen_gguf" {
        return Err(bad("Backend ASR sconosciuto"));
    }
    if req.device == execution::Device::Cuda {
        return Err(bad("Il runner Qwen GGUF configurato usa CPU. Se vuoi la GPU seleziona WhisperX e un profilo Whisper installato; nessun backend è stato cambiato automaticamente."));
    }
    let cfg = profile
        .map(|p| p.qwen())
        .map(Ok)
        .unwrap_or_else(|| load(&state))?;
    validate(&cfg)?;
    let p = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    let asset = p
        .assets
        .iter()
        .find(|a| a.id == req.asset_id && matches!(a.kind, AssetKind::Audio | AssetKind::Video))
        .ok_or_else(|| bad("Scegli audio o video del progetto"))?;
    let duration = asset
        .media
        .as_ref()
        .and_then(|m| m.duration_ms)
        .ok_or_else(|| bad("Durata media non disponibile"))?;
    let root = store::project_dir(&state.data_dir, id)
        .join("assets")
        .canonicalize()
        .map_err(bad)?;
    let source = root.join(&asset.file_name).canonicalize().map_err(bad)?;
    if !source.starts_with(&root) {
        return Err(bad("Media fuori dal progetto"));
    }
    let config = state.config();
    if !config.python_executable.is_file() {
        return Err(bad("Runtime condiviso assente"));
    }
    let job = Job::queued(
        JobKind::Pipeline,
        Some(id),
        "ASR Qwen GGUF in CPU · in coda",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let jid = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        let work = store::project_dir(&app.data_dir, id)
            .join("jobs")
            .join(jid.to_string());
        let result=async{
            tokio::fs::create_dir_all(&work).await?;
            tokio::fs::write(work.join("qwen_bridge.py"),include_str!("../bridge/qwen_asr_bridge.py")).await?;
            let root=std::env::current_dir()?;
            let payload=json!({"config":cfg,"source":source,"duration_ms":duration,"work_dir":work,"cache_dir":store::project_dir(&app.data_dir,id).join("analysis/qwen"),"ffmpeg":root.join("app/tools/ffmpeg.exe"),"language":req.language,"reserve_ram_mib":config.resources.reserve_ram_mib});
            store::write_json(&work.join("request.json"),&payload)?;
            let log=std::fs::File::create(work.join("asr.log"))?;
            let mut command=tokio::process::Command::new(&config.python_executable);
            #[cfg(windows)]command.creation_flags(0x08000000);
            let mut child=command.arg(work.join("qwen_bridge.py")).arg(work.join("request.json")).arg(work.join("result.json")).env("PYTHONIOENCODING","utf-8").env("PYTHONDONTWRITEBYTECODE","1").stdout(log.try_clone()?).stderr(log).kill_on_drop(true).spawn()?;
            let status=loop{
                if let Some(status)=child.try_wait()?{break status;}
                let progress:Value=store::read_json(&work.join("progress.json")).unwrap_or(Value::Null);
                let stage=progress["stage"].as_str().unwrap_or("Avvio ASR Qwen GGUF in CPU");
                let message=if let(Some(done),Some(total))=(progress["completed_chunks"].as_u64(),progress["total_chunks"].as_u64()){format!("{stage} · {done}/{total} chunk")}else{stage.to_owned()};
                let changed=app.jobs.read().await.get(&jid).is_none_or(|j|j.message!=message);
                if changed{app.set_job(jid,JobStatus::Running,message,vec![]).await;}
                tokio::time::sleep(Duration::from_millis(500)).await;
            };
            let value:Value=store::read_json(&work.join("result.json"))?;
            anyhow::ensure!(status.success()&&value.get("error").is_none(),"Qwen: {}",value["error"].as_str().unwrap_or("runner terminato; vedi asr.log"));
            let mut transcript=editor::normalise_transcript(req.asset_id,&value)?;
            if req.language=="auto"{transcript.language="unknown".into();}
            // A cache hit preserves the raw transcript UUID and editorial word IDs.
            update_project(&app,id,|p|{
                transcript=save_result(p,transcript.clone(),value["metadata"].clone());Ok(())
            }).await.map_err(|e|anyhow::anyhow!(e.1))?;
            Ok::<_,anyhow::Error>(json!({"transcript_id":transcript.id,"word_count":transcript.words.len(),"metadata":value["metadata"],"cache_hit":value["cache_hit"]}))
        }.await;
        match result {
            Ok(value) => {
                if let Some(j) = app.jobs.write().await.get_mut(&jid) {
                    j.result = Some(value);
                }
                app.set_job(
                    jid,
                    JobStatus::Succeeded,
                    "Trascrizione e allineamento Qwen disponibili nel progetto",
                    vec![],
                )
                .await;
            }
            Err(e) => {
                app.set_job(jid, JobStatus::Failed, e.to_string(), vec![])
                    .await
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revisions_preserve_raw_and_provenance_even_when_words_match() {
        let asset = Uuid::new_v4();
        let mut p = Project::new("Raw".into(), None);
        let raw = json!({"language":"it","segments":[{"start":0,"end":1,"text":"Ciao"}]});
        let a = save_result(
            &mut p,
            editor::normalise_transcript(asset, &raw).unwrap(),
            json!({"engine":"whisperx","model":"a"}),
        );
        let repeat = save_result(
            &mut p,
            editor::normalise_transcript(asset, &raw).unwrap(),
            json!({"engine":"whisperx","model":"a","wall_seconds":2}),
        );
        assert_eq!(a.id, repeat.id);
        assert_eq!(p.transcripts.len(), 1);
        assert!(p.transcript_metadata[&a.id]["wall_seconds"].is_null());
        let b = save_result(
            &mut p,
            editor::normalise_transcript(asset, &raw).unwrap(),
            json!({"engine":"qwen3_asr_cpp","model":"b"}),
        );
        assert_ne!(a.id, b.id);
        assert_eq!(p.transcripts.len(), 2);
        assert_eq!(active(&p, asset).unwrap().id, a.id);
        p.longform.active_transcripts.insert(asset, b.id);
        assert_eq!(active(&p, asset).unwrap().id, b.id);
        assert_eq!(p.transcript_metadata[&a.id]["engine"], "whisperx");
        let restored: Project = serde_json::from_value(serde_json::to_value(&p).unwrap()).unwrap();
        assert_eq!(active(&restored, asset).unwrap().id, b.id);
    }
    #[test]
    fn legacy_edit_state_and_foreign_selection_do_not_fall_back_silently() {
        let state: crate::longform::EditState =
            serde_json::from_value(json!({"schema_version":1,"revision":0})).unwrap();
        assert!(state.active_transcripts.is_empty());
        assert!(state.asr_profile_id.is_none());
        let mut p = Project::new("Legacy".into(), None);
        let asset = Uuid::new_v4();
        retain_raw(
            &mut p,
            editor::normalise_transcript(
                asset,
                &json!({"segments":[{"start":0,"end":1,"text":"Old"}]}),
            )
            .unwrap(),
        );
        assert!(active(&p, asset).is_some());
        p.longform.active_transcripts.insert(asset, Uuid::new_v4());
        assert!(active(&p, asset).is_none());
    }
}
