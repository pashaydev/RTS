use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::types::*;
use crate::world::fog::FogTweakSettings;
use crate::world::ground::{is_in_mountain_border, BorderSettings, HeightMap};
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;

use super::spawning::{dead_tree_wood_amount, terrain_translation};

// ── Decoration spawning ──

/// Decorations per biome: (grass_weight, bush_weight, rock_weight, dead_tree_weight)
/// Weights control relative probability; 0 means none.
fn biome_decoration_weights(biome: Biome) -> (f32, f32, f32, f32) {
    match biome {
        Biome::Grassland => (0.3, 0.25, 0.0, 0.0),
        // Forest grass weight 0 — dense grass handled separately via GPU instancing
        Biome::Forest => (0.0, 0.35, 0.0, 0.0),
        Biome::Desert => (0.0, 0.0, 0.0, 0.3),
        Biome::Beach => (0.0, 0.0, 0.0, 0.0),
        Biome::Wetland => (0.35, 0.35, 0.0, 0.0),
        Biome::Mountain => (0.0, 0.0, 0.0, 0.15),
        Biome::Water => (0.0, 0.0, 0.0, 0.0),
    }
}

enum DecoKind {
    Grass,
    Bush,
    Rock,
    DeadTree,
}

pub(super) fn spawn_decorations(
    mut commands: Commands,
    biome_map: Res<BiomeMap>,
    model_assets: Res<ModelAssets>,
    height_map: Res<HeightMap>,
    tree_growth_config: Res<TreeGrowthConfig>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
) {
    let mut rng = StdRng::seed_from_u64(map_seed.0.wrapping_add(2000));
    let deco_seed = (map_seed.0 >> 12) as u32;
    let deco_noise = Fbm::<Perlin>::new(deco_seed).set_octaves(2);
    let spacing = 8.0;
    let half = height_map.half_map;
    let border = BorderSettings::from_map_size(height_map.map_size);
    let max_decorations = ((height_map.map_size / 500.0).powi(2) * 700.0) as u32;
    let mut count = 0u32;

    // Collect pending placements for chunk-merged decorations
    let pending_bushes: Vec<(usize, Vec3, f32, f32)> = Vec::new();
    let mut pending_rocks: Vec<(usize, Vec3, f32, f32)> = Vec::new();
    let mut pending_grass: Vec<(usize, Vec3, f32, f32)> = Vec::new();

    let mut x = -half + 4.0;
    while x < half - 4.0 {
        let mut z = -half + 4.0;
        while z < half - 4.0 {
            if count >= max_decorations {
                break;
            }

            if is_in_mountain_border(x, z, half, border) {
                z += spacing;
                continue;
            }

            // Keep starting areas clear (all faction spawn positions)
            let deco_spawn_positions = config.spawn_positions(map_seed.0);
            let mut too_close_to_spawn = false;
            for &(_, (sx, sz)) in &deco_spawn_positions {
                let dx = x - sx;
                let dz = z - sz;
                if (dx * dx + dz * dz).sqrt() < 25.0 {
                    too_close_to_spawn = true;
                    break;
                }
            }
            if too_close_to_spawn {
                z += spacing;
                continue;
            }

            let biome = biome_map.get_biome(x, z);
            let (gw, bw, rw, dw) = biome_decoration_weights(biome);
            let total_weight = gw + bw + rw + dw;
            if total_weight < 0.01 {
                z += spacing;
                continue;
            }

            let noise_val = deco_noise.get([x as f64 * 0.15, z as f64 * 0.15]) as f32;
            if noise_val < 0.05 {
                z += spacing;
                continue;
            }

            // Pick decoration kind based on weights
            let roll = rng.random_range(0.0..total_weight);
            let kind = if roll < gw {
                DecoKind::Grass
            } else if roll < gw + bw {
                DecoKind::Bush
            } else if roll < gw + bw + rw {
                DecoKind::Rock
            } else {
                DecoKind::DeadTree
            };

            let (models, scale_min, scale_max) = match kind {
                DecoKind::Grass => (&model_assets.grass, 0.6_f32, 1.0_f32),
                DecoKind::Bush => (&model_assets.bushes, 0.05, 0.21),
                DecoKind::Rock => (&model_assets.rocks, 0.8, 1.5),
                DecoKind::DeadTree => (&model_assets.dead_trees, 0.7, 1.1),
            };

            if !models.is_empty() {
                let variant_idx = rng.random_range(0..models.len());
                let ox = x + rng.random_range(-2.0_f32..2.0);
                let oz = z + rng.random_range(-2.0_f32..2.0);
                let y = terrain_translation(&height_map, ox, oz, 0.0).y;
                let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);
                let scale = rng.random_range(scale_min..scale_max);
                let pos = Vec3::new(ox, y, oz);

                if matches!(kind, DecoKind::DeadTree) {
                    // Dead trees are harvestable — keep as individual entities.
                    commands.spawn((
                        GameWorld,
                        ResourceNode {
                            resource_type: ResourceType::Wood,
                            amount_remaining: dead_tree_wood_amount(&tree_growth_config),
                        },
                        TerrainHeightOffset(0.0),
                        FogHideable::Object,
                        PickRadius(2.0 * scale),
                        SceneRoot(models[variant_idx].clone()),
                        NotShadowCaster,
                        Transform::from_translation(pos)
                            .with_rotation(Quat::from_rotation_y(y_rotation))
                            .with_scale(Vec3::splat(scale)),
                    ));
                } else if matches!(kind, DecoKind::Bush) {
                    // Bushes need per-instance fog hiding; chunk merging reveals too much.
                    commands.spawn((
                        GameWorld,
                        Decoration,
                        FogHideable::Object,
                        CullingBounds::new((2.0 * scale).max(1.5)),
                        CullReason::default(),
                        SceneRoot(models[variant_idx].clone()),
                        NotShadowCaster,
                        Transform::from_translation(pos)
                            .with_rotation(Quat::from_rotation_y(y_rotation))
                            .with_scale(Vec3::splat(scale)),
                    ));
                } else {
                    // Grass and rocks → collect for chunk merging.
                    let placement = (variant_idx, pos, scale, y_rotation);
                    match kind {
                        DecoKind::Rock => pending_rocks.push(placement),
                        DecoKind::Grass => pending_grass.push(placement),
                        DecoKind::Bush | DecoKind::DeadTree => unreachable!(),
                    }
                }
                count += 1;
            }

            z += spacing;
        }
        x += spacing;
    }

    // Store pending placements for the deferred chunk-merge system
    commands.insert_resource(PendingDecorationPlacements {
        bushes: pending_bushes,
        rocks: pending_rocks,
        grass: pending_grass,
    });
}

