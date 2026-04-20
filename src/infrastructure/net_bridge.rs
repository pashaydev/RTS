//! Stable entity IDs for lockstep wire commands.
//!
//! Deterministic lockstep carries `entity_ids: Vec<u32>` in `PlayerInput`, so
//! every peer needs a matching Bevy `Entity` ↔ stable `u32` table. Each peer
//! runs the same sort-and-assign pass locally against the same deterministic
//! simulation state, so the same ECS entity gets the same `NetworkId` on
//! every machine.

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

/// Bidirectional map: Bevy Entity ↔ network u32.
#[derive(Resource, Default)]
pub struct EntityNetMap {
    pub to_net: HashMap<Entity, u32>,
    pub to_ecs: HashMap<u32, Entity>,
}

fn assign_network_ids(
    mut commands: Commands,
    mut counter: ResMut<NetworkIdCounter>,
    mut net_map: ResMut<EntityNetMap>,
    gameplay_query: Query<
        (Entity, &EntityKind, Option<&Faction>, Option<&Transform>),
        (With<EntityKind>, Without<NetworkId>),
    >,
    neutral_query: Query<
        (Entity, Option<&Transform>),
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

    let mut pending: Vec<_> = gameplay_query
        .iter()
        .map(|(entity, kind, faction, transform)| {
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
            (entity, kind_sort_key(*kind), faction_key, transform_key)
        })
        .collect();

    let mut neutral_pending: Vec<_> = neutral_query
        .iter()
        .map(|(entity, transform)| {
            let transform_key = transform
                .map(|t| {
                    (
                        ordered_f32_bits(t.translation.x),
                        ordered_f32_bits(t.translation.y),
                        ordered_f32_bits(t.translation.z),
                    )
                })
                .unwrap_or((u32::MAX, u32::MAX, u32::MAX));
            (entity, usize::MAX, u8::MAX, transform_key)
        })
        .collect();
    neutral_pending.sort_by_key(|(_, _, _, transform_key)| *transform_key);

    pending.sort_by_key(|(_, kind_key, faction_key, transform_key)| {
        (*kind_key, *faction_key, *transform_key)
    });

    pending.extend(neutral_pending);

    for (entity, _, _, _) in pending {
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
            .add_systems(OnEnter(AppState::InGame), reset_network_identity)
            // Runs at the start of every simulated fixed tick so every peer
            // walks the same ECS state (lockstep) and assigns identical
            // NetworkIds. Running in `Update` is not lockstep-synchronized:
            // different peers can call it against different tick-counts of
            // state and produce divergent id assignments.
            .add_systems(
                FixedFirst,
                assign_network_ids.run_if(in_state(AppState::InGame)),
            );
    }
}

fn reset_network_identity(
    mut counter: ResMut<NetworkIdCounter>,
    mut net_map: ResMut<EntityNetMap>,
) {
    counter.0 = 0;
    net_map.to_net.clear();
    net_map.to_ecs.clear();
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
