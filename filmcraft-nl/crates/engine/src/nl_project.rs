//! Import an NL Video Suite `project.json` as a FilmCraft project.
//!
//! The NL project is the source of truth for media, trim, track layout and transcript. Media files
//! stay where they are (`<project dir>/assets/<file name>`): the import links them, it never copies
//! or transcodes. The result is installed as an unsaved FilmCraft project, so the user can inspect
//! it and choose where to save it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use filmcraft_color::ColorInfo;
use filmcraft_media::{AudioStreamInfo, MediaInfo, MediaKind, VideoStreamInfo};
use filmcraft_project::{ItemKind, Label, MediaClip, MediaRef, Project, TrackKind, Transcript, Word};
use filmcraft_time::{FrameRate, Tick, TimeRange};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{EngineError, Result, Session};

#[derive(Debug, Default, Deserialize)]
struct NlProject {
    #[serde(default)]
    title: String,
    #[serde(default)]
    aspect_ratio: String,
    #[serde(default)]
    assets: Vec<NlAsset>,
    #[serde(default)]
    timeline: NlTimeline,
    #[serde(default)]
    transcripts: Vec<NlTranscript>,
}

#[derive(Debug, Default, Deserialize)]
struct NlAsset {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    file_name: String,
    #[serde(default)]
    media: Option<NlMedia>,
}

#[derive(Debug, Default, Deserialize)]
struct NlMedia {
    #[serde(default)]
    duration_ms: u64,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    frame_rate: Option<f64>,
    #[serde(default)]
    has_audio: bool,
}

#[derive(Debug, Default, Deserialize)]
struct NlTimeline {
    #[serde(default)]
    tracks: Vec<NlTrack>,
    #[serde(default)]
    clips: Vec<NlClip>,
}

#[derive(Debug, Default, Deserialize)]
struct NlTrack {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    kind: String,
}

#[derive(Debug, Default, Deserialize)]
struct NlClip {
    #[serde(default)]
    asset_id: String,
    #[serde(default)]
    track_id: String,
    #[serde(default)]
    start_ms: u64,
    #[serde(default)]
    trim_in_ms: u64,
    #[serde(default)]
    duration_ms: u64,
    #[serde(default)]
    label: String,
    #[serde(default = "one")]
    volume: f64,
    #[serde(default)]
    transform: Option<NlTransform>,
}

#[derive(Debug, Default, Deserialize)]
struct NlTransform {
    #[serde(default)]
    fit: String,
}

#[derive(Debug, Default, Deserialize)]
struct NlTranscript {
    #[serde(default)]
    asset_id: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    segments: Vec<NlSegment>,
    #[serde(default)]
    words: Vec<NlWord>,
}

#[derive(Debug, Default, Deserialize)]
struct NlSegment {
    #[serde(default)]
    start_ms: u64,
    #[serde(default)]
    end_ms: u64,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Default, Deserialize)]
struct NlWord {
    #[serde(default)]
    start_ms: u64,
    #[serde(default)]
    end_ms: u64,
    #[serde(default)]
    text: String,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Default)]
pub(crate) struct Summary {
    pub name: String,
    pub items: usize,
    pub clips: usize,
    pub linked_audio: usize,
    pub transcripts: usize,
    pub missing_media: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
struct TrackSlot {
    kind: TrackKind,
    index: usize,
    /// The video track this slot came from, for the linked audio of its clips.
    video: Option<usize>,
}

/// `file.importNlProject {path}`: read `project.json`, convert it and install the result as the
/// current, unsaved project.
pub(crate) fn import(s: &mut Session, p: &Value) -> Result<Value> {
    let path = p
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| EngineError::BadParams { cmd: "file.importNlProject".into(), msg: "need `path` to an NL project.json".into() })?;
    let bytes = s.services.read_file_bounded(path, 64 * 1024 * 1024).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
    let text = String::from_utf8(bytes).map_err(|_| EngineError::Other(format!("{path}: the NL project is not UTF-8")))?;
    let dir = Path::new(path).parent().unwrap_or_else(|| Path::new("."));
    let (project, summary) = convert(dir, &text)?;
    crate::commands::install_project(s, project, None, false);
    if let Some(seq) = s.state.active_sequence {
        s.events.push(crate::Event::OpenSequence(seq));
    }
    Ok(json!({
        "name": summary.name,
        "items": summary.items,
        "clips": summary.clips,
        "linkedAudio": summary.linked_audio,
        "transcripts": summary.transcripts,
        "missingMedia": summary.missing_media,
        "warnings": summary.warnings,
        "unsaved": true,
    }))
}

