//! Find short candidates from the sequence transcript and derive a sequence per candidate.
//!
//! Two rankers feed the same output shape: a local transcript heuristic (no network) and an
//! optional Ollama pass that selects moments and quotes a hook. Both refine the boundaries into
//! the gaps between words and leave a reviewable sequence in the Project panel plus a range
//! marker. The score ranks text and timing, not likely views.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use filmcraft_project::{BinEntry, BinId, Caption, ClipId, ItemId, ItemKind, Label, Marker, MarkerId, MarkerKind, Project, Sequence, TrackItem};
use filmcraft_time::{FrameRate, Tick, TimeRange};
use serde_json::{Value, json};

use crate::{EngineError, Result, Session};

/// Candidate as refined for review: sequence-time bounds and the original word range.
#[derive(Clone, Debug)]
struct Candidate {
    start_word: usize,
    end_word: usize,
    start: Tick,
    end: Tick,
    score: u32,
    title: String,
    hook: Option<String>,
    reason: Option<String>,
    source: &'static str,
    hook_verified: bool,
}

const BIN: &str = "AI Shorts";
const MARKER_COLOR: Label = Label::Rose;

/// Bounds and count shared by both rankers.
#[derive(Clone, Copy)]
struct RankOptions {
    min: f64,
    max: f64,
    count: usize,
}

fn bad(e: impl ToString) -> EngineError {
    EngineError::BadParams { cmd: "sequence.findShorts".into(), msg: e.to_string() }
}

fn has_transcript(s: &Session) -> std::result::Result<(), String> {
    if s.state.active_sequence.is_none() {
        return Err("open a sequence first".into());
    }
    if crate::transcript::sequence_words(s).len() < 8 {
        return Err("transcribe the sequence first: Find Shorts needs word timings".into());
    }
    Ok(())
}

/// `sequence.findShorts {minSeconds?, maxSeconds?, count?}`.
fn find(s: &mut Session, p: &Value) -> Result<Value> {
    has_transcript(s).map_err(bad)?;
    let source_id = s.state.active_sequence.ok_or(EngineError::NoSequence)?;
    let source = s.project.sequence(source_id).cloned().ok_or(EngineError::NoSequence)?;
    let words = crate::transcript::sequence_words(s);
    if words.len() < 8 {
        return Err(bad("the sequence transcript has too few words"));
    }
    let min = p.get("minSeconds").and_then(Value::as_f64).unwrap_or(12.0);
    let max = p.get("maxSeconds").and_then(Value::as_f64).unwrap_or(60.0);
    let count = p.get("count").and_then(Value::as_u64).unwrap_or(8).clamp(1, 20) as usize;
    if !min.is_finite() || !max.is_finite() || min < 2.0 || max < min || max > 180.0 {
        return Err(bad("minSeconds/maxSeconds must be finite, 2–180 s, with min ≤ max"));
    }
    let candidates = candidates(&words, source.settings.frame_rate, Tick::from_seconds_f64(min), Tick::from_seconds_f64(max), count, source.duration());
    if candidates.is_empty() {
        return Err(bad("no candidate long enough: lower minSeconds or check the transcript"));
    }
    let created = apply(s, source_id, &candidates)?;
    s.toast(format!("Created {} short(s) in {BIN}; markers are on the source sequence", created.len()));
    Ok(json!({
        "created": created,
        "bin": BIN,
        "method": "local-transcript-heuristic",
        "warnings": ["The score ranks text and timing, not likely views; review every cut."],
    }))
}

fn apply(s: &mut Session, source_id: ItemId, candidates: &[Candidate]) -> Result<Vec<Value>> {
    let source = s.project.sequence(source_id).cloned().ok_or(EngineError::NoSequence)?;
    let candidates = candidates.to_vec();
    s.edit("Find Shorts", move |project, _| apply_in_project(project, source_id, &source, &candidates))
}

