use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

use crate::blueprints::EntityKind;
use crate::components::{
    AppState, Biome, BiomeMap, BuildSitePreparation, Building, BuildingFootprint,
    BuildingState, Ground, MoveTarget, Unit,
};
use crate::ground::{foundation_radii, HeightMap};

// ── Constants ──

const TRAFFIC_PER_TICK: f32 = 0.002;
const TRAFFIC_SEED: f32 = 0.6;
const DECAY_RATE: f32 = 0.001;
const MAX_DEPRESSION: f32 = 0.4;
const UPDATE_INTERVAL: f32 = 1.5;
const ASTAR_RADIUS: f32 = 120.0;
const ASTAR_NODE_LIMIT: usize = 5000;

// ── Resources ──

#[derive(Resource)]
pub struct TrafficMap {
    pub intensity: Vec<f32>,
    pub original_colors: Vec<[f32; 4]>,
    pub grid_size: usize,
    pub step: f32,
    pub half_map: f32,
    pub dirty: bool,
}

#[derive(Resource)]
pub struct FoundationTerrain {
    pub heights: Vec<f32>,
    pub dirty: bool,
}

impl TrafficMap {
    fn world_to_index(&self, x: f32, z: f32) -> Option<usize> {
        let gx = ((x + self.half_map) / self.step).round() as isize;
        let gz = ((z + self.half_map) / self.step).round() as isize;
        let gs = self.grid_size as isize;
        if gx < 0 || gz < 0 || gx >= gs || gz >= gs {
            return None;
        }
        Some(gz as usize * self.grid_size + gx as usize)
    }

    fn index_to_grid(&self, idx: usize) -> (usize, usize) {
        (idx % self.grid_size, idx / self.grid_size)
    }

    fn grid_to_index(&self, gx: usize, gz: usize) -> usize {
        gz * self.grid_size + gx
    }
}

#[derive(Resource)]
pub struct MeshUpdateTimer(pub Timer);

// ── Biome road colors ──

fn road_color_for_biome(biome: Biome) -> Option<[f32; 4]> {
    match biome {
        Biome::Grassland => Some([0.32, 0.28, 0.16, 1.0]),
        Biome::Forest => Some([0.35, 0.30, 0.18, 1.0]),
        Biome::Desert => Some([0.78, 0.70, 0.50, 1.0]),
        Biome::Beach => Some([0.72, 0.65, 0.45, 1.0]),
        Biome::Wetland => Some([0.25, 0.20, 0.12, 1.0]),
        Biome::Mountain => Some([0.55, 0.52, 0.48, 1.0]),
        Biome::Water => None,
    }
}

// ── Plugin ──

pub struct RoadPlugin;

impl Plugin for RoadPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::InGame),
            init_traffic_map.after(crate::ground::spawn_ground),
        )
        .add_systems(
            Update,
            (
                accumulate_unit_traffic,
                decay_traffic,
                seed_building_paths,
                recompute_foundation_terrain,
                update_terrain_mesh,
            )
                .chain()
                .run_if(in_state(AppState::InGame))
                .run_if(resource_exists::<TrafficMap>),
        );
    }
}

// ── Systems ──

fn init_traffic_map(
    height_map: Res<HeightMap>,
    ground_q: Query<&Mesh3d, With<Ground>>,
    meshes: Res<Assets<Mesh>>,
    mut commands: Commands,
) {
    let Ok(mesh_handle) = ground_q.single() else {
        return;
    };
    let Some(mesh) = meshes.get(&mesh_handle.0) else {
        return;
    };

    let original_colors = match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
        Some(VertexAttributeValues::Float32x4(colors)) => colors.clone(),
        _ => {
            warn!("Ground mesh has no Float32x4 vertex colors");
            return;
        }
    };

    let grid_size = height_map.grid_size;
    let vertex_count = grid_size * grid_size;

    commands.insert_resource(TrafficMap {
        intensity: vec![0.0; vertex_count],
        original_colors,
        grid_size,
        step: height_map.step,
        half_map: height_map.half_map,
        dirty: false,
    });
    commands.insert_resource(FoundationTerrain {
        heights: height_map.natural_heights.clone(),
        dirty: false,
    });

    commands.insert_resource(MeshUpdateTimer(Timer::from_seconds(
        UPDATE_INTERVAL,
        TimerMode::Repeating,
    )));
}

