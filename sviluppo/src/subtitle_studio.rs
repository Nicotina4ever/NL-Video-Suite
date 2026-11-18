//! Source-anchored editorial captions, projected through the common timeline.
//! libass is the single typography renderer for monitor PNGs and burned video.
use crate::{
    caption_core, compositor, model::*, store, time_map::TimeRange, transcript_core,
    update_project, ApiError, AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashSet, path::Path as FsPath, sync::OnceLock};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Document {
    pub schema_version: u32,
    pub revision: u64,
    pub enabled: bool,
    pub language: String,
    pub style: Style,
    pub units: Vec<Unit>,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            enabled: false,
            language: "it".into(),
            style: Style::default(),
            units: vec![],
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Style {
    pub font: String,
    pub size: f64,
    pub color: String,
    pub highlight: String,
    pub outline: String,
    pub outline_size: f64,
    pub x: f64,
    pub y: f64,
    pub max_chars: u32,
    pub mode: String,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            font: "Arial".into(),
            size: 0.048,
            color: "#FFFFFF".into(),
            highlight: "#E5469C".into(),
            outline: "#101018".into(),
            outline_size: 3.,
            x: 0.5,
            y: 0.82,
            max_chars: 32,
            mode: "plain".into(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Word {
    pub id: Uuid,
    pub text: String,
    pub source: TimeRange,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Unit {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub transcript_id: Uuid,
    pub source: TimeRange,
    pub raw_text: String,
    pub text: String,
    pub language: String,
    pub words: Vec<Word>,
    pub align_status: String,
    pub approved: bool,
    pub style: Option<Style>,
    #[serde(default)]
    pub alignment: Option<Value>,
    /// Editorial presentation shift, independent from raw speech timestamps.
    #[serde(default)]
    pub offset_ms: i64,
    /// Translation is editorial text, never word-aligned to another language's audio.
    #[serde(default)]
    pub translation_of: Option<Uuid>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Projection {
    pub cue: caption_core::Cue,
    pub unit_id: Uuid,
    pub style: Style,
    pub words: Vec<Word>,
    pub approved: bool,
    pub language: String,
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
fn valid_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|c| c.is_ascii_hexdigit())
}
fn validate_style(s: &Style) -> Result<(), ApiError> {
    if !crate::fonts::safe_name(&s.font)
        || !s.size.is_finite()
        || !(0.01..=0.15).contains(&s.size)
        || !s.x.is_finite()
        || !s.y.is_finite()
        || !(0. ..=1.).contains(&s.x)
        || !(0. ..=1.).contains(&s.y)
        || !s.outline_size.is_finite()
        || !(0. ..=10.).contains(&s.outline_size)
        || !["plain", "word_highlight", "reveal"].contains(&s.mode.as_str())
        || !(12..=80).contains(&s.max_chars)
        || ![&s.color, &s.highlight, &s.outline]
            .iter()
            .all(|c| valid_color(c))
    {
        return Err(bad("Stile non valido: font Windows supportato, dimensione 1–15%, posizione 0–1, colore #RRGGBB, 12–80 caratteri/riga"));
    }
    Ok(())
}
fn anchored_units(p: &Project) -> Vec<Unit> {
    let mut source = p.clone();
    source.subtitles = Document::default();
    source.timeline = Timeline::default();
    let track = source.timeline.tracks[0].id;
    for t in p
        .transcripts
        .iter()
        .filter(|t| crate::asr::active(p, t.asset_id).is_some_and(|a| a.id == t.id))
    {
        if let Some(duration) = p
            .assets
            .iter()
            .find(|a| a.id == t.asset_id)
            .and_then(|a| a.media.as_ref())
            .and_then(|m| m.duration_ms)
        {
            source.timeline.clips.push(TimelineClip {
                id: t.asset_id,
                asset_id: t.asset_id,
                track_id: track,
                start_ms: 0,
                trim_in_ms: 0,
                duration_ms: duration,
                label: None,
                volume: 1.,
                opacity: 1.,
                transform: VisualTransform::default(),
            });
        }
    }
    caption_core::foundation_cues(&source)
        .into_iter()
        .filter_map(|cue| {
            let t = crate::asr::active(p, cue.asset_id)?;
            let meta = p.transcript_metadata.get(&t.id);
            let engine = meta.and_then(|m| m["engine"].as_str()).unwrap_or("unknown");
            let aligned = meta.is_some_and(|m| m["alignment_completed"] == true);
            let words = transcript_core::words(t, &p.longform.corrections, engine, aligned)
                .into_iter()
                .filter(|w| cue.word_ids.contains(&w.id) && w.timing_unit == "word")
                .map(|w| Word {
                    id: w.id,
                    text: w.corrected_text.unwrap_or(w.raw_text),
                    source: TimeRange {
                        start_ms: w.start_ms,
                        end_ms: w.end_ms,
                    },
                })
                .collect();
            Some(Unit {
                id: cue.id,
                asset_id: cue.asset_id,
                transcript_id: t.id,
                source: cue.source,
                raw_text: cue.raw_text,
                text: cue.text,
                language: t.language.clone(),
                words,
                align_status: if cue.requires_review {
                    "review"
                } else {
                    "aligned"
                }
                .into(),
                approved: false,
                style: None,
                alignment: meta.cloned(),
                offset_ms: 0,
                translation_of: None,
            })
        })
        .collect()
}
pub fn projections(p: &Project, approved_only: bool) -> Vec<Projection> {
    let mut result = vec![];
    let timeline_end = p
        .timeline
        .clips
        .iter()
        .map(|c| c.start_ms + c.duration_ms)
        .max()
        .unwrap_or(0);
    for clip in &p.timeline.clips {
        if !p
            .timeline
            .tracks
            .iter()
            .any(|t| t.id == clip.track_id && t.visible)
        {
            continue;
        }
        let source = TimeRange {
            start_ms: clip.trim_in_ms,
            end_ms: clip.trim_in_ms + clip.duration_ms,
        };
        for unit in p.subtitles.units.iter().filter(|u| {
            u.asset_id == clip.asset_id
                && u.language == p.subtitles.language
                && (!approved_only || u.approved)
        }) {
            if !unit.transcript_id.is_nil()
                && !p
                    .transcripts
                    .iter()
                    .any(|t| t.id == unit.transcript_id && t.asset_id == unit.asset_id)
            {
                continue;
            }
            let Some(overlap) = source.intersection(unit.source) else {
                continue;
            };
            let clipped =
                overlap.start_ms != unit.source.start_ms || overlap.end_ms != unit.source.end_ms;
            let matches_words = unit
                .words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
                .split_whitespace()
                .collect::<Vec<_>>()
                == unit.text.split_whitespace().collect::<Vec<_>>();
            if clipped && (!matches_words || unit.words.is_empty()) && approved_only {
                continue;
            }
            let selected = unit
                .words
                .iter()
                .filter_map(|w| {
                    w.source.intersection(overlap).map(|r| Word {
                        id: w.id,
                        text: w.text.clone(),
                        source: TimeRange {
                            start_ms: (clip.start_ms + r.start_ms - source.start_ms)
                                .saturating_add_signed(unit.offset_ms)
                                .min(timeline_end),
                            end_ms: (clip.start_ms + r.end_ms - source.start_ms)
                                .saturating_add_signed(unit.offset_ms)
                                .min(timeline_end),
                        },
                    })
                })
                .filter(|w| w.source.end_ms > w.source.start_ms)
                .collect::<Vec<_>>();
            let text = if clipped && !unit.words.is_empty() && matches_words {
                selected
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                unit.text.clone()
            };
            let from = (clip.start_ms + overlap.start_ms - source.start_ms)
                .saturating_add_signed(unit.offset_ms)
                .min(timeline_end);
            let to = (clip.start_ms + overlap.end_ms - source.start_ms)
                .saturating_add_signed(unit.offset_ms)
                .min(timeline_end);
            if text.trim().is_empty() || to <= from {
                continue;
            }
            result.push(Projection {
                cue: caption_core::Cue {
                    id: Uuid::new_v5(&clip.id, unit.id.as_bytes()),
                    clip_id: clip.id,
                    asset_id: unit.asset_id,
                    source: overlap,
                    timeline: TimeRange {
                        start_ms: from,
                        end_ms: to,
                    },
                    word_ids: selected.iter().map(|w| w.id).collect(),
                    text,
                    raw_text: unit.raw_text.clone(),
                    requires_review: !["aligned", "timing_reviewed"]
                        .contains(&unit.align_status.as_str())
                        || unit.words.iter().any(|w| {
                            w.source.intersection(overlap).is_some_and(|r| {
                                r.start_ms != w.source.start_ms || r.end_ms != w.source.end_ms
                            })
                        }),
                    timing_unit: if unit.words.is_empty() {
                        "segment"
                    } else {
                        "word"
                    }
                    .into(),
                },
                unit_id: unit.id,
                style: unit
                    .style
                    .clone()
                    .unwrap_or_else(|| p.subtitles.style.clone()),
                words: selected,
                approved: unit.approved,
                language: unit.language.clone(),
            });
        }
    }
    result.sort_by_key(|p| (p.cue.timeline.start_ms, p.cue.clip_id));
    result
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
}
fn check(p: &Project, at: chrono::DateTime<chrono::Utc>) -> Result<(), ApiError> {
    if p.updated_at != at {
        Err(ApiError(
            StatusCode::CONFLICT,
            "Progetto modificato: ricarica i sottotitoli".into(),
        ))
    } else {
        Ok(())
    }
}
pub async fn sync(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Revision>,
) -> Result<Json<Project>, ApiError> {
    update_project(&state, id, |p| {
        check(p, req.expected_updated_at)?;
        let before = p.subtitles.clone();
        let source = anchored_units(p);
        if source.is_empty() {
            return Err(bad("Trascrivi il media prima di creare i sottotitoli"));
        }
        let ids = source.iter().map(|u| u.id).collect::<HashSet<_>>();
        let existing = p
            .subtitles
            .units
            .iter()
            .map(|u| u.id)
            .collect::<HashSet<_>>();
        p.subtitles
            .units
            .extend(source.into_iter().filter(|u| !existing.contains(&u.id)));
        if before.units.is_empty()
            && !p
                .subtitles
                .units
                .iter()
                .any(|u| u.language == p.subtitles.language)
        {
            if let Some(first) = p.subtitles.units.first() {
                p.subtitles.language = first.language.clone();
            }
        }
        for unit in &mut p.subtitles.units {
            if !unit.transcript_id.is_nil()
                && !ids.contains(&unit.id)
                && !p.transcripts.iter().any(|t| t.id == unit.transcript_id)
            {
                unit.approved = false;
                unit.align_status = "stale_transcript".into();
            }
        }
        p.subtitles.revision += 1;
        p.subtitle_history.record(&before, &p.subtitles);
        Ok(())
    })
    .await?;
    Ok(Json(project(&state, id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Save {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub document: Document,
}
pub async fn save(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Save>,
) -> Result<Json<Project>, ApiError> {
    update_project(&state, id, |p| {
        check(p, req.expected_updated_at)?;
        let mut d = req.document;
        if d.schema_version != 1
            || d.revision != p.subtitles.revision
            || d.units.len() > 30000
            || !["it", "en", "es", "unknown", "auto"].contains(&d.language.as_str())
        {
            return Err(bad("Documento sottotitoli non valido o revisione obsoleta"));
        }
        validate_style(&d.style)?;
        let mut ids = HashSet::new();
        for unit in &mut d.units {
            if !ids.insert(unit.id)
                || unit.text.len() > 4000
                || unit.text.trim().is_empty()
                || !(-10000..=10000).contains(&unit.offset_ms)
            {
                return Err(bad("Cue duplicato, vuoto o troppo lungo"));
            }
            let old = p
                .subtitles
                .units
                .iter()
                .find(|u| u.id == unit.id)
                .ok_or_else(|| bad("Nuovi cue: usa Split/Merge o Sincronizza dal parlato"))?;
            if unit.asset_id != old.asset_id
                || unit.transcript_id != old.transcript_id
                || unit.language != old.language
            {
                return Err(bad("Provenienza cue alterata"));
            }
            TimeRange::new(unit.source.start_ms, unit.source.end_ms).map_err(bad)?;
            let duration = p
                .assets
                .iter()
                .find(|a| a.id == unit.asset_id)
                .and_then(|a| a.media.as_ref())
                .and_then(|m| m.duration_ms)
                .unwrap_or(0);
            if unit.source.end_ms > duration || unit.source.duration() > 60000 {
                return Err(bad("Cue oltre il media o superiore a 60 secondi"));
            }
            if let Some(style) = &unit.style {
                validate_style(style)?;
            }
            unit.raw_text = old.raw_text.clone();
            unit.words = old.words.clone();
            unit.align_status = old.align_status.clone();
            unit.alignment = old.alignment.clone();
            unit.translation_of = old.translation_of;
            if unit.text != old.text {
                unit.align_status = "edited_needs_alignment".into();
            }
            if unit.source.start_ms != old.source.start_ms
                || unit.source.end_ms != old.source.end_ms
            {
                unit.align_status = "timing_edited".into();
            }
            if unit.approved
                && !unit.transcript_id.is_nil()
                && !p.transcripts.iter().any(|t| t.id == unit.transcript_id)
            {
                return Err(bad(
                    "Trascrizione sostituita: sincronizza e revisiona i nuovi cue",
                ));
            }
        }
        d.revision += 1;
        p.subtitle_history.record(&p.subtitles, &d);
        p.subtitles = d;
        Ok(())
    })
    .await?;
    Ok(Json(project(&state, id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub operation: String,
    pub unit_id: Uuid,
    pub at_source_ms: Option<u64>,
    pub next_unit_id: Option<Uuid>,
    pub target_language: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticRef {
    pub transcript_id: Uuid,
    pub index: usize,
}
fn diagnostic_review(
    p: &Project,
    asset: Uuid,
    reference: &DiagnosticRef,
) -> Result<(String, Value), ApiError> {
    if !p
        .transcripts
        .iter()
        .any(|t| t.id == reference.transcript_id && t.asset_id == asset)
    {
        return Err(bad(
            "Diagnostico riferito a una trascrizione estranea al media",
        ));
    }
    let d = p
        .transcript_metadata
        .get(&reference.transcript_id)
        .and_then(|m| m["alignment_diagnostics"]["unplaced_words"].as_array())
        .and_then(|v| v.get(reference.index))
        .ok_or_else(|| bad("Parola diagnostica non disponibile: ricarica il media"))?;
    let raw = d["text"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("Diagnostico senza testo raw"))?;
    if p.subtitles.units.iter().any(|u| {
        u.alignment.as_ref().is_some_and(|m| {
            m["diagnostic_review"]["transcript_id"] == reference.transcript_id.to_string()
                && m["diagnostic_review"]["index"].as_u64() == Some(reference.index as u64)
        })
    }) {
        return Err(bad(
            "Esiste già un cue correttivo per questa parola: modificalo in Subtitle Studio",
        ));
    }
    Ok((
        raw.to_owned(),
        json!({"diagnostic_review":{"transcript_id":reference.transcript_id,"index":reference.index,"original_diagnostic":d},"timing_provenance":"user_selected_cue_interval_not_forced_alignment","word_timing_verified":false}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manual {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub asset_id: Uuid,
    pub source: TimeRange,
    pub text: String,
    pub language: String,
    pub diagnostic: Option<DiagnosticRef>,
}
pub async fn manual(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Manual>,
) -> Result<Json<Project>, ApiError> {
    TimeRange::new(req.source.start_ms, req.source.end_ms).map_err(bad)?;
    if req.source.duration() > 60000
        || req.text.trim().is_empty()
        || req.text.len() > 4000
        || !["it", "en", "es"].contains(&req.language.as_str())
            && !(req.diagnostic.is_some() && req.language == "unknown")
    {
        return Err(bad(
            "Cue manuale: testo non vuoto, IT/EN/ES, massimo 60 secondi",
        ));
    }
    update_project(&state, id, |p| {
        check(p, req.expected_updated_at)?;
        let duration = p
            .assets
            .iter()
            .find(|a| a.id == req.asset_id && matches!(a.kind, AssetKind::Video | AssetKind::Audio))
            .and_then(|a| a.media.as_ref())
            .and_then(|m| m.duration_ms)
            .ok_or_else(|| bad("Scegli audio/video del progetto"))?;
        if req.source.end_ms > duration || p.subtitles.units.len() >= 30000 {
            return Err(bad("Cue fuori media o limite documento raggiunto"));
        }
        let (raw_text, alignment) = if let Some(reference) = &req.diagnostic {
            let (raw, meta) = diagnostic_review(p, req.asset_id, reference)?;
            (raw, Some(meta))
        } else {
            (String::new(), None)
        };
        let before = p.subtitles.clone();
        p.subtitles.units.push(Unit {
            id: Uuid::new_v4(),
            asset_id: req.asset_id,
            transcript_id: Uuid::nil(),
            source: req.source,
            raw_text,
            text: req.text,
            language: req.language.clone(),
            words: vec![],
            align_status: "manual_no_word_timing".into(),
            approved: false,
            style: None,
            alignment,
            offset_ms: 0,
            translation_of: None,
        });
        p.subtitles.language = req.language;
        p.subtitles.revision += 1;
        p.subtitle_history.record(&before, &p.subtitles);
        Ok(())
    })
    .await?;
    Ok(Json(project(&state, id).await?))
}
pub async fn structure(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Structure>,
) -> Result<Json<Project>, ApiError> {
    update_project(&state, id, |p| {
        check(p, req.expected_updated_at)?;
        if ["undo", "redo"].contains(&req.operation.as_str()) {
            p.subtitle_history.step(&mut p.subtitles, req.operation == "redo").map_err(bad)?;
            return Ok(());
        }
        let before = p.subtitles.clone();
        let index = p
            .subtitles
            .units
            .iter()
            .position(|u| u.id == req.unit_id)
            .ok_or_else(|| bad("Cue assente"))?;
        let original = p.subtitles.units[index].clone();
        if req.operation == "duplicate_language" {
            let language = req.target_language.as_deref().filter(|l| ["it", "en", "es"].contains(l) && *l != original.language).ok_or_else(|| bad("Scegli una lingua diversa: IT, EN o ES"))?;
            let parent = original.translation_of.unwrap_or(original.id);
            let mut translated = original.clone();
            translated.id = Uuid::new_v5(&parent, format!("translation-v1:{language}").as_bytes());
            if p.subtitles.units.iter().any(|u| u.id == translated.id) { return Err(bad("Livello tradotto del cue già presente")); }
            translated.language = language.into();
            translated.translation_of = Some(parent);
            translated.words.clear();
            translated.alignment = None;
            translated.align_status = "translation_draft".into();
            translated.approved = false;
            p.subtitles.units.push(translated);
            p.subtitles.language = language.into();
        } else if req.operation == "review_timing" {
            let normalized=original.words.iter().map(|w|w.text.as_str()).collect::<Vec<_>>().join(" ");
            if original.words.is_empty()||normalized.split_whitespace().collect::<Vec<_>>()!=original.text.split_whitespace().collect::<Vec<_>>(){return Err(bad("Il testo è cambiato: serve un nuovo forced alignment, non una conferma dei vecchi tempi"));}
            p.subtitles.units[index].align_status="timing_reviewed".into();p.subtitles.units[index].approved=false;
        }else if req.operation == "split" {
            let at = req
                .at_source_ms
                .ok_or_else(|| bad("Indica il punto sorgente di split"))?;
            if at <= original.source.start_ms || at >= original.source.end_ms {
                return Err(bad("Split fuori dal cue"));
            }
            if !["aligned","timing_reviewed"].contains(&original.align_status.as_str()) || original.words.len() < 2 {
                return Err(bad(
                    "Lo split per parola richiede timing allineati: riallinea il testo modificato",
                ));
            }
            let (left, right): (Vec<_>, Vec<_>) = original
                .words
                .iter()
                .cloned()
                .partition(|w| w.source.start_ms < at);
            if left.is_empty() || right.is_empty() {
                return Err(bad("Split richiede almeno una parola per lato"));
            }
            let mut a = original.clone();
            let mut b = original.clone();
            a.source.end_ms = left.last().unwrap().source.end_ms;
            a.words = left;
            a.text = a
                .words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            a.raw_text = raw_for_range(p, &a);
            a.approved = false;
            b.id = Uuid::new_v4();
            b.source.start_ms = right[0].source.start_ms;
            b.words = right;
            b.text = b
                .words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            b.raw_text = raw_for_range(p, &b);
            b.approved = false;
            p.subtitles.units[index] = a;
            p.subtitles.units.insert(index + 1, b);
        } else if req.operation == "merge" {
            let next = req
                .next_unit_id
                .ok_or_else(|| bad("Scegli il secondo cue"))?;
            let j = p
                .subtitles
                .units
                .iter()
                .position(|u| u.id == next && u.id != original.id)
                .ok_or_else(|| bad("Secondo cue assente"))?;
            let second = p.subtitles.units[j].clone();
            if second.asset_id != original.asset_id
                || second.transcript_id != original.transcript_id
                || second.language != original.language
                || second.offset_ms != original.offset_ms
                || second.source.start_ms < original.source.end_ms
                || second.source.end_ms - original.source.start_ms > 60000
            {
                return Err(bad(
                    "Unisci cue ordinati dello stesso media/lingua entro 60 secondi",
                ));
            }
            let mut merged = original;
            merged.source.end_ms = second.source.end_ms;
            merged.text.push(' ');
            merged.text.push_str(&second.text);
            merged.raw_text.push(' ');
            merged.raw_text.push_str(&second.raw_text);
            merged.words.extend(second.words);
            merged.approved = false;
            if !["aligned", "timing_reviewed"].contains(&merged.align_status.as_str()) || !["aligned", "timing_reviewed"].contains(&second.align_status.as_str()) {
                merged.align_status = "review".into();
            } else if merged.align_status != "aligned" || second.align_status != "aligned" {
                merged.align_status = "timing_reviewed".into();
            }
            p.subtitles.units[index] = merged;
            p.subtitles.units.remove(j);
        } else {
            return Err(bad("Operazione cue: split o merge"));
        }
        p.subtitles.revision += 1;
        p.subtitle_history.record(&before, &p.subtitles);
        Ok(())
    })
    .await?;
    Ok(Json(project(&state, id).await?))
}
fn raw_for_range(p: &Project, unit: &Unit) -> String {
    p.transcripts
        .iter()
        .find(|t| t.id == unit.transcript_id)
        .map(|t| {
            transcript_core::words(t, &Default::default(), "raw", false)
                .into_iter()
                .filter(|w| w.start_ms < unit.source.end_ms && w.end_ms > unit.source.start_ms)
                .map(|w| w.raw_text)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_else(|| unit.raw_text.clone())
}
fn ass_color(hex: &str) -> String {
    // Internal override colors share the motion IR/coverage-mask convention.
    // Eight-digit style colors remain in the ASS header, not inline overrides.
    format!("&H{}{}{}&", &hex[5..7], &hex[3..5], &hex[1..3])
}
fn ass_time(ms: u64) -> String {
    let cs = (ms + 5) / 10;
    format!(
        "{}:{:02}:{:02}.{:02}",
        cs / 360000,
        (cs / 6000) % 60,
        (cs / 100) % 60,
        cs % 100
    )
}
fn escape(text: &str) -> String {
    text.replace('\\', "＼")
        .replace('{', "｛")
        .replace('}', "｝")
        .replace('\n', "\\N")
        .replace('\r', "")
}
fn wrap(text: &str, max: u32) -> String {
    let mut output = String::new();
    for line in text.lines() {
        if !output.is_empty() {
            output.push_str("\\N");
        }
        let mut chars = 0;
        for word in line.split_whitespace() {
            let n = word.chars().count();
            if chars > 0 {
                if chars + n + 1 > max as usize {
                    output.push_str("\\N");
                    chars = 0;
                } else {
                    output.push(' ');
                    chars += 1;
                }
            }
            output.push_str(&escape(word));
            chars += n;
        }
    }
    output
}
pub fn ass(p: &Project, size: (u32, u32), approved_only: bool) -> String {
    let (width, height) = size;
    let mut output=format!("[Script Info]\nScriptType: v4.00+\nYCbCr Matrix: None\nPlayResX: {width}\nPlayResY: {height}\nScaledBorderAndShadow: yes\nWrapStyle: 2\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,48,&H00FFFFFF,&H00FFFFFF,&H00101018,&H00000000,0,0,0,0,100,100,0,0,1,3,0,5,0,0,0,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    for projected in projections(p, approved_only) {
        let c = &projected.cue;
        let s = &projected.style;
        let prefix = format!(
            "{{\\an5\\pos({:.2},{:.2})\\fn{}\\fs{:.2}\\c{}\\3c{}\\bord{:.2}}}",
            s.x * width as f64,
            s.y * height as f64,
            crate::fonts::resolve(&s.font),
            s.size * height as f64,
            ass_color(&s.color),
            ass_color(&s.outline),
            s.outline_size * height as f64 / 1080.
        );
        let matching = projected
            .words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            == c.text.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut points = vec![c.timeline.start_ms, c.timeline.end_ms];
        if s.mode != "plain" && !c.requires_review && matching {
            for w in &projected.words {
                points.push(w.source.start_ms.max(c.timeline.start_ms));
                points.push(w.source.end_ms.min(c.timeline.end_ms));
            }
        }
        points.sort_unstable();
        points.dedup();
        for pair in points.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            if end <= start {
                continue;
            }
            let text = if points.len() > 2 {
                let at = (start + end) / 2;
                let mut line = String::new();
                let mut chars = 0;
                for w in &projected.words {
                    if s.mode == "reveal" && w.source.start_ms > at {
                        continue;
                    }
                    let n = w.text.chars().count();
                    if chars > 0 {
                        if chars + n + 1 > s.max_chars as usize {
                            line.push_str("\\N");
                            chars = 0;
                        } else {
                            line.push(' ');
                            chars += 1;
                        }
                    }
                    if s.mode == "word_highlight" && w.source.start_ms <= at && w.source.end_ms > at
                    {
                        line.push_str(&format!(
                            "{{\\c{}}}{}{{\\c{}}}",
                            ass_color(&s.highlight),
                            escape(&w.text),
                            ass_color(&s.color)
                        ));
                    } else {
                        line.push_str(&escape(&w.text));
                    }
                    chars += n;
                }
                line
            } else {
                wrap(&c.text, s.max_chars)
            };
            if !text.trim().is_empty() {
                output.push_str(&format!(
                    "Dialogue: 1000000,{},{},Default,,0,0,0,,{prefix}{text}\n",
                    ass_time(start),
                    ass_time(end)
                ));
            }
        }
    }
    output
}
pub fn write_delivery(
    p: &Project,
    output: &FsPath,
    size: (u32, u32),
) -> anyhow::Result<Vec<String>> {
    validate_delivery(p)?;
    if p.subtitles.units.is_empty() {
        return Ok(vec![]);
    }
    let cues = projections(p, true)
        .into_iter()
        .map(|p| p.cue)
        .collect::<Vec<_>>();
    let mut paths = vec![];
    for format in ["srt", "vtt"] {
        let path = output.with_extension(format);
        std::fs::write(
            &path,
            caption_core::sidecar(&cues, format).map_err(|e| anyhow::anyhow!(e.1))?,
        )?;
        paths.push(path.to_string_lossy().to_string());
    }
    let path = output.with_extension("ass");
    std::fs::write(&path, ass(p, size, true))?;
    paths.push(path.to_string_lossy().to_string());
    let path = output.with_extension("captions.json");
    let projected = projections(p, true);
    let included = projected.iter().map(|c| c.unit_id).collect::<HashSet<_>>();
    let mut delivered_document = p.subtitles.clone();
    delivered_document
        .units
        .retain(|u| included.contains(&u.id));
    store::write_json(
        &path,
        &json!({"schema_version":1,"document":delivered_document,"projection":projected,"project_updated_at":p.updated_at}),
    )?;
    paths.push(path.to_string_lossy().to_string());
    Ok(paths)
}
pub fn validate_delivery(p: &Project) -> anyhow::Result<()> {
    for clip in &p.timeline.clips {
        if !p
            .timeline
            .tracks
            .iter()
            .any(|t| t.id == clip.track_id && t.visible)
        {
            continue;
        }
        let source = TimeRange {
            start_ms: clip.trim_in_ms,
            end_ms: clip.trim_in_ms + clip.duration_ms,
        };
        for u in p.subtitles.units.iter().filter(|u| {
            u.asset_id == clip.asset_id && u.approved && u.language == p.subtitles.language
        }) {
            let Some(overlap) = source.intersection(u.source) else {
                continue;
            };
            if overlap.start_ms == u.source.start_ms && overlap.end_ms == u.source.end_ms {
                continue;
            }
            let matches = u
                .words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
                .split_whitespace()
                .collect::<Vec<_>>()
                == u.text.split_whitespace().collect::<Vec<_>>();
            anyhow::ensure!(!u.words.is_empty()&&matches,"Cue {} tagliato parzialmente con testo privo di timing coerenti: riallinea o revisiona prima dell'export",u.id);
        }
    }
    Ok(())
}
pub async fn view(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let p = project(&state, id).await?;
    Ok(Json(
        json!({"schema_version":1,"project_updated_at":p.updated_at,"document":p.subtitles,"projection":projections(&p,false),"fonts":"Font del PC; fallback esplicito Arial se assente, file non distribuiti. Verifica l'anteprima nativa.","font_missing":!crate::fonts::available(&p.subtitles.style.font),"renderer":"libass"}),
    ))
}
pub async fn export(
    State(state): State<AppState>,
    Path((id, format)): Path<(Uuid, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let p = project(&state, id).await?;
    let text = match format.as_str() {
        "ass" => ass(
            &p,
            compositor::dimensions(&p.aspect_ratio).map_err(bad)?,
            true,
        ),
        "json" => serde_json::to_string_pretty(
            &json!({"document":p.subtitles,"projection":projections(&p,true)}),
        )
        .map_err(bad)?,
        "srt" | "vtt" => caption_core::sidecar(
            &projections(&p, true)
                .into_iter()
                .map(|p| p.cue)
                .collect::<Vec<_>>(),
            &format,
        )?,
        _ => return Err(bad("Formati sottotitoli: srt, vtt, ass, json")),
    };
    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=montaggio.{format}"),
            ),
        ],
        text,
    ))
}
#[derive(Deserialize)]
pub struct Frame {
    pub at_ms: u64,
}
static FRAME_GATE: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
pub async fn frame(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<Frame>,
) -> Result<impl IntoResponse, ApiError> {
    let mut p = project(&state, id).await?;
    if q.at_ms > 86400000 {
        return Err(bad("Tempo frame fuori dai limiti"));
    }
    let size = compositor::dimensions(&p.aspect_ratio).map_err(bad)?;
    let last_clip = p
        .timeline
        .clips
        .iter()
        .max_by_key(|c| c.start_ms + c.duration_ms)
        .map(|c| c.id);
    p.timeline.clips.retain(|c| {
        Some(c.id) == last_clip
            || (c.start_ms.saturating_sub(10000) <= q.at_ms
                && c.start_ms + c.duration_ms + 10000 > q.at_ms)
    });
    let active = projections(&p, true)
        .into_iter()
        .filter(|c| c.cue.timeline.start_ms <= q.at_ms && c.cue.timeline.end_ms > q.at_ms)
        .collect::<Vec<_>>();
    let units = active.iter().map(|c| c.unit_id).collect::<HashSet<_>>();
    p.subtitles.units.retain(|u| units.contains(&u.id));
    let key = Uuid::new_v5(
        &id,
        format!(
            "subtitle-png-v6:{}:{:?}:{}",
            p.updated_at,
            active
                .iter()
                .map(|c| (
                    c.unit_id,
                    c.cue.clip_id,
                    c.words
                        .iter()
                        .position(|w| w.source.start_ms <= q.at_ms && w.source.end_ms > q.at_ms)
                ))
                .collect::<Vec<_>>(),
            crate::fonts::catalog().families.join("|")
        )
        .as_bytes(),
    );
    let root = store::project_dir(&state.data_dir, id).join(".cache/previews/subtitles");
    let image = root.join(format!("{key}.png"));
    let _permit = FRAME_GATE
        .get_or_init(|| tokio::sync::Semaphore::new(1))
        .acquire()
        .await
        .expect("preview gate");
    if !image.is_file() {
        let image = image.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            std::fs::create_dir_all(&root)?;
            let script = root.join(format!("{key}.ass"));
            let mask = root.join(format!("{key}-mask.ass"));
            let body = ass(&p, size, true);
            std::fs::write(&mask, crate::alpha_render::mask(&body))?;
            std::fs::write(&script, body)?;
            // Use a relative safe UUID filename, avoiding Windows filter path escaping.
            let graph = crate::alpha_render::filter(
                &format!("{key}.ass"),
                &format!("{key}-mask.ass"),
                size,
                "1",
                1.,
                q.at_ms as f64 / 1000.,
            );
            let result = std::process::Command::new("ffmpeg")
                .current_dir(&root)
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-n",
                    "-filter_complex_threads",
                    "1",
                    "-f",
                    "lavfi",
                    "-i",
                    &graph,
                    "-frames:v",
                    "1",
                    "-fps_mode",
                    "passthrough",
                    "-threads",
                    "1",
                    "-pix_fmt",
                    "rgba",
                ])
                .arg(&image)
                .output()?;
            anyhow::ensure!(
                result.status.success(),
                "libass preview: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            let _ = std::fs::remove_file(script);
            let _ = std::fs::remove_file(mask);
            Ok(())
        })
        .await
        .map_err(bad)?
        .map_err(bad)?;
    }
    let bytes = tokio::fs::read(image).await.map_err(bad)?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        bytes,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inline_caption_color_uses_the_coverage_mask_convention() {
        let color = ass_color("#E5469C");
        assert_eq!(color, "&H9C46E5&");
        assert_eq!(
            crate::alpha_render::mask(&format!("{{\\c{color}}}Voce")),
            "{\\c&HFFFFFF&}Voce"
        );
    }
    #[test]
    fn source_captions_follow_cuts_and_never_leak_excluded_editorial_words() {
        let mut p = Project::new("Conformità tagli".into(), None);
        let aid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        let uid = Uuid::new_v4();
        p.transcripts.push(Transcript {
            id: tid,
            asset_id: aid,
            language: "it".into(),
            segments: vec![],
            words: vec![],
            imported_at: chrono::Utc::now(),
        });
        let words = vec![
            Word {
                id: Uuid::new_v4(),
                text: "Caffè".into(),
                source: TimeRange {
                    start_ms: 1000,
                    end_ms: 2000,
                },
            },
            Word {
                id: Uuid::new_v4(),
                text: "sì.".into(),
                source: TimeRange {
                    start_ms: 3000,
                    end_ms: 4000,
                },
            },
        ];
        p.subtitles.units.push(Unit {
            id: uid,
            asset_id: aid,
            transcript_id: tid,
            source: TimeRange {
                start_ms: 1000,
                end_ms: 4000,
            },
            raw_text: "Caffè sì.".into(),
            text: "Caffè sì.".into(),
            language: "it".into(),
            words,
            align_status: "aligned".into(),
            approved: true,
            style: None,
            alignment: None,
            offset_ms: 0,
            translation_of: None,
        });
        for (start, trim) in [(0, 1000), (1000, 3000)] {
            p.timeline.clips.push(TimelineClip {
                id: Uuid::new_v4(),
                asset_id: aid,
                track_id: p.timeline.tracks[0].id,
                start_ms: start,
                trim_in_ms: trim,
                duration_ms: 1000,
                label: None,
                volume: 1.,
                opacity: 1.,
                transform: VisualTransform::default(),
            });
        }
        let projected = projections(&p, true);
        assert_eq!(projected.len(), 2);
        assert_eq!(projected[0].cue.text, "Caffè");
        assert_eq!(projected[1].cue.text, "sì.");
        assert_eq!(projected[1].words[0].source.start_ms, 1000);
        assert!(projected.iter().all(|p| !p.cue.requires_review));
        assert!(validate_delivery(&p).is_ok());
        p.subtitles.units[0].text = "Una riscrittura senza timing".into();
        p.subtitles.units[0].align_status = "edited_needs_alignment".into();
        assert!(validate_delivery(&p).is_err());
        assert!(
            projections(&p, true).is_empty(),
            "Never reinsert a full rewritten sentence over a retained fragment"
        );
    }
    #[test]
    fn diagnostic_review_keeps_raw_and_rejects_foreign_reference() {
        let mut p = Project::new("Review".into(), None);
        let tid = Uuid::new_v4();
        let aid = Uuid::new_v4();
        p.transcripts.push(Transcript {
            id: tid,
            asset_id: aid,
            language: "unknown".into(),
            segments: vec![],
            words: vec![],
            imported_at: chrono::Utc::now(),
        });
        p.transcript_metadata.insert(tid,json!({"alignment_diagnostics":{"unplaced_words":[{"text":"leggenda.","at_ms":7840,"chunk_index":0}]}}));
        let reference = DiagnosticRef {
            transcript_id: tid,
            index: 0,
        };
        let (raw, meta) = diagnostic_review(&p, aid, &reference).unwrap();
        assert_eq!(raw, "leggenda.");
        assert_eq!(meta["word_timing_verified"], false);
        assert_eq!(
            meta["timing_provenance"],
            "user_selected_cue_interval_not_forced_alignment"
        );
        assert!(meta.get("start_ms").is_none());
        assert!(p.transcripts[0].words.is_empty());
        assert!(diagnostic_review(&p, Uuid::new_v4(), &reference).is_err());
        assert!(diagnostic_review(
            &p,
            aid,
            &DiagnosticRef {
                transcript_id: tid,
                index: 99
            }
        )
        .is_err());
    }
    #[test]
    fn ass_escaping_and_style_limits() {
        assert_eq!(escape("Caffè {x}\\n\n"), "Caffè ｛x｝＼n\\N");
        assert_eq!(ass_time(1234), "0:00:01.23");
        assert!(validate_style(&Style::default()).is_ok());
        let style = Style {
            color: "red;evil".into(),
            ..Style::default()
        };
        assert!(validate_style(&style).is_err());
        assert_eq!(
            wrap("Una prima seconda terza riga", 12),
            "Una prima\\Nseconda\\Nterza riga"
        );
    }
}
