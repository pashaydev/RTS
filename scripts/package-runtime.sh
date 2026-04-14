#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

copy_asset_dir() {
    local assets_root="$1"
    local dest_root="$2"
    local rel_path="$3"
    local src="$assets_root/$rel_path"
    local dst="$dest_root/$rel_path"

    if [ -d "$src" ]; then
        mkdir -p "$dst"
        cp -r "$src/." "$dst/"
    else
        echo "  WARNING: missing asset directory: $rel_path"
    fi
}

stage_runtime_tree() {
    local dist_dir="$1"
    local binary_path="$2"
    local binary_name="$3"

    echo "==> Assembling distribution at $dist_dir"
    rm -rf "$dist_dir"
    mkdir -p "$dist_dir"

    cp "$binary_path" "$dist_dir/$binary_name"
    chmod +x "$dist_dir/$binary_name"

    mkdir -p "$dist_dir/config" "$dist_dir/saves" "$dist_dir/logs"

    local assets_root="$ROOT/assets"
    local dest_root="$dist_dir/assets"

    echo "  Copying assets..."
    copy_asset_dir "$assets_root" "$dest_root" "audio"
    copy_asset_dir "$assets_root" "$dest_root" "fonts"
    copy_asset_dir "$assets_root" "$dest_root" "shaders"
    copy_asset_dir "$assets_root" "$dest_root" "icons"
    copy_asset_dir "$assets_root" "$dest_root" "textures"
    copy_asset_dir "$assets_root" "$dest_root" "trees_compressed"
    copy_asset_dir "$assets_root" "$dest_root" "KayKit_Forest_Nature/Assets/gltf"
    copy_asset_dir "$assets_root" "$dest_root" "UltimateFantasyRTS/glTF"
    copy_asset_dir "$assets_root" "$dest_root" "ToonyTinyPeople/models/buildings"
    copy_asset_dir "$assets_root" "$dest_root" "ToonyTinyPeople/models/units"
    copy_asset_dir "$assets_root" "$dest_root" "ToonyTinyPeople/models/extras/projectiles"
    copy_asset_dir "$assets_root" "$dest_root" "ToonyTinyPeople/textures/buildings"
    copy_asset_dir "$assets_root" "$dest_root" "ToonyTinyPeople/textures/units"
    copy_asset_dir "$assets_root" "$dest_root" "KayKit_Character_Animations/Animations/gltf/Rig_Medium"
    copy_asset_dir "$assets_root" "$dest_root" "models"
}

create_runtime_archive() {
    local package_root="$1"
    local zip_path="$2"

    echo "==> Creating archive at $zip_path"
    rm -f "$zip_path"
    (cd "$package_root" && zip -r "$zip_path" rts/)

    local zip_size
    zip_size=$(du -sh "$zip_path" | cut -f1)
    echo ""
    echo "==> Done!"
    echo "    Archive:  $zip_size"
    echo "    Output:   $zip_path"
}
