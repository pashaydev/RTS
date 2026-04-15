use bevy::ecs::entity::Entities;
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::prelude::*;
use bevy::time::Fixed;
use bevy::tasks::{block_on, poll_once, AsyncComputeTaskPool, Task};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use crate::types::*;
use crate::world::ground::{playable_half_map, HeightMap};
use crate::world::spatial::WallSpatialGrid;

// ── Constants ──

const NAV_GRID_STEP: f32 = 2.5;
const ASTAR_NODE_LIMIT: usize = 12_000;
const PATHS_PER_FRAME: usize = usize::MAX;
const NEW_PATH_REQUESTS_PER_FRAME: usize = usize::MAX;
/// Distance threshold below which we skip A* and just walk directly
const DIRECT_MOVE_THRESHOLD: f32 = 3.0;
/// Cost for cells near obstacles (soft margin — guides paths away but doesn't block)
const OBSTACLE_MARGIN_COST: u8 = 40;
/// Maximum height delta between two adjacent grid cells before they are treated as
/// an unwalkable cliff. At NAV_GRID_STEP=2.5 this corresponds to ~36° (atan(1.8/2.5)).
const MAX_STEP_HEIGHT_DELTA: f32 = 1.8;
/// Height delta beyond which a slope starts incurring extra A* cost.
const SLOPE_PENALTY_START: f32 = 0.6;

// ── Components ──

/// A sequence of waypoints produced by A* pathfinding.
#[derive(Component)]
pub struct NavPath {
    pub waypoints: Vec<Vec3>,
    pub current_index: usize,
}

/// Marker: skip pathfinding for this MoveTarget (short-range adjustment)
#[derive(Component)]
pub struct NavDirect;

/// Marker: path request is queued/pending — unit should wait, not walk blindly
#[derive(Component)]
pub struct NavPending;

#[derive(Component, Default)]
pub struct NavRequestTracker {
    pub latest_request_id: u64,
    pub last_goal: Option<Vec3>,
}

#[derive(Component)]
struct NavPathTask(Task<AsyncPathResult>);

// ── Resources ──

/// Navigation grid — passability costs aligned to the terrain grid.
/// 0 = impassable, 1-255 = traversal cost (1 = cheapest).
#[derive(Resource, Clone)]
pub struct NavGrid {
    pub costs: Vec<u8>,
    /// Terrain-only base costs computed once at init. Used to quickly reset
    /// the grid before re-stamping buildings (avoids full biome/height recompute).
    terrain_base_costs: Vec<u8>,
    /// Sampled terrain height per grid cell. Used by the height-aware
    /// line-of-sight check so smoothing/direct moves don't cut across cliffs
    /// that the rasterized cell passability missed.
    pub heights: Vec<f32>,
    pub grid_size: usize,
    pub step: f32,
    pub half_map: f32,
    pub revision: u64,
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

    pub fn is_world_passable(&self, x: f32, z: f32) -> bool {
        let (gx, gz) = self.world_to_grid(x, z);
        self.costs[self.index(gx, gz)] > 0
    }
}

#[derive(Resource, Default)]
pub struct NavGridDirty {
    /// Full rebuild needed (building removed, state changed, etc.)
    pub full: bool,
    /// Buildings added this frame — only stamp their footprints (fast path).
    pub added: Vec<(Vec3, f32, f32)>, // (pos, footprint radius+padding, margin_radius)
}

pub struct PathRequest {
    pub entity: Entity,
    pub start: Vec3,
    pub goal: Vec3,
    pub request_id: u64,
    pub nav_revision: u64,
}

#[derive(Resource, Default)]
pub struct PathRequestQueue {
    pub requests: VecDeque<PathRequest>,
}

#[derive(Resource, Default)]
pub struct PathRequestIds {
    next: u64,
}

impl PathRequestIds {
    fn next_id(&mut self) -> u64 {
        self.next = self.next.wrapping_add(1).max(1);
        self.next
    }
}

struct AsyncPathResult {
    request_id: u64,
    nav_revision: u64,
    goal: Vec3,
    path: Option<Vec<(f32, f32)>>,
}

// ── Plugin ──

pub struct PathfindingPlugin;

