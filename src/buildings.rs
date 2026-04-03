use std::time::Duration;

use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::audio::{PlaySfx, SfxKind};
use crate::blueprints::{
    spawn_from_blueprint_with_faction, BlueprintRegistry, EntityCategory, EntityKind,
    EntityVisualCache, LevelBonus,
};
use crate::camera;
use crate::components::*;
use crate::ground::{
    apply_terrain_shape_op, foundation_radii, sync_ground_mesh_partial, BorderSettings, HeightMap,
    TerrainSurfaceDirtyArea, TerrainSurfaceDirtyQueue,
};
use game_state::message::TerrainShapeOp;
use crate::model_assets::{BuildingConstructionAssets, BuildingModelAssets, UnitModelAssets};
#[cfg(not(target_arch = "wasm32"))]
use bevy_mod_outline::{AsyncSceneInheritOutline, InheritOutline};

#[derive(SystemParam)]
struct PlacementOnlineParams<'w> {
    net_role: Res<'w, crate::multiplayer::NetRole>,
    client_state: Option<Res<'w, crate::multiplayer::ClientNetState>>,
    matchbox_socket: Option<ResMut<'w, bevy_matchbox::prelude::MatchboxSocket>>,
    time: Res<'w, Time>,
}

/// Worker availability priority for building placement.
/// Lower number = preferred (idle workers are picked before assigned ones).
/// `build_counts` tracks how many workers target each construction site — extras are stealable.
fn worker_availability_priority(
    state: &UnitState,
    build_counts: &std::collections::HashMap<Entity, u32>,
) -> Option<u8> {
    match state {
        UnitState::Idle | UnitState::Moving(_) => Some(0),
        UnitState::Gathering(_)
        | UnitState::ReturningToDeposit { .. }
        | UnitState::Depositing { .. }
        | UnitState::WaitingForStorage { .. } => Some(1),
        UnitState::AssignedGathering { .. } => Some(2),
        // Extra builders on a construction site can be reassigned (keep at least 1)
        UnitState::MovingToBuild(building) | UnitState::Building(building) => {
            let count = build_counts.get(building).copied().unwrap_or(1);
            if count > 1 {
                Some(3) // stealable, but lowest priority
            } else {
                None // sole builder — never steal
            }
        }
        _ => None,
    }
}

/// Find the best available worker for a build task at `build_pos`.
/// Returns `(entity, priority)` — the closest worker at the best (lowest) priority tier.
/// Workers building/moving-to-build are stealable only if their target building has >1 worker.
fn find_best_worker_for_build<'a>(
    workers: impl Iterator<
        Item = (
            Entity,
            &'a Transform,
            &'a UnitState,
            &'a Faction,
            &'a EntityKind,
        ),
    >,
    faction: Faction,
    build_pos: Vec3,
) -> Option<(Entity, u8)> {
    // First pass: collect faction workers and count builders per construction site
    let mut faction_workers: Vec<(Entity, Vec3, &UnitState)> = Vec::new();
    let mut build_counts: std::collections::HashMap<Entity, u32> =
        std::collections::HashMap::new();

    for (w_entity, w_tf, w_state, w_faction, w_kind) in workers {
        if *w_kind != EntityKind::Worker || *w_faction != faction {
            continue;
        }
        faction_workers.push((w_entity, w_tf.translation, w_state));
        match w_state {
            UnitState::MovingToBuild(building) | UnitState::Building(building) => {
                *build_counts.entry(*building).or_insert(0) += 1;
            }
            _ => {}
        }
    }

    // Second pass: pick the best candidate
    let mut best: Option<(Entity, u8, f32)> = None;
    for &(w_entity, w_pos, w_state) in &faction_workers {
        let Some(prio) = worker_availability_priority(w_state, &build_counts) else {
            continue;
        };
        let dist = w_pos.distance(build_pos);
        let dominated = best.map_or(false, |(_, best_prio, best_dist)| {
            prio > best_prio || (prio == best_prio && dist >= best_dist)
        });
        if !dominated {
            best = Some((w_entity, prio, dist));
        }
    }
    best.map(|(e, prio, _)| (e, prio))
}

fn has_available_worker_for_build<'a>(
    workers: impl Iterator<
        Item = (
            Entity,
            &'a Transform,
            &'a UnitState,
            &'a Faction,
            &'a EntityKind,
        ),
    >,
    faction: Faction,
    build_pos: Vec3,
) -> bool {
    find_best_worker_for_build(workers, faction, build_pos).is_some()
}

/// When reassigning a worker away from their current task, clean up the old building's
/// worker list and remove `BuildingAssignment`.
fn cleanup_worker_assignment(commands: &mut Commands, worker: Entity, state: &UnitState) {
    match state {
        UnitState::AssignedGathering { building, .. }
        | UnitState::MovingToBuild(building)
        | UnitState::Building(building) => {
            let building_entity = *building;
            commands
                .entity(building_entity)
                .entry::<AssignedWorkers>()
                .and_modify(move |mut aw| {
                    aw.workers.retain(|w| *w != worker);
                });
        }
        _ => {}
    }
    commands.entity(worker).remove::<BuildingAssignment>();
}

/// Spawn wall entities at grid cells and register them in the WallGrid.
/// Each cell gets auto-tiled based on its neighbors.
/// Returns all spawned entities.
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

pub fn footprint_for_kind(kind: EntityKind) -> f32 {
    // Based on actual GLTF bounding boxes (scaled) + ~0.5 margin
    match kind {
        EntityKind::Outpost => 4.5,           // BeastLair: raw 3.93 @ 1.0
        EntityKind::Base | EntityKind::Smelter => 3.5, // Castle: 3.02, Keep: 3.11 @ 0.75
        EntityKind::Barracks
        | EntityKind::Workshop
        | EntityKind::MageTower
        | EntityKind::Temple
        | EntityKind::Stable
        | EntityKind::SiegeWorks
        | EntityKind::Sawmill
        | EntityKind::Mine
        | EntityKind::OilRig
        | EntityKind::Alchemist
        | EntityKind::Gatehouse => 3.0,       // ~2.5 radius @ 0.75
        EntityKind::Tower
        | EntityKind::WatchTower
        | EntityKind::GuardTower
        | EntityKind::BallistaTower
        | EntityKind::BombardTower
        | EntityKind::Storage
        | EntityKind::House => 2.0,           // towers ~1.5, Granary 1.6, House 1.7 @ 0.75
        EntityKind::Floor => 2.2,
        EntityKind::WallSegment | EntityKind::WallPost | EntityKind::WallCorner => 1.5,
        _ => 2.5,
    }
}

pub fn building_height_for_kind(kind: EntityKind) -> f32 {
    match kind {
        EntityKind::Tower
        | EntityKind::WatchTower
        | EntityKind::GuardTower
        | EntityKind::BallistaTower
        | EntityKind::BombardTower => 10.0,
        EntityKind::Outpost => 8.0,
        EntityKind::Base | EntityKind::Smelter => 7.0,
        EntityKind::Barracks
        | EntityKind::Workshop
        | EntityKind::MageTower
        | EntityKind::Temple
        | EntityKind::Stable
        | EntityKind::SiegeWorks
        | EntityKind::Sawmill
        | EntityKind::Mine
        | EntityKind::OilRig
        | EntityKind::Alchemist
        | EntityKind::Gatehouse => 6.0,
        EntityKind::House | EntityKind::Storage => 5.0,
        EntityKind::WallSegment | EntityKind::WallPost | EntityKind::WallCorner => 4.0,
        EntityKind::Floor => 0.5,
        _ => 5.0,
    }
}

pub fn is_wall_like_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::WallSegment
            | EntityKind::WallPost
            | EntityKind::WallCorner
            | EntityKind::Gatehouse
    )
}

pub fn is_floor_kind(kind: EntityKind) -> bool {
    kind == EntityKind::Floor
}

fn blocks_construction_overlap(kind: EntityKind) -> bool {
    !is_floor_kind(kind)
}

pub fn uses_terrain_foundation(kind: EntityKind) -> bool {
    !matches!(
        kind,
        EntityKind::WallSegment
            | EntityKind::WallPost
            | EntityKind::WallCorner
            | EntityKind::OilRig
            | EntityKind::Floor // floor batch-flattens in confirm_floor_plot instead
    )
}

/// Returns the allowed biomes for a building kind, or `None` for default (any non-water).
pub fn allowed_biomes(kind: EntityKind) -> Option<&'static [Biome]> {
    match kind {
        EntityKind::Sawmill => Some(&[Biome::Forest, Biome::Grassland]),
        EntityKind::Mine => Some(&[Biome::Mountain, Biome::Wetland, Biome::Desert]),
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
        EntityKind::Sawmill => Some("Sawmill must be placed on Forest or Grassland"),
        EntityKind::Mine => Some("Mine must be placed on Mountain, Wetland, or Desert"),
        EntityKind::OilRig => Some("Oil Rig must be placed on Water"),
        _ => Some("Cannot place on Water"),
    }
}

pub fn try_queue_build_order_authoritative(
    commands: &mut Commands,
    kind: EntityKind,
    build_pos: Vec3,
    faction: Faction,
    all_resources: &mut AllPlayerResources,
    base_state: &FactionBaseState,
    carried_totals: &CarriedResourceTotals,
    pending_drains: &mut PendingCarriedDrains,
    registry: &BlueprintRegistry,
    all_completed: &AllCompletedBuildings,
    biome_map: Option<&BiomeMap>,
    faction_ages: &crate::ages::FactionAges,
    height_map: &HeightMap,
    existing_buildings: &Query<
        (&Transform, &BuildingFootprint, &Faction, &EntityKind),
        (With<Building>, Without<GhostBuilding>),
    >,
    workers: &Query<
        (
            Entity,
            &Transform,
            &UnitState,
            &Faction,
            &EntityKind,
            Option<&PendingBuildOrder>,
        ),
        With<Unit>,
    >,
    obstacle_grid: &ObstacleGrid,
) -> Result<(), String> {
    if matches!(
        kind,
        EntityKind::WallSegment
            | EntityKind::WallPost
            | EntityKind::WallCorner
            | EntityKind::Gatehouse
            | EntityKind::Floor
    ) {
        return Err("This building uses a specialized placement flow.".to_string());
    }

    let bp = registry.get(kind);
    let new_footprint = footprint_for_kind(kind);
    let has_base_started = base_state.is_founded(&faction)
        || existing_buildings
            .iter()
            .any(|(_, _, building_faction, building_kind)| {
                *building_faction == faction && *building_kind == EntityKind::Base
            })
        || workers
            .iter()
            .any(|(_, _, _, worker_faction, _, pending_order)| {
                *worker_faction == faction
                    && pending_order.is_some_and(|order| order.kind == EntityKind::Base)
            });

    if kind == EntityKind::Base && has_base_started {
        return Err("Base is already being founded.".to_string());
    }

    let prereq_met = if let Some(ref bd) = bp.building {
        match bd.prerequisite {
            None => true,
            Some(prereq_kind) => {
                if prereq_kind == EntityKind::Base {
                    base_state.is_founded(&faction) || all_completed.has(&faction, prereq_kind)
                } else {
                    all_completed.has(&faction, prereq_kind)
                }
            }
        }
    } else {
        true
    };
    if !prereq_met {
        return Err("Prerequisite not met.".to_string());
    }

    let required_age = crate::ages::required_age_for_building(kind);
    let current_age = faction_ages.get_age(&faction);
    if current_age < required_age {
        return Err(format!("Requires {}", required_age.display_name()));
    }

    if let Some(biome_map) = biome_map {
        if !is_biome_valid_for(kind, biome_map.get_biome(build_pos.x, build_pos.z)) {
            return Err(biome_requirement_text(kind)
                .unwrap_or("Invalid biome for building placement")
                .to_string());
        }
    }

    if !matches!(kind, EntityKind::WallSegment | EntityKind::WallPost | EntityKind::WallCorner) {
        const MAX_BUILDING_SLOPE: f32 = 0.5;
        let slope = height_map.max_slope_under_footprint(build_pos.x, build_pos.z, new_footprint);
        if slope > MAX_BUILDING_SLOPE {
            return Err("Ground is too steep here.".to_string());
        }
    }

    for (building_tf, existing_fp, _, existing_kind) in existing_buildings {
        if !blocks_construction_overlap(*existing_kind) {
            continue;
        }
        let dx = building_tf.translation.x - build_pos.x;
        let dz = building_tf.translation.z - build_pos.z;
        if (dx * dx + dz * dz).sqrt() < existing_fp.0 + new_footprint {
            return Err("Building footprint is blocked.".to_string());
        }
    }

    if obstacle_grid.is_footprint_blocked(build_pos, new_footprint) {
        return Err("Blocked by trees.".to_string());
    }

    let half_map = height_map.half_map;
    if build_pos.x.abs() > half_map - 5.0 || build_pos.z.abs() > half_map - 5.0 {
        return Err("Too close to the edge of the map.".to_string());
    }

    // Find best worker using priority-based selection (includes processor-assigned workers)
    let worker_iter = workers.iter().map(|(e, tf, state, fac, kind, _)| (e, tf, state, fac, kind));
    let Some((worker_entity, _worker_prio)) = find_best_worker_for_build(worker_iter, faction, build_pos) else {
        return Err("No workers available!".to_string());
    };

    let player_res = all_resources.get(&faction);
    let carried = carried_totals.get(&faction);
    if !bp.cost.can_afford_with_carried(player_res, carried) {
        return Err("Not enough resources.".to_string());
    }

    let player_res_mut = all_resources.get_mut(&faction);
    let deficits = bp.cost.deduct_with_carried(player_res_mut);
    let drain = SpendFromCarried {
        faction,
        amounts: deficits,
    };
    if drain.has_deficit() {
        pending_drains.drains.push(drain);
    }

    // Clean up any existing gathering assignment before reassigning
    if let Ok((_, _, w_state, _, _, _)) = workers.get(worker_entity) {
        cleanup_worker_assignment(commands, worker_entity, w_state);
    }

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
            rotation_y: 0.0,
        })
        .insert(MoveTarget(build_pos));
    commands
        .entity(worker_entity)
        .entry::<TaskQueue>()
        .and_modify(|mut tq| tq.queue.clear());

    Ok(())
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

