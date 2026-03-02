use bevy::prelude::*;

use crate::components::*;
use crate::ground::terrain_height;

pub struct ResourcesPlugin;

impl Plugin for ResourcesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerResources>()
            .add_systems(
                Startup,
                (create_resource_node_materials, spawn_resource_nodes).chain(),
            )
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

fn spawn_resource_nodes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    node_mats: Res<ResourceNodeMaterials>,
) {
    let wood_mesh = meshes.add(Cuboid::new(0.6, 2.5, 0.6));
    let ore_mesh = meshes.add(Cuboid::new(1.0, 0.8, 1.0));
    let gold_mesh = meshes.add(Cuboid::new(0.8, 0.8, 0.8));
    let oil_mesh = meshes.add(Cylinder::new(0.5, 1.2));

    // Helper: spawn a resource node at (x, z) with correct terrain Y
    let spawn_node = |commands: &mut Commands,
                          x: f32,
                          z: f32,
                          half_h: f32,
                          rt: ResourceType,
                          amount: u32,
                          mesh: Handle<Mesh>,
                          mat: Handle<StandardMaterial>| {
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
    };

    // Wood — 6 scattered trees (x10 positions)
    let wood_xz: [(f32, f32); 6] = [
        (60.0, 40.0),
        (-50.0, 70.0),
        (80.0, -30.0),
        (-90.0, -60.0),
        (30.0, 120.0),
        (-40.0, -110.0),
    ];
    for (x, z) in wood_xz {
        spawn_node(&mut commands, x, z, 1.25, ResourceType::Wood, 300, wood_mesh.clone(), node_mats.wood.clone());
    }

    // Copper — 4 clustered (x10 positions)
    let copper_xz: [(f32, f32); 4] = [
        (140.0, 100.0),
        (155.0, 110.0),
        (135.0, 120.0),
        (150.0, 90.0),
    ];
    for (x, z) in copper_xz {
        spawn_node(&mut commands, x, z, 0.4, ResourceType::Copper, 400, ore_mesh.clone(), node_mats.copper.clone());
    }

    // Iron — 4 clustered (x10 positions)
    let iron_xz: [(f32, f32); 4] = [
        (-130.0, -100.0),
        (-145.0, -110.0),
        (-125.0, -120.0),
        (-140.0, -90.0),
    ];
    for (x, z) in iron_xz {
        spawn_node(&mut commands, x, z, 0.4, ResourceType::Iron, 500, ore_mesh.clone(), node_mats.iron.clone());
    }

    // Gold — 2 far from center (x10 positions)
    let gold_xz: [(f32, f32); 2] = [
        (180.0, -160.0),
        (-180.0, 160.0),
    ];
    for (x, z) in gold_xz {
        spawn_node(&mut commands, x, z, 0.4, ResourceType::Gold, 600, gold_mesh.clone(), node_mats.gold.clone());
    }

    // Oil — 2 farthest (x10 positions)
    let oil_xz: [(f32, f32); 2] = [
        (200.0, 190.0),
        (-200.0, -190.0),
    ];
    for (x, z) in oil_xz {
        spawn_node(&mut commands, x, z, 0.6, ResourceType::Oil, 800, oil_mesh.clone(), node_mats.oil.clone());
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
        // Only workers auto-gather
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
    mut units: Query<
        (Entity, &GatherTarget, &mut Carrying, &GatherSpeed),
        With<Unit>,
    >,
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

        // Gather resources
        let amount = (speed.0 * time.delta_secs()) as u32;
        let amount = amount.max(1).min(node.amount_remaining);
        node.amount_remaining -= amount;
        carrying.amount += amount;
        carrying.resource_type = Some(rt);

        // Auto-deposit when full
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
