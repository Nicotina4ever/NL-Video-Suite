# NL Video Suite — stato integrazione (22/11/2026)

Build: `filmcraft-nl/target/release/filmcraft.exe` (release, ~80 MB).
Avvio: doppio clic, oppure `filmcraft.exe --control 9876` per l’automazione.

## Cosa è integrato e verificato

| Funzione | Dove | Prova |
|---|---|---|
| Identità NL, icona, palette, wordmark | Finestra, About, taskbar | `prove/f1-branding/` |
| Import progetto NL (`project.json`) | `File > Import NL Project…` | `prove/f2-import/import-verified.png` |
| Media, tracce video/overlay/audio, trim, audio incorporato, trascrizione | Import | GTA6: 9 clip video, 9 audio collegati, 377 parole |
| Correzione parola inline (doppio clic, Invio/Esc) | `Text > Trascrizione` | `prove/f3-text/text-panel.png` |
| Mirror testo NL (`project.synced.json`) | `File > Sync NL Text to Mirror` | 3.545 parole, sezioni NL intatte |
| Find Shorts locale | `Sequence > Find Shorts` | 2 short, marker, bin `AI Shorts` |
| Find Shorts AI (Ollama) | `Sequence > Find Shorts with AI` | job + hook verificabile; test mock ok, test reale bloccato da ComfyUI |
| Export short | `File > Export > Media` | 1080×1920, 30 fps, H.264/AAC, 30,25 s |
| Salvataggio/riapertura `.fcproj` | `File > Save As` / `File > Open` | marker, bin, transcript presenti |

Dettagli tecnici: `F2_IMPORT_NL.md`, `F3_TEXT.md`, `F4_SHORTS.md`.

## Flusso GTA6 verificato

1. Apri NL Video Suite → `File > Import NL Project…` → `Mi ficco in GTA6/project.json`.
2. Apri il pannello `Text`, tab `Trascrizione`: 377 parole sulla timeline.
3. Doppio clic su una parola, correggi, Invio. `Ctrl+Z` annulla.
4. `File > Sync NL Text to Mirror` (facoltativo) → `project.synced.json` accanto al progetto NL.
5. `Sequence > Find Shorts`: crea 2 sequenze in `AI Shorts` e 2 range marker sulla sorgente.
6. Apri uno short dal Project panel; il Text panel mostra le parole del taglio.
7. `File > Export > Media`: 1080×1920, H.264/AAC.

## Cosa manca (dichiarato, non finto)

- **ASR selezionabile Qwen/WhisperX**: richiede il bridge Python del motore NL; oggi funziona
  l’ASR di FilmCraft/Whisper e l’import della trascrizione NL.
- **Traduzioni EN/ES**: richiedono lo stesso bridge LLM.
- **Ranking LLM**: `Find Shorts with AI` è implementato con Ollama e job in background; il test
  con server finto passa. Il primo test reale GTA6 ha restituito una risposta senza JSON: la
  diagnosi è il modello thinking che consuma i token e il contesto non dichiarato. Sono stati
  aggiunti `think: false`, `num_ctx: 16384` e un messaggio d'errore con anteprima della risposta.
  Il retest reale con un modello piccolo è in corso.
- **Longform** come comando: non ancora esposto.
- **ComfyUI**: `graphics.generateComfy` verificato end-to-end (immagine 1024×1024 importata);
  `audio.generateMusic` ha completato il workflow Ace Step (MP3 120 s) e attende il retest
  dell'import nell'app. Accesso GUI: dialoghi nativi in Grafica e titoli / Clip.
- **ASR NL**: bridge WhisperX verificato (42 parole, timing reali) e comando
  `transcript.transcribeNl` aggiunto; il pulsante Trascrivi del pannello Text lo usa.
- **Parità completa**: matrice in `PARITA_FEATURE.md`, fasi P1–P7 in `TODO.md`.
- **Risoluzione**: il motore deriva le immagini dalla sequenza (1×/2×) e i video a 2/3 o 1/2,
  con arrotondamento a 8 px; il dialogo espone la scelta 1×/2×.
