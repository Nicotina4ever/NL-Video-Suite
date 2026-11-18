//! Reviewable candidates, transparent heuristics, common NLE and serial batch.
//! No new transcription engine and no claim that heuristic scores predict views.
use crate::{
    compositor, editor, longform, model::*, store, time_map::TimeRange, update_project, ApiError,
    AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ShortsState {
    pub revision: u64,
    pub candidates: Vec<Candidate>,
    pub weights: BTreeMap<String, u32>,
}
impl Default for ShortsState {
    fn default() -> Self {
        Self {
            revision: 0,
            candidates: vec![],
            weights: default_weights(),
        }
    }
}
fn default_weights() -> BTreeMap<String, u32> {
    [
        "hook_indizio_testuale",
        "autonomia_contesto",
        "chiusura_frase",
        "ritmo_parole",
        "leggibilita_stimata",
        "durata",
    ]
    .into_iter()
    .map(|k| (k.into(), 1))
    .collect()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub transcript_id: Uuid,
    pub title: String,
    pub ranges: Vec<TimeRange>,
    pub evidence: String,
    pub producer: String,
    pub approved: bool,
    pub transform: VisualTransform,
    pub scores: BTreeMap<String, u32>,
    pub score: u32,
    pub warnings: Vec<String>,
    pub duplicate_of: Option<Uuid>,
    pub reframe: Option<Value>,
    #[serde(default)]
    pub category: String,
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub asset_id: Uuid,
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub analysis_job_id: Option<Uuid>,
    pub min_seconds: Option<u64>,
    pub max_seconds: Option<u64>,
}
fn revision(p: &Project, at: chrono::DateTime<chrono::Utc>) -> Result<(), ApiError> {
    if p.updated_at != at {
        Err(ApiError(
            StatusCode::CONFLICT,
            "Il progetto è cambiato: ricarica prima di continuare".into(),
        ))
    } else {
        Ok(())
    }
}
fn normalise_ranges(
    p: &Project,
    asset: Uuid,
    ranges: &[TimeRange],
) -> Result<Vec<TimeRange>, ApiError> {
    let keeps = longform::kept_ranges(p, asset)?;
    let mut result = vec![];
    for r in ranges {
        TimeRange::new(r.start_ms, r.end_ms).map_err(bad)?;
        for keep in &keeps {
            if let Some(overlap) = r.intersection(*keep) {
                result.push(overlap);
            }
        }
    }
    result.sort_by_key(|r| r.start_ms);
    let mut merged: Vec<TimeRange> = vec![];
    for r in result {
        if let Some(last) = merged.last_mut() {
            if r.start_ms <= last.end_ms {
                last.end_ms = last.end_ms.max(r.end_ms);
                continue;
            }
        }
        merged.push(r);
    }
    if merged.is_empty() {
        return Err(bad("Il candidato ricade interamente in materiale escluso"));
    }
    Ok(merged)
}
fn text_for(t: &Transcript, ranges: &[TimeRange]) -> String {
    let words = if t.words.is_empty() {
        &t.segments
    } else {
        &t.words
    };
    words
        .iter()
        .filter(|w| {
            ranges
                .iter()
                .any(|r| w.start_ms < r.end_ms && w.end_ms > r.start_ms)
        })
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}
fn token_set(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() > 2)
        .map(str::to_owned)
        .collect()
}
fn score(c: &mut Candidate, weights: &BTreeMap<String, u32>) {
    let duration = c.ranges.iter().map(|r| r.duration()).sum::<u64>() as f64 / 1000.;
    let words = c.evidence.split_whitespace().count();
    let rate = words as f64 / duration.max(0.1);
    let first = c
        .evidence
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();
    let context = !matches!(
        first.as_str(),
        "questo" | "quello" | "quindi" | "perciò" | "infatti" | "lui" | "lei" | "esso" | "e" | "ma"
    );
    let end = c.evidence.trim_end().ends_with(['.', '!', '?', '…']);
    c.scores = BTreeMap::from([
        (
            "hook_indizio_testuale".into(),
            if c.evidence.chars().take(140).any(|v| v == '?') {
                85
            } else {
                55
            },
        ),
        ("autonomia_contesto".into(), if context { 75 } else { 30 }),
        ("chiusura_frase".into(), if end { 90 } else { 35 }),
        (
            "ritmo_parole".into(),
            if (1.2..=3.5).contains(&rate) { 85 } else { 45 },
        ),
        (
            "leggibilita_stimata".into(),
            if rate <= 3.2 { 85 } else { 35 },
        ),
        (
            "durata".into(),
            if (12. ..=60.).contains(&duration) {
                90
            } else {
                40
            },
        ),
    ]);
    let total = c
        .scores
        .keys()
        .map(|k| weights.get(k).copied().unwrap_or(1))
        .sum::<u32>()
        .max(1);
    c.score = c
        .scores
        .iter()
        .map(|(k, v)| v * weights.get(k).copied().unwrap_or(1))
        .sum::<u32>()
        / total;
    c.warnings = vec![
        "Score euristico, non giudizio creativo né previsione di visualizzazioni.".into(),
        "Musica, licenze e completezza del contesto richiedono revisione umana.".into(),
    ];
    if !context {
        c.warnings
            .push("Apertura forse dipendente da un preambolo: ascolta il contesto.".into());
    }
    if !end {
        c.warnings
            .push("Chiusura senza punteggiatura finale: verifica il payoff e il taglio.".into());
    }
    if rate > 3.2 {
        c.warnings
            .push("Parlato rapido: controlla leggibilità dei sottotitoli.".into());
    }
}
fn dedup(candidates: &mut [Candidate]) {
    for i in 0..candidates.len() {
        candidates[i].duplicate_of = None;
        for j in 0..i {
            if candidates[i].asset_id != candidates[j].asset_id {
                continue;
            }
            let a = &candidates[i];
            let b = &candidates[j];
            let overlap = a
                .ranges
                .iter()
                .flat_map(|r| b.ranges.iter().filter_map(move |s| r.intersection(*s)))
                .map(|r| r.duration())
                .sum::<u64>();
            let shorter = a
                .ranges
                .iter()
                .map(|r| r.duration())
                .sum::<u64>()
                .min(b.ranges.iter().map(|r| r.duration()).sum::<u64>());
            let ta = token_set(&a.evidence);
            let tb = token_set(&b.evidence);
            let union = ta.union(&tb).count();
            if overlap as f64 / shorter.max(1) as f64 > 0.8
                || (ta.len() >= 8
                    && union > 0
                    && ta.intersection(&tb).count() as f64 / union as f64 > 0.92)
            {
                candidates[i].duplicate_of = Some(candidates[j].id);
                break;
            }
        }
    }
}
pub async fn view(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let p = project(&state, id).await?;
    Ok(Json(
        json!({"schema_version":1,"project_updated_at":p.updated_at,"state":p.shorts,"score_method":"heuristic-v1","auto_approval":false}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualRequest {
    pub asset_id: Uuid,
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub start_ms: u64,
    pub end_ms: u64,
    pub title: String,
}
fn is_manual(c: &Candidate) -> bool {
    c.transcript_id.is_nil() && c.producer == "manual"
}
pub async fn manual(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ManualRequest>,
) -> Result<Json<Project>, ApiError> {
    update_project(&state, id, |p| {
        revision(p, req.expected_updated_at)?;
        if req.title.len() > 1000 {
            return Err(bad("Titolo troppo lungo"));
        }
        let ranges = normalise_ranges(
            p,
            req.asset_id,
            &[TimeRange::new(req.start_ms, req.end_ms).map_err(bad)?],
        )?;
        let mut c = Candidate {
            id: Uuid::new_v4(),
            asset_id: req.asset_id,
            transcript_id: Uuid::nil(),
            title: req.title.clone(),
            ranges,
            evidence: "Intervallo scelto manualmente; nessuna trascrizione richiesta.".into(),
            producer: "manual".into(),
            approved: false,
            transform: VisualTransform {
                fit: "auto".into(),
                ..Default::default()
            },
            scores: BTreeMap::new(),
            score: 0,
            warnings: vec![],
            duplicate_of: None,
            reframe: None,
            category: "momento forte".into(),
        };
        score(&mut c, &p.shorts.weights);
        p.shorts.candidates.push(c);
        p.shorts.revision += 1;
        Ok(())
    })
    .await?;
    Ok(Json(project(&state, id).await?))
}
pub async fn suggest(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Request>,
) -> Result<Json<Project>, ApiError> {
    let p = project(&state, id).await?;
    revision(&p, req.expected_updated_at)?;
    let t = crate::asr::active(&p, req.asset_id).ok_or_else(|| {
        bad("Trascrivi una sola volta il media da Longform, poi genera i candidati")
    })?;
    let min = req.min_seconds.unwrap_or(12) * 1000;
    let max = req.max_seconds.unwrap_or(60) * 1000;
    if !(3000..=180000).contains(&min) || max < min || max > 180000 {
        return Err(bad(
            "Durate candidati: da 3 a 180 secondi, minimo ≤ massimo",
        ));
    }
    let mut raw: Vec<(String, Vec<TimeRange>, String)> = vec![];
    if let Some(jid) = req.analysis_job_id {
        let jobs = state.jobs.read().await;
        let j = jobs
            .get(&jid)
            .filter(|j| j.project_id == Some(id) && matches!(j.status, JobStatus::Succeeded))
            .ok_or_else(|| bad("Analisi ClipsAI non completata o di altro progetto"))?;
        let result = j
            .result
            .as_ref()
            .ok_or_else(|| bad("Risultato analisi assente"))?;
        if result.get("_nl_suite_asset_id").and_then(Value::as_str)
            != Some(req.asset_id.to_string().as_str())
        {
            return Err(bad("Analisi riferita a un media diverso"));
        }
        let result = result.get("result").unwrap_or(result);
        let items = ["shorts", "reels", "clips"]
            .iter()
            .find_map(|key| result.get(*key).and_then(Value::as_array))
            .ok_or_else(|| bad("Nessun candidato nel risultato ClipsAI"))?;
        for item in items.iter().take(300) {
            let segments = item
                .get("segments")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_else(|| vec![json!([item["start"], item["end"]])]);
            let mut ranges = vec![];
            for segment in segments {
                let from = segment[0]
                    .as_f64()
                    .ok_or_else(|| bad("Inizio candidato non numerico"))?;
                let end = segment[1]
                    .as_f64()
                    .ok_or_else(|| bad("Fine candidato non numerica"))?;
                if !from.is_finite() || !end.is_finite() || from < 0. || end <= from {
                    return Err(bad("Intervallo ClipsAI non valido"));
                }
                ranges.push(
                    TimeRange::new((from * 1000.).round() as u64, (end * 1000.).round() as u64)
                        .map_err(bad)?,
                );
            }
            raw.push((
                item["title"].as_str().unwrap_or("Proposta ClipsAI").into(),
                ranges,
                "clipsai_shared_asr".into(),
            ));
        }
    } else {
        for d in p
            .longform
            .decisions
            .iter()
            .filter(|d| d.asset_id == req.asset_id && d.action == "highlight")
        {
            raw.push((d.reason.clone(), vec![d.range], "highlight".into()));
        }
        // Chapters provide grounded topic labels; no model invented quote.
        for chapter in p
            .longform
            .chapters
            .iter()
            .filter(|c| c.asset_id == req.asset_id)
        {
            let segments = t
                .segments
                .iter()
                .filter(|s| {
                    s.start_ms < chapter.source.end_ms && s.end_ms > chapter.source.start_ms
                })
                .collect::<Vec<_>>();
            let mut start = None;
            let mut end = 0;
            for segment in segments {
                let from = segment.start_ms.max(chapter.source.start_ms);
                let to = segment.end_ms.min(chapter.source.end_ms);
                if let Some(s) = start {
                    if to - s > max && end > s {
                        raw.push((
                            chapter.title.clone(),
                            vec![TimeRange {
                                start_ms: s,
                                end_ms: end,
                            }],
                            "chapter_window".into(),
                        ));
                        start = None;
                    }
                }
                if start.is_none() {
                    start = Some(from);
                }
                end = to;
                if end - start.unwrap_or(from) >= min
                    && segment.text.trim_end().ends_with(['.', '!', '?'])
                {
                    raw.push((
                        chapter.title.clone(),
                        vec![TimeRange {
                            start_ms: start.unwrap_or(from),
                            end_ms: end,
                        }],
                        "chapter_window".into(),
                    ));
                    start = None;
                }
            }
            if let Some(s) = start {
                if end > s {
                    raw.push((
                        chapter.title.clone(),
                        vec![TimeRange {
                            start_ms: s,
                            end_ms: end,
                        }],
                        "chapter_window".into(),
                    ));
                }
            }
        }
        if raw.is_empty() {
            // Grounded offline baseline, not semantic AI scoring.
            let mut start = None;
            let mut end = 0;
            for segment in &t.segments {
                if start.is_none() {
                    start = Some(segment.start_ms);
                }
                if segment.end_ms - start.unwrap_or(0) > max && end > start.unwrap_or(0) {
                    let r = TimeRange {
                        start_ms: start.unwrap_or(0),
                        end_ms: end,
                    };
                    raw.push((
                        text_for(t, &[r]).chars().take(70).collect(),
                        vec![r],
                        "offline_sentence_window".into(),
                    ));
                    start = Some(segment.start_ms);
                }
                end = segment.end_ms;
                if end - start.unwrap_or(0) >= min
                    && segment.text.trim_end().ends_with(['.', '!', '?'])
                {
                    let r = TimeRange {
                        start_ms: start.unwrap_or(0),
                        end_ms: end,
                    };
                    raw.push((
                        text_for(t, &[r]).chars().take(70).collect(),
                        vec![r],
                        "offline_sentence_window".into(),
                    ));
                    start = None;
                }
            }
            if let Some(s) = start {
                if end > s {
                    let r = TimeRange {
                        start_ms: s,
                        end_ms: end,
                    };
                    raw.push((
                        text_for(t, &[r]).chars().take(70).collect(),
                        vec![r],
                        "offline_sentence_window".into(),
                    ));
                }
            }
        }
    }
    let mut next = p
        .shorts
        .candidates
        .iter()
        .filter(|c| c.asset_id != req.asset_id || c.approved)
        .cloned()
        .collect::<Vec<_>>();
    for (title, ranges, producer) in raw.into_iter().take(300) {
        let Ok(ranges) = normalise_ranges(&p, req.asset_id, &ranges) else {
            continue;
        };
        let duration = ranges.iter().map(|r| r.duration()).sum::<u64>();
        if duration < min || duration > max {
            continue;
        }
        let cid = Uuid::new_v5(
            &req.asset_id,
            format!(
                "candidate-v1:{}",
                serde_json::to_string(&ranges).map_err(bad)?
            )
            .as_bytes(),
        );
        if next.iter().any(|c| c.id == cid) {
            continue;
        }
        let mut c = Candidate {
            id: cid,
            asset_id: req.asset_id,
            transcript_id: t.id,
            title: title.chars().take(180).collect(),
            evidence: text_for(t, &ranges),
            ranges,
            producer,
            approved: false,
            transform: VisualTransform {
                fit: "auto".into(),
                ..Default::default()
            },
            scores: BTreeMap::new(),
            score: 0,
            warnings: vec![],
            duplicate_of: None,
            reframe: None,
            category: if title.contains('?') {
                "Q&A".into()
            } else {
                "momento forte".into()
            },
        };
        score(&mut c, &p.shorts.weights);
        next.push(c);
    }
    dedup(&mut next);
    if next.len() > 1000 {
        return Err(bad("Massimo 1000 candidati per progetto"));
    }
    update_project(&state, id, |p| {
        revision(p, req.expected_updated_at)?;
        p.shorts.candidates = next;
        p.shorts.revision += 1;
        Ok(())
    })
    .await?;
    Ok(Json(project(&state, id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveRequest {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub state: ShortsState,
}
pub async fn save(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<SaveRequest>,
) -> Result<Json<Project>, ApiError> {
    update_project(&state, id, |p| {
        revision(p, req.expected_updated_at)?;
        if req.state.revision != p.shorts.revision || req.state.candidates.len() > 1000 {
            return Err(bad("Revisione shorts non valida"));
        }
        let mut ids = HashSet::new();
        let weights = &req.state.weights;
        let supported = default_weights();
        if weights.len() != supported.len()
            || weights
                .iter()
                .any(|(key, w)| !supported.contains_key(key) || *w > 10)
            || weights.values().sum::<u32>() == 0
        {
            return Err(bad(
                "Pesi score: tutti i sei fattori, valori 0–10, almeno uno positivo",
            ));
        }
        let mut next = vec![];
        for mut c in req.state.candidates {
            if !ids.insert(c.id) {
                return Err(bad("ID candidati duplicati"));
            }
            let old = p
                .shorts
                .candidates
                .iter()
                .find(|old| old.id == c.id)
                .ok_or_else(|| bad("Candidato non appartenente al progetto"))?;
            if c.asset_id != old.asset_id
                || c.transcript_id != old.transcript_id
                || c.title.len() > 1000
            {
                return Err(bad("Provenienza candidato alterata"));
            }
            c.transform.validate().map_err(bad)?;
            c.ranges = normalise_ranges(p, c.asset_id, &c.ranges)?;
            if is_manual(old) {
                c.evidence = old.evidence.clone();
            } else {
                let t = p
                    .transcripts
                    .iter()
                    .find(|t| t.id == c.transcript_id)
                    .ok_or_else(|| bad("Trascrizione cambiata: revisiona i candidati"))?;
                c.evidence = text_for(t, &c.ranges);
            }
            c.producer = old.producer.clone();
            c.reframe = old.reframe.clone();
            if ![
                "hook",
                "tutorial",
                "momento forte",
                "recap",
                "curiosità",
                "Q&A",
                "reaction",
            ]
            .contains(&c.category.as_str())
            {
                return Err(bad("Categoria editoriale non supportata"));
            }
            if !is_manual(&c) && c.ranges.iter().map(|r| r.duration()).sum::<u64>() > 180000 {
                return Err(bad("Un candidato short non può superare 180 secondi"));
            }
            score(&mut c, weights);
            next.push(c);
        }
        dedup(&mut next);
        p.shorts.candidates = next;
        p.shorts.weights = req.state.weights;
        p.shorts.revision += 1;
        Ok(())
    })
    .await?;
    Ok(Json(project(&state, id).await?))
}
fn producer(c: &Candidate) -> String {
    format!("shorts:{}", c.id)
}
pub fn assembled(
    p: &Project,
    asset: Uuid,
) -> Result<(Timeline, BTreeMap<Uuid, longform::ClipOrigin>), ApiError> {
    let candidates = p
        .shorts
        .candidates
        .iter()
        .filter(|c| c.asset_id == asset && c.approved)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(bad("Approva almeno un candidato prima del montaggio"));
    }
    let owned = p
        .clip_origins
        .iter()
        .filter(|(_, o)| o.asset_id == asset && o.producer.starts_with("shorts:"))
        .map(|(id, _)| *id)
        .collect::<HashSet<_>>();
    if p.timeline.clips.iter().any(|c| {
        owned.contains(&c.id)
            && p.timeline
                .tracks
                .iter()
                .any(|t| t.id == c.track_id && t.locked)
    }) {
        return Err(bad("Sblocca le tracce shorts prima di aggiornare"));
    }
    let anchor = p
        .timeline
        .clips
        .iter()
        .filter(|c| owned.contains(&c.id))
        .map(|c| c.start_ms)
        .min();
    let mut timeline = p.timeline.clone();
    timeline.clips.retain(|c| !owned.contains(&c.id));
    let mut origins = p.clip_origins.clone();
    origins.retain(|id, _| !owned.contains(id));
    let a = p
        .assets
        .iter()
        .find(|a| a.id == asset)
        .ok_or_else(|| bad("Asset assente"))?;
    let kind = if a.kind == AssetKind::Audio {
        TrackKind::Audio
    } else {
        TrackKind::Video
    };
    let track = if let Some(t) = timeline
        .tracks
        .iter()
        .find(|t| t.kind == kind && t.visible && !t.locked)
    {
        t.id
    } else {
        let t = Track::new("Shorts", kind);
        let id = t.id;
        timeline.tracks.push(t);
        id
    };
    let mut start = anchor.unwrap_or_else(|| {
        timeline
            .clips
            .iter()
            .map(|c| c.start_ms + c.duration_ms)
            .max()
            .unwrap_or(0)
    });
    for candidate in candidates {
        if !is_manual(candidate)
            && !p
                .transcripts
                .iter()
                .any(|t| t.id == candidate.transcript_id && t.asset_id == asset)
        {
            return Err(bad("ASR cambiata: revisiona i nuovi candidati"));
        }
        for r in normalise_ranges(p, asset, &candidate.ranges)? {
            let cid = Uuid::new_v5(
                &candidate.id,
                format!("{}:{}", r.start_ms, r.end_ms).as_bytes(),
            );
            timeline.clips.push(TimelineClip {
                id: cid,
                asset_id: asset,
                track_id: track,
                start_ms: start,
                trim_in_ms: r.start_ms,
                duration_ms: r.duration(),
                label: Some(candidate.title.clone()),
                volume: 1.,
                opacity: 1.,
                transform: candidate.transform.clone(),
            });
            start += r.duration();
            origins.insert(
                cid,
                longform::ClipOrigin {
                    producer: producer(candidate),
                    asset_id: asset,
                    input_revision: p.shorts.revision,
                },
            );
        }
    }
    editor::validate_timeline(p, &timeline).map_err(bad)?;
    Ok((timeline, origins))
}
pub async fn apply(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Request>,
) -> Result<Json<Project>, ApiError> {
    update_project(&state, id, |p| {
        revision(p, req.expected_updated_at)?;
        let (tl, origins) = assembled(p, req.asset_id)?;
        p.timeline = tl;
        p.clip_origins = origins;
        p.aspect_ratio = "9:16".into();
        Ok(())
    })
    .await?;
    Ok(Json(project(&state, id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub candidate_ids: Vec<Uuid>,
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
}
fn window(p: &Project, c: &Candidate) -> Result<Project, ApiError> {
    let clips = p
        .timeline
        .clips
        .iter()
        .filter(|clip| {
            p.clip_origins
                .get(&clip.id)
                .is_some_and(|o| o.producer == producer(c))
        })
        .collect::<Vec<_>>();
    let start = clips
        .iter()
        .map(|c| c.start_ms)
        .min()
        .ok_or_else(|| bad("Monta prima i candidati approvati nella timeline comune"))?;
    if !is_manual(c)
        && !p
            .transcripts
            .iter()
            .any(|t| t.id == c.transcript_id && t.asset_id == c.asset_id)
    {
        return Err(bad(
            "ASR cambiata: revisiona i candidati prima di esportare",
        ));
    }
    for clip in &clips {
        let range = TimeRange::new(
            clip.trim_in_ms,
            clip.trim_in_ms.saturating_add(clip.duration_ms),
        )
        .map_err(bad)?;
        let kept = normalise_ranges(p, c.asset_id, &[range]).map_err(|_| bad("Montaggio shorts superato da nuove esclusioni: usa Rimonta ed esporta prima del batch"))?;
        if clip.asset_id != c.asset_id
            || kept != [range]
            || !c
                .ranges
                .iter()
                .any(|r| r.start_ms <= range.start_ms && r.end_ms >= range.end_ms)
        {
            return Err(bad("Montaggio shorts superato da modifiche ai range/esclusioni: usa Rimonta ed esporta prima del batch"));
        }
    }
    let end = clips
        .iter()
        .map(|c| c.start_ms + c.duration_ms)
        .max()
        .unwrap_or(start);
    Ok(crate::compositor::window(p, start, end))
}
pub async fn batch(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Selection>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let p = project(&state, id).await?;
    revision(&p, req.expected_updated_at)?;
    if req.candidate_ids.is_empty() || req.candidate_ids.len() > 30 {
        return Err(bad("Batch: seleziona da 1 a 30 shorts approvati"));
    }
    let mut snapshots = vec![];
    let mut ids = HashSet::new();
    for cid in req.candidate_ids {
        if !ids.insert(cid) {
            return Err(bad("Candidato ripetuto nel batch"));
        }
        let c = p
            .shorts
            .candidates
            .iter()
            .find(|c| c.id == cid && c.approved)
            .ok_or_else(|| bad("Candidato non approvato"))?;
        snapshots.push((cid, window(&p, c)?));
    }
    let job = Job::queued(
        JobKind::ExportTimeline,
        Some(id),
        format!(
            "Batch {} shorts dalla timeline comune · in coda",
            snapshots.len()
        ),
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let jid = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        let root = store::project_dir(&app.data_dir, id);
        let mut outputs = vec![];
        let count = snapshots.len();
        for (index, (cid, snapshot)) in snapshots.into_iter().enumerate() {
            app.set_job(
                jid,
                JobStatus::Running,
                format!(
                    "Short {}/{} · render della timeline revisionata",
                    index + 1,
                    count
                ),
                outputs.clone(),
            )
            .await;
            let root = root.clone();
            let result = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<String>> {
                let directory = root.join("exports");
                std::fs::create_dir_all(&directory)?;
                let output = directory.join(format!("short-{cid}-{jid}.mp4"));
                let layers = compositor::manifest(&snapshot, &root)?;
                let size = compositor::dimensions(&snapshot.aspect_ratio)?;
                crate::subtitle_studio::validate_delivery(&snapshot)?;
                compositor::render_project(
                    &snapshot,
                    &root,
                    &layers,
                    &output,
                    size,
                    crate::social::render_options(&snapshot),
                )?;
                let cues = crate::caption_core::cues(&snapshot);
                let mut files = vec![output.to_string_lossy().to_string()];
                for format in ["srt", "vtt"] {
                    let path = output.with_extension(format);
                    std::fs::write(
                        &path,
                        crate::caption_core::sidecar(&cues, format)
                            .map_err(|e| anyhow::anyhow!(e.1))?,
                    )?;
                    files.push(path.to_string_lossy().to_string());
                }
                for path in crate::subtitle_studio::write_delivery(&snapshot, &output, size)? {
                    if !files.contains(&path) {
                        files.push(path);
                    }
                }
                files.extend(crate::social::write_delivery(&snapshot, &output)?);
                Ok(files)
            })
            .await;
            match result {
                Ok(Ok(files)) => outputs.extend(files),
                error => {
                    let message = match error {
                        Ok(Err(e)) => e.to_string(),
                        Err(e) => e.to_string(),
                        _ => unreachable!(),
                    };
                    app.set_job(jid, JobStatus::Failed, message, outputs).await;
                    return;
                }
            }
        }
        if let Some(j) = app.jobs.write().await.get_mut(&jid) {
            j.result = Some(
                json!({"schema_version":1,"shorts":count,"source_project_updated_at":p.updated_at,"asr_repeated":false,"render":"common_timeline","captions":"sidecars_only_until_M4"}),
            );
        }
        app.set_job(
            jid,
            JobStatus::Succeeded,
            format!("Batch completato · {count} video con SRT/VTT"),
            outputs,
        )
        .await;
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}
pub async fn reframe(
    State(state): State<AppState>,
    Path((id, cid)): Path<(Uuid, Uuid)>,
    Json(req): Json<Selection>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let p = project(&state, id).await?;
    revision(&p, req.expected_updated_at)?;
    let c = p
        .shorts
        .candidates
        .iter()
        .find(|c| c.id == cid)
        .cloned()
        .ok_or_else(|| bad("Candidato assente"))?;
    let asset = p
        .assets
        .iter()
        .find(|a| a.id == c.asset_id && a.kind == AssetKind::Video)
        .ok_or_else(|| {
            bad("Reframe volti disponibile sui video; per audio resta il fondo intero")
        })?;
    let root = store::project_dir(&state.data_dir, id);
    let source = root.join("assets").join(&asset.file_name);
    let ranges = normalise_ranges(&p, c.asset_id, &c.ranges)?;
    let job = Job::queued(
        JobKind::PreparePreview,
        Some(id),
        "Reframe CPU · campionamento massimo 7 fotogrammi",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let jid = job.id;
    let python = state.config().python_executable;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(
            jid,
            JobStatus::Running,
            "Rilevamento volti Haar CPU: nessun modello IA in download",
            vec![],
        )
        .await;
        let input_c = c.clone();
        let result = tokio::task::spawn_blocking(move || -> anyhow::Result<Value> {
            let work = root.join("jobs").join(jid.to_string());
            std::fs::create_dir_all(&work)?;
            std::fs::write(
                work.join("reframe.py"),
                include_str!("../bridge/reframe_bridge.py"),
            )?;
            store::write_json(
                &work.join("request.json"),
                &json!({"source":source,"ranges":ranges}),
            )?;
            let mut cmd = std::process::Command::new(python);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x08000000);
            }
            let output = cmd
                .arg(work.join("reframe.py"))
                .arg(work.join("request.json"))
                .arg(work.join("result.json"))
                .env("PYTHONDONTWRITEBYTECODE", "1")
                .output()?;
            anyhow::ensure!(
                output.status.success(),
                "Reframe CPU: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            store::read_json(&work.join("result.json"))
        })
        .await;
        let result = match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(e)) => Err(e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        match result {
            Ok(value) => {
                let saved = update_project(&app, id, |p| {
                    let current = p
                        .shorts
                        .candidates
                        .iter_mut()
                        .find(|c| c.id == cid)
                        .ok_or_else(|| bad("Candidato rimosso durante l'analisi"))?;
                    if serde_json::to_value(&current.ranges).ok()
                        != serde_json::to_value(&input_c.ranges).ok()
                        || current.transcript_id != input_c.transcript_id
                    {
                        return Err(bad("Candidato cambiato: ripeti il reframe"));
                    }
                    current.reframe = Some(value.clone());
                    p.shorts.revision += 1;
                    Ok(())
                })
                .await;
                match saved {
                    Ok(_) => {
                        if let Some(j) = app.jobs.write().await.get_mut(&jid) {
                            j.result = Some(value);
                        }
                        app.set_job(
                            jid,
                            JobStatus::Succeeded,
                            "Suggerimento reframe pronto: applicalo soltanto dopo revisione",
                            vec![],
                        )
                        .await;
                    }
                    Err(e) => app.set_job(jid, JobStatus::Failed, e.1, vec![]).await,
                }
            }
            Err(e) => app.set_job(jid, JobStatus::Failed, e, vec![]).await,
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn candidate() -> Candidate {
        Candidate{id:Uuid::new_v4(),asset_id:Uuid::nil(),transcript_id:Uuid::nil(),title:"Prova".into(),ranges:vec![TimeRange{start_ms:1000,end_ms:21000}],evidence:"Come funziona un esempio realmente completo di montaggio video per una frase intera?".into(),producer:"test".into(),approved:false,transform:VisualTransform::default(),scores:BTreeMap::new(),score:0,warnings:vec![],duplicate_of:None,reframe:None,category:"momento forte".into()}
    }
    #[test]
    fn stale_short_window_is_blocked_until_explicit_remount_without_repeating_asr() {
        let mut p = Project::new("Stale".into(), None);
        let aid = Uuid::new_v4();
        let tid = Uuid::new_v4();
        p.assets.push(Asset {
            id: aid,
            name: "Fixture".into(),
            kind: AssetKind::Video,
            file_name: "fixture.mp4".into(),
            source_path: String::new(),
            imported_at: chrono::Utc::now(),
            media: Some(MediaInfo {
                duration_ms: Some(3000),
                width: Some(640),
                height: Some(360),
                frame_rate: Some(30.),
                has_audio: true,
            }),
        });
        p.transcripts.push(Transcript {
            id: tid,
            asset_id: aid,
            language: "it".into(),
            segments: vec![],
            words: vec![],
            imported_at: chrono::Utc::now(),
        });
        let mut c = candidate();
        c.asset_id = aid;
        c.transcript_id = tid;
        c.approved = true;
        c.ranges = vec![TimeRange {
            start_ms: 0,
            end_ms: 3000,
        }];
        p.shorts.candidates.push(c.clone());
        (p.timeline, p.clip_origins) = assembled(&p, aid).unwrap();
        assert!(window(&p, &c).is_ok());
        p.longform.decisions.push(longform::Decision {
            id: Uuid::new_v4(),
            asset_id: aid,
            range: TimeRange {
                start_ms: 1000,
                end_ms: 2000,
            },
            action: "exclude".into(),
            reason: "New edit".into(),
        });
        assert!(window(&p, &c).is_err());
        (p.timeline, p.clip_origins) = assembled(&p, aid).unwrap();
        let snapshot = window(&p, &c).unwrap();
        assert_eq!(
            snapshot
                .timeline
                .clips
                .iter()
                .map(|c| c.duration_ms)
                .sum::<u64>(),
            2000
        );
        assert_eq!(p.transcripts.len(), 1);
        assert_eq!(p.shorts.candidates[0].ranges[0].end_ms, 3000);
    }
    #[test]
    fn scores_are_explainable_and_duplicates_are_not_deleted_or_auto_approved() {
        let mut a = candidate();
        score(&mut a, &default_weights());
        assert_eq!(a.scores.len(), 6);
        let mut b = a.clone();
        b.id = Uuid::new_v4();
        let mut all = vec![a, b];
        dedup(&mut all);
        assert_eq!(all[1].duplicate_of, Some(all[0].id));
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|c| !c.approved));
    }
}
