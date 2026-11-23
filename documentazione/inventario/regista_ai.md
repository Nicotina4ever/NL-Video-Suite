# Inventario Regista AI — studio sola lettura

Data: 23/11/2026 · Fonte: `K:\AI\Regista_AI` (nessuna esecuzione, nessuna modifica).
Percorsi relativi a `K:\AI\Regista_AI\`. Metodo: `AGENTS.md` (1110 righe), `todo.md`, `task.md`,
`workflow_preferences.json`, e verifica puntuale nel codice dell'app `utility/RegistaAI/`
(`server.py`, `agents.py`, `config.py`, `workflow_engine.py`, `workflow_repairs.py`,
`voice_refs/`). Regola: comportamento parafrasato, nessun codice/wording copiato.

## 1. Identità

- **Prodotto**: agente/regista locale "UI-first" (dashboard web) per creare clip, asset, reel e
  storyboard; genera immagini/video con ComfyUI, voce con Qwen3-TTS, musica con ACE Step/MiniMax,
  e monta una timeline propria. Evidenza: `AGENTS.md:3-7`.
- **Stack**: server HTTP Python (`utility/RegistaAI/server.py`, ~420 KB, `do_GET`/`do_POST` con
  routing a stringa), UI HTML+Vanilla JS in `utility/RegistaAI/web/`, agent loop
  (`cli_agent.py`, `director_orchestrator.py`), motore workflow ComfyUI (`workflow_engine.py`),
  timeline engine (`timeline_engine.py`), progetti su JSON (`projects/<id>/config.json`,
  `timeline_engine.json`). Python di progetto: `K:\AI\Regista_AI\python_win\python.exe`
  (`todo.md:14`).
- **Configurazione**: variabili d'ambiente `REGISTA_LLM_MODEL` (default `qwen3.5:9b`),
  `REGISTA_LLM_PROVIDER` (`ollama`), `REGISTA_LLM_BASE_URL` (`http://127.0.0.1:11434`),
  `REGISTA_LLM_API_KEY`, `COMFYUI_URL` (`http://127.0.0.1:8188`), più
  `configs/agent.local.yaml` che sovrascrive (`AGENTS.md:11-18`). Nel codice:
  `config.py:10` COMFYUI_URL, `config.py:76-84` URL/modelli Ollama, `config.py:296-310`
  modelli Whisper, `config.py:328-507` risoluzioni/durate/algoritmi, `config.py:859-863` cache.
- **GPU**: un lock a livello di classe serializza **Ollama XOR Whisper XOR ComfyUI**
  (`AGENTS.md:305`); TTS acquisisce il lock e libera VRAM (`AGENTS.md:239-241`). ComfyUI è
  gestito dalla propria coda; l'app ha endpoint per liberare VRAM/ollama e per start/stop dei
  servizi (`server.py:3584-3673`, `AGENTS.md:51-52`).

## 2. Catalogo funzioni