- **GPU-first**: ASR `device: auto` prova CUDA e ripiega su CPU una volta; ComfyUI usa la GPU
  gestita dalla propria coda.
- **Portable con motore AI e launcher unico**: F8.

## Ottimizzazione spazio (23/11/2026, passo 2 — dopo P1–P4b)

- Misurato prima: progetto **32,84 GB**, disco K: libero **91,9 GB**.
- Rimossi rigenerabili: `filmcraft-nl/target/debug` **14,30 GB** (la release e le sue dipendenze
  restano), `sviluppo/target` **0,79 GB** (vecchia app classica), **754** `__pycache__` (**73,7 MB**),
  **917** cartelle temporanee `%TEMP%/filmcraft-*` dei test (**756 MB**).
- Spostati in archivio: `sviluppo/tests/artifacts` → `da_eliminare/2026-11-23-ottimizzazione-p2/`
  (**0,16 GB**, artefatti dei test completati della vecchia suite).
- Dopo: progetto **17,67 GB** (−15,17 GB), disco K: libero **105,3 GB** (+13,4 GB).
  Ricevuta e log: `da_eliminare/2026-11-23-ottimizzazione-p2/{receipt.json,cleanup.log}`.
- **Non toccati**: progetti utente (GTA6, Pubblicità, Trailer, Riferimenti autenti, ZMateriale),
  `runtime/` (5,1 GB), `models/` (4,05 GB), `engines/`, release del fork, `app/tools`,
  `documentazione/prove`. Nessun servizio avviato o fermato.
- **Passo finale (P7)**: la cache debug verrà ricreata dai test delle fasi successive; va rimossa
  di nuovo a fine lavoro, insieme alla valutazione di `target/release/deps` (3,3 GB) se non
  servono altre build. `da_eliminare` si svuota solo su richiesta esplicita.

## Parità NL — P8e sottotitoli e titoli (23/11/2026)

- Pannello **NL Subtitles**: cue list della traccia sottotitoli con editing del testo, split al
  playhead, merge con la cue successiva, delete, import/export SRT/VTT dai selettori nativi.
- Pannello **NL Titles**: template MoGRT (8 built-in + utente) con i controlli reali editabili
  (testo, colore, slider, checkbox, font, posizione), inserimento in timeline con `nl.titles.add`
  e responsive intro/outro (`graphics.setResponsiveTime`).
- Prove live: titolo piazzato con controlli e intro/outro 0,5 s; sottotitoli importati, testo
  modificato, split→merge e SRT esportato; `documentazione/prove/p8e-studio/`.

## Parità NL — P5 musica di scena live (23/11/2026)

- `nl.story.extendAudio` → `audio.generateMusic` con Ace Step (workflow regista/audio, doctor
  pulito): musica **MP3 10 s 48 kHz stereo** generata in 250 s, importata, collegata alla scena e
  posizionata su A1 con `nl.story.placeAudio`.
- Campi **Widget overrides JSON** nei pannelli NL Generate e NL Queue (array node/field/value
  validato dal motore: nodi o campi inesistenti rifiutati).
- Evidenze: `documentazione/prove/p5-music/`.

## Parità NL — P9d video live con reference (23/11/2026)

- Prova end-to-end riuscita: prompt H3 full-reference (`nl.prompt.h3`), due reference immagini
  caricate e patchate, **overrides** per stare nel budget GPU (`megapixels=0.25`, durata 2 s),
  generazione MiniMax H3 ref2va e import del risultato.
- Output: **video H.264 672×384 @24 fps, 2,33 s, con audio AAC stereo 32 kHz** (H3 genera video e
  audio), 488 s di generazione su RTX 4060 Ti 8 GB con offload.
- Evidenze: `documentazione/prove/p9d-video/` (mp4, ffprobe, frame, prompt, log).

## Parità NL — P8c assistente con citazioni (23/11/2026)

