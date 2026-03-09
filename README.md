# RTS Prototype

A real-time strategy game prototype built with [Bevy](https://bevyengine.org/) 0.18.

<!-- screenshot placeholder -->

## Features

- **World** — 500x500 procedural terrain with Perlin noise heightmap (fBm, 4 octaves) and 5 distinct biomes (Forest, Desert, Mud, Water, Mountain) using moisture/temperature noise layers
- **Biomes** — Each biome has unique vertex coloring, biome-appropriate resource distribution, and scattered decorations (grass, bushes, rocks, dead trees)
- **3D Assets** — KayKit Forest Nature Pack (trees, rocks, props), KayKit Adventurers (Barbarian, Knight, Ranger, Mage, Rogue characters), KayKit Skeletons (Skeleton Warrior, Rogue, Minion, Mage), and KayKit Character Animations (shared Idle/Walk/Attack/Die animation sets)
- **Animation** — Skeletal GLTF character animations with state machine (Idle, Walk, Attack, Die), 200ms blended transitions, and smooth directional facing via slerp
- **Buildings** — 9 building types with placement preview, construction timer, prerequisite system, and 3-level upgrade system with unique bonuses per level
- **Units** — 8 types: Worker, Soldier, Archer (ranged), Tank, Knight, Mage, Priest, Cavalry — trained from buildings. Soldiers can upgrade to Knights
- **Siege** — 2 siege units: Catapult (long-range AoE) and Battering Ram (melee anti-structure)
- **Abilities** — Knights (Charge, Shield Bash), Mages (Fireball, Frost Nova), Priests (Heal, Holy Smite), Catapults (Boulder Throw)
- **Summons** — Skeleton Minion, Spirit Wolf, Fire Elemental
- **Enemies** — 4 mob camps (Goblin, Skeleton, Orc, Demon) with patrol AI, aggro detection, and boss variants
- **Combat** — Melee and ranged attacks, auto-targeting idle units, projectiles, hit-flash VFX, tower auto-attack
- **Economy** — 5 resource types (Wood, Copper, Iron, Gold, Oil) procedurally distributed across biomes with auto-gathering workers
- **Tree Growth** — Saplings spawn and grow through stages into harvestable mature trees over time
- **Day/Night Cycle** — 600-second animated cycle (Dawn/Day/Dusk/Night) with keyframed sun illuminance, color, pitch, ambient light, and sky color
- **Volumetric Fog** — Atmospheric fog volume on camera with density and color animated per time-of-day phase
- **Entity Lights** — Dynamic point lights spawned at entity clusters (buildings and units), intensity scales with day/night cycle (full at night, 30% during day)
- **Fog of War** — Texture-based fog of war with per-entity vision ranges, edge glow, noise overlay, and explored/unexplored tinting
- **Save/Load** — JSON game state serialization with stable entity IDs, cross-reference resolution, and resource node position matching
- **Minimap** — Interactive minimap with real-time unit and building positions, camera viewport indicator
- **Controls** — Click, box-select, shift-toggle selection; formation movement; right-click attack targeting; building placement
- **Camera** — WASD pan, Q/E rotate, scroll zoom
- **UI** — Top resource bar, selection panel, context-sensitive action bar with card-hand building UI (dynamic states: enabled / can't afford with per-resource red highlights / locked with hover tooltips), train buttons, building info, training queue with progress bars
- **Pathfinding** — Thin dashed lines with destination ring, terrain-following
- **Debug Tools** — F3 debug panel with organized sections (Visuals, Entities, Game), real-time tweaking of lighting/fog/shader parameters, entity spawning/manipulation, save/load controls, JSON config persistence

## Getting Started

### Prerequisites

- [Rust toolchain](https://rustup.rs/)

### Run

```sh
cargo run
```

Dev profile has dependency optimizations (`opt-level = 2`) for acceptable framerate.

## Controls

| Input | Action |
|---|---|
| W A S D | Pan camera |
| Q / E | Rotate camera |
| Scroll wheel | Zoom in / out |
| Left click | Select unit or building |
| Left drag | Box select units |
| Shift + click | Add / remove from selection |
| Right click ground | Move selected units (formation) |
| Right click mob | Attack target with selected units |
| Bottom buttons | Place buildings (when nothing selected) or train units (when building selected) |
| Escape / Right click | Cancel building placement |
| F3 | Toggle debug panel |

## How to Play

1. You start with a pre-built **Base**, **3 Workers**, and **300 Wood / 60 Copper / 20 Iron**
2. Once the Base is ready, **Barracks, Workshop, Tower, Storage, Mage Tower, Temple, Stable, and Siege Works** unlock
3. Click a building button at the bottom bar to enter placement mode
4. A green ghost preview follows your cursor — left-click to place, right-click or Escape to cancel
5. Buildings construct over time (shown with translucent material and scale animation)
6. Select a completed building to train units or upgrade it (up to level 3)
7. Select a completed **Barracks** to train Workers, Soldiers, and Archers
8. Select a completed **Workshop** to train Tanks
9. Select a completed **Mage Tower** to train Mages and Priests
10. Select a completed **Stable** to train Cavalry and Knights
11. Select a completed **Siege Works** to train Catapults and Battering Rams
12. **Towers** automatically attack nearby mobs with projectiles
13. Send workers near resource nodes to auto-gather — resources are distributed by biome
14. Buildings can be demolished for a 50% resource refund

## Biomes

| Biome | Terrain Color | Primary Resource | Secondary Resource |
|---|---|---|---|
| Forest | Green | Wood (high density) | — |
| Desert | Sandy yellow | Copper | Gold |
| Mud/Dirt | Brown | Iron | Copper |
| Water (edges) | Blue | Oil | — |
| Mountain | Gray/white | Gold | Iron |

## Buildings

| Type | Cost | Build Time | Requires | Function |
|---|---|---|---|---|
| Base | 100W 20C | 15s | — | Unlocks other buildings, trains Workers |
| Barracks | 80W 40C 20I | 12s | Base | Trains Workers, Soldiers, Archers |
| Workshop | 60W 60C 40I 10G | 18s | Base | Trains Tanks |
| Tower | 40W 30C 30I | 10s | Base | Auto-attacks nearby mobs (range 15) |
| Storage | 60W 10C | 8s | Base | Resource depot; gather aura at level 1+ |
| Mage Tower | 60W 30I 40G | 20s | Base | Trains Mages, Priests |
| Temple | 80W 20C 50G | 22s | Base | Trains Priests; healing aura at level 1+ |
| Stable | 70W 30C 20I | 14s | Base | Trains Cavalry, Knights |
| Siege Works | 80W 60I 20G | 20s | Base | Trains Catapults, Battering Rams |

All buildings support 3-level upgrades with bonuses like vision boost, train time reduction, stat boosts, range/damage increase, gather aura, and heal aura.

## Training Costs

| Unit | Cost | Train Time | Trained At |
|---|---|---|---|
| Worker | 30W | 5s | Base, Barracks |
| Soldier | 10W 20C 10I | 8s | Barracks |
| Archer | 20W 10C 5I | 7s | Barracks |
| Tank | 30C 40I 10G 5O | 15s | Workshop |
| Knight | 10W 20C 40I 20G | 12s | Stable |
| Mage | 10W 40G | 15s | Mage Tower |
| Priest | 15W 30G | 12s | Mage Tower, Temple |
| Cavalry | 20W 15C 20I 10G | 10s | Stable |
| Catapult | 80W 60I 20G | 20s | Siege Works |
| Battering Ram | 100W 40I | 18s | Siege Works |

## Unit Stats

| Type | HP | Speed | Damage | Range | Cooldown | Abilities |
|---|---|---|---|---|---|---|
| Worker | 100 | 5.0 | 3 | 1.5 | 1.5s | — |
| Soldier | 100 | 4.5 | 12 | 2.0 | 1.0s | Upgrades to Knight |
| Archer | 100 | 5.5 | 8 | 12.0 | 1.5s | — |
| Tank | 100 | 3.0 | 18 | 2.5 | 2.0s | — |
| Knight | 200 | 6.0 | 18 | 2.5 | 0.8s | Charge, Shield Bash |
| Mage | 70 | 4.0 | 15 | 14.0 | 2.0s | Fireball, Frost Nova |
| Priest | 80 | 4.5 | 6 | 10.0 | 2.0s | Heal, Holy Smite |
| Cavalry | 150 | 7.0 | 14 | 2.0 | 0.9s | — |
| Catapult | 150 | 2.0 | 40 | 25.0 | 5.0s | Boulder Throw |
| Battering Ram | 200 | 2.5 | 50 | 2.0 | 4.0s | — |

## Enemy Camps

| Camp | Count | HP (Regular) | HP (Boss) | Damage | Aggro Range |
|---|---|---|---|---|---|
| Goblin | 5 | 50 | — | 5 | 15 |
| Skeleton | 5 + boss | 80 | 200 | 10 | 18 |
| Orc | 6 + boss | 120 | 300 | 15 | 20 |
| Demon | 5 + boss | 200 | 500 | 25 | 25 |

## Architecture

Plugin-based ECS architecture — each gameplay system is a self-contained Bevy plugin.

```
src/
├── main.rs           Entry point, plugin registration
├── blueprints.rs     Entity blueprint registry (stats, costs, visuals for all entities)
├── components.rs     All ECS components and resources
├── ground.rs         Procedural terrain with biome generation
├── camera.rs         RTS camera (pan, zoom, rotate)
├── lighting.rs       Day/night cycle, sun/ambient/sky animation, volumetric fog, entity lights
├── units.rs          Player unit spawning and movement
├── buildings.rs      Building placement, construction, training, upgrades, demolish
├── selection.rs      Click, box, and shift selection + right-click commands
├── ui.rs             HUD: resource bar, selection panel, context-sensitive action bar
├── model_assets.rs   Loads KayKit 3D models (characters, trees, rocks, props)
├── resources.rs      Biome-based resource node spawning, auto-gathering, tree growth, decoration scatter
├── mobs.rs           Enemy camps, patrol / aggro / chase AI
├── combat.rs         Melee and ranged attacks, auto-targeting, death
├── fog.rs            Fog of war system
├── fog_material.rs   Custom fog shader material
├── hover_material.rs Custom hover effect material
├── minimap.rs        Interactive minimap UI
├── pathvis.rs        Dashed path lines with destination ring
├── vfx.rs            Projectiles, melee flashes, impact effects
├── animation.rs      Skeletal GLTF animation state machine and directional facing
├── save.rs           JSON game state serialization and deserialization
├── theme.rs          UI color tokens and design constants
└── debug.rs          Debug panel with visual/entity/save tweaks
```

| Plugin | Description |
|---|---|
| `BlueprintPlugin` | Unified entity blueprint registry — stats, costs, visuals, abilities, upgrades for all 26 entity types |
| `GroundPlugin` | Generates 500x500 heightmap mesh with biome-based vertex colors, inserts BiomeMap |
| `CameraPlugin` | WASD pan, scroll zoom, Q/E orbit camera |
| `LightingPlugin` | Day/night cycle with keyframed sun/ambient/sky, volumetric atmospheric fog, dynamic entity cluster lights |
| `UnitsPlugin` | Spawns 3 starting workers, handles movement and avoidance |
| `BuildingsPlugin` | Building placement preview, construction progress, unit training queues, tower auto-attack, upgrades, demolish |
| `SelectionPlugin` | Click/box/shift selection for units and buildings, right-click move and attack |
| `UiPlugin` | Resource bar, selection panel, context-sensitive action bar with build/train buttons |
| `ModelAssetsPlugin` | Loads KayKit 3D models — Forest Nature Pack, Adventurers, Skeletons, Character Animations |
| `ResourcesPlugin` | Procedural biome-based resource node spawning with 3D models, auto-gather + deposit loop, tree growth, biome-aware decoration scatter |
| `MobsPlugin` | Spawns 4 enemy camps with patrol, aggro, chase, and return AI |
| `CombatPlugin` | Melee/ranged attacks, auto-acquire targets, death cleanup |
| `FogPlugin` | Texture-based fog of war with per-entity vision ranges, edge glow, noise overlay |
| `MinimapPlugin` | Interactive 200x200 minimap with real-time entity tracking and camera viewport |
| `PathVisPlugin` | Terrain-following dashed path lines with destination ring |
| `VfxPlugin` | Projectile flight, melee flash, impact flash |
| `AnimationPlugin` | GLTF skeletal animation discovery, state-driven playback (Idle/Walk/Attack/Die), smooth directional facing |
| `SavePlugin` | JSON save/load with stable entity IDs, two-pass world reconstruction, resource node position matching |
| `DebugPlugin` | F3 debug panel — organized Visuals/Entities/Game sections, real-time parameter tweaking, entity spawn/manipulate tools, JSON config persistence |

## Tech Stack

- **Rust** / **Bevy 0.18** / **noise 0.9** (fBm Perlin terrain + biome generation) / **bevy_mod_outline 0.12** (selection highlighting) / **serde + serde_json** (save system + debug config) / **rand 0.9** (procedural scatter)
