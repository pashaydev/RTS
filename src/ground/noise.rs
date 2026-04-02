use bevy::prelude::*;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::components::{Biome, GameSetupConfig, MapSeed};

use super::data::{edge_distance_to_square, BorderSettings, AMPLITUDE, WATER_LEVEL};

const NOISE_SCALE: f64 = 0.006;
const WARP_SCALE: f64 = 0.003;
const WARP_AMP: f32 = 35.0;
const MOISTURE_SCALE: f64 = 0.005;
const TEMPERATURE_SCALE: f64 = 0.004;
const MOISTURE_MACRO_SCALE: f64 = 0.0017;
const TEMPERATURE_MACRO_SCALE: f64 = 0.0014;
const WATER_BIOME_MARGIN: f32 = 0.45;
const BEACH_BIOME_MARGIN: f32 = 2.0;
const MOUNTAIN_BIOME_HEIGHT_NORM: f32 = 0.76;
const BIOME_COUNT: usize = 7;

fn height_to_norm(height: f32) -> f32 {
    ((height / AMPLITUDE) * 0.5 + 0.5).clamp(0.0, 1.0)
}

/// Holds seed-derived noise generators for terrain generation.
pub struct TerrainNoise {
    height_fbm: Fbm<Perlin>,
    warp_fbm: Fbm<Perlin>,
    moisture_fbm: Fbm<Perlin>,
    temperature_fbm: Fbm<Perlin>,
}

impl TerrainNoise {
    pub fn from_seed(seed: u64) -> Self {
        let s0 = seed as u32;
        let s1 = (seed >> 16) as u32;
        let s2 = (seed >> 32) as u32;
        let s3 = seed.wrapping_mul(7919) as u32;
        Self {
            height_fbm: Fbm::<Perlin>::new(s0).set_octaves(6),
            warp_fbm: Fbm::<Perlin>::new(s3).set_octaves(4),
            moisture_fbm: Fbm::<Perlin>::new(s1).set_octaves(3),
            temperature_fbm: Fbm::<Perlin>::new(s2).set_octaves(3),
        }
    }

    fn base_terrain_height(&self, x: f32, z: f32) -> f32 {
        let wx = self
            .warp_fbm
            .get([x as f64 * WARP_SCALE, z as f64 * WARP_SCALE]) as f32
            * WARP_AMP;
        let wz = self
            .warp_fbm
            .get([x as f64 * WARP_SCALE + 100.0, z as f64 * WARP_SCALE + 100.0])
            as f32
            * WARP_AMP;

        let val = self
            .height_fbm
            .get([(x + wx) as f64 * NOISE_SCALE, (z + wz) as f64 * NOISE_SCALE])
            as f32;
        val * AMPLITUDE
    }

    pub fn terrain_height(&self, x: f32, z: f32, half_map: f32) -> f32 {
        let mut height = self.base_terrain_height(x, z);

        let center_dist = (x * x + z * z).sqrt() / half_map;
        let continent_mask = 1.0 - (center_dist * 0.65).powi(2);
        height = height * continent_mask + AMPLITUDE * 0.25;

        let terrace_scale = 0.25;
        let terraced = (height * terrace_scale).round() / terrace_scale;
        height = height * 0.85 + terraced * 0.15;

        let border = BorderSettings::from_map_size(half_map * 2.0);
        let edge_distance = edge_distance_to_square(x, z, half_map);

        if edge_distance > border.thickness + border.transition {
            return height;
        }

        let ridge_noise = self
            .moisture_fbm
            .get([x as f64 * 0.021 + 37.0, z as f64 * 0.021 - 19.0]) as f32;
        let ridge_variation = ridge_noise * 3.5;

        if edge_distance <= border.thickness {
            return height.max(AMPLITUDE * 0.62 + border.ridge_height + ridge_variation);
        }

        let blend_t =
            1.0 - ((edge_distance - border.thickness) / border.transition).clamp(0.0, 1.0);
        let smooth_t = blend_t * blend_t * (3.0 - 2.0 * blend_t);
        let transition_lift = border.ridge_height * 0.35;
        let ridge_target = AMPLITUDE * 0.55 + transition_lift * smooth_t + ridge_variation * 0.35;
        height + (ridge_target - height) * smooth_t
    }

