//! NL Video Suite panels: Generate, Music, ASR, Clipping, Rough Cut, Story, Queue and Settings.
//!
//! Each panel is a dockable surface over the engine's NL commands: it reads the session and calls
//! commands, so everything it does is also available to the CLI, the control channel and agents.
//! The NL preferences (Settings ▸ NL Video Suite) are the defaults; a panel's own controls only
//! override them for the run being launched. Nothing here downloads a model or starts a service.
//!
//! Automation ids: `nl.<panel>.<control>`.

use egui::{Rect, RichText, vec2};
use serde_json::{Value, json};

use crate::FilmcraftApp;
use crate::dock::PanelKind;

/// One panel body. Returned as a function pointer so [`every_nl_panel_has_a_body`] can prove that
/// every NL panel in [`crate::dock::NL_PANELS`] has a renderer instead of falling through.
type Body = fn(&mut FilmcraftApp, &mut egui::Ui);

fn body(panel: PanelKind) -> Option<Body> {
    match panel {
        PanelKind::NlGenerate => Some(generate),
        PanelKind::NlMusic => Some(music),
        PanelKind::NlAsr => Some(asr),
        PanelKind::NlClipping => Some(clipping),
        PanelKind::NlSgrossa => Some(sgrossa),
        PanelKind::NlStory => Some(story),
        PanelKind::NlAnalyze => Some(analyze),
        PanelKind::NlSubtitles => Some(super::nl_studio::subtitles),
        PanelKind::NlTitles => Some(super::nl_studio::titles),
        PanelKind::NlGraphics => Some(super::nl_studio::graphics_panel),
        PanelKind::NlAudio => Some(super::nl_studio::audio_panel),
        PanelKind::NlDelivery => Some(super::nl_studio::delivery_panel),
        PanelKind::NlAlbum => Some(super::nl_studio::album_panel),
        PanelKind::NlVersions => Some(super::nl_studio::versions_panel),
        PanelKind::NlMontage => Some(super::nl_studio::montage_panel),
        PanelKind::NlBridge => Some(super::nl_studio::bridge_panel),
        PanelKind::NlQueue => Some(queue),
        PanelKind::NlSettings => Some(settings),
        PanelKind::NlCover => Some(cover),
        PanelKind::NlLongform => Some(longform),
        PanelKind::NlTranscript => Some(transcript),
        PanelKind::NlSearch => Some(search),
        PanelKind::NlSpeech => Some(speech),
        PanelKind::NlHome => Some(home),
        PanelKind::NlAgent => Some(agent),
        PanelKind::NlShots => Some(shots),
        PanelKind::NlSocial => Some(social),
        PanelKind::NlStudio => Some(studio),
        PanelKind::NlServices => Some(services),
        PanelKind::NlWorkflows => Some(workflows_panel),
        _ => None,
    }
}

/// The engine commands each NL panel is built on. The test below checks every id against the
/// engine's registry, so a panel cannot point at a command that no longer exists (the ASR panel's
/// old `transcript.importSubtitle` promise was exactly that).
pub const PANEL_COMMANDS: &[(PanelKind, &[&str])] = &[
    (PanelKind::NlHome, &[]),
    (
        PanelKind::NlAgent,
        &[
            "nl.agent.plan",
            "nl.agent.sketch",
            "nl.agent.add",
            "nl.agent.prompts",
            "nl.agent.prompt",
            "nl.agent.generate",
            "nl.agent.collect",
            "nl.agent.chat",
            "nl.agent.workflows",
            "nl.agent.place",
            "nl.agent.characterSheet",
            "nl.agent.animate",
            "nl.agent.audio",
            "nl.agent.gap",
            "nl.agent.update",
            "nl.agent.squad",
        ],
    ),
    (
        PanelKind::NlGenerate,
        &[
            "nl.models.workflows",
            "graphics.generateComfy",
            "nl.generate.text",
            "nl.generate.extend",
            "nl.prompt.enhance",
            "nl.prompt.h3",
            "nl.timeline.analyze",
        ],
    ),
    (PanelKind::NlMusic, &["nl.models.workflows", "audio.generateMusic"]),
    (PanelKind::NlAsr, &["nl.asr.models", "transcript.transcribeNl", "captions.import", "nl.translate.captions", "nl.longform.analyze"]),
    (
        PanelKind::NlClipping,
        &["nl.clipping.propose", "nl.clipping.render", "nl.clipping.sequences", "nl.reframe.sequence", "nl.reframe.active", "nl.subtitles.srt"],
    ),
    (PanelKind::NlSgrossa, &["nl.sgrossa.analyze", "nl.sgrossa.apply", "nl.sgrossa.report"]),
    (PanelKind::NlAnalyze, &["nl.longform.analyze", "nl.analyze.segment", "graphics.upscaleVideo", "nl.search.index", "nl.search.query", "nl.translate.captions", "nl.assistant.ask"]),
    (PanelKind::NlTranscript, &["transcript.transcribeNl", "captions.import", "transcript.createCaptions", "nl.translate.captions", "nl.longform.analyze"]),
    (
        PanelKind::NlSubtitles,
        &["captions.list", "captions.setText", "captions.split", "captions.merge", "captions.delete", "captions.import", "captions.export"],
    ),
    (PanelKind::NlTitles, &["graphics.template.list", "nl.titles.add"]),
    (PanelKind::NlGraphics, &["nl.graphics.add"]),
    (PanelKind::NlAudio, &["essentialSound.inspect", "essentialSound.set", "mixer.inspect"]),
    (PanelKind::NlDelivery, &["nl.delivery.qa", "nl.delivery.package", "nl.delivery.social", "nl.storage.cleanup", "nl.storage.status"]),
    (PanelKind::NlAlbum, &["nl.album.list", "nl.album.insert"]),
    (PanelKind::NlVersions, &["nl.versions.list", "nl.versions.save", "nl.versions.restore", "nl.versions.delete"]),
    (PanelKind::NlMontage, &["nl.montage.plan", "nl.montage.apply"]),
    (PanelKind::NlBridge, &["nl.bridge.plan", "nl.bridge.generate", "nl.bridge.place", "nl.agent.gap"]),
    (
        PanelKind::NlStory,
        &[
            "nl.story.get",
            "nl.story.draft",
            "nl.story.update",
            "nl.story.generate",
            "nl.story.place",
            "nl.story.placeAll",
            "nl.story.shotmap",
            "nl.story.extendAudio",
            "nl.story.placeAudio",
        ],
    ),
    (PanelKind::NlQueue, &["nl.queue.list", "nl.queue.add", "nl.queue.prepare", "nl.queue.send", "nl.queue.remove", "nl.queue.free", "nl.batch.add", "nl.batch.list", "nl.batch.remove", "nl.batch.clear", "nl.batch.start"]),
    (PanelKind::NlSpeech, &["nl.asr.models", "nl.asr.transcribe"]),
    (PanelKind::NlSettings, &["nl.asr.models", "nl.size.preview", "nl.models.ollama"]),
    (PanelKind::NlCover, &["nl.cover.prompt", "nl.cover.set"]),
    (
        PanelKind::NlWorkflows,
        &[
            "nl.workflows.list",
            "nl.workflows.verify",
            "nl.workflows.diagnose",
            "nl.workflows.fix",
            "nl.workflows.parameters",
            "nl.workflows.import",
            "nl.workflows.presets",
        ],
    ),
    (PanelKind::NlLongform, &["nl.longform.analyze", "nl.longform.get", "nl.longform.markers", "nl.longform.export", "nl.sgrossa.analyze"]),
    (PanelKind::NlSearch, &["nl.search.index", "nl.search.query"]),
    (PanelKind::NlShots, &["clip.sceneEditDetection"]),
    (PanelKind::NlSocial, &["nl.delivery.social"]),
    (PanelKind::NlStudio, &[]),
    (PanelKind::NlServices, &["nl.services.status", "nl.models.llm", "nl.storage.status", "project.inspect"]),
];

/// Which panel body to draw.
pub fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect, panel: PanelKind) {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(12.0, 8.0))));
    let ui = &mut child;
    context(app, ui);
    egui::ScrollArea::vertical().id_salt(("nl-panel", panel.id())).auto_shrink([false, false]).show(ui, |ui| {
        if let Some(body) = body(panel) {
            body(app, ui);
        }
    });
}

/// The one line every NL panel shows before its own controls: the open project, the active
/// sequence, the playhead and selection, and the current NL parameters. It is registered as
/// `nl.context`, so the panel audit can prove that each panel is looking at the sequence that is
/// actually open instead of a placeholder.
fn context(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    let project = app.session.project.name.clone();
    let selection = app.session.state.selection.len();
    let (sequence_line, parameters) = match app.session.active_sequence() {
        Some(sequence) => {
            let name = app
                .session
                .state
                .active_sequence
                .and_then(|id| app.session.project.item(id))
                .map(|item| item.name.clone())
                .unwrap_or_else(|| "Sequence".into());
            let settings = &sequence.settings;
            let fps = settings.frame_rate.num as f64 / settings.frame_rate.den.max(1) as f64;
            let duration = sequence.duration().seconds();
            let playhead = app.session.playhead().seconds();
            let cues: usize = sequence.caption_tracks.iter().map(|track| track.captions.len()).sum();
            let words = filmcraft_engine::transcript::sequence_words(&app.session).len();
            let line = format!(
                "Progetto: {project} · Sequenza: {name} · {}×{} @ {fps:.2} fps · {playhead:.1}/{duration:.1} s · selezione: {selection}",
                settings.width, settings.height
            );
            let nl = &app.session.prefs.nl;
            let parameters = format!(
                "Trascrizione: {words} parole · {cues} cue · ASR: {} / {} · ComfyUI: {}",
                nl.asr_provider(),
                if nl.asr_model.trim().is_empty() { "portable" } else { nl.asr_model.as_str() },
                nl.comfy_url()
            );
            (line, parameters)
        }
        None => (
            format!("Progetto: {project} · Nessuna sequenza aperta · selezione: {selection}"),
            format!("ASR: {} · ComfyUI: {}", app.session.prefs.nl.asr_provider(), app.session.prefs.nl.comfy_url()),
        ),
    };
    let dim = app.tokens.text_dim;
    let r = ui.label(RichText::new(&sequence_line).size(10.5).color(dim));
    app.auto.add("nl.context", r.rect, &sequence_line);
    ui.label(RichText::new(parameters).size(10.0).color(app.tokens.text_faint));
    ui.separator();
}

// ---------------------------------------------------------------- shared widgets

pub(super) fn title(app: &mut FilmcraftApp, ui: &mut egui::Ui, title: &str, subtitle: &str) {
    let t = app.tokens;
    ui.add_space(2.0);
    ui.label(RichText::new(title).size(17.0).strong().color(t.text));
    ui.label(RichText::new(subtitle).size(12.0).color(t.text_dim));
    ui.add_space(6.0);
}

pub(super) fn error_line(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    drain_nl_queue(app, &ui.ctx().clone());
    let status = app.ui.status.clone();
    if status.is_empty() {
        return;
    }
    let lower = status.to_ascii_lowercase();
    let needs_transcript = lower.contains("transcript") || lower.contains("trascriz") || lower.contains("asr");
    let needs_comfy = lower.contains("comfyui") && (lower.contains("reach") || lower.contains("connect") || lower.contains("not running") || lower.contains("impossibile"));
    let needs_models = lower.contains("modello") || lower.contains("model") || lower.contains("nodo") || lower.contains("node");
    let needs_story = lower.contains("story") || lower.contains("storia");
    let needs_ollama = lower.contains("ollama") || lower.contains("vision model") || lower.contains("llm");
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&status).size(12.0).color(egui::Color32::from_rgb(233, 120, 120)));
    });
    if needs_transcript {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Serve prima la trascrizione.").size(11.0).color(app.tokens.text_dim));
            if ui.button("Apri Trascrizione / ASR").clicked() {
                app.show_panel(PanelKind::NlTranscript);
            }
            if ui.button("Trascrivi ora").clicked() {
                run(app, ui, "transcript.transcribeNl", json!({"language": "it", "device": "auto"}));
            }
            if ui.button("Poi analizza capitoli").clicked() {
                app.show_panel(PanelKind::NlLongform);
            }
        });
    } else if needs_comfy || needs_models {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Controlla servizi e workflow prima di riprovare.").size(11.0).color(app.tokens.text_dim));
            if ui.button("Apri Servizi NL").clicked() {
                app.show_panel(PanelKind::NlServices);
            }
            if ui.button("Apri Workflow").clicked() {
                app.show_panel(PanelKind::NlWorkflows);
            }
        });
    } else if needs_ollama {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Configura il modello/provider.").size(11.0).color(app.tokens.text_dim));
            if ui.button("Apri Impostazioni NL").clicked() {
                app.show_panel(PanelKind::NlSettings);
            }
        });
    } else if needs_story {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Apri NL Story").clicked() {
                app.show_panel(PanelKind::NlStory);
            }
        });
    }
    ui.add_space(4.0);
}

fn nl_queue_id() -> egui::Id {
    egui::Id::unique("nl-command-queue")
}

fn active_jobs(app: &FilmcraftApp) -> usize {
    app.session.jobs.iter().filter(|job| !job.progress.finished.load(std::sync::atomic::Ordering::Relaxed)).count()
}

fn drain_nl_queue(app: &mut FilmcraftApp, ctx: &egui::Context) {
    if active_jobs(app) > 0 {
        return;
    }
    let next = ctx.data_mut(|data| {
        let mut queue: std::collections::VecDeque<(String, Value)> = data.get_temp(nl_queue_id()).unwrap_or_default();
        let next = queue.pop_front();
        data.insert_temp(nl_queue_id(), queue);
        next
    });
    if let Some((id, params)) = next
        && let Err(error) = crate::menus::invoke(app, ctx, &id, params)
    {
        app.ui.status = error;
    }
}

/// Run one command for a button press, showing a failure in the panel status line. While another
/// NL/ComfyUI job is running the command waits in the internal queue instead of being refused.
pub(super) fn run(app: &mut FilmcraftApp, ui: &egui::Ui, id: &str, params: Value) {
    let ctx = ui.ctx().clone();
    drain_nl_queue(app, &ctx);
    if active_jobs(app) > 0 {
        ctx.data_mut(|data| {
            let mut queue: std::collections::VecDeque<(String, Value)> = data.get_temp(nl_queue_id()).unwrap_or_default();
            queue.push_back((id.to_string(), params));
            data.insert_temp(nl_queue_id(), queue);
        });
        let pending = app.session.jobs.iter().filter(|job| !job.progress.finished.load(std::sync::atomic::Ordering::Relaxed)).count();
        app.ui.status = format!("In coda: {pending} job attivo/i, il comando partirà appena liberi.");
        return;
    }
    if let Err(e) = crate::menus::invoke(app, &ctx, id, params) {
        app.ui.status = e;
    }
}

/// A labelled single-line field with its automation id.
pub(super) fn field(app: &mut FilmcraftApp, ui: &mut egui::Ui, key: &str, label: &str, text: &mut String) -> egui::Response {
    ui.label(RichText::new(label).size(12.0).color(app.tokens.text_dim));
    let r = crate::widgets::accessible_name(ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY).id_salt(key)), label);
    app.auto.add(&format!("nl.{key}"), r.rect, label);
    r
}

pub(super) fn button(app: &mut FilmcraftApp, ui: &mut egui::Ui, key: &str, label: &str) -> bool {
    let r = ui.add(egui::Button::new(RichText::new(label).size(13.0)).min_size(vec2(0.0, 26.0)));
    app.auto.add(&format!("nl.{key}"), r.rect, label);
    r.clicked()
}

fn combo(app: &mut FilmcraftApp, ui: &mut egui::Ui, key: &str, label: &str, value: &mut usize, options: &[(String, String)]) {
    ui.label(RichText::new(label).size(12.0).color(app.tokens.text_dim));
    let current = options.get(*value).map(|(_, label)| label.clone()).unwrap_or_else(|| "—".into());
    let r = craft_ui::ComboBox::from_id_salt(key).selected_text(current).width(ui.available_width().min(520.0)).show_ui(ui, |ui| {
        for (i, (_, label)) in options.iter().enumerate() {
            ui.selectable_value(value, i, label);
        }
    });
    app.auto.add(&format!("nl.{key}"), r.response.rect, label);
    crate::widgets::accessible_name(r.response, label);
}

/// The last job whose label starts with `prefix`, with its status line.
fn job_line(app: &mut FilmcraftApp, ui: &mut egui::Ui, prefix: &str) {
    let Some(job) = app.session.jobs.iter().rev().find(|j| j.label.starts_with(prefix)) else { return };
    let progress = job.progress.fraction();
    let status = job.progress.status.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let finished = job.progress.finished.load(std::sync::atomic::Ordering::Relaxed);
    let mut line = format!("{}: {:.0}%", if finished { "Last job" } else { "Running" }, progress * 100.0);
    if !status.is_empty() {
        line.push_str(" · ");
        line.push_str(&status);
    }
    ui.label(RichText::new(line).size(12.0).color(app.tokens.text_dim));
}

/// “Elimina workflow” beside the selector: first click asks confirmation, the second moves the
/// JSON to `da_eliminare/workflows` (never deletes it outright).
fn delete_workflow_button(app: &mut FilmcraftApp, ui: &mut egui::Ui, kind: &str, path: &str) {
    if path.is_empty() {
        return;
    }
    let id = egui::Id::new(("nl-delete-workflow", path));
    let armed = ui.data(|data| data.get_temp::<bool>(id)).unwrap_or(false);
    let label = if armed { "Conferma eliminazione" } else { "Elimina workflow" };
    if ui.button(label).clicked() {
        if !armed {
            ui.data_mut(|data| data.insert_temp(id, true));
            return;
        }
        match app.session.execute("nl.workflows.delete", json!({"workflow": path})) {
            Ok(_) => {
                ui.data_mut(|data| {
                    data.remove::<bool>(id);
                    data.remove::<(f64, Vec<(String, String)>)>(egui::Id::unique(("nl-workflows", kind)));
                });
                app.ui.status = "Workflow spostato in da_eliminare/workflows".into();
            }
            Err(error) => {
                ui.data_mut(|data| data.remove::<bool>(id));
                app.ui.status = error.to_string();
            }
        }
    }
}

