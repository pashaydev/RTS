use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::time::Fixed;

use crate::blueprints::EntityKind;
use crate::simulation::items::vfx::{ItemVfxTrigger, ItemVfxTriggerKind};
use crate::simulation::items::{ItemKind, SpawnItemPickup, UnitInventory};
use crate::simulation::mobs::CampItemDrops;
use crate::types::*;
use crate::world::spatial::{SpatialHashGrid, WallSpatialGrid};

use super::damage::apply_damage;
use super::{slot_anchor, CombatBudgetState};

pub struct CombatPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CombatCoreSet {
    ResolveIntent,
    Approach,
    Windup,
    ResolveHit,
    Recovery,
    Cleanup,
}

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            FixedUpdate,
            (
                CombatCoreSet::ResolveIntent,
                CombatCoreSet::Approach,
                CombatCoreSet::Windup,
                CombatCoreSet::ResolveHit,
                CombatCoreSet::Recovery,
                CombatCoreSet::Cleanup,
            )
                .chain()
                .in_set(SimSet::Combat),
        );
        app.add_systems(
            FixedUpdate,
            (
                tick_damage_reservations.in_set(CombatCoreSet::ResolveIntent),
                resolve_combat_intents.in_set(CombatCoreSet::ResolveIntent),
                approach_attack_target.in_set(CombatCoreSet::Approach),
                start_attack_windups.in_set(CombatCoreSet::Windup),
                resolve_attack_windups.in_set(CombatCoreSet::ResolveHit),
                emit_item_combat_vfx.in_set(CombatCoreSet::ResolveHit),
                tick_attack_recovery.in_set(CombatCoreSet::Recovery),
                explode_props.in_set(CombatCoreSet::Cleanup),
                handle_death.in_set(CombatCoreSet::Cleanup),
                emit_item_death_vfx.in_set(CombatCoreSet::Cleanup),
                tick_dying.in_set(CombatCoreSet::Cleanup),
                sync_display_state
                    .in_set(CombatCoreSet::Cleanup)
                    .after(handle_death),
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Resolve the unit's current attack target.
///
/// Primary source is `CombatOrder` (canonical entry state). Legacy
/// `Engagement`/`CombatIntent`/`CombatTargetLock` are consulted only as a
/// fallback for entities without a `CombatOrder` yet — e.g. loaded from a
/// pre-CombatOrder save, or runtime-mutated engagement state during
/// AttackMove target acquisition.
fn intended_attack_target(
    order: Option<&CombatOrder>,
    engagement: Option<&Engagement>,
    intent: Option<&CombatIntent>,
    target_lock: Option<&CombatTargetLock>,
) -> Option<Entity> {
    if let Some(order) = order {
        match order.goal {
            CombatGoal::Attack(target) => return Some(target),
            CombatGoal::AttackMove(_) | CombatGoal::Hold => {
                // For AttackMove/Hold, the target is acquired at runtime by scan
                // logic and stored on Engagement/CombatTargetLock. Fall through.
            }
            CombatGoal::Move(_) | CombatGoal::Stop => return None,
        }
    }
    if let Some(engagement) = engagement {
        if let Some(target) = engagement.target {
            return Some(target);
        }
    }
    match intent {
        Some(CombatIntent::Attack(target, _)) => Some(*target),
        Some(CombatIntent::AttackMove(_, _)) => target_lock.map(|lock| lock.target),
        Some(CombatIntent::Hold) => target_lock.map(|lock| lock.target),
        _ => None,
    }
}

fn resolve_combat_intents(
    mut commands: Commands,
    all_entities: Query<()>,
    mut actors: Query<
        (
            Entity,
            Option<&Faction>,
            Option<&CombatOrder>,
            Option<&Engagement>,
            Option<&CombatIntent>,
            Option<&CombatTargetLock>,
            Option<&AttackTarget>,
            Option<&MoveTarget>,
            Option<&AttackWindup>,
            Option<&AttackRecovery>,
            Option<&UnitState>,
        ),
        Or<(With<Unit>, With<Mob>)>,
    >,
) {
    for (
        entity,
        _faction,
        order,
        engagement,
        intent,
        target_lock,
        attack_target,
        move_target,
        windup,
        _recovery,
        unit_state,
    ) in &mut actors
    {
        // Skip non-combat states — these own their own MoveTarget management.
        if let Some(state) = unit_state {
            match state {
                UnitState::Building(_)
                | UnitState::MovingToBuild(_)
                | UnitState::MovingToPlot(_)
                | UnitState::AssignedGathering { .. }
                | UnitState::Depositing { .. }
                | UnitState::ReturningToDeposit { .. }
                | UnitState::WaitingForStorage { .. }
                | UnitState::WaitingForDepot { .. }
                | UnitState::Gathering(_) => continue,
                _ => {}
            }
        }

        // Windup is the true commit point. Recovery should still accept the next move/order.
        let committed = windup.is_some();
        let desired_target = intended_attack_target(order, engagement, intent, target_lock)
            .filter(|target| all_entities.contains(*target));

        if let Some(target) = desired_target {
            let needs_sync = attack_target.map_or(true, |current| current.0 != target);
            if needs_sync && !committed {
                commands
                    .entity(entity)
                    .insert(AttackTarget(target))
                    .remove::<MoveTarget>();
            }
            continue;
        }

        // Prefer CombatGoal when present, fall back to legacy Engagement::mode / CombatIntent.
        enum Dispatch {
            Move(Vec3),
            AttackMove(Vec3),
            Hold,
            Stop,
            None,
        }
        let dispatch = match order.map(|o| o.goal) {
            Some(CombatGoal::Move(dest)) => Dispatch::Move(dest),
            Some(CombatGoal::AttackMove(dest)) => Dispatch::AttackMove(dest),
            Some(CombatGoal::Hold) => Dispatch::Hold,
            Some(CombatGoal::Stop) => Dispatch::Stop,
            Some(CombatGoal::Attack(_)) => Dispatch::None, // handled above by desired_target path
            None => match engagement.map(|e| e.mode) {
                Some(EngageMode::AttackMove) => {
                    let dest = engagement.map(|e| e.anchor).unwrap_or(Vec3::ZERO);
                    Dispatch::AttackMove(dest)
                }
                Some(EngageMode::Hold) => Dispatch::Hold,
                _ => match intent.copied().unwrap_or_default() {
                    CombatIntent::Move(dest) => Dispatch::Move(dest),
                    CombatIntent::AttackMove(dest, _) => Dispatch::AttackMove(dest),
                    CombatIntent::Hold => Dispatch::Hold,
                    CombatIntent::None => Dispatch::Stop,
                    CombatIntent::Attack(_, _) => Dispatch::None,
                },
            },
        };
        match dispatch {
            Dispatch::AttackMove(destination) => {
                if attack_target.is_some() && !committed {
                    commands
                        .entity(entity)
                        .remove::<AttackTarget>()
                        .remove::<ChaseTimer>();
                }
                if !committed
                    && move_target.map_or(true, |current| current.0.distance(destination) > 0.6)
                {
                    commands.entity(entity).insert(MoveTarget(destination));
                }
            }
            Dispatch::Hold => {
                if !committed {
                    commands
                        .entity(entity)
                        .remove::<MoveTarget>()
                        .remove::<ChaseTimer>();
                }
            }
            Dispatch::Move(destination) => {
                if attack_target.is_some() && !committed {
                    commands
                        .entity(entity)
                        .remove::<AttackTarget>()
                        .remove::<ChaseTimer>();
                }
                if !committed
                    && move_target.map_or(true, |current| current.0.distance(destination) > 0.6)
                {
                    commands.entity(entity).insert(MoveTarget(destination));
                }
            }
            Dispatch::Stop => {
                if attack_target.is_some() && !committed {
                    commands
                        .entity(entity)
                        .remove::<AttackTarget>()
                        .remove::<ChaseTimer>();
                }
            }
            Dispatch::None => {}
        }
    }
}

pub fn attack_surface_distance(attacker_pos: Vec3, target_pos: Vec3, target_radius: f32) -> f32 {
    let dx = target_pos.x - attacker_pos.x;
    let dz = target_pos.z - attacker_pos.z;
    ((dx * dx + dz * dz).sqrt() - target_radius).max(0.0)
}

pub fn is_in_attack_band(
    surface_distance: f32,
    max_range: f32,
    minimum_range: f32,
    tolerance: f32,
) -> bool {
    let min_allowed = (minimum_range - tolerance).max(0.0);
    let max_allowed = max_range + tolerance;
    surface_distance >= min_allowed && surface_distance <= max_allowed
}

fn desired_attack_surface_distance(max_range: f32, minimum_range: f32) -> f32 {
    if minimum_range > 0.0 {
        ((minimum_range + max_range) * 0.5)
            .max(minimum_range + 0.25)
            .min((max_range - 0.1).max(minimum_range))
    } else {
        (max_range - 0.15).max(0.25)
    }
}

fn desired_attack_move_target(
    attacker_entity: Entity,
    attacker_pos: Vec3,
    target_pos: Vec3,
    target_radius: f32,
    max_range: f32,
    minimum_range: f32,
) -> Vec3 {
    let away = Vec2::new(attacker_pos.x - target_pos.x, attacker_pos.z - target_pos.z);
    let dir = if away.length_squared() > 0.0001 {
        away.normalize()
    } else {
        let angle = (attacker_entity.to_bits() % 360) as f32 * std::f32::consts::TAU / 360.0;
        Vec2::new(angle.cos(), angle.sin())
    };
    let desired_surface = desired_attack_surface_distance(max_range, minimum_range);
    let tangent = Vec2::new(-dir.y, dir.x);
    let slot_seed = (attacker_entity.to_bits() % 7) as i32 - 3;
    let lateral_offset = if minimum_range > 0.0 || max_range > 3.0 {
        slot_seed as f32 * 0.85
    } else {
        0.0
    };
    target_pos
        + Vec3::new(dir.x, 0.0, dir.y) * (target_radius + desired_surface)
        + Vec3::new(tangent.x, 0.0, tangent.y) * lateral_offset
}

fn explode_props(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut damage_events: MessageWriter<DamageApplied>,
    mut queries: ParamSet<(
        Query<(Entity, &Transform, &ExplosiveProp, &Health)>,
        Query<(Entity, &mut Transform, &mut Health), Without<Projectile>>,
    )>,
) {
    let Some(vfx) = vfx_assets else { return };

    let detonations: Vec<_> = queries
        .p0()
        .iter()
        .filter(|(_, _, _, health)| health.current <= 0.0)
        .map(|(entity, tf, prop, _)| (entity, tf.translation, *prop))
        .collect();

    for (source_entity, origin, prop) in detonations {
        let now = time.elapsed_secs_f64();
        commands.spawn((
            VfxFlash {
                timer: Timer::from_seconds(0.3, TimerMode::Once),
                start_scale: 0.4,
                end_scale: prop.radius * 0.55,
                rise_speed: 0.6,
            },
            FogHideable::Vfx,
            Mesh3d(vfx.sphere_mesh.clone()),
            MeshMaterial3d(vfx.impact_material.clone()),
            Transform::from_translation(origin).with_scale(Vec3::splat(0.4)),
            NotShadowCaster,
            NotShadowReceiver,
        ));

        for (target_entity, mut target_tf, mut health) in &mut queries.p1() {
            if target_entity == source_entity {
                continue;
            }

            let offset = target_tf.translation - origin;
            let dist = offset.length();
            if dist > prop.radius {
                continue;
            }

            let falloff = 1.0 - (dist / prop.radius).min(1.0);
            if falloff <= 0.0 {
                continue;
            }

            let dealt = apply_damage(
                &mut commands,
                target_entity,
                None,
                prop.damage * falloff,
                DamageType::SiegeDmg,
                &mut health,
                None,
                None,
                now,
                0.0,
            );
            damage_events.write(DamageApplied {
                target: target_entity,
                source: None,
                amount: dealt,
                damage_type: DamageType::SiegeDmg,
                now_secs: now,
            });
            if dist > 0.05 {
                let push = Vec3::new(offset.x, 0.0, offset.z).normalize_or_zero() * falloff * 0.9;
                target_tf.translation += push;
            }
        }
    }
}

pub fn approach_attack_target(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    tuning: Res<CombatTuning>,
    combat_budget: Res<CombatBudget>,
    mut budget_state: ResMut<CombatBudgetState>,
    teams: Res<TeamConfig>,
    wall_grid: Res<WallSpatialGrid>,
    mut attackers: Query<
        (
            Entity,
            &Transform,
            &AttackTarget,
            &AttackRange,
            Option<&AttackTiming>,
            Option<&AttackProfile>,
            &Faction,
            (
                Option<&mut UnitState>,
                Option<&MoveTarget>,
                Option<&AttackWindup>,
                Option<&AttackRecovery>,
            ),
            (
                Option<&mut ChaseTimer>,
                Option<&TaskSource>,
                Option<&mut Engagement>,
                Option<&mut CombatTargetLock>,
                Option<&SlotClaim>,
                Option<&MeleeContact>,
                Option<&CombatOrder>,
            ),
        ),
        Or<(With<Unit>, With<Mob>)>,
    >,
    scene_queries: (
        Query<
            (),
            (
                With<Building>,
                Or<(
                    With<WallSegmentPiece>,
                    With<WallPostPiece>,
                    With<WallCornerPiece>,
                )>,
            ),
        >,
        Query<&Transform>,
        Query<&BuildingFootprint, With<Building>>,
        Query<&TacticalRole>,
        Query<&Faction>,
    ),
    spatial_grid: Res<SpatialHashGrid>,
    mut nearby_entities: Local<Vec<(Entity, Vec3)>>,
) {
    let (wall_check, all_transforms, building_footprints, tactical_roles, factions) = scene_queries;
    for (
        attacker_entity,
        tf,
        attack_target,
        range,
        attack_timing,
        opt_profile,
        faction,
        (opt_state, current_move_target, windup, recovery),
        (
            opt_chase_timer,
            opt_task_source,
            opt_engagement,
            opt_target_lock,
            slot_claim,
            opt_melee_contact,
            opt_order,
        ),
    ) in &mut attackers
    {
        // During windup/recovery, unit is locked in animation — skip
        if windup.is_some() || recovery.is_some() {
            continue;
        }
        let Ok(target_tf) = all_transforms.get(attack_target.0) else {
            continue;
        };
        let target_radius = building_footprints
            .get(attack_target.0)
            .map_or(0.0, |fp| fp.0);
        let minimum_range = attack_timing.map_or(0.0, |timing| timing.minimum_range);

        // ── Wall redirect: if a hostile wall blocks the path, retarget it ──
        let target_is_wall = wall_check.get(attack_target.0).is_ok();
        if !target_is_wall {
            let from = Vec2::new(tf.translation.x, tf.translation.z);
            let to = Vec2::new(target_tf.translation.x, target_tf.translation.z);
            let delta = to - from;
            let line_len = delta.length();

            if line_len > 0.5 {
                let dir = delta / line_len;
                let mut blocking_wall: Option<(Entity, f32)> = None;

                let mid = tf.translation.lerp(target_tf.translation, 0.5);
                let search_radius = line_len * 0.5 + 2.0;
                let nearby_walls = wall_grid.query_radius(mid, search_radius);

                for (wall_entity, wall_pos_3d, wall_fp, wall_faction) in &nearby_walls {
                    if !teams.is_hostile(faction, &wall_faction) {
                        continue;
                    }
                    let wall_pos = Vec2::new(wall_pos_3d.x, wall_pos_3d.z);
                    let rel = wall_pos - from;
                    let t = rel.dot(dir);
                    if t <= 0.3 || t >= line_len - 0.3 {
                        continue;
                    }
                    let closest = from + dir * t;
                    let perp_dist = wall_pos.distance(closest);
                    if perp_dist <= wall_fp + 0.35
                        && blocking_wall.map_or(true, |(_, best_t)| t < best_t)
                    {
                        blocking_wall = Some((*wall_entity, t));
                    }
                }

                if let Some((wall_entity, _)) = blocking_wall {
                    commands
                        .entity(attacker_entity)
                        .insert(AttackTarget(wall_entity));
                    let source = if opt_task_source
                        .map_or(false, |task_source| *task_source == TaskSource::Manual)
                    {
                        IntentSource::Manual
                    } else {
                        IntentSource::Auto
                    };
                    commands
                        .entity(attacker_entity)
                        .insert(CombatIntent::Attack(wall_entity, source));
                    if let Some(mut target_lock) = opt_target_lock {
                        target_lock.target = wall_entity;
                        target_lock.locked_until = time.elapsed_secs_f64() + 0.5;
                        target_lock.source = source;
                    } else {
                        commands.entity(attacker_entity).insert(CombatTargetLock {
                            target: wall_entity,
                            locked_until: time.elapsed_secs_f64() + 0.5,
                            source,
                        });
                    }
                    if let Some(mut engagement) = opt_engagement {
                        engagement.target = Some(wall_entity);
                        engagement.last_confirmed_at = time.elapsed_secs_f64();
                        engagement.persistence_until = engagement
                            .persistence_until
                            .max(time.elapsed_secs_f64() + 0.5);
                        engagement.status = EngageStatus::Approaching;
                    }
                    continue;
                }
            }
        }

        // ── Distance check (2D only — ignore terrain height) ──
        let stay_tolerance = opt_engagement
            .as_ref()
            .is_some_and(|engagement| {
                matches!(
                    engagement.status,
                    EngageStatus::InBand | EngageStatus::Windup | EngageStatus::Recovery
                )
            })
            .then_some(tuning.range_stay_buffer)
            .unwrap_or(0.15);
        let surface_dist =
            attack_surface_distance(tf.translation, target_tf.translation, target_radius);
        let in_band = is_in_attack_band(surface_dist, range.0, minimum_range, stay_tolerance);
        let is_ranged_unit = opt_profile.map_or(false, |p| p.is_ranged());

        if !in_band {
            if opt_melee_contact.is_some() {
                commands.entity(attacker_entity).remove::<MeleeContact>();
            }
            if let Some(mut engagement) = opt_engagement {
                engagement.status = EngageStatus::Approaching;
                engagement.last_confirmed_at = time.elapsed_secs_f64();
            }
            // Reposition until the target sits inside the allowed attack band.
            if budget_state.repath_requests_this_frame
                >= combat_budget.max_repath_requests_per_frame
            {
                continue;
            }
            let desired_pos = if !is_ranged_unit
                && slot_claim.is_some_and(|claim| claim.target == attack_target.0)
            {
                let claim = slot_claim.unwrap();
                slot_anchor(
                    attacker_entity,
                    target_tf.translation,
                    target_radius.max(0.75),
                    claim.slot_index,
                    12,
                    minimum_range.max(range.0 * 0.85),
                )
            } else {
                desired_attack_move_target(
                    attacker_entity,
                    tf.translation,
                    target_tf.translation,
                    target_radius,
                    range.0,
                    minimum_range,
                )
            };
            if current_move_target.map_or(true, |current| current.0.distance(desired_pos) > 0.9) {
                commands
                    .entity(attacker_entity)
                    .insert(MoveTarget(desired_pos));
            }
            budget_state.repath_requests_this_frame += 1;

            // ── Chase timeout ──
            if let Some(mut chase) = opt_chase_timer {
                chase.elapsed += time.delta_secs();
                if chase.elapsed > chase.max_secs {
                    // Give up chasing
                    commands
                        .entity(attacker_entity)
                        .remove::<AttackTarget>()
                        .remove::<MoveTarget>()
                        .remove::<LeashOrigin>()
                        .remove::<ChaseTimer>();
                    commands
                        .entity(attacker_entity)
                        .remove::<CombatTargetLock>()
                        .insert(CombatIntent::None);
                    if let Some(mut state) = opt_state {
                        *state = UnitState::Idle;
                    }
                    continue;
                }
            } else {
                // Start chase timer — unified via OrderSource::chase_timeout_secs().
                // Falls back to TaskSource for entities that haven't had a CombatOrder
                // written yet (e.g. loaded from a pre-CombatOrder save).
                let max_secs = opt_order
                    .map(|o| o.source.chase_timeout_secs())
                    .unwrap_or_else(|| {
                        if opt_task_source.map_or(false, |s| *s == TaskSource::Manual) {
                            10.0
                        } else {
                            6.0
                        }
                    });
                commands.entity(attacker_entity).insert(ChaseTimer {
                    elapsed: 0.0,
                    max_secs,
                });
            }
        } else {
            // In range — stop moving, reset chase timer
            commands
                .entity(attacker_entity)
                .remove::<MoveTarget>()
                .remove::<ChaseTimer>();
            if let Some(mut engagement) = opt_engagement {
                engagement.status = EngageStatus::InBand;
                engagement.last_confirmed_at = time.elapsed_secs_f64();
            }
            if !is_ranged_unit {
                commands.entity(attacker_entity).insert(MeleeContact {
                    target: attack_target.0,
                    sticky_until: time.elapsed_secs_f64() + tuning.melee_contact_sticky_secs as f64,
                });
            } else if opt_melee_contact.is_some() {
                commands.entity(attacker_entity).remove::<MeleeContact>();
            }

            // Ranged kiting: if a melee enemy is dangerously close, retreat backward
            if is_ranged_unit
                && tactical_roles
                    .get(attacker_entity)
                    .ok()
                    .is_some_and(|r| *r == TacticalRole::RangedKiter)
                && budget_state.repath_requests_this_frame
                    < combat_budget.max_repath_requests_per_frame
            {
                let kite_threshold = range.0 * 0.35;
                spatial_grid.collect_radius_limited(
                    tf.translation,
                    kite_threshold,
                    4,
                    &mut nearby_entities,
                );
                let mut closest_melee_dist = f32::MAX;
                let mut closest_melee_pos = Vec3::ZERO;
                for (nearby_entity, nearby_pos) in nearby_entities.iter() {
                    if *nearby_entity == attacker_entity {
                        continue;
                    }
                    // Only kite from melee enemies (non-ranged, hostile)
                    if tactical_roles.get(*nearby_entity).ok().is_some_and(|r| {
                        *r == TacticalRole::RangedKiter || *r == TacticalRole::SiegeSupport
                    }) {
                        continue; // Not a melee threat
                    }
                    let Some(nearby_faction) = factions.get(*nearby_entity).ok() else {
                        continue;
                    };
                    if !teams.is_hostile(faction, nearby_faction) {
                        continue;
                    }
                    let dx = nearby_pos.x - tf.translation.x;
                    let dz = nearby_pos.z - tf.translation.z;
                    let dist = (dx * dx + dz * dz).sqrt();
                    if dist < closest_melee_dist {
                        closest_melee_dist = dist;
                        closest_melee_pos = *nearby_pos;
                    }
                }
                if closest_melee_dist < kite_threshold {
                    // Retreat away from the melee threat
                    let away = Vec2::new(
                        tf.translation.x - closest_melee_pos.x,
                        tf.translation.z - closest_melee_pos.z,
                    );
                    let dir = away.normalize_or_zero();
                    let retreat_pos = tf.translation + Vec3::new(dir.x, 0.0, dir.y) * 4.0;
                    commands
                        .entity(attacker_entity)
                        .insert(MoveTarget(retreat_pos));
                    budget_state.repath_requests_this_frame += 1;
                }
            }
        }
    }
}

fn start_attack_windups(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut attackers: Query<(
        Entity,
        &Transform,
        &AttackTarget,
        &mut AttackCooldown,
        &AttackRange,
        Option<&AttackTiming>,
        &AttackProfile,
        &AttackDamage,
        &Faction,
        Option<&mut Engagement>,
        Option<&AttackWindup>,
        Option<&AttackRecovery>,
        Option<&StatusEffects>,
    )>,
    targets: Query<&Transform>,
    building_footprints: Query<&BuildingFootprint, With<Building>>,
    mut reserved_q: Query<&mut ReservedIncomingDamage>,
) {
    for (
        entity,
        atk_tf,
        attack_target,
        mut cooldown,
        range,
        attack_timing,
        profile,
        atk_damage,
        _faction,
        opt_engagement,
        windup,
        recovery,
        opt_status,
    ) in &mut attackers
    {
        if windup.is_some() || recovery.is_some() {
            continue;
        }
        // Stunned units cannot attack
        if opt_status.map_or(false, |s| s.is_stunned()) {
            continue;
        }
        cooldown.ready_in = (cooldown.ready_in - time.delta_secs()).max(0.0);
        if cooldown.ready_in > 0.0 {
            continue;
        }

        let Ok(target_tf) = targets.get(attack_target.0) else {
            continue;
        };
        let target_radius = building_footprints
            .get(attack_target.0)
            .map_or(0.0, |fp| fp.0);
        let minimum_range = attack_timing.map_or(0.0, |timing| timing.minimum_range);
        let surface_dist =
            attack_surface_distance(atk_tf.translation, target_tf.translation, target_radius);
        if !is_in_attack_band(surface_dist, range.0, minimum_range, range.0 * 0.15) {
            continue;
        }

        cooldown.ready_in = cooldown.interval;
        commands.entity(entity).insert(AttackWindup {
            target: attack_target.0,
            remaining_secs: profile.windup_secs.max(0.01),
        });
        if let Some(mut engagement) = opt_engagement {
            engagement.status = EngageStatus::Windup;
        }

        // Reserve damage on the target
        if let Ok(mut reserved) = reserved_q.get_mut(attack_target.0) {
            let ttl = profile.windup_secs + 2.0;
            reserved.reservations.push((entity, atk_damage.0, ttl));
        }
    }
}

fn resolve_attack_windups(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    tuning: Res<CombatTuning>,
    vfx_assets: Option<Res<VfxAssets>>,
    projectile_assets: Option<Res<crate::presentation::model_assets::ProjectileModelAssets>>,
    mut damage_events: MessageWriter<DamageApplied>,
    mut attackers: Query<(
        Entity,
        &Transform,
        &AttackProfile,
        &CombatFxKind,
        &AttackDamage,
        &AttackRange,
        Option<&AttackTiming>,
        &Faction,
        Option<&DamageType>,
        Option<&mut Engagement>,
        &mut AttackWindup,
        Option<&ChargeBonus>,
        Option<&EntityKind>,
    )>,
    mut healths: Query<(
        &Transform,
        &mut Health,
        Option<&ArmorType>,
        Option<&mut ReservedIncomingDamage>,
        Option<&HitRecoil>,
    )>,
    building_footprints: Query<&BuildingFootprint, With<Building>>,
    camera_q: Query<Entity, With<RtsCamera>>,
) {
    let Some(vfx) = vfx_assets else { return };

    for (
        entity,
        atk_tf,
        profile,
        fx_kind,
        damage,
        range,
        attack_timing,
        _faction,
        opt_dmg_type,
        opt_engagement,
        mut windup,
        opt_charge,
        opt_entity_kind,
    ) in &mut attackers
    {
        windup.remaining_secs -= time.delta_secs();
        if windup.remaining_secs > 0.0 {
            continue;
        }

        let target = windup.target;
        let Ok((target_tf, mut health, opt_armor, opt_reserved, opt_existing_recoil)) =
            healths.get_mut(target)
        else {
            commands.entity(entity).remove::<AttackWindup>();
            commands.entity(entity).insert(AttackRecovery {
                remaining_secs: profile.recovery_secs,
            });
            continue;
        };

        let target_radius = building_footprints.get(target).map_or(0.0, |fp| fp.0);
        let minimum_range = attack_timing.map_or(0.0, |timing| timing.minimum_range);
        let surface_dist =
            attack_surface_distance(atk_tf.translation, target_tf.translation, target_radius);
        if !is_in_attack_band(surface_dist, range.0, minimum_range, range.0 * 0.2) {
            // Out of range — clear windup reservation
            if let Some(mut reserved) = opt_reserved {
                reserved.reservations.retain(|(src, _, _)| *src != entity);
            }
            commands.entity(entity).remove::<AttackWindup>();
            commands.entity(entity).insert(AttackRecovery {
                remaining_secs: (profile.recovery_secs * 0.5).max(0.05),
            });
            continue;
        }

        if profile.is_ranged() {
            // Replace windup reservation with projectile-travel reservation
            if let Some(mut reserved) = opt_reserved {
                reserved.reservations.retain(|(src, _, _)| *src != entity);
                let travel_ttl = surface_dist / profile.projectile_speed.max(8.0) + 0.35;
                reserved.reservations.push((entity, damage.0, travel_ttl));
            }
            // Ranged: spawn projectile (carries damage_type for on-hit multiplier)
            let proj_visual = opt_entity_kind
                .and_then(|k| crate::presentation::model_assets::projectile_visual_for(*k));
            let use_model = proj_visual.is_some() && projectile_assets.is_some();
            let orient = use_model
                && !matches!(
                    proj_visual,
                    Some(crate::presentation::model_assets::ProjectileVisualKind::CatapultRock)
                );
            let spawn_pos = atk_tf.translation + Vec3::Y * 0.5;
            let dir_to_target = (target_tf.translation - spawn_pos).normalize_or_zero();
            let projectile_speed = profile.projectile_speed.max(8.0);
            let proj_component = Projectile {
                source: entity,
                target,
                velocity: dir_to_target * projectile_speed,
                speed: projectile_speed,
                damage: damage.0,
                damage_type: opt_dmg_type.copied().unwrap_or(DamageType::Melee),
                fx_kind: *fx_kind,
                impact_scale: profile.impact_scale,
                lifetime_secs: surface_dist / projectile_speed + 0.35,
                orient_to_velocity: orient,
            };
            if let (Some(visual_kind), Some(ref proj_res)) = (proj_visual, &projectile_assets) {
                let scene = proj_res.scene_for(visual_kind, entity.to_bits() as usize);
                let proj_scale = match visual_kind {
                    crate::presentation::model_assets::ProjectileVisualKind::Arrow => 0.35,
                    crate::presentation::model_assets::ProjectileVisualKind::Bolt => 0.4,
                    crate::presentation::model_assets::ProjectileVisualKind::CatapultRock => 0.5,
                };
                let rotation = if orient {
                    Quat::from_rotation_arc(Vec3::Z, dir_to_target)
                } else {
                    Quat::IDENTITY
                };
                commands.spawn((
                    proj_component,
                    FogHideable::Vfx,
                    SceneRoot(scene),
                    Transform::from_translation(spawn_pos)
                        .with_rotation(rotation)
                        .with_scale(Vec3::splat(proj_scale)),
                ));
            } else {
                commands.spawn((
                    proj_component,
                    FogHideable::Vfx,
                    Mesh3d(vfx.sphere_mesh.clone()),
                    MeshMaterial3d(vfx.projectile_material.clone()),
                    Transform::from_translation(spawn_pos)
                        .with_scale(Vec3::splat(profile.projectile_scale.max(0.12))),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            };
            spawn_combat_flash(
                &mut commands,
                &vfx,
                atk_tf.translation + Vec3::Y * 0.7,
                *fx_kind,
                profile.projectile_scale.max(0.16),
                profile.projectile_scale.max(0.32),
                0.18,
                0.4,
            );
        } else {
            // Melee: apply damage directly with multiplier + flash VFX
            let charge_mult = opt_charge.map(|c| c.damage_mult).unwrap_or(1.0);
            let melee_type = opt_dmg_type.copied().unwrap_or(DamageType::Melee);
            let now = time.elapsed_secs_f64();
            let dealt = apply_damage(
                &mut commands,
                target,
                Some(entity),
                damage.0 * charge_mult,
                melee_type,
                &mut health,
                opt_armor.copied(),
                opt_reserved.map(|r| r.into_inner()),
                now,
                tuning.damage_memory_secs,
            );
            damage_events.write(DamageApplied {
                target,
                source: Some(entity),
                amount: dealt,
                damage_type: melee_type,
                now_secs: now,
            });
            // Consume charge bonus after use
            if opt_charge.is_some() {
                commands.entity(entity).remove::<ChargeBonus>();
            }
            spawn_combat_flash(
                &mut commands,
                &vfx,
                target_tf.translation,
                *fx_kind,
                0.2,
                profile.impact_scale,
                0.15,
                0.8,
            );
            spawn_combat_dust_scaled(
                &mut commands,
                &vfx,
                target_tf.translation,
                profile.impact_scale,
                dealt,
            );

            // ── Juice: melee lunge (attacker lunges forward briefly) ──
            let hit_dir = (target_tf.translation - atk_tf.translation).normalize_or_zero();
            let hit_dir_flat = Vec3::new(hit_dir.x, 0.0, hit_dir.z);
            commands.entity(entity).insert(AttackLunge {
                direction: hit_dir_flat,
                timer: Timer::from_seconds(0.14, TimerMode::Once),
                strength: (0.28 + dealt * 0.008).min(0.7),
                applied_offset: Vec3::ZERO,
            });

            // ── Juice: hit recoil on target ──
            // Use existing recoil's base_scale to avoid ratcheting scale up on repeated hits
            let recoil_base = opt_existing_recoil.map_or(target_tf.scale, |r| r.base_scale);
            commands.entity(target).insert(HitRecoil {
                direction: hit_dir_flat,
                timer: Timer::from_seconds(0.2, TimerMode::Once),
                strength: (0.18 + dealt * 0.006).min(0.5),
                lift: (0.04 + dealt * 0.002).min(0.14),
                base_scale: recoil_base,
                applied_offset: Vec3::ZERO,
            });

            // ── Juice: hit reaction anim on target ──
            commands
                .entity(target)
                .insert(HitReaction(Timer::from_seconds(0.24, TimerMode::Once)));

            // ── Juice: camera shake for heavy melee hits ──
            if dealt > 18.0 {
                if let Ok(cam_entity) = camera_q.single() {
                    commands.entity(cam_entity).insert(CameraShake {
                        timer: Timer::from_seconds(0.15, TimerMode::Once),
                        intensity: (dealt * 0.005).min(0.38),
                    });
                }
            }
        }

        // Reset chase timer on successful hit
        commands.entity(entity).remove::<ChaseTimer>();

        commands.entity(entity).remove::<AttackWindup>();
        commands.entity(entity).insert(AttackRecovery {
            remaining_secs: profile.recovery_secs,
        });
        if let Some(mut engagement) = opt_engagement {
            engagement.status = EngageStatus::Recovery;
        }
    }
}

fn tick_attack_recovery(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut recoveries: Query<(Entity, &mut AttackRecovery, Option<&mut Engagement>)>,
) {
    for (entity, mut recovery, mut opt_engagement) in &mut recoveries {
        recovery.remaining_secs -= time.delta_secs();
        if let Some(ref mut engagement) = opt_engagement {
            engagement.status = EngageStatus::Recovery;
        }
        if recovery.remaining_secs <= 0.0 {
            commands.entity(entity).remove::<AttackRecovery>();
            if let Some(ref mut engagement) = opt_engagement {
                engagement.status = EngageStatus::InBand;
            }
        }
    }
}

fn spawn_combat_flash(
    commands: &mut Commands,
    vfx: &VfxAssets,
    pos: Vec3,
    fx_kind: CombatFxKind,
    start_scale: f32,
    end_scale: f32,
    lifetime: f32,
    rise_speed: f32,
) {
    let material = match fx_kind {
        CombatFxKind::Slash | CombatFxKind::Shadow => vfx.melee_material.clone(),
        CombatFxKind::Pierce | CombatFxKind::Arcane | CombatFxKind::Siege => {
            vfx.impact_material.clone()
        }
    };
    commands.spawn((
        VfxFlash {
            timer: Timer::from_seconds(lifetime, TimerMode::Once),
            start_scale,
            end_scale,
            rise_speed,
        },
        FogHideable::Vfx,
        Mesh3d(vfx.sphere_mesh.clone()),
        MeshMaterial3d(material),
        Transform::from_translation(pos).with_scale(Vec3::splat(start_scale)),
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

fn spawn_combat_dust_scaled(
    commands: &mut Commands,
    vfx: &VfxAssets,
    pos: Vec3,
    intensity: f32,
    damage: f32,
) {
    // Scale particle count by damage: 2 base + up to 4 extra for heavy hits
    let count = 2 + ((damage / 15.0).min(4.0) as usize);
    let base_offsets = [
        (Vec3::new(0.2, 0.0, 0.1), 0.8f32),
        (Vec3::new(-0.15, 0.0, -0.05), 1.0),
        (Vec3::new(0.1, 0.0, -0.2), 0.9),
        (Vec3::new(-0.25, 0.0, 0.15), 1.1),
        (Vec3::new(0.0, 0.0, 0.25), 0.7),
        (Vec3::new(0.18, 0.0, -0.12), 1.2),
    ];
    let spread = 1.0 + damage * 0.01; // heavier hits spread particles wider
    for i in 0..count {
        let (offset, vel_scale) = base_offsets[i % base_offsets.len()];
        let scaled_offset = offset * spread;
        commands.spawn((
            CombatDust {
                timer: Timer::from_seconds(0.35 + intensity * 0.08, TimerMode::Once),
                velocity: Vec3::new(
                    scaled_offset.x * 2.5,
                    (1.1 + damage * 0.02) * vel_scale, // heavier hits launch higher
                    scaled_offset.z * 2.5,
                ),
                start_scale: 0.08 + intensity * 0.04,
            },
            FogHideable::Vfx,
            Mesh3d(vfx.sphere_mesh.clone()),
            MeshMaterial3d(vfx.dust_material.clone()),
            Transform::from_translation(pos + scaled_offset).with_scale(Vec3::splat(0.08)),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

fn handle_death(
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
            Option<&CampReward>,
            Option<&CampItemDrops>,
        ),
        Without<Dying>,
    >,
    mut attackers_with_target: Query<
        (
            Entity,
            &AttackTarget,
            Option<&mut PatrolState>,
            Option<&mut Engagement>,
        ),
        Without<Dying>,
    >,
    mut experience_q: Query<&mut Experience>,
    mut all_assigned_workers: Query<&mut AssignedWorkers>,
    workers_with_state: Query<(Entity, &UnitState), With<Unit>>,
    time: Res<Time<Fixed>>,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
    mut all_resources: ResMut<AllPlayerResources>,
    attacker_factions: Query<&Faction, With<AttackTarget>>,
    mut wall_grid: ResMut<WallGrid>,
    wall_coord_q: Query<&WallGridCoord>,
) {
    // Collect dead entities first to avoid borrow issues
    let dead_list: Vec<_> = dead
        .iter()
        .filter(|(_, health, _, _, _, _, _, _, _, _)| health.current <= 0.0)
        .map(
            |(
                entity,
                _,
                opt_building,
                opt_selected,
                opt_kind,
                opt_transform,
                opt_unit_state,
                opt_faction,
                opt_reward,
                opt_item_drops,
            )| {
                (
                    entity,
                    opt_building.is_some(),
                    opt_selected.is_some(),
                    opt_kind.map(|k| *k),
                    opt_transform.map(|t| *t),
                    opt_unit_state.copied(),
                    opt_faction.copied(),
                    opt_reward.cloned(),
                    opt_item_drops.cloned(),
                )
            },
        )
        .collect();

    for (
        dead_entity,
        is_building,
        is_selected,
        opt_kind,
        opt_transform,
        opt_unit_state,
        opt_faction,
        opt_camp_reward,
        opt_camp_item_drops,
    ) in &dead_list
    {
        // Grant camp reward resources to the killing faction
        if let (Some(drop_table), Some(transform)) = (opt_camp_item_drops, opt_transform) {
            for (idx, &item) in drop_table.items.iter().enumerate() {
                let angle = if drop_table.items.len() == 1 {
                    0.0
                } else {
                    idx as f32 / drop_table.items.len() as f32 * std::f32::consts::TAU
                };
                let radius = if drop_table.items.len() == 1 {
                    0.0
                } else {
                    0.9
                };
                item_pickup_spawns.write(SpawnItemPickup {
                    item,
                    position: transform.translation
                        + Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius),
                    owner: None,
                    lifetime_secs: 90.0,
                });
            }
        }

        if let Some(reward) = opt_camp_reward {
            // Find who was attacking this mob to determine the rewarded faction
            let killer_faction = attackers_with_target
                .iter()
                .find(|(_, at, _, _)| at.0 == *dead_entity)
                .and_then(|(attacker_e, _, _, _)| attacker_factions.get(attacker_e).ok());
            if let Some(killer_f) = killer_faction {
                if let Some(res) = all_resources.resources.get_mut(killer_f) {
                    for (rt, amt) in reward.resources.cost_entries() {
                        res.amounts[rt.index()] += amt;
                    }
                }
                event_log.push(
                    time.elapsed_secs(),
                    format!("Camp cleared! Resources gained."),
                    crate::ui::event_log_widget::EventCategory::Resource,
                    opt_transform.map(|t| t.translation),
                    Some(*killer_f),
                );
            }
        }

        for (attacker_entity, attack_target, opt_patrol, opt_engagement) in
            &mut attackers_with_target
        {
            if attack_target.0 == *dead_entity {
                let preserve_engagement_mode = opt_engagement.as_ref().is_some_and(|engagement| {
                    matches!(engagement.mode, EngageMode::AttackMove | EngageMode::Hold)
                });
                commands
                    .entity(attacker_entity)
                    .remove::<AttackTarget>()
                    .remove::<CombatTargetLock>()
                    .remove::<MeleeContact>()
                    .remove::<SlotClaim>();
                if preserve_engagement_mode {
                    if let Some(mut engagement) = opt_engagement {
                        engagement.target = None;
                        engagement.status = EngageStatus::Acquiring;
                        engagement.last_confirmed_at = time.elapsed_secs_f64();
                    }
                } else {
                    commands
                        .entity(attacker_entity)
                        .remove::<Engagement>()
                        .insert(CombatIntent::None);
                }
                if let Some(mut patrol) = opt_patrol {
                    patrol.state = PatrolStateKind::Returning;
                }
            }
        }

        // If a worker dies while assigned to a processor, remove it from AssignedWorkers
        if let Some(UnitState::AssignedGathering { building, .. }) = opt_unit_state {
            crate::simulation::buildings::remove_assigned_worker(
                &mut commands,
                *building,
                *dead_entity,
            );
        }

        // If a building dies with assigned workers, eject them all
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

        // Log death event
        let name = opt_kind.map_or("Unit", |k| k.display_name());
        let pos = opt_transform.map(|t| t.translation);
        event_log.push(
            time.elapsed_secs(),
            format!("{} destroyed", name),
            crate::ui::event_log_widget::EventCategory::Combat,
            pos,
            *opt_faction,
        );

        // Clear selection if selected
        if *is_selected {
            commands.entity(*dead_entity).remove::<Selected>();
        }

        if *is_building {
            // Remove from wall grid if this was a wall piece
            if let Ok(coord) = wall_coord_q.get(*dead_entity) {
                let (gx, gz) = (coord.0, coord.1);
                wall_grid.cells.remove(&(gx, gz));
                for (nx, nz) in WallGrid::cardinal_neighbors(gx, gz) {
                    wall_grid.dirty.push((nx, nz));
                }
            }
            // Buildings despawn immediately
            commands.entity(*dead_entity).try_despawn();
        } else {
            // Units play death animation before despawning
            let scale = opt_transform.map(|t| t.scale).unwrap_or(Vec3::ONE);
            // Find killer entity and faction for XP granting
            let killer_entity = attackers_with_target
                .iter()
                .find(|(_, at, _, _)| at.0 == *dead_entity)
                .map(|(e, _, _, _)| e);
            let killer_faction = killer_entity
                .and_then(|e| attacker_factions.get(e).ok())
                .copied();

            // Grant XP to killer
            if let Some(killer_e) = killer_entity {
                let dead_max_hp = dead
                    .iter()
                    .find(|(e, ..)| *e == *dead_entity)
                    .map(|(_, h, ..)| h.max)
                    .unwrap_or(50.0);
                let xp = (dead_max_hp / 5.0) as u32;
                if let Ok(mut exp) = experience_q.get_mut(killer_e) {
                    exp.current += xp;
                    // Check for level-up
                    if let Some((next_level, threshold)) = exp.level.next() {
                        if exp.current >= threshold {
                            exp.level = next_level;
                        }
                    }
                }
            }

            commands
                .entity(*dead_entity)
                .remove::<Unit>()
                .remove::<AttackTarget>()
                .remove::<MoveTarget>()
                .remove::<Engagement>()
                .remove::<MeleeContact>()
                .remove::<AttackWindup>()
                .remove::<AttackRecovery>()
                .remove::<AttackCooldown>()
                .remove::<ReservedIncomingDamage>()
                .insert(Dying {
                    timer: Timer::from_seconds(1.5, TimerMode::Once),
                    _killed_by: killer_faction,
                    original_scale: scale,
                });
        }
    }
}

fn tick_dying(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut dying: Query<(Entity, &mut Dying, &mut Transform)>,
) {
    for (entity, mut dying, mut tf) in &mut dying {
        dying.timer.tick(time.delta());

        // Shrink during the last 0.4 seconds
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

// ── Scored targeting ──

pub struct TargetScoreInput<'a> {
    pub profile: &'a TargetingProfile,
    pub attacker_pos: Vec3,
    pub attacker_damage_type: DamageType,
    pub scan_range: f32,
    pub target_pos: Vec3,
    pub target_health: &'a Health,
    pub target_armor: ArmorType,
    pub target_threat: f32,
    pub target_is_building: bool,
    pub target_reserved_damage: f32,
}

/// Pure scoring function for target selection. Lower score = better target.
/// Returns `None` if the target should be hard-rejected.
pub fn target_score(input: &TargetScoreInput) -> Option<f32> {
    // Hard reject: dead or dying
    if input.target_health.current <= 0.0 {
        return None;
    }

    let dx = input.target_pos.x - input.attacker_pos.x;
    let dz = input.target_pos.z - input.attacker_pos.z;
    let dist = (dx * dx + dz * dz).sqrt();

    // Hard reject: outside scan range (allow slight overshoot)
    if dist > input.scan_range * 1.5 {
        return None;
    }

    let dist_norm = (dist / input.scan_range.max(0.1)).clamp(0.0, 1.5);

    let hp_frac = (input.target_health.current / input.target_health.max.max(1.0)).clamp(0.0, 1.0);

    let multiplier = input.attacker_damage_type.multiplier_vs(input.target_armor);

    let overkill_frac = if input.target_health.current > 0.0 {
        (input.target_reserved_damage / input.target_health.current).min(2.0)
    } else {
        2.0
    };

    // All terms: lower = more desirable target
    let distance_term = input.profile.distance_weight * dist_norm;
    let low_hp_term = input.profile.low_hp_weight * hp_frac; // low hp_frac → low score → preferred
    let threat_term = -input.profile.threat_weight * input.target_threat;
    let counter_term = -input.profile.counter_weight * (multiplier - 1.0);
    let building_term = if input.target_is_building {
        input.profile.building_penalty
    } else {
        0.0
    };
    let reserved_term = input.profile.reserved_damage_penalty * overkill_frac;

    Some(distance_term + low_hp_term + threat_term + counter_term + building_term + reserved_term)
}

// ── Damage reservation TTL tick ──

fn tick_damage_reservations(time: Res<Time<Fixed>>, mut query: Query<&mut ReservedIncomingDamage>) {
    let dt = time.delta_secs();
    for mut reserved in &mut query {
        if reserved.reservations.is_empty() {
            continue;
        }
        reserved.reservations.retain_mut(|(_, _, ttl)| {
            *ttl -= dt;
            *ttl > 0.0
        });
    }
}

/// Emits item VFX triggers when units with equipped items land or receive melee hits.
/// Runs after resolve_attack_windups — detects freshly inserted HitReaction components.
fn emit_item_combat_vfx(
    mut vfx_writer: MessageWriter<ItemVfxTrigger>,
    // Units that just got hit (have HitReaction with freshly added timer)
    hit_targets: Query<(Entity, &Transform, &UnitInventory, &HitReaction), Changed<HitReaction>>,
    // Attackers with active lunge (just landed a hit)
    hit_attackers: Query<
        (
            Entity,
            &Transform,
            &UnitInventory,
            &AttackLunge,
            &AttackTarget,
        ),
        Changed<AttackLunge>,
    >,
    target_transforms: Query<&Transform>,
    target_healths: Query<&Health>,
) {
    // Defensive item triggers (target was hit)
    for (_entity, transform, inventory, _reaction) in &hit_targets {
        let pos = transform.translation;
        for &item in &inventory.items {
            match item {
                ItemKind::PaddedVest => {
                    vfx_writer.write(ItemVfxTrigger {
                        owner: _entity,
                        item,
                        kind: ItemVfxTriggerKind::RangedHitAbsorbed { pos },
                    });
                }
                ItemKind::BronzeCuirass => {
                    vfx_writer.write(ItemVfxTrigger {
                        owner: _entity,
                        item,
                        kind: ItemVfxTriggerKind::MeleeDeflect { pos },
                    });
                }
                ItemKind::PlateCuirass => {
                    vfx_writer.write(ItemVfxTrigger {
                        owner: _entity,
                        item,
                        kind: ItemVfxTriggerKind::LethalPrevented { pos },
                    });
                }
                ItemKind::CrusaderHelm => {
                    vfx_writer.write(ItemVfxTrigger {
                        owner: _entity,
                        item,
                        kind: ItemVfxTriggerKind::CCReduced { pos },
                    });
                }
                ItemKind::KettleHelm => {
                    vfx_writer.write(ItemVfxTrigger {
                        owner: _entity,
                        item,
                        kind: ItemVfxTriggerKind::HeightDeflect { pos },
                    });
                }
                _ => {}
            }
        }
    }

    // Offensive item triggers (attacker landed a melee hit)
    for (_entity, atk_tf, inventory, lunge, attack_target) in &hit_attackers {
        let target_pos = target_transforms
            .get(attack_target.0)
            .map(|t| t.translation)
            .unwrap_or(atk_tf.translation + lunge.direction * 1.5);

        let target_hp_low = target_healths
            .get(attack_target.0)
            .map(|h| h.current / h.max < 0.3)
            .unwrap_or(false);

        for &item in &inventory.items {
            match item {
                ItemKind::ArmingSword => {
                    vfx_writer.write(ItemVfxTrigger {
                        owner: _entity,
                        item,
                        kind: ItemVfxTriggerKind::BleedProc {
                            pos: target_pos,
                            direction: lunge.direction,
                        },
                    });
                }
                ItemKind::VikingBlade if target_hp_low => {
                    vfx_writer.write(ItemVfxTrigger {
                        owner: _entity,
                        item,
                        kind: ItemVfxTriggerKind::ExecuteStrike { target_pos },
                    });
                }
                ItemKind::BattleStaff => {
                    vfx_writer.write(ItemVfxTrigger {
                        owner: _entity,
                        item,
                        kind: ItemVfxTriggerKind::SplashImpact { pos: target_pos },
                    });
                }
                _ => {}
            }
        }
    }
}

/// Emits item VFX triggers when a unit with kill-rewarding items gets a kill.
/// Runs after handle_death. Finds the killer by checking who was attacking the dying entity.
fn emit_item_death_vfx(
    mut vfx_writer: MessageWriter<ItemVfxTrigger>,
    dying_q: Query<Entity, Added<Dying>>,
    inventory_q: Query<(&Transform, &UnitInventory)>,
    attacker_q: Query<(Entity, &AttackTarget)>,
) {
    for dead_entity in &dying_q {
        // Find the killer: who was targeting this entity
        let Some((killer_entity, _)) = attacker_q.iter().find(|(_, at)| at.0 == dead_entity) else {
            continue;
        };
        let Ok((killer_tf, inventory)) = inventory_q.get(killer_entity) else {
            continue;
        };

        for &item in &inventory.items {
            match item {
                ItemKind::VikingHelm => {
                    vfx_writer.write(ItemVfxTrigger {
                        owner: killer_entity,
                        item,
                        kind: ItemVfxTriggerKind::KillMoveBurst {
                            pos: killer_tf.translation,
                        },
                    });
                }
                ItemKind::GoldenBand => {
                    vfx_writer.write(ItemVfxTrigger {
                        owner: killer_entity,
                        item,
                        kind: ItemVfxTriggerKind::EnergyRestored {
                            pos: killer_tf.translation,
                        },
                    });
                }
                _ => {}
            }
        }
    }
}

/// Derives the combat-related variants of `UnitState` from the canonical
/// entry-state components (`CombatOrder` + `AttackTarget` + `MoveTarget`).
///
/// After Step 5 of the combat refactor, this is the *only* place that writes
/// `UnitState::{Attacking, AttackMoving, HoldPosition, Moving, Idle}` for
/// combat-related transitions. Non-combat states (`Gathering`, `Building`,
/// `AssignedGathering`, etc.) own their own transitions and are left alone.
///
/// Priority order (first matching wins):
///   1. `AttackTarget` present and order is not `Hold` → `Attacking(target)`
///   2. `CombatGoal::Hold`                              → `HoldPosition`
///   3. `CombatGoal::AttackMove(dest)`                  → `AttackMoving(dest)`
///   4. `CombatGoal::Move(dest)` or lone `MoveTarget`   → `Moving(dest)`
///   5. `CombatGoal::Stop` / no order                   → `Idle` (only if
///                                                        currently a combat
///                                                        variant)
///
/// Entities without a `CombatOrder` yet fall back to `CombatIntent` — this
/// covers the one-tick gap between wall-redirect runtime mutation and the
/// next entry-API write.
fn sync_display_state(
    mut actors: Query<
        (
            Option<&CombatOrder>,
            Option<&CombatIntent>,
            Option<&AttackTarget>,
            Option<&MoveTarget>,
            &mut UnitState,
        ),
        (Or<(With<Unit>, With<Mob>)>, Without<Dying>),
    >,
) {
    for (order, intent, attack_target, move_target, mut state) in &mut actors {
        // Never override non-combat states — they own their own transitions.
        match *state {
            UnitState::Building(_)
            | UnitState::MovingToBuild(_)
            | UnitState::MovingToPlot(_)
            | UnitState::AssignedGathering { .. }
            | UnitState::Depositing { .. }
            | UnitState::ReturningToDeposit { .. }
            | UnitState::WaitingForStorage { .. }
            | UnitState::WaitingForDepot { .. }
            | UnitState::Gathering(_)
            | UnitState::Patrolling { .. } => continue,
            _ => {}
        }

        // Canonical goal: CombatOrder first, fall back to legacy CombatIntent.
        let goal = order.map(|o| o.goal).or_else(|| {
            intent.map(|i| match *i {
                CombatIntent::None => CombatGoal::Stop,
                CombatIntent::Move(dest) => CombatGoal::Move(dest),
                CombatIntent::Attack(target, _) => CombatGoal::Attack(target),
                CombatIntent::AttackMove(dest, _) => CombatGoal::AttackMove(dest),
                CombatIntent::Hold => CombatGoal::Hold,
            })
        });

        // Priority 1: active target → Attacking (unless Hold, which fires in place).
        if let Some(at) = attack_target {
            if !matches!(goal, Some(CombatGoal::Hold)) {
                let new_state = UnitState::Attacking(at.0);
                if *state != new_state {
                    *state = new_state;
                }
                continue;
            }
        }

        match goal {
            Some(CombatGoal::Hold) => {
                if *state != UnitState::HoldPosition {
                    *state = UnitState::HoldPosition;
                }
            }
            Some(CombatGoal::AttackMove(dest)) => {
                if !matches!(*state, UnitState::AttackMoving(d) if d.distance(dest) <= 0.1) {
                    *state = UnitState::AttackMoving(dest);
                }
            }
            Some(CombatGoal::Move(dest)) => {
                if !matches!(*state, UnitState::Moving(d) if d.distance(dest) <= 0.1) {
                    *state = UnitState::Moving(dest);
                }
            }
            Some(CombatGoal::Attack(_)) => {
                // Order wants an attack but the pipeline has not attached an
                // AttackTarget yet (mid-acquisition). Leave display state alone.
            }
            Some(CombatGoal::Stop) | None => {
                // Bare MoveTarget with no order: reflect it as Moving.
                if order.is_none() && intent.is_none() {
                    if let Some(mt) = move_target {
                        if !matches!(*state, UnitState::Moving(d) if d.distance(mt.0) <= 0.1) {
                            *state = UnitState::Moving(mt.0);
                        }
                        continue;
                    }
                }
                // Drop combat variants to Idle; leave other non-combat alone.
                if matches!(
                    *state,
                    UnitState::Attacking(_) | UnitState::AttackMoving(_) | UnitState::HoldPosition
                ) {
                    *state = UnitState::Idle;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_health(current: f32, max: f32) -> Health {
        Health { current, max }
    }

    fn base_profile() -> TargetingProfile {
        TargetingProfile {
            distance_weight: 1.0,
            low_hp_weight: 1.0,
            threat_weight: 1.0,
            counter_weight: 1.0,
            building_penalty: 3.0,
            reserved_damage_penalty: 1.0,
        }
    }

    #[test]
    fn rejects_dead_target() {
        let profile = base_profile();
        let health = make_health(0.0, 100.0);
        let result = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 10.0,
            target_pos: Vec3::new(3.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Light,
            target_threat: 1.0,
            target_is_building: false,
            target_reserved_damage: 0.0,
        });
        assert!(result.is_none());
    }

    #[test]
    fn rejects_out_of_range() {
        let profile = base_profile();
        let health = make_health(50.0, 100.0);
        let result = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 5.0,
            target_pos: Vec3::new(20.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Light,
            target_threat: 1.0,
            target_is_building: false,
            target_reserved_damage: 0.0,
        });
        assert!(result.is_none());
    }

    #[test]
    fn prefers_closer_target() {
        let profile = base_profile();
        let health = make_health(80.0, 100.0);
        let close = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 15.0,
            target_pos: Vec3::new(2.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Light,
            target_threat: 1.0,
            target_is_building: false,
            target_reserved_damage: 0.0,
        })
        .unwrap();
        let far = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 15.0,
            target_pos: Vec3::new(12.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Light,
            target_threat: 1.0,
            target_is_building: false,
            target_reserved_damage: 0.0,
        })
        .unwrap();
        assert!(close < far, "closer target should score lower (better)");
    }

    #[test]
    fn prefers_low_hp_target() {
        let profile = base_profile();
        let low_hp = make_health(10.0, 100.0);
        let high_hp = make_health(90.0, 100.0);
        let low = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 15.0,
            target_pos: Vec3::new(5.0, 0.0, 0.0),
            target_health: &low_hp,
            target_armor: ArmorType::Light,
            target_threat: 1.0,
            target_is_building: false,
            target_reserved_damage: 0.0,
        })
        .unwrap();
        let high = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 15.0,
            target_pos: Vec3::new(5.0, 0.0, 0.0),
            target_health: &high_hp,
            target_armor: ArmorType::Light,
            target_threat: 1.0,
            target_is_building: false,
            target_reserved_damage: 0.0,
        })
        .unwrap();
        assert!(low < high, "low HP target should score lower (better)");
    }

    #[test]
    fn prefers_high_threat_target() {
        let profile = base_profile();
        let health = make_health(80.0, 100.0);
        let high_threat = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 15.0,
            target_pos: Vec3::new(5.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Light,
            target_threat: 2.0,
            target_is_building: false,
            target_reserved_damage: 0.0,
        })
        .unwrap();
        let low_threat = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 15.0,
            target_pos: Vec3::new(5.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Light,
            target_threat: 0.2,
            target_is_building: false,
            target_reserved_damage: 0.0,
        })
        .unwrap();
        assert!(
            high_threat < low_threat,
            "high threat target should score lower (better)"
        );
    }

    #[test]
    fn building_penalty_applied() {
        let profile = base_profile();
        let health = make_health(80.0, 100.0);
        let unit = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 15.0,
            target_pos: Vec3::new(5.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Light,
            target_threat: 1.0,
            target_is_building: false,
            target_reserved_damage: 0.0,
        })
        .unwrap();
        let building = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 15.0,
            target_pos: Vec3::new(5.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Structure,
            target_threat: 1.0,
            target_is_building: true,
            target_reserved_damage: 0.0,
        })
        .unwrap();
        assert!(unit < building, "units should be preferred over buildings");
    }

    #[test]
    fn negative_building_penalty_prefers_buildings() {
        let mut profile = base_profile();
        profile.building_penalty = -3.0; // siege profile
        let health = make_health(200.0, 200.0);
        let unit = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::SiegeDmg,
            scan_range: 15.0,
            target_pos: Vec3::new(5.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Light,
            target_threat: 1.0,
            target_is_building: false,
            target_reserved_damage: 0.0,
        })
        .unwrap();
        let building = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::SiegeDmg,
            scan_range: 15.0,
            target_pos: Vec3::new(5.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Structure,
            target_threat: 1.0,
            target_is_building: true,
            target_reserved_damage: 0.0,
        })
        .unwrap();
        assert!(
            building < unit,
            "siege should prefer buildings with negative penalty"
        );
    }

    #[test]
    fn reserved_damage_discourages_overkill() {
        let profile = base_profile();
        let health = make_health(50.0, 100.0);
        let no_reservation = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 15.0,
            target_pos: Vec3::new(5.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Light,
            target_threat: 1.0,
            target_is_building: false,
            target_reserved_damage: 0.0,
        })
        .unwrap();
        let with_reservation = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 15.0,
            target_pos: Vec3::new(5.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Light,
            target_threat: 1.0,
            target_is_building: false,
            target_reserved_damage: 45.0,
        })
        .unwrap();
        assert!(
            no_reservation < with_reservation,
            "target with reserved damage should score higher (worse)"
        );
    }
}
