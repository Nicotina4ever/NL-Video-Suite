//! NL Subtitles and NL Titles: the caption track as a cue list with SRT/VTT import and export, and
//! the built-in motion graphics templates with their live-updatable controls.
//!
//! Every button calls a registered engine command (`captions.*`, `graphics.template.*`), so the
//! same operations are available from the CLI, the control channel and agents.

use egui::RichText;
use serde_json::{Value, json};

use super::nl::{button, draft, error_line, field, run, save_draft, title};
use crate::FilmcraftApp;
use crate::dock::PanelKind;

#[derive(Clone, Default)]
struct SubtitleDraft {
    track: usize,
    cue: usize,
    text: String,
    loaded: i64,
}

/// NL Subtitles: pick a caption track and a cue, edit its text, split at the playhead, merge with
/// the next cue or delete it; import and export SRT/VTT with the native dialogs.
pub(super) fn subtitles(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Subtitles",
        "Edit the caption track: pick a cue, change its text, split at the playhead or merge with the next one, and import or export SRT/VTT.",
    );
    error_line(app, ui);
    ui.horizontal(|ui| {
        if button(app, ui, "subtitles.import", "Import SRT/VTT…") {
            app.pick_ui(
                crate::native_dialogs::FileDialog::OpenFile { filter: "Subtitle files".into(), exts: vec!["srt".into(), "vtt".into(), "webvtt".into()] },
                |app, paths| {
                    let Some(path) = paths.first() else { return };
                    if let Err(e) = app.session.execute("captions.import", json!({"path": path})) {
                        app.ui.status = e.to_string();
                    }
                },
            );
        }
        if button(app, ui, "subtitles.export", "Export captions…") {
            app.pick_ui(
                crate::native_dialogs::FileDialog::SaveAs {
                    filter: "Subtitle files".into(),
                    exts: vec!["srt".into(), "vtt".into()],
                    name: "captions.srt".into(),
                    adds_ext: true,
                },
                |app, paths| {
                    let Some(path) = paths.first() else { return };
                    if let Err(e) = app.session.execute("captions.export", json!({"path": path})) {
                        app.ui.status = e.to_string();
                    }
                },
            );
        }
    });
    let now = ui.input(|input| input.time);
    let cache = egui::Id::unique("nl-subtitles-cache");
    let stale = ui.data(|data| data.get_temp::<f64>(cache.with("at")).is_none_or(|at| now - at > 1.5));
    if stale || ui.data(|data| data.get_temp::<Value>(cache).is_none()) {
        match app.session.execute("captions.list", json!({})) {
            Ok(value) => {
                ui.data_mut(|data| {
                    data.insert_temp(cache, value);
                    data.insert_temp(cache.with("at"), now);
                });
            }
            Err(e) => app.ui.status = e.to_string(),
        }
    }
    let data = ui.data(|data| data.get_temp::<Value>(cache)).unwrap_or(Value::Null);
    let tracks = data["tracks"].as_array().cloned().unwrap_or_default();
    if tracks.is_empty() {
        ui.label(RichText::new("No caption track: import an SRT or VTT file, or add captions from the Captions menu.").size(12.0).color(app.tokens.text_faint));
        return;
    }
    let mut d: SubtitleDraft = draft(ui, "nl-subtitles-draft");
    if d.track >= tracks.len() {
        d.track = 0;
    }
    let names: Vec<String> =
        tracks.iter().map(|track| format!("{} · {}", track["name"].as_str().unwrap_or("Captions"), track["label"].as_str().unwrap_or(""))).collect();
    let mut selected_track = d.track;
    craft_ui::ComboBox::from_id_salt("nl-subtitles-track").selected_text(&names[selected_track]).show_ui(ui, |ui| {
        for (index, name) in names.iter().enumerate() {
            ui.selectable_value(&mut selected_track, index, name);
        }
    });
    if selected_track != d.track {
        d.track = selected_track;
        d.cue = 0;
        d.loaded = -1;
    }
    let cues = tracks[d.track]["captions"].as_array().cloned().unwrap_or_default();
    if cues.is_empty() {
        ui.label(RichText::new("This track has no cues yet.").size(12.0).color(app.tokens.text_faint));
        return;
    }
    if d.cue >= cues.len() {
        d.cue = 0;
    }
    egui::ScrollArea::vertical().id_salt("nl-subtitles-cues").max_height(220.0).show(ui, |ui| {
        for (index, cue) in cues.iter().enumerate() {
            let label = format!(
                "{} → {}  {}",
                cue["in"].as_str().unwrap_or(""),
                cue["out"].as_str().unwrap_or(""),
                cue["text"].as_str().unwrap_or("").replace('\n', " / ")
            );
            if ui.selectable_label(d.cue == index, RichText::new(label).size(11.0)).clicked() {
                d.cue = index;
            }
        }
    });
    let cue = cues[d.cue].clone();
    let cue_id = cue["id"].as_u64().unwrap_or(0);
    if d.loaded != cue_id as i64 {
        d.text = cue["text"].as_str().unwrap_or("").to_string();
        d.loaded = cue_id as i64;
    }
    ui.add_space(4.0);
    let mut text = d.text.clone();
    let r = ui.add(egui::TextEdit::multiline(&mut text).desired_rows(3).desired_width(f32::INFINITY).id_salt("nl-subtitles-text"));
    app.auto.add("nl.subtitles.text", r.rect, "Caption text");
    d.text = text;
    ui.horizontal(|ui| {
        if button(app, ui, "subtitles.save", "Save text") {
            run(app, ui, "captions.setText", json!({"caption": cue_id, "text": d.text}));
            d.loaded = -1;
        }
        if button(app, ui, "subtitles.split", "Split at playhead") {
            run(app, ui, "captions.split", json!({"caption": cue_id}));
            d.loaded = -1;
        }
        if button(app, ui, "subtitles.merge", "Merge with next") && d.cue + 1 < cues.len() {
            let next = cues[d.cue + 1]["id"].as_u64().unwrap_or(0);
            run(app, ui, "captions.merge", json!({"captions": [cue_id, next]}));
            d.loaded = -1;
        }
        if button(app, ui, "subtitles.delete", "Delete cue") {
            run(app, ui, "captions.delete", json!({"captions": [cue_id]}));
            d.loaded = -1;
        }
    });
    save_draft(ui, "nl-subtitles-draft", d);
}

