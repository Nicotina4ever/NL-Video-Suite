//! Explicit API JSON import, independent from discovery and execution.
use crate::{media, model::*, store, ApiError, AppState};
use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub name: String,
    pub graph: Value,
}
fn bad(message: impl ToString) -> ApiError {
    ApiError::bad_request(message.to_string())
}
fn api_graph(value: Value) -> Result<Value, ApiError> {
    let value = if value.get("prompt").is_some_and(Value::is_object) {
        value["prompt"].clone()
    } else {
        value
    };
    if value.get("nodes").is_some_and(Value::is_array) {
        return Err(bad("Questo è il formato editor di ComfyUI. Esporta il workflow in formato API e importa quel JSON; i widget non vengono convertiti o indovinati."));
    }
    let nodes = value
        .as_object()
        .filter(|m| !m.is_empty())
        .ok_or_else(|| bad("Grafo API vuoto o non valido"))?;
    for (id, node) in nodes {
        if id.trim().is_empty()
            || node["class_type"]
                .as_str()
                .is_none_or(|s| s.trim().is_empty())
            || !node["inputs"].is_object()
        {
            return Err(bad(format!(
                "Nodo {id}: servono class_type e inputs nel formato API"
            )));
        }
        for (input, v) in node["inputs"].as_object().unwrap() {
            if let Some(link) = v
                .as_array()
                .filter(|a| a.len() == 2 && a[0].is_string() && a[1].is_u64())
            {
                if !nodes.contains_key(link[0].as_str().unwrap()) {
                    return Err(bad(format!(
                        "Collegamento {id}.{input} a nodo assente {}",
                        link[0]
                    )));
                }
            }
        }
    }
    Ok(value)
}
pub async fn import(
    State(state): State<AppState>,
    Json(request): Json<Request>,
) -> Result<Json<Value>, ApiError> {
    if state.jobs.read().await.values().any(|j| {
        matches!(j.kind, JobKind::ScanWorkflows)
            && matches!(j.status, JobStatus::Queued | JobStatus::Running)
    }) {
        return Err(bad(
            "Attendi l’aggiornamento del catalogo prima di importare",
        ));
    }
    let graph = api_graph(request.graph)?;
    let stem = crate::filesystem_stem(request.name.strip_suffix(".json").unwrap_or(&request.name));
    let key = Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        &serde_json::to_vec(&graph).map_err(bad)?,
    );
    let folder = state
        .data_dir
        .join("workflows/imported")
        .join(key.to_string());
    let mut registry = state.workflows.write().await;
    let existing = if folder.is_dir() {
        media::scan_workflows(std::slice::from_ref(&folder)).map_err(bad)?
    } else {
        vec![]
    };
    let reused = !existing.is_empty();
    let summary = if let Some(summary) = existing.into_iter().next() {
        summary
    } else {
        std::fs::create_dir_all(&folder).map_err(bad)?;
        let path = folder.join(format!("{stem}.json"));
        store::write_json(&path, &graph).map_err(bad)?;
        media::scan_workflows(std::slice::from_ref(&folder))
            .map_err(bad)?
            .into_iter()
            .next()
            .ok_or_else(|| bad("Workflow salvato ma non riconosciuto"))?
    };
    let mut next = registry.clone();
    next.retain(|w| w.id != summary.id);
    next.push(summary.clone());
    next.sort_by(|a, b| a.name.cmp(&b.name));
    store::save_workflows(&state.data_dir, &next).map_err(bad)?;
    *registry = next;
    Ok(Json(
        json!({"workflow":summary,"reused":reused,"notice":"Copia API nel catalogo interno. Nessuna esecuzione, riparazione o installazione. Diagnostica il grafo contro ComfyUI prima di generare."}),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_api_envelope_without_rewriting_graph_and_rejects_editor_or_broken_links() {
        let graph = json!({"1":{"class_type":"LoadAudio","inputs":{"audio":"original.wav"}},"2":{"class_type":"SaveAudio","inputs":{"audio":["1",0],"filename_prefix":"KEEP"}}});
        assert_eq!(api_graph(json!({"prompt":graph})).unwrap(), graph);
        assert!(api_graph(json!({"nodes":[],"links":[]})).is_err());
        let mut broken = graph.clone();
        broken["2"]["inputs"]["audio"] = json!(["missing", 0]);
        assert!(api_graph(broken).is_err());
    }
}
