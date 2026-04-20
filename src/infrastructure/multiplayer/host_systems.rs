//! Host-side systems: `execute_input_command` shared with lockstep,
//! non-input client message handling (joins/leaves/pings), and disconnect
//! handling. All state sync / entity replication was removed when the
//! simulation moved to deterministic lockstep.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_matchbox::prelude::*;

use game_state::message::{ClientMessage, GameEvent, InputCommand, PlayerInput, ServerMessage};

use crate::blueprints::{BlueprintRegistry, EntityKind, LevelBonus};
use crate::infrastructure::net_bridge::EntityNetMap;
use crate::simulation::buildings::{cleanup_worker_assignment, find_best_worker_for_build};
use crate::simulation::combat::{
    apply_manual_attack_intent, apply_manual_attack_move_intent, apply_manual_hold_intent,
    apply_manual_move_intent, clear_combat_intent,
};
use crate::simulation::orders;
use crate::types::*;
use crate::ui::event_log_widget::{EventCategory, GameEventLog, LogLevel};

use super::debug_tap;
use super::transport::{self, MatchboxInbox, PeerMap};
use super::{HostNetState, LobbyState};

#[derive(Clone, Copy)]
pub struct BuildWorkerSnapshot {
    pub entity: Entity,
    pub translation: Vec3,
    pub state: UnitState,
    pub faction: Faction,
    pub kind: EntityKind,
}

/// Send a ServerMessage to a specific client by player_id.
fn send_to_player(
    socket: &mut MatchboxSocket,
    peer_map: &PeerMap,
    player_id: u8,
    msg: &ServerMessage,
) {
    transport::send_to_player(socket, peer_map, player_id, msg);
}

/// Broadcast a single ServerMessage to all connected peers (reliable channel).
fn broadcast_msg(socket: &mut MatchboxSocket, msg: &ServerMessage) {
    transport::broadcast_reliable(socket, msg);
}

// ── Shared command execution ────────────────────────────────────────────────

fn building_can_train(
    registry: &BlueprintRegistry,
    building_kind: EntityKind,
    building_level: u8,
    unit_kind: EntityKind,
) -> bool {
    let Some(building) = registry.get(building_kind).building.as_ref() else {
        return false;
    };
    if building.trains.contains(&unit_kind) {
        return true;
    }

    building
        .level_upgrades
        .iter()
        .enumerate()
        .take(building_level.saturating_sub(1) as usize)
        .any(|(_, upgrade)| {
            matches!(
                &upgrade.bonus,
                LevelBonus::UnlocksTraining(kinds) if kinds.contains(&unit_kind)
            )
        })
}

/// Bundled host-side system params for the full contextual command executor.
/// Currently only used by tests; lockstep uses the narrower signature below.
#[derive(SystemParam)]
#[allow(dead_code)]
pub struct HostCommandExecution<'w, 's> {
    registry: Res<'w, BlueprintRegistry>,
    carried_totals: Res<'w, CarriedResourceTotals>,
    carried_totals_dirty: ResMut<'w, crate::simulation::resources::CarriedTotalsDirty>,
    pending_drains: ResMut<'w, PendingCarriedDrains>,
    all_resources: ResMut<'w, AllPlayerResources>,
    carrying: Query<'w, 's, &'static mut Carrying, With<Unit>>,
    worker_assignments: Query<'w, 's, &'static BuildingAssignment, With<Unit>>,
    unit_states: ParamSet<
        'w,
        's,
        (
            Query<'w, 's, &'static mut UnitState>,
            Query<
                'w,
                's,
                (
                    Entity,
                    &'static Transform,
                    &'static UnitState,
                    &'static Faction,
                    &'static EntityKind,
                    Option<&'static PendingBuildOrder>,
                ),
                With<Unit>,
            >,
        ),
    >,
    task_queues: Query<'w, 's, &'static mut TaskQueue, With<Unit>>,
    training_buildings: ParamSet<
        'w,
        's,
        (
            Query<
                'w,
                's,
                (
                    &'static mut TrainingQueue,
                    &'static EntityKind,
                    Option<&'static BuildingLevel>,
                ),
                With<Building>,
            >,
            Query<'w, 's, (&'static Faction, &'static TrainingQueue), With<Building>>,
        ),
    >,
    next_task_id: ResMut<'w, NextTaskId>,
    transforms: Query<'w, 's, &'static GlobalTransform>,
}

