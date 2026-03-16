use bevy::ecs::entity::Entities;
use bevy::prelude::*;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use crate::components::*;
use crate::ground::HeightMap;
use crate::spatial::WallSpatialGrid;

// ── Constants ──

const NAV_GRID_STEP: f32 = 2.5;
const ASTAR_NODE_LIMIT: usize = 12_000;
const PATHS_PER_FRAME: usize = 12;
/// Distance threshold below which we skip A* and just walk directly
const DIRECT_MOVE_THRESHOLD: f32 = 3.0;
/// Cost for cells near obstacles (soft margin — guides paths away but doesn't block)
const OBSTACLE_MARGIN_COST: u8 = 40;

// ── Components ──

/// A sequence of waypoints produced by A* pathfinding.
#[derive(Component)]
pub struct NavPath {
    pub waypoints: Vec<Vec3>,
    pub current_index: usize,
    /// The final destination (same as last waypoint)
    pub destination: Vec3,
}

/// Marker: skip pathfinding for this MoveTarget (short-range adjustment)
#[derive(Component)]
pub struct NavDirect;

/// Marker: path request is queued/pending — unit should wait, not walk blindly
#[derive(Component)]
pub struct NavPending;

// ── Resources ──

/// Navigation grid — passability costs aligned to the terrain grid.
/// 0 = impassable, 1-255 = traversal cost (1 = cheapest).
#[derive(Resource)]
pub struct NavGrid {
    pub costs: Vec<u8>,
    pub grid_size: usize,
    pub step: f32,
    pub half_map: f32,
}

impl NavGrid {
    pub fn world_to_grid(&self, x: f32, z: f32) -> (usize, usize) {
        let gx = ((x + self.half_map) / self.step)
            .round()
            .max(0.0)
            .min((self.grid_size - 1) as f32) as usize;
        let gz = ((z + self.half_map) / self.step)
            .round()
            .max(0.0)
            .min((self.grid_size - 1) as f32) as usize;
        (gx, gz)
    }

    pub fn grid_to_world(&self, gx: usize, gz: usize) -> (f32, f32) {
        let x = gx as f32 * self.step - self.half_map;
        let z = gz as f32 * self.step - self.half_map;
        (x, z)
    }

    pub fn index(&self, gx: usize, gz: usize) -> usize {
        gz * self.grid_size + gx
    }

}

#[derive(Resource)]
pub struct NavGridDirty(pub bool);

/// Timer to periodically refresh the nav grid obstacle overlay
#[derive(Resource)]
pub struct NavGridRefreshTimer(pub Timer);

pub struct PathRequest {
    pub entity: Entity,
    pub start: Vec3,
    pub goal: Vec3,
}

#[derive(Resource, Default)]
pub struct PathRequestQueue {
    pub requests: VecDeque<PathRequest>,
}

// ── Plugin ──

pub struct PathfindingPlugin;

impl Plugin for PathfindingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(NavGridDirty(true))
            .insert_resource(NavGridRefreshTimer(Timer::from_seconds(2.0, TimerMode::Repeating)))
            .init_resource::<PathRequestQueue>()
            .add_systems(
                OnEnter(AppState::InGame),
                build_nav_grid.after(crate::ground::spawn_ground),
            )
            .add_systems(
                Update,
                (
                    invalidate_stale_paths,
                    cleanup_orphan_paths,
                    refresh_nav_grid,
                    queue_path_requests,
                    process_path_requests,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<NavGrid>),
            );
    }
}

// ── Systems ──

/// When MoveTarget changes destination, clear the old NavPath so A* re-runs.
fn invalidate_stale_paths(
    mut commands: Commands,
    pathed: Query<(Entity, &MoveTarget, &NavPath), With<Unit>>,
    pending: Query<(Entity, &MoveTarget), (With<Unit>, With<NavPending>, Without<NavPath>)>,
    mut queue: ResMut<PathRequestQueue>,
) {
    for (entity, target, nav) in &pathed {
        let flat_dist = Vec2::new(
            target.0.x - nav.destination.x,
            target.0.z - nav.destination.z,
        )
        .length();
        if flat_dist > 2.0 {
            // Destination changed — clear old path and pending
            commands
                .entity(entity)
                .remove::<NavPath>()
                .remove::<NavPending>();
            queue.requests.retain(|r| r.entity != entity);
        }
    }
    // Also clear pending if MoveTarget destination changed while waiting
    for (entity, _target) in &pending {
        // NavPending without NavPath — check if still queued
        if !queue.requests.iter().any(|r| r.entity == entity) {
            // Request was lost or already processed, remove stale pending
            commands.entity(entity).remove::<NavPending>();
        }
    }
}

