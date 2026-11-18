//! Editable mixed-media sequence. Proposals are snapshots, never hidden edits.
use crate::{editor, model::*, ApiError, AppState};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub note: String,
    pub section: String,
    pub duration_ms: u64,
    pub trim_in_ms: u64,
    pub movement: String,
    pub fit: String,
    pub locked: bool,
    pub title: String,
    pub reason: String,
    pub emotion: String,
    pub transition_ms: Option<u64>,
    pub source_volume: f64,
}
impl Default for Item {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            asset_id: Uuid::nil(),
            note: String::new(),
            section: "development".into(),
            duration_ms: 5000,
            trim_in_ms: 0,
            movement: "still".into(),
            fit: "contain".into(),
            locked: false,
            title: String::new(),
            reason: String::new(),
            emotion: String::new(),
            transition_ms: None,
            source_volume: 0.0,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Document {
    /// Server-owned registry, limited to sequence clips, never entire tracks.
    pub owned_clip_ids: Vec<Uuid>,
    pub rhythm_markers_ms: Vec<u64>,
    pub brief: String,
    pub items: Vec<Item>,
    pub transition_ms: u64,
    pub music_asset_id: Option<Uuid>,
    pub music_volume: f64,
    pub title_design: String,
    pub bpm: Option<f64>,
    pub beat_offset_ms: u64,
    pub narration_asset_id: Option<Uuid>,
    pub narration_note: String,
    pub music_note: String,
    pub plans: Vec<crate::montage_planner::Plan>,
    pub applied_clips: Vec<TimelineClip>,
    pub preserve_timeline_edits: bool,
    pub visual_evidence: std::collections::BTreeMap<Uuid, crate::montage_planner::Evidence>,
    pub album: crate::album::Album,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            owned_clip_ids: vec![],
            rhythm_markers_ms: vec![],
            brief: String::new(),
            items: vec![],
            transition_ms: 400,
            music_asset_id: None,
            music_volume: 0.25,
            title_design: "editorial".into(),
            bpm: None,
            beat_offset_ms: 0,
            narration_asset_id: None,
            narration_note: String::new(),
            music_note: String::new(),
            plans: vec![],
            applied_clips: vec![],
            preserve_timeline_edits: true,
            visual_evidence: Default::default(),
            album: Default::default(),
        }
    }
}
impl Document {
    pub(crate) fn validate(&self, p: &Project) -> anyhow::Result<()> {
        anyhow::ensure!(
            ["editorial", "cinematic", "social"].contains(&self.title_design.as_str()),
            "Composizione titolo non valida"
        );
        anyhow::ensure!(
            self.narration_note.len() <= 12000 && self.music_note.len() <= 4000,
            "Note audio troppo estese"
        );
        anyhow::ensure!(
            self.rhythm_markers_ms.len() <= 20_000
                && self.rhythm_markers_ms.iter().all(|v| *v <= 7_200_000)
                && self.rhythm_markers_ms.windows(2).all(|w| w[0] < w[1]),
            "Pulsazioni: massimo20.000, ordinate/distinte, entro due ore"
        );
        anyhow::ensure!(self.brief.len() <= 12000, "Massimo 12.000 byte di tema");
        self.album.validate(p)?;
        anyhow::ensure!(
            self.transition_ms <= 3000
                && self.music_volume.is_finite()
                && (0.0..=2.0).contains(&self.music_volume),
            "Transizione 0–3s, volume musica 0–200%"
        );
        anyhow::ensure!(
            self.bpm
                .is_none_or(|v| v.is_finite() && (30.0..=300.0).contains(&v))
                && self.beat_offset_ms <= 60_000,
            "Griglia manuale: 30–300 BPM, offset massimo 60s"
        );
        let mut ids = std::collections::HashSet::new();
        for item in &self.items {
            anyhow::ensure!(
                item.title.len() <= 160
                    && item.reason.len() <= 4000
                    && item.emotion.len() <= 1000
                    && item.transition_ms.is_none_or(|v| v <= 3000)
                    && item.source_volume.is_finite()
                    && (0.0..=2.0).contains(&item.source_volume),
                "Titolo, intenzione, raccordo o volume non validi"
            );
            anyhow::ensure!(
                ids.insert(item.id) && !item.id.is_nil(),
                "ID elemento duplicato o assente"
            );
            let a = p
                .assets
                .iter()
                .find(|a| a.id == item.asset_id)
                .ok_or_else(|| anyhow::anyhow!("Media fuori progetto"))?;
            anyhow::ensure!(
                matches!(a.kind, AssetKind::Image | AssetKind::Video),
                "La sequenza accetta foto/video; la musica va nel controllo dedicato"
            );
            anyhow::ensure!(
                (500..=600000).contains(&item.duration_ms) && item.trim_in_ms <= 86_400_000,
                "Durata elemento 0,5–600s, trim valido"
            );
            if a.kind == AssetKind::Video {
                anyhow::ensure!(a.media.as_ref().and_then(|m|m.duration_ms).is_some_and(|v| item.trim_in_ms.saturating_add(item.duration_ms)<=v+50), "Trim/durata oltre il video disponibile");
            }
            anyhow::ensure!(
                ["still", "push", "pull", "pan_left", "pan_right"]
                    .contains(&item.movement.as_str())
                    && ["contain", "cover", "blur", "auto"].contains(&item.fit.as_str()),
                "Movimento o adattamento non supportati"
            );
            anyhow::ensure!(
                item.note.len() <= 4000
                    && ["opening", "development", "finale"].contains(&item.section.as_str()),
                "Nota troppo lunga o sezione non valida"
            );
        }
        for id in [self.music_asset_id, self.narration_asset_id]
            .into_iter()
            .flatten()
        {
            anyhow::ensure!(
                p.assets
                    .iter()
                    .any(|a| a.id == id && a.kind == AssetKind::Audio),
                "Scegli una musica audio del progetto"
            );
        }
        Ok(())
    }
}
#[derive(Deserialize)]
pub struct Save {
    pub document: Document,
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
}
pub async fn save(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(r): Json<Save>,
) -> Result<Json<Project>, ApiError> {
    let mut all = state.projects.write().await;
    let p = all
        .get_mut(&id)
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    if p.updated_at != r.expected_updated_at {
        return Err(ApiError(
            axum::http::StatusCode::CONFLICT,
            "Progetto aggiornato: ricarica, nessuna modifica applicata".into(),
        ));
    }
    r.document
        .validate(p)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let mut next = p.clone();
    let mut doc = r.document;
    crate::album::preserve_evidence(&p.montage.album, &mut doc.album);
    doc.plans = p.montage.plans.clone();
    doc.applied_clips = p.montage.applied_clips.clone();
    let approvals = doc.visual_evidence;
    doc.visual_evidence = p.montage.visual_evidence.clone();
    doc.visual_evidence
        .retain(|id, _| doc.items.iter().any(|i| i.id == *id));
    for (id, evidence) in &mut doc.visual_evidence {
        if let Some(user) = approvals.get(id) {
            evidence.approved = user.approved;
        }
    }
    let current_ids = p
        .timeline
        .clips
        .iter()
        .map(|c| c.id)
        .collect::<std::collections::HashSet<_>>();
    let mut owned = p
        .montage
        .owned_clip_ids
        .iter()
        .copied()
        .chain(p.montage.items.iter().map(|v| clip_id(p.id, v.id)))
        .filter(|id| current_ids.contains(id))
        .collect::<std::collections::HashSet<_>>();
    owned.extend(doc.items.iter().map(|v| clip_id(p.id, v.id)));
    owned.insert(Uuid::new_v5(&p.id, b"creative-montage:music"));
    doc.owned_clip_ids = owned.into_iter().collect();
    doc.owned_clip_ids.sort();
    next.montage = doc;
    next.updated_at = chrono::Utc::now();
    state.save_project(&next).await?;
    *p = next.clone();
    Ok(Json(next))
}
pub(crate) fn clip_id(pid: Uuid, id: Uuid) -> Uuid {
    Uuid::new_v5(&pid, format!("creative-montage:{id}").as_bytes())
}
pub(crate) fn proposal(p: &Project) -> anyhow::Result<Value> {
    let d = &p.montage;
    d.validate(p)?;
    anyhow::ensure!(
        !d.items.is_empty(),
        "Aggiungi fotografie e video alla sequenza"
    );
    let track_id = Uuid::new_v5(&p.id, b"creative-montage:video");
    let music_track = Uuid::new_v5(&p.id, b"creative-montage:audio");
    let music_clip = Uuid::new_v5(&p.id, b"creative-montage:music");
    let voice_track = Uuid::new_v5(&p.id, b"creative-montage:voice");
    let voice_clip = Uuid::new_v5(&p.id, b"creative-montage:narration");
    let mut tl = p.timeline.clone();
    anyhow::ensure!(
        !tl.tracks
            .iter()
            .any(|t| [track_id, music_track, voice_track].contains(&t.id) && t.locked),
        "Sblocca le tracce del montaggio creativo"
    );
    // Ownership is by clip ID, not track: manual inserts on these tracks survive.
    let owned = d
        .owned_clip_ids
        .iter()
        .copied()
        .chain(d.items.iter().map(|i| clip_id(p.id, i.id)))
        .chain([music_clip, voice_clip])
        .collect::<std::collections::HashSet<_>>();
    tl.clips.retain(|c| !owned.contains(&c.id));
    for (id, name, kind) in [
        (track_id, "Montaggio creativo · Visual", TrackKind::Video),
        (music_track, "Montaggio creativo · Musica", TrackKind::Audio),
        (
            voice_track,
            "Montaggio creativo · Narrazione",
            TrackKind::Audio,
        ),
    ] {
        if !tl.tracks.iter().any(|t| t.id == id) {
            tl.tracks.push(Track {
                id,
                name: name.into(),
                kind,
                visible: true,
                locked: false,
            });
        }
    }
    let origin = p
        .timeline
        .clips
        .iter()
        .filter(|c| owned.contains(&c.id))
        .map(|c| c.start_ms)
        .min()
        .unwrap_or_else(|| {
            tl.clips
                .iter()
                .map(|c| c.start_ms + c.duration_ms)
                .max()
                .unwrap_or(0)
        });
    let mut cursor = origin;
    let mut summary = vec![];
    for (i, item) in d.items.iter().enumerate() {
        let a = p.assets.iter().find(|a| a.id == item.asset_id).unwrap();
        let previous = i
            .checked_sub(1)
            .map(|i| d.items[i].duration_ms)
            .unwrap_or(0);
        let overlap = if i == 0 {
            0
        } else {
            item.transition_ms
                .unwrap_or(d.transition_ms)
                .min(previous / 3)
                .min(item.duration_ms / 3)
        };
        cursor -= overlap;
        let mut tr = VisualTransform {
            fit: sequence_fit(p, a, item).into(),
            fade_in_ms: if i == 0 {
                d.transition_ms.min(item.duration_ms / 3)
            } else {
                overlap
            },
            fade_out_ms: if i + 1 == d.items.len() {
                d.transition_ms.min(item.duration_ms / 3)
            } else {
                0
            },
            ..Default::default()
        };
        match item.movement.as_str() {
            "push" => tr.zoom_end = 1.08,
            "pull" => tr.zoom_start = 1.08,
            "pan_left" => {
                tr.zoom_start = 1.1;
                tr.zoom_end = 1.1;
                tr.x = 0.525;
                tr.pan_x = -0.05;
            }
            "pan_right" => {
                tr.zoom_start = 1.1;
                tr.zoom_end = 1.1;
                tr.x = 0.475;
                tr.pan_x = 0.05;
            }
            _ => {}
        }
        let mut clip = TimelineClip {
            id: clip_id(p.id, item.id),
            asset_id: a.id,
            track_id,
            start_ms: cursor,
            trim_in_ms: item.trim_in_ms,
            duration_ms: item.duration_ms,
            label: Some(format!(
                "{} · {}",
                item.section,
                if item.note.is_empty() {
                    &a.name
                } else {
                    &item.note
                }
            )),
            volume: item.source_volume,
            opacity: 1.0,
            transform: tr,
        };
        let mut preserved = false;
        if d.preserve_timeline_edits {
            if let Some(old) = p.timeline.clips.iter().find(|c| c.id == clip.id) {
                let changed = d
                    .applied_clips
                    .iter()
                    .find(|c| c.id == old.id)
                    .is_some_and(|base| {
                        serde_json::to_value(base).ok() != serde_json::to_value(old).ok()
                    });
                if changed {
                    clip = old.clone();
                    preserved = true;
                }
            } else if d.applied_clips.iter().any(|c| c.id == clip.id) {
                summary.push(json!({"asset_id":a.id,"start_ms":cursor,"duration_ms":0,"transition_ms":0,"section":item.section,"preserved_deletion":true}));
                cursor += overlap;
                continue;
            }
        }
        cursor = clip.start_ms;
        let duration = clip.duration_ms;
        tl.clips.push(clip);
        summary.push(json!({"asset_id":a.id,"start_ms":cursor,"duration_ms":item.duration_ms,"transition_ms":overlap,"section":item.section}));
        summary.last_mut().unwrap()["preserved_manual"] = json!(preserved);
        cursor += duration;
    }
    if let Some(id) = d.narration_asset_id {
        let available = p
            .assets
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.media.as_ref())
            .and_then(|m| m.duration_ms)
            .unwrap_or(cursor - origin);
        tl.clips.push(TimelineClip {
            id: voice_clip,
            asset_id: id,
            track_id: voice_track,
            start_ms: origin,
            trim_in_ms: 0,
            duration_ms: available.min(cursor - origin),
            label: Some("Narrazione originale · revisionare sincronismo".into()),
            volume: 1.0,
            opacity: 1.0,
            transform: VisualTransform::default(),
        });
    }
    cursor = tl
        .clips
        .iter()
        .filter(|c| owned.contains(&c.id))
        .map(|c| c.start_ms + c.duration_ms)
        .max()
        .unwrap_or(cursor)
        .max(cursor);
    if let Some(id) = d.music_asset_id {
        tl.clips.push(TimelineClip {
            id: music_clip,
            asset_id: id,
            track_id: music_track,
            start_ms: origin,
            trim_in_ms: 0,
            duration_ms: cursor - origin,
            label: Some("Colonna sonora · controlla diritti e mix".into()),
            volume: d.music_volume,
            opacity: 1.0,
            transform: VisualTransform {
                audio_fade_in_ms: 500,
                audio_fade_out_ms: 1000,
                ..Default::default()
            },
        });
    }
    if d.preserve_timeline_edits {
        for cid in [music_clip, voice_clip] {
            if let Some(base) = d.applied_clips.iter().find(|c| c.id == cid) {
                match p.timeline.clips.iter().find(|c| c.id == cid) {
                    Some(old)
                        if serde_json::to_value(old).ok() != serde_json::to_value(base).ok() =>
                    {
                        if let Some(new) = tl.clips.iter_mut().find(|c| c.id == cid) {
                            *new = old.clone();
                        }
                    }
                    None => tl.clips.retain(|c| c.id != cid),
                    _ => {}
                }
            }
        }
    }
    editor::validate_timeline(p, &tl)?;
    Ok(
        json!({"timeline":tl,"summary":summary,"duration_ms":cursor-origin,"origin_ms":origin,"notice":"Assemblaggio nativo del piano revisionato. Modifiche manuali e clip bloccati conservati se la protezione è attiva; controlla eventuali sovrapposizioni. Nessuna generazione di risorse mancanti. Titoli editabili in Motion."}),
    )
}

