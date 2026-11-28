//! Local generative studio: text-to-video from zero and "bridge" clips that fill the gap between two
//! shots by generating a transition from the last frame of the outgoing clip to the first frame of
//! the incoming one.
//!
//! Everything runs on the operator's ComfyUI: the workflows are the ones already in `workflows/`
//! (`video_minimax_h3_t2v.json` for text, its MiniMax H3 node with `first_frame`/`last_frame` for the
//! bridge). No service is started or downloaded; a missing workflow is an explicit error.
//!
//! The bridge exports the two frames from the program at the exact timeline times, uploads them,
//! submits the first/last-frame workflow, imports the result and places it in the gap at the outgoing
//! clip's end, trimmed to the gap length.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use filmcraft_time::Tick;
use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{EngineError, Result, Session};

/// Text-to-video workflow added for the NL suite; its node supports optional `first_frame` and
/// `last_frame` inputs, which the bridge fills with the two timeline frames.
const TEXT_WORKFLOW: &str = "workflows/video_minimax_h3_t2v.json";
/// Fallback bridge workflows when the text/flf one is unavailable.
const R2V_WORKFLOW: &str = "workflows/video/reference-to-video/minimax_h3_video_reference.json";
/// Extend workflow: video + two reference frames, used by the v2v extension.
const EXTEND_WORKFLOW: &str = "workflows/video/edit/MiniMax-H3_-_V2V_Extend_Any_Video_latent_masking.json";
const I2V_WORKFLOW: &str = "workflows/video/text-to-video/video_minimax_h3_t2v.json";
/// A bridge shorter than this is not worth a generation.
const MIN_GAP: f64 = 0.25;

fn bad_cmd(cmd: &str, msg: impl ToString) -> EngineError {
    bad(cmd, msg.to_string())
}

fn data_dir() -> PathBuf {
    crate::autosave::default_data_dir().unwrap_or_else(std::env::temp_dir)
}

/// A workflow under the suite root, when it exists.
fn suite_workflow(s: &Session, relative: &str) -> Option<PathBuf> {
    let _ = s;
    crate::nl::suite_root().map(|root| root.join(relative)).filter(|path| path.is_file())
}

/// The workflow named by the command or the first existing default.
fn choose_workflow(s: &Session, p: &Value, defaults: &[&str]) -> Result<PathBuf> {
    if let Some(path) = p.get("workflow").and_then(Value::as_str).map(str::trim).filter(|path| !path.is_empty()) {
        let path = Path::new(path);
        let resolved = if path.is_absolute() { path.to_path_buf() } else { crate::nl::suite_root().unwrap_or_default().join(path) };
        if resolved.is_file() {
            return Ok(resolved);
        }
        return Err(bad_cmd("nl.generate", format!("workflow not found: {}", resolved.display())));
    }
    for relative in defaults {
        if let Some(path) = suite_workflow(s, relative) {
            return Ok(path);
        }
    }
    Err(bad_cmd("nl.generate", format!("no workflow found at {}", defaults.join(" or "))))
}

/// `nl.generate.text` — create a video from a prompt alone (text-to-video) and place it.
fn generate_text(s: &mut Session, p: &Value) -> Result<Value> {
    let prompt = p
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .ok_or_else(|| bad_cmd("nl.generate.text", "need `prompt`"))?
        .to_string();
    let workflow = choose_workflow(s, p, &[TEXT_WORKFLOW])?;
    let seconds = p.get("seconds").and_then(Value::as_u64).unwrap_or(4).clamp(1, 30);
    let mut params = json!({
        "workflow": workflow.to_string_lossy(),
        "prompt": prompt,
        "seconds": seconds,
        "resolutionMode": "video",
        "kind": "video",
    });
    for key in ["wait", "dryRun", "seed", "overrides", "timeoutSeconds", "negative"] {
        if let Some(value) = p.get(key) {
            params[key] = value.clone();
        }
    }
    // Video generation on a small GPU can take longer than the interactive default; do not cut a
    // finished render off just before ComfyUI writes it.
    params["timeoutSeconds"] = json!(p.get("timeoutSeconds").and_then(Value::as_u64).unwrap_or(3600).clamp(120, 7200));
    let at = match p.get("atSeconds").and_then(Value::as_f64) {
        Some(seconds) if seconds.is_finite() => Tick::from_seconds_f64(seconds.max(0.0)),
        _ => s.playhead(),
    };
    params["place"] = json!({
        "track": p.get("track").and_then(Value::as_str).unwrap_or("V1"),
        "time": at.0,
        "duration": Tick::from_seconds_f64(seconds as f64).0,
        "video": true,
        "audio": true,
    });
    s.nl.results.remove("comfy.last_place");
    let mut result = s.execute("graphics.generateComfy", params)?;
    if let Some(placement) = s.nl.results.get("comfy.last_place").cloned() {
        result["placement"] = placement;
    }
    if let Some(last) = s.nl.results.get("comfy.last").cloned() {
        result["imported"] = last;
    }
    result["mode"] = json!("text2video");
    result["workflow"] = json!(workflow.to_string_lossy());
    result["place"] = json!({"track": p.get("track").and_then(Value::as_str).unwrap_or("V1"), "atSeconds": at.seconds()});
    Ok(result)
}

