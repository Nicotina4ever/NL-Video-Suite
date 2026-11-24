# Punto di ripresa — NL Video Suite 2.0.0 (24/11/2026)

## P25 — Montaggio base, menu NL e agente timeline (24/11/2026)

- **Niente più muro di 29 tab**: il workspace **Editing (Montaggio)** ha nel gruppo in alto a
  destra `Proprietà / Grafica essenziale / Testo / NL Home / NL Agent`. «NL Video Suite» resta un
  alias dello stesso layout, non un workspace separato con tutte le tab.
- **Menu di primo livello «NL Video Suite»** con gruppi: Panoramica, Timeline e analisi,
  Generazione, Testo/voce/grafica, Consegna, Workflow, Agente timeline. I 30 pannelli restano
  anche nel menu Window per chi lo preferisce.
- **Tasto destro sulla timeline** (righello/gap) → **NL Video Suite** → Aggiungi oggetto da
  generare, Apri NL Agent, Compila prompt con AI, Genera e sostituisci, Raccogli asset nel
  progetto.
- **Agente timeline**: `nl.agent.sketch` da soggetto → piano, prompt e traccia **NL** di
  placeholder; `nl.agent.prompts` riempie i prompt con il modello locale; `nl.agent.generate`
  genera con ComfyUI, sposta il file in `<progetto>/generated/`, lo ricollega e sostituisce il
  clip placeholder nella posizione originale; `nl.agent.collect` raccoglie anche i media sotto
  `generated/comfy-*`.
- **Timeline-first**: `target_media` ora preferisce i clip video della sequenza aperta, poi
  l'audio; Clips AI non richiede più un file se il video è già in timeline. NL Speech può
  trascrivere senza percorso (usa la selezione/sequenza).
- **Prove live**: sketch esplicito 3 oggetti (clip 70/72/74, traccia NL); sketch agentico da
  soggetto con LLM (2 oggetti, `source: model`); prompt riempito dall'LLM; **generazione reale
  ComfyUI** di 1 oggetto (`z-image-turbo`), asset in `<progetto>/generated/`, clip 70 sostituito
  dall'item 71 con stessa posizione e durata 3 s. Evidenze in `prove/p25-redesign/`.
- **UI smoke**: `Editing` con tab NL, menu di primo livello NL, pannello NL Agent (9 controlli),
  menu contestuale timeline con 6 voci (`timeline.nlMenu.*`); screenshot in
  `prove/p25-redesign/`.
- **Test**: engine `nl::agent` 4/4, timeline-first ok; dock 11/11; menus default ok; NL panels 3/3;
  workspaces UI 10/10.
- **Limiti**: `nl.agent.generate` è sincrono (UI occupata durante ComfyUI); una nuova sketch non
  cancella i clip della precedente; «Raccogli asset» è esplicito, non automatico per ogni
  generazione fuori dall'agente.
- **Pulizia P25**: il `filmcraft-nl/target` ricreato da test e release (23.042,8 MB) è stato
  spostato in `da_eliminare/2026-11-24-ottimizzazione/filmcraft-nl_target_p25`; `da_eliminare`
  contiene ora 23.149,3 MB. Spazio K: **72,71 GB liberi**; svuotando `da_eliminare` si torna a
  circa **95,3 GB**. Con `target` spostato la prossima build parte da zero (~6 min release).

## Stato verificato 24/11/2026 — P19/P20/P21

Baseline riprodotta con l'exe precedente (`documentazione/prove/p19-nl-surface/baseline.json`):
i 28 pannelli NL **erano già instradati** (nessun «coming in a later milestone»), ma il workspace
«NL Video Suite» **non era nel menu** (`WORKSPACES` aveva 11 voci), NL Home non elencava NL Shot
Map, NL Audio restava vuoto, il pannello ASR prometteva comandi inesistenti e NL Social leggeva
`name` invece di `id`. La ricevuta P18 dichiarava il workspace built-in: non era vero.

- **P19 — workspace e routing**: `WORKSPACES` 12 voci con «NL Video Suite» (menu Window ▸
  Workspaces, pulsante Workspaces, `Alt+Shift+9`); `show_nl_layout()` passa da `set_workspace` e
  persiste il workspace; `NL_PANELS` (29) è la lista unica usata da layout, Home e test;
  `nl::body()` + `nl.context` su ogni pannello; `panel.placeholder` è osservabile dagli audit.
