# Distribuzione multi-OS e provider AI modulari

Obiettivo: poter **avviare NL Video Suite su Windows, Linux e macOS**, ricreare tutto dai sorgenti
con script/installer, e **non dipendere da un provider AI specifico**: Ollama, llama.cpp, LM Studio,
vLLM o qualunque server compatibile con l'API OpenAI si scelgono dalle impostazioni. Niente modelli
obbligatori e niente download automatici: la suite usa ciò che l'operatore ha già installato.

## Principi

- Il programma è Rust nativo: **si compila sull'OS di destinazione**. Non garantiamo cross-compile
  Windows→Linux/macOS; gli script sono pensati per girare sullo stesso sistema che producono.
- **L'AI resta esterna**: ComfyUI e provider linguistici sono servizi locali/LAN configurati per
  indirizzo. La suite non li avvia e non li scarica (nessuna sovrapposizione Ollama/ComfyUI gestita
  a mano o dal workflow).
- **Provider modulari**: vale la precedenza *parametro del comando → preferenza → variabile
  d'ambiente `NL_*` → default*. Tutto si imposta da **Impostazioni ▸ NL Video Suite** o da
  `prefs.set`.
- **Portable**: nessuna scrittura nel registro; impostazioni in `%APPDATA%\NL Video Suite` (o
  `~/.config`/`~/Library`), progetti dove scegli l'operatore.

## Avvio rapido su Windows

- Doppio clic su `AVVIA-NL-Video-Suite-NUOVA.cmd` nella radice del workspace: cerca la build più
  recente in `distribuzione\NLVideoSuite-*-windows-x64` e la avvia.
- `INSTALLA-NL-Video-Suite-NUOVA.cmd` installa per-utente con scorciatoie Start Menu/Desktop.
- Guida dettagliata: `documentazione/GUIDA_AVVIO_NL_VIDEO_SUITE.md`.

## Build e installer per piattaforma

| Piattaforma | Portabile (consigliato per provare) | Installer / pacchetto |
|---|---|---|
| Windows | `packaging\nl\portable.ps1 -Version 0.1.0` → cartella + `.zip` con launcher | `packaging\windows\package.ps1` (MSI/WiX; prerequisiti in `packaging/windows/INSTALLATION.md`), `sign.ps1` per la firma |
| Linux | `packaging/nl/portable.sh --version 0.1.0` → cartella + `.tar.gz` | `packaging/linux/package.sh` (deb/rpm/AppImage secondo il file), `install.sh`, `flatpak-*.sh`, metadati `.desktop`/AppStream |
| macOS | `packaging/nl/portable.sh --version 0.1.0` → cartella + `.tar.gz`, launcher `.command` | `packaging/macos/package.sh` (`.app` + DMG, Info.plist, entitlements, import-cert per la firma) |
| FreeBSD | — | `packaging/freebsd/package.sh` |
| Web/wasm | — | `packaging/web` (build wasm, senza AI locale) |

### Prerequisiti per ricreare tutto

1. **Rust** (toolchain stable; il repository fissa i toolchain con `.config`/`rust-toolchain` se
   presente). Windows: MSVC build tools; macOS: Xcode command line tools; Linux: build-essential,
   pkg-config e le librerie di sistema richieste da winit/egui/cpal.
2. **ffmpeg/ffprobe**: su Windows il portabile copia `app\tools\ffmpeg.exe` e `ffprobe.exe`; sulle
   altre piattaforme mettili accanto all'eseguibile o su `PATH`.
3. **ComfyUI** (per generazione immagini/video/musica) e un **provider linguistico** (Ollama oppure
   un server OpenAI-compatibile) installati a parte.
4. **ASR** (facoltativo): il bridge Python e i modelli whisper/alignment indicati in
   `Impostazioni ▸ NL Video Suite ▸ Speech` o nelle variabili d'ambiente.

Comandi tipici da `filmcraft-nl`:

```bash
cargo xtask ci          # fmt, clippy, test e controlli di progetto
cargo build --release -p filmcraft
./packaging/nl/portable.sh --version 0.1.0 --no-build
```

Su Windows: `cargo build --release -p filmcraft; .\packaging\nl\portable.ps1 -NoBuild`.

## Contenuto del bundle portabile

```
NL Video Suite.exe        (Linux/macOS: "NL Video Suite")   l'editor
Avvia NL Video Suite.cmd  (Linux/macOS: .sh/.command)       launcher che imposta NL_SUITE_ROOT
workflows/                la libreria workflow ComfyUI copiata
tools/                    ffmpeg/ffprobe (solo Windows, se presenti)
DISTRIBUZIONE.md          questo documento
README.txt                istruzioni rapide
```

Non contiene: ComfyUI, Ollama o qualsiasi modello, media generati, progetti. Chi riceve il bundle
installa i servizi AI a parte e li configura in Impostazioni.

## Provider AI: cosa si configura

### ComfyUI

