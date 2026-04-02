use std::collections::{HashSet, VecDeque};

use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use game_state::message::TerrainShapeOp;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use rand::SeedableRng;

use crate::blueprints::EntityKind;
use crate::components::{
    AppState, Biome, BiomeMap, Building, BuildingFootprint, CullingBounds, Decoration,
    FogHideable, GameFlowSet, GameSetupConfig, GameWorld, Ground, MapSeed, ModelAssets,
    RtsCamera,
};
use crate::fog::FogTextures;
use crate::lighting::SunLight;
use crate::terrain_material::{TerrainExtension, TerrainMaterial, TerrainSettings};
use crate::water_material::{WaterMaterial, WaterSettings};
use bevy::light::NotShadowCaster;

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

    fn world_pos_for_grid(&self, ix: usize, iz: usize) -> (f32, f32) {
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
    pub fn max_slope_under_footprint(&self, x: f32, z: f32, footprint: f32) -> f32 {
        let r = footprint * 0.7;
        let h_center = self.sample(x, z);
        let offsets = [(r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)];
        offsets
            .iter()
            .map(|(dx, dz)| (self.sample(x + dx, z + dz) - h_center).abs() / r)
            .fold(0.0_f32, f32::max)
    }

    pub fn foundation_target_height(&self, x: f32, z: f32, footprint: f32) -> f32 {
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
                sum += self.natural_heights[idx];
                count += 1;
            }
        }

        if count == 0 {
            self.sample_natural(x, z)
        } else {
            sum / count as f32
        }
    }
}

pub fn foundation_radii(footprint: f32, step: f32) -> (f32, f32) {
    let inner = (footprint * 0.65).max(step * 1.1);
    let outer = inner + footprint * 0.35 + step * 2.0;
    (inner, outer)
}
const NOISE_SCALE: f64 = 0.006;
const AMPLITUDE: f32 = 18.0;
pub const WATER_LEVEL: f32 = AMPLITUDE * -0.18;
const WARP_SCALE: f64 = 0.003;
const WARP_AMP: f32 = 35.0;

const MOISTURE_SCALE: f64 = 0.005;
const TEMPERATURE_SCALE: f64 = 0.004;
const MOISTURE_MACRO_SCALE: f64 = 0.0017;
const TEMPERATURE_MACRO_SCALE: f64 = 0.0014;
const WATER_BIOME_MARGIN: f32 = 0.45;
const BEACH_BIOME_MARGIN: f32 = 2.0;
const MOUNTAIN_BIOME_HEIGHT_NORM: f32 = 0.76;

fn height_to_norm(height: f32) -> f32 {
    ((height / AMPLITUDE) * 0.5 + 0.5).clamp(0.0, 1.0)
}

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
        // Domain warping for organic landforms
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

        // Continental shaping: gentle bowl that raises center, lowers edges
        let center_dist = (x * x + z * z).sqrt() / half_map;
        let continent_mask = 1.0 - (center_dist * 0.65).powi(2);
        height = height * continent_mask + AMPLITUDE * 0.25;

        // Soft terracing for sculpted plateau look (subtle to avoid visible seams)
        let terrace_scale = 0.25;
        let terraced = (height * terrace_scale).round() / terrace_scale;
        height = height * 0.85 + terraced * 0.15;

        let border = BorderSettings::from_map_size(half_map * 2.0);
        let edge_distance = edge_distance_to_square(x, z, half_map);

        if edge_distance > border.thickness + border.transition {
            return height;
        }

        let ridge_noise =
            self.moisture_fbm
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
            .get([x as f64 * MOISTURE_SCALE, z as f64 * MOISTURE_SCALE])
            as f32
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

const BIOME_COUNT: usize = 7;

fn biome_index(b: Biome) -> usize {
    match b {
        Biome::Grassland => 0,
        Biome::Forest => 1,
        Biome::Desert => 2,
        Biome::Beach => 3,
        Biome::Wetland => 4,
        Biome::Water => 5,
        Biome::Mountain => 6,
    }
}

