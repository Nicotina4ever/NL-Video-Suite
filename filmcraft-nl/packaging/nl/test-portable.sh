#!/usr/bin/env bash
# Headless verification of the Linux/macOS portable bundle of NL Video Suite.
#
#   ./test-portable.sh [--bundle DIR] [--install] [--keep]
#
# Checks, without opening the GUI:
#   - the app and the CLI are present and executable;
#   - the CLI answers `commands` and lists the NL commands (bridge, storage cleanup, ASR provider…);
#   - a tiny CLI script opens the demo project, inspects the sequence and reports storage;
#   - with --install: install.sh (Linux) / install-macos.sh (macOS) into a temporary prefix/apps
#     folder, verifies the launcher/desktop entry and runs the installed CLI, then uninstalls.
# Exit code 0 = all checks passed. Run this on the actual machine after building the bundle.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUNDLE=""
INSTALL=0
KEEP=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --bundle) BUNDLE="$2"; shift 2 ;;
        --install) INSTALL=1; shift ;;
        --keep) KEEP=1; shift ;;
        -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

if [[ -z "$BUNDLE" ]]; then
    # Newest portable bundle next to packaging/ or in distribuzione/
    BUNDLE="$(find "$SCRIPT_DIR/../.." "$SCRIPT_DIR/../../distribuzione" -maxdepth 2 -type d -name 'NLVideoSuite-*-*' 2>/dev/null | sort -r | head -n 1 || true)"
fi
if [[ -z "$BUNDLE" || ! -d "$BUNDLE" ]]; then
    echo "FAIL: portable bundle not found (pass --bundle DIR)" >&2
    exit 1
fi

APP="$BUNDLE/NL Video Suite"
CLI="$BUNDLE/NL Video Suite CLI"
TMP="$(mktemp -d)"
FAILED=0
trap 'rm -rf "$TMP"' EXIT

check() {
    local name="$1"; shift
    if "$@" >/dev/null 2>&1; then
        echo "PASS  $name"
    else
        echo "FAIL  $name" >&2
        FAILED=1
    fi
}

echo "Bundle: $BUNDLE"
echo "OS:     $(uname -s) $(uname -m)"
check "app present and executable" test -x "$APP"
check "CLI present and executable" test -x "$CLI"

if [[ -x "$CLI" ]]; then
    check "CLI lists the NL commands" bash -c "'$CLI' commands 2>/dev/null | grep -q 'nl.bridge.generate'"
    check "CLI lists storage cleanup" bash -c "'$CLI' commands 2>/dev/null | grep -q 'nl.storage.cleanup'"
    check "CLI lists ASR provider" bash -c "'$CLI' commands 2>/dev/null | grep -q 'nl.asr.transcribe'"
    cat > "$TMP/smoke.jsonl" <<'JSONL'
{"id":"file.openDemoProject","params":{}}
{"id":"sequence.inspect","params":{}}
{"id":"nl.storage.status","params":{}}
JSONL
    if "$CLI" run "$TMP/smoke.jsonl" > "$TMP/smoke.out" 2> "$TMP/smoke.err"; then
        if grep -q '"ok":true' "$TMP/smoke.out" && grep -q 'sequence' "$TMP/smoke.out" && grep -q 'dataBytes' "$TMP/smoke.out"; then
            echo "PASS  demo smoke test (open + inspect + storage)"
        else
            echo "FAIL  demo smoke test output incomplete" >&2
            sed -n '1,5p' "$TMP/smoke.out" >&2
            FAILED=1
        fi
    else
        echo "FAIL  demo smoke test" >&2
        sed -n '1,10p' "$TMP/smoke.err" >&2
        FAILED=1
    fi
fi

if [[ "$INSTALL" -eq 1 ]]; then
    if [[ "$(uname -s)" == "Darwin" ]]; then
        APPS="$TMP/Applications"
        if "$SCRIPT_DIR/install-macos.sh" --bundle "$BUNDLE" --applications "$APPS" >/dev/null 2>&1; then
            check "macOS app bundle installed" test -d "$APPS/NL Video Suite.app"
            check "macOS launcher present" test -x "$APPS/NL Video Suite.app/Contents/MacOS/launcher"
            check "macOS resources carry the app" test -x "$APPS/NL Video Suite.app/Contents/Resources/NL Video Suite"
            "$SCRIPT_DIR/install-macos.sh" --applications "$APPS" --uninstall >/dev/null || true
            echo "PASS  macOS uninstall"
        else
            echo "FAIL  macOS install" >&2
            FAILED=1
        fi
    else
        PREFIX="$TMP/prefix"
        if "$SCRIPT_DIR/install.sh" --bundle "$BUNDLE" --prefix "$PREFIX" >/dev/null 2>&1; then
            check "Linux install (app)" test -x "$PREFIX/opt/nl-video-suite/NL Video Suite"
            check "Linux launcher" test -x "$PREFIX/bin/nl-video-suite"
            check "Linux desktop entry" test -f "$PREFIX/share/applications/nl-video-suite.desktop"
            "$SCRIPT_DIR/install.sh" --prefix "$PREFIX" --uninstall >/dev/null
            check "Linux uninstall removes the app" test ! -e "$PREFIX/opt/nl-video-suite"
        else
            echo "FAIL  Linux install" >&2
            FAILED=1
        fi
    fi
fi

if [[ "$FAILED" -ne 0 ]]; then
    echo "RESULT: FAILED" >&2
    exit 1
fi
echo "RESULT: ALL CHECKS PASSED"
