//! Derived per-project SQLite index: FTS5 speech search over editorial words.
//! `project.json` stays the source of truth. `index.db` lives inside the
//! project folder, is rebuilt from the record whenever its fingerprint changes,
//! is never followed by renderers, and disappears with project deletion.
use crate::{model::Project, store, transcript_core, ApiError, AppState};
use anyhow::Context;
use axum::{
    extract::{Path as RoutePath, Query, State},
    Json,
};
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{Mutex, OnceLock},
};
use uuid::Uuid;

const SCHEMA_VERSION: i32 = 2;
const DB_NAME: &str = "index.db";

/// Rebuilds are best-effort and cheap to redo, so only one runs per project.
fn rebuilding() -> &'static Mutex<Vec<Uuid>> {
    static SLOT: OnceLock<Mutex<Vec<Uuid>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(Vec::new()))
}

fn fingerprint(project: &Project) -> String {
    json!({
        "updated_at": project.updated_at,
        "transcripts": project.transcripts.iter().map(|t| (t.id, t.words.len(), t.segments.len())).collect::<Vec<_>>(),
        "revision": project.longform.revision,
        "corrections": project.longform.corrections.len(),
        "chapters": project.longform.chapters.len(),
        "decisions": project.longform.decisions.len(),
    })
    .to_string()
}

fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "busy_timeout", 10_000u64)?;
    Ok(conn)
}

fn create_schema(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "BEGIN;
         DROP TABLE IF EXISTS meta;
         DROP TABLE IF EXISTS words;
         DROP TABLE IF EXISTS segments;
         DROP TABLE IF EXISTS chapters;
         DROP TABLE IF EXISTS decisions;
         DROP TABLE IF EXISTS words_search;
         CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE words(
             transcript_id TEXT NOT NULL,
             word_id TEXT NOT NULL,
             asset_id TEXT NOT NULL,
             start_ms INTEGER NOT NULL,
             end_ms INTEGER NOT NULL,
             raw_text TEXT NOT NULL,
             corrected_text TEXT,
             language TEXT NOT NULL,
             origin_engine TEXT NOT NULL,
             align_status TEXT NOT NULL,
             timing_unit TEXT NOT NULL,
             PRIMARY KEY(transcript_id, word_id)
         ) WITHOUT ROWID;
         CREATE INDEX words_asset_time ON words(asset_id, start_ms);
         CREATE TABLE segments(
             transcript_id TEXT NOT NULL,
             asset_id TEXT NOT NULL,
             start_ms INTEGER NOT NULL,
             end_ms INTEGER NOT NULL,
             text TEXT NOT NULL
         );
         CREATE INDEX segments_time ON segments(transcript_id, start_ms);
         CREATE TABLE chapters(
             id TEXT PRIMARY KEY,
             asset_id TEXT NOT NULL,
             transcript_id TEXT,
             title TEXT NOT NULL,
             summary TEXT NOT NULL,
             start_ms INTEGER NOT NULL,
             end_ms INTEGER NOT NULL,
             approved INTEGER NOT NULL,
             engine TEXT NOT NULL,
             model TEXT NOT NULL
         );
         CREATE INDEX chapters_asset_time ON chapters(asset_id, start_ms);
         CREATE TABLE decisions(
             id TEXT PRIMARY KEY,
             asset_id TEXT NOT NULL,
             action TEXT NOT NULL,
             reason TEXT NOT NULL,
             start_ms INTEGER NOT NULL,
             end_ms INTEGER NOT NULL
         );
         CREATE INDEX decisions_asset_time ON decisions(asset_id, start_ms);
         CREATE VIRTUAL TABLE words_search USING fts5(
             text,
             transcript_id UNINDEXED,
             word_id UNINDEXED,
             asset_id UNINDEXED,
             start_ms UNINDEXED,
             end_ms UNINDEXED,
             tokenize='unicode61 remove_diacritics 2'
         );
         COMMIT;",
    )?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

