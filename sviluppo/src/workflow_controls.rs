//! Present actual ComfyUI combobox values, never inferred model substitutions.
use crate::{store, workflow_doctor, ApiError, AppState};
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;
pub async fn controls(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let summary = state
        .workflows
        .read()
        .await
        .iter()
        .find(|w| w.id == id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Workflow assente"))?;
    let graph: Value =
        store::read_json(std::path::Path::new(&summary.path)).map_err(ApiError::internal)?;
    let definitions = workflow_doctor::node_definitions_for(&state, &graph)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let mut result = serde_json::Map::new();
    for (node_id, node) in graph.as_object().into_iter().flatten() {
        let Some(class) = node["class_type"].as_str() else {
            continue;
        };
        for (input, _) in node["inputs"].as_object().into_iter().flatten() {
            let schema = definitions.get(class).and_then(|d| d.get("input"));
            let field = schema.and_then(|v| {
                v.get("required")
                    .and_then(|r| r.get(input))
                    .or_else(|| v.get("optional").and_then(|r| r.get(input)))
            });
            if let Some(choices) = field.and_then(|f| f.get(0)).and_then(Value::as_array) {
                if choices
                    .iter()
                    .all(|v| v.is_string() || v.is_number() || v.is_boolean())
                {
                    result.insert(format!("{node_id}.{input}"), json!(choices));
                }
            }
        }
    }
    Ok(Json(
        json!({"choices":result,"source":"ComfyUI object_info","automatic_substitution":false}),
    ))
}
