# Inventario ClipsAI — studio sola lettura

Data: 2026-11-23 · Fonte: `K:\AI\ClipsAI` (nessuna esecuzione, nessuna modifica).
Perimetro dichiarato: `AGENTS.md` + `todo.md` come specifica, poi verifica puntuale SOLO di:
costanti `sgrossatore_raw.py:1-87`, `MODE_RULES`/costanti `taglia_clip_per_editing.py:1-60`,
costanti + elenco `def` `motore.py`, route `server.py`, tab `gui/index.html`, campi/checkbox
`gui/app.js`. Il resto del codice NON è stato letto: ciò che non è provato da una riga citata è
marcato **NON VERIFICATO**. Percorsi relativi a `K:\AI\ClipsAI\` salvo indicazione diversa.

## 1. Identità e avvio

- **Prodotto**: toolkit locale Windows per post-produzione di parlato (doppiatore): sgrossatura,
  clipping social con AI, short verticali, montaggio timeline. Nessuna API cloud. `AGENTS.md:3`; `todo.md:10`.
- **Componenti**: `sgrossatore_raw.py`, `taglia_clip_per_editing.py`, `motore.py` (ASR/cache/AI/SRT
  condivisi), `shorts.py`, `brucia_sottotitoli.py`, `server.py` + `gui/` (index/app/style). `AGENTS.md:7-11,175-183`; `server.py:25-31`.
- **Avvio GUI**: `Avvia_GUI.bat` → `python_env\python.exe server.py`; UI su `http://127.0.0.1:5057`.
  `server.py:10-11,34`; `AGENTS.md:175`.
