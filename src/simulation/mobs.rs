use bevy::prelude::*;
use bevy::time::Fixed;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;

use crate::blueprints::{spawn_from_blueprint, BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::presentation::model_assets::UnitModelAssets;
use crate::simulation::combat::{
    apply_auto_attack_intent, reset_combat_state, CombatBudgetState, CombatCoreSet,
};
use crate::simulation::items::ItemKind;
use crate::types::*;
use crate::world::ground::{is_in_mountain_border, BorderSettings, HeightMap};
use crate::world::lighting::{DayCycle, DayPhase};

// ── Night-wave tuning ────────────────────────────────────────────────────
//
// Changing these values changes simulation output. Saves / replays / lockstep
// multiplayer sessions started before a change will diverge from ones started
// after, because wave sizes and cluster layouts depend on them.

const CLUSTER_MIN: usize = 3;
const CLUSTER_MAX: usize = 5;
const MIN_CLUSTER_SPACING: f32 = 40.0;
const MAX_SAMPLE_ATTEMPTS_PER_MOB: u32 = 16;

/// Golden-ratio constant (2^64 / φ), a well-studied multiplier for mixing
/// integer seeds into well-distributed outputs.
const PHI_U64: u64 = 0x9E3779B97F4A7C15;

pub struct MobsPlugin;

impl Plugin for MobsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NightWaveState>()
            .add_systems(
                FixedUpdate,
                night_wave_spawn_system
                    .in_set(SimSet::Ai)
                    .before(CombatCoreSet::Approach)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (mob_patrol, mob_aggro, mob_leash)
                    .chain()
                    .in_set(SimSet::Ai)
                    .after(night_wave_spawn_system)
                    .before(CombatCoreSet::Approach)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// ── Night wave state ─────────────────────────────────────────────────────

/// Authoritative, deterministic state for the escalating night-wave system.
///
/// All fields are read on every peer each FixedUpdate tick. Debug-tweak
/// writes into this resource are single-player only (see
/// `sync_night_wave_tweaks`) to avoid desync in lockstep multiplayer.
#[derive(Resource)]
pub struct NightWaveState {
    pub enabled: bool,
    pub night_count: u32,
    pub prev_phase: DayPhase,
    pub last_wave_size: u32,
    pub last_wave_tick: u64,
    pub base_count: f32,
    pub growth_per_night: f32,
    pub min_player_dist: f32,
    pub force_wave: bool,
}

impl Default for NightWaveState {
    fn default() -> Self {
        Self {
            enabled: true,
            night_count: 0,
            prev_phase: DayPhase::Day,
            last_wave_size: 0,
            last_wave_tick: 0,
            base_count: 6.0,
            growth_per_night: 3.0,
            min_player_dist: 60.0,
            force_wave: false,
        }
    }
}

#[derive(Component, Clone)]
pub struct CampItemDrops {
    pub items: Vec<ItemKind>,
}

// ── Spawn sampling ───────────────────────────────────────────────────────

struct ClusterSpawn {
    center: Vec3,
    size: usize,
}

fn generate_wave_clusters(
    rng: &mut StdRng,
    half_map: f32,
    total_mobs: u32,
    min_player_dist: f32,
    player_positions: &[Vec3],
    biome_map: &BiomeMap,
    border: BorderSettings,
) -> Vec<ClusterSpawn> {
    let mut clusters: Vec<ClusterSpawn> = Vec::new();
    let min_player_dist_sq = min_player_dist * min_player_dist;
    let min_cluster_dist_sq = MIN_CLUSTER_SPACING * MIN_CLUSTER_SPACING;

    let mut remaining = total_mobs as i32;
    while remaining > 0 {
        let size = rng
            .random_range(CLUSTER_MIN..=CLUSTER_MAX)
            .min(remaining as usize);
        let total_attempts = MAX_SAMPLE_ATTEMPTS_PER_MOB * size as u32;

        let mut placed = false;
        for _ in 0..total_attempts {
            let x = rng.random_range(-half_map..half_map);
            let z = rng.random_range(-half_map..half_map);

            if is_in_mountain_border(x, z, half_map, border) {
                continue;
            }
            if biome_map.get_biome(x, z) == Biome::Water {
                continue;
            }

            let too_close_player = player_positions.iter().any(|p| {
                let dx = x - p.x;
                let dz = z - p.z;
                dx * dx + dz * dz < min_player_dist_sq
            });
            if too_close_player {
                continue;
            }

            let too_close_cluster = clusters.iter().any(|c| {
                let dx = x - c.center.x;
                let dz = z - c.center.z;
                dx * dx + dz * dz < min_cluster_dist_sq
            });
            if too_close_cluster {
                continue;
            }

            clusters.push(ClusterSpawn {
                center: Vec3::new(x, 0.0, z),
                size,
            });
            remaining -= size as i32;
            placed = true;
            break;
        }

        if !placed {
            // Map is too crowded for this wave — bail out and take what we got.
            break;
        }
    }

    clusters
}

/// Returns `Some(night_count)` if a wave should spawn this tick, else `None`.
/// Always updates `prev_phase` and clears `force_wave`, so the detector stays
/// idempotent even when the system is disabled mid-game.
fn resolve_wave_trigger(wave: &mut NightWaveState, phase: DayPhase) -> Option<u32> {
    let prev = wave.prev_phase;
    wave.prev_phase = phase;

    let entered_night = prev != DayPhase::Night && phase == DayPhase::Night;
    let force = std::mem::take(&mut wave.force_wave);

    if !wave.enabled {
        return None;
    }
    if !entered_night && !force {
        return None;
    }

    wave.night_count += 1;
    Some(wave.night_count)
}

/// Collect positions of all living player-owned units and buildings. Used
/// as an exclusion set when sampling spawn points. Iteration order doesn't
/// affect determinism because consumers use `any()` / distance checks.
fn gather_exclusion_points(
    unit_tfs: &Query<(&Transform, &Faction), With<Unit>>,
    building_tfs: &Query<(&Transform, &Faction), (With<Building>, Without<Unit>, Without<FloorTile>)>,
) -> Vec<Vec3> {
    let mut out = Vec::new();
    for (tf, faction) in unit_tfs.iter() {
        if *faction != Faction::Neutral {
            out.push(tf.translation);
        }
    }
    for (tf, faction) in building_tfs.iter() {
        if *faction != Faction::Neutral {
            out.push(tf.translation);
        }
    }
    out
}

// ── Spawn system ─────────────────────────────────────────────────────────

fn night_wave_spawn_system(
    mut commands: Commands,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    unit_models: Option<Res<UnitModelAssets>>,
    height_map: Option<Res<HeightMap>>,
    biome_map: Option<Res<BiomeMap>>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
    day_cycle: Res<DayCycle>,
    sim_clock: Res<SimClock>,
    unit_tfs: Query<(&Transform, &Faction), With<Unit>>,
    building_tfs: Query<(&Transform, &Faction), (With<Building>, Without<Unit>, Without<FloorTile>)>,
    mut wave: ResMut<NightWaveState>,
) {
    let Some(night_num) = resolve_wave_trigger(&mut wave, day_cycle.phase) else {
        return;
    };
    let (Some(height_map), Some(biome_map)) = (height_map, biome_map) else {
        return;
    };

    let mut rng = StdRng::seed_from_u64(map_seed.0 ^ (night_num as u64).wrapping_mul(PHI_U64));

    let total = (wave.base_count + wave.growth_per_night * night_num as f32)
        .max(0.0)
        .round() as u32;

    if total == 0 {
        wave.last_wave_size = 0;
        wave.last_wave_tick = sim_clock.tick;
        return;
    }

    let player_positions = gather_exclusion_points(&unit_tfs, &building_tfs);
    let half_map = config.map_size.world_size() / 2.0;
    let border = BorderSettings::from_map_size(config.map_size.world_size());

    let clusters = generate_wave_clusters(
        &mut rng,
        half_map,
        total,
        wave.min_player_dist,
        &player_positions,
        &biome_map,
        border,
    );

    let mut spawned: u32 = 0;
    for cluster in &clusters {
        spawned += spawn_night_cluster(
            &mut commands,
            &cache,
            &registry,
            unit_models.as_deref(),
            &height_map,
            cluster,
            &mut rng,
        );
    }

    wave.last_wave_size = spawned;
    wave.last_wave_tick = sim_clock.tick;
}

fn spawn_night_cluster(
    commands: &mut Commands,
    cache: &EntityVisualCache,
    registry: &BlueprintRegistry,
    unit_models: Option<&UnitModelAssets>,
    height_map: &HeightMap,
    cluster: &ClusterSpawn,
    rng: &mut StdRng,
) -> u32 {
    let center = Vec3::new(
        cluster.center.x,
        height_map.sample(cluster.center.x, cluster.center.z),
        cluster.center.z,
    );

    let bp = registry.get(EntityKind::Goblin);
    let base_patrol = bp
        .mob_ai
        .as_ref()
        .map(|ai| ai.patrol_radius)
        .unwrap_or(12.0);
    let patrol_radius = base_patrol + cluster.size as f32 * 0.6;

    let reward = cluster_reward();
    let drops = cluster_item_drops(rng);

    for i in 0..cluster.size {
        let angle = (i as f32 / cluster.size as f32) * std::f32::consts::TAU;
        let offset_r = patrol_radius * 0.35;
        let x = center.x + angle.cos() * offset_r;
        let z = center.z + angle.sin() * offset_r;

        let entity = spawn_mob_member(
            commands,
            cache,
            registry,
            unit_models,
            height_map,
            EntityKind::Goblin,
            Vec3::new(x, 0.0, z),
            center,
            patrol_radius,
        );

        if i == 0 {
            commands.entity(entity).insert((reward.clone(), drops.clone()));
        }
    }

    cluster.size as u32
}

fn spawn_mob_member(
    commands: &mut Commands,
    cache: &EntityVisualCache,
    registry: &BlueprintRegistry,
    unit_models: Option<&UnitModelAssets>,
    height_map: &HeightMap,
    kind: EntityKind,
    pos: Vec3,
    patrol_center: Vec3,
    patrol_radius: f32,
) -> Entity {
    let y_off = registry
        .get(kind)
        .movement
        .as_ref()
        .map(|m| m.y_offset)
        .unwrap_or(0.0);

    let entity = spawn_from_blueprint(
        commands,
        cache,
        kind,
        pos,
        registry,
        None,
        unit_models,
        height_map,
    );

    commands.entity(entity).insert((
        PatrolState {
            state: PatrolStateKind::Idle,
            center: patrol_center,
            radius: patrol_radius,
            patrol_target: None,
            chase_elapsed: 0.0,
            idle_until: 0.0,
            patrol_started: 0.0,
        },
        Transform::from_translation(Vec3::new(
            pos.x,
            height_map.sample(pos.x, pos.z) + y_off,
            pos.z,
        ))
        .with_scale(Vec3::splat(1.0)),
    ));

    entity
}

fn cluster_reward() -> CampReward {
    use crate::blueprints::ResourceCost;
    CampReward {
        resources: ResourceCost::new()
            .with(ResourceType::Wood, 30)
            .with(ResourceType::Copper, 15),
    }
}

fn cluster_item_drops(rng: &mut StdRng) -> CampItemDrops {
    use ItemKind::*;
    let pool: &[ItemKind] = &[
        PaddedVest,
        KettleHelm,
        PlainBand,
        ArmingSword,
        BattleStaff,
        YewLongbow,
    ];

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
    time: Res<Time<Fixed>>,
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
        if matches!(
            intent,
            Some(CombatIntent::Attack(_, _) | CombatIntent::AttackMove(_, _))
        ) || lock.is_some()
        {
            continue;
        }

        match patrol.state {
            PatrolStateKind::Idle => {
                if move_target.is_some() {
                    commands.entity(entity).remove::<MoveTarget>();
                }

                if now < patrol.idle_until {
                    continue;
                }

                let bits = entity.to_bits();
                let seed = bits.wrapping_mul(2654435761) as f32;
                let mut target = None;
                for attempt in 0..4u32 {
                    let angle =
                        seed % std::f32::consts::TAU + (now as f32) * 0.3 + attempt as f32 * 1.57;
                    let r = patrol.radius
                        * (0.3 + ((seed * 0.7 + attempt as f32).sin() * 0.5 + 0.5) * 0.7);
                    let candidate = Vec3::new(
                        patrol.center.x + angle.cos() * r,
                        0.0,
                        patrol.center.z + angle.sin() * r,
                    );

                    let passable = nav_grid.as_ref().map_or(true, |grid| {
                        grid.is_world_passable(candidate.x, candidate.z)
                    });
                    if passable {
                        target = Some(candidate);
                        break;
                    }
                }

                let Some(target) = target else {
                    patrol.idle_until = now + 3.0;
                    continue;
                };

                patrol.patrol_target = Some(target);
                patrol.patrol_started = now;
                patrol.state = PatrolStateKind::Patrolling;

                commands.entity(entity).insert(MoveTarget(target));
            }
            PatrolStateKind::Patrolling => {
                if let Some(target) = patrol.patrol_target {
                    let dist = Vec2::new(target.x - tf.translation.x, target.z - tf.translation.z)
                        .length();

                    if dist < 2.5 || (now - patrol.patrol_started) > PATROL_STUCK_SECS {
                        patrol.state = PatrolStateKind::Idle;
                        patrol.patrol_target = None;
                        let idle_extra = (entity.to_bits() % 3000) as f64 / 1000.0;
                        patrol.idle_until = now + 2.0 + idle_extra;
                        commands.entity(entity).remove::<MoveTarget>();
                    } else if move_target.is_none() {
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
                    commands.entity(entity).insert(MoveTarget(patrol.center));
                }
            }
            _ => {}
        }
    }
}

fn mob_aggro(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    combat_budget: Res<CombatBudget>,
    mut budget_state: ResMut<CombatBudgetState>,
    spatial: Res<crate::world::spatial::SpatialHashGrid>,
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
    unit_factions: Query<&Faction, With<Unit>>,
    building_factions: Query<
        &Faction,
        (With<Building>, Without<Unit>, Without<FloorTile>),
    >,
    mob_marker: Query<(), With<Mob>>,
) {
    const PACK_ALERT_RADIUS: f32 = 15.0;
    const PACK_ALERT_MAX: usize = 4;

    let now = time.elapsed_secs_f64();
    let mut candidates: Vec<(Entity, Vec3)> = Vec::new();
    let mut pack_buf: Vec<(Entity, Vec3)> = Vec::new();

    for (mob_entity, mob_tf, aggro, intent, lock, _) in &mut mobs {
        if budget_state.target_rescans_this_frame >= combat_budget.max_target_rescans_per_frame {
            continue;
        }
        if matches!(intent, Some(CombatIntent::Attack(_, _)))
            && lock.is_some_and(|lock| now <= lock.locked_until)
        {
            continue;
        }
        let mob_pos = mob_tf.translation;

        // Spatial radius query + faction filter. The grid contains units,
        // mobs, and non-floor buildings — the closure keeps only non-neutral
        // units and buildings (mobs are Neutral so they're filtered out too).
        spatial.collect_radius_filter(mob_pos, aggro.0, &mut candidates, |entity| {
            if let Ok(faction) = unit_factions.get(entity) {
                return *faction != Faction::Neutral;
            }
            if let Ok(faction) = building_factions.get(entity) {
                return *faction != Faction::Neutral;
            }
            false
        });

        let mut closest_unit_dist_sq = f32::MAX;
        let mut closest_unit: Option<Entity> = None;
        let mut closest_building_dist_sq = f32::MAX;
        let mut closest_building: Option<Entity> = None;

        for (entity, pos) in candidates.iter().copied() {
            let d_sq = mob_pos.distance_squared(pos);
            if unit_factions.contains(entity) {
                if d_sq < closest_unit_dist_sq {
                    closest_unit_dist_sq = d_sq;
                    closest_unit = Some(entity);
                }
            } else if building_factions.contains(entity)
                && d_sq < closest_building_dist_sq
            {
                closest_building_dist_sq = d_sq;
                closest_building = Some(entity);
            }
        }

        let target = closest_unit.or(closest_building);
        budget_state.target_rescans_this_frame += 1;

        if let Some(target) = target {
            apply_auto_attack_intent(&mut commands, mob_entity, target, mob_pos, now);

            spatial.collect_radius_filter(
                mob_pos,
                PACK_ALERT_RADIUS,
                &mut pack_buf,
                |entity| entity != mob_entity && mob_marker.contains(entity),
            );
            for (other_entity, other_pos) in pack_buf.iter().take(PACK_ALERT_MAX).copied() {
                apply_auto_attack_intent(&mut commands, other_entity, target, other_pos, now);
            }
        }
    }
}

fn mob_leash(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
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

        if windup.is_some() || recovery.is_some() {
            patrol.state = PatrolStateKind::Attacking;
        } else {
            patrol.state = PatrolStateKind::Chasing;
        }
    }
}
