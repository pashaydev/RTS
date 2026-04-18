"""Bake an 8-direction billboard impostor atlas + JSON sidecar from
Orc.glb for a Bevy 0.18 RTS far-distance crowd renderer.

Runs headless under Blender 4.5+:

    blender --background --python scripts/bake_goblin_atlas.py -- [args]

The trailing `--` is a Blender convention: everything after it is forwarded
to this script's argparse. Run the wrapper shell script for convenience.

Coordinate conventions
----------------------
After glTF import, Blender's axes are X=right, Y=depth, Z=up. The camera
sits on the -Y side, pitched 30° down toward the origin. A positive yaw
step rotates the root counterclockwise around +Z (seen from above) — i.e.
around Blender's up axis, which equals Bevy's +Y up after the glTF
coordinate transform.

At yaw=0 the character is presented chest-to-camera. In the glTF/Bevy
world frame (Y up, -Z forward) that means the character's facing
direction projects to +Z world space — the convention the Phase 3 shader
is expected to parse. Resulting column layout:

    col 0 (0°)   front view (chest toward camera)
    col 4 (180°) back view  (mirror pair with col 0)
    col 2 (90°)  facing screen-right
    col 6 (270°) facing screen-left
"""

import argparse
import json
import math
import os
import shutil
import sys
from pathlib import Path

import bpy
import mathutils
import numpy as np


STATES_ALL = [
    ("Idle",   "CharacterArmature|Idle"),
    ("Walk",   "CharacterArmature|Walk"),
    ("Run",    "CharacterArmature|Run"),
    ("Attack", "CharacterArmature|Punch"),
    ("Death",  "CharacterArmature|Death"),
]

YAWS_DEG = [0, 45, 90, 135, 180, 225, 270, 315]

# Armature object name exposed by the KayKit Orc rig after glTF import.
# Change this (and MESH_NAMES) if baking a different source model.
ARMATURE_NAME = "CharacterArmature"
MESH_NAMES = ("Orc", "Orc_Weapon")


def parse_args():
    argv = sys.argv
    argv = argv[argv.index("--") + 1:] if "--" in argv else []
    p = argparse.ArgumentParser(prog="bake_goblin_atlas")
    p.add_argument("--source", default="assets/models/Orc.glb")
    p.add_argument("--out-png", default="assets/impostors/goblin_atlas.png")
    p.add_argument("--out-json", default="assets/impostors/goblin_atlas.json")
    p.add_argument("--cell-size", type=int, default=128)
    p.add_argument("--frames-per-state", type=int, default=16)
    p.add_argument("--pitch-deg", type=float, default=30.0)
    p.add_argument("--margin-px", type=int, default=10)
    p.add_argument("--fps", type=int, default=12)
    p.add_argument("--only-idle-walk", action="store_true",
                   help="Bake only Idle + Walk (useful for quick iteration).")
    p.add_argument("--keep-intermediates", action="store_true")
    return p.parse_args(argv)


def clear_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    # Deterministic defaults regardless of user prefs.
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE_NEXT"


def import_glb(source_abs: str):
    bpy.ops.import_scene.gltf(filepath=source_abs)
    arm = bpy.data.objects.get(ARMATURE_NAME)
    meshes = [bpy.data.objects.get(n) for n in MESH_NAMES]
    meshes = [m for m in meshes if m is not None]
    if arm is None or not meshes:
        raise RuntimeError(
            f"Expected '{ARMATURE_NAME}' and one of {MESH_NAMES} in "
            f"{source_abs}, found: {[o.name for o in bpy.data.objects]}"
        )
    # Remove stray helpers / auto-imported lights & cameras.
    for o in list(bpy.data.objects):
        if o.type in ("LIGHT", "CAMERA"):
            bpy.data.objects.remove(o, do_unlink=True)
        elif o.name == "Icosphere":
            bpy.data.objects.remove(o, do_unlink=True)
    return arm, meshes


