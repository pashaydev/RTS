#!/usr/bin/env bash
set -euo pipefail

# Build a Windows distribution bundle.
# Usage: ./scripts/build-windows.sh [--skip-build]
#
# Output: dist/windows/rts/
#   rts.exe
#   assets/   (only the asset subtrees actually used by the game)
#   config/   (empty, created for runtime settings)
#   saves/    (empty, created for save files)

SKIP_BUILD=false
if [[ "${1:-}" == "--skip-build" ]]; then
    SKIP_BUILD=true
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist/windows/rts"

# --- Build ---
if [ "$SKIP_BUILD" = false ]; then
    echo "==> Building for x86_64-pc-windows-msvc..."
    PATH="/tmp:/opt/homebrew/opt/llvm/bin:$PATH" \
        cargo xwin build --release --target x86_64-pc-windows-msvc \
        --manifest-path "$ROOT/Cargo.toml"
fi

# --- Assemble distribution ---
echo "==> Assembling distribution at $DIST"
rm -rf "$DIST"
mkdir -p "$DIST"

# Copy executable
cp "$ROOT/target/x86_64-pc-windows-msvc/release/rts.exe" "$DIST/"

# Create runtime directories
mkdir -p "$DIST/config" "$DIST/saves"

# Copy only the asset subtrees the game actually loads.
# This list mirrors the Dockerfile and model_assets.rs references.
ASSETS="$ROOT/assets"
DEST="$DIST/assets"

copy_asset_dir() {
    local src="$ASSETS/$1"
    local dst="$DEST/$1"
    if [ -d "$src" ]; then
        mkdir -p "$dst"
        cp -r "$src/." "$dst/"
    else
        echo "  WARNING: missing asset directory: $1"
    fi
}

echo "  Copying assets..."
copy_asset_dir "fonts"
copy_asset_dir "shaders"
copy_asset_dir "icons"
copy_asset_dir "KayKit_Forest_Nature/Assets/gltf"
copy_asset_dir "UltimateFantasyRTS/glTF"
copy_asset_dir "ToonyTinyPeople/models/buildings"
copy_asset_dir "ToonyTinyPeople/models/units"
copy_asset_dir "ToonyTinyPeople/textures/buildings"
copy_asset_dir "ToonyTinyPeople/textures/units"
copy_asset_dir "KayKit_Skeletons/characters/gltf"
copy_asset_dir "KayKit_Character_Animations/Animations/gltf/Rig_Medium"

# --- Summary ---
EXE_SIZE=$(du -sh "$DIST/rts.exe" | cut -f1)
ASSET_SIZE=$(du -sh "$DEST" | cut -f1)
echo ""
echo "==> Done!"
echo "    Executable:  $EXE_SIZE"
echo "    Assets:      $ASSET_SIZE"
echo "    Output:      $DIST/"
echo ""
echo "    To distribute, zip the $DIST folder."
echo "    Users run rts.exe from inside the folder."
