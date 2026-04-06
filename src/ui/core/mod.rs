pub mod animations;
pub mod button_visuals;
pub mod components;
pub mod constants;
pub mod fonts;
pub mod framework;
pub mod hud;
pub mod interactions;
pub mod shared;
pub mod text_input;
pub mod tooltips;

use bevy::prelude::*;

use crate::components::*;
use crate::database::GameDatabase;
use crate::selection::SelectionSet;

pub struct UiCorePlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum UiCoreSet {
    Interactions,
    Lifecycle,
    Mode,
    Overlay,
    Framework,
    Tooltips,
    Visuals,
    Animation,
    Scale,
}

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
            .init_resource::<framework::WidgetEditState>()
            .init_resource::<framework::ScrollConsumedByWidget>()
            .init_resource::<ControlGroupState>()
            .configure_sets(
                Update,
                (
                    UiCoreSet::Interactions,
                    UiCoreSet::Lifecycle,
                    UiCoreSet::Mode,
                    UiCoreSet::Overlay,
                    UiCoreSet::Framework,
                    UiCoreSet::Tooltips,
                    UiCoreSet::Visuals,
                    UiCoreSet::Animation,
                    UiCoreSet::Scale,
                )
                    .chain(),
            )
            .add_systems(
                PreUpdate,
                (
                    interactions::attach_interaction_state,
                    interactions::update_interaction_states,
                ),
            )
            // HUD lifecycle
            .add_systems(
                OnEnter(AppState::InGame),
                (hud::spawn_hud_roots, framework::spawn_grid_overlay)
                    .chain()
                    .in_set(UiCoreSet::Lifecycle),
            )
            // Compute UI mode after selection
            .add_systems(
                Update,
                (ApplyDeferred, hud::compute_ui_mode)
                    .chain()
                    .in_set(UiCoreSet::Mode)
                    .in_set(GameFlowSet::Ui)
                    .after(SelectionSet)
                    .run_if(in_state(AppState::InGame)),
            )
            // Placement hint
            .add_systems(
                Update,
                hud::update_placement_hint
                    .in_set(UiCoreSet::Overlay)
                    .in_set(GameFlowSet::Ui)
                    .run_if(in_state(AppState::InGame)),
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
                    .in_set(UiCoreSet::Overlay)
                    .in_set(GameFlowSet::Ui)
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
                    framework::update_edit_mode_visuals,
                    framework::dismiss_edit_mode,
                    framework::toggle_grid_overlay,
                )
                    .in_set(UiCoreSet::Framework)
                    .in_set(GameFlowSet::Ui)
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
                    .in_set(UiCoreSet::Tooltips)
                    .in_set(GameFlowSet::Ui)
                    .run_if(in_state(AppState::InGame)),
            )
            // Button visuals run in ALL states (menu + game)
            .add_systems(
                Update,
                (
                    button_visuals::button_hover_visual,
                    button_visuals::animated_button_chrome_system,
                    button_visuals::animated_button_hover_system,
                )
                    .in_set(UiCoreSet::Visuals)
                    .in_set(GameFlowSet::Ui),
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
                )
                    .in_set(UiCoreSet::Animation)
                    .in_set(GameFlowSet::Ui),
            )
            // UI scale runs in ALL states
            .add_systems(
                Update,
                hud::update_ui_scale
                    .in_set(UiCoreSet::Scale)
                    .in_set(GameFlowSet::Ui),
            )
            // Font fallback in PostUpdate
            .add_systems(PostUpdate, fonts::apply_default_fonts)
            // Load saved widget layout from DB
            .add_systems(Startup, load_widget_layout_from_db);
    }
}

fn load_widget_layout_from_db(db: Res<GameDatabase>, mut registry: ResMut<framework::WidgetRegistry>) {
    if let Some(saved) = db.load_settings_blob::<framework::WidgetRegistry>("widget_layout") {
        *registry = saved;
    }
}
