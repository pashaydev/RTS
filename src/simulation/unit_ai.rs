use bevy::prelude::*;
use bevy::time::Fixed;
use std::f32::consts::TAU;

use crate::blueprints::EntityKind;
use crate::simulation::buildings::is_wall_like_kind;
use crate::simulation::combat::{
    apply_auto_attack_intent, apply_auto_move_intent, apply_manual_attack_intent,
    apply_manual_attack_move_intent, apply_manual_hold_intent, apply_manual_move_intent,
    clear_combat_intent, reset_combat_state, set_auto_target, target_score,
    CombatBudgetState, TargetScoreInput,
};
use crate::types::*;
use crate::world::spatial::SpatialHashGrid;

pub struct UnitAiPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum UnitAiSet {
    Hotspots,
    Decision,
    TaskAdvance,
    Execute,
    Leash,
    Heal,
}

impl Plugin for UnitAiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatHotspots>()
            .configure_sets(
                FixedUpdate,
                (
                    UnitAiSet::Hotspots,
                    UnitAiSet::Decision,
                    UnitAiSet::TaskAdvance,
                    UnitAiSet::Execute,
                    UnitAiSet::Leash,
                    UnitAiSet::Heal,
                )
                    .chain(),
            )
            .add_systems(
                FixedUpdate,
                (cleanup_recent_combat_damage, update_combat_hotspots)
                    .in_set(UnitAiSet::Hotspots)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                decision_priority_system
                    .in_set(UnitAiSet::Decision)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                task_queue_advance_system
                    .in_set(UnitAiSet::TaskAdvance)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                unit_state_executor_system
                    .in_set(UnitAiSet::Execute)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                leash_return_system
                    .in_set(UnitAiSet::Leash)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                auto_heal_system
                    .in_set(UnitAiSet::Heal)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Decision priority system — runs every 0.2s and evaluates what idle/auto units should do.
/// Priority order:
/// 1. Manual task → skip (handled by task_queue_advance)
/// 2. Survival retreat (hp < 25%, non-Aggressive stance)
/// 3. Threat response by stance (Defensive/Aggressive auto-engage)
/// 4. Auto-role behavior (handled by worker_ai_system for Economy)
/// 5. Idle
/// Number of frames over which to spread unit AI decisions within one timer period.

/// Collects positions of units currently in combat for ally-assist detection.
fn update_combat_hotspots(
    mut hotspots: ResMut<CombatHotspots>,
    units: Query<
        (
            &Transform,
            &UnitState,
            &Faction,
            Option<&RecentCombatDamage>,
        ),
        With<Unit>,
    >,
) {
    hotspots.spots.clear();
    for (tf, state, faction, recent_damage) in &units {
        if let UnitState::Attacking(target) = *state {
            hotspots.spots.push((tf.translation, target, *faction));
            continue;
        }
        if let Some(recent_damage) = recent_damage {
            hotspots
                .spots
                .push((tf.translation, recent_damage.attacker, *faction));
        }
    }
    hotspots.spots.truncate(128);
}

fn cleanup_recent_combat_damage(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    memories: Query<(Entity, &RecentCombatDamage)>,
) {
    let now = time.elapsed_secs_f64();
    for (entity, memory) in &memories {
        if now > memory.expires_at {
            commands.entity(entity).remove::<RecentCombatDamage>();
        }
    }
}

fn decision_priority_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    combat_tuning: Res<CombatTuning>,
    budgeting: (Res<CombatBudget>, ResMut<CombatBudgetState>),
    teams: Res<TeamConfig>,
    spatial_grid: Res<SpatialHashGrid>,
    hotspots: Res<CombatHotspots>,
    mut units: Query<
        (
            Entity,
            &Transform,
            &mut UnitState,
            &mut TaskSource,
            &UnitStance,
            &Faction,
            &Health,
            Option<&AttackRange>,
            &TaskQueue,
            (
                Option<&TargetingProfile>,
                Option<&DamageType>,
                Option<&RecentCombatDamage>,
                Option<&CombatIntent>,
                Option<&mut CombatThinkTimer>,
            ),
            (Option<&TacticalRole>, Option<&ManualIdleSince>),
        ),
        With<Unit>,
    >,
    factions: Query<&Faction>,
    building_check: Query<(), With<Building>>,
    deposit_points: Query<(Entity, &Transform, &Faction), (With<DepositPoint>, Without<Unit>)>,
    target_data: Query<(
        &Health,
        &ArmorType,
        Option<&ThreatValue>,
        Option<&ReservedIncomingDamage>,
        Option<&AttackProfile>,
        Option<&TacticalRole>,
    )>,
) {
    let (combat_budget, mut budget_state) = budgeting;
    let mut nearby_targets = Vec::new();

    for (
        entity,
        tf,
        mut state,
        mut source,
        stance,
        faction,
        health,
        attack_range,
        task_queue,
        (
            opt_targeting_profile,
            opt_damage_type,
            opt_recent_damage,
            combat_intent,
            _opt_think_timer,
        ),
        (opt_tactical_role, manual_idle_since),
    ) in units.iter_mut()
    {
        let now = time.elapsed_secs_f64();
        // Skip units with manual orders or queued tasks
        if *source == TaskSource::Manual
            || task_queue.current.is_some()
            || !task_queue.queue.is_empty()
        {
            continue;
        }

        // ── Priority 1: Damage response (bypasses grace period entirely) ──
        // This branch runs BEFORE the busy-state filter so workers in
        // `AssignedGathering` (which would otherwise be considered
        // non-interruptible) drop their gathering loop and fight back. The
        // doc-flagged "workers killed unmolested" bug came from the busy-
        // state filter blocking this path.
        if let Some(recent_damage) = opt_recent_damage {
            let attacker_still_hostile = now <= recent_damage.expires_at
                && factions
                    .get(recent_damage.attacker)
                    .ok()
                    .is_some_and(|target_faction| teams.is_hostile(faction, target_faction));
            if attacker_still_hostile
                && attack_range.is_some()
                && *stance != UnitStance::Passive
                && matches!(
                    *state,
                    UnitState::Idle
                        | UnitState::Gathering(_)
                        | UnitState::AssignedGathering { .. }
                )
            {
                // Workers under attack drop their assignment first so they
                // exit the FSM cleanly, then enter combat as Auto.
                if let UnitState::AssignedGathering { building, .. } = *state {
                    crate::simulation::resources::unassign_worker_from_processor(
                        &mut commands,
                        entity,
                        Some(building),
                    );
                    *state = UnitState::Idle;
                }
                apply_auto_attack_intent(
                    &mut commands,
                    entity,
                    recent_damage.attacker,
                    tf.translation,
                    now,
                );
                *source = TaskSource::Auto;
                commands.entity(entity).remove::<ManualIdleSince>();
                continue;
            }
        }

        // Skip units that are busy with non-interruptible states
        match *state {
            UnitState::Building(_)
            | UnitState::MovingToBuild(_)
            | UnitState::MovingToPlot(_)
            | UnitState::AssignedGathering { .. }
            | UnitState::Depositing { .. }
            | UnitState::ReturningToDeposit { .. }
            | UnitState::WaitingForStorage { .. }
            | UnitState::WaitingForDepot { .. }
            | UnitState::HoldPosition
            | UnitState::Patrolling { .. }
            | UnitState::AttackMoving(_) => continue,
            _ => {}
        }

        // Manual-idle grace period: combat units get a short grace (0.5s) so they
        // react to nearby threats quickly; non-combat units (workers) keep a longer
        // grace (5s) to prevent AI from immediately reassigning them after a manual move.
        let grace_secs = if attack_range.is_some() { 0.5 } else { 5.0 };
        let in_grace = manual_idle_since.is_some_and(|s| now - s.0 < grace_secs);

        // Grace period blocks proactive scanning (but not damage response above)
        if in_grace {
            continue;
        }

        // ── Priority 2: Survival retreat (workers flee at <50%, others <25%) ──
        // Workers are too fragile and too cheap to die heroically — letting one
        // fall back to base preserves population for next-day economy.
        let is_worker = stance == &UnitStance::Defensive && attack_range.is_none();
        let retreat_threshold = if is_worker { 0.50 } else { 0.25 };
        if *stance != UnitStance::Aggressive
            && health.current > 0.0
            && health.current / health.max < retreat_threshold
        {
            // Only trigger retreat if currently being attacked (in Attacking state or being hit)
            if matches!(*state, UnitState::Attacking(_)) {
                // Find nearest allied deposit point to retreat toward
                let mut nearest_depot: Option<(Vec3, f32)> = None;
                for (_depot_entity, depot_tf, depot_faction) in &deposit_points {
                    if !teams.is_allied(faction, depot_faction) {
                        continue;
                    }
                    let dist = tf.translation.distance(depot_tf.translation);
                    if nearest_depot.is_none() || dist < nearest_depot.unwrap().1 {
                        nearest_depot = Some((depot_tf.translation, dist));
                    }
                }

                if let Some((retreat_pos, _)) = nearest_depot {
                    apply_auto_move_intent(&mut commands, entity, retreat_pos);
                    commands
                        .entity(entity)
                        .remove::<AttackTarget>()
                        .insert(MoveTarget(retreat_pos));
                    *state = UnitState::Moving(retreat_pos);
                    *source = TaskSource::Auto;
                    continue;
                }
            }
        }

        // ── Priority 3: Threat response by stance ──
        if *stance == UnitStance::Passive {
            // Passive units never auto-engage
            continue;
        }

        // Only process idle or gathering units for threat response
        if !matches!(*state, UnitState::Idle | UnitState::Gathering(_)) {
            continue;
        }

        if let Some(attack_r) = attack_range {
            if budget_state.target_rescans_this_frame >= combat_budget.max_target_rescans_per_frame
            {
                continue;
            }
            let scan_range = attack_r.0 * combat_tuning.scan_multiplier(*stance);
            if scan_range <= 0.0 {
                continue;
            }
            let mut best_score = f32::MAX;
            let mut best_target = None;

            spatial_grid.collect_radius_limited(
                tf.translation,
                scan_range,
                16,
                &mut nearby_targets,
            );
            budget_state.target_rescans_this_frame += 1;
            for (target_entity, target_pos) in nearby_targets.iter() {
                if *target_entity == entity {
                    continue;
                }
                let is_building = building_check.get(*target_entity).is_ok();
                // Skip buildings unless aggressive stance
                if *stance != UnitStance::Aggressive && is_building {
                    continue;
                }
                let Some(target_faction) = factions.get(*target_entity).ok() else {
                    continue;
                };
                if !teams.is_hostile(faction, target_faction) {
                    continue;
                }

                // Use scored targeting if profile available, else fall back to distance
                if let Some(profile) = opt_targeting_profile {
                    let Ok((t_health, t_armor, t_threat, t_reserved, t_profile, t_role)) =
                        target_data.get(*target_entity)
                    else {
                        continue;
                    };
                    let dmg_type = opt_damage_type.copied().unwrap_or(DamageType::Melee);
                    if let Some(mut score) = target_score(&TargetScoreInput {
                        profile,
                        attacker_pos: tf.translation,
                        attacker_damage_type: dmg_type,
                        scan_range,
                        target_pos: *target_pos,
                        target_health: t_health,
                        target_armor: *t_armor,
                        target_threat: t_threat.map_or(0.0, |t| t.0),
                        target_is_building: is_building,
                        target_reserved_damage: t_reserved.map_or(0.0, |r| r.total()),
                    }) {
                        // Tactical role modifiers
                        let role = opt_tactical_role.copied().unwrap_or_default();
                        let target_is_ranged = t_profile.map_or(false, |p| p.is_ranged())
                            || matches!(
                                t_role,
                                Some(TacticalRole::RangedKiter | TacticalRole::Healer)
                            );
                        match role {
                            TacticalRole::Frontline => {
                                // Prefer engaging ranged/caster threats to protect backline
                                if target_is_ranged {
                                    score -= 0.4;
                                }
                            }
                            TacticalRole::Flanker => {
                                // Aggressively seek backline targets, avoid heavy armor
                                if target_is_ranged {
                                    score -= 0.5;
                                }
                                if *t_armor == ArmorType::Heavy {
                                    score += 0.3;
                                }
                            }
                            _ => {}
                        }

                        if score < best_score {
                            best_score = score;
                            best_target = Some(*target_entity);
                        }
                    }
                } else {
                    // Fallback: nearest enemy
                    let dx = target_pos.x - tf.translation.x;
                    let dz = target_pos.z - tf.translation.z;
                    let dist = (dx * dx + dz * dz).sqrt();
                    if dist < best_score {
                        best_score = dist;
                        best_target = Some(*target_entity);
                    }
                }
            }

            // Ally-assist: if no enemy found in scan range, check if nearby allies are fighting
            if best_target.is_none() {
                let assist_range = match stance {
                    UnitStance::Defensive => combat_tuning.ally_alert_assist_range,
                    UnitStance::Aggressive => combat_tuning.ally_alert_assist_range * 1.75,
                    _ => 0.0,
                };
                if assist_range > 0.0 {
                    let mut best_assist_dist = assist_range;
                    for (spot_pos, spot_target, spot_faction) in &hotspots.spots {
                        if !teams.is_allied(faction, spot_faction) {
                            continue;
                        }
                        let dist = tf.translation.distance(*spot_pos);
                        if dist < best_assist_dist {
                            // Validate target still exists and is hostile
                            if let Ok(target_faction) = factions.get(*spot_target) {
                                if teams.is_hostile(faction, target_faction) {
                                    best_assist_dist = dist;
                                    best_target = Some(*spot_target);
                                }
                            }
                        }
                    }
                }
            }

            if let Some(target) = best_target {
                apply_auto_attack_intent(&mut commands, entity, target, tf.translation, now);
                *source = TaskSource::Auto;
            } else if matches!(
                combat_intent,
                Some(CombatIntent::Attack(_, IntentSource::Auto))
            ) {
                reset_combat_state(&mut commands, entity);
            }
        }
    }
}

