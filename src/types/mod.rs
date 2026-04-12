//! Domain-organized type modules for the RTS game.
//!
//! Each submodule groups related components, resources, and data types by domain.
//! All public items are re-exported here so that `use crate::types::*` (or the
//! backward-compatible `use crate::types::*`) gives access to everything.

pub mod app;
pub mod ai;
pub mod buildings;
pub mod combat;
pub mod core;
pub mod economy;
pub mod rendering;
pub mod rng;
pub mod ui;
pub mod units;

pub use ai::*;
pub use app::*;
pub use buildings::*;
pub use combat::*;
// `core` intentionally re-exports a subset already covered by `app::*`; the
// glob below is a no-op today but documents the canonical leaf import path.
#[allow(unused_imports)]
pub use core::*;
pub use economy::*;
pub use rendering::*;
pub use rng::*;
pub use ui::*;
pub use units::*;
