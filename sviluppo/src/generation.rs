use crate::{media, model::*, store, update_project, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;
pub(crate) mod reference_audio;

/// The caller passes only downloads created by this job, never imported originals.
/// Move the completed file inside the same data volume rather than keeping two
/// copies of every generated video/audio asset indefinitely.
fn adopt_comfy_download(
    root: &std::path::Path,
    path: &std::path::Path,
    job_id: Uuid,
    asset_id: Uuid,
    remote_name: &str,
) -> anyhow::Result<Asset> {
    let filename = media::safe_file_name(path)?;
    let kind = media::asset_kind_for(path);
    let info = if matches!(kind, AssetKind::Video | AssetKind::Audio | AssetKind::Image) {
        media::probe_media(path)?
    } else {
        None
    };
    let directory = root.join("assets");
    std::fs::create_dir_all(&directory)?;
    let destination = crate::comfy_recovery::asset_path(root, remote_name, asset_id)?;
    if destination.is_file() {
        std::fs::remove_file(path)?;
    } else {
        std::fs::rename(path, &destination)?;
    }
    Ok(Asset {
        id: asset_id,
        name: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        kind,
        file_name: destination
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        source_path: format!("ComfyUI job {job_id}: {filename}"),
        imported_at: chrono::Utc::now(),
        media: info,
    })
}

pub async fn workflow_detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let workflows = state.workflows.read().await;
    let w = workflows
        .iter()
        .find(|w| w.id == id)
        .ok_or_else(|| ApiError::not_found("Workflow assente"))?;
    store::read_json(std::path::Path::new(&w.path))
        .map(Json)
        .map_err(ApiError::internal)
}

