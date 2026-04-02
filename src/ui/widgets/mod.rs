pub(crate) mod buttons;

pub mod actions_widget;
pub mod army_overview_widget;
pub mod event_log_widget;
pub mod group_hotkeys_widget;
pub mod hints_widget;
pub mod notifications;
pub mod production_queue_widget;
pub mod resources_widget;
pub mod selection_widget;
pub mod tech_tree_widget;
pub mod widget_toolbar;

pub use super::core;
pub use super::core::framework as widget_framework;

use bevy::prelude::*;

use crate::components::AppState;

use core::fonts::UiFonts;
use core::framework::{spawn_widget_frame, WidgetId, WidgetRegistry};
use core::hud::MainHudRoot;

pub struct WidgetsPlugin;

impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            resources_widget::ResourcesWidgetPlugin,
            army_overview_widget::ArmyOverviewWidgetPlugin,
            tech_tree_widget::TechTreeWidgetPlugin,
            hints_widget::HintsWidgetPlugin,
            notifications::NotificationsWidgetPlugin,
            widget_toolbar::WidgetToolbarPlugin,
            event_log_widget::EventLogWidgetPlugin,
            group_hotkeys_widget::GroupHotkeysWidgetPlugin,
            production_queue_widget::ProductionQueueWidgetPlugin,
            selection_widget::SelectionWidgetPlugin,
            actions_widget::ActionsWidgetPlugin,
            ExternalWidgetFramesPlugin,
        ));
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
                .run_if(any_with_component::<MainHudRoot>),
        );
    }
}

fn spawn_external_widget_frames(
    mut commands: Commands,
    registry: Res<WidgetRegistry>,
    fonts: Res<UiFonts>,
    theme: Res<crate::theme::Theme>,
    root_q: Query<Entity, Added<MainHudRoot>>,
) {
    let Ok(hud_root) = root_q.single() else {
        return;
    };

    let minimap_content = spawn_widget_frame(
        &mut commands,
        hud_root,
        WidgetId::Minimap,
        registry.slots.get(&WidgetId::Minimap).unwrap(),
        registry.is_visible(WidgetId::Minimap),
        &fonts,
        &theme,
    );
    commands
        .entity(minimap_content)
        .insert(crate::minimap::MinimapWidgetContent);

    let debug_content = spawn_widget_frame(
        &mut commands,
        hud_root,
        WidgetId::Debug,
        registry.slots.get(&WidgetId::Debug).unwrap(),
        registry.is_visible(WidgetId::Debug),
        &fonts,
        &theme,
    );
    crate::debug::spawn_debug_content(&mut commands, debug_content);
}
