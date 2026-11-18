//! Explicit, typed bindings over real ComfyUI API graphs; never rewrites templates.
use crate::{compositor, model::*, store, ApiError, AppState};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowBinding {
    pub workflow_id: Uuid,
    /// The user-selected literal STRING input, never inferred from a filename.
    pub text_input: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, Value>,
    #[serde(default)]
    pub asset_inputs: BTreeMap<String, Uuid>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub asset_ranges: BTreeMap<String, crate::time_map::TimeRange>,
}
impl WorkflowBinding {
    pub(crate) fn validate_ranges(&self, project: &Project) -> Result<(), ApiError> {
        for (address, range) in &self.asset_ranges {
            let asset = self
                .asset_inputs
                .get(address)
                .and_then(|id| project.assets.iter().find(|a| a.id == *id))
                .ok_or_else(|| {
                    ApiError::bad_request("Intervallo audio senza reference assegnata")
                })?;
            crate::generation::reference_audio::validate(asset, *range)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputKind {
    Image,
    Video,
    Audio,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScalarControl {
    pub address: String,
    pub node_id: String,
    pub class_type: String,
    pub title: String,
    pub input: String,
    pub value: Value,
    pub choices: Vec<Value>,
    pub schema_type: Option<String>,
    pub role: String,
    pub editable: bool,
    pub bindable_text: bool,
    pub media_kind: Option<String>,
    pub upload_supported: bool,
    pub asset_kinds: Vec<String>,
    pub requires_audio: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NarrativeControls {
    pub workflow_id: Uuid,
    pub name: String,
    pub offline: bool,
    pub notice: String,
    pub text_inputs: Vec<ScalarControl>,
    pub reference_inputs: Vec<ScalarControl>,
    pub scalar_inputs: Vec<ScalarControl>,
    pub output_roles: Vec<String>,
    /// Computed only when a project is supplied: the interface shows the size
    /// the suite will patch instead of inventing one in JavaScript.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_target: Option<ImageTarget>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageTarget {
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub label: String,
    pub capped: bool,
}

#[derive(Debug, Deserialize)]
pub struct ControlsQuery {
    pub project_id: Option<Uuid>,
}

fn schema_input<'a>(definitions: Option<&'a Value>, class: &str, name: &str) -> Option<&'a Value> {
    let node = definitions?.get(class)?;
    node.pointer("/input/required")
        .and_then(|group| group.get(name))
        .or_else(|| {
            node.pointer("/input/optional")
                .and_then(|group| group.get(name))
        })
}

fn link(value: &Value) -> Option<&str> {
    let values = value.as_array()?;
    (values.len() == 2 && values[1].is_u64())
        .then(|| values[0].as_str())
        .flatten()
}

fn words(value: &str) -> Vec<String> {
    value
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn protected_name(value: &str) -> bool {
    joined_protected(value)
        || words(value).iter().any(|word| {
            matches!(
                word.as_str(),
                "negative"
                    | "negativo"
                    | "system"
                    | "model"
                    | "models"
                    | "ckpt"
                    | "checkpoint"
                    | "clip"
                    | "vae"
                    | "unet"
                    | "lora"
                    | "encoder"
                    | "sampler"
                    | "scheduler"
                    | "seed"
                    | "filename"
                    | "file"
                    | "path"
                    | "output"
                    | "voice"
                    | "speaker"
                    | "language"
                    | "dtype"
                    | "device"
                    | "precision"
                    | "ref"
                    | "reference"
                    | "style"
                    | "instruction"
            )
        })
}

fn joined_protected(value: &str) -> bool {
    let lower = value.to_lowercase();
    [
        "systemprompt",
        "negativeprompt",
        "modelname",
        "checkpointname",
        "filenameprefix",
        "voicename",
        "voiceid",
        "speakername",
        "speakerid",
    ]
    .iter()
    .any(|name| lower.contains(name))
}

fn protected_title(value: &str) -> bool {
    // "CLIP Text Encode" and "Voice synthesis" are ordinary positive/TTS
    // node titles, not evidence that their text input is a model/voice selector.
    joined_protected(value)
        || words(value).iter().any(|word| {
            matches!(
                word.as_str(),
                "negative" | "negativo" | "system" | "checkpoint" | "ckpt" | "model" | "lora"
            )
        })
}

/// A negative/system branch can have neutral node titles. Protect its upstream
/// literals as well, including shared inputs, rather than guessing from titles.
fn protected_branches(graph: &Value) -> BTreeSet<String> {
    let mut pending = Vec::new();
    for node in graph.as_object().into_iter().flat_map(|map| map.values()) {
        for (name, value) in node
            .get("inputs")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|map| map.iter())
        {
            if joined_protected(name)
                || words(name).iter().any(|word| {
                    matches!(word.as_str(), "negative" | "system" | "voice" | "speaker")
                })
            {
                if let Some(source) = link(value) {
                    pending.push(source.to_owned());
                }
            }
        }
    }
    let mut result = BTreeSet::new();
    while let Some(id) = pending.pop() {
        // A zeroed conditioning derived from the positive encoder is not a
        // separate negative prompt. Preserve the graph, allow its positive text.
        if graph
            .get(&id)
            .and_then(|node| node.get("class_type"))
            .and_then(Value::as_str)
            == Some("ConditioningZeroOut")
        {
            continue;
        }
        if !result.insert(id.clone()) {
            continue;
        }
        for value in graph
            .get(&id)
            .and_then(|node| node.get("inputs"))
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|map| map.values())
        {
            if let Some(source) = link(value) {
                pending.push(source.to_owned());
            }
        }
    }
    result
}

fn numbered_input(name: &str, stem: &str) -> bool {
    name == stem
        || name.strip_prefix(stem).is_some_and(|tail| {
            !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit() || c == '_')
        })
}

pub(crate) fn reference_kind(
    class: &str,
    name: &str,
    spec: Option<&Value>,
) -> Option<(&'static str, bool)> {
    for kind in ["image", "audio", "video"] {
        if spec
            .and_then(|s| s.get(1))
            .and_then(|metadata| metadata.get(format!("{kind}_upload")))
            .and_then(Value::as_bool)
            == Some(true)
        {
            return Some((kind, true));
        }
    }
    // These exact core loader contracts and generic-byte /upload/image transport
    // were checked against the installed ComfyUI source, not inferred from names.
    match (class, name) {
        ("LoadImage", "image") => return Some(("image", true)),
        ("LoadAudio", "audio") => return Some(("audio", true)),
        ("LoadVideo", "file") => return Some(("video", true)),
        _ => {}
    }
    let lower = name.to_lowercase();
    for kind in ["image", "audio", "video"] {
        if numbered_input(&lower, kind)
            || lower == format!("{kind}_file")
            || lower == format!("reference_{kind}")
        {
            return Some((kind, false));
        }
    }
    None
}

pub fn describe(
    summary: &WorkflowSummary,
    graph: &Value,
    definitions: Option<&Value>,
) -> Result<NarrativeControls, ApiError> {
    let nodes = graph
        .as_object()
        .ok_or_else(|| ApiError::bad_request("Workflow non in formato API ComfyUI"))?;
    if nodes.contains_key("nodes")
        || nodes
            .values()
            .any(|node| !node.get("class_type").is_some_and(Value::is_string))
    {
        return Err(ApiError::bad_request("Serve un workflow ComfyUI esportato in formato API, non il progetto grafico dell'editor"));
    }
    let protected = protected_branches(graph);
    let mut scalar_inputs = Vec::new();
    for (id, node) in nodes {
        let class = node["class_type"].as_str().unwrap_or("");
        let title = node
            .pointer("/_meta/title")
            .and_then(Value::as_str)
            .unwrap_or(class);
        for (name, value) in node
            .get("inputs")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|map| map.iter())
        {
            if !(value.is_string() || value.is_number() || value.is_boolean()) {
                continue;
            }
            let spec = schema_input(definitions, class, name);
            let choices = spec
                .and_then(|s| s.get(0))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let schema_type = spec
                .and_then(|s| s.get(0))
                .and_then(Value::as_str)
                .map(str::to_owned);
            let reference = value
                .is_string()
                .then(|| reference_kind(class, name, spec))
                .flatten();
            let is_protected =
                protected.contains(id) || protected_name(name) || protected_title(title);
            let bindable_text = value.is_string()
                && !definitions
                    .and_then(|d| d.get(class))
                    .and_then(|n| n.get("output_node"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                && reference.is_none()
                && !is_protected
                && choices.is_empty()
                && schema_type.as_deref().is_none_or(|kind| kind == "STRING");
            let (media_kind, upload_supported) = reference
                .map(|(kind, supported)| (Some(kind.to_owned()), supported))
                .unwrap_or((None, false));
            let asset_kinds = match media_kind.as_deref() {
                Some("image") => vec!["image".into()],
                Some("video") => vec!["video".into()],
                Some("audio") => vec!["audio".into(), "video".into()],
                _ => vec![],
            };
            let reason = if reference.is_some() && !upload_supported {
                "Loader custom non verificato: usa il suo valore già disponibile in ComfyUI, oppure un input con metadati upload espliciti".into()
            } else if reference.is_some() {
                "Reference scelta esplicitamente; copia del media del progetto caricata senza cambiare gli altri input".into()
            } else if is_protected {
                "Non usato come testo narrativo: modello, negativo, sistema, voce o configurazione preservati salvo override esplicito".into()
            } else if bindable_text {
                "Input testuale reale: seleziona esplicitamente il campo che deve ricevere il testo della scena".into()
            } else {
                "Parametro scalare: resta invariato se non modificato esplicitamente".into()
            };
            scalar_inputs.push(ScalarControl {
                address: format!("{id}.{name}"),
                node_id: id.clone(),
                class_type: class.into(),
                title: title.into(),
                input: name.clone(),
                value: value.clone(),
                choices,
                schema_type,
                role: if reference.is_some() {
                    "reference"
                } else if is_protected {
                    "protected"
                } else if bindable_text {
                    "text"
                } else {
                    "scalar"
                }
                .into(),
                editable: true,
                bindable_text,
                requires_audio: media_kind.as_deref() == Some("audio"),
                media_kind,
                upload_supported,
                asset_kinds,
                reason,
            });
        }
    }
    Ok(NarrativeControls { workflow_id:summary.id, name:summary.name.clone(), offline:definitions.is_none(),
        notice:"Sono campi reali del workflow, non preset inventati. Scegli esplicitamente testo e reference. I campi non scelti restano invariati; il preflight ComfyUI e la verifica dell'output restano necessari.".into(),
        text_inputs:scalar_inputs.iter().filter(|control| control.bindable_text).cloned().collect(),
        reference_inputs:scalar_inputs.iter().filter(|control| control.media_kind.is_some()).cloned().collect(),
        scalar_inputs, output_roles:summary.output_roles.clone(), image_target:None })
}

async fn summary(state: &AppState, id: Uuid) -> Result<WorkflowSummary, ApiError> {
    state
        .workflows
        .read()
        .await
        .iter()
        .find(|workflow| workflow.id == id)
        .cloned()
        .ok_or_else(|| {
            ApiError::bad_request(
                "Workflow assente dal catalogo; aggiorna il catalogo e sceglilo esplicitamente",
            )
        })
}

pub async fn definitions(state: &AppState) -> Option<Value> {
    let response = state
        .http
        .get(format!(
            "{}/object_info",
            state.config().comfy_url.trim_end_matches('/')
        ))
        .timeout(Duration::from_secs(6))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let value: Value = response.json().await.ok()?;
    value
        .as_object()
        .is_some_and(|map| map.values().any(|node| node.get("input").is_some()))
        .then_some(value)
}

pub async fn controls(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ControlsQuery>,
) -> Result<Json<NarrativeControls>, ApiError> {
    let workflow = summary(&state, id).await?;
    let graph: Value =
        store::read_json(std::path::Path::new(&workflow.path)).map_err(ApiError::internal)?;
    let definitions = crate::workflow_doctor::node_definitions_for(&state, &graph)
        .await
        .ok();
    let mut controls = describe(&workflow, &graph, definitions.as_ref())?;
    if let Some(project_id) = query.project_id {
        let projects = state.projects.read().await;
        if let Some(project) = projects.get(&project_id) {
            let basis = compositor::image_basis(project, None);
            controls.image_target = Some(ImageTarget {
                width: basis.width,
                height: basis.height,
                source_width: basis.source_width,
                source_height: basis.source_height,
                label: basis.label(),
                capped: basis.width.max(basis.height) >= compositor::IMAGE_MAX_SIDE,
            });
        }
    }
    Ok(Json(controls))
}

/// Patch only literal or PrimitiveInt-backed width/height inputs. An explicit
/// user override in the binding always wins, and audio returns untouched.
fn apply_size(
    inputs: &mut BTreeMap<String, Value>,
    graph: &Value,
    kind: OutputKind,
    image: (u32, u32),
    delivery: (u32, u32),
    definitions: Option<&Value>,
) {
    let (width, height) = match kind {
        OutputKind::Image => image,
        OutputKind::Video => delivery,
        OutputKind::Audio => return,
    };
    let limit = |class: &str, name: &str, size: u32| {
        definitions
            .and_then(|all| all.get(class))
            .and_then(|node| node.pointer("/input/required"))
            .and_then(|group| group.get(name))
            .and_then(|spec| spec.get(1))
            .and_then(|meta| meta.get("max"))
            .and_then(Value::as_u64)
            .map_or(size, |max| size.min(max as u32))
    };
    for (id, node) in graph.as_object().into_iter().flat_map(|map| map.iter()) {
        let class = node["class_type"].as_str().unwrap_or("");
        for (key, value) in node
            .get("inputs")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|map| map.iter())
        {
            let size = match key.as_str() {
                "width" => width,
                "height" => height,
                _ => continue,
            };
            if value.is_number() {
                inputs
                    .entry(format!("{id}.{key}"))
                    .or_insert_with(|| serde_json::json!(limit(class, key, size)));
            } else if let Some(parent) = value
                .as_array()
                .and_then(|link| link.first())
                .and_then(Value::as_str)
            {
                let linked = nodes_class(graph, parent);
                if matches!(linked, Some("PrimitiveInt" | "INTConstant" | "Int"))
                    && graph[parent]["inputs"]["value"].is_number()
                {
                    inputs.entry(format!("{parent}.value")).or_insert_with(|| {
                        serde_json::json!(limit(linked.unwrap_or(""), "value", size))
                    });
                }
            }
        }
    }
}

fn nodes_class<'a>(graph: &'a Value, id: &str) -> Option<&'a str> {
    graph.get(id)?.get("class_type")?.as_str()
}

