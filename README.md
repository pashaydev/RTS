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
./scripts/build-macos.sh
```

### Debug
```sh
# In separate terminal run capture
tracy-capture -o trace.tracy
# Run bevy with tracy feature
cargo run --features tracy
# For memory allocation tracking too:
cargo run --features tracy_memory
# After play session open tracy UI with
tracy trace.tracy
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
./scripts/deploy.sh                        # bump patch + Windows build + macOS build + Fly.io deploy
./scripts/deploy.sh --windows-only         # bump patch + Windows zip only
./scripts/deploy.sh --macos-only           # bump patch + macOS zip only
./scripts/deploy.sh --fly-only             # bump patch + Web deploy only
./scripts/deploy.sh --minor --windows-only # bump minor + Windows zip only
./scripts/deploy.sh --minor --macos-only   # bump minor + macOS zip only
./scripts/deploy.sh --major --fly-only     # bump major + Web deploy only
```

**Prerequisites:**

| Target  | Requirement |
|---------|-------------|
| Windows | `cargo install cargo-xwin` + LLVM (`/opt/homebrew/opt/llvm`) |
| macOS   | `rustup target add aarch64-apple-darwin` (or set `MACOS_TARGET` to another Rust target triple) |
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

## Architecture

The codebase is organized into six domain-driven PluginGroups, plus shared types and a separate protocol crate for networked state and messages.


## Tech Stack

- Rust
- Bevy 0.18
- `bevy_mod_outline`
- `serde` / `serde_json` / `rmp-serde` (MessagePack binary codec)
- `bevy_matchbox` (WebRTC transport with embedded signaling server)
- `rusqlite` (SQLite persistence for profiles, match history, ELO, settings)
