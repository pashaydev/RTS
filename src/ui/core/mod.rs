pub mod animations;
pub mod button_visuals;
pub mod components;
pub mod fonts;
pub mod framework;
pub mod hud;
pub mod interactions;
pub mod shared;
pub mod text_input;
pub mod tooltips;

use bevy::prelude::*;

use crate::components::*;
use crate::selection::SelectionSet;

pub struct UiCorePlugin;

impl Plugin for UiCorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RallyPointMode>()
            .init_resource::<UiMode>()
            .init_resource::<fonts::UiFonts>()
            .add_message::<interactions::UiClickEvent>()
            .add_message::<interactions::UiHoldCompleteEvent>()
            .init_resource::<framework::WidgetRegistry>()
            .init_resource::<framework::WidgetResizeState>()
            .init_resource::<framework::WidgetDragState>()
            .init_resource::<framework::GridInteractionActive>()
            .init_resource::<ControlGroupState>()
            .add_systems(
                PreUpdate,
                (
                    interactions::attach_interaction_state,
                    interactions::update_interaction_states,
                ),
            )
            // HUD lifecycle
            .add_systems(OnEnter(AppState::InGame), hud::mark_pending_ui_spawn)
            .add_systems(
                Update,
                (hud::spawn_hud_roots, framework::spawn_grid_overlay, hud::clear_pending_ui_spawn)
                    .chain()
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<hud::PendingUiSpawn>),
            )
            // Compute UI mode after selection
            .add_systems(
                Update,
                (ApplyDeferred, hud::compute_ui_mode)
                    .chain()
                    .after(SelectionSet)
                    .run_if(in_state(AppState::InGame)),
            )
            // Placement hint
            .add_systems(
                Update,
                hud::update_placement_hint.run_if(in_state(AppState::InGame)),
            )
            // Overlay adoption
            .add_systems(
                Update,
                (
                    ApplyDeferred,
                    hud::adopt_back_overlay_items,
                    hud::adopt_front_overlay_items,
                )
                    .chain()
                    .in_set(OverlayLifecycleSet::Adopt)
                    .after(OverlayLifecycleSet::Manage)
                    .run_if(in_state(AppState::InGame)),
            )
            // Widget framework systems
            .add_systems(
                Update,
                (
                    framework::sync_widget_visibility,
                    framework::handle_widget_buttons,
                    framework::handle_widget_drag,
                    framework::handle_widget_scroll,
                    framework::handle_widget_resize,
                    framework::update_resize_handle_visuals,
                    framework::toggle_grid_overlay,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            // Tooltip systems
            .add_systems(
                Update,
                (
                    tooltips::show_action_tooltips,
                    tooltips::update_action_tooltip_positions,
                    tooltips::cleanup_action_tooltips,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            // Button visuals run in ALL states (menu + game)
            .add_systems(
                Update,
                (
                    button_visuals::button_hover_visual,
                    button_visuals::animated_button_chrome_system,
                    button_visuals::animated_button_hover_system,
                ),
            )
            // Animation systems run in ALL states
            .add_systems(
                Update,
                (
                    animations::ui_fade_system,
                    animations::ui_slide_system,
                    animations::ui_scale_in_system,
                    animations::ui_line_expand_system,
                    animations::menu_particle_system,
                    animations::title_shimmer_system,
                    animations::ui_glow_pulse_system,
                ),
            )
            // UI scale runs in ALL states
            .add_systems(Update, hud::update_ui_scale)
            // Font fallback in PostUpdate
            .add_systems(PostUpdate, fonts::apply_default_fonts);
    }
}
