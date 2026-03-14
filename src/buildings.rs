use std::time::Duration;

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::blueprints::{
    spawn_from_blueprint_with_faction, BlueprintRegistry, EntityCategory, EntityKind,
    EntityVisualCache, LevelBonus,
};
use crate::components::*;
use crate::ground::HeightMap;
use crate::model_assets::{BuildingModelAssets, UnitModelAssets};
use bevy_mod_outline::{AsyncSceneInheritOutline, InheritOutline};

pub fn footprint_for_kind(kind: EntityKind) -> f32 {
    match kind {
        EntityKind::Base | EntityKind::Storage => 7.0,
        EntityKind::Gatehouse => 4.0,
        EntityKind::WallSegment | EntityKind::WallPost => 1.6,
        EntityKind::Sawmill | EntityKind::Mine | EntityKind::OilRig => 4.0,
        _ => 2.5,
    }
}

/// Returns the allowed biomes for a building kind, or `None` for default (any non-water).
pub fn allowed_biomes(kind: EntityKind) -> Option<&'static [Biome]> {
    match kind {
        EntityKind::Sawmill => Some(&[Biome::Forest]),
        EntityKind::Mine => Some(&[Biome::Mountain, Biome::Mud]),
        EntityKind::OilRig => Some(&[Biome::Water]),
        _ => None,
    }
}

/// Checks if a building kind can be placed on the given biome.
pub fn is_biome_valid_for(kind: EntityKind, biome: Biome) -> bool {
    match allowed_biomes(kind) {
        Some(allowed) => allowed.contains(&biome),
        None => biome != Biome::Water,
    }
}

/// Returns a human-readable biome requirement hint for placement feedback.
pub fn biome_requirement_text(kind: EntityKind) -> Option<&'static str> {
    match kind {
        EntityKind::Sawmill => Some("Sawmill must be placed on Forest"),
        EntityKind::Mine => Some("Mine must be placed on Mountain or Mud"),
        EntityKind::OilRig => Some("Oil Rig must be placed on Water"),
        _ => Some("Cannot place on Water"),
    }
}

pub struct BuildingsPlugin;

impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildingPlacementState>()
            .init_resource::<WallPlotPreview>()
            .add_systems(Startup, create_ghost_materials)
            .add_systems(
                Update,
                (
                    update_placement_preview,
                    update_wall_plot_preview,
                    update_gate_plot_preview,
                    apply_ghost_materials,
                    confirm_placement,
                    confirm_wall_plot,
                    confirm_gate_plot,
                    cancel_placement,
                    pending_build_arrival_system,
                    pending_build_cleanup_system,
                    construction_progress_system,
                    tower_auto_attack,
                    training_queue_system,
                    update_completed_buildings_tracker,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    building_upgrade_system,
                    demolish_system,
                    building_scale_anim_system,
                    healing_aura_system,
                    level_indicator_system,
                    sync_storage_on_spend,
                    update_storage_piles,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// ── Asset creation (ghost materials only) ──

fn create_ghost_materials(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(BuildingGhostMaterials {
        ghost_valid: materials.add(StandardMaterial {
            base_color: Color::srgba(0.2, 0.8, 0.3, 0.4),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        ghost_invalid: materials.add(StandardMaterial {
            base_color: Color::srgba(0.8, 0.2, 0.2, 0.4),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        under_construction: materials.add(StandardMaterial {
            base_color: Color::srgba(0.7, 0.65, 0.5, 0.6),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
    });
}

// ── Placement preview ──

fn cursor_ground_pos(
    camera_q: &Query<(&Camera, &GlobalTransform)>,
    windows: &Query<&Window, With<PrimaryWindow>>,
) -> Option<Vec3> {
    let Ok(window) = windows.single() else {
        return None;
    };
    let cursor = window.cursor_position()?;
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return None;
    };
    let Ok(ray) = camera.viewport_to_world(cam_gt, cursor) else {
        return None;
    };
    let dist = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
    Some(ray.get_point(dist))
}

const WALL_SEGMENT_LENGTH: f32 = 3.0;

fn wall_layout_points(start: Vec3, end: Vec3) -> Vec<Vec3> {
    let flat_delta = Vec3::new(end.x - start.x, 0.0, end.z - start.z);
    let distance = flat_delta.length();
    if distance < 0.1 {
        return vec![start];
    }

    let steps = (distance / WALL_SEGMENT_LENGTH).round().max(1.0) as usize;
    let dir = flat_delta.normalize();
    let spacing = distance / steps as f32;

    (0..=steps)
        .map(|i| {
            let offset = dir * spacing * i as f32;
            Vec3::new(start.x + offset.x, 0.0, start.z + offset.z)
        })
        .collect()
}

fn wall_cost_from_points(points: &[Vec3]) -> crate::blueprints::ResourceCost {
    let mut total = crate::blueprints::ResourceCost::default();
    if points.len() < 2 {
        return total;
    }

    let segments = (points.len() - 1) as u32;
    let posts = points.len() as u32;
    total.wood = segments * 12 + posts * 16;
    total.copper = segments * 4 + posts * 6;
    total
}

fn clear_wall_preview(commands: &mut Commands, wall_preview: &mut WallPlotPreview) {
    for entity in wall_preview.ghost_entities.drain(..) {
        commands.entity(entity).despawn();
    }
    wall_preview.start = None;
    wall_preview.snapped_points.clear();
    wall_preview.total_cost = crate::blueprints::ResourceCost::default();
    wall_preview.valid = false;
}

fn placement_kind(mode: PlacementMode) -> Option<EntityKind> {
    match mode {
        PlacementMode::Placing(kind) => Some(kind),
        PlacementMode::PlotBase => Some(EntityKind::Base),
        PlacementMode::None | PlacementMode::PlotWall { .. } | PlacementMode::PlotGate => None,
    }
}

fn update_placement_preview(
    mut commands: Commands,
    mut placement: ResMut<BuildingPlacementState>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    ghost_mats: Res<BuildingGhostMaterials>,
    building_models: Option<Res<BuildingModelAssets>>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut ghosts: Query<&mut Transform, With<GhostBuilding>>,
    mut ghost_valid_q: Query<&mut GhostValid, With<GhostBuilding>>,
    existing_buildings: Query<
        (&Transform, &BuildingFootprint),
        (With<Building>, Without<GhostBuilding>),
    >,
    biome_map: Option<Res<BiomeMap>>,
    height_map: Res<HeightMap>,
) {
    let Some(kind) = placement_kind(placement.mode) else {
        return;
    };

    let bp = registry.get(kind);
    let is_gltf = bp.visual.mesh_kind.is_gltf();
    let half_h = if is_gltf {
        0.0
    } else {
        bp.building.as_ref().map(|b| b.half_height).unwrap_or(1.0)
    };
    let new_footprint = footprint_for_kind(kind);

    // Spawn ghost if it doesn't exist
    if placement.preview_entity.is_none() {
        let ghost = if is_gltf {
            // Use actual GLTF building model for the ghost
            let mut ghost_cmds = commands.spawn((
                GhostBuilding,
                GhostValid(true),
                Transform::from_translation(Vec3::new(0.0, -100.0, 0.0)),
                Visibility::default(),
                NotShadowCaster,
                NotShadowReceiver,
            ));
            // Attach the GLTF scene as a child
            if let Some(ref models) = building_models {
                if let Some(scene_handle) = models.scenes.get(&(kind, 1)) {
                    let cal = models.calibration.get(&kind);
                    let scale = cal.map(|c| c.scale).unwrap_or(1.0);
                    let y_off = cal.map(|c| c.y_offset).unwrap_or(0.0);
                    ghost_cmds.with_child((
                        SceneRoot(scene_handle.clone()),
                        Transform::from_scale(Vec3::splat(scale))
                            .with_translation(Vec3::new(0.0, y_off, 0.0)),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
            ghost_cmds.id()
        } else {
            // Non-GLTF: use cache mesh with ghost material directly
            let mesh = cache.meshes.get(&kind).expect("Missing mesh").clone();
            commands
                .spawn((
                    GhostBuilding,
                    GhostValid(true),
                    Mesh3d(mesh),
                    MeshMaterial3d(ghost_mats.ghost_valid.clone()),
                    Transform::from_translation(Vec3::new(0.0, -100.0, 0.0)),
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id()
        };
        placement.preview_entity = Some(ghost);
    }

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows) else {
        return;
    };

    let Some(ghost_entity) = placement.preview_entity else {
        return;
    };
    let Ok(mut ghost_tf) = ghosts.get_mut(ghost_entity) else {
        return;
    };

    let y = height_map.sample(world_pos.x, world_pos.z) + half_h;
    ghost_tf.translation = Vec3::new(world_pos.x, y, world_pos.z);

    let mut valid = true;
    let mut hint: Option<String> = None;

    if let Some(ref bm) = biome_map {
        let biome = bm.get_biome(world_pos.x, world_pos.z);
        if !is_biome_valid_for(kind, biome) {
            valid = false;
            hint = biome_requirement_text(kind).map(ToOwned::to_owned);
        }
    }

    for (building_tf, existing_footprint) in &existing_buildings {
        let min_dist = existing_footprint.0 + new_footprint;
        if building_tf.translation.distance(ghost_tf.translation) < min_dist {
            valid = false;
            break;
        }
    }

    let half_map = 250.0;
    if world_pos.x.abs() > half_map - 5.0 || world_pos.z.abs() > half_map - 5.0 {
        valid = false;
    }

    placement.hint_text = if !valid { hint } else { None };

    if let Ok(mut gv) = ghost_valid_q.get_mut(ghost_entity) {
        gv.0 = valid;
    }
}

fn update_wall_plot_preview(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut wall_preview: ResMut<WallPlotPreview>,
    ghost_mats: Res<BuildingGhostMaterials>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    existing_buildings: Query<
        (&Transform, &BuildingFootprint),
        (With<Building>, Without<GhostBuilding>),
    >,
    height_map: Res<HeightMap>,
) {
    if !matches!(placement.mode, PlacementMode::PlotWall { .. }) {
        if wall_preview.start.is_some() || !wall_preview.ghost_entities.is_empty() {
            clear_wall_preview(&mut commands, &mut wall_preview);
        }
        return;
    }

    clear_wall_preview(&mut commands, &mut wall_preview);

    let start = match placement.mode {
        PlacementMode::PlotWall { start } if start != Vec3::ZERO => start,
        _ => {
            placement.hint_text = Some("Click ground to start wall".to_string());
            return;
        }
    };

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows) else {
        return;
    };
    let points = wall_layout_points(start, Vec3::new(world_pos.x, 0.0, world_pos.z));
    if points.len() < 2 {
        placement.hint_text = Some("Drag farther to plot a wall".to_string());
        return;
    }

    wall_preview.start = Some(start);
    wall_preview.snapped_points = points.clone();
    wall_preview.total_cost = wall_cost_from_points(&points);
    wall_preview.valid = true;

    for point in &points {
        let blocked = existing_buildings.iter().any(|(building_tf, existing_fp)| {
            let check_pos = Vec3::new(point.x, building_tf.translation.y, point.z);
            building_tf.translation.distance(check_pos)
                < existing_fp.0 + footprint_for_kind(EntityKind::WallPost)
        });
        if blocked {
            wall_preview.valid = false;
        }
    }

    for window in points.windows(2) {
        let mid = (window[0] + window[1]) * 0.5;
        let blocked = existing_buildings.iter().any(|(building_tf, existing_fp)| {
            let check_pos = Vec3::new(mid.x, building_tf.translation.y, mid.z);
            building_tf.translation.distance(check_pos)
                < existing_fp.0 + footprint_for_kind(EntityKind::WallSegment)
        });
        if blocked {
            wall_preview.valid = false;
        }
    }

    let post_mesh = meshes.add(Cuboid::new(0.6, 2.0, 0.6));
    let post_mat = if wall_preview.valid {
        ghost_mats.ghost_valid.clone()
    } else {
        ghost_mats.ghost_invalid.clone()
    };
    for point in &points {
        let y = height_map.sample(point.x, point.z);
        let entity = commands
            .spawn((
                GhostBuilding,
                GhostValid(wall_preview.valid),
                Mesh3d(post_mesh.clone()),
                MeshMaterial3d(post_mat.clone()),
                Transform::from_translation(Vec3::new(point.x, y + 1.0, point.z)),
                NotShadowCaster,
                NotShadowReceiver,
            ))
            .id();
        wall_preview.ghost_entities.push(entity);
    }

    let seg_mat = if wall_preview.valid {
        ghost_mats.ghost_valid.clone()
    } else {
        ghost_mats.ghost_invalid.clone()
    };
    for window in points.windows(2) {
        let a = window[0];
        let b = window[1];
        let mid = (a + b) * 0.5;
        let seg_len = a.distance(b).max(0.8);
        let angle = (b.z - a.z).atan2(b.x - a.x);
        let seg_mesh = meshes.add(Cuboid::new(seg_len, 1.1, 0.4));
        let y = height_map.sample(mid.x, mid.z);
        let entity = commands
            .spawn((
                GhostBuilding,
                GhostValid(wall_preview.valid),
                Mesh3d(seg_mesh),
                MeshMaterial3d(seg_mat.clone()),
                Transform::from_translation(Vec3::new(mid.x, y + 0.55, mid.z))
                    .with_rotation(Quat::from_rotation_y(-angle)),
                NotShadowCaster,
                NotShadowReceiver,
            ))
            .id();
        wall_preview.ghost_entities.push(entity);
    }

    let segments = points.len() - 1;
    let posts = points.len();
    let cost = &wall_preview.total_cost;
    placement.hint_text = Some(if wall_preview.valid {
        format!(
            "Wall: {segments} segments, {posts} posts | Cost: {}W {}C",
            cost.wood, cost.copper
        )
    } else {
        "Wall path blocked".to_string()
    });
}

fn update_gate_plot_preview(
    mut commands: Commands,
    mut placement: ResMut<BuildingPlacementState>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    ghost_mats: Res<BuildingGhostMaterials>,
    building_models: Option<Res<BuildingModelAssets>>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut ghosts: Query<&mut Transform, With<GhostBuilding>>,
    mut ghost_valid_q: Query<&mut GhostValid, With<GhostBuilding>>,
    wall_segments: Query<
        (&Transform, &Faction),
        (
            With<WallSegmentPiece>,
            With<Building>,
            Without<GhostBuilding>,
        ),
    >,
    active_player: Res<ActivePlayer>,
) {
    if placement.mode != PlacementMode::PlotGate {
        return;
    }

    let kind = EntityKind::Gatehouse;
    let bp = registry.get(kind);
    let is_gltf = bp.visual.mesh_kind.is_gltf();
    if placement.preview_entity.is_none() {
        let ghost = if is_gltf {
            let mut ghost_cmds = commands.spawn((
                GhostBuilding,
                GhostValid(false),
                Transform::from_translation(Vec3::new(0.0, -100.0, 0.0)),
                Visibility::default(),
                NotShadowCaster,
                NotShadowReceiver,
            ));
            if let Some(ref models) = building_models {
                if let Some(scene_handle) = models.scenes.get(&(kind, 1)) {
                    let cal = models.calibration.get(&kind);
                    let scale = cal.map(|c| c.scale).unwrap_or(1.0);
                    let y_off = cal.map(|c| c.y_offset).unwrap_or(0.0);
                    ghost_cmds.with_child((
                        SceneRoot(scene_handle.clone()),
                        Transform::from_scale(Vec3::splat(scale))
                            .with_translation(Vec3::new(0.0, y_off, 0.0)),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
            ghost_cmds.id()
        } else {
            let mesh = cache.meshes.get(&kind).expect("Missing mesh").clone();
            commands
                .spawn((
                    GhostBuilding,
                    GhostValid(false),
                    Mesh3d(mesh),
                    MeshMaterial3d(ghost_mats.ghost_invalid.clone()),
                    Transform::from_translation(Vec3::new(0.0, -100.0, 0.0)),
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id()
        };
        placement.preview_entity = Some(ghost);
    }

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows) else {
        return;
    };
    let nearest = wall_segments
        .iter()
        .filter(|(_, faction)| **faction == active_player.0)
        .filter_map(|(tf, _)| {
            let d = tf
                .translation
                .distance(Vec3::new(world_pos.x, tf.translation.y, world_pos.z));
            (d <= 6.0).then_some((tf, d))
        })
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let Some(ghost_entity) = placement.preview_entity else {
        return;
    };
    let Ok(mut ghost_tf) = ghosts.get_mut(ghost_entity) else {
        return;
    };

    if let Some((segment_tf, _)) = nearest {
        ghost_tf.translation = segment_tf.translation;
        ghost_tf.rotation = segment_tf.rotation;
        if let Ok(mut gv) = ghost_valid_q.get_mut(ghost_entity) {
            gv.0 = true;
        }
        placement.hint_text = Some("Click to replace wall segment with Gatehouse".to_string());
    } else {
        ghost_tf.translation = Vec3::new(world_pos.x, -100.0, world_pos.z);
        if let Ok(mut gv) = ghost_valid_q.get_mut(ghost_entity) {
            gv.0 = false;
        }
        placement.hint_text = Some("Gatehouse must replace an owned wall segment".to_string());
    }
}

/// Overrides materials on all mesh descendants of ghost buildings to ghost_valid/ghost_invalid.
fn apply_ghost_materials(
    mut commands: Commands,
    ghost_mats: Res<BuildingGhostMaterials>,
    ghosts: Query<(Entity, &GhostValid), With<GhostBuilding>>,
    children_q: Query<&Children>,
    mesh_q: Query<Entity, (With<Mesh3d>, Without<GhostMaterialApplied>)>,
    mut applied_q: Query<
        (Entity, &mut MeshMaterial3d<StandardMaterial>),
        With<GhostMaterialApplied>,
    >,
) {
    for (ghost_entity, ghost_valid) in &ghosts {
        let mat = if ghost_valid.0 {
            ghost_mats.ghost_valid.clone()
        } else {
            ghost_mats.ghost_invalid.clone()
        };

        // Walk all descendants and apply ghost material to mesh entities
        let mut stack = vec![ghost_entity];
        while let Some(entity) = stack.pop() {
            // New mesh entities that haven't been tagged yet
            if mesh_q.get(entity).is_ok() {
                commands.entity(entity).insert((
                    MeshMaterial3d(mat.clone()),
                    GhostMaterialApplied,
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
            // Already-tagged mesh entities: update material if validity changed
            if let Ok((_, mut existing_mat)) = applied_q.get_mut(entity) {
                existing_mat.0 = mat.clone();
            }
            // Recurse into children
            if let Ok(children) = children_q.get(entity) {
                for child in children {
                    stack.push(*child);
                }
            }
        }
    }
}

fn confirm_placement(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    base_state: Res<FactionBaseState>,
    carried_totals: Res<CarriedResourceTotals>,
    mut pending_drains: ResMut<PendingCarriedDrains>,
    registry: Res<BlueprintRegistry>,
    extras: (Res<AllCompletedBuildings>, Option<Res<BiomeMap>>),
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    existing_buildings: Query<
        (&Transform, &BuildingFootprint),
        (With<Building>, Without<GhostBuilding>),
    >,
    workers: Query<(Entity, &Transform, &UnitState, &Faction, &EntityKind), With<Unit>>,
) {
    let (all_completed, biome_map) = extras;
    let mode = placement.mode;
    let Some(kind) = placement_kind(mode) else {
        return;
    };

    let new_footprint = footprint_for_kind(kind);

    // Phase 1: awaiting initial mouse release
    if placement.awaiting_release {
        if mouse.just_released(MouseButton::Left) {
            placement.awaiting_release = false;

            if let Some(world_pos) = cursor_ground_pos(&camera_q, &windows) {
                let bad_biome = biome_map.as_ref().map_or(false, |bm| {
                    !is_biome_valid_for(kind, bm.get_biome(world_pos.x, world_pos.z))
                });
                let too_close = existing_buildings.iter().any(|(building_tf, existing_fp)| {
                    let check_pos = Vec3::new(world_pos.x, building_tf.translation.y, world_pos.z);
                    building_tf.translation.distance(check_pos) < existing_fp.0 + new_footprint
                });
                let half_map = 250.0;
                let out_of_bounds =
                    world_pos.x.abs() > half_map - 5.0 || world_pos.z.abs() > half_map - 5.0;

                if !bad_biome && !too_close && !out_of_bounds {
                    // Valid drag-and-drop
                } else {
                    return;
                }
            } else {
                return;
            }
        } else {
            return;
        }
    } else if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows) else {
        return;
    };

    let bp = registry.get(kind);

    let faction = active_player.0;
    let is_initial_base_plot = matches!(mode, PlacementMode::PlotBase)
        && kind == EntityKind::Base
        && !base_state.is_founded(&faction);

    // Check prerequisite
    let prereq_met = if let Some(ref bd) = bp.building {
        match (is_initial_base_plot, bd.prerequisite) {
            (true, _) => true,
            (false, None) => true,
            (false, Some(prereq_kind)) => all_completed.has(&faction, prereq_kind),
        }
    } else {
        true
    };
    if !prereq_met {
        return;
    }

    // Check biome validity
    if let Some(ref bm) = biome_map {
        if !is_biome_valid_for(kind, bm.get_biome(world_pos.x, world_pos.z)) {
            return;
        }
    }
    for (building_tf, existing_fp) in &existing_buildings {
        let check_pos = Vec3::new(world_pos.x, building_tf.translation.y, world_pos.z);
        if building_tf.translation.distance(check_pos) < existing_fp.0 + new_footprint {
            return;
        }
    }
    let half_map = 250.0;
    if world_pos.x.abs() > half_map - 5.0 || world_pos.z.abs() > half_map - 5.0 {
        return;
    }

    // Find closest available worker (idle, gathering, returning, depositing, waiting)
    let build_pos = Vec3::new(world_pos.x, 0.0, world_pos.z);
    let mut best_worker: Option<(Entity, f32)> = None;
    for (w_entity, w_tf, w_state, w_faction, w_kind) in &workers {
        if *w_kind != EntityKind::Worker || *w_faction != faction {
            continue;
        }
        // Skip workers already assigned to plot or build something, or inside processors
        let available = matches!(
            w_state,
            UnitState::Idle
                | UnitState::Gathering(_)
                | UnitState::ReturningToDeposit { .. }
                | UnitState::Depositing { .. }
                | UnitState::WaitingForStorage { .. }
                | UnitState::Moving(_)
        );
        if !available {
            continue;
        }
        let dist = w_tf.translation.distance(build_pos);
        if best_worker.map_or(true, |(_, best_dist)| dist < best_dist) {
            best_worker = Some((w_entity, dist));
        }
    }

    let Some((worker_entity, _)) = best_worker else {
        // No workers available — show hint and abort
        placement.hint_text = Some("No workers available!".to_string());
        return;
    };

    // Check affordability (stored + carried)
    let player_res = all_resources.get(&faction);
    let carried = carried_totals.get(&faction);
    if !bp.cost.can_afford_with_carried(player_res, carried) {
        return;
    }

    // Deduct from stored first, queue carried drain for any deficit
    let player_res_mut = all_resources.get_mut(&faction);
    let (dw, dc, di, dg, do_) = bp.cost.deduct_with_carried(player_res_mut);
    let drain = SpendFromCarried {
        faction,
        amounts: [dw, dc, di, dg, do_],
    };
    if drain.has_deficit() {
        pending_drains.drains.push(drain);
    }

    // Despawn ghost
    if let Some(ghost) = placement.preview_entity {
        commands.entity(ghost).despawn();
    }

    // Assign worker to move to the build site (building spawns on arrival)
    commands
        .entity(worker_entity)
        .remove::<MoveTarget>()
        .remove::<AttackTarget>()
        .insert(UnitState::MovingToPlot(build_pos))
        .insert(TaskSource::Manual)
        .insert(PendingBuildOrder {
            kind,
            position: build_pos,
            faction,
        })
        .insert(MoveTarget(build_pos));
    // Clear any queued tasks
    commands
        .entity(worker_entity)
        .entry::<TaskQueue>()
        .and_modify(|mut tq| tq.queue.clear());

    // Reset placement
    placement.mode = PlacementMode::None;
    placement.preview_entity = None;
    placement.hint_text = None;
}

fn confirm_wall_plot(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut wall_preview: ResMut<WallPlotPreview>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    height_map: Res<HeightMap>,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    building_models: Option<Res<BuildingModelAssets>>,
    workers: Query<(Entity, &Transform, &UnitState, &Faction, &EntityKind), With<Unit>>,
) {
    if !matches!(placement.mode, PlacementMode::PlotWall { .. }) || placement.awaiting_release {
        return;
    }

    if mouse.just_pressed(MouseButton::Left) {
        if let PlacementMode::PlotWall { start } = placement.mode {
            if start == Vec3::ZERO {
                if let Some(world_pos) = cursor_ground_pos(&camera_q, &windows) {
                    let first = Vec3::new(world_pos.x, 0.0, world_pos.z);
                    wall_preview.start = Some(first);
                    placement.mode = PlacementMode::PlotWall { start: first };
                    placement.hint_text =
                        Some("Move cursor and click again to confirm wall".to_string());
                }
                return;
            }
        }

        if wall_preview.snapped_points.len() < 2 || !wall_preview.valid {
            return;
        }

        let faction = active_player.0;
        let player_res = all_resources.get(&faction);
        if !wall_preview.total_cost.can_afford(player_res) {
            placement.hint_text = Some("Not enough resources for wall".to_string());
            return;
        }
        wall_preview
            .total_cost
            .deduct(all_resources.get_mut(&faction));

        let mut spawned_entities = Vec::new();
        for (idx, point) in wall_preview.snapped_points.iter().enumerate() {
            let kind = if idx == 0 || idx == wall_preview.snapped_points.len() - 1 {
                EntityKind::WallPost
            } else {
                EntityKind::WallPost
            };
            let entity = spawn_from_blueprint_with_faction(
                &mut commands,
                &cache,
                kind,
                *point,
                &registry,
                building_models.as_deref(),
                None,
                &height_map,
                faction,
            );
            commands.entity(entity).insert(WallPostPiece);
            spawned_entities.push(entity);
        }

        for window in wall_preview.snapped_points.windows(2) {
            let a = window[0];
            let b = window[1];
            let mid = (a + b) * 0.5;
            let seg_len = a.distance(b).max(0.8);
            let angle = (b.z - a.z).atan2(b.x - a.x);
            let entity = spawn_from_blueprint_with_faction(
                &mut commands,
                &cache,
                EntityKind::WallSegment,
                mid,
                &registry,
                building_models.as_deref(),
                None,
                &height_map,
                faction,
            );
            commands.entity(entity).insert((
                WallSegmentPiece,
                Transform::from_translation(Vec3::new(
                    mid.x,
                    height_map.sample(mid.x, mid.z),
                    mid.z,
                ))
                .with_rotation(Quat::from_rotation_y(-angle))
                .with_scale(Vec3::new(seg_len.max(1.0), 1.0, 1.0)),
            ));
            spawned_entities.push(entity);
        }

        if let Some(worker_entity) = workers
            .iter()
            .filter(|(_, _, state, worker_faction, kind)| {
                **kind == EntityKind::Worker
                    && **worker_faction == faction
                    && matches!(
                        state,
                        UnitState::Idle
                            | UnitState::Gathering(_)
                            | UnitState::ReturningToDeposit { .. }
                            | UnitState::Depositing { .. }
                            | UnitState::WaitingForStorage { .. }
                            | UnitState::Moving(_)
                    )
            })
            .min_by(|(_, a_tf, _, _, _), (_, b_tf, _, _, _)| {
                let a_dist = a_tf.translation.distance(wall_preview.snapped_points[0]);
                let b_dist = b_tf.translation.distance(wall_preview.snapped_points[0]);
                a_dist
                    .partial_cmp(&b_dist)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(worker, _, _, _, _)| worker)
        {
            let target_building = spawned_entities[0];
            commands
                .entity(worker_entity)
                .remove::<AttackTarget>()
                .remove::<MoveTarget>()
                .insert(UnitState::MovingToBuild(target_building))
                .insert(TaskSource::Manual);
            commands
                .entity(target_building)
                .entry::<AssignedWorkers>()
                .and_modify(move |mut aw| {
                    if !aw.workers.contains(&worker_entity) {
                        aw.workers.push(worker_entity);
                    }
                })
                .or_insert(AssignedWorkers {
                    workers: vec![worker_entity],
                });
        }

        clear_wall_preview(&mut commands, &mut wall_preview);
        placement.mode = PlacementMode::None;
        placement.preview_entity = None;
        placement.hint_text = None;
    }
}

fn confirm_gate_plot(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    height_map: Res<HeightMap>,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    building_models: Option<Res<BuildingModelAssets>>,
    wall_segments: Query<(Entity, &Transform, &Faction), (With<WallSegmentPiece>, With<Building>)>,
    workers: Query<(Entity, &Transform, &UnitState, &Faction, &EntityKind), With<Unit>>,
) {
    if placement.mode != PlacementMode::PlotGate || !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows) else {
        return;
    };

    let Some((segment_entity, segment_tf, faction)) = wall_segments
        .iter()
        .filter(|(_, _, faction)| **faction == active_player.0)
        .filter_map(|(entity, tf, faction)| {
            let d = tf
                .translation
                .distance(Vec3::new(world_pos.x, tf.translation.y, world_pos.z));
            (d <= 6.0).then_some((entity, tf, faction, d))
        })
        .min_by(|(_, _, _, a), (_, _, _, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(e, tf, faction, _)| (e, tf, faction))
    else {
        placement.hint_text = Some("Gatehouse must replace an owned wall segment".to_string());
        return;
    };

    let bp = registry.get(EntityKind::Gatehouse);
    let player_res = all_resources.get(faction);
    if !bp.cost.can_afford(player_res) {
        placement.hint_text = Some("Not enough resources for Gatehouse".to_string());
        return;
    }
    bp.cost.deduct(all_resources.get_mut(faction));

    commands.entity(segment_entity).despawn();
    if let Some(preview) = placement.preview_entity.take() {
        commands.entity(preview).despawn();
    }

    let gate_entity = spawn_from_blueprint_with_faction(
        &mut commands,
        &cache,
        EntityKind::Gatehouse,
        segment_tf.translation,
        &registry,
        building_models.as_deref(),
        None,
        &height_map,
        *faction,
    );
    commands
        .entity(gate_entity)
        .insert(GatePiece)
        .insert(*segment_tf);

    if let Some(worker_entity) = workers
        .iter()
        .filter(|(_, _, state, worker_faction, kind)| {
            **kind == EntityKind::Worker
                && **worker_faction == *faction
                && matches!(
                    state,
                    UnitState::Idle
                        | UnitState::Gathering(_)
                        | UnitState::ReturningToDeposit { .. }
                        | UnitState::Depositing { .. }
                        | UnitState::WaitingForStorage { .. }
                        | UnitState::Moving(_)
                )
        })
        .min_by(|(_, a_tf, _, _, _), (_, b_tf, _, _, _)| {
            let a_dist = a_tf.translation.distance(segment_tf.translation);
            let b_dist = b_tf.translation.distance(segment_tf.translation);
            a_dist
                .partial_cmp(&b_dist)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(worker, _, _, _, _)| worker)
    {
        commands
            .entity(worker_entity)
            .remove::<AttackTarget>()
            .remove::<MoveTarget>()
            .insert(UnitState::MovingToBuild(gate_entity))
            .insert(TaskSource::Manual);
        commands.entity(gate_entity).insert(AssignedWorkers {
            workers: vec![worker_entity],
        });
    }

    placement.mode = PlacementMode::None;
    placement.awaiting_release = false;
    placement.hint_text = None;
}

fn cancel_placement(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut wall_preview: ResMut<WallPlotPreview>,
) {
    if placement.mode == PlacementMode::None {
        return;
    }

    if mouse.just_pressed(MouseButton::Right) || keyboard.just_pressed(KeyCode::Escape) {
        if let Some(preview) = placement.preview_entity {
            commands.entity(preview).despawn();
        }
        clear_wall_preview(&mut commands, &mut wall_preview);
        placement.mode = PlacementMode::None;
        placement.preview_entity = None;
        placement.awaiting_release = false;
        placement.hint_text = None;
    }
}

// ── Worker arrives to plot building ──

fn pending_build_arrival_system(
    mut commands: Commands,
    mut workers: Query<(Entity, &Transform, &UnitState, &PendingBuildOrder), With<Unit>>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    ghost_mats: Res<BuildingGhostMaterials>,
    height_map: Res<HeightMap>,
    building_models: Option<Res<BuildingModelAssets>>,
    existing_buildings: Query<
        (&Transform, &BuildingFootprint),
        (With<Building>, Without<GhostBuilding>),
    >,
    mut all_resources: ResMut<AllPlayerResources>,
) {
    let plot_range = 4.0;

    for (w_entity, w_tf, w_state, pending) in &mut workers {
        // Only act when in MovingToPlot state
        let UnitState::MovingToPlot(_) = *w_state else {
            continue;
        };

        let dist = w_tf.translation.distance(pending.position);
        if dist > plot_range {
            continue; // Still walking
        }

        let kind = pending.kind;
        let faction = pending.faction;
        let build_pos = pending.position;
        let new_footprint = footprint_for_kind(kind);

        // Final collision check — another building may have been placed in the meantime
        let blocked = existing_buildings.iter().any(|(building_tf, existing_fp)| {
            let check_pos = Vec3::new(build_pos.x, building_tf.translation.y, build_pos.z);
            building_tf.translation.distance(check_pos) < existing_fp.0 + new_footprint
        });

        if blocked {
            // Refund resources and cancel
            let bp = registry.get(kind);
            let res = all_resources.get_mut(&faction);
            res.add(ResourceType::Wood, bp.cost.wood);
            res.add(ResourceType::Copper, bp.cost.copper);
            res.add(ResourceType::Iron, bp.cost.iron);
            res.add(ResourceType::Gold, bp.cost.gold);
            res.add(ResourceType::Oil, bp.cost.oil);

            commands
                .entity(w_entity)
                .remove::<PendingBuildOrder>()
                .remove::<MoveTarget>()
                .insert(UnitState::Idle)
                .insert(TaskSource::Auto);
            continue;
        }

        // Spawn the building
        let bp = registry.get(kind);
        let is_gltf = bp.visual.mesh_kind.is_gltf();
        let building_entity = spawn_from_blueprint_with_faction(
            &mut commands,
            &cache,
            kind,
            build_pos,
            &registry,
            building_models.as_deref(),
            None,
            &height_map,
            faction,
        );

        if !is_gltf {
            commands
                .entity(building_entity)
                .insert(MeshMaterial3d(ghost_mats.under_construction.clone()));
        }

        // Transition worker to actively building
        commands
            .entity(w_entity)
            .remove::<PendingBuildOrder>()
            .remove::<MoveTarget>()
            .insert(UnitState::Building(building_entity))
            .insert(TaskSource::Manual);
    }
}

/// If a worker with a PendingBuildOrder dies or is reassigned, refund the building cost.
fn pending_build_cleanup_system(
    mut commands: Commands,
    removed: Query<(Entity, &PendingBuildOrder, &UnitState), With<Unit>>,
    mut all_resources: ResMut<AllPlayerResources>,
    registry: Res<BlueprintRegistry>,
) {
    for (entity, pending, state) in &removed {
        // If the worker is no longer in MovingToPlot state, the order was interrupted
        if !matches!(state, UnitState::MovingToPlot(_)) {
            let bp = registry.get(pending.kind);
            let res = all_resources.get_mut(&pending.faction);
            res.add(ResourceType::Wood, bp.cost.wood);
            res.add(ResourceType::Copper, bp.cost.copper);
            res.add(ResourceType::Iron, bp.cost.iron);
            res.add(ResourceType::Gold, bp.cost.gold);
            res.add(ResourceType::Oil, bp.cost.oil);

            commands.entity(entity).remove::<PendingBuildOrder>();
        }
    }
}

// ── Construction ──

fn construction_progress_system(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    mut base_state: ResMut<FactionBaseState>,
    mut buildings: Query<(
        Entity,
        &EntityKind,
        &mut BuildingState,
        &mut ConstructionProgress,
        &mut Transform,
        &Faction,
    )>,
    workers: Query<&UnitState, With<Unit>>,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
) {
    for (entity, kind, mut state, mut progress, mut transform, faction) in &mut buildings {
        if *state != BuildingState::UnderConstruction {
            continue;
        }

        // Count workers actively building this entity
        let builder_count = workers
            .iter()
            .filter(|state| matches!(state, UnitState::Building(e) if *e == entity))
            .count();

        if builder_count == 0 {
            // No workers assigned — pause and show current scale
            progress.timer.pause();
            let bp = registry.get(*kind);
            let base_scale = bp.visual.scale;
            let fraction = progress.timer.fraction();
            let current_scale = 0.3 * base_scale + (base_scale - 0.3 * base_scale) * fraction;
            transform.scale = Vec3::splat(current_scale);
            continue;
        }

        // Unpause when workers are present
        progress.timer.unpause();
        let speed_mult = 1.0 + 0.5 * (builder_count as f32 - 1.0);

        let bp = registry.get(*kind);
        let base_scale = bp.visual.scale;

        progress
            .timer
            .tick(Duration::from_secs_f32(time.delta_secs() * speed_mult));

        // Lerp scale during construction
        let fraction = progress.timer.fraction();
        let current_scale = 0.3 * base_scale + (base_scale - 0.3 * base_scale) * fraction;
        transform.scale = Vec3::splat(current_scale);

        if progress.timer.is_finished() {
            *state = BuildingState::Complete;
            transform.scale = Vec3::splat(base_scale);

            if *kind == EntityKind::Base {
                base_state.set_founded(*faction, true);
            }

            // Swap to final material (only for non-GLTF buildings)
            let is_gltf = bp.visual.mesh_kind.is_gltf();
            if !is_gltf {
                if let Some(mat) = cache.materials_default.get(kind) {
                    commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                }
            }
            commands
                .entity(entity)
                .remove::<ConstructionProgress>()
                .remove::<ConstructionWorkers>();

            // Add training queue for production buildings
            if let Some(ref bd) = bp.building {
                if !bd.trains.is_empty() {
                    commands.entity(entity).insert(TrainingQueue {
                        queue: vec![],
                        timer: None,
                    });
                }
            }

            // Log construction complete event
            event_log.push(
                time.elapsed_secs(),
                format!("{} construction complete", kind.display_name()),
                crate::ui::event_log_widget::EventCategory::Construction,
                Some(transform.translation),
                Some(*faction),
            );
        }
    }
}

// ── Tower auto-attack ──

fn tower_auto_attack(
    mut commands: Commands,
    time: Res<Time>,
    teams: Res<TeamConfig>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut towers: Query<
        (
            &Transform,
            &EntityKind,
            &BuildingState,
            &mut AttackCooldown,
            &AttackDamage,
            &AttackRange,
            Option<&TowerAutoAttackEnabled>,
            &Faction,
        ),
        With<Building>,
    >,
    hostiles: Query<(Entity, &Transform, &Faction), Or<(With<Mob>, With<Unit>)>>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (tower_tf, kind, state, mut cooldown, damage, range, auto_attack, tower_faction) in
        &mut towers
    {
        if !kind.uses_tower_auto_attack() || *state != BuildingState::Complete {
            continue;
        }

        // Check if auto-attack is disabled
        if let Some(enabled) = auto_attack {
            if !enabled.0 {
                continue;
            }
        }

        cooldown.timer.tick(time.delta());
        if !cooldown.timer.just_finished() {
            continue;
        }

        let mut closest_dist = f32::MAX;
        let mut closest_target = None;
        for (target_entity, target_tf, target_faction) in &hostiles {
            if !teams.is_hostile(tower_faction, target_faction) {
                continue;
            }
            let dist = tower_tf.translation.distance(target_tf.translation);
            if dist < range.0 && dist < closest_dist {
                closest_dist = dist;
                closest_target = Some(target_entity);
            }
        }

        if let Some(target_entity) = closest_target {
            commands.spawn((
                Projectile {
                    target: target_entity,
                    speed: 20.0,
                    damage: damage.0,
                },
                Mesh3d(vfx.sphere_mesh.clone()),
                MeshMaterial3d(vfx.projectile_material.clone()),
                Transform::from_translation(tower_tf.translation + Vec3::Y * 3.0)
                    .with_scale(Vec3::splat(0.2)),
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
    }
}

// ── Training ──

fn training_queue_system(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    unit_models: Option<Res<UnitModelAssets>>,
    height_map: Res<HeightMap>,
    mut buildings: Query<
        (
            &Transform,
            &EntityKind,
            &mut TrainingQueue,
            Option<&RallyPoint>,
            &Faction,
        ),
        With<Building>,
    >,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
) {
    for (transform, _kind, mut queue, rally_point, building_faction) in &mut buildings {
        if queue.queue.is_empty() {
            continue;
        }

        // Start timer for first item if not started
        if queue.timer.is_none() {
            let unit_kind = queue.queue[0];
            let bp = registry.get(unit_kind);
            queue.timer = Some(Timer::from_seconds(bp.train_time_secs, TimerMode::Once));
        }

        if let Some(ref mut timer) = queue.timer {
            timer.tick(time.delta());
            if timer.is_finished() {
                let unit_kind = queue.queue.remove(0);
                let spawn_pos = transform.translation + Vec3::new(3.0, 0.0, 3.0);
                let unit_entity = spawn_from_blueprint_with_faction(
                    &mut commands,
                    &cache,
                    unit_kind,
                    spawn_pos,
                    &registry,
                    None,
                    unit_models.as_deref(),
                    &height_map,
                    *building_faction,
                );

                // Log training complete
                event_log.push(
                    time.elapsed_secs(),
                    format!("{} trained", unit_kind.display_name()),
                    crate::ui::event_log_widget::EventCategory::Training,
                    Some(spawn_pos),
                    Some(*building_faction),
                );

                // If building has a rally point, send the unit there
                if let Some(rally) = rally_point {
                    commands.entity(unit_entity).insert(MoveTarget(rally.0));
                }

                queue.timer = None;
            }
        }
    }
}

// ── Track completed buildings ──

fn update_completed_buildings_tracker(
    mut all_completed: ResMut<AllCompletedBuildings>,
    mut base_state: ResMut<FactionBaseState>,
    buildings: Query<(&EntityKind, &BuildingState, &Faction), With<Building>>,
) {
    let mut per_faction: std::collections::HashMap<Faction, Vec<EntityKind>> =
        std::collections::HashMap::new();
    let mut founded: std::collections::HashMap<Faction, bool> = std::collections::HashMap::new();

    for (kind, state, faction) in &buildings {
        if *state == BuildingState::Complete && kind.category() == EntityCategory::Building {
            let list = per_faction.entry(*faction).or_default();
            if !list.contains(kind) {
                list.push(*kind);
            }
            if *kind == EntityKind::Base {
                founded.insert(*faction, true);
            }
        }
    }

    if all_completed.per_faction != per_faction {
        all_completed.per_faction = per_faction;
    }

    if base_state.founded != founded {
        base_state.founded = founded;
    }
}

// ── Building Upgrade ──

/// Start an upgrade on a building. Returns true if the upgrade was started.
pub fn start_upgrade(
    commands: &mut Commands,
    entity: Entity,
    current_level: u8,
    kind: EntityKind,
    registry: &BlueprintRegistry,
    player_res: &mut PlayerResources,
    faction: Faction,
    carried: &PlayerResources,
    pending_drains: &mut PendingCarriedDrains,
) -> bool {
    // Must be below max level (3)
    if current_level >= 3 {
        return false;
    }

    let bp = registry.get(kind);
    let bd = match bp.building.as_ref() {
        Some(bd) => bd,
        None => return false,
    };

    // level_upgrades is 0-indexed: index 0 = upgrade from L1->L2, index 1 = L2->L3
    let upgrade_index = (current_level - 1) as usize;
    if upgrade_index >= bd.level_upgrades.len() {
        return false;
    }

    let level_data = &bd.level_upgrades[upgrade_index];

    // Check affordability (stored + carried)
    if !level_data.cost.can_afford_with_carried(player_res, carried) {
        return false;
    }

    // Deduct from stored first, queue carried drain for deficit
    let (dw, dc, di, dg, do_) = level_data.cost.deduct_with_carried(player_res);
    let drain = SpendFromCarried {
        faction,
        amounts: [dw, dc, di, dg, do_],
    };
    if drain.has_deficit() {
        pending_drains.drains.push(drain);
    }

    // Insert UpgradeProgress component
    commands.entity(entity).insert(UpgradeProgress {
        timer: Timer::from_seconds(level_data.time_secs, TimerMode::Once),
        target_level: current_level + 1,
    });

    true
}

fn building_upgrade_system(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
    building_models: Option<Res<BuildingModelAssets>>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut buildings: Query<
        (
            Entity,
            &EntityKind,
            &mut BuildingLevel,
            &mut UpgradeProgress,
            &Transform,
            &Faction,
            Option<&mut VisionRange>,
            Option<&mut AttackRange>,
            Option<&mut AttackDamage>,
            Option<&mut StorageInventory>,
            Option<&mut ResourceProcessor>,
            Option<&mut ResourceRespawnConfig>,
        ),
        With<Building>,
    >,
    children_q: Query<&Children>,
    scene_child_q: Query<Entity, With<BuildingSceneChild>>,
) {
    for (
        entity,
        kind,
        mut level,
        mut upgrade,
        transform,
        faction,
        vision,
        attack_range,
        attack_damage,
        storage_inv,
        processor,
        respawn_config,
    ) in &mut buildings
    {
        upgrade.timer.tick(time.delta());

        if !upgrade.timer.is_finished() {
            continue;
        }

        // Upgrade complete
        let new_level = upgrade.target_level;
        level.0 = new_level;

        let bp = registry.get(*kind);
        let bd = match bp.building.as_ref() {
            Some(bd) => bd,
            None => continue,
        };

        // Get the level data for the upgrade that just completed
        let upgrade_index = (new_level - 2) as usize; // L2 = index 0, L3 = index 1
        if upgrade_index >= bd.level_upgrades.len() {
            commands.entity(entity).remove::<UpgradeProgress>();
            continue;
        }

        let level_data = &bd.level_upgrades[upgrade_index];

        // For GLTF buildings: swap scene child to new level's model
        let bp = registry.get(*kind);
        let is_gltf = bp.visual.mesh_kind.is_gltf();
        if is_gltf {
            if let Some(ref models) = building_models {
                if let Some(new_scene) = models.scenes.get(&(*kind, new_level)) {
                    // Despawn old scene child
                    if let Ok(children) = children_q.get(entity) {
                        for child in children.iter() {
                            if scene_child_q.contains(child) {
                                commands.entity(child).try_despawn();
                            }
                        }
                    }
                    // Spawn new scene child with calibration
                    let cal = models.calibration.get(kind);
                    let scale = cal.map(|c| c.scale).unwrap_or(1.0);
                    let y_off = cal.map(|c| c.y_offset).unwrap_or(0.0);
                    let child = commands
                        .spawn((
                            SceneRoot(new_scene.clone()),
                            BuildingSceneChild,
                            InheritOutline,
                            AsyncSceneInheritOutline::default(),
                            Transform::from_scale(Vec3::splat(scale))
                                .with_translation(Vec3::new(0.0, y_off, 0.0)),
                        ))
                        .id();
                    commands.entity(entity).add_child(child);
                }
            }
        }

        // Apply scale multiplier via animation (skip for GLTF — model swap IS the visual feedback)
        if !is_gltf {
            let current_scale = transform.scale;
            let new_scale = current_scale * level_data.scale_multiplier;
            commands.entity(entity).insert(BuildingScaleAnim {
                timer: Timer::from_seconds(0.5, TimerMode::Once),
                from: current_scale,
                to: new_scale,
            });
        }

        // Apply LevelBonus
        match &level_data.bonus {
            LevelBonus::None => {}
            LevelBonus::VisionBoost(boost) => {
                if let Some(mut vr) = vision {
                    vr.0 += boost;
                }
            }
            LevelBonus::TrainTimeMultiplier(_mult) => {
                // Stored on the building; training system reads from blueprint + level
                // No component change needed here — could be enhanced later
            }
            LevelBonus::TrainedStatBoost { .. } => {
                // Affects trained units, not the building itself
            }
            LevelBonus::RangeAndDamage {
                range_boost,
                damage_boost,
            } => {
                if let Some(mut ar) = attack_range {
                    ar.0 += range_boost;
                }
                if let Some(mut ad) = attack_damage {
                    ad.0 += damage_boost;
                }
            }
            LevelBonus::CooldownMultiplier(_mult) => {
                // Could modify AttackCooldown timer duration — skipped for simplicity
            }
            LevelBonus::GatherAura { speed_bonus, range } => {
                commands.entity(entity).insert(StorageAura {
                    gather_speed_bonus: *speed_bonus,
                    range: *range,
                });
            }
            LevelBonus::HealAura {
                heal_per_sec,
                range,
            } => {
                commands.entity(entity).insert(HealingAura {
                    heal_per_sec: *heal_per_sec,
                    range: *range,
                });
            }
            LevelBonus::UnlocksTraining(_kinds) => {
                // Handled at UI level — train button filtering checks building level
            }
            LevelBonus::ProcessorUpgrade {
                harvest_rate_boost,
                radius_boost,
                extra_worker_slots,
                ref unlock_resources,
            } => {
                if let Some(mut proc) = processor {
                    proc.harvest_rate += harvest_rate_boost;
                    proc.harvest_radius += radius_boost;
                    proc.max_workers += extra_worker_slots;
                    for rt in unlock_resources {
                        if !proc.resource_types.contains(rt) {
                            proc.resource_types.push(*rt);
                        }
                    }
                }
                if let Some(mut rc) = respawn_config {
                    for rt in unlock_resources {
                        if !rc.resource_types.contains(rt) {
                            rc.resource_types.push(*rt);
                        }
                    }
                    // Increase max nodes on upgrade
                    rc.max_nodes = (rc.max_nodes + 2).min(12);
                    // Reduce respawn timer slightly
                    let current_secs = rc.respawn_timer.duration().as_secs_f32();
                    rc.respawn_timer =
                        Timer::from_seconds((current_secs * 0.75).max(10.0), TimerMode::Repeating);
                }
            }
        }

        // Scale storage capacities +15% on any upgrade for buildings with storage
        if let Some(mut inv) = storage_inv {
            inv.scale_caps(1.15);
        }

        // Spawn VFX burst (4-6 flash entities in a ring)
        if let Some(ref vfx) = vfx_assets {
            let center = transform.translation;
            let flash_count = 5;
            for i in 0..flash_count {
                let angle = std::f32::consts::TAU * (i as f32 / flash_count as f32);
                let offset = Vec3::new(angle.cos() * 3.0, 2.0, angle.sin() * 3.0);
                commands.spawn((
                    VfxFlash {
                        timer: Timer::from_seconds(0.6, TimerMode::Once),
                        start_scale: 0.8,
                        end_scale: 0.0,
                    },
                    Mesh3d(vfx.sphere_mesh.clone()),
                    MeshMaterial3d(vfx.impact_material.clone()),
                    Transform::from_translation(center + offset).with_scale(Vec3::splat(0.8)),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
        }

        // Log upgrade complete
        event_log.push(
            time.elapsed_secs(),
            format!("{} upgraded to L{}", kind.display_name(), new_level),
            crate::ui::event_log_widget::EventCategory::Upgrade,
            Some(transform.translation),
            Some(*faction),
        );

        // Remove UpgradeProgress
        commands.entity(entity).remove::<UpgradeProgress>();
    }
}

// ── Demolish ──

/// Start the demolish animation on a building.
pub fn start_demolish(commands: &mut Commands, entity: Entity, transform: &Transform) {
    commands.entity(entity).insert(DemolishAnimation {
        timer: Timer::from_seconds(0.5, TimerMode::Once),
        original_scale: transform.scale,
    });
}

fn demolish_system(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
    mut all_resources: ResMut<AllPlayerResources>,
    mut buildings: Query<
        (
            Entity,
            &EntityKind,
            &mut Transform,
            &mut DemolishAnimation,
            &Faction,
        ),
        With<Building>,
    >,
) {
    for (entity, kind, mut transform, mut demolish, faction) in &mut buildings {
        demolish.timer.tick(time.delta());

        let fraction = demolish.timer.fraction();
        // Lerp scale from original to zero
        transform.scale = demolish.original_scale * (1.0 - fraction);

        if demolish.timer.is_finished() {
            // Log demolish event
            event_log.push(
                time.elapsed_secs(),
                format!("{} demolished", kind.display_name()),
                crate::ui::event_log_widget::EventCategory::Demolish,
                Some(transform.translation),
                Some(*faction),
            );

            // Refund 50% of building cost
            let bp = registry.get(*kind);
            let cost = &bp.cost;
            let res = all_resources.get_mut(faction);
            res.add(ResourceType::Wood, cost.wood / 2);
            res.add(ResourceType::Copper, cost.copper / 2);
            res.add(ResourceType::Iron, cost.iron / 2);
            res.add(ResourceType::Gold, cost.gold / 2);
            res.add(ResourceType::Oil, cost.oil / 2);

            // Despawn
            commands.entity(entity).despawn();
        }
    }
}

// ── Building Scale Animation ──

fn building_scale_anim_system(
    mut commands: Commands,
    time: Res<Time>,
    mut buildings: Query<(Entity, &mut Transform, &mut BuildingScaleAnim)>,
) {
    for (entity, mut transform, mut anim) in &mut buildings {
        anim.timer.tick(time.delta());

        let t = anim.timer.fraction();
        // Ease-in-out (smoothstep)
        let eased = t * t * (3.0 - 2.0 * t);
        transform.scale = anim.from.lerp(anim.to, eased);

        if anim.timer.is_finished() {
            transform.scale = anim.to;
            commands.entity(entity).remove::<BuildingScaleAnim>();
        }
    }
}

// ── Aura Systems ──

fn healing_aura_system(
    time: Res<Time>,
    teams: Res<TeamConfig>,
    auras: Query<(&Transform, &HealingAura, &BuildingState, &Faction), With<Building>>,
    mut healable: Query<(&Transform, &mut Health, &Faction), Without<Building>>,
) {
    for (aura_tf, aura, state, aura_faction) in &auras {
        if *state != BuildingState::Complete {
            continue;
        }
        for (unit_tf, mut health, faction) in &mut healable {
            if !teams.is_allied(aura_faction, faction) {
                continue;
            }
            let dist = aura_tf.translation.distance(unit_tf.translation);
            if dist <= aura.range && health.current < health.max {
                health.current =
                    (health.current + aura.heal_per_sec * time.delta_secs()).min(health.max);
            }
        }
    }
}

/// Returns the highest gather speed bonus from any StorageAura in range of the given position.
pub fn storage_aura_bonus(
    worker_pos: Vec3,
    auras: &Query<(&Transform, &StorageAura, &BuildingState), With<Building>>,
) -> f32 {
    let mut bonus = 0.0f32;
    for (aura_tf, aura, state) in auras {
        if *state != BuildingState::Complete {
            continue;
        }
        let dist = aura_tf.translation.distance(worker_pos);
        if dist <= aura.range {
            bonus = bonus.max(aura.gather_speed_bonus); // Don't stack, take highest
        }
    }
    bonus
}

// ── Level Indicator ──

fn level_indicator_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    building_models: Option<Res<BuildingModelAssets>>,
    registry: Res<BlueprintRegistry>,
    buildings: Query<
        (Entity, &BuildingLevel, &Transform, &EntityKind),
        (With<Building>, Changed<BuildingLevel>),
    >,
    existing_indicators: Query<(Entity, &LevelIndicator)>,
) {
    for (building_entity, level, transform, kind) in &buildings {
        if level.0 <= 1 {
            continue;
        }

        // Remove existing indicators for this building
        for (ind_entity, indicator) in &existing_indicators {
            if indicator.building == building_entity {
                commands.entity(ind_entity).try_despawn();
            }
        }

        // Spawn pip spheres above the building
        let pip_count = (level.0 - 1) as usize; // 1 for L2, 2 for L3
        let pip_mesh = meshes.add(Sphere::new(0.2));
        let pip_material = materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.85, 0.2),
            emissive: LinearRgba::new(2.0, 1.7, 0.4, 1.0),
            ..default()
        });

        let bp = registry.get(*kind);
        let base_y = if bp.visual.mesh_kind.is_gltf() {
            let height = building_models
                .as_ref()
                .and_then(|m| m.calibration.get(kind))
                .map(|c| c.building_height)
                .unwrap_or(4.0);
            transform.translation.y + height + 1.0
        } else {
            transform.translation.y + transform.scale.y * 2.0 + 1.0
        };

        for i in 0..pip_count {
            let x_offset = if pip_count == 1 {
                0.0
            } else {
                (i as f32 - (pip_count - 1) as f32 / 2.0) * 0.6
            };

            commands.spawn((
                LevelIndicator {
                    building: building_entity,
                },
                Mesh3d(pip_mesh.clone()),
                MeshMaterial3d(pip_material.clone()),
                Transform::from_translation(Vec3::new(
                    transform.translation.x + x_offset,
                    base_y,
                    transform.translation.z,
                ))
                .with_scale(Vec3::splat(1.0)),
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
    }
}

// ── Sync Storage on Spend ──

fn sync_storage_on_spend(
    all_resources: Res<AllPlayerResources>,
    mut storages: Query<(&Faction, &mut StorageInventory), (With<Building>, With<DepositPoint>)>,
) {
    // For each faction, sum up all storage inventories per resource type.
    // If the total exceeds AllPlayerResources (meaning player spent some),
    // drain from the largest inventory first.
    use std::collections::HashMap;

    // Collect per-faction storage totals
    let mut faction_totals: HashMap<Faction, [u32; ResourceType::COUNT]> = HashMap::new();
    for (faction, inv) in &storages {
        let totals = faction_totals
            .entry(*faction)
            .or_insert([0; ResourceType::COUNT]);
        for rt in ResourceType::ALL {
            totals[rt.index()] += inv.get(rt);
        }
    }

    // For each faction, check if inventories exceed player resources
    for (faction, totals) in &faction_totals {
        let player_res = all_resources.get(faction);
        let mut excess = [0u32; ResourceType::COUNT];
        for rt in ResourceType::ALL {
            excess[rt.index()] = totals[rt.index()].saturating_sub(player_res.get(rt));
        }

        if excess.iter().all(|&e| e == 0) {
            continue;
        }

        // Drain excess from inventories (proportionally)
        let mut remaining = excess;
        for (f, mut inv) in &mut storages {
            if f != faction {
                continue;
            }
            for rt in ResourceType::ALL {
                let i = rt.index();
                let drain = remaining[i].min(inv.get(rt));
                inv.amounts[i] -= drain;
                remaining[i] -= drain;
            }
        }
    }
}

// ── Storage Pile Visuals ──

fn update_storage_piles(
    mut commands: Commands,
    pile_assets: Option<Res<StoragePileAssets>>,
    height_map: Res<HeightMap>,
    mut storages: Query<
        (
            Entity,
            &Transform,
            &mut StorageInventory,
            Option<&ResourcePileVisuals>,
        ),
        (With<Building>, With<DepositPoint>),
    >,
) {
    let Some(assets) = pile_assets else { return };

    for (entity, transform, mut inventory, pile_visuals) in &mut storages {
        let new_total = inventory.total();
        if new_total == inventory.last_total {
            continue;
        }
        inventory.last_total = new_total;

        // Despawn old pile visuals
        if let Some(piles) = pile_visuals {
            for pile_entity in &piles.entities {
                commands.entity(*pile_entity).try_despawn();
            }
        }

        let mut pile_entities = Vec::new();

        // Collect accepted resource types that have items stored
        let accepted = inventory.accepted_types();
        let stored: Vec<ResourceType> = accepted
            .iter()
            .copied()
            .filter(|rt| inventory.get(*rt) > 0)
            .collect();

        if stored.is_empty() {
            commands.entity(entity).insert(ResourcePileVisuals {
                entities: pile_entities,
            });
            continue;
        }

        // Place all piles on one side (East) in an inner grid layout
        let side_offset = 4.0; // distance from building center to pile side
        let grid_spacing = 1.2; // spacing between piles in the grid
        let max_cols = 3;

        for (idx, rt) in stored.iter().enumerate() {
            let amount = inventory.get(*rt);
            let cap = inventory.cap_for(*rt);
            let fill_ratio = (amount as f32 / cap.max(1) as f32).min(1.0);
            let scale = fill_ratio * 0.8 + 0.2;
            let half_pile_height = scale * 0.5;

            // Grid position: row and column within the side
            let col = (idx % max_cols) as f32;
            let row = (idx / max_cols) as f32;
            let grid_width = (stored.len().min(max_cols) as f32 - 1.0) * grid_spacing;
            let local_x = side_offset;
            let local_z = col * grid_spacing - grid_width * 0.5 + row * grid_spacing * 0.5;

            let (mesh, mat) = match rt {
                ResourceType::Wood => (
                    assets.cube_mesh.clone(),
                    assets.materials.get(rt).cloned().unwrap_or_default(),
                ),
                ResourceType::Gold => (
                    assets.sphere_mesh.clone(),
                    assets.materials.get(rt).cloned().unwrap_or_default(),
                ),
                ResourceType::Oil => (
                    assets.cylinder_mesh.clone(),
                    assets.materials.get(rt).cloned().unwrap_or_default(),
                ),
                _ => (
                    assets.cube_mesh.clone(),
                    assets.materials.get(rt).cloned().unwrap_or_default(),
                ),
            };

            let world_x = transform.translation.x + local_x;
            let world_z = transform.translation.z + local_z;
            let ground_y = height_map.sample(world_x, world_z);

            let pile = commands
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(mat),
                    Transform::from_translation(Vec3::new(
                        world_x,
                        ground_y + half_pile_height,
                        world_z,
                    ))
                    .with_scale(Vec3::splat(scale)),
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id();
            pile_entities.push(pile);
        }

        commands.entity(entity).insert(ResourcePileVisuals {
            entities: pile_entities,
        });
    }
}