impl Plugin for PathfindingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(NavGridDirty {
            full: true,
            ..default()
        })
        .init_resource::<PathRequestQueue>()
        .init_resource::<PathRequestIds>()
        .add_systems(
            OnEnter(AppState::InGame),
            build_nav_grid.after(crate::world::ground::spawn_ground),
        )
        .add_systems(
            FixedUpdate,
            (
                mark_nav_grid_dirty,
                handle_move_target_updates,
                cleanup_orphan_paths,
                refresh_nav_grid,
                invalidate_paths_on_nav_revision,
                detect_stuck_units,
                queue_path_requests,
                spawn_pathfinding_tasks,
                collect_completed_path_tasks,
            )
                .chain()
                .in_set(SimSet::Spatial)
                .run_if(in_state(AppState::InGame))
                .run_if(resource_exists::<NavGrid>),
        );
    }
}

// ── Systems ──

/// React directly to MoveTarget mutations instead of comparing destinations every tick.
fn handle_move_target_updates(
    mut commands: Commands,
    movers: Query<
        (Entity, Option<&MoveTarget>, Option<&mut NavRequestTracker>),
        (
            Or<(With<Unit>, With<Mob>)>,
            Or<(Added<MoveTarget>, Changed<MoveTarget>)>,
        ),
    >,
    mut queue: ResMut<PathRequestQueue>,
) {
    for (entity, move_target, tracker) in movers {
        match move_target {
            Some(target) => {
                let same_goal = tracker
                    .as_ref()
                    .and_then(|tracker| tracker.last_goal)
                    .is_some_and(|previous| same_nav_goal(previous, target.0));
                if same_goal {
                    continue;
                }

                let mut entity_commands = commands.entity(entity);
                entity_commands
                    .remove::<NavPath>()
                    .remove::<NavPending>()
                    .remove::<NavPathTask>();
                queue.requests.retain(|r| r.entity != entity);

                if let Some(mut tracker) = tracker {
                    tracker.last_goal = Some(target.0);
                } else {
                    entity_commands.insert(NavRequestTracker {
                        latest_request_id: 0,
                        last_goal: Some(target.0),
                    });
                }
            }
            None => {
                let mut entity_commands = commands.entity(entity);
                entity_commands
                    .remove::<NavPath>()
                    .remove::<NavPending>()
                    .remove::<NavPathTask>()
                    .remove::<NavRequestTracker>();
                queue.requests.retain(|r| r.entity != entity);
            }
        }
    }
}

/// Clean up NavPath/NavPending/StuckTimer on entities that lost their MoveTarget.
fn cleanup_orphan_paths(
    mut commands: Commands,
    pathed: Query<Entity, (With<NavPath>, Without<MoveTarget>)>,
    pending: Query<Entity, (With<NavPending>, Without<MoveTarget>)>,
    path_tasks: Query<Entity, (With<NavPathTask>, Without<MoveTarget>)>,
    stuck: Query<Entity, (With<StuckTimer>, Without<MoveTarget>)>,
    mut queue: ResMut<PathRequestQueue>,
) {
    for entity in &pathed {
        commands
            .entity(entity)
            .remove::<NavPath>()
            .remove::<NavPathTask>()
            .remove::<NavRequestTracker>();
    }
    for entity in &pending {
        commands
            .entity(entity)
            .remove::<NavPending>()
            .remove::<NavPathTask>()
            .remove::<NavRequestTracker>();
        queue.requests.retain(|r| r.entity != entity);
    }
    for entity in &path_tasks {
        commands
            .entity(entity)
            .remove::<NavPathTask>()
            .remove::<NavRequestTracker>();
        queue.requests.retain(|r| r.entity != entity);
    }
    for entity in &stuck {
        commands
            .entity(entity)
            .remove::<StuckTimer>()
            .remove::<NavRequestTracker>()
            .remove::<NavPathTask>();
    }
}

