use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub aspect_ratio: String,
    /// Content direction for this project, independent from the UI theme.
    #[serde(default)]
    pub creative_brief: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production_preset: Option<crate::production_preset::Selection>,
    #[serde(default)]
    pub story: Story,
    #[serde(default)]
    pub assets: Vec<Asset>,
    #[serde(default)]
    pub timeline: Timeline,
    #[serde(default)]
    pub transcripts: Vec<Transcript>,
    /// Editable AI graphic placeholders. Media enter the timeline only after review.
    #[serde(default)]
    pub graphic_objects: Vec<GraphicObject>,
    #[serde(default)]
    pub longform: crate::longform::EditState,
    #[serde(default)]
    pub clip_origins: BTreeMap<Uuid, crate::longform::ClipOrigin>,
    #[serde(default)]
    pub transcript_metadata: BTreeMap<Uuid, Value>,
    #[serde(default)]
    pub shorts: crate::shorts_studio::ShortsState,
    #[serde(default)]
    pub subtitles: crate::subtitle_studio::Document,
    #[serde(default)]
    pub subtitle_history: crate::subtitle_history::History,
    #[serde(default)]
    pub motion: crate::motion::Document,
    #[serde(default)]
    pub social: crate::social::Document,
    #[serde(default)]
    pub montage: crate::creative_montage::Document,
    #[serde(default)]
    pub audio: crate::audio_mixer::Document,
    /// Render-only source for stateful audio across cropped preview windows.
    #[serde(skip)]
    pub render_audio_source: Option<crate::audio_mixer::Source>,
    /// Reviewed:false translation layers per transcript and language. The
    /// editorial corrections above feed the source text; raw ASR never moves.
    #[serde(default)]
    pub translations: crate::transcript_assistant::TranslationStore,
}

impl Project {
    pub fn new(title: String, aspect_ratio: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title,
            created_at: now,
            updated_at: now,
            aspect_ratio: aspect_ratio.unwrap_or_else(|| "16:9".to_owned()),
            creative_brief: String::new(),
            production_preset: None,
            story: Story::default(),
            assets: Vec::new(),
            timeline: Timeline::default(),
            transcripts: Vec::new(),
            graphic_objects: Vec::new(),
            longform: crate::longform::EditState::default(),
            clip_origins: BTreeMap::new(),
            transcript_metadata: BTreeMap::new(),
            shorts: crate::shorts_studio::ShortsState::default(),
            subtitles: crate::subtitle_studio::Document::default(),
            subtitle_history: crate::subtitle_history::History::default(),
            motion: crate::motion::Document::default(),
            social: crate::social::Document::default(),
            montage: crate::creative_montage::Document::default(),
            audio: crate::audio_mixer::Document::default(),
            render_audio_source: None,
            translations: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphicObject {
    pub id: Uuid,
    pub title: String,
    pub start_ms: u64,
    pub duration_ms: u64,
    #[serde(default)]
    pub source_clip_id: Option<Uuid>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub source_text: String,
    #[serde(default)]
    pub workflow_id: Option<Uuid>,
    pub status: String,
    #[serde(default)]
    pub job_id: Option<Uuid>,
    #[serde(default)]
    pub asset_id: Option<Uuid>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default = "image_kind")]
    pub media_kind: AssetKind,
    #[serde(default)]
    pub workflow_binding: Option<crate::narrative_workflows::WorkflowBinding>,
}