/// The gap between two consecutive clips of one video track.
#[derive(Clone, Debug)]
struct Gap {
    track: String,
    /// The `timeline.place` track name (`V1`, `V2`), which is not the display name ("Video 1").
    track_param: String,
    start: Tick,
    end: Tick,
    outgoing: u64,
    incoming: u64,
    outgoing_end: Tick,
    incoming_start: Tick,
}

impl Gap {
    fn seconds(&self) -> f64 {
        (self.end.0 - self.start.0) as f64 / filmcraft_time::TICKS_PER_SECOND as f64
    }
}

/// Find the gap between two clips: explicit ids, or the empty space around the playhead.
fn find_gap(s: &Session, p: &Value) -> Result<Gap> {
    let sequence = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let track_name = p.get("track").and_then(Value::as_str).unwrap_or("V1");
    let explicit = match (p.get("outgoing").and_then(Value::as_u64), p.get("incoming").and_then(Value::as_u64)) {
        (Some(outgoing), Some(incoming)) => Some((outgoing, incoming)),
        (None, None) => None,
        _ => return Err(bad_cmd("nl.bridge.plan", "give both `outgoing` and `incoming` clip ids, or neither")),
    };
    // With explicit clip ids the track is whichever one holds them (the panel passes ids from the
    // selection, whose track name may not be "V1").
    let track = match explicit {
        Some((outgoing, incoming)) => sequence
            .video_tracks
            .iter()
            .find(|track| track.items.iter().any(|clip| clip.id.0 == outgoing) && track.items.iter().any(|clip| clip.id.0 == incoming))
            .ok_or_else(|| bad_cmd("nl.bridge.plan", format!("clips {outgoing} and {incoming} are not on the same video track")))?,
        None => sequence
            .video_tracks
            .iter()
            .find(|track| track.name == track_name || track.name.eq_ignore_ascii_case(track_name))
            .or_else(|| sequence.video_tracks.first())
            .ok_or_else(|| bad_cmd("nl.bridge.plan", "the sequence has no video track"))?,
    };
    let mut clips: Vec<&filmcraft_project::TrackItem> = track.items.iter().collect();
    clips.sort_by_key(|clip| clip.start.0);
    let (outgoing, incoming) = match explicit {
        Some(ids) => {
            let outgoing = clips
                .iter()
                .find(|clip| clip.id.0 == ids.0)
                .copied()
                .ok_or_else(|| bad_cmd("nl.bridge.plan", format!("no clip {} on track {}", ids.0, track.name)))?;
            let incoming = clips
                .iter()
                .find(|clip| clip.id.0 == ids.1)
                .copied()
                .ok_or_else(|| bad_cmd("nl.bridge.plan", format!("no clip {} on track {}", ids.1, track.name)))?;
            (outgoing, incoming)
        }
        None => {
            let playhead = s.playhead();
            let mut found = None;
            for pair in clips.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let end = Tick(a.start.0 + a.duration.0);
                if end <= playhead && playhead <= b.start && b.start.0 - end.0 > 0 {
                    found = Some((a, b));
                    break;
                }
            }
            found.ok_or_else(|| {
                bad_cmd(
                    "nl.bridge.plan",
                    "no gap at the playhead: put the playhead in the empty space between two clips, or pass `outgoing` and `incoming` clip ids",
                )
            })?
        }
    };
    let outgoing_end = Tick(outgoing.start.0 + outgoing.duration.0);
    let incoming_start = incoming.start;
    if incoming_start.0 <= outgoing_end.0 {
        return Err(bad_cmd("nl.bridge.plan", "the incoming clip starts before the outgoing clip ends: there is no gap to fill"));
    }
    let track_index = sequence.video_tracks.iter().position(|candidate| std::ptr::eq(candidate, track)).unwrap_or(0);
    let gap = Gap {
        track: track.name.clone(),
        track_param: format!("V{}", track_index + 1),
        start: outgoing_end,
        end: incoming_start,
        outgoing: outgoing.id.0,
        incoming: incoming.id.0,
        outgoing_end,
        incoming_start,
    };
    if gap.seconds() < MIN_GAP {
        return Err(bad_cmd("nl.bridge.plan", format!("the gap is only {:.2} s; move the clips apart or extend the clip", gap.seconds())));
    }
    Ok(gap)
}

