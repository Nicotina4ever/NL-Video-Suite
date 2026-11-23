# Parità funzionale NL Video Suite

Data: 23/11/2026. Obiettivo: **tutte** le funzioni delle app NL originali devono esistere in
`filmcraft-nl` e essere raggiungibili dalla GUI (menu, dialoghi, pannelli, impostazioni), non solo
da comandi headless. Nessun download obbligatorio: i modelli già presenti sono selezionabili.

Inventari verificati per app: `inventario/regista_ai.md`, `inventario/clipsai.md`,
`inventario/calliope.md`, `inventario/zp_video.md`. Design esecutivo e comandi previsti:
`TRACCIA_PARITA.md`. Stato reale delle fasi: `TODO.md`.

### Screening operatore (23/11/2026)

- **Già coperto dalla NL Video Suite precedente** (da preservare, non rifare): import progetto NL,
  media in place, trascrizione e mirror testo, correzioni parole, Find Shorts locale e AI,
  ComfyUI immagini di base, bridge ASR WhisperX. Restano da portare le funzioni elencate sotto.
- **Fuori scope**: Calliope **Build Scene / AI Canvas / 3D** (Three.js/MediaRecorder: non è una
  funzione NLE; eventuale P8 separato). **TTS/NL voce escluso**: l'operatore è doppiatore e un TTS
  sarebbe un placeholder; il Text to Speech interno del fork resta com'è e non fa parte della
  parità. Resta la **musica generativa**.
- L'harness agente completo di Calliope e il server web di Regista non si replicano: se ne porta
  il comportamento editoriale (chat minima opzionale, scritture testuali, coda), non
  l'architettura.

## Sorgenti di riferimento (sola lettura)

| App | Percorso | Ruolo |
|---|---|---|
| ZP Shorts Studio | `K:\AI\ZioPaolo Video` | short/compilation da video lunghi, sessione e cache |
| Regista AI | `K:\AI\Regista_AI` | agent, storyboard, image/video/audio/TTS, workflow repair, coda |
| ClipsAI | `K:\AI\ClipsAI` | Sgrossatura, Clipping social, WhisperX, SRT, reframe |
| Calliope | `K:\AI\Calliope` | story → scene → shot → video → film, settings e coda |
| NL Video Suite | `K:\AI\NL_Video_Suite` | integrazione attuale e bridge |

Regola clean-room: si studiano documentazione, UI e comportamento; **non si copia codice o
wording** di altre app. Calliope è MIT, ma resta un prodotto esterno: se ne replica il
comportamento, non il codice.

## Matrice

Legenda: ✅ integrato e verificato; 🟡 codice pronto, verifica GUI in corso; ⬜ da fare.

### Progetto, media, timeline

| Funzione | Fonte | Stato | Accesso GUI previsto |
|---|---|---|---|
| Import progetto NL, media in place, trascrizione | NL | ✅ | File > Import NL Project |
| Correzione parole + mirror | NL | ✅ | Text panel |
| Sequenze, bin, marker, export NLE | FilmCraft | ✅ | UI FilmCraft |
| Creazione clip/asset/reel da UI | Regista | ⬜ | pannello Clip/Reel |
| Sessioni e cache per video | ZP | 🟡 (cache ASR) | pannello ASR |
| Apertura progetto NL originale | NL | ✅ | già |

### Shorts e clipping

| Funzione | Fonte | Stato | Accesso GUI previsto |
|---|---|---|---|
| Find Shorts locale | NL/F4a | ✅ | Sequence |
| Find Shorts AI con hook verificabile | NL/F4b | ✅ | Sequence |
| Candidati editoriali + compilation | ZP | ⬜ | pannello Shorts |
| Clipping social con score/dedupe | ClipsAI | 🟡 P3a: proposta/selezione/render via bridge | pannello NL Clipping |
| SRT normale + “hormozi” per clip | ClipsAI | ✅ P3a (timestamp parola, ASS generabile) | pannello NL Clipping |
| Reframe 9:16 con sfondo sfocato | ClipsAI/Regista | 🟡 P3a: copia verticale nel render Clipping | pannello NL Clipping |
| Burn sottotitoli | ClipsAI | ⬜ P3b (ASS con PlayRes) | pannello NL Clipping |
| Sgrossatura silenzi/fiati/filler/bad-takes | ClipsAI | ✅ P3a via bridge | pannello NL Rough Cut |
| Editor waveform con tagli manuali | ClipsAI | ⬜ P3b | pannello NL Rough Cut |
| Report tagli + anteprima | ClipsAI | 🟡 P3a: report sì, anteprima no | pannello NL Rough Cut |

