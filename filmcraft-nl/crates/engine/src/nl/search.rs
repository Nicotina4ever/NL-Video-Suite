//! Project search: one index over what the project already knows (clips, markers, captions, the
//! story and the transcripts), queried locally, with the snippet and time of every hit.
//!
//! The classic suite kept a SQLite FTS index; this build keeps the same behaviour without a second
//! database: the index is built on request, lives in memory while the project is open and is
//! rebuilt when the operator asks. Results are plain JSON, so the panel and the CLI show the same.

use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{EngineError, Result, Session};

const MAX_DOCS: usize = 20_000;
const MAX_SNIPPET: usize = 240;
const MAX_QUERY: usize = 200;
const TRANSCRIPT_WINDOW_CHARS: usize = 180;

fn bad_cmd(cmd: &str, msg: impl ToString) -> EngineError {
    bad(cmd, msg.to_string())
}

fn doc(source: &str, title: &str, text: &str, item: Option<u64>, time_sec: Option<f64>) -> Value {
    let text = text.trim().chars().take(4000).collect::<String>();
    json!({
        "source": source,
        "title": title.chars().take(160).collect::<String>(),
        "text": text,
        "item": item,
        "timeSec": time_sec,
    })
}

/// Every searchable document of this project.
pub fn build(s: &Session) -> Vec<Value> {
    let mut documents: Vec<Value> = Vec::new();
    if let Some(id) = s.state.active_sequence
        && let Some(sequence) = s.project.sequence(id)
    {
        for track in sequence.video_tracks.iter().chain(sequence.audio_tracks.iter()) {
            for clip in &track.items {
                if !clip.enabled {
                    continue;
                }
                documents.push(doc("clip", &clip.name, &format!("{} {}", clip.name, track.name), Some(clip.item.0), Some(clip.start.seconds())));
            }
        }
        for marker in &sequence.markers {
            let text = format!("{} {} {}", marker.name, marker.comment, marker.url);
            documents.push(doc("marker", &marker.name, &text, None, Some(marker.start.seconds())));
        }
        for track in &sequence.caption_tracks {
            for caption in &track.captions {
                documents.push(doc("caption", &track.name, &caption.text, None, Some(caption.start.seconds())));
            }
        }
    }
    // The story: title, synopsis, characters, locations and every scene's text and prompts.
    let story = s.nl.story.clone().or_else(|| {
        let path = s.path.as_deref().map(std::path::Path::new)?.parent()?.join("nl").join("story.json");
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str::<Value>(&text).ok()
    });
    if let Some(story) = story {
        let title = story["title"].as_str().unwrap_or("Story");
        documents.push(doc(
            "story",
            title,
            &format!("{} {} {} {}", title, story["synopsis"].as_str().unwrap_or_default(), story["style"].as_str().unwrap_or_default(), story["characters"]),
            None,
            None,
        ));
        for scene in story["scenes"].as_array().into_iter().flatten() {
            let heading = scene["heading"].as_str().unwrap_or("Scene");
            let text = format!(
                "{} {} {} {} {}",
                heading,
                scene["action"].as_str().unwrap_or_default(),
                scene["dialogue"].as_str().unwrap_or_default(),
                scene["imagePrompt"].as_str().unwrap_or_default(),
                scene["location"].as_str().unwrap_or_default()
            );
            let time = scene["startSec"].as_f64();
            documents.push(doc("scene", heading, &text, scene["image"]["item"].as_u64(), time));
        }
    }
    // Transcripts: windows of speech, with their media time.
    for (item, transcript) in &s.project.transcripts {
        let mut window = String::new();
        let mut window_start: Option<f64> = None;
        for word in &transcript.words {
            if window_start.is_none() {
                window_start = Some(word.start.seconds());
            }
            if !window.is_empty() {
                window.push(' ');
            }
            window.push_str(&word.text);
            if window.chars().count() >= TRANSCRIPT_WINDOW_CHARS {
                let title = format!("Transcription · item {}", item.0);
                documents.push(doc("transcript", &title, &window, Some(item.0), window_start.take()));
                window.clear();
            }
        }
        if !window.trim().is_empty() {
            let title = format!("Transcription · item {}", item.0);
            documents.push(doc("transcript", &title, &window, Some(item.0), window_start));
        }
    }
    documents.truncate(MAX_DOCS);
    documents
}

fn tokens(query: &str) -> Vec<String> {
    query.to_lowercase().split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-').filter(|token| token.chars().count() > 1).map(str::to_string).collect()
}