/// Detect units that are stuck (not making progress toward their waypoint)
/// and force a repath by clearing their NavPath.
fn detect_stuck_units(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    nav_grid: Option<Res<NavGrid>>,
    mut units: Query<
        (Entity, &mut Transform, Option<&mut StuckTimer>),
        (Or<(With<Unit>, With<Mob>)>, With<MoveTarget>, With<NavPath>),
    >,
) {
    let dt = time.delta_secs();
    let stuck_threshold = 1.5; // seconds before triggering repath
    let movement_epsilon = 0.3; // minimum distance per check to count as "moving"
    let max_repaths_before_teleport: u8 = 3; // teleport after this many failed repaths

    for (entity, mut tf, stuck_timer) in &mut units {
        if let Some(mut timer) = stuck_timer {
            let moved = Vec2::new(
                tf.translation.x - timer.last_position.x,
                tf.translation.z - timer.last_position.z,
            )
            .length();

            if moved < movement_epsilon {
                timer.stuck_secs += dt;
            } else {
                timer.stuck_secs = 0.0;
                timer.repath_count = 0;
                timer.last_position = tf.translation;
            }

            if timer.stuck_secs > stuck_threshold {
                timer.repath_count += 1;
                timer.stuck_secs = 0.0;

                if timer.repath_count >= max_repaths_before_teleport {
                    // Teleport to nearest passable cell
                    if let Some(ref grid) = nav_grid {
                        let (gx, gz) = grid.world_to_grid(tf.translation.x, tf.translation.z);
                        if let Some(idx) = find_nearest_passable(grid, gx, gz) {
                            let nx = idx % grid.grid_size;
                            let nz = idx / grid.grid_size;
                            let (wx, wz) = grid.grid_to_world(nx, nz);
                            info!(
                                "Teleporting stuck unit {:?} from ({:.1}, {:.1}) to ({:.1}, {:.1})",
                                entity, tf.translation.x, tf.translation.z, wx, wz
                            );
                            tf.translation.x = wx;
                            tf.translation.z = wz;
                        }
                    }
                    timer.repath_count = 0;
                    timer.last_position = tf.translation;
                }

                // Force repath by clearing the current path
                commands
                    .entity(entity)
                    .remove::<NavPath>()
                    .remove::<NavPending>();
                timer.last_position = tf.translation;
            }
        } else {
            commands.entity(entity).insert(StuckTimer {
                last_position: tf.translation,
                stuck_secs: 0.0,
                repath_count: 0,
            });
        }
    }
}

fn build_nav_grid(height_map: Res<HeightMap>, biome_map: Res<BiomeMap>, mut commands: Commands) {
    let half_map = height_map.half_map;
    let playable_half = playable_half_map(height_map.map_size);
    let step = NAV_GRID_STEP;
    let grid_size = (height_map.map_size / step).round() as usize + 1;
    let total = grid_size * grid_size;

    let mut costs = vec![1u8; total];
    let mut heights = vec![0f32; total];

    // First pass: sample heights at every cell so the slope test below can
    // compare to neighbours without re-sampling the heightmap repeatedly.
    for gz in 0..grid_size {
        for gx in 0..grid_size {
            let wx = gx as f32 * step - half_map;
            let wz = gz as f32 * step - half_map;
            heights[gz * grid_size + gx] = height_map.sample(wx, wz);
        }
    }

    // Mark terrain costs from biome and height
    for gz in 0..grid_size {
        for gx in 0..grid_size {
            let idx = gz * grid_size + gx;
            let wx = gx as f32 * step - half_map;
            let wz = gz as f32 * step - half_map;

            if wx.abs() > playable_half || wz.abs() > playable_half {
                costs[idx] = 0;
                continue;
            }

            let biome = biome_map.get_biome(wx, wz);
            let h = heights[idx];

            // Check slope to all 8 neighbours. At NAV_GRID_STEP=2.5,
            // MAX_STEP_HEIGHT_DELTA=1.8 corresponds to ~36° — anything steeper is a cliff.
            let max_slope = MAX_STEP_HEIGHT_DELTA;
            let steep_slope = 1.0; // height diff that starts adding noticeable cost
            let mut max_diff: f32 = 0.0;
            // Sub-sample the midpoint between cells too, so cliffs that fall
            // between grid nodes still register as impassable.
            for dz in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dz == 0 {
                        continue;
                    }
                    let nx = gx as i32 + dx;
                    let nz = gz as i32 + dz;
                    if nx < 0 || nz < 0 || nx >= grid_size as i32 || nz >= grid_size as i32 {
                        continue;
                    }
                    let nh = heights[nz as usize * grid_size + nx as usize];
                    let diag_scale = if dx != 0 && dz != 0 { 1.4142 } else { 1.0 };
                    let normalized = (h - nh).abs() / diag_scale;
                    if normalized > max_diff {
                        max_diff = normalized;
                    }
                    // Mid-point sub-sample (catches sub-grid cliffs)
                    let mwx = wx + dx as f32 * step * 0.5;
                    let mwz = wz + dz as f32 * step * 0.5;
                    let mh = height_map.sample(mwx, mwz);
                    let mid_diff = (h - mh).abs() * 2.0 / diag_scale;
                    if mid_diff > max_diff {
                        max_diff = mid_diff;
                    }
                }
            }

            if max_diff > max_slope {
                costs[idx] = 0; // impassable cliff
                continue;
            }

            match biome {
                Biome::Water => {
                    costs[idx] = 0; // impassable
                }
                Biome::Mountain => {
                    // Mountains are passable but expensive
                    if h > 8.0 {
                        costs[idx] = 0; // too steep
                    } else {
                        costs[idx] = 20;
                    }
                }
                _ => {
                    // Height gradient cost + slope penalty
                    let height_cost = (h.abs() * 2.0).min(30.0) as u8;
                    let slope_cost = if max_diff > steep_slope {
                        ((max_diff - steep_slope) * 10.0).min(30.0) as u8
                    } else {
                        0
                    };
                    costs[idx] = 1 + height_cost + slope_cost;
                }
            }
        }
    }

    let terrain_base_costs = costs.clone();
    commands.insert_resource(NavGrid {
        costs,
        terrain_base_costs,
        heights,
        grid_size,
        step,
        half_map,
        revision: 1,
    });
    commands.insert_resource(NavGridDirty::default());
}