fn image_kind() -> AssetKind {
    AssetKind::Image
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Story {
    #[serde(default)]
    pub premise: String,
    #[serde(default)]
    pub beats: Vec<StoryBeat>,
    #[serde(default)]
    pub characters: Vec<StoryEntity>,
    #[serde(default)]
    pub locations: Vec<StoryEntity>,
    #[serde(default)]
    pub scenes: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryBeat {
    pub id: Uuid,
    pub order: u32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryEntity {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub reference_asset_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: Uuid,
    pub name: String,
    pub kind: AssetKind,
    pub file_name: String,
    /// Original location, retained for audit only. Rendering always uses the project copy.
    pub source_path: String,
    pub imported_at: DateTime<Utc>,
    #[serde(default)]
    pub media: Option<MediaInfo>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Video,
    Audio,
    Image,
    Subtitle,
    Document,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaInfo {
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub frame_rate: Option<f64>,
    #[serde(default)]
    pub has_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub language: String,
    pub segments: Vec<TranscriptSegment>,
    #[serde(default)]
    pub words: Vec<TranscriptSegment>,
    pub imported_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub tracks: Vec<Track>,
    pub clips: Vec<TimelineClip>,
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            tracks: vec![
                Track::new("video", TrackKind::Video),
                Track::new("overlay", TrackKind::Overlay),
                Track::new("audio", TrackKind::Audio),
            ],
            clips: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: Uuid,
    pub name: String,
    pub kind: TrackKind,
    pub visible: bool,
    pub locked: bool,
}

impl Track {
    pub fn new(name: &str, kind: TrackKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_owned(),
            kind,
            visible: true,
            locked: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Video,
    Overlay,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineClip {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub track_id: Uuid,
    pub start_ms: u64,
    pub trim_in_ms: u64,
    pub duration_ms: u64,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "unit_float")]
    pub volume: f64,
    #[serde(default = "unit_float")]
    pub opacity: f64,
    #[serde(default)]
    pub transform: VisualTransform,
}

/// Normalized centre, proportional contain-scale and clockwise rotation.
/// Shared by the visual monitor and the FFmpeg compositor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VisualTransform {
    /// Transient render window: preserve legacy fade/slide phase after rebasing.
    #[serde(skip)]
    pub render_motion_origin_ms: Option<i64>,
    #[serde(skip)]
    pub render_motion_duration_ms: Option<u64>,
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rotation: f64,
    pub motion: String,
    pub fit: String,
    /// Background sigma as a fraction of canvas width; independent of foreground.
    pub background_blur: f64,
    pub background_brightness: f64,
    pub background_intensity: f64,
    pub foreground_crop: Option<crate::motion::Crop>,
    pub focus_x: f64,
    pub focus_y: f64,
    pub camera_path: Option<crate::reframe::CameraPath>,
    /// Explicit envelopes; zero retains the old motion preset when selected.
    pub fade_in_ms: u64,
    pub fade_out_ms: u64,
    pub audio_fade_in_ms: u64,
    pub audio_fade_out_ms: u64,
    /// Proportional camera movement, never a face warp or source modification.
    pub zoom_start: f64,
    pub zoom_end: f64,
    pub pan_x: f64,
    pub pan_y: f64,
}
impl Default for VisualTransform {
    fn default() -> Self {
        Self {
            render_motion_origin_ms: None,
            render_motion_duration_ms: None,
            x: 0.5,
            y: 0.5,
            scale: 1.0,
            rotation: 0.0,
            motion: "none".into(),
            fit: "contain".into(),
            background_blur: 0.035,
            background_brightness: 0.48,
            background_intensity: 1.0,
            foreground_crop: None,
            focus_x: 0.5,
            focus_y: 0.5,
            camera_path: None,
            fade_in_ms: 0,
            fade_out_ms: 0,
            audio_fade_in_ms: 0,
            audio_fade_out_ms: 0,
            zoom_start: 1.0,
            zoom_end: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}
impl VisualTransform {
    pub fn resolved_fit(&self, source_ratio: f64, target_ratio: f64) -> &str {
        if self.fit != "auto" {
            return &self.fit;
        }
        let retained = (source_ratio / target_ratio).min(target_ratio / source_ratio);
        if retained >= 0.8
            || self
                .camera_path
                .as_ref()
                .is_some_and(|p| p.enabled && p.preferred_fit == "cover")
        {
            "cover"
        } else {
            "blur"
        }
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(c) = &self.foreground_crop {
            anyhow::ensure!(
                [c.left, c.right, c.top, c.bottom]
                    .iter()
                    .all(|v| v.is_finite() && (0. ..=0.95).contains(v))
                    && c.left + c.right <= 0.95
                    && c.top + c.bottom <= 0.95,
                "Ritaglio foreground fuori sorgente: conserva almeno5% per asse"
            );
        }
        anyhow::ensure!(
            self.background_blur.is_finite()
                && (0.0..=0.08).contains(&self.background_blur)
                && self.background_brightness.is_finite()
                && (0.0..=1.5).contains(&self.background_brightness)
                && self.background_intensity.is_finite()
                && (0.0..=1.0).contains(&self.background_intensity),
            "Sfondo: blur0–8%W, luminosità0–150%, intensità0–100%"
        );
        if let Some(path) = &self.camera_path {
            path.validate()?;
        }
        anyhow::ensure!(
            [
                self.fade_in_ms,
                self.fade_out_ms,
                self.audio_fade_in_ms,
                self.audio_fade_out_ms
            ]
            .iter()
            .all(|v| *v <= 60_000)
                && [self.zoom_start, self.zoom_end]
                    .iter()
                    .all(|v| v.is_finite() && (1.0..=1.5).contains(v))
                && [self.pan_x, self.pan_y]
                    .iter()
                    .all(|v| v.is_finite() && (-0.25..=0.25).contains(v)),
            "Movimento: zoom 100–150%, pan ±25%; fade massimo 60 secondi"
        );
        anyhow::ensure!(
            ["contain", "cover", "blur", "auto"].contains(&self.fit.as_str())
                && self.focus_x.is_finite()
                && self.focus_y.is_finite()
                && (0.0..=1.0).contains(&self.focus_x)
                && (0.0..=1.0).contains(&self.focus_y),
            "Inquadratura: contain/cover/blur/auto e fuoco normalizzato tra 0 e 1"
        );
        anyhow::ensure!(
            matches!(
                self.motion.as_str(),
                "none" | "fade" | "slide_left" | "slide_up"
            ),
            "Animazione non supportata"
        );
        anyhow::ensure!(self.x.is_finite() && (-4.0..=4.0).contains(&self.x)
            && self.y.is_finite() && (-4.0..=4.0).contains(&self.y)
            && self.scale.is_finite() && (0.01..=8.0).contains(&self.scale)
            && self.rotation.is_finite() && (-360.0..=360.0).contains(&self.rotation),
            "Trasformazione fuori dai limiti: posizione -400..400%, scala 1..800%, rotazione -360..360°");
        Ok(())
    }
}

#[cfg(test)]
mod animation_tests {
    use super::*;
    #[test]
    fn old_transforms_keep_defaults_and_invalid_camera_is_rejected() {
        let mut t: VisualTransform =
            serde_json::from_str(r#"{"scale":1.2,"motion":"fade"}"#).unwrap();
        assert_eq!(t.zoom_end, 1.0);
        assert_eq!(t.fade_in_ms, 0);
        assert!(t.validate().is_ok());
        t.pan_x = f64::NAN;
        assert!(t.validate().is_err());
        t.pan_x = 0.1;
        t.zoom_end = 1.51;
        assert!(t.validate().is_err());
    }
}

fn unit_float() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: Uuid,
    pub kind: JobKind,
    pub status: JobStatus,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    pub message: String,
    #[serde(default)]
    pub output_paths: Vec<String>,
    #[serde(default)]
    pub result: Option<Value>,
    /// UI archive only: records remain available to coordinators and approvals.
    #[serde(default)]
    pub archived: bool,
}

impl Job {
    pub fn queued(kind: JobKind, project_id: Option<Uuid>, message: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            status: JobStatus::Queued,
            project_id,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            message: message.into(),
            output_paths: Vec::new(),
            result: None,
            archived: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    ExportProductionPackage,
    ProduceNarrative,
    PreparePreview,
    ImportAsset,
    ExtractClip,
    ExportTimeline,
    ScanWorkflows,
    SubmitComfy,
    Pipeline,
    GenerateStory,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub comfy_url: String,
    pub ollama_url: String,
    #[serde(default)]
    pub llm_provider: LlmProvider,
    #[serde(default = "default_openai_base")]
    pub openai_base_url: String,
    pub legacy_sources: BTreeMap<String, PathBuf>,
    #[serde(default = "default_model")]
    pub llm_model: String,
    /// Optional model per real operation on the selected provider.
    #[serde(default)]
    pub role_llm_models: BTreeMap<String, String>,
    /// Per-request policy only: never changes the Ollama service configuration.
    #[serde(default)]
    pub ollama_execution: OllamaExecution,
    #[serde(default)]
    pub resources: crate::resources::Policy,
    #[serde(default = "default_python")]
    pub python_executable: PathBuf,
    #[serde(default = "default_engines")]
    pub engine_root: PathBuf,
    #[serde(default)]
    pub workflow_roots: Vec<PathBuf>,
    #[serde(default = "default_asr")]
    pub asr_model_path: PathBuf,
    #[serde(default = "default_model_cache")]
    pub model_cache_root: PathBuf,
    #[serde(default = "default_alignment")]
    pub alignment_model_path: PathBuf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    #[default]
    Ollama,
    OpenaiCompatible,
}

fn default_openai_base() -> String {
    "http://127.0.0.1:8080/v1".to_owned()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OllamaExecution {
    #[default]
    Cpu,
    Auto,
}

impl OllamaExecution {
    pub fn options(self, num_ctx: u32, num_predict: u32) -> Value {
        let mut options = serde_json::json!({"num_ctx": num_ctx, "num_predict": num_predict});
        if self == Self::Cpu {
            options["num_gpu"] = serde_json::json!(0);
        }
        options
    }
}
fn default_alignment() -> PathBuf {
    PathBuf::from("models/alignment/italian-xlsr53")
}
fn default_asr() -> PathBuf {
    PathBuf::from("models/asr/faster-whisper-large-v3")
}
fn default_model_cache() -> PathBuf {
    PathBuf::from("models")
}

fn default_python() -> PathBuf {
    PathBuf::from("runtime/python/python.exe")
}

fn default_engines() -> PathBuf {
    PathBuf::from("engines")
}

fn default_model() -> String {
    "qwen2.5:14b-instruct".to_owned()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            comfy_url: "http://127.0.0.1:8188".to_owned(),
            ollama_url: "http://127.0.0.1:11434".to_owned(),
            llm_provider: LlmProvider::default(),
            openai_base_url: default_openai_base(),
            llm_model: default_model(),
            role_llm_models: BTreeMap::new(),
            ollama_execution: OllamaExecution::default(),
            resources: crate::resources::Policy::default(),
            python_executable: default_python(),
            engine_root: default_engines(),
            workflow_roots: vec![],
            asr_model_path: default_asr(),
            model_cache_root: default_model_cache(),
            alignment_model_path: default_alignment(),
            legacy_sources: BTreeMap::new(),
        }
    }
}

impl AppConfig {
    pub fn llm_for_role(&self, role: &str) -> &str {
        self.role_llm_models
            .get(role)
            .map(String::as_str)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&self.llm_model)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub input_roles: Vec<String>,
    pub output_roles: Vec<String>,
    pub node_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Integration {
    pub id: String,
    pub source_path: String,
    pub source_available: bool,
    pub capabilities: Vec<String>,
    pub migration_status: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub title: String,
    pub aspect_ratio: Option<String>,
}

#[cfg(test)]
mod composition_tests {
    use super::*;
    #[test]
    fn legacy_contain_and_auto_aspect_policy_are_explicit_and_serialized() {
        let old: VisualTransform = serde_json::from_str("{}").unwrap();
        assert_eq!(old.fit, "contain");
        let mut tr = VisualTransform {
            fit: "auto".into(),
            ..old
        };
        assert_eq!(tr.resolved_fit(16. / 9., 9. / 16.), "blur");
        assert_eq!(tr.resolved_fit(16. / 9., 16. / 9.), "cover");
        tr.foreground_crop = Some(crate::motion::Crop {
            left: 0.08,
            right: 0.1,
            top: 0.,
            bottom: 0.,
        });
        tr.background_blur = 0.055;
        let reopened: VisualTransform =
            serde_json::from_value(serde_json::to_value(&tr).unwrap()).unwrap();
        assert_eq!(reopened.foreground_crop.unwrap().right, 0.1);
        tr.validate().unwrap();
        tr.foreground_crop.as_mut().unwrap().right = 0.95;
        assert!(tr.validate().is_err());
        tr.foreground_crop = None;
        tr.background_blur = f64::NAN;
        assert!(tr.validate().is_err());
    }
}

#[derive(Debug, Deserialize)]
pub struct ImportAssetRequest {
    pub path: PathBuf,
    pub kind: Option<AssetKind>,
}

#[derive(Debug, Deserialize)]
pub struct AddTimelineClipRequest {
    pub asset_id: Uuid,
    pub track_id: Option<Uuid>,
    pub start_ms: Option<u64>,
    pub trim_in_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub label: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SplitClipRequest {
    pub at_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct ExtractClipRequest {
    pub asset_id: Uuid,
    pub start_ms: u64,
    pub end_ms: u64,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowScanRequest {
    pub roots: Option<Vec<PathBuf>>,
}

#[derive(Debug, Deserialize)]
pub struct ComfySubmitRequest {
    pub workflow_path: PathBuf,
    pub project_id: Option<Uuid>,
    /// Keys are `node_id.input_name`, values are inserted into the workflow JSON.
    #[serde(default)]
    pub inputs: BTreeMap<String, Value>,
    #[serde(default)]
    pub asset_inputs: BTreeMap<String, Uuid>,
    #[serde(default)]
    pub asset_ranges: BTreeMap<String, crate::time_map::TimeRange>,
    pub placement: Option<GraphicPlacement>,
    #[serde(default)]
    pub review_before_insert: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphicPlacement {
    pub source_clip_id: Uuid,
    pub offset_ms: u64,
    pub duration_ms: u64,
    pub label: String,
}
