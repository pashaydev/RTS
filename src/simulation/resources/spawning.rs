use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::time::Fixed;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::types::*;
use crate::world::ground::{is_in_mountain_border, BorderSettings, HeightMap};
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;

pub(super) fn biome_spawn_threshold(biome: Biome) -> f32 {
    match biome {
        Biome::Grassland => 0.22,
        Biome::Forest => 0.1,
        Biome::Desert => 0.11,
        Biome::Beach => 0.35,
        Biome::Wetland => 0.24,
        Biome::Water => 0.4,
        Biome::Mountain => 0.35,
    }
}

fn secondary_spawn_bonus(biome: Biome, tier: usize) -> f32 {
    let base = match biome {
        Biome::Desert | Biome::Wetland => 0.14,
        Biome::Forest => 0.16,
        _ => 0.2,
    };
    base + tier as f32 * 0.12
}

fn primary_resource_for(
    biome: Biome,
    wood_mesh: &Handle<Mesh>,
    ore_mesh: &Handle<Mesh>,
    _gold_mesh: &Handle<Mesh>,
    oil_mesh: &Handle<Mesh>,
    stone_mesh: &Handle<Mesh>,
    mats: &ResourceNodeMaterials,
) -> Option<(
    ResourceType,
    u32,
    Handle<Mesh>,
    Handle<StandardMaterial>,
    f32,
)> {
    match biome {
        Biome::Grassland => Some((
            ResourceType::Stone,
            175,
            stone_mesh.clone(),
            mats.stone.clone(),
            0.6,
        )),
        Biome::Forest => Some((
            ResourceType::Wood,
            300,
            wood_mesh.clone(),
            mats.wood.clone(),
            1.25,
        )),
        Biome::Desert => Some((
            ResourceType::Copper,
            760,
            ore_mesh.clone(),
            mats.copper.clone(),
            0.4,
        )),
        Biome::Beach => Some((
            ResourceType::Stone,
            125,
            stone_mesh.clone(),
            mats.stone.clone(),
            0.4,
        )),
        Biome::Wetland => Some((
            ResourceType::Iron,
            1000,
            ore_mesh.clone(),
            mats.iron.clone(),
            0.4,
        )),
        Biome::Water => Some((
            ResourceType::Oil,
            800,
            oil_mesh.clone(),
            mats.oil.clone(),
            0.6,
        )),
        Biome::Mountain => Some((
            ResourceType::Stone,
            250,
            stone_mesh.clone(),
            mats.stone.clone(),
            0.5,
        )),
    }
}

fn secondary_resource_for(
    biome: Biome,
    tier: usize,
    _wood_mesh: &Handle<Mesh>,
    ore_mesh: &Handle<Mesh>,
    _gold_mesh: &Handle<Mesh>,
    mats: &ResourceNodeMaterials,
) -> Option<(
    ResourceType,
    u32,
    Handle<Mesh>,
    Handle<StandardMaterial>,
    f32,
)> {
    match (biome, tier) {
        (Biome::Grassland, 0) => Some((
            ResourceType::Copper,
            360,
            ore_mesh.clone(),
            mats.copper.clone(),
            0.5,
        )),
        (Biome::Grassland, 1) => Some((
            ResourceType::Iron,
            550,
            ore_mesh.clone(),
            mats.iron.clone(),
            0.45,
        )),
        (Biome::Forest, 0) => Some((
            ResourceType::Copper,
            380,
            ore_mesh.clone(),
            mats.copper.clone(),
            0.4,
        )),
        (Biome::Forest, 1) => Some((
            ResourceType::Iron,
            525,
            ore_mesh.clone(),
            mats.iron.clone(),
            0.45,
        )),
        (Biome::Desert, 0) => Some((
            ResourceType::Iron,
            750,
            ore_mesh.clone(),
            mats.iron.clone(),
            0.4,
        )),
        (Biome::Desert, 1) => Some((
            ResourceType::Stone,
            140,
            ore_mesh.clone(),
            mats.stone.clone(),
            0.45,
        )),
        (Biome::Beach, 0) => Some((
            ResourceType::Copper,
            260,
            ore_mesh.clone(),
            mats.copper.clone(),
            0.45,
        )),
        (Biome::Beach, 1) => Some((
            ResourceType::Iron,
            450,
            ore_mesh.clone(),
            mats.iron.clone(),
            0.45,
        )),
        (Biome::Wetland, 0) => Some((
            ResourceType::Copper,
            500,
            ore_mesh.clone(),
            mats.copper.clone(),
            0.4,
        )),
        (Biome::Wetland, 1) => Some((
            ResourceType::Stone,
            130,
            ore_mesh.clone(),
            mats.stone.clone(),
            0.45,
        )),
        (Biome::Mountain, 0) => Some((
            ResourceType::Iron,
            850,
            ore_mesh.clone(),
            mats.iron.clone(),
            0.4,
        )),
        (Biome::Mountain, 1) => Some((
            ResourceType::Copper,
            360,
            ore_mesh.clone(),
            mats.copper.clone(),
            0.4,
        )),
        _ => None,
    }
}

