//! AI story → scenes → keyframes → timeline.
//!
//! The operator writes an idea, the local model drafts a reviewed story (beats, characters,
//! locations, scenes with visual and motion prompts), the story is edited by hand, and each scene's
//! keyframe (or video) is generated with ComfyUI and placed on the timeline in order. Nothing is
//! invented silently: the story is a JSON document shown in the panel, the generated media is
//! imported as normal project media, and placing it is an explicit command.
//!
//! Storage: with a saved project the story lives in `nl/story.json` beside it; before the first
//! save it lives in the session and says so. The model is unloaded right after drafting
//! (`keep_alive: 0`) so a ComfyUI generation that follows does not share VRAM with it.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use filmcraft_time::Tick;
use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{Result, Session};

use super::state::PendingStory;

/// Longest strings kept from the model, so a runaway answer cannot bloat the project.
const MAX_TITLE: usize = 200;
const MAX_TEXT: usize = 4000;
const MAX_PROMPT: usize = 1500;
const MAX_SCENES: usize = 40;
const MAX_SHOTS: usize = 8;

fn bad_cmd(cmd: &str, msg: impl ToString) -> crate::EngineError {
    bad(cmd, msg.to_string())
}

/// The empty story document the panel starts from.
pub fn empty() -> Value {
    json!({
        "title": "",
        "synopsis": "",
        "style": "",
        "characters": [],
        "locations": [],
        "scenes": [],
    })
}

/// The story of this session (loaded from the sidecar on first use).
pub fn load(s: &mut Session) -> Value {
    if let Some(story) = &s.nl.story {
        return story.clone();
    }
    let story = story_path(s)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .map(|v| sanitize(&v).unwrap_or_else(|_| empty()))
        .unwrap_or_else(empty);
    s.nl.story = Some(story.clone());
    story
}

/// Where the story is kept: `nl/story.json` beside a saved project.
fn story_path(s: &Session) -> Option<std::path::PathBuf> {
    let project = s.path.as_deref()?;
    let dir = std::path::Path::new(project).parent()?.to_path_buf();
    Some(dir.join("nl").join("story.json"))
}

/// Keep only sane values from a model/user document and number the scenes/shots.
pub fn sanitize(value: &Value) -> std::result::Result<Value, String> {
    let object = value.as_object().ok_or("the story must be a JSON object")?;
    let text = |key: &str, max: usize| object.get(key).and_then(Value::as_str).unwrap_or_default().trim().chars().take(max).collect::<String>();
    let mut scenes = Vec::new();
    for (index, scene) in object.get("scenes").and_then(Value::as_array).into_iter().flatten().enumerate().take(MAX_SCENES) {
        let Some(scene) = scene.as_object() else { continue };
        let scene_text = |key: &str, max: usize| scene.get(key).and_then(Value::as_str).unwrap_or_default().trim().chars().take(max).collect::<String>();
        let duration = scene.get("durationSec").and_then(Value::as_f64).filter(|d| d.is_finite()).unwrap_or(5.0).clamp(1.0, 120.0);
        let mut shots = Vec::new();
        for (shot_index, shot) in scene.get("shots").and_then(Value::as_array).into_iter().flatten().enumerate().take(MAX_SHOTS) {
            let Some(shot) = shot.as_object() else { continue };
            let shot_text = |key: &str, max: usize| shot.get(key).and_then(Value::as_str).unwrap_or_default().trim().chars().take(max).collect::<String>();
            shots.push(json!({
                "id": shot_index as u64 + 1,
                "camera": shot_text("camera", 80),
                "description": shot_text("description", MAX_TEXT),
                "imagePrompt": shot_text("imagePrompt", MAX_PROMPT),
                "durationSec": shot.get("durationSec").and_then(Value::as_f64).filter(|d| d.is_finite()).unwrap_or(3.0).clamp(0.5, 60.0),
            }));
        }
        let mut entry = json!({
            "id": scene.get("id").and_then(Value::as_u64).unwrap_or(index as u64 + 1),
            "heading": scene_text("heading", MAX_TITLE),
            "action": scene_text("action", MAX_TEXT),
            "dialogue": scene_text("dialogue", MAX_TEXT),
            "durationSec": duration,
            "imagePrompt": scene_text("imagePrompt", MAX_PROMPT),
            "motionPrompt": scene_text("motionPrompt", MAX_PROMPT),
            "location": scene_text("location", MAX_TITLE),
            "shots": shots,
        });
        if let Some(image) = scene.get("image").filter(|v| v.get("item").and_then(Value::as_u64).is_some()) {
            entry["image"] = image.clone();
        }
        if let Some(video) = scene.get("video").filter(|v| v.get("item").and_then(Value::as_u64).is_some()) {
            entry["video"] = video.clone();
        }
        scenes.push(entry);
    }
    let people = |key: &str| -> Vec<Value> {
        object
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(24)
            .filter_map(|person| {
                let person = person.as_object()?;
                let name = person.get("name").and_then(Value::as_str)?.trim().chars().take(MAX_TITLE).collect::<String>();
                if name.is_empty() {
                    return None;
                }
                let description = person.get("description").and_then(Value::as_str).unwrap_or_default().trim().chars().take(MAX_TEXT).collect::<String>();
                let mut entry = json!({"name": name, "description": description});
                if let Some(link) = person.get("sheet").filter(|v| v.get("item").and_then(Value::as_u64).is_some()) {
                    entry["sheet"] = link.clone();
                }
                if let Some(link) = person.get("plate").filter(|v| v.get("item").and_then(Value::as_u64).is_some()) {
                    entry["plate"] = link.clone();
                }
                Some(entry)
            })
            .collect()
    };
    Ok(json!({
        "title": text("title", MAX_TITLE),
        "synopsis": text("synopsis", MAX_TEXT),
        "style": text("style", MAX_TITLE),
        "characters": people("characters"),
        "locations": people("locations"),
        "scenes": scenes,
    }))
}

