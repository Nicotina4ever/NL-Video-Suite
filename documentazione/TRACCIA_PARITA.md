# Traccia di parità NL — design esecutivo P1–P7

Data: 23/11/2026. Fonte di verità dello stato: `PARITA_FEATURE.md`; inventari verificati:
`inventario/{regista_ai,clipsai,calliope,zp_video}.md`. Questo file fissa **come** la parità viene
portata in `filmcraft-nl`: comandi headless, pannelli, impostazioni, test e ricevute.

## Scope

Dentro: tutto ciò che serve al flusso editoriale della suite, non già coperto dalle fasi F2–F6:

- **P1** Impostazioni NL uniche e persistite.
- **P2** ASR completo (modello/device/cache/import) + traduzioni EN/ES.
- **P2b** Branding: nessun "FilmCraft" nei testi visibili; pannelli NL dedicati.
- **P3** Clipping social e Sgrossatura.
- **P4** Storyboard/Storia (idea → beat → scene → shot → media, coda).
- **P5** Musica generativa con workflow/modelli installati. **TTS escluso**: l'operatore è
  doppiatore; un TTS NL sarebbe un placeholder. Il pannello Text to Speech esistente (voce del
  fork) resta com'è e non fa parte della parità.
- **P6** Riparazione workflow, diagnostica e ciclo servizi.
- **P7** Verifica, ricevute, commit, regressione e documentazione.

Fuori scope (deciso con l'operatore): **Calliope Build Scene / AI Canvas / 3D** (Three.js,
MediaRecorder) — non è una funzione NLE e il port in egui non è giustificato ora; resta come
eventuale P8 separato. Fuori scope anche l'harness agente completo di Calliope e il server web di
Regista: se ne replica il comportamento editoriale, non l'architettura.

## Architettura

### 1. Impostazioni NL (P1)

Nuovo struct `NlPrefs` in `crates/engine/src/settings/nl.rs`, incluso in
`autosave::Preferences` (serializzato in `preferences.json`, quindi persistente per utente) e
esposto come categoria `nl` in `settings::schema::categories()`. Così `prefs.get/set/reset`,
il dialogo Impostazioni e il pannello Impostazioni NL vedono gli stessi valori, senza doppioni.

Chiavi (tutte con default sicuri, nessun download):

| Chiave | Tipo | Default | Uso |
|---|---|---|---|
| `nl.comfyUrl` | text | `http://127.0.0.1:8188` | client ComfyUI |
| `nl.comfyImageWorkflow` | text | vuoto = default | workflow immagine |
| `nl.comfyVideoWorkflow` | text | vuoto = default | workflow video/I2V |
| `nl.comfyMusicWorkflow` | text | vuoto = default Ace Step | workflow musica |
| `nl.comfyWorkflowDir` | path | vuoto = `workflows/regista` | elenco workflow |
| `nl.ollamaUrl` | text | `http://127.0.0.1:11434` | LLM |
| `nl.ollamaModel` | text | `lfm2.5:latest` (fallback: primo installato) | ranking/traduzioni/prompt |
| `nl.ollamaModelVision` | text | vuoto = stesso | visione |
| `nl.asrModel` | text | `large-v3` | bridge WhisperX |
| `nl.asrDevice` | choice | `auto` (GPU-first, fallback CPU dichiarato) | bridge |
| `nl.asrLanguage` | choice | `it` | bridge |
| `nl.asrCache` | bool | true | cache trascrizione |
| `nl.asrCacheDir` | path | vuoto = dati utente | cache |
| `nl.imageScale` | choice `1x`/`2x` | `1x` | risoluzione immagini dalla sequenza |
| `nl.videoScale` | choice `2/3`/`1/2` | `2/3` | risoluzione video dalla sequenza |
| `nl.sizeMultiple` | choice 8/16 | 8 | arrotondamento |
| `nl.gpuFirst` | bool | true | ASR CUDA poi CPU, una volta |
| `nl.queueTimeoutSec` | int | 1800 | timeout job ComfyUI |
| `nl.queuePollSec` | int | 2 | polling |
| `nl.serviceComfyDir` | path | vuoto | avvio ComfyUI |
| `nl.serviceOllamaPath` | path | vuoto | avvio Ollama |
| `nl.musicModel` | text | vuoto = dal workflow | selezione modello musica |
| `nl.voiceRefsDir` | path | vuoto | **solo** elenco reference (nessun TTS NL) |

Regola d'oro: i comandi leggono **parametro esplicito → impostazione NL → variabile d'ambiente →
default**, così CLI/agenti possono sempre forzare, e la GUI usa le impostazioni.

La risoluzione non inventa nulla: `resolution_from_sequence` diventa l'unica funzione
(`nl::size::from_sequence`) usata da immagini e video, con profilo da impostazioni e override nel
dialogo. Se la sequenza non ha dimensioni valide, il comando fallisce dichiarandolo.

