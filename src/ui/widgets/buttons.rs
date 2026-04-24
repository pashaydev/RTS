//! Shared button component library used by in-game widgets (action,
//! production, selection); handlers emit the corresponding input commands.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use game_state::message::InputCommand;

use super::unit_action_panel::PreferResourceButton;
use crate::blueprints::{BlueprintRegistry, EntityKind};
use crate::infrastructure::multiplayer::lockstep::GameplayInputSubmit;
use crate::infrastructure::multiplayer::NetRole;
use crate::presentation::camera;
use crate::simulation::buildings;
use crate::simulation::combat::{
    apply_manual_cast_intent, apply_manual_hold_intent, clear_combat_intent, AbilityRegistry,
    CastTarget,
};
use crate::types::*;
use crate::ui::theme::{self, Theme};

/// Mark the UI as having been clicked this frame (prevents clicks falling through to the game world).
#[inline]
fn mark_click(ui_clicked: &mut UiClickedThisFrame, ui_press: &mut UiPressActive) {
    ui_clicked.0 = 2;
    ui_press.0 = true;
}

#[derive(SystemParam)]
pub(crate) struct OnlineInputParams<'w> {
    net_role: Res<'w, NetRole>,
    submit: GameplayInputSubmit<'w>,
}

fn push_online_input(
    submit: &mut GameplayInputSubmit,
    entity_ids: Vec<u32>,
    commands: Vec<InputCommand>,
) {
    submit.push_raw(entity_ids, commands);
}

// ── Build button handler ──

pub fn handle_build_buttons(
    interactions: Query<
        (
            Entity,
            &Interaction,
            &BuildButton,
            Option<&super::action_panel::BuildGridButton>,
        ),
        Changed<Interaction>,
    >,
    mut placement: ResMut<BuildingPlacementState>,
    all_completed: Res<AllCompletedBuildings>,
    base_state: Res<FactionBaseState>,
    all_resources: Res<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    carried_totals: Res<CarriedResourceTotals>,
    registry: Res<BlueprintRegistry>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
) {
    for (_entity, interaction, build_btn, _grid_btn) in &interactions {
        if *interaction == Interaction::Pressed {
            ui_clicked.0 = 2;
            let kind = build_btn.0;
            let founded = base_state.is_founded(&active_player.0);

            if kind == EntityKind::Base && !founded {
                let player_res = all_resources.get(&active_player.0);
                let carried = carried_totals.get(&active_player.0);
                let bp = registry.get(kind);
                if !bp.cost.can_afford_with_carried(player_res, carried) {
                    continue;
                }

                placement.mode = PlacementMode::PlotBase;
                placement.awaiting_release = true;
                placement.hint_text = Some(
                    "Left-click ground to place Base (Right-click/Escape to cancel)".to_string(),
                );
                continue;
            }

            let bp = registry.get(kind);
            let prereq_met = if let Some(ref bd) = bp.building {
                match bd.prerequisite {
                    None => true,
                    Some(prereq_kind) => {
                        if prereq_kind == EntityKind::Base {
                            founded || all_completed.has(&active_player.0, prereq_kind)
                        } else {
                            all_completed.has(&active_player.0, prereq_kind)
                        }
                    }
                }
            } else {
                true
            };
            if !prereq_met {
                continue;
            }

            // Wall corners and posts are auto-placed, not manually buildable
            if kind == EntityKind::WallCorner || kind == EntityKind::WallPost {
                continue;
            }

            if kind == EntityKind::WallSegment && founded {
                placement.mode = PlacementMode::PlotWall { start: Vec3::ZERO };
                placement.awaiting_release = false;
                placement.hint_text =
                    Some("Click ground to start wall (Right-click/Escape to cancel)".to_string());
                continue;
            }

            if kind == EntityKind::Gatehouse && founded {
                placement.mode = PlacementMode::PlotGate;
                placement.awaiting_release = false;
                placement.hint_text = Some(
                    "Hover an owned wall segment and left-click (Right-click/Escape to cancel)"
                        .to_string(),
                );
                continue;
            }

            if kind == EntityKind::Floor && founded {
                placement.mode = PlacementMode::PlotFloor;
                placement.awaiting_release = false;
                placement.hint_text = Some(
                    "Hold left-click and drag to paint floor (Right-click/Escape to cancel)"
                        .to_string(),
                );
                continue;
            }

            let player_res = all_resources.get(&active_player.0);
            let carried = carried_totals.get(&active_player.0);
            if !bp.cost.can_afford_with_carried(player_res, carried) {
                continue;
            }

            placement.mode = PlacementMode::Placing(kind);
            placement.awaiting_release = true;
            placement.hint_text =
                Some("Left-click ground to place (Right-click/Escape to cancel)".to_string());
        }
    }
}

// ── Train button handler ──