/// Track nav grid changes. New buildings use a fast incremental stamp;
/// removals or transform/footprint changes trigger a full rebuild.
/// BuildingState changes (e.g. UnderConstruction → Complete) are ignored
/// because the footprint was already stamped when the building was added.
fn mark_nav_grid_dirty(
    mut dirty: ResMut<NavGridDirty>,
    nav_grid: Res<NavGrid>,
    new_buildings: Query<
        (Entity, &Transform, &BuildingFootprint, &BuildingState),
        (With<Building>, Added<Building>),
    >,
    moved_buildings: Query<
        Entity,
        (
            With<Building>,
            Or<(Changed<Transform>, Changed<BuildingFootprint>)>,
        ),
    >,
    mut removed_buildings: RemovedComponents<Building>,
) {
    // Removals need a full rebuild.
    if removed_buildings.read().next().is_some() {
        dirty.full = true;
    }

    // Transform or footprint changes (not just state) need full rebuild.
    for entity in &moved_buildings {
        if new_buildings.get(entity).is_ok() {
            continue;
        }
        dirty.full = true;
        break;
    }

    // Newly added buildings can be stamped incrementally (fast path).
    for (_entity, tf, footprint, state) in &new_buildings {
        if *state != BuildingState::Complete && *state != BuildingState::UnderConstruction {
            continue;
        }
        let radius = footprint.0 + 0.8;
        let margin_radius = radius + nav_grid.step;
        dirty.added.push((tf.translation, radius, margin_radius));
    }
}

fn refresh_nav_grid(
    mut dirty: ResMut<NavGridDirty>,
    mut nav_grid: ResMut<NavGrid>,
    buildings: Query<(&Transform, &BuildingFootprint, &BuildingState), With<Building>>,
    wall_grid: Res<WallSpatialGrid>,
) {
    let has_adds = !dirty.added.is_empty();
    let needs_full = dirty.full;

    if !needs_full && !has_adds {
        return;
    }

    let gs = nav_grid.grid_size;

    if needs_full {
        // ── Full rebuild (building removed / state changed) ──
        dirty.full = false;
        dirty.added.clear();

        // Reset to cached terrain costs.
        for i in 0..nav_grid.costs.len() {
            nav_grid.costs[i] = nav_grid.terrain_base_costs[i];
        }
        let step = nav_grid.step;

        // Stamp ALL building footprints.
        for (tf, footprint, state) in &buildings {
            if *state != BuildingState::Complete && *state != BuildingState::UnderConstruction {
                continue;
            }
            let radius = footprint.0 + 0.8;
            let margin_radius = radius + step;
            stamp_obstacle(&mut nav_grid, tf.translation, radius, margin_radius, gs);
        }

        // Stamp walls.
        for cells in wall_grid.cells.values() {
            for &(_entity, wall_pos, wall_fp, _faction, _stable_id) in cells {
                let radius = wall_fp + 0.6;
                let margin_radius = radius + step;
                stamp_obstacle(&mut nav_grid, wall_pos, radius, margin_radius, gs);
            }
        }
    } else {
        // ── Incremental: only stamp newly added buildings (fast path) ──
        let added: Vec<_> = dirty.added.drain(..).collect();
        for (pos, radius, margin_radius) in added {
            stamp_obstacle(&mut nav_grid, pos, radius, margin_radius, gs);
        }
    }

    nav_grid.revision = nav_grid.revision.wrapping_add(1).max(1);
}

