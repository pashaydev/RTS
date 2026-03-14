# RTS Prototype

A real-time strategy game prototype built with [Bevy](https://bevyengine.org/) 0.18.

<!-- screenshot placeholder -->

## Features

- **World** — 500x500 procedural terrain with Perlin noise heightmap (fBm, 4 octaves) and 5 distinct biomes (Forest, Desert, Mud, Water, Mountain) using moisture/temperature noise layers
- **Biomes** — Each biome has unique vertex coloring, biome-appropriate resource distribution, and scattered decorations (grass, bushes, rocks, dead trees)
- **3D Assets** — KayKit Forest Nature Pack (trees, rocks, props), KayKit Adventurers (Barbarian, Knight, Ranger, Mage, Rogue characters), KayKit Skeletons (Skeleton Warrior, Rogue, Minion, Mage), and KayKit Character Animations (shared Idle/Walk/Attack/Die animation sets)
- **Animation** — Skeletal GLTF character animations with state machine (Idle, Walk, Attack, Die), 200ms blended transitions, and smooth directional facing via slerp
- **Opening Loop** — Each faction starts with 2 Workers and no Base; the first major action is founding a settlement
- **Buildings** — Expanded building roster with placement preview, construction timer, prerequisite system, and 3-level upgrades
- **Fortifications** — Specialized defenses: Watch Tower, Guard Tower, Ballista Tower, Bombard Tower, Outpost, plotted walls, and Gatehouse conversion
- **Units** — 8 types: Worker, Soldier, Archer (ranged), Tank, Knight, Mage, Priest, Cavalry — trained from buildings. Soldiers can upgrade to Knights
- **Siege** — 2 siege units: Catapult (long-range AoE) and Battering Ram (melee anti-structure)
- **Abilities** — Knights (Charge, Shield Bash), Mages (Fireball, Frost Nova), Priests (Heal, Holy Smite), Catapults (Boulder Throw)
- **Summons** — Skeleton Minion, Spirit Wolf, Fire Elemental
- **Enemies** — 4 mob camps (Goblin, Skeleton, Orc, Demon) with patrol AI, aggro detection, and boss variants
- **Combat** — Melee and ranged attacks, stance-aware auto-targeting (Passive/Defensive/Aggressive), projectiles, hit-flash VFX, tower auto-attack, explosive props with chain reactions
- **Unit AI** — Decision priority system (0.2s tick): manual orders > survival retreat (hp <25%) > stance-based threat response > auto-role. Defensive leash (12u) returns units that chase too far
- **Economy** — 5 resource types (Wood, Copper, Iron, Gold, Oil) procedurally distributed across biomes with auto-gathering workers
- **Tree Growth** — Saplings spawn and grow through stages into harvestable mature trees over time
- **Day/Night Cycle** — 600-second animated cycle (Dawn/Day/Dusk/Night) with keyframed sun illuminance, color, pitch, ambient light, and sky color
- **Volumetric Fog** — Atmospheric fog volume on camera with density and color animated per time-of-day phase
- **Entity Lights** — Dynamic point lights spawned at entity clusters (buildings and units), intensity scales with day/night cycle (full at night, 30% during day)
- **Fog of War** — Texture-based fog of war with per-entity vision ranges, edge glow, noise overlay, and explored/unexplored tinting
- **Save/Load** — JSON game state serialization with stable entity IDs, cross-reference resolution, and resource node position matching
- **Minimap** — Interactive minimap with real-time unit and building positions, camera viewport indicator
- **Controls** — Click, box-select, shift-toggle selection; formation movement; contextual right-click (attack enemies, gather resources, assist builds, assign processors, move to allies); hotkey-based unit orders (A-move, Patrol, Hold, Stop, Stance cycle); Ctrl+1-9 control groups
- **Camera** — WASD pan, Q/E rotate, scroll zoom, edge-scroll
- **UI** — Widget-based HUD system with 12x8 snap-to-grid layout, closable/pinnable panels (F1-F10 toggles), integrated tile content styling (no nested panel shells), responsive action/build grids, resource bar, selection panel, production queue, army overview, tech tree, control groups, event log
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

### Camera

| Input | Action |
|---|---|
| W / A / S / D | Pan camera forward / left / back / right |
| Arrow keys | Pan camera (alternative) |
| Q / E | Rotate camera left / right |
| Scroll wheel | Zoom in / out |
| +/- keys | Zoom in / out (alternative) |
| Screen edges | Auto-pan when cursor reaches edge |

### Selection

| Input | Action |
|---|---|
| Left click | Select unit or building |
| Left drag | Box-select multiple units |
| Shift + click | Add / remove from selection |
| Ctrl + 1-9 | Assign selected units to control group |
| Shift + 1-9 | Add selected units to control group |
| 1-9 | Recall (select) control group |

### Unit Commands

| Input | Action |
|---|---|
| Right click ground | Move selected units (formation spread) |
| Right click enemy unit/building | Attack target |
| Right click resource | Gather (workers) / move (combat units) |
| Right click construction | Assign workers to build |
| Right click processor | Assign workers to processor building |
| Right click allied building | Move to building |
| A + left click | Attack-move to location (engage enemies on the way) |
| P + left click | Patrol to location |
| H | Hold position (stop and clear orders) |
| S | Stop (clear all orders, workers go idle) |
| V | Cycle unit stance: Passive → Defensive → Aggressive |
| Escape | Cancel attack-move / patrol mode |

### Building

| Input | Action |
|---|---|
| Click building button | Enter placement mode |
| Click `Found Base` | Enter first-base founding mode |
| Click `Wall` | Enter wall plotting mode |
| Click `Gatehouse` | Enter gate conversion mode |
| Left click (placing) | Confirm building placement |
| Left click (wall) | Set wall start / confirm wall end |
| Right click / Escape | Cancel building placement |
| Hover wall segment + left click | Replace with Gatehouse |
| Click rally point button | Enter rally point mode |
| Left click (rally mode) | Set rally point for trained units |
| Right click (rally mode) | Cancel rally point mode |

### UI Widgets

| Input | Action |
|---|---|
| F1 | Toggle Resources panel |
| F2 | Toggle Army Overview panel |
| F3 | Toggle Debug panel |
| F4 | Toggle Actions panel |
| F5 | Toggle Minimap panel |
| F6 | Toggle Production Queue panel |
| F7 | Toggle Tech Tree panel |
| F8 | Toggle Control Groups panel |
| F9 | Toggle Event Log panel |
| F10 | Toggle Debug widget |
| Click minimap | Pan camera to location |

## How to Play

1. You start with **2 Workers**, **no Base**, and a light stockpile of **200 Wood / 40 Copper / 20 Iron**
2. Use the **Settlement** action to **Found Base**
3. Place the Base at a strong starting position, then let workers construct it
4. Once the Base completes, early economy and fortification options unlock
5. Build **Storage**, **Barracks**, and early defenses like **Watch Tower** or **Outpost**
6. Use the **Wall** tool to plot a straight wall line in one gesture
7. Use **Gatehouse** to replace an owned wall segment and create a chokepoint opening
8. Select completed production buildings to train units or upgrade structures to level 3
9. **Watch Towers**, **Guard Towers**, **Ballista Towers**, and **Bombard Towers** fill different defensive roles
10. Send workers near resource nodes to auto-gather; resource processors and depots improve efficiency
11. Buildings can be demolished for a 50% refund after completion

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
| Watch Tower | 30W 20C 10I | 8s | Base | Cheap early anti-raider defense |
| Guard Tower | 55W 35C 30I | 11s | Base | Durable general-purpose tower |
| Ballista Tower | 60W 40C 55I | 14s | Workshop | Long-range anti-heavy / anti-siege tower |
| Bombard Tower | 70W 50C 40I 20G | 15s | Mage Tower | Splash-oriented tower for swarm defense |
| Outpost | 25W 10C | 6s | Base | Vision structure for map control and wall anchoring |
| Wall Segment | 12W 4C | 4s | Base | Built through wall plotting flow |
| Wall Post | 16W 6C | 5s | Base | Endpoint / junction support for plotted walls |
| Gatehouse | 45W 15C 20I | 10s | Base | Replaces a wall segment to create a fortified opening |
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
| Worker | 115 | 5.0 | 6 | 1.8 | 1.2s | — |
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
├── selection.rs      Click, box, shift selection + right-click commands + hotkey orders
├── ui/
│   ├── mod.rs                    UiPlugin, spawn_hud, compute_ui_mode
│   ├── widget_framework.rs       Widget/GridSlot/WidgetRegistry types, spawn_widget_frame()
│   ├── widget_toolbar.rs         Top toolbar with F1-F10 toggle buttons
│   ├── resources_widget.rs       Resource display (wood, copper, iron, gold, oil)
│   ├── selection_widget.rs       Selection panel (unit/building detail cards)
│   ├── actions_widget.rs         Action bar + categorized building grid
│   ├── production_queue_widget.rs Global training queue overview
│   ├── army_overview_widget.rs   Unit counts + idle worker badges
│   ├── tech_tree_widget.rs       Building prerequisite tree (built/available/locked)
│   ├── group_hotkeys_widget.rs   Ctrl+1-9 control groups display + keybinds
│   ├── event_log_widget.rs       Combat/construction/training event log
│   ├── animations.rs             UiFadeIn, UiFadeOut, UiSlideIn systems
│   ├── buttons.rs                All button handlers (build, train, upgrade, demolish, etc.)
│   ├── notifications.rs          Ally notification toasts
│   └── shared.rs                 Shared helpers (hp_color, spawn_hp_bar, format_cost)
├── model_assets.rs   Loads KayKit 3D models (characters, trees, rocks, props)
├── resources.rs      Biome-based resource node spawning, auto-gathering, tree growth
├── mobs.rs           Enemy camps, patrol / aggro / chase AI
├── unit_ai.rs        Unit AI decision layer, task queue, state executor, leash system
├── combat.rs         Melee and ranged attacks, auto-targeting, death + event logging
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
| `BlueprintPlugin` | Unified entity blueprint registry — units, economy buildings, defense towers, walls, gates, mobs, summons |
| `GroundPlugin` | Generates 500x500 heightmap mesh with biome-based vertex colors, inserts BiomeMap |
| `CameraPlugin` | WASD pan, scroll zoom, Q/E orbit camera |
| `LightingPlugin` | Day/night cycle with keyframed sun/ambient/sky, volumetric atmospheric fog, dynamic entity cluster lights |
| `UnitsPlugin` | Spawns 2 starting workers with no initial Base, handles movement and avoidance |
| `BuildingsPlugin` | Base founding, building placement preview, wall plotting, gate conversion, construction, training, fortification auto-attack, upgrades, demolish |
| `SelectionPlugin` | Click/box/shift selection, contextual right-click resolver, hotkey orders (A/P/H/S/V stance), control groups |
| `UiPlugin` | Widget-based HUD — 12x8 grid layout with closable/pinnable panels, building grid, production queue, army overview, tech tree, event log |
| `ModelAssetsPlugin` | Loads KayKit 3D models — Forest Nature Pack, Adventurers, Skeletons, Character Animations |
| `ResourcesPlugin` | Procedural biome-based resource node spawning with 3D models, auto-gather + deposit loop, tree growth, biome-aware decoration scatter |
| `MobsPlugin` | Spawns 4 enemy camps with patrol, aggro, chase, and return AI |
| `UnitAiPlugin` | Decision priority system (0.2s tick), task queue processing, unit state executor, defensive leash return |
| `CombatPlugin` | Melee/ranged attacks, stance-aware auto-acquire, explosive prop chain reactions, death cleanup |
| `FogPlugin` | Texture-based fog of war with per-entity vision ranges, edge glow, noise overlay |
| `MinimapPlugin` | Interactive 200x200 minimap with real-time entity tracking and camera viewport |
| `PathVisPlugin` | Terrain-following dashed path lines with destination ring |
| `VfxPlugin` | Projectile flight, melee flash, impact flash |
| `AnimationPlugin` | GLTF skeletal animation discovery, state-driven playback (Idle/Walk/Attack/Die), smooth directional facing |
| `SavePlugin` | JSON save/load with stable entity IDs, fortification transform persistence, two-pass world reconstruction, resource node position matching |
| `DebugPlugin` | F3 debug panel — organized Visuals/Entities/Game sections, real-time parameter tweaking, entity spawn/manipulate tools, JSON config persistence |

## Tech Stack

- **Rust** / **Bevy 0.18** / **noise 0.9** (fBm Perlin terrain + biome generation) / **bevy_mod_outline 0.12** (selection highlighting) / **serde + serde_json** (save system + debug config) / **rand 0.9** (procedural scatter)
