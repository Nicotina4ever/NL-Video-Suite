//! Persisted bounded deltas, not copies of a multi-hour caption document.
use crate::subtitle_studio::{Document, Style, Unit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct History {
    pub undo: Vec<Change>,
    pub redo: Vec<Change>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Header {
    enabled: bool,
    language: String,
    style: Style,
}
impl Header {
    fn of(d: &Document) -> Self {
        Self {
            enabled: d.enabled,
            language: d.language.clone(),
            style: d.style.clone(),
        }
    }
    fn set(&self, d: &mut Document) {
        d.enabled = self.enabled;
        d.language = self.language.clone();
        d.style = self.style.clone();
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    before: Header,
    after: Header,
    // Missing entries mean insertion/deletion. Only changed units are retained.
    units_before: BTreeMap<Uuid, Option<Unit>>,
    units_after: BTreeMap<Uuid, Option<Unit>>,
}
impl History {
    pub fn record(&mut self, before: &Document, after: &Document) {
        let a = before
            .units
            .iter()
            .map(|u| (u.id, u))
            .collect::<BTreeMap<_, _>>();
        let b = after
            .units
            .iter()
            .map(|u| (u.id, u))
            .collect::<BTreeMap<_, _>>();
        let mut change = Change {
            before: Header::of(before),
            after: Header::of(after),
            units_before: BTreeMap::new(),
            units_after: BTreeMap::new(),
        };
        for id in a.keys().chain(b.keys()) {
            if a.get(id) != b.get(id) {
                change
                    .units_before
                    .insert(*id, a.get(id).map(|u| (*u).clone()));
                change
                    .units_after
                    .insert(*id, b.get(id).map(|u| (*u).clone()));
            }
        }
        if change.before == change.after && change.units_before.is_empty() {
            return;
        }
        self.redo.clear();
        self.undo.push(change);
        // Explicit disk/RAM budget, including large bulk edits. No unbounded snapshots.
        while self.undo.len() > 24
            || serde_json::to_vec(&self.undo).map_or(true, |v| v.len() > 8 * 1024 * 1024)
        {
            self.undo.remove(0);
            if self.undo.is_empty() {
                break;
            }
        }
    }
    pub fn step(&mut self, document: &mut Document, redo: bool) -> anyhow::Result<()> {
        let from = if redo { &mut self.redo } else { &mut self.undo };
        let change = from
            .last()
            .ok_or_else(|| anyhow::anyhow!("Nessuna modifica sottotitoli da ripristinare"))?;
        let (expected, replacement, header, new_header) = if redo {
            (
                &change.units_before,
                &change.units_after,
                &change.before,
                &change.after,
            )
        } else {
            (
                &change.units_after,
                &change.units_before,
                &change.after,
                &change.before,
            )
        };
        anyhow::ensure!(
            Header::of(document) == *header,
            "Stile cambiato: cronologia non applicabile"
        );
        for (id, value) in expected {
            anyhow::ensure!(
                document.units.iter().find(|u| u.id == *id) == value.as_ref(),
                "Cue cambiato: cronologia non applicabile"
            );
        }
        document.units.retain(|u| !replacement.contains_key(&u.id));
        document
            .units
            .extend(replacement.values().flatten().cloned());
        document
            .units
            .sort_by_key(|u| (u.asset_id, u.language.clone(), u.source.start_ms, u.id));
        new_header.set(document);
        document.revision += 1;
        let change = from.pop().expect("checked change");
        if redo {
            self.undo.push(change);
        } else {
            self.redo.push(change);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deltas_persist_and_revisions_never_roll_back() {
        let mut a = Document::default();
        let before = a.clone();
        a.style.x = 0.2;
        a.revision = 1;
        let mut h = History::default();
        h.record(&before, &a);
        let bytes = serde_json::to_vec(&h).unwrap();
        let mut h: History = serde_json::from_slice(&bytes).unwrap();
        h.step(&mut a, false).unwrap();
        assert_eq!(a.style.x, 0.5);
        assert_eq!(a.revision, 2);
        h.step(&mut a, true).unwrap();
        assert_eq!(a.style.x, 0.2);
        assert_eq!(a.revision, 3);
        a.style.y = 0.1;
        assert!(h.step(&mut a, false).is_err());
    }
}
