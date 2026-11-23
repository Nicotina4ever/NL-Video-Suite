# Inventario — versione test classica di NL Video Suite

Data: 23/11/2026. Fonte: `K:\AI\NL_Video_Suite` (letto in locale, nessuna esecuzione).
Perimetro: la **versione test classica** della suite, cioè l'app `app/nl-video-suite.exe`
(axum + shell Svelte in `sviluppo/web-app`, servita su `/shell`) e i suoi moduli in
`sviluppo/src/*.rs`; il pacchetto pubblicato è
`distribuzione/NL_Video_Suite-Experimental-2026.11.18-Windows-x64-Core.zip`.
Scopo: elencare **tutto** ciò che quella versione già faceva, per portarlo in `filmcraft-nl`
(fork FilmCraft) senza perdere funzioni, con la mappa verso pannelli/comandi del fork.

Metodo: doc-comment di ogni modulo (`//!`), firme pubbliche e superficie HTTP (165 route uniche
`/api...` in `sviluppo/src/main.rs`), shell (`sviluppo/web-app/src/App.svelte` e componenti),
registro di `sviluppo/src/*.rs` elencato sotto. Ogni voce ha l'evidenza; ciò che non è stato
verificato è marcato **NON VERIFICATO**.

## 0. Regola di prodotto

La versione test è il riferimento delle **funzioni già scremate** dalle cartelle delle app
originali; il fork deve contenerle tutte, ma **di base deve restare rapido per i montaggi veloci**:
avvio e editing prima di tutto, AI solo su richiesta, nessun job all'apertura, pannelli NL non
precaricati. Le funzioni pesanti sono pannelli/comandi che si aprono quando servono.

## 1. Superficie della versione test

- **Backend**: `sviluppo/src` ~80 moduli Rust; entry `main.rs` con 165 route `/api...`; job
  persistenti con recupero (`/api/jobs`, `/api/jobs/{id}/recover-comfy`); `/api/health`,
  `/api/runtime`, `/api/integrations`, `/api/v1/capabilities`.
- **Shell**: `sviluppo/web-app/src/App.svelte` con workspace **materials / edit / social / story /
  subtitles / classic** (`App.svelte:212-256,451-462`), palette comandi, pannelli
  `Timeline.svelte`, `Program.svelte`, `Transcript.svelte`, `Shorts.svelte`, `Subtitles.svelte`,
  `Inspector.svelte`, `Sorgente.svelte`, `Media.svelte` (`web-app/src/lib/`).
- **Dati**: `project.json` per progetto, `index.db` SQLite FTS5 derivato, `data/projects/<Titolo>--<id8>`,
  proxy/preview rigenerabili, ricevute di migrazione (`documentazione/ARCHITETTURA.md:12-18`).

## 2. Catalogo funzioni per gruppo

Legenda stato nel fork: ✅ già portato | 🟡 parziale | ⬜ da portare. La colonna “dove” indica il
pannello/IMPOSTAZIONE previsto nel fork.

### 2.1 Progetto, media, timeline, export

| ID | Funzione (versione test) | Evidenza | Dove nel fork | Stato |
|---|---|---|---|---|
| T01 | Apri/crea/elenca/elimina progetto; rename con ricevuta; anteprima eliminazione | `main.rs` route `/api/projects*`, `/api/projects/{id}/delete-preview`; `project_management.rs` | Project panel + comandi progetto | ✅ (fork nativo) |
| T02 | Import asset/media, upload, thumbnail, waveform, frame, playback proxy | route `/assets*`, `playback.rs`, `previews.rs`, `waveform.rs`, `thumbnail.rs` | Media/Project/Program | ✅ (fork nativo) |
| T03 | Timeline: clip add/split/reframe, algebra ripple/roll/slip/slide, snap, marker, bins | route `/timeline*`, `main.rs` algebra; `time_map.rs` | Timeline nativa | ✅ (fork nativo) |
| T04 | Export timeline ed export per formato; capability encoder | route `/export`, `/export/capabilities`; `export_encoding.rs` | File ▸ Export | ✅ |
| T05 | Versioni di montaggio (checkpoint nominati, restore, rinomina) | route `/edit-versions*`; `edit_versions.rs` | pannello Coda/Versioni (P8f) | ⬜ |
| T06 | Legacy import (progetti delle vecchie app) | `/legacy-import`; `legacy.rs`, `migration.rs` | File ▸ Import (esiste Import NL) | ✅/🟡 |
| T07 | Album: modelli, analisi, script, pausa, recupero evidenza | `/api/v1/album/models`, `/montage/album/*`; `album.rs` | pannello Album (P8f) | ⬜ |
| T08 | Production package/preset (consegna pronta) | `/production-package`, `/production-preset`; `production_package.rs`, `production_preset.rs` | Esporta ▸ Pacchetto (P8f) | ⬜ |
| T09 | Storage: scansione modelli, anteprima pulizia, cleanup conservativo | `/storage/*`; `storage.rs` | Impostazioni ▸ Manutenzione (P8b/P6) | ⬜ |

