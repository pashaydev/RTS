use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::components::{AppState, Biome, BiomeMap, GameSetupConfig, Ground, MapSeed};

pub const MAP_SIZE: f32 = 500.0;
pub const HALF_MAP: f32 = 250.0;

pub const GRID_SIZE: usize = 201; // 200x200 cells

/// Pre-computed grid of terrain heights that matches the rendered mesh exactly.
/// Use `sample(x, z)` for bilinear interpolation between grid vertices.
#[derive(Resource)]
pub struct HeightMap {
    pub heights: Vec<f32>,
    pub grid_size: usize,
    pub step: f32,
    pub map_size: f32,
    pub half_map: f32,
}

impl HeightMap {
    /// Sample terrain height at any world position using bilinear interpolation
    /// of the actual mesh grid vertices.
    pub fn sample(&self, x: f32, z: f32) -> f32 {
        let gx = (x + self.half_map) / self.step;
        let gz = (z + self.half_map) / self.step;
        let ix = (gx.floor().max(0.0) as usize).min(self.grid_size - 2);
        let iz = (gz.floor().max(0.0) as usize).min(self.grid_size - 2);
        let fx = (gx - ix as f32).clamp(0.0, 1.0);
        let fz = (gz - iz as f32).clamp(0.0, 1.0);

        let h00 = self.heights[iz * self.grid_size + ix];
        let h10 = self.heights[iz * self.grid_size + ix + 1];
        let h01 = self.heights[(iz + 1) * self.grid_size + ix];
        let h11 = self.heights[(iz + 1) * self.grid_size + ix + 1];

        let h0 = h00 + (h10 - h00) * fx;
        let h1 = h01 + (h11 - h01) * fx;
        h0 + (h1 - h0) * fz
    }
}
const NOISE_SCALE: f64 = 0.008;
const AMPLITUDE: f32 = 10.0;

const MOISTURE_SCALE: f64 = 0.005;
const TEMPERATURE_SCALE: f64 = 0.004;

/// Holds seed-derived noise generators for terrain generation.
pub struct TerrainNoise {
    height_fbm: Fbm<Perlin>,
    moisture_fbm: Fbm<Perlin>,
    temperature_fbm: Fbm<Perlin>,
}

impl TerrainNoise {
    pub fn from_seed(seed: u64) -> Self {
        let s0 = seed as u32;
        let s1 = (seed >> 16) as u32;
        let s2 = (seed >> 32) as u32;
        Self {
            height_fbm: Fbm::<Perlin>::new(s0).set_octaves(4),
            moisture_fbm: Fbm::<Perlin>::new(s1).set_octaves(3),
            temperature_fbm: Fbm::<Perlin>::new(s2).set_octaves(3),
        }
    }

    pub fn terrain_height(&self, x: f32, z: f32) -> f32 {
        let val = self
            .height_fbm
            .get([x as f64 * NOISE_SCALE, z as f64 * NOISE_SCALE]) as f32;
        val * AMPLITUDE
    }

    pub fn biome_at(&self, x: f32, z: f32) -> Biome {
        let height = self.terrain_height(x, z);
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

        if height_norm < 0.3 {
            return Biome::Water;
        }
        if height_norm > 0.75 {
            return Biome::Mountain;
        }
        if temperature > 0.6 && moisture < 0.4 {
            return Biome::Desert;
        }
        if moisture > 0.6 && temperature < 0.6 {
            return Biome::Forest;
        }
        Biome::Mud
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
        Biome::Forest => {
            let t = ((height_norm - 0.3) / 0.45).clamp(0.0, 1.0);
            [0.1 + t * 0.1, 0.45 + t * 0.2, 0.08 + t * 0.07, 1.0]
        }
        Biome::Desert => [
            0.85 + height_norm * 0.1,
            0.75 + height_norm * 0.1,
            0.45,
            1.0,
        ],
        Biome::Mud => [
            0.35 + height_norm * 0.15,
            0.25 + height_norm * 0.1,
            0.12 + height_norm * 0.08,
            1.0,
        ],
        Biome::Water => {
            let depth = 1.0 - height_norm;
            [
                0.05 + depth * 0.1,
                0.15 + depth * 0.2,
                0.5 + depth * 0.2,
                1.0,
            ]
        }
        Biome::Mountain => {
            let t = ((height_norm - 0.75) / 0.25).clamp(0.0, 1.0);
            [0.5 + t * 0.35, 0.48 + t * 0.35, 0.45 + t * 0.35, 1.0]
        }
    }
}

pub struct GroundPlugin;

impl Plugin for GroundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::InGame),
            (resolve_map_seed, spawn_ground).chain(),
        );
    }
}

pub fn spawn_ground(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
) {
    let noise = TerrainNoise::from_seed(map_seed.0);

    let actual_map_size = config.map_size.world_size();
    let actual_half_map = actual_map_size / 2.0;
    let actual_grid_size = (actual_map_size / 2.5) as usize + 1;

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
            let y = noise.terrain_height(x, z);
            let height_norm = ((y / AMPLITUDE) * 0.5 + 0.5).clamp(0.0, 1.0);

            let biome = noise.biome_at(x, z);
            biome_data.push(biome);

            positions.push([x, y, z]);
            uvs.push([
                ix as f32 / (actual_grid_size - 1) as f32,
                iz as f32 / (actual_grid_size - 1) as f32,
            ]);
            colors.push(biome_color(biome, height_norm));

            // Central-difference normals
            let h_l = noise.terrain_height(x - eps, z);
            let h_r = noise.terrain_height(x + eps, z);
            let h_d = noise.terrain_height(x, z - eps);
            let h_u = noise.terrain_height(x, z + eps);
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
        Ground,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::from_translation(Vec3::ZERO),
    ));
    commands.insert_resource(HeightMap {
        heights: grid_heights,
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
}
