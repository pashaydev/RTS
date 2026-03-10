use bevy::prelude::*;

use crate::blueprints::EntityKind;
use crate::components::*;

pub struct UnitAiPlugin;

impl Plugin for UnitAiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                cleanup_assigned_workers_system,
                task_queue_advance_system,
                unit_state_executor_system,
            )
                .chain(),
        );
    }
}

/// Removes dead/invalid worker entities from all AssignedWorkers lists,
/// and ejects workers whose building no longer exists.
pub fn cleanup_assigned_workers_system(
    mut commands: Commands,
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
                matches!(state, UnitState::InsideProcessor(_) | UnitState::MovingToProcessor(_))
            } else {
                false
            }
        });
    }
}

/// When a unit is Idle and has queued tasks, pop the next task and set UnitState accordingly.
pub fn task_queue_advance_system(
    mut commands: Commands,
    mut units: Query<
        (Entity, &mut UnitState, &mut TaskSource, &mut TaskQueue, &EntityKind),
        With<Unit>,
    >,
    transforms: Query<&Transform>,
    processors: Query<(&ResourceProcessor, &BuildingState, &Faction), With<Building>>,
    assigned_workers_q: Query<&AssignedWorkers>,
) {
    for (entity, mut state, mut source, mut queue, _kind) in &mut units {
        if *state != UnitState::Idle || queue.queue.is_empty() {
            continue;
        }

        let task = queue.queue.pop_front().unwrap();
        *source = TaskSource::Manual;

        match task {
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
                    commands.entity(entity).insert(MoveTarget(node_tf.translation));
                }
                *state = UnitState::Gathering(node);
            }
            QueuedTask::Build(building) => {
                if let Ok(building_tf) = transforms.get(building) {
                    commands.entity(entity).insert(MoveTarget(building_tf.translation));
                }
                *state = UnitState::MovingToBuild(building);
            }
            QueuedTask::Patrol(pos) => {
                if let Ok(unit_tf) = transforms.get(entity) {
                    *state = UnitState::Patrolling { target: pos, origin: unit_tf.translation };
                    commands.entity(entity).insert(MoveTarget(pos));
                }
            }
            QueuedTask::AssignToProcessor(building) => {
                // Check if building has capacity
                let can_assign = if let Ok((proc, bstate, _)) = processors.get(building) {
                    if *bstate == BuildingState::Complete {
                        let current = assigned_workers_q.get(building)
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
                        commands.entity(entity).insert(MoveTarget(building_tf.translation));
                    }
                    *state = UnitState::MovingToProcessor(building);
                }
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
        (Entity, &Transform, &mut UnitState, &mut TaskSource, &EntityKind, &Faction,
         Option<&MoveTarget>, Option<&AttackRange>),
        With<Unit>,
    >,
    transforms: Query<&Transform, Without<Unit>>,
    _nodes: Query<&ResourceNode>,
    construction_sites: Query<(&BuildingState, &Faction), (With<Building>, With<ConstructionProgress>)>,
    processors: Query<(&ResourceProcessor, &BuildingState, &Faction), With<Building>>,
    mut assigned_workers_q: Query<&mut AssignedWorkers>,
    potential_targets: Query<(Entity, &Transform, &Faction), Or<(With<Mob>, With<Unit>)>>,
    buildings_with_faction: Query<(Entity, &Transform, &Faction), (With<Building>, Without<Unit>)>,
) {
    let gather_range = 3.0;
    let build_range = 4.0;
    let processor_range = 3.0;

    for (entity, tf, mut state, mut source, _kind, faction, move_target, attack_range) in &mut units {
        match *state {
            UnitState::Idle | UnitState::HoldPosition => {
                // Remove stale targets
                commands.entity(entity).remove::<MoveTarget>().remove::<AttackTarget>();
            }

            UnitState::Moving(pos) => {
                // Check if arrived (MoveTarget removed by move_units system on arrival)
                if move_target.is_none() {
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
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
                    commands.entity(entity).remove::<AttackTarget>();
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
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
                        commands.entity(entity).insert(MoveTarget(node_tf.translation));
                    }
                } else {
                    // Node gone
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                }
            }

            UnitState::ReturningToDeposit { depot, gather_node: _ } => {
                if transforms.get(depot).is_err() {
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
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
                        continue;
                    }
                    if let Ok(build_tf) = transforms.get(building) {
                        let dist = tf.translation.distance(build_tf.translation);
                        if dist <= build_range {
                            commands.entity(entity).remove::<MoveTarget>();
                            *state = UnitState::Building(building);
                        } else {
                            commands.entity(entity).insert(MoveTarget(build_tf.translation));
                        }
                    }
                } else {
                    commands.entity(entity).remove::<MoveTarget>();
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                }
            }

            UnitState::Building(building) => {
                if let Ok((build_state, _)) = construction_sites.get(building) {
                    if *build_state != BuildingState::UnderConstruction {
                        commands.entity(entity).remove::<MoveTarget>();
                        *state = UnitState::Idle;
                        *source = TaskSource::Auto;
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
                                let current = assigned_workers_q.get(building)
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
                            commands.entity(entity)
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
                        }
                    } else {
                        commands.entity(entity).insert(MoveTarget(build_tf.translation));
                    }
                } else {
                    commands.entity(entity).remove::<MoveTarget>();
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                }
            }

            UnitState::InsideProcessor(building) => {
                // Check building still exists
                if processors.get(building).is_err() {
                    // Building destroyed — eject worker
                    commands.entity(entity)
                        .insert(Visibility::Inherited)
                        .remove::<ProcessorWorkerState>();
                    // Remove from AssignedWorkers (building gone, so this is a no-op but safe)
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                }
            }

            UnitState::AttackMoving(_pos) => {
                if move_target.is_none() {
                    // Arrived at destination
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                } else {
                    // Scan for enemies en route
                    if let Some(scan_range) = attack_range.map(|r| r.0 * 2.0) {
                        let mut closest_dist = f32::MAX;
                        let mut closest_target = None;

                        for (target_entity, target_tf, target_faction) in &potential_targets {
                            if target_entity == entity { continue; }
                            if !teams.is_hostile(faction, target_faction) { continue; }
                            let dist = tf.translation.distance(target_tf.translation);
                            if dist < scan_range && dist < closest_dist {
                                closest_dist = dist;
                                closest_target = Some(target_entity);
                            }
                        }

                        for (target_entity, target_tf, target_faction) in &buildings_with_faction {
                            if !teams.is_hostile(faction, target_faction) { continue; }
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
                    *state = UnitState::Patrolling { target: new_target, origin: new_origin };

                    // Also scan for enemies while patrolling
                    if let Some(scan_range) = attack_range.map(|r| r.0 * 2.0) {
                        for (target_entity, target_tf, target_faction) in &potential_targets {
                            if target_entity == entity { continue; }
                            if !teams.is_hostile(faction, target_faction) { continue; }
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
