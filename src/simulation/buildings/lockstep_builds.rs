//! Deterministic drains for wall/gate/floor commands routed through the
//! lockstep input pipeline.
//!
//! `execute_input_command` parses `InputCommand::BuildWall/Gate/Floor`
//! variants and appends them to `PendingLockstepBuilds`. These systems run
//! later in the same FixedUpdate tick, own the grids / meshes / player
//! resources those commands need to mutate, and apply them on every peer.

use bevy::prelude::*;

use crate::blueprints::{BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::infrastructure::net_bridge::EntityNetMap;
use crate::presentation::model_assets::BuildingModelAssets;
use crate::types::*;
use crate::world::ground::{
    apply_terrain_shape_op, foundation_radii, paint_floor_blend_on_ground,
    sync_ground_mesh_partial, HeightMap, TerrainShapeOp, TerrainShapeSyncState,
    TerrainSurfaceDirtyArea, TerrainSurfaceDirtyQueue,
};
use crate::world::pathfinding::NavGridDirty;

use super::{
    add_assigned_worker, cleanup_worker_assignment, find_best_worker_for_build, footprint_for_kind,
    spawn_floor_grid_cells, spawn_wall_grid_cells,
};

pub(super) fn apply_pending_wall_builds(
    mut commands: Commands,
    mut pending: ResMut<PendingLockstepBuilds>,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    building_models: Option<Res<BuildingModelAssets>>,
    height_map: Res<HeightMap>,
    mut wall_grid: ResMut<WallGrid>,
    mut all_resources: ResMut<AllPlayerResources>,
    obstacle_grid: Res<ObstacleGrid>,
    workers: Query<(Entity, &Transform, &UnitState, &Faction, &EntityKind), With<Unit>>,
    net_map: Option<Res<EntityNetMap>>,
) {
    let walls = std::mem::take(&mut pending.walls);
    if walls.is_empty() {
        return;
    }

    for build in walls {
        // Filter cells: drop ones blocked by obstacles or already occupied.
        let cells: Vec<(i32, i32)> = build
            .cells
            .into_iter()
            .filter(|(gx, gz)| !wall_grid.cells.contains_key(&(*gx, *gz)))
            .filter(|(gx, gz)| !obstacle_grid.is_cell_blocked(*gx, *gz))
            .collect();
        if cells.is_empty() {
            continue;
        }

        let total_cost = super::placement::wall_cost_for_cells(&cells, &wall_grid, &registry);
        let player_res = all_resources.get(&build.faction);
        if !total_cost.can_afford(player_res) {
            continue;
        }

        let anchor = WallGrid::grid_to_world(cells[0].0, cells[0].1);
        let worker_iter = workers.iter();
        let Some((worker_entity, _)) = find_best_worker_for_build(
            worker_iter,
            build.faction,
            anchor,
            |entity| {
                net_map
                    .as_deref()
                    .and_then(|nm| nm.to_net.get(&entity).copied())
            },
        ) else {
            continue;
        };

        total_cost.deduct(all_resources.get_mut(&build.faction));

        let spawned = spawn_wall_grid_cells(
            &mut commands,
            &cache,
            &registry,
            building_models.as_deref(),
            &height_map,
            &mut wall_grid,
            build.faction,
            &cells,
        );

        if let Some(&first) = spawned.first() {
            if let Ok((_, _, w_state, _, _)) = workers.get(worker_entity) {
                cleanup_worker_assignment(&mut commands, worker_entity, w_state);
            }
            crate::simulation::combat::reset_combat_state(&mut commands, worker_entity);
            commands
                .entity(worker_entity)
                .remove::<MoveTarget>()
                .insert(UnitState::MovingToBuild(first))
                .insert(TaskSource::Manual);
            add_assigned_worker(&mut commands, first, worker_entity);
        }
    }
}

pub(super) fn apply_pending_gate_builds(
    mut commands: Commands,
    mut pending: ResMut<PendingLockstepBuilds>,
    mut wall_grid: ResMut<WallGrid>,
    mut all_resources: ResMut<AllPlayerResources>,
    registry: Res<BlueprintRegistry>,
    wall_segments: Query<
        (Entity, &Transform, &Faction, &WallGridCoord),
        (With<WallSegmentPiece>, With<Building>),
    >,
    workers: Query<(Entity, &Transform, &UnitState, &Faction, &EntityKind), With<Unit>>,
    net_map: Option<Res<EntityNetMap>>,
) {
    let gates = std::mem::take(&mut pending.gates);
    if gates.is_empty() {
        return;
    }

    for build in gates {
        let (gx, gz) = build.cell;
        let Some((segment_entity, segment_tf, _, _)) = wall_segments
            .iter()
            .find(|(_, _, faction, coord)| **faction == build.faction && coord.0 == gx && coord.1 == gz)
        else {
            continue;
        };

        let bp = registry.get(EntityKind::Gatehouse);
        let player_res = all_resources.get(&build.faction);
        if !bp.cost.can_afford(player_res) {
            continue;
        }

        let worker_iter = workers.iter();
        let Some((worker_entity, _)) = find_best_worker_for_build(
            worker_iter,
            build.faction,
            segment_tf.translation,
            |entity| {
                net_map
                    .as_deref()
                    .and_then(|nm| nm.to_net.get(&entity).copied())
            },
        ) else {
            continue;
        };

        bp.cost.deduct(all_resources.get_mut(&build.faction));

        if let Some(cell) = wall_grid.cells.get_mut(&(gx, gz)) {
            cell.is_gate = true;
        }
        wall_grid.mark_dirty(gx, gz);

        crate::simulation::combat::reset_combat_state(&mut commands, worker_entity);
        commands
            .entity(worker_entity)
            .remove::<MoveTarget>()
            .insert(UnitState::MovingToBuild(segment_entity))
            .insert(TaskSource::Manual);
        add_assigned_worker(&mut commands, segment_entity, worker_entity);
    }
}

pub(super) fn apply_pending_floor_builds(
    mut commands: Commands,
    mut pending: ResMut<PendingLockstepBuilds>,
    mut floor_grid: ResMut<FloorGrid>,
    mut height_map: ResMut<HeightMap>,
    mut all_resources: ResMut<AllPlayerResources>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    obstacle_grid: Res<ObstacleGrid>,
    ground_q: Query<&Mesh3d, With<Ground>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut sync_state: ResMut<TerrainShapeSyncState>,
    mut dirty_areas: ResMut<TerrainSurfaceDirtyQueue>,
    mut nav_dirty: ResMut<NavGridDirty>,
    bush_decorations: Query<(Entity, &Transform), With<Decoration>>,
) {
    let floors = std::mem::take(&mut pending.floors);
    if floors.is_empty() {
        return;
    }

    for build in floors {
        let (gx, gz) = build.cell;
        if floor_grid.cells.contains_key(&(gx, gz)) {
            continue;
        }
        if obstacle_grid.is_cell_blocked(gx, gz) {
            continue;
        }

        let world = WallGrid::grid_to_world(gx, gz);
        let half_map = height_map.half_map;
        if world.x.abs() > half_map - 5.0 || world.z.abs() > half_map - 5.0 {
            continue;
        }

        let cells = vec![(gx, gz)];
        let cell_cost = super::placement::floor_cost_for_cells(&cells, &floor_grid, &registry);
        let player_res = all_resources.get(&build.faction);
        if !cell_cost.can_afford(player_res) {
            continue;
        }
        cell_cost.deduct(all_resources.get_mut(&build.faction));

        let cx = world.x;
        let cz = world.z;
        let footprint = footprint_for_kind(EntityKind::Floor);
        let shared_height = height_map.foundation_target_height_shaped(cx, cz, footprint);
        let op = TerrainShapeOp {
            center: [cx, cz],
            footprint,
            target_height: shared_height,
        };
        let (_, outer_radius) = foundation_radii(op.footprint, height_map.step);

        let changed = apply_terrain_shape_op(&mut height_map, &op);
        sync_state.applied_history.insert(op.clone());
        sync_state.applied_history_ordered.push(op.clone());
        sync_state.pending_network.push(op);

        if changed {
            if let Ok(ground_mesh) = ground_q.single() {
                if let Some(mesh) = meshes.get_mut(&ground_mesh.0) {
                    let op_min_x = (((cx - outer_radius) + half_map) / height_map.step)
                        .floor()
                        .max(0.0) as usize;
                    let op_max_x = (((cx + outer_radius) + half_map) / height_map.step)
                        .ceil()
                        .min((height_map.grid_size - 1) as f32) as usize;
                    let op_min_z = (((cz - outer_radius) + half_map) / height_map.step)
                        .floor()
                        .max(0.0) as usize;
                    let op_max_z = (((cz + outer_radius) + half_map) / height_map.step)
                        .ceil()
                        .min((height_map.grid_size - 1) as f32) as usize;
                    let norm_min_x = op_min_x.saturating_sub(1);
                    let norm_max_x = (op_max_x + 1).min(height_map.grid_size - 1);
                    let norm_min_z = op_min_z.saturating_sub(1);
                    let norm_max_z = (op_max_z + 1).min(height_map.grid_size - 1);
                    sync_ground_mesh_partial(
                        mesh,
                        &height_map,
                        norm_min_x,
                        norm_max_x,
                        norm_min_z,
                        norm_max_z,
                    );
                }
            }
            nav_dirty
                .terrain_updated
                .push((Vec2::new(cx, cz), outer_radius));
        }

        dirty_areas.pending.push_back(TerrainSurfaceDirtyArea {
            center: Vec2::new(cx, cz),
            radius: outer_radius,
        });

        spawn_floor_grid_cells(
            &mut commands,
            &cache,
            &height_map,
            &mut floor_grid,
            build.faction,
            &cells,
            Some(shared_height),
        );

        if let Ok(ground_mesh) = ground_q.single() {
            if let Some(mesh) = meshes.get_mut(&ground_mesh.0) {
                let floor_cell_half = WALL_CELL_SIZE * 0.5;
                let transition = WALL_CELL_SIZE * 0.8;
                paint_floor_blend_on_ground(
                    mesh,
                    &height_map,
                    cx,
                    cz,
                    floor_cell_half,
                    transition,
                );
            }
        }

        let clear_r2 = (footprint + 2.0) * (footprint + 2.0);
        for (deco_entity, deco_tf) in &bush_decorations {
            let dx = deco_tf.translation.x - cx;
            let dz = deco_tf.translation.z - cz;
            if dx * dx + dz * dz <= clear_r2 {
                commands.entity(deco_entity).try_despawn();
            }
        }
    }

}
