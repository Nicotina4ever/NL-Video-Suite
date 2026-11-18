//! Streaming, regenerable display peaks; never buffers a long decoded soundtrack.
use crate::{model::*, store, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Instant, UNIX_EPOCH},
};
use uuid::Uuid;

const RATE: u64 = 48_000;
const BIN_MS: u64 = 100;
const MAX_MS: u64 = 6 * 60 * 60 * 1000;
#[derive(Serialize, Deserialize)]
pub struct Peaks {
    pub version: u8,
    pub bin_ms: u64,
    pub sample_rate: u64,
    pub decoded_samples: u64,
    pub duration_ms: u64,
    pub peaks: Vec<[i16; 2]>,
    pub decoder_peak_rss_bytes: Option<u64>,
}
struct Source {
    path: PathBuf,
    cache: PathBuf,
    duration: u64,
    bytes: u64,
    modified: u128,
}
async fn source(state: &AppState, project: Uuid, asset: Uuid) -> Result<Source, ApiError> {
    let a = state
        .projects
        .read()
        .await
        .get(&project)
        .and_then(|p| p.assets.iter().find(|a| a.id == asset))
        .cloned()
        .ok_or_else(|| ApiError::not_found("Media non trovato"))?;
    let duration = a.media.as_ref().and_then(|m| m.duration_ms).unwrap_or(0);
    if !a.media.as_ref().is_some_and(|m| m.has_audio) || duration == 0 || duration > MAX_MS {
        return Err(ApiError::bad_request(
            "Waveform: richiede audio di durata nota, massimo6ore",
        ));
    }
    let owned = store::project_dir(&state.data_dir, project)
        .join("assets")
        .canonicalize()
        .map_err(|e| ApiError::internal(e.into()))?;
    let path = owned
        .join(&a.file_name)
        .canonicalize()
        .map_err(|e| ApiError::internal(e.into()))?;
    if !path.starts_with(&owned) {
        return Err(ApiError::bad_request("Media fuori dal progetto"));
    }
    let stat = std::fs::metadata(&path).map_err(|e| ApiError::internal(e.into()))?;
    let modified = stat
        .modified()
        .ok()
        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // Owned media identity is UUID+stat, not a claim that equal-sized files are identical.
    let key = Uuid::new_v5(
        &asset,
        format!("peaks-pcm48-mono-v1:{}:{modified}", stat.len()).as_bytes(),
    );
    Ok(Source {
        path,
        cache: state
            .data_dir
            .join("previews")
            .join(project.to_string())
            .join(format!("{asset}-{key}-peaks.json")),
        duration,
        bytes: stat.len(),
        modified,
    })
}

#[cfg(windows)]
pub(crate) fn child_rss(child: &std::process::Child) -> Option<u64> {
    use std::os::windows::io::AsRawHandle;
    #[repr(C)]
    struct Counters {
        cb: u32,
        faults: u32,
        peak: usize,
        working: usize,
        peak_paged: usize,
        paged: usize,
        peak_nonpaged: usize,
        nonpaged: usize,
        pagefile: usize,
        peak_pagefile: usize,
    }
    #[link(name = "psapi")]
    extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut Counters,
            bytes: u32,
        ) -> i32;
    }
    let mut c = Counters {
        cb: std::mem::size_of::<Counters>() as u32,
        faults: 0,
        peak: 0,
        working: 0,
        peak_paged: 0,
        paged: 0,
        peak_nonpaged: 0,
        nonpaged: 0,
        pagefile: 0,
        peak_pagefile: 0,
    };
    // Read counters only from the decoder process this module launched.
    if unsafe {
        GetProcessMemoryInfo(
            child.as_raw_handle(),
            &mut c,
            std::mem::size_of::<Counters>() as u32,
        )
    } != 0
    {
        Some(c.peak as u64)
    } else {
        None
    }
}
#[cfg(not(windows))]
pub(crate) fn child_rss(_: &std::process::Child) -> Option<u64> {
    None
}