pub fn handle_train_buttons(
    interactions: Query<(&Interaction, &TrainButton), Changed<Interaction>>,
    mut online: OnlineInputParams,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    carried_totals: Res<CarriedResourceTotals>,
    mut pending_drains: ResMut<PendingCarriedDrains>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut queue_queries: ParamSet<(
        Query<(&Faction, &TrainingQueue), With<Building>>,
        Query<&mut TrainingQueue>,
    )>,
    all_units: Query<&Faction, With<Unit>>,
    all_buildings_for_cap: Query<
        (&Faction, &EntityKind, &BuildingState, &BuildingLevel),
        With<Building>,
    >,
    registry: Res<BlueprintRegistry>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    let unit_cap = faction_unit_cap_stats(
        active_player.0,
        all_units.iter(),
        queue_queries.p0().iter(),
        all_buildings_for_cap.iter(),
    );

    for (interaction, train_btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        if !unit_cap.has_room(1) {
            continue;
        }

        let kind = train_btn.0;
        let bp = registry.get(kind);
        let player_res = all_resources.get(&active_player.0);
        let carried = carried_totals.get(&active_player.0);
        if !bp.cost.can_afford_with_carried(player_res, carried) {
            continue;
        }

        if *online.net_role != NetRole::Offline {
            let Some(building_entity) = selected_buildings.iter().next() else {
                continue;
            };
            let Some(entity_ids) = online.submit.entity_ids_for([building_entity]) else {
                continue;
            };
            let Some(&building_id) = entity_ids.first() else {
                continue;
            };

            push_online_input(
                &mut online.submit,
                Vec::new(),
                vec![InputCommand::Train {
                    building_id,
                    kind: kind.to_index(),
                }],
            );
            continue;
        }

        for building_entity in &selected_buildings {
            if let Ok(mut queue) = queue_queries.p1().get_mut(building_entity) {
                let player_res_mut = all_resources.get_mut(&active_player.0);
                let deficits = bp.cost.deduct_with_carried(player_res_mut);
                let drain = SpendFromCarried {
                    faction: active_player.0,
                    amounts: deficits,
                };
                if drain.has_deficit() {
                    pending_drains.drains.push(drain);
                }
                queue.queue.push(kind);
                break;
            }
        }
    }
}

// ── Upgrade button handler ──

pub fn handle_upgrade_button(
    mut commands: Commands,
    interactions: Query<&Interaction, (Changed<Interaction>, With<UpgradeButton>)>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    carried_totals: Res<CarriedResourceTotals>,
    mut pending_drains: ResMut<PendingCarriedDrains>,
    selected_buildings: Query<
        (
            Entity,
            &EntityKind,
            &BuildingLevel,
            &BuildingState,
            &Faction,
        ),
        (With<Building>, With<Selected>, Without<UpgradeProgress>),
    >,
    registry: Res<BlueprintRegistry>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        if let Ok((entity, kind, level, state, faction)) = selected_buildings.single() {
            if *state != BuildingState::Complete {
                continue;
            }
            let carried = carried_totals.get(faction);
            let player_res = all_resources.get_mut(&active_player.0);
            buildings::start_upgrade(
                &mut commands,
                entity,
                level.0,
                *kind,
                &registry,
                player_res,
                *faction,
                carried,
                &mut pending_drains,
            );
        }
    }
}

// ── Demolish button handler ──

pub fn handle_demolish_button(
    mut commands: Commands,
    theme: Res<Theme>,
    interactions: Query<&Interaction, (Changed<Interaction>, With<DemolishButton>)>,
    action_bar: Query<Entity, With<ActionBarInner>>,
    selected_buildings: Query<
        (Entity, &EntityKind, &BuildingState, &Transform),
        (With<Building>, With<Selected>),
    >,
    registry: Res<BlueprintRegistry>,
    existing_confirm: Query<Entity, With<DemolishConfirmPanel>>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        if !existing_confirm.is_empty() {
            continue;
        }

        let Ok(bar_entity) = action_bar.single() else {
            continue;
        };

        if let Ok((_entity, kind, state, _transform)) = selected_buildings.single() {
            let bp = registry.get(*kind);
            let refund_pct = if *state == BuildingState::Complete {
                50
            } else {
                100
            };
            let refund_str = {
                let entries: Vec<String> = bp
                    .cost
                    .cost_entries()
                    .iter()
                    .map(|(rt, amt)| {
                        format!("{}:{}", rt.abbreviation(), amt * refund_pct as u32 / 100)
                    })
                    .collect();
                format!("Refunds ~{}% ({})", refund_pct, entries.join(" "))
            };

            let panel = commands
                .spawn((
                    DemolishConfirmPanel,
                    Node {
                        position_type: PositionType::Absolute,
                        bottom: Val::Percent(100.0),
                        left: Val::Px(0.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(4.0),
                        padding: UiRect::all(Val::Px(8.0)),
                        // border_radius: RADIUS_LG,
                        ..default()
                    },
                    BackgroundColor(theme.colors.bg_panel),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("Demolish?"),
                        TextFont {
                            font_size: theme.typography.medium,
                            ..default()
                        },
                        TextColor(theme.colors.destructive),
                    ));
                    panel.spawn((
                        Text::new(refund_str),
                        TextFont {
                            font_size: theme.typography.small,
                            ..default()
                        },
                        TextColor(theme.colors.text_secondary),
                    ));
                    panel
                        .spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(6.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Button,
                                StandardButton,
                                ConfirmDemolishButton,
                                Node {
                                    padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                                    // border_radius: RADIUS_MD,
                                    ..default()
                                },
                                BackgroundColor(theme.colors.destructive),
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new("Yes"),
                                    TextFont {
                                        font_size: theme.typography.body,
                                        ..default()
                                    },
                                    TextColor(theme.colors.text_primary),
                                ));
                            });

                            row.spawn((
                                Button,
                                StandardButton,
                                CancelDemolishButton,
                                Node {
                                    padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                                    // border_radius: RADIUS_MD,
                                    ..default()
                                },
                                BackgroundColor(theme.colors.btn_primary),
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new("No"),
                                    TextFont {
                                        font_size: theme.typography.body,
                                        ..default()
                                    },
                                    TextColor(theme.colors.text_primary),
                                ));
                            });
                        });
                })
                .id();
            commands.entity(bar_entity).add_child(panel);
        }
    }
}