fn invalidate_paths_on_nav_revision(
    mut commands: Commands,
    nav_grid: Res<NavGrid>,
    mut queue: ResMut<PathRequestQueue>,
    movers: Query<Entity, (Or<(With<Unit>, With<Mob>)>, With<MoveTarget>)>,
) {
    if !nav_grid.is_changed() {
        return;
    }

    queue.requests.clear();
    for entity in &movers {
        commands
            .entity(entity)
            .remove::<NavPath>()
            .remove::<NavPending>()
            .remove::<NavPathTask>();
    }
}

/// Stamp a circular obstacle into the nav grid.
fn stamp_obstacle(nav_grid: &mut NavGrid, pos: Vec3, radius: f32, margin_radius: f32, gs: usize) {
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

/// Detect entities with MoveTarget but no NavPath — queue path requests.
fn queue_path_requests(
    mut commands: Commands,
    mut queue: ResMut<PathRequestQueue>,
    mut request_ids: ResMut<PathRequestIds>,
    nav_grid: Res<NavGrid>,
    mut new_movers: Query<
        (
            Entity,
            &Transform,
            &MoveTarget,
            Option<&mut NavRequestTracker>,
        ),
        (
            Or<(With<Unit>, With<Mob>)>,
            Without<NavPath>,
            Without<NavDirect>,
            Without<NavPending>,
            Without<NavPathTask>,
        ),
    >,
) {
    let mut queued_this_frame = 0usize;
    for (entity, tf, target, tracker) in &mut new_movers {
        if queued_this_frame >= NEW_PATH_REQUESTS_PER_FRAME {
            break;
        }

        let start = tf.translation;
        let goal = target.0;
        let flat_dist = Vec2::new(goal.x - start.x, goal.z - start.z).length();

        // Skip pathfinding only when the direct segment stays entirely walkable.
        if flat_dist < DIRECT_MOVE_THRESHOLD && is_direct_path_walkable(&nav_grid, start, goal) {
            continue;
        }

        let request_id = request_ids.next_id();
        let nav_revision = nav_grid.revision;
        if let Some(mut tracker) = tracker {
            tracker.latest_request_id = request_id;
            tracker.last_goal = Some(goal);
        } else {
            commands.entity(entity).insert(NavRequestTracker {
                latest_request_id: request_id,
                last_goal: Some(goal),
            });
        }

        if let Some(existing) = queue
            .requests
            .iter_mut()
            .find(|request| request.entity == entity)
        {
            existing.start = start;
            existing.goal = goal;
            existing.request_id = request_id;
            existing.nav_revision = nav_revision;
        } else {
            queue.requests.push_back(PathRequest {
                entity,
                start,
                goal,
                request_id,
                nav_revision,
            });
        }
        // Mark as pending so the unit waits instead of walking blindly
        commands.entity(entity).insert(NavPending);
        queued_this_frame += 1;
    }
}

/// Spawn async pathfinding jobs from the request queue.
fn spawn_pathfinding_tasks(
    mut commands: Commands,
    mut queue: ResMut<PathRequestQueue>,
    nav_grid: Res<NavGrid>,
    trackers: Query<&NavRequestTracker>,
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

        if request.nav_revision != nav_grid.revision
            || trackers.get(request.entity).map_or(true, |tracker| {
                tracker.latest_request_id != request.request_id
            })
        {
            commands.entity(request.entity).remove::<NavPending>();
            processed += 1;
            continue;
        }

        let grid = nav_grid.clone();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            AsyncPathResult {
                request_id: request.request_id,
                nav_revision: request.nav_revision,
                goal: request.goal,
                path: find_path(&grid, request.start, request.goal),
            }
        });
        commands.entity(request.entity).insert(NavPathTask(task));

        processed += 1;
    }
}