/// Workflows for a kind, read from the engine and refreshed when empty or older than a few
/// seconds; `select` is the draft's index into the list.
fn workflow_options(app: &mut FilmcraftApp, ui: &egui::Ui, kind: &str) -> Vec<(String, String)> {
    let id = egui::Id::unique(("nl-workflows", kind));
    let now = ui.input(|i| i.time);
    let cached: Option<(f64, Vec<(String, String)>)> = ui.data(|d| d.get_temp(id));
    if let Some((at, list)) = &cached
        && now - at < 5.0
        && !list.is_empty()
    {
        return list.clone();
    }
    let list = match app.session.execute("nl.models.workflows", json!({"kind": kind})) {
        Ok(value) => value["workflows"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .map(|w| {
                        (
                            w["path"].as_str().unwrap_or_default().to_string(),
                            format!(
                                "{} · {} · {}",
                                w["category"].as_str().unwrap_or("other"),
                                w["kind"].as_str().unwrap_or(""),
                                w["name"].as_str().unwrap_or("workflow")
                            ),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        Err(e) => {
            app.ui.status = e.to_string();
            Vec::new()
        }
    };
    ui.data_mut(|d| d.insert_temp(id, (now, list.clone())));
    list
}

/// Ask the local LLM for a workflow-specific prompt. This is the guided step: the engine chooses
/// the model guide from the workflow name (MiniMax H3, Flux2/Krea/Z-image, Wan, music) and returns
/// one validated prompt, which the panel writes back into the field.
fn auto_prompt(app: &mut FilmcraftApp, idea: &str, kind: &str, workflow: &str) -> std::result::Result<String, String> {
    let result = app
        .session
        .execute("nl.prompt.enhance", json!({"brief": idea, "kind": kind, "workflow": workflow, "language": "it", "wait": true}))
        .map_err(|e| e.to_string())?;
    result["prompt"].as_str().map(str::to_string).ok_or_else(|| "the model returned no prompt".to_string())
}

/// The parameters a Generate click sends. The compact action row is above the long advanced
/// fields, so the operator can always reach Generate without scrolling a docked panel.
fn generate_params(d: &GenerateDraft, kind: &str, options: &[(String, String)]) -> Value {
    let mut params = json!({
        "prompt": d.prompt.trim(),
        "seed": d.seed,
        "resolutionMode": kind,
        "kind": kind,
    });
    if let Some((path, _)) = options.get(d.workflow) {
        params["workflow"] = json!(path);
    }
    if kind == "video" {
        params["videoScale"] = json!("1/3");
    } else {
        params["imageScale"] = json!(if d.scale == 1 { "2x" } else { "1x" });
    }
    if !d.negative.trim().is_empty() {
        params["negative"] = json!(d.negative.trim());
    }
    if !d.overrides.trim().is_empty()
        && let Ok(overrides) = serde_json::from_str::<Value>(&d.overrides)
        && overrides.is_array()
    {
        params["overrides"] = overrides;
    }
    let mut overrides: Vec<Value> = params["overrides"].as_array().cloned().unwrap_or_default();
    for (key, value) in &d.param_overrides {
        if let Some((node, field)) = key.split_once('.') {
            overrides.push(json!({"node": node, "field": field, "value": value}));
        }
    }
    if !overrides.is_empty() {
        params["overrides"] = json!(overrides);
    }
    if !d.character.trim().is_empty() {
        params["character"] = json!(d.character.trim());
    }
    if !d.background.trim().is_empty() {
        params["background"] = json!(d.background.trim());
    }
    let images: Vec<String> = d.ref_images.iter().map(|path| path.trim()).filter(|path| !path.is_empty()).map(str::to_string).collect();
    if !images.is_empty() {
        params["images"] = json!(images);
    }
    let videos: Vec<String> = d.videos.iter().map(|path| path.trim()).filter(|path| !path.is_empty()).map(str::to_string).collect();
    if !videos.is_empty() {
        params["videos"] = json!(videos);
    }
    let audios: Vec<String> = d.audios.iter().map(|path| path.trim()).filter(|path| !path.is_empty()).map(str::to_string).collect();
    if !audios.is_empty() {
        params["audios"] = json!(audios);
    }
    if !d.lora.trim().is_empty() {
        params["loras"] = json!([{"name": d.lora.trim(), "strength": d.lora_strength.trim().parse::<f64>().unwrap_or(1.0)}]);
    }
    params
}

/// The real widgets of a workflow from ComfyUI's `/object_info`, cached for a few seconds.
fn workflow_parameters(app: &mut FilmcraftApp, ui: &egui::Ui, path: &str) -> Value {
    if path.is_empty() {
        return Value::Null;
    }
    let id = egui::Id::unique(("nl-workflow-params", path));
    let now = ui.input(|i| i.time);
    if let Some((at, value)) = ui.data(|d| d.get_temp::<(f64, Value)>(id))
        && now - at < 5.0
    {
        return value;
    }
    let value = app.session.execute("nl.workflows.parameters", json!({"workflow": path, "api": true})).unwrap_or(Value::Null);
    ui.data_mut(|d| d.insert_temp(id, (now, value.clone())));
    value
}

/// Dropdowns for the workflow's real model/LoRA/sampler widgets, exactly as ComfyUI reports them.
/// Choosing a value becomes an `overrides` entry, so no JSON has to be typed by hand.
fn model_parameter_section(app: &mut FilmcraftApp, ui: &mut egui::Ui, workflow: &str, d: &mut GenerateDraft) {
    let report = workflow_parameters(app, ui, workflow);
    if report.is_null() {
        return;
    }
    if d.params_workflow != workflow {
        d.param_overrides.clear();
        d.params_workflow = workflow.to_string();
    }
    let fields = [
        "unet_name",
        "ckpt_name",
        "lora_name",
        "vae_name",
        "clip_name",
        "text_encoder",
        "model_name",
        "control_net_name",
        "sampler_name",
        "scheduler",
        "steps",
        "cfg",
        "denoise",
        "weight_dtype",
        "type",
    ];
    let parameters: Vec<Value> = report["parameters"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|row| row.get("linked").and_then(Value::as_bool) != Some(true))
                .filter(|row| row.get("field").and_then(Value::as_str).is_some_and(|field| fields.contains(&field)))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if parameters.is_empty() {
        return;
    }
    ui.collapsing(format!("Modelli, LoRA e parametri ({})", parameters.len()), |ui| {
        if button(app, ui, "generate.params_reset", "Ripristina i valori del workflow") {
            d.param_overrides.clear();
        }
        for row in parameters.into_iter().take(14) {
            let node = row["node"].as_str().unwrap_or_default();
            let field = row["field"].as_str().unwrap_or_default();
            let key = format!("{node}.{field}");
            let current = d.param_overrides.get(&key).cloned().unwrap_or_else(|| row["value"].clone());
            let label = format!("{} · {field}", row["title"].as_str().unwrap_or(node));
            match row["kind"].as_str() {
                Some("choice") => {
                    let choices: Vec<String> =
                        row["choices"].as_array().map(|c| c.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
                    let selected = current.as_str().unwrap_or("").to_string();
                    ui.label(RichText::new(&label).size(10.0).color(app.tokens.text_dim));
                    let combo = craft_ui::ComboBox::from_id_salt(("nl-param", key.clone()))
                        .selected_text(selected.clone())
                        .width(ui.available_width().min(430.0))
                        .show_ui(ui, |ui| {
                            for choice in &choices {
                                if ui.selectable_label(*choice == selected, choice).clicked() {
                                    d.param_overrides.insert(key.clone(), json!(choice));
                                }
                            }
                        });
                    app.auto.add(&format!("nl.generate.param.{key}"), combo.response.rect, &label);
                }
                Some("int") | Some("float") => {
                    let mut number = current.as_f64().unwrap_or(0.0);
                    ui.label(RichText::new(&label).size(10.0).color(app.tokens.text_dim));
                    let response = ui.add(egui::DragValue::new(&mut number).speed(1.0));
                    app.auto.add(&format!("nl.generate.param.{key}"), response.rect, &label);
                    if response.changed() {
                        d.param_overrides.insert(key.clone(), json!(number));
                    }
                }
                _ => {}
            }
        }
    });
}

/// Capabilities of the selected workflow: reference slots and LoRA nodes. Cached briefly because
/// the panel asks on every frame and the file does not change while the picker is open.
fn workflow_capabilities(app: &mut FilmcraftApp, ui: &egui::Ui, path: &str) -> Value {
    if path.is_empty() {
        return Value::Null;
    }
    let id = egui::Id::unique(("nl-workflow-caps", path));
    let now = ui.input(|i| i.time);
    if let Some((at, value)) = ui.data(|d| d.get_temp::<(f64, Value)>(id))
        && now - at < 5.0
    {
        return value;
    }
    let value = app.session.execute("nl.workflows.capabilities", json!({"workflow": path})).unwrap_or(Value::Null);
    ui.data_mut(|d| d.insert_temp(id, (now, value.clone())));
    value
}

/// Keys the native picker writes into the session result map. The map's keys are `'static`, so
/// each slot has one fixed key; the panel applies it on the next frame.
const PICK_REF_IMAGE: [&str; 8] =
    ["refpick.ref0", "refpick.ref1", "refpick.ref2", "refpick.ref3", "refpick.ref4", "refpick.ref5", "refpick.ref6", "refpick.ref7"];
const PICK_REF_VIDEO: [&str; 3] = ["refpick.video0", "refpick.video1", "refpick.video2"];
const PICK_REF_AUDIO: [&str; 3] = ["refpick.audio0", "refpick.audio1", "refpick.audio2"];

/// A path field with a native picker. The callback writes the picked path where the next frame can
/// apply it: a panel writes its draft copy back at the end of the frame, so writing the field from
/// inside the picker would be overwritten.
fn reference_row(app: &mut FilmcraftApp, ui: &mut egui::Ui, key: &str, pick_key: &'static str, label: &str, value: &mut String) {
    field(app, ui, key, label, value);
    if button(app, ui, &format!("{key}.browse"), "Scegli file…") {
        app.pick_ui(
            crate::native_dialogs::FileDialog::open_file(label, &["png", "jpg", "jpeg", "webp", "mp4", "mov", "mkv", "wav", "mp3", "flac"]),
            move |app, paths| {
                if let Some(path) = paths.first() {
                    app.session.nl.results.insert(pick_key, json!(path));
                }
            },
        );
    }
}

/// Take one path the picker wrote for `key`, if any.
fn picked_reference(app: &mut FilmcraftApp, key: &'static str) -> Option<String> {
    app.session.nl.results.remove(key).and_then(|value| value.as_str().map(str::to_string))
}

/// The reference fields a workflow really has. A T2V/T2I without loaders shows nothing, so the
/// panel never asks for a character sheet a text-only workflow cannot use.
fn reference_section(app: &mut FilmcraftApp, ui: &mut egui::Ui, workflow: &str, d: &mut GenerateDraft) {
    let caps = workflow_capabilities(app, ui, workflow);
    if caps.is_null() {
        return;
    }
    let images = caps["imageRefs"].as_array().map_or(0, Vec::len);
    let videos = caps["videoInputs"].as_array().map_or(0, Vec::len);
    let audios = caps["audioInputs"].as_array().map_or(0, Vec::len);
    let loras = caps["loras"].as_array().cloned().unwrap_or_default();
    let has_refs = images > 0 || videos > 0 || audios > 0;
    if !has_refs && loras.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.separator();
    ui.label(RichText::new(if has_refs { "Reference dell'oggetto" } else { "LoRA extra" }).size(12.0).strong());
    if images > 0 {
        ui.label(
            RichText::new(format!(
                "Il workflow ha {images} ingresso/i immagine; i primi due sono character sheet e background, gli altri reference opzionali."
            ))
            .size(10.0)
            .color(app.tokens.text_faint),
        );
        reference_row(app, ui, "generate.character", "refpick.character", "Character sheet (immagine 1)", &mut d.character);
        if images >= 2 {
            reference_row(app, ui, "generate.background", "refpick.background", "Background / location (immagine 2)", &mut d.background);
        }
        let extra = images.saturating_sub(2);
        if d.ref_images.len() < extra {
            d.ref_images.resize(extra, String::new());
        }
        for index in 0..extra.min(PICK_REF_IMAGE.len()) {
            let label = format!("Reference image {}", index + 3);
            reference_row(app, ui, &format!("generate.ref{index}"), PICK_REF_IMAGE[index], &label, &mut d.ref_images[index]);
        }
    }
    if videos > 0 {
        if d.videos.len() < videos {
            d.videos.resize(videos, String::new());
        }
        for index in 0..videos.min(PICK_REF_VIDEO.len()) {
            let label = format!("Video reference {}", index + 1);
            reference_row(app, ui, &format!("generate.video{index}"), PICK_REF_VIDEO[index], &label, &mut d.videos[index]);
        }
    }
    if audios > 0 {
        if d.audios.len() < audios {
            d.audios.resize(audios, String::new());
        }
        for index in 0..audios.min(PICK_REF_AUDIO.len()) {
            let label = format!("Audio reference {}", index + 1);
            reference_row(app, ui, &format!("generate.audio{index}"), PICK_REF_AUDIO[index], &label, &mut d.audios[index]);
        }
    }
    if !loras.is_empty() {
        field(app, ui, "generate.lora", "LoRA extra (nome file)", &mut d.lora);
        field(app, ui, "generate.lora_strength", "LoRA strength", &mut d.lora_strength);
    }
    if has_refs {
        ui.horizontal_wrapped(|ui| {
            if button(app, ui, "generate.reference_tags", "Aggiungi tag <Picture N>") {
                let mut tags = Vec::new();
                let count = usize::from(!d.character.trim().is_empty()) + usize::from(!d.background.trim().is_empty());
                for index in 0..count {
                    tags.push(format!("<Picture {}>", index + 1));
                }
                for (index, path) in d.ref_images.iter().enumerate() {
                    if !path.trim().is_empty() {
                        tags.push(format!("<Picture {}>", index + 3));
                    }
                }
                if !tags.is_empty() {
                    if !d.prompt.trim().is_empty() {
                        d.prompt.push(' ');
                    }
                    d.prompt.push_str(&tags.join(" "));
                }
            }
            ui.label(RichText::new("MiniMax H3: i tag seguono l'ordine dei file.").size(10.0).color(app.tokens.text_faint));
        });
    }
    if let Some(first) = loras.first() {
        ui.label(
            RichText::new(format!("LoRA nel workflow: nodo {} · {}", first["node"].as_str().unwrap_or(""), first["name"].as_str().unwrap_or("(vuoto)")))
                .size(10.0)
                .color(app.tokens.text_faint),
        );
    }
}

/// The default workflow index for a kind from the NL preferences, else 0.
fn default_workflow(options: &[(String, String)], app: &FilmcraftApp, kind: &str) -> usize {
    let nl = &app.session.prefs.nl;
    let wanted = match kind {
        "video" => nl.comfy_video_workflow.as_str(),
        "audio" => nl.comfy_music_workflow.as_str(),
        _ => nl.comfy_image_workflow.as_str(),
    };
    let wanted_is_sheet = wanted.to_ascii_lowercase().contains("character_sheet");
    if !wanted.is_empty() && !wanted_is_sheet {
        if let Some(i) = options.iter().position(|(path, _)| path == wanted) {
            return i;
        }
        // The workflow library is reorganised into category folders; the preference still names
        // the old relative path. Match the file name instead of falling back to a character sheet.
        let wanted_name = std::path::Path::new(wanted).file_name().and_then(|n| n.to_str());
        if let Some(name) = wanted_name
            && let Some(i) = options.iter().position(|(path, _)| std::path::Path::new(path).file_name().and_then(|n| n.to_str()) == Some(name))
        {
            return i;
        }
    }
    let preferred = match kind {
        "video" => "reference-to-video",
        "audio" => "music",
        _ => "text-to-image",
    };
    options
        .iter()
        .position(|(_, label)| label.contains(preferred) && !label.contains("character-sheet"))
        .or_else(|| options.iter().position(|(_, label)| !label.contains("character-sheet")))
        .unwrap_or(0)
}

/// The size a generation will use, read from the engine so the panel shows the real number.
fn size_preview(app: &mut FilmcraftApp, ui: &mut egui::Ui, mode: &str, scale: &str) {
    let mut params = json!({"resolutionMode": mode});
    if mode == "image" {
        params["imageScale"] = json!(scale);
    } else {
        params["videoScale"] = json!(scale);
    }
    match app.session.execute("nl.size.preview", params) {
        Ok(size) => {
            let text = format!("{} × {} px", size["width"], size["height"]);
            ui.label(RichText::new(format!("Derived from the open sequence: {text}")).size(12.0).color(app.tokens.text_dim));
        }
        Err(e) => {
            ui.label(RichText::new(e.to_string()).size(12.0).color(egui::Color32::from_rgb(233, 180, 120)));
        }
    }
}

// ---------------------------------------------------------------- draft state

#[derive(Clone, Default)]
struct GenerateDraft {
    prompt: String,
    negative: String,
    seed: u64,
    kind: usize,
    workflow: usize,
    scale: usize,
    overrides: String,
    auto_prompt: bool,
    param_overrides: std::collections::BTreeMap<String, Value>,
    params_workflow: String,
    character: String,
    background: String,
    ref_images: Vec<String>,
    videos: Vec<String>,
    audios: Vec<String>,
    lora: String,
    lora_strength: String,
}

pub(super) fn draft<T: Clone + Default + Send + Sync + 'static>(ui: &egui::Ui, id: &str) -> T {
    ui.data(|d| d.get_temp::<T>(egui::Id::unique(id))).unwrap_or_default()
}

pub(super) fn save_draft<T: Clone + Send + Sync + 'static>(ui: &egui::Ui, id: &str, value: T) {
    ui.data_mut(|d| d.insert_temp(egui::Id::unique(id), value));
}

// ---------------------------------------------------------------- panels

fn generate(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    let height = ui.available_height().clamp(160.0, 520.0);
    egui::ScrollArea::vertical().id_salt("nl-generate-scroll").max_height(height).auto_shrink([false, false]).show(ui, |ui| generate_body(app, ui));
}

fn generate_body(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(app, ui, "NL Generate", "Images from a prompt with the local ComfyUI. The size comes from the open sequence: 1× or 2×, rounded to the model's grid.");
    error_line(app, ui);
    let mut d: GenerateDraft = draft(ui, "nl-generate");
    if let Some(value) = picked_reference(app, "refpick.character") {
        d.character = value;
    }
    if let Some(value) = picked_reference(app, "refpick.background") {
        d.background = value;
    }
    for index in 0..PICK_REF_IMAGE.len() {
        if let Some(value) = picked_reference(app, PICK_REF_IMAGE[index]) {
            if d.ref_images.len() <= index {
                d.ref_images.resize(index + 1, String::new());
            }
            d.ref_images[index] = value;
        }
    }
    for index in 0..PICK_REF_VIDEO.len() {
        if let Some(value) = picked_reference(app, PICK_REF_VIDEO[index]) {
            if d.videos.len() <= index {
                d.videos.resize(index + 1, String::new());
            }
            d.videos[index] = value;
        }
    }
    for index in 0..PICK_REF_AUDIO.len() {
        if let Some(value) = picked_reference(app, PICK_REF_AUDIO[index]) {
            if d.audios.len() <= index {
                d.audios.resize(index + 1, String::new());
            }
            d.audios[index] = value;
        }
    }
    let kinds = [("image", "Image"), ("video", "Video")];
    let mut kind_label = kinds[d.kind.min(kinds.len() - 1)].1.to_string();
    let kind_combo = craft_ui::ComboBox::from_id_salt("nl-generate-kind").selected_text(&kind_label).show_ui(ui, |ui| {
        for (i, (_, label)) in kinds.iter().enumerate() {
            ui.selectable_value(&mut d.kind, i, *label);
        }
    });
    crate::widgets::accessible_name(kind_combo.response, "Generation kind");
    kind_label = kinds[d.kind.min(kinds.len() - 1)].1.to_string();
    let _ = &kind_label;
    let kind = kinds[d.kind.min(kinds.len() - 1)].0;
    let options = workflow_options(app, ui, kind);
    if d.workflow >= options.len() || (d.params_workflow.is_empty() && options.get(d.workflow).is_some_and(|(_, label)| label.contains("character-sheet"))) {
        d.workflow = default_workflow(&options, app, kind);
    }
    combo(app, ui, "generate.workflow", "Workflow", &mut d.workflow, &options);
    let selected_workflow = options.get(d.workflow).map(|(path, _)| path.clone()).unwrap_or_default();
    delete_workflow_button(app, ui, kind, &selected_workflow);
    if let Some((path, _)) = options.get(d.workflow).cloned() {
        model_parameter_section(app, ui, &path, &mut d);
    }
    ui.add_space(4.0);
    let mut prompt = d.prompt.clone();
    ui.label(RichText::new("Prompt").size(12.0).color(app.tokens.text_dim));
    let r = crate::widgets::accessible_name(
        ui.add(egui::TextEdit::multiline(&mut prompt).desired_rows(4).desired_width(f32::INFINITY).id_salt("nl-generate-prompt")),
        "Prompt",
    );
    app.auto.add("nl.generate.prompt", r.rect, "Prompt");
    d.prompt = prompt;
    let auto = ui.checkbox(&mut d.auto_prompt, "Prompt automatico per questo modello (LLM)");
    app.auto.add("nl.generate.auto_prompt", auto.rect, "Prompt automatico per questo modello");
    let top_params = generate_params(&d, kind, &options);
    ui.horizontal_wrapped(|ui| {
        if button(app, ui, "generate.run", "Generate") {
            if d.prompt.trim().is_empty() {
                app.ui.status = "Write a prompt first.".into();
            } else if d.auto_prompt {
                let workflow = options.get(d.workflow).map(|(path, _)| path.clone()).unwrap_or_default();
                match auto_prompt(app, d.prompt.trim(), kind, &workflow) {
                    Ok(text) => {
                        d.prompt = text.clone();
                        let mut params = generate_params(&d, kind, &options);
                        params["prompt"] = json!(text);
                        run(app, ui, "graphics.generateComfy", params);
                    }
                    Err(error) => app.ui.status = format!("Prompt AI: {error}"),
                }
            } else {
                run(app, ui, "graphics.generateComfy", top_params.clone());
            }
        }
        if button(app, ui, "generate.dry_run", "Dry run (no send)") {
            if d.prompt.trim().is_empty() {
                app.ui.status = "Write a prompt first.".into();
            } else {
                let mut dry = top_params.clone();
                dry["dryRun"] = json!(true);
                let ctx = ui.ctx().clone();
                match crate::menus::invoke(app, &ctx, "graphics.generateComfy", dry) {
                    Ok(value) => {
                        ui.data_mut(|data| data.insert_temp(egui::Id::unique("nl-generate-dry"), value));
                    }
                    Err(e) => app.ui.status = e,
                }
            }
        }
    });
    // Prompt AI: the local model writes a prompt from the brief; the operator applies it.
    ui.horizontal(|ui| {
        if button(app, ui, "generate.enhance", "Enhance prompt with AI") {
            let brief = if d.prompt.trim().is_empty() { "cinematic studio scene".to_string() } else { d.prompt.clone() };
            let ctx = ui.ctx().clone();
            if let Err(e) = crate::menus::invoke(app, &ctx, "nl.prompt.enhance", json!({"brief": brief, "kind": kind})) {
                app.ui.status = e;
            }
        }
        if let Some(suggestion) = app.session.nl.result("prompt").cloned()
            && let Some(text) = suggestion["prompt"].as_str().map(str::to_string)
        {
            ui.label(RichText::new("suggestion ready").size(11.0).color(app.tokens.text_faint));
            if button(app, ui, "generate.use_ai", "Use AI prompt") {
                d.prompt = text;
            }
        }
    });
    if kind == "video" {
        ui.horizontal(|ui| {
            if button(app, ui, "generate.h3", "H3 prompt with AI") {
                let brief = if d.prompt.trim().is_empty() { "the reference video with its reference picture".to_string() } else { d.prompt.clone() };
                let ctx = ui.ctx().clone();
                if let Err(e) = crate::menus::invoke(app, &ctx, "nl.prompt.h3", json!({"brief": brief, "mode": "ref"})) {
                    app.ui.status = e;
                }
            }
            if let Some(suggestion) = app.session.nl.result("h3prompt").cloned()
                && let Some(text) = suggestion["prompt"].as_str().map(str::to_string)
            {
                ui.label(RichText::new("H3 prompt ready").size(11.0).color(app.tokens.text_faint));
                if button(app, ui, "generate.use_h3", "Use H3 prompt") {
                    d.prompt = text;
                }
            }
            let selected = app.session.state.selection.first().map(|clip| clip.0);
            if button(app, ui, "generate.extend", "Extend selected clip (v2v)") {
                if let Some(clip) = selected {
                    if let Err(e) = crate::menus::invoke(app, &ui.ctx().clone(), "nl.generate.extend", json!({"clip": clip, "wait": false})) {
                        app.ui.status = e;
                    }
                } else {
                    app.ui.status = "Select a video clip on the timeline to extend it.".into();
                }
            }
        });
    }
    if let Some((path, _)) = options.get(d.workflow).cloned() {
        reference_section(app, ui, &path, &mut d);
    }
    ui.add_space(4.0);
    ui.label(RichText::new("Timeline analysis (local vision)").size(12.0).color(app.tokens.text_dim));
    ui.horizontal(|ui| {
        if button(app, ui, "generate.analyze_timeline", "Analyze timeline with vision") {
            let ctx = ui.ctx().clone();
            match crate::menus::invoke(app, &ctx, "nl.timeline.analyze", json!({"everySeconds": 5.0, "maxFrames": 4, "includeAudio": false})) {
                Ok(value) => {
                    ui.data_mut(|data| data.insert_temp(egui::Id::unique("nl-timeline-analysis"), value));
                }
                Err(e) => app.ui.status = e,
            }
        }
        if let Some(analysis) = app.session.nl.result("analysis").cloned()
            && let Some(context) = analysis["context"].as_str().map(str::to_string)
        {
            ui.label(RichText::new(format!("{} frame(s) analyzed", analysis["frames"].as_array().map_or(0, Vec::len))).size(11.0).color(app.tokens.text_faint));
            if button(app, ui, "generate.use_analysis", "Use analysis in prompt") {
                d.prompt = context;
            }
        }
    });
    if let Some(analysis) = ui.data(|data| data.get_temp::<Value>(egui::Id::unique("nl-timeline-analysis"))) {
        egui::ScrollArea::vertical().id_salt("nl-timeline-analysis-list").max_height(120.0).show(ui, |ui| {
            for frame in analysis["frames"].as_array().into_iter().flatten() {
                ui.label(
                    RichText::new(format!("{:.0}s · {}", frame["timeSeconds"].as_f64().unwrap_or(0.0), frame["description"].as_str().unwrap_or("")))
                        .size(10.0)
                        .color(app.tokens.text_dim),
                );
            }
        });
    }
    let mut negative = d.negative.clone();
    field(app, ui, "generate.negative", "Negative prompt (optional)", &mut negative);
    d.negative = negative;
    let mut seed = d.seed.to_string();
    field(app, ui, "generate.seed", "Seed (0 = random)", &mut seed);
    d.seed = seed.parse().unwrap_or(0);
    let mut overrides = d.overrides.clone();
    field(app, ui, "generate.overrides", "Widget overrides JSON (from Show parameters)", &mut overrides);
    d.overrides = overrides;
    ui.horizontal(|ui| {
        if button(app, ui, "generate.use_workflow_overrides", "Use workflow overrides") {
            if let Some(value) = ui.data(|data| data.get_temp::<Value>(egui::Id::unique("nl-workflow-overrides"))) {
                d.overrides = serde_json::to_string_pretty(&value).unwrap_or_else(|_| "[]".into());
                app.ui.status = "Override dal pannello NL Workflows applicati alla generazione.".into();
            } else {
                app.ui.status = "Nessun override: NL Workflows → Show parameters → modifica → «Usa per la prossima generazione».".into();
            }
        }
        ui.label(RichText::new("(NL Workflows → Show parameters → Avanzate)").size(10.0).color(app.tokens.text_faint));
    });
    let scale = if kind == "video" {
        // Video output is one third of the sequence, preserving its aspect ratio.
        let options = [("1/3", "One third of the sequence")];
        let mut i = 0;
        ui.horizontal(|ui| {
            for (j, (_, label)) in options.iter().enumerate() {
                ui.selectable_value(&mut i, j, *label);
            }
        });
        d.scale = i;
        options[i].0
    } else {
        let options = [("1x", "Same as the sequence"), ("2x", "Double")];
        let mut i = if app.session.prefs.nl.image_scale == "2x" { 1 } else { 0 };
        ui.horizontal(|ui| {
            for (j, (_, label)) in options.iter().enumerate() {
                ui.selectable_value(&mut i, j, *label);
            }
        });
        d.scale = i;
        options[i].0
    };
    size_preview(app, ui, kind, scale);
    ui.add_space(6.0);
    if let Some(dry) = ui.data(|data| data.get_temp::<Value>(egui::Id::unique("nl-generate-dry"))) {
        let size = dry["size"]
            .as_object()
            .map(|size| format!("{}×{}", size.get("width").and_then(Value::as_u64).unwrap_or(0), size.get("height").and_then(Value::as_u64).unwrap_or(0)));
        let refs = dry["references"].as_array().map_or(0, Vec::len);
        ui.label(
            RichText::new(format!(
                "Dry run: {} node(s), {refs} reference group(s), size {} — nothing sent to ComfyUI.",
                dry["nodes"].as_u64().unwrap_or(0),
                size.unwrap_or_else(|| "explicit".into())
            ))
            .size(11.0)
            .color(app.tokens.text_dim),
        );
    }
    job_line(app, ui, "ComfyUI:");
    job_line(app, ui, "NL prompt");
    save_draft(ui, "nl-generate", d);
}

fn music(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(app, ui, "NL Music", "Music with the local ComfyUI (Ace Step or whatever workflow you installed). No model is downloaded.");
    error_line(app, ui);
    #[derive(Clone, Default)]
    struct MusicDraft {
        caption: String,
        lyrics: String,
        seconds: u32,
        workflow: usize,
    }
    let mut d: MusicDraft = draft(ui, "nl-music");
    let options = workflow_options(app, ui, "audio");
    if d.workflow >= options.len() {
        d.workflow = default_workflow(&options, app, "audio");
    }
    if d.seconds == 0 {
        d.seconds = 30;
    }
    combo(app, ui, "music.workflow", "Workflow", &mut d.workflow, &options);
    let selected_workflow = options.get(d.workflow).map(|(path, _)| path.clone()).unwrap_or_default();
    delete_workflow_button(app, ui, "audio", &selected_workflow);
    let mut caption = d.caption.clone();
    ui.label(RichText::new("Style or subject").size(12.0).color(app.tokens.text_dim));
    let r = ui.add(egui::TextEdit::multiline(&mut caption).desired_rows(3).desired_width(f32::INFINITY).id_salt("nl-music-caption"));
    app.auto.add("nl.music.caption", r.rect, "Style or subject");
    d.caption = caption;
    let mut lyrics = d.lyrics.clone();
    ui.label(RichText::new("Lyrics (optional)").size(12.0).color(app.tokens.text_dim));
    let r = ui.add(egui::TextEdit::multiline(&mut lyrics).desired_rows(3).desired_width(f32::INFINITY).id_salt("nl-music-lyrics"));
    app.auto.add("nl.music.lyrics", r.rect, "Lyrics");
    d.lyrics = lyrics;
    let mut seconds = d.seconds.to_string();
    field(app, ui, "music.seconds", "Seconds", &mut seconds);
    d.seconds = seconds.parse().unwrap_or(30).clamp(5, 600);
    ui.add_space(6.0);
    if button(app, ui, "music.run", "Generate music") {
        let mut params = json!({"prompt": d.caption.trim(), "seconds": d.seconds, "kind": "audio"});
        if let Some((path, _)) = options.get(d.workflow) {
            params["workflow"] = json!(path);
        }
        if !d.lyrics.trim().is_empty() {
            params["lyrics"] = json!(d.lyrics.trim());
        }
        run(app, ui, "audio.generateMusic", params);
    }
    job_line(app, ui, "ComfyUI:");
    save_draft(ui, "nl-music", d);
}

fn asr(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(app, ui, "NL ASR", "Speech recognition manager: local models, provider/device, transcription with word timings, subtitle import and translation.");
    error_line(app, ui);
    if let Some(path) = app.session.nl.results.remove("asr.pick").and_then(|value| value.as_str().map(str::to_string)) {
        match app.session.execute("captions.import", json!({"path": path})) {
            Ok(value) => app.ui.status = format!("Sottotitoli importati da {path}: {}", value),
            Err(e) => app.ui.status = e.to_string(),
        }
    }
    #[derive(Clone, Default)]
    struct AsrDraft {
        language: usize,
        device: usize,
    }
    let mut d: AsrDraft = draft(ui, "nl-asr");
    let languages = ["it", "en", "es", "fr", "de", "pt", "nl", "auto"];
    let devices = ["auto", "cuda", "cpu"];
    if d.language == 0 && app.session.prefs.nl.asr_language == "auto" {
        d.language = 7;
    }
    if d.device == 0 && app.session.prefs.nl.asr_device() == "cpu" {
        d.device = 2;
    }
    let options_lang: Vec<(String, String)> = languages.iter().map(|l| (l.to_string(), l.to_uppercase())).collect();
    combo(app, ui, "asr.language", "Language", &mut d.language, &options_lang);
    let options_dev: Vec<(String, String)> = devices.iter().map(|v| (v.to_string(), v.to_string())).collect();
    combo(app, ui, "asr.device", "Device", &mut d.device, &options_dev);
    ui.label(
        RichText::new(format!(
            "Model: {}. Cache: {}. Bridge: {}.",
            if app.session.prefs.nl.asr_model.trim().is_empty() { "the portable one" } else { app.session.prefs.nl.asr_model.as_str() },
            if app.session.prefs.nl.asr_cache { "reused for unchanged media" } else { "per run" },
            if filmcraft_engine::nl_asr::available() { "found" } else { "not found in this suite folder" }
        ))
        .size(12.0)
        .color(app.tokens.text_dim),
    );
    ui.add_space(6.0);
    if button(app, ui, "asr.run", "Trascrivi la selezione/media") {
        run(app, ui, "transcript.transcribeNl", json!({"language": languages[d.language], "device": devices[d.device]}));
    }
    job_line(app, ui, "Transcribe NL");
    ui.add_space(8.0);
    ui.separator();
    asr_model_manager(app, ui, "asr.manager");
    ui.add_space(8.0);
    ui.separator();
    ui.label(RichText::new("Sottotitoli e traduzione").size(13.0).strong());
    ui.horizontal(|ui| {
        if button(app, ui, "asr.import", "Importa SRT/VTT…") {
            app.pick_ui(
                crate::native_dialogs::FileDialog::OpenFile {
                    filter: "Sottotitoli".into(),
                    exts: vec!["srt".into(), "vtt".into(), "scc".into(), "mcc".into()],
                },
                |app, paths| {
                    if let Some(path) = paths.first() {
                        app.session.nl.results.insert("asr.pick", json!(path));
                    }
                },
            );
        }
        if button(app, ui, "asr.translate_en", "Traduci in inglese") {
            run(app, ui, "nl.translate.captions", json!({"to": "en"}));
        }
        if button(app, ui, "asr.translate_es", "Traduci in spagnolo") {
            run(app, ui, "nl.translate.captions", json!({"to": "es"}));
        }
        if button(app, ui, "asr.transcript_panel", "Apri NL Transcript") {
            app.show_panel(PanelKind::NlTranscript);
        }
    });
    save_draft(ui, "nl-asr", d);
}

/// The local speech models the suite found, with the buttons that select one. Shared by the ASR
/// manager and the Speech panel so the same discovery result drives both.
fn asr_model_manager(app: &mut FilmcraftApp, ui: &mut egui::Ui, salt: &str) {
    let scan_id = egui::Id::unique(("nl-asr-scan", salt));
    ui.horizontal(|ui| {
        if button(app, ui, &format!("{salt}.scan"), "Scansiona modelli") {
            match app.session.execute("nl.asr.models", json!({})) {
                Ok(value) => {
                    ui.data_mut(|data| data.insert_temp(scan_id, value));
                }
                Err(e) => app.ui.status = e.to_string(),
            }
        }
        ui.label(RichText::new(format!("Provider: {}", app.session.prefs.nl.asr_provider())).size(11.0).color(app.tokens.text_dim));
    });
    let Some(scan) = ui.data(|data| data.get_temp::<Value>(scan_id)) else { return };
    let models = scan["models"].as_array().cloned().unwrap_or_default();
    let engines = scan["engines"].as_array().cloned().unwrap_or_default();
    ui.label(RichText::new(format!("{} verificati · {} engine", scan["count"].as_u64().unwrap_or(0), engines.len())).size(11.0).color(app.tokens.text_faint));
    let qwen_model = models.iter().find(|m| m["kind"] == "qwen-gguf" && m["role"] == "asr").cloned();
    let qwen_aligner = models.iter().find(|m| m["kind"] == "qwen-gguf" && m["role"] == "aligner").cloned();
    let qwen_runner = engines.iter().find(|e| e["engine"] == "qwen3-asr").cloned();
    if let (Some(model), Some(aligner), Some(runner)) = (qwen_model, qwen_aligner, qwen_runner)
        && button(app, ui, &format!("{salt}.qwen"), "Usa Qwen3-ASR")
    {
        for (key, value) in [
            ("nl.asrProvider", "qwen_gguf"),
            ("nl.asrRunner", runner["path"].as_str().unwrap_or("")),
            ("nl.asrQwenModel", model["path"].as_str().unwrap_or("")),
            ("nl.asrQwenAligner", aligner["path"].as_str().unwrap_or("")),
        ] {
            if let Err(e) = app.session.execute("prefs.set", json!({"key": key, "value": value})) {
                app.ui.status = e.to_string();
            }
        }
        app.ui.status = "Qwen3-ASR selezionato.".into();
    }
    if let Some(whisper) = models.iter().find(|m| m["kind"] == "whisper-ct2").cloned()
        && button(app, ui, &format!("{salt}.whisper"), "Usa Whisper")
    {
        let _ = app.session.execute("prefs.set", json!({"key": "nl.asrProvider", "value": "bridge"}));
        let _ = app.session.execute("prefs.set", json!({"key": "nl.asrModel", "value": whisper["path"].as_str().unwrap_or("")}));
        app.ui.status = "Whisper bridge selezionato.".into();
    }
    for model in models.iter().take(6) {
        ui.label(
            RichText::new(format!(
                "{} · {} · {}",
                model["kind"].as_str().unwrap_or(""),
                model["role"].as_str().unwrap_or(""),
                model["path"].as_str().unwrap_or("")
            ))
            .size(10.0)
            .color(app.tokens.text_faint),
        );
    }
}

/// Selection flags for a list, kept in the panel's own UI state.
fn selection(ui: &egui::Ui, id: &str, len: usize) -> Vec<bool> {
    let mut flags: Vec<bool> = ui.data(|d| d.get_temp::<Vec<bool>>(egui::Id::unique(id))).unwrap_or_default();
    flags.resize(len, true);
    if flags.len() > len {
        flags.truncate(len);
    }
    flags
}

fn save_selection(ui: &egui::Ui, id: &str, flags: &[bool]) {
    ui.data_mut(|d| d.insert_temp(egui::Id::unique(id), flags.to_vec()));
}

fn time_range(start: f64, end: f64) -> String {
    let stamp = |s: f64| {
        let s = s.max(0.0);
        format!("{:02}:{:04.1}", (s / 60.0) as u64, s % 60.0)
    };
    format!("{} – {}", stamp(start), stamp(end))
}

/// The timeline range of one clip of the active sequence, for the direct reframe action.
fn clip_range(session: &filmcraft_engine::Session, clip: u64) -> Option<(f64, f64)> {
    let sequence = session.active_sequence()?;
    for track in sequence.video_tracks.iter().chain(sequence.audio_tracks.iter()) {
        for item in &track.items {
            if item.id.0 == clip {
                return Some((item.start.seconds(), item.end().seconds()));
            }
        }
    }
    None
}

/// The dashboard groups. One list, so a new NL panel must be added to a group and the test below
/// fails if it is missing from the dashboard.
fn home_groups() -> [(&'static str, &'static [PanelKind]); 4] {
    [
        ("Creazione", &[PanelKind::NlGenerate, PanelKind::NlMusic, PanelKind::NlBridge, PanelKind::NlStory, PanelKind::NlCover, PanelKind::NlQueue]),
        (
            "Agente e timeline",
            &[
                PanelKind::NlAgent,
                PanelKind::NlMontage,
                PanelKind::NlSgrossa,
                PanelKind::NlClipping,
                PanelKind::NlLongform,
                PanelKind::NlTranscript,
                PanelKind::NlAnalyze,
                PanelKind::NlSearch,
                PanelKind::NlShots,
                PanelKind::NlVersions,
                PanelKind::NlAlbum,
            ],
        ),
        (
            "Finitura",
            &[
                PanelKind::NlStudio,
                PanelKind::NlSubtitles,
                PanelKind::NlTitles,
                PanelKind::NlGraphics,
                PanelKind::NlAudio,
                PanelKind::NlDelivery,
                PanelKind::NlSocial,
            ],
        ),
        ("Voce e configurazione", &[PanelKind::NlAsr, PanelKind::NlSpeech, PanelKind::NlServices, PanelKind::NlSettings, PanelKind::NlWorkflows]),
    ]
}

/// NL Home: the dashboard with every NL function one click away. It keeps the default workspace
/// readable instead of squeezing twenty tabs into one strip.
fn home(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Video Suite",
        "Tutte le funzioni NL in un unico posto. Clicca un pulsante per aprire il pannello corrispondente; le impostazioni AI sono in NL Settings.",
    );
    error_line(app, ui);
    if button(app, ui, "home.nl_layout", "Apri layout NL completo") {
        app.show_nl_layout();
    }
    // A compact state of the session the dashboard acts on: no AI call, no job, just what is open.
    if let Some(sequence) = app.session.active_sequence() {
        let settings = &sequence.settings;
        let fps = settings.frame_rate.num as f64 / settings.frame_rate.den.max(1) as f64;
        let cues: usize = sequence.caption_tracks.iter().map(|track| track.captions.len()).sum();
        let words = filmcraft_engine::transcript::sequence_words(&app.session).len();
        let scenes = app.session.nl.story.as_ref().and_then(|story| story.get("scenes")).and_then(Value::as_array).map_or(0, Vec::len);
        let running = app.session.jobs.iter().filter(|job| !job.progress.finished.load(std::sync::atomic::Ordering::Relaxed)).count();
        ui.label(
            RichText::new(format!(
                "{}×{} @ {fps:.2} fps · {words} parole · {cues} cue · {scenes} scene storia · {running} job attivi",
                settings.width, settings.height
            ))
            .size(11.0)
            .color(app.tokens.text_dim),
        );
    }
    for (group, panels) in home_groups() {
        ui.add_space(6.0);
        ui.label(RichText::new(group).size(12.0).strong());
        ui.horizontal_wrapped(|ui| {
            for panel in panels {
                let id = format!("nlhome.{}", panel.id());
                if button(app, ui, &id, panel.title()) {
                    app.show_panel(*panel);
                }
            }
        });
    }
    ui.add_space(10.0);
    ui.separator();
    ui.label(
        RichText::new("Suggerimento: NL Clips AI accetta un video scelto dal disco o la selezione corrente; NL Generate usa ComfyUI locale.")
            .size(11.0)
            .color(app.tokens.text_dim),
    );
}

/// NL Agent: from a subject to a timeline of generation objects, with prompts written by the
/// local model and generated media replacing the placeholders in place.
fn agent(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    let height = ui.available_height().clamp(160.0, 520.0);
    egui::ScrollArea::vertical().id_salt("nl-agent-scroll").max_height(height).auto_shrink([false, false]).show(ui, |ui| agent_body(app, ui));
}

fn agent_body(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Agent",
        "Assistente di produzione: da una richiesta a una storia, una serie o oggetti; l'agente scrive i prompt, sceglie il workflow per la qualità richiesta, crea i placeholder e li sostituisce con i media generati.",
    );
    error_line(app, ui);
    #[derive(Clone, Default)]
    struct AgentDraft {
        subject: String,
        style: String,
        scenes: String,
        message: String,
        mode: usize,
        media: usize,
        quality: usize,
        sheet_name: String,
        sheet_description: String,
        squad_story: bool,
        squad_prompts: bool,
        squad_assets: bool,
        squad_video: bool,
    }
    let mut d: AgentDraft = draft(ui, "nl-agent");
    let modes = [("story", "Storia"), ("series", "Serie"), ("objects", "Oggetti")];
    let medias = [("image", "Immagini"), ("video", "Video")];
    let qualities = [("bozza", "Bozza"), ("standard", "Standard"), ("alta", "Alta"), ("massima", "Massima")];
    let mode_options: Vec<(String, String)> = modes.iter().map(|(v, l)| (v.to_string(), l.to_string())).collect();
    let media_options: Vec<(String, String)> = medias.iter().map(|(v, l)| (v.to_string(), l.to_string())).collect();
    let quality_options: Vec<(String, String)> = qualities.iter().map(|(v, l)| (v.to_string(), l.to_string())).collect();
    combo(app, ui, "agent.mode", "Modalità", &mut d.mode, &mode_options);
    ui.horizontal_wrapped(|ui| {
        combo(app, ui, "agent.media", "Media", &mut d.media, &media_options);
        combo(app, ui, "agent.quality", "Qualità", &mut d.quality, &quality_options);
    });
    let quality = qualities[d.quality.min(qualities.len() - 1)].0;
    let media = medias[d.media.min(medias.len() - 1)].0;
    let mode = modes[d.mode.min(modes.len() - 1)].0;
    ui.add_space(4.0);
    ui.label(RichText::new("Chiedi all'agente").size(12.0).strong());
    let response = ui.add(egui::TextEdit::multiline(&mut d.message).desired_rows(3).desired_width(f32::INFINITY).id_salt("nl-agent-message"));
    app.auto.add("nl.agent.message", response.rect, "Messaggio all'agente");
    ui.horizontal_wrapped(|ui| {
        if button(app, ui, "agent.chat", "Invia e crea piano") && !d.message.trim().is_empty() {
            let scenes = d.scenes.trim().parse::<u64>().unwrap_or(6);
            run(
                app,
                ui,
                "nl.agent.chat",
                json!({
                    "message": d.message.trim(),
                    "mode": mode,
                    "media": media,
                    "quality": quality,
                    "scenes": scenes,
                    "place": true,
                    "startSeconds": app.session.playhead().seconds(),
                }),
            );
        }
        if button(app, ui, "agent.workflows", "Workflow consigliati") {
            run(app, ui, "nl.agent.workflows", json!({"kind": media, "quality": quality}));
        }
        if button(app, ui, "agent.place", "Metti il piano in timeline") {
            run(app, ui, "nl.agent.place", json!({"startSeconds": app.session.playhead().seconds()}));
        }
    });
    if let Some(reply) = app.session.nl.result("agent.reply").cloned() {
        let text = reply["reply"].as_str().unwrap_or("");
        if !text.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new(text).size(11.0).color(app.tokens.text_dim));
        }
    }
    if let Some(recommendation) = app.session.nl.result("agent.workflows").cloned()
        && let Some(chosen) = recommendation.get("chosen")
    {
        ui.label(
            RichText::new(format!(
                "Workflow {} · {} · {}",
                chosen["name"].as_str().unwrap_or(""),
                chosen["quality"].as_str().unwrap_or(""),
                chosen["reason"].as_str().unwrap_or("")
            ))
            .size(10.0)
            .color(app.tokens.text_faint),
        );
    }
    ui.collapsing("Squadra di produzione", |ui| {
        ui.label(RichText::new("Ruoli attivi in questo giro (l'agente attiva solo quelli scelti, in sequenza).").size(10.0).color(app.tokens.text_faint));
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut d.squad_story, "Storia");
            ui.checkbox(&mut d.squad_prompts, "Prompt");
            ui.checkbox(&mut d.squad_assets, "Asset immagini");
            ui.checkbox(&mut d.squad_video, "Video i2v");
        });
        let mut roles = Vec::new();
        if d.squad_story {
            roles.push("story");
        }
        if d.squad_prompts {
            roles.push("prompts");
        }
        if d.squad_assets {
            roles.push("assets");
        }
        if d.squad_video {
            roles.push("video");
        }
        let brief = if !d.message.trim().is_empty() { d.message.clone() } else { d.subject.clone() };
        ui.horizontal_wrapped(|ui| {
            if button(app, ui, "agent.squad_plan", "Pianifica squadra") && !brief.trim().is_empty() {
                let scenes = d.scenes.trim().parse::<u64>().unwrap_or(6);
                run(
                    app,
                    ui,
                    "nl.agent.squad",
                    json!({
                        "action": "plan",
                        "roles": roles,
                        "brief": brief.trim(),
                        "mode": mode,
                        "media": media,
                        "quality": quality,
                        "scenes": scenes,
                    }),
                );
            }
            if button(app, ui, "agent.squad_run", "Esegui coda ComfyUI") {
                run(app, ui, "nl.agent.squad", json!({"action": "run", "roles": roles}));
            }
        });
        if let Some(plan) = app.session.nl.result("agent.plan").cloned()
            && let Some(stages) = plan["stages"].as_array()
        {
            for stage in stages {
                ui.label(
                    RichText::new(format!(
                        "{} · {} · {}",
                        stage["role"].as_str().unwrap_or(""),
                        stage["status"].as_str().unwrap_or(""),
                        stage["note"].as_str().or_else(|| stage["title"].as_str()).unwrap_or("")
                    ))
                    .size(10.0)
                    .color(app.tokens.text_faint),
                );
            }
        }
        ui.label(
            RichText::new("La pianificazione usa il LLM; la coda ComfyUI parte solo quando premi Esegui, così locale LLM e ComfyUI non competono.")
                .size(10.0)
                .color(app.tokens.text_faint),
        );
    });
    ui.collapsing("Character sheet in Footage_AI", |ui| {
        field(app, ui, "agent.sheet_name", "Nome personaggio", &mut d.sheet_name);
        field(app, ui, "agent.sheet_description", "Descrizione fisica / costume", &mut d.sheet_description);
        if button(app, ui, "agent.sheet_create", "Genera character sheet") {
            let description = if d.sheet_description.trim().is_empty() { d.message.clone() } else { d.sheet_description.clone() };
            run(app, ui, "nl.agent.characterSheet", json!({"name": d.sheet_name.trim(), "description": description.trim(), "quality": quality}));
        }
        if let Some(sheet) = app.session.nl.result("agent.characterSheet").cloned() {
            ui.label(
                RichText::new(format!("In Footage_AI: item {} · {}", sheet["item"].as_u64().unwrap_or(0), sheet["asset"].as_str().unwrap_or("")))
                    .size(10.0)
                    .color(app.tokens.text_faint),
            );
        }
    });
    ui.add_space(6.0);
    ui.separator();
    ui.label(RichText::new("Sketch rapida di oggetti").size(12.0).strong());
    field(app, ui, "agent.subject", "Soggetto / brief", &mut d.subject);
    field(app, ui, "agent.style", "Stile (opzionale)", &mut d.style);
    field(app, ui, "agent.scenes", "Numero di oggetti", &mut d.scenes);
    ui.horizontal_wrapped(|ui| {
        if button(app, ui, "agent.sketch", "Crea timeline bozza") {
            let scenes = d.scenes.trim().parse::<u64>().unwrap_or(6);
            run(
                app,
                ui,
                "nl.agent.sketch",
                json!({
                    "subject": d.subject.trim(),
                    "style": d.style.trim(),
                    "scenes": scenes,
                    "quality": quality,
                    "mode": "objects",
                    "startSeconds": app.session.playhead().seconds(),
                }),
            );
        }
        if button(app, ui, "agent.add", "Aggiungi oggetto qui") {
            run(app, ui, "nl.agent.add", json!({"seconds": 4, "quality": quality, "startSeconds": app.session.playhead().seconds()}));
        }
        if button(app, ui, "agent.add_audio", "Aggiungi audio placeholder") {
            run(app, ui, "nl.agent.add", json!({"seconds": 10, "kind": "audio", "startSeconds": app.session.playhead().seconds()}));
        }
        if button(app, ui, "agent.prompts", "Compila prompt con AI") {
            run(app, ui, "nl.agent.prompts", json!({}));
        }
    });
    ui.horizontal_wrapped(|ui| {
        if button(app, ui, "agent.generate", "Genera e sostituisci") {
            run(app, ui, "nl.agent.generate", json!({"all": true}));
        }
        if button(app, ui, "agent.collect", "Raccogli asset nel progetto") {
            run(app, ui, "nl.agent.collect", json!({}));
        }
        if button(app, ui, "agent.home", "Apri NL Home") {
            app.show_panel(PanelKind::NlHome);
        }
    });
    job_line(app, ui, "ComfyUI");
    ui.add_space(6.0);
    ui.separator();
    let plan = app.session.execute("nl.agent.plan", json!({})).ok().and_then(|value| value.get("plan").cloned());
    if let Some(plan) = plan {
        let entries = plan["entries"].as_array().cloned().unwrap_or_default();
        let generated = entries.iter().filter(|entry| entry["status"] == "generated").count();
        ui.label(
            RichText::new(format!(
                "{} oggetti · {generated} generati · {} · qualità {}",
                entries.len(),
                plan["subject"].as_str().unwrap_or("—"),
                plan["quality"].as_str().unwrap_or("standard")
            ))
            .size(11.0)
            .color(app.tokens.text_dim),
        );
        if let Some(error) = plan["llmError"].as_str() {
            ui.label(RichText::new(format!("Agente: {error}")).size(10.0).color(egui::Color32::from_rgb(233, 180, 120)));
        }
        egui::ScrollArea::vertical().id_salt("nl-agent-plan").max_height(300.0).show(ui, |ui| {
            for entry in entries {
                let title = entry["title"].as_str().unwrap_or("Oggetto");
                let status = entry["status"].as_str().unwrap_or("draft");
                let seconds = entry["seconds"].as_f64().unwrap_or(4.0);
                let workflow = entry["workflow"].as_str().and_then(|path| path.rsplit('/').next()).unwrap_or("workflow da scegliere");
                ui.label(RichText::new(format!("{title} · {status} · {seconds:.0}s")).size(11.0).strong());
                ui.label(RichText::new(format!("{workflow} · {}", entry["workflowQuality"].as_str().unwrap_or(""))).size(9.5).color(app.tokens.text_faint));
                let prompt = entry["prompt"].as_str().unwrap_or("");
                let id = entry["id"].as_u64().unwrap_or(0);
                let mut edited = prompt.to_string();
                let response = ui.add(egui::TextEdit::multiline(&mut edited).desired_rows(2).desired_width(f32::INFINITY).id_salt(("nl-agent-prompt", id)));
                app.auto.add(&format!("nl.agent.prompt.edit.{id}"), response.rect, title);
                let saved = ui.add_enabled(!edited.trim().is_empty(), egui::Button::new("Salva prompt"));
                app.auto.add(&format!("nl.agent.prompt.save.{id}"), saved.rect, "Salva prompt");
                if saved.clicked() && edited.trim() != prompt.trim() {
                    run(app, ui, "nl.agent.prompt", json!({"id": id, "prompt": edited.trim()}));
                }
                if entry.get("asset").is_some() && button(app, ui, &format!("agent.animate.{id}"), "Anima in video (i2v)") {
                    run(app, ui, "nl.agent.animate", json!({"id": id, "seconds": seconds.round() as u64, "quality": quality}));
                }
            }
        });
    } else {
        ui.label(RichText::new("Nessun piano: scrivi una richiesta all'assistente e premi «Invia e crea piano».").size(11.0).color(app.tokens.text_faint));
    }
    save_draft(ui, "nl-agent", d);
}

