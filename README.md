# RTS Prototype

A 3D RTS prototype built with [Bevy](https://bevyengine.org/) 0.18. The project focuses on settlement-first progression, biome-driven maps, layered economy, combined-arms combat, strong in-game tooling, and a playable host-authoritative LAN multiplayer path.

## Overview

- Procedural maps with five biomes, distributed resources, decoration, fog of war, minimap, and terrain-wear roads
- Settlement-first macro loop: start with workers, found a base, unlock production, fortify, and scale
- Economy with raw and processed resources, worker assignment, recipes, storage, and building upgrades
- Combined-arms roster with infantry, ranged, cavalry, siege, casters, towers, walls, and gatehouses
- Skirmish configuration for AI count, AI difficulty, teams, map size, resource density, day length, seed, and player color
- Multiplayer via Matchbox WebRTC with host simulation, client command relay, delta-compressed state sync, entity and resource node replication, NAT traversal for internet play, built-in web client hosting, and 30s reconnection grace before AI takeover

For gameplay details, controls, unit/building stats, and match setup options see [docs/gameplay.md](docs/gameplay.md).

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

## Deployment

Build and deploy everything with a single command:

```sh
./scripts/deploy.sh                        # bump patch + Windows build + Fly.io deploy
./scripts/deploy.sh --windows-only         # bump patch + Windows zip only
./scripts/deploy.sh --fly-only             # bump patch + Web deploy only
./scripts/deploy.sh --minor --windows-only # bump minor + Windows zip only
./scripts/deploy.sh --major --fly-only     # bump major + Web deploy only
```

**Prerequisites:**

| Target  | Requirement |
|---------|-------------|
| Windows | `cargo install cargo-xwin` + LLVM (`/opt/homebrew/opt/llvm`) |
| Web     | [flyctl](https://fly.io/docs/flyctl/install/) + `fly auth login` |

### Docker / Fly.io

The Dockerfile builds the WASM client with Trunk and serves it with nginx. This is suitable for hosting a downloadable web client, though for LAN multiplayer the native host can serve the client directly (see below).

## Multiplayer

### Quick Start

#### Host

1. Open `Multiplayer`
2. Choose `Host Game`
3. Share the displayed session code (signaling URL)
4. Start once players are connected

#### Client (Native or WASM)

1. Open `Multiplayer`
2. Choose `Join Game`
3. Enter the session code (signaling URL like `ws://IP:3536/rts_room` or just the host IP)
4. Wait for host start

#### Client (Web Browser on LAN)

1. Open the URL shown in the host lobby (e.g., `http://192.168.1.5:7880`)
2. Choose `Join Game`
3. Enter the host session code
4. Wait for host start

The host automatically serves the WASM client when a `dist/` directory is present. Web and native clients can play together in the same lobby.

For full multiplayer details (transport, replication, VPN setup, limits, debug tap) see [docs/gameplay.md#multiplayer](docs/gameplay.md#multiplayer) and [docs/multiplayer-architecture.md](docs/multiplayer-architecture.md).

## Architecture

The codebase is organized into six domain-driven PluginGroups, plus shared types and a separate protocol crate for networked state and messages.

```
src/
├── types/            Shared game types (app state, economy, combat, units, buildings, AI, UI, rendering)
│                       core.rs is the leaf primitives module; rng.rs hosts the
│                       seeded `GameRng` used by the deterministic sim path.
├── blueprints/       Entity definitions, spawn logic, visual cache
│                       asset.rs is the scaffold for the RON-backed blueprint
│                       loader that will replace the hard-coded registry.
├── world/            WorldPlugins — terrain, environment, spatial indexing
│   ├── ground/         Procedural terrain generation, biomes, borders, water
│   ├── fog.rs          Fog of war
│   ├── lighting.rs     Day/night cycle, ambient light
│   ├── culling.rs      Frustum and distance culling
│   ├── spatial.rs      Spatial hash grid for queries
│   └── pathfinding.rs  A* navigation
├── simulation/       SimulationPlugins — core gameplay logic
│   ├── units.rs        Unit spawning, movement, stances
│   ├── buildings/      Construction, training, upgrades, placement, walls
│   ├── combat/         Damage, intents, budget, engagement slots
│                         Emits `DamageApplied` messages at every damage
│                         chokepoint for observability and replication.
│   ├── resources/      Gathering, processing, worker assignment, trees
│                         worker_fsm.rs owns the canonical assign / unassign
│                         entry points for the worker state machine.
│   ├── selection/      Click/box selection, unit commands
│   ├── ai/             AI strategy, economy, military, tactics
│   ├── items/          Loot, equipment, VFX
│   ├── abilities.rs    Active abilities
│   ├── orders.rs       Command queue
│   ├── unit_ai.rs      Per-unit decision making
│   ├── mobs.rs         Neutral creatures
│   ├── ages.rs         Age/tech progression
│   └── victory.rs      Win/loss conditions, match recording
├── presentation/     PresentationPlugins — rendering, VFX, assets
│   ├── camera.rs       Camera controls and zoom
│   ├── animation.rs    Skeletal and procedural animation
│   ├── vfx.rs          Particle effects, projectiles
│   ├── model_assets.rs Model loading and caching
│   ├── minimap.rs      Minimap rendering
│   ├── pathvis.rs      Path visualization
│   ├── procedural_mobs.rs  Procedural mob meshes
│   ├── entity_labels.rs    Floating health bars, names
│   └── materials/      Custom shader materials (fog, grass, terrain, water, hover, tree occlusion)
├── infrastructure/   InfraPlugins — persistence, networking, debug
│   ├── database.rs     SQLite profiles, match history, ELO, settings
│   ├── save_load.rs    Game save/restore
│   ├── multiplayer/    WebRTC transport, host/client systems, replication, debug tap
│                         replication.rs hosts the `Replicated` trait + registry
│                         scaffold targeted by the per-component sync rewrite.
│   ├── net_bridge.rs   Network ID assignment and ECS/network mapping
│   ├── logging.rs      Session logging
│   ├── audio.rs        Sound effects and music
│   └── debug/          Debug overlay, tweaks, inspector
├── ui/               UiPlugin — HUD, menus, theming
│   ├── theme.rs        Color palettes, dark/light modes
│   ├── core/           Shared UI framework, fonts, text input, tooltips, animations
│   ├── widgets/        In-game HUD widgets (resources, selection, actions, minimap, etc.)
│   ├── menu/           Main menu, new game, options, multiplayer lobby, pause menu
│   └── attention.rs    Screen-edge alerts
└── game_state/       Shared protocol crate (MessagePack codec, replicated data)
```

## Tech Stack

- Rust
- Bevy 0.18
- `bevy_mod_outline`
- `serde` / `serde_json` / `rmp-serde` (MessagePack binary codec)
- `bevy_matchbox` (WebRTC transport with embedded signaling server)
- `rusqlite` (SQLite persistence for profiles, match history, ELO, settings)