fn collect_completed_path_tasks(
    mut commands: Commands,
    height_map: Res<HeightMap>,
    nav_grid: Res<NavGrid>,
    mut tasks: Query<
        (
            Entity,
            &mut NavPathTask,
            &NavRequestTracker,
            Option<&MoveTarget>,
        ),
        Or<(With<Unit>, With<Mob>)>,
    >,
) {
    for (entity, mut task, tracker, move_target) in &mut tasks {
        let Some(result) = block_on(poll_once(&mut task.0)) else {
            continue;
        };

        let mut entity_commands = commands.entity(entity);
        entity_commands.remove::<NavPathTask>();
        entity_commands.remove::<NavPending>();

        if tracker.latest_request_id != result.request_id
            || nav_grid.revision != result.nav_revision
            || move_target.is_none_or(|target| {
                Vec2::new(target.0.x - result.goal.x, target.0.z - result.goal.z).length_squared()
                    > 0.01
            })
        {
            continue;
        }

        if let Some(path) = result.path {
            let waypoints: Vec<Vec3> = path
                .into_iter()
                .skip(1)
                .map(|(wx, wz)| {
                    let y = height_map.sample(wx, wz);
                    Vec3::new(wx, y, wz)
                })
                .collect();

            if !waypoints.is_empty() {
                entity_commands.insert(NavPath {
                    waypoints,
                    current_index: 0,
                });
            }
        }
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
    let original_goal_idx = nav_grid.index(gx, gz);

    if start_idx == original_goal_idx && nav_grid.costs[original_goal_idx] > 0 {
        let (wx, wz) = nav_grid.grid_to_world(gx, gz);
        return Some(vec![(wx, wz)]);
    }

    // If goal is impassable, find nearest passable cell
    let goal_idx = if nav_grid.costs[original_goal_idx] == 0 {
        if let Some(alt) = find_nearest_passable(nav_grid, gx, gz) {
            alt
        } else {
            return None;
        }
    } else {
        original_goal_idx
    };
    let goal_is_projected = goal_idx != original_goal_idx;

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

            let mut result = world_path;
            if !goal_is_projected {
                // Preserve the exact destination when the target itself is valid.
                result.push((goal.x, goal.z));
            }

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
                // Penalize traversal across height deltas. Anything above
                // SLOPE_PENALTY_START scales linearly; above MAX_STEP_HEIGHT_DELTA
                // we already rejected the cell as a cliff in build_nav_grid.
                let h_here = nav_grid.heights[current];
                let h_neigh = nav_grid.costs.get(ni).map(|_| nav_grid.heights[ni]).unwrap_or(h_here);
                let dh = (h_neigh - h_here).abs();
                let slope_penalty = if dh > SLOPE_PENALTY_START {
                    ((dh - SLOPE_PENALTY_START) * 800.0) as u32
                } else {
                    0
                };
                let edge_cost = base_cost + cell_cost as u32 * 10 + slope_penalty;
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

fn is_direct_path_walkable(nav_grid: &NavGrid, start: Vec3, goal: Vec3) -> bool {
    if !nav_grid.is_world_passable(start.x, start.z) || !nav_grid.is_world_passable(goal.x, goal.z)
    {
        return false;
    }

    let (sx, sz) = nav_grid.world_to_grid(start.x, start.z);
    let (gx, gz) = nav_grid.world_to_grid(goal.x, goal.z);
    line_of_sight(nav_grid, sx, sz, gx, gz)
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

/// Bresenham-style line of sight check on the nav grid.
/// Rejects when the segment crosses an impassable cell OR when consecutive
/// cells along the segment have a height delta exceeding MAX_STEP_HEIGHT_DELTA
/// (so smoothing and direct-walk shortcuts can't cut across cliffs).
fn line_of_sight(nav_grid: &NavGrid, x0: usize, z0: usize, x1: usize, z1: usize) -> bool {
    let gs = nav_grid.grid_size;
    let dx = (x1 as i32 - x0 as i32).abs();
    let dz = (z1 as i32 - z0 as i32).abs();
    let sx: i32 = if x0 < x1 { 1 } else { -1 };
    let sz: i32 = if z0 < z1 { 1 } else { -1 };
    let mut err = dx - dz;
    let mut x = x0 as i32;
    let mut z = z0 as i32;
    let mut prev_h = nav_grid.heights[z0 * gs + x0];

    loop {
        if x < 0 || z < 0 || x >= gs as i32 || z >= gs as i32 {
            return false;
        }
        let idx = z as usize * gs + x as usize;
        if nav_grid.costs[idx] == 0 {
            return false;
        }
        let cur_h = nav_grid.heights[idx];
        if (cur_h - prev_h).abs() > MAX_STEP_HEIGHT_DELTA {
            return false;
        }
        prev_h = cur_h;
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
                    let dist =
                        ((x as f32 - gx as f32).powi(2) + (z as f32 - gz as f32).powi(2)).sqrt();
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

fn same_nav_goal(a: Vec3, b: Vec3) -> bool {
    Vec2::new(a.x - b.x, a.z - b.z).length_squared() <= 0.01
}