fn populate(conn: &mut Connection, project: &Project) -> anyhow::Result<()> {
    let tx = conn.transaction()?;
    {
        let mut word =
            tx.prepare_cached("INSERT INTO words VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)")?;
        let mut segment = tx.prepare_cached("INSERT INTO segments VALUES (?1,?2,?3,?4,?5)")?;
        let mut search = tx.prepare_cached(
            "INSERT INTO words_search(text,transcript_id,word_id,asset_id,start_ms,end_ms) VALUES (?1,?2,?3,?4,?5,?6)",
        )?;
        for t in &project.transcripts {
            let meta = project.transcript_metadata.get(&t.id);
            let engine = meta.and_then(|m| m["engine"].as_str()).unwrap_or("unknown");
            let aligned = meta.is_some_and(|m| m["alignment_completed"] == true);
            for w in transcript_core::words(t, &project.longform.corrections, engine, aligned) {
                let text = w
                    .corrected_text
                    .clone()
                    .unwrap_or_else(|| w.raw_text.clone());
                word.execute(params![
                    t.id.to_string(),
                    w.id.to_string(),
                    t.asset_id.to_string(),
                    w.start_ms as i64,
                    w.end_ms as i64,
                    w.raw_text,
                    w.corrected_text,
                    w.language,
                    w.origin_engine,
                    w.align_status,
                    w.timing_unit
                ])?;
                if !text.trim().is_empty() {
                    search.execute(params![
                        text,
                        t.id.to_string(),
                        w.id.to_string(),
                        t.asset_id.to_string(),
                        w.start_ms as i64,
                        w.end_ms as i64
                    ])?;
                }
            }
            for s in &t.segments {
                segment.execute(params![
                    t.id.to_string(),
                    t.asset_id.to_string(),
                    s.start_ms as i64,
                    s.end_ms as i64,
                    s.text
                ])?;
            }
        }
        let mut chapter = tx.prepare_cached(
            "INSERT OR REPLACE INTO chapters VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        )?;
        for c in &project.longform.chapters {
            chapter.execute(params![
                c.id.to_string(),
                c.asset_id.to_string(),
                c.transcript_id.to_string(),
                c.title,
                c.summary,
                c.source.start_ms as i64,
                c.source.end_ms as i64,
                c.approved as i64,
                c.engine,
                c.model
            ])?;
        }
        let mut decision =
            tx.prepare_cached("INSERT OR REPLACE INTO decisions VALUES (?1,?2,?3,?4,?5,?6)")?;
        for d in &project.longform.decisions {
            decision.execute(params![
                d.id.to_string(),
                d.asset_id.to_string(),
                d.action,
                d.reason,
                d.range.start_ms as i64,
                d.range.end_ms as i64
            ])?;
        }
    }
    tx.execute(
        "INSERT INTO meta(key,value) VALUES('fingerprint',?1)",
        [fingerprint(project)],
    )?;
    tx.commit()?;
    Ok(())
}

fn build(path: &Path, project: &Project) -> anyhow::Result<Connection> {
    // Never create the parent: a missing project folder means the project is
    // gone, and derived data must not resurrect deleted records.
    anyhow::ensure!(
        path.parent().is_some_and(|parent| parent.is_dir()),
        "cartella progetto assente: nessun indice ricostruito"
    );
    let want = fingerprint(project);
    if let Ok(conn) = open(path) {
        let usable = conn
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
            .unwrap_or_default()
            == SCHEMA_VERSION;
        if usable {
            let stored = conn
                .query_row(
                    "SELECT value FROM meta WHERE key='fingerprint'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap_or_default();
            if stored == want {
                return Ok(conn);
            }
        }
        drop(conn);
    }
    // The index is derived data: a damaged, stale or wrong-version file is
    // replaced from the record, never repaired by hand.
    std::fs::remove_file(path).ok();
    std::fs::remove_file(format!("{}-wal", path.display())).ok();
    std::fs::remove_file(format!("{}-shm", path.display())).ok();
    let mut conn = open(path)?;
    create_schema(&conn)?;
    populate(&mut conn, project)?;
    Ok(conn)
}

/// Fire-and-forget after a project save: rebuild only when the fingerprint
/// changed and only if no rebuild for this project is already in flight.
pub fn rebuild_if_stale(data_dir: &Path, project: &Project) {
    let id = project.id;
    let claimed = rebuilding()
        .lock()
        .map(|mut busy| {
            if busy.contains(&id) {
                false
            } else {
                busy.push(id);
                true
            }
        })
        .unwrap_or(false);
    if !claimed {
        return;
    }
    let path = store::project_dir(data_dir, id).join(DB_NAME);
    let project = project.clone();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(&path, &project)));
    let result: anyhow::Result<Connection> = match outcome {
        Ok(Ok(conn)) => Ok(conn),
        Ok(Err(error)) => Err(error),
        Err(_) => Err(anyhow::anyhow!(
            "indice SQLite: panic durante la ricostruzione"
        )),
    };
    if let Err(error) = result {
        tracing::error!(%error, project_id=%id, "indice SQLite non ricostruito: la ricerca userà l'indice precedente");
    }
    if let Ok(mut busy) = rebuilding().lock() {
        busy.retain(|x| *x != id);
    }
}

