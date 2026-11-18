//! Deterministic, non-destructive workflow diagnosis. Node definitions come
//! from the running ComfyUI, never from guessed model names or local paths.
use crate::{media, model::WorkflowSummary, store, ApiError, AppState};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::Path,
    time::Duration,
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub severity: String,
    pub code: String,
    pub node_id: String,
    pub input: String,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub choices: Vec<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub node_id: String,
    /// None means remove a node; a value means replace one existing input.
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rename_to: Option<String>,
    pub before: Value,
    pub after: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repair {
    pub id: String,
    pub description: String,
    pub changes: Vec<Change>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub ready: bool,
    pub live_checked: bool,
    pub issues: Vec<Issue>,
    pub repairs: Vec<Repair>,
    pub active_nodes: usize,
    pub total_nodes: usize,
    pub quality_notice: &'static str,
}
impl Report {
    fn issue(
        &mut self,
        severity: &str,
        code: &str,
        node: &str,
        input: &str,
        message: impl Into<String>,
    ) {
        self.issues.push(Issue {
            severity: severity.into(),
            code: code.into(),
            node_id: node.into(),
            input: input.into(),
            message: message.into(),
            choices: vec![],
        });
    }
    pub fn error_summary(&self) -> String {
        self.issues
            .iter()
            .filter(|i| i.severity == "error")
            .take(8)
            .map(|i| format!("{} {}.{}: {}", i.code, i.node_id, i.input, i.message))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Deserialize)]
pub struct DoctorRequest {
    pub workflow_id: Uuid,
    #[serde(default)]
    pub inputs: BTreeMap<String, Value>,
    /// Input addresses satisfied by a project reference upload at execution.
    #[serde(default)]
    pub asset_inputs: BTreeMap<String, Uuid>,
    #[serde(default)]
    pub selected_repairs: Vec<String>,
}

/// Query only classes actually present in this graph. A large installed node
/// registry must not turn a small workflow into a multi-megabyte timeout.
pub async fn node_definitions_for(state: &AppState, graph: &Value) -> anyhow::Result<Value> {
    let classes = graph
        .as_object()
        .into_iter()
        .flat_map(|v| v.values())
        .filter_map(|n| n["class_type"].as_str())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let expected = classes.clone();
    let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
    let mut tasks = tokio::task::JoinSet::new();
    for class in classes {
        let client = state.http.clone();
        let permits = permits.clone();
        let mut url = reqwest::Url::parse(state.config().comfy_url.trim_end_matches('/'))?;
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("Indirizzo ComfyUI non valido"))?
            .pop_if_empty()
            .push("object_info")
            .push(&class);
        tasks.spawn(async move {
            let _permit = permits.acquire_owned().await?;
            let response = client
                .get(url)
                .timeout(Duration::from_secs(6))
                .send()
                .await?
                .error_for_status()?;
            let mut data: Value = response.json().await?;
            Ok::<_, anyhow::Error>((
                class.clone(),
                data.as_object_mut().and_then(|m| m.remove(&class)),
            ))
        });
    }
    let mut result = serde_json::Map::new();
    while let Some(task) = tasks.join_next().await {
        let (class, node) = task??;
        if let Some(node) = node {
            result.insert(class, node);
        }
    }
    // Older servers/test doubles may only expose the aggregate endpoint.
    // Use it solely for classes absent from the focused responses.
    if result.len() != expected.len() {
        let mut all: Value = state
            .http
            .get(format!(
                "{}/object_info",
                state.config().comfy_url.trim_end_matches('/')
            ))
            .timeout(Duration::from_secs(6))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        for class in expected {
            if let Some(node) = all.as_object_mut().and_then(|m| m.remove(&class)) {
                result.entry(class).or_insert(node);
            }
        }
    }
    Ok(Value::Object(result))
}

