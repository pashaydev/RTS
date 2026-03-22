use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;

use crate::blueprints::{spawn_from_blueprint, BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::components::*;
use crate::ground::{is_in_mountain_border, BorderSettings, HeightMap};
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
    border: BorderSettings,
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
                if is_in_mountain_border(x, z, half_map, border) {
                    continue;
                }

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
    net_role: Res<crate::multiplayer::NetRole>,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    unit_models: Option<Res<UnitModelAssets>>,
    height_map: Res<HeightMap>,
    biome_map: Res<BiomeMap>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
) {
    if *net_role == crate::multiplayer::NetRole::Client {
        return;
    }

    let mut rng = StdRng::seed_from_u64(map_seed.0.wrapping_add(3000));
    let half_map = config.map_size.world_size() / 2.0;

    let num_camps = match config.map_size {
        MapSize::Small => 4,
        MapSize::Medium => 6,
        MapSize::Large => 8,
    };

    let player_spawns = config.spawn_positions(map_seed.0);
    let border = BorderSettings::from_map_size(config.map_size.world_size());
    let camps = generate_camps(
        &mut rng,
        half_map,
        num_camps,
        &player_spawns,
        &biome_map,
        border,
    );

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

        // Determine camp reward based on mob tier
        let camp_reward = camp_reward_for_kind(camp.kind);

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
                chase_elapsed: 0.0,
            });

            // Attach camp reward to first mob if camp has no boss
            if i == 0 && !camp.has_boss {
                commands.entity(entity).insert(camp_reward.clone());
            }
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

            // Apply boss modifiers + camp reward
            commands.entity(entity).insert((
                Boss,
                camp_reward.clone(),
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
                    chase_elapsed: 0.0,
                },
            ));
        }
    }
}

/// Returns the resource reward for clearing a mob camp of the given kind.
fn camp_reward_for_kind(kind: EntityKind) -> CampReward {
    use crate::blueprints::ResourceCost;
    let resources = match kind {
        EntityKind::Goblin => ResourceCost::new()
            .with(ResourceType::Wood, 30)
            .with(ResourceType::Copper, 15),
        EntityKind::Skeleton | EntityKind::Orc => ResourceCost::new()
            .with(ResourceType::Wood, 50)
            .with(ResourceType::Iron, 30)
            .with(ResourceType::Gold, 20),
        EntityKind::Demon => ResourceCost::new()
            .with(ResourceType::Wood, 80)
            .with(ResourceType::Iron, 50)
            .with(ResourceType::Gold, 40),
        _ => ResourceCost::new(),
    };
    CampReward { resources }
}

