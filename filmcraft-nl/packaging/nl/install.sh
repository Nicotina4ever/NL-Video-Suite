#!/usr/bin/env bash
# Per-user installer for the Linux portable bundle of NL Video Suite.
#
# Run it from the extracted portable folder (the one with "NL Video Suite", workflows/ and
# README.txt), or pass --bundle:
#   ./install.sh [--bundle DIR] [--prefix ~/.local] [--uninstall]
#
# It copies the app under <prefix>/opt/nl-video-suite, adds a launcher in <prefix>/bin and a
# .desktop entry in <prefix>/share/applications. No root, no service installation: ComfyUI, Ollama
# or an OpenAI-compatible server stay external and are configured in Settings.

set -euo pipefail

PREFIX="${HOME}/.local"
BUNDLE=""
UNINSTALL=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --bundle) BUNDLE="$2"; shift 2 ;;
        --prefix) PREFIX="$2"; shift 2 ;;
        --uninstall) UNINSTALL=1; shift ;;
        -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

APP_DIR="$PREFIX/opt/nl-video-suite"
BIN_LINK="$PREFIX/bin/nl-video-suite"
DESKTOP="$PREFIX/share/applications/nl-video-suite.desktop"

if [[ "$UNINSTALL" -eq 1 ]]; then
    rm -f "$BIN_LINK" "$DESKTOP"
    rm -rf "$APP_DIR"
    echo "NL Video Suite removed from $PREFIX"
    exit 0
fi

if [[ -z "$BUNDLE" ]]; then
    BUNDLE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fi
if [[ ! -x "$BUNDLE/NL Video Suite" ]]; then
    echo "no 'NL Video Suite' executable in $BUNDLE (build it with packaging/nl/portable.sh)" >&2
    exit 1
fi

mkdir -p "$APP_DIR" "$PREFIX/bin" "$PREFIX/share/applications"
cp -R "$BUNDLE/." "$APP_DIR/"
chmod +x "$APP_DIR/NL Video Suite"

cat > "$BIN_LINK" <<LAUNCHER
#!/usr/bin/env bash
export NL_SUITE_ROOT="$APP_DIR"
exec "$APP_DIR/NL Video Suite" "\$@"
LAUNCHER
chmod +x "$BIN_LINK"

cat > "$DESKTOP" <<ENTRY
[Desktop Entry]
Type=Application
Name=NL Video Suite
Comment=Local AI video editor
Exec=$BIN_LINK %f
Icon=video-editor
Terminal=false
Categories=AudioVideo;Video;
MimeType=application/x-nl-video-suite;
ENTRY

echo "NL Video Suite installed in $APP_DIR"
echo "Launcher: $BIN_LINK"
echo "Uninstall: $0 --prefix $PREFIX --uninstall"
