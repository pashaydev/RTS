use std::collections::{HashSet, VecDeque};

use bevy::prelude::*;
use game_state::message::TerrainShapeOp;

/// Pre-computed grid of terrain heights that matches the rendered mesh exactly.
/// Use `sample(x, z)` for triangle-matched interpolation between grid vertices.
#[derive(Resource)]
pub struct HeightMap {
    pub heights: Vec<f32>,
    pub natural_heights: Vec<f32>,
    pub grid_size: usize,
    pub step: f32,
    pub map_size: f32,
    pub half_map: f32,
}

#[derive(Resource, Default)]
pub struct TerrainShapeUpdateQueue {
    pub pending: VecDeque<TerrainShapeOp>,
}

#[derive(Resource, Default)]
pub struct TerrainShapeSyncState {
    pub applied_history: HashSet<TerrainShapeOp>,
    pub applied_history_ordered: Vec<TerrainShapeOp>,
    pub pending_network: Vec<TerrainShapeOp>,
}

#[derive(Clone, Copy, Debug)]
pub struct TerrainSurfaceDirtyArea {
    pub center: Vec2,
    pub radius: f32,
}

#[derive(Resource, Default)]
pub struct TerrainSurfaceDirtyQueue {
    pub pending: VecDeque<TerrainSurfaceDirtyArea>,
}

impl HeightMap {
    fn sample_from(&self, heights: &[f32], x: f32, z: f32) -> f32 {
        let gx = (x + self.half_map) / self.step;
        let gz = (z + self.half_map) / self.step;
        let ix = (gx.floor().max(0.0) as usize).min(self.grid_size - 2);
        let iz = (gz.floor().max(0.0) as usize).min(self.grid_size - 2);
        let fx = (gx - ix as f32).clamp(0.0, 1.0);
        let fz = (gz - iz as f32).clamp(0.0, 1.0);

        let h00 = heights[iz * self.grid_size + ix];
        let h10 = heights[iz * self.grid_size + ix + 1];
        let h01 = heights[(iz + 1) * self.grid_size + ix];
        let h11 = heights[(iz + 1) * self.grid_size + ix + 1];

        if fx + fz <= 1.0 {
            h00 + (h10 - h00) * fx + (h01 - h00) * fz
        } else {
            h11 + (h10 - h11) * (1.0 - fz) + (h01 - h11) * (1.0 - fx)
        }
    }

    pub(crate) fn world_pos_for_grid(&self, ix: usize, iz: usize) -> (f32, f32) {
        (
            -self.half_map + ix as f32 * self.step,
            -self.half_map + iz as f32 * self.step,
        )
    }

    /// Sample terrain height at any world position using triangle interpolation
    /// that exactly matches the rendered mesh triangulation (tl-bl-tr / tr-bl-br).
    pub fn sample(&self, x: f32, z: f32) -> f32 {
        self.sample_from(&self.heights, x, z)
    }

    pub fn sample_natural(&self, x: f32, z: f32) -> f32 {
        self.sample_from(&self.natural_heights, x, z)
    }

    /// Returns the maximum slope (rise/run) under a building footprint.
    /// March a ray against the heightmap and return the world-space hit point.
    pub fn raycast(&self, ray: Ray3d) -> Option<Vec3> {
        let max_dist = self.map_size * 4.0;
        let step = self.step.max(1.0);

        let in_bounds = |p: Vec3| p.x.abs() <= self.half_map && p.z.abs() <= self.half_map;
        let terrain_delta = |p: Vec3| p.y - self.sample(p.x, p.z);

        let mut prev_t = 0.0_f32;
        let mut prev_delta: Option<f32> = None;

        let mut t = 0.0_f32;
        while t <= max_dist {
            let point = ray.get_point(t);
            if in_bounds(point) {
                let delta = terrain_delta(point);
                if delta <= 0.0 {
                    let mut low_t = if prev_delta.is_some() { prev_t } else { 0.0 };
                    let mut high_t = t;
                    for _ in 0..12 {
                        let mid_t = (low_t + high_t) * 0.5;
                        if terrain_delta(ray.get_point(mid_t)) > 0.0 {
                            low_t = mid_t;
                        } else {
                            high_t = mid_t;
                        }
                    }
                    let hit = ray.get_point((low_t + high_t) * 0.5);
                    return Some(Vec3::new(hit.x, self.sample(hit.x, hit.z), hit.z));
                }
                prev_t = t;
                prev_delta = Some(delta);
            } else if prev_delta.is_some() {
                break;
            } else {
                prev_t = t;
            }
            t += step;
        }

        // Fallback: Y=0 plane intersection
        let dist = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
        let fallback = ray.get_point(dist);
        if in_bounds(fallback) {
            Some(Vec3::new(
                fallback.x,
                self.sample(fallback.x, fallback.z),
                fallback.z,
            ))
        } else {
            None
        }
    }