#[derive(Clone, Default)]
struct TitleDraft {
    template: usize,
    animate: bool,
    intro: String,
    outro: String,
    loaded: i64,
    values: serde_json::Map<String, Value>,
}

fn default_control_value(control: &Value) -> Value {
    match control["kind"].as_str().unwrap_or("text") {
        "color" => json!("#ffffff"),
        "slider" => json!(control["min"].as_f64().unwrap_or(40.0).max(40.0)),
        "checkbox" => json!(true),
        "font" => json!("Inter"),
        "position" => json!([960.0, 540.0]),
        _ => json!(control["name"].as_str().unwrap_or("Title")),
    }
}

/// NL Titles: built-in motion graphics templates with their editable controls, placed at the
/// playhead with an optional responsive intro/outro.
pub(super) fn titles(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(app, ui, "NL Titles", "Motion graphics templates: fill in their controls and place the title on the timeline, with a responsive intro and outro.");
    error_line(app, ui);
    let now = ui.input(|input| input.time);
    let cache = egui::Id::unique("nl-titles-cache");
    let stale = ui.data(|data| data.get_temp::<f64>(cache.with("at")).is_none_or(|at| now - at > 5.0));
    if stale || ui.data(|data| data.get_temp::<Value>(cache).is_none()) {
        match app.session.execute("graphics.template.list", json!({})) {
            Ok(value) => {
                ui.data_mut(|data| {
                    data.insert_temp(cache, value);
                    data.insert_temp(cache.with("at"), now);
                });
            }
            Err(e) => app.ui.status = e.to_string(),
        }
    }
    let templates = ui.data(|data| data.get_temp::<Value>(cache)).unwrap_or(Value::Array(Vec::new()));
    let list = templates.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        ui.label(RichText::new("No graphics templates found.").size(12.0).color(app.tokens.text_faint));
        return;
    }
    let mut d: TitleDraft = draft(ui, "nl-titles-draft");
    if d.template >= list.len() {
        d.template = 0;
        d.loaded = -1;
    }
    let names: Vec<String> =
        list.iter().map(|template| format!("{} · {}", template["name"].as_str().unwrap_or("Template"), template["category"].as_str().unwrap_or(""))).collect();
    let mut selected = d.template;
    craft_ui::ComboBox::from_id_salt("nl-titles-template").selected_text(&names[selected]).show_ui(ui, |ui| {
        for (index, name) in names.iter().enumerate() {
            ui.selectable_value(&mut selected, index, name);
        }
    });
    if selected != d.template {
        d.template = selected;
        d.loaded = -1;
        d.values.clear();
    }
    let template = list[d.template].clone();
    let template_id = template["id"].as_str().unwrap_or_default().to_string();
    if d.loaded != d.template as i64 {
        d.values.clear();
        for control in template["controls"].as_array().into_iter().flatten() {
            let Some(id) = control["id"].as_str() else { continue };
            let value = control.get("value").filter(|value| !value.is_null()).cloned().unwrap_or_else(|| default_control_value(control));
            d.values.insert(id.to_string(), value);
        }
        d.loaded = d.template as i64;
    }
    ui.label(RichText::new(template["description"].as_str().unwrap_or("")).size(11.0).color(app.tokens.text_faint));
    egui::ScrollArea::vertical().id_salt("nl-titles-controls").max_height(230.0).show(ui, |ui| {
        for control in template["controls"].as_array().into_iter().flatten() {
            let Some(id) = control["id"].as_str() else { continue };
            let kind = control["kind"].as_str().unwrap_or("text");
            let name = control["name"].as_str().unwrap_or(id);
            let value = d.values.get(id).cloned().unwrap_or(Value::Null);
            match kind {
                "slider" => {
                    let mut number = value.as_f64().unwrap_or(0.0);
                    let min = control["min"].as_f64().unwrap_or(0.0);
                    let max = control["max"].as_f64().unwrap_or(1000.0);
                    let r = ui.add(egui::Slider::new(&mut number, min..=max).text(name));
                    app.auto.add(&format!("nl.titles.{id}"), r.rect, name);
                    d.values.insert(id.to_string(), json!(number));
                }
                "checkbox" => {
                    let mut checked = value.as_bool().unwrap_or(true);
                    let r = ui.checkbox(&mut checked, RichText::new(name).size(11.0));
                    app.auto.add(&format!("nl.titles.{id}"), r.rect, name);
                    d.values.insert(id.to_string(), json!(checked));
                }
                "position" => {
                    let mut text = value.to_string();
                    if field(app, ui, &format!("titles.{id}"), name, &mut text).changed()
                        && let Ok(parsed) = serde_json::from_str::<Value>(&text)
                    {
                        d.values.insert(id.to_string(), parsed);
                    }
                }
                _ => {
                    let mut text = value.as_str().map(str::to_string).unwrap_or_else(|| value.to_string());
                    if field(app, ui, &format!("titles.{id}"), name, &mut text).changed() {
                        d.values.insert(id.to_string(), json!(text));
                    }
                }
            }
        }
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut d.animate, RichText::new("Responsive intro/outro").size(11.0));
        let mut intro = d.intro.clone();
        field(app, ui, "titles.intro", "Intro s", &mut intro);
        d.intro = intro;
        let mut outro = d.outro.clone();
        field(app, ui, "titles.outro", "Outro s", &mut outro);
        d.outro = outro;
    });
    if button(app, ui, "titles.place", "Place title on the timeline") {
        let ctx = ui.ctx().clone();
        let values = Value::Object(d.values.clone());
        let mut params = json!({"template": template_id, "values": values});
        if d.animate {
            params["introSeconds"] = json!(d.intro.trim().parse::<f64>().unwrap_or(0.5).max(0.0));
            params["outroSeconds"] = json!(d.outro.trim().parse::<f64>().unwrap_or(0.5).max(0.0));
        }
        match crate::menus::invoke(app, &ctx, "nl.titles.add", params) {
            Ok(_) => app.ui.status = "Title placed on the timeline.".into(),
            Err(e) => app.ui.status = e,
        }
    }
    save_draft(ui, "nl-titles-draft", d);
}

// ---------------------------------------------------------------- graphics objects

