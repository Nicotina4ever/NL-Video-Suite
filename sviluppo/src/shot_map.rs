//! CPU scene-change candidates over owned video or an existing time-compatible proxy.
//! No AI, automatic timeline cuts, semantic certainty or arbitrary FFmpeg arguments.
use crate::{model::*, store, update_project, ApiError, AppState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{Command, Stdio},
    time::{Instant, UNIX_EPOCH},
};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Params {
    pub threshold: f64,
    pub min_shot_ms: u64,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            threshold: 10.,
            min_shot_ms: 500,
        }
    }
}
impl Params {
    fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.threshold.is_finite()
                && (4. ..=50.).contains(&self.threshold)
                && (200..=10000).contains(&self.min_shot_ms),
            "Shot map: soglia4–50 e distanza minima200–10000ms"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Decision {
    pub asset_id: Uuid,
    pub input_key: Uuid,
    pub boundary_ms: u64,
    /// Explicit user marker, not a detector result or inferred timestamp.
    #[serde(default)]
    pub manual: bool,
    pub accepted: bool,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Boundary {
    pub at_ms: u64,
    pub score: f64,
}
#[derive(Clone, Serialize, Deserialize)]
struct Stamp {
    bytes: u64,
    modified: u128,
}
impl PartialEq for Stamp {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes && self.modified == other.modified
    }
}
fn stamp(path: &std::path::Path) -> anyhow::Result<Stamp> {
    let m = std::fs::metadata(path)?;
    Ok(Stamp {
        bytes: m.len(),
        modified: m.modified()?.duration_since(UNIX_EPOCH)?.as_nanos(),
    })
}
struct Source {
    asset_id: Uuid,
    original: PathBuf,
    input: PathBuf,
    original_stamp: Stamp,
    input_stamp: Stamp,
    mode: &'static str,
    directory: PathBuf,
    duration: u64,
    fps: f64,
    params: Params,
    path: PathBuf,
    engine: PathBuf,
    engine_signature: String,
}
#[derive(Serialize, Deserialize)]
struct Map {
    version: u8,
    input_key: Uuid,
    asset_id: Uuid,
    params: Params,
    analysis_input_sha256: String,
    engine_signature: String,
    source_mode: String,
    original_stamp: Stamp,
    input_stamp: Stamp,
    duration_ms: u64,
    analysis_frame_rate: f64,
    decoded_frames: u64,
    decoder_peak_rss_bytes: Option<u64>,
    analysis_seconds: f64,
    boundaries: Vec<Boundary>,
}
#[derive(Serialize, Deserialize)]
struct Partial {
    input_key: Uuid,
    last_ms: u64,
    decoded_frames: u64,
    raw: Vec<Boundary>,
}
async fn source(
    state: &AppState,
    pid: Uuid,
    aid: Uuid,
    params: Params,
) -> Result<Source, ApiError> {
    params
        .validate()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let asset = state
        .projects
        .read()
        .await
        .get(&pid)
        .and_then(|p| p.assets.iter().find(|a| a.id == aid))
        .cloned()
        .ok_or_else(|| ApiError::not_found("Media assente"))?;
    let duration = asset
        .media
        .as_ref()
        .and_then(|m| m.duration_ms)
        .unwrap_or(0);
    if asset.kind != AssetKind::Video || duration == 0 || duration > 21_600_000 {
        return Err(ApiError::bad_request(
            "Shot map: seleziona un video di durata nota, massimo6ore",
        ));
    }
    let owned = store::project_dir(&state.data_dir, pid)
        .join("assets")
        .canonicalize()
        .map_err(|e| ApiError::internal(e.into()))?;
    let original = owned
        .join(&asset.file_name)
        .canonicalize()
        .map_err(|e| ApiError::internal(e.into()))?;
    if !original.starts_with(&owned) {
        return Err(ApiError::bad_request("Video fuori dal progetto"));
    }
    let original_stamp = stamp(&original).map_err(ApiError::internal)?;
    let directory = state.data_dir.join("previews").join(pid.to_string());
    let proxy = directory.join(format!("{aid}-playback-v2.mp4"));
    let mut input = original.clone();
    let mut mode = "owned_original";
    let mut fps = asset
        .media
        .as_ref()
        .and_then(|m| m.frame_rate)
        .unwrap_or(30.);
    if proxy.is_file() {
        let candidate = proxy
            .canonicalize()
            .map_err(|e| ApiError::internal(e.into()))?;
        let root = directory
            .canonicalize()
            .map_err(|e| ApiError::internal(e.into()))?;
        if candidate.starts_with(root)
            && stamp(&candidate).map_err(ApiError::internal)?.modified >= original_stamp.modified
        {
            let probe = candidate.clone();
            let meta = tokio::task::spawn_blocking(move || crate::media::probe_media(&probe))
                .await
                .map_err(|e| ApiError::internal(e.into()))?
                .map_err(ApiError::internal)?;
            if let Some(m) = meta.filter(|m| {
                m.width.is_some() && m.duration_ms.is_some_and(|d| d.abs_diff(duration) <= 100)
            }) {
                fps = m.frame_rate.unwrap_or(fps);
                input = candidate;
                mode = "existing_playback_proxy";
            }
        }
    }
    let input_stamp = stamp(&input).map_err(ApiError::internal)?;
    let engine = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|p| {
            p.join(if cfg!(windows) {
                "ffmpeg.exe"
            } else {
                "ffmpeg"
            })
        })
        .find(|p| p.is_file())
        .ok_or_else(|| ApiError::bad_request("FFmpeg non disponibile"))?;
    let engine_stamp = stamp(&engine).map_err(ApiError::internal)?;
    let engine_signature = format!(
        "ffmpeg-scdet-v1:{}:{}",
        engine_stamp.bytes, engine_stamp.modified
    );
    let key = Uuid::new_v5(
        &aid,
        format!("scdet320-v1:{}:{}", params.threshold, params.min_shot_ms).as_bytes(),
    );
    let path = directory.join(format!("{aid}-{key}-shots.json"));
    Ok(Source {
        asset_id: aid,
        original,
        input,
        original_stamp,
        input_stamp,
        mode,
        directory,
        duration,
        fps,
        params,
        path,
        engine,
        engine_signature,
    })
}
fn valid_map(src: &Source) -> anyhow::Result<Option<Map>> {
    if !src.path.is_file() {
        return Ok(None);
    }
    anyhow::ensure!(
        std::fs::metadata(&src.path)?.len() <= 32 * 1024 * 1024,
        "Cache shot map oltre il limite32MiB"
    );
    let m: Map = store::read_json(&src.path)?;
    Ok((m.version == 1
        && m.original_stamp == src.original_stamp
        && m.input_stamp == src.input_stamp
        && m.source_mode == src.mode
        && m.engine_signature == src.engine_signature)
        .then_some(m))
}
fn suppress(raw: &[Boundary], min: u64, duration: u64) -> Vec<Boundary> {
    let mut sorted = raw
        .iter()
        .filter(|b| b.at_ms >= min && b.at_ms + min <= duration && b.score.is_finite())
        .cloned()
        .collect::<Vec<_>>();
    sorted.sort_by_key(|b| b.at_ms);
    let mut out: Vec<Boundary> = vec![];
    for b in sorted {
        if let Some(last) = out.last_mut() {
            if b.at_ms - last.at_ms < min {
                if b.score > last.score {
                    *last = b;
                }
                continue;
            }
        }
        out.push(b);
    }
    out
}
fn time_from_line(line: &str) -> Option<u64> {
    let v = line
        .split("pts_time:")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()?;
    (v.is_finite() && (0. ..21601.).contains(&v)).then_some((v * 1000.).round() as u64)
}
fn input_hash(src: &Source) -> anyhow::Result<String> {
    std::fs::create_dir_all(&src.directory)?;
    let id_key = Uuid::new_v5(
        &src.asset_id,
        format!(
            "input:{}:{}",
            src.input_stamp.bytes, src.input_stamp.modified
        )
        .as_bytes(),
    );
    let hash_path = src
        .directory
        .join(format!("{}-{id_key}-analysis-sha.json", src.asset_id));
    let sha = if hash_path.is_file() {
        let v: Value = store::read_json(&hash_path)?;
        v["sha256"]
            .as_str()
            .filter(|s| s.len() == 64)
            .ok_or_else(|| anyhow::anyhow!("Identità media corrotta"))?
            .to_owned()
    } else {
        let hash = crate::storage::media_sha256(&src.input)?;
        store::write_json(&hash_path, &json!({"sha256":hash}))?;
        hash
    };
    Ok(sha)
}
fn map_key(src: &Source, sha: &str) -> Uuid {
    Uuid::new_v5(
        &src.asset_id,
        format!(
            "scdet320-v1:{sha}:{}:{}:{}",
            src.params.threshold, src.params.min_shot_ms, src.engine_signature
        )
        .as_bytes(),
    )
}
fn build(
    src: &Source,
    progress: tokio::sync::watch::Sender<(u64, u64)>,
) -> anyhow::Result<(Map, bool, bool)> {
    if let Some(m) = valid_map(src)? {
        return Ok((m, true, false));
    }
    let sha = input_hash(src)?;
    let input_key = map_key(src, &sha);
    let partial_path = src.path.with_extension("partial.json");
    let mut partial = Partial {
        input_key,
        last_ms: 0,
        decoded_frames: 0,
        raw: vec![],
    };
    if partial_path.is_file() {
        anyhow::ensure!(
            std::fs::metadata(&partial_path)?.len() <= 32 * 1024 * 1024,
            "Checkpoint shot map oltre il limite32MiB"
        );
        let old: Partial = store::read_json(&partial_path)?;
        if old.input_key == input_key
            && old.last_ms < src.duration
            && old.raw.len() <= 100_000
            && old.raw.iter().all(|b| {
                b.at_ms <= old.last_ms && b.score.is_finite() && (0. ..=100.).contains(&b.score)
            })
        {
            partial = old;
        }
    }
    let resumed = partial.last_ms > 0;
    let resume_at = partial.last_ms;
    let start_ms = resume_at.saturating_sub(1000);
    let started = Instant::now();
    let mut command = Command::new(&src.engine);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.args(["-nostdin","-v","error","-copyts","-start_at_zero","-ss",&format!("{:.3}",start_ms as f64/1000.),"-threads","2","-i"]).arg(&src.input).args(["-an","-sn","-filter_threads","1","-vf",&format!("scale=320:320:force_original_aspect_ratio=decrease:force_divisible_by=2:flags=area,format=yuv420p,scdet=t={},metadata=mode=print:key=lavfi.scd.score:file=-:direct=1",src.params.threshold),"-f","null","-"]).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stderr = child.stderr.take().expect("stderr");
    let log = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.take(8192).read_to_string(&mut s);
        s
    });
    let reader = BufReader::new(child.stdout.take().expect("stdout"));
    let mut at = 0;
    let mut peak = 0;
    let mut wall = Instant::now();
    let mut checkpoint = Instant::now();
    let parsed = (|| -> anyhow::Result<()> {
        for line in reader.lines() {
            let line = line?;
            anyhow::ensure!(line.len() < 8192, "Metadata frame non valida");
            if let Some(ms) = time_from_line(&line) {
                at = ms;
                if at > resume_at || !resumed {
                    partial.last_ms = at;
                    partial.decoded_frames += 1;
                }
                if wall.elapsed().as_millis() >= 250 || peak == 0 {
                    progress.send_replace((partial.last_ms, partial.decoded_frames));
                    peak = peak.max(crate::waveform::child_rss(&child).unwrap_or(0));
                    wall = Instant::now();
                }
            }
            if let Some(value) = line.strip_prefix("lavfi.scd.score=") {
                if let Ok(score) = value.trim().parse::<f64>() {
                    if score.is_finite()
                        && score >= src.params.threshold
                        && (at > resume_at || !resumed)
                    {
                        partial.raw.push(Boundary { at_ms: at, score });
                        anyhow::ensure!(
                            partial.raw.len() <= 100_000,
                            "Troppi candidati: aumenta la soglia e riprova"
                        );
                    }
                }
            }
            if checkpoint.elapsed().as_secs() >= 5 {
                store::write_json(&partial_path, &partial)?;
                checkpoint = Instant::now();
            }
        }
        Ok(())
    })();
    if parsed.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let errors = log.join().unwrap_or_default();
    store::write_json(&partial_path, &partial)?;
    parsed?;
    anyhow::ensure!(status.success(), "Shot map FFmpeg: {errors}");
    anyhow::ensure!(
        stamp(&src.original)? == src.original_stamp && stamp(&src.input)? == src.input_stamp,
        "Media cambiato durante la shot map"
    );
    let boundaries = suppress(&partial.raw, src.params.min_shot_ms, src.duration);
    anyhow::ensure!(
        partial.decoded_frames > 0,
        "Nessun frame video decodificato"
    );
    anyhow::ensure!(
        boundaries.len() <= 20_000,
        "Limite20mila inquadrature superato"
    );
    let map = Map {
        version: 1,
        input_key,
        asset_id: src.asset_id,
        params: src.params.clone(),
        analysis_input_sha256: sha,
        engine_signature: src.engine_signature.clone(),
        source_mode: src.mode.into(),
        original_stamp: src.original_stamp.clone(),
        input_stamp: src.input_stamp.clone(),
        duration_ms: src.duration,
        analysis_frame_rate: src.fps,
        decoded_frames: partial.decoded_frames,
        decoder_peak_rss_bytes: (peak > 0).then_some(peak),
        analysis_seconds: started.elapsed().as_secs_f64(),
        boundaries,
    };
    store::write_json(&src.path, &map)?;
    let _ = std::fs::remove_file(partial_path);
    Ok((map, false, resumed))
}
pub async fn prepare(
    State(state): State<AppState>,
    Path((pid, aid)): Path<(Uuid, Uuid)>,
    Json(params): Json<Params>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let _ = source(&state, pid, aid, params.clone()).await?;
    let changed = state
        .projects
        .read()
        .await
        .get(&pid)
        .is_none_or(|p| p.longform.shot_parameters.get(&aid) != Some(&params));
    if changed {
        let selected = params.clone();
        update_project(&state, pid, |p| {
            p.longform.shot_parameters.insert(aid, selected);
            p.longform.revision += 1;
            Ok(())
        })
        .await?;
    }
    let job = Job::queued(
        JobKind::PreparePreview,
        Some(pid),
        "Shot map CPU in coda · nessun taglio automatico",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let jid = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        let start = Instant::now();
        app.set_job(
            jid,
            JobStatus::Running,
            "Identità media e lettura inquadrature CPU",
            vec![],
        )
        .await;
        let result=async{let src=source(&app,pid,aid,params).await.map_err(|e|anyhow::anyhow!(e.1))?;let duration=src.duration;let (tx,mut rx)=tokio::sync::watch::channel((0,0));let task=tokio::task::spawn_blocking(move||build(&src,tx));tokio::pin!(task);loop{tokio::select!{result=&mut task=>break result.map_err(anyhow::Error::from)?,changed=rx.changed()=>{if changed.is_ok(){let (at,frames)=*rx.borrow_and_update();app.set_job(jid,JobStatus::Running,format!("Shot map: {:.1}s / {:.1}s · {frames} frame letti",at as f64/1000.,duration as f64/1000.),vec![]).await;}else{break task.await.map_err(anyhow::Error::from)?;}}}}}.await;
        match result {
            Ok((map, hit, resumed)) => {
                let mut jobs = app.jobs.write().await;
                if let Some(j) = jobs.get_mut(&jid) {
                    j.result = Some(
                        json!({"asset_id":aid,"input_key":map.input_key,"boundaries":map.boundaries.len(),"cache_hit":hit,"resumed_partial":resumed,"source_mode":map.source_mode,"analysis_seconds":map.analysis_seconds,"wall_seconds":start.elapsed().as_secs_f64(),"decoder_peak_rss_bytes":map.decoder_peak_rss_bytes}),
                    );
                }
                drop(jobs);
                app.set_job(
                    jid,
                    JobStatus::Succeeded,
                    if hit {
                        "Shot map recuperata dalla cache"
                    } else {
                        "Shot map pronta · candidati da verificare"
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
pub async fn view(
    State(state): State<AppState>,
    Path((pid, aid)): Path<(Uuid, Uuid)>,
    Query(params): Query<Params>,
) -> Result<Json<Value>, ApiError> {
    let src = source(&state, pid, aid, params).await?;
    let map = tokio::task::spawn_blocking(move || valid_map(&src))
        .await
        .map_err(|e| ApiError::internal(e.into()))?
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Shot map non preparata o media cambiato"))?;
    let reviews = state
        .projects
        .read()
        .await
        .get(&pid)
        .map(|p| {
            p.longform
                .shot_decisions
                .iter()
                .filter(|d| d.asset_id == aid && d.input_key == map.input_key)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(Json(
        json!({"map":map,"reviews":reviews,"limitations":["Cambi visivi candidati, non analisi semantica o verità di montaggio.","Movimenti, flash, grafiche e dissolvenze possono produrre falsi o mancare.","Il proxy può avere meno frame della sorgente; controlla il player originale prima di tagliare.","Nessun punto viene applicato alla timeline automaticamente."]}),
    ))
}
const SAMPLE_W: usize = 80;
const SAMPLE_H: usize = 46;
const SAMPLE_BYTES: usize = SAMPLE_W * SAMPLE_H;
/// Bounded local evidence, never a semantic classifier or an automatic edit.
fn window_metrics(frames: &[&[u8]]) -> Value {
    let delta = |a: &[u8], b: &[u8], dx: isize, dy: isize| {
        let mut sum = 0u64;
        for y in 4..SAMPLE_H - 4 {
            for x in 4..SAMPLE_W - 4 {
                let shifted = (y as isize + dy) as usize * SAMPLE_W + (x as isize + dx) as usize;
                sum += a[y * SAMPLE_W + x].abs_diff(b[shifted]) as u64;
            }
        }
        sum as f64 / ((SAMPLE_H - 8) * (SAMPLE_W - 8)) as f64 / 2.55
    };
    let changes = frames
        .windows(2)
        .map(|f| delta(f[0], f[1], 0, 0))
        .collect::<Vec<_>>();
    let total: f64 = changes.iter().sum();
    let peak = changes.iter().copied().fold(0., f64::max);
    let share = if total > 0. { peak / total } else { 0. };
    let first = frames[0];
    let last = frames[frames.len() - 1];
    let endpoints = delta(first, last, 0, 0);
    let mut compensated = 0.;
    for f in frames.windows(2) {
        let mut best = delta(f[0], f[1], 0, 0);
        for dy in (-4..=4).step_by(2) {
            for dx in (-4..=4).step_by(2) {
                best = best.min(delta(f[0], f[1], dx, dy));
            }
        }
        compensated += best;
    }
    let translation_reduction = if total > 0. {
        (1. - compensated / total).clamp(0., 1.)
    } else {
        0.
    };
    let mut changed = 0;
    for ty in 0..4 {
        for tx in 0..8 {
            let mut sum = 0u64;
            let mut count = 0;
            for y in ty * SAMPLE_H / 4..(ty + 1) * SAMPLE_H / 4 {
                for x in tx * SAMPLE_W / 8..(tx + 1) * SAMPLE_W / 8 {
                    sum += first[y * SAMPLE_W + x].abs_diff(last[y * SAMPLE_W + x]) as u64;
                    count += 1;
                }
            }
            if sum as f64 / count as f64 / 2.55 >= 6. {
                changed += 1;
            }
        }
    }
    let area = changed as f64 / 32.;
    let mut hints = vec![];
    if peak >= 4. && endpoints < peak * 0.35 {
        hints.push("transient_flash_or_return");
    }
    if total > 2. && translation_reduction >= 0.55 {
        hints.push("translation_like_motion");
    }
    if endpoints >= 2. && area > 0. && area <= 0.5 {
        hints.push("localized_change_or_overlay");
    }
    if endpoints >= 5. && share < 0.4 && changes.iter().filter(|&&v| v >= 0.5).count() >= 4 {
        hints.push("distributed_gradual_change");
    }
    if peak >= 4. && share >= 0.45 && endpoints >= 4. {
        hints.push("abrupt_visual_change");
    }
    if hints.is_empty() {
        hints.push("ambiguous_or_low_change");
    }
    json!({"adjacent_mae_percent":changes,"endpoint_mae_percent":endpoints,
        "peak_step_share":share,"changed_endpoint_tile_fraction":area,
        "translation_reduction":translation_reduction,"hints":hints})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inspect {
    pub center_ms: u64,
    #[serde(default)]
    pub params: Params,
}
fn inspect_window(src: &Source, center: u64) -> anyhow::Result<Value> {
    let sha = input_hash(src)?;
    let key = map_key(src, &sha);
    let cache = src.directory.join(format!(
        "{}-{key}-{center}-shot-inspection-v1.json",
        src.asset_id
    ));
    if cache.is_file() {
        anyhow::ensure!(
            std::fs::metadata(&cache)?.len() < 64 * 1024,
            "Diagnostico troppo grande"
        );
        let v: Value = store::read_json(&cache)?;
        if v["engine_signature"] == src.engine_signature && v["input_sha256"] == sha {
            return Ok(json!({"cache_hit":true,"inspection":v}));
        }
    }
    let start = center.saturating_sub(800);
    let end = (start + 1600).min(src.duration);
    let mut command = Command::new(&src.engine);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let started = Instant::now();
    let result = command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-ss",
            &format!("{:.3}", start as f64 / 1000.),
            "-threads",
            "2",
            "-i",
        ])
        .arg(&src.input)
        .args([
            "-t",
            &format!("{:.3}", (end - start) as f64 / 1000.),
            "-an",
            "-sn",
            "-filter_threads",
            "1",
            "-vf",
            "scale=80:46:flags=area,format=gray,fps=10",
            "-frames:v",
            "17",
            "-pix_fmt",
            "gray",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()?;
    anyhow::ensure!(
        result.status.success(),
        "Ispezione FFmpeg fallita: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    anyhow::ensure!(
        result.stdout.len() <= 17 * SAMPLE_BYTES
            && result.stdout.len().is_multiple_of(SAMPLE_BYTES),
        "Campioni video non validi"
    );
    let frames = result
        .stdout
        .as_chunks::<SAMPLE_BYTES>()
        .0
        .iter()
        .map(|f| f.as_slice())
        .collect::<Vec<_>>();
    anyhow::ensure!(
        frames.len() >= 3,
        "Spostati almeno0,3s prima della fine del video"
    );
    anyhow::ensure!(
        stamp(&src.original)? == src.original_stamp && stamp(&src.input)? == src.input_stamp,
        "Media cambiato durante l'ispezione"
    );
    let v = json!({"version":1,"center_ms":center,"sample_start_ms":start,"sample_end_ms":end,
        "sample_step_ms":100,"sample_frames":frames.len(),"sample_dimensions":[SAMPLE_W,SAMPLE_H],
        "input_sha256":sha,"input_key":key,"engine_signature":src.engine_signature,"source_mode":src.mode,
        "metrics":window_metrics(&frames),"analysis_seconds":started.elapsed().as_secs_f64(),
        "limitations":["Indizi non esclusivi su luminanza80×46/10fps, non classificazione semantica o confini precisi.",
        "Movimento, dissolvenze e sovrimpressioni possono coesistere. Un cambio brusco non dimostra uno stacco di camera.",
        "Nessun confine, decisione o montaggio viene modificato da questa ispezione."]});
    store::write_json(&cache, &v)?;
    Ok(json!({"cache_hit":false,"inspection":v}))
}
pub async fn inspect(
    State(state): State<AppState>,
    Path((pid, aid)): Path<(Uuid, Uuid)>,
    Json(req): Json<Inspect>,
) -> Result<Json<Value>, ApiError> {
    let src = source(&state, pid, aid, req.params).await?;
    if req.center_ms >= src.duration {
        return Err(ApiError::bad_request("Punto fuori dal video"));
    }
    let _permit = state
        .worker
        .acquire()
        .await
        .map_err(|e| ApiError::internal(e.into()))?;
    Ok(Json(
        tokio::task::spawn_blocking(move || inspect_window(&src, req.center_ms))
            .await
            .map_err(|e| ApiError::internal(e.into()))?
            .map_err(ApiError::internal)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub expected_revision: u64,
    pub params: Params,
    pub decision: Decision,
}
pub async fn review(
    State(state): State<AppState>,
    Path((pid, aid)): Path<(Uuid, Uuid)>,
    Json(req): Json<Review>,
) -> Result<Json<Project>, ApiError> {
    let src = source(&state, pid, aid, req.params).await?;
    let map = valid_map(&src)
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::bad_request("Ricalcola la mappa prima della revisione"))?;
    if req.decision.asset_id != aid
        || req.decision.input_key != map.input_key
        || (!req.decision.manual
            && !map
                .boundaries
                .iter()
                .any(|b| b.at_ms == req.decision.boundary_ms))
    {
        return Err(ApiError::bad_request(
            "Confine non appartenente alla mappa corrente",
        ));
    }
    update_project(&state, pid, |p| {
        if p.updated_at != req.expected_updated_at || p.longform.revision != req.expected_revision {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "Progetto cambiato: ricarica la shot map".into(),
            ));
        }
        p.longform.shot_decisions.retain(|d| {
            !(d.asset_id == aid
                && d.input_key == req.decision.input_key
                && d.boundary_ms == req.decision.boundary_ms)
        });
        p.longform.shot_decisions.push(req.decision);
        validate_decisions(p, &p.longform.shot_decisions)?;
        p.longform.revision += 1;
        Ok(())
    })
    .await?;
    Ok(Json(
        state
            .projects
            .read()
            .await
            .get(&pid)
            .cloned()
            .ok_or_else(|| ApiError::not_found("Progetto assente"))?,
    ))
}
pub fn validate_parameters(
    p: &Project,
    parameters: &std::collections::BTreeMap<Uuid, Params>,
) -> Result<(), ApiError> {
    if parameters.len() > 5000 {
        return Err(ApiError::bad_request("Troppe configurazioni shot map"));
    }
    for (asset, params) in parameters {
        params
            .validate()
            .map_err(|e| ApiError::bad_request(e.to_string()))?;
        if !p
            .assets
            .iter()
            .any(|a| a.id == *asset && a.kind == AssetKind::Video)
        {
            return Err(ApiError::bad_request(
                "Configurazione shot map riferita a media estraneo",
            ));
        }
    }
    Ok(())
}
pub fn validate_decisions(p: &Project, decisions: &[Decision]) -> Result<(), ApiError> {
    let mut seen = std::collections::HashSet::new();
    if decisions.len() > 20_000 {
        return Err(ApiError::bad_request("Troppe revisioni inquadrature"));
    }
    for d in decisions {
        if d.input_key.is_nil()
            || d.reason.len() > 2000
            || !seen.insert((d.asset_id, d.input_key, d.boundary_ms))
            || !p.assets.iter().any(|a| {
                a.id == d.asset_id
                    && a.kind == AssetKind::Video
                    && a.media
                        .as_ref()
                        .and_then(|m| m.duration_ms)
                        .is_some_and(|duration| d.boundary_ms > 0 && d.boundary_ms < duration)
            })
        {
            return Err(ApiError::bad_request(
                "Revisione shot map non valida o riferita a media estraneo",
            ));
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn evidence(frames: &[Vec<u8>]) -> Value {
        window_metrics(&frames.iter().map(Vec::as_slice).collect::<Vec<_>>())
    }
    #[test]
    fn local_evidence_distinguishes_synthetic_steps_fades_flashes_and_overlays() {
        let step = (0..16)
            .map(|i| vec![if i < 8 { 20 } else { 220 }; SAMPLE_BYTES])
            .collect::<Vec<_>>();
        assert!(evidence(&step)["hints"]
            .as_array()
            .unwrap()
            .contains(&json!("abrupt_visual_change")));
        let fade = (0..16)
            .map(|i| vec![20 + i * 12; SAMPLE_BYTES])
            .collect::<Vec<_>>();
        assert!(evidence(&fade)["hints"]
            .as_array()
            .unwrap()
            .contains(&json!("distributed_gradual_change")));
        let flash = (0..16)
            .map(|i| vec![if i == 8 { 220 } else { 20 }; SAMPLE_BYTES])
            .collect::<Vec<_>>();
        assert!(evidence(&flash)["hints"]
            .as_array()
            .unwrap()
            .contains(&json!("transient_flash_or_return")));
        let overlay = (0..16)
            .map(|i| {
                let mut f = vec![20; SAMPLE_BYTES];
                if i >= 8 {
                    for y in 0..SAMPLE_H / 2 {
                        for x in 0..SAMPLE_W / 2 {
                            f[y * SAMPLE_W + x] = 220;
                        }
                    }
                }
                f
            })
            .collect::<Vec<_>>();
        assert!(evidence(&overlay)["hints"]
            .as_array()
            .unwrap()
            .contains(&json!("localized_change_or_overlay")));
        let pan = (0..16)
            .map(|i| {
                (0..SAMPLE_BYTES)
                    .map(|j| (((j % SAMPLE_W + 2 * i) / 4 % 2) * 180 + 20) as u8)
                    .collect()
            })
            .collect::<Vec<Vec<u8>>>();
        assert!(evidence(&pan)["hints"]
            .as_array()
            .unwrap()
            .contains(&json!("translation_like_motion")));
    }
    #[test]
    fn suppression_keeps_strongest_nearby_cut_without_empty_shots() {
        let raw = vec![
            Boundary {
                at_ms: 2000,
                score: 12.,
            },
            Boundary {
                at_ms: 2100,
                score: 25.,
            },
            Boundary {
                at_ms: 5000,
                score: 13.,
            },
            Boundary {
                at_ms: 9900,
                score: 40.,
            },
        ];
        let cuts = suppress(&raw, 500, 10000);
        assert_eq!(
            cuts.iter().map(|b| b.at_ms).collect::<Vec<_>>(),
            [2100, 5000]
        );
        assert_eq!(
            time_from_line("frame:1 pts:123 pts_time:2.033333"),
            Some(2033)
        );
        assert!(time_from_line("pts_time:NaN").is_none());
        assert!(Params {
            threshold: f64::NAN,
            ..Params::default()
        }
        .validate()
        .is_err());
    }
}