### 2. Modulo motore `crates/engine/src/nl/`

```
nl/mod.rs        // comandi pubblici + helper condivisi (JSON, HTTP, ffmpeg, job)
nl/settings.rs   // NlPrefs + accesso tipizzato
nl/size.rs       // risoluzione dalla sequenza (1x/2x, 2/3, 1/2, multipli 8/16)
nl/services.rs   // status/start/stop ComfyUI e Ollama, diagnostica
nl/llm.rs        // client Ollama JSON (chat, lista modelli, traduzione, ranking)
nl/models.rs     // elenco workflow e modelli installati (filesystem + Ollama /api/tags)
nl/asr.rs        // trascrizione NL (proxy a nl_asr) + import SRT/VTT + traduzioni
nl/clipping.rs   // candidati editoriali (score/dedupe), SRT normal/hormozi, reframe, burn
nl/sgrossa.rs    // piano tagli (silenzi/fiati/filler/bad-takes), apply, report, anteprima
nl/story.rs      // storia: beat/cast/luoghi/scene/shot su sidecar JSON del progetto
nl/music.rs      // musica con workflow installati (riusa comfy::generate_music)
nl/queue.rs      // vista coda unificata (job interni + coda ComfyUI)
```

I comandi si registrano in `commands::build()` con `v.extend(crate::nl::commands())`, come già
fanno `settings`, `shorts`, `comfy`. Ogni comando ha: id, label, `params` documentati, `enabled`,
`run`; le funzioni che generano media in background aggiornano `Session.jobs` come le esistenti.

### 3. Pannelli NL (P2b)

Nuovi `PanelKind` in `crates/ui-egui/src/dock.rs` e corpo in `crates/ui-egui/src/panels/nl/`:

| Pannello | Titolo | Contenuto minimo |
|---|---|---|
| `NlGenerate` | NL Generate | prompt, workflow, seed, profilo risoluzione (1×/2×) con valori calcolati, Genera, stato job |
| `NlMusic` | NL Music | prompt/lyrics, durata, workflow, Genera |
| `NlAsr` | NL ASR | lingua, device (auto/GPU/CPU), modello, Trascrivi, import SRT/VTT, traduzioni EN/ES |
| `NlClipping` | NL Clipping | analizza, tabella candidati (score, durata, hook), selezione, SRT, reframe 9:16, burn |
| `NlSgrossa` | NL Rough Cut | analizza, piano tagli con motivi, modifica manuale, applica, report, anteprima |
| `NlStory` | NL Story | idea, beat, personaggi/luoghi, scene, shot, prompt, genera immagine/video per shot, coda |
| `NlQueue` | NL Queue | job raggruppati per tipo/workflow, stato, annulla/riprova, coda ComfyUI |
| `NlSettings` | NL Settings | tutte le chiavi P1 + stato servizi + diagnostica + elenco modelli installati |

Regole UI: palette NL (viola/magenta/ciano), testi **senza "FilmCraft"**, i pannelli si aprono da
Window (il registro `PanelKind::ALL` li aggiunge) e le funzioni restano raggiungibili da menu e
comandi. Nessun pannello duplica stato: legge `Session` e le risposte dei comandi.

## Comandi previsti (headless, testabili)

- P1: nessun comando nuovo obbligatorio (`prefs.schema/get/set` esistenti), più
  `nl.models.workflows`, `nl.models.ollama`, `nl.diagnostics`.
- P2: `transcript.transcribeNl` (esiste, esteso con modello/cache), `transcript.importSubtitle`,
  `transcript.translate`, `transcript.translations`.
- P3: `nl.clipping.propose`, `nl.clipping.render`, `nl.clipping.srt`, `nl.clipping.reframe`,
  `nl.clipping.burn`, `nl.sgrossa.analyze`, `nl.sgrossa.plan.set`, `nl.sgrossa.apply`,
  `nl.sgrossa.report`, `nl.sgrossa.preview`.
- P4: `nl.story.get`, `nl.story.set`, `nl.story.draft`, `nl.story.prompts`, `nl.story.generate`
  (immagine/video per shot, accodato).
- P5: `audio.generateMusic` (esiste, esteso con workflow/modello da impostazioni).
- P6: `nl.workflows.diagnose`, `nl.workflows.repair`, `nl.services.status`,
  `nl.services.start`, `nl.services.stop`.
- Coda: `nl.queue.list`, `nl.queue.cancel`.

## Test e ricevute

- Unit test motore per ogni comando puro: risoluzione (1×/2×, 2/3, 1/2, multipli 8/16),
  impostazioni (default, precedenza parametro > prefs > env), parsing candidati/sgrossa,
  sanitizzazione nomi, stato servizi, piano tagli (silenzi e filler su tracce sintetiche).