/// Convert the JSON text; `dir` is the NL project folder (the parent of `project.json`).
pub(crate) fn convert(dir: &Path, text: &str) -> Result<(Project, Summary)> {
    let nl: NlProject = serde_json::from_str(text).map_err(|e| EngineError::Other(format!("not an NL project.json: {e}")))?;
    if nl.assets.is_empty() && nl.timeline.clips.is_empty() {
        return Err(EngineError::Other("the NL project has no media and no timeline clips".into()));
    }
    let title = if nl.title.trim().is_empty() { "NL Project".to_string() } else { nl.title.trim().to_string() };
    let mut project = Project::new(&title);
    let footage = project.add_bin("Footage", None);
    let sequences = project.add_bin("Sequences", None);
    let mut summary = Summary { name: title.clone(), ..Summary::default() };
    let mut media = HashMap::new();
    let mut rates = HashMap::new();
    let source_end = source_ends(&nl);

    for asset in &nl.assets {
        let Some(clip) = media_clip(dir, asset, source_end.get(&asset.id).copied().unwrap_or(0), &mut summary) else {
            continue;
        };
        if let Some(media) = clip.info.video.as_ref().map(|v| v.frame_rate) {
            rates.insert(asset.id.clone(), media);
        }
        let name = if asset.name.trim().is_empty() { asset.file_name.clone() } else { asset.name.clone() };
        let id = project.add_item(&name, Label::Iris, ItemKind::Media(Box::new(clip)), Some(footage));
        if let Some(item) = project.item_mut(id) {
            item.metadata.insert("nlAssetId".into(), asset.id.clone());
        }
        media.insert(asset.id.clone(), id);
        summary.items += 1;
    }

    let sequence_rate = rates.values().next().copied().unwrap_or(FrameRate::FPS_30);
    let (width, height) = aspect(&nl.aspect_ratio).unwrap_or_else(|| first_size(&nl).unwrap_or((1920, 1080)));
    let video_tracks = nl.timeline.tracks.iter().filter(|t| !t.kind.eq_ignore_ascii_case("audio")).count().max(1);
    let nl_audio_tracks = nl.timeline.tracks.iter().filter(|t| t.kind.eq_ignore_ascii_case("audio")).count();
    // Every video track gets an audio track for its clips' embedded sound, then the NL audio tracks.
    let audio_tracks = video_tracks + nl_audio_tracks;
    let settings = filmcraft_project::SequenceSettings {
        width,
        height,
        frame_rate: sequence_rate,
        preset: format!("{width}x{height} {:.3} fps", sequence_rate.num as f64 / sequence_rate.den.max(1) as f64),
        ..Default::default()
    };
    let sequence = project.new_sequence(&title, settings, video_tracks, audio_tracks, Some(sequences));
    if let Some(item) = project.item_mut(sequence) {
        item.metadata.insert("nlProjectDir".into(), dir.to_string_lossy().into_owned());
    }
    let track_map = map_tracks(&nl.timeline.tracks, video_tracks);
    let audio_assets: HashMap<&str, bool> = nl.assets.iter().map(|a| (a.id.as_str(), a.media.as_ref().is_some_and(|m| m.has_audio))).collect();
    // Give the tracks the NL names, so Video/Overlay/Audio read as they did in NL.
    if let Some(seq) = project.sequence_mut(sequence) {
        for track in &nl.timeline.tracks {
            if let Some(slot) = track_map.get(&track.id) {
                let list = match slot.kind {
                    TrackKind::Video => &mut seq.video_tracks,
                    TrackKind::Audio => &mut seq.audio_tracks,
                };
                if let Some(dst) = list.get_mut(slot.index)
                    && !track.name.trim().is_empty()
                {
                    dst.name = track.name.clone();
                }
            }
        }
        let video_names: Vec<String> = seq.video_tracks.iter().map(|t| t.name.clone()).collect();
        for (i, name) in video_names.iter().enumerate() {
            if let Some(track) = seq.audio_tracks.get_mut(i) {
                track.name = format!("{name} audio");
            }
        }
    }

    for clip in &nl.timeline.clips {
        let Some(&item) = media.get(&clip.asset_id) else {
            summary.warnings.push(format!("clip on unknown media `{}` skipped", clip.asset_id));
            continue;
        };
        let Some(slot) = track_map.get(&clip.track_id).copied() else {
            summary.warnings.push(format!("clip on unknown track `{}` skipped", clip.track_id));
            continue;
        };
        let start = sequence_rate.snap_nearest(Tick::from_units(clip.start_ms as i64, 1000));
        let source_in = Tick::from_units(clip.trim_in_ms as i64, 1000);
        let duration = Tick::from_units(clip.duration_ms as i64, 1000);
        if duration.0 <= 0 {
            summary.warnings.push(format!("clip with a zero or negative duration skipped: {}", clip.label));
            continue;
        }
        let range = TimeRange::new(source_in, duration);
        let Some(mut placed) = project.make_track_item(item, slot.kind, start, range, sequence_rate) else {
            summary.warnings.push(format!("media for clip `{}` is missing", clip.label));
            continue;
        };
        if !clip.label.trim().is_empty() {
            placed.name = clip.label.clone();
        }
        let clip_name = placed.name.clone();
        placed.scale_to_frame = clip.transform.as_ref().is_some_and(|t| t.fit.eq_ignore_ascii_case("contain"));
        set_volume(&mut placed, clip.volume);
        let linked = if slot.kind == TrackKind::Video && audio_assets.get(clip.asset_id.as_str()).copied().unwrap_or(false) {
            let link = project.alloc_id();
            placed.link = Some(link);
            Some(link)
        } else {
            None
        };
        if let Some(seq) = project.sequence_mut(sequence) {
            let list = match slot.kind {
                TrackKind::Video => &mut seq.video_tracks,
                TrackKind::Audio => &mut seq.audio_tracks,
            };
            if let Some(dst) = list.get_mut(slot.index) {
                dst.items.push(placed);
                summary.clips += 1;
            }
        }
        if let (Some(link), Some(video)) = (linked, slot.video) {
            let Some(mut sound) = project.make_track_item(item, TrackKind::Audio, start, range, sequence_rate) else { continue };
            sound.link = Some(link);
            sound.name = clip_name;
            set_volume(&mut sound, clip.volume);
            if let Some(seq) = project.sequence_mut(sequence)
                && let Some(dst) = seq.audio_tracks.get_mut(video)
            {
                dst.items.push(sound);
                summary.linked_audio += 1;
            }
        }
    }
    if let Some(seq) = project.sequence_mut(sequence) {
        for track in seq.video_tracks.iter_mut().chain(seq.audio_tracks.iter_mut()) {
            track.sort();
        }
    }

    for nl_transcript in &nl.transcripts {
        let Some(&item) = media.get(&nl_transcript.asset_id) else {
            summary.warnings.push("transcript for unknown media skipped".into());
            continue;
        };
        let transcript = transcript(nl_transcript);
        if transcript.words.is_empty() {
            summary.warnings.push("transcript without words skipped".into());
            continue;
        }
        project.transcripts.insert(item, Arc::new(transcript));
        summary.transcripts += 1;
    }

    Ok((project, summary))
}

