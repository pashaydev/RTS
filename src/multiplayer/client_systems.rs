//! Client-side systems: receive relayed commands from host, handle disconnect.

use bevy::prelude::*;
use std::sync::atomic::Ordering;
use std::sync::mpsc::TryRecvError;

use game_state::message::{EntitySpawnData, ServerMessage};

use crate::blueprints::{spawn_from_blueprint_with_faction, BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::components::*;
use crate::ground::HeightMap;
use crate::model_assets::{BuildingModelAssets, UnitModelAssets};
use crate::net_bridge::{EntityNetMap, NetworkId};

use super::debug_tap;
use super::{ClientNetState, NetRole};
use super::host_systems::execute_input_command;

/// Timer for sending periodic pings to the host (keeps VPN/Hamachi tunnels alive).
#[derive(Resource)]
pub struct ClientPingTimer(pub Timer);

impl Default for ClientPingTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(5.0, TimerMode::Repeating))
    }
}

/// Pending spawn/despawn events queued by the network receiver,
/// processed by a separate system that has access to blueprint resources.
#[derive(Resource, Default)]
pub struct PendingNetSpawns {
    pub spawns: Vec<EntitySpawnData>,
    pub despawns: Vec<u32>,
}

/// Returns true if the given ECS entity belongs to the active player's faction.
fn is_local_entity(
    entity: Entity,
    factions: &Query<&Faction>,
    active_player: &ActivePlayer,
) -> bool {
    factions.get(entity).map_or(false, |f| *f == active_player.0)
}

/// Polls incoming `ServerMessage`s from the host and applies relayed commands
/// and state sync snapshots.
pub fn client_receive_commands(
    mut commands: Commands,
    client: Res<ClientNetState>,
    net_map: Res<EntityNetMap>,
    mut unit_states: Query<&mut UnitState>,
    mut task_queues: Query<&mut TaskQueue, With<Unit>>,
    mut next_task_id: ResMut<NextTaskId>,
    read_transforms: Query<&GlobalTransform>,
    mut write_transforms: Query<&mut Transform>,
    mut healths: Query<&mut Health>,
    factions: Query<&Faction>,
    active_player: Res<ActivePlayer>,
    mut pending_spawns: ResMut<PendingNetSpawns>,
) {
    let rx = client.incoming.lock().unwrap();
    // On WASM (single-threaded), limit per-frame work to avoid blocking the event loop.
    #[cfg(target_arch = "wasm32")]
    const MAX_PER_FRAME: usize = 32;
    #[cfg(not(target_arch = "wasm32"))]
    const MAX_PER_FRAME: usize = 256;
    for _ in 0..MAX_PER_FRAME {
        match rx.try_recv() {
            Ok(msg) => match &msg {
                ServerMessage::RelayedInput { input, .. } => {
                    execute_input_command(
                        &mut commands,
                        input,
                        &net_map,
                        &mut unit_states,
                        &mut task_queues,
                        &mut next_task_id,
                        &read_transforms,
                    );
                }
                ServerMessage::StateSync { seq, entities } => {
                    let mut matched = 0u32;
                    let total = entities.len();
                    for snap in entities {
                        if let Some(&ecs_entity) = net_map.to_ecs.get(&snap.net_id) {
                            // Skip the active player's own units — they are moved locally
                            if is_local_entity(ecs_entity, &factions, &active_player) {
                                continue;
                            }
                            if let Ok(mut transform) = write_transforms.get_mut(ecs_entity) {
                                transform.translation.x = snap.pos[0];
                                transform.translation.y = snap.pos[1];
                                transform.translation.z = snap.pos[2];
                                transform.rotation =
                                    Quat::from_rotation_y(snap.rot_y);
                                matched += 1;
                            }
                            // Apply health from host for remote entities
                            if let Some(hp) = snap.health {
                                if let Ok(mut health) = healths.get_mut(ecs_entity) {
                                    health.current = hp;
                                }
                            }
                        }
                    }
                    // Log once every ~5 seconds
                    if *seq % 50 == 1 {
                        info!(
                            "StateSync received: {}/{} entities matched (seq={}, net_map size={})",
                            matched, total, seq, net_map.to_ecs.len(),
                        );
                    }
                }
                ServerMessage::EntitySpawn { spawns, .. } => {
                    pending_spawns.spawns.extend(spawns.iter().cloned());
                    debug_tap::record_info(
                        "client_entity_sync",
                        format!("queued {} entity spawns", spawns.len()),
                    );
                }
                ServerMessage::EntityDespawn { net_ids, .. } => {
                    pending_spawns.despawns.extend(net_ids.iter().copied());
                    debug_tap::record_info(
                        "client_entity_sync",
                        format!("queued {} entity despawns", net_ids.len()),
                    );
                }
                ServerMessage::Pong { .. } => {
                    // Keepalive acknowledged — connection is alive
                }
                ServerMessage::Event { events, .. } => {
                    for event in events {
                        match event {
                            game_state::message::GameEvent::Announcement { text } => {
                                info!("Server announcement: {}", text);
                                debug_tap::record_info(
                                    "client_game_events",
                                    format!("announcement: {}", text),
                                );
                            }
                            game_state::message::GameEvent::HostShutdown { reason } => {
                                warn!("Host ended match: {}", reason);
                                debug_tap::record_info(
                                    "client_game_events",
                                    format!("host_shutdown: {}", reason),
                                );
                                client.shutdown.store(true, Ordering::Relaxed);
                            }
                            _ => {}
                        }
                    }
                }
            },
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                client.shutdown.store(true, Ordering::Relaxed);
                debug_tap::record_error("client_receive", "incoming channel disconnected");
                break;
            }
        }
    }
}