#[derive(Clone, Default)]
struct GraphicsDraft {
    kind: usize,
    preset: usize,
    text: String,
    fill: String,
    width: String,
    height: String,
    x: String,
    y: String,
    seconds: String,
    font_size: String,
}

const GRAPHIC_KINDS: [&str; 3] = ["Rectangle", "Ellipse", "Text"];
const GRAPHIC_PRESETS: [(&str, usize, &str, &str, &str, &str, &str); 3] = [
    ("Lower bar", 0, "1400", "110", "960", "1000", "#1d4ed8"),
    ("Badge", 1, "220", "220", "1700", "180", "#facc15"),
    ("Text title", 2, "1200", "220", "960", "480", "#ffffff"),
];

fn number(text: &str, fallback: f64) -> f64 {
    text.trim().parse::<f64>().ok().filter(|value| value.is_finite()).unwrap_or(fallback)
}

/// NL Graphics: solid objects and text with a chosen fill colour, placed on the timeline.
pub(super) fn graphics_panel(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(app, ui, "NL Graphics", "Solid objects and text with a colour, size and position, placed on the timeline at the playhead.");
    error_line(app, ui);
    let mut d: GraphicsDraft = draft(ui, "nl-graphics-draft");
    if d.fill.is_empty() {
        d.fill = "#1d4ed8".into();
    }
    if d.width.is_empty() {
        d.width = "1400".into();
        d.height = "110".into();
        d.x = "960".into();
        d.y = "1000".into();
    }
    if d.seconds.is_empty() {
        d.seconds = "5".into();
    }
    if d.font_size.is_empty() {
        d.font_size = "120".into();
    }
    let mut kind = d.kind.min(GRAPHIC_KINDS.len() - 1);
    craft_ui::ComboBox::from_id_salt("nl-graphics-kind").selected_text(GRAPHIC_KINDS[kind]).show_ui(ui, |ui| {
        for (index, name) in GRAPHIC_KINDS.iter().enumerate() {
            ui.selectable_value(&mut kind, index, *name);
        }
    });
    d.kind = kind;
    let mut preset = d.preset;
    craft_ui::ComboBox::from_id_salt("nl-graphics-preset").selected_text(if preset == 0 { "Custom" } else { GRAPHIC_PRESETS[preset - 1].0 }).show_ui(
        ui,
        |ui| {
            ui.selectable_value(&mut preset, 0, "Custom");
            for (index, (name, ..)) in GRAPHIC_PRESETS.iter().enumerate() {
                ui.selectable_value(&mut preset, index + 1, *name);
            }
        },
    );
    if preset != d.preset && preset > 0 {
        let (_, preset_kind, width, height, x, y, fill) = GRAPHIC_PRESETS[preset - 1];
        d.kind = preset_kind;
        d.width = width.into();
        d.height = height.into();
        d.x = x.into();
        d.y = y.into();
        d.fill = fill.into();
    }
    d.preset = preset;
    if d.kind == 2 {
        let mut text = d.text.clone();
        field(app, ui, "graphics.text", "Text", &mut text);
        d.text = text;
        let mut font_size = d.font_size.clone();
        field(app, ui, "graphics.font_size", "Font size", &mut font_size);
        d.font_size = font_size;
    }
    let mut fill = d.fill.clone();
    field(app, ui, "graphics.fill", "Fill colour (#rrggbb)", &mut fill);
    d.fill = fill;
    let mut width = d.width.clone();
    let mut height = d.height.clone();
    ui.horizontal(|ui| {
        field(app, ui, "graphics.width", "Width", &mut width);
        field(app, ui, "graphics.height", "Height", &mut height);
    });
    d.width = width;
    d.height = height;
    let mut x = d.x.clone();
    let mut y = d.y.clone();
    ui.horizontal(|ui| {
        field(app, ui, "graphics.x", "X", &mut x);
        field(app, ui, "graphics.y", "Y", &mut y);
    });
    d.x = x;
    d.y = y;
    let mut seconds = d.seconds.clone();
    field(app, ui, "graphics.seconds", "Seconds", &mut seconds);
    d.seconds = seconds;
    if button(app, ui, "graphics.add", "Add graphic") {
        let ctx = ui.ctx().clone();
        let position = json!([number(&d.x, 960.0), number(&d.y, 540.0)]);
        let kind = if d.kind == 2 {
            "text"
        } else if d.kind == 1 {
            "ellipse"
        } else {
            "rectangle"
        };
        let params = json!({
            "kind": kind,
            "text": if d.text.trim().is_empty() { "Title" } else { d.text.trim() },
            "fontSize": number(&d.font_size, 120.0),
            "position": position,
            "size": [number(&d.width, 1400.0), number(&d.height, 110.0)],
            "fillColor": d.fill,
            "seconds": number(&d.seconds, 5.0),
        });
        match crate::menus::invoke(app, &ctx, "nl.graphics.add", params) {
            Ok(_) => app.ui.status = "Graphic added to the timeline.".into(),
            Err(e) => app.ui.status = e,
        }
    }
    save_draft(ui, "nl-graphics-draft", d);
}

// ---------------------------------------------------------------- audio master

#[derive(Clone, Default)]
struct AudioDraft {
    audio_type: usize,
    preset: String,
    target: String,
    strip: String,
    volume: String,
    muted: bool,
    solo: bool,
}

const AUDIO_TYPES: [&str; 4] = ["dialogue", "music", "sfx", "ambience"];

