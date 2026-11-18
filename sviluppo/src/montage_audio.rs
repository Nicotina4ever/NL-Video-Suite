//! Bounded CPU audio-onset analysis. This is rhythmic evidence, not an infallible
//! musical beat/downbeat/chorus classifier. No ASR, Python or AI service needed.
use crate::{model::*, store, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    io::Read,
    path::Path as FsPath,
    process::{Command, Stdio},
    time::Instant,
};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
pub struct Rhythm {
    pub engine: String,
    pub asset_id: Uuid,
    pub input_bytes: u64,
    pub modified_ns: String,
    pub decoded_ms: u64,
    pub onsets_ms: Vec<u64>,
    pub suggested_bpm: Option<f64>,
    pub regularity: f64,
    pub energy_sections_ms: Vec<u64>,
    pub elapsed_seconds: f64,
    pub working_bins: usize,
    pub notice: String,
}
fn detect(energies: &[f64]) -> (Vec<u64>, Option<f64>, f64, Vec<u64>) {
    if energies.len() < 30 {
        return (vec![], None, 0., vec![]);
    }
    let mut novelty = vec![0.; energies.len()];
    for i in 1..energies.len() {
        let from = i.saturating_sub(25);
        let mean = energies[from..i].iter().sum::<f64>() / (i - from) as f64;
        novelty[i] = (energies[i] - mean).max(0.);
    }
    let mean = novelty.iter().sum::<f64>() / novelty.len() as f64;
    let deviation =
        (novelty.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / novelty.len() as f64).sqrt();
    let threshold = (mean + deviation * 0.6).max(0.00002);
    let mut hits: Vec<usize> = vec![];
    for i in 1..novelty.len() - 1 {
        if novelty[i] > threshold && novelty[i] >= novelty[i - 1] && novelty[i] > novelty[i + 1] {
            if let Some(last) = hits.last_mut() {
                if i - *last < 10 {
                    if novelty[i] > novelty[*last] {
                        *last = i;
                    }
                    continue;
                }
            }
            hits.push(i);
        }
    }
    let onsets = hits.iter().map(|i| *i as u64 * 20).collect::<Vec<_>>();
    let mut intervals = onsets
        .windows(2)
        .map(|w| (w[1] - w[0]) as f64)
        .filter(|v| (200.0..=2000.).contains(v))
        .collect::<Vec<_>>();
    intervals.sort_by(f64::total_cmp);
    let period = intervals.get(intervals.len() / 2).copied().unwrap_or(0.);
    let regularity = if intervals.len() < 4 {
        0.
    } else {
        intervals
            .iter()
            .filter(|v| (**v - period).abs() <= period * 0.15)
            .count() as f64
            / intervals.len() as f64
    };
    let bpm = if regularity >= 0.6 {
        Some((60000. / period * 10.).round() / 10.)
    } else {
        None
    };
    // Four-second energy differences only, no semantic labels such as chorus.
    let sections = energies
        .chunks(200)
        .enumerate()
        .scan(None, |prior, (i, part)| {
            let energy = part.iter().sum::<f64>() / part.len() as f64;
            let changed =
                prior.is_some_and(|v: f64| energy > v * 2.0 || (v > 0.00002 && energy < v / 2.));
            *prior = Some(energy);
            Some((i, changed))
        })
        .filter(|(_, changed)| *changed)
        .map(|(i, _)| i as u64 * 4000)
        .collect();
    (onsets, bpm, regularity, sections)
}
fn analyze(path: &FsPath, id: Uuid, cache: &FsPath) -> anyhow::Result<(Rhythm, bool)> {
    std::fs::create_dir_all(
        cache
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Cartella cache audio assente"))?,
    )?;
    let stat = std::fs::metadata(path)?;
    let modified = stat
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos()
        .to_string();
    if let Ok(old) = store::read_json::<Rhythm>(cache) {
        if old.engine == "energy-onsets-20ms-v1"
            && old.asset_id == id
            && old.input_bytes == stat.len()
            && old.modified_ns == modified
        {
            return Ok((old, true));
        }
    }
    let now = Instant::now();
    let mut decoder = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-xerror", "-threads", "2", "-i"])
        .arg(path)
        .args([
            "-map", "0:a:0", "-t", "7200", "-ac", "1", "-ar", "8000", "-f", "s16le", "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut out = decoder.stdout.take().unwrap();
    let mut bytes = [0u8; 320];
    let mut energies = Vec::new();
    let result = (|| -> anyhow::Result<()> {
        loop {
            match out.read_exact(&mut bytes) {
                Ok(()) => {
                    anyhow::ensure!(energies.len() < 360_001, "Audio oltre il limite di due ore");
                    energies.push(
                        bytes
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|b| {
                                let x = i16::from_le_bytes([b[0], b[1]]) as f64 / 32768.;
                                x * x
                            })
                            .sum::<f64>()
                            / 160.,
                    );
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    })();
    drop(out);
    if result.is_err() {
        let _ = decoder.kill();
    }
    let finished = decoder.wait_with_output()?;
    result?;
    anyhow::ensure!(
        finished.status.success(),
        "Analisi audio: {}",
        String::from_utf8_lossy(&finished.stderr)
    );
    let (onsets, bpm, regularity, sections) = detect(&energies);
    let rhythm=Rhythm{engine:"energy-onsets-20ms-v1".into(),asset_id:id,input_bytes:stat.len(),modified_ns:modified,decoded_ms:energies.len() as u64*20,onsets_ms:onsets,suggested_bpm:bpm,regularity,energy_sections_ms:sections,elapsed_seconds:now.elapsed().as_secs_f64(),working_bins:energies.len(),notice:"Attacchi energetici proposti, non beat/downbeat musicali certificati. Voce, sincopi e rumore possono produrre falsi positivi; BPM può essere metà/doppio. Ascolta, correggi o usa la griglia manuale. Nessun cambio automatico della timeline.".into()};
    store::write_json(cache, &rhythm)?;
    Ok((rhythm, false))
}
pub async fn start(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let p = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    let aid = p.montage.music_asset_id.ok_or_else(|| {
        ApiError::bad_request("Scegli e salva una colonna sonora prima di analizzarla")
    })?;
    let asset = p
        .assets
        .iter()
        .find(|a| a.id == aid && a.kind == AssetKind::Audio)
        .ok_or_else(|| ApiError::bad_request("Audio non presente nel progetto"))?;
    if asset
        .media
        .as_ref()
        .and_then(|m| m.duration_ms)
        .is_none_or(|v| v > 7_200_000)
    {
        return Err(ApiError::bad_request(
            "Analisi ritmica: durata audio nota, massimo due ore",
        ));
    }
    let project_root = store::project_dir(&state.data_dir, id);
    let path = project_root.join("assets").join(&asset.file_name);
    let cache = project_root
        .join("analysis")
        .join("montage-audio")
        .join(format!("{aid}.json"));
    let job = Job::queued(
        JobKind::PreparePreview,
        Some(id),
        "Analisi ritmica CPU in coda: attacchi ed energia, senza IA",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let jid = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(
            jid,
            JobStatus::Running,
            "Lettura audio mono8kHz, finestre20ms, memoria limitata",
            vec![],
        )
        .await;
        let result = tokio::task::spawn_blocking(move || analyze(&path, aid, &cache)).await;
        match result {
            Ok(Ok((rhythm, cached))) => {
                if let Some(j) = app.jobs.write().await.get_mut(&jid) {
                    j.result = Some(json!({"rhythm":rhythm,"cache_hit":cached}));
                }
                app.set_job(
                    jid,
                    JobStatus::Succeeded,
                    "Attacchi pronti da ascoltare: scegli esplicitamente se usarli",
                    vec![],
                )
                .await;
            }
            e => {
                let error = match e {
                    Ok(Err(e)) => e.to_string(),
                    Err(e) => e.to_string(),
                    _ => unreachable!(),
                };
                app.set_job(jid, JobStatus::Failed, error, vec![]).await;
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_regular_pulses_are_detected_without_calling_them_human_accuracy() {
        let mut bins = vec![0.00001; 500];
        for i in (25..500).step_by(25) {
            bins[i] = 0.1;
        }
        let (hits, bpm, regularity, _) = detect(&bins);
        assert_eq!(hits.len(), 19);
        assert_eq!(bpm, Some(120.));
        assert_eq!(regularity, 1.);
        assert_eq!(hits[0], 500);
    }
    #[test]
    fn silence_never_creates_beats() {
        let (h, b, _, s) = detect(&vec![0.; 300]);
        assert!(h.is_empty());
        assert!(b.is_none());
        assert!(s.is_empty());
    }
}
