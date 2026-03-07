use bevy::prelude::*;

use crate::components::*;
use crate::ground::terrain_height;

pub struct PathVisPlugin;

impl Plugin for PathVisPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, create_path_vis_assets)
            .add_systems(
                Update,
                (spawn_path_visualization, animate_path_ring, cleanup_path_vis),
            );
    }
}

fn create_path_vis_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Small diamond-shaped dash for a fantasy breadcrumb trail
    let dash_mesh = meshes.add(Cuboid::new(0.12, 0.02, 0.12));
    let dash_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.85, 0.65, 0.2, 0.6),
        emissive: LinearRgba::new(0.6, 0.4, 0.1, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    // Destination ring — warm golden
    let ring_mesh = meshes.add(Torus::new(0.55, 0.05));
    let ring_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.9, 0.7, 0.15, 0.65),
        emissive: LinearRgba::new(0.7, 0.45, 0.05, 1.0),
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
        (Entity, &Transform, &MoveTarget, Option<&PathVisState>),
        With<Unit>,
    >,
    existing_dashes: Query<(Entity, &PathDash)>,
    existing_rings: Query<(Entity, &PathRing)>,
) {
    for (entity, transform, move_target, vis_state) in &mut units {
        let pos = transform.translation;
        let target = move_target.0;

        // Only rebuild when unit moved enough or target changed
        if let Some(state) = vis_state {
            let pos_moved = Vec2::new(pos.x - state.last_pos.x, pos.z - state.last_pos.z).length();
            let target_moved =
                Vec2::new(target.x - state.target.x, target.z - state.target.z).length();
            if pos_moved < 0.4 && target_moved < 0.1 {
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

        let dir = Vec3::new(target.x - pos.x, 0.0, target.z - pos.z);
        let dist = dir.length();

        if dist < 0.8 {
            commands.entity(entity).insert(PathVisState {
                last_pos: pos,
                target,
            });
            continue;
        }

        let dir_norm = dir.normalize();
        let angle = (-dir_norm.x).atan2(-dir_norm.z);
        // Rotate 45 degrees to make cuboid look diamond-shaped
        let rotation = Quat::from_rotation_y(angle + std::f32::consts::FRAC_PI_4);

        // Dashes from just ahead of unit to just before target
        let start_offset = 0.6;
        let end_margin = 0.6;
        let dash_spacing = 0.5;
        let available = dist - start_offset - end_margin;

        if available > 0.0 {
            let num_dashes = (available / dash_spacing).floor().max(0.0) as usize;
            let num_dashes = num_dashes.min(24);

            for i in 0..num_dashes {
                let t = start_offset + i as f32 * dash_spacing + dash_spacing * 0.5;
                if t >= dist - end_margin {
                    break;
                }

                let px = pos.x + dir_norm.x * t;
                let pz = pos.z + dir_norm.z * t;
                let py = terrain_height(px, pz) + 0.06;

                // Scale up toward the destination for a tapered trail
                let frac = t / dist;
                let scale = 0.65 + 0.35 * frac;

                commands.spawn((
                    PathDash { owner: entity },
                    Mesh3d(path_assets.dash_mesh.clone()),
                    MeshMaterial3d(path_assets.dash_material.clone()),
                    Transform::from_translation(Vec3::new(px, py, pz))
                        .with_rotation(rotation)
                        .with_scale(Vec3::splat(scale)),
                ));
            }
        }

        // Ring at destination
        let ring_y = terrain_height(target.x, target.z) + 0.05;
        commands.spawn((
            PathRing { owner: entity },
            Mesh3d(path_assets.ring_mesh.clone()),
            MeshMaterial3d(path_assets.ring_material.clone()),
            Transform::from_translation(Vec3::new(target.x, ring_y, target.z)),
        ));

        commands.entity(entity).insert(PathVisState {
            last_pos: pos,
            target,
        });
    }
}

/// Gentle pulse + slow rotation on destination rings
fn animate_path_ring(time: Res<Time>, mut rings: Query<&mut Transform, With<PathRing>>) {
    let t = time.elapsed_secs();
    let pulse = 1.0 + 0.12 * (t * 2.5).sin();
    let rot = Quat::from_rotation_y(t * 0.8);

    for mut transform in &mut rings {
        let base_y = transform.translation.y;
        transform.scale = Vec3::splat(pulse);
        transform.rotation = rot;
        transform.translation.y = base_y;
    }
}

fn cleanup_path_vis(
    mut commands: Commands,
    units_without_target: Query<
        Entity,
        (With<Unit>, With<PathVisState>, Without<MoveTarget>),
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
        commands.entity(unit_entity).remove::<PathVisState>();
    }
}