fn media_clip(dir: &Path, asset: &NlAsset, source_end_ms: u64, summary: &mut Summary) -> Option<MediaClip> {
    let file_name = if asset.file_name.trim().is_empty() { asset.name.clone() } else { asset.file_name.clone() };
    if file_name.trim().is_empty() {
        summary.warnings.push("media with no file name skipped".into());
        return None;
    }
    let path = dir.join("assets").join(&file_name);
    if !path.is_file() {
        summary.missing_media += 1;
    }
    let info = asset.media.as_ref();
    let duration_ms = info.map(|m| m.duration_ms).unwrap_or(0).max(source_end_ms);
    let kind = match asset.kind.to_ascii_lowercase().as_str() {
        "audio" => MediaKind::AudioOnly,
        "image" | "photo" | "still" => MediaKind::Still,
        _ => MediaKind::Movie,
    };
    let (width, height) = info.and_then(|m| m.width.zip(m.height)).filter(|(w, h)| *w > 0 && *h > 0).unwrap_or((1920, 1080));
    let rate = rate(info.and_then(|m| m.frame_rate));
    let video = (kind != MediaKind::AudioOnly).then(|| VideoStreamInfo {
        width,
        height,
        frame_rate: rate,
        par: (1, 1),
        codec: String::new(),
        pixel_format: String::new(),
        color: ColorInfo::default(),
        has_alpha: false,
        bitrate: None,
        hdr: None,
        color_assumed: None,
    });
    let audio_streams = if info.is_some_and(|m| m.has_audio) || kind == MediaKind::AudioOnly {
        vec![AudioStreamInfo { sample_rate: 48_000, channels: 2, codec: String::new(), bits_per_sample: None }]
    } else {
        Vec::new()
    };
    let extension = Path::new(&file_name).extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_uppercase();
    Some(MediaClip {
        media: MediaRef::File { path: path.to_string_lossy().into_owned() },
        info: MediaInfo {
            name: if asset.name.trim().is_empty() { file_name.clone() } else { asset.name.clone() },
            kind,
            duration: Tick::from_units(duration_ms as i64, 1000),
            video,
            audio_streams,
            container: if extension.is_empty() { "unknown".into() } else { extension },
            start_timecode: None,
            file_size: std::fs::metadata(&path).ok().map(|m| m.len()),
        },
        interpret: Default::default(),
        mark_in: None,
        mark_out: None,
        markers: Vec::new(),
        offline: false,
        proxy: None,
        identity: None,
    })
}

