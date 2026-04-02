use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use game_state::message::TerrainShapeOp;

use crate::blueprints::EntityKind;
use crate::components::{Building, BuildingFootprint, Ground};

use super::data::{
    foundation_radii, HeightMap, TerrainShapeSyncState, TerrainShapeUpdateQueue,
    TerrainSurfaceDirtyArea, TerrainSurfaceDirtyQueue,
};

fn terrain_op_grid_bounds(
    height_map: &HeightMap,
    update: &TerrainShapeOp,
    radius: f32,
) -> (usize, usize, usize, usize) {
    let min_x = (((update.center[0] - radius) + height_map.half_map) / height_map.step)
        .floor()
        .max(0.0) as usize;
    let max_x = (((update.center[0] + radius) + height_map.half_map) / height_map.step)
        .ceil()
        .min((height_map.grid_size - 1) as f32) as usize;
    let min_z = (((update.center[1] - radius) + height_map.half_map) / height_map.step)
        .floor()
        .max(0.0) as usize;
    let max_z = (((update.center[1] + radius) + height_map.half_map) / height_map.step)
        .ceil()
        .min((height_map.grid_size - 1) as f32) as usize;

    (min_x, max_x, min_z, max_z)
}

pub fn enqueue_building_terrain_updates(
    new_buildings: Query<
        (&Transform, &EntityKind, &BuildingFootprint),
        (With<Building>, Added<Building>),
    >,
    height_map: Res<HeightMap>,
    net_role: Option<Res<crate::multiplayer::NetRole>>,
    mut queue: ResMut<TerrainShapeUpdateQueue>,
) {
    if matches!(net_role.as_deref(), Some(crate::multiplayer::NetRole::Client)) {
        return;
    }

    for (transform, kind, footprint) in &new_buildings {
        if !crate::buildings::uses_terrain_foundation(*kind) {
            continue;
        }

        let center = transform.translation.xz();
        queue.pending.push_back(TerrainShapeOp {
            center: [center.x, center.y],
            footprint: footprint.0,
            target_height: height_map.foundation_target_height(center.x, center.y, footprint.0),
        });
    }
}

/// Process at most one terrain shape update per frame to avoid large GPU re-upload spikes.
pub fn process_terrain_shape_update_queue(
    mut queue: ResMut<TerrainShapeUpdateQueue>,
    mut height_map: ResMut<HeightMap>,
    mut sync_state: ResMut<TerrainShapeSyncState>,
    mut dirty_areas: ResMut<TerrainSurfaceDirtyQueue>,
    net_role: Option<Res<crate::multiplayer::NetRole>>,
    ground_q: Query<&Mesh3d, With<Ground>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if queue.pending.is_empty() {
        return;
    }

    let is_client = matches!(net_role.as_deref(), Some(crate::multiplayer::NetRole::Client));
    let Some(update) = queue.pending.pop_front() else {
        return;
    };

    let (_, outer_radius) = foundation_radii(update.footprint, height_map.step);
    let (op_min_x, op_max_x, op_min_z, op_max_z) =
        terrain_op_grid_bounds(&height_map, &update, outer_radius);

    let changed = apply_terrain_shape_op(&mut height_map, &update);

    if changed {
        dirty_areas.pending.push_back(TerrainSurfaceDirtyArea {
            center: Vec2::new(update.center[0], update.center[1]),
            radius: outer_radius,
        });

        let Ok(ground_mesh) = ground_q.single() else {
            return;
        };
        let Some(mesh) = meshes.get_mut(&ground_mesh.0) else {
            return;
        };

        let norm_min_x = op_min_x.saturating_sub(1);
        let norm_max_x = (op_max_x + 1).min(height_map.grid_size - 1);
        let norm_min_z = op_min_z.saturating_sub(1);
        let norm_max_z = (op_max_z + 1).min(height_map.grid_size - 1);

        sync_ground_mesh_partial(mesh, &height_map, norm_min_x, norm_max_x, norm_min_z, norm_max_z);
    }

    sync_state.applied_history.insert(update.clone());
    sync_state.applied_history_ordered.push(update.clone());
    if !is_client {
        sync_state.pending_network.push(update);
    }
}

pub fn apply_terrain_shape_op(height_map: &mut HeightMap, update: &TerrainShapeOp) -> bool {
    let (inner_radius, outer_radius) = foundation_radii(update.footprint, height_map.step);
    let (min_x, max_x, min_z, max_z) = terrain_op_grid_bounds(height_map, update, outer_radius);

    let mut changed = false;
    for iz in min_z..=max_z {
        for ix in min_x..=max_x {
            let (world_x, world_z) = height_map.world_pos_for_grid(ix, iz);
            let dist =
                Vec2::new(world_x - update.center[0], world_z - update.center[1]).length();
            if dist > outer_radius {
                continue;
            }

            let blend = if dist <= inner_radius {
                1.0
            } else {
                let t = ((dist - inner_radius) / (outer_radius - inner_radius)).clamp(0.0, 1.0);
                1.0 - (t * t * (3.0 - 2.0 * t))
            };

            let idx = iz * height_map.grid_size + ix;
            let current = height_map.heights[idx];
            let next = current + (update.target_height - current) * blend;
            if (next - current).abs() > 0.001 {
                height_map.heights[idx] = next;
                changed = true;
            }
        }
    }

    changed
}

