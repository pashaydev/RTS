//! Host-side systems: relay client commands, handle disconnects.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::TryRecvError;

use game_state::message::{
    BuildingSnapshot, ClientMessage, DayCycleSnapshot, EntitySnapshot, EntitySpawnData, GameEvent,
    InputCommand, NetCarrying, NetUnitState, PlayerInput, ServerMessage,
};

use crate::components::*;
use crate::lighting::DayCycle;
use crate::net_bridge::{EntityNetMap, NetworkId};
use crate::orders;
use crate::ui::event_log_widget::{EventCategory, GameEventLog, LogLevel};

use super::debug_tap;
use super::{HostNetState, NetStats};

// ── Shared command execution ────────────────────────────────────────────────

/// Execute a player input command on the ECS. Used by both host and client.
/// Mirrors the full component setup that the local right-click handler does,
/// so that MoveTarget, TaskSource, TaskQueue, etc. are all set correctly.
pub fn execute_input_command(
    commands: &mut Commands,
    input: &PlayerInput,
    net_map: &EntityNetMap,
    unit_states: &mut Query<&mut UnitState>,
    task_queues: &mut Query<&mut TaskQueue, With<Unit>>,
    next_task_id: &mut ResMut<NextTaskId>,
    transforms: &Query<&GlobalTransform>,
) {
    for cmd in &input.commands {
        match cmd {
            InputCommand::Move { target } => {
                let pos = Vec3::new(target[0], target[1], target[2]);
                let n = input.entity_ids.len();
                let spacing = 1.5;
                let radius = if n > 1 {
                    (spacing * n as f32 / std::f32::consts::TAU).max(1.0)
                } else {
                    0.0
                };
                for (i, &eid) in input.entity_ids.iter().enumerate() {
                    if let Some(&ecs_entity) = net_map.to_ecs.get(&eid) {
                        let dest = if n > 1 {
                            let angle = i as f32 / n as f32 * std::f32::consts::TAU;
                            let offset =
                                Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
                            pos + offset
                        } else {
                            pos
                        };
                        if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
                            *state = UnitState::Moving(dest);
                        }
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
                            if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
                                *state = UnitState::Attacking(target_ecs);
                            }
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
                        if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
                            *state = UnitState::AttackMoving(pos);
                        }
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
                        if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
                            *state = UnitState::HoldPosition;
                        }
                        commands
                            .entity(ecs_entity)
                            .remove::<MoveTarget>()
                            .remove::<AttackTarget>()
                            .insert(TaskSource::Manual);
                        if let Ok(mut queue) = task_queues.get_mut(ecs_entity) {
                            queue.clear();
                        }
                    }
                }
            }
            _ => {
                debug!("Unhandled command: {:?}", cmd);
            }
        }
    }
}

// ── System: host_process_client_commands ─────────────────────────────────────

