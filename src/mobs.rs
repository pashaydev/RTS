use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;

use crate::blueprints::{spawn_from_blueprint, BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::components::*;
use crate::ground::HeightMap;
use crate::model_assets::UnitModelAssets;

pub struct MobsPlugin;

impl Plugin for MobsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::InGame),
            spawn_mob_camps.after(crate::ground::spawn_ground),
        )
        .add_systems(
            Update,
            (mob_patrol, mob_aggro, mob_chase, mob_return)
                .chain()
                .run_if(in_state(AppState::InGame)),
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

/// Ring zone descriptor for procedural camp generation.
struct RingZone {
    min_radius_frac: f32,
    max_radius_frac: f32,
    kinds: &'static [EntityKind],
    mob_count: (usize, usize), // (min, max)
    has_boss: bool,
    boss_hp: (f32, f32), // (min, max)
}

const RING_ZONES: &[RingZone] = &[
    RingZone {
        min_radius_frac: 0.0,
        max_radius_frac: 0.3,
        kinds: &[EntityKind::Goblin],
        mob_count: (3, 4),
        has_boss: false,
        boss_hp: (0.0, 0.0),
    },
    RingZone {
        min_radius_frac: 0.3,
        max_radius_frac: 0.6,
        kinds: &[EntityKind::Skeleton, EntityKind::Orc],
        mob_count: (5, 6),
        has_boss: true,
        boss_hp: (200.0, 300.0),
    },
    RingZone {
        min_radius_frac: 0.6,
        max_radius_frac: 1.0,
        kinds: &[EntityKind::Demon],
        mob_count: (5, 7),
        has_boss: true,
        boss_hp: (400.0, 500.0),
    },
];

fn generate_camps(
    rng: &mut StdRng,
    half_map: f32,
    num_camps: usize,
    player_spawns: &[(Faction, (f32, f32))],
    biome_map: &BiomeMap,
) -> Vec<CampSpawn> {
    let mut camps = Vec::new();
    let min_player_dist = 40.0;
    let min_camp_dist = 50.0;
    let max_attempts = 100;

    // Distribute camps across zones roughly evenly
    let zone_counts = distribute_camps(num_camps);

    for (zone_idx, &count) in zone_counts.iter().enumerate() {
        let zone = &RING_ZONES[zone_idx];
        let r_min = zone.min_radius_frac * half_map;
        let r_max = zone.max_radius_frac * half_map;

        for _ in 0..count {
            let mut placed = false;
            for _ in 0..max_attempts {
                let angle = rng.random_range(0.0..std::f32::consts::TAU);
                let r = rng.random_range(r_min..r_max);
                let x = angle.cos() * r;
                let z = angle.sin() * r;

                // Check biome
                let biome = biome_map.get_biome(x, z);
                if biome == Biome::Water {
                    continue;
                }

                // Check distance from player spawns
                let too_close_player = player_spawns.iter().any(|&(_, (sx, sz))| {
                    let dx = x - sx;
                    let dz = z - sz;
                    (dx * dx + dz * dz).sqrt() < min_player_dist
                });
                if too_close_player {
                    continue;
                }

                // Check distance from other camps
                let too_close_camp = camps.iter().any(|c: &CampSpawn| {
                    let dx = x - c.center.x;
                    let dz = z - c.center.z;
                    (dx * dx + dz * dz).sqrt() < min_camp_dist
                });
                if too_close_camp {
                    continue;
                }

                let kind = zone.kinds[rng.random_range(0..zone.kinds.len())];
                let mob_count = rng.random_range(zone.mob_count.0..=zone.mob_count.1);
                let boss_hp = if zone.has_boss {
                    rng.random_range(zone.boss_hp.0..=zone.boss_hp.1)
                } else {
                    0.0
                };

                camps.push(CampSpawn {
                    kind,
                    center: Vec3::new(x, 0.0, z),
                    count: mob_count,
                    has_boss: zone.has_boss,
                    boss_hp,
                });
                placed = true;
                break;
            }
            if !placed {
                warn!("Could not place mob camp in zone {}", zone_idx);
            }
        }
    }
    camps
}

fn distribute_camps(total: usize) -> Vec<usize> {
    let num_zones = RING_ZONES.len();
    let base = total / num_zones;
    let remainder = total % num_zones;
    let mut counts = vec![base; num_zones];
    for i in 0..remainder {
        counts[i] += 1;
    }
    counts
}

fn spawn_mob_camps(
    mut commands: Commands,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    unit_models: Option<Res<UnitModelAssets>>,
    height_map: Res<HeightMap>,
    biome_map: Res<BiomeMap>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
) {
    let mut rng = StdRng::seed_from_u64(map_seed.0.wrapping_add(3000));
    let half_map = config.map_size.world_size() / 2.0;

    let num_camps = match config.map_size {
        MapSize::Small => 4,
        MapSize::Medium => 6,
        MapSize::Large => 8,
    };

    let player_spawns = config.spawn_positions(map_seed.0);
    let camps = generate_camps(&mut rng, half_map, num_camps, &player_spawns, &biome_map);

    for camp in &camps {
        let bp = registry.get(camp.kind);
        let patrol_radius = bp
            .mob_ai
            .as_ref()
            .map(|ai| ai.patrol_radius)
            .unwrap_or(12.0);
        let y_off = bp.movement.as_ref().map(|m| m.y_offset).unwrap_or(0.8);

        let center = Vec3::new(
            camp.center.x,
            height_map.sample(camp.center.x, camp.center.z),
            camp.center.z,
        );

        // Spawn regular mobs in a circle
        for i in 0..camp.count {
            let angle = i as f32 / camp.count as f32 * std::f32::consts::TAU;
            let offset_r = patrol_radius * 0.3;
            let x = center.x + angle.cos() * offset_r;
            let z = center.z + angle.sin() * offset_r;

            let entity = spawn_from_blueprint(
                &mut commands,
                &cache,
                camp.kind,
                Vec3::new(x, 0.0, z),
                &registry,
                None,
                unit_models.as_deref(),
                &height_map,
            );

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

            let entity = spawn_from_blueprint(
                &mut commands,
                &cache,
                camp.kind,
                Vec3::new(center.x, 0.0, center.z),
                &registry,
                None,
                unit_models.as_deref(),
                &height_map,
            );

            // Apply boss modifiers
            commands.entity(entity).insert((
                Boss,
                Health {
                    current: camp.boss_hp,
                    max: camp.boss_hp,
                },
                UnitSpeed(bp.movement.as_ref().unwrap().speed * 0.9),
                AttackDamage(combat.damage * 1.5),
                AttackRange(combat.attack_range * 1.2),
                AttackCooldown {
                    timer: Timer::from_seconds(1.0, TimerMode::Repeating),
                },
                Transform::from_translation(Vec3::new(
                    center.x,
                    height_map.sample(center.x, center.z) + y_off * 1.5,
                    center.z,
                ))
                .with_scale(Vec3::splat(1.5)),
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
    height_map: Res<HeightMap>,
    mut mobs: Query<
        (&mut Transform, &mut PatrolState, &UnitSpeed, &EntityKind),
        (With<Mob>, Without<AttackTarget>),
    >,
) {
    for (mut tf, mut patrol, speed, kind) in &mut mobs {
        let y_off = registry
            .get(*kind)
            .movement
            .as_ref()
            .map(|m| m.y_offset)
            .unwrap_or(0.8);
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
                            height_map.sample(tf.translation.x, tf.translation.z) + y_off;
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
                        height_map.sample(tf.translation.x, tf.translation.z) + y_off;
                }
            }
            _ => {}
        }
    }
}

fn mob_aggro(
    mut commands: Commands,
    mobs: Query<(Entity, &Transform, &AggroRange), (With<Mob>, Without<AttackTarget>)>,
    players: Query<(Entity, &Transform, &Faction), With<Unit>>,
) {
    for (mob_entity, mob_tf, aggro) in &mobs {
        let mut closest_dist = f32::MAX;
        let mut closest_player = None;

        for (player_entity, player_tf, faction) in &players {
            // Mobs aggro on all player factions (not neutral)
            if *faction == Faction::Neutral {
                continue;
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
    height_map: Res<HeightMap>,
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
            let y_off = registry
                .get(*kind)
                .movement
                .as_ref()
                .map(|m| m.y_offset)
                .unwrap_or(0.8);
            tf.translation.y = height_map.sample(tf.translation.x, tf.translation.z) + y_off;
        }
    }
}

fn mob_return(mut commands: Commands, mut mobs: Query<(Entity, &PatrolState), With<Mob>>) {
    for (entity, patrol) in &mut mobs {
        if patrol.state == PatrolStateKind::Returning {
            commands.entity(entity).remove::<AttackTarget>();
        }
    }
}
