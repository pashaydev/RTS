# RTS Prototype

A 3D RTS prototype built with [Bevy](https://bevyengine.org/) 0.18. The project focuses on settlement-first progression, biome-driven maps, layered economy, combined-arms combat, strong in-game tooling, and deterministic lockstep multiplayer.

## Overview

- Procedural maps with five biomes, distributed resources, decoration, fog of war, minimap, and terrain-wear roads
- Settlement-first macro loop: start with workers, found a base, unlock production, fortify, and scale
- Economy with raw and processed resources, worker assignment, recipes, storage, and building upgrades
- Combined-arms roster with infantry, ranged, cavalry, siege, casters, towers, walls, and gatehouses
- Skirmish configuration for AI count, AI difficulty, teams, map size, resource density, day length, seed, and player color
- Deterministic lockstep multiplayer via Matchbox WebRTC with input synchronization, FNV-1a checksum-based desync detection, NAT traversal for internet play, and 30s reconnection grace before AI takeover


## Quick Start

### Requirements

- [Rust toolchain](https://rustup.rs/)

### Native

```sh
cargo run
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

## Deployment

Build and deploy with:

```sh
./scripts/deploy.sh                        # bump patch + Windows build + macOS build
./scripts/deploy.sh --windows-only         # bump patch + Windows zip only
./scripts/deploy.sh --macos-only           # bump patch + macOS zip only
./scripts/deploy.sh --minor --windows-only # bump minor + Windows zip only
./scripts/deploy.sh --minor --macos-only   # bump minor + macOS zip only
```

**Prerequisites:**

| Target  | Requirement |
|---------|-------------|
| Windows | `cargo install cargo-xwin` + LLVM (`/opt/homebrew/opt/llvm`) |
| macOS   | `rustup target add aarch64-apple-darwin` (or set `MACOS_TARGET` to another Rust target triple) |

## Multiplayer

### Quick Start

#### Host

1. Open `Multiplayer`
2. Choose `Host Game`
3. Share the displayed session code (signaling URL)
4. Start once players are connected

#### Client

1. Open `Multiplayer`
2. Choose `Join Game`
3. Enter the session code (signaling URL like `ws://IP:3536/rts_room` or just the host IP)
4. Wait for host start

All peers run the full deterministic simulation locally. Only player inputs are exchanged over the network. See [docs/mul-arch.md](docs/mul-arch.md) for architecture details.

## Architecture

The codebase is organized into six domain-driven PluginGroups, plus shared types and a separate protocol crate for networked state and messages.


## Tech Stack

- Rust
- Bevy 0.18
- `bevy_mod_outline`
- `serde` / `serde_json` / `rmp-serde` (MessagePack binary codec)
- `bevy_matchbox` (WebRTC transport with embedded signaling server)
- `rusqlite` (SQLite persistence for profiles, match history, ELO, settings)