/// NL Longform: chapters over a transcription.
fn longform(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(app, ui, "NL Longform", "Capitoli e struttura del parlato: analisi con il modello locale, marker e export.");
    error_line(app, ui);
    #[derive(Clone, Default)]
    struct LongformDraft {
        path: String,
    }
    let mut d: LongformDraft = draft(ui, "nl-longform");
    field(app, ui, "longform.path", "Export file (.md, .srt, .json)", &mut d.path);
    ui.horizontal(|ui| {
        if button(app, ui, "longform.analyze", "Analizza capitoli") {
            run(app, ui, "nl.longform.analyze", json!({"wait": false}));
        }
        if button(app, ui, "longform.markers", "Metti i marker") {
            run(app, ui, "nl.longform.markers", json!({}));
        }
        if button(app, ui, "longform.export", "Esporta") && !d.path.trim().is_empty() {
            run(app, ui, "nl.longform.export", json!({"path": d.path.trim()}));
        }
    });
    if let Some(longform) = app.session.nl.longform.clone() {
        let chapters = longform["chapters"].as_array().cloned().unwrap_or_default();
        ui.add_space(6.0);
        ui.label(RichText::new(format!("{} capitoli", chapters.len())).size(12.0));
        egui::ScrollArea::vertical().id_salt("nl-longform-list").max_height(220.0).show(ui, |ui| {
            for chapter in chapters {
                let start = chapter["start"].as_f64().unwrap_or(0.0);
                ui.label(RichText::new(format!("{:.1}s · {}", start, chapter["title"].as_str().unwrap_or("Capitolo"))).size(11.0).color(app.tokens.text_dim));
            }
        });
    } else {
        ui.label(RichText::new("Nessuna analisi ancora: serve una trascrizione nel progetto.").size(11.0).color(app.tokens.text_faint));
    }
    ui.add_space(8.0);
    ui.separator();
    ui.label(RichText::new("Rough cut e silenzi").size(12.0).strong());
    ui.horizontal(|ui| {
        if button(app, ui, "longform.roughcut", "Analizza rough cut (silenzi)") {
            run(app, ui, "nl.sgrossa.analyze", json!({"wait": false}));
        }
        if button(app, ui, "longform.sgrossa", "Apri NL Sgrossa") {
            app.show_panel(PanelKind::NlSgrossa);
        }
    });
    if let Some(plan) = app.session.nl.result("cleanup").cloned() {
        let cuts = plan["cuts"].as_array().map_or(0, Vec::len);
        let good = plan["good"].as_array().map_or(0, Vec::len);
        ui.label(RichText::new(format!("{cuts} tagli proposti · {good} pezzi buoni")).size(11.0).color(app.tokens.text_dim));
        for piece in plan["good"].as_array().into_iter().flatten().take(6) {
            let start = piece["start"].as_f64().unwrap_or(0.0);
            let end = piece["end"].as_f64().unwrap_or(start);
            ui.label(
                RichText::new(format!("{} · {}", time_range(start, end), piece["reason"].as_str().unwrap_or("keep"))).size(10.0).color(app.tokens.text_faint),
            );
        }
    } else {
        ui.label(RichText::new("Nessun piano di rough cut: seleziona un video e avvia l'analisi.").size(10.0).color(app.tokens.text_faint));
    }
    save_draft(ui, "nl-longform", d);
}