fn mob_patrol(
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    height_map: Res<HeightMap>,
    mut mobs: Query<
        (
            &mut Transform,
            &mut PatrolState,
            &UnitSpeed,
            &EntityKind,
            &mut Health,
        ),
        (With<Mob>, Without<AttackTarget>),
    >,
) {
    for (mut tf, mut patrol, speed, kind, mut health) in &mut mobs {
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
                // Regenerate health while returning home (10% max HP per second)
                health.current =
                    (health.current + health.max * 0.1 * time.delta_secs()).min(health.max);

                let dir = Vec3::new(
                    patrol.center.x - tf.translation.x,
                    0.0,
                    patrol.center.z - tf.translation.z,
                );
                let dist = dir.length();
                if dist < 2.0 {
                    // Fully heal when arriving home
                    health.current = health.max;
                    patrol.state = PatrolStateKind::Idle;
                } else {
                    // Move faster when returning (1.5x speed)
                    let step = dir.normalize() * speed.0 * 1.5 * time.delta_secs();
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
    buildings: Query<(Entity, &Transform, &Faction), (With<Building>, Without<Unit>)>,
) {
    for (mob_entity, mob_tf, aggro) in &mobs {
        let mut closest_dist = f32::MAX;
        let mut closest_target = None;

        for (player_entity, player_tf, faction) in &players {
            // Mobs aggro on all player factions (not neutral)
            if *faction == Faction::Neutral {
                continue;
            }
            let dist = mob_tf.translation.distance(player_tf.translation);
            if dist < aggro.0 && dist < closest_dist {
                closest_dist = dist;
                closest_target = Some(player_entity);
            }
        }

        for (building_entity, building_tf, faction) in &buildings {
            if *faction == Faction::Neutral {
                continue;
            }
            let dist = mob_tf.translation.distance(building_tf.translation);
            if dist < aggro.0 && dist < closest_dist {
                closest_dist = dist;
                closest_target = Some(building_entity);
            }
        }

        if let Some(target) = closest_target {
            commands.entity(mob_entity).insert(AttackTarget(target));

            // Pack aggro: alert nearby mobs to chase the same target
            let mob_pos = mob_tf.translation;
            for (other_entity, other_tf, _) in &mobs {
                if other_entity == mob_entity {
                    continue;
                }
                if mob_pos.distance(other_tf.translation) < 15.0 {
                    commands.entity(other_entity).insert(AttackTarget(target));
                }
            }
        }
    }
}

fn mob_chase(
    mut commands: Commands,
    time: Res<Time>,
    teams: Res<TeamConfig>,
    registry: Res<BlueprintRegistry>,
    height_map: Res<HeightMap>,
    walls: Query<
        (Entity, &Transform, &BuildingFootprint, &Faction),
        (
            With<Building>,
            Without<Mob>,
            Or<(With<WallSegmentPiece>, With<WallPostPiece>)>,
        ),
    >,
    mut mobs: Query<
        (
            Entity,
            &mut Transform,
            &mut PatrolState,
            &AttackTarget,
            &UnitSpeed,
            &AttackRange,
            &EntityKind,
            &Faction,
        ),
        With<Mob>,
    >,
    targets: Query<&Transform, Without<Mob>>,
) {
    // Leash: max distance from camp center before giving up chase
    const LEASH_DISTANCE: f32 = 40.0;
    // Max seconds a mob will chase before giving up
    const MAX_CHASE_SECS: f32 = 15.0;

    for (mob_entity, mut tf, mut patrol, attack_target, speed, range, kind, faction) in &mut mobs {
        let Ok(target_tf) = targets.get(attack_target.0) else {
            patrol.state = PatrolStateKind::Returning;
            patrol.chase_elapsed = 0.0;
            continue;
        };

        // Leash check: distance from home or chase duration exceeded
        let dist_from_home = Vec2::new(
            tf.translation.x - patrol.center.x,
            tf.translation.z - patrol.center.z,
        )
        .length();
        patrol.chase_elapsed += time.delta_secs();

        if dist_from_home > LEASH_DISTANCE || patrol.chase_elapsed > MAX_CHASE_SECS {
            patrol.state = PatrolStateKind::Returning;
            patrol.chase_elapsed = 0.0;
            commands.entity(mob_entity).remove::<AttackTarget>();
            continue;
        }

        let target_is_wall = walls.get(attack_target.0).is_ok();
        if !target_is_wall {
            let from = Vec2::new(tf.translation.x, tf.translation.z);
            let to = Vec2::new(target_tf.translation.x, target_tf.translation.z);
            let delta = to - from;
            let line_len = delta.length();
            if line_len > 0.5 {
                let dir = delta / line_len;
                let mut blocking_wall: Option<(Entity, f32)> = None;

                for (wall_entity, wall_tf, wall_fp, wall_faction) in &walls {
                    if !teams.is_hostile(faction, wall_faction) {
                        continue;
                    }
                    let wall_pos = Vec2::new(wall_tf.translation.x, wall_tf.translation.z);
                    let rel = wall_pos - from;
                    let t = rel.dot(dir);
                    if t <= 0.3 || t >= line_len - 0.3 {
                        continue;
                    }
                    let closest = from + dir * t;
                    let perp_dist = wall_pos.distance(closest);
                    if perp_dist <= wall_fp.0 + 0.35
                        && blocking_wall.map_or(true, |(_, best_t)| t < best_t)
                    {
                        blocking_wall = Some((wall_entity, t));
                    }
                }

                if let Some((wall_entity, _)) = blocking_wall {
                    commands
                        .entity(mob_entity)
                        .insert(AttackTarget(wall_entity));
                    continue;
                }
            }
        }

        let dir = Vec3::new(
            target_tf.translation.x - tf.translation.x,
            0.0,
            target_tf.translation.z - tf.translation.z,
        );
        let dist = dir.length();

        if dist <= range.0 {
            patrol.state = PatrolStateKind::Attacking;
            // Reset chase timer while in combat range
            patrol.chase_elapsed = 0.0;
        } else {
            patrol.state = PatrolStateKind::Chasing;
            let step = dir.normalize() * speed.0 * time.delta_secs();
            let candidate = tf.translation + step;
            let blocked = walls
                .iter()
                .filter(|(wall_entity, _, _, _)| *wall_entity != attack_target.0)
                .any(|(_, wall_tf, wall_fp, wall_faction)| {
                    if !teams.is_hostile(faction, wall_faction) {
                        return false;
                    }
                    let a = Vec2::new(candidate.x, candidate.z);
                    let b = Vec2::new(wall_tf.translation.x, wall_tf.translation.z);
                    a.distance(b) < wall_fp.0 + 0.6
                });
            if blocked {
                continue;
            }
            tf.translation = candidate;
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
