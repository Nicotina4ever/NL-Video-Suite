//! Optional shared bus gain, sidechain and measured master processing.
#[path = "audio_master.rs"]
pub mod master;
use crate::{model::*, update_project, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Channel {
    pub db: f64,
    pub mute: bool,
    pub solo: bool,
    pub points: Vec<Point>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub time_ms: u64,
    pub db: f64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Document {
    pub enabled: bool,
    pub voice: Channel,
    pub music: Channel,
    pub fx: Channel,
    pub master: Channel,
    /// Explicit routing; "direct" bypasses category buses, retaining master processing.
    /// Historical unassigned video/overlay -> voice, audio -> music remains compatible.
    pub routing: BTreeMap<Uuid, String>,
    pub ducking: Ducking,
    pub finalization: Finalization,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ducking {
    pub enabled: bool,
    pub threshold_db: f64,
    pub ratio: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    /// Negative dB: floor of the dry component bounds the maximum attenuation.
    pub depth: Channel,
}
impl Default for Ducking {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -30.,
            ratio: 8.,
            attack_ms: 35.,
            release_ms: 500.,
            depth: Channel {
                db: -12.,
                ..Default::default()
            },
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Finalization {
    pub enabled: bool,
    pub profile: String,
    pub normalize: bool,
    pub target_lufs: f64,
    pub gain_db: f64,
    pub limiter: bool,
    pub ceiling_dbtp: f64,
}
impl Default for Finalization {
    fn default() -> Self {
        Self {
            enabled: false,
            profile: "custom".into(),
            normalize: false,
            target_lufs: -18.,
            gain_db: 0.,
            limiter: false,
            ceiling_dbtp: -1.5,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Source {
    pub timeline: Timeline,
    pub origin_ms: u64,
}
pub fn processed(p: &Project) -> bool {
    p.audio.enabled && (p.audio.ducking.enabled || p.audio.finalization.enabled)
}
impl Document {
    pub fn channel(&self, key: &str) -> &Channel {
        match key {
            "voice" => &self.voice,
            "music" => &self.music,
            "fx" => &self.fx,
            _ => &self.master,
        }
    }
    pub fn route<'a>(&'a self, track: &Track) -> &'a str {
        self.routing.get(&track.id).map(String::as_str).unwrap_or(
            if track.kind == TrackKind::Audio {
                "music"
            } else {
                "voice"
            },
        )
    }
    pub fn audible(&self, key: &str) -> bool {
        if key == "direct" {
            return !self.master.mute && !(self.voice.solo || self.music.solo || self.fx.solo);
        }
        !self.master.mute
            && !self.channel(key).mute
            && (!(self.voice.solo || self.music.solo || self.fx.solo) || self.channel(key).solo)
    }
}
pub fn validate(d: &Document, _p: &Project) -> anyhow::Result<()> {
    for c in [&d.voice, &d.music, &d.fx, &d.master, &d.ducking.depth] {
        anyhow::ensure!(
            c.db.is_finite() && (-96.0..=24.0).contains(&c.db),
            "Gain audio fuori scala (-96…+24 dB)"
        );
        let mut before = None;
        for v in &c.points {
            anyhow::ensure!(
                v.db.is_finite()
                    && (-96.0..=24.0).contains(&v.db)
                    && v.time_ms <= 86_400_000
                    && before.is_none_or(|b| v.time_ms > b),
                "Automazione: tempi unici crescenti, entro la timeline 24h; livelli -96…+24 dB"
            );
            before = Some(v.time_ms);
        }
    }
    let duck = &d.ducking;
    anyhow::ensure!(
        duck.threshold_db.is_finite()
            && (-60.0..=0.0).contains(&duck.threshold_db)
            && duck.ratio.is_finite()
            && (1.0..=20.0).contains(&duck.ratio)
            && duck.attack_ms.is_finite()
            && (0.01..=2000.).contains(&duck.attack_ms)
            && duck.release_ms.is_finite()
            && (0.01..=9000.).contains(&duck.release_ms),
        "Ducking: parametri fuori dai range FFmpeg"
    );
    anyhow::ensure!(
        (-36.0..=0.0).contains(&duck.depth.db)
            && duck
                .depth
                .points
                .iter()
                .all(|v| (-36.0..=0.0).contains(&v.db))
            && !duck.depth.mute
            && !duck.depth.solo,
        "Profondità ducking: da -36 a 0 dB, senza mute/solo"
    );
    let f = &d.finalization;
    anyhow::ensure!(
        f.target_lufs.is_finite()
            && (-36.0..=-5.0).contains(&f.target_lufs)
            && f.gain_db.is_finite()
            && (-24.0..=24.0).contains(&f.gain_db)
            && f.ceiling_dbtp.is_finite()
            && (-12.0..=0.0).contains(&f.ceiling_dbtp)
            && f.profile.len() <= 100,
        "Finalizzazione: livelli non validi"
    );
    anyhow::ensure!(!d.master.solo, "Solo è disponibile sui bus, non sul master");
    for key in d.routing.values() {
        // Retain routing of temporarily removed tracks for timeline Undo.
        anyhow::ensure!(
            ["voice", "music", "fx", "direct"].contains(&key.as_str()),
            "Routing audio non valido"
        );
    }
    Ok(())
}
/// dB-linear automation, held outside the keyed range. The same contract is
/// scheduled with WebAudio exponential ramps; export evaluates it per sample.
fn expression(c: &Channel, clock: &str) -> String {
    let Some(last) = c.points.last() else {
        return format!("{:.9}", c.db);
    };
    let mut result = format!("{:.9}", last.db);
    for pair in c.points.windows(2).rev() {
        let (a, b) = (&pair[0], &pair[1]);
        let start = a.time_ms as f64 / 1000.;
        let end = b.time_ms as f64 / 1000.;
        result = format!(
            "if(lt({clock},{end:.6}),{:.9}+({:.9})*({clock}-{start:.6})/{:.6},{result})",
            a.db,
            b.db - a.db,
            end - start
        );
    }
    let first = &c.points[0];
    format!(
        "if(lt({clock},{:.6}),{:.9},{result})",
        first.time_ms as f64 / 1000.,
        first.db
    )
}
pub fn filter(
    d: &Document,
    p: &Project,
    clip: &TimelineClip,
    origin_ms: u64,
) -> anyhow::Result<String> {
    if !d.enabled {
        return Ok(String::new());
    }
    validate(d, p)?;
    let track = p
        .timeline
        .tracks
        .iter()
        .find(|t| t.id == clip.track_id)
        .ok_or_else(|| anyhow::anyhow!("Traccia audio assente"))?;
    let key = d.route(track);
    if !d.audible(key) {
        return Ok(",volume=0".into());
    }
    let clock = format!(
        "(t+{:.6})",
        (origin_ms as f64 + clip.start_ms as f64) / 1000.
    );
    let db = if key == "direct" {
        expression(&d.master, &clock)
    } else {
        format!(
            "({})+({})",
            expression(d.channel(key), &clock),
            expression(&d.master, &clock)
        )
    };
    Ok(format!(
        ",aeval=exprs='val(0)*pow(10,({db})/20)|val(1)*pow(10,({db})/20)'"
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    audio: Document,
    expected_updated_at: DateTime<Utc>,
}
/// Shared clip source trim/fade/gain stage for live export and prepared audio.
pub fn clip_filter(
    p: &Project,
    clip: &TimelineClip,
    index: usize,
    buses: bool,
) -> anyhow::Result<String> {
    let tr = &clip.transform;
    let fade = if tr.audio_fade_in_ms > 0 || tr.audio_fade_out_ms > 0 {
        let duration = tr.render_motion_duration_ms.unwrap_or(clip.duration_ms) as f64 / 1000.;
        let origin = tr.render_motion_origin_ms.unwrap_or(clip.start_ms as i64) as f64 / 1000.
            - clip.start_ms as f64 / 1000.;
        let e = crate::compositor::envelope(
            tr.audio_fade_in_ms,
            tr.audio_fade_out_ms,
            duration,
            "t",
            origin,
        );
        format!(",aeval=exprs='val(0)*({e})|val(1)*({e})'")
    } else {
        String::new()
    };
    let mixer = if buses {
        filter(&p.audio, p, clip, p.motion.timeline_origin_ms)?
    } else {
        String::new()
    };
    Ok(format!("[{index}:a]asetpts=PTS-STARTPTS,aresample=48000,aformat=channel_layouts=stereo,volume={:.4}{fade}{mixer},adelay={}:all=1[a{index}]",clip.volume,clip.start_ms))
}
pub async fn save(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(r): Json<Request>,
) -> Result<Json<Project>, ApiError> {
    update_project(&s, id, |p| {
        if p.updated_at != r.expected_updated_at {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "Progetto cambiato: ricarica il mixer".into(),
            ));
        }
        validate(&r.audio, p).map_err(|e| ApiError::bad_request(e.to_string()))?;
        p.audio = r.audio;
        Ok(())
    })
    .await?;
    Ok(Json(s.projects.read().await.get(&id).unwrap().clone()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_mixed_audio_bypasses_category_gains_and_receives_master_once() {
        let mut p = Project::new("Original mix".into(), None);
        let clip:TimelineClip=serde_json::from_value(serde_json::json!({"id":Uuid::new_v4(),"asset_id":Uuid::new_v4(),"track_id":p.timeline.tracks[2].id,"start_ms":0,"trim_in_ms":0,"duration_ms":1000,"volume":1.,"opacity":1.,"transform":{}})).unwrap();
        p.audio.enabled = true;
        p.audio.music.db = -40.;
        p.audio.master.db = -3.;
        p.audio.routing.insert(clip.track_id, "direct".into());
        let f = filter(&p.audio, &p, &clip, 0).unwrap();
        assert!(!f.contains("-40"));
        assert!(!f.contains("+"));
        assert!(f.contains("-3.000000000"));
        p.audio.music.solo = true;
        assert_eq!(filter(&p.audio, &p, &clip, 0).unwrap(), ",volume=0");
    }
    #[test]
    fn advanced_effects_default_off_and_depth_is_bounded() {
        let mut p = Project::new("compat".into(), None);
        assert!(!processed(&p));
        p.audio.enabled = true;
        assert!(!processed(&p));
        p.audio.ducking.enabled = true;
        assert!(processed(&p));
        assert!(validate(&p.audio, &p).is_ok());
        p.audio.ducking.depth.db = 1.;
        assert!(validate(&p.audio, &p).is_err());
        p.audio.ducking.depth.db = -12.;
        p.audio.finalization.target_lufs = f64::NAN;
        assert!(validate(&p.audio, &p).is_err());
    }
    #[test]
    fn old_projects_bypass_and_bus_solo_are_explicit() {
        let p = Project::new("old".into(), None);
        let d: Document = serde_json::from_str("{}").unwrap();
        assert!(!d.enabled);
        assert!(validate(&d, &p).is_ok());
        let mut d = d;
        d.music.solo = true;
        assert!(!d.audible("voice"));
        assert!(d.audible("music"));
        d.master.mute = true;
        assert!(!d.audible("music"));
    }
    #[test]
    fn trimmed_window_keeps_absolute_automation_clock() {
        let mut p = Project::new("window".into(), None);
        p.audio.enabled = true;
        p.audio.voice.points = vec![
            Point {
                time_ms: 0,
                db: -12.,
            },
            Point {
                time_ms: 5000,
                db: 0.,
            },
        ];
        let c: TimelineClip = serde_json::from_value(serde_json::json!({
            "id":Uuid::new_v4(), "asset_id":Uuid::new_v4(), "track_id":p.timeline.tracks[0].id,
            "start_ms":1000, "trim_in_ms":200, "duration_ms":1000,
            "volume":1., "opacity":1., "transform":{}
        }))
        .unwrap();
        let e = filter(&p.audio, &p, &c, 2000).unwrap();
        assert!(e.contains("(t+3.000000)"));
        p.audio.enabled = false;
        assert_eq!(filter(&p.audio, &p, &c, 2000).unwrap(), "");
    }
    #[test]
    fn automation_is_db_linear_and_rejects_duplicate_times() {
        let c = Channel {
            points: vec![
                Point {
                    time_ms: 1000,
                    db: -12.,
                },
                Point {
                    time_ms: 3000,
                    db: 0.,
                },
            ],
            ..Default::default()
        };
        let e = expression(&c, "t");
        assert!(e.contains("/2.000000"));
        assert!(e.contains("if(lt(t,1.000000),-12.000000000"));
        let p = Project::new("test".into(), None);
        let mut d = Document {
            voice: c,
            ..Default::default()
        };
        assert!(validate(&d, &p).is_ok());
        d.voice.points[1].time_ms = 1000;
        assert!(validate(&d, &p).is_err());
    }
}