### ASR e testo

| Funzione | Fonte | Stato | Accesso GUI previsto |
|---|---|---|---|
| WhisperX large-v3 + allineamento IT | ClipsAI | ✅ bridge / 🟡 comando | Text > Transcribe / dialogo |
| Scelta modello/device ASR | ClipsAI/Regista | ⬜ | Impostazioni NL |
| Import SRT/VTT/SBV | ZP/ClipsAI | 🟡 (FilmCraft captions) | File > Import |
| Traduzioni EN/ES | NL | ⬜ | Text > Translate |
| Diarizzazione/VAD | ClipsAI | 🟡 (VAD interno) | Impostazioni ASR |
| Trascrizione da URL/voce | Regista | ⬜ | pannello ASR |

### Generazione visiva

| Funzione | Fonte | Stato | Accesso GUI previsto |
|---|---|---|---|
| Immagine da prompt via ComfyUI | F5 | ✅ | Grafica > Generate |
| Scelta workflow/seed/size | F5 | ✅ | dialogo Generate |
| Cover AI da frame + contesto parlato | NL/F5 + GOAL_UX R7 | ✅ P4b (payload verificato, live da provare) | pannello NL Cover + Graphics ▸ Generate Cover from Frame… |
| Storyboard/character sheet | Regista/Calliope | ⬜ | pannello Storyboard |
| Image/video generation per scena | Regista/Calliope | ⬜ | pannello Storyboard |
| Vision/analyze asset | Regista | ⬜ | pannello Asset |
| Workflow repair/autofix/install | Regista | ⬜ | Impostazioni > Workflow |
| Coda raggruppata per modello/workflow | Regista/Calliope | 🟡 (job singoli) | pannello Coda |

### Audio, musica, voce

| Funzione | Fonte | Stato | Accesso GUI previsto |
|---|---|---|---|
| Musica Ace Step / MiniMax | F6 | 🟡 codice, test da rifare | Audio > Generate Music |
| TTS Qwen3 / voci | Regista | ➖ **escluso** (operatore doppiatore) | — |
| Mixer, gain, pan, sends | FilmCraft | ✅ | Mixer |
| Ducking/loudness/limiter | FilmCraft | ✅ base | Essential Sound |
| Sgrossatura audio e fiati | ClipsAI | ⬜ | Sgrossa |
| Analisi energia/topic reel | Regista | ⬜ | pannello Reel |

### Regia, storia, agenti

| Funzione | Fonte | Stato | Accesso GUI previsto |
|---|---|---|---|
| Idea → beat → personaggi/luoghi | Calliope | ⬜ | pannello Storia |
| Script per scena | Calliope | ⬜ | pannello Storia |
| Shot list e prompt per clip | Calliope/Regista | ⬜ | pannello Storia |
| Montaggio film con crossfade/loudness | Calliope | ⬜ | esporta film |
| Chat agent con workspace | Regista/Calliope | ⬜ | pannello Assistente |
| Shell sandbox / regole | Calliope | ⬜ | Impostazioni Agente |
| Progetti SQLite e stato | Calliope | ⬜ | gestione progetti |

### Impostazioni e servizi

| Funzione | Fonte | Stato | Accesso GUI previsto |
|---|---|---|---|
| ComfyUI URL, modelli, workflow default | tutte | 🟡 hardcoded/default | Impostazioni NL |
| Ollama URL e modello per funzione | tutte | 🟡 default | Impostazioni NL |
| ASR provider/modello/device | ClipsAI | 🟡 device nel dialogo | Impostazioni NL |
| Lingua interfaccia | FilmCraft | ✅ | Impostazioni |
| Avvio/stop ComfyUI e Ollama | Regista | ⬜ | Impostazioni Servizi |
| Diagnostica/versioni/porte | NL/Regista | ⬜ | Aiuto > Diagnostica |
| Nessun download implicito | tutte | ✅ regola | ovunque |

## Politica risoluzione (obbligatoria, da impostazioni)

La sequenza aperta è la fonte di verità. Le generazioni non devono inventare risoluzioni.

| Tipo | Regola | Esempio |
|---|---|---|
| Immagine | pari alla sequenza **oppure** doppia (1×/2×) | seq 1920×1080 → 1920×1080 o 3840×2160 |
| Video | **2/3** (riduzione di 1/3) **oppure** metà della sequenza, arrotondato a 8/16 | seq 1920×1080 → 1280×720 o 960×540 |
| Audio | durata dalla sequenza o dal campo durata | 30 s, 60 s… |

