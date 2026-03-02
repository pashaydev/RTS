use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::components::Ground;

pub const MAP_SIZE: f32 = 500.0;
pub const HALF_MAP: f32 = 250.0;

const GRID_SIZE: usize = 201; // 200x200 cells
const NOISE_SCALE: f64 = 0.008;
const AMPLITUDE: f32 = 10.0;

fn noise_gen() -> Fbm<Perlin> {
    Fbm::<Perlin>::new(42).set_octaves(4)
}

pub fn terrain_height(x: f32, z: f32) -> f32 {
    let fbm = noise_gen();
    let val = fbm.get([x as f64 * NOISE_SCALE, z as f64 * NOISE_SCALE]) as f32;
    val * AMPLITUDE
}

fn height_color(h: f32) -> [f32; 4] {
    // Normalize height from [-AMPLITUDE, AMPLITUDE] to [0, 1]
    let t = ((h / AMPLITUDE) * 0.5 + 0.5).clamp(0.0, 1.0);

    if t < 0.45 {
        // Low: dark green grass
        [0.15, 0.45, 0.12, 1.0]
    } else if t < 0.55 {
        // Mid: lighter green
        let blend = (t - 0.45) / 0.1;
        [
            0.15 + blend * 0.15,
            0.45 + blend * 0.15,
            0.12 + blend * 0.08,
            1.0,
        ]
    } else if t < 0.7 {
        // Mid-high: brownish
        let blend = (t - 0.55) / 0.15;
        [
            0.3 + blend * 0.15,
            0.6 - blend * 0.2,
            0.2 - blend * 0.05,
            1.0,
        ]
    } else {
        // High: gray rock
        let blend = (t - 0.7) / 0.3;
        [
            0.45 + blend * 0.1,
            0.4 + blend * 0.05,
            0.15 + blend * 0.2,
            1.0,
        ]
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

    let step = MAP_SIZE / (GRID_SIZE - 1) as f32;
    let eps = 0.5_f32; // for normal calculation

    for iz in 0..GRID_SIZE {
        for ix in 0..GRID_SIZE {
            let x = -HALF_MAP + ix as f32 * step;
            let z = -HALF_MAP + iz as f32 * step;
            let y = terrain_height(x, z);

            positions.push([x, y, z]);
            uvs.push([ix as f32 / (GRID_SIZE - 1) as f32, iz as f32 / (GRID_SIZE - 1) as f32]);
            colors.push(height_color(y));

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

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(colors));
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

    // Directional light (sun)
    commands.spawn((
        DirectionalLight {
            illuminance: 8000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, 0.3, 0.0)),
    ));

    // Ambient light
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 300.0,
    });
}