/// Clean up NavPath/NavPending on entities that lost their MoveTarget.
fn cleanup_orphan_paths(
    mut commands: Commands,
    pathed: Query<Entity, (With<NavPath>, Without<MoveTarget>)>,
    pending: Query<Entity, (With<NavPending>, Without<MoveTarget>)>,
    mut queue: ResMut<PathRequestQueue>,
) {
    for entity in &pathed {
        commands.entity(entity).remove::<NavPath>();
    }
    for entity in &pending {
        commands.entity(entity).remove::<NavPending>();
        queue.requests.retain(|r| r.entity != entity);
    }
}

fn build_nav_grid(
    height_map: Res<HeightMap>,
    biome_map: Res<BiomeMap>,
    mut commands: Commands,
) {
    let half_map = height_map.half_map;
    let step = NAV_GRID_STEP;
    let grid_size = (height_map.map_size / step).round() as usize + 1;
    let total = grid_size * grid_size;

    let mut costs = vec![1u8; total];

    // Mark terrain costs from biome and height
    for gz in 0..grid_size {
        for gx in 0..grid_size {
            let idx = gz * grid_size + gx;
            let wx = gx as f32 * step - half_map;
            let wz = gz as f32 * step - half_map;

            let biome = biome_map.get_biome(wx, wz);
            match biome {
                Biome::Water => {
                    costs[idx] = 0; // impassable
                }
                Biome::Mountain => {
                    // Mountains are passable but expensive
                    let h = height_map.sample(wx, wz);
                    if h > 8.0 {
                        costs[idx] = 0; // too steep
                    } else {
                        costs[idx] = 20;
                    }
                }
                _ => {
                    // Height gradient cost
                    let h = height_map.sample(wx, wz);
                    let base = 1 + (h.abs() * 2.0).min(30.0) as u8;
                    costs[idx] = base;
                }
            }
        }
    }

    commands.insert_resource(NavGrid {
        costs,
        grid_size,
        step,
        half_map,
    });
    commands.insert_resource(NavGridDirty(false));
}