/// Called only on explicit montage timeline commit, inside the existing atomic save.
pub(crate) fn add_titles(
    p: &mut Project,
    previous: &Project,
    root: Option<&std::path::Path>,
) -> Result<(), ApiError> {
    for item in &p.montage.items {
        let Some(clip) = p
            .timeline
            .clips
            .iter()
            .find(|c| c.id == clip_id(p.id, item.id))
        else {
            continue;
        };
        let id = Uuid::new_v5(&item.id, b"montage-title");
        if item.title.trim().is_empty() || p.motion.graphics.iter().any(|g| g.id == id) {
            continue;
        }
        let duration = clip.duration_ms.min(8000);
        let (w, h) = crate::compositor::dimensions(&p.aspect_ratio).map_err(ApiError::internal)?;
        let mut graphic = crate::motion::title_design::build(
            &item.title,
            "",
            &p.montage.title_design,
            f64::from(w) / f64::from(h),
            duration,
            root,
        )
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
        graphic.id = id;
        graphic.start_ms = clip.start_ms;
        graphic.anchor_clip_id = Some(clip.id);
        graphic.anchor_source_ms = clip.trim_in_ms;
        graphic.approved = true;
        graphic.evidence.push_str(" Testo della proposta approvata; grafica modificabile in Motion. Nessun branding imposto.");
        p.motion.graphics.push(graphic);
    }
    crate::motion::validate(&p.motion)?;
    p.motion.revision += 1;
    // A preserved manual edit must NOT become the next generated baseline:
    // otherwise the second reapply would erase it. Keep deletion tombstones too.
    let owned = p
        .montage
        .items
        .iter()
        .map(|i| clip_id(p.id, i.id))
        .chain([
            Uuid::new_v5(&p.id, b"creative-montage:music"),
            Uuid::new_v5(&p.id, b"creative-montage:narration"),
        ])
        .collect::<std::collections::HashSet<_>>();
    let mut snapshots = vec![];
    for id in owned {
        let base = previous.montage.applied_clips.iter().find(|c| c.id == id);
        let before = previous.timeline.clips.iter().find(|c| c.id == id);
        let edited = base.is_some_and(|base| {
            serde_json::to_value(base).ok() != before.and_then(|v| serde_json::to_value(v).ok())
        });
        if p.montage.preserve_timeline_edits && edited {
            if let Some(base) = base {
                snapshots.push(base.clone());
            }
        } else if let Some(c) = p.timeline.clips.iter().find(|c| c.id == id) {
            snapshots.push(c.clone());
        }
    }
    snapshots.sort_by_key(|c| c.id);
    p.montage.applied_clips = snapshots;
    Ok(())
}
// Unknown subject locations are not evidence that a centered crop is safe.
// Resolve new automatic photo proposals to the existing whole-image compositor modes;
// video auto-framing and any explicit/manual fit remain unchanged.
fn sequence_fit<'a>(project: &Project, asset: &Asset, item: &'a Item) -> &'a str {
    if asset.kind != AssetKind::Image || item.fit != "auto" {
        return &item.fit;
    }
    let same_aspect = asset
        .media
        .as_ref()
        .and_then(|m| Some((m.width?, m.height?)))
        .zip(crate::compositor::dimensions(&project.aspect_ratio).ok())
        .is_some_and(|((w, h), (cw, ch))| {
            w > 0 && h > 0 && u64::from(w) * u64::from(ch) == u64::from(h) * u64::from(cw)
        });
    if same_aspect {
        "contain"
    } else {
        "blur"
    }
}