/// Pick a random scene handle from a slice, returning None if empty.
fn random_model(rng: &mut impl Rng, models: &[Handle<Scene>]) -> Option<Handle<Scene>> {
    if models.is_empty() {
        None
    } else {
        Some(models[rng.random_range(0..models.len())].clone())
    }
}

/// Pick a random tree model and return (scene_handle, base_scale).
pub(super) fn random_tree(
    rng: &mut impl Rng,
    assets: &ModelAssets,
) -> Option<(Handle<Scene>, f32)> {
    if assets.trees.is_empty() {
        None
    } else {
        let idx = rng.random_range(0..assets.trees.len());
        let base_scale = assets.tree_base_scales.get(idx).copied().unwrap_or(1.0);
        Some((assets.trees[idx].clone(), base_scale))
    }
}

pub(super) fn terrain_translation(height_map: &HeightMap, x: f32, z: f32, y_offset: f32) -> Vec3 {
    Vec3::new(x, height_map.sample(x, z) + y_offset, z)
}

pub(super) fn dead_tree_wood_amount(config: &TreeGrowthConfig) -> u32 {
    (config.mature_wood_amount / 4).max(1)
}

pub(super) fn spawn_resource_nodes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    node_mats: Res<ResourceNodeMaterials>,
    biome_map: Res<BiomeMap>,
    model_assets: Res<ModelAssets>,
    height_map: Res<HeightMap>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
) {
    let wood_mesh = meshes.add(Cuboid::new(0.6, 2.5, 0.6));
    let ore_mesh = meshes.add(Cuboid::new(1.0, 0.8, 1.0));
    let gold_mesh = meshes.add(Cuboid::new(0.8, 0.8, 0.8));
    let oil_mesh = meshes.add(Cylinder::new(0.5, 1.2));
    let stone_mesh = meshes.add(Cuboid::new(0.9, 0.7, 0.9));

    let has_tree_models = !model_assets.trees.is_empty();
    let has_rock_models = !model_assets.rocks.is_empty();
    let mut rng = StdRng::seed_from_u64(map_seed.0.wrapping_add(1000));

    let resource_seed = (map_seed.0 >> 8) as u32;
    let placement_noise = Fbm::<Perlin>::new(resource_seed).set_octaves(2);
    let density_mult = config.resource_density.multiplier();
    let spacing = 12.0;
    let half = height_map.half_map;
    let border = BorderSettings::from_map_size(height_map.map_size);

    let spawn_positions = config.spawn_positions(map_seed.0);

    let mut biome_counts: std::collections::HashMap<Biome, u32> = std::collections::HashMap::new();
    let mut resource_counts: std::collections::HashMap<ResourceType, u32> =
        std::collections::HashMap::new();

    let mut x = -half + 5.0;
    while x < half - 5.0 {
        let mut z = -half + 5.0;
        while z < half - 5.0 {
            if is_in_mountain_border(x, z, half, border) {
                z += spacing;
                continue;
            }

            // Keep starting areas clear (all faction spawn positions)
            let mut too_close_to_spawn = false;
            for &(_, (sx, sz)) in &spawn_positions {
                let dx = x - sx;
                let dz = z - sz;
                if (dx * dx + dz * dz).sqrt() < 22.0 {
                    too_close_to_spawn = true;
                    break;
                }
            }
            if too_close_to_spawn {
                z += spacing;
                continue;
            }

            let biome = biome_map.get_biome(x, z);
            *biome_counts.entry(biome).or_insert(0) += 1;
            let noise_val = placement_noise.get([x as f64 * 0.1, z as f64 * 0.1]) as f32;
            let threshold = biome_spawn_threshold(biome) / density_mult;

            if noise_val > threshold {
                // For Water, only spawn at edges
                if biome == Biome::Water {
                    let is_edge = biome_map.get_biome(x + spacing, z) != Biome::Water
                        || biome_map.get_biome(x - spacing, z) != Biome::Water
                        || biome_map.get_biome(x, z + spacing) != Biome::Water
                        || biome_map.get_biome(x, z - spacing) != Biome::Water;
                    if !is_edge {
                        z += spacing;
                        continue;
                    }
                }

                if let Some((rt, amount, mesh, mat, half_h)) = primary_resource_for(
                    biome,
                    &wood_mesh,
                    &ore_mesh,
                    &gold_mesh,
                    &oil_mesh,
                    &stone_mesh,
                    &node_mats,
                ) {
                    *resource_counts.entry(rt).or_insert(0) += 1;
                    let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);
                    let scale_factor = rng.random_range(0.8_f32..1.2);

                    // Wood nodes → tree models
                    if rt == ResourceType::Wood && has_tree_models {
                        let (scene_handle, base_scale) =
                            random_tree(&mut rng, &model_assets).unwrap();
                        let tree_scale = scale_factor * base_scale;
                        commands.spawn((
                            GameWorld,
                            ResourceNode {
                                resource_type: rt,
                                amount_remaining: amount,
                            },
                            TerrainHeightOffset(0.0),
                            MatureTree,
                            FogHideable::Object,
                            PickRadius(3.0 * scale_factor),
                            SceneRoot(scene_handle),
                            NotShadowCaster,
                            Transform::from_translation(terrain_translation(
                                &height_map,
                                x,
                                z,
                                0.0,
                            ))
                            .with_rotation(Quat::from_rotation_y(y_rotation))
                            .with_scale(Vec3::splat(tree_scale)),
                        ));
                    }
                    // Ore/Stone nodes → rock models
                    else if matches!(
                        rt,
                        ResourceType::Copper
                            | ResourceType::Iron
                            | ResourceType::Gold
                            | ResourceType::Stone
                    ) && has_rock_models
                    {
                        let scene_handle = random_model(&mut rng, &model_assets.rocks).unwrap();
                        commands.spawn((
                            GameWorld,
                            ResourceNode {
                                resource_type: rt,
                                amount_remaining: amount,
                            },
                            TerrainHeightOffset(0.0),
                            FogHideable::Object,
                            PickRadius(1.8 * scale_factor),
                            SceneRoot(scene_handle),
                            NotShadowCaster,
                            Transform::from_translation(terrain_translation(
                                &height_map,
                                x,
                                z,
                                0.0,
                            ))
                            .with_rotation(Quat::from_rotation_y(y_rotation))
                            .with_scale(Vec3::splat(scale_factor)),
                        ));
                    }
                    // Oil + fallbacks → primitive mesh
                    else {
                        let y = terrain_translation(&height_map, x, z, half_h).y;
                        commands.spawn((
                            GameWorld,
                            ResourceNode {
                                resource_type: rt,
                                amount_remaining: amount,
                            },
                            TerrainHeightOffset(half_h),
                            FogHideable::Object,
                            PickRadius(half_h * 1.5),
                            Mesh3d(mesh),
                            MeshMaterial3d(mat),
                            NotShadowCaster,
                            Transform::from_translation(Vec3::new(x, y, z)),
                        ));
                    }
                }

                // Secondary resources: two lower-probability biome-specific rolls.
                for tier in 0..2 {
                    let secondary_noise = placement_noise.get([
                        x as f64 * (0.13 + tier as f64 * 0.017) + 50.0 + tier as f64 * 37.0,
                        z as f64 * (0.13 + tier as f64 * 0.019) + 50.0 + tier as f64 * 41.0,
                    ]) as f32;
                    if secondary_noise
                        <= threshold + secondary_spawn_bonus(biome, tier) / density_mult
                    {
                        continue;
                    }

                    if let Some((rt, amount, mesh, mat, half_h)) = secondary_resource_for(
                        biome, tier, &wood_mesh, &ore_mesh, &gold_mesh, &node_mats,
                    ) {
                        *resource_counts.entry(rt).or_insert(0) += 1;
                        let offset_x = x + 3.0 + tier as f32 * 2.5;
                        let offset_z = z + 2.0 - tier as f32 * 2.0;
                        let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);
                        let scale_factor = rng.random_range(0.8_f32..1.2);

                        if rt == ResourceType::Wood && has_tree_models {
                            let (scene_handle, base_scale) =
                                random_tree(&mut rng, &model_assets).unwrap();
                            let tree_scale = scale_factor * base_scale;
                            commands.spawn((
                                GameWorld,
                                ResourceNode {
                                    resource_type: rt,
                                    amount_remaining: amount,
                                },
                                TerrainHeightOffset(0.0),
                                MatureTree,
                                FogHideable::Object,
                                PickRadius(3.0 * scale_factor),
                                SceneRoot(scene_handle),
                                NotShadowCaster,
                                Transform::from_translation(terrain_translation(
                                    &height_map,
                                    offset_x,
                                    offset_z,
                                    0.0,
                                ))
                                .with_rotation(Quat::from_rotation_y(y_rotation))
                                .with_scale(Vec3::splat(tree_scale)),
                            ));
                        } else if has_rock_models {
                            let scene_handle = random_model(&mut rng, &model_assets.rocks).unwrap();
                            commands.spawn((
                                GameWorld,
                                ResourceNode {
                                    resource_type: rt,
                                    amount_remaining: amount,
                                },
                                TerrainHeightOffset(0.0),
                                FogHideable::Object,
                                PickRadius(1.8 * scale_factor),
                                SceneRoot(scene_handle),
                                NotShadowCaster,
                                Transform::from_translation(terrain_translation(
                                    &height_map,
                                    offset_x,
                                    offset_z,
                                    0.0,
                                ))
                                .with_rotation(Quat::from_rotation_y(y_rotation))
                                .with_scale(Vec3::splat(scale_factor)),
                            ));
                        } else {
                            let y = terrain_translation(&height_map, offset_x, offset_z, half_h).y;
                            commands.spawn((
                                GameWorld,
                                ResourceNode {
                                    resource_type: rt,
                                    amount_remaining: amount,
                                },
                                TerrainHeightOffset(half_h),
                                FogHideable::Object,
                                PickRadius(half_h * 1.5),
                                Mesh3d(mesh),
                                MeshMaterial3d(mat),
                                NotShadowCaster,
                                Transform::from_translation(Vec3::new(offset_x, y, offset_z)),
                            ));
                        }
                    }
                }
            }

            z += spacing;
        }
        x += spacing;
    }

    // ── Minimum resource node enforcement ──
    // Guarantee a minimum number of each resource type regardless of noise rolls.
    // This mirrors ensure_all_biomes() but for resource placement.
    let min_nodes: u32 = ((half * 2.0 / 500.0).powi(2) * 5.0).max(3.0) as u32;

    let deficit_types: Vec<(ResourceType, u32, Handle<StandardMaterial>, f32)> = [
        (ResourceType::Iron, 1000, node_mats.iron.clone(), 0.4),
        (ResourceType::Copper, 500, node_mats.copper.clone(), 0.5),
        (ResourceType::Stone, 175, node_mats.stone.clone(), 0.6),
        (ResourceType::Oil, 800, node_mats.oil.clone(), 0.6),
    ]
    .into_iter()
    .filter(|(rt, _, _, _)| {
        let count = *resource_counts.get(rt).unwrap_or(&0);
        count < min_nodes
    })
    .collect();

    if !deficit_types.is_empty() {
        // Collect candidate positions: grid cells that are valid spawn locations
        let mut candidates: Vec<(f32, f32, Biome)> = Vec::new();
        let scan_spacing = spacing * 2.0;
        let mut sx = -half + 5.0;
        while sx < half - 5.0 {
            let mut sz = -half + 5.0;
            while sz < half - 5.0 {
                if is_in_mountain_border(sx, sz, half, border) {
                    sz += scan_spacing;
                    continue;
                }
                let mut too_close = false;
                for &(_, (px, pz)) in &spawn_positions {
                    if ((sx - px).powi(2) + (sz - pz).powi(2)).sqrt() < 22.0 {
                        too_close = true;
                        break;
                    }
                }
                if !too_close {
                    let biome = biome_map.get_biome(sx, sz);
                    if !matches!(biome, Biome::Water | Biome::Mountain | Biome::Beach) {
                        candidates.push((sx, sz, biome));
                    }
                }
                sz += scan_spacing;
            }
            sx += scan_spacing;
        }

        for (rt, amount, mat, _half_h) in &deficit_types {
            let current = *resource_counts.get(rt).unwrap_or(&0);
            let needed = min_nodes - current;

            // Prefer biomes that naturally produce this resource type
            let preferred_biome = match rt {
                ResourceType::Iron => Some(Biome::Wetland),
                ResourceType::Copper => Some(Biome::Desert),
                ResourceType::Stone => Some(Biome::Grassland),
                ResourceType::Oil => None, // Water-edge only, just pick any open cell
                _ => None,
            };

            // Sort candidates: preferred biome first, then shuffle within each group
            let mut sorted_candidates = candidates.clone();
            sorted_candidates
                .sort_by_key(|&(_, _, b)| if Some(b) == preferred_biome { 0 } else { 1 });

            let mut spawned = 0u32;
            for &(cx, cz, _) in &sorted_candidates {
                if spawned >= needed {
                    break;
                }
                // Avoid placing too close to an already-placed enforcement node
                let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);
                let scale_factor = rng.random_range(0.8_f32..1.2);

                if has_rock_models && *rt != ResourceType::Oil {
                    let scene_handle = random_model(&mut rng, &model_assets.rocks).unwrap();
                    commands.spawn((
                        GameWorld,
                        ResourceNode {
                            resource_type: *rt,
                            amount_remaining: *amount,
                        },
                        TerrainHeightOffset(0.0),
                        FogHideable::Object,
                        PickRadius(1.8 * scale_factor),
                        SceneRoot(scene_handle),
                        NotShadowCaster,
                        Transform::from_translation(terrain_translation(&height_map, cx, cz, 0.0))
                            .with_rotation(Quat::from_rotation_y(y_rotation))
                            .with_scale(Vec3::splat(scale_factor)),
                    ));
                } else {
                    // Oil or fallback: primitive mesh
                    let mesh = if *rt == ResourceType::Oil {
                        oil_mesh.clone()
                    } else {
                        ore_mesh.clone()
                    };
                    let half_h = if *rt == ResourceType::Oil { 0.6 } else { 0.4 };
                    let y = terrain_translation(&height_map, cx, cz, half_h).y;
                    commands.spawn((
                        GameWorld,
                        ResourceNode {
                            resource_type: *rt,
                            amount_remaining: *amount,
                        },
                        TerrainHeightOffset(half_h),
                        FogHideable::Object,
                        PickRadius(half_h * 1.5),
                        Mesh3d(mesh),
                        MeshMaterial3d(mat.clone()),
                        NotShadowCaster,
                        Transform::from_translation(Vec3::new(cx, y, cz)),
                    ));
                }
                spawned += 1;
            }

            if spawned > 0 {
                *resource_counts.entry(*rt).or_insert(0) += spawned;
                warn!(
                    "Enforced minimum: spawned {} extra {} nodes (had {}, need {})",
                    spawned,
                    rt.display_name(),
                    current,
                    min_nodes
                );
            }
        }
    }

    info!("Biome cell counts: {:?}", biome_counts);
    info!("Resource node counts: {:?}", resource_counts);
}

