//! ECS network identity bridge — assigns stable network IDs to entities.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::EntityKind;
use crate::components::{AppState, Faction};
use crate::multiplayer::NetRole;

// ── Core bridge types ────────────────────────────────────────────────────────

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

// ── System: assign_network_ids ───────────────────────────────────────────────

fn assign_network_ids(
    mut commands: Commands,
    mut counter: ResMut<NetworkIdCounter>,
    net_role: Res<NetRole>,
    query: Query<
        (Entity, &EntityKind, Option<&Faction>, Option<&Transform>),
        (With<EntityKind>, Without<NetworkId>),
    >,
) {
    // Client: don't assign IDs locally — all NetworkIds come from the host
    // via EntitySpawn messages. This prevents ID mismatches between host and client.
    if *net_role == NetRole::Client {
        return;
    }
    let mut pending: Vec<_> = query
        .iter()
        .map(|(entity, kind, faction, transform)| {
            let faction_key = faction.map(faction_sort_key).unwrap_or(u8::MAX);
            let transform_key = transform
                .map(|transform| {
                    (
                        ordered_f32_bits(transform.translation.x),
                        ordered_f32_bits(transform.translation.y),
                        ordered_f32_bits(transform.translation.z),
                    )
                })
                .unwrap_or((u32::MAX, u32::MAX, u32::MAX));
            (
                entity,
                kind_sort_key(*kind),
                faction_key,
                transform_key,
            )
        })
        .collect();

    pending.sort_by_key(|(_, kind_key, faction_key, transform_key)| {
        (*kind_key, *faction_key, *transform_key)
    });

    if !pending.is_empty() {
        info!(
            "NetworkId: assigning {} new IDs (counter will be {}..={})",
            pending.len(),
            counter.0 + 1,
            counter.0 + pending.len() as u32,
        );
    }

    for (entity, _, _, _) in pending {
        let id = counter.next();
        commands.entity(entity).insert(NetworkId(id));
    }
}

// ── System: rebuild_entity_net_map ───────────────────────────────────────────

fn rebuild_entity_net_map(
    mut net_map: ResMut<EntityNetMap>,
    query: Query<(Entity, &NetworkId)>,
) {
    net_map.to_net.clear();
    net_map.to_ecs.clear();
    for (entity, net_id) in &query {
        net_map.to_net.insert(entity, net_id.0);
        net_map.to_ecs.insert(net_id.0, entity);
    }
}

// ── Plugin ───────────────────────────────────────────────────────────────────

pub struct NetBridgePlugin;

impl Plugin for NetBridgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NetworkIdCounter>()
            .init_resource::<EntityNetMap>()
            .add_systems(OnEnter(AppState::InGame), reset_network_identity)
            .add_systems(
                Update,
                (
                    assign_network_ids,
                    rebuild_entity_net_map.after(assign_network_ids),
                )
                    .run_if(in_state(AppState::InGame)),
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