fn apply_in_project(project: &mut Project, source_id: ItemId, source: &Sequence, candidates: &[Candidate]) -> Result<Vec<Value>> {
    let bin = ensure_bin(project);
    let existing: HashSet<String> = project
        .root
        .find_bin(bin)
        .map(|b| b.children.iter().filter_map(|c| if let BinEntry::Item(id) = c { project.item(*id).map(|i| i.name.clone()) } else { None }).collect())
        .unwrap_or_default();
    let mut created = Vec::with_capacity(candidates.len());
    for (i, c) in candidates.iter().enumerate() {
        let name = format!("Short {:02} - {}", i + 1, c.title);
        if existing.contains(&name) {
            continue;
        }
        let id = derive(project, source, TimeRange::new(c.start, c.end - c.start), name.clone(), bin);
        let mut comment = match c.source {
            "ollama" => c.reason.clone().unwrap_or_else(|| "Ollama selection; review the hook and boundaries.".into()),
            _ => "Local transcript heuristic; review the boundaries in the short.".into(),
        };
        if c.source == "ollama" && !c.hook_verified {
            comment.push_str(" The quoted hook was not found verbatim in the transcript: check the cut.");
        }
        let marker = Marker {
            url: String::new(),
            author: String::new(),
            custom_color: None,
            id: MarkerId(project.alloc_id()),
            start: c.start,
            duration: c.end - c.start,
            name: format!("{:02} - {} ({})", i + 1, c.title, c.score),
            comment,
            kind: MarkerKind::Comment,
            color: MARKER_COLOR,
        };
        if let Some(seq) = project.sequence_mut(source_id) {
            seq.markers.push(marker);
        }
        created.push(json!({
            "sequence": id.0,
            "name": name,
            "score": c.score,
            "start": c.start.0,
            "end": c.end.0,
            "startWord": c.start_word,
            "endWord": c.end_word,
            "source": c.source,
            "hook": c.hook,
            "reason": c.reason,
            "hookVerified": c.hook_verified,
        }));
    }
    if let Some(seq) = project.sequence_mut(source_id) {
        seq.markers.sort_by_key(|m| m.start);
    }
    if created.is_empty() {
        return Err(bad("those shorts already exist in AI Shorts"));
    }
    Ok(created)
}

/// A background Ollama ranking whose candidates are applied by [`poll`] when it finishes.
pub struct PendingShorts {
    job: u64,
    seq: ItemId,
    candidates: Arc<Mutex<Option<Vec<Candidate>>>>,
    epoch: u64,
}

