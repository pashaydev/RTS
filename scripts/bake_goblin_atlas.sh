#!/usr/bin/env bash
# Thin wrapper around the headless Blender bake script.
#
#   ./scripts/bake_goblin_atlas.sh                 # full 5-state bake
#   ./scripts/bake_goblin_atlas.sh --only-idle-walk
#   ./scripts/bake_goblin_atlas.sh --keep-intermediates --cell-size 256
#
# Any flags are forwarded verbatim to bake_goblin_atlas.py via Blender's
# `--` passthrough convention.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Locate Blender: PATH first, then the standard macOS install location.
if command -v blender >/dev/null 2>&1; then
    BLENDER_BIN="blender"
elif [[ -x "/Applications/Blender.app/Contents/MacOS/Blender" ]]; then
    BLENDER_BIN="/Applications/Blender.app/Contents/MacOS/Blender"
else
    echo "error: blender not found in PATH or /Applications/Blender.app" >&2
    exit 1
fi

cd "${REPO_ROOT}"

exec "${BLENDER_BIN}" \
    --background \
    --python "${SCRIPT_DIR}/bake_goblin_atlas.py" \
    -- "$@"