/// Save the story: session always, sidecar when the project has a path.
fn save(s: &mut Session, story: &Value) -> Value {
    s.nl.story = Some(story.clone());
    let mut saved = false;
    let mut note = Value::Null;
    match story_path(s) {
        Some(path) => {
            let ok = path
                .parent()
                .map(std::fs::create_dir_all)
                .transpose()
                .and_then(|_| std::fs::write(&path, serde_json::to_vec_pretty(story).unwrap_or_default()))
                .is_ok();
            if ok {
                saved = true;
            } else {
                note = json!(format!("the story could not be written to {}", path.display()));
            }
        }
        None => note = json!("save the project to keep the story beside it; for now it lives in this session"),
    }
    json!({"story": story, "saved": saved, "note": note})
}

/// The system instruction shared by draft and refine.
fn system_prompt(language: &str) -> String {
    format!(
        "Sei un regista e sceneggiatore. Rispondi SOLO con un oggetto JSON valido, senza testo attorno e senza markdown. \
Lingua del testo: {language}. Schema esatto: \
{{\"title\":string,\"synopsis\":string,\"style\":string,\"characters\":[{{\"name\":string,\"description\":string}}],\
\"locations\":[{{\"name\":string,\"description\":string}}],\
\"scenes\":[{{\"heading\":string,\"location\":string,\"action\":string,\"dialogue\":string,\"durationSec\":number(3..20),\
\"imagePrompt\":string,\"motionPrompt\":string}}]}}. \
Massimo {MAX_SCENES} scene. imagePrompt e motionPrompt devono essere in inglese, visivi e concreti: \
soggetto, azione, ambiente, luce, camera; NON citare nomi di modelli, risoluzione, durata o aspect ratio. \
motionPrompt descrive il movimento di camera/movimento interno in una frase."
    )
}

fn draft_call(llm: &super::llm::LlmConfig, prompt: &str, language: &str, timeout: Duration) -> std::result::Result<Value, String> {
    let value = llm.auto_json(&system_prompt(language), prompt, timeout, Some("0"))?;
    match sanitize(&value) {
        Ok(story) if story["scenes"].as_array().is_some_and(|scenes| !scenes.is_empty()) => Ok(story),
        Ok(_) => Err("the model answered JSON without scenes".into()),
        Err(e) => Err(format!("the model's story is not usable: {e}")),
    }
}

/// `nl.story.get` — the current story and where it is kept.
fn story_get(s: &mut Session, _: &Value) -> Result<Value> {
    let story = load(s);
    let path = story_path(s);
    Ok(json!({"story": story, "saved": path.is_some(), "path": path.map(|p| p.to_string_lossy().to_string())}))
}

/// `nl.story.set` — replace the story (manual editing and refinements from the panel).
fn story_set(s: &mut Session, p: &Value) -> Result<Value> {
    let story = p.get("story").ok_or_else(|| bad_cmd("nl.story.set", "need `story`"))?;
    let story = sanitize(story).map_err(|e| bad_cmd("nl.story.set", e))?;
    Ok(save(s, &story))
}

/// `nl.story.update` — change one scene's fields without resending the whole story.
fn story_update(s: &mut Session, p: &Value) -> Result<Value> {
    let scene_id = p.get("scene").and_then(Value::as_u64).ok_or_else(|| bad_cmd("nl.story.update", "need `scene` (the scene id)"))?;
    let fields = p.get("fields").and_then(Value::as_object).ok_or_else(|| bad_cmd("nl.story.update", "need `fields` (an object)"))?;
    let mut story = load(s);
    let scenes = story["scenes"].as_array_mut().ok_or_else(|| bad_cmd("nl.story.update", "the story has no scenes"))?;
    let scene =
        scenes.iter_mut().find(|scene| scene["id"].as_u64() == Some(scene_id)).ok_or_else(|| bad_cmd("nl.story.update", format!("no scene {scene_id}")))?;
    for (key, value) in fields {
        if !matches!(key.as_str(), "heading" | "action" | "dialogue" | "imagePrompt" | "motionPrompt" | "durationSec") {
            return Err(bad_cmd("nl.story.update", format!("`{key}` is not an editable scene field")));
        }
        scene[key] = value.clone();
    }
    let story = sanitize(&story).map_err(|e| bad_cmd("nl.story.update", e))?;
    Ok(save(s, &story))
}

/// `nl.story.draft` — draft the whole story from an idea with the local model.
fn story_draft(s: &mut Session, p: &Value) -> Result<Value> {
    let idea = p
        .get("idea")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|idea| !idea.is_empty())
        .ok_or_else(|| bad_cmd("nl.story.draft", "need `idea`"))?
        .to_string();
    if idea.chars().count() > 4000 {
        return Err(bad_cmd("nl.story.draft", "`idea` is too long (4000 characters max)"));
    }
    let language = p.get("language").and_then(Value::as_str).unwrap_or("it").to_string();
    let llm = super::llm::LlmConfig::for_session(s, p.get("model").and_then(Value::as_str));
    let prompt = format!("Idea del video: {idea}\nScrivi la storia per questo video.");
    let wait = p.get("wait").and_then(Value::as_bool).unwrap_or(false);
    if wait {
        let story = draft_call(&llm, &prompt, &language, Duration::from_secs(300)).map_err(|e| bad_cmd("nl.story.draft", e))?;
        let mut result = save(s, &story);
        result["model"] = json!(llm.model);
        return Ok(result);
    }
    let result: Arc<Mutex<Option<std::result::Result<Value, String>>>> = Arc::default();
    let out = result.clone();
    let id = s.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let job = crate::Job { id, label: format!("NL story draft ({})", llm.model), progress: Default::default(), result: Default::default() };
    let progress = job.progress.clone();
    let llm_run = llm.clone();
    s.jobs.push(job);
    s.nl.story_pending = Some(PendingStory { job: id, result, epoch: s.document_epoch });
    std::thread::Builder::new()
        .name("filmcraft-nl-story-draft".into())
        .spawn(move || {
            *progress.status.lock().unwrap_or_else(|e| e.into_inner()) = format!("NL story: drafting with {}", llm_run.model);
            let value = draft_call(&llm_run, &prompt, &language, Duration::from_secs(300));
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
            progress.finished.store(true, std::sync::atomic::Ordering::Relaxed);
        })
        .map_err(|e| crate::EngineError::Other(format!("can't start the story thread: {e}")))?;
    Ok(json!({"job": id, "model": llm.model}))
}

