//! Longform analysis: chapters for a long recording, read from the transcript the ASR step
//! produced, written by the local model and turned into timeline markers.
//!
//! The transcript is the source of truth for timing: the model only proposes the chapter
//! boundaries from numbered time-stamped blocks, and every proposed time is snapped to a real
//! word boundary, so a chapter cannot point at a time the media does not have.
//!
//! Long analyses run as a background job (`wait: false`); the result lands in
//! `s.nl.results["longform"]` and the panel offers it.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use filmcraft_project::{ItemId, Transcript};
use filmcraft_time::Tick;
use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{EngineError, Result, Session};

use super::state::PendingLongform;

/// One block of transcript text the model sees: 20 seconds or a long pause end it.
const CHUNK_SECONDS: f64 = 20.0;
const CHUNK_MAX_WORDS: usize = 60;
const MAX_CHUNKS: usize = 200;
const MAX_CHAPTERS: usize = 24;
const MAX_TITLE: usize = 120;
const MAX_SUMMARY: usize = 400;

fn bad_cmd(cmd: &str, msg: impl ToString) -> EngineError {
    bad(cmd, msg.to_string())
}

/// One transcript block with its timeline times (seconds).
pub struct Chunk {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// The clip and transcript the analysis should use: `item` picks that media, otherwise the first
/// clip of the active sequence that already has a transcript.
fn pick_source(s: &Session, item: Option<u64>) -> Option<(ItemId, Arc<Transcript>, Vec<Chunk>, f64, i64)> {
    let id = s.state.active_sequence?;
    let sequence = s.project.sequence(id)?;
    let mut candidates: Vec<(ItemId, i64)> = Vec::new();
    for track in sequence.video_tracks.iter().chain(sequence.audio_tracks.iter()) {
        for clip in &track.items {
            if !clip.enabled || !s.project.transcripts.contains_key(&clip.item) {
                continue;
            }
            if item.is_some_and(|item| item != clip.item.0) {
                continue;
            }
            candidates.push((clip.item, clip.start.0));
        }
    }
    candidates.sort_by_key(|(_, start)| *start);
    let (media, _) = candidates.first().copied()?;
    let transcript = s.project.transcripts.get(&media)?.clone();
    let offset: i64 = {
        let clip = sequence
            .video_tracks
            .iter()
            .chain(sequence.audio_tracks.iter())
            .flat_map(|track| track.items.iter())
            .find(|clip| clip.item == media && clip.enabled)?;
        // Place the media times on the timeline: a trimmed clip starts later on the sequence.
        (clip.start - clip.source_in).0
    };
    let duration = sequence.duration().seconds();
    let chunks = chunks_from_words(&transcript, offset, duration);
    Some((media, transcript, chunks, duration, offset))
}

/// Group words into time-stamped blocks: a pause over a second, 20 seconds or 60 words end one.
fn chunks_from_words(transcript: &Transcript, offset_ticks: i64, media_end: f64) -> Vec<Chunk> {
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut current: Option<Chunk> = None;
    let mut words = 0;
    let mut last_end = f64::NEG_INFINITY;
    for word in &transcript.words {
        let start = ((word.start.0 + offset_ticks) as f64 / filmcraft_time::TICKS_PER_SECOND as f64).max(0.0);
        let end = ((word.end.0 + offset_ticks) as f64 / filmcraft_time::TICKS_PER_SECOND as f64).min(media_end.max(0.0));
        if end <= start {
            continue;
        }
        let split = current.as_ref().is_some_and(|chunk| start - last_end > 1.0 || start - chunk.start >= CHUNK_SECONDS || words >= CHUNK_MAX_WORDS);
        if split {
            if let Some(chunk) = current.take() {
                chunks.push(chunk);
            }
            words = 0;
        }
        match &mut current {
            Some(chunk) => {
                chunk.text.push(' ');
                chunk.text.push_str(&word.text);
                chunk.end = end.max(chunk.end);
            }
            None => {
                current = Some(Chunk { start, end, text: word.text.clone() });
            }
        }
        words += 1;
        last_end = end;
    }
    if let Some(chunk) = current.take() {
        chunks.push(chunk);
    }
    chunks.truncate(MAX_CHUNKS);
    // Keep the model prompt bounded: a block carries at most ~600 characters of text.
    for chunk in &mut chunks {
        chunk.text = chunk.text.trim().chars().take(600).collect();
    }
    chunks
}

/// The prompt the model answers: numbered blocks between their real times.
fn analysis_prompt(chunks: &[Chunk], language: &str, max_chapters: usize) -> String {
    let mut user = format!(
        "Lingua del testo: {language}. Dividi la trascrizione in capitoli significativi (massimo {max_chapters}). \
Rispondi SOLO con un oggetto JSON valido {{\"chapters\":[{{\"startSec\":number,\"endSec\":number,\"title\":string,\"summary\":string}}]}}, senza testo attorno. \
I tempi di ogni capitolo devono coincidere con i tempi dei blocchi qui sotto (inizio del primo blocco, fine dell'ultimo), i capitoli devono coprire tutto il video in ordine, senza sovrapposizioni.\n\nBlocchi:"
    );
    for (index, chunk) in chunks.iter().enumerate() {
        user.push_str(&format!("\n{}. {:.1}s–{:.1}s: {}", index + 1, chunk.start, chunk.end, chunk.text));
    }
    user
}

/// Keep only usable chapters: sorted, inside the media, snapped to block boundaries, bounded text.
pub fn sanitize_chapters(value: &Value, chunks: &[Chunk], duration: f64, max_chapters: usize) -> Vec<Value> {
    let Some(list) = value.get("chapters").and_then(Value::as_array) else { return Vec::new() };
    let mut chapters: Vec<Value> = Vec::new();
    for chapter in list.iter().take(max_chapters) {
        let Some(object) = chapter.as_object() else { continue };
        let title = object.get("title").and_then(Value::as_str).unwrap_or_default().trim().chars().take(MAX_TITLE).collect::<String>();
        let summary = object.get("summary").and_then(Value::as_str).unwrap_or_default().trim().chars().take(MAX_SUMMARY).collect::<String>();
        let start = object.get("startSec").and_then(Value::as_f64).filter(|start| start.is_finite()).unwrap_or(0.0).clamp(0.0, duration.max(0.0));
        let end = object.get("endSec").and_then(Value::as_f64).filter(|end| end.is_finite()).unwrap_or(start).clamp(start, duration.max(start));
        if title.is_empty() || end <= start {
            continue;
        }
        // Snap to the nearest block boundary at or before the proposed time, so a chapter starts
        // where a sentence starts.
        let snap_start = chunks.iter().rev().find(|chunk| chunk.start <= start + 0.001).map(|chunk| chunk.start).unwrap_or(start);
        let snap_end = chunks.iter().find(|chunk| chunk.end >= end - 0.001).map(|chunk| chunk.end).unwrap_or(end);
        if snap_end <= snap_start {
            continue;
        }
        chapters.push(json!({"startSec": snap_start, "endSec": snap_end, "title": title, "summary": summary}));
    }
    chapters.sort_by(|a, b| a["startSec"].as_f64().partial_cmp(&b["startSec"].as_f64()).unwrap_or(std::cmp::Ordering::Equal));
    // Drop overlaps, keeping the earlier chapter.
    let mut out: Vec<Value> = Vec::new();
    for chapter in chapters {
        let start = chapter["startSec"].as_f64().unwrap_or(0.0);
        if let Some(previous) = out.last()
            && start < previous["endSec"].as_f64().unwrap_or(0.0) - 0.001
        {
            continue;
        }
        out.push(chapter);
    }
    out
}

/// `nl.longform.analyze` — chapters for the recording with a transcript.
fn analyze(s: &mut Session, p: &Value) -> Result<Value> {
    let item = p.get("item").and_then(Value::as_u64);
    let Some((media, transcript, chunks, duration, offset)) = pick_source(s, item) else {
        return Err(bad_cmd(
            "nl.longform.analyze",
            "no transcript on the timeline: run the ASR step first (NL ASR panel or transcript.transcribeNl), then analyze",
        ));
    };
    if chunks.is_empty() {
        return Err(bad_cmd(
            "nl.longform.analyze",
            format!(
                "the transcript of item {} has {} word(s) but none fall inside the sequence ({:.1} s); offset {} ticks, first word {:.2}-{:.2} s",
                media.0,
                transcript.words.len(),
                duration,
                offset,
                transcript.words.first().map(|word| word.start.seconds()).unwrap_or(0.0),
                transcript.words.first().map(|word| word.end.seconds()).unwrap_or(0.0)
            ),
        ));
    }
    let llm = super::llm::LlmConfig::for_session(s, p.get("model").and_then(Value::as_str));
    let model = llm.model.clone();
    let language = p.get("language").and_then(Value::as_str).unwrap_or("it").to_string();
    let max_chapters = p.get("maxChapters").and_then(Value::as_u64).unwrap_or(10).clamp(1, MAX_CHAPTERS as u64) as usize;
    let user = analysis_prompt(&chunks, &language, max_chapters);
    let system = "Sei un editor video esperto. Rispondi SOLO con JSON valido, senza markdown.";
    let wait = p.get("wait").and_then(Value::as_bool).unwrap_or(false);
    let chunks = Arc::new(chunks);
    let finish = |value: Value| -> Value {
        let chapters = sanitize_chapters(&value, &chunks, duration, max_chapters);
        json!({
            "item": media.0,
            "durationSec": duration,
            "chapters": chapters,
            "blocks": chunks.len(),
            "model": model,
        })
    };
    if wait {
        let value = llm.auto_json(system, &user, Duration::from_secs(300), Some("0")).map_err(|e| bad_cmd("nl.longform.analyze", e))?;
        let result = finish(value);
        s.nl.longform = Some(result.clone());
        s.nl.results.insert("longform", result.clone());
        return Ok(result);
    }
    let job_result: Arc<Mutex<Option<std::result::Result<Value, String>>>> = Arc::default();
    let out = job_result.clone();
    let id = s.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let job = crate::Job { id, label: format!("NL longform ({model})"), progress: Default::default(), result: Default::default() };
    let progress = job.progress.clone();
    let llm_run = llm.clone();
    let model_run = llm_run.model.clone();
    let chunks_run = chunks.clone();
    s.jobs.push(job);
    s.nl.longform_pending = Some(PendingLongform { job: id, result: job_result, epoch: s.document_epoch });
    std::thread::Builder::new()
        .name("filmcraft-nl-longform".into())
        .spawn(move || {
            *progress.status.lock().unwrap_or_else(|e| e.into_inner()) = format!("NL longform: analyzing with {model_run}");
            let value = llm_run
                .auto_json(system, &user, Duration::from_secs(300), Some("0"))
                .map(|value| {
                    let chapters = sanitize_chapters(&value, &chunks_run, duration, max_chapters);
                    json!({"item": media.0, "durationSec": duration, "chapters": chapters, "blocks": chunks_run.len(), "model": model_run})
                })
                .map_err(|e| e.to_string());
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
            progress.finished.store(true, std::sync::atomic::Ordering::Relaxed);
        })
        .map_err(|e| EngineError::Other(format!("can't start the longform thread: {e}")))?;
    Ok(json!({"job": id, "model": model, "blocks": chunks.len()}))
}
/// `nl.longform.get` — the chapters of the last analysis.
fn longform_get(s: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!({"longform": s.nl.longform.clone().unwrap_or(Value::Null)}))
}