    fn sample_moisture(&self, x: f32, z: f32, half_map: f32) -> f32 {
        let local = (self
            .moisture_fbm
            .get([x as f64 * MOISTURE_SCALE, z as f64 * MOISTURE_SCALE]) as f32
            * 0.5
            + 0.5)
            .clamp(0.0, 1.0);
        let macro_pattern = (self.moisture_fbm.get([
            x as f64 * MOISTURE_MACRO_SCALE + 311.0,
            z as f64 * MOISTURE_MACRO_SCALE - 173.0,
        ]) as f32
            * 0.5
            + 0.5)
            .clamp(0.0, 1.0);
        let radial = (1.0 - ((x * x + z * z).sqrt() / half_map)).clamp(0.0, 1.0);

        (local * 0.45 + macro_pattern * 0.4 + radial * 0.15).clamp(0.0, 1.0)
    }

    fn sample_temperature(&self, x: f32, z: f32, half_map: f32, height_norm: f32) -> f32 {
        let local = (self
            .temperature_fbm
            .get([x as f64 * TEMPERATURE_SCALE, z as f64 * TEMPERATURE_SCALE])
            as f32
            * 0.5
            + 0.5)
            .clamp(0.0, 1.0);
        let macro_pattern = (self.temperature_fbm.get([
            x as f64 * TEMPERATURE_MACRO_SCALE - 251.0,
            z as f64 * TEMPERATURE_MACRO_SCALE + 97.0,
        ]) as f32
            * 0.5
            + 0.5)
            .clamp(0.0, 1.0);
        let latitude = 1.0 - (z.abs() / half_map).clamp(0.0, 1.0);
        let elevation_cooling = ((height_norm - 0.52).max(0.0) * 0.45).clamp(0.0, 0.25);

        (local * 0.35 + macro_pattern * 0.25 + latitude * 0.4 - elevation_cooling).clamp(0.0, 1.0)
    }

    pub fn biome_at(&self, x: f32, z: f32, half_map: f32) -> Biome {
        let border = BorderSettings::from_map_size(half_map * 2.0);
        let edge_distance = edge_distance_to_square(x, z, half_map);
        if edge_distance <= border.thickness {
            return Biome::Mountain;
        }

        let height = self.terrain_height(x, z, half_map);
        let height_norm = height_to_norm(height);
        let moisture = self.sample_moisture(x, z, half_map);
        let temperature = self.sample_temperature(x, z, half_map, height_norm);

        if height <= WATER_LEVEL + WATER_BIOME_MARGIN {
            return Biome::Water;
        }
        if height <= WATER_LEVEL + BEACH_BIOME_MARGIN {
            return Biome::Beach;
        }
        if height_norm > MOUNTAIN_BIOME_HEIGHT_NORM {
            return Biome::Mountain;
        }
        if height_norm < 0.54 && moisture > 0.67 {
            return Biome::Wetland;
        }
        if temperature > 0.66 && moisture < 0.38 {
            return Biome::Desert;
        }
        if moisture > 0.56 && temperature > 0.28 {
            return Biome::Forest;
        }
        Biome::Grassland
    }
}

/// Resolves the map seed: if 0, generates a random one. Inserts MapSeed resource.
pub fn resolve_map_seed(mut commands: Commands, config: Res<GameSetupConfig>) {
    let seed = if config.map_seed == 0 {
        rand::random::<u64>()
    } else {
        config.map_seed
    };
    info!("Map seed: {}", seed);
    commands.insert_resource(MapSeed(seed));
}

