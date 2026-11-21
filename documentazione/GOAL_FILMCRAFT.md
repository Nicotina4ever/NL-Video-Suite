# GOAL — NL Video Suite 2.0 (fork di FilmCraft)

**Obiettivo:** NL Video Suite *è* l'esperienza FilmCraft: stessi menu, pannelli, timeline, mixer, export. Le funzioni NL sono comandi/pannelli nativi, mai un menu separato.

**Criterio di uscita:** GTA6 → trascrizione/correzione → short → reframe 9:16 + maschera → copertina → mix → export 1080×1920, in una sola app.

Regola: una fase = commit con build + test + prova GUI. Nessuna fase a metà.

## Board

### F0 — Build & baseline (IN CORSO)
- [x] Fork locale `filmcraft-nl` da 0.20.1 (git pulito, 34 MB)
- [ ] `cargo build --release -p filmcraft` (in background)
- [ ] Avvio GUI + baseline test
- [ ] Se manca rete: vendor delle 44 dipendenze git

### F7 — Guardrail (COMPLETATO)
- [x] Via `guard_ram`/VRAM dall'ammissione LLM
- [x] Resta coda: 1 job pesante alla volta + attesa coda ComfyUI
- [x] ComfyUI offline = nessun blocco (non può esserci overlap)
- [x] Test: 64 MiB di RAM riportata non bloccano una richiesta LLM loopback; 193 test verdi

### F1 — Identità NL
- [ ] Nome/versione/About/icona = NL Video Suite
- [ ] Crediti FilmCraft (MIT/Apache) + `THIRD_PARTY`

### F2 — Ponte progetto
- [ ] Adapter `project.json` NL → progetto FilmCraft
- [ ] `File > Import NL Project…`
- [ ] Sync FilmCraft → mirror NL

### F3 — Text panel
- [ ] ASR selezionabile (Whisper / Qwen / WhisperX)
- [ ] Correzione inline, Invio/Esc, autosave
- [ ] Traduzioni

### F4 — Montaggio AI
- [ ] Longform/shorts come comandi in Clip/Sequence
- [ ] Risultati nel Project panel

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