fn quote_match(query: &str) -> Option<String> {
    let tokens = query
        .split_whitespace()
        .take(12)
        .filter_map(|token| {
            // FTS5 wildcards must survive escaping: "stem"* outside the quotes.
            if let Some(stem) = token.strip_suffix('*') {
                let stem = stem.replace('"', "\"\"");
                if stem.is_empty() {
                    return None;
                }
                return Some(format!("\"{stem}\"*"));
            }
            let escaped = token.replace('"', "\"\"");
            (!escaped.is_empty()).then(|| format!("\"{escaped}\""))
        })
        .collect::<Vec<_>>();
    (!tokens.is_empty()).then(|| tokens.join(" "))
}

#[derive(Debug)]
pub struct Hit {
    pub text: String,
    pub transcript_id: String,
    pub word_id: String,
    pub asset_id: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub segment_text: Option<String>,
    pub segment_start_ms: Option<i64>,
}

pub fn search(
    data_dir: &Path,
    project: &Project,
    query: &str,
    limit: usize,
    offset: usize,
) -> anyhow::Result<(Vec<Hit>, usize)> {
    let match_query = quote_match(query).context("La ricerca non contiene termini validi")?;
    let path = store::project_dir(data_dir, project.id).join(DB_NAME);
    let conn = build(&path, project)?;
    let total: usize = conn.query_row(
        "SELECT count(*) FROM words_search WHERE words_search MATCH ?1",
        [&match_query],
        |row| row.get(0),
    )?;
    let found = {
        let mut rows = conn.prepare_cached(
            "SELECT text,transcript_id,word_id,asset_id,start_ms,end_ms FROM words_search WHERE words_search MATCH ?1 ORDER BY rank LIMIT ?2 OFFSET ?3",
        )?;
        let fetched = rows.query_map(
            params![match_query, limit.clamp(1, 200) as i64, offset as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )?;
        fetched.collect::<Result<Vec<_>, _>>()?
    };
    let mut hits = Vec::new();
    for (text, transcript_id, word_id, asset_id, start_ms, end_ms) in found {
        let segment: Option<(String, i64)> = conn
            .query_row(
                "SELECT text,start_ms FROM segments WHERE transcript_id=?1 AND start_ms<=?2 ORDER BY start_ms DESC LIMIT 1",
                params![transcript_id, start_ms],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        hits.push(Hit {
            text,
            transcript_id,
            word_id,
            asset_id,
            start_ms,
            end_ms,
            segment_text: segment.as_ref().map(|s| s.0.clone()),
            segment_start_ms: segment.map(|s| s.1),
        });
    }
    Ok((hits, total))
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

pub async fn view(
    State(state): State<AppState>,
    RoutePath(id): RoutePath<Uuid>,
    Query(request): Query<SearchQuery>,
) -> Result<Json<Value>, ApiError> {
    let project = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    let query = request.q;
    let echo = query.clone();
    let limit = request.limit.unwrap_or(80);
    let offset = request.offset.unwrap_or(0);
    let data = state.data_dir.as_ref().clone();
    let (hits, total) =
        tokio::task::spawn_blocking(move || search(&data, &project, &query, limit, offset))
            .await
            .map_err(|e| ApiError::internal(e.into()))?
            .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(Json(json!({
        "query": echo,
        "total": total,
        "offset": offset,
        "hits": hits.iter().map(|h| json!({
            "word_id": h.word_id,
            "transcript_id": h.transcript_id,
            "asset_id": h.asset_id,
            "text": h.text,
            "start_ms": h.start_ms,
            "end_ms": h.end_ms,
            "segment_text": h.segment_text,
            "segment_start_ms": h.segment_start_ms,
        })).collect::<Vec<_>>(),
        "note": "Indice derivato da project.json: nessuna modifica ai raw ASR o alle correzioni.",
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Transcript, TranscriptSegment};
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    fn project_with_words() -> (PathBuf, Project) {
        let data = std::env::temp_dir().join(format!("nl-index-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&data).unwrap();
        let mut p = Project::new("Indice QA".into(), None);
        let asset = Uuid::new_v4();
        let t = Transcript {
            id: Uuid::new_v4(),
            asset_id: asset,
            language: "it".into(),
            segments: vec![
                TranscriptSegment {
                    start_ms: 0,
                    end_ms: 10_000,
                    text: "Il caffè di questa mattina è amaro".into(),
                },
                TranscriptSegment {
                    start_ms: 10_000,
                    end_ms: 20_000,
                    text: "Secondo segmento senza accenti".into(),
                },
            ],
            words: vec![
                TranscriptSegment {
                    start_ms: 0,
                    end_ms: 500,
                    text: "Il".into(),
                },
                TranscriptSegment {
                    start_ms: 1_000,
                    end_ms: 2_000,
                    text: "caffè".into(),
                },
                TranscriptSegment {
                    start_ms: 3_000,
                    end_ms: 4_000,
                    text: "amaro".into(),
                },
            ],
            imported_at: chrono::Utc::now(),
        };
        p.transcripts.push(t.clone());
        p.transcript_metadata
            .insert(t.id, json!({"engine":"whisperx"}));
        store::save_project(&data, &p).unwrap();
        (data, p)
    }
    #[test]
    fn accent_insensitive_search_returns_timed_hits_with_segment_context() {
        let (data, p) = project_with_words();
        let (hits, total) = search(&data, &p, "caffe", 10, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(hits[0].text, "caffè");
        assert_eq!(hits[0].start_ms, 1_000);
        assert!(hits[0].segment_text.as_deref().unwrap().contains("amaro"));
        std::fs::remove_dir_all(&data).unwrap();
    }
    #[test]
    fn stale_fingerprint_rebuild_picks_up_new_corrections() {
        let (data, mut p) = project_with_words();
        let (_, first) = search(&data, &p, "amaro", 10, 0).unwrap();
        assert_eq!(first, 1);
        let word_id =
            transcript_core::words(&p.transcripts[0], &BTreeMap::new(), "whisperx", false)
                .iter()
                .find(|w| w.raw_text == "caffè")
                .unwrap()
                .id;
        p.longform
            .corrections
            .insert(word_id, "tè alla menta".into());
        p.updated_at = chrono::Utc::now();
        store::save_project(&data, &p).unwrap();
        let (hits, _) = search(&data, &p, "menta", 10, 0).unwrap();
        assert_eq!(hits[0].text, "tè alla menta");
        std::fs::remove_dir_all(&data).unwrap();
    }
    #[test]
    fn damaged_index_is_replaced_not_fatal() {
        let (data, p) = project_with_words();
        let db = store::project_dir(&data, p.id).join(DB_NAME);
        search(&data, &p, "caffè", 10, 0).unwrap();
        std::fs::write(&db, b"definitely not a database").unwrap();
        let (hits, _) = search(&data, &p, "caffè", 10, 0).unwrap();
        assert_eq!(hits.len(), 1);
        std::fs::remove_dir_all(&data).unwrap();
    }
    #[test]
    fn queries_without_terms_are_refused() {
        let (data, p) = project_with_words();
        assert!(search(&data, &p, "   ", 10, 0).is_err());
        std::fs::remove_dir_all(&data).unwrap();
    }
    #[test]
    fn prefix_wildcard_survives_escaping() {
        let (data, p) = project_with_words();
        let (_, total) = search(&data, &p, "caff*", 10, 0).unwrap();
        assert_eq!(total, 1);
        let error = search(&data, &p, "*", 10, 0).unwrap_err().to_string();
        assert!(error.contains("termini validi"));
        std::fs::remove_dir_all(&data).unwrap();
    }
    #[test]
    fn rebuild_is_deduplicated_and_survives_reopen() {
        let (data, p) = project_with_words();
        rebuild_if_stale(&data, &p);
        rebuild_if_stale(&data, &p);
        let db = store::project_dir(&data, p.id).join(DB_NAME);
        assert!(db.is_file());
        let (hits, _) = search(&data, &p, "caffè", 10, 0).unwrap();
        assert_eq!(hits.len(), 1);
        std::fs::remove_dir_all(&data).unwrap();
    }
    #[test]
    fn chapters_and_decisions_are_indexed_for_later_retrieval() {
        let (data, mut p) = project_with_words();
        let t = p.transcripts[0].clone();
        p.longform.chapters.push(crate::longform_analysis::Chapter {
            id: Uuid::new_v4(),
            asset_id: t.asset_id,
            transcript_id: t.id,
            source: crate::time_map::TimeRange {
                start_ms: 0,
                end_ms: 5_000,
            },
            title: "Apertura".into(),
            summary: "Il caffè della mattina".into(),
            keywords: vec!["caffè".into()],
            evidence: vec![],
            approved: true,
            engine: "local_llm".into(),
            model: "test".into(),
        });
        p.longform.decisions.push(crate::longform::Decision {
            id: Uuid::new_v4(),
            asset_id: t.asset_id,
            range: crate::time_map::TimeRange {
                start_ms: 60_000,
                end_ms: 62_000,
            },
            action: "exclude".into(),
            reason: "pausa lunga".into(),
        });
        store::save_project(&data, &p).unwrap();
        rebuild_if_stale(&data, &p);
        let db = store::project_dir(&data, p.id).join(DB_NAME);
        let conn = Connection::open(&db).unwrap();
        let chapters: i64 = conn
            .query_row("SELECT count(*) FROM chapters WHERE approved=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        let decisions: i64 = conn
            .query_row(
                "SELECT count(*) FROM decisions WHERE action='exclude'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!((chapters, decisions), (1, 1));
        drop(conn);
        std::fs::remove_dir_all(&data).unwrap();
    }
}