/// Leash return system — Defensive units that chased too far return to their origin.
fn leash_return_system(
    mut commands: Commands,
    combat_tuning: Res<CombatTuning>,
    mut units: Query<
        (
            Entity,
            &Transform,
            &mut UnitState,
            &mut TaskSource,
            &UnitStance,
            &LeashOrigin,
            &Faction,
        ),
        With<Unit>,
    >,
) {
    for (entity, tf, mut state, mut source, stance, leash_origin, _faction) in &mut units {
        // Only apply leash to auto-sourced attacks
        if *source != TaskSource::Auto {
            continue;
        }

        if !matches!(*state, UnitState::Attacking(_)) {
            // No longer attacking — clean up leash
            commands.entity(entity).remove::<LeashOrigin>();
            continue;
        }

        let leash_dist = combat_tuning.leash_distance(*stance);
        if leash_dist <= 0.0 {
            commands.entity(entity).remove::<LeashOrigin>();
            continue;
        }

        let dist_from_origin = tf.translation.distance(leash_origin.0);
        if dist_from_origin > leash_dist {
            // Exceeded leash — return to origin
            apply_auto_move_intent(&mut commands, entity, leash_origin.0);
            commands
                .entity(entity)
                .remove::<AttackTarget>()
                .remove::<LeashOrigin>()
                .insert(MoveTarget(leash_origin.0));
            *state = UnitState::Moving(leash_origin.0);
            *source = TaskSource::Auto;
        }
    }
}

