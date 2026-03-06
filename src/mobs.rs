use bevy::prelude::*;

use crate::blueprints::{BlueprintRegistry, EntityKind, EntityVisualCache, spawn_from_blueprint};
use crate::components::*;
use crate::ground::terrain_height;

pub struct MobsPlugin;

impl Plugin for MobsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_mob_camps)
            .add_systems(
                Update,
                (mob_patrol, mob_aggro, mob_chase, mob_return).chain(),
            );
    }
}

struct CampSpawn {
    kind: EntityKind,
    center: Vec3,
    count: usize,
    has_boss: bool,
    boss_hp: f32,
}

fn spawn_mob_camps(
    mut commands: Commands,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
) {
    let camps = [
        CampSpawn {
            kind: EntityKind::Goblin,
            center: Vec3::new(50.0, 0.0, 0.0),
            count: 5,
            has_boss: false,
            boss_hp: 0.0,
        },
        CampSpawn {
            kind: EntityKind::Skeleton,
            center: Vec3::new(-40.0, 0.0, 100.0),
            count: 5,
            has_boss: true,
            boss_hp: 200.0,
        },
        CampSpawn {
            kind: EntityKind::Orc,
            center: Vec3::new(80.0, 0.0, -150.0),
            count: 6,
            has_boss: true,
            boss_hp: 300.0,
        },
        CampSpawn {
            kind: EntityKind::Demon,
            center: Vec3::new(-100.0, 0.0, -170.0),
            count: 5,
            has_boss: true,
            boss_hp: 500.0,
        },
    ];

    for camp in &camps {
        let bp = registry.get(camp.kind);
        let patrol_radius = bp.mob_ai.as_ref().map(|ai| ai.patrol_radius).unwrap_or(12.0);
        let y_off = bp.movement.as_ref().map(|m| m.y_offset).unwrap_or(0.8);

        let center = Vec3::new(
            camp.center.x,
            terrain_height(camp.center.x, camp.center.z),
            camp.center.z,
        );

        // Spawn regular mobs in a circle
        for i in 0..camp.count {
            let angle = i as f32 / camp.count as f32 * std::f32::consts::TAU;
            let offset_r = patrol_radius * 0.3;
            let x = center.x + angle.cos() * offset_r;
            let z = center.z + angle.sin() * offset_r;

            let entity = spawn_from_blueprint(&mut commands, &cache, camp.kind, Vec3::new(x, 0.0, z), &registry);

            // Override patrol center
            commands.entity(entity).insert(PatrolState {
                state: PatrolStateKind::Idle,
                center,
                radius: patrol_radius,
                patrol_target: None,
            });
        }

        // Spawn boss
        if camp.has_boss {
            let combat = bp.combat.as_ref().unwrap();

            let entity = spawn_from_blueprint(&mut commands, &cache, camp.kind, Vec3::new(center.x, 0.0, center.z), &registry);

            // Apply boss modifiers
            commands.entity(entity).insert((
                Boss,
                Health { current: camp.boss_hp, max: camp.boss_hp },
                UnitSpeed(bp.movement.as_ref().unwrap().speed * 0.9),
                AttackDamage(combat.damage * 1.5),
                AttackRange(combat.attack_range * 1.2),
                AttackCooldown {
                    timer: Timer::from_seconds(1.0, TimerMode::Repeating),
                },
                Transform::from_translation(Vec3::new(
                    center.x,
                    terrain_height(center.x, center.z) + y_off * 1.5,
                    center.z,
                )).with_scale(Vec3::splat(1.5)),
                PatrolState {
                    state: PatrolStateKind::Idle,
                    center,
                    radius: patrol_radius,
                    patrol_target: None,
                },
            ));
        }
    }
}

fn mob_patrol(
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    mut mobs: Query<
        (&mut Transform, &mut PatrolState, &UnitSpeed, &EntityKind),
        (With<Mob>, Without<AttackTarget>),
    >,
) {
    for (mut tf, mut patrol, speed, kind) in &mut mobs {
        let y_off = registry.get(*kind).movement.as_ref().map(|m| m.y_offset).unwrap_or(0.8);
        match patrol.state {
            PatrolStateKind::Idle => {
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
                            terrain_height(tf.translation.x, tf.translation.z) + y_off;
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
                        terrain_height(tf.translation.x, tf.translation.z) + y_off;
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
            let dist = mob_tf.translation.distance(player_tf.translation);
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
    registry: Res<BlueprintRegistry>,
    mut mobs: Query<
        (
            &mut Transform,
            &mut PatrolState,
            &AttackTarget,
            &UnitSpeed,
            &AttackRange,
            &EntityKind,
        ),
        With<Mob>,
    >,
    targets: Query<&Transform, Without<Mob>>,
) {
    for (mut tf, mut patrol, attack_target, speed, range, kind) in &mut mobs {
        let Ok(target_tf) = targets.get(attack_target.0) else {
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
            let y_off = registry.get(*kind).movement.as_ref().map(|m| m.y_offset).unwrap_or(0.8);
            tf.translation.y =
                terrain_height(tf.translation.x, tf.translation.z) + y_off;
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
