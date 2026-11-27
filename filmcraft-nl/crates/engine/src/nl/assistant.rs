//! Ask the project: the local model answers a question using only the project's searchable
//! documents (clips, markers, captions, story, transcripts) and cites the ones it used.
//!
//! Retrieval reuses the same index as `nl.search`; the model never sees the whole project, only the
//! best few documents, so an answer can be checked against the sources the panel lists next to it.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{EngineError, Result, Session};

use super::state::PendingPrompt;

const MAX_QUESTION: usize = 500;
const MAX_ANSWER: usize = 4000;
const SOURCES: usize = 6;

fn bad_cmd(cmd: &str, msg: impl ToString) -> EngineError {
    bad(cmd, msg.to_string())
}

/// The prompt: question plus numbered documents, each with its source and time.
fn prompt(question: &str, results: &[Value], language: &str) -> String {
    let mut user = format!(
        "Domanda: {question}\n\nRispondi in {language} usando SOLO i documenti qui sotto e cita la fonte con [n]. Se i documenti non bastano, dillo.\n\nDocumenti:"
    );
    for (index, result) in results.iter().enumerate() {
        let time = result["timeSec"].as_f64().map(|seconds| format!("{seconds:.1}s")).unwrap_or_default();
        user.push_str(&format!(
            "\n[{}] ({}, {}, {}) {}",
            index + 1,
            result["source"].as_str().unwrap_or(""),
            time,
            result["title"].as_str().unwrap_or(""),
            result["snippet"].as_str().unwrap_or("")
        ));
    }
    user
}

/// `nl.assistant.ask` — answer from the project's documents, with the sources listed.
fn ask(s: &mut Session, p: &Value) -> Result<Value> {
    let question = p
        .get("question")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|question| !question.is_empty())
        .ok_or_else(|| bad_cmd("nl.assistant.ask", "need `question`"))?;
    if question.chars().count() > MAX_QUESTION {
        return Err(bad_cmd("nl.assistant.ask", format!("`question` is longer than {MAX_QUESTION} characters")));
    }
    let question = question.to_string();
    if s.nl.index.is_none() {
        s.nl.index = Some(super::search::build(s));
    }
    let documents = s.nl.index.clone().unwrap_or_default();
    let found = super::search::query(&documents, &question, SOURCES).map_err(|e| bad_cmd("nl.assistant.ask", e))?;
    let results: Vec<Value> = found["results"].as_array().cloned().unwrap_or_default();
    if results.is_empty() {
        return Err(bad_cmd("nl.assistant.ask", "no project document matches the question: rephrase it or rebuild the index"));
    }
    let sources: Vec<Value> = results
        .iter()
        .enumerate()
        .map(|(index, result)| {
            json!({
                "n": index + 1,
                "source": result["source"],
                "title": result["title"],
                "timeSec": result["timeSec"],
                "snippet": result["snippet"],
            })
        })
        .collect();
    let language = p.get("language").and_then(Value::as_str).unwrap_or("italiano").to_string();
    let llm = super::llm::LlmConfig::for_role(s, "prompt", p.get("model").and_then(Value::as_str));
    let system = "Sei un assistente che risponde SOLO con un oggetto JSON valido {\"answer\": string}, senza testo attorno. Usa esclusivamente i documenti forniti, cita le fonti con [n] e non inventare nulla: se non bastano, dillo nell'answer.";
    let user = prompt(&question, &results, &language);
    let wait = p.get("wait").and_then(Value::as_bool).unwrap_or(false);
    if wait {
        let value = llm.auto_json(system, &user, Duration::from_secs(300), Some("0")).map_err(|e| bad_cmd("nl.assistant.ask", e))?;
        let result = build_answer(&value, &sources, &llm.model, documents.len())?;
        s.nl.results.insert("assistant", result.clone());
        return Ok(result);
    }
    let job_result: Arc<Mutex<Option<std::result::Result<Value, String>>>> = Arc::default();
    let out = job_result.clone();
    let id = s.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let job = crate::Job { id, label: format!("NL assistant ({})", llm.model), progress: Default::default(), result: Default::default() };
    let progress = job.progress.clone();
    let llm_run = llm.clone();
    let sources_run = sources.clone();
    let documents_len = documents.len();
    s.jobs.push(job);
    s.nl.assistant_pending = Some(PendingPrompt { job: id, result: job_result, epoch: s.document_epoch, key: "assistant" });
    std::thread::Builder::new()
        .name("filmcraft-nl-assistant".into())
        .spawn(move || {
            *progress.status.lock().unwrap_or_else(|e| e.into_inner()) = format!("NL assistant: answering with {}", llm_run.model);
            let value = llm_run
                .auto_json(system, &user, Duration::from_secs(300), Some("0"))
                .and_then(|value| build_answer(&value, &sources_run, &llm_run.model, documents_len).map_err(|error| error.to_string()));
            *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
            progress.finished.store(true, std::sync::atomic::Ordering::Relaxed);
        })
        .map_err(|e| EngineError::Other(format!("can't start the assistant thread: {e}")))?;
    Ok(json!({"job": id, "model": llm.model, "sources": sources.len()}))
}