pub fn handle_demolish_confirm(
    mut commands: Commands,
    confirm_interactions: Query<&Interaction, (Changed<Interaction>, With<ConfirmDemolishButton>)>,
    cancel_interactions: Query<&Interaction, (Changed<Interaction>, With<CancelDemolishButton>)>,
    selected_buildings: Query<
        (Entity, &Transform, &BuildingState),
        (With<Building>, With<Selected>),
    >,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    registry: Res<BlueprintRegistry>,
    building_kinds: Query<&EntityKind, With<Building>>,
    confirm_panels: Query<Entity, With<DemolishConfirmPanel>>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &confirm_interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        if let Ok((entity, transform, state)) = selected_buildings.single() {
            if *state == BuildingState::UnderConstruction {
                if let Ok(kind) = building_kinds.get(entity) {
                    let bp = registry.get(*kind);
                    let player_res = all_resources.get_mut(&active_player.0);
                    for (i, &amt) in bp.cost.amounts.iter().enumerate() {
                        player_res.amounts[i] += amt;
                    }
                }
                commands.entity(entity).try_despawn();
            } else {
                buildings::start_demolish(&mut commands, entity, transform);
            }
        }

        for panel in &confirm_panels {
            commands.entity(panel).try_despawn();
        }
    }

    for interaction in &cancel_interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        for panel in &confirm_panels {
            commands.entity(panel).try_despawn();
        }
    }
}

pub fn handle_scuttle_unit_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<ScuttleUnitButton>)>,
    mut online: OnlineInputParams,
    selected_units: Query<(Entity, &EntityKind, &Faction), (With<Unit>, With<Selected>)>,
    active_player: Res<ActivePlayer>,
    mut health_q: Query<&mut Health, With<Unit>>,
    mut cmd_mode: ResMut<CommandMode>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        *cmd_mode = CommandMode::Normal;

        if online.submit.submit(
            selected_units
                .iter()
                .filter(|(_, kind, faction)| {
                    **faction == active_player.0 && **kind == EntityKind::Worker
                })
                .map(|(entity, _, _)| entity),
            vec![InputCommand::Scuttle],
        ) {
            continue;
        }

        for (entity, kind, faction) in &selected_units {
            if *faction != active_player.0 || *kind != EntityKind::Worker {
                continue;
            }
            if let Ok(mut hp) = health_q.get_mut(entity) {
                hp.current = 0.0;
            }
        }
    }
}

// ── Drop cargo button handler ──

pub fn handle_drop_cargo_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<DropCargoButton>)>,
    mut online: OnlineInputParams,
    mut workers: Query<
        (Entity, &EntityKind, &Faction, &mut Carrying, &mut UnitState),
        (With<Unit>, With<Selected>),
    >,
    active_player: Res<ActivePlayer>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        if online.submit.submit(
            workers
                .iter()
                .filter(|(_, kind, faction, carrying, _)| {
                    **faction == active_player.0
                        && **kind == EntityKind::Worker
                        && carrying.amount > 0
                })
                .map(|(entity, _, _, _, _)| entity),
            vec![InputCommand::DropCargo],
        ) {
            continue;
        }

        for (_, kind, faction, mut carrying, mut state) in &mut workers {
            if *faction != active_player.0 || *kind != EntityKind::Worker {
                continue;
            }
            carrying.amount = 0;
            carrying.weight = 0.0;
            carrying.resource_type = None;
            // If the worker was stuck returning/waiting, reset to idle
            if matches!(
                *state,
                UnitState::ReturningToDeposit { .. }
                    | UnitState::WaitingForStorage { .. }
                    | UnitState::WaitingForDepot { .. }
                    | UnitState::Depositing { .. }
            ) {
                *state = UnitState::Idle;
            }
        }
    }
}

// ── Rally point button handler ──