    /// Returns the maximum slope (rise/run) under a building footprint.
    pub fn max_slope_under_footprint(&self, x: f32, z: f32, footprint: f32) -> f32 {
        let r = footprint * 0.7;
        let h_center = self.sample(x, z);
        let offsets = [(r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)];
        offsets
            .iter()
            .map(|(dx, dz)| (self.sample(x + dx, z + dz) - h_center).abs() / r)
            .fold(0.0_f32, f32::max)
    }

    fn foundation_target_height_from(
        &self,
        heights: &[f32],
        fallback_sample: impl FnOnce(&Self, f32, f32) -> f32,
        x: f32,
        z: f32,
        footprint: f32,
    ) -> f32 {
        let (inner_radius, _) = foundation_radii(footprint, self.step);
        let min_x = (((x - inner_radius) + self.half_map) / self.step)
            .floor()
            .max(0.0) as usize;
        let max_x = (((x + inner_radius) + self.half_map) / self.step)
            .ceil()
            .min((self.grid_size - 1) as f32) as usize;
        let min_z = (((z - inner_radius) + self.half_map) / self.step)
            .floor()
            .max(0.0) as usize;
        let max_z = (((z + inner_radius) + self.half_map) / self.step)
            .ceil()
            .min((self.grid_size - 1) as f32) as usize;

        let radius_sq = inner_radius * inner_radius;
        let mut sum = 0.0;
        let mut count = 0usize;

        for iz in min_z..=max_z {
            for ix in min_x..=max_x {
                let (world_x, world_z) = self.world_pos_for_grid(ix, iz);
                let dx = world_x - x;
                let dz = world_z - z;
                if dx * dx + dz * dz > radius_sq {
                    continue;
                }

                let idx = iz * self.grid_size + ix;
                sum += heights[idx];
                count += 1;
            }
        }

        if count == 0 {
            fallback_sample(self, x, z)
        } else {
            sum / count as f32
        }
    }

    pub fn foundation_target_height(&self, x: f32, z: f32, footprint: f32) -> f32 {
        self.foundation_target_height_from(
            &self.natural_heights,
            Self::sample_natural,
            x,
            z,
            footprint,
        )
    }

    pub fn foundation_target_height_shaped(&self, x: f32, z: f32, footprint: f32) -> f32 {
        self.foundation_target_height_from(&self.heights, Self::sample, x, z, footprint)
    }
}

pub fn foundation_radii(footprint: f32, step: f32) -> (f32, f32) {
    let inner = (footprint * 0.65).max(step * 1.1);
    let outer = inner + footprint * 0.35 + step * 2.0;
    (inner, outer)
}

pub const AMPLITUDE: f32 = 18.0;
pub const WATER_LEVEL: f32 = AMPLITUDE * -0.18;

#[derive(Clone, Copy)]
pub struct BorderSettings {
    pub thickness: f32,
    pub transition: f32,
    pub ridge_height: f32,
    pub prop_inset: f32,
}

impl BorderSettings {
    pub fn from_map_size(map_size: f32) -> Self {
        if map_size <= 320.0 {
            Self {
                thickness: 32.0,
                transition: 12.0,
                ridge_height: 11.0,
                prop_inset: 10.0,
            }
        } else if map_size <= 520.0 {
            Self {
                thickness: 48.0,
                transition: 18.0,
                ridge_height: 14.0,
                prop_inset: 12.0,
            }
        } else {
            Self {
                thickness: 64.0,
                transition: 24.0,
                ridge_height: 18.0,
                prop_inset: 14.0,
            }
        }
    }
}

pub fn edge_distance_to_square(x: f32, z: f32, half_map: f32) -> f32 {
    half_map - x.abs().max(z.abs())
}

pub fn is_in_mountain_border(x: f32, z: f32, half_map: f32, settings: BorderSettings) -> bool {
    edge_distance_to_square(x, z, half_map) <= settings.thickness
}

pub fn playable_half_map(map_size: f32) -> f32 {
    let border = BorderSettings::from_map_size(map_size);
    map_size * 0.5 - border.thickness - border.transition
}
