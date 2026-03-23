pub mod core;
pub mod widgets;

pub mod actions_widget;
pub mod army_overview_widget;
pub mod buttons;
pub mod event_log_widget;
pub mod group_hotkeys_widget;
pub mod hints_widget;
pub mod notifications;
pub mod production_queue_widget;
pub mod resources_widget;
pub mod selection_widget;
pub mod tech_tree_widget;
pub mod widget_toolbar;

// Re-export from core so existing `crate::ui::*` paths keep working
pub use core::fonts;
pub use core::framework as widget_framework;
pub use core::shared;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

use crate::components::*;

use core::framework::{spawn_widget_frame, WidgetId, WidgetRegistry};
use core::fonts::UiFonts;
use core::hud::HudReady;

/// UI plugin group that composes all UI sub-plugins.
///
/// Each widget is a self-contained plugin that registers its own
/// resources, spawn system, and update systems.
pub struct UiPlugin;

impl PluginGroup for UiPlugin {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(core::UiCorePlugin)
            .add(resources_widget::ResourcesWidgetPlugin)
            .add(army_overview_widget::ArmyOverviewWidgetPlugin)
            .add(tech_tree_widget::TechTreeWidgetPlugin)
            .add(hints_widget::HintsWidgetPlugin)
            .add(notifications::NotificationsWidgetPlugin)
            .add(widget_toolbar::WidgetToolbarPlugin)
            .add(event_log_widget::EventLogWidgetPlugin)
            .add(group_hotkeys_widget::GroupHotkeysWidgetPlugin)
            .add(production_queue_widget::ProductionQueueWidgetPlugin)
            .add(selection_widget::SelectionWidgetPlugin)
            .add(actions_widget::ActionsWidgetPlugin)
            .add(ExternalWidgetFramesPlugin)
    }
}

/// Spawns widget frames for widgets whose content is owned by external plugins
/// (minimap, debug). These plugins populate the content themselves.
struct ExternalWidgetFramesPlugin;

impl Plugin for ExternalWidgetFramesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            spawn_external_widget_frames
                .run_if(in_state(AppState::InGame))
                .run_if(resource_added::<HudReady>),
        );
    }
}

fn spawn_external_widget_frames(
    mut commands: Commands,
    registry: Res<WidgetRegistry>,
    fonts: Res<UiFonts>,
    hud_ready: Res<HudReady>,
) {
    let hud_root = hud_ready.hud_root;

    // Minimap widget (content populated by MinimapPlugin)
    let minimap_content = spawn_widget_frame(
        &mut commands,
        hud_root,
        WidgetId::Minimap,
        registry.slots.get(&WidgetId::Minimap).unwrap(),
        registry.is_visible(WidgetId::Minimap),
        &fonts,
    );
    commands
        .entity(minimap_content)
        .insert(crate::minimap::MinimapWidgetContent);

    // Debug widget
    let debug_content = spawn_widget_frame(
        &mut commands,
        hud_root,
        WidgetId::Debug,
        registry.slots.get(&WidgetId::Debug).unwrap(),
        registry.is_visible(WidgetId::Debug),
        &fonts,
    );
    crate::debug::spawn_debug_content(&mut commands, debug_content);
}