- `nl.assistant.ask` risponde usando solo i documenti dell'indice di ricerca (i 6 migliori),
  cita le fonti e restituisce l'elenco esatto di fonte/tempo/titolo/estratto; senza documenti
  corrispondenti rifiuta prima di chiamare il modello locale. Riquadro «Ask the project» nel
  pannello NL Analyze.
- Prova live: «Chi è Ada e quale esercizio fa nella scena?» → risposta con [1][2][3] da scena,
  storia e trascrizione in 24 s con qwen3.5:9b; `documentazione/prove/p8c-assistant/`.

## Parità NL — P9c coda varianti e P8c traduzioni (23/11/2026)

- **Coda varianti** (`nl.queue.*`): seed deterministici, dedupe, `prepare` che costruisce con un
  dry run l'esatto payload (niente invii), `send` solo con `confirm: true`, rimozione e
  **free VRAM** confermato su endpoint loopback. Pannello **NL Queue** con doppia conferma per invio
  e free. `graphics.generateComfy` accetta ora `overrides` [{node, field, value}] validati.
  Prova live: 3 varianti preparate (100/101/102) e una inviata davvero (seed 4242, 512×512,
  completata in 182 s); `documentazione/prove/p9c-queue/`.
- **Traduzione sottotitoli** (`nl.translate.captions/text`): batch da 40 cue, validazione id/testo,
  nuova traccia con gli stessi tempi, originale intatta; nel pannello NL Analyze. Prova live: 3 cue
  IT → EN con qwen3.5:9b e nuova traccia «cues · EN»; `documentazione/prove/p8c-translate/`.

## Parità NL — P8c ricerca del progetto (23/11/2026)

- Indice e ricerca locale (`nl.search.index`/`nl.search.query`): clip, marcatori, sottotitoli,
  storia (scene comprese) e trascrizioni ASR; risultati con fonte, tempo, titolo ed estratto,
  mostrati nel pannello **NL Analyze** (campo + Search + Rebuild index).
- Prova live: 21 documenti indicizzati e tre domande con risultati corretti (respirazione → scena e
  trascrizione; Ada studio → scena e storia; Open → marcatore); `documentazione/prove/p8c-search/`.

## Parità NL — P8c longform: capitoli e marcatori (23/11/2026)

- Nuovo pannello **NL Analyze** e comandi `nl.longform.*`: dalla trascrizione ASR il modello locale
  propone i capitoli, che vengono agganciati ai confini reali dei blocchi (nessun tempo inventato),
  ordinati e limitati alla durata; un clic li trasforma in marcatori di timeline (`markers.add`).
- Export dei capitoli in JSON/Markdown/SRT (``nl.longform.export``, path assoluto) con selettore file nel pannello.\n- Prova live: 46 parole → 6 blocchi → 2 capitoli (qwen3.5:9b), 2 marcatori aggiunti, 26 s;
  `documentazione/prove/p8c-longform/`.

## Parità NL — P9b prompt AI, parametri e dry run (23/11/2026)

- **Prompt AI** (
l.prompt.enhance): brief → prompt + varianti validate (niente risoluzioni/aspect
  ratio, niente internals del workflow), eseguibile in background; pulsanti in NL Generate e NL
  Story con applicazione su clic. Il modello Ollama viene scaricato subito (`keep_alive=0`).
- **Parametri reali** (
l.workflows.parameters): widget di un workflow risolti da
  `/object_info` (valori, enum, min/max, input collegati, classi mancanti) e mostrati nel
  pannello NL Workflows.
- **Dry run** (`graphics.generateComfy dryRun:true`): costruisce il payload completo (reference
  caricate e loader patchati, size dalla sequenza) senza inviare nulla; riepilogo nel pannello.
- Prova live: `documentazione/prove/p9b-prompt-params/` (prompt 39 s con qwen3.5:9b; dry run del
  workflow v2v MiniMax con 23 nodi e reference image+video patched=1+1; 78 parametri, 0 classi
  mancanti; prompt H3 base in 34 s e full-reference in 22 s con qwen3.5:9b).

## Parità NL — P9d reference media (23/11/2026)

