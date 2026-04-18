#!/usr/bin/env bash
set -euo pipefail

# Unified deployment script for the RTS game.
# Usage: ./scripts/deploy.sh [--all | --windows-only | --macos-only] [--patch | --minor | --major]
#
# Flags:
#   --all            Build Windows bundle and macOS bundle (default)
#   --windows-only   Only build the Windows distribution zip
#   --macos-only     Only build the macOS distribution zip
#   --patch          Increment patch version before deploy (default)
#   --minor          Increment minor version before deploy
#   --major          Increment major version before deploy
#   --help           Show this help message

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# --- Usage ---
usage() {
    sed -n '3,14p' "$0" | sed 's/^# \?//'
    exit 0
}

# --- Parse arguments ---
DO_WINDOWS=true
DO_MACOS=true
VERSION_BUMP="patch"
TARGET_SELECTED=false

for arg in "$@"; do
    case "$arg" in
        --all)
            if [ "$TARGET_SELECTED" = true ]; then
                echo "ERROR: only one deployment target flag can be used"
                exit 1
            fi
            DO_WINDOWS=true
            DO_MACOS=true
            TARGET_SELECTED=true
            ;;
        --windows-only)
            if [ "$TARGET_SELECTED" = true ]; then
                echo "ERROR: only one deployment target flag can be used"
                exit 1
            fi
            DO_WINDOWS=true
            DO_MACOS=false
            TARGET_SELECTED=true
            ;;
        --macos-only)
            if [ "$TARGET_SELECTED" = true ]; then
                echo "ERROR: only one deployment target flag can be used"
                exit 1
            fi
            DO_WINDOWS=false
            DO_MACOS=true
            TARGET_SELECTED=true
            ;;
        --patch)
            VERSION_BUMP="patch"
            ;;
        --minor)
            VERSION_BUMP="minor"
            ;;
        --major)
            VERSION_BUMP="major"
            ;;
        --help|-h)
            usage
            ;;
        *)
            echo "Unknown flag: $arg"
            usage
            ;;
    esac
done

bump_version() {
    local manifest="$1"
    local bump="$2"
    local current
    current="$(sed -nE 's/^version = "([0-9]+)\.([0-9]+)\.([0-9]+)"$/\1.\2.\3/p' "$manifest" | head -n1)"

    if [ -z "$current" ]; then
        echo "ERROR: failed to read version from $manifest"
        exit 1
    fi

    IFS=. read -r major minor patch <<< "$current"
    case "$bump" in
        patch)
            patch=$((patch + 1))
            ;;
        minor)
            minor=$((minor + 1))
            patch=0
            ;;
        major)
            major=$((major + 1))
            minor=0
            patch=0
            ;;
        *)
            echo "ERROR: unsupported version bump type: $bump"
            exit 1
            ;;
    esac

    local next_version="${major}.${minor}.${patch}"
    perl -0pi -e 's/^version = "\Q'"$current"'\E"$/version = "'"$next_version"'"/m' "$manifest"
    echo "$next_version"
}

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

if [ "$DO_MACOS" = true ]; then
    check_tool cargo "Install Rust: https://rustup.rs/"
    check_tool rustup "Install Rust: https://rustup.rs/"
fi

# --- Version bump ---
MANIFEST="$ROOT/Cargo.toml"
NEW_VERSION="$(bump_version "$MANIFEST" "$VERSION_BUMP")"
echo "==> Version bumped to $NEW_VERSION ($VERSION_BUMP)"

# --- Count steps ---
TOTAL=1
[ "$DO_WINDOWS" = true ] && TOTAL=$((TOTAL + 1))
[ "$DO_MACOS" = true ]   && TOTAL=$((TOTAL + 1))
STEP=0

STEP=$((STEP + 1))
echo "==> [$STEP/$TOTAL] Updated package version to $NEW_VERSION"

# --- Step: Windows build ---
if [ "$DO_WINDOWS" = true ]; then
    STEP=$((STEP + 1))
    echo "==> [$STEP/$TOTAL] Building Windows distribution..."
    "$ROOT/scripts/build-windows.sh"
fi

# --- Step: macOS build ---
if [ "$DO_MACOS" = true ]; then
    STEP=$((STEP + 1))
    echo "==> [$STEP/$TOTAL] Building macOS distribution..."
    "$ROOT/scripts/build-macos.sh"
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
if [ "$DO_MACOS" = true ]; then
    ZIP="$ROOT/dist/macos-rts.zip"
    if [ -f "$ZIP" ]; then
        ZIP_SIZE=$(du -sh "$ZIP" | cut -f1)
        echo "    macOS:    $ZIP  ($ZIP_SIZE)"
    fi
fi
echo "    Version:  $NEW_VERSION"
echo ""