/// The camera vocabulary of the shot map, in a stable cycle.
const SHOT_CAMERAS: [&str; 6] = ["establishing wide shot", "medium shot", "close-up", "over-the-shoulder shot", "insert shot", "low-angle shot"];

/// `nl.story.shotmap` — break the story scenes into numbered shots with a camera for each.
fn story_shotmap(s: &mut Session, p: &Value) -> Result<Value> {
    let per_scene = p.get("shotsPerScene").and_then(Value::as_u64).unwrap_or(3).clamp(1, 6) as usize;
    let wanted = p.get("scene").and_then(Value::as_u64);
    let mut story = load(s);
    let Some(scenes) = story["scenes"].as_array_mut() else {
        return Err(bad_cmd("nl.story.shotmap", "the story has no scenes: draft it first"));
    };
    let mut touched = 0;
    let mut total = 0;
    for scene in scenes.iter_mut() {
        let scene_id = scene["id"].as_u64().unwrap_or(0);
        if wanted.is_some_and(|wanted| wanted != scene_id) {
            continue;
        }
        let duration = scene["durationSec"].as_f64().unwrap_or(5.0).clamp(1.0, 120.0);
        let count = per_scene.min((duration.floor() as usize).max(1));
        let each = ((duration / count as f64) * 10.0).round() / 10.0;
        let heading = scene["heading"].as_str().unwrap_or("Scene").to_string();
        let action: String = scene["action"].as_str().unwrap_or_default().chars().take(90).collect();
        let base_prompt = scene["imagePrompt"].as_str().unwrap_or_default().to_string();
        let mut shots = Vec::new();
        for index in 0..count {
            let camera = SHOT_CAMERAS[(scene_id as usize + index) % SHOT_CAMERAS.len()];
            let image_prompt = if base_prompt.trim().is_empty() { format!("{camera}, {action}") } else { format!("{camera}, {base_prompt}") };
            shots.push(json!({
                "id": index as u64 + 1,
                "camera": camera,
                "description": format!("{camera} · {heading}: {action}"),
                "imagePrompt": image_prompt,
                "durationSec": each,
            }));
            total += 1;
        }
        scene["shots"] = json!(shots);
        touched += 1;
    }
    if wanted.is_some() && touched == 0 {
        return Err(bad_cmd("nl.story.shotmap", format!("no scene {}", wanted.unwrap_or(0))));
    }
    let story_value = story.clone();
    s.execute("nl.story.set", json!({"story": story_value}))?;
    Ok(json!({"scenes": touched, "shots": total, "shotsPerScene": per_scene}))
}

/// `nl.story.refine` — ask the model to rewrite one scene from an instruction.
fn story_refine(s: &mut Session, p: &Value) -> Result<Value> {
    let scene_id = p.get("scene").and_then(Value::as_u64).ok_or_else(|| bad_cmd("nl.story.refine", "need `scene`"))?;
    let instructions = p
        .get("instructions")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|i| !i.is_empty())
        .ok_or_else(|| bad_cmd("nl.story.refine", "need `instructions`: what should change in this scene?"))?;
    let mut story = load(s);
    let scene = story["scenes"]
        .as_array()
        .and_then(|scenes| scenes.iter().find(|scene| scene["id"].as_u64() == Some(scene_id)))
        .cloned()
        .ok_or_else(|| bad_cmd("nl.story.refine", format!("no scene {scene_id}")))?;
    let llm = super::llm::LlmConfig::for_session(s, p.get("model").and_then(Value::as_str));
    let language = p.get("language").and_then(Value::as_str).unwrap_or("it");
    let prompt = format!(
        "Scena attuale (JSON): {scene}\nIstruzione dell'operatore: {instructions}\nRiscrivi SOLO questa scena e rispondi con {{\"scene\": {{...}}}}; mantieni o aggiorna la location in base all'istruzione."
    );
    let system = format!(
        "Sei un regista e sceneggiatore. Rispondi SOLO con un oggetto JSON {{\"scene\":{{...}}}}, senza testo attorno. Lingua del testo: {language}. \
La scena ha: heading, location, action, dialogue, durationSec (3..20), imagePrompt e motionPrompt in inglese (soggetto, azione, ambiente, luce, camera; niente nomi di modelli, risoluzione, durata)."
    );
    let value = llm.auto_json(&system, &prompt, Duration::from_secs(300), Some("0")).map_err(|e| bad_cmd("nl.story.refine", e))?;
    let rewritten = value.get("scene").cloned().unwrap_or_else(|| value.clone());
    let rewritten = sanitize(&json!({"scenes": [rewritten]})).map_err(|e| bad_cmd("nl.story.refine", e))?;
    let Some(mut new_scene) = rewritten["scenes"].as_array().and_then(|s| s.first().cloned()) else {
        return Err(bad_cmd("nl.story.refine", "the model did not return a usable scene"));
    };
    new_scene["id"] = json!(scene_id);
    if let Some(scenes) = story["scenes"].as_array_mut()
        && let Some(slot) = scenes.iter_mut().find(|scene| scene["id"].as_u64() == Some(scene_id))
    {
        new_scene["image"] = slot.get("image").cloned().unwrap_or(Value::Null);
        new_scene["video"] = slot.get("video").cloned().unwrap_or(Value::Null);
        *slot = new_scene;
    } else {
        return Err(bad_cmd("nl.story.refine", format!("no scene {scene_id}")));
    }
    let mut result = save(s, &story);
    result["scene"] = json!(scene_id);
    result["model"] = json!(llm.model);
    Ok(result)
}

