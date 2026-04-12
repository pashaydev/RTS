//! Leaf-only primitive types shared across every other `types` submodule.
//!
//! Architectural rule: this module must not import from any sibling
//! `types::*` submodule. It is the single root that every other submodule
//! may depend on. Cycles between `types::*` submodules should resolve by
//! promoting the shared type here, not by adding a cross-submodule `use`.
//!
//! Today `core` re-exports primitives that still physically live in
//! [`super::app`]; the long-term goal is to move their definitions here so
//! this becomes a genuine leaf. Callers outside `types` should prefer
//! `crate::types::core::Foo` over `crate::types::Foo` for clarity.

#[allow(unused_imports)]
pub use super::app::{
    AppState, Faction, GameFlowSet, MapSeed, OverlayLifecycleSet, SimClock, SimSet, TeamColor,
    TeamConfig,
};