/// Sample biomes from the (potentially patched) biome_data grid with Gaussian-weighted blending
/// instead of querying `noise.biome_at`, so terrain colors match patched biomes.
fn blended_biome_color_patched(
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
            let w = (-dist_sq * inv_sigma_sq).exp();

            let sx = x + dx;
            let sz = z + dz;
            let h = noise.terrain_height(sx, sz, half_map);
            let hn = ((h / AMPLITUDE) * 0.5 + 0.5).clamp(0.0, 1.0);

            // Look up biome from the patched grid
            let gx = ((sx + half_map) / map_size * (grid_size - 1) as f32)
                .round()
                .clamp(0.0, (grid_size - 1) as f32) as usize;
            let gz = ((sz + half_map) / map_size * (grid_size - 1) as f32)
                .round()
                .clamp(0.0, (grid_size - 1) as f32) as usize;
            let b = biome_data[gz * grid_size + gx];

            let idx = biome_index(b);
            weights[idx] += w;
            height_norms[idx] += hn * w;
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
    for i in 0..BIOME_COUNT {
        if weights[i] > 0.0 {
            let avg_hn = height_norms[i] / weights[i];
            let w = weights[i] / total;
            let bc = biome_color(all_biomes[i], avg_hn);
            color[0] += bc[0] * w;
            color[1] += bc[1] * w;
            color[2] += bc[2] * w;
            color[3] += bc[3] * w;
        }
    }
    color
}

#[derive(Resource)]
pub struct TerrainTextures {
    pub grass: Handle<Image>,
    pub rock: Handle<Image>,
    pub sand: Handle<Image>,
    pub snow: Handle<Image>,
}

pub struct GroundPlugin;

/// Marker for the water plane entity.
#[derive(Component)]
pub struct WaterPlane;

impl Plugin for GroundPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .add_plugins(MaterialPlugin::<TerrainMaterial>::default())
            .init_resource::<TerrainShapeUpdateQueue>()
            .init_resource::<TerrainShapeSyncState>()
            .add_systems(Startup, load_terrain_textures)
            .add_systems(
                OnEnter(AppState::InGame),
                (resolve_map_seed, spawn_ground, spawn_mountain_border).chain(),
            )
            .add_systems(
                Update,
                (update_water_time, patch_water_fog_textures)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (enqueue_building_terrain_updates, process_terrain_shape_update_queue)
                    .chain()
                    .in_set(GameFlowSet::Simulation)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn load_terrain_textures(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(TerrainTextures {
        grass: asset_server.load("textures/terrain/grass.png"),
        rock: asset_server.load("textures/terrain/rock.png"),
        sand: asset_server.load("textures/terrain/sand.png"),
        snow: asset_server.load("textures/terrain/snow.png"),
    });
}

fn update_water_time(
    time: Res<Time>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    query: Query<&MeshMaterial3d<WaterMaterial>, With<WaterPlane>>,
    camera_q: Query<&Transform, With<RtsCamera>>,
    sun_q: Query<&Transform, (With<SunLight>, Without<RtsCamera>)>,
) {
    let cam_pos = camera_q
        .iter()
        .next()
        .map(|t| t.translation)
        .unwrap_or(Vec3::new(0.0, 50.0, 0.0));

    // Get sun direction from scene light (negate forward = direction toward light)
    let sun_dir = sun_q
        .iter()
        .next()
        .map(|t| {
            let fwd = t.forward().as_vec3();
            Vec4::new(-fwd.x, -fwd.y, -fwd.z, 0.0)
        })
        .unwrap_or(Vec4::new(0.5, 0.7, 0.3, 0.0));

    for mat_handle in &query {
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            mat.settings.time = time.elapsed_secs();
            mat.settings.camera_position = Vec4::new(cam_pos.x, cam_pos.y, cam_pos.z, 0.0);
            mat.settings.sun_direction = sun_dir;
        }
    }
}

/// Once FogTextures exist, patch all water materials to reference them.
/// Re-runs if fog textures change (e.g. after returning to main menu and starting a new game).
fn patch_water_fog_textures(
    fog_tex: Option<Res<FogTextures>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    query: Query<&MeshMaterial3d<WaterMaterial>, With<WaterPlane>>,
) {
    let Some(fog_tex) = fog_tex else {
        return;
    };

    for mat_handle in &query {
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            if mat.fog_visible_texture.is_none() {
                mat.fog_visible_texture = Some(fog_tex.visible.clone());
                mat.fog_explored_texture = Some(fog_tex.explored.clone());
            }
        }
    }
}

