# Handoff — NL Video Suite / FilmCraft

Aggiornato: 23/11/2026. Questo file serve a riprendere il lavoro in un'altra conversazione o
harness senza perdere stato. Il codice e le prove restano su disco; questo è il riassunto.

## Obiettivo

Una sola app (fork FilmCraft 0.20.1, `filmcraft-nl`) con l'esperienza NLE di FilmCraft e **tutte**
le funzioni AI di NL Video Suite come comandi/pannelli nativi. Niente menu "NL" separato, ma
menù e impostazioni NL dentro l'app. UI viola / magenta / ciano, icona e wordmark NL.

**Scope completo:** unione di Regista AI, Clips AI, Calliope, ZP Video e delle funzioni già in NL
Video Suite. Ogni funzione deve essere **raggiungibile dalla GUI** (menu, dialoghi, pannelli,
impostazioni), non solo da comando headless. I modelli già installati devono essere selezionabili;
nessun download obbligatorio.

Percorsi di riferimento (sola lettura, niente copia di codice/wording):

| Percorso | Cosa contiene |
|---|---|
| `K:\AI\ZioPaolo Video` | ZP Shorts Studio: pipeline short/compilation, sessioni, CLI |
| `K:\AI\Regista_AI` | agente Regista: storyboard, image/video/TTS/audio, workflow repair, coda |
| `K:\AI\ClipsAI` | Sgrossatura, Clipping social, WhisperX, SRT, reframe |
| `K:\AI\Calliope` | story→scene→shot→film, settings/coda, progetto SQLite (MIT) |

Matrice dettagliata: `documentazione/PARITA_FEATURE.md`.

**Requisiti aggiunti il 23/11/2026 (vincolanti):**

- **Risoluzione dalla sequenza**: immagini a 1× o 2× la sequenza; video a 2/3 (riduzione di
  1/3) o metà (1080p → 720p/540p; 2048 → 1024). Regole configurabili e forzabili nelle
  Impostazioni NL, con override nel dialogo; arrotondamento a 8/16 px.
- **GPU-first**: ASR/WhisperX prova CUDA e ripiega su CPU una sola volta, dichiarandolo;
  ComfyUI gestisce la propria GPU; nessun download implicito.
- **Branding**: niente “FilmCraft” nell’interfaccia visibile; resta solo in licenza/attribuzione.
  Ogni funzione NL ha un pannello dedicato (Genera, Musica, ASR, Clipping, Sgrossa,
  Storyboard/Storia, Coda, Impostazioni NL), non solo una voce di menu.

## Stato verificato