pub fn handle_rally_point_button(
    mut commands: Commands,
    interactions: Query<&Interaction, (Changed<Interaction>, With<RallyPointButton>)>,
    mut online: OnlineInputParams,
    mut rally_mode: ResMut<RallyPointMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    camera_q: Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    graphics: Res<GraphicsSettings>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            mark_click(&mut ui_clicked, &mut ui_press);
            rally_mode.0 = !rally_mode.0;
        }
    }

    if rally_mode.0 && mouse.just_pressed(MouseButton::Left) {
        let Ok(window) = windows.single() else { return };
        let Ok((camera, cam_gt)) = camera_q.single() else {
            return;
        };
        let Some(ray) = camera::viewport_ray_from_window_cursor(camera, cam_gt, window, &graphics)
        else {
            return;
        };
        let Some(dist) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) else {
            return;
        };
        let world_pos = ray.get_point(dist);

        for entity in &selected_buildings {
            if *online.net_role != NetRole::Offline {
                let Some(entity_ids) = online.submit.entity_ids_for([entity]) else {
                    continue;
                };
                let Some(&building_id) = entity_ids.first() else {
                    continue;
                };
                push_online_input(
                    &mut online.submit,
                    Vec::new(),
                    vec![InputCommand::SetRallyPoint {
                        building_id,
                        position: [world_pos.x, world_pos.y, world_pos.z],
                    }],
                );
            } else {
                commands.entity(entity).insert(RallyPoint(world_pos));
            }
        }
        rally_mode.0 = false;
    }

    if rally_mode.0 && mouse.just_pressed(MouseButton::Right) {
        rally_mode.0 = false;
    }
}

// ── Toggle auto-attack handler ──

pub fn handle_toggle_auto_attack(
    interactions: Query<&Interaction, (Changed<Interaction>, With<ToggleAutoAttackButton>)>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut auto_attacks: Query<&mut TowerAutoAttackEnabled>,
    mut online: OnlineInputParams,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        for entity in &selected_buildings {
            if *online.net_role != NetRole::Offline {
                let Some(entity_ids) = online.submit.entity_ids_for([entity]) else {
                    continue;
                };
                let Some(&building_id) = entity_ids.first() else {
                    continue;
                };
                push_online_input(
                    &mut online.submit,
                    Vec::new(),
                    vec![InputCommand::ToggleAutoAttack { building_id }],
                );
            } else if let Ok(mut aa) = auto_attacks.get_mut(entity) {
                aa.0 = !aa.0;
            }
        }
    }
}

// ── Assign/Unassign worker to processor building ──

fn processor_assignment_priority(
    kind: EntityKind,
    state: UnitState,
    source: TaskSource,
    carrying: &Carrying,
    queue: &TaskQueue,
    assignment: Option<&BuildingAssignment>,
) -> Option<u8> {
    if kind != EntityKind::Worker {
        return None;
    }
    if assignment.is_some() || carrying.amount > 0 {
        return None;
    }
    if source == TaskSource::Manual || queue.current.is_some() || !queue.queue.is_empty() {
        return None;
    }

    match state {
        UnitState::Idle => Some(0),
        UnitState::Gathering(_)
        | UnitState::Moving(_)
        | UnitState::ReturningToDeposit { .. }
        | UnitState::Depositing { .. }
        | UnitState::WaitingForStorage { .. }
        | UnitState::WaitingForDepot { .. } => Some(1),
        _ => None,
    }
}

