# Inventario — Prompter ComfyUI (sola lettura)

Data: 23/11/2026. Fonte: `K:\AI\Prompter_Comfyui` (letta, non eseguita, non modificata).
Scopo: capire cosa portare nella GUI di NL Video Suite e cosa unire al workflow doctor.
Evidenze: `LEGGIMI.md`, `PROMPTER.md`, `prompter/core/*`, `prompter/ui/*`, `workflows/`,
`tests/`. Ciò che non è provato è marcato **NON VERIFICATO**.

## 1. Identità

- **Prodotto**: GUI Tkinter standalone (un solo processo Python stdlib) che apre un workflow ComfyUI
  in formato API, espone i parametri reali, gestisce LoRA/reference, prepara una coda di varianti e
  invia a ComfyUI solo dopo preparazione e conferma (`PROMPTER.md:1-30`; `LEGGIMI.md:1-30`).
- **Avvio**: `AVVIA.bat` → `python -m prompter`, oppure runtime portabile `runtime/`; ComfyUI locale
  su `127.0.0.1:8188` (`LEGGIMI.md:7-15`).
- **Garanzie dichiarate**: solo standard library; i JSON in `workflows/` non vengono mai scritti;
  nessun upload/invio nei test; INVIA solo dopo PREPARA e conferma; runtime portabile senza torch
  (`PROMPTER.md` §Garanzie).
- **Config**: `config.json` (URL ComfyUI, cartella workflow, Ollama, `free_every`), salvabile dalla
  GUI (`LEGGIMI.md:20-26`).

## 2. Funzioni utili da portare

| ID | Funzione | Evidenza | Destinazione NL Video Suite |
|---|---|---|---|
| PR1 | **Verifica API**: legge `/object_info` e segna per ogni workflow le classi mancanti; risolve enum/intervalli reali per i parametri | `prompter/ui/app.py:913-935` (`verify_api`, `_api_done`), `prompter/core/comfy_client.py:47-48` | **Unita al workflow doctor**: `nl.workflows.verify` + `nl.workflows.diagnose` |
| PR2 | Workflow doctor: diagnosi statica + controlli live su classi, modelli e reference | (NL suite, `workflow_doctor.rs`) + PR1 | stessi comandi; il doctor usa `/object_info` quando disponibile |
| PR3 | Parametri per sezioni comprimibili con filtro, enum e limiti reali | `prompter/core/schema.py:177-263`, `ui/panel.py` | pannello NL Workflows / parametri nel pannello Genera (P9b) |
| PR4 | LoRA e reference: aggiunta/rimozione con ricucitura dei link e limiti per tipo | `prompter/core/surgery.py`, `PROMPTER.md` §Editing | P9b |
| PR5 | Coda di varianti (seed deterministici, tag univoci), PREPARA (nessun upload), INVIA con conferma, dedupe | `prompter/core/jobs.py`, `batch.py`, `PROMPTER.md` §Coda | pannello NL Queue / Generate varianti (P9c) |
| PR6 | Prompt AI: brief → proposte JSON validate → diff → applica (Ollama, contesto dei soli slot reali) | `prompter/core/prompter.py`, `PROMPTER.md` §Prompt AI | pannello Storia/Genera: `nl.prompt.enhance`/`nl.prompt.propose` (P9b) |
| PR7 | Super Prompter: brief → domande → piano → proposte validate | `prompter/core/guide.py` | P9b (facoltativo, dopo Prompt AI) |
| PR8 | Reference media: immagini via `/upload/image`, video/audio copiati in `ComfyUI/input/Prompter_ComfyUI/` **solo se loopback**, percorso relativo `Prompter_ComfyUI/<nome>` per VHS_LoadVideo/LoadAudioUI | `prompter/core/comfy_client.py:59-142`, `LEGGIMI.md:30-34` | **v2v/estensione video**: staging media in `graphics.generateComfy` (P9d) |
| PR9 | Storico `/history` in worker thread; `/free` ogni N generazioni e pulsante manuale | `prompter/core/history.py`, `PROMPTER.md` §Coda | pannello Coda / servizi (P9c, unione P6) |
| PR10 | Bindings sidecar `<stem>.bindings.json` (pin/override/preset) senza toccare il workflow | `prompter/core/bindings.py` | P9b (parametri salvati per workflow) |
| PR11 | Lint del grafo e `trace_driver` (chi guida un input) per non duplicare controlli | `prompter/core/graph.py` | doctor (P9a) |

## 3. Workflow copiati (libreria di partenza)

Vedi `workflows/prompter/PROVENANCE.md`: 8 JSON API (character sheet, Krea2, MiniMax Music,
r2v e v2v extend, Qwen reference). Copiati su richiesta esplicita; originali intatti.

## 4. Cosa NON portare

- La GUI Tkinter e il suo codice: la suite ha la GUI egui e i pannelli nativi.
- Il formato `coda.json`/`inviati.jsonl`: la suite ha già i job e la ricevuta; si porta il
  comportamento (varianti deterministiche, dedupe, prepara/invia) non il file.
- Il gating a due passi per l'utente: nella suite il pulsante Genera è già esplicito; resta la
  regola "nessun invio senza azione dell'operatore".

## 5. Unione richiesta: doctor + verifica API

Un solo comando (`nl.workflows.verify`) scarica `/object_info` (sola lettura) e un solo doctor
(`nl.workflows.diagnose`) fa i controlli statici **e** quelli live: classi installate, file modello
presenti negli enum dei loader, reference presenti, link rotti, prompt vuoti, dimensioni fuori
griglia, durate impossibili, difetti noti (LTX AV split) — con la riparazione deterministica
(`nl.workflows.fix`) che scrive una **copia** e non tocca l'originale. Nel pannello **NL Workflows**
i due pulsanti sono affiancati, come chiesto ("magari unire le due funzioni").
