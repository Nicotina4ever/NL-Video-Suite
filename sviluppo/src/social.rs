//! Versioned local delivery profiles. No publishing, credentials or cloud requests.
use crate::{model::*, store, time_map::FrameRate, update_project, ApiError, AppState};
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path as FsPath,
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Document {
    pub schema_version: u32,
    pub revision: u64,
    pub selected: String,
    pub profiles: Vec<Profile>,
    pub metadata: BTreeMap<String, Draft>,
    pub show_guides: bool,
    pub thumbnail: crate::thumbnail::Design,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            selected: String::new(),
            profiles: profiles(),
            metadata: BTreeMap::new(),
            show_guides: false,
            thumbnail: crate::thumbnail::Design::default(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub version: u32,
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub fps: FrameRate,
    pub codec: String,
    pub crf: u8,
    pub audio_kbps: u32,
    #[serde(default)]
    pub encoding: crate::export_encoding::Encoding,
    pub safe: Safe,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Safe {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Draft {
    pub title: String,
    pub description: String,
    pub hashtags: String,
    pub cta: String,
    pub reviewed: bool,
    pub evidence: Vec<String>,
    pub producer: String,
}
pub fn profiles() -> Vec<Profile> {
    [
        ("youtube", "YouTube video", 1920, 1080),
        ("shorts", "YouTube Shorts", 1080, 1920),
        ("reels", "Instagram Reels", 1080, 1920),
        ("tiktok", "TikTok", 1080, 1920),
        ("vertical", "Vertical generico", 1080, 1920),
        ("square", "Quadrato", 1080, 1080),
        ("custom", "Personalizzato", 1280, 720),
    ]
    .into_iter()
    .map(|(id, label, width, height)| Profile {
        id: id.into(),
        version: 1,
        label: label.into(),
        width,
        height,
        fps: FrameRate::default(),
        codec: "h264".into(),
        crf: 18,
        audio_kbps: 192,
        encoding: Default::default(),
        safe: Safe {
            left: 0.1,
            right: 0.12,
            top: 0.12,
            bottom: if height > width { 0.22 } else { 0.12 },
        },
    })
    .collect()
}
fn bad(e: impl ToString) -> ApiError {
    ApiError::bad_request(e.to_string())
}
pub fn validate(p: &Profile) -> anyhow::Result<()> {
    anyhow::ensure!(
        !p.id.is_empty()
            && p.id.len() <= 40
            && p.id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && p.label.len() <= 180
            && p.version > 0,
        "Identità profilo non valida"
    );
    anyhow::ensure!(
        (64..=4096).contains(&p.width)
            && (64..=4096).contains(&p.height)
            && p.width.is_multiple_of(2)
            && p.height.is_multiple_of(2),
        "Dimensioni pari tra 64 e 4096 pixel"
    );
    p.fps.validate()?;
    anyhow::ensure!(
        (10..=51).contains(&p.crf) && (64..=384).contains(&p.audio_kbps),
        "CRF 10–51, audio AAC 64–384kbps"
    );
    p.encoding.validate(&p.codec)?;
    for v in [p.safe.left, p.safe.right, p.safe.top, p.safe.bottom] {
        anyhow::ensure!(
            v.is_finite() && (0. ..=0.45).contains(&v),
            "Guide normalizzate tra 0 e 45%"
        );
    }
    Ok(())
}
pub fn selected(p: &Project) -> Option<&Profile> {
    p.social.profiles.iter().find(|v| v.id == p.social.selected)
}
pub fn frame_rate(p: &Project) -> FrameRate {
    selected(p).map(|v| v.fps).unwrap_or_default()
}
pub fn render_options(p: &Project) -> crate::compositor::RenderOptions {
    selected(p)
        .map(|v| crate::compositor::RenderOptions {
            fps: v.fps,
            crf: v.crf,
            audio_kbps: v.audio_kbps,
            codec: v.codec.clone(),
            encoding: v.encoding.clone(),
        })
        .unwrap_or_default()
}
pub fn advice(p: &Project) -> Vec<String> {
    let mut warnings=vec!["Guide indicative e modificabili: le UI social cambiano per app, dispositivo e account. Non sono garanzia di visibilità.".into(),"Verifica diritti di musica, immagini, marchi e dialoghi; nessuna autorizzazione alla pubblicazione automatica.".into()];
    let Some(profile) = selected(p) else {
        return warnings;
    };
    let duration = p
        .timeline
        .clips
        .iter()
        .map(|c| c.start_ms + c.duration_ms)
        .max()
        .unwrap_or(0);
    if profile.id == "shorts" && (duration > 180000 || profile.width > profile.height) {
        warnings.push("YouTube Shorts: profilo non idoneo al requisito quadrato/verticale entro 3 minuti; verifica anche le condizioni del canale.".into());
    }
    if ["reels", "tiktok"].contains(&profile.id.as_str()) {
        warnings.push("Preset tecnico 9:16, non certificazione di upload/boost: controlla le condizioni correnti della piattaforma. Limiti di durata/account non imposti senza fonte verificata.".into());
    }
    if let Some(source) = p
        .assets
        .iter()
        .find_map(|a| a.media.as_ref().and_then(|m| m.frame_rate))
    {
        let fps = profile.fps.numerator as f64 / profile.fps.denominator as f64;
        if (source - fps).abs() > 0.05 {
            warnings.push(format!("Sorgente {source:.3}fps, output {fps:.3}fps: conversione di cadenza esplicita; usa FPS sorgente se desiderato."));
        }
    }
    for c in crate::subtitle_studio::projections(p, true) {
        if c.style.x < profile.safe.left
            || c.style.x > 1. - profile.safe.right
            || c.style.y < profile.safe.top
            || c.style.y > 1. - profile.safe.bottom
        {
            warnings.push("Uno o più cue hanno il centro fuori dalle guide: controlla ingombro, leggibilità e posizione nel Programma.".into());
            break;
        }
    }
    if !p
        .social
        .metadata
        .get(&profile.id)
        .is_some_and(|d| d.reviewed)
    {
        warnings.push(
            "Metadati non revisionati: testo esportato come bozza, nessuna pubblicazione.".into(),
        );
    }
    warnings.extend(crate::motion::warnings(p));
    warnings
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Save {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub document: Document,
}
pub async fn save(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Save>,
) -> Result<Json<Project>, ApiError> {
    crate::thumbnail::validate(&req.document.thumbnail).map_err(bad)?;
    if req.document.schema_version != 1
        || req.document.profiles.is_empty()
        || req.document.profiles.len() > 30
    {
        return Err(bad("Documento destinazioni non valido"));
    }
    let mut ids = HashSet::new();
    for profile in &req.document.profiles {
        validate(profile).map_err(bad)?;
        if !ids.insert(&profile.id) {
            return Err(bad("Profili duplicati"));
        }
    }
    if !req.document.selected.is_empty() && !ids.contains(&req.document.selected) {
        return Err(bad("Destinazione selezionata assente"));
    }
    for (key, d) in &req.document.metadata {
        if !ids.contains(key)
            || d.title.len() > 500
            || d.description.len() > 20000
            || d.hashtags.len() > 3000
            || d.cta.len() > 2000
            || d.evidence.len() > 100
            || d.evidence.iter().any(|e| e.len() > 4000)
            || d.producer.len() > 100
        {
            return Err(bad("Metadati troppo grandi o non associati al profilo"));
        }
    }
    update_project(&state, id, |p| {
        if p.updated_at != req.expected_updated_at || p.social.revision != req.document.revision {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "Destinazioni cambiate: ricarica il progetto".into(),
            ));
        }
        p.social = req.document;
        p.social.revision += 1;
        if let Some(profile) = selected(p) {
            p.aspect_ratio = format!("{}:{}", profile.width, profile.height);
        }
        Ok(())
    })
    .await?;
    Ok(Json(
        state
            .projects
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| ApiError::not_found("Progetto assente"))?,
    ))
}
pub async fn view(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let p = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    Ok(Json(
        json!({"document":p.social,"advice":advice(&p),"source_rules_checked_at":"2026-10-26","sources":["https://support.google.com/youtube/answer/1722171","https://support.google.com/youtube/answer/15424877"],"instagram_rules":"not_verified_http_429","tiktok_rules":"no_universal_limits_claimed","publishing":false}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
}
pub async fn suggest(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Revision>,
) -> Result<Json<Project>, ApiError> {
    update_project(&state, id, |p| {
        if p.updated_at != req.expected_updated_at {
            return Err(ApiError(StatusCode::CONFLICT, "Progetto modificato".into()));
        }
        let key = selected(p)
            .ok_or_else(|| bad("Scegli prima una destinazione"))?
            .id
            .clone();
        let chapters = crate::longform_analysis::chapter_export(p);
        let mapped = chapters["chapters"].as_array().cloned().unwrap_or_default();
        let mut d = Draft {
            title: p.title.clone(),
            producer: "local_grounded_rules_v1_no_new_inference".into(),
            ..Default::default()
        };
        if !mapped.is_empty() {
            d.title = mapped[0]["title"].as_str().unwrap_or(&p.title).into();
            d.description = mapped
                .iter()
                .take(8)
                .filter_map(|c| c["title"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            d.evidence = mapped
                .iter()
                .take(8)
                .map(|c| {
                    format!(
                        "Capitolo approvato: {} · {}ms timeline",
                        c["title"].as_str().unwrap_or(""),
                        c["export_start_ms"].as_u64().unwrap_or(0)
                    )
                })
                .collect();
        } else {
            let cues = crate::caption_core::cues(p);
            d.description = cues
                .iter()
                .take(3)
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            d.evidence = cues
                .iter()
                .take(3)
                .map(|c| {
                    format!(
                        "{}–{}ms timeline · {}",
                        c.timeline.start_ms, c.timeline.end_ms, c.text
                    )
                })
                .collect();
        }
        // Do not synthesize hashtags, statistics, promises or private material from discarded source.
        p.social.metadata.insert(key, d);
        p.social.revision += 1;
        Ok(())
    })
    .await?;
    Ok(Json(
        state
            .projects
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| ApiError::not_found("Progetto assente"))?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub profile_ids: Vec<String>,
    pub approved: bool,
}
pub async fn batch(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Batch>,
) -> Result<(StatusCode, Json<Vec<Job>>), ApiError> {
    if !req.approved || req.profile_ids.is_empty() || req.profile_ids.len() > 10 {
        return Err(bad(
            "Approva esplicitamente 1–10 destinazioni prima dell'export",
        ));
    }
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
            "Timeline modificata: ricontrolla prima di esportare".into(),
        ));
    }
    let mut ids = HashSet::new();
    let mut snapshots = vec![];
    for key in req.profile_ids {
        if !ids.insert(key.clone()) {
            return Err(bad("Destinazione ripetuta"));
        }
        let profile = p
            .social
            .profiles
            .iter()
            .find(|v| v.id == key)
            .ok_or_else(|| bad("Profilo assente"))?;
        validate(profile).map_err(bad)?;
        let mut snapshot = p.clone();
        snapshot.title = format!("{} - {}", p.title, profile.label);
        snapshot.social.selected = key;
        snapshot.aspect_ratio = format!("{}:{}", profile.width, profile.height);
        crate::subtitle_studio::validate_delivery(&snapshot).map_err(bad)?;
        crate::compositor::manifest(&snapshot, &store::project_dir(&state.data_dir, id))
            .map_err(bad)?;
        snapshots.push(snapshot);
    }
    let mut jobs = vec![];
    for p in snapshots {
        let (_, Json(j)) = crate::queue_export(state.clone(), p).await?;
        jobs.push(j);
    }
    Ok((StatusCode::ACCEPTED, Json(jobs)))
}
fn text(d: &Draft) -> String {
    format!(
        "{}\n\n{}\n\n{}\n{}\n\nStato: {}\n",
        d.title,
        d.description,
        d.hashtags,
        d.cta,
        if d.reviewed {
            "revisionato dall'utente"
        } else {
            "BOZZA DA REVISIONARE"
        }
    )
}
pub fn write_delivery(p: &Project, output: &FsPath) -> anyhow::Result<Vec<String>> {
    let Some(profile) = selected(p) else {
        return Ok(vec![]);
    };
    let draft = p
        .social
        .metadata
        .get(&profile.id)
        .cloned()
        .unwrap_or_default();
    let file = output.with_extension("social.json");
    store::write_json(
        &file,
        &json!({"schema_version":1,"profile":profile,"draft":draft,"warnings":advice(p),"project_updated_at":p.updated_at,"publishing":false,"chapters":crate::longform_analysis::chapter_export(p)}),
    )?;
    let qa = output.with_extension("qa.json");
    store::write_json(&qa, &crate::editorial_qa::report(p))?;
    let txt = output.with_extension("social.txt");
    std::fs::write(&txt, text(&draft))?;
    Ok(vec![
        file.to_string_lossy().into(),
        txt.to_string_lossy().into(),
        qa.to_string_lossy().into(),
    ])
}
pub async fn export(
    State(state): State<AppState>,
    Path((id, format)): Path<(Uuid, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let p = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    let draft = p
        .social
        .metadata
        .get(&p.social.selected)
        .cloned()
        .unwrap_or_default();
    let value = match format.as_str() {
        "txt" => text(&draft),
        "json" => serde_json::to_string_pretty(&json!({"document":p.social,"advice":advice(&p)}))
            .map_err(bad)?,
        _ => return Err(bad("Formati metadati TXT/JSON")),
    };
    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8".into()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=social.{format}"),
            ),
        ],
        value,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_validation_never_allows_filter_injection_or_odd_dimensions() {
        let mut p = profiles()[0].clone();
        assert!(validate(&p).is_ok());
        p.width = 1919;
        assert!(validate(&p).is_err());
        p.width = 1920;
        p.codec = "h264;movie=private".into();
        assert!(validate(&p).is_err());
        p.codec = "h264".into();
        p.safe.top = f64::NAN;
        assert!(validate(&p).is_err());
    }
}