- **P20 — pannelli**: nuovo **NL Transcript**; ASR manager con scansione modelli, provider/device,
  import SRT/VTT e traduzioni reali; Social con profili e nomi controlli corretti; Clips AI con
  **focus automatico** e controlli stabilità/reattività/comportamento; Longform con rough cut;
  Shot Map con stato sequenza; Studio con riepilogo finitura; Services leggibile; tabella
  `PANEL_COMMANDS` e test che ogni comando esista nel registro.
- **P21 — reframe**: port della camera path nostra (`camera-cpu-v3`) in
  `engine/src/nl/reframe_camera.py`; il motore applica i punti come keyframe di
  `motion.position`. Prova live GTA6 0–6 s: `composed` 2 punti/2 keyframe, `legacy` 63/63, offset
  −797,33 px, x da −527 a −257 px; corretto il calcolo dell'offset (era scalato sul margine
  libero invece che sulla picture dipinta). Valutazione dei tre repo in `REFRAME_REPOS.md`:
  nessun codice copiato; due MIT senza implementazione utilizzabile, `Cliper-AI` senza licenza.
- **Test completi**: engine 1055 + 17 verdi (3 ignored esistenti); UI 374 lib + 4 + 1 + 818
  integration + 6 + 3 verdi (8 ignored esistenti); CLI 20 + 23 verdi; clippy engine/UI/CLI
  `-D warnings` pulito; fmt applicato.
- **P22 — audit dimostrabile**: `documentazione/prove/p19-nl-audit/audit.py` sull'exe 2.0.0 →
  29 pannelli, workspace nel menu e attivo, layout completo, 0 senza contesto, 0 placeholder,
  0 comandi falliti (4 rifiuti attesi: pipeline/cleanup/montage/bridge senza risultato), 28
  pulsanti Home, 29 screenshot; `audit.json` + `receipt.json`.
- **P23 — release**: `NL Video Suite.exe` 85.547.008 byte (SHA-256
  `F37354B14C4576BFB1292448BF110E322150CF50A07052D2BA0C72E3FB2B7DA5`), `NL Video Suite CLI.exe`
  54.339.072 byte; bundle `distribuzione/NLVideoSuite-2.0.0-windows-x64` + zip 235.060.798 byte;
  audit del bundle 29/29 verde; reframe del bundle senza `runtime/` → fallback statico dichiarato
  (`fallback: camera tracker unavailable`, 0 keyframe).
- **P23 — consegna e pulizia**: commit `b4bf1cc` (P19–P22) e `53e4339` (P23) su `main`, push su
  https://github.com/Nicotina4ever/NL-Video-Suite.git; GitHub head `53e4339`, 0 workflow/action,
  commit unsigned normale. Pulizia: rimossi bundle 1.4.1/1.5.0 (1.584,7 MB) e
  `filmcraft-nl/target/debug` (18.766,3 MB); spazio K: da 77,53 a **93,95 GB** liberi
  (+16,42 GB). Restano radice exe/CLI, bundle 2.0.0, workflows, runtime, models, prove e progetti.
  Ricevute: `documentazione/prove/p23-release/receipt.json` e `cleanup.json`.

## P24 — ottimizzazione cartella e da_eliminare (24/11/2026)

- Spostati in `da_eliminare/2026-11-24-ottimizzazione/` **2.502,4 MB** di materiale vecchio o
  rigenerabile: `filmcraft-nl/target` (2.395,9 MB), `sviluppo/web-app/node_modules` (56,9 MB),
  `app/stabile-backup` (36,5 MB), `sviluppo/tests/node_modules` (11,6 MB), 29 screenshot duplicati
  dell'audit (1,4 MB), `sviluppo/web-dist` (0,15 MB) e due cartelle di fixture dei vecchi test.
- **Non** sono stati toccati: `runtime/`, `models/`, `data/projects/`, `ZMateriale/`,
  `app/tools/`, `sviluppo/bridge/`, `engines/clipsai/`, `distribuzione/2.0.0`, exe/CLI in radice,
  `documentazione/prove/` e `reference_software/`. `short01.mp4` e `ace-step.mp3` sono stati
  ripristinati perché referenziati dalle ricevute F4/F6.
- Spazio K: **93,95 GB liberi**; svuotando `da_eliminare` si liberano circa **96,4 GB**. Lo
  spostamento è sullo stesso disco, quindi lo spazio si libera solo all'eliminazione.