/// Chapter text as Markdown: one entry per chapter with its time range and summary.
pub fn to_markdown(chapters: &[Value]) -> String {
    let mut out = String::from("# Chapters\n\n");
    for chapter in chapters {
        let start = chapter["startSec"].as_f64().unwrap_or(0.0);
        let end = chapter["endSec"].as_f64().unwrap_or(start);
        out.push_str(&format!("- [{start:.1}s–{end:.1}s] {}\n", chapter["title"].as_str().unwrap_or("")));
        let summary = chapter["summary"].as_str().unwrap_or_default();
        if !summary.is_empty() {
            out.push_str(&format!("  {summary}\n"));
        }
    }
    out
}

/// Chapter text as an SRT where each chapter is one cue (title and, if any, its summary).
pub fn to_srt(chapters: &[Value]) -> String {
    let time = |seconds: f64| {
        let total = seconds.max(0.0);
        let hours = (total / 3600.0) as u64;
        let minutes = ((total % 3600.0) / 60.0) as u64;
        let secs = total % 60.0;
        format!("{hours:02}:{minutes:02}:{secs:06.3}")
    };
    let mut out = String::new();
    for (index, chapter) in chapters.iter().enumerate() {
        let start = chapter["startSec"].as_f64().unwrap_or(0.0);
        let end = chapter["endSec"].as_f64().unwrap_or(start);
        let title = chapter["title"].as_str().unwrap_or("");
        let summary = chapter["summary"].as_str().unwrap_or_default();
        out.push_str(&format!("{}\n{} --> {}\n{title}", index + 1, time(start).replace('.', ","), time(end).replace('.', ",")));
        if !summary.is_empty() {
            out.push_str(&format!("\n{summary}"));
        }
        out.push_str("\n\n");
    }
    out
}