### 2.2 Testo, longform, ricerca, traduzioni

| ID | Funzione | Evidenza | Dove nel fork | Stato |
|---|---|---|---|---|
| T10 | ASR versionato: provider/catalog/discover/hardware/store, profili, selezione raw | `/api/v1/asr/*`, `/projects/{id}/asr*`; `asr.rs`, `asr_catalog.rs`, `asr_store.rs`, `resources.rs` | pannello ASR + Impostazioni NL | 🟡 (bridge sì, catalog/store no) |
| T11 | Trascrizione, correzioni IA ancorate al testo mostrato, SRT, import SRT | route `/transcripts/*`, `/captions/*`; `transcript_core.rs`, `transcript_assistant.rs`, `caption_core.rs`, `subtitle_history.rs` | Text panel | 🟡 (correzioni sì, import SRT no) |
| T12 | Traduzioni EN/ES per transcript e SRT | `/translate`, `/translations/{language}` | Text ▸ Traduci (P2) | ⬜ |
| T13 | Longform: overview, topics, chapters, rough-cut candidati, preview A/B, apply, time-map | `/v1/projects/{id}/longform*`, `/time-map`; `longform.rs`, `longform_analysis.rs`, `longform_preview.rs`, `time_map.rs` | pannello Longform (P8c) | ⬜ |
| T14 | Indice SQLite FTS5 e ricerca con tempi/contesto | `/v1/projects/{id}/search`; `project_index.rs` | Ricerca/Filtro (P8c) | ⬜ |
| T15 | Assistente trascrizione: proposte di correzione review-only | `/transcripts/{id}/corrections`; `transcript_assistant.rs` | Text ▸ Assistente (P8c) | ⬜ |

### 2.3 Sottotitoli e grafica di testo

| ID | Funzione | Evidenza | Dove nel fork | Stato |
|---|---|---|---|---|
| T16 | Subtitle studio: struttura cue, sync, align selettivo CPU, manuale, frame, export formati, sidecar | `/v1/projects/{id}/subtitles*`; `subtitle_studio.rs`, `subtitle_alignment.rs`, `caption_core.rs` | pannello Sottotitoli (P8e) | 🟡 (Text/captions del fork + SRT NL) |
| T17 | Motion/graphics: libreria ASS, grafici nativi, titoli, export/preview, idea e stile suggeriti | `/v1/projects/{id}/motion*`; `motion.rs`, `graphics.rs` | pannello Motion (P8e) | 🟡 (graphics nativo FilmCraft) |
| T18 | Graphic objects: draft ancorati al parlato, descrizione/generazione/approvazione | `/graphic-objects*`, `/graphics/*`; `graphic_objects.rs`, `generation.rs` | pannello Grafica AI (P8e) | ⬜ |
| T19 | Font catalog locale (sola lettura) | `/v1/fonts`; `fonts.rs` | Impostazioni/Titoli | 🟡 (font di sistema FilmCraft) |

### 2.4 Shorts, social, QA, copertine

| ID | Funzione | Evidenza | Dove nel fork | Stato |
|---|---|---|---|---|
| T20 | Shorts: suggest/manual/apply/batch/reframe con candidati trasparenti | `/v1/projects/{id}/shorts*`; `shorts_studio.rs` | NL Clipping + Find Shorts | 🟡 (Find Shorts + Clipping, batch/reframe separati) |
| T21 | Social: profili versionati, batch, export per formato, QA editoriale (spiegabile, no scores) | `/v1/projects/{id}/social*`; `social.rs`, `editorial_qa.rs` | pannello Social (P8d/P8f) | ⬜ |
| T22 | Copertina nativa full-res da snapshot della timeline | `/social/thumbnail`; `thumbnail.rs` | NL Cover (P4b, con AI) | ✅ (con AI; manca la variante “nativa senza AI” → P8f) |

### 2.5 Audio

| ID | Funzione | Evidenza | Dove nel fork | Stato |
|---|---|---|---|---|
| T23 | Audio master PCM rigenerabile, cache/prepare | `/audio*`; `audio_master.rs` | audio engine fork | 🟡 |
| T24 | Mixer: gain bus, sidechain, loudness misurata | `audio_mixer.rs`; route mixer in `/timeline` | Mixer/Essential Sound | ✅/🟡 |
| T25 | Narrazione/voce: preparazione, durata, mix | `narration` (fork) + `/audio/prepare` | pannello Narrazione (P8e) | 🟡 |
| T26 | Analisi audio montaggio: onset ritmico | `/montage/audio/analyze`; `montage_audio.rs` | pannello Montaggio (P8d) | ⬜ |