/// NL Audio: Essential Sound on the selected clips and the mixer strip controls, with the
/// sequence's audio master settings in view (narration is recorded with the suite's voiceover).
pub(super) fn audio_panel(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Audio",
        "Essential Sound and mix: classify and match the selected audio clips, apply presets, generate ducking and set the strip level. Narration is recorded with the voiceover recorder (no TTS).",
    );
    error_line(app, ui);
    let now = ui.input(|input| input.time);
    let cache = egui::Id::unique("nl-audio-cache");
    let stale = ui.data(|data| data.get_temp::<f64>(cache.with("at")).is_none_or(|at| now - at > 2.0));
    if stale || ui.data(|data| data.get_temp::<Value>(cache).is_none()) {
        match app.session.execute("sequence.inspect", json!({})) {
            Ok(value) => {
                ui.data_mut(|data| {
                    data.insert_temp(cache, value);
                    data.insert_temp(cache.with("at"), now);
                });
            }
            Err(e) => app.ui.status = e.to_string(),
        }
    }
    if let Some(sequence) = ui.data(|data| data.get_temp::<Value>(cache)) {
        let settings = &sequence["settings"];
        ui.label(
            RichText::new(format!(
                "Master: {} · {} Hz · sequence {}×{}",
                settings["audio_master"].as_str().unwrap_or("Stereo"),
                settings["sample_rate"].as_u64().unwrap_or(48000),
                settings["width"].as_u64().unwrap_or(0),
                settings["height"].as_u64().unwrap_or(0)
            ))
            .size(11.0)
            .color(app.tokens.text_faint),
        );
    }
    let selected: Vec<u64> = app.session.state.selection.iter().map(|clip| clip.0).collect();
    ui.label(
        RichText::new(if selected.is_empty() {
            "Select audio clips in the timeline to classify them.".to_string()
        } else {
            format!("{} clip(s) selected", selected.len())
        })
        .size(11.0)
        .color(app.tokens.text_dim),
    );
    ui.horizontal(|ui| {
        if button(app, ui, "audio.mixer", "Apri Mixer") {
            app.show_panel(PanelKind::AudioTrackMixer);
        }
        if button(app, ui, "audio.essential", "Apri Essential Sound") {
            app.show_panel(PanelKind::EssentialSound);
        }
    });
    if let Some(first) = selected.first() {
        let mut d: AudioDraft = draft(ui, "nl-audio-draft");
        if d.preset.is_empty() {
            d.preset = "(Default)".into();
        }
        if d.target.is_empty() {
            d.target = "-16".into();
        }
        if d.strip.is_empty() {
            d.strip = "A1".into();
        }
        if d.volume.is_empty() {
            d.volume = "-3".into();
        }
        let mut audio_type = d.audio_type.min(AUDIO_TYPES.len() - 1);
        craft_ui::ComboBox::from_id_salt("nl-audio-type").selected_text(AUDIO_TYPES[audio_type]).show_ui(ui, |ui| {
            for (index, name) in AUDIO_TYPES.iter().enumerate() {
                ui.selectable_value(&mut audio_type, index, *name);
            }
        });
        d.audio_type = audio_type;
        let mut preset = d.preset.clone();
        field(app, ui, "audio.preset", "Preset", &mut preset);
        d.preset = preset;
        let mut target = d.target.clone();
        field(app, ui, "audio.target", "Loudness target (LUFS)", &mut target);
        d.target = target;
        ui.horizontal(|ui| {
            if button(app, ui, "audio.set_type", "Set type") {
                run(app, ui, "essentialSound.setType", json!({"clips": selected, "type": AUDIO_TYPES[d.audio_type]}));
            }
            if button(app, ui, "audio.preset_apply", "Apply preset") {
                run(app, ui, "essentialSound.applyPreset", json!({"clips": selected, "preset": d.preset, "type": AUDIO_TYPES[d.audio_type]}));
            }
            if button(app, ui, "audio.automatch", "Auto match") {
                run(app, ui, "essentialSound.autoMatch", json!({"clips": selected, "target": number(&d.target, -16.0)}));
            }
            if button(app, ui, "audio.ducking", "Generate ducking") {
                run(app, ui, "essentialSound.generateDucking", json!({"clips": selected}));
            }
        });
        let _ = first;
        ui.add_space(6.0);
        ui.label(RichText::new("Mixer strip").size(12.0).color(app.tokens.text_dim));
        let mut strip = d.strip.clone();
        let mut volume = d.volume.clone();
        ui.horizontal(|ui| {
            field(app, ui, "audio.strip", "Strip", &mut strip);
            field(app, ui, "audio.volume", "Volume dB", &mut volume);
            ui.checkbox(&mut d.muted, RichText::new("Mute").size(11.0));
            ui.checkbox(&mut d.solo, RichText::new("Solo").size(11.0));
        });
        d.strip = strip;
        d.volume = volume;
        if button(app, ui, "audio.strip_apply", "Apply strip settings") {
            run(app, ui, "mixer.setStrip", json!({"strip": d.strip, "volumeDb": number(&d.volume, -3.0), "muted": d.muted, "solo": d.solo}));
        }
        save_draft(ui, "nl-audio-draft", d);
    } else {
        ui.add_space(6.0);
        ui.label(RichText::new("Select an audio clip to set its Essential Sound type, preset and loudness match.").size(12.0).color(app.tokens.text_faint));
    }
}

// ---------------------------------------------------------------- delivery

