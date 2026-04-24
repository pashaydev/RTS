//! Worker FSM entry / exit helpers and type anchors.
//!
//! The worker state machine is anchored by the [`AssignedPhase`] enum in
//! `types/economy.rs`. Transitions between phases today still live in
//! [`super::processing::processor_worker_visual_system`] (large match on the
//! current phase). The long-term intent for this file is to host one
//! trait-object handler per phase and a dispatch loop, so the phase
//! transitions have a single place to read and extend.
//!
//! For now this file owns only the canonical enter / exit helpers —
//! `assign_worker_to_processor` and `unassign_worker_from_processor` — which
//! are the single choke points for starting and ending a worker's assigned
//! lifecycle. Other systems (auto-assign, reconcile, enforce-limit, user UI)
//! must route through these two functions so the FSM has well-defined edges.

use bevy::prelude::*;

use crate::types::*;

/// Start a worker's assigned-gathering lifecycle at a processor building.
///
/// Inserts `UnitState::AssignedGathering { phase: SeekingNode }`, the
/// `BuildingAssignment` marker, and an initial `MoveTarget` pointing at the
/// building. Clears any stale selection/hover/attack target. The building's
/// `AssignedWorkers` list is updated via
/// `crate::simulation::buildings::add_assigned_worker`.
pub fn assign_worker_to_processor(
    commands: &mut Commands,
    worker: Entity,
    building: Entity,
    building_pos: Vec3,
    source: TaskSource,
) {
    crate::simulation::buildings::add_assigned_worker(commands, building, worker);
    commands
        .entity(worker)
        .insert(UnitState::AssignedGathering {
            building,
            phase: AssignedPhase::SeekingNode,
        })
        .insert(source)
        .insert(BuildingAssignment(building))
        .insert(MoveTarget(building_pos))
        .remove::<Selected>()
        .remove::<Hovered>();
}

/// End a worker's assigned-gathering lifecycle.
///
/// Resets the worker to `UnitState::Idle`, clears the assignment marker and
/// any pending move target, and updates the building's `AssignedWorkers`
/// list (if a building was provided).
pub fn unassign_worker_from_processor(
    commands: &mut Commands,
    worker: Entity,
    building: Option<Entity>,
) {
    if let Some(building) = building {
        crate::simulation::buildings::remove_assigned_worker(commands, building, worker);
    }
    commands
        .entity(worker)
        .insert(UnitState::Idle)
        .insert(TaskSource::Auto)
        .remove::<BuildingAssignment>()
        .remove::<MoveTarget>();
}
