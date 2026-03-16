# ToonyTinyPeople RTS — Bevy 0.18 Asset Integration Guide

## Overview

| Metric | Value |
|---|---|
| Total GLB models | 284 |
| Total PNG textures | 40 |
| Total size | 21 MB |
| Source format | FBX (Unity) → glTF binary (.glb) |
| Target engine | Bevy 0.18 |

---

## Directory Structure

```
assets/
├── models/
│   ├── units/                    25 animated unit GLBs
│   │   ├── TT_Archer.glb
│   │   ├── TT_Commander.glb
│   │   ├── ...
│   │   └── machines/             4 animated siege weapon GLBs
│   │       ├── ballista.glb
│   │       ├── cart.glb
│   │       ├── catapult.glb
│   │       └── ram.glb
│   ├── buildings/                30 static building GLBs
│   │   ├── Castle.glb
│   │   ├── ...
│   │   ├── wall_A_gate_animated.glb   2 animated gate GLBs
│   │   ├── wall_B_gate_animated.glb
│   │   └── construction/         54 construction stage GLBs
│   │       ├── Castle_0.glb
│   │       ├── Castle_1.glb
│   │       └── ...
│   ├── extras/
│   │   ├── weapons/              75 static weapon GLBs
│   │   ├── heads/                71 static head GLBs
│   │   ├── banners/              2 animated banner GLBs
│   │   ├── backstuff/            5 static back items
│   │   └── projectiles/          8 static projectile GLBs
│   └── fx/                       10 static FX mesh GLBs
└── textures/
    ├── units/
    │   ├── TT_RTS_Units_texture.png       base unit texture
    │   └── color/                          12 team color variants
    ├── buildings/
    │   ├── TT_RTS_Buildings_texture.png   base building texture
    │   └── color/                          12 team color variants
    ├── banners/                            12 banner color variants
    └── fx/
        └── FX_MeshParts.png
```

---

## Unit Models (25 total)

Each unit GLB contains: mesh + skeleton + all animation clips baked in.
Load one file, get everything.

### Infantry (16 units)

| Unit | File | Body | Head | Weapon(s) | Shield | Extra | Animation Set |
|---|---|---|---|---|---|---|---|
| Archer | `TT_Archer.glb` | Body_03a | Head_03b | w_long_bow | — | quiver_A | Archer |
| Scout | `TT_Scout.glb` | Body_02a | Head_03a | w_short_bow | — | quiver_A | Archer |
| Crossbowman | `TT_Crossbowman.glb` | Body_07a | Head_06c | w_crossbow | — | quiver_A | Crossbow |
| Light Infantry | `TT_Light_Infantry.glb` | Body_04b | Head_05b | w_sword | shield_06 | — | Infantry |
| Swordman | `TT_Swordman.glb` | Body_02c | Head_06a | w_short_sword | shield_03 | — | Shield |
| Heavy Infantry | `TT_Heavy_Infantry.glb` | Body_06b | Head_08f | w_broad_sword_B | shield_11 | — | Shield |
| King | `TT_King.glb` | Body_10e | Head_12d | w_sword_B | shield_18 | — | Shield |
| Commander | `TT_Commander.glb` | Body_10e | Head_10f | w_warhammer | shield_20 | — | Shield |
| Spearman | `TT_Spearman.glb` | Body_04a | Head_05a | w_spear | shield_08 | — | Spear+Shield |
| Halberdier | `TT_Halberdier.glb` | Body_07b | Head_08b | w_halberd | — | — | Polearm |
| HeavySwordman | `TT_HeavySwordman.glb` | Body_08c | Head_09d | w_TH_sword_B | — | — | TwoHanded |
| Paladin | `TT_Paladin.glb` | Body_11e | Head_04d | w_TH_maul_B | — | — | TwoHanded |
| Peasant | `TT_Peasant.glb` | Body_01b | Head_01a | w_hammer | — | — | Infantry |
| Mage | `TT_Mage.glb` | Body_12e | Head_11d | w_staff_B, w_dagger_C | — | — | Staff |
| Priest | `TT_Priest.glb` | Body_12a | Head_01c | w_staff_A | — | — | Staff |
| High Priest | `TT_HighPriest.glb` | Body_12f | Head_03c | w_mace, w_staff_C | — | — | Staff |

