# Reframe automatico — valutazione dei tre repository e decisione

Data: 24/11/2026. Scopo: decidere che cosa prendere dai tre repository indicati per il reframe
automatico di NL Video Suite, senza copiare codice di terzi e senza aggiungere dipendenze o
download. La valutazione usa la licenza dichiarata dall'API GitHub, il README e la struttura dei
file; nessun archivio o eseguibile dei repository è stato scaricato o eseguito.

## Esito in una riga

Nessun codice dei tre repository entra nella suite. Il reframe automatico usa la **camera path
nostra** della vecchia NL Video Suite (`camera-cpu-v3`), portata nel motore nuovo; i repository
confermano le stesse scelte algoritmiche (face tracking, scene cut, EMA/damping, target lock), ma
due sono MIT senza un'implementazione utilizzabile e uno non ha una licenza valida.

## Tabella

| Repository | Licenza (API GitHub, 24/11/2026) | Algoritmo | Integrabilità | Decisione |
|---|---|---|---|---|
| `obi19999/smart-video-reframe` | **MIT** (`license.spdx_id = MIT`) | Python; YOLOv8 face tracking, scene detection, EMA, crop cover/blur; README con link a un ZIP dentro `src/utils` | Richiede `ultralytics`/modelli e FFmpeg; il README è una pagina di download generica, non documentazione tecnica | **Non si copia codice.** Solo comportamento: face tracking + scene cut + smoothing. Il README con archivio scaricabile è trattato come contenuto non fidato: non eseguire. |
| `x-eight/smart-video-reframe` | **MIT** | Scheletro Python (`src/cli.py`, `src/config.py`, `src/detectors`, `src/reframer`, `src/services`, `models/` vuoto); 14 KB totali | Incompleto: nessun modello, nessuna pipeline eseguibile | **Non utilizzabile come implementazione.** Conferma solo la struttura a detector + reframer. |
| `dapaaa07/Cliper-AI` | **Nessuna licenza** (`license = null`; il README dice «MIT» ma nel repository non c'è un file `LICENSE`) | Laravel/PHP + Python; doppio passaggio, MediaPipe con fallback OpenCV Haar, target lock + hysteresis, lost-face hold, dead-zone, EMA, sottotitoli ASS dinamici | Stack web Laravel + Python + API Groq; non integrabile e non riutilizzabile senza licenza | **Solo specifica funzionale**, nessun codice. Le idee utili sono già nella camera path nostra. |

URL verificati:

- https://api.github.com/repos/obi19999/smart-video-reframe
- https://api.github.com/repos/x-eight/smart-video-reframe
- https://api.github.com/repos/dapaaa07/Cliper-AI
- https://raw.githubusercontent.com/obi19999/smart-video-reframe/main/README.md
- https://raw.githubusercontent.com/dapaaa07/Cliper-AI/main/README.md

## Che cosa è stato portato davvero

Non i repository, ma la **camera path della vecchia NL Video Suite**, che è codice nostro e non di
terzi:

- `sviluppo/bridge/camera_path.py` (`camera-cpu-v2/v3`) → port in
  `filmcraft-nl/crates/engine/src/nl/reframe_camera.py`, incorporato nel motore con
  `include_str!` e scritto nella cartella di lavoro a ogni analisi.
- Campionamento ogni 250 ms, Haar cascade OpenCV già presente nel runtime, scene cut su
  thumbnail + istogramma, target lock con hysteresis, gestione volti multipli/mancanti, dead-zone,
  damping critico con limiti di velocità/accelerazione, semplificazione adattiva, modalità
  `composed` con `preferred_fit` cover/blur.
- Il motore Rust mappa i punti normalizzati in offset di crop e li applica come **keyframe** del
  parametro `motion.position` sulla clip reframed; il risultato dichiara punti, keyframe,
  copertura, scene cut, `preferredFit` e avviso.

Differenze rispetto ai repo:

- niente YOLO/MediaPipe: si usa il rilevatore Haar del runtime esistente, nessun download;
- niente Python di terzi: lo script è nostro, con la stessa provenienza della vecchia suite;
- niente cloud/API key: tutto locale;
- il crop non certificato resta un avviso, non un successo silenzioso.

## Prova live (24/11/2026)

Progetto GTA6 in copia (`documentazione/prove/f4-shorts/gta6-shorts.fcproj`), item 3, 0–6 s,
target verticale 1080×1920:

| Modalità | Punti | Keyframe | Offset x | Note |
|---|---|---|---|---|
| `composed` (default UI) | 2 | 2 | −797,33 px | camera composta: inquadratura quasi fissa, `preferredFit: blur`, avviso di rivedere |
| `legacy` (confronto) | 63 | 63 | −797,33 px | traiettoria continua, x da −527 a −257 px |

La sequenza creata è 1080×1920, il clip è posizionato e i keyframe sono presenti nel progetto
salvato. Il test Rust `nl::reframe::tests` copre parsing, clamping e offset; il test live è
ripetibile con `filmcraft-cli` (vedi `documentazione/prove/p21-reframe/`).

## Regole rispettate

- Nessun codice o asset dei tre repository è stato copiato.
- Nessun archivio/ZIP scaricato o eseguito.
- Nessuna nuova dipendenza nel `Cargo.toml`.
- La licenza di `Cliper-AI` è trattata come assente: il README non sostituisce un file `LICENSE`.
- La camera path portata è codice del progetto NL, non di terzi; la provenienza è citata nel file.
