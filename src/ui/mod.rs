pub mod actions_widget;
pub mod animations;
pub mod army_overview_widget;
pub mod buttons;
pub mod event_log_widget;
pub mod fonts;
pub mod group_hotkeys_widget;
pub mod hints_widget;
pub mod menu_helpers;
pub mod notifications;
pub mod production_queue_widget;
pub mod resources_widget;
pub mod selection_widget;
pub mod shared;
pub mod tech_tree_widget;
pub mod widget_framework;
pub mod widget_toolbar;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::components::*;
use crate::selection::SelectionSet;
use crate::theme;

use widget_framework::{spawn_widget_frame, WidgetId, WidgetRegistry};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RallyPointMode>()
            .init_resource::<UiMode>()
            .init_resource::<fonts::UiFonts>()
            .init_resource::<actions_widget::ActionBarLayoutRevision>()
            .init_resource::<WidgetRegistry>()
            .init_resource::<widget_framework::WidgetResizeState>()
            .init_resource::<widget_framework::WidgetDragState>()
            .init_resource::<widget_framework::GridInteractionActive>()
            .init_resource::<group_hotkeys_widget::ControlGroups>()
            .init_resource::<ControlGroupState>()
            .init_resource::<event_log_widget::GameEventLog>()
            .init_resource::<event_log_widget::EventLogRenderState>()
            .init_resource::<event_log_widget::EventLogFilter>()
            .init_resource::<hints_widget::HintState>()
            .add_systems(OnEnter(AppState::InGame), mark_pending_ui_spawn)
            .add_systems(
                Update,
                (
                    hints_widget::hints_system,
                    hints_widget::idle_worker_notification_system,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                PostUpdate,
                fonts::apply_default_fonts,
            )
            .add_systems(
                Update,
                (spawn_hud, widget_framework::spawn_grid_overlay, clear_pending_ui_spawn)
                    .chain()
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<PendingUiSpawn>),
            )
            .add_systems(
                Update,
                (ApplyDeferred, compute_ui_mode)
                    .chain()
                    .after(SelectionSet)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                update_placement_hint.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    ApplyDeferred,
                    adopt_back_overlay_items,
                    adopt_front_overlay_items,
                )
                    .chain()
                    .in_set(OverlayLifecycleSet::Adopt)
                    .after(OverlayLifecycleSet::Manage)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    resources_widget::update_resource_texts,
                    resources_widget::update_processed_resource_visibility,
                    selection_widget::rebuild_selection_panel,
                    buttons::update_hp_bars,
                    buttons::handle_unit_card_click,
                    buttons::clear_stale_inspected,
                )
                    .after(compute_ui_mode)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    actions_widget::track_action_bar_layout,
                    actions_widget::update_action_bar,
                )
                    .chain()
                    .after(compute_ui_mode)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (buttons::handle_build_buttons, buttons::handle_train_buttons)
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            .add_systems(
                Update,
                (
                    buttons::handle_upgrade_button,
                    buttons::handle_demolish_button,
                    buttons::handle_demolish_confirm,
                    buttons::handle_scuttle_unit_button,
                    buttons::handle_drop_cargo_button,
                    buttons::handle_rally_point_button,
                    buttons::handle_toggle_auto_attack,
                    buttons::handle_cancel_train,
                    buttons::handle_assign_worker_button,
                    buttons::handle_unassign_worker_button,
                    buttons::handle_unassign_specific_worker_button,
                    buttons::handle_unassign_one_worker_button,
                    buttons::handle_pause_building_button,
                    buttons::handle_select_recipe_button,
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            .add_systems(
                Update,
                (
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
                    buttons::action_bar_transition_system,
                    buttons::show_action_tooltips,
                    buttons::update_action_tooltip_positions,
                    buttons::cleanup_action_tooltips,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    buttons::handle_attack_move_button,
                    buttons::handle_patrol_button,
                    buttons::handle_hold_position_button,
                    buttons::handle_stop_button,
                    buttons::handle_cycle_stance_button,
                    buttons::handle_ability_button,
                    buttons::handle_formation_button,
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
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
            .add_systems(
                Update,
                (
                    notifications::update_ally_notifications,
                    notifications::handle_notification_click,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    production_queue_widget::update_production_queue,
                    production_queue_widget::handle_queue_row_click,
                    production_queue_widget::handle_queue_cancel_buttons,
                    army_overview_widget::update_army_overview,
                    tech_tree_widget::update_tech_tree,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    event_log_widget::handle_log_level_pills,
                    event_log_widget::update_event_log
                        .after(event_log_widget::handle_log_level_pills),
                    event_log_widget::handle_event_log_click,
                    group_hotkeys_widget::update_group_hotkeys_widget,
                    group_hotkeys_widget::handle_group_slot_click,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                group_hotkeys_widget::handle_control_group_keys
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
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

fn placement_default_hint(mode: PlacementMode) -> Option<String> {
    match mode {
        PlacementMode::None => None,
        PlacementMode::Placing(kind) => Some(format!(
            "Placing {}: Left-click ground to place (Right-click/Escape to cancel)",
            kind.display_name()
        )),
        PlacementMode::PlotBase => Some(
            "Founding Base: Left-click ground to place (Right-click/Escape to cancel)".to_string(),
        ),
        PlacementMode::PlotWall { start } => {
            if start == Vec3::ZERO {
                Some("Wall: Click ground to start (Right-click/Escape to cancel)".to_string())
            } else {
                Some(
                    "Wall: Move cursor, then left-click to confirm (Right-click/Escape to cancel)"
                        .to_string(),
                )
            }
        }
        PlacementMode::PlotGate => Some(
            "Gatehouse: Hover owned wall and left-click (Right-click/Escape to cancel)".to_string(),
        ),
    }
}

fn is_error_hint(hint: &str) -> bool {
    hint.contains("Not enough")
        || hint.contains("must")
        || hint.contains("blocked")
        || hint.contains("No workers")
        || hint.contains("Cannot place")
}

/// Root UI container that holds all widgets
#[derive(Component)]
pub(crate) struct UiRoot;

#[derive(Component)]
struct MainHudRoot;

/// Floating label showing biome placement feedback
#[derive(Component)]
struct PlacementHintLabel;

#[derive(Resource)]
struct PendingUiSpawn;

fn mark_pending_ui_spawn(mut commands: Commands) {
    commands.insert_resource(PendingUiSpawn);
}

fn clear_pending_ui_spawn(mut commands: Commands) {
    commands.remove_resource::<PendingUiSpawn>();
}

pub fn spawn_hud(
    mut commands: Commands,
    icons: Res<IconAssets>,
    registry: Res<WidgetRegistry>,
    fonts: Res<fonts::UiFonts>,
    existing_roots: Query<Entity, With<UiRoot>>,
) {
    if !existing_roots.is_empty() {
        return;
    }

    // Root full-screen container for in-game HUD layering
    let root = commands
        .spawn((
            GameWorld,
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

    let back_overlay_root = commands
        .spawn((
            GameWorld,
            WorldOverlayBackRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            GlobalZIndex(-10),
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(root).add_child(back_overlay_root);

    let hud_root = commands
        .spawn((
            GameWorld,
            MainHudRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(root).add_child(hud_root);

    let front_overlay_root = commands
        .spawn((
            GameWorld,
            WorldOverlayFrontRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            GlobalZIndex(80),
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(root).add_child(front_overlay_root);

    // Spawn widget toolbar at top-center
    widget_toolbar::spawn_toolbar(&mut commands, hud_root, &fonts);

    // Spawn Resources widget
    let resources_content = spawn_widget_frame(
        &mut commands,
        hud_root,
        WidgetId::Resources,
        registry.slots.get(&WidgetId::Resources).unwrap(),
        registry.is_visible(WidgetId::Resources),
        &fonts,
    );
    resources_widget::spawn_resource_content(&mut commands, resources_content, &icons);

    // Spawn Selection widget (content is dynamic, rebuilt by rebuild_selection_panel)
    let selection_content = spawn_widget_frame(
        &mut commands,
        hud_root,
        WidgetId::Selection,
        registry.slots.get(&WidgetId::Selection).unwrap(),
        registry.is_visible(WidgetId::Selection),
        &fonts,
    );
    // Tag the content entity so rebuild_selection_panel can find it
    commands
        .entity(selection_content)
        .insert(SelectionInfoPanel);

    // Spawn Actions widget (content is dynamic, rebuilt by update_action_bar)
    let actions_content = spawn_widget_frame(
        &mut commands,
        hud_root,
        WidgetId::Actions,
        registry.slots.get(&WidgetId::Actions).unwrap(),
        registry.is_visible(WidgetId::Actions),
        &fonts,
    );
    // Tag the content entity so update_action_bar can find it
    commands.entity(actions_content).insert(ActionBarInner);

    // Spawn Army Overview widget
    spawn_widget_frame(
        &mut commands,
        hud_root,
        WidgetId::ArmyOverview,
        registry.slots.get(&WidgetId::ArmyOverview).unwrap(),
        registry.is_visible(WidgetId::ArmyOverview),
        &fonts,
    );

    // Spawn Production Queue widget
    spawn_widget_frame(
        &mut commands,
        hud_root,
        WidgetId::ProductionQueue,
        registry.slots.get(&WidgetId::ProductionQueue).unwrap(),
        registry.is_visible(WidgetId::ProductionQueue),
        &fonts,
    );

    // Spawn Tech Tree widget (overlay, closed by default)
    spawn_widget_frame(
        &mut commands,
        hud_root,
        WidgetId::TechTree,
        registry.slots.get(&WidgetId::TechTree).unwrap(),
        registry.is_visible(WidgetId::TechTree),
        &fonts,
    );

    // Spawn Group Hotkeys widget
    spawn_widget_frame(
        &mut commands,
        hud_root,
        WidgetId::GroupHotkeys,
        registry.slots.get(&WidgetId::GroupHotkeys).unwrap(),
        registry.is_visible(WidgetId::GroupHotkeys),
        &fonts,
    );

    // Spawn Event Log widget
    spawn_widget_frame(
        &mut commands,
        hud_root,
        WidgetId::EventLog,
        registry.slots.get(&WidgetId::EventLog).unwrap(),
        registry.is_visible(WidgetId::EventLog),
        &fonts,
    );

    // Spawn Minimap widget (content populated by MinimapPlugin in PostStartup)
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

    // Spawn Debug widget
    let debug_content = spawn_widget_frame(
        &mut commands,
        hud_root,
        WidgetId::Debug,
        registry.slots.get(&WidgetId::Debug).unwrap(),
        registry.is_visible(WidgetId::Debug),
        &fonts,
    );
    crate::debug::spawn_debug_content(&mut commands, debug_content);

    // Spawn notification container
    notifications::spawn_notification_container(&mut commands, hud_root);

    // Spawn placement hint label (hidden by default)
    let placement_hint = commands
        .spawn((
        PlacementHintLabel,
        WorldOverlayFrontItem,
        Text::new(""),
        TextFont {
            font_size: theme::FONT_BODY,
            ..default()
        },
        TextColor(theme::TEXT_SECONDARY),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            max_width: Val::Px(420.0),
            padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
            border_radius: BorderRadius::all(Val::Px(5.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.05, 0.07, 0.9)),
        BorderColor::all(Color::srgba(0.25, 0.25, 0.30, 0.5)),
        GlobalZIndex(90),
        Visibility::Hidden,
        Pickable::IGNORE,
        ))
        .id();
    commands.entity(front_overlay_root).add_child(placement_hint);
}

fn update_placement_hint(
    placement: Res<BuildingPlacementState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    mut hint_q: Query<
        (&mut Text, &mut TextColor, &mut Node, &mut Visibility),
        With<PlacementHintLabel>,
    >,
) {
    let Ok((mut text, mut color, mut node, mut vis)) = hint_q.single_mut() else {
        return;
    };

    let hint = placement
        .hint_text
        .clone()
        .or_else(|| placement_default_hint(placement.mode));

    if let Some(hint) = hint {
        **text = hint.clone();
        *color = TextColor(if is_error_hint(&hint) {
            theme::DESTRUCTIVE
        } else {
            theme::TEXT_SECONDARY
        });

        if let Ok(window) = windows.single() {
            if let Some(cursor) = window.cursor_position() {
                let scale = ui_scale.0.max(0.001);
                let ui_w = window.width() / scale;
                let ui_h = window.height() / scale;
                let left = (cursor.x / scale + 14.0).clamp(6.0, (ui_w - 430.0).max(6.0));
                let top = (cursor.y / scale + 18.0).clamp(6.0, (ui_h - 70.0).max(6.0));
                node.left = Val::Px(left);
                node.top = Val::Px(top);
            }
        }

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

fn adopt_back_overlay_items(
    root_q: Query<Entity, With<WorldOverlayBackRoot>>,
    items: Query<Entity, (With<WorldOverlayBackItem>, Without<ChildOf>)>,
    mut commands: Commands,
) {
    let Ok(root) = root_q.single() else {
        return;
    };
    for entity in &items {
        commands.entity(root).add_child(entity);
    }
}

fn adopt_front_overlay_items(
    root_q: Query<Entity, With<WorldOverlayFrontRoot>>,
    items: Query<Entity, (With<WorldOverlayFrontItem>, Without<ChildOf>)>,
    mut commands: Commands,
) {
    let Ok(root) = root_q.single() else {
        return;
    };
    for entity in &items {
        commands.entity(root).add_child(entity);
    }
}