pub fn task_queue_advance_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut units: Query<
        (
            Entity,
            &mut UnitState,
            &mut TaskSource,
            &mut TaskQueue,
            &EntityKind,
            &Faction,
        ),
        With<Unit>,
    >,
    transforms: Query<&Transform>,
    _processors: Query<(&ResourceProcessor, &BuildingState, &Faction), With<Building>>,
    _assigned_workers_q: Query<&AssignedWorkers>,
) {
    for (entity, mut state, mut source, mut queue, _kind, _faction) in &mut units {
        if *state != UnitState::Idle || queue.current.is_some() || queue.queue.is_empty() {
            continue;
        }

        let task = queue.queue.pop_front().unwrap();
        queue.current = Some(task.clone());
        *source = TaskSource::Manual;

        match task.task {
            QueuedTask::Move(pos) => {
                *state = UnitState::Moving(pos);
                apply_manual_move_intent(&mut commands, entity, pos, time.elapsed_secs_f64());
                commands.entity(entity).insert(MoveTarget(pos));
            }
            QueuedTask::AttackMove(pos) => {
                apply_manual_attack_move_intent(
                    &mut commands,
                    entity,
                    pos,
                    time.elapsed_secs_f64(),
                );
                commands.entity(entity).insert(MoveTarget(pos));
            }
            QueuedTask::Attack(target) => {
                apply_manual_attack_intent(&mut commands, entity, target, time.elapsed_secs_f64());
            }
            QueuedTask::Gather(node) => {
                clear_combat_intent(&mut commands, entity, time.elapsed_secs_f64());
                if let Ok(node_tf) = transforms.get(node) {
                    commands
                        .entity(entity)
                        .insert(MoveTarget(node_tf.translation));
                }
                *state = UnitState::Gathering(node);
            }
            QueuedTask::Build(building) => {
                clear_combat_intent(&mut commands, entity, time.elapsed_secs_f64());
                if let Ok(building_tf) = transforms.get(building) {
                    commands
                        .entity(entity)
                        .insert(MoveTarget(building_tf.translation));
                }
                *state = UnitState::MovingToBuild(building);
            }
            QueuedTask::Patrol(pos) => {
                clear_combat_intent(&mut commands, entity, time.elapsed_secs_f64());
                if let Ok(unit_tf) = transforms.get(entity) {
                    *state = UnitState::Patrolling {
                        target: pos,
                        origin: unit_tf.translation,
                    };
                    commands.entity(entity).insert(MoveTarget(pos));
                }
            }
            QueuedTask::HoldPosition => {
                apply_manual_hold_intent(&mut commands, entity, time.elapsed_secs_f64());
                commands
                    .entity(entity)
                    .remove::<MoveTarget>()
                    .remove::<AttackTarget>();
            }
        }
    }
}

