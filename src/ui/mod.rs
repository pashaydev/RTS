pub mod attention;
pub mod core;
pub mod menu;
pub mod theme;
pub mod widgets;

// Compatibility re-exports keep existing `crate::ui::*` paths working
#[allow(unused_imports)]
pub use core::{fonts, framework as widget_framework, shared};
#[allow(unused_imports)]
pub use widgets::{
    actions_widget, army_overview_widget, event_log_widget, group_hotkeys_widget, hints_widget,
    notifications, production_queue_widget, resources_widget, selection_widget, tech_tree_widget,
    widget_toolbar,
};

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
///   resources header, actions, selection, production queue, army overview,
///   tech tree, event log, hints, notifications — gated on
///   [`crate::types::AppState::InGame`], again per-system inside the
///   widget plugins.
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
