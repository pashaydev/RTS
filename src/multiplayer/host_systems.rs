//! Host-side systems: relay client commands, handle disconnects.

use bevy::prelude::*;
use std::sync::mpsc::TryRecvError;

use game_state::message::{ClientMessage, GameEvent, InputCommand, PlayerInput, ServerMessage};

use crate::components::*;
use crate::net_bridge::EntityNetMap;

use super::HostNetState;

// ── Shared command execution ────────────────────────────────────────────────

/// Execute a player input command on the ECS. Used by both host and client.
pub fn execute_input_command(
    input: &PlayerInput,
    net_map: &EntityNetMap,
    unit_states: &mut Query<&mut UnitState>,
) {
    for cmd in &input.commands {
        match cmd {
            InputCommand::Move { target } => {
                for &eid in &input.entity_ids {
                    if let Some(&ecs_entity) = net_map.to_ecs.get(&eid) {
                        if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
                            *state = UnitState::Moving(bevy::math::Vec3::new(
                                target[0], target[1], target[2],
                            ));
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
                        }
                    }
                }
            }
            InputCommand::Gather { target_id } => {
                if let Some(&target_ecs) = net_map.to_ecs.get(target_id) {
                    for &eid in &input.entity_ids {
                        if let Some(&ecs_entity) = net_map.to_ecs.get(&eid) {
                            if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
                                *state = UnitState::Gathering(target_ecs);
                            }
                        }
                    }
                }
            }
            InputCommand::Patrol { target } => {
                for &eid in &input.entity_ids {
                    if let Some(&ecs_entity) = net_map.to_ecs.get(&eid) {
                        if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
                            let pos = bevy::math::Vec3::new(target[0], target[1], target[2]);
                            *state = UnitState::Patrolling {
                                target: pos,
                                origin: pos,
                            };
                        }
                    }
                }
            }
            InputCommand::AttackMove { target } => {
                for &eid in &input.entity_ids {
                    if let Some(&ecs_entity) = net_map.to_ecs.get(&eid) {
                        if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
                            *state = UnitState::AttackMoving(bevy::math::Vec3::new(
                                target[0], target[1], target[2],
                            ));
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
    host: Res<HostNetState>,
    net_map: Res<EntityNetMap>,
    mut unit_states: Query<&mut UnitState>,
    time: Res<Time>,
) {
    let rx = host.incoming_commands.lock().unwrap();
    for _ in 0..64 {
        match rx.try_recv() {
            Ok((player_id, msg)) => {
                match &msg {
                    ClientMessage::Input { input, .. } => {
                        // Execute on host ECS
                        execute_input_command(input, &net_map, &mut unit_states);

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
                            input: input.clone(),
                        };
                        if let Ok(json) = serde_json::to_vec(&relay) {
                            let senders = host.client_senders.lock().unwrap();
                            for (id, sender) in senders.iter() {
                                if *id != player_id {
                                    let _ = sender.send(json.clone());
                                }
                            }
                        }
                    }
                    ClientMessage::JoinRequest { player_name, .. } => {
                        info!("Player {} joined: {}", player_id, player_name);
                    }
                    ClientMessage::LeaveNotice { .. } => {
                        info!("Player {} left gracefully", player_id);
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
) {
    let dc_rx = host.disconnect_rx.lock().unwrap();
    let mut senders = host.client_senders.lock().unwrap();
    let seq = *host.seq.lock().unwrap();

    loop {
        match dc_rx.try_recv() {
            Ok(player_id) => {
                info!("Player {} disconnected", player_id);

                if let Some(player) = lobby
                    .players
                    .iter_mut()
                    .find(|p| !p.is_host && p.connected)
                {
                    player.connected = false;
                    ai_factions.factions.insert(player.faction);
                }

                senders.retain(|(id, _)| *id != player_id);

                let announce = ServerMessage::Event {
                    seq,
                    timestamp: 0.0,
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
