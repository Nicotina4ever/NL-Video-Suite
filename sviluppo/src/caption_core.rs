//! Canonical cue projection. Raw ASR is never rewritten; all timings come from
//! the same source/clip mappings used for transcript-driven editing.
use crate::{model::Project, time_map::TimeRange, transcript_core, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::header,
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Cue {
    pub id: Uuid,
    pub clip_id: Uuid,
    pub asset_id: Uuid,
    pub source: TimeRange,
    pub timeline: TimeRange,
    pub word_ids: Vec<Uuid>,
    pub text: String,
    pub raw_text: String,
    pub requires_review: bool,
    pub timing_unit: String,
}
pub fn cues(p: &Project) -> Vec<Cue> {
    if !p.subtitles.units.is_empty() {
        return crate::subtitle_studio::projections(p, true)
            .into_iter()
            .map(|p| p.cue)
            .collect();
    }
    foundation_cues(p)
}
pub fn foundation_cues(p: &Project) -> Vec<Cue> {
    let mut cues = vec![];
    for clip in &p.timeline.clips {
        if !p
            .timeline
            .tracks
            .iter()
            .any(|t| t.id == clip.track_id && t.visible)
        {
            continue;
        }
        let Some(transcript) = crate::asr::active(p, clip.asset_id) else {
            continue;
        };
        let metadata = p.transcript_metadata.get(&transcript.id);
        let words = transcript_core::words(
            transcript,
            &p.longform.corrections,
            metadata
                .and_then(|m| m["engine"].as_str())
                .unwrap_or("unknown"),
            metadata.is_some_and(|m| m["alignment_completed"] == true),
        );
        let source = TimeRange {
            start_ms: clip.trim_in_ms,
            end_ms: clip.trim_in_ms + clip.duration_ms,
        };
        let mut current: Option<Cue> = None;
        for word in words {
            let Some(overlap) = source.intersection(TimeRange {
                start_ms: word.start_ms,
                end_ms: word.end_ms,
            }) else {
                continue;
            };
            let text = word.corrected_text.as_deref().unwrap_or(&word.raw_text);
            let split = current.as_ref().is_some_and(|c| {
                c.text.chars().count() + text.chars().count() + 1 > 84
                    || overlap.end_ms.saturating_sub(c.source.start_ms) > 6000
                    || overlap.start_ms.saturating_sub(c.source.end_ms) > 700
                    || word.timing_unit == "segment"
            });
            if split {
                if let Some(c) = current.take() {
                    cues.push(c);
                }
            }
            let cue = current.get_or_insert_with(|| Cue {
                id: Uuid::new_v5(&clip.id, word.id.as_bytes()),
                clip_id: clip.id,
                asset_id: clip.asset_id,
                source: overlap,
                timeline: TimeRange {
                    start_ms: clip.start_ms + overlap.start_ms - source.start_ms,
                    end_ms: clip.start_ms + overlap.end_ms - source.start_ms,
                },
                word_ids: vec![],
                text: String::new(),
                raw_text: String::new(),
                requires_review: false,
                timing_unit: word.timing_unit.clone(),
            });
            if !cue.text.is_empty() {
                cue.text.push(' ');
                cue.raw_text.push(' ');
            }
            cue.text.push_str(text);
            cue.raw_text.push_str(&word.raw_text);
            cue.word_ids.push(word.id);
            cue.source.end_ms = cue.source.end_ms.max(overlap.end_ms);
            cue.timeline.end_ms = clip.start_ms + cue.source.end_ms - source.start_ms;
            cue.requires_review |= word.align_status != "aligned"
                || overlap.start_ms != word.start_ms
                || overlap.end_ms != word.end_ms;
        }
        if let Some(c) = current {
            cues.push(c);
        }
    }
    cues.sort_by_key(|c| (c.timeline.start_ms, c.clip_id));
    cues
}
fn timestamp(ms: u64, vtt: bool) -> String {
    format!(
        "{:02}:{:02}:{:02}{}{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        if vtt { '.' } else { ',' },
        ms % 1000
    )
}
pub fn sidecar(cues: &[Cue], format: &str) -> Result<String, ApiError> {
    if !["srt", "vtt"].contains(&format) {
        return Err(ApiError::bad_request("Formato foundation: srt o vtt"));
    }
    let vtt = format == "vtt";
    let mut text = if vtt {
        "WEBVTT\n\n".into()
    } else {
        String::new()
    };
    for (index, c) in cues
        .iter()
        .filter(|c| !c.text.trim().is_empty() && c.timeline.end_ms > c.timeline.start_ms)
        .enumerate()
    {
        text.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            index + 1,
            timestamp(c.timeline.start_ms, vtt),
            timestamp(c.timeline.end_ms, vtt),
            c.text.replace("-->", "→")
        ));
    }
    Ok(text)
}
async fn project(state: &AppState, id: Uuid) -> Result<Project, ApiError> {
    state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))
}
pub async fn proposal(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let p = project(&state, id).await?;
    Ok(Json(
        serde_json::json!({"schema_version":1,"project_updated_at":p.updated_at,"cues":cues(&p),"status":"proposal","layout_rendered":false}),
    ))
}
pub async fn download(
    State(state): State<AppState>,
    Path((id, format)): Path<(Uuid, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let p = project(&state, id).await?;
    let cues = cues(&p);
    if cues.is_empty() {
        return Err(ApiError::bad_request(
            "Monta un media trascritto prima di esportare i sottotitoli",
        ));
    }
    let text = sidecar(&cues, &format)?;
    Ok((
        [
            (
                header::CONTENT_TYPE,
                if format == "vtt" {
                    "text/vtt; charset=utf-8"
                } else {
                    "application/x-subrip; charset=utf-8"
                },
            ),
            (
                header::CONTENT_DISPOSITION,
                if format == "vtt" {
                    "attachment; filename=montaggio.vtt"
                } else {
                    "attachment; filename=montaggio.srt"
                },
            ),
        ],
        text,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sidecar_unicode_and_integer_boundaries_are_preserved() {
        let cue = Cue {
            id: Uuid::new_v4(),
            clip_id: Uuid::new_v4(),
            asset_id: Uuid::new_v4(),
            source: TimeRange {
                start_ms: 4000,
                end_ms: 5001,
            },
            timeline: TimeRange {
                start_ms: 1000,
                end_ms: 2001,
            },
            word_ids: vec![],
            text: "Caffè, perché sì!".into(),
            raw_text: "Caffe".into(),
            requires_review: true,
            timing_unit: "word".into(),
        };
        assert!(sidecar(std::slice::from_ref(&cue), "srt")
            .unwrap()
            .contains("00:00:01,000 --> 00:00:02,001\nCaffè"));
        assert!(sidecar(&[cue], "vtt")
            .unwrap()
            .starts_with("WEBVTT\n\n1\n00:00:01.000"));
    }
}