def normalize_materials(mesh):
    """Flatten each material to diffuse-only while preserving its albedo.

    We want the baked atlas to carry each material's visual color
    (Orc skin green, belt brown, weapon metal, etc.) so far-distance
    sprites read as a recognizable character — not a white blob. But we
    still want flat shading (no specular highlights baked in) so runtime
    lighting isn't double-applied when the shader eventually tints.

    What we change per material:
      * Disconnect any texture node bound to Base Color, but **keep** the
        existing `baseColorFactor` / default value. KayKit-style GLBs
        encode the color entirely in `baseColorFactor`, so preserving
        that gives us the authored palette for free.
      * Force roughness=1, metallic=0, specular=0 — kills view-dependent
        highlights that would look wrong in a low-res sprite.
    """
    for slot in mesh.material_slots:
        mat = slot.material
        if mat is None or not mat.use_nodes:
            continue
        nt = mat.node_tree
        bsdf = next((n for n in nt.nodes if n.type == "BSDF_PRINCIPLED"), None)
        if bsdf is None:
            continue

        # Strip texture links from Base Color but keep the default factor.
        base = bsdf.inputs.get("Base Color")
        if base is not None:
            for link in list(base.links):
                nt.links.remove(link)

        for name, value in (("Roughness", 1.0), ("Metallic", 0.0)):
            sock = bsdf.inputs.get(name)
            if sock is not None:
                for link in list(sock.links):
                    nt.links.remove(link)
                sock.default_value = value

        # "Specular IOR Level" in Blender 4.x; older versions used "Specular".
        for name in ("Specular IOR Level", "Specular"):
            sock = bsdf.inputs.get(name)
            if sock is not None:
                for link in list(sock.links):
                    nt.links.remove(link)
                sock.default_value = 0.0
                break


def setup_lighting():
    """Three-quarter key + soft fill. Keeps the goblin readable without
    blowing out the flat-white material."""
    key = bpy.data.lights.new("KeyLight", type="SUN")
    key.energy = 1.6
    key.angle = math.radians(15)  # softer shadows
    key_obj = bpy.data.objects.new("KeyLight", key)
    bpy.context.collection.objects.link(key_obj)
    # Aim roughly from camera-upper-left so yaw variation shows up as shading
    # differences between left/right-facing columns.
    key_obj.rotation_euler = (math.radians(50), 0.0, math.radians(-30))

    fill = bpy.data.lights.new("FillLight", type="SUN")
    fill.energy = 0.5
    fill_obj = bpy.data.objects.new("FillLight", fill)
    bpy.context.collection.objects.link(fill_obj)
    fill_obj.rotation_euler = (math.radians(-20), 0.0, math.radians(150))


def make_root_empty(arm, meshes):
    bpy.ops.object.empty_add(type="PLAIN_AXES", location=(0, 0, 0))
    root = bpy.context.active_object
    root.name = "ImpostorRoot"
    # Re-parent the armature plus any meshes whose parent is not the
    # armature. Meshes skinned to the armature follow it automatically, so
    # they don't need their own root-parent link — but if the source rig
    # stores them as siblings of the armature we still want them under the
    # root so yaw rotation covers them too.
    for obj in [arm, *meshes]:
        if obj.parent is None:
            obj.parent = root
            obj.matrix_parent_inverse = root.matrix_world.inverted()
    return root


def meshes_world_aabb(meshes):
    """World-space AABB of the fully-evaluated meshes.

    Uses the depsgraph so armature skinning is applied before measurement.
    The raw `mesh.data.vertices` are rest-pose positions in mesh-local
    space, which on a rig like KayKit's (armature at 100× scale, mesh at
    identity) would give a box ~1/100th the visible size — fatal for
    framing. Evaluating via `depsgraph.evaluated_get()` yields the deformed
    mesh as the renderer sees it.
    """
    depsgraph = bpy.context.evaluated_depsgraph_get()
    xs, ys, zs = [], [], []
    for mesh_obj in meshes:
        eval_obj = mesh_obj.evaluated_get(depsgraph)
        eval_mesh = eval_obj.to_mesh()
        bm = eval_obj.matrix_world
        for v in eval_mesh.vertices:
            w = bm @ v.co
            xs.append(w.x)
            ys.append(w.y)
            zs.append(w.z)
        eval_obj.to_mesh_clear()
    return (mathutils.Vector((min(xs), min(ys), min(zs))),
            mathutils.Vector((max(xs), max(ys), max(zs))))


def setup_camera(center, ortho_scale, pitch_deg):
    cam_data = bpy.data.cameras.new("ImpostorCam")
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = ortho_scale
    cam_data.clip_start = 0.1
    cam_data.clip_end = 1000.0
    cam_obj = bpy.data.objects.new("ImpostorCam", cam_data)
    bpy.context.collection.objects.link(cam_obj)

    pitch = math.radians(pitch_deg)
    # Distance is arbitrary for ortho; pick something well outside the rig.
    D = 20.0
    cam_loc = mathutils.Vector((
        center.x,
        center.y - D * math.cos(pitch),
        center.z + D * math.sin(pitch),
    ))
    cam_obj.location = cam_loc
    direction = center - cam_loc
    cam_obj.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()
    bpy.context.scene.camera = cam_obj
    return cam_obj


