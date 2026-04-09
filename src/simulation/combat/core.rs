use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

use crate::blueprints::{EntityKind, IsRanged};
use crate::types::*;
use crate::simulation::items::{ItemKind, SpawnItemPickup, UnitInventory};
use crate::simulation::items::vfx::{ItemVfxTrigger, ItemVfxTriggerKind};
use crate::infrastructure::multiplayer::NetRole;
use crate::simulation::mobs::CampItemDrops;
use crate::world::spatial::{SpatialHashGrid, WallSpatialGrid};

use super::{slot_anchor, CombatBudgetState};

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                tick_damage_reservations,
                resolve_combat_intents,
                approach_attack_target,
                start_attack_windups,
                resolve_attack_windups,
                emit_item_combat_vfx,
                tick_attack_recovery,
                explode_props,
                handle_death,
                emit_item_death_vfx,
                tick_dying,
            )
                .chain()
                .in_set(GameFlowSet::Simulation)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn intended_attack_target(
    intent: Option<&CombatIntent>,
    target_lock: Option<&CombatTargetLock>,
) -> Option<Entity> {
    match intent {
        Some(CombatIntent::Attack(target, _)) => Some(*target),
        Some(CombatIntent::AttackMove(_, _)) => target_lock.map(|lock| lock.target),
        Some(CombatIntent::Hold) => target_lock.map(|lock| lock.target),
        _ => None,
    }
}

