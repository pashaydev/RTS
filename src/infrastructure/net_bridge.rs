//! Stable entity IDs for lockstep wire commands.
//!
//! Deterministic lockstep carries `entity_ids: Vec<u32>` in `PlayerInput`, so
//! every peer needs a matching Bevy `Entity` ↔ stable `u32` table. Each peer
//! runs the same sort-and-assign pass locally against the same deterministic
//! simulation state, so the same ECS entity gets the same `NetworkId` on
//! every machine.
//!
//! # Tie-breaking
//!
//! Sorting by `(kind, faction, transform)` alone is not enough when multiple
//! same-kind entities spawn at the exact same position and faction during
//! the same tick (starter workers stacked on a rally point, AI mass-spawns,
//! wave spawners, wall pieces that collapsed onto the same cell). The final
//! tie-breaker is a [`SpawnSerial`] that the spawn helpers attach from a
//! per-match monotonic counter. Under lockstep, every peer executes the
//! same commands in the same order, so the serials match — and when the
//! transform/kind/faction keys happen to collide, the serial still breaks
//! the tie deterministically.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::EntityKind;
use crate::types::{
    AppState, ExplosiveProp, Faction, GrowingResource, GrowingTree, MatureTree, ResourceNode,
    Sapling,
};

/// Stable network identity for an ECS entity. Persists across ticks.
#[derive(Component, Clone, Copy, Debug)]
pub struct NetworkId(pub u32);

/// Monotonically increasing counter for assigning NetworkIds.
#[derive(Resource, Default)]
pub struct NetworkIdCounter(u32);

impl NetworkIdCounter {
    pub fn next(&mut self) -> u32 {
        self.0 += 1;
        self.0
    }
}

/// Deterministic spawn-order tag attached to entities that should participate
/// in the NetworkId assignment tie-break.
///
/// The value is drawn from [`SpawnSerialCounter`] at spawn time. Lockstep
/// guarantees both peers execute the same `commands.spawn()` sequence in the
/// same tick order, so the serials match across peers.
#[derive(Component, Clone, Copy, Debug)]
pub struct SpawnSerial(pub u64);

/// Per-match monotonic counter that drives [`SpawnSerial`]. Reset on
/// `OnEnter(InGame)` (or whenever `reset_network_identity` runs) so the
/// serials are scoped to a single match.
#[derive(Resource, Default, Debug)]
pub struct SpawnSerialCounter(u64);

impl SpawnSerialCounter {
    pub fn next(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }
}

/// Bidirectional map: Bevy Entity ↔ network u32.
#[derive(Resource, Default)]
pub struct EntityNetMap {
    pub to_net: HashMap<Entity, u32>,
    pub to_ecs: HashMap<u32, Entity>,
}

