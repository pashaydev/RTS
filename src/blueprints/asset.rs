//! Blueprint asset scaffold (RON migration target).
//!
//! Today `registry::build_registry` hard-codes every unit, building and mob
//! blueprint as Rust literals. Balance tweaks require recompiles and the
//! registry file has grown to dominate build times when it changes.
//!
//! This file is the landing zone for the data-driven replacement:
//!
//! ```text
//! assets/blueprints/
//!   units.ron        # Worker, Soldier, Archer, Tank, …
//!   buildings.ron    # Base, Barracks, Sawmill, Smelter, …
//!   mobs.ron         # Goblin, Skeleton, Orc, Demon
//!   siege.ron        # Catapult, BatteringRam
//! ```
//!
//! A [`BlueprintAsset`] mirrors the runtime `Blueprint` struct with
//! `serde::Deserialize` derived, a [`bevy::asset::AssetLoader`] reads each
//! RON file into a handle, and at `OnEnter(AppState::InGame)` the loaded
//! assets are flattened into the existing [`BlueprintRegistry`] resource —
//! so every downstream caller keeps the same API.
//!
//! Opt-in hot reload (dev builds only) picks up balance tweaks without a
//! restart by watching `assets/blueprints/` for changes.
//!
//! Follow-up work to finish the migration:
//!
//! 1. Derive `Deserialize` on every `Stats` sub-struct referenced here —
//!    many already have it for savegame support, some need `#[serde(default)]`.
//! 2. Add `bevy_common_assets` (or hand-roll a minimal RON loader) as the
//!    `AssetLoader`.
//! 3. Generate the initial RON files by serializing the current
//!    `build_registry` output once — keeps balance identical.
//! 4. Delete the Rust literals in `registry.rs` and replace with a thin
//!    loader that consumes the loaded assets.
//! 5. Gate `watch_for_changes` behind `#[cfg(debug_assertions)]`.

use serde::Deserialize;

use crate::blueprints::EntityKind;

/// Serde-deserializable mirror of the runtime `Blueprint` struct.
///
/// Matches the field layout of `blueprints::types::Blueprint` so conversion
/// is a field-for-field copy. Kept in a sibling module to avoid polluting
/// the runtime types with `#[serde(default)]` noise.
#[derive(Debug, Clone, Deserialize)]
pub struct BlueprintAsset {
    pub kind: EntityKind,
    // Runtime stats sub-structs plug in here once their `Deserialize`
    // impls stabilize. Left intentionally minimal until migration starts.
}

/// Top-level RON document shape: a file contains many blueprints keyed by
/// `EntityKind`. One file per category (units/buildings/mobs/siege).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct BlueprintDocument {
    pub blueprints: Vec<BlueprintAsset>,
}
