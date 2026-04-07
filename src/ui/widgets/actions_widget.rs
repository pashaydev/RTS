use bevy::prelude::*;

use super::core::framework::WidgetId;
use super::core::hud::MainHudRoot;
use super::core::shared::widget_content_stack;
use crate::blueprints::{BlueprintRegistry, EntityKind};
use crate::types::*;
use crate::ui::theme::Theme;

use super::buttons;

use super::actions_buildings::{
    spawn_building_action_bar, spawn_building_grid, spawn_construction_action_bar,
    spawn_found_base_panel,
};
use super::actions_units::spawn_units_action_bar;

pub struct ActionsWidgetPlugin;

impl Plugin for ActionsWidgetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActionBarLayoutRevision>()
            // Spawn actions widget frame
            .add_systems(
                Update,
                spawn_actions_widget
                    .run_if(in_state(AppState::InGame))
                    .run_if(any_with_component::<MainHudRoot>),
            )
            // Actions widget update
            .add_systems(
                Update,
                (track_action_bar_layout, update_action_bar)
                    .chain()
                    .after(super::core::hud::compute_ui_mode)
                    .run_if(in_state(AppState::InGame)),
            )
            // Build & train buttons (player command gated)
            .add_systems(
                Update,
                (buttons::handle_build_buttons, buttons::handle_train_buttons)
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            // Building action buttons (player command gated)
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
            // Training/construction display updates
            .add_systems(
                Update,
                (
                    buttons::update_training_queue_display,
                    buttons::update_construction_progress_display,
                    buttons::update_train_cost_colors,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            // Upgrade progress & action bar transitions
            .add_systems(
                Update,
                (
                    buttons::update_upgrade_progress_display,
                    buttons::action_bar_transition_system,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            // Unit command buttons (player command gated)
            .add_systems(
                Update,
                (
                    buttons::handle_command_mode_buttons,
                    buttons::handle_hold_position_button,
                    buttons::handle_stop_button,
                    buttons::handle_cycle_stance_button,
                    buttons::handle_ability_button,
                    buttons::handle_formation_button,
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            );
    }
}

widget_spawn_system!(spawn_actions_widget, WidgetId::Actions, |commands, content| {
    commands.entity(content).insert(ActionBarInner);
});

#[derive(Resource)]
pub struct ActionBarLayoutRevision {
    pub revision: u64,
    pub bucket: u8,
}

impl Default for ActionBarLayoutRevision {
    fn default() -> Self {
        Self {
            revision: 0,
            bucket: u8::MAX,
        }
    }
}

pub fn track_action_bar_layout(
    mut layout: ResMut<ActionBarLayoutRevision>,
    action_bar: Query<&ComputedNode, With<ActionBarInner>>,
) {
    let Ok(node) = action_bar.single() else {
        return;
    };
    let logical_width = node.size().x * node.inverse_scale_factor();
    let bucket = if logical_width < 300.0 {
        0
    } else if logical_width < 420.0 {
        1
    } else {
        2
    };
    if bucket != layout.bucket {
        layout.bucket = bucket;
        layout.revision = layout.revision.saturating_add(1);
    }
}

pub fn update_action_bar(
    mut commands: Commands,
    ui_mode: Res<UiMode>,
    theme: Res<Theme>,
    selected_units: Query<
        (
            &EntityKind,
            Option<&Carrying>,
            Option<&CarryCapacity>,
            Option<&UnitState>,
        ),
        (With<Unit>, With<Selected>),
    >,
    selected_buildings: Query<
        (
            Entity,
            &EntityKind,
            &BuildingState,
            &BuildingLevel,
            Option<&UpgradeProgress>,
            Option<&ConstructionProgress>,
            Option<&TrainingQueue>,
            Option<&StorageInventory>,
            Option<&Health>,
            Option<&TowerAutoAttackEnabled>,
            Option<&ResourceProcessor>,
            Option<&ProductionState>,
            Option<&BuildingPaused>,
        ),
        (With<Building>, With<Selected>),
    >,
    assigned_workers_q: Query<&AssignedWorkers>,
    worker_states_q: Query<&UnitState, With<Unit>>,
    player_state: (
        Res<AllCompletedBuildings>,
        Res<FactionBaseState>,
        Res<ActivePlayer>,
        Res<AllPlayerResources>,
    ),
    unit_cap_queries: (
        Query<&Faction, With<Unit>>,
        Query<(&Faction, &TrainingQueue), With<Building>>,
        Query<(&Faction, &EntityKind, &BuildingState, &BuildingLevel), With<Building>>,
    ),
    registry: Res<BlueprintRegistry>,
    action_state: (
        Query<(Entity, Option<&Children>), With<ActionBarInner>>,
        Query<
            Entity,
            Or<(
                Changed<BuildingState>,
                Changed<BuildingLevel>,
                Changed<UpgradeProgress>,
                Changed<TowerAutoAttackEnabled>,
                Changed<AssignedWorkers>,
                Changed<ProductionState>,
                Changed<BuildingPaused>,
            )>,
        >,
        Query<Entity, With<BuildGridButton>>,
        Query<Entity, With<DemolishConfirmPanel>>,
        Query<&Children>,
        Res<ActionBarLayoutRevision>,
    ),
    local_state: (
        Local<usize>,
        Local<[u32; ResourceType::COUNT]>,
        Local<UnitCapStats>,
    ),
    ui_state: (Res<IconAssets>, Res<RallyPointMode>),
    formation: Res<ActiveFormation>,
    faction_ages: Res<crate::simulation::ages::FactionAges>,
) {
    let (all_completed, base_state, active_player, all_resources) = player_state;
    let current_age = faction_ages.get_age(&active_player.0);
    let (all_units, all_training_queues, all_buildings_for_cap) = unit_cap_queries;
    let (
        action_bar,
        changed_buildings,
        existing_cards,
        confirm_panels,
        children_q_readonly,
        layout_revision,
    ) = action_state;
    let (mut last_queue_len, mut last_res_snapshot, mut last_unit_cap) = local_state;
    let (icons, rally_mode) = ui_state;

    if !confirm_panels.is_empty() {
        return;
    }

    if matches!(*ui_mode, UiMode::PlacingBuilding(_)) {
        return;
    }

    let mode_changed = ui_mode.is_changed();
    let has_building_change = !changed_buildings.is_empty();
    let completed_changed = all_completed.is_changed();
    let founded_changed = base_state.is_changed();
    let rally_changed = rally_mode.is_changed();
    let current_amounts = all_resources.get(&active_player.0).amounts;
    let resources_changed = current_amounts != *last_res_snapshot;
    *last_res_snapshot = current_amounts;
    let current_unit_cap = faction_unit_cap_stats(
        active_player.0,
        all_units.iter(),
        all_training_queues.iter(),
        all_buildings_for_cap.iter(),
    );
    let unit_cap_changed = current_unit_cap != *last_unit_cap;
    *last_unit_cap = current_unit_cap;
    let layout_changed = layout_revision.is_changed();

    let current_queue_len = selected_buildings
        .iter()
        .next()
        .and_then(|(_, _, _, _, _, _, q, _, _, _, _, _, _)| q.map(|q| q.queue.len()))
        .unwrap_or(0);
    let queue_changed = current_queue_len != *last_queue_len;
    *last_queue_len = current_queue_len;

    if !mode_changed
        && !has_building_change
        && !completed_changed
        && !founded_changed
        && !queue_changed
        && !rally_changed
        && !resources_changed
        && !unit_cap_changed
        && !layout_changed
    {
        return;
    }

    let Ok((bar_entity, bar_children)) = action_bar.single() else {
        return;
    };

    if !mode_changed
        && *ui_mode == UiMode::Idle
        && !existing_cards.is_empty()
        && !completed_changed
        && !founded_changed
        && !resources_changed
        && !unit_cap_changed
        && !layout_changed
    {
        return;
    }

    // Clear existing children — despawn immediately to avoid duplicates
    if let Some(children) = bar_children {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }

    let layout_bucket = layout_revision.bucket;

    let is_building_grid;
    match &*ui_mode {
        UiMode::SelectedBuilding(_) => {
            is_building_grid = false;
            if let Ok((
                building_entity,
                kind,
                state,
                level,
                upgrade_progress,
                construction,
                training_queue,
                storage_inv,
                health,
                auto_attack,
                proc_opt,
                production_state,
                building_paused,
            )) = selected_buildings.single()
            {
                if *state == BuildingState::Complete {
                    let player_res = all_resources.get(&active_player.0);
                    let worker_info: Vec<(Entity, AssignedPhase)> = assigned_workers_q
                        .get(building_entity)
                        .map(|aw| {
                            aw.workers
                                .iter()
                                .filter_map(|&w| {
                                    if let Ok(unit_state) = worker_states_q.get(w) {
                                        if let UnitState::AssignedGathering { phase, .. } =
                                            unit_state
                                        {
                                            return Some((w, phase.clone()));
                                        }
                                    }
                                    Some((w, AssignedPhase::SeekingNode))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    spawn_building_action_bar(
                        &mut commands,
                        bar_entity,
                        *kind,
                        level.0,
                        upgrade_progress,
                        training_queue,
                        storage_inv,
                        health,
                        auto_attack,
                        proc_opt,
                        production_state,
                        &worker_info,
                        building_paused.is_some(),
                        &icons,
                        &registry,
                        player_res,
                        current_unit_cap,
                        &rally_mode,
                        layout_bucket,
                        &theme,
                    );
                } else {
                    spawn_construction_action_bar(
                        &mut commands,
                        bar_entity,
                        *kind,
                        construction,
                        &registry,
                        layout_bucket,
                        &theme,
                    );
                }
            }
        }
        UiMode::SelectedUnits(_) => {
            let founded = base_state.is_founded(&active_player.0);
            let has_workers = selected_units
                .iter()
                .any(|(k, ..)| *k == EntityKind::Worker);
            if !founded && has_workers {
                is_building_grid = true;
                let player_res = all_resources.get(&active_player.0);
                spawn_found_base_panel(
                    &mut commands,
                    bar_entity,
                    &icons,
                    &registry,
                    player_res,
                    layout_bucket,
                    &theme,
                );
            } else {
                is_building_grid = false;
                spawn_units_action_bar(
                    &mut commands,
                    bar_entity,
                    &selected_units,
                    layout_bucket,
                    &formation,
                    &theme,
                );
            }
        }
        _ => {
            is_building_grid = true;
            let player_res = all_resources.get(&active_player.0);
            let founded = base_state.is_founded(&active_player.0);
            if founded {
                let completed = all_completed.completed_for(&active_player.0);
                spawn_building_grid(
                    &mut commands,
                    bar_entity,
                    completed,
                    founded,
                    &icons,
                    &registry,
                    player_res,
                    layout_bucket,
                    &theme,
                    current_age,
                );
            } else {
                spawn_found_base_panel(
                    &mut commands,
                    bar_entity,
                    &icons,
                    &registry,
                    player_res,
                    layout_bucket,
                    &theme,
                );
            }
        }
    }

    // Only play entrance animations when the UI mode structurally changed
    // (e.g. switching from idle→building selected), not on data-only refreshes
    // like resource ticks or queue length updates.
    if !is_building_grid && mode_changed {
        if let Ok(children) = children_q_readonly.get(bar_entity) {
            for child in children.iter() {
                commands.entity(child).try_insert((
                    ActionBarFadeIn {
                        timer: Timer::from_seconds(0.2, TimerMode::Once),
                        delay: Timer::from_seconds(0.1, TimerMode::Once),
                        started: false,
                    },
                    Visibility::Hidden,
                ));
            }
        }
    }
}

// ── Building Grid (replaces card hand) ──

/// New component replacing BuildCard for the grid-based building buttons
#[derive(Component)]
pub struct BuildGridButton(#[allow(dead_code)] pub EntityKind);