/// Poll incoming commands from clients, execute them on host ECS, and relay
/// to all other clients.
pub fn host_process_client_commands(
    mut commands: Commands,
    host: Res<HostNetState>,
    lobby: Res<super::LobbyState>,
    net_map: Res<EntityNetMap>,
    mut unit_states: Query<&mut UnitState>,
    mut task_queues: Query<&mut TaskQueue, With<Unit>>,
    mut next_task_id: ResMut<NextTaskId>,
    transforms: Query<&GlobalTransform>,
    unit_factions: Query<&Faction>,
    time: Res<Time>,
    mut event_log: ResMut<GameEventLog>,
) {
    let rx = host.incoming_commands.lock().unwrap();
    for _ in 0..64 {
        match rx.try_recv() {
            Ok((player_id, msg)) => {
                match &msg {
                    ClientMessage::Input { input, .. } => {
                        let Some(player) = lobby
                            .players
                            .iter()
                            .find(|p| p.player_id == player_id && p.connected)
                        else {
                            debug_tap::record_error(
                                "host_commands",
                                format!("dropping input from unknown/disconnected player {}", player_id),
                            );
                            continue;
                        };

                        let owned_entity_ids: Vec<_> = input
                            .entity_ids
                            .iter()
                            .copied()
                            .filter(|entity_id| {
                                net_map
                                    .to_ecs
                                    .get(entity_id)
                                    .and_then(|entity| unit_factions.get(*entity).ok())
                                    .is_some_and(|faction| *faction == player.faction)
                            })
                            .collect();

                        if owned_entity_ids.is_empty() {
                            debug_tap::record_error(
                                "host_commands",
                                format!("player {} attempted to command no owned units", player_id),
                            );
                            continue;
                        }

                        let mut sanitized_input = input.clone();
                        sanitized_input.player_id = player_id as u32;
                        sanitized_input.entity_ids = owned_entity_ids;

                        // Execute on host ECS
                        execute_input_command(
                            &mut commands,
                            &sanitized_input,
                            &net_map,
                            &mut unit_states,
                            &mut task_queues,
                            &mut next_task_id,
                            &transforms,
                        );
                        debug_tap::record_info(
                            "host_commands",
                            format!(
                                "player {} input: {} entities / {} cmds",
                                player_id,
                                sanitized_input.entity_ids.len(),
                                sanitized_input.commands.len()
                            ),
                        );

                        // Relay to all other clients
                        let seq = {
                            let mut s = host.seq.lock().unwrap();
                            *s += 1;
                            *s
                        };
                        let relay = ServerMessage::RelayedInput {
                            seq,
                            timestamp: time.elapsed_secs_f64(),
                            player_id,
                            input: sanitized_input,
                        };
                        if let Ok(json) = serde_json::to_vec(&relay) {
                            let senders = host.client_senders.lock().unwrap();
                            for (_id, sender) in senders.iter() {
                                let _ = sender.send(json.clone());
                            }
                        }
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
                        // Reply with Pong to keep VPN/Hamachi tunnels alive
                        let seq = {
                            let mut s = host.seq.lock().unwrap();
                            *s += 1;
                            *s
                        };
                        let pong = ServerMessage::Pong {
                            seq,
                            timestamp: *timestamp,
                        };
                        if let Ok(json) = serde_json::to_vec(&pong) {
                            let senders = host.client_senders.lock().unwrap();
                            for (id, sender) in senders.iter() {
                                if *id == player_id {
                                    let _ = sender.send(json.clone());
                                }
                            }
                        }
                    }
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }
}

/// Detect disconnected clients and convert their factions to AI control.
pub fn host_handle_disconnects(
    host: Res<HostNetState>,
    mut lobby: ResMut<super::LobbyState>,
    mut ai_factions: ResMut<AiControlledFactions>,
    time: Res<Time>,
    mut event_log: ResMut<GameEventLog>,
) {
    let dc_rx = host.disconnect_rx.lock().unwrap();
    let mut senders = host.client_senders.lock().unwrap();

    loop {
        match dc_rx.try_recv() {
            Ok(player_id) => {
                info!("Player {} disconnected", player_id);
                debug_tap::record_info("host_disconnects", format!("player {} disconnected", player_id));

                let player_name = lobby
                    .players
                    .iter()
                    .find(|p| p.player_id == player_id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| format!("Player {}", player_id));

                if let Some(player) = lobby
                    .players
                    .iter_mut()
                    .find(|p| p.player_id == player_id && p.connected)
                {
                    player.connected = false;
                    ai_factions.factions.insert(player.faction);
                }

                event_log.push_with_level(
                    time.elapsed_secs(),
                    format!("{} disconnected — AI taking over", player_name),
                    EventCategory::Network,
                    LogLevel::Warning,
                    None,
                    None,
                );

                senders.retain(|(id, _)| *id != player_id);

                let seq = {
                    let mut s = host.seq.lock().unwrap();
                    *s += 1;
                    *s
                };
                let announce = ServerMessage::Event {
                    seq,
                    timestamp: time.elapsed_secs_f64(),
                    events: vec![GameEvent::Announcement {
                        text: format!("Player {} disconnected — AI taking over", player_id),
                    }],
                };
                if let Ok(json) = serde_json::to_vec(&announce) {
                    for (_id, sender) in senders.iter() {
                        let _ = sender.send(json.clone());
                    }
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }
}

// ── State sync: host → clients ──────────────────────────────────────────────

/// Convert ECS `UnitState` to network-safe `NetUnitState`.
/// Falls back to `Idle` if a referenced entity is not in the net map.
fn ecs_to_net_unit_state(state: &UnitState, net_map: &EntityNetMap) -> NetUnitState {
    match state {
        UnitState::Idle => NetUnitState::Idle,
        UnitState::Moving(pos) => NetUnitState::Moving {
            target: [pos.x, pos.y, pos.z],
        },
        UnitState::Attacking(e) => {
            if let Some(&nid) = net_map.to_net.get(e) {
                NetUnitState::Attacking { target_id: nid }
            } else {
                NetUnitState::Idle
            }
        }
        UnitState::Gathering(e) => {
            if let Some(&nid) = net_map.to_net.get(e) {
                NetUnitState::Gathering { target_id: nid }
            } else {
                NetUnitState::Idle
            }
        }
        UnitState::ReturningToDeposit { depot, .. } => {
            if let Some(&nid) = net_map.to_net.get(depot) {
                NetUnitState::Returning { depot_id: nid }
            } else {
                NetUnitState::Idle
            }
        }
        UnitState::Depositing { depot, .. } | UnitState::WaitingForStorage { depot, .. } => {
            if let Some(&nid) = net_map.to_net.get(depot) {
                NetUnitState::Depositing { depot_id: nid }
            } else {
                NetUnitState::Idle
            }
        }
        UnitState::MovingToPlot(pos) => NetUnitState::MovingToPlot {
            target: [pos.x, pos.y, pos.z],
        },
        UnitState::MovingToBuild(e) => {
            if let Some(&nid) = net_map.to_net.get(e) {
                NetUnitState::MovingToBuild { target_id: nid }
            } else {
                NetUnitState::Idle
            }
        }
        UnitState::Building(e) => {
            if let Some(&nid) = net_map.to_net.get(e) {
                NetUnitState::Building { target_id: nid }
            } else {
                NetUnitState::Idle
            }
        }
        UnitState::AssignedGathering { building, phase } => {
            if let Some(&nid) = net_map.to_net.get(building) {
                let phase_u8 = match phase {
                    AssignedPhase::SeekingNode => 0,
                    AssignedPhase::MovingToNode(_) => 1,
                    AssignedPhase::Harvesting { .. } => 2,
                    AssignedPhase::ReturningToBuilding => 3,
                    AssignedPhase::Depositing { .. } => 4,
                };
                NetUnitState::AssignedGathering {
                    building_id: nid,
                    phase: phase_u8,
                }
            } else {
                NetUnitState::Idle
            }
        }
        UnitState::Patrolling { target, origin } => NetUnitState::Patrolling {
            target: [target.x, target.y, target.z],
            origin: [origin.x, origin.y, origin.z],
        },
        UnitState::AttackMoving(pos) => NetUnitState::AttackMoving {
            target: [pos.x, pos.y, pos.z],
        },
        UnitState::HoldPosition => NetUnitState::HoldPosition,
    }
}

fn stance_to_u8(stance: &UnitStance) -> u8 {
    match stance {
        UnitStance::Passive => 0,
        UnitStance::Defensive => 1,
        UnitStance::Aggressive => 2,
    }
}

/// Timer resource controlling how often the host broadcasts state sync.
#[derive(Resource)]
pub struct StateSyncTimer(pub Timer);

impl Default for StateSyncTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(0.1, TimerMode::Repeating))
    }
}

/// Tracks previous snapshots for delta compression.
#[derive(Resource, Default)]
pub struct PreviousSnapshots {
    pub snapshots: HashMap<u32, EntitySnapshot>,
    pub full_sync_counter: u32,
}

/// Periodically broadcasts authoritative entity positions from host to all clients.
pub fn host_broadcast_state_sync(
    host: Res<HostNetState>,
    time: Res<Time>,
    mut sync_timer: ResMut<StateSyncTimer>,
    net_map: Res<EntityNetMap>,
    mut prev_snapshots: ResMut<PreviousSnapshots>,
    entities: Query<(
        &NetworkId,
        &GlobalTransform,
        Option<&Health>,
        Option<&UnitState>,
        Option<&MoveTarget>,
        Option<&AttackTarget>,
        Option<&Carrying>,
        Option<&UnitStance>,
    ), With<crate::blueprints::EntityKind>>,
    mut event_log: ResMut<GameEventLog>,
    mut net_stats: ResMut<NetStats>,
) {
    sync_timer.0.tick(time.delta());
    if !sync_timer.0.just_finished() {
        return;
    }

    let senders = host.client_senders.lock().unwrap();
    net_stats.connected_clients = senders.len() as u8;
    net_stats.net_map_size = net_map.to_ecs.len() as u32;
    if senders.is_empty() {
        return;
    }

    prev_snapshots.full_sync_counter += 1;
    let is_full_sync = prev_snapshots.full_sync_counter % 50 == 0;

    let mut new_prev = HashMap::new();
    let mut snapshots: Vec<EntitySnapshot> = Vec::new();

    for (net_id, gt, opt_health, opt_state, opt_move, opt_attack, opt_carry, opt_stance) in &entities {
        let pos = gt.translation();
        let (_, rot, _) = gt.to_scale_rotation_translation();
        let snap = EntitySnapshot {
            net_id: net_id.0,
            pos: [pos.x, pos.y, pos.z],
            rot_y: rot.to_euler(bevy::math::EulerRot::YXZ).0,
            health: opt_health.map(|h| h.current),
            unit_state: opt_state.map(|s| ecs_to_net_unit_state(s, &net_map)),
            move_target: opt_move.map(|m| [m.0.x, m.0.y, m.0.z]),
            attack_target: opt_attack.and_then(|a| net_map.to_net.get(&a.0).copied()),
            carrying: opt_carry.and_then(|c| {
                c.resource_type.map(|rt| NetCarrying {
                    resource_type: rt.index() as u8,
                    amount: c.amount,
                })
            }),
            stance: opt_stance.map(|s| stance_to_u8(s)),
        };

        // Delta compression: skip unchanged entities (unless full sync)
        if !is_full_sync {
            if let Some(prev) = prev_snapshots.snapshots.get(&net_id.0) {
                let pos_changed = (snap.pos[0] - prev.pos[0]).abs() > 0.05
                    || (snap.pos[1] - prev.pos[1]).abs() > 0.05
                    || (snap.pos[2] - prev.pos[2]).abs() > 0.05;
                let rot_changed = (snap.rot_y - prev.rot_y).abs() > 0.02;
                let health_changed = snap.health != prev.health;
                let state_changed = snap.unit_state != prev.unit_state
                    || snap.move_target != prev.move_target
                    || snap.attack_target != prev.attack_target
                    || snap.carrying != prev.carrying
                    || snap.stance != prev.stance;

                if !pos_changed && !rot_changed && !health_changed && !state_changed {
                    new_prev.insert(net_id.0, snap);
                    continue;
                }
            }
        }

        new_prev.insert(net_id.0, snap.clone());
        snapshots.push(snap);
    }

    prev_snapshots.snapshots = new_prev;

    net_stats.last_sync_entity_count = snapshots.len() as u32;

    if snapshots.is_empty() {
        return;
    }

    let seq = {
        let mut s = host.seq.lock().unwrap();
        *s += 1;
        *s
    };

    // Log once every ~5 seconds (every 50th sync)
    if seq % 50 == 1 {
        let msg_text = format!(
            "Sync: {} entities → {} client(s) (seq={})",
            snapshots.len(),
            senders.len(),
            seq,
        );
        info!("{}", msg_text);
        event_log.push_with_level(
            time.elapsed_secs(),
            msg_text,
            EventCategory::Sync,
            LogLevel::Info,
            None,
            None,
        );
    }

    let msg = ServerMessage::StateSync {
        seq,
        entities: snapshots,
    };
    if let Ok(json) = serde_json::to_vec(&msg) {
        for (_id, sender) in senders.iter() {
            let _ = sender.send(json.clone());
        }
    }
}

// ── Building sync: host → clients (500ms) ──────────────────────────────────

#[derive(Resource)]
pub struct BuildingSyncTimer(pub Timer);

impl Default for BuildingSyncTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(0.5, TimerMode::Repeating))
    }
}