fn wall_auto_tile_system(
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

            let mut child = commands
                .spawn((
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

fn floor_auto_tile_system(
    mut floor_grid: ResMut<FloorGrid>,
) {
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

pub struct BuildingsPlugin;

impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildingPlacementState>()
            .init_resource::<WallPlotPreview>()
            .init_resource::<FloorPlotPreview>()
            .init_resource::<WallGrid>()
            .init_resource::<FloorGrid>()
            .init_resource::<ObstacleGrid>()
            .add_systems(Startup, create_ghost_materials)
            .add_systems(
                Update,
                sync_obstacle_grid
                    .in_set(GameFlowSet::Simulation)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    update_placement_preview,
                    update_wall_plot_preview,
                    update_gate_plot_preview,
                    update_floor_plot_preview,
                    apply_ghost_materials,
                    animate_placement_preview_vfx,
                    confirm_placement,
                    confirm_wall_plot,
                    confirm_gate_plot,
                    confirm_floor_plot,
                    cancel_placement,
                )
                    .chain()
                    .in_set(GameFlowSet::Input)
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            .add_systems(
                Update,
                (
                    pending_build_arrival_system,
                    build_site_preparation_system,
                    pending_build_cleanup_system,
                    construction_progress_system,
                    wall_auto_tile_system,
                    floor_auto_tile_system,
                    tower_auto_attack,
                    training_queue_system,
                    update_completed_buildings_tracker,
                )
                    .chain()
                    .in_set(GameFlowSet::Simulation)
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
                    sawmill_yard_system,
                    yard_tree_regrowth_system,
                    sync_environment_to_terrain_changes,
                    clear_vegetation_around_buildings,
                )
                    .in_set(GameFlowSet::Simulation)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// ── Obstacle grid sync ──

fn sync_obstacle_grid(
    mut grid: ResMut<ObstacleGrid>,
    trees: Query<&Transform, Or<(With<Sapling>, With<GrowingTree>, With<MatureTree>)>>,
    resource_nodes: Query<&Transform, (With<ResourceNode>, Without<Sapling>, Without<GrowingTree>, Without<MatureTree>)>,
    height_map: Res<HeightMap>,
) {
    grid.cells.clear();
    for tf in &trees {
        let (gx, gz) = WallGrid::world_to_grid(tf.translation);
        grid.cells.insert((gx, gz));
    }
    // Resource nodes (iron, copper, trees, etc.) also block floor/wall placement
    for tf in &resource_nodes {
        let (gx, gz) = WallGrid::world_to_grid(tf.translation);
        grid.cells.insert((gx, gz));
    }
    // Compute playable boundary from border hill settings
    let border = BorderSettings::from_map_size(height_map.map_size);
    grid.playable_half = height_map.half_map - border.thickness - border.transition;
}

fn sync_environment_to_terrain_changes(
    mut commands: Commands,
    mut dirty_areas: ResMut<TerrainSurfaceDirtyQueue>,
    height_map: Res<HeightMap>,
    registry: Res<BlueprintRegistry>,
    mut terrain_followers: ParamSet<(
        Query<
            '_, 
            '_, 
            &mut Transform,
            (
                Or<(With<Sapling>, With<GrowingTree>, With<MatureTree>)>,
                Without<Building>,
            ),
        >,
        Query<
            '_,
            '_,
            (&mut Transform, &ResourceNode, Option<&TerrainHeightOffset>),
            (
                Without<Building>,
                Without<Sapling>,
                Without<GrowingTree>,
                Without<MatureTree>,
            ),
        >,
        Query<
            '_,
            '_,
            (&mut Transform, &GrowingResource, Option<&TerrainHeightOffset>),
            (Without<Building>, Without<Sapling>, Without<GrowingTree>, Without<MatureTree>),
        >,
        Query<
            '_,
            '_,
            (Entity, &DecoChunk, &Transform, &Mesh3d),
            (Without<Building>, Without<Sapling>, Without<GrowingTree>, Without<MatureTree>),
        >,
    )>,
    mut building_q: Query<(&mut Transform, &EntityKind, &BuildingFootprint), With<Building>>,
    grass_chunks: Query<(Entity, &GrassChunk, &Mesh3d)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    while let Some(area) = dirty_areas.pending.pop_front() {
        sync_building_heights_for_area(&height_map, &registry, area, &mut building_q);
        sync_tree_heights_for_area(&height_map, area, &mut terrain_followers.p0());
        sync_resource_node_heights_for_area(&height_map, area, &mut terrain_followers.p1());
        sync_growing_resource_heights_for_area(&height_map, area, &mut terrain_followers.p2());
        clear_vegetation_in_radius(
            &mut commands,
            &grass_chunks,
            &terrain_followers.p3(),
            &mut meshes,
            area.center.x,
            area.center.y,
            area.radius + 2.0,
        );
    }
}

fn sync_building_heights_for_area(
    height_map: &HeightMap,
    registry: &BlueprintRegistry,
    area: TerrainSurfaceDirtyArea,
    building_q: &mut Query<(&mut Transform, &EntityKind, &BuildingFootprint), With<Building>>,
) {
    let area_r2 = area.radius * area.radius;

    for (mut transform, kind, footprint) in building_q.iter_mut() {
        let dx = transform.translation.x - area.center.x;
        let dz = transform.translation.z - area.center.y;
        let reach = area.radius + footprint.0;
        if dx * dx + dz * dz > reach * reach {
            continue;
        }

        let bp = registry.get(*kind);
        let half_height = if bp.visual.mesh_kind.is_gltf() && !bp.visual.mesh_kind.is_gltf_character()
        {
            0.0
        } else {
            bp.building.as_ref().map(|b| b.half_height).unwrap_or(0.0)
        };
        let ground_y = if uses_terrain_foundation(*kind) {
            height_map.foundation_target_height_shaped(
                transform.translation.x,
                transform.translation.z,
                footprint.0,
            )
        } else {
            height_map.sample(transform.translation.x, transform.translation.z)
        };

        if (transform.translation.y - (ground_y + half_height)).abs() > 0.001
            || dx * dx + dz * dz <= area_r2
        {
            transform.translation.y = ground_y + half_height;
        }
    }
}

fn sync_tree_heights_for_area(
    height_map: &HeightMap,
    area: TerrainSurfaceDirtyArea,
    trees: &mut Query<
        &mut Transform,
        (
            Or<(With<Sapling>, With<GrowingTree>, With<MatureTree>)>,
            Without<Building>,
        ),
    >,
) {
    let area_r2 = area.radius * area.radius;

    for mut transform in trees.iter_mut() {
        let dx = transform.translation.x - area.center.x;
        let dz = transform.translation.z - area.center.y;
        if dx * dx + dz * dz > area_r2 {
            continue;
        }

        transform.translation.y =
            height_map.sample(transform.translation.x, transform.translation.z);
    }
}

fn sync_resource_node_heights_for_area(
    height_map: &HeightMap,
    area: TerrainSurfaceDirtyArea,
    resource_nodes: &mut Query<
        (&mut Transform, &ResourceNode, Option<&TerrainHeightOffset>),
        (
            Without<Building>,
            Without<Sapling>,
            Without<GrowingTree>,
            Without<MatureTree>,
        ),
    >,
) {
    let area_r2 = area.radius * area.radius;

    for (mut transform, node, offset) in resource_nodes.iter_mut() {
        let dx = transform.translation.x - area.center.x;
        let dz = transform.translation.z - area.center.y;
        if dx * dx + dz * dz > area_r2 {
            continue;
        }

        let terrain_offset = offset
            .map(|o| o.0)
            .unwrap_or_else(|| default_resource_height_offset(node.resource_type));
        transform.translation.y =
            height_map.sample(transform.translation.x, transform.translation.z) + terrain_offset;
    }
}

fn sync_growing_resource_heights_for_area(
    height_map: &HeightMap,
    area: TerrainSurfaceDirtyArea,
    growing_resources: &mut Query<
        (&mut Transform, &GrowingResource, Option<&TerrainHeightOffset>),
        (Without<Building>, Without<Sapling>, Without<GrowingTree>, Without<MatureTree>),
    >,
) {
    let area_r2 = area.radius * area.radius;

    for (mut transform, growing, offset) in growing_resources.iter_mut() {
        let dx = transform.translation.x - area.center.x;
        let dz = transform.translation.z - area.center.y;
        if dx * dx + dz * dz > area_r2 {
            continue;
        }

        let terrain_offset = offset
            .map(|o| o.0)
            .unwrap_or_else(|| default_resource_height_offset(growing.resource_type));
        transform.translation.y =
            height_map.sample(transform.translation.x, transform.translation.z) + terrain_offset;
    }
}

fn default_resource_height_offset(resource_type: ResourceType) -> f32 {
    match resource_type {
        ResourceType::Oil => 0.6,
        _ => 0.0,
    }
}

// ── Asset creation (ghost materials only) ──

fn create_ghost_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Create a procedural hologram grid texture for the placement ground plane.
    let grid_tex = {
        use bevy::image::{ImageAddressMode, ImageSamplerDescriptor};
        const TEX_SIZE: u32 = 128;
        const MINOR_CELLS: f32 = 12.0;
        const MAJOR_CELLS: f32 = 3.0;
        let mut data = vec![0u8; (TEX_SIZE * TEX_SIZE * 4) as usize];

        let line_strength = |coord: f32, cells: f32, width: f32| -> f32 {
            let frac = (coord * cells).fract();
            let dist = (frac - 0.5).abs() * 2.0;
            ((width - dist) / width).clamp(0.0, 1.0)
        };

        for y in 0..TEX_SIZE {
            for x in 0..TEX_SIZE {
                let u = (x as f32 + 0.5) / TEX_SIZE as f32;
                let v = (y as f32 + 0.5) / TEX_SIZE as f32;
                let centered_x = u * 2.0 - 1.0;
                let centered_y = v * 2.0 - 1.0;
                let radial = (centered_x * centered_x + centered_y * centered_y).sqrt();
                let center_glow = (1.0 - radial).clamp(0.0, 1.0).powf(1.6);
                let edge_fade = (1.0 - radial * 0.85).clamp(0.0, 1.0).powf(2.2);
                let minor_grid = line_strength(u, MINOR_CELLS, 0.18)
                    .max(line_strength(v, MINOR_CELLS, 0.18));
                let major_grid = line_strength(u, MAJOR_CELLS, 0.28)
                    .max(line_strength(v, MAJOR_CELLS, 0.28));
                let border = centered_x.abs().max(centered_y.abs());
                let border_glow = ((border - 0.78) / 0.22).clamp(0.0, 1.0).powf(1.5);
                let idx = ((y * TEX_SIZE + x) * 4) as usize;

                let line_mix = (minor_grid * 0.45 + major_grid * 0.85 + border_glow * 0.55)
                    .clamp(0.0, 1.0);
                let alpha = (12.0 + edge_fade * 40.0 + center_glow * 36.0 + line_mix * 150.0)
                    .clamp(0.0, 255.0);
                let red = (18.0 + center_glow * 20.0 + major_grid * 14.0).clamp(0.0, 255.0);
                let green = (90.0 + edge_fade * 48.0 + line_mix * 108.0).clamp(0.0, 255.0);
                let blue = (70.0 + center_glow * 44.0 + line_mix * 118.0).clamp(0.0, 255.0);

                data[idx] = red as u8;
                data[idx + 1] = green as u8;
                data[idx + 2] = blue as u8;
                data[idx + 3] = alpha as u8;
            }
        }
        let mut img = Image::new_fill(
            bevy::render::render_resource::Extent3d {
                width: TEX_SIZE,
                height: TEX_SIZE,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            &[255, 255, 255, 255],
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        img.data = Some(data);
        img.sampler = bevy::image::ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            ..default()
        });
        images.add(img)
    };

    commands.insert_resource(BuildingGhostMaterials {
        ghost_valid: materials.add(StandardMaterial {
            base_color: Color::srgba(0.35, 0.98, 0.72, 0.34),
            emissive: LinearRgba::new(0.18, 0.72, 0.45, 1.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        ghost_invalid: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.38, 0.22, 0.34),
            emissive: LinearRgba::new(0.82, 0.16, 0.08, 1.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        under_construction: materials.add(StandardMaterial {
            base_color: Color::srgba(0.7, 0.65, 0.5, 0.6),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        grid_plane: materials.add(StandardMaterial {
            base_color: Color::srgba(0.82, 1.0, 0.96, 0.9),
            base_color_texture: Some(grid_tex),
            emissive: LinearRgba::new(0.18, 0.56, 0.42, 1.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        }),
    });
}

// ── Placement preview ──

fn cursor_ground_pos(
    camera_q: &Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
    windows: &Query<&Window, With<PrimaryWindow>>,
    graphics: &GraphicsSettings,
    height_map: &HeightMap,
) -> Option<Vec3> {
    let Ok(window) = windows.single() else {
        return None;
    };
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return None;
    };
    let ray = camera::viewport_ray_from_window_cursor(camera, cam_gt, window, graphics)?;
    cursor_terrain_hit(ray, height_map)
}

fn cursor_terrain_hit(ray: Ray3d, height_map: &HeightMap) -> Option<Vec3> {
    height_map.raycast(ray)
}

fn is_pointer_over_ui(ui_interactions: &Query<&Interaction, With<Node>>) -> bool {
    ui_interactions
        .iter()
        .any(|interaction| matches!(*interaction, Interaction::Hovered | Interaction::Pressed))
}

/// Compute grid cells for a wall between two world positions.
/// Walks axis-aligned: first along X, then along Z (L-shape).
fn wall_layout_grid_cells(start: Vec3, end: Vec3) -> Vec<(i32, i32)> {
    let (sx, sz) = WallGrid::world_to_grid(start);
    let (ex, ez) = WallGrid::world_to_grid(end);

    let mut cells = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Walk along X first
    let dx = if ex > sx { 1 } else { -1 };
    let mut cx = sx;
    loop {
        if seen.insert((cx, sz)) {
            cells.push((cx, sz));
        }
        if cx == ex {
            break;
        }
        cx += dx;
    }

    // Then walk along Z
    let dz = if ez > sz { 1 } else { -1 };
    let mut cz = sz;
    loop {
        if seen.insert((ex, cz)) {
            cells.push((ex, cz));
        }
        if cz == ez {
            break;
        }
        cz += dz;
    }

    cells
}

/// Compute the cost for placing walls at the given grid cells.
/// Uses a merged grid (existing + proposed) to determine piece types.
fn wall_cost_from_cells(
    proposed: &[(i32, i32)],
    existing_grid: &WallGrid,
    registry: &BlueprintRegistry,
) -> crate::blueprints::ResourceCost {
    use crate::blueprints::ResourceCost;

    let mut total = ResourceCost::default();
    if proposed.is_empty() {
        return total;
    }

    // Build temporary merged set for neighbor lookups
    let mut merged: std::collections::HashSet<(i32, i32)> = existing_grid.cells.keys().copied().collect();
    for &cell in proposed {
        merged.insert(cell);
    }

    for &(gx, gz) in proposed {
        // Skip cells already in the grid
        if existing_grid.cells.contains_key(&(gx, gz)) {
            continue;
        }

        // Compute neighbor mask from merged set
        let mut mask = 0u8;
        for (i, (nx, nz)) in WallGrid::cardinal_neighbors(gx, gz).iter().enumerate() {
            if merged.contains(&(*nx, *nz)) {
                mask |= 1 << i;
            }
        }

        let (piece_kind, _) = auto_tile_piece(mask, false);
        let kind = piece_kind_to_entity_kind(piece_kind);
        let cost = &registry.get(kind).cost;

        for rt in ResourceType::ALL.iter() {
            total.set(*rt, total.get(*rt) + cost.get(*rt));
        }
    }
    total
}

fn clear_wall_preview(commands: &mut Commands, wall_preview: &mut WallPlotPreview) {
    for entity in wall_preview.ghost_entities.drain(..) {
        commands.entity(entity).try_despawn();
    }
    wall_preview.start = None;
    wall_preview.snapped_points.clear();
    wall_preview.total_cost = crate::blueprints::ResourceCost::default();
    wall_preview.valid = false;
}

fn floor_layout_grid_cells(start: Vec3, end: Vec3) -> Vec<(i32, i32)> {
    let (sx, sz) = WallGrid::world_to_grid(start);
    let (ex, ez) = WallGrid::world_to_grid(end);
    let min_x = sx.min(ex);
    let max_x = sx.max(ex);
    let min_z = sz.min(ez);
    let max_z = sz.max(ez);

    let mut cells = Vec::new();
    for gz in min_z..=max_z {
        for gx in min_x..=max_x {
            cells.push((gx, gz));
        }
    }
    cells
}

fn floor_cost_from_cells(
    cells: &[(i32, i32)],
    floor_grid: &FloorGrid,
    registry: &BlueprintRegistry,
) -> crate::blueprints::ResourceCost {
    use crate::blueprints::ResourceCost;

    let mut total = ResourceCost::default();
    let cost = &registry.get(EntityKind::Floor).cost;
    for cell in cells {
        if floor_grid.cells.contains_key(cell) {
            continue;
        }
        for rt in ResourceType::ALL.iter() {
            total.set(*rt, total.get(*rt) + cost.get(*rt));
        }
    }
    total
}

fn clear_floor_preview(commands: &mut Commands, floor_preview: &mut FloorPlotPreview) {
    for entity in floor_preview.ghost_entities.drain(..) {
        commands.entity(entity).try_despawn();
    }
    floor_preview.start = None;
    floor_preview.cells.clear();
    floor_preview.total_cost = crate::blueprints::ResourceCost::default();
    floor_preview.valid = false;
}

fn placement_kind(mode: PlacementMode) -> Option<EntityKind> {
    match mode {
        PlacementMode::Placing(kind) => Some(kind),
        PlacementMode::PlotBase => Some(EntityKind::Base),
        PlacementMode::None
        | PlacementMode::PlotWall { .. }
        | PlacementMode::PlotGate
        | PlacementMode::PlotFloor => None,
    }
}

fn update_placement_preview(
    mut commands: Commands,
    mut placement: ResMut<BuildingPlacementState>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    ghost_mats: Res<BuildingGhostMaterials>,
    building_models: Option<Res<BuildingModelAssets>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut meshes: ResMut<Assets<Mesh>>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    mut ghosts: Query<&mut Transform, With<GhostBuilding>>,
    mut ghost_valid_q: Query<&mut GhostValid, With<GhostBuilding>>,
    existing_buildings: Query<
        (&Transform, &BuildingFootprint, &EntityKind),
        (With<Building>, Without<GhostBuilding>),
    >,
    worker_context: (
        Res<ActivePlayer>,
        Query<
            (Entity, &Transform, &UnitState, &Faction, &EntityKind),
            (With<Unit>, Without<GhostBuilding>),
        >,
    ),
    environment: (
        Option<Res<BiomeMap>>,
        Res<HeightMap>,
        Res<ObstacleGrid>,
    ),
) {
    let (active_player, workers) = worker_context;
    let (biome_map, height_map, obstacle_grid) = environment;
    let (camera_q, windows, graphics) = viewport;
    let Some(kind) = placement_kind(placement.mode) else {
        return;
    };

    // Handle rotation input (H = rotate left, J = rotate right) — 90 degree steps
    if keyboard.just_pressed(KeyCode::KeyH) {
        placement.rotation_y += std::f32::consts::FRAC_PI_2;
    }
    if keyboard.just_pressed(KeyCode::KeyJ) {
        placement.rotation_y -= std::f32::consts::FRAC_PI_2;
    }

    let bp = registry.get(kind);
    let is_gltf = bp.visual.mesh_kind.is_gltf();
    let half_h = if is_gltf {
        0.0
    } else {
        bp.building.as_ref().map(|b| b.half_height).unwrap_or(1.0)
    };
    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) else {
        return;
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
                if let Some(scene_handle) =
                    models.scene_for(kind, 1, Vec3::new(world_pos.x, 0.0, world_pos.z))
                {
                    ghost_cmds.with_child((
                        SceneRoot(scene_handle),
                        models.child_transform(kind, 1.0),
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

    // Spawn grid plane if it doesn't exist
    if placement.grid_plane_entity.is_none() {
        let grid_size = new_footprint * 2.5;
        // Number of grid cells across the plane (UV tiling)
        let uv_tiles = (grid_size / 2.0).max(2.0);
        let plane_mesh = meshes.add(build_grid_plane_mesh(grid_size, uv_tiles, &height_map));
        let grid_entity = commands
            .spawn((
                GhostGridPlane,
                Mesh3d(plane_mesh),
                MeshMaterial3d(ghost_mats.grid_plane.clone()),
                Transform::from_translation(Vec3::new(0.0, -100.0, 0.0)),
                NotShadowCaster,
                NotShadowReceiver,
            ))
            .id();
        placement.grid_plane_entity = Some(grid_entity);
    }

    let Some(ghost_entity) = placement.preview_entity else {
        return;
    };
    let Ok(mut ghost_tf) = ghosts.get_mut(ghost_entity) else {
        return;
    };

    let ground_y = if uses_terrain_foundation(kind) {
        height_map.foundation_target_height_shaped(world_pos.x, world_pos.z, new_footprint)
    } else {
        height_map.sample(world_pos.x, world_pos.z)
    };
    let y = ground_y + half_h;
    ghost_tf.translation = Vec3::new(world_pos.x, y, world_pos.z);
    ghost_tf.rotation = Quat::from_rotation_y(placement.rotation_y);

    // Update grid plane position & mesh to align with terrain
    if let Some(grid_entity) = placement.grid_plane_entity {
        let grid_size = new_footprint * 2.5;
        let uv_tiles = (grid_size / 2.0).max(2.0);
        let new_mesh = meshes.add(build_grid_plane_mesh_at(
            world_pos.x,
            world_pos.z,
            grid_size,
            uv_tiles,
            &height_map,
        ));
        commands.entity(grid_entity).insert((
            Mesh3d(new_mesh),
            Transform::from_translation(Vec3::new(world_pos.x, 0.0, world_pos.z)),
        ));
    }

    let mut valid = true;
    let mut hint: Option<String> = None;

    if let Some(ref bm) = biome_map {
        let biome = bm.get_biome(world_pos.x, world_pos.z);
        if !is_biome_valid_for(kind, biome) {
            valid = false;
            hint = biome_requirement_text(kind).map(ToOwned::to_owned);
        }
    }

    // Slope validation (walls are exempt — they're designed for varied terrain)
    if !matches!(kind, EntityKind::WallSegment | EntityKind::WallPost | EntityKind::WallCorner) {
        const MAX_BUILDING_SLOPE: f32 = 0.5; // ~27 degrees
        let slope = height_map.max_slope_under_footprint(world_pos.x, world_pos.z, new_footprint);
        if slope > MAX_BUILDING_SLOPE {
            valid = false;
            if hint.is_none() {
                hint = Some("Terrain too steep".to_owned());
            }
        }
    }

    for (building_tf, existing_footprint, existing_kind) in &existing_buildings {
        if !blocks_construction_overlap(*existing_kind) {
            continue;
        }
        let min_dist = existing_footprint.0 + new_footprint;
        let dx = building_tf.translation.x - ghost_tf.translation.x;
        let dz = building_tf.translation.z - ghost_tf.translation.z;
        if (dx * dx + dz * dz).sqrt() < min_dist {
            valid = false;
            break;
        }
    }

    if obstacle_grid.is_footprint_blocked(Vec3::new(world_pos.x, 0.0, world_pos.z), new_footprint) {
        valid = false;
        if hint.is_none() {
            hint = Some("Blocked by trees".to_owned());
        }
    }

    let half_map = height_map.half_map;
    if world_pos.x.abs() > half_map - 5.0 || world_pos.z.abs() > half_map - 5.0 {
        valid = false;
    }

    if valid {
        let build_pos = Vec3::new(world_pos.x, 0.0, world_pos.z);
        let worker_iter = workers.iter().map(|(e, tf, state, fac, kind)| (e, tf, state, fac, kind));
        if !has_available_worker_for_build(worker_iter, active_player.0, build_pos) {
            valid = false;
            hint = Some("All workers are busy".to_owned());
        }
    }

    placement.hint_text = if !valid { hint } else { None };

    if let Ok(mut gv) = ghost_valid_q.get_mut(ghost_entity) {
        gv.0 = valid;
    }
}

/// Build a flat grid plane mesh centered at origin. Used as initial mesh (position set later).
fn build_grid_plane_mesh(size: f32, uv_tiles: f32, _height_map: &HeightMap) -> Mesh {
    build_grid_plane_mesh_at(0.0, 0.0, size, uv_tiles, _height_map)
}

/// Build a grid plane mesh that conforms to the terrain at the given world position.
/// The mesh is centered at (0,0,0) — the Transform positions it in world space.
fn build_grid_plane_mesh_at(
    cx: f32,
    cz: f32,
    size: f32,
    uv_tiles: f32,
    height_map: &HeightMap,
) -> Mesh {
    // Subdivide the plane into a grid so it follows terrain contour
    let subdivisions: u32 = 12;
    let verts_per_side = subdivisions + 1;
    let total_verts = (verts_per_side * verts_per_side) as usize;

    let mut positions = Vec::with_capacity(total_verts);
    let mut normals = Vec::with_capacity(total_verts);
    let mut uvs = Vec::with_capacity(total_verts);

    let half = size / 2.0;
    let step = size / subdivisions as f32;

    for iz in 0..verts_per_side {
        for ix in 0..verts_per_side {
            let lx = -half + ix as f32 * step;
            let lz = -half + iz as f32 * step;
            let wx = cx + lx;
            let wz = cz + lz;
            let wy = height_map.sample(wx, wz) + 0.15; // slight offset above terrain
            positions.push([lx, wy, lz]);
            normals.push([0.0, 1.0, 0.0]);
            let u = ix as f32 / subdivisions as f32 * uv_tiles;
            let v = iz as f32 / subdivisions as f32 * uv_tiles;
            uvs.push([u, v]);
        }
    }

    let mut indices = Vec::with_capacity((subdivisions * subdivisions * 6) as usize);
    for iz in 0..subdivisions {
        for ix in 0..subdivisions {
            let tl = iz * verts_per_side + ix;
            let tr = tl + 1;
            let bl = (iz + 1) * verts_per_side + ix;
            let br = bl + 1;
            indices.push(tl);
            indices.push(bl);
            indices.push(tr);
            indices.push(tr);
            indices.push(bl);
            indices.push(br);
        }
    }

    Mesh::new(
        bevy::render::render_resource::PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(bevy::mesh::Indices::U32(indices))
}

fn update_wall_plot_preview(
    mut commands: Commands,
    mut placement: ResMut<BuildingPlacementState>,
    mut wall_preview: ResMut<WallPlotPreview>,
    active_player: Res<ActivePlayer>,
    wall_grid: Res<WallGrid>,
    registry: Res<BlueprintRegistry>,
    ghost_mats: Res<BuildingGhostMaterials>,
    building_models: Option<Res<BuildingModelAssets>>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    existing_buildings: Query<
        (&Transform, &BuildingFootprint, &EntityKind),
        (With<Building>, Without<GhostBuilding>),
    >,
    workers: Query<
        (Entity, &Transform, &UnitState, &Faction, &EntityKind),
        (With<Unit>, Without<GhostBuilding>),
    >,
    height_map: Res<HeightMap>,
    obstacle_grid: Res<ObstacleGrid>,
) {
    let (camera_q, windows, graphics) = viewport;
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

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) else {
        return;
    };

    let cells = wall_layout_grid_cells(start, Vec3::new(world_pos.x, 0.0, world_pos.z));
    if cells.is_empty() {
        placement.hint_text = Some("Move cursor to plot wall".to_string());
        return;
    }

    // Filter out cells already occupied in the grid or blocked by obstacles
    let new_cells: Vec<(i32, i32)> = cells
        .iter()
        .copied()
        .filter(|c| !wall_grid.cells.contains_key(c) && !obstacle_grid.is_cell_blocked(c.0, c.1))
        .collect();

    if new_cells.is_empty() {
        placement.hint_text = Some("All cells already have walls".to_string());
        return;
    }

    // Store snapped world points for the confirm system
    wall_preview.start = Some(start);
    wall_preview.snapped_points = new_cells
        .iter()
        .map(|&(gx, gz)| WallGrid::grid_to_world(gx, gz))
        .collect();
    wall_preview.total_cost = wall_cost_from_cells(&new_cells, &wall_grid, &registry);
    wall_preview.valid = true;

    // Build merged set for neighbor lookups (existing grid + proposed cells)
    let mut merged: std::collections::HashSet<(i32, i32)> =
        wall_grid.cells.keys().copied().collect();
    for &cell in &new_cells {
        merged.insert(cell);
    }

    // Validate each cell and spawn ghost
    let half_map = height_map.half_map;
    for &(gx, gz) in &new_cells {
        let world = WallGrid::grid_to_world(gx, gz);
        let y = height_map.sample(world.x, world.z);

        // Check map bounds
        if world.x.abs() > half_map - 5.0 || world.z.abs() > half_map - 5.0 {
            wall_preview.valid = false;
        }

        // Check collision with non-wall buildings
        let fp = footprint_for_kind(EntityKind::WallPost);
        let blocked = existing_buildings.iter().any(|(building_tf, existing_fp, existing_kind)| {
            if !blocks_construction_overlap(*existing_kind) {
                return false;
            }
            let check_pos = Vec3::new(world.x, building_tf.translation.y, world.z);
            building_tf.translation.distance(check_pos) < existing_fp.0 + fp
        });
        if blocked {
            wall_preview.valid = false;
        }

        // Determine auto-tiled piece for this cell
        let mut mask = 0u8;
        for (i, (nx, nz)) in WallGrid::cardinal_neighbors(gx, gz).iter().enumerate() {
            if merged.contains(&(*nx, *nz)) {
                mask |= 1 << i;
            }
        }
        let (piece_kind, rotation_y) = auto_tile_piece(mask, false);
        let kind = piece_kind_to_entity_kind(piece_kind);

        // Spawn GLTF ghost with correct model and rotation
        let _ghost_mat = if wall_preview.valid {
            ghost_mats.ghost_valid.clone()
        } else {
            ghost_mats.ghost_invalid.clone()
        };

        let mut ghost_cmds = commands.spawn((
            GhostBuilding,
            GhostValid(wall_preview.valid),
            Transform::from_translation(Vec3::new(world.x, y, world.z))
                .with_rotation(Quat::from_rotation_y(rotation_y)),
            Visibility::default(),
            NotShadowCaster,
            NotShadowReceiver,
        ));

        if let Some(ref models) = building_models {
            if let Some(scene_handle) = models.scene_for(kind, 1, world) {
                ghost_cmds.with_child((
                    SceneRoot(scene_handle),
                    models.child_transform(kind, 1.0),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
        }

        wall_preview.ghost_entities.push(ghost_cmds.id());
    }

    let count = new_cells.len();
    let cost = &wall_preview.total_cost;
    let wood = cost.get(ResourceType::Wood);
    let stone = cost.get(ResourceType::Stone);
    if wall_preview.valid {
        let wall_pos = wall_preview.snapped_points[0];
        let worker_iter = workers.iter().map(|(e, tf, state, fac, kind)| (e, tf, state, fac, kind));
        if !has_available_worker_for_build(worker_iter, active_player.0, wall_pos) {
            wall_preview.valid = false;
        }
    }
    placement.hint_text = Some(if wall_preview.valid {
        format!("Wall: {count} pieces | Cost: {wood}W {stone}S")
    } else {
        "Wall path blocked or all workers are busy".to_string()
    });
}

fn update_gate_plot_preview(
    mut commands: Commands,
    mut placement: ResMut<BuildingPlacementState>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    ghost_mats: Res<BuildingGhostMaterials>,
    building_models: Option<Res<BuildingModelAssets>>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
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
    workers: Query<
        (Entity, &Transform, &UnitState, &Faction, &EntityKind),
        (With<Unit>, Without<GhostBuilding>),
    >,
    active_player: Res<ActivePlayer>,
    height_map: Res<HeightMap>,
) {
    let (camera_q, windows, graphics) = viewport;
    if placement.mode != PlacementMode::PlotGate {
        return;
    }

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) else {
        return;
    };

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
                if let Some(scene_handle) =
                    models.scene_for(kind, 1, Vec3::new(world_pos.x, 0.0, world_pos.z))
                {
                    ghost_cmds.with_child((
                        SceneRoot(scene_handle),
                        models.child_transform(kind, 1.0),
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
        let worker_iter = workers.iter().map(|(e, tf, state, fac, kind)| (e, tf, state, fac, kind));
        let has_worker =
            has_available_worker_for_build(worker_iter, active_player.0, segment_tf.translation);
        if let Ok(mut gv) = ghost_valid_q.get_mut(ghost_entity) {
            gv.0 = has_worker;
        }
        placement.hint_text = Some(if has_worker {
            "Click to replace wall segment with Gatehouse".to_string()
        } else {
            "All workers are busy".to_string()
        });
    } else {
        ghost_tf.translation = Vec3::new(world_pos.x, -100.0, world_pos.z);
        if let Ok(mut gv) = ghost_valid_q.get_mut(ghost_entity) {
            gv.0 = false;
        }
        placement.hint_text = Some("Gatehouse must replace an owned wall segment".to_string());
    }
}

fn update_floor_plot_preview(
    mut commands: Commands,
    mut placement: ResMut<BuildingPlacementState>,
    mut floor_preview: ResMut<FloorPlotPreview>,
    floor_grid: Res<FloorGrid>,
    registry: Res<BlueprintRegistry>,
    ghost_mats: Res<BuildingGhostMaterials>,
    cache: Res<EntityVisualCache>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    height_map: Res<HeightMap>,
    obstacle_grid: Res<ObstacleGrid>,
) {
    let (camera_q, windows, graphics) = viewport;
    if !matches!(placement.mode, PlacementMode::PlotFloor) {
        if floor_preview.start.is_some() || !floor_preview.ghost_entities.is_empty() {
            clear_floor_preview(&mut commands, &mut floor_preview);
        }
        return;
    }

    // Clear old brush indicator each frame
    clear_floor_preview(&mut commands, &mut floor_preview);

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) else {
        return;
    };

    let (gx, gz) = WallGrid::world_to_grid(Vec3::new(world_pos.x, 0.0, world_pos.z));
    let world = WallGrid::grid_to_world(gx, gz);

    let already_placed = floor_grid.cells.contains_key(&(gx, gz));
    let blocked = obstacle_grid.is_cell_blocked(gx, gz);
    let half_map = height_map.half_map;
    let out_of_bounds = world.x.abs() > half_map - 5.0 || world.z.abs() > half_map - 5.0;
    let valid = !already_placed && !blocked && !out_of_bounds;

    let y = height_map.sample(world.x, world.z) + 0.08;

    // Flat plane brush indicator — no side walls, no shadow artifacts
    let Some(mesh_handle) = cache.floor_brush_indicator.clone() else {
        return;
    };

    let mat = if valid {
        ghost_mats.ghost_valid.clone()
    } else {
        ghost_mats.ghost_invalid.clone()
    };

    let ghost = commands
        .spawn((
            GhostBuilding,
            GhostValid(valid),
            Mesh3d(mesh_handle),
            MeshMaterial3d(mat),
            Transform::from_translation(Vec3::new(world.x, y, world.z)),
            NotShadowCaster,
            NotShadowReceiver,
        ))
        .id();
    floor_preview.ghost_entities.push(ghost);

    if valid {
        let cost = registry.get(EntityKind::Floor).cost.clone();
        let wood = cost.get(ResourceType::Wood);
        let stone = cost.get(ResourceType::Stone);
        placement.hint_text =
            Some(format!("Floor brush | Cost per tile: {wood}W {stone}S"));
    } else if already_placed {
        placement.hint_text = Some("Already has floor".to_string());
    } else if blocked {
        placement.hint_text = Some("Blocked by resource".to_string());
    } else {
        placement.hint_text = Some("Out of bounds".to_string());
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

fn animate_placement_preview_vfx(
    time: Res<Time>,
    ghost_mats: Res<BuildingGhostMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    ghost_valid_q: Query<&GhostValid, With<GhostBuilding>>,
    mut grid_planes: Query<&mut Transform, With<GhostGridPlane>>,
) {
    let pulse = (time.elapsed_secs() * 3.6).sin() * 0.5 + 0.5;
    let intensity = 0.82 + pulse * 0.18;
    let invalid_active = ghost_valid_q.iter().any(|valid| !valid.0);

    if let Some(material) = materials.get_mut(&ghost_mats.ghost_valid) {
        material.base_color = Color::srgba(0.35, 0.98, 0.72, 0.28 + pulse * 0.1);
        material.emissive = LinearRgba::new(
            0.14 * intensity,
            0.72 * intensity,
            0.44 * intensity,
            1.0,
        );
    }

    if let Some(material) = materials.get_mut(&ghost_mats.ghost_invalid) {
        material.base_color = Color::srgba(1.0, 0.38, 0.22, 0.28 + pulse * 0.12);
        material.emissive = LinearRgba::new(
            0.82 * intensity,
            0.18 * intensity,
            0.08 * intensity,
            1.0,
        );
    }

    if let Some(material) = materials.get_mut(&ghost_mats.grid_plane) {
        let (base_color, emissive) = if invalid_active {
            (
                Color::srgba(1.0, 0.8, 0.72, 0.94),
                LinearRgba::new(0.58 * intensity, 0.17 * intensity, 0.08 * intensity, 1.0),
            )
        } else {
            (
                Color::srgba(0.82, 1.0, 0.96, 0.9),
                LinearRgba::new(0.16 * intensity, 0.56 * intensity, 0.42 * intensity, 1.0),
            )
        };
        material.base_color = base_color;
        material.emissive = emissive;
    }

    for mut transform in &mut grid_planes {
        transform.translation.y = 0.02 + pulse * 0.035;
    }
}

fn confirm_placement(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut online: PlacementOnlineParams,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    base_state: Res<FactionBaseState>,
    carried_totals: Res<CarriedResourceTotals>,
    mut pending_drains: ResMut<PendingCarriedDrains>,
    registry: Res<BlueprintRegistry>,
    extras: (
        Res<AllCompletedBuildings>,
        Option<Res<BiomeMap>>,
        Res<crate::ages::FactionAges>,
    ),
    height_map: Res<HeightMap>,
    queries: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
        Query<&Interaction, With<Node>>,
        Query<
            (&Transform, &BuildingFootprint, &Faction, &EntityKind),
            (With<Building>, Without<GhostBuilding>),
        >,
        Query<
            (
                Entity,
                &Transform,
                &UnitState,
                &Faction,
                &EntityKind,
                Option<&PendingBuildOrder>,
            ),
            With<Unit>,
        >,
    ),
    obstacle_grid: Res<ObstacleGrid>,
) {
    let (all_completed, biome_map, faction_ages) = extras;
    let (camera_q, windows, graphics, ui_interactions, existing_buildings, workers) = queries;
    let mode = placement.mode;
    let Some(kind) = placement_kind(mode) else {
        return;
    };

    let new_footprint = footprint_for_kind(kind);

    // Phase 1: awaiting initial mouse release
    if placement.awaiting_release {
        if mouse.just_released(MouseButton::Left) {
            placement.awaiting_release = false;
            return;
        } else {
            return;
        }
    } else if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    if is_pointer_over_ui(&ui_interactions) {
        return;
    }

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) else {
        return;
    };

    let bp = registry.get(kind);

    let faction = active_player.0;
    let has_base_started = base_state.is_founded(&faction)
        || existing_buildings
            .iter()
            .any(|(_, _, building_faction, building_kind)| {
                *building_faction == faction && *building_kind == EntityKind::Base
            })
        || workers
            .iter()
            .any(|(_, _, _, worker_faction, _, pending_order)| {
                *worker_faction == faction
                    && pending_order.is_some_and(|order| order.kind == EntityKind::Base)
            });

    let is_initial_base_plot =
        matches!(mode, PlacementMode::PlotBase) && kind == EntityKind::Base && !has_base_started;

    if matches!(mode, PlacementMode::PlotBase) && kind == EntityKind::Base && has_base_started {
        placement.hint_text = Some("Base is already being founded.".to_string());
        return;
    }

    // Check prerequisite
    let prereq_met = if let Some(ref bd) = bp.building {
        match (is_initial_base_plot, bd.prerequisite) {
            (true, _) => true,
            (false, None) => true,
            (false, Some(prereq_kind)) => {
                if prereq_kind == EntityKind::Base {
                    base_state.is_founded(&faction) || all_completed.has(&faction, prereq_kind)
                } else {
                    all_completed.has(&faction, prereq_kind)
                }
            }
        }
    } else {
        true
    };
    if !prereq_met {
        return;
    }

    // Check age requirement
    let required_age = crate::ages::required_age_for_building(kind);
    let current_age = faction_ages.get_age(&faction);
    if current_age < required_age {
        placement.hint_text = Some(format!("Requires {}", required_age.display_name()));
        return;
    }

    // Check biome validity
    if let Some(ref bm) = biome_map {
        if !is_biome_valid_for(kind, bm.get_biome(world_pos.x, world_pos.z)) {
            return;
        }
    }
    // Slope validation (walls exempt)
    if !matches!(kind, EntityKind::WallSegment | EntityKind::WallPost | EntityKind::WallCorner) {
        const MAX_BUILDING_SLOPE: f32 = 0.5;
        let slope = height_map.max_slope_under_footprint(world_pos.x, world_pos.z, new_footprint);
        if slope > MAX_BUILDING_SLOPE {
            return;
        }
    }
    for (building_tf, existing_fp, _, existing_kind) in &existing_buildings {
        if !blocks_construction_overlap(*existing_kind) {
            continue;
        }
        let dx = building_tf.translation.x - world_pos.x;
        let dz = building_tf.translation.z - world_pos.z;
        if (dx * dx + dz * dz).sqrt() < existing_fp.0 + new_footprint {
            return;
        }
    }
    if obstacle_grid.is_footprint_blocked(Vec3::new(world_pos.x, 0.0, world_pos.z), new_footprint) {
        return;
    }
    let half_map = height_map.half_map;
    if world_pos.x.abs() > half_map - 5.0 || world_pos.z.abs() > half_map - 5.0 {
        return;
    }

    // Find best available worker using priority-based selection
    let build_pos = Vec3::new(world_pos.x, 0.0, world_pos.z);
    let worker_iter = workers.iter().map(|(e, tf, state, fac, kind, _)| (e, tf, state, fac, kind));
    let Some((worker_entity, _)) = find_best_worker_for_build(worker_iter, faction, build_pos) else {
        placement.hint_text = Some("No workers available!".to_string());
        return;
    };

    // Check affordability (stored + carried)
    let player_res = all_resources.get(&faction);
    let carried = carried_totals.get(&faction);
    if !bp.cost.can_afford_with_carried(player_res, carried) {
        return;
    }

    if *online.net_role == crate::multiplayer::NetRole::Client {
        let (Some(client), Some(ref mut socket)) = (
            online.client_state.as_ref(),
            online.matchbox_socket.as_mut(),
        ) else {
            return;
        };
        let seq = {
            let mut s = client.seq.lock().unwrap();
            *s += 1;
            *s
        };
        let msg = game_state::message::ClientMessage::Input {
            seq,
            timestamp: online.time.elapsed_secs_f64(),
            input: game_state::message::PlayerInput {
                player_id: client.player_id as u32,
                tick: 0,
                entity_ids: Vec::new(),
                commands: vec![game_state::message::InputCommand::Build {
                    kind: kind.to_index(),
                    position: [build_pos.x, build_pos.y, build_pos.z],
                }],
            },
        };
        crate::multiplayer::matchbox_transport::send_to_host(socket, &msg);

        if let Some(ghost) = placement.preview_entity {
            commands.entity(ghost).try_despawn();
        }
        if let Some(grid) = placement.grid_plane_entity {
            commands.entity(grid).try_despawn();
        }
        placement.mode = PlacementMode::None;
        placement.preview_entity = None;
        placement.grid_plane_entity = None;
        placement.hint_text = None;
        placement.rotation_y = 0.0;
        return;
    }

    // Deduct from stored first, queue carried drain for any deficit
    let player_res_mut = all_resources.get_mut(&faction);
    let deficits = bp.cost.deduct_with_carried(player_res_mut);
    let drain = SpendFromCarried {
        faction,
        amounts: deficits,
    };
    if drain.has_deficit() {
        pending_drains.drains.push(drain);
    }

    let rotation_y = placement.rotation_y;

    // Despawn ghost & grid plane
    if let Some(ghost) = placement.preview_entity {
        commands.entity(ghost).try_despawn();
    }
    if let Some(grid) = placement.grid_plane_entity {
        commands.entity(grid).try_despawn();
    }

    // Clean up any existing gathering assignment before reassigning
    if let Ok((_, _, w_state, _, _, _)) = workers.get(worker_entity) {
        cleanup_worker_assignment(&mut commands, worker_entity, w_state);
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
            rotation_y,
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
    placement.grid_plane_entity = None;
    placement.hint_text = None;
    placement.rotation_y = 0.0;
}

fn confirm_wall_plot(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut wall_preview: ResMut<WallPlotPreview>,
    mut wall_grid: ResMut<WallGrid>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    ui_interactions: Query<&Interaction, With<Node>>,
    height_map: Res<HeightMap>,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    building_models: Option<Res<BuildingModelAssets>>,
    workers: Query<(Entity, &Transform, &UnitState, &Faction, &EntityKind), With<Unit>>,
    obstacle_grid: Res<ObstacleGrid>,
) {
    let (camera_q, windows, graphics) = viewport;
    if !matches!(placement.mode, PlacementMode::PlotWall { .. }) || placement.awaiting_release {
        return;
    }

    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    if is_pointer_over_ui(&ui_interactions) {
        return;
    }

    if let PlacementMode::PlotWall { start } = placement.mode {
        if start == Vec3::ZERO {
            if let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) {
                // Snap start to grid
                let (gx, gz) = WallGrid::world_to_grid(Vec3::new(world_pos.x, 0.0, world_pos.z));
                let first = WallGrid::grid_to_world(gx, gz);
                wall_preview.start = Some(first);
                placement.mode = PlacementMode::PlotWall { start: first };
                placement.hint_text =
                    Some("Move cursor and click again to confirm wall".to_string());
            }
            return;
        }
    }

    if wall_preview.snapped_points.is_empty() || !wall_preview.valid {
        return;
    }

    let faction = active_player.0;
    let wall_pos = wall_preview.snapped_points[0];
    let worker_iter = workers.iter().map(|(e, tf, state, fac, kind)| (e, tf, state, fac, kind));
    let Some((worker_entity, _)) = find_best_worker_for_build(worker_iter, faction, wall_pos) else {
        placement.hint_text = Some("All workers are busy".to_string());
        return;
    };

    let player_res = all_resources.get(&faction);
    if !wall_preview.total_cost.can_afford(player_res) {
        placement.hint_text = Some("Not enough resources for wall".to_string());
        return;
    }
    wall_preview
        .total_cost
        .deduct(all_resources.get_mut(&faction));

    // Convert snapped world points back to grid cells, filtering out any newly blocked
    let cells: Vec<(i32, i32)> = wall_preview
        .snapped_points
        .iter()
        .map(|p| WallGrid::world_to_grid(*p))
        .filter(|(gx, gz)| !obstacle_grid.is_cell_blocked(*gx, *gz))
        .collect();
    if cells.is_empty() {
        clear_wall_preview(&mut commands, &mut wall_preview);
        placement.mode = PlacementMode::None;
        placement.hint_text = Some("Wall path blocked by trees".to_string());
        return;
    }

    let spawned_entities = spawn_wall_grid_cells(
        &mut commands,
        &cache,
        &registry,
        building_models.as_deref(),
        &height_map,
        &mut wall_grid,
        faction,
        &cells,
    );

    if !spawned_entities.is_empty() {
        // Clean up any existing gathering assignment before reassigning
        if let Ok((_, _, w_state, _, _)) = workers.get(worker_entity) {
            cleanup_worker_assignment(&mut commands, worker_entity, w_state);
        }
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

fn confirm_gate_plot(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut wall_grid: ResMut<WallGrid>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    ui_interactions: Query<&Interaction, With<Node>>,
    height_map: Res<HeightMap>,
    wall_segments: Query<
        (Entity, &Transform, &Faction, &WallGridCoord),
        (With<WallSegmentPiece>, With<Building>),
    >,
    registry: Res<BlueprintRegistry>,
    workers: Query<(Entity, &Transform, &UnitState, &Faction, &EntityKind), With<Unit>>,
) {
    let (camera_q, windows, graphics) = viewport;
    if placement.mode != PlacementMode::PlotGate || !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    if is_pointer_over_ui(&ui_interactions) {
        return;
    }

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) else {
        return;
    };

    // Find nearest owned straight wall segment
    let Some((segment_entity, segment_tf, faction, grid_coord)) = wall_segments
        .iter()
        .filter(|(_, _, faction, _)| **faction == active_player.0)
        .filter_map(|(entity, tf, faction, coord)| {
            let d = tf
                .translation
                .distance(Vec3::new(world_pos.x, tf.translation.y, world_pos.z));
            (d <= 6.0).then_some((entity, tf, faction, coord, d))
        })
        .min_by(|(_, _, _, _, a), (_, _, _, _, b)| {
            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(e, tf, faction, coord, _)| (e, tf, faction, coord))
    else {
        placement.hint_text = Some("Gatehouse must replace an owned wall segment".to_string());
        return;
    };

    let bp = registry.get(EntityKind::Gatehouse);
    let worker_entity = workers
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
        .map(|(worker, _, _, _, _)| worker);
    let Some(worker_entity) = worker_entity else {
        placement.hint_text = Some("All workers are busy".to_string());
        return;
    };

    let player_res = all_resources.get(faction);
    if !bp.cost.can_afford(player_res) {
        placement.hint_text = Some("Not enough resources for Gatehouse".to_string());
        return;
    }
    bp.cost.deduct(all_resources.get_mut(faction));

    // Mark the grid cell as a gate — auto-tile system will swap the model
    let (gx, gz) = (grid_coord.0, grid_coord.1);
    if let Some(cell) = wall_grid.cells.get_mut(&(gx, gz)) {
        cell.is_gate = true;
    }
    wall_grid.mark_dirty(gx, gz);

    if let Some(preview) = placement.preview_entity.take() {
        commands.entity(preview).try_despawn();
    }
    commands
        .entity(worker_entity)
        .remove::<AttackTarget>()
        .remove::<MoveTarget>()
        .insert(UnitState::MovingToBuild(segment_entity))
        .insert(TaskSource::Manual);
    commands
        .entity(segment_entity)
        .entry::<AssignedWorkers>()
        .and_modify(move |mut aw| {
            if !aw.workers.contains(&worker_entity) {
                aw.workers.push(worker_entity);
            }
        })
        .or_insert(AssignedWorkers {
            workers: vec![worker_entity],
        });

    placement.mode = PlacementMode::None;
    placement.awaiting_release = false;
    placement.hint_text = None;
}

fn confirm_floor_plot(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut floor_preview: ResMut<FloorPlotPreview>,
    mut floor_grid: ResMut<FloorGrid>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    ui_interactions: Query<&Interaction, With<Node>>,
    mut height_map: ResMut<HeightMap>,
    resources_and_queries: (
        Res<EntityVisualCache>,
        Res<BlueprintRegistry>,
        Res<ObstacleGrid>,
    ),
    ground_q: Query<&Mesh3d, With<Ground>>,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_and_decos: (
        ResMut<crate::ground::TerrainShapeSyncState>,
        ResMut<TerrainSurfaceDirtyQueue>,
    ),
    bush_decorations: Query<(Entity, &Transform), With<Decoration>>,
) {
    let (cache, registry, obstacle_grid) = resources_and_queries;
    let (mut sync_state, mut dirty_areas) = terrain_and_decos;
    let (camera_q, windows, graphics) = viewport;
    if !matches!(placement.mode, PlacementMode::PlotFloor) || placement.awaiting_release {
        return;
    }

    // Brush mode: paint while holding left mouse button
    if !mouse.pressed(MouseButton::Left) {
        return;
    }

    if is_pointer_over_ui(&ui_interactions) {
        return;
    }

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) else {
        return;
    };

    let (gx, gz) = WallGrid::world_to_grid(Vec3::new(world_pos.x, 0.0, world_pos.z));
    let world = WallGrid::grid_to_world(gx, gz);

    // Skip if already placed, blocked, or out of bounds
    if floor_grid.cells.contains_key(&(gx, gz)) {
        return;
    }
    if obstacle_grid.is_cell_blocked(gx, gz) {
        return;
    }
    let half_map = height_map.half_map;
    if world.x.abs() > half_map - 5.0 || world.z.abs() > half_map - 5.0 {
        return;
    }

    let faction = active_player.0;
    let cells = vec![(gx, gz)];
    let cell_cost = floor_cost_from_cells(&cells, &floor_grid, &registry);
    let player_res = all_resources.get(&faction);
    if !cell_cost.can_afford(player_res) {
        placement.hint_text = Some("Not enough resources".to_string());
        return;
    }
    cell_cost.deduct(all_resources.get_mut(&faction));

    let cx = world.x;
    let cz = world.z;
    let footprint = footprint_for_kind(EntityKind::Floor);
    let shared_height = height_map.foundation_target_height_shaped(cx, cz, footprint);

    // Flatten terrain for single cell
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
        faction,
        &cells,
        Some(shared_height),
    );

    // Paint floor stone texture on the ground mesh
    if let Ok(ground_mesh) = ground_q.single() {
        if let Some(mesh) = meshes.get_mut(&ground_mesh.0) {
            let floor_cell_half = WALL_CELL_SIZE * 0.5;
            let transition = WALL_CELL_SIZE * 0.8;
            crate::ground::paint_floor_blend_on_ground(
                mesh,
                &height_map,
                cx,
                cz,
                floor_cell_half,
                transition,
            );
        }
    }

    // Despawn bush decorations inside the painted cell
    let clear_r2 = (footprint + 2.0) * (footprint + 2.0);
    for (deco_entity, deco_tf) in &bush_decorations {
        let dx = deco_tf.translation.x - cx;
        let dz = deco_tf.translation.z - cz;
        if dx * dx + dz * dz <= clear_r2 {
            commands.entity(deco_entity).try_despawn();
        }
    }
}

fn cancel_placement(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut wall_preview: ResMut<WallPlotPreview>,
    mut floor_preview: ResMut<FloorPlotPreview>,
) {
    if placement.mode == PlacementMode::None {
        return;
    }

    if mouse.just_pressed(MouseButton::Right) || keyboard.just_pressed(KeyCode::Escape) {
        if let Some(preview) = placement.preview_entity {
            commands.entity(preview).try_despawn();
        }
        if let Some(grid) = placement.grid_plane_entity {
            commands.entity(grid).try_despawn();
        }
        clear_wall_preview(&mut commands, &mut wall_preview);
        clear_floor_preview(&mut commands, &mut floor_preview);
        placement.mode = PlacementMode::None;
        placement.preview_entity = None;
        placement.grid_plane_entity = None;
        placement.awaiting_release = false;
        placement.hint_text = None;
        placement.rotation_y = 0.0;
    }
}

// ── Worker arrives to plot building ──

fn pending_build_arrival_system(
    mut commands: Commands,
    mut workers: Query<(Entity, &Transform, &UnitState, &PendingBuildOrder), With<Unit>>,
    registry: Res<BlueprintRegistry>,
    existing_buildings: Query<
        (&Transform, &BuildingFootprint, &EntityKind),
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

        let flat_dist = Vec2::new(
            w_tf.translation.x - pending.position.x,
            w_tf.translation.z - pending.position.z,
        )
        .length();
        if flat_dist > plot_range {
            continue; // Still walking
        }

        let kind = pending.kind;
        let build_pos = pending.position;
        let new_footprint = footprint_for_kind(kind);

        // Final collision check — another building may have been placed in the meantime
        let blocked = existing_buildings
            .iter()
            .any(|(building_tf, existing_fp, existing_kind)| {
                if !blocks_construction_overlap(*existing_kind) {
                    return false;
                }
            let check_pos = Vec3::new(build_pos.x, building_tf.translation.y, build_pos.z);
            building_tf.translation.distance(check_pos) < existing_fp.0 + new_footprint
            });

        if blocked {
            let bp = registry.get(kind);
            let res = all_resources.get_mut(&pending.faction);
            for (rt, amt) in bp.cost.cost_entries() {
                res.add(rt, amt);
            }

            commands
                .entity(w_entity)
                .remove::<PendingBuildOrder>()
                .remove::<MoveTarget>()
                .insert(UnitState::Idle)
                .insert(TaskSource::Auto);
            continue;
        }

        commands
            .entity(w_entity)
            .remove::<PendingBuildOrder>()
            .insert(BuildSitePreparation {
                kind,
                position: build_pos,
                faction: pending.faction,
                rotation_y: pending.rotation_y,
                prep_timer: Timer::from_seconds(1.25, TimerMode::Once),
                vfx_timer: Timer::from_seconds(0.12, TimerMode::Repeating),
                burst_count: 0,
            })
            .insert(UnitState::MovingToPlot(build_pos))
            .insert(TaskSource::Manual);
    }
}

fn build_site_preparation_system(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    ghost_mats: Res<BuildingGhostMaterials>,
    height_map: Res<HeightMap>,
    building_models: Option<Res<BuildingModelAssets>>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut workers: Query<(Entity, &Transform, &UnitState, &mut BuildSitePreparation), With<Unit>>,
    existing_buildings: Query<
        (&Transform, &BuildingFootprint, &EntityKind),
        (With<Building>, Without<GhostBuilding>),
    >,
    mut all_resources: ResMut<AllPlayerResources>,
) {
    for (worker_entity, worker_tf, worker_state, mut prep) in &mut workers {
        if !matches!(*worker_state, UnitState::MovingToPlot(_)) {
            continue;
        }

        let flat_dist = Vec2::new(
            worker_tf.translation.x - prep.position.x,
            worker_tf.translation.z - prep.position.z,
        )
        .length();
        if flat_dist > 4.0 {
            commands
                .entity(worker_entity)
                .insert(MoveTarget(prep.position));
            continue;
        }

        prep.prep_timer.tick(time.delta());
        prep.vfx_timer.tick(time.delta());

        if prep.vfx_timer.just_finished() {
            spawn_foundation_prep_vfx(
                &mut commands,
                vfx_assets.as_deref(),
                &height_map,
                prep.position,
                footprint_for_kind(prep.kind),
                prep.burst_count,
            );
            prep.burst_count = prep.burst_count.wrapping_add(1);
        }

        if !prep.prep_timer.is_finished() {
            commands
                .entity(worker_entity)
                .insert(MoveTarget(prep.position));
            continue;
        }

        let new_footprint = footprint_for_kind(prep.kind);
        let blocked = existing_buildings
            .iter()
            .any(|(building_tf, existing_fp, existing_kind)| {
                if !blocks_construction_overlap(*existing_kind) {
                    return false;
                }
            let check_pos = Vec3::new(prep.position.x, building_tf.translation.y, prep.position.z);
            building_tf.translation.distance(check_pos) < existing_fp.0 + new_footprint
            });

        if blocked {
            let bp = registry.get(prep.kind);
            let res = all_resources.get_mut(&prep.faction);
            for (rt, amt) in bp.cost.cost_entries() {
                res.add(rt, amt);
            }

            commands
                .entity(worker_entity)
                .remove::<BuildSitePreparation>()
                .remove::<MoveTarget>()
                .insert(UnitState::Idle)
                .insert(TaskSource::Auto);
            continue;
        }

        let bp = registry.get(prep.kind);
        let is_gltf = bp.visual.mesh_kind.is_gltf();
        let rot_y = prep.rotation_y;
        let building_entity = spawn_from_blueprint_with_faction(
            &mut commands,
            &cache,
            prep.kind,
            prep.position,
            &registry,
            building_models.as_deref(),
            None,
            &height_map,
            prep.faction,
        );

        commands
            .entity(building_entity)
            .entry::<Transform>()
            .and_modify(move |mut tf| tf.rotation = Quat::from_rotation_y(rot_y));

        if !is_gltf {
            commands
                .entity(building_entity)
                .insert(MeshMaterial3d(ghost_mats.under_construction.clone()));
        }

        commands
            .entity(worker_entity)
            .remove::<BuildSitePreparation>()
            .remove::<MoveTarget>()
            .insert(UnitState::Building(building_entity))
            .insert(TaskSource::Manual);
    }
}

/// If a worker with a PendingBuildOrder dies or is reassigned, refund the building cost.
fn pending_build_cleanup_system(
    mut commands: Commands,
    removed: Query<(Entity, &PendingBuildOrder, &UnitState), With<Unit>>,
    preparing: Query<(Entity, &BuildSitePreparation, &UnitState), With<Unit>>,
    mut all_resources: ResMut<AllPlayerResources>,
    registry: Res<BlueprintRegistry>,
) {
    for (entity, pending, state) in &removed {
        // If the worker is no longer in MovingToPlot state, the order was interrupted
        if !matches!(state, UnitState::MovingToPlot(_)) {
            let bp = registry.get(pending.kind);
            let res = all_resources.get_mut(&pending.faction);
            for (rt, amt) in bp.cost.cost_entries() {
                res.add(rt, amt);
            }

            commands.entity(entity).remove::<PendingBuildOrder>();
        }
    }

    for (entity, prep, state) in &preparing {
        if matches!(state, UnitState::MovingToPlot(_)) {
            continue;
        }

        let bp = registry.get(prep.kind);
        let res = all_resources.get_mut(&prep.faction);
        for (rt, amt) in bp.cost.cost_entries() {
            res.add(rt, amt);
        }

        commands.entity(entity).remove::<BuildSitePreparation>();
    }
}

fn spawn_foundation_prep_vfx(
    commands: &mut Commands,
    vfx_assets: Option<&VfxAssets>,
    height_map: &HeightMap,
    position: Vec3,
    footprint: f32,
    burst_count: u8,
) {
    let Some(vfx) = vfx_assets else {
        return;
    };

    let burst_seed = burst_count as f32 * 0.73;
    let particle_count = 6usize;
    for idx in 0..particle_count {
        let t = idx as f32 / particle_count as f32;
        let angle = burst_seed + t * std::f32::consts::TAU;
        let radius = footprint * (0.35 + 0.45 * ((burst_count as f32 * 0.27 + t).fract()));
        let world_x = position.x + angle.cos() * radius;
        let world_z = position.z + angle.sin() * radius;
        let ground_y = height_map.sample(world_x, world_z);
        let outward = Vec3::new(angle.cos(), 0.0, angle.sin());
        let lift = 1.6 + t * 0.6;

        commands.spawn((
            GatherParticle {
                velocity: outward * 1.6 + Vec3::Y * lift,
                timer: Timer::from_seconds(0.55 + t * 0.2, TimerMode::Once),
                start_scale: 0.16 + t * 0.06,
            },
            FogHideable::Vfx,
            Mesh3d(vfx.cube_mesh.clone()),
            MeshMaterial3d(vfx.dust_material.clone()),
            Transform::from_translation(Vec3::new(world_x, ground_y + 0.15, world_z)),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

// ── Construction ──

fn construction_progress_system(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    building_models: Res<crate::model_assets::BuildingModelAssets>,
    construction_assets: Res<crate::model_assets::BuildingConstructionAssets>,
    mut base_state: ResMut<FactionBaseState>,
    mut buildings: Query<(
        Entity,
        &EntityKind,
        &mut BuildingState,
        &mut ConstructionProgress,
        Option<&mut ConstructionStage>,
        &mut Transform,
        &Faction,
    )>,
    workers: Query<&UnitState, With<Unit>>,
    children_q: Query<&Children>,
    scene_child_q: Query<Entity, With<BuildingSceneChild>>,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
    mut sfx: bevy::ecs::message::MessageWriter<PlaySfx>,
    mut hammer_timer: Local<f32>,
) {
    const HAMMER_INTERVAL: f32 = 1.5;
    *hammer_timer += time.delta_secs();
    let play_hammer = *hammer_timer >= HAMMER_INTERVAL;
    if play_hammer {
        *hammer_timer = 0.0;
    }

    for (entity, kind, mut state, mut progress, construction_stage, mut transform, faction) in
        &mut buildings
    {
        if *state != BuildingState::UnderConstruction {
            continue;
        }

        // Count workers actively building this entity
        let builder_count = workers
            .iter()
            .filter(|state| matches!(state, UnitState::Building(e) if *e == entity))
            .count();

        if builder_count == 0 {
            progress.timer.pause();
            continue;
        }

        // Unpause when workers are present
        progress.timer.unpause();
        let mut speed_mult = 1.0 + 0.5 * (builder_count as f32 - 1.0);
        if is_wall_like_kind(*kind) {
            speed_mult *= 2.0;
        }

        let bp = registry.get(*kind);
        let base_scale = bp.visual.scale;
        let is_gltf = bp.visual.mesh_kind.is_gltf();

        progress
            .timer
            .tick(Duration::from_secs_f32(time.delta_secs() * speed_mult));

        let fraction = progress.timer.fraction();

        if play_hammer && !progress.timer.is_finished() {
            sfx.write(PlaySfx {
                kind: SfxKind::ConstructionHammer,
                position: Some(transform.translation),
            });
        }

        // For GLTF buildings: swap construction stage models at thresholds
        if is_gltf {
            let desired_stage = if fraction >= 1.0 {
                2 // complete
            } else if fraction >= 0.5 {
                1 // partial
            } else {
                0 // foundation
            };

            let current = construction_stage.as_ref().map(|s| s.0).unwrap_or(255);
            if current != desired_stage && desired_stage < 2 {
                // Swap scene child to construction stage model
                if let Some(stage_scene) = construction_assets.stages.get(&(*kind, desired_stage)) {
                    // Remove old scene child
                    if let Ok(children) = children_q.get(entity) {
                        for child in children.iter() {
                            if scene_child_q.contains(child) {
                                commands.entity(child).try_despawn();
                            }
                        }
                    }

                    let mut child = commands
                        .spawn((
                            SceneRoot(stage_scene.clone()),
                            BuildingSceneChild,
                            building_models.child_transform(*kind, base_scale),
                        ));
                    #[cfg(not(target_arch = "wasm32"))]
                    child.insert((InheritOutline, AsyncSceneInheritOutline::default()));
                    let child = child.id();
                    commands.entity(entity).add_child(child);
                    commands
                        .entity(entity)
                        .insert(ConstructionStage(desired_stage));
                }
            }
        } else {
            // Non-GLTF: legacy scale lerp
            let current_scale = 0.3 * base_scale + (base_scale - 0.3 * base_scale) * fraction;
            transform.scale = Vec3::splat(current_scale);
        }

        if progress.timer.is_finished() {
            *state = BuildingState::Complete;

            if is_gltf {
                // Swap to final complete building model
                if let Ok(children) = children_q.get(entity) {
                    for child in children.iter() {
                        if scene_child_q.contains(child) {
                            commands.entity(child).try_despawn();
                        }
                    }
                }

                if let Some(complete_scene) =
                    building_models.scene_for(*kind, 1, transform.translation)
                {
                    let mut child = commands
                        .spawn((
                            SceneRoot(complete_scene),
                            BuildingSceneChild,
                            building_models.child_transform(*kind, base_scale),
                        ));
                    #[cfg(not(target_arch = "wasm32"))]
                    child.insert((InheritOutline, AsyncSceneInheritOutline::default()));
                    let child = child.id();
                    commands.entity(entity).add_child(child);
                }
                commands
                    .entity(entity)
                    .insert(ConstructionStage(2))
                    .remove::<TeamColorApplied>();
            } else {
                transform.scale = Vec3::splat(base_scale);
                if let Some(mat) = cache.materials_default.get(kind) {
                    commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                }
            }

            if *kind == EntityKind::Base {
                base_state.set_founded(*faction, true);
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
                        total_trained: 0,
                    });
                }
            }

            sfx.write(PlaySfx {
                kind: SfxKind::ConstructionComplete,
                position: Some(transform.translation),
            });

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
    projectile_assets: Option<Res<crate::model_assets::ProjectileModelAssets>>,
    net_role: Res<crate::multiplayer::NetRole>,
    active_player: Res<ActivePlayer>,
    mut towers: Query<
        (
            Entity,
            &Transform,
            &EntityKind,
            &BuildingState,
            &mut AttackCooldown,
            &AttackDamage,
            &AttackRange,
            &AttackProfile,
            &CombatFxKind,
            Option<&AttackTiming>,
            Option<&TowerAutoAttackEnabled>,
            &Faction,
            Option<&TargetingProfile>,
            Option<&DamageType>,
        ),
        With<Building>,
    >,
    mut hostiles: Query<
        (
            Entity,
            &Transform,
            &Faction,
            &Health,
            &ArmorType,
            Option<&ThreatValue>,
            Option<&mut ReservedIncomingDamage>,
        ),
        Or<(With<Mob>, With<Unit>)>,
    >,
) {
    let Some(vfx) = vfx_assets else { return };

    for (
        tower_entity,
        tower_tf,
        kind,
        state,
        mut cooldown,
        damage,
        range,
        attack_profile,
        fx_kind,
        attack_timing,
        auto_attack,
        tower_faction,
        opt_profile,
        opt_dmg_type,
    ) in &mut towers
    {
        if !kind.uses_tower_auto_attack() || *state != BuildingState::Complete {
            continue;
        }

        // Client: only run tower attacks for local player's towers
        if *net_role == crate::multiplayer::NetRole::Client && *tower_faction != active_player.0 {
            continue;
        }

        // Check if auto-attack is disabled
        if let Some(enabled) = auto_attack {
            if !enabled.0 {
                continue;
            }
        }

        cooldown.ready_in = (cooldown.ready_in - time.delta_secs()).max(0.0);
        if cooldown.ready_in > 0.0 {
            continue;
        }

        let mut best_score = f32::MAX;
        let mut best_target: Option<(Entity, f32)> = None; // (entity, travel_dist)
        let tower_dmg_type = opt_dmg_type.copied().unwrap_or(DamageType::Pierce);
        let minimum_range = attack_timing.map_or(0.0, |timing| timing.minimum_range);

        for (target_entity, target_tf, target_faction, t_health, t_armor, t_threat, t_reserved) in
            hostiles.iter()
        {
            if !teams.is_hostile(tower_faction, target_faction) {
                continue;
            }

            let surface_dist =
                crate::combat::attack_surface_distance(tower_tf.translation, target_tf.translation, 0.0);
            if !crate::combat::is_in_attack_band(surface_dist, range.0, minimum_range, 0.1) {
                continue;
            }

            if let Some(profile) = opt_profile {
                let reserved_total = t_reserved.as_ref().map_or(0.0, |r| r.total());
                if let Some(score) = crate::combat::target_score(
                    &crate::combat::TargetScoreInput {
                        profile,
                        attacker_pos: tower_tf.translation,
                        attacker_damage_type: tower_dmg_type,
                        scan_range: range.0,
                        target_pos: target_tf.translation,
                        target_health: t_health,
                        target_armor: *t_armor,
                        target_threat: t_threat.map_or(0.0, |t| t.0),
                        target_is_building: false,
                        target_reserved_damage: reserved_total,
                    },
                ) {
                    if score < best_score {
                        best_score = score;
                        best_target = Some((target_entity, surface_dist));
                    }
                }
            } else {
                // Fallback: nearest enemy
                if surface_dist < best_score {
                    best_score = surface_dist;
                    best_target = Some((target_entity, surface_dist));
                }
            }
        }

        if let Some((target_entity, travel_dist)) = best_target {
            cooldown.ready_in = cooldown.interval;

            // Add damage reservation on target
            let projectile_speed = attack_profile.projectile_speed.max(8.0);
            let ttl = travel_dist / projectile_speed + attack_profile.windup_secs + 0.35;
            if let Ok((_, _, _, _, _, _, Some(mut reserved))) = hostiles.get_mut(target_entity) {
                reserved.reservations.push((tower_entity, damage.0, ttl));
            }

            let proj_visual =
                crate::model_assets::projectile_visual_for(*kind);
            let orient = proj_visual.is_some()
                && !matches!(
                    proj_visual,
                    Some(crate::model_assets::ProjectileVisualKind::CatapultRock)
                );
            let spawn_pos = tower_tf.translation + Vec3::Y * 3.0;
            let proj_component = Projectile {
                source: tower_entity,
                target: target_entity,
                speed: projectile_speed,
                damage: damage.0,
                damage_type: tower_dmg_type,
                fx_kind: *fx_kind,
                impact_scale: attack_profile.impact_scale,
                orient_to_velocity: orient,
            };
            if let (Some(visual_kind), Some(ref proj_res)) = (proj_visual, &projectile_assets) {
                let target_tf = hostiles
                    .get(target_entity)
                    .map(|(_, tf, ..)| tf.translation)
                    .unwrap_or(spawn_pos);
                let dir = (target_tf - spawn_pos).normalize_or_zero();
                let proj_scale = match visual_kind {
                    crate::model_assets::ProjectileVisualKind::Arrow => 0.35,
                    crate::model_assets::ProjectileVisualKind::Bolt => 0.4,
                    crate::model_assets::ProjectileVisualKind::CatapultRock => 0.5,
                };
                let rotation = if orient {
                    Quat::from_rotation_arc(Vec3::Z, dir)
                } else {
                    Quat::IDENTITY
                };
                let scene =
                    proj_res.scene_for(visual_kind, tower_entity.to_bits() as usize);
                commands.spawn((
                    proj_component,
                    SceneRoot(scene),
                    Transform::from_translation(spawn_pos)
                        .with_rotation(rotation)
                        .with_scale(Vec3::splat(proj_scale)),
                ));
            } else {
                commands.spawn((
                    proj_component,
                    Mesh3d(vfx.sphere_mesh.clone()),
                    MeshMaterial3d(vfx.projectile_material.clone()),
                    Transform::from_translation(spawn_pos)
                        .with_scale(Vec3::splat(0.2)),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            };
        }
    }
}

// ── Training ──

fn training_queue_system(
    mut commands: Commands,
    time: Res<Time>,
    net_role: Res<crate::multiplayer::NetRole>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    unit_models: Option<Res<UnitModelAssets>>,
    height_map: Res<HeightMap>,
    unit_factions: Query<&Faction, With<Unit>>,
    cap_buildings: Query<(&Faction, &EntityKind, &BuildingState, &BuildingLevel), With<Building>>,
    mut buildings: Query<
        (
            &Transform,
            &EntityKind,
            &mut TrainingQueue,
            Option<&RallyPoint>,
            &Faction,
            &BuildingLevel,
        ),
        With<Building>,
    >,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
) {
    if *net_role == crate::multiplayer::NetRole::Client {
        return;
    }

    let mut used_by_faction: std::collections::HashMap<Faction, u32> =
        std::collections::HashMap::new();
    for faction in &unit_factions {
        *used_by_faction.entry(*faction).or_default() += 1;
    }

    let mut cap_by_faction: std::collections::HashMap<Faction, u32> = Faction::PLAYERS
        .into_iter()
        .map(|faction| (faction, DEFAULT_UNIT_CAP))
        .collect();
    for (faction, kind, state, level) in &cap_buildings {
        if *state != BuildingState::Complete {
            continue;
        }
        *cap_by_faction.entry(*faction).or_default() +=
            unit_capacity_bonus_for_building(*kind, level.0);
    }

    for (transform, building_kind, mut queue, rally_point, building_faction, building_level) in
        &mut buildings
    {
        if queue.queue.is_empty() {
            continue;
        }

        // Start timer for first item if not started
        if queue.timer.is_none() {
            let unit_kind = queue.queue[0];
            let bp = registry.get(unit_kind);
            let mut train_secs = bp.train_time_secs;

            // Apply TrainTimeMultiplier from building level bonuses
            let building_bp = registry.get(*building_kind);
            if let Some(ref bd) = building_bp.building {
                for (i, ld) in bd.level_upgrades.iter().enumerate() {
                    if (i as u8 + 2) <= building_level.0 {
                        if let LevelBonus::TrainTimeMultiplier(mult) = ld.bonus {
                            train_secs *= mult;
                        }
                    }
                }
            }

            queue.timer = Some(Timer::from_seconds(train_secs, TimerMode::Once));
        }

        if let Some(ref mut timer) = queue.timer {
            timer.tick(time.delta());
            if timer.is_finished() {
                let used = used_by_faction.get(building_faction).copied().unwrap_or(0);
                let cap = cap_by_faction
                    .get(building_faction)
                    .copied()
                    .unwrap_or(DEFAULT_UNIT_CAP);
                if used >= cap {
                    continue;
                }

                let unit_kind = queue.queue.remove(0);

                // Scatter spawn positions around the building to avoid stacking
                let spawn_index = queue.total_trained;
                queue.total_trained = queue.total_trained.wrapping_add(1);
                let angle = std::f32::consts::TAU * (spawn_index as f32 * 0.618034); // golden angle
                let radius = 3.5;
                let offset = Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
                let spawn_pos = transform.translation + offset;

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

                *used_by_faction.entry(*building_faction).or_default() += 1;

                // If building has a rally point, set UnitState::Moving so the
                // unit actually walks there (plain MoveTarget gets stripped by
                // the unit-state executor when state is Idle).
                if let Some(rally) = rally_point {
                    commands
                        .entity(unit_entity)
                        .insert(MoveTarget(rally.0))
                        .insert(UnitState::Moving(rally.0));
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
        if *kind == EntityKind::Base {
            founded.insert(*faction, true);
        }

        if *state == BuildingState::Complete && kind.category() == EntityCategory::Building {
            let list = per_faction.entry(*faction).or_default();
            if !list.contains(kind) {
                list.push(*kind);
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
    let deficits = level_data.cost.deduct_with_carried(player_res);
    let drain = SpendFromCarried {
        faction,
        amounts: deficits,
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
        mut storage_inv,
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
                if let Some(new_scene) = models.scene_for(*kind, new_level, transform.translation) {
                    // Despawn old scene child
                    if let Ok(children) = children_q.get(entity) {
                        for child in children.iter() {
                            if scene_child_q.contains(child) {
                                commands.entity(child).try_despawn();
                            }
                        }
                    }
                    // Spawn new scene child with calibration
                    let mut child = commands
                        .spawn((
                            SceneRoot(new_scene),
                            BuildingSceneChild,
                            models.child_transform(*kind, 1.0),
                        ));
                    #[cfg(not(target_arch = "wasm32"))]
                    child.insert((InheritOutline, AsyncSceneInheritOutline::default()));
                    let child = child.id();
                    commands.entity(entity).add_child(child);
                    // Remove TeamColorApplied so the new scene gets recolored
                    commands.entity(entity).remove::<TeamColorApplied>();
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
                // Grant storage capacity for newly unlocked resource types
                if let Some(ref mut inv) = storage_inv {
                    for rt in unlock_resources {
                        if inv.caps[rt.index()] == 0 {
                            inv.caps[rt.index()] = 500;
                        }
                    }
                }
            }
            LevelBonus::UnlockRecipe {
                recipe_index: _,
                extra_worker_slots,
            } => {
                // Recipe unlock is checked at runtime via building level vs requires_level
                if let Some(mut proc) = processor {
                    proc.max_workers += extra_worker_slots;
                }
            }
            LevelBonus::ProductionSpeedMultiplier(_mult) => {
                // Applied at runtime in production_chain_system by checking building level
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
                        rise_speed: 0.7,
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
    mut wall_grid: ResMut<WallGrid>,
    mut floor_grid: ResMut<FloorGrid>,
    grid_coord_q: Query<&WallGridCoord>,
    floor_coord_q: Query<&FloorGridCoord>,
    yard_q: Query<&SawmillYard>,
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
            for (rt, amt) in cost.cost_entries() {
                res.add(rt, amt / 2);
            }

            // Remove from wall grid if this was a wall piece
            if let Ok(coord) = grid_coord_q.get(entity) {
                let (gx, gz) = (coord.0, coord.1);
                wall_grid.cells.remove(&(gx, gz));
                // Mark neighbors dirty so they re-tile
                for (nx, nz) in WallGrid::cardinal_neighbors(gx, gz) {
                    wall_grid.dirty.push((nx, nz));
                }
            }

            if let Ok(coord) = floor_coord_q.get(entity) {
                floor_grid.cells.remove(&(coord.0, coord.1));
                floor_grid.mark_dirty(coord.0, coord.1);
            }

            // Clean up sawmill yard entities
            if let Ok(yard) = yard_q.get(entity) {
                for &e in yard.fence_entities.iter().chain(yard.tree_entities.iter()) {
                    commands.entity(e).try_despawn();
                }
            }

            // Despawn
            commands.entity(entity).try_despawn();
        }
    }
}

// ── Building Scale Animation ──

fn building_scale_anim_system(
    mut commands: Commands,
    time: Res<Time>,
    mut buildings: Query<(Entity, &mut Transform, &mut BuildingScaleAnim), Without<FrustumCulled>>,
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

// ── Sawmill Tree Yard ──

// Yard center is placed east of the sawmill, outside the 3.0 footprint
pub const SAWMILL_YARD_OFFSET: Vec3 = Vec3::new(5.5, 0.0, 0.0);
const SAWMILL_YARD_HALF_X: f32 = 2.0;
const SAWMILL_YARD_HALF_Z: f32 = 2.0;
const SAWMILL_MINI_TREE_SCALE: f32 = 0.06;

fn sawmill_yard_max_trees(level: u8) -> u8 {
    // Level 1: 2 trees, Level 2: 3, Level 3: 4
    1 + level
}

pub const SAWMILL_TREE_SLOTS: [Vec3; 4] = [
    Vec3::new(-0.8, 0.0, -0.8),
    Vec3::new(0.8, 0.0, -0.8),
    Vec3::new(-0.8, 0.0, 0.8),
    Vec3::new(0.8, 0.0, 0.8),
];

fn sawmill_yard_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    height_map: Res<HeightMap>,
    model_assets: Option<Res<ModelAssets>>,
    new_sawmills: Query<
        (Entity, &Transform, &BuildingLevel, &BuildingState),
        (
            With<Building>,
            Without<SawmillYard>,
        ),
    >,
    mut upgraded_sawmills: Query<
        (Entity, &Transform, &BuildingLevel, &mut SawmillYard),
        (With<Building>, Changed<BuildingLevel>),
    >,
    kind_q: Query<&EntityKind>,
) {
    let fence_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.35, 0.22, 0.1),
        perceptual_roughness: 0.9,
        ..default()
    });

    // Spawn yard for newly completed sawmills
    for (entity, transform, level, state) in &new_sawmills {
        if *state != BuildingState::Complete {
            continue;
        }
        let Ok(kind) = kind_q.get(entity) else { continue };
        if *kind != EntityKind::Sawmill {
            continue;
        }

        let center = transform.translation + SAWMILL_YARD_OFFSET;
        let mut fence_entities = Vec::new();

        // Fence corner positions (local to yard center)
        let corners = [
            Vec3::new(-SAWMILL_YARD_HALF_X, 0.0, -SAWMILL_YARD_HALF_Z),
            Vec3::new(SAWMILL_YARD_HALF_X, 0.0, -SAWMILL_YARD_HALF_Z),
            Vec3::new(SAWMILL_YARD_HALF_X, 0.0, SAWMILL_YARD_HALF_Z),
            Vec3::new(-SAWMILL_YARD_HALF_X, 0.0, SAWMILL_YARD_HALF_Z),
        ];

        let post_mesh = meshes.add(Cylinder::new(0.05, 0.8));

        // Spawn fence posts at corners
        for local_pos in corners.iter() {
            let world_pos = center + *local_pos;
            let ground_y = height_map.sample(world_pos.x, world_pos.z);
            let post = commands
                .spawn((
                    Mesh3d(post_mesh.clone()),
                    MeshMaterial3d(fence_mat.clone()),
                    Transform::from_translation(Vec3::new(
                        world_pos.x,
                        ground_y + 0.4,
                        world_pos.z,
                    )),
                    NotShadowCaster,
                    NotShadowReceiver,
                    GameWorld,
                ))
                .id();
            fence_entities.push(post);
        }

        // Spawn fence rails between consecutive corners
        let rail_height_offsets = [0.25, 0.55]; // two horizontal rails
        for i in 0..4 {
            let a = corners[i];
            let b = corners[(i + 1) % 4];
            let mid = (a + b) * 0.5;
            let span = (b - a).length();
            let world_mid = center + mid;
            let ground_y = height_map.sample(world_mid.x, world_mid.z);

            // Determine rotation: rails along X or Z
            let angle = if (b.z - a.z).abs() > (b.x - a.x).abs() {
                std::f32::consts::FRAC_PI_2
            } else {
                0.0
            };

            for &h in &rail_height_offsets {
                let rail_mesh = meshes.add(Cuboid::new(span, 0.06, 0.06));
                let rail = commands
                    .spawn((
                        Mesh3d(rail_mesh),
                        MeshMaterial3d(fence_mat.clone()),
                        Transform::from_translation(Vec3::new(
                            world_mid.x,
                            ground_y + h,
                            world_mid.z,
                        ))
                        .with_rotation(Quat::from_rotation_y(angle)),
                        NotShadowCaster,
                        NotShadowReceiver,
                        GameWorld,
                    ))
                    .id();
                fence_entities.push(rail);
            }
        }

        // Spawn initial mini trees (as harvestable ResourceNodes)
        let max_trees = sawmill_yard_max_trees(level.0);
        let mut tree_entities = Vec::new();
        spawn_mini_trees(
            &mut commands,
            &height_map,
            model_assets.as_deref(),
            center,
            0,
            max_trees,
            &mut tree_entities,
            entity,
        );

        commands.entity(entity).insert(SawmillYard {
            fence_entities,
            tree_entities,
            current_tree_count: max_trees,
        });
    }

    // Handle level upgrades — add more trees
    for (entity, transform, level, mut yard) in &mut upgraded_sawmills {
        let Ok(kind) = kind_q.get(entity) else { continue };
        if *kind != EntityKind::Sawmill {
            continue;
        }

        let new_max = sawmill_yard_max_trees(level.0);
        if new_max <= yard.current_tree_count {
            continue;
        }

        let center = transform.translation + SAWMILL_YARD_OFFSET;
        spawn_mini_trees(
            &mut commands,
            &height_map,
            model_assets.as_deref(),
            center,
            yard.current_tree_count,
            new_max,
            &mut yard.tree_entities,
            entity,
        );
        yard.current_tree_count = new_max;
    }
}

const YARD_TREE_WOOD_AMOUNT: u32 = 80;

fn spawn_mini_trees(
    commands: &mut Commands,
    height_map: &HeightMap,
    model_assets: Option<&ModelAssets>,
    yard_center: Vec3,
    from_slot: u8,
    to_slot: u8,
    tree_entities: &mut Vec<Entity>,
    sawmill_entity: Entity,
) {
    let Some(assets) = model_assets else { return };
    if assets.trees.is_empty() {
        return;
    }

    for slot_idx in from_slot..to_slot.min(SAWMILL_TREE_SLOTS.len() as u8) {
        let local = SAWMILL_TREE_SLOTS[slot_idx as usize];
        let world_pos = yard_center + local;
        let ground_y = height_map.sample(world_pos.x, world_pos.z);

        // Pick a random tree variant based on slot index for determinism
        let tree_idx = slot_idx as usize % assets.trees.len();
        let scene = assets.trees[tree_idx].clone();
        let y_rotation = (slot_idx as f32) * 1.57; // vary rotation per slot

        let tree = commands
            .spawn((
                SceneRoot(scene),
                Transform::from_translation(Vec3::new(world_pos.x, ground_y, world_pos.z))
                    .with_rotation(Quat::from_rotation_y(y_rotation))
                    .with_scale(Vec3::splat(SAWMILL_MINI_TREE_SCALE)),
                NotShadowCaster,
                GameWorld,
                ResourceNode {
                    resource_type: ResourceType::Wood,
                    amount_remaining: YARD_TREE_WOOD_AMOUNT,
                },
                YardResourceNode(sawmill_entity),
            ))
            .id();
        tree_entities.push(tree);
    }
}

// ── Yard tree regrowth ──

const YARD_TREE_REGROW_SECS: f32 = 20.0;

/// Regrows depleted yard trees over time so the sawmill has a renewable wood supply.
fn yard_tree_regrowth_system(
    time: Res<Time>,
    sawmills: Query<(&SawmillYard, &BuildingState), With<Building>>,
    mut nodes: Query<&mut ResourceNode, With<YardResourceNode>>,
    mut timers: Local<std::collections::HashMap<Entity, f32>>,
) {
    for (yard, state) in &sawmills {
        if *state != BuildingState::Complete {
            continue;
        }
        for &tree_entity in &yard.tree_entities {
            let Ok(mut node) = nodes.get_mut(tree_entity) else {
                continue;
            };
            if node.amount_remaining > 0 {
                timers.remove(&tree_entity);
                continue;
            }
            let elapsed = timers.entry(tree_entity).or_insert(0.0);
            *elapsed += time.delta_secs();
            if *elapsed >= YARD_TREE_REGROW_SECS {
                node.amount_remaining = YARD_TREE_WOOD_AMOUNT;
                timers.remove(&tree_entity);
            }
        }
    }
}

// ── Vegetation clearing around buildings ──

/// Removes grass and decoration geometry within a building's footprint.
///
/// Operates on merged chunk meshes: for each triangle whose centroid falls
/// inside the clear radius, the triangle's indices are dropped and the mesh
/// is rebuilt.  Empty chunks are despawned entirely.
/// Max buildings to clear vegetation for per frame — spreads GPU re-upload
/// cost when many buildings spawn at once (e.g. AI wall lines).
const VEGETATION_CLEAR_BUDGET: usize = 2;
/// Chunk world-space size used for AABB pre-filtering (must match resources.rs).
const VEG_CHUNK_SIZE: f32 = 32.0;

fn clear_vegetation_around_buildings(
    mut commands: Commands,
    new_buildings: Query<
        (Entity, &Transform, &BuildingFootprint),
        (With<Building>, Without<VegetationCleared>),
    >,
    grass_chunks: Query<(Entity, &GrassChunk, &Mesh3d)>,
    deco_chunks: Query<
        (Entity, &DecoChunk, &Transform, &Mesh3d),
        (Without<Building>, Without<Sapling>, Without<GrowingTree>, Without<MatureTree>),
    >,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mut processed = 0;
    for (building_entity, building_tf, footprint) in &new_buildings {
        if processed >= VEGETATION_CLEAR_BUDGET {
            break;
        }
        processed += 1;

        let bx = building_tf.translation.x;
        let bz = building_tf.translation.z;
        // Clear a bit beyond the footprint so there is a visible gap.
        clear_vegetation_in_radius(
            &mut commands,
            &grass_chunks,
            &deco_chunks,
            &mut meshes,
            bx,
            bz,
            footprint.0 + 2.0,
        );

        commands.entity(building_entity).insert(VegetationCleared);
    }
}

fn clear_vegetation_in_radius(
    commands: &mut Commands,
    grass_chunks: &Query<(Entity, &GrassChunk, &Mesh3d)>,
    deco_chunks: &Query<
        (Entity, &DecoChunk, &Transform, &Mesh3d),
        (Without<Building>, Without<Sapling>, Without<GrowingTree>, Without<MatureTree>),
    >,
    meshes: &mut Assets<Mesh>,
    bx: f32,
    bz: f32,
    clear_radius: f32,
) {
    let clear_r2 = clear_radius * clear_radius;

    // Grass chunks store vertices directly in world space.
    for (chunk_entity, chunk, mesh_handle) in grass_chunks.iter() {
        let chunk_cx = (chunk.chunk_x as f32 + 0.5) * VEG_CHUNK_SIZE;
        let chunk_cz = (chunk.chunk_z as f32 + 0.5) * VEG_CHUNK_SIZE;
        let half = VEG_CHUNK_SIZE * 0.5 + clear_radius;
        if (bx - chunk_cx).abs() > half || (bz - chunk_cz).abs() > half {
            continue;
        }

        let needs_strip = {
            let Some(mesh) = meshes.get(&mesh_handle.0) else {
                continue;
            };
            has_triangles_in_radius(mesh, bx, bz, clear_r2, 0.0, 0.0)
        };
        if !needs_strip {
            continue;
        }

        let Some(mesh) = meshes.get_mut(&mesh_handle.0) else {
            continue;
        };
        if strip_triangles_in_radius(mesh, bx, bz, clear_r2, 0.0, 0.0) {
            commands.entity(chunk_entity).despawn();
        }
    }

    // Deco chunks store vertices relative to the chunk transform.
    for (chunk_entity, chunk, chunk_tf, mesh_handle) in deco_chunks.iter() {
        let ox = chunk_tf.translation.x;
        let oz = chunk_tf.translation.z;
        let chunk_cx = (chunk.chunk_x as f32 + 0.5) * VEG_CHUNK_SIZE;
        let chunk_cz = (chunk.chunk_z as f32 + 0.5) * VEG_CHUNK_SIZE;
        let half = VEG_CHUNK_SIZE * 0.5 + clear_radius;
        if (bx - chunk_cx).abs() > half || (bz - chunk_cz).abs() > half {
            continue;
        }

        let needs_strip = {
            let Some(mesh) = meshes.get(&mesh_handle.0) else {
                continue;
            };
            has_triangles_in_radius(mesh, bx, bz, clear_r2, ox, oz)
        };
        if !needs_strip {
            continue;
        }

        let Some(mesh) = meshes.get_mut(&mesh_handle.0) else {
            continue;
        };
        if strip_triangles_in_radius(mesh, bx, bz, clear_r2, ox, oz) {
            commands.entity(chunk_entity).despawn();
        }
    }
}

/// Read-only check: are there any triangles whose centroid falls within radius?
/// Does NOT clone data or mutate the mesh — much cheaper than strip_triangles_in_radius.
fn has_triangles_in_radius(
    mesh: &Mesh,
    cx: f32,
    cz: f32,
    radius_sq: f32,
    offset_x: f32,
    offset_z: f32,
) -> bool {
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return false;
    };
    let Some(bevy::mesh::Indices::U32(indices)) = mesh.indices() else {
        return false;
    };
    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
            continue;
        }
        let ax = (positions[i0][0] + positions[i1][0] + positions[i2][0]) / 3.0 + offset_x;
        let az = (positions[i0][2] + positions[i1][2] + positions[i2][2]) / 3.0 + offset_z;
        let dx = ax - cx;
        let dz = az - cz;
        if dx * dx + dz * dz <= radius_sq {
            return true;
        }
    }
    false
}

fn strip_triangles_in_radius(
    mesh: &mut Mesh,
    cx: f32,
    cz: f32,
    radius_sq: f32,
    offset_x: f32,
    offset_z: f32,
) -> bool {
    let positions: Vec<[f32; 3]> = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(bevy::mesh::VertexAttributeValues::Float32x3(v)) => v.clone(),
        _ => return false,
    };

    let old_indices: Vec<u32> = match mesh.indices() {
        Some(bevy::mesh::Indices::U32(v)) => v.clone(),
        _ => return false,
    };

    if old_indices.len() % 3 != 0 {
        return false;
    }

    let mut new_indices = Vec::with_capacity(old_indices.len());
    for tri in old_indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
            new_indices.extend_from_slice(tri);
            continue;
        }
        let ax = (positions[i0][0] + positions[i1][0] + positions[i2][0]) / 3.0 + offset_x;
        let az = (positions[i0][2] + positions[i1][2] + positions[i2][2]) / 3.0 + offset_z;
        let dx = ax - cx;
        let dz = az - cz;
        if dx * dx + dz * dz > radius_sq {
            new_indices.extend_from_slice(tri);
        }
    }

    if new_indices.is_empty() {
        return true;
    }

    mesh.insert_indices(bevy::mesh::Indices::U32(new_indices));
    false
}