/// `file.syncNlText {path?}`: write a copy of the source NL project with the current FilmCraft
/// transcripts, leaving every other NL section (longform, shorts, subtitles, …) untouched.
pub(crate) fn sync_text(s: &mut Session, p: &Value) -> Result<Value> {
    let sequence = s.state.active_sequence.ok_or(EngineError::NoSequence)?;
    let dir = s
        .project
        .item(sequence)
        .and_then(|i| i.metadata.get("nlProjectDir"))
        .cloned()
        .ok_or_else(|| EngineError::Other("this project was not imported from an NL project, so it has no NL mirror".into()))?;
    let source = Path::new(&dir).join("project.json");
    let out = p.get("path").and_then(Value::as_str).map(std::path::PathBuf::from).unwrap_or_else(|| Path::new(&dir).join("project.synced.json"));
    let bytes =
        s.services.read_file_bounded(&source.to_string_lossy(), 64 * 1024 * 1024).map_err(|e| EngineError::Other(format!("{}: {e}", source.display())))?;
    let mut root: Value = serde_json::from_slice(&bytes).map_err(|e| EngineError::Other(format!("{}: {e}", source.display())))?;
    let (transcripts, words) = patch_transcripts(&mut root, &s.project);
    if transcripts == 0 {
        return Err(EngineError::Other("no imported NL transcript to sync".into()));
    }
    let out_text = out.to_string_lossy().into_owned();
    let data = serde_json::to_vec_pretty(&root).map_err(|e| EngineError::Other(format!("NL mirror: {e}")))?;
    s.services.write_file(&out_text, &data).map_err(|e| EngineError::Other(format!("{out_text}: {e}")))?;
    Ok(json!({"path": out_text, "transcripts": transcripts, "words": words}))
}

