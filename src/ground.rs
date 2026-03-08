use bevy::prelude::*;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::components::{Biome, BiomeMap, Ground};

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
}

impl HeightMap {
    /// Sample terrain height at any world position using bilinear interpolation
    /// of the actual mesh grid vertices.
    pub fn sample(&self, x: f32, z: f32) -> f32 {
        let gx = (x + HALF_MAP) / self.step;
        let gz = (z + HALF_MAP) / self.step;
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

fn noise_gen() -> Fbm<Perlin> {
    Fbm::<Perlin>::new(42).set_octaves(4)
}

fn moisture_noise() -> Fbm<Perlin> {
    Fbm::<Perlin>::new(137).set_octaves(3)
}

fn temperature_noise() -> Fbm<Perlin> {
    Fbm::<Perlin>::new(271).set_octaves(3)
}

pub fn terrain_height(x: f32, z: f32) -> f32 {
    let fbm = noise_gen();
    let val = fbm.get([x as f64 * NOISE_SCALE, z as f64 * NOISE_SCALE]) as f32;
    val * AMPLITUDE
}

pub fn biome_at(x: f32, z: f32) -> Biome {
    let height = terrain_height(x, z);
    let height_norm = ((height / AMPLITUDE) * 0.5 + 0.5).clamp(0.0, 1.0);

    let moisture_fbm = moisture_noise();
    let moisture = (moisture_fbm.get([x as f64 * MOISTURE_SCALE, z as f64 * MOISTURE_SCALE]) as f32
        * 0.5
        + 0.5)
        .clamp(0.0, 1.0);

    let temp_fbm = temperature_noise();
    let temperature = (temp_fbm
        .get([x as f64 * TEMPERATURE_SCALE, z as f64 * TEMPERATURE_SCALE])
        as f32
        * 0.5
        + 0.5)
        .clamp(0.0, 1.0);

    // Very low areas -> Water
    if height_norm < 0.3 {
        return Biome::Water;
    }
    // Very high areas -> Mountain
    if height_norm > 0.75 {
        return Biome::Mountain;
    }
    // Hot and dry -> Desert
    if temperature > 0.6 && moisture < 0.4 {
        return Biome::Desert;
    }
    // Cool and wet -> Forest
    if moisture > 0.6 && temperature < 0.6 {
        return Biome::Forest;
    }
    // Default mid-range
    Biome::Mud
}

fn biome_color(biome: Biome, height_norm: f32) -> [f32; 4] {
    match biome {
        Biome::Forest => {
            let t = ((height_norm - 0.3) / 0.45).clamp(0.0, 1.0);
            [
                0.1 + t * 0.1,
                0.45 + t * 0.2,
                0.08 + t * 0.07,
                1.0,
            ]
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
            [
                0.5 + t * 0.35,
                0.48 + t * 0.35,
                0.45 + t * 0.35,
                1.0,
            ]
        }
    }
}

pub struct GroundPlugin;

impl Plugin for GroundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_ground);
    }
}

fn spawn_ground(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Generate terrain mesh
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(GRID_SIZE * GRID_SIZE);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(GRID_SIZE * GRID_SIZE);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(GRID_SIZE * GRID_SIZE);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(GRID_SIZE * GRID_SIZE);
    let mut biome_data: Vec<Biome> = Vec::with_capacity(GRID_SIZE * GRID_SIZE);

    let step = MAP_SIZE / (GRID_SIZE - 1) as f32;
    let eps = 0.5_f32; // for normal calculation

    for iz in 0..GRID_SIZE {
        for ix in 0..GRID_SIZE {
            let x = -HALF_MAP + ix as f32 * step;
            let z = -HALF_MAP + iz as f32 * step;
            let y = terrain_height(x, z);
            let height_norm = ((y / AMPLITUDE) * 0.5 + 0.5).clamp(0.0, 1.0);

            let biome = biome_at(x, z);
            biome_data.push(biome);

            positions.push([x, y, z]);
            uvs.push([
                ix as f32 / (GRID_SIZE - 1) as f32,
                iz as f32 / (GRID_SIZE - 1) as f32,
            ]);
            colors.push(biome_color(biome, height_norm));

            // Central-difference normals
            let h_l = terrain_height(x - eps, z);
            let h_r = terrain_height(x + eps, z);
            let h_d = terrain_height(x, z - eps);
            let h_u = terrain_height(x, z + eps);
            let normal = Vec3::new(h_l - h_r, 2.0 * eps, h_d - h_u).normalize();
            normals.push(normal.to_array());
        }
    }

    // Generate indices
    let mut indices: Vec<u32> = Vec::with_capacity((GRID_SIZE - 1) * (GRID_SIZE - 1) * 6);
    for iz in 0..(GRID_SIZE - 1) {
        for ix in 0..(GRID_SIZE - 1) {
            let tl = (iz * GRID_SIZE + ix) as u32;
            let tr = tl + 1;
            let bl = tl + GRID_SIZE as u32;
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
        grid_size: GRID_SIZE,
        step,
    });

    // Insert BiomeMap resource
    commands.insert_resource(BiomeMap {
        data: biome_data,
        grid_size: GRID_SIZE,
        map_size: MAP_SIZE,
    });
}
