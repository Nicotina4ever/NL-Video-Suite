# Handoff — NL Video Suite / FilmCraft

Aggiornato: 23/11/2026. Questo file serve a riprendere il lavoro in un'altra conversazione o
harness senza perdere stato. Il codice e le prove restano su disco; questo è il riassunto.

## Obiettivo

Una sola app (fork FilmCraft 0.20.1, `filmcraft-nl`) con l'esperienza NLE di FilmCraft e **tutte**
le funzioni AI di NL Video Suite come comandi/pannelli nativi. Niente menu "NL" separato, ma
menù e impostazioni NL dentro l'app. UI viola / magenta / ciano, icona e wordmark NL.

**Scope completo:** unione di Regista AI, Clips AI, Calliope, ZP Video e delle funzioni già in NL
Video Suite. Ogni funzione deve essere **raggiungibile dalla GUI** (menu, dialoghi, pannelli,
impostazioni), non solo da comando headless. I modelli già installati devono essere selezionabili;
nessun download obbligatorio.

Percorsi di riferimento (sola lettura, niente copia di codice/wording):
`K:\AI\ZioPaolo Video`, `K:\AI\Regista_AI`, `K:\AI\ClipsAI`, `K:\AI\Calliope`.
Matrice dettagliata: `documentazione/PARITA_FEATURE.md`.

## Stato verificato

| Fase | Stato | Prova |
|---|---|---|
| F0 fork/build | fatto | build release, test engine/UI |
| F1 branding NL | fatto | commit `0845ee6a` |
| F2 import progetto NL | fatto | GTA6, 1 media, 9 clip, 377 parole |
| F3a testo + mirror | fatto | commit `0133016c` |
| F4a Find Shorts locale | fatto | commit `ca54d717`, 2 short |
| F4b Find Shorts AI | fatto e verificato | `documentazione/prove/f4b-ollama/` (LFM, 2 short, 3,2 s) |
| F5 ComfyUI immagini | **verificato** | `documentazione/prove/f5-comfy/` (1024×1024) |
| F6 musica Ace Step/MiniMax | codice fatto, test da ripetere | `documentazione/F6_AUDIO.md` |
| F3b ASR | bridge verificato, comando pronto | `documentazione/prove/f3b-asr/`, `nl_asr.rs` |
| F3b traduzioni | da fare | — |
| GUI NL | dialoghi Generate/Music/ASR aggiunti; clippy pulito | `menu_dialogs.rs`, `text.rs` |
| Parità completa NL | da fare, matrice pronta | `documentazione/PARITA_FEATURE.md` |
| F8 portable | da fare | — |

Ultimi test (23/11/2026): clippy engine+UI pulito; UI 367 + 815 passati, 0 falliti;
engine ASR 2/2. Commit codice: **`48f2ab9e`** (F5/F6/ASR + dialoghi GUI).
Build release con GUI/ASR in corso al momento del handoff.
F6: workflow Ace Step completato (MP3 120 s, `prove/f6-audio/ace-step.mp3`); resta il retest
dell'import nell'app dopo il fix `seconds`.

## File chiave

- `filmcraft-nl/crates/engine/src/comfy.rs` — client ComfyUI, comandi
  `graphics.generateComfy` e `audio.generateMusic`.
- `filmcraft-nl/crates/engine/src/shorts.rs` — F4a/F4b, fix `think:false`/`num_ctx`.
- `filmcraft-nl/crates/engine/src/ollama.rs` — client Ollama.
- `filmcraft-nl/crates/engine/src/nl_project.rs` — import e mirror NL.
- `filmcraft-nl/crates/engine/src/transcript.rs` — testi e parole.
- `documentazione/TODO.md` — todos operativi.
- `documentazione/STATO_INTEGRAZIONE.md` — stato e limiti.
- `documentazione/F4_SHORTS.md`, `F5_COMFY.md`, `F6_AUDIO.md`, `F3B_ASR.md`.
- `da_eliminare/2026-11-23-ottimizzazione/ricevuta.json` — pulizia spazio.

## Comandi utili

```powershell
# build release
cd K:\AI\NL_Video_Suite\filmcraft-nl
cargo build --release -p filmcraft

# controlli
cargo clippy -p filmcraft-engine --all-targets -- -D warnings
cargo test -p filmcraft-engine

# app con canale di controllo
$env:FILMCRAFT_CONTROL_TOKEN=('f4' + ('0' * 62))
.\target\release\filmcraft.exe --control 9876

# test F5/F6 (app già avviata; ComfyUI libero, Ollama scaricato)
python K:\AI\NL_Video_Suite\documentazione\prove\f5-comfy\real_test.py
python K:\AI\NL_Video_Suite\documentazione\prove\f6-audio\real_test.py
```

## Regole operative

- Mai Ollama e ComfyUI insieme: usare un modello piccolo Ollama (`lfm2.5:latest`), poi
  `ollama stop` prima di un job ComfyUI.
- ComfyUI: coda gestita da ComfyUI; non uccidere job e non liberare modelli di altre app.
- Non toccare i 3 progetti utente (GTA6, Pubblicità, Trailer).
- Media NL linkati in place; il progetto NL non viene mai sovrascritto.
- Spostare in `da_eliminare` con ricevuta; niente cache debug nel repo.
- Non dichiarare fatto ciò che non è stato testato.

## Prossimi passi

1. Rifare il test F6 in un'app pulita (una sola istanza, controllo 9876 libero), con il job
   selezionato per id; verificare audio importato, durata e ricevuta.
2. Testare `transcript.transcribeNl` sul progetto demo (sample.wav 12 s) e su GTA6; poi traduzioni.
3. Compilare la release con i dialoghi GUI e verificare con screenshot:
   Grafica > Generate, Audio > Generate Music, Text > Transcribe; aprire i dialoghi e controllare
   che siano usabili senza JSON.
4. Impostazioni NL uniche (ComfyUI/Ollama/ASR/workflow) persistite.
5. Pannelli Clipping, Sgrossa, Storyboard/Storia, TTS, Coda: seguire `PARITA_FEATURE.md`.
6. Portable, regressione GTA6, documentazione e ottimizzazione finale (eliminare la cache debug
   ricreata e aggiornare la ricevuta).

## Se si migra a un altro harness

Non è necessario per il codice: tutto è su disco e documentato. Se serve, usare come prompt:
"Leggi `K:\AI\NL_Video_Suite\documentazione\HANDOFF.md` e `TODO.md`; riprendi dal primo passo
non fatto. Non toccare i progetti utente, non sovrapporre Ollama e ComfyUI, testa e documenta
ogni fase."
