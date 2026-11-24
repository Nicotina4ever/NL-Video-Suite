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

/// Ask the suite's OpenCV runtime where the main face is across a few frames. This is our own
/// detector (Haar cascade shipped with OpenCV): no model download and no tracking dependency.
fn auto_focus(s: &Session, source: &Path, start: f64, end: f64) -> Option<(f64, f64)> {
    let root = crate::nl::suite_root()?;
    let python = root.join("runtime").join("python").join(if cfg!(windows) { "python.exe" } else { "python" });
    let ffmpeg = root.join("app").join("tools").join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
    if !python.is_file() || !ffmpeg.is_file() {
        return None;
    }
    let dir = crate::nl::media_work_dir(s, &source.to_string_lossy(), "autofocus");
    std::fs::create_dir_all(&dir).ok()?;
    let mut frames = Vec::new();
    for index in 0..7 {
        let at = start + (end - start) * (index as f64 + 0.5) / 7.0;
        let png = dir.join(format!("focus-{index}.png"));
        let output = Command::new(&ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-ss", &format!("{at:.3}"), "-i"])
            .arg(source)
            .args(["-frames:v", "1"])
            .arg(&png)
            .output()
            .ok()?;
        if output.status.success() && png.is_file() {
            frames.push(png);
        }
    }
    if frames.is_empty() {
        return None;
    }
    let script = dir.join("auto-focus.py");
    let code = r#"
import cv2, sys
cascade = cv2.CascadeClassifier(cv2.data.haarcascades + 'haarcascade_frontalface_default.xml')
centres = []
for path in sys.argv[1:]:
    image = cv2.imread(path)
    if image is None:
        continue
    gray = cv2.cvtColor(image, cv2.COLOR_BGR2GRAY)
    faces = cascade.detectMultiScale(gray, 1.1, 5, minSize=(24, 24))
    for x, y, w, h in faces:
        centres.append(((x + w / 2) / image.shape[1], (y + h / 2) / image.shape[0]))
if centres:
    print(sum(c[0] for c in centres) / len(centres), sum(c[1] for c in centres) / len(centres))
"#;
    std::fs::write(&script, code).ok()?;
    let output = Command::new(python).arg(&script).args(&frames).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut parts = text.split_whitespace();
    let cx: f64 = parts.next()?.parse().ok()?;
    let cy: f64 = parts.next()?.parse().ok()?;
    (0.0..=1.0).contains(&cx).then_some((cx, cy))
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
    pub subtitles: bool,
    pub words: &'a [Value],
}

/// Create one new sequence for a candidate.
pub(crate) fn create_sequence(s: &mut Session, spec: &ReframeSpec<'_>) -> Result<Value> {
    let ReframeSpec { media, start, end, title, target, focus, subtitles, words } = *spec;
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
    let mut auto_centre = None;
    let (dx, dy) = if focus == "auto" {
        if let Some(source) = media_path(s, media).and_then(|path| auto_focus(s, &path, start, end)) {
            auto_centre = Some(source);
            let dx = ((0.5 - source.0) * free_x).clamp(-free_x / 2.0, free_x / 2.0);
            let dy = ((0.5 - source.1) * free_y).clamp(-free_y / 2.0, free_y / 2.0);
            (dx, dy)
        } else {
            (0.0, 0.0)
        }
    } else {
        focus_offset(focus, free_x, free_y)
    };
    s.execute("effects.setParam", json!({"clip": clip, "effect": "motion", "param": "scale", "value": scale}))?;
    s.execute("effects.setParam", json!({"clip": clip, "effect": "motion", "param": "position", "value": [tw as f64 / 2.0 + dx, th as f64 / 2.0 + dy]}))?;
    let captions = if subtitles { add_captions(s, words, start, end)? } else { json!({"count": 0, "skipped": true}) };
    Ok(json!({
        "sequence": seq.0,
        "name": title,
        "clip": clip,
        "source": {"width": sw, "height": sh},
        "target": {"width": tw, "height": th},
        "reframe": {"scale": scale, "focus": focus, "offset": [dx, dy], "faceCentre": auto_centre},
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
    let title = p.get("title").and_then(Value::as_str).unwrap_or("Clips AI");
    let subtitles = p.get("subtitles").and_then(Value::as_bool).unwrap_or(false);
    let words = p.get("words").and_then(Value::as_array).cloned().unwrap_or_default();
    let result = create_sequence(s, &ReframeSpec { media, start, end, title, target, focus, subtitles, words: &words })?;
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
    let subtitles = p.get("subtitles").and_then(Value::as_bool).unwrap_or(true);
    let words = p.get("words").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut sequences = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let start = item.get("start").and_then(Value::as_f64).unwrap_or(0.0);
        let end = item.get("end").and_then(Value::as_f64).unwrap_or(start + 5.0);
        let title = item.get("title").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("Clips AI {}", index + 1));
        match create_sequence(s, &ReframeSpec { media, start, end, title: &title, target, focus, subtitles, words: &words }) {
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
            r#"{"item":id?,"startSeconds":f,"endSeconds":f,"title":str?,"target":"vertical|horizontal"?,"focus":"auto|center|left|right|top|bottom"?,"subtitles":bool?,"words":[obj]?}"#,
            reframe_sequence,
        ),
        spec(
            "nl.clipping.sequences",
            "NL Clips AI → Sequences",
            r#"{"items":[obj],"item":id?,"target":"vertical|horizontal"?,"focus":"auto|center|left|right|top|bottom"?,"subtitles":bool?,"words":[obj]?}"#,
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
}
