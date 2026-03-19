//! Client-side systems: receive relayed commands from host, handle disconnect.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::sync::atomic::Ordering;
use std::sync::mpsc::TryRecvError;

use game_state::codec;
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

/// Pending neutral world updates from the host, processed by a separate system.
#[derive(Resource, Default)]
pub struct PendingNeutralUpdates {
    pub deltas: Vec<game_state::message::NeutralWorldSnapshot>,
    pub despawns: Vec<u32>,
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
    pub pending_neutral: ResMut<'w, PendingNeutralUpdates>,
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
                    for (faction_idx, amounts) in faction_data {
                        if let Some(faction) = Faction::from_net_index(*faction_idx) {
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
                            "  spawn net_id={} kind={} faction={} pos=({:.1},{:.1},{:.1})",
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
                ServerMessage::NeutralWorldDelta { objects, .. } => {
                    building_sync.pending_neutral.deltas.extend(objects.iter().cloned());
                }
                ServerMessage::NeutralWorldDespawn { net_ids, .. } => {
                    building_sync.pending_neutral.despawns.extend(net_ids.iter().copied());
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
///
/// Deterministic: always trusts host EntitySpawn as authoritative.
/// No distance-based adoption heuristic — if an entity with the same NetworkId
/// already exists, skip it; otherwise spawn fresh from blueprint.
pub fn client_apply_entity_sync(
    mut commands: Commands,
    mut pending: ResMut<PendingNetSpawns>,
    net_map: Res<EntityNetMap>,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    height_map: Res<HeightMap>,
    building_models: Option<Res<BuildingModelAssets>>,
    unit_models: Option<Res<UnitModelAssets>>,
    existing_with_id: Query<(Entity, &NetworkId)>,
) {
    // ── Handle spawns (batched: max 8 per frame to avoid WASM stalls) ──
    if !pending.spawns.is_empty() {
        let mut known_ids: std::collections::HashSet<u32> =
            existing_with_id.iter().map(|(_, nid)| nid.0).collect();

        let batch_size = 8;
        let remaining = if pending.spawns.len() > batch_size {
            pending.spawns.split_off(batch_size)
        } else {
            Vec::new()
        };
        let spawns = std::mem::replace(&mut pending.spawns, remaining);
        let mut spawned = 0u32;
        let mut skipped_known = 0u32;
        let mut skipped_parse = 0u32;
        for spawn_data in &spawns {
            if known_ids.contains(&spawn_data.net_id) {
                skipped_known += 1;
                continue;
            }

            let Some(kind) = EntityKind::from_index(spawn_data.kind) else {
                warn!("Unknown EntityKind index from host: {}", spawn_data.kind);
                skipped_parse += 1;
                continue;
            };
            let Some(faction) = Faction::from_net_index(spawn_data.faction) else {
                warn!("Unknown Faction index from host: {}", spawn_data.faction);
                skipped_parse += 1;
                continue;
            };

            let pos = Vec3::new(spawn_data.pos[0], spawn_data.pos[1], spawn_data.pos[2]);

            // Deterministic: always spawn fresh from blueprint, no distance heuristic
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
            known_ids.insert(spawn_data.net_id);
            spawned += 1;
        }
        if spawned > 0 || skipped_known > 0 || skipped_parse > 0 {
            info!(
                "Client entity sync: {} spawned, {} already known, {} parse failures (pending remaining: {})",
                spawned, skipped_known, skipped_parse, pending.spawns.len(),
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


/// Applies neutral world delta updates (resource node amounts) from the host.
/// Matches by NetworkId if available, otherwise by position (both sides share world gen).
pub fn client_apply_neutral_sync(
    mut commands: Commands,
    mut pending: ResMut<PendingNeutralUpdates>,
    net_map: Res<EntityNetMap>,
    mut resource_nodes: Query<(Entity, &mut ResourceNode, &GlobalTransform, Option<&NetworkId>)>,
) {
    if pending.deltas.is_empty() && pending.despawns.is_empty() {
        return;
    }

    let deltas = std::mem::take(&mut pending.deltas);
    let mut matched = 0u32;
    let mut unmatched = 0u32;

    for snap in &deltas {
        let amount = snap.amount_remaining.unwrap_or(0);

        // Try direct net_id lookup first
        if let Some(&ecs_entity) = net_map.to_ecs.get(&snap.net_id) {
            if let Ok((_, mut node, _, _)) = resource_nodes.get_mut(ecs_entity) {
                node.amount_remaining = amount;
                matched += 1;
                continue;
            }
        }

        // Fallback: match by position (both sides share the same world gen seed)
        let snap_pos = Vec3::new(snap.pos[0], snap.pos[1], snap.pos[2]);
        let mut found = false;
        for (entity, mut node, gt, existing_nid) in &mut resource_nodes {
            if existing_nid.is_some() {
                // Already has a NetworkId that didn't match — skip
                if existing_nid.unwrap().0 != snap.net_id {
                    continue;
                }
            }
            let dist = gt.translation().distance(snap_pos);
            if dist < 1.0 {
                node.amount_remaining = amount;
                // Assign the host's NetworkId so future lookups are O(1)
                if existing_nid.is_none() {
                    commands.entity(entity).insert(NetworkId(snap.net_id));
                }
                matched += 1;
                found = true;
                break;
            }
        }
        if !found {
            unmatched += 1;
        }
    }

    // Handle despawns
    let despawns = std::mem::take(&mut pending.despawns);
    for net_id in &despawns {
        if let Some(&ecs_entity) = net_map.to_ecs.get(net_id) {
            commands.entity(ecs_entity).despawn();
        }
    }

    if matched > 0 || unmatched > 0 {
        debug!(
            "Neutral world sync: {} matched, {} unmatched",
            matched, unmatched,
        );
    }
}

/// Parse an EntityKind from its Debug name (e.g. "Worker", "Base").
/// Still needed for BuildingSync training queue which uses string names.
fn parse_entity_kind(s: &str) -> Option<EntityKind> {
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
    if let Ok(bytes) = codec::encode(&ping) {
        let _ = client.outgoing.send(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_state::message::{ClientMessage, NetUnitState};
    use std::sync::atomic::AtomicBool;
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    use crate::components::{AppState, AssignedPhase};
    use crate::net_bridge::EntityNetMap;
    use crate::ui::event_log_widget::GameEventLog;

    #[test]
    fn u8_to_stance_maps_expected_values() {
        assert_eq!(u8_to_stance(0), UnitStance::Passive);
        assert_eq!(u8_to_stance(1), UnitStance::Defensive);
        assert_eq!(u8_to_stance(2), UnitStance::Aggressive);
        assert_eq!(u8_to_stance(9), UnitStance::Defensive);
    }

    #[test]
    fn net_to_ecs_unit_state_resolves_entity_references() {
        let building = Entity::from_bits(11);
        let target = Entity::from_bits(22);
        let mut net_map = EntityNetMap::default();
        net_map.to_ecs.insert(7, target);
        net_map.to_ecs.insert(9, building);

        assert_eq!(
            net_to_ecs_unit_state(&NetUnitState::Moving { target: [1.0, 2.0, 3.0] }, &net_map),
            Some(UnitState::Moving(Vec3::new(1.0, 2.0, 3.0)))
        );
        assert_eq!(
            net_to_ecs_unit_state(&NetUnitState::Attacking { target_id: 7 }, &net_map),
            Some(UnitState::Attacking(target))
        );
        assert_eq!(
            net_to_ecs_unit_state(
                &NetUnitState::AssignedGathering {
                    building_id: 9,
                    phase: 2,
                },
                &net_map
            ),
            Some(UnitState::AssignedGathering {
                building,
                phase: AssignedPhase::Harvesting {
                    node: building,
                    timer_secs: 0.0,
                },
            })
        );
        assert_eq!(
            net_to_ecs_unit_state(&NetUnitState::MovingToBuild { target_id: 77 }, &net_map),
            None
        );
    }

    #[test]
    fn parse_entity_kind_accepts_debug_names() {
        assert_eq!(parse_entity_kind("Worker"), Some(EntityKind::Worker));
        assert_eq!(parse_entity_kind("Base"), Some(EntityKind::Base));
        assert_eq!(parse_entity_kind("NotARealKind"), None);
    }

    #[test]
    fn client_interpolate_remote_units_blends_transform() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        let entity = app
            .world_mut()
            .spawn((
                Transform::default(),
                NetInterpolation {
                    prev_pos: Vec3::ZERO,
                    target_pos: Vec3::new(10.0, 0.0, 0.0),
                    prev_rot: 0.0,
                    target_rot: std::f32::consts::FRAC_PI_2,
                    blend: 0.0,
                    rate: 2.0,
                },
            ))
            .id();
        app.add_systems(Update, client_interpolate_remote_units);

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(0.25));
        app.update();

        let transform = app.world().entity(entity).get::<Transform>().unwrap();
        let interp = app.world().entity(entity).get::<NetInterpolation>().unwrap();
        assert!((transform.translation.x - 5.0).abs() < 0.001);
        assert!((interp.blend - 0.5).abs() < 0.001);
    }

    #[test]
    fn client_handle_disconnect_returns_to_menu_and_clears_role() {
        let (_incoming_tx, incoming_rx) = mpsc::channel();
        let (outgoing_tx, _outgoing_rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(true));

        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.init_state::<AppState>();
        app.insert_resource(ClientNetState {
            incoming: Mutex::new(incoming_rx),
            outgoing: outgoing_tx,
            shutdown,
            player_id: 1,
            seat_index: 0,
            my_faction: Faction::Player2,
            color_index: 1,
            seq: Mutex::new(0),
            session_token: 0,
        });
        app.insert_resource(NetRole::Client);
        app.insert_resource(GameEventLog::default());
        app.insert_resource(Time::<()>::default());
        app.add_systems(Update, client_handle_disconnect);

        app.update();

        assert_eq!(*app.world().resource::<NetRole>(), NetRole::Offline);
        assert_eq!(
            app.world().resource::<State<AppState>>().get(),
            &AppState::MainMenu
        );
        assert_eq!(app.world().resource::<GameEventLog>().entries.len(), 1);
    }

    #[test]
    fn client_send_ping_emits_encoded_message_and_updates_seq() {
        let (_incoming_tx, incoming_rx) = mpsc::channel();
        let (outgoing_tx, outgoing_rx) = mpsc::channel();

        let mut app = App::new();
        app.insert_resource(ClientNetState {
            incoming: Mutex::new(incoming_rx),
            outgoing: outgoing_tx,
            shutdown: Arc::new(AtomicBool::new(false)),
            player_id: 3,
            seat_index: 1,
            my_faction: Faction::Player3,
            color_index: 2,
            seq: Mutex::new(0),
            session_token: 0,
        });
        app.insert_resource(ClientPingTimer(Timer::from_seconds(0.1, TimerMode::Repeating)));
        app.insert_resource(NetStats::default());
        app.insert_resource(Time::<()>::default());
        app.add_systems(Update, client_send_ping);

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(0.1));
        app.update();

        let encoded = outgoing_rx.try_recv().unwrap();
        let decoded: ClientMessage = codec::decode(&encoded).unwrap();
        match decoded {
            ClientMessage::Ping { seq, timestamp } => {
                assert_eq!(seq, 1);
                assert!(timestamp >= 0.1);
            }
            other => panic!("expected ping, got {other:?}"),
        }

        assert_eq!(*app.world().resource::<ClientNetState>().seq.lock().unwrap(), 1);
        assert!(app.world().resource::<NetStats>().last_ping_sent_at >= 0.1);
    }
}
