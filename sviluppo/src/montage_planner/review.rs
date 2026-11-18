//! Deterministic receipts and review gates. Never a claim of artistic accuracy.
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Requirements {
    pub version: u32,
    pub audio_policy: String,
    pub title_item_id: Option<Uuid>,
    pub target_ms: u64,
}
impl Default for Requirements {
    fn default() -> Self {
        Self {
            version: 0,
            audio_policy: "keep_current".into(),
            title_item_id: None,
            target_ms: 0,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub code: String,
    pub severity: String,
    pub item_id: Option<Uuid>,
    pub message: String,
}
fn issue(code: &str, severity: &str, id: Option<Uuid>, message: impl Into<String>) -> Issue {
    Issue {
        code: code.into(),
        severity: severity.into(),
        item_id: id,
        message: message.into(),
    }
}
pub(super) fn has_audio(p: &Project, item: &Item) -> bool {
    p.assets.iter().any(|a| {
        a.id == item.asset_id
            && a.kind == AssetKind::Video
            && a.media.as_ref().is_some_and(|m| m.has_audio)
    })
}
pub(super) fn summary(p: &Project, items: &[Item], transition: u64) -> String {
    let duration = items
        .iter()
        .map(|i| i.duration_ms)
        .sum::<u64>()
        .saturating_sub(
            items
                .windows(2)
                .map(|w| {
                    w[1].transition_ms
                        .unwrap_or(transition)
                        .min(w[0].duration_ms / 3)
                        .min(w[1].duration_ms / 3)
                })
                .sum(),
        );
    let order = items
        .iter()
        .take(12)
        .enumerate()
        .map(|(n, i)| {
            let a = p.assets.iter().find(|a| a.id == i.asset_id);
            let kind = match a.map(|a| a.kind) {
                Some(AssetKind::Video) => "Video",
                Some(AssetKind::Image) => "Foto",
                _ => "Media",
            };
            let name = a
                .map(|a| a.name.chars().take(55).collect::<String>())
                .unwrap_or_default();
            let audio = if has_audio(p, i) {
                format!(" · audio originale {:.0}%", i.source_volume * 100.)
            } else {
                String::new()
            };
            format!(
                "{}. {kind} «{name}» ({:.2}s{audio})",
                n + 1,
                i.duration_ms as f64 / 1000.
            )
        })
        .collect::<Vec<_>>()
        .join(" → ");
    format!(
        "Ordine effettivo: {order}{}. Durata proposta {:.2}s · {} titoli nativi.",
        if items.len() > 12 {
            format!(
                " → … (prime12 di {} scelte; ordine completo nelle schede)",
                items.len()
            )
        } else {
            String::new()
        },
        duration as f64 / 1000.,
        items.iter().filter(|i| !i.title.trim().is_empty()).count()
    )
}
pub(super) fn issues(
    p: &Project,
    items: &[Item],
    requirements: &Requirements,
    mode: &str,
) -> Vec<Issue> {
    let mut result = vec![];
    for i in items {
        if has_audio(p, i) {
            if let Some(duration) = p
                .assets
                .iter()
                .find(|a| a.id == i.asset_id)
                .and_then(|a| a.media.as_ref())
                .and_then(|m| m.duration_ms)
            {
                if i.trim_in_ms > 0 || i.trim_in_ms.saturating_add(i.duration_ms) < duration {
                    result.push(issue("audio_boundary_unverified","warning",Some(i.id),format!("Raccordo audio da ascoltare: estratto {:.2}–{:.2}s su sorgente {:.2}s. Il piano non dimostra un confine di frase, anche se il commento lo garantisce. Puoi correggere tempi/trim o approvare esplicitamente il taglio.",i.trim_in_ms as f64/1000.,(i.trim_in_ms+i.duration_ms) as f64/1000.,duration as f64/1000.)));
                }
            }
        }
        if i.trim_in_ms > 0
            && p.assets
                .iter()
                .any(|a| a.id == i.asset_id && a.kind == AssetKind::Image)
        {
            result.push(issue("still_image_trim","warning",Some(i.id),"Trim su immagine statica: non rappresenta un momento diverso della fotografia. Verifica se serve per la tua animazione; nessuna correzione automatica."));
        }
        if mode == "ai" && i.reason.trim().is_empty() {
            result.push(issue("reason_missing","error",Some(i.id),"Motivazione assente: scrivi perché questo materiale è qui, oppure richiedi una revisione mirata. Non è un’analisi verificata."));
        }
        if i.note.trim().is_empty() {
            result.push(issue("content_unknown","warning",Some(i.id),"Nessuna annotazione utente: i metadati non dimostrano contenuti, persone o eventi. Verifica il materiale e le ipotesi creative."));
        }
    }
    let later_titles = items
        .iter()
        .skip(1)
        .filter(|i| !i.title.trim().is_empty())
        .count();
    if later_titles > 0 {
        result.push(issue("additional_titles","warning",None,format!("{later_titles} titoli presenti dopo l’apertura: verifica se sono voluti, soprattutto quando hai richiesto il solo titolo iniziale. Non vengono rimossi automaticamente.")));
    }
    // Selective application replaces only the chosen existing slots. A title
    // on a later card must not masquerade as the requested opening title.
    if requirements.title_item_id.is_some() {
        let opening = match p.montage.items.first() {
            Some(first) if items.iter().any(|i| i.id == first.id) => items
                .iter()
                .find(|i| p.montage.items.iter().any(|old| old.id == i.id)),
            Some(first) => Some(first),
            None => items.first(),
        };
        if let Some(i) = opening.filter(|i| i.title.trim().is_empty()) {
            result.push(issue("title_missing","error",Some(i.id),"Titolo iniziale richiesto ma assente nell’apertura effettiva: seleziona e compila il primo elemento, oppure rivedi l’ordine."));
        }
    }
    if items.windows(2).any(|w| w[0].asset_id == w[1].asset_id) {
        result.push(issue(
            "repeated_asset",
            "warning",
            None,
            "Lo stesso asset compare in posizioni adiacenti: verifica se la ripetizione è voluta.",
        ));
    }
    if items.iter().enumerate().any(|(n, i)| {
        (i.section == "opening" && n != 0) || (i.section == "finale" && n + 1 != items.len())
    }) {
        result.push(issue("section_order","warning",None,"Le etichette apertura/finale non coincidono con le posizioni proposte. Sono intenzioni creative, non l’ordine effettivo: rivedile nella sequenza."));
    }
    if requirements.audio_policy == "mute_sources" {
        result.push(issue("audio_muted_by_user","info",None,"Audio originale escluso per scelta esplicita della richiesta, non per decisione autonoma del modello."));
    }
    result
}
pub(super) fn finalize(p: &Project, plan: &mut Plan, sources: &[Item], r: &Request) {
    plan.request_instruction = r.instruction.clone();
    plan.request_brief = p.montage.brief.clone();
    plan.model_summary = std::mem::take(&mut plan.summary);
    plan.model_arc = std::mem::take(&mut plan.arc);
    let audio_editable =
        r.revision_fields.is_empty() || r.revision_fields.iter().any(|f| f == "audio");
    let audio_policy = if audio_editable {
        r.audio_policy.as_str()
    } else {
        "keep_current"
    };
    let title_editable = r.revision_fields.is_empty()
        || r.revision_fields
            .iter()
            .any(|f| f == "titles" || f == "order");
    let opening_in_scope = p
        .montage
        .items
        .first()
        .is_none_or(|first| sources.iter().any(|i| i.id == first.id));
    let title_item_id = if r.require_opening_title && opening_in_scope && title_editable {
        plan.sequence.first().map(|i| i.id)
    } else {
        None
    };
    plan.requirements = Requirements {
        version: 1,
        audio_policy: audio_policy.into(),
        title_item_id,
        target_ms: r.target_ms,
    };
    let mut corrections = vec![];
    for i in &mut plan.sequence {
        if let Some(source) = sources.iter().find(|s| s.id == i.id) {
            if has_audio(p, i) {
                let requested = i.source_volume;
                i.source_volume = match audio_policy {
                    "preserve_original" => {
                        if source.source_volume > 0. {
                            source.source_volume
                        } else {
                            1.
                        }
                    }
                    "mute_sources" => 0.,
                    _ => source.source_volume,
                };
                if (requested - i.source_volume).abs() > 0.0001 {
                    corrections.push(issue("source_audio_policy","info",Some(i.id),format!("Volume proposto {:.0}% adeguato alla regola esplicita {audio_policy}: volume {:.0}%. Nessuna modifica ai media originali; se il provider è IA, la risposta raw resta in cache.",requested*100.,i.source_volume*100.)));
                }
            }
        }
        if title_item_id == Some(i.id) && !r.opening_title.trim().is_empty() {
            i.title = r.opening_title.trim().into();
        }
    }
    plan.review_notes = issues(p, &plan.sequence, &plan.requirements, &plan.mode);
    plan.review_notes.extend(corrections);
    for required in &plan.required_items {
        if !plan
            .sequence
            .iter()
            .any(|i| i.asset_id == required.asset_id)
        {
            plan.review_notes.push(issue("mandatory_omitted","warning",Some(required.id),"Il modello ha omesso una foto obbligatoria. Puoi recuperarla senza IA nella revisione e scegliere ordine, durata e raccordi. I clip esistenti non vengono cancellati."));
        }
    }
    plan.summary = summary(p, &plan.sequence, p.montage.transition_ms);
    plan.arc="Le motivazioni dei singoli elementi sono intenzioni creative revisionabili. Annotazioni utente, metadati misurati e descrizioni IA approvate restano fonti distinte; nessuna verifica automatica di persone/eventi.".into();
}
pub(super) fn validate_accept(
    p: &Project,
    plan: &Plan,
    chosen: &[Item],
    audio_overrides: &[Uuid],
) -> anyhow::Result<()> {
    anyhow::ensure!(
        audio_overrides.iter().collect::<HashSet<_>>().len() == audio_overrides.len()
            && audio_overrides
                .iter()
                .all(|id| chosen.iter().any(|i| i.id == *id)),
        "Approvazioni audio estranee o duplicate"
    );
    let failures = issues(p, chosen, &plan.requirements, &plan.mode)
        .into_iter()
        .filter(|i| i.severity == "error")
        .map(|i| i.message)
        .collect::<Vec<_>>();
    anyhow::ensure!(
        failures.is_empty(),
        "Revisione richiesta: {}",
        failures.join(" ")
    );
    for required in &plan.required_items {
        anyhow::ensure!(
            chosen.iter().any(|i| i.asset_id == required.asset_id)
                || p.montage
                    .items
                    .iter()
                    .any(|i| i.asset_id == required.asset_id),
            "Recupera e seleziona la foto obbligatoria mancante prima di applicare il piano"
        );
    }
    if plan.requirements.audio_policy == "preserve_original" {
        for i in chosen.iter().filter(|i| has_audio(p, i)) {
            let base = plan
                .sequence
                .iter()
                .chain(plan.required_items.iter())
                .find(|base| base.id == i.id);
            anyhow::ensure!(audio_overrides.contains(&i.id)||base.is_some_and(|b|(b.source_volume-i.source_volume).abs()<0.0001),"Il volume originale è protetto: approva esplicitamente la variazione manuale, oppure conserva il volume proposto");
        }
    }
    Ok(())
}
