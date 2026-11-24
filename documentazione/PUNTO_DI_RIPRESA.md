# Punto di ripresa — CHIUSO 1.2.0 (24/11/2026)



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