/// One PNG frame of the program at a sequence time.
fn export_frame(s: &mut Session, path: &Path, time: Tick) -> Result<Value> {
    s.execute("file.exportFrame", json!({"target": "program", "format": "png", "path": path.to_string_lossy(), "time": time.0}))
}

/// A short visual description of one exported bridge frame, used to build the default prompt.
/// Best effort: a missing vision model leaves the structured fallback text in place.
fn describe_bridge_frame(s: &mut Session, p: &Value, path: &Path) -> Option<String> {
    let png = std::fs::read(path).ok()?;
    let endpoint = super::analysis::vision_endpoint(s, p);
    let first = p
        .get("visionModel")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|model| !model.trim().is_empty())
        .or_else(|| Some(s.prefs.nl.role_model("vision")).filter(|model| !model.is_empty()))
        .or_else(|| Some(s.prefs.nl.vision_model.trim().to_string()).filter(|model| !model.is_empty()))
        .or_else(|| Some(s.prefs.nl.ollama_model_vision()).filter(|model| !model.trim().is_empty()))?;
    let mut candidates = vec![first];
    if let Ok(declared) = super::analysis::first_model(&endpoint) {
        if !candidates.iter().any(|model| model == &declared) {
            candidates.push(declared);
        }
    }
    if !candidates.iter().any(|model| model.starts_with("lfm3")) {
        candidates.push("lfm3-omni:latest".into());
    }
    for model in candidates {
        match super::analysis::ask_vision(
            &endpoint,
            &model,
            &png,
            "Describe this video frame for a continuity transition: subject, pose, action, environment, lighting, camera and colour. One concise English sentence, no guesses.",
        ) {
            Ok(text) if !text.trim().is_empty() => return Some(text.trim().trim_end_matches([',', '.', '!', '?']).chars().take(500).collect()),
            Ok(_) => {}
            Err(error) => eprintln!("bridge vision failed: endpoint={endpoint} model={model} error={error}"),
        }
    }
    None
}

/// The two frames of a bridge, plus the outgoing audio tail (audio reference when the workflow
/// supports it), exported under the suite data folder and kept for the receipt.
fn export_bridge_frames(s: &mut Session, gap: &Gap) -> Result<(PathBuf, PathBuf, Option<PathBuf>)> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_millis()).unwrap_or(0);
    let dir = data_dir().join("nl").join("bridge").join(format!("{stamp}"));
    std::fs::create_dir_all(&dir).map_err(|e| bad_cmd("nl.bridge.plan", format!("{}: {e}", dir.display())))?;
    let one_frame = s.active_sequence().map(|sequence| sequence.settings.frame_rate.tick_of(1)).unwrap_or(Tick(1)).0.max(1);
    let last = dir.join("outgoing-last-frame.png");
    let first = dir.join("incoming-first-frame.png");
    export_frame(s, &last, Tick(gap.outgoing_end.0.saturating_sub(one_frame).max(0)))?;
    export_frame(s, &first, gap.incoming_start)?;
    // Last two seconds before the gap: this is the sound the bridge has to continue.
    let audio = dir.join("outgoing-audio.wav");
    let end = gap.start.0 as f64 / 254_016_000_000.0;
    let start = (end - 2.0).max(0.0);
    let exported = s.execute(
        "file.exportMedia",
        json!({"path": audio.to_string_lossy(), "format": "wav", "range": "custom", "startSeconds": start, "endSeconds": end, "audio": true, "wait": true}),
    );
    let audio = exported.is_ok().then_some(audio).filter(|path| path.is_file());
    Ok((last, first, audio))
}