/// NL Delivery: the QA checks, the storage report and the production package.
pub(super) fn delivery_panel(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Delivery",
        "Check the sequence before delivering, read how much space the suite uses and export a production package (manifest, captions, chapters, story, thumbnail).",
    );
    error_line(app, ui);
    let qa_id = egui::Id::unique("nl-delivery-qa");
    let storage_id = egui::Id::unique("nl-delivery-storage");
    let social_id = egui::Id::unique("nl-delivery-social");
    let cleanup_id = egui::Id::unique("nl-delivery-cleanup");
    let platforms = ["youtube", "tiktok", "reels", "instagram"];
    let platform_id = egui::Id::unique("nl-delivery-platform");
    let mut platform = ui.data(|data| data.get_temp::<usize>(platform_id)).unwrap_or(0).min(platforms.len() - 1);
    ui.horizontal(|ui| {
        if button(app, ui, "delivery.qa", "Run checks") {
            let ctx = ui.ctx().clone();
            match crate::menus::invoke(app, &ctx, "nl.delivery.qa", json!({})) {
                Ok(value) => {
                    ui.data_mut(|data| {
                        data.insert_temp(qa_id, value);
                    });
                }
                Err(e) => app.ui.status = e,
            }
        }
        craft_ui::ComboBox::from_id_salt("nl-delivery-platform").selected_text(platforms[platform]).show_ui(ui, |ui| {
            for (index, name) in platforms.iter().enumerate() {
                ui.selectable_value(&mut platform, index, *name);
            }
        });
        if button(app, ui, "delivery.social", "Social check") {
            let ctx = ui.ctx().clone();
            match crate::menus::invoke(app, &ctx, "nl.delivery.social", json!({"platform": platforms[platform]})) {
                Ok(value) => {
                    ui.data_mut(|data| {
                        data.insert_temp(social_id, value);
                    });
                }
                Err(e) => app.ui.status = e,
            }
        }
        if button(app, ui, "delivery.cleanup_preview", "Preview cleanup") {
            let ctx = ui.ctx().clone();
            match crate::menus::invoke(app, &ctx, "nl.storage.cleanup", json!({"dryRun": true})) {
                Ok(value) => {
                    ui.data_mut(|data| {
                        data.insert_temp(cleanup_id, value);
                    });
                }
                Err(e) => app.ui.status = e,
            }
        }
        if button(app, ui, "delivery.cleanup_run", "Clean up now") {
            let ctx = ui.ctx().clone();
            match crate::menus::invoke(app, &ctx, "nl.storage.cleanup", json!({"force": true})) {
                Ok(value) => {
                    ui.data_mut(|data| {
                        data.insert_temp(cleanup_id, value);
                    });
                }
                Err(e) => app.ui.status = e,
            }
        }
        if button(app, ui, "delivery.storage", "Storage report") {
            let ctx = ui.ctx().clone();
            match crate::menus::invoke(app, &ctx, "nl.storage.status", json!({})) {
                Ok(value) => {
                    ui.data_mut(|data| {
                        data.insert_temp(storage_id, value);
                    });
                }
                Err(e) => app.ui.status = e,
            }
        }
        if button(app, ui, "delivery.package", "Export production package…") {
            app.pick_ui(crate::native_dialogs::FileDialog::Folder { at: None }, |app, paths| {
                let Some(folder) = paths.first() else { return };
                if let Err(e) = app.session.execute("nl.delivery.package", json!({"path": folder})) {
                    app.ui.status = e.to_string();
                }
            });
        }
    });
    if let Some(qa) = ui.data(|data| data.get_temp::<Value>(qa_id)) {
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!(
                "Checks: {} error(s), {} warning(s){}",
                qa["errors"].as_u64().unwrap_or(0),
                qa["warnings"].as_u64().unwrap_or(0),
                if qa["ready"] == true { " · ready" } else { "" }
            ))
            .size(12.0)
            .strong(),
        );
        for check in qa["checks"].as_array().into_iter().flatten() {
            let mark = match check["status"].as_str().unwrap_or("info") {
                "ok" => "✓",
                "warn" => "!",
                "error" => "✗",
                _ => "·",
            };
            ui.label(
                RichText::new(format!("{mark} [{}] {}", check["id"].as_str().unwrap_or(""), check["detail"].as_str().unwrap_or("")))
                    .size(11.0)
                    .color(if check["status"] == "warn" { egui::Color32::from_rgb(233, 180, 120) } else { app.tokens.text_dim }),
            );
        }
    }
    ui.data_mut(|data| data.insert_temp(platform_id, platform));
    if let Some(cleanup) = ui.data(|data| data.get_temp::<Value>(cleanup_id)) {
        ui.add_space(6.0);
        let removed = cleanup["removedFiles"].as_u64().unwrap_or(0);
        let megabytes = cleanup["removedBytes"].as_u64().unwrap_or(0) as f64 / (1024.0 * 1024.0);
        let verb = if cleanup["dryRun"] == true { "would remove" } else { "removed" };
        ui.label(
            RichText::new(format!(
                "Cleanup: {verb} {removed} file(s), {megabytes:.1} MB · disposable older than {} day(s){}",
                cleanup["tempDays"].as_u64().unwrap_or(0),
                if cleanup["includeGenerated"] == true { " · includes generated media" } else { "" }
            ))
            .size(11.0)
            .color(app.tokens.text_dim),
        );
        for category in cleanup["categories"].as_array().into_iter().flatten() {
            if category["removedFiles"].as_u64().unwrap_or(0) == 0 {
                continue;
            }
            ui.label(
                RichText::new(format!(
                    "  {}: {} file(s), {:.1} MB",
                    category["category"].as_str().unwrap_or(""),
                    category["removedFiles"].as_u64().unwrap_or(0),
                    category["removedBytes"].as_u64().unwrap_or(0) as f64 / (1024.0 * 1024.0)
                ))
                .size(10.0)
                .color(app.tokens.text_faint),
            );
        }
    }
    if let Some(social) = ui.data(|data| data.get_temp::<Value>(social_id)) {
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!(
                "Social ({}): {}{}",
                social["platform"].as_str().unwrap_or(""),
                if social["ready"] == true { "pronto" } else { "da sistemare" },
                ""
            ))
            .size(12.0)
            .strong(),
        );
        for check in social["checks"].as_array().into_iter().flatten() {
            ui.label(
                RichText::new(format!(
                    "{} [{}] {}",
                    if check["ok"] == true { "✓" } else { "✗" },
                    check["id"].as_str().unwrap_or(""),
                    check["detail"].as_str().unwrap_or("")
                ))
                .size(11.0)
                .color(app.tokens.text_dim),
            );
        }
    }
    if let Some(storage) = ui.data(|data| data.get_temp::<Value>(storage_id)) {
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!(
                "Data: {:.1} MB in {} file(s) · generated {:.1} MB · project {:.1} MB",
                storage["dataBytes"].as_u64().unwrap_or(0) as f64 / (1024.0 * 1024.0),
                storage["dataFiles"].as_u64().unwrap_or(0),
                storage["generatedBytes"].as_u64().unwrap_or(0) as f64 / (1024.0 * 1024.0),
                storage["projectBytes"].as_u64().unwrap_or(0) as f64 / (1024.0 * 1024.0),
            ))
            .size(11.0)
            .color(app.tokens.text_dim),
        );
    }
}

// ---------------------------------------------------------------- album