fn resolve_combat_intents(
    mut commands: Commands,
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
    all_entities: Query<()>,
    mut actors: Query<
        (
            Entity,
            Option<&Faction>,
            Option<&CombatIntent>,
            Option<&CombatTargetLock>,
            Option<&mut UnitState>,
            Option<&AttackTarget>,
            Option<&MoveTarget>,
            Option<&AttackWindup>,
            Option<&AttackRecovery>,
        ),
        Or<(With<Unit>, With<Mob>)>,
    >,
) {
    for (
        entity,
        faction,
        intent,
        target_lock,
        opt_state,
        attack_target,
        move_target,
        windup,
        _recovery,
    ) in &mut actors
    {
        if net_role.as_ref() == &NetRole::Client && faction.is_some_and(|f| *f != active_player.0) {
            continue;
        }

        // Windup is the true commit point. Recovery should still accept the next move/order.
        let committed = windup.is_some();
        let desired_target = intended_attack_target(intent, target_lock)
            .filter(|target| all_entities.contains(*target));

        if let Some(target) = desired_target {
            let needs_sync = attack_target.map_or(true, |current| current.0 != target);
            if needs_sync && !committed {
                commands
                    .entity(entity)
                    .insert(AttackTarget(target))
                    .remove::<MoveTarget>();
                // Hold intent: keep HoldPosition state (attack in place, don't chase)
                let is_hold = matches!(intent, Some(CombatIntent::Hold));
                if let Some(mut state) = opt_state {
                    if !is_hold {
                        *state = UnitState::Attacking(target);
                    }
                    // HoldPosition stays — unit fires without moving
                }
            }
            continue;
        }

        match intent.copied().unwrap_or_default() {
            CombatIntent::AttackMove(destination, _) => {
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
                if let Some(mut state) = opt_state {
                    if !matches!(*state, UnitState::AttackMoving(dest) if dest.distance(destination) <= 0.1)
                        && !committed
                    {
                        *state = UnitState::AttackMoving(destination);
                    }
                }
            }
            CombatIntent::Move(destination) => {
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
                if let Some(mut state) = opt_state {
                    if !matches!(*state, UnitState::Moving(dest) if dest.distance(destination) <= 0.1)
                        && !committed
                    {
                        *state = UnitState::Moving(destination);
                    }
                }
            }
            CombatIntent::Hold => {
                if !committed {
                    // Hold: never move, but keep AttackTarget if we have a valid lock
                    // (target scanning is done in unit_state_executor_system)
                    commands
                        .entity(entity)
                        .remove::<MoveTarget>()
                        .remove::<ChaseTimer>();
                    if let Some(mut state) = opt_state {
                        *state = UnitState::HoldPosition;
                    }
                }
            }
            CombatIntent::None => {
                if attack_target.is_some() && !committed {
                    commands
                        .entity(entity)
                        .remove::<AttackTarget>()
                        .remove::<ChaseTimer>();
                    if let Some(mut state) = opt_state {
                        if matches!(*state, UnitState::Attacking(_)) {
                            *state = UnitState::Idle;
                        }
                    }
                }
            }
            CombatIntent::Attack(_, _) => {}
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
    vfx_assets: Option<Res<VfxAssets>>,
    net_role: Res<NetRole>,
    mut queries: ParamSet<(
        Query<(Entity, &Transform, &ExplosiveProp, &Health)>,
        Query<(Entity, &mut Transform, &mut Health), Without<Projectile>>,
    )>,
) {
    let Some(vfx) = vfx_assets else { return };
    // Client: skip explosion damage — host handles it and syncs health
    if *net_role == NetRole::Client {
        return;
    }

    let detonations: Vec<_> = queries
        .p0()
        .iter()
        .filter(|(_, _, _, health)| health.current <= 0.0)
        .map(|(entity, tf, prop, _)| (entity, tf.translation, *prop))
        .collect();

    for (source_entity, origin, prop) in detonations {
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

            health.current -= prop.damage * falloff;
            if dist > 0.05 {
                let push = Vec3::new(offset.x, 0.0, offset.z).normalize_or_zero() * falloff * 0.9;
                target_tf.translation += push;
            }
        }
    }
}

pub fn approach_attack_target(
    mut commands: Commands,
    time: Res<Time>,
    combat_budget: Res<CombatBudget>,
    mut budget_state: ResMut<CombatBudgetState>,
    teams: Res<TeamConfig>,
    wall_grid: Res<WallSpatialGrid>,
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
    mut attackers: Query<
        (
            Entity,
            &Transform,
            &AttackTarget,
            &AttackRange,
            Option<&AttackTiming>,
            Option<&IsRanged>,
            &Faction,
            Option<&mut UnitState>,
            Option<&MoveTarget>,
            Option<&AttackWindup>,
            Option<&AttackRecovery>,
            Option<&mut ChaseTimer>,
            Option<&TaskSource>,
            Option<&mut CombatTargetLock>,
            Option<&SlotClaim>,
        ),
        Or<(With<Unit>, With<Mob>)>,
    >,
    wall_check: Query<
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
    all_transforms: Query<&Transform>,
    building_footprints: Query<&BuildingFootprint, With<Building>>,
    tactical_roles: Query<&TacticalRole>,
    spatial_grid: Res<SpatialHashGrid>,
    factions: Query<&Faction>,
    mut nearby_entities: Local<Vec<(Entity, Vec3)>>,
) {
    for (
        attacker_entity,
        tf,
        attack_target,
        range,
        attack_timing,
        is_ranged,
        faction,
        opt_state,
        current_move_target,
        windup,
        recovery,
        opt_chase_timer,
        opt_task_source,
        opt_target_lock,
        slot_claim,
    ) in &mut attackers
    {
        // During windup/recovery, unit is locked in animation — skip
        if windup.is_some() || recovery.is_some() {
            continue;
        }
        // Client: only approach for local player's units
        if *net_role == NetRole::Client && *faction != active_player.0 {
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
                    if let Some(mut state) = opt_state {
                        if matches!(*state, UnitState::Attacking(_)) {
                            *state = UnitState::Attacking(wall_entity);
                        }
                    }
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
                    } else {
                        commands.entity(attacker_entity).insert(CombatTargetLock {
                            target: wall_entity,
                            locked_until: time.elapsed_secs_f64() + 0.5,
                            source,
                        });
                    }
                    continue;
                }
            }
        }

        // ── Distance check (2D only — ignore terrain height) ──
        let surface_dist =
            attack_surface_distance(tf.translation, target_tf.translation, target_radius);
        let in_band = is_in_attack_band(surface_dist, range.0, minimum_range, 0.15);

        if !in_band {
            // Reposition until the target sits inside the allowed attack band.
            if budget_state.repath_requests_this_frame
                >= combat_budget.max_repath_requests_per_frame
            {
                continue;
            }
            let desired_pos = if is_ranged.is_none()
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
                // Start chase timer
                let max_secs = if opt_task_source.map_or(false, |s| *s == TaskSource::Manual) {
                    10.0
                } else {
                    6.0
                };
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

            // Ranged kiting: if a melee enemy is dangerously close, retreat backward
            if is_ranged.is_some()
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
                    if tactical_roles
                        .get(*nearby_entity)
                        .ok()
                        .is_some_and(|r| *r == TacticalRole::RangedKiter || *r == TacticalRole::SiegeSupport)
                    {
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
                    let retreat_pos = tf.translation
                        + Vec3::new(dir.x, 0.0, dir.y) * 4.0;
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
    time: Res<Time>,
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
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
        faction,
        windup,
        recovery,
        opt_status,
    ) in &mut attackers
    {
        if *net_role == NetRole::Client && *faction != active_player.0 {
            continue;
        }
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

        // Reserve damage on the target
        if let Ok(mut reserved) = reserved_q.get_mut(attack_target.0) {
            let ttl = profile.windup_secs + 2.0;
            reserved.reservations.push((entity, atk_damage.0, ttl));
        }
    }
}