pub fn handle_assign_worker_button(
    mut commands: Commands,
    interactions: Query<&Interaction, (Changed<Interaction>, With<AssignWorkerButton>)>,
    selected_buildings: Query<
        (Entity, &ResourceProcessor, &Transform, Option<&SawmillYard>),
        (With<Building>, With<Selected>),
    >,
    mut worker_queries: ParamSet<(
        Query<
            (
                Entity,
                &EntityKind,
                &UnitState,
                &Faction,
                &TaskSource,
                &Carrying,
                &TaskQueue,
                Option<&BuildingAssignment>,
                &Transform,
            ),
            (With<Unit>, With<GatherSpeed>, Without<Building>),
        >,
        Query<&mut Transform, (With<Unit>, With<GatherSpeed>, Without<Building>)>,
        Query<&mut TaskQueue, With<Unit>>,
    )>,
    assigned_workers_q: Query<&AssignedWorkers>,
    active_player: Res<ActivePlayer>,
    height_map: Res<crate::world::ground::HeightMap>,
    time: Res<Time<bevy::time::Fixed>>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        for (building_entity, processor, building_tf, sawmill_yard) in &selected_buildings {
            let current_count = assigned_workers_q
                .get(building_entity)
                .map(|aw| aw.workers.len())
                .unwrap_or(0);
            if current_count >= processor.max_workers as usize {
                continue;
            }

            // Compute yard center for sawmills so workers can be placed inside the fence
            let yard_center = if sawmill_yard.is_some() {
                Some(building_tf.translation + crate::simulation::buildings::SAWMILL_YARD_OFFSET)
            } else {
                None
            };

            let mut best: Option<(Entity, u8, f32)> = None;
            for (
                worker_entity,
                kind,
                state,
                faction,
                source,
                carrying,
                queue,
                assignment,
                worker_tf,
            ) in worker_queries.p0().iter()
            {
                if *faction != active_player.0 {
                    continue;
                }
                let Some(priority) = processor_assignment_priority(
                    *kind, *state, *source, carrying, queue, assignment,
                ) else {
                    continue;
                };
                let dist = worker_tf.translation.distance(building_tf.translation);
                let dominated = best.map_or(false, |(_, best_priority, best_dist)| {
                    priority > best_priority || (priority == best_priority && dist >= best_dist)
                });
                if !dominated {
                    best = Some((worker_entity, priority, dist));
                }
            }

            let Some((worker_entity, _, _)) = best else {
                continue;
            };

            clear_combat_intent(&mut commands, worker_entity, time.elapsed_secs_f64());
            commands
                .entity(worker_entity)
                .remove::<ManualIdleSince>();
            if let Ok(mut queue) = worker_queries.p2().get_mut(worker_entity) {
                queue.clear();
            }

            crate::simulation::resources::assign_worker_to_processor(
                &mut commands,
                worker_entity,
                building_entity,
                building_tf.translation,
                TaskSource::Manual,
            );

            // Teleport worker inside the fence for sawmills
            if let Some(center) = yard_center {
                if let Ok(mut worker_tf) = worker_queries.p1().get_mut(worker_entity) {
                    let slot_offset = crate::simulation::buildings::SAWMILL_TREE_SLOTS
                        .get(current_count)
                        .copied()
                        .unwrap_or(Vec3::ZERO);
                    let target_pos = center + slot_offset * 0.5;
                    let ground_y = height_map.sample(target_pos.x, target_pos.z);
                    worker_tf.translation = Vec3::new(target_pos.x, ground_y, target_pos.z);
                }
            }
        }
    }
}

pub fn handle_unassign_worker_button(
    mut commands: Commands,
    interactions: Query<&Interaction, (Changed<Interaction>, With<UnassignWorkerButton>)>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    assigned_workers_q: Query<&AssignedWorkers>,
    _unit_states: Query<&UnitState, With<Unit>>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        for building_entity in &selected_buildings {
            if let Ok(aw) = assigned_workers_q.get(building_entity) {
                let workers_to_unassign: Vec<Entity> = aw.workers.clone();
                for worker_entity in workers_to_unassign {
                    crate::simulation::resources::unassign_worker_from_processor(
                        &mut commands,
                        worker_entity,
                        Some(building_entity),
                    );
                }
            }
        }
    }
}

pub fn handle_unassign_specific_worker_button(
    mut commands: Commands,
    interactions: Query<(&Interaction, &UnassignSpecificWorkerButton), Changed<Interaction>>,
    worker_assignments: Query<&BuildingAssignment>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        let worker = btn.0;
        let building = worker_assignments
            .get(worker)
            .ok()
            .map(|assignment| assignment.0);
        crate::simulation::resources::unassign_worker_from_processor(
            &mut commands,
            worker,
            building,
        );
    }
}

// ── Unassign one worker (last in list) ──

pub fn handle_unassign_one_worker_button(
    mut commands: Commands,
    interactions: Query<&Interaction, (Changed<Interaction>, With<UnassignOneWorkerButton>)>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut assigned_workers_q: Query<&mut AssignedWorkers>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        for building_entity in &selected_buildings {
            if let Ok(mut aw) = assigned_workers_q.get_mut(building_entity) {
                if let Some(worker_entity) = aw.workers.pop() {
                    crate::simulation::resources::unassign_worker_from_processor(
                        &mut commands,
                        worker_entity,
                        Some(building_entity),
                    );
                }
            }
        }
    }
}

// ── Pause/Resume building toggle ──

pub fn handle_pause_building_button(
    mut commands: Commands,
    interactions: Query<&Interaction, (Changed<Interaction>, With<PauseBuildingButton>)>,
    selected_buildings: Query<(Entity, Option<&BuildingPaused>), (With<Building>, With<Selected>)>,
    mut online: OnlineInputParams,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        for (building_entity, paused) in &selected_buildings {
            if *online.net_role != NetRole::Offline {
                let Some(entity_ids) = online.submit.entity_ids_for([building_entity]) else {
                    continue;
                };
                let Some(&building_id) = entity_ids.first() else {
                    continue;
                };
                push_online_input(
                    &mut online.submit,
                    Vec::new(),
                    vec![InputCommand::TogglePauseBuilding { building_id }],
                );
            } else if paused.is_some() {
                commands.entity(building_entity).remove::<BuildingPaused>();
            } else {
                commands.entity(building_entity).insert(BuildingPaused);
            }
        }
    }
}

// ── Select production recipe ──