- **Impostazioni ▸ NL Video Suite ▸ ComfyUI**: `nl.comfyUrl` (es. `http://127.0.0.1:8188`).
- Funziona con qualunque endpoint HTTP di ComfyUI raggiungibile (locale o in LAN), purché esponga
  `/prompt`, `/object_info`, `/upload/image`, `/queue`, `/history`, `/free`.
- I workflow si importano/aggiornano dal pannello **NL Workflows** (`nl.workflows.import`), oppure
  copiando JSON API nella cartella `workflows`.

### Modello linguistico (Ollama o compatibile OpenAI)

| Impostazione | Preferenza | Variabile | Default |
|---|---|---|---|
| Provider | `nl.llmProvider` (`ollama` / `openai_compatible`) | `NL_LLM_PROVIDER` | `ollama` |
| Endpoint Ollama | `nl.ollamaUrl` | `NL_OLLAMA_URL` | `http://127.0.0.1:11434` |
| Endpoint compatibile | `nl.llmUrl` (con `/v1`) | `NL_LLM_URL` | `http://127.0.0.1:8080/v1` |
| Modello | `nl.llmModel` (vuoto = `nl.ollamaModel`) | `NL_LLM_MODEL` | modello Ollama |
| Token bearer (facoltativo) | `nl.llmApiKey` | `NL_LLM_API_KEY` | nessuno |

Provider compatibili provati: qualunque server che espone `POST /v1/chat/completions` e
`GET /v1/models` — llama.cpp server, LM Studio, vLLM, LocalAI, gateway. La suite chiede
`response_format: json_object` e, se il server lo rifiuta, riprova senza. Funzionano anche
endpoint OpenAI-compatibili di Ollama (`http://127.0.0.1:11434/v1`).
Il pannello **NL Settings** mostra provider, endpoint e modello in uso; **NL Workflows ▸ Diagnostica**
e `nl.services.status` dicono se l'endpoint risponde (senza avviarlo).

### Analisi timeline con LFM (vision) e audio

- Avvia il server vision locale (esempio, LFM3 su llama.cpp):
  `K:\AI\LlamaCPP\llama-server.exe -m K:\AI\LlamaCPP\Modello\LFM3\LFM3-Omni-2B-Q8_0.gguf --mmproj K:\AI\LlamaCPP\Modello\LFM3\mmproj-LFM3-Omni-2B-Q8_0.gguf -c 4096 -ngl 99 --host 127.0.0.1 --port 8080 --jinja`
- Impostazioni ▸ NL Video Suite ▸ **Timeline analysis (vision)**: `nl.visionUrl` (default
  `http://127.0.0.1:8080/v1`), `nl.visionModel` (vuoto = primo modello dichiarato).
- GUI: pannello **NL Generate** → «Analyze timeline with vision»; i frame analizzati compaiono in
  elenco e «Use analysis in prompt» li inserisce nel prompt. Con `includeAudio` (CLI) il range
  viene trascritto con il provider ASR e aggiunto al contesto.
- CLI: `filmcraft-cli exec nl.timeline.analyze '{"startSeconds":0,"endSeconds":60,"everySeconds":10,"maxFrames":6,"includeAudio":true}'`.

### Trascrizione (ASR)

- `nl.asrProvider`: `bridge` (bridge WhisperX della suite, default) oppure
  `openai_compatible` (whisper.cpp server, faster-whisper-server, vLLM whisper, LocalAI).
- `nl.asrUrl` (base con `/v1`, default `http://127.0.0.1:8000/v1`), `nl.asrApiKey`
  (opzionale, `NL_ASR_API_KEY`), `nl.asrModel`, `nl.asrLanguage`, `nl.asrDevice`,
  `nl.asrCache`/`nl.asrCacheDir`.
- Con il backend compatibile: `nl.asr.transcribe {path|item, dryRun?}` invia il file a
  `/audio/transcriptions` e restituisce testo/lingua/segmenti; con il bridge usare
  `transcript.transcribeNl` che applica anche la trascrizione al media.

- `nl.asrDevice` (`auto`/`cuda`/`cpu`), `nl.asrLanguage`, `nl.asrModel` (nome o cartella),
  `nl.asrCache`, `nl.asrCacheDir`.
- Il bridge attuale è quello della suite (Python + WhisperX/faster-whisper): la scelta del motore è
  un'estensione prevista; nel frattempo i percorsi si impostano dalle preferenze e la diagnostica
  (`nl.diagnostics`) elenca i file che mancano.
- **Nessun TTS**: la narrazione resta la registrazione voiceover della suite (scelta dell'operatore).

### Altre funzioni

ComfyUI, provider linguistico e ASR non vanno in conflitto tra loro: la suite non avvia servizi. Se
due servizi devono stare sulla stessa GPU, l'operatore li alterna (le nostre prove usano sempre un
servizio per volta; le chiamate al modello linguistico usano `keep_alive=0` dove previsto).

## Dove stanno i dati e pulizia automatica

Impostazioni, cache e media generati stanno in una cartella per utente:

- Windows: `%APPDATA%\NL Video Suite` (Roaming);
- Linux: `~/.config/NL Video Suite` o `$XDG_CONFIG_HOME/NL Video Suite`;
- macOS: `~/Library/Application Support/NL Video Suite`.

