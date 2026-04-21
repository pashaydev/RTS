//! In-game HUD composition.
//!
//! This module currently contains three different concerns:
//!
//! - Dockable widget panels backed by [`WidgetId`] and the grid framework.
//! - Global HUD bars / overlays that are anchored to the HUD root instead of a
//!   widget slot.
//! - Internal support modules used by those panels.
//!
//! Keeping those categories explicit makes naming less misleading. A "widget"
//! in the docking sense should map to a [`WidgetId`], while header bars,
//! banners, toasts, and onboarding overlays should be named after their HUD
//! role rather than forced into the widget vocabulary.

/// Generates a system that spawns a widget frame when `WidgetGridArea` is added.
/// Widgets are parented to the grid area (below the header bar).
macro_rules! widget_spawn_system {
    ($fn_name:ident, $widget_id:expr) => {
        fn $fn_name(
            mut commands: Commands,
            registry: Res<super::core::framework::WidgetRegistry>,
            fonts: Res<super::core::fonts::UiFonts>,
            theme: Res<$crate::ui::theme::Theme>,
            grid_q: Query<Entity, Added<super::core::hud::WidgetGridArea>>,
        ) {
            let Ok(grid_area) = grid_q.single() else {
                return;
            };
            super::core::framework::spawn_widget_frame(
                &mut commands,
                grid_area,
                $widget_id,
                registry.slots.get(&$widget_id).unwrap(),
                registry.is_visible($widget_id),
                &fonts,
                &theme,
            );
        }
    };
    ($fn_name:ident, $widget_id:expr, |$cmd:ident, $content:ident| $body:expr) => {
        fn $fn_name(
            mut commands: Commands,
            registry: Res<super::core::framework::WidgetRegistry>,
            fonts: Res<super::core::fonts::UiFonts>,
            theme: Res<$crate::ui::theme::Theme>,
            grid_q: Query<Entity, Added<super::core::hud::WidgetGridArea>>,
        ) {
            let Ok(grid_area) = grid_q.single() else {
                return;
            };
            let $content = super::core::framework::spawn_widget_frame(
                &mut commands,
                grid_area,
                $widget_id,
                registry.slots.get(&$widget_id).unwrap(),
                registry.is_visible($widget_id),
                &fonts,
                &theme,
            );
            let $cmd = &mut commands;
            $body
        }
    };
}

pub(crate) mod buttons;

// Dockable panels.
pub mod action_panel;
pub mod army_overview_widget;
pub mod event_log_widget;
pub mod group_hotkeys_widget;
pub mod production_queue_widget;
pub mod selection_panel;
pub mod tech_tree_widget;

// Global HUD sections / overlays.
pub mod hud_header;
pub mod notification_toasts;
pub mod onboarding_hints;
pub mod wave_alerts_overlay;
pub mod widget_visibility_toolbar;

// Panel-internal support modules.
pub mod building_action_panel;
pub mod selection_panel_cards;
pub mod unit_action_panel;

pub use super::core;
pub use super::core::framework as widget_framework;

use bevy::prelude::*;

use crate::types::AppState;

use core::fonts::UiFonts;
use core::framework::{spawn_widget_frame, WidgetId, WidgetRegistry};
use core::hud::WidgetGridArea;

pub struct WidgetsPlugin;

impl Plugin for WidgetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            // Root-anchored HUD chrome.
            hud_header::HudHeaderBarPlugin,
            onboarding_hints::OnboardingHintsPlugin,
            notification_toasts::NotificationToastsPlugin,
            widget_visibility_toolbar::WidgetVisibilityToolbarPlugin,
            wave_alerts_overlay::WaveAlertsOverlayPlugin,

            // Dockable widget panels.
            army_overview_widget::ArmyOverviewWidgetPlugin,
            tech_tree_widget::TechTreeWidgetPlugin,
            event_log_widget::EventLogWidgetPlugin,
            group_hotkeys_widget::GroupHotkeysWidgetPlugin,
            production_queue_widget::ProductionQueueWidgetPlugin,
            selection_panel::SelectionPanelPlugin,
            action_panel::ActionPanelPlugin,
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
                .run_if(any_with_component::<WidgetGridArea>),
        );
    }
}

fn spawn_external_widget_frames(
    mut commands: Commands,
    registry: Res<WidgetRegistry>,
    fonts: Res<UiFonts>,
    theme: Res<crate::ui::theme::Theme>,
    grid_q: Query<Entity, Added<WidgetGridArea>>,
) {
    let Ok(grid_area) = grid_q.single() else {
        return;
    };

    let minimap_content = spawn_widget_frame(
        &mut commands,
        grid_area,
        WidgetId::Minimap,
        registry.slots.get(&WidgetId::Minimap).unwrap(),
        registry.is_visible(WidgetId::Minimap),
        &fonts,
        &theme,
    );
    commands
        .entity(minimap_content)
        .insert(crate::presentation::minimap::MinimapWidgetContent);

    let debug_content = spawn_widget_frame(
        &mut commands,
        grid_area,
        WidgetId::Debug,
        registry.slots.get(&WidgetId::Debug).unwrap(),
        registry.is_visible(WidgetId::Debug),
        &fonts,
        &theme,
    );
    crate::infrastructure::debug::spawn_debug_content(&mut commands, debug_content);
}
