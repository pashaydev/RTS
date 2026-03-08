use bevy::prelude::*;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::blueprints::EntityKind;
use crate::components::*;
use crate::ground::{HeightMap, MAP_SIZE};
use rand::Rng;

pub struct ResourcesPlugin;

impl Plugin for ResourcesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerResources>()
            .init_resource::<TreeGrowthConfig>()
            .add_systems(Startup, (create_resource_node_materials, create_carry_visual_assets))
            .add_systems(PostStartup, (spawn_resource_nodes, spawn_decorations))
            .add_systems(
                Update,
                (worker_ai_system, deplete_resource_nodes, update_carry_visuals).chain(),
            )
            .add_systems(
                Update,
                (spawn_saplings_system, grow_saplings_system, grow_trees_system),
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
        Biome::Desert => 0.2,
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
            500,
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
        Biome::Forest => Some((
            ResourceType::Copper,
            200,
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
                            MatureTree,
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
                            PickRadius(half_h * 1.5),
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
    height_map: Res<HeightMap>,
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
                let y = height_map.sample(ox, oz);
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

fn create_carry_visual_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use std::collections::HashMap;

    let cube_mesh = meshes.add(Cuboid::new(0.3, 0.3, 0.3));
    let sphere_mesh = meshes.add(Sphere::new(0.2));

    let mut mats = HashMap::new();
    for rt in [ResourceType::Wood, ResourceType::Copper, ResourceType::Iron, ResourceType::Gold, ResourceType::Oil] {
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
    for rt in [ResourceType::Wood, ResourceType::Copper, ResourceType::Iron, ResourceType::Gold, ResourceType::Oil] {
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
    mut player_res: ResMut<PlayerResources>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut workers: Query<
        (Entity, &Transform, &mut WorkerTask, &mut Carrying, &GatherSpeed, &CarryCapacity, &EntityKind),
        With<Unit>,
    >,
    mut nodes: Query<(&Transform, &mut ResourceNode), Without<Unit>>,
    deposit_points: Query<(Entity, &Transform, &BuildingState), (With<DepositPoint>, Without<Unit>)>,
    mut inventories: Query<(&Transform, Option<&mut StorageInventory>), (With<DepositPoint>, Without<Unit>)>,
    all_nodes: Query<(Entity, &Transform), (With<ResourceNode>, Without<Unit>)>,
    construction_sites: Query<(Entity, &Transform, &BuildingState), (With<Building>, Without<Unit>, Without<ResourceNode>)>,
) {
    let gather_range = 3.0;
    let deposit_range = 4.0;
    let auto_scan_range = 3.0;

    for (entity, tf, mut task, mut carrying, speed, capacity, kind) in &mut workers {
        if *kind != EntityKind::Worker {
            continue;
        }

        match *task {
            WorkerTask::Idle => {
                // If carrying resources, find depot to deposit
                if carrying.amount > 0 {
                    if let Some(depot) = find_nearest_deposit(&tf.translation, &deposit_points) {
                        let depot_pos = deposit_points.get(depot).unwrap().1.translation;
                        commands.entity(entity).insert(MoveTarget(depot_pos));
                        *task = WorkerTask::ReturningToDeposit { depot, gather_node: None };
                    }
                    continue;
                }
                // Scan for nearby resource to auto-gather
                let mut closest_dist = f32::MAX;
                let mut closest_node = None;
                for (node_entity, node_tf) in &all_nodes {
                    let dist = tf.translation.distance(node_tf.translation);
                    if dist < auto_scan_range && dist < closest_dist {
                        closest_dist = dist;
                        closest_node = Some(node_entity);
                    }
                }
                if let Some(node) = closest_node {
                    *task = WorkerTask::MovingToResource(node);
                } else {
                    // No nearby resource — check for nearby construction sites
                    let auto_build_range = 10.0;
                    let mut closest_site = None;
                    let mut closest_site_dist = f32::MAX;
                    for (site_entity, site_tf, site_state) in &construction_sites {
                        if *site_state != BuildingState::UnderConstruction {
                            continue;
                        }
                        let dist = tf.translation.distance(site_tf.translation);
                        if dist < auto_build_range && dist < closest_site_dist {
                            closest_site_dist = dist;
                            closest_site = Some(site_entity);
                        }
                    }
                    if let Some(site) = closest_site {
                        *task = WorkerTask::MovingToBuild(site);
                    }
                }
            }

            WorkerTask::MovingToResource(node) => {
                // Check node still exists and has resources
                let Ok((node_tf, node_data)) = nodes.get(node) else {
                    commands.entity(entity).remove::<MoveTarget>();
                    *task = WorkerTask::Idle;
                    continue;
                };
                if node_data.amount_remaining == 0 {
                    commands.entity(entity).remove::<MoveTarget>();
                    *task = WorkerTask::Idle;
                    continue;
                }

                let dist = tf.translation.distance(node_tf.translation);
                if dist <= gather_range {
                    commands.entity(entity).remove::<MoveTarget>();
                    *task = WorkerTask::Gathering(node);
                } else {
                    // Ensure we have a MoveTarget to the node
                    commands.entity(entity).insert(MoveTarget(node_tf.translation));
                }
            }

            WorkerTask::Gathering(node) => {
                let Ok((node_tf, mut node_data)) = nodes.get_mut(node) else {
                    // Node gone
                    commands.entity(entity).remove::<MoveTarget>();
                    if carrying.amount > 0 {
                        if let Some(depot) = find_nearest_deposit(&tf.translation, &deposit_points) {
                            let depot_pos = deposit_points.get(depot).unwrap().1.translation;
                            commands.entity(entity).insert(MoveTarget(depot_pos));
                            *task = WorkerTask::ReturningToDeposit { depot, gather_node: None };
                        } else {
                            *task = WorkerTask::Idle;
                        }
                    } else {
                        *task = WorkerTask::Idle;
                    }
                    continue;
                };

                if node_data.amount_remaining == 0 {
                    if carrying.amount > 0 {
                        if let Some(depot) = find_nearest_deposit(&tf.translation, &deposit_points) {
                            let depot_pos = deposit_points.get(depot).unwrap().1.translation;
                            commands.entity(entity).insert(MoveTarget(depot_pos));
                            *task = WorkerTask::ReturningToDeposit { depot, gather_node: None };
                        } else {
                            *task = WorkerTask::Idle;
                        }
                    } else {
                        *task = WorkerTask::Idle;
                    }
                    continue;
                }

                // If pushed away by avoidance, go back
                let dist = tf.translation.distance(node_tf.translation);
                if dist > gather_range {
                    *task = WorkerTask::MovingToResource(node);
                    continue;
                }

                // Remove any stale MoveTarget
                commands.entity(entity).remove::<MoveTarget>();

                // Gather tick
                let rt = node_data.resource_type;
                let unit_weight = rt.weight();
                let amount = (speed.0 * time.delta_secs()) as u32;
                let amount = amount.max(1).min(node_data.amount_remaining);

                let new_weight = carrying.weight + amount as f32 * unit_weight;
                if new_weight > capacity.0 {
                    // Fill remaining capacity
                    let remaining_capacity = capacity.0 - carrying.weight;
                    let can_carry = (remaining_capacity / unit_weight).floor() as u32;
                    if can_carry > 0 {
                        let actual = can_carry.min(node_data.amount_remaining);
                        node_data.amount_remaining -= actual;
                        carrying.amount += actual;
                        carrying.weight += actual as f32 * unit_weight;
                        carrying.resource_type = Some(rt);
                    }

                    // Full — go deposit
                    if let Some(depot) = find_nearest_deposit(&tf.translation, &deposit_points) {
                        let depot_pos = deposit_points.get(depot).unwrap().1.translation;
                        commands.entity(entity).insert(MoveTarget(depot_pos));
                        *task = WorkerTask::ReturningToDeposit { depot, gather_node: Some(node) };
                    }
                } else {
                    node_data.amount_remaining -= amount;
                    carrying.amount += amount;
                    carrying.weight += amount as f32 * unit_weight;
                    carrying.resource_type = Some(rt);
                }
            }

            WorkerTask::ReturningToDeposit { depot, gather_node } => {
                // Check depot still exists
                let Ok((depot_tf, _)) = inventories.get(depot) else {
                    // Try find another depot
                    commands.entity(entity).remove::<MoveTarget>();
                    if let Some(new_depot) = find_nearest_deposit(&tf.translation, &deposit_points) {
                        let depot_pos = deposit_points.get(new_depot).unwrap().1.translation;
                        commands.entity(entity).insert(MoveTarget(depot_pos));
                        *task = WorkerTask::ReturningToDeposit { depot: new_depot, gather_node };
                    } else {
                        *task = WorkerTask::Idle;
                    }
                    continue;
                };

                let dist = tf.translation.distance(depot_tf.translation);
                if dist <= deposit_range {
                    *task = WorkerTask::Depositing { depot, gather_node };
                } else {
                    // Ensure MoveTarget
                    commands.entity(entity).insert(MoveTarget(depot_tf.translation));
                }
            }

            WorkerTask::Depositing { depot, gather_node } => {
                // Transfer resources
                if let Some(rt) = carrying.resource_type {
                    player_res.add(rt, carrying.amount);

                    // Track in storage inventory
                    if let Ok((_, inventory)) = inventories.get_mut(depot) {
                        if let Some(mut inv) = inventory {
                            match rt {
                                ResourceType::Wood => inv.wood += carrying.amount,
                                ResourceType::Copper => inv.copper += carrying.amount,
                                ResourceType::Iron => inv.iron += carrying.amount,
                                ResourceType::Gold => inv.gold += carrying.amount,
                                ResourceType::Oil => inv.oil += carrying.amount,
                            }
                        }
                    }
                }

                // Spawn deposit VFX
                if let Some(ref vfx) = vfx_assets {
                    if let Ok((depot_tf, _)) = inventories.get(depot) {
                        let deposit_pos = depot_tf.translation + Vec3::Y * 2.0;
                        for i in 0..4 {
                            let angle = std::f32::consts::TAU * (i as f32 / 4.0);
                            let offset = Vec3::new(angle.cos() * 0.5, 0.5, angle.sin() * 0.5);
                            commands.spawn((
                                VfxFlash {
                                    timer: Timer::from_seconds(0.3, TimerMode::Once),
                                    start_scale: 0.15,
                                    end_scale: 0.0,
                                },
                                Mesh3d(vfx.sphere_mesh.clone()),
                                MeshMaterial3d(vfx.deposit_material.clone()),
                                Transform::from_translation(deposit_pos + offset)
                                    .with_scale(Vec3::splat(0.15)),
                            ));
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
                            *task = WorkerTask::MovingToResource(gn);
                            continue;
                        }
                    }
                }
                *task = WorkerTask::Idle;
            }

            WorkerTask::MovingToBuild(building) => {
                // Check building still exists and is under construction
                let Ok((_, build_tf, build_state)) = construction_sites.get(building) else {
                    commands.entity(entity).remove::<MoveTarget>();
                    *task = WorkerTask::Idle;
                    continue;
                };
                if *build_state != BuildingState::UnderConstruction {
                    commands.entity(entity).remove::<MoveTarget>();
                    *task = WorkerTask::Idle;
                    continue;
                }

                let dist = tf.translation.distance(build_tf.translation);
                if dist <= 4.0 {
                    commands.entity(entity).remove::<MoveTarget>();
                    *task = WorkerTask::Building(building);
                } else {
                    commands.entity(entity).insert(MoveTarget(build_tf.translation));
                }
            }

            WorkerTask::Building(building) => {
                let Ok((_, build_tf, build_state)) = construction_sites.get(building) else {
                    commands.entity(entity).remove::<MoveTarget>();
                    *task = WorkerTask::Idle;
                    continue;
                };
                if *build_state != BuildingState::UnderConstruction {
                    commands.entity(entity).remove::<MoveTarget>();
                    *task = WorkerTask::Idle;
                    continue;
                }

                let dist = tf.translation.distance(build_tf.translation);
                if dist > 4.0 {
                    *task = WorkerTask::MovingToBuild(building);
                } else {
                    commands.entity(entity).remove::<MoveTarget>();
                }
            }
        }
    }
}

fn find_nearest_deposit(
    pos: &Vec3,
    deposit_points: &Query<(Entity, &Transform, &BuildingState), (With<DepositPoint>, Without<Unit>)>,
) -> Option<Entity> {
    let mut closest_dist = f32::MAX;
    let mut closest = None;
    for (entity, tf, state) in deposit_points {
        if *state != BuildingState::Complete {
            continue;
        }
        let dist = pos.distance(tf.translation);
        if dist < closest_dist {
            closest_dist = dist;
            closest = Some(entity);
        }
    }
    closest
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

fn deplete_resource_nodes(mut commands: Commands, nodes: Query<(Entity, &ResourceNode)>) {
    for (entity, node) in &nodes {
        if node.amount_remaining == 0 {
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

        // Don't spawn too close to center
        if (x * x + z * z).sqrt() < 20.0 {
            continue;
        }

        let scene_handle = model_assets.trees[rng.random_range(0..model_assets.trees.len())].clone();
        let y_rotation = rng.random_range(0.0..std::f32::consts::TAU);
        let target_scale = rng.random_range(0.8_f32..1.2);
        let initial_scale = 0.15;

        commands.spawn((
            Sapling {
                timer: Timer::from_seconds(config.sapling_duration, TimerMode::Once),
                target_scale,
            },
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
