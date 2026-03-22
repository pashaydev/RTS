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

The codebase is organized as Bevy plugins around runtime domains, with a separate shared protocol crate for networked state and messages.

### Runtime Areas

- `menu`, `pause_menu`, `theme`: shell flow, skirmish setup, options, and in-session overlays
- `multiplayer`: Matchbox WebRTC transport, lobby state, host/client systems, built-in HTTP file server, debug tap
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
- `bevy_matchbox` (WebRTC transport with embedded signaling server)
