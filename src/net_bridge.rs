//! ECS network identity bridge — assigns stable network IDs to entities.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::EntityKind;
use crate::components::AppState;

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
    query: Query<Entity, (With<EntityKind>, Without<NetworkId>)>,
) {
    for entity in &query {
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