/// The first/last-frame workflow: the text-to-video graph plus two LoadImage nodes wired to the H3
/// node's optional `first_frame`/`last_frame` inputs.
fn bridge_workflow(s: &Session, t2v: &Path) -> Result<PathBuf> {
    let text = std::fs::read_to_string(t2v).map_err(|e| bad_cmd("nl.bridge.generate", format!("{}: {e}", t2v.display())))?;
    let mut workflow: Value = serde_json::from_str(&text).map_err(|e| bad_cmd("nl.bridge.generate", format!("{}: {e}", t2v.display())))?;
    let Some(nodes) = workflow.as_object_mut() else { return Err(bad_cmd("nl.bridge.generate", "the workflow is not a JSON object")) };
    let Some((video_id, _)) = nodes.iter().find(|(_, node)| node.get("class_type").and_then(Value::as_str) == Some("MiniMaxH3ImageToVideo")) else {
        return Err(bad_cmd("nl.bridge.generate", "the text workflow has no MiniMax H3 video node to bridge with"));
    };
    let video_id = video_id.clone();
    // Numeric ids let the reference staging order them (last frame first, first frame second).
    nodes.insert("7001".into(), json!({"class_type": "LoadImage", "inputs": {"image": ""}, "_meta": {"title": "Bridge: outgoing last frame"}}));
    nodes.insert("7002".into(), json!({"class_type": "LoadImage", "inputs": {"image": ""}, "_meta": {"title": "Bridge: incoming first frame"}}));
    let node = nodes.get_mut(&video_id).ok_or_else(|| bad_cmd("nl.bridge.generate", "the video node disappeared"))?;
    let inputs = node.get_mut("inputs").and_then(Value::as_object_mut).ok_or_else(|| bad_cmd("nl.bridge.generate", "the video node has no inputs"))?;
    inputs.insert("first_frame".into(), json!(["7001", 0]));
    inputs.insert("last_frame".into(), json!(["7002", 0]));
    let dir = data_dir().join("nl").join("workflows");
    std::fs::create_dir_all(&dir).map_err(|e| bad_cmd("nl.bridge.generate", format!("{}: {e}", dir.display())))?;
    let path = dir.join("bridge-first-last-frame.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&workflow).unwrap_or_default())
        .map_err(|e| bad_cmd("nl.bridge.generate", format!("{}: {e}", path.display())))?;
    let _ = s;
    Ok(path)
}

/// The transition requested for the bridge (`cut` or `dissolve`).
fn bridge_transition(p: &Value) -> String {
    p.get("transition").and_then(Value::as_str).map(str::to_ascii_lowercase).filter(|transition| transition == "dissolve").unwrap_or_else(|| "cut".into())
}

/// A MiniMax H3-aligned bridge prompt built from the real frames. FL2VA/I2VA use the base
/// three-field schema; the reference workflow uses the full-reference six-section schema so the
/// two frames and the outgoing audio are explicitly labelled.
fn bridge_prompt(p: &Value, mode: &str, seconds: f64, last_desc: Option<&str>, first_desc: Option<&str>, has_audio: bool) -> String {
    if let Some(prompt) = p.get("prompt").and_then(Value::as_str).map(str::trim).filter(|prompt| !prompt.is_empty()) {
        return prompt.to_string();
    }
    let last = last_desc.unwrap_or("the outgoing shot's final composition, pose and lighting");
    let first = first_desc.unwrap_or("the incoming shot's opening composition, pose and lighting");
    if mode == "i2v" {
        return format!(
            "I2VA: For the target video, at 0.00 seconds into the target video, <Picture 1> (from [Shot 1]) is fully referenced.\n\n\
             integrated_multimodal_description: [Shot 1] Begin exactly on {last}. The subject continues its current action with natural, continuous motion; the camera holds or moves slightly as the action resolves into {first}. Preserve identity, wardrobe, colours, environment, lighting and lens character from the reference frame.\n\n\
             overall_soundscape: Natural diegetic room tone and the subject's breathing and cloth movement continue across the shot, with no sudden audio cut.\n\n\
             non_diegetic_music: N/A"
        );
    }
    if mode == "flf" {
        return format!(
            "FL2VA: How the reference pictures align with the target video — Picture 1 (from Shot 1) aligns with the 0.00-second mark of the target video; Picture 2 (from Shot 1) aligns with the {seconds:.2}-second mark of the target video.\n\n\
             integrated_multimodal_description: [Shot 1] One continuous shot. Begin exactly on {last}; the subject continues its action with natural, continuous motion while the camera holds or makes a restrained push. The motion resolves smoothly and lands exactly on {first} at the end, with identity, wardrobe, environment, lighting and colour preserved.\n\n\
             overall_soundscape: The outgoing ambience and physical sounds continue without a cut, matching the visual action and fading naturally into the incoming room tone.\n\n\
             non_diegetic_music: N/A"
        );
    }
    let audio_def = if has_audio {
        "\n<Audio 1> is the outgoing audio tail: ambience, voice and physical sounds from the last seconds before the gap."
    } else {
        ""
    };
    let audio_retention = if has_audio {
        "\n<Audio 1>: reference — Continue the outgoing sound texture across the bridge without inventing new dialogue."
    } else {
        ""
    };
    let audio_sound = if has_audio {
        "Continue <Audio 1>'s ambience and physical sounds without a cut, then blend into the incoming room tone."
    } else {
        "The outgoing ambience and physical sounds continue without a cut and blend into the incoming room tone."
    };
    format!(
        "subject_definitions:\n\
         <Picture 1> is the outgoing shot's last frame: {last}.\n\
         <Picture 2> is the incoming shot's first frame: {first}.{audio_def}\n\n\
         summary: [reference generation] The target video is a continuous bridge that starts on <Picture 1>, develops the outgoing action, and lands exactly on <Picture 2> after {seconds:.2} seconds.\n\n\
         retention_analysis:\n\
         <Picture 1>: fully_preserved — The opening frame matches <Picture 1> exactly.\n\
         <Picture 2>: fully_preserved — The final frame matches <Picture 2> exactly.{audio_retention}\n\n\
         detailed_description:\n\
         [Shot 1] Begin exactly on <Picture 1>; the subject continues the current action with natural, continuous motion, keeping identity, wardrobe, environment, lighting and lens character. The camera remains steady or makes a restrained push as the action resolves. Land exactly on <Picture 2> at the end; no cut, no text, no logos, no overlays.\n\n\
         overall_soundscape: {audio_sound}\n\n\
         non_diegetic_music: N/A"
    )
}

/// A video clip on the timeline, with the track name used by `timeline.place`.
struct TimelineClip {
    item: u64,
    name: String,
    start: Tick,
    end: Tick,
    track_param: String,
    track_name: String,
}

/// The clip named by `clip`, or the first selected clip. Video tracks only.
fn find_video_clip(s: &Session, p: &Value) -> Result<TimelineClip> {
    let sequence = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let wanted = p.get("clip").and_then(Value::as_u64).or_else(|| s.state.selection.first().map(|clip| clip.0));
    let wanted = wanted.ok_or_else(|| bad_cmd("nl.generate.extend", "select a video clip in the timeline or pass `clip`"))?;
    for (index, track) in sequence.video_tracks.iter().enumerate() {
        for clip in &track.items {
            if clip.id.0 == wanted {
                return Ok(TimelineClip {
                    item: clip.item.0,
                    name: clip.name.clone(),
                    start: clip.start,
                    end: Tick(clip.start.0 + clip.duration.0),
                    track_param: format!("V{}", index + 1),
                    track_name: track.name.clone(),
                });
            }
        }
    }
    Err(bad_cmd("nl.generate.extend", format!("clip {wanted} is not on a video track")))
}

/// `nl.generate.extend` — extend a clip with a locally generated continuation conditioned on the
/// clip itself (v2v extend): a short h264 excerpt of the clip end, two frames of that excerpt and
/// the extension workflow, then the result is placed right after the clip in the empty space.
fn generate_extend(s: &mut Session, p: &Value) -> Result<Value> {
    let clip = find_video_clip(s, p)?;
    let seconds = p.get("seconds").and_then(Value::as_f64).unwrap_or(2.0).clamp(0.5, 8.0);
    let context_seconds = p.get("contextSeconds").and_then(Value::as_f64).unwrap_or(2.5).clamp(1.0, 8.0);
    // Space after the clip: the next clip on the same track (if any) bounds the placement.
    let sequence = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let track =
        sequence.video_tracks.iter().find(|track| track.name == clip.track_name).ok_or_else(|| bad_cmd("nl.generate.extend", "the clip track disappeared"))?;
    let mut next_start: Option<Tick> = None;
    for other in &track.items {
        if other.id.0 != clip.item && other.start.0 >= clip.end.0 {
            next_start = Some(next_start.map_or(other.start, |current| current.min(other.start)));
        }
    }
    let available = next_start.map(|next| Tick(next.0 - clip.end.0)).unwrap_or(Tick::from_seconds_f64(seconds));
    if next_start.is_some() && available.0 <= 0 {
        return Err(bad_cmd("nl.generate.extend", "no empty space after the clip: move the following clip or use the bridge to fill the gap"));
    }
    let placement = Tick::from_seconds_f64(seconds).min(available);
    if placement.0 <= 0 {
        return Err(bad_cmd("nl.generate.extend", "no room to place the extension"));
    }
    let context_start = Tick((clip.end.0 - Tick::from_seconds_f64(context_seconds).0).max(clip.start.0));
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_millis()).unwrap_or(0);
    let dir = data_dir().join("nl").join("extend").join(format!("{stamp}"));
    std::fs::create_dir_all(&dir).map_err(|e| bad_cmd("nl.generate.extend", format!("{}: {e}", dir.display())))?;
    let clip_path = dir.join("context.mp4");
    s.execute(
        "file.exportMedia",
        json!({
            "path": clip_path.to_string_lossy(),
            "format": "h264",
            "range": "custom",
            "startSeconds": context_start.seconds(),
            "endSeconds": clip.end.seconds(),
            "wait": true,
            "crf": 26,
            "audio": true,
        }),
    )?;
    let first_frame = dir.join("context-first.png");
    let last_frame = dir.join("context-last.png");
    export_frame(s, &first_frame, context_start)?;
    export_frame(s, &last_frame, Tick(clip.end.0.saturating_sub(1).max(clip.start.0)))?;
    let workflow = choose_workflow(s, p, &[EXTEND_WORKFLOW])?;
    let prompt = p
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            "Continue the action after the end of this clip: smooth continuous motion, consistent lighting, camera, colour and style; the shot flows without a cut. No text, subtitles, logos or overlays."
                .to_string()
        });
    let mut params = json!({
        "workflow": workflow.to_string_lossy(),
        "prompt": prompt,
        "video": clip_path.to_string_lossy(),
        "images": [first_frame.to_string_lossy(), last_frame.to_string_lossy()],
        "seconds": seconds.ceil() as u64,
        "resolutionMode": "video",
        "kind": "video",
        "place": {
            "track": clip.track_param,
            "time": clip.end.0,
            "duration": placement.0,
            "video": true,
            "audio": true,
        },
    });
    for key in ["wait", "dryRun", "seed", "overrides", "timeoutSeconds", "negative"] {
        if let Some(value) = p.get(key) {
            params[key] = value.clone();
        }
    }
    params["timeoutSeconds"] = json!(p.get("timeoutSeconds").and_then(Value::as_u64).unwrap_or(3600).clamp(120, 7200));
    s.nl.results.remove("comfy.last_place");
    if let Some(transition) = p.get("transition") {
        params["place"]["transition"] = transition.clone();
    }
    let generation = s.execute("graphics.generateComfy", params)?;
    Ok(json!({
        "clip": clip.item,
        "clipName": clip.name,
        "track": clip.track_param,
        "contextSeconds": context_seconds,
        "contextVideo": clip_path.to_string_lossy(),
        "frames": [first_frame.to_string_lossy(), last_frame.to_string_lossy()],
        "seconds": seconds,
        "placementSeconds": placement.seconds(),
        "workflow": workflow.to_string_lossy(),
        "generation": generation,
        "placement": s.nl.results.get("comfy.last_place").cloned(),
    }))
}

