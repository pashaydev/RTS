//! Initial terrain + water mesh spawn and texture baking at game start,
//! reading `MapSeed` and `GameSetupConfig` for procedural generation.

use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::tasks::ComputeTaskPool;

use crate::presentation::materials::terrain::{TerrainExtension, TerrainMaterial, TerrainSettings};
use crate::presentation::materials::water::{WaterExtension, WaterMaterial, WaterSettings};
use crate::types::{Biome, BiomeMap, CullingBounds, GameSetupConfig, GameWorld, Ground, MapSeed};

use super::data::{HeightMap, AMPLITUDE};
use super::noise::{blended_biome_color_patched, TerrainNoise};
use super::water::{build_water_body_mesh, find_water_bodies, water_body_bounds, WaterPlane};

#[derive(Resource)]
pub struct TerrainTextures {
    pub grass: Handle<Image>,
    pub rock: Handle<Image>,
    pub sand: Handle<Image>,
    pub snow: Handle<Image>,
    pub floor: Handle<Image>,
}

#[derive(Resource, Default)]
pub struct TerrainTextureSamplerState {
    configured: bool,
}

pub fn load_terrain_textures(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(TerrainTextures {
        grass: asset_server.load("textures/terrain/grass.png"),
        rock: asset_server.load("textures/terrain/rock.png"),
        sand: asset_server.load("textures/terrain/sand.png"),
        snow: asset_server.load("textures/terrain/snow.png"),
        floor: asset_server.load("textures/floor_stone.png"),
    });
    commands.insert_resource(TerrainTextureSamplerState::default());
}

