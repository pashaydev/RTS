//! Host-side systems: relay client commands, handle disconnects.

use bevy::prelude::*;
use std::sync::mpsc::TryRecvError;

use std::collections::HashSet;

use game_state::message::{
    ClientMessage, EntitySnapshot, EntitySpawnData, GameEvent, InputCommand, PlayerInput,
    ServerMessage,
};

use crate::components::*;
use crate::net_bridge::{EntityNetMap, NetworkId};
use crate::orders;

use super::debug_tap;
use super::HostNetState;

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
                    }
                    ClientMessage::LeaveNotice { .. } => {
                        info!("Player {} left gracefully", player_id);
                        debug_tap::record_info(
                            "host_commands",
                            format!("player {} leave notice", player_id),
                        );
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
) {
    let dc_rx = host.disconnect_rx.lock().unwrap();
    let mut senders = host.client_senders.lock().unwrap();

    loop {
        match dc_rx.try_recv() {
            Ok(player_id) => {
                info!("Player {} disconnected", player_id);
                debug_tap::record_info("host_disconnects", format!("player {} disconnected", player_id));

                if let Some(player) = lobby
                    .players
                    .iter_mut()
                    .find(|p| p.player_id == player_id && p.connected)
                {
                    player.connected = false;
                    ai_factions.factions.insert(player.faction);
                }

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

/// Timer resource controlling how often the host broadcasts state sync.
#[derive(Resource)]
pub struct StateSyncTimer(pub Timer);

impl Default for StateSyncTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(0.1, TimerMode::Repeating))
    }
}

/// Periodically broadcasts authoritative entity positions from host to all clients.
pub fn host_broadcast_state_sync(
    host: Res<HostNetState>,
    time: Res<Time>,
    mut sync_timer: ResMut<StateSyncTimer>,
    entities: Query<(&NetworkId, &GlobalTransform, Option<&crate::components::Health>), With<crate::blueprints::EntityKind>>,
) {
    sync_timer.0.tick(time.delta());
    if !sync_timer.0.just_finished() {
        return;
    }

    let senders = host.client_senders.lock().unwrap();
    if senders.is_empty() {
        return;
    }

    let snapshots: Vec<EntitySnapshot> = entities
        .iter()
        .map(|(net_id, gt, opt_health)| {
            let pos = gt.translation();
            let (_, rot, _) = gt.to_scale_rotation_translation();
            EntitySnapshot {
                net_id: net_id.0,
                pos: [pos.x, pos.y, pos.z],
                rot_y: rot.to_euler(bevy::math::EulerRot::YXZ).0,
                health: opt_health.map(|h| h.current),
            }
        })
        .collect();

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
        info!(
            "StateSync: broadcasting {} entities to {} client(s) (seq={})",
            snapshots.len(),
            senders.len(),
            seq,
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

// ── Entity spawn/despawn replication: host → clients ────────────────────────

/// Tracks which network-IDs the host has already broadcast as spawns.
#[derive(Resource, Default)]
pub struct SyncedEntitySet {
    pub known: HashSet<u32>,
}

/// Detects newly spawned entities (those with NetworkId but not yet in SyncedEntitySet)
/// and broadcasts `EntitySpawn` messages so clients can replicate them.
/// Also detects entities that disappeared and broadcasts `EntityDespawn`.
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

    // Collect current entity set
    let mut current_ids = HashSet::new();
    let mut new_spawns: Vec<EntitySpawnData> = Vec::new();

    for (net_id, kind, faction, gt) in &entities {
        current_ids.insert(net_id.0);
        if !synced.known.contains(&net_id.0) {
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
        info!(
            "EntitySpawn: broadcasting {} new entities to {} client(s)",
            new_spawns.len(),
            senders.len(),
        );
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
