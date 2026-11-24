//! Reframe a candidate into a new sequence (16:9 ↔ 9:16): the clip is placed at the candidate
//! in/out, "motion" scale/position crop it to the target frame and, when the transcription words are
//! supplied, a styled caption track is generated. This is the missing step between Clips AI proposals
//! and a ready-to-edit vertical sequence.

use std::path::{Path, PathBuf};
use std::process::Command;

use filmcraft_project::ItemKind;
use filmcraft_time::Tick;
use serde_json::{Value, json};

use crate::commands::{always, bad};
use crate::{EngineError, Result, Session};

fn bad_cmd(msg: impl ToString) -> EngineError {
    bad("nl.reframe.sequence", msg.to_string())
}

fn video_size(s: &Session, media: filmcraft_project::ItemId) -> (u32, u32, f64) {
    let item = s.project.item(media);
    let Some(ItemKind::Media(clip)) = item.map(|item| &item.kind) else {
        return (1920, 1080, 30.0);
    };
    let Some(video) = clip.info.video.as_ref() else {
        return (1920, 1080, 30.0);
    };
    (video.width, video.height, video.frame_rate.as_f64())
}

/// The media file of an item, when it is file-backed.
fn media_path(s: &Session, media: filmcraft_project::ItemId) -> Option<PathBuf> {
    let item = s.project.item(media)?;
    let ItemKind::Media(clip) = &item.kind else { return None };
    match &clip.media {
        filmcraft_project::MediaRef::File { path } => Some(PathBuf::from(path)),
        _ => None,
    }
}

/// The camera trajectory of one candidate, ported from the old NL Video Suite's own
/// camera-cpu-v3 (`sviluppo/bridge/camera_path.py`). It samples the range every 250 ms with the
/// OpenCV Haar detector shipped in the suite runtime, detects scene cuts, locks the main face and
/// smooths the crop with a bounded, critically damped path. No model is downloaded.
#[derive(Clone, Debug, Default)]
pub(crate) struct CameraTrack {
    /// `(seconds from the start of the range, x, y)` points, sorted and deduplicated.
    pub points: Vec<(f64, f64, f64)>,
    pub samples: usize,
    pub scene_cuts: usize,
    pub coverage: f64,
    pub maximum_gap_ms: f64,
    pub shots: u64,
    pub preferred_fit: String,
    pub advice: String,
    pub version: String,
    pub behavior: String,
}

impl CameraTrack {
    fn first_centre(&self) -> Option<(f64, f64)> {
        self.points.first().map(|(_, x, y)| (*x, *y))
    }

    fn to_json(&self) -> Value {
        json!({
            "tracking": "smooth",
            "points": self.points.len(),
            "samples": self.samples,
            "sceneCuts": self.scene_cuts,
            "coverage": self.coverage,
            "maximumGapMs": self.maximum_gap_ms,
            "shots": self.shots,
            "preferredFit": self.preferred_fit,
            "advice": self.advice,
            "version": self.version,
            "behavior": self.behavior,
        })
    }
}

