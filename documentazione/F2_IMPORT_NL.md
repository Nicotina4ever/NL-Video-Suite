# F2 — Import di un progetto NL in NL Video Suite

Data: 2026-11-21. Commit F1 di riferimento: `0845ee6a`.

## Comando

`File > Import NL Project…` oppure, da automazione:

```json
{"command":"file.importNlProject","params":{"path":"...\\project.json"}}
```

Il comando legge il `project.json`, converte e installa il risultato come progetto
**non salvato**; l'utente sceglie poi dove salvarlo in `.fcproj`. Se il progetto corrente ha
modifiche non salvate, il dialogo si ferma e chiede di salvarle o chiuderle prima.

## Mappatura

| NL | FilmCraft |
|---|---|
| `title` | nome progetto e sequenza |
| `assets[]` | item media, percorso `<project dir>/assets/<file_name>` (link, nessuna copia) |
| `asset.media.width/height/frame_rate` | `VideoStreamInfo`, frame rate sequenza |
| `aspect_ratio` pixel (`1080:1920`) o rapporto (`16:9`) | impostazioni sequenza |
| tracce `video` / `overlay` | tracce video V1, V2 |
| tracce `audio` | tracce audio dopo quelle dell'audio incorporato |
| clip video | clip video + clip audio collegata (se il media ha audio) |
| clip audio NL | clip audio sulla traccia NL |
| `trim_in_ms`, `start_ms`, `duration_ms` | `source_in`, `start`, `duration` (snappati al frame) |
| `volume` | effetto `volume` in dB |
| `transform.fit = "contain"` | `scale_to_frame` |
| `transcripts[]` | `Project::transcripts` per media; parole in tempo media |

Le tracce audio incorporate sono nominate `<nome video> audio`; le tracce NL mantengono il
nome originale (`video`, `overlay`, `audio`).

## Verifica GTA6

File: `sviluppo/anteprima_creativa/data/projects/Mi ficco in GTA6--3dd6fb69/project.json`.

Risultato dell'import:

- 1 media (video 3840×2160, 30 fps, 28:21), 0 media mancanti;
- V1 `video`: 9 clip, trim preservati;
- V2 `overlay`: 0 clip;
- A1 `video audio`: 9 clip collegate (`link`);
- A2 `overlay audio`: 0 clip;
- A3 `audio`: 1 clip NL;
- 1 trascrizione, **377 parole** sulla timeline (le parole dentro i segmenti tagliati);
- durata sequenza 4.476 frame (149,2 s a 30 fps).

Prove: `documentazione/prove/f2-import/import-verified.png` (a 40 s) e `import.png`.

## Limiti e rinvii

- I media restano dove sono: se la cartella NL viene spostata, i percorsi vanno ricollegati.
- Non vengono importati longform, shorts, sottotitoli, motion/social e gli altri documenti NL:
  restano nel progetto NL originale e saranno collegati nelle fasi F3–F6.
- La sync FilmCraft → mirror NL è rinviata a F3: le chiamate AI la richiedono e deve preservare
  le sezioni NL non note (longform, shorts, …), non ricostruirle. L'import conserva già
  `nlProjectDir` e `nlAssetId` nei metadati per farlo.
- Il transform avanzato NL (blur, focus, camera path) non è ancora mappato; per ora `contain`
  diventa `scale_to_frame` e gli altri valori restano nel progetto NL.
- Il clip audio NL che duplica l'audio di un clip video viene importato così com'è: NL lo mixa
  allo stesso modo, quindi il risultato è fedele anche se il segmento risulta più forte.