/// Rebuild obstacle overlay when buildings/walls change.
fn refresh_nav_grid(
    time: Res<Time>,
    mut timer: ResMut<NavGridRefreshTimer>,
    mut dirty: ResMut<NavGridDirty>,
    mut nav_grid: ResMut<NavGrid>,
    height_map: Res<HeightMap>,
    biome_map: Res<BiomeMap>,
    buildings: Query<
        (&Transform, &BuildingFootprint, &BuildingState),
        With<Building>,
    >,
    wall_grid: Res<WallSpatialGrid>,
) {
    timer.0.tick(time.delta());
    if timer.0.just_finished() {
        dirty.0 = true;
    }
    if !dirty.0 {
        return;
    }
    dirty.0 = false;

    let gs = nav_grid.grid_size;
    let step = nav_grid.step;
    let half = nav_grid.half_map;

    // Reset to terrain-only costs
    for gz in 0..gs {
        for gx in 0..gs {
            let idx = gz * gs + gx;
            let wx = gx as f32 * step - half;
            let wz = gz as f32 * step - half;

            let biome = biome_map.get_biome(wx, wz);
            match biome {
                Biome::Water => {
                    nav_grid.costs[idx] = 0;
                }
                Biome::Mountain => {
                    let h = height_map.sample(wx, wz);
                    if h > 8.0 {
                        nav_grid.costs[idx] = 0;
                    } else {
                        nav_grid.costs[idx] = 20;
                    }
                }
                _ => {
                    let h = height_map.sample(wx, wz);
                    let base = 1 + (h.abs() * 2.0).min(30.0) as u8;
                    nav_grid.costs[idx] = base;
                }
            }
        }
    }

    // Stamp building footprints as impassable
    for (tf, footprint, state) in &buildings {
        // Only stamp completed or under-construction buildings
        if *state != BuildingState::Complete && *state != BuildingState::UnderConstruction {
            continue;
        }
        let radius = footprint.0 + 0.8; // padding
        let margin_radius = radius + step; // soft margin ring
        let pos = tf.translation;

        let (min_gx, min_gz) = nav_grid.world_to_grid(pos.x - margin_radius, pos.z - margin_radius);
        let (max_gx, max_gz) = nav_grid.world_to_grid(pos.x + margin_radius, pos.z + margin_radius);

        for gz in min_gz..=max_gz.min(gs - 1) {
            for gx in min_gx..=max_gx.min(gs - 1) {
                let (wx, wz) = nav_grid.grid_to_world(gx, gz);
                let dist = ((wx - pos.x).powi(2) + (wz - pos.z).powi(2)).sqrt();
                let idx = nav_grid.index(gx, gz);
                if dist < radius {
                    nav_grid.costs[idx] = 0;
                } else if dist < margin_radius && nav_grid.costs[idx] > 0 {
                    nav_grid.costs[idx] = nav_grid.costs[idx].max(OBSTACLE_MARGIN_COST);
                }
            }
        }
    }

    // Stamp walls as impassable
    for cells in wall_grid.cells.values() {
        for &(_entity, wall_pos, wall_fp, _faction) in cells {
            let radius = wall_fp + 0.6;
            let margin_radius = radius + step;
            let (min_gx, min_gz) =
                nav_grid.world_to_grid(wall_pos.x - margin_radius, wall_pos.z - margin_radius);
            let (max_gx, max_gz) =
                nav_grid.world_to_grid(wall_pos.x + margin_radius, wall_pos.z + margin_radius);

            for gz in min_gz..=max_gz.min(gs - 1) {
                for gx in min_gx..=max_gx.min(gs - 1) {
                    let (wx, wz) = nav_grid.grid_to_world(gx, gz);
                    let dist = ((wx - wall_pos.x).powi(2) + (wz - wall_pos.z).powi(2)).sqrt();
                    let idx = nav_grid.index(gx, gz);
                    if dist < radius {
                        nav_grid.costs[idx] = 0;
                    } else if dist < margin_radius && nav_grid.costs[idx] > 0 {
                        nav_grid.costs[idx] = nav_grid.costs[idx].max(OBSTACLE_MARGIN_COST);
                    }
                }
            }
        }
    }
}

/// Detect entities with MoveTarget but no NavPath — queue path requests.
fn queue_path_requests(
    mut commands: Commands,
    mut queue: ResMut<PathRequestQueue>,
    new_movers: Query<
        (Entity, &Transform, &MoveTarget),
        (With<Unit>, Without<NavPath>, Without<NavDirect>, Without<NavPending>),
    >,
) {
    for (entity, tf, target) in &new_movers {
        let start = tf.translation;
        let goal = target.0;
        let flat_dist = Vec2::new(goal.x - start.x, goal.z - start.z).length();

        // Skip pathfinding for very short moves
        if flat_dist < DIRECT_MOVE_THRESHOLD {
            continue;
        }

        queue.requests.push_back(PathRequest {
            entity,
            start,
            goal,
        });
        // Mark as pending so the unit waits instead of walking blindly
        commands.entity(entity).insert(NavPending);
    }
}

/// Process queued path requests (throttled per frame).
fn process_path_requests(
    mut commands: Commands,
    mut queue: ResMut<PathRequestQueue>,
    nav_grid: Res<NavGrid>,
    height_map: Res<HeightMap>,
    entities: &Entities,
) {
    let mut processed = 0;
    while processed < PATHS_PER_FRAME {
        let Some(request) = queue.requests.pop_front() else {
            break;
        };

        // Entity may have been despawned
        if !entities.contains(request.entity) {
            processed += 1;
            continue;
        }

        // Always remove pending marker
        commands.entity(request.entity).remove::<NavPending>();

        if let Some(path) = find_path(&nav_grid, request.start, request.goal) {
            // Convert grid path to world waypoints with terrain Y.
            // Skip the first waypoint (start position) — the unit is already there.
            let waypoints: Vec<Vec3> = path
                .into_iter()
                .skip(1)
                .map(|(wx, wz)| {
                    let y = height_map.sample(wx, wz);
                    Vec3::new(wx, y, wz)
                })
                .collect();

            if !waypoints.is_empty() {
                let destination = *waypoints.last().unwrap();
                commands.entity(request.entity).insert(NavPath {
                    waypoints,
                    current_index: 0,
                    destination,
                });
            }
        }
        // If pathfinding fails, NavPending is removed so unit falls back to direct movement

        processed += 1;
    }
}

