//! Client-side systems: receive relayed commands from host, handle disconnect.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::sync::atomic::Ordering;
use std::sync::mpsc::TryRecvError;

use game_state::message::{EntitySpawnData, NetUnitState, ServerMessage};

use crate::blueprints::{spawn_from_blueprint_with_faction, BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::components::*;
use crate::ground::HeightMap;
use crate::lighting::DayCycle;
use crate::model_assets::{BuildingModelAssets, UnitModelAssets};
use crate::net_bridge::{EntityNetMap, NetworkId};

use super::debug_tap;
use super::{ClientNetState, NetRole, NetStats};
use super::host_systems::execute_input_command;
use crate::ui::event_log_widget::{EventCategory, GameEventLog, LogLevel};

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

/// Interpolation state for smooth remote entity movement.
#[derive(Component)]
pub struct NetInterpolation {
    pub prev_pos: Vec3,
    pub target_pos: Vec3,
    pub prev_rot: f32,
    pub target_rot: f32,
    pub blend: f32,
    pub rate: f32,
}

impl Default for NetInterpolation {
    fn default() -> Self {
        Self {
            prev_pos: Vec3::ZERO,
            target_pos: Vec3::ZERO,
            prev_rot: 0.0,
            target_rot: 0.0,
            blend: 1.0,
            rate: 10.0, // 1.0 / 0.1s sync interval
        }
    }
}

/// Returns true if the given ECS entity belongs to the active player's faction.
fn is_local_entity(
    entity: Entity,
    factions: &Query<&Faction>,
    active_player: &ActivePlayer,
) -> bool {
    factions.get(entity).map_or(false, |f| *f == active_player.0)
}

fn u8_to_stance(v: u8) -> UnitStance {
    match v {
        0 => UnitStance::Passive,
        2 => UnitStance::Aggressive,
        _ => UnitStance::Defensive,
    }
}

fn net_to_ecs_unit_state(net: &NetUnitState, net_map: &EntityNetMap) -> Option<UnitState> {
    match net {
        NetUnitState::Idle => Some(UnitState::Idle),
        NetUnitState::Moving { target } => {
            Some(UnitState::Moving(Vec3::new(target[0], target[1], target[2])))
        }
        NetUnitState::Attacking { target_id } => {
            net_map.to_ecs.get(target_id).map(|&e| UnitState::Attacking(e))
        }
        NetUnitState::Gathering { target_id } => {
            net_map.to_ecs.get(target_id).map(|&e| UnitState::Gathering(e))
        }
        NetUnitState::Returning { depot_id } => {
            net_map.to_ecs.get(depot_id).map(|&e| UnitState::ReturningToDeposit {
                depot: e,
                gather_node: None,
            })
        }
        NetUnitState::Depositing { depot_id } => {
            net_map.to_ecs.get(depot_id).map(|&e| UnitState::Depositing {
                depot: e,
                gather_node: None,
            })
        }
        NetUnitState::MovingToPlot { target } => {
            Some(UnitState::MovingToPlot(Vec3::new(target[0], target[1], target[2])))
        }
        NetUnitState::MovingToBuild { target_id } => {
            net_map.to_ecs.get(target_id).map(|&e| UnitState::MovingToBuild(e))
        }
        NetUnitState::Building { target_id } => {
            net_map.to_ecs.get(target_id).map(|&e| UnitState::Building(e))
        }
        NetUnitState::AssignedGathering { building_id, phase } => {
            net_map.to_ecs.get(building_id).map(|&e| {
                let assigned_phase = match phase {
                    1 => AssignedPhase::MovingToNode(e), // fallback: use building as target
                    2 => AssignedPhase::Harvesting { node: e, timer_secs: 0.0 },
                    3 => AssignedPhase::ReturningToBuilding,
                    4 => AssignedPhase::Depositing { timer_secs: 0.0 },
                    _ => AssignedPhase::SeekingNode,
                };
                UnitState::AssignedGathering {
                    building: e,
                    phase: assigned_phase,
                }
            })
        }
        NetUnitState::Patrolling { target, origin } => Some(UnitState::Patrolling {
            target: Vec3::new(target[0], target[1], target[2]),
            origin: Vec3::new(origin[0], origin[1], origin[2]),
        }),
        NetUnitState::AttackMoving { target } => {
            Some(UnitState::AttackMoving(Vec3::new(target[0], target[1], target[2])))
        }
        NetUnitState::HoldPosition => Some(UnitState::HoldPosition),
    }
}

/// Bundled system params for building/resource sync to stay under Bevy's 16-param limit.
#[derive(SystemParam)]
pub struct BuildingSyncParams<'w, 's> {
    building_levels: Query<'w, 's, &'static mut BuildingLevel>,
    construction_q: Query<'w, 's, &'static mut ConstructionProgress>,
    training_q: Query<'w, 's, &'static mut TrainingQueue>,
    production_q: Query<'w, 's, &'static mut ProductionState>,
    all_resources: ResMut<'w, AllPlayerResources>,
    day_cycle: ResMut<'w, DayCycle>,
    pub net_stats: ResMut<'w, NetStats>,
}

/// Bundled system params for extended unit state sync.
#[derive(SystemParam)]
pub struct UnitSyncParams<'w, 's> {
    carrying_q: Query<'w, 's, &'static mut Carrying>,
    stances_q: Query<'w, 's, &'static mut UnitStance>,
    interp_q: Query<'w, 's, &'static mut NetInterpolation>,
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
    mut event_log: ResMut<GameEventLog>,
    time: Res<Time>,
    mut unit_sync: UnitSyncParams,
    mut building_sync: BuildingSyncParams,
) {
    let rx = client.incoming.lock().unwrap();
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
                    building_sync.net_stats.last_sync_entity_count = entities.len() as u32;
                    building_sync.net_stats.net_map_size = net_map.to_ecs.len() as u32;
                    building_sync.net_stats.pending_spawns = pending_spawns.spawns.len() as u32;
                    let mut matched = 0u32;
                    let mut unmatched = 0u32;
                    let total = entities.len();
                    for snap in entities {
                        let Some(&ecs_entity) = net_map.to_ecs.get(&snap.net_id) else {
                            unmatched += 1;
                            continue;
                        };
                        // Skip the active player's own units — they are moved locally
                        if is_local_entity(ecs_entity, &factions, &active_player) {
                            continue;
                        }

                        let new_pos = Vec3::new(snap.pos[0], snap.pos[1], snap.pos[2]);

                        // Interpolation: store target instead of snapping directly
                        if let Ok(mut interp) = unit_sync.interp_q.get_mut(ecs_entity) {
                            let dist = interp.target_pos.distance(new_pos);
                            if dist > 10.0 {
                                // Teleport threshold — snap directly
                                if let Ok(mut transform) = write_transforms.get_mut(ecs_entity) {
                                    transform.translation = new_pos;
                                    transform.rotation = Quat::from_rotation_y(snap.rot_y);
                                }
                                interp.prev_pos = new_pos;
                                interp.target_pos = new_pos;
                                interp.prev_rot = snap.rot_y;
                                interp.target_rot = snap.rot_y;
                                interp.blend = 1.0;
                            } else {
                                // Start interpolation toward new target
                                interp.prev_pos = interp.target_pos;
                                interp.target_pos = new_pos;
                                interp.prev_rot = interp.target_rot;
                                interp.target_rot = snap.rot_y;
                                interp.blend = 0.0;
                            }
                        } else {
                            // No interpolation component yet — insert one and snap
                            commands.entity(ecs_entity).insert(NetInterpolation {
                                prev_pos: new_pos,
                                target_pos: new_pos,
                                prev_rot: snap.rot_y,
                                target_rot: snap.rot_y,
                                blend: 1.0,
                                rate: 10.0,
                            });
                            if let Ok(mut transform) = write_transforms.get_mut(ecs_entity) {
                                transform.translation = new_pos;
                                transform.rotation = Quat::from_rotation_y(snap.rot_y);
                            }
                        }
                        matched += 1;

                        // Apply health from host
                        if let Some(hp) = snap.health {
                            if let Ok(mut health) = healths.get_mut(ecs_entity) {
                                health.current = hp;
                            }
                        }

                        // Apply unit state
                        if let Some(ref net_state) = snap.unit_state {
                            if let Some(new_state) = net_to_ecs_unit_state(net_state, &net_map) {
                                if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
                                    *state = new_state;
                                }
                            }
                        }

                        // Apply move target
                        if let Some(mt) = snap.move_target {
                            commands.entity(ecs_entity).insert(MoveTarget(Vec3::new(mt[0], mt[1], mt[2])));
                        } else {
                            commands.entity(ecs_entity).remove::<MoveTarget>();
                        }

                        // Apply attack target
                        if let Some(at_id) = snap.attack_target {
                            if let Some(&target_ecs) = net_map.to_ecs.get(&at_id) {
                                commands.entity(ecs_entity).insert(AttackTarget(target_ecs));
                            }
                        } else {
                            commands.entity(ecs_entity).remove::<AttackTarget>();
                        }

                        // Apply carrying
                        if let Some(ref net_carry) = snap.carrying {
                            if let Ok(mut carry) = unit_sync.carrying_q.get_mut(ecs_entity) {
                                carry.resource_type = ResourceType::ALL.get(net_carry.resource_type as usize).copied();
                                carry.amount = net_carry.amount;
                            }
                        }

                        // Apply stance
                        if let Some(stance_u8) = snap.stance {
                            if let Ok(mut stance) = unit_sync.stances_q.get_mut(ecs_entity) {
                                *stance = u8_to_stance(stance_u8);
                            }
                        }
                    }
                    // Log once every ~5 seconds
                    if *seq % 50 == 1 {
                        let msg_text = format!(
                            "Sync: {}/{} matched, {} unmatched (seq={}, net_map={})",
                            matched, total, unmatched, seq, net_map.to_ecs.len(),
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
                }
                ServerMessage::BuildingSync { buildings, .. } => {
                    for bsnap in buildings {
                        let Some(&ecs_entity) = net_map.to_ecs.get(&bsnap.net_id) else {
                            continue;
                        };
                        if is_local_entity(ecs_entity, &factions, &active_player) {
                            continue;
                        }
                        if let Some(level) = bsnap.level {
                            if let Ok(mut bl) = building_sync.building_levels.get_mut(ecs_entity) {
                                bl.0 = level;
                            }
                        }
                        if let Some(progress) = bsnap.construction_progress {
                            if let Ok(mut cp) = building_sync.construction_q.get_mut(ecs_entity) {
                                // Set timer fraction to reflect host progress
                                let duration = cp.timer.duration().as_secs_f32();
                                cp.timer.set_elapsed(std::time::Duration::from_secs_f32(
                                    duration * progress,
                                ));
                            }
                        }
                        if let Some(ref queue_names) = bsnap.training_queue {
                            if let Ok(mut tq) = building_sync.training_q.get_mut(ecs_entity) {
                                // Update queue from host data
                                tq.queue = queue_names
                                    .iter()
                                    .filter_map(|name| parse_entity_kind(name))
                                    .collect();
                                if let Some(progress) = bsnap.training_progress {
                                    if let Some(ref mut timer) = tq.timer {
                                        let duration = timer.duration().as_secs_f32();
                                        timer.set_elapsed(std::time::Duration::from_secs_f32(
                                            duration * progress,
                                        ));
                                    }
                                }
                            }
                        }
                        if let Some(recipe_idx) = bsnap.active_recipe {
                            if let Ok(mut ps) = building_sync.production_q.get_mut(ecs_entity) {
                                ps.active_recipe = Some(recipe_idx as usize);
                                if let Some(progress) = bsnap.production_progress {
                                    let duration = ps.progress_timer.duration().as_secs_f32();
                                    ps.progress_timer.set_elapsed(
                                        std::time::Duration::from_secs_f32(duration * progress),
                                    );
                                }
                            }
                        }
                    }
                }
                ServerMessage::ResourceSync { factions: faction_data, .. } => {
                    for (faction_name, amounts) in faction_data {
                        if let Some(faction) = parse_faction(faction_name) {
                            let pr = building_sync.all_resources.get_mut(&faction);
                            pr.amounts = *amounts;
                        }
                    }
                }
                ServerMessage::DayCycleSync { cycle, .. } => {
                    building_sync.day_cycle.cycle_duration = cycle.cycle_duration.max(0.01);
                    building_sync.day_cycle.paused = cycle.paused;
                    building_sync.day_cycle.set_time(cycle.time);
                }
                ServerMessage::EntitySpawn { spawns, .. } => {
                    info!(
                        "EntitySpawn received: {} entities (pending queue: {})",
                        spawns.len(),
                        pending_spawns.spawns.len(),
                    );
                    for s in spawns.iter() {
                        debug!(
                            "  spawn net_id={} kind='{}' faction='{}' pos=({:.1},{:.1},{:.1})",
                            s.net_id, s.kind, s.faction, s.pos[0], s.pos[1], s.pos[2],
                        );
                    }
                    pending_spawns.spawns.extend(spawns.iter().cloned());
                }
                ServerMessage::EntityDespawn { net_ids, .. } => {
                    pending_spawns.despawns.extend(net_ids.iter().copied());
                    debug_tap::record_info(
                        "client_entity_sync",
                        format!("queued {} entity despawns", net_ids.len()),
                    );
                }
                ServerMessage::WorldBaseline { .. } => {
                    debug_tap::record_info(
                        "client_world_sync",
                        "received world baseline (handler not wired yet)",
                    );
                }
                ServerMessage::NeutralWorldDelta { .. } => {
                    debug_tap::record_info(
                        "client_world_sync",
                        "received neutral world delta (handler not wired yet)",
                    );
                }
                ServerMessage::NeutralWorldDespawn { .. } => {
                    debug_tap::record_info(
                        "client_world_sync",
                        "received neutral world despawn (handler not wired yet)",
                    );
                }
                ServerMessage::Pong { .. } => {
                    let now = time.elapsed_secs_f64();
                    let rtt = ((now - building_sync.net_stats.last_ping_sent_at) * 1000.0) as f32;
                    building_sync.net_stats.rtt_ms = rtt;
                    if building_sync.net_stats.rtt_smoothed_ms == 0.0 {
                        building_sync.net_stats.rtt_smoothed_ms = rtt;
                    } else {
                        building_sync.net_stats.rtt_smoothed_ms =
                            building_sync.net_stats.rtt_smoothed_ms * 0.8 + rtt * 0.2;
                    }
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
                                event_log.push_with_level(
                                    time.elapsed_secs(),
                                    text.clone(),
                                    EventCategory::Network,
                                    LogLevel::Info,
                                    None,
                                    None,
                                );
                            }
                            game_state::message::GameEvent::HostShutdown { reason } => {
                                warn!("Host ended match: {}", reason);
                                debug_tap::record_info(
                                    "client_game_events",
                                    format!("host_shutdown: {}", reason),
                                );
                                event_log.push_with_level(
                                    time.elapsed_secs(),
                                    format!("Host ended match: {}", reason),
                                    EventCategory::Network,
                                    LogLevel::Error,
                                    None,
                                    None,
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

/// Smoothly interpolates remote entity positions between state sync snapshots.
pub fn client_interpolate_remote_units(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut NetInterpolation)>,
) {
    let dt = time.delta_secs();
    for (mut transform, mut interp) in &mut query {
        if interp.blend >= 1.0 {
            continue;
        }
        interp.blend = (interp.blend + dt * interp.rate).min(1.0);
        let t = interp.blend;

        // Lerp position
        transform.translation = interp.prev_pos.lerp(interp.target_pos, t);

        // Slerp rotation (Y-axis only)
        let prev_rot = Quat::from_rotation_y(interp.prev_rot);
        let target_rot = Quat::from_rotation_y(interp.target_rot);
        transform.rotation = prev_rot.slerp(target_rot, t);
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
        let mut known_ids: std::collections::HashSet<u32> =
            existing_with_id.iter().map(|(_, nid)| nid.0).collect();
        let mut claimed_local_entities: std::collections::HashSet<Entity> =
            std::collections::HashSet::new();

        let batch_size = 8;
        let remaining = if pending.spawns.len() > batch_size {
            pending.spawns.split_off(batch_size)
        } else {
            Vec::new()
        };
        let spawns = std::mem::replace(&mut pending.spawns, remaining);
        let mut spawned = 0u32;
        let mut adopted = 0u32;
        let mut skipped_known = 0u32;
        let mut skipped_parse = 0u32;
        for spawn_data in &spawns {
            if known_ids.contains(&spawn_data.net_id) {
                skipped_known += 1;
                continue;
            }

            let Some(kind) = parse_entity_kind(&spawn_data.kind) else {
                warn!("Unknown EntityKind from host: '{}'", spawn_data.kind);
                skipped_parse += 1;
                continue;
            };
            let Some(faction) = parse_faction(&spawn_data.faction) else {
                warn!("Unknown Faction from host: '{}'", spawn_data.faction);
                skipped_parse += 1;
                continue;
            };

            let pos = Vec3::new(spawn_data.pos[0], spawn_data.pos[1], spawn_data.pos[2]);

            // Check for a matching local entity that doesn't have a NetworkId yet.
            // Two-pass: first try entities without NetworkId (prefer adoption over re-matching).
            // Increased threshold to 15.0 units to account for height map differences.
            let mut matched_local = None;
            let mut best_dist = f32::MAX;
            for (entity, ek, ef, etf, opt_nid) in &all_entities {
                if claimed_local_entities.contains(&entity) {
                    continue;
                }
                if *ek != kind || *ef != faction {
                    continue;
                }
                if let Some(nid) = opt_nid {
                    if nid.0 != spawn_data.net_id {
                        continue;
                    }
                }
                let dist = etf.translation.distance(pos);
                if dist < 15.0 && dist < best_dist {
                    best_dist = dist;
                    matched_local = Some(entity);
                }
            }

            if let Some(local_entity) = matched_local {
                commands.entity(local_entity).insert(NetworkId(spawn_data.net_id));
                claimed_local_entities.insert(local_entity);
                known_ids.insert(spawn_data.net_id);
                adopted += 1;
            } else {
                // No local match — spawn a new entity from blueprint
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
                claimed_local_entities.insert(entity);
                known_ids.insert(spawn_data.net_id);
                spawned += 1;
                info!(
                    "EntitySync: no local match for net_id={} {:?}/{:?} at ({:.1},{:.1},{:.1}), spawned new",
                    spawn_data.net_id, kind, faction, pos.x, pos.y, pos.z,
                );
            }
        }
        if spawned > 0 || adopted > 0 || skipped_known > 0 || skipped_parse > 0 {
            info!(
                "Client entity sync: {} spawned, {} adopted, {} already known, {} parse failures (pending remaining: {})",
                spawned, adopted, skipped_known, skipped_parse, pending.spawns.len(),
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
    mut event_log: ResMut<GameEventLog>,
    time: Res<Time>,
) {
    if client.shutdown.load(Ordering::Relaxed) {
        warn!("Host disconnected — returning to main menu");
        debug_tap::record_info("client_state", "host disconnected -> main menu");
        event_log.push_with_level(
            time.elapsed_secs(),
            "Host disconnected — returning to menu".to_string(),
            EventCategory::Network,
            LogLevel::Error,
            None,
            None,
        );
        *net_role = NetRole::Offline;
        next_state.set(AppState::MainMenu);
    }
}

/// Periodically send Ping to the host to keep VPN/Hamachi tunnels alive.
pub fn client_send_ping(
    client: Res<ClientNetState>,
    time: Res<Time>,
    mut ping_timer: ResMut<ClientPingTimer>,
    mut net_stats: ResMut<NetStats>,
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
    net_stats.last_ping_sent_at = time.elapsed_secs_f64();
    let ping = game_state::message::ClientMessage::Ping {
        seq,
        timestamp: time.elapsed_secs_f64(),
    };
    if let Ok(json) = serde_json::to_vec(&ping) {
        let _ = client.outgoing.send(json);
    }
}