/// NL Album: the media ComfyUI generated for this suite, newest first, insertable again.
pub(super) fn album_panel(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(app, ui, "NL Album", "Images, videos and music ComfyUI generated for this suite. Insert one again on the timeline at the playhead.");
    error_line(app, ui);
    let now = ui.input(|input| input.time);
    let cache = egui::Id::unique("nl-album-cache");
    let stale = ui.data(|data| data.get_temp::<f64>(cache.with("at")).is_none_or(|at| now - at > 3.0));
    if stale || ui.data(|data| data.get_temp::<Value>(cache).is_none()) {
        match app.session.execute("nl.album.list", json!({})) {
            Ok(value) => {
                ui.data_mut(|data| {
                    data.insert_temp(cache, value);
                    data.insert_temp(cache.with("at"), now);
                });
            }
            Err(e) => app.ui.status = e.to_string(),
        }
    }
    let album = ui.data(|data| data.get_temp::<Value>(cache)).unwrap_or(Value::Null);
    let entries = album["entries"].as_array().cloned().unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "{} file(s) · {:.1} MB · {}",
                album["count"].as_u64().unwrap_or(0),
                album["bytes"].as_u64().unwrap_or(0) as f64 / (1024.0 * 1024.0),
                album["folder"].as_str().unwrap_or("")
            ))
            .size(11.0)
            .color(app.tokens.text_faint),
        );
        if button(app, ui, "album.refresh", "Refresh") {
            ui.data_mut(|data| data.insert_temp(cache.with("at"), 0.0f64));
        }
    });
    if entries.is_empty() {
        ui.label(RichText::new("No generated media yet: use NL Generate, NL Music or NL Story.").size(12.0).color(app.tokens.text_faint));
        return;
    }
    egui::ScrollArea::vertical().id_salt("nl-album-list").max_height(300.0).show(ui, |ui| {
        for (index, entry) in entries.iter().enumerate() {
            let kind = entry["kind"].as_str().unwrap_or("file");
            let name = entry["name"].as_str().unwrap_or("file");
            let size = entry["bytes"].as_u64().unwrap_or(0) as f64 / 1024.0;
            let mark = match kind {
                "video" => "▶",
                "audio" => "♪",
                _ => "▣",
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{mark} {name} · {size:.0} KB")).size(11.0));
                let id = format!("album.insert.{index}");
                if button(app, ui, &id, "Insert") {
                    run(app, ui, "nl.album.insert", json!({"path": entry["path"]}));
                }
            });
        }
    });
}

// ---------------------------------------------------------------- edit versions

#[derive(Clone, Default)]
struct VersionsDraft {
    name: String,
}

/// NL Versions: save a named cut of the active sequence and come back to it later.
pub(super) fn versions_panel(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Versions",
        "Save the active sequence under a name and restore a cut later. Restoring keeps the state it leaves behind as an automatic version.",
    );
    error_line(app, ui);
    let mut d: VersionsDraft = draft(ui, "nl-versions-draft");
    let now = ui.input(|input| input.time);
    let cache = egui::Id::unique("nl-versions-cache");
    let stale = ui.data(|data| data.get_temp::<f64>(cache.with("at")).is_none_or(|at| now - at > 2.0));
    if stale || ui.data(|data| data.get_temp::<Value>(cache).is_none()) {
        match app.session.execute("nl.versions.list", json!({})) {
            Ok(value) => {
                ui.data_mut(|data| {
                    data.insert_temp(cache, value);
                    data.insert_temp(cache.with("at"), now);
                });
            }
            Err(e) => app.ui.status = e.to_string(),
        }
    }
    let list = ui.data(|data| data.get_temp::<Value>(cache)).unwrap_or(Value::Null);
    let versions = list["versions"].as_array().cloned().unwrap_or_default();
    ui.horizontal(|ui| {
        let mut name = d.name.clone();
        field(app, ui, "versions.name", "Version name", &mut name);
        d.name = name;
        if button(app, ui, "versions.save", "Save current version") {
            let name = if d.name.trim().is_empty() { "Version".to_string() } else { d.name.trim().to_string() };
            run(app, ui, "nl.versions.save", json!({"name": name}));
            ui.data_mut(|data| data.insert_temp(cache.with("at"), 0.0f64));
        }
        if button(app, ui, "versions.refresh", "Refresh") {
            ui.data_mut(|data| data.insert_temp(cache.with("at"), 0.0f64));
        }
    });
    ui.label(RichText::new(format!("{} version(s) · {}", versions.len(), list["folder"].as_str().unwrap_or(""))).size(11.0).color(app.tokens.text_faint));
    if versions.is_empty() {
        ui.label(RichText::new("No saved versions yet: name the current cut and save it.").size(12.0).color(app.tokens.text_faint));
        return;
    }
    egui::ScrollArea::vertical().id_salt("nl-versions-list").max_height(300.0).show(ui, |ui| {
        for (index, version) in versions.iter().enumerate() {
            let id = version["id"].as_str().unwrap_or("").to_string();
            let name = version["name"].as_str().unwrap_or("Version");
            let clips = version["clips"].as_u64().unwrap_or(0);
            let seconds = version["durationSeconds"].as_f64().unwrap_or(0.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{name} · {clips} clip · {seconds:.1} s")).size(11.0));
                let restore_id = format!("versions.restore.{index}");
                if button(app, ui, &restore_id, "Restore") {
                    run(app, ui, "nl.versions.restore", json!({"id": id}));
                    ui.data_mut(|data| data.insert_temp(cache.with("at"), 0.0f64));
                }
                let delete_id = format!("versions.delete.{index}");
                if button(app, ui, &delete_id, "Delete") {
                    run(app, ui, "nl.versions.delete", json!({"id": id}));
                    ui.data_mut(|data| data.insert_temp(cache.with("at"), 0.0f64));
                }
            });
        }
    });
    save_draft(ui, "nl-versions-draft", d);
}

// ---------------------------------------------------------------- montage planner

#[derive(Clone, Default)]
struct MontageDraft {
    target: String,
    shot: String,
    transition: usize,
    rhythm: usize,
    bpm: String,
    beats: String,
    name: String,
    new_sequence: bool,
}