### Cavalry (9 units)

Each cavalry GLB includes the rider + horse mesh + combined skeleton.

| Unit | File | Body | Head | Weapon(s) | Shield | Horse |  Animation Set |
|---|---|---|---|---|---|---|---|
| Light Cavalry | `TT_Light_Cavalry.glb` | Body_04a | Head_05b | w_sword | shield_08 | TT_Horse_B | Cavalry |
| Heavy Cavalry | `TT_Heavy_Cavalry.glb` | Body_05c | Head_09b | w_broad_sword | shield_11 | TT_Horse_F | Cav+Shield |
| Mounted King | `TT_Mounted_King.glb` | Body_10e | Head_12d | w_sword_B | shield_20 | TT_Horse_H | Cav+Shield |
| Mounted Knight | `TT_Mounted_Knight.glb` | Body_06f | Head_08b | w_cav_lance_C | shield_12 | TT_Horse_D | Cav+Spear |
| Mounted Mage | `TT_Mounted_Mage.glb` | Body_12g | Head_11d | w_staff_D, w_sword | — | TT_Horse_A | Cav+Staff |
| Mounted Paladin | `TT_Mounted_Paladin.glb` | Body_11e | Head_04f | w_TH_warhammer_B | — | TT_Horse_G | Cavalry |
| Mounted Priest | `TT_Mounted_Priest.glb` | Body_12b | Head_03d | w_mace_B, w_staff_C | — | TT_Horse_C | Cav+Staff |
| Mounted Scout | `TT_Mounted_Scout.glb` | Body_01c | Head_01a | w_short_bow | — | TT_Horse_A | Cav+Archer |
| Settler | `TT_Settler.glb` | Body_01c | Head_02c | — | — | TT_Horse_B | Cavalry |

---

## Animation Clip Names

### Infantry Animations

All infantry units have these **base clips**:

| Clip Name | Description | Duration |
|---|---|---|
| `idle` | Standing idle loop | ~5s |
| `walk` | Walking loop | ~1s |
| `run` | Running loop | ~1s |
| `attack_A` | Primary attack | ~2s |
| `attack_B` | Secondary attack variant | ~2s |
| `damage` | Hit reaction | ~1s |
| `death_A` | Death variant A | ~2s |
| `death_B` | Death variant B | ~2s |

**Infantry-type units** (Light Infantry, Peasant) also have:
| `punch_A` | Unarmed attack A | ~1s |
| `punch_B` | Unarmed attack B | ~1s |

**Staff-type units** (Mage, Priest, HighPriest) also have:
| `cast_A` | Spell cast A | ~2s |
| `cast_B` | Spell cast B | ~2s |

**Note:** Each GLB also contains two internal clips that should be **ignored**:
- `Bip001|Take 001|BaseLayer` — T-pose reference (from base FBX)
- `Bip001 Footsteps|Take 001|BaseLayer` — empty footstep track

### Cavalry Animations

| Clip Name | Description | Duration |
|---|---|---|
| `idle` | Mounted idle loop | ~3s |
| `walk` | Horse walking | ~1s |
| `run` | Horse running | ~1s |
| `attack` | Mounted attack (single variant) | ~1s |
| `damage` | Hit reaction on horse | ~1s |
| `death_A` | Death variant A (dismount) | ~2s |
| `death_B` | Death variant B | ~2s |

**Staff cavalry** (Mounted Mage, Mounted Priest) also have:
| `cast_A` | Mounted spell cast A | ~1s |
| `cast_B` | Mounted spell cast B | ~2s |

### Machine Animations

| Clip Name | Ballista | Catapult | Ram | Cart |
|---|---|---|---|---|
| `idle` | yes | yes | yes | yes |
| `move` | yes | yes | yes | yes |
| `attack` | yes | yes | yes | — |
| `damage` | yes | yes | yes | — |
| `death` | yes | yes | yes | yes |

### Gate Animations

Files: `wall_A_gate_animated.glb`, `wall_B_gate_animated.glb`