pub async fn submit_comfy(
    State(state): State<AppState>,
    Json(request): Json<ComfySubmitRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let path = request
        .workflow_path
        .canonicalize()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let known = state.workflows.read().await.iter().any(|w| {
        std::path::Path::new(&w.path)
            .canonicalize()
            .is_ok_and(|p| p == path)
    });
    if !known {
        return Err(ApiError::bad_request(
            "Scansiona e seleziona un workflow del catalogo",
        ));
    }
    if let Some(id) = request.project_id {
        if !state.projects.read().await.contains_key(&id) {
            return Err(ApiError::not_found("Progetto assente"));
        }
    }
    let mut workflow = media::patch_workflow_inputs(&path, &request.inputs)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let mut upload_assets = vec![];
    if request
        .asset_ranges
        .keys()
        .any(|k| !request.asset_inputs.contains_key(k))
    {
        return Err(ApiError::bad_request(
            "Intervallo audio senza file di reference assegnato",
        ));
    }
    if !request.asset_inputs.is_empty() {
        let id = request
            .project_id
            .ok_or_else(|| ApiError::bad_request("Scegli un progetto per usare le reference"))?;
        let projects = state.projects.read().await;
        let project = projects
            .get(&id)
            .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
        for (address, asset_id) in &request.asset_inputs {
            let asset = project
                .assets
                .iter()
                .find(|a| a.id == *asset_id)
                .ok_or_else(|| ApiError::bad_request("Reference assente"))?;
            if let Some(range) = request.asset_ranges.get(address) {
                reference_audio::validate(asset, *range)?;
            }
            let (node, input) = address
                .split_once('.')
                .ok_or_else(|| ApiError::bad_request("Indirizzo input non valido"))?;
            if workflow
                .get(node)
                .and_then(|n| n.get("inputs"))
                .and_then(|i| i.get(input))
                .is_none()
            {
                return Err(ApiError::bad_request(
                    "Input reference non presente nel workflow",
                ));
            }
            upload_assets.push((
                address.clone(),
                store::project_dir(&state.data_dir, id)
                    .join("assets")
                    .join(&asset.file_name),
                format!("suite-{}-{}", Uuid::new_v4(), asset.file_name),
            ));
        }
    }
    let job = Job::queued(
        JobKind::SubmitComfy,
        request.project_id,
        "Generazione ComfyUI in coda",
    );
    state.add_job(job.clone()).await?;
    let config = state.config();
    let app = state.clone();
    let job_id = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(
            job_id,
            JobStatus::Running,
            "Generazione ComfyUI: invio workflow",
            vec![],
        )
        .await;
        let result=async {
            let base=config.comfy_url.trim_end_matches('/');
            let definitions=crate::workflow_doctor::node_definitions_for(&app,&workflow).await?;
            let mut reference_loader_changes=vec![];
            for (address,range) in &request.asset_ranges { reference_loader_changes.extend(reference_audio::rebase_loader_trim(&mut workflow,address,*range)?); }
            let references=upload_assets.iter().map(|(address,_,_)|address.clone()).collect();
            let report=crate::workflow_doctor::analyze(&workflow,Some(&definitions),&references);
            let diagnostics=app.data_dir.join("generated").join(job_id.to_string());
            std::fs::create_dir_all(&diagnostics)?;
            store::write_json(&diagnostics.join("preflight.json"),&report)?;
            if !reference_loader_changes.is_empty(){ store::write_json(&diagnostics.join("reference-loader-changes.json"),&reference_loader_changes)?; }
            anyhow::ensure!(report.ready,"Preflight workflow non superato. Apri Generazione → Diagnostica e ripara.\n{}",report.error_summary());
            for address in request.asset_ranges.keys(){
                let(node,input)=address.split_once('.').ok_or_else(||anyhow::anyhow!("Indirizzo reference non valido"))?;
                let class=workflow[node]["class_type"].as_str().unwrap_or("");
                let spec=definitions[class]["input"]["required"].get(input).or_else(||definitions[class]["input"]["optional"].get(input));
                anyhow::ensure!(crate::narrative_workflows::reference_kind(class,input,spec)==Some(("audio",true)),"Intervallo audio richiesto su loader non verificato AUDIO: {address}");
            }
            let mut audio_references=vec![];
            for (address,mut path,mut filename) in upload_assets {
                if let Some(range)=request.asset_ranges.get(&address).copied(){
                    let pid=request.project_id.ok_or_else(||anyhow::anyhow!("Progetto reference assente"))?;
                    let owned=store::project_dir(&app.data_dir,pid).join("assets").canonicalize()?;let source=path.canonicalize()?;
                    anyhow::ensure!(source.starts_with(&owned),"Reference audio esterna ai media del progetto");
                    let root=store::project_dir(&app.data_dir,pid);let asset=request.asset_inputs[&address];
                    let (prepared,receipt)=tokio::task::spawn_blocking(move||reference_audio::prepare(&root,&source,asset,range)).await??;
                    path=prepared;filename=format!("suite-{}-excerpt.wav",Uuid::new_v4());audio_references.push(json!({"address":address,"receipt":receipt}));
                    store::write_json(&diagnostics.join("audio-references.json"),&audio_references)?;
                }
                // ComfyUI's /upload/image is its generic binary upload route,
                // including core LoadAudio/LoadVideo files. Stream, don't load
                // a whole reference movie into the suite's memory.
                let part=reqwest::multipart::Part::file(&path).await?.file_name(filename);
                let response=app.http.post(format!("{base}/upload/image")).timeout(Duration::from_secs(600))
                    .multipart(reqwest::multipart::Form::new().part("image",part).text("type","input").text("overwrite","false"))
                    .send().await?.error_for_status()?.json::<Value>().await?;
                let name=response.get("name").and_then(Value::as_str).ok_or_else(||anyhow::anyhow!("Upload reference privo di nome"))?;
                let sub=response.get("subfolder").and_then(Value::as_str).unwrap_or("");
                let value=if sub.is_empty(){name.to_owned()}else{format!("{sub}/{name}")};
                let (node,input)=address.split_once('.').unwrap();
                workflow[node]["inputs"][input]=json!(value);
            }
            // Recheck after uploads: a real reference must now be accepted by
            // ComfyUI; an upload does not justify ignoring unrelated failures.
            if !references.is_empty() {
                let definitions=crate::workflow_doctor::node_definitions_for(&app,&workflow).await?;
                let checked=crate::workflow_doctor::analyze(&workflow,Some(&definitions),&HashSet::new());
                store::write_json(&diagnostics.join("preflight-after-upload.json"),&checked)?;
                anyhow::ensure!(checked.ready,"Reference non verificata dopo upload: {}",checked.error_summary());
            }
            crate::comfy_recovery::record_intent(&app,job_id,base,&request,&workflow).await?;
            let response=app.http.post(format!("{base}/prompt")).timeout(Duration::from_secs(30))
                .json(&json!({"prompt":workflow,"client_id":job_id.to_string(),"extra_data":{"nl_suite_job_id":job_id.to_string()}})).send().await?;
            let status=response.status();let value=response.json::<Value>().await?;
            anyhow::ensure!(status.is_success(),"ComfyUI ({status}): {value}");
            let prompt_id=value.get("prompt_id").and_then(Value::as_str).ok_or_else(||anyhow::anyhow!("ComfyUI non ha restituito prompt_id"))?.to_owned();
            crate::comfy_recovery::record(&app,job_id,&prompt_id,base,&request).await?;
            let mut phase=String::new();
            // A 3 s request timeout (the client default) would fail a long paid
            // generation on one stall while ComfyUI loads a model or decodes.
            // Tolerate a bounded window of transport errors instead.
            const HISTORY_TIMEOUT: Duration = Duration::from_secs(20);
            const HISTORY_GRACE: Duration = Duration::from_secs(120);
            let mut trouble: Option<Instant> = None;
            let start=Instant::now();
            let mut entry=loop {
                anyhow::ensure!(start.elapsed()<Duration::from_secs(7200),"Timeout ComfyUI dopo 2 ore. Prompt {prompt_id} potrebbe essere ancora in esecuzione; controlla ComfyUI prima di rilanciare.");
                let mut fetched: Option<Value> = None;
                let mut failure: Option<String> = None;
                match app.http.get(format!("{base}/history/{prompt_id}")).timeout(HISTORY_TIMEOUT).send().await {
                    Ok(response) => match response.error_for_status() {
                        Ok(response) => match response.json::<Value>().await {
                            Ok(history) => fetched = Some(history),
                            Err(error) => failure = Some(error.to_string()),
                        },
                        Err(error) => failure = Some(error.to_string()),
                    },
                    Err(error) => failure = Some(error.to_string()),
                }
                if let Some(error) = failure {
                    let since = *trouble.get_or_insert_with(Instant::now);
                    anyhow::ensure!(since.elapsed()<HISTORY_GRACE,"ComfyUI non risponde da {}s: {error}. Prompt {prompt_id} ancora in sospeso: controlla ComfyUI prima di rilanciare.",since.elapsed().as_secs());
                    if phase!="connection" {phase="connection".into();app.set_job(job_id,JobStatus::Running,format!("ComfyUI momentaneamente non raggiungibile · nuovo tentativo · prompt {prompt_id}"),vec![]).await;}
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                trouble=None;
                let queue_json=match app.http.get(format!("{base}/queue")).timeout(Duration::from_secs(5)).send().await {
                    Ok(response) => response.json::<Value>().await.ok(),
                    Err(_) => None,
                };
                let contains=|key:&str| queue_json.as_ref().is_some_and(|queue|queue[key].as_array().is_some_and(|rows|rows.iter().any(|row|row.get(1).and_then(Value::as_str)==Some(prompt_id.as_str()))));
                let running=contains("queue_running");
                let pending=contains("queue_pending");
                if let Some(entry)=fetched.as_ref().and_then(|history|history.get(&prompt_id)) {
                    anyhow::ensure!(entry.pointer("/status/status_str").and_then(Value::as_str)!=Some("error"),"ComfyUI: {}",entry.get("status").unwrap_or(&Value::Null));
                    // Older ComfyUI builds omit status.completed; a non-empty
                    // outputs map for a prompt that left the queue is equivalent.
                    let completed=entry.pointer("/status/completed").and_then(Value::as_bool)==Some(true);
                    let settled=queue_json.is_some() && !running && !pending
                        && entry.get("outputs").and_then(Value::as_object).is_some_and(|outputs|!outputs.is_empty());
                    if completed||settled {break entry.clone();}
                }
                let mut message=format!("ComfyUI ha accettato il workflow · prompt {prompt_id} · attesa del risultato");
                if running {message=format!("ComfyUI in esecuzione · prompt {prompt_id}");}
                else if pending {message=format!("In attesa nella coda ComfyUI · prompt {prompt_id}");}
                if phase!=message {phase=message.clone();app.set_job(job_id,JobStatus::Running,message,vec![]).await;}
                tokio::time::sleep(Duration::from_secs(2)).await;
            };
            app.set_job(job_id,JobStatus::Running,"ComfyUI completato: trasferimento e verifica dei media nel progetto",vec![]).await;
            let root=request.project_id.map(|id|store::project_dir(&app.data_dir,id));
            let download_dir=app.data_dir.join("generated").join(job_id.to_string());
            tokio::fs::create_dir_all(&download_dir).await?;
            let mut assets=vec![];let mut paths=vec![];let mut seen=HashSet::new();
            let outputs=entry.get("outputs").and_then(Value::as_object).ok_or_else(||anyhow::anyhow!("ComfyUI completato senza output"))?;
            for node in outputs.values() {
                for collection in node.as_object().into_iter().flat_map(|o|o.values()).filter_map(Value::as_array) {
                    for file in collection {
                        let Some(name)=file.get("filename").and_then(Value::as_str) else {continue;};
                        let subfolder=file.get("subfolder").and_then(Value::as_str).unwrap_or("");
                        let kind=file.get("type").and_then(Value::as_str).unwrap_or("output");
                        if !seen.insert((name.to_owned(),subfolder.to_owned(),kind.to_owned())){continue;}
                        let filename=media::safe_file_name(std::path::Path::new(name))?;
                        let destination=media::unique_destination(&download_dir,&filename);
                        let mut response=app.http.get(format!("{base}/view")).query(&[("filename",name),("subfolder",subfolder),("type",kind)])
                            .timeout(Duration::from_secs(600)).send().await?.error_for_status()?;
                        let mut output=tokio::fs::File::create(&destination).await?;
                        while let Some(chunk)=response.chunk().await? {output.write_all(&chunk).await?;}
                        output.flush().await?;drop(output);
                        if let Some(root)=&root {let task_root=root.clone();let downloaded=destination.clone();let aid=crate::comfy_recovery::file_id(job_id,name,subfolder,kind);let remote_name=name.to_owned();let asset=tokio::task::spawn_blocking(move||adopt_comfy_download(&task_root,&downloaded,job_id,aid,&remote_name)).await??;paths.push(root.join("assets").join(&asset.file_name).to_string_lossy().into_owned());assets.push(asset);}
                        else {paths.push(destination.to_string_lossy().into_owned());}
                    }
                }
            }
            anyhow::ensure!(!paths.is_empty(),"ComfyUI completato senza file media scaricabili: {entry}");
            let review_asset=assets.iter().find(|a|a.kind==AssetKind::Image).map(|a|a.id);
            // Stable ownership proof for graphic-object review. Never infer an
            // asset from a filename or from another generation in the project.
            let imported_asset_ids=assets.iter().map(|a|a.id).collect::<Vec<_>>();
            if request.review_before_insert && request.placement.is_some() {
                anyhow::ensure!(review_asset.is_some(),"La generazione da revisionare non ha prodotto immagini");
            }
            if let Some(id)=request.project_id {
                // Register the media before any placement check: a rejected clip
                // must never lose or orphan a file ComfyUI already produced.
                update_project(&app,id,|p|{for a in &assets{if !p.assets.iter().any(|v|v.id==a.id){p.assets.push(a.clone());}}Ok(())}).await.map_err(|e|anyhow::anyhow!(e.1))?;
                if let Some(placement)=request.placement.as_ref().filter(|_|!request.review_before_insert) {
                    update_project(&app,id,|p|{
                        let mut next=p.clone();
                        let source=next.timeline.clips.iter().find(|c|c.id==placement.source_clip_id).ok_or_else(||ApiError::bad_request("Il clip sorgente della grafica è stato rimosso"))?.clone();
                        let image=assets.iter().find(|a|a.kind==AssetKind::Image).ok_or_else(||ApiError::bad_request("Il workflow selezionato non ha prodotto un'immagine"))?;
                        let track=next.timeline.tracks.iter().find(|t|t.kind==TrackKind::Overlay && !t.locked).ok_or_else(||ApiError::bad_request("Sblocca la traccia overlay"))?.id;
                        let duration_ms=placement.duration_ms.min(source.duration_ms.saturating_sub(placement.offset_ms));
                        if duration_ms==0{return Err(ApiError::bad_request("La grafica non rientra più nella durata del clip"));}
                        next.timeline.clips.push(TimelineClip{id:Uuid::new_v4(),asset_id:image.id,track_id:track,
                            start_ms:source.start_ms+placement.offset_ms,trim_in_ms:0,duration_ms,label:Some(placement.label.clone()),volume:0.0,opacity:1.0,transform:VisualTransform::default()});
                        *p=next;Ok(())
                    }).await.map_err(|e|anyhow::anyhow!("{} Il media generato resta nei media del progetto.",e.1))?;
                }
            }
            if request.review_before_insert {
                if let (Some(placement),Some(asset_id))=(&request.placement,review_asset) {
                    entry["_nl_suite"]=json!({"pending_placement":placement,"asset_id":asset_id,"approved":false});
                }
            }
            if !entry["_nl_suite"].is_object() { entry["_nl_suite"]=json!({}); }
            entry["_nl_suite"]["asset_ids"]=json!(imported_asset_ids);
            entry["_nl_suite"]["prompt_id"]=json!(prompt_id);
            entry["_nl_suite"]["recoverable"]=json!(true);
            entry["_nl_suite"]["audio_references"]=json!(audio_references);
            entry["_nl_suite"]["reference_loader_changes"]=json!(reference_loader_changes);
            Ok::<_,anyhow::Error>((paths,entry))
        }.await;
        match result {
            Ok((paths, entry)) => {
                {
                    let mut jobs = app.jobs.write().await;
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.result = Some(entry);
                    }
                }
                app.set_job(
                    job_id,
                    JobStatus::Succeeded,
                    "Generazione completata e output importati",
                    paths,
                )
                .await;
            }
            Err(e) => {
                tracing::error!(job_id=%job_id, error=%e, "ComfyUI job failed");
                app.set_job(job_id, JobStatus::Failed, e.to_string(), vec![])
                    .await
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}