pub fn handle_select_recipe_button(
    interactions: Query<(&Interaction, &SelectRecipeButton), Changed<Interaction>>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut productions: Query<&mut ProductionState>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        for building_entity in &selected_buildings {
            if let Ok(mut prod) = productions.get_mut(building_entity) {
                if prod.active_recipe == Some(btn.0) {
                    // Click active recipe again → stop production
                    prod.active_recipe = None;
                } else {
                    prod.active_recipe = Some(btn.0);
                    prod.progress_timer.reset();
                }
            }
        }
    }
}

// ── Cancel training queue item handler ──

pub fn handle_cancel_train(
    interactions: Query<(&Interaction, &CancelTrainButton), Changed<Interaction>>,
    selected_buildings: Query<Entity, (With<Building>, With<Selected>)>,
    mut queues: Query<&mut TrainingQueue>,
    registry: Res<BlueprintRegistry>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, cancel_btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        for building_entity in &selected_buildings {
            if let Ok(mut queue) = queues.get_mut(building_entity) {
                let idx = cancel_btn.0;
                if idx < queue.queue.len() {
                    let removed_kind = queue.queue.remove(idx);
                    let bp = registry.get(removed_kind);
                    let player_res = all_resources.get_mut(&active_player.0);
                    for (i, &amt) in bp.cost.amounts.iter().enumerate() {
                        player_res.amounts[i] += amt;
                    }
                    if idx == 0 {
                        queue.timer = None;
                    }
                }
                break;
            }
        }
    }
}

/// Action bar fade-out/fade-in transition system
pub fn action_bar_transition_system(
    mut commands: Commands,
    time: Res<Time>,
    mut fade_outs: Query<(Entity, &mut ActionBarFadeOut, &mut Transform)>,
    mut fade_ins: Query<
        (
            Entity,
            &mut ActionBarFadeIn,
            &mut Transform,
            &mut Visibility,
        ),
        Without<ActionBarFadeOut>,
    >,
) {
    let dt = time.delta_secs();

    for (entity, mut fade, mut transform) in &mut fade_outs {
        fade.timer.tick(std::time::Duration::from_secs_f32(dt));
        let t = fade.timer.fraction();
        let scale = 1.0 - t * 0.05;
        transform.scale = Vec3::splat(scale);
        transform.translation.y = -t * 8.0 + fade.initial_offset;

        if fade.timer.is_finished() {
            commands.entity(entity).try_despawn();
        }
    }

    for (entity, mut fade, mut transform, mut vis) in &mut fade_ins {
        fade.delay.tick(std::time::Duration::from_secs_f32(dt));
        if !fade.delay.is_finished() {
            continue;
        }
        if !fade.started {
            fade.started = true;
            *vis = Visibility::Inherited;
        }
        fade.timer.tick(std::time::Duration::from_secs_f32(dt));
        let t = fade.timer.fraction();
        let scale = 0.95 + t * 0.05;
        transform.scale = Vec3::splat(scale);
        transform.translation.y = (1.0 - t) * 8.0;

        if fade.timer.is_finished() {
            commands.entity(entity).remove::<ActionBarFadeIn>();
            transform.scale = Vec3::ONE;
            transform.translation.y = 0.0;
        }
    }
}

pub fn update_training_queue_display(
    selected_buildings: Query<&TrainingQueue, (With<Building>, With<Selected>)>,
    mut progress_bars: Query<&mut Node, With<TrainingProgressBar>>,
) {
    let Ok(queue) = selected_buildings.single() else {
        return;
    };
    for mut node in &mut progress_bars {
        let fraction = queue.timer.as_ref().map_or(0.0, |t| t.fraction());
        node.width = Val::Percent(fraction * 100.0);
    }
}

pub fn update_train_cost_colors(
    theme: Res<Theme>,
    all_resources: Res<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    carried_totals: Res<CarriedResourceTotals>,
    registry: Res<BlueprintRegistry>,
    all_units: Query<&Faction, With<Unit>>,
    all_training_queues: Query<(&Faction, &TrainingQueue), With<Building>>,
    all_buildings_for_cap: Query<
        (&Faction, &EntityKind, &BuildingState, &BuildingLevel),
        With<Building>,
    >,
    mut cost_texts: Query<(&TrainCostText, &mut TextColor)>,
) {
    let player_res = all_resources.get(&active_player.0);
    let carried = carried_totals.get(&active_player.0);
    let unit_cap = faction_unit_cap_stats(
        active_player.0,
        all_units.iter(),
        all_training_queues.iter(),
        all_buildings_for_cap.iter(),
    );
    for (cost_text, mut color) in &mut cost_texts {
        let bp = registry.get(cost_text.kind);
        if bp.cost.can_afford_with_carried(player_res, carried) && unit_cap.has_room(1) {
            *color = TextColor(theme.colors.text_secondary);
        } else {
            *color = TextColor(theme.colors.destructive);
        }
    }
}