| Clip Name | Description |
|---|---|
| `wall_A_open` / `wall_B_open` | Gate opening |
| `wall_A_gate_close` / `wall_B_gate_close` | Gate closing |
| `wall_A_idle_open` / `wall_B_idle_open` | Idle in open position |
| `wall_A_idle_closed` / `wall_B_idle_closed` | Idle in closed position |

### Banner Animations

Files: `banner.glb`, `banner_plain.glb`

| Clip Name | Description |
|---|---|
| `banner_deploy` | Banner being planted |
| `banner_fall` | Banner falling over |
| `banner_idle_A` | Waving idle A |
| `banner_idle_B` | Waving idle B |
| `banner_idle_static` | Static/no wind |

---

## Buildings (84 total)

### Complete Buildings (30)

Static meshes, no animations. One GLB per building.

```
Archery, Barracks, BeastLair, Blacksmith, Castle, Farm, Granary,
House, Keep, Library, LumberMill, MageTower, Market, Stables,
Temple, Tower_A, Tower_B, Tower_C, TownHall, Workshop,
Wall_A_1x1, Wall_A_corner, Wall_A_gate, Wall_A_wall,
Wall_B_1x1, Wall_B_corner, Wall_B_gate, Wall_B_wall
```

### Construction Stages (54)

Each building has 2 construction stages: `_0` (foundation) and `_1` (partial).
The complete building is the final stage.

```
Castle_0.glb → Castle_1.glb → Castle.glb
```

### Animated Gates (2)

`wall_A_gate_animated.glb` and `wall_B_gate_animated.glb` — use these
instead of the static `Wall_A_gate.glb` / `Wall_B_gate.glb` when you
need open/close animations.

---

## Extras (Modular Attachments)

These are standalone meshes meant to be **attached to bone sockets** at runtime
for character customization beyond the 25 preset units.

| Category | Count | Path | Purpose |
|---|---|---|---|
| Weapons | 75 | `extras/weapons/` | Swords, axes, bows, staves, shields |
| Heads | 71 | `extras/heads/` | Head variants (Head_01a through Head_12e) |
| Backstuff | 5 | `extras/backstuff/` | Quivers, bags, wood bundles |
| Projectiles | 8 | `extras/projectiles/` | Arrows, bolts, catapult rocks |

---

## Textures & Team Colors

### Base Textures

All units share **one** texture atlas: `textures/units/TT_RTS_Units_texture.png`
All buildings share **one** texture atlas: `textures/buildings/TT_RTS_Buildings_texture.png`

### Team Color Variants (12 colors)

```
black, blue, blueB, brown, green, greenB,
pink, purple, red, tan, white, yellow
```

Unit variants at: `textures/units/color/TT_RTS_Units_{color}.png`
Building variants at: `textures/buildings/color/TT_RTS_Buildings_{color}.png`
Banner variants at: `textures/banners/TT_RTS_Banner_{Color}.png`

To swap team colors at runtime, replace the `StandardMaterial` base_color_texture
on the entity with the appropriate color variant PNG.

---

## Bevy 0.18 Integration Code

### Loading a Unit

```rust
use bevy::prelude::*;

fn spawn_archer(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    commands.spawn((
        SceneRoot(asset_server.load(
            GltfAssetLabel::Scene(0).from_asset("models/units/TT_Archer.glb")
        )),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
}
```

### Playing Animations

```rust
use bevy::animation::AnimationPlayer;

fn setup_animations(
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    // Load animation clips by label
    let idle = asset_server.load(
        GltfAssetLabel::Animation(6).from_asset("models/units/TT_Archer.glb")
    );
    let walk = asset_server.load(
        GltfAssetLabel::Animation(8).from_asset("models/units/TT_Archer.glb")
    );
    let run = asset_server.load(
        GltfAssetLabel::Animation(7).from_asset("models/units/TT_Archer.glb")
    );
    let attack = asset_server.load(
        GltfAssetLabel::Animation(0).from_asset("models/units/TT_Archer.glb")
    );

    // Build animation graph
    let (graph, indices) = AnimationGraph::from_clips([
        idle, walk, run, attack,
    ]);
    let graph_handle = graphs.add(graph);

    // Store indices for runtime switching
    // indices[0] = idle, indices[1] = walk, etc.
}
```

### Looking Up Animations by Name