- `graphics.generateComfy` accetta reference **image/video/audio**: le carica in ComfyUI e
  patcha i loader per ruolo (`LoadImage`; `LoadVideo`/`VHS_LoadVideo`;
  `LoadAudio`/`LoadAudioUI`) in ordine di node-id; un workflow senza l'input richiesto viene
  rifiutato con un messaggio esplicito, senza inviare nulla.
- `nl.story.generate kind=video` parte dalla keyframe della scena (image-to-video) e con
  `extendFromScene` usa il video di un'altra scena come reference (v2v extend); il media
  importato viene collegato alla scena. Prova live: `documentazione/prove/p9d-reference/`.

## Parità NL — P8d prima verticale: storia AI → timeline (23/11/2026)

- Pannello **NL Story** e comandi `nl.story.*`: idea → bozza di storia con Ollama (JSON validato,
  scene con prompt immagine/movimento), affinamenti (editing campi scena, "refine" con istruzione),
  keyframe per scena con ComfyUI e **posizionamento in timeline** (`place`/`placeAll`, durata
  dalla scena). Il media generato viene collegato alla scena (`nl/story.json` accanto al progetto).
- Rispetto della regola GPU: il modello Ollama viene scaricato appena finita la bozza
  (`keep_alive=0`), quindi ComfyUI genera senza sovrapposizione (`ollama ps` vuoto verificato).
- Prova live: `documentazione/prove/p8d-story/` (storia di 10 scene, keyframe 512×512 importata,
  scena posizionata a 4,25 s; 107 s totali con qwen3.5:9b + z-image-turbo).

## Parità NL — P8b default rapido e P9a workflow doctor+API (23/11/2026)

- **Nessuna workspace built-in aggancia pannelli NL**: il layout di avvio resta *Editing* e per un
  montaggio essenziale c'è **Quick Edit** (Window ▸ Workspaces). Aprire un progetto non crea job,
  non contatta ComfyUI/Ollama e non legge `/object_info` (test `nl::performance_tests`). Garanzie e misure-obiettivo:
  `documentazione/PERFORMANCE.md`; ricevuta `prove/p8b-performance/`.
- **Workflow doctor unito alla verifica API** (come chiesto, unendo la “Verifica API” del Prompter
  al workflow doctor): `nl.workflows.verify` legge `/object_info` (sola lettura, 5991 classi su
  questa installazione), `nl.workflows.diagnose` controlla classi installate, file modello e
  reference contro gli enum del server oltre ai difetti statici (link, prompt, dimensioni, LTX AV
  split); `nl.workflows.fix` scrive una copia riparata, `nl.workflows.import` aggiunge workflow.
  Pannello **NL Workflows** con Verify/Doctor/Fix/Import.
- **Libreria iniziale**: 8 workflow copiati da `K:\AI\Prompter_Comfyui\workflows` (letta in sola
  lettura) in `workflows/prompter/` con `PROVENANCE.md`; si amplia importando altri JSON API.
- **Prova live**: 40 workflow esaminati, 4 con classi mancanti (tutti legacy calliope/regista),
  **0 errori sui workflow copiati dal Prompter**; dettagli in `prove/p9-workflows/`.

## Parità NL — P3a/P4b clipping, sgrossa e copertine (23/11/2026)

- **Clipping e Sgrossatura** girano sul bridge della suite (`clipsai_bridge.py`, motori vendored in
  `engines/clipsai`): proposte con score/dedupe, piano tagli con motivi e avviso
  parlato-dentro-i-tagli, render dei soli pezzi scelti, SRT normal/hormozi dai timestamp parola,
  report `sgrossa_report.json`. Pannelli **NL Clipping** e **NL Rough Cut**.
- **Copertine AI**: pannello **NL Cover** e comando `graphics.generateCover` — frame al playhead
  esportato a piena risoluzione, upload a ComfyUI (`/upload/image`), patch del nodo `LoadImage`,
  prompt derivato dal parlato vicino al frame (`nl.cover.prompt`), scelta workflow immagine,
  dimensione dal profilo NL, review e marcatura con `nl.cover.set` (`nl/cover.json`).
