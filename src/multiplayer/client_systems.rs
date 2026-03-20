//! Client-side systems: receive relayed commands from host, handle disconnect.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_matchbox::prelude::*;
use std::sync::atomic::Ordering;

use game_state::message::{
    BuildingSnapshot, DayCycleSnapshot, EntitySnapshot, EntitySpawnData, GameEvent, NetUnitState,
    PlayerInput, ServerMessage, WorldBaseline,
};

use crate::blueprints::{
    spawn_from_blueprint_with_faction, BlueprintRegistry, EntityKind, EntityVisualCache,
};
use crate::components::*;
use crate::ground::HeightMap;
use crate::lighting::DayCycle;
use crate::model_assets::{BuildingModelAssets, UnitModelAssets};
use crate::net_bridge::{EntityNetMap, NetworkId};

use super::debug_tap;
use super::host_systems::execute_input_command;
use super::transport::{self, MatchboxInbox};
use super::{ClientNetState, NetRole, NetStats};
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

#[derive(Resource, Default)]
pub struct PendingRelayedInputs {
    pub inputs: Vec<PlayerInput>,
}

#[derive(Resource, Default)]
pub struct PendingStateSync {
    pub latest_seq: u32,
    pub entities: Vec<EntitySnapshot>,
}

#[derive(Resource, Default)]
pub struct PendingBuildingSync {
    pub buildings: Vec<BuildingSnapshot>,
}

#[derive(Resource, Default)]
pub struct PendingResourceSync {
    pub factions: Vec<(u8, [u32; 10])>,
}

#[derive(Resource, Default)]
pub struct PendingDayCycleSync {
    pub cycle: Option<DayCycleSnapshot>,
}

#[derive(Resource, Default)]
pub struct PendingNetEvents {
    pub events: Vec<GameEvent>,
}