Since animation indices may vary per GLB, use the `Gltf` asset to find clips by name:

```rust
fn find_animation_by_name(
    gltf_assets: Res<Assets<Gltf>>,
    gltf_handle: &Handle<Gltf>,
    name: &str,
) -> Option<Handle<AnimationClip>> {
    let gltf = gltf_assets.get(gltf_handle)?;
    gltf.named_animations.get(name).cloned()
}

// Usage:
// let idle_clip = find_animation_by_name(&gltf_assets, &handle, "idle");
// let attack_clip = find_animation_by_name(&gltf_assets, &handle, "attack_A");
```

### Swapping Team Colors

```rust
fn set_team_color(
    asset_server: &AssetServer,
    materials: &mut Assets<StandardMaterial>,
    material_handle: &Handle<StandardMaterial>,
    color: &str, // "red", "blue", "green", etc.
) {
    if let Some(mat) = materials.get_mut(material_handle) {
        mat.base_color_texture = Some(
            asset_server.load(format!("textures/units/color/TT_RTS_Units_{color}.png"))
        );
    }
}
```

### Loading a Building with Construction Stages

```rust
// Construction stage 0 (foundation)
let stage_0 = asset_server.load(
    GltfAssetLabel::Scene(0).from_asset("models/buildings/construction/Castle_0.glb")
);
// Construction stage 1 (partial)
let stage_1 = asset_server.load(
    GltfAssetLabel::Scene(0).from_asset("models/buildings/construction/Castle_1.glb")
);
// Completed building
let complete = asset_server.load(
    GltfAssetLabel::Scene(0).from_asset("models/buildings/Castle.glb")
);
```

### Animated Gate

```rust
fn spawn_gate(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        SceneRoot(asset_server.load(
            GltfAssetLabel::Scene(0)
                .from_asset("models/buildings/wall_A_gate_animated.glb")
        )),
        Transform::default(),
    ));
}

// Animation clips: wall_A_open, wall_A_gate_close, wall_A_idle_open, wall_A_idle_closed
```

---

## Animation Index Reference

Since `GltfAssetLabel::Animation(index)` requires numeric indices, here are the
indices for each animation type. Use `named_animations` (shown above) for
name-based lookup instead.

### Infantry (Archer/Scout/Crossbowman type — 8+2 clips)

| Index | Name |
|---|---|
| 0 | attack_A |
| 1 | attack_B |
| 2 | ~~Bip001\|Take 001\|BaseLayer~~ (skip) |
| 3 | damage |
| 4 | death_A |
| 5 | death_B |
| 6 | idle |
| 7 | run |
| 8 | walk |
| 9 | ~~Bip001 Footsteps~~ (skip) |

### Cavalry (7-9 clips)

| Index | Name |
|---|---|
| 0 | attack |
| 1 | damage |
| 2 | death_A |
| 3 | death_B |
| 4 | idle |
| 5 | run |
| 6 | walk |
| 7 | cast_A (staff units only) |
| 8 | cast_B (staff units only) |

### Machines (5 clips)

| Index | Name |
|---|---|
| 0 | attack |
| 1 | damage |
| 2 | death |
| 3 | idle |
| 4 | move |

---

## FX Meshes

Static destruction/debris meshes for visual effects:

| File | Purpose |
|---|---|
| `FX_wreck_A/B/C/D.glb` | Building destruction debris |
| `FX_wood_A/B/C.glb` | Wooden debris |
| `FX_stone_A/B.glb` | Stone debris |
| `FX_collision_plane.glb` | Ground collision plane |

---

## Conversion Notes

- **Infantry units**: Converted via Blender 4.5.6 Python API (FBX → GLB)
- **Cavalry + Cart**: Blender FBX importer has a bug with these files (`KeyError: Bip001 Model`).
  Converted via Meta's FBX2glTF tool, animations merged in Blender from GLB + animation FBXes.
- **Textures**: TGA → PNG via Pillow. Embedded FBX textures are **not** included in GLBs —
  you must assign textures from `textures/` at runtime via materials.
- **Root motion**: `_rm` animation variants (root motion baked) were excluded.
  Use the non-`_rm` versions and handle movement in code.
- Conversion scripts are in `scripts/` if you need to re-run or modify.
