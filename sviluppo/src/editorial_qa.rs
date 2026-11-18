//! Fast, explainable review assistance. No inferred truth, virality scores or new model calls.
use crate::{
    model::*,
    time_map::{self, TimeRange},
    ApiError, AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

#[derive(Serialize)]
pub struct Issue {
    pub code: &'static str,
    pub severity: &'static str,
    pub area: &'static str,
    pub target_id: Option<Uuid>,
    pub timeline: Option<TimeRange>,
    pub message: String,
    pub evidence: String,
}
#[derive(Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub project_updated_at: chrono::DateTime<chrono::Utc>,
    pub producer: &'static str,
    pub real_ai_inference: bool,
    pub issues: Vec<Issue>,
    pub languages: BTreeMap<String, usize>,
    pub checks: Vec<&'static str>,
    pub limitations: Vec<&'static str>,
}
fn issue(
    code: &'static str,
    area: &'static str,
    id: Option<Uuid>,
    timeline: Option<TimeRange>,
    message: impl Into<String>,
    evidence: impl Into<String>,
) -> Issue {
    Issue {
        code,
        severity: "review",
        area,
        target_id: id,
        timeline,
        message: message.into(),
        evidence: evidence.into(),
    }
}
fn timeline(p: &Project, asset: Uuid, range: TimeRange) -> Option<TimeRange> {
    time_map::source_to_timeline(p, asset, range)
        .ok()?
        .first()
        .map(|m| m.timeline)
}
fn luminance(hex: &str) -> f64 {
    let f = |at| {
        let c = u8::from_str_radix(&hex[at..at + 2], 16).unwrap_or(0) as f64 / 255.;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    f(1) * 0.2126 + f(3) * 0.7152 + f(5) * 0.0722
}
pub fn report(p: &Project) -> Report {
    let mut issues = vec![];
    let mut languages = BTreeMap::new();
    for (tid, metadata) in &p.transcript_metadata {
        if let Some(count) = metadata["alignment_diagnostics"]["unplaced_word_count"]
            .as_u64()
            .filter(|n| *n > 0)
        {
            issues.push(issue("asr_unplaced_words", "asr", Some(*tid), None,
                format!("{count} parole riconosciute senza durata utilizzabile: ascolta e correggi in Longform prima di approvare i sottotitoli."),
                "Il testo è conservato nei diagnostici per blocco; i timing non sono inventati. La presenza di parole allineate non certifica la completezza della trascrizione."));
        }
    }
    let mut seen = HashSet::new();
    for u in &p.subtitles.units {
        *languages.entry(u.language.clone()).or_insert(0) += 1;
        if u.translation_of.is_some() && !u.words.is_empty() {
            issues.push(issue("translation_word_timing","subtitles",Some(u.id),timeline(p,u.asset_id,u.source),"Traduzione con tempi parola:non trasferire l’allineamento di un’altra lingua senza verificarlo.",&u.text));
        }
        if u.text.contains('\u{fffd}') {
            issues.push(issue(
                "unicode_replacement",
                "subtitles",
                Some(u.id),
                timeline(p, u.asset_id, u.source),
                "Carattere sostitutivo Unicode:controlla codifica e testo originale.",
                &u.text,
            ));
        }
        if u.text
            .chars()
            .any(|c| ('\u{1f300}'..='\u{1faff}').contains(&c))
        {
            issues.push(issue("emoji_font_review","subtitles",Some(u.id),timeline(p,u.asset_id,u.source),"Emoji nel testo:controlla il glifo nel PNG nativo e nell’export;il font di sistema potrebbe non renderlo.",&u.text));
        }
    }
    let projections = crate::subtitle_studio::projections(p, false);
    for c in &projections {
        let chars = c.cue.text.chars().filter(|c| !c.is_control()).count();
        let duration = c
            .cue
            .timeline
            .end_ms
            .saturating_sub(c.cue.timeline.start_ms);
        let cps = chars as f64 * 1000. / duration.max(1) as f64;
        if cps > 20. {
            issues.push(issue("reading_speed","subtitles",Some(c.unit_id),Some(c.cue.timeline),format!("Lettura rapida:{cps:.1}caratteri/s. Valuta spezzatura, durata o testo senza cambiare il parlato."),&c.cue.text));
        }
        if c.cue.requires_review && c.style.mode != "plain" {
            issues.push(issue("word_timing_review","subtitles",Some(c.unit_id),Some(c.cue.timeline),"Effetto parola richiesto ma tempi non confermati:il renderer usa il testo statico,non inventa allineamenti.",&c.cue.text));
        }
        if !crate::fonts::available(&c.style.font) && seen.insert(c.style.font.clone()) {
            issues.push(issue(
                "font_fallback",
                "subtitles",
                Some(c.unit_id),
                Some(c.cue.timeline),
                format!(
                    "Font {} assente:fallbackArial, controlla ingombro e accenti.",
                    c.style.font
                ),
                &c.cue.text,
            ));
        }
        let rows = c
            .cue
            .text
            .lines()
            .map(|line| {
                line.chars()
                    .count()
                    .div_ceil(c.style.max_chars.max(1) as usize)
                    .max(1)
            })
            .sum::<usize>();
        if rows > 3 {
            issues.push(issue("caption_density","subtitles",Some(c.unit_id),Some(c.cue.timeline),format!("Circa{rows}righe:controlla leggibilità e taglio sui bordi;stima testuale,non misura dei glifi."),&c.cue.text));
        }
    }
    // Sweep active cue intervals, bounded report size. A repeat on another clip remains intentional.
    let mut sorted = projections
        .iter()
        .filter(|c| c.approved)
        .collect::<Vec<_>>();
    sorted.sort_by_key(|c| c.cue.timeline.start_ms);
    for pair in sorted.windows(2) {
        if pair[0].cue.timeline.end_ms > pair[1].cue.timeline.start_ms {
            issues.push(issue("caption_overlap","subtitles",Some(pair[1].unit_id),Some(pair[1].cue.timeline),"Cue approvati sovrapposti:verifica se il doppio testo è intenzionale,nessuno spostamento automatico.",format!("{} / {}",pair[0].cue.text,pair[1].cue.text)));
        }
    }
    for c in &p.longform.chapters {
        let text = format!("{} {}", c.title, c.summary).to_lowercase();
        let medical = ["cancro", "farmac", "cancer", "drug record"]
            .iter()
            .any(|s| text.contains(s));
        let context = crate::asr::active(p, c.asset_id)
            .into_iter()
            .flat_map(|t| &t.segments)
            .filter(|s| {
                s.end_ms >= c.source.start_ms.saturating_sub(180000)
                    && s.start_ms < c.source.end_ms + 180000
            })
            .map(|s| s.text.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
        let production = [
            "doppiagg",
            "reaper",
            "mixaggio",
            "plugin",
            "traccia",
            "dialogue",
            "voiceover",
        ]
        .iter()
        .filter(|s| context.contains(**s))
        .count();
        let framed = ["dialogh", "cinematograf", "doppiagg", "mixaggio", "reaper"]
            .iter()
            .any(|s| c.title.to_lowercase().contains(s));
        if medical && production >= 2 && !framed {
            issues.push(issue("quoted_dialogue_context","chapters",Some(c.id),timeline(p,c.asset_id,c.source),"Possibile dialogo citato dentro una lezione di produzione audio:non trattare la battuta del film come argomento reale. Rivedi titolo e riassunto nel contesto.",format!("Titolo:{} · contesto:{production}segnali editoriali;euristica non semantica",c.title)));
        }
        if c.evidence.is_empty() {
            issues.push(issue(
                "chapter_no_evidence",
                "chapters",
                Some(c.id),
                timeline(p, c.asset_id, c.source),
                "Capitolo senza evidenze testuali:verifica manualmente prima di consegnarlo.",
                &c.title,
            ));
        }
    }
    let d = &p.social.thumbnail;
    if !d.title.is_empty() {
        if !crate::fonts::available(&d.font) {
            issues.push(issue(
                "thumbnail_font_fallback",
                "thumbnail",
                None,
                None,
                "Font miniatura assente:fallbackArial, rivedi la composizione.",
                &d.font,
            ));
        }
        if d.x - d.title_width / 2. < 0.03 || d.x + d.title_width / 2. > 0.97 {
            issues.push(issue(
                "thumbnail_safe_width",
                "thumbnail",
                None,
                None,
                "Box titolo miniatura vicino/fuori bordo:riduci larghezza o riposiziona.",
                &d.title,
            ));
        }
        if d.backing && d.color.len() == 7 {
            let a = luminance(&d.color);
            let b = luminance("#151020");
            let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
            if contrast < 3. {
                issues.push(issue("thumbnail_contrast","thumbnail",None,None,format!("Contrasto testo/fondo circa{contrast:.2}:1:prova un colore più leggibile e controlla a320px."),&d.title));
            }
        }
        if d.title.chars().count() > 70 || d.title.lines().count() > 3 {
            issues.push(issue(
                "thumbnail_density",
                "thumbnail",
                None,
                None,
                "Titolo miniatura denso:prova poche parole leggibili senza inventare promesse.",
                &d.title,
            ));
        }
    }
    if p.timeline.clips.iter().any(|c| c.volume > 1.) {
        issues.push(issue("audio_gain_review","audio",None,None,"Una traccia supera guadagno unitario:misura peak/LUFS e ascolta il mix esportato. Nessuna normalizzazione automatica.","Guadagno clip >1"));
    }
    if p.social.profiles.iter().any(|s| s.id == p.social.selected)
        && !p
            .social
            .metadata
            .get(&p.social.selected)
            .is_some_and(|d| d.reviewed)
    {
        issues.push(issue("social_metadata_unreviewed","social",None,None,"Metadati ancora in bozza:controlla veridicità, privacy, diritti e CTA prima della pubblicazione.","Nessun upload automatico"));
    }
    issues.truncate(500);
    Report{schema_version:1,project_updated_at:p.updated_at,producer:"local_editorial_rules_v1",real_ai_inference:false,issues,languages,checks:vec!["Densità/velocità testoUnicode, timing parole, sovrapposizioni","Lingue separate, traduzioni e fallback font","Contesto dialoghi citati (euristica IT/EN), non fact-checking","Leggibilità miniatura su fondo noto, guadagno audio e metadati"],limitations:vec!["Non misura WER/CER o accuratezzaASR:serve ground truth indipendente.","Nessuna certificazione artistica/virality;lettura e ascolto restano necessari.","Velocità in Unicode scalars,non clustergrapheme;overflow e volti richiedono controllo del render.","L’euristica di citazione segnala alcuni dialoghi di produzione,non comprende universalmente il contesto.","Massimo500avvisi nel report;nessun taglio o correzione viene applicato."]}
}
pub async fn view(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Report>, ApiError> {
    let p = state
        .projects
        .read()
        .await
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Progetto assente"))?;
    Ok(Json(report(&p)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thumbnail_low_contrast_and_unreviewed_metadata_are_explainable_not_scores() {
        let mut p = Project::new("Test".into(), None);
        p.social.selected = "youtube".into();
        p.social.thumbnail.title = "Titolo".into();
        p.social.thumbnail.color = "#151020".into();
        let r = report(&p);
        assert!(r.issues.iter().any(|i| i.code == "thumbnail_contrast"));
        assert!(r
            .issues
            .iter()
            .any(|i| i.code == "social_metadata_unreviewed"));
        assert!(!r.real_ai_inference);
        assert_eq!(
            serde_json::to_value(r).unwrap()["producer"],
            "local_editorial_rules_v1"
        );
    }
}