/// Execute a player input command on the ECS. Called from the lockstep
/// apply system every tick whose gate is open — runs identically on every
/// peer, so the simulation stays in sync.
pub fn execute_input_command(
    commands: &mut Commands,
    input: &PlayerInput,
    issue_time: f64,
    lobby: &LobbyState,
    net_map: &EntityNetMap,
    all_resources: &mut ResMut<AllPlayerResources>,
    carried_totals: &CarriedResourceTotals,
    carried_totals_dirty: &mut ResMut<crate::simulation::resources::CarriedTotalsDirty>,
    pending_drains: &mut ResMut<PendingCarriedDrains>,
    unit_states: &mut Query<&mut UnitState>,
    carrying_q: &mut Query<&mut Carrying, With<Unit>>,
    health_q: &mut Query<&mut Health, With<Unit>>,
    unit_abilities_q: &mut Query<&mut UnitAbilities, With<Unit>>,
    worker_assignments: &Query<&BuildingAssignment, With<Unit>>,
    task_queues: &mut Query<&mut TaskQueue, With<Unit>>,
    training_buildings: &mut Query<
        (&mut TrainingQueue, &EntityKind, Option<&BuildingLevel>),
        With<Building>,
    >,
    next_task_id: &mut ResMut<NextTaskId>,
    transforms: &Query<&GlobalTransform>,
    existing_buildings: &Query<
        (&Transform, &BuildingFootprint, &EntityKind),
        (With<Building>, Without<GhostBuilding>),
    >,
    building_state: &Query<(&Faction, Has<BuildingPaused>), With<Building>>,
    tower_auto_attack: &mut Query<&mut TowerAutoAttackEnabled, With<Building>>,
    workers: &[BuildWorkerSnapshot],
    obstacle_grid: &ObstacleGrid,
    registry: &BlueprintRegistry,
    pending_lockstep_builds: &mut ResMut<PendingLockstepBuilds>,
) {
    let input_faction = lobby
        .players
        .iter()
        .find(|player| player.player_id as u32 == input.player_id)
        .map(|player| player.faction);

    for cmd in &input.commands {
        match cmd {
            InputCommand::Move { target, formation } => {
                let pos = Vec3::new(target[0], target[1], target[2]);
                let n = input.entity_ids.len();
                let formation = formation
                    .map(FormationType::from_net_u8)
                    .unwrap_or_default();
                let centroid = if n > 1 {
                    let mut sum = Vec3::ZERO;
                    let mut counted = 0usize;
                    for &eid in &input.entity_ids {
                        let Some(&ecs_entity) = net_map.to_ecs.get(&eid) else {
                            continue;
                        };
                        let Ok(tf) = transforms.get(ecs_entity) else {
                            continue;
                        };
                        sum += tf.translation();
                        counted += 1;
                    }
                    if counted > 0 {
                        sum / counted as f32
                    } else {
                        pos
                    }
                } else {
                    pos
                };
                let facing = Vec2::new(pos.x - centroid.x, pos.z - centroid.z).normalize_or_zero();
                let offsets = if n > 1 {
                    formation_offsets(formation, n, facing)
                } else {
                    Vec::new()
                };
                for (i, &eid) in input.entity_ids.iter().enumerate() {
                    if let Some(&ecs_entity) = net_map.to_ecs.get(&eid) {
                        let dest = if n > 1 {
                            let offset = offsets.get(i).copied().unwrap_or(Vec2::ZERO);
                            pos + Vec3::new(offset.x, 0.0, offset.y)
                        } else {
                            pos
                        };
                        if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
                            *state = UnitState::Moving(dest);
                        }
                        apply_manual_move_intent(commands, ecs_entity, dest, issue_time);
                        commands
                            .entity(ecs_entity)
                            .remove::<AttackTarget>()
                            .insert(MoveTarget(dest))
                            .insert(TaskSource::Manual);
                        if let Ok(mut queue) = task_queues.get_mut(ecs_entity) {
                            queue.clear_queued();
                            orders::set_current_task(
                                &mut queue,
                                next_task_id,
                                QueuedTask::Move(dest),
                            );
                        }
                    }
                }
            }
            InputCommand::Attack { target_id } => {
                if let Some(&target_ecs) = net_map.to_ecs.get(target_id) {
                    for &eid in &input.entity_ids {
                        if let Some(&ecs_entity) = net_map.to_ecs.get(&eid) {
                            apply_manual_attack_intent(
                                commands, ecs_entity, target_ecs, issue_time,
                            );
                            commands
                                .entity(ecs_entity)
                                .remove::<MoveTarget>()
                                .insert(AttackTarget(target_ecs))
                                .insert(TaskSource::Manual);
                            if let Ok(mut queue) = task_queues.get_mut(ecs_entity) {
                                queue.clear_queued();
                                orders::set_current_task(
                                    &mut queue,
                                    next_task_id,
                                    QueuedTask::Attack(target_ecs),
                                );
                            }
                        }
                    }
                }
            }
            InputCommand::Gather { target_id } => {
                if let Some(&target_ecs) = net_map.to_ecs.get(target_id) {
                    let node_pos = transforms
                        .get(target_ecs)
                        .map(|gt| gt.translation())
                        .unwrap_or(Vec3::ZERO);
                    for &eid in &input.entity_ids {
                        if let Some(&ecs_entity) = net_map.to_ecs.get(&eid) {
                            if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
                                *state = UnitState::Gathering(target_ecs);
                            }
                            clear_combat_intent(commands, ecs_entity, issue_time);
                            commands
                                .entity(ecs_entity)
                                .remove::<AttackTarget>()
                                .insert(MoveTarget(node_pos))
                                .insert(TaskSource::Manual);
                            if let Ok(mut queue) = task_queues.get_mut(ecs_entity) {
                                queue.clear_queued();
                                orders::set_current_task(
                                    &mut queue,
                                    next_task_id,
                                    QueuedTask::Gather(target_ecs),
                                );
                            }
                        }
                    }
                }
            }
            InputCommand::UseAbility { ability_id, target } => {
                let ability = AbilityId::from_u8(*ability_id);
                let target_pos = Vec3::new(target[0], target[1], target[2]);
                for &eid in &input.entity_ids {
                    let Some(&ecs_entity) = net_map.to_ecs.get(&eid) else {
                        continue;
                    };
                    let Ok(mut unit_abilities) = unit_abilities_q.get_mut(ecs_entity) else {
                        continue;
                    };
                    if !unit_abilities.abilities.contains(&ability)
                        || !unit_abilities.is_ready(ability)
                    {
                        continue;
                    }
                    unit_abilities.trigger_cooldown(ability);
                    commands.entity(ecs_entity).insert(CastingAbility {
                        ability,
                        target_pos: (ability.targeting() != AbilityTargeting::NoTarget)
                            .then_some(target_pos),
                        target_entity: None,
                        cast_timer: Timer::from_seconds(0.3, TimerMode::Once),
                    });
                }
            }
            InputCommand::Patrol { target } => {
                let pos = Vec3::new(target[0], target[1], target[2]);
                for &eid in &input.entity_ids {
                    if let Some(&ecs_entity) = net_map.to_ecs.get(&eid) {
                        if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
                            *state = UnitState::Patrolling {
                                target: pos,
                                origin: pos,
                            };
                        }
                        clear_combat_intent(commands, ecs_entity, issue_time);
                        commands
                            .entity(ecs_entity)
                            .remove::<AttackTarget>()
                            .remove::<PreferredResource>()
                            .insert(MoveTarget(pos))
                            .insert(TaskSource::Manual);
                        if let Ok(mut queue) = task_queues.get_mut(ecs_entity) {
                            queue.clear_queued();
                            orders::set_current_task(
                                &mut queue,
                                next_task_id,
                                QueuedTask::Move(pos),
                            );
                        }
                    }
                }
            }
            InputCommand::AttackMove { target } => {
                let pos = Vec3::new(target[0], target[1], target[2]);
                for &eid in &input.entity_ids {
                    if let Some(&ecs_entity) = net_map.to_ecs.get(&eid) {
                        apply_manual_attack_move_intent(commands, ecs_entity, pos, issue_time);
                        commands
                            .entity(ecs_entity)
                            .remove::<AttackTarget>()
                            .insert(MoveTarget(pos))
                            .insert(TaskSource::Manual);
                        if let Ok(mut queue) = task_queues.get_mut(ecs_entity) {
                            queue.clear_queued();
                            orders::set_current_task(
                                &mut queue,
                                next_task_id,
                                QueuedTask::AttackMove(pos),
                            );
                        }
                    }
                }
            }
            InputCommand::HoldPosition => {
                for &eid in &input.entity_ids {
                    if let Some(&ecs_entity) = net_map.to_ecs.get(&eid) {
                        apply_manual_hold_intent(commands, ecs_entity, issue_time);
                        commands
                            .entity(ecs_entity)
                            .remove::<MoveTarget>()
                            .remove::<AttackTarget>()
                            .insert(UnitState::HoldPosition)
                            .insert(TaskSource::Manual);
                        if let Ok(mut queue) = task_queues.get_mut(ecs_entity) {
                            queue.clear();
                            orders::set_current_task(
                                &mut queue,
                                next_task_id,
                                QueuedTask::HoldPosition,
                            );
                        }
                    }
                }
            }
            InputCommand::Stop => {
                for &eid in &input.entity_ids {
                    if let Some(&ecs_entity) = net_map.to_ecs.get(&eid) {
                        if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
                            *state = UnitState::Idle;
                        }
                        clear_combat_intent(commands, ecs_entity, issue_time);
                        let grace = ManualIdleSince(issue_time);
                        if workers.iter().any(|worker| {
                            worker.entity == ecs_entity && worker.kind == EntityKind::Worker
                        }) {
                            crate::simulation::resources::unassign_worker_from_processor(
                                commands,
                                ecs_entity,
                                worker_assignments
                                    .get(ecs_entity)
                                    .ok()
                                    .map(|assignment| assignment.0),
                            );
                            commands
                                .entity(ecs_entity)
                                .remove::<PreferredResource>()
                                .insert(grace);
                        } else {
                            commands
                                .entity(ecs_entity)
                                .insert(TaskSource::Auto)
                                .insert(grace);
                        }
                        commands
                            .entity(ecs_entity)
                            .remove::<MoveTarget>()
                            .remove::<AttackTarget>()
                            .insert(TaskSource::Auto);
                        if let Ok(mut queue) = task_queues.get_mut(ecs_entity) {
                            queue.clear();
                        }
                    }
                }
            }
            InputCommand::DropCargo => {
                let mut changed_any = false;
                for &eid in &input.entity_ids {
                    let Some(&ecs_entity) = net_map.to_ecs.get(&eid) else {
                        continue;
                    };
                    let Ok(mut carrying) = carrying_q.get_mut(ecs_entity) else {
                        continue;
                    };
                    if carrying.amount == 0 {
                        continue;
                    }
                    carrying.amount = 0;
                    carrying.weight = 0.0;
                    carrying.resource_type = None;
                    changed_any = true;

                    if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
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
                    commands
                        .entity(ecs_entity)
                        .remove::<MoveTarget>()
                        .remove::<AttackTarget>()
                        .insert(TaskSource::Auto);
                    if let Ok(mut queue) = task_queues.get_mut(ecs_entity) {
                        queue.clear();
                    }
                }
                if changed_any {
                    carried_totals_dirty.0 = true;
                }
            }
            InputCommand::Scuttle => {
                for &eid in &input.entity_ids {
                    let Some(&ecs_entity) = net_map.to_ecs.get(&eid) else {
                        continue;
                    };
                    if !workers.iter().any(|worker| {
                        worker.entity == ecs_entity && worker.kind == EntityKind::Worker
                    }) {
                        continue;
                    }
                    let Ok(mut hp) = health_q.get_mut(ecs_entity) else {
                        continue;
                    };
                    hp.current = 0.0;
                }
            }
            InputCommand::Train { building_id, kind } => {
                let Some(&ecs_entity) = net_map.to_ecs.get(building_id) else {
                    continue;
                };
                let Some(unit_kind) = EntityKind::from_index(*kind) else {
                    continue;
                };
                let Ok((mut queue, building_kind, building_level)) =
                    training_buildings.get_mut(ecs_entity)
                else {
                    continue;
                };
                let level = building_level.map_or(1, |level| level.0);
                if building_can_train(registry, *building_kind, level, unit_kind) {
                    queue.queue.push(unit_kind);
                }
            }
            InputCommand::Build { kind, position } => {
                let Some(faction) = input_faction else {
                    continue;
                };
                let Some(kind) = EntityKind::from_index(*kind) else {
                    continue;
                };
                let bp = registry.get(kind);
                let build_pos = Vec3::new(position[0], position[1], position[2]);
                let footprint = crate::simulation::buildings::footprint_for_kind(kind);

                let blocked =
                    existing_buildings
                        .iter()
                        .any(|(building_tf, existing_fp, existing_kind)| {
                            if !crate::simulation::buildings::blocks_construction_overlap(
                                *existing_kind,
                            ) {
                                return false;
                            }
                            let check_pos =
                                Vec3::new(build_pos.x, building_tf.translation.y, build_pos.z);
                            building_tf.translation.distance(check_pos) < existing_fp.0 + footprint
                        });
                if blocked || obstacle_grid.is_footprint_blocked(build_pos, footprint) {
                    continue;
                }

                let player_res = all_resources.get(&faction);
                let carried = carried_totals.get(&faction);
                if !bp.cost.can_afford_with_carried(player_res, carried) {
                    continue;
                }

                let worker_candidates: Vec<_> = workers
                    .iter()
                    .map(|worker| {
                        (
                            worker.entity,
                            Transform::from_translation(worker.translation),
                            worker.state,
                            worker.faction,
                            worker.kind,
                        )
                    })
                    .collect();
                let worker_iter = worker_candidates.iter().map(
                    |(entity, transform, state, worker_faction, worker_kind)| {
                        (
                            entity.to_owned(),
                            transform,
                            state,
                            worker_faction,
                            worker_kind,
                        )
                    },
                );
                let Some((worker_entity, _)) =
                    find_best_worker_for_build(worker_iter, faction, build_pos, |e| {
                        net_map.to_net.get(&e).copied()
                    })
                else {
                    continue;
                };

                let deficits = bp.cost.deduct_with_carried(all_resources.get_mut(&faction));
                let drain = SpendFromCarried {
                    faction,
                    amounts: deficits,
                };
                if drain.has_deficit() {
                    pending_drains.drains.push(drain);
                }

                if let Some(worker) = workers.iter().find(|worker| worker.entity == worker_entity) {
                    cleanup_worker_assignment(commands, worker_entity, &worker.state);
                }
                clear_combat_intent(commands, worker_entity, issue_time);
                commands
                    .entity(worker_entity)
                    .remove::<MoveTarget>()
                    .remove::<AttackTarget>()
                    .insert(UnitState::MovingToPlot(build_pos))
                    .insert(TaskSource::Manual)
                    .insert(PendingBuildOrder {
                        kind,
                        position: build_pos,
                        faction,
                        rotation_y: 0.0,
                    })
                    .insert(MoveTarget(build_pos));
                if let Ok(mut queue) = task_queues.get_mut(worker_entity) {
                    queue.clear_queued();
                }
            }
            InputCommand::SetRallyPoint {
                building_id,
                position,
            } => {
                let Some(&ecs_entity) = net_map.to_ecs.get(building_id) else {
                    continue;
                };
                commands.entity(ecs_entity).insert(RallyPoint(Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                )));
            }
            InputCommand::ToggleAutoAttack { building_id } => {
                let Some(&ecs_entity) = net_map.to_ecs.get(building_id) else {
                    continue;
                };
                let Ok((faction, _)) = building_state.get(ecs_entity) else {
                    continue;
                };
                if Some(*faction) != input_faction {
                    continue;
                }
                if let Ok(mut aa) = tower_auto_attack.get_mut(ecs_entity) {
                    aa.0 = !aa.0;
                }
            }
            InputCommand::TogglePauseBuilding { building_id } => {
                let Some(&ecs_entity) = net_map.to_ecs.get(building_id) else {
                    continue;
                };
                let Ok((faction, is_paused)) = building_state.get(ecs_entity) else {
                    continue;
                };
                if Some(*faction) != input_faction {
                    continue;
                }
                if is_paused {
                    commands.entity(ecs_entity).remove::<BuildingPaused>();
                } else {
                    commands.entity(ecs_entity).insert(BuildingPaused);
                }
            }
            InputCommand::SetStance { stance } => {
                let new_stance = UnitStance::from_u8(*stance);
                for &eid in &input.entity_ids {
                    if let Some(&ecs_entity) = net_map.to_ecs.get(&eid) {
                        commands.entity(ecs_entity).insert(new_stance);
                    }
                }
            }
            InputCommand::SetPreferredResource { resource } => {
                let preferred = if *resource == u8::MAX {
                    None
                } else {
                    ResourceType::ALL.get(*resource as usize).copied()
                };
                for &eid in &input.entity_ids {
                    if let Some(&ecs_entity) = net_map.to_ecs.get(&eid) {
                        match preferred {
                            Some(resource) => {
                                commands
                                    .entity(ecs_entity)
                                    .insert(PreferredResource(resource));
                            }
                            None => {
                                commands.entity(ecs_entity).remove::<PreferredResource>();
                            }
                        }
                    }
                }
            }
            InputCommand::BuildWall { cells } => {
                let Some(faction) = input_faction else {
                    continue;
                };
                if cells.is_empty() {
                    continue;
                }
                let grid_cells: Vec<(i32, i32)> = cells.iter().map(|c| (c[0], c[1])).collect();
                pending_lockstep_builds.walls.push(PendingWallBuild {
                    faction,
                    cells: grid_cells,
                });
            }
            InputCommand::BuildGate { cell } => {
                let Some(faction) = input_faction else {
                    continue;
                };
                pending_lockstep_builds.gates.push(PendingGateBuild {
                    faction,
                    cell: (cell[0], cell[1]),
                });
            }
            InputCommand::BuildFloor { cell } => {
                let Some(faction) = input_faction else {
                    continue;
                };
                pending_lockstep_builds.floors.push(PendingFloorBuild {
                    faction,
                    cell: (cell[0], cell[1]),
                });
            }
            _ => {
                debug!("Unhandled command: {:?}", cmd);
            }
        }
    }
}