pub fn update_construction_progress_display(
    selected_buildings: Query<(Entity, &ConstructionProgress), (With<Building>, With<Selected>)>,
    mut progress_bars: Query<&mut Node, With<ConstructionProgressBar>>,
    mut worker_texts: Query<(&mut Text, &mut TextColor), With<ConstructionWorkerCountText>>,
    workers: Query<&UnitState, With<Unit>>,
) {
    let Ok((building_entity, progress)) = selected_buildings.single() else {
        return;
    };
    for mut node in &mut progress_bars {
        node.width = Val::Percent(progress.timer.fraction() * 100.0);
    }

    let builder_count = workers
        .iter()
        .filter(|state| matches!(state, UnitState::Building(e) if *e == building_entity))
        .count();

    for (mut text, mut color) in &mut worker_texts {
        if builder_count == 0 {
            **text = "Waiting for workers...".to_string();
            *color = TextColor(theme::PRESTIGE);
        } else {
            let pct = (progress.timer.fraction() * 100.0) as u32;
            **text = format!(
                "{}% ({} worker{})",
                pct,
                builder_count,
                if builder_count == 1 { "" } else { "s" }
            );
            *color = TextColor(theme::SUCCESS);
        }
    }
}

pub fn update_upgrade_progress_display(
    selected_buildings: Query<&UpgradeProgress, (With<Building>, With<Selected>)>,
    mut progress_bars: Query<&mut Node, With<UpgradeProgressBar>>,
) {
    let Ok(progress) = selected_buildings.single() else {
        return;
    };
    for mut node in &mut progress_bars {
        node.width = Val::Percent(progress.timer.fraction() * 100.0);
    }
}

// ── Unit command button handlers ──

pub fn handle_command_mode_buttons(
    interactions: Query<(&Interaction, &CommandModeButton), Changed<Interaction>>,
    mut cmd_mode: ResMut<CommandMode>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, btn) in &interactions {
        if *interaction == Interaction::Pressed {
            mark_click(&mut ui_clicked, &mut ui_press);
            *cmd_mode = btn.0;
        }
    }
}

pub fn handle_hold_position_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<HoldPositionButton>)>,
    mut online: OnlineInputParams,
    mut commands: Commands,
    selected_units: Query<(Entity, &Faction), (With<Unit>, With<Selected>)>,
    mut task_queues: Query<&mut TaskQueue, With<Unit>>,
    active_player: Res<ActivePlayer>,
    mut next_task_id: ResMut<NextTaskId>,
    mut cmd_mode: ResMut<CommandMode>,
    time: Res<Time<bevy::time::Fixed>>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        *cmd_mode = CommandMode::Normal;

        if online.submit.submit(
            selected_units
                .iter()
                .filter(|(_, faction)| **faction == active_player.0)
                .map(|(entity, _)| entity),
            vec![InputCommand::HoldPosition],
        ) {
            continue;
        }

        for (entity, faction) in &selected_units {
            if *faction != active_player.0 {
                continue;
            }
            apply_manual_hold_intent(&mut commands, entity, time.elapsed_secs_f64());
            commands
                .entity(entity)
                .remove::<MoveTarget>()
                .insert(UnitState::HoldPosition)
                .insert(TaskSource::Manual);
            if let Ok(mut queue) = task_queues.get_mut(entity) {
                queue.clear_queued();
                crate::simulation::orders::set_current_task(
                    &mut queue,
                    &mut next_task_id,
                    QueuedTask::HoldPosition,
                );
            }
        }
    }
}

pub fn handle_stop_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<StopButton>)>,
    mut online: OnlineInputParams,
    mut commands: Commands,
    selected_units: Query<
        (Entity, &Faction, &EntityKind, Option<&BuildingAssignment>),
        (With<Unit>, With<Selected>),
    >,
    active_player: Res<ActivePlayer>,
    mut cmd_mode: ResMut<CommandMode>,
    time: Res<Time<bevy::time::Fixed>>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;
        *cmd_mode = CommandMode::Normal;

        if online.submit.submit(
            selected_units
                .iter()
                .filter(|(_, faction, _, _)| **faction == active_player.0)
                .map(|(entity, _, _, _)| entity),
            vec![InputCommand::Stop],
        ) {
            continue;
        }

        for (entity, faction, kind, assignment) in &selected_units {
            if *faction != active_player.0 {
                continue;
            }
            clear_combat_intent(&mut commands, entity, time.elapsed_secs_f64());
            let grace = ManualIdleSince(time.elapsed_secs_f64());
            if *kind == EntityKind::Worker {
                crate::simulation::resources::unassign_worker_from_processor(
                    &mut commands,
                    entity,
                    assignment.map(|assignment| assignment.0),
                );
                commands.entity(entity).insert(grace);
            } else {
                commands
                    .entity(entity)
                    .remove::<MoveTarget>()
                    .insert(UnitState::Idle)
                    .insert(TaskSource::Auto)
                    .insert(grace);
            }
            commands
                .entity(entity)
                .entry::<TaskQueue>()
                .and_modify(|mut tq| tq.clear());
        }
    }
}