/// Apply a dissolve/constant-power transition at both edges of the placed bridge clips. Returns the
/// number of video transitions applied; audio transitions are best-effort.
pub(crate) fn apply_place_transitions(s: &mut Session, placement: &Value, transition: &str) -> usize {
    if transition != "dissolve" {
        return 0;
    }
    let Some(clips) = placement["clips"].as_array() else { return 0 };
    let mut applied = 0;
    for clip in clips.iter().filter_map(Value::as_u64) {
        for edge in ["in", "out"] {
            if s.execute("sequence.applyVideoTransition", json!({"clip": clip, "effect": "cross_dissolve", "frames": 12, "edge": edge})).is_ok() {
                applied += 1;
            }
            let _ = s.execute("sequence.applyAudioTransition", json!({"clip": clip, "effect": "constant_power", "frames": 12, "edge": edge}));
        }
    }
    applied
}
/// `nl.bridge.plan` — inspect the gap, export the two frames and name the workflow, without
/// generating anything.
fn bridge_plan(s: &mut Session, p: &Value) -> Result<Value> {
    let gap = find_gap(s, p)?;
    let (last, first, audio) = export_bridge_frames(s, &gap)?;
    let mode = p.get("mode").and_then(Value::as_str).unwrap_or("flf").to_ascii_lowercase();
    if !matches!(mode.as_str(), "flf" | "r2v" | "i2v") {
        return Err(bad_cmd("nl.bridge.plan", "`mode` must be flf, r2v or i2v"));
    }
    let workflow = match mode.as_str() {
        "flf" => suite_workflow(s, TEXT_WORKFLOW),
        "r2v" => suite_workflow(s, R2V_WORKFLOW),
        _ => suite_workflow(s, I2V_WORKFLOW),
    };
    let references: Vec<String> =
        if mode == "i2v" { vec![last.to_string_lossy().to_string()] } else { vec![last.to_string_lossy().to_string(), first.to_string_lossy().to_string()] };
    // Build the default prompt from the real outgoing/incoming frames, not a canned sentence. A
    // caller-supplied prompt is kept as-is; the vision pass is best effort so a missing model never
    // blocks the bridge plan.
    let prompt_given = p.get("prompt").and_then(Value::as_str).is_some_and(|prompt| !prompt.trim().is_empty());
    let (mut last_description, mut first_description) = (None, None);
    if !prompt_given && p.get("analyzeIncoming").and_then(Value::as_bool).unwrap_or(false) {
        // Prefer the configured short-lived Llama server (LFM3): one load for both frames. Only if
        // it is not configured or fails fall back to the global vision provider.
        match super::segment::describe_frame_pair(s, &last, &first) {
            Ok((outgoing, incoming)) => {
                last_description = Some(outgoing);
                first_description = Some(incoming);
            }
            Err(error) => {
                eprintln!("bridge llama analysis failed: {error}");
                last_description = describe_bridge_frame(s, p, &last);
                first_description = describe_bridge_frame(s, p, &first);
            }
        }
    }
    let prompt = bridge_prompt(p, &mode, gap.seconds(), last_description.as_deref(), first_description.as_deref(), audio.is_some());
    Ok(json!({
        "track": gap.track,
        "outgoing": gap.outgoing,
        "incoming": gap.incoming,
        "gapStartTicks": gap.start.0,
        "gapEndTicks": gap.end.0,
        "gapSeconds": gap.seconds(),
        "lastFrame": last.to_string_lossy(),
        "firstFrame": first.to_string_lossy(),
        "audio": audio.as_ref().map(|path| path.to_string_lossy().to_string()),
        "references": references,
        "mode": mode,
        "workflow": workflow.map(|path| path.to_string_lossy().to_string()),
        "prompt": prompt,
        "incomingDescription": first_description,
        "outgoingDescription": last_description,
        "placement": {"track": gap.track_param, "time": gap.start.0, "duration": gap.end.0 - gap.start.0, "video": true, "audio": true, "fit": "frame", "transition": bridge_transition(p)},
    }))
}