/// NL Montage: plan a quick recut at a fixed shot length and apply it, by default in a new
/// sequence so the current cut stays as it is.
pub(super) fn montage_panel(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(app, ui, "NL Montage", "Plan a quick recut of the sequence's footage at a fixed shot length, then apply it — by default into a new sequence.");
    error_line(app, ui);
    let mut d: MontageDraft = draft(ui, "nl-montage-draft");
    if d.target.is_empty() {
        d.target = "30".into();
        d.shot = "2".into();
        d.bpm = "0".into();
        d.beats = "2".into();
        d.name = "Montaggio rapido".into();
        d.new_sequence = true;
    }
    let transitions = [("cut", "Cut"), ("dissolve", "Dissolve")];
    let rhythms = [("steady", "Steady"), ("pulse", "Pulse"), ("accelerando", "Accelerando")];
    let mut transition = d.transition.min(transitions.len() - 1);
    let mut rhythm = d.rhythm.min(rhythms.len() - 1);
    ui.horizontal(|ui| {
        let mut target = d.target.clone();
        field(app, ui, "montage.target", "Target seconds", &mut target);
        d.target = target;
        let mut shot = d.shot.clone();
        field(app, ui, "montage.shot", "Shot seconds", &mut shot);
        d.shot = shot;
        craft_ui::ComboBox::from_id_salt("nl-montage-transition").selected_text(transitions[transition].1).show_ui(ui, |ui| {
            for (index, (_, label)) in transitions.iter().enumerate() {
                ui.selectable_value(&mut transition, index, *label);
            }
        });
        craft_ui::ComboBox::from_id_salt("nl-montage-rhythm").selected_text(rhythms[rhythm].1).show_ui(ui, |ui| {
            for (index, (_, label)) in rhythms.iter().enumerate() {
                ui.selectable_value(&mut rhythm, index, *label);
            }
        });
        d.transition = transition;
        d.rhythm = rhythm;
    });
    ui.horizontal(|ui| {
        let mut bpm = d.bpm.clone();
        field(app, ui, "montage.bpm", "BPM (0 = off)", &mut bpm);
        d.bpm = bpm;
        let mut beats = d.beats.clone();
        field(app, ui, "montage.beats", "Beats per shot", &mut beats);
        d.beats = beats;
    });
    let plan_id = egui::Id::unique("nl-montage-plan");
    let applied_id = egui::Id::unique("nl-montage-applied");
    ui.horizontal(|ui| {
        if button(app, ui, "montage.plan", "Plan montage") {
            let ctx = ui.ctx().clone();
            let mut params = json!({
                "targetSeconds": number(&d.target, 30.0),
                "shotSeconds": number(&d.shot, 2.0),
                "transition": transitions[d.transition].0,
                "rhythm": rhythms[d.rhythm].0,
            });
            let bpm = number(&d.bpm, 0.0);
            if bpm > 0.0 {
                params["bpm"] = json!(bpm);
                params["beatsPerShot"] = json!(number(&d.beats, 2.0).max(0.25));
            }
            match crate::menus::invoke(app, &ctx, "nl.montage.plan", params) {
                Ok(value) => {
                    ui.data_mut(|data| {
                        data.insert_temp(plan_id, value);
                        data.insert_temp(applied_id, Value::Null);
                    });
                }
                Err(e) => app.ui.status = e,
            }
        }
        let mut name = d.name.clone();
        field(app, ui, "montage.name", "New sequence name", &mut name);
        d.name = name;
        ui.checkbox(&mut d.new_sequence, RichText::new("New sequence").size(11.0));
    });
    let plan = ui.data(|data| data.get_temp::<Value>(plan_id)).unwrap_or(Value::Null);
    if plan["shots"].as_array().is_some_and(|shots| !shots.is_empty()) {
        ui.label(
            RichText::new(format!(
                "{} shots · {} s · {} source clip(s) · {} · {}",
                plan["count"].as_u64().unwrap_or(0),
                plan["totalSeconds"].as_f64().unwrap_or(0.0),
                plan["sources"].as_u64().unwrap_or(0),
                plan["transition"].as_str().unwrap_or("cut"),
                plan["rhythm"].as_str().unwrap_or("steady")
            ))
            .size(11.0)
            .color(app.tokens.text_dim),
        );
        egui::ScrollArea::vertical().id_salt("nl-montage-plan").max_height(180.0).show(ui, |ui| {
            for shot in plan["shots"].as_array().into_iter().flatten() {
                ui.label(
                    RichText::new(format!(
                        "{:>2}. {} · in {:.1}s · {:.1}s",
                        shot["index"].as_u64().unwrap_or(0),
                        shot["name"].as_str().unwrap_or(""),
                        shot["sourceInSeconds"].as_f64().unwrap_or(0.0),
                        shot["durationSeconds"].as_f64().unwrap_or(0.0)
                    ))
                    .size(11.0),
                );
            }
        });
        if button(app, ui, "montage.apply", "Apply montage") {
            let ctx = ui.ctx().clone();
            let params = json!({
                "createSequence": d.new_sequence,
                "name": if d.name.trim().is_empty() { "Montaggio rapido" } else { d.name.trim() },
                "plan": plan,
            });
            match crate::menus::invoke(app, &ctx, "nl.montage.apply", params) {
                Ok(value) => {
                    ui.data_mut(|data| {
                        data.insert_temp(applied_id, value);
                    });
                }
                Err(e) => app.ui.status = e,
            }
        }
    }
    if let Some(applied) = ui.data(|data| data.get_temp::<Value>(applied_id)).filter(|value| !value.is_null()) {
        ui.label(
            RichText::new(format!(
                "Montage placed: {} shot(s), {} transition(s), new sequence: {}",
                applied["placed"].as_array().map_or(0, Vec::len),
                applied["transitions"].as_u64().unwrap_or(0),
                applied["newSequence"]
            ))
            .size(11.0)
            .color(app.tokens.text_dim),
        );
    }
    save_draft(ui, "nl-montage-draft", d);
}

// ---------------------------------------------------------------- bridge (generative fill)

#[derive(Clone)]
struct BridgeDraft {
    outgoing: String,
    incoming: String,
    mode: usize,
    transition: usize,
    analyze: bool,
    prompt: String,
    place_path: String,
}

impl Default for BridgeDraft {
    fn default() -> Self {
        Self {
            outgoing: String::new(),
            incoming: String::new(),
            // Two references + the outgoing audio tail is the mode that keeps both sides and sound.
            mode: 1,
            transition: 0,
            analyze: true,
            prompt: String::new(),
            place_path: String::new(),
        }
    }
}