pub(super) fn deplete_resource_nodes(
    mut commands: Commands,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
    time: Res<Time<Fixed>>,
    nodes: Query<(
        Entity,
        &ResourceNode,
        &Transform,
        Option<&YardResourceNode>,
        Option<&DepletionAnimation>,
    )>,
) {
    for (entity, node, transform, yard_tag, depletion) in &nodes {
        if node.amount_remaining == 0 {
            // Yard trees regrow — don't despawn them
            if yard_tag.is_some() {
                continue;
            }
            // Already animating
            if depletion.is_some() {
                continue;
            }
            event_log.push(
                time.elapsed_secs(),
                format!("{} node depleted", node.resource_type.display_name()),
                crate::ui::event_log_widget::EventCategory::Resource,
                Some(transform.translation),
                None,
            );

            let kind = match node.resource_type {
                ResourceType::Wood => {
                    let angle = rand::random::<f32>() * std::f32::consts::TAU;
                    DepletionKind::TreeFall {
                        fall_direction: Vec3::new(angle.cos(), 0.0, angle.sin()),
                    }
                }
                ResourceType::Oil => DepletionKind::OilSpray,
                _ => DepletionKind::MinePuff,
            };

            let duration = match kind {
                DepletionKind::TreeFall { .. } => 1.0,
                DepletionKind::MinePuff => 0.6,
                DepletionKind::OilSpray => 0.8,
            };

            commands
                .entity(entity)
                .remove::<ResourceNode>()
                .insert(DepletionAnimation {
                    timer: Timer::from_seconds(duration, TimerMode::Once),
                    kind,
                    initial_scale: transform.scale,
                });
        }
    }
}