/// Translates UnitState into low-level component management.
/// Handles arrival detection, state transitions, and MoveTarget/AttackTarget sync.
pub fn unit_state_executor_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    teams: Res<TeamConfig>,
    spatial_grid: Res<SpatialHashGrid>,
    combat_budget: Res<CombatBudget>,
    mut budget_state: ResMut<CombatBudgetState>,
    mut units: Query<
        (
            Entity,
            &Transform,
            &mut UnitState,
            &mut TaskSource,
            &mut TaskQueue,
            &EntityKind,
            &Faction,
            Option<&MoveTarget>,
            Option<&AttackRange>,
            Option<&mut CombatThinkTimer>,
            &UnitStance,
        ),
        With<Unit>,
    >,
    transforms: Query<&Transform, Without<Unit>>,
    _nodes: Query<&ResourceNode>,
    construction_sites: Query<
        (&BuildingState, &Faction, &BuildingFootprint, &EntityKind),
        (With<Building>, With<ConstructionProgress>),
    >,
    processors: Query<(&ResourceProcessor, &BuildingState, &Faction), With<Building>>,
    _assigned_workers_q: Query<&AssignedWorkers>,
    factions: Query<&Faction>,
    entity_check: Query<()>,
) {
    let gather_range = 3.0;
    let build_range = 4.0;
    let wall_build_range_bonus = 2.5;
    let mut nearby_targets = Vec::new();
    let mut corridor_targets = Vec::new();

    for (
        entity,
        tf,
        mut state,
        mut source,
        mut task_queue,
        _kind,
        faction,
        move_target,
        attack_range,
        opt_think_timer,
        stance,
    ) in &mut units
    {
        match *state {
            UnitState::Idle => {
                // Remove stale targets and always clear combat intent so
                // resolve_combat_intents doesn't re-insert a MoveTarget.
                commands
                    .entity(entity)
                    .remove::<MoveTarget>()
                    .remove::<AttackTarget>();
                reset_combat_state(&mut commands, entity);
                // Transition source Manual → Auto once we're confirmed idle.
                // The Moving→Idle path deliberately keeps source=Manual for
                // one tick to guard against same-tick auto-reassignment; by
                // the time we reach here on the next tick, ManualIdleSince
                // (inserted via deferred commands) is in place and the grace
                // period in worker_ai / decision_priority will protect us.
                if *source == TaskSource::Manual {
                    *source = TaskSource::Auto;
                }
            }

            UnitState::HoldPosition => {
                // Never move when holding position
                commands.entity(entity).remove::<MoveTarget>();

                // Auto-attack enemies in weapon range (unless Passive stance)
                if *stance != UnitStance::Passive {
                    if let Some(attack_r) = attack_range {
                        let now = time.elapsed_secs_f64();
                        let can_think = opt_think_timer
                            .as_ref()
                            .map_or(true, |timer| now >= timer.next_think_at);
                        if can_think
                            && budget_state.target_rescans_this_frame
                                < combat_budget.max_target_rescans_per_frame
                        {
                            let scan_range = attack_r.0;
                            spatial_grid.collect_radius_limited(
                                tf.translation,
                                scan_range,
                                8,
                                &mut nearby_targets,
                            );
                            budget_state.target_rescans_this_frame += 1;

                            let mut closest_dist = f32::MAX;
                            let mut closest_target = None;
                            for (target_entity, target_pos) in nearby_targets.iter() {
                                if *target_entity == entity {
                                    continue;
                                }
                                let Some(target_faction) = factions.get(*target_entity).ok() else {
                                    continue;
                                };
                                if !teams.is_hostile(faction, target_faction) {
                                    continue;
                                }
                                let dx = target_pos.x - tf.translation.x;
                                let dz = target_pos.z - tf.translation.z;
                                let dist = (dx * dx + dz * dz).sqrt();
                                if dist < closest_dist {
                                    closest_dist = dist;
                                    closest_target = Some(*target_entity);
                                }
                            }

                            if let Some(target) = closest_target {
                                // HoldPosition: retarget without changing the standing Hold order.
                                set_auto_target(&mut commands, entity, target, now);
                            } else {
                                commands.entity(entity).remove::<AttackTarget>();
                            }
                        }
                    }
                } else {
                    // Passive stance: no auto-attack
                    commands.entity(entity).remove::<AttackTarget>();
                }
            }

            UnitState::Moving(pos) => {
                // Check if arrived (MoveTarget removed by move_units system on arrival)
                if move_target.is_none() {
                    let was_manual = *source == TaskSource::Manual;
                    *state = UnitState::Idle;
                    task_queue.current = None;
                    // Always clear combat intent/order so resolve_combat_intents
                    // doesn't re-insert MoveTarget after we've arrived.
                    reset_combat_state(&mut commands, entity);
                    if was_manual {
                        // Keep source as Manual for this tick so that
                        // worker_ai_system / decision_priority_system (which
                        // may run later in the same FixedUpdate) skip this
                        // unit.  ManualIdleSince is a deferred command and
                        // won't be visible until the next tick — if we set
                        // source to Auto now, those systems would
                        // immediately auto-reassign the unit.
                        // The Idle handler sets source to Auto on the next
                        // tick once ManualIdleSince is confirmed present.
                        commands
                            .entity(entity)
                            .insert(ManualIdleSince(time.elapsed_secs_f64()));
                    } else {
                        *source = TaskSource::Auto;
                    }
                } else {
                    // Keep MoveTarget synced
                    commands.entity(entity).insert(MoveTarget(pos));
                }
            }

            UnitState::Attacking(target) => {
                // Check target still exists
                if entity_check.get(target).is_err() {
                    reset_combat_state(&mut commands, entity);
                    commands
                        .entity(entity)
                        .remove::<AttackTarget>()
                        .remove::<LeashOrigin>();

                    // Resume previous behavioral task if one exists
                    match task_queue.current.as_ref().map(|t| &t.task) {
                        Some(QueuedTask::AttackMove(dest)) => {
                            let dest = *dest;
                            commands.entity(entity).insert(MoveTarget(dest));
                            apply_manual_attack_move_intent(
                                &mut commands,
                                entity,
                                dest,
                                time.elapsed_secs_f64(),
                            );
                        }
                        Some(QueuedTask::Patrol(patrol_target)) => {
                            let patrol_target = *patrol_target;
                            *state = UnitState::Patrolling {
                                target: patrol_target,
                                origin: tf.translation,
                            };
                            commands.entity(entity).insert(MoveTarget(patrol_target));
                        }
                        _ => {
                            // Scan for nearby enemies before going idle
                            let scan_range = attack_range.map_or(0.0, |r| r.0 * 1.5);
                            let mut found_new_target = false;
                            if scan_range > 0.0 && *stance != UnitStance::Passive {
                                spatial_grid.collect_radius_limited(
                                    tf.translation,
                                    scan_range,
                                    8,
                                    &mut nearby_targets,
                                );
                                let mut best_dist = f32::MAX;
                                let mut best_new_target = None;
                                for (candidate, candidate_pos) in nearby_targets.iter() {
                                    if *candidate == entity {
                                        continue;
                                    }
                                    let Some(target_faction) = factions.get(*candidate).ok() else {
                                        continue;
                                    };
                                    if !teams.is_hostile(faction, target_faction) {
                                        continue;
                                    }
                                    let dist = tf.translation.distance(*candidate_pos);
                                    if dist < best_dist {
                                        best_dist = dist;
                                        best_new_target = Some(*candidate);
                                    }
                                }
                                if let Some(new_target) = best_new_target {
                                    apply_auto_attack_intent(
                                        &mut commands,
                                        entity,
                                        new_target,
                                        tf.translation,
                                        time.elapsed_secs_f64(),
                                    );
                                    *source = TaskSource::Auto;
                                    found_new_target = true;
                                }
                            }
                            if !found_new_target {
                                *state = UnitState::Idle;
                                *source = TaskSource::Auto;
                                task_queue.current = None;
                            }
                        }
                    }
                }
            }

            UnitState::Gathering(node) => {
                // This is now handled by worker_ai_system in resources.rs
                // We just need to ensure MoveTarget points to the node if we're far away
                if let Ok(node_tf) = transforms.get(node) {
                    let dist = tf.translation.distance(node_tf.translation);
                    if dist > gather_range {
                        commands
                            .entity(entity)
                            .insert(MoveTarget(node_tf.translation));
                    }
                } else {
                    // Node gone
                    reset_combat_state(&mut commands, entity);
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                }
            }

            UnitState::ReturningToDeposit {
                depot,
                gather_node: _,
            } => {
                if transforms.get(depot).is_err() {
                    reset_combat_state(&mut commands, entity);
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                }
            }

            UnitState::Depositing { .. }
            | UnitState::WaitingForStorage { .. }
            | UnitState::WaitingForDepot { .. } => {
                // Handled by worker_ai_system
            }

            UnitState::MovingToPlot(pos) => {
                // Worker walking to plot a new building — keep MoveTarget synced.
                // Actual building spawn is handled by pending_build_arrival_system.
                if move_target.is_none() {
                    // Re-insert MoveTarget in case it was consumed
                    commands.entity(entity).insert(MoveTarget(pos));
                }
            }

            UnitState::MovingToBuild(building) => {
                if let Ok((build_state, _, footprint, build_kind)) =
                    construction_sites.get(building)
                {
                    if *build_state != BuildingState::UnderConstruction {
                        commands.entity(entity).remove::<MoveTarget>();
                        reset_combat_state(&mut commands, entity);
                        *state = UnitState::Idle;
                        *source = TaskSource::Auto;
                        task_queue.current = None;
                        continue;
                    }
                    if let Ok(build_tf) = transforms.get(building) {
                        let flat_dist_to_center = Vec2::new(tf.translation.x, tf.translation.z)
                            .distance(Vec2::new(build_tf.translation.x, build_tf.translation.z));
                        // Close enough to building center to start work
                        let is_wall_like = is_wall_like_kind(*build_kind);
                        let work_range = footprint.0
                            + build_range
                            + if is_wall_like {
                                wall_build_range_bonus
                            } else {
                                0.0
                            };
                        if flat_dist_to_center <= work_range {
                            commands.entity(entity).remove::<MoveTarget>();
                            *state = UnitState::Building(building);
                        } else {
                            // Walk toward an offset outside the footprint
                            let stand_dist = footprint.0 + if is_wall_like { 0.75 } else { 1.5 };
                            let angle = (entity.index_u32() as f32 * 2.399) % TAU;
                            let offset =
                                Vec3::new(angle.cos() * stand_dist, 0.0, angle.sin() * stand_dist);
                            let target_pos = build_tf.translation + offset;
                            commands.entity(entity).insert(MoveTarget(target_pos));
                        }
                    }
                } else {
                    commands.entity(entity).remove::<MoveTarget>();
                    reset_combat_state(&mut commands, entity);
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                }
            }

            UnitState::Building(building) => {
                if let Ok((build_state, _, footprint, build_kind)) =
                    construction_sites.get(building)
                {
                    if *build_state != BuildingState::UnderConstruction {
                        commands.entity(entity).remove::<MoveTarget>();
                        reset_combat_state(&mut commands, entity);
                        *state = UnitState::Idle;
                        *source = TaskSource::Auto;
                        task_queue.current = None;
                    } else if let Ok(build_tf) = transforms.get(building) {
                        let flat_dist_to_center = Vec2::new(tf.translation.x, tf.translation.z)
                            .distance(Vec2::new(build_tf.translation.x, build_tf.translation.z));
                        let max_work_range = footprint.0
                            + build_range
                            + if is_wall_like_kind(*build_kind) {
                                wall_build_range_bonus
                            } else {
                                0.0
                            }
                            + 2.0;
                        if flat_dist_to_center > max_work_range {
                            // Pushed too far away — re-path to building
                            *state = UnitState::MovingToBuild(building);
                        } else {
                            // Within work range — no movement needed, just build
                            commands.entity(entity).remove::<MoveTarget>();
                        }
                    }
                } else {
                    commands.entity(entity).remove::<MoveTarget>();
                    reset_combat_state(&mut commands, entity);
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                }
            }

            UnitState::AssignedGathering { building, .. } => {
                // Check building still exists
                if processors.get(building).is_err() {
                    // Building destroyed — unassign worker
                    crate::simulation::buildings::remove_assigned_worker(
                        &mut commands,
                        building,
                        entity,
                    );
                    commands.entity(entity).remove::<BuildingAssignment>();
                    reset_combat_state(&mut commands, entity);
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                }
            }

            UnitState::AttackMoving(pos) => {
                if move_target.is_none() {
                    // Arrived at destination
                    reset_combat_state(&mut commands, entity);
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                } else {
                    // Scan for enemies en route using spatial hash
                    let now = time.elapsed_secs_f64();
                    let can_think = opt_think_timer
                        .as_ref()
                        .map_or(true, |timer| now >= timer.next_think_at);
                    if budget_state.target_rescans_this_frame
                        >= combat_budget.max_target_rescans_per_frame
                    {
                        continue;
                    }
                    if let Some(scan_range) = attack_range.map(|r| r.0 * 2.0).filter(|_| can_think)
                    {
                        let mut closest_dist = f32::MAX;
                        let mut closest_target = None;

                        if let Some(move_target) = move_target {
                            spatial_grid.collect_corridor_limited(
                                tf.translation,
                                move_target.0,
                                scan_range * 0.45,
                                10,
                                &mut nearby_targets,
                                &mut corridor_targets,
                            );
                        } else {
                            spatial_grid.collect_radius_limited(
                                tf.translation,
                                scan_range,
                                10,
                                &mut nearby_targets,
                            );
                        }
                        budget_state.target_rescans_this_frame += 1;
                        for (target_entity, target_pos) in nearby_targets.iter() {
                            if *target_entity == entity {
                                continue;
                            }
                            let Some(target_faction) = factions.get(*target_entity).ok() else {
                                continue;
                            };
                            if !teams.is_hostile(faction, target_faction) {
                                continue;
                            }
                            let dx = target_pos.x - tf.translation.x;
                            let dz = target_pos.z - tf.translation.z;
                            let dist = (dx * dx + dz * dz).sqrt();
                            if dist < closest_dist {
                                closest_dist = dist;
                                closest_target = Some(*target_entity);
                            }
                        }

                        if let Some(target) = closest_target {
                            set_auto_target(&mut commands, entity, target, time.elapsed_secs_f64());
                            commands.entity(entity).remove::<MoveTarget>();
                        }
                    }
                }
            }

            UnitState::Patrolling { target, origin } => {
                if move_target.is_none() {
                    // Arrived at target/origin — swap
                    let new_origin = target;
                    let new_target = origin;
                    commands.entity(entity).insert(MoveTarget(new_target));
                    *state = UnitState::Patrolling {
                        target: new_target,
                        origin: new_origin,
                    };

                    // Also scan for enemies while patrolling using spatial hash
                    if let Some(scan_range) = attack_range.map(|r| r.0 * 2.0) {
                        if budget_state.target_rescans_this_frame
                            >= combat_budget.max_target_rescans_per_frame
                        {
                            continue;
                        }
                        spatial_grid.collect_radius_limited(
                            tf.translation,
                            scan_range,
                            8,
                            &mut nearby_targets,
                        );
                        budget_state.target_rescans_this_frame += 1;
                        for (target_entity, _target_pos) in nearby_targets.iter() {
                            if *target_entity == entity {
                                continue;
                            }
                            let Some(target_faction) = factions.get(*target_entity).ok() else {
                                continue;
                            };
                            if !teams.is_hostile(faction, target_faction) {
                                continue;
                            }
                            apply_manual_attack_intent(
                                &mut commands,
                                entity,
                                *target_entity,
                                time.elapsed_secs_f64(),
                            );
                            commands.entity(entity).remove::<MoveTarget>();
                            break;
                        }
                    }
                }
            }
        }
    }
}