/// NL Transcript: the sequence transcript, ASR, SRT/VTT import, captions, translation and chapters
/// in one panel. It is the transcription half of the old longform/transcript workspace.
fn transcript(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Transcript",
        "Trascrizione della sequenza attiva: ASR locale, import SRT/VTT, creazione sottotitoli, traduzione e capitoli. Nessun download automatico.",
    );
    error_line(app, ui);
    #[derive(Clone, Default)]
    struct TranscriptDraft {
        path: String,
        to: usize,
    }
    let mut d: TranscriptDraft = draft(ui, "nl-transcript");
    if let Some(path) = app.session.nl.results.remove("transcript.pick").and_then(|value| value.as_str().map(str::to_string)) {
        d.path = path;
        app.ui.status = format!("File scelto: {}", d.path);
    }
    let words = filmcraft_engine::transcript::sequence_words(&app.session).len();
    let cues = app.session.active_sequence().map_or(0, |sequence| sequence.caption_tracks.iter().map(|track| track.captions.len()).sum());
    let selection = app.session.state.selection.len();
    ui.label(RichText::new(format!("{words} parole · {cues} cue nella sequenza attiva · {selection} clip selezionati")).size(11.0).color(app.tokens.text_dim));
    ui.horizontal(|ui| {
        if button(app, ui, "transcript.run", "Trascrivi selezione/media") {
            run(app, ui, "transcript.transcribeNl", json!({}));
        }
        if button(app, ui, "transcript.captions", "Crea sottotitoli dal testo") {
            run(app, ui, "transcript.createCaptions", json!({}));
        }
        if button(app, ui, "transcript.text_panel", "Apri pannello Testo") {
            app.show_panel(PanelKind::Text);
            app.ui.text_tab = "Transcript".into();
        }
    });
    job_line(app, ui, "Transcribe NL");
    ui.add_space(6.0);
    ui.separator();
    ui.label(RichText::new("Importa SRT/VTT nella sequenza attiva").size(12.0).strong());
    field(app, ui, "transcript.path", "File SRT/VTT", &mut d.path);
    ui.horizontal(|ui| {
        if button(app, ui, "transcript.pick", "Scegli file…") {
            app.pick_ui(
                crate::native_dialogs::FileDialog::OpenFile {
                    filter: "Sottotitoli".into(),
                    exts: vec!["srt".into(), "vtt".into(), "scc".into(), "mcc".into()],
                },
                |app, paths| {
                    if let Some(path) = paths.first() {
                        app.session.nl.results.insert("transcript.pick", json!(path));
                    }
                },
            );
        }
        if button(app, ui, "transcript.import", "Importa") && !d.path.trim().is_empty() {
            run(app, ui, "captions.import", json!({"path": d.path.trim()}));
        }
    });
    ui.add_space(6.0);
    ui.separator();
    ui.label(RichText::new("Traduzione e capitoli").size(12.0).strong());
    let options = vec![("en".to_string(), "Inglese".to_string()), ("es".to_string(), "Spagnolo".to_string())];
    combo(app, ui, "transcript.to", "Lingua di destinazione", &mut d.to, &options);
    ui.horizontal(|ui| {
        if button(app, ui, "transcript.translate", "Traduci sottotitoli") {
            run(app, ui, "nl.translate.captions", json!({"to": options[d.to.min(options.len() - 1)].0}));
        }
        if button(app, ui, "transcript.longform", "Analizza capitoli") {
            run(app, ui, "nl.longform.analyze", json!({"wait": false}));
        }
    });
    save_draft(ui, "nl-transcript", d);
}

/// NL Search: index and search the project's clips, markers and story.
fn search(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(app, ui, "NL Search", "Cerca in clip, marker, scene e storia del progetto.");
    error_line(app, ui);
    #[derive(Clone, Default)]
    struct SearchDraft {
        query: String,
        limit: String,
    }
    let mut d: SearchDraft = draft(ui, "nl-search");
    field(app, ui, "search.query", "Cerca", &mut d.query);
    field(app, ui, "search.limit", "Risultati max", &mut d.limit);
    ui.horizontal(|ui| {
        if button(app, ui, "search.index", "Ricostruisci indice") {
            match app.session.execute("nl.search.index", json!({})) {
                Ok(value) => app.ui.status = format!("Indice: {} documenti.", value["documents"].as_u64().or_else(|| value["count"].as_u64()).unwrap_or(0)),
                Err(e) => app.ui.status = e.to_string(),
            }
        }
        if button(app, ui, "search.query", "Cerca") {
            let limit = d.limit.trim().parse::<u64>().unwrap_or(20);
            match app.session.execute("nl.search.query", json!({"query": d.query.trim(), "limit": limit})) {
                Ok(value) => {
                    ui.data_mut(|data| data.insert_temp(egui::Id::unique("nl-search-result"), value));
                }
                Err(e) => app.ui.status = e.to_string(),
            }
        }
    });
    if let Some(result) = ui.data(|data| data.get_temp::<Value>(egui::Id::unique("nl-search-result"))) {
        let hits = result["hits"].as_array().cloned().unwrap_or_default();
        ui.add_space(6.0);
        ui.label(RichText::new(format!("{} risultati", hits.len())).size(12.0));
        egui::ScrollArea::vertical().id_salt("nl-search-hits").max_height(240.0).show(ui, |ui| {
            for hit in hits {
                ui.label(
                    RichText::new(format!(
                        "[{}] {} — {}",
                        hit["source"].as_str().unwrap_or(""),
                        hit["title"].as_str().unwrap_or(""),
                        hit["text"].as_str().unwrap_or("")
                    ))
                    .size(11.0)
                    .color(app.tokens.text_dim),
                );
            }
        });
    }
    save_draft(ui, "nl-search", d);
}

/// NL Speech: discover/select speech models and run a transcription on a file.
fn speech(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(app, ui, "NL Speech", "Modelli ASR locali (Whisper, Qwen3-ASR) e trascrizione di un file.");
    error_line(app, ui);
    #[derive(Clone, Default)]
    struct SpeechDraft {
        path: String,
    }
    let mut d: SpeechDraft = draft(ui, "nl-speech");
    let scan_id = egui::Id::unique("nl-speech-scan");
    ui.horizontal(|ui| {
        if button(app, ui, "speech.scan", "Scansiona modelli") {
            match app.session.execute("nl.asr.models", json!({})) {
                Ok(value) => {
                    ui.data_mut(|data| data.insert_temp(scan_id, value));
                }
                Err(e) => app.ui.status = e.to_string(),
            }
        }
        ui.label(RichText::new(format!("Provider: {}", app.session.prefs.nl.asr_provider())).size(11.0).color(app.tokens.text_dim));
    });
    if let Some(scan) = ui.data(|data| data.get_temp::<Value>(scan_id)) {
        let models = scan["models"].as_array().cloned().unwrap_or_default();
        let engines = scan["engines"].as_array().cloned().unwrap_or_default();
        let qwen_model = models.iter().find(|m| m["kind"] == "qwen-gguf" && m["role"] == "asr").cloned();
        let qwen_aligner = models.iter().find(|m| m["kind"] == "qwen-gguf" && m["role"] == "aligner").cloned();
        let qwen_runner = engines.iter().find(|e| e["engine"] == "qwen3-asr").cloned();
        ui.label(RichText::new(format!("{} verificati · {} engine", scan["count"].as_u64().unwrap_or(0), engines.len())).size(11.0));
        if let (Some(model), Some(aligner), Some(runner)) = (qwen_model, qwen_aligner, qwen_runner)
            && button(app, ui, "speech.use_qwen", "Usa Qwen3-ASR")
        {
            for (key, value) in [
                ("nl.asrProvider", "qwen_gguf"),
                ("nl.asrRunner", runner["path"].as_str().unwrap_or("")),
                ("nl.asrQwenModel", model["path"].as_str().unwrap_or("")),
                ("nl.asrQwenAligner", aligner["path"].as_str().unwrap_or("")),
            ] {
                if let Err(e) = app.session.execute("prefs.set", json!({"key": key, "value": value})) {
                    app.ui.status = e.to_string();
                }
            }
            app.ui.status = "Qwen3-ASR selezionato.".into();
        }
        if let Some(whisper) = models.iter().find(|m| m["kind"] == "whisper-ct2").cloned()
            && button(app, ui, "speech.use_whisper", "Usa Whisper")
        {
            if let Err(e) = app.session.execute("prefs.set", json!({"key": "nl.asrProvider", "value": "bridge"})) {
                app.ui.status = e.to_string();
            }
            if let Err(e) = app.session.execute("prefs.set", json!({"key": "nl.asrModel", "value": whisper["path"].as_str().unwrap_or("")})) {
                app.ui.status = e.to_string();
            }
            app.ui.status = "Whisper bridge selezionato.".into();
        }
        for model in models.iter().take(6) {
            ui.label(
                RichText::new(format!(
                    "{} · {} · {}",
                    model["kind"].as_str().unwrap_or(""),
                    model["role"].as_str().unwrap_or(""),
                    model["path"].as_str().unwrap_or("")
                ))
                .size(10.0)
                .color(app.tokens.text_faint),
            );
        }
    }
    field(app, ui, "speech.path", "File audio/video da trascrivere", &mut d.path);
    ui.horizontal(|ui| {
        if button(app, ui, "speech.pick", "Scegli file…") {
            app.pick_ui(
                crate::native_dialogs::FileDialog::OpenFile {
                    filter: "Audio or video".into(),
                    exts: vec!["wav".into(), "mp3".into(), "mp4".into(), "m4a".into(), "mov".into(), "mkv".into()],
                },
                |app, paths| {
                    if let Some(path) = paths.first() {
                        app.ui.status = format!("File scelto: {path}");
                    }
                },
            );
        }
        if button(app, ui, "speech.transcribe", "Trascrivi") {
            let mut params = json!({"backend": app.session.prefs.nl.asr_provider()});
            if !d.path.trim().is_empty() {
                params["path"] = json!(d.path.trim());
            }
            run(app, ui, "nl.asr.transcribe", params);
        }
    });
    save_draft(ui, "nl-speech", d);
}
/// NL Shot Map: scene cuts through the NLE's own scene detector, with markers.
fn shots(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Shot Map",
        "Rileva i cambi scena dei clip selezionati e mette un marker su ogni taglio. Usa il motore scene della timeline, non un servizio esterno.",
    );
    error_line(app, ui);
    #[derive(Clone)]
    struct ShotsDraft {
        sensitivity: String,
        min_frames: String,
        markers: bool,
    }
    impl Default for ShotsDraft {
        fn default() -> Self {
            Self { sensitivity: "50".into(), min_frames: "6".into(), markers: true }
        }
    }
    let mut d: ShotsDraft = draft(ui, "nl-shots");
    field(app, ui, "shots.sensitivity", "Sensibilità (0–100)", &mut d.sensitivity);
    field(app, ui, "shots.min_frames", "Durata minima inquadratura (frame)", &mut d.min_frames);
    crate::widgets::accessible_name(ui.checkbox(&mut d.markers, "Marker sui tagli"), "Marker sui tagli");
    ui.horizontal(|ui| {
        if button(app, ui, "shots.detect", "Rileva cambi scena") {
            let sensitivity = d.sensitivity.trim().parse::<f64>().unwrap_or(50.0);
            let min_frames = d.min_frames.trim().parse::<u64>().unwrap_or(6);
            run(app, ui, "clip.sceneEditDetection", json!({"sensitivity": sensitivity, "minShotFrames": min_frames, "applyCuts": false, "createSubclips": false, "generateMarkers": d.markers, "wait": false}));
        }
        if button(app, ui, "shots.markers_panel", "Apri Marker") { app.show_panel(PanelKind::Markers); }
        if button(app, ui, "shots.timeline", "Vai alla Timeline") { app.show_panel(PanelKind::Timeline); }
    });
    job_line(app, ui, "Scene Edit Detection");
    if let Some(sequence) = app.session.active_sequence() {
        let clips: usize = sequence.video_tracks.iter().map(|track| track.items.len()).sum();
        ui.label(RichText::new(format!("Sequenza attiva: {clips} clip video · {} marker", sequence.markers.len())).size(11.0).color(app.tokens.text_dim));
    }
    ui.label(
        RichText::new("I marker compaiono sul clip e nella timeline; i tagli non vengono applicati finché non li confermi con lo strumento nativo.")
            .size(10.0)
            .color(app.tokens.text_faint),
    );
    save_draft(ui, "nl-shots", d);
}

/// NL Social: the platform checks before publishing, with the profile the checks use.
fn social(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(app, ui, "NL Social", "Profilo di pubblicazione e controlli di consegna: formato, durata, sottotitoli e audio della sequenza attiva.");
    error_line(app, ui);
    let platforms = [
        ("youtube", "YouTube", "16:9 o 9:16", "Shorts fino a 60 s", "consigliati"),
        ("tiktok", "TikTok", "9:16 verticale", "15–600 s", "richiesti"),
        ("reels", "Instagram Reels", "9:16 verticale", "fino a 90 s", "consigliati"),
        ("instagram", "Instagram feed", "4:5 o 1:1", "fino a 60 s", "consigliati"),
    ];
    let mut platform = ui.data(|d| d.get_temp::<usize>(egui::Id::unique("nl-social-platform"))).unwrap_or(0).min(platforms.len() - 1);
    ui.horizontal(|ui| {
        craft_ui::ComboBox::from_id_salt("nl-social-platform-combo").selected_text(platforms[platform].1).show_ui(ui, |ui| {
            for (i, (_, label, _, _, _)) in platforms.iter().enumerate() {
                if ui.selectable_value(&mut platform, i, *label).changed() {
                    ui.data_mut(|d| d.insert_temp(egui::Id::unique("nl-social-platform"), platform));
                }
            }
        });
        if button(app, ui, "social.check", "Controlla la consegna") {
            match app.session.execute("nl.delivery.social", json!({"platform": platforms[platform].0})) {
                Ok(value) => {
                    ui.data_mut(|d| d.insert_temp(egui::Id::unique("nl-social-result"), value));
                }
                Err(e) => app.ui.status = e.to_string(),
            }
        }
    });
    let (_, label, aspect, duration, captions) = platforms[platform];
    ui.label(RichText::new(format!("{label}: {aspect} · {duration} · sottotitoli {captions}")).size(11.0).color(app.tokens.text_dim));
    if let Some(sequence) = app.session.active_sequence() {
        let settings = &sequence.settings;
        ui.label(
            RichText::new(format!(
                "Sequenza attiva: {}×{} · {:.1} s · {} cue",
                settings.width,
                settings.height,
                sequence.duration().seconds(),
                sequence.caption_tracks.iter().map(|track| track.captions.len()).sum::<usize>()
            ))
            .size(10.0)
            .color(app.tokens.text_faint),
        );
    }
    if let Some(result) = ui.data(|d| d.get_temp::<Value>(egui::Id::unique("nl-social-result"))) {
        ui.add_space(6.0);
        let checks = result["checks"].as_array().cloned().unwrap_or_default();
        let ready = result["ready"].as_bool().unwrap_or(false);
        ui.label(RichText::new(format!("{} controlli · {}", checks.len(), if ready { "pronto" } else { "da rivedere" })).size(12.0).color(if ready {
            app.tokens.text_dim
        } else {
            egui::Color32::from_rgb(233, 180, 120)
        }));
        for check in checks.iter().take(40) {
            let ok = check["ok"].as_bool().unwrap_or(false);
            ui.label(
                RichText::new(format!(
                    "{} {} — {}",
                    if ok { "OK" } else { "!" },
                    check["id"].as_str().unwrap_or("check"),
                    check["detail"].as_str().unwrap_or("")
                ))
                .size(11.0)
                .color(if ok { app.tokens.text_dim } else { egui::Color32::from_rgb(233, 180, 120) }),
            );
        }
    }
    ui.horizontal(|ui| {
        if button(app, ui, "social.open_clips", "Apri NL Clips AI") {
            app.show_panel(PanelKind::NlClipping);
        }
        if button(app, ui, "social.open_delivery", "Apri NL Delivery") {
            app.show_panel(PanelKind::NlDelivery);
        }
        if button(app, ui, "social.open_studio", "Apri NL Studio") {
            app.show_panel(PanelKind::NlStudio);
        }
    });
}

