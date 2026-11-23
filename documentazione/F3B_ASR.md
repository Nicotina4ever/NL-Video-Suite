# F3b — ASR e traduzioni NL

Data: 2026-11-23. Stato: verifica del bridge in corso.

## Obiettivo

Usare il motore ASR già presente nella suite NL senza riscaricare modelli:

- WhisperX (`engines/clipsai` + `models/asr/faster-whisper-large-v3`);
- allineatore italiano locale `models/alignment/italian-xlsr53`;
- runtime Python `runtime/python/python.exe`;
- bridge `sviluppo/bridge/clipsai_bridge.py` con `operation: "transcribe"`.

## Prova diretta del bridge

Estratto di 12 s da `ZMateriale/Reel Partorito Finito.mp4`, request:

```json
{"source_root":"engines/clipsai","input_path":"sample.wav","work_dir":"work",
 "operation":"transcribe","language":"it","device":"cpu"}
```

Variabili: `NL_ALIGNMENT_MODEL_PATH`, `CLIPSAI_WHISPER_MODEL`, `PATH` con `app/tools`.

**Esito:** 42 parole, 1 segmento, lingua `it`, timing di parola validi (es. `guarda
0.191-0.491`, `professionalità 1.192-1.852`). WhisperX ha emesso un warning per `torchcodec`
assente, senza impedire la trascrizione. Prova: `documentazione/prove/f3b-asr/receipt.json`;
log completo in `da_eliminare/2026-11-23-ottimizzazione/asr-check/`.

## Accesso GUI

- **Sequence > Transcript > Transcribe with NL ASR…** apre il dialogo con lingua e device.
- Nel pannello **Text**, quando il riconoscimento FilmCraft non è compilato ma il runtime NL è
  presente, il pulsante **Trascrivi** usa direttamente l'ASR NL.

## Comando

```json
{"command":"transcript.transcribeNl","params":{
  "item":9,"provider":"whisperx","language":"it","device":"cpu","wait":false}}
```

- richiede un media file nella timeline o nel Project panel;
- device `auto` (default): prova CUDA, poi ripiega su CPU una sola volta dichiarandolo;
- job in background, stesso modello di F4b/F5;
- al termine importa la trascrizione con timing di parola nel progetto FilmCraft;
- il progetto NL non viene modificato.

## Limiti

- Il modello WhisperX è multilingue; l'allineamento locale è solo italiano. Per EN/ES il bridge
  usa i tempi CTranslate2 e lo dichiara.
- Nessun download implicito.
- La trascrizione è una proposta: va riletta prima di usarle per sottotitoli o tagli.

## Traduzioni

Le traduzioni EN/ES useranno Ollama con un modello piccolo, su segmenti/frasi (non parola per
parola), e verranno scritte in una traccia separata o nel mirror NL; i tempi restano quelli
originali e vanno verificati.