fn accumulate_unit_traffic(
    time: Res<Time>,
    mut traffic: ResMut<TrafficMap>,
    biome_map: Res<BiomeMap>,
    units: Query<&Transform, (With<Unit>, With<MoveTarget>)>,
) {
    let dt = time.delta_secs();
    let amount = TRAFFIC_PER_TICK * dt * 60.0; // normalize so constant is per-frame at 60fps
    let gs = traffic.grid_size as isize;

    for tf in &units {
        let x = tf.translation.x;
        let z = tf.translation.z;

        let gx = ((x + traffic.half_map) / traffic.step).round() as isize;
        let gz = ((z + traffic.half_map) / traffic.step).round() as isize;

        // Stamp 3x3 kernel
        for dz in -1..=1 {
            for dx in -1..=1 {
                let nx = gx + dx;
                let nz = gz + dz;
                if nx < 0 || nz < 0 || nx >= gs || nz >= gs {
                    continue;
                }
                let idx = nz as usize * traffic.grid_size + nx as usize;
                let biome = biome_map.data[idx];
                if biome == Biome::Water {
                    continue;
                }
                let weight = if dx == 0 && dz == 0 { 1.0 } else { 0.5 };
                traffic.intensity[idx] = (traffic.intensity[idx] + amount * weight).min(1.0);
            }
        }
        traffic.dirty = true;
    }
}

fn decay_traffic(time: Res<Time>, mut traffic: ResMut<TrafficMap>) {
    let dt = time.delta_secs();
    let factor = 1.0 - DECAY_RATE * dt * 60.0;
    let mut any_nonzero = false;

    for v in traffic.intensity.iter_mut() {
        if *v > 0.0 {
            *v *= factor;
            if *v < 0.001 {
                *v = 0.0;
            } else {
                any_nonzero = true;
            }
        }
    }

    if any_nonzero {
        traffic.dirty = true;
    }
}

fn seed_building_paths(
    mut traffic: ResMut<TrafficMap>,
    biome_map: Res<BiomeMap>,
    height_map: Res<HeightMap>,
    buildings: Query<(Entity, &Transform, &BuildingState), With<Building>>,
    mut processed: Local<HashSet<Entity>>,
) {
    let gs = traffic.grid_size;

    for (entity, tf, state) in &buildings {
        if *state != BuildingState::Complete || processed.contains(&entity) {
            continue;
        }
        processed.insert(entity);

        // Find other complete buildings within radius
        for (other_entity, other_tf, other_state) in &buildings {
            if other_entity == entity || *other_state != BuildingState::Complete {
                continue;
            }
            let dist = tf.translation.distance(other_tf.translation);
            if dist > ASTAR_RADIUS {
                continue;
            }

            // Run A* between the two buildings
            let start = traffic.world_to_index(tf.translation.x, tf.translation.z);
            let goal = traffic.world_to_index(other_tf.translation.x, other_tf.translation.z);

            let (start, goal) = match (start, goal) {
                (Some(s), Some(g)) => (s, g),
                _ => continue,
            };

            if let Some(path) = astar_grid(
                start,
                goal,
                gs,
                &height_map.heights,
                &biome_map.data,
            ) {
                for &idx in &path {
                    traffic.intensity[idx] = traffic.intensity[idx].max(TRAFFIC_SEED).min(1.0);

                    // Apply 40% falloff to 1-step neighbors for road width
                    let (gx, gz) = traffic.index_to_grid(idx);
                    for dz in -1i32..=1 {
                        for dx in -1i32..=1 {
                            if dx == 0 && dz == 0 {
                                continue;
                            }
                            let nx = gx as i32 + dx;
                            let nz = gz as i32 + dz;
                            if nx < 0 || nz < 0 || nx >= gs as i32 || nz >= gs as i32 {
                                continue;
                            }
                            let ni = traffic.grid_to_index(nx as usize, nz as usize);
                            let neighbor_val = TRAFFIC_SEED * 0.4;
                            if biome_map.data[ni] != Biome::Water {
                                traffic.intensity[ni] =
                                    traffic.intensity[ni].max(neighbor_val).min(1.0);
                            }
                        }
                    }
                }
                traffic.dirty = true;
            }
        }
    }
}