// ── A* Pathfinding ──

/// Find a path from start to goal using A* on the NavGrid.
/// Returns a smoothed list of world-space (x, z) waypoints.
pub fn find_path(nav_grid: &NavGrid, start: Vec3, goal: Vec3) -> Option<Vec<(f32, f32)>> {
    let gs = nav_grid.grid_size;
    let (sx, sz) = nav_grid.world_to_grid(start.x, start.z);
    let (gx, gz) = nav_grid.world_to_grid(goal.x, goal.z);
    let start_idx = nav_grid.index(sx, sz);
    let goal_idx = nav_grid.index(gx, gz);

    if start_idx == goal_idx {
        let (wx, wz) = nav_grid.grid_to_world(gx, gz);
        return Some(vec![(wx, wz)]);
    }

    // If goal is impassable, find nearest passable cell
    let goal_idx = if nav_grid.costs[goal_idx] == 0 {
        if let Some(alt) = find_nearest_passable(nav_grid, gx, gz) {
            alt
        } else {
            return None;
        }
    } else {
        goal_idx
    };

    // If start is impassable, find nearest passable
    let start_idx = if nav_grid.costs[start_idx] == 0 {
        if let Some(alt) = find_nearest_passable(nav_grid, sx, sz) {
            alt
        } else {
            return None;
        }
    } else {
        start_idx
    };

    let total = gs * gs;
    let mut g_cost = vec![u32::MAX; total];
    let mut came_from = vec![usize::MAX; total];
    let mut open = BinaryHeap::new();
    let mut visited = 0usize;

    let goal_gx = goal_idx % gs;
    let goal_gz = goal_idx / gs;

    g_cost[start_idx] = 0;
    let h = octile_heuristic(start_idx % gs, start_idx / gs, goal_gx, goal_gz);
    open.push(Reverse((h, start_idx)));

    while let Some(Reverse((_, current))) = open.pop() {
        if current == goal_idx {
            let grid_path = reconstruct_path(&came_from, start_idx, goal_idx);
            let smoothed = smooth_path(nav_grid, &grid_path);

            // Convert to world coords
            let world_path: Vec<(f32, f32)> = smoothed
                .into_iter()
                .map(|idx| {
                    let gx = idx % gs;
                    let gz = idx / gs;
                    nav_grid.grid_to_world(gx, gz)
                })
                .collect();

            // Always append the exact goal position so NavPath.destination
            // matches MoveTarget exactly (avoids invalidation from grid-snap error).
            let mut result = world_path;
            result.push((goal.x, goal.z));

            return Some(result);
        }

        visited += 1;
        if visited > ASTAR_NODE_LIMIT {
            return None;
        }

        let cx = current % gs;
        let cz = current / gs;
        let current_g = g_cost[current];

        // Skip if we already found a better path to this node
        // (stale entry in the heap)
        if current_g == u32::MAX {
            continue;
        }

        // 8-connected neighbors
        for dz in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let nx = cx as i32 + dx;
                let nz = cz as i32 + dz;
                if nx < 0 || nz < 0 || nx >= gs as i32 || nz >= gs as i32 {
                    continue;
                }
                let ni = nz as usize * gs + nx as usize;
                let cell_cost = nav_grid.costs[ni];
                if cell_cost == 0 {
                    continue; // impassable
                }

                // Diagonal movement: prevent corner-cutting through impassable cells
                if dx != 0 && dz != 0 {
                    let adj1 = nav_grid.costs[cz * gs + (cx as i32 + dx) as usize];
                    let adj2 = nav_grid.costs[(cz as i32 + dz) as usize * gs + cx];
                    if adj1 == 0 || adj2 == 0 {
                        continue;
                    }
                }

                let base_cost = if dx != 0 && dz != 0 { 1414u32 } else { 1000u32 };
                let edge_cost = base_cost + cell_cost as u32 * 10;
                let tentative_g = current_g.saturating_add(edge_cost);

                if tentative_g < g_cost[ni] {
                    g_cost[ni] = tentative_g;
                    came_from[ni] = current;
                    let h = octile_heuristic(nx as usize, nz as usize, goal_gx, goal_gz);
                    let f = tentative_g + h;
                    open.push(Reverse((f, ni)));
                }
            }
        }
    }

    None
}