async fn load(
    state: &AppState,
    request: &DoctorRequest,
) -> Result<(WorkflowSummary, Value), ApiError> {
    let summary = state
        .workflows
        .read()
        .await
        .iter()
        .find(|w| w.id == request.workflow_id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Workflow assente"))?;
    let value = media::patch_workflow_inputs(Path::new(&summary.path), &request.inputs)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok((summary, value))
}

pub async fn diagnose(
    State(state): State<AppState>,
    Json(request): Json<DoctorRequest>,
) -> Result<Json<Report>, ApiError> {
    let (_, value) = load(&state, &request).await?;
    let info = node_definitions_for(&state, &value).await;
    let mut report = analyze(
        &value,
        info.as_ref().ok(),
        &request.asset_inputs.keys().cloned().collect(),
    );
    if let Err(error) = info {
        report.issue(
            "error",
            "live_unavailable",
            "",
            "",
            format!("ComfyUI non verificabile: {error}"),
        );
        report.ready = false;
    }
    Ok(Json(report))
}

pub async fn repair(
    State(state): State<AppState>,
    Json(request): Json<DoctorRequest>,
) -> Result<Json<Value>, ApiError> {
    let (original, mut workflow) = load(&state, &request).await?;
    if request.selected_repairs.is_empty() {
        return Err(ApiError::bad_request(
            "Seleziona le correzioni da applicare",
        ));
    }
    let definitions = node_definitions_for(&state, &workflow)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let references = request.asset_inputs.keys().cloned().collect();
    let report = analyze(&workflow, Some(&definitions), &references);
    for id in &request.selected_repairs {
        if !report.repairs.iter().any(|r| &r.id == id) {
            return Err(ApiError::bad_request(
                "Il piano è cambiato: ripeti la diagnosi",
            ));
        }
    }
    let applied = report
        .repairs
        .into_iter()
        .filter(|r| request.selected_repairs.contains(&r.id))
        .collect::<Vec<_>>();
    for repair in &applied {
        apply(&mut workflow, repair).map_err(|e| ApiError::bad_request(e.to_string()))?;
    }
    let directory = state
        .data_dir
        .join("workflows")
        .join("repaired")
        .join(Uuid::new_v4().to_string());
    std::fs::create_dir_all(&directory).map_err(|e| ApiError::internal(e.into()))?;
    let name = media::safe_file_name(Path::new(&original.path)).map_err(ApiError::internal)?;
    let stem = Path::new(&name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("workflow");
    let target = directory.join(format!("{stem}--riparato.json"));
    store::write_json(&target, &workflow).map_err(ApiError::internal)?;
    let after = analyze(&workflow, Some(&definitions), &references);
    let mut summary = media::scan_workflows(&[directory])
        .map_err(ApiError::internal)?
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::bad_request("La copia non è un workflow API"))?;
    crate::runtime::portable_workflow_paths(std::slice::from_mut(&mut summary));
    let audit = json!({"source":original,"saved":summary,"applied":applied,"diagnosis_after":after,"original_modified":false});
    store::write_json(&target.with_extension("repair-report.json"), &audit)
        .map_err(ApiError::internal)?;
    let mut workflows = state.workflows.write().await;
    workflows.push(summary.clone());
    store::save_workflows(&state.data_dir, &workflows).map_err(ApiError::internal)?;
    Ok(Json(
        json!({"workflow":summary,"report":after,"applied":applied,"original_modified":false}),
    ))
}

/// Link syntax in API prompts; a dangling link must not be mistaken for a literal.
fn link(value: &Value) -> Option<(&str, usize)> {
    let values = value.as_array()?;
    if values.len() != 2 {
        return None;
    }
    Some((
        values[0].as_str()?,
        usize::try_from(values[1].as_u64()?).ok()?,
    ))
}
fn upstream(workflow: &Value, roots: Vec<String>) -> HashSet<String> {
    let mut pending = roots;
    let mut seen = HashSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        for value in workflow[&id]["inputs"]
            .as_object()
            .into_iter()
            .flat_map(|i| i.values())
        {
            if let Some((parent, _)) = link(value) {
                pending.push(parent.into());
            }
        }
    }
    seen
}
fn visit(
    id: &str,
    workflow: &Value,
    visiting: &mut HashSet<String>,
    done: &mut HashSet<String>,
) -> bool {
    if done.contains(id) {
        return false;
    }
    if !visiting.insert(id.into()) {
        return true;
    }
    for value in workflow[id]["inputs"]
        .as_object()
        .into_iter()
        .flat_map(|i| i.values())
    {
        if let Some((parent, _)) = link(value) {
            if workflow.get(parent).is_some() && visit(parent, workflow, visiting, done) {
                return true;
            }
        }
    }
    visiting.remove(id);
    done.insert(id.into());
    false
}