| Fase | Stato | Prova |
|---|---|---|
| F0 fork/build | fatto | build release, test engine/UI |
| F1 branding NL | fatto | commit `0845ee6a` |
| F2 import progetto NL | fatto | GTA6, 1 media, 9 clip, 377 parole |
| F3a testo + mirror | fatto | commit `0133016c` |
| F4a Find Shorts locale | fatto | commit `ca54d717`, 2 short |
| F4b Find Shorts AI | fatto e verificato | `documentazione/prove/f4b-ollama/` (LFM, 2 short, 3,2 s) |
| F5 ComfyUI immagini | **verificato** | `documentazione/prove/f5-comfy/` (1024×1024) |
| F6 musica Ace Step/MiniMax | codice fatto, test da ripetere | `documentazione/F6_AUDIO.md` |
| F3b ASR | bridge verificato, comando pronto | `documentazione/prove/f3b-asr/`, `nl_asr.rs` |
| F3b traduzioni | da fare | — |
| GUI NL | dialoghi Generate/Music/ASR aggiunti; clippy pulito | `menu_dialogs.rs`, `text.rs` |
| P1 Impostazioni NL | **fatto** | categoria `nl` in `preferences.json`, wiring comfy/ASR/shorts, `nl/size.rs`; `prove/p1-settings/` |
| P2b Pannelli NL + branding | **fatto** | 10 pannelli nel dock, testi visibili senza “FilmCraft” |
| P3a Clipping + Sgrossa | **fatto** | bridge ClipsAI, comandi `nl.clipping.*`/`nl.sgrossa.*`/`nl.subtitles.srt`; `prove/p3-clipping-sgrossa/` |
| P4b Copertine AI dal frame | **fatto** | `graphics.generateCover`, `nl.cover.*`, pannello NL Cover; `prove/p4b-cover/` |
| P8b Default rapido | **fatto** | workspace **Quick Edit** disponibile, nessun pannello NL docked di default, nessun AI all'apertura, `PERFORMANCE.md`; `prove/p8b-performance/` |
| P9a Workflow doctor + API | **fatto** | `nl.workflows.verify/diagnose/fix/import`, pannello NL Workflows, 8 workflow Prompter copiati; `prove/p9-workflows/` |
| P8d Storia AI → timeline | **verticale completa, con video** | pannello NL Story, `nl.story.*`, sheet/plate + keyframe con reference, **video di scena H3 con audio e place in timeline**; `prove/p8d-story/` |
| P9d Reference media | **fatta, con video live** | image/video/audio refs con patch loader, `extendFromScene`, **H3 full-reference reale 672×384 2,33 s con audio** (overrides, 488 s); prove `p9d-reference/`, `p9d-video/` |
| P9b Prompt AI/parametri/dry run | **prima parte fatta** | `nl.prompt.enhance`, `nl.prompt.h3` (schema MiniMax H3), `nl.workflows.parameters`, `dryRun` + pulsanti pannelli; `prove/p9b-prompt-params/` |
| P8c Longform/analisi | **prima parte fatta** | pannello NL Analyze, `nl.longform.*` (capitoli da ASR + marcatori); `prove/p8c-longform/` |
| P8c Ricerca progetto | **fatta** | `nl.search.index/query` (clip, marcatori, sottotitoli, storia, trascrizioni) + riquadro in NL Analyze; `prove/p8c-search/` |
| P8c Traduzioni | **fatte** | `nl.translate.captions/text` (nuova traccia EN/ES) + pulsante in NL Analyze; `prove/p8c-translate/` |
| P8c Assistente | **fatto** | `nl.assistant.ask` con citazioni dai documenti indicizzati; `prove/p8c-assistant/` |
| P8e Sottotitoli/Titoli/Grafici/Audio | **fatta** | NL Subtitles, NL Titles, NL Graphics (`nl.graphics.add`), NL Audio (Essential Sound + mixer); `prove/p8e-studio/` |
| P11f Vision+Bridge E2E | **fatta** | vision gemma4:e4b 26 s, bridge completo 1161 s con 2 transizioni; `prove/p11-vision/` |
| P11e Transizioni ponte | **fatta** | Cut/Dissolve su plan/generate/place, pannello NL Bridge, prova 2 transizioni; `prove/p11-bridge-transitions/` |
| P10d Trascrizione provider | **fatta** | `nl.asrProvider` bridge/OpenAI-compatibile, `nl.asr.transcribe` multipart, dry run; `prove/p10-asr-provider/` |
| P10c Installer per-utente | **fatta** | install.ps1 Windows testato, Inno/Linux/macOS forniti, docs; `prove/p10-installers/` |
| P7 Export completo GTA6 | **fatto** | 4476 frame, 149,2 s, 40,7 MB, 43,7 fps in 103 s; `prove/p7-regression/` |
| P11d Estensione v2v | **fatta** | `nl.generate.extend` (estratto coda + 2 frame, workflow extend), pulsante Generate, dry run 32 nodi; `prove/p11-v2v-extend/` |\n| P7 Release/Perf/Pulizia | **CHIUSA** | release 1.0.0, portabile in distribuzione, test completi, pulizia target/temp; `prove/p7-final/` | release 422 s, 48 pannelli, avvio 71–80 ms, GTA6 open 96 ms, frame 1176 ms; `prove/p7-release/`. Export GTA6 e portabile 0.2.0 fatti; restano riproduzione completa, ricevuta finale e pulizia finale di target |\n| P12 Manutenzione spazio | **fatta** | `nl.storage.cleanup` + impostazioni + pannello; `prove/p12-maintenance/` |\n| P11 Generativo locale | **fatta la parte principale** | `nl.generate.text` (t2v), `nl.bridge.plan/generate` (FLF first+last frame), pannello NL Bridge, placement nel gap; `prove/p11-generative/` |\n| P10 Provider/Distribuzione | **fatta la parte principale** | provider Ollama/OpenAI-compatibile dalle impostazioni; portabili Windows/Linux/macOS + DISTRIBUZIONE.md; `prove/p10-providers/`, `prove/p10-dist/` |\n| P8d Montage/Shot map | **fatta la parte principale** | `nl.montage.plan/apply` (+bpm/rhythm), `nl.story.shotmap`, pannelli NL Montage/NL Story; prove `p8d-montage/`, `p8d-shotmap/`. Restano flows review-gated e beat detection reale |\n| P8f Delivery/Album/Social/Versions | **fatta** | `nl.delivery.qa/package/social`, `nl.storage.status`, pannello NL Delivery; `nl.album.*`, pannello NL Album; `nl.versions.*`, pannello NL Versions; prove `p8f-*` |
| P9c Coda varianti | **fatta (manca free auto)** | `nl.queue.add/list/prepare/send/remove/free`, override widget (campo JSON in Generate/Queue), pannello NL Queue; `prove/p9c-queue/`; musica live `prove/p5-music/` |
| Parità NL completa | in corso (P2, P3b, P4, P5, P6, P8c–f, P9b–d) | `documentazione/PARITA_FEATURE.md`, `TRACCIA_PARITA.md`, `inventario/versione_test.md` |
| F8 portable | da fare | — |