// ── System: host_process_client_commands ─────────────────────────────────────

/// Drain non-lockstep client messages (joins, leaves, pings, chat, etc.).
/// Input handling lives in `lockstep::host_receive_remote_inputs` and
/// runs earlier in the network-receive set.
pub fn host_process_client_commands(
    mut socket: ResMut<MatchboxSocket>,
    peer_map: Res<PeerMap>,
    host: Res<HostNetState>,
    mut inbox: ResMut<MatchboxInbox>,
    lobby: Res<super::LobbyState>,
    time: Res<Time>,
    mut event_log: ResMut<GameEventLog>,
) {
    let client_commands = std::mem::take(&mut inbox.client_commands);
    for (player_id, msg) in client_commands {
        match &msg {
            // Lockstep: real handling lives in
            // `lockstep::host_receive_remote_inputs`, which runs first and
            // peels these off the inbox. Anything that reaches here is
            // leftover and safe to ignore.
            ClientMessage::InputBroadcast { .. } => {
                continue;
            }
            // Desync detection: `checksum::host_drain_checksum_reports`
            // peels these off earlier. Anything reaching here is leftover.
            ClientMessage::ChecksumReport { .. } => {
                continue;
            }
            ClientMessage::JoinRequest { player_name, .. } => {
                info!("Player {} joined: {}", player_id, player_name);
                debug_tap::record_info(
                    "host_commands",
                    format!("player {} join request: {}", player_id, player_name),
                );
                event_log.push_with_level(
                    time.elapsed_secs(),
                    format!("{} joined the game", player_name),
                    EventCategory::Network,
                    LogLevel::Info,
                    None,
                    None,
                );
            }
            ClientMessage::LeaveNotice { .. } => {
                info!("Player {} left gracefully", player_id);
                debug_tap::record_info(
                    "host_commands",
                    format!("player {} leave notice", player_id),
                );
                let name = lobby
                    .players
                    .iter()
                    .find(|p| p.player_id == player_id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| format!("Player {}", player_id));
                event_log.push_with_level(
                    time.elapsed_secs(),
                    format!("{} left the game", name),
                    EventCategory::Network,
                    LogLevel::Warning,
                    None,
                    None,
                );
            }
            ClientMessage::Ping { timestamp, .. } => {
                let seq = {
                    let mut s = host.seq.lock().unwrap();
                    *s += 1;
                    *s
                };
                let pong = ServerMessage::Pong {
                    seq,
                    timestamp: *timestamp,
                };
                send_to_player(&mut socket, &peer_map, player_id, &pong);
            }
            ClientMessage::Reconnect { session_token, .. } => {
                info!(
                    "Reconnect request from player {} with token {}",
                    player_id, session_token
                );
                debug_tap::record_info(
                    "host_commands",
                    format!(
                        "player {} reconnect request token={}",
                        player_id, session_token
                    ),
                );
            }
            ClientMessage::Chat { .. } => {
                // Chat during gameplay — not handled here, lobby handles it
            }
            ClientMessage::NameUpdate { .. } => {
                // Name updates are handled in the lobby, not during gameplay
            }
        }
    }
}

