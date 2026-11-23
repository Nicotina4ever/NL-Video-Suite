# Inventario ZP Shorts Studio — sola lettura

Data: 2026-11-23 · Fonte: `K:\AI\ZioPaolo Video` (nessuna esecuzione, nessuna modifica a ZP).
Base percorsi: `ZP-Shorts-Studio-Stable\` per codice/config (`src\...`, `config\...`); i due documenti
radice sono `..\project_engine_architecture.md` e `..\MEMORY.md`. Metodo: documenti indicati (in ordine)
+ ispezione puntuale dei soli range richiesti. Tutto il resto NON è stato letto → "NON VERIFICATO".
Esiste una seconda copia `ZP Shorts Studio - prova\` con gli stessi file: **scopo di questo inventario =
la copia Stable**; la copia "prova" non è stata letta (tranne constatarne l'esistenza).

## 1. Identità

- **Prodotto**: ZP Shorts Studio PBNL-1.0, "copia portatile" Mac/Windows; commit sorgente `wip/prova 8954cf0`;
  creata 2026-09-27; non include video, sessioni, modelli AI, chiavi API — `VERSIONE.txt:1-4`.
  Contenuto dichiarato nella versione: Preview finale → Finalizza → Esporta, Final Version
  (original / 16:9 / 9:16), Master finale -14 LUFS / -1,0 dBTP, titolo, durata a fotogrammi,
  "Salva progetto" conserva musica e mixer — `VERSIONE.txt:2`.
- **Config interna**: `version: "0.1.0"`, `project_name: "ZP Shorts"` — `config\default_config.json:2-3`
  (disallineamento col nome prodotto PBNL-1.0: probabile debito, non verificato altrove).
- **Cos'è**: app web locale per short verticali; motore Python + editor browser, entrambi su 127.0.0.1;
  **nessuna dipendenza pip** (stdlib + tkinter), whisper.cpp/MacWhisper solo CLI esterni — `ZP-Shorts-Studio-Stable\AGENTS.md:4`.
- **Stack**: server HTTP in `src\web\server.py` (~2350 righe, static in `src\web\static\`) — `AGENTS.md:22`;
  CLI pipeline in `src\cli\main.py` — `AGENTS.md:23`; traduzione "engine" versionata nel health:
  `engine: zp-shorts-web-v1` + `version` + `PRODUCT_VERSION` — `src\web\server.py:1636`.
- **Avvio**: macOS `Avvia ZP Shorts Mac.command` / `.app`; Windows `Avvia ZP Shorts Windows.cmd` (trova
  Python 3.12–3.14 via `py`); Windows embedded scarica Python 3.13.9 + FFmpeg in `dep_windows\` — `AGENTS.md:7-10`.
  Dev: `python -B -m src.web.server --port 8766 --sessions-root sessions` — `AGENTS.md:14`.
  Launcher: `script/start_studio.py` con fallback porta **8766–8799** e single-instance via
  `runtime/server.json` — `AGENTS.md:21`; `LEGGIMI - PROVA.txt:31`.
  `--clean` (cache/tmp/runtime) e `--clean-all` (anche sessioni) — `AGENTS.md:10`; `LEGGIMI - PROVA.txt:15-16`.
- **Solo loopback**: il server non è esposto in rete — `AGENTS.md:29`; `LEGGIMI - PROVA.txt:24`.
- **Contratto architetturale**: il core Python è autorità unica su timeline math, KEEP segments,
  subtitle remapping e `candidates.json`; ogni interfaccia legge JSON e invoca la CLI, senza
  duplicare la logica — `..\project_engine_architecture.md:15,217-220,224-230`.
- **Pipeline dichiarata**: probe → audio.wav → transcript.json → candidates.json → editorial_selection.json
  → compilation_proposals.json → compilations/*.json → mp4 finali — `..\project_engine_architecture.md:32-36`.
- **Regola editoriale di prodotto** (non codice): lavoro reale, mai tutorial/consigli generici —
  `..\project_engine_architecture.md:26`; `src\models\short.py:16-17`.
- **Dipendenze esterne**: ffmpeg/ffprobe obbligatori; Whisper opzionale; Python 3.10+ — `..\project_engine_architecture.md:234-240`.

## 2. Catalogo funzioni (parafrasi)

| ID | Funzione | Input | Output | Opzioni/default | Superficie | Evidenza |
|---|---|---|---|---|---|---|
| F01 | Probe metadati video (ffprobe) e creazione sessione | path video | `metadata.json`, `session.json`, session_id deterministico `YYYY-MM-DD_<nome>_<sha8>` | — | CLI | `src\cli\main.py:1967-1968`; `..\project_engine_architecture.md:38-44` |
| F02 | Estrazione audio per STT | video | WAV 16 kHz mono PCM s16le | `--force` sovrascrive | CLI | `src\cli\main.py:1971-1973`; `config\default_config.json:6-11` |
| F03 | Trascrizione | video | `transcript.json` + `transcript.txt` | `--backend auto\|macwhisper\|mock\|whispercpp`, `--model`, `--language it`, `--force` | CLI | `src\cli\main.py:1976-1981`; `config\default_config.json:12-20` |
| F04 | Import trascrizione da sottotitoli | sessione + `.srt/.sbv/.vtt` | `transcript.json` (salta audio/Whisper) | `--language it`, `--force` distruttivo | CLI | `src\cli\main.py:1984-1994` |
| F05 | Analisi end-to-end | video | probe+audio+transcript+candidates+editorial+proposals | come F03 + `--transcript-file` | CLI | `src\cli\main.py:2013-2023` |
| F06 | Generazione candidati | trascritto in finestre temporali | `candidates.json` con score multidimensionale | finestre ~5–8 min, overlap 60–90 s; logica **rule-based** dichiarata | CLI | `..\project_engine_architecture.md:61-67,213-214` |
| F07 | Consultazione/validazione candidati | sessione | tabella/JSON | `--min-score`, `--category`, `--json` | CLI | `src\cli\main.py:2032-2036` |
| F08 | Selezione editoriale | candidati | `editorial_selection.json` con ruoli e relazioni | ruoli `single\|compilation_member\|both`; `--force` scarta stato precedente | CLI | `src\cli\main.py:2163-2165`; `src\models\short.py:31-40` |
| F09 | Proposte compilation (sequenze narrative) | candidati/editorial | `compilation_proposals.json` | `--force` scarta `human_review` e `promoted_compilation_id` | CLI | `src\cli\main.py:2176-2182` |
| F10 | Review umana + tagli interni | id candidato | stato review + override cut | `--approve/--reject/--needs-revision/--unreviewed`, `--cut-in/--cut-out`, `--notes`, `--selection`; `candidate-cut add/remove/clear` con `--start/--end` | CLI | `src\cli\main.py:2077-2088,2091-2107` |
| F11 | Export candidati | candidati approvati/id | clip 16:9 e/o short 9:16, proof clip | `--id`, `--all-approved`, `--use-semantic`, `--proof`, `--vertical`, `--format 16:9\|9:16\|both`, `--background-mode`, `--output-fps 30\|60\|source`, `--subtitles`, `--waveform`, `--all-candidates` (debug), `--force` | CLI | `src\cli\main.py:2045-2074` |
| F12 | Compilation base e verticale | id compilation, clip ordinati | video 16:9 base / 9:16 finale | create: `--clips`, `--max-duration 180`, `--title`; render-base: `--audio-fade-ms 0`; render-vertical: bg/fps, sottotitoli e waveform **default true** | CLI | `src\cli\main.py:2120-2150` |
| F13 | Proposte: review/effective/plan/render/promote | id proposta | preview 9:16 o compilation promossa | review `--approve/--reject/--needs-revision` + `--clips/--title/--notes`; render `--force`; promote `--effective` (snapshot autonomo, non muta la proposta), `--force` | CLI | `src\cli\main.py:2185-2235` |
| F14 | Versioni (Version Architecture) | sessione/version_id o JSON da stdin | lista/risoluzione/salvataggio/render | `version-list/resolve/save/save-as/delete/render`; render `--preview` (default) o `--final`, `--force`; `--no-verify-files` solo diagnostica | CLI | `src\cli\main.py:2238-2272` |
| F15 | Preview finale / Finalizza / Esporta | bozza salvata + revisione | piano finale, file per formato | formati `original`, `16:9`, `9:16`; differenze solo non semantiche preview/export | GUI(HTTP) | `src\web\server.py:1697-1712`; `src\media\final_output.py:1-16,38-42` |
| F16 | Master loudness / true peak | mix del piano | master normalizzato | target **-14 LUFS**, **-1.0 dBTP**, limiter ceiling -1.5, tolleranza 1 LU, calibrazione 0.5 LU × 4 round | GUI(HTTP)/motore | `src\media\final_output.py:52-60` |
| F17 | Mixer / musica | piano mix | piano mix JSON + anteprima audio FLAC | route `mix` e `mix/audio` | GUI(HTTP) | `src\web\server.py:1713-1721` |
| F18 | Waveform | media id | picchi JSON (cache in `waveforms/`) | variante `?density=montage` con densità dedicata | GUI(HTTP) | `src\web\server.py:1794-1806` |
| F19 | Editing proposal / preset / istruzioni / log | stato editor + revisione bozza | proposta editing legata alla revisione | catalogo preset, istruzioni e log separati | GUI(HTTP) | `src\web\server.py:1681-1696` |
| F20 | Montage proposal (clip_reel / short) | asset id | proposta montage salvata in `ai_proposals/` | validazione id e fingerprint media | GUI(HTTP) | `src\web\server.py:1763-1780` |
| F21 | Rough-cut e proposta-da-testo | asset id / timeline | proposta rough-cut + keep ranges + audit; `text_cuts.json` | entrambe legate alla firma della timeline | GUI(HTTP) | `src\web\server.py:1733-1757` |
| F22 | Progetti, job, media, timeline, document | path/cartella video | progetto, job, stream media, dipendenze, layout timeline, documento | creazione da file o cartella (`folder_path`), nome progetto, job history | GUI(HTTP) | `src\web\server.py:1651-1669,1722-1732,1781-1793,1882-1900` |
| F23 | Diagnostica, capacità, modelli, AI locale | — / modello scelto | stato servizi, capacità, modelli trovati, stato LM Studio/Ollama/OpenCode | `model_search_paths` scrivibili via POST | GUI(HTTP) | `src\web\server.py:1638-1650,1824-1847` |
| F24 | Selettori file/cartella nativi | prompt | path selezionati | dialog nativo Windows; macOS via osascript; altri OS errore | GUI(HTTP) | `src\web\server.py:1848-1881` |
| F25 | Migrazione provenienza score | sessione | rimozione score sintetici con backup, idempotente | — | CLI | `src\cli\main.py:2167-2169` |
| F26 | Utility pipeline | video/sessione | run parziale, elenco sessioni, stato backend, export manuale | `run` (probe→audio→transcribe), `sessions`, `backends`, `export --start/--end` | CLI | `src\cli\main.py:1996-2010,2025-2029` |

## 3. Sessioni e cache su disco

- Radice `sessions/<session_id>/`; identificativo derivato da data + nome pulito + primi 8 hex di sha256
  (deterministico) — `..\project_engine_architecture.md:43-44,102-103`.
- Contenuto per sessione: `session.json`, `metadata.json`, `audio.wav`, `transcript.json/.txt`,
  `candidates.json`, `editorial_selection.json`, `compilation_proposals.json`, `compilations/`, `clips/`,
  `vertical_clips/`, `proof_clips/`, `precomposed/`, `compilation_previews/` —
  `..\project_engine_architecture.md:104-118`. Non verificati sul disco reale (nessuna sessione letta).
- Cache aggiuntive osservate: `waveforms/<asset>.json` (`src\web\server.py:1802-1805`), `ai_proposals/`
  per rough-cut/testo/montage (`:1736,1769`), loop ambient 10 s memorizzato in cache
  (`src\media\renderer.py:63-73`).
- **Fingerprint / invalidazione**: preassemblato valido solo se `signature` coincide con timeline+libreria
  (`src\web\server.py:1677-1678`); stessa firma per la proposta da testo (`:1740-1741`); proposte montage
  legate a `size:mtime_ns` del media (`:1776-1778`); proposta editing legata a `draft_revision` (`:1695`).
- **Protezioni**: master video trattato read-only, mai spostato/rinominato/copiato
  (`..\project_engine_architecture.md:229`); trascrizione non sovrascritta senza `--force`
  (`:230`; `src\cli\main.py:1981,1994`); editorial e proposals con `--force` esplicito che scarta stato
  umano (`src\cli\main.py:2165,2178`); promozione `--effective` crea snapshot in `compilations/<id>.json`
  senza mutare la proposta (`:2200-2204`).
- Unica cartella `sessions/` = un'istanza server; il secondo avvio riusa quella esistente — `AGENTS.md:30`.
- Log in `runtime/` — `AGENTS.md:26`; `LEGGIMI - PROVA.txt:30`.

## 4. Impostazioni e default

- Config runtime `config\default_config.json`: durata short min 3 s / max 180 s (`:4-5`); audio 16 kHz mono
  PCM wav (`:6-11`); trascrizione backend `auto`, lingua `it`, modelli MacWhisper whisperkit large-v3 /
  small (`:12-20`); export `libx264` crf 18 preset `fast`, audio `aac` 192k, tolleranza durata 0.35 s
  (`:21-28`); layout completo (`:29-68`); `model_search_paths` vuoto (`:70`).
- Default CLI: lingua `it` ovunque (`src\cli\main.py:1980,1990,2009,2017`); compilation `--max-duration 180`
  (`:2128`); `--output-fps` default dichiarato 30 (`:2064-2065`); sottotitoli/waveform default `true` per
  render verticale compilation e preview proposta (`:2148-2149,2233-2234`), default "non specificato"
  (tri-state) su export-candidates (`:2067-2068`); `--background-mode` default dichiarato `blurred_source`
  (`:2060`).
- La pipeline finale **ri-definisce** i parametri di export, prevalendo sulla config: export scala 1.0 /
  crf 18 / preset `medium`, preview 0.5 / crf 28 / `ultrafast`; audio AAC 48 kHz stereo 320k; classe
  output 1080p (`MAX_LONG_SIDE=1920`) — `src\media\final_output.py:43-51`. Mismatch da riconciliare
  (crf/preset/bitrate) rispetto a `config\default_config.json:21-28`.
- La GUI può **riscrivere** `default_config.json` via POST `/api/model-search` (percorsi modelli) —
  `src\web\server.py:1836-1846`.
- Default GUI (stato iniziale pannelli, preset grafici, ecc.) NON VERIFICATO: static non letto.

## 5. Selezione/scoring e layout verticale

- **Score**: quattro dimensioni 0–1 `standalone`, `hook`, `value`, `closure` + `overall`; se `overall`
  manca viene calcolato **35/25/25/15** — `src\models\short.py:65-115` (pesi a `:108`);
  `..\project_engine_architecture.md:146-150`.
- `overall` fornito esternamente **non** viene ricalcolato (fiducia nel valore salvato) — `src\models\short.py:104-108`.
- **Categorie** (10, inclusa `unclassified` baseline): `real_work_case`, `behind_the_scenes`,
  `technical_observation`, `decision_in_context`, `problem_in_session`, `experiment`, `performance`,
  `human_moment`, `workflow_moment` — `src\models\short.py:18-29`; la mappa legacy traduce categorie
  vecchie (es. `mini_tutorial`) nei nuovi nomi (`:42-62`). La doc di architettura elenca 9 categorie
  senza `unclassified`: allineare le fonti.
- Ruoli editoriali `single/compilation_member/both` (`:31`); relazioni tra candidati `setup_for`,
  `payoff_of`, `follows`, `contrasts_with`, `same_problem`, `same_workflow` (`:33-40`).
- **KEEP segments** (tagli interni): `resolve_keep_segments(lo, hi, exclusions)` in `src\models\short.py`
  è dichiarata algoritmo autorevole e non duplicabile — `..\project_engine_architecture.md:155-156,226`.
  Range salvati come `excluded_ranges` con id stabile `cut_00N` (`src\cli\main.py:2100-2103`).
- **Layout verticale**: canvas 1080×1920, fps 30, sfondo `blurred_source`, allineamento `top_safe`,
  foreground y 110 e larghezza 1080 — `config\default_config.json:29-35`; `src\media\layout.py:124-137`.
- **Safe area**: left 70, right 200, top 80, bottom 350 → area utile 810×1490; dentro: sottotitoli,
  titoli, CTA e elementi vitali; possono sforare: sfondo blur, waveform, glow — `src\media\layout.py:9-48`.
- **Zone**: sottotitoli 760–1220 (alto 460), waveform 1280–1480 (alto 200) —
  `src\media\layout.py:60-101`; `config\default_config.json:43-50`.
- **Sfondo blur** (2 stadi): downscale 180×320, blur radius 12 power 3, brightness -0.35, contrast 0.60,
  saturation 0.75, tinta `#100726` opacità 0.50 — `config\default_config.json:51-61`; `src\media\layout.py:139-148`.
