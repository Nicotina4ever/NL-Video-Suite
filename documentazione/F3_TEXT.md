# F3 — Text panel: correzioni e mirror NL

Data: 2026-11-21. Commit F2 di riferimento: `6b7577a6`.

## Correzione inline

Nel pannello **Text ▸ Trascrizione**:

1. doppio clic su una parola → compare un campo di testo al suo posto;
2. `Ctrl+A`, scrivi la correzione;
3. **Invio** salva, **Esc** annulla;
4. il testo della timeline e la trascrizione del media si aggiornano insieme;
5. `Ctrl+Z` annulla la correzione (è un normale passo di undo FilmCraft).

Comando usato dal pannello (e disponibile da automazione):

```json
{"command":"transcript.editWord","params":{"item":3,"index":13,"text":"CORRETTO"}}
```

`item` è il media, `index` è l'indice della parola nella trascrizione del media (non nella
timeline). Il pannello lo ricava dal `SeqWord` mostrato, quindi la correzione resta corretta
anche se la parola è usata da più clip.

## Mirror NL

`File > Sync NL Text to Mirror` (o `file.syncNlText {path?}`) scrive una copia del
`project.json` originale con le trascrizioni correnti:

- default: `<cartella progetto NL>/project.synced.json`;
- con `path`: il percorso scelto;
- **non tocca** `project.json`;
- preserva longform, shorts, sottotitoli, audio, social, montage, story e ogni altra sezione;
- aggiorna solo `transcripts[].words` (testo e tempi in ms) e `language`;
- il percorso della cartella NL è conservato nei metadati della sequenza (`nlProjectDir`),
  l'id asset NL nei metadati del media (`nlAssetId`).

Prova end-to-end su GTA6:

- import, apertura Text, doppio clic sulla prima parola della timeline (`pensato`), sostituzione
  con `CORRETTO`, Invio;
- `transcript.inspect` mostra 377 parole di timeline e la prima è `CORRETTO`;
- `file.syncNlText` scrive 3.545 parole (trascrizione completa del media) in
  `documentazione/prove/f3-text/project.synced.json`;
- `longform`, `shorts`, `subtitles`, `audio`, `social`, `montage`, `story` risultano
  semanticamente identici all'originale; cambia solo la parola corretta;
- screenshot: `documentazione/prove/f3-text/text-panel.png`.

## Ancora da fare in F3

- Scelta motore ASR (Whisper FilmCraft / Qwen / WhisperX NL) nel pannello Text.
- Traduzioni EN/ES con scrittura nel mirror.
- Import del mirror dopo una correzione esterna e conflitti (oggi il mirror è one-way).