/// Broadcasts building state (construction, training, production) at lower frequency.
pub fn host_broadcast_building_sync(
    host: Res<HostNetState>,
    time: Res<Time>,
    mut timer: ResMut<BuildingSyncTimer>,
    buildings: Query<(
        &NetworkId,
        Option<&BuildingLevel>,
        Option<&ConstructionProgress>,
        Option<&TrainingQueue>,
        Option<&ProductionState>,
    ), With<crate::blueprints::EntityKind>>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    let senders = host.client_senders.lock().unwrap();
    if senders.is_empty() {
        return;
    }

    let mut building_snaps: Vec<BuildingSnapshot> = Vec::new();

    for (net_id, opt_level, opt_construction, opt_training, opt_production) in &buildings {
        let has_construction = opt_construction.is_some();
        let has_training = opt_training.is_some_and(|tq| !tq.queue.is_empty());
        let has_production = opt_production.is_some_and(|ps| ps.active_recipe.is_some());
        let has_level = opt_level.is_some();

        if !has_construction && !has_training && !has_production && !has_level {
            continue;
        }

        building_snaps.push(BuildingSnapshot {
            net_id: net_id.0,
            level: opt_level.map(|l| l.0),
            construction_progress: opt_construction.map(|cp| cp.timer.fraction()),
            training_queue: opt_training.and_then(|tq| {
                if tq.queue.is_empty() {
                    None
                } else {
                    Some(tq.queue.iter().map(|k| format!("{:?}", k)).collect())
                }
            }),
            training_progress: opt_training.and_then(|tq| {
                tq.timer.as_ref().map(|t| t.fraction())
            }),
            active_recipe: opt_production.and_then(|ps| ps.active_recipe.map(|r| r as u8)),
            production_progress: opt_production.map(|ps| ps.progress_timer.fraction()),
        });
    }

    if building_snaps.is_empty() {
        return;
    }

    let seq = {
        let mut s = host.seq.lock().unwrap();
        *s += 1;
        *s
    };

    let msg = ServerMessage::BuildingSync {
        seq,
        buildings: building_snaps,
    };
    if let Ok(json) = serde_json::to_vec(&msg) {
        for (_id, sender) in senders.iter() {
            let _ = sender.send(json.clone());
        }
    }
}