- Le regole sono **forzabili** in Impostazioni NL: profilo immagine (1×/2×) e profilo video
  (1/2, 1/3, 720p standard), con override esplicito nel dialogo.
- Se il workflow non ha un nodo latente dimensionabile, il motore lo dichiara e non finge.
- I valori vanno arrotondati alla granularità del modello (8 o 16 px) e mostrati nel dialogo.

## GPU-first, fallback CPU

- ASR/WhisperX: provare **CUDA** per default; se il modello o l'allineamento non partono,
  riprovare in CPU una volta e dichiararlo nel risultato.
- ComfyUI: la GPU è gestita da ComfyUI; la suite non forza CPU e non uccide modelli di altre app.
- Ollama: modello piccolo, `num_gpu` automatico; niente download impliciti.
- Ogni job mostra device e fallback nella ricevuta, non solo nel log.

## Branding e pannelli

- L'interfaccia visibile è **NL Video Suite**: niente “FilmCraft” nei menu, titoli, dialoghi,
  status, messaggi o nomi dei pannelli. Il nome resta solo in licenza/attribuzione e nei file
  interni del fork.
- Ogni funzione NL ha un **pannello dedicato**: Genera, Musica, ASR, Clipping, Sgrossa,
  Storyboard/Storia, Coda, Impostazioni NL. I menu FilmCraft non bastano.
- I pannelli usano la palette NL viola/magenta/ciano e possono essere aperti da Window/Workspace.

## Ordine di lavoro proposto

1. **P1** Impostazioni NL uniche (URL, modelli, workflow, ASR, lingua) + persistenza.
2. **P2** Pannello ASR completo (provider/modello/device/cache/import) e traduzioni.
3. **P3** Pannello Clipping (analisi, selezione, SRT, reframe, burn) e Sgrossa (tagli, waveform, report).
4. **P4** Pannello Storyboard/Storia (Calliope/Regista) con generazione immagini/video e coda.
5. **P5** TTS e musica con selezione voci/modelli già installati.
6. **P6** Workflow repair/autofix e diagnostica servizi.
7. **P7** Portable, regressione GTA6, documentazione e ottimizzazione.

## Versione test classica e Prompter (P8/P9)

La parità non è solo verso le 4 app esterne: va portato in `filmcraft-nl` **tutto ciò che la
versione test della suite già faceva**. Inventario verificato (165 route, ~80 moduli, shell):
`inventario/versione_test.md`; fasi e criteri: `TRACCIA_PARITA.md` §P8. Inventario Prompter:
`inventario/prompter_comfyui.md`; workflow copiati in `workflows/prompter/` con `PROVENANCE.md`.

- **P8b fatto**: workspace di avvio **Quick Edit** (nessun pannello NL agganciato), nessun lavoro AI
  all'apertura, misure-obiettivo in `PERFORMANCE.md`.
- **P9a fatto**: workflow doctor **unito** alla verifica API (`nl.workflows.verify` legge
  `/object_info`, `nl.workflows.diagnose` statico+live, `nl.workflows.fix` copia riparata,
  `nl.workflows.import` per aggiungere workflow in futuro) e pannello **NL Workflows**.
- **P8d prima verticale + libreria reference fatta**: pannello **NL Story** — idea → bozza di
  storia con Ollama, affinamenti (campi scena + refine), library reference (character sheet +
  location plate generati una volta), keyframe per scena che usano le reference, video di scena,
  musica di scena e **posizionamento in timeline** (`nl.story.*`); prove `prove/p8d-story/`.
- **Da fare**: P8c (longform/ricerca/assistente/import SRT/traduzioni), P8d resto (video per scena,
  v2v/r2v, estensione sezioni, montage planner, shot map, flows), P8e (sottotitoli studio, motion,
  graphic objects, audio master), P8f (servizi/versioni/album/production package/storage/social),
  P9b–d (parametri reali + Prompt AI, coda varianti/prepara/invia, v2v con reference video/audio e
  inserimento in timeline).

## Regole non negoziabili

- Ogni funzione ha comando headless **e** accesso GUI.
- Modelli già presenti selezionabili; nessun download obbligatorio.
- Non sovrapporre Ollama e ComfyUI; usare modelli piccoli.
- Non toccare i progetti utente.
- Test e ricevute per ogni fase; niente funzioni dichiarate finte.
