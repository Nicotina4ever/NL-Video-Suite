# TODO operativo — NL Video Suite

Aggiornato: 23/11/2026. Regola di scheduling: **mai Ollama e ComfyUI insieme**; per Ollama usare
un modello piccolo (`lfm2.5:latest` o `granite4.2:3b`), poi scaricarlo prima di un job ComfyUI.

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

- [ ] Client ComfyUI nel motore: submit `/prompt`, poll `/history`, download `/view`, import nel
      Project panel; coda unica e job in background.
- [ ] `Generate` immagine da prompt con workflow locale; scelta modello e seed.
- [ ] `Graphics > Generate Cover` da frame corrente + prompt.
- [ ] Reframe verticale 9:16 con sfondo sfocato in Effect Controls.
- [ ] Maschere/guide social in View > Guide Templates.
- [ ] Autoprompter: brief → prompt e varianti seed.
- [ ] Test con ComfyUI libero, salvataggio ricevute e screenshot.

## F6 — Audio

- [ ] Mixer NL: persistenza e ascolto reale.
- [ ] Ducking/loudness/limiter in Essential Sound.
- [ ] MiniMax Music / Ace Step da ComfyUI: workflow, download audio, import nel progetto.
- [ ] Test con traccia reale e verifica forma d'onda/durata.

## F8 — Consegna

- [ ] Portable con motore AI e launcher unico.
- [ ] Regressione GTA6 completa: import, testo, shorts, export, riapertura.
- [ ] Documentazione finale, ricevute e spazio sotto controllo.

## Manutenzione

- [ ] Tenere `da_eliminare` con ricevute; niente cache debug nel repo.
- [ ] Aggiornare `STATO_INTEGRAZIONE.md` a ogni fase.
- [ ] Non toccare i 3 progetti utente (GTA6, Pubblicità, Trailer).