fn camera_track(s: &Session, source: &Path, start: f64, end: f64, stability: f64, response: f64, behavior: &str) -> Option<CameraTrack> {
    let root = crate::nl::suite_root()?;
    let python = root.join("runtime").join("python").join(if cfg!(windows) { "python.exe" } else { "python" });
    let ffmpeg = root.join("app").join("tools").join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
    if !python.is_file() {
        return None;
    }
    let dir = crate::nl::media_work_dir(s, &source.to_string_lossy(), "autofocus");
    std::fs::create_dir_all(&dir).ok()?;
    let script = dir.join("reframe_camera.py");
    std::fs::write(&script, include_str!("reframe_camera.py")).ok()?;
    let request = dir.join("camera-request.json");
    let start_ms = (start * 1000.0).round() as i64;
    let end_ms = (end * 1000.0).round() as i64;
    let mut payload = json!({
        "start_ms": start_ms,
        "end_ms": end_ms,
        "stability": stability,
        "response": response,
        "behavior": behavior,
    });
    payload["source"] = json!(source.to_string_lossy());
    std::fs::write(&request, serde_json::to_vec(&payload).ok()?).ok()?;
    if let Some(track) = run_camera(&python, &script, &request) {
        return Some(track);
    }
    // OpenCV could not read the video (a build without its FFmpeg backend): extract frames with
    // the suite's own ffmpeg and run the same detector on them.
    if !ffmpeg.is_file() {
        return None;
    }
    let frames_dir = dir.join("frames");
    std::fs::create_dir_all(&frames_dir).ok()?;
    let mut paths = Vec::new();
    let mut times = Vec::new();
    let mut index = 0usize;
    let mut at = start;
    while at < end && index < 480 {
        let png = frames_dir.join(format!("frame-{index:03}.png"));
        let output = Command::new(&ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-ss", &format!("{at:.3}"), "-i"])
            .arg(source)
            .args(["-frames:v", "1"])
            .arg(&png)
            .output()
            .ok()?;
        if output.status.success() && png.is_file() {
            paths.push(png.to_string_lossy().to_string());
            times.push((at * 1000.0).round() as i64);
        }
        index += 1;
        at += 0.25;
    }
    if paths.len() < 2 {
        return None;
    }
    let mut payload = json!({
        "frames": paths,
        "times_ms": times,
        "stability": stability,
        "response": response,
        "behavior": behavior,
    });
    payload["source"] = json!(source.to_string_lossy());
    std::fs::write(&request, serde_json::to_vec(&payload).ok()?).ok()?;
    run_camera(&python, &script, &request)
}

fn run_camera(python: &Path, script: &Path, request: &Path) -> Option<CameraTrack> {
    let output = Command::new(python).arg(script).arg(request).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value: Value = serde_json::from_slice(&output.stdout).ok()?;
    parse_camera_track(&value)
}

/// Parse the camera script's reply into points relative to the start of the range. Kept separate
/// so the parsing and the clamping are testable without Python.
pub(crate) fn parse_camera_track(value: &Value) -> Option<CameraTrack> {
    let start_ms = value["points"].as_array().and_then(|points| points.first()).and_then(|point| point["source_ms"].as_f64()).unwrap_or(0.0);
    let mut points: Vec<(f64, f64, f64)> = Vec::new();
    for point in value["points"].as_array()? {
        let at = point["source_ms"].as_f64()?;
        let x = point["x"].as_f64()?;
        let y = point["y"].as_f64()?;
        if !at.is_finite() || !x.is_finite() || !y.is_finite() {
            continue;
        }
        let seconds = ((at - start_ms) / 1000.0).max(0.0);
        let x = x.clamp(0.0, 1.0);
        let y = y.clamp(0.0, 1.0);
        if points.last().is_some_and(|(last, lx, ly)| (*last - seconds).abs() < 1e-6 && (*lx - x).abs() < 1e-6 && (*ly - y).abs() < 1e-6) {
            continue;
        }
        points.push((seconds, x, y));
    }
    if points.is_empty() {
        return None;
    }
    let samples = value["samples"].as_array().map_or(0, Vec::len);
    let scene_cuts = value["samples"].as_array().map_or(0, |samples| samples.iter().filter(|sample| sample["cut"].as_bool().unwrap_or(false)).count());
    let coverage = value["diagnostics"]["coverage"].as_f64().unwrap_or(0.0);
    let maximum_gap_ms = value["diagnostics"]["maximum_gap_ms"].as_f64().unwrap_or(0.0);
    let shots = value["diagnostics"]["shots"].as_u64().unwrap_or(0);
    Some(CameraTrack {
        points,
        samples,
        scene_cuts,
        coverage,
        maximum_gap_ms,
        shots,
        preferred_fit: value["preferred_fit"].as_str().unwrap_or("cover").to_string(),
        advice: value["advice"].as_str().unwrap_or("").to_string(),
        version: value["version"].as_str().unwrap_or("camera-cpu-v3").to_string(),
        behavior: value["behavior"].as_str().unwrap_or("composed").to_string(),
    })
}

fn focus_offset(focus: &str, free_x: f64, free_y: f64) -> (f64, f64) {
    match focus {
        "left" => (free_x * 0.5, 0.0),
        "right" => (-free_x * 0.5, 0.0),
        "top" => (0.0, free_y * 0.5),
        "bottom" => (0.0, -free_y * 0.5),
        _ => (0.0, 0.0),
    }
}

fn caption_cues(words: &[Value], start: f64, end: f64) -> Vec<(String, f64, f64)> {
    let mut rows: Vec<(String, f64, f64)> = Vec::new();
    let mut current = String::new();
    let mut cue_start = 0.0;
    let mut cue_end = 0.0;
    let mut count = 0;
    for word in words {
        let text = word.get("word").or_else(|| word.get("text")).and_then(Value::as_str).unwrap_or("").trim();
        let a = word.get("start").and_then(Value::as_f64).unwrap_or(0.0);
        let b = word.get("end").and_then(Value::as_f64).unwrap_or(a);
        if text.is_empty() || b <= start || a >= end {
            continue;
        }
        if count == 0 {
            cue_start = a.max(start) - start;
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(text);
        cue_end = b.min(end) - start;
        count += 1;
        if count >= 5 || cue_end - cue_start >= 2.5 {
            rows.push((current.clone(), cue_start, cue_end));
            current.clear();
            count = 0;
        }
    }
    if !current.is_empty() && cue_end > cue_start {
        rows.push((current, cue_start, cue_end));
    }
    rows
}

fn add_captions(s: &mut Session, words: &[Value], start: f64, end: f64) -> Result<Value> {
    let cues = caption_cues(words, start, end);
    if cues.is_empty() {
        return Ok(json!({"count": 0, "reason": "no words in the candidate range"}));
    }
    let track = s.execute("captions.newTrack", json!({"format": "Subtitle", "name": "Clips AI", "language": "it"}))?;
    let track_id = track["track"].clone();
    s.execute(
        "captions.setStyle",
        json!({
            "track": track_id,
            "font": "Inter",
            "fontStyle": "Bold",
            "size": 56.0,
            "color": "#ffffffff",
            "background": true,
            "backgroundColor": "#000000b8",
            "align": "center",
            "anchor": "bottom",
            "margin": 0.09,
            "outline": 3.0,
            "outlineColor": "#000000ff",
        }),
    )?;
    let mut added = 0;
    for (text, a, b) in &cues {
        s.execute("captions.add", json!({"track": track_id, "text": text, "seconds": a, "durationSeconds": (b - a).max(0.4)}))?;
        added += 1;
    }
    Ok(json!({"count": added, "track": track_id, "cues": cues.len()}))
}

/// Everything one reframed sequence needs, kept in one value so the helper stays readable.
pub(crate) struct ReframeSpec<'a> {
    pub media: filmcraft_project::ItemId,
    pub start: f64,
    pub end: f64,
    pub title: &'a str,
    pub target: &'a str,
    pub focus: &'a str,
    pub tracking: &'a str,
    pub stability: f64,
    pub response: f64,
    pub behavior: &'a str,
    pub subtitles: bool,
    pub words: &'a [Value],
}

/// The offset that puts a normalized source centre at the centre of the target frame. The crop
/// travels over the whole painted picture, so the shift is scaled by the painted size, not by the
/// free margin; it is then clamped to the margin that keeps the picture covering the frame.
fn centre_offset(centre: (f64, f64), painted_w: f64, painted_h: f64, target_w: f64, target_h: f64) -> (f64, f64) {
    let free_x = (painted_w - target_w).max(0.0);
    let free_y = (painted_h - target_h).max(0.0);
    let dx = ((0.5 - centre.0) * painted_w).clamp(-free_x / 2.0, free_x / 2.0);
    let dy = ((0.5 - centre.1) * painted_h).clamp(-free_y / 2.0, free_y / 2.0);
    (dx, dy)
}

/// Create one new sequence for a candidate.
pub(crate) fn create_sequence(s: &mut Session, spec: &ReframeSpec<'_>) -> Result<Value> {
    let ReframeSpec { media, start, end, title, target, focus, tracking, stability, response, behavior, subtitles, words } = *spec;
    if end <= start {
        return Err(bad_cmd("the candidate range is empty"));
    }
    let (sw, sh, fps) = video_size(s, media);
    let (tw, th) = if target == "horizontal" { (1920, 1080) } else { (1080, 1920) };
    let rate = s.sequence_rate().as_f64().max(1.0);
    s.execute("file.newSequence", json!({"name": title, "width": tw, "height": th, "fps": if fps > 0.0 { fps } else { rate }, "video": 1, "audio": 1}))?;
    let seq = s.state.active_sequence.ok_or(EngineError::NoSequence)?;
    let placed = s.execute(
        "timeline.place",
        json!({
            "item": media.0, "track": "V1", "audioTrack": "A1", "time": 0,
            "sourceIn": Tick::from_seconds_f64(start).0,
            "duration": Tick::from_seconds_f64(end - start).0,
        }),
    )?;
    let Some(clip) = placed["clips"].as_array().and_then(|clips| clips.first()).and_then(Value::as_u64) else {
        return Err(bad_cmd("the clip could not be placed in the new sequence"));
    };
    let scale = (tw as f64 / sw.max(1) as f64).max(th as f64 / sh.max(1) as f64) * 100.0;
    let painted_w = sw as f64 * scale / 100.0;
    let painted_h = sh as f64 * scale / 100.0;
    let free_x = (painted_w - tw as f64).max(0.0);
    let free_y = (painted_h - th as f64).max(0.0);
    let mut camera = None;
    let mut centre = None;
    let (mut dx, mut dy) = (0.0, 0.0);
    if focus == "auto" {
        if let Some(source) = media_path(s, media)
            && let Some(track) = camera_track(s, &source, start, end, stability, response, behavior)
        {
            centre = track.first_centre();
            if let Some(c) = centre {
                (dx, dy) = centre_offset(c, painted_w, painted_h, tw as f64, th as f64);
            }
            camera = Some(track);
        }
    } else {
        (dx, dy) = focus_offset(focus, free_x, free_y);
    }
    s.execute("effects.setParam", json!({"clip": clip, "effect": "motion", "param": "scale", "value": scale}))?;
    let mut keyframes = 0usize;
    let smooth = tracking == "smooth";
    if let Some(track) = camera.as_ref().filter(|track| smooth && track.points.len() > 1) {
        for (seconds, x, y) in &track.points {
            let (ox, oy) = centre_offset((*x, *y), painted_w, painted_h, tw as f64, th as f64);
            s.execute(
                "effects.setParam",
                json!({
                    "clip": clip,
                    "effect": "motion",
                    "param": "position",
                    "value": [tw as f64 / 2.0 + ox, th as f64 / 2.0 + oy],
                    "keyframe": true,
                    "time": Tick::from_seconds_f64(*seconds).0,
                }),
            )?;
            keyframes += 1;
        }
    } else {
        s.execute("effects.setParam", json!({"clip": clip, "effect": "motion", "param": "position", "value": [tw as f64 / 2.0 + dx, th as f64 / 2.0 + dy]}))?;
    }
    let captions = if subtitles { add_captions(s, words, start, end)? } else { json!({"count": 0, "skipped": true}) };
    let reframe = match camera {
        Some(track) => {
            let mut value = track.to_json();
            value["keyframes"] = json!(keyframes);
            value["offset"] = json!([dx, dy]);
            value["faceCentre"] = json!(centre.map(|c| [c.0, c.1]));
            value["scale"] = json!(scale);
            value["focus"] = json!(focus);
            value
        }
        None => json!({
            "tracking": "static",
            "points": 0,
            "keyframes": 0,
            "scale": scale,
            "focus": focus,
            "offset": [dx, dy],
            "faceCentre": Value::Null,
            "fallback": "camera tracker unavailable",
        }),
    };
    Ok(json!({
        "sequence": seq.0,
        "name": title,
        "clip": clip,
        "source": {"width": sw, "height": sh},
        "target": {"width": tw, "height": th},
        "reframe": reframe,
        "startSeconds": start,
        "endSeconds": end,
        "subtitles": captions,
    }))
}

/// `nl.reframe.sequence` — one candidate to a new reframed sequence with optional captions.
fn reframe_sequence(s: &mut Session, p: &Value) -> Result<Value> {
    let media = if let Some(item) = p.get("item").and_then(Value::as_u64) {
        filmcraft_project::ItemId(item)
    } else {
        let (media, _) = super::target_media(s, p).ok_or_else(|| bad_cmd("select a video in the project or pass `item`"))?;
        media
    };
    let start = p.get("startSeconds").and_then(Value::as_f64).unwrap_or(0.0);
    let end = p.get("endSeconds").and_then(Value::as_f64).unwrap_or(start + 10.0);
    let target = p.get("target").and_then(Value::as_str).unwrap_or("vertical");
    if !matches!(target, "vertical" | "horizontal") {
        return Err(bad_cmd("`target` must be vertical or horizontal"));
    }
    let focus = p.get("focus").and_then(Value::as_str).unwrap_or("center");
    let tracking = p.get("tracking").and_then(Value::as_str).unwrap_or("static");
    if !matches!(tracking, "static" | "smooth") {
        return Err(bad_cmd("`tracking` must be static or smooth"));
    }
    let behavior = p.get("behavior").and_then(Value::as_str).unwrap_or("composed");
    if !matches!(behavior, "composed" | "legacy") {
        return Err(bad_cmd("`behavior` must be composed or legacy"));
    }
    let stability = p.get("stability").and_then(Value::as_f64).unwrap_or(0.65).clamp(0.0, 1.0);
    let response = p.get("response").and_then(Value::as_f64).unwrap_or(0.65).clamp(0.0, 1.0);
    let title = p.get("title").and_then(Value::as_str).unwrap_or("Clips AI");
    let subtitles = p.get("subtitles").and_then(Value::as_bool).unwrap_or(false);
    let words = p.get("words").and_then(Value::as_array).cloned().unwrap_or_default();
    let result =
        create_sequence(s, &ReframeSpec { media, start, end, title, target, focus, tracking, stability, response, behavior, subtitles, words: &words })?;
    s.nl.results.insert("reframe.last", result.clone());
    Ok(result)
}

/// `nl.clipping.sequences` — every selected candidate to its own reframed sequence.
fn clipping_sequences(s: &mut Session, p: &Value) -> Result<Value> {
    let (media, _) = super::target_media(s, p).ok_or_else(|| bad("nl.clipping.sequences", "select a video in the project or pass `item`"))?;
    let items = p.get("items").and_then(Value::as_array).cloned().ok_or_else(|| bad("nl.clipping.sequences", "pass the selected candidate `items`"))?;
    if items.is_empty() || items.len() > 12 {
        return Err(bad("nl.clipping.sequences", "select 1 to 12 candidates"));
    }
    let target = p.get("target").and_then(Value::as_str).unwrap_or("vertical");
    let focus = p.get("focus").and_then(Value::as_str).unwrap_or("center");
    let tracking = p.get("tracking").and_then(Value::as_str).unwrap_or("static");
    if !matches!(tracking, "static" | "smooth") {
        return Err(bad("nl.clipping.sequences", "`tracking` must be static or smooth"));
    }
    let behavior = p.get("behavior").and_then(Value::as_str).unwrap_or("composed");
    if !matches!(behavior, "composed" | "legacy") {
        return Err(bad("nl.clipping.sequences", "`behavior` must be composed or legacy"));
    }
    let stability = p.get("stability").and_then(Value::as_f64).unwrap_or(0.65).clamp(0.0, 1.0);
    let response = p.get("response").and_then(Value::as_f64).unwrap_or(0.65).clamp(0.0, 1.0);
    let subtitles = p.get("subtitles").and_then(Value::as_bool).unwrap_or(true);
    let words = p.get("words").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut sequences = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let start = item.get("start").and_then(Value::as_f64).unwrap_or(0.0);
        let end = item.get("end").and_then(Value::as_f64).unwrap_or(start + 5.0);
        let title = item.get("title").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("Clips AI {}", index + 1));
        let spec = ReframeSpec { media, start, end, title: &title, target, focus, tracking, stability, response, behavior, subtitles, words: &words };
        match create_sequence(s, &spec) {
            Ok(value) => sequences.push(value),
            Err(e) => return Err(bad("nl.clipping.sequences", format!("{title}: {e}"))),
        }
    }
    s.nl.results.insert("reframe.sequences", json!({"sequences": sequences}));
    Ok(json!({"count": sequences.len(), "sequences": sequences}))
}