/// Search `documents` with `query`, best first. An empty query is refused: a search that matches
/// everything is not a search.
pub fn query(documents: &[Value], query: &str, limit: usize) -> std::result::Result<Value, String> {
    let query = query.trim();
    if query.is_empty() || query.chars().count() > MAX_QUERY {
        return Err(format!("the query must be 1 to {MAX_QUERY} characters"));
    }
    let tokens = tokens(query);
    if tokens.is_empty() {
        return Err("the query has no searchable words".into());
    }
    let mut scored: Vec<(u32, Value)> = Vec::new();
    for document in documents {
        let text = document["text"].as_str().unwrap_or_default().to_lowercase();
        let title = document["title"].as_str().unwrap_or_default().to_lowercase();
        let mut score = 0u32;
        for token in &tokens {
            if text.contains(token.as_str()) {
                score += 1;
            }
            if title.contains(token.as_str()) {
                score += 1;
            }
        }
        if score == 0 {
            continue;
        }
        let snippet = snippet(&document["text"], &tokens);
        scored.push((
            score,
            json!({
                "source": document["source"],
                "title": document["title"],
                "item": document["item"],
                "timeSec": document["timeSec"],
                "score": score,
                "snippet": snippet,
            }),
        ));
    }
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    let total = scored.len();
    let results: Vec<Value> = scored.into_iter().take(limit).map(|(_, result)| result).collect();
    Ok(json!({"query": query, "documents": documents.len(), "total": total, "results": results}))
}

/// A window of the document around the first matched token.
fn snippet(text: &Value, tokens: &[String]) -> String {
    let text = text.as_str().unwrap_or_default();
    let lower = text.to_lowercase();
    let Some(position) = tokens.iter().filter_map(|token| lower.find(token.as_str())).min() else {
        return text.chars().take(MAX_SNIPPET).collect();
    };
    let start = text[..position].char_indices().rev().nth(40).map(|(index, _)| index).unwrap_or(0);
    text[start..].chars().take(MAX_SNIPPET).collect::<String>().trim().to_string()
}

/// `nl.search.index` — (re)build the index and report what went in.
fn index(s: &mut Session, _: &Value) -> Result<Value> {
    let documents = build(s);
    let mut counts = serde_json::Map::new();
    for document in &documents {
        if let Some(source) = document["source"].as_str() {
            let count = counts.get(source).and_then(Value::as_u64).unwrap_or(0) + 1;
            counts.insert(source.to_string(), json!(count));
        }
    }
    s.nl.index = Some(documents);
    Ok(json!({"documents": s.nl.index.as_ref().map_or(0, Vec::len), "sources": counts}))
}

/// `nl.search.query` — search the project; builds the index on first use.
fn search(s: &mut Session, p: &Value) -> Result<Value> {
    let text = p.get("query").and_then(Value::as_str).ok_or_else(|| bad_cmd("nl.search.query", "need `query`"))?;
    let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(20).clamp(1, 100) as usize;
    if s.nl.index.is_none() {
        s.nl.index = Some(build(s));
    }
    let documents = s.nl.index.clone().unwrap_or_default();
    query(&documents, text, limit).map_err(|e| bad_cmd("nl.search.query", e))
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    let spec = |id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>| -> CommandSpec {
        CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: always, run, journal: false, named: crate::commands::NamedKeys::NONE }
    };
    vec![spec("nl.search.index", "NL Build Search Index", "{}", index), spec("nl.search.query", "NL Search", r#"{"query":str,"limit":int?}"#, search)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn documents_come_from_clips_markers_and_story() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        s.nl.story = Some(
            json!({"title": "Prova", "synopsis": "Un doppiaggio", "characters": [], "scenes": [{"heading": "INT Studio", "action": "Ada registra", "dialogue": "Ci siamo"}]}),
        );
        let documents = build(&s);
        assert!(documents.iter().any(|document| document["source"] == "clip"));
        assert!(documents.iter().any(|document| document["source"] == "marker" && document["title"] == "Open"));
        assert!(documents.iter().any(|document| document["source"] == "scene" && document["title"] == "INT Studio"));
        assert!(documents.iter().any(|document| document["source"] == "story"));
    }

    #[test]
    fn a_query_ranks_the_documents_that_contain_its_words() {
        let documents = vec![
            doc("clip", "Studio", "una scena in studio con microfono", Some(5), Some(1.0)),
            doc("marker", "Respirazione", "esercizio di respirazione", None, Some(3.0)),
            doc("story", "Storia", "un doppiatore prepara la voce", None, None),
        ];
        let found = query(&documents, "respirazione", 10).unwrap();
        assert_eq!(found["total"], 1);
        assert_eq!(found["results"][0]["title"], "Respirazione");
        assert_eq!(found["results"][0]["timeSec"], 3.0);
        let both = query(&documents, "studio microfono", 10).unwrap();
        assert_eq!(both["results"][0]["source"], "clip");
        assert!(query(&documents, "  ", 10).is_err());
        assert!(query(&documents, "!!!", 10).is_err());
    }

    #[test]
    fn the_index_command_counts_sources() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        let report = s.execute("nl.search.index", json!({})).unwrap();
        assert!(report["documents"].as_u64().unwrap() > 0);
        assert!(report["sources"]["marker"].as_u64().unwrap_or(0) >= 4);
    }
}
