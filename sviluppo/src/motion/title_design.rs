//! Three original type compositions, using the existing editable Motion IR.
//! Text may come from a reviewed suggestion; geometry NEVER comes from an LLM.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub text: String,
    #[serde(default)]
    pub subtitle: String,
    pub design: String,
    #[serde(default = "duration")]
    pub duration_ms: u64,
}
fn duration() -> u64 {
    4000
}

#[allow(clippy::too_many_arguments)]
fn layer(
    kind: &str,
    text: &str,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    size: f64,
    color: &str,
    font: &str,
    bold: bool,
    duration: u64,
) -> Layer {
    let ramp = 240.min(duration / 3);
    let keyframes: Vec<_> = [
        (0, 0., y + 0.006),
        (ramp, 1., y),
        (duration - ramp, 1., y),
        (duration, 0., y),
    ]
    .into_iter()
    .map(|(at_ms, opacity, y)| Keyframe {
        at_ms,
        x,
        y,
        scale: 1.,
        opacity,
        rotation: 0.,
        stroke_progress: 1.,
        easing: "smooth".into(),
    })
    .collect();
    serde_json::from_value(json!({"id":Uuid::new_v4(),"name":if kind=="text"{"Testo editabile"}else{"Modulo tipografico"},"kind":kind,"text":text,"font":font,"font_size":size,"color":color,"width":w,"height":h,"visible":true,"keyframes":keyframes,"decoration":{"bold":bold}})).expect("native title layer")
}

pub fn build(
    text: &str,
    subtitle: &str,
    design: &str,
    ratio: f64,
    duration: u64,
    root: Option<&std::path::Path>,
) -> anyhow::Result<Graphic> {
    anyhow::ensure!(
        !text.trim().is_empty() && text.len() <= 1000 && subtitle.len() <= 1000,
        "Scrivi un titolo specifico, massimo1000byte; nessun segnaposto automatico"
    );
    anyhow::ensure!(
        ["editorial", "cinematic", "social"].contains(&design) && (500..=30000).contains(&duration),
        "Composizione o durata non valida"
    );
    let vertical = ratio < 1.;
    let size = if vertical { (720, 1280) } else { (1280, 720) };
    let fs = if vertical { 0.034 } else { 0.055 };
    let small = if vertical { 0.015 } else { 0.022 };
    let (cy, font, accent) = match design {
        "cinematic" => (0.19, "Georgia", "#C4B38B"),
        "social" => (0.18, "Segoe UI", "#E5469C"),
        _ => (0.78, "Segoe UI", "#A3CDD1"),
    };
    let name = match design {
        "cinematic" => "Cinema · apertura essenziale",
        "social" => "Social · voce in evidenza",
        _ => "Editoriale · firma di voce",
    };
    let mut layers = vec![];
    let panel_height = if vertical { 0.17 } else { 0.23 };
    let mut panel = layer(
        "rect",
        "",
        0.5,
        cy,
        0.86,
        panel_height,
        0.01,
        "#141322",
        "Arial",
        false,
        duration,
    );
    for k in &mut panel.keyframes {
        k.opacity *= if design == "cinematic" { 0.78 } else { 0.93 };
    }
    layers.push(panel);
    if design != "cinematic" {
        layers.push(layer(
            "rect",
            "",
            0.083,
            cy,
            0.005,
            panel_height * 0.72,
            0.01,
            accent,
            "Arial",
            false,
            duration,
        ));
    }
    let ty = cy
        - if subtitle.trim().is_empty() {
            0.
        } else {
            if vertical {
                0.014
            } else {
                0.023
            }
        };
    let mut title = layer(
        "text",
        text.trim(),
        0.5,
        ty,
        0.76,
        if vertical { 0.09 } else { 0.13 },
        fs,
        "#F5F1EA",
        font,
        true,
        duration,
    );
    if design == "cinematic" {
        title.decoration.bold = false;
        title.decoration.spacing = 0.0005;
    }
    if let Some(root) = root {
        // Real libass ink, including wrapping and installed-font fallback. Not
        // character-count sizing. Over-dense copy fails instead of being clipped.
        for attempt in 0..6 {
            let m = measure_text(&title, size, root)?;
            if m.height <= title.height * 0.95 {
                break;
            }
            anyhow::ensure!(
                attempt < 5,
                "Titolo troppo denso: spezzalo o abbrevialo nell’editor, non verrà troncato"
            );
            title.font_size *= 0.9;
        }
    }
    let title_id = title.id;
    layers.push(title);
    if !subtitle.trim().is_empty() {
        layers.push(layer(
            "text",
            subtitle.trim(),
            0.5,
            cy + if vertical { 0.045 } else { 0.071 },
            0.74,
            0.045,
            small,
            accent,
            "Segoe UI",
            false,
            duration,
        ));
    }
    let mut rule = layer(
        "rect",
        "",
        0.5,
        cy - panel_height / 2. + 0.012,
        0.72,
        0.0013,
        0.01,
        accent,
        "Arial",
        false,
        duration,
    );
    rule.width_link = Some(WidthLink {
        layer_id: title_id,
        padding: 0.,
    });
    layers.push(rule);
    Ok(Graphic{id:Uuid::new_v4(),name:name.into(),start_ms:0,duration_ms:duration,anchor_clip_id:None,anchor_source_ms:0,approved:false,layers,evidence:format!("Composizione {design} deterministica; testo fornito/revisionato, nessuna geometria IA. Font/colori/livelli/keyframe modificabili.")})
}

pub async fn view(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(r): Json<Request>,
) -> Result<Json<Graphic>, ApiError> {
    let ratio = {
        let all = state.projects.read().await;
        let p = all
            .get(&id)
            .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
        let (w, h) = crate::compositor::dimensions(&p.aspect_ratio).map_err(ApiError::internal)?;
        f64::from(w) / f64::from(h)
    };
    let root = store::project_dir(&state.data_dir, id);
    tokio::task::spawn_blocking(move || {
        build(
            &r.text,
            &r.subtitle,
            &r.design,
            ratio,
            r.duration_ms,
            Some(&root),
        )
    })
    .await
    .map_err(|e| ApiError::internal(e.into()))?
    .map(Json)
    .map_err(|e| ApiError::bad_request(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_designs_keep_editable_type_safe_positions_and_real_width_links() {
        for design in ["editorial", "cinematic", "social"] {
            let g = build(
                "Nicola Lanci",
                "Voce e interpretazione",
                design,
                9. / 16.,
                4000,
                None,
            )
            .unwrap();
            assert!(g
                .layers
                .iter()
                .any(|l| l.kind == "text" && l.text == "Nicola Lanci"));
            assert!(g.layers.iter().any(|l| l.width_link.is_some()));
            assert!(g
                .layers
                .iter()
                .all(|l| l.keyframes.iter().all(|k| (0.07..=0.88).contains(&k.y))));
            let d = Document {
                graphics: vec![g],
                ..Document::default()
            };
            validate(&d).unwrap();
        }
    }
}