pub(crate) fn commands() -> Vec<crate::CommandSpec> {
    let spec = |id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>| crate::CommandSpec {
        id,
        label,
        menu: &[],
        shortcut: None,
        params,
        enabled: always,
        run,
        journal: false,
        named: crate::commands::NamedKeys::NONE,
    };
    vec![
        spec(
            "nl.reframe.sequence",
            "NL Reframe Candidate",
            r#"{"item":id?,"startSeconds":f,"endSeconds":f,"title":str?,"target":"vertical|horizontal"?,"focus":"auto|center|left|right|top|bottom"?,"tracking":"static|smooth"?,"stability":f?,"response":f?,"behavior":"composed|legacy"?,"subtitles":bool?,"words":[obj]?}"#,
            reframe_sequence,
        ),
        spec(
            "nl.clipping.sequences",
            "NL Clips AI → Sequences",
            r#"{"items":[obj],"item":id?,"target":"vertical|horizontal"?,"focus":"auto|center|left|right|top|bottom"?,"tracking":"static|smooth"?,"stability":f?,"response":f?,"behavior":"composed|legacy"?,"subtitles":bool?,"words":[obj]?}"#,
            clipping_sequences,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captions_group_words_stay_inside_the_candidate_and_are_relative() {
        let words = vec![
            json!({"word": "uno", "start": 10.0, "end": 10.4}),
            json!({"word": "due", "start": 11.0, "end": 11.2}),
            json!({"word": "fuori", "start": 20.0, "end": 20.4}),
        ];
        let cues = caption_cues(&words, 10.0, 15.0);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].0, "uno due");
        assert!((cues[0].1 - 0.0).abs() < 0.01);
    }

    #[test]
    fn focus_offsets_move_towards_the_named_edge() {
        assert!(focus_offset("left", 200.0, 0.0).0 > 0.0);
        assert!(focus_offset("right", 200.0, 0.0).0 < 0.0);
        assert_eq!(focus_offset("center", 200.0, 100.0), (0.0, 0.0));
    }

    #[test]
    fn a_face_centre_becomes_a_bounded_crop_offset() {
        assert_eq!(centre_offset((0.5, 0.5), 1280.0, 720.0, 640.0, 360.0), (0.0, 0.0));
        let (dx, dy) = centre_offset((0.4, 0.3), 1280.0, 720.0, 640.0, 360.0);
        assert!((dx - 128.0).abs() < 1e-9, "{dx}");
        assert!((dy - 144.0).abs() < 1e-9, "{dy}");
        // A face near the edge never moves the crop beyond the free range.
        let (dx, dy) = centre_offset((0.0, 1.0), 1280.0, 720.0, 640.0, 360.0);
        assert_eq!((dx, dy), (320.0, -180.0));
    }

    #[test]
    fn the_camera_reply_is_relative_to_the_range_and_keeps_cuts() {
        let value = json!({
            "points": [
                {"source_ms": 1000.0, "x": 0.4, "y": 0.5},
                {"source_ms": 1500.0, "x": 0.6, "y": 0.5},
                {"source_ms": 2000.0, "x": 99.0, "y": -4.0}
            ],
            "samples": [{"cut": false}, {"cut": true}],
            "diagnostics": {"coverage": 0.9, "maximum_gap_ms": 250, "shots": 2},
            "preferred_fit": "cover",
            "advice": "ok",
            "version": "camera-cpu-v3",
            "behavior": "composed"
        });
        let track = parse_camera_track(&value).expect("a track");
        assert_eq!(track.points.len(), 3);
        assert!((track.points[0].0 - 0.0).abs() < 1e-9);
        assert!((track.points[1].0 - 0.5).abs() < 1e-9);
        assert_eq!(track.points[2].1, 1.0, "x is clamped");
        assert_eq!(track.points[2].2, 0.0, "y is clamped");
        assert_eq!(track.scene_cuts, 1);
        assert_eq!(track.shots, 2);
        assert_eq!(track.samples, 2);
    }

    #[test]
    fn an_empty_camera_reply_is_no_track() {
        assert!(parse_camera_track(&json!({"points": []})).is_none());
        assert!(parse_camera_track(&json!({})).is_none());
    }
}
