//! Runtime state for NL background work that is not a media job of the editor itself: the bridge
//! pipeline (clipping, rough cut) and the last result each operation produced.
//!
//! Results live in the session so panels, commands and agents all read the same proposals; nothing
//! here is persisted in the project, which keeps a failed analysis from touching the edit.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

/// One finding of the workflow doctor.
#[derive(Clone, Debug)]
pub struct WorkflowIssue {
    pub severity: &'static str,
    pub node: Option<String>,
    pub class: Option<String>,
    pub message: String,
    pub fix: Option<String>,
}

impl WorkflowIssue {
    pub fn error(node: Option<String>, class: Option<String>, message: impl Into<String>, fix: Option<String>) -> Self {
        Self { severity: "error", node, class, message: message.into(), fix }
    }
    pub fn warning(node: Option<String>, class: Option<String>, message: impl Into<String>, fix: Option<String>) -> Self {
        Self { severity: "warning", node, class, message: message.into(), fix }
    }
    pub fn info(node: Option<String>, class: Option<String>, message: impl Into<String>, fix: Option<String>) -> Self {
        Self { severity: "info", node, class, message: message.into(), fix }
    }
    pub fn to_json(&self) -> Value {
        json!({"severity": self.severity, "node": self.node, "class": self.class, "message": self.message, "fix": self.fix})
    }
}

/// One bridge run whose result is moved into [`NlState::results`] when it finishes.
pub struct PendingNl {
    pub job: u64,
    pub operation: &'static str,
    pub result: Arc<Mutex<Option<std::result::Result<Value, String>>>>,
    pub epoch: u64,
}

/// A story draft running in the background.
pub struct PendingStory {
    pub job: u64,
    pub result: Arc<Mutex<Option<std::result::Result<Value, String>>>>,
    pub epoch: u64,
}

/// A longform analysis running in the background.
pub struct PendingLongform {
    pub job: u64,
    pub result: Arc<Mutex<Option<std::result::Result<Value, String>>>>,
    pub epoch: u64,
}

/// A prompt suggestion running in the background.
pub struct PendingPrompt {
    pub job: u64,
    pub result: Arc<Mutex<Option<std::result::Result<Value, String>>>>,
    pub epoch: u64,
    /// Where the finished answer goes in [`NlState::results`] (`prompt`, `h3prompt`).
    pub key: &'static str,
}

/// NL work in progress and the results the panels show.
#[derive(Default)]
pub struct NlState {
    pub pending: Vec<PendingNl>,
    /// Last result per operation (`clips`, `reels`, `cleanup`, `render_clips`, `render_cleanup`).
    pub results: BTreeMap<&'static str, Value>,
    /// Files the last render wrote and the project imported, for the report.
    pub files: Vec<String>,
    /// The image the operator marked as this project's cover (`nl.cover.set`).
    pub cover: Option<Value>,
    /// ComfyUI's `/object_info` as last read by `nl.workflows.verify`, reused by the doctor so
    /// clicking Diagnose after Verify does not download it again.
    pub object_info: Option<Value>,
    /// The AI story (scenes, prompts and the generated media linked to them).
    pub story: Option<Value>,
    /// A background story draft waiting to be applied.
    pub story_pending: Option<PendingStory>,
    /// A background prompt suggestion waiting to be offered to the operator.
    pub prompt_pending: Option<PendingPrompt>,
    /// The longform chapters of the last analysis.
    pub longform: Option<Value>,
    /// The project search index (clips, markers, captions, story, transcripts).
    pub index: Option<Vec<Value>>,
    /// The variation queue (prompt/seed/workflow with their prepared payloads).
    pub queue: Vec<Value>,
    /// The last montage plan, ready to apply.
    pub montage: Option<Value>,
    /// Next queue entry id.
    pub queue_next: u64,
    /// Explicit batch of NL commands started together (separate from the variation queue).
    pub batch: Vec<Value>,
    pub batch_next: u64,
    /// A background longform analysis waiting to be applied.
    pub longform_pending: Option<PendingLongform>,
    /// A background project-assistant answer waiting to be offered.
    pub assistant_pending: Option<PendingPrompt>,
    /// The NL timeline sketch: generation placeholders with their prompts and generated media.
    pub agent_plan: Option<Value>,
}

impl NlState {
    /// The last result of `operation`, if that step has run in this session.
    pub fn result(&self, operation: &str) -> Option<&Value> {
        self.results.get(operation)
    }
}
