//! Death bookkeeping and the dying-animation shrink.
//!
//! Cleans up after a unit/mob/building's HP hits zero: spawns loot drops,
//! retargets attackers, unassigns workers, ejects building tenants, awards
//! XP/bonuses, removes wall grid cells, and starts the shrink-and-despawn
//! Dying timer.
//!
//! Mob-specific: when a mob's `MobEngagement.primary` dies, we run one
//! re-pick (capped at 2 across the mob's lifetime). When a mob itself dies,
//! we ask `mobs::try_spawn_mob_drop` for a tier-weighted item roll.

use bevy::prelude::*;
use bevy::time::Fixed;

use crate::blueprints::EntityKind;
use crate::simulation::items::{SpawnItemPickup, UnitInventory};
use crate::simulation::mobs::{active_wave_seed, try_spawn_mob_drop, NightWaveState};
use crate::types::{
    app::Faction, AppState, AssignedWorkers, Building, BuildingFootprint, Dying,
    EngagementTargetKind, Experience, FloorTile, Health, MobEngagement, MobTier, Selected,
    SimClock, Unit, UnitState, WallCornerPiece, WallGrid, WallGridCoord, WallPostPiece,
    WallSegmentPiece,
};

use super::brain::{BrainState, CombatSet, Order, UnitBrain};
use super::intents::apply_auto_attack_intent;

pub struct CombatDeathPlugin;