- Prova: `documentazione/prove/p3-clipping-sgrossa/` e `prove/p4b-cover/`. La generazione
  copertina live va provata con ComfyUI libero (nessun servizio avviato durante lo sviluppo).

## Parità NL — P1 impostazioni (23/11/2026)

- Nuova categoria Impostazioni **NL Video Suite** (`nl.*`) persistita in `preferences.json`:
  endpoint ComfyUI/Ollama, workflow per tipo, modelli installati, profili risoluzione dalla
  sequenza (immagini 1×/2×, video 2/3 o 1/2, arrotondamento 8/16 px), ASR (device GPU-first,
  lingua, modello, cache), timeout coda e percorsi servizi.
- I comandi leggono **parametro → impostazione → variabile d'ambiente → default**: nessun
  workflow video viene inventato, la sequenza resta la fonte di verità per le dimensioni.
- `nl_asr` usa lingua/device/modello/cache dalle impostazioni, con un solo ripiego CPU dichiarato
  e cartella cache stabile per media; `shorts` usa URL/modello Ollama dalle impostazioni.
- Comandi nuovi: `nl.models.workflows`, `nl.models.ollama`, `nl.services.status`,
  `nl.diagnostics`, `nl.size.preview`. Prova: `documentazione/prove/p1-settings/receipt.json`.

## Ottimizzazione spazio (22/11/2026)

- Rimossa la cache debug `filmcraft-nl/target/debug`: **8,69 GB** liberati (rimane il binario
  release e i sorgenti). Ricevuta: `da_eliminare/2026-11-22-ottimizzazione/ricevuta.json`.
- Log di build/test F1–F4 spostati in `da_eliminare/2026-11-22-ottimizzazione/logs/`.
- Spazio K: da 93,7 GB a **98,6 GB liberi**.
- ComfyUI era ancora in esecuzione con un job H3 (1 running, 0 pending) e commit di sistema
  quasi esaurito: build/test pesanti sospesi finché non libera GPU/RAM.

## Ottimizzazione spazio (23/11/2026)

- Rimossi i media generati dai test completati in `sviluppo/tests/artifacts` (517 file, ~3,9 GB):
  report JSON/HTML/log e immagini piccole restano come traccia.
- Rimossa la cache debug `sviluppo/target/debug` (~0,64 GB) e le anteprime rigenerabili
  `sviluppo/anteprima_creativa/data/previews` (~0,14 GB); puliti i `__pycache__`.
- Spostati in `da_eliminare/2026-11-23-ottimizzazione/` il FilmCraft 0.20.1 portable e il
  sorgente originale in `reference_software/` (sostituiti dal fork `filmcraft-nl`).
- Eliminato il trash del 21/11 (`test-storici`, `exe-storici`), già superato.
- Spazio K: da 100,06 GB a **102,90 GB liberi** (recuperati ~2,84 GB netti mentre la build era in
  corso). Ricevuta: `da_eliminare/2026-11-23-ottimizzazione/ricevuta.json`.
- Restano intatti: progetti utente, `runtime/`, `models/`, `engines/`, `ZMateriale/`, prove in
  `documentazione/prove/`.
- **Pulizia finale (23/11):** rimossa `filmcraft-nl/target/debug` (13,77 GB) ricreata da
  clippy/test, la cache di lavoro ASR e un `__pycache__`; spazio K da **81,43 a 97,47 GB
  liberi** (+16,05 GB). La release `filmcraft.exe` (77 MB, 02:05) contiene F5/F6/ASR, dialoghi
  GUI, risoluzione da sequenza e ASR GPU-first. Ricevuta aggiornata:
  `da_eliminare/2026-11-23-ottimizzazione/ricevuta.json`.

## Regole rispettate

- Nessun menu “NL” separato: comandi dentro i menu FilmCraft.
- Ogni funzione ha un comando headless, non solo un pulsante.
- Nessun dato inventato: i risultati dichiarano il metodo e i limiti.
- Media NL linkati in place, mai copiati/duplicati.
- Il progetto NL originale non viene sovrascritto: il mirror è un file separato.
