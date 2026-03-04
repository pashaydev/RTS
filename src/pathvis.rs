use bevy::prelude::*;

use crate::components::*;
use crate::ground::terrain_height;

pub struct PathVisPlugin;

impl Plugin for PathVisPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, create_path_vis_assets)
            .add_systems(Update, (spawn_path_visualization, cleanup_path_vis));
    }
}

fn create_path_vis_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dash_mesh = meshes.add(Cuboid::new(0.08, 0.015, 0.25));
    let dash_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.4, 0.9, 1.0, 0.45),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    // Flat thin cylinder as destination ring
    let ring_mesh = meshes.add(Torus::new(0.6, 0.04));
    let ring_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.3, 0.85, 1.0, 0.55),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    commands.insert_resource(PathVisAssets {
        dash_mesh,
        dash_material,
        ring_mesh,
        ring_material,
    });
}

fn spawn_path_visualization(
    mut commands: Commands,
    path_assets: Res<PathVisAssets>,
    mut units: Query<
        (Entity, &Transform, &MoveTarget, Option<&PreviousMoveTarget>),
        With<Unit>,
    >,
    existing_dashes: Query<(Entity, &PathDash)>,
    existing_rings: Query<(Entity, &PathRing)>,
) {
    for (entity, transform, move_target, prev) in &mut units {
        // Skip if target hasn't changed
        if let Some(prev_target) = prev {
            if prev_target.0.distance(move_target.0) < 0.1 {
                continue;
            }
        }

        // Despawn old visualization for this unit
        for (e, dash) in &existing_dashes {
            if dash.owner == entity {
                commands.entity(e).despawn();
            }
        }
        for (e, ring) in &existing_rings {
            if ring.owner == entity {
                commands.entity(e).despawn();
            }
        }

        let start = transform.translation;
        let end = move_target.0;
        let dir = Vec3::new(end.x - start.x, 0.0, end.z - start.z);
        let dist = dir.length();

        if dist < 0.5 {
            commands
                .entity(entity)
                .insert(PreviousMoveTarget(move_target.0));
            continue;
        }

        let dir_norm = dir.normalize();
        let angle = (-dir_norm.x).atan2(-dir_norm.z);
        let rotation = Quat::from_rotation_y(angle);

        // Dashes: alternating dash-gap pattern
        let dash_length = 0.25;
        let gap_length = 0.25;
        let stride = dash_length + gap_length;
        let num_dashes = ((dist - 1.0) / stride).floor().max(0.0) as usize;
        let num_dashes = num_dashes.min(20);

        for i in 0..num_dashes {
            let t_offset = 1.0 + i as f32 * stride + dash_length * 0.5;
            if t_offset >= dist - 0.5 {
                break;
            }
            let pos_x = start.x + dir_norm.x * t_offset;
            let pos_z = start.z + dir_norm.z * t_offset;
            let pos_y = terrain_height(pos_x, pos_z) + 0.08;

            commands.spawn((
                PathDash { owner: entity },
                Mesh3d(path_assets.dash_mesh.clone()),
                MeshMaterial3d(path_assets.dash_material.clone()),
                Transform::from_translation(Vec3::new(pos_x, pos_y, pos_z))
                    .with_rotation(rotation),
            ));
        }

        // Ring at destination
        let ring_y = terrain_height(end.x, end.z) + 0.05;
        commands.spawn((
            PathRing { owner: entity },
            Mesh3d(path_assets.ring_mesh.clone()),
            MeshMaterial3d(path_assets.ring_material.clone()),
            Transform::from_translation(Vec3::new(end.x, ring_y, end.z)),
        ));

        commands
            .entity(entity)
            .insert(PreviousMoveTarget(move_target.0));
    }
}

fn cleanup_path_vis(
    mut commands: Commands,
    units_without_target: Query<
        Entity,
        (With<Unit>, With<PreviousMoveTarget>, Without<MoveTarget>),
    >,
    dashes: Query<(Entity, &PathDash)>,
    rings: Query<(Entity, &PathRing)>,
) {
    for unit_entity in &units_without_target {
        for (e, dash) in &dashes {
            if dash.owner == unit_entity {
                commands.entity(e).despawn();
            }
        }
        for (e, ring) in &rings {
            if ring.owner == unit_entity {
                commands.entity(e).despawn();
            }
        }
        commands.entity(unit_entity).remove::<PreviousMoveTarget>();
    }
}
