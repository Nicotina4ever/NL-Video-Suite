# Session Handoff — restauro 21 novembre 2026

Letto da soli, questo file + [Stato e roadmap operativa](../../documentazione/STATO_E_ROADMAP_1_0.md). La storia precedente (incluso il handoff del 19/11) resta nel git a partire dal tag `restore-base-20261121`; non copiarla di nuovo qui.

## Dove siamo

Piano di restauro in 5 fasi: **0 ✅ · 1 ✅ · 2 ✅ (backend) · 3a ✅ · 3b+ in corso · 4–5 alla fine**.

- **Fase 0**: lavoro certificato 19/11 committato (`96cc5d5`), gitignore per reference/materiali, un solo nome binario (`nl-video-suite.exe`); i vecchi EXE variante sono in `da_eliminare/exe-storici-20261121` (~115 MB reali, liberabili a svuotamento).
- **Fase 1**: `PUT /api/projects/{id}/title` — il nome della cartella segue il titolo (marcatore `.project-id` invariato, ricevuta doppia, refusing su lavori attivi e revisioni stale); GUI «Rinomina progetto…»; il bridge Qwen **riporta gli intervalli saltati come silenzio** (nuovi run e cache vecchie, senza re-inferenza) con nota cliccabile nel pannello; proposta sgrossatura etichettata come durata *dopo i tagli*; verificato sul progetto reale: sorgente 28:21.17, parlato fino a 28:13.17, copertura 99%.
- **Fase 2**: `index.db` SQLite **derivato** dentro ogni progetto (FTS5 parole + frasi, ricerca `GET /api/v1/projects/{id}/search` con tempi, contestualizzazione per segmento, prefissi `parola*`); fingerprint su project.json, ricostruzione a richiesta e alla partenza, mai autoritativo, mai resuscitato dopo cancellazione. project.json resta la fonte di verità; DB eliminato insieme al progetto.
- **Fase 3a**: shell nuova in `sviluppo/web-app` (Vite + Svelte 5 + TS, tema scuro stile editor): barra progetto con rinomina, rail workspace Alt+1…4, pannelli ridimensionabili da mouse e tastiera, status bar con servizi reali, command palette Ctrl+Shift+P su registry di comandi (pattern command-registry). Dev: `npm run dev` su 127.0.0.1:5173 con proxy al backend; build in `sviluppo/web-dist` (non ancora servito dall'EXE).

## Candidato corrente

`sviluppo/target/release/nl-video-suite.exe` — SHA `BABB164C…` obsoleto: ricertificato dal report mirato `20261121T120717Z` (longform_chapters + project_delete_browser + project_rename_browser **3/3 PASS**; scope: targeted, non regressione completa). Il launcher Anteprima lo riconosce (passa la condizione `browser` + SHA).

Verifiche verde su questo sorgente: `cargo fmt --check`, `cargo clippy --release --all-targets -D warnings`, **188 unit test**, `test_qwen_adapter.py` 4/4, 3 baseline mirati. Versione interna ancora 0.7.1: la promozione resta un atto deliberato.

## Prossimo lavoro verificabile

1. **3b — timeline canvas nella shell**: filmstrip da `/frame`, waveform da `/peaks`, trim con hit-test e snapping, shortcut Premiere-derived (J/K/L, I/O, Q/W, C, D) dal registry; il porting dei pannelli esistenti avviene uno alla volta, la UI classica resta autoritativa finché un pannello non è passato (link in fondo al rail).
2. **3c — pannello Longform nella shell** (trascrizione + Panoramica + ricerca FTS con click→seek) e Shorts.
3. Servire `web-dist` dall'EXE (file disk-served o embed) e decidere il canale di rilascio della shell.
4. **Fase 4**: consolidamento documentale (MANUALE/ARCHITETTURA/STATO come unici vivi, resto in `documentazione/archivio/`) + script `tools/Promuovi_Anteprima.ps1`.
5. **Fase 5**: ricertificazione mirata + regressione proporzionata + riordino finale; `da_eliminare` si svuota solo per decisione del proprietario.

## Regole vive del restauro

- Niente si butta: si archivia (git o `da_eliminare` con `PERCHE.txt`). Ogni unità finita = commit subito.
- Nessuna re-inferenza ASR/LLM non necessaria: riuso cache e raw; gli artefatti di prove autentiche costose (`v1-authentic-longform`, replay planner, confronti percettivi) **non si toccano**.
- Da filmcraft (MIT/Apache, in `reference_software/`, non distribuito): solo pattern, specifiche UX (`ui-parity.md`, `keyboard.md`) e funzioni pure con avviso di copyright; mai marchi, mai egui, mai codec.
- I test non devono dipendere dalla RAM libera del momento: dove il provider è simulato si dichiara `NL_SUITE_LLM_ADMISSION=off` nel test (guardia di ammissione solo per Ollama reale).
