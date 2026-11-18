//! Optional creative guidance shared by the existing production tools.
use crate::{model::Project, ApiError, AppState};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub source: String,
    pub name: String,
    pub narrative: String,
    pub visual: String,
    pub audio: String,
    pub materials: String,
}
pub fn validate(v: &Selection) -> Result<(), ApiError> {
    for (text, limit) in [
        (&v.source, 80),
        (&v.name, 160),
        (&v.narrative, 6000),
        (&v.visual, 6000),
        (&v.audio, 6000),
        (&v.materials, 6000),
    ] {
        if text.chars().count() > limit || text.contains('\0') {
            return Err(ApiError::bad_request(
                "Indicazioni di produzione troppo estese o non valide",
            ));
        }
    }
    Ok(())
}
pub fn guidance(p: &Project) -> Value {
    p.production_preset.as_ref().map_or(Value::Null, |v| json!({"status":"User-selected editable artistic guidance, NOT observed facts, confirmed biography, approval, or required shot count. Explicit user decisions and original audio take precedence.","selection":v}))
}
pub fn key(p: &Project) -> Option<String> {
    p.production_preset
        .as_ref()
        .map(|v| Uuid::new_v5(&p.id, serde_json::to_string(v).unwrap().as_bytes()).to_string())
}
pub async fn catalog() -> Json<Value> {
    let rows = [
        ("free","Progetto libero","Definisci una domanda o un'intenzione; sviluppa un arco adattabile ai materiali reali.","Scegli una grammatica visiva coerente e lascia spazio alle eccezioni motivate.","Conserva le sorgenti; progetta pause, voce e musica in funzione del racconto."),
        ("celebration","Ritratto / ricorrenza","Apri con un tratto confermato della persona, alterna ricordi e contrasti, chiudi con una dedica approvata. Nessuna parentela o biografia inferita.","Alterna immagini d'insieme e dettagli senza ritagliare soggetti importanti; titoli sobri.","Alterna slanci e pause emotive. Copione registrabile, musica sotto la voce solo per scelta esplicita."),
        ("wedding","Matrimonio / evento","Segui attese, incontri e momenti documentati; non attribuire relazioni senza conferma.","Continuità di luce e colore; respiri sui gesti autentici, non sequenze uniformi.","Conserva dialoghi e suoni significativi; accompagna senza coprirli."),
        ("documentary","Documentario","Apri una domanda, sviluppa evidenze e punti di vista, distingui fatti da ricostruzioni; conclusione motivata.","Geografia leggibile, dettagli come evidenza; ricostruzioni riconoscibili.","Priorità all'intelligibilità; silenzi e suoni ambientali come parte del racconto."),
        ("trailer","Trailer","Promessa, tensione, contrasti e rilancio finale senza inventare eventi o svelare necessariamente la risoluzione.","Varia scala e durata delle inquadrature; raccorda direzione, soggetti e atmosfera.","Usa il master approvato come clock. Accenti e pause guidano gli shot senza imporre tagli su ogni beat."),
        ("shortfilm","Cortometraggio","Obiettivo, ostacolo, conseguenze e trasformazione proposti come finzione revisionabile; rispetta i fatti di continuità stabiliti.","Asse, sguardi, stato dei personaggi e ambienti coerenti tra shot.","Dialogo e ambiente sostengono il punto di vista; sound design intenzionale."),
        ("advertising","Spot","Problema o desiderio, beneficio dimostrabile e invito all'azione approvato. Niente promesse o testimonianze inventate.","Gerarchia leggibile, prodotto riconoscibile, spazio per il messaggio.","Ritmo mirato al messaggio, voce comprensibile; nessun trattamento obbligatorio."),
        ("musicvideo","Videoclip","Costruisci motivi visivi e variazioni emotive con la musica; nessuna trama obbligatoria.","Ritorni stilistici e contrasti intenzionali, movimenti variati.","Preserva il master musicale; beat come suggerimenti, rispetta fraseggi e pause."),
        ("corporate","Aziendale","Contesto, attività documentate e valore verificabile; conclusione adatta al pubblico.","Persone e processi reali, tipografia leggibile e identità facoltativa.","Voce chiara, ambiente pertinente; musica discreta se scelta."),
        ("social","Social","Apertura specifica, sviluppo comprensibile e conclusione utile; evita agganci generici e fatti non verificati.","Safe area e leggibilità su piccolo schermo; enfasi selettiva, non saturazione grafica.","Intelligibilità e tempi di lettura prima della densità dei tagli."),
    ];
    Json(json!(rows.into_iter().map(|(source,name,narrative,visual,audio)| Selection{source:source.into(),name:name.into(),narrative:narrative.into(),visual:visual.into(),audio:audio.into(),materials:"Usa foto, video, audio, reference e materiali manuali o generati già disponibili. Distingui originali, osservazioni incerte, indicazioni confermate e risorse soltanto proposte. Segnala ciò che manca, non generarlo automaticamente.".into()}).collect::<Vec<_>>()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
    pub selection: Option<Selection>,
}
pub async fn save(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(r): Json<Request>,
) -> Result<Json<Project>, ApiError> {
    if let Some(v) = &r.selection {
        validate(v)?;
    }
    crate::update_project(&state, id, |p| {
        if p.updated_at != r.expected_updated_at {
            return Err(ApiError::bad_request(
                "Progetto cambiato: ricarica prima di salvare l'orientamento",
            ));
        }
        p.production_preset = r.selection;
        Ok(())
    })
    .await?;
    Ok(Json(state.projects.read().await.get(&id).unwrap().clone()))
}
