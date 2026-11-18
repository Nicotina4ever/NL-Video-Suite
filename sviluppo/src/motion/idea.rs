//! Ideas become reviewed layout trees, then the existing editable Motion IR.
//! The model chooses content/hierarchy; trusted code owns geometry and typography.
use super::*;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub key: String,
    /// Empty means the safe-area root. Parents must precede their children.
    pub parent: String,
    pub kind: String,
    pub text: String,
    pub role: String,
    pub color: String,
    pub weight: f64,
    pub animation: String,
    pub delay_ms: u64,
    /// Only an explicitly selected, existing project image UUID, or empty.
    pub asset_id: String,
    /// A missing resource is a request, never an invented project asset.
    pub resource: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub key: String,
    pub name: String,
    pub rationale: String,
    pub duration_ms: u64,
    pub placement: String,
    pub nodes: Vec<Node>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub key: String,
    pub kind: String,
    pub description: String,
    pub prompt: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub summary: String,
    pub scenes: Vec<Scene>,
    pub resources: Vec<Resource>,
    pub limitations: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub brief: String,
    #[serde(default)]
    pub asset_ids: Vec<Uuid>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compile {
    pub proposal: Proposal,
    pub start_ms: u64,
    pub font: String,
}

fn schema() -> Value {
    let string = json!({"type":"string"});
    let node = json!({"type":"object","additionalProperties":false,"properties":{
        "key":string,"parent":string,"kind":{"type":"string","enum":["column","row","stack","text","rect","ellipse","path","image"]},
        "text":string,"role":{"type":"string","enum":["title","body","caption"]},"color":{"type":"string","pattern":"^#[0-9A-Fa-f]{6}$"},
        "weight":{"type":"number","minimum":0.1,"maximum":10},"animation":{"type":"string","enum":["fade","rise","slide","pop","draw","none"]},
        "delay_ms":{"type":"integer","minimum":0},"asset_id":string,"resource":string},
        "required":["key","parent","kind","text","role","color","weight","animation","delay_ms","asset_id","resource"]});
    json!({"type":"object","additionalProperties":false,"properties":{
        "summary":string,"scenes":{"type":"array","items":{"type":"object","additionalProperties":false,"properties":{
            "key":string,"name":string,"rationale":string,"duration_ms":{"type":"integer","minimum":600,"maximum":30000},
            "placement":{"type":"string","enum":["full","left","right","top","bottom","center"]},"nodes":{"type":"array","items":node}},
            "required":["key","name","rationale","duration_ms","placement","nodes"]}},
        "resources":{"type":"array","items":{"type":"object","additionalProperties":false,"properties":{
            "key":string,"kind":{"type":"string","enum":["image","audio","video"]},"description":string,"prompt":string},"required":["key","kind","description","prompt"]}},
        "limitations":{"type":"array","items":string}},"required":["summary","scenes","resources","limitations"]})
}

