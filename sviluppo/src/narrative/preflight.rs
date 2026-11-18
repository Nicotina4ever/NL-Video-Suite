//! Read-only preparation on the same bindings and graph used by generation.
use super::*;

pub async fn check(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<GenerateRequest>,
) -> Result<Json<Value>, ApiError> {
    let (p, prod) = {
        let _guard = production_lock().lock().await;
        let p = project(&state, id).await?;
        let prod = load(&state, &p)?;
        (p, prod)
    };
    let ids = selected(&prod.scenes, &request.scene_ids)?;
    if ids.is_empty() || (!request.visuals && !request.voices) {
        return Err(bad("Seleziona almeno uno shot e un tipo di output"));
    }
    let workflows = state.workflows.read().await.clone();
    let mut graphs = serde_json::Map::new();
    for binding in prod
        .scenes
        .iter()
        .filter(|s| ids.contains(&s.id))
        .flat_map(|s| [s.visual_workflow.as_ref(), s.voice_workflow.as_ref()])
        .flatten()
    {
        if let Some(w) = workflows.iter().find(|w| w.id == binding.workflow_id) {
            if let Ok(Value::Object(graph)) =
                store::read_json::<Value>(std::path::Path::new(&w.path))
            {
                for (key, node) in graph {
                    graphs.insert(format!("{}-{key}", w.id), node);
                }
            }
        }
    }
    let definitions = crate::workflow_doctor::node_definitions_for(&state, &Value::Object(graphs))
        .await
        .ok();
    let base = state.config().comfy_url.trim_end_matches('/').to_owned();
    let queue = state
        .http
        .get(format!("{base}/queue"))
        .timeout(Duration::from_secs(5))
        .send()
        .await;
    let queue = match queue {
        Ok(v) if v.status().is_success() => v.json::<Value>().await.ok(),
        _ => None,
    };
    let online = definitions.is_some() && queue.is_some();
    let running = queue
        .as_ref()
        .and_then(|v| v["queue_running"].as_array())
        .map(Vec::len);
    let pending = queue
        .as_ref()
        .and_then(|v| v["queue_pending"].as_array())
        .map(Vec::len);
    let mut items = vec![];
    for scene in prod.scenes.iter().filter(|s| ids.contains(&s.id)) {
        for voice in [false, true] {
            if (voice && (!request.voices || scene.voice_mode != VoiceMode::Generate))
                || (!voice && !request.visuals)
            {
                continue;
            }
            let kind = if voice { "voice" } else { "visual" };
            let asset = if voice {
                scene.voice_asset_id
            } else {
                scene.visual_asset_id
            };
            if let Some(asset) = asset {
                items.push(json!({"scene_id":scene.id,"title":scene.title,"kind":kind,"status":"existing","asset_id":asset,"message":"Media già assegnato: non sarà rigenerato. Sgancia esplicitamente il risultato se vuoi sostituirlo; le reference restano collegate."}));
                continue;
            }
            let review_error = references::check_scene(&prod, scene).err().map(|e| e.1);
            let outcome=async {
                let binding=if voice{scene.voice_workflow.as_ref()}else{scene.visual_workflow.as_ref()}.ok_or_else(||bad("Scegli un workflow e il suo ingresso testuale"))?;
                let output=if voice{OutputKind::Audio}else if scene.visual_kind==VisualKind::Video{OutputKind::Video}else{OutputKind::Image};
                let text=references::bound_scene_prompt(&p,&prod,scene,voice).unwrap_or_else(|_|if voice{scene.narration.clone()}else{scene.visual_prompt.clone()});
                let submit=narrative_workflows::bind(&state,&p,binding,&text,output,definitions.as_ref()).await?;
                let mut graph:Value=store::read_json(&submit.workflow_path).map_err(ApiError::internal)?;
                // Apply exactly the explicit binding values; no upload or seed change.
                for (address,value) in &submit.inputs {let(node,key)=address.split_once('.').ok_or_else(||bad("Indirizzo input non valido"))?;graph[node]["inputs"][key]=value.clone();}
                let mut trim_changes=vec![];
                for (address,range) in &submit.asset_ranges {trim_changes.extend(generation::reference_audio::rebase_loader_trim(&mut graph,address,*range).map_err(|e|bad(e.to_string()))?);}
                let refs=submit.asset_inputs.keys().cloned().collect();
                let report=crate::workflow_doctor::analyze(&graph,definitions.as_ref(),&refs);
                Ok::<_,ApiError>(json!({"scene_id":scene.id,"title":scene.title,"kind":kind,"status":if report.ready && online && review_error.is_none(){"ready"}else{"blocked"},"workflow_id":binding.workflow_id,"message":review_error,"report":report,"loader_changes":trim_changes,"asset_inputs":submit.asset_inputs,"asset_ranges":submit.asset_ranges}))
            }.await;
            items.push(match outcome{Ok(v)=>v,Err(e)=>json!({"scene_id":scene.id,"title":scene.title,"kind":kind,"status":"blocked","message":e.1})});
        }
    }
    let count = items.iter().filter(|v| v["status"] != "existing").count();
    let ready = online
        && running == Some(0)
        && pending == Some(0)
        && count > 0
        && items
            .iter()
            .all(|v| v["status"] == "ready" || v["status"] == "existing");
    Ok(Json(
        json!({"ready":ready,"revision":prod.revision,"script_approved":prod.script_approved,"selected_count":ids.len(),"generation_count":count,"comfy":{"online":online,"running":running,"pending":pending},"items":items,"notice":"Preflight della suite sui binding salvati e object_info reale. Nessun upload, inferenza, modifica o prenotazione della GPU. La disponibilità viene ricontrollata prima dell’invio; qualità e lip-sync richiedono revisione."}),
    ))
}

pub(super) async fn require_idle(state: &AppState) -> Result<(), ApiError> {
    let url = format!("{}/queue", state.config().comfy_url.trim_end_matches('/'));
    let result=state.http.get(url).timeout(Duration::from_secs(5)).send().await.map_err(|_|bad("ComfyUI offline o non raggiungibile: verifica indirizzo e connessione prima di inviare"))?;
    let value = result
        .error_for_status()
        .map_err(|e| bad(e.to_string()))?
        .json::<Value>()
        .await
        .map_err(|e| bad(e.to_string()))?;
    if !["queue_running", "queue_pending"]
        .iter()
        .all(|key| value[*key].as_array().is_some_and(Vec::is_empty))
    {
        return Err(bad("ComfyUI occupato: nessun job inviato. Attendi i lavori presenti e ripeti il preflight; non verranno interrotti."));
    }
    Ok(())
}
