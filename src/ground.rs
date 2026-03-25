use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use rand::SeedableRng;

use crate::components::{
    AppState, Biome, BiomeMap, GameSetupConfig, GameWorld, Ground, MapSeed, ModelAssets, RtsCamera,
};
use crate::lighting::SunLight;
use crate::terrain_material::{TerrainMaterial, TerrainSettings};
use crate::water_material::{WaterMaterial, WaterSettings};

pub const MAP_SIZE: f32 = 500.0;
pub const HALF_MAP: f32 = 250.0;

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

        // Soft terracing for sculpted plateau look
        let terrace_scale = 0.25;
        let terraced = (height * terrace_scale).round() / terrace_scale;
        height = height * 0.7 + terraced * 0.3;

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

    pub fn biome_at(&self, x: f32, z: f32, half_map: f32) -> Biome {
        let border = BorderSettings::from_map_size(half_map * 2.0);
        let edge_distance = edge_distance_to_square(x, z, half_map);
        if edge_distance <= border.thickness {
            return Biome::Mountain;
        }

        let height = self.terrain_height(x, z, half_map);
        let height_norm = ((height / AMPLITUDE) * 0.5 + 0.5).clamp(0.0, 1.0);

        let moisture = (self
            .moisture_fbm
            .get([x as f64 * MOISTURE_SCALE, z as f64 * MOISTURE_SCALE])
            as f32
            * 0.5
            + 0.5)
            .clamp(0.0, 1.0);

        let temperature = (self
            .temperature_fbm
            .get([x as f64 * TEMPERATURE_SCALE, z as f64 * TEMPERATURE_SCALE])
            as f32
            * 0.5
            + 0.5)
            .clamp(0.0, 1.0);

        if height_norm < 0.28 {
            return Biome::Water;
        }
        if height_norm < 0.33 {
            return Biome::Beach;
        }
        if height_norm > 0.75 {
            return Biome::Mountain;
        }
        if temperature > 0.65 && moisture < 0.35 {
            return Biome::Desert;
        }
        if moisture > 0.55 {
            return Biome::Forest;
        }
        if height_norm < 0.42 && moisture > 0.5 {
            return Biome::Wetland;
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

/// Sample biome at 9 nearby offsets and return a blended vertex color.
fn blended_biome_color(
    noise: &TerrainNoise,
    x: f32,
    z: f32,
    half_map: f32,
    blend_radius: f32,
) -> [f32; 4] {
    let offsets: [(f32, f32); 9] = [
        (0.0, 0.0),
        (-1.0, 0.0),
        (1.0, 0.0),
        (0.0, -1.0),
        (0.0, 1.0),
        (-1.0, -1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (1.0, 1.0),
    ];
    let mut weights = [0.0f32; BIOME_COUNT];
    let mut height_norms = [0.0f32; BIOME_COUNT];
    let mut weight_counts = [0.0f32; BIOME_COUNT];

    for &(dx, dz) in &offsets {
        let sx = x + dx * blend_radius;
        let sz = z + dz * blend_radius;
        let h = noise.terrain_height(sx, sz, half_map);
        let hn = ((h / AMPLITUDE) * 0.5 + 0.5).clamp(0.0, 1.0);
        let b = noise.biome_at(sx, sz, half_map);
        let idx = biome_index(b);
        weights[idx] += 1.0;
        height_norms[idx] += hn;
        weight_counts[idx] += 1.0;
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
            let avg_hn = height_norms[i] / weight_counts[i];
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
            .add_systems(Startup, load_terrain_textures)
            .add_systems(
                OnEnter(AppState::InGame),
                (resolve_map_seed, spawn_ground, spawn_mountain_border).chain(),
            )
            .add_systems(
                Update,
                (update_water_time, update_terrain_lighting).run_if(in_state(AppState::InGame)),
            );
    }
}

fn update_terrain_lighting(
    mut materials: ResMut<Assets<TerrainMaterial>>,
    terrain_q: Query<&MeshMaterial3d<TerrainMaterial>, With<Ground>>,
    sun_q: Query<(&DirectionalLight, &Transform), With<SunLight>>,
    ambient: Res<GlobalAmbientLight>,
) {
    let Ok(mat_handle) = terrain_q.single() else {
        return;
    };
    let Some(mat) = materials.get_mut(&mat_handle.0) else {
        return;
    };

    // Sync sun direction, color, intensity from scene DirectionalLight
    if let Ok((sun_light, sun_tf)) = sun_q.single() {
        let sun_dir = sun_tf.forward().as_vec3();
        // DirectionalLight forward points *toward* what it illuminates,
        // but in lighting math we want the direction *from* the surface to the light
        mat.settings.sun_direction = Vec4::new(-sun_dir.x, -sun_dir.y, -sun_dir.z, 0.0);
        let c = sun_light.color.to_srgba();
        mat.settings.sun_color = Vec4::new(c.red, c.green, c.blue, 1.0);
        // Normalize illuminance: peak sun ~6000 lux in this game's day cycle
        mat.settings.sun_intensity = (sun_light.illuminance / 6000.0).clamp(0.0, 2.0);
    }

    // Sync ambient light — normalize brightness (peaks at ~300 in day cycle)
    let ac = ambient.color.to_srgba();
    mat.settings.ambient_color = Vec4::new(ac.red, ac.green, ac.blue, 1.0);
    mat.settings.ambient_brightness = (ambient.brightness / 300.0).clamp(0.0, 1.0) * 0.5;
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

    // Generate terrain mesh
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(actual_grid_size * actual_grid_size);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(actual_grid_size * actual_grid_size);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(actual_grid_size * actual_grid_size);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(actual_grid_size * actual_grid_size);
    let mut biome_data: Vec<Biome> = Vec::with_capacity(actual_grid_size * actual_grid_size);

    let step = actual_map_size / (actual_grid_size - 1) as f32;
    let eps = 0.5_f32; // for normal calculation

    for iz in 0..actual_grid_size {
        for ix in 0..actual_grid_size {
            let x = -actual_half_map + ix as f32 * step;
            let z = -actual_half_map + iz as f32 * step;
            let y = noise.terrain_height(x, z, actual_half_map);

            let biome = noise.biome_at(x, z, actual_half_map);
            biome_data.push(biome);

            positions.push([x, y, z]);
            uvs.push([
                ix as f32 / (actual_grid_size - 1) as f32,
                iz as f32 / (actual_grid_size - 1) as f32,
            ]);
            colors.push(blended_biome_color(&noise, x, z, actual_half_map, step));

            // Central-difference normals
            let h_l = noise.terrain_height(x - eps, z, actual_half_map);
            let h_r = noise.terrain_height(x + eps, z, actual_half_map);
            let h_d = noise.terrain_height(x, z - eps, actual_half_map);
            let h_u = noise.terrain_height(x, z + eps, actual_half_map);
            let normal = Vec3::new(h_l - h_r, 2.0 * eps, h_d - h_u).normalize();
            normals.push(normal.to_array());
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

    // Build HeightMap from the same grid heights used by the mesh
    let grid_heights: Vec<f32> = positions.iter().map(|p| p[1]).collect();

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
            settings: TerrainSettings {
                amplitude: AMPLITUDE,
                ..default()
            },
            grass_texture: Some(terrain_textures.grass.clone()),
            rock_texture: Some(terrain_textures.rock.clone()),
            sand_texture: Some(terrain_textures.sand.clone()),
            snow_texture: Some(terrain_textures.snow.clone()),
        })),
        Transform::from_translation(Vec3::ZERO),
    ));
    commands.insert_resource(HeightMap {
        heights: grid_heights.clone(),
        natural_heights: grid_heights,
        grid_size: actual_grid_size,
        step,
        map_size: actual_map_size,
        half_map: actual_half_map,
    });

    // Insert BiomeMap resource
    commands.insert_resource(BiomeMap {
        data: biome_data,
        grid_size: actual_grid_size,
        map_size: actual_map_size,
    });

    // Spawn water plane
    let water_level = WATER_LEVEL;
    let water_mesh = Mesh::from(Plane3d::new(Vec3::Y, Vec2::splat(actual_half_map)));
    commands.spawn((
        GameWorld,
        WaterPlane,
        Mesh3d(meshes.add(water_mesh)),
        MeshMaterial3d(water_materials.add(WaterMaterial {
            settings: WaterSettings::default(),
        })),
        Transform::from_translation(Vec3::new(0.0, water_level, 0.0)),
    ));
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
        SceneRoot(scene),
        Transform::from_translation(Vec3::new(x, y - 0.5, z))
            .with_rotation(Quat::from_rotation_y(
                rng.random_range(0.0..std::f32::consts::TAU),
            ))
            .with_scale(Vec3::splat(scale)),
    ));
}
