# MANUALE — NL Video Suite (uso quotidiano)

Documento vivo. Stato tecnico e punto di ripresa: `STATO.md`. Dettagli storici: `documentazione/archivio/`.

## Avvio

- **Doppio clic su `Avvia_NL_Video_Suite.cmd`** → scegli **Stabile** o **Anteprima**. Le due istanze usano binari e dati separati (`data/` vs `sviluppo/anteprima_creativa/data`, porte 8088/8099). Il launcher avvia solo binari con hash certificato nei report; non compila nulla.
- **Interfaccia nuova (shell)**: a serveur attivo apri `http://127.0.0.1:8099/shell/` (o 8088/shell) — timeline canvas, monitor Programma con riproduzione reale, Trascrizione con correzioni e traduzioni, Shorts, ispettore. Per lo sviluppo UI: `sviluppo/tools/Avvia_Shell.ps1` (Vite con proxy; senza Node usa il `/shell/` servito dal binario).
- `Ferma_NL_Video_Suite.cmd` ferma solo il server della suite (mai servizi altrui).

## Concetti in due righe

Un **progetto** ha una cartella col **nome leggibile che segue il titolo** (`Mi ficco in GTA6--3dd6fb69`): rinomina da Progetti → «Rinomina progetto…» e la cartella lo segue (marcatore `.project-id` interno, mai i file). «Elimina progetto…» mostra l'inventario, chiede il titolo come conferma e non tocca originali né modelli.

## Montaggio (shell)

- **Timeline**: rotella scorri, Ctrl+rotella zoom, click = playhead (con snap), click clip = selezione. Strumenti V/N/R/Y/U o toolbar: Taglio, Ripple (il seguito si avvicina), Roll (sposti il taglio, totale invariato), Slip (finestra sorgente mobile), Slide (i vicini assorbono). C = coltello alla playhead. Frecce = nudge 1 frame (Shift = 5).
- **Programma**: ⏮ ▶/⏸ ◀︎◀︎ 1×/2× oppure Space/J/K/L; timecode in basso; usa il proxy nativo del motore (preparato da solo).
- **Ispettore**: etichetta, inizio/durata/trim, volume, opacità, fade, inquadratura (contain/cover/blur). Ogni scrittura passa da una PUT con guardia di revisione: conflitti rifiutano e ricaricano, mai sovrascrivono.
- Il montaggio multitraccia completo, i titoli Motion e l'audio restano disponibili anche nella **UI classica** (`/`), che rimane autoritativa finché la shell non copre tutto.

## Trascrizione e parole

- «Trascrivi e allinea» (UI classica) o pannello **Trascrizione** (shell): durata reale del sorgente, fine del parlato, nota se il file ha coda senza testo, intervallo della pagina.
- **Correzioni manuali**: click parola → testo → Applica. Il raw ASR non si tocca mai; i tempi corretti restano marcati «tempi da verificare».
- **Revisione assistita**: il provider (Ollama/API configurabile in Sistema) *propone* correzioni citando la parola esatta; scegli tu quali approvare. Se il provider è spento o la coda GPU è occupata, la richiesta resta in attesa con messaggio chiaro (nessun rifiuto secco, nessun fallback finto).
- **Traduzioni**: per lingua (en/es/fr/de/pt/nl/pl), copertura esplicita dei segmenti, editing manuale, flag «revisionata», export SRT. Bozza finché non la approvi.
- **Ricerca**: indice SQLite per progetto (derivato, ricostruito da solo): tutte le occorrenze con tempi e frase contesto, prefissi `parola*`.

## Shorts

Pannello **Shorts** (shell) e area Social (classica): candidati da trascrizione/capitoli/highlight, approva uno per uno, scelta manuale, assemblaggio 9:16 con conferma esplicita (sostituisce il montaggio, non i file).

## Risorse e GPU

- Nessuna richiesta IA viene più rifiutata per RAM/VRAM occupata: resta **in coda** e parte quando le risorse si liberano.
- Pulsante **«Libera RAM/VRAM»** (top bar shell, conferma esplicita): scarica i modelli Ollama residenti e, *solo se la coda ComfyUI è vuota*, rilascia la memoria ComfyUI. Mai interrompere lavori in corso, mai terminare processi esterni.
- ProfiloRisorse in Sistema: CPU-first / Bilanciato / GPU-preferred. Auto = GPU quando libera, fallback CPU dichiarato. NPU esposto solo se presente davvero.

## Serve altro?

Tutto ciò che la shell non ha ancora (sottotitoli avanzati, Motion, mixer, regia, workflow ComfyUI) è nella UI classica `/` — stessa autorità, stessi dati, nessun doppio stato.
