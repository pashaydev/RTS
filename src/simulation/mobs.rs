use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;

use crate::blueprints::{spawn_from_blueprint, BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::simulation::combat::{
    apply_auto_attack_intent, reset_combat_state, CombatBudgetState, CombatCoreSet,
};
use crate::types::*;
use crate::world::ground::{is_in_mountain_border, BorderSettings, HeightMap};
use crate::simulation::items::ItemKind;
use crate::presentation::model_assets::UnitModelAssets;

pub struct MobsPlugin;

impl Plugin for MobsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppState::InGame),
            spawn_mob_camps
                .after(crate::world::ground::spawn_ground)
                .run_if(not(resource_exists::<crate::infrastructure::save_load::PendingLoad>)),
        )
        .add_systems(
            FixedUpdate,
            (mob_patrol, mob_aggro, mob_leash)
                .chain()
                .in_set(SimSet::Ai)
                .before(CombatCoreSet::Approach)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

struct CampSpawn {
    center: Vec3,
    reward_kind: EntityKind,
    members: Vec<CampMemberSpawn>,
}

#[derive(Component, Clone)]
pub struct CampItemDrops {
    pub items: Vec<ItemKind>,
}

#[derive(Clone, Copy)]
struct CampMemberSpawn {
    kind: EntityKind,
    ring_radius: f32,
    angle_offset: f32,
    hp_mult: f32,
    damage_mult: f32,
    speed_mult: f32,
    range_mult: f32,
    scale_mult: f32,
    aggro_bonus: f32,
    boss: bool,
}

/// Ring zone descriptor for procedural camp generation.
struct RingZone {
    min_radius_frac: f32,
    max_radius_frac: f32,
    kinds: &'static [EntityKind],
}

const RING_ZONES: &[RingZone] = &[
    RingZone {
        min_radius_frac: 0.0,
        max_radius_frac: 0.3,
        kinds: &[EntityKind::Goblin],
    },
    RingZone {
        min_radius_frac: 0.3,
        max_radius_frac: 0.6,
        kinds: &[EntityKind::Skeleton, EntityKind::Orc],
    },
    RingZone {
        min_radius_frac: 0.6,
        max_radius_frac: 1.0,
        kinds: &[EntityKind::Demon],
    },
];

fn build_camp_members(
    rng: &mut StdRng,
    zone_idx: usize,
    primary_kind: EntityKind,
) -> (EntityKind, Vec<CampMemberSpawn>) {
    use EntityKind::*;

    let members = match zone_idx {
        0 => {
            if rng.random_bool(0.5) {
                vec![
                    CampMemberSpawn {
                        kind: Goblin,
                        ring_radius: 0.25,
                        angle_offset: 0.1,
                        hp_mult: 0.95,
                        damage_mult: 1.0,
                        speed_mult: 1.15,
                        range_mult: 1.0,
                        scale_mult: 0.92,
                        aggro_bonus: 1.5,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Goblin,
                        ring_radius: 0.35,
                        angle_offset: 1.5,
                        hp_mult: 0.95,
                        damage_mult: 1.0,
                        speed_mult: 1.12,
                        range_mult: 1.0,
                        scale_mult: 0.92,
                        aggro_bonus: 1.5,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Goblin,
                        ring_radius: 0.42,
                        angle_offset: 2.9,
                        hp_mult: 1.0,
                        damage_mult: 1.1,
                        speed_mult: 1.05,
                        range_mult: 1.0,
                        scale_mult: 0.98,
                        aggro_bonus: 1.0,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Goblin,
                        ring_radius: 0.2,
                        angle_offset: 4.2,
                        hp_mult: 1.15,
                        damage_mult: 1.25,
                        speed_mult: 1.0,
                        range_mult: 1.05,
                        scale_mult: 1.08,
                        aggro_bonus: 2.0,
                        boss: false,
                    },
                ]
            } else {
                vec![
                    CampMemberSpawn {
                        kind: Goblin,
                        ring_radius: 0.3,
                        angle_offset: 0.2,
                        hp_mult: 0.9,
                        damage_mult: 1.0,
                        speed_mult: 1.18,
                        range_mult: 1.0,
                        scale_mult: 0.9,
                        aggro_bonus: 1.0,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Goblin,
                        ring_radius: 0.38,
                        angle_offset: 2.1,
                        hp_mult: 0.9,
                        damage_mult: 1.0,
                        speed_mult: 1.18,
                        range_mult: 1.0,
                        scale_mult: 0.9,
                        aggro_bonus: 1.0,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Goblin,
                        ring_radius: 0.26,
                        angle_offset: 4.3,
                        hp_mult: 0.95,
                        damage_mult: 1.05,
                        speed_mult: 1.12,
                        range_mult: 1.0,
                        scale_mult: 0.93,
                        aggro_bonus: 1.0,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Skeleton,
                        ring_radius: 0.08,
                        angle_offset: 5.4,
                        hp_mult: 1.2,
                        damage_mult: 1.2,
                        speed_mult: 0.92,
                        range_mult: 1.1,
                        scale_mult: 1.04,
                        aggro_bonus: 2.5,
                        boss: false,
                    },
                ]
            }
        }
        1 => {
            if primary_kind == Orc {
                vec![
                    CampMemberSpawn {
                        kind: Orc,
                        ring_radius: 0.14,
                        angle_offset: 0.0,
                        hp_mult: 1.45,
                        damage_mult: 1.3,
                        speed_mult: 0.95,
                        range_mult: 1.05,
                        scale_mult: 1.16,
                        aggro_bonus: 3.0,
                        boss: true,
                    },
                    CampMemberSpawn {
                        kind: Skeleton,
                        ring_radius: 0.34,
                        angle_offset: 1.7,
                        hp_mult: 1.0,
                        damage_mult: 1.0,
                        speed_mult: 1.0,
                        range_mult: 1.0,
                        scale_mult: 1.0,
                        aggro_bonus: 1.5,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Skeleton,
                        ring_radius: 0.3,
                        angle_offset: 3.4,
                        hp_mult: 1.0,
                        damage_mult: 1.0,
                        speed_mult: 1.0,
                        range_mult: 1.0,
                        scale_mult: 1.0,
                        aggro_bonus: 1.5,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Orc,
                        ring_radius: 0.42,
                        angle_offset: 4.9,
                        hp_mult: 1.1,
                        damage_mult: 1.15,
                        speed_mult: 0.98,
                        range_mult: 1.0,
                        scale_mult: 1.06,
                        aggro_bonus: 2.0,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Goblin,
                        ring_radius: 0.46,
                        angle_offset: 2.6,
                        hp_mult: 0.95,
                        damage_mult: 1.0,
                        speed_mult: 1.1,
                        range_mult: 1.0,
                        scale_mult: 0.92,
                        aggro_bonus: 1.0,
                        boss: false,
                    },
                ]
            } else {
                vec![
                    CampMemberSpawn {
                        kind: Skeleton,
                        ring_radius: 0.14,
                        angle_offset: 0.0,
                        hp_mult: 1.55,
                        damage_mult: 1.25,
                        speed_mult: 0.96,
                        range_mult: 1.15,
                        scale_mult: 1.14,
                        aggro_bonus: 3.0,
                        boss: true,
                    },
                    CampMemberSpawn {
                        kind: Skeleton,
                        ring_radius: 0.3,
                        angle_offset: 1.3,
                        hp_mult: 1.0,
                        damage_mult: 1.0,
                        speed_mult: 1.0,
                        range_mult: 1.0,
                        scale_mult: 1.0,
                        aggro_bonus: 1.5,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Skeleton,
                        ring_radius: 0.38,
                        angle_offset: 2.9,
                        hp_mult: 1.0,
                        damage_mult: 1.0,
                        speed_mult: 1.0,
                        range_mult: 1.0,
                        scale_mult: 1.0,
                        aggro_bonus: 1.5,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Orc,
                        ring_radius: 0.34,
                        angle_offset: 4.3,
                        hp_mult: 1.15,
                        damage_mult: 1.15,
                        speed_mult: 0.96,
                        range_mult: 1.0,
                        scale_mult: 1.08,
                        aggro_bonus: 2.0,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Goblin,
                        ring_radius: 0.46,
                        angle_offset: 5.4,
                        hp_mult: 0.9,
                        damage_mult: 1.0,
                        speed_mult: 1.12,
                        range_mult: 1.0,
                        scale_mult: 0.92,
                        aggro_bonus: 1.0,
                        boss: false,
                    },
                ]
            }
        }
        _ => {
            if rng.random_bool(0.5) {
                vec![
                    CampMemberSpawn {
                        kind: Demon,
                        ring_radius: 0.12,
                        angle_offset: 0.2,
                        hp_mult: 1.55,
                        damage_mult: 1.35,
                        speed_mult: 1.0,
                        range_mult: 1.2,
                        scale_mult: 1.18,
                        aggro_bonus: 4.0,
                        boss: true,
                    },
                    CampMemberSpawn {
                        kind: Orc,
                        ring_radius: 0.34,
                        angle_offset: 1.6,
                        hp_mult: 1.2,
                        damage_mult: 1.15,
                        speed_mult: 0.95,
                        range_mult: 1.0,
                        scale_mult: 1.08,
                        aggro_bonus: 2.0,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Orc,
                        ring_radius: 0.4,
                        angle_offset: 3.2,
                        hp_mult: 1.2,
                        damage_mult: 1.15,
                        speed_mult: 0.95,
                        range_mult: 1.0,
                        scale_mult: 1.08,
                        aggro_bonus: 2.0,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Skeleton,
                        ring_radius: 0.42,
                        angle_offset: 4.6,
                        hp_mult: 1.0,
                        damage_mult: 1.05,
                        speed_mult: 1.0,
                        range_mult: 1.05,
                        scale_mult: 1.0,
                        aggro_bonus: 1.5,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Skeleton,
                        ring_radius: 0.28,
                        angle_offset: 5.4,
                        hp_mult: 1.0,
                        damage_mult: 1.05,
                        speed_mult: 1.0,
                        range_mult: 1.05,
                        scale_mult: 1.0,
                        aggro_bonus: 1.5,
                        boss: false,
                    },
                ]
            } else {
                vec![
                    CampMemberSpawn {
                        kind: Demon,
                        ring_radius: 0.18,
                        angle_offset: 0.0,
                        hp_mult: 1.15,
                        damage_mult: 1.15,
                        speed_mult: 1.05,
                        range_mult: 1.12,
                        scale_mult: 1.06,
                        aggro_bonus: 3.0,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Demon,
                        ring_radius: 0.18,
                        angle_offset: 3.14,
                        hp_mult: 1.15,
                        damage_mult: 1.15,
                        speed_mult: 1.05,
                        range_mult: 1.12,
                        scale_mult: 1.06,
                        aggro_bonus: 3.0,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Orc,
                        ring_radius: 0.38,
                        angle_offset: 1.3,
                        hp_mult: 1.1,
                        damage_mult: 1.1,
                        speed_mult: 0.98,
                        range_mult: 1.0,
                        scale_mult: 1.02,
                        aggro_bonus: 1.8,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Orc,
                        ring_radius: 0.36,
                        angle_offset: 4.5,
                        hp_mult: 1.1,
                        damage_mult: 1.1,
                        speed_mult: 0.98,
                        range_mult: 1.0,
                        scale_mult: 1.02,
                        aggro_bonus: 1.8,
                        boss: false,
                    },
                    CampMemberSpawn {
                        kind: Demon,
                        ring_radius: 0.08,
                        angle_offset: 2.1,
                        hp_mult: 1.65,
                        damage_mult: 1.35,
                        speed_mult: 1.0,
                        range_mult: 1.18,
                        scale_mult: 1.2,
                        aggro_bonus: 4.0,
                        boss: true,
                    },
                ]
            }
        }
    };

    let reward_kind = members
        .iter()
        .map(|m| m.kind)
        .max_by_key(|kind| match kind {
            Goblin => 0,
            Skeleton => 1,
            Orc => 2,
            Demon => 3,
            _ => 0,
        })
        .unwrap_or(primary_kind);

    (reward_kind, members)
}

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
                let (reward_kind, members) = build_camp_members(rng, zone_idx, kind);

                camps.push(CampSpawn {
                    center: Vec3::new(x, 0.0, z),
                    reward_kind,
                    members,
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
    net_role: Res<crate::infrastructure::multiplayer::NetRole>,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    unit_models: Option<Res<UnitModelAssets>>,
    height_map: Res<HeightMap>,
    biome_map: Res<BiomeMap>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
) {
    if *net_role == crate::infrastructure::multiplayer::NetRole::Client {
        return;
    }

    let mut rng = StdRng::seed_from_u64(map_seed.0.wrapping_add(3000));
    let half_map = config.map_size.world_size() / 2.0;

    let num_camps = match config.map_size {
        MapSize::Small => 4,
        MapSize::Medium => 6,
        MapSize::Large => 8,
        MapSize::ExtraLarge => 12,
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
        let bp = registry.get(camp.reward_kind);
        let patrol_radius = bp
            .mob_ai
            .as_ref()
            .map(|ai| ai.patrol_radius)
            .unwrap_or(12.0)
            + camp.members.len() as f32 * 0.6;

        let center = Vec3::new(
            camp.center.x,
            height_map.sample(camp.center.x, camp.center.z),
            camp.center.z,
        );

        // Determine camp reward based on mob tier
        let camp_reward = camp_reward_for_kind(camp.reward_kind);
        let camp_item_drops = camp_item_drops_for_kind(&mut rng, camp.reward_kind);

        for (i, member) in camp.members.iter().enumerate() {
            let member_bp = registry.get(member.kind);
            let member_combat = member_bp.combat.as_ref().unwrap();
            let member_move = member_bp.movement.as_ref().unwrap();
            let member_y_off = member_move.y_offset;
            let angle = member.angle_offset;
            let offset_r = patrol_radius * member.ring_radius;
            let x = center.x + angle.cos() * offset_r;
            let z = center.z + angle.sin() * offset_r;

            let entity = spawn_from_blueprint(
                &mut commands,
                &cache,
                member.kind,
                Vec3::new(x, 0.0, z),
                &registry,
                None,
                unit_models.as_deref(),
                &height_map,
            );

            let mut entity_cmd = commands.entity(entity);
            entity_cmd.insert((
                PatrolState {
                    state: PatrolStateKind::Idle,
                    center,
                    radius: patrol_radius,
                    patrol_target: None,
                    chase_elapsed: 0.0,
                    idle_until: 0.0,
                    patrol_started: 0.0,
                },
                Health {
                    current: member_combat.hp * member.hp_mult,
                    max: member_combat.hp * member.hp_mult,
                },
                UnitSpeed(member_move.speed * member.speed_mult),
                AttackDamage(member_combat.damage * member.damage_mult),
                AttackRange(member_combat.attack_range * member.range_mult),
                AggroRange(member_combat.aggro_range.unwrap_or(12.0) + member.aggro_bonus),
                Transform::from_translation(Vec3::new(
                    x,
                    height_map.sample(x, z) + member_y_off,
                    z,
                ))
                .with_scale(Vec3::splat(member.scale_mult)),
            ));

            if member.boss {
                entity_cmd.insert((
                    Boss,
                    camp_reward.clone(),
                    camp_item_drops.clone(),
                    CombatFxKind::Siege,
                ));
            } else if i == 0 && camp.members.iter().all(|m| !m.boss) {
                entity_cmd.insert((camp_reward.clone(), camp_item_drops.clone()));
            }
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

fn camp_item_drops_for_kind(rng: &mut StdRng, kind: EntityKind) -> CampItemDrops {
    use ItemKind::*;

    let pool: &[ItemKind] = match kind {
        EntityKind::Goblin => &[PaddedVest, KettleHelm, PlainBand, ArmingSword, BattleStaff, YewLongbow],
        EntityKind::Skeleton | EntityKind::Orc => &[
            BronzeCuirass,
            CrusaderHelm,
            WeddingBand,
            GoldenBand,
            VikingBlade,
            MageCrozier,
            WarBow,
        ],
        EntityKind::Demon => &[
            PlateCuirass,
            VikingHelm,
            JewelRing,
            TwinRings,
            LinkedRings,
            VikingBlade,
            MageCrozier,
            WarBow,
        ],
        _ => &[PlainBand],
    };

    let count = match rng.random_range(0..100) {
        0..=1 => 3,
        2..=11 => 2,
        _ => 1,
    };

    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let idx = rng.random_range(0..pool.len());
        items.push(pool[idx]);
    }

    CampItemDrops { items }
}

fn mob_patrol(
    mut commands: Commands,
    time: Res<Time>,
    nav_grid: Option<Res<crate::world::pathfinding::NavGrid>>,
    mut mobs: Query<
        (
            Entity,
            &Transform,
            &mut PatrolState,
            Option<&CombatIntent>,
            Option<&CombatTargetLock>,
            Option<&MoveTarget>,
        ),
        With<Mob>,
    >,
) {
    let now = time.elapsed_secs_f64();
    const PATROL_STUCK_SECS: f64 = 6.0;

    for (entity, tf, mut patrol, intent, lock, move_target) in &mut mobs {
        // Skip mobs that are actively fighting
        if matches!(
            intent,
            Some(CombatIntent::Attack(_, _) | CombatIntent::AttackMove(_, _))
        ) || lock.is_some()
        {
            continue;
        }

        match patrol.state {
            PatrolStateKind::Idle => {
                // Remove leftover MoveTarget from previous patrol/return
                if move_target.is_some() {
                    commands.entity(entity).remove::<MoveTarget>();
                }

                // Wait until idle timer expires before picking a new patrol point
                if now < patrol.idle_until {
                    continue;
                }

                // Try a few candidates and pick the first passable one
                let bits = entity.to_bits();
                let seed = bits.wrapping_mul(2654435761) as f32;
                let mut target = None;
                for attempt in 0..4u32 {
                    let angle = seed % std::f32::consts::TAU
                        + (now as f32) * 0.3
                        + attempt as f32 * 1.57; // rotate 90deg per attempt
                    let r = patrol.radius * (0.3 + ((seed * 0.7 + attempt as f32).sin() * 0.5 + 0.5) * 0.7);
                    let candidate = Vec3::new(
                        patrol.center.x + angle.cos() * r,
                        0.0,
                        patrol.center.z + angle.sin() * r,
                    );

                    // Check passability if NavGrid is available
                    let passable = nav_grid
                        .as_ref()
                        .map_or(true, |grid| grid.is_world_passable(candidate.x, candidate.z));
                    if passable {
                        target = Some(candidate);
                        break;
                    }
                }

                let Some(target) = target else {
                    // All candidates impassable — stay idle a bit longer
                    patrol.idle_until = now + 3.0;
                    continue;
                };

                patrol.patrol_target = Some(target);
                patrol.patrol_started = now;
                patrol.state = PatrolStateKind::Patrolling;

                // Insert MoveTarget so the pathfinding system handles movement
                commands.entity(entity).insert(MoveTarget(target));
            }
            PatrolStateKind::Patrolling => {
                if let Some(target) = patrol.patrol_target {
                    let dist = Vec2::new(
                        target.x - tf.translation.x,
                        target.z - tf.translation.z,
                    )
                    .length();

                    // Reached target or stuck too long → go idle
                    if dist < 2.5 || (now - patrol.patrol_started) > PATROL_STUCK_SECS {
                        patrol.state = PatrolStateKind::Idle;
                        patrol.patrol_target = None;
                        // Randomized idle duration: 2-5 seconds per mob
                        let idle_extra = (entity.to_bits() % 3000) as f64 / 1000.0;
                        patrol.idle_until = now + 2.0 + idle_extra;
                        commands.entity(entity).remove::<MoveTarget>();
                    } else if move_target.is_none() {
                        // MoveTarget was consumed (path completed or cleared) — re-insert
                        commands.entity(entity).insert(MoveTarget(target));
                    }
                }
            }
            PatrolStateKind::Returning => {
                let dist = Vec2::new(
                    patrol.center.x - tf.translation.x,
                    patrol.center.z - tf.translation.z,
                )
                .length();
                if dist < 3.0 {
                    patrol.state = PatrolStateKind::Idle;
                    patrol.idle_until = now + 1.0;
                    commands.entity(entity).remove::<MoveTarget>();
                } else if move_target.is_none() {
                    // Insert MoveTarget toward camp center so pathfinding handles return
                    commands
                        .entity(entity)
                        .insert(MoveTarget(patrol.center));
                }
            }
            _ => {}
        }
    }
}

fn mob_aggro(
    mut commands: Commands,
    time: Res<Time>,
    combat_budget: Res<CombatBudget>,
    mut budget_state: ResMut<CombatBudgetState>,
    mut mobs: Query<
        (
            Entity,
            &Transform,
            &AggroRange,
            Option<&CombatIntent>,
            Option<&CombatTargetLock>,
            Option<&mut CombatThinkTimer>,
        ),
        With<Mob>,
    >,
    pack_mobs: Query<(Entity, &Transform), With<Mob>>,
    players: Query<(Entity, &Transform, &Faction), With<Unit>>,
    buildings: Query<
        (Entity, &Transform, &Faction),
        (With<Building>, Without<Unit>, Without<FloorTile>),
    >,
) {
    let now = time.elapsed_secs_f64();
    for (mob_entity, mob_tf, aggro, intent, lock, opt_think_timer) in &mut mobs {
        if budget_state.target_rescans_this_frame >= combat_budget.max_target_rescans_per_frame {
            continue;
        }
        if opt_think_timer.is_some_and(|timer| now < timer.next_think_at) {
            continue;
        }
        if matches!(intent, Some(CombatIntent::Attack(_, _)))
            && lock.is_some_and(|lock| now <= lock.locked_until)
        {
            continue;
        }
        // Prefer units over buildings: only target buildings if no unit is in range
        let mut closest_unit_dist = f32::MAX;
        let mut closest_unit = None;
        let mut closest_building_dist = f32::MAX;
        let mut closest_building = None;

        for (player_entity, player_tf, faction) in &players {
            if *faction == Faction::Neutral {
                continue;
            }
            let dist = mob_tf.translation.distance(player_tf.translation);
            if dist < aggro.0 && dist < closest_unit_dist {
                closest_unit_dist = dist;
                closest_unit = Some(player_entity);
            }
        }

        for (building_entity, building_tf, faction) in &buildings {
            if *faction == Faction::Neutral {
                continue;
            }
            let dist = mob_tf.translation.distance(building_tf.translation);
            if dist < aggro.0 && dist < closest_building_dist {
                closest_building_dist = dist;
                closest_building = Some(building_entity);
            }
        }

        // Strongly prefer units — only target buildings if no unit is within aggro range
        let target = closest_unit.or(closest_building);
        budget_state.target_rescans_this_frame += 1;

        if let Some(target) = target {
            apply_auto_attack_intent(&mut commands, mob_entity, target, mob_tf.translation, now);
            commands.entity(mob_entity).insert(CombatThinkTimer {
                next_think_at: now + 0.5 + (mob_entity.to_bits() % 5) as f64 * 0.03,
                interval_secs: 0.5,
            });

            // Pack aggro: alert nearby mobs to chase the same target
            let mob_pos = mob_tf.translation;
            let mut alerted = 0;
            for (other_entity, other_tf) in &pack_mobs {
                if other_entity == mob_entity {
                    continue;
                }
                if mob_pos.distance(other_tf.translation) < 15.0 && alerted < 4 {
                    apply_auto_attack_intent(
                        &mut commands,
                        other_entity,
                        target,
                        other_tf.translation,
                        now,
                    );
                    commands.entity(other_entity).insert(CombatThinkTimer {
                        next_think_at: now + 0.75,
                        interval_secs: 0.5,
                    });
                    alerted += 1;
                }
            }
        } else {
            commands.entity(mob_entity).insert(CombatThinkTimer {
                next_think_at: now + 0.5 + (mob_entity.to_bits() % 5) as f64 * 0.03,
                interval_secs: 0.5,
            });
        }
    }
}

/// Leash system: checks if a chasing mob has wandered too far from camp or chased
/// too long, and returns it home. Also updates PatrolState for procedural animations.
/// All movement and attacking is handled by the shared combat pipeline
/// (approach_attack_target → start_attack_windups → resolve_attack_windups).
fn mob_leash(
    mut commands: Commands,
    time: Res<Time>,
    mut mobs: Query<
        (
            Entity,
            &Transform,
            &mut PatrolState,
            Option<&CombatIntent>,
            Option<&CombatTargetLock>,
            Option<&AttackWindup>,
            Option<&AttackRecovery>,
        ),
        With<Mob>,
    >,
    targets: Query<&Transform, Without<Mob>>,
) {
    const LEASH_DISTANCE: f32 = 40.0;
    const MAX_CHASE_SECS: f32 = 15.0;

    for (mob_entity, tf, mut patrol, intent, lock, windup, recovery) in &mut mobs {
        let target_entity = match intent {
            Some(CombatIntent::Attack(target, _)) => Some(*target),
            Some(CombatIntent::AttackMove(_, _)) => lock.map(|lock| lock.target),
            _ => lock.map(|lock| lock.target),
        };
        if target_entity.is_none() {
            if matches!(
                patrol.state,
                PatrolStateKind::Chasing | PatrolStateKind::Attacking
            ) {
                patrol.state = PatrolStateKind::Returning;
            }
            continue;
        }
        patrol.chase_elapsed += time.delta_secs();

        let dist_from_home = Vec2::new(
            tf.translation.x - patrol.center.x,
            tf.translation.z - patrol.center.z,
        )
        .length();

        // Target gone or leash exceeded → return home
        let target_gone = target_entity.is_some_and(|target| !targets.contains(target));
        if target_gone || dist_from_home > LEASH_DISTANCE || patrol.chase_elapsed > MAX_CHASE_SECS {
            patrol.state = PatrolStateKind::Returning;
            patrol.chase_elapsed = 0.0;
            reset_combat_state(&mut commands, mob_entity);
            commands
                .entity(mob_entity)
                .remove::<MoveTarget>()
                .remove::<ChaseTimer>();
            continue;
        }

        // Update patrol state for procedural animations
        if windup.is_some() || recovery.is_some() {
            patrol.state = PatrolStateKind::Attacking;
        } else {
            patrol.state = PatrolStateKind::Chasing;
        }
    }
}