- Dopo la pulizia l'audit è stato rieseguito: **29 pannelli, workspace attivo, 0 placeholder,
  0 senza contesto, 29 screenshot** (`documentazione/prove/p19-nl-audit/audit.json`). Smoke CLI su
  copia GTA6 (`sequence.inspect`, `nl.story.get`) ok.
- Con `filmcraft-nl/target` spostato, una nuova build parte da zero (release ~6 min); gli eseguibili
  in radice e il bundle 2.0.0 non dipendono da `target`.
- Ricevuta: `documentazione/prove/p24-optimization/receipt.json`.


## P18 — correzione (24/11/2026)

La voce P18 sotto è **superata e incompleta**: il routing era stato corretto, ma il workspace non
era registrato in `WORKSPACES`, `show_nl_layout` non persisteva e la ricevuta dichiarava un menu
che non esisteva. P19 chiude quei punti con test e prove; la ricevuta `p18-menu-routing` resta
come storia, non come stato attuale.




## P18 — NL Video Suite nel menu Workspace e routing definitivo (24/11/2026)

- «NL Video Suite» ora è un **workspace built-in** selezionabile dal menu Workspace, con tutti i
  28 pannelli NL e i pannelli standard.
- `PanelKind::is_nl()` + return anticipato in `panels.rs`: Bridge, Titles, Home e ogni altro pannello
  NL non possono più finire in «coming in a later milestone».
- Test dedicato: ogni pannello Nl è routed e presente nel layout NL; workspaces_ui aggiornato.

## P17 — Verifica pannelli e reframe automatico (24/11/2026)

- Nuovi pannelli **NL Shot Map** (scene detection sulla sequenza attiva), **NL Social** (controlli
  piattaforma), **NL Studio** (hub finitura), **NL Services/Project** (servizi, modelli, storage,
  progetto); tutti collegati a comandi verificati con progetto/sequenza aperti.
- Corretto il pannello **NL Search** (leggeva `hits` invece di `results`): indice 133 documenti e
  query funzionanti.
- **Reframe automatico**: 7 frame + rilevatore volti OpenCV del runtime suite, media dei centri,
  fallback centro; prova GTA6 0–6 s: faceCentre [0.530,0.429], offset −70 px, sequenza 1080×1920.
- Valutati i repo indicati: due MIT (YOLO/MediaPipe + EMA), uno senza licenza non riutilizzabile;
  nessun codice copiato, implementazione propria con OpenCV.

## P15/P16 — Clips AI sequenze reframe, Home e layout NL (24/11/2026)

- NL Clips AI: scelta video dal disco o selezione, target 9:16/16:9, focus, sottotitoli automatici;
  «Crea sequenze» produce nuove sequenze reframed con caption track stilizzata (prova live 1080×1920,
  2 cue).
- NL Home: dashboard con tutte le funzioni NL; layout NL completo applicabile da NL Settings.
- Instradati al renderer NL i pannelli che mostravano il placeholder; aggiunti NL Longform, NL Search,
  NL Speech; nomi accessibili sui controlli.
- Evidenze P15 in `prove/p15-reframe/`; test engine 1052+17, UI 368+4+1+815, clippy pulito.

## P14 — Parametri workflow e preset (24/11/2026)

- Editor in NL Workflows: tendine per LoRA/UNET/VAE/CLIP/sampler, numeri/testi, filtro e sezioni
  «Avanzate»; «Usa per la prossima generazione» porta gli override in NL Generate.
- Preset nominati (`nl.workflows.presets`) salvati in `%APPDATA%\NL Video Suite\nl\workflow-presets`,
  con riuso per classe+campo su altri workflow; dry run verificato (overridden=1).
- Evidenze: `documentazione/prove/p14-workflow-params/`.

## CONSEGNA GITHUB — 24/11/2026

- Push riuscito su https://github.com/Nicotina4ever/NL-Video-Suite.git (main, commit ef4ead6):
  95 commit (docs + import del sorgente filmcraft-nl, 1331 file).
- Verifiche GitHub: 0 workflow/action (nessun workflow alla radice), 0 check configurati, commit
  «unsigned» normale; nessun errore di permessi o push protection; repo PUBBLICO.