fn resolve_attack_windups(
    mut commands: Commands,
    time: Res<Time>,
    vfx_assets: Option<Res<VfxAssets>>,
    projectile_assets: Option<Res<crate::presentation::model_assets::ProjectileModelAssets>>,
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
    mut attackers: Query<(
        Entity,
        &Transform,
        &AttackProfile,
        &CombatFxKind,
        &AttackDamage,
        &AttackRange,
        Option<&AttackTiming>,
        Option<&IsRanged>,
        &Faction,
        Option<&DamageType>,
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
        is_ranged,
        faction,
        opt_dmg_type,
        mut windup,
        opt_charge,
        opt_entity_kind,
    ) in &mut attackers
    {
        // Client: only execute attacks for local player's units
        if *net_role == NetRole::Client && *faction != active_player.0 {
            continue;
        }
        windup.remaining_secs -= time.delta_secs();
        if windup.remaining_secs > 0.0 {
            continue;
        }

        let target = windup.target;
        let Ok((target_tf, mut health, opt_armor, opt_reserved, opt_existing_recoil)) = healths.get_mut(target) else {
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

        // Compute damage multiplier from damage type vs armor type
        let multiplier = match (opt_dmg_type, opt_armor) {
            (Some(dmg_type), Some(armor_type)) => dmg_type.multiplier_vs(*armor_type),
            _ => 1.0,
        };

        if is_ranged.is_some() {
            // Replace windup reservation with projectile-travel reservation
            if let Some(mut reserved) = opt_reserved {
                reserved.reservations.retain(|(src, _, _)| *src != entity);
                let travel_ttl = surface_dist / profile.projectile_speed.max(8.0) + 0.35;
                reserved.reservations.push((entity, damage.0, travel_ttl));
            }
            // Ranged: spawn projectile (carries damage_type for on-hit multiplier)
            let proj_visual =
                opt_entity_kind.and_then(|k| crate::presentation::model_assets::projectile_visual_for(*k));
            let use_model = proj_visual.is_some() && projectile_assets.is_some();
            let orient = use_model
                && !matches!(
                    proj_visual,
                    Some(crate::presentation::model_assets::ProjectileVisualKind::CatapultRock)
                );
            let proj_component = Projectile {
                source: entity,
                target,
                speed: profile.projectile_speed.max(8.0),
                damage: damage.0,
                damage_type: opt_dmg_type.copied().unwrap_or(DamageType::Melee),
                fx_kind: *fx_kind,
                impact_scale: profile.impact_scale,
                orient_to_velocity: orient,
            };
            let spawn_pos = atk_tf.translation + Vec3::Y * 0.5;
            let dir_to_target = (target_tf.translation - spawn_pos).normalize_or_zero();
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
            // Clear windup reservation — damage applied immediately
            if let Some(mut reserved) = opt_reserved {
                reserved.reservations.retain(|(src, _, _)| *src != entity);
            }
            let charge_mult = opt_charge.map(|c| c.damage_mult).unwrap_or(1.0);
            let dealt = damage.0 * multiplier * charge_mult;
            health.current -= dealt;
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
    }
}

fn tick_attack_recovery(
    mut commands: Commands,
    time: Res<Time>,
    mut recoveries: Query<(Entity, &mut AttackRecovery)>,
) {
    for (entity, mut recovery) in &mut recoveries {
        recovery.remaining_secs -= time.delta_secs();
        if recovery.remaining_secs <= 0.0 {
            commands.entity(entity).remove::<AttackRecovery>();
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
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
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
        (Entity, &AttackTarget, Option<&mut PatrolState>),
        Without<Dying>,
    >,
    mut experience_q: Query<&mut Experience>,
    mut all_assigned_workers: Query<&mut AssignedWorkers>,
    workers_with_state: Query<(Entity, &UnitState), With<Unit>>,
    time: Res<Time>,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
    mut all_resources: ResMut<AllPlayerResources>,
    attacker_factions: Query<&Faction, With<AttackTarget>>,
    mut wall_grid: ResMut<WallGrid>,
    wall_coord_q: Query<&WallGridCoord>,
) {
    let is_client = *net_role == NetRole::Client;
    // Collect dead entities first to avoid borrow issues
    // On client: only detect death for local player's entities (remote deaths come via EntityDespawn)
    let dead_list: Vec<_> = dead
        .iter()
        .filter(|(_, health, _, _, _, _, _, opt_faction, _, _)| {
            if health.current > 0.0 {
                return false;
            }
            if is_client {
                // Only handle death for local player's entities
                opt_faction.map_or(false, |f| *f == active_player.0)
            } else {
                true
            }
        })
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
        // Grant camp reward resources to the killing faction (host only)
        if !is_client {
            if let (Some(drop_table), Some(transform)) = (opt_camp_item_drops, opt_transform) {
                for (idx, &item) in drop_table.items.iter().enumerate() {
                    let angle = if drop_table.items.len() == 1 {
                        0.0
                    } else {
                        idx as f32 / drop_table.items.len() as f32 * std::f32::consts::TAU
                    };
                    let radius = if drop_table.items.len() == 1 { 0.0 } else { 0.9 };
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
                    .find(|(_, at, _)| at.0 == *dead_entity)
                    .and_then(|(attacker_e, _, _)| attacker_factions.get(attacker_e).ok());
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
        }

        for (attacker_entity, attack_target, opt_patrol) in &mut attackers_with_target {
            if attack_target.0 == *dead_entity {
                commands
                    .entity(attacker_entity)
                    .remove::<AttackTarget>()
                    .remove::<CombatTargetLock>()
                    .remove::<SlotClaim>()
                    .insert(CombatIntent::None);
                if let Some(mut patrol) = opt_patrol {
                    patrol.state = PatrolStateKind::Returning;
                }
            }
        }

        // If a worker dies while assigned to a processor, remove it from AssignedWorkers
        if let Some(UnitState::AssignedGathering { building, .. }) = opt_unit_state {
            crate::simulation::buildings::remove_assigned_worker(&mut commands, *building, *dead_entity);
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
                .find(|(_, at, _)| at.0 == *dead_entity)
                .map(|(e, _, _)| e);
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
    time: Res<Time>,
    mut dying: Query<(Entity, &mut Dying, &mut Transform), Without<ProceduralMob>>,
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

fn tick_damage_reservations(time: Res<Time>, mut query: Query<&mut ReservedIncomingDamage>) {
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
    hit_targets: Query<
        (Entity, &Transform, &UnitInventory, &HitReaction),
        Changed<HitReaction>,
    >,
    // Attackers with active lunge (just landed a hit)
    hit_attackers: Query<
        (Entity, &Transform, &UnitInventory, &AttackLunge, &AttackTarget),
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
        let Some((killer_entity, _)) = attacker_q
            .iter()
            .find(|(_, at)| at.0 == dead_entity)
        else {
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
