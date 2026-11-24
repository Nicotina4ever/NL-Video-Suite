# Packaging NL Video Suite

This folder adds the NL Video Suite portable bundles on top of the upstream packaging scripts.

| File | What it does |
|---|---|
| `portable.ps1` | Windows: builds the release executable and assembles + zips a portable folder (`-NoBuild` reuses `target\release\filmcraft.exe`). |
| `portable.sh` | Linux/macOS: builds the release executable and assembles a portable folder + `.tar.gz`. Run it on the OS you are packaging (no cross-compiling). |

Both scripts copy the ComfyUI workflow library from the workspace's `workflows` folder and write a
launcher that sets `NL_SUITE_ROOT` so the suite finds its workflows, plus `README.txt` and
`DISTRIBUZIONE.md`. They never bundle ComfyUI, Ollama, models or generated media.

Quick start on Windows:

```powershell
# from filmcraft-nl
.\packaging\nl\portable.ps1 -Version 0.1.0
# or, to reuse an existing release build:
.\packaging\nl\portable.ps1 -Version 0.1.0 -NoBuild
```

On Linux or macOS:

```bash
./packaging/nl/portable.sh --version 0.1.0
./packaging/nl/portable.sh --version 0.1.0 --no-build
```

## Per-user installers

| File | Platform | What it does |
|---|---|---|
| `install.ps1` | Windows | Copies the bundle to `%LOCALAPPDATA%\\Programs\\NL Video Suite`, Start Menu/desktop shortcuts, HKCU uninstall entry; `-Uninstall` removes it. Tested with the 0.2.0 bundle. |
| `installer.iss` | Windows | Inno Setup 6 per-user installer; compile with `iscc /DMyAppVersion=… /DSourceBundle=<bundle> packaging\\nl\\installer.iss`. |
| `install.sh` | Linux | Installs under `~/.local/opt/nl-video-suite`, launcher and `.desktop`; `--uninstall`. |
| `install-macos.sh` | macOS | Wraps the portable folder in a minimal `NL Video Suite.app` under `~/Applications`; `--uninstall`. |

All of them install nothing AI-related: ComfyUI, Ollama or an OpenAI-compatible server and the models
stay external and are configured in Settings ▸ NL Video Suite.

Output goes to `packaging/nl/out/` by default (ignored by git). For distro packages and signed
installers, see the upstream scripts in `packaging/windows` (MSI/WiX), `packaging/linux`
(deb/rpm/AppImage, Flatpak) and `packaging/macos` (app bundle + DMG), and the complete instructions in
`documentazione/DISTRIBUZIONE.md`.

Linux/macOS: see [documentazione/INSTALL_LINUX_MACOS.md](../../documentazione/INSTALL_LINUX_MACOS.md) for prerequisites, build, headless checks (	est-portable.sh) and per-user installation.
