//! `UiPlugin`: composes the UI layer — core (shared, every state), menu
//! (MainMenu), widgets (InGame HUD), and attention overlays.

pub mod attention;
pub mod core;
pub mod menu;
pub mod theme;
pub mod widgets;

#[allow(unused_imports)]
pub use core::{fonts, framework as widget_framework, shared};

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

/// UI plugin group composing all UI sub-plugins.
///
/// Three-way split by lifecycle:
///
/// - **Shared** ([`core::UiCorePlugin`]): framework, theme, interactions,
///   animations, tooltips, mode — runs in every [`crate::types::AppState`]
///   because both the main menu and the in-game HUD depend on it.
/// - **Menu** ([`menu::MenuPlugin`]): title, options, new-game, pause,
///   multiplayer lobby — gated on [`crate::types::AppState::MainMenu`].
///   Gating is applied per-system inside `menu::MenuPlugin` so the plugin
///   set stays composable.
/// - **Runtime** ([`widgets::WidgetsPlugin`], [`attention::AttentionPlugin`]):
///   dockable panels (selection, actions, production queue, army overview,
///   tech tree, event log), plus HUD-root overlays and bars (header,
///   onboarding hints, notifications, wave alerts) — gated on
///   [`crate::types::AppState::InGame`], again per-system inside the
///   runtime plugins.
///
/// Each sub-plugin already owns its own state gating; this comment documents
/// the invariant so future work can add plugin-level `configure_sets` with
/// state run_if once Bevy exposes that cleanly for `PluginGroup`s.
pub struct UiPlugin;

impl PluginGroup for UiPlugin {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            // Shared (runs in every state).
            .add(core::UiCorePlugin)
            // In-game runtime widgets (self-gated on InGame).
            .add(widgets::WidgetsPlugin)
            .add(attention::AttentionPlugin)
            // Main menu (self-gated on MainMenu).
            .add(menu::MenuPlugin)
    }
}