impl Plugin for CombatDeathPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (handle_death, tick_dying)
                .chain()
                .in_set(CombatSet::Death)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn handle_death(
    mut commands: Commands,
    mut item_pickup_spawns: MessageWriter<SpawnItemPickup>,
    dead: Query<
        (
            Entity,
            &Health,
            Option<&Building>,
            Option<&Selected>,
            Option<&EntityKind>,
            Option<&Transform>,
            Option<&UnitState>,
            Option<&Faction>,
            Option<&MobTier>,
        ),
        Without<Dying>,
    >,
    mut attackers: Query<(Entity, &mut UnitBrain, Option<&mut MobEngagement>), Without<Dying>>,
    mut experience_q: Query<&mut Experience>,
    all_assigned_workers: Query<&AssignedWorkers>,
    workers_with_state: Query<(Entity, &UnitState), With<Unit>>,
    time: Res<Time<Fixed>>,
    sim_clock: Res<SimClock>,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
    mut wave: ResMut<NightWaveState>,
    attacker_factions: Query<&Faction>,
    mut wall_grid: ResMut<WallGrid>,
    wall_coord_q: Query<&WallGridCoord>,
    rescan_units: Query<(Entity, &Transform, &Faction), With<Unit>>,
    rescan_buildings: Query<
        (Entity, &Transform, &Faction, Option<&EntityKind>),
        (With<Building>, Without<Unit>, Without<FloorTile>),
    >,
) {
    let dead_list: Vec<_> = dead
        .iter()
        .filter(|(_, health, ..)| health.current <= 0.0)
        .map(|(e, _, b, sel, k, tf, us, f, mt)| {
            (
                e,
                b.is_some(),
                sel.is_some(),
                k.copied(),
                tf.copied(),
                us.copied(),
                f.copied(),
                mt.copied(),
            )
        })
        .collect();

    let wave_seed = active_wave_seed(&wave);

    for (
        dead_entity,
        is_building,
        is_selected,
        opt_kind,
        opt_transform,
        opt_unit_state,
        opt_faction,
        opt_mob_tier,
    ) in &dead_list
    {
        // ── Mob loot: tier-weighted single-item drop.
        if let (Some(tier), Some(transform)) = (opt_mob_tier, opt_transform) {
            try_spawn_mob_drop(
                &mut item_pickup_spawns,
                wave_seed,
                *dead_entity,
                *tier,
                transform.translation,
            );
        }

        // ── Wave killed-counter bookkeeping.
        if opt_mob_tier.is_some() {
            if let Some(active) = wave.active.as_mut() {
                active.killed = active.killed.saturating_add(1);
            }
        }
        if *is_building && !matches!(opt_faction, Some(Faction::Neutral) | None) {
            if let Some(active) = wave.active.as_mut() {
                active.buildings_damaged = active.buildings_damaged.saturating_add(1);
            }
        }

        // Find the killing faction via whoever was targeting this entity.
        let killer_entity = attackers
            .iter()
            .find(|(_, brain, _)| brain.target == Some(*dead_entity))
            .map(|(e, _, _)| e);
        let killer_faction = killer_entity.and_then(|e| attacker_factions.get(e).ok()).copied();

        // ── Re-target attackers and run the mob's primary-death rescan.
        for (attacker_entity, mut brain, opt_engagement) in &mut attackers {
            if brain.target != Some(*dead_entity) {
                continue;
            }
            // AttackMove/Hold orders survive target death — re-acquire via auto-aggro.
            let preserve_order = matches!(brain.order, Order::AttackMove(_) | Order::Hold);
            brain.target = None;
            brain.slot_claim = None;
            brain.chase_started_at = None;
            if !preserve_order {
                brain.order = Order::Stop;
                brain.state = BrainState::Idle;
            } else if matches!(brain.state, BrainState::Chasing | BrainState::InRange) {
                brain.state = BrainState::Idle;
            }

            // Mob-specific: if we lost our primary engagement target, try to
            // pick a new one within RESCAN_RADIUS, capped at MAX_RESCANS_PER_MOB.
            if let Some(mut engagement) = opt_engagement {
                if engagement.primary == *dead_entity && engagement.rescans_used < 2 {
                    engagement.rescans_used += 1;
                    let attacker_pos =
                        opt_transform.map(|t| t.translation).unwrap_or(Vec3::ZERO);
                    if let Some((new_target, kind)) = rescan_for_mob(
                        attacker_entity,
                        attacker_pos,
                        &rescan_units,
                        &rescan_buildings,
                    ) {
                        engagement.primary = new_target;
                        engagement.primary_kind = kind;
                        let now = time.elapsed_secs_f64();
                        apply_auto_attack_intent(
                            &mut commands,
                            attacker_entity,
                            new_target,
                            attacker_pos,
                            now,
                        );
                    }
                }
            }
        }

        // Worker unassignment.
        if let Some(UnitState::AssignedGathering { building, .. }) = opt_unit_state {
            crate::simulation::buildings::remove_assigned_worker(&mut commands, *building, *dead_entity);
        }

        // Building death: eject assigned workers.
        if *is_building {
            if let Ok(aw) = all_assigned_workers.get(*dead_entity) {
                let workers_to_eject: Vec<Entity> = aw.workers.clone();
                for worker in workers_to_eject {
                    if let Ok((_, worker_state)) = workers_with_state.get(worker) {
                        if matches!(worker_state, UnitState::AssignedGathering { building, .. } if *building == *dead_entity)
                        {
                            crate::simulation::resources::unassign_worker_from_processor(
                                &mut commands,
                                worker,
                                Some(*dead_entity),
                            );
                        }
                    }
                }
            }
        }

        // Event log.
        let name = opt_kind.map_or("Unit", |k| k.display_name());
        let pos = opt_transform.map(|t| t.translation);
        event_log.push(
            time.elapsed_secs(),
            format!("{} destroyed", name),
            crate::ui::event_log_widget::EventCategory::Combat,
            pos,
            *opt_faction,
        );

        if *is_selected {
            commands.entity(*dead_entity).remove::<Selected>();
        }

        if *is_building {
            if let Ok(coord) = wall_coord_q.get(*dead_entity) {
                let (gx, gz) = (coord.0, coord.1);
                wall_grid.cells.remove(&(gx, gz));
                for (nx, nz) in WallGrid::cardinal_neighbors(gx, gz) {
                    wall_grid.dirty.push((nx, nz));
                }
            }
            commands.entity(*dead_entity).try_despawn();
        } else {
            let scale = opt_transform.map(|t| t.scale).unwrap_or(Vec3::ONE);

            // XP: max HP / 5.
            if let Some(killer_e) = killer_entity {
                let dead_max_hp = dead
                    .iter()
                    .find(|(e, ..)| *e == *dead_entity)
                    .map(|(_, h, ..)| h.max)
                    .unwrap_or(50.0);
                let xp = (dead_max_hp / 5.0) as u32;
                if let Ok(mut exp) = experience_q.get_mut(killer_e) {
                    exp.current += xp;
                    if let Some((next_level, threshold)) = exp.level.next() {
                        if exp.current >= threshold {
                            exp.level = next_level;
                        }
                    }
                }
            }

            commands
                .entity(*dead_entity)
                .remove::<crate::types::Mob>()
                .remove::<MobEngagement>()
                .remove::<MobTier>()
                .remove::<crate::types::RetreatingMob>()
                .remove::<Unit>()
                .remove::<UnitBrain>()
                .remove::<crate::types::MoveTarget>()
                .insert(Dying {
                    timer: Timer::from_seconds(1.5, TimerMode::Once),
                    _killed_by: killer_faction,
                    original_scale: scale,
                });
        }
    }
    let _ = sim_clock;
}