/// The reference images a scene's keyframe should use: the location plate first (skill order:
/// background, then characters) and the sheets of the characters named in the scene. Bounded to
/// three references, which is what the Flux2-Klein reference workflows expect.
fn scene_references(story: &Value, scene: &Value) -> Vec<(String, String)> {
    let text = format!(
        "{} {} {} {}",
        scene["heading"].as_str().unwrap_or_default(),
        scene["action"].as_str().unwrap_or_default(),
        scene["dialogue"].as_str().unwrap_or_default(),
        scene["imagePrompt"].as_str().unwrap_or_default()
    )
    .to_lowercase();
    let mut references: Vec<(String, String)> = Vec::new();
    let location = scene["location"].as_str().unwrap_or_default();
    if !location.is_empty() {
        let plate = story["locations"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|entry| entry["name"].as_str().is_some_and(|name| name.eq_ignore_ascii_case(location)))
            .and_then(|entry| entry["plate"]["path"].as_str());
        if let Some(path) = plate {
            references.push(("background".to_string(), path.to_string()));
        }
    }
    for character in story["characters"].as_array().into_iter().flatten() {
        let Some(name) = character["name"].as_str() else { continue };
        if name.is_empty() || !text.contains(&name.to_lowercase()) {
            continue;
        }
        if let Some(path) = character["sheet"]["path"].as_str() {
            references.push((name.to_string(), path.to_string()));
        }
    }
    references.truncate(3);
    references
}

/// The images a scene video should reference: the keyframe first (i2v anchor) and, when asked, the
/// sheet of the first character named in the scene as a second reference (ref2v identity).
fn video_reference_images(story: &Value, scene: &Value, use_sheet: bool) -> Vec<String> {
    let mut images = Vec::new();
    if let Some(path) = scene["image"]["path"].as_str().filter(|path| std::path::Path::new(path).is_file()) {
        images.push(path.to_string());
    }
    if use_sheet {
        for (name, path) in scene_references(story, scene) {
            if name == "background" || images.contains(&path) || !std::path::Path::new(&path).is_file() {
                continue;
            }
            images.push(path);
            break;
        }
    }
    images.truncate(2);
    images
}

/// The index of a character/location named by `character`/`location` (1-based number or name).
fn library_index(story: &Value, key: &str, selector: &Value) -> Option<usize> {
    let entries = story[key].as_array()?;
    match selector {
        Value::Number(number) => {
            let index = number.as_u64()? as usize;
            (index >= 1 && index <= entries.len()).then_some(index - 1)
        }
        Value::String(name) => entries.iter().position(|entry| entry["name"].as_str().is_some_and(|entry| entry.eq_ignore_ascii_case(name.trim()))),
        _ => None,
    }
}

/// `nl.story.sheet` — a multi-angle character sheet, generated once and reused by every scene.
fn story_sheet(s: &mut Session, p: &Value) -> Result<Value> {
    let story = load(s);
    let selector = p.get("character").cloned().unwrap_or(Value::Null);
    let index = library_index(&story, "characters", &selector)
        .ok_or_else(|| bad_cmd("nl.story.sheet", "pass `character`: the 1-based number or the name from the story"))?;
    let character = &story["characters"][index];
    let name = character["name"].as_str().unwrap_or("the character");
    let description = character["description"].as_str().unwrap_or_default();
    let style = story["style"].as_str().unwrap_or_default();
    let prompt = p.get("prompt").and_then(Value::as_str).map(str::trim).filter(|prompt| !prompt.is_empty()).map(str::to_string).unwrap_or_else(|| {
        format!(
            "Character reference sheet, multiple views on white background: front view full body, side view full body, back view full body, and close-up headshot portrait. {name}, {description}. {style}. clean white background, high detail, sharp focus, professional character design sheet"
        )
    });
    let mut params = json!({
        "prompt": prompt,
        "kind": "image",
        "resolutionMode": "image",
        "storyTarget": {"character": index + 1, "role": "sheet"},
    });
    for key in ["workflow", "seed", "wait", "dryRun"] {
        if let Some(value) = p.get(key) {
            params[key] = value.clone();
        }
    }
    let mut result = s.execute("graphics.generateComfy", params)?;
    result["character"] = json!(index + 1);
    result["name"] = json!(name);
    Ok(result)
}

/// `nl.story.plate` — an establishing background plate for a location, generated once.
fn story_plate(s: &mut Session, p: &Value) -> Result<Value> {
    let story = load(s);
    let selector = p.get("location").cloned().unwrap_or(Value::Null);
    let index = library_index(&story, "locations", &selector)
        .ok_or_else(|| bad_cmd("nl.story.plate", "pass `location`: the 1-based number or the name from the story"))?;
    let location = &story["locations"][index];
    let name = location["name"].as_str().unwrap_or("the location");
    let description = location["description"].as_str().unwrap_or_default();
    let style = story["style"].as_str().unwrap_or_default();
    let prompt = p
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("Establishing shot, wide angle, no people, background plate, high detail, {description}. {style}"));
    let mut params = json!({
        "prompt": prompt,
        "kind": "image",
        "resolutionMode": "image",
        "storyTarget": {"location": index + 1, "role": "plate"},
    });
    for key in ["workflow", "seed", "wait", "dryRun"] {
        if let Some(value) = p.get(key) {
            params[key] = value.clone();
        }
    }
    let mut result = s.execute("graphics.generateComfy", params)?;
    result["location"] = json!(index + 1);
    result["name"] = json!(name);
    Ok(result)
}

