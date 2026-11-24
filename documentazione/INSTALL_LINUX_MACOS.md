# Installazione e test su Linux e macOS

Questa guida serve per portare NL Video Suite sulle macchine Linux/macOS e verificarla in autonomia.
La build **non è cross-compilata da Windows**: si compila sullo stesso sistema dove gira (o in CI).

## 1. Prerequisiti

### Linux
- Rust stable (`rustup`), `cargo`, linker e tool di build:
  - Debian/Ubuntu: `sudo apt install build-essential pkg-config libasound2-dev libudev-dev libxkbcommon-dev libwayland-dev libx11-dev libxi-dev libgl1-mesa-dev`
  - Fedora: `sudo dnf groupinstall "Development Tools" && sudo dnf install alsa-lib-devel systemd-devel libxkbcommon-devel wayland-devel libXi-devel mesa-libGL-devel`
- `ffmpeg`/`ffprobe` su `PATH` (oppure mettili accanto all'eseguibile come in Windows).
- ComfyUI e provider AI restano esterni: si configurano in Impostazioni ▸ NL Video Suite.

### macOS
- Xcode Command Line Tools: `xcode-select --install`
- Rust stable (`rustup`), eventualmente `brew install ffmpeg`
- Per firmare/notarizzare: certificato Apple e `packaging/macos/import-cert.sh`.

## 2. Compilare la suite

Dalla cartella `filmcraft-nl`:

```bash
cargo build --release -p filmcraft -p filmcraft-cli
```

Questo produce:
- `target/release/filmcraft` (app)
- `target/release/filmcraft-cli` (automazione/test)

## 3. Creare il bundle portabile

```bash
./packaging/nl/portable.sh --version 2.0.0 --no-build
```

Risultato in `packaging/nl/out/NLVideoSuite-2.0.0-linux-x86_64/` (o `-macos-arm64`), con app, CLI,
`workflows/`, README e launcher. Per un archivio: `--version 2.0.0` crea anche il `.tar.gz`.

## 4. Verifica automatica (headless)

```bash
bash packaging/nl/test-portable.sh --bundle packaging/nl/out/NLVideoSuite-2.0.0-linux-x86_64
# con prova di installazione in una cartella temporanea:
bash packaging/nl/test-portable.sh --bundle ... --install
```

Controlla: app e CLI eseguibili, comandi NL (`nl.bridge.generate`, `nl.storage.cleanup`,
`nl.asr.transcribe`), apertura demo + inspect + storage, e (con `--install`) installazione
Linux/macOS in una destinazione temporanea, launcher/.desktop o .app, e disinstallazione.

Output atteso: una serie di `PASS ...` e `RESULT: ALL CHECKS PASSED`. Se qualcosa fallisce, lo script
stampa il comando e le prime righe dell'errore.

## 5. Installazione per-utente

### Linux
```bash
./packaging/nl/install.sh --bundle packaging/nl/out/NLVideoSuite-2.0.0-linux-x86_64
# avvio: ~/.local/bin/nl-video-suite
# disinstallazione:
./packaging/nl/install.sh --uninstall
```

### macOS
```bash
./packaging/nl/install-macos.sh --bundle packaging/nl/out/NLVideoSuite-2.0.0-macos-arm64
# crea ~/Applications/NL Video Suite.app
# se l'app è scaricata da internet e Gatekeeper la blocca:
xattr -dr com.apple.quarantine "$HOME/Applications/NL Video Suite.app"
# disinstallazione:
./packaging/nl/install-macos.sh --uninstall
```

Per pacchetti nativi (deb/rpm/AppImage/Flatpak) usare `packaging/linux/package.sh`; per `.app` firmata
e DMG usare `packaging/macos/package.sh` (solo su macOS).

## 6. Test di sviluppo (facoltativi ma consigliati)

```bash
cargo test -p filmcraft-engine --lib
cargo test -p filmcraft-ui-egui
cargo clippy -p filmcraft-engine -p filmcraft-ui-egui -p filmcraft-cli --all-targets -- -D warnings
bash -n packaging/nl/*.sh
```

Nota: non lanciare due comandi cargo (build/test) in parallelo; su Windows una build release e una
suite di test contemporanee hanno corrotto la rlib della UI (LNK2019/LNK2001). Eseguire in sequenza.

## 7. Primo avvio e configurazione AI

1. Avvia dal launcher (`NL Video Suite` o `nl-video-suite`).
2. Impostazioni ▸ NL Video Suite:
   - ComfyUI: indirizzo (es. `http://127.0.0.1:8188`);
   - provider linguistico: Ollama o OpenAI-compatibile (URL, modello, token);
   - ASR: `bridge` oppure server OpenAI-compatibile (`/v1`);
   - cartelle modelli/workflow se diverse.
3. `NL Settings ▸ Check services` deve mostrare ComfyUI/provider reachable.
4. Prova un montaggio senza AI (Quick Edit) e poi un dry run di generazione.

## 8. Troubleshooting rapido

| Sintomo | Cosa controllare |
|---|---|
| “command not found” o app non parte | `chmod +x "NL Video Suite"`; `ldd "NL Video Suite"` per librerie mancanti (Linux) |
| Nessuna finestra su Linux Wayland | prova `WAYLAND_DISPLAY=` o sessione X11; servono libwayland/xkbcommon |
| macOS “app danneggiata/non identificata” | `xattr -dr com.apple.quarantine`; per distribuzione firmare con `packaging/macos/package.sh` |
| ComfyUI/provider non reachable | indirizzo/porta nelle impostazioni; servizi avviati a mano, la suite non li avvia |
| CLI non trova i workflow | avvia con `NL_SUITE_ROOT=<cartella bundle>` (il launcher lo imposta già) |
| Test UI falliscono con errori di link | `cargo clean -p filmcraft-ui-egui` e rieseguire in sequenza |

Quando i test su Linux/macOS sono fatti, riporta l'output di `test-portable.sh` e le eventuali
differenze: le ricevute P10/P7 vanno aggiornate con quelle evidenze.
