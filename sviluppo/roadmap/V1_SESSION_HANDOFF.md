# Session Handoff - 19 novembre 2026

## Stato da cui riprendere

Leggi prima questo file e [Stato e roadmap operativa](../../documentazione/STATO_E_ROADMAP_1_0.md). La specifica completa unica è [V1_MASTER_ROADMAP.md](V1_MASTER_ROADMAP.md). Non ricominciare audit o pipeline già realizzati.

**Candidato certificato:** `sviluppo/target/release/nl-video-suite-context.exe`
**SHA256:** `f8856961baa4a952383972e5bb643cbbef2960fa8976ebee0910a22372909b87`
Versione 0.7.1, NON 1.0. Apri con `sviluppo/tools/Avvia_Anteprima_Creativa.ps1`: il launcher sceglie automaticamente l'EXE più recente con report di verifica e hash corrispondenti.

## Verifiche eseguite su questo candidato

- `cargo fmt --check` PASS.
- `cargo clippy --release --offline --all-targets -- -D warnings` PASS.
- `cargo test --release --bin nl-video-suite` **179 PASS, 0 FAIL**.
- Baseline mirata `20261119T004021Z` PASS: `longform_chapters` + `project_delete_browser`, SHA del report uguale all'EXE. Nessuna inferenza reale: LLM simulato, FFmpeg/Edge reali.
- `longform_chapters` copre digest deterministico, sintesi LLM in cache, pannello Panoramica, cache argomenti v3, capitoli/evidenze, export MP4 36 s.
- `project_delete_browser` copre avvio, migrazione cartelle leggibili, cancellazione progetto e conservazione dell'originale.

## Lavoro completato

1. **Compilazione ripristinata**: import `store` in `storage.rs`; `edit_versions::production::recover_pending` non dipende più dal nome cartella UUID ma da `.project-id`.
2. **Cartelle riconoscibili**: `<uuid>` → `<Titolo>--<id breve>` con marcatore `.project-id`, ricevuta `data/project-folders-migration.json` e rollback automatico se il marcatore non si scrive. I nuovi progetti nascono già con nome leggibile.
3. **Duplicati non bloccanti**: vince `updated_at` più recente; la copia viene saltata, mai sovrascritta, e segnalata in `data/project-folder-issues.json` + `data_warnings` in `/api/health`.
4. **Trascrizione completa e navigabile**: durata sorgente, inizio/fine parlato, intervallo della pagina, pulsanti Inizio/Fine testo e nota esplicita quando il parlato finisce prima del file.
5. **Contesto dell'intero video**: digest deterministico sempre presente (durata, parlato, copertura, capitoli, parole chiave, apertura/chiusura) + una sintesi LLM opzionale, salvata in `analysis/longform-overview` e con fallback al solo digest se il modello non è disponibile. Cache argomenti aggiornata a `contextual-topics-v3`.
6. **Test adattati**: 30 file in `sviluppo/tests/` risolvono il progetto tramite `projectDir`/`firstProjectId` (`sviluppo/tests/project_folder.cjs`) invece di assumere `projects/<uuid>`.

## Verifica sul progetto reale (dati migrati)

"Mi ficco in GTA" (`sviluppo/anteprima_creativa/data`): file 24:28.77, parlato 0:00.00 → 24:27.01, 3048 parole, 170 segmenti, ultimi 1,75 s senza testo. Nessun ASR rilanciato.

Il 19/11/2026 il candidato è stato avviato sui dati reali dell'Anteprima: migrazione eseguita e tracciata in `data/project-folders-migration.json` (`Trailer--82f391db`, `Progetto Compleanno Elita--9dc2860a`, `Mi ficco in GTA--b64f74d2`), nessun duplicato, `data_warnings` vuoto. Verifica UI headless sul progetto reale: `#lf-total` = "3048 parole · sorgente 24:28.77 · parlato 0:00.00 → 24:27.01", nota "gli ultimi 0:01.75 del file non hanno testo", pagina "1–200 di 3048 · 0:00.00 → 1:17.09", pannello Panoramica con digest (copertura 99%, parole chiave, apertura reale) e zero errori JS.

## Limiti dichiarati

- Nessuna regressione generale nuova; la baseline è mirata. `browser.cjs` resta obsoleto rispetto alla UI corrente (import spostato in `#editor-import`, `#render` non visibile nella vista compatta): è un debito di test preesistente, non un difetto introdotto qui.
- Migrazione dati reali eseguita solo sul data-dir dell'Anteprima (`sviluppo/anteprima_creativa/data`), reversibile con la ricevuta `project-folders-migration.json`. Gli altri data-dir restano UUID e migreranno al primo avvio del candidato. Progetti, stabile e originali non sono stati toccati.
- Nessun test GPU, nessuna inferenza ASR/LLM reale, nessun render lungo: Topaz Video era in esecuzione.
- La promozione 1.0 e la redistribuzione restano bloccate dai gate esistenti. Stabile locale SHA `61442816` invariata.

## Prove rimandate (da fare quando Topaz Video è terminato)

1. Sintesi LLM reale sul progetto "Mi ficco in GTA" con modello già installato, verificando cache e fallback.
2. Accettazione GUI completa del candidato con dati reali migrati (apertura, chiusura, riapertura, cancellazione).
3. Regressione generale `v1_baseline.py` completa, inclusi i test browser da riallineare alla UI corrente.
4. Solo se richiesto: export lunghi e test audio/video pesanti.

## Dopo questo lavoro

Restano i gate noti: QA audiovisiva/narrativa indipendente, parità residua, recovery/performance, licenze, portable completo, congelamento e regressione finale prima di qualunque promozione autorizzata. Non ripetere pubblicazione, CoreQA o inferenze già esistenti. Deduplicazione Gemini 1,368 GiB e quarantena `da_eliminare` restano da svuotare manualmente; non dire già liberate.