/// `nl.story.generate` — generate the keyframe (or video) of one scene with ComfyUI and link it.
fn story_generate(s: &mut Session, p: &Value) -> Result<Value> {
    let scene_id = p.get("scene").and_then(Value::as_u64).ok_or_else(|| bad_cmd("nl.story.generate", "need `scene`"))?;
    let story = load(s);
    let scene = story["scenes"]
        .as_array()
        .and_then(|scenes| scenes.iter().find(|scene| scene["id"].as_u64() == Some(scene_id)))
        .cloned()
        .ok_or_else(|| bad_cmd("nl.story.generate", format!("no scene {scene_id}")))?;
    let kind = p.get("kind").and_then(Value::as_str).unwrap_or("image");
    if !matches!(kind, "image" | "video") {
        return Err(bad_cmd("nl.story.generate", "`kind` must be image or video"));
    }
    let prompt = p
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .or_else(|| {
            let field = if kind == "video" { "motionPrompt" } else { "imagePrompt" };
            scene[field].as_str().map(str::trim).filter(|p| !p.is_empty()).map(str::to_string)
        })
        .or_else(|| scene["imagePrompt"].as_str().map(str::trim).filter(|p| !p.is_empty()).map(str::to_string))
        .ok_or_else(|| bad_cmd("nl.story.generate", "the scene has no prompt: write an image prompt or use Prompt AI first"))?;
    let mut params = json!({
        "prompt": prompt,
        "kind": kind,
        "resolutionMode": if kind == "video" { "video" } else { "image" },
        "storyTarget": {"scene": scene_id},
    });
    // A video generation is guided by the scene's keyframe (image-to-video) and, when asked, by
    // the video of an earlier scene (v2v extend): the media becomes a reference the workflow
    // really declares, or the call is refused with a clear message.
    if kind == "video" {
        let images = video_reference_images(&story, &scene, p.get("useSheet").and_then(Value::as_bool).unwrap_or(false));
        if !images.is_empty() {
            params["images"] = json!(images);
        }
        if let Some(from) = p.get("extendFromScene").and_then(Value::as_u64) {
            let source = story["scenes"].as_array().and_then(|scenes| scenes.iter().find(|scene| scene["id"].as_u64() == Some(from)));
            let path = source
                .and_then(|scene| scene["video"]["path"].as_str().or_else(|| scene["image"]["path"].as_str()))
                .filter(|p| std::path::Path::new(p).is_file());
            let Some(path) = path else {
                return Err(bad_cmd("nl.story.generate", "the scene to extend has no generated media: generate it first"));
            };
            params["video"] = json!(path);
        }
    }
    if let Some(workflow) = p.get("workflow").and_then(Value::as_str).filter(|w| !w.trim().is_empty()) {
        params["workflow"] = json!(workflow);
    }
    if let Some(seed) = p.get("seed").and_then(Value::as_u64) {
        params["seed"] = json!(seed);
    }
    // A scene keyframe can lean on the story library: the location plate first, then the sheets of
    // the characters the scene names (the reference order the Flux2-Klein workflows expect).
    if kind == "image" {
        let references = scene_references(&story, &scene);
        if !references.is_empty() {
            let hints: Vec<String> = references
                .iter()
                .enumerate()
                .map(
                    |(index, (name, _))| {
                        if name == "background" { format!("(image {}) is the background", index + 1) } else { format!("(image {}) is {name}", index + 1) }
                    },
                )
                .collect();
            let prompt = params["prompt"].as_str().unwrap_or_default().to_string();
            params["prompt"] = json!(format!("{}. {prompt}", hints.join("; ")));
            params["images"] = json!(references.iter().map(|(_, path)| path).collect::<Vec<_>>());
        }
    }
    // `wait` and the timeout are forwarded so a script (CLI, agent) can generate synchronously,
    // exactly like calling graphics.generateComfy directly.
    if let Some(wait) = p.get("wait").and_then(Value::as_bool) {
        params["wait"] = json!(wait);
    }
    if let Some(timeout) = p.get("timeoutSeconds").and_then(Value::as_u64) {
        params["timeoutSeconds"] = json!(timeout);
    }
    if let Some(dry) = p.get("dryRun").and_then(Value::as_bool) {
        params["dryRun"] = json!(dry);
    }
    if let Some(overrides) = p.get("overrides").filter(|overrides| overrides.is_array()) {
        params["overrides"] = overrides.clone();
    }
    if let Some(shot) = p.get("shot").and_then(Value::as_u64) {
        params["storyTarget"] = json!({"scene": scene_id, "shot": shot});
    }
    let mut result = s.execute("graphics.generateComfy", params)?;
    result["scene"] = json!(scene_id);
    result["kind"] = json!(kind);
    Ok(result)
}

/// The media item linked to a scene (keyframe preferred, else the video), or a shot's item.
fn scene_item(story: &Value, scene_id: u64, shot: Option<u64>) -> Option<u64> {
    let scene = story["scenes"].as_array()?.iter().find(|scene| scene["id"].as_u64() == Some(scene_id))?;
    if let Some(shot_id) = shot {
        return scene["shots"].as_array()?.iter().find(|shot| shot["id"].as_u64() == Some(shot_id))?.get("item").and_then(Value::as_u64);
    }
    for key in ["video", "image"] {
        if let Some(item) = scene[key].get("item").and_then(Value::as_u64) {
            return Some(item);
        }
    }
    None
}

/// `nl.story.extendAudio` — generate music for a scene/section and link it to the scene.
fn story_extend_audio(s: &mut Session, p: &Value) -> Result<Value> {
    let scene_id = p.get("scene").and_then(Value::as_u64).ok_or_else(|| bad_cmd("nl.story.extendAudio", "need `scene`"))?;
    let story = load(s);
    let scene = story["scenes"]
        .as_array()
        .and_then(|scenes| scenes.iter().find(|scene| scene["id"].as_u64() == Some(scene_id)))
        .cloned()
        .ok_or_else(|| bad_cmd("nl.story.extendAudio", format!("no scene {scene_id}")))?;
    let seconds = p
        .get("seconds")
        .and_then(Value::as_f64)
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or_else(|| scene["durationSec"].as_f64().unwrap_or(15.0))
        .clamp(3.0, 600.0);
    let prompt = p
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .or_else(|| {
            story["style"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|style| format!("{style}, underscore for a scene about: {}", scene["heading"].as_str().unwrap_or("the scene")))
        })
        .or_else(|| scene["heading"].as_str().map(str::trim).filter(|h| !h.is_empty()).map(|h| format!("cinematic underscore for {h}")))
        .ok_or_else(|| bad_cmd("nl.story.extendAudio", "the story has no style and the scene no heading: pass `prompt` for the music"))?;
    let mut params = json!({
        "prompt": prompt,
        "seconds": seconds.round() as u64,
        "storyTarget": {"scene": scene_id, "role": "audio"},
    });
    if let Some(workflow) = p.get("workflow").and_then(Value::as_str).filter(|w| !w.trim().is_empty()) {
        params["workflow"] = json!(workflow);
    }
    if let Some(wait) = p.get("wait").and_then(Value::as_bool) {
        params["wait"] = json!(wait);
    }
    let mut result = s.execute("audio.generateMusic", params)?;
    result["scene"] = json!(scene_id);
    result["seconds"] = json!(seconds);
    Ok(result)
}

