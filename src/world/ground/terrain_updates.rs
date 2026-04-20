//! Incremental terrain sculpting (shape operations from buildings and
//! mining) and mesh re-sync so the ground matches the authoritative heightmap.

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

use crate::blueprints::EntityKind;
use crate::types::{Building, BuildingFootprint, Ground};

use super::data::{
    foundation_radii, HeightMap, TerrainShapeOp, TerrainShapeSyncState, TerrainShapeUpdateQueue,
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
    mut queue: ResMut<TerrainShapeUpdateQueue>,
) {
    // Collect first, then order deterministically by (x,z,footprint). Both
    // host and client run this so all peers produce the same height map —
    // any divergence in ECS iteration order would otherwise make processed
    // ops differ between peers when they overlap.
    let mut additions: Vec<TerrainShapeOp> = new_buildings
        .iter()
        .filter_map(|(transform, kind, footprint)| {
            if !crate::simulation::buildings::uses_terrain_foundation(*kind) {
                return None;
            }
            let center = transform.translation.xz();
            Some(TerrainShapeOp {
                center: [center.x, center.y],
                footprint: footprint.0,
                target_height: height_map.foundation_target_height(center.x, center.y, footprint.0),
            })
        })
        .collect();

    additions.sort_by(|a, b| {
        a.center[0]
            .to_bits()
            .cmp(&b.center[0].to_bits())
            .then(a.center[1].to_bits().cmp(&b.center[1].to_bits()))
            .then(a.footprint.to_bits().cmp(&b.footprint.to_bits()))
    });

    for op in additions {
        queue.pending.push_back(op);
    }
}

/// Process at most one terrain shape update per frame to avoid large GPU re-upload spikes.
pub fn process_terrain_shape_update_queue(
    mut queue: ResMut<TerrainShapeUpdateQueue>,
    mut height_map: ResMut<HeightMap>,
    mut sync_state: ResMut<TerrainShapeSyncState>,
    mut dirty_areas: ResMut<TerrainSurfaceDirtyQueue>,
    ground_q: Query<&Mesh3d, With<Ground>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if queue.pending.is_empty() {
        return;
    }

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

        sync_ground_mesh_partial(
            mesh,
            &height_map,
            norm_min_x,
            norm_max_x,
            norm_min_z,
            norm_max_z,
        );
    }

    sync_state.applied_history.insert(update.clone());
    sync_state.applied_history_ordered.push(update.clone());
    sync_state.pending_network.push(update);
}

pub fn apply_terrain_shape_op(height_map: &mut HeightMap, update: &TerrainShapeOp) -> bool {
    let (inner_radius, outer_radius) = foundation_radii(update.footprint, height_map.step);
    let (min_x, max_x, min_z, max_z) = terrain_op_grid_bounds(height_map, update, outer_radius);

    let mut changed = false;
    for iz in min_z..=max_z {
        for ix in min_x..=max_x {
            let (world_x, world_z) = height_map.world_pos_for_grid(ix, iz);
            let dist = Vec2::new(world_x - update.center[0], world_z - update.center[1]).length();
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
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sort_ops(ops: &mut [TerrainShapeOp]) {
        ops.sort_by(|a, b| {
            a.center[0]
                .to_bits()
                .cmp(&b.center[0].to_bits())
                .then(a.center[1].to_bits().cmp(&b.center[1].to_bits()))
                .then(a.footprint.to_bits().cmp(&b.footprint.to_bits()))
        });
    }

    /// Different input permutations of the same op set must yield identical
    /// order after the deterministic sort — that's what lets every peer end
    /// up with the same `TerrainShapeUpdateQueue` contents and the same
    /// heightmap after processing.
    #[test]
    fn terrain_op_sort_is_permutation_stable() {
        let a = TerrainShapeOp {
            center: [1.5, -2.0],
            footprint: 3.0,
            target_height: 0.5,
        };
        let b = TerrainShapeOp {
            center: [-4.0, 6.0],
            footprint: 2.5,
            target_height: 1.0,
        };
        let c = TerrainShapeOp {
            center: [1.5, 0.0],
            footprint: 3.0,
            target_height: -0.5,
        };

        let mut p1 = vec![a, b, c];
        let mut p2 = vec![c, a, b];
        let mut p3 = vec![b, c, a];

        sort_ops(&mut p1);
        sort_ops(&mut p2);
        sort_ops(&mut p3);

        assert_eq!(p1, p2);
        assert_eq!(p2, p3);
    }

    fn flat_heightmap(grid_size: usize, step: f32) -> HeightMap {
        HeightMap {
            heights: vec![0.0; grid_size * grid_size],
            natural_heights: vec![0.0; grid_size * grid_size],
            grid_size,
            step,
            map_size: step * (grid_size as f32 - 1.0),
            half_map: step * (grid_size as f32 - 1.0) * 0.5,
        }
    }

    /// Under deterministic ordering, applying the same sorted op set on two
    /// fresh heightmaps yields byte-identical results. This is the property
    /// lockstep peers rely on for unit-Y sampling to converge.
    #[test]
    fn applying_sorted_ops_converges_across_permutations() {
        // Two ops whose inner regions fully overlap → target_height dominates
        // inside, transition region is order-sensitive outside.
        let op_a = TerrainShapeOp {
            center: [0.0, 0.0],
            footprint: 1.5,
            target_height: 1.0,
        };
        let op_b = TerrainShapeOp {
            center: [0.5, 0.2],
            footprint: 1.5,
            target_height: -0.5,
        };

        let mut hm1 = flat_heightmap(16, 1.0);
        let mut hm2 = flat_heightmap(16, 1.0);

        let mut order1 = vec![op_a, op_b];
        let mut order2 = vec![op_b, op_a];
        sort_ops(&mut order1);
        sort_ops(&mut order2);

        for op in &order1 {
            apply_terrain_shape_op(&mut hm1, op);
        }
        for op in &order2 {
            apply_terrain_shape_op(&mut hm2, op);
        }

        assert_eq!(
            hm1.heights, hm2.heights,
            "sorted ops must produce identical heightmaps on all peers"
        );
    }
}
