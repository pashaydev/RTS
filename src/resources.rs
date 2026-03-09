use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::blueprints::EntityKind;
use crate::components::*;
use crate::ground::{HeightMap, MAP_SIZE};
use rand::Rng;

pub struct ResourcesPlugin;

impl Plugin for ResourcesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TreeGrowthConfig>()
            .init_resource::<CarriedResourceTotals>()
            .init_resource::<PendingCarriedDrains>()
            .add_systems(Startup, (create_resource_node_materials, create_carry_visual_assets))
            .add_systems(PostStartup, (spawn_resource_nodes, spawn_decorations))
            .add_systems(
                Update,
                (compute_carried_totals, worker_ai_system, resource_processor_system, deplete_resource_nodes).chain(),
            )
            .add_systems(
                Update,
                (drain_carried_from_workers, update_carry_visuals).chain()
                    .after(deplete_resource_nodes),
            )
            .add_systems(
                Update,
                (spawn_saplings_system, grow_saplings_system, grow_trees_system),
            )
            .add_systems(
                Update,
                (processor_worker_visual_system, resource_respawn_system, grow_resource_system),
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
            // Keep starting areas clear (all faction spawn positions)
            let mut too_close_to_spawn = false;
            for &(_, (sx, sz)) in &SPAWN_POSITIONS {
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

            // Keep starting areas clear (all faction spawn positions)
            let mut too_close_to_spawn = false;
            for &(_, (sx, sz)) in &SPAWN_POSITIONS {
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
    mut all_resources: ResMut<AllPlayerResources>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut workers: Query<
        (Entity, &Transform, &mut WorkerTask, &mut Carrying, &GatherSpeed, &CarryCapacity, &EntityKind, &Faction, Option<&MoveTarget>),
        With<Unit>,
    >,
    mut nodes: Query<(&Transform, &mut ResourceNode), Without<Unit>>,
    deposit_points: Query<(Entity, &Transform, &BuildingState, &Faction), (With<DepositPoint>, Without<Unit>)>,
    mut inventories: Query<(&Transform, Option<&mut StorageInventory>), (With<DepositPoint>, Without<Unit>)>,
    all_nodes: Query<(Entity, &Transform), (With<ResourceNode>, Without<Unit>)>,
    construction_sites: Query<(Entity, &Transform, &BuildingState, &Faction), (With<Building>, Without<Unit>, Without<ResourceNode>)>,
    storage_auras: Query<(&Transform, &StorageAura, &BuildingState), With<Building>>,
) {
    let gather_range = 3.0;
    let deposit_range = 4.0;
    let auto_scan_range = 20.0;

    for (entity, tf, mut task, mut carrying, speed, capacity, kind, worker_faction, move_target) in &mut workers {
        if *kind != EntityKind::Worker {
            continue;
        }

        match *task {
            WorkerTask::ManualMove => {
                // Player issued a manual move — only transition to Idle once arrived
                if move_target.is_none() {
                    *task = WorkerTask::Idle;
                }
                continue;
            }
            WorkerTask::Idle => {
                // If carrying resources, find depot to deposit
                if carrying.amount > 0 {
                    if let Some(depot) = find_nearest_deposit(&tf.translation, worker_faction, &deposit_points) {
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
                    // No nearby resource — check for nearby construction sites (same faction)
                    let auto_build_range = 10.0;
                    let mut closest_site = None;
                    let mut closest_site_dist = f32::MAX;
                    for (site_entity, site_tf, site_state, site_faction) in &construction_sites {
                        if *site_state != BuildingState::UnderConstruction || site_faction != worker_faction {
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
                        if let Some(depot) = find_nearest_deposit(&tf.translation, worker_faction, &deposit_points) {
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
                        if let Some(depot) = find_nearest_deposit(&tf.translation, worker_faction, &deposit_points) {
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

                // Gather tick (with storage aura bonus)
                let rt = node_data.resource_type;
                let unit_weight = rt.weight();
                let aura_bonus = crate::buildings::storage_aura_bonus(tf.translation, &storage_auras);
                let effective_speed = speed.0 * (1.0 + aura_bonus);
                let amount = (effective_speed * time.delta_secs()) as u32;
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
                    if let Some(depot) = find_nearest_deposit(&tf.translation, worker_faction, &deposit_points) {
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
                    if let Some(new_depot) = find_nearest_deposit(&tf.translation, worker_faction, &deposit_points) {
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
                // Transfer resources (capacity-limited)
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
                        commands.entity(entity).remove::<MoveTarget>();
                        *task = WorkerTask::WaitingForStorage { depot, gather_node };
                        continue;
                    }

                    // Add deposited amount to global resources
                    all_resources.get_mut(worker_faction).add(rt, deposited);

                    // Update carrying with leftover
                    let leftover = carrying.amount - deposited;
                    if leftover > 0 {
                        carrying.amount = leftover;
                        carrying.weight = leftover as f32 * rt.weight();
                        // Worker still has resources — wait for capacity
                        commands.entity(entity).remove::<MoveTarget>();
                        *task = WorkerTask::WaitingForStorage { depot, gather_node };

                        // Still spawn VFX for partial deposit
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
                                        FogHideable::Vfx,
                                        Mesh3d(vfx.sphere_mesh.clone()),
                                        MeshMaterial3d(vfx.deposit_material.clone()),
                                        Transform::from_translation(deposit_pos + offset)
                                            .with_scale(Vec3::splat(0.15)),
                                    ));
                                }
                            }
                        }
                        continue;
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
                                FogHideable::Vfx,
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
                // No gather node or depleted — scan broadly for next resource
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
                    *task = WorkerTask::Idle;
                }
            }

            WorkerTask::WaitingForStorage { depot, gather_node } => {
                // Periodically check if depot has capacity again
                let has_space = if let Ok((_, inventory)) = inventories.get(depot) {
                    inventory.map_or(true, |inv| inv.remaining_capacity() > 0)
                } else {
                    false
                };

                if has_space {
                    *task = WorkerTask::Depositing { depot, gather_node };
                    continue;
                }

                // Try a different depot that has space
                let mut best_depot = None;
                let mut best_dist = f32::MAX;
                for (dp_entity, dp_tf, dp_state, dp_faction) in &deposit_points {
                    if dp_faction != worker_faction || *dp_state != BuildingState::Complete {
                        continue;
                    }
                    // Check this depot has capacity
                    if let Ok((_, inv_opt)) = inventories.get(dp_entity) {
                        if let Some(inv) = inv_opt {
                            if inv.remaining_capacity() == 0 {
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
                    *task = WorkerTask::ReturningToDeposit { depot: new_depot, gather_node };
                }
                // Otherwise keep waiting at current depot
            }

            WorkerTask::MovingToBuild(building) => {
                // Check building still exists and is under construction
                let Ok((_, build_tf, build_state, _)) = construction_sites.get(building) else {
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
                let Ok((_, build_tf, build_state, _)) = construction_sites.get(building) else {
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

            WorkerTask::AssignedToBuilding(_) => {
                // Handled by processor_worker_visual_system — skip
                continue;
            }
        }
    }
}

/// Resource processing buildings auto-harvest nearby nodes and deposit into player resources.
fn resource_processor_system(
    time: Res<Time>,
    mut all_resources: ResMut<AllPlayerResources>,
    mut processors: Query<
        (Entity, &Transform, &mut ResourceProcessor, &BuildingState, &Faction, Option<&mut StorageInventory>),
        With<Building>,
    >,
    mut nodes: Query<(&Transform, &mut ResourceNode), Without<Building>>,
    assigned_workers: Query<&AssignedToProcessor>,
) {
    for (building_entity, building_tf, mut processor, state, faction, storage) in &mut processors {
        if *state != BuildingState::Complete {
            continue;
        }

        // Count assigned workers for this building
        let worker_count = assigned_workers
            .iter()
            .filter(|a| a.0 == building_entity)
            .count() as f32;

        // Effective rate = base_rate + (worker_count * base_rate * worker_rate_bonus)
        let effective_rate = processor.harvest_rate + (worker_count * processor.harvest_rate * processor.worker_rate_bonus);
        let rate = effective_rate * time.delta_secs();
        let amount = rate as u32;
        if amount == 0 && (rate * 10.0) as u32 == 0 {
            processor.buffer += if rand::random::<f32>() < rate { 1 } else { 0 };
        } else {
            processor.buffer += amount.max(1);
        }

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

        // Transfer harvested resources to player
        if let Some((rt, amount)) = harvested_type {
            if let Some(mut inv) = storage {
                let stored = inv.add_capped(rt, amount);
                if stored > 0 {
                    all_resources.get_mut(faction).add(rt, stored);
                }
                if amount > stored {
                    processor.buffer += amount - stored;
                }
            } else {
                all_resources.get_mut(faction).add(rt, amount);
            }
        }
    }
}

fn find_nearest_deposit(
    pos: &Vec3,
    faction: &Faction,
    deposit_points: &Query<(Entity, &Transform, &BuildingState, &Faction), (With<DepositPoint>, Without<Unit>)>,
) -> Option<Entity> {
    let mut closest_dist = f32::MAX;
    let mut closest = None;
    for (entity, tf, state, depot_faction) in deposit_points {
        if *state != BuildingState::Complete || depot_faction != faction {
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

/// Recompute per-faction totals of resources carried by workers each frame.
fn compute_carried_totals(
    workers: Query<(&Carrying, &Faction), With<Unit>>,
    mut totals: ResMut<CarriedResourceTotals>,
) {
    totals.per_faction.clear();
    for (carrying, faction) in &workers {
        if let Some(rt) = carrying.resource_type {
            if carrying.amount > 0 {
                totals.per_faction
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
            let Some(rt) = carrying.resource_type else { continue };
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

        // Don't spawn too close to any player base
        let mut near_base = false;
        for &(_, (sx, sz)) in &SPAWN_POSITIONS {
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

        let scene_handle = model_assets.trees[rng.random_range(0..model_assets.trees.len())].clone();
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

/// Drives the ProcessorWorkerState state machine for workers assigned to processor buildings.
fn processor_worker_visual_system(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut workers: Query<
        (Entity, &Transform, &mut ProcessorWorkerState, &AssignedToProcessor, &mut WorkerTask, &mut Carrying),
        With<Unit>,
    >,
    processors: Query<(Entity, &Transform, &ResourceProcessor, &BuildingState), With<Building>>,
    nodes: Query<(Entity, &Transform, &ResourceNode), Without<Unit>>,
) {
    // Collect nodes targeted by other workers to avoid clustering
    let mut targeted_nodes: Vec<Entity> = Vec::new();
    for (_, _, state, _, _, _) in workers.iter() {
        if let ProcessorWorkerState::MovingToNode(node) | ProcessorWorkerState::Harvesting { node, .. } = *state {
            targeted_nodes.push(node);
        }
    }

    for (entity, tf, mut worker_state, assigned, mut task, mut carrying) in &mut workers {
        let building_entity = assigned.0;
        let Ok((_, building_tf, processor, building_state)) = processors.get(building_entity) else {
            // Building gone — unassign
            commands.entity(entity)
                .remove::<AssignedToProcessor>()
                .remove::<ProcessorWorkerState>();
            *task = WorkerTask::Idle;
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
                    // Prefer nodes not already targeted
                    let already_targeted = targeted_nodes.iter().filter(|&&n| n == node_entity).count();
                    if already_targeted >= 2 {
                        continue; // max 2 workers per node
                    }
                    let dist = tf.translation.distance(node_tf.translation);
                    if best.is_none() || dist < best.unwrap().1 {
                        best = Some((node_entity, dist));
                    }
                }
                if let Some((node, _)) = best {
                    *worker_state = ProcessorWorkerState::MovingToNode(node);
                    if let Ok((_, node_tf, _)) = nodes.get(node) {
                        commands.entity(entity).insert(MoveTarget(node_tf.translation));
                    }
                }
            }
            ProcessorWorkerState::MovingToNode(node) => {
                let Ok((_, node_tf, node_data)) = nodes.get(node) else {
                    commands.entity(entity).remove::<MoveTarget>();
                    *worker_state = ProcessorWorkerState::Idle;
                    continue;
                };
                if node_data.amount_remaining == 0 {
                    commands.entity(entity).remove::<MoveTarget>();
                    *worker_state = ProcessorWorkerState::Idle;
                    continue;
                }
                let dist = tf.translation.distance(node_tf.translation);
                if dist <= 2.5 {
                    commands.entity(entity).remove::<MoveTarget>();
                    *worker_state = ProcessorWorkerState::Harvesting { node, timer_secs: 0.0 };
                }
            }
            ProcessorWorkerState::Harvesting { node, ref mut timer_secs } => {
                // Check node still valid
                if nodes.get(node).is_err() || nodes.get(node).map(|(_, _, n)| n.amount_remaining == 0).unwrap_or(true) {
                    *worker_state = ProcessorWorkerState::Idle;
                    continue;
                }
                *timer_secs += time.delta_secs();
                if *timer_secs >= 2.5 {
                    // Done "harvesting" — pretend to carry resource back
                    if let Ok((_, _, node_data)) = nodes.get(node) {
                        carrying.resource_type = Some(node_data.resource_type);
                        carrying.amount = 1; // Visual only
                        carrying.weight = 1.0;
                    }
                    *worker_state = ProcessorWorkerState::ReturningToBuilding;
                    commands.entity(entity).insert(MoveTarget(building_tf.translation));
                }
            }
            ProcessorWorkerState::ReturningToBuilding => {
                let dist = tf.translation.distance(building_tf.translation);
                if dist <= 3.0 {
                    commands.entity(entity).remove::<MoveTarget>();
                    *worker_state = ProcessorWorkerState::Depositing { timer_secs: 0.0 };
                }
            }
            ProcessorWorkerState::Depositing { ref mut timer_secs } => {
                *timer_secs += time.delta_secs();
                if *timer_secs >= 0.5 {
                    // Clear visual carry
                    carrying.amount = 0;
                    carrying.weight = 0.0;
                    carrying.resource_type = None;

                    // Deposit VFX
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

                    // Loop back
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
    mut buildings: Query<
        (&Transform, &mut ResourceRespawnConfig, &BuildingState),
        With<Building>,
    >,
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
                        let scene_handle = model_assets.trees[rng.random_range(0..model_assets.trees.len())].clone();
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
                    if matches!(rt, ResourceType::Copper | ResourceType::Iron | ResourceType::Gold) && !model_assets.rocks.is_empty() {
                        let scene_handle = model_assets.rocks[rng.random_range(0..model_assets.rocks.len())].clone();
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

/// Assign a worker to a processor building.
pub fn assign_worker_to_processor(commands: &mut Commands, worker: Entity, building: Entity) {
    commands.entity(worker).insert((
        AssignedToProcessor(building),
        ProcessorWorkerState::Idle,
        WorkerTask::AssignedToBuilding(building),
    ));
}

/// Unassign a worker from a processor building.
pub fn unassign_worker_from_processor(commands: &mut Commands, worker: Entity) {
    commands.entity(worker)
        .remove::<AssignedToProcessor>()
        .remove::<ProcessorWorkerState>();
    commands.entity(worker).insert(WorkerTask::Idle);
}
