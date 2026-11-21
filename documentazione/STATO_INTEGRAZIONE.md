# NL Video Suite — stato integrazione (22/11/2026)

Build: `filmcraft-nl/target/release/filmcraft.exe` (release, ~80 MB).
Avvio: doppio clic, oppure `filmcraft.exe --control 9876` per l’automazione.

## Cosa è integrato e verificato

| Funzione | Dove | Prova |
|---|---|---|
| Identità NL, icona, palette, wordmark | Finestra, About, taskbar | `prove/f1-branding/` |
| Import progetto NL (`project.json`) | `File > Import NL Project…` | `prove/f2-import/import-verified.png` |
| Media, tracce video/overlay/audio, trim, audio incorporato, trascrizione | Import | GTA6: 9 clip video, 9 audio collegati, 377 parole |
| Correzione parola inline (doppio clic, Invio/Esc) | `Text > Trascrizione` | `prove/f3-text/text-panel.png` |
| Mirror testo NL (`project.synced.json`) | `File > Sync NL Text to Mirror` | 3.545 parole, sezioni NL intatte |
| Find Shorts locale | `Sequence > Find Shorts` | 2 short, marker, bin `AI Shorts` |
| Export short | `File > Export > Media` | 1080×1920, 30 fps, H.264/AAC, 30,25 s |
| Salvataggio/riapertura `.fcproj` | `File > Save As` / `File > Open` | marker, bin, transcript presenti |

Dettagli tecnici: `F2_IMPORT_NL.md`, `F3_TEXT.md`, `F4_SHORTS.md`.

## Flusso GTA6 verificato

1. Apri NL Video Suite → `File > Import NL Project…` → `Mi ficco in GTA6/project.json`.
2. Apri il pannello `Text`, tab `Trascrizione`: 377 parole sulla timeline.
3. Doppio clic su una parola, correggi, Invio. `Ctrl+Z` annulla.
4. `File > Sync NL Text to Mirror` (facoltativo) → `project.synced.json` accanto al progetto NL.
5. `Sequence > Find Shorts`: crea 2 sequenze in `AI Shorts` e 2 range marker sulla sorgente.
6. Apri uno short dal Project panel; il Text panel mostra le parole del taglio.
7. `File > Export > Media`: 1080×1920, H.264/AAC.

## Cosa manca (dichiarato, non finto)

- **ASR selezionabile Qwen/WhisperX**: richiede il bridge Python del motore NL; oggi funziona
  l’ASR di FilmCraft/Whisper e l’import della trascrizione NL.
- **Traduzioni EN/ES**: richiedono lo stesso bridge LLM.
- **Ranking LLM e hook verificabile** per Find Shorts: F4b; l’attuale è un’euristica locale
  dichiarata (`method: local-transcript-heuristic`).
- **Longform** come comando: non ancora esposto.
- **Pannello Generate/ComfyUI, copertina, autoprompter, MiniMax/AceStep**: F5/F6.
- **Portable con motore AI e launcher unico**: F8.

## Regole rispettate

- Nessun menu “NL” separato: comandi dentro i menu FilmCraft.
- Ogni funzione ha un comando headless, non solo un pulsante.
- Nessun dato inventato: i risultati dichiarano il metodo e i limiti.
- Media NL linkati in place, mai copiati/duplicati.
- Il progetto NL originale non viene sovrascritto: il mirror è un file separato.