- **Livelli di composizione** (ordine): 1 sfondo (blur/dark/ambient procedurale), 2 video 16:9 intero
  senza crop, 3 titolo sezione opzionale, 4 sottotitoli ASS/libass, 5 waveform `showwaves` —
  `src\media\renderer.py:33-42`.
- Formati finali come **varianti dello stesso piano** (non nuove versioni): `original` (come il progetto),
  `16:9`, `9:16`; canvas base montaggio 1920×1080 — `src\media\final_output.py:39-48`.

## 6. Rischi / limiti e cosa NON replicare

1. **Master loudness**: le costanti -14 LUFS/-1 dBTP esistono (`src\media\final_output.py:52-54`), ma il
   docstring del modulo dice che il master (leveler/ducking/loudness/limiter) "non è ancora applicato"
   (`:13-15`) mentre la copia prodotto lo dichiara attivo (`VERSIONE.txt:2`). Contraddizione:
   effettiva applicazione **NON VERIFICATA** (richiede lettura oltre le prime 60 righe).
2. **Doppia copia su disco** (`ZP-Shorts-Studio-Stable` e `ZP Shorts Studio - prova`): rischio di
   modificare/leggere l'albero sbagliato. Confine di questo inventario: solo Stable.
3. **Config mutabile a runtime** via HTTP (`src\web\server.py:1838-1843`): i default spediti vengono
   sovrascritti; non replicare senza separare config utente e default di prodotto.
