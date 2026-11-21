# Integrazione dei 4 progetti e pulizia workflow

Data: 21/11/2026

## Sorgenti dichiarate

| Progetto | Ruolo | Stato nella suite |
| --- | --- | --- |
| `K:\AI\ClipsAI` | clipping, sgrossatore, ASR | **integrato**: Longform (trascrizione, pulizia, clip), Shorts, ASR Qwen/WhisperX |
| `K:\AI\Calliope` | storia → scene → shot → ComfyUI → montaggio | **in gran parte integrato**: Regia/Storia, scene e testi, generazione media, montaggio; da valutare character/location library e beat structure; **esclusa** la modellazione 3D della scena |
| `K:\AI\ZioPaolo Video` | ZP Shorts Studio | **integrato**: Shorts Studio (candidati, revisione, export) |
| `K:\AI\Prompter_Comfyui` | editor workflow, Prompt AI, Super Prompter, varianti, coda | **parziale**: editor input workflow e coda esistono; mancano Prompt AI/Super Prompter e varianti di seed guidate. Da integrare in modo contestuale (Copertina/Generazione), non come nuova scheda |

## Pulizia workflow (LTX)

- Regola: ogni workflow il cui percorso o nome contiene `ltx` (case-insensitive) è stato **spostato**, non cancellato.
- Destinazione: `K:\AI\NL_Video_Suite\da_eliminare\workflows-ltx-20261121\` (struttura originale preservata).
- Ricevuta: `da_eliminare\workflows-ltx-20261121\ricevuta.json` (45 file, 977.973 byte, data, regola, elenco).
- Dopo il rescan: catalogo **32 workflow**, **0 LTX**, **12 MiniMax/H3**, **3 Ace Step/Music**.
- MiniMax H3 mantenuti: Turbo T2V, Turbo I2V, R2V 2/3/4/5 ref, Extend, più varianti; audio: `MiniMax Music`, `audio_ace_step1_5_xl_sft`.

## Cosa manca per chiudere l'integrazione

1. **Prompter contestuale**: pulsante «Prompt guidato» nella Copertina e nella Generazione, che usa il brief e il parlato per proporre un prompt; opzione «3 varianti di seed»; galleria reference dagli asset del progetto. Niente nuova scheda.
2. **Calliope**: valutare biblioteca personaggi/luoghi e struttura a beat **solo se** l'uso reale li richiede; la generazione per scena esiste già.
3. **GUI**: la toolbar del Montaggio espone 17 pulsanti (Mixer, Comandi, grafiche, audio, segnaposto, versioni, dossier, ricarica…). Proposta: 5 azioni primarie sempre visibili (Importa, Esporta, Mixer, Comandi, Regola clip) e il resto in «Altri strumenti» + palette `Ctrl+Shift+P`. Da fare dopo un uso reale, come chiesto.

## Verifica end-to-end eseguita

Progetto usa-e-getta 9:16, clip 16:9 di 6 s con audio, `fit=blur`:

- export → **1080×1920, 30/1 fps, H.264 High / AAC 48 kHz stereo, 6,000 s, 180 frame, 2,77 MB**
- file in `projects/ZZ e2e export--…/exports/…mp4`
- progetto eliminato con ricevuta (7 file, 5.175.678 byte); i 3 progetti dell'utente intatti.

## Audit GUI (utente inesperto)

Visibili e funzionanti nel Montaggio: Esporta, 🎚 Mixer, ⌘ Comandi, Scollega audio/Ricollega, Waveform, Regola clip → Social 9:16, trascrizione con doppio clic, canvas 608×1080. Nel Social: Copertina + Copertina AI dal frame. Criticità: toolbar troppo densa (17 pulsanti) e alcune etichette tecniche; la palette aiuta ma non sostituisce una gerarchia visiva.
