//! Bounded calls, unbounded collection: existing planner decisions and renderer.
//! Leaves retain edit IDs; parent permutations never invent or discard a leaf.
use super::*;
use std::collections::{BTreeMap, HashMap};

// 6500 output tokens must carry full editing decisions, not hundreds of IDs.
// These are per-call transport budgets, not collection-size restrictions.
pub(super) const LEAF_ITEMS: usize = 20;
const LEAF_BYTES: usize = 24000;
const PARENT_ITEMS: usize = 12;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Chapter {
    pub id: Uuid,
    pub children: Vec<Uuid>,
    pub item_ids: Vec<Uuid>,
    pub title: String,
    pub intention: String,
    pub script: String,
    pub cache_hit: bool,
    pub raw_path: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChapterEdit {
    pub id: Uuid,
    pub title: String,
    pub intention: String,
    pub script: String,
}
pub(super) fn edit_chapters(
    nodes: &[Chapter],
    edits: &[ChapterEdit],
) -> anyhow::Result<Vec<Chapter>> {
    let mut result = nodes.to_vec();
    let mut seen = HashSet::new();
    for edit in edits {
        anyhow::ensure!(
            seen.insert(edit.id)
                && edit.title.len() <= 960
                && edit.intention.len() <= 6000
                && edit.script.len() <= 12000,
            "Modifica capitolo duplicata o oltre budget"
        );
        let node = result
            .iter_mut()
            .find(|n| n.id == edit.id)
            .ok_or_else(|| anyhow::anyhow!("Capitolo estraneo: nessuna reference modificata"))?;
        node.title = edit.title.clone();
        node.intention = edit.intention.clone();
        node.script = edit.script.clone();
    }
    Ok(result)
}

/// Group explicit user chapters without pretending that imported order is age.
/// Evidence is packed intact: an oversized single record is diagnosed, not cut.
fn leaves(
    p: &Project,
    items: &[Item],
    entries: &[Value],
    bounded: bool,
) -> anyhow::Result<Vec<Vec<usize>>> {
    anyhow::ensure!(
        items.len() == entries.len(),
        "Inventario contesto incoerente"
    );
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut order = vec![];
    for (index, item) in items.iter().enumerate() {
        let key = p
            .montage
            .album
            .photos
            .get(&item.asset_id)
            .map(|v| v.chapter.trim())
            .unwrap_or("")
            .to_string();
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(index);
    }
    let mut result = vec![];
    for key in order {
        let mut chunk = vec![];
        let mut bytes = 0;
        for index in &groups[&key] {
            let size = if bounded {
                entries[*index].to_string().len()
            } else {
                0
            };
            anyhow::ensure!(size <= LEAF_BYTES, "Evidenza del materiale {} oltre il budget di una chiamata ({} byte): revisiona la descrizione o la finestra ASR; nessuna foto troncata/esclusa", items[*index].id, size);
            if !chunk.is_empty() && (chunk.len() >= LEAF_ITEMS || bytes + size > LEAF_BYTES) {
                result.push(std::mem::take(&mut chunk));
                bytes = 0;
            }
            chunk.push(*index);
            bytes += size;
        }
        if !chunk.is_empty() {
            result.push(chunk);
        }
    }
    Ok(result)
}

async fn cached_call(
    app: &AppState,
    jid: Uuid,
    p: &Project,
    context: &Value,
    format: Value,
    system: &str,
    tokens: u32,
) -> anyhow::Result<(Value, bool, String)> {
    let config = app.config();
    let key = Uuid::new_v5(&p.id, json!({"contract":"album-hierarchy-v1","context":context,"schema":format,"system":system,"model":config.llm_for_role("story"),"provider":config.llm_provider,"ollama":config.ollama_url,"openai":config.openai_base_url}).to_string().as_bytes());
    let path = store::project_dir(&app.data_dir, p.id)
        .join("analysis/montage-planner/hierarchy")
        .join(format!("{key}.json"));
    std::fs::create_dir_all(path.parent().unwrap())?;
    store::write_json(&path.with_extension("context.json"), context)?;
    let hit = path.exists();
    let reply: Value = if hit {
        store::read_json(&path)?
    } else {
        let reply=providers::chat(&app.http,&config,"story",json!([{"role":"system","content":system},{"role":"user","content":context.to_string()}]),format,tokens).await?;
        store::write_json(&path, &reply)?;
        reply
    };
    if let Some(job) = app.jobs.write().await.get_mut(&jid) {
        job.result = Some(
            json!({"hierarchical":true,"last_raw_path":path,"last_cache_hit":hit,"recovery":"Riproponi sullo stesso stato: i gruppi completati sono riutilizzati senza inferenza"}),
        );
    }
    let raw = reply
        .pointer("/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("JSON assente: raw conservato in {}", path.display()))?;
    let value: Value = serde_json::from_str(raw)?;
    Ok((value, hit, path.to_string_lossy().into()))
}

const SYNTHESIS: &str = "Sei un montatore. Ricevi capitoli PROPOSTI, non fatti verificati. Ordina TUTTI gli ID figli una sola volta costruendo apertura, sviluppo, contrasto, respiro e conclusione. I beat non sono vincoli. Non inventare identità, parentele, epoche o eventi. title è una proposta editoriale (non un titolo Motion automatico); intention spiega la progressione e i compromessi, script è un breve raccordo narrativo registrabile, non biografia o audio generato. Non cancellare capitoli, non modificare gli ID. Il copione e le intenzioni restano da revisionare.";

fn parent_schema(nodes: &[Chapter]) -> Value {
    json!({"type":"object","additionalProperties":false,"properties":{"order":{"type":"array","minItems":nodes.len(),"maxItems":nodes.len(),"uniqueItems":true,"items":{"type":"string","enum":nodes.iter().map(|n|n.id).collect::<Vec<_>>()}},"title":{"type":"string","maxLength":240},"intention":{"type":"string","maxLength":3000},"script":{"type":"string","maxLength":4000}},"required":["order","title","intention","script"]})
}
fn parent_order(value: &Value, nodes: &[Chapter]) -> anyhow::Result<Vec<Uuid>> {
    let order: Vec<Uuid> = serde_json::from_value(value["order"].clone())?;
    anyhow::ensure!(order.len()==nodes.len() && order.iter().collect::<HashSet<_>>().len()==nodes.len() && order.iter().all(|id|nodes.iter().any(|n|n.id==*id)),"Sintesi incompleta o ID duplicati/estranei: raw e gruppi conservati, nessun capitolo escluso automaticamente");
    Ok(order)
}

pub(super) async fn build(
    app: &AppState,
    jid: Uuid,
    p: &Project,
    r: &Request,
    items: &[Item],
    context: &Value,
) -> anyhow::Result<Plan> {
    let entries = context["candidates"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Candidati assenti"))?;
    let groups = leaves(p, items, entries, r.mode == "ai")?;
    let mut shared = context.clone();
    shared.as_object_mut().unwrap().remove("candidates");
    // Fixed edits never become new candidates or a second enormous prompt.
    shared["fixed_items"] = json!({"count":context["fixed_items"].as_array().map_or(0,Vec::len),"policy":"Locked and out-of-scope edits remain untouched by selective merge"});
    shared["album_user_profile"]
        .as_object_mut()
        .unwrap()
        .remove("editable_recording_script");
    if r.mode == "ai" {
        anyhow::ensure!(shared.to_string().len()<=30000,"Profilo/continuità comune oltre30kbyte per chiamata: riduci il contesto narrativo selezionato, non il numero di fotografie. Nessun dato troncato");
    }
    let mut nodes = vec![];
    let mut by_node: HashMap<Uuid, Vec<Item>> = HashMap::new();
    let mut model = Value::Null;
    let mut provider = Value::Null;
    let mut repeated = 0;
    let mut missing = vec![];
    for (n, indexes) in groups.iter().enumerate() {
        app.set_job(
            jid,
            JobStatus::Running,
            format!(
                "Album · regia gruppo {}/{} · cache incrementale, timeline invariata",
                n + 1,
                groups.len()
            ),
            vec![],
        )
        .await;
        let source = indexes
            .iter()
            .map(|i| items[*i].clone())
            .collect::<Vec<_>>();
        let mut local = r.clone();
        local.target_ms =
            ((r.target_ms as u128 * source.len() as u128 / items.len() as u128) as u64).max(1000);
        let mut leaf_context = shared.clone();
        leaf_context["target_ms"] = json!(local.target_ms);
        leaf_context["candidates"] = json!(indexes
            .iter()
            .map(|i| entries[*i].clone())
            .collect::<Vec<_>>());
        leaf_context["hierarchy_scope"] = json!({"group":n+1,"groups":groups.len(),"collection_items":items.len(),"policy":"This is a production group, NOT an observed life stage. Parent synthesis will order groups. Preserve individual references and do not invent facts about other groups."});
        // Only the global opening may require an opening title.
        if n > 0 {
            leaf_context["requirements"]["require_opening_title"] = json!(false);
            leaf_context["requirements"]["opening_title_text"] = json!("");
        }
        let (sequence, title, intention, hit, raw_path) = if r.mode == "offline" {
            let plan = offline(p, &local, &source);
            (plan.sequence,format!("Gruppo {} · da raccontare",n+1),"Raggruppamento tecnico e capitoli indicati dall’utente, non riconoscimento di epoche o contenuti.".into(),false,None)
        } else {
            let (v, hit, path) = cached_call(
                app,
                jid,
                p,
                &leaf_context,
                schema(
                    &source,
                    source
                        .iter()
                        .filter(|i| {
                            p.montage
                                .album
                                .photos
                                .get(&i.asset_id)
                                .is_some_and(|v| v.mandatory)
                        })
                        .count(),
                ),
                include_str!("prompt.txt"),
                6500,
            )
            .await?;
            let (sequence, duplicates) = decode_sequence(&v, &source)?;
            repeated += duplicates.len();
            let leaf_plan = Plan {
                sequence: sequence.clone(),
                summary: String::new(),
                arc: String::new(),
                model_summary: String::new(),
                model_arc: String::new(),
                warnings: vec![],
                missing: vec![],
                ..Default::default()
            };
            validate_plan(p, &leaf_plan, &source)?;
            let arc = v["arc"].as_str().unwrap_or("").to_string();
            anyhow::ensure!(arc.len() <= 6000, "Intenzione gruppo oltre budget");
            for m in serde_json::from_value::<Vec<Missing>>(v["missing"].clone())? {
                if missing.len() < 12 {
                    missing.push(m);
                }
            }
            model = json!(app.config().llm_for_role("story"));
            provider = json!(app.config().llm_provider);
            (
                sequence,
                format!("Gruppo {} · proposta IA", n + 1),
                arc,
                hit,
                Some(path),
            )
        };
        let node_id = Uuid::new_v5(
            &p.id,
            json!({"sources":source.iter().map(|i|i.id).collect::<Vec<_>>(),"raw":raw_path})
                .to_string()
                .as_bytes(),
        );
        nodes.push(Chapter {
            id: node_id,
            item_ids: sequence.iter().map(|i| i.id).collect(),
            title,
            intention,
            cache_hit: hit,
            raw_path,
            ..Default::default()
        });
        by_node.insert(node_id, sequence);
    }
    let mut level = nodes.clone();
    while level.len() > 1 {
        let mut next = vec![];
        let mut packs: Vec<Vec<Chapter>> = vec![];
        let mut pack = vec![];
        let mut bytes = 0;
        for node in &level {
            let size = node.title.len() + node.intention.len() + 120;
            if !pack.is_empty() && (pack.len() >= PARENT_ITEMS || bytes + size > 24000) {
                packs.push(std::mem::take(&mut pack));
                bytes = 0;
            }
            pack.push(node.clone());
            bytes += size;
        }
        if !pack.is_empty() {
            packs.push(pack);
        }
        anyhow::ensure!(
            packs.len() < level.len(),
            "Sintesi senza riduzione: intenzioni troppo lunghe, gruppi conservati"
        );
        for children in &packs {
            app.set_job(
                jid,
                JobStatus::Running,
                format!(
                    "Album · sintesi gerarchica di {} capitoli · nessun materiale applicato",
                    children.len()
                ),
                vec![],
            )
            .await;
            let (order, title, intention, script, hit, path) = if r.mode == "offline" {
                (children.iter().map(|c|c.id).collect::<Vec<_>>(),"Racconto da revisionare".into(),"Ordine dei capitoli manuali preservato; progressione emotiva da definire con il montatore.".into(),String::new(),false,None)
            } else {
                let compact=children.iter().map(|c|json!({"id":c.id,"title":c.title,"proposed_intention_not_facts":c.intention,"items":by_node[&c.id].len()})).collect::<Vec<_>>();
                let input = json!({"user_instruction":r.instruction,"user_brief":p.montage.brief,"user_profile":{"name":p.montage.album.protagonist,"confirmed_facts":p.montage.album.confirmed_facts,"style":p.montage.album.style},"children":compact});
                anyhow::ensure!(
                    input.to_string().len() <= 64000,
                    "Sintesi oltre il budget di contesto: gruppi conservati"
                );
                let (v, hit, path) = cached_call(
                    app,
                    jid,
                    p,
                    &input,
                    parent_schema(children),
                    SYNTHESIS,
                    2500,
                )
                .await?;
                let order = parent_order(&v, children)?;
                let text = |key: &str, max: usize| -> anyhow::Result<String> {
                    let s = v[key]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Sintesi: campo {key} assente"))?;
                    anyhow::ensure!(s.len() <= max, "Sintesi {key} oltre budget");
                    Ok(s.into())
                };
                (
                    order,
                    text("title", 960)?,
                    text("intention", 6000)?,
                    text("script", 12000)?,
                    hit,
                    Some(path),
                )
            };
            let id = Uuid::new_v5(
                &p.id,
                json!({"children":order,"raw":path}).to_string().as_bytes(),
            );
            let sequence = order
                .iter()
                .flat_map(|id| by_node.remove(id).unwrap())
                .collect::<Vec<_>>();
            let node = Chapter {
                id,
                children: order,
                title,
                intention,
                script,
                cache_hit: hit,
                raw_path: path,
                ..Default::default()
            };
            by_node.insert(id, sequence);
            nodes.push(node.clone());
            next.push(node);
        }
        level = next;
    }
    let mut sequence = by_node.remove(&level[0].id).unwrap();
    let sequence_len = sequence.len();
    for (n, item) in sequence.iter_mut().enumerate() {
        item.section = if n == 0 {
            "opening"
        } else if n + 1 == sequence_len {
            "finale"
        } else {
            "development"
        }
        .into();
    }
    let mut warnings=vec!["Sintesi gerarchica revisionabile: i gruppi tecnici non provano epoche, identità o eventi. Copione proposto, non audio prodotto.".into()];
    if repeated > 0 {
        warnings.push(format!("{repeated} ID ripetuti nelle risposte: conservata la prima occorrenza. Controlla l’inventario delle omissioni; raw intatti."));
    }
    Ok(Plan {
        id: Uuid::new_v4(),
        based_on: fingerprint(p),
        sequence,
        mode: r.mode.clone(),
        model,
        provider,
        hierarchy: nodes,
        warnings,
        missing,
        created_at: chrono::Utc::now(),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collection_is_packed_without_count_cap_and_oversized_manual_evidence_is_allowed() {
        let p = Project::new("Scale".into(), None);
        let items = (0..137).map(|_| Item::default()).collect::<Vec<_>>();
        let entries = items.iter().map(|i| json!({"id":i.id})).collect::<Vec<_>>();
        let groups = leaves(&p, &items, &entries, true).unwrap();
        assert_eq!(
            groups.iter().flatten().copied().collect::<Vec<_>>(),
            (0..137).collect::<Vec<_>>()
        );
        assert!(groups.iter().all(|g| g.len() <= LEAF_ITEMS));
        let mut huge = entries;
        huge[0] = json!({"large": "x".repeat(LEAF_BYTES+1)});
        assert!(leaves(&p, &items, &huge, true).is_err());
        assert!(leaves(&p, &items, &huge, false).is_ok());
    }
    #[test]
    fn chapter_revision_preserves_original_evidence_and_rejects_foreign_nodes() {
        let node = Chapter {
            id: Uuid::new_v4(),
            item_ids: vec![Uuid::new_v4()],
            title: "Original proposal".into(),
            ..Default::default()
        };
        let mut edit = ChapterEdit {
            id: node.id,
            title: "User title".into(),
            intention: "User intention".into(),
            script: "User script".into(),
        };
        let revised =
            edit_chapters(std::slice::from_ref(&node), std::slice::from_ref(&edit)).unwrap();
        assert_eq!(revised[0].item_ids, node.item_ids);
        assert_eq!(node.title, "Original proposal");
        assert_eq!(revised[0].script, "User script");
        edit.id = Uuid::new_v4();
        assert!(edit_chapters(&[node], &[edit]).is_err());
    }
    #[test]
    fn synthesis_cannot_silently_drop_or_repeat_groups() {
        let nodes = (0..3)
            .map(|_| Chapter {
                id: Uuid::new_v4(),
                ..Default::default()
            })
            .collect::<Vec<_>>();
        assert!(parent_order(
            &json!({"order":[nodes[0].id,nodes[0].id,nodes[2].id]}),
            &nodes
        )
        .is_err());
        assert!(parent_order(
            &json!({"order":[nodes[1].id,nodes[2].id,nodes[0].id]}),
            &nodes
        )
        .is_ok());
    }
}