/// Deferred system: once DecorationInstanceAssets are extracted from GLTF,
/// consume pending placements and build chunk-merged meshes.
const DECO_CHUNK_SIZE: f32 = 32.0;

pub(super) fn build_decoration_chunks(
    mut commands: Commands,
    deco_assets: Option<Res<DecorationInstanceAssets>>,
    pending: Option<Res<PendingDecorationPlacements>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut has_run: Local<bool>,
) {
    if *has_run {
        return;
    }
    let Some(deco_assets) = deco_assets else {
        return;
    };
    let Some(pending) = pending else { return };

    // Pre-extract source mesh data for each variant
    let bush_sources: Vec<Option<SourceMeshData>> = deco_assets
        .bushes
        .iter()
        .map(|(mesh_h, _)| meshes.get(mesh_h).and_then(SourceMeshData::from_mesh))
        .collect();
    let rock_sources: Vec<Option<SourceMeshData>> = deco_assets
        .rocks
        .iter()
        .map(|(mesh_h, _)| meshes.get(mesh_h).and_then(SourceMeshData::from_mesh))
        .collect();
    let grass_sources: Vec<Option<SourceMeshData>> = deco_assets
        .grass
        .iter()
        .map(|(mesh_h, _)| meshes.get(mesh_h).and_then(SourceMeshData::from_mesh))
        .collect();

    // Check all meshes are loaded
    if bush_sources.iter().any(|s| s.is_none()) && !deco_assets.bushes.is_empty() {
        return;
    }
    if rock_sources.iter().any(|s| s.is_none()) && !deco_assets.rocks.is_empty() {
        return;
    }
    if grass_sources.iter().any(|s| s.is_none()) && !deco_assets.grass.is_empty() {
        return;
    }

    *has_run = true;

    let mut chunk_map = DecoChunkMap::default();
    let mut total_count = 0usize;

    // Expand bush placements: each placement uses a model index (0,1,...) but the
    // flat assets list has multiple primitives per model. Expand into one placement
    // per primitive so the generic loop can index directly.
    let expanded_bushes: Vec<(usize, Vec3, f32, f32)> = {
        // Build cumulative offsets: model_starts[i] = sum of sizes before model i
        let model_starts: Vec<usize> = deco_assets
            .bush_model_sizes
            .iter()
            .scan(0usize, |acc, &sz| {
                let start = *acc;
                *acc += sz;
                Some(start)
            })
            .collect();
        let num_models = deco_assets.bush_model_sizes.len();

        let mut expanded = Vec::with_capacity(pending.bushes.len() * 2);
        for &(model_idx, pos, scale, y_rot) in &pending.bushes {
            let mi = model_idx.min(num_models.saturating_sub(1));
            let start = model_starts[mi];
            let count = deco_assets.bush_model_sizes[mi];
            for prim in 0..count {
                expanded.push((start + prim, pos, scale, y_rot));
            }
        }
        expanded
    };

    // Process each decoration category
    let categories: &[(
        &[(usize, Vec3, f32, f32)],
        &[Option<SourceMeshData>],
        &[(Handle<Mesh>, Handle<StandardMaterial>)],
    )] = &[
        (&expanded_bushes, &bush_sources, &deco_assets.bushes),
        (&pending.rocks, &rock_sources, &deco_assets.rocks),
        (&pending.grass, &grass_sources, &deco_assets.grass),
    ];

    for &(placements, sources, assets) in categories {
        if placements.is_empty() || assets.is_empty() {
            continue;
        }

        // Group placements by (chunk_coord, material_handle_id) for batching
        // Variants that share the same material get merged into one chunk mesh.
        let inv_chunk = 1.0 / DECO_CHUNK_SIZE;
        let mut chunk_groups: std::collections::HashMap<
            (i32, i32, AssetId<StandardMaterial>),
            Vec<(Vec3, f32, f32)>,
        > = std::collections::HashMap::new();
        // Track which material handle to use per asset id
        let mut mat_handles: std::collections::HashMap<
            AssetId<StandardMaterial>,
            Handle<StandardMaterial>,
        > = std::collections::HashMap::new();

        for &(variant_idx, pos, scale, y_rot) in placements {
            let vi = variant_idx.min(assets.len() - 1);
            let src = match &sources[vi] {
                Some(s) => s,
                None => continue,
            };
            let (_, ref mat_handle) = assets[vi];
            let mat_id = mat_handle.id();
            mat_handles
                .entry(mat_id)
                .or_insert_with(|| mat_handle.clone());

            let cx = (pos.x * inv_chunk).floor() as i32;
            let cz = (pos.z * inv_chunk).floor() as i32;

            // For simplicity, merge all variants with same material into one mesh per chunk.
            // Different variants may have different geometry but same texture atlas — this is fine,
            // merge_instances_into_mesh handles varying source meshes per-instance.
            // However our helper takes a single source mesh. So we group by (chunk, material, variant).
            // To keep it simple and correct: group by (chunk, variant).
            chunk_groups
                .entry((cx, cz, mat_id))
                .or_default()
                .push((pos, scale, y_rot));
            let _ = src; // used below
        }

        // Actually, we need to handle multiple variants with potentially different meshes
        // but same material. Let's regroup by (chunk, variant) to be safe.
        let mut variant_chunk_groups: std::collections::HashMap<
            (i32, i32, usize),
            Vec<(Vec3, f32, f32)>,
        > = std::collections::HashMap::new();

        for &(variant_idx, pos, scale, y_rot) in placements {
            let vi = variant_idx.min(assets.len() - 1);
            let cx = (pos.x * inv_chunk).floor() as i32;
            let cz = (pos.z * inv_chunk).floor() as i32;
            variant_chunk_groups
                .entry((cx, cz, vi))
                .or_default()
                .push((pos, scale, y_rot));
        }

        // Now for each (chunk, variant) group, merge and spawn
        // But we want to further merge variants with the same material into one mesh per chunk.
        // Build a per-chunk, per-material merged mesh by iterating all variant groups in that chunk.
        let mut chunk_meshes: std::collections::HashMap<
            (i32, i32, AssetId<StandardMaterial>),
            (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>),
        > = std::collections::HashMap::new();

        for ((cx, cz, vi), instances) in &variant_chunk_groups {
            let src = match &sources[*vi] {
                Some(s) => s,
                None => continue,
            };
            let (_, ref mat_handle) = assets[*vi];
            let mat_id = mat_handle.id();

            let entry = chunk_meshes
                .entry((*cx, *cz, mat_id))
                .or_insert_with(|| (Vec::new(), Vec::new(), Vec::new(), Vec::new()));

            for &(pos, scale, y_rot) in instances {
                let rot = Quat::from_rotation_y(y_rot);
                let base_idx = entry.0.len() as u32;

                for vi_idx in 0..src.positions.len() {
                    let sp = Vec3::from(src.positions[vi_idx]) * scale;
                    let transformed = rot * sp + pos;
                    entry.0.push([transformed.x, transformed.y, transformed.z]);

                    if vi_idx < src.normals.len() {
                        let sn = Vec3::from(src.normals[vi_idx]);
                        let tn = rot * sn;
                        entry.1.push([tn.x, tn.y, tn.z]);
                    } else {
                        entry.1.push([0.0, 1.0, 0.0]);
                    }

                    if vi_idx < src.uvs.len() {
                        entry.2.push(src.uvs[vi_idx]);
                    } else {
                        entry.2.push([0.0, 0.0]);
                    }
                }

                for &idx in &src.indices {
                    entry.3.push(base_idx + idx);
                }
                total_count += 1;
            }
        }

        // Spawn chunk entities
        for ((cx, cz, mat_id), (positions, normals, uvs, indices)) in chunk_meshes {
            if positions.is_empty() {
                continue;
            }

            // Compute chunk center from vertex positions and shift vertices
            // into local space so the entity Transform sits at the chunk center.
            // This gives Bevy correct frustum culling via the auto-computed Aabb.
            let chunk_center = Vec3::new(
                (cx as f32 + 0.5) * DECO_CHUNK_SIZE,
                0.0,
                (cz as f32 + 0.5) * DECO_CHUNK_SIZE,
            );
            // Compute Y center from actual vertex data for a tighter Aabb
            let (mut y_min, mut y_max) = (f32::MAX, f32::MIN);
            let local_positions: Vec<[f32; 3]> = positions
                .iter()
                .map(|p| {
                    y_min = y_min.min(p[1]);
                    y_max = y_max.max(p[1]);
                    [p[0] - chunk_center.x, p[1], p[2] - chunk_center.z]
                })
                .collect();
            let y_center = (y_min + y_max) * 0.5;
            let chunk_pos = Vec3::new(chunk_center.x, y_center, chunk_center.z);
            let local_positions: Vec<[f32; 3]> = local_positions
                .iter()
                .map(|p| [p[0], p[1] - y_center, p[2]])
                .collect();

            let mut mesh = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, default());
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, local_positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            mesh.insert_indices(bevy::mesh::Indices::U32(indices));

            // Culling sphere radius: half-diagonal of chunk + vertical extent
            let half_diag = DECO_CHUNK_SIZE * 0.5 * std::f32::consts::SQRT_2;
            let y_half = (y_max - y_min) * 0.5;
            let cull_radius = (half_diag * half_diag + y_half * y_half).sqrt();

            let mat_handle = mat_handles.get(&mat_id).cloned().unwrap_or_default();
            let entity = commands
                .spawn((
                    GameWorld,
                    DecoChunk {
                        chunk_x: cx,
                        chunk_z: cz,
                    },
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(mat_handle),
                    Transform::from_translation(chunk_pos),
                    Visibility::Hidden,
                    CullingBounds::new(cull_radius),
                    CullReason::default(),
                    bevy::camera::visibility::NoFrustumCulling,
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id();
            chunk_map.0.entry((cx, cz)).or_default().push(entity);
        }
    }

    commands.remove_resource::<PendingDecorationPlacements>();
    commands.insert_resource(chunk_map);
    info!("Built decoration chunks: {} instances merged", total_count);
}

const GRASS_CHUNK_SIZE: f32 = 32.0;

// ── Shared vertex-merge helpers for chunk instancing ──

/// Source mesh data extracted from a GLTF primitive for CPU vertex merging.
struct SourceMeshData {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl SourceMeshData {
    /// Extract vertex attributes from a loaded Mesh asset.
    fn from_mesh(mesh: &Mesh) -> Option<Self> {
        let positions: Vec<[f32; 3]> = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attr| {
                if let bevy::mesh::VertexAttributeValues::Float32x3(v) = attr {
                    Some(v.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        if positions.is_empty() {
            return None;
        }
        let normals: Vec<[f32; 3]> = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(|attr| {
                if let bevy::mesh::VertexAttributeValues::Float32x3(v) = attr {
                    Some(v.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let uvs: Vec<[f32; 2]> = mesh
            .attribute(Mesh::ATTRIBUTE_UV_0)
            .and_then(|attr| {
                if let bevy::mesh::VertexAttributeValues::Float32x2(v) = attr {
                    Some(v.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let colors: Vec<[f32; 4]> = mesh
            .attribute(Mesh::ATTRIBUTE_COLOR)
            .and_then(|attr| {
                if let bevy::mesh::VertexAttributeValues::Float32x4(v) = attr {
                    Some(v.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let indices: Vec<u32> = mesh
            .indices()
            .map(|idx| match idx {
                bevy::mesh::Indices::U16(v) => v.iter().map(|&i| i as u32).collect(),
                bevy::mesh::Indices::U32(v) => v.clone(),
            })
            .unwrap_or_default();
        Some(Self {
            positions,
            normals,
            uvs,
            colors,
            indices,
        })
    }
}

fn merge_grass_instances_into_mesh(
    src: &SourceMeshData,
    instances: &[(Vec3, f32, f32)],
    _lean_strength: f32,
) -> Mesh {
    let verts_per = src.positions.len();
    let indices_per = src.indices.len();
    let total_verts = verts_per * instances.len();
    let total_indices = indices_per * instances.len();

    let mut positions = Vec::with_capacity(total_verts);
    let mut normals = Vec::with_capacity(total_verts);
    let mut uvs = Vec::with_capacity(total_verts);
    let mut uv1s = Vec::with_capacity(total_verts);
    let mut colors = Vec::with_capacity(total_verts);
    let mut indices = Vec::with_capacity(total_indices);

    for (i, (pos, scale, y_rot)) in instances.iter().enumerate() {
        let base_idx = (i * verts_per) as u32;

        // All vertices store the blade base position — shader reconstructs geometry
        for vi in 0..verts_per {
            positions.push([pos.x, pos.y, pos.z]);
            normals.push([0.0, 1.0, 0.0]);

            if vi < src.uvs.len() {
                uvs.push(src.uvs[vi]);
            } else {
                uvs.push([0.0, 0.0]);
            }

            // Placeholder UV_1 — vertex shader writes actual height_pct via out.uv_b
            uv1s.push([0.0, 0.0]);

            // Pack per-blade data: [y_rot, scale, 0, 0]
            colors.push([*y_rot, *scale, 0.0, 0.0]);
        }

        for &idx in &src.indices {
            indices.push(base_idx + idx);
        }
    }

    let mut mesh = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv1s);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(bevy::mesh::Indices::U32(indices));
    mesh
}

fn strip_world_space_triangles_in_radius(mesh: &mut Mesh, cx: f32, cz: f32, radius_sq: f32) -> bool {
    let positions: Vec<[f32; 3]> = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(bevy::mesh::VertexAttributeValues::Float32x3(v)) => v.clone(),
        _ => return false,
    };

    let old_indices: Vec<u32> = match mesh.indices() {
        Some(bevy::mesh::Indices::U32(v)) => v.clone(),
        _ => return false,
    };

    if old_indices.len() % 3 != 0 {
        return false;
    }

    let mut new_indices = Vec::with_capacity(old_indices.len());
    for tri in old_indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
            new_indices.extend_from_slice(tri);
            continue;
        }
        let tx = (positions[i0][0] + positions[i1][0] + positions[i2][0]) / 3.0;
        let tz = (positions[i0][2] + positions[i1][2] + positions[i2][2]) / 3.0;
        let dx = tx - cx;
        let dz = tz - cz;
        if dx * dx + dz * dz > radius_sq {
            new_indices.extend_from_slice(tri);
        }
    }

    if new_indices.is_empty() {
        return true;
    }

    mesh.insert_indices(bevy::mesh::Indices::U32(new_indices));
    false
}

// ── Dense Grass ──

fn grass_biome_weight(biome: Biome, settings: &GrassDebugSettings) -> f32 {
    match biome {
        Biome::Grassland => settings.grassland_weight,
        Biome::Wetland => settings.wetland_weight,
        Biome::Forest => settings.forest_weight,
        _ => 0.0,
    }
}

fn terrain_slope_factor(height_map: &HeightMap, x: f32, z: f32) -> f32 {
    let eps = (height_map.step * 1.25).clamp(1.0, 2.5);
    let h_l = height_map.sample(x - eps, z);
    let h_r = height_map.sample(x + eps, z);
    let h_d = height_map.sample(x, z - eps);
    let h_u = height_map.sample(x, z + eps);
    let normal = Vec3::new(h_l - h_r, 2.0 * eps, h_d - h_u).normalize_or_zero();
    ((normal.y - 0.72) / 0.28).clamp(0.0, 1.0)
}

fn shoreline_grass_factor(biome_map: &BiomeMap, x: f32, z: f32) -> f32 {
    let near_water = [
        biome_map.get_biome(x + 3.5, z),
        biome_map.get_biome(x - 3.5, z),
        biome_map.get_biome(x, z + 3.5),
        biome_map.get_biome(x, z - 3.5),
    ]
    .into_iter()
    .any(|biome| matches!(biome, Biome::Water | Biome::Beach));

    if near_water { 0.3 } else { 1.0 }
}

fn sample_grass_density(
    biome_map: &BiomeMap,
    height_map: &HeightMap,
    macro_noise: &Fbm<Perlin>,
    micro_noise: &Fbm<Perlin>,
    settings: &GrassDebugSettings,
    x: f32,
    z: f32,
) -> f32 {
    let biome = biome_map.get_biome(x, z);
    let biome_weight = grass_biome_weight(biome, settings);
    if biome_weight <= 0.0 {
        return 0.0;
    }

    let macro_patch =
        (macro_noise.get([x as f64 * 0.018, z as f64 * 0.018]) as f32 * 0.5 + 0.5).clamp(0.0, 1.0);
    let micro_patch = (micro_noise.get([x as f64 * 0.075 + 83.0, z as f64 * 0.075 - 41.0]) as f32
        * 0.5
        + 0.5)
        .clamp(0.0, 1.0);
    let patchiness = (macro_patch * 0.75 + micro_patch * 0.25).clamp(0.0, 1.0);
    let slope = terrain_slope_factor(height_map, x, z);
    let shoreline = shoreline_grass_factor(biome_map, x, z);

    biome_weight * patchiness * slope * shoreline
}

/// Deterministic hash-based jitter — independent per grid cell (no spatial correlation).
fn sample_grass_offset_hashed(x: f32, z: f32, jitter: f32, seed: u64) -> (f32, f32) {
    // Quantize to grid cell for deterministic hashing
    let cx = (x * 10000.0) as i32;
    let cz = (z * 10000.0) as i32;
    let h1 = hash_cell(cx, cz, seed as u32 ^ 0x9E37_79B9) as f32 / u32::MAX as f32;
    let h2 = hash_cell(cx, cz, seed as u32 ^ 0x517C_C1B7) as f32 / u32::MAX as f32;
    ((h1 - 0.5) * 2.0 * jitter, (h2 - 0.5) * 2.0 * jitter)
}

/// Murmurhash-style bit mixing for deterministic per-cell randomness.
fn hash_cell(x: i32, z: i32, seed: u32) -> u32 {
    let mut h = seed;
    h ^= x as u32;
    h = h.wrapping_mul(0xCC9E_2D51);
    h = h.rotate_left(15);
    h ^= z as u32;
    h = h.wrapping_mul(0x1B87_3593);
    h = h.rotate_left(13);
    h = h.wrapping_mul(5).wrapping_add(0xE654_6B64);
    // Final avalanche
    h ^= h >> 16;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 16;
    h
}

/// Hash a row index for per-row jitter.
fn hash_row(row: i32, seed: u64) -> f32 {
    let h = hash_cell(row, 0, seed as u32 ^ 0x27D4_EB2D);
    (h as f32 / u32::MAX as f32) - 0.5
}

fn sample_grass_rotation(rotation_noise: &Fbm<Perlin>, x: f32, z: f32) -> f32 {
    let n = (rotation_noise.get([x as f64 * 0.17 - 11.0, z as f64 * 0.17 + 29.0]) as f32
        * 0.5
        + 0.5)
        .clamp(0.0, 1.0);
    n * std::f32::consts::TAU
}

pub(super) fn rebuild_dense_grass(
    mut commands: Commands,
    grass_assets: Res<GrassInstanceAssets>,
    biome_map: Res<BiomeMap>,
    height_map: Res<HeightMap>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
    grass_settings: Res<GrassDebugSettings>,
    mut grass_rebuild: ResMut<GrassRebuildState>,
    buildings: Query<(&Transform, &BuildingFootprint), With<Building>>,
    grass_chunks: Query<Entity, With<GrassChunk>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !grass_rebuild.dirty {
        return;
    }

    for entity in &grass_chunks {
        commands.entity(entity).despawn();
    }

    let mut chunk_map = GrassChunkMap::default();
    grass_rebuild.chunk_count = 0;
    grass_rebuild.instance_count = 0;

    if !grass_settings.enabled {
        commands.insert_resource(chunk_map);
        grass_rebuild.dirty = false;
        return;
    }

    let Some(source_mesh) = meshes.get(&grass_assets.mesh) else {
        return;
    };
    let Some(src) = SourceMeshData::from_mesh(source_mesh) else {
        warn!("Grass source mesh has no positions");
        return;
    };

    let mut rng = StdRng::seed_from_u64(map_seed.0.wrapping_add(5000));
    let macro_noise = Fbm::<Perlin>::new((map_seed.0 >> 8) as u32).set_octaves(3);
    let micro_noise = Fbm::<Perlin>::new((map_seed.0 >> 28) as u32 ^ 0x5F37_59DF).set_octaves(2);
    let rotation_noise =
        Fbm::<Perlin>::new((map_seed.0 >> 18) as u32 ^ 0x85EB_CA6B).set_octaves(2);
    let spacing = grass_settings.spacing;
    let half = height_map.half_map;
    let border = BorderSettings::from_map_size(height_map.map_size);
    let spawn_positions = config.spawn_positions(map_seed.0);
    let spawn_clear_radius = 30.0_f32;
    let row_step = spacing * grass_settings.row_step_factor;
    let jitter = grass_settings.jitter.min(spacing * 0.48);
    let building_clear_areas: Vec<(f32, f32, f32, f32)> = buildings
        .iter()
        .map(|(transform, footprint)| {
            (
                transform.translation.x,
                transform.translation.z,
                footprint.0 + 2.0,
                (footprint.0 + 2.0).powi(2),
            )
        })
        .collect();

    // Collect grass instances into chunk buckets
    let inv_chunk = 1.0 / GRASS_CHUNK_SIZE;
    let mut chunk_instances: std::collections::HashMap<(i32, i32), Vec<(Vec3, f32, f32)>> =
        std::collections::HashMap::new();

    let mut count = 0u32;
    let mut row_index = 0_i32;
    let mut z = -half + row_step * 0.5;
    while z < half - row_step * 0.5 {
        let row_jitter = hash_row(row_index, map_seed.0) * spacing * 0.15;
        let row_shift = if row_index.rem_euclid(2) == 0 {
            0.0
        } else {
            spacing * 0.5
        } + row_jitter;
        let mut x = -half + spacing * 0.5 + row_shift;
        while x < half - spacing * 0.5 {
            let base_density = sample_grass_density(
                &biome_map,
                &height_map,
                &macro_noise,
                &micro_noise,
                &grass_settings,
                x,
                z,
            );
            if base_density < grass_settings.density_threshold {
                x += spacing;
                continue;
            }

            let (off_x, off_z) = sample_grass_offset_hashed(x, z, jitter, map_seed.0);
            let jx = x + off_x;
            let jz = z + off_z;
            if is_in_mountain_border(jx, jz, half, border) {
                x += spacing;
                continue;
            }

            let density = sample_grass_density(
                &biome_map,
                &height_map,
                &macro_noise,
                &micro_noise,
                &grass_settings,
                jx,
                jz,
            );
            if density < grass_settings.density_threshold {
                x += spacing;
                continue;
            }

            let biome = biome_map.get_biome(jx, jz);
            if grass_biome_weight(biome, &grass_settings) <= 0.0 {
                x += spacing;
                continue;
            }

            let too_close = spawn_positions.iter().any(|(_, (sx, sz))| {
                let dx = jx - *sx;
                let dz = jz - *sz;
                (dx * dx + dz * dz).sqrt() < spawn_clear_radius
            });
            if too_close {
                x += spacing;
                continue;
            }

            let inside_building_clear_area = building_clear_areas
                .iter()
                .any(|(bx, bz, _clear_radius, clear_r2)| {
                    let dx = jx - *bx;
                    let dz = jz - *bz;
                    dx * dx + dz * dz < *clear_r2
                });
            if inside_building_clear_area {
                x += spacing;
                continue;
            }

            let y = terrain_translation(&height_map, jx, jz, 0.0).y;
            let scale = rng.random_range(grass_settings.scale_min..=grass_settings.scale_max)
                * (0.88 + density * 0.16);
            let y_rot = sample_grass_rotation(&rotation_noise, jx, jz);

            let cx = (jx * inv_chunk).floor() as i32;
            let cz = (jz * inv_chunk).floor() as i32;
            chunk_instances
                .entry((cx, cz))
                .or_default()
                .push((Vec3::new(jx, y, jz), scale, y_rot));
            count += 1;

            x += spacing;
        }
        row_index += 1;
        z += row_step;
    }

    // Build merged meshes per chunk using shared helper
    let chunk_count = chunk_instances.len();

    for ((cx, cz), instances) in chunk_instances {
        let mut mesh = merge_grass_instances_into_mesh(&src, &instances, grass_settings.lean_strength);
        let chunk_center_x = (cx as f32 + 0.5) * GRASS_CHUNK_SIZE;
        let chunk_center_z = (cz as f32 + 0.5) * GRASS_CHUNK_SIZE;
        let mut stripped_empty = false;
        for (bx, bz, clear_radius, clear_r2) in &building_clear_areas {
            let half_extent = GRASS_CHUNK_SIZE * 0.5 + *clear_radius;
            if (chunk_center_x - *bx).abs() > half_extent || (chunk_center_z - *bz).abs() > half_extent
            {
                continue;
            }
            if strip_world_space_triangles_in_radius(&mut mesh, *bx, *bz, *clear_r2) {
                stripped_empty = true;
                break;
            }
        }
        if stripped_empty {
            continue;
        }

        let entity = commands
            .spawn((
                GameWorld,
                GrassChunk {
                    chunk_x: cx,
                    chunk_z: cz,
                },
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(grass_assets.material.clone()),
                Transform::default(),
                Visibility::Hidden,
                NotShadowCaster,
                CullingBounds::with_offset(
                    GRASS_CHUNK_SIZE * 0.71,
                    Vec3::new(
                        (cx as f32 + 0.5) * GRASS_CHUNK_SIZE,
                        0.0,
                        (cz as f32 + 0.5) * GRASS_CHUNK_SIZE,
                    ),
                ),
            ))
            .id();
        chunk_map.0.insert((cx, cz), entity);
    }

    let rebuilt_chunk_count = chunk_map.0.len();
    commands.insert_resource(chunk_map);
    grass_rebuild.chunk_count = rebuilt_chunk_count;
    grass_rebuild.instance_count = count;
    grass_rebuild.dirty = false;
    info!(
        "Spawned {} grass instances merged into {} chunks",
        count, chunk_count
    );
}

pub fn reveal_explored_grass(
    mut commands: Commands,
    fog_map: Res<FogOfWarMap>,
    fog_settings: Res<FogTweakSettings>,
    mut grass_query: Query<(Entity, &GrassChunk, &mut Visibility), Without<GrassRevealed>>,
) {
    let step = GRASS_CHUNK_SIZE;
    for (entity, chunk, mut vis) in grass_query.iter_mut() {
        if fog_settings.reveal_all {
            commands.entity(entity).insert(GrassRevealed);
            *vis = Visibility::Inherited;
            continue;
        }
        // Check if any cell in this chunk's bounds is explored
        let x_start = chunk.chunk_x as f32 * step;
        let z_start = chunk.chunk_z as f32 * step;
        let sample_step = step / 4.0; // Check 4x4 sample points in chunk

        let mut explored = false;
        let mut sx = x_start;
        while sx < x_start + step && !explored {
            let mut sz = z_start;
            while sz < z_start + step && !explored {
                if fog_map.is_explored(sx, sz) {
                    explored = true;
                }
                sz += sample_step;
            }
            sx += sample_step;
        }

        if explored {
            commands.entity(entity).insert(GrassRevealed);
            *vis = Visibility::Inherited;
        }
    }
}

/// Reveal decoration chunks when any part of the chunk has been explored.
/// Adds `DecoRevealed` marker and sets Visibility::Inherited.
/// The culling system then manages Visibility independently for revealed chunks.
pub(super) fn reveal_explored_decorations(
    mut commands: Commands,
    fog_map: Res<FogOfWarMap>,
    fog_settings: Res<FogTweakSettings>,
    mut deco_query: Query<(Entity, &DecoChunk, &mut Visibility), Without<DecoRevealed>>,
) {
    let step = DECO_CHUNK_SIZE;
    for (entity, chunk, mut vis) in deco_query.iter_mut() {
        if fog_settings.reveal_all {
            commands.entity(entity).insert(DecoRevealed);
            *vis = Visibility::Inherited;
            continue;
        }
        let x_start = chunk.chunk_x as f32 * step;
        let z_start = chunk.chunk_z as f32 * step;
        let sample_step = step / 4.0;

        let mut explored = false;
        let mut sx = x_start;
        while sx < x_start + step && !explored {
            let mut sz = z_start;
            while sz < z_start + step && !explored {
                if fog_map.is_explored(sx, sz) {
                    explored = true;
                }
                sz += sample_step;
            }
            sx += sample_step;
        }

        if explored {
            commands.entity(entity).insert(DecoRevealed);
            *vis = Visibility::Inherited;
        }
    }
}
