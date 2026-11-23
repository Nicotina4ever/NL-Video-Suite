# TODO operativo — NL Video Suite

Aggiornato: 23/11/2026. Regola di scheduling: **mai Ollama e ComfyUI insieme**; per Ollama usare
un modello piccolo (`lfm2.5:latest` o `granite4.2:3b`), poi scaricarlo prima di un job ComfyUI.

## Punto di ripresa (leggere prima di tutto)

- F1–F4b sono fatti e verificati; F4b reale: `documentazione/prove/f4b-ollama/receipt.json`.
- F5/F6: codice e test unitari fatti (clippy pulito, engine 931+17 passati). Build release in
  corso; test reali ComfyUI da eseguire con `prove/f5-comfy/real_test.py` e
  `prove/f6-audio/real_test.py`.
- F3b: bridge ASR in verifica su 12 s (`da_eliminare/.../asr-check`); poi comando
  `transcript.transcribeNl`.
- Handoff completo: `documentazione/HANDOFF.md`.
- Ottimizzazione 23/11 già eseguita (~2,84 GB netti); alla fine eliminare la cache debug
  ricreata dai test e aggiornare la ricevuta.

## Fatto ora

- [x] Ottimizzazione spazio 23/11: ~3,9 GB di media di test completati, cache debug `sviluppo/target/debug`,
      anteprime rigenerabili e `__pycache__` rimossi; reference software originale spostato in
      `da_eliminare/2026-11-23-ottimizzazione/`; vecchio trash 21/11 eliminato.
      Ricevuta: `da_eliminare/2026-11-23-ottimizzazione/ricevuta.json`.
- [x] F4b diagnosi: il prompt con timestamp supera il contesto Ollama di default (4k); aggiunti
      `num_ctx: 16384`, `think: false` e anteprima della risposta negli errori.

## F4b — Find Shorts with AI (chiudere)

- [ ] Ricompilare release (in corso) e rilanciare il test reale GTA6 con `lfm2.5:latest`.
- [ ] Verificare job, hook, marker, bin `AI Shorts`, screenshot e assenza di modifiche al progetto NL.
- [ ] Se il modello non rispetta il JSON, ridurre la finestra a 500 parole e riprovare una volta.
- [ ] Salvare prova in `documentazione/prove/f4b-ollama/`, aggiornare `F4_SHORTS.md`, commit.

## F3b — ASR e traduzioni

- [ ] ASR selezionabile: WhisperX via `sviluppo/bridge/clipsai_bridge.py`, Qwen via
      `qwen_asr_bridge.py`; comando headless `transcript.transcribe` con lingua e provider.
- [ ] Scrivere la trascrizione nel progetto FilmCraft con timing di parola e metadati del motore.
- [ ] Traduzioni EN/ES con Ollama su modello piccolo; comando `transcript.translate` e scrittura
      nel mirror NL senza sovrascrivere l'originale.
- [ ] Test con voce sintetica o estratto GTA6; documentare limiti e tempi.

## F5 — Visual AI / ComfyUI

- [x] Client ComfyUI nel motore: submit `/prompt`, poll `/history`, download `/view`, import nel
      Project panel; job in background (`comfy.rs`, `graphics.generateComfy`).
- [x] `Generate` immagine da prompt con workflow locale; scelta workflow/seed/size.
- [ ] `Graphics > Generate Cover` da frame corrente + prompt (serve upload reference).
- [ ] Reframe verticale 9:16 con sfondo sfocato in Effect Controls.
- [ ] Maschere/guide social in View > Guide Templates.
- [ ] Autoprompter: brief → prompt e varianti seed.
- [ ] Test reale con ComfyUI libero, ricevute e screenshot.

## F6 — Audio

- [x] Comando `audio.generateMusic` con workflow Ace Step/MiniMax, durata e testi.
- [ ] Mixer NL: persistenza e ascolto reale (già presente la base FilmCraft).
- [ ] Ducking/loudness/limiter in Essential Sound.
- [ ] Test reale Ace Step, verifica durata/forma d'onda e ricevuta.

## F8 — Consegna

- [ ] Portable con motore AI e launcher unico.
- [ ] Regressione GTA6 completa: import, testo, shorts, export, riapertura.
- [ ] Documentazione finale, ricevute e spazio sotto controllo.

## Manutenzione

- [ ] Tenere `da_eliminare` con ricevute; niente cache debug nel repo.
- [ ] Aggiornare `STATO_INTEGRAZIONE.md` a ogni fase.
- [ ] Non toccare i 3 progetti utente (GTA6, Pubblicità, Trailer).
