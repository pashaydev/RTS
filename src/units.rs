use bevy::prelude::*;

use crate::components::*;
use crate::ground::terrain_height;

pub struct UnitsPlugin;

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (create_unit_materials, create_unit_meshes, spawn_units).chain(),
        )
        .add_systems(
            Update,
            (steer_avoidance, move_units, update_unit_visuals).chain(),
        );
    }
}

fn create_unit_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(UnitMaterials {
        // Worker — yellow
        worker_default: materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.8, 0.2),
            ..default()
        }),
        worker_selected: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 1.0, 0.4),
            emissive: LinearRgba::new(0.3, 0.3, 0.0, 1.0),
            ..default()
        }),
        // Soldier — red
        soldier_default: materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.15, 0.15),
            ..default()
        }),
        soldier_selected: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.3, 0.3),
            emissive: LinearRgba::new(0.3, 0.05, 0.05, 1.0),
            ..default()
        }),
        // Archer — green
        archer_default: materials.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.7, 0.2),
            ..default()
        }),
        archer_selected: materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 1.0, 0.4),
            emissive: LinearRgba::new(0.05, 0.3, 0.05, 1.0),
            ..default()
        }),
        // Tank — dark gray
        tank_default: materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.35, 0.4),
            ..default()
        }),
        tank_selected: materials.add(StandardMaterial {
            base_color: Color::srgb(0.6, 0.6, 0.65),
            emissive: LinearRgba::new(0.1, 0.1, 0.12, 1.0),
            ..default()
        }),
    });
}

fn create_unit_meshes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(UnitMeshes {
        worker: meshes.add(Capsule3d::new(0.3, 1.0)),
        soldier: meshes.add(Capsule3d::new(0.35, 1.2)),
        archer: meshes.add(Capsule3d::new(0.25, 1.0)),
        tank: meshes.add(Capsule3d::new(0.5, 1.5)),
    });
}

pub fn y_offset(ut: UnitType) -> f32 {
    match ut {
        UnitType::Worker => 0.8,
        UnitType::Soldier => 0.9,
        UnitType::Archer => 0.75,
        UnitType::Tank => 1.25,
    }
}

fn speed_for(ut: UnitType) -> f32 {
    match ut {
        UnitType::Worker => 5.0,
        UnitType::Soldier => 4.5,
        UnitType::Archer => 5.5,
        UnitType::Tank => 3.0,
    }
}

fn damage_for(ut: UnitType) -> f32 {
    match ut {
        UnitType::Worker => 3.0,
        UnitType::Soldier => 12.0,
        UnitType::Archer => 8.0,
        UnitType::Tank => 18.0,
    }
}

fn range_for(ut: UnitType) -> f32 {
    match ut {
        UnitType::Worker => 1.5,
        UnitType::Soldier => 2.0,
        UnitType::Archer => 12.0,
        UnitType::Tank => 2.5,
    }
}

fn cooldown_for(ut: UnitType) -> f32 {
    match ut {
        UnitType::Worker => 1.5,
        UnitType::Soldier => 1.0,
        UnitType::Archer => 1.5,
        UnitType::Tank => 2.0,
    }
}

fn vision_range_for(ut: UnitType) -> f32 {
    match ut {
        UnitType::Worker => 15.0,
        UnitType::Soldier => 12.0,
        UnitType::Archer => 18.0,
        UnitType::Tank => 10.0,
    }
}

pub fn spawn_unit_of_type(
    commands: &mut Commands,
    unit_mats: &UnitMaterials,
    unit_meshes: &UnitMeshes,
    ut: UnitType,
    pos: Vec3,
) -> Entity {
    let y = terrain_height(pos.x, pos.z) + y_offset(ut);
    commands
        .spawn((
            Unit,
            ut,
            Faction::Player,
            Health::default(),
            Carrying::default(),
            GatherSpeed(5.0),
            UnitSpeed(speed_for(ut)),
            AttackDamage(damage_for(ut)),
            AttackRange(range_for(ut)),
            AttackCooldown {
                timer: Timer::from_seconds(cooldown_for(ut), TimerMode::Repeating),
            },
            VisionRange(vision_range_for(ut)),
            Mesh3d(unit_meshes.mesh_for(ut)),
            MeshMaterial3d(unit_mats.default_for(ut)),
            Transform::from_translation(Vec3::new(pos.x, y, pos.z)),
        ))
        .id()
}

fn spawn_units(
    mut commands: Commands,
    unit_mats: Res<UnitMaterials>,
    unit_meshes: Res<UnitMeshes>,
) {
    // 2 Workers near center
    let worker_positions = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(2.0, 0.0, 1.0),
    ];
    for pos in worker_positions {
        spawn_unit_of_type(&mut commands, &unit_mats, &unit_meshes, UnitType::Worker, pos);
    }
}

fn steer_avoidance(
    time: Res<Time>,
    mut units: Query<(Entity, &mut Transform), With<Unit>>,
) {
    let avoidance_radius = 1.8;
    let strength = 4.0;

    // Collect positions first to avoid borrow conflict
    let positions: Vec<(Entity, Vec3)> = units
        .iter()
        .map(|(e, t)| (e, t.translation))
        .collect();

    for (entity, mut transform) in &mut units {
        let my_pos = transform.translation;
        let mut separation = Vec3::ZERO;

        for (other_e, other_pos) in &positions {
            if *other_e == entity {
                continue;
            }
            let diff = my_pos - *other_pos;
            let flat_diff = Vec3::new(diff.x, 0.0, diff.z);
            let dist = flat_diff.length();
            if dist > 0.01 && dist < avoidance_radius {
                separation += flat_diff.normalize() * (avoidance_radius - dist) / avoidance_radius;
            }
        }

        if separation.length_squared() > 0.0 {
            transform.translation += separation * strength * time.delta_secs();
        }
    }
}

fn move_units(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Transform, &MoveTarget, &UnitSpeed, &UnitType), With<Unit>>,
) {
    for (entity, mut transform, target, unit_speed, ut) in &mut query {
        let direction = target.0 - transform.translation;
        let flat_dir = Vec3::new(direction.x, 0.0, direction.z);
        let distance = flat_dir.length();

        if distance < 0.2 {
            commands.entity(entity).remove::<MoveTarget>();
        } else {
            let step = flat_dir.normalize() * unit_speed.0 * time.delta_secs();
            transform.translation += step;
        }
        // Snap Y to terrain
        transform.translation.y = terrain_height(transform.translation.x, transform.translation.z) + y_offset(*ut);
    }
}

fn update_unit_visuals(
    mut commands: Commands,
    unit_mats: Res<UnitMaterials>,
    added: Query<(Entity, &UnitType), (With<Unit>, Added<Selected>)>,
    mut removed: RemovedComponents<Selected>,
    units: Query<(Entity, &UnitType), With<Unit>>,
) {
    // Units that just became selected
    for (entity, ut) in &added {
        commands
            .entity(entity)
            .insert(MeshMaterial3d(unit_mats.selected_for(*ut)));
    }

    // Units that lost Selected
    for entity in removed.read() {
        if let Ok((_, ut)) = units.get(entity) {
            commands
                .entity(entity)
                .insert(MeshMaterial3d(unit_mats.default_for(*ut)));
        }
    }
}
