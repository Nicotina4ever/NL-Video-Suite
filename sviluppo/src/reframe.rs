//! Editable source-time trajectory. Shared Hermite polynomial in canvas/FFmpeg.
use crate::{model::*, store, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    pub source_ms: u64,
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub cut: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraPath {
    pub version: String,
    pub enabled: bool,
    pub stability: f64,
    pub response: f64,
    pub points: Vec<Point>,
    pub samples: Vec<Value>,
    pub review_required: bool,
    #[serde(default)]
    pub cache_hit: bool,
    #[serde(default)]
    pub behavior: String,
    #[serde(default)]
    pub preferred_fit: String,
    #[serde(default)]
    pub advice: String,
    #[serde(default)]
    pub diagnostics: Value,
}
impl CameraPath {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            ["camera-cpu-v1", "camera-cpu-v2", "camera-cpu-v3"].contains(&self.version.as_str())
                && (0. ..=1.).contains(&self.stability)
                && (0. ..=1.).contains(&self.response)
                && self.points.len() >= 2
                && self.points.len() <= 18721
                && self.samples.len() <= 721,
            "Traiettoria non valida o oltre180s: dividi il clip per revisionare le inquadrature"
        );
        for (i, p) in self.points.iter().enumerate() {
            anyhow::ensure!(
                p.source_ms <= 86_400_000
                    && (0. ..=1.).contains(&p.x)
                    && (0. ..=1.).contains(&p.y)
                    && p.vx.is_finite()
                    && p.vy.is_finite()
                    && p.vx.abs() <= 4.
                    && p.vy.abs() <= 4.
                    && (i == 0 || p.source_ms > self.points[i - 1].source_ms),
                "Coordinate/tempi camera non validi"
            );
        }
        anyhow::ensure!(
            serde_json::to_vec(&self.samples)?.len() <= 1_000_000,
            "Diagnostici camera troppo grandi"
        );
        Ok(())
    }
    pub fn expression(&self, axis: &str, clock: &str) -> String {
        fn build(p: &[Point], axis: &str, clock: &str) -> String {
            let (value, velocity) = if axis == "x" {
                (p[0].x, p[0].vx)
            } else {
                (p[0].y, p[0].vy)
            };
            if velocity == 0.
                && p.iter().all(|v| {
                    if axis == "x" {
                        v.x == value && v.vx == 0.
                    } else {
                        v.y == value && v.vy == 0.
                    }
                })
            {
                return format!("{value:.9}");
            }
            if p.len() > 2 {
                let mid = p.len() / 2;
                return format!(
                    "if(lt({clock},{:.6}),{},{})",
                    p[mid].source_ms as f64 / 1000.,
                    build(&p[..=mid], axis, clock),
                    build(&p[mid..], axis, clock)
                );
            }
            let (a, b) = (&p[0], &p[1]);
            let (x, y, va, vb) = if axis == "x" {
                (a.x, b.x, a.vx, b.vx)
            } else {
                (a.y, b.y, a.vy, b.vy)
            };
            if b.cut {
                return format!(
                    "if(lt({clock},{:.6}),{x:.9},{y:.9})",
                    b.source_ms as f64 / 1000.
                );
            }
            let dt = (b.source_ms - a.source_ms) as f64 / 1000.;
            let q = format!(
                "clip(({clock}-{:.6})/{dt:.6},0,1)",
                a.source_ms as f64 / 1000.
            );
            let c = dt * va;
            let d = 3. * (y - x) - dt * (2. * va + vb);
            let e = 2. * (x - y) + dt * (va + vb);
            format!("({x:.9}+({q})*({c:.9}+({q})*({d:.9}+({q})*{e:.9})))")
        }
        build(&self.points, axis, clock)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub stability: f64,
    pub response: f64,
    #[serde(default = "legacy_behavior")]
    pub behavior: String,
}
fn legacy_behavior() -> String {
    "legacy".into()
}
pub async fn analyze(
    State(state): State<AppState>,
    Path((id, cid)): Path<(Uuid, Uuid)>,
    Json(r): Json<Request>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let p = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    if p.updated_at != r.expected_updated_at {
        return Err(ApiError::bad_request("Progetto cambiato: ricarica"));
    }
    let c = p
        .timeline
        .clips
        .iter()
        .find(|c| c.id == cid)
        .cloned()
        .ok_or_else(|| ApiError::bad_request("Clip assente"))?;
    let a = p
        .assets
        .iter()
        .find(|a| a.id == c.asset_id && a.kind == AssetKind::Video)
        .ok_or_else(|| ApiError::bad_request("Seleziona un video"))?;
    if c.duration_ms < 250
        || c.duration_ms > 180000
        || ![r.stability, r.response]
            .iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(v))
        || !["legacy", "composed", "fixed"].contains(&r.behavior.as_str())
    {
        return Err(ApiError::bad_request(
            "Analisi CPU: clip250ms–180s; controlli0–1. Dividi i video lunghi per inquadratura.",
        ));
    }
    let root = store::project_dir(&state.data_dir, id);
    let delivery = crate::compositor::dimensions(&p.aspect_ratio)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let info = a
        .media
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("Dimensioni video assenti"))?;
    let source_ratio = f64::from(info.width.unwrap_or(0)) / f64::from(info.height.unwrap_or(0));
    if !source_ratio.is_finite() || source_ratio <= 0. {
        return Err(ApiError::bad_request("Dimensioni video non valide"));
    }
    let target_ratio = f64::from(delivery.0) / f64::from(delivery.1);
    let (crop_x, crop_y) = (
        (target_ratio / source_ratio).min(1.),
        (source_ratio / target_ratio).min(1.),
    );
    let bounds = [
        [crop_x / 2., 1. - crop_x / 2.],
        [crop_y / 2., 1. - crop_y / 2.],
    ];
    let source = root.join("assets").join(&a.file_name);
    let meta = std::fs::metadata(&source).map_err(|e| ApiError::internal(e.into()))?;
    let key = Uuid::new_v5(
        &a.id,
        format!(
            "camera-detect-pts-v2|{}|{:?}|{}|{}",
            meta.len(),
            meta.modified(),
            c.trim_in_ms,
            c.duration_ms
        )
        .as_bytes(),
    );
    let job = Job::queued(
        JobKind::PreparePreview,
        Some(id),
        "Traiettoria camera CPU · proposta non applicata",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let jid = job.id;
    let python = state.config().python_executable;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(
            jid,
            JobStatus::Running,
            "Analisi volti/stacchi CPU4Hz · nessuna identità riconosciuta",
            vec![],
        )
        .await;
        let result=tokio::task::spawn_blocking(move || -> anyhow::Result<CameraPath> {
            let work=root.join("jobs").join(jid.to_string());std::fs::create_dir_all(&work)?;
            let cache=root.join("cache").join("camera");std::fs::create_dir_all(&cache)?;
            std::fs::write(work.join("camera.py"),include_str!("../bridge/camera_path.py"))?;
            store::write_json(&work.join("request.json"),&json!({"source":source,"start_ms":c.trim_in_ms,"end_ms":c.trim_in_ms+c.duration_ms,"cache":cache.join(format!("{key}.json")),"stability":r.stability,"response":r.response,"bounds":bounds,"behavior":r.behavior}))?;
            let mut cmd=std::process::Command::new(python);
            #[cfg(windows)] {use std::os::windows::process::CommandExt;cmd.creation_flags(0x08000000);}
            let output=cmd.arg(work.join("camera.py")).arg(work.join("request.json")).arg(work.join("result.json")).env("PYTHONDONTWRITEBYTECODE","1").output()?;
            anyhow::ensure!(output.status.success(),"Camera CPU: {}",String::from_utf8_lossy(&output.stderr));
            let track:CameraPath=store::read_json(&work.join("result.json"))?;track.validate()?;Ok(track)
        }).await;
        match result {
            Ok(Ok(track)) => {
                if let Some(j) = app.jobs.write().await.get_mut(&jid) {
                    j.result = Some(
                        json!({"clip_id":cid,"asset_id":c.asset_id,"trim_in_ms":c.trim_in_ms,"duration_ms":c.duration_ms,"camera_path":track}),
                    );
                }
                app.set_job(
                    jid,
                    JobStatus::Succeeded,
                    "Traiettoria pronta: adotta, verifica e modifica nell’inspector",
                    vec![],
                )
                .await;
            }
            other => {
                app.set_job(jid, JobStatus::Failed, format!("{other:?}"), vec![])
                    .await;
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_validation_rejects_time_reversal_and_nonfinite_coordinates() {
        let mut t = CameraPath {
            version: "camera-cpu-v1".into(),
            enabled: true,
            stability: 0.5,
            response: 0.5,
            points: vec![
                Point {
                    source_ms: 0,
                    x: 0.5,
                    y: 0.5,
                    vx: 0.,
                    vy: 0.,
                    cut: false,
                },
                Point {
                    source_ms: 1000,
                    x: 0.7,
                    y: 0.5,
                    vx: 0.,
                    vy: 0.,
                    cut: false,
                },
            ],
            samples: vec![],
            review_required: true,
            cache_hit: false,
            behavior: String::new(),
            preferred_fit: String::new(),
            advice: String::new(),
            diagnostics: Value::Null,
        };
        assert!(t.validate().is_ok());
        assert!(t.expression("x", "t").contains("0.500000000"));
        t.points[1].source_ms = 0;
        assert!(t.validate().is_err());
        t.points[1].source_ms = 1;
        t.points[1].x = f64::NAN;
        assert!(t.validate().is_err());
    }
}
