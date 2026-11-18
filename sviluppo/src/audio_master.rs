//! One regenerable PCM master used by NLE playback and the existing compositor.
//! Sources, timing, fades and bus gains come from audio_mixer, never a second timeline.
use super::*;
use crate::{compositor, store};
use anyhow::Context;
use axum::{
    body::Body,
    http::Request as HttpRequest,
    response::{IntoResponse, Response},
};
use serde_json::{json, Value};
use std::{
    path::{Path as FsPath, PathBuf},
    process::{Command, Output},
    sync::Mutex,
};
use tower::ServiceExt;
use tower_http::services::ServeFile;

static GATE: Mutex<()> = Mutex::new(());
static QUEUE_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
#[cfg(windows)]
fn free_bytes(root: &FsPath) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            path: *const u16,
            available: *mut u64,
            total: *mut u64,
            free: *mut u64,
        ) -> i32;
    }
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    let (mut available, mut total, mut free) = (0, 0, 0);
    // Read-only OS query; null-terminated UTF16 path and initialized out values.
    if unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free) } != 0 {
        Some(available)
    } else {
        None
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Measurement {
    pub integrated_lufs: Option<f64>,
    pub true_peak_dbtp: Option<f64>,
    pub loudness_range_lu: Option<f64>,
}
#[derive(Clone)]
pub struct Prepared {
    pub key: Uuid,
    pub wav: PathBuf,
    pub report: Value,
}
fn command() -> Command {
    let mut c = Command::new("ffmpeg");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    c.args(["-nostdin", "-hide_banner", "-nostats", "-threads", "2"]);
    c
}
fn checked(c: &mut Command) -> anyhow::Result<Output> {
    let out = c.output()?;
    anyhow::ensure!(
        out.status.success(),
        "Audio FFmpeg: {}",
        String::from_utf8_lossy(&out.stderr)
            .chars()
            .rev()
            .take(3000)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
    );
    Ok(out)
}
pub fn measure(path: &FsPath) -> anyhow::Result<Measurement> {
    let out = checked(command().args(["-v", "info", "-i"]).arg(path).args([
        "-map",
        "0:a:0",
        "-af",
        "ebur128=peak=true:framelog=verbose",
        "-f",
        "null",
        "-",
    ]))?;
    let text = String::from_utf8_lossy(&out.stderr);
    let summary = text
        .rsplit_once("Summary:")
        .context("Misura EBU R128 assente")?
        .1;
    let value = |prefix: &str| {
        summary
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix(prefix)
                    .and_then(|v| v.split_whitespace().next())
                    .and_then(|v| v.parse::<f64>().ok())
            })
            .filter(|v| v.is_finite())
    };
    let mut measurement = Measurement {
        integrated_lufs: value("I:"),
        true_peak_dbtp: summary
            .split_once("True peak:")
            .and_then(|(_, s)| {
                s.lines().find_map(|l| {
                    l.trim()
                        .strip_prefix("Peak:")
                        .and_then(|v| v.split_whitespace().next())
                        .and_then(|v| v.parse::<f64>().ok())
                })
            })
            .filter(|v| v.is_finite()),
        loudness_range_lu: value("LRA:"),
    };
    if measurement.true_peak_dbtp.is_none() {
        measurement.integrated_lufs = None;
    }
    Ok(measurement)
}
fn canonical(p: &Project) -> Project {
    let mut p = p.clone();
    if let Some(s) = p.render_audio_source.take() {
        p.timeline = s.timeline;
        p.motion.timeline_origin_ms = s.origin_ms;
    }
    p
}
pub fn identity(p: &Project, root: &FsPath, kbps: u32) -> anyhow::Result<(Uuid, u64)> {
    let p = canonical(p);
    validate(&p.audio, &p)?;
    let layers = compositor::manifest(&p, root)?;
    let duration = layers
        .iter()
        .map(|l| l.clip.start_ms + l.clip.duration_ms)
        .max()
        .context("Timeline vuota")?;
    let mut inputs = vec![];
    for l in layers.iter().filter(|l| l.has_audio && l.clip.volume > 0.) {
        let c = &l.clip;
        let m = std::fs::metadata(&l.path)?;
        let tr = &c.transform;
        inputs.push(json!([
            l.path,
            m.len(),
            m.modified()?
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
                .to_string(),
            c.track_id,
            l.track,
            c.start_ms,
            c.trim_in_ms,
            c.duration_ms,
            c.volume,
            tr.audio_fade_in_ms,
            tr.audio_fade_out_ms,
            tr.render_motion_origin_ms,
            tr.render_motion_duration_ms
        ]));
    }
    let key = Uuid::new_v5(
        &p.id,
        &serde_json::to_vec(&json!([
            "audio-master-v2-route-kind",
            p.audio,
            p.motion.timeline_origin_ms,
            kbps,
            duration,
            inputs
        ]))?,
    );
    Ok((key, duration))
}
fn folder(root: &FsPath, key: Uuid) -> PathBuf {
    root.join(".cache/previews/audio").join(key.to_string())
}
pub fn cached(p: &Project, root: &FsPath, kbps: u32) -> anyhow::Result<Option<Prepared>> {
    let (key, _) = identity(p, root, kbps)?;
    let dir = folder(root, key);
    if dir.join("mix.wav").is_file()
        && dir.join("listen.m4a").is_file()
        && dir.join("report.json").is_file()
    {
        let report: Value = store::read_json(&dir.join("report.json"))?;
        if report["key"] == key.to_string() {
            return Ok(Some(Prepared {
                key,
                wav: dir.join("mix.wav"),
                report,
            }));
        }
    }
    Ok(None)
}
fn channel(c: &Channel, clock: &str) -> String {
    let e = expression(c, clock);
    format!("aeval=exprs='val(0)*pow(10,({e})/20)|val(1)*pow(10,({e})/20)'")
}
fn bus_graph(p: &Project, layers: &[compositor::Layer], seconds: f64) -> anyhow::Result<String> {
    let mut g = String::new();
    let mut labels: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let direct = layers.iter().any(|l| {
        l.has_audio
            && p.timeline
                .tracks
                .iter()
                .any(|t| t.id == l.clip.track_id && p.audio.route(t) == "direct")
    });
    let mut buses = vec!["voice", "music", "fx"];
    if direct {
        buses.push("direct");
    }
    for key in buses {
        g.push_str(&format!(
            "anullsrc=r=48000:cl=stereo,atrim=duration={seconds:.6}[z{key}];"
        ));
        labels.insert(key, vec![format!("[z{key}]")]);
    }
    for (i, l) in layers
        .iter()
        .enumerate()
        .filter(|(_, l)| l.has_audio && l.clip.volume > 0.)
    {
        let track = p
            .timeline
            .tracks
            .iter()
            .find(|t| t.id == l.clip.track_id)
            .context("Traccia assente")?;
        g.push_str(&clip_filter(p, &l.clip, i, false)?);
        g.push(';');
        labels
            .get_mut(p.audio.route(track))
            .context("Bus non valido")?
            .push(format!("[a{i}]"));
    }
    let clock = format!("(t+{:.6})", p.motion.timeline_origin_ms as f64 / 1000.);
    for (key, ls) in labels {
        let gain = if !p.audio.enabled {
            "anull".into()
        } else if !p.audio.audible(key) {
            "volume=0".into()
        } else if key == "direct" {
            "anull".into()
        } else {
            channel(p.audio.channel(key), &clock)
        };
        g.push_str(&format!("{}amix=inputs={}:normalize=0:duration=longest,atrim=duration={seconds:.6},{gain}[{key}];",ls.join(""),ls.len()));
    }
    let duck = &p.audio.ducking;
    let music = if p.audio.enabled && duck.enabled {
        let depth = expression(&duck.depth, &clock);
        let floor = format!("pow(10,({depth})/20)");
        g.push_str(&format!("[voice]asplit=2[vout][vsc];[music]asplit=2[mdry][mcomp];[mcomp][vsc]sidechaincompress=threshold={:.9}:ratio={:.6}:attack={:.6}:release={:.6}:makeup=1:detection=rms:link=maximum[ducked];[mdry]aeval=exprs='val(0)*({floor})|val(1)*({floor})'[dryfloor];[ducked]aeval=exprs='val(0)*(1-({floor}))|val(1)*(1-({floor}))'[wet];[dryfloor][wet]amix=inputs=2:normalize=0:duration=longest[mout];",10f64.powf(duck.threshold_db/20.),duck.ratio,duck.attack_ms,duck.release_ms));
        "[vout][mout][fx]"
    } else {
        "[voice][music][fx]"
    };
    let master = if !p.audio.enabled {
        "anull".into()
    } else if p.audio.master.mute {
        "volume=0".into()
    } else {
        channel(&p.audio.master, &clock)
    };
    let extra = if direct { "[direct]" } else { "" };
    let inputs = if direct { 4 } else { 3 };
    g.push_str(&format!("{music}{extra}amix=inputs={inputs}:normalize=0:duration=longest,{master},atrim=duration={seconds:.6}[mix]"));
    Ok(g)
}
pub fn prepare(project: &Project, root: &FsPath, kbps: u32) -> anyhow::Result<Prepared> {
    let _guard = GATE
        .lock()
        .map_err(|_| anyhow::anyhow!("Preparazione audio interrotta"))?;
    if let Some(v) = cached(project, root, kbps)? {
        return Ok(v);
    }
    let p = canonical(project);
    validate(&p.audio, &p)?;
    let (key, duration) = identity(&p, root, kbps)?;
    #[cfg(windows)]
    if let Some(available) = free_bytes(root) {
        let required = duration
            .saturating_mul(768 + u64::from(kbps) / 8)
            .saturating_add(25 * 1024 * 1024);
        anyhow::ensure!(available>=required,"Spazio insufficiente per il master audio: servono circa {:.1} MiB temporanei, disponibili {:.1} MiB. Nessun modello o media è stato rimosso.",required as f64/1048576.,available as f64/1048576.);
    }
    let dir = folder(root, key);
    std::fs::create_dir_all(&dir)?;
    let raw = dir.join("raw.work.wav");
    let wav = dir.join("mix.wav");
    let listen = dir.join("listen.m4a");
    let layers = compositor::manifest(&p, root)?;
    let graph = dir.join("mix.filters.txt");
    std::fs::write(&graph, bus_graph(&p, &layers, duration as f64 / 1000.)?)?;
    let mut cmd = command();
    cmd.args(["-v", "error", "-y", "-filter_complex_threads", "2"]);
    for l in &layers {
        if l.kind == AssetKind::Image {
            cmd.args(["-loop", "1"]);
        }
        cmd.args([
            "-ss",
            &format!("{:.3}", l.clip.trim_in_ms as f64 / 1000.),
            "-t",
            &format!("{:.3}", l.clip.duration_ms as f64 / 1000.),
            "-i",
        ])
        .arg(&l.path);
    }
    checked(
        cmd.arg("-/filter_complex")
            .arg(&graph)
            .args([
                "-map",
                "[mix]",
                "-vn",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-c:a",
                "pcm_f32le",
            ])
            .arg(&raw),
    )?;
    let before = measure(&raw)?;
    let f = &p.audio.finalization;
    let enabled = p.audio.enabled && f.enabled;
    let mut gain = if enabled {
        f.gain_db
            + if f.normalize {
                before
                    .integrated_lufs
                    .map(|v| f.target_lufs - v)
                    .unwrap_or(0.)
            } else {
                0.
            }
    } else {
        0.
    };
    let mut ceiling = f.ceiling_dbtp;
    let mut rounds = vec![];
    let mut delivered;
    for round in 0..3 {
        let mut chain = format!("volume={gain:.9}dB");
        if enabled && f.limiter {
            chain.push_str(&format!(",aresample=192000,alimiter=limit={:.9}:attack=5:release=80:level=0:latency=1,aresample=48000",10f64.powf(ceiling/20.)));
        }
        checked(
            command()
                .args(["-v", "error", "-y", "-i"])
                .arg(&raw)
                .args(["-af", &chain, "-c:a", "pcm_f32le", "-ar", "48000"])
                .arg(&wav),
        )?;
        checked(
            command()
                .args(["-v", "error", "-y", "-i"])
                .arg(&wav)
                .args([
                    "-c:a",
                    "aac",
                    "-b:a",
                    &format!("{kbps}k"),
                    "-ar",
                    "48000",
                    "-ac",
                    "2",
                ])
                .arg(&listen),
        )?;
        delivered = measure(&listen)?;
        rounds.push(json!({"gain_db":gain,"limiter_ceiling_dbtp":if enabled && f.limiter {Some(ceiling)} else {None},"aac":delivered}));
        let peak_over = enabled
            && f.limiter
            && delivered
                .true_peak_dbtp
                .is_some_and(|v| v > f.ceiling_dbtp + 0.05);
        let loud_delta = if enabled && f.normalize {
            delivered
                .integrated_lufs
                .map(|v| f.target_lufs + f.gain_db - v)
                .unwrap_or(0.)
        } else {
            0.
        };
        if round == 2 || (!peak_over && loud_delta.abs() <= 0.5) {
            break;
        }
        if peak_over {
            ceiling =
                (ceiling - (delivered.true_peak_dbtp.unwrap() - f.ceiling_dbtp) - 0.2).max(-23.9);
        }
        if loud_delta.abs() > 0.5 {
            gain = (gain + loud_delta).clamp(-60., 60.);
        }
    }
    let after = measure(&wav)?;
    let delivered = measure(&listen)?;
    let met_loudness = if enabled && f.normalize {
        delivered
            .integrated_lufs
            .map(|v| (v - f.target_lufs - f.gain_db).abs() <= 0.5)
    } else {
        None
    };
    let met_peak = if enabled && f.limiter {
        delivered.true_peak_dbtp.map(|v| v <= f.ceiling_dbtp + 0.05)
    } else {
        None
    };
    let report = json!({"key":key,"renderer":"shared-ffmpeg-audio-v1","duration_ms":duration,"source_mix":before,"pcm":after,"aac":delivered,"audio_kbps":kbps,"settings":p.audio,"calibration_rounds":rounds,"loudness_target_met":met_loudness,"true_peak_target_met":met_peak,"estimated_pcm_bytes":duration.saturating_mul(384),"silence_or_unmeasurable":after.integrated_lufs.is_none(),"normalization_applied":enabled && f.normalize && before.integrated_lufs.is_some(),"limiter_applied":enabled && f.limiter});
    store::write_json(&dir.join("report.json"), &report)?;
    // Only these two task-owned intermediates; cache/master/listening/report remain.
    std::fs::remove_file(raw)?;
    std::fs::remove_file(graph)?;
    Ok(Prepared { key, wav, report })
}
pub async fn status(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let p = s
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    let root = store::project_dir(&s.data_dir, id);
    let kbps = crate::social::render_options(&p).audio_kbps;
    let (key, duration) =
        identity(&p, &root, kbps).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let hit = cached(&p, &root, kbps).map_err(ApiError::internal)?;
    Ok(Json(
        json!({"key":key,"ready":hit.is_some(),"url":format!("/api/projects/{id}/audio/cache/{key}"),"estimated_pcm_bytes":duration.saturating_mul(384),"analysis":hit.map(|v|v.report)}),
    ))
}
#[derive(Deserialize)]
pub struct PrepareRequest {
    pub expected_updated_at: DateTime<Utc>,
}
pub async fn queue(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(r): Json<PrepareRequest>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let _queue_guard = QUEUE_GATE.lock().await;
    let p = s
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    if p.updated_at != r.expected_updated_at {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Progetto cambiato: salva/aggiorna prima di misurare".into(),
        ));
    }
    let root = store::project_dir(&s.data_dir, id);
    let kbps = crate::social::render_options(&p).audio_kbps;
    let (key, _) = identity(&p, &root, kbps).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let message = format!("Mix audio condiviso {key}");
    if let Some(job) = s
        .jobs
        .read()
        .await
        .values()
        .find(|j| {
            j.project_id == Some(id)
                && j.message == message
                && matches!(j.status, JobStatus::Queued | JobStatus::Running)
        })
        .cloned()
    {
        return Ok((StatusCode::ACCEPTED, Json(job)));
    }
    let mut job = Job::queued(JobKind::PreparePreview, Some(id), &message);
    job.result = Some(json!({"audio_key":key}));
    s.add_job(job.clone()).await?;
    let app = s.clone();
    let jid = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(jid, JobStatus::Running, message, vec![]).await;
        let result = tokio::task::spawn_blocking(move || prepare(&p, &root, kbps)).await;
        match result {
            Ok(Ok(v)) => {
                if let Some(j) = app.jobs.write().await.get_mut(&jid) {
                    j.result = Some(json!({"audio_key":v.key,"analysis":v.report}));
                }
                app.set_job(
                    jid,
                    JobStatus::Succeeded,
                    "Mix pronto · LUFS/true-peak misurati, ascolto disponibile",
                    vec![v.wav.to_string_lossy().into_owned()],
                )
                .await;
            }
            Ok(Err(e)) => {
                app.set_job(jid, JobStatus::Failed, e.to_string(), vec![])
                    .await
            }
            Err(e) => {
                app.set_job(jid, JobStatus::Failed, e.to_string(), vec![])
                    .await
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}
pub async fn file(
    State(s): State<AppState>,
    Path((id, key)): Path<(Uuid, Uuid)>,
    request: HttpRequest<Body>,
) -> Result<Response, ApiError> {
    if !s.projects.read().await.contains_key(&id) {
        return Err(ApiError::not_found("Progetto assente"));
    }
    let root = store::project_dir(&s.data_dir, id)
        .canonicalize()
        .map_err(|_| ApiError::not_found("Progetto assente"))?;
    let path = folder(&root, key)
        .join("listen.m4a")
        .canonicalize()
        .map_err(|_| ApiError::not_found("Mix non preparato"))?;
    if !path.starts_with(root.join(".cache/previews/audio")) {
        return Err(ApiError::bad_request("Cache audio fuori dal progetto"));
    }
    let response = ServeFile::new(path)
        .oneshot(request)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(response.into_response())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silent_graph_and_preview_window_keep_explicit_processing() {
        let mut p = Project::new("window".into(), None);
        p.audio.enabled = true;
        p.audio.ducking.enabled = true;
        let graph = bus_graph(&p, &[], 2.).unwrap();
        assert!(graph.contains("sidechaincompress"));
        assert!(graph.contains("normalize=0"));
        let w = crate::compositor::window(&p, 1000, 2000);
        assert!(w.render_audio_source.is_some());
        assert_eq!(canonical(&w).motion.timeline_origin_ms, 0);
        p.audio.ducking.enabled = false;
        assert!(!bus_graph(&p, &[], 2.)
            .unwrap()
            .contains("sidechaincompress"));
    }
    #[test]
    fn cache_identity_ignores_titles_and_visual_changes_but_tracks_audio() {
        let root = std::env::temp_dir().join(format!("nl-audio-key-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(
            root.join("assets/voice.wav"),
            b"identity fixture, not decoded",
        )
        .unwrap();
        let mut p = Project::new("A".into(), None);
        let id = Uuid::new_v4();
        p.assets.push(serde_json::from_value(json!({"id":id,"name":"voice","kind":"audio","file_name":"voice.wav","source_path":"test","imported_at":Utc::now(),"media":{"has_audio":true,"duration_ms":1000}})).unwrap());
        p.timeline.clips.push(serde_json::from_value(json!({"id":Uuid::new_v4(),"asset_id":id,"track_id":p.timeline.tracks[0].id,"start_ms":0,"trim_in_ms":0,"duration_ms":1000,"volume":1.,"opacity":1.,"transform":{}})).unwrap());
        let key = identity(&p, &root, 192).unwrap().0;
        p.title = "B".into();
        p.timeline.clips[0].label = Some("renamed".into());
        p.timeline.clips[0].transform.scale = 1.2;
        assert_eq!(key, identity(&p, &root, 192).unwrap().0);
        let original = serde_json::to_value(&p).unwrap();
        let mut edits = vec![];
        for field in ["start_ms", "trim_in_ms", "duration_ms", "volume"] {
            let mut v = original.clone();
            v["timeline"]["clips"][0][field] = if field == "volume" {
                json!(0.5)
            } else {
                json!(if field == "duration_ms" { 900 } else { 10 })
            };
            edits.push(v);
        }
        let mut v = original.clone();
        v["timeline"]["tracks"][0]["kind"] = json!("audio");
        edits.push(v);
        let mut v = original.clone();
        v["timeline"]["clips"][0]["transform"]["audio_fade_in_ms"] = json!(100);
        edits.push(v);
        let mut v = original.clone();
        v["audio"]["routing"] = json!({p.timeline.tracks[0].id.to_string():"fx"});
        edits.push(v);
        let mut v = original.clone();
        v["audio"]["voice"]["db"] = json!(-3);
        edits.push(v);
        let mut v = original.clone();
        v["audio"]["music"]["points"] = json!([{"time_ms":0,"db":-6},{"time_ms":800,"db":0}]);
        edits.push(v);
        let mut v = original.clone();
        v["audio"]["ducking"]["enabled"] = json!(true);
        edits.push(v);
        let mut v = original.clone();
        v["audio"]["ducking"]["depth"]["points"] = json!([{"time_ms":0,"db":-9}]);
        edits.push(v);
        let mut v = original.clone();
        v["audio"]["finalization"]["enabled"] = json!(true);
        edits.push(v);
        let mut v = original.clone();
        v["audio"]["finalization"]["ceiling_dbtp"] = json!(-3);
        edits.push(v);
        for v in edits {
            let changed: Project = serde_json::from_value(v).unwrap();
            assert_ne!(key, identity(&changed, &root, 192).unwrap().0);
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