/// NL Bridge: fill the empty gap between two shots with a locally generated transition. The two
/// frames come from the timeline (last frame of the outgoing clip, first frame of the incoming one)
/// and the result is placed back in the gap when ComfyUI finishes.
pub(super) fn bridge_panel(app: &mut FilmcraftApp, ui: &mut egui::Ui) {
    title(
        app,
        ui,
        "NL Bridge",
        "Generate a transition between two shots: the bridge takes the last frame of the outgoing clip and the first frame of the incoming one, generates the missing section with the local ComfyUI and puts it in the gap.",
    );
    error_line(app, ui);
    let mut d: BridgeDraft = draft(ui, "nl-bridge-draft");
    let modes = [
        ("flf (first/last frame)", "First + last frame (MiniMax H3 FLF)"),
        ("r2v (2 references)", "Two references (H3 R2V)"),
        ("i2v (last frame only)", "Last frame only (image→video)"),
    ];
    let mut mode = d.mode.min(modes.len() - 1);
    ui.horizontal(|ui| {
        field(app, ui, "bridge.outgoing", "Outgoing clip id", &mut d.outgoing);
        field(app, ui, "bridge.incoming", "Incoming clip id", &mut d.incoming);
    });
    let _ = &mut mode;
    ui.horizontal(|ui| {
        craft_ui::ComboBox::from_id_salt("nl-bridge-mode").selected_text(modes[d.mode.min(modes.len() - 1)].1).show_ui(ui, |ui| {
            for (index, (_, label)) in modes.iter().enumerate() {
                ui.selectable_value(&mut d.mode, index, *label);
            }
        });
        let transitions = [("cut", "Cut"), ("dissolve", "Dissolve")];
        craft_ui::ComboBox::from_id_salt("nl-bridge-transition").selected_text(transitions[d.transition.min(1)].1).show_ui(ui, |ui| {
            for (index, (_, label)) in transitions.iter().enumerate() {
                ui.selectable_value(&mut d.transition, index, *label);
            }
        });
        ui.label(RichText::new("Leave the ids empty to use the empty space around the playhead.").size(11.0).color(app.tokens.text_faint));
    });
    let mut prompt = d.prompt.clone();
    ui.label(RichText::new("Prompt (optional)").size(12.0).color(app.tokens.text_dim));
    let r = ui.add(egui::TextEdit::multiline(&mut prompt).desired_rows(3).desired_width(f32::INFINITY).id_salt("nl-bridge-prompt"));
    app.auto.add("nl.bridge.prompt", r.rect, "Bridge prompt");
    d.prompt = prompt;
    let plan_id = egui::Id::unique("nl-bridge-plan");
    let parsed = |value: &str| -> u64 { value.trim().parse().unwrap_or(0) };
    let ids = || -> (Option<u64>, Option<u64>) {
        let outgoing = parsed(&d.outgoing);
        let incoming = parsed(&d.incoming);
        match (outgoing, incoming) {
            (0, 0) => (None, None),
            (outgoing, incoming) if outgoing > 0 && incoming > 0 => (Some(outgoing), Some(incoming)),
            _ => (None, None),
        }
    };
    let (outgoing, incoming) = ids();
    let mut params = json!({"mode": modes[d.mode.min(modes.len() - 1)].0.split(' ').next().unwrap_or("flf"), "transition": if d.transition == 1 { "dissolve" } else { "cut" }, "analyzeIncoming": true});
    if let Some(outgoing) = outgoing {
        params["outgoing"] = json!(outgoing);
    }
    if let Some(incoming) = incoming {
        params["incoming"] = json!(incoming);
    }
    if !d.prompt.trim().is_empty() {
        params["prompt"] = json!(d.prompt.trim());
    }
    ui.horizontal(|ui| {
        if button(app, ui, "bridge.placeholder", "Crea segnaposto nel gap") {
            let ctx = ui.ctx().clone();
            match crate::menus::invoke(app, &ctx, "nl.agent.gap", params.clone()) {
                Ok(value) => {
                    let msg = value["message"].as_str().unwrap_or("Ponte AI creato nel gap.").to_string();
                    app.ui.status = msg;
                }
                Err(e) => app.ui.status = e,
            }
        }
        if button(app, ui, "bridge.plan", "Anteprima frame (senza generare)") {
            let ctx = ui.ctx().clone();
            match crate::menus::invoke(app, &ctx, "nl.bridge.plan", params.clone()) {
                Ok(value) => {
                    ui.data_mut(|data| data.insert_temp(plan_id, value));
                }
                Err(e) => app.ui.status = e,
            }
        }
        if button(app, ui, "bridge.generate", "Generate bridge") {
            let ctx = ui.ctx().clone();
            match crate::menus::invoke(app, &ctx, "nl.bridge.generate", params.clone()) {
                Ok(_) => app.ui.status = "Bridge generation started: the result will be placed in the gap.".into(),
                Err(e) => app.ui.status = e,
            }
        }
    });
    ui.horizontal(|ui| {
        let mut path = d.place_path.clone();
        field(app, ui, "bridge.place_path", "Generated file to place in the gap", &mut path);
        d.place_path = path;
        if button(app, ui, "bridge.place", "Place file in gap") {
            let path = d.place_path.trim().to_string();
            if path.is_empty() {
                app.ui.status = "Choose a generated file first (for example from NL Album).".into();
            } else {
                let mut place = params.clone();
                place["path"] = json!(path);
                let ctx = ui.ctx().clone();
                match crate::menus::invoke(app, &ctx, "nl.bridge.place", place) {
                    Ok(_) => app.ui.status = "Generated file placed in the gap.".into(),
                    Err(e) => app.ui.status = e,
                }
            }
        }
    });
    if let Some(plan) = ui.data(|data| data.get_temp::<Value>(plan_id)) {
        ui.label(
            RichText::new(format!(
                "Gap {:.2} s · {} → {} · mode {} · {}",
                plan["gapSeconds"].as_f64().unwrap_or(0.0),
                plan["outgoing"].as_u64().unwrap_or(0),
                plan["incoming"].as_u64().unwrap_or(0),
                plan["mode"].as_str().unwrap_or(""),
                plan["workflow"].as_str().unwrap_or("(no workflow found)"),
            ))
            .size(11.0)
            .color(app.tokens.text_dim),
        );
        for key in ["lastFrame", "firstFrame"] {
            if let Some(path) = plan[key].as_str() {
                ui.label(RichText::new(format!("{key}: {path}")).size(10.0).color(app.tokens.text_faint));
            }
        }
    }
    save_draft(ui, "nl-bridge-draft", d);
}