/// NL Studio: the finishing surfaces in one place, with the state of the sequence they finish.
fn studio(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Studio",
        "Motion graphics, titoli, sottotitoli, audio e consegna: le superfici di finitura con un clic e lo stato della sequenza attiva.",
    );
    error_line(app, ui);
    if let Some(sequence) = app.session.active_sequence() {
        let video: usize = sequence.video_tracks.iter().map(|track| track.items.len()).sum();
        let audio: usize = sequence.audio_tracks.iter().map(|track| track.items.len()).sum();
        let cues: usize = sequence.caption_tracks.iter().map(|track| track.captions.len()).sum();
        ui.label(
            RichText::new(format!("Sequenza attiva: {video} clip video · {audio} clip audio · {cues} cue · {:.1} s", sequence.duration().seconds()))
                .size(11.0)
                .color(app.tokens.text_dim),
        );
    }
    ui.add_space(6.0);
    for (label, panel) in [
        ("Titles & Graphics", PanelKind::NlTitles),
        ("Graphics", PanelKind::NlGraphics),
        ("Subtitles", PanelKind::NlSubtitles),
        ("Audio", PanelKind::NlAudio),
        ("Delivery", PanelKind::NlDelivery),
        ("Social", PanelKind::NlSocial),
        ("Versions", PanelKind::NlVersions),
        ("Album", PanelKind::NlAlbum),
    ] {
        if button(app, ui, &format!("studio.{}", panel.id()), label) {
            app.show_panel(panel);
        }
    }
    ui.add_space(6.0);
    ui.label(
        RichText::new(
            "Suggerimento: i sottotitoli automatici delle nuove sequenze Clips AI sono già stilizzati; qui puoi rifinirli, esportarli o consegnarli.",
        )
        .size(10.0)
        .color(app.tokens.text_faint),
    );
}

/// NL Services/Project: services, models, storage and project facts.
fn services(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Services",
        "Stato di ComfyUI/Ollama, modelli disponibili, spazio dati e informazioni del progetto aperto. Nessun servizio viene avviato.",
    );
    error_line(app, ui);
    let project = app.session.project.name.clone();
    let sequence = app
        .session
        .active_sequence()
        .map(|sequence| format!("{}×{} · {:.1} s", sequence.settings.width, sequence.settings.height, sequence.duration().seconds()))
        .unwrap_or_else(|| "nessuna sequenza".into());
    ui.label(RichText::new(format!("Progetto: {project} · Sequenza: {sequence}")).size(11.0).color(app.tokens.text_dim));
    ui.horizontal(|ui| {
        if button(app, ui, "services.status", "Stato servizi") {
            match app.session.execute("nl.services.status", json!({})) {
                Ok(value) => {
                    ui.data_mut(|d| d.insert_temp(egui::Id::unique("nl-services-status"), value));
                }
                Err(e) => app.ui.status = e.to_string(),
            }
        }
        if button(app, ui, "services.models", "Modelli LLM") {
            match app.session.execute("nl.models.llm", json!({})) {
                Ok(value) => {
                    ui.data_mut(|d| d.insert_temp(egui::Id::unique("nl-services-models"), value));
                }
                Err(e) => app.ui.status = e.to_string(),
            }
        }
        if button(app, ui, "services.storage", "Spazio") {
            match app.session.execute("nl.storage.status", json!({})) {
                Ok(value) => {
                    ui.data_mut(|d| d.insert_temp(egui::Id::unique("nl-services-storage"), value));
                }
                Err(e) => app.ui.status = e.to_string(),
            }
        }
        if button(app, ui, "services.project", "Progetto") {
            match app.session.execute("project.inspect", json!({})) {
                Ok(value) => {
                    ui.data_mut(|d| d.insert_temp(egui::Id::unique("nl-services-project"), value));
                }
                Err(e) => app.ui.status = e.to_string(),
            }
        }
        if button(app, ui, "services.open_settings", "Impostazioni NL") {
            app.show_panel(PanelKind::NlSettings);
        }
    });
    for (id, title_text) in
        [("nl-services-status", "Servizi"), ("nl-services-models", "Modelli"), ("nl-services-storage", "Spazio"), ("nl-services-project", "Progetto")]
    {
        let Some(value) = ui.data(|d| d.get_temp::<Value>(egui::Id::unique(id))) else { continue };
        ui.add_space(4.0);
        ui.label(RichText::new(title_text).size(12.0).strong());
        match id {
            "nl-services-status" => {
                let row = |ui: &mut egui::Ui, name: &str, value: &Value| {
                    let reachable = value["reachable"].as_bool().unwrap_or(false);
                    let detail = if reachable {
                        value["version"]
                            .as_str()
                            .map(str::to_string)
                            .or_else(|| value["system"]["comfyui_version"].as_str().map(str::to_string))
                            .unwrap_or_else(|| "attivo".into())
                    } else {
                        value["error"].as_str().unwrap_or("non raggiungibile").to_string()
                    };
                    ui.label(
                        RichText::new(format!("{name}: {} · {} · {}", if reachable { "OK" } else { "!" }, detail, value["url"].as_str().unwrap_or("")))
                            .size(10.5)
                            .color(if reachable { app.tokens.text_dim } else { egui::Color32::from_rgb(233, 180, 120) }),
                    );
                };
                if let Some(comfy) = value.get("comfy") {
                    row(ui, "ComfyUI", comfy);
                }
                if let Some(ollama) = value.get("ollama") {
                    row(ui, "Ollama", ollama);
                }
                if let Some(llm) = value.get("llm") {
                    ui.label(
                        RichText::new(format!(
                            "LLM: {} · {} · {} · {}",
                            llm["provider"].as_str().unwrap_or(""),
                            llm["model"].as_str().unwrap_or(""),
                            llm["url"].as_str().unwrap_or(""),
                            if llm["reachable"].as_bool().unwrap_or(false) { "OK" } else { "non raggiungibile" }
                        ))
                        .size(10.5)
                        .color(app.tokens.text_dim),
                    );
                }
            }
            "nl-services-models" => {
                let models = value["models"].as_array().cloned().unwrap_or_default();
                ui.label(
                    RichText::new(format!("{} modelli · provider {}", models.len(), value["provider"].as_str().unwrap_or("")))
                        .size(10.5)
                        .color(app.tokens.text_dim),
                );
                for model in models.iter().take(8) {
                    ui.label(RichText::new(format!("· {}", model.as_str().unwrap_or(""))).size(10.0).color(app.tokens.text_faint));
                }
            }
            "nl-services-storage" => {
                let text = serde_json::to_string(&value).unwrap_or_default();
                ui.label(RichText::new(text).size(10.0).color(app.tokens.text_dim));
            }
            "nl-services-project" => {
                let name = value["name"].as_str().or_else(|| value["project"]["name"].as_str()).unwrap_or("progetto");
                let sequence = value["activeSequence"].clone();
                ui.label(RichText::new(format!("{name} · sequenza attiva {sequence}")).size(10.5).color(app.tokens.text_dim));
            }
            _ => {}
        }
    }
}
/// NL Clipping: candidates from the bridge, selection, render and subtitles.
fn clipping(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Clips AI",
        "Clip editoriali da un video lungo: scegli un file dal disco o seleziona un video nel progetto/timeline, poi «Trova candidati». Render 16:9 e copia 9:16 con sottotitoli.",
    );
    error_line(app, ui);
    #[derive(Clone, Default)]
    struct ClipDraft {
        mode: usize,
        vertical: bool,
        hormozi: bool,
        source: String,
        item: Option<u64>,
        target: usize,
        focus: usize,
        tracking: usize,
        behavior: usize,
        stability: String,
        response: String,
        subtitles: bool,
    }
    let mut d: ClipDraft = draft(ui, "nl-clipping");
    // Source: the user can pick a file (imported into the project) or leave the project selection.
    let picked_id = egui::Id::unique("nl-clips-picked");
    // The file picker callback only remembers the path; importing here keeps the async callback
    // free of UI/session work.
    if let Some(path) = app.session.nl.result("clips.pick").and_then(Value::as_str).map(str::to_string) {
        app.session.nl.results.remove("clips.pick");
        match app.session.execute("mediaBrowser.import", json!({"paths": [path]})) {
            Ok(value) => {
                ui.data_mut(|data| data.insert_temp(picked_id, value));
                app.ui.status = format!("Video importato: {path}. Premi «Trova candidati».");
            }
            Err(e) => app.ui.status = e.to_string(),
        }
    }
    if let Some(picked) = ui.data(|data| data.get_temp::<Value>(picked_id)) {
        let id = picked["items"][0]["id"].as_u64().or_else(|| picked["items"][0]["item"].as_u64());
        if let Some(id) = id {
            d.item = Some(id);
            if let Some(path) = picked["items"][0]["path"].as_str() {
                d.source = path.to_string();
            }
        }
    }
    field(app, ui, "clipping.source", "Video sul disco (opzionale)", &mut d.source);
    ui.horizontal(|ui| {
        if button(app, ui, "clipping.pick", "Scegli video…") {
            app.pick_ui(
                crate::native_dialogs::FileDialog::OpenFile {
                    filter: "Video".into(),
                    exts: vec!["mp4".into(), "mov".into(), "mkv".into(), "avi".into(), "webm".into(), "m4v".into()],
                },
                |app, paths| {
                    if let Some(path) = paths.first() {
                        app.session.nl.results.insert("clips.pick", json!(path));
                    }
                },
            );
        }
        if button(app, ui, "clipping.use_selection", "Usa selezione corrente") {
            d.item = None;
            d.source.clear();
            ui.data_mut(|data| data.remove::<Value>(picked_id));
            app.ui.status = "Userò il video selezionato nel progetto o sulla timeline.".into();
        }
        if d.item.is_some() {
            ui.label(RichText::new("video importato pronto").size(10.0).color(app.tokens.text_faint));
        }
    });
    let modes = [("1", "Stories (≤25 s)"), ("2", "Reels (30–75 s)"), ("3", "Narrative reel (multi-segment)")];
    let mut mode_label = modes[d.mode.min(modes.len() - 1)].1.to_string();
    craft_ui::ComboBox::from_id_salt("nl-clipping-mode").selected_text(&mode_label).show_ui(ui, |ui| {
        for (i, (_, label)) in modes.iter().enumerate() {
            ui.selectable_value(&mut d.mode, i, *label);
        }
    });
    mode_label = modes[d.mode.min(modes.len() - 1)].1.to_string();
    let _ = &mode_label;
    ui.checkbox(&mut d.vertical, "Render a vertical 9:16 copy too");
    ui.separator();
    ui.label(RichText::new("Nuove sequenze con reframe e sottotitoli").size(12.0).strong());
    let targets = [("vertical", "Verticale 9:16"), ("horizontal", "Orizzontale 16:9")];
    let focuses = [
        ("auto", "Automatico (volti)"),
        ("center", "Centro"),
        ("left", "Soggetto a sinistra"),
        ("right", "Soggetto a destra"),
        ("top", "Soggetto in alto"),
        ("bottom", "Soggetto in basso"),
    ];
    ui.horizontal(|ui| {
        craft_ui::ComboBox::from_id_salt("clips-target").selected_text(targets[d.target.min(1)].1).show_ui(ui, |ui| {
            for (i, (_, label)) in targets.iter().enumerate() {
                ui.selectable_value(&mut d.target, i, *label);
            }
        });
        craft_ui::ComboBox::from_id_salt("clips-focus").selected_text(focuses[d.focus.min(focuses.len() - 1)].1).show_ui(ui, |ui| {
            for (i, (_, label)) in focuses.iter().enumerate() {
                ui.selectable_value(&mut d.focus, i, *label);
            }
        });
        ui.checkbox(&mut d.subtitles, "Sottotitoli automatici");
    });
    let selected_focus = focuses[d.focus.min(focuses.len() - 1)].0;
    if selected_focus == "auto" {
        let trackings = vec![("smooth".to_string(), "Traiettoria fluida".to_string()), ("static".to_string(), "Inquadratura statica".to_string())];
        let behaviors = vec![("composed".to_string(), "Camera composta (consigliata)".to_string()), ("legacy".to_string(), "Inseguimento legacy".to_string())];
        ui.horizontal(|ui| {
            combo(app, ui, "clipping.tracking", "Reframe", &mut d.tracking, &trackings);
            combo(app, ui, "clipping.behavior", "Comportamento", &mut d.behavior, &behaviors);
        });
        ui.horizontal(|ui| {
            field(app, ui, "clipping.stability", "Stabilità (0–100)", &mut d.stability);
            field(app, ui, "clipping.response", "Reattività (0–100)", &mut d.response);
        });
        ui.label(
            RichText::new(
                "La camera segue i volti con tagli di scena, dead-zone e limiti di velocità; se il crop non è certificato resta un avviso da rivedere.",
            )
            .size(10.0)
            .color(app.tokens.text_faint),
        );
    }
    if button(app, ui, "clipping.reframe", "Reframe automatico della selezione") {
        let mut params = json!({
            "target": targets[d.target.min(1)].0,
            "focus": selected_focus,
            "subtitles": d.subtitles,
        });
        if let Some(item) = d.item {
            params["item"] = json!(item);
        }
        if let Some(clip) = app.session.state.selection.first().map(|clip| clip.0)
            && let Some((start, end)) = clip_range(&app.session, clip)
        {
            params["startSeconds"] = json!(start);
            params["endSeconds"] = json!(end);
        }
        if selected_focus == "auto" {
            params["tracking"] = json!(if d.tracking == 0 { "smooth" } else { "static" });
            params["behavior"] = json!(if d.behavior == 0 { "composed" } else { "legacy" });
            params["stability"] = json!(d.stability.trim().parse::<f64>().unwrap_or(65.0) / 100.0);
            params["response"] = json!(d.response.trim().parse::<f64>().unwrap_or(65.0) / 100.0);
        }
        run(app, ui, "nl.reframe.sequence", params);
    }
    if button(app, ui, "clipping.reframe_active", "Reframe intera sequenza (16:9 → 9:16)") {
        let mut params = json!({
            "title": "Sequenza reframed",
            "target": targets[d.target.min(1)].0,
            "focus": selected_focus,
            "tracking": if d.tracking == 0 { "smooth" } else { "static" },
            "behavior": if d.behavior == 0 { "composed" } else { "legacy" },
            "subtitles": d.subtitles,
        });
        if selected_focus == "auto" {
            params["stability"] = json!(d.stability.trim().parse::<f64>().unwrap_or(65.0) / 100.0);
            params["response"] = json!(d.response.trim().parse::<f64>().unwrap_or(65.0) / 100.0);
        }
        run(app, ui, "nl.reframe.active", params);
    }
    if button(app, ui, "clipping.analyze", "Find candidates") {
        let mut params = json!({"mode": modes[d.mode.min(modes.len() - 1)].0});
        if let Some(item) = d.item {
            params["item"] = json!(item);
        }
        run(app, ui, "nl.clipping.propose", params);
    }
    job_line(app, ui, "NL clipping");
    let clips = app.session.nl.result("clips").cloned();
    let reels = app.session.nl.result("reels").cloned();
    let mut actions: Vec<(&str, Value)> = Vec::new();
    if let Some(result) = clips {
        let rows = result["clips"].as_array().cloned().unwrap_or_default();
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!("{} candidate(s) · {} words analysed", rows.len(), result["words_count"].as_u64().unwrap_or(0)))
                .size(12.0)
                .color(app.tokens.text_dim),
        );
        let mut flags = selection(ui, "nl-clipping-selected", rows.len());
        let mut selected: Vec<Value> = Vec::new();
        egui::ScrollArea::vertical().id_salt("nl-clipping-list").max_height(260.0).show(ui, |ui| {
            for (i, row) in rows.iter().enumerate() {
                let start = row["start"].as_f64().unwrap_or(0.0);
                let end = row["end"].as_f64().unwrap_or(0.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut flags[i], "");
                    ui.label(RichText::new(row["title"].as_str().unwrap_or("Clip")).size(12.0));
                    ui.label(
                        RichText::new(format!("{} · {:.1} s · {}/10", time_range(start, end), end - start, row["score"].as_u64().unwrap_or(0)))
                            .size(11.0)
                            .color(app.tokens.text_dim),
                    );
                });
                if flags[i] {
                    selected.push(json!({"title": row["title"], "start": start, "end": end, "score": row["score"]}));
                }
            }
        });
        save_selection(ui, "nl-clipping-selected", &flags);
        ui.horizontal(|ui| {
            if button(app, ui, "clipping.render", "Render selected") && !selected.is_empty() {
                actions.push(("nl.clipping.render", json!({"items": selected, "vertical": d.vertical})));
            }
            if button(app, ui, "clipping.sequences", "Crea sequenze con reframe + sottotitoli") && !selected.is_empty() {
                let mut params = json!({
                    "items": selected.clone(),
                    "target": targets[d.target.min(1)].0,
                    "focus": focuses[d.focus.min(focuses.len() - 1)].0,
                    "subtitles": d.subtitles,
                });
                if focuses[d.focus.min(focuses.len() - 1)].0 == "auto" {
                    params["tracking"] = json!(if d.tracking == 0 { "smooth" } else { "static" });
                    params["behavior"] = json!(if d.behavior == 0 { "composed" } else { "legacy" });
                    params["stability"] = json!(d.stability.trim().parse::<f64>().unwrap_or(65.0) / 100.0);
                    params["response"] = json!(d.response.trim().parse::<f64>().unwrap_or(65.0) / 100.0);
                }
                if let Some(item) = d.item {
                    params["item"] = json!(item);
                }
                if let Some(words) = result["_transcript"]["words"].as_array().cloned().or_else(|| result["words"].as_array().cloned()) {
                    params["words"] = json!(words);
                }
                actions.push(("nl.clipping.sequences", params));
            }
            if button(app, ui, "clipping.srt", "Write SRT") && !selected.is_empty() {
                for clip in &selected {
                    actions.push(("nl.subtitles.srt", json!({"start": clip["start"], "end": clip["end"], "hormozi": d.hormozi})));
                }
            }
        });
        ui.checkbox(&mut d.hormozi, "Two-word uppercase subtitles (Hormozi style)");
    }
    if let Some(result) = reels {
        let rows = result["reels"].as_array().cloned().unwrap_or_default();
        ui.add_space(6.0);
        ui.label(RichText::new(format!("{} narrative reel(s) proposed", rows.len())).size(12.0).color(app.tokens.text_dim));
        let mut flags = selection(ui, "nl-reels-selected", rows.len());
        for (i, row) in rows.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.checkbox(&mut flags[i], "");
                ui.label(RichText::new(row["title"].as_str().unwrap_or("Reel")).size(12.0));
                ui.label(
                    RichText::new(format!("{} segment(s) · {:.1} s", row["segments"].as_array().map_or(0, Vec::len), row["total_dur"].as_f64().unwrap_or(0.0)))
                        .size(11.0)
                        .color(app.tokens.text_dim),
                );
            });
        }
        save_selection(ui, "nl-reels-selected", &flags);
        let chosen: Vec<Value> = rows.iter().enumerate().filter(|(i, _)| flags[*i]).map(|(_, row)| row.clone()).collect();
        if button(app, ui, "clipping.render_reels", "Render selected reels") && !chosen.is_empty() {
            actions.push(("nl.clipping.render", json!({"items": chosen, "vertical": d.vertical})));
        }
    }
    for (cmd, params) in actions {
        run(app, ui, cmd, params);
    }
    save_draft(ui, "nl-clipping", d);
}

/// NL Rough Cut: the cut plan and its render.
fn sgrossa(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Rough Cut",
        "Silences, breaths, fillers and retakes removed from the spoken track: a reviewable plan first, the render only for the pieces you keep.",
    );
    error_line(app, ui);
    let mut actions: Vec<(&str, Value)> = Vec::new();
    if button(app, ui, "sgrossa.analyze", "Analyse") {
        actions.push(("nl.sgrossa.analyze", json!({})));
    }
    job_line(app, ui, "NL rough cut");
    let result = app.session.nl.result("cleanup").cloned();
    if let Some(result) = result {
        let cuts = result["cuts"].as_array().cloned().unwrap_or_default();
        let good = result["good"].as_array().cloned().unwrap_or_default();
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!(
                "{:.1} s source · {} cut(s), {:.1} s removed · {} piece(s) to keep",
                result["duration"].as_f64().unwrap_or(0.0),
                cuts.len(),
                result["removed_sec"].as_f64().unwrap_or(0.0),
                good.len()
            ))
            .size(12.0)
            .color(app.tokens.text_dim),
        );
        let overlap = result["overlap_sec"].as_f64().unwrap_or(0.0);
        if overlap > 0.5 {
            ui.label(
                RichText::new(format!("Warning: {overlap:.2} s of speech would fall inside the cuts; review the plan before rendering."))
                    .size(12.0)
                    .color(egui::Color32::from_rgb(233, 180, 120)),
            );
        }
        if !cuts.is_empty() {
            egui::ScrollArea::vertical().id_salt("nl-sgrossa-cuts").max_height(200.0).show(ui, |ui| {
                for cut in &cuts {
                    ui.label(
                        RichText::new(format!(
                            "✂ {} · {:.2} s · {}",
                            time_range(cut["start"].as_f64().unwrap_or(0.0), cut["end"].as_f64().unwrap_or(0.0)),
                            cut["dur"].as_f64().unwrap_or(0.0),
                            cut["reason"].as_str().unwrap_or("cut")
                        ))
                        .size(11.0)
                        .color(app.tokens.text_dim),
                    );
                }
            });
        }
        let mut flags = selection(ui, "nl-sgrossa-keep", good.len());
        ui.label(RichText::new("Pieces to keep (uncheck one to leave it out)").size(12.0));
        let mut keep: Vec<Value> = Vec::new();
        egui::ScrollArea::vertical().id_salt("nl-sgrossa-good").max_height(220.0).show(ui, |ui| {
            for (i, piece) in good.iter().enumerate() {
                let start = piece[0].as_f64().unwrap_or(0.0);
                let end = piece[1].as_f64().unwrap_or(0.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut flags[i], "");
                    ui.label(RichText::new(format!("{} ({:.2} s)", time_range(start, end), end - start)).size(11.0).color(app.tokens.text_dim));
                });
                if flags[i] {
                    keep.push(json!([start, end]));
                }
            }
        });
        save_selection(ui, "nl-sgrossa-keep", &flags);
        ui.horizontal(|ui| {
            if button(app, ui, "sgrossa.apply", "Render kept pieces") && !keep.is_empty() {
                actions.push(("nl.sgrossa.apply", json!({"items": keep})));
            }
            if button(app, ui, "sgrossa.report", "Write report") {
                actions.push(("nl.sgrossa.report", json!({})));
            }
        });
    }
    for (cmd, params) in actions {
        run(app, ui, cmd, params);
    }
}