pub fn spawn_ground(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
    terrain_textures: Res<TerrainTextures>,
) {
    let noise = TerrainNoise::from_seed(map_seed.0);

    let actual_map_size = config.map_size.world_size();
    let actual_half_map = actual_map_size / 2.0;
    let actual_grid_size = ((actual_map_size / 1.5) as usize + 1).min(351);

    // Generate terrain mesh — pass 1: positions, normals, UVs, biome data
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(actual_grid_size * actual_grid_size);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(actual_grid_size * actual_grid_size);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(actual_grid_size * actual_grid_size);
    let mut biome_data: Vec<Biome> = Vec::with_capacity(actual_grid_size * actual_grid_size);

    let step = actual_map_size / (actual_grid_size - 1) as f32;
    let eps = 0.5_f32; // for normal calculation

    for iz in 0..actual_grid_size {
        for ix in 0..actual_grid_size {
            let x = -actual_half_map + ix as f32 * step;
            let z = -actual_half_map + iz as f32 * step;
            let y = noise.terrain_height(x, z, actual_half_map);

            biome_data.push(noise.biome_at(x, z, actual_half_map));

            positions.push([x, y, z]);
            uvs.push([
                ix as f32 / (actual_grid_size - 1) as f32,
                iz as f32 / (actual_grid_size - 1) as f32,
            ]);

            // Central-difference normals
            let h_l = noise.terrain_height(x - eps, z, actual_half_map);
            let h_r = noise.terrain_height(x + eps, z, actual_half_map);
            let h_d = noise.terrain_height(x, z - eps, actual_half_map);
            let h_u = noise.terrain_height(x, z + eps, actual_half_map);
            let normal = Vec3::new(h_l - h_r, 2.0 * eps, h_d - h_u).normalize();
            normals.push(normal.to_array());
        }
    }

    // Build HeightMap early so ensure_all_biomes can use it
    let grid_heights: Vec<f32> = positions.iter().map(|p| p[1]).collect();

    // Ensure every biome type appears on the map — patch in missing ones
    ensure_all_biomes(
        &mut biome_data,
        actual_grid_size,
        &grid_heights,
        actual_half_map,
    );

    // Pass 2: compute vertex colors using (potentially patched) biome data
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(actual_grid_size * actual_grid_size);
    for iz in 0..actual_grid_size {
        for ix in 0..actual_grid_size {
            let x = -actual_half_map + ix as f32 * step;
            let z = -actual_half_map + iz as f32 * step;
            colors.push(blended_biome_color_patched(
                &noise,
                &biome_data,
                actual_grid_size,
                x,
                z,
                actual_half_map,
                step,
            ));
        }
    }

    // Generate indices
    let mut indices: Vec<u32> =
        Vec::with_capacity((actual_grid_size - 1) * (actual_grid_size - 1) * 6);
    for iz in 0..(actual_grid_size - 1) {
        for ix in 0..(actual_grid_size - 1) {
            let tl = (iz * actual_grid_size + ix) as u32;
            let tr = tl + 1;
            let bl = tl + actual_grid_size as u32;
            let br = bl + 1;

            indices.push(tl);
            indices.push(bl);
            indices.push(tr);

            indices.push(tr);
            indices.push(bl);
            indices.push(br);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        VertexAttributeValues::Float32x4(colors),
    );
    mesh.insert_indices(Indices::U32(indices));

    commands.spawn((
        GameWorld,
        Ground,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(terrain_materials.add(TerrainMaterial {
            base: StandardMaterial {
                perceptual_roughness: 0.9,
                metallic: 0.0,
                reflectance: 0.1,
                ..default()
            },
            extension: TerrainExtension {
                settings: TerrainSettings {
                    amplitude: AMPLITUDE,
                    ..default()
                },
                grass_texture: Some(terrain_textures.grass.clone()),
                rock_texture: Some(terrain_textures.rock.clone()),
                sand_texture: Some(terrain_textures.sand.clone()),
                snow_texture: Some(terrain_textures.snow.clone()),
            },
        })),
        Transform::from_translation(Vec3::ZERO),
    ));
    // ── Detect and spawn separate water bodies ──
    let water_mat_handle = water_materials.add(WaterMaterial {
        settings: WaterSettings::default(),
        fog_visible_texture: None,
        fog_explored_texture: None,
    });

    let water_bodies = find_water_bodies(&grid_heights, actual_grid_size, step, actual_half_map);

    for body_cells in &water_bodies {
        // Compute bounding sphere for frustum culling
        let (center, radius) =
            water_body_bounds(body_cells, actual_grid_size, step, actual_half_map);
        if let Some(mesh) = build_water_body_mesh(
            body_cells,
            &grid_heights,
            actual_grid_size,
            step,
            actual_half_map,
            center,
        ) {
            commands.spawn((
                GameWorld,
                WaterPlane,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(water_mat_handle.clone()),
                Transform::from_translation(center),
                CullingBounds::new(radius),
            ));
        }
    }

    // Insert HeightMap & BiomeMap resources
    commands.insert_resource(HeightMap {
        heights: grid_heights.clone(),
        natural_heights: grid_heights,
        grid_size: actual_grid_size,
        step,
        map_size: actual_map_size,
        half_map: actual_half_map,
    });
    commands.insert_resource(BiomeMap {
        data: biome_data,
        grid_size: actual_grid_size,
        map_size: actual_map_size,
    });
}

/// Ensure every biome type has meaningful coverage on the map.
/// Biomes below a minimum cell count get additional patches stamped at suitable locations.
fn ensure_all_biomes(
    biome_data: &mut [Biome],
    grid_size: usize,
    heights: &[f32],
    half_map: f32,
) {
    let total_cells = biome_data.len();
    // Each non-structural biome should cover at least 5% of the map
    let min_cells = total_cells / 20;

    // Biomes we guarantee coverage for (Water/Mountain are structural)
    let target_biomes = [
        Biome::Wetland,
        Biome::Desert,
        Biome::Forest,
        Biome::Grassland,
    ];

    let patch_radius: usize = (grid_size / 14).max(5);
    let margin = patch_radius + 4;
    let stride = grid_size / 8; // coarse sampling stride to avoid O(n²) full scans

    for biome in target_biomes {
        let mut count = biome_data.iter().filter(|&&b| b == biome).count();
        let mut patch_centers: Vec<(usize, usize)> = Vec::new();
        let mut attempts = 0;

        while count < min_cells && attempts < 6 {
            attempts += 1;

            // Coarse-grid search: sample every `stride` cells for speed
            let mut best_score = f32::NEG_INFINITY;
            let mut best_ix = grid_size / 2;
            let mut best_iz = grid_size / 2;
            let sample_step = stride.max(1);

            let mut iz = margin;
            while iz < grid_size - margin {
                let mut ix = margin;
                while ix < grid_size - margin {
                    let idx = iz * grid_size + ix;
                    let existing = biome_data[idx];
                    if existing == biome
                        || matches!(existing, Biome::Water | Biome::Mountain | Biome::Beach)
                    {
                        ix += sample_step;
                        continue;
                    }

                    let h_norm = ((heights[idx] / AMPLITUDE) * 0.5 + 0.5).clamp(0.0, 1.0);
                    let suitability =
                        biome_suitability_score(biome, h_norm, ix, iz, grid_size, half_map);

                    // Spread bonus: distance from previous patch centers (cheap)
                    let spread_bonus = patch_centers
                        .iter()
                        .map(|&(cx, cz)| {
                            ((ix as f32 - cx as f32).powi(2) + (iz as f32 - cz as f32).powi(2))
                                .sqrt()
                        })
                        .min_by(|a, b| a.partial_cmp(b).unwrap())
                        .map(|d| (d / patch_radius as f32).min(2.0) * 0.5)
                        .unwrap_or(1.0);

                    let score = suitability + spread_bonus;
                    if score > best_score {
                        best_score = score;
                        best_ix = ix;
                        best_iz = iz;
                    }
                    ix += sample_step;
                }
                iz += sample_step;
            }

            patch_centers.push((best_ix, best_iz));

            // Stamp a circular patch
            let r = patch_radius as i32;
            let r_sq = r * r;
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dz * dz > r_sq {
                        continue;
                    }
                    let ix = best_ix as i32 + dx;
                    let iz = best_iz as i32 + dz;
                    if ix < 0 || iz < 0 || ix >= grid_size as i32 || iz >= grid_size as i32 {
                        continue;
                    }
                    let idx = iz as usize * grid_size + ix as usize;
                    if !matches!(biome_data[idx], Biome::Water | Biome::Mountain) {
                        if biome_data[idx] != biome {
                            count += 1;
                        }
                        biome_data[idx] = biome;
                    }
                }
            }
        }
    }
}