/// `nl.bridge.generate` — build the first/last-frame workflow and generate the bridge; it is placed
/// in the gap when it finishes (also asynchronously, through the generation's `place`).
fn bridge_generate(s: &mut Session, p: &Value) -> Result<Value> {
    let plan = bridge_plan(s, p)?;
    let mode = plan["mode"].as_str().unwrap_or("flf").to_string();
    let seconds = (plan["gapSeconds"].as_f64().unwrap_or(2.0).ceil() as u64).clamp(1, 30);
    let (workflow, images) = match mode.as_str() {
        "flf" => {
            let t2v = choose_workflow(s, p, &[TEXT_WORKFLOW])?;
            let bridge = bridge_workflow(s, &t2v)?;
            (bridge, json!([plan["lastFrame"], plan["firstFrame"]]))
        }
        "r2v" => {
            let workflow = choose_workflow(s, p, &[R2V_WORKFLOW])?;
            (workflow, json!([plan["lastFrame"], plan["firstFrame"]]))
        }
        _ => {
            let workflow = choose_workflow(s, p, &[I2V_WORKFLOW])?;
            (workflow, json!([plan["lastFrame"]]))
        }
    };
    // Keyframe conditioning patches the latent: both sides must be a multiple of 32 px or the
    // MiniMax H3 model cannot reshape the keyframe latent (a 1080p sequence at the 2/3 video profile
    // gives 720, which is an odd number of 16 px latent cells).
    let mut params = json!({
        "workflow": workflow.to_string_lossy(),
        "prompt": plan["prompt"],
        "images": images,
        "seconds": seconds,
        "resolutionMode": "video",
        "kind": "video",
        "place": plan["placement"],
    });
    if let Some((width, height)) = crate::nl::size::from_params(s, &json!({"resolutionMode": "video"})) {
        let round32 = |value: u32| (((value as f64) / 32.0).round() as u32 * 32).max(32);
        params["width"] = json!(round32(width));
        params["height"] = json!(round32(height));
    }
    if let Some(audio) = plan["audio"].as_str().filter(|audio| !audio.is_empty()) {
        params["audio"] = json!(audio);
    }
    for key in ["wait", "dryRun", "seed", "overrides", "timeoutSeconds", "negative"] {
        if let Some(value) = p.get(key) {
            params[key] = value.clone();
        }
    }
    s.nl.results.remove("comfy.last_place");
    if let Some(transition) = p.get("transition") {
        params["place"]["transition"] = transition.clone();
    }
    let generation = s.execute("graphics.generateComfy", params)?;
    let placement = s.nl.results.get("comfy.last_place").cloned();
    let imported = s.nl.results.get("comfy.last").cloned();
    Ok(json!({"plan": plan, "workflow": workflow.to_string_lossy(), "generation": generation, "placement": placement, "imported": imported}))
}

