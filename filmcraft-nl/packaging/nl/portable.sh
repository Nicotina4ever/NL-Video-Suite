#!/usr/bin/env bash
# Builds the portable NL Video Suite bundle for Linux or macOS.
#
# It builds the release executable (unless --no-build), then assembles a folder with the executable,
# the copied ComfyUI workflow library, the distribution notes and a launcher. Nothing AI is bundled:
# ComfyUI, Ollama and the language models stay external and are configured in Settings.
#
# Usage:
#   ./portable.sh [--version 0.1.0] [--no-build] [--suite-root /path/to/NL_Video_Suite] [--output DIR]
#
# The script must run on the OS it packages (Rust builds for the host); it does not cross-compile.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SCRIPT_DIR/../.." && pwd)"
VERSION="0.1.0"
NO_BUILD=0
SUITE_ROOT="$(cd "$REPO/.." && pwd)"
OUTPUT="$SCRIPT_DIR/out"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) VERSION="$2"; shift 2 ;;
        --no-build) NO_BUILD=1; shift ;;
        --suite-root) SUITE_ROOT="$2"; shift 2 ;;
        --output) OUTPUT="$2"; shift 2 ;;
        -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

if [[ "$NO_BUILD" -eq 0 ]]; then
    echo "Building the release executable…"
    (cd "$REPO" && cargo build --release -p filmcraft)
fi

if [[ "$(uname -s)" == "Darwin" ]]; then
    OS="macos"
else
    OS="linux"
fi
ARCH="$(uname -m)"
EXE="$REPO/target/release/filmcraft"
if [[ ! -x "$EXE" ]]; then
    echo "release binary not found: $EXE (run without --no-build)" >&2
    exit 1
fi

NAME="NLVideoSuite-$VERSION-$OS-$ARCH"
DEST="$OUTPUT/$NAME"
rm -rf "$DEST"
mkdir -p "$DEST"
cp "$EXE" "$DEST/NL Video Suite"
if [[ -x "$REPO/target/release/filmcraft-cli" ]]; then
    cp "$REPO/target/release/filmcraft-cli" "$DEST/NL Video Suite CLI"
    chmod +x "$DEST/NL Video Suite CLI"
else
    echo "warning: release CLI not found; automation tests will be skipped" >&2
fi
chmod +x "$DEST/NL Video Suite"

if [[ -d "$SUITE_ROOT/workflows" ]]; then
    cp -R "$SUITE_ROOT/workflows" "$DEST/workflows"
    echo "Workflows copied from $SUITE_ROOT/workflows"
else
    echo "warning: no workflows folder at $SUITE_ROOT/workflows" >&2
fi

if [[ -f "$SUITE_ROOT/documentazione/DISTRIBUZIONE.md" ]]; then
    cp "$SUITE_ROOT/documentazione/DISTRIBUZIONE.md" "$DEST/DISTRIBUZIONE.md"
fi

cat > "$DEST/NL Video Suite.sh" <<'LAUNCHER'
#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export NL_SUITE_ROOT="$HERE"
exec "$HERE/NL Video Suite" "$@"
LAUNCHER
chmod +x "$DEST/NL Video Suite.sh"

if [[ "$OS" == "macos" ]]; then
    cp "$DEST/NL Video Suite.sh" "$DEST/NL Video Suite.command"
    chmod +x "$DEST/NL Video Suite.command"
fi

cat > "$DEST/README.txt" <<README
NL Video Suite $VERSION - portable $OS bundle ($ARCH)

Start it with: ./NL Video Suite.sh$( [[ "$OS" == "macos" ]] && printf ' (or double-click NL Video Suite.command)' )

What is inside:
  NL Video Suite     the editor (no installer)
  NL Video Suite CLI automation/test client
  workflows/         the ComfyUI workflow library
  DISTRIBUZIONE.md   how to build/install on each OS and how to point the suite at AI services

What is NOT inside, by design: ComfyUI, Ollama or any language model, and generated media.
Install them separately and configure their addresses in Settings > NL Video Suite. Bundle your own
ffmpeg/ffprobe next to the executable or have them on PATH.
README
printf '\n' >> "$DEST/README.txt"

TARBALL="$DEST.tar.gz"
rm -f "$TARBALL"
tar -C "$OUTPUT" -czf "$TARBALL" "$NAME"
echo "Portable bundle: $DEST"
echo "Archive:         $TARBALL"