fn find_ai(s: &mut Session, p: &Value) -> Result<Value> {
    has_transcript(s).map_err(bad_ai)?;
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (s, p);
        return Err(EngineError::Other("Find Shorts with AI is only available in the desktop build".into()));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        find_ai_native(s, p)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn find_ai_native(s: &mut Session, p: &Value) -> Result<Value> {
    let source_id = s.state.active_sequence.ok_or(EngineError::NoSequence)?;
    let source = s.project.sequence(source_id).cloned().ok_or(EngineError::NoSequence)?;
    let words = crate::transcript::sequence_words(s);
    if words.len() < 8 {
        return Err(bad_ai("the sequence transcript has too few words"));
    }
    let min = p.get("minSeconds").and_then(Value::as_f64).unwrap_or(12.0);
    let max = p.get("maxSeconds").and_then(Value::as_f64).unwrap_or(60.0);
    let count = p.get("count").and_then(Value::as_u64).unwrap_or(5).clamp(1, 20) as usize;
    if !min.is_finite() || !max.is_finite() || min < 2.0 || max < min || max > 180.0 {
        return Err(bad_ai("minSeconds/maxSeconds must be finite, 2–180 s, with min ≤ max"));
    }
    let url = p.get("url").and_then(Value::as_str).map(str::trim).filter(|u| !u.is_empty()).map(str::to_string).unwrap_or_else(|| s.prefs.nl.ollama_url());
    let model =
        p.get("model").and_then(Value::as_str).map(str::trim).filter(|m| !m.is_empty()).map(str::to_string).unwrap_or_else(|| s.prefs.nl.ollama_model());
    let id = s.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let job = crate::Job { id, label: format!("Find Shorts AI ({model})"), progress: Default::default(), result: Default::default() };
    let candidates: Arc<Mutex<Option<Vec<Candidate>>>> = Arc::default();
    let (prog, res, out) = (job.progress.clone(), job.result.clone(), candidates.clone());
    let total = windows(words.len()).len();
    prog.total.store(total as u64, std::sync::atomic::Ordering::Relaxed);
    let words_for_run = words.clone();
    let (url_run, model_run) = (url.clone(), model.clone());
    let run = move || {
        use std::sync::atomic::Ordering;
        let t0 = web_time::Instant::now();
        let progress = {
            let prog = prog.clone();
            move |done: usize, total: usize| {
                prog.total.store(total as u64, Ordering::Relaxed);
                prog.done.store(done as u64, Ordering::Relaxed);
                *prog.status.lock().unwrap_or_else(|e| e.into_inner()) = format!("Find Shorts AI: window {done}/{total}");
            }
        };
        let options = RankOptions { min, max, count };
        let result = rank_with_ollama(&url_run, &model_run, &words_for_run, source.settings.frame_rate, options, &progress);
        let secs = t0.elapsed().as_secs_f64();
        match result {
            Ok(found) => {
                *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(found);
                *res.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(Ok(filmcraft_export::Report { path: String::new(), frames: 0, seconds: secs, ..Default::default() }));
            }
            Err(e) => *res.lock().unwrap_or_else(|e| e.into_inner()) = Some(Err(e)),
        }
        prog.finished.store(true, Ordering::Relaxed);
    };
    s.jobs.push(job);
    s.shorts_jobs.push(PendingShorts { job: id, seq: source_id, candidates, epoch: s.document_epoch });
    let wait = p.get("wait").and_then(Value::as_bool).unwrap_or(false);
    if wait {
        run();
        poll(s);
        if let Some(Err(e)) = s.jobs.iter().find(|j| j.id == id).and_then(|j| j.result.lock().ok().and_then(|r| r.clone())) {
            return Err(EngineError::Other(e));
        }
    } else {
        std::thread::Builder::new()
            .name("filmcraft-find-shorts-ai".into())
            .spawn(run)
            .map_err(|e| EngineError::Other(format!("can't start the AI ranking thread: {e}")))?;
    }
    Ok(json!({"job": id, "model": model, "url": url, "source": source_id.0}))
}

/// Apply finished AI jobs (one undo step each) and drop cancelled ones.
pub(crate) fn poll(s: &mut Session) {
    use std::sync::atomic::Ordering;
    let mut i = 0;
    while i < s.shorts_jobs.len() {
        let job = s.jobs.iter().find(|j| j.id == s.shorts_jobs[i].job);
        let finished = job.is_none_or(|j| j.progress.finished.load(Ordering::Relaxed));
        let cancelled = job.is_some_and(|j| j.progress.cancel.load(Ordering::Relaxed));
        if !finished {
            i += 1;
            continue;
        }
        let pending = s.shorts_jobs.remove(i);
        let result = job.and_then(|j| j.result.lock().ok().and_then(|r| r.clone()));
        if cancelled {
            continue;
        }
        if let Some(Err(e)) = result {
            s.error_toast("sequence.findShortsAi", format!("Find Shorts AI: {e}"));
            continue;
        }
        let Some(candidates) = pending.candidates.lock().unwrap_or_else(|e| e.into_inner()).take() else { continue };
        if pending.epoch != s.document_epoch {
            s.error_toast("sequence.findShortsAi", "the project was replaced; the AI shorts were not applied");
            continue;
        }
        match apply(s, pending.seq, &candidates) {
            Ok(created) => s.toast(format!("AI created {} short(s) in {BIN}; check each quote and boundary", created.len())),
            Err(e) => s.error_toast("sequence.findShortsAi", e.to_string()),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn windows(len: usize) -> Vec<(usize, usize)> {
    const MAX: usize = 900;
    const OVERLAP: usize = 80;
    if len <= MAX {
        return if len == 0 { Vec::new() } else { vec![(0, len - 1)] };
    }
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let end = (start + MAX - 1).min(len - 1);
        out.push((start, end));
        if end + 1 >= len {
            break;
        }
        start = (end + 1).saturating_sub(OVERLAP);
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn rank_with_ollama(
    url: &str,
    model: &str,
    words: &[filmcraft_edit::transcript::SeqWord],
    rate: FrameRate,
    options: RankOptions,
    progress: &dyn Fn(usize, usize),
) -> std::result::Result<Vec<Candidate>, String> {
    let RankOptions { min, max, count } = options;
    let ranges = windows(words.len());
    let total = ranges.len();
    let mut raw = Vec::new();
    let mut last_error = None;
    for (i, (a, b)) in ranges.iter().copied().enumerate() {
        let prompt = prompt(words, a, b, min, max, count);
        match crate::ollama::chat(url, model, SYSTEM, &prompt, Duration::from_secs(300)) {
            Ok(text) => match parse_clips(&text, words, a, b, min, max, count) {
                Ok(mut found) => raw.append(&mut found),
                Err(e) => last_error = Some(e),
            },
            Err(e) => last_error = Some(e),
        }
        progress(i + 1, total);
    }
    if raw.is_empty() {
        return Err(last_error.unwrap_or_else(|| "Ollama returned no usable clips".into()));
    }
    raw.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.start.cmp(&b.start)));
    let mut kept: Vec<Candidate> = Vec::new();
    for c in raw {
        if kept.iter().all(|k| overlap(k, &c) < 0.55) {
            kept.push(c);
        }
        if kept.len() >= count {
            break;
        }
    }
    for c in &mut kept {
        let (start, end) = refine(words, c.start_word, c.end_word, rate, words.last().map(|w| w.end).unwrap_or(Tick::ZERO));
        c.start = start;
        c.end = end;
    }
    kept.retain(|c| c.end > c.start);
    Ok(kept)
}

#[cfg(not(target_arch = "wasm32"))]
const SYSTEM: &str = "You are a short-form video editor. You find self-contained moments in a transcript and return only the requested JSON. Never invent a quote: the hook must be copied verbatim from the transcript.";

#[cfg(not(target_arch = "wasm32"))]
fn prompt(words: &[filmcraft_edit::transcript::SeqWord], a: usize, b: usize, min: f64, max: f64, count: usize) -> String {
    let mut transcript = String::new();
    for (i, w) in words.iter().enumerate().take(b + 1).skip(a) {
        transcript.push_str(&format!("[{i}]{} ({:.2}-{:.2})\n", w.text, w.start.seconds(), w.end.seconds()));
    }
    format!(
        "Choose at most {count} short clips from this transcript section (global word indices {a}..{b}).\n\
Rules:\n\
- start_word_index and end_word_index are the bracketed global indices, exactly as printed.\n\
- Start on the first word of a complete thought and end on the last word of a complete thought.\n\
- Each clip lasts between {min:.0} and {max:.0} seconds.\n\
- The hook is the exact opening words of the clip, copied verbatim from the transcript.\n\
- Score 0-100. Do not return a clip below 60. Prefer a strong opening and a real payoff.\n\
- Return at most {count} clips. If nothing is worth cutting, return {{\"clips\":[]}}.\n\
- Reply with JSON only, no markdown: \
{{\"clips\":[{{\"start_word_index\":0,\"end_word_index\":0,\"title\":\"...\",\"hook\":\"...\",\"score\":0,\"reason\":\"...\"}}]}}\n\
Transcript:\n{transcript}"
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_clips(
    text: &str,
    words: &[filmcraft_edit::transcript::SeqWord],
    window_start: usize,
    window_end: usize,
    min: f64,
    max: f64,
    count: usize,
) -> std::result::Result<Vec<Candidate>, String> {
    let start = text.find('{').ok_or_else(|| {
        let preview: String = text.trim().chars().take(160).collect();
        if preview.is_empty() {
            "Ollama returned an empty reply (the model may have run out of context)".to_string()
        } else {
            format!("Ollama did not return a JSON object; reply starts with: {preview:?}")
        }
    })?;
    let end = text.rfind('}').ok_or_else(|| "Ollama returned incomplete JSON".to_string())?;
    let value: Value = serde_json::from_str(&text[start..=end]).map_err(|e| format!("Ollama returned invalid JSON: {e}"))?;
    let clips = value.get("clips").and_then(Value::as_array).ok_or_else(|| "Ollama JSON has no clips array".to_string())?;
    let mut out = Vec::new();
    for clip in clips.iter().take(count.saturating_mul(2).max(1)) {
        let Some(a) = clip.get("start_word_index").and_then(Value::as_u64).map(|v| v as usize) else { continue };
        let Some(b) = clip.get("end_word_index").and_then(Value::as_u64).map(|v| v as usize) else { continue };
        if a < window_start || b > window_end || a >= b || b >= words.len() {
            continue;
        }
        let start = words[a].start;
        let end = words[b].end;
        let duration = (end - start).seconds();
        if duration < min * 0.8 || duration > max * 1.4 {
            continue;
        }
        let text = words[a..=b].iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
        let hook = clip.get("hook").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        let hook_verified = hook.as_deref().is_some_and(|h| {
            let h = normalized(h);
            !h.is_empty() && normalized(&text).contains(&h)
        });
        let reason = clip.get("reason").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        let title =
            clip.get("title").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).unwrap_or_else(|| title(words, a, b));
        let score = clip.get("score").and_then(Value::as_u64).unwrap_or(60).min(100) as u32;
        out.push(Candidate { start_word: a, end_word: b, start, end, score, title, hook, reason, source: "ollama", hook_verified });
    }
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn normalized(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn bad_ai(e: impl ToString) -> EngineError {
    EngineError::BadParams { cmd: "sequence.findShortsAi".into(), msg: e.to_string() }
}

fn ensure_bin(project: &mut Project) -> BinId {
    for child in &project.root.children {
        if let BinEntry::Bin(b) = child
            && b.name == BIN
        {
            return b.id;
        }
    }
    project.add_bin(BIN, None)
}

fn candidates(words: &[filmcraft_edit::transcript::SeqWord], rate: FrameRate, min: Tick, max: Tick, count: usize, sequence_end: Tick) -> Vec<Candidate> {
    let paras = filmcraft_edit::transcript::paragraphs(words, Tick::from_seconds_f64(1.2));
    let mut raw = Vec::new();
    for (p, para) in paras.iter().enumerate() {
        let start = words[para.start].start;
        let mut q = p;
        loop {
            let end = words[paras[q].end - 1].end;
            let dur = end - start;
            if dur > max && q > p {
                break;
            }
            if dur >= min {
                if dur <= max + rate.frame_duration() {
                    let (a, b) = (para.start, paras[q].end - 1);
                    raw.push(Candidate {
                        start_word: a,
                        end_word: b,
                        start,
                        end,
                        score: score(words, a, b),
                        title: title(words, a, b),
                        hook: None,
                        reason: None,
                        source: "local",
                        hook_verified: false,
                    });
                }
                break;
            }
            if q + 1 >= paras.len() {
                break;
            }
            q += 1;
        }
    }
    raw.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.start.cmp(&b.start)));
    let mut kept: Vec<Candidate> = Vec::new();
    for c in raw {
        if kept.iter().all(|k| overlap(k, &c) < 0.55) {
            kept.push(c);
        }
        if kept.len() >= count {
            break;
        }
    }
    for c in &mut kept {
        let (start, end) = refine(words, c.start_word, c.end_word, rate, sequence_end);
        c.start = start;
        c.end = end;
    }
    kept.retain(|c| c.end > c.start);
    kept
}

fn overlap(a: &Candidate, b: &Candidate) -> f64 {
    let start = a.start.max(b.start);
    let end = a.end.min(b.end);
    let shared = (end - start).max(Tick::ZERO).0 as f64;
    let shorter = (a.end - a.start).min(b.end - b.start).0 as f64;
    if shorter <= 0.0 { 0.0 } else { shared / shorter }
}

fn refine(words: &[filmcraft_edit::transcript::SeqWord], a: usize, b: usize, rate: FrameRate, sequence_end: Tick) -> (Tick, Tick) {
    let first = &words[a];
    let last = &words[b];
    let gap_before = if a == 0 { first.start } else { first.start - words[a - 1].end };
    let lead = if gap_before > Tick::ZERO { (gap_before.0 / 2).min(Tick::from_seconds_f64(0.12).0) } else { 0 };
    let gap_after = if b + 1 < words.len() { words[b + 1].start - last.end } else { Tick::ZERO };
    let tail = if gap_after > Tick::ZERO { (gap_after.0 / 2).min(Tick::from_seconds_f64(0.28).0) } else { 0 };
    let raw_start = Tick((first.start.0 - lead).max(0));
    let raw_end = (last.end + Tick(tail)).min(sequence_end.max(last.end));
    let start = rate.snap(raw_start).max(Tick::ZERO);
    let mut end = rate.snap(raw_end);
    if end < raw_end {
        end += rate.frame_duration();
    }
    if end <= start {
        end = start + rate.frame_duration();
    }
    (start, end)
}

fn score(words: &[filmcraft_edit::transcript::SeqWord], a: usize, b: usize) -> u32 {
    let duration = (words[b].end - words[a].start).seconds().max(0.1);
    let count = (b - a + 1) as f64;
    let rate = count / duration;
    let text = words[a..=b].iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
    let first = words[a].text.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    let filler =
        matches!(first.as_str(), "e" | "ed" | "ma" | "però" | "pero" | "quindi" | "allora" | "cioè" | "cioe" | "like" | "so" | "um" | "uh" | "okay" | "ok");
    let closes = words[b].text.trim_end().ends_with(['.', '!', '?', '…']);
    let duration_score = if (15.0..=45.0).contains(&duration) {
        90
    } else if (8.0..=60.0).contains(&duration) {
        65
    } else {
        40
    };
    let hook_score = if filler {
        25
    } else if text.contains('?') {
        90
    } else {
        70
    };
    let context_score = if filler { 30 } else { 85 };
    let close_score = if closes { 90 } else { 40 };
    let rate_score = if (1.2..=3.6).contains(&rate) { 85 } else { 45 };
    (duration_score + hook_score + context_score + close_score + rate_score) / 5
}

fn title(words: &[filmcraft_edit::transcript::SeqWord], a: usize, b: usize) -> String {
    let text = words[a..=b].iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
    let text = text.trim();
    if text.is_empty() {
        return "Untitled short".into();
    }
    let mut out: String = text.chars().take(72).collect();
    if text.chars().count() > 72 {
        out.push('…');
    }
    out
}

fn derive(project: &mut Project, source: &Sequence, range: TimeRange, name: String, bin: BinId) -> ItemId {
    let mut seq = source.clone();
    seq.markers.clear();
    seq.mark_in = None;
    seq.mark_out = None;
    seq.work_area = None;
    seq.start_timecode = 0;
    seq.split = Default::default();
    seq.multicam = None;
    seq.merged = None;
    let mut links: HashMap<u64, u64> = HashMap::new();
    for track in &mut seq.video_tracks {
        let old = std::mem::take(&mut track.items);
        track.items = copy_items(project, &old, range, &mut links);
        track.transitions.clear();
    }
    for track in &mut seq.audio_tracks {
        let old = std::mem::take(&mut track.items);
        track.items = copy_items(project, &old, range, &mut links);
        track.transitions.clear();
    }
    for track in &mut seq.caption_tracks {
        let old = std::mem::take(&mut track.captions);
        track.captions = copy_captions(project, &old, range);
        track.vtt_blocks.clear();
    }
    project.add_item(&name, MARKER_COLOR, ItemKind::Sequence(Arc::new(seq)), Some(bin))
}

fn copy_items(project: &mut Project, items: &[TrackItem], range: TimeRange, links: &mut HashMap<u64, u64>) -> Vec<TrackItem> {
    let mut out = Vec::new();
    for item in items {
        if item.speed <= 0.0 {
            continue;
        }
        let start = item.start.max(range.start);
        let end = item.end().min(range.end());
        if end <= start {
            continue;
        }
        let mut copy = item.clone();
        copy.id = ClipId(project.alloc_id());
        copy.start = start - range.start;
        copy.duration = end - start;
        let offset = start - item.start;
        copy.source_in = item.source_in + Tick((offset.0 as f64 * item.speed).round() as i64);
        copy.link = match item.link {
            Some(old) => {
                let new = *links.entry(old).or_insert_with(|| project.alloc_id());
                Some(new)
            }
            None => None,
        };
        copy.markers.clear();
        out.push(copy);
    }
    out
}

fn copy_captions(project: &mut Project, captions: &[Caption], range: TimeRange) -> Vec<Caption> {
    let mut out = Vec::new();
    for caption in captions {
        let start = caption.start.max(range.start);
        let end = caption.end().min(range.end());
        if end <= start {
            continue;
        }
        let mut copy = caption.clone();
        copy.id = ClipId(project.alloc_id());
        copy.start = start - range.start;
        copy.duration = end - start;
        out.push(copy);
    }
    out
}

pub(crate) fn commands() -> Vec<crate::CommandSpec> {
    vec![
        spec("sequence.findShorts", "Find Shorts", &["Sequence"], r#"{"minSeconds":f?,"maxSeconds":f?,"count":n?}"#, has_transcript, find, true),
        spec(
            "sequence.findShortsAi",
            "Find Shorts with AI",
            &["Sequence"],
            r#"{"url":str?,"model":str?,"minSeconds":f?,"maxSeconds":f?,"count":n?,"wait":bool?}"#,
            has_transcript,
            find_ai,
            true,
        ),
    ]
}

fn spec(
    id: &'static str,
    label: &'static str,
    menu: &'static [&'static str],
    params: &'static str,
    enabled: fn(&Session) -> std::result::Result<(), String>,
    run: fn(&mut Session, &Value) -> Result<Value>,
    undoable: bool,
) -> crate::CommandSpec {
    crate::CommandSpec { id, label, menu, shortcut: None, params, enabled, run, journal: undoable, named: Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_with_transcript() -> Session {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        let clip = s.active_sequence().unwrap().audio_tracks[0].items[0].clone();
        let span = clip.duration.0;
        let words: Vec<Value> = (0..30)
            .map(|i| {
                let start = clip.source_in.0 + span * i / 30;
                let end = clip.source_in.0 + span * (i + 1) / 30;
                let text = if i == 4 {
                    "domanda?".to_string()
                } else if i == 29 {
                    "fine.".to_string()
                } else {
                    format!("parola{i}")
                };
                json!({"text": text, "start": start, "end": end})
            })
            .collect();
        s.execute("transcript.set", json!({"item": clip.item.0, "transcript": {"language": "it", "words": words}})).unwrap();
        s
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn ai_job_applies_candidates_from_a_mock_ollama() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let _ = socket.read(&mut buf).unwrap();
            let content =
                r#"{"clips":[{"start_word_index":0,"end_word_index":9,"title":"Mock short","hook":"parola0","score":88,"reason":"A complete mock moment."}]}"#;
            let body = json!({"message": {"content": content}}).to_string();
            let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            socket.write_all(head.as_bytes()).unwrap();
            socket.write_all(body.as_bytes()).unwrap();
        });
        let mut s = demo_with_transcript();
        let url = format!("http://127.0.0.1:{port}");
        let r = s.execute("sequence.findShortsAi", json!({"url": url, "model": "mock", "minSeconds": 2, "maxSeconds": 8, "count": 2, "wait": true})).unwrap();
        server.join().unwrap();
        assert!(r["job"].is_u64(), "{r}");
        let ai = s
            .project
            .root
            .children
            .iter()
            .find_map(|c| {
                if let BinEntry::Bin(b) = c
                    && b.name == BIN
                {
                    Some(b)
                } else {
                    None
                }
            })
            .expect("AI Shorts bin");
        assert_eq!(ai.children.len(), 1);
        let source = s.state.active_sequence.unwrap();
        assert!(s.project.sequence(source).unwrap().markers.iter().any(|m| m.name.contains("Mock short")));
        let item = match ai.children[0] {
            BinEntry::Item(id) => id,
            _ => panic!("sequence item"),
        };
        assert!(s.project.sequence(item).is_some());
    }

    #[test]
    fn finds_refines_and_derives_reviewable_sequences() {
        let mut s = demo_with_transcript();
        let source = s.state.active_sequence.unwrap();
        let before = s.project.sequence(source).unwrap().markers.len();
        let r = s.execute("sequence.findShorts", json!({"minSeconds": 2, "maxSeconds": 8, "count": 2})).unwrap();
        let created = r["created"].as_array().unwrap();
        assert!(!created.is_empty(), "{r}");
        assert!(s.project.root.children.iter().any(|c| matches!(c, BinEntry::Bin(b) if b.name == BIN)));
        assert!(s.project.sequence(source).unwrap().markers.len() > before);
        for c in created {
            let id = ItemId(c["sequence"].as_u64().unwrap());
            let seq = s.project.sequence(id).expect("derived sequence");
            assert!(seq.duration() > Tick::ZERO);
            assert!(seq.video_tracks.iter().any(|t| !t.items.is_empty()) || seq.audio_tracks.iter().any(|t| !t.items.is_empty()));
        }
        assert!(s.execute("sequence.findShorts", json!({"minSeconds": 2, "maxSeconds": 8, "count": 2})).is_err(), "a second run is a no-op");
        s.undo();
        assert!(!s.project.root.children.iter().any(|c| matches!(c, BinEntry::Bin(b) if b.name == BIN)));
        assert_eq!(s.project.sequence(source).unwrap().markers.len(), before);
    }
}