fn pcm_peaks(mut reader: impl Read, mut progress: impl FnMut(u64)) -> anyhow::Result<Peaks> {
    let mut bytes = [0_u8; (RATE * BIN_MS / 1000 * 2) as usize];
    let mut peaks = Vec::new();
    let mut samples = 0;
    loop {
        let mut length = 0;
        while length < bytes.len() {
            let n = reader.read(&mut bytes[length..])?;
            if n == 0 {
                break;
            }
            length += n;
        }
        if length == 0 {
            break;
        }
        anyhow::ensure!(length % 2 == 0, "PCM incompleto");
        let mut lo = i16::MAX;
        let mut hi = i16::MIN;
        for b in bytes[..length].as_chunks::<2>().0 {
            let value = i16::from_le_bytes([b[0], b[1]]);
            lo = lo.min(value);
            hi = hi.max(value);
        }
        peaks.push([lo, hi]);
        samples += length as u64 / 2;
        anyhow::ensure!(
            samples <= RATE * (MAX_MS / 1000 + 1),
            "Waveform oltre il limite6ore"
        );
        if peaks.len() % 10 == 0 {
            progress(samples);
        }
        if length < bytes.len() {
            break;
        }
    }
    anyhow::ensure!(samples > 0, "Nessun campione audio decodificato");
    progress(samples);
    Ok(Peaks {
        version: 1,
        bin_ms: BIN_MS,
        sample_rate: RATE,
        decoded_samples: samples,
        duration_ms: samples * 1000 / RATE,
        peaks,
        decoder_peak_rss_bytes: None,
    })
}
fn build(src: &Source, progress: tokio::sync::watch::Sender<u64>) -> anyhow::Result<(Peaks, bool)> {
    if src.cache.is_file() {
        let peaks: Peaks = store::read_json(&src.cache)?;
        anyhow::ensure!(peaks.version == 1, "Versione waveform non valida");
        return Ok((peaks, true));
    }
    std::fs::create_dir_all(src.cache.parent().expect("cache parent"))?;
    let mut command = Command::new("ffmpeg");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .args(["-nostdin", "-v", "error", "-threads", "2", "-i"])
        .arg(&src.path)
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-ac",
            "1",
            "-ar",
            "48000",
            "-c:a",
            "pcm_s16le",
            "-f",
            "s16le",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stderr = child.stderr.take().expect("stderr");
    // Drain bounded diagnostic text concurrently, never risk a full error pipe deadlock.
    let log = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.take(8192).read_to_string(&mut text);
        text
    });
    let stdout = child.stdout.take().expect("stdout");
    let mut peak = 0;
    let mut last = Instant::now();
    let parsed = pcm_peaks(stdout, |n| {
        if peak == 0 || last.elapsed() >= std::time::Duration::from_millis(250) {
            progress.send_replace(n);
            peak = peak.max(child_rss(&child).unwrap_or(0));
            last = Instant::now();
        }
    });
    if parsed.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let error = log.join().unwrap_or_default();
    anyhow::ensure!(status.success(), "Waveform decoder: {error}");
    let mut peaks = parsed?;
    peaks.decoder_peak_rss_bytes = (peak > 0).then_some(peak);
    let after = std::fs::metadata(&src.path)?;
    let modified = after.modified()?.duration_since(UNIX_EPOCH)?.as_nanos();
    anyhow::ensure!(
        after.len() == src.bytes && modified == src.modified,
        "Media cambiato durante l'analisi: cache non salvata"
    );
    store::write_json(&src.cache, &peaks)?;
    Ok((peaks, false))
}
pub async fn prepare(
    State(state): State<AppState>,
    Path((pid, aid)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let _ = source(&state, pid, aid).await?;
    let job = Job::queued(
        JobKind::PreparePreview,
        Some(pid),
        "Waveform indicizzata: lettura audio a memoria limitata",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let jid = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        let started = Instant::now();
        app.set_job(
            jid,
            JobStatus::Running,
            "Preparazione waveform a picchi PCM; nessuna IA",
            vec![],
        )
        .await;
        let result=async{
            let src=source(&app,pid,aid).await.map_err(|e|anyhow::anyhow!(e.1))?;let duration=src.duration;
            let (tx,mut rx)=tokio::sync::watch::channel(0_u64);let work=tokio::task::spawn_blocking(move||build(&src,tx));tokio::pin!(work);
            loop{tokio::select!{
                done=&mut work=>break done.map_err(anyhow::Error::from)?,
                change=rx.changed()=>{if change.is_ok(){let samples=*rx.borrow_and_update();app.set_job(jid,JobStatus::Running,format!("Waveform: {:.1}s audio decodificati / {:.1}s",samples as f64/RATE as f64,duration as f64/1000.),vec![]).await;}else{break work.await.map_err(anyhow::Error::from)?;}}
            }}
        }.await;
        match result {
            Ok((peaks, hit)) => {
                let mut jobs = app.jobs.write().await;
                if let Some(j) = jobs.get_mut(&jid) {
                    j.result = Some(
                        json!({"url":format!("/api/v1/projects/{pid}/assets/{aid}/peaks"),"cache_hit":hit,"bins":peaks.peaks.len(),"duration_ms":peaks.duration_ms,"bin_ms":peaks.bin_ms,"elapsed_seconds":started.elapsed().as_secs_f64(),"decoder_peak_rss_bytes":peaks.decoder_peak_rss_bytes,"source_identity":"owned_uuid_size_mtime_not_content_hash"}),
                    );
                }
                drop(jobs);
                app.set_job(
                    jid,
                    JobStatus::Succeeded,
                    if hit {
                        "Waveform già in cache"
                    } else {
                        "Waveform indicizzata pronta"
                    },
                    vec![],
                )
                .await;
            }
            Err(e) => {
                app.set_job(jid, JobStatus::Failed, e.to_string(), vec![])
                    .await
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}
pub async fn data(
    State(state): State<AppState>,
    Path((pid, aid)): Path<(Uuid, Uuid)>,
) -> Result<Json<Peaks>, ApiError> {
    let src = source(&state, pid, aid).await?;
    if !src.cache.is_file() {
        return Err(ApiError::not_found("Waveform non ancora preparata"));
    }
    Ok(Json(
        store::read_json(&src.cache).map_err(ApiError::internal)?,
    ))
}
pub async fn overview(
    state: AppState,
    pid: Uuid,
    aid: Uuid,
) -> Result<axum::response::Response, ApiError> {
    let src = source(&state, pid, aid).await?;
    let _permit = state.worker.acquire().await.expect("worker open");
    let (tx, _rx) = tokio::sync::watch::channel(0);
    let (peaks, _) = tokio::task::spawn_blocking(move || build(&src, tx))
        .await
        .map_err(|e| ApiError::internal(e.into()))?
        .map_err(ApiError::internal)?;
    let mut d = String::new();
    for x in 0..1024 {
        let from = x * peaks.peaks.len() / 1024;
        let to = ((x + 1) * peaks.peaks.len() / 1024)
            .max(from + 1)
            .min(peaks.peaks.len());
        let mut lo = 0_i16;
        let mut hi = 0_i16;
        for p in &peaks.peaks[from.min(peaks.peaks.len() - 1)..to] {
            lo = lo.min(p[0]);
            hi = hi.max(p[1]);
        }
        d.push_str(&format!(
            "M{x},{:.2}V{:.2}",
            32. - hi as f64 / 32768. * 31.,
            32. - lo as f64 / 32768. * 31.
        ));
    }
    Ok(([(header::CONTENT_TYPE,"image/svg+xml"),(header::CACHE_CONTROL,"no-cache")],format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1024\" height=\"64\" viewBox=\"0 0 1024 64\"><path d=\"{d}\" stroke=\"#8be4e6\" stroke-width=\"1\"/></svg>")).into_response())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn peaks_keep_signed_extrema_partial_bin_and_real_duration() {
        let mut bytes = vec![];
        for i in 0..5000 {
            bytes.extend_from_slice(
                &(if i == 12 {
                    i16::MIN
                } else if i == 4801 {
                    i16::MAX
                } else {
                    120_i16
                })
                .to_le_bytes(),
            );
        }
        let p = pcm_peaks(&bytes[..], |_| {}).unwrap();
        assert_eq!(p.peaks, vec![[i16::MIN, 120], [120, i16::MAX]]);
        assert_eq!(p.duration_ms, 104);
        assert_eq!(p.decoded_samples, 5000);
        assert!(pcm_peaks(&[1_u8][..], |_| {}).is_err());
        assert!(pcm_peaks(&[][..], |_| {}).is_err());
    }
}