/// `nl.bridge.place` — put an already generated file into the gap (useful when ComfyUI finished
/// after the client timeout: the file is in the album, but trimming it to the gap by hand is fiddly).
fn bridge_place(s: &mut Session, p: &Value) -> Result<Value> {
    let path = p
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| bad_cmd("nl.bridge.place", "need `path`: the generated file to place"))?;
    if !std::path::Path::new(path).is_file() {
        return Err(bad_cmd("nl.bridge.place", format!("file not found: {path}")));
    }
    let gap = find_gap(s, p)?;
    let item = crate::commands::import_streamed(s, path, None)?;
    let placed = s.execute(
        "timeline.place",
        json!({
            "item": item.0,
            "track": gap.track_param,
            "time": gap.start.0,
            "duration": gap.end.0 - gap.start.0,
            "video": true,
            "audio": true,
        }),
    )?;
    if let Some(clip) = placed["clips"].as_array().and_then(|clips| clips.first()).and_then(Value::as_u64) {
        let _ = s.execute("timeline.select", json!({"clips": [clip]}));
        let _ = s.execute("clip.fillFrame", json!({"clips": [clip]}));
    }
    let transitions = apply_place_transitions(s, &placed, &bridge_transition(p));
    Ok(json!({"item": item.0, "path": path, "gapSeconds": gap.seconds(), "placement": placed, "transitions": transitions}))
}
pub(crate) fn commands() -> Vec<CommandSpec> {
    let spec = |id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>| -> CommandSpec {
        CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: always, run, journal: false, named: crate::commands::NamedKeys::NONE }
    };
    vec![
        spec(
            "nl.generate.text",
            "NL Text to Video",
            r##"{"prompt":str,"seconds":int?,"workflow":str?,"track":str?,"atSeconds":f?,"wait":bool?,"dryRun":bool?,"seed":int?,"overrides":[...]?,"timeoutSeconds":int?}"##,
            generate_text,
        ),
        spec("nl.bridge.plan", "NL Bridge Plan", r#"{"outgoing":id?,"incoming":id?,"track":str?,"mode":"flf|r2v|i2v"?,"prompt":str?}"#, bridge_plan),
        spec(
            "nl.generate.extend",
            "NL Extend Clip",
            r#"{"clip":id?,"seconds":f?,"contextSeconds":f?,"prompt":str?,"workflow":str?,"wait":bool?,"dryRun":bool?,"seed":int?,"overrides":[...]?,"timeoutSeconds":int?}"#,
            generate_extend,
        ),
        spec("nl.bridge.place", "NL Bridge Place", r#"{"path":str,"outgoing":id?,"incoming":id?,"track":str?}"#, bridge_place),
        spec(
            "nl.bridge.generate",
            "NL Bridge Generate",
            r#"{"outgoing":id?,"incoming":id?,"track":str?,"mode":"flf|r2v|i2v"?,"prompt":str?,"wait":bool?,"dryRun":bool?,"seed":int?,"timeoutSeconds":int?}"#,
            bridge_generate,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_to_video_needs_a_prompt_and_plans_without_submitting() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        assert!(s.execute("nl.generate.text", json!({})).is_err());
        if suite_workflow(&s, TEXT_WORKFLOW).is_none() {
            return; // the suite root with workflows is not available in this checkout
        }
        let plan = s.execute("nl.generate.text", json!({"prompt": "a lighthouse in a storm", "seconds": 3, "dryRun": true})).unwrap();
        assert_eq!(plan["dryRun"], true);
        assert_eq!(plan["submitted"], false);
        assert!(plan["workflow"].as_str().unwrap().contains("video_minimax_h3_t2v"));
        assert!(plan["prompt"].as_str().unwrap().contains("lighthouse"));
    }

    #[test]
    fn a_bridge_plan_finds_the_gap_and_exports_both_frames() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        s.execute("file.newSequence", json!({"name": "Bridge test", "video": 1, "audio": 1})).unwrap();
        let first =
            s.execute("timeline.place", json!({"item": 5, "track": "V1", "time": 0, "duration": {"seconds": 2}, "video": true, "audio": false})).unwrap();
        let second = s
            .execute(
                "timeline.place",
                json!({"item": 6, "track": "V1", "time": Tick::from_seconds_f64(4.0).0, "duration": {"seconds": 2}, "video": true, "audio": false}),
            )
            .unwrap();
        let outgoing = first["clips"][0].as_u64().unwrap();
        let incoming = second["clips"][0].as_u64().unwrap();
        let plan = s.execute("nl.bridge.plan", json!({"outgoing": outgoing, "incoming": incoming, "mode": "flf"})).unwrap();
        assert!((plan["gapSeconds"].as_f64().unwrap() - 2.0).abs() < 0.01);
        assert!(std::path::Path::new(plan["lastFrame"].as_str().unwrap()).is_file());
        assert!(std::path::Path::new(plan["firstFrame"].as_str().unwrap()).is_file());
        assert_eq!(plan["references"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn extending_needs_a_selected_video_clip_and_empty_space_after_it() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        assert!(s.execute("nl.generate.extend", json!({})).is_err(), "no clip selected");
        let clip = s.active_sequence().unwrap().video_tracks[0].items[0].id.0;
        let error = s.execute("nl.generate.extend", json!({"clip": clip })).unwrap_err().to_string();
        assert!(error.contains("no empty space"), "{error}");
    }

    #[test]
    fn the_bridge_workflow_gets_the_first_and_last_frame_inputs() {
        let s = Session::default();
        let Some(t2v) = suite_workflow(&s, TEXT_WORKFLOW) else { return };
        let path = bridge_workflow(&s, &t2v).unwrap();
        let workflow: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let node = workflow.as_object().unwrap().values().find(|node| node["class_type"] == "MiniMaxH3ImageToVideo").unwrap();
        assert_eq!(node["inputs"]["first_frame"], json!(["7001", 0]));
        assert_eq!(node["inputs"]["last_frame"], json!(["7002", 0]));
        let loaders = workflow.as_object().unwrap().values().filter(|node| node["class_type"] == "LoadImage").count();
        assert_eq!(loaders, 2);
    }
}
