# Parità funzionale NL Video Suite

Data: 23/11/2026. Obiettivo: **tutte** le funzioni delle app NL originali devono esistere in
`filmcraft-nl` e essere raggiungibili dalla GUI (menu, dialoghi, pannelli, impostazioni), non solo
da comandi headless. Nessun download obbligatorio: i modelli già presenti sono selezionabili.

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
| Clipping social con score/dedupe | ClipsAI | ⬜ | pannello Clipping |
| SRT normale + “hormozi” per clip | ClipsAI | ⬜ | pannello Clipping |
| Reframe 9:16 con sfondo sfocato | ClipsAI/Regista | 🟡 (motore presente) | Effect Controls / Clipping |
| Burn sottotitoli | ClipsAI | ⬜ | pannello Clipping |
| Sgrossatura silenzi/fiati/filler/bad-takes | ClipsAI | ⬜ | pannello Sgrossa |
| Editor waveform con tagli manuali | ClipsAI | ⬜ | pannello Sgrossa |
| Report tagli + anteprima | ClipsAI | ⬜ | pannello Sgrossa |

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
| Cover da frame + prompt | NL/F5 | ⬜ | Grafica > Generate Cover |
| Storyboard/character sheet | Regista/Calliope | ⬜ | pannello Storyboard |
| Image/video generation per scena | Regista/Calliope | ⬜ | pannello Storyboard |
| Vision/analyze asset | Regista | ⬜ | pannello Asset |
| Workflow repair/autofix/install | Regista | ⬜ | Impostazioni > Workflow |
| Coda raggruppata per modello/workflow | Regista/Calliope | 🟡 (job singoli) | pannello Coda |

### Audio, musica, voce

| Funzione | Fonte | Stato | Accesso GUI previsto |
|---|---|---|---|
| Musica Ace Step / MiniMax | F6 | 🟡 codice, test da rifare | Audio > Generate Music |
| TTS Qwen3 / voci | Regista | ⬜ | Text to Speech |
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

## Regole non negoziabili

- Ogni funzione ha comando headless **e** accesso GUI.
- Modelli già presenti selezionabili; nessun download obbligatorio.
- Non sovrapporre Ollama e ComfyUI; usare modelli piccoli.
- Non toccare i progetti utente.
- Test e ricevute per ogni fase; niente funzioni dichiarate finte.
