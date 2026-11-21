//! Transcript-driven editing over the common project/timeline, not another NLE.
use crate::{
    editor,
    model::*,
    time_map::{self, TimeRange},
    transcript_core, update_project, ApiError, AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub range: TimeRange,
    pub action: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EditState {
    pub active_transcripts: BTreeMap<Uuid, Uuid>,
    pub asr_profile_id: Option<String>,
    pub schema_version: u32,
    pub revision: u64,
    pub corrections: BTreeMap<Uuid, String>,
    pub decisions: Vec<Decision>,
    pub chapters: Vec<crate::longform_analysis::Chapter>,
    pub cut_candidates: Vec<crate::longform_analysis::CutCandidate>,
    pub analysis_job_id: Option<Uuid>,
    pub shot_decisions: Vec<crate::shot_map::Decision>,
    pub shot_parameters: BTreeMap<Uuid, crate::shot_map::Params>,
}
impl Default for EditState {
    fn default() -> Self {
        Self {
            active_transcripts: BTreeMap::new(),
            asr_profile_id: None,
            schema_version: 1,
            revision: 0,
            corrections: BTreeMap::new(),
            decisions: vec![],
            chapters: vec![],
            cut_candidates: vec![],
            analysis_job_id: None,
            shot_decisions: vec![],
            shot_parameters: BTreeMap::new(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipOrigin {
    pub producer: String,
    pub asset_id: Uuid,
    pub input_revision: u64,
}

fn bad(e: impl ToString) -> ApiError {
    ApiError::bad_request(e.to_string())
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
fn source(p: &Project, id: Uuid) -> Result<&Asset, ApiError> {
    p.assets
        .iter()
        .find(|a| a.id == id && matches!(a.kind, AssetKind::Audio | AssetKind::Video))
        .ok_or_else(|| bad("Scegli audio o video del progetto"))
}

#[derive(Deserialize, Default)]
pub struct WordQuery {
    #[serde(default)]
    pub offset: usize,
    pub limit: Option<usize>,
    pub search: Option<String>,
}
pub async fn view(
    State(state): State<AppState>,
    Path((id, asset)): Path<(Uuid, Uuid)>,
    Query(q): Query<WordQuery>,
) -> Result<Json<Value>, ApiError> {
    let p = project(&state, id).await?;
    source(&p, asset)?;
    let transcript = crate::asr::active(&p, asset);
    let meta = transcript.and_then(|t| p.transcript_metadata.get(&t.id));
    let engine = meta.and_then(|m| m["engine"].as_str()).unwrap_or("unknown");
    let aligned = meta.is_some_and(|m| m["alignment_completed"] == true);
    let all = transcript
        .map(|t| transcript_core::words(t, &p.longform.corrections, engine, aligned))
        .unwrap_or_default();
    let search = q.search.unwrap_or_default().to_lowercase();
    let total = all.len();
    // Explicit facts for the editor: the media may be longer than the speech.
    let text_start_ms = all.iter().map(|w| w.start_ms).min().unwrap_or(0);
    let text_end_ms = all.iter().map(|w| w.end_ms).max().unwrap_or(0);
    let filtered = all
        .into_iter()
        .filter(|w| {
            search.is_empty()
                || w.corrected_text
                    .as_deref()
                    .unwrap_or(&w.raw_text)
                    .to_lowercase()
                    .contains(&search)
        })
        .collect::<Vec<_>>();
    let matches = filtered.len();
    let words = filtered
        .into_iter()
        .skip(q.offset)
        .take(q.limit.unwrap_or(200).clamp(1, 500))
        .collect::<Vec<_>>();
    // Visible interval of this page, so the UI never has to guess it.
    let page_start_ms = words.iter().map(|w| w.start_ms).min().unwrap_or(0);
    let page_end_ms = words.iter().map(|w| w.end_ms).max().unwrap_or(0);
    let digest = transcript.map(|t| crate::longform_analysis::digest(&p, t));
    let duration = source(&p, asset)?
        .media
        .as_ref()
        .and_then(|m| m.duration_ms)
        .unwrap_or(0);
    let mapping = if duration > 0 {
        time_map::source_to_timeline(&p, asset, TimeRange::new(0, duration).map_err(bad)?)
            .map_err(bad)?
    } else {
        vec![]
    };
    Ok(Json(
        json!({"schema_version":2,"state":p.longform,"project_updated_at":p.updated_at,"asset_id":asset,"transcript_id":transcript.map(|t|t.id),"words":words,"total_words":total,"matched_words":matches,"offset":q.offset,"duration_ms":duration,"text_start_ms":text_start_ms,"text_end_ms":text_end_ms,"page_start_ms":page_start_ms,"page_end_ms":page_end_ms,"digest":digest,"mapping":mapping,"metadata":meta}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveRequest {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub state: EditState,
}
fn validate(p: &Project, next: &EditState) -> Result<(), ApiError> {
    for (asset, transcript) in &next.active_transcripts {
        if !p
            .transcripts
            .iter()
            .any(|t| t.id == *transcript && t.asset_id == *asset)
        {
            return Err(bad("Versione raw estranea al media"));
        }
    }
    crate::shot_map::validate_decisions(p, &next.shot_decisions)?;
    crate::shot_map::validate_parameters(p, &next.shot_parameters)?;
    crate::longform_analysis::validate(p, next)?;
    if next.schema_version != 1 || next.decisions.len() > 20_000 || next.corrections.len() > 100_000
    {
        return Err(bad("Stato editing oltre i limiti del formato 1"));
    }
    let known = p
        .transcripts
        .iter()
        .flat_map(|t| transcript_core::words(t, &BTreeMap::new(), "unknown", false))
        .map(|w| w.id)
        .collect::<HashSet<_>>();
    for (id, text) in &next.corrections {
        if !known.contains(id) && !p.longform.corrections.contains_key(id) {
            return Err(bad("Correzione riferita a una parola estranea al progetto"));
        }
        if text.trim().is_empty()
            || text.chars().count() > 2000
            || text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
        {
            return Err(bad("Correzione vuota o non valida"));
        }
    }
    let mut ids = HashSet::new();
    for d in &next.decisions {
        let a = source(p, d.asset_id)?;
        TimeRange::new(d.range.start_ms, d.range.end_ms).map_err(bad)?;
        if !ids.insert(d.id)
            || d.id.is_nil()
            || !["exclude", "keep", "review", "highlight"].contains(&d.action.as_str())
            || d.reason.len() > 2000
        {
            return Err(bad("Decisione non valida o duplicata"));
        }
        if a.media
            .as_ref()
            .and_then(|m| m.duration_ms)
            .is_none_or(|ms| d.range.end_ms > ms)
        {
            return Err(bad("Decisione oltre la durata del media"));
        }
    }
    Ok(())
}
pub async fn save(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<SaveRequest>,
) -> Result<Json<Project>, ApiError> {
    update_project(&state, id, |p| {
        if p.updated_at != req.expected_updated_at || p.longform.revision != req.state.revision {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "Lavoro aggiornato altrove: ricarica prima di salvare".into(),
            ));
        }
        validate(p, &req.state)?;
        p.longform = req.state;
        p.longform.revision = p.longform.revision.saturating_add(1);
        Ok(())
    })
    .await?;
    Ok(Json(project(&state, id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyRequest {
    pub asset_id: Uuid,
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub expected_revision: u64,
}
pub fn kept_ranges(p: &Project, asset: Uuid) -> Result<Vec<TimeRange>, ApiError> {
    let a = source(p, asset)?;
    let duration = a
        .media
        .as_ref()
        .and_then(|m| m.duration_ms)
        .ok_or_else(|| bad("Durata media assente"))?;
    // Decisions are ordered: a later Keep can restore part of an exclusion.
    let mut excludes: Vec<TimeRange> = p
        .longform
        .cut_candidates
        .iter()
        .filter(|c| c.asset_id == asset && c.approved)
        .map(|c| c.range)
        .collect();
    for d in p.longform.decisions.iter().filter(|d| d.asset_id == asset) {
        if d.action == "exclude" {
            excludes.push(d.range);
        } else if d.action == "keep" {
            excludes = excludes
                .into_iter()
                .flat_map(|r| {
                    if let Some(overlap) = r.intersection(d.range) {
                        let mut parts = vec![];
                        if r.start_ms < overlap.start_ms {
                            parts.push(TimeRange {
                                start_ms: r.start_ms,
                                end_ms: overlap.start_ms,
                            });
                        }
                        if overlap.end_ms < r.end_ms {
                            parts.push(TimeRange {
                                start_ms: overlap.end_ms,
                                end_ms: r.end_ms,
                            });
                        }
                        parts
                    } else {
                        vec![r]
                    }
                })
                .collect();
        }
    }
    let keeps = time_map::subtract(duration, &excludes).map_err(bad)?;
    if keeps.is_empty() {
        return Err(bad(
            "La proposta escluderebbe tutto il media: nessun montaggio applicato",
        ));
    }
    Ok(keeps)
}
pub fn proposal(
    p: &Project,
    asset: Uuid,
) -> Result<(Timeline, BTreeMap<Uuid, ClipOrigin>), ApiError> {
    let a = source(p, asset)?;
    let keeps = kept_ranges(p, asset)?;
    let owned = p
        .clip_origins
        .iter()
        .filter(|(_, o)| o.producer == "longform" && o.asset_id == asset)
        .map(|(id, _)| *id)
        .collect::<HashSet<_>>();
    if p.timeline.clips.iter().any(|c| {
        owned.contains(&c.id)
            && p.timeline
                .tracks
                .iter()
                .any(|t| t.id == c.track_id && t.locked)
    }) {
        return Err(bad(
            "Sblocca le tracce dei clip longform prima di aggiornare il montaggio",
        ));
    }
    let anchor = p
        .timeline
        .clips
        .iter()
        .filter(|c| owned.contains(&c.id))
        .map(|c| c.start_ms)
        .min();
    let mut tl = p.timeline.clone();
    tl.clips.retain(|c| !owned.contains(&c.id));
    let kind = if a.kind == AssetKind::Audio {
        TrackKind::Audio
    } else {
        TrackKind::Video
    };
    let track = if let Some(t) = tl
        .tracks
        .iter()
        .find(|t| t.kind == kind && t.visible && !t.locked)
    {
        t.id
    } else {
        let t = Track::new(
            if kind == TrackKind::Audio {
                "Longform · Audio"
            } else {
                "Longform · Video"
            },
            kind,
        );
        let id = t.id;
        tl.tracks.push(t);
        id
    };
    let mut start = anchor.unwrap_or_else(|| {
        tl.clips
            .iter()
            .map(|c| c.start_ms + c.duration_ms)
            .max()
            .unwrap_or(0)
    });
    let mut origins = p.clip_origins.clone();
    origins.retain(|id, _| !owned.contains(id));
    for (i, r) in keeps.into_iter().enumerate() {
        let id = Uuid::new_v5(
            &p.id,
            format!("longform:{asset}:{}:{}", r.start_ms, r.end_ms).as_bytes(),
        );
        tl.clips.push(TimelineClip {
            id,
            asset_id: asset,
            track_id: track,
            start_ms: start,
            trim_in_ms: r.start_ms,
            duration_ms: r.duration(),
            label: Some(format!("{} · {}", a.name, i + 1)),
            volume: 1.,
            opacity: 1.,
            transform: VisualTransform::default(),
        });
        start += r.duration();
        origins.insert(
            id,
            ClipOrigin {
                producer: "longform".into(),
                asset_id: asset,
                input_revision: p.longform.revision,
            },
        );
    }
    editor::validate_timeline(p, &tl).map_err(bad)?;
    Ok((tl, origins))
}
pub async fn preview(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ApplyRequest>,
) -> Result<Json<Value>, ApiError> {
    let p = project(&state, id).await?;
    check_revision(&p, &req)?;
    let (timeline, origins) = proposal(&p, req.asset_id)?;
    let mut proposed = p.clone();
    proposed.timeline = timeline.clone();
    let a = source(&p, req.asset_id)?;
    let duration = a.media.as_ref().and_then(|m| m.duration_ms).unwrap_or(0);
    let mapping = time_map::source_to_timeline(
        &proposed,
        req.asset_id,
        TimeRange::new(0, duration).map_err(bad)?,
    )
    .map_err(bad)?
    .into_iter()
    .filter(|m| {
        origins
            .get(&m.clip_id)
            .is_some_and(|o| o.producer == "longform" && o.asset_id == req.asset_id)
    })
    .collect::<Vec<_>>();
    Ok(Json(
        json!({"timeline":timeline,"source_duration_ms":duration,"mapping":mapping,"expected_updated_at":p.updated_at,"expected_revision":p.longform.revision}),
    ))
}
fn check_revision(p: &Project, req: &ApplyRequest) -> Result<(), ApiError> {
    if p.updated_at != req.expected_updated_at || p.longform.revision != req.expected_revision {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "La proposta è cambiata: aggiorna l'anteprima prima di applicare".into(),
        ));
    }
    Ok(())
}
pub async fn apply(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ApplyRequest>,
) -> Result<Json<Project>, ApiError> {
    update_project(&state, id, |p| {
        check_revision(p, &req)?;
        let (tl, origins) = proposal(p, req.asset_id)?;
        p.timeline = tl;
        p.clip_origins = origins;
        Ok(())
    })
    .await?;
    Ok(Json(project(&state, id).await?))
}

#[derive(Deserialize)]
pub struct MapRequest {
    pub asset_id: Option<Uuid>,
    pub range: TimeRange,
}
pub async fn mapping(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<MapRequest>,
) -> Result<Json<Value>, ApiError> {
    let p = project(&state, id).await?;
    let maps = if let Some(asset) = req.asset_id {
        source(&p, asset)?;
        time_map::source_to_timeline(&p, asset, req.range)
    } else {
        time_map::timeline_to_source(&p, req.range)
    }
    .map_err(bad)?;
    Ok(Json(json!({"schema_version":1,"mapping":maps})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audio_cut_is_stable_and_preserves_foreign_clips() {
        let mut p = Project::new("Test".into(), None);
        let asset = Asset {
            id: Uuid::new_v4(),
            name: "Voce".into(),
            kind: AssetKind::Audio,
            file_name: "x.wav".into(),
            source_path: String::new(),
            imported_at: chrono::Utc::now(),
            media: Some(MediaInfo {
                duration_ms: Some(10000),
                width: None,
                height: None,
                frame_rate: None,
                has_audio: true,
            }),
        };
        let id = asset.id;
        p.assets.push(asset);
        p.longform.decisions.push(Decision {
            id: Uuid::new_v4(),
            asset_id: id,
            range: TimeRange::new(2000, 4000).unwrap(),
            action: "exclude".into(),
            reason: "Scelta esplicita".into(),
        });
        let (tl, origin) = proposal(&p, id).unwrap();
        assert_eq!(tl.clips.len(), 2);
        assert_eq!(tl.clips[1].trim_in_ms, 4000);
        assert_eq!(tl.clips[1].start_ms, 2000);
        assert_eq!(tl.clips.iter().map(|c| c.duration_ms).sum::<u64>(), 8000);
        p.timeline = tl;
        p.clip_origins = origin;
        let (tl, _) = proposal(&p, id).unwrap();
        assert_eq!(tl.clips.len(), 2);
        let track = tl.clips[0].track_id;
        p.timeline
            .tracks
            .iter_mut()
            .find(|t| t.id == track)
            .unwrap()
            .locked = true;
        assert!(proposal(&p, id).is_err());
    }
}