/// Score how suitable a grid cell is for a given biome.
fn biome_suitability_score(
    biome: Biome,
    height_norm: f32,
    ix: usize,
    iz: usize,
    grid_size: usize,
    half_map: f32,
) -> f32 {
    let step = (half_map * 2.0) / (grid_size - 1) as f32;
    let x = -half_map + ix as f32 * step;
    let z = -half_map + iz as f32 * step;
    let center_dist = (x * x + z * z).sqrt() / half_map;

    match biome {
        Biome::Wetland => -height_norm * 3.0 + center_dist * 0.5,
        Biome::Desert => {
            let ideal_height = 1.0 - (height_norm - 0.55).abs() * 4.0;
            ideal_height + center_dist * 0.3
        }
        Biome::Forest => {
            let ideal_height = 1.0 - (height_norm - 0.50).abs() * 3.0;
            ideal_height + center_dist * 0.2
        }
        Biome::Grassland => 1.0 - (height_norm - 0.48).abs() * 2.0,
        _ => 0.0,
    }
}

/// Flood-fill on the grid-cell (quad) level to find connected water regions.
/// A cell (ix, iz) is "wet" if any of its 4 corner vertices are below WATER_LEVEL.
/// Returns a Vec of water bodies, each being a Vec of (ix, iz) cell coordinates.
fn find_water_bodies(
    heights: &[f32],
    grid_size: usize,
    _step: f32,
    _half_map: f32,
) -> Vec<Vec<(usize, usize)>> {
    let cells = grid_size - 1;
    let mut visited = vec![false; cells * cells];
    let mut bodies = Vec::new();

    let is_wet = |ix: usize, iz: usize| -> bool {
        let tl = iz * grid_size + ix;
        let tr = tl + 1;
        let bl = tl + grid_size;
        let br = bl + 1;
        heights[tl] < WATER_LEVEL
            || heights[tr] < WATER_LEVEL
            || heights[bl] < WATER_LEVEL
            || heights[br] < WATER_LEVEL
    };

    for sz in 0..cells {
        for sx in 0..cells {
            let idx = sz * cells + sx;
            if visited[idx] || !is_wet(sx, sz) {
                continue;
            }
            // BFS flood-fill
            let mut body = Vec::new();
            let mut queue = VecDeque::new();
            visited[idx] = true;
            queue.push_back((sx, sz));
            while let Some((cx, cz)) = queue.pop_front() {
                body.push((cx, cz));
                for (dx, dz) in [(-1i32, 0), (1, 0), (0, -1), (0, 1)] {
                    let nx = cx as i32 + dx;
                    let nz = cz as i32 + dz;
                    if nx < 0 || nz < 0 || nx >= cells as i32 || nz >= cells as i32 {
                        continue;
                    }
                    let (nx, nz) = (nx as usize, nz as usize);
                    let ni = nz * cells + nx;
                    if !visited[ni] && is_wet(nx, nz) {
                        visited[ni] = true;
                        queue.push_back((nx, nz));
                    }
                }
            }
            if body.len() >= 4 {
                // Skip tiny 1-3 cell puddles
                bodies.push(body);
            }
        }
    }
    bodies
}