fn check(p: &Proposal, project: &Project) -> anyhow::Result<()> {
    anyhow::ensure!(
        !p.scenes.is_empty() && p.scenes.len() <= 200,
        "Servono 1–200 composizioni, limite del documento Motion"
    );
    anyhow::ensure!(
        serde_json::to_vec(p)?.len() <= 200_000,
        "Proposta troppo grande per un documento Motion"
    );
    let mut keys = HashSet::new();
    let mut resources = HashSet::new();
    for r in &p.resources {
        anyhow::ensure!(
            !r.key.is_empty()
                && resources.insert(r.key.clone())
                && matches!(r.kind.as_str(), "image" | "audio" | "video"),
            "Richiesta risorsa ambigua o non supportata"
        );
    }
    for scene in &p.scenes {
        anyhow::ensure!(
            keys.insert(scene.key.clone())
                && !scene.key.is_empty()
                && !scene.name.trim().is_empty()
                && scene.name.len() <= 180
                && !scene.rationale.trim().is_empty(),
            "Nome, motivazione o identificatore della scena mancanti/duplicati"
        );
        anyhow::ensure!(
            (600..=30000).contains(&scene.duration_ms)
                && matches!(
                    scene.placement.as_str(),
                    "full" | "left" | "right" | "top" | "bottom" | "center"
                ),
            "Durata o posizione della composizione non supportata"
        );
        anyhow::ensure!(
            !scene.nodes.is_empty() && scene.nodes.len() <= 80,
            "Struttura troppo densa: dividila in composizioni"
        );
        let mut seen = std::collections::HashMap::new();
        let mut rendered = 0;
        for n in &scene.nodes {
            anyhow::ensure!(
                !n.key.is_empty() && n.key.len() <= 100 && !seen.contains_key(&n.key),
                "Identificatore livello ambiguo"
            );
            let depth = if n.parent.is_empty() {
                0
            } else {
                let (kind, depth) = seen
                    .get(&n.parent)
                    .ok_or_else(|| anyhow::anyhow!("Il contenitore deve precedere il figlio"))?;
                anyhow::ensure!(
                    matches!(*kind, "row" | "column" | "stack"),
                    "Un livello disegnato non è un contenitore"
                );
                depth + 1
            };
            anyhow::ensure!(depth <= 8, "Struttura annidata troppo fitta per il layout");
            anyhow::ensure!(
                matches!(
                    n.kind.as_str(),
                    "column" | "row" | "stack" | "text" | "rect" | "ellipse" | "path" | "image"
                ) && matches!(n.role.as_str(), "title" | "body" | "caption")
                    && matches!(
                        n.animation.as_str(),
                        "fade" | "rise" | "slide" | "pop" | "draw" | "none"
                    ),
                "Elemento o animazione non supportati: rivedi la proposta"
            );
            anyhow::ensure!(
                n.color.len() == 7
                    && n.color.starts_with('#')
                    && n.color[1..].bytes().all(|b| b.is_ascii_hexdigit())
                    && n.weight.is_finite()
                    && (0.1..=10.).contains(&n.weight),
                "Colore o proporzioni non validi"
            );
            anyhow::ensure!(
                n.delay_ms < scene.duration_ms.saturating_sub(400) && n.text.len() <= 1000,
                "Ingresso fuori durata o testo troppo lungo: modifica la scena"
            );
            if !n.asset_id.is_empty() {
                let id = Uuid::parse_str(&n.asset_id)?;
                anyhow::ensure!(
                    n.kind == "image"
                        && project
                            .assets
                            .iter()
                            .any(|a| a.id == id && a.kind == AssetKind::Image),
                    "Immagine non presente nel progetto: collegala dalla libreria"
                );
            }
            anyhow::ensure!(
                n.resource.is_empty()
                    || (n.kind == "image"
                        && resources.contains(&n.resource)
                        && p.resources
                            .iter()
                            .any(|r| r.key == n.resource && r.kind == "image")),
                "Richiesta risorsa senza collegamento esplicito"
            );
            if !matches!(n.kind.as_str(), "column" | "row" | "stack") {
                rendered += 1;
            }
            seen.insert(n.key.clone(), (n.kind.as_str(), depth));
        }
        anyhow::ensure!(
            (1..=20).contains(&rendered),
            "Servono 1–20 livelli nativi per composizione: dividi la scena"
        );
    }
    Ok(())
}