pub fn handle_cycle_stance_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<CycleStanceButton>)>,
    mut commands: Commands,
    mut online: OnlineInputParams,
    selected_units: Query<(Entity, &Faction, Option<&UnitStance>), (With<Unit>, With<Selected>)>,
    active_player: Res<ActivePlayer>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        if *online.net_role != NetRole::Offline {
            let mut entities = Vec::new();
            let mut next_stance = None;
            for (entity, faction, stance) in &selected_units {
                if *faction != active_player.0 {
                    continue;
                }
                entities.push(entity);
                if next_stance.is_none() {
                    next_stance = Some(stance.copied().unwrap_or(UnitStance::Defensive).cycle());
                }
            }
            let Some(stance) = next_stance else {
                continue;
            };
            online.submit.submit(
                entities,
                vec![InputCommand::SetStance {
                    stance: stance.to_u8(),
                }],
            );
            continue;
        }

        for (entity, faction, _) in &selected_units {
            if *faction != active_player.0 {
                continue;
            }
            commands
                .entity(entity)
                .entry::<UnitStance>()
                .and_modify(|mut stance| {
                    *stance = stance.cycle();
                });
        }
    }
}

pub fn handle_formation_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<CycleFormationButton>)>,
    mut formation: ResMut<ActiveFormation>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            mark_click(&mut ui_clicked, &mut ui_press);
            formation.formation = formation.formation.cycle();
        }
    }
}

pub fn handle_prefer_resource_button(
    interactions: Query<&Interaction, (Changed<Interaction>, With<PreferResourceButton>)>,
    mut commands: Commands,
    mut online: OnlineInputParams,
    selected: Query<
        (Entity, &Faction, &EntityKind, Option<&PreferredResource>),
        (With<Unit>, With<Selected>),
    >,
    active_player: Res<ActivePlayer>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        mark_click(&mut ui_clicked, &mut ui_press);

        let current = SelectedWorkerPreference::from_worker_prefs(
            selected
                .iter()
                .filter(|(_, f, k, _)| **f == active_player.0 && **k == EntityKind::Worker)
                .map(|(_, _, _, pref)| pref.map(|p| p.0)),
        );
        let next = current
            .unwrap_or(SelectedWorkerPreference::Off)
            .cycle_target();

        if *online.net_role != NetRole::Offline {
            online.submit.submit(
                selected
                    .iter()
                    .filter(|(_, faction, kind, _)| {
                        **faction == active_player.0 && **kind == EntityKind::Worker
                    })
                    .map(|(entity, _, _, _)| entity),
                vec![InputCommand::SetPreferredResource {
                    resource: next.map(|rt| rt.index() as u8).unwrap_or(u8::MAX),
                }],
            );
            continue;
        }

        for (entity, faction, kind, _) in &selected {
            if *faction != active_player.0 || *kind != EntityKind::Worker {
                continue;
            }
            match next {
                Some(resource) => {
                    commands.entity(entity).insert(PreferredResource(resource));
                }
                None => {
                    commands.entity(entity).remove::<PreferredResource>();
                }
            }
        }
    }
}

pub fn handle_ability_button(
    mut commands: Commands,
    interactions: Query<(&Interaction, &AbilityButton), Changed<Interaction>>,
    mut online: OnlineInputParams,
    combat_registry: Res<AbilityRegistry>,
    mut selected_units: Query<
        (
            Entity,
            &Faction,
            &crate::simulation::combat::Abilities,
            &Transform,
        ),
        (With<Unit>, With<Selected>),
    >,
    active_player: Res<ActivePlayer>,
    mut cmd_mode: ResMut<CommandMode>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
) {
    for (interaction, ability_btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        ui_clicked.0 = 2;
        ui_press.0 = true;

        let ability = ability_btn.0;
        let combat_id = ability.combat_id();

        if ability.targeting() == AbilityTargeting::NoTarget {
            if online.submit.submit(
                selected_units
                    .iter_mut()
                    .filter(|(_, faction, abilities, _)| {
                        **faction == active_player.0
                            && abilities.castable.contains(&combat_id)
                            && abilities.is_ready(&combat_id)
                    })
                    .map(|(entity, _, _, _)| entity),
                vec![InputCommand::UseAbility {
                    ability_id: ability.to_u8(),
                    target: [0.0, 0.0, 0.0],
                }],
            ) {
                continue;
            }
            // Execute immediately on all selected units that have this ability
            let Some(ability_meta) = combat_registry.get(&combat_id) else {
                continue;
            };
            for (entity, faction, abilities, _tf) in &mut selected_units {
                if *faction != active_player.0 {
                    continue;
                }
                if abilities.castable.contains(&combat_id) && abilities.is_ready(&combat_id) {
                    let target = match ability_meta.target_type {
                        crate::simulation::combat::TargetType::Ground => {
                            CastTarget::Ground(Vec3::ZERO)
                        }
                        _ => CastTarget::SelfOnly,
                    };
                    apply_manual_cast_intent(
                        &mut commands,
                        entity,
                        combat_id.clone(),
                        target,
                        0.0,
                    );
                }
            }
        } else {
            // Enter ability targeting mode
            *cmd_mode = CommandMode::AbilityTarget(ability);
        }
    }
}
