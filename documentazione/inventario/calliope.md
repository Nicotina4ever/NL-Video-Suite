# Inventario Calliope — studio sola lettura

Data: 2026-11-23 · Fonte: `K:\AI\Calliope` (nessuna esecuzione, nessuna modifica).
Tutti i percorsi sono relativi a `K:\AI\Calliope\`. Metodo: README + `docs/` + struttura, poi
solo i file chiave (entry point, router, schema DB, config, coda, export, client ComfyUI).
`docs/` contiene solo `changelogs/` (19 file `.md`); la cartella `docs/wiki/` citata in
`README.md:264` **non esiste su disco** → NON VERIFICATO.

## 1. Identità

- **Prodotto**: studio locale "storia → video": idea → storyline con beat/personaggi/luoghi →
  script per scena → rottura in clip (shot) → generazione video via ComfyUI locale → montaggio
  finale ffmpeg. Progetti in SQLite, media su cartelle, LLM OpenAI-compatibile (locale o remoto).
  Evidenza: `README.md:3`.
- **Backend**: FastAPI + uvicorn, Python 3.11, layout `src/calliope` (pacchetto installabile;
  `calliope-backend/pyproject.toml` presente). Versione **1.5.9** — `calliope-backend/src/calliope/__init__.py:7`.
- **Frontend**: SvelteKit in `calliope-web/` (`calliope-web/src/routes/**`); dev server 5173 con
  proxy `/api` → 8247 (`README.md:42-48`). In build pacchettizzata la SPA è servita dal backend
  con fallback `index.html` e `Cache-Control: no-store` per l'HTML (`calliope-backend/src/calliope/main.py:61-84`, `:158-160`).
- **Porte**: backend default `127.0.0.1:8247` (`.../calliope/config.py:160-161`), ComfyUI default
  `http://127.0.0.1:8188` (`config.py:188`), LLM legacy default Ollama `http://127.0.0.1:11434/v1`
  (`config.py:175-176`). CORS `allow_origins=["*"]` (`main.py:124`) → esporre solo su loopback.
- **Avvio**: `python -m calliope.main --host 127.0.0.1 --port 8247` (`main.py:165-179`); lo script
  locale `start.bat` usa il runtime Python embedded in `python\` e Node in `K:\ZZSitoWeb\nodejs`
  (`start.bat:6-7,19-20`). `start-bg.bat` avvia in background con log in `logs\`.
  Al boot: `migrate_db`, rebase dei path asset di un'installazione precedente, recovery delle
  sessioni agente orfane, avvio worker (`main.py:88-116`). Packaging PyInstaller via
  `calliope-backend/launcher.py` + `calliope.spec`.
- **Licenza**: MIT, Copyright (c) 2026 Benjiyaya (`LICENSE:1-3`) → riuso/port consentito con
  conservazione di copyright e notice.
- **Nota struttura**: `python/` NON è codice applicativo ma una distribuzione CPython 3.11 embedded
  (`python/python311.dll`, `python/python.exe`, `python/LICENSE.txt`).

## 2. Catalogo funzioni (parafrasi)

| ID | Funzione | Input | Output | Opzioni/default | Dipendenze | Superficie | Evidenza |
|---|---|---|---|---|---|---|---|
| F01 | CRUD progetti (crea/elenca/leggi/modifica/elimina) | titolo (alias `name`), idea, genere, tono, durata target, cover, stato | progetto JSON (+statistiche) | stato `draft`→`completed`; `ingest_mode` `builtin`/`external` | SQLite | GUI+API+CLI | `models/schemas.py:9-63`; `routers/projects.py:50-139`; `db.py:9-21` |
| F02 | Genera storyline (beat, cast, luoghi, oggetti) + CRUD | idea/genere/tono + sessione agente | righe `story_beats`, `characters`, `locations`, `items` | sostituzione del contenuto esistente | LLM, agente | GUI+API+CLI | `routers/story.py:251,455,507-769`; `db.py:23-64` |
| F03 | Genera script per scena | progetto + sessione agente | `scenes` (heading/azione/dialogo/durata) | replace di default; conteggio scene | LLM | GUI+API | `routers/scenes.py:122`; `db.py:66-81` |
| F04 | Rottura scena → clip (coverage) | scene o progetto, `guidance`, `clip_cap` | N `clips` per scena | durata clip ~5-10 s; 1 clip di default se non espansa | LLM coverage | GUI+API | `routers/scenes.py:137-178`; `db.py:89-108` |
| F05 | CRUD/reorder clip (shot list) | scena, descrizione, taglia, righe dialogo, durata, workflow | `clips` | ordine per `order_index` | SQLite | GUI+API+CLI | `routers/scenes.py:304-405`; `db.py:94-108` |
| F06 | Genera immagini asset per personaggio/luogo/oggetto | id entità, workflow, `input_values`, prompt | `portrait_path`/`sheet_path`/`reference_image_path` + job | `missing_only=true`, `asset_target="sheet"` | ComfyUI | GUI+API | `routers/assets.py:47`; `models/schemas.py:236-247` |
| F07 | Import/analisi/CRUD workflow ComfyUI | JSON in formato API | workflow + `input_schema`/`output_schema` | `strict_mode=true`, `prompt_profile` auto-suggerito | ComfyUI API-format | GUI+API | `routers/workflows.py:27-208`; `comfyui/profiles.py:18`; `db.py:112-126` |
| F08 | Genera video per clip | `clip_ids`/`scene_ids`, workflow, `input_values`, `prompts` | job `kind=video` | il prompt confermato vince su bozza/rewrite | ComfyUI, LLM (solo H3 rewrite) | GUI+API | `routers/jobs.py:64-80`; `models/schemas.py:256-264`; `agent/video_agent.py:960-962` |
| F09 | Anteprima prompt (HITL) | `clip_id`/`scene_id`, workflow, valori form, `force` | testo prompt + metadati (bozza, `based_on`) | `force=false` = compilazione deterministica | — | API/GUI | `routers/jobs.py:83-112`; `agent/video_agent.py:553-591` |
| F10 | Batch prompt per molti clip | lista clip, `only_missing`, `save`, `force` | prompt compilati, nessun render | default: solo compile | LLM solo con `force` | API | `routers/jobs.py:115-135` |
| F11 | Coda job: lista/stato/pausa/riprendi/retry/cancel | filtri progetto/stato, limite | job | `limit=100` max 500 | SQLite | GUI+API | `routers/jobs.py:38-58,194-224`; `queue/manager.py` |
| F12 | Export film | progetto | job `kind=export` → MP4 | 1080p, xfade 0.5 s, fps maggioritario, loudnorm | ffmpeg/ffprobe | GUI+API | `routers/jobs.py:138-174`; `export/runner.py` |
| F13 | Libreria media | tipo/filtri | elenco file asset + delete | — | filesystem | GUI+API | `routers/library.py:77,113` |
| F14 | Playground (generazione libera) | workflow, input arbitrari, upload immagine/video/audio | job + risultato allegabile a progetto | — | ComfyUI | GUI+API | `routers/playground.py:247-351` |
| F15 | Chat agente + sessioni + cancel | messaggio, progetto opzionale | messaggi, eventi, tool call | `agent_max_steps` | LLM + harness | GUI+API | `routers/agent.py:169-407` |
| F16 | Memoria agente (preferenze/convenzioni/correzioni) | scope global/project, testo | righe `agent_memory` | `use_count` aggiornato | SQLite | GUI+API | `routers/agent.py:420-493`; `db.py:231-243` |
| F17 | Skill agente (elenca/leggi file) | nome skill | file markdown | — | filesystem `skills_*` | GUI+API | `routers/agent.py:508-535` |
| F18 | AI Canvas (nodi/edge/workflow/artifact) | canvas, nodi, edge | grafo persistito | dedupe artifact per job | SQLite | GUI+API | `routers/canvas.py:317-931`; `db.py:181-229` |
| F19 | Build Scene (composizione 3D preliminare) | `scene_json`, gate Brief/Cut | composizioni + catture image/video | export lato server in H.264 | Three.js + ffmpeg | GUI+API | `routers/shots.py:218-623`; `db.py:245-264` |
| F20 | Settings: lettura/scrittura + probe LLM | profili LLM, URL ComfyUI, code, path, flag | config pubblica | vedi §6 | — | GUI+API | `routers/settings.py:92-227` |
| F21 | Eventi live (SSE) per UI/agente | canale | stream eventi | — | in-process bus | GUI+API | `routers/events.py:13`; `events/bus.py` |
| F22 | CLI autoring (`calliope-cli`) | file JSON/stdin, `--expect-hash` | scritture su progetto + audit | 32 comandi, 10 gruppi; niente delete, niente modello, niente media | SQLite | CLI | `README.md:207-248`; `cli/main.py:38-83`; `cli/read_ops.py:299-316`; `cli/write_ops.py:537-550` |
| F23 | Health check | — | `{status,version,dry_run}` | — | — | API | `main.py:144-146` |

## 3. Pipeline creativa e schema dati

Stadi (GUI): **Story → Assets → Script → Video** (+ **Film** per l'export) — `README.md:83-90`.
Flusso DB: `projects` → `story_beats` → `characters`/`locations`/`items` → (`scene_characters`) →
`scenes` → `clips` (1:N) → `jobs` → file in `data/assets/...` → export MP4 (`db.py:8-265`).

| Tabella | Campi principali | Note |
|---|---|---|
| `projects` | title, idea, genre, tone, target_duration, cover_path, status, continuity_json, `ingest_mode` (migrazione) | `db.py:9-21,395` |
| `story_beats` | project_id, order_index, title, description | `db.py:23-30` |
| `characters` | name, role, age, appearance, personality, portrait_path, sheet_path, consistency_prompt | `db.py:32-44` |
| `locations` / `items` | name, description, reference_image_path, consistency_prompt | `db.py:46-64` |
| `scenes` | beat_id, order_index, heading, action, dialog, duration_sec, workflow_id, env_image_path, location_id, video_path, video_settings_json | conserva il testo screenplay (`db.py:66-81`) |
| `scene_characters` | scene_id, character_id | ponte (`db.py:83-87`) |
| `clips` | scene_id, project_id, order_index, description, shot_size, dialog_lines_covered, duration_sec, workflow_id, clip_path, video_settings_json, chain_from_prev | unità renderizzabile e di timeline (`db.py:89-108`) |
| `workflows` | name, kind image/video, workflow_json, input_node_map, output_node_name, input_schema, output_schema, prompt_profile, is_enabled, strict_mode | `db.py:112-126` |
| `jobs` | project/scene/clip, kind, workflow_id, status, payload_json, output_paths_json, error, timestamps, retry_count | `db.py:128-143` |
| `cli_audit` | progetto, comando, before-images per riga | `db.py:401-420` |
| agent_* / canvas* / shot_* | sessioni, eventi, memoria, nodi/edge, composizioni | `db.py:145-264` |

- Impostazioni SQLite: WAL, `busy_timeout=10000`, foreign keys ON (`db.py:273-275`); migrazioni additive.
- Un progetto passa a `completed` quando i video risultano completati (`queue/worker.py:368-378`).
- `chain_from_prev` esiste in DB ma il concatenamento video è dichiarato "workflow-owned" in
  `README.md:163-165` (il flag non ha picker lato UI) → semantica residua NON VERIFICATA.

## 4. ComfyUI

- **Solo formato API** (`README.md:188`); i nodi editabili sono scoperti da tag nel titolo
  `(Input:role)` / `(Output:role)` (`comfyui/roles.py:8-31`), con alias canonici (prompt/positive,
  character/char/ref, location/env/…, seed, duration, image, video, audio) — `roles.py:14-31`.
- **Parse**: `parse_dynamic_inputs` elenca i nodi taggati con `kind`, `role`, `defaultValue`
  (`comfyui/parser.py:44-66`); tipi dedotti da una registry di `class_type`
  (`comfyui/registry.py:14-141`).
- **Mapping modelli**: nessuna mappatura di nomi checkpoint/modello. Il peso usato è quello
  referenziato nel JSON del workflow importato; Calliope patcha solo i widget dei nodi taggati,
  risolvendo il campo (text/value/int/float/image/audio/file) dalla registry
  (`comfyui/patcher.py:84-110`). Cambiare modello = cambiare/importare un altro workflow.
- **Riempimento**: precedenza default non vuoti → contesto per ruolo → extra del form (i valori
  utente vincono); slot `(Input:image)` multipli riempiti in ordine di node-id, che numera i
  `<Subject N>` (`comfyui/smart_fill.py:88-205`).
- **Strict mode**: rimuove dai nodi esposti prompt/ref "cablati" nel file, lasciando passare solo
  prompt LLM e riferimenti utente (`queue/worker.py:31-33,98`).
- **Trasporto**: upload media su `POST /upload/image` e riscrittura dell'input locale
  (`comfyui/client.py:71-231`), `POST /prompt`, polling `GET /history/{id}`, download `GET /view`;
  errori di validazione risaliti con nodo e messaggio (`client.py:17-52`).
- **Output**: preferisce i nodi `(Output:video|image)`; scarta gli echo dei loader
  (`client.py:295-342`).
- **Prompt profile** per workflow: `prose` (default) o `minimax_h3_ref` (`db.py:122`); per H3 il
  compile è deterministico in 6 sezioni, l'LLM interviene solo su richiesta esplicita
  (`agent/video_agent.py:553-591`; `README.md:151-161`).
- **Polling**: intervallo 2 s, timeout 1800 s configurabile, `0` = attesa illimitata; cancel
  interrompe anche il prompt su ComfyUI (`config.py:192-198`; `queue/worker.py:335-366`).

## 5. Montaggio / esportazione (ffmpeg)

Costanti e catena in `export/runner.py`: 1920×1080 (`:27-28`), fps = voto di maggioranza degli fps
dei clip (tie → più basso, nessun clip → 30) (`:74-88`), crossfade 0.5 s (`:30`).
Comando validato: normalizza ogni clip (scale + pad + setsar + fps + settb), aggiunge silenzio
`anullsrc` ai clip senza audio, concatena con `xfade=transition=fade` e `acrossfade` triangolare,
poi `format=yuv420p` DOPO l'ultimo xfade, loudness `loudnorm=I=-16:TP=-1.5:LRA=11,aresample=48000`,
encoding `libx264 -preset medium -crf 20`, audio `aac 192k`, `-movflags +faststart`
(`export/runner.py:154-247`). Clip singolo: stessa normalizzazione senza xfade (`:213-216`).
Destinazione `data/assets/<project>/export/<slug>-job<id>.mp4`; progresso da `-progress pipe:1`
(`out_time_us`), kill del processo supportato, ffmpeg/ffprobe richiesti su PATH (`:48-52,251-345`).
Formati in ingresso: qualunque cosa decodifichi ffmpeg; output solo MP4 H.264/AAC.

## 6. Settings e coda

Chiavi (Settings + schema update): `host`/`port`; `data_dir`, `assets_dir`, `db_name`; `agent_workspace_dir`,
`agent_shell_enabled=false`; LLM `llm_base_url`, `llm_model`, `llm_api_key`, `llm_profiles[]` con
`llm_active_id`, `llm_context_tokens`, `agent_llm_assignments` per ruolo; `comfyui_base_url`;
coda `queue_concurrency=1`, `queue_poll_interval_sec=2.0`, `queue_poll_timeout_sec=1800`,
`queue_max_retries=2`; agente `agent_max_steps=24`, `agent_hardening_prompt`, budget history;
`dry_run=false` (`config.py:152-249`; `routers/settings.py:50-78`). Persistenza: file
`calliope-backend/calliope_config.json` (esempio versionato, il file reale non va committato) —
`README.md:38`; `config.py:610` `save_config_file`.
Coda: stati `pending` → `running` → `done`/`failed` (+ `error="cancelled"`), `retry_count`,
`reset_stale_jobs` al boot riporta i `running` a `pending`, claim FIFO per `created_at`, pausa in
memoria, export pendenti superati da un nuovo export (`queue/manager.py:18-140`; `routers/jobs.py:151-159`).
`queue_max_retries` è salvato ma **nessun retry automatico è stato trovato**: solo retry manuale
via endpoint → comportamento effettivo NON VERIFICATO (`config.py:199`; `queue/worker.py`).
Incoerenza da sanare: i commenti del worker parlano di stato `cancelled`, mentre
`manager.cancel` scrive `failed`+`error="cancelled"` (`queue/worker.py:215`; `queue/manager.py:126-140`).
`dry_run=true` produce segnaposto (PNG/MP4) invece di chiamare ComfyUI (`comfyui/dry_run.py`).

## 7. Web UI

Route SvelteKit: `/` lista progetti, `/projects`, `/project/[id]` (4 stadi + Film), `/agents`,
`/playground`, `/canvas` + `/canvas/[id]`, `/build-scene`, `/library`, `/settings`
(`calliope-web/src/routes/**`). Azioni principali: Draft Storyline, Generate asset per entità,
Regenerate Script, Break into shots, Generate/Generate all con anteprima prompt, Export film,
coda con pausa/stop, Settings (LLM, ComfyUI, Agente, Coda, Workflows, Path), playground con
upload e "attach to project", canvas con nodi immagine/video/workflow, Build Scene con viewport 3D.
Interfaccia multilingua (7 lingue, selettore nell'header) — `docs/changelogs/README.md:13`.
Dettagli: `README.md:81-96`.

## 8. Rischi / limiti / cosa non replicare

- **Licenza MIT** consente il port, ma richiede attribuzione: non copiare testi/prompt (es.
  `agent/prompts.py`, 38 KB) senza notice; meglio riscrivere.
- **Nessuna autenticazione** e CORS aperto (`main.py:124`) → richiede bind su loopback/firewall.
- **Accoppiamento Windows**: `start.bat`/`setup.bat` puntano a `K:\ZZSitoWeb\nodejs` e al Python
  embedded (`start.bat:6-7`) → non portabile così.
- **ComfyUI accoppiato per titoli-nodo**: nessun ID fisso, ma serve disciplina sui tag; un
  workflow senza `(Input:prompt)` degrada su fallback deprecato (`README.md:196-201`).
- **Nessuna mappatura modello** e nessun auto-chaining video: da decidere esplicitamente nella suite.
- **Agente/harness molto grande** (orchestrator ~45 KB, 13 plugin, loop/log ~40 KB): non replicare
  in una suite NLE; semmai esporre solo scritture testuali tipo CLI.
- **Build Scene / AI Canvas** sono mondi a sé (Three.js, MediaRecorder) fuori dal perimetro NLE.
- **Doppio stato di cancel** e `queue_max_retries` inerte: non copiare tali incongruenze.
- **Single-writer SQLite** con WAL: ok per uso locale, non per multiutente.
- `docs/wiki/` mancante e alcuni changelog parlano di funzioni non ispezionate qui (chunking,
  steering, SSG) → NON VERIFICATE nel codice.
- Non replicare: dipendenza da `ffmpeg` di sistema senza fallback; `dry_run` come placeholder
  silenzioso (può mascherare errori se lasciato attivo).

## 9. Endpoint e CLI principali

Prefissi API (tutti sotto `/api`, `main.py:130-146`): `/projects`, `/projects/{id}/story`,
`/projects/{id}/scenes`, `/projects/{id}/generate-script|expand-clips`, `/projects/{id}/assets`,
`/projects/{id}/generate-assets`, `/workflows` (+`/analyze`), `/jobs` (+`/queue-status`,
`/pause`, `/resume`, `/projects/{id}/generate-videos`, `/preview-prompt`, `/batch-prompt`,
`/export`, `/{id}/retry`, `/{id}/cancel`), `/playground` (+`/uploads`, `/generate`, `/attach`),
`/library/media`, `/canvas`, `/shots`, `/agent` (+`/sessions`, `/memories`, `/skills`),
`/events` (SSE), `/settings` (+`/llm/models`, `/llm/test`), `/health`.
CLI `calliope-cli.bat`: gruppi `project story cast script clips context shots plan log schema`
(32 comandi; read: list/show/source/hash/get/order/next/validate; write: create/set/append/update/
upsert/set) — `cli/read_ops.py:299-316`, `cli/write_ops.py:537-550`. Garanzie: nessun verbo di
cancellazione, nessuna chiamata al modello, media read-only, `--expect-hash` con exit 3 su drift,
audit per riga (`README.md:223-242`).
