use std::collections::HashMap;

use bevy::ecs::entity::Entities;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::components::*;
use crate::ground::HeightMap;
use crate::pathfinding::NavPath;

pub struct PathVisPlugin;

impl Plugin for PathVisPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, create_path_vis_assets)
            .add_systems(
                Update,
                (
                    spawn_path_visualization,
                    animate_path_dashes,
                    animate_path_ring,
                    cleanup_path_vis,
                    cleanup_orphaned_path_vis,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn create_path_vis_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let dash_mesh = meshes.add(Cuboid::new(0.1, 0.3, 0.1));
    let ring_mesh = meshes.add(Torus::new(0.55, 0.05));
    let crosshair_h_mesh = meshes.add(Cuboid::new(0.9, 0.06, 0.08));
    let crosshair_v_mesh = meshes.add(Cuboid::new(0.08, 0.06, 0.9));

    let mut dash_materials = HashMap::new();
    let mut ring_materials = HashMap::new();

    for cat in PathVisCategory::ALL {
        let (dash_color, dash_emissive, ring_color, ring_emissive) = cat.colors();

        dash_materials.insert(
            cat,
            materials.add(StandardMaterial {
                base_color: dash_color,
                emissive: dash_emissive,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            }),
        );

        ring_materials.insert(
            cat,
            materials.add(StandardMaterial {
                base_color: ring_color,
                emissive: ring_emissive,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            }),
        );
    }

    commands.insert_resource(PathVisAssets {
        dash_mesh,
        ring_mesh,
        crosshair_h_mesh,
        crosshair_v_mesh,
        dash_materials,
        ring_materials,
    });
}

/// Classify a unit's current state into a path visualization category.
fn classify_unit_state(state: &UnitState) -> PathVisCategory {
    match state {
        UnitState::Attacking(_) | UnitState::AttackMoving(_) => PathVisCategory::Attack,
        UnitState::Patrolling { .. } => PathVisCategory::Patrol,
        UnitState::Gathering(_)
        | UnitState::ReturningToDeposit { .. }
        | UnitState::Depositing { .. }
        | UnitState::WaitingForStorage { .. }
        | UnitState::AssignedGathering { .. } => PathVisCategory::Gather,
        UnitState::MovingToPlot(_) | UnitState::MovingToBuild(_) | UnitState::Building(_) => {
            PathVisCategory::Build
        }
        _ => PathVisCategory::Move,
    }
}

/// Resolve the effective target position for path visualization.
/// Returns None if no valid target exists (unit is idle/stationary).
fn resolve_target(
    unit_state: &UnitState,
    move_target: Option<&MoveTarget>,
    attack_target: Option<&AttackTarget>,
    target_transforms: &Query<&Transform, Without<Unit>>,
) -> Option<Vec3> {
    // MoveTarget takes priority when present
    if let Some(mt) = move_target {
        return Some(mt.0);
    }

    // For Attacking state (no MoveTarget), resolve position from AttackTarget entity
    if let UnitState::Attacking(_) = unit_state {
        if let Some(at) = attack_target {
            return target_transforms
                .get(at.0)
                .ok()
                .map(|tf| tf.translation);
        }
    }

    None
}

/// Spawn dashes along a polyline path, returning spawned entity IDs.
fn spawn_dashes_along_path(
    commands: &mut Commands,
    path_assets: &PathVisAssets,
    height_map: &HeightMap,
    owner: Entity,
    trace_points: &[Vec3],
    category: PathVisCategory,
    scale_mult: f32,
) -> Vec<Entity> {
    let total_dist: f32 = trace_points
        .windows(2)
        .map(|w| {
            let d = w[1] - w[0];
            Vec2::new(d.x, d.z).length()
        })
        .sum();

    let mut spawned = Vec::new();

    if total_dist < 0.8 {
        return spawned;
    }

    let pos = trace_points[0];

    let max_dashes: usize = 40;
    let min_spacing = 0.5;
    let start_offset = 0.6;
    let end_margin = 0.6;
    let available = total_dist - start_offset - end_margin;

    let spacing_mult = category.spacing_mult();
    let dash_scale_base = category.dash_scale() * scale_mult;

    let dash_mat = path_assets.dash_materials.get(&category).unwrap().clone();

    if available > 0.0 {
        let dash_spacing = (available / max_dashes as f32).max(min_spacing) * spacing_mult;
        let num_dashes = (available / dash_spacing).floor() as usize;

        for i in 0..num_dashes {
            let t = start_offset + i as f32 * dash_spacing + dash_spacing * 0.5;
            if t >= total_dist - end_margin {
                break;
            }

            // Walk along the polyline to find position at distance t
            let mut remaining = t;
            let mut dash_pos = pos;
            let mut dash_dir = Vec3::Z;
            for w in trace_points.windows(2) {
                let seg = w[1] - w[0];
                let seg_flat = Vec2::new(seg.x, seg.z);
                let seg_len = seg_flat.length();
                if seg_len < 0.01 {
                    continue;
                }
                if remaining <= seg_len {
                    let frac = remaining / seg_len;
                    dash_pos = w[0] + seg * frac;
                    dash_dir = Vec3::new(seg_flat.x, 0.0, seg_flat.y).normalize();
                    break;
                }
                remaining -= seg_len;
            }

            let py = height_map.sample(dash_pos.x, dash_pos.z) + 0.35;
            let angle = (-dash_dir.x).atan2(-dash_dir.z);
            let rotation = Quat::from_rotation_y(angle + std::f32::consts::FRAC_PI_4);
            let frac = t / total_dist;
            let scale = (0.65 + 0.35 * frac) * dash_scale_base;

            let dash_id = commands
                .spawn((
                    PathDash { owner },
                    PathDashMeta {
                        frac,
                        category,
                        base_y: py,
                        base_scale: scale,
                    },
                    Mesh3d(path_assets.dash_mesh.clone()),
                    MeshMaterial3d(dash_mat.clone()),
                    Transform::from_translation(Vec3::new(dash_pos.x, py, dash_pos.z))
                        .with_rotation(rotation)
                        .with_scale(Vec3::splat(scale)),
                    NotShadowCaster,
                ))
                .id();
            spawned.push(dash_id);
        }
    }

    spawned
}

/// Spawn the destination marker (ring or crosshair) and return spawned entity IDs.
fn spawn_destination_marker(
    commands: &mut Commands,
    path_assets: &PathVisAssets,
    height_map: &HeightMap,
    owner: Entity,
    target: Vec3,
    category: PathVisCategory,
    scale: f32,
) -> Vec<Entity> {
    let ring_y = height_map.sample(target.x, target.z) + 0.3;
    let ring_mat = path_assets.ring_materials.get(&category).unwrap().clone();
    let mut spawned = Vec::new();

    match category {
        PathVisCategory::Attack => {
            // Crosshair at destination
            let h = commands
                .spawn((
                    PathRing {
                        owner,
                        category,
                        base_y: ring_y,
                        base_scale: scale,
                    },
                    Mesh3d(path_assets.crosshair_h_mesh.clone()),
                    MeshMaterial3d(ring_mat.clone()),
                    Transform::from_translation(Vec3::new(target.x, ring_y, target.z))
                        .with_scale(Vec3::splat(scale)),
                    NotShadowCaster,
                ))
                .id();
            let v = commands
                .spawn((
                    PathRing {
                        owner,
                        category,
                        base_y: ring_y,
                        base_scale: scale,
                    },
                    Mesh3d(path_assets.crosshair_v_mesh.clone()),
                    MeshMaterial3d(ring_mat),
                    Transform::from_translation(Vec3::new(target.x, ring_y, target.z))
                        .with_scale(Vec3::splat(scale)),
                    NotShadowCaster,
                ))
                .id();
            spawned.push(h);
            spawned.push(v);
        }
        _ => {
            let ring = commands
                .spawn((
                    PathRing {
                        owner,
                        category,
                        base_y: ring_y,
                        base_scale: scale,
                    },
                    Mesh3d(path_assets.ring_mesh.clone()),
                    MeshMaterial3d(ring_mat),
                    Transform::from_translation(Vec3::new(target.x, ring_y, target.z))
                        .with_scale(Vec3::splat(scale)),
                    NotShadowCaster,
                ))
                .id();
            spawned.push(ring);
        }
    }

    spawned
}

fn spawn_path_visualization(
    mut commands: Commands,
    path_assets: Res<PathVisAssets>,
    height_map: Res<HeightMap>,
    active_player: Res<ActivePlayer>,
    mut units: Query<
        (
            Entity,
            &Transform,
            &Faction,
            &UnitState,
            Option<&MoveTarget>,
            Option<&AttackTarget>,
            Option<&mut PathVisEntities>,
            Option<&NavPath>,
        ),
        With<Unit>,
    >,
    target_transforms: Query<&Transform, Without<Unit>>,
) {
    for (entity, transform, faction, unit_state, move_target, attack_target, vis_entities, nav_path) in &mut units {
        if *faction != active_player.0 {
            continue;
        }

        let pos = transform.translation;
        let category = classify_unit_state(unit_state);

        // Resolve target position — may come from MoveTarget or AttackTarget entity
        let Some(target) = resolve_target(unit_state, move_target, attack_target, &target_transforms) else {
            // No valid target — if we had old vis, clean it up
            if let Some(ref state) = vis_entities {
                for &e in &state.entities {
                    if let Ok(mut cmd) = commands.get_entity(e) {
                        cmd.despawn();
                    }
                }
                commands.entity(entity).remove::<PathVisEntities>();
            }
            continue;
        };

        // Only rebuild when unit moved enough, target changed, or category changed
        if let Some(ref state) = vis_entities {
            let pos_moved =
                Vec2::new(pos.x - state.last_pos.x, pos.z - state.last_pos.z).length();
            let target_moved =
                Vec2::new(target.x - state.target.x, target.z - state.target.z).length();
            if pos_moved < 0.4 && target_moved < 0.1 && state.category == category {
                continue;
            }
        }

        // Despawn old visualization
        if let Some(ref state) = vis_entities {
            for &e in &state.entities {
                if let Ok(mut cmd) = commands.get_entity(e) {
                    cmd.despawn();
                }
            }
        }

        // Build trace points: use NavPath waypoints if available, otherwise straight line
        let trace_points: Vec<Vec3> = if let Some(nav) = nav_path {
            let mut pts = vec![pos];
            for i in nav.current_index..nav.waypoints.len() {
                pts.push(nav.waypoints[i]);
            }
            pts
        } else {
            vec![pos, target]
        };

        let total_dist: f32 = trace_points
            .windows(2)
            .map(|w| {
                let d = w[1] - w[0];
                Vec2::new(d.x, d.z).length()
            })
            .sum();

        let mut spawned_entities = Vec::new();

        if total_dist < 0.8 {
            commands.entity(entity).insert(PathVisEntities {
                entities: spawned_entities,
                last_pos: pos,
                target,
                category,
            });
            continue;
        }

        // Spawn dashes along the main path
        spawned_entities.extend(spawn_dashes_along_path(
            &mut commands,
            &path_assets,
            &height_map,
            entity,
            &trace_points,
            category,
            1.0,
        ));

        // Spawn destination marker
        spawned_entities.extend(spawn_destination_marker(
            &mut commands,
            &path_assets,
            &height_map,
            entity,
            target,
            category,
            1.0,
        ));

        // Patrol: spawn return leg (target → origin) with smaller dashes + ring at origin
        if let UnitState::Patrolling { origin, .. } = unit_state {
            let return_trace = vec![target, *origin];
            spawned_entities.extend(spawn_dashes_along_path(
                &mut commands,
                &path_assets,
                &height_map,
                entity,
                &return_trace,
                PathVisCategory::Patrol,
                0.7,
            ));
            spawned_entities.extend(spawn_destination_marker(
                &mut commands,
                &path_assets,
                &height_map,
                entity,
                *origin,
                PathVisCategory::Patrol,
                0.8,
            ));
        }

        commands.entity(entity).insert(PathVisEntities {
            entities: spawned_entities,
            last_pos: pos,
            target,
            category,
        });
    }
}

/// Directional flow animation — sine wave traveling from unit toward destination.
fn animate_path_dashes(
    time: Res<Time>,
    mut dashes: Query<(&PathDashMeta, &mut Transform), With<PathDash>>,
) {
    let t = time.elapsed_secs();

    for (meta, mut transform) in &mut dashes {
        let flow_speed = meta.category.flow_speed();
        let wavelength = 0.3;

        // Sine wave traveling from unit (frac=0) toward target (frac=1)
        let phase = (meta.frac / wavelength - t * flow_speed) * std::f32::consts::TAU;
        let wave = (phase.sin() + 1.0) * 0.5; // 0..1

        // Y-scale pulse: dashes "breathe" as the wave passes
        let y_pulse = 1.0 + 0.35 * wave;
        let xz_scale = meta.base_scale * (1.0 + 0.08 * wave);
        transform.scale = Vec3::new(xz_scale, meta.base_scale * y_pulse, xz_scale);

        // Subtle Y-position bob
        transform.translation.y = meta.base_y + 0.06 * wave;
    }
}

/// Category-aware ring/crosshair animation using stable base_y.
fn animate_path_ring(time: Res<Time>, mut rings: Query<(&PathRing, &mut Transform)>) {
    let t = time.elapsed_secs();

    for (ring, mut transform) in &mut rings {
        let (pulse, rot) = match ring.category {
            PathVisCategory::Attack => {
                // Fast aggressive pulse + rotation
                (
                    1.0 + 0.2 * (t * 4.0).sin(),
                    Quat::from_rotation_y(t * 2.0),
                )
            }
            PathVisCategory::Patrol => {
                // Gentle bounce
                (
                    1.0 + 0.1 * (t * 1.8).sin(),
                    Quat::from_rotation_y(t * 0.5),
                )
            }
            _ => {
                // Standard gentle pulse + slow rotation
                (
                    1.0 + 0.12 * (t * 2.5).sin(),
                    Quat::from_rotation_y(t * 0.8),
                )
            }
        };

        transform.scale = Vec3::splat(pulse * ring.base_scale);
        transform.rotation = rot;
        transform.translation.y = ring.base_y;
    }
}

/// Clean up path vis when a unit has no valid target anymore.
fn cleanup_path_vis(
    mut commands: Commands,
    units_without_target: Query<
        (Entity, &PathVisEntities),
        (With<Unit>, Without<MoveTarget>, Without<AttackTarget>),
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

/// Safety net: despawn orphaned dashes/rings whose owner entity no longer exists.
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
