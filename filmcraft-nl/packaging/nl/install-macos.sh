#!/usr/bin/env bash
# Per-user installer for the macOS portable bundle of NL Video Suite.
#
# It wraps the portable folder (the one with "NL Video Suite", workflows/ and README.txt) in a
# minimal .app bundle under ~/Applications, so Finder and Launchpad can start it. No administrator
# rights, no service installation: ComfyUI, Ollama or an OpenAI-compatible server stay external and
# are configured in Settings.
#
#   ./install-macos.sh [--bundle DIR] [--applications DIR] [--uninstall]
#
# For a fully signed/notarised DMG use packaging/macos/package.sh instead.

set -euo pipefail

APPS="${HOME}/Applications"
BUNDLE=""
UNINSTALL=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --bundle) BUNDLE="$2"; shift 2 ;;
        --applications) APPS="$2"; shift 2 ;;
        --uninstall) UNINSTALL=1; shift ;;
        -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

APP="$APPS/NL Video Suite.app"
CONTENTS="$APP/Contents"

if [[ "$UNINSTALL" -eq 1 ]]; then
    rm -rf "$APP"
    echo "NL Video Suite removed from $APPS"
    exit 0
fi

if [[ -z "$BUNDLE" ]]; then
    BUNDLE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fi
if [[ ! -x "$BUNDLE/NL Video Suite" ]]; then
    echo "no 'NL Video Suite' executable in $BUNDLE (build it with packaging/nl/portable.sh)" >&2
    exit 1
fi

rm -rf "$APP"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"
cp -R "$BUNDLE/." "$CONTENTS/Resources/"
chmod +x "$CONTENTS/Resources/NL Video Suite"

cat > "$CONTENTS/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>NL Video Suite</string>
  <key>CFBundleDisplayName</key><string>NL Video Suite</string>
  <key>CFBundleIdentifier</key><string>ai.nlvideo.suite</string>
  <key>CFBundleExecutable</key><string>launcher</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.2.0</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

cat > "$CONTENTS/MacOS/launcher" <<'LAUNCHER'
#!/usr/bin/env bash
HERE="$(cd "$(dirname "$0")/../Resources" && pwd)"
export NL_SUITE_ROOT="$HERE"
exec "$HERE/NL Video Suite" "$@"
LAUNCHER
chmod +x "$CONTENTS/MacOS/launcher"

echo "NL Video Suite installed in $APP"
echo "Uninstall: $0 --applications '$APPS' --uninstall"