- Pulizia finale: eliminati 1.569 MB di vecchie build in da_eliminare, 2,9 MB di temp e 2,6 MB di cache
  analisi; disco ~95,8 GB. Restano portabile 1.2.0, exe in radice, prove, modelli e progetti.

## CHIUSURA 1.1.0
- **Vision del frame entrante** implementata e provata (gemma4:e4b, 26 s) e **bridge end-to-end riuscito**
  in 1161 s: MiniMax_H3_00070_.mp4 importato e piazzato con 2 transizioni (0–48 / 48–96 / 96–144).
- **Tutti i test da zero** dopo aver cancellato il target: engine 1046+17, UI 368+4+1+815, clippy pulito.
- **Portabile 1.1.0** in `distribuzione/` (zip 223,7 MB, con CLI) + **launcher** `AVVIA-NL-Video-Suite-NUOVA.cmd`
  e `INSTALLA-NL-Video-Suite-NUOVA.cmd`; guida `GUIDA_AVVIO_NL_VIDEO_SUITE.md`.
- **Linux/macOS**: `test-portable.sh` + `INSTALL_LINUX_MACOS.md` pronti per le macchine dell’operatore.
- Pulizia: target −17,1 GB, build 1.0.0 in `da_eliminare`, temp rimossi; disco libero ~95,8 GB.
- Release 1.0.0 (48 pannelli), portabile definitivo in `distribuzione/`, test engine/UI verdi, clippy
  pulito; GTA6 read-only con export completo; performance misurate; pulizia `target/` (−16,97 GB) e
  temp (−1,47 GB), disco libero ~95,9 GB.
- Limiti dichiarati: bridge FLF reale oltre i 30 min su questa GPU (recupero `nl.bridge.place`), vision
  del frame entrante non implementata, ASR HTTP solo dry run, script Linux/macOS solo sintassi, Inno
  Setup non compilato, rename upstream rimandato.
- Dettaglio in `documentazione/prove/p7-final/receipt.json` e `STATO_INTEGRAZIONE.md`.

# Punto di ripresa storico — aggiornato al 23/11/2026 (sera)

Stato aggiornato dopo P10 (provider modulari e distribuzione) e P11 (text-to-video e bridge locale).
## Aggiornamento 23/11 sera — P10 provider/distribuzione e P11 generativo locale

- **P10a provider modulari**: `nl.llmProvider` (`ollama` | `openai_compatible`), `nl.llmUrl`,
  `nl.llmModel`, `nl.llmApiKey` (+ `NL_LLM_*`); client OpenAI-compatibile con `response_format` e
  retry; `nl.models.llm`; provider in `nl.services.status` e in NL Settings. Moduli prompt/storia/
  longform/assistente/traduzioni provider-aware, anche nei job in background. Prova live con
  l'endpoint `/v1` di Ollama: 13 modelli, prompt generato. `prove/p10-providers/`.
- **P10b distribuzione**: `packaging/nl/portable.ps1` e `portable.sh`, launcher con `NL_SUITE_ROOT`,
  workflows e ffmpeg copiati, zip/tar.gz; `documentazione/DISTRIBUZIONE.md` con piattaforme,
  prerequisiti, installer upstream e configurazione servizi AI. Prova Windows: bundle con 51
  workflow e zip 202 MB. `prove/p10-dist/`.
- **P11 generativo locale**: `nl.generate.text` (workflow `video_minimax_h3_t2v.json`, dimensioni
  dalla sequenza, dry run, placement) e `nl.bridge.plan/generate` (gap tra due clip, ultimo frame
  uscente + primo frame entrante, workflow first/last-frame derivato con due LoadImage, generazione e
  inserimento nel gap). Pannello **NL Bridge** e pulsante Text → Video. Estensioni comfy: prompt
  widget H3, durata PrimitiveFloat, size H3, `place` automatico e `comfy.last`.
- **Prove reali**: t2v 669 s → MP4 1280×720, 24 fps, 2.333 s importato; bridge FLF 794 s → file
  importato e gap riempito 0–48/48–96/96–144 dopo il trim; auto-placement verificato con una
  generazione z-image; timeout predefinito dei comandi video portato a 1800 s.
  `prove/p11-generative/`.
- **P12 manutenzione spazio**: pulizia automatica all'avvio (max 1/giorno) di frame bridge, log e
  cache vecchie in `%APPDATA%\NL Video Suite`; media generati protetti salvo `cleanupGenerated` con
  retention; impostazioni e pulsanti Preview/Clean up now. Prova live: frame vecchio rimosso, nuovo
  e media intatti. `prove/p12-maintenance/`.
