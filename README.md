# RTS Prototype

A real-time strategy game prototype built with [Bevy](https://bevyengine.org/) 0.15.

<!-- screenshot placeholder -->

## Features

- **World** — 500x500 procedural terrain with Perlin noise heightmap (fBm, 4 octaves) and vertex-colored biomes (grass, dirt, rock)
- **Units** — 4 types: Worker, Soldier, Archer (ranged), Tank, each with unique stats and visuals
- **Enemies** — 4 mob camps (Goblin, Skeleton, Orc, Demon) with patrol AI, aggro detection, and boss variants
- **Combat** — Melee and ranged attacks, auto-targeting idle units, projectiles, hit-flash VFX
- **Economy** — 5 resource types (Wood, Copper, Iron, Gold, Oil) with auto-gathering workers
- **Controls** — Click, box-select, shift-toggle selection; formation movement; right-click attack targeting
- **Camera** — WASD pan, Q/E rotate, scroll zoom
- **UI** — Top resource bar, selection panel, bottom spawn buttons with hover feedback
- **Pathfinding** — Movement arrows that follow terrain contours

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
| Left click | Select unit |
| Left drag | Box select |
| Shift + click | Add / remove from selection |
| Right click ground | Move selected units (formation) |
| Right click mob | Attack target with selected units |
| Bottom buttons | Spawn new units |

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
├── ground.rs      Procedural terrain generation
├── camera.rs      RTS camera (pan, zoom, rotate)
├── units.rs       Player unit spawning and movement
├── selection.rs   Click, box, and shift selection + right-click commands
├── ui.rs          HUD: resource bar, selection panel, spawn buttons
├── resources.rs   Resource nodes and auto-gathering
├── mobs.rs        Enemy camps, patrol / aggro / chase AI
├── combat.rs      Melee and ranged attacks, auto-targeting, death
├── pathvis.rs     Movement arrow visualization
└── vfx.rs         Projectiles, melee flashes, impact effects
```

| Plugin | Description |
|---|---|
| `GroundPlugin` | Generates 500x500 heightmap mesh with vertex colors |
| `CameraPlugin` | WASD pan, scroll zoom, Q/E orbit camera |
| `UnitsPlugin` | Spawns player units, handles movement and avoidance |
| `SelectionPlugin` | Click/box/shift selection, right-click move and attack commands |
| `UiPlugin` | Resource bar, selection panel, spawn buttons |
| `ResourcesPlugin` | Spawns resource nodes, auto-gather + deposit loop |
| `MobsPlugin` | Spawns 4 enemy camps with patrol, aggro, chase, and return AI |
| `CombatPlugin` | Melee/ranged attacks, auto-acquire targets, death cleanup |
| `PathVisPlugin` | Terrain-following movement arrows |
| `VfxPlugin` | Projectile flight, melee flash, impact flash |

## Tech Stack

- **Rust** / **Bevy 0.15** / **noise 0.9** (fBm Perlin terrain)