fn patch_transcripts(root: &mut Value, project: &Project) -> (usize, usize) {
    let mut updates: HashMap<String, Value> = HashMap::new();
    let mut total = 0usize;
    for (id, transcript) in &project.transcripts {
        let Some(asset) = project.item(*id).and_then(|i| i.metadata.get("nlAssetId")) else { continue };
        let words: Vec<Value> = transcript
            .words
            .iter()
            .filter(|w| !w.text.trim().is_empty())
            .map(|w| {
                let start = w.start.to_units_floor(1000).max(0);
                let end = w.end.max(w.start).to_units_floor(1000).max(start);
                json!({"start_ms": start, "end_ms": end, "text": w.text.clone()})
            })
            .collect();
        total += words.len();
        updates.insert(asset.clone(), json!({"words": words, "language": transcript.language.clone()}));
    }
    let Some(list) = root.get_mut("transcripts").and_then(Value::as_array_mut) else { return (0, 0) };
    let mut count = 0usize;
    for item in list {
        let Some(asset) = item.get("asset_id").and_then(Value::as_str) else { continue };
        let Some(update) = updates.get(asset) else { continue };
        if let Some(words) = update.get("words") {
            item["words"] = words.clone();
        }
        if let Some(language) = update.get("language")
            && language.as_str().is_some_and(|l| !l.is_empty())
        {
            item["language"] = language.clone();
        }
        count += 1;
    }
    (count, total)
}

fn transcript(nl: &NlTranscript) -> Transcript {
    let mut words: Vec<Word> = nl
        .words
        .iter()
        .filter(|w| !w.text.trim().is_empty())
        .map(|w| Word::new(w.text.trim(), Tick::from_units(w.start_ms as i64, 1000), Tick::from_units(w.end_ms.max(w.start_ms) as i64, 1000)))
        .collect();
    if words.is_empty() {
        // Older NL projects may carry only segments; split each one and spread its duration by
        // character count, so the Text panel still has word-level anchors.
        for segment in &nl.segments {
            let parts: Vec<&str> = segment.text.split_whitespace().collect();
            let total: usize = parts.iter().map(|w| w.chars().count().max(1)).sum();
            let start = segment.start_ms as i64;
            let span = segment.end_ms.saturating_sub(segment.start_ms) as i64;
            let mut at = 0i64;
            for part in parts {
                let share = (span * part.chars().count().max(1) as i64 / total.max(1) as i64).max(1);
                let from = start + at;
                let to = (from + share).min(segment.end_ms as i64);
                words.push(Word::new(part, Tick::from_units(from, 1000), Tick::from_units(to.max(from), 1000)));
                at += share;
            }
        }
    }
    let mut t = Transcript { language: nl.language.clone(), source: "imported:nl-video-suite".into(), speakers: Vec::new(), words };
    t.normalize();
    t
}

fn map_tracks(tracks: &[NlTrack], video_tracks: usize) -> HashMap<String, TrackSlot> {
    let mut map = HashMap::new();
    let mut video = 0usize;
    let mut audio = 0usize;
    for track in tracks {
        if track.id.is_empty() {
            continue;
        }
        let is_audio = track.kind.eq_ignore_ascii_case("audio");
        let slot = if is_audio {
            let index = video_tracks + audio;
            audio += 1;
            TrackSlot { kind: TrackKind::Audio, index, video: None }
        } else {
            let index = video;
            video += 1;
            TrackSlot { kind: TrackKind::Video, index, video: Some(index) }
        };
        map.insert(track.id.clone(), slot);
    }
    map
}

fn set_volume(item: &mut filmcraft_project::TrackItem, volume: f64) {
    if !volume.is_finite() || (volume - 1.0).abs() <= 0.0001 {
        return;
    }
    let db = if volume <= 0.0 { -60.0 } else { (20.0 * volume.log10()).clamp(-60.0, 15.0) };
    if let Some(effect) = item.effects.iter_mut().find(|e| e.effect == "volume")
        && let Some(level) = effect.params.get_mut("level")
    {
        level.value = filmcraft_project::ParamValue::Float(db);
    }
}

