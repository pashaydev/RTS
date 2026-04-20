//! Data-driven entity definitions. Re-exports the blueprint asset loader,
//! registry builder, spawn helpers, and visual cache.

pub mod asset;
mod plugin;
mod registry;
mod spawn;
mod types;
mod visuals;

pub use plugin::BlueprintPlugin;
pub use registry::build_registry;
pub use spawn::{spawn_from_blueprint, spawn_from_blueprint_with_faction};
pub use types::*;
pub use visuals::build_visual_cache;
