//! Tower-defense night raid system.
//!
//! At Dusk, a `WaveAlert` is emitted for the upcoming Night with a count and
//! direction. At Night entry, `ActiveWave` is set up; throughout the night,
//! mobs drip-spawn one at a time at the map perimeter (biased toward the
//! wave direction). Each mob picks a primary target on spawn (building > unit),
//! then commits to combat. On the Night→Dawn transition, the active wave is
//! cleared, surviving mobs are ordered to retreat off-map, and a per-night
//! clear bonus is awarded if the player's Base survived.
//!
//! Determinism: every RNG used by spawn/composition/loot is seeded from
//! `MapSeed ^ night_count.mul(PHI_U64)`; per-mob substreams are derived from
//! the same seed using the entity-bits trick. No `rand::rng()`, no
//! `Instant::now()`, no HashMap iteration in this module.

use bevy::prelude::*;
use bevy::time::Fixed;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::Rng;
use rand::SeedableRng;

use crate::blueprints::{spawn_from_blueprint, BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::presentation::model_assets::UnitModelAssets;
use crate::simulation::combat::{apply_auto_attack_intent, apply_auto_move_intent, CombatSet};
use crate::simulation::items::{ItemKind, SpawnItemPickup};
use crate::types::*;
use crate::ui::event_log_widget::{EventCategory, GameEventLog, LogLevel};
use crate::world::ground::{is_in_mountain_border, BorderSettings, HeightMap};
use crate::world::lighting::{DayCycle, DayPhase};

// ── Constants ────────────────────────────────────────────────────────────

/// Golden-ratio constant (2^64 / φ), a well-distributed multiplier for
/// mixing integer seeds.
const PHI_U64: u64 = 0x9E3779B97F4A7C15;

/// FixedUpdate runs at 30 Hz in this project.
const TICKS_PER_SEC: u64 = 30;

/// Hard cap on mob rescans across a single mob's lifetime (one for the
/// initial primary death, one fallback).
const MAX_RESCANS_PER_MOB: u8 = 2;

// Note: rescan radius for retarget-on-death lives in `combat/death.rs`
// because the rescan runs there, not here.

/// Hard despawn timeout once a stranded mob enters retreat (kept aside in
/// case future code wants a different timeout for stranded vs. dawn retreats).
const STRAND_TIMEOUT_SECS: f32 = 10.0;

/// Hard timeout for retreat at Dawn — mobs still on-map after this are
/// despawned outright.
const RETREAT_HARD_TIMEOUT_SECS: f32 = 30.0;

/// Bias for building targets in the spawn-time scoring heuristic.
const BUILDING_BIAS: f32 = -10.0;
const STRATEGIC_BUILDING_BIAS: f32 = -20.0;

// ── Plugin ───────────────────────────────────────────────────────────────

pub struct MobsPlugin;

impl Plugin for MobsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NightWaveState>()
            .add_message::<WaveAlert>()
            .add_systems(
                FixedUpdate,
                (
                    on_dusk_compose_wave,
                    on_dawn_clear_wave,
                    night_wave_drip_spawn,
                    mob_recommit_to_primary,
                    mob_strand_timeout,
                    mob_retreat_despawn,
                )
                    .chain()
                    .in_set(SimSet::Ai)
                    .before(CombatSet::Approach)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// ── Wave state ───────────────────────────────────────────────────────────

/// Persistent state across the whole match. `active` is `Some(_)` only while
/// a Night is in progress.
#[derive(Resource)]
pub struct NightWaveState {
    pub enabled: bool,
    pub night_count: u32,
    /// True iff Dusk has fired and a wave is queued for the next Night.
    pub dusk_pending: bool,
    /// What the next-night wave will look like; populated at Dusk, consumed
    /// when Night begins.
    pub queued: Option<QueuedWave>,
    pub active: Option<ActiveWave>,
    /// Debug tweak: if set, force wave on the next FixedUpdate regardless of
    /// phase. Used by `DebugTweaks`.
    pub force_wave: bool,
}

impl Default for NightWaveState {
    fn default() -> Self {
        Self {
            enabled: true,
            night_count: 0,
            dusk_pending: false,
            queued: None,
            active: None,
            force_wave: false,
        }
    }
}

/// Wave plan computed at Dusk, instantiated at Night entry.
#[derive(Clone, Debug)]
pub struct QueuedWave {
    pub night: u32,
    pub total: u32,
    pub spawn_interval_ticks: u64,
    pub direction: WaveDirection,
    /// Tier per spawn slot, indexed 0..total. Pre-shuffled so champions don't
    /// always arrive last.
    pub tier_order: Vec<MobTier>,
    pub seed: u64,
}

#[derive(Debug)]
pub struct ActiveWave {
    pub night: u32,
    pub total: u32,
    pub spawned: u32,
    pub killed: u32,
    pub buildings_damaged: u32,
    pub direction: WaveDirection,
    pub spawn_interval_ticks: u64,
    pub next_spawn_tick: u64,
    pub tier_order: Vec<MobTier>,
    pub seed: u64,
}

impl ActiveWave {
    fn from_queued(queued: QueuedWave, now_tick: u64) -> Self {
        Self {
            night: queued.night,
            total: queued.total,
            spawned: 0,
            killed: 0,
            buildings_damaged: 0,
            direction: queued.direction,
            spawn_interval_ticks: queued.spawn_interval_ticks,
            // First mob spawns immediately on Night entry.
            next_spawn_tick: now_tick,
            tier_order: queued.tier_order,
            seed: queued.seed,
        }
    }
}

// ── Wave composition ─────────────────────────────────────────────────────

/// Total mobs and spawn cadence for a given night number. Plain hardcoded
/// table — easy to tune without touching the spawn logic.
fn compose_wave_count_and_cadence(night: u32) -> (u32, u64) {
    let total = (8 + night.saturating_mul(2)).min(80);
    // Interval ramps from 240t (8s) down to 60t (2s), reaching minimum at
    // night 15.
    let interval = 240u64.saturating_sub((night as u64).saturating_mul(12)).max(60);
    (total, interval)
}

/// Tier composition table: returns (runner_pct, veteran_pct, champion_pct)
/// summing to 100. Veterans appear from night 3, Champions from night 6.
fn tier_composition(night: u32) -> (u8, u8, u8) {
    match night {
        0..=2 => (100, 0, 0),
        3..=5 => (70, 30, 0),
        6..=8 => (50, 35, 15),
        _ => (30, 45, 25),
    }
}

fn compose_tier_order(rng: &mut StdRng, total: u32, night: u32) -> Vec<MobTier> {
    let (runner_pct, veteran_pct, _champion_pct) = tier_composition(night);
    let total_usize = total as usize;
    let runners = (total_usize * runner_pct as usize + 50) / 100;
    let veterans = (total_usize * veteran_pct as usize + 50) / 100;
    // Champions take whatever's left so percentages always sum.
    let champions = total_usize.saturating_sub(runners + veterans);

    let mut order: Vec<MobTier> = Vec::with_capacity(total_usize);
    order.extend(std::iter::repeat(MobTier::Runner).take(runners));
    order.extend(std::iter::repeat(MobTier::Veteran).take(veterans));
    order.extend(std::iter::repeat(MobTier::Champion).take(champions));
    // Shuffle so high tiers don't all arrive last.
    order.shuffle(rng);
    order
}

fn pick_wave_direction(rng: &mut StdRng) -> WaveDirection {
    // 25% chance of "all sides" wave; otherwise pick one of the four cardinals.
    if rng.random_range(0..4u32) == 0 {
        WaveDirection::All
    } else {
        match rng.random_range(0..4u32) {
            0 => WaveDirection::North,
            1 => WaveDirection::East,
            2 => WaveDirection::South,
            _ => WaveDirection::West,
        }
    }
}

// ── Dusk: compose & emit alert ───────────────────────────────────────────

fn on_dusk_compose_wave(
    mut wave: ResMut<NightWaveState>,
    map_seed: Res<MapSeed>,
    mut dusk_rx: MessageReader<DuskBegan>,
    mut wave_alert_tx: MessageWriter<WaveAlert>,
    mut event_log: ResMut<GameEventLog>,
    time: Res<Time<Fixed>>,
) {
    let mut fire = dusk_rx.read().count() > 0;
    if wave.force_wave {
        fire = true;
        wave.force_wave = false;
    }
    if !fire || !wave.enabled {
        return;
    }

    let next_night = wave.night_count + 1;
    let seed = map_seed
        .0
        .wrapping_mul(1)
        ^ (next_night as u64).wrapping_mul(PHI_U64);
    let mut rng = StdRng::seed_from_u64(seed);

    let (total, interval_ticks) = compose_wave_count_and_cadence(next_night);
    let direction = pick_wave_direction(&mut rng);
    let tier_order = compose_tier_order(&mut rng, total, next_night);

    let queued = QueuedWave {
        night: next_night,
        total,
        spawn_interval_ticks: interval_ticks,
        direction,
        tier_order,
        seed,
    };

    let alert = WaveAlert {
        night: next_night,
        incoming_count: total,
        direction,
    };
    wave_alert_tx.write(alert);
    event_log.push_with_level(
        time.elapsed_secs(),
        format!(
            "Night {} approaching — {} attackers from the {}",
            next_night,
            total,
            direction.label()
        ),
        EventCategory::Alert,
        LogLevel::Warning,
        None,
        None,
    );

    wave.queued = Some(queued);
    wave.dusk_pending = true;
}

// ── Dawn: clear active, retreat survivors, post summary ──────────────────

#[allow(clippy::too_many_arguments)]
fn on_dawn_clear_wave(
    mut commands: Commands,
    mut wave: ResMut<NightWaveState>,
    mut dawn_rx: MessageReader<DawnBegan>,
    mut event_log: ResMut<GameEventLog>,
    time: Res<Time<Fixed>>,
    sim_clock: Res<SimClock>,
    config: Res<GameSetupConfig>,
    surviving_mobs: Query<(Entity, &Transform), With<Mob>>,
    mut all_resources: ResMut<AllPlayerResources>,
    bases: Query<&Faction, (With<Building>, Without<Unit>, Without<FloorTile>)>,
) {
    if dawn_rx.read().count() == 0 {
        return;
    }
    let Some(active) = wave.active.take() else {
        return;
    };

    let half_map = config.map_size.world_size() / 2.0;
    for (mob_entity, mob_tf) in surviving_mobs.iter() {
        let pos = mob_tf.translation;
        // Project the mob's XZ direction outward to the nearest edge.
        let edge = nearest_map_edge_point(Vec2::new(pos.x, pos.z), half_map);
        let edge_world = Vec3::new(edge.x, pos.y, edge.y);
        apply_auto_move_intent(&mut commands, mob_entity, edge_world);
        commands.entity(mob_entity).insert(RetreatingMob {
            started_at_tick: sim_clock.tick,
        });
    }

    // Per-night clear bonus: small reward to every player whose base survived.
    let mut alive_factions: Vec<Faction> = Vec::new();
    for faction in bases.iter() {
        if matches!(faction, Faction::Neutral) {
            continue;
        }
        if !alive_factions.contains(faction) {
            alive_factions.push(*faction);
        }
    }
    for f in &alive_factions {
        if let Some(res) = all_resources.resources.get_mut(f) {
            res.add(ResourceType::Gold, 50);
            res.add(ResourceType::Wood, 30);
        }
        event_log.push(
            time.elapsed_secs(),
            format!(
                "Night {} survived — {} killed, {} buildings damaged. Bonus: 50 Gold, 30 Wood",
                active.night, active.killed, active.buildings_damaged
            ),
            EventCategory::Alert,
            None,
            Some(*f),
        );
    }

    // Push a global notification toast as well so the local player sees it
    // even when `bases` returns nothing for them yet.
    commands.queue(move |world: &mut World| {
        let now = world.resource::<Time>().elapsed_secs();
        let mut notifications = world.resource_mut::<AllyNotifications>();
        notifications.push(
            AllyNotifyKind::Attacking,
            format!(
                "Night {} survived — {} killed",
                active.night, active.killed
            ),
            None,
            now,
        );
    });
}

fn nearest_map_edge_point(pos: Vec2, half_map: f32) -> Vec2 {
    // Pick whichever map edge is closest along the dominant axis.
    let dist_east = half_map - pos.x;
    let dist_west = pos.x + half_map;
    let dist_south = half_map - pos.y;
    let dist_north = pos.y + half_map;
    // Use a margin of 1.0 inside the edge so the unit reaches the border
    // without the path planner refusing the goal.
    let margin = 1.0_f32;
    let m = dist_east.min(dist_west).min(dist_south).min(dist_north);
    if (m - dist_east).abs() < 0.001 {
        Vec2::new(half_map - margin, pos.y)
    } else if (m - dist_west).abs() < 0.001 {
        Vec2::new(-half_map + margin, pos.y)
    } else if (m - dist_south).abs() < 0.001 {
        Vec2::new(pos.x, half_map - margin)
    } else {
        Vec2::new(pos.x, -half_map + margin)
    }
}

// ── Drip spawn ───────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn night_wave_drip_spawn(
    mut commands: Commands,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    unit_models: Option<Res<UnitModelAssets>>,
    height_map: Option<Res<HeightMap>>,
    biome_map: Option<Res<BiomeMap>>,
    config: Res<GameSetupConfig>,
    day_cycle: Res<DayCycle>,
    sim_clock: Res<SimClock>,
    time: Res<Time<Fixed>>,
    mut wave: ResMut<NightWaveState>,
    target_factions: Query<&Faction>,
    target_units: Query<(Entity, &Transform, &Faction), With<Unit>>,
    target_buildings: Query<
        (Entity, &Transform, &Faction, Option<&EntityKind>),
        (With<Building>, Without<Unit>, Without<FloorTile>),
    >,
    mut event_log: ResMut<GameEventLog>,
) {
    if !wave.enabled {
        return;
    }
    let (Some(height_map), Some(biome_map)) = (height_map, biome_map) else {
        return;
    };

    // Activate queued wave on Night entry.
    if matches!(day_cycle.phase, DayPhase::Night) && wave.active.is_none() {
        if let Some(queued) = wave.queued.take() {
            wave.night_count = queued.night;
            wave.dusk_pending = false;
            wave.active = Some(ActiveWave::from_queued(queued, sim_clock.tick));
        }
    }

    // Outside Night, no spawning.
    if !matches!(day_cycle.phase, DayPhase::Night) {
        return;
    }

    let Some(active) = wave.active.as_mut() else {
        return;
    };
    if active.spawned >= active.total {
        return;
    }
    if sim_clock.tick < active.next_spawn_tick {
        return;
    }

    let half_map = config.map_size.world_size() / 2.0;
    let border = BorderSettings::from_map_size(config.map_size.world_size());

    // Per-spawn RNG seeded from wave seed + spawn index — reproducible across
    // peers without depending on system iteration order.
    let spawn_seed = active.seed
        ^ (active.spawned as u64).wrapping_mul(0xD1B5_4A32_D192_ED03)
        ^ 0xFEED_FEED_FEED_FEED;
    let mut rng = StdRng::seed_from_u64(spawn_seed);

    let tier = active
        .tier_order
        .get(active.spawned as usize)
        .copied()
        .unwrap_or(MobTier::Runner);

    // Bias spawn angle toward the wave direction (70% inside the cone, 30%
    // scattered) unless direction is All.
    let angle = pick_spawn_angle(&mut rng, active.direction);
    let spawn_pos = pick_perimeter_spawn(
        &mut rng,
        angle,
        half_map,
        border,
        biome_map.as_ref(),
        height_map.as_ref(),
    );
    let Some(spawn_pos) = spawn_pos else {
        // Could not find a valid perimeter point this tick — push the
        // attempt forward by one interval and try again next tick.
        active.next_spawn_tick = sim_clock.tick + 1;
        return;
    };

    let entity = spawn_mob(
        &mut commands,
        &cache,
        &registry,
        unit_models.as_deref(),
        &height_map,
        spawn_pos,
        tier,
    );

    let now = time.elapsed_secs_f64();
    init_mob_engagement(
        &mut commands,
        entity,
        spawn_pos,
        now,
        &target_units,
        &target_buildings,
        &target_factions,
    );

    // Telegraph: log every 4th spawn so the player sees a steady stream
    // without spam.
    if active.spawned % 4 == 0 {
        event_log.push_with_level(
            time.elapsed_secs(),
            format!("Goblins emerging from the {}", active.direction.label()),
            EventCategory::Combat,
            LogLevel::Warning,
            Some(spawn_pos),
            None,
        );
    }

    active.spawned += 1;
    active.next_spawn_tick = sim_clock.tick + active.spawn_interval_ticks;
}

fn pick_spawn_angle(rng: &mut StdRng, dir: WaveDirection) -> f32 {
    use std::f32::consts::{PI, TAU};
    match dir.primary_angle() {
        Some(primary) => {
            // 70% inside ±π/4 of primary; 30% anywhere on perimeter.
            if rng.random_bool(0.7) {
                let jitter = rng.random_range(-PI * 0.25..PI * 0.25);
                (primary + jitter).rem_euclid(TAU)
            } else {
                rng.random_range(0.0..TAU)
            }
        }
        None => rng.random_range(0.0..TAU),
    }
}

fn pick_perimeter_spawn(
    rng: &mut StdRng,
    angle: f32,
    half_map: f32,
    border: BorderSettings,
    biome_map: &BiomeMap,
    _height_map: &HeightMap,
) -> Option<Vec3> {
    // Try the requested angle first, then nudge a few times if it's blocked.
    for attempt in 0..6u32 {
        let nudge = (attempt as f32) * 0.18 * if attempt % 2 == 0 { 1.0 } else { -1.0 };
        let a = angle + nudge;
        let radial = half_map - rng.random_range(2.0..5.0);
        let x = a.cos() * radial;
        let z = a.sin() * radial;
        if is_in_mountain_border(x, z, half_map, border) {
            continue;
        }
        if biome_map.get_biome(x, z) == Biome::Water {
            continue;
        }
        return Some(Vec3::new(x, 0.0, z));
    }
    None
}

// ── Target selection ─────────────────────────────────────────────────────

/// Pick the best initial target for a freshly spawned mob. Buildings outrank
/// units (with Base/Storage outranking other buildings); ties broken by
/// distance. Returns `None` only if the map has no hostile entities at all.
pub fn pick_primary_target(
    mob_pos: Vec3,
    units: &Query<(Entity, &Transform, &Faction), With<Unit>>,
    buildings: &Query<
        (Entity, &Transform, &Faction, Option<&EntityKind>),
        (With<Building>, Without<Unit>, Without<FloorTile>),
    >,
    _factions: &Query<&Faction>,
) -> Option<(Entity, EngagementTargetKind)> {
    let mut best_score = f32::MAX;
    let mut best: Option<(Entity, EngagementTargetKind)> = None;

    for (e, tf, faction, opt_kind) in buildings.iter() {
        if matches!(faction, Faction::Neutral) {
            continue;
        }
        let dist = mob_pos.distance(tf.translation);
        let kind_bonus = match opt_kind.copied() {
            Some(EntityKind::Base) | Some(EntityKind::Storage) => STRATEGIC_BUILDING_BIAS,
            Some(EntityKind::Sawmill)
            | Some(EntityKind::Mine)
            | Some(EntityKind::OilRig)
            | Some(EntityKind::Smelter)
            | Some(EntityKind::Alchemist) => BUILDING_BIAS - 2.0,
            Some(_) => BUILDING_BIAS,
            None => 0.0,
        };
        let score = dist + kind_bonus;
        if score < best_score {
            best_score = score;
            best = Some((e, EngagementTargetKind::Building));
        }
    }

    for (e, tf, faction, _) in units.iter().map(|(e, tf, f)| (e, tf, f, ())) {
        if matches!(faction, Faction::Neutral) {
            continue;
        }
        let dist = mob_pos.distance(tf.translation);
        let score = dist; // no bonus
        if score < best_score {
            best_score = score;
            best = Some((e, EngagementTargetKind::Unit));
        }
    }

    best
}

// ── Re-commit hook ───────────────────────────────────────────────────────

/// After a detour fight ends (intercepting unit dies or runs off, mob's
/// brain returns to Idle with no target), re-issue the original Attack
/// order so the mob resumes its march.
#[allow(clippy::too_many_arguments)]
fn mob_recommit_to_primary(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mobs: Query<
        (
            Entity,
            &Transform,
            &MobEngagement,
            &crate::simulation::combat::UnitBrain,
            Option<&RecentCombatDamage>,
        ),
        (With<Mob>, Without<RetreatingMob>),
    >,
    targets_alive: Query<&Health>,
) {
    use crate::simulation::combat::{BrainState, Order};
    let now = time.elapsed_secs_f64();

    for (entity, tf, engagement, brain, recent_damage) in mobs.iter() {
        // Don't disturb brains that are in any active combat phase.
        if !matches!(brain.state, BrainState::Idle) {
            continue;
        }
        // If we just got hit, retaliation will handle the response — don't
        // override.
        if recent_damage.is_some() {
            continue;
        }
        // Skip if we're already attacking the primary.
        if matches!(brain.order, Order::Attack(t) if t == engagement.primary) {
            continue;
        }
        // Primary must still be alive. Death-handling re-targets are done in
        // `mob_retarget_on_primary_death` (in death.rs); this hook only
        // re-asserts the existing primary.
        let alive = targets_alive
            .get(engagement.primary)
            .map(|h| h.current > 0.0)
            .unwrap_or(false);
        if !alive {
            continue;
        }
        apply_auto_attack_intent(&mut commands, entity, engagement.primary, tf.translation, now);
    }
}

/// Mobs that exhausted their rescans and have nothing left to do walk off
/// the map edge. The retreat-despawn system tears them down on edge crossing
/// or after `RETREAT_HARD_TIMEOUT_SECS`.
fn mob_strand_timeout(
    mut commands: Commands,
    sim_clock: Res<SimClock>,
    config: Res<GameSetupConfig>,
    mobs: Query<
        (
            Entity,
            &Transform,
            &MobEngagement,
            &crate::simulation::combat::UnitBrain,
        ),
        (With<Mob>, Without<RetreatingMob>),
    >,
) {
    use crate::simulation::combat::{BrainState, Order};
    let half_map = config.map_size.world_size() / 2.0;
    for (entity, tf, engagement, brain) in mobs.iter() {
        if engagement.rescans_used < MAX_RESCANS_PER_MOB {
            continue;
        }
        if !matches!(brain.state, BrainState::Idle) {
            continue;
        }
        if !matches!(brain.order, Order::Stop) {
            continue;
        }
        let pos = tf.translation;
        let edge = nearest_map_edge_point(Vec2::new(pos.x, pos.z), half_map);
        let edge_world = Vec3::new(edge.x, pos.y, edge.y);
        apply_auto_move_intent(&mut commands, entity, edge_world);
        commands.entity(entity).insert(RetreatingMob {
            started_at_tick: sim_clock.tick,
        });
    }
    let _ = STRAND_TIMEOUT_SECS;
}

/// Remove `RetreatingMob` entities that crossed the map edge OR exceeded the
/// hard timeout.
fn mob_retreat_despawn(
    mut commands: Commands,
    config: Res<GameSetupConfig>,
    sim_clock: Res<SimClock>,
    retreating: Query<(Entity, &Transform, &RetreatingMob)>,
) {
    let half_map = config.map_size.world_size() / 2.0;
    let edge = half_map - 1.0;
    let timeout_ticks = (RETREAT_HARD_TIMEOUT_SECS * TICKS_PER_SEC as f32) as u64;
    for (entity, tf, retreat) in retreating.iter() {
        let off_map =
            tf.translation.x.abs() > edge || tf.translation.z.abs() > edge;
        let timed_out = sim_clock.tick.saturating_sub(retreat.started_at_tick) > timeout_ticks;
        if off_map || timed_out {
            commands.entity(entity).try_despawn();
        }
    }
}

// ── Spawn helpers ────────────────────────────────────────────────────────

fn spawn_mob(
    commands: &mut Commands,
    cache: &EntityVisualCache,
    registry: &BlueprintRegistry,
    unit_models: Option<&UnitModelAssets>,
    height_map: &HeightMap,
    pos: Vec3,
    tier: MobTier,
) -> Entity {
    let entity = spawn_from_blueprint(
        commands,
        cache,
        EntityKind::Goblin,
        pos,
        registry,
        None,
        unit_models,
        height_map,
    );
    apply_mob_tier(commands, entity, registry, height_map, pos, tier);
    entity
}

/// Apply a tier (stat multipliers + visual scale) to a freshly spawned
/// `EntityKind::Goblin`. Used by the wave drip spawn AND by the debug
/// "Spawn at Camera" path so debug-spawned mobs look and behave like
/// wave-spawned ones.
pub fn apply_mob_tier(
    commands: &mut Commands,
    entity: Entity,
    registry: &BlueprintRegistry,
    height_map: &HeightMap,
    pos: Vec3,
    tier: MobTier,
) {
    let bp = registry.get(EntityKind::Goblin);
    let y_off = bp.movement.as_ref().map(|m| m.y_offset).unwrap_or(0.0);
    let visual_scale = bp.visual.scale * tier.visual_scale_mul();

    // Stat multipliers — applied via deferred world mutation so we can read
    // and write the components the blueprint just inserted.
    let hp_mul = tier.hp_mul();
    let dmg_mul = tier.damage_mul();
    let move_mul = tier.move_mul();
    commands.queue(move |world: &mut World| {
        if let Some(mut h) = world.get_mut::<Health>(entity) {
            h.max *= hp_mul;
            h.current = h.max;
        }
        if let Some(mut d) = world.get_mut::<AttackDamage>(entity) {
            d.0 *= dmg_mul;
        }
        if let Some(mut s) = world.get_mut::<UnitSpeed>(entity) {
            s.0 *= move_mul;
        }
    });

    commands.entity(entity).insert((
        tier,
        Transform::from_translation(Vec3::new(
            pos.x,
            height_map.sample(pos.x, pos.z) + y_off,
            pos.z,
        ))
        .with_scale(Vec3::splat(visual_scale)),
    ));
}

/// Init a mob's engagement: pick a primary target, store it on the entity,
/// and issue the initial Attack intent. Public so the debug spawn path can
/// call it after `spawn_from_blueprint`. If no hostile target exists, falls
/// back to a Move toward map center so the mob doesn't sit idle forever.
pub fn init_mob_engagement(
    commands: &mut Commands,
    entity: Entity,
    pos: Vec3,
    now: f64,
    units: &Query<(Entity, &Transform, &Faction), With<Unit>>,
    buildings: &Query<
        (Entity, &Transform, &Faction, Option<&EntityKind>),
        (With<Building>, Without<Unit>, Without<FloorTile>),
    >,
    factions: &Query<&Faction>,
) {
    if let Some((target, kind)) = pick_primary_target(pos, units, buildings, factions) {
        commands.entity(entity).insert(MobEngagement {
            primary: target,
            primary_kind: kind,
            rescans_used: 0,
        });
        apply_auto_attack_intent(commands, entity, target, pos, now);
    } else {
        apply_auto_move_intent(commands, entity, Vec3::ZERO);
    }
}

// ── Loot helpers (called from death.rs) ──────────────────────────────────

/// Determine what (if any) item to drop for a slain mob. Caller passes a
/// per-mob deterministic RNG.
pub fn goblin_drop_for(tier: MobTier, rng: &mut StdRng) -> Option<ItemKind> {
    let drop_chance = match tier {
        MobTier::Runner => 0.05,
        MobTier::Veteran => 0.15,
        MobTier::Champion => 0.30,
    };
    if !rng.random_bool(drop_chance) {
        return None;
    }
    let pool: &[ItemKind] = match tier {
        MobTier::Runner => &[ItemKind::PaddedVest, ItemKind::KettleHelm, ItemKind::PlainBand],
        MobTier::Veteran | MobTier::Champion => &[
            ItemKind::ArmingSword,
            ItemKind::BattleStaff,
            ItemKind::YewLongbow,
        ],
    };
    Some(pool[rng.random_range(0..pool.len())])
}

/// Build a per-mob RNG from the active wave seed plus the entity's bits, so
/// drop rolls stay deterministic across peers without holding wave state in
/// the death handler.
pub fn per_mob_drop_rng(wave_seed: u64, entity: Entity) -> StdRng {
    StdRng::seed_from_u64(wave_seed ^ entity.to_bits().wrapping_mul(PHI_U64))
}

/// Helper for death.rs to spawn a single drop pickup if the tier rolls.
pub fn try_spawn_mob_drop(
    spawns: &mut MessageWriter<SpawnItemPickup>,
    wave_seed: u64,
    entity: Entity,
    tier: MobTier,
    pos: Vec3,
) {
    let mut rng = per_mob_drop_rng(wave_seed, entity);
    if let Some(item) = goblin_drop_for(tier, &mut rng) {
        spawns.write(SpawnItemPickup {
            item,
            position: pos,
            owner: None,
            lifetime_secs: 90.0,
        });
    }
}

/// Read the active wave seed (for death-time drop rolls). Returns 0 if no
/// wave is active — callers should treat 0 as "no wave context", drop
/// nothing.
pub fn active_wave_seed(wave: &NightWaveState) -> u64 {
    wave.active.as_ref().map(|w| w.seed).unwrap_or(0)
}