Contenuto tipico: `preferences.json`, `workspaces.json`, `nl/` (bridge, workflow derivati, versioni),
`generated/` (media prodotti da ComfyUI), `Auto-Save/`, `Logs/`. I **progetti** non stanno qui: sono
i file `.fcproj` che scegli tu.

**Pulizia automatica**: all'avvio l'app elimina (al massimo una volta al giorno) i file temporanei e
le cache più vecchie di `nl.cleanupDays` giorni (default 7): frame del bridge generativo, log,
cache ASR. I **media generati non vengono mai cancellati automaticamente**: per includerli attiva
`nl.cleanupGenerated` e imposta `nl.generatedKeepDays` (default 30). Da pannello **NL Delivery** puoi
fare un'anteprima (`Preview cleanup`) o eseguire subito (`Clean up now`); da CLI:
`filmcraft-cli exec nl.storage.cleanup '{"dryRun":true}'`. Nessuna cancellazione fuori dalla cartella
dati.
### Installer per-utente NL (senza amministratore)

| Piattaforma | Comando | Cosa fa |
|---|---|---|
| Windows | `packaging\nl\install.ps1 -Bundle <cartella portabile>` | copia in `%LOCALAPPDATA%\Programs\NL Video Suite`, scorciatoie Start Menu/desktop opzionali, voce di disinstallazione HKCU; `-Uninstall` rimuove tutto; `-NoShortcuts` per uso portatile |
| Windows (Inno) | `iscc /DMyAppVersion=… /DSourceBundle=<cartella> packaging\nl\installer.iss` | installer Inno Setup 6 per-utente, con licenza, scorciatoie e uninstaller (richiede Inno Setup installato) |
| Linux | `packaging/nl/install.sh` | installa in `~/.local/opt/nl-video-suite`, launcher in `~/.local/bin`, voce `.desktop`; `--uninstall` |
| macOS | `packaging/nl/install-macos.sh` | crea `NL Video Suite.app` in `~/Applications` dal bundle portabile; `--uninstall` |

Nessuno di questi installer include o avvia ComfyUI, Ollama, modelli o servizi: restano esterni e si
configurano in Impostazioni ▸ NL Video Suite. L'installer Windows per-utente è stato provato davvero
(install + uninstall su cartella temporanea); gli script Linux/macOS hanno la sintassi verificata e
vanno provati sulla rispettiva piattaforma; l'installer Inno Setup è pronto ma va compilato con Inno
Setup 6 (qui non installato).
## Installer come "ricreano tutto"

- **Windows**: `packaging\windows\package.ps1` produce l'MSI con WiX (file `.wxs`, policy di
  installazione, controllo prerequisiti); `sign.ps1` firma l'installer.
- **Linux**: `packaging/linux/package.sh` produce pacchetti nativi (deb/rpm/AppImage) e metadati
  desktop/AppStream; `install.sh` per un'installazione locale; Flatpak per la distribuzione sandbox.
- **macOS**: `packaging/macos/package.sh` produce `NL Video Suite.app` e il DMG con Info.plist,
  entitlements e finestra del DMG; `import-cert.sh` per la firma.
- **Portabile** (`packaging/nl`): la via più semplice per provare su un'altra macchina senza
  installer; copia workflows e ffmpeg quando disponibili.
- Gli script upstream sono ereditati dal fork: contengono ancora riferimenti al nome tecnico
  "FilmCraft" nei percorsi/codici, ma i testi visibili dell'app sono "NL Video Suite". La
  rinomina completa dei pacchetti installer è tracciata in P10.

## Verifica su una nuova macchina

Per Linux e macOS, con prerequisiti, build, bundle, `test-portable.sh` headless e installazione
per-utente, segui `documentazione/INSTALL_LINUX_MACOS.md`.

1. Copiare la cartella portabile (o installare il pacchetto).
2. Avviare con il launcher.
3. **Impostazioni ▸ NL Video Suite**: indirizzo ComfyUI; provider linguistico (Ollama o compatibile),
   endpoint, modello; cartelle ASR.
4. Pannello **NL Settings ▸ Check services**: ComfyUI e provider devono risultare reachable.
5. **NL Workflows ▸ Verify ComfyUI API / Doctor**: i workflow devono essere validi.
6. Montaggio rapido: workspace **Quick Edit** di default, nessun AI precaricato.

## Limiti noti

- Gli script Linux/macOS non sono stati eseguiti su questa macchina Windows: sono forniti con la
  procedura e vanno verificati sulla rispettiva piattaforma (P10).
- I pacchetti installer upstream usano il nome tecnico del fork; la rinomina NL è un lavoro
  successivo di P10.
- ASR ha un solo backend (bridge della suite); l'astrazione multi-motore è prevista ma non ancora
  implementata.
- ComfyUI deve essere raggiungibile per la generazione; senza ComfyUI il montaggio e l'analisi
  locale (trascrizioni già presenti, ricerca, capitoli, titoli, delivery) restano disponibili.