fn assign_network_ids(
    mut commands: Commands,
    mut counter: ResMut<NetworkIdCounter>,
    mut serial_counter: ResMut<SpawnSerialCounter>,
    mut net_map: ResMut<EntityNetMap>,
    gameplay_query: Query<
        (
            Entity,
            &EntityKind,
            Option<&Faction>,
            Option<&Transform>,
            Option<&SpawnSerial>,
        ),
        (With<EntityKind>, Without<NetworkId>),
    >,
    neutral_query: Query<
        (Entity, Option<&Transform>, Option<&SpawnSerial>),
        (
            Without<EntityKind>,
            Without<NetworkId>,
            Or<(
                With<ResourceNode>,
                With<Sapling>,
                With<GrowingTree>,
                With<GrowingResource>,
                With<MatureTree>,
                With<ExplosiveProp>,
            )>,
        ),
    >,
    added: Query<(Entity, &NetworkId), Added<NetworkId>>,
    mut removed: RemovedComponents<NetworkId>,
) {
    // Keep the map in sync with NetworkId churn first — this lets the
    // assignment below run against a clean view of "already mapped" state.
    for entity in removed.read() {
        if let Some(net_id) = net_map.to_net.remove(&entity) {
            net_map.to_ecs.remove(&net_id);
        }
    }
    for (entity, net_id) in &added {
        net_map.to_net.insert(entity, net_id.0);
        net_map.to_ecs.insert(net_id.0, entity);
    }

    // Gameplay entities sort by kind → faction → transform → spawn serial.
    // The serial is the desync-safe final tie-break when two entities share
    // the first three keys (e.g. two workers spawned at the same rally
    // point in the same tick). Entities without a serial yet get assigned
    // one now, sorted by the preceding keys plus `entity.index()` as a
    // one-time bootstrap — this is the *only* place a raw Bevy entity id
    // is consulted, and it only affects the order in which freshly spawned
    // peers pick up their persistent serials (which under lockstep matches
    // across peers because `commands.spawn()` order matches).
    let mut pending: Vec<(
        usize,
        u8,
        (u32, u32, u32),
        Option<u64>,
        u32,
        Entity,
    )> = gameplay_query
        .iter()
        .map(|(entity, kind, faction, transform, spawn_serial)| {
            let faction_key = faction.map(faction_sort_key).unwrap_or(u8::MAX);
            let transform_key = transform
                .map(|t| {
                    (
                        ordered_f32_bits(t.translation.x),
                        ordered_f32_bits(t.translation.y),
                        ordered_f32_bits(t.translation.z),
                    )
                })
                .unwrap_or((u32::MAX, u32::MAX, u32::MAX));
            (
                kind_sort_key(*kind),
                faction_key,
                transform_key,
                spawn_serial.map(|s| s.0),
                entity.index().index(),
                entity,
            )
        })
        .collect();
    pending.sort_by_key(|(kind, faction, transform, serial, entity_index, _)| {
        (
            *kind,
            *faction,
            *transform,
            serial.unwrap_or(u64::MAX),
            *entity_index,
        )
    });

    let mut neutral_pending: Vec<((u32, u32, u32), Option<u64>, u32, Entity)> = neutral_query
        .iter()
        .map(|(entity, transform, spawn_serial)| {
            let transform_key = transform
                .map(|t| {
                    (
                        ordered_f32_bits(t.translation.x),
                        ordered_f32_bits(t.translation.y),
                        ordered_f32_bits(t.translation.z),
                    )
                })
                .unwrap_or((u32::MAX, u32::MAX, u32::MAX));
            (transform_key, spawn_serial.map(|s| s.0), entity.index().index(), entity)
        })
        .collect();
    neutral_pending.sort_by_key(|(transform, serial, entity_index, _)| {
        (*transform, serial.unwrap_or(u64::MAX), *entity_index)
    });

    for (_, _, _, existing_serial, _, entity) in pending {
        if existing_serial.is_none() {
            commands
                .entity(entity)
                .insert(SpawnSerial(serial_counter.next()));
        }
        let id = counter.next();
        commands.entity(entity).insert(NetworkId(id));
        net_map.to_net.insert(entity, id);
        net_map.to_ecs.insert(id, entity);
    }
    for (_, existing_serial, _, entity) in neutral_pending {
        if existing_serial.is_none() {
            commands
                .entity(entity)
                .insert(SpawnSerial(serial_counter.next()));
        }
        let id = counter.next();
        commands.entity(entity).insert(NetworkId(id));
        net_map.to_net.insert(entity, id);
        net_map.to_ecs.insert(id, entity);
    }
}

pub struct NetBridgePlugin;

impl Plugin for NetBridgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NetworkIdCounter>()
            .init_resource::<EntityNetMap>()
            .init_resource::<SpawnSerialCounter>()
            .add_systems(OnEnter(AppState::InGame), reset_network_identity)
            // `assign_network_ids` also assigns SpawnSerial to any newly
            // spawned gameplay entity before using it as the final tie-break
            // for NetworkId assignment. Running at FixedFirst keeps the two
            // passes in lockstep with the tick gate.
            .add_systems(
                FixedFirst,
                assign_network_ids.run_if(in_state(AppState::InGame)),
            );
    }
}

fn reset_network_identity(
    mut counter: ResMut<NetworkIdCounter>,
    mut net_map: ResMut<EntityNetMap>,
    mut spawn_serial: ResMut<SpawnSerialCounter>,
) {
    counter.0 = 0;
    net_map.to_net.clear();
    net_map.to_ecs.clear();
    spawn_serial.0 = 0;
}


fn ordered_f32_bits(value: f32) -> u32 {
    let bits = value.to_bits();
    if bits & 0x8000_0000 == 0 {
        bits | 0x8000_0000
    } else {
        !bits
    }
}

fn faction_sort_key(faction: &Faction) -> u8 {
    match faction {
        Faction::Player1 => 0,
        Faction::Player2 => 1,
        Faction::Player3 => 2,
        Faction::Player4 => 3,
        Faction::Neutral => 4,
    }
}

fn kind_sort_key(kind: EntityKind) -> usize {
    EntityKind::ALL
        .iter()
        .position(|candidate| *candidate == kind)
        .unwrap_or(usize::MAX)
}
