//! Subtitle translation with the local model: a caption track goes in, a translated caption track
//! comes out on the sequence, one request per batch of cues.
//!
//! The model only ever sees the cue ids and their text; each answer is validated (same ids, no empty
//! text) before anything is added, so a partial or shuffled answer creates nothing.

use std::time::Duration;

use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{EngineError, Result, Session};

const MAX_CUES_PER_BATCH: usize = 40;
const MAX_TEXT: usize = 1000;

fn bad_cmd(cmd: &str, msg: impl ToString) -> EngineError {
    bad(cmd, msg.to_string())
}

/// A readable English label for the target tag, used inside the prompt.
fn language_name(tag: &str) -> &str {
    match tag.split(['-', '_']).next().unwrap_or(tag) {
        "en" => "English",
        "es" => "Spanish",
        "it" => "Italian",
        "fr" => "French",
        "de" => "German",
        "pt" => "Portuguese",
        "ja" => "Japanese",
        other => other,
    }
}

fn validate_target(to: &str) -> Result<String> {
    let to = to.trim().to_ascii_lowercase();
    if to.is_empty() || to.chars().count() > 10 || !to.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return Err(bad_cmd("nl.translate", "`to` must be a language tag like en or es"));
    }
    Ok(to)
}

/// The cue texts in batches, keeping each batch under the request size.
pub fn batches(cues: &[(u64, String)]) -> Vec<Vec<(u64, String)>> {
    cues.chunks(MAX_CUES_PER_BATCH).map(|chunk| chunk.to_vec()).collect()
}

/// Validate one model answer: the same ids, in any order, with usable text.
fn parse_batch(value: &Value, ids: &[u64]) -> std::result::Result<Vec<(u64, String)>, String> {
    let Some(list) = value.get("translations").and_then(Value::as_array) else {
        return Err("the model answered without translations".into());
    };
    let mut translated: Vec<(u64, String)> = Vec::new();
    for entry in list {
        let Some(id) = entry.get("id").and_then(Value::as_u64) else { continue };
        let text = entry.get("text").and_then(Value::as_str).unwrap_or_default().trim();
        if !ids.contains(&id) || text.is_empty() || text.chars().count() > MAX_TEXT {
            continue;
        }
        if translated.iter().any(|(existing, _)| *existing == id) {
            continue;
        }
        translated.push((id, text.to_string()));
    }
    if translated.len() != ids.len() {
        return Err(format!("the model translated {} of {} cues", translated.len(), ids.len()));
    }
    Ok(translated)
}

/// `nl.translate.captions` — translate a caption track and add the translation as a new track.
fn translate_captions(s: &mut Session, p: &Value) -> Result<Value> {
    let to = validate_target(p.get("to").and_then(Value::as_str).ok_or_else(|| bad_cmd("nl.translate.captions", "need `to` (en, es, …)"))?)?;
    let source_track = p.get("track").and_then(Value::as_u64);
    let sequence_id = s.state.active_sequence.ok_or(EngineError::NoSequence)?;
    let sequence = s.project.sequence(sequence_id).ok_or(EngineError::NoSequence)?;
    let track = match source_track {
        Some(id) => sequence.caption_tracks.iter().find(|track| track.id.0 == id),
        None => sequence.caption_tracks.first(),
    }
    .ok_or_else(|| bad_cmd("nl.translate.captions", "the sequence has no caption track to translate"))?;
    let track_name = track.name.clone();
    let cues: Vec<(u64, String, i64, i64)> =
        track.captions.iter().map(|caption| (caption.id.0, caption.text.clone(), caption.start.0, caption.duration.0)).collect();
    if cues.is_empty() {
        return Err(bad_cmd("nl.translate.captions", "the caption track has no cues to translate"));
    }
    let llm = super::llm::LlmConfig::for_role(s, "prompt", p.get("model").and_then(Value::as_str));
    let system = format!(
        "Sei un traduttore di sottotitoli. Traduci i testi in {}. Mantieni tono, significato e brevità da sottotitolo. Rispondi SOLO con un oggetto JSON valido {{\"translations\":[{{\"id\":number,\"text\":string}}]}} con lo stesso id di ogni riga e nessun testo attorno.",
        language_name(&to)
    );
    let simple: Vec<(u64, String)> = cues.iter().map(|(id, text, _, _)| (*id, text.clone())).collect();
    let mut translations = Vec::new();
    for batch in batches(&simple) {
        let ids: Vec<u64> = batch.iter().map(|(id, _)| *id).collect();
        let mut user = String::from("Righe da tradurre:");
        for (id, text) in &batch {
            user.push_str(&format!("\n{id}: {text}"));
        }
        let value = llm.auto_json(&system, &user, Duration::from_secs(300), Some("0")).map_err(|e| bad_cmd("nl.translate.captions", e))?;
        let translated = parse_batch(&value, &ids).map_err(|e| bad_cmd("nl.translate.captions", e))?;
        translations.extend(translated);
    }
    // Create the translated track and add every cue in its original time range.
    let new_track = s.execute("captions.newTrack", json!({"name": format!("{track_name} · {}", to.to_uppercase()), "language": to}))?;
    let track_id = new_track["track"].as_u64().ok_or_else(|| bad_cmd("nl.translate.captions", "the new caption track was not created"))?;
    let mut added = 0;
    for (id, _text, start, duration) in &cues {
        let Some((_, translated)) = translations.iter().find(|(translated_id, _)| translated_id == id) else { continue };
        let seconds = *duration as f64 / filmcraft_time::TICKS_PER_SECOND as f64;
        let added_caption = s.execute("captions.add", json!({"track": track_id, "time": start, "durationSeconds": seconds, "text": translated}));
        if added_caption.is_ok() {
            added += 1;
        }
    }
    if added == 0 {
        return Err(bad_cmd("nl.translate.captions", "no translated cue could be added to the timeline"));
    }
    s.toast(format!("NL translate: {added} cue(s) added to track {}", to.to_uppercase()));
    Ok(json!({"track": track_id, "language": to, "captions": added, "sourceTrack": track_name, "model": llm.model}))
}