/// NL Cover: a thumbnail generated from a chosen frame, with the speech near it as context.
fn cover(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Cover",
        "A cover (thumbnail) generated from the frame at the playhead, guided by what is being said there. Review it, then mark the image you like as the cover.",
    );
    error_line(app, ui);
    #[derive(Clone, Default)]
    struct CoverDraft {
        prompt: String,
        workflow: usize,
        seed: u64,
    }
    let mut d: CoverDraft = draft(ui, "nl-cover");
    let options = workflow_options(app, ui, "image");
    if d.workflow >= options.len() {
        d.workflow = default_workflow(&options, app, "image");
    }
    let time = app.session.playhead();
    let seconds = time.0 as f64 / filmcraft_time::TICKS_PER_SECOND as f64;
    ui.label(RichText::new(format!("Frame at {}", time_range(seconds, seconds))).size(12.0).color(app.tokens.text_dim));
    combo(app, ui, "cover.workflow", "Workflow (needs an image input)", &mut d.workflow, &options);
    let selected_workflow = options.get(d.workflow).map(|(path, _)| path.clone()).unwrap_or_default();
    delete_workflow_button(app, ui, "image", &selected_workflow);
    let mut prompt = d.prompt.clone();
    ui.label(RichText::new("Description (empty = taken from the speech near the frame)").size(12.0).color(app.tokens.text_dim));
    let r = ui.add(egui::TextEdit::multiline(&mut prompt).desired_rows(3).desired_width(f32::INFINITY).id_salt("nl-cover-prompt"));
    app.auto.add("nl.cover.prompt", r.rect, "Description");
    d.prompt = prompt;
    if button(app, ui, "cover.from_speech", "Take the speech here") {
        match app.session.execute("nl.cover.prompt", json!({"time": time.0})) {
            Ok(value) => match value["prompt"].as_str() {
                Some(text) => d.prompt = text.to_string(),
                None => app.ui.status = "No speech near this frame: transcribe the sequence first.".into(),
            },
            Err(e) => app.ui.status = e.to_string(),
        }
    }
    let mut seed = d.seed.to_string();
    field(app, ui, "cover.seed", "Seed (0 = random)", &mut seed);
    d.seed = seed.parse().unwrap_or(0);
    let image_scale = app.session.prefs.nl.image_scale.clone();
    size_preview(app, ui, "image", &image_scale);
    ui.add_space(6.0);
    if button(app, ui, "cover.generate", "Generate cover from this frame") {
        let mut params = json!({"time": time.0, "seed": d.seed});
        if !d.prompt.trim().is_empty() {
            params["prompt"] = json!(d.prompt.trim());
        }
        if let Some((path, _)) = options.get(d.workflow) {
            params["workflow"] = json!(path);
        }
        run(app, ui, "graphics.generateCover", params);
    }
    job_line(app, ui, "ComfyUI cover");
    ui.add_space(8.0);
    ui.separator();
    let cover = app.session.nl.cover.clone();
    match &cover {
        Some(value) => {
            let path = value["path"].as_str().unwrap_or("the selected image");
            ui.label(RichText::new(format!("Cover set: {path}")).size(12.0));
        }
        None => {
            ui.label(RichText::new("No cover set yet: select the generated image in the Project panel and mark it.").size(12.0).color(app.tokens.text_faint));
        }
    }
    if button(app, ui, "cover.set", "Use the selected image as cover") {
        run(app, ui, "nl.cover.set", json!({}));
    }
    save_draft(ui, "nl-cover", d);
}

/// NL Workflows: the library, the API check and the doctor in one place.
fn workflow_parameters_editor(app: &mut FilmcraftApp, ui: &mut egui::Ui, value: &Value) {
    let total = value["total"].as_u64().unwrap_or(0);
    let missing = value["missingClasses"].as_array().map_or(0, Vec::len);
    ui.label(
        RichText::new(format!(
            "Parameters: {total} widget(s){}{}",
            if missing > 0 { format!(", {missing} class(es) missing") } else { String::new() },
            if value["truncated"] == true { " (showing the first ones)".to_string() } else { String::new() }
        ))
        .size(12.0),
    );
    let edit_id = egui::Id::unique("nl-workflow-param-edits");
    let filter_id = egui::Id::unique("nl-workflow-param-filter");
    let mut edits = ui.data(|d| d.get_temp::<Value>(edit_id)).unwrap_or_else(|| json!({}));
    let mut filter = ui.data(|d| d.get_temp::<String>(filter_id)).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Filtra").size(11.0));
        if ui.text_edit_singleline(&mut filter).changed() {
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        }
    });
    let rows: Vec<Value> = value["parameters"].as_array().cloned().unwrap_or_default();
    let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
    for row in rows {
        let class = row["class"].as_str().unwrap_or("node").to_string();
        if !filter.trim().is_empty() {
            let hay = format!("{} {} {}", class, row["field"].as_str().unwrap_or(""), row["title"].as_str().unwrap_or("")).to_lowercase();
            if !hay.contains(&filter.trim().to_lowercase()) {
                continue;
            }
        }
        match groups.iter_mut().find(|(name, _)| name == &class) {
            Some((_, list)) => list.push(row),
            None => groups.push((class, vec![row])),
        }
    }
    groups.sort_by(|a, b| a.0.cmp(&b.0));
    let workflow_path = value["workflow"].as_str().unwrap_or("").to_string();
    let mut edits_changed = false;
    for (class, list) in &groups {
        egui::CollapsingHeader::new(format!("Avanzate · {class} ({} parametri)", list.len())).default_open(false).show(ui, |ui| {
            for row in list {
                if row["linked"] == true {
                    continue;
                }
                let node = row["node"].as_str().unwrap_or("");
                let field = row["field"].as_str().unwrap_or("");
                let key = format!("{node}:{field}");
                let base = row["value"].clone();
                let mut current = edits.get(&key).cloned().unwrap_or_else(|| base.clone());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{node} · {field}")).size(11.0).color(app.tokens.text_dim));
                    match row["kind"].as_str().unwrap_or("input") {
                        "choice" => {
                            let choices = row["choices"].as_array().cloned().unwrap_or_default();
                            let selected = current.as_str().unwrap_or("").to_string();
                            craft_ui::ComboBox::from_id_salt(("wf-param", node, field)).selected_text(selected.clone()).width(260.0).show_ui(ui, |ui| {
                                for choice in choices {
                                    let label = choice.as_str().map(str::to_string).unwrap_or_else(|| choice.to_string());
                                    if ui.selectable_label(selected == label, label.clone()).clicked() {
                                        current = choice.clone();
                                    }
                                }
                            });
                        }
                        "bool" => {
                            let mut v = current.as_bool().unwrap_or(false);
                            if ui.checkbox(&mut v, "").changed() {
                                current = json!(v);
                            }
                        }
                        "int" => {
                            let mut v = current.as_f64().unwrap_or(0.0);
                            let mut dv = egui::DragValue::new(&mut v).speed(1.0);
                            if let Some(min) = row["min"].as_f64() {
                                dv = dv.range(min..=row["max"].as_f64().unwrap_or(f64::MAX));
                            }
                            if ui.add(dv).changed() {
                                current = json!(v.round() as i64);
                            }
                        }
                        "float" => {
                            let mut v = current.as_f64().unwrap_or(0.0);
                            let mut dv = egui::DragValue::new(&mut v).speed(0.1);
                            if let Some(min) = row["min"].as_f64() {
                                dv = dv.range(min..=row["max"].as_f64().unwrap_or(f64::MAX));
                            }
                            if ui.add(dv).changed() {
                                current = json!(v);
                            }
                        }
                        _ => {
                            let mut text = current.as_str().unwrap_or("").to_string();
                            if ui.text_edit_singleline(&mut text).changed() {
                                current = json!(text);
                            }
                        }
                    }
                });
                if current != base {
                    edits[key] = current;
                    edits_changed = true;
                }
            }
        });
    }
    if edits_changed {
        ui.data_mut(|d| d.insert_temp(edit_id, edits.clone()));
    }
    let overrides: Vec<Value> = groups
        .iter()
        .flat_map(|(class, list)| {
            let mut out = Vec::new();
            for row in list {
                let node = row["node"].as_str().unwrap_or("");
                let field = row["field"].as_str().unwrap_or("");
                if let Some(value) = edits.get(format!("{node}:{field}")) {
                    out.push(json!({
                        "node": node, "class": class, "title": row["title"],
                        "field": field, "value": value,
                    }));
                }
            }
            Some(out)
        })
        .flatten()
        .collect();
    ui.horizontal(|ui| {
        if ui.button("Copia overrides JSON").clicked() {
            let text = serde_json::to_string_pretty(&overrides).unwrap_or_else(|_| "[]".into());
            ui.ctx().copy_text(text);
            app.ui.status = format!("{} override(s) copiati: incollali nel campo Widget overrides o usali qui.", overrides.len());
        }
        if ui.button("Usa per la prossima generazione").clicked() {
            ui.data_mut(|d| d.insert_temp(egui::Id::unique("nl-workflow-overrides"), json!(overrides)));
            app.ui.status = format!("{} override(s) pronti: NL Generate → Widget overrides JSON.", overrides.len());
        }
    });
    ui.add_space(6.0);
    ui.separator();
    ui.label(RichText::new("Preset parametri (riutilizzabili su altri workflow)").size(12.0));
    let presets_id = egui::Id::unique("nl-workflow-presets");
    let name_id = egui::Id::unique("nl-workflow-preset-name");
    let mut preset_name = ui.data(|d| d.get_temp::<String>(name_id)).unwrap_or_default();
    ui.horizontal(|ui| {
        if ui.button("Aggiorna elenco").clicked() {
            match app.session.execute("nl.workflows.presets", json!({"action": "list"})) {
                Ok(value) => {
                    ui.data_mut(|d| d.insert_temp(presets_id, value));
                }
                Err(e) => app.ui.status = e.to_string(),
            }
        }
        ui.label(RichText::new("Nome").size(11.0));
        if ui.text_edit_singleline(&mut preset_name).changed() {
            ui.data_mut(|d| d.insert_temp(name_id, preset_name.clone()));
        }
        if ui.button("Salva preset").clicked() {
            if preset_name.trim().is_empty() {
                app.ui.status = "Dai un nome al preset.".into();
            } else {
                match app
                    .session
                    .execute("nl.workflows.presets", json!({"action": "save", "name": preset_name.trim(), "workflow": workflow_path, "overrides": overrides}))
                {
                    Ok(_) => app.ui.status = format!("Preset «{}» salvato.", preset_name.trim()),
                    Err(e) => app.ui.status = e.to_string(),
                }
            }
        }
    });
    if let Some(list) = ui.data(|d| d.get_temp::<Value>(presets_id)) {
        let names: Vec<String> = list["presets"].as_array().into_iter().flatten().filter_map(|p| p["name"].as_str().map(str::to_string)).collect();
        let selected_id = egui::Id::unique("nl-workflow-preset-selected");
        let mut selected = ui.data(|d| d.get_temp::<String>(selected_id)).unwrap_or_default();
        ui.horizontal(|ui| {
            craft_ui::ComboBox::from_id_salt("wf-preset-select")
                .selected_text(if selected.is_empty() { "scegli preset".to_string() } else { selected.clone() })
                .width(220.0)
                .show_ui(ui, |ui| {
                    for name in &names {
                        if ui.selectable_label(selected == *name, name).clicked() {
                            selected = name.clone();
                            ui.data_mut(|d| d.insert_temp(selected_id, selected.clone()));
                        }
                    }
                });
            if ui.button("Carica").clicked() && !selected.is_empty() {
                match app.session.execute("nl.workflows.presets", json!({"action": "load", "name": selected})) {
                    Ok(value) => {
                        if let Some(list) = value["preset"]["overrides"].as_array() {
                            let mut loaded = ui.data(|d| d.get_temp::<Value>(edit_id)).unwrap_or_else(|| json!({}));
                            for override_value in list {
                                let node = override_value["node"].as_str().unwrap_or("");
                                let field = override_value["field"].as_str().unwrap_or("");
                                let class = override_value["class"].as_str().unwrap_or("");
                                let mut key = format!("{node}:{field}");
                                if edits.get(&key).is_none() {
                                    // A preset copied from another workflow may only match by class.
                                    for (_, list) in &groups {
                                        for row in list {
                                            if row["field"] == field && (node.is_empty() || row["class"] == class) {
                                                key = format!("{}:{}", row["node"].as_str().unwrap_or(""), field);
                                            }
                                        }
                                    }
                                }
                                loaded[key] = override_value["value"].clone();
                            }
                            ui.data_mut(|d| d.insert_temp(edit_id, loaded));
                            app.ui.status = format!("Preset «{selected}» caricato: valori applicati al pannello.");
                        }
                    }
                    Err(e) => app.ui.status = e.to_string(),
                }
            }
            if ui.button("Elimina").clicked() && !selected.is_empty() {
                match app.session.execute("nl.workflows.presets", json!({"action": "delete", "name": selected})) {
                    Ok(_) => app.ui.status = format!("Preset «{selected}» eliminato."),
                    Err(e) => app.ui.status = e.to_string(),
                }
            }
        });
    }
}
fn workflows_panel(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Workflows",
        "Everything the suite can run, checked against the ComfyUI that is really running: node classes, model files and reference inputs. Add workflows by importing the JSON saved in API Format.",
    );
    error_line(app, ui);
    let now = ui.input(|i| i.time);
    let list_id = egui::Id::unique("nl-workflows-panel-list");
    let mut result: Value = ui.data(|d| d.get_temp::<Value>(list_id)).unwrap_or(Value::Null);
    let stale = ui.data(|d| d.get_temp::<f64>(list_id.with("at")).is_none_or(|at| now - at > 5.0));
    let mut refresh = stale || result.is_null();
    if button(app, ui, "workflows.refresh", "Refresh library") {
        refresh = true;
    }
    if refresh {
        match app.session.execute("nl.workflows.list", json!({"kind": "all"})) {
            Ok(value) => {
                result = value;
                ui.data_mut(|d| {
                    d.insert_temp(list_id, result.clone());
                    d.insert_temp(list_id.with("at"), now);
                });
            }
            Err(e) => app.ui.status = e.to_string(),
        }
    }
    let mut actions: Vec<(&str, Value)> = Vec::new();
    ui.horizontal(|ui| {
        if button(app, ui, "workflows.verify", "Verify ComfyUI API") {
            actions.push(("nl.workflows.verify", json!({})));
        }
        if button(app, ui, "workflows.diagnose_all", "Doctor all workflows") {
            actions.push(("nl.workflows.diagnose", json!({"all": true})));
        }
    });
    let verified = result["apiVerified"].as_bool().unwrap_or(false);
    ui.label(
        RichText::new(if verified {
            "API verified: the checks below include the node classes and models this ComfyUI has."
        } else {
            "Run “Verify ComfyUI API” for live checks; without it only the file itself is examined."
        })
        .size(12.0)
        .color(app.tokens.text_dim),
    );
    let rows = result["workflows"].as_array().cloned().unwrap_or_default();
    #[derive(Clone, Default)]
    struct PanelState {
        selected: usize,
        import_path: String,
    }
    let mut state: PanelState = draft(ui, "nl-workflows-panel");
    if state.selected >= rows.len() {
        state.selected = 0;
    }
    ui.add_space(4.0);
    ui.label(RichText::new(format!("{} workflow(s), root: {}", rows.len(), result["root"].as_str().unwrap_or("—"))).size(12.0).color(app.tokens.text_dim));
    egui::ScrollArea::vertical().id_salt("nl-workflows-list").max_height(300.0).show(ui, |ui| {
        for (i, row) in rows.iter().enumerate() {
            let kind = row["kind"].as_str().unwrap_or("other");
            let caps = &row["capabilities"];
            let mut badges = vec![kind.to_string()];
            if caps["v2v"].as_bool().unwrap_or(false) {
                badges.push("v2v".into());
            }
            if !caps["imageRefs"].as_array().is_none_or(Vec::is_empty) {
                badges.push("ref img".into());
            }
            if !caps["audioInputs"].as_array().is_none_or(Vec::is_empty) {
                badges.push("audio in".into());
            }
            let label = format!(
                "{}  [{}]  {} KB{}",
                row["name"].as_str().unwrap_or("workflow"),
                badges.join(" · "),
                row["bytes"].as_u64().unwrap_or(0) / 1024,
                if row.get("error").is_some() { "  ⚠ unreadable" } else { "" }
            );
            if ui.selectable_label(state.selected == i, RichText::new(label).size(12.0)).clicked() {
                state.selected = i;
            }
        }
    });
    ui.add_space(6.0);
    if let Some(row) = rows.get(state.selected) {
        let path = row["path"].as_str().unwrap_or_default().to_string();
        ui.label(RichText::new(format!("Selected: {path}")).size(12.0));
        ui.horizontal(|ui| {
            if button(app, ui, "workflows.diagnose_one", "Doctor this one") {
                actions.push(("nl.workflows.diagnose", json!({"workflow": path})));
            }
            if button(app, ui, "workflows.fix", "Write a repaired copy") {
                actions.push(("nl.workflows.fix", json!({"workflow": path})));
            }
            if button(app, ui, "workflows.params", "Show parameters") {
                actions.push(("nl.workflows.parameters", json!({"workflow": path, "api": true})));
            }
        });
    }
    ui.add_space(8.0);
    ui.separator();
    ui.label(RichText::new("Import a workflow (ComfyUI ▸ Save (API Format))").size(12.0));
    ui.horizontal(|ui| {
        field(app, ui, "workflows.import_path", "JSON file", &mut state.import_path);
        if ui.button("Pick…").clicked() {
            app.pick_ui(crate::native_dialogs::FileDialog::OpenFile { filter: "ComfyUI API workflow".into(), exts: vec!["json".into()] }, |app, paths| {
                if let Some(path) = paths.first()
                    && let Err(e) = app.session.execute("nl.workflows.import", json!({"path": path}))
                {
                    app.ui.status = e.to_string();
                }
            });
        }
        if button(app, ui, "workflows.import", "Import") {
            if state.import_path.trim().is_empty() {
                app.ui.status = "Choose the workflow JSON (API Format) first.".into();
            } else {
                actions.push(("nl.workflows.import", json!({"path": state.import_path.trim()})));
            }
        }
    });
    for (cmd, params) in actions {
        match app.session.execute(cmd, params.clone()) {
            Ok(value) => {
                let key = format!("nl-result-{cmd}");
                ui.data_mut(|d| d.insert_temp(egui::Id::unique(key.as_str()), value.clone()));
                if cmd == "nl.workflows.import" {
                    state.import_path.clear();
                    ui.data_mut(|d| d.insert_temp(list_id.with("at"), 0.0f64));
                }
            }
            Err(e) => app.ui.status = e.to_string(),
        }
    }
    save_draft(ui, "nl-workflows-panel", state);
    // The last doctor/verify answer, kept until the next run.
    for key in ["nl.workflows.diagnose", "nl.workflows.verify", "nl.workflows.fix", "nl.workflows.parameters"] {
        let id = egui::Id::unique(format!("nl-result-{key}").as_str());
        let Some(value) = ui.data(|d| d.get_temp::<Value>(id)) else { continue };
        ui.add_space(8.0);
        ui.separator();
        if key == "nl.workflows.parameters" {
            workflow_parameters_editor(app, ui, &value);
        } else if key == "nl.workflows.verify" {
            let classes = value["classes"].as_u64().unwrap_or(0);
            let missing = value["missingWorkflows"].as_u64().unwrap_or(0);
            ui.label(RichText::new(format!("API: {classes} node classes · {missing} workflow(s) with missing classes")).size(12.0));
            if let Some(rows) = value["workflows"].as_array() {
                for row in rows.iter().filter(|r| r["ok"] == false) {
                    ui.label(
                        RichText::new(format!("{}: missing {}", row["path"].as_str().unwrap_or(""), row["missing"].as_array().map_or(0, Vec::len)))
                            .size(11.0)
                            .color(egui::Color32::from_rgb(233, 180, 120)),
                    );
                }
            }
        } else if key == "nl.workflows.fix" {
            if value["fixed"].as_bool().unwrap_or(false) {
                ui.label(RichText::new(format!("Repaired copy: {}", value["path"].as_str().unwrap_or(""))).size(12.0));
                for fix in value["fixes"].as_array().into_iter().flatten() {
                    ui.label(RichText::new(format!("· {}", fix.as_str().unwrap_or(""))).size(11.0).color(app.tokens.text_dim));
                }
            } else {
                ui.label(RichText::new(value["note"].as_str().unwrap_or("no automatic fix")).size(12.0).color(app.tokens.text_dim));
            }
        } else {
            let counts = &value["counts"];
            ui.label(
                RichText::new(format!(
                    "Doctor: {} error(s), {} warning(s), {} note(s){}",
                    counts["error"].as_u64().unwrap_or(0),
                    counts["warning"].as_u64().unwrap_or(0),
                    counts["info"].as_u64().unwrap_or(0),
                    if value["apiVerified"].as_bool().unwrap_or(false) { " (live)" } else { "" }
                ))
                .size(12.0),
            );
            for issue in value["issues"].as_array().into_iter().flatten().take(60) {
                let color = match issue["severity"].as_str() {
                    Some("error") => egui::Color32::from_rgb(233, 120, 120),
                    Some("warning") => egui::Color32::from_rgb(233, 180, 120),
                    _ => app.tokens.text_dim,
                };
                let node = issue["node"].as_str().map(|n| format!("node {n}: ")).unwrap_or_default();
                ui.label(RichText::new(format!("{node}{}", issue["message"].as_str().unwrap_or(""))).size(11.0).color(color));
                if let Some(fix) = issue["fix"].as_str() {
                    ui.label(RichText::new(format!("   → {fix}")).size(11.0).color(app.tokens.text_faint));
                }
            }
            // Doctor-all summary.
            if let Some(rows) = value["workflows"].as_array() {
                ui.label(RichText::new(format!("{} workflow(s) with issues", rows.len())).size(12.0).color(app.tokens.text_dim));
                for row in rows.iter().take(20) {
                    let counts = &row["counts"];
                    ui.label(
                        RichText::new(format!(
                            "{}: {} error(s), {} warning(s)",
                            row["path"].as_str().unwrap_or(""),
                            counts["error"].as_u64().unwrap_or(0),
                            counts["warning"].as_u64().unwrap_or(0)
                        ))
                        .size(11.0)
                        .color(app.tokens.text_dim),
                    );
                }
            }
        }
    }
}