fn validate_override(control: &ScalarControl, value: &Value) -> Result<(), ApiError> {
    let valid = match control.schema_type.as_deref() {
        Some("INT") => value.is_i64() || value.is_u64(),
        Some("FLOAT") => value.is_number(),
        Some("BOOLEAN") => value.is_boolean(),
        Some("STRING") => value.is_string(),
        _ => {
            (control.value.is_string() && value.is_string())
                || (control.value.is_boolean() && value.is_boolean())
                || (control.value.is_number()
                    && value.is_number()
                    && (!(control.value.is_i64() || control.value.is_u64())
                        || value.is_i64()
                        || value.is_u64()))
        }
    };
    if !valid {
        return Err(ApiError::bad_request(format!(
            "Tipo non valido per {}: non si possono sostituire link o aggiungere campi impliciti",
            control.address
        )));
    }
    if !control.choices.is_empty() && !control.choices.contains(value) {
        return Err(ApiError::bad_request(format!(
            "Valore non presente nelle scelte attuali di ComfyUI per {}",
            control.address
        )));
    }
    Ok(())
}

fn prepare(
    summary: &WorkflowSummary,
    graph: &Value,
    project: &Project,
    binding: &WorkflowBinding,
    text: &str,
    kind: OutputKind,
    definitions: Option<&Value>,
) -> Result<ComfySubmitRequest, ApiError> {
    if binding.workflow_id != summary.id {
        return Err(ApiError::bad_request(
            "Il binding non corrisponde al workflow selezionato",
        ));
    }
    if text.trim().is_empty() || text.len() > 60_000 {
        return Err(ApiError::bad_request(
            "Testo narrativo vuoto o troppo lungo (massimo 60000 byte)",
        ));
    }
    let descriptor = describe(summary, graph, definitions)?;
    let text_control = descriptor.text_inputs.iter().find(|control| control.address == binding.text_input)
        .ok_or_else(|| ApiError::bad_request("Scegli esplicitamente un input testuale narrativo valido; link, modelli, negativo, sistema e voce non sono destinazioni automatiche"))?;
    let mut inputs = binding.inputs.clone();
    for (address, value) in &inputs {
        let control = descriptor
            .scalar_inputs
            .iter()
            .find(|control| &control.address == address)
            .ok_or_else(|| {
                ApiError::bad_request(format!("Input scalare assente o collegato: {address}"))
            })?;
        validate_override(control, value)?;
    }
    for (address, id) in &binding.asset_inputs {
        if inputs.contains_key(address) || address == &binding.text_input {
            return Err(ApiError::bad_request(format!("Input {address} impostato sia come valore sia come reference: scegli una sola sorgente")));
        }
        let control = descriptor
            .reference_inputs
            .iter()
            .find(|control| &control.address == address && control.upload_supported)
            .ok_or_else(|| {
                ApiError::bad_request(format!(
                    "Reference {address} assente o loader custom non verificato"
                ))
            })?;
        let asset = project
            .assets
            .iter()
            .find(|asset| asset.id == *id)
            .ok_or_else(|| {
                ApiError::bad_request("Reference non presente nei media di questo progetto")
            })?;
        let compatible = match control.media_kind.as_deref() {
            Some("image") => asset.kind == AssetKind::Image,
            Some("video") => asset.kind == AssetKind::Video,
            Some("audio") => {
                asset.kind == AssetKind::Audio
                    || (asset.kind == AssetKind::Video
                        && asset.media.as_ref().is_some_and(|media| media.has_audio))
            }
            _ => false,
        };
        if !compatible {
            return Err(ApiError::bad_request(format!(
                "Il tipo del media non è compatibile con {} ({})",
                address,
                control.media_kind.as_deref().unwrap_or("sconosciuto")
            )));
        }
    }
    inputs.insert(text_control.address.clone(), Value::String(text.to_owned()));
    for (address, range) in &binding.asset_ranges {
        descriptor.reference_inputs.iter().find(|c|c.address==*address && c.upload_supported && c.media_kind.as_deref()==Some("audio")).ok_or_else(||ApiError::bad_request("Un intervallo audio richiede un loader AUDIO verificato, non un ingresso immagine/video o custom ambiguo"))?;
        let asset = binding
            .asset_inputs
            .get(address)
            .and_then(|id| project.assets.iter().find(|a| a.id == *id))
            .ok_or_else(|| ApiError::bad_request("Intervallo audio senza reference assegnata"))?;
        crate::generation::reference_audio::validate(asset, *range)?;
    }
    // The source frame decides the picture size; an explicit binding override
    // still wins because apply_size only inserts missing addresses.
    let linked_video = binding
        .asset_inputs
        .iter()
        .filter(|(address, _)| {
            descriptor.reference_inputs.iter().any(|c| {
                c.address.as_str() == address.as_str() && c.media_kind.as_deref() == Some("video")
            })
        })
        .map(|(_, id)| *id)
        .find(|id| {
            project
                .assets
                .iter()
                .any(|a| a.id == *id && a.kind == AssetKind::Video)
        });
    let basis = compositor::image_basis(project, linked_video);
    let delivery = compositor::dimensions(&project.aspect_ratio).unwrap_or((1920, 1080));
    apply_size(
        &mut inputs,
        graph,
        kind,
        (basis.width, basis.height),
        delivery,
        definitions,
    );
    Ok(ComfySubmitRequest {
        workflow_path: summary.path.clone().into(),
        project_id: Some(project.id),
        inputs,
        asset_inputs: binding.asset_inputs.clone(),
        asset_ranges: binding.asset_ranges.clone(),
        placement: None,
        review_before_insert: false,
    })
}

