use bevy::prelude::*;

use crate::blueprints::{BlueprintRegistry, EntityKind, EntityVisualCache, spawn_from_blueprint};
use crate::components::*;
use crate::ground::terrain_height;

pub struct UnitsPlugin;

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_units)
            .add_systems(
                Update,
                (steer_avoidance, move_units, update_unit_visuals).chain(),
            );
    }
}

pub fn y_offset_for(kind: EntityKind, registry: &BlueprintRegistry) -> f32 {
    let bp = registry.get(kind);
    bp.movement.as_ref().map(|m| m.y_offset).unwrap_or(0.8)
}

fn spawn_units(
    mut commands: Commands,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
) {
    let worker_positions = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(2.0, 0.0, 1.0),
    ];
    for pos in worker_positions {
        spawn_from_blueprint(&mut commands, &cache, EntityKind::Worker, pos, &registry);
    }
}

fn steer_avoidance(
    time: Res<Time>,
    mut units: Query<(Entity, &mut Transform), With<Unit>>,
) {
    let avoidance_radius = 1.8;
    let strength = 4.0;

    let positions: Vec<(Entity, Vec3)> = units
        .iter()
        .map(|(e, t)| (e, t.translation))
        .collect();

    for (entity, mut transform) in &mut units {
        let my_pos = transform.translation;
        let mut separation = Vec3::ZERO;

        for (other_e, other_pos) in &positions {
            if *other_e == entity {
                continue;
            }
            let diff = my_pos - *other_pos;
            let flat_diff = Vec3::new(diff.x, 0.0, diff.z);
            let dist = flat_diff.length();
            if dist > 0.01 && dist < avoidance_radius {
                separation += flat_diff.normalize() * (avoidance_radius - dist) / avoidance_radius;
            }
        }

        if separation.length_squared() > 0.0 {
            transform.translation += separation * strength * time.delta_secs();
        }
    }
}

fn move_units(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    mut query: Query<(Entity, &mut Transform, &MoveTarget, &UnitSpeed, &EntityKind), With<Unit>>,
) {
    for (entity, mut transform, target, unit_speed, kind) in &mut query {
        let direction = target.0 - transform.translation;
        let flat_dir = Vec3::new(direction.x, 0.0, direction.z);
        let distance = flat_dir.length();

        if distance < 0.2 {
            commands.entity(entity).remove::<MoveTarget>();
        } else {
            let step = flat_dir.normalize() * unit_speed.0 * time.delta_secs();
            transform.translation += step;
        }
        // Snap Y to terrain
        transform.translation.y = terrain_height(transform.translation.x, transform.translation.z) + y_offset_for(*kind, &registry);
    }
}

fn update_unit_visuals(
    mut commands: Commands,
    cache: Res<EntityVisualCache>,
    added: Query<(Entity, &EntityKind), (With<Unit>, Added<Selected>)>,
    mut removed: RemovedComponents<Selected>,
    units: Query<(Entity, &EntityKind), With<Unit>>,
) {
    for (entity, kind) in &added {
        if let Some(mat) = cache.materials_selected.get(kind) {
            commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
        }
    }

    for entity in removed.read() {
        if let Ok((_, kind)) = units.get(entity) {
            if let Some(mat) = cache.materials_default.get(kind) {
                commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
            }
        }
    }
}