| ID | Funzione (parafrasi) | Input | Output | Opzioni/default | Dipendenze | Superficie | Evidenza |
|---|---|---|---|---|---|---|---|
| R01 | Chat agente con tool (workspace, memoria, comfyui, regista) | messaggio, sessione | eventi token/tool/status, risposta | modelli e URL da env; sessioni JSON | LLM | API+UI | `AGENTS.md:23-30`; `server.py:2631-2682` |
| R02 | Sessioni agente (lista/leggi/crea/rinomina/elimina) | id | JSON conversazioni | indice in `conversations/index.json` | — | API+CLI | `AGENTS.md:243-248` |
| R03 | Storyboard (JSON: titolo, genere, stile, scene con prompt immagine/movimento, dialogo, durata, camera, mood, luce) | prompt/idea | storyboard + anteprima | fallback deterministico se l'LLM fallisce | LLM | API+UI | `AGENTS.md:44`; `agents.py:735-1282`; `server.py:4401-4445` |
| R04 | Generazione immagine (workflow ComfyUI, img2img/edit) | prompt, size, seed, reference | file importato in progetto | risoluzione alta proporzionata (~4K lato lungo, min 2K) | ComfyUI | API+UI | `AGENTS.md:45,98-119`; `agents.py:1282-1572` |
| R05 | Generazione video (I2V/T2V, multi-ref, audio ref) | prompt, immagine, durata, workflow | file video | durata forzata 5–8 s; lato corto ≥720; griglia 32 px | ComfyUI | API+UI | `AGENTS.md:46,104-119`; `agents.py:1871-2139` |
| R06 | TTS voce narrante con clone e controllo emozione | testo, voce (default `nicola`), emozione | audio | voci da `voice_refs/*.wav`+`.txt`; lock GPU | Qwen3-TTS | API+UI | `AGENTS.md:230-241`; `agents.py:2140-2300`; `voice_refs/` (13 file) |
| R07 | Musica/audio generativo ACE Step/MiniMax | prompt/lyrics, durata, turbo/base | audio | input iniettati su titoli nodo | ComfyUI | API+UI | `AGENTS.md:47-48,232-238`; `agents.py:2301-2391` |
| R08 | Analisi asset (trascrizione, emozione, prosodia) | file audio/video | JSON descrittivo | Whisper `medium` per analisi, `tiny` per dettatura; CPU int8 | faster-whisper, opensmile | API+UI | `AGENTS.md:49,131-152` |
| R09 | Visione immagine (descrizione, personaggi, ambiente, oggetti, colori, luce, camera, composizione, prompt ComfyUI) | immagine | JSON | modello vision Ollama | Ollama | API+UI | `AGENTS.md:43`; `agents.py:169-325` |
| R10 | Analisi scena → workflow/prompt consigliati | descrizione/frame | workflow, prompt immagine/movimento, camera, mood, durata | — | LLM | API+UI | `AGENTS.md:50`; `agents.py:2392-2454` |
| R11 | Reel analysis (trascrizione, argomento/keywords, energia, b-roll, suggerimenti) | media/audio | trascrizione, topic, picchi, suggerimenti | lock GPU per Whisper; cache in sessione | Whisper, Ollama | API+UI | `AGENTS.md:52,131-152`; `server.py:5615-5960` |
| R12 | Coda di generazione (`RenderQueue`) | job con `generate_type`, `workflow`, `clip_id`, aspect, durata | esecuzione raggruppata, output assegnato al clip | ordinamento per tipo+workflow | ComfyUI | API+UI | `AGENTS.md:84-96` |
| R13 | Workflow repair deterministico (AV split LTX, reference mancanti, modelli mancanti, preview rimpicciolite, risoluzioni assurde) | workflow JSON | varianti riparate + diagnosi | senza LLM; `deterministic_only` | — | API+UI | `AGENTS.md:56-70`; `workflow_repairs.py:38-456` |
| R14 | Diagnosi/autofix/install nodi mancanti/scan workflow | workflow | diagnosi, varianti, lista nodi | 409 "ComfyUI occupato" solo per LLM | ComfyUI | API+UI | `AGENTS.md:54-67`; `server.py:4006-4265` |
| R15 | Servizi: status/start/stop ComfyUI e Ollama, log, test path | servizio | stato/avvio | path configurabili; nessun kill globale | OS | API+UI | `AGENTS.md:51-52`; `server.py:3601-3673` |
| R16 | Diagnostica risorse: stato GPU/lock, libera VRAM, unload Ollama | — | stato/azioni | senza toccare generazioni in corso | GPU | API+UI | `server.py:2833-2847,3584-3600` |
| R17 | Catalogo workflow con preferenze per categoria | categoria | lista workflow + limiti | i 4 default in `workflow_preferences.json` | filesystem | API+UI | `workflow_preferences.json`; `server.py:2690-2764,3269-3336` |
| R18 | LoRA (scan/cache, indici LTX/image, selezione per scena) | cartella LoRA | alias/selezioni | cache persistente | filesystem | API+UI | `task.md:30-32,39-40`; `config.py:287-288` |
| R19 | Timeline engine (clip CRUD, trim, split, duplica, move, snap, restack, restore, export) | clip/azioni | timeline JSON + render | durata derivata dal media | ffmpeg | API+UI | `todo.md:97-138`; `server.py:6691-7330` |
| R20 | Reel pipeline: upload media/audio, frame extraction, generate-graphic contestuale, batch, render | media + contesto | clip/timeline/reel | contesto = clip + topic + progetto | ComfyUI, LLM | API+UI | `AGENTS.md:71-82,164-228`; `server.py:5468-6590` |
| R21 | Prompt skill (migliora prompt immagine/video) | prompt | prompt migliorato | niente modello/durata/AR nel testo | LLM/template | API+UI | `AGENTS.md:121-129`; `server.py:7433-7469` |
| R22 | Memoria/preferenze/todo agente | chiave/valore | file `.vimax/` | — | — | API/CLI | `AGENTS.md:128` |
| R23 | Project manager (crea/elenca/config/timeline/asset/delete) | progetto | JSON progetto | — | — | API+UI | `server.py:2876-3034,3674-3714` |
| R24 | Cache scan/clean | — | elenco/spazio | `CACHE_KEEP_DAYS=30` | — | API | `server.py:3035-3042`; `config.py:859-863` |