// ── Resource sync: host → clients (1s) ─────────────────────────────────────

#[derive(Resource)]
pub struct ResourceSyncTimer(pub Timer);

impl Default for ResourceSyncTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(1.0, TimerMode::Repeating))
    }
}

/// Broadcasts all player resources to clients at ~1Hz.
pub fn host_broadcast_resource_sync(
    host: Res<HostNetState>,
    time: Res<Time>,
    mut timer: ResMut<ResourceSyncTimer>,
    all_resources: Res<AllPlayerResources>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    let senders = host.client_senders.lock().unwrap();
    if senders.is_empty() {
        return;
    }

    let factions: Vec<(String, [u32; 10])> = all_resources
        .resources
        .iter()
        .map(|(faction, pr)| (format!("{:?}", faction), pr.amounts))
        .collect();

    let seq = {
        let mut s = host.seq.lock().unwrap();
        *s += 1;
        *s
    };

    let msg = ServerMessage::ResourceSync { seq, factions };
    if let Ok(json) = serde_json::to_vec(&msg) {
        for (_id, sender) in senders.iter() {
            let _ = sender.send(json.clone());
        }
    }
}

// ── Day-cycle sync: host → clients (4Hz) ───────────────────────────────────

#[derive(Resource)]
pub struct DayCycleSyncTimer(pub Timer);

