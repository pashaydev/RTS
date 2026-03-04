use bevy::prelude::*;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::components::*;
use crate::ground::{terrain_height, MAP_SIZE};

pub struct ResourcesPlugin;

impl Plugin for ResourcesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerResources>()
            .add_systems(Startup, create_resource_node_materials)
            .add_systems(PostStartup, spawn_resource_nodes)
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

fn spawn_resource_nodes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    node_mats: Res<ResourceNodeMaterials>,
    biome_map: Res<BiomeMap>,
) {
    let wood_mesh = meshes.add(Cuboid::new(0.6, 2.5, 0.6));
    let ore_mesh = meshes.add(Cuboid::new(1.0, 0.8, 1.0));
    let gold_mesh = meshes.add(Cuboid::new(0.8, 0.8, 0.8));
    let oil_mesh = meshes.add(Cylinder::new(0.5, 1.2));

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