pub async fn suggest(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Request>,
) -> Result<(StatusCode, Json<Job>), ApiError> {
    let p = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    if req.brief.trim().is_empty() || req.brief.len() > 12000 {
        return Err(bad("Descrivi l’idea in 1–12000 byte"));
    }
    let mut assets = vec![];
    for aid in &req.asset_ids {
        let a = p
            .assets
            .iter()
            .find(|a| a.id == *aid)
            .ok_or_else(|| bad("Materiale selezionato non presente nel progetto"))?;
        // Names/metadata are not visual observations. No implicit album analysis.
        assets.push(json!({"id":a.id,"name":a.name,"kind":a.kind,"metadata":a.media,"content_unverified":true}));
    }
    let cfg = state.config.read().expect("settings lock").clone();
    let job = Job::queued(
        JobKind::GenerateStory,
        Some(id),
        "Idea animata: proposta da revisionare",
    );
    state.add_job(job.clone()).await?;
    let jid = job.id;
    let app = state.clone();
    tokio::spawn(async move {
        let _permit = app.worker.acquire().await.expect("worker open");
        app.set_job(
            jid,
            JobStatus::Running,
            "Composizione di scene, testi e animazioni; nessun asset generato automaticamente",
            vec![],
        )
        .await;
        let result=async {
            let response=crate::providers::chat(&app.http,&cfg,"graphics",json!([
                {"role":"system","content":"Sei un motion designer: trasforma una richiesta libera in una composizione animata ORIGINALE, anche con più scene. Restituisci solo il JSON richiesto, prosa italiana. Non sei limitato a categorie o template. Usa gerarchia row/column/stack con pesi per ordinare e sovrapporre elementi; i contenitori non si disegnano. Genitori prima dei figli, radice parent vuota è column. Geometria e font sono curati dall'app, non inventare coordinate. Elementi: text, rect, ellipse, path (linea orizzontale con tratto animabile), image. Ogni scena 1–20 elementi disegnati; puoi dividere su più scene. Scene 600–30000ms, ingressi delay_ms+400<durata; nodi senza contenuto text vuoto. Applica contrasto, spazio negativo, gerarchia title/body/caption e ingressi sobri; rect/ellipse riempiono la loro cella, stack consente pannelli dietro al testo. Non inventare punteggi, fatti, identità, relazioni o contenuto delle immagini: i nomi file NON sono osservazioni visive. Testi creativi vanno presentati come proposte, dati mancanti come limiti da risolvere. UUID image solo dai materiali selezionati. Per immagini mancanti crea resources con key e prompt, nodo image con resource=key e asset_id vuoto: impedirà l'inserimento finché collegata. Audio/video richiesti vanno in resources, non in finti livelli: si producono e inseriscono con la Generazione/timeline comuni. Elenca limiti reali (3D, personaggi parlanti o effetti non rappresentabili) invece di fingere che siano renderizzati. Non fare chiamate, non generare media. Palette facoltativa NL viola #7531FF rosa #E5469C ciano #4FE5E8 su scuro, modificabile. summary e rationale devono descrivere esattamente le scene proposte."},
                {"role":"user","content":serde_json::to_string(&json!({"request":req.brief,"selected_materials":assets,"aspect_ratio":p.aspect_ratio}))?}
            ]),schema(),5000).await?;
            let raw=response.pointer("/message/content").and_then(Value::as_str).ok_or_else(||anyhow::anyhow!("Proposta vuota"))?;
            let proposal:Proposal=serde_json::from_str(raw)?;
            check(&proposal,&p)?;
            // No selected asset may be silently replaced by another library image.
            anyhow::ensure!(proposal.scenes.iter().flat_map(|s|&s.nodes).filter(|n|!n.asset_id.is_empty()).all(|n|req.asset_ids.iter().any(|id|id.to_string()==n.asset_id)),"La proposta usa immagini non selezionate: rivedi i collegamenti");
            Ok::<_,anyhow::Error>(json!({"kind":"motion_idea","proposal":proposal,"brief":req.brief,"asset_ids":req.asset_ids,"provider":cfg.llm_provider,"model":cfg.llm_for_role("graphics"),"approval_required":true,"ai_inference":true}))
        }.await;
        match result {
            Ok(value) => {
                if let Some(j) = app.jobs.write().await.get_mut(&jid) {
                    j.result = Some(value);
                }
                app.set_job(
                    jid,
                    JobStatus::Succeeded,
                    "Idea pronta: revisiona scene, testi e risorse prima dell’inserimento",
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

#[derive(Clone, Copy)]
struct Box2 {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}
fn cells(
    nodes: &[Node],
    parent: &str,
    kind: &str,
    bounds: Box2,
    boxes: &mut std::collections::HashMap<String, Box2>,
) {
    let children: Vec<_> = nodes.iter().filter(|n| n.parent == parent).collect();
    let sum: f64 = children.iter().map(|n| n.weight).sum();
    let gap = if children.len() > 1 {
        0.012_f64
            .min(if kind == "row" { bounds.w } else { bounds.h } / (children.len() as f64 * 3.))
    } else {
        0.
    };
    let length = if kind == "row" { bounds.w } else { bounds.h };
    let available = (length - gap * children.len().saturating_sub(1) as f64).max(0.);
    let mut at = 0.;
    for n in children {
        let side = available * n.weight / sum;
        let b = match kind {
            "row" => Box2 {
                x: bounds.x + at,
                y: bounds.y,
                w: side,
                h: bounds.h,
            },
            "stack" => bounds,
            _ => Box2 {
                x: bounds.x,
                y: bounds.y + at,
                w: bounds.w,
                h: side,
            },
        };
        boxes.insert(n.key.clone(), b);
        if matches!(n.kind.as_str(), "row" | "column" | "stack") {
            let pad = 0.012_f64.min(b.w / 8.).min(b.h / 8.);
            cells(
                nodes,
                &n.key,
                &n.kind,
                Box2 {
                    x: b.x + pad,
                    y: b.y + pad,
                    w: b.w - 2. * pad,
                    h: b.h - 2. * pad,
                },
                boxes,
            );
        }
        at += side + gap;
    }
}
fn build(
    scene: &Scene,
    at: u64,
    font: &str,
    project: &Project,
    root: Option<&std::path::Path>,
) -> anyhow::Result<Graphic> {
    let (x, y, w, h) = match scene.placement.as_str() {
        "left" => (0.07, 0.14, 0.40, 0.72),
        "right" => (0.53, 0.14, 0.40, 0.72),
        "top" => (0.08, 0.08, 0.84, 0.30),
        "bottom" => (0.08, 0.64, 0.84, 0.28),
        "center" => (0.15, 0.25, 0.70, 0.50),
        _ => (0.08, 0.10, 0.84, 0.80),
    };
    let mut boxes = std::collections::HashMap::new();
    cells(&scene.nodes, "", "column", Box2 { x, y, w, h }, &mut boxes);
    let size = crate::compositor::dimensions(&project.aspect_ratio)?;
    let mut layers = vec![];
    for n in &scene.nodes {
        if matches!(n.kind.as_str(), "row" | "column" | "stack") {
            continue;
        }
        anyhow::ensure!(
            n.kind != "image" || !n.asset_id.is_empty(),
            "Risorsa «{}» da collegare o rimuovere esplicitamente prima dell’inserimento",
            n.resource
        );
        let b = boxes[&n.key];
        anyhow::ensure!(
            b.w >= 0.015 && b.h >= 0.015,
            "Composizione troppo fitta: dividi la scena o riduci gli elementi"
        );
        let cx = b.x + b.w / 2.;
        let cy = b.y + b.h / 2.;
        let d = scene.duration_ms;
        let ramp = 240.min((d - n.delay_ms) / 3);
        let mut k = vec![Keyframe {
            at_ms: 0,
            x: cx,
            y: cy,
            scale: 1.,
            opacity: if n.animation == "none" { 1. } else { 0. },
            rotation: 0.,
            stroke_progress: if n.animation == "draw" { 0. } else { 1. },
            easing: "smooth".into(),
        }];
        if n.delay_ms > 0 {
            k.push(Keyframe {
                at_ms: n.delay_ms,
                ..k[0].clone()
            });
        }
        if n.animation == "rise" {
            for key in &mut k {
                key.y += 0.02;
            }
        }
        if n.animation == "slide" {
            for key in &mut k {
                key.x -= 0.025;
            }
        }
        if n.animation == "pop" {
            for key in &mut k {
                key.scale = 0.92;
            }
        }
        let hold = Keyframe {
            at_ms: n.delay_ms + ramp,
            x: cx,
            y: cy,
            scale: 1.,
            opacity: 1.,
            rotation: 0.,
            stroke_progress: 1.,
            easing: "smooth".into(),
        };
        k.push(hold.clone());
        k.push(Keyframe {
            at_ms: d - ramp,
            ..hold.clone()
        });
        k.push(Keyframe {
            at_ms: d,
            opacity: 0.,
            ..hold
        });
        k.sort_by_key(|k| k.at_ms);
        k.dedup_by_key(|k| k.at_ms);
        if n.animation == "none" {
            k = vec![Keyframe {
                at_ms: 0,
                x: cx,
                y: cy,
                scale: 1.,
                opacity: if n.delay_ms == 0 { 1. } else { 0. },
                rotation: 0.,
                stroke_progress: 1.,
                easing: "hold".into(),
            }];
            if n.delay_ms > 0 {
                k.push(Keyframe {
                    at_ms: n.delay_ms,
                    opacity: 1.,
                    ..k[0].clone()
                });
            }
        }
        let fs: f64 = match n.role.as_str() {
            "title" => 0.06,
            "caption" => 0.028,
            _ => 0.04,
        };
        let mut layer: Layer = serde_json::from_value(
            json!({"id":Uuid::new_v4(),"name":n.key,"kind":n.kind,"text":n.text,"color":n.color,"font":font,"font_size":fs.min(b.h*0.58).max(0.01),"width":b.w,"height":b.h,"visible":true,"keyframes":k,"asset_id":if n.asset_id.is_empty(){None}else{Some(Uuid::parse_str(&n.asset_id)?)},"decoration":{"bold":n.role=="title"}}),
        )?;
        if layer.kind == "text" {
            anyhow::ensure!(
                !n.text.trim().is_empty(),
                "Testo vuoto: completa o rimuovi il livello"
            );
            if let Some(root) = root {
                let mut fits = false;
                for _ in 0..16 {
                    let m = measure_text(&layer, size, root)?;
                    if m.height <= b.h * 0.94 && m.width <= b.w * 1.01 {
                        fits = true;
                        break;
                    }
                    if layer.font_size <= 0.0101 {
                        break;
                    }
                    layer.font_size = (layer.font_size * 0.9).max(0.01);
                }
                anyhow::ensure!(
                    fits,
                    "Testo «{}» troppo denso: abbrevialo o amplia la scena; non sarà tagliato",
                    n.key
                );
            }
        }
        layers.push(layer);
    }
    let g = Graphic {
        id: Uuid::new_v4(),
        name: scene.name.clone(),
        start_ms: at,
        duration_ms: scene.duration_ms,
        anchor_clip_id: None,
        anchor_source_ms: 0,
        approved: false,
        layers,
        evidence: format!("Idea revisionabile. {}", scene.rationale)
            .chars()
            .take(900)
            .collect(),
    };
    validate(&Document {
        graphics: vec![g.clone()],
        ..Document::default()
    })
    .map_err(|e| anyhow::anyhow!(e.1))?;
    Ok(g)
}
pub async fn compile(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<Compile>,
) -> Result<Json<Value>, ApiError> {
    let projects = state.projects.read().await;
    let p = projects
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    crate::project_management::ensure_ready(&state.data_dir, id)?;
    check(&req.proposal, &p).map_err(bad)?;
    if !crate::fonts::safe_name(&req.font) || !crate::fonts::available(&req.font) {
        return Err(bad("Scegli un font installato"));
    }
    let root = store::project_dir(&state.data_dir, id);
    let result = tokio::task::spawn_blocking(move || {
        let mut at = req.start_ms;
        let mut graphics = vec![];
        for scene in &req.proposal.scenes {
            graphics.push(build(scene, at, &req.font, &p, Some(&root))?);
            at = at
                .checked_add(scene.duration_ms)
                .ok_or_else(|| anyhow::anyhow!("Durata oltre limite"))?;
        }
        Ok::<_, anyhow::Error>(json!({"graphics":graphics}))
    })
    .await
    .map_err(bad)?
    .map_err(bad)?;
    drop(projects);
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standalone_canvas_respects_explicit_export_windows() {
        let mut p = Project::new("Animazione autonoma".into(), None);
        let mut g = build(&proposal().scenes[0], 0, "Arial", &p, None).unwrap();
        g.approved = true;
        p.motion.graphics.push(g);
        assert_eq!(super::super::duration_ms(&p), 4000);
        let window = crate::compositor::window(&p, 500, 1000);
        assert_eq!(super::super::duration_ms(&window), 500);
        let outside = crate::compositor::window(&p, 5000, 5500);
        assert_eq!(super::super::duration_ms(&outside), 0);
        let saved = serde_json::to_value(&window.motion).unwrap();
        assert!(saved.get("render_duration_ms").is_none());
        assert!(saved.get("timeline_origin_ms").is_none());
    }
    fn proposal() -> Proposal {
        serde_json::from_value(json!({"summary":"Due concetti, entrambi da revisionare","scenes":[{"key":"a","name":"Un confronto","rationale":"Due idee affiancate, non una graduatoria","duration_ms":4000,"placement":"full","nodes":[
        {"key":"row","parent":"","kind":"row","text":"","role":"body","color":"#FFFFFF","weight":1,"animation":"none","delay_ms":0,"asset_id":"","resource":""},
        {"key":"uno","parent":"row","kind":"text","text":"Ascoltare","role":"title","color":"#7531FF","weight":1,"animation":"rise","delay_ms":0,"asset_id":"","resource":""},
        {"key":"due","parent":"row","kind":"text","text":"Raccontare","role":"title","color":"#E5469C","weight":1,"animation":"slide","delay_ms":400,"asset_id":"","resource":""}]}],"resources":[],"limitations":[]})).unwrap()
    }
    #[test]
    fn generic_layout_is_editable_and_not_category_bound() {
        let p = Project::new("Idea".into(), None);
        let v = proposal();
        check(&v, &p).unwrap();
        let g = build(&v.scenes[0], 1234, "Arial", &p, None).unwrap();
        assert_eq!(g.start_ms, 1234);
        assert_eq!(g.layers.len(), 2);
        assert!(!g.approved);
        assert!(g.layers[0].keyframes[1].x < g.layers[1].keyframes[2].x);
        assert_eq!(g.layers[1].keyframes[1].opacity, 0.);
    }
    #[test]
    fn unresolved_resources_and_forged_assets_are_not_rendered() {
        let mut v = proposal();
        let p = Project::new("Idea".into(), None);
        v.resources.push(Resource {
            key: "portrait".into(),
            kind: "image".into(),
            description: "Da scegliere".into(),
            prompt: "Reference".into(),
        });
        v.scenes[0].nodes[1].kind = "image".into();
        v.scenes[0].nodes[1].resource = "portrait".into();
        check(&v, &p).unwrap();
        assert!(build(&v.scenes[0], 0, "Arial", &p, None).is_err());
        v.scenes[0].nodes[1].asset_id = Uuid::new_v4().to_string();
        assert!(check(&v, &p).is_err());
    }
    #[test]
    fn cycles_and_bad_delays_are_rejected() {
        let mut v = proposal();
        let p = Project::new("Idea".into(), None);
        v.scenes[0].nodes[0].parent = "due".into();
        assert!(check(&v, &p).is_err());
        v = proposal();
        v.scenes[0].nodes[1].delay_ms = 3900;
        assert!(check(&v, &p).is_err());
    }
}