pub async fn preview(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let all = state.projects.read().await;
    let p = all
        .get(&id)
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    proposal(p)
        .map(Json)
        .map_err(|e| ApiError::bad_request(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn picture() -> Asset {
        Asset {
            id: Uuid::new_v4(),
            name: "Foto".into(),
            kind: AssetKind::Image,
            file_name: "a.png".into(),
            source_path: String::new(),
            imported_at: chrono::Utc::now(),
            media: None,
        }
    }
    #[test]
    fn automatic_photos_keep_the_frame_and_respect_explicit_crops() {
        let mut p = Project::new("Whole photograph".into(), Some("16:9".into()));
        p.assets.push(picture());
        p.montage.items.push(Item {
            asset_id: p.assets[0].id,
            fit: "auto".into(),
            ..Default::default()
        });
        let proposed: Timeline =
            serde_json::from_value(proposal(&p).unwrap()["timeline"].clone()).unwrap();
        assert_eq!(proposed.clips[0].transform.fit, "blur");
        p.assets[0].media = Some(MediaInfo {
            duration_ms: None,
            width: Some(1920),
            height: Some(1080),
            frame_rate: None,
            has_audio: false,
        });
        assert_eq!(
            sequence_fit(&p, &p.assets[0], &p.montage.items[0]),
            "contain"
        );
        p.montage.items[0].fit = "cover".into();
        assert_eq!(sequence_fit(&p, &p.assets[0], &p.montage.items[0]), "cover");
        p.montage.items[0].fit = "auto".into();
        p.assets[0].kind = AssetKind::Video;
        assert_eq!(sequence_fit(&p, &p.assets[0], &p.montage.items[0]), "auto");
    }
    #[test]
    fn overlaps_are_bounded_and_reapplication_is_idempotent() {
        let mut p = Project::new("Test".into(), None);
        p.assets.push(picture());
        p.montage.items = (0..3)
            .map(|_| Item {
                asset_id: p.assets[0].id,
                ..Default::default()
            })
            .collect();
        let first = proposal(&p).unwrap();
        assert_eq!(first["duration_ms"], 14200);
        p.timeline = serde_json::from_value(first["timeline"].clone()).unwrap();
        assert_eq!(proposal(&p).unwrap()["timeline"], first["timeline"]);
    }
    #[test]
    fn foreign_media_and_locked_track_rejected() {
        let mut p = Project::new("Test".into(), None);
        p.montage.items.push(Item::default());
        assert!(proposal(&p).is_err());
        p.assets.push(picture());
        p.montage.items[0].asset_id = p.assets[0].id;
        let v = proposal(&p).unwrap();
        p.timeline = serde_json::from_value(v["timeline"].clone()).unwrap();
        p.timeline.tracks.last_mut().unwrap().locked = true;
        assert!(proposal(&p).is_err());
    }
    #[test]
    fn manual_inserts_on_sequence_track_are_never_owned_by_track() {
        let mut p = Project::new("Test".into(), None);
        p.assets.push(picture());
        p.montage.items.push(Item {
            asset_id: p.assets[0].id,
            ..Default::default()
        });
        p.timeline = serde_json::from_value(proposal(&p).unwrap()["timeline"].clone()).unwrap();
        let mut manual = p.timeline.clips[0].clone();
        manual.id = Uuid::new_v4();
        manual.start_ms = 12_000;
        p.timeline.clips.push(manual.clone());
        let next: Timeline =
            serde_json::from_value(proposal(&p).unwrap()["timeline"].clone()).unwrap();
        assert!(next
            .clips
            .iter()
            .any(|c| c.id == manual.id && c.start_ms == 12_000));
        assert_eq!(next.clips.len(), 2);
    }
    #[test]
    fn manual_edits_survive_repeated_commits_and_deletions_stay_deleted() {
        let mut p = Project::new("T".into(), None);
        p.assets.push(picture());
        p.montage.items.push(Item {
            asset_id: p.assets[0].id,
            ..Item::default()
        });
        let before = p.clone();
        p.timeline = serde_json::from_value(proposal(&p).unwrap()["timeline"].clone()).unwrap();
        add_titles(&mut p, &before, None).unwrap();
        p.timeline.clips[0].transform.rotation = 7.0;
        for _ in 0..3 {
            let before = p.clone();
            p.timeline = serde_json::from_value(proposal(&p).unwrap()["timeline"].clone()).unwrap();
            add_titles(&mut p, &before, None).unwrap();
            assert_eq!(p.timeline.clips[0].transform.rotation, 7.0);
        }
        p.timeline.clips.clear();
        let before = p.clone();
        p.timeline = serde_json::from_value(proposal(&p).unwrap()["timeline"].clone()).unwrap();
        add_titles(&mut p, &before, None).unwrap();
        assert!(p.timeline.clips.is_empty());
    }
    #[test]
    fn new_title_compositions_are_native_and_existing_manual_designs_are_preserved() {
        let mut p = Project::new("Titles".into(), None);
        p.assets.push(picture());
        p.montage.items.push(Item {
            asset_id: p.assets[0].id,
            title: "Nicola Lanci".into(),
            fit: "auto".into(),
            ..Item::default()
        });
        let before = p.clone();
        p.timeline = serde_json::from_value(proposal(&p).unwrap()["timeline"].clone()).unwrap();
        add_titles(&mut p, &before, None).unwrap();
        assert!(p.motion.graphics[0].layers.len() >= 3);
        assert!(p.motion.graphics[0]
            .layers
            .iter()
            .any(|l| l.width_link.is_some()));
        p.motion.graphics[0].layers[0].color = "#7531FF".into();
        p.montage.title_design = "cinematic".into();
        let before = p.clone();
        add_titles(&mut p, &before, None).unwrap();
        assert_eq!(p.motion.graphics.len(), 1);
        assert_eq!(p.motion.graphics[0].layers[0].color, "#7531FF");
        let legacy: Document = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.title_design, "editorial");
    }
}
