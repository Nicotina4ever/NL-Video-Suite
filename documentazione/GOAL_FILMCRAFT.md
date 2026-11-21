# GOAL — NL Video Suite 2.0 (fork di FilmCraft)

**Obiettivo:** NL Video Suite *è* l'esperienza FilmCraft: stessi menu, pannelli, timeline, mixer, export. Le funzioni NL sono comandi/pannelli nativi, mai un menu separato.

**Criterio di uscita:** GTA6 → trascrizione/correzione → short → reframe 9:16 + maschera → copertina → mix → export 1080×1920, in una sola app.

Regola: una fase = commit con build + test + prova GUI. Nessuna fase a metà.

## Board

### F0 - Build & baseline (COMPLETATO)
- [x] Fork locale `filmcraft-nl` da 0.20.1 (git pulito, 34 MB)
- [x] `cargo build --release -p filmcraft` (6m45s)
- [x] Avvio GUI + screenshot (`documentazione/prove/f0-filmcraft/`)
- [x] Test engine: 920 passati, 0 falliti, 3 ignorati
- [x] Test UI: 367 passati, 0 falliti, 1 ignorato

### F7 — Guardrail (COMPLETATO)
- [x] Via `guard_ram`/VRAM dall'ammissione LLM
- [x] Resta coda: 1 job pesante alla volta + attesa coda ComfyUI
- [x] ComfyUI offline = nessun blocco (non può esserci overlap)
- [x] Test: 64 MiB di RAM riportata non bloccano una richiesta LLM loopback; 193 test verdi

### F1 — Identità NL (COMPLETATO)
- [x] Nome/titolo/APP_ID = NL Video Suite; versione 2.0.0; About e menu nativi aggiornati
- [x] Icona NL (mic + waveform) da `LogoMioBozza.svg`: SVG, PNG, ICO, ICNS, hicolor
- [x] Palette NL in `theme.rs`: viola #7531fe, magenta #c42b86, ciano #8ff0ff
- [x] Wordmark NL chiaro/scuro con sidecar e righe ATTRIBUTION
- [x] Rimossi i marchi ArtCraft dal fork (file, link Discord e crediti di marca)
- [x] Metadati export (AAF/OMF/MXF/DPX/MCC/STL) e cartella dati = NL Video Suite
- [x] Build release (79,9 MB), avvio, screenshot About/UI e icona exe
- [x] Commit `0845ee6a`

### F2 — Ponte progetto (COMPLETATO)
- [x] Modulo `filmcraft-engine/src/nl_project.rs`: media, tracce, clip, trim, trascrizioni
- [x] Audio incorporato dei clip video ricollegato su tracce audio dedicate (link)
- [x] Comando `file.importNlProject {path}` (headless/automazione)
- [x] Voce `File > Import NL Project…` con dialogo e guardia modifiche non salvate
- [x] Verifica GTA6: 1 media, V1 9 clip, V2 overlay vuoto, 9 audio collegati, 1 audio NL, 377 parole sulla timeline, 0 media mancanti
- [x] Screenshot `documentazione/prove/f2-import/import-verified.png`
- [x] Commit `6b7577a6`
- [ ] Sync FilmCraft → mirror NL: spostato in F3, dove le chiamate AI lo richiedono; preserva le sezioni NL non note invece di ricostruirle

### F3 - Text panel (IN CORSO)
- [x] Trascrizioni NL importate e visibili nel Text panel (sequence transcript)
- [x] `transcript.editWord {item,index,text}` con undo/redo
- [x] Doppio clic sulla parola nel Text panel: modifica inline, Invio salva, Esc annulla
- [x] `file.syncNlText` scrive `project.synced.json` preservando longform/shorts/sezioni NL
- [x] Prova GTA6: `pensato` → `CORRETTO`, 377 parole timeline, 3.545 parole mirror, sezioni identiche
- [x] Screenshot `documentazione/prove/f3-text/text-panel.png`, commit `0133016c`
- [ ] ASR selezionabile (Whisper / Qwen / WhisperX)
- [ ] Traduzioni EN/ES

### F4 — Montaggio AI (IN CORSO)
- [x] `sequence.findShorts`: candidati da trascrizione, punteggio locale, confini su parola/silenzio
- [x] Nuova sequenza per candidato nel bin `AI Shorts` + range marker sulla sequenza sorgente
- [x] Copia tagli video/audio/caption con link e trim, undo in un solo passo
- [x] GTA6: 2 short (30,2 s e 29,7 s), 74 parole nel primo, 2 marker, seconda esecuzione no-op
- [x] Screenshot `documentazione/prove/f4-shorts/`, commit `ca54d717`
- [ ] Hook verificabile / ranking LLM (AutoClip) in F4b
- [ ] Longform (capitoli/pulizia) come comando

### F5 — Visual AI
- [ ] Reframe 9:16 + sfondo sfocato in Effect Controls
- [ ] Maschere social in View > Guide Templates
- [ ] Pannello Generate (ComfyUI) + copertina + autoprompter

### F6 — Audio
- [ ] Mixer NL nei pannelli audio
- [ ] Ducking/loudness in Essential Sound
- [ ] MiniMax Music / AceStep

### F8 — Consegna
- [ ] Portable NL Video Suite + motore AI
- [ ] Docs, licenze, regressione completa

## Registro

- F0: fork clonato; build avviato (`filmcraft-nl\build-f0.log`).
- F7: analizzati `resources.rs` (`guard_llm`, `guard_ram`, `check_comfy_queue`) e `asr/execution.rs`.
- Decisione: fork FilmCraft → NL Video Suite; nessun menu NL.