- **Avvio CLI**: `Avvia_Sgrossatore.bat` (input `video_da_sgrossare\*.mp4`, output
  `video_output\[video]_SGROSSO\`); `Avvio_Clipping.bat` (input `video_da_elaborare\*.mp4`);
  burner `python brucia_sottotitoli.py` (`reframed\` → `completed\`). `AGENTS.md:50-54`;
  `sgrossatore_raw.py:36-38`; `taglia_clip_per_editing.py:34-36`.
- **Server Flask**: solo loopback, `threaded=True`, `use_reloader=False` → dopo ogni modifica il
  processo va riavviato; statici serviti da `gui/` (`/` e catch-all). `server.py:33-34,184-191,760-762`; `AGENTS.md:60,118`.
- **Modello di esecuzione**: job asincroni in RAM (`jobs`/`jobs_lock`), id, polling
  `GET /api/job/<jid>`; un solo job pesante alla volta (`run_lock`) per la VRAM condivisa; i job
  si perdono al riavvio. `server.py:39-52,266-272`; `AGENTS.md:189,192`.
- **Runtime**: interprete dedicato `python_env\python.exe` (presente su disco); versione 3.10.11
  solo dichiarata nei doc. `AGENTS.md:71-73`.
- **UI**: 4 tab — Sgrossa, Clip/Reel, Shorts, Timeline. `gui/index.html:20-23`.
- **Modelli locali** (nessun download a runtime): ASR CT2 `D:\Faster-Whisper-XXL\...\faster-whisper-large-v3`;
  AST in `models\hf` (esiste) o cartella condivisa su `H:`. `motore.py:57-96`; `AGENTS.md:85-95`.

## 2. Catalogo funzioni

| ID | Funzione (parafrasi) | Input → Output | Opzioni/default | Dipendenze | Superficie | Evidenza |
|---|---|---|---|---|---|---|
| F01 | Sgrossatura: rileva e toglie le pause lunghe tra parole | video + parole allineate → intervalli di pausa e spezzoni buoni residui | `MAX_SILENCE_SEC=1.2`, `SILENCE_KEEP_PAD=0.20`, `CUT_EDGE_SILENCE=True`, `MIN_CUT_SEC=0.15`, `MIN_CLIP_SEC=0.8` | motore (ASR), ffmpeg | CLI+GUI | `sgrossatore_raw.py:41,57-61,69`; `AGENTS.md:36` |
| F02 | Sgrossatura: toglie i respiri solo se isolati e con guardia anti-abuso | gap tra parole + analisi silenzio → tagli fiato | `ENABLE_BREATH_DETECTION=True`, `BREATH_MIN_SEC=0.08`, `BREATH_MAX_SEC=0.7`, `BREATH_DB_THRESHOLD=-40`, `BREATH_ISOLATION_SEC=0.06`, `BREATH_MAX_WORD_RATIO=0.10` | ffmpeg silencedetect (dichiarato) | CLI+GUI | `sgrossatore_raw.py:50-54,62-63`; `AGENTS.md:37` |
| F03 | Sgrossatura: toglie i filler brevi, senza invadere le parole vicine | parole allineate + lista filler → tagli filler | `FILLER_WORDS=[ehm,uhm,uh,mhm,hmm,mh,eh,uhh,uhmm]`, `FILLER_MAX_SEC=0.7`, `WORD_SAFE_PAD=0.02` | motore | CLI+GUI | `sgrossatore_raw.py:42-43,61,66`; `AGENTS.md:38` |
| F04 | Bad-takes AI a 2 fasi: proposta con citazione delle parole, poi revisione che conferma o scarta | frasi numerate + annotazioni → tagli confermati | `ENABLE_AI_BAD_TAKES=True`, `MAX_CUT_SEC=60`, `AI_MIN_CONFIDENCE=55`, `AI_DEBUG_DUMP=True`, `AI_CHUNK_SEGS=90` (GUI invia 45) | Ollama, motore | CLI+GUI | `sgrossatore_raw.py:44-47,67-68`; `gui/app.js:125`; `AGENTS.md:39` |
| F05 | Sgrossatura: trasforma il piano in spezzoni buoni e li renderizza con micro-fade | piano tagli + video → `NNN_clip.mp4` numerati | `AUDIO_FADE_SEC=0.015`, `MERGE_GOOD_GAP_SEC=0.05`; un solo taglio piano alla volta | ffmpeg libx264 crf 20 (doc) | CLI+GUI | `sgrossatore_raw.py:64-65,69`; `AGENTS.md:40-41` |
| F06 | Report e verifica: scrive il piano e i range sorgente, misura ogni clip resa | tagli + clip → `tagli_sgrossatura.txt/.json`, scostamenti >0.30s segnalati | `AUTO_REPORT=True` (anche dai job GUI) | ffprobe | CLI+GUI | `sgrossatore_raw.py:66`; `AGENTS.md:42`; `todo.md:68-70` |
| F07 | Anteprima ritmo: concatena gli spezzoni in un unico file | spezzoni buoni → `00_anteprima_sgrossata.mp4` | `MAKE_PREVIEW=True` | ffmpeg concat | CLI+GUI | `sgrossatore_raw.py:65`; `AGENTS.md:42,70` |
| F08 | Analisi senza taglio + taglio solo dei selezionati + piano modificato a mano | video / `selected` / `cuts` → piano ricalcolato e render | piano riscritto in `sgrossa_analyze.json`; `manual_edit_at` | Flask job, sgrossatore | GUI | `server.py:323-356,698-748`; `AGENTS.md:185-187` |
| F09 | Waveform per l'editor: cache di picchi per video + downsample a richiesta | video / `n` → picchi + durata + fingerprint | cache max 12000 picchi, `n` clampato 200..12000; file `{base}_waveform.json` accanto al video | ffmpeg s16le 8 kHz, numpy | GUI | `server.py:633,636-695`; `AGENTS.md:67` |
| F10 | Clipping: 3 modalità con durate e tetti clip imposti | video + modo (1/2/3) → clip proposte | 1: max 25 / min 4 / max 8; 2: max 75 / min 20 / max 6; 3: max 75 / min 40 / max 4, 2 passaggi AI (modo 3) | Ollama, motore | CLI+GUI | `taglia_clip_per_editing.py:39-43`; `gui/index.html:110-112`; `AGENTS.md:150` |
| F11 | Ranking clip: punteggio 1-10 e titolo, scarto dei doppioni sovrapposti | clip proposte → selezione ordinata | `DEDUPE_OVERLAP=0.50`; `AI_CHUNK_SEGS=45`; contenuto esatto di score/titolo **NON VERIFICATO** (schema oltre il perimetro letto) | Ollama | CLI+GUI | `taglia_clip_per_editing.py:45-46`; `AGENTS.md:29-31`; `todo.md:18` |
| F12 | Render clip: master 16:9 + eventuale verticale, più SRT per clip | clip + video → `NN_TITOLO.mp4`, `_V.mp4`, SRT | `vertical` default true lato API; boxblur verticale solo da doc (**NON VERIFICATO** nel codice) | ffmpeg | CLI+GUI | `server.py:383-432`; `gui/index.html:115`; `AGENTS.md:33` |
| F13 | Sottotitoli: cue dai timestamp parola, versione normale e versione "hormozi" | parole allineate → cue/SRT, opzione maiuscolo e offset | `save_srt(cues, path, offset=0.0, upper=False)`; 2 parole/cue e MAIUSCOLO solo da doc | motore | CLI+GUI | `motore.py:667,678,713`; `AGENTS.md:32` |
| F14 | Reframe 9:16 con sfondo sfocato/zoom e sub laterali | video orizzontale → 1080x1920 | nei Shorts: verticale ON, burn OFF di default (GUI) | ffmpeg | GUI | `gui/index.html:161-163`; `gui/app.js:530`; `AGENTS.md:221-224` |
| F15 | Burn sottotitoli su master verticale | video + SRT/ASS → video con sub fusi | CLI: `brucia_sottotitoli.py` `reframed/`→`completed/`; Shorts: `burn_subs` default false | ffmpeg/libass | CLI+GUI | `server.py:486-493`; `gui/index.html:162`; `AGENTS.md:10,54,205`; esistenza del file verificata su disco, contenuto **NON VERIFICATO** |
| F16 | Shorts: momenti → raggruppamento per argomento → short 9:16 con sub | video + lingua → `SHORT_NN_*_FINAL_V.mp4` | GUI: min shorts 3, durata 20-60s, verticale ON, sub fusi OFF | Ollama, taglia, motore | GUI | `server.py:436-496`; `gui/index.html:149-150`; `gui/app.js:513`; `AGENTS.md:181,187` |
| F17 | Timeline montage: normalizza pezzi, concatena senza ricodifica finale | item range/file → `MONTAGE_*_<nome>.mp4` | `format=auto` (9:16 solo se tutti verticali), `16:9`, `9:16`, `source`; fps 30, scale+pad, x264 crf 20 | ffmpeg | GUI | `server.py:522-619`; `AGENTS.md:234-236` |
| F18 | Eventi audio AST (fiati/risate/musica) come annotazioni per l'AI | audio → eventi + annotazioni per frase | `ENABLE_AUDIO_EVENTS=False`, `AST_MODEL=MIT/ast-finetuned-audioset-10-10-0.4593`, finestra 5s / hop 2.5s / batch 4 | `audio_events.py` (esiste), torch | CLI+GUI | `motore.py:66-79,344,382`; `AGENTS.md:226-232` |
| F19 | Cache trascrizione per video, con firma di modello e lingua | video → JSON riusabile | `{work_dir}/{base}_trascrizione.json`; hit richiede fingerprint+signature+words; `force` per ignorarla | motore | CLI+GUI | `motore.py:142,181-187,206,220-235`; `AGENTS.md:20` |
| F20 | Ollama: stato live, avvio, elenco modelli per la GUI | richieste HTTP locali → stato/pill e select modello | status timeout 3s; start lancia `ollama app` detached; modelli con "embed"/"cloud" esclusi | Ollama `:11434` | GUI | `server.py:211-263`; `gui/app.js:14-33`; `AGENTS.md:218` |

## 3. Impostazioni e default

### 3.1 Sgrossatura (`sgrossatore_raw.py`)

| Costante | Valore | Evidenza |
|---|---|---|
| `VIDEO_INPUT_DIR` / `VIDEO_OUTPUT_DIR` | `K:\AI\ClipsAI\video_da_sgrossare` / `...\video_output` | `sgrossatore_raw.py:36-37` |
| `LANGUAGE` | `it` | `:38` |
| `MAX_SILENCE_SEC` | 1.2 | `:41` |
| `FILLER_MAX_SEC` / `MIN_CLIP_SEC` / `MIN_CUT_SEC` | 0.7 / 0.8 / 0.15 | `:43,57-58` |
| `SILENCE_KEEP_PAD` / `WORD_SAFE_PAD` | 0.20 / 0.02 | `:60-61` |
| `BREATH_ISOLATION_SEC` / `BREATH_MAX_WORD_RATIO` | 0.06 / 0.10 | `:62-63` |
| `AUDIO_FADE_SEC` / `MERGE_GOOD_GAP_SEC` | 0.015 / 0.05 | `:64,69` |
| `MAKE_PREVIEW` / `AUTO_REPORT` | True / True | `:65-66` |
| `AI_CHUNK_SEGS` | 90 nel modulo, ma la GUI invia 45 come default | `:45`; `gui/index.html:46`; `gui/app.js:125` |
| `MAX_CUT_SEC` / `AI_MIN_CONFIDENCE` | 60.0 / 55 | `:47,67` |
| Chiavi override GUI | lista in `_PARAM_KEYS` + `run_with_params` | `:72-90` |

### 3.2 Clipping (`taglia_clip_per_editing.py`)

| Costante | Valore | Evidenza |
|---|---|---|
| `VIDEO_INPUT_DIR` / `VIDEO_OUTPUT_DIR` | `...\video_da_elaborare` / `...\video_output` | `:34-35` |
| `LANGUAGE_DEFAULT` | `it` | `:36` |
| `MODE_RULES` | 1 = 25/4/8; 2 = 75/20/6; 3 = 75/40/4 (max_dur/min_dur/max_clips) | `:39-43` |
| `AI_CHUNK_SEGS` / `DEDUPE_OVERLAP` | 45 / 0.50 | `:45-46` |
| Override per modo | `run_with_params` patcha solo la regola della modalità attiva | `:48-60` |

### 3.3 Motore (`motore.py`)

| Costante | Valore | Evidenza |
|---|---|---|
| `OLLAMA_HOST` / `OLLAMA_MODEL` | `http://localhost:11434` / `qwen2.5:14b-instruct` | `:37-38` |
| `OLLAMA_TIMEOUT` / `TEMPERATURE` | 900 s / 0.2 | `:39,49` |
| `NUM_CTX` | env `CLIPSAI_NUM_CTX`, default 8192 (doc: modello max 32768) | `:40`; `AGENTS.md:27` |
| `WHISPER_MODEL` / `WHISPER_DEVICE` / `WHISPER_INT8` | `large-v3` / `cuda` / True | `:52-54` |
| `WHISPER_MODEL_PATH` | env `CLIPSAI_WHISPER_MODEL` oppure `D:\...\faster-whisper-large-v3` | `:56-58` |
| `SHARED_MODELS_DIR` | `H:\...\Diarizzazione\models` | `:61` |
| `ENABLE_AUDIO_EVENTS` / `AST_MODEL` | False / `MIT/ast-finetuned-audioset-10-10-0.4593` | `:66-67` |
| `AST_WINDOW_SEC` / `AST_HOP_SEC` / `AST_BATCH` | 5.0 / 2.5 / 4 | `:68-70` |
| `AST_CACHE_DIR` | env `CLIPSAI_AST_CACHE`, altrimenti prima cache valida tra progetto, `H:\...\hf\hub`, `H:\...\hf` | `:74-96` |
| `FFMPEG_EXE` / `FFPROBE_EXE` | `ffmpeg` / `ffprobe` (PATH) | `:98-99` |
| `AI_SEG_MAX_SEC` / `AI_SEG_MAX_WORDS` | 12.0 / 16 | `:102-103` |

### 3.4 Server (`server.py`)

| Costante | Valore | Evidenza |
|---|---|---|
| `PORT` / bind | 5057 / `127.0.0.1` | `:34,762` |
| `VIDEO_EXTS` / `PREVIEW_EXTS` | `.mp4,.mkv,.mov` / `.mp4,.srt,.txt,.json` | `:36-37` |
| `MAX_PEAKS` | 12000 (doc dice anche 1500: vedi §6) | `:633` |

### 3.5 Variabili d'ambiente e percorsi

- Env supportate: `CLIPSAI_NUM_CTX`, `CLIPSAI_WHISPER_MODEL`, `CLIPSAI_AST_CACHE` (`motore.py:40,57,94`; `AGENTS.md:91-93`).
- Input/scratch: `video_da_sgrossare\`, `video_da_elaborare\`, `video_output\`, `_scratch\` (mai fuori progetto) (`AGENTS.md:124-135`; `todo.md:43-47`).
- Cache/artefatti per video: `{base}_trascrizione.json` (`motore.py:220`), `{base}_waveform.json` accanto al video (`server.py:649-650`), `sgrossa_analyze.json`/`clip_propose_{mode}.json`/`reel_propose.json`/`shorts_propose.json` (`AGENTS.md:216`; `server.py:284-287,719`).
- Backup output precedenti: cartella `_backup_YYYYMMDD_HHMMSS_microsecondi` (`motore.py:190-201`; `AGENTS.md:112`).
- Layout di `work_dir` e nomi esatti delle cartelle di lavoro: **NON VERIFICATI** (funzioni non lette).

## 4. ASR, modelli, device

- **Engine**: WhisperX importato in lazy (`_import_whisperx`), faster-whisper CTranslate2 `large-v3`,
  `compute_type=int8` se `WHISPER_INT8`, batch 16, lingua passata a `transcribe`. `motore.py:52-54,148,237-258,269`.
- **Modello**: solo copia locale; se la cartella o `model.bin` mancano solleva errore esplicito e non
  scarica nulla. `motore.py:163-178`.
- **VAD**: la spec dice pyannote configurato in `load_model(..., vad_method="pyannote")`
  (`AGENTS.md:18,99-101`), **ma la chiamata reale non passa `vad_method`** (`motore.py:248-252`) e
  una ricerca di `vad|pyannote` trova solo docstring/commenti → **NON VERIFICATO** che il VAD sia
  pyannote: probabile default della libreria. Da verificare a runtime prima di replicare.
- **Alignment**: `load_align_model(language_code=...)` + `whisperx.align(...)`; il nome
  `VOXPOPULI_ASR_BASE_10K_IT` compare solo nei doc (`motore.py:280-285`; `AGENTS.md:19,87`) →
  checkpoint esatto **NON VERIFICATO**.
- **Fallback CPU**: esiste ed è reale per il modello ASR: `try` su GPU, `except` → ricarica su
  `cpu` int8 (`motore.py:246-258`). Se invece fallisce l'allineamento, non c'è fallback CPU: si
  tengono i timestamp CTranslate2 (`motore.py:287-288`). Il device AST è scelto a runtime
  (cuda se disponibile, altrimenti cpu) (`motore.py:358-359`).
- **Cache**: fingerprint del video + firma (lingua, modello, path modello, `engine`) + presenza
  parole; su hit si restituisce subito il JSON e si calcolano solo gli eventi AST in lazy; la
  return è fuori dal `with` per un bug Windows di replace (`motore.py:142,181-187,220-235,323-325,344`).
- **Stime doc**: ~35-45 s per 150 s di audio su RTX 4060 Ti 8 GB con Ollama in VRAM (`AGENTS.md:111,171`; `todo.md:26`) — non misurate qui.

## 5. API Flask

| Metodo | Path | Parametri principali | Evidenza |
|---|---|---|---|
| GET | `/` | — (serve `gui/index.html`) | `server.py:184-186` |
| GET | `/<path:fname>` | nome file statico in `gui/` | `server.py:189-191` |
| GET | `/api/videos` | `pipe` (default `sgrossa`; `clip`/`shorts`) | `server.py:195-208` |
| GET | `/api/ollama/status` | — → `{running, models, loaded[]}` | `server.py:211-226` |
| POST | `/api/ollama/start` | — (avvia `ollama app` se spento) | `server.py:229-245` |
| GET | `/api/models` | — (modelli Ollama filtrati + `default`) | `server.py:248-263` |
| GET | `/api/job/<jid>` | job id nel path | `server.py:266-272` |
| GET | `/api/last` | `video`, `pipe` (`sgrossa`/`clip`/`reel`/`shorts`), `mode` | `server.py:275-296` |
| POST | `/api/sgrossa/analyze` | `video`, `model`, `params` (es. `MAX_SILENCE_SEC`, `AI_CHUNK_SEGS`) | `server.py:323-338`; `gui/app.js:119-130` |
| POST | `/api/sgrossa/cut` | `job_id` oppure `input_path`+`work_dir`; `selected=[[start,end],...]` | `server.py:341-356` |
| POST | `/api/clip/propose` | `video`, `mode` (default `2`), `lingua`, `model`, `params` | `server.py:360-380` |
| POST | `/api/clip/render` | `job_id`/`items`/`input_path`+`work_dir`, `selected`, `vertical` (default true), `lingua`, `model`, `params` | `server.py:383-432` |
| POST | `/api/shorts/propose` | `video`, `lingua`, `model`, `params` (`N_SHORTS_MIN`, `SHORT_MIN_DUR`, `SHORT_MAX_DUR`) | `server.py:436-452`; `gui/app.js:513` |
| POST | `/api/shorts/render` | `items` o pool+`selected`, `vertical` (true), `burn_subs` (false), `params`, `model`, `lingua`, path espliciti | `server.py:455-496` |
| GET | `/api/workfiles` | `dir` → mp4 con durata e dimensioni | `server.py:522-535` |
| POST | `/api/montage/render` | `items` (`kind:range|file`), `work_dir`, `name` (default montaggio), `format` (auto/16:9/9:16/source) | `server.py:538-619` |
| GET | `/api/file` | `path` (mp4/srt/txt/json) | `server.py:623-630` |
| GET | `/api/waveform` | `video`, `n` (default 12000, clamp 200..12000) | `server.py:636-695` |
| POST | `/api/sgrossa/plan` | `work_dir`, `cuts=[{start,end,reason}]`, `duration` | `server.py:698-748` |
| POST | `/api/open` | `path` (apre Explorer) | `server.py:751-757` |

Nota: i parametri di path passano da `_req_path`/`_safe_path`; il contenimento nel progetto è
dichiarato (`AGENTS.md:192`) ma l'implementazione **NON VERIFICATA**.

## 6. Rischi, limiti e cosa NON replicare

**Divergenze doc↔codice trovate in questo perimetro**
1. VAD pyannote dichiarato ma non passato a `load_model` (`AGENTS.md:101` vs `motore.py:248-252`) → rischio di credere a un VAD che non c'è.
2. Modo 2: `MODE_RULES` 20-75 s (`taglia_clip_per_editing.py:41`) e GUI 20-75 (`index.html:111`), ma `AGENTS.md:150` dice 30-75 e il docstring 30-60 (`taglia:7`). Vale il codice.
3. `AI_CHUNK_SEGS` sgrossatura: 90 nel modulo (`:45`), 45 in GUI (`index.html:46`) e in `todo.md:84` → default effettivo diverso tra CLI e GUI.
4. Waveform: doc "1500 picchi" (`AGENTS.md:201`) vs 12000 nel codice (`server.py:633`) e `AGENTS.md:67`.

**Limiti tecnici**
- Solo Windows: percorsi assoluti `K:`/`D:`/`H:`, `os.startfile`, `DETACHED_PROCESS`, `chcp 65001`, workaround cp1252 nel logger. `motore.py:35,57-61,109-116,358`; `server.py:239-242,756`; `AGENTS.md:110`.
- GPU 8 GB condivisa tra Ollama (~5 GB) e ASR int8 (~3 GB): un job alla volta, margini stretti. `AGENTS.md:113,189`.
- Job e stato solo in RAM; risultati su JSON sparsi; nessun DB, nessuna coda, nessuna autenticazione (solo loopback). `server.py:40-42`; `AGENTS.md:192`.
- Parametri globali mutati con context manager: corretto solo grazie al lock a job singolo. `sgrossatore_raw.py:90-100`; `taglia_clip_per_editing.py:54-60`; `AGENTS.md:190`.
- Cache basata su size+mtime: ricodifica che preserva entrambi può produrre cache stantia (rischio teorico). `motore.py:142,229-231`.
- Burn SRT su 9:16 noto per libass senza `PlayRes` (testo fuori scala); i Shorts generano `.ass` con `PlayRes` espliciti, il burner CLI ha lo stesso difetto latente. `AGENTS.md:205-206`.
- Eventi AST opt-in, precisione alta ma recall basso sui respiri; modello ~500 MB, doppia copia su `H:` non necessaria. `AGENTS.md:89,228-230`.
- Nessun test automatico/CI documentato: la verifica è manuale via checklist e log. `AGENTS.md:158-171`.

**Cosa NON replicare in NL_Video_Suite**
- Non reintrodurre i tagli "secchi" dentro le parole né il taglio dell'intero gap senza cuscinetto attorno alla voce (`AGENTS.md:117`).
- Non usare matching fuzzy testo→timestamp: solo indici di frase e structured output (`AGENTS.md:25`).
- Non bruciare SRT su verticale senza file ASS con `PlayRes` (`AGENTS.md:205-206`).
- Non usare modelli Ollama `*-cloud` né API cloud: vincolo "tutto locale" (`AGENTS.md:116`; `todo.md:90`).
- Non dipendere dalla cartella condivisa `H:` né da percorsi assoluti: rendere modelli e input configurabili.
- Non basare la persistenza dei job sulla RAM del server; non esporre le API oltre il loopback.
- Non fidarsi dei numeri nei doc senza ricontrollo: questo perimetro ha già prodotto 4 divergenze.
- Non scrivere artefatti di test fuori dal progetto (`todo.md:43-44`).
- Non copiare il wording dei prompt (qui solo parafrasi) e non riusare il codice `audio_events.py` senza capirne la dipendenza da torch/HF.