fn biome_color(biome: Biome, height_norm: f32) -> [f32; 4] {
    match biome {
        Biome::Grassland => {
            let t = ((height_norm - 0.33) / 0.42).clamp(0.0, 1.0);
            [0.22 + t * 0.12, 0.50 + t * 0.15, 0.10 + t * 0.08, 1.0]
        }
        Biome::Forest => {
            let t = ((height_norm - 0.33) / 0.42).clamp(0.0, 1.0);
            [0.10 + t * 0.08, 0.35 + t * 0.13, 0.06 + t * 0.04, 1.0]
        }
        Biome::Desert => {
            let t = ((height_norm - 0.33) / 0.42).clamp(0.0, 1.0);
            [0.82 + t * 0.10, 0.74 + t * 0.08, 0.48 + t * 0.06, 1.0]
        }
        Biome::Beach => {
            let t = ((height_norm - 0.28) / 0.05).clamp(0.0, 1.0);
            [0.80 + t * 0.08, 0.73 + t * 0.07, 0.50 + t * 0.08, 1.0]
        }
        Biome::Wetland => {
            let t = ((height_norm - 0.33) / 0.09).clamp(0.0, 1.0);
            [0.25 + t * 0.08, 0.38 + t * 0.08, 0.16 + t * 0.06, 1.0]
        }
        Biome::Water => {
            let depth = 1.0 - height_norm;
            [
                0.03 + depth * 0.08,
                0.10 + depth * 0.15,
                0.40 + depth * 0.25,
                1.0,
            ]
        }
        Biome::Mountain => {
            let t = ((height_norm - 0.75) / 0.25).clamp(0.0, 1.0);
            [0.48 + t * 0.40, 0.46 + t * 0.40, 0.43 + t * 0.42, 1.0]
        }
    }
}

fn biome_index(biome: Biome) -> usize {
    match biome {
        Biome::Grassland => 0,
        Biome::Forest => 1,
        Biome::Desert => 2,
        Biome::Beach => 3,
        Biome::Wetland => 4,
        Biome::Water => 5,
        Biome::Mountain => 6,
    }
}

pub fn blended_biome_color_patched(
    noise: &TerrainNoise,
    biome_data: &[Biome],
    grid_size: usize,
    x: f32,
    z: f32,
    half_map: f32,
    step: f32,
) -> [f32; 4] {
    let blend_radius = step * 3.0;
    const SAMPLE_STEPS: i32 = 2;
    let mut weights = [0.0f32; BIOME_COUNT];
    let mut height_norms = [0.0f32; BIOME_COUNT];

    let inv_sigma_sq = 1.0 / (blend_radius * blend_radius * 0.5);
    let map_size = half_map * 2.0;

    for dzi in -SAMPLE_STEPS..=SAMPLE_STEPS {
        for dxi in -SAMPLE_STEPS..=SAMPLE_STEPS {
            let frac_x = dxi as f32 / SAMPLE_STEPS as f32;
            let frac_z = dzi as f32 / SAMPLE_STEPS as f32;
            let dx = frac_x * blend_radius;
            let dz = frac_z * blend_radius;

            let dist_sq = dx * dx + dz * dz;
            let weight = (-dist_sq * inv_sigma_sq).exp();

            let sx = x + dx;
            let sz = z + dz;
            let h = noise.terrain_height(sx, sz, half_map);
            let hn = ((h / AMPLITUDE) * 0.5 + 0.5).clamp(0.0, 1.0);

            let gx = ((sx + half_map) / map_size * (grid_size - 1) as f32)
                .round()
                .clamp(0.0, (grid_size - 1) as f32) as usize;
            let gz = ((sz + half_map) / map_size * (grid_size - 1) as f32)
                .round()
                .clamp(0.0, (grid_size - 1) as f32) as usize;
            let biome = biome_data[gz * grid_size + gx];

            let index = biome_index(biome);
            weights[index] += weight;
            height_norms[index] += hn * weight;
        }
    }

    let total: f32 = weights.iter().sum();
    let mut color = [0.0f32; 4];
    let all_biomes = [
        Biome::Grassland,
        Biome::Forest,
        Biome::Desert,
        Biome::Beach,
        Biome::Wetland,
        Biome::Water,
        Biome::Mountain,
    ];

    for (index, biome) in all_biomes.into_iter().enumerate() {
        if weights[index] > 0.0 {
            let avg_hn = height_norms[index] / weights[index];
            let weight = weights[index] / total;
            let biome_color = biome_color(biome, avg_hn);
            color[0] += biome_color[0] * weight;
            color[1] += biome_color[1] * weight;
            color[2] += biome_color[2] * weight;
            color[3] += biome_color[3] * weight;
        }
    }

    color
}
