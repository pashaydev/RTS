#!/usr/bin/env bash
set -euo pipefail

# Build a macOS distribution bundle.
# Usage: ./scripts/build-macos.sh [--skip-build]
#
# Output: dist/macos-rts.zip (containing rts/ folder with binary + assets)

SKIP_BUILD=false
if [[ "${1:-}" == "--skip-build" ]]; then
    SKIP_BUILD=true
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_TRIPLE="${MACOS_TARGET:-aarch64-apple-darwin}"
DIST="$ROOT/dist/macos/rts"
source "$ROOT/scripts/package-runtime.sh"

# --- Build ---
if [ "$SKIP_BUILD" = false ]; then
    echo "==> Building for $TARGET_TRIPLE..."
    rustup target add "$TARGET_TRIPLE"
    cargo build --release --target "$TARGET_TRIPLE" \
        --manifest-path "$ROOT/Cargo.toml"
fi

stage_runtime_tree \
    "$DIST" \
    "$ROOT/target/$TARGET_TRIPLE/release/rts" \
    "rts"

# --- Create zip archive ---
ZIP="$ROOT/dist/macos-rts.zip"
create_runtime_archive "$ROOT/dist/macos" "$ZIP"

# --- Clean up folder ---
rm -rf "$ROOT/dist/macos"

echo ""
echo "    Extract the zip and run ./rts from inside the rts/ folder."
