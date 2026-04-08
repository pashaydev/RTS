use bevy::prelude::*;
use std::f32::consts::TAU;

use crate::blueprints::{EntityKind, IsRanged};
use crate::simulation::buildings::is_wall_like_kind;
use crate::simulation::combat::{
    apply_auto_attack_intent, apply_auto_move_intent, apply_manual_attack_intent,
    apply_manual_attack_move_intent, apply_manual_hold_intent, apply_manual_move_intent,
    clear_combat_intent, reset_combat_state, set_intent_target_lock, target_score,
    CombatBudgetState, TargetScoreInput,
};
use crate::types::*;
use crate::infrastructure::multiplayer::NetRole;
use crate::world::spatial::SpatialHashGrid;

pub struct UnitAiPlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum UnitAiSet {
    Cleanup,
    Hotspots,
    Decision,
    TaskAdvance,
    Execute,
    Leash,
    Heal,
}

impl Plugin for UnitAiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DecisionTimer>()
            .init_resource::<CombatHotspots>()
            .configure_sets(
                FixedUpdate,
                (
                    UnitAiSet::Cleanup,
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
                cleanup_assigned_workers_system
                    .in_set(UnitAiSet::Cleanup)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                update_combat_hotspots
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

/// Removes dead/invalid worker entities from all AssignedWorkers lists,
/// and ejects workers whose building no longer exists.
pub fn cleanup_assigned_workers_system(
    mut commands: Commands,
    mut buildings: Query<(Entity, &mut AssignedWorkers), With<Building>>,
    workers: Query<(Entity, &UnitState, Option<&BuildingAssignment>), With<Unit>>,
) {
    for (building_entity, mut aw) in &mut buildings {
        // Check if retain would actually remove anything before mutating,
        // to avoid triggering Changed<AssignedWorkers> every frame.
        let has_invalid = aw.workers.iter().any(|&worker| {
            !matches!(
                workers.get(worker),
                Ok((
                    _,
                    UnitState::AssignedGathering { building, .. },
                    _
                )) if *building == building_entity
            )
        });
        if has_invalid {
            aw.workers.retain(|&worker| {
                matches!(
                    workers.get(worker),
                    Ok((
                        _,
                        UnitState::AssignedGathering { building, .. },
                        _
                    )) if *building == building_entity
                )
            });
        }
    }

    // Canonicalize the worker-side assignment marker from the authoritative UnitState.
    for (worker, state, assignment) in &workers {
        match *state {
            UnitState::AssignedGathering { building, .. } => {
                if assignment.map(|a| a.0) != Some(building) {
                    commands.entity(worker).insert(BuildingAssignment(building));
                }
            }
            _ => {
                if assignment.is_some() {
                    commands.entity(worker).remove::<BuildingAssignment>();
                }
            }
        }
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
const DECISION_AMORTIZE_FRAMES: usize = 8;

fn stance_scan_multiplier(stance: UnitStance, tuning: &CombatTuning) -> f32 {
    match stance {
        UnitStance::Passive => tuning.passive_scan_multiplier,
        UnitStance::Defensive => tuning.defensive_scan_multiplier,
        UnitStance::Aggressive => tuning.aggressive_scan_multiplier,
    }
}

fn stance_leash_distance(stance: UnitStance, tuning: &CombatTuning) -> f32 {
    match stance {
        UnitStance::Passive => tuning.passive_leash_distance,
        UnitStance::Defensive => tuning.defensive_leash_distance,
        UnitStance::Aggressive => tuning.aggressive_leash_distance,
    }
}

/// Collects positions of units currently in combat for ally-assist detection.
fn update_combat_hotspots(
    mut hotspots: ResMut<CombatHotspots>,
    mut frame_counter: Local<u32>,
    units: Query<(&Transform, &UnitState, &Faction), With<Unit>>,
) {
    *frame_counter = frame_counter.wrapping_add(1);
    // Only update every 6 frames to save cost
    if *frame_counter % 6 != 0 {
        return;
    }
    hotspots.spots.clear();
    for (tf, state, faction) in &units {
        if let UnitState::Attacking(target) = *state {
            hotspots.spots.push((tf.translation, target, *faction));
        }
    }
    hotspots.spots.truncate(128);
}

fn decision_priority_system(
    mut commands: Commands,
    time: Res<Time>,
    mut decision_timer: ResMut<DecisionTimer>,
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
            Option<&TargetingProfile>,
            Option<&DamageType>,
            Option<&CombatIntent>,
            Option<&CombatTargetLock>,
            Option<&mut CombatThinkTimer>,
            (Option<&TacticalRole>, Option<&ManualIdleSince>),
        ),
        With<Unit>,
    >,
    factions: Query<&Faction>,
    net_state: (Res<NetRole>, Res<ActivePlayer>),
    building_check: Query<(), With<Building>>,
    deposit_points: Query<(Entity, &Transform, &Faction), (With<DepositPoint>, Without<Unit>)>,
    target_data: Query<(
        &Health,
        &ArmorType,
        Option<&ThreatValue>,
        Option<&ReservedIncomingDamage>,
        Option<&IsRanged>,
        Option<&TacticalRole>,
    )>,
    mut batch_offset: Local<usize>,
    mut batch_total: Local<usize>,
) {
    let (combat_budget, mut budget_state) = budgeting;
    let (net_role, active_player) = net_state;
    decision_timer.timer.tick(time.delta());
    let mut nearby_targets = Vec::new();

    // On timer tick: start a new amortization cycle.
    if decision_timer.timer.just_finished() {
        *batch_offset = 0;
        *batch_total = units.iter().len();
    }

    // Nothing to process if cycle is complete.
    if *batch_offset >= *batch_total || *batch_total == 0 {
        return;
    }

    // Process one chunk of units this frame.
    let remaining = *batch_total - *batch_offset;
    let chunk_size = (remaining + DECISION_AMORTIZE_FRAMES - 1) / DECISION_AMORTIZE_FRAMES;
    let chunk_start = *batch_offset;
    let chunk_end = (chunk_start + chunk_size).min(*batch_total);
    *batch_offset = chunk_end;

    for (
        idx,
        (
            entity,
            tf,
            mut state,
            mut source,
            stance,
            faction,
            health,
            attack_range,
            task_queue,
            opt_targeting_profile,
            opt_damage_type,
            combat_intent,
            target_lock,
            opt_think_timer,
            (opt_tactical_role, manual_idle_since),
        ),
    ) in units.iter_mut().enumerate()
    {
        if idx < chunk_start || idx >= chunk_end {
            continue;
        }
        let now = time.elapsed_secs_f64();
        if opt_think_timer.is_some_and(|timer| now < timer.next_think_at) {
            continue;
        }
        // Client: only process local player's units; remote units are driven by host state sync
        if *net_role == NetRole::Client && *faction != active_player.0 {
            continue;
        }

        // Skip units with manual orders, queued tasks, or in manual-idle grace period
        if *source == TaskSource::Manual
            || task_queue.current.is_some()
            || !task_queue.queue.is_empty()
            || manual_idle_since.is_some_and(|s| now - s.0 < 5.0)
        {
            continue;
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
            | UnitState::HoldPosition
            | UnitState::Patrolling { .. }
            | UnitState::AttackMoving(_) => continue,
            _ => {}
        }

        // ── Priority 2: Survival retreat (hp < 25%, not Aggressive) ──
        if *stance != UnitStance::Aggressive
            && health.current > 0.0
            && health.current / health.max < 0.25
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
            let scan_range = attack_r.0 * stance_scan_multiplier(*stance, &combat_tuning);
            if scan_range <= 0.0 {
                continue;
            }
            if let Some(lock) = target_lock {
                let lock_still_valid = now <= lock.locked_until
                    && factions
                        .get(lock.target)
                        .ok()
                        .is_some_and(|target_faction| teams.is_hostile(faction, target_faction));
                if lock_still_valid {
                    let current_matches_lock = matches!(
                        combat_intent,
                        Some(CombatIntent::Attack(target, IntentSource::Auto)) if *target == lock.target
                    );
                    if !current_matches_lock
                        || !matches!(*state, UnitState::Attacking(target) if target == lock.target)
                    {
                        apply_auto_attack_intent(
                            &mut commands,
                            entity,
                            lock.target,
                            tf.translation,
                            now,
                        );
                        *state = UnitState::Attacking(lock.target);
                        *source = TaskSource::Auto;
                    }
                    continue;
                }
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
                    let Ok((t_health, t_armor, t_threat, t_reserved, t_is_ranged, t_role)) =
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
                        let target_is_ranged = t_is_ranged.is_some()
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
                    UnitStance::Defensive => 18.0,
                    UnitStance::Aggressive => 30.0,
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
                *state = UnitState::Attacking(target);
                *source = TaskSource::Auto;
            } else if matches!(
                combat_intent,
                Some(CombatIntent::Attack(_, IntentSource::Auto))
            ) {
                reset_combat_state(&mut commands, entity);
            }
            commands.entity(entity).insert(CombatThinkTimer {
                next_think_at: now
                    + 0.16
                    + ((entity.to_bits() % DECISION_AMORTIZE_FRAMES as u64) as f64 * 0.006),
                interval_secs: 0.16,
            });
        }
    }
}

/// Leash return system — Defensive units that chased too far return to their origin.
fn leash_return_system(
    mut commands: Commands,
    combat_tuning: Res<CombatTuning>,
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
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
    for (entity, tf, mut state, mut source, stance, leash_origin, faction) in &mut units {
        // Client: only process local player's units; remote units driven by host
        if *net_role == NetRole::Client && *faction != active_player.0 {
            continue;
        }
        // Only apply leash to auto-sourced attacks
        if *source != TaskSource::Auto {
            continue;
        }

        if !matches!(*state, UnitState::Attacking(_)) {
            // No longer attacking — clean up leash
            commands.entity(entity).remove::<LeashOrigin>();
            continue;
        }

        let leash_dist = stance_leash_distance(*stance, &combat_tuning);
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

/// When a unit is Idle and has queued tasks, pop the next task and set UnitState accordingly.
pub fn task_queue_advance_system(
    mut commands: Commands,
    time: Res<Time>,
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
    processors: Query<(&ResourceProcessor, &BuildingState, &Faction), With<Building>>,
    mut assigned_workers_q: Query<&mut AssignedWorkers>,
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
) {
    for (entity, mut state, mut source, mut queue, _kind, faction) in &mut units {
        // Client: only process local player's units
        if *net_role == NetRole::Client && *faction != active_player.0 {
            continue;
        }

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
                *state = UnitState::AttackMoving(pos);
                apply_manual_attack_move_intent(
                    &mut commands,
                    entity,
                    pos,
                    time.elapsed_secs_f64(),
                );
                commands.entity(entity).insert(MoveTarget(pos));
            }
            QueuedTask::Attack(target) => {
                *state = UnitState::Attacking(target);
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
            QueuedTask::AssignToProcessor(building) => {
                clear_combat_intent(&mut commands, entity, time.elapsed_secs_f64());
                // Check if building has capacity
                let can_assign = if let Ok((proc, bstate, _)) = processors.get(building) {
                    if *bstate == BuildingState::Complete {
                        let current = assigned_workers_q
                            .get(building)
                            .map(|aw| aw.workers.len())
                            .unwrap_or(0);
                        current < proc.max_workers as usize
                    } else {
                        false
                    }
                } else {
                    false
                };

                if can_assign {
                    let building_pos = transforms
                        .get(building)
                        .map(|t| t.translation)
                        .unwrap_or(Vec3::ZERO);
                    crate::simulation::resources::assign_worker_to_processor(
                        &mut commands,
                        entity,
                        building,
                        building_pos,
                        TaskSource::Manual,
                    );
                    // Add to building's AssignedWorkers
                    if let Ok(mut aw) = assigned_workers_q.get_mut(building) {
                        if !aw.workers.contains(&entity) {
                            aw.workers.push(entity);
                        }
                    }
                }
            }
            QueuedTask::HoldPosition => {
                apply_manual_hold_intent(&mut commands, entity, time.elapsed_secs_f64());
                commands
                    .entity(entity)
                    .remove::<MoveTarget>()
                    .remove::<AttackTarget>();
                *state = UnitState::HoldPosition;
            }
        }
    }
}

/// Translates UnitState into low-level component management.
/// Handles arrival detection, state transitions, and MoveTarget/AttackTarget sync.
pub fn unit_state_executor_system(
    mut commands: Commands,
    time: Res<Time>,
    teams: Res<TeamConfig>,
    spatial_grid: Res<SpatialHashGrid>,
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
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
        // Client: only process local player's units; remote units are driven by host state sync
        if *net_role == NetRole::Client && *faction != active_player.0 {
            continue;
        }

        match *state {
            UnitState::Idle => {
                // Remove stale targets
                commands
                    .entity(entity)
                    .remove::<MoveTarget>()
                    .remove::<AttackTarget>()
                    .remove::<ChaseTimer>();
                if matches!(*source, TaskSource::Auto) {
                    reset_combat_state(&mut commands, entity);
                }
            }

            UnitState::HoldPosition => {
                // Never move when holding position
                commands
                    .entity(entity)
                    .remove::<MoveTarget>()
                    .remove::<ChaseTimer>();

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
                                let Some(target_faction) =
                                    factions.get(*target_entity).ok()
                                else {
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
                                // Set target lock so resolve_combat_intents picks it up
                                // CombatIntent::Hold is already set — it will fire without moving
                                set_intent_target_lock(
                                    &mut commands,
                                    entity,
                                    target,
                                    IntentSource::Auto,
                                    now,
                                );
                            } else {
                                // No enemies in range — clear attack target
                                commands.entity(entity).remove::<AttackTarget>();
                            }

                            commands.entity(entity).insert(CombatThinkTimer {
                                next_think_at: now + 0.2,
                                interval_secs: 0.2,
                            });
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
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                    if was_manual {
                        // Grace period: prevent AI from immediately reassigning
                        commands
                            .entity(entity)
                            .insert(ManualIdleSince(time.elapsed_secs_f64()));
                    } else {
                        reset_combat_state(&mut commands, entity);
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
                        .remove::<LeashOrigin>()
                        .remove::<ChaseTimer>();

                    // Resume previous behavioral task if one exists
                    match task_queue.current.as_ref().map(|t| &t.task) {
                        Some(QueuedTask::AttackMove(dest)) => {
                            let dest = *dest;
                            *state = UnitState::AttackMoving(dest);
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
                            *state = UnitState::Idle;
                            *source = TaskSource::Auto;
                            task_queue.current = None;
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

            UnitState::Depositing { .. } | UnitState::WaitingForStorage { .. } => {
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
                    commands.entity(entity).remove::<BuildingAssignment>();
                    reset_combat_state(&mut commands, entity);
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                }
            }

            UnitState::AttackMoving(_pos) => {
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
                            set_intent_target_lock(
                                &mut commands,
                                entity,
                                target,
                                IntentSource::Manual,
                                time.elapsed_secs_f64(),
                            );
                            commands.entity(entity).remove::<MoveTarget>();
                        }
                        commands.entity(entity).insert(CombatThinkTimer {
                            next_think_at: now + 0.18,
                            interval_secs: 0.18,
                        });
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
    _time: Res<Time>,
    spatial_grid: Res<SpatialHashGrid>,
    teams: Res<TeamConfig>,
    net_role: Res<NetRole>,
    active_player: Res<ActivePlayer>,
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
        // Client: only process local player's units
        if *net_role == NetRole::Client && *faction != active_player.0 {
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
            let Ok((ally_entity, ally_health, _ally_tf, ally_faction)) =
                allies.get(*nearby_entity)
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