- **P7 release e performance**: release build con 48 pannelli in 422 s (`filmcraft.exe` 80,4 MB,
  `filmcraft-cli.exe` 51,3 MB); avvio CLI warm 71–80 ms, demo open+inspect+storage 108 ms, GTA6 open
  96 ms, frame 1080×1920 1176 ms. Regressione GTA6 read-only su copia (hash originale invariato).
  `prove/p7-release/`, `prove/p7-regression/`.
- **Pulizia spazio in corso**: rimossa la cache `target/debug/incremental` (11,9 GB) che causava anche
  errori di link con artefatti stale; `da_eliminare/` svuotato (−163,9 MB, ricevute conservate in
  `documentazione/prove/da-eliminare-2026-11-23/`), nessun log/tmp residuo trovato. Test engine 1042
  verdi dopo la ricostruzione. Alla chiusura del progetto resta da svuotare `target/` (deps
  debug/release dopo aver copiato gli eseguibili); prove, ricevute e media non si toccano.
- **P7 export e portabile**: export GTA6 0–2 s h264 (1080×1920, 60 frame, 0,41 MB, 28,2 fps) in 2,3 s
  con apertura; portabile 0.2.0 rigenerato con la release nuova (zip 203,4 MB, eseguibile 84,3 MB).
- **P11d estensione v2v**: `nl.generate.extend` (estratto h264 della coda + due frame, workflow
  extend R2V 2refs+video, placement dopo la clip) e pulsante nel pannello Generate; dry run live
  32 nodi con 2 reference immagini + 1 video. Resta la generazione reale su GPU più veloce.
- **P10c installer per-utente**: `install.ps1` Windows testato (install+uninstall), Inno Setup
  `installer.iss`, `install.sh` Linux e `install-macos.sh` (sintassi verificata); nessun servizio AI.
- **P7 export completo GTA6**: 4476 frame, 149,2 s, 1080×1920, 40,7 MB, 43,7 fps in 103,3 s (copia
  read-only, nessun media offline).
- **P10d trascrizione modulare**: ``nl.asrProvider`` bridge/OpenAI-compatibile, ``nl.asr.transcribe``
  multipart ``/audio/transcriptions`` con dry run e validazione, impostazioni e docs; il bridge resta
  per applicare la trascrizione al media. Prova live dry run in ``prove/p10-asr-provider/``.
- **P11e transizioni ponte**: Cut/Dissolve su plan/generate/place, cross dissolve video + constant power
  audio sui bordi, anche nel placement automatico; pannello NL Bridge aggiornato; prova live
  2 transizioni su file generato.
- **Da fare subito dopo**: rerun end-to-end automatico del bridge con timeout 1800 s (la generazione
  e il placement sono verificati separatamente), prompt bridge con analisi vision del frame entrante,
  transizioni/audio del ponte, estensione v2v; rename NL degli installer upstream e prova reale degli
  script Linux/macOS; P7 release/performance/regressione; portable definitivo.

Stato aggiornato dopo il lavoro successivo alla pausa. Tutto committato, nessun job in esecuzione,
nessun servizio avviato dalla suite. Regola rispettata: Ollama e ComfyUI mai in esecuzione insieme
per le nostre prove.

## Cosa è stato completato (in più rispetto al punto di ripresa precedente)

- **P9c coda varianti** (`nl.queue.add/list/prepare/send/remove/free`): seed deterministici, dedupe,
  prepare con dry run (`submitted=false`), invio solo con `confirm:true`, free VRAM confermato;
  pannello **NL Queue** con doppia conferma. Prova live: 3 varianti preparate, una inviata davvero
  (z-image 512×512, 182 s). `prove/p9c-queue/`.
- **Override dei widget**: `graphics.generateComfy overrides [{node,field,value}]` validati dal
  motore (nodi/campi inesistenti rifiutati) + campo «Widget overrides JSON» nei pannelli Generate e
  Queue. Usati nella prova video H3 (megapixels, durata).
- **P9d video live**: MiniMax H3 reference-to-video con due reference immagini → **video 672×384,
  2,33 s, con audio AAC stereo**, importato (488 s). `prove/p9d-video/`.
