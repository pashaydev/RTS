use bevy::prelude::*;

use crate::components::*;
use crate::ground::terrain_height;

pub struct MobsPlugin;

impl Plugin for MobsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (create_mob_assets, spawn_mob_camps).chain(),
        )
        .add_systems(
            Update,
            (mob_patrol, mob_aggro, mob_chase, mob_return).chain(),
        );
    }
}

fn create_mob_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(MobMaterials {
        goblin: materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.6, 0.15),
            ..default()
        }),
        skeleton: materials.add(StandardMaterial {
            base_color: Color::srgb(0.85, 0.82, 0.75),
            ..default()
        }),
        orc: materials.add(StandardMaterial {
            base_color: Color::srgb(0.4, 0.3, 0.15),
            ..default()
        }),
        demon: materials.add(StandardMaterial {
            base_color: Color::srgb(0.6, 0.1, 0.1),
            emissive: LinearRgba::new(0.4, 0.05, 0.05, 1.0),
            ..default()
        }),
    });

    commands.insert_resource(MobMeshes {
        goblin: meshes.add(Capsule3d::new(0.25, 0.8)),
        skeleton: meshes.add(Capsule3d::new(0.28, 1.0)),
        orc: meshes.add(Capsule3d::new(0.4, 1.3)),
        demon: meshes.add(Capsule3d::new(0.45, 1.4)),
    });
}

struct CampDef {
    mob_type: MobType,
    center: Vec3,
    patrol_radius: f32,
    count: usize,
    has_boss: bool,
    hp: f32,
    boss_hp: f32,
    damage: f32,
    speed: f32,
    aggro: f32,
    attack_range: f32,
}

fn mob_y_offset(mt: MobType) -> f32 {
    match mt {
        MobType::Goblin => 0.65,
        MobType::Skeleton => 0.78,
        MobType::Orc => 1.05,
        MobType::Demon => 1.15,
    }
}