fn recompute_foundation_terrain(
    height_map: Res<HeightMap>,
    mut terrain: ResMut<FoundationTerrain>,
    mut traffic: ResMut<TrafficMap>,
    buildings: Query<(&Transform, &BuildingFootprint, &EntityKind), With<Building>>,
    prep_sites: Query<&BuildSitePreparation, With<Unit>>,
    changed_buildings: Query<
        Entity,
        (
            With<Building>,
            Or<(Added<Building>, Changed<Transform>, Changed<BuildingFootprint>)>,
        ),
    >,
    changed_prep_sites: Query<Entity, Added<BuildSitePreparation>>,
    removed_buildings: RemovedComponents<Building>,
    removed_prep_sites: RemovedComponents<BuildSitePreparation>,
) {
    let building_removed = !removed_buildings.is_empty();
    let prep_removed = !removed_prep_sites.is_empty();
    if changed_buildings.is_empty()
        && changed_prep_sites.is_empty()
        && !building_removed
        && !prep_removed
    {
        return;
    }

    let mut stamped = height_map.natural_heights.clone();

    for (transform, footprint, kind) in &buildings {
        if !crate::buildings::uses_terrain_foundation(*kind) {
            continue;
        }
        stamp_foundation(
            &mut stamped,
            &height_map,
            transform.translation.x,
            transform.translation.z,
            footprint.0,
        );
    }

    for prep in &prep_sites {
        if !crate::buildings::uses_terrain_foundation(prep.kind) {
            continue;
        }
        stamp_foundation(
            &mut stamped,
            &height_map,
            prep.position.x,
            prep.position.z,
            crate::buildings::footprint_for_kind(prep.kind),
        );
    }

    terrain.heights = stamped;
    terrain.dirty = true;
    traffic.dirty = true;
}

fn update_terrain_mesh(
    time: Res<Time>,
    mut timer: ResMut<MeshUpdateTimer>,
    mut traffic: ResMut<TrafficMap>,
    biome_map: Res<BiomeMap>,
    mut height_map: ResMut<HeightMap>,
    mut foundation_terrain: ResMut<FoundationTerrain>,
    ground_q: Query<&Mesh3d, With<Ground>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    timer.0.tick(time.delta());
    if !foundation_terrain.dirty && (!timer.0.just_finished() || !traffic.dirty) {
        return;
    }

    let Ok(mesh_handle) = ground_q.single() else {
        return;
    };
    let Some(mesh) = meshes.get_mut(&mesh_handle.0) else {
        return;
    };

    let gs = traffic.grid_size;
    let vertex_count = gs * gs;

    // Update positions and colors
    let positions = match mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(p)) => p,
        _ => return,
    };

    // Collect new heights first
    let mut new_heights = vec![0.0f32; vertex_count];
    for i in 0..vertex_count {
        let t = traffic.intensity[i];
        let h = (foundation_terrain.heights[i] - t * MAX_DEPRESSION)
            .max(crate::ground::WATER_LEVEL + 0.1);
        new_heights[i] = h;
        positions[i][1] = h;
    }

    // Write back to HeightMap for unit movement
    height_map.heights.copy_from_slice(&new_heights);

    // Update vertex colors
    let colors = match mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) {
        Some(VertexAttributeValues::Float32x4(c)) => c,
        _ => return,
    };

    for i in 0..vertex_count {
        let t = traffic.intensity[i];
        if t < 0.001 {
            colors[i] = traffic.original_colors[i];
            continue;
        }
        let biome = biome_map.data[i];
        if let Some(road_col) = road_color_for_biome(biome) {
            for ch in 0..4 {
                colors[i][ch] = traffic.original_colors[i][ch]
                    + (road_col[ch] - traffic.original_colors[i][ch]) * t;
            }
        }
    }

    // Recompute normals via central difference
    let normals = match mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(n)) => n,
        _ => return,
    };

    let step = traffic.step;
    for iz in 0..gs {
        for ix in 0..gs {
            let idx = iz * gs + ix;
            let h_self = new_heights[idx];
            let h_left = if ix > 0 { new_heights[iz * gs + ix - 1] } else { h_self };
            let h_right = if ix < gs - 1 { new_heights[iz * gs + ix + 1] } else { h_self };
            let h_up = if iz > 0 { new_heights[(iz - 1) * gs + ix] } else { h_self };
            let h_down = if iz < gs - 1 { new_heights[(iz + 1) * gs + ix] } else { h_self };

            // Extrapolate missing neighbors at edges (mirrors slope from the other side)
            let hx0 = if ix > 0 { h_left } else { 2.0 * h_self - h_right };
            let hx1 = if ix < gs - 1 { h_right } else { 2.0 * h_self - h_left };
            let hz0 = if iz > 0 { h_up } else { 2.0 * h_self - h_down };
            let hz1 = if iz < gs - 1 { h_down } else { 2.0 * h_self - h_up };

            let dx_span = if ix > 0 && ix < gs - 1 { 2.0 * step } else { step };
            let dz_span = if iz > 0 && iz < gs - 1 { 2.0 * step } else { step };

            let nx = (hx0 - hx1) / dx_span;
            let nz = (hz0 - hz1) / dz_span;
            let len = (nx * nx + 1.0 + nz * nz).sqrt();
            normals[idx] = [nx / len, 1.0 / len, nz / len];
        }
    }

    traffic.dirty = false;
    foundation_terrain.dirty = false;
}