/// Rescan from the mob's current position for a replacement primary target.
/// Bias toward buildings; falls back to nearest unit. Returns `None` if
/// nothing within `RESCAN_RADIUS` qualifies.
fn rescan_for_mob(
    self_entity: Entity,
    pos: Vec3,
    units: &Query<(Entity, &Transform, &Faction), With<Unit>>,
    buildings: &Query<
        (Entity, &Transform, &Faction, Option<&EntityKind>),
        (With<Building>, Without<Unit>, Without<FloorTile>),
    >,
) -> Option<(Entity, EngagementTargetKind)> {
    const RESCAN_RADIUS_SQ: f32 = 80.0 * 80.0;
    const BUILDING_BIAS: f32 = -10.0;

    let mut best_score = f32::MAX;
    let mut best: Option<(Entity, EngagementTargetKind)> = None;

    for (e, tf, faction, opt_kind) in buildings.iter() {
        if matches!(faction, Faction::Neutral) {
            continue;
        }
        let d_sq = pos.distance_squared(tf.translation);
        if d_sq > RESCAN_RADIUS_SQ {
            continue;
        }
        let dist = d_sq.sqrt();
        let bias = match opt_kind.copied() {
            Some(EntityKind::Base) | Some(EntityKind::Storage) => -20.0,
            _ => BUILDING_BIAS,
        };
        let score = dist + bias;
        if score < best_score {
            best_score = score;
            best = Some((e, EngagementTargetKind::Building));
        }
    }
    for (e, tf, faction) in units.iter() {
        if e == self_entity {
            continue;
        }
        if matches!(faction, Faction::Neutral) {
            continue;
        }
        let d_sq = pos.distance_squared(tf.translation);
        if d_sq > RESCAN_RADIUS_SQ {
            continue;
        }
        let dist = d_sq.sqrt();
        if dist < best_score {
            best_score = dist;
            best = Some((e, EngagementTargetKind::Unit));
        }
    }
    best
}

pub fn tick_dying(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut dying: Query<(Entity, &mut Dying, &mut Transform)>,
) {
    for (entity, mut dying, mut tf) in &mut dying {
        dying.timer.tick(time.delta());
        let remaining = dying.timer.remaining_secs();
        if remaining < 0.4 {
            let shrink_frac = (remaining / 0.4).max(0.0);
            tf.scale = dying.original_scale * shrink_frac;
        }
        if dying.timer.is_finished() {
            commands.entity(entity).try_despawn();
        }
    }
}

// Silence unused-import warnings for types only referenced in bodies.
#[allow(dead_code)]
fn _deny_unused() {
    let _ = std::marker::PhantomData::<(
        BuildingFootprint,
        WallSegmentPiece,
        WallPostPiece,
        WallCornerPiece,
        UnitInventory,
    )>;
}