/// The furthest source point any clip uses from each asset, so a media whose metadata lacks a
/// duration still has a sensible one.
fn source_ends(nl: &NlProject) -> HashMap<String, u64> {
    let mut ends: HashMap<String, u64> = HashMap::new();
    for clip in &nl.timeline.clips {
        let end = clip.trim_in_ms.saturating_add(clip.duration_ms);
        ends.entry(clip.asset_id.clone()).and_modify(|v| *v = (*v).max(end)).or_insert(end);
    }
    ends
}

fn aspect(text: &str) -> Option<(u32, u32)> {
    let parts: Vec<u32> = text.split([':', 'x', 'X']).filter_map(|p| p.trim().parse().ok()).collect();
    let (a, b) = match parts.as_slice() {
        [w, h] if *w > 0 && *h > 0 => (*w, *h),
        _ => return None,
    };
    if a >= 128 || b >= 128 {
        return Some((a, b));
    }
    // A ratio such as 16:9 or 9:16: use the usual 1080-wide or 1080-tall frame.
    if a == b {
        return Some((1080, 1080));
    }
    if a > b {
        Some((1920, ((1920.0 * f64::from(b) / f64::from(a)).round() as u32).max(2)))
    } else {
        Some((((1920.0 * f64::from(a) / f64::from(b)).round() as u32).max(2), 1920))
    }
}

fn first_size(nl: &NlProject) -> Option<(u32, u32)> {
    nl.assets.iter().find_map(|a| a.media.as_ref().and_then(|m| m.width.zip(m.height)).filter(|(w, h)| *w > 0 && *h > 0))
}

