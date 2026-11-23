# Punto di ripresa — aggiornato al 23/11/2026 (dopo la ripresa pomeridiana)

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