/// Auto-heal system for Priests: scans nearby allies and heals the lowest-HP one.
fn auto_heal_system(
    mut commands: Commands,
    _time: Res<Time<Fixed>>,
    spatial_grid: Res<SpatialHashGrid>,
    teams: Res<TeamConfig>,
    mut nearby_allies: Local<Vec<(Entity, Vec3)>>,
    mut priests: Query<
        (
            Entity,
            &Transform,
            &Faction,
            &mut UnitAbilities,
            &UnitState,
            &TacticalRole,
            Option<&CastingAbility>,
        ),
        With<Unit>,
    >,
    allies: Query<(Entity, &Health, &Transform, &Faction), With<Unit>>,
) {
    for (entity, tf, faction, mut abilities, state, role, casting) in &mut priests {
        if *role != TacticalRole::Healer {
            continue;
        }
        // Only auto-heal when idle, holding, or attacking (not building, gathering, etc.)
        if !matches!(
            state,
            UnitState::Idle | UnitState::HoldPosition | UnitState::Attacking(_)
        ) {
            continue;
        }
        // Don't interrupt an active cast
        if casting.is_some() {
            continue;
        }
        // Check if PriestHeal is available and off cooldown
        if !abilities.abilities.contains(&AbilityId::PriestHeal) {
            continue;
        }
        if !abilities.is_ready(AbilityId::PriestHeal) {
            continue;
        }

        // Scan nearby allies for lowest HP
        let heal_range = 10.0;
        spatial_grid.collect_radius_limited(tf.translation, heal_range, 8, &mut nearby_allies);
        let mut best_target: Option<(Entity, f32)> = None; // (entity, hp_fraction)

        for (nearby_entity, _nearby_pos) in nearby_allies.iter() {
            if *nearby_entity == entity {
                continue;
            }
            let Ok((ally_entity, ally_health, _ally_tf, ally_faction)) = allies.get(*nearby_entity)
            else {
                continue;
            };
            if !teams.is_allied(faction, ally_faction) {
                continue;
            }
            let hp_frac = ally_health.current / ally_health.max;
            if hp_frac >= 0.7 || ally_health.current <= 0.0 {
                continue; // Only heal if below 70% HP
            }
            if best_target.is_none() || hp_frac < best_target.unwrap().1 {
                best_target = Some((ally_entity, hp_frac));
            }
        }

        if let Some((target, _)) = best_target {
            // Trigger the heal ability
            abilities.trigger_cooldown(AbilityId::PriestHeal);
            commands.entity(entity).insert(CastingAbility {
                ability: AbilityId::PriestHeal,
                target_pos: None,
                target_entity: Some(target),
                cast_timer: Timer::from_seconds(0.3, TimerMode::Once),
            });
        }
    }
}
