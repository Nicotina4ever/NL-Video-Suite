//! Shared integer-time contract. Intervals are half-open and mappings are plural:
//! repeating a source passage on multiple tracks must never silently pick one.
use crate::model::{Project, TimelineClip};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_TIME_MS: u64 = 86_400_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TimeRange {
    pub start_ms: u64,
    pub end_ms: u64,
}
impl TimeRange {
    pub fn new(start_ms: u64, end_ms: u64) -> Result<Self> {
        ensure!(
            start_ms < end_ms && end_ms <= MAX_TIME_MS,
            "Intervallo non valido: 0 ≤ inizio < fine ≤ 24 ore"
        );
        Ok(Self { start_ms, end_ms })
    }
    pub fn intersection(self, other: Self) -> Option<Self> {
        let start_ms = self.start_ms.max(other.start_ms);
        let end_ms = self.end_ms.min(other.end_ms);
        (start_ms < end_ms).then_some(Self { start_ms, end_ms })
    }
    pub fn duration(self) -> u64 {
        self.end_ms - self.start_ms
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FrameRate {
    pub numerator: u32,
    pub denominator: u32,
}
impl Default for FrameRate {
    fn default() -> Self {
        Self {
            numerator: 30,
            denominator: 1,
        }
    }
}
impl FrameRate {
    pub fn validate(self) -> Result<()> {
        ensure!(
            self.denominator > 0
                && self.denominator <= 10000
                && self.numerator <= 120000
                && self.numerator >= self.denominator
                && u64::from(self.numerator) <= 120 * u64::from(self.denominator),
            "Cadenza razionale: da 1 a 120 fps, numeratore ≤120000 e denominatore 1–10000"
        );
        Ok(())
    }
    pub fn frame_at_ms(self, ms: u64) -> Result<u64> {
        self.validate()?;
        ensure!(ms <= MAX_TIME_MS, "Tempo oltre 24 ore");
        let div = u128::from(self.denominator) * 1000;
        Ok(((u128::from(ms) * u128::from(self.numerator) + div / 2) / div) as u64)
    }
    pub fn ms_at_frame(self, frame: u64) -> Result<u64> {
        self.validate()?;
        let value = (u128::from(frame) * u128::from(self.denominator) * 1000
            + u128::from(self.numerator) / 2)
            / u128::from(self.numerator);
        ensure!(value <= u128::from(MAX_TIME_MS), "Frame oltre 24 ore");
        Ok(value as u64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mapping {
    pub clip_id: Uuid,
    pub asset_id: Uuid,
    pub track_id: Uuid,
    pub source: TimeRange,
    pub timeline: TimeRange,
    pub visible: bool,
}
fn checked_clip(c: &TimelineClip) -> Result<(TimeRange, TimeRange)> {
    let source = TimeRange::new(
        c.trim_in_ms,
        c.trim_in_ms
            .checked_add(c.duration_ms)
            .ok_or_else(|| anyhow::anyhow!("Overflow sorgente"))?,
    )?;
    let timeline = TimeRange::new(
        c.start_ms,
        c.start_ms
            .checked_add(c.duration_ms)
            .ok_or_else(|| anyhow::anyhow!("Overflow timeline"))?,
    )?;
    Ok((source, timeline))
}
pub fn source_to_timeline(p: &Project, asset: Uuid, range: TimeRange) -> Result<Vec<Mapping>> {
    TimeRange::new(range.start_ms, range.end_ms)?;
    let mut out = vec![];
    for c in p.timeline.clips.iter().filter(|c| c.asset_id == asset) {
        let (source, _) = checked_clip(c)?;
        if let Some(overlap) = source.intersection(range) {
            let timeline = TimeRange {
                start_ms: c.start_ms + overlap.start_ms - source.start_ms,
                end_ms: c.start_ms + overlap.end_ms - source.start_ms,
            };
            out.push(Mapping {
                clip_id: c.id,
                asset_id: asset,
                track_id: c.track_id,
                source: overlap,
                timeline,
                visible: p
                    .timeline
                    .tracks
                    .iter()
                    .any(|t| t.id == c.track_id && t.visible),
            });
        }
    }
    out.sort_by_key(|m| (m.timeline.start_ms, m.clip_id));
    Ok(out)
}
pub fn timeline_to_source(p: &Project, range: TimeRange) -> Result<Vec<Mapping>> {
    TimeRange::new(range.start_ms, range.end_ms)?;
    let mut out = vec![];
    for c in &p.timeline.clips {
        let (_, timeline) = checked_clip(c)?;
        if let Some(overlap) = timeline.intersection(range) {
            out.push(Mapping {
                clip_id: c.id,
                asset_id: c.asset_id,
                track_id: c.track_id,
                source: TimeRange {
                    start_ms: c.trim_in_ms + overlap.start_ms - timeline.start_ms,
                    end_ms: c.trim_in_ms + overlap.end_ms - timeline.start_ms,
                },
                timeline: overlap,
                visible: p
                    .timeline
                    .tracks
                    .iter()
                    .any(|t| t.id == c.track_id && t.visible),
            });
        }
    }
    Ok(out)
}
pub fn subtract(duration: u64, excluded: &[TimeRange]) -> Result<Vec<TimeRange>> {
    TimeRange::new(0, duration)?;
    let mut ranges = excluded.to_vec();
    ranges.sort_by_key(|r| r.start_ms);
    let mut cursor = 0;
    let mut kept = vec![];
    for r in ranges {
        TimeRange::new(r.start_ms, r.end_ms)?;
        ensure!(r.end_ms <= duration, "Esclusione oltre la durata sorgente");
        if cursor < r.start_ms {
            kept.push(TimeRange {
                start_ms: cursor,
                end_ms: r.start_ms,
            });
        }
        cursor = cursor.max(r.end_ms);
    }
    if cursor < duration {
        kept.push(TimeRange {
            start_ms: cursor,
            end_ms: duration,
        });
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::VisualTransform;
    fn clip(asset: Uuid, track: Uuid, start: u64, trim: u64, duration: u64) -> TimelineClip {
        TimelineClip {
            id: Uuid::new_v4(),
            asset_id: asset,
            track_id: track,
            start_ms: start,
            trim_in_ms: trim,
            duration_ms: duration,
            label: None,
            volume: 1.,
            opacity: 1.,
            transform: VisualTransform::default(),
        }
    }
    #[test]
    fn repeated_trimmed_source_maps_to_all_occurrences_and_round_trips() {
        let mut p = Project::new("Test".into(), None);
        let asset = Uuid::new_v4();
        let track = p.timeline.tracks[0].id;
        p.timeline.clips = vec![
            clip(asset, track, 1000, 5000, 2000),
            clip(asset, track, 9000, 5000, 2000),
        ];
        let source = TimeRange::new(5500, 6500).unwrap();
        let mapped = source_to_timeline(&p, asset, source).unwrap();
        assert_eq!(mapped.len(), 2);
        assert_eq!(mapped[0].timeline, TimeRange::new(1500, 2500).unwrap());
        assert_eq!(mapped[1].timeline, TimeRange::new(9500, 10500).unwrap());
        for m in mapped {
            assert_eq!(
                timeline_to_source(&p, m.timeline).unwrap()[0].source,
                source
            );
        }
        assert!(
            source_to_timeline(&p, asset, TimeRange::new(7000, 8000).unwrap())
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn exclusions_merge_overlaps_but_preserve_gaps_and_all_edges() {
        let keep = subtract(
            10000,
            &[
                TimeRange::new(2000, 4000).unwrap(),
                TimeRange::new(3000, 5000).unwrap(),
                TimeRange::new(9000, 10000).unwrap(),
            ],
        )
        .unwrap();
        assert_eq!(
            keep,
            vec![
                TimeRange::new(0, 2000).unwrap(),
                TimeRange::new(5000, 9000).unwrap()
            ]
        );
        assert!(subtract(100, &[TimeRange::new(50, 101).unwrap()]).is_err());
        assert!(subtract(100, &[TimeRange::new(0, 100).unwrap()])
            .unwrap()
            .is_empty());
    }
    #[test]
    fn supported_fps_round_trip_within_one_frame_at_two_hours() {
        for (n, d) in [
            (24000, 1001),
            (24, 1),
            (25, 1),
            (30000, 1001),
            (30, 1),
            (50, 1),
            (60000, 1001),
            (60, 1),
        ] {
            let rate = FrameRate {
                numerator: n,
                denominator: d,
            };
            for ms in [0, 1, 17, 999, 3_600_001, 7_200_000] {
                let again = rate.ms_at_frame(rate.frame_at_ms(ms).unwrap()).unwrap();
                assert!(again.abs_diff(ms) <= ((1000 * d / n) + 1) as u64);
            }
        }
        assert!(FrameRate {
            numerator: 30,
            denominator: 0
        }
        .frame_at_ms(4)
        .is_err());
        assert!(TimeRange::new(5, 5).is_err());
    }
}