def configure_render(scene, cell_size):
    scene.render.resolution_x = cell_size
    scene.render.resolution_y = cell_size
    scene.render.resolution_percentage = 100
    scene.render.film_transparent = True
    scene.render.dither_intensity = 0.0

    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.image_settings.color_depth = "8"
    scene.render.image_settings.compression = 15

    # Standard view transform: linear → sRGB display encoding, no tonemap.
    scene.view_settings.view_transform = "Standard"
    scene.view_settings.look = "None"
    scene.view_settings.exposure = 0.0
    scene.view_settings.gamma = 1.0

    # Eevee Next settings (deterministic: low samples, no motion blur).
    try:
        scene.eevee.taa_render_samples = 32
        scene.eevee.use_motion_blur = False
    except AttributeError:
        pass

    # Transparent world.
    world = bpy.data.worlds.new("ImpostorWorld") if not bpy.data.worlds else bpy.data.worlds[0]
    scene.world = world
    world.use_nodes = False
    if hasattr(world, "color"):
        world.color = (0.0, 0.0, 0.0)


def uniform_frame_samples(action, count):
    fr_start = action.frame_range[0]
    fr_end = action.frame_range[1]
    span = fr_end - fr_start
    if span <= 0 or count <= 1:
        return [fr_start] * count
    # Include both endpoints, evenly spaced.
    return [fr_start + (i / (count - 1)) * span for i in range(count)]


def bake_tiles(arm, root, scene, tmp_dir: Path, states, frames_per_state, cell_size):
    tiles = []  # list of (path, col, row)
    row_cursor = 0
    json_states = []

    for state_name, clip_name in states:
        action = bpy.data.actions.get(clip_name)
        if action is None:
            print(f"WARNING: action {clip_name!r} not found; skipping state {state_name!r}")
            continue

        if arm.animation_data is None:
            arm.animation_data_create()
        arm.animation_data.action = action

        frames = uniform_frame_samples(action, frames_per_state)

        json_states.append({
            "name": state_name,
            "clip": clip_name,
            "frame_count": frames_per_state,
            "row_offset": row_cursor,
        })
        print(f"[{state_name}] clip={clip_name!r} frames={frames_per_state} row_offset={row_cursor}")

        for frame_idx, f in enumerate(frames):
            scene.frame_set(int(round(f)))
            for dir_idx, yaw in enumerate(YAWS_DEG):
                root.rotation_euler = (0.0, 0.0, math.radians(yaw))
                # Force dependency graph refresh so armature pose + root rotation
                # are applied before rendering.
                bpy.context.view_layer.update()

                row = row_cursor + frame_idx
                col = dir_idx
                tile_path = tmp_dir / f"tile_{state_name}_f{frame_idx:02d}_d{dir_idx}.png"
                scene.render.filepath = str(tile_path)
                bpy.ops.render.render(write_still=True)
                tiles.append((tile_path, col, row))

        row_cursor += frames_per_state

    return tiles, json_states, row_cursor


def load_tile_linear(path: Path, cell_size: int) -> np.ndarray:
    img = bpy.data.images.load(str(path), check_existing=False)
    # Keep the default sRGB colorspace: `.pixels` will return values in linear
    # space, which is what we want for pixel-perfect atlas stitching.
    w, h = img.size
    if (w, h) != (cell_size, cell_size):
        bpy.data.images.remove(img)
        raise RuntimeError(f"Tile {path} has size {w}x{h}, expected {cell_size}x{cell_size}")
    buf = np.empty(w * h * 4, dtype=np.float32)
    img.pixels.foreach_get(buf)
    bpy.data.images.remove(img)
    tile = buf.reshape(h, w, 4)
    # Blender stores bottom-up; flip to top-down for easier atlas indexing.
    return np.flipud(tile).copy()


def assemble_atlas(tiles, total_rows, cell_size) -> np.ndarray:
    atlas_w = len(YAWS_DEG) * cell_size
    atlas_h = total_rows * cell_size
    atlas = np.zeros((atlas_h, atlas_w, 4), dtype=np.float32)
    for tile_path, col, row in tiles:
        tile = load_tile_linear(tile_path, cell_size)
        y0 = row * cell_size
        x0 = col * cell_size
        atlas[y0:y0 + cell_size, x0:x0 + cell_size, :] = tile
    return atlas