- Test UI: pannelli registrati, titoli senza "FilmCraft", palette, `PanelKind::from_name` per i
  nuovi pannelli, comandi bindabili.
- Test d'integrazione con provider finti: Ollama e ComfyUI simulati via HTTP locale (già usato in
  `comfy.rs` test) — nessuna GPU, nessun download.
- Ricevute: `documentazione/prove/p1-.../receipt.json` + screenshot GUI quando i pannelli sono
  pronti; ogni fase = commit separato con messaggio `P<n>: ...`.

## P8 — versione test classica (inventario `inventario/versione_test.md`)

Tutte le funzioni della versione test vanno portate come pannelli/menù/comandi, **senza perdere
il default rapido**. Sotto-fasi:

- **P8b default rapido**: workspace di avvio **Quick Edit** (Program, Project, Effects, Timeline,
  meters; nessun pannello NL agganciato), nessun contatto con ComfyUI/Ollama all'apertura, liste
  costose in cache, test `opening_a_project_starts_no_ai_work`, misure in `PERFORMANCE.md`.
- **P8c testo/longform**: analisi longform (topics, chapters, rough-cut preview/apply), indice
  FTS5 e ricerca, assistente correzioni review-only, import SRT/VTT, traduzioni EN/ES.
- **P8d montaggio/storia**: montage planner, creative montage + audio onset, story/narrative
  (piano, scene, assemble, preflight), assistente narrativa, reference narrative, shot map,
  pipeline/flows review-gated. Include P4: storia AI con affinamenti e **posizionamento in
  timeline** degli elementi generati. *Prima verticale fatta (23/11):* `nl/story.rs` + pannello
  NL Story (draft/refine/update, keyframe ComfyUI con storyTarget, place/placeAll, sidecar
  `nl/story.json`); prova live in `prove/p8d-story/`. Restano video per scena, v2v, estensione
  sezioni e gli strumenti di montaggio.
- **P8e sottotitoli/grafica/audio**: subtitle studio completo (struttura, sync, align, export),
  motion/titoli nativi, graphic objects, narration/audio master.
- **P8f servizi/workflow/progetto**: workflow doctor + verifica API (unione P9a), import/parametri
  workflow, provider/servizi start/stop, versioni montaggio, album, production package, storage,
  social QA/export, thumbnail nativa.

## P9 — Prompter_Comfyui (inventario `inventario/prompter_comfyui.md`)

- **P9a** (fatto): workflow copiati in `workflows/prompter/` (+ provenance); **doctor unito alla
  verifica API**: `nl.workflows.verify` legge `/object_info` (sola lettura), `nl.workflows.diagnose`
  fa controlli statici + live (classi, modelli, reference, link, prompt, dimensioni, difetti noti),
  `nl.workflows.fix` scrive una copia riparata, `nl.workflows.import` aggiunge workflow; pannello
  **NL Workflows** con Verify, Doctor, Fix, Import.
- **P9b** parametri reali da `/object_info` (enum/intervalli) e Prompt AI (brief → proposte JSON
  validate → diff → applica), con bindings per workflow.
- **P9c** coda varianti deterministiche (seed/tag), PREPARA (nessun upload) e INVIA con conferma,
  dedupe; `/free` ComfyUI ogni N generazioni e pulsante manuale — senza mai uccidere processi.
- **P9d** staging reference video/audio per v2v/r2v/extend: immagini via `/upload/image`, video e
  audio nella cartella input ComfyUI con percorso relativo `Prompter_ComfyUI/<nome>` solo su
  loopback; generazione con input e inserimento/estensione in timeline.

## Ordine e criteri di accettazione
1. **P1** — `prefs.schema` contiene `nl`; i valori sopravvivono a riavvio; i comandi usano le
   impostazioni; test verdi. **Fatto = commit + ricevuta.**
2. **P2b** — nessuna stringa "FilmCraft" nei testi visibili (grep mirato + test); 8 pannelli
   apribili e con i campi minimi; test UI verdi.
3. **P2** — ASR da pannello con device/fallback dichiarato; traduzioni EN/ES scritte nel mirror
   senza sovrascrivere l'originale; ricevuta con output reale o provider finto dichiarato.
4. **P3** — clipping e sgrossa producono piano/report/artefatti; applicazione su timeline con undo;
   test su media sintetico; ricevuta.
5. **P4** — storia persistita per progetto, shot con prompt, generazione accodata, coda visibile;
   test e ricevuta.
6. **P5** — musica con workflow/modello scelto tra gli installati; nessun download; ricevuta.
7. **P6** — diagnosi/riparazione deterministica su workflow di test; status servizi; start/stop
   solo su richiesta esplicita; nessuna sovrapposizione Ollama/ComfyUI.
8. **P7** — clippy/test verdi, regressione GTA6, documenti aggiornati, spazio controllato.
