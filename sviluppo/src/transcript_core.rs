//! Stable editorial words/cues over an immutable raw ASR transcript.
use crate::model::Transcript;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    pub id: Uuid,
    pub transcript_id: Uuid,
    pub asset_id: Uuid,
    pub raw_text: String,
    pub corrected_text: Option<String>,
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker_id: Option<String>,
    pub language: String,
    pub origin_engine: String,
    pub align_status: String,
    pub timing_unit: String,
}
pub fn words(
    t: &Transcript,
    corrections: &BTreeMap<Uuid, String>,
    engine: &str,
    aligned: bool,
) -> Vec<Word> {
    let (values, unit) = if t.words.is_empty() {
        (&t.segments, "segment")
    } else {
        (&t.words, "word")
    };
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let id = Uuid::new_v5(
                &t.id,
                format!("{i}:{}:{}:{}", v.start_ms, v.end_ms, v.text).as_bytes(),
            );
            Word {
                id,
                transcript_id: t.id,
                asset_id: t.asset_id,
                raw_text: v.text.clone(),
                corrected_text: corrections.get(&id).cloned(),
                start_ms: v.start_ms,
                end_ms: v.end_ms,
                speaker_id: None,
                language: t.language.clone(),
                origin_engine: engine.into(),
                align_status: if corrections.get(&id).is_some_and(|text| text != &v.text) {
                    "edited_needs_review"
                } else if unit == "segment" {
                    "segment_only"
                } else if aligned {
                    "aligned"
                } else {
                    "unknown"
                }
                .into(),
                timing_unit: unit.into(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TranscriptSegment;
    #[test]
    fn correction_preserves_raw_ids_times_and_unknown_alignment() {
        let t = Transcript {
            id: Uuid::new_v4(),
            asset_id: Uuid::new_v4(),
            language: "it".into(),
            segments: vec![TranscriptSegment {
                start_ms: 100,
                end_ms: 900,
                text: "Caffe".into(),
            }],
            words: vec![],
            imported_at: chrono::Utc::now(),
        };
        let first = words(&t, &BTreeMap::new(), "unknown", false);
        let corrections = BTreeMap::from([(first[0].id, "Caffè".into())]);
        let corrected = words(&t, &corrections, "unknown", false);
        assert_eq!(first[0].id, corrected[0].id);
        assert_eq!(corrected[0].raw_text, "Caffe");
        assert_eq!(corrected[0].corrected_text.as_deref(), Some("Caffè"));
        assert_eq!(corrected[0].timing_unit, "segment");
        assert_eq!(first[0].align_status, "segment_only");
        assert_eq!(corrected[0].align_status, "edited_needs_review");
        assert_eq!(t.segments[0].text, "Caffe");
    }
}