fn reconstruct_path(came_from: &[usize], start: usize, goal: usize) -> Vec<usize> {
    let mut path = Vec::new();
    let mut node = goal;
    while node != start {
        path.push(node);
        node = came_from[node];
        if node == usize::MAX {
            return Vec::new(); // broken path
        }
    }
    path.push(start);
    path.reverse();
    path
}

/// Line-of-sight path smoothing: remove unnecessary intermediate waypoints
fn smooth_path(nav_grid: &NavGrid, path: &[usize]) -> Vec<usize> {
    if path.len() <= 2 {
        return path.to_vec();
    }

    let gs = nav_grid.grid_size;
    let mut smoothed = vec![path[0]];
    let mut anchor = 0;

    while anchor < path.len() - 1 {
        let mut farthest = anchor + 1;

        // Find farthest reachable waypoint with clear line of sight
        for probe in (anchor + 2)..path.len() {
            let ax = path[anchor] % gs;
            let az = path[anchor] / gs;
            let bx = path[probe] % gs;
            let bz = path[probe] / gs;

            if line_of_sight(nav_grid, ax, az, bx, bz) {
                farthest = probe;
            }
        }

        smoothed.push(path[farthest]);
        anchor = farthest;
    }

    smoothed
}

/// Bresenham-style line of sight check on the nav grid
fn line_of_sight(nav_grid: &NavGrid, x0: usize, z0: usize, x1: usize, z1: usize) -> bool {
    let gs = nav_grid.grid_size;
    let dx = (x1 as i32 - x0 as i32).abs();
    let dz = (z1 as i32 - z0 as i32).abs();
    let sx: i32 = if x0 < x1 { 1 } else { -1 };
    let sz: i32 = if z0 < z1 { 1 } else { -1 };
    let mut err = dx - dz;
    let mut x = x0 as i32;
    let mut z = z0 as i32;

    loop {
        if x < 0 || z < 0 || x >= gs as i32 || z >= gs as i32 {
            return false;
        }
        if nav_grid.costs[z as usize * gs + x as usize] == 0 {
            return false;
        }
        if x == x1 as i32 && z == z1 as i32 {
            return true;
        }
        let e2 = 2 * err;
        if e2 > -dz {
            err -= dz;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            z += sz;
        }
    }
}

fn octile_heuristic(ax: usize, az: usize, bx: usize, bz: usize) -> u32 {
    let dx = (ax as i32 - bx as i32).unsigned_abs();
    let dz = (az as i32 - bz as i32).unsigned_abs();
    let diag = dx.min(dz);
    let straight = dx.max(dz) - diag;
    diag * 1414 + straight * 1000
}

/// Find the nearest passable cell to (gx, gz) using a small BFS
fn find_nearest_passable(nav_grid: &NavGrid, gx: usize, gz: usize) -> Option<usize> {
    let gs = nav_grid.grid_size;
    for radius in 1..=10 {
        let min_x = (gx as i32 - radius).max(0) as usize;
        let max_x = (gx as i32 + radius).min(gs as i32 - 1) as usize;
        let min_z = (gz as i32 - radius).max(0) as usize;
        let max_z = (gz as i32 + radius).min(gs as i32 - 1) as usize;

        let mut best: Option<(usize, f32)> = None;
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                let idx = z * gs + x;
                if nav_grid.costs[idx] > 0 {
                    let dist = ((x as f32 - gx as f32).powi(2) + (z as f32 - gz as f32).powi(2))
                        .sqrt();
                    if best.map_or(true, |(_, d)| dist < d) {
                        best = Some((idx, dist));
                    }
                }
            }
        }
        if let Some((idx, _)) = best {
            return Some(idx);
        }
    }
    None
}
