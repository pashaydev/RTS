//! Wall, floor, and gate grid auto-tiling systems.

use bevy::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
use bevy_mod_outline::{AsyncSceneInheritOutline, InheritOutline};

use crate::blueprints::{spawn_from_blueprint_with_faction, BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::types::*;
use crate::world::ground::HeightMap;
use crate::presentation::model_assets::{BuildingConstructionAssets, BuildingModelAssets};

use super::footprint_for_kind;

pub fn spawn_wall_grid_cells(
    commands: &mut Commands,
    cache: &EntityVisualCache,
    registry: &BlueprintRegistry,
    building_models: Option<&BuildingModelAssets>,
    height_map: &HeightMap,
    wall_grid: &mut WallGrid,
    faction: Faction,
    cells: &[(i32, i32)],
) -> Vec<Entity> {
    let mut spawned_entities = Vec::new();

    // First pass: insert all cells into grid so neighbor lookups work
    // We spawn as WallPost initially; auto-tile system will fix piece kinds
    for &(gx, gz) in cells {
        if wall_grid.cells.contains_key(&(gx, gz)) {
            continue;
        }

        let world = WallGrid::grid_to_world(gx, gz);
        let entity = spawn_from_blueprint_with_faction(
            commands,
            cache,
            EntityKind::WallPost,
            world,
            registry,
            building_models,
            None,
            height_map,
            faction,
        );
        commands
            .entity(entity)
            .insert((WallPostPiece, WallGridCoord(gx, gz)));

        wall_grid.cells.insert(
            (gx, gz),
            WallGridCell {
                entity,
                _faction: faction,
                piece_kind: WallPieceKind::Post,
                is_gate: false,
                rotation_y: 0.0,
            },
        );

        // Mark dirty so auto-tile system picks it up
        wall_grid.mark_dirty(gx, gz);

        spawned_entities.push(entity);
    }

    spawned_entities
}

/// Legacy wrapper: spawn walls from world-space points by snapping to grid.
/// Used by AI and other systems that work with Vec3 positions.
pub fn spawn_wall_line(
    commands: &mut Commands,
    cache: &EntityVisualCache,
    registry: &BlueprintRegistry,
    building_models: Option<&BuildingModelAssets>,
    height_map: &HeightMap,
    wall_grid: &mut WallGrid,
    faction: Faction,
    points: &[Vec3],
) -> Vec<Entity> {
    // Convert points to grid cells
    let mut cells = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for point in points {
        let coord = WallGrid::world_to_grid(*point);
        if seen.insert(coord) {
            cells.push(coord);
        }
    }

    spawn_wall_grid_cells(
        commands,
        cache,
        registry,
        building_models,
        height_map,
        wall_grid,
        faction,
        &cells,
    )
}

pub fn spawn_floor_grid_cells(
    commands: &mut Commands,
    _cache: &EntityVisualCache,
    height_map: &HeightMap,
    floor_grid: &mut FloorGrid,
    faction: Faction,
    cells: &[(i32, i32)],
    shared_height: Option<f32>,
) -> Vec<Entity> {
    let footprint = footprint_for_kind(EntityKind::Floor);
    let mut spawned = Vec::new();

    for &(gx, gz) in cells {
        if floor_grid.cells.contains_key(&(gx, gz)) {
            continue;
        }

        let world = WallGrid::grid_to_world(gx, gz);
        let ground_y = shared_height.unwrap_or_else(|| {
            height_map.foundation_target_height_shaped(world.x, world.z, footprint)
        });
        // Floor tiles are invisible — the terrain blend handles the visuals.
        // Entity is kept for game logic (FloorGrid, FloorTile filtering, etc.)
        let entity = commands
            .spawn((
                GameWorld,
                EntityKind::Floor,
                faction,
                Building,
                FloorTile,
                FloorGridCoord(gx, gz),
                BuildingFootprint(footprint),
                VegetationCleared,
                Transform::from_translation(Vec3::new(world.x, ground_y, world.z)),
                Visibility::Hidden,
            ))
            .id();

        floor_grid.cells.insert(
            (gx, gz),
            FloorGridCell {
                entity,
                _faction: faction,
                piece_kind: FloorPieceKind::Isolated,
                rotation_y: 0.0,
            },
        );
        floor_grid.mark_dirty(gx, gz);
        spawned.push(entity);
    }

    spawned
}

fn floor_piece_and_rotation(neighbor_mask: u8) -> (FloorPieceKind, f32) {
    use std::f32::consts::{FRAC_PI_2, PI};

    let count = neighbor_mask.count_ones();
    let n = neighbor_mask & 1 != 0;
    let e = neighbor_mask & 2 != 0;
    let s = neighbor_mask & 4 != 0;
    let w = neighbor_mask & 8 != 0;

    match count {
        0 => (FloorPieceKind::Isolated, 0.0),
        1 => {
            let rot = if n {
                0.0
            } else if e {
                FRAC_PI_2
            } else if s {
                PI
            } else {
                -FRAC_PI_2
            };
            (FloorPieceKind::End, rot)
        }
        2 => {
            if (n && s) || (e && w) {
                let rot = if e && w { FRAC_PI_2 } else { 0.0 };
                (FloorPieceKind::Straight, rot)
            } else {
                let rot = if n && e {
                    0.0
                } else if e && s {
                    FRAC_PI_2
                } else if s && w {
                    PI
                } else {
                    -FRAC_PI_2
                };
                (FloorPieceKind::Corner, rot)
            }
        }
        3 => {
            let rot = if !s {
                0.0
            } else if !w {
                FRAC_PI_2
            } else if !n {
                PI
            } else {
                -FRAC_PI_2
            };
            (FloorPieceKind::Tee, rot)
        }
        _ => (FloorPieceKind::Cross, 0.0),
    }
}

// ── Wall Auto-Tile Logic ──

/// Determine wall piece kind and rotation from a 4-bit neighbor mask.
/// Bits: 0=North(-Z), 1=East(+X), 2=South(+Z), 3=West(-X).
pub fn auto_tile_piece(neighbor_mask: u8, is_gate: bool) -> (WallPieceKind, f32) {
    use std::f32::consts::{FRAC_PI_2, PI};

    let count = neighbor_mask.count_ones();
    let n = neighbor_mask & 1 != 0;
    let e = neighbor_mask & 2 != 0;
    let s = neighbor_mask & 4 != 0;
    let w = neighbor_mask & 8 != 0;

    if count == 2 {
        // Two opposite neighbors → straight segment
        // Wall_A_wall.glb runs along Z by default, so:
        //   E+W (along X) needs PI/2 rotation to reorient
        //   N+S (along Z) needs 0 rotation (already aligned)
        if (n && s) || (e && w) {
            let rot = if e && w { FRAC_PI_2 } else { 0.0 };
            if is_gate {
                return (WallPieceKind::Gate, rot);
            }
            return (WallPieceKind::Straight, rot);
        }
        // Two adjacent neighbors → corner piece
        // Wall_A_corner.glb default orientation: connects +X and +Z directions
        let rot = if s && e {
            0.0
        } else if s && w {
            FRAC_PI_2
        } else if n && w {
            PI
        } else {
            // n && e
            -FRAC_PI_2
        };
        return (WallPieceKind::Corner, rot);
    }

    // 0, 1, 3, or 4 neighbors → post
    (WallPieceKind::Post, 0.0)
}

/// Map WallPieceKind to the EntityKind used for spawning/model lookup.
pub fn piece_kind_to_entity_kind(pk: WallPieceKind) -> EntityKind {
    match pk {
        WallPieceKind::Post => EntityKind::WallPost,
        WallPieceKind::Straight => EntityKind::WallSegment,
        WallPieceKind::Corner => EntityKind::WallCorner,
        WallPieceKind::Gate => EntityKind::Gatehouse,
    }
}

pub(crate) fn wall_auto_tile_system(
    mut commands: Commands,
    mut wall_grid: ResMut<WallGrid>,
    building_models: Option<Res<BuildingModelAssets>>,
    construction_assets: Option<Res<BuildingConstructionAssets>>,
    children_q: Query<&Children>,
    scene_child_q: Query<Entity, With<BuildingSceneChild>>,
    building_state_q: Query<(&BuildingState, Option<&ConstructionStage>)>,
    transform_q: Query<&Transform>,
) {
    let Some(ref building_models) = building_models else {
        return;
    };

    // Deduplicate dirty cells, then process a limited budget per frame to
    // avoid scene-swap spikes when many wall segments spawn at once.
    const AUTO_TILE_BUDGET: usize = 8;

    let dirty: Vec<(i32, i32)> = wall_grid.dirty.drain(..).collect();
    let mut dirty_set: Vec<(i32, i32)> = {
        let mut set = std::collections::HashSet::new();
        dirty.into_iter().filter(|c| set.insert(*c)).collect()
    };

    // Re-queue cells beyond the budget for the next frame.
    if dirty_set.len() > AUTO_TILE_BUDGET {
        let deferred = dirty_set.split_off(AUTO_TILE_BUDGET);
        wall_grid.dirty.extend(deferred);
    }

    for (gx, gz) in dirty_set {
        let Some(cell) = wall_grid.cells.get(&(gx, gz)).cloned() else {
            continue;
        };

        let mask = wall_grid.neighbor_mask(gx, gz);
        let (new_piece, new_rot) = auto_tile_piece(mask, cell.is_gate);

        if new_piece == cell.piece_kind && (new_rot - cell.rotation_y).abs() < 0.01 {
            continue; // No change needed
        }

        let entity = cell.entity;
        let new_kind = piece_kind_to_entity_kind(new_piece);

        // Update grid cell
        if let Some(cell_mut) = wall_grid.cells.get_mut(&(gx, gz)) {
            cell_mut.piece_kind = new_piece;
            cell_mut.rotation_y = new_rot;
        }

        // Update EntityKind component
        commands.entity(entity).insert(new_kind);

        // Swap marker components
        commands
            .entity(entity)
            .remove::<WallSegmentPiece>()
            .remove::<WallPostPiece>()
            .remove::<WallCornerPiece>()
            .remove::<GatePiece>();
        match new_piece {
            WallPieceKind::Post => {
                commands.entity(entity).insert(WallPostPiece);
            }
            WallPieceKind::Straight => {
                commands.entity(entity).insert(WallSegmentPiece);
            }
            WallPieceKind::Corner => {
                commands.entity(entity).insert(WallCornerPiece);
            }
            WallPieceKind::Gate => {
                commands.entity(entity).insert(GatePiece);
            }
        }

        // Update rotation
        if let Ok(current_tf) = transform_q.get(entity) {
            let mut new_tf = *current_tf;
            new_tf.rotation = Quat::from_rotation_y(new_rot);
            // Reset scale to uniform (no more stretching)
            new_tf.scale = Vec3::ONE;
            commands.entity(entity).insert(new_tf);
        }

        // Swap GLTF scene child to match new piece kind
        // Determine which scene to use based on construction state
        let scene_handle = if let Ok((state, construction_stage)) = building_state_q.get(entity) {
            match state {
                BuildingState::UnderConstruction => {
                    let stage = construction_stage.map(|cs| cs.0).unwrap_or(0);
                    construction_assets
                        .as_ref()
                        .and_then(|ca| ca.stages.get(&(new_kind, stage)).cloned())
                }
                BuildingState::Complete => {
                    let world_pos = transform_q
                        .get(entity)
                        .map(|tf| tf.translation)
                        .unwrap_or_default();
                    building_models.scene_for(new_kind, 1, world_pos)
                }
            }
        } else {
            let world_pos = transform_q
                .get(entity)
                .map(|tf| tf.translation)
                .unwrap_or_default();
            building_models.scene_for(new_kind, 1, world_pos)
        };

        if let Some(scene) = scene_handle {
            // Remove old scene children
            if let Ok(children) = children_q.get(entity) {
                for child in children.iter() {
                    if scene_child_q.contains(child) {
                        commands.entity(child).try_despawn();
                    }
                }
            }

            let mut child = commands.spawn((
                SceneRoot(scene),
                BuildingSceneChild,
                building_models.child_transform(new_kind, 1.0),
            ));
            #[cfg(not(target_arch = "wasm32"))]
            child.insert((InheritOutline, AsyncSceneInheritOutline::default()));
            let child = child.id();
            commands.entity(entity).add_child(child);

            // Clear team color so it gets re-applied
            commands.entity(entity).remove::<TeamColorApplied>();
        }
    }
}

pub(crate) fn floor_auto_tile_system(mut floor_grid: ResMut<FloorGrid>) {
    // Floor tiles have no geometry — just update the grid metadata for neighbor tracking.
    let dirty: Vec<(i32, i32)> = floor_grid.dirty.drain(..).collect();
    let mut dirty_set: Vec<(i32, i32)> = {
        let mut set = std::collections::HashSet::new();
        dirty.into_iter().filter(|c| set.insert(*c)).collect()
    };

    const AUTO_TILE_BUDGET: usize = 64;
    if dirty_set.len() > AUTO_TILE_BUDGET {
        let deferred = dirty_set.split_off(AUTO_TILE_BUDGET);
        floor_grid.dirty.extend(deferred);
    }

    for (gx, gz) in dirty_set {
        let mask = floor_grid.neighbor_mask(gx, gz);
        let (new_piece, new_rot) = floor_piece_and_rotation(mask);

        if let Some(cell) = floor_grid.cells.get_mut(&(gx, gz)) {
            cell.piece_kind = new_piece;
            cell.rotation_y = new_rot;
        }
    }
}