### 2.6 Montaggio creativo, planner, narrazione, shot map

| ID | Funzione | Evidenza | Dove nel fork | Stato |
|---|---|---|---|---|
| T27 | Montage planner: analisi/proposta/applica con revisione | `/montage/planner/*`; `montage_planner.rs` (+ `montage_planner/`) | pannello Montaggio (P8d) | ⬜ |
| T28 | Creative montage: sequenza mista con proposte-snapshot | `/montage`, `/montage/preview`; `creative_montage.rs`, `compositor.rs` | pannello Montaggio (P8d) | ⬜ |
| T29 | Story/narrative: story, piano, scene, approve, assemble, export, preflight, audio-first | `/story*`, `/narrative/*`; `narrative.rs`, `narrative/` | pannello Storia (P4/P8d) | ⬜ |
| T30 | Assistente narrativa (proposte review-only, nessun tool arbitrario) | `/narrative/assistant/*`; `narrative_assistant.rs` | pannello Assistente (P8d) | ⬜ |
| T31 | Reference narrative di progetto (analyze/approve/generate/suggest) | `/narrative/references/*`; `narrative_references.rs` | pannello Storia (P8d) | ⬜ |
| T32 | Shot map: scene-change CPU su video/proxy, review | `/assets/{id}/shots*`; `shot_map.rs` | pannello Shot map (P8d) | ⬜ |
| T33 | Pipeline/flows review-gated per video lunghi + export flow | `/pipeline`, `/flows*`; `pipeline.rs`, `pipeline_flow.rs` | pannello Flusso (P8d) | ⬜ |

### 2.7 AI, workflow, servizi, diagnostica

| ID | Funzione | Evidenza | Dove nel fork | Stato |
|---|---|---|---|---|
| T34 | Workflow: elenco, scan, import API JSON, controlli reali, narrative controls | `/workflows*`; `workflow_import.rs`, `workflow_controls.rs`, `narrative_workflows.rs` | NL Generate/Settings | 🟡 (elenco e generazione sì, import/controls no) |
| T35 | Workflow doctor: diagnose/repair deterministico, non distruttivo | `/workflows/diagnose|repair`; `workflow_doctor.rs` | pannello Manutenzione (P6/P8f) | ⬜ |
| T36 | Generazione ComfyUI con submit/job/recovery e preset produzione | `/comfy/submit`, `/jobs/{id}/recover-comfy`; `generation.rs`, `comfy_recovery.rs` | NL Generate/Musica/Cover | 🟡 (submit sì, recovery no) |
| T37 | Provider LLM locali, attivazione, modelli; settings servizi; runtime/integrations | `/providers*`, `/settings`, `/services*`, `/runtime`, `/integrations`; `providers.rs`, `services.rs`, `settings.rs`, `runtime.rs` | Impostazioni NL + pannello Servizi (P6) | 🟡 (status sì, start/stop no) |
| T38 | Rilascio risorse RAM/VRAM (Ollama keep_alive 0; ComfyUI solo a coda vuota, nessun kill) | `/api/v1/resources/release`; `resources.rs` | pannello Coda/Servizi (P6) | ⬜ |
| T39 | Job: lista, dettaglio, archivio, recupero | `/jobs*` | pannello Coda | ✅ |

## 3. Gap prioritari (ordine di lavoro P8)

1. **P8b performance/default rapido** (trasversale): workspace Quick Edit, pannelli NL chiusi di
   default, nessun job AI all'apertura, misura di avvio/edit, guardia nei test.
2. **P8c testo/longform**: longform analysis+chapters+rough-cut preview, indice FTS e ricerca,
   assistente correzioni, import SRT, traduzioni EN/ES.
3. **P8d montaggio/storia**: montage planner + creative montage + audio onset, story/narrative +
   assistente + reference + shot map + pipeline/flows (unione con P4).
4. **P8e sottotitoli/grafica/audio**: subtitle studio completo, motion/titoli, graphic objects,
   narration, audio master.
5. **P8f servizi/workflow/progetto**: workflow doctor, import/controls workflow, provider/servizi
   start/stop e rilascio risorse (unione P6), versioni di montaggio, album, production package,
   storage, social QA/export, thumbnail nativa.

## 4. Vincoli e regole

- Nessuna funzione della versione test va **persa**: se una non è ancora nel fork, resta in questa
  lista finché non è portata e testata.
- Le funzioni pesanti restano **su richiesta**: niente job, indici o analisi all'apertura del
  progetto; l'utente può montare subito e aprire i pannelli NL quando vuole.
- Nessun download, nessun servizio esterno avviato senza azione esplicita, nessun progetto utente
  toccato; sorgenti esterne in sola lettura (le 4 app) e nessuna copia di codice/wording.
- Ogni fase: comando headless + accesso GUI + test + ricevuta + commit, come P1–P4b.