## 3. Risoluzione, durata, prompt (regole verificate)

- Immagini: nativo alto proporzionato all'aspect (~4K di area; 9:16 → 2160×3840), **niente
  upscale**, rete di sicurezza ≥2K lato lungo; override area con `REGISTA_IMAGE_MP`
  (`AGENTS.md:100-103`; `config.py:394-498`).
- Video: rispetto del rapporto entro 0,5%, lato corto ≥720, dimensioni allineate alla griglia
  **32 px**, durata forzata 5–8 s (`AGENTS.md:104-107`; `config.py:499-507`).
- Prompt: mai nome modello/durata/aspect/risoluzione nel testo del prompt (`AGENTS.md:126`).
- Limiti preferenze: `video_min_duration_sec 5`, `video_max_duration_sec 8`,
  `video_min_short_side 720`, `image_max_mp 2.1`, `vram_limit high`
  (`workflow_preferences.json:30-36`).

## 4. Workflow ComfyUI (categorie e default)

`workflow_preferences.json` elenca per categoria i workflow preferiti (ordine = priorità):
`image_edit` (qwen_edit_local, flux2_9b), `i2v` (LTX-I2V-1280x720-NoAudio, LTX23 v2,
LTX-2.3 Ultimate, LTX-2.3 Basic Custom Audio), `multi_i2v`, `i2v_audio`, `image_gen`
(z-image-turbo, ideogram4), `audio_gen` (ace_step1.5 xl sft, MiniMax Music), `t2v`
(`workflow_preferences.json:1-37`). L'installazione reale ha ~90 workflow in
`workflow_comfy/{image,video,audio,image_edit,analisi,autofix}`.
Regole verificate: i modelli dei workflow salvati non si sostituiscono se il file esiste; i text
encoder non si sostituiscono mai; modelli assenti = segnalazione (`AGENTS.md:110-119`).

## 5. Voci TTS installate (selezionabili, nessun download)

`utility/RegistaAI/voice_refs/` contiene 6 riferimenti audio + 6 trascrizioni:
`affail_indian_seed.wav`, `nicola_clone_op1..3`, `nicola_clone_standard(_noeffetti_mono|_stereo)`.
La voce di default è `nicola` (`agents.py:533,573`). Il TTS è Qwen3-TTS (`utility/Qwen3-TTS/`).

## 6. Impostazioni necessarie nella suite (mappatura)

| Impostazione Regista | Valore/default | Dove va in NL Suite |
|---|---|---|
| URL Ollama + modello per ruolo | `127.0.0.1:11434`, `qwen3.5:9b`, vision `qwen3.5:9b` | Impostazioni NL (LLM) |
| URL ComfyUI | `127.0.0.1:8188` | Impostazioni NL (ComfyUI) |
| Workflow per categoria | tabella §4 | Impostazioni NL + pannello Genera |
| Limiti video/immagine | 5–8 s, ≥720 lato corto, 32 px | profili risoluzione NL |
| Voce TTS + cartella reference | `nicola`, `voice_refs/` | pannello Voce + Impostazioni NL |
| Modello Whisper analisi/dettatura | `medium` / `tiny` | pannello ASR |
| Path servizi ComfyUI/Ollama | `K:\AI\ComfyUI`, porta 11434 | Impostazioni Servizi |
| Cache giorni | 30 | Impostazioni NL |

## 7. Rischi, limiti, cosa non replicare

- Server HTTP monolitico con routing a stringa e UI in JS: **non replicare**; esporre solo
  comportamento e comandi.
- Nessuna suite di test dichiarata (`AGENTS.md:290-294`): le verifiche sono manuali → in NL Suite
  ogni funzione deve avere test.
- GPU 8 GB: Ollama ~5 GB + WhisperX ~3 GB convivono a fatica; serializzare sempre
  (regola già recepita).
- Molti percorsi hardcoded (`K:\AI\ComfyUI\...`, `K:\AI\Regista_AI\...`): in NL Suite vanno in
  impostazioni, mai hardcoded.
- `409 ComfyUI occupato` solo per l'LLM; le riparazioni deterministiche no (`AGENTS.md:67`).
- Non copiare i prompt di sistema né le descrizioni italiane della UI: riscrivere.