/// NL Story: an AI-drafted story with scenes and prompts, keyframes generated with ComfyUI and
/// placed on the timeline.
fn story(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Story",
        "Draft a story with the local model, refine each scene, generate its keyframe with ComfyUI and place the scenes on the timeline in order.",
    );
    error_line(app, ui);
    let now = ui.input(|i| i.time);
    let cache = egui::Id::unique("nl-story-cache");
    let mut data: Value = ui.data(|d| d.get_temp::<Value>(cache)).unwrap_or(Value::Null);
    let stale = ui.data(|d| d.get_temp::<f64>(cache.with("at")).is_none_or(|at| now - at > 2.0));
    let refresh = stale || data.is_null();
    if refresh {
        match app.session.execute("nl.story.get", json!({})) {
            Ok(value) => {
                data = value;
                ui.data_mut(|d| {
                    d.insert_temp(cache, data.clone());
                    d.insert_temp(cache.with("at"), now);
                });
            }
            Err(e) => app.ui.status = e.to_string(),
        }
    }
    #[derive(Clone, Default)]
    struct StoryDraft {
        idea: String,
        scene: usize,
        heading: String,
        action: String,
        dialogue: String,
        image_prompt: String,
        motion_prompt: String,
        duration: f64,
        instructions: String,
        loaded_scene: i64,
        use_sheet: bool,
    }
    let mut d: StoryDraft = draft(ui, "nl-story-draft");
    let story = data["story"].clone();
    let scenes = story["scenes"].as_array().cloned().unwrap_or_default();
    let mut actions: Vec<(&str, Value)> = Vec::new();
    // Draft from an idea.
    let mut idea = d.idea.clone();
    ui.label(RichText::new("Idea").size(12.0).color(app.tokens.text_dim));
    let r = ui.add(egui::TextEdit::multiline(&mut idea).desired_rows(2).desired_width(f32::INFINITY).id_salt("nl-story-idea"));
    app.auto.add("nl.story.idea", r.rect, "Idea");
    d.idea = idea;
    ui.horizontal(|ui| {
        if button(app, ui, "story.draft", "Draft story with AI") {
            if d.idea.trim().is_empty() {
                app.ui.status = "Write the idea first.".into();
            } else {
                actions.push(("nl.story.draft", json!({"idea": d.idea.trim()})));
            }
        }
        let saved = data["saved"].as_bool().unwrap_or(false);
        ui.label(
            RichText::new(if saved { "saved beside the project".to_string() } else { "session only: save the project to keep it".to_string() })
                .size(11.0)
                .color(app.tokens.text_faint),
        );
    });
    job_line(app, ui, "NL story draft");
    if !story["title"].as_str().unwrap_or_default().is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(story["title"].as_str().unwrap_or_default()).size(14.0).strong());
        let synopsis = story["synopsis"].as_str().unwrap_or_default();
        if !synopsis.is_empty() {
            ui.label(RichText::new(synopsis).size(12.0).color(app.tokens.text_dim));
        }
    }
    // Reference library: sheets and plates are generated once and reused by every scene keyframe.
    let characters = story["characters"].as_array().cloned().unwrap_or_default();
    let locations = story["locations"].as_array().cloned().unwrap_or_default();
    if !characters.is_empty() || !locations.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new("Reference library: generate a sheet/plate once, scenes reuse them automatically").size(11.0).color(app.tokens.text_dim));
        ui.horizontal_wrapped(|ui| {
            for (index, character) in characters.iter().enumerate() {
                let name = character["name"].as_str().unwrap_or("character");
                let linked = character["sheet"]["item"].as_u64().is_some();
                let id = format!("nl.story.sheet.{index}");
                if button(app, ui, &id, &format!("Sheet: {name}{}", if linked { " ✓" } else { "" })) {
                    actions.push(("nl.story.sheet", json!({"character": index + 1})));
                }
            }
            for (index, location) in locations.iter().enumerate() {
                let name = location["name"].as_str().unwrap_or("location");
                let linked = location["plate"]["item"].as_u64().is_some();
                let id = format!("nl.story.plate.{index}");
                if button(app, ui, &id, &format!("Plate: {name}{}", if linked { " ✓" } else { "" })) {
                    actions.push(("nl.story.plate", json!({"location": index + 1})));
                }
            }
        });
    }
    if scenes.is_empty() {
        ui.add_space(6.0);
        ui.label(
            RichText::new("No story yet: write an idea and draft it, or load a saved project that already has one.").size(12.0).color(app.tokens.text_faint),
        );
    } else {
        ui.add_space(6.0);
        ui.label(RichText::new(format!("{} scene(s): select one to refine, generate or place", scenes.len())).size(12.0).color(app.tokens.text_dim));
        if d.scene >= scenes.len() {
            d.scene = 0;
        }
        egui::ScrollArea::vertical().id_salt("nl-story-scenes").max_height(150.0).show(ui, |ui| {
            for (i, scene) in scenes.iter().enumerate() {
                let generated = scene["image"].get("item").is_some() || scene["video"].get("item").is_some();
                let duration = scene["durationSec"].as_f64().unwrap_or(0.0);
                let shots = scene["shots"].as_array().map_or(0, Vec::len);
                let label = format!(
                    "{:>2}. {}  · {:.1} s{}{}",
                    scene["id"].as_u64().unwrap_or(i as u64 + 1),
                    scene["heading"].as_str().unwrap_or("scene"),
                    duration,
                    if shots > 0 { format!("  · {shots} shot(s)") } else { String::new() },
                    if generated { "  ✓ keyframe" } else { "" }
                );
                if ui.selectable_label(d.scene == i, RichText::new(label).size(12.0)).clicked() {
                    d.scene = i;
                }
            }
        });
        let Some(scene) = scenes.get(d.scene).cloned() else { return };
        let scene_id = scene["id"].as_u64().unwrap_or(d.scene as u64 + 1);
        // Load the selected scene into the editor once per selection (and after saving).
        if d.loaded_scene != scene_id as i64 {
            d.heading = scene["heading"].as_str().unwrap_or_default().to_string();
            d.action = scene["action"].as_str().unwrap_or_default().to_string();
            d.dialogue = scene["dialogue"].as_str().unwrap_or_default().to_string();
            d.image_prompt = scene["imagePrompt"].as_str().unwrap_or_default().to_string();
            d.motion_prompt = scene["motionPrompt"].as_str().unwrap_or_default().to_string();
            d.duration = scene["durationSec"].as_f64().unwrap_or(5.0);
            d.instructions.clear();
            d.loaded_scene = scene_id as i64;
        }
        ui.add_space(6.0);
        let mut heading = d.heading.clone();
        field(app, ui, "story.heading", "Scene heading", &mut heading);
        d.heading = heading;
        let mut action = d.action.clone();
        ui.label(RichText::new("Action").size(12.0).color(app.tokens.text_dim));
        let r = ui.add(egui::TextEdit::multiline(&mut action).desired_rows(2).desired_width(f32::INFINITY).id_salt("nl-story-action"));
        app.auto.add("nl.story.action", r.rect, "Action");
        d.action = action;
        let mut dialogue = d.dialogue.clone();
        ui.label(RichText::new("Dialogue").size(12.0).color(app.tokens.text_dim));
        let r = ui.add(egui::TextEdit::multiline(&mut dialogue).desired_rows(2).desired_width(f32::INFINITY).id_salt("nl-story-dialogue"));
        app.auto.add("nl.story.dialogue", r.rect, "Dialogue");
        d.dialogue = dialogue;
        let mut image_prompt = d.image_prompt.clone();
        ui.label(RichText::new("Keyframe prompt (English)").size(12.0).color(app.tokens.text_dim));
        let r = ui.add(egui::TextEdit::multiline(&mut image_prompt).desired_rows(2).desired_width(f32::INFINITY).id_salt("nl-story-image-prompt"));
        app.auto.add("nl.story.imagePrompt", r.rect, "Keyframe prompt");
        d.image_prompt = image_prompt;
        ui.horizontal(|ui| {
            if button(app, ui, "story.enhance_prompt", "Enhance keyframe prompt with AI") {
                let brief = format!("{} {}", d.heading.trim(), d.action.trim()).trim().to_string();
                let brief = if brief.is_empty() { d.image_prompt.clone() } else { brief };
                let ctx = ui.ctx().clone();
                if let Err(e) = crate::menus::invoke(app, &ctx, "nl.prompt.enhance", json!({"brief": brief, "kind": "image"})) {
                    app.ui.status = e;
                }
            }
            if let Some(suggestion) = app.session.nl.result("prompt").cloned()
                && let Some(text) = suggestion["prompt"].as_str().map(str::to_string)
            {
                ui.label(RichText::new("suggestion ready").size(11.0).color(app.tokens.text_faint));
                if button(app, ui, "story.use_ai_prompt", "Use AI prompt") {
                    d.image_prompt = text;
                }
            }
        });
        let mut motion_prompt = d.motion_prompt.clone();
        field(app, ui, "story.motion", "Motion prompt (English)", &mut motion_prompt);
        d.motion_prompt = motion_prompt;
        let mut duration = d.duration.to_string();
        field(app, ui, "story.duration", "Seconds", &mut duration);
        d.duration = duration.parse::<f64>().unwrap_or(5.0).clamp(1.0, 120.0);
        ui.horizontal(|ui| {
            if button(app, ui, "story.save_scene", "Save scene") {
                actions.push((
                    "nl.story.update",
                    json!({"scene": scene_id, "fields": {
                        "heading": d.heading.trim(), "action": d.action.trim(), "dialogue": d.dialogue.trim(),
                        "imagePrompt": d.image_prompt.trim(), "motionPrompt": d.motion_prompt.trim(), "durationSec": d.duration,
                    }}),
                ));
            }
            if button(app, ui, "story.generate", "Generate keyframe") {
                actions.push(("nl.story.generate", json!({"scene": scene_id})));
            }
            if button(app, ui, "story.place", "Place on timeline") {
                actions.push(("nl.story.place", json!({"scene": scene_id})));
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut d.use_sheet, RichText::new("Use the character sheet as second reference").size(11.0));
            if button(app, ui, "story.generate_video", "Generate video from keyframe") {
                actions.push(("nl.story.generate", json!({"scene": scene_id, "kind": "video", "useSheet": d.use_sheet})));
            }
            if button(app, ui, "story.shotmap_scene", "Shot map for this scene") {
                actions.push(("nl.story.shotmap", json!({"scene": scene_id})));
            }
            let previous = d.scene.checked_sub(1).and_then(|index| scenes.get(index)).and_then(|scene| scene["id"].as_u64());
            if let Some(previous) = previous
                && button(app, ui, "story.extend_video", "Extend video from previous scene")
            {
                actions.push(("nl.story.generate", json!({"scene": scene_id, "kind": "video", "extendFromScene": previous, "useSheet": d.use_sheet})));
            }
        });
        ui.horizontal(|ui| {
            if button(app, ui, "story.extend_audio", "Generate music for this scene") {
                actions.push(("nl.story.extendAudio", json!({"scene": scene_id})));
            }
            if scene["audio"]["item"].as_u64().is_some() && button(app, ui, "story.place_audio", "Place music on the timeline") {
                actions.push(("nl.story.placeAudio", json!({"scene": scene_id})));
            }
        });
        let video_workflow = app.session.prefs.nl.comfy_video_workflow.clone();
        ui.label(
            RichText::new(if video_workflow.trim().is_empty() {
                "Video workflow: set one in Settings ▸ NL Video Suite, or a video generation is refused with the reason.".to_string()
            } else {
                format!("Video workflow: {video_workflow}")
            })
            .size(11.0)
            .color(app.tokens.text_faint),
        );
        let mut instructions = d.instructions.clone();
        ui.label(RichText::new("Refine with AI (what should change?)").size(12.0).color(app.tokens.text_dim));
        let r = ui.add(egui::TextEdit::singleline(&mut instructions).desired_width(f32::INFINITY).id_salt("nl-story-refine"));
        app.auto.add("nl.story.refine", r.rect, "Refine instructions");
        d.instructions = instructions;
        if button(app, ui, "story.refine_run", "Refine this scene") {
            if d.instructions.trim().is_empty() {
                app.ui.status = "Write what should change in the scene.".into();
            } else {
                actions.push(("nl.story.refine", json!({"scene": scene_id, "instructions": d.instructions.trim()})));
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if button(app, ui, "story.place_all", "Place all generated scenes on the timeline") {
                actions.push(("nl.story.placeAll", json!({})));
            }
            if button(app, ui, "story.shotmap_all", "Shot map (all scenes)") {
                actions.push(("nl.story.shotmap", json!({})));
            }
        });
    }
    for (cmd, params) in actions {
        match app.session.execute(cmd, params) {
            Ok(_) => {
                ui.data_mut(|d| d.insert_temp(cache.with("at"), 0.0f64));
                d.loaded_scene = -1;
            }
            Err(e) => app.ui.status = e.to_string(),
        }
    }
    save_draft(ui, "nl-story-draft", d);
}

/// NL Analyze: chapters of a long recording read from its ASR transcript by the local model, with
/// one click to turn every chapter into a timeline marker.
#[derive(Clone)]
struct SegmentDraft {
    server: String,
    model: String,
    mmproj: String,
    start: String,
    end: String,
    prompt: String,
}

impl Default for SegmentDraft {
    fn default() -> Self {
        Self {
            server: r"K:\AI\LlamaCPP\llama-server.exe".into(),
            model: r"K:\AI\LlamaCPP\Modello\LFM3\LFM3-Omni-2B-Q8_0.gguf".into(),
            mmproj: r"K:\AI\LlamaCPP\Modello\LFM3\mmproj-LFM3-Omni-2B-Q8_0.gguf".into(),
            start: "0".into(),
            end: "5".into(),
            prompt: String::new(),
        }
    }
}

fn segment_section(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    let mut d: SegmentDraft = draft(ui, "nl-segment-draft");
    egui::CollapsingHeader::new("Analisi rapida segmento (immagini + audio, llama-server)").default_open(true).show(ui, |ui| {
        if button(app, ui, "segment.run", "Analizza segmento") {
            let start: f64 = d.start.trim().parse().unwrap_or(0.0);
            let end: f64 = d.end.trim().parse().unwrap_or(start + 5.0);
            let ctx = ui.ctx().clone();
            let value = crate::menus::invoke(
                app,
                &ctx,
                "nl.analyze.segment",
                json!({"startSeconds": start, "endSeconds": end, "prompt": d.prompt, "llamaServer": d.server, "model": d.model, "mmproj": d.mmproj}),
            );
            match value {
                Ok(result) => {
                    ui.data_mut(|data| data.insert_temp(egui::Id::unique("nl-segment-result"), result));
                }
                Err(e) => app.ui.status = e,
            }
        }

        if let Some(result) = ui.data(|data| data.get_temp::<Value>(egui::Id::unique("nl-segment-result"))) {
            ui.add_space(4.0);
            ui.label(RichText::new(format!("Segmento {:.1}–{:.1}s · {}", result["segment"]["startSeconds"].as_f64().unwrap_or(0.0), result["segment"]["endSeconds"].as_f64().unwrap_or(0.0), result["model"].as_str().unwrap_or(""))).size(10.0).color(app.tokens.text_faint));
            let mut text = result["text"].as_str().unwrap_or("").to_string();
            ui.add(egui::TextEdit::multiline(&mut text).desired_rows(6).desired_width(f32::INFINITY).interactive(false));
        }

        let mut start = d.start.clone();
        field(app, ui, "segment.start", "Da secondi", &mut start);
        d.start = start;
        let mut end = d.end.clone();
        field(app, ui, "segment.end", "A secondi", &mut end);
        d.end = end;
        let mut server = d.server.clone();
        field(app, ui, "segment.server", "llama-server.exe", &mut server);
        d.server = server;
        let mut model = d.model.clone();
        field(app, ui, "segment.model", "Modello GGUF", &mut model);
        d.model = model;
        let mut mmproj = d.mmproj.clone();
        field(app, ui, "segment.mmproj", "mmproj GGUF", &mut mmproj);
        d.mmproj = mmproj;
        let mut prompt = d.prompt.clone();
        field(app, ui, "segment.prompt", "Prompt (opzionale)", &mut prompt);
        d.prompt = prompt;
    });
    save_draft(ui, "nl-segment-draft", d);
}

fn analyze(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Analyze",
        "Chapters for a long recording, read from the ASR transcript with the local model; each chapter can become a timeline marker.",
    );
    error_line(app, ui);
    ui.horizontal(|ui| {
        if button(app, ui, "analyze.upscale", "Upscale video selezionato (RTX 3×)") {
            let clip = app.session.state.selection.first().map(|clip| clip.0);
            match clip {
                Some(clip) => run(app, ui, "graphics.upscaleVideo", json!({"clip": clip, "multiplier": 3.0, "quality": "ULTRA"})),
                None => app.ui.status = "Seleziona la clip generata da upscalare.".into(),
            }
        }
        ui.label(RichText::new("Sostituisce il footage nella clip, senza creare un secondo item.").size(10.0).color(app.tokens.text_faint));
    });
    segment_section(app, ui);
    if button(app, ui, "analyze.run", "Analyze chapters") {
        run(app, ui, "nl.longform.analyze", json!({"wait": false}));
    }
    job_line(app, ui, "NL longform");
    let longform = app.session.nl.result("longform").cloned().or_else(|| app.session.nl.longform.clone());
    let Some(longform) = longform.filter(|value| !value.is_null()) else {
        ui.add_space(6.0);
        ui.label(RichText::new("No chapters yet: transcribe the sequence first (NL ASR), then analyze it here.").size(12.0).color(app.tokens.text_faint));
        return;
    };
    let chapters = longform["chapters"].as_array().cloned().unwrap_or_default();
    let duration = longform["durationSec"].as_f64().unwrap_or(0.0);
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("{} chapter(s) from {} block(s) · media {:.1} s", chapters.len(), longform["blocks"].as_u64().unwrap_or(0), duration))
            .size(11.0)
            .color(app.tokens.text_dim),
    );
    egui::ScrollArea::vertical().id_salt("nl-analyze-chapters").max_height(260.0).show(ui, |ui| {
        for chapter in &chapters {
            let start = chapter["startSec"].as_f64().unwrap_or(0.0);
            let end = chapter["endSec"].as_f64().unwrap_or(start);
            let time = format!("{:02}:{:04.1}–{:02}:{:04.1}", (start / 60.0) as u64, start % 60.0, (end / 60.0) as u64, end % 60.0);
            ui.label(RichText::new(format!("{time}  {}", chapter["title"].as_str().unwrap_or(""))).size(12.0).strong());
            if let Some(summary) = chapter["summary"].as_str().filter(|summary| !summary.is_empty()) {
                ui.label(RichText::new(summary).size(11.0).color(app.tokens.text_dim));
            }
        }
    });
    ui.add_space(6.0);
    if button(app, ui, "analyze.markers", "Add chapter markers to the timeline") {
        run(app, ui, "nl.longform.markers", json!({}));
    }
    if button(app, ui, "analyze.export", "Export chapters…") {
        app.pick_ui(
            crate::native_dialogs::FileDialog::SaveAs {
                filter: "Chapter list".into(),
                exts: vec!["json".into(), "md".into(), "srt".into()],
                name: "chapters.json".into(),
                adds_ext: true,
            },
            |app, paths| {
                let Some(path) = paths.first() else { return };
                if let Err(e) = app.session.execute("nl.longform.export", json!({"path": path})) {
                    app.ui.status = e.to_string();
                }
            },
        );
    }
    ui.add_space(8.0);
    ui.separator();
    // Project search: one index over clips, markers, captions, the story and the transcripts.
    ui.label(RichText::new("Search the project (clips, markers, captions, story, transcripts)").size(12.0).color(app.tokens.text_dim));
    let query_id = egui::Id::unique("nl-search-query");
    let mut query = ui.data(|data| data.get_temp::<String>(query_id)).unwrap_or_default();
    let r = ui.add(egui::TextEdit::singleline(&mut query).desired_width(f32::INFINITY).hint_text("what are you looking for?").id_salt("nl-search-query"));
    app.auto.add("nl.search.query", r.rect, "Search the project");
    let submitted = r.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    ui.data_mut(|data| data.insert_temp(query_id, query.clone()));
    let result_id = egui::Id::unique("nl-search-result");
    ui.horizontal(|ui| {
        if button(app, ui, "search.run", "Search") || submitted {
            if query.trim().is_empty() {
                app.ui.status = "Write what to search for.".into();
            } else {
                let ctx = ui.ctx().clone();
                match crate::menus::invoke(app, &ctx, "nl.search.query", json!({"query": query.trim(), "limit": 20})) {
                    Ok(value) => {
                        ui.data_mut(|data| {
                            data.insert_temp(result_id, value);
                        });
                    }
                    Err(e) => app.ui.status = e,
                }
            }
        }
        if button(app, ui, "search.rebuild", "Rebuild index") {
            let ctx = ui.ctx().clone();
            match crate::menus::invoke(app, &ctx, "nl.search.index", json!({})) {
                Ok(value) => {
                    ui.data_mut(|data| {
                        data.insert_temp(result_id, value);
                    });
                }
                Err(e) => app.ui.status = e,
            }
        }
    });
    ui.add_space(8.0);
    ui.separator();
    ui.label(RichText::new("Translate the captions with the local model").size(12.0).color(app.tokens.text_dim));
    let languages = [("en", "English"), ("es", "Spanish")];
    let lang_id = egui::Id::unique("nl-translate-language");
    let mut language = ui.data(|data| data.get_temp::<usize>(lang_id)).unwrap_or(0).min(languages.len() - 1);
    ui.horizontal(|ui| {
        for (index, (_, label)) in languages.iter().enumerate() {
            ui.selectable_value(&mut language, index, *label);
        }
        if button(app, ui, "translate.captions", "Translate caption track") {
            run(app, ui, "nl.translate.captions", json!({"to": languages[language].0}));
        }
    });
    ui.data_mut(|data| data.insert_temp(lang_id, language));
    ui.add_space(8.0);
    ui.separator();
    // Ask the project: the answer cites the documents it used.
    ui.label(RichText::new("Ask the project (answers only from the indexed documents)").size(12.0).color(app.tokens.text_dim));
    let question_id = egui::Id::unique("nl-assistant-question");
    let mut question = ui.data(|data| data.get_temp::<String>(question_id)).unwrap_or_default();
    let r = ui
        .add(egui::TextEdit::singleline(&mut question).desired_width(f32::INFINITY).hint_text("a question about the project").id_salt("nl-assistant-question"));
    app.auto.add("nl.assistant.question", r.rect, "Ask the project");
    let ask_submitted = r.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
    ui.data_mut(|data| data.insert_temp(question_id, question.clone()));
    if button(app, ui, "assistant.ask", "Ask the project") || ask_submitted {
        if question.trim().is_empty() {
            app.ui.status = "Write a question first.".into();
        } else {
            run(app, ui, "nl.assistant.ask", json!({"question": question.trim()}));
        }
    }
    job_line(app, ui, "NL assistant");
    if let Some(answer) = app.session.nl.result("assistant").cloned() {
        ui.label(RichText::new("Answer").size(12.0).strong());
        ui.label(RichText::new(answer["answer"].as_str().unwrap_or("")).size(12.0));
        if let Some(sources) = answer["sources"].as_array() {
            for source in sources {
                let time = source["timeSec"].as_f64().map(|seconds| format!("{:02}:{:04.1}  ", (seconds / 60.0) as u64, seconds % 60.0)).unwrap_or_default();
                ui.label(
                    RichText::new(format!(
                        "[{}] {time}[{}] {} — {}",
                        source["n"].as_u64().unwrap_or(0),
                        source["source"].as_str().unwrap_or(""),
                        source["title"].as_str().unwrap_or(""),
                        source["snippet"].as_str().unwrap_or("")
                    ))
                    .size(10.0)
                    .color(app.tokens.text_faint),
                );
            }
        }
    }
    if let Some(result) = ui.data(|data| data.get_temp::<Value>(result_id)) {
        if let Some(results) = result["results"].as_array() {
            ui.label(
                RichText::new(format!("{} hit(s) of {} document(s)", result["total"].as_u64().unwrap_or(0), result["documents"].as_u64().unwrap_or(0)))
                    .size(11.0)
                    .color(app.tokens.text_faint),
            );
            egui::ScrollArea::vertical().id_salt("nl-search-results").max_height(220.0).show(ui, |ui| {
                for hit in results {
                    let time = hit["timeSec"].as_f64().map(|seconds| format!("{:02}:{:04.1}  ", (seconds / 60.0) as u64, seconds % 60.0)).unwrap_or_default();
                    ui.label(
                        RichText::new(format!("{time}[{}] {}", hit["source"].as_str().unwrap_or(""), hit["title"].as_str().unwrap_or(""))).size(11.0).strong(),
                    );
                    ui.label(RichText::new(hit["snippet"].as_str().unwrap_or("")).size(11.0).color(app.tokens.text_dim));
                }
            });
        } else {
            ui.label(
                RichText::new(format!("Index: {} document(s) — {}", result["documents"].as_u64().unwrap_or(0), result["sources"]))
                    .size(11.0)
                    .color(app.tokens.text_faint),
            );
        }
    }
}

