//! Reviewed editing decisions, not another renderer or an autonomous generator.
use crate::{
    creative_montage::{Document, Item},
    model::*,
    providers, store, ApiError, AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use uuid::Uuid;
mod hierarchy;
mod review;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Plan {
    pub id: Uuid,
    pub based_on: String,
    pub summary: String,
    pub arc: String,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub technical_notes: Vec<String>,
    pub missing: Vec<Missing>,
    pub sequence: Vec<Item>,
    pub mode: String,
    pub model: Value,
    pub provider: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub applied: bool,
    #[serde(default)]
    pub narrative_fingerprint: Option<String>,
    #[serde(default)]
    pub model_summary: String,
    #[serde(default)]
    pub model_arc: String,
    #[serde(default)]
    pub requirements: review::Requirements,
    #[serde(default)]
    pub review_notes: Vec<review::Issue>,
    #[serde(default)]
    pub accepted_items: Vec<Item>,
    #[serde(default)]
    pub accepted_summary: String,
    #[serde(default)]
    pub request_instruction: String,
    #[serde(default)]
    pub request_brief: String,
    /// Original mandatory candidates, not invented or generated assets.
    /// A deficient model response remains reviewable without another inference.
    #[serde(default)]
    pub required_items: Vec<Item>,
    #[serde(default)]
    pub omitted_items: Vec<Item>,
    #[serde(default)]
    pub hierarchy: Vec<hierarchy::Chapter>,
    #[serde(default)]
    pub accepted_hierarchy: Vec<hierarchy::Chapter>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Missing {
    pub kind: String,
    pub description: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub asset_id: Uuid,
    pub at_ms: u64,
    pub description: String,
    pub model: Value,
    pub provider: Value,
    pub approved: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyzeRequest {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub item_ids: Vec<Uuid>,
}
pub async fn analyze(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(r): Json<AnalyzeRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let p = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| bad("Progetto assente"))?;
    if p.updated_at != r.expected_updated_at {
        return Err(bad("Salva e ricarica i materiali prima dell’analisi"));
    }
    if r.item_ids.is_empty()
        || r.item_ids.len() > 12
        || r.item_ids.iter().collect::<HashSet<_>>().len() != r.item_ids.len()
    {
        return Err(bad(
            "Seleziona da1 a12 elementi distinti per l’analisi facoltativa",
        ));
    }
    let config = state.config();
    if config
        .role_llm_models
        .get("vision")
        .is_none_or(|v| v.trim().is_empty())
    {
        return Err(bad("Assegna un modello visivo già installato al ruolo Analisi immagini in Sistema. Nessun download o scelta automatica."));
    }
    let items = r
        .item_ids
        .iter()
        .map(|id| {
            p.montage
                .items
                .iter()
                .find(|i| i.id == *id)
                .cloned()
                .ok_or_else(|| bad("Elemento estraneo alla sequenza"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    p.montage.validate(&p).map_err(bad)?;
    let job = Job::queued(
        JobKind::ProduceNarrative,
        Some(id),
        "Campioni visivi: analisi facoltativa in coda",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let jid = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        let result=async {
            let mut results=vec![];
            for (index,item) in items.iter().enumerate() {
                app.set_job(jid,JobStatus::Running,format!("Campione visivo {}/{} · non analisi dell’intero filmato",index+1,items.len()),vec![]).await;
                let asset=p.assets.iter().find(|a|a.id==item.asset_id).unwrap();let at=if asset.kind==AssetKind::Video{item.trim_in_ms}else{0};
                let root=store::project_dir(&app.data_dir,id).join("assets").canonicalize()?;let source=root.join(&asset.file_name).canonicalize()?;anyhow::ensure!(source.starts_with(root),"Media fuori progetto");
                let stat=std::fs::metadata(&source)?;let modified=stat.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_nanos();
                let key=Uuid::new_v5(&asset.id,format!("vision-sample-v1|{at}|{}|{modified}|{}|{:?}|{}|{}",stat.len(),config.llm_for_role("vision"),config.llm_provider,config.ollama_url,config.openai_base_url).as_bytes());
                let cache=store::project_dir(&app.data_dir,id).join("analysis/montage-vision").join(format!("{key}.json"));std::fs::create_dir_all(cache.parent().unwrap())?;
                let cached=cache.exists();let reply:Value=if cached{store::read_json(&cache)?}else{let reply=providers::visual_chat(&app.http,&config,source,at,asset.kind==AssetKind::Video,"Descrivi questo campione per aiutare un montatore a scegliere ordine e varietà. Segnala se il piano è stretto/medio/largo senza inventare eventi temporali. Non dedurre il contenuto da nomi di file.".into()).await?;store::write_json(&cache,&reply)?;reply};
                let text=reply.pointer("/message/content").and_then(Value::as_str).ok_or_else(||anyhow::anyhow!("Descrizione visiva assente"))?;anyhow::ensure!(!text.trim().is_empty()&&text.len()<=12000,"Descrizione visiva vuota o troppo estesa");
                // Persist each finished sample incrementally; interruption never loses
                // earlier evidence and a retry uses the raw per-source cache.
                let evidence=Evidence{asset_id:asset.id,at_ms:at,description:text.into(),model:reply["model"].clone(),provider:reply["provider"].clone(),approved:false};
                let mut all=app.projects.write().await;let current=all.get_mut(&id).ok_or_else(||anyhow::anyhow!("Progetto assente"))?;
                anyhow::ensure!(current.montage.items.iter().any(|i|i.id==item.id&&i.asset_id==item.asset_id&&i.trim_in_ms==item.trim_in_ms),"Materiale o campione cambiato: raw conservato, analisi non assegnata");
                let mut next=current.clone();next.montage.visual_evidence.insert(item.id,evidence);next.updated_at=chrono::Utc::now();app.save_project(&next).await.map_err(|e|anyhow::anyhow!(e.1))?;*current=next;
                results.push(json!({"item_id":item.id,"cache_hit":cached,"raw_path":cache}));if let Some(j)=app.jobs.write().await.get_mut(&jid){j.result=Some(json!({"samples":results,"whole_video_analysis":false}));}
            }Ok::<_,anyhow::Error>(())
        }.await;
        match result {
            Ok(()) => {
                app.set_job(
                    jid,
                    JobStatus::Succeeded,
                    "Campioni pronti da revisionare: approva le descrizioni prima del planner",
                    vec![],
                )
                .await
            }
            Err(e) => {
                app.set_job(jid, JobStatus::Failed, e.to_string(), vec![])
                    .await
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub mode: String,
    pub instruction: String,
    pub target_ms: u64,
    #[serde(default)]
    pub scope_ids: Vec<Uuid>,
    #[serde(default)]
    pub include_library: bool,
    #[serde(default)]
    pub use_continuity: bool,
    #[serde(default)]
    pub revision_fields: Vec<String>,
    #[serde(default = "default_audio_policy")]
    pub audio_policy: String,
    #[serde(default)]
    pub require_opening_title: bool,
    #[serde(default)]
    pub opening_title: String,
}
fn default_audio_policy() -> String {
    "preserve_original".into()
}
/// User claims are not observations. Prior model rationales must not feed back
/// as evidence or become new factual annotations on the original material.
fn context_item(i: &Item) -> Value {
    json!({"id":i.id,"asset_id":i.asset_id,"note":i.note})
}
fn revise_fields(source: &Item, proposed: &Item, fields: &[String]) -> Item {
    if fields.is_empty() {
        let mut result = proposed.clone();
        result.note = source.note.clone();
        return result;
    }
    let mut result = source.clone();
    for field in fields {
        match field.as_str() {
            "timing" => {
                result.duration_ms = proposed.duration_ms;
                result.trim_in_ms = proposed.trim_in_ms;
            }
            "movement" => {
                result.movement = proposed.movement.clone();
                result.fit = proposed.fit.clone();
            }
            "transitions" => result.transition_ms = proposed.transition_ms,
            "titles" => result.title = proposed.title.clone(),
            "tone" => {
                result.emotion = proposed.emotion.clone();
                result.section = proposed.section.clone();
            }
            "audio" => result.source_volume = proposed.source_volume,
            _ => (),
        }
    }
    result.reason = proposed.reason.clone();
    result
}
fn bad(s: impl ToString) -> ApiError {
    ApiError::bad_request(s.to_string())
}
fn narrative_fingerprint(v: &Value) -> Option<String> {
    if v.is_null() {
        None
    } else {
        Some(Uuid::new_v5(&Uuid::NAMESPACE_OID, v.to_string().as_bytes()).to_string())
    }
}
fn fingerprint(p: &Project) -> String {
    let mut d = p.montage.clone();
    d.plans.clear();
    let mut evidence = json!({"montage":d,"assets":p.assets,"transcripts":p.transcripts});
    if p.production_preset.is_some() {
        evidence["production_guidance"] = crate::production_preset::guidance(p);
    }
    Uuid::new_v5(&p.id, evidence.to_string().as_bytes()).to_string()
}
fn candidates(p: &Project, r: &Request) -> anyhow::Result<Vec<Item>> {
    anyhow::ensure!(
        r.scope_ids.iter().collect::<HashSet<_>>().len() == r.scope_ids.len(),
        "Selezione duplicata"
    );
    anyhow::ensure!(
        r.scope_ids
            .iter()
            .all(|id| p.montage.items.iter().any(|i| i.id == *id && !i.locked)),
        "La selezione contiene elementi bloccati o estranei"
    );
    let mut items = p
        .montage
        .items
        .iter()
        .filter(|i| {
            !i.locked
                && (r.scope_ids.is_empty() || r.scope_ids.contains(&i.id))
                && !p
                    .montage
                    .album
                    .photos
                    .get(&i.asset_id)
                    .is_some_and(|v| v.excluded)
        })
        .cloned()
        .collect::<Vec<_>>();
    if r.include_library {
        for a in p.assets.iter().filter(|a| {
            matches!(a.kind, AssetKind::Image | AssetKind::Video)
                && !p.montage.items.iter().any(|i| i.asset_id == a.id)
                && !p
                    .montage
                    .album
                    .photos
                    .get(&a.id)
                    .is_some_and(|v| v.excluded)
        }) {
            items.push(Item {
                id: Uuid::new_v5(&p.id, format!("planner-candidate:{}", a.id).as_bytes()),
                asset_id: a.id,
                fit: "auto".into(),
                duration_ms: a
                    .media
                    .as_ref()
                    .and_then(|m| m.duration_ms)
                    .unwrap_or(5000)
                    .min(5000),
                ..Item::default()
            });
        }
    }
    anyhow::ensure!(
        !items.is_empty(),
        "Scegli almeno un materiale sbloccato; sblocca esplicitamente quelli da rigenerare"
    );
    Ok(items)
}
fn validate_plan(p: &Project, plan: &Plan, candidates: &[Item]) -> anyhow::Result<()> {
    anyhow::ensure!(
        !plan.sequence.is_empty() && plan.sequence.len() <= candidates.len(),
        "Proposta vuota o con più occorrenze dei materiali disponibili"
    );
    let mut ids = HashSet::new();
    for item in &plan.sequence {
        anyhow::ensure!(
            ids.insert(item.id)
                && candidates
                    .iter()
                    .any(|c| c.id == item.id && c.asset_id == item.asset_id)
                && !item.locked,
            "Elemento duplicato, bloccato o media inventato"
        );
    }
    let d = Document {
        items: plan.sequence.clone(),
        ..Document::default()
    };
    d.validate(p)?;
    anyhow::ensure!(
        plan.summary.len() <= 1_048_576
            && plan.arc.len() <= 6000
            && plan.model_summary.len() <= 6000
            && plan.model_arc.len() <= 6000
            && plan.warnings.len() <= 20
            && plan.warnings.iter().all(|s| s.len() <= 2000)
            && plan.missing.len() <= 12,
        "Riepilogo troppo esteso"
    );
    for m in &plan.missing {
        anyhow::ensure!(
            ["image", "video", "narration", "sound", "title"].contains(&m.kind.as_str())
                && m.description.len() <= 4000
                && m.reason.len() <= 2000,
            "Risorsa mancante non valida"
        );
    }
    Ok(())
}
fn offline(p: &Project, r: &Request, items: &[Item]) -> Plan {
    let n = items.len();
    let mut sequence = items.to_vec();
    // Preserve editorial section precedence; among equal sections prefer variety
    // of source type and explicit user notes, never pretend to recognise pixels.
    sequence.sort_by_key(|i| match i.section.as_str() {
        "opening" => 0,
        "finale" => 2,
        _ => 1,
    });
    for i in 1..n.saturating_sub(1) {
        let previous = p
            .assets
            .iter()
            .find(|a| a.id == sequence[i - 1].asset_id)
            .unwrap()
            .kind;
        if let Some(j) = (i..n.saturating_sub(1)).find(|j| {
            p.assets
                .iter()
                .find(|a| a.id == sequence[*j].asset_id)
                .unwrap()
                .kind
                != previous
                && sequence[*j].section == sequence[i].section
        }) {
            sequence.swap(i, j);
        }
    }
    let weights = (0..n)
        .map(|i| {
            if i + 1 == n {
                1.25
            } else if i == 0 {
                1.1
            } else if i % 2 == 0 {
                0.85
            } else {
                1.0
            }
        })
        .collect::<Vec<_>>();
    let sum = weights.iter().sum::<f64>();
    let mut cursor = 0;
    for (index, item) in sequence.iter_mut().enumerate() {
        let a = p.assets.iter().find(|a| a.id == item.asset_id).unwrap();
        let available = if a.kind == AssetKind::Video {
            a.media
                .as_ref()
                .and_then(|m| m.duration_ms)
                .unwrap_or(0)
                .saturating_sub(item.trim_in_ms)
        } else {
            600000
        };
        let desired = (r.target_ms as f64 * weights[index] / sum) as u64;
        let beat = p
            .montage
            .rhythm_markers_ms
            .iter()
            .filter(|b| **b > cursor + 500)
            .min_by_key(|b| b.abs_diff(cursor + desired));
        let duration = beat
            .filter(|b| b.abs_diff(cursor + desired) <= desired / 7)
            .map(|b| b - cursor)
            .unwrap_or(desired);
        item.duration_ms = duration.clamp(500, 600000).min(available);
        item.section = if index == 0 {
            "opening"
        } else if index + 1 == n {
            "finale"
        } else {
            "development"
        }
        .into();
        item.movement = if a.kind == AssetKind::Image {
            if index + 1 == n {
                "pull"
            } else {
                "push"
            }
        } else {
            "still"
        }
        .into();
        item.transition_ms = Some(if a.kind == AssetKind::Video { 0 } else { 300 });
        item.reason="Bozza offline: alternanza del tipo di media, respiro iniziale/finale e pulsazioni solo se vicine. Contenuto da verificare nelle note.".into();
        item.emotion = if index + 1 == n {
            "Risoluzione da verificare"
        } else {
            "Intenzione da definire"
        }
        .into();
        cursor += item.duration_ms;
    }
    Plan{id:Uuid::new_v4(),based_on:fingerprint(p),summary:"Bozza offline revisionabile, non analisi semantica o visione artificiale.".into(),arc:p.montage.brief.clone(),warnings:vec!["Contenuto visivo, varietà delle inquadrature e andamento emotivo non riconosciuti automaticamente. Aggiungi note e usa il provider per una regia contestuale.".into(),"Durata obiettivo indicativa: vincoli delle sorgenti e sovrapposizioni possono ridurla. Nessun clip escluso o generato automaticamente.".into()],missing:vec![],sequence,mode:"offline".into(),model:Value::Null,provider:Value::Null,created_at:chrono::Utc::now(),applied:false,narrative_fingerprint:None,technical_notes:vec![],..Default::default()}
}
fn technical_notes(p: &Project, plan: &Plan, candidates: &[Item], target: u64) -> Vec<String> {
    let proposed = plan.sequence.iter().map(|i| i.duration_ms).sum::<u64>();
    let overlaps = plan
        .sequence
        .windows(2)
        .map(|w| {
            w[1].transition_ms
                .unwrap_or(p.montage.transition_ms)
                .min(w[0].duration_ms / 3)
                .min(w[1].duration_ms / 3)
        })
        .sum::<u64>();
    let duration = proposed.saturating_sub(overlaps);
    let mut notes=vec![format!("Controllo locale: {} elementi proposti, circa{:.2}s prima di eventuali rifiniture. Obiettivo{:.2}s; il riepilogo testuale NON è una misura del montaggio.",plan.sequence.len(),duration as f64/1000.,target as f64/1000.)];
    let source_audio = plan
        .sequence
        .iter()
        .filter(|i| {
            i.source_volume > 0.
                && p.assets
                    .iter()
                    .any(|a| a.id == i.asset_id && a.media.as_ref().is_some_and(|m| m.has_audio))
        })
        .count();
    notes.push(format!("Inventario strutturato: {source_audio} elementi con audio sorgente attivo; narrazione separata {}. Emozioni/motivazioni sono ipotesi creative; note utente e descrizioni IA approvate non sono verità indipendenti.",if p.montage.narration_asset_id.is_some(){"presente"}else{"assente"}));
    if plan
        .sequence
        .windows(2)
        .any(|w| w[0].asset_id == w[1].asset_id)
    {
        notes.push("La proposta riusa lo stesso asset in elementi adiacenti: verifica ripetizione e intenzione; nessuna esclusione automatica.".into());
    }
    let omitted = p
        .montage
        .items
        .iter()
        .filter(|i| {
            candidates.iter().any(|c| c.id == i.id) && !plan.sequence.iter().any(|c| c.id == i.id)
        })
        .count();
    if omitted > 0 {
        notes.push(format!("{omitted} elementi esistenti non sono proposti: resteranno nella sequenza. Se vuoi escluderli, rimuovili esplicitamente dalla sequenza, senza cancellare i media."));
    }
    if !plan.sequence.iter().any(|i| {
        p.assets
            .iter()
            .any(|a| a.id == i.asset_id && a.kind == AssetKind::Video)
    }) {
        notes.push("La proposta strutturata non contiene video: eventuali video citati nel riepilogo NON sono stati pianificati.".into());
    }
    if plan.sequence.iter().all(|i| i.title.trim().is_empty()) {
        notes.push("Nessun titolo strutturato: citarlo nel riepilogo non crea un livello Motion. Puoi aggiungerlo nei campi della proposta.".into());
    }
    if plan.mode == "ai"
        && plan
            .sequence
            .iter()
            .any(|i| i.reason.contains("Bozza offline") || i.emotion.contains("da definire"))
    {
        notes.push("Il modello ha ripreso motivazioni/segnaposto della bozza offline: rivedi la regia, non è una nuova analisi semantica verificata.".into());
    }
    notes
}
fn schema(items: &[Item], required_count: usize) -> Value {
    let fields = json!({"id":{"type":"string","enum":items.iter().map(|i|i.id.to_string()).collect::<Vec<_>>()},"duration_ms":{"type":"integer","minimum":500,"maximum":600000},"trim_in_ms":{"type":"integer","minimum":0},"section":{"type":"string","enum":["opening","development","finale"]},"movement":{"type":"string","enum":["still","push","pull","pan_left","pan_right"]},"fit":{"type":"string","enum":["auto","blur","contain","cover"]},"transition_ms":{"type":"integer","minimum":0,"maximum":3000},"source_volume":{"type":"number","minimum":0,"maximum":2},"title":{"type":"string"},"emotion":{"type":"string"},"reason":{"type":"string","minLength":1}});
    let required = fields
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    json!({"type":"object","additionalProperties":false,"properties":{"summary":{"type":"string"},"arc":{"type":"string"},"warnings":{"type":"array","maxItems":20,"items":{"type":"string"}},"missing":{"type":"array","maxItems":12,"items":{"type":"object","additionalProperties":false,"properties":{"kind":{"type":"string","enum":["image","video","narration","sound","title"]},"description":{"type":"string"},"reason":{"type":"string"}},"required":["kind","description","reason"]}},"sequence":{"type":"array","minItems":required_count.max(1),"maxItems":items.len(),"items":{"type":"object","additionalProperties":false,"properties":fields,"required":required}}},"required":["summary","arc","warnings","missing","sequence"]})
}
/// A repeated edit ID is not a new occurrence. Retain the first known source,
/// disclose discarded repetitions, and keep the immutable raw for review.
fn decode_sequence(v: &Value, items: &[Item]) -> anyhow::Result<(Vec<Item>, Vec<Uuid>)> {
    let entries = v["sequence"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Sequenza assente"))?;
    let mut sequence = vec![];
    let mut duplicates = vec![];
    let mut seen = HashSet::new();
    for entry in entries {
        let id: Uuid = serde_json::from_value(entry["id"].clone())?;
        let source = items.iter().find(|i| i.id == id).ok_or_else(|| {
            anyhow::anyhow!("ID estraneo alla selezione: nessun materiale inventato accettato")
        })?;
        if !seen.insert(id) {
            duplicates.push(id);
            continue;
        }
        let mut value = entry.clone();
        value["asset_id"] = json!(source.asset_id);
        value["note"] = json!(source.note);
        value["locked"] = json!(false);
        sequence.push(serde_json::from_value(value)?);
    }
    Ok((sequence, duplicates))
}
/// Do not resend all word tokens/58 minutes of ASR for a five-second source
/// range. The explicit window is evidence scope, not a claim about unseen speech.
fn transcript_window(p: &Project, asset: Uuid, start: u64, end: u64) -> Vec<Value> {
    crate::asr::active(p,asset).into_iter().map(|t|json!({"transcript_id":t.id,"language":t.language,"window_start_ms":start,"window_end_ms":end,"scope":"Only overlapping cached segments of the explicitly active raw; word alignment and speech outside this range not included","segments":t.segments.iter().filter(|s|s.end_ms>start&&s.start_ms<end).collect::<Vec<_>>()})).collect()
}
pub async fn propose(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(r): Json<Request>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let p = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| bad("Progetto assente"))?;
    if p.updated_at != r.expected_updated_at {
        return Err(bad("Progetto cambiato: salva e ricarica"));
    }
    if !["offline", "ai"].contains(&r.mode.as_str())
        || r.instruction.len() > 6000
        || !(1000..=3600000).contains(&r.target_ms)
    {
        return Err(bad("Modalità, richiesta o durata obiettivo non valida"));
    }
    if !r.revision_fields.iter().all(|f| {
        [
            "order",
            "timing",
            "movement",
            "transitions",
            "titles",
            "tone",
            "audio",
        ]
        .contains(&f.as_str())
    }) || (!r.revision_fields.is_empty() && r.include_library)
    {
        return Err(bad("Revisione mirata: scegli campi supportati e materiali già nella sequenza, senza aggiungere la libreria"));
    }
    p.montage.validate(&p).map_err(bad)?;
    if !["preserve_original", "mute_sources", "keep_current"].contains(&r.audio_policy.as_str())
        || r.opening_title.chars().count() > 160
    {
        return Err(bad("Regola audio o titolo iniziale non valido"));
    }
    let items = candidates(&p, &r).map_err(bad)?;
    let narrative_path = store::project_dir(&state.data_dir, id).join("narrative.json");
    let narrative: Value = if narrative_path.exists() {
        store::read_json(&narrative_path).map_err(ApiError::internal)?
    } else {
        Value::Null
    };
    let continuity = if r.use_continuity {
        json!({"continuity":narrative["continuity"],"scenes":narrative["scenes"],"story":p.story})
    } else {
        Value::Null
    };
    let mut context = json!({"version":"montage-planner-v3-decisions","revision_fields":r.revision_fields,"requirements":{"audio_policy":r.audio_policy,"require_opening_title":r.require_opening_title,"opening_title_text":r.opening_title,"nonempty_reason_for_every_item":true,"summary_is_untrusted_commentary":"Server derives actual order/duration/audio/titles from the structured decisions"},"evidence_contract":{"observed_facts":"Only technical metadata are measured. No pixels or audio are supplied to this text call.","user_claims":"item.note, reference user_claims and audio user_note: provided by user, not independently verified","model_observations":"approved visual_sample is a fallible model description of ONE sample, not identity or events across a video","creative_hypotheses":"reason/emotion/arc/missing: proposals, never facts about originals","reference_analysis":"unapproved reference analysis is not evidence; identity references do not imply objects/events in a story"},"brief":p.montage.brief,"instruction":r.instruction,"target_ms":r.target_ms,"delivery_aspect_ratio":p.aspect_ratio,"candidates":items.iter().map(|i|{
        let a=p.assets.iter().find(|a|a.id==i.asset_id).unwrap();
        let refs=narrative["references"].as_array().into_iter().flatten().filter(|v|v["source_asset_id"]==json!(a.id)&&v["use_for_story"]==true).map(|v|json!({"role":v["kind"],"user_claims":v["notes"],"approved_analysis":if v["approved"]==true{v["analysis"].clone()}else{Value::Null},"approved":v["approved"]})).collect::<Vec<_>>();
        json!({"item":context_item(i),"current_edit_not_a_proposal":{"duration_ms":i.duration_ms,"trim_in_ms":i.trim_in_ms,"movement":i.movement,"fit":i.fit,"source_volume":i.source_volume,"title":i.title},"kind":a.kind,"media":a.media,"visual_sample":p.montage.visual_evidence.get(&i.id).filter(|e|e.approved&&e.asset_id==i.asset_id&&(a.kind!=AssetKind::Video||e.at_ms==i.trim_in_ms)),"references":refs,"cached_transcripts":transcript_window(&p,a.id,i.trim_in_ms,i.trim_in_ms.saturating_add(i.duration_ms)),"evidence":"User note and optional approved model description/transcript. Filename is NOT evidence. Create actual decisions and nonempty reasons; source facts are not an output-shaped template to copy."})
    }).collect::<Vec<_>>(),"fixed_items":p.montage.items.iter().filter(|i|!items.iter().any(|v|v.id==i.id)).collect::<Vec<_>>(),"music":{"asset_id":p.montage.music_asset_id,"user_note":p.montage.music_note,"optional_onsets_ms":p.montage.rhythm_markers_ms},"narration":{"asset_id":p.montage.narration_asset_id,"user_note":p.montage.narration_note,"cached_transcripts":p.montage.narration_asset_id.map(|id|transcript_window(&p,id,0,r.target_ms))},"optional_narrative_context":continuity});
    if p.production_preset.is_some() {
        context["production_guidance"] = crate::production_preset::guidance(&p);
    }
    context["album_user_profile"] = json!({"protagonist":p.montage.album.protagonist,"confirmed_user_facts":p.montage.album.confirmed_facts,"requested_style":p.montage.album.style,"editable_recording_script":p.montage.album.recording_script,"contract":"Only user-confirmed identity and biography. Observations are fallible, dates in EXIF are unverified, filename dates are not capture dates. Build an emotional progression, do not infer relations or make chronological sorting a substitute for narrative."});
    if let Some(entries) = context["candidates"].as_array_mut() {
        for entry in entries {
            if let Some(id) = entry
                .pointer("/item/asset_id")
                .and_then(Value::as_str)
                .and_then(|s| Uuid::parse_str(s).ok())
            {
                entry["album_evidence"] = crate::album::context(&p, id);
            }
        }
    }
    let hierarchical = items.len() > hierarchy::LEAF_ITEMS || context.to_string().len() > 64000;
    let job = Job::queued(
        JobKind::ProduceNarrative,
        Some(id),
        "Montage Planner: proposta in coda, timeline invariata",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let jid = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(
            jid,
            JobStatus::Running,
            "Regia del montaggio: arco, materiali, ritmo e raccordi",
            vec![],
        )
        .await;
        let outcome=async {
            let mut cache_hit=false;let mut raw_path=None;let mut duplicate_ids=vec![];
            let mut plan=if hierarchical {hierarchy::build(&app,jid,&p,&r,&items,&context).await?}else if r.mode=="offline" {offline(&p,&r,&items)}else{
                let config=app.config();
                let key=Uuid::new_v5(&id,format!("{}|{}|{}|{:?}|{}|{}",context,fingerprint(&p),config.llm_for_role("story"),config.llm_provider,config.ollama_url,config.openai_base_url).as_bytes());
                let cache=store::project_dir(&app.data_dir,id).join("analysis/montage-planner").join(format!("{key}.json"));std::fs::create_dir_all(cache.parent().unwrap())?;
                store::write_json(&cache.with_extension("context.json"),&context)?;
                let response=if cache.exists(){cache_hit=true;store::read_json(&cache)?}else{
                    let reply=providers::chat(&app.http,&config,"story",json!([
                        {"role":"system","content":include_str!("montage_planner/prompt.txt")},
                        {"role":"user","content":context.to_string()}
                    ]),schema(&items,items.iter().filter(|i|p.montage.album.photos.get(&i.asset_id).is_some_and(|v|v.mandatory)).count()),6500).await?;store::write_json(&cache,&reply)?;reply
                };raw_path=Some(cache);
                if let Some(j)=app.jobs.write().await.get_mut(&jid){j.result=Some(json!({"raw_path":raw_path,"cache_hit":cache_hit,"mode":r.mode,"model":response["model"],"provider":response["provider"]}));}
                let raw=response.pointer("/message/content").and_then(Value::as_str).ok_or_else(||anyhow::anyhow!("Risposta priva di JSON"))?;let v:Value=serde_json::from_str(raw)?;
                let (sequence,repeated)=decode_sequence(&v,&items)?;duplicate_ids=repeated;
                Plan{id:Uuid::new_v4(),based_on:fingerprint(&p),summary:serde_json::from_value(v["summary"].clone())?,arc:serde_json::from_value(v["arc"].clone())?,warnings:serde_json::from_value(v["warnings"].clone())?,missing:serde_json::from_value(v["missing"].clone())?,sequence,mode:r.mode.clone(),model:response["model"].clone(),provider:response["provider"].clone(),created_at:chrono::Utc::now(),applied:false,narrative_fingerprint:None,technical_notes:vec![],..Default::default()}
            };
            if !r.revision_fields.is_empty() {
                plan.sequence=plan.sequence.iter().map(|i|revise_fields(items.iter().find(|s|s.id==i.id).unwrap_or(i),i,&r.revision_fields)).collect();
                if !r.revision_fields.iter().any(|f|f=="order") {plan.sequence.sort_by_key(|i|items.iter().position(|s|s.id==i.id).unwrap_or(usize::MAX));}
            } else {for item in &mut plan.sequence {if let Some(source)=items.iter().find(|s|s.id==item.id){item.note=source.note.clone();}}}
            plan.narrative_fingerprint=narrative_fingerprint(&narrative);
            plan.required_items=items.iter().filter(|i|p.montage.album.photos.get(&i.asset_id).is_some_and(|v|v.mandatory)).cloned().collect();
            plan.omitted_items=items.iter().filter(|i|!plan.sequence.iter().any(|v|v.id==i.id)).cloned().collect();
            review::finalize(&p,&mut plan,&items,&r);
            if !duplicate_ids.is_empty(){plan.review_notes.push(review::Issue{code:"duplicate_model_ids".into(),severity:"warning".into(),item_id:None,message:format!("Il modello ha ripetuto {} ID: conservata soltanto la prima proposta per ciascuno. Raw originale intatto. Revisiona ordine e motivazioni, e recupera gli eventuali materiali obbligatori omessi prima di approvare.",duplicate_ids.len())});}
            validate_plan(&p,&plan,&items)?;
            plan.technical_notes=technical_notes(&p,&plan,&items,r.target_ms);
            if !r.revision_fields.is_empty() {plan.technical_notes.push(format!("Revisione limitata a {:?}: campi fuori ambito e note originali conservati dal server. Verifica il riepilogo rispetto ai dati effettivi.",r.revision_fields));}
            let mut all=app.projects.write().await;let current=all.get_mut(&id).ok_or_else(||anyhow::anyhow!("Progetto assente"))?;
            anyhow::ensure!(fingerprint(current)==plan.based_on,"Materiali/sequenza cambiati durante il lavoro: risposta conservata in cache, nessuna modifica applicata");
            {let now:Value=if narrative_path.exists(){store::read_json(&narrative_path)?}else{Value::Null};anyhow::ensure!(now==narrative,"Continuità cambiata durante il lavoro: riproponi sullo stato aggiornato");}
            let mut next=current.clone();if next.montage.plans.len()>=8{next.montage.plans.remove(0);}next.montage.plans.push(plan.clone());next.updated_at=chrono::Utc::now();app.save_project(&next).await.map_err(|e|anyhow::anyhow!(e.1))?;*current=next;
            if let Some(j)=app.jobs.write().await.get_mut(&jid){j.result=Some(json!({"plan_id":plan.id,"cache_hit":cache_hit,"raw_path":raw_path,"mode":plan.mode,"model":plan.model,"provider":plan.provider}));}Ok::<_,anyhow::Error>(())
        }.await;
        match outcome {
            Ok(()) => {
                app.set_job(
                    jid,
                    JobStatus::Succeeded,
                    "Piano pronto: revisiona e approva selettivamente; timeline invariata",
                    vec![],
                )
                .await
            }
            Err(e) => {
                app.set_job(jid, JobStatus::Failed, e.to_string(), vec![])
                    .await
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Apply {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub plan_id: Uuid,
    pub items: Vec<Item>,
    #[serde(default)]
    pub approved_audio_overrides: Vec<Uuid>,
    #[serde(default)]
    pub chapter_edits: Vec<hierarchy::ChapterEdit>,
}
fn merge(p: &Project, plan: &Plan, chosen: &[Item]) -> anyhow::Result<Document> {
    anyhow::ensure!(
        !plan.applied && plan.based_on == fingerprint(p),
        "Proposta già applicata o stato cambiato: riproponi senza perdere le scelte manuali"
    );
    anyhow::ensure!(!chosen.is_empty(), "Scegli almeno una proposta");
    let mut ids = HashSet::new();
    for i in chosen {
        anyhow::ensure!(
            ids.insert(i.id)
                && plan
                    .sequence
                    .iter()
                    .chain(plan.required_items.iter())
                    .chain(plan.omitted_items.iter())
                    .any(|c| c.id == i.id && c.asset_id == i.asset_id)
                && !p.montage.items.iter().any(|v| v.id == i.id && v.locked),
            "ID estraneo, duplicato o scelta bloccata"
        );
    }
    let mut doc = p.montage.clone();
    let existing = doc.items.iter().map(|i| i.id).collect::<HashSet<_>>();
    let mut replacements = chosen.iter().filter(|i| existing.contains(&i.id));
    for item in &mut doc.items {
        if ids.contains(&item.id) {
            *item = replacements.next().unwrap().clone();
            item.locked = true;
        }
    }
    for i in chosen.iter().filter(|i| !existing.contains(&i.id)) {
        let mut item = i.clone();
        item.locked = true;
        doc.items.push(item);
    }
    doc.validate(p)?;
    doc.plans
        .iter_mut()
        .find(|v| v.id == plan.id)
        .unwrap()
        .applied = true;
    Ok(doc)
}
pub async fn apply(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(r): Json<Apply>,
) -> Result<Json<Project>, ApiError> {
    let mut all = state.projects.write().await;
    let p = all.get_mut(&id).ok_or_else(|| bad("Progetto assente"))?;
    if p.updated_at != r.expected_updated_at {
        return Err(bad(
            "Progetto cambiato: ricarica, nessuna modifica applicata",
        ));
    }
    let plan = p
        .montage
        .plans
        .iter()
        .find(|v| v.id == r.plan_id)
        .ok_or_else(|| bad("Proposta assente"))?;
    let path = store::project_dir(&state.data_dir, id).join("narrative.json");
    let current: Value = if path.exists() {
        store::read_json(&path).map_err(ApiError::internal)?
    } else {
        Value::Null
    };
    if plan.narrative_fingerprint != narrative_fingerprint(&current) {
        return Err(bad(
            "Reference/continuità cambiate: riproponi, nessuna scelta sovrascritta",
        ));
    }
    let mut next = p.clone();
    let accepted_hierarchy =
        hierarchy::edit_chapters(&plan.hierarchy, &r.chapter_edits).map_err(bad)?;
    review::validate_accept(p, plan, &r.items, &r.approved_audio_overrides).map_err(bad)?;
    next.montage = merge(p, plan, &r.items).map_err(bad)?;
    let accepted_summary = review::summary(p, &next.montage.items, next.montage.transition_ms);
    if let Some(saved) = next.montage.plans.iter_mut().find(|v| v.id == r.plan_id) {
        saved.accepted_items = r.items;
        saved.accepted_summary = accepted_summary;
        saved.accepted_hierarchy = accepted_hierarchy;
    }
    next.updated_at = chrono::Utc::now();
    state.save_project(&next).await?;
    *p = next.clone();
    Ok(Json(next))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evidence_does_not_recycle_rationales_and_targeted_revision_preserves_facts_and_timing() {
        let original = Item {
            note: "Nota fornita dall’utente".into(),
            reason: "Bozza offline".into(),
            emotion: "da definire".into(),
            duration_ms: 3500,
            title: "Scelta manuale".into(),
            ..Item::default()
        };
        let input = context_item(&original);
        assert!(
            input.get("reason").is_none()
                && input.get("emotion").is_none()
                && input.get("source_volume").is_none()
        );
        assert_eq!(input["note"], original.note);
        let proposed = Item {
            duration_ms: 9000,
            note: "Persona inventata".into(),
            title: "Titolo proposto".into(),
            movement: "push".into(),
            ..original.clone()
        };
        let revised = revise_fields(&original, &proposed, &["movement".into()]);
        assert_eq!(revised.duration_ms, 3500);
        assert_eq!(revised.title, "Scelta manuale");
        assert_eq!(revised.note, original.note);
        assert_eq!(revised.movement, "push");
        assert_eq!(revise_fields(&original, &proposed, &[]).note, original.note);
    }
    fn sample() -> Project {
        let mut p = Project::new("P".into(), None);
        p.assets.push(Asset {
            id: Uuid::new_v4(),
            name: "Never infer this name".into(),
            kind: AssetKind::Image,
            file_name: "a.png".into(),
            source_path: String::new(),
            imported_at: chrono::Utc::now(),
            media: None,
        });
        p.montage.items = (0..3)
            .map(|_| Item {
                asset_id: p.assets[0].id,
                ..Item::default()
            })
            .collect();
        p
    }
    fn request() -> Request {
        Request {
            expected_updated_at: chrono::Utc::now(),
            mode: "offline".into(),
            instruction: String::new(),
            target_ms: 12000,
            scope_ids: vec![],
            include_library: false,
            use_continuity: false,
            revision_fields: vec![],
            audio_policy: default_audio_policy(),
            require_opening_title: false,
            opening_title: String::new(),
        }
    }
    #[test]
    fn omitted_required_photo_can_be_reviewed_and_recovered_without_another_model_call() {
        let mut p = sample();
        p.montage.items.truncate(1);
        let mut other = p.assets[0].clone();
        other.id = Uuid::new_v4();
        p.assets.push(other.clone());
        let missing = Item {
            asset_id: other.id,
            ..Item::default()
        };
        let candidates = vec![p.montage.items[0].clone(), missing.clone()];
        let r = request();
        let mut plan = offline(&p, &r, &candidates);
        plan.sequence.truncate(1);
        plan.required_items = vec![missing.clone()];
        review::finalize(&p, &mut plan, &candidates, &r);
        assert!(plan
            .review_notes
            .iter()
            .any(|i| i.code == "mandatory_omitted"));
        assert!(validate_plan(&p, &plan, &candidates).is_ok());
        assert!(review::validate_accept(&p, &plan, &plan.sequence, &[]).is_err());
        let mut chosen = plan.sequence.clone();
        chosen.push(missing.clone());
        assert!(review::validate_accept(&p, &plan, &chosen, &[]).is_ok());
        p.montage.plans.push(plan.clone());
        let recovered = merge(&p, &plan, &chosen).unwrap();
        assert!(recovered
            .items
            .iter()
            .any(|i| i.asset_id == missing.asset_id));
        chosen[1].asset_id = Uuid::new_v4();
        assert!(merge(&p, &plan, &chosen).is_err());
        assert_eq!(
            schema(&candidates, 2)["properties"]["sequence"]["minItems"],
            2
        );
        assert_eq!(
            schema(&candidates, 2)["properties"]["sequence"]["maxItems"],
            2
        );
    }
    #[test]
    fn repeated_model_ids_are_disclosed_not_turned_into_fake_edit_occurrences() {
        let p = sample();
        let a = &p.montage.items[0];
        let raw = json!({"sequence":[a,a]});
        let (items, repeated) = decode_sequence(&raw, &p.montage.items).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(repeated, vec![a.id]);
        assert_eq!(items[0].asset_id, a.asset_id);
        let mut foreign = raw;
        foreign["sequence"][0]["id"] = json!(Uuid::new_v4());
        assert!(decode_sequence(&foreign, &p.montage.items).is_err());
    }
    fn voiced_sample() -> Project {
        let mut p = sample();
        p.assets[0].kind = AssetKind::Video;
        p.assets[0].media = Some(MediaInfo {
            duration_ms: Some(12000),
            width: Some(1920),
            height: Some(1080),
            frame_rate: Some(30.),
            has_audio: true,
        });
        p
    }
    #[test]
    fn semantic_receipt_uses_decisions_not_model_claims_and_preserves_audio() {
        let p = voiced_sample();
        let mut r = request();
        r.mode = "ai".into();
        r.require_opening_title = true;
        let mut plan = offline(&p, &r, &p.montage.items);
        plan.mode = "ai".into();
        plan.summary = "Titolo e voce presenti, ordine opposto".into();
        plan.sequence.reverse();
        plan.sequence.iter_mut().for_each(|i| {
            i.reason.clear();
            i.title.clear();
            i.source_volume = 0.;
        });
        review::finalize(&p, &mut plan, &p.montage.items, &r);
        assert!(plan.sequence.iter().all(|i| i.source_volume == 1.));
        assert!(plan.summary.starts_with("Ordine effettivo: 1. Video"));
        assert!(!plan.summary.contains("ordine opposto"));
        assert!(plan.model_summary.contains("ordine opposto"));
        assert!(plan.review_notes.iter().any(|i| i.code == "title_missing"));
        assert_eq!(
            plan.review_notes
                .iter()
                .filter(|i| i.code == "reason_missing")
                .count(),
            3
        );
        assert!(review::validate_accept(&p, &plan, &plan.sequence, &[]).is_err());
        let mut chosen = plan.sequence.clone();
        chosen
            .iter_mut()
            .for_each(|i| i.reason = "Intenzione creativa rivista dall’utente".into());
        chosen[0].title = "Titolo approvato".into();
        review::validate_accept(&p, &plan, &chosen, &[]).unwrap();
        chosen[0].source_volume = 0.;
        assert!(review::validate_accept(&p, &plan, &chosen, &[]).is_err());
        review::validate_accept(&p, &plan, &chosen, &[chosen[0].id]).unwrap();
    }
    #[test]
    fn explicit_mute_and_targeted_non_audio_revision_do_not_get_overridden() {
        let mut p = voiced_sample();
        p.montage.items[0].source_volume = 0.35;
        let mut r = request();
        r.audio_policy = "mute_sources".into();
        let mut plan = offline(&p, &r, &p.montage.items);
        review::finalize(&p, &mut plan, &p.montage.items, &r);
        assert!(plan.sequence.iter().all(|i| i.source_volume == 0.));
        r.revision_fields = vec!["titles".into()];
        review::finalize(&p, &mut plan, &p.montage.items, &r);
        assert_eq!(plan.sequence[0].source_volume, 0.35);
        assert_eq!(plan.requirements.audio_policy, "keep_current");
    }
    #[test]
    fn observable_semantic_risks_are_warnings_not_arbitrary_editing_bans() {
        let p = voiced_sample();
        let mut r = request();
        r.mode = "ai".into();
        let mut plan = offline(&p, &r, &p.montage.items);
        plan.mode = "ai".into();
        plan.sequence[0].title = "Apertura".into();
        plan.sequence[1].title = "Titolo aggiuntivo intenzionale".into();
        review::finalize(&p, &mut plan, &p.montage.items, &r);
        assert!(plan
            .review_notes
            .iter()
            .any(|i| i.code == "audio_boundary_unverified"));
        assert!(plan
            .review_notes
            .iter()
            .any(|i| i.code == "additional_titles"));
        // No purported ASR truth or automatic exclusion: a reviewer can approve.
        review::validate_accept(&p, &plan, &plan.sequence, &[]).unwrap();
        let p = sample();
        let mut plan = offline(&p, &r, &p.montage.items);
        plan.sequence[0].trim_in_ms = 2000;
        review::finalize(&p, &mut plan, &p.montage.items, &r);
        assert!(plan
            .review_notes
            .iter()
            .any(|i| i.code == "still_image_trim"));
    }
    #[test]
    fn title_requirement_follows_actual_opening_after_selective_reorder() {
        let p = sample();
        let mut r = request();
        r.require_opening_title = true;
        let mut plan = offline(&p, &r, &p.montage.items);
        review::finalize(&p, &mut plan, &p.montage.items, &r);
        let mut chosen = plan.sequence.clone();
        chosen[0].title = "Prima apertura".into();
        review::validate_accept(&p, &plan, &chosen, &[]).unwrap();
        chosen.swap(0, 1);
        assert!(review::validate_accept(&p, &plan, &chosen, &[]).is_err());
        chosen[0].title = "Nuova apertura".into();
        review::validate_accept(&p, &plan, &chosen, &[]).unwrap();
    }
    #[test]
    fn legacy_plan_deserializes_without_semantic_fields_and_old_request_protects_audio() {
        let p = sample();
        let plan = offline(&p, &request(), &p.montage.items);
        let mut value = serde_json::to_value(plan).unwrap();
        for key in [
            "model_summary",
            "model_arc",
            "requirements",
            "review_notes",
            "accepted_items",
            "accepted_summary",
            "request_instruction",
            "request_brief",
        ] {
            value.as_object_mut().unwrap().remove(key);
        }
        let old: Plan = serde_json::from_value(value).unwrap();
        assert_eq!(old.requirements.version, 0);
        assert_eq!(old.requirements.audio_policy, "keep_current");
        let r:Request=serde_json::from_value(json!({"expected_updated_at":p.updated_at,"mode":"offline","instruction":"","target_ms":1000})).unwrap();
        assert_eq!(r.audio_policy, "preserve_original");
        assert!(!r.require_opening_title);
    }
    #[test]
    fn selective_merge_keeps_fixed_slots_and_locks_approved_choices() {
        let mut p = sample();
        p.montage.items[1].locked = true;
        let c = candidates(&p, &request()).unwrap();
        let mut plan = offline(&p, &request(), &c);
        plan.sequence.reverse();
        p.montage.plans.push(plan.clone());
        let d = merge(&p, &plan, &plan.sequence).unwrap();
        assert_eq!(d.items[1].id, p.montage.items[1].id);
        assert_eq!(d.items[0].id, p.montage.items[2].id);
        assert!(d.items.iter().all(|i| i.locked));
        assert!(d.plans[0].applied);
    }
    #[test]
    fn stale_foreign_and_locked_inputs_cannot_replace_manual_work() {
        let mut p = sample();
        let plan = offline(&p, &request(), &p.montage.items);
        p.montage.plans.push(plan.clone());
        p.montage.items[0].note = "Manual".into();
        assert!(merge(&p, &plan, &plan.sequence).is_err());
        let mut r = request();
        r.scope_ids.push(Uuid::new_v4());
        assert!(candidates(&p, &r).is_err());
        p.montage.items.iter_mut().for_each(|i| i.locked = true);
        assert!(candidates(&p, &request()).is_err());
    }
    #[test]
    fn offline_is_variable_bounded_and_old_projects_default_safely() {
        let p = sample();
        let plan = offline(&p, &request(), &p.montage.items);
        validate_plan(&p, &plan, &p.montage.items).unwrap();
        assert_ne!(plan.sequence[0].duration_ms, plan.sequence[1].duration_ms);
        let old: Document = serde_json::from_value(json!({"items":[]})).unwrap();
        assert!(old.plans.is_empty() && old.preserve_timeline_edits);
    }
    #[test]
    fn local_qa_exposes_missing_video_title_and_omissions_without_approving_art() {
        let p = sample();
        let mut plan = offline(&p, &request(), &p.montage.items);
        plan.mode = "ai".into();
        plan.sequence.truncate(1);
        let notes = technical_notes(&p, &plan, &p.montage.items, 9000);
        assert!(notes.iter().any(|s| s.contains("non sono proposti")));
        assert!(notes.iter().any(|s| s.contains("non contiene video")));
        assert!(notes.iter().any(|s| s.contains("Nessun titolo")));
        assert!(notes.iter().any(|s| s.contains("bozza offline")));
    }
    #[test]
    fn cached_transcript_context_is_windowed_without_retiming_or_word_duplication() {
        let mut p = sample();
        let asset = p.assets[0].id;
        p.transcripts.push(Transcript {
            id: Uuid::new_v4(),
            asset_id: asset,
            language: "it".into(),
            segments: (0..3)
                .map(|i| TranscriptSegment {
                    start_ms: i * 5000,
                    end_ms: (i + 1) * 5000,
                    text: format!("Parte{i}"),
                })
                .collect(),
            words: vec![],
            imported_at: chrono::Utc::now(),
        });
        let context = transcript_window(&p, asset, 5000, 10000);
        assert_eq!(context[0]["segments"].as_array().unwrap().len(), 1);
        assert_eq!(context[0]["segments"][0]["start_ms"], 5000);
        assert_eq!(context[0]["segments"][0]["text"], "Parte1");
        assert!(context[0].get("words").is_none());
    }
}