4. **Versioni incoerenti**: `0.1.0` in config (`config\default_config.json:3`) vs PBNL-1.0
   (`VERSIONE.txt:1`); la CLI si autodescrive "Local macOS engine" anche su Windows
   (`src\cli\main.py:1962`).
5. **Path assoluti macOS** nel default (MacWhisper `/Applications/...`, `config\default_config.json:16`) e
   progetti non portabili tra OS (`AGENTS.md:32`). Non replicare path hard-coded.
6. **Comandi distruttivi con `--force`**: re-trascrizione, import sottotitoli, editorial, proposals
   (`src\cli\main.py:1981,1994,2165,2178`). Qualsiasi port deve chiedere conferma e fare backup.
7. **Score `overall` non derivato**: valori esterni incoerenti coi pesi restano accettati
   (`src\models\short.py:104-115`). Per parità: preservare il valore salvato, non ricalcolarlo a sorpresa.
8. **Autorità della matematica Python**: session_id, KEEP segments, timeline math, subtitle remapping non
   vanno reimplementati altrove (`..\project_engine_architecture.md:220,226`). Rischio alto di divergenza.
9. **Generazione candidati rule-based** dichiarata, con punto di estensione LLM previsto ma non presente
   (`..\project_engine_architecture.md:213-214`). Non assumere qualità semantica dell'attuale scoring.