#[derive(Resource, Default)]
pub struct PendingBaseline {
    pub baseline: Option<WorldBaseline>,
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
    factions
        .get(entity)
        .map_or(false, |f| *f == active_player.0)
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
        NetUnitState::Moving { target } => Some(UnitState::Moving(Vec3::new(
            target[0], target[1], target[2],
        ))),
        NetUnitState::Attacking { target_id } => net_map
            .to_ecs
            .get(target_id)
            .map(|&e| UnitState::Attacking(e)),
        NetUnitState::Gathering { target_id } => net_map
            .to_ecs
            .get(target_id)
            .map(|&e| UnitState::Gathering(e)),
        NetUnitState::Returning { depot_id } => {
            net_map
                .to_ecs
                .get(depot_id)
                .map(|&e| UnitState::ReturningToDeposit {
                    depot: e,
                    gather_node: None,
                })
        }
        NetUnitState::Depositing { depot_id } => {
            net_map
                .to_ecs
                .get(depot_id)
                .map(|&e| UnitState::Depositing {
                    depot: e,
                    gather_node: None,
                })
        }
        NetUnitState::MovingToPlot { target } => Some(UnitState::MovingToPlot(Vec3::new(
            target[0], target[1], target[2],
        ))),
        NetUnitState::MovingToBuild { target_id } => net_map
            .to_ecs
            .get(target_id)
            .map(|&e| UnitState::MovingToBuild(e)),
        NetUnitState::Building { target_id } => net_map
            .to_ecs
            .get(target_id)
            .map(|&e| UnitState::Building(e)),
        NetUnitState::AssignedGathering { building_id, phase } => {
            net_map.to_ecs.get(building_id).map(|&e| {
                let assigned_phase = match phase {
                    1 => AssignedPhase::MovingToNode(e), // fallback: use building as target
                    2 => AssignedPhase::Harvesting {
                        node: e,
                        timer_secs: 0.0,
                    },
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
        NetUnitState::AttackMoving { target } => Some(UnitState::AttackMoving(Vec3::new(
            target[0], target[1], target[2],
        ))),
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
/// and stages them for follow-up apply systems.
pub fn client_receive_commands(
    net_state: (Res<ClientNetState>, ResMut<MatchboxInbox>),
    mut pending_inputs: ResMut<PendingRelayedInputs>,
    mut pending_state: ResMut<PendingStateSync>,
    mut pending_buildings: ResMut<PendingBuildingSync>,
    mut pending_resources: ResMut<PendingResourceSync>,
    mut pending_day_cycle: ResMut<PendingDayCycleSync>,
    mut pending_events: ResMut<PendingNetEvents>,
    mut pending_baseline: ResMut<PendingBaseline>,
    mut pending_spawns: ResMut<PendingNetSpawns>,
    mut pending_neutral: ResMut<PendingNeutralUpdates>,
    mut net_stats: ResMut<NetStats>,
    time: Res<Time>,
) {
    let (client, mut inbox) = net_state;

    // Detect host disconnect from matchbox peer events
    if !inbox.disconnected.is_empty() {
        inbox.disconnected.clear();
        client.disconnected.store(true, Ordering::Relaxed);
    }

    let messages = std::mem::take(&mut inbox.server_messages);
    for msg in &messages {
        match msg {
            ServerMessage::RelayedInput { input, .. } => {
                pending_inputs.inputs.push(input.clone());
            }
            ServerMessage::StateSync { seq, entities } => {
                pending_state.latest_seq = *seq;
                pending_state.entities = entities.clone();
                net_stats.last_sync_entity_count = entities.len() as u32;
                net_stats.pending_spawns = pending_spawns.spawns.len() as u32;
            }
            ServerMessage::BuildingSync { buildings, .. } => {
                pending_buildings
                    .buildings
                    .extend(buildings.iter().cloned());
            }
            ServerMessage::ResourceSync { factions, .. } => {
                pending_resources.factions.extend(factions.iter().copied());
            }
            ServerMessage::DayCycleSync { cycle, .. } => {
                pending_day_cycle.cycle = Some(cycle.clone());
            }
            ServerMessage::EntitySpawn { spawns, .. } => {
                pending_spawns.spawns.extend(spawns.iter().cloned());
            }
            ServerMessage::EntityDespawn { net_ids, .. } => {
                pending_spawns.despawns.extend(net_ids.iter().copied());
            }
            ServerMessage::WorldBaseline { baseline, .. } => {
                pending_baseline.baseline = Some(baseline.clone());
                debug_tap::record_info("client_world_sync", "queued world baseline");
            }
            ServerMessage::NeutralWorldDelta { objects, .. } => {
                pending_neutral.deltas.extend(objects.iter().cloned());
            }
            ServerMessage::NeutralWorldDespawn { net_ids, .. } => {
                pending_neutral.despawns.extend(net_ids.iter().copied());
            }
            ServerMessage::Pong { .. } => {
                let rtt = ((time.elapsed_secs_f64() - net_stats.last_ping_sent_at) * 1000.0) as f32;
                if rtt.is_finite() && rtt >= 0.0 {
                    net_stats.rtt_ms = rtt;
                    if net_stats.rtt_smoothed_ms == 0.0 {
                        net_stats.rtt_smoothed_ms = rtt;
                    } else {
                        net_stats.rtt_smoothed_ms = net_stats.rtt_smoothed_ms * 0.8 + rtt * 0.2;
                    }
                }
            }
            ServerMessage::Event { events, .. } => {
                pending_events.events.extend(events.iter().cloned());
            }
        }
    }
}

pub fn client_apply_relayed_inputs(
    mut commands: Commands,
    net_map: Res<EntityNetMap>,
    mut unit_states: Query<&mut UnitState>,
    mut task_queues: Query<&mut TaskQueue, With<Unit>>,
    mut next_task_id: ResMut<NextTaskId>,
    read_transforms: Query<&GlobalTransform>,
    mut pending_inputs: ResMut<PendingRelayedInputs>,
) {
    let inputs = std::mem::take(&mut pending_inputs.inputs);
    for input in &inputs {
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
}

pub fn client_apply_state_sync(
    mut commands: Commands,
    net_map: Res<EntityNetMap>,
    mut unit_states: Query<&mut UnitState>,
    mut write_transforms: Query<&mut Transform>,
    mut healths: Query<&mut Health>,
    factions: Query<&Faction>,
    active_player: Res<ActivePlayer>,
    mut pending_state: ResMut<PendingStateSync>,
    mut event_log: ResMut<GameEventLog>,
    time: Res<Time>,
    mut unit_sync: UnitSyncParams,
    mut building_sync: BuildingSyncParams,
) {
    let seq = pending_state.latest_seq;
    let entities = std::mem::take(&mut pending_state.entities);
    if entities.is_empty() {
        return;
    }

    building_sync.net_stats.net_map_size = net_map.to_ecs.len() as u32;
    let mut matched = 0u32;
    let mut unmatched = 0u32;
    let total = entities.len();
    for snap in &entities {
        let Some(&ecs_entity) = net_map.to_ecs.get(&snap.net_id) else {
            unmatched += 1;
            continue;
        };
        if is_local_entity(ecs_entity, &factions, &active_player) {
            continue;
        }

        let new_pos = Vec3::new(snap.pos[0], snap.pos[1], snap.pos[2]);
        if let Ok(mut interp) = unit_sync.interp_q.get_mut(ecs_entity) {
            let dist = interp.target_pos.distance(new_pos);
            if dist > 10.0 {
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
                interp.prev_pos = interp.target_pos;
                interp.target_pos = new_pos;
                interp.prev_rot = interp.target_rot;
                interp.target_rot = snap.rot_y;
                interp.blend = 0.0;
            }
        } else {
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

        if let Some(hp) = snap.health {
            if let Ok(mut health) = healths.get_mut(ecs_entity) {
                health.current = hp;
            }
        }
        if let Some(ref net_state) = snap.unit_state {
            if let Some(new_state) = net_to_ecs_unit_state(net_state, &net_map) {
                if let Ok(mut state) = unit_states.get_mut(ecs_entity) {
                    *state = new_state;
                }
            }
        }
        if let Some(mt) = snap.move_target {
            commands
                .entity(ecs_entity)
                .insert(MoveTarget(Vec3::new(mt[0], mt[1], mt[2])));
        } else {
            commands.entity(ecs_entity).remove::<MoveTarget>();
        }
        if let Some(at_id) = snap.attack_target {
            if let Some(&target_ecs) = net_map.to_ecs.get(&at_id) {
                commands.entity(ecs_entity).insert(AttackTarget(target_ecs));
            }
        } else {
            commands.entity(ecs_entity).remove::<AttackTarget>();
        }
        if let Some(ref net_carry) = snap.carrying {
            if let Ok(mut carry) = unit_sync.carrying_q.get_mut(ecs_entity) {
                carry.resource_type = ResourceType::ALL
                    .get(net_carry.resource_type as usize)
                    .copied();
                carry.amount = net_carry.amount;
            }
        }
        if let Some(stance_u8) = snap.stance {
            if let Ok(mut stance) = unit_sync.stances_q.get_mut(ecs_entity) {
                *stance = u8_to_stance(stance_u8);
            }
        }
    }

    if seq % 50 == 1 {
        let msg_text = format!(
            "Sync: {}/{} matched, {} unmatched (seq={}, net_map={})",
            matched,
            total,
            unmatched,
            seq,
            net_map.to_ecs.len(),
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

pub fn client_apply_building_sync(
    net_map: Res<EntityNetMap>,
    factions: Query<&Faction>,
    active_player: Res<ActivePlayer>,
    mut pending_buildings: ResMut<PendingBuildingSync>,
    mut building_sync: BuildingSyncParams,
) {
    let buildings = std::mem::take(&mut pending_buildings.buildings);
    for bsnap in &buildings {
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
                let duration = cp.timer.duration().as_secs_f32();
                cp.timer
                    .set_elapsed(std::time::Duration::from_secs_f32(duration * progress));
            }
        }
        if let Some(ref queue_names) = bsnap.training_queue {
            if let Ok(mut tq) = building_sync.training_q.get_mut(ecs_entity) {
                tq.queue = queue_names
                    .iter()
                    .filter_map(|name| parse_entity_kind(name))
                    .collect();
                if let Some(progress) = bsnap.training_progress {
                    if let Some(ref mut timer) = tq.timer {
                        let duration = timer.duration().as_secs_f32();
                        timer.set_elapsed(std::time::Duration::from_secs_f32(duration * progress));
                    }
                }
            }
        }
        if let Some(recipe_idx) = bsnap.active_recipe {
            if let Ok(mut ps) = building_sync.production_q.get_mut(ecs_entity) {
                ps.active_recipe = Some(recipe_idx as usize);
                if let Some(progress) = bsnap.production_progress {
                    let duration = ps.progress_timer.duration().as_secs_f32();
                    ps.progress_timer
                        .set_elapsed(std::time::Duration::from_secs_f32(duration * progress));
                }
            }
        }
    }
}

pub fn client_apply_resource_sync(
    mut pending_resources: ResMut<PendingResourceSync>,
    all_resources: ResMut<AllPlayerResources>,
) {
    let factions = std::mem::take(&mut pending_resources.factions);
    let mut all_resources = all_resources;
    for (faction_idx, amounts) in &factions {
        if let Some(faction) = Faction::from_net_index(*faction_idx) {
            let pr = all_resources.get_mut(&faction);
            pr.amounts = *amounts;
        }
    }
}

pub fn client_apply_day_cycle_sync(
    mut pending_day_cycle: ResMut<PendingDayCycleSync>,
    mut day_cycle: ResMut<DayCycle>,
) {
    let Some(cycle) = pending_day_cycle.cycle.take() else {
        return;
    };
    day_cycle.cycle_duration = cycle.cycle_duration.max(0.01);
    day_cycle.paused = cycle.paused;
    day_cycle.set_time(cycle.time);
}

pub fn client_apply_server_events(
    client: Res<ClientNetState>,
    mut pending_events: ResMut<PendingNetEvents>,
    mut event_log: ResMut<GameEventLog>,
    time: Res<Time>,
) {
    let events = std::mem::take(&mut pending_events.events);
    for event in &events {
        match event {
            GameEvent::Announcement { text } => {
                info!("Server announcement: {}", text);
                debug_tap::record_info("client_game_events", format!("announcement: {}", text));
                event_log.push_with_level(
                    time.elapsed_secs(),
                    text.clone(),
                    EventCategory::Network,
                    LogLevel::Info,
                    None,
                    None,
                );
            }
            GameEvent::HostShutdown { reason } => {
                warn!("Host ended match: {}", reason);
                debug_tap::record_info("client_game_events", format!("host_shutdown: {}", reason));
                event_log.push_with_level(
                    time.elapsed_secs(),
                    format!("Host ended match: {}", reason),
                    EventCategory::Network,
                    LogLevel::Error,
                    None,
                    None,
                );
                client.disconnected.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

pub fn client_apply_world_baseline(
    mut commands: Commands,
    mut pending_baseline: ResMut<PendingBaseline>,
    mut pending_neutral: ResMut<PendingNeutralUpdates>,
    mut resource_nodes: Query<(Entity, &GlobalTransform, Option<&NetworkId>), With<ResourceNode>>,
) {
    let Some(baseline) = pending_baseline.baseline.take() else {
        return;
    };
    for object in &baseline.neutral_objects {
        pending_neutral.deltas.push(object.clone());
        let snap_pos = Vec3::new(object.pos[0], object.pos[1], object.pos[2]);
        for (entity, gt, existing_nid) in &mut resource_nodes {
            if existing_nid.is_some() {
                continue;
            }
            if gt.translation().distance(snap_pos) < 1.0 {
                commands.entity(entity).insert(NetworkId(object.net_id));
                break;
            }
        }
    }
    debug_tap::record_info(
        "client_world_sync",
        format!(
            "applied world baseline with {} neutral objects",
            baseline.neutral_objects.len()
        ),
    );
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
    mut resource_nodes: Query<(
        Entity,
        &mut ResourceNode,
        &GlobalTransform,
        Option<&NetworkId>,
    )>,
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
    if client.disconnected.load(Ordering::Relaxed) {
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

/// Periodically send Ping to the host to keep connections alive and measure RTT.
pub fn client_send_ping(
    client: Res<ClientNetState>,
    mut socket: ResMut<MatchboxSocket>,
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
    transport::send_to_host(&mut socket, &ping);
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_state::message::{ClientMessage, NetUnitState};
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
            net_to_ecs_unit_state(
                &NetUnitState::Moving {
                    target: [1.0, 2.0, 3.0]
                },
                &net_map
            ),
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
        let interp = app
            .world()
            .entity(entity)
            .get::<NetInterpolation>()
            .unwrap();
        assert!((transform.translation.x - 5.0).abs() < 0.001);
        assert!((interp.blend - 0.5).abs() < 0.001);
    }

    #[test]
    fn client_handle_disconnect_returns_to_menu_and_clears_role() {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.init_state::<AppState>();
        let mut client = ClientNetState {
            player_id: 1,
            seat_index: 0,
            my_faction: Faction::Player2,
            color_index: 1,
            ..Default::default()
        };
        client.disconnected.store(true, Ordering::Relaxed);
        app.insert_resource(client);
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

    // Note: client_send_ping test removed — requires MatchboxSocket which needs a live connection.
}
