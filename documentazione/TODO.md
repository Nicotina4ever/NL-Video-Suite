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
- [x] Pulizia finale 23/11: `filmcraft-nl/target/debug` (13,77 GB), cache ASR e `__pycache__`;
      spazio K da 81,43 a **97,47 GB liberi** (+16,05 GB). Release aggiornata.
      Ricevuta: `da_eliminare/2026-11-23-ottimizzazione/ricevuta.json`.
- [x] F4b diagnosi: il prompt con timestamp supera il contesto Ollama di default (4k); aggiunti
      `num_ctx: 16384`, `think: false` e anteprima della risposta negli errori.

## F4b — Find Shorts with AI (chiudere)

- [ ] Ricompilare release (in corso) e rilanciare il test reale GTA6 con `lfm2.5:latest`.
- [ ] Verificare job, hook, marker, bin `AI Shorts`, screenshot e assenza di modifiche al progetto NL.
- [ ] Se il modello non rispetta il JSON, ridurre la finestra a 500 parole e riprovare una volta.
- [ ] Salvare prova in `documentazione/prove/f4b-ollama/`, aggiornare `F4_SHORTS.md`, commit.

## F3b — ASR e traduzioni

- [x] Bridge WhisperX verificato su 12 s reali: 42 parole, timing validi
      (`documentazione/prove/f3b-asr/receipt.json`).
- [x] Comando `transcript.transcribeNl` nel motore (job background, import nel progetto);
      compilazione/test da eseguire dopo F5/F6.
- [ ] Test del comando sul progetto demo e su GTA6.
- [ ] Traduzioni EN/ES con Ollama su modello piccolo; comando `transcript.translate` e scrittura
      nel mirror NL senza sovrascrivere l'originale.
- [ ] Documentare limiti e tempi.

## F5 — Visual AI / ComfyUI

- [x] Client ComfyUI nel motore: submit `/prompt`, poll `/history`, download `/view`, import nel
      Project panel; job in background (`comfy.rs`, `graphics.generateComfy`).
- [x] `Generate` immagine da prompt con workflow locale; scelta workflow/seed/size.
- [x] Accesso GUI: voci `Grafica > Generate with ComfyUI…` e `Audio > Generate Music with
      ComfyUI…` aprono dialoghi nativi (prompt, workflow, seed/durata); il pannello Text usa
      l'ASR NL quando il riconoscimento FilmCraft non è compilato.
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

## Parità NL completa (Regista AI + ClipsAI + Calliope + ZP Video)

Matrice: `documentazione/PARITA_FEATURE.md`. Ogni voce deve avere GUI e comando.

- [x] P1: impostazioni NL uniche (ComfyUI, Ollama, ASR provider/modello/device, workflow default,
      profilo risoluzione immagini 1×/2× e video 2/3–1/2 (720p/540p), GPU-first con fallback CPU).
      Fatto: categoria `nl` persistita in `preferences.json`, wiring comfy/ASR/shorts,
      risoluzione unica `nl/size.rs`, comandi `nl.*`. Prova: `documentazione/prove/p1-settings/receipt.json`.
- [ ] P2: pannello ASR completo + traduzioni EN/ES; device automatico CUDA→CPU con esito visibile.
- [ ] P2b: rimuovere “FilmCraft” da tutti i testi visibili e creare i pannelli NL dedicati
      (Genera, Musica, ASR, Clipping, Sgrossa, Storyboard, Coda, Impostazioni NL).
      Stato 23/11: 8 pannelli creati e apribili da Window; testi visibili ripuliti (About, link
      upstream, export progetto). Clipping/Sgrossa/Storia mostrano lo stato onesto “engine step
      P3/P4” finché i comandi non esistono; ASR e Coda funzionano già.
- [ ] P4b (richiesta operatore): **copertine AI/thumbnail** stile YouTube — frame scelto dalla
      timeline + contesto discusso nel video, workflow immagine selezionabile, import come cover
      del progetto. Da implementare dopo P3/P4 (serve l'upload reference a ComfyUI).
- [x] P3a: pannello Clipping (analisi, selezione, SRT normal/hormozi, render 16:9+9:16) e
      Sgrossa (piano tagli con motivi, avviso parlato, selezione pezzi, render, report) sopra il
      bridge ClipsAI esistente; comandi `nl.clipping.*`, `nl.sgrossa.*`, `nl.subtitles.srt`,
      `nl.pipeline.result`. Prova: `documentazione/prove/p3-clipping-sgrossa/receipt.json`.
- [ ] P3b: editor waveform con tagli manuali (piano modificabile), reframe 9:16 di un file reso,
      burn sottotitoli via ASS con PlayRes, anteprima del ritmo.
- [ ] P4: pannello Storyboard/Storia (Calliope/Regista) con image/video per scena e coda.
- [ ] P5: TTS Qwen3 e musica con voci/modelli già installati, selezione nella GUI.
- [ ] P6: workflow repair/autofix, diagnostica servizi, avvio/stop ComfyUI e Ollama.
- [ ] P7: verifica parità, portable, regressione GTA6, docs e ottimizzazione.

## F8 — Consegna

- [ ] Portable con motore AI e launcher unico.
- [ ] Regressione GTA6 completa: import, testo, shorts, export, riapertura.
- [ ] Documentazione finale, ricevute e spazio sotto controllo.

## Manutenzione

- [ ] Pulizia finale: dopo l'ultima release eliminare `filmcraft-nl/target/debug` (cache test,
      ~13 GB), `da_eliminare/.../asr-check/work` e i residui; aggiornare la ricevuta e lo spazio.
- [ ] Tenere `da_eliminare` con ricevute; niente cache debug nel repo.
- [ ] Aggiornare `STATO_INTEGRAZIONE.md` a ogni fase.
- [ ] Non toccare i 3 progetti utente (GTA6, Pubblicità, Trailer).