fn rate(fps: Option<f64>) -> FrameRate {
    let Some(fps) = fps.filter(|f| f.is_finite() && *f > 0.0 && *f <= 1000.0) else { return FrameRate::FPS_30 };
    for (value, rate) in [
        (23.976, FrameRate::FPS_23_976),
        (24.0, FrameRate::FPS_24),
        (25.0, FrameRate::FPS_25),
        (29.97, FrameRate::FPS_29_97),
        (30.0, FrameRate::FPS_30),
        (48.0, FrameRate::FPS_48),
        (50.0, FrameRate::FPS_50),
        (59.94, FrameRate::FPS_59_94),
        (60.0, FrameRate::FPS_60),
        (119.88, FrameRate::FPS_119_88),
        (120.0, FrameRate::FPS_120),
    ] {
        if (fps - value).abs() < 0.02 {
            return rate;
        }
    }
    FrameRate::new((fps * 1000.0).round() as i64, 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NL: &str = r#"{
      "title": "Test NL",
      "aspect_ratio": "1080:1920",
      "assets": [{
        "id": "a1", "name": "take.mp4", "kind": "video", "file_name": "take.mp4",
        "media": {"duration_ms": 12000, "width": 3840, "height": 2160, "frame_rate": 30.0, "has_audio": true}
      }],
      "timeline": {
        "tracks": [
          {"id": "v", "name": "video", "kind": "video"},
          {"id": "a", "name": "audio", "kind": "audio"}
        ],
        "clips": [
          {"asset_id": "a1", "track_id": "v", "start_ms": 0, "trim_in_ms": 1000, "duration_ms": 2000, "label": "one"},
          {"asset_id": "a1", "track_id": "v", "start_ms": 2000, "trim_in_ms": 3000, "duration_ms": 2000, "label": "two"},
          {"asset_id": "a1", "track_id": "a", "start_ms": 0, "trim_in_ms": 1000, "duration_ms": 4000, "label": "sound", "volume": 0.5}
        ]
      },
      "transcripts": [{
        "asset_id": "a1", "language": "it",
        "segments": [],
        "words": [
          {"start_ms": 0, "end_ms": 500, "text": "ciao"},
          {"start_ms": 500, "end_ms": 900, "text": "a"},
          {"start_ms": 900, "end_ms": 1200, "text": "tutti"},
          {"start_ms": 1200, "end_ms": 1600, "text": "ok"}
        ]
      }]
    }"#;

    #[test]
    fn converts_media_tracks_clips_and_transcript() {
        let dir = std::env::temp_dir().join(format!("nl-project-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        let (project, summary) = convert(&dir, NL).unwrap();
        assert_eq!(summary.items, 1);
        assert_eq!(summary.clips, 3);
        assert_eq!(summary.linked_audio, 2);
        assert_eq!(summary.transcripts, 1);
        assert_eq!(summary.missing_media, 1);
        let seq_item = project.sequences().next().unwrap();
        let seq = seq_item.as_sequence().unwrap();
        assert_eq!((seq.settings.width, seq.settings.height), (1080, 1920));
        assert_eq!(seq.video_tracks.len(), 1);
        assert_eq!(seq.audio_tracks.len(), 2);
        assert_eq!(seq.video_tracks[0].items.len(), 2);
        assert_eq!(seq.audio_tracks[0].items.len(), 2, "linked audio for the video clips");
        assert_eq!(seq.audio_tracks[1].items.len(), 1, "the NL audio track");
        assert_eq!(seq.video_tracks[0].items[0].source_in, Tick::from_units(1000, 1000));
        assert!(seq_item.metadata.contains_key("nlProjectDir"));
        let media = project.items.values().find(|i| i.as_media().is_some()).unwrap();
        assert_eq!(media.metadata.get("nlAssetId").map(String::as_str), Some("a1"));
        let transcript = project.transcripts.get(&media.id).unwrap();
        assert_eq!(transcript.words.len(), 4);
        assert_eq!(transcript.words[2].text, "tutti");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn aspect_accepts_pixels_and_ratios() {
        assert_eq!(aspect("1080:1920"), Some((1080, 1920)));
        assert_eq!(aspect("16:9"), Some((1920, 1080)));
        assert_eq!(aspect("9:16"), Some((1080, 1920)));
        assert_eq!(aspect("1:1"), Some((1080, 1080)));
    }

    #[test]
    fn text_sync_updates_only_the_transcripts() {
        let dir = std::env::temp_dir().join(format!("nl-project-sync-{}", std::process::id()));
        let (mut project, _) = convert(&dir, NL).unwrap();
        let media = project.items.values().find(|i| i.as_media().is_some()).unwrap().id;
        Arc::make_mut(project.transcripts.get_mut(&media).unwrap()).words[0].text = "salve".into();
        let mut root: Value = serde_json::from_str(NL).unwrap();
        root["longform"] = json!({"keep": true});
        let (count, words) = patch_transcripts(&mut root, &project);
        assert_eq!((count, words), (1, 4));
        assert_eq!(root["transcripts"][0]["words"][0]["text"], "salve");
        assert_eq!(root["longform"]["keep"], true);
        assert_eq!(root["assets"][0]["name"], "take.mp4");
    }

    #[test]
    fn segments_become_words_when_word_timings_are_missing() {
        let json = r#"{
          "title": "Segments",
          "assets": [{"id":"a","name":"a.wav","kind":"audio","file_name":"a.wav","media":{"duration_ms":2000,"has_audio":true}}],
          "timeline": {"tracks":[{"id":"t","name":"audio","kind":"audio"}],"clips":[]},
          "transcripts": [{"asset_id":"a","language":"it","segments":[{"start_ms":0,"end_ms":1000,"text":"due parole"}],"words":[]}]
        }"#;
        let dir = std::env::temp_dir().join(format!("nl-project-segments-{}", std::process::id()));
        let (project, summary) = convert(&dir, json).unwrap();
        assert_eq!(summary.transcripts, 1);
        let words = &project.transcripts.values().next().unwrap().words;
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "due");
        assert!(words[1].start >= words[0].start);
    }
}