/// Validate the model answer and pair it with the sources the panel lists.
fn build_answer(value: &Value, sources: &[Value], model: &str, documents: usize) -> Result<Value> {
    let answer = value.get("answer").and_then(Value::as_str).unwrap_or_default().trim();
    if answer.is_empty() || answer.chars().count() > MAX_ANSWER {
        return Err(bad_cmd("nl.assistant.ask", "the model answered without a usable answer"));
    }
    Ok(json!({"answer": answer, "sources": sources, "model": model, "documents": documents}))
}

/// Move a finished answer into the panel result.
pub(crate) fn poll(s: &mut Session) {
    let finished =
        s.nl.assistant_pending
            .as_ref()
            .is_none_or(|pending| s.jobs.iter().find(|j| j.id == pending.job).is_none_or(|j| j.progress.finished.load(std::sync::atomic::Ordering::Relaxed)));
    if !finished {
        return;
    }
    let Some(pending) = s.nl.assistant_pending.take() else { return };
    let Some(result) = pending.result.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
    if pending.epoch != s.document_epoch {
        s.error_toast("nl.assistant.ask", "the project was replaced; the answer was discarded");
        return;
    }
    match result {
        Ok(value) => {
            s.nl.results.insert("assistant", value);
            s.toast("NL assistant: answer ready");
        }
        Err(e) => s.error_toast("nl.assistant.ask", format!("NL assistant: {e}")),
    }
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "nl.assistant.ask",
        label: "NL Ask the Project",
        menu: &[],
        shortcut: None,
        params: r#"{"question":str,"language":str?,"model":str?,"wait":bool?}"#,
        enabled: always,
        run: ask,
        journal: false,
        named: crate::commands::NamedKeys::NONE,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_is_required_and_answers_come_from_documents() {
        let mut s = Session::default();
        assert!(s.execute("nl.assistant.ask", json!({})).is_err());
        assert!(s.execute("nl.assistant.ask", json!({"question": "   "})).is_err());
        s.execute("file.openDemoProject", json!({})).unwrap();
        // nothing in the demo matches this: refused before contacting Ollama
        let error = s.execute("nl.assistant.ask", json!({"question": "zzzzqqq", "wait": true})).unwrap_err().to_string();
        assert!(error.contains("no project document matches"), "{error}");
    }

    #[test]
    fn the_prompt_numbers_the_documents_with_source_and_time() {
        let results = vec![json!({"source": "marker", "title": "Intro", "timeSec": 2.0, "snippet": "hello"})];
        let text = prompt("what is this?", &results, "italiano");
        assert!(text.contains("[1] (marker, 2.0s, Intro) hello"), "{text}");
        assert!(text.contains("Rispondi in italiano"));
    }
}
