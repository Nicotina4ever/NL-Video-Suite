# Handoff — NL Video Suite / FilmCraft

Aggiornato: 23/11/2026. Questo file serve a riprendere il lavoro in un'altra conversazione o
harness senza perdere stato. Il codice e le prove restano su disco; questo è il riassunto.

## Obiettivo

Una sola app (fork FilmCraft 0.20.1, `filmcraft-nl`) con l'esperienza NLE di FilmCraft e le
funzioni AI di NL Video Suite come comandi/pannelli nativi. Niente menu "NL" separato. UI viola /
magenta / ciano, icona e wordmark NL. Fasi F0–F8, ognuna testata e documentata.

## Stato verificato

| Fase | Stato | Prova |
|---|---|---|
| F0 fork/build | fatto | build release, test engine/UI |
| F1 branding NL | fatto | commit `0845ee6a` |
| F2 import progetto NL | fatto | GTA6, 1 media, 9 clip, 377 parole |
| F3a testo + mirror | fatto | commit `0133016c` |
| F4a Find Shorts locale | fatto | commit `ca54d717`, 2 short |
| F4b Find Shorts AI | fatto e verificato | `documentazione/prove/f4b-ollama/` (LFM, 2 short, 3,2 s) |
| F5 ComfyUI immagini | codice fatto, test reale in corso | `comfy.rs`, `documentazione/F5_COMFY.md` |
| F6 musica Ace Step/MiniMax | codice fatto, test reale in corso | `documentazione/F6_AUDIO.md` |
| F3b ASR/traduzioni | bridge in verifica | `documentazione/F3B_ASR.md` |
| F8 portable | da fare | — |

Ultimi test: `cargo clippy -p filmcraft-engine --all-targets -- -D warnings` pulito;
`cargo test -p filmcraft-engine` → **931 + 17 passati, 0 falliti, 3 ignorati**.
Build release con F5/F6 in corso al momento del handoff.

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

1. Attendere la build release, eseguire `f5-comfy/real_test.py`: verificare immagine importata e
   screenshot; salvare `receipt.json`.
2. Eseguire `f6-audio/real_test.py`: verificare audio importato, durata e forma d'onda; ricevuta.
3. Completare F3b: se il bridge `asr-check` passa, aggiungere `transcript.transcribeNl` con job
   in background e import della trascrizione; poi traduzioni EN/ES via Ollama.
4. F5 residuo: cover da frame (upload reference), reframe 9:16, guide, autoprompter.
5. F8: portable con runtime/modelli e launcher unico; regressione GTA6 completa.
6. Ottimizzazione finale: eliminare `filmcraft-nl/target/debug` ricreato dai test, svuotare
   `da_eliminare` vecchio, aggiornare le ricevute e lo spazio.

## Se si migra a un altro harness

Non è necessario per il codice: tutto è su disco e documentato. Se serve, usare come prompt:
"Leggi `K:\AI\NL_Video_Suite\documentazione\HANDOFF.md` e `TODO.md`; riprendi dal primo passo
non fatto. Non toccare i progetti utente, non sovrapporre Ollama e ComfyUI, testa e documenta
ogni fase."