/// `nl.longform.markers` — add a marker at the start of every chapter.
fn longform_markers(s: &mut Session, p: &Value) -> Result<Value> {
    let chapters = p
        .get("chapters")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| s.nl.longform.as_ref().and_then(|longform| longform["chapters"].as_array().cloned()))
        .unwrap_or_default();
    if chapters.is_empty() {
        return Err(bad_cmd("nl.longform.markers", "no chapters yet: run nl.longform.analyze first"));
    }
    let mut added = Vec::new();
    for chapter in chapters.iter().take(MAX_CHAPTERS) {
        let Some(start) = chapter.get("startSec").and_then(Value::as_f64) else { continue };
        let title = chapter.get("title").and_then(Value::as_str).unwrap_or("Chapter");
        let summary = chapter.get("summary").and_then(Value::as_str).unwrap_or_default();
        let params = json!({"time": Tick::from_seconds_f64(start).0, "name": title, "comment": summary});
        match s.execute("markers.add", params) {
            Ok(value) => added.push(json!({"marker": value, "title": title, "startSec": start})),
            Err(e) => added.push(json!({"error": e.to_string(), "title": title})),
        }
    }
    if added.iter().all(|entry| entry.get("error").is_some()) {
        return Err(bad_cmd("nl.longform.markers", "the timeline refused every marker"));
    }
    s.toast(format!("NL longform: {} chapter marker(s) added", added.iter().filter(|entry| entry.get("error").is_none()).count()));
    Ok(json!({"markers": added}))
}

