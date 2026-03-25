pub mod core;
pub mod widgets;

// Compatibility re-exports keep existing `crate::ui::*` paths working
#[allow(unused_imports)]
pub use core::{fonts, framework as widget_framework, shared};
#[allow(unused_imports)]
pub use widgets::{
    actions_widget, army_overview_widget, event_log_widget, group_hotkeys_widget, hints_widget,
    notifications, production_queue_widget, resources_widget, selection_widget,
    tech_tree_widget, widget_toolbar,
};

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

/// UI plugin group that composes all UI sub-plugins.
///
/// Each widget is a self-contained plugin that registers its own
/// resources, spawn system, and update systems.
pub struct UiPlugin;

impl PluginGroup for UiPlugin {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(core::UiCorePlugin)
            .add(widgets::WidgetsPlugin)
    }
}
