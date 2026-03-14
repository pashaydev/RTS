use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::blueprints::EntityKind;
use crate::components::*;
use crate::ground::HeightMap;
use crate::theme;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;

pub struct ResourcesPlugin;

impl Plugin for ResourcesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TreeGrowthConfig>()
            .init_resource::<CarriedResourceTotals>()
            .init_resource::<PendingCarriedDrains>()
            .add_systems(
                Startup,
                (create_resource_node_materials, create_carry_visual_assets),
            )
            .add_systems(
                OnEnter(AppState::InGame),
                (
                    spawn_resource_nodes,
                    spawn_decorations,
                    spawn_explosive_props,
                )
                    .after(crate::ground::spawn_ground),
            )
            .add_systems(
                Update,
                (
                    compute_carried_totals,
                    worker_ai_system,
                    resource_processor_system,
                    deplete_resource_nodes,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (drain_carried_from_workers, update_carry_visuals)
                    .chain()
                    .after(deplete_resource_nodes)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    spawn_saplings_system,
                    grow_saplings_system,
                    grow_trees_system,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    processor_worker_visual_system,
                    resource_respawn_system,
                    grow_resource_system,
                    update_resource_popups,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                spawn_dense_grass
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<GrassInstanceAssets>),
            )
            .add_systems(
                Update,
                reveal_explored_grass
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<FogOfWarMap>),
            );
    }
}

fn create_resource_node_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(ResourceNodeMaterials {
        wood: materials.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.45, 0.1),
            ..default()
        }),
        copper: materials.add(StandardMaterial {
            base_color: Color::srgb(0.72, 0.45, 0.2),
            ..default()
        }),
        iron: materials.add(StandardMaterial {
            base_color: Color::srgb(0.55, 0.55, 0.58),
            ..default()
        }),
        gold: materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.8, 0.2),
            emissive: LinearRgba::new(0.4, 0.35, 0.05, 1.0),
            ..default()
        }),
        oil: materials.add(StandardMaterial {
            base_color: Color::srgb(0.08, 0.08, 0.1),
            ..default()
        }),
    });
}

fn biome_spawn_threshold(biome: Biome) -> f32 {
    match biome {
        Biome::Forest => 0.1,
        Biome::Desert => 0.14,
        Biome::Mud => 0.28,
        Biome::Water => 0.4,
        Biome::Mountain => 0.35,
    }
}

fn primary_resource_for(
    biome: Biome,
    wood_mesh: &Handle<Mesh>,
    ore_mesh: &Handle<Mesh>,
    gold_mesh: &Handle<Mesh>,
    oil_mesh: &Handle<Mesh>,
    mats: &ResourceNodeMaterials,
) -> Option<(
    ResourceType,
    u32,
    Handle<Mesh>,
    Handle<StandardMaterial>,
    f32,
)> {
    match biome {
        Biome::Forest => Some((
            ResourceType::Wood,
            300,
            wood_mesh.clone(),
            mats.wood.clone(),
            1.25,
        )),
        Biome::Desert => Some((
            ResourceType::Copper,
            650,
            ore_mesh.clone(),
            mats.copper.clone(),
            0.4,
        )),
        Biome::Mud => Some((
            ResourceType::Iron,
            500,
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
            ResourceType::Gold,
            600,
            gold_mesh.clone(),
            mats.gold.clone(),
            0.4,
        )),
    }
}