/// Detect disconnected clients — start grace period for reconnection.
/// After RECONNECT_GRACE_PERIOD seconds, convert their factions to AI.
pub fn host_handle_disconnects(
    mut socket: ResMut<MatchboxSocket>,
    mut peer_map: ResMut<PeerMap>,
    host: Res<HostNetState>,
    mut inbox: ResMut<MatchboxInbox>,
    mut lobby: ResMut<super::LobbyState>,
    mut ai_factions: ResMut<AiControlledFactions>,
    mut session_tokens: ResMut<super::SessionTokens>,
    time: Res<Time>,
    mut event_log: ResMut<GameEventLog>,
) {
    let disconnected_peers = std::mem::take(&mut inbox.disconnected);
    for peer in disconnected_peers {
        let Some(player_id) = peer_map.remove_peer(&peer) else {
            continue;
        };

        info!(
            "Player {} disconnected — starting {}s reconnection grace period",
            player_id,
            super::RECONNECT_GRACE_PERIOD
        );
        debug_tap::record_info(
            "host_disconnects",
            format!("player {} disconnected", player_id),
        );

        let player_info = lobby.players.iter().find(|p| p.player_id == player_id);
        let player_name = player_info
            .map(|p| p.name.clone())
            .unwrap_or_else(|| format!("Player {}", player_id));

        if let Some(player) = lobby
            .players
            .iter_mut()
            .find(|p| p.player_id == player_id && p.connected)
        {
            player.connected = false;

            let token = session_tokens
                .tokens
                .iter()
                .find(|(_, &pid)| pid == player_id)
                .map(|(&t, _)| t)
                .unwrap_or_else(|| session_tokens.generate(player_id));

            session_tokens.disconnected.push(super::DisconnectedPlayer {
                _session_token: token,
                player_id,
                faction: player.faction,
                _seat_index: player.seat_index,
                _color_index: player.color_index,
                name: player_name.clone(),
                disconnect_time: time.elapsed_secs(),
            });
        }

        event_log.push_with_level(
            time.elapsed_secs(),
            format!(
                "{} disconnected — waiting for reconnection ({}s)",
                player_name,
                super::RECONNECT_GRACE_PERIOD as u32
            ),
            EventCategory::Network,
            LogLevel::Warning,
            None,
            None,
        );

        let seq = {
            let mut s = host.seq.lock().unwrap();
            *s += 1;
            *s
        };
        let announce = ServerMessage::Event {
            seq,
            timestamp: time.elapsed_secs_f64(),
            events: vec![GameEvent::Announcement {
                text: format!("{} disconnected — waiting for reconnection", player_name),
            }],
        };
        broadcast_msg(&mut socket, &announce);
    }

    let now = time.elapsed_secs();
    let expired: Vec<super::DisconnectedPlayer> = session_tokens
        .disconnected
        .extract_if(.., |dc| {
            now - dc.disconnect_time >= super::RECONNECT_GRACE_PERIOD
        })
        .collect();

    for dc in expired {
        info!(
            "Reconnection grace period expired for {} — converting to AI",
            dc.name
        );
        ai_factions.factions.insert(dc.faction);
        session_tokens.tokens.retain(|_, pid| *pid != dc.player_id);
        event_log.push_with_level(
            time.elapsed_secs(),
            format!("{} — reconnection timed out, AI taking over", dc.name),
            EventCategory::Network,
            LogLevel::Warning,
            None,
            None,
        );

        let seq = {
            let mut s = host.seq.lock().unwrap();
            *s += 1;
            *s
        };
        let announce = ServerMessage::Event {
            seq,
            timestamp: time.elapsed_secs_f64(),
            events: vec![GameEvent::Announcement {
                text: format!("{} — AI taking over", dc.name),
            }],
        };
        broadcast_msg(&mut socket, &announce);
    }
}
