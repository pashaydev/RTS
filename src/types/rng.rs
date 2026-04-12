//! Deterministic RNG resource for the fixed-step simulation.
//!
//! **Determinism rule** — every `rand::random()`, `rand::rng()`, and
//! `thread_rng()` call inside `src/simulation/` or `src/world/` is a
//! desync bug waiting to happen. Sim-path randomness must read and advance
//! [`GameRng`] instead, so the same seed produces the same sequence on
//! every machine. Callers outside the sim path (UI flavor text, VFX
//! variation, debug tools) may continue to use thread-local RNGs.
//!
//! The seed is taken from [`crate::types::MapSeed`] on match start so save
//! games, replays, and host/client runs stay in sync. Resetting the seed
//! on `OnEnter(AppState::InGame)` is the responsibility of whichever
//! system decides the seed (lobby host, save loader, AI test harness).
//!
//! Pass 10 of the architecture refactor wires this resource in but does
//! not yet rewrite every call site. Follow-up work:
//!
//! 1. Grep `src/simulation/` and `src/world/` for `rand::random`,
//!    `rand::rng`, `thread_rng` — each hit either routes through
//!    [`GameRng`] or moves out of the sim path.
//! 2. Switch replicated `HashMap`s (PreviousSnapshots, spatial cells,
//!    TeamConfig) to `BTreeMap` / `IndexMap` so iteration order matches.
//! 3. Add a feature-gated `sim_checksum_system` that hashes
//!    `(tick, sorted entity states)` every N ticks and compares against
//!    the host's checksum to flag desyncs.

#[cfg(feature = "client")]
use bevy::prelude::*;
#[cfg(feature = "client")]
use rand::rngs::StdRng;
#[cfg(feature = "client")]
use rand::SeedableRng;

/// Seeded, deterministic RNG scoped to the active match.
///
/// Prefer a dedicated local `StdRng` built from `GameRng::fork(tag)` when
/// a system wants independent sequences (e.g. per-faction AI decisions)
/// — see the `fork` method below.
#[cfg(feature = "client")]
#[derive(Resource, Debug)]
pub struct GameRng {
    pub seed: u64,
    pub rng: StdRng,
}

#[cfg(feature = "client")]
impl Default for GameRng {
    fn default() -> Self {
        Self::from_seed(0xC0FFEE_u64)
    }
}

#[cfg(feature = "client")]
impl GameRng {
    pub fn from_seed(seed: u64) -> Self {
        Self {
            seed,
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// Produce an independent child RNG that will be reproducible as long
    /// as the same `tag` is used at the same tick from the same parent
    /// seed. Useful for per-system substreams so one system's consumption
    /// can't shift another's sequence.
    pub fn fork(&self, tag: u64) -> StdRng {
        StdRng::seed_from_u64(self.seed ^ tag.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    /// Re-seed in place. Used on match start / load so every machine
    /// begins the sim from the same deterministic state.
    pub fn reseed(&mut self, seed: u64) {
        self.seed = seed;
        self.rng = StdRng::seed_from_u64(seed);
    }
}
