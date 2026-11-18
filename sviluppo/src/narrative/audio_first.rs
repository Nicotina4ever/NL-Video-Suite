//! Original soundtrack is an independent source, not narration regenerated per shot.
use super::*;
use crate::time_map::TimeRange;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Segment {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub source: TimeRange,
    pub start_ms: u64,
    pub gain: f64,
    pub label: String,
    pub notes: String,
}
impl Default for Segment {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            asset_id: Uuid::nil(),
            source: TimeRange {
                start_ms: 0,
                end_ms: 0,
            },
            start_ms: 0,
            gain: 1.,
            label: String::new(),
            notes: String::new(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub segment_id: Uuid,
    pub source: TimeRange,
}

pub fn clip_id(pid: Uuid, id: Uuid) -> Uuid {
    Uuid::new_v5(&pid, format!("narrative-audio-master:{id}").as_bytes())
}
pub fn track_id(pid: Uuid) -> Uuid {
    Uuid::new_v5(&pid, b"narrative-audio-master-track")
}
pub fn validate(p: &Project, prod: &mut Production) -> Result<(), ApiError> {
    let mut ids = HashSet::new();
    for segment in &mut prod.audio_segments {
        if segment.id.is_nil() || !ids.insert(segment.id) {
            return Err(bad("ID segmento audio assente o duplicato"));
        }
        let asset = p
            .assets
            .iter()
            .find(|a| a.id == segment.asset_id)
            .ok_or_else(|| bad("Audio master fuori progetto"))?;
        let media = asset
            .media
            .as_ref()
            .filter(|m| m.has_audio)
            .ok_or_else(|| bad("La sorgente master non contiene audio verificato"))?;
        TimeRange::new(segment.source.start_ms, segment.source.end_ms)
            .map_err(|e| bad(e.to_string()))?;
        if media.duration_ms.is_none_or(|d| segment.source.end_ms > d)
            || segment
                .start_ms
                .checked_add(segment.source.end_ms - segment.source.start_ms)
                .is_none_or(|end| end > 86_400_000)
            || !segment.gain.is_finite()
            || !(0.0..=2.0).contains(&segment.gain)
        {
            return Err(bad(
                "Range, posizione o livello del segmento audio non validi",
            ));
        }
        segment.label = checked_text(&segment.label, "Nome segmento audio", 300)?;
        segment.notes = checked_text(&segment.notes, "Annotazioni audio confermate", 8000)?;
    }
    for scene in &mut prod.scenes {
        if let Some(anchor) = &scene.audio_anchor {
            let segment=prod.audio_segments.iter().find(|v|v.id==anchor.segment_id).ok_or_else(||bad("Shot collegato a un segmento master rimosso: correggi il raccordo esplicitamente"))?;
            TimeRange::new(anchor.source.start_ms, anchor.source.end_ms)
                .map_err(|e| bad(e.to_string()))?;
            if anchor.source.start_ms < segment.source.start_ms
                || anchor.source.end_ms > segment.source.end_ms
            {
                return Err(bad("Shot oltre l’intervallo del master originale"));
            }
            scene.duration_ms = anchor.source.end_ms - anchor.source.start_ms;
        }
    }
    Ok(())
}
pub fn audio_fingerprint(segments: &[Segment]) -> Value {
    json!(segments.iter().map(|s|json!({"id":s.id,"asset":s.asset_id,"source":s.source,"start":s.start_ms,"gain":s.gain})).collect::<Vec<_>>())
}
pub fn context(p: &Project, prod: &Production) -> Value {
    json!({"contract":"Original mixed audio, not classified as speech/music/FX. Notes are user statements. Cached ASR is fallible; no new ASR or sound identification is implied. Four source files are not automatically four scenes.","segments":prod.audio_segments.iter().map(|s|json!({"segment_id":s.id,"source_asset_id":s.asset_id,"source":s.source,"timeline_start_ms":s.start_ms,"user_label":s.label,"user_notes":s.notes,"cached_transcripts":crate::asr::active(p,s.asset_id).into_iter().map(|t|json!({"raw_id":t.id,"language":t.language,"segments":t.segments.iter().filter(|r|r.end_ms>s.source.start_ms&&r.start_ms<s.source.end_ms).collect::<Vec<_>>()})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
}
pub fn append_master(
    p: &Project,
    prod: &Production,
    timeline: &mut Timeline,
    previous: &std::collections::BTreeMap<Uuid, TimelineClip>,
    origin: u64,
) -> Result<(), ApiError> {
    if prod.audio_segments.is_empty() {
        return Ok(());
    }
    if !prod.audio_master_approved {
        return Err(bad(
            "Ascolta e conferma il master audio originale prima del montaggio",
        ));
    }
    let track = track_id(p.id);
    if timeline.tracks.iter().any(|t| t.id == track && t.locked) {
        return Err(bad("Sblocca la traccia master audio originale"));
    }
    if !timeline.tracks.iter().any(|t| t.id == track) {
        timeline.tracks.push(Track {
            id: track,
            name: "Storia · Master originale (ruolo da assegnare)".into(),
            kind: TrackKind::Audio,
            visible: true,
            locked: false,
        });
    }
    for segment in &prod.audio_segments {
        let unchanged = prod.assembled_audio_sources.get(&segment.id)
            == Some(&audio_fingerprint(std::slice::from_ref(segment)));
        if unchanged && !prod.audio_rebuild_ids.contains(&segment.id) {
            let old = previous.get(&clip_id(p.id, segment.id)).ok_or_else(||bad("Master rimosso dalla timeline: ricostruisci esplicitamente la sorgente o correggi il piano"))?;
            if old.asset_id != segment.asset_id {
                return Err(bad("Sorgente master sostituita nella timeline: riconcilia il piano prima di assemblare"));
            }
            timeline.clips.push(old.clone());
            continue;
        }
        let start = origin
            .checked_add(segment.start_ms)
            .ok_or_else(|| bad("Posizione master fuori dominio"))?;
        let duration = segment.source.end_ms - segment.source.start_ms;
        if start
            .checked_add(duration)
            .is_none_or(|end| end > 86_400_000)
        {
            return Err(bad("Master oltre le24ore della timeline"));
        }
        timeline.clips.push(TimelineClip {
            id: clip_id(p.id, segment.id),
            asset_id: segment.asset_id,
            track_id: track,
            start_ms: start,
            trim_in_ms: segment.source.start_ms,
            duration_ms: duration,
            label: Some(format!("Originale · {}", segment.label)),
            volume: segment.gain,
            opacity: 1.,
            transform: Default::default(),
        });
    }
    Ok(())
}
pub fn placement(
    pid: Uuid,
    prod: &Production,
    scene: &Scene,
    timeline: &Timeline,
) -> Result<Option<(u64, u64)>, ApiError> {
    let Some(anchor) = &scene.audio_anchor else {
        return Ok(None);
    };
    let segment = prod
        .audio_segments
        .iter()
        .find(|v| v.id == anchor.segment_id)
        .ok_or_else(|| bad("Segmento master dello shot assente"))?;
    if anchor.source.start_ms < segment.source.start_ms
        || anchor.source.end_ms > segment.source.end_ms
        || anchor.source.end_ms <= anchor.source.start_ms
    {
        return Err(bad("Raccordo shot/master non valido"));
    }
    let master = timeline
        .clips
        .iter()
        .find(|c| c.id == clip_id(pid, segment.id))
        .ok_or_else(|| bad("Master dello shot assente dalla timeline"))?;
    if master.asset_id != segment.asset_id
        || anchor.source.start_ms < master.trim_in_ms
        || anchor.source.end_ms > master.trim_in_ms.saturating_add(master.duration_ms)
    {
        return Err(bad("Raccordo fuori dal master effettivo: rivedi il trim o lo shot, nessun audio viene ricreato automaticamente"));
    }
    let start = master
        .start_ms
        .checked_add(anchor.source.start_ms - master.trim_in_ms)
        .ok_or_else(|| bad("Clock master oltre dominio"))?;
    Ok(Some((start, anchor.source.end_ms - anchor.source.start_ms)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approve {
    pub expected_revision: u64,
    #[serde(default)]
    pub rebuild_ids: Vec<Uuid>,
}
pub async fn approve(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(r): Json<Approve>,
) -> Result<Json<Production>, ApiError> {
    let _guard = production_lock().lock().await;
    let p = project(&state, id).await?;
    let mut prod = load(&state, &p)?;
    ensure_idle(&state, &prod).await?;
    if prod.revision != r.expected_revision {
        return Err(bad("Master cambiato: ricarica prima della conferma"));
    }
    validate(&p, &mut prod)?;
    if prod.audio_segments.is_empty() {
        return Err(bad("Aggiungi prima uno o più segmenti audio originali"));
    }
    if r.rebuild_ids.iter().collect::<HashSet<_>>().len() != r.rebuild_ids.len()
        || r.rebuild_ids
            .iter()
            .any(|id| !prod.audio_segments.iter().any(|s| s.id == *id))
    {
        return Err(bad(
            "Scegli sorgenti distinte del piano corrente per la ricostruzione",
        ));
    }
    for id in r.rebuild_ids {
        if !prod.audio_rebuild_ids.contains(&id) {
            prod.audio_rebuild_ids.push(id);
        }
    }
    prod.audio_master_approved = true;
    persist(&state, id, &mut prod)?;
    Ok(Json(prod))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Segment {
        Segment {
            asset_id: Uuid::new_v4(),
            source: TimeRange {
                start_ms: 1000,
                end_ms: 6000,
            },
            label: "Master".into(),
            ..Default::default()
        }
    }
    #[test]
    fn source_master_is_independent_and_reassembly_preserves_manual_clock_and_gain() {
        let p = Project::new("Audio first".into(), None);
        let segment = source();
        let mut prod = Production {
            audio_segments: vec![segment.clone()],
            audio_master_approved: true,
            ..Default::default()
        };
        let mut timeline = p.timeline.clone();
        append_master(&p, &prod, &mut timeline, &Default::default(), 2000).unwrap();
        assert_eq!(timeline.clips.len(), 1);
        assert_eq!(timeline.clips[0].trim_in_ms, 1000);
        let mut edited = timeline.clips[0].clone();
        edited.start_ms = 7000;
        edited.volume = 0.42;
        prod.assembled_audio_sources.insert(
            segment.id,
            audio_fingerprint(std::slice::from_ref(&segment)),
        );
        let previous = std::collections::BTreeMap::from([(edited.id, edited)]);
        timeline.clips.clear();
        append_master(&p, &prod, &mut timeline, &previous, 99_000).unwrap();
        assert_eq!(timeline.clips.len(), 1);
        assert_eq!(timeline.clips[0].start_ms, 7000);
        assert_eq!(timeline.clips[0].volume, 0.42);
        let scene = Scene {
            audio_anchor: Some(Anchor {
                segment_id: segment.id,
                source: TimeRange {
                    start_ms: 2500,
                    end_ms: 3500,
                },
            }),
            ..Default::default()
        };
        assert_eq!(
            placement(p.id, &prod, &scene, &timeline).unwrap(),
            Some((8500, 1000))
        );
        timeline.clips[0].trim_in_ms = 3000;
        assert!(placement(p.id, &prod, &scene, &timeline).is_err());
    }
    #[test]
    fn missing_manual_master_requires_explicit_rebuild_and_legacy_json_remains_readable() {
        let p = Project::new("Master recovery".into(), None);
        let segment = source();
        let mut prod: Production =
            serde_json::from_value(json!({"title":"Old","scenes":[]})).unwrap();
        assert!(prod.audio_segments.is_empty());
        prod.audio_segments.push(segment.clone());
        prod.audio_master_approved = true;
        prod.assembled_audio_sources.insert(
            segment.id,
            audio_fingerprint(std::slice::from_ref(&segment)),
        );
        let mut timeline = p.timeline.clone();
        assert!(append_master(&p, &prod, &mut timeline, &Default::default(), 0).is_err());
        assert!(timeline.clips.is_empty());
        prod.audio_rebuild_ids.push(segment.id);
        append_master(&p, &prod, &mut timeline, &Default::default(), 0).unwrap();
        assert_eq!(timeline.clips.len(), 1);
        let restored: Production =
            serde_json::from_value(serde_json::to_value(&prod).unwrap()).unwrap();
        assert_eq!(restored.audio_segments[0].source, segment.source);
    }
}
