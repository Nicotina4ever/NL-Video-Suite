# Session Handoff — restauro 21 novembre 2026

Letto da soli, questo file + [Stato e roadmap operativa](../../documentazione/STATO_E_ROADMAP_1_0.md). La storia precedente (incluso il handoff del 19/11) resta nel git a partire dal tag `restore-base-20261121`; non copiarla di nuovo qui.

## Dove siamo

Piano di restauro in 5 fasi: **0 ✅ · 1 ✅ · 2 ✅ · 3a/3b(slices 1-5) ✅ · correzioni+traduzioni IA ✅ · R1 coda risorse ✅ · R2 libera-RAM ✅ · R3 catena GPU/CPU ✅ · R5 razor C ✅ · promozione stabile ✅ · 4 parziale · 5 in corso (real_playback a ComfyUI libero; Shorts/Sorgente/ispettore dopo)**.

- **Fase 0**: lavoro certificato 19/11 committato (`96cc5d5`), gitignore per reference/materiali, un solo nome binario (`nl-video-suite.exe`); i vecchi EXE variante sono in `da_eliminare/exe-storici-20261121` (~115 MB reali) e 41 cartelle di artefatti sintetici storici in `da_eliminare/test-storici-20261121` (~430 MB; le prove autentiche costose e i report citati dalla documentazione NON sono stati mossi).
- **Fase 1**: `PUT /api/projects/{id}/title` — il nome della cartella segue il titolo (marcatore `.project-id` invariato, ricevuta doppia, refusing su lavori attivi e revisioni stale); GUI «Rinomina progetto…»; il bridge Qwen **riporta gli intervalli saltati come silenzio** (nuovi run e cache vecchie, senza re-inferenza) con nota cliccabile nel pannello; proposta sgrossatura etichettata come durata *dopo i tagli*; verificato sul progetto reale: sorgente 28:21.17, parlato fino a 28:13.17, copertura 99%.
- **Fase 2**: `index.db` SQLite **derivato** dentro ogni progetto (FTS5 parole + frasi, ricerca `GET /api/v1/projects/{id}/search` con tempi, contestualizzazione per segmento, prefissi `parola*`); fingerprint su project.json, ricostruzione a richiesta e alla partenza, mai autoritativo, mai resuscitato dopo cancellazione. project.json resta la fonte di verità; DB eliminato insieme al progetto.
- **Fase 3 shell** (`sviluppo/web-app`, Vite+Svelte5+TS, tema scuro): top bar progetto + rinomina, rail workspace Alt+1…4, splitter con tastiera, command palette Ctrl+Shift+P; **timeline canvas live** (righello adattivo, zoom, snap 6 px, trim/movi con PUT revision-guard e coda anti-race, poster filmstrip, banda waveform Premiere-style dai peak interi con normalizzazione limitata); **monitor Programma** con barra visibile (⏮ ▶ ⏸ ◀︎◀︎ 1× 2×), timecode e clock dal media (native o proxy auto-preparato); **pannello Trascrizione**: parole paginato con durata reale, ricerca indice con contesto, **correzione manuale round-trip verificato sul GTA6 reale**, **proposte IA con accettazione singola** (anchor validate: old inventati scartati), **traduzione** a strati per lingua con copertura esplicita, editing, flag revisionato e SRT; riapertura automatica del progetto remembered.
- **Guardia risorse (aggiornata 21/11 pomeriggio)**: nessun rifiuto più — `guard_llm` e la risoluzione device ASR **mettono in coda** aspettando le risorse libere (poll 15s, deadline dichiarata 24h); `cpu_fallback` è dichiarato nel metadato quando Auto ripiega in CPU; il pulsante **Libera RAM/VRAM** (`POST /api/v1/resources/release`, conferma SVUOTA) scarica i modelli Ollama residenti e rilascia ComfyUI solo a coda vuota, senza mai terminare o interrompere nulla. Test con provider finto: `NL_SUITE_LLM_ADMISSION=off`.

## Candidato e canali

- Anteprima: `sviluppo/target/release/nl-video-suite.exe` SHA `ade48793…` certificato dal report mirato `v1-baseline/20261121T142020Z` (longform_chapters, project_delete_browser, project_rename_browser, transcript_assistant, asr_guided → **5/5 PASS**).
- **Stabile promosso 21/11 15:26**: `app/nl-video-suite.exe` = stessi byte `ade48793…`, ricevuta `app/ultima-promozione.json`, backup `app/stabile-backup/nl-video-suite-20261121-152600.exe` (61442816…). Smoke sul data-dir stabile reale: 3 progetti, thumbnail 200 jpeg, Range 206. Le 3 cartelle `data/projects` sono state migrate a nome leggibile con ricevuta (il vecchio binario non le avrebbe più trovate: la promozione era il passo successivo obbligato del piano).
- QA REA su `index.db` reale di GTA6: schema completo (meta/words/segments/chapters/decisions/words_search FTS5).
- fmt/clippy verdi, **193 unit test**, svelte-check 0/0.

## Prossimo lavoro verificabile

1. **Fase 3c**: pannello Shorts e ispettore clip nella shell (trasformazioni/volume/fade); porting sottotitoli; quindi sorgente-monitor con media proprio.
2. Esportare `web-dist` servito dal binario (o flag) per l'uso senza Node; al momento la shell gira con `sviluppo/tools/Avvia_Shell.ps1` (proxy al backend 8099).
3. **Fase 4 residua**: consolidamento completo documenti (archiviazione dei V1_* non più vivi) e prova reale della correzione IA con Ollama acceso su materiale autentico.
4. **Fase 5 residua**: regressione proporzionata sul candidato congelato + accettazione UX umana + eventuale promozione con `tools/Promuovi_Anteprima.ps1` (solo decisione del proprietario; chiede "PROMUOCI").

## Regole vive del restauro

- Niente si butta: si archivia (git o `da_eliminare` con `PERCHE.txt`). Ogni unità finita = commit subito.
- Nessuna re-inferenza ASR/LLM non necessaria: riuso cache e raw; gli artefatti di prove autentiche costose (`v1-authentic-longform`, replay planner, confronti percettivi) **non si toccano**.
- Da filmcraft (MIT/Apache, in `reference_software/`, non distribuito): solo pattern, specifiche UX (`ui-parity.md`, `keyboard.md`) e formule di interazione (hit-test 7 px, zona waveform 42 %, normalizzazione con tetto); nessun marchio, egui, codec o media incluso.
- Le correzioni IA sono proposte validate contro il testo esatto mostrato; l'applicazione passa SOLO dal layer editoriale esistente. Le traduzioni restano bozze finché l'umano non le segna revisionate.
