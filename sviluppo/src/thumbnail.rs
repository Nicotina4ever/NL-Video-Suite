//! A native, full-resolution still from a frozen timeline snapshot, never a browser screenshot.
use crate::{model::*, store, ApiError, AppState};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Design {
    pub at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_asset_id: Option<Uuid>,
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub font: String,
    pub color: String,
    pub font_size: f64,
    pub x: f64,
    pub y: f64,
    pub title_width: f64,
    pub backing: bool,
    pub include_motion: bool,
    pub include_captions: bool,
}
impl Default for Design {
    fn default() -> Self {
        Self {
            at_ms: 0,
            source_asset_id: None,
            width: 1280,
            height: 720,
            title: String::new(),
            font: "Arial".into(),
            color: "#FFFFFF".into(),
            font_size: 0.07,
            x: 0.5,
            y: 0.8,
            title_width: 0.85,
            backing: true,
            include_motion: false,
            include_captions: false,
        }
    }
}
pub fn validate(d: &Design) -> anyhow::Result<()> {
    anyhow::ensure!(
        (64..=4096).contains(&d.width)
            && (64..=4096).contains(&d.height)
            && d.width.is_multiple_of(2)
            && d.height.is_multiple_of(2),
        "Miniatura: dimensioni pari 64–4096px"
    );
    anyhow::ensure!(
        d.at_ms < 86_400_000
            && d.title.len() <= 250
            && crate::fonts::safe_name(&d.font)
            && d.color.len() == 7
            && d.color.starts_with('#')
            && d.color[1..].bytes().all(|b| b.is_ascii_hexdigit()),
        "Miniatura: tempo, testo, font o colore non valido"
    );
    anyhow::ensure!(
        d.font_size.is_finite()
            && (0.02..=0.18).contains(&d.font_size)
            && d.x.is_finite()
            && d.y.is_finite()
            && (0.05..=0.95).contains(&d.x)
            && (0.05..=0.95).contains(&d.y)
            && d.title_width.is_finite()
            && (0.1..=0.95).contains(&d.title_width),
        "Miniatura: geometria titolo fuori dai limiti"
    );
    Ok(())
}
fn title_layer(d: &Design, kind: &str, height: f64) -> crate::motion::Layer {
    // Deserialize through the canonical layer contract, including backward-compatible defaults.
    serde_json::from_value(json!({"id":Uuid::new_v4(),"name":"Titolo miniatura", "kind":kind,"text":d.title,"color":if kind=="text"{&d.color}else{"#151020"},"font":d.font,"font_size":d.font_size,"width":d.title_width,"height":height,"visible":true,"keyframes":[{"at_ms":0,"x":d.x,"y":d.y,"scale":1.,"opacity":1.,"rotation":0.,"stroke_progress":1.,"easing":"hold"}]})).expect("canonical thumbnail layer")
}
pub fn snapshot(p: &Project, d: &Design) -> anyhow::Result<(Project, u64)> {
    validate(d)?;
    let mut source_project;
    let p = if let Some(id) = d.source_asset_id {
        let asset = p
            .assets
            .iter()
            .find(|a| a.id == id && matches!(a.kind, AssetKind::Video | AssetKind::Image))
            .ok_or_else(|| {
                anyhow::anyhow!("Scegli una foto o un video del progetto per la copertina")
            })?;
        let duration = asset
            .media
            .as_ref()
            .and_then(|m| m.duration_ms)
            .unwrap_or(5000);
        anyhow::ensure!(d.at_ms < duration, "Frame fuori dalla sorgente selezionata");
        source_project = p.clone();
        let tid = Uuid::new_v4();
        source_project.timeline.tracks = vec![Track {
            id: tid,
            name: "Sorgente copertina".into(),
            kind: TrackKind::Video,
            visible: true,
            locked: false,
        }];
        source_project.timeline.clips = vec![TimelineClip {
            id: Uuid::new_v4(),
            asset_id: id,
            track_id: tid,
            start_ms: 0,
            trim_in_ms: 0,
            duration_ms: duration,
            label: None,
            volume: 1.,
            opacity: 1.,
            transform: VisualTransform::default(),
        }];
        &source_project
    } else {
        p
    };
    let fps = crate::social::frame_rate(p);
    let frame = fps.frame_at_ms(d.at_ms)?;
    let at = fps.ms_at_frame(frame)?;
    let end = p
        .timeline
        .clips
        .iter()
        .map(|c| c.start_ms + c.duration_ms)
        .max()
        .unwrap_or(0);
    anyhow::ensure!(at < end, "Frame miniatura fuori dalla timeline");
    let to = (at + 1000 * u64::from(fps.denominator) / u64::from(fps.numerator) + 2).min(end);
    let mut snap = crate::compositor::window(p, at, to);
    if !d.include_motion {
        snap.motion.graphics.clear();
    }
    if !d.include_captions {
        snap.subtitles.enabled = false;
        snap.subtitles.units.clear();
    }
    if !d.title.trim().is_empty() {
        let mut layers = vec![];
        let height = (d.font_size * 2.5).min(0.6);
        if d.backing {
            layers.push(title_layer(d, "rect", height));
        }
        layers.push(title_layer(d, "text", height));
        snap.motion.graphics.push(crate::motion::Graphic {
            id: Uuid::new_v4(),
            name: "Titolo miniatura editabile".into(),
            start_ms: at,
            duration_ms: 1000,
            anchor_clip_id: None,
            anchor_source_ms: 0,
            approved: true,
            layers,
            evidence: "Titolo manuale miniatura, nessuna inferenza".into(),
        });
    }
    Ok((snap, at))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Render {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
}
pub async fn render(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Render>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let p = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    if p.updated_at != req.expected_updated_at {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Miniatura:progetto modificato, applica e riprova".into(),
        ));
    }
    let d = p.social.thumbnail.clone();
    let (snap, at) = snapshot(&p, &d).map_err(|e| ApiError::bad_request(e.to_string()))?;
    crate::subtitle_studio::validate_delivery(&snap)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let mut identity = p.clone();
    identity.updated_at = identity.created_at;
    let key = Uuid::new_v5(
        &id,
        format!(
            "thumbnail-rgb-v3-independent-overlays:{}",
            serde_json::to_string(&identity).map_err(|e| ApiError::internal(e.into()))?
        )
        .as_bytes(),
    );
    let job = Job::queued(
        JobKind::PreparePreview,
        Some(id),
        "Miniatura nativa: frame e livelli alla risoluzione richiesta",
    );
    state.add_job(job.clone()).await?;
    let app = state.clone();
    let jid = job.id;
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(
            jid,
            JobStatus::Running,
            "Render PNG RGB · nessuna ASR o inferenza",
            vec![],
        )
        .await;
        let root = store::project_dir(&app.data_dir, id);
        let result=tokio::task::spawn_blocking(move||->anyhow::Result<String>{
            let directory=root.join("exports");std::fs::create_dir_all(&directory)?;
            let output=directory.join(format!("thumbnail-{key}.png"));
            if !output.is_file(){
                let temporary=directory.join(format!("thumbnail-{key}-{jid}.png"));
                let layers=crate::compositor::manifest(&snap,&root)?;
                let size=(d.width,d.height);
                crate::compositor::render_project(&snap,&root,&layers,&temporary,size,crate::social::render_options(&snap))?;
                std::fs::rename(temporary,&output)?;
            }
            if !output.with_extension("json").is_file(){store::write_json(&output.with_extension("json"),&json!({"schema_version":1,"design":d,"timeline_frame_ms":at,"project_revision":p.updated_at,"renderer":"native RGB compositor","real_ai_inference":false}))?;}
            Ok(output.to_string_lossy().into_owned())
        }).await;
        match result {
            Ok(Ok(path)) => {
                if let Some(j) = app.jobs.write().await.get_mut(&jid) {
                    j.result = Some(
                        json!({"url":format!("/api/v1/projects/{id}/social/thumbnail/{key}"),"at_ms":at,"key":key,"ai_inference":false}),
                    );
                }
                app.set_job(
                    jid,
                    JobStatus::Succeeded,
                    "Miniatura PNG pronta: controlla anche a dimensione telefono",
                    vec![path],
                )
                .await;
            }
            other => {
                let e = match other {
                    Ok(Err(e)) => e.to_string(),
                    Err(e) => e.to_string(),
                    _ => unreachable!(),
                };
                app.set_job(jid, JobStatus::Failed, e, vec![]).await;
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(job)))
}
pub async fn file(
    State(state): State<AppState>,
    Path((id, key)): Path<(Uuid, Uuid)>,
    request: Request<Body>,
) -> Result<Response, ApiError> {
    let path = store::project_dir(&state.data_dir, id)
        .join("exports")
        .join(format!("thumbnail-{key}.png"));
    if !path.is_file() {
        return Err(ApiError::not_found("Miniatura non disponibile"));
    }
    Ok(ServeFile::new(path)
        .oneshot(request)
        .await
        .unwrap()
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thumbnail_validation_is_bounded_and_font_and_color_are_not_filter_code() {
        let mut d = Design::default();
        assert!(!d.include_motion && !d.include_captions);
        assert!(validate(&d).is_ok());
        d.color = "#fff;movie=secret".into();
        assert!(validate(&d).is_err());
        d = Design::default();
        d.font = "Arial\\fnOther".into();
        assert!(validate(&d).is_err());
        d = Design::default();
        d.width = 1281;
        assert!(validate(&d).is_err());
        d = Design::default();
        d.x = f64::NAN;
        assert!(validate(&d).is_err());
    }
    #[test]
    fn still_snapshot_rebases_both_motion_clocks_without_mutating_project() {
        let mut p = Project::new("Test".into(), None);
        p.timeline.clips.push(TimelineClip {
            id: Uuid::new_v4(),
            asset_id: Uuid::new_v4(),
            track_id: p.timeline.tracks[0].id,
            start_ms: 500,
            trim_in_ms: 200,
            duration_ms: 4000,
            label: None,
            volume: 1.,
            opacity: 1.,
            transform: VisualTransform {
                motion: "slide_left".into(),
                ..Default::default()
            },
        });
        let before = serde_json::to_string(&p).unwrap();
        let design = Design {
            at_ms: 1000,
            title: "Titolo".into(),
            ..Default::default()
        };
        let (s, at) = snapshot(&p, &design).unwrap();
        assert_eq!(at, 1000);
        assert_eq!(s.timeline.clips[0].trim_in_ms, 700);
        assert_eq!(
            s.timeline.clips[0].transform.render_motion_origin_ms,
            Some(-500)
        );
        assert_eq!(
            s.timeline.clips[0].transform.render_motion_duration_ms,
            Some(4000)
        );
        assert_eq!(s.motion.timeline_origin_ms, 1000);
        assert_eq!(s.motion.graphics[0].layers.last().unwrap().text, "Titolo");
        assert!(!serde_json::to_string(&s)
            .unwrap()
            .contains("render_motion_origin"));
        assert_eq!(serde_json::to_string(&p).unwrap(), before);
    }
}