/// `nl.longform.export` — write the chapters to a file (Markdown, SRT or JSON, from the extension).
fn longform_export(s: &mut Session, p: &Value) -> Result<Value> {
    let chapters = p
        .get("chapters")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| s.nl.longform.as_ref().and_then(|longform| longform["chapters"].as_array().cloned()))
        .unwrap_or_default();
    if chapters.is_empty() {
        return Err(bad_cmd("nl.longform.export", "no chapters yet: run nl.longform.analyze first"));
    }
    let path = p
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| bad_cmd("nl.longform.export", "need `path` (an absolute file path)"))?;
    let file = std::path::Path::new(path);
    if !file.is_absolute() {
        return Err(bad_cmd("nl.longform.export", "`path` must be absolute"));
    }
    let declared = p.get("format").and_then(Value::as_str).map(str::to_ascii_lowercase);
    let from_extension = file.extension().and_then(|extension| extension.to_str()).map(str::to_ascii_lowercase);
    let format = declared.or(from_extension).unwrap_or_else(|| "json".into());
    let (text, format) = match format.as_str() {
        "md" | "markdown" => (to_markdown(&chapters), "md"),
        "srt" => (to_srt(&chapters), "srt"),
        "json" => (serde_json::to_string_pretty(&json!({"chapters": chapters})).unwrap_or_default(), "json"),
        other => return Err(bad_cmd("nl.longform.export", format!("unknown format `{other}`: use json, md or srt"))),
    };
    std::fs::write(file, &text).map_err(|e| bad_cmd("nl.longform.export", format!("{}: {e}", file.display())))?;
    Ok(json!({"path": path, "format": format, "chapters": chapters.len(), "bytes": text.len()}))
}
/// Move a finished analysis into the panel result.
pub(crate) fn poll(s: &mut Session) {
    let finished =
        s.nl.longform_pending
            .as_ref()
            .is_none_or(|pending| s.jobs.iter().find(|j| j.id == pending.job).is_none_or(|j| j.progress.finished.load(std::sync::atomic::Ordering::Relaxed)));
    if !finished {
        return;
    }
    let Some(pending) = s.nl.longform_pending.take() else { return };
    let Some(result) = pending.result.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
    if pending.epoch != s.document_epoch {
        s.error_toast("nl.longform.analyze", "the project was replaced; the analysis was discarded");
        return;
    }
    match result {
        Ok(value) => {
            let chapters = value["chapters"].as_array().map_or(0, Vec::len);
            s.nl.longform = Some(value.clone());
            s.nl.results.insert("longform", value);
            s.toast(format!("NL longform: {chapters} chapter(s) ready"));
        }
        Err(e) => s.error_toast("nl.longform.analyze", format!("NL longform: {e}")),
    }
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    let spec = |id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>| -> CommandSpec {
        CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: always, run, journal: false, named: crate::commands::NamedKeys::NONE }
    };
    vec![
        spec("nl.longform.analyze", "NL Longform Chapters", r#"{"item":id?,"language":str?,"model":str?,"maxChapters":int?,"wait":bool?}"#, analyze),
        spec("nl.longform.get", "NL Longform Result", "{}", longform_get),
        spec("nl.longform.markers", "NL Chapter Markers", r#"{"chapters":[obj]?}"#, longform_markers),
        spec("nl.longform.export", "NL Export Chapters", r#"{"path":str,"format":"json|md|srt"?,"chapters":[obj]?}"#, longform_export),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use filmcraft_project::{Speaker, Word};

    fn transcript(words: &[(&str, f64, f64)]) -> Transcript {
        Transcript {
            language: "it".into(),
            source: "test".into(),
            speakers: vec![Speaker { name: "Ada".into() }],
            words: words
                .iter()
                .map(|(text, start, end)| Word {
                    text: text.to_string(),
                    start: Tick::from_seconds_f64(*start),
                    end: Tick::from_seconds_f64(*end),
                    speaker: None,
                    confidence: 1.0,
                })
                .collect(),
        }
    }

    #[test]
    fn blocks_split_on_pauses_and_duration() {
        let transcript = transcript(&[("ciao", 0.0, 0.4), ("a", 0.5, 0.6), ("tutti", 1.5, 1.9), ("fine", 25.0, 25.4)]);
        let chunks = chunks_from_words(&transcript, 0, 60.0);
        assert_eq!(chunks.len(), 2, "{:?}", chunks.iter().map(|c| (&c.text, c.start)).collect::<Vec<_>>());
        assert!(chunks[0].text.starts_with("ciao a tutti"));
        assert_eq!(chunks[1].text, "fine");
        assert!(chunks[1].start >= 25.0);
    }

    #[test]
    fn a_trimmed_clip_moves_the_words_to_their_timeline_position() {
        let transcript = transcript(&[("ciao", 10.0, 10.4)]);
        let offset = Tick::from_seconds_f64(5.0).0 - Tick::from_seconds_f64(8.0).0;
        let chunks = chunks_from_words(&transcript, offset, 60.0);
        assert!((chunks[0].start - 7.0).abs() < 0.001, "{:?}", chunks[0].start);
    }

    #[test]
    fn chapters_are_sorted_snapped_and_bounded() {
        let chunks = vec![
            Chunk { start: 0.0, end: 10.0, text: "uno".into() },
            Chunk { start: 10.0, end: 20.0, text: "due".into() },
            Chunk { start: 20.0, end: 30.0, text: "tre".into() },
        ];
        let value = json!({"chapters": [
            {"startSec": 21.0, "endSec": 31.0, "title": "Terzo", "summary": "fine"},
            {"startSec": 0.4, "endSec": 9.0, "title": "Primo", "summary": "inizio"},
            {"startSec": 50.0, "endSec": 60.0, "title": "", "summary": "senza titolo"},
            {"startSec": 5.0, "endSec": 5.0, "title": "Vuoto", "summary": ""}
        ]});
        let chapters = sanitize_chapters(&value, &chunks, 30.0, 10);
        assert_eq!(chapters.len(), 2, "{chapters:?}");
        assert_eq!(chapters[0]["title"], "Primo");
        assert!(chapters[0]["startSec"].as_f64().unwrap() <= 0.001);
        assert_eq!(chapters[1]["title"], "Terzo");
        assert!(chapters[1]["endSec"].as_f64().unwrap() <= 30.0);
    }

    #[test]
    fn chapters_export_to_markdown_and_srt() {
        let chapters = vec![
            json!({"startSec": 0.0, "endSec": 2.4, "title": "Intro", "summary": "Si comincia"}),
            json!({"startSec": 4.1, "endSec": 13.3, "title": "Esercizi", "summary": ""}),
        ];
        let markdown = to_markdown(&chapters);
        assert!(markdown.contains("# Chapters"));
        assert!(markdown.contains("[0.0s–2.4s] Intro"), "{markdown}");
        let srt = to_srt(&chapters);
        assert!(srt.contains("00:00:00,000 --> 00:00:02,400"), "{srt}");
        assert!(srt.contains("Intro") && srt.contains("Si comincia"));
        assert!(srt.contains("00:00:04,100 --> 00:00:13,300"));
    }

    #[test]
    fn exporting_without_chapters_or_with_a_relative_path_is_refused() {
        let mut s = Session::default();
        assert!(s.execute("nl.longform.export", json!({"path": "C:/tmp/x.json"})).is_err(), "no chapters");
        s.nl.longform = Some(json!({"chapters": [{"startSec": 0.0, "endSec": 1.0, "title": "A"}]}));
        assert!(s.execute("nl.longform.export", json!({"path": "relative.json"})).is_err(), "relative path");
        assert!(s.execute("nl.longform.export", json!({"path": "C:/tmp/x.json", "format": "pdf"})).is_err(), "unknown format");
    }

    #[test]
    fn analyzing_without_a_transcript_is_refused() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        let error = s.execute("nl.longform.analyze", json!({"wait": true})).unwrap_err().to_string();
        assert!(error.contains("no transcript"), "{error}");
        assert!(s.execute("nl.longform.markers", json!({})).is_err(), "no chapters yet");
    }
}