fn spawn_mob_camps(
    mut commands: Commands,
    mob_mats: Res<MobMaterials>,
    mob_meshes: Res<MobMeshes>,
) {
    let camps = [
        CampDef {
            mob_type: MobType::Goblin,
            center: Vec3::new(50.0, 0.0, 0.0),
            patrol_radius: 12.0,
            count: 5,
            has_boss: false,
            hp: 50.0,
            boss_hp: 0.0,
            damage: 5.0,
            speed: 3.5,
            aggro: 15.0,
            attack_range: 1.5,
        },
        CampDef {
            mob_type: MobType::Skeleton,
            center: Vec3::new(-40.0, 0.0, 100.0),
            patrol_radius: 15.0,
            count: 5,
            has_boss: true,
            hp: 80.0,
            boss_hp: 200.0,
            damage: 10.0,
            speed: 3.0,
            aggro: 18.0,
            attack_range: 1.8,
        },
        CampDef {
            mob_type: MobType::Orc,
            center: Vec3::new(80.0, 0.0, -150.0),
            patrol_radius: 18.0,
            count: 6,
            has_boss: true,
            hp: 120.0,
            boss_hp: 300.0,
            damage: 15.0,
            speed: 2.5,
            aggro: 20.0,
            attack_range: 2.0,
        },
        CampDef {
            mob_type: MobType::Demon,
            center: Vec3::new(-100.0, 0.0, -170.0),
            patrol_radius: 20.0,
            count: 5,
            has_boss: true,
            hp: 200.0,
            boss_hp: 500.0,
            damage: 25.0,
            speed: 3.0,
            aggro: 25.0,
            attack_range: 2.2,
        },
    ];

    for camp in &camps {
        let mesh = match camp.mob_type {
            MobType::Goblin => mob_meshes.goblin.clone(),
            MobType::Skeleton => mob_meshes.skeleton.clone(),
            MobType::Orc => mob_meshes.orc.clone(),
            MobType::Demon => mob_meshes.demon.clone(),
        };
        let mat = match camp.mob_type {
            MobType::Goblin => mob_mats.goblin.clone(),
            MobType::Skeleton => mob_mats.skeleton.clone(),
            MobType::Orc => mob_mats.orc.clone(),
            MobType::Demon => mob_mats.demon.clone(),
        };

        // Set center Y to terrain
        let center = Vec3::new(
            camp.center.x,
            terrain_height(camp.center.x, camp.center.z),
            camp.center.z,
        );

        // Spawn regular mobs in a circle around center
        for i in 0..camp.count {
            let angle = i as f32 / camp.count as f32 * std::f32::consts::TAU;
            let offset_r = camp.patrol_radius * 0.3;
            let x = center.x + angle.cos() * offset_r;
            let z = center.z + angle.sin() * offset_r;
            let y = terrain_height(x, z) + mob_y_offset(camp.mob_type);

            commands.spawn((
                Mob,
                camp.mob_type,
                Faction::Enemy,
                Health {
                    current: camp.hp,
                    max: camp.hp,
                },
                UnitSpeed(camp.speed),
                AttackDamage(camp.damage),
                AttackRange(camp.attack_range),
                AttackCooldown {
                    timer: Timer::from_seconds(1.2, TimerMode::Repeating),
                },
                AggroRange(camp.aggro),
                PatrolState {
                    state: PatrolStateKind::Idle,
                    center,
                    radius: camp.patrol_radius,
                    patrol_target: None,
                },
                Mesh3d(mesh.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(Vec3::new(x, y, z)),
            ));
        }

        // Spawn boss
        if camp.has_boss {
            let x = center.x;
            let z = center.z;
            let y = terrain_height(x, z) + mob_y_offset(camp.mob_type) * 1.5;

            commands.spawn((
                Mob,
                camp.mob_type,
                Faction::Enemy,
                Health {
                    current: camp.boss_hp,
                    max: camp.boss_hp,
                },
                UnitSpeed(camp.speed * 0.9),
                AttackDamage(camp.damage * 1.5),
                AttackRange(camp.attack_range * 1.2),
                AttackCooldown {
                    timer: Timer::from_seconds(1.0, TimerMode::Repeating),
                },
                AggroRange(camp.aggro),
                PatrolState {
                    state: PatrolStateKind::Idle,
                    center,
                    radius: camp.patrol_radius,
                    patrol_target: None,
                },
                Mesh3d(mesh.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(Vec3::new(x, y, z))
                    .with_scale(Vec3::splat(1.5)),
            ));
        }
    }
}

fn mob_patrol(
    time: Res<Time>,
    mut mobs: Query<
        (&mut Transform, &mut PatrolState, &UnitSpeed, &MobType),
        (With<Mob>, Without<AttackTarget>),
    >,
) {
    for (mut tf, mut patrol, speed, mob_type) in &mut mobs {
        match patrol.state {
            PatrolStateKind::Idle => {
                // Pick a random patrol point within radius
                let angle = time.elapsed_secs() * 1.7 + tf.translation.x * 0.1;
                let r = patrol.radius * (0.3 + (angle.sin() * 0.5 + 0.5) * 0.7);
                let target = Vec3::new(
                    patrol.center.x + angle.cos() * r,
                    0.0,
                    patrol.center.z + angle.sin() * r,
                );
                patrol.patrol_target = Some(target);
                patrol.state = PatrolStateKind::Patrolling;
            }
            PatrolStateKind::Patrolling => {
                if let Some(target) = patrol.patrol_target {
                    let dir = Vec3::new(
                        target.x - tf.translation.x,
                        0.0,
                        target.z - tf.translation.z,
                    );
                    let dist = dir.length();
                    if dist < 1.0 {
                        patrol.state = PatrolStateKind::Idle;
                        patrol.patrol_target = None;
                    } else {
                        let step = dir.normalize() * speed.0 * 0.5 * time.delta_secs();
                        tf.translation += step;
                        tf.translation.y =
                            terrain_height(tf.translation.x, tf.translation.z)
                                + mob_y_offset(*mob_type);
                    }
                }
            }
            PatrolStateKind::Returning => {
                let dir = Vec3::new(
                    patrol.center.x - tf.translation.x,
                    0.0,
                    patrol.center.z - tf.translation.z,
                );
                let dist = dir.length();
                if dist < 2.0 {
                    patrol.state = PatrolStateKind::Idle;
                } else {
                    let step = dir.normalize() * speed.0 * time.delta_secs();
                    tf.translation += step;
                    tf.translation.y =
                        terrain_height(tf.translation.x, tf.translation.z)
                            + mob_y_offset(*mob_type);
                }
            }
            _ => {}
        }
    }
}

fn mob_aggro(
    mut commands: Commands,
    mobs: Query<
        (Entity, &Transform, &AggroRange),
        (With<Mob>, Without<AttackTarget>),
    >,
    players: Query<(Entity, &Transform), (With<Unit>, With<Faction>)>,
    factions: Query<&Faction>,
) {
    for (mob_entity, mob_tf, aggro) in &mobs {
        let mut closest_dist = f32::MAX;
        let mut closest_player = None;

        for (player_entity, player_tf) in &players {
            if let Ok(faction) = factions.get(player_entity) {
                if *faction != Faction::Player {
                    continue;
                }
            }
            let dist = mob_tf
                .translation
                .distance(player_tf.translation);
            if dist < aggro.0 && dist < closest_dist {
                closest_dist = dist;
                closest_player = Some(player_entity);
            }
        }

        if let Some(target) = closest_player {
            commands.entity(mob_entity).insert(AttackTarget(target));
        }
    }
}

fn mob_chase(
    time: Res<Time>,
    mut mobs: Query<
        (
            &mut Transform,
            &mut PatrolState,
            &AttackTarget,
            &UnitSpeed,
            &AttackRange,
            &MobType,
        ),
        With<Mob>,
    >,
    targets: Query<&Transform, Without<Mob>>,
) {
    for (mut tf, mut patrol, attack_target, speed, range, mob_type) in &mut mobs {
        let Ok(target_tf) = targets.get(attack_target.0) else {
            // Target gone, return
            patrol.state = PatrolStateKind::Returning;
            continue;
        };

        let dir = Vec3::new(
            target_tf.translation.x - tf.translation.x,
            0.0,
            target_tf.translation.z - tf.translation.z,
        );
        let dist = dir.length();

        if dist <= range.0 {
            patrol.state = PatrolStateKind::Attacking;
        } else {
            patrol.state = PatrolStateKind::Chasing;
            let step = dir.normalize() * speed.0 * time.delta_secs();
            tf.translation += step;
            tf.translation.y =
                terrain_height(tf.translation.x, tf.translation.z)
                    + mob_y_offset(*mob_type);
        }
    }
}

fn mob_return(
    mut commands: Commands,
    mut mobs: Query<(Entity, &PatrolState), With<Mob>>,
) {
    for (entity, patrol) in &mut mobs {
        if patrol.state == PatrolStateKind::Returning {
            commands.entity(entity).remove::<AttackTarget>();
        }
    }
}
