use bevy::ecs::entity::Entities;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::components::*;
use crate::ground::HeightMap;

pub struct PathVisPlugin;

impl Plugin for PathVisPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, create_path_vis_assets)
            .add_systems(
                Update,
                (
                    spawn_path_visualization,
                    animate_path_ring,
                    cleanup_path_vis,
                    cleanup_orphaned_path_vis,
                ),
            );
    }
}

fn create_path_vis_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Tall thin diamond marker that pokes above terrain on any slope
    let dash_mesh = meshes.add(Cuboid::new(0.1, 0.3, 0.1));
    let dash_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.8, 0.2, 0.8),
        emissive: LinearRgba::new(1.2, 0.8, 0.2, 1.0),
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
    height_map: Res<HeightMap>,
    active_player: Res<ActivePlayer>,
    mut units: Query<
        (Entity, &Transform, &MoveTarget, &Faction, Option<&mut PathVisEntities>),
        With<Unit>,
    >,
) {
    for (entity, transform, move_target, faction, vis_entities) in &mut units {
        // Only show path visualization for the active player's units
        if *faction != active_player.0 {
            continue;
        }
        let pos = transform.translation;
        let target = move_target.0;

        // Only rebuild when unit moved enough or target changed
        if let Some(ref state) = vis_entities {
            let pos_moved = Vec2::new(pos.x - state.last_pos.x, pos.z - state.last_pos.z).length();
            let target_moved =
                Vec2::new(target.x - state.target.x, target.z - state.target.z).length();
            if pos_moved < 0.4 && target_moved < 0.1 {
                continue;
            }
        }

        // Despawn old visualization for this unit via stored entity list
        if let Some(ref state) = vis_entities {
            for &e in &state.entities {
                if let Ok(mut cmd) = commands.get_entity(e) {
                    cmd.despawn();
                }
            }
        }

        let dir = Vec3::new(target.x - pos.x, 0.0, target.z - pos.z);
        let dist = dir.length();

        let mut spawned_entities = Vec::new();

        if dist < 0.8 {
            commands.entity(entity).insert(PathVisEntities {
                entities: spawned_entities,
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
        let available = dist - start_offset - end_margin;
        // Scale spacing so we always get a reasonable density across any distance
        let max_dashes: usize = 40;
        let min_spacing = 0.5;
        let dash_spacing = (available / max_dashes as f32).max(min_spacing);

        if available > 0.0 {
            let num_dashes = (available / dash_spacing).floor() as usize;

            for i in 0..num_dashes {
                let t = start_offset + i as f32 * dash_spacing + dash_spacing * 0.5;
                if t >= dist - end_margin {
                    break;
                }

                let px = pos.x + dir_norm.x * t;
                let pz = pos.z + dir_norm.z * t;
                let py = height_map.sample(px, pz) + 0.35;

                // Scale up toward the destination for a tapered trail
                let frac = t / dist;
                let scale = 0.65 + 0.35 * frac;

                let dash_id = commands
                    .spawn((
                        PathDash { owner: entity },
                        Mesh3d(path_assets.dash_mesh.clone()),
                        MeshMaterial3d(path_assets.dash_material.clone()),
                        Transform::from_translation(Vec3::new(px, py, pz))
                            .with_rotation(rotation)
                            .with_scale(Vec3::splat(scale)),
                        NotShadowCaster,
                    ))
                    .id();
                spawned_entities.push(dash_id);
            }
        }

        // Ring at destination
        let ring_y = height_map.sample(target.x, target.z) + 0.3;
        let ring_id = commands
            .spawn((
                PathRing { owner: entity },
                Mesh3d(path_assets.ring_mesh.clone()),
                MeshMaterial3d(path_assets.ring_material.clone()),
                Transform::from_translation(Vec3::new(target.x, ring_y, target.z)),
                NotShadowCaster,
            ))
            .id();
        spawned_entities.push(ring_id);

        commands.entity(entity).insert(PathVisEntities {
            entities: spawned_entities,
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

/// Clean up path vis when a unit stops moving (no longer has MoveTarget)
fn cleanup_path_vis(
    mut commands: Commands,
    units_without_target: Query<
        (Entity, &PathVisEntities),
        (With<Unit>, Without<MoveTarget>),
    >,
) {
    for (unit_entity, vis) in &units_without_target {
        for &e in &vis.entities {
            if let Ok(mut cmd) = commands.get_entity(e) {
                cmd.despawn();
            }
        }
        commands.entity(unit_entity).remove::<PathVisEntities>();
    }
}

/// Safety net: despawn orphaned dashes/rings whose owner entity no longer exists
fn cleanup_orphaned_path_vis(
    mut commands: Commands,
    dashes: Query<(Entity, &PathDash)>,
    rings: Query<(Entity, &PathRing)>,
    entities: &Entities,
) {
    for (e, dash) in &dashes {
        if !entities.contains(dash.owner) {
            commands.entity(e).despawn();
        }
    }
    for (e, ring) in &rings {
        if !entities.contains(ring.owner) {
            commands.entity(e).despawn();
        }
    }
}