/// `nl.story.placeAudio` — put a scene's generated music on an audio track.
fn story_place_audio(s: &mut Session, p: &Value) -> Result<Value> {
    let scene_id = p.get("scene").and_then(Value::as_u64).ok_or_else(|| bad_cmd("nl.story.placeAudio", "need `scene`"))?;
    let story = load(s);
    let scene = story["scenes"]
        .as_array()
        .and_then(|scenes| scenes.iter().find(|scene| scene["id"].as_u64() == Some(scene_id)))
        .cloned()
        .ok_or_else(|| bad_cmd("nl.story.placeAudio", format!("no scene {scene_id}")))?;
    let item =
        scene["audio"]["item"].as_u64().ok_or_else(|| bad_cmd("nl.story.placeAudio", "that scene has no generated music yet: extend the audio first"))?;
    let mut params = json!({"item": item, "audioTrack": p.get("track").and_then(Value::as_str).unwrap_or("A1"), "audio": true, "video": false});
    if let Some(seconds) = p.get("atSeconds").and_then(Value::as_f64) {
        params["time"] = json!(Tick::from_seconds_f64(seconds).0);
    }
    if let Some(seconds) = p.get("durationSeconds").and_then(Value::as_f64).filter(|s| *s > 0.0) {
        params["duration"] = json!(Tick::from_seconds_f64(seconds).0);
    }
    let result = s.execute("timeline.place", params)?;
    Ok(json!({"placed": result, "scene": scene_id, "item": item}))
}

/// `nl.story.place` — put a scene's generated media on the timeline.
fn story_place(s: &mut Session, p: &Value) -> Result<Value> {
    let scene_id = p.get("scene").and_then(Value::as_u64).ok_or_else(|| bad_cmd("nl.story.place", "need `scene`"))?;
    let story = load(s);
    let scene = story["scenes"]
        .as_array()
        .and_then(|scenes| scenes.iter().find(|scene| scene["id"].as_u64() == Some(scene_id)))
        .cloned()
        .ok_or_else(|| bad_cmd("nl.story.place", format!("no scene {scene_id}")))?;
    let shot = p.get("shot").and_then(Value::as_u64);
    let item =
        scene_item(&story, scene_id, shot).ok_or_else(|| bad_cmd("nl.story.place", "that scene has no generated media yet: generate the keyframe first"))?;
    let mut params = json!({"item": item, "track": p.get("track").and_then(Value::as_str).unwrap_or("V1")});
    if let Some(seconds) = p.get("atSeconds").and_then(Value::as_f64) {
        params["time"] = json!(Tick::from_seconds_f64(seconds).0);
    }
    if shot.is_none() && scene["video"].get("item").is_none() {
        let duration = scene["durationSec"].as_f64().unwrap_or(5.0);
        params["duration"] = json!({"seconds": duration});
    }
    let result = s.execute("timeline.place", params)?;
    Ok(json!({"placed": result, "scene": scene_id, "item": item}))
}

/// `nl.story.placeAll` — place every generated scene keyframe in order from a start point.
fn story_place_all(s: &mut Session, p: &Value) -> Result<Value> {
    let story = load(s);
    let start = p.get("startSeconds").and_then(Value::as_f64).unwrap_or_else(|| s.playhead().seconds());
    let track = p.get("track").and_then(Value::as_str).unwrap_or("V1").to_string();
    let mut at = start;
    let mut placed = Vec::new();
    for scene in story["scenes"].as_array().into_iter().flatten() {
        let Some(scene_id) = scene["id"].as_u64() else { continue };
        let Some(item) = scene_item(&story, scene_id, None) else {
            at += scene["durationSec"].as_f64().unwrap_or(5.0);
            continue;
        };
        let duration = scene["durationSec"].as_f64().unwrap_or(5.0);
        let params = json!({"item": item, "track": track, "time": Tick::from_seconds_f64(at).0, "duration": Tick::from_seconds_f64(duration).0});
        match s.execute("timeline.place", params) {
            Ok(value) => placed.push(json!({"scene": scene_id, "item": item, "at": at, "duration": duration, "placed": value})),
            Err(e) => placed.push(json!({"scene": scene_id, "item": item, "error": e.to_string()})),
        }
        at += duration;
    }
    if placed.iter().all(|p| p.get("error").is_some()) {
        return Err(bad_cmd("nl.story.placeAll", "no scene had generated media to place: generate keyframes first"));
    }
    s.toast(format!("NL story: placed {} scene(s) on the timeline", placed.iter().filter(|p| p.get("error").is_none()).count()));
    Ok(json!({"placed": placed, "startSeconds": start, "endSeconds": at}))
}

/// Move a finished draft into the story and report it.
pub(crate) fn poll(s: &mut Session) {
    let finished =
        s.nl.story_pending
            .as_ref()
            .is_none_or(|pending| s.jobs.iter().find(|j| j.id == pending.job).is_none_or(|j| j.progress.finished.load(std::sync::atomic::Ordering::Relaxed)));
    if !finished {
        return;
    }
    let Some(pending) = s.nl.story_pending.take() else { return };
    let Some(result) = pending.result.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
    if pending.epoch != s.document_epoch {
        s.error_toast("nl.story.draft", "the project was replaced; the drafted story was not applied");
        return;
    }
    match result {
        Ok(story) => {
            let scenes = story["scenes"].as_array().map_or(0, Vec::len);
            let saved = save(s, &story);
            s.toast(format!("NL story: {scenes} scene(s) drafted{}", if saved["saved"] == true { "" } else { " (session only)" }));
        }
        Err(e) => s.error_toast("nl.story.draft", format!("NL story: {e}")),
    }
}