10. **Sicurezza perimetro**: bind solo 127.0.0.1 (`AGENTS.md:29`), ma il controllo `self.allowed(...)`
    (`src\web\server.py:1631,1814`) e la servitù statica (`:1807`) non sono stati ispezionati → livello di
    auth/host-check **NON VERIFICATO**. Da non replicare "fiducia implicita nel localhost" senza verifica.
11. **Dialog nativi per OS** (osascript / Win32) dentro il server (`src\web\server.py:1848-1881`): accoppia
    il motore alla GUI desktop; in un port va isolato.
12. **Sincronizzazione approvazione** dichiarata in `ShortCandidate.__post_init__`
    (`..\project_engine_architecture.md:228`): non vista nel range letto → **NON VERIFICATO**.
13. **Nessun test/esecuzione** eseguiti: comportamento runtime, errori ffmpeg, tempi e resa reale non
    verificati. Le durate "5–8 min finestre" (`..\project_engine_architecture.md:65`) e il limite 180 s
    (`config\default_config.json:4`) sono su piani diversi (finestra di analisi vs durata output): da
    riconciliare prima di una parità.
14. **Cosa NON replicare**: launcher/.app/`.cmd` e download di Python embedded (`AGENTS.md:8-11`);
    layout/UX del browser editor (static non letto); nomi categoria legacy (`src\models\short.py:43-52`);
    endpoint che riscrive la config; duplicazione di pipeline parallele (il modulo finale dichiara
    "no parallel pipeline", `src\media\final_output.py:1-3`).