/// Compute the center and radius of a bounding sphere for a water body.
fn water_body_bounds(
    cells: &[(usize, usize)],
    _grid_size: usize,
    step: f32,
    half_map: f32,
) -> (Vec3, f32) {
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_z = f32::MAX;
    let mut max_z = f32::MIN;
    for &(cx, cz) in cells {
        let x0 = -half_map + cx as f32 * step;
        let x1 = -half_map + (cx + 1) as f32 * step;
        let z0 = -half_map + cz as f32 * step;
        let z1 = -half_map + (cz + 1) as f32 * step;
        min_x = min_x.min(x0);
        max_x = max_x.max(x1);
        min_z = min_z.min(z0);
        max_z = max_z.max(z1);
    }
    let center = Vec3::new((min_x + max_x) * 0.5, WATER_LEVEL, (min_z + max_z) * 0.5);
    let half_w = (max_x - min_x) * 0.5;
    let half_h = (max_z - min_z) * 0.5;
    let radius = (half_w * half_w + half_h * half_h).sqrt();
    (center, radius)
}

/// Build a mesh for a single water body from its constituent grid cells.
/// The mesh sits at WATER_LEVEL and clips to the terrain boundary where terrain
/// meets the water surface.
fn build_water_body_mesh(
    cells: &[(usize, usize)],
    _heights: &[f32],
    grid_size: usize,
    step: f32,
    half_map: f32,
    center: Vec3,
) -> Option<Mesh> {
    if cells.is_empty() {
        return None;
    }

    // Collect unique vertex indices used by these cells, and remap them for the mesh.
    use std::collections::HashMap;
    let mut vert_remap: HashMap<usize, u32> = HashMap::new();
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();

    let mut get_or_insert = |vi: usize| -> u32 {
        if let Some(&idx) = vert_remap.get(&vi) {
            return idx;
        }
        let iz = vi / grid_size;
        let ix = vi % grid_size;
        // Build positions relative to center so the entity Transform places them correctly
        let x = -half_map + ix as f32 * step - center.x;
        let z = -half_map + iz as f32 * step - center.z;
        let idx = positions.len() as u32;

        positions.push([x, WATER_LEVEL - center.y, z]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([
            ix as f32 / (grid_size - 1) as f32,
            iz as f32 / (grid_size - 1) as f32,
        ]);
        vert_remap.insert(vi, idx);
        idx
    };

    let mut indices: Vec<u32> = Vec::with_capacity(cells.len() * 6);

    for &(cx, cz) in cells {
        let tl = cz * grid_size + cx;
        let tr = tl + 1;
        let bl = tl + grid_size;
        let br = bl + 1;

        let i_tl = get_or_insert(tl);
        let i_tr = get_or_insert(tr);
        let i_bl = get_or_insert(bl);
        let i_br = get_or_insert(br);

        indices.push(i_tl);
        indices.push(i_bl);
        indices.push(i_tr);
        indices.push(i_tr);
        indices.push(i_bl);
        indices.push(i_br);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

fn spawn_mountain_border(
    mut commands: Commands,
    model_assets: Res<ModelAssets>,
    height_map: Res<HeightMap>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
) {
    if model_assets.mountains.is_empty() {
        return;
    }

    let settings = BorderSettings::from_map_size(config.map_size.world_size());
    let half_map = height_map.half_map;
    let mut rng = rand::rngs::StdRng::seed_from_u64(map_seed.0.wrapping_add(9000));
    let spacing = if height_map.map_size <= 320.0 {
        24.0
    } else if height_map.map_size <= 520.0 {
        28.0
    } else {
        34.0
    };
    let ring_depth = if height_map.map_size <= 320.0 { 1 } else { 2 };

    for layer in 0..ring_depth {
        let inset = settings.prop_inset + layer as f32 * spacing * 0.65;
        let limit = half_map - inset;
        let mut cursor = -limit;
        while cursor <= limit {
            spawn_border_mountain(
                &mut commands,
                &model_assets.mountains,
                &height_map,
                &mut rng,
                Vec3::new(cursor, 0.0, -limit),
                layer,
            );
            spawn_border_mountain(
                &mut commands,
                &model_assets.mountains,
                &height_map,
                &mut rng,
                Vec3::new(cursor, 0.0, limit),
                layer,
            );
            spawn_border_mountain(
                &mut commands,
                &model_assets.mountains,
                &height_map,
                &mut rng,
                Vec3::new(-limit, 0.0, cursor),
                layer,
            );
            spawn_border_mountain(
                &mut commands,
                &model_assets.mountains,
                &height_map,
                &mut rng,
                Vec3::new(limit, 0.0, cursor),
                layer,
            );
            cursor += spacing;
        }
    }
}

fn spawn_border_mountain(
    commands: &mut Commands,
    models: &[Handle<Scene>],
    height_map: &HeightMap,
    rng: &mut rand::rngs::StdRng,
    pos: Vec3,
    layer: usize,
) {
    use rand::Rng;

    let x = pos.x + rng.random_range(-5.0..5.0);
    let z = pos.z + rng.random_range(-5.0..5.0);
    let y = height_map.sample(x, z);
    let scene = models[rng.random_range(0..models.len())].clone();
    let base_scale = if layer == 0 { 2.8 * 5.0 } else { 2.1 * 5.0 };
    let scale = rng.random_range(base_scale * 0.9..base_scale * 1.15);

    commands.spawn((
        GameWorld,
        Decoration,
        FogHideable::Object,
        NotShadowCaster,
        SceneRoot(scene),
        Transform::from_translation(Vec3::new(x, y - 0.5, z))
            .with_rotation(Quat::from_rotation_y(
                rng.random_range(0.0..std::f32::consts::TAU),
            ))
            .with_scale(Vec3::splat(scale)),
    ));
}

fn enqueue_building_terrain_updates(
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

/// Process at most one terrain shape update per frame to avoid large GPU
/// re-upload spikes. Each update modifies only a small region of the height
/// map and mesh, but `meshes.get_mut()` marks the entire asset as changed,
/// triggering a full GPU re-upload (~3 MB for a large terrain). Limiting to
/// one per frame spreads this cost across frames.
fn process_terrain_shape_update_queue(
    mut queue: ResMut<TerrainShapeUpdateQueue>,
    mut height_map: ResMut<HeightMap>,
    mut sync_state: ResMut<TerrainShapeSyncState>,
    net_role: Option<Res<crate::multiplayer::NetRole>>,
    ground_q: Query<&Mesh3d, With<Ground>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if queue.pending.is_empty() {
        return;
    }

    let is_client = matches!(net_role.as_deref(), Some(crate::multiplayer::NetRole::Client));

    // Process only ONE update per frame to limit GPU re-upload cost.
    let Some(update) = queue.pending.pop_front() else {
        return;
    };

    let (_, outer_radius) = foundation_radii(update.footprint, height_map.step);
    let op_min_x = (((update.center[0] - outer_radius) + height_map.half_map) / height_map.step)
        .floor()
        .max(0.0) as usize;
    let op_max_x = (((update.center[0] + outer_radius) + height_map.half_map) / height_map.step)
        .ceil()
        .min((height_map.grid_size - 1) as f32) as usize;
    let op_min_z = (((update.center[1] - outer_radius) + height_map.half_map) / height_map.step)
        .floor()
        .max(0.0) as usize;
    let op_max_z = (((update.center[1] + outer_radius) + height_map.half_map) / height_map.step)
        .ceil()
        .min((height_map.grid_size - 1) as f32) as usize;

    let changed = apply_terrain_shape_op(&mut height_map, &update);

    sync_state.applied_history.insert(update.clone());
    sync_state.applied_history_ordered.push(update.clone());
    if !is_client {
        sync_state.pending_network.push(update);
    }

    if changed {
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
}

pub fn apply_terrain_shape_op(height_map: &mut HeightMap, update: &TerrainShapeOp) -> bool {
    let (inner_radius, outer_radius) = foundation_radii(update.footprint, height_map.step);
    let min_x = (((update.center[0] - outer_radius) + height_map.half_map) / height_map.step)
        .floor()
        .max(0.0) as usize;
    let max_x = (((update.center[0] + outer_radius) + height_map.half_map) / height_map.step)
        .ceil()
        .min((height_map.grid_size - 1) as f32) as usize;
    let min_z = (((update.center[1] - outer_radius) + height_map.half_map) / height_map.step)
        .floor()
        .max(0.0) as usize;
    let max_z = (((update.center[1] + outer_radius) + height_map.half_map) / height_map.step)
        .ceil()
        .min((height_map.grid_size - 1) as f32) as usize;

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

    for (pos, height) in positions.iter_mut().zip(height_map.heights.iter().copied()) {
        pos[1] = height;
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

    // Update positions in dirty region
    for iz in min_z..=max_z {
        for ix in min_x..=max_x {
            let idx = iz * height_map.grid_size + ix;
            positions[idx][1] = height_map.heights[idx];
        }
    }

    // Recalculate normals only in dirty region
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

pub fn terrain_heights_hash(height_map: &HeightMap) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for height in &height_map.heights {
        hash ^= height.to_bits() as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