fn secondary_resource_for(
    biome: Biome,
    ore_mesh: &Handle<Mesh>,
    gold_mesh: &Handle<Mesh>,
    mats: &ResourceNodeMaterials,
) -> Option<(
    ResourceType,
    u32,
    Handle<Mesh>,
    Handle<StandardMaterial>,
    f32,
)> {
    match biome {
        Biome::Desert => Some((
            ResourceType::Gold,
            600,
            gold_mesh.clone(),
            mats.gold.clone(),
            0.4,
        )),
        Biome::Mud => Some((
            ResourceType::Copper,
            420,
            ore_mesh.clone(),
            mats.copper.clone(),
            0.4,
        )),
        Biome::Mountain => Some((
            ResourceType::Iron,
            400,
            ore_mesh.clone(),
            mats.iron.clone(),
            0.4,
        )),
        Biome::Forest => Some((
            ResourceType::Copper,
            320,
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

fn spawn_resource_nodes(
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

    let has_tree_models = !model_assets.trees.is_empty();
    let has_rock_models = !model_assets.rocks.is_empty();
    let mut rng = StdRng::seed_from_u64(map_seed.0.wrapping_add(1000));

    let resource_seed = (map_seed.0 >> 8) as u32;
    let placement_noise = Fbm::<Perlin>::new(resource_seed).set_octaves(2);
    let density_mult = config.resource_density.multiplier();
    let spacing = 12.0;
    let half = height_map.half_map;

    let spawn_positions = config.spawn_positions(map_seed.0);

    let mut x = -half + 5.0;
    while x < half - 5.0 {
        let mut z = -half + 5.0;
        while z < half - 5.0 {
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
                    biome, &wood_mesh, &ore_mesh, &gold_mesh, &oil_mesh, &node_mats,
                ) {
                    let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);
                    let scale_factor = rng.random_range(0.8_f32..1.2);

                    // Wood nodes → tree models
                    if rt == ResourceType::Wood && has_tree_models {
                        let scene_handle = random_model(&mut rng, &model_assets.trees).unwrap();
                        commands.spawn((
                            ResourceNode {
                                resource_type: rt,
                                amount_remaining: amount,
                            },
                            MatureTree,
                            FogHideable::Object,
                            PickRadius(3.0 * scale_factor),
                            SceneRoot(scene_handle),
                            Transform::from_translation(Vec3::new(x, height_map.sample(x, z), z))
                                .with_rotation(Quat::from_rotation_y(y_rotation))
                                .with_scale(Vec3::splat(scale_factor)),
                        ));
                    }
                    // Ore nodes (Copper/Iron/Gold) → rock models
                    else if matches!(
                        rt,
                        ResourceType::Copper | ResourceType::Iron | ResourceType::Gold
                    ) && has_rock_models
                    {
                        let scene_handle = random_model(&mut rng, &model_assets.rocks).unwrap();
                        commands.spawn((
                            ResourceNode {
                                resource_type: rt,
                                amount_remaining: amount,
                            },
                            FogHideable::Object,
                            PickRadius(1.8 * scale_factor),
                            SceneRoot(scene_handle),
                            Transform::from_translation(Vec3::new(x, height_map.sample(x, z), z))
                                .with_rotation(Quat::from_rotation_y(y_rotation))
                                .with_scale(Vec3::splat(scale_factor)),
                        ));
                    }
                    // Oil + fallbacks → primitive mesh
                    else {
                        let y = height_map.sample(x, z) + half_h;
                        commands.spawn((
                            ResourceNode {
                                resource_type: rt,
                                amount_remaining: amount,
                            },
                            FogHideable::Object,
                            PickRadius(half_h * 1.5),
                            Mesh3d(mesh),
                            MeshMaterial3d(mat),
                            Transform::from_translation(Vec3::new(x, y, z)),
                        ));
                    }
                }

                // Secondary resource (lower probability)
                let secondary_noise =
                    placement_noise.get([x as f64 * 0.13 + 50.0, z as f64 * 0.13 + 50.0]) as f32;
                if secondary_noise > threshold + 0.2 / density_mult {
                    if let Some((rt, amount, mesh, mat, half_h)) =
                        secondary_resource_for(biome, &ore_mesh, &gold_mesh, &node_mats)
                    {
                        let offset_x = x + 3.0;
                        let offset_z = z + 2.0;

                        if has_rock_models {
                            let scene_handle = random_model(&mut rng, &model_assets.rocks).unwrap();
                            let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);
                            let scale_factor = rng.random_range(0.8_f32..1.2);
                            commands.spawn((
                                ResourceNode {
                                    resource_type: rt,
                                    amount_remaining: amount,
                                },
                                FogHideable::Object,
                                PickRadius(1.8 * scale_factor),
                                SceneRoot(scene_handle),
                                Transform::from_translation(Vec3::new(
                                    offset_x,
                                    height_map.sample(offset_x, offset_z),
                                    offset_z,
                                ))
                                .with_rotation(Quat::from_rotation_y(y_rotation))
                                .with_scale(Vec3::splat(scale_factor)),
                            ));
                        } else {
                            let y = height_map.sample(offset_x, offset_z) + half_h;
                            commands.spawn((
                                ResourceNode {
                                    resource_type: rt,
                                    amount_remaining: amount,
                                },
                                FogHideable::Object,
                                PickRadius(half_h * 1.5),
                                Mesh3d(mesh),
                                MeshMaterial3d(mat),
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
}

// ── Decoration spawning ──

/// Decorations per biome: (grass_weight, bush_weight, rock_weight, dead_tree_weight)
/// Weights control relative probability; 0 means none.
fn biome_decoration_weights(biome: Biome) -> (f32, f32, f32, f32) {
    match biome {
        // Forest grass weight 0 — dense grass handled separately via GPU instancing
        Biome::Forest => (0.0, 0.35, 0.1, 0.0),
        Biome::Desert => (0.0, 0.0, 0.5, 0.3),
        Biome::Mud => (0.35, 0.35, 0.0, 0.0),
        Biome::Mountain => (0.0, 0.0, 0.6, 0.15),
        Biome::Water => (0.0, 0.0, 0.0, 0.0),
    }
}

enum DecoKind {
    Grass,
    Bush,
    Rock,
    DeadTree,
}

fn spawn_explosive_props(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    biome_map: Res<BiomeMap>,
    height_map: Res<HeightMap>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
) {
    let barrel_mesh = meshes.add(Cylinder::new(0.45, 1.1));
    let barrel_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.62, 0.18, 0.08),
        emissive: LinearRgba::new(0.12, 0.03, 0.01, 1.0),
        perceptual_roughness: 0.85,
        ..default()
    });

    let mut rng = StdRng::seed_from_u64(map_seed.0.wrapping_add(3000));
    let half = height_map.half_map - 12.0;
    let spawn_positions = config.spawn_positions(map_seed.0);
    let barrel_count = ((config.map_size.world_size() / 500.0).powi(2) * 22.0).round() as u32;

    for _ in 0..barrel_count.max(8) {
        let mut placed = false;
        for _ in 0..20 {
            let x = rng.random_range(-half..half);
            let z = rng.random_range(-half..half);
            let biome = biome_map.get_biome(x, z);
            if matches!(biome, Biome::Water | Biome::Mountain) {
                continue;
            }

            let too_close_to_spawn = spawn_positions.iter().any(|(_, (sx, sz))| {
                let dx = x - *sx;
                let dz = z - *sz;
                (dx * dx + dz * dz).sqrt() < 32.0
            });
            if too_close_to_spawn {
                continue;
            }

            let y = height_map.sample(x, z) + 0.55;
            commands.spawn((
                ExplosiveProp {
                    damage: 45.0,
                    radius: 4.5,
                },
                Health {
                    current: 35.0,
                    max: 35.0,
                },
                FogHideable::Object,
                PickRadius(1.0),
                Mesh3d(barrel_mesh.clone()),
                MeshMaterial3d(barrel_material.clone()),
                Transform::from_translation(Vec3::new(x, y, z)).with_rotation(
                    Quat::from_rotation_y(rng.random_range(0.0..std::f32::consts::TAU)),
                ),
            ));
            placed = true;
            break;
        }

        if !placed {
            continue;
        }
    }
}

fn spawn_decorations(
    mut commands: Commands,
    biome_map: Res<BiomeMap>,
    model_assets: Res<ModelAssets>,
    height_map: Res<HeightMap>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
) {
    let mut rng = StdRng::seed_from_u64(map_seed.0.wrapping_add(2000));
    let deco_seed = (map_seed.0 >> 12) as u32;
    let deco_noise = Fbm::<Perlin>::new(deco_seed).set_octaves(2);
    let spacing = 8.0;
    let half = height_map.half_map;
    let max_decorations = ((height_map.map_size / 500.0).powi(2) * 700.0) as u32;
    let mut count = 0u32;

    let mut x = -half + 4.0;
    while x < half - 4.0 {
        let mut z = -half + 4.0;
        while z < half - 4.0 {
            if count >= max_decorations {
                return;
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
            // Only place decorations where noise is positive (roughly half)
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
                DecoKind::Bush => (&model_assets.bushes, 0.7, 1.1),
                DecoKind::Rock => (&model_assets.rocks, 0.8, 1.5),
                DecoKind::DeadTree => (&model_assets.dead_trees, 0.7, 1.1),
            };

            if let Some(scene_handle) = random_model(&mut rng, models) {
                // Small random offset so decorations don't align to a grid
                let ox = x + rng.random_range(-2.0_f32..2.0);
                let oz = z + rng.random_range(-2.0_f32..2.0);
                let y = height_map.sample(ox, oz);
                let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);
                let scale = rng.random_range(scale_min..scale_max);

                commands.spawn((
                    Decoration,
                    FogHideable::Object,
                    SceneRoot(scene_handle),
                    Transform::from_translation(Vec3::new(ox, y, oz))
                        .with_rotation(Quat::from_rotation_y(y_rotation))
                        .with_scale(Vec3::splat(scale)),
                ));
                count += 1;
            }

            z += spacing;
        }
        x += spacing;
    }
}

fn spawn_dense_grass(
    mut commands: Commands,
    grass_assets: Res<GrassInstanceAssets>,
    biome_map: Res<BiomeMap>,
    height_map: Res<HeightMap>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
    mut has_run: Local<bool>,
) {
    if *has_run {
        return;
    }
    *has_run = true;

    let mut rng = StdRng::seed_from_u64(map_seed.0.wrapping_add(5000));
    let grass_noise = Fbm::<Perlin>::new((map_seed.0 >> 8) as u32).set_octaves(3);
    let spacing = 3.0;
    let half = height_map.half_map;
    let spawn_positions = config.spawn_positions(map_seed.0);
    let spawn_clear_radius = 30.0_f32;

    let mut count = 0u32;
    let mut x = -half + 1.5;
    while x < half - 1.5 {
        let mut z = -half + 1.5;
        while z < half - 1.5 {
            let biome = biome_map.get_biome(x, z);

            // Only Forest biome gets dense grass
            if biome != Biome::Forest {
                z += spacing;
                continue;
            }
            let threshold = -0.1;

            // Noise-based clustering
            let noise_val = grass_noise.get([x as f64 * 0.08, z as f64 * 0.08]) as f32;
            if noise_val < threshold {
                z += spacing;
                continue;
            }

            // Jitter position to break grid
            let jx = x + rng.random_range(-1.5_f32..1.5);
            let jz = z + rng.random_range(-1.5_f32..1.5);

            // Re-check biome at jittered position (jitter can cross biome boundaries)
            let jittered_biome = biome_map.get_biome(jx, jz);
            if jittered_biome != Biome::Forest {
                z += spacing;
                continue;
            }

            // Skip spawn areas
            let too_close = spawn_positions.iter().any(|(_, (sx, sz))| {
                let dx = jx - *sx;
                let dz = jz - *sz;
                (dx * dx + dz * dz).sqrt() < spawn_clear_radius
            });
            if too_close {
                z += spacing;
                continue;
            }

            let y = height_map.sample(jx, jz);
            let scale = rng.random_range(0.5_f32..1.2);
            let y_rot = rng.random_range(0.0..std::f32::consts::TAU);

            commands.spawn((
                DenseGrass,
                Mesh3d(grass_assets.mesh.clone()),
                MeshMaterial3d(grass_assets.material.clone()),
                Transform::from_translation(Vec3::new(jx, y, jz))
                    .with_rotation(Quat::from_rotation_y(y_rot))
                    .with_scale(Vec3::splat(scale)),
                Visibility::Hidden,
                NotShadowCaster,
                NotShadowReceiver,
            ));
            count += 1;

            z += spacing;
        }
        x += spacing;
    }

    info!("Spawned {} dense grass instances", count);
}

pub fn reveal_explored_grass(
    fog_map: Res<FogOfWarMap>,
    mut grass_query: Query<(&Transform, &mut Visibility), With<DenseGrass>>,
) {
    for (transform, mut vis) in grass_query.iter_mut() {
        if *vis != Visibility::Hidden {
            continue;
        }
        let pos = transform.translation;
        if fog_map.is_explored(pos.x, pos.z) {
            *vis = Visibility::Inherited;
        }
    }
}

fn create_carry_visual_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use std::collections::HashMap;

    let cube_mesh = meshes.add(Cuboid::new(0.3, 0.3, 0.3));
    let sphere_mesh = meshes.add(Sphere::new(0.2));

    let mut mats = HashMap::new();
    for rt in [
        ResourceType::Wood,
        ResourceType::Copper,
        ResourceType::Iron,
        ResourceType::Gold,
        ResourceType::Oil,
    ] {
        let color = rt.carry_color();
        let mat = materials.add(StandardMaterial {
            base_color: color,
            ..default()
        });
        mats.insert(rt, mat);
    }

    commands.insert_resource(CarryVisualAssets {
        cube_mesh,
        sphere_mesh,
        materials: mats,
    });

    // Storage pile assets
    let pile_cube = meshes.add(Cuboid::new(0.4, 0.4, 0.4));
    let pile_sphere = meshes.add(Sphere::new(0.25));
    let pile_cylinder = meshes.add(Cylinder::new(0.2, 0.5));

    let mut pile_mats = HashMap::new();
    for rt in [
        ResourceType::Wood,
        ResourceType::Copper,
        ResourceType::Iron,
        ResourceType::Gold,
        ResourceType::Oil,
    ] {
        let color = rt.carry_color();
        let mat = materials.add(StandardMaterial {
            base_color: color,
            ..default()
        });
        pile_mats.insert(rt, mat);
    }

    commands.insert_resource(StoragePileAssets {
        cube_mesh: pile_cube,
        sphere_mesh: pile_sphere,
        cylinder_mesh: pile_cylinder,
        materials: pile_mats,
    });
}

fn worker_ai_system(
    mut commands: Commands,
    time: Res<Time>,
    mut all_resources: ResMut<AllPlayerResources>,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut workers: Query<
        (
            Entity,
            &Transform,
            &mut UnitState,
            &mut TaskSource,
            &mut Carrying,
            &GatherSpeed,
            &CarryCapacity,
            &mut GatherAccumulator,
            &EntityKind,
            &Faction,
            Option<&MoveTarget>,
            &mut TaskQueue,
        ),
        With<Unit>,
    >,
    mut nodes: Query<(&Transform, &mut ResourceNode), Without<Unit>>,
    deposit_points: Query<
        (Entity, &Transform, &BuildingState, &Faction),
        (With<DepositPoint>, Without<Unit>),
    >,
    mut inventories: Query<
        (&Transform, Option<&mut StorageInventory>),
        (With<DepositPoint>, Without<Unit>),
    >,
    all_nodes: Query<(Entity, &Transform), (With<ResourceNode>, Without<Unit>)>,
    construction_sites: Query<
        (Entity, &Transform, &BuildingState, &Faction),
        (With<Building>, Without<Unit>, Without<ResourceNode>),
    >,
    storage_auras: Query<(&Transform, &StorageAura, &BuildingState), With<Building>>,
) {
    let gather_range = 3.0;
    let deposit_range = 4.0;
    let auto_scan_range = 20.0;

    for (
        entity,
        tf,
        mut state,
        mut source,
        mut carrying,
        speed,
        capacity,
        mut gather_accumulator,
        kind,
        worker_faction,
        _move_target,
        mut task_queue,
    ) in &mut workers
    {
        if *kind != EntityKind::Worker {
            continue;
        }

        if !matches!(*state, UnitState::Gathering(_)) {
            gather_accumulator.0 = 0.0;
        }

        match *state {
            UnitState::Moving(_) => {
                // Player issued a manual move — handled by unit_state_executor
                continue;
            }
            UnitState::Idle => {
                // Only auto-decide if source is Auto and queue is empty
                if *source == TaskSource::Manual
                    || task_queue.current.is_some()
                    || !task_queue.queue.is_empty()
                {
                    continue;
                }

                // If carrying resources, find depot to deposit (resource-type aware)
                if carrying.amount > 0 {
                    let new_state = return_to_depot_or_idle(
                        &mut commands,
                        entity,
                        &tf.translation,
                        worker_faction,
                        &carrying,
                        None,
                        &deposit_points,
                        Some(&inventories),
                    );
                    if !matches!(new_state, UnitState::Idle) {
                        *state = new_state;
                    }
                    continue;
                }
                // Check for nearby unfinished buildings first (prioritize construction)
                let auto_build_range = 20.0;
                let mut closest_site = None;
                let mut closest_site_dist = f32::MAX;
                for (site_entity, site_tf, site_state, site_faction) in &construction_sites {
                    if *site_state != BuildingState::UnderConstruction
                        || site_faction != worker_faction
                    {
                        continue;
                    }
                    let dist = tf.translation.distance(site_tf.translation);
                    if dist < auto_build_range && dist < closest_site_dist {
                        closest_site_dist = dist;
                        closest_site = Some(site_entity);
                    }
                }
                if let Some(site) = closest_site {
                    *state = UnitState::MovingToBuild(site);
                } else if let Some(node) =
                    find_nearest_node(&tf.translation, auto_scan_range, &all_nodes)
                {
                    *state = UnitState::Gathering(node);
                }
            }

            UnitState::Gathering(node) => {
                // Interrupt auto-gathering for nearby construction (only if not carrying)
                if *source == TaskSource::Auto && carrying.amount == 0 {
                    let auto_build_range = 20.0;
                    let mut closest_site = None;
                    let mut closest_site_dist = f32::MAX;
                    for (site_entity, site_tf, site_state, site_faction) in &construction_sites {
                        if *site_state != BuildingState::UnderConstruction
                            || site_faction != worker_faction
                        {
                            continue;
                        }
                        let dist = tf.translation.distance(site_tf.translation);
                        if dist < auto_build_range && dist < closest_site_dist {
                            closest_site_dist = dist;
                            closest_site = Some(site_entity);
                        }
                    }
                    if let Some(site) = closest_site {
                        commands.entity(entity).remove::<MoveTarget>();
                        *state = UnitState::MovingToBuild(site);
                        continue;
                    }
                }

                let Ok((node_tf, mut node_data)) = nodes.get_mut(node) else {
                    // Node gone
                    commands.entity(entity).remove::<MoveTarget>();
                    let new_state = return_to_depot_or_idle(
                        &mut commands,
                        entity,
                        &tf.translation,
                        worker_faction,
                        &carrying,
                        None,
                        &deposit_points,
                        Some(&inventories),
                    );
                    *state = new_state;
                    if matches!(*state, UnitState::Idle) {
                        *source = TaskSource::Auto;
                        task_queue.current = None;
                    }
                    continue;
                };

                if node_data.amount_remaining == 0 {
                    let new_state = return_to_depot_or_idle(
                        &mut commands,
                        entity,
                        &tf.translation,
                        worker_faction,
                        &carrying,
                        None,
                        &deposit_points,
                        Some(&inventories),
                    );
                    *state = new_state;
                    if matches!(*state, UnitState::Idle) {
                        *source = TaskSource::Auto;
                        task_queue.current = None;
                    }
                    continue;
                }

                // If pushed away by avoidance, go back
                let dist = tf.translation.distance(node_tf.translation);
                if dist > gather_range {
                    // MovingToResource equivalent: just set MoveTarget, stay in Gathering
                    commands
                        .entity(entity)
                        .insert(MoveTarget(node_tf.translation));
                    continue;
                }

                // Remove any stale MoveTarget
                commands.entity(entity).remove::<MoveTarget>();

                let rt = node_data.resource_type;
                let node_pos = node_tf.translation;

                // If carrying a different resource type, return to deposit first.
                if carrying.amount > 0 {
                    if let Some(carried_rt) = carrying.resource_type {
                        if carried_rt != rt {
                            let new_state = return_to_depot_or_idle(
                                &mut commands,
                                entity,
                                &tf.translation,
                                worker_faction,
                                &carrying,
                                Some(node),
                                &deposit_points,
                                Some(&inventories),
                            );
                            *state = new_state;
                            if matches!(*state, UnitState::Idle) {
                                *source = TaskSource::Auto;
                                task_queue.current = None;
                            }
                            continue;
                        }
                    }
                }

                // Frame-rate independent gather tick (with storage aura + per-resource modifier).
                let unit_weight = rt.weight();
                let aura_bonus =
                    crate::buildings::storage_aura_bonus(tf.translation, &storage_auras);
                let effective_speed = speed.0 * (1.0 + aura_bonus) * rt.gather_rate_multiplier();
                gather_accumulator.0 += effective_speed * time.delta_secs();
                let mut amount = gather_accumulator.0.floor() as u32;
                if amount == 0 {
                    continue;
                }
                gather_accumulator.0 -= amount as f32;
                amount = amount.min(node_data.amount_remaining);

                let remaining_capacity = (capacity.0 - carrying.weight).max(0.0);
                let can_carry = (remaining_capacity / unit_weight).floor() as u32;
                if can_carry == 0 {
                    let new_state = return_to_depot_or_idle(
                        &mut commands,
                        entity,
                        &tf.translation,
                        worker_faction,
                        &carrying,
                        Some(node),
                        &deposit_points,
                        Some(&inventories),
                    );
                    *state = new_state;
                    if matches!(*state, UnitState::Idle) {
                        *source = TaskSource::Auto;
                        task_queue.current = None;
                    }
                    continue;
                }

                let actual = amount.min(can_carry);
                if actual == 0 {
                    continue;
                }

                node_data.amount_remaining -= actual;
                carrying.amount += actual;
                carrying.weight += actual as f32 * unit_weight;
                carrying.resource_type = Some(rt);

                if let Some(ref vfx) = vfx_assets {
                    spawn_gather_vfx(&mut commands, vfx, tf.translation, node_pos, rt, actual);
                }

                // If effectively full for this resource type, head to a depot.
                let remaining_capacity_after = (capacity.0 - carrying.weight).max(0.0);
                if remaining_capacity_after < unit_weight {
                    let new_state = return_to_depot_or_idle(
                        &mut commands,
                        entity,
                        &tf.translation,
                        worker_faction,
                        &carrying,
                        Some(node),
                        &deposit_points,
                        Some(&inventories),
                    );
                    *state = new_state;
                    if matches!(*state, UnitState::Idle) {
                        *source = TaskSource::Auto;
                        task_queue.current = None;
                    }
                }
            }

            UnitState::ReturningToDeposit { depot, gather_node } => {
                // Check depot still exists
                let Ok((depot_tf, _)) = inventories.get(depot) else {
                    commands.entity(entity).remove::<MoveTarget>();
                    let new_state = return_to_depot_or_idle(
                        &mut commands,
                        entity,
                        &tf.translation,
                        worker_faction,
                        &carrying,
                        gather_node,
                        &deposit_points,
                        Some(&inventories),
                    );
                    *state = new_state;
                    if matches!(*state, UnitState::Idle) {
                        *source = TaskSource::Auto;
                        task_queue.current = None;
                    }
                    continue;
                };

                let dist = tf.translation.distance(depot_tf.translation);
                if dist <= deposit_range {
                    *state = UnitState::Depositing { depot, gather_node };
                } else {
                    // Ensure MoveTarget
                    commands
                        .entity(entity)
                        .insert(MoveTarget(depot_tf.translation));
                }
            }

            UnitState::Depositing { depot, gather_node } => {
                // Transfer resources (capacity-limited)
                let mut deposit_info: Option<(ResourceType, u32)> = None;
                if let Some(rt) = carrying.resource_type {
                    let mut deposited = carrying.amount;

                    // Check storage capacity
                    if let Ok((_, inventory)) = inventories.get_mut(depot) {
                        if let Some(mut inv) = inventory {
                            deposited = inv.add_capped(rt, carrying.amount);
                        }
                    }

                    if deposited == 0 {
                        // Storage full — wait nearby
                        event_log.push(
                            time.elapsed_secs(),
                            "Storage full — worker waiting".into(),
                            crate::ui::event_log_widget::EventCategory::Alert,
                            Some(tf.translation),
                            Some(*worker_faction),
                        );
                        commands.entity(entity).remove::<MoveTarget>();
                        *state = UnitState::WaitingForStorage { depot, gather_node };
                        continue;
                    }

                    // Add deposited amount to global resources
                    all_resources.get_mut(worker_faction).add(rt, deposited);
                    deposit_info = Some((rt, deposited));

                    // Update carrying with leftover
                    let leftover = carrying.amount - deposited;
                    if leftover > 0 {
                        carrying.amount = leftover;
                        carrying.weight = leftover as f32 * rt.weight();
                        // Worker still has resources — wait for capacity
                        commands.entity(entity).remove::<MoveTarget>();
                        *state = UnitState::WaitingForStorage { depot, gather_node };

                        // Still spawn VFX for partial deposit
                        if let Some(ref vfx) = vfx_assets {
                            if let Ok((depot_tf, _)) = inventories.get(depot) {
                                let deposit_pos = depot_tf.translation + Vec3::Y * 2.0;
                                spawn_deposit_vfx(&mut commands, &vfx, deposit_pos, 4, 0.15, 0.3);
                                spawn_resource_popup(
                                    &mut commands,
                                    deposit_pos + Vec3::Y * 1.5,
                                    rt,
                                    deposited,
                                );
                            }
                        }
                        continue;
                    }
                }

                // Spawn deposit VFX + resource popup
                if let Some(ref vfx) = vfx_assets {
                    if let Ok((depot_tf, _)) = inventories.get(depot) {
                        let deposit_pos = depot_tf.translation + Vec3::Y * 2.0;
                        spawn_deposit_vfx(&mut commands, &vfx, deposit_pos, 4, 0.15, 0.3);
                        if let Some((rt, amount)) = deposit_info {
                            spawn_resource_popup(
                                &mut commands,
                                deposit_pos + Vec3::Y * 1.5,
                                rt,
                                amount,
                            );
                        }
                    }
                }

                // Clear carrying
                carrying.amount = 0;
                carrying.weight = 0.0;
                carrying.resource_type = None;
                commands.entity(entity).remove::<MoveTarget>();

                // Return to gather node if it still has resources
                if let Some(gn) = gather_node {
                    if let Ok((_, node_data)) = nodes.get(gn) {
                        if node_data.amount_remaining > 0 {
                            *state = UnitState::Gathering(gn);
                            continue;
                        }
                    }
                }
                // No gather node or depleted — scan broadly for next resource
                if let Some(node) = find_nearest_node(&tf.translation, auto_scan_range, &all_nodes)
                {
                    *state = UnitState::Gathering(node);
                } else {
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                }
            }

            UnitState::WaitingForStorage { depot, gather_node } => {
                let carried_rt = carrying.resource_type;

                // Periodically check if depot has capacity for the carried resource type
                let has_space = if let Ok((_, inventory)) = inventories.get(depot) {
                    inventory.map_or(true, |inv| {
                        if let Some(rt) = carried_rt {
                            inv.accepts(rt) && inv.remaining_capacity_for(rt) > 0
                        } else {
                            inv.remaining_capacity() > 0
                        }
                    })
                } else {
                    false
                };

                if has_space {
                    *state = UnitState::Depositing { depot, gather_node };
                    continue;
                }

                // Try a different depot that accepts our resource type and has space
                let mut best_depot = None;
                let mut best_dist = f32::MAX;
                for (dp_entity, dp_tf, dp_state, dp_faction) in &deposit_points {
                    if dp_faction != worker_faction || *dp_state != BuildingState::Complete {
                        continue;
                    }
                    // Check this depot accepts and has capacity for the carried resource
                    if let Ok((_, inv_opt)) = inventories.get(dp_entity) {
                        if let Some(inv) = inv_opt {
                            if let Some(rt) = carried_rt {
                                if !inv.accepts(rt) || inv.remaining_capacity_for(rt) == 0 {
                                    continue;
                                }
                            } else if inv.remaining_capacity() == 0 {
                                continue;
                            }
                        }
                    }
                    let dist = tf.translation.distance(dp_tf.translation);
                    if dist < best_dist {
                        best_dist = dist;
                        best_depot = Some(dp_entity);
                    }
                }

                if let Some(new_depot) = best_depot {
                    let depot_pos = deposit_points.get(new_depot).unwrap().1.translation;
                    commands.entity(entity).insert(MoveTarget(depot_pos));
                    *state = UnitState::ReturningToDeposit {
                        depot: new_depot,
                        gather_node,
                    };
                }
                // Otherwise keep waiting at current depot
            }

            // Building/MovingToBuild states are handled by unit_state_executor
            // InsideProcessor/MovingToProcessor handled by processor_worker_visual_system + unit_state_executor
            _ => {}
        }
    }
}

/// Resource processing buildings auto-harvest nearby nodes on a timer and deposit into player resources.
fn resource_processor_system(
    time: Res<Time>,
    mut commands: Commands,
    mut all_resources: ResMut<AllPlayerResources>,
    mut processors: Query<
        (
            Entity,
            &Transform,
            &mut ResourceProcessor,
            &BuildingState,
            &Faction,
            Option<&mut StorageInventory>,
            Option<&AssignedWorkers>,
        ),
        With<Building>,
    >,
    mut nodes: Query<(&Transform, &mut ResourceNode), Without<Building>>,
    vfx_assets: Option<Res<VfxAssets>>,
) {
    for (_building_entity, building_tf, mut processor, state, faction, storage, assigned_workers) in
        &mut processors
    {
        if *state != BuildingState::Complete {
            continue;
        }

        processor.harvest_timer.tick(time.delta());
        if !processor.harvest_timer.just_finished() {
            continue;
        }

        // Count assigned workers for this building
        let worker_count = assigned_workers.map(|aw| aw.workers.len()).unwrap_or(0) as f32;

        // Effective amount per tick = base_rate + (worker_count * base_rate * worker_rate_bonus)
        let effective_rate = processor.harvest_rate
            + (worker_count * processor.harvest_rate * processor.worker_rate_bonus);
        processor.harvest_accumulator += effective_rate;
        let amount = processor.harvest_accumulator as u32;
        processor.harvest_accumulator -= amount as f32;
        if amount == 0 {
            continue;
        }
        processor.buffer += amount;

        // Find nearest matching resource node in range and drain from it
        let mut harvested_type = None;
        for (node_tf, mut node) in &mut nodes {
            if !processor.resource_types.contains(&node.resource_type) {
                continue;
            }
            let dist = building_tf.translation.distance(node_tf.translation);
            if dist > processor.harvest_radius {
                continue;
            }
            if node.amount_remaining == 0 {
                continue;
            }

            let drain = processor.buffer.min(node.amount_remaining);
            if drain > 0 {
                node.amount_remaining -= drain;
                harvested_type = Some((node.resource_type, drain));
                processor.buffer -= drain;
                break;
            }
        }

        // Transfer harvested resources to player and spawn popup
        if let Some((rt, amount)) = harvested_type {
            let stored_amount;
            if let Some(mut inv) = storage {
                let stored = inv.add_capped(rt, amount);
                if stored > 0 {
                    all_resources.get_mut(faction).add(rt, stored);
                }
                if amount > stored {
                    processor.buffer += amount - stored;
                }
                stored_amount = stored;
            } else {
                all_resources.get_mut(faction).add(rt, amount);
                stored_amount = amount;
            }

            // Spawn floating "+N" resource popup above the building
            if stored_amount > 0 {
                let popup_pos = building_tf.translation + Vec3::Y * 3.5;
                spawn_resource_popup(&mut commands, popup_pos, rt, stored_amount);

                // Also spawn deposit VFX particles
                if let Some(ref vfx) = vfx_assets {
                    let deposit_pos = building_tf.translation + Vec3::Y * 2.0;
                    spawn_deposit_vfx(&mut commands, &vfx, deposit_pos, 3, 0.12, 0.25);
                }
            }
        }
    }
}

/// Spawn a floating "+N" resource popup UI node at a world position.
fn spawn_resource_popup(commands: &mut Commands, world_pos: Vec3, rt: ResourceType, amount: u32) {
    let color = rt.carry_color();
    let srgba = color.to_srgba();
    commands.spawn((
        ResourcePopup {
            lifetime: Timer::from_seconds(1.2, TimerMode::Once),
            world_pos,
            resource_type: rt,
            amount,
        },
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(-1000.0),
            top: Val::Px(-1000.0),
            ..default()
        },
        Text::new(format!("+{}", amount)),
        TextFont {
            font_size: theme::FONT_LARGE,
            ..default()
        },
        TextColor(Color::srgb(
            srgba.red.max(0.4),
            srgba.green.max(0.6),
            srgba.blue.max(0.3),
        )),
        TextLayout::new_with_justify(Justify::Center),
        Pickable::IGNORE,
    ));
}

/// Tick resource popup lifetimes, animate font size, fade out, and despawn when done.
fn update_resource_popups(
    mut commands: Commands,
    time: Res<Time>,
    mut popups: Query<(Entity, &mut ResourcePopup, &mut TextColor, &mut TextFont)>,
) {
    for (entity, mut popup, mut color, mut font) in &mut popups {
        popup.lifetime.tick(time.delta());
        let frac = popup.lifetime.fraction();

        // Pop-in scale animation
        let scale = if frac < 0.1 {
            0.5 + (frac / 0.1) * 0.7
        } else if frac < 0.25 {
            1.2 - ((frac - 0.1) / 0.15) * 0.2
        } else {
            1.0
        };
        font.font_size = theme::FONT_LARGE * scale;

        // Rise in world Y
        popup.world_pos.y += 30.0 * time.delta_secs() * 0.02;

        // Fade out in the last 40%
        let alpha = if frac > 0.6 {
            1.0 - (frac - 0.6) / 0.4
        } else {
            1.0
        };
        let base = color.0.to_srgba();
        color.0 = Color::srgba(base.red, base.green, base.blue, alpha);

        if popup.lifetime.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

/// Try to send a worker to the nearest depot, or go idle if none found.
/// Returns the new state to assign.
fn return_to_depot_or_idle(
    commands: &mut Commands,
    entity: Entity,
    pos: &Vec3,
    faction: &Faction,
    carrying: &Carrying,
    gather_node: Option<Entity>,
    deposit_points: &Query<
        (Entity, &Transform, &BuildingState, &Faction),
        (With<DepositPoint>, Without<Unit>),
    >,
    inventories: Option<
        &Query<(&Transform, Option<&mut StorageInventory>), (With<DepositPoint>, Without<Unit>)>,
    >,
) -> UnitState {
    if carrying.amount > 0 {
        if let Some(depot) = find_nearest_deposit_for(
            pos,
            faction,
            carrying.resource_type,
            deposit_points,
            inventories,
        ) {
            let depot_pos = deposit_points.get(depot).unwrap().1.translation;
            commands.entity(entity).insert(MoveTarget(depot_pos));
            return UnitState::ReturningToDeposit { depot, gather_node };
        }
    }
    UnitState::Idle
}

/// Scan for the nearest resource node within range.
fn find_nearest_node(
    pos: &Vec3,
    range: f32,
    all_nodes: &Query<(Entity, &Transform), (With<ResourceNode>, Without<Unit>)>,
) -> Option<Entity> {
    let mut closest_dist = f32::MAX;
    let mut closest_node = None;
    for (node_entity, node_tf) in all_nodes {
        let dist = pos.distance(node_tf.translation);
        if dist < range && dist < closest_dist {
            closest_dist = dist;
            closest_node = Some(node_entity);
        }
    }
    closest_node
}

/// Spawn a short burst of gather particles when a worker extracts resources from a node.
fn spawn_gather_vfx(
    commands: &mut Commands,
    vfx: &VfxAssets,
    worker_pos: Vec3,
    node_pos: Vec3,
    rt: ResourceType,
    amount: u32,
) {
    let mesh = match rt {
        ResourceType::Wood => vfx.cube_mesh.clone(),
        _ => vfx.sphere_mesh.clone(),
    };
    let material = vfx
        .resource_particle_materials
        .get(&rt)
        .cloned()
        .unwrap_or_else(|| vfx.impact_material.clone());
    let base_count = match rt {
        ResourceType::Wood => 2,
        ResourceType::Copper => 3,
        ResourceType::Iron => 3,
        ResourceType::Gold => 4,
        ResourceType::Oil => 2,
    };
    let count = (base_count + amount.min(4)).min(8).max(1);
    let base_scale = match rt {
        ResourceType::Wood => 0.09,
        ResourceType::Oil => 0.07,
        ResourceType::Gold => 0.08,
        _ => 0.075,
    };
    let toward_worker = (worker_pos - node_pos).normalize_or_zero();

    for i in 0..count {
        let frac = i as f32 / count as f32;
        let angle = std::f32::consts::TAU * frac;
        let lateral = Vec3::new(angle.cos() * 0.35, 0.0, angle.sin() * 0.35);
        let jitter = Vec3::new(
            (frac * 11.3).sin() * 0.12,
            0.2 + frac * 0.18,
            (frac * 7.7).cos() * 0.12,
        );
        let velocity =
            toward_worker * (1.1 + frac * 0.9) + lateral * 0.9 + Vec3::Y * (1.2 + frac * 0.8);

        commands.spawn((
            GatherParticle {
                timer: Timer::from_seconds(0.42, TimerMode::Once),
                velocity,
                start_scale: base_scale * (1.0 + frac * 0.35),
            },
            FogHideable::Vfx,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(node_pos + Vec3::Y * 0.7 + jitter)
                .with_scale(Vec3::splat(base_scale)),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

/// Spawn deposit VFX particles around a position.
fn spawn_deposit_vfx(
    commands: &mut Commands,
    vfx: &VfxAssets,
    deposit_pos: Vec3,
    count: u32,
    scale: f32,
    duration: f32,
) {
    for i in 0..count {
        let angle = std::f32::consts::TAU * (i as f32 / count as f32);
        let offset = Vec3::new(angle.cos() * 0.5, 0.5, angle.sin() * 0.5);
        commands.spawn((
            VfxFlash {
                timer: Timer::from_seconds(duration, TimerMode::Once),
                start_scale: scale,
                end_scale: 0.0,
            },
            FogHideable::Vfx,
            Mesh3d(vfx.sphere_mesh.clone()),
            MeshMaterial3d(vfx.deposit_material.clone()),
            Transform::from_translation(deposit_pos + offset).with_scale(Vec3::splat(scale)),
        ));
    }
}

/// Find nearest deposit that accepts the given resource type.
/// If `resource_type` is Some, only depots that accept that type AND have remaining capacity are returned.
/// If `inventories` is None, capacity checks are skipped.
fn find_nearest_deposit_for(
    pos: &Vec3,
    faction: &Faction,
    resource_type: Option<ResourceType>,
    deposit_points: &Query<
        (Entity, &Transform, &BuildingState, &Faction),
        (With<DepositPoint>, Without<Unit>),
    >,
    inventories: Option<
        &Query<(&Transform, Option<&mut StorageInventory>), (With<DepositPoint>, Without<Unit>)>,
    >,
) -> Option<Entity> {
    let mut closest_dist = f32::MAX;
    let mut closest = None;
    for (entity, tf, state, depot_faction) in deposit_points {
        if *state != BuildingState::Complete || depot_faction != faction {
            continue;
        }
        // If we have a resource type, check that the depot accepts it and has capacity
        if let Some(rt) = resource_type {
            if let Some(inv_query) = inventories {
                if let Ok((_, inv_opt)) = inv_query.get(entity) {
                    if let Some(inv) = inv_opt {
                        if !inv.accepts(rt) || inv.remaining_capacity_for(rt) == 0 {
                            continue;
                        }
                    }
                }
            }
        }
        let dist = pos.distance(tf.translation);
        if dist < closest_dist {
            closest_dist = dist;
            closest = Some(entity);
        }
    }
    closest
}

/// Recompute per-faction totals of resources carried by workers each frame.
fn compute_carried_totals(
    workers: Query<(&Carrying, &Faction), With<Unit>>,
    mut totals: ResMut<CarriedResourceTotals>,
) {
    totals.per_faction.clear();
    for (carrying, faction) in &workers {
        if let Some(rt) = carrying.resource_type {
            if carrying.amount > 0 {
                totals
                    .per_faction
                    .entry(*faction)
                    .or_insert_with(PlayerResources::empty)
                    .add(rt, carrying.amount);
            }
        }
    }
}

/// Drain carried resources from workers for pending spend requests.
fn drain_carried_from_workers(
    mut drains: ResMut<PendingCarriedDrains>,
    mut workers: Query<(&mut Carrying, &Faction), With<Unit>>,
) {
    for msg in drains.drains.iter_mut() {
        if !msg.has_deficit() {
            continue;
        }
        for (mut carrying, faction) in &mut workers {
            if *faction != msg.faction {
                continue;
            }
            if !msg.has_deficit() {
                break;
            }
            let Some(rt) = carrying.resource_type else {
                continue;
            };
            let needed = msg.get(rt);
            if needed == 0 || carrying.amount == 0 {
                continue;
            }
            let take = needed.min(carrying.amount);
            carrying.amount -= take;
            carrying.weight = carrying.amount as f32 * rt.weight();
            msg.sub(rt, take);
            if carrying.amount == 0 {
                carrying.resource_type = None;
                carrying.weight = 0.0;
            }
        }
    }
    drains.drains.clear();
}

fn update_carry_visuals(
    mut commands: Commands,
    carry_assets: Option<Res<CarryVisualAssets>>,
    mut workers: Query<
        (Entity, &Carrying, &CarryCapacity, Option<&CarryVisual>),
        (With<Unit>, With<GatherSpeed>),
    >,
) {
    let Some(assets) = carry_assets else { return };

    for (entity, carrying, capacity, carry_visual) in &mut workers {
        if carrying.amount > 0 && carry_visual.is_none() {
            // Spawn carry visual
            if let Some(rt) = carrying.resource_type {
                let mesh = match rt {
                    ResourceType::Wood => assets.cube_mesh.clone(),
                    _ => assets.sphere_mesh.clone(),
                };
                let mat = assets.materials.get(&rt).cloned().unwrap_or_default();
                let scale_factor = 0.5 + 0.5 * (carrying.weight / capacity.0).min(1.0);

                let child = commands
                    .spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(mat),
                        Transform::from_translation(Vec3::new(0.0, 0.8, -0.3))
                            .with_scale(Vec3::splat(scale_factor)),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ))
                    .id();
                commands.entity(entity).add_child(child);
                commands.entity(entity).insert(CarryVisual(child));
            }
        } else if carrying.amount == 0 {
            if let Some(visual) = carry_visual {
                commands.entity(visual.0).despawn();
                commands.entity(entity).remove::<CarryVisual>();
            }
        } else if let Some(visual) = carry_visual {
            // Update scale based on current weight
            let scale_factor = 0.5 + 0.5 * (carrying.weight / capacity.0).min(1.0);
            commands.entity(visual.0).insert(
                Transform::from_translation(Vec3::new(0.0, 0.8, -0.3))
                    .with_scale(Vec3::splat(scale_factor)),
            );
        }
    }
}

fn deplete_resource_nodes(
    mut commands: Commands,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
    time: Res<Time>,
    nodes: Query<(Entity, &ResourceNode, &Transform)>,
) {
    for (entity, node, transform) in &nodes {
        if node.amount_remaining == 0 {
            event_log.push(
                time.elapsed_secs(),
                format!("{} node depleted", node.resource_type.display_name()),
                crate::ui::event_log_widget::EventCategory::Resource,
                Some(transform.translation),
                None, // resource nodes are global, no faction
            );
            commands.entity(entity).despawn();
        }
    }
}

// ── Tree Growth Systems ──

fn spawn_saplings_system(
    mut commands: Commands,
    time: Res<Time>,
    mut config: ResMut<TreeGrowthConfig>,
    biome_map: Res<BiomeMap>,
    height_map: Res<HeightMap>,
    model_assets: Res<ModelAssets>,
    mature_trees: Query<&Transform, With<MatureTree>>,
    saplings: Query<&Sapling>,
    growing: Query<&GrowingTree>,
    game_config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
) {
    config.spawn_timer.tick(time.delta());
    if !config.spawn_timer.just_finished() {
        return;
    }

    let sapling_count = saplings.iter().count() as u32;
    let _growing_count = growing.iter().count() as u32;
    if sapling_count >= config.max_saplings {
        return;
    }

    if model_assets.trees.is_empty() {
        return;
    }

    let mut rng = rand::rng();
    let trees: Vec<Vec3> = mature_trees.iter().map(|t| t.translation).collect();
    if trees.is_empty() {
        return;
    }

    // Try to spawn a few saplings near random existing trees
    let spawns_per_tick = 3u32.min(config.max_saplings - sapling_count);
    for _ in 0..spawns_per_tick {
        let parent_pos = trees[rng.random_range(0..trees.len())];
        let angle = rng.random_range(0.0..std::f32::consts::TAU);
        let dist = rng.random_range(4.0..config.spawn_radius);
        let x = parent_pos.x + angle.cos() * dist;
        let z = parent_pos.z + angle.sin() * dist;

        // Only spawn in forest biome
        if biome_map.get_biome(x, z) != Biome::Forest {
            continue;
        }

        // Don't spawn too close to any player base
        let sapling_spawn_positions = game_config.spawn_positions(map_seed.0);
        let mut near_base = false;
        for &(_, (sx, sz)) in &sapling_spawn_positions {
            let dx = x - sx;
            let dz = z - sz;
            if (dx * dx + dz * dz).sqrt() < 25.0 {
                near_base = true;
                break;
            }
        }
        if near_base {
            continue;
        }

        let scene_handle =
            model_assets.trees[rng.random_range(0..model_assets.trees.len())].clone();
        let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);
        let target_scale = rng.random_range(0.8_f32..1.2);
        let initial_scale = 0.15;

        commands.spawn((
            Sapling {
                timer: Timer::from_seconds(config.sapling_duration, TimerMode::Once),
                target_scale,
            },
            FogHideable::Object,
            SceneRoot(scene_handle),
            Transform::from_translation(Vec3::new(x, height_map.sample(x, z), z))
                .with_rotation(Quat::from_rotation_y(y_rotation))
                .with_scale(Vec3::splat(initial_scale)),
        ));
    }
}

fn grow_saplings_system(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<TreeGrowthConfig>,
    mut saplings: Query<(Entity, &mut Sapling, &mut Transform)>,
) {
    for (entity, mut sapling, mut tf) in &mut saplings {
        sapling.timer.tick(time.delta());
        let progress = sapling.timer.fraction();
        // Lerp scale from 0.15 to 0.4
        let scale = 0.15 + progress * 0.25;
        tf.scale = Vec3::splat(scale);

        if sapling.timer.is_finished() {
            commands.entity(entity).remove::<Sapling>();
            commands.entity(entity).insert(GrowingTree {
                stage: 0,
                timer: Timer::from_seconds(config.growth_stage_duration, TimerMode::Once),
                target_scale: sapling.target_scale,
            });
        }
    }
}

fn grow_trees_system(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<TreeGrowthConfig>,
    mut growing: Query<(Entity, &mut GrowingTree, &mut Transform)>,
) {
    for (entity, mut tree, mut tf) in &mut growing {
        tree.timer.tick(time.delta());
        let progress = tree.timer.fraction();

        // Stage scale ranges: 0→(0.4..0.6), 1→(0.6..0.8), 2→(0.8..target)
        let (from, to) = match tree.stage {
            0 => (0.4, 0.6),
            1 => (0.6, 0.8),
            _ => (0.8, tree.target_scale),
        };
        let scale = from + progress * (to - from);
        tf.scale = Vec3::splat(scale);

        if tree.timer.is_finished() {
            if tree.stage >= 2 {
                // Promote to mature tree
                commands.entity(entity).remove::<GrowingTree>();
                commands.entity(entity).insert((
                    MatureTree,
                    ResourceNode {
                        resource_type: ResourceType::Wood,
                        amount_remaining: config.mature_wood_amount,
                    },
                    PickRadius(3.0 * tree.target_scale),
                ));
            } else {
                tree.stage += 1;
                tree.timer = Timer::from_seconds(config.growth_stage_duration, TimerMode::Once);
            }
        }
    }
}

// ── Processor Worker Visual System ──

/// Drives the ProcessorWorkerState state machine for workers in InsideProcessor state.
/// Workers in InsideProcessor are hidden but still run a visual harvest/deposit loop.
fn processor_worker_visual_system(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut workers: Query<
        (
            Entity,
            &Transform,
            &mut ProcessorWorkerState,
            &UnitState,
            &mut Carrying,
        ),
        With<Unit>,
    >,
    processors: Query<(Entity, &Transform, &ResourceProcessor, &BuildingState), With<Building>>,
    nodes: Query<(Entity, &Transform, &ResourceNode), Without<Unit>>,
) {
    // Collect nodes targeted by other workers to avoid clustering
    let mut targeted_nodes: Vec<Entity> = Vec::new();
    for (_, _, pstate, ustate, _) in workers.iter() {
        if !matches!(ustate, UnitState::InsideProcessor(_)) {
            continue;
        }
        if let ProcessorWorkerState::MovingToNode(node)
        | ProcessorWorkerState::Harvesting { node, .. } = *pstate
        {
            targeted_nodes.push(node);
        }
    }

    for (_entity, tf, mut worker_state, unit_state, _carrying) in &mut workers {
        let UnitState::InsideProcessor(building_entity) = *unit_state else {
            continue;
        };

        let Ok((_, building_tf, processor, building_state)) = processors.get(building_entity)
        else {
            // Building gone — handled by unit_state_executor
            continue;
        };

        if *building_state != BuildingState::Complete {
            continue;
        }

        match *worker_state {
            ProcessorWorkerState::Idle => {
                // Find nearest resource node within processor's harvest_radius not targeted by another worker
                let mut best: Option<(Entity, f32)> = None;
                for (node_entity, node_tf, node_data) in &nodes {
                    if !processor.resource_types.contains(&node_data.resource_type) {
                        continue;
                    }
                    if node_data.amount_remaining == 0 {
                        continue;
                    }
                    let dist_to_building = building_tf.translation.distance(node_tf.translation);
                    if dist_to_building > processor.harvest_radius {
                        continue;
                    }
                    let already_targeted =
                        targeted_nodes.iter().filter(|&&n| n == node_entity).count();
                    if already_targeted >= 2 {
                        continue;
                    }
                    let dist = tf.translation.distance(node_tf.translation);
                    if best.is_none() || dist < best.unwrap().1 {
                        best = Some((node_entity, dist));
                    }
                }
                if let Some((node, _)) = best {
                    *worker_state = ProcessorWorkerState::MovingToNode(node);
                }
            }
            ProcessorWorkerState::MovingToNode(node) => {
                let Ok((_, _node_tf, node_data)) = nodes.get(node) else {
                    *worker_state = ProcessorWorkerState::Idle;
                    continue;
                };
                if node_data.amount_remaining == 0 {
                    *worker_state = ProcessorWorkerState::Idle;
                    continue;
                }
                // Workers inside processors are hidden, so just timer-simulate "moving"
                *worker_state = ProcessorWorkerState::Harvesting {
                    node,
                    timer_secs: 0.0,
                };
            }
            ProcessorWorkerState::Harvesting {
                node,
                ref mut timer_secs,
            } => {
                if nodes.get(node).is_err()
                    || nodes
                        .get(node)
                        .map(|(_, _, n)| n.amount_remaining == 0)
                        .unwrap_or(true)
                {
                    *worker_state = ProcessorWorkerState::Idle;
                    continue;
                }
                *timer_secs += time.delta_secs();
                if *timer_secs >= 2.5 {
                    *worker_state = ProcessorWorkerState::ReturningToBuilding;
                }
            }
            ProcessorWorkerState::ReturningToBuilding => {
                *worker_state = ProcessorWorkerState::Depositing { timer_secs: 0.0 };
            }
            ProcessorWorkerState::Depositing { ref mut timer_secs } => {
                *timer_secs += time.delta_secs();
                if *timer_secs >= 0.5 {
                    // Deposit VFX at building
                    if let Some(ref vfx) = vfx_assets {
                        let deposit_pos = building_tf.translation + Vec3::Y * 2.0;
                        for i in 0..3 {
                            let angle = std::f32::consts::TAU * (i as f32 / 3.0);
                            let offset = Vec3::new(angle.cos() * 0.4, 0.3, angle.sin() * 0.4);
                            commands.spawn((
                                VfxFlash {
                                    timer: Timer::from_seconds(0.25, TimerMode::Once),
                                    start_scale: 0.12,
                                    end_scale: 0.0,
                                },
                                FogHideable::Vfx,
                                Mesh3d(vfx.sphere_mesh.clone()),
                                MeshMaterial3d(vfx.deposit_material.clone()),
                                Transform::from_translation(deposit_pos + offset)
                                    .with_scale(Vec3::splat(0.12)),
                            ));
                        }
                    }
                    *worker_state = ProcessorWorkerState::Idle;
                }
            }
        }
    }
}

// ── Resource Respawn System ──

/// Processing buildings periodically spawn new resource nodes nearby.
fn resource_respawn_system(
    mut commands: Commands,
    time: Res<Time>,
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
        for rt in config.resource_types.clone() {
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
            let mut rng = rand::rng();
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

                let y = height_map.sample(x, z);

                if rt == ResourceType::Wood {
                    // Reuse sapling system — spawn a Sapling
                    if !model_assets.trees.is_empty() {
                        let scene_handle = model_assets.trees
                            [rng.random_range(0..model_assets.trees.len())]
                        .clone();
                        let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);
                        let target_scale = rng.random_range(0.8_f32..1.2);

                        commands.spawn((
                            Sapling {
                                timer: Timer::from_seconds(20.0, TimerMode::Once),
                                target_scale,
                            },
                            FogHideable::Object,
                            SceneRoot(scene_handle),
                            Transform::from_translation(Vec3::new(x, y, z))
                                .with_rotation(Quat::from_rotation_y(y_rotation))
                                .with_scale(Vec3::splat(0.15)),
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
                            FogHideable::Object,
                            SceneRoot(scene_handle),
                            Transform::from_translation(Vec3::new(x, y - 0.5, z))
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
                            FogHideable::Object,
                            Mesh3d(mesh),
                            MeshMaterial3d(mat),
                            Transform::from_translation(Vec3::new(x, y, z))
                                .with_scale(Vec3::splat(0.1)),
                        ));
                    }
                }
                break;
            }
        }
    }
}

// ── Growing Resource System ──

/// Animates GrowingResource entities and promotes them to ResourceNode when complete.
fn grow_resource_system(
    mut commands: Commands,
    time: Res<Time>,
    mut growing: Query<(Entity, &mut GrowingResource, &mut Transform)>,
) {
    for (entity, mut res, mut tf) in &mut growing {
        res.timer.tick(time.delta());
        let progress = res.timer.fraction();

        // Scale from 0.1 to target_scale, with slight upward translation
        let scale = 0.1 + progress * (res.target_scale - 0.1);
        tf.scale = Vec3::splat(scale);
        // Slight emergence effect
        tf.translation.y += time.delta_secs() * 0.02 * (1.0 - progress);

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

// ── Worker assignment helpers ──

pub fn assign_worker_to_processor(commands: &mut Commands, worker: Entity, building: Entity) {
    commands
        .entity(worker)
        .insert(UnitState::InsideProcessor(building))
        .insert(TaskSource::Manual)
        .insert(ProcessorWorkerState::default())
        .remove::<MoveTarget>()
        .remove::<AttackTarget>()
        .insert(Visibility::Hidden);
}

pub fn unassign_worker_from_processor(commands: &mut Commands, worker: Entity) {
    commands
        .entity(worker)
        .remove::<ProcessorWorkerState>()
        .insert(UnitState::Idle)
        .insert(TaskSource::Auto)
        .insert(Visibility::Inherited);
}

// processor_worker_loop_system removed — merged into processor_worker_visual_system
