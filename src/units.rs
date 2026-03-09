use bevy::prelude::*;

use crate::blueprints::{BlueprintRegistry, EntityKind, EntityVisualCache, spawn_from_blueprint_with_faction};
use crate::components::*;
use crate::ground::HeightMap;
use crate::model_assets::{BuildingModelAssets, UnitModelAssets};

pub struct UnitsPlugin;

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActivePlayer>()
            .init_resource::<AllPlayerResources>()
            .init_resource::<AllCompletedBuildings>()
            .init_resource::<TeamConfig>()
            .add_systems(PostStartup, spawn_all_players)
            .add_systems(
                Update,
                (steer_avoidance, move_units).chain(),
            );
    }
}

pub fn y_offset_for(kind: EntityKind, registry: &BlueprintRegistry) -> f32 {
    let bp = registry.get(kind);
    bp.movement.as_ref().map(|m| m.y_offset).unwrap_or(0.8)
}

fn spawn_all_players(
    mut commands: Commands,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    building_models: Option<Res<BuildingModelAssets>>,
    unit_models: Option<Res<UnitModelAssets>>,
    mut all_completed: ResMut<AllCompletedBuildings>,
    mut all_resources: ResMut<AllPlayerResources>,
    height_map: Res<HeightMap>,
) {
    for &(faction, (sx, sz)) in &SPAWN_POSITIONS {
        let base_pos = Vec3::new(sx, 0.0, sz);
        let base_entity = spawn_from_blueprint_with_faction(
            &mut commands, &cache, EntityKind::Base, base_pos,
            &registry, building_models.as_deref(), None, &height_map, faction,
        );

        // Mark as already complete
        commands.entity(base_entity).remove::<ConstructionProgress>();
        commands.entity(base_entity).insert(BuildingState::Complete);
        commands.entity(base_entity).insert(TrainingQueue {
            queue: vec![],
            timer: None,
        });

        // Register Base as completed for this faction
        let completed = all_completed.per_faction.entry(faction).or_default();
        if !completed.contains(&EntityKind::Base) {
            completed.push(EntityKind::Base);
        }

        // Initialize resources for this faction
        all_resources.resources.insert(faction, PlayerResources::default());

        // Spawn 3 workers near the base
        let worker_offsets = [
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::new(-3.0, 0.0, 2.0),
            Vec3::new(0.0, 0.0, -3.0),
        ];
        for offset in worker_offsets {
            spawn_from_blueprint_with_faction(
                &mut commands, &cache, EntityKind::Worker, base_pos + offset,
                &registry, None, unit_models.as_deref(), &height_map, faction,
            );
        }
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
    height_map: Res<HeightMap>,
    mut query: Query<(Entity, &mut Transform, &MoveTarget, &UnitSpeed, &EntityKind, Option<&Carrying>, Option<&CarryCapacity>), With<Unit>>,
) {
    for (entity, mut transform, target, unit_speed, kind, carrying, capacity) in &mut query {
        let direction = target.0 - transform.translation;
        let flat_dir = Vec3::new(direction.x, 0.0, direction.z);
        let distance = flat_dir.length();

        if distance < 0.2 {
            commands.entity(entity).remove::<MoveTarget>();
        } else {
            // Encumbrance: slow down when carrying heavy loads
            let speed_mult = if let (Some(carry), Some(cap)) = (carrying, capacity) {
                if cap.0 > 0.0 && carry.weight > 0.0 {
                    let load_fraction = (carry.weight / cap.0).min(1.0);
                    1.0 - load_fraction * 0.4 // 40% slower at full load
                } else {
                    1.0
                }
            } else {
                1.0
            };

            let step = flat_dir.normalize() * unit_speed.0 * speed_mult * time.delta_secs();
            transform.translation += step;
        }
        // Snap Y to terrain
        transform.translation.y = height_map.sample(transform.translation.x, transform.translation.z) + y_offset_for(*kind, &registry);
    }
}