// ---------------------------------------------------------------- variation queue

#[derive(Clone, Default)]
struct QueueDraft {
    prompt: String,
    kind: usize,
    count: String,
    seed: String,
    overrides: String,
    confirm_send: bool,
    confirm_free: bool,
}

/// Variants with deterministic seeds: prepare shows the payload, send waits for a second click.

/// Work batch: enqueue internal NL/ComfyUI work while editing, then start it once. Kept separate
/// from the variation queue, which remains below untouched.
fn batch_section(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    ui.label(RichText::new("Coda lavori (batch)").size(13.0).strong());
    ui.label(RichText::new("Accoda le operazioni, continua a montare, poi avvia tutto una volta. ComfyUI gestisce la propria coda.").size(10.0).color(app.tokens.text_faint));
    let selected = app.session.state.selection.first().map(|clip| clip.0);
    let range = selected.and_then(|clip| {
        let sequence = app.session.active_sequence()?;
        sequence
            .video_tracks
            .iter()
            .flat_map(|track| track.items.iter())
            .find(|item| item.id.0 == clip)
            .map(|item| (item.start.0 as f64 / 254_016_000_000.0, (item.start.0 + item.duration.0) as f64 / 254_016_000_000.0))
    });
    ui.horizontal(|ui| {
        if button(app, ui, "batch.add_upscale", "Accoda upscale clip") {
            match selected {
                Some(clip) => run(app, ui, "nl.batch.add", json!({"command": "graphics.upscaleVideo", "label": "Upscale clip", "params": {"clip": clip, "multiplier": 3.0, "quality": "ULTRA"}})),
                None => app.ui.status = "Seleziona una clip da upscalare.".into(),
            }
        }
        if button(app, ui, "batch.add_segment", "Accoda analisi segmento") {
            match range {
                Some((start, end)) => run(app, ui, "nl.batch.add", json!({"command": "nl.analyze.segment", "label": "Analisi segmento", "params": {"startSeconds": start, "endSeconds": end}})),
                None => app.ui.status = "Seleziona una clip per il segmento.".into(),
            }
        }
        if button(app, ui, "batch.clear", "Svuota coda") {
            run(app, ui, "nl.batch.clear", json!({}));
        }
        if button(app, ui, "batch.start", "Avvia coda") {
            run(app, ui, "nl.batch.start", json!({}));
        }
    });
    let entries = app.session.nl.batch.clone();
    if entries.is_empty() {
        ui.label(RichText::new("Coda vuota.").size(11.0).color(app.tokens.text_faint));
    } else {
        egui::ScrollArea::vertical().id_salt("nl-batch-entries").max_height(120.0).show(ui, |ui| {
            for entry in &entries {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} · {}", entry["id"].as_u64().unwrap_or(0), entry["label"].as_str().unwrap_or(""))).size(11.0));
                    ui.label(RichText::new(entry["status"].as_str().unwrap_or("")).size(10.0).color(app.tokens.text_faint));
                    if entry["status"] == "pending" && ui.small_button("Rimuovi").clicked() {
                        run(app, ui, "nl.batch.remove", json!({"id": entry["id"]}));
                    }
                });
            }
        });
    }
    ui.separator();
}

fn queue_variants(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    batch_section(app, ui);
    let mut d: QueueDraft = draft(ui, "nl-queue-draft");
    ui.label(RichText::new("Variants").size(13.0).strong());
    let mut prompt = d.prompt.clone();
    ui.label(RichText::new("Prompt").size(12.0).color(app.tokens.text_dim));
    let r = ui.add(egui::TextEdit::multiline(&mut prompt).desired_rows(2).desired_width(f32::INFINITY).id_salt("nl-queue-prompt"));
    app.auto.add("nl.queue.prompt", r.rect, "Variants prompt");
    d.prompt = prompt;
    let kinds = [("image", "Image"), ("video", "Video"), ("audio", "Audio")];
    let mut kind = kinds[d.kind.min(kinds.len() - 1)].0.to_string();
    let label = kinds[d.kind.min(kinds.len() - 1)].1;
    craft_ui::ComboBox::from_id_salt("nl-queue-kind").selected_text(label).show_ui(ui, |ui| {
        for (index, (value, label)) in kinds.iter().enumerate() {
            if ui.selectable_value(&mut d.kind, index, *label).clicked() {
                kind = value.to_string();
            }
        }
    });
    let mut count = d.count.clone();
    let mut seed = d.seed.clone();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Variants").size(12.0));
        let r = ui.add(egui::TextEdit::singleline(&mut count).desired_width(48.0).id_salt("nl-queue-count"));
        app.auto.add("nl.queue.count", r.rect, "Variants");
        ui.label(RichText::new("First seed (0 = random)").size(12.0));
        let r = ui.add(egui::TextEdit::singleline(&mut seed).desired_width(96.0).id_salt("nl-queue-seed"));
        app.auto.add("nl.queue.seed", r.rect, "First seed");
    });
    d.count = count;
    d.seed = seed;
    let count_value: u64 = d.count.trim().parse().unwrap_or(1).clamp(1, 32);
    let seed_value: u64 = d.seed.trim().parse().unwrap_or(0);
    let mut overrides = d.overrides.clone();
    field(app, ui, "queue.overrides", "Widget overrides JSON (optional)", &mut overrides);
    d.overrides = overrides;
    let parsed_overrides =
        if d.overrides.trim().is_empty() { Some(Value::Null) } else { serde_json::from_str::<Value>(&d.overrides).ok().filter(Value::is_array) };
    if button(app, ui, "queue.add", "Add variants") {
        if d.prompt.trim().is_empty() {
            app.ui.status = "Write a prompt for the variants.".into();
        } else if let Some(overrides) = parsed_overrides {
            run(app, ui, "nl.queue.add", json!({"prompt": d.prompt.trim(), "kind": kind, "count": count_value, "seed": seed_value, "overrides": overrides}));
        } else {
            app.ui.status = "Widget overrides must be a JSON array like [{\"node\":\"1\",\"field\":\"steps\",\"value\":8}]".into();
        }
    }
    let entries = app.session.nl.queue.clone();
    if entries.is_empty() {
        ui.label(RichText::new("The queue is empty: add variants to prepare them.").size(11.0).color(app.tokens.text_faint));
    } else {
        let prepared = entries.iter().filter(|entry| entry["status"].as_str() == Some("prepared")).count();
        ui.label(RichText::new(format!("{} entr(ies), {prepared} prepared", entries.len())).size(11.0).color(app.tokens.text_dim));
        egui::ScrollArea::vertical().id_salt("nl-queue-entries").max_height(170.0).show(ui, |ui| {
            for entry in &entries {
                let status = entry["status"].as_str().unwrap_or("new");
                let mark = match status {
                    "prepared" => "◔",
                    "sent" => "✓",
                    "error" => "!",
                    _ => "·",
                };
                let workflow = entry["workflow"]
                    .as_str()
                    .filter(|workflow| !workflow.is_empty())
                    .map(|workflow| workflow.rsplit(['/', '\\']).next().unwrap_or(workflow))
                    .unwrap_or("default workflow");
                let size = entry["prepared"]["size"]
                    .as_object()
                    .map(|size| {
                        format!(" · {}×{}", size.get("width").and_then(Value::as_u64).unwrap_or(0), size.get("height").and_then(Value::as_u64).unwrap_or(0))
                    })
                    .unwrap_or_default();
                ui.label(
                    RichText::new(format!(
                        "{mark} #{} seed {} · {status} · {workflow}{size}",
                        entry["id"].as_u64().unwrap_or(0),
                        entry["seed"].as_u64().unwrap_or(0),
                    ))
                    .size(11.0)
                    .color(if status == "error" { egui::Color32::from_rgb(233, 150, 120) } else { app.tokens.text_dim }),
                );
                if let Some(error) = entry["error"].as_str() {
                    ui.label(RichText::new(error).size(10.0).color(egui::Color32::from_rgb(233, 150, 120)));
                }
            }
        });
        ui.horizontal(|ui| {
            if button(app, ui, "queue.prepare", "Prepare variants") {
                run(app, ui, "nl.queue.prepare", json!({}));
            }
            if button(app, ui, "queue.remove", "Remove finished") {
                run(app, ui, "nl.queue.remove", json!({"finished": true}));
            }
        });
        if d.confirm_send {
            ui.label(
                RichText::new(format!("Sending {prepared} prepared variant(s) to ComfyUI: confirm?")).size(12.0).color(egui::Color32::from_rgb(233, 180, 120)),
            );
            ui.horizontal(|ui| {
                if button(app, ui, "queue.send_confirm", "Confirm send") {
                    run(app, ui, "nl.queue.send", json!({"confirm": true}));
                    d.confirm_send = false;
                }
                if button(app, ui, "queue.send_cancel", "Cancel") {
                    d.confirm_send = false;
                }
            });
        } else {
            if button(app, ui, "queue.send", "Send prepared…") {
                if prepared == 0 {
                    app.ui.status = "Prepare the variants first: nothing is sent unprepared.".into();
                } else {
                    d.confirm_send = true;
                }
            }
        }
    }
    if d.confirm_free {
        ui.label(
            RichText::new("Free ComfyUI's VRAM now? It unloads its models; queued generations may reload them.")
                .size(12.0)
                .color(egui::Color32::from_rgb(233, 180, 120)),
        );
        ui.horizontal(|ui| {
            if button(app, ui, "queue.free_confirm", "Confirm free") {
                run(app, ui, "nl.queue.free", json!({"confirm": true}));
                d.confirm_free = false;
            }
            if button(app, ui, "queue.free_cancel", "Cancel") {
                d.confirm_free = false;
            }
        });
    } else if button(app, ui, "queue.free", "Free ComfyUI VRAM…") {
        d.confirm_free = true;
    }
    save_draft(ui, "nl-queue-draft", d);
}

fn queue(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(app, ui, "NL Queue", "Variants prepared and sent on purpose, and the background jobs of the suite.");
    error_line(app, ui);
    queue_variants(app, ui);
    ui.add_space(8.0);
    ui.separator();
    ui.label(RichText::new("Background jobs").size(12.0).color(app.tokens.text_dim));
    let jobs: Vec<Value> = app.session.jobs.iter().rev().map(|j| j.to_json()).collect();
    if jobs.is_empty() {
        ui.label(RichText::new("No background jobs.").size(12.0).color(app.tokens.text_faint));
    }
    let mut cancel: Option<u64> = None;
    for job in &jobs {
        let id = job["id"].as_u64().unwrap_or(0);
        let label = job["label"].as_str().unwrap_or("Job");
        let done = job["done"].as_u64().unwrap_or(0);
        let total = job["total"].as_u64().unwrap_or(0);
        let finished = job["finished"].as_bool().unwrap_or(false);
        let status = job["status"].as_str().unwrap_or("");
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(12.0));
            ui.label(
                RichText::new(if finished {
                    "finished".to_string()
                } else if total > 0 {
                    format!("{done}/{total}")
                } else {
                    "running".to_string()
                })
                .size(12.0)
                .color(app.tokens.text_dim),
            );
            if !status.is_empty() {
                ui.label(RichText::new(status).size(11.0).color(app.tokens.text_faint));
            }
            if !finished && button(app, ui, &format!("queue.cancel.{id}"), "Cancel") {
                cancel = Some(id);
            }
        });
    }
    if let Some(id) = cancel {
        run(app, ui, "jobs.cancel", json!({"job": id}));
    }
    ui.add_space(8.0);
    let mut refresh = false;
    if button(app, ui, "queue.services", "Check services") {
        refresh = true;
    }
    if refresh {
        match app.session.execute("nl.services.status", json!({})) {
            Ok(status) => {
                for (name, value) in [("ComfyUI", &status["comfy"]), ("Ollama", &status["ollama"]), ("Language model", &status["llm"])] {
                    let reachable = value["reachable"].as_bool().unwrap_or(false);
                    let url = value["url"].as_str().unwrap_or("");
                    let text = if reachable { format!("{name}: reachable at {url}") } else { format!("{name}: not reachable at {url}") };
                    let color = if reachable { app.tokens.text_dim } else { egui::Color32::from_rgb(233, 180, 120) };
                    ui.label(RichText::new(text).size(12.0).color(color));
                }
            }
            Err(e) => app.ui.status = e.to_string(),
        }
    }
}

fn settings(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(app, ui, "NL Settings", "The same values as Settings ▸ NL Video Suite: endpoints, installed models, resolution profiles and speech recognition.");
    error_line(app, ui);
    let models_id = egui::Id::unique("nl-ollama-models");
    ui.horizontal(|ui| {
        if button(app, ui, "settings.ollama_fetch", "Carica modelli Ollama") {
            let ctx = ui.ctx().clone();
            match crate::menus::invoke(app, &ctx, "nl.models.ollama", json!({})) {
                Ok(value) => {
                    ui.data_mut(|data| data.insert_temp(models_id, value));
                }
                Err(e) => app.ui.status = e,
            }
        }
        if let Some(value) = ui.data(|data| data.get_temp::<Value>(models_id)) {
            ui.label(
                RichText::new(format!("{} modelli", value["models"].as_array().map_or(0, Vec::len)))
                    .size(11.0)
                    .color(app.tokens.text_faint),
            );
        }
    });
    if let Some(value) = ui.data(|data| data.get_temp::<Value>(models_id)) {
        let options: Vec<(String, String)> = value["models"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| row["name"].as_str().map(|name| (name.to_string(), name.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        if !options.is_empty() {
            let mut prompt_index = options.iter().position(|(name, _)| name == &app.session.prefs.nl.ollama_model).or_else(|| options.iter().position(|(name, _)| name.starts_with("lfm3-omni"))).unwrap_or(0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Modello prompt").size(11.0).color(app.tokens.text_dim));
                let before = prompt_index;
                combo(app, ui, "settings.ollama_prompt_model", "Modello prompt", &mut prompt_index, &options);
                if prompt_index != before {
                    let name = options[prompt_index].0.clone();
                    if let Err(e) = app.session.execute("prefs.set", json!({"key": "nl.ollamaModel", "value": name})) {
                        app.ui.status = e.to_string();
                    }
                }
            });
            let mut vision_index = options.iter().position(|(name, _)| name == &app.session.prefs.nl.ollama_model_vision).or_else(|| options.iter().position(|(name, _)| name.starts_with("gemma4"))).unwrap_or(0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Modello vision").size(11.0).color(app.tokens.text_dim));
                let before = vision_index;
                combo(app, ui, "settings.ollama_vision_model", "Modello vision", &mut vision_index, &options);
                if vision_index != before {
                    let name = options[vision_index].0.clone();
                    if let Err(e) = app.session.execute("prefs.set", json!({"key": "nl.ollamaModelVision", "value": name})) {
                        app.ui.status = e.to_string();
                    }
                }
            });
            for (label, key, current, pref) in [
                ("Agente storia", "settings.agent_story_model", app.session.prefs.nl.agent_story_model.clone(), "nl.agentStoryModel"),
                ("Agente prompt", "settings.agent_prompt_model", app.session.prefs.nl.agent_prompt_model.clone(), "nl.agentPromptModel"),
                ("Agente vision", "settings.agent_vision_model", app.session.prefs.nl.agent_vision_model.clone(), "nl.agentVisionModel"),
            ] {
                let mut index = options.iter().position(|(name, _)| name == &current).unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).size(11.0).color(app.tokens.text_dim));
                    let before = index;
                    combo(app, ui, key, label, &mut index, &options);
                    if index != before {
                        let name = options[index].0.clone();
                        if let Err(e) = app.session.execute("prefs.set", json!({"key": pref, "value": name})) {
                            app.ui.status = e.to_string();
                        }
                    }
                });
            }
        }
    }


    let nl = app.session.prefs.nl.clone();
    let rows = [
        ("ComfyUI", nl.comfy_url()),
        ("Language model", format!("{} · {} · {}", nl.llm_provider(), nl.llm_url(), nl.llm_model())),
        ("Workflow folder", nl.comfy_workflow_dir.clone()),
        ("Images", format!("{} (round to {} px)", nl.image_scale, nl.size_multiple)),
        ("Video", format!("{} of the sequence", nl.video_scale)),
        ("Speech", format!("{} · {} · {} · cache {}", nl.asr_provider(), nl.asr_device(), nl.asr_language(), if nl.asr_cache { "on" } else { "off" })),
        ("Queue timeout", format!("{} s", nl.queue_timeout_sec)),
    ];
    for (label, value) in rows {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(12.0).color(app.tokens.text_dim));
            ui.label(RichText::new(value).size(12.0));
        });
    }
    ui.add_space(8.0);
    ui.label(RichText::new("Speech models found locally").size(12.0).color(app.tokens.text_dim));
    let scan_id = egui::Id::unique("nl-asr-scan");
    ui.horizontal(|ui| {
        if button(app, ui, "settings.asr_scan", "Scan ASR models") {
            let ctx = ui.ctx().clone();
            match crate::menus::invoke(app, &ctx, "nl.asr.models", json!({})) {
                Ok(value) => {
                    ui.data_mut(|data| data.insert_temp(scan_id, value));
                }
                Err(e) => app.ui.status = e,
            }
        }
        if let Some(scan) = ui.data(|data| data.get_temp::<Value>(scan_id)) {
            ui.label(
                RichText::new(format!(
                    "{} verified model(s), {} engine(s)",
                    scan["count"].as_u64().unwrap_or(0),
                    scan["engines"].as_array().map_or(0, Vec::len)
                ))
                .size(11.0)
                .color(app.tokens.text_faint),
            );
            let models = scan["models"].as_array().cloned().unwrap_or_default();
            let engines = scan["engines"].as_array().cloned().unwrap_or_default();
            let qwen_model = models.iter().find(|m| m["kind"] == "qwen-gguf" && m["role"] == "asr").cloned();
            let qwen_aligner = models.iter().find(|m| m["kind"] == "qwen-gguf" && m["role"] == "aligner").cloned();
            let qwen_runner = engines.iter().find(|e| e["engine"] == "qwen3-asr").cloned();
            if let (Some(model), Some(aligner), Some(runner)) = (qwen_model, qwen_aligner, qwen_runner)
                && button(app, ui, "settings.asr_use_qwen", "Use Qwen3-ASR")
            {
                let mut set = |key: &str, value: &str| {
                    if let Err(e) = app.session.execute("prefs.set", json!({"key": key, "value": value})) {
                        app.ui.status = e.to_string();
                    }
                };
                set("nl.asrProvider", "qwen_gguf");
                set("nl.asrRunner", runner["path"].as_str().unwrap_or(""));
                set("nl.asrQwenModel", model["path"].as_str().unwrap_or(""));
                set("nl.asrQwenAligner", aligner["path"].as_str().unwrap_or(""));
                app.ui.status = "ASR provider set to Qwen3-ASR (local GGUF).".into();
            }
            if let Some(whisper) = models.iter().find(|m| m["kind"] == "whisper-ct2").cloned()
                && button(app, ui, "settings.asr_use_whisper", "Use Whisper (bridge)")
            {
                if let Err(e) = app.session.execute("prefs.set", json!({"key": "nl.asrProvider", "value": "bridge"})) {
                    app.ui.status = e.to_string();
                }
                if let Err(e) = app.session.execute("prefs.set", json!({"key": "nl.asrModel", "value": whisper["path"].as_str().unwrap_or("")})) {
                    app.ui.status = e.to_string();
                }
                app.ui.status = "ASR provider set to the local Whisper bridge model.".into();
            }
        }
    });
    if let Some(scan) = ui.data(|data| data.get_temp::<Value>(scan_id)) {
        for model in scan["models"].as_array().into_iter().flatten().take(6) {
            ui.label(
                RichText::new(format!(
                    "{} · {} · verified {} · {}",
                    model["kind"].as_str().unwrap_or(""),
                    model["role"].as_str().unwrap_or(""),
                    model["verified"],
                    model["path"].as_str().unwrap_or("")
                ))
                .size(10.0)
                .color(app.tokens.text_faint),
            );
        }
    }
    if button(app, ui, "settings.open", "Open NL settings…") {
        run(app, ui, "app.settings.nl", json!({}));
    }
    ui.add_space(10.0);
    ui.label(
        RichText::new("Nothing is downloaded automatically: the suite uses the ComfyUI endpoint and the language-model provider you already run (Ollama, or any OpenAI-compatible server such as llama.cpp/LM Studio) and the models they already have.")
            .size(12.0)
            .color(app.tokens.text_dim),
    );
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::dock::NL_PANELS;

    /// Every NL panel in the layout has a body: a new panel cannot be added to `NL_PANELS` and
    /// silently render an empty panel.
    #[test]
    fn every_nl_panel_has_a_body() {
        for panel in NL_PANELS {
            assert!(body(panel).is_some(), "{} has no NL body", panel.title());
        }
        assert_eq!(NL_PANELS.len(), 30);
    }

    /// NL Home is the dashboard with all the functions: every NL panel except Home itself appears
    /// once, and no panel is listed twice.
    #[test]
    fn home_links_every_nl_panel_once() {
        let mut seen = HashSet::new();
        for (_, panels) in home_groups() {
            for panel in panels {
                assert!(seen.insert(*panel), "{} is listed twice in NL Home", panel.title());
            }
        }
        for panel in NL_PANELS {
            if panel != PanelKind::NlHome {
                assert!(seen.contains(&panel), "{} is missing from NL Home", panel.title());
            }
        }
        assert_eq!(seen.len(), NL_PANELS.len() - 1);
    }
}

/// Every command a panel says it uses exists in the engine registry, and every NL panel has a
/// contract entry. This is the static half of "all panels connected to commands".
#[test]
fn panel_contract_commands_are_registered() {
    use std::collections::HashSet;

    use crate::dock::NL_PANELS;

    let ids: HashSet<&str> = filmcraft_engine::commands::command_specs().iter().map(|spec| spec.id).collect();
    let mut panels = HashSet::new();
    for (panel, commands) in PANEL_COMMANDS {
        assert!(panels.insert(*panel), "{} has two contract rows", panel.title());
        for id in *commands {
            assert!(ids.contains(id), "{} points at an unknown command `{id}`", panel.title());
        }
    }
    for panel in NL_PANELS {
        assert!(panels.contains(&panel), "{} has no command contract", panel.title());
    }
}