fn stamp_foundation(
    heights: &mut [f32],
    height_map: &HeightMap,
    center_x: f32,
    center_z: f32,
    footprint: f32,
) {
    let target_height = height_map.foundation_target_height(center_x, center_z, footprint);
    let (inner_radius, outer_radius) = foundation_radii(footprint, height_map.step);
    let min_x = (((center_x - outer_radius) + height_map.half_map) / height_map.step)
        .floor()
        .max(0.0) as usize;
    let max_x = (((center_x + outer_radius) + height_map.half_map) / height_map.step)
        .ceil()
        .min((height_map.grid_size - 1) as f32) as usize;
    let min_z = (((center_z - outer_radius) + height_map.half_map) / height_map.step)
        .floor()
        .max(0.0) as usize;
    let max_z = (((center_z + outer_radius) + height_map.half_map) / height_map.step)
        .ceil()
        .min((height_map.grid_size - 1) as f32) as usize;

    for iz in min_z..=max_z {
        for ix in min_x..=max_x {
            let world_x = -height_map.half_map + ix as f32 * height_map.step;
            let world_z = -height_map.half_map + iz as f32 * height_map.step;
            let dx = world_x - center_x;
            let dz = world_z - center_z;
            let distance = (dx * dx + dz * dz).sqrt();
            if distance > outer_radius {
                continue;
            }

            let blend = if distance <= inner_radius {
                1.0
            } else {
                let t = ((distance - inner_radius) / (outer_radius - inner_radius)).clamp(0.0, 1.0);
                1.0 - (t * t * (3.0 - 2.0 * t))
            };

            let idx = iz * height_map.grid_size + ix;
            heights[idx] = heights[idx] + (target_height - heights[idx]) * blend;
        }
    }
}

// ── A* Pathfinding ──

fn astar_grid(
    start: usize,
    goal: usize,
    grid_size: usize,
    heights: &[f32],
    biomes: &[Biome],
) -> Option<Vec<usize>> {
    if start == goal {
        return Some(vec![start]);
    }

    let total = grid_size * grid_size;
    let mut g_cost = vec![u32::MAX; total];
    let mut came_from = vec![usize::MAX; total];
    let mut open = BinaryHeap::new();
    let mut visited = 0usize;

    g_cost[start] = 0;
    open.push(Reverse((heuristic(start, goal, grid_size), start)));

    let goal_gx = goal % grid_size;
    let goal_gz = goal / grid_size;

    while let Some(Reverse((_, current))) = open.pop() {
        if current == goal {
            // Reconstruct path
            let mut path = Vec::new();
            let mut node = goal;
            while node != start {
                path.push(node);
                node = came_from[node];
            }
            path.push(start);
            path.reverse();
            return Some(path);
        }

        visited += 1;
        if visited > ASTAR_NODE_LIMIT {
            return None;
        }

        let cx = current % grid_size;
        let cz = current / grid_size;
        let current_g = g_cost[current];

        // 8-connected neighbors
        for dz in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let nx = cx as i32 + dx;
                let nz = cz as i32 + dz;
                if nx < 0 || nz < 0 || nx >= grid_size as i32 || nz >= grid_size as i32 {
                    continue;
                }
                let ni = nz as usize * grid_size + nx as usize;

                // Water is impassable
                if biomes[ni] == Biome::Water {
                    continue;
                }

                let base_cost = if dx != 0 && dz != 0 { 1414u32 } else { 1000u32 };
                let height_diff = (heights[ni] - heights[current]).abs();
                let edge_cost = base_cost + (height_diff * 3000.0) as u32;
                let tentative_g = current_g.saturating_add(edge_cost);

                if tentative_g < g_cost[ni] {
                    g_cost[ni] = tentative_g;
                    came_from[ni] = current;
                    let h = chebyshev(nx as usize, nz as usize, goal_gx, goal_gz);
                    let f = tentative_g + h * 1000;
                    open.push(Reverse((f, ni)));
                }
            }
        }
    }

    None
}

fn heuristic(a: usize, b: usize, grid_size: usize) -> u32 {
    let ax = a % grid_size;
    let az = a / grid_size;
    let bx = b % grid_size;
    let bz = b / grid_size;
    chebyshev(ax, az, bx, bz) * 1000
}

fn chebyshev(ax: usize, az: usize, bx: usize, bz: usize) -> u32 {
    let dx = (ax as i32 - bx as i32).unsigned_abs();
    let dz = (az as i32 - bz as i32).unsigned_abs();
    let diag = dx.min(dz);
    let straight = dx.max(dz) - diag;
    diag * 1414 / 1000 + straight
}