- **P8d video di scena live**: `nl.story.generate kind=video` con keyframe+sheet come reference,
  `useSheet`, overrides; video importato, collegato alla scena e **posizionato in timeline**
  (clip video+audio) in 425 s. `prove/p8d-story/`.
- **P9b prompt H3** (`nl.prompt.h3`): schema base/keyframe/full-reference, validazione, retry e
  riparazione tag dialogo; prove base 34 s e full-reference 22 s. Anche **Prompt AI** validato,
  **parametri reali** da `/object_info` e **dry run** senza invio (23 nodi v2v, 78 parametri).
- **P8c**: capitoli longform + marcatori (`nl.longform.analyze/get/markers`), **export capitoli**
  JSON/MD/SRT (`nl.longform.export` + selettore file), **ricerca progetto** (`nl.search.*`),
  **assistente con citazioni** (`nl.assistant.ask`), **traduzioni** (`nl.translate.captions/text`,
  nuova traccia EN/ES). Prove: `prove/p8c-longform/`, `p8c-search/`, `p8c-assistant/`,
  `p8c-translate/`.
- **P5 musica live**: Ace Step, MP3 10 s 48 kHz stereo, collegata alla scena e messa su A1
  (`nl.story.extendAudio`/`placeAudio`). `prove/p5-music/`.
- **P8d libreria reference**: character sheet + location plate generati una volta e usati dalle
  keyframe di scena (dry run verificati). `prove/p8d-story/library-*.jsonl`.
- **P8e completa**: pannelli **NL Subtitles** (cue list, split/merge/delete, import/export SRT/VTT),
  **NL Titles** (template MoGRT con controlli e responsive intro/outro), **NL Graphics**
  (`nl.graphics.add`), **NL Audio** (Essential Sound + mixer strip). `prove/p8e-studio/`.
- **P8f principale**: **NL Delivery** (`nl.delivery.qa/package/social`, `nl.storage.status`, pacchetto
  con manifest/captions/chapters/story/thumbnail, QA social per piattaforma) e **NL Album**
  (`nl.album.list/insert`, inserimento in timeline). `prove/p8f-delivery/`, `prove/p8f-album/`.

## Commit fork principali

`809931a0` album + social; `99a922de` delivery + grafici/audio; `d9e74c91` sottotitoli/titoli;\n`8a10f8fa` override UI + musica; `9374c3b9` video di scena + export capitoli; `cb5d81a6` assistente;
`8c3711d2` coda varianti + traduzioni; `208dc2d8` longform + ricerca; `7cd91d24` libreria reference;
`7aff8f47` H3; `a4abc203` prompt AI/parametri/dry run; `826a1d5d` reference media/video scena/musica;
`f602e3a3` storia→timeline; `660f4ad9` P8b/P9a; `4927a196` P4b; `c46162f5` P3a; `d9102850` P1.

## Verifiche

- clippy `-D warnings` pulito (engine, ui, cli); **engine 1020+17 test**, **UI 368 + 4 + 1 +
  integrazione** verdi. Due test real-time (`marked_playback_position`, `slip_monitor_ui`) sono
  flaky sotto carico GPU/CPU intenso e passano isolati.
- Nessun progetto utente toccato; nessun download; testi visibili senza "FilmCraft".

## Cosa manca (ordine proposto)

1. **P8f resto**: versioni montaggio (snapshot/varianti) ed export social diretto; il resto di P8f
   (album, delivery, social check, thumbnail) è fatto.
2. **P8e**: fatto (sottotitoli, titoli, grafici, audio).
3. **P8d resto**: montage planner/creative montage/audio onset, shot map, pipeline/flows.
4. **P9d resto**: estensione video con v2v su segmento esistente e audio (Ace Step via coda);
   i2v da keyframe con workflow dedicato (non presente: servirebbe un i2v a 1 reference).
5. **P7**: release completa con tutti i pannelli, misure performance, regressione GTA6 (sola
   lettura), ricevute, pulizia finale cache debug e spazio.
6. **Portable** con motore AI e launcher unico.

## Come riprendere

Riprendere dal punto 1 dell'elenco qui sopra. Riferimenti: `TRACCIA_PARITA.md`, `TODO.md`,
`HANDOFF.md`, questo file. Regole invariate: lettura sola delle sorgenti esterne, progetti utente
intatti, mai Ollama e ComfyUI insieme, nessun download obbligatorio, test mai indeboliti.