Ultimi test (23/11/2026): clippy engine+UI pulito; engine 966 + 17 passati; UI 367 + 4 + 1 + 828
passati, 0 falliti. Commit fork: `7cd91d24` (P8d libreria reference), `7aff8f47` (P9b H3 + prompt AI + parametri + dry run),\n`826a1d5d` (P9d reference media + video scena + musica), `f602e3a3` (P8d storia AI → timeline), `660f4ad9` (P8b/P9a quick edit + doctor/API), `4927a196` (P4b copertine),
`c46162f5` (P3a clipping/sgrossa), `d9102850` (P1 impostazioni + pannelli). Base `48f2ab9e`.
Release finale pronta: `filmcraft-nl/target/release/filmcraft.exe` (77 MB, 02:05) con F5/F6/ASR,
dialoghi GUI, risoluzione da sequenza e ASR GPU-first (da ricompilare a fine P1–P7: la release non
contiene ancora i pannelli NL).
F6: workflow Ace Step completato (MP3 120 s, `prove/f6-audio/ace-step.mp3`); resta il retest
dell'import nell'app dopo il fix `seconds` e la regola 2/3.
Ottimizzazione spazio passo 2 (23/11): progetto da 32,84 a **17,67 GB**, K: **105,3 GB liberi**;
ricevuta `da_eliminare/2026-11-23-ottimizzazione-p2/` (la cache debug ricreata dai test va
rimossa di nuovo a fine P7).

## File chiave

- `filmcraft-nl/crates/engine/src/comfy.rs` — client ComfyUI, comandi
  `graphics.generateComfy` e `audio.generateMusic`.
- `filmcraft-nl/crates/engine/src/shorts.rs` — F4a/F4b, fix `think:false`/`num_ctx`.
- `filmcraft-nl/crates/engine/src/ollama.rs` — client Ollama.
- `filmcraft-nl/crates/engine/src/nl_project.rs` — import e mirror NL.
- `filmcraft-nl/crates/engine/src/transcript.rs` — testi e parole.
- `documentazione/TODO.md` — todos operativi.
- `documentazione/STATO_INTEGRAZIONE.md` — stato e limiti.
- `documentazione/F4_SHORTS.md`, `F5_COMFY.md`, `F6_AUDIO.md`, `F3B_ASR.md`.
- `da_eliminare/2026-11-23-ottimizzazione/ricevuta.json` — pulizia spazio.

## Comandi utili

```powershell
# build release
cd K:\AI\NL_Video_Suite\filmcraft-nl
cargo build --release -p filmcraft

# controlli
cargo clippy -p filmcraft-engine --all-targets -- -D warnings
cargo test -p filmcraft-engine

# app con canale di controllo
$env:FILMCRAFT_CONTROL_TOKEN=('f4' + ('0' * 62))
.\target\release\filmcraft.exe --control 9876

# test F5/F6 (app già avviata; ComfyUI libero, Ollama scaricato)
python K:\AI\NL_Video_Suite\documentazione\prove\f5-comfy\real_test.py
python K:\AI\NL_Video_Suite\documentazione\prove\f6-audio\real_test.py
```

## Regole operative

- Mai Ollama e ComfyUI insieme: usare un modello piccolo Ollama (`lfm2.5:latest`), poi
  `ollama stop` prima di un job ComfyUI.
- ComfyUI: coda gestita da ComfyUI; non uccidere job e non liberare modelli di altre app.
- Non toccare i 3 progetti utente (GTA6, Pubblicità, Trailer).
- Media NL linkati in place; il progetto NL non viene mai sovrascritto.
- Spostare in `da_eliminare` con ricevuta; niente cache debug nel repo.
- Non dichiarare fatto ciò che non è stato testato.

## Prossimi passi

1. **P2b**: pannelli NL dedicati (Genera, Musica, ASR, Clipping, Sgrossa, Storyboard, Coda,
   Impostazioni NL) e rimozione di “FilmCraft” dai testi visibili.
2. **P2**: pannello ASR completo (import SRT/VTT, traduzioni EN/ES) con device/fallback visibile.
3. **P3**: Clipping e Sgrossa via bridge esistente (`sviluppo/bridge/clipsai_bridge.py`), con
   pannelli, piano tagli, report e anteprima.
4. **P4**: Storyboard/Storia con generazione immagini/video per scena e coda.
5. **P5**: musica con workflow/modelli installati selezionabili (TTS escluso su richiesta).
6. **P6/P7**: riparazione workflow, servizi, parità verificata, portable, regressione GTA6, docs.
7. Restano validi: retest F6 in app pulita, prova `transcript.transcribeNl` su demo/GTA6, e alla
   fine pulizia cache debug con ricevuta.

## Se si migra a un altro harness

Non è necessario per il codice: tutto è su disco e documentato. Se serve, usare come prompt:
"Leggi `K:\AI\NL_Video_Suite\documentazione\HANDOFF.md`, `PARITA_FEATURE.md` e `TODO.md`.
Riprendi dal primo passo non fatto. Obiettivo: tutte le funzioni di Regista AI, ClipsAI, Calliope
e ZioPaolo Video dentro NL Video Suite, con pannelli dedicati e impostazioni. Risoluzioni derivate
dalla sequenza (immagini 1×/2×, video 2/3 o 1/2), GPU-first con fallback CPU, niente ‘FilmCraft’
nei testi visibili. Usa i percorsi indicati in sola lettura, non copiare codice o wording, non
toccare i progetti utente, non sovrapporre Ollama e ComfyUI, testa e documenta ogni fase."
