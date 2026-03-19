# RTS Prototype

A 3D RTS prototype built with [Bevy](https://bevyengine.org/) 0.18. The project focuses on settlement-first progression, biome-driven maps, layered economy, combined-arms combat, strong in-game tooling, and a playable host-authoritative LAN multiplayer path.

## Overview

- Procedural maps with five biomes, distributed resources, decoration, fog of war, minimap, and terrain-wear roads
- Settlement-first macro loop: start with workers, found a base, unlock production, fortify, and scale
- Economy with raw and processed resources, worker assignment, recipes, storage, and building upgrades
- Combined-arms roster with infantry, ranged, cavalry, siege, casters, towers, walls, and gatehouses
- Skirmish configuration for AI count, AI difficulty, teams, map size, resource density, day length, seed, and player color
- LAN multiplayer with host simulation, client command relay, delta-compressed state sync, entity and resource node replication, and 30s reconnection grace before AI takeover

## Quick Start

### Requirements

- [Rust toolchain](https://rustup.rs/)

### Native

```sh
cargo run
```

### Web

```sh
trunk serve --config .trunk.toml
```

### Windows
```sh
PATH="/tmp:/opt/homebrew/opt/llvm/bin:$PATH" cargo xwin build --release --target x86_64-pc-windows-msvc
```

### MacOS
```sh
rustup target add aarch64-apple-darwin
cargo build --release --target aarch64-apple-darwin
```

The dev profile uses dependency optimization (`opt-level = 2`) for better iteration-time performance.

## Testing

### Native

Run the full native test suite:

```sh
cargo test
```

Run only the multiplayer-focused native tests:

```sh
cargo test multiplayer -- --nocapture
```

### WASM

Compile the wasm-targeted tests:

```sh
cargo test --target wasm32-unknown-unknown --no-run multiplayer
```

Run the wasm-specific multiplayer tests under the wasm bindgen runner:

```sh
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
  cargo test --target wasm32-unknown-unknown wasm_tests -- --nocapture
```

### Notes

- The wasm test flow requires the `wasm32-unknown-unknown` Rust target.
- `wasm-bindgen-test-runner` is used to execute the generated `.wasm` test binary instead of trying to run it directly as a native executable.
- The current multiplayer test coverage includes native host/client transport and systems plus wasm-side WebSocket payload encoding and decoding paths.

### Fly.io

The Fly.io deployment now uses a native `session_router` process that serves the generated `dist/` output and exposes hosted-session routing endpoints from the same origin.

- It serves the browser client and same-origin hosted-session API from one app
- Web clients now resolve hosted session codes to `/session/<code>/ws` on the same origin
- The hosted-session router is in place, but full internet-capable match hosting still needs host registration and target machine bootstrapping

## Core Gameplay

### Match Flow

1. Start with 2 workers and no base
2. Use `Found Base` to establish the first settlement
3. Expand gathering, processing, storage, and production
4. Unlock stronger buildings, upgrades, units, and defensive structures
5. Pressure camps or enemy factions while securing terrain with walls and towers

### Economy

- Raw resources: Wood, Copper, Iron, Gold, Oil
- Processed resources: Planks, Charcoal, Bronze, Steel, Gunpowder
- Workers can gather directly or be assigned to processor buildings
- Buildings can run recipes, buffer resources, and scale throughput through upgrades and staffing

### Combat

- Roster: Worker, Soldier, Archer, Tank, Knight, Mage, Priest, Cavalry, Catapult, Battering Ram
- Units support formation movement, patrol, attack-move, hold position, stances, and queued tasks
- Abilities include charge, shield bash, fireball, frost nova, heal, holy smite, and siege attacks
- Enemy camps include Goblin, Skeleton, Orc, and Demon factions with patrol and aggro behavior

### World and Presentation

- 500x500 default world with Forest, Desert, Mud, Water, and Mountain biomes
- Skeletal animation with blend transitions and facing interpolation
- Day/night cycle, sun and ambient lighting control, entity lights, VFX, and attention overlays
- Frustum culling pauses off-screen animation work
- Terrain traffic gradually forms visible road wear between active structures and routes

### AI and Automation

- Up to 3 AI factions can fill remaining match seats
- AI runs strategy, economy, tactical, and military layers
- Friendly AI can respect allied space, gather, expand, scout, rally, defend, and attack
- Multiplayer disconnects enter a 30-second reconnection grace period before handing the faction to AI

## Match Setup

- AI opponents: `0-3`
- AI difficulty: per slot
- Team modes: `FFA`, `Teams`, `Custom`
- Map size: `Small 300`, `Medium 500`, `Large 700`
- Resource density: `Sparse`, `Normal`, `Dense`
- Day cycle length: `5`, `10`, or `20` minutes
- Starting resources: `0.5x`, `1x`, `2x`
- Map seed: fixed or random
- Player name and player color
- Graphics: resolution, fullscreen, shadow quality, entity lights, UI scale

## Multiplayer

### Current Status

The project has a playable LAN and VPN multiplayer path, plus the first production-oriented hosted-session routing pieces for browser deployment.

- Transport: TCP (native) and WebSocket (WASM) with 4-byte length-prefixed MessagePack binary framing (JSON fallback for legacy clients)
- Model: host runs the full simulation, clients send inputs and receive authoritative sync
- Lobby: native host game, join by direct session code (`IP:port`) for LAN/VPN, plus web-side hosted session code routing groundwork
- Replication: delta-compressed state sync at ~10Hz, entity spawn/despawn, building sync, resource node amounts via NeutralWorldDelta, player resources, and day/night cycle
- Recovery: 30-second reconnection grace period with session tokens before AI takeover
- VPN/Hamachi: auto-detects VPN adapters, shows all available IPs, TCP keepalive and app-level heartbeat prevent tunnel dropout
- Hosted-session router: same-origin `GET /session/<code>/ws`, `POST /api/sessions`, and `GET /api/sessions/<code>` endpoints via the `session_router` binary
- See [docs/multiplayer-architecture.md](docs/multiplayer-architecture.md) for the full protocol and system topology

### VPN / Hamachi Play

The multiplayer stack works through Hamachi, ZeroTier, WireGuard, and similar VPN tools:

1. All players install and join the same VPN network
2. Host opens `Multiplayer` → `Host Game`
3. The lobby shows all detected IPs — look for the one tagged **[VPN]** (green text)
4. Share that VPN IP with clients (the Copy button copies the displayed session code)
5. If the auto-detected VPN IP is wrong, clients can manually enter `HAMACHI_IP:7878`

The host binds on all interfaces (`0.0.0.0`), so any adapter — LAN, Hamachi, ZeroTier, WireGuard — will accept connections. TCP keepalive and a 5-second application ping keep the tunnel alive during idle periods.

### Current Limits

- `ggrs_matchbox` is scaffolding for a future rollback path, not the active transport
- Native transport uses raw TCP sockets; WASM clients use WebSocket (binary frames)
- Client commands are fire-and-forget with no rollback or server reconciliation
- The match model assumes four total faction seats shared between humans and AI
- No NAT traversal for direct native hosting — LAN or VPN only
- Hosted browser sessions are not end-to-end complete yet: the router exists, but host registration and per-session machine targeting still need to be wired

### Quick Start

#### Host

1. Open `Multiplayer`
2. Choose `Host Game`
3. Share the displayed session code
4. Start once players are connected

#### Client

Native / VPN:

1. Open `Multiplayer`
2. Choose `Join Game`
3. Enter the host code as `IP:port`
4. Wait for host start

Web / hosted-session path:

1. Open the deployed web client
2. Choose `Join Game`
3. Enter a hosted session code
4. The client connects to the same origin using `/session/<code>/ws`

The web UI rejects direct `IP:port` joins on HTTPS because browsers block insecure `ws://` connections from secure pages.

### Network Debug Tap

The local network debug tap exposes recent events over HTTP on `127.0.0.1`, defaulting to ports `8787-8795`.

- `GET /health`
- `GET /events`
- `GET /events?since=<id>`
- `POST /clear`

Use `RTS_NET_DEBUG_PORT` to pin the port.

## Persistence and Tooling

- Local save/load exists and serializes entities, fog exploration, resource nodes, explosive props, rally points, upgrades, and training state
- Save path: `saves/save.json`
- Graphics settings persist to `config/graphics_settings.json`
- Debug tweak values persist to `config/debug_tweaks.json`
- Save/load controls currently live inside the in-game debug flow

## Controls

### Camera

| Input | Action |
|---|---|
| `W / A / S / D` | Pan camera |
| Arrow keys | Alternate pan |
| `Q / E` | Rotate camera |
| Scroll wheel | Zoom |
| `+ / -` | Alternate zoom |
| Screen edges | Edge-scroll |

### Selection

| Input | Action |
|---|---|
| Left click | Select unit or building |
| Left drag | Box-select units |
| `Shift` + click | Add or remove from selection |
| `Ctrl + 1-9` | Assign control group |
| `Shift + 1-9` | Add to control group |
| `1-9` | Recall control group |

### Orders

| Input | Action |
|---|---|
| Right click ground | Move |
| Right click enemy | Attack |
| Right click resource | Gather with workers |
| Right click construction | Assign workers to build |
| Right click processor | Assign workers to processor |
| `A` + left click | Attack-move |
| `P` + left click | Patrol |
| `H` | Hold position |
| `S` | Stop |
| `V` | Cycle stance |
| `Escape` | Cancel command mode |

### Building and UI

| Input | Action |
|---|---|
| Build button | Enter placement preview |
| `Found Base` | Start first-base placement |
| `Wall` | Start wall plotting |
| `Gatehouse` | Convert wall segment |
| Left click | Confirm placement |
| Right click / `Escape` | Cancel placement |
| Rally button | Enter rally mode |
| `F1` | Resources |
| `F2` | Army Overview |
| `F3` | Debug |
| `F4` | Actions |
| `F5` | Minimap |
| `F6` | Production Queue |
| `F7` | Tech Tree |
| `F8` | Control Groups |
| `F9` | Event Log |
| `F10` | Debug widget |

## Reference Data

### Biomes

| Biome | Terrain Color | Primary Resource | Secondary Resource |
|---|---|---|---|
| Forest | Green | Wood | Copper |
| Desert | Sandy yellow | Copper | Gold |
| Mud/Dirt | Brown | Iron | Copper |
| Water | Blue | Oil | — |
| Mountain | Gray/white | Gold | Iron |

### Buildings

| Type | Cost | Build Time | Requires | Function |
|---|---|---|---|---|
| Base | 90W 15I | 15s | — | Tier 1 anchor, trains Workers |
| Barracks | 75W 30I | 12s | Base | Trains Workers and Soldiers |
| Storage | 55W 15I | 8s | Base | Depot with gather aura |
| Sawmill | 50W 15I | 12s | Base | Produces Planks and Charcoal |
| Mine | 70W 35I | 15s | Base | Ore processing and upgrades |
| Outpost | 20W 10I | 6s | Base | Vision and wall-control anchor |
| Watch Tower | 35W 15I | 8s | Base | Early defense |
| Wall Segment | 12W | 4s | Base | Plotted wall section |
| Wall Post | 16W | 5s | Base | Wall endpoint or junction |
| Gatehouse | 40W 10C 35I | 10s | Outpost | Replaces a wall segment |
| Workshop | 90W 25C 55I 15G 10Bronze | 18s | Mine | Tier 2 military tech |
| Stable | 85W 30C 45I | 14s | Barracks | Cavalry production |
| Guard Tower | 60W 20C 45I | 11s | Barracks | Durable general defense |
| Siege Works | 100W 35C 90I 30G | 20s | Workshop | Siege production |
| Mage Tower | 80W 30C 40I 55G | 20s | Workshop | Mage and Priest production |
| Temple | 90W 20C 40I 70G | 22s | Mage Tower | Priest training and healing aura |
| Ballista Tower | 70W 55C 80I | 14s | Siege Works | Anti-heavy tower |
| Bombard Tower | 85W 45C 65I 35G | 15s | Mage Tower | Splash tower |
| Smelter | 80W 20C 40I | 16s | Mine | Produces Bronze and Steel |
| Alchemist | 60W 30I 25G 15O | 18s | Smelter | Produces Gunpowder |
| Oil Rig | 75W 25C 35I | 14s | Workshop | Water-biome oil processing |

### Training Costs

| Unit | Cost | Train Time | Trained At |
|---|---|---|---|
| Worker | 30W | 5s | Base, Barracks |
| Soldier | 20W 15I | 8s | Barracks |
| Archer | 25W 10I | 7s | Barracks |
| Tank | 20C 50I 15G 5O 5Steel | 15s | Workshop |
| Knight | 20W 15C 45I 20G 5Bronze | 12s | Stable |
| Mage | 10W 40G | 15s | Mage Tower |
| Priest | 15W 30G | 12s | Mage Tower, Temple |
| Cavalry | 25W 10C 25I 10G | 10s | Stable |
| Catapult | 80W 60I 20G 5Gunpowder | 20s | Siege Works |
| Battering Ram | 100W 40I 15Planks | 18s | Siege Works |

### Unit Stats

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

## Architecture

The codebase is organized as Bevy plugins around runtime domains, with a separate shared protocol crate for networked state and messages.

### Runtime Areas

- `menu`, `pause_menu`, `theme`: shell flow, skirmish setup, options, and in-session overlays
- `multiplayer`: LAN transport, lobby state, host/client systems, debug tap, and rollback scaffolding
- `game_state`: shared protocol crate for serialized messages, MessagePack codec, and replicated gameplay data
- `net_bridge`: stable network IDs and ECS/network mapping
- `components`, `blueprints`, `orders`, `selection`, `spatial`: shared gameplay state, entity typing, commands, and world queries
- `units`, `buildings`, `resources`, `combat`, `unit_ai`, `mobs`, `ai`, `pathfinding`: simulation and faction behavior
- `ground`, `lighting`, `fog`, `fog_material`, `hover_material`, `camera`, `minimap`, `pathvis`, `roads`, `attention`, `animation`, `vfx`, `culling`, `model_assets`: rendering, asset loading, feedback, and performance
- `ui`: HUD widgets, widgets framework, notifications, and action surfaces
- `debug`, `save`: local tooling, tweak flows, persistence, and restoration

## Tech Stack

- Rust
- Bevy 0.18
- `bevy_mod_outline`
- `serde` / `serde_json` / `rmp-serde` (MessagePack binary codec)
- `bevy_matchbox` and `bevy_ggrs` (rollback scaffolding, not the active transport)
