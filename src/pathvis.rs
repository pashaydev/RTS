use bevy::prelude::*;

use crate::components::*;
use crate::ground::terrain_height;

pub struct PathVisPlugin;

impl Plugin for PathVisPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, create_arrow_assets)
            .add_systems(Update, (spawn_path_arrows, cleanup_path_arrows));
    }
}

fn create_arrow_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Cuboid::new(0.3, 0.02, 0.5));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.9, 0.85, 0.2, 0.7),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    commands.insert_resource(ArrowAssets { mesh, material });
}

fn spawn_path_arrows(
    mut commands: Commands,
    arrow_assets: Res<ArrowAssets>,
    mut units: Query<
        (Entity, &Transform, &MoveTarget, Option<&PreviousMoveTarget>),
        With<Unit>,
    >,
    existing_arrows: Query<(Entity, &PathArrow)>,
) {
    for (entity, transform, move_target, prev) in &mut units {
        // Skip if target hasn't changed
        if let Some(prev_target) = prev {
            if prev_target.0.distance(move_target.0) < 0.1 {
                continue;
            }
        }

        // Despawn old arrows for this unit
        for (arrow_entity, arrow) in &existing_arrows {
            if arrow.owner == entity {
                commands.entity(arrow_entity).despawn();
            }
        }

        let start = transform.translation;
        let end = move_target.0;
        let dir = Vec3::new(end.x - start.x, 0.0, end.z - start.z);
        let dist = dir.length();

        if dist < 0.5 {
            commands.entity(entity).insert(PreviousMoveTarget(move_target.0));
            continue;
        }

        let dir_norm = dir.normalize();
        let num_arrows = 4usize;

        for i in 1..=num_arrows {
            let t = i as f32 / (num_arrows + 1) as f32;
            let ax = start.x + dir.x * t;
            let az = start.z + dir.z * t;
            let pos = Vec3::new(
                ax,
                terrain_height(ax, az) + 0.1,
                az,
            );

            // Rotate to face movement direction
            let angle = (-dir_norm.x).atan2(-dir_norm.z);
            let rotation = Quat::from_rotation_y(angle);

            commands.spawn((
                PathArrow { owner: entity },
                Mesh3d(arrow_assets.mesh.clone()),
                MeshMaterial3d(arrow_assets.material.clone()),
                Transform::from_translation(pos).with_rotation(rotation),
            ));
        }

        commands.entity(entity).insert(PreviousMoveTarget(move_target.0));
    }
}

fn cleanup_path_arrows(
    mut commands: Commands,
    units_without_target: Query<
        Entity,
        (With<Unit>, With<PreviousMoveTarget>, Without<MoveTarget>),
    >,
    arrows: Query<(Entity, &PathArrow)>,
) {
    for unit_entity in &units_without_target {
        for (arrow_entity, arrow) in &arrows {
            if arrow.owner == unit_entity {
                commands.entity(arrow_entity).despawn();
            }
        }
        commands.entity(unit_entity).remove::<PreviousMoveTarget>();
    }
}
