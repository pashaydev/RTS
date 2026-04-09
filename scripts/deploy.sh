#!/usr/bin/env bash
set -euo pipefail

# Unified deployment script for the RTS game.
# Usage: ./scripts/deploy.sh [--all | --windows-only | --fly-only | --help]
#
# Flags:
#   --all            Build Windows bundle AND deploy web to Fly.io (default)
#   --windows-only   Only build the Windows distribution zip
#   --fly-only       Only deploy the web version to Fly.io
#   --help           Show this help message

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# --- Usage ---
usage() {
    sed -n '3,11p' "$0" | sed 's/^# \?//'
    exit 0
}

# --- Parse arguments ---
DO_WINDOWS=true
DO_FLY=true

case "${1:-}" in
    --all)           DO_WINDOWS=true;  DO_FLY=true  ;;
    --windows-only)  DO_WINDOWS=true;  DO_FLY=false ;;
    --fly-only)      DO_WINDOWS=false; DO_FLY=true  ;;
    --help|-h)       usage ;;
    "")              ;;  # default: --all
    *)
        echo "Unknown flag: $1"
        usage
        ;;
esac

# --- Dependency checks ---
check_tool() {
    local name="$1"
    local hint="$2"
    if ! command -v "$name" &>/dev/null; then
        echo "ERROR: '$name' not found. $hint"
        exit 1
    fi
}

if [ "$DO_WINDOWS" = true ]; then
    check_tool cargo "Install Rust: https://rustup.rs/"
    if ! cargo xwin --version &>/dev/null; then
        echo "ERROR: 'cargo-xwin' not found. Install with: cargo install cargo-xwin"
        exit 1
    fi
fi

if [ "$DO_FLY" = true ]; then
    FLY_CMD=""
    if command -v flyctl &>/dev/null; then
        FLY_CMD="flyctl"
    elif command -v fly &>/dev/null; then
        FLY_CMD="fly"
    else
        echo "ERROR: 'flyctl' not found. Install: curl -L https://fly.io/install.sh | sh"
        exit 1
    fi

    if ! "$FLY_CMD" auth whoami &>/dev/null; then
        echo "ERROR: Not logged in to Fly.io. Run: $FLY_CMD auth login"
        exit 1
    fi
fi

# --- Count steps ---
TOTAL=0
[ "$DO_WINDOWS" = true ] && TOTAL=$((TOTAL + 1))
[ "$DO_FLY" = true ]     && TOTAL=$((TOTAL + 1))
STEP=0

# --- Step: Windows build ---
if [ "$DO_WINDOWS" = true ]; then
    STEP=$((STEP + 1))
    echo "==> [$STEP/$TOTAL] Building Windows distribution..."
    "$ROOT/scripts/build-windows.sh"
fi

# --- Step: Fly.io deploy ---
if [ "$DO_FLY" = true ]; then
    STEP=$((STEP + 1))
    echo "==> [$STEP/$TOTAL] Deploying web version to Fly.io..."
    "$FLY_CMD" deploy --config "$ROOT/fly.toml"
fi

# --- Summary ---
echo ""
echo "==> Done!"
if [ "$DO_WINDOWS" = true ]; then
    ZIP="$ROOT/dist/windows-rts.zip"
    if [ -f "$ZIP" ]; then
        ZIP_SIZE=$(du -sh "$ZIP" | cut -f1)
        echo "    Windows:  $ZIP  ($ZIP_SIZE)"
    fi
fi
if [ "$DO_FLY" = true ]; then
    echo "    Web:      https://rts-game.fly.dev"
fi
echo ""