/// Link a generated media item to the scene/shot that asked for it (called after import).
pub(crate) fn assign(s: &mut Session, target: &Value, item: filmcraft_project::ItemId, path: &str) {
    let link = json!({"item": item.0, "path": path});
    let role = target.get("role").and_then(Value::as_str).unwrap_or_default();
    // Reference images belong to the story's character/location library, not to a scene.
    if matches!(role, "sheet" | "plate") {
        let index = target.get("character").or_else(|| target.get("location")).and_then(Value::as_u64).unwrap_or(0);
        if index == 0 {
            return;
        }
        let mut story = load(s);
        let key = if role == "sheet" { "characters" } else { "locations" };
        let field = if role == "sheet" { "sheet" } else { "plate" };
        if let Some(entries) = story[key].as_array_mut()
            && let Some(entry) = entries.get_mut(index as usize - 1)
        {
            entry[field] = link;
            let _ = save(s, &story);
        }
        return;
    }
    let Some(scene_id) = target.get("scene").and_then(Value::as_u64) else { return };
    let shot_id = target.get("shot").and_then(Value::as_u64);
    let mut story = load(s);
    let Some(scenes) = story["scenes"].as_array_mut() else { return };
    let Some(scene) = scenes.iter_mut().find(|scene| scene["id"].as_u64() == Some(scene_id)) else { return };
    if role == "audio" {
        scene["audio"] = link;
    } else if let Some(shot_id) = shot_id {
        if let Some(shots) = scene["shots"].as_array_mut()
            && let Some(shot) = shots.iter_mut().find(|shot| shot["id"].as_u64() == Some(shot_id))
        {
            shot["item"] = json!(item.0);
            shot["path"] = json!(path);
        }
    } else if path.to_ascii_lowercase().ends_with(".mp4") || path.to_ascii_lowercase().ends_with(".webm") || path.to_ascii_lowercase().ends_with(".mov") {
        scene["video"] = link;
    } else {
        scene["image"] = link;
    }
    let _ = save(s, &story);
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    let spec = |id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>| -> CommandSpec {
        CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: always, run, journal: false, named: crate::commands::NamedKeys::NONE }
    };
    vec![
        spec("nl.story.get", "NL Story", "{}", story_get),
        spec("nl.story.set", "NL Story Save", r#"{"story":obj}"#, story_set),
        spec("nl.story.update", "NL Story Scene", r#"{"scene":int,"fields":obj}"#, story_update),
        spec("nl.story.draft", "NL Story Draft", r#"{"idea":str,"language":str?,"model":str?,"wait":bool?}"#, story_draft),
        spec("nl.story.refine", "NL Story Refine", r#"{"scene":int,"instructions":str,"language":str?,"model":str?}"#, story_refine),
        spec(
            "nl.story.generate",
            "NL Story Keyframe",
            r#"{"scene":int,"shot":int?,"kind":"image|video"?,"prompt":str?,"workflow":str?,"seed":int?"}"#,
            story_generate,
        ),
        spec(
            "nl.story.sheet",
            "NL Story Character Sheet",
            r#"{"character":int|str,"prompt":str?,"workflow":str?,"seed":int?,"wait":bool?,"dryRun":bool?}"#,
            story_sheet,
        ),
        spec(
            "nl.story.plate",
            "NL Story Location Plate",
            r#"{"location":int|str,"prompt":str?,"workflow":str?,"seed":int?,"wait":bool?,"dryRun":bool?}"#,
            story_plate,
        ),
        spec("nl.story.shotmap", "NL Story Shot Map", r#"{"scene":int?,"shotsPerScene":int?"}"#, story_shotmap),
        spec("nl.story.place", "NL Story Place", r#"{"scene":int,"shot":int?,"track":str?,"atSeconds":f?}"#, story_place),
        spec("nl.story.placeAll", "NL Story Place All", r#"{"startSeconds":f?,"track":str?}"#, story_place_all),
        spec("nl.story.extendAudio", "NL Story Music", r#"{"scene":int,"seconds":f?,"prompt":str?,"workflow":str?,"wait":bool?}"#, story_extend_audio),
        spec("nl.story.placeAudio", "NL Story Place Music", r#"{"scene":int,"track":str?,"atSeconds":f?,"durationSeconds":f?}"#, story_place_audio),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_story_is_sanitised_and_numbered() {
        let story = sanitize(&json!({
            "title": "  Prova  ",
            "scenes": [
                {"heading": "INT", "action": "x", "durationSec": 999, "imagePrompt": "a room"},
                {"heading": "EXT", "durationSec": "bad", "shots": [{"description": "wide"}]}
            ],
            "characters": [{"name": "Ada", "description": "pilot"}, {"description": "nameless"}]
        }))
        .unwrap();
        assert_eq!(story["title"], "Prova");
        assert_eq!(story["scenes"][0]["id"], 1);
        let scenes = story["scenes"].as_array().unwrap();
        assert_eq!(scenes.len(), 2);
        assert_eq!(scenes[0]["durationSec"], 120.0, "clamped");
        assert_eq!(scenes[1]["durationSec"], 5.0, "default when unusable");
        assert_eq!(scenes[1]["shots"].as_array().unwrap().len(), 1);
        assert_eq!(story["characters"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_story_without_scenes_is_refused_by_the_draft_call_but_sanitises_empty() {
        let story = sanitize(&json!({"title": "vuota"})).unwrap();
        assert_eq!(story["scenes"].as_array().unwrap().len(), 0);
        assert_eq!(story["title"], "vuota");
    }

    #[test]
    fn placing_needs_generated_media() {
        let mut s = crate::Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        s.nl.story = Some(json!({"scenes": [{"id": 1, "durationSec": 5}]}));
        let e = s.execute("nl.story.place", json!({"scene": 1})).unwrap_err().to_string();
        assert!(e.contains("no generated media"), "{e}");
    }

    #[test]
    fn the_story_keeps_links_and_scene_locations() {
        let story = sanitize(&json!({
            "characters": [{"name": "Ada", "description": "pilot", "sheet": {"item": 7, "path": "C:/a.png"}}],
            "locations": [{"name": "Studio", "description": "room", "plate": {"item": 8, "path": "C:/b.png"}}],
            "scenes": [{"heading": "INT", "location": "Studio", "durationSec": 4, "sheet": {"item": 9}}]
        }))
        .unwrap();
        assert_eq!(story["characters"][0]["sheet"]["item"], 7);
        assert_eq!(story["locations"][0]["plate"]["item"], 8);
        assert_eq!(story["scenes"][0]["location"], "Studio");
    }

    #[test]
    fn scene_references_put_the_background_first_then_the_characters_named() {
        let story = json!({
            "style": "noir",
            "characters": [
                {"name": "Ada", "sheet": {"path": "C:/ada.png"}},
                {"name": "Bruno", "sheet": {"path": "C:/bruno.png"}},
                {"name": "Carla", "sheet": {"path": "C:/carla.png"}}
            ],
            "locations": [{"name": "Studio", "plate": {"path": "C:/studio.png"}}],
            "scenes": [{"heading": "INT Studio", "location": "studio", "action": "Ada parla con Bruno", "imagePrompt": "wide"}]
        });
        let scene = &story["scenes"][0];
        let references = scene_references(&story, scene);
        assert_eq!(references[0], ("background".to_string(), "C:/studio.png".to_string()));
        assert_eq!(references.len(), 3, "background + the two named characters, bounded to three");
        assert!(references.iter().any(|(name, path)| name == "Ada" && path == "C:/ada.png"));
        assert_eq!(library_index(&story, "characters", &json!("ada")), Some(0));
        assert_eq!(library_index(&story, "characters", &json!(2)), Some(1));
        assert_eq!(library_index(&story, "characters", &json!(9)), None);
    }

    #[test]
    fn a_video_scene_uses_the_keyframe_and_optionally_the_character_sheet() {
        // paths must exist on disk: use the test binary's own path as a stand-in for both files
        let here = std::env::current_exe().unwrap().to_string_lossy().to_string();
        let story = json!({"characters": [{"name": "Ada", "sheet": {"path": here}}], "scenes": []});
        let scene = json!({"heading": "Ada", "image": {"path": here}, "imagePrompt": "close"});
        assert_eq!(video_reference_images(&story, &scene, false).len(), 1);
        assert_eq!(video_reference_images(&story, &scene, true).len(), 1, "the same file is not added twice");
        let story = json!({"characters": [{"name": "Ada", "sheet": {"path": here}}], "scenes": []});
        let scene = json!({"heading": "Ada", "imagePrompt": "close"});
        assert_eq!(video_reference_images(&story, &scene, true).len(), 1, "without keyframe the sheet is the reference");
    }

    #[test]
    fn sheet_and_plate_need_a_known_character_or_location() {
        let mut s = crate::Session::default();
        assert!(s.execute("nl.story.sheet", json!({})).is_err());
        assert!(s.execute("nl.story.plate", json!({})).is_err());
        s.nl.story = Some(json!({"characters": [{"name": "Ada"}], "locations": [{"name": "Studio"}]}));
        assert!(s.execute("nl.story.sheet", json!({"character": "Nope"})).is_err());
    }

    #[test]
    fn the_shot_map_breaks_scenes_into_camera_shots() {
        let mut s = crate::Session::default();
        s.nl.story = Some(json!({"scenes": [{"id": 1, "durationSec": 6, "imagePrompt": "a room", "heading": "INT", "action": "Ada entra"}]}));
        let report = s.execute("nl.story.shotmap", json!({"scene": 1, "shotsPerScene": 3})).unwrap();
        assert_eq!(report["shots"], 3);
        let story = s.nl.story.clone().unwrap();
        let shots = story["scenes"][0]["shots"].as_array().unwrap();
        assert_eq!(shots.len(), 3);
        assert!(shots.iter().all(|shot| shot["camera"].as_str().is_some_and(|camera| !camera.is_empty())));
        assert!(shots.iter().all(|shot| shot["imagePrompt"].as_str().unwrap().contains("a room")));
        let total: f64 = shots.iter().map(|shot| shot["durationSec"].as_f64().unwrap()).sum();
        assert!((total - 6.0).abs() < 0.2, "{total}");
    }

    #[test]
    fn audio_extension_needs_a_scene_and_a_prompt_source() {
        let mut s = crate::Session::default();
        s.nl.story = Some(json!({"scenes": [{"id": 1, "durationSec": 8}]}));
        assert!(s.execute("nl.story.extendAudio", json!({"scene": 9})).is_err(), "unknown scene");
        // Without a style term or a scene heading there is no music prompt to derive: refused
        // before any ComfyUI call, so this test never contacts a service.
        assert!(s.execute("nl.story.extendAudio", json!({"scene": 1})).is_err());
        let e = s.execute("nl.story.placeAudio", json!({"scene": 1})).unwrap_err().to_string();
        assert!(e.contains("no generated music"), "{e}");
    }

    #[test]
    fn updating_an_unknown_field_or_scene_is_refused() {
        let mut s = crate::Session::default();
        s.nl.story = Some(json!({"scenes": [{"id": 1, "durationSec": 5}]}));
        assert!(s.execute("nl.story.update", json!({"scene": 9, "fields": {"action": "x"}})).is_err());
        assert!(s.execute("nl.story.update", json!({"scene": 1, "fields": {"nope": "x"}})).is_err());
        let ok = s.execute("nl.story.update", json!({"scene": 1, "fields": {"action": "un'azione"}})).unwrap();
        assert_eq!(ok["story"]["scenes"][0]["action"], "un'azione");
    }
}