/// `nl.translate.text` — translate a short text (story synopsis, scene action, a note).
fn translate_text(s: &mut Session, p: &Value) -> Result<Value> {
    let text =
        p.get("text").and_then(Value::as_str).map(str::trim).filter(|text| !text.is_empty()).ok_or_else(|| bad_cmd("nl.translate.text", "need `text`"))?;
    if text.chars().count() > 4000 {
        return Err(bad_cmd("nl.translate.text", "`text` is too long (4000 characters max)"));
    }
    let to = validate_target(p.get("to").and_then(Value::as_str).ok_or_else(|| bad_cmd("nl.translate.text", "need `to`"))?)?;
    let llm = super::llm::LlmConfig::for_role(s, "prompt", p.get("model").and_then(Value::as_str));
    let system = format!(
        "Sei un traduttore. Traduci in {}. Rispondi SOLO con un oggetto JSON valido {{\"translation\":string}}, senza testo attorno.",
        language_name(&to)
    );
    let value = llm.auto_json(&system, text, Duration::from_secs(180), Some("0")).map_err(|e| bad_cmd("nl.translate.text", e))?;
    let translation = value.get("translation").and_then(Value::as_str).map(str::trim).unwrap_or_default();
    if translation.is_empty() {
        return Err(bad_cmd("nl.translate.text", "the model answered without a translation"));
    }
    Ok(json!({"translation": translation, "language": to, "model": llm.model}))
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    let spec = |id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>| -> CommandSpec {
        CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: always, run, journal: false, named: crate::commands::NamedKeys::NONE }
    };
    vec![
        spec("nl.translate.captions", "NL Translate Subtitles", r#"{"to":str,"track":id?,"model":str?}"#, translate_captions),
        spec("nl.translate.text", "NL Translate Text", r#"{"text":str,"to":str,"model":str?}"#, translate_text),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_are_batched_and_every_id_must_come_back() {
        let cues: Vec<(u64, String)> = (1..=95).map(|id| (id, format!("cue {id}"))).collect();
        let batches = batches(&cues);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].len(), MAX_CUES_PER_BATCH);
        let answer = json!({"translations": [{"id": 1, "text": "one"}, {"id": 2, "text": "two"}]});
        assert!(parse_batch(&answer, &[1, 2]).is_ok());
        assert!(parse_batch(&answer, &[1, 2, 3]).is_err(), "a missing cue refuses the batch");
        assert!(parse_batch(&json!({"translations": [{"id": 1, "text": " "}]}), &[1]).is_err(), "empty text refused");
    }

    #[test]
    fn translation_commands_refuse_bad_parameters_before_any_request() {
        let mut s = Session::default();
        assert!(s.execute("nl.translate.captions", json!({})).is_err());
        assert!(s.execute("nl.translate.text", json!({"text": "x"})).is_err());
        assert!(s.execute("nl.translate.text", json!({"text": "x", "to": "!!"})).is_err());
        s.execute("file.openDemoProject", json!({})).unwrap();
        // the demo sequence has no caption track: refused without contacting Ollama
        let error = s.execute("nl.translate.captions", json!({"to": "en"})).unwrap_err().to_string();
        assert!(error.contains("no caption track") || error.contains("no cues"), "{error}");
        assert_eq!(language_name("en-GB"), "English");
        assert_eq!(language_name("es"), "Spanish");
    }
}
