use crate::{editor, media, model::*, store, update_project, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    process::{Command, Stdio},
};
use uuid::Uuid;

#[derive(Clone, Deserialize)]
pub struct PipelineRequest {
    pub asset_id: Uuid,
    pub operation: String,
    #[serde(default = "language")]
    pub language: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub items: Vec<Value>,
    #[serde(default)]
    pub burn_subs: bool,
    #[serde(default = "yes")]
    pub vertical: bool,
    #[serde(default = "mode")]
    pub mode: String,
}
fn language() -> String {
    "it".into()
}
fn mode() -> String {
    "2".into()
}
fn yes() -> bool {
    true
}

pub async fn run_pipeline(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<PipelineRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    validate_request(&request)?;
    let p = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    if !p
        .assets
        .iter()
        .any(|a| a.id == request.asset_id && matches!(a.kind, AssetKind::Audio | AssetKind::Video))
    {
        return Err(ApiError::bad_request(
            "Seleziona audio o video del progetto",
        ));
    }
    // Cached editorial work must remain usable even when a model is offline.
    if request.operation.starts_with("render_")
        || (request.operation != "transcribe" && crate::asr::active(&p, request.asset_id).is_some())
    {
        let mut request = request;
        if let Some(params) = request.params.as_object_mut() {
            params.remove("asr_profile_id");
            params.remove("asr_engine");
        }
        return run_ready(State(state), Path(id), Json(request)).await;
    }
    let explicit = request.params["asr_profile_id"]
        .as_str()
        .or_else(|| (request.params["asr_engine"] == "whisperx").then_some("whisper-default"));
    let profile = crate::asr_catalog::effective(&state, &p, explicit)?;
    profile.validate()?;
    if profile.engine == "whisperx" {
        if request.language != "it" {
            return Err(ApiError::bad_request("WhisperX configurato con allineamento italiano: scegli un profilo con adapter adatto alla lingua, nessun download automatico"));
        }
        let mut request = request;
        request.params["asr_profile_id"] = json!(profile.id);
        return run_ready(State(state), Path(id), Json(request)).await;
    }
    if !["it", "en", "es", "auto"].contains(&request.language.as_str()) {
        return Err(ApiError::bad_request("Lingua ASR non supportata"));
    }
    let (_, Json(child)) = Box::pin(crate::asr::transcribe(
        State(state.clone()),
        Path(id),
        Json(crate::asr::Request {
            asset_id: request.asset_id,
            engine: profile.engine.clone(),
            device: serde_json::from_value(
                request
                    .params
                    .get("asr_device")
                    .cloned()
                    .unwrap_or(json!("auto")),
            )
            .map_err(|_| ApiError::bad_request("Dispositivo ASR: auto, cuda o cpu"))?,
            profile_id: Some(profile.id.clone()),
            language: request.language.clone(),
        }),
    ))
    .await?;
    if request.operation == "transcribe" {
        return Ok((StatusCode::ACCEPTED, Json(child)));
    }
    let parent = Job::queued(
        JobKind::Pipeline,
        Some(id),
        "Analisi guidata: ASR selezionata → proposte",
    );
    let jid = parent.id;
    state.add_job(parent.clone()).await?;
    tokio::spawn(async move {
        // Never hold a worker permit while waiting for a child that needs it.
        let result=async{
            if let Some(j)=state.jobs.write().await.get_mut(&jid){j.result=Some(json!({"stage":"asr","child_id":child.id,"profile_id":profile.id}));}
            state.set_job(jid,JobStatus::Running,"Trascrizione con il profilo scelto",vec![]).await;
            let asr=wait_child(&state,child.id).await?;
            let current=state.projects.read().await.get(&id).cloned().ok_or_else(||ApiError::not_found("Progetto assente"))?;
            if crate::asr::active(&current,request.asset_id).map(|t|t.id.to_string())!=asr.result.as_ref().and_then(|v|v["transcript_id"].as_str().map(str::to_owned)) {
                return Err(ApiError::bad_request("La versione raw attiva è cambiata durante l'ASR: revisiona prima di continuare"));
            }
            let mut request=request;
            request.params.as_object_mut().map(|m|m.remove("asr_profile_id"));
            let (_,Json(analysis))=run_ready(State(state.clone()),Path(id),Json(request)).await?;
            if let Some(j)=state.jobs.write().await.get_mut(&jid){j.result=Some(json!({"stage":"analysis","asr_job_id":child.id,"child_id":analysis.id,"profile_id":profile.id}));}
            state.set_job(jid,JobStatus::Running,"ASR conservata · preparazione delle proposte",vec![]).await;
            wait_child(&state,analysis.id).await
        }.await;
        match result {
            Ok(done) => {
                if let Some(j) = state.jobs.write().await.get_mut(&jid) {
                    let chain = j.result.clone();
                    let mut result = done.result.unwrap_or_else(|| json!({}));
                    if let Some(object) = result.as_object_mut() {
                        object.insert("_nl_asr_chain".into(), chain.unwrap_or(Value::Null));
                    }
                    j.result = Some(result);
                }
                state
                    .set_job(
                        jid,
                        JobStatus::Succeeded,
                        "Analisi guidata completata con ASR selezionata",
                        done.output_paths,
                    )
                    .await;
            }
            Err(e) => state.set_job(jid, JobStatus::Failed, e.1, vec![]).await,
        }
    });
    Ok((StatusCode::ACCEPTED, Json(parent)))
}
async fn wait_child(state: &AppState, id: Uuid) -> Result<Job, ApiError> {
    loop {
        let job = state
            .jobs
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| ApiError::bad_request("Job figlio non più disponibile"))?;
        match job.status {
            JobStatus::Succeeded => return Ok(job),
            JobStatus::Failed => return Err(ApiError::bad_request(job.message)),
            _ => tokio::time::sleep(std::time::Duration::from_millis(250)).await,
        }
    }
}
fn validate_request(request: &PipelineRequest) -> Result<(), ApiError> {
    if !request.params.is_null() && !request.params.is_object() {
        return Err(ApiError::bad_request(
            "Parametri pipeline: oggetto JSON richiesto",
        ));
    }
    if !matches!(
        request.operation.as_str(),
        "transcribe"
            | "cleanup"
            | "clips"
            | "reels"
            | "shorts"
            | "render_clips"
            | "render_reels"
            | "render_shorts"
            | "render_cleanup"
    ) {
        return Err(ApiError::bad_request("Operazione pipeline non supportata"));
    }
    if !matches!(request.mode.as_str(), "1" | "2") {
        return Err(ApiError::bad_request("Modalità clip: 1 o 2"));
    }
    if request.operation.starts_with("render_") && request.items.is_empty() {
        return Err(ApiError::bad_request(
            "Seleziona almeno una proposta da renderizzare",
        ));
    }
    Ok(())
}
async fn run_ready(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<PipelineRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let root = store::project_dir(&state.data_dir, id);
    let (input, canonical_transcript, canonical_metadata) = {
        let projects = state.projects.read().await;
        let project = projects
            .get(&id)
            .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
        let asset = project
            .assets
            .iter()
            .find(|a| a.id == request.asset_id)
            .ok_or_else(|| ApiError::not_found("Asset assente"))?;
        if !matches!(asset.kind, AssetKind::Video | AssetKind::Audio) {
            return Err(ApiError::bad_request("Seleziona un video o audio"));
        }
        let transcript = crate::asr::active(project, asset.id).cloned();
        let metadata = transcript
            .as_ref()
            .and_then(|t| project.transcript_metadata.get(&t.id))
            .cloned();
        (
            root.join("assets").join(&asset.file_name),
            transcript,
            metadata,
        )
    };
    let mut config = state.config();
    let mut asr_cache_key = None;
    if let Some(id) = request.params["asr_profile_id"].as_str() {
        let profile = crate::asr_catalog::resolve(&state, id)?;
        profile.validate()?;
        if profile.engine != "whisperx" {
            return Err(ApiError::bad_request("Questo percorso usa WhisperX: trascrivi prima con il backend selezionato in Longform"));
        }
        asr_cache_key = Some(profile.cache_key(&input, &request.language));
        config.asr_model_path = profile.model;
        config.alignment_model_path = profile.aligner;
    }
    let requested: crate::asr::execution::Device = serde_json::from_value(
        request
            .params
            .get("asr_device")
            .cloned()
            .unwrap_or(json!("auto")),
    )
    .map_err(|_| ApiError::bad_request("Dispositivo ASR: auto, cuda o cpu"))?;
    let needs_asr = canonical_transcript.is_none() && !request.operation.starts_with("render_");
    let actual = if needs_asr {
        crate::asr::execution::resolve(&state, requested).await?
    } else {
        crate::asr::execution::Device::Cpu
    };
    let execution = crate::asr::execution::metadata(requested, actual);
    if let Some(mode) = request.params["llm_execution"].as_str() {
        config.ollama_execution = match mode {
            "cpu" => OllamaExecution::Cpu,
            "auto" => OllamaExecution::Auto,
            _ => return Err(ApiError::bad_request("Esecuzione IA: auto o cpu")),
        };
    }
    if request.params["llm_execution"].as_str().is_some() {
        config.resources.profile = "balanced".into();
        if request.operation != "transcribe" && !request.operation.starts_with("render_") {
            crate::resources::guard_llm(&state.http, &config, config.llm_for_role("clipping"))
                .await
                .map_err(|e| ApiError::bad_request(e.to_string()))?;
        }
    }
    let source = config.engine_root.join("clipsai");
    let python = config.python_executable.clone();
    if !python.is_file() {
        return Err(ApiError::bad_request(
            "Runtime condiviso assente: esegui Prepara_Runtime.cmd nella suite",
        ));
    }
    let source = source
        .canonicalize()
        .map_err(|_| ApiError::bad_request("Motori interni assenti: esegui Prepara_Runtime.cmd"))?;
    let python = python
        .canonicalize()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let mut job = Job::queued(
        JobKind::Pipeline,
        Some(id),
        format!(
            "In coda: {} · {}",
            request.operation,
            if canonical_transcript.is_none() && !request.operation.starts_with("render_") {
                format!("ASR {}", actual.label())
            } else {
                "trascrizione riusata, nessuna nuova ASR".into()
            }
        ),
    );
    job.result = Some(
        json!({"asset_id":request.asset_id,"operation":request.operation,"execution":execution}),
    );
    if request.operation == "transcribe" {
        if let Some(existing) = state.jobs.read().await.values().find(|j| {
            j.project_id == Some(id)
                && matches!(j.status, JobStatus::Queued | JobStatus::Running)
                && j.result.as_ref().is_some_and(|r| {
                    r["asset_id"] == json!(request.asset_id) && r["operation"] == "transcribe"
                })
        }) {
            return Ok((StatusCode::ACCEPTED, Json(existing.clone())));
        }
    }
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let job_id = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(
            job_id,
            JobStatus::Running,
            format!("ClipsAI: {}", request.operation),
            vec![],
        )
        .await;
        let mut work = root.join("analysis").join(request.asset_id.to_string());
        if request.operation == "transcribe" {
            if let Some(key) = asr_cache_key {
                work = work.join("asr").join(key);
            }
        }
        let task_dir = root.join("jobs").join(job_id.to_string());
        let operation = request.operation.clone();
        let render_items = request.items.clone();
        let render_operation = operation.clone();
        let render_work = if operation.starts_with("render_") {
            task_dir.join("output")
        } else {
            work.clone()
        };
        let payload = json!({"source_root":source,"input_path":input,"work_dir":render_work,
            "operation":request.operation,"language":request.language,"mode":request.mode,
            "progress_path":task_dir.join("progress.json"),"params":request.params,"model":config.llm_for_role("clipping"),"ollama_url":config.ollama_url,
            "ollama_execution":config.ollama_execution,
            "llm_provider":config.llm_provider,"openai_base_url":config.openai_base_url,
            "items":request.items,"vertical":request.vertical,"burn_subs":request.burn_subs,"device":actual.label(),"asr_execution":execution,
            "canonical_transcript":canonical_transcript,"canonical_metadata":canonical_metadata});
        let suite_root = std::env::current_dir().unwrap_or_default();
        let asr = suite_root.join(&config.asr_model_path);
        let asr_model = crate::runtime::native_path(&asr.canonicalize().unwrap_or(asr));
        let cache = suite_root.join(&config.model_cache_root);
        let model_cache = crate::runtime::native_path(&cache.canonicalize().unwrap_or(cache));
        let alignment = suite_root.join(&config.alignment_model_path);
        let alignment_model =
            crate::runtime::native_path(&alignment.canonicalize().unwrap_or(alignment));
        let task_dir_clone = task_dir.clone();
        let progress_state = app.clone();
        let progress_label = if canonical_transcript.is_none() && !operation.starts_with("render_")
        {
            format!("ASR {}", actual.label())
        } else if operation.starts_with("render_") {
            "Render".into()
        } else {
            format!(
                "Analisi IA {} · ASR riusata",
                match config.ollama_execution {
                    OllamaExecution::Cpu => "CPU",
                    OllamaExecution::Auto => "Auto GPU/CPU",
                }
            )
        };
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<Value> {
            fs::create_dir_all(&task_dir_clone)?;
            fs::create_dir_all(&render_work)?;
            // Reuse the aligned transcript for render without triggering new ASR.
            if operation.starts_with("render_") && work.exists() {
                for entry in fs::read_dir(&work)? {
                    let entry = entry?;
                    if entry
                        .file_name()
                        .to_string_lossy()
                        .ends_with("_trascrizione.json")
                    {
                        fs::copy(entry.path(), render_work.join(entry.file_name()))?;
                    }
                }
            }
            let script = task_dir_clone.join("bridge.py");
            fs::write(&script, include_str!("../bridge/clipsai_bridge.py"))?;
            let request_path = task_dir_clone.join("request.json");
            let response_path = task_dir_clone.join("result.json");
            store::write_json(&request_path, &payload)?;
            let log = fs::File::create(task_dir_clone.join("pipeline.log"))?;
            let mut child = Command::new(python)
                .arg(script)
                .arg(&request_path)
                .arg(&response_path)
                .env("PYTHONIOENCODING", "utf-8")
                .env("HF_HUB_OFFLINE", "1")
                .env("TRANSFORMERS_OFFLINE", "1")
                .env("CLIPSAI_WHISPER_MODEL", asr_model)
                .env("NL_ALIGNMENT_MODEL_PATH", alignment_model)
                .env("TORCH_HOME", model_cache.join("torch"))
                .env("HF_HOME", model_cache.join("huggingface"))
                .env("HF_HUB_CACHE", model_cache.join("huggingface/hub"))
                .env("PYTHONDONTWRITEBYTECODE", "1")
                .current_dir(&task_dir_clone)
                .stdout(Stdio::from(log.try_clone()?))
                .stderr(Stdio::from(log))
                .spawn()?;
            let mut last_stage = String::new();
            let status = loop {
                if let Some(status) = child.try_wait()? {
                    break status;
                }
                if let Ok(progress) =
                    store::read_json::<Value>(&task_dir_clone.join("progress.json"))
                {
                    if let Some(stage) = progress["stage"]
                        .as_str()
                        .filter(|v| !v.trim().is_empty() && **v != last_stage)
                    {
                        last_stage = stage.to_owned();
                        tokio::runtime::Handle::current().block_on(progress_state.set_job(
                            job_id,
                            JobStatus::Running,
                            format!("{progress_label} · {stage}"),
                            vec![],
                        ));
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            };
            if !status.success() {
                let log = fs::read_to_string(task_dir_clone.join("pipeline.log"))?;
                anyhow::bail!(
                    "ClipsAI: {}",
                    log.lines()
                        .rev()
                        .take(14)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join("\n")
                );
            }
            let mut value: Value = store::read_json(&response_path)?;
            if request.operation.starts_with("render_") {
                let mut outputs = vec![];
                for entry in fs::read_dir(&render_work)? {
                    let path = entry?.path();
                    if matches!(
                        media::asset_kind_for(&path),
                        AssetKind::Video | AssetKind::Subtitle
                    ) {
                        outputs.push(path);
                    }
                }
                value["output_files"] = serde_json::to_value(outputs)?;
            }
            Ok(value)
        })
        .await;
        match result {
            Ok(Ok(mut value)) => {
                value["_nl_suite_asset_id"] = json!(request.asset_id);
                let mut outputs = vec![];
                let mut assets = vec![];
                if let Some(files) = value.get("output_files").and_then(Value::as_array) {
                    for file in files.iter().filter_map(Value::as_str) {
                        match editor::import_generated(&root, std::path::Path::new(file)) {
                            Ok(asset) => {
                                outputs.push(
                                    root.join("assets")
                                        .join(&asset.file_name)
                                        .to_string_lossy()
                                        .to_string(),
                                );
                                assets.push(asset);
                            }
                            Err(e) => {
                                app.set_job(job_id, JobStatus::Failed, e.to_string(), outputs)
                                    .await;
                                return;
                            }
                        }
                    }
                }
                let transcript =
                    if value.get("_transcript").is_some() || value.get("segments").is_some() {
                        editor::normalise_transcript(
                            request.asset_id,
                            value.get("_transcript").unwrap_or(&value),
                        )
                        .ok()
                    } else {
                        None
                    };
                let saved = update_project(&app, id, |project| {
                    if let Some(transcript) = transcript {
                        let metadata=value.get("_nl_suite_canonical_metadata").filter(|v|v.is_object()).cloned().unwrap_or_else(||json!({"engine":"whisperx","adapter_version":"whisperx-profile-v2","alignment_completed":value.pointer("/_nl_suite_alignment/completed").and_then(Value::as_bool).unwrap_or(false),"model":config.asr_model_path,"models":{"model":{"path":config.asr_model_path},"aligner":{"path":config.alignment_model_path}},"parameters":{"language":request.language,"device":value.pointer("/_nl_suite_execution/device").and_then(Value::as_str).unwrap_or("unknown"),"requested_device":requested.label()},"execution":value.get("_nl_suite_execution").unwrap_or(&execution),"language":transcript.language,"confidence":null}));
                        crate::asr::save_result(project, transcript, metadata);
                    }
                    if let Some(source) = crate::asr::active(project, request.asset_id).cloned()
                    {
                        for asset in assets.iter().filter(|a| a.kind == AssetKind::Video) {
                            let index = std::path::Path::new(&asset.file_name)
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .split('_')
                                .find_map(|s| s.parse::<usize>().ok());
                            if let Some(item) = index
                                .and_then(|i| i.checked_sub(1))
                                .and_then(|i| render_items.get(i))
                            {
                                let raw = if render_operation == "render_cleanup" {
                                    vec![item.clone()]
                                } else if let Some(segments) =
                                    item.get("segments").and_then(Value::as_array)
                                {
                                    segments.clone()
                                } else {
                                    vec![serde_json::json!([item["start"], item["end"]])]
                                };
                                let ranges = raw
                                    .iter()
                                    .filter_map(|pair| {
                                        let a = pair[0].as_f64()?;
                                        let b = pair[1].as_f64()?;
                                        (a >= 0.0 && b > a).then_some((
                                            (a * 1000.0).round() as u64,
                                            (b * 1000.0).round() as u64,
                                        ))
                                    })
                                    .collect::<Vec<_>>();
                                if !ranges.is_empty() {
                                    project
                                        .transcripts
                                        .push(editor::remap_transcript(&source, asset.id, &ranges));
                                }
                            }
                        }
                    }
                    project.assets.extend(assets);
                    Ok(())
                })
                .await;
                match saved {
                    Ok(()) => {
                        {
                            let mut jobs = app.jobs.write().await;
                            if let Some(job) = jobs.get_mut(&job_id) {
                                job.result = Some(value);
                            }
                        }
                        app.set_job(
                            job_id,
                            JobStatus::Succeeded,
                            "Pipeline completata: apri le proposte nel progetto",
                            outputs,
                        )
                        .await;
                    }
                    Err(e) => app.set_job(job_id, JobStatus::Failed, e.1, outputs).await,
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
