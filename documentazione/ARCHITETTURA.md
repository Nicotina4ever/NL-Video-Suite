# ARCHITETTURA — NL Video Suite (fonte tecnica unica)

Documento vivo al 21/11/2026. Storia e audit precedenti: `documentazione/archivio/`. Uso: `MANUALE.md`. Stato: `STATO.md`.

## Stack

- **Backend**: Rust 2021, axum 0.8 + tokio, singolo binario `nl-video-suite.exe`. UI classica incorporata via `include_str!`; la **shell Svelte è servita da disco** (`web-dist/` accanto all'exe o `sviluppo/web-dist` in dev) sotto `/shell` con tower-http ServeDir — nessuna ricompilazione per la shell.
- **Frontend shell**: `sviluppo/web-app` — Vite 7 + Svelte 5 + TypeScript, zero framework extra. `base:/shell/`. Test algebra con `node --test`.
- **Media**: FFmpeg/ffprobe (esterno, riusato da PATH o `app/tools`). Motori Python snapshot in `engines/` con `runtime/python` condiviso; bridge in `sviluppo/bridge/`.
- **IA**: provider Ollama o OpenAI-compatibili per ruolo (`role_llm_models`), ComfyUI per generazione. Mai download automatico, mai stop globali.

## Persistenza (contratti)

- `project.json` per progetto = **fonte di verità** (tempi interi ms, intervalli semiaperti, revisioni). Mutazioni solo copia→salvataggio atomico→pubblicazione in memoria.
- Cartelle progetto `<Titolo>--<id8>` con marcatore `.project-id`; risoluzione per marcatore (mai per nome); migrazione UUID→leggibile all'avvio con ricevuta `project-folders-migration.json`; il rename del titolo muove la cartella e aggiunge la ricevuta. Originali esterni = provenienza, non percorso di render.
- **`index.db` SQLite derivato** (rusqlite bundled, WAL, FTS5): parole+segmenti+capitoli+decisioni indicizzati, fingerprint su updated_at/transcripts/revision/corrections/chapters/decisions; ricostruito a richiesta o all'avvio; mai autoritativo, mai creato se la cartella progetto non esiste (una cancellazione non può essere "resuscitata"), rimosso col progetto.
- **Traduzioni** = layer in `project.json`: per transcript e lingua, segmenti mancanti = non tradotti (mai inventati), provenienza engine/model, flag `reviewed`.
- Correzioni trascrizione = `longform.corrections` (word id deterministici v5 su raw+tempi). Il raw ASR non muta mai; una correzione marcata «edited · tempi da verificare» non sposta timestamp.

## API principali (oltre alle route classiche)

- `PUT /api/projects/{id}/title` — rename con guardia revisione/lavori, sposta la cartella, ricevuta.
- `GET /api/v1/projects/{id}/search?q=` — FTS5 con tempi, contesto frase, prefissi `parola*`.
- `POST /api/v1/projects/{id}/transcripts/{tid}/corrections` — proposte IA **validate contro le parole realmente mostrate** (old inesatto/estraneo/no-op scartato server-side); l'applicazione avviene solo via PUT longform guardato.
- `POST /api/v1/projects/{id}/translate`, `GET|PUT .../translations/{lang}`, `GET .../translations/{lang}/srt` — chunk bounded, copertura esplicita.
- `POST /api/v1/resources/release` {confirmation:"SVUOTA"} — rilascio manuale: Ollama keep_alive 0; ComfyUI **solo** se coda vuota; nessun kill.
- `/shell/*` — shell servita dal binario quando `web-dist/index.html` esiste.

## Coda risorse (regola del proprietario 21/11)

RAM/VRAM/Comfy occupate = **attesa**, non errore: `guard_llm` e `asr::execution::resolve` riprovano ogni 15s (deadline dichiarata 24h, messaggi nei log e negli stati job). `cpu_fallback:true` nel metadato ASR quando Auto ripiega in CPU; `cuda` esplicito non fa mai fallback silenzioso; NPU solo se reale. I test con provider finto dichiarano `NL_SUITE_LLM_ADMISSION=off` (i test unit RAM-based sono deterministici via override `#[cfg(test)]`).

## Algebra di montaggio

`sviluppo/web-app/src/lib/editAlgebra.ts`: ripple/roll/slip/slide in ms interi, semantica verificata contro il layer pubblico di FilmCraft (storytold/filmcraft, dual MIT/Apache-2.0 — implementazione indipendente, attribuzione in `licenses/THIRD_PARTY.md`, nessun marchio/codice copiato). Test: `npm run test:algebra` (8 casi, inclusi rifiuto roll non adiacente, clamp testa/coda, media sconosciuto ≠ limiti inventati).

## Frontend shell (struttura)

`App.svelte` (layout, workspace Alt+1…4, palette Ctrl+Shift+P, registry comandi id/label/shortcut/enabled), `lib/Timeline.svelte` (canvas: hit-test 7px come spec, snap 6px, coda write anti-race), `lib/Program.svelte` (monitor con clock dal media), `lib/Transcript.svelte`, `lib/Shorts.svelte`, `lib/Inspector.svelte`, `lib/Sorgente.svelte`, `lib/Splitter.svelte` (pattern ARIA separator), `lib/api|media|types|shell|editAlgebra`. I dati passano solo dalle API del backend: la shell non duplica stato.

## Test e certificazione

Rust: 193 unit (`cargo test --release --bin nl-video-suite`) + fmt/clippy -D warnings. Browser/Node: `sviluppo/tests/*.cjs` (Playwright+Edge headless, provider simulati dichiarati). Baseline: `runtime/python sviluppo/tests/v1_baseline.py [--only …]` produce `tests/artifacts/v1-baseline/<ts>/report.json` con SHA dell'exe — il launcher e `Promuovi_Anteprima.ps1` accettano solo report PASS con SHA corrispondente. Promozione = copia con backup (`app/stabile-backup`) + ricevuta (`app/ultima-promozione.json`) + conferma "PROMUOCI".

## Regole non negoziabili del codebase

1. Nulla si butta: archivio git / `da_eliminare` con ricevute; originali e data-dir intatti.
2. Ogni unità completata = commit immediato.
3. Nessuna inferenza/renders costosa ripetuta senza necessità; cache e raw si riusano.
4. Proposte IA = dati fino all'approvazione umana; mai presentare comportamento non supportato come successo (linguaggio nei messaggi: «bozza da revisionare», «non ancora tradotto», «in attesa di risorse»).
5. La suite non tocca processi/servizi/code altrui (stop/unload globali vietati, anche via UI, se non il rilascio manuale autorizzato qui sopra).