impl Default for DayCycleSyncTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(0.25, TimerMode::Repeating))
    }
}

pub fn host_broadcast_day_cycle_sync(
    host: Res<HostNetState>,
    time: Res<Time>,
    mut timer: ResMut<DayCycleSyncTimer>,
    cycle: Res<DayCycle>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    let senders = host.client_senders.lock().unwrap();
    if senders.is_empty() {
        return;
    }

    let seq = {
        let mut s = host.seq.lock().unwrap();
        *s += 1;
        *s
    };

    let msg = ServerMessage::DayCycleSync {
        seq,
        cycle: DayCycleSnapshot {
            time: cycle.time,
            cycle_duration: cycle.cycle_duration,
            paused: cycle.paused,
        },
    };
    if let Ok(json) = serde_json::to_vec(&msg) {
        for (_id, sender) in senders.iter() {
            let _ = sender.send(json.clone());
        }
    }
}

// ── Entity spawn/despawn replication: host → clients ────────────────────────

/// Tracks which network-IDs the host has already broadcast as spawns.
#[derive(Resource, Default)]
pub struct SyncedEntitySet {
    pub known: HashSet<u32>,
    /// Counter for periodic full re-broadcasts (every ~5s).
    pub full_resync_counter: u32,
}

/// Detects newly spawned entities (those with NetworkId but not yet in SyncedEntitySet)
/// and broadcasts `EntitySpawn` messages so clients can replicate them.
/// Also detects entities that disappeared and broadcasts `EntityDespawn`.
/// Every ~5 seconds, re-broadcasts ALL entities to ensure clients catch up.
pub fn host_broadcast_entity_spawns(
    host: Res<HostNetState>,
    sync_timer: Res<StateSyncTimer>,
    mut synced: ResMut<SyncedEntitySet>,
    entities: Query<
        (
            &NetworkId,
            &crate::blueprints::EntityKind,
            &crate::components::Faction,
            &GlobalTransform,
        ),
    >,
) {
    // Piggyback on the same timer as state sync
    if !sync_timer.0.just_finished() {
        return;
    }

    let senders = host.client_senders.lock().unwrap();
    if senders.is_empty() {
        return;
    }

    synced.full_resync_counter += 1;
    // Every 50 ticks (~5s), do a full re-broadcast of ALL entities
    let is_full_resync = synced.full_resync_counter % 50 == 0;

    // Collect current entity set
    let mut current_ids = HashSet::new();
    let mut new_spawns: Vec<EntitySpawnData> = Vec::new();

    for (net_id, kind, faction, gt) in &entities {
        current_ids.insert(net_id.0);
        if is_full_resync || !synced.known.contains(&net_id.0) {
            let pos = gt.translation();
            let (_, rot, _) = gt.to_scale_rotation_translation();
            new_spawns.push(EntitySpawnData {
                net_id: net_id.0,
                kind: format!("{:?}", kind),
                faction: format!("{:?}", faction),
                pos: [pos.x, pos.y, pos.z],
                rot_y: rot.to_euler(bevy::math::EulerRot::YXZ).0,
            });
        }
    }

    // Detect despawned entities
    let despawned: Vec<u32> = synced
        .known
        .iter()
        .copied()
        .filter(|id| !current_ids.contains(id))
        .collect();

    // Send spawns
    if !new_spawns.is_empty() {
        let seq = {
            let mut s = host.seq.lock().unwrap();
            *s += 1;
            *s
        };
        if is_full_resync {
            info!(
                "EntitySpawn: full resync — {} entities to {} client(s)",
                new_spawns.len(),
                senders.len(),
            );
        } else {
            info!(
                "EntitySpawn: broadcasting {} new entities to {} client(s)",
                new_spawns.len(),
                senders.len(),
            );
        }
        let msg = ServerMessage::EntitySpawn {
            seq,
            spawns: new_spawns,
        };
        if let Ok(json) = serde_json::to_vec(&msg) {
            for (_id, sender) in senders.iter() {
                let _ = sender.send(json.clone());
            }
        }
    }

    // Send despawns
    if !despawned.is_empty() {
        let seq = {
            let mut s = host.seq.lock().unwrap();
            *s += 1;
            *s
        };
        info!(
            "EntityDespawn: broadcasting {} removed entities",
            despawned.len(),
        );
        let msg = ServerMessage::EntityDespawn {
            seq,
            net_ids: despawned.clone(),
        };
        if let Ok(json) = serde_json::to_vec(&msg) {
            for (_id, sender) in senders.iter() {
                let _ = sender.send(json.clone());
            }
        }
    }

    // Update known set
    for id in &despawned {
        synced.known.remove(id);
    }
    synced.known.extend(current_ids);
}
