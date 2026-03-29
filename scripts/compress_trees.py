"""
Blender script to compress tree/bush GLB models for RTS game.

Usage:
    /Applications/Blender.app/Contents/MacOS/Blender --background --python scripts/compress_trees.py

Compression steps per model:
  1. Decimate meshes (~50% reduction)
  2. Resize all textures to max 512px
  3. Export as GLB with Draco mesh compression
"""

import bpy
import os
import sys

# --- Configuration ---
INPUT_DIR = os.path.join(os.path.dirname(__file__), "..", "assets", "trees")
OUTPUT_DIR = os.path.join(os.path.dirname(__file__), "..", "assets", "trees_compressed")
DECIMATE_RATIO = 0.5       # keep 50% of faces
MAX_TEXTURE_SIZE = 512      # max texture dimension in px
DRACO_COMPRESSION = 6       # Draco compression level (1-10)

# Files to process
MODEL_FILES = [
    "tree (1).glb",
    "tree (2).glb",
    "tree (3).glb",
    "tree (4).glb",
    "tree (7).glb",
    "tree (8).glb",
    "bush (1).glb",
    "bush (2).glb",
]


def clear_scene():
    """Remove all objects, meshes, materials, images from the scene."""
    bpy.ops.wm.read_factory_settings(use_empty=True)


def decimate_meshes(ratio):
    """Apply decimate modifier to all mesh objects."""
    for obj in bpy.data.objects:
        if obj.type != 'MESH':
            continue
        # Skip very low-poly meshes
        if len(obj.data.polygons) < 100:
            continue

        mod = obj.modifiers.new(name="Decimate", type='DECIMATE')
        mod.ratio = ratio
        mod.use_collapse_triangulate = True

        # Apply modifier
        bpy.context.view_layer.objects.active = obj
        bpy.ops.object.modifier_apply(modifier=mod.name)

    print(f"  Decimated meshes (ratio={ratio})")


def resize_textures(max_size):
    """Resize all images to max_size, keeping aspect ratio."""
    for img in bpy.data.images:
        if img.size[0] == 0 or img.size[1] == 0:
            continue
        w, h = img.size
        if w <= max_size and h <= max_size:
            continue
        scale = max_size / max(w, h)
        new_w = max(1, int(w * scale))
        new_h = max(1, int(h * scale))
        img.scale(new_w, new_h)
        print(f"  Resized image '{img.name}': {w}x{h} -> {new_w}x{new_h}")


def process_model(input_path, output_path):
    """Load, compress, and export a single GLB model."""
    filename = os.path.basename(input_path)
    print(f"\nProcessing: {filename}")

    clear_scene()

    # Import GLB
    bpy.ops.import_scene.gltf(filepath=input_path)
    print(f"  Imported: {len(bpy.data.objects)} objects, {len(bpy.data.meshes)} meshes")

    # Count original stats
    orig_verts = sum(len(m.vertices) for m in bpy.data.meshes)
    orig_faces = sum(len(m.polygons) for m in bpy.data.meshes)

    # Decimate
    decimate_meshes(DECIMATE_RATIO)

    # Count new stats
    new_verts = sum(len(m.vertices) for m in bpy.data.meshes)
    new_faces = sum(len(m.polygons) for m in bpy.data.meshes)
    print(f"  Geometry: {orig_verts} -> {new_verts} verts, {orig_faces} -> {new_faces} faces")

    # Resize textures
    resize_textures(MAX_TEXTURE_SIZE)

    # Export as standard GLB (no Draco/WebP — Bevy doesn't support those extensions)
    bpy.ops.export_scene.gltf(
        filepath=output_path,
        export_format='GLB',
        export_draco_mesh_compression_enable=False,
        export_image_format='AUTO',
    )

    orig_size = os.path.getsize(input_path)
    new_size = os.path.getsize(output_path)
    ratio = (1 - new_size / orig_size) * 100
    print(f"  Size: {orig_size/1024:.0f}KB -> {new_size/1024:.0f}KB ({ratio:.1f}% reduction)")


def main():
    os.makedirs(OUTPUT_DIR, exist_ok=True)

    total_orig = 0
    total_new = 0

    for filename in MODEL_FILES:
        input_path = os.path.join(INPUT_DIR, filename)
        output_path = os.path.join(OUTPUT_DIR, filename)

        if not os.path.exists(input_path):
            print(f"SKIP: {filename} not found")
            continue

        process_model(input_path, output_path)
        total_orig += os.path.getsize(input_path)
        total_new += os.path.getsize(output_path)

    print(f"\n{'='*50}")
    print(f"TOTAL: {total_orig/1024/1024:.1f}MB -> {total_new/1024/1024:.1f}MB "
          f"({(1-total_new/total_orig)*100:.1f}% reduction)")
    print(f"Output directory: {OUTPUT_DIR}")


if __name__ == "__main__":
    main()
