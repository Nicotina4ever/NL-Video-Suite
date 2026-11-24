# Come avviare NL Video Suite (versione nuova, 2.0.0)

Questa è la versione **nuova** (`filmcraft-nl`), diversa dalla suite classica avviata dai vecchi
`.bat`. Non serve nessun installer per provarla.

## La superficie NL in un colpo d'occhio

- All'avvio scegli **Window ▸ Workspaces ▸ NL Video Suite** (oppure `Alt+Shift+9`, o il pulsante
  Workspaces nella barra): il layout mette a vista **NL Home** (tutte le funzioni con un clic), i
  pannelli di creazione, il Program, la timeline e i pannelli di finitura.
- Ogni pannello NL mostra in alto il progetto, la sequenza aperta (risoluzione/fps/playhead/selection),
  la trascrizione, il provider ASR e l'endpoint ComfyUI.
- **NL Transcript** gestisce trascrizione, import SRT/VTT, sottotitoli e traduzioni; **NL Clips AI**
  ha il reframe automatico con focus volti, stabilità/reattività e comportamento.
- Il reframe automatico usa il runtime della suite (`runtime/python` + OpenCV); senza runtime il
  risultato lo dichiara e usa il centro. Il bundle portabile non include `runtime/` né `models/`.

## Avvio rapido (consigliato)

1. Doppio clic su **`AVVIA-NL-Video-Suite-NUOVA.cmd`** nella cartella `K:\AI\NL_Video_Suite`.
2. Il launcher cerca automaticamente la build più recente in
   `distribuzione\NLVideoSuite-*-windows-x64\` e la avvia.
3. Se non parte, controlla che esista una cartella `distribuzione\NLVideoSuite-...-windows-x64`
   con dentro `NL Video Suite.exe` e `Avvia NL Video Suite.cmd`.

## Installazione per-utente (scorciatoia nel menu Start)

1. Doppio clic su **`INSTALLA-NL-Video-Suite-NUOVA.cmd`**.
2. Copia l'app in `%LOCALAPPDATA%\Programs\NL Video Suite`, crea le scorciatoie sul Desktop e nel
   menu Start (cerca “NL Video Suite”).
3. Per disinstallare: `packaging\nl\install.ps1 -Uninstall` (o rimuovi la cartella e le scorciatoie).

## Portabile a mano

- Cartella: `distribuzione\NLVideoSuite-<versione>-windows-x64\`
- Avvio con launcher: `Avvia NL Video Suite.cmd` (imposta `NL_SUITE_ROOT`, indispensabile per trovare
  `workflows\`).
- Avvio diretto: `NL Video Suite.exe` (funziona, ma il launcher è più sicuro).
- Include anche `NL Video Suite CLI.exe` per automazione/test:
  `NL Video Suite CLI.exe commands`, `NL Video Suite CLI.exe run script.jsonl`.
- Zip pronto da copiare su un altro PC Windows: `distribuzione\NLVideoSuite-<versione>-windows-x64.zip`.

## Al primo avvio

- **Impostazioni ▸ NL Video Suite**: indirizzo ComfyUI (default `http://127.0.0.1:8188`), provider
  del modello linguistico (Ollama o OpenAI-compatibile), provider ASR.
- **NL Settings ▸ Check services**: verifica che ComfyUI/provider rispondano.
- I progetti si aprono da `File ▸ Open`; le impostazioni stanno in `%APPDATA%\NL Video Suite`.
- Nessun modello viene scaricato automaticamente e nessun servizio AI viene avviato dalla suite.

## Differenza rispetto ai vecchi .bat

I `.bat` nella radice (`Avvia_NL_Video_Suite.cmd`, `Crea_Portable.cmd`, ecc.) sono della **suite
classica**. La versione nuova si avvia solo dai due launcher qui sopra o dal bundle in
`distribuzione\`.

## Eseguibile in radice (novità)

- **`NL Video Suite.exe`** nella radice `K:\AI\NL_Video_Suite` è la copia dell'ultima release: doppio
  clic per avviarla (usa i workflow del workspace).
- `AVVIA-NL-Video-Suite-NUOVA.cmd` sceglie automaticamente la build più recente in `distribuzione\`.
- Per l'analisi timeline con vision serve il server locale (vedi `DISTRIBUZIONE.md`, sezione LFM).