//! Host-side systems: relay client commands, handle disconnects.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::TryRecvError;

use game_state::codec;
use game_state::message::{
    BuildingSnapshot, ClientMessage, DayCycleSnapshot, EntitySnapshot, EntitySpawnData, GameEvent,
    InputCommand, NetCarrying, NetUnitState, NeutralKind, NeutralWorldSnapshot, PlayerInput,
    ServerFrame, ServerMessage,
};

use crate::components::*;
use crate::lighting::DayCycle;
use crate::net_bridge::{EntityNetMap, NetworkId};
use crate::orders;
use crate::ui::event_log_widget::{EventCategory, GameEventLog, LogLevel};

use super::debug_tap;
use super::{HostNetState, NetStats};

// ── Pending frame buffer for message batching ───────────────────────────────

/// Accumulates server messages during a tick, flushed as a single ServerFrame.
#[derive(Resource, Default)]
pub struct PendingServerFrame {
    pub messages: Vec<ServerMessage>,
}

/// Serialize a ServerFrame and broadcast to all connected clients.
fn broadcast_frame(host: &HostNetState, frame: &ServerFrame) {
    let senders = host.client_senders.lock().unwrap();
    if senders.is_empty() {
        return;
    }
    let Ok(bytes) = codec::encode(frame) else {
        bevy::log::error!("Failed to encode ServerFrame");
        return;
    };
    for (_id, sender) in senders.iter() {
        let _ = sender.send(bytes.clone());
    }
}

/// Serialize a single ServerMessage and broadcast to all connected clients.
fn broadcast_msg(host: &HostNetState, msg: &ServerMessage) {
    let senders = host.client_senders.lock().unwrap();
    if senders.is_empty() {
        return;
    }
    let Ok(bytes) = codec::encode(msg) else {
        bevy::log::error!("Failed to encode ServerMessage");
        return;
    };
    for (_id, sender) in senders.iter() {
        let _ = sender.send(bytes.clone());
    }
}

/// Serialize a ServerMessage and send to a specific client.
fn send_to_player(host: &HostNetState, player_id: u8, msg: &ServerMessage) {
    let senders = host.client_senders.lock().unwrap();
    let Ok(bytes) = codec::encode(msg) else { return };
    for (id, sender) in senders.iter() {
        if *id == player_id {
            let _ = sender.send(bytes.clone());
        }
    }
}

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
                        broadcast_msg(&host, &relay);
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
                        send_to_player(&host, player_id, &pong);
                    }
                    ClientMessage::Reconnect { session_token, .. } => {
                        info!("Reconnect request from player {} with token {}", player_id, session_token);
                        debug_tap::record_info(
                            "host_commands",
                            format!("player {} reconnect request token={}", player_id, session_token),
                        );
                        // Reconnection is handled in the lobby system (menu/multiplayer.rs)
                        // Here we just log it — the actual reconnection logic requires lobby access
                    }
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }
}

/// Detect disconnected clients — start grace period for reconnection.
/// After RECONNECT_GRACE_PERIOD seconds, convert their factions to AI.
pub fn host_handle_disconnects(
    host: Res<HostNetState>,
    mut lobby: ResMut<super::LobbyState>,
    mut ai_factions: ResMut<AiControlledFactions>,
    mut session_tokens: ResMut<super::SessionTokens>,
    time: Res<Time>,
    mut event_log: ResMut<GameEventLog>,
) {
    let dc_rx = host.disconnect_rx.lock().unwrap();
    let mut senders = host.client_senders.lock().unwrap();

    // Process new disconnections — enter grace period
    loop {
        match dc_rx.try_recv() {
            Ok(player_id) => {
                info!("Player {} disconnected — starting {}s reconnection grace period",
                    player_id, super::RECONNECT_GRACE_PERIOD);
                debug_tap::record_info("host_disconnects", format!("player {} disconnected", player_id));

                let player_info = lobby.players.iter().find(|p| p.player_id == player_id);
                let player_name = player_info.map(|p| p.name.clone())
                    .unwrap_or_else(|| format!("Player {}", player_id));

                if let Some(player) = lobby
                    .players
                    .iter_mut()
                    .find(|p| p.player_id == player_id && p.connected)
                {
                    player.connected = false;

                    // Find existing session token for this player
                    let token = session_tokens.tokens.iter()
                        .find(|(_, &pid)| pid == player_id)
                        .map(|(&t, _)| t)
                        .unwrap_or_else(|| session_tokens.generate(player_id));

                    // Add to grace period list instead of immediately converting to AI
                    session_tokens.disconnected.push(super::DisconnectedPlayer {
                        session_token: token,
                        player_id,
                        faction: player.faction,
                        seat_index: player.seat_index,
                        color_index: player.color_index,
                        name: player_name.clone(),
                        disconnect_time: time.elapsed_secs(),
                    });
                }

                event_log.push_with_level(
                    time.elapsed_secs(),
                    format!("{} disconnected — waiting for reconnection ({}s)", player_name, super::RECONNECT_GRACE_PERIOD as u32),
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
                        text: format!("{} disconnected — waiting for reconnection", player_name),
                    }],
                };
                if let Ok(bytes) = codec::encode(&announce) {
                    for (_id, sender) in senders.iter() {
                        let _ = sender.send(bytes.clone());
                    }
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }

    // Check grace period expiry — convert to AI after timeout
    let now = time.elapsed_secs();
    let expired: Vec<super::DisconnectedPlayer> = session_tokens.disconnected
        .extract_if(.., |dc| now - dc.disconnect_time >= super::RECONNECT_GRACE_PERIOD)
        .collect();

    for dc in expired {
        info!("Reconnection grace period expired for {} — converting to AI", dc.name);
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
        if let Ok(bytes) = codec::encode(&announce) {
            for (_id, sender) in senders.iter() {
                let _ = sender.send(bytes.clone());
            }
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
    if let Ok(bytes) = codec::encode(&msg) {
        for (_id, sender) in senders.iter() {
            let _ = sender.send(bytes.clone());
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

/// Tracks previous building snapshots for delta compression.
#[derive(Resource, Default)]
pub struct PreviousBuildingSnapshots {
    pub snapshots: HashMap<u32, BuildingSnapshot>,
}

/// Broadcasts building state (construction, training, production) at lower frequency.
/// Now delta-compressed: only sends buildings whose state changed since last broadcast.
pub fn host_broadcast_building_sync(
    host: Res<HostNetState>,
    time: Res<Time>,
    mut timer: ResMut<BuildingSyncTimer>,
    mut prev_buildings: ResMut<PreviousBuildingSnapshots>,
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
    let mut new_prev = HashMap::new();

    for (net_id, opt_level, opt_construction, opt_training, opt_production) in &buildings {
        let has_construction = opt_construction.is_some();
        let has_training = opt_training.is_some_and(|tq| !tq.queue.is_empty());
        let has_production = opt_production.is_some_and(|ps| ps.active_recipe.is_some());
        let has_level = opt_level.is_some();

        if !has_construction && !has_training && !has_production && !has_level {
            continue;
        }

        let snap = BuildingSnapshot {
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
        };

        // Delta compression: skip unchanged buildings
        let changed = if let Some(prev) = prev_buildings.snapshots.get(&net_id.0) {
            snap.level != prev.level
                || snap.training_queue != prev.training_queue
                || snap.active_recipe != prev.active_recipe
                || snap.construction_progress.map(|p| (p * 100.0) as u32)
                    != prev.construction_progress.map(|p| (p * 100.0) as u32)
                || snap.training_progress.map(|p| (p * 100.0) as u32)
                    != prev.training_progress.map(|p| (p * 100.0) as u32)
                || snap.production_progress.map(|p| (p * 100.0) as u32)
                    != prev.production_progress.map(|p| (p * 100.0) as u32)
        } else {
            true // New building, always send
        };

        new_prev.insert(net_id.0, snap.clone());
        if changed {
            building_snaps.push(snap);
        }
    }

    prev_buildings.snapshots = new_prev;

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
    if let Ok(bytes) = codec::encode(&msg) {
        for (_id, sender) in senders.iter() {
            let _ = sender.send(bytes.clone());
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

    let factions: Vec<(u8, [u32; 10])> = all_resources
        .resources
        .iter()
        .map(|(faction, pr)| (faction.to_net_index(), pr.amounts))
        .collect();

    let seq = {
        let mut s = host.seq.lock().unwrap();
        *s += 1;
        *s
    };

    let msg = ServerMessage::ResourceSync { seq, factions };
    if let Ok(bytes) = codec::encode(&msg) {
        for (_id, sender) in senders.iter() {
            let _ = sender.send(bytes.clone());
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
    if let Ok(bytes) = codec::encode(&msg) {
        for (_id, sender) in senders.iter() {
            let _ = sender.send(bytes.clone());
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
                kind: kind.to_index(),
                faction: faction.to_net_index(),
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
        if let Ok(bytes) = codec::encode(&msg) {
            for (_id, sender) in senders.iter() {
                let _ = sender.send(bytes.clone());
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
        if let Ok(bytes) = codec::encode(&msg) {
            for (_id, sender) in senders.iter() {
                let _ = sender.send(bytes.clone());
            }
        }
    }

    // Update known set
    for id in &despawned {
        synced.known.remove(id);
    }
    synced.known.extend(current_ids);
}

// ── Neutral world sync: host → clients (2Hz) ───────────────────────────────

#[derive(Resource)]
pub struct NeutralWorldSyncTimer(pub Timer);

impl Default for NeutralWorldSyncTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(0.5, TimerMode::Repeating))
    }
}

/// Tracks previous resource node amounts for delta compression.
#[derive(Resource, Default)]
pub struct PreviousNeutralSnapshots {
    pub amounts: HashMap<u32, u32>,
}

/// Broadcasts resource node amount changes to clients at ~2Hz.
/// Uses delta compression: only sends nodes whose amount_remaining changed.
pub fn host_broadcast_neutral_world_sync(
    host: Res<HostNetState>,
    time: Res<Time>,
    mut timer: ResMut<NeutralWorldSyncTimer>,
    mut prev_neutral: ResMut<PreviousNeutralSnapshots>,
    resource_nodes: Query<(
        &NetworkId,
        &crate::components::ResourceNode,
        &GlobalTransform,
    )>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    let senders = host.client_senders.lock().unwrap();
    if senders.is_empty() {
        return;
    }

    let mut changed: Vec<NeutralWorldSnapshot> = Vec::new();
    let mut new_prev = HashMap::new();

    for (net_id, node, gt) in &resource_nodes {
        let prev_amount = prev_neutral.amounts.get(&net_id.0).copied();
        new_prev.insert(net_id.0, node.amount_remaining);

        // Delta: skip if unchanged
        if prev_amount == Some(node.amount_remaining) {
            continue;
        }

        let pos = gt.translation();
        changed.push(NeutralWorldSnapshot {
            net_id: net_id.0,
            kind: NeutralKind::ResourceNode,
            pos: [pos.x, pos.y, pos.z],
            rot_y: 0.0,
            scale: 1.0,
            resource_type: Some(node.resource_type.index() as u8),
            amount_remaining: Some(node.amount_remaining),
            stage: None,
            health: None,
            variant: None,
        });
    }

    prev_neutral.amounts = new_prev;

    if changed.is_empty() {
        return;
    }

    let seq = {
        let mut s = host.seq.lock().unwrap();
        *s += 1;
        *s
    };

    let msg = ServerMessage::NeutralWorldDelta {
        seq,
        objects: changed,
    };
    if let Ok(bytes) = codec::encode(&msg) {
        for (_id, sender) in senders.iter() {
            let _ = sender.send(bytes.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    use crate::blueprints::EntityKind;
    use crate::components::{
        AiControlledFactions, Faction, Health, NextTaskId, TaskQueue, Unit, UnitStance,
        UnitState,
    };

    fn host_state_with_clients(client_ids: &[u8]) -> (HostNetState, Vec<mpsc::Receiver<Vec<u8>>>) {
        let (_incoming_tx, incoming_rx) = mpsc::channel();
        let (_new_client_tx, new_client_rx) = mpsc::channel();
        let (_new_ws_tx, new_ws_rx) = mpsc::channel();
        let (_disconnect_tx, disconnect_rx) = mpsc::channel();
        let mut senders = Vec::new();
        let mut receivers = Vec::new();
        for id in client_ids {
            let (tx, rx) = mpsc::channel();
            senders.push((*id, tx));
            receivers.push(rx);
        }
        (
            HostNetState {
                incoming_commands: Mutex::new(incoming_rx),
                client_senders: Mutex::new(senders),
                new_clients: Mutex::new(new_client_rx),
                new_ws_clients: Mutex::new(new_ws_rx),
                disconnect_rx: Mutex::new(disconnect_rx),
                shutdown: Arc::new(AtomicBool::new(false)),
                seq: Mutex::new(0),
            },
            receivers,
        )
    }

    #[test]
    fn ecs_to_net_unit_state_converts_and_falls_back_for_missing_links() {
        let attack_entity = Entity::from_bits(10);
        let depot_entity = Entity::from_bits(11);
        let build_entity = Entity::from_bits(12);
        let mut net_map = EntityNetMap::default();
        net_map.to_net.insert(attack_entity, 50);
        net_map.to_net.insert(depot_entity, 60);
        net_map.to_net.insert(build_entity, 70);

        assert_eq!(
            ecs_to_net_unit_state(&UnitState::Moving(Vec3::new(1.0, 2.0, 3.0)), &net_map),
            NetUnitState::Moving {
                target: [1.0, 2.0, 3.0]
            }
        );
        assert_eq!(
            ecs_to_net_unit_state(&UnitState::Attacking(attack_entity), &net_map),
            NetUnitState::Attacking { target_id: 50 }
        );
        assert_eq!(
            ecs_to_net_unit_state(
                &UnitState::ReturningToDeposit {
                    depot: depot_entity,
                    gather_node: None,
                },
                &net_map
            ),
            NetUnitState::Returning { depot_id: 60 }
        );
        assert_eq!(
            ecs_to_net_unit_state(&UnitState::Building(build_entity), &net_map),
            NetUnitState::Building { target_id: 70 }
        );
        assert_eq!(
            ecs_to_net_unit_state(&UnitState::Attacking(Entity::from_bits(999)), &net_map),
            NetUnitState::Idle
        );
    }

    #[test]
    fn stance_to_u8_matches_wire_encoding() {
        assert_eq!(stance_to_u8(&UnitStance::Passive), 0);
        assert_eq!(stance_to_u8(&UnitStance::Defensive), 1);
        assert_eq!(stance_to_u8(&UnitStance::Aggressive), 2);
    }

    #[test]
    fn host_handle_disconnects_starts_grace_period_and_then_assigns_ai() {
        let (_incoming_tx, incoming_rx) = mpsc::channel();
        let (_new_client_tx, new_client_rx) = mpsc::channel();
        let (_new_ws_tx, new_ws_rx) = mpsc::channel();
        let (disconnect_tx, disconnect_rx) = mpsc::channel();
        let (disconnected_tx, _disconnected_rx) = mpsc::channel();
        let (remaining_tx, remaining_rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));

        let host = HostNetState {
            incoming_commands: Mutex::new(incoming_rx),
            client_senders: Mutex::new(vec![(1, disconnected_tx.clone()), (2, remaining_tx.clone())]),
            new_clients: Mutex::new(new_client_rx),
            new_ws_clients: Mutex::new(new_ws_rx),
            disconnect_rx: Mutex::new(disconnect_rx),
            shutdown,
            seq: Mutex::new(0),
        };

        let mut app = App::new();
        app.insert_resource(host);
        app.insert_resource(super::super::LobbyState {
            players: vec![super::super::LobbyPlayer {
                player_id: 1,
                name: "Guest".to_string(),
                seat_index: 1,
                faction: Faction::Player2,
                color_index: 1,
                is_host: false,
                connected: true,
            }],
            ..Default::default()
        });
        app.insert_resource(AiControlledFactions {
            factions: std::collections::HashSet::new(),
        });
        app.insert_resource(super::super::SessionTokens::default());
        app.insert_resource(GameEventLog::default());
        app.insert_resource(Time::<()>::default());
        app.add_systems(Update, host_handle_disconnects);

        disconnect_tx.send(1).unwrap();
        app.update();

        {
            let lobby = app.world().resource::<super::super::LobbyState>();
            let tokens = app.world().resource::<super::super::SessionTokens>();
            assert!(!lobby.players[0].connected);
            assert_eq!(tokens.disconnected.len(), 1);
            assert_eq!(tokens.disconnected[0].player_id, 1);
            assert!(tokens.tokens.values().any(|pid| *pid == 1));
        }

        let bytes = remaining_rx.try_recv().unwrap();
        let msg: ServerMessage = codec::decode(&bytes).unwrap();
        match msg {
            ServerMessage::Event { events, .. } => {
                assert!(matches!(
                    events.first(),
                    Some(GameEvent::Announcement { text }) if text.contains("waiting for reconnection")
                ));
            }
            other => panic!("expected announcement, got {other:?}"),
        }

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(super::super::RECONNECT_GRACE_PERIOD + 0.1));
        app.update();

        let ai = app.world().resource::<AiControlledFactions>();
        let tokens = app.world().resource::<super::super::SessionTokens>();
        assert!(ai.factions.contains(&Faction::Player2));
        assert!(tokens.disconnected.is_empty());
        assert!(tokens.tokens.is_empty());
    }

    #[test]
    fn host_process_client_commands_filters_to_owned_entities_and_relays_input() {
        let (incoming_tx, incoming_rx) = mpsc::channel();
        let (_new_client_tx, new_client_rx) = mpsc::channel();
        let (_new_ws_tx, new_ws_rx) = mpsc::channel();
        let (_disconnect_tx, disconnect_rx) = mpsc::channel();
        let (client_tx, client_rx) = mpsc::channel();

        let mut app = App::new();
        app.insert_resource(HostNetState {
            incoming_commands: Mutex::new(incoming_rx),
            client_senders: Mutex::new(vec![(1, client_tx)]),
            new_clients: Mutex::new(new_client_rx),
            new_ws_clients: Mutex::new(new_ws_rx),
            disconnect_rx: Mutex::new(disconnect_rx),
            shutdown: Arc::new(AtomicBool::new(false)),
            seq: Mutex::new(0),
        });
        app.insert_resource(super::super::LobbyState {
            players: vec![super::super::LobbyPlayer {
                player_id: 1,
                name: "Guest".to_string(),
                seat_index: 1,
                faction: Faction::Player2,
                color_index: 1,
                is_host: false,
                connected: true,
            }],
            ..Default::default()
        });
        app.insert_resource(EntityNetMap::default());
        app.insert_resource(NextTaskId::default());
        app.insert_resource(GameEventLog::default());
        app.insert_resource(Time::<()>::default());

        let owned = app
            .world_mut()
            .spawn((
                Unit,
                Faction::Player2,
                UnitState::Idle,
                TaskQueue::default(),
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        let foreign = app
            .world_mut()
            .spawn((
                Unit,
                Faction::Player3,
                UnitState::Idle,
                TaskQueue::default(),
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();

        {
            let mut net_map = app.world_mut().resource_mut::<EntityNetMap>();
            net_map.to_ecs.insert(10, owned);
            net_map.to_net.insert(owned, 10);
            net_map.to_ecs.insert(20, foreign);
            net_map.to_net.insert(foreign, 20);
        }

        app.add_systems(Update, host_process_client_commands);

        incoming_tx
            .send((
                1,
                ClientMessage::Input {
                    seq: 1,
                    timestamp: 0.0,
                    input: PlayerInput {
                        player_id: 99,
                        tick: 5,
                        entity_ids: vec![10, 20],
                        commands: vec![InputCommand::HoldPosition],
                    },
                },
            ))
            .unwrap();

        app.update();

        assert_eq!(
            *app.world().entity(owned).get::<UnitState>().unwrap(),
            UnitState::HoldPosition
        );
        assert_eq!(
            *app.world().entity(foreign).get::<UnitState>().unwrap(),
            UnitState::Idle
        );

        let relay_bytes = client_rx.try_recv().unwrap();
        let relay: ServerMessage = codec::decode(&relay_bytes).unwrap();
        match relay {
            ServerMessage::RelayedInput { player_id, input, .. } => {
                assert_eq!(player_id, 1);
                assert_eq!(input.player_id, 1);
                assert_eq!(input.entity_ids, vec![10]);
            }
            other => panic!("expected relayed input, got {other:?}"),
        }
    }

    #[test]
    fn host_broadcast_state_sync_sends_delta_then_suppresses_unchanged_entities() {
        let (host, receivers) = host_state_with_clients(&[1]);

        let mut app = App::new();
        app.insert_resource(host);
        app.insert_resource(StateSyncTimer(Timer::from_seconds(0.1, TimerMode::Repeating)));
        app.insert_resource(PreviousSnapshots::default());
        app.insert_resource(EntityNetMap::default());
        app.insert_resource(GameEventLog::default());
        app.insert_resource(NetStats::default());
        app.insert_resource(Time::<()>::default());
        app.add_systems(Update, host_broadcast_state_sync);

        let target = app.world_mut().spawn_empty().id();
        let unit = app
            .world_mut()
            .spawn((
                NetworkId(7),
                EntityKind::Worker,
                Transform::from_xyz(1.0, 0.0, 2.0),
                GlobalTransform::from(Transform::from_xyz(1.0, 0.0, 2.0)),
                Health {
                    current: 15.0,
                    max: 20.0,
                },
                UnitState::Attacking(target),
                MoveTarget(Vec3::new(4.0, 0.0, 5.0)),
                AttackTarget(target),
                Carrying {
                    amount: 3,
                    weight: 1.0,
                    resource_type: Some(ResourceType::Wood),
                },
                UnitStance::Aggressive,
            ))
            .id();
        {
            let mut net_map = app.world_mut().resource_mut::<EntityNetMap>();
            net_map.to_net.insert(target, 99);
            net_map.to_ecs.insert(99, target);
            net_map.to_net.insert(unit, 7);
            net_map.to_ecs.insert(7, unit);
        }

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(0.1));
        app.update();

        let first_bytes = receivers[0].try_recv().unwrap();
        let first_msg: ServerMessage = codec::decode(&first_bytes).unwrap();
        match first_msg {
            ServerMessage::StateSync { entities, .. } => {
                assert_eq!(entities.len(), 1);
                assert_eq!(entities[0].net_id, 7);
                assert_eq!(entities[0].attack_target, Some(99));
                assert_eq!(entities[0].stance, Some(2));
            }
            other => panic!("expected state sync, got {other:?}"),
        }

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(0.1));
        app.update();
        assert!(receivers[0].try_recv().is_err());
        assert_eq!(app.world().resource::<NetStats>().connected_clients, 1);
        assert_eq!(app.world().resource::<NetStats>().last_sync_entity_count, 0);
    }

    #[test]
    fn host_broadcast_entity_spawns_sends_spawns_and_despawns() {
        let (host, receivers) = host_state_with_clients(&[1]);

        let mut app = App::new();
        app.insert_resource(host);
        let mut timer = StateSyncTimer::default();
        timer.0.tick(Duration::from_secs_f32(0.1));
        app.insert_resource(timer);
        app.insert_resource(SyncedEntitySet::default());
        app.add_systems(Update, host_broadcast_entity_spawns);

        let entity = app
            .world_mut()
            .spawn((
                NetworkId(55),
                EntityKind::Base,
                Faction::Player1,
                Transform::from_xyz(3.0, 0.0, 4.0),
                GlobalTransform::from(Transform::from_xyz(3.0, 0.0, 4.0)),
            ))
            .id();

        app.update();

        let spawn_bytes = receivers[0].try_recv().unwrap();
        let spawn_msg: ServerMessage = codec::decode(&spawn_bytes).unwrap();
        match spawn_msg {
            ServerMessage::EntitySpawn { spawns, .. } => {
                assert_eq!(spawns.len(), 1);
                assert_eq!(spawns[0].net_id, 55);
            }
            other => panic!("expected entity spawn, got {other:?}"),
        }

        app.world_mut().entity_mut(entity).despawn();
        let mut timer = app.world_mut().resource_mut::<StateSyncTimer>();
        timer.0.tick(Duration::from_secs_f32(0.1));
        drop(timer);
        app.update();

        let despawn_bytes = receivers[0].try_recv().unwrap();
        let despawn_msg: ServerMessage = codec::decode(&despawn_bytes).unwrap();
        match despawn_msg {
            ServerMessage::EntityDespawn { net_ids, .. } => assert_eq!(net_ids, vec![55]),
            other => panic!("expected entity despawn, got {other:?}"),
        }
    }
}
