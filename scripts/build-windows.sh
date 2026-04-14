#!/usr/bin/env bash
set -euo pipefail

# Build a Windows distribution bundle.
# Usage: ./scripts/build-windows.sh [--skip-build]
#
# Output: dist/windows-rts.zip (containing rts/ folder with exe + assets)

SKIP_BUILD=false
if [[ "${1:-}" == "--skip-build" ]]; then
    SKIP_BUILD=true
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist/windows/rts"
source "$ROOT/scripts/package-runtime.sh"

# --- Build ---
if [ "$SKIP_BUILD" = false ]; then
    echo "==> Building for x86_64-pc-windows-msvc..."
    PATH="/tmp:/opt/homebrew/opt/llvm/bin:$PATH" \
        cargo xwin build --release --target x86_64-pc-windows-msvc \
        --manifest-path "$ROOT/Cargo.toml"
fi

stage_runtime_tree \
    "$DIST" \
    "$ROOT/target/x86_64-pc-windows-msvc/release/rts.exe" \
    "rts.exe"

# --- Create zip archive ---
ZIP="$ROOT/dist/windows-rts.zip"
create_runtime_archive "$ROOT/dist/windows" "$ZIP"

# --- Clean up folder ---
rm -rf "$ROOT/dist/windows"

echo ""
echo "    Extract the zip and run rts.exe from inside the rts/ folder."
