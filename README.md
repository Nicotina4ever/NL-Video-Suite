# NL Video Suite · editor locale con IA (in restauro verso 1.0)

Suite video in italiano: montaggio multitraccia, video lunghi→shorts, sottotitoli, motion graphics editabili, foto/video/musica, copioni e regia assistita con Ollama/API locali e ComfyUI. Versione interna 0.7.1: **non ancora release 1.0 certificata** — stato e limiti sono in `documentazione/STATO.md`, senza promesse.

Due interfacce, uno stato: la **UI completa classica** (`http://127.0.0.1:8088/`) e la **shell nuova stile Premiere** (`/shell/`) servita dallo stesso binario, stessa autorità e stessi dati.

- [Manuale d'uso](documentazione/MANUALE.md)
- [Stato e prossimo lavoro](documentazione/STATO.md)
- [Architettura](documentazione/ARCHITETTURA.md)
- [Specifiche complete e storia](documentazione/archivio/) · [Handoff sessione](sviluppo/roadmap/V1_SESSION_HANDOFF.md)
- [Prerelease Windows x64](https://github.com/Nicotina4ever/NL-Video-Suite/releases)

## Avvio

1. `Avvia_NL_Video_Suite.cmd` → Stabile o Anteprima (dati e porte separati, binari solo se certificati nei report).
2. FFmpeg necessario per media: nel PATH o `app/tools`, oppure `Configura_PC.cmd`. Nessun download automatico di modelli/runtime.
3. Shell: con server attivo apri `http://127.0.0.1:8099/shell/` (anteprima) o `http://127.0.0.1:8088/shell/` (stabile). Per sviluppare la UI: `sviluppo/tools/Avvia_Shell.ps1` (Vite, proxy sul backend; `npm run check|test:algebra|build`).

## Percorsi frequenti

- Lungo→shorts: Trascrizione (con durata reale e nota copertura) → candidati Shorts → approva → assembla 9:16 (confermato).
- Parole da rivedere: pannello Trascrizione → correzione manuale, oppure «L'IA propone correzioni» (proposte ancorate alla parola esatta; approvi tu una per una).
- Tradurre: pannello Trascrizione → Traduzione (lingua, copertura esplicita, editing, «revisionata», SRT).
- Montaggio in shell: V/N/R/Y/U strumenti, C coltello alla playhead, Space/J/K/L riproduzione, Ispettore per i valori della clip.
- RAM/VRAM sotto pressione: le richieste restano in coda; pulsante «Libera RAM/VRAM» solo su decisione tua (mai interruzioni di lavori altrui).
- Eliminare: Progetti e materiali → Elimina progetto… → inventario → conferma. Il nome progetto fa nome cartella: «Rinomina progetto…».

## Licenze

Codice suite MIT ([LICENSE](LICENSE)); distincta componenti in [licenses/THIRD_PARTY.md](licenses/THIRD_PARTY.md) — include le note di attribuzione per il pattern di algebra di montaggio studiato da FilmCraft (MIT/Apache-2.0) e l'inventario dipendenze Rust rigenerato. Marchi ArtCraft/FilmCraft, codice egui e codec di quel progetto non sono usati né distribuiti. La redistribuzione completa dei runtime/FFmpeg/modelli resta gate aperto per la 1.0.