pub fn sync_ground_mesh_to_height_map(mesh: &mut Mesh, height_map: &HeightMap) {
    let mut positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(values)) => values.clone(),
        _ => return,
    };

    if positions.len() != height_map.heights.len() {
        return;
    }

    for (position, height) in positions.iter_mut().zip(height_map.heights.iter().copied()) {
        position[1] = height;
    }

    let mut normals = Vec::with_capacity(height_map.heights.len());
    for iz in 0..height_map.grid_size {
        for ix in 0..height_map.grid_size {
            let left_ix = ix.saturating_sub(1);
            let right_ix = (ix + 1).min(height_map.grid_size - 1);
            let down_iz = iz.saturating_sub(1);
            let up_iz = (iz + 1).min(height_map.grid_size - 1);

            let h_l = height_map.heights[iz * height_map.grid_size + left_ix];
            let h_r = height_map.heights[iz * height_map.grid_size + right_ix];
            let h_d = height_map.heights[down_iz * height_map.grid_size + ix];
            let h_u = height_map.heights[up_iz * height_map.grid_size + ix];
            let normal = Vec3::new(h_l - h_r, 2.0 * height_map.step, h_d - h_u).normalize();
            normals.push(normal.to_array());
        }
    }

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
}

/// Update only the dirty region of the mesh instead of all vertices.
pub fn sync_ground_mesh_partial(
    mesh: &mut Mesh,
    height_map: &HeightMap,
    min_x: usize,
    max_x: usize,
    min_z: usize,
    max_z: usize,
) {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };

    if positions.len() != height_map.heights.len() {
        return;
    }

    for iz in min_z..=max_z {
        for ix in min_x..=max_x {
            let idx = iz * height_map.grid_size + ix;
            positions[idx][1] = height_map.heights[idx];
        }
    }

    let Some(VertexAttributeValues::Float32x3(normals)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    else {
        return;
    };

    for iz in min_z..=max_z {
        for ix in min_x..=max_x {
            let left_ix = ix.saturating_sub(1);
            let right_ix = (ix + 1).min(height_map.grid_size - 1);
            let down_iz = iz.saturating_sub(1);
            let up_iz = (iz + 1).min(height_map.grid_size - 1);

            let h_l = height_map.heights[iz * height_map.grid_size + left_ix];
            let h_r = height_map.heights[iz * height_map.grid_size + right_ix];
            let h_d = height_map.heights[down_iz * height_map.grid_size + ix];
            let h_u = height_map.heights[up_iz * height_map.grid_size + ix];
            let normal = Vec3::new(h_l - h_r, 2.0 * height_map.step, h_d - h_u).normalize();

            let idx = iz * height_map.grid_size + ix;
            normals[idx] = normal.to_array();
        }
    }
}

pub fn reset_terrain_to_natural(height_map: &mut HeightMap, mesh: &mut Mesh) {
    height_map.heights.clone_from(&height_map.natural_heights);
    sync_ground_mesh_to_height_map(mesh, height_map);
}

/// Paint floor blend on the ground mesh vertex colors.
/// Sets vertex alpha to 0.0 inside the floor area (full floor texture),
/// with a smooth transition at the edges blending back to 1.0 (terrain).
pub fn paint_floor_blend_on_ground(
    mesh: &mut Mesh,
    height_map: &HeightMap,
    center_x: f32,
    center_z: f32,
    inner_radius: f32,
    transition: f32,
) {
    let Some(VertexAttributeValues::Float32x4(colors)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
    else {
        return;
    };

    let outer_radius = inner_radius + transition;
    let outer_r2 = outer_radius * outer_radius;

    // Only iterate grid cells within the affected region
    let min_ix = (((center_x - outer_radius) + height_map.half_map) / height_map.step)
        .floor()
        .max(0.0) as usize;
    let max_ix = (((center_x + outer_radius) + height_map.half_map) / height_map.step)
        .ceil()
        .min((height_map.grid_size - 1) as f32) as usize;
    let min_iz = (((center_z - outer_radius) + height_map.half_map) / height_map.step)
        .floor()
        .max(0.0) as usize;
    let max_iz = (((center_z + outer_radius) + height_map.half_map) / height_map.step)
        .ceil()
        .min((height_map.grid_size - 1) as f32) as usize;

    for iz in min_iz..=max_iz {
        for ix in min_ix..=max_ix {
            let wx = -height_map.half_map + ix as f32 * height_map.step;
            let wz = -height_map.half_map + iz as f32 * height_map.step;
            let dx = wx - center_x;
            let dz = wz - center_z;
            let dist_sq = dx * dx + dz * dz;

            if dist_sq > outer_r2 {
                continue;
            }

            let dist = dist_sq.sqrt();
            let idx = iz * height_map.grid_size + ix;

            // Inside inner radius: full floor (alpha = 0)
            // Transition zone: smooth blend from 0 to current alpha
            let floor_factor = if dist <= inner_radius {
                0.0
            } else {
                let t = (dist - inner_radius) / transition;
                // Smooth hermite interpolation
                t * t * (3.0 - 2.0 * t)
            };

            // Take min with existing alpha so overlapping floors merge correctly
            colors[idx][3] = colors[idx][3].min(floor_factor);
        }
    }
}

pub fn terrain_heights_hash(height_map: &HeightMap) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for height in &height_map.heights {
        hash ^= height.to_bits() as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
