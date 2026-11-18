//! Read-only production handoff over the existing project and narrative state.
use crate::{album, model::*, narrative, store, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

async fn snapshot(state: &AppState, id: Uuid) -> Result<Value, ApiError> {
    let p = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    let prod = narrative::load(state, &p)?;
    let mut narrative_data =
        serde_json::to_value(&prod).map_err(|e| ApiError::internal(e.into()))?;
    for (scene, row) in prod
        .scenes
        .iter()
        .zip(narrative_data["scenes"].as_array_mut().unwrap())
    {
        row["timeline_clip_id"] = json!(p
            .timeline
            .clips
            .iter()
            .find(|c| c.id == narrative::visual_clip_id(scene.id)
                && Some(c.asset_id) == scene.visual_asset_id)
            .map(|c| c.id));
    }
    let root = store::project_dir(&state.data_dir, id);
    fn uses(v: &Value, id: Uuid) -> bool {
        match v {
            Value::String(s) => s == &id.to_string(),
            Value::Array(a) => a.iter().any(|v| uses(v, id)),
            Value::Object(o) => o.values().any(|v| uses(v, id)),
            _ => false,
        }
    }
    let active = json!([
        p.timeline,
        p.motion,
        p.graphic_objects,
        p.montage.items,
        prod
    ]);
    let used = p
        .assets
        .iter()
        .filter(|a| uses(&active, a.id))
        .map(|a| a.id)
        .collect::<std::collections::HashSet<_>>();
    let assets=p.assets.iter().map(|a|{
        let safe=std::path::Path::new(&a.file_name).components().count()==1;
        let file=root.join("assets").join(&a.file_name);
        let bytes=if safe{std::fs::metadata(&file).ok().filter(|m|m.is_file()).map(|m|m.len())}else{None};
        let pertinent=a.kind!=AssetKind::Other && (used.contains(&a.id)||(used.is_empty()&&!p.montage.album.photos.get(&a.id).is_some_and(|photo|photo.excluded)));
        json!({"id":a.id,"name":a.name,"kind":a.kind,"file_name":a.file_name,"project_relative_path":format!("assets/{}",a.file_name),"import_source_path":a.source_path,"media":a.media,"bytes":bytes,"available":bytes.is_some(),"pertinent":pertinent,"package_path":format!("media/{}/{}",a.id,a.file_name),"provenance_notice":"Imported source location is audit information, not proof that content is authentic or AI-generated."})
    }).collect::<Vec<_>>();
    fn workflow_ids(v: &Value, ids: &mut std::collections::HashSet<Uuid>) {
        match v {
            Value::Object(m) => {
                for (k, v) in m {
                    if k == "workflow_id" {
                        if let Some(id) = v.as_str().and_then(|s| s.parse().ok()) {
                            ids.insert(id);
                        }
                    }
                    workflow_ids(v, ids);
                }
            }
            Value::Array(a) => {
                for v in a {
                    workflow_ids(v, ids)
                }
            }
            _ => (),
        }
    }
    let mut selected = std::collections::HashSet::new();
    workflow_ids(&json!([prod, p.graphic_objects]), &mut selected);
    let workflows=state.workflows.read().await.iter().filter(|w|selected.contains(&w.id)).map(|w|{
        let graph:Option<Value>=store::read_json(std::path::Path::new(&w.path)).ok();
        let controls=graph.as_ref().and_then(|g|crate::narrative_workflows::describe(w,g,None).ok());
        json!({"id":w.id,"summary":w,"api_graph":graph,"static_controls":controls,"compatibility":"Static graph inspection only. No live ComfyUI preflight or inference performed for this export; semantic identity/reference support is not certified."})
    }).collect::<Vec<_>>();
    let mut result = json!({"schema":"nl-production-package-v1","created_at":chrono::Utc::now(),"project_id":id,"project_title":p.title,"project_updated_at":p.updated_at,"project_root":crate::runtime::native_path(&root),"asset_bytes":assets.iter().filter_map(|a|a["bytes"].as_u64()).sum::<u64>(),"assets":assets,"album_script":album::script::rows(&p),"narrative":prod,"project":p,"workflows":workflows,"notices":["Snapshot of saved data only. No new inference, ASR, rendering or approval.","No AEP export, automatic After Effects reconstruction or frame-accurate AE equivalence promised.","Workflow bindings and parameters are saved selections, not model recommendations verified by this export.","Reference-only packages require the source project media; include-media packages copy only project-owned media, never model checkpoints or runtimes.","The dossier contains local source paths and production notes. Review before sharing."]});
    result["narrative"] = narrative_data;
    result["asset_bytes"] = json!(assets
        .iter()
        .filter(|a| a["pertinent"] == true)
        .filter_map(|a| a["bytes"].as_u64())
        .sum::<u64>());
    result["pertinent_asset_count"] =
        json!(assets.iter().filter(|a| a["pertinent"] == true).count());
    for row in result["album_script"].as_array_mut().unwrap() {
        row["timeline_clip_id"] = json!(p
            .timeline
            .clips
            .iter()
            .find(|c| c.id
                == crate::creative_montage::clip_id(
                    p.id,
                    serde_json::from_value(row["item_id"].clone()).unwrap()
                ))
            .map(|c| c.id));
        row["title"] = json!(p
            .montage
            .items
            .iter()
            .find(|i| json!(i.id) == row["item_id"])
            .map(|i| i.title.clone())
            .unwrap_or_default());
    }
    result["motion_titles"]=json!(p.motion.graphics.iter().map(|g|{
        let range=crate::motion::span(&p,g).map(|(_,start,end)|json!({"start_ms":start,"end_ms":end}));
        json!({"id":g.id,"name":g.name,"approved":g.approved,"effective_range":range,"text_layers":g.layers.iter().filter(|l|l.visible&&l.kind=="text").collect::<Vec<_>>(),"native_graphic":g})
    }).collect::<Vec<_>>());
    let mut needs = vec![];
    for workflow in &selected {
        if !workflows.iter().any(|w| w["id"] == json!(workflow)) {
            needs.push(json!({"kind":"workflow","id":workflow,"status":"missing_from_catalog"}));
        }
    }
    for workflow in &workflows {
        if workflow["api_graph"].is_null() {
            needs.push(json!({"kind":"workflow","id":workflow["id"],"name":workflow["summary"]["name"],"status":"missing_graph_file"}));
        }
    }
    for a in &assets {
        if a["pertinent"] == true && a["available"] == false {
            needs.push(json!({"kind":"missing_media","id":a["id"],"name":a["name"],"status":"missing_file"}));
        }
    }
    for scene in &prod.scenes {
        if scene.visual_asset_id.is_none() {
            needs.push(json!({"kind":"shot_visual","id":scene.id,"name":scene.title,"status":"not_generated_or_unassigned"}));
        } else if !scene.visual_approved {
            needs.push(json!({"kind":"shot_visual","id":scene.id,"name":scene.title,"status":"needs_review"}));
        }
        if scene.voice_mode != narrative::VoiceMode::None {
            if scene.voice_asset_id.is_none() {
                needs.push(json!({"kind":"shot_voice","id":scene.id,"name":scene.title,"status":if scene.voice_mode==narrative::VoiceMode::Generate{"not_generated"}else{"unassigned_manual_audio"}}));
            } else if !scene.voice_approved {
                needs.push(json!({"kind":"shot_voice","id":scene.id,"name":scene.title,"status":"needs_review"}));
            }
        }
    }
    for reference in &prod.references {
        if reference.asset_id.or(reference.source_asset_id).is_none() {
            needs.push(json!({"kind":"reference","id":reference.id,"name":reference.name,"status":"not_generated_or_unassigned"}));
        } else if !reference.approved {
            needs.push(json!({"kind":"reference","id":reference.id,"name":reference.name,"status":"needs_review"}));
        }
    }
    result["needs_attention"] = json!(needs);
    Ok(result)
}
pub async fn inspect(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(snapshot(&state, id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub expected_narrative_revision: u64,
    #[serde(default)]
    pub include_media: bool,
}
pub async fn export(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(r): Json<Request>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let mut data = snapshot(&state, id).await?;
    if data["project_updated_at"] != json!(r.expected_updated_at)
        || data["narrative"]["revision"] != json!(r.expected_narrative_revision)
    {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Progetto cambiato: rileggi il dossier prima di esportare".into(),
        ));
    }
    data["include_media"] = json!(r.include_media);
    let python = state.config().python_executable;
    if !python.is_file() {
        return Err(ApiError::bad_request("Il dossier ZIP usa zipfile del runtime condiviso già configurato; runtime non disponibile. Il manifest JSON rimane consultabile."));
    }
    let job = Job::queued(
        JobKind::ExportProductionPackage,
        Some(id),
        "Dossier di produzione in coda · nessuna generazione IA",
    );
    let receipt = store::project_dir(&state.data_dir, id)
        .join("analysis/production-packages")
        .join(format!("{}.json", job.id));
    std::fs::create_dir_all(receipt.parent().unwrap()).map_err(|e| ApiError::internal(e.into()))?;
    store::write_json(&receipt, &data).map_err(ApiError::internal)?;
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let jid = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(
            jid,
            JobStatus::Running,
            "Dossier: copione, shot, timeline, grafica e riferimenti salvati",
            vec![],
        )
        .await;
        let result = async {
            let exports = store::project_dir(&app.data_dir, id).join("exports");
            std::fs::create_dir_all(&exports)?;
            let output = exports.join(format!("produzione-{jid}.zip"));
            let mut cmd = tokio::process::Command::new(python);
            #[cfg(windows)]
            cmd.creation_flags(0x08000000);
            cmd.arg("-I")
                .arg("-c")
                .arg(include_str!("../bridge/production_package.py"))
                .arg(&receipt)
                .arg(&output)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            let mut child = cmd.spawn()?;
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut lines = BufReader::new(
                child
                    .stdout
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("stdout assente"))?,
            )
            .lines();
            while let Some(line) = lines.next_line().await? {
                if let Ok(progress) = serde_json::from_str::<Value>(&line) {
                    app.set_job(
                        jid,
                        JobStatus::Running,
                        format!(
                            "Dossier: media {}/{} esaminati",
                            progress["done"], progress["total"]
                        ),
                        vec![],
                    )
                    .await;
                }
            }
            let ended = child.wait_with_output().await?;
            anyhow::ensure!(
                ended.status.success(),
                "Dossier non creato: {}",
                String::from_utf8_lossy(&ended.stderr)
            );
            let bytes = std::fs::metadata(&output)?.len();
            anyhow::Ok((output, bytes))
        }
        .await;
        match result {
            Ok((output, bytes)) => {
                if let Some(j) = app.jobs.write().await.get_mut(&jid) {
                    j.result = Some(
                        json!({"bytes":bytes,"include_media":r.include_media,"snapshot_receipt":receipt,"aep":false,"live_comfy_preflight":false}),
                    );
                }
                app.set_job(
                    jid,
                    JobStatus::Succeeded,
                    "Dossier ZIP pronto: apri LEGGIMI.html; nessun progetto o media modificato",
                    vec![output.to_string_lossy().into()],
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