pub async fn bind(
    state: &AppState,
    project: &Project,
    binding: &WorkflowBinding,
    text: &str,
    kind: OutputKind,
    definitions: Option<&Value>,
) -> Result<ComfySubmitRequest, ApiError> {
    let workflow = summary(state, binding.workflow_id).await?;
    let graph: Value =
        store::read_json(std::path::Path::new(&workflow.path)).map_err(ApiError::internal)?;
    let focused = if definitions.is_none() {
        crate::workflow_doctor::node_definitions_for(state, &graph)
            .await
            .ok()
    } else {
        None
    };
    let definitions = definitions.or(focused.as_ref());
    let request = prepare(&workflow, &graph, project, binding, text, kind, definitions)?;
    for id in request.asset_inputs.values() {
        let asset = project
            .assets
            .iter()
            .find(|asset| asset.id == *id)
            .expect("reference validated");
        let owned = store::project_dir(&state.data_dir, project.id)
            .join("assets")
            .canonicalize()
            .map_err(|_| ApiError::bad_request("Cartella media del progetto assente"))?;
        let file = owned.join(&asset.file_name).canonicalize().map_err(|_| {
            ApiError::bad_request("File reference assente dalla copia del progetto")
        })?;
        if !file.starts_with(&owned) || !file.is_file() {
            return Err(ApiError::bad_request(
                "La reference deve essere un file interno ai media del progetto",
            ));
        }
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn image_size_follows_the_project_video_and_keeps_explicit_overrides() {
        let (summary, mut graph, project, binding) = fixture();
        graph["latent"] =
            json!({"class_type":"EmptyFlux2LatentImage","inputs":{"width":1024,"height":576}});
        graph["w"] = json!({"class_type":"PrimitiveInt","inputs":{"value":1024}});
        graph["h"] = json!({"class_type":"PrimitiveInt","inputs":{"value":576}});
        graph["linked"] = json!({"class_type":"EmptyFlux2LatentImage","inputs":{"width":["w",0],"height":["h",0]}});
        let request = prepare(
            &summary,
            &graph,
            &project,
            &binding,
            "scene",
            OutputKind::Image,
            None,
        )
        .unwrap();
        assert_eq!(request.inputs["latent.width"], 2880);
        assert_eq!(request.inputs["latent.height"], 1616);
        assert_eq!(request.inputs["w.value"], 2880);
        assert_eq!(request.inputs["h.value"], 1616);
        assert_eq!(request.inputs["p.text"], "scene");

        // Video workflows keep the delivery format instead of inflating it.
        let request = prepare(
            &summary,
            &graph,
            &project,
            &binding,
            "clip",
            OutputKind::Video,
            None,
        )
        .unwrap();
        assert_eq!(request.inputs["latent.width"], 1920);
        assert_eq!(request.inputs["latent.height"], 1080);

        // Audio graphs are never resized.
        let request = prepare(
            &summary,
            &graph,
            &project,
            &binding,
            "voice",
            OutputKind::Audio,
            None,
        )
        .unwrap();
        assert!(!request.inputs.contains_key("latent.width"));
        assert!(!request.inputs.contains_key("w.value"));

        // An explicit user override always wins over the computed target.
        let mut custom = binding.clone();
        custom.inputs.insert("latent.width".into(), json!(768));
        let request = prepare(
            &summary,
            &graph,
            &project,
            &custom,
            "scene",
            OutputKind::Image,
            None,
        )
        .unwrap();
        assert_eq!(request.inputs["latent.width"], 768);
        assert_eq!(request.inputs["latent.height"], 1616);
    }
    #[test]
    fn ace_positive_tags_survive_zero_conditioning_but_save_ui_is_not_a_prompt() {
        let (summary, _, project, _) = fixture();
        let graph = json!({"p":{"class_type":"TextEncodeAceStepAudio1.5","inputs":{"tags":"calm","lyrics":"","duration":4}},"z":{"class_type":"ConditioningZeroOut","inputs":{"conditioning":["p",0]}},"s":{"class_type":"KSampler","inputs":{"positive":["p",0],"negative":["z",0]}},"o":{"class_type":"SaveAudioMP3","inputs":{"audioUI":""}}});
        let definitions = json!({"SaveAudioMP3":{"output_node":true}});
        let controls = describe(&summary, &graph, Some(&definitions)).unwrap();
        assert!(controls.text_inputs.iter().any(|v| v.address == "p.tags"));
        assert!(!controls
            .text_inputs
            .iter()
            .any(|v| v.address == "o.audioUI"));
        let binding = WorkflowBinding {
            workflow_id: summary.id,
            text_input: "p.tags".into(),
            inputs: Default::default(),
            asset_inputs: Default::default(),
            asset_ranges: Default::default(),
        };
        assert!(prepare(
            &summary,
            &graph,
            &project,
            &binding,
            "new music",
            OutputKind::Audio,
            Some(&definitions)
        )
        .is_ok());
    }
    fn fixture() -> (WorkflowSummary, Value, Project, WorkflowBinding) {
        let id = Uuid::new_v4();
        (
            WorkflowSummary {
                id,
                name: "Actual graph".into(),
                path: "workflows/example.json".into(),
                input_roles: vec![],
                output_roles: vec!["image".into()],
                node_count: 4,
            },
            json!({"p":{"class_type":"CLIPTextEncode","inputs":{"text":"old positive"}}, "n":{"class_type":"CLIPTextEncode","inputs":{"text":"original negative"}}, "m":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"correct.safetensors"}}, "s":{"class_type":"KSampler","inputs":{"positive":["p",0],"negative":["n",0],"model":["m",0],"seed":42}}}),
            Project::new("Test".into(), None),
            WorkflowBinding {
                workflow_id: id,
                text_input: "p.text".into(),
                inputs: BTreeMap::new(),
                asset_inputs: BTreeMap::new(),
                asset_ranges: BTreeMap::new(),
            },
        )
    }
    #[test]
    fn explicit_text_changes_only_that_field_preserving_negative_model_and_graph() {
        let (summary, graph, project, binding) = fixture();
        let before = graph.clone();
        let request = prepare(
            &summary,
            &graph,
            &project,
            &binding,
            "new scene",
            OutputKind::Image,
            None,
        )
        .unwrap();
        assert_eq!(request.inputs.len(), 1);
        assert_eq!(request.inputs["p.text"], "new scene");
        assert_eq!(graph, before);
        let mut patched = graph.clone();
        for (address, value) in request.inputs {
            let (node, input) = address.split_once('.').unwrap();
            patched[node]["inputs"][input] = value;
        }
        assert_eq!(patched["n"]["inputs"]["text"], "original negative");
        assert_eq!(patched["m"]["inputs"]["ckpt_name"], "correct.safetensors");
        assert_eq!(patched["s"], graph["s"]);
    }
    #[test]
    fn real_positive_clip_titles_and_camelcase_system_prompt_are_distinguished() {
        let (summary, mut graph, project, mut binding) = fixture();
        graph["p"]["_meta"] = json!({"title":"CLIP Text Encode (Prompt)"});
        graph["system"] = json!({"class_type":"PrimitiveStringMultiline","_meta":{"title":"(Input:SystemPrompt)"},"inputs":{"value":"Keep the exact instructions"}});
        let descriptor = describe(&summary, &graph, None).unwrap();
        assert!(descriptor
            .text_inputs
            .iter()
            .any(|control| control.address == "p.text"));
        assert!(!descriptor
            .text_inputs
            .iter()
            .any(|control| control.address == "system.value"));
        binding.text_input = "system.value".into();
        assert!(prepare(
            &summary,
            &graph,
            &project,
            &binding,
            "narration",
            OutputKind::Audio,
            None
        )
        .is_err());
    }
    #[test]
    fn ambiguity_protected_branches_links_and_wrong_types_are_never_guessed() {
        let (summary, mut graph, project, mut binding) = fixture();
        graph["other"] = json!({"class_type":"String","inputs":{"text":"other positive"}});
        assert_eq!(
            describe(&summary, &graph, None).unwrap().text_inputs.len(),
            2
        );
        for address in ["", "n.text", "m.ckpt_name", "s.positive", "missing.text"] {
            binding.text_input = address.into();
            assert!(
                prepare(
                    &summary,
                    &graph,
                    &project,
                    &binding,
                    "scene",
                    OutputKind::Video,
                    None
                )
                .is_err(),
                "{address}"
            );
        }
        binding.text_input = "p.text".into();
        binding.inputs.insert("s.seed".into(), json!("wrong type"));
        assert!(prepare(
            &summary,
            &graph,
            &project,
            &binding,
            "scene",
            OutputKind::Image,
            None
        )
        .is_err());
        binding.inputs.clear();
        binding
            .inputs
            .insert("s.positive".into(), json!("cannot replace link"));
        assert!(prepare(
            &summary,
            &graph,
            &project,
            &binding,
            "scene",
            OutputKind::Image,
            None
        )
        .is_err());
    }
    #[test]
    fn actual_schema_choices_validate_explicit_model_changes() {
        let (summary, graph, project, mut binding) = fixture();
        let defs = json!({"CheckpointLoaderSimple":{"input":{"required":{"ckpt_name":[["correct.safetensors","other.safetensors"]]}}}});
        binding
            .inputs
            .insert("m.ckpt_name".into(), json!("nonexistent.safetensors"));
        assert!(prepare(
            &summary,
            &graph,
            &project,
            &binding,
            "scene",
            OutputKind::Image,
            Some(&defs)
        )
        .is_err());
        binding
            .inputs
            .insert("m.ckpt_name".into(), json!("other.safetensors"));
        assert!(prepare(
            &summary,
            &graph,
            &project,
            &binding,
            "scene",
            OutputKind::Image,
            Some(&defs)
        )
        .is_ok());
    }
    #[test]
    fn verified_core_audio_video_and_explicit_upload_widgets_are_typed() {
        let (summary, mut graph, mut project, mut binding) = fixture();
        graph["a"] = json!({"class_type":"LoadAudio","inputs":{"audio":"existing.wav"}});
        graph["v"] = json!({"class_type":"LoadVideo","inputs":{"file":"existing.mp4"}});
        graph["i"] = json!({"class_type":"CustomLoad","inputs":{"image":"existing.png"}});
        let asset_id = Uuid::new_v4();
        project.assets.push(Asset {
            id: asset_id,
            name: "Video".into(),
            kind: AssetKind::Video,
            file_name: "video.mp4".into(),
            source_path: String::new(),
            imported_at: chrono::Utc::now(),
            media: Some(MediaInfo {
                duration_ms: Some(1000),
                width: Some(640),
                height: Some(360),
                frame_rate: Some(30.0),
                has_audio: true,
            }),
        });
        binding.asset_inputs.insert("a.audio".into(), asset_id);
        binding.asset_inputs.insert("v.file".into(), asset_id);
        assert!(prepare(
            &summary,
            &graph,
            &project,
            &binding,
            "narration",
            OutputKind::Audio,
            None
        )
        .is_ok());
        project.assets[0].media.as_mut().unwrap().has_audio = false;
        assert!(prepare(
            &summary,
            &graph,
            &project,
            &binding,
            "narration",
            OutputKind::Audio,
            None
        )
        .is_err());
        let controls = describe(&summary, &graph, None).unwrap();
        assert!(
            !controls
                .reference_inputs
                .iter()
                .find(|input| input.address == "i.image")
                .unwrap()
                .upload_supported
        );
        let defs = json!({"CustomLoad":{"input":{"required":{"image":[["existing.png"],{"image_upload":true}]}}}});
        assert!(
            describe(&summary, &graph, Some(&defs))
                .unwrap()
                .reference_inputs
                .iter()
                .find(|input| input.address == "i.image")
                .unwrap()
                .upload_supported
        );
    }
}
