use bevy::prelude::*;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::components::*;
use crate::ground::{terrain_height, MAP_SIZE};
use rand::Rng;

pub struct ResourcesPlugin;

impl Plugin for ResourcesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerResources>()
            .add_systems(Startup, create_resource_node_materials)
            .add_systems(PostStartup, (spawn_resource_nodes, spawn_decorations))
            .add_systems(
                Update,
                (auto_gather_nearby, gather_resources, deplete_resource_nodes),
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
        Biome::Desert => 0.3,
        Biome::Mud => 0.3,
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
) -> Option<(ResourceType, u32, Handle<Mesh>, Handle<StandardMaterial>, f32)> {
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
            400,
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
) -> Option<(ResourceType, u32, Handle<Mesh>, Handle<StandardMaterial>, f32)> {
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
            300,
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
) {
    let wood_mesh = meshes.add(Cuboid::new(0.6, 2.5, 0.6));
    let ore_mesh = meshes.add(Cuboid::new(1.0, 0.8, 1.0));
    let gold_mesh = meshes.add(Cuboid::new(0.8, 0.8, 0.8));
    let oil_mesh = meshes.add(Cylinder::new(0.5, 1.2));

    let has_tree_models = !model_assets.trees.is_empty();
    let has_rock_models = !model_assets.rocks.is_empty();
    let mut rng = rand::rng();

    let placement_noise = Fbm::<Perlin>::new(999).set_octaves(2);
    let spacing = 12.0;
    let half = MAP_SIZE / 2.0;

    let mut x = -half + 5.0;
    while x < half - 5.0 {
        let mut z = -half + 5.0;
        while z < half - 5.0 {
            // Keep starting area clear
            let dist_from_center = (x * x + z * z).sqrt();
            if dist_from_center < 20.0 {
                z += spacing;
                continue;
            }

            let biome = biome_map.get_biome(x, z);
            let noise_val = placement_noise.get([x as f64 * 0.1, z as f64 * 0.1]) as f32;
            let threshold = biome_spawn_threshold(biome);

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
                    &node_mats,
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
                            SceneRoot(scene_handle),
                            Transform::from_translation(Vec3::new(x, terrain_height(x, z), z))
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
                            SceneRoot(scene_handle),
                            Transform::from_translation(Vec3::new(x, terrain_height(x, z), z))
                                .with_rotation(Quat::from_rotation_y(y_rotation))
                                .with_scale(Vec3::splat(scale_factor)),
                        ));
                    }
                    // Oil + fallbacks → primitive mesh
                    else {
                        let y = terrain_height(x, z) + half_h;
                        commands.spawn((
                            ResourceNode {
                                resource_type: rt,
                                amount_remaining: amount,
                            },
                            Mesh3d(mesh),
                            MeshMaterial3d(mat),
                            Transform::from_translation(Vec3::new(x, y, z)),
                        ));
                    }
                }

                // Secondary resource (lower probability)
                let secondary_noise = placement_noise
                    .get([x as f64 * 0.13 + 50.0, z as f64 * 0.13 + 50.0])
                    as f32;
                if secondary_noise > threshold + 0.3 {
                    if let Some((rt, amount, mesh, mat, half_h)) =
                        secondary_resource_for(biome, &ore_mesh, &gold_mesh, &node_mats)
                    {
                        let offset_x = x + 3.0;
                        let offset_z = z + 2.0;

                        if has_rock_models {
                            let scene_handle =
                                random_model(&mut rng, &model_assets.rocks).unwrap();
                            let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);
                            let scale_factor = rng.random_range(0.8_f32..1.2);
                            commands.spawn((
                                ResourceNode {
                                    resource_type: rt,
                                    amount_remaining: amount,
                                },
                                SceneRoot(scene_handle),
                                Transform::from_translation(Vec3::new(
                                    offset_x,
                                    terrain_height(offset_x, offset_z),
                                    offset_z,
                                ))
                                .with_rotation(Quat::from_rotation_y(y_rotation))
                                .with_scale(Vec3::splat(scale_factor)),
                            ));
                        } else {
                            let y = terrain_height(offset_x, offset_z) + half_h;
                            commands.spawn((
                                ResourceNode {
                                    resource_type: rt,
                                    amount_remaining: amount,
                                },
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
        Biome::Forest => (0.4, 0.35, 0.1, 0.0),
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

fn spawn_decorations(
    mut commands: Commands,
    biome_map: Res<BiomeMap>,
    model_assets: Res<ModelAssets>,
) {
    let mut rng = rand::rng();
    let deco_noise = Fbm::<Perlin>::new(777).set_octaves(2);
    let spacing = 8.0;
    let half = MAP_SIZE / 2.0;
    let max_decorations = 700;
    let mut count = 0u32;

    let mut x = -half + 4.0;
    while x < half - 4.0 {
        let mut z = -half + 4.0;
        while z < half - 4.0 {
            if count >= max_decorations {
                return;
            }

            // Keep starting area clear
            let dist_from_center = (x * x + z * z).sqrt();
            if dist_from_center < 20.0 {
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
                let y = terrain_height(ox, oz);
                let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);
                let scale = rng.random_range(scale_min..scale_max);

                commands.spawn((
                    Decoration,
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

fn auto_gather_nearby(
    mut commands: Commands,
    units: Query<
        (Entity, &Transform, &UnitType),
        (With<Unit>, Without<MoveTarget>, Without<GatherTarget>),
    >,
    nodes: Query<(Entity, &Transform), With<ResourceNode>>,
) {
    let gather_range = 3.0;

    for (unit_entity, unit_tf, unit_type) in &units {
        if *unit_type != UnitType::Worker {
            continue;
        }

        for (node_entity, node_tf) in &nodes {
            let dist = unit_tf.translation.distance(node_tf.translation);
            if dist < gather_range {
                commands
                    .entity(unit_entity)
                    .insert(GatherTarget(node_entity));
                break;
            }
        }
    }
}

fn gather_resources(
    mut commands: Commands,
    time: Res<Time>,
    mut player_res: ResMut<PlayerResources>,
    mut units: Query<(Entity, &GatherTarget, &mut Carrying, &GatherSpeed), With<Unit>>,
    mut nodes: Query<&mut ResourceNode>,
) {
    let carry_capacity = 20;

    for (unit_entity, gather_target, mut carrying, speed) in &mut units {
        let Ok(mut node) = nodes.get_mut(gather_target.0) else {
            commands.entity(unit_entity).remove::<GatherTarget>();
            continue;
        };

        if node.amount_remaining == 0 {
            commands.entity(unit_entity).remove::<GatherTarget>();
            continue;
        }

        let rt = node.resource_type;

        let amount = (speed.0 * time.delta_secs()) as u32;
        let amount = amount.max(1).min(node.amount_remaining);
        node.amount_remaining -= amount;
        carrying.amount += amount;
        carrying.resource_type = Some(rt);

        if carrying.amount >= carry_capacity {
            if let Some(crt) = carrying.resource_type {
                player_res.add(crt, carrying.amount);
            }
            carrying.amount = 0;
            carrying.resource_type = None;
        }
    }
}

fn deplete_resource_nodes(mut commands: Commands, nodes: Query<(Entity, &ResourceNode)>) {
    for (entity, node) in &nodes {
        if node.amount_remaining == 0 {
            commands.entity(entity).despawn();
        }
    }
}