/// Processes pending entity spawns/despawns from the host.
/// Runs as a separate system because it needs access to blueprint/visual resources.
pub fn client_apply_entity_sync(
    mut commands: Commands,
    mut pending: ResMut<PendingNetSpawns>,
    net_map: Res<EntityNetMap>,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    height_map: Res<HeightMap>,
    building_models: Option<Res<BuildingModelAssets>>,
    unit_models: Option<Res<UnitModelAssets>>,
    // Query existing NetworkId entities to avoid duplicate spawns
    existing_with_id: Query<(Entity, &NetworkId)>,
    // Query ALL entities with EntityKind+Faction+Transform (may or may not have NetworkId yet)
    all_entities: Query<(Entity, &EntityKind, &Faction, &Transform, Option<&NetworkId>)>,
) {
    // ── Handle spawns (batched: max 8 per frame to avoid WASM stalls) ──
    if !pending.spawns.is_empty() {
        // Build set of already-known net IDs (from local spawns or prior sync)
        let known_ids: std::collections::HashSet<u32> =
            existing_with_id.iter().map(|(_, nid)| nid.0).collect();

        let batch_size = 8;
        let remaining = if pending.spawns.len() > batch_size {
            pending.spawns.split_off(batch_size)
        } else {
            Vec::new()
        };
        let spawns = std::mem::replace(&mut pending.spawns, remaining);
        let mut spawned = 0u32;
        let mut adopted = 0u32;
        for spawn_data in &spawns {
            if known_ids.contains(&spawn_data.net_id) {
                continue; // Already exists locally with matching NetworkId
            }

            let Some(kind) = parse_entity_kind(&spawn_data.kind) else {
                warn!("Unknown EntityKind from host: {}", spawn_data.kind);
                continue;
            };
            let Some(faction) = parse_faction(&spawn_data.faction) else {
                warn!("Unknown Faction from host: {}", spawn_data.faction);
                continue;
            };

            let pos = Vec3::new(spawn_data.pos[0], spawn_data.pos[1], spawn_data.pos[2]);

            // Check for a matching local entity that doesn't have a NetworkId yet
            // (e.g. initial workers from spawn_all_players that haven't been ID'd yet).
            // Also catches entities whose local NetworkId differs from host's.
            let mut matched_local = None;
            let mut best_dist = f32::MAX;
            for (entity, ek, ef, etf, opt_nid) in &all_entities {
                if *ek != kind || *ef != faction {
                    continue;
                }
                // Skip entities that already have a DIFFERENT NetworkId (they belong to another host entity)
                if let Some(nid) = opt_nid {
                    if nid.0 != spawn_data.net_id {
                        continue;
                    }
                }
                let dist = etf.translation.distance(pos);
                if dist < 5.0 && dist < best_dist {
                    best_dist = dist;
                    matched_local = Some(entity);
                }
            }

            if let Some(local_entity) = matched_local {
                // Adopt local entity: assign the host's NetworkId
                commands.entity(local_entity).insert(NetworkId(spawn_data.net_id));
                adopted += 1;
            } else {
                // No local match — spawn a new entity
                let entity = spawn_from_blueprint_with_faction(
                    &mut commands,
                    &cache,
                    kind,
                    pos,
                    &registry,
                    building_models.as_deref(),
                    unit_models.as_deref(),
                    &height_map,
                    faction,
                );
                commands.entity(entity).insert(NetworkId(spawn_data.net_id));
                spawned += 1;
            }
        }
        if spawned > 0 || adopted > 0 {
            info!(
                "Client entity sync: {} spawned, {} adopted (matched local)",
                spawned, adopted
            );
        }
    }

    // ── Handle despawns ──
    if !pending.despawns.is_empty() {
        let despawns = std::mem::take(&mut pending.despawns);
        let mut removed = 0u32;
        for net_id in &despawns {
            if let Some(&ecs_entity) = net_map.to_ecs.get(net_id) {
                commands.entity(ecs_entity).despawn();
                removed += 1;
            }
        }
        if removed > 0 {
            info!("Client despawned {} entities from host", removed);
        }
    }
}

/// Parse an EntityKind from its Debug name (e.g. "Worker", "Base").
fn parse_entity_kind(s: &str) -> Option<EntityKind> {
    // Use serde deserialization — EntityKind derives Deserialize
    serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
}

/// Parse a Faction from its Debug name (e.g. "Player1", "Neutral").
fn parse_faction(s: &str) -> Option<Faction> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
}

/// Detect host disconnect and return to main menu.
pub fn client_handle_disconnect(
    client: Res<ClientNetState>,
    mut next_state: ResMut<NextState<AppState>>,
    mut net_role: ResMut<NetRole>,
) {
    if client.shutdown.load(Ordering::Relaxed) {
        warn!("Host disconnected — returning to main menu");
        debug_tap::record_info("client_state", "host disconnected -> main menu");
        *net_role = NetRole::Offline;
        next_state.set(AppState::MainMenu);
    }
}

/// Periodically send Ping to the host to keep VPN/Hamachi tunnels alive.
pub fn client_send_ping(
    client: Res<ClientNetState>,
    time: Res<Time>,
    mut ping_timer: ResMut<ClientPingTimer>,
) {
    ping_timer.0.tick(time.delta());
    if !ping_timer.0.just_finished() {
        return;
    }
    let seq = {
        let mut s = client.seq.lock().unwrap();
        *s += 1;
        *s
    };
    let ping = game_state::message::ClientMessage::Ping {
        seq,
        timestamp: time.elapsed_secs_f64(),
    };
    if let Ok(json) = serde_json::to_vec(&ping) {
        let _ = client.outgoing.send(json);
    }
}