// V3 schemas contain containers, not literal required inputs named "values"
// or "variables". Expand them exactly to their dotted API input addresses.
fn normalize_schema(schema: &Value, live: &Value) -> Value {
    fn expand(groups: &Value, prefix: &str, live: &Value, out: &mut Value) {
        for group in ["required", "optional"] {
            for (name, spec) in groups[group].as_object().into_iter().flat_map(|v| v.iter()) {
                let address = format!("{prefix}{name}");
                match spec[0].as_str() {
                    Some("COMFY_AUTOGROW_V3") => {
                        let template = &spec[1]["template"];
                        let (inner, required) = if let Some(v) = template["input"]["required"]
                            .as_object()
                            .and_then(|v| v.values().next())
                        {
                            (v, true)
                        } else if let Some(v) = template["input"]["optional"]
                            .as_object()
                            .and_then(|v| v.values().next())
                        {
                            (v, false)
                        } else {
                            continue;
                        };
                        let names = grow_names(template);
                        let min = template["min"].as_u64().unwrap_or(0) as usize;
                        for (index, name) in names.iter().enumerate() {
                            out[if required && index < min {
                                "required"
                            } else {
                                "optional"
                            }][format!("{address}.{name}")] = inner.clone();
                        }
                    }
                    Some("COMFY_DYNAMICCOMBO_V3") => {
                        let options = spec[1]["options"].as_array().cloned().unwrap_or_default();
                        out[group][&address] = json!([options
                            .iter()
                            .filter_map(|o| o.get("key"))
                            .collect::<Vec<_>>()]);
                        if let Some(selected) = options.iter().find(|o| o["key"] == live[&address])
                        {
                            expand(&selected["inputs"], &format!("{address}."), live, out);
                        }
                    }
                    Some("COMFY_MATCHTYPE_V3") => {
                        let mut normalized = spec.clone();
                        normalized[0] = spec[1]["template"]["allowed_types"]
                            .as_str()
                            .unwrap_or("*")
                            .into();
                        out[group][&address] = normalized;
                    }
                    Some("COMBO") if spec[1]["options"].is_array() => {
                        out[group][&address] = json!([spec[1]["options"]]);
                    }
                    _ => {
                        out[group][&address] = spec.clone();
                    }
                }
            }
        }
    }
    let mut normalized = schema.clone();
    let mut inputs = json!({"required":{},"optional":{}});
    expand(&schema["input"], "", live, &mut inputs);
    normalized["input"] = inputs;
    normalized
}
fn grow_names(template: &Value) -> Vec<String> {
    if let Some(names) = template["names"].as_array() {
        return names
            .iter()
            .take(100)
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
    }
    let prefix = template["prefix"].as_str().unwrap_or("input");
    (0..template["max"].as_u64().unwrap_or(0).min(100))
        .map(|index| format!("{prefix}{index}"))
        .collect()
}
fn resolved_output(
    workflow: &Value,
    definitions: &Value,
    node: &str,
    port: usize,
    depth: usize,
) -> String {
    if depth > 32 {
        return "*".into();
    }
    let schema = &definitions[workflow[node]["class_type"].as_str().unwrap_or("")];
    let kind = schema["output"][port].as_str().unwrap_or("*");
    if kind != "COMFY_MATCHTYPE_V3" {
        return kind.into();
    }
    let template = schema["output_matchtypes"][port].as_str().unwrap_or("");
    let mut types = vec![];
    for group in ["required", "optional"] {
        for (key, spec) in schema["input"][group]
            .as_object()
            .into_iter()
            .flat_map(|g| g.iter())
        {
            if spec[1]["template"]["template_id"].as_str() != Some(template) {
                continue;
            }
            let value = &workflow[node]["inputs"][key];
            let resolved = if let Some((source, output)) = link(value) {
                resolved_output(workflow, definitions, source, output, depth + 1)
            } else if value.is_string() {
                "STRING".into()
            } else if value.is_boolean() {
                "BOOLEAN".into()
            } else if value.is_u64() || value.is_i64() {
                "INT".into()
            } else if value.is_number() {
                "FLOAT".into()
            } else {
                continue;
            };
            if resolved != "*" && !types.contains(&resolved) {
                types.push(resolved);
            }
        }
    }
    if types.is_empty() {
        "*".into()
    } else {
        types.join(",")
    }
}

