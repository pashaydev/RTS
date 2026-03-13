pub mod widget_framework;
pub mod widget_toolbar;
pub mod resources_widget;
pub mod selection_widget;
pub mod actions_widget;
pub mod production_queue_widget;
pub mod army_overview_widget;
pub mod tech_tree_widget;
pub mod group_hotkeys_widget;
pub mod event_log_widget;
pub mod animations;
pub mod buttons;
pub mod notifications;
pub mod shared;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::components::*;
use crate::selection::SelectionSet;
use crate::theme;

use widget_framework::{WidgetRegistry, spawn_widget_frame, WidgetId};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RallyPointMode>()
            .init_resource::<UiMode>()
            .init_resource::<WidgetRegistry>()
            .init_resource::<widget_framework::WidgetResizeState>()
            .init_resource::<widget_framework::WidgetDragState>()
            .init_resource::<widget_framework::GridInteractionActive>()
            .init_resource::<group_hotkeys_widget::ControlGroups>()
            .init_resource::<event_log_widget::GameEventLog>()
            .init_resource::<event_log_widget::EventLogRenderState>()
            .add_systems(OnEnter(AppState::InGame), (spawn_hud, widget_framework::spawn_grid_overlay))
            .add_systems(
                Update,
                (ApplyDeferred, compute_ui_mode)
                    .chain()
                    .after(SelectionSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(Update, update_placement_hint.run_if(in_state(AppState::InGame)))
            .add_systems(
                Update,
                (
                    resources_widget::update_resource_texts,
                    selection_widget::rebuild_selection_panel,
                    buttons::update_hp_bars,
                    buttons::handle_unit_card_click,
                    buttons::clear_stale_inspected,
                )
                    .after(compute_ui_mode)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(Update, actions_widget::update_action_bar.after(compute_ui_mode).run_if(in_state(AppState::InGame)))
            .add_systems(
                Update,
                (
                    buttons::handle_build_buttons,
                    buttons::handle_train_buttons,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    buttons::handle_upgrade_button,
                    buttons::handle_demolish_button,
                    buttons::handle_demolish_confirm,
                    buttons::handle_rally_point_button,
                    buttons::handle_toggle_auto_attack,
                    buttons::handle_cancel_train,
                    buttons::handle_assign_worker_button,
                    buttons::handle_unassign_worker_button,
                    buttons::handle_unassign_specific_worker_button,
                    buttons::update_training_queue_display,
                    buttons::update_construction_progress_display,
                    buttons::update_train_cost_colors,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    buttons::update_upgrade_progress_display,
                    // button_hover_visual and animated_button_hover_system run always (needed for menu)
                    buttons::action_bar_transition_system,
                    buttons::show_action_tooltips,
                    buttons::cleanup_action_tooltips,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            // These run in ALL states so menu buttons animate too
            .add_systems(
                Update,
                (
                    buttons::button_hover_visual,
                    buttons::animated_button_hover_system,
                ),
            )
            .add_systems(
                Update,
                (
                    widget_toolbar::widget_toolbar_system,
                    widget_toolbar::update_toolbar_visuals,
                    widget_framework::sync_widget_visibility,
                    widget_framework::handle_widget_buttons,
                    widget_framework::handle_widget_drag,
                    widget_framework::handle_widget_scroll,
                    widget_framework::handle_widget_resize,
                    widget_framework::update_resize_handle_visuals,
                    widget_framework::toggle_grid_overlay,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(Update, (notifications::update_ally_notifications, notifications::handle_notification_click).run_if(in_state(AppState::InGame)))
            .add_systems(
                Update,
                (
                    production_queue_widget::update_production_queue,
                    production_queue_widget::handle_queue_row_click,
                    army_overview_widget::update_army_overview,
                    tech_tree_widget::update_tech_tree,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    event_log_widget::update_event_log,
                    event_log_widget::handle_event_log_click,
                    group_hotkeys_widget::update_group_hotkeys_widget,
                    group_hotkeys_widget::handle_control_group_keys,
                    group_hotkeys_widget::handle_group_slot_click,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            // Animation systems run in ALL states so menu animations work
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
            // UI scale system runs in ALL states
            .add_systems(Update, update_ui_scale);
    }
}

fn update_ui_scale(
    graphics: Res<GraphicsSettings>,
    mut ui_scale: ResMut<UiScale>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = windows.single() else { return };
    let logical_height = window.physical_height() as f32 / window.scale_factor();
    let base_height = 720.0_f32;
    let auto = logical_height / base_height;
    let new_scale = auto * graphics.ui_scale;
    if (ui_scale.0 - new_scale).abs() > 0.001 {
        ui_scale.0 = new_scale;
    }
}

/// Root UI container that holds all widgets
#[derive(Component)]
struct UiRoot;

/// Floating label showing biome placement feedback
#[derive(Component)]
struct PlacementHintLabel;

pub fn spawn_hud(mut commands: Commands, icons: Res<IconAssets>, registry: Res<WidgetRegistry>) {
    // Root full-screen container for widget grid
    let root = commands
        .spawn((
            UiRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            // Allow click-through on the root
            Pickable::IGNORE,
        ))
        .id();

    // Spawn widget toolbar at top-center
    widget_toolbar::spawn_toolbar(&mut commands, root);

    // Spawn Resources widget
    let resources_content = spawn_widget_frame(
        &mut commands, root, WidgetId::Resources,
        registry.slots.get(&WidgetId::Resources).unwrap(),
        registry.is_visible(WidgetId::Resources),
    );
    resources_widget::spawn_resource_content(&mut commands, resources_content, &icons);

    // Spawn Selection widget (content is dynamic, rebuilt by rebuild_selection_panel)
    let selection_content = spawn_widget_frame(
        &mut commands, root, WidgetId::Selection,
        registry.slots.get(&WidgetId::Selection).unwrap(),
        registry.is_visible(WidgetId::Selection),
    );
    // Tag the content entity so rebuild_selection_panel can find it
    commands.entity(selection_content).insert(SelectionInfoPanel);

    // Spawn Actions widget (content is dynamic, rebuilt by update_action_bar)
    let actions_content = spawn_widget_frame(
        &mut commands, root, WidgetId::Actions,
        registry.slots.get(&WidgetId::Actions).unwrap(),
        registry.is_visible(WidgetId::Actions),
    );
    // Tag the content entity so update_action_bar can find it
    commands.entity(actions_content).insert(ActionBarInner);

    // Spawn Army Overview widget
    spawn_widget_frame(
        &mut commands, root, WidgetId::ArmyOverview,
        registry.slots.get(&WidgetId::ArmyOverview).unwrap(),
        registry.is_visible(WidgetId::ArmyOverview),
    );

    // Spawn Production Queue widget
    spawn_widget_frame(
        &mut commands, root, WidgetId::ProductionQueue,
        registry.slots.get(&WidgetId::ProductionQueue).unwrap(),
        registry.is_visible(WidgetId::ProductionQueue),
    );

    // Spawn Tech Tree widget (overlay, closed by default)
    spawn_widget_frame(
        &mut commands, root, WidgetId::TechTree,
        registry.slots.get(&WidgetId::TechTree).unwrap(),
        registry.is_visible(WidgetId::TechTree),
    );

    // Spawn Group Hotkeys widget
    spawn_widget_frame(
        &mut commands, root, WidgetId::GroupHotkeys,
        registry.slots.get(&WidgetId::GroupHotkeys).unwrap(),
        registry.is_visible(WidgetId::GroupHotkeys),
    );

    // Spawn Event Log widget
    spawn_widget_frame(
        &mut commands, root, WidgetId::EventLog,
        registry.slots.get(&WidgetId::EventLog).unwrap(),
        registry.is_visible(WidgetId::EventLog),
    );

    // Spawn Minimap widget (content populated by MinimapPlugin in PostStartup)
    let minimap_content = spawn_widget_frame(
        &mut commands, root, WidgetId::Minimap,
        registry.slots.get(&WidgetId::Minimap).unwrap(),
        registry.is_visible(WidgetId::Minimap),
    );
    commands.entity(minimap_content).insert(crate::minimap::MinimapWidgetContent);

    // Spawn notification container
    notifications::spawn_notification_container(&mut commands, root);

    // Spawn placement hint label (hidden by default)
    commands.spawn((
        PlacementHintLabel,
        Text::new(""),
        TextFont {
            font_size: theme::FONT_BUTTON,
            ..default()
        },
        TextColor(Color::srgba(1.0, 0.3, 0.3, 0.95)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(60.0),
            left: Val::Percent(50.0),
            ..default()
        },
        Visibility::Hidden,
        Pickable::IGNORE,
    ));
}

fn update_placement_hint(
    placement: Res<BuildingPlacementState>,
    mut hint_q: Query<(&mut Text, &mut Visibility), With<PlacementHintLabel>>,
) {
    let Ok((mut text, mut vis)) = hint_q.single_mut() else {
        return;
    };
    if let Some(ref hint) = placement.hint_text {
        **text = hint.clone();
        *vis = Visibility::Inherited;
    } else {
        *vis = Visibility::Hidden;
    }
}

fn compute_ui_mode(
    mut ui_mode: ResMut<UiMode>,
    placement: Res<BuildingPlacementState>,
    selected_units: Query<Entity, (With<Unit>, With<Selected>)>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
) {
    let new_mode = match placement.mode {
        PlacementMode::Placing(kind) => UiMode::PlacingBuilding(kind),
        PlacementMode::PlotBase => UiMode::PlacingBuilding(crate::blueprints::EntityKind::Base),
        PlacementMode::None | PlacementMode::PlotWall { .. } | PlacementMode::PlotGate => {
            if let Ok(building_entity) = selected_buildings.single() {
                UiMode::SelectedBuilding(building_entity)
            } else {
                let units: Vec<Entity> = selected_units.iter().collect();
                if units.is_empty() {
                    UiMode::Idle
                } else {
                    UiMode::SelectedUnits(units)
                }
            }
        }
    };

    if *ui_mode != new_mode {
        *ui_mode = new_mode;
    }
}
