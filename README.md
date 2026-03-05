# RTS Prototype

A real-time strategy game prototype built with [Bevy](https://bevyengine.org/) 0.18.

<!-- screenshot placeholder -->

## Features

- **World** — 500x500 procedural terrain with Perlin noise heightmap (fBm, 4 octaves) and 5 distinct biomes (Forest, Desert, Mud, Water, Mountain) using moisture/temperature noise layers
- **Biomes** — Each biome has unique vertex coloring, biome-appropriate resource distribution, and scattered decorations (grass, bushes, rocks, dead trees)
- **3D Assets** — KayKit Forest Nature Pack: low-poly trees for wood nodes, rocks for ore nodes, and decorative props placed via noise-based scatter
- **Buildings** — 5 building types (Base, Barracks, Workshop, Tower, Storage) with placement preview, construction timer, and prerequisite system
- **Units** — 4 types: Worker, Soldier, Archer (ranged), Tank — trained from buildings
- **Enemies** — 4 mob camps (Goblin, Skeleton, Orc, Demon) with patrol AI, aggro detection, and boss variants
- **Combat** — Melee and ranged attacks, auto-targeting idle units, projectiles, hit-flash VFX, tower auto-attack
- **Economy** — 5 resource types (Wood, Copper, Iron, Gold, Oil) procedurally distributed across biomes with auto-gathering workers. Start with 150 Wood and 30 Copper
- **Controls** — Click, box-select, shift-toggle selection; formation movement; right-click attack targeting; building placement
- **Camera** — WASD pan, Q/E rotate, scroll zoom
- **UI** — Top resource bar, selection panel, context-sensitive action bar with card-hand building UI (dynamic states: enabled / can't afford with per-resource red highlights / locked with hover tooltips), train buttons, building info
- **Pathfinding** — Thin dashed lines with destination ring, terrain-following

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

## How to Play

1. You start with **2 Workers** and **150 Wood / 30 Copper**
2. Click the **Base** button at the bottom bar to enter placement mode
3. A green ghost preview follows your cursor — left-click to place, right-click or Escape to cancel
4. The Base takes 15 seconds to construct (shown with translucent material)
5. Once the Base is complete, **Barracks, Workshop, Tower, and Storage** unlock
6. Select a completed **Barracks** to train Workers, Soldiers, and Archers
7. Select a completed **Workshop** to train Tanks
8. **Towers** automatically attack nearby mobs with projectiles
9. Send workers near resource nodes to auto-gather — resources are distributed by biome

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
| Storage | 60W 10C | 8s | Base | Storage building |

## Training Costs

| Unit | Cost | Train Time |
|---|---|---|
| Worker | 30W | 5s |
| Soldier | 10W 20C 10I | 8s |
| Archer | 20W 10C 5I | 7s |
| Tank | 30C 40I 10G 5O | 15s |

## Unit Stats

| Type | HP | Speed | Damage | Range | Cooldown |
|---|---|---|---|---|---|
| Worker | 100 | 5.0 | 3 | 1.5 | 1.5s |
| Soldier | 100 | 4.5 | 12 | 2.0 | 1.0s |
| Archer | 100 | 5.5 | 8 | 12.0 | 1.5s |
| Tank | 100 | 3.0 | 18 | 2.5 | 2.0s |

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
├── main.rs        Entry point, plugin registration
├── components.rs  All ECS components and resources
├── ground.rs      Procedural terrain with biome generation
├── camera.rs      RTS camera (pan, zoom, rotate)
├── units.rs       Player unit spawning and movement
├── buildings.rs   Building placement, construction, training, tower combat
├── selection.rs   Click, box, and shift selection + right-click commands
├── ui.rs          HUD: resource bar, selection panel, context-sensitive action bar
├── model_assets.rs Loads KayKit 3D models (trees, rocks, bushes, grass)
├── resources.rs   Biome-based resource node spawning, auto-gathering, and decoration scatter
├── mobs.rs        Enemy camps, patrol / aggro / chase AI
├── combat.rs      Melee and ranged attacks, auto-targeting, death
├── pathvis.rs     Dashed path lines with destination ring
└── vfx.rs         Projectiles, melee flashes, impact effects
```

| Plugin | Description |
|---|---|
| `GroundPlugin` | Generates 500x500 heightmap mesh with biome-based vertex colors, inserts BiomeMap |
| `CameraPlugin` | WASD pan, scroll zoom, Q/E orbit camera |
| `UnitsPlugin` | Spawns 2 starting workers, handles movement and avoidance |
| `BuildingsPlugin` | Building placement preview, construction progress, unit training queues, tower auto-attack |
| `SelectionPlugin` | Click/box/shift selection for units and buildings, right-click move and attack |
| `UiPlugin` | Resource bar, selection panel, context-sensitive action bar with build/train buttons |
| `ModelAssetsPlugin` | Loads KayKit Forest Nature Pack 3D models (trees, dead trees, rocks, bushes, grass) |
| `ResourcesPlugin` | Procedural biome-based resource node spawning with 3D models, auto-gather + deposit loop, biome-aware decoration scatter |
| `MobsPlugin` | Spawns 4 enemy camps with patrol, aggro, chase, and return AI |
| `CombatPlugin` | Melee/ranged attacks, auto-acquire targets, death cleanup |
| `PathVisPlugin` | Terrain-following dashed path lines with destination ring |
| `VfxPlugin` | Projectile flight, melee flash, impact flash |

## Tech Stack

- **Rust** / **Bevy 0.18** / **noise 0.9** (fBm Perlin terrain + biome generation)