pub fn analyze(
    workflow: &Value,
    definitions: Option<&Value>,
    references: &HashSet<String>,
) -> Report {
    let mut report=Report{ready:false,live_checked:definitions.is_some(),issues:vec![],repairs:vec![],active_nodes:0,total_nodes:0,
        quality_notice:"Pronto significa compatibilità tecnica preliminare, non qualità artistica, assenza di OOM o garanzia di un risultato impeccabile. Revisiona sempre l'output."};
    let Some(nodes) = workflow.as_object() else {
        report.issue(
            "error",
            "api_format",
            "",
            "",
            "Serve il JSON API ComfyUI, non il formato canvas",
        );
        return report;
    };
    let nodes = nodes
        .iter()
        .filter(|(_, n)| n.get("class_type").and_then(Value::as_str).is_some())
        .collect::<BTreeMap<_, _>>();
    report.total_nodes = nodes.len();
    if nodes.is_empty() {
        report.issue("error", "api_format", "", "", "Nessun nodo API");
        return report;
    }
    let outputs = nodes
        .iter()
        .filter(|(_, n)| {
            let class = n["class_type"].as_str().unwrap_or("");
            definitions
                .and_then(|d| d.get(class))
                .and_then(|d| d.get("output_node"))
                .and_then(Value::as_bool)
                .unwrap_or(matches!(
                    class,
                    "SaveImage" | "PreviewImage" | "SaveAudio" | "VHS_VideoCombine" | "SaveVideo"
                ))
        })
        .map(|(id, _)| (*id).clone())
        .collect::<Vec<_>>();
    if outputs.is_empty() {
        report.issue(
            "error",
            "no_output",
            "",
            "",
            "Nessun nodo di output riconosciuto",
        );
    }
    let active = upstream(workflow, outputs.clone());
    report.active_nodes = active.len();
    let mut done = HashSet::new();
    for root in &outputs {
        if visit(root, workflow, &mut HashSet::new(), &mut done) {
            report.issue("error", "cycle", root, "", "Il grafo contiene un ciclo");
            break;
        }
    }
    for (id, node) in &nodes {
        let class = node["class_type"].as_str().unwrap_or("");
        let executing = active.contains(*id) || outputs.is_empty();
        let severity = if executing { "error" } else { "warning" };
        if !executing {
            report.issue(
                "warning",
                "unused_node",
                id,
                "",
                "Nodo non collegato agli output: non verrà eseguito",
            );
        }
        let original_schema = definitions.and_then(|defs| defs.get(class));
        if executing {
            for group in ["required", "optional"] {
                for (container, spec) in original_schema
                    .and_then(|s| s["input"][group].as_object())
                    .into_iter()
                    .flat_map(|g| g.iter())
                {
                    if spec[0] != "COMFY_AUTOGROW_V3" {
                        continue;
                    }
                    for name in grow_names(&spec[1]["template"]) {
                        let target = format!("{container}.{name}");
                        if let Some(value) = node["inputs"]
                            .get(&name)
                            .filter(|_| node["inputs"].get(&target).is_none())
                        {
                            report.issue("error","legacy_dynamic_input",id,&name,format!("Input precedente all'API V3: rinomina in {target} mantenendo esattamente il valore/collegamento"));
                            report.repairs.push(Repair {
                                id: format!("v3_autogrow:{id}.{name}"),
                                description: format!(
                                    "Migra {id}.{name} → {target}, senza alterare il contenuto"
                                ),
                                changes: vec![Change {
                                    node_id: (*id).clone(),
                                    input: Some(name.clone()),
                                    rename_to: Some(target),
                                    before: value.clone(),
                                    after: value.clone(),
                                }],
                            });
                        }
                    }
                }
            }
        }
        let normalized_schema = original_schema.map(|s| normalize_schema(s, &node["inputs"]));
        let schema = normalized_schema.as_ref();
        // API V3 moved old flat combo children into qualified addresses. Offer
        // an explicit rename only for one unambiguous active enum destination
        // accepting the exact saved value. Never choose a new codec/model.
        if executing {
            if let (Some(schema), Some(inputs)) = (schema, node["inputs"].as_object()) {
                for (name, value) in inputs {
                    let hidden_legacy = original_schema
                        .and_then(|s| s["input"]["optional"].get(name))
                        .is_some_and(|s| s[1]["hidden"] == true);
                    if name.contains('.')
                        || schema["input"]["required"].get(name).is_some()
                        || (schema["input"]["optional"].get(name).is_some() && !hidden_legacy)
                    {
                        continue;
                    }
                    let suffix = format!(".{name}");
                    let targets = ["required", "optional"]
                        .into_iter()
                        .filter_map(|group| schema["input"][group].as_object())
                        .flat_map(|group| group.iter())
                        .filter(|(target, spec)| {
                            let parent = target.split('.').next().unwrap_or("");
                            let root = original_schema.and_then(|s| {
                                s["input"]["required"]
                                    .get(parent)
                                    .or_else(|| s["input"]["optional"].get(parent))
                            });
                            target.ends_with(&suffix)
                                && !inputs.contains_key(*target)
                                && root.is_some_and(|s| s[0] == "COMFY_DYNAMICCOMBO_V3")
                                && spec[0]
                                    .as_array()
                                    .is_some_and(|choices| choices.contains(value))
                        })
                        .map(|(target, _)| target.clone())
                        .collect::<BTreeSet<_>>();
                    if targets.len() != 1 {
                        continue;
                    }
                    let target = targets.into_iter().next().unwrap();
                    report.issue("error","legacy_dynamic_combo",id,name,format!("API V3: {name} è ora {target}. Proposta di rinomina esplicita, stesso valore salvato; nessuna scelta automatica del codec."));
                    report.repairs.push(Repair {
                        id: format!("v3_dynamic_combo:{id}.{name}"),
                        description: format!(
                            "Migra {id}.{name} → {target}, stesso valore, nella copia"
                        ),
                        changes: vec![Change {
                            node_id: (*id).clone(),
                            input: Some(name.clone()),
                            rename_to: Some(target),
                            before: value.clone(),
                            after: value.clone(),
                        }],
                    });
                }
            }
        }
        if executing && schema.is_some_and(|s| s["api_node"] == true) {
            report.issue("error","external_api_node",id,"","Nodo API esterno: non è un modello locale e non viene eseguito automaticamente dalla suite locale.");
        }
        if definitions.is_some() && schema.is_none() {
            report.issue(severity,"missing_node",id,"",format!("Nodo {class} non installato/esposto da ComfyUI. Nessuna installazione automatica."));
        }
        let Some(inputs) = node.get("inputs").and_then(Value::as_object) else {
            report.issue(severity, "missing_inputs", id, "", "Oggetto inputs assente");
            continue;
        };
        if let Some(required) = schema
            .and_then(|s| s.pointer("/input/required"))
            .and_then(Value::as_object)
        {
            for key in required.keys() {
                if !inputs.contains_key(key) {
                    report.issue(
                        severity,
                        "required_input",
                        id,
                        key,
                        "Input obbligatorio assente",
                    );
                }
            }
        }
        for (key, value) in inputs {
            let spec = schema
                .and_then(|s| s.pointer("/input/required"))
                .and_then(|s| s.get(key))
                .or_else(|| {
                    schema
                        .and_then(|s| s.pointer("/input/optional"))
                        .and_then(|s| s.get(key))
                });
            if let Some((parent, port)) = link(value) {
                if executing
                    && matches!(key.as_str(), "width" | "height")
                    && workflow[parent]["inputs"]["value"]
                        .as_f64()
                        .is_some_and(|n| n.abs() > 16384.0)
                {
                    report.issue("error","unsafe_resolution",parent,"value",format!("Costante anomala collegata a {id}.{key}: imposta esplicitamente le dimensioni, senza trasformare il formato in un quadrato"));
                }
                if !nodes.keys().any(|key| key.as_str() == parent) {
                    report.issue(
                        severity,
                        "dangling_link",
                        id,
                        key,
                        format!("Sorgente {parent} assente"),
                    );
                    continue;
                }
                if let Some(parent_schema) = definitions
                    .and_then(|d| d.get(workflow[parent]["class_type"].as_str().unwrap_or("")))
                {
                    if let Some(ports) = parent_schema["output"].as_array() {
                        if port >= ports.len() {
                            report.issue(
                                severity,
                                "output_port",
                                id,
                                key,
                                format!("Porta {port} assente sul nodo {parent}"),
                            );
                        } else if let Some(expected) = spec.and_then(|s| s[0].as_str()) {
                            let actual =
                                resolved_output(workflow, definitions.unwrap(), parent, port, 0);
                            if !compatible(&actual, expected) {
                                report.issue(
                                    severity,
                                    "socket_type",
                                    id,
                                    key,
                                    format!(
                                        "{parent}:{port} produce {actual}, richiesto {expected}"
                                    ),
                                );
                            }
                        }
                    }
                }
                continue;
            }
            if references.contains(&format!("{id}.{key}")) {
                continue;
            }
            if let Some(options) = spec.and_then(|s| s[0].as_array()) {
                if !options.contains(value) {
                    let code = if key.starts_with("clip_name") {
                        "missing_text_encoder"
                    } else if matches!(
                        key.as_str(),
                        "image" | "audio" | "video" | "audio_file" | "video_file"
                    ) {
                        "missing_reference"
                    } else {
                        "invalid_choice"
                    };
                    report.issue(severity,code,id,key,format!("Valore {value} non disponibile in {class}. Seleziona esplicitamente un valore valido; nessuna sostituzione automatica."));
                    report.issues.last_mut().unwrap().choices = options.clone();
                    // Same relative path, different slash convention: this is
                    // not permission to choose a different model or basename.
                    if let Some(requested) = value.as_str().filter(|_| executing) {
                        let exact = options
                            .iter()
                            .filter(|option| {
                                option.as_str().is_some_and(|name| {
                                    name.replace('\\', "/") == requested.replace('\\', "/")
                                })
                            })
                            .collect::<Vec<_>>();
                        if exact.len() == 1 {
                            report.repairs.push(Repair{id:format!("path_separator:{id}.{key}"),description:format!("Normalizza solo i separatori del percorso {id}.{key}, stesso file relativo"),changes:vec![Change{node_id:(*id).clone(),input:Some(key.clone()),rename_to:None,before:value.clone(),after:exact[0].clone()}]});
                        }
                    }
                }
            } else if let Some(kind) = spec.and_then(|s| s[0].as_str()) {
                let valid = match kind {
                    "INT" => value.is_i64() || value.is_u64(),
                    "FLOAT" => value.is_number(),
                    "STRING" => value.is_string(),
                    "BOOLEAN" => value.is_boolean(),
                    _ => true,
                };
                if !valid {
                    report.issue(
                        severity,
                        "literal_type",
                        id,
                        key,
                        format!("Richiesto {kind}, ricevuto {value}"),
                    );
                }
                if let Some(number) = value.as_f64() {
                    let min = spec.and_then(|s| s[1].get("min")).and_then(Value::as_f64);
                    let max = spec.and_then(|s| s[1].get("max")).and_then(Value::as_f64);
                    if min.is_some_and(|n| number < n) || max.is_some_and(|n| number > n) {
                        report.issue(
                            severity,
                            "out_of_range",
                            id,
                            key,
                            format!("{number} fuori dai limiti del nodo ({min:?}..{max:?})"),
                        );
                    }
                }
            }
            if executing
                && matches!(key.as_str(), "width" | "height")
                && value.as_f64().is_some_and(|v| v.abs() > 16384.0)
            {
                report.issue(
                    "error",
                    "unsafe_resolution",
                    id,
                    key,
                    "Risoluzione anomala: correggi le dimensioni esplicitamente prima di eseguire",
                );
            }
            if executing
                && matches!(class, "ResizeImageMaskNode" | "ImageScaleBy")
                && matches!(
                    key.as_str(),
                    "multiplier" | "resize_type.multiplier" | "scale_by"
                )
                && value.as_f64().is_some_and(|v| v > 0.0 && v < 1.0)
            {
                report.issue("warning","preview_downscale",id,key,"Riduzione di risoluzione attiva. Potrebbe essere voluta: non viene rimossa automaticamente.");
            }
        }
        if !executing {
            report.repairs.push(Repair {
                id: format!("remove_unused:{id}"),
                description: format!(
                    "Rimuovi nodo scollegato {id} ({class}) dalla copia; non partecipa agli output"
                ),
                changes: vec![Change {
                    node_id: (*id).clone(),
                    input: None,
                    rename_to: None,
                    before: (*node).clone(),
                    after: Value::Null,
                }],
            });
        }
    }
    // Bypass only a provably video-only LTX branch. Never discard audio output
    // or infer audio absence merely from a missing node name in another branch.
    for (id, node) in &nodes {
        if node["class_type"] != "LTXVSeparateAVLatent" || !active.contains(*id) {
            continue;
        }
        let Some((parent, _)) = link(&node["inputs"]["av_latent"]) else {
            continue;
        };
        let ancestors = upstream(workflow, vec![parent.into()]);
        let video = ancestors
            .iter()
            .any(|id| workflow[id]["class_type"] == "EmptyLTXVLatentVideo");
        let audio = ancestors.iter().any(|id| {
            workflow[id]["class_type"]
                .as_str()
                .is_some_and(|c| c.contains("Audio") || c == "LTXVConcatAVLatent")
        });
        if !video || audio {
            continue;
        }
        let consumers = nodes
            .iter()
            .flat_map(|(target, n)| {
                n["inputs"]
                    .as_object()
                    .into_iter()
                    .flat_map(|i| i.iter())
                    .filter_map(|(key, v)| {
                        link(v)
                            .filter(|(source, _)| *source == id.as_str())
                            .map(|(_, port)| ((*target).clone(), key.clone(), port, v.clone()))
                    })
            })
            .collect::<Vec<_>>();
        if consumers.iter().any(|(_, _, port, _)| *port != 0) {
            report.issue("error","ltx_audio_missing",id,"av_latent","Il separatore legge un latente solo video ma la porta audio è usata: occorre una vera sorgente audio.");
            continue;
        }
        let mut changes = consumers
            .into_iter()
            .map(|(target, key, _, before)| Change {
                node_id: target,
                input: Some(key),
                rename_to: None,
                before,
                after: node["inputs"]["av_latent"].clone(),
            })
            .collect::<Vec<_>>();
        changes.push(Change {
            node_id: (*id).clone(),
            input: None,
            rename_to: None,
            before: (*node).clone(),
            after: Value::Null,
        });
        report.issue(
            "error",
            "ltx_video_only_split",
            id,
            "av_latent",
            "Separatore AV su ramo solo video: disponibile bypass deterministico su una copia.",
        );
        report.repairs.push(Repair {
            id: format!("ltx_video_only_split:{id}"),
            description: format!(
                "Rimuovi separatore AV {id} dal ramo solo video e ricollega i consumatori video"
            ),
            changes,
        });
    }
    report.ready = report.live_checked && !report.issues.iter().any(|i| i.severity == "error");
    report
}
fn compatible(actual: &str, expected: &str) -> bool {
    if actual == "*" || expected == "*" || actual == expected {
        return true;
    }
    actual
        .split(',')
        .any(|a| expected.split(',').any(|e| a.trim() == e.trim()))
}
fn apply(workflow: &mut Value, repair: &Repair) -> anyhow::Result<()> {
    for change in &repair.changes {
        if let Some(input) = &change.input {
            anyhow::ensure!(
                workflow[&change.node_id]["inputs"][input] == change.before,
                "Input cambiato: ripeti la diagnosi"
            );
            if let Some(target) = &change.rename_to {
                anyhow::ensure!(
                    workflow[&change.node_id]["inputs"].get(target).is_none(),
                    "Destinazione già presente: ripeti la diagnosi"
                );
                let inputs = workflow[&change.node_id]["inputs"].as_object_mut().unwrap();
                inputs.remove(input);
                inputs.insert(target.clone(), change.after.clone());
            } else {
                workflow[&change.node_id]["inputs"][input] = change.after.clone();
            }
        } else {
            anyhow::ensure!(
                workflow[&change.node_id] == change.before,
                "Nodo cambiato: ripeti la diagnosi"
            );
            workflow.as_object_mut().unwrap().remove(&change.node_id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn definitions() -> Value {
        json!({
            "EmptyLTXVLatentVideo":{"input":{"required":{"width":["INT",{"max":16384}]}},"output":["LATENT"]},
            "LTXVSeparateAVLatent":{"input":{"required":{"av_latent":["LATENT"]}},"output":["LATENT","LATENT"]},
            "SaveVideo":{"input":{"required":{"samples":["LATENT"]}},"output_node":true,"output":[]},
            "CLIPLoader":{"input":{"required":{"clip_name":[["correct-encoder.safetensors"]]}},"output":["CLIP"]}
        })
    }
    #[test]
    fn deterministic_ltx_repair_is_idempotent_and_preserves_models() {
        let source = json!({"a":{"class_type":"EmptyLTXVLatentVideo","inputs":{"width":1280}},"b":{"class_type":"LTXVSeparateAVLatent","inputs":{"av_latent":["a",0]}},"c":{"class_type":"SaveVideo","inputs":{"samples":["b",0]}},"d":{"class_type":"CLIPLoader","inputs":{"clip_name":"correct-encoder.safetensors"}}});
        let report = analyze(&source, Some(&definitions()), &HashSet::new());
        assert!(!report.ready);
        let repair = report
            .repairs
            .iter()
            .find(|r| r.id.starts_with("ltx_video_only_split:"))
            .unwrap();
        let mut copy = source.clone();
        apply(&mut copy, repair).unwrap();
        assert_eq!(source["c"]["inputs"]["samples"], json!(["b", 0]));
        assert_eq!(copy["d"], source["d"]);
        let after = analyze(&copy, Some(&definitions()), &HashSet::new());
        assert!(after.ready);
        assert!(!after
            .repairs
            .iter()
            .any(|r| r.id.starts_with("ltx_video_only_split:")));
    }
    #[test]
    fn never_replace_encoder_reference_or_discard_audio() {
        let source = json!({"a":{"class_type":"EmptyLTXVLatentVideo","inputs":{"width":1280}},"b":{"class_type":"LTXVSeparateAVLatent","inputs":{"av_latent":["a",0]}},"c":{"class_type":"SaveVideo","inputs":{"samples":["b",1]}},"d":{"class_type":"CLIPLoader","inputs":{"clip_name":"missing-encoder.safetensors"}}});
        let report = analyze(&source, Some(&definitions()), &HashSet::new());
        assert!(!report
            .repairs
            .iter()
            .any(|r| r.id.starts_with("ltx_video_only_split:")));
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == "missing_text_encoder"));
        assert!(report.issues.iter().any(|i| i.code == "ltx_audio_missing"));
    }
    #[test]
    fn catches_cycles_dangling_links_and_absurd_resolution() {
        let source = json!({"a":{"class_type":"EmptyLTXVLatentVideo","inputs":{"width":841716597278330u64,"extra":["absent",0]}},"c":{"class_type":"SaveVideo","inputs":{"samples":["a",0]}},"x":{"class_type":"SaveVideo","inputs":{"samples":["x",0]}}});
        let report = analyze(&source, Some(&definitions()), &HashSet::new());
        for code in ["cycle", "dangling_link", "unsafe_resolution"] {
            assert!(report.issues.iter().any(|i| i.code == code), "{code}");
        }
    }
    #[test]
    fn v3_matchtypes_dynamic_combo_and_autogrow_are_not_false_errors() {
        let definitions = json!({
            "Load":{"input":{"required":{}},"output":["IMAGE"]},
            "Resize":{"input":{"required":{"image":["COMFY_MATCHTYPE_V3",{"template":{"template_id":"kind","allowed_types":"IMAGE,MASK"}}],"mode":["COMFY_DYNAMICCOMBO_V3",{"options":[{"key":"scale","inputs":{"required":{"factor":["FLOAT",{"min":0.01}]}}}]}]}},"output":["COMFY_MATCHTYPE_V3"],"output_matchtypes":["kind"]},
            "Save":{"input":{"required":{"image":["IMAGE"]}},"output_node":true,"output":[]},
            "Math":{"input":{"required":{"values":["COMFY_AUTOGROW_V3",{"template":{"input":{"required":{"value":["INT"]}},"names":["a","b"],"min":1}}]}},"output":["INT"]}
        });
        let graph = json!({"load":{"class_type":"Load","inputs":{}},"resize":{"class_type":"Resize","inputs":{"image":["load",0],"mode":"scale","mode.factor":1.0}},"save":{"class_type":"Save","inputs":{"image":["resize",0]}}});
        let report = analyze(&graph, Some(&definitions), &HashSet::new());
        assert!(report.ready, "{}", report.error_summary());
        let normalized = normalize_schema(&definitions["Math"], &json!({"values.a":5}));
        assert!(normalized["input"]["required"].get("values").is_none());
        assert_eq!(normalized["input"]["required"]["values.a"][0], "INT");
    }
    #[test]
    fn autogrow_migration_preserves_the_link_and_is_explicit() {
        let defs = json!({"Int":{"input":{"required":{}},"output":["INT"]},"Math":{"input":{"required":{"expression":["STRING"],"variables":["COMFY_AUTOGROW_V3",{"template":{"input":{"optional":{"var":["INT"]}},"names":["a","b"],"min":2}}]}},"output_node":true,"output":["INT"]}});
        let original = json!({"int":{"class_type":"Int","inputs":{}},"math":{"class_type":"Math","inputs":{"expression":"a*2","a":["int",0]}}});
        let report = analyze(&original, Some(&defs), &HashSet::new());
        assert!(!report.ready);
        let repair = report
            .repairs
            .iter()
            .find(|r| r.id.starts_with("v3_autogrow:"))
            .unwrap();
        let mut changed = original.clone();
        apply(&mut changed, repair).unwrap();
        assert_eq!(changed["math"]["inputs"]["variables.a"], json!(["int", 0]));
        assert_eq!(changed["math"]["inputs"]["expression"], "a*2");
        assert!(analyze(&changed, Some(&defs), &HashSet::new()).ready);
        assert!(original["math"]["inputs"].get("a").is_some());
    }
    #[test]
    fn dynamic_combo_migration_preserves_codec_and_rejects_ambiguous_or_invalid_choices() {
        let mut defs = json!({"SaveVideo":{"input":{"required":{"format":["COMFY_DYNAMICCOMBO_V3",{"options":[{"key":"auto","inputs":{"required":{"codec":[["auto","h264","av1"]]}}}]}]}},"output_node":true,"output":[]}});
        // The installed SaveVideo also advertises its old field as hidden.
        defs["SaveVideo"]["input"]["optional"] = json!({"codec":["COMFY_DYNAMICCOMBO_V3",{"hidden":true,"options":[{"key":"auto","inputs":{"required":{}}}]}]});
        let original =
            json!({"92":{"class_type":"SaveVideo","inputs":{"format":"auto","codec":"auto"}}});
        let report = analyze(&original, Some(&defs), &HashSet::new());
        assert!(!report.ready);
        let repair = report
            .repairs
            .iter()
            .find(|r| r.id == "v3_dynamic_combo:92.codec")
            .unwrap();
        let mut copy = original.clone();
        apply(&mut copy, repair).unwrap();
        assert_eq!(copy["92"]["inputs"]["format.codec"], "auto");
        assert!(copy["92"]["inputs"].get("codec").is_none());
        assert_eq!(original["92"]["inputs"]["codec"], "auto");
        assert!(analyze(&copy, Some(&defs), &HashSet::new()).ready);
        let mut invalid = original.clone();
        invalid["92"]["inputs"]["codec"] = json!("unavailable");
        assert!(analyze(&invalid, Some(&defs), &HashSet::new())
            .repairs
            .is_empty());
        let mut visible = defs.clone();
        visible["SaveVideo"]["input"]["optional"]["codec"][1]["hidden"] = json!(false);
        assert!(analyze(&original, Some(&visible), &HashSet::new())
            .repairs
            .is_empty());
        let mut ambiguous = defs.clone();
        ambiguous["SaveVideo"]["input"]["required"]["other"] =
            defs["SaveVideo"]["input"]["required"]["format"].clone();
        let mut ambiguous_graph = original.clone();
        ambiguous_graph["92"]["inputs"]["other"] = json!("auto");
        assert!(analyze(&ambiguous_graph, Some(&ambiguous), &HashSet::new())
            .repairs
            .is_empty());
    }
}