def save_atlas_png(atlas: np.ndarray, out_path: Path, scene):
    """Save a top-down RGBA float atlas as an sRGB-encoded 8-bit PNG."""
    h, w, _ = atlas.shape
    # Flip back to Blender's bottom-up storage.
    atlas_bu = np.flipud(atlas).astype(np.float32)

    out_img = bpy.data.images.new("goblin_atlas_out", width=w, height=h, alpha=True)
    out_img.colorspace_settings.name = "sRGB"
    out_img.pixels.foreach_set(atlas_bu.reshape(-1))

    out_path.parent.mkdir(parents=True, exist_ok=True)
    # save_render applies scene.render.image_settings (PNG RGBA 8-bit) and the
    # Standard view transform (linear → sRGB encoding), matching the pipeline
    # used for each rendered tile — so the linear pixel values round-trip
    # faithfully.
    out_img.save_render(filepath=str(out_path), scene=scene)
    bpy.data.images.remove(out_img)


def main():
    args = parse_args()
    # Anchor relative paths to the repo root (this script lives in repo/scripts).
    repo_root = Path(__file__).resolve().parent.parent
    os.chdir(repo_root)

    source_abs = str((repo_root / args.source).resolve())
    out_png = (repo_root / args.out_png).resolve()
    out_json = (repo_root / args.out_json).resolve()
    tmp_dir = out_png.parent / "_bake_tmp"

    print(f"Source : {source_abs}")
    print(f"Atlas  : {out_png}")
    print(f"JSON   : {out_json}")

    states = STATES_ALL[:2] if args.only_idle_walk else STATES_ALL

    clear_scene()
    arm, meshes = import_glb(source_abs)
    for mesh in meshes:
        normalize_materials(mesh)
    root = make_root_empty(arm, meshes)

    # Evaluate T-pose AABB to size the orthographic frustum.
    bpy.context.view_layer.update()
    aabb_min, aabb_max = meshes_world_aabb(meshes)
    size = aabb_max - aabb_min
    center = (aabb_min + aabb_max) * 0.5

    # Height-driven framing: fit the character's head-to-toe projected
    # height into (cell_size - 2*margin) pixels. T-pose arms extend much
    # wider than the body is tall, but arms come down during animation —
    # framing by height gives a much tighter silhouette per frame, at the
    # cost of clipping the arms of rare T-pose-shaped frames. Those few
    # frames read fine as "character with arms off-screen", while
    # arms-in-frame framing makes every other animation look like a
    # shrunken dot.
    pitch_rad = math.radians(args.pitch_deg)
    projected_h = size.z * math.cos(pitch_rad) + size.y * math.sin(pitch_rad)
    margin_scale = args.cell_size / max(args.cell_size - 2 * args.margin_px, 1)
    ortho_scale = projected_h * margin_scale

    # Re-center vertically on the rig midpoint so feet and head share the margin.
    cam_center = mathutils.Vector((0.0, center.y, center.z))
    setup_camera(cam_center, ortho_scale, args.pitch_deg)
    setup_lighting()

    scene = bpy.context.scene
    configure_render(scene, args.cell_size)

    # Clean tmp dir (idempotent runs).
    if tmp_dir.exists():
        shutil.rmtree(tmp_dir)
    tmp_dir.mkdir(parents=True, exist_ok=True)

    tiles, json_states, total_rows = bake_tiles(
        arm, root, scene, tmp_dir, states, args.frames_per_state, args.cell_size,
    )

    atlas = assemble_atlas(tiles, total_rows, args.cell_size)
    save_atlas_png(atlas, out_png, scene)

    atlas_w = len(YAWS_DEG) * args.cell_size
    atlas_h = total_rows * args.cell_size
    json_data = {
        "atlas_width": atlas_w,
        "atlas_height": atlas_h,
        "directions": len(YAWS_DEG),
        "cell_width": args.cell_size,
        "cell_height": args.cell_size,
        "fps": args.fps,
        "states": json_states,
    }
    out_json.write_text(json.dumps(json_data, indent=2) + "\n")

    if not args.keep_intermediates:
        shutil.rmtree(tmp_dir)

    print("=== BAKE COMPLETE ===")
    print(f"Atlas dimensions: {atlas_w} x {atlas_h}")
    print(f"States baked: {[s['name'] for s in json_states]}")
    print(f"Total frames: {sum(s['frame_count'] for s in json_states) * len(YAWS_DEG)} sprites")
    print(f"PNG: {out_png}")
    print(f"JSON: {out_json}")


if __name__ == "__main__":
    main()
