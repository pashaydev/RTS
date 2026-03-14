use bevy::prelude::*;

use crate::blueprints::EntityKind;
use crate::components::*;

pub struct UnitAiPlugin;

impl Plugin for UnitAiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DecisionTimer>().add_systems(
            Update,
            (
                cleanup_assigned_workers_system,
                decision_priority_system,
                task_queue_advance_system,
                unit_state_executor_system,
                leash_return_system,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Removes dead/invalid worker entities from all AssignedWorkers lists,
/// and ejects workers whose building no longer exists.
pub fn cleanup_assigned_workers_system(
    _commands: Commands,
    mut buildings: Query<&mut AssignedWorkers, With<Building>>,
    workers: Query<Entity, With<Unit>>,
    unit_states: Query<&UnitState, With<Unit>>,
) {
    for mut aw in &mut buildings {
        aw.workers.retain(|&worker| {
            // Remove if worker entity no longer exists
            if workers.get(worker).is_err() {
                return false;
            }
            // Remove if worker is no longer InsideProcessor (was unassigned externally)
            if let Ok(state) = unit_states.get(worker) {
                matches!(
                    state,
                    UnitState::InsideProcessor(_) | UnitState::MovingToProcessor(_)
                )
            } else {
                false
            }
        });
    }
}

/// Decision priority system — runs every 0.2s and evaluates what idle/auto units should do.
/// Priority order:
/// 1. Manual task → skip (handled by task_queue_advance)
/// 2. Survival retreat (hp < 25%, non-Aggressive stance)
/// 3. Threat response by stance (Defensive/Aggressive auto-engage)
/// 4. Auto-role behavior (handled by worker_ai_system for Economy)
/// 5. Idle
fn decision_priority_system(
    mut commands: Commands,
    time: Res<Time>,
    mut decision_timer: ResMut<DecisionTimer>,
    teams: Res<TeamConfig>,
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
        ),
        With<Unit>,
    >,
    potential_targets: Query<(Entity, &Transform, &Faction), Or<(With<Mob>, With<Unit>)>>,
    buildings_with_faction: Query<(Entity, &Transform, &Faction), (With<Building>, Without<Unit>)>,
    deposit_points: Query<(Entity, &Transform, &Faction), (With<DepositPoint>, Without<Unit>)>,
) {
    decision_timer.timer.tick(time.delta());
    if !decision_timer.timer.just_finished() {
        return;
    }

    for (entity, tf, mut state, mut source, stance, faction, health, attack_range, task_queue) in
        &mut units
    {
        // Skip units with manual orders or queued tasks
        if *source == TaskSource::Manual || task_queue.current.is_some() || !task_queue.queue.is_empty() {
            continue;
        }

        // Skip units that are busy with non-interruptible states
        match *state {
            UnitState::Building(_)
            | UnitState::MovingToBuild(_)
            | UnitState::MovingToPlot(_)
            | UnitState::InsideProcessor(_)
            | UnitState::MovingToProcessor(_)
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
            let scan_range = attack_r.0 * stance.scan_multiplier();
            if scan_range <= 0.0 {
                continue;
            }

            let mut closest_dist = f32::MAX;
            let mut closest_target = None;

            for (target_entity, target_tf, target_faction) in &potential_targets {
                if target_entity == entity {
                    continue;
                }
                if !teams.is_hostile(faction, target_faction) {
                    continue;
                }
                let dist = tf.translation.distance(target_tf.translation);
                if dist < scan_range && dist < closest_dist {
                    closest_dist = dist;
                    closest_target = Some(target_entity);
                }
            }

            // Aggressive stance also scans hostile buildings
            if *stance == UnitStance::Aggressive {
                for (target_entity, target_tf, target_faction) in &buildings_with_faction {
                    if !teams.is_hostile(faction, target_faction) {
                        continue;
                    }
                    let dist = tf.translation.distance(target_tf.translation);
                    if dist < scan_range && dist < closest_dist {
                        closest_dist = dist;
                        closest_target = Some(target_entity);
                    }
                }
            }

            if let Some(target) = closest_target {
                // Record leash origin before engaging
                commands
                    .entity(entity)
                    .insert(LeashOrigin(tf.translation))
                    .insert(AttackTarget(target));
                *state = UnitState::Attacking(target);
                *source = TaskSource::Auto;
            }
        }
    }
}

/// Leash return system — Defensive units that chased too far return to their origin.
fn leash_return_system(
    mut commands: Commands,
    mut units: Query<
        (
            Entity,
            &Transform,
            &mut UnitState,
            &mut TaskSource,
            &UnitStance,
            &LeashOrigin,
        ),
        With<Unit>,
    >,
) {
    for (entity, tf, mut state, mut source, stance, leash_origin) in &mut units {
        // Only apply leash to auto-sourced attacks
        if *source != TaskSource::Auto {
            continue;
        }

        if !matches!(*state, UnitState::Attacking(_)) {
            // No longer attacking — clean up leash
            commands.entity(entity).remove::<LeashOrigin>();
            continue;
        }

        let leash_dist = stance.leash_distance();
        if leash_dist <= 0.0 {
            commands.entity(entity).remove::<LeashOrigin>();
            continue;
        }

        let dist_from_origin = tf.translation.distance(leash_origin.0);
        if dist_from_origin > leash_dist {
            // Exceeded leash — return to origin
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
    mut units: Query<
        (
            Entity,
            &mut UnitState,
            &mut TaskSource,
            &mut TaskQueue,
            &EntityKind,
        ),
        With<Unit>,
    >,
    transforms: Query<&Transform>,
    processors: Query<(&ResourceProcessor, &BuildingState, &Faction), With<Building>>,
    assigned_workers_q: Query<&AssignedWorkers>,
) {
    for (entity, mut state, mut source, mut queue, _kind) in &mut units {
        if *state != UnitState::Idle || queue.current.is_some() || queue.queue.is_empty() {
            continue;
        }

        let task = queue.queue.pop_front().unwrap();
        queue.current = Some(task.clone());
        *source = TaskSource::Manual;

        match task.task {
            QueuedTask::Move(pos) => {
                *state = UnitState::Moving(pos);
                commands.entity(entity).insert(MoveTarget(pos));
            }
            QueuedTask::AttackMove(pos) => {
                *state = UnitState::AttackMoving(pos);
                commands.entity(entity).insert(MoveTarget(pos));
            }
            QueuedTask::Attack(target) => {
                *state = UnitState::Attacking(target);
                commands.entity(entity).insert(AttackTarget(target));
            }
            QueuedTask::Gather(node) => {
                if let Ok(node_tf) = transforms.get(node) {
                    commands
                        .entity(entity)
                        .insert(MoveTarget(node_tf.translation));
                }
                *state = UnitState::Gathering(node);
            }
            QueuedTask::Build(building) => {
                if let Ok(building_tf) = transforms.get(building) {
                    commands
                        .entity(entity)
                        .insert(MoveTarget(building_tf.translation));
                }
                *state = UnitState::MovingToBuild(building);
            }
            QueuedTask::Patrol(pos) => {
                if let Ok(unit_tf) = transforms.get(entity) {
                    *state = UnitState::Patrolling {
                        target: pos,
                        origin: unit_tf.translation,
                    };
                    commands.entity(entity).insert(MoveTarget(pos));
                }
            }
            QueuedTask::AssignToProcessor(building) => {
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
                    if let Ok(building_tf) = transforms.get(building) {
                        commands
                            .entity(entity)
                            .insert(MoveTarget(building_tf.translation));
                    }
                    *state = UnitState::MovingToProcessor(building);
                }
            }
            QueuedTask::HoldPosition => {
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
    _time: Res<Time>,
    teams: Res<TeamConfig>,
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
        ),
        With<Unit>,
    >,
    transforms: Query<&Transform, Without<Unit>>,
    _nodes: Query<&ResourceNode>,
    construction_sites: Query<
        (&BuildingState, &Faction),
        (With<Building>, With<ConstructionProgress>),
    >,
    processors: Query<(&ResourceProcessor, &BuildingState, &Faction), With<Building>>,
    mut assigned_workers_q: Query<&mut AssignedWorkers>,
    potential_targets: Query<(Entity, &Transform, &Faction), Or<(With<Mob>, With<Unit>)>>,
    buildings_with_faction: Query<(Entity, &Transform, &Faction), (With<Building>, Without<Unit>)>,
) {
    let gather_range = 3.0;
    let build_range = 4.0;
    let processor_range = 3.0;

    for (entity, tf, mut state, mut source, mut task_queue, _kind, faction, move_target, attack_range) in &mut units
    {
        match *state {
            UnitState::Idle | UnitState::HoldPosition => {
                // Remove stale targets
                commands
                    .entity(entity)
                    .remove::<MoveTarget>()
                    .remove::<AttackTarget>();
            }

            UnitState::Moving(pos) => {
                // Check if arrived (MoveTarget removed by move_units system on arrival)
                if move_target.is_none() {
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                } else {
                    // Keep MoveTarget synced
                    commands.entity(entity).insert(MoveTarget(pos));
                }
            }

            UnitState::Attacking(target) => {
                // Check target still exists
                if transforms.get(target).is_err()
                    && potential_targets.get(target).is_err()
                    && buildings_with_faction.get(target).is_err()
                {
                    commands
                        .entity(entity)
                        .remove::<AttackTarget>()
                        .remove::<LeashOrigin>();
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                } else {
                    commands.entity(entity).insert(AttackTarget(target));
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
                if let Ok((build_state, _)) = construction_sites.get(building) {
                    if *build_state != BuildingState::UnderConstruction {
                        commands.entity(entity).remove::<MoveTarget>();
                        *state = UnitState::Idle;
                        *source = TaskSource::Auto;
                        task_queue.current = None;
                        continue;
                    }
                    if let Ok(build_tf) = transforms.get(building) {
                        let dist = tf.translation.distance(build_tf.translation);
                        if dist <= build_range {
                            commands.entity(entity).remove::<MoveTarget>();
                            *state = UnitState::Building(building);
                        } else {
                            commands
                                .entity(entity)
                                .insert(MoveTarget(build_tf.translation));
                        }
                    }
                } else {
                    commands.entity(entity).remove::<MoveTarget>();
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                }
            }

            UnitState::Building(building) => {
                if let Ok((build_state, _)) = construction_sites.get(building) {
                    if *build_state != BuildingState::UnderConstruction {
                        commands.entity(entity).remove::<MoveTarget>();
                        *state = UnitState::Idle;
                        *source = TaskSource::Auto;
                        task_queue.current = None;
                    } else if let Ok(build_tf) = transforms.get(building) {
                        let dist = tf.translation.distance(build_tf.translation);
                        if dist > build_range {
                            *state = UnitState::MovingToBuild(building);
                        } else {
                            commands.entity(entity).remove::<MoveTarget>();
                        }
                    }
                } else {
                    commands.entity(entity).remove::<MoveTarget>();
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                }
            }

            UnitState::MovingToProcessor(building) => {
                if let Ok(build_tf) = transforms.get(building) {
                    let dist = tf.translation.distance(build_tf.translation);
                    if dist <= processor_range {
                        // Arrived — absorb into building
                        commands.entity(entity).remove::<MoveTarget>();

                        // Check building still valid and has capacity
                        let can_enter = if let Ok((proc, bstate, _)) = processors.get(building) {
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

                        if can_enter {
                            *state = UnitState::InsideProcessor(building);
                            commands
                                .entity(entity)
                                .insert(Visibility::Hidden)
                                .insert(ProcessorWorkerState::default());
                            // Add to building's AssignedWorkers
                            if let Ok(mut aw) = assigned_workers_q.get_mut(building) {
                                if !aw.workers.contains(&entity) {
                                    aw.workers.push(entity);
                                }
                            }
                        } else {
                            *state = UnitState::Idle;
                            *source = TaskSource::Auto;
                            task_queue.current = None;
                        }
                    } else {
                        commands
                            .entity(entity)
                            .insert(MoveTarget(build_tf.translation));
                    }
                } else {
                    commands.entity(entity).remove::<MoveTarget>();
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                }
            }

            UnitState::InsideProcessor(building) => {
                // Check building still exists
                if processors.get(building).is_err() {
                    // Building destroyed — eject worker
                    commands
                        .entity(entity)
                        .insert(Visibility::Inherited)
                        .remove::<ProcessorWorkerState>();
                    // Remove from AssignedWorkers (building gone, so this is a no-op but safe)
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                }
            }

            UnitState::AttackMoving(_pos) => {
                if move_target.is_none() {
                    // Arrived at destination
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                } else {
                    // Scan for enemies en route
                    if let Some(scan_range) = attack_range.map(|r| r.0 * 2.0) {
                        let mut closest_dist = f32::MAX;
                        let mut closest_target = None;

                        for (target_entity, target_tf, target_faction) in &potential_targets {
                            if target_entity == entity {
                                continue;
                            }
                            if !teams.is_hostile(faction, target_faction) {
                                continue;
                            }
                            let dist = tf.translation.distance(target_tf.translation);
                            if dist < scan_range && dist < closest_dist {
                                closest_dist = dist;
                                closest_target = Some(target_entity);
                            }
                        }

                        for (target_entity, target_tf, target_faction) in &buildings_with_faction {
                            if !teams.is_hostile(faction, target_faction) {
                                continue;
                            }
                            let dist = tf.translation.distance(target_tf.translation);
                            if dist < scan_range && dist < closest_dist {
                                closest_dist = dist;
                                closest_target = Some(target_entity);
                            }
                        }

                        if let Some(target) = closest_target {
                            commands.entity(entity).remove::<MoveTarget>();
                            commands.entity(entity).insert(AttackTarget(target));
                            *state = UnitState::Attacking(target);
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

                    // Also scan for enemies while patrolling
                    if let Some(scan_range) = attack_range.map(|r| r.0 * 2.0) {
                        for (target_entity, target_tf, target_faction) in &potential_targets {
                            if target_entity == entity {
                                continue;
                            }
                            if !teams.is_hostile(faction, target_faction) {
                                continue;
                            }
                            let dist = tf.translation.distance(target_tf.translation);
                            if dist < scan_range {
                                commands.entity(entity).remove::<MoveTarget>();
                                commands.entity(entity).insert(AttackTarget(target_entity));
                                *state = UnitState::Attacking(target_entity);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}