/// Processing buildings periodically spawn new resource nodes nearby.
pub(super) fn resource_respawn_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut game_rng: ResMut<GameRng>,
    height_map: Res<HeightMap>,
    model_assets: Res<ModelAssets>,
    node_mats: Res<ResourceNodeMaterials>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut buildings: Query<(&Transform, &mut ResourceRespawnConfig, &BuildingState), With<Building>>,
    existing_nodes: Query<(&Transform, &ResourceNode), Without<Building>>,
    growing_resources: Query<(&Transform, &GrowingResource), Without<Building>>,
    building_positions: Query<&Transform, (With<Building>, Without<ResourceNode>)>,
) {
    for (building_tf, mut config, state) in &mut buildings {
        if *state != BuildingState::Complete {
            continue;
        }

        config.respawn_timer.tick(time.delta());
        if !config.respawn_timer.just_finished() {
            continue;
        }

        // For each resource type this building manages
        // Only Wood regrows; mineral nodes (Copper, Iron, Gold, Oil) are finite
        for rt in config.resource_types.clone() {
            if rt != ResourceType::Wood {
                continue;
            }
            // Count existing nodes + growing resources of this type within radius
            let mut count = 0u8;
            for (node_tf, node) in &existing_nodes {
                if node.resource_type == rt {
                    let dist = building_tf.translation.distance(node_tf.translation);
                    if dist <= config.respawn_radius + 5.0 {
                        count += 1;
                    }
                }
            }
            for (grow_tf, grow) in &growing_resources {
                if grow.resource_type == rt {
                    let dist = building_tf.translation.distance(grow_tf.translation);
                    if dist <= config.respawn_radius + 5.0 {
                        count += 1;
                    }
                }
            }

            if count >= config.max_nodes {
                continue;
            }

            // Find spawn position: random point within radius, avoiding building overlap
            let rng = &mut game_rng.rng;
            let mut attempts = 0;
            loop {
                if attempts >= 10 {
                    break;
                }
                attempts += 1;

                let angle = rng.random_range(0.0..std::f32::consts::TAU);
                let dist = rng.random_range(5.0..config.respawn_radius);
                let x = building_tf.translation.x + angle.cos() * dist;
                let z = building_tf.translation.z + angle.sin() * dist;

                // Check clearance from buildings
                let mut too_close = false;
                for b_tf in &building_positions {
                    if b_tf.translation.distance(Vec3::new(x, 0.0, z)) < 5.0 {
                        too_close = true;
                        break;
                    }
                }
                if too_close {
                    continue;
                }

                let terrain_pos = terrain_translation(&height_map, x, z, 0.0);

                if rt == ResourceType::Wood {
                    // Reuse sapling system — spawn a Sapling
                    if let Some((scene_handle, base_scale)) = random_tree(rng, &model_assets) {
                        let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);
                        let target_scale = rng.random_range(0.8_f32..1.2) * base_scale;

                        commands.spawn((
                            Sapling {
                                timer: Timer::from_seconds(20.0, TimerMode::Once),
                                target_scale,
                            },
                            FogHideable::Object,
                            SceneRoot(scene_handle),
                            Transform::from_translation(terrain_pos)
                                .with_rotation(Quat::from_rotation_y(y_rotation))
                                .with_scale(Vec3::splat(0.15 * base_scale)),
                        ));
                    }
                } else {
                    // Ore/oil: spawn GrowingResource
                    let grow_time = match rt {
                        ResourceType::Oil => 15.0,
                        _ => 10.0,
                    };
                    let target_scale = rng.random_range(0.8_f32..1.2);

                    // Use rock models for ore, cylinder mesh for oil
                    if matches!(
                        rt,
                        ResourceType::Copper | ResourceType::Iron | ResourceType::Gold
                    ) && !model_assets.rocks.is_empty()
                    {
                        let scene_handle = model_assets.rocks
                            [rng.random_range(0..model_assets.rocks.len())]
                        .clone();
                        let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);

                        commands.spawn((
                            GrowingResource {
                                timer: Timer::from_seconds(grow_time, TimerMode::Once),
                                target_scale,
                                resource_type: rt,
                                amount: config.amount_per_node,
                            },
                            TerrainHeightOffset(0.0),
                            FogHideable::Object,
                            SceneRoot(scene_handle),
                            Transform::from_translation(terrain_pos)
                                .with_rotation(Quat::from_rotation_y(y_rotation))
                                .with_scale(Vec3::splat(0.1)),
                        ));
                    } else {
                        // Oil: cylinder mesh
                        let mesh = meshes.add(Cylinder::new(0.5, 1.2));
                        let mat = node_mats.oil.clone();
                        commands.spawn((
                            GrowingResource {
                                timer: Timer::from_seconds(grow_time, TimerMode::Once),
                                target_scale: target_scale * 0.6,
                                resource_type: rt,
                                amount: config.amount_per_node,
                            },
                            TerrainHeightOffset(0.6),
                            FogHideable::Object,
                            Mesh3d(mesh),
                            MeshMaterial3d(mat),
                            Transform::from_translation(terrain_translation(
                                &height_map,
                                x,
                                z,
                                0.6,
                            ))
                            .with_scale(Vec3::splat(0.1)),
                        ));
                    }
                }
                break;
            }
        }
    }
}

/// Animates GrowingResource entities and promotes them to ResourceNode when complete.
pub(super) fn grow_resource_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    height_map: Res<HeightMap>,
    mut growing: Query<(Entity, &mut GrowingResource, &mut Transform), Without<FrustumCulled>>,
) {
    for (entity, mut res, mut tf) in &mut growing {
        res.timer.tick(time.delta());
        let progress = res.timer.fraction();

        // Scale from 0.1 to target_scale, with slight upward translation
        let scale = 0.1 + progress * (res.target_scale - 0.1);
        tf.scale = Vec3::splat(scale);
        let y_offset = match res.resource_type {
            ResourceType::Oil => 0.6,
            _ => 0.0,
        };
        tf.translation.y =
            terrain_translation(&height_map, tf.translation.x, tf.translation.z, y_offset).y;

        if res.timer.is_finished() {
            commands.entity(entity).remove::<GrowingResource>();
            commands.entity(entity).insert((
                ResourceNode {
                    resource_type: res.resource_type,
                    amount_remaining: res.amount,
                },
                PickRadius(1.8 * res.target_scale),
            ));
        }
    }
}