pub fn configure_terrain_texture_samplers(
    terrain_textures: Res<TerrainTextures>,
    mut sampler_state: ResMut<TerrainTextureSamplerState>,
    mut images: ResMut<Assets<Image>>,
) {
    if sampler_state.configured {
        return;
    }

    let handles = [
        &terrain_textures.grass,
        &terrain_textures.rock,
        &terrain_textures.sand,
        &terrain_textures.snow,
        &terrain_textures.floor,
    ];

    if handles.iter().any(|handle| images.get(*handle).is_none()) {
        return;
    }

    for handle in handles {
        let Some(image) = images.get_mut(handle) else {
            return;
        };

        let mut sampler = ImageSamplerDescriptor::linear();
        sampler.address_mode_u = ImageAddressMode::Repeat;
        sampler.address_mode_v = ImageAddressMode::Repeat;
        sampler.address_mode_w = ImageAddressMode::Repeat;
        sampler.set_anisotropic_filter(8);
        image.sampler = ImageSampler::Descriptor(sampler);
    }

    sampler_state.configured = true;
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
    let step = actual_map_size / (actual_grid_size - 1) as f32;
    let eps = 0.5_f32;

    // Parallel vertex pass: each row is an independent task operating on a
    // disjoint slice of each output buffer. Every noise lookup is a pure
    // function of (x, z), so the only sharing is the read-only `noise`.
    let cell_count = actual_grid_size * actual_grid_size;
    let mut positions: Vec<[f32; 3]> = vec![[0.0; 3]; cell_count];
    let mut normals: Vec<[f32; 3]> = vec![[0.0; 3]; cell_count];
    let mut uvs: Vec<[f32; 2]> = vec![[0.0; 2]; cell_count];
    let mut biome_data: Vec<Biome> = vec![Biome::Grassland; cell_count];

    {
        let noise_ref = &noise;
        let row_stride = actual_grid_size;
        let inv_last = 1.0 / (actual_grid_size - 1) as f32;
        let pos_rows = positions.chunks_mut(row_stride);
        let nrm_rows = normals.chunks_mut(row_stride);
        let uv_rows = uvs.chunks_mut(row_stride);
        let bio_rows = biome_data.chunks_mut(row_stride);

        ComputeTaskPool::get().scope(|s| {
            for (iz, (((pos_row, nrm_row), uv_row), bio_row)) in pos_rows
                .zip(nrm_rows)
                .zip(uv_rows)
                .zip(bio_rows)
                .enumerate()
            {
                s.spawn(async move {
                    let z = -actual_half_map + iz as f32 * step;
                    let v = iz as f32 * inv_last;
                    for ix in 0..actual_grid_size {
                        let x = -actual_half_map + ix as f32 * step;
                        let y = noise_ref.terrain_height(x, z, actual_half_map);

                        bio_row[ix] = noise_ref.biome_at(x, z, actual_half_map);
                        pos_row[ix] = [x, y, z];
                        uv_row[ix] = [ix as f32 * inv_last, v];

                        let h_l = noise_ref.terrain_height(x - eps, z, actual_half_map);
                        let h_r = noise_ref.terrain_height(x + eps, z, actual_half_map);
                        let h_d = noise_ref.terrain_height(x, z - eps, actual_half_map);
                        let h_u = noise_ref.terrain_height(x, z + eps, actual_half_map);
                        let normal = Vec3::new(h_l - h_r, 2.0 * eps, h_d - h_u).normalize();
                        nrm_row[ix] = normal.to_array();
                    }
                });
            }
        });
    }

    let grid_heights: Vec<f32> = positions.iter().map(|position| position[1]).collect();
    ensure_all_biomes(
        &mut biome_data,
        actual_grid_size,
        &grid_heights,
        actual_half_map,
    );

    // Parallel biome-color pass: reads only `noise` and `biome_data` (now
    // finalized above), so rows can be filled in parallel.
    let mut colors: Vec<[f32; 4]> = vec![[0.0; 4]; cell_count];
    {
        let noise_ref = &noise;
        let biome_ref: &[Biome] = &biome_data;
        let row_stride = actual_grid_size;
        let col_rows = colors.chunks_mut(row_stride);

        ComputeTaskPool::get().scope(|s| {
            for (iz, col_row) in col_rows.enumerate() {
                s.spawn(async move {
                    let z = -actual_half_map + iz as f32 * step;
                    for ix in 0..actual_grid_size {
                        let x = -actual_half_map + ix as f32 * step;
                        col_row[ix] = blended_biome_color_patched(
                            noise_ref,
                            biome_ref,
                            actual_grid_size,
                            x,
                            z,
                            actual_half_map,
                            step,
                        );
                    }
                });
            }
        });
    }

    let mut indices = Vec::with_capacity((actual_grid_size - 1) * (actual_grid_size - 1) * 6);
    for iz in 0..(actual_grid_size - 1) {
        for ix in 0..(actual_grid_size - 1) {
            let tl = (iz * actual_grid_size + ix) as u32;
            let tr = tl + 1;
            let bl = tl + actual_grid_size as u32;
            let br = bl + 1;

            indices.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
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
        Transform::default(),
    ));

    let water_material = water_materials.add(WaterMaterial {
        base: StandardMaterial {
            base_color: Color::srgba(0.05, 0.12, 0.20, 0.72),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.08,
            metallic: 0.0,
            reflectance: 0.30,
            double_sided: true,
            ..default()
        },
        extension: WaterExtension {
            settings: WaterSettings::default(),
            fog_visible_texture: None,
            fog_explored_texture: None,
        },
    });
    let water_bodies = find_water_bodies(&grid_heights, actual_grid_size);

    for body_cells in &water_bodies {
        let (center, radius) = water_body_bounds(body_cells, step, actual_half_map);
        if let Some(mesh) =
            build_water_body_mesh(body_cells, actual_grid_size, step, actual_half_map, center)
        {
            commands.spawn((
                GameWorld,
                WaterPlane,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(water_material.clone()),
                Transform::from_translation(center),
                CullingBounds::new(radius),
            ));
        }
    }

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

fn ensure_all_biomes(biome_data: &mut [Biome], grid_size: usize, heights: &[f32], half_map: f32) {
    let total_cells = biome_data.len();
    let min_cells = total_cells / 20;
    let target_biomes = [
        Biome::Wetland,
        Biome::Desert,
        Biome::Forest,
        Biome::Grassland,
    ];

    let patch_radius: usize = (grid_size / 14).max(5);
    let margin = patch_radius + 4;
    let stride = grid_size / 8;

    for biome in target_biomes {
        let mut count = biome_data
            .iter()
            .filter(|&&candidate| candidate == biome)
            .count();
        let mut patch_centers = Vec::new();
        let mut attempts = 0;

        while count < min_cells && attempts < 6 {
            attempts += 1;

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

                    let spread_bonus = patch_centers
                        .iter()
                        .map(|&(cx, cz)| {
                            ((ix as f32 - cx as f32).powi(2) + (iz as f32 - cz as f32).powi(2))
                                .sqrt()
                        })
                        .min_by(|left, right| left.total_cmp(right))
                        .map(|distance| (distance / patch_radius as f32).min(2.0) * 0.5)
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

            let radius = patch_radius as i32;
            let radius_sq = radius * radius;
            for dz in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dz * dz > radius_sq {
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
