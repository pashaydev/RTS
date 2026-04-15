use bevy::prelude::*;
use bevy::time::Fixed;
use std::collections::HashMap;

use crate::blueprints::{BlueprintRegistry, EntityKind, LevelBonus};
use crate::types::*;

use super::workers::{building_worker_interaction_target, spawn_deposit_vfx, spawn_resource_popup};

/// Resource processing buildings auto-harvest nearby nodes on a timer and deposit into player resources.
pub(super) fn resource_processor_system(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut all_resources: ResMut<AllPlayerResources>,
    mut processors: Query<
        (
            Entity,
            &Transform,
            &mut ResourceProcessor,
            &BuildingState,
            &Faction,
            Option<&mut StorageInventory>,
            Option<&AssignedWorkers>,
            Option<&BuildingPaused>,
            Option<&SawmillYard>,
        ),
        With<Building>,
    >,
    mut nodes: Query<
        (
            Entity,
            &Transform,
            &mut ResourceNode,
            Option<&YardResourceNode>,
        ),
        Without<Building>,
    >,
    vfx_assets: Option<Res<VfxAssets>>,
    unit_factions: Query<&Faction, With<Unit>>,
) {
    // Pre-compute unit counts per faction for upkeep modifier
    let mut faction_unit_counts: std::collections::HashMap<Faction, u32> =
        std::collections::HashMap::new();
    for f in &unit_factions {
        *faction_unit_counts.entry(*f).or_default() += 1;
    }
    for (
        building_entity,
        building_tf,
        mut processor,
        state,
        faction,
        storage,
        assigned_workers,
        paused,
        sawmill_yard,
    ) in &mut processors
    {
        if *state != BuildingState::Complete {
            continue;
        }

        // Skip harvesting if building is paused
        if paused.is_some() {
            continue;
        }

        processor.harvest_timer.tick(time.delta());
        if !processor.harvest_timer.just_finished() {
            continue;
        }

        // Count assigned workers for this building
        let worker_count = assigned_workers.map(|aw| aw.workers.len()).unwrap_or(0) as f32;

        // Sawmills with yards require workers — no trickle without workers
        let is_yard_building = sawmill_yard.is_some();
        let trickle_fraction = if is_yard_building {
            0.0
        } else if worker_count == 0.0 {
            0.3
        } else {
            0.0
        };
        let base_rate = processor.harvest_rate * trickle_fraction
            + (worker_count * processor.harvest_rate * processor.worker_rate_bonus);
        // Apply population upkeep modifier
        let upkeep =
            income_modifier_for_population(faction_unit_counts.get(faction).copied().unwrap_or(0));
        let effective_rate = base_rate * upkeep;
        processor.harvest_accumulator += effective_rate;
        let amount = processor.harvest_accumulator as u32;
        processor.harvest_accumulator -= amount as f32;
        if amount == 0 {
            continue;
        }
        processor.buffer += amount;

        // Find nearest matching resource node in range and drain from it.
        // Sawmills with yards only harvest from their own yard nodes.
        let mut harvested_type = None;
        for (_node_entity, node_tf, mut node, yard_tag) in &mut nodes {
            if !processor.resource_types.contains(&node.resource_type) {
                continue;
            }
            // Yard-based filtering: sawmills only use their own yard nodes
            if is_yard_building {
                match yard_tag {
                    Some(YardResourceNode(owner)) if *owner == building_entity => {}
                    _ => continue,
                }
            }
            let dist = building_tf.translation.distance(node_tf.translation);
            if dist > processor.harvest_radius {
                continue;
            }
            if node.amount_remaining == 0 {
                continue;
            }

            let drain = processor.buffer.min(node.amount_remaining);
            if drain > 0 {
                node.amount_remaining -= drain;
                harvested_type = Some((node.resource_type, drain));
                processor.buffer -= drain;
                break;
            }
        }

        // Transfer harvested resources to player and spawn popup
        if let Some((rt, amount)) = harvested_type {
            let stored_amount;
            if let Some(mut inv) = storage {
                let stored = inv.add_capped(rt, amount);
                if stored > 0 {
                    all_resources.get_mut(faction).add(rt, stored);
                }
                if amount > stored {
                    processor.buffer += amount - stored;
                }
                stored_amount = stored;
            } else {
                all_resources.get_mut(faction).add(rt, amount);
                stored_amount = amount;
            }

            // Spawn floating "+N" resource popup above the building
            if stored_amount > 0 {
                let popup_pos = building_tf.translation + Vec3::Y * 3.5;
                spawn_resource_popup(&mut commands, popup_pos, rt, stored_amount);

                // Also spawn deposit VFX particles
                if let Some(ref vfx) = vfx_assets {
                    let deposit_pos = building_tf.translation + Vec3::Y * 2.0;
                    spawn_deposit_vfx(&mut commands, &vfx, deposit_pos, 3, 0.12, 0.25);
                }
            }
        }
    }
}

/// Production chain system: buildings with ProductionState convert input resources to outputs.
pub(super) fn production_chain_system(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut all_resources: ResMut<AllPlayerResources>,
    registry: Res<BlueprintRegistry>,
    mut producers: Query<
        (
            Entity,
            &Transform,
            &mut ProductionState,
            &BuildingState,
            &BuildingLevel,
            &Faction,
            Option<&mut StorageInventory>,
            &EntityKind,
            Option<&BuildingPaused>,
        ),
        With<Building>,
    >,
    vfx_assets: Option<Res<VfxAssets>>,
) {
    for (
        _entity,
        building_tf,
        mut production,
        state,
        level,
        faction,
        mut storage,
        building_kind,
        paused,
    ) in &mut producers
    {
        if *state != BuildingState::Complete {
            continue;
        }

        // Skip production if building is paused
        if paused.is_some() {
            continue;
        }

        let Some(recipe_idx) = production.active_recipe else {
            continue;
        };

        if recipe_idx >= production.recipes.len() {
            continue;
        }

        // Check if recipe is unlocked at current building level
        let requires_level = production.recipes[recipe_idx].requires_level;
        if requires_level > level.0 {
            continue;
        }

        // Copy recipe data to avoid borrow conflicts
        let inputs: Vec<(ResourceType, u32)> = production.recipes[recipe_idx].inputs.clone();
        let outputs: Vec<(ResourceType, u32)> = production.recipes[recipe_idx].outputs.clone();
        let mut cycle_secs = production.recipes[recipe_idx].cycle_secs;

        // Apply ProductionSpeedMultiplier from building level bonuses
        let building_bp = registry.get(*building_kind);
        if let Some(ref bd) = building_bp.building {
            for (i, ld) in bd.level_upgrades.iter().enumerate() {
                if (i as u8 + 2) <= level.0 {
                    if let LevelBonus::ProductionSpeedMultiplier(mult) = ld.bonus {
                        cycle_secs *= mult;
                    }
                }
            }
        }

        // Check if we have inputs in the buffer — if not, auto-pull from player resources
        if !production.has_inputs_for_active() {
            let player_res = all_resources.get_mut(faction);
            for (rt, amt) in &inputs {
                if production.input_buffer[rt.index()] < *amt {
                    let needed = *amt - production.input_buffer[rt.index()];
                    let available = player_res.get(*rt);
                    let take = needed.min(available);
                    player_res.amounts[rt.index()] -= take;
                    production.input_buffer[rt.index()] += take;
                }
            }
            if !production.has_inputs_for_active() {
                continue;
            }
        }

        production.progress_timer.tick(time.delta());
        if !production.progress_timer.is_finished() {
            continue;
        }

        // Consume inputs and produce outputs
        production.consume_inputs();
        production.produce_outputs();

        // Transfer outputs to storage/player resources
        for (rt, amt) in &outputs {
            if let Some(ref mut inv) = storage {
                let stored = inv.add_capped(*rt, *amt);
                if stored > 0 {
                    all_resources.get_mut(faction).add(*rt, stored);
                }
            } else {
                all_resources.get_mut(faction).add(*rt, *amt);
            }

            // Spawn floating popup
            let popup_pos = building_tf.translation + Vec3::Y * 3.5;
            spawn_resource_popup(&mut commands, popup_pos, *rt, *amt);

            if let Some(ref vfx) = vfx_assets {
                let deposit_pos = building_tf.translation + Vec3::Y * 2.0;
                spawn_deposit_vfx(&mut commands, &vfx, deposit_pos, 3, 0.12, 0.25);
            }
        }

        // Drain outputs from output buffer
        for (rt, amt) in &outputs {
            production.output_buffer[rt.index()] =
                production.output_buffer[rt.index()].saturating_sub(*amt);
        }

        // Reset timer for next cycle
        if production.auto_repeat {
            production.progress_timer = Timer::from_seconds(cycle_secs, TimerMode::Once);
        }
    }
}

// ── Processor Worker Visual System ──

/// Drives the AssignedPhase state machine for workers in AssignedGathering state.
/// Workers are visible and physically walk between nodes and their assigned building.
pub(super) fn processor_worker_visual_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut workers: Query<
        (
            Entity,
            &Transform,
            &mut UnitState,
            &mut Carrying,
            &Faction,
            Option<&MoveTarget>,
        ),
        With<Unit>,
    >,
    processors: Query<
        (
            Entity,
            &Transform,
            &EntityKind,
            &ResourceProcessor,
            &BuildingState,
            Option<&BuildingPaused>,
            &BuildingFootprint,
            Option<&SawmillYard>,
        ),
        With<Building>,
    >,
    nodes: Query<(Entity, &Transform, &ResourceNode, Option<&YardResourceNode>), Without<Unit>>,
) {
    // Collect nodes targeted by other workers to avoid clustering
    let mut targeted_nodes: Vec<Entity> = Vec::new();
    for (_, _, ustate, _, _, _) in workers.iter() {
        if let UnitState::AssignedGathering { phase, .. } = ustate {
            match phase {
                AssignedPhase::MovingToNode(node) | AssignedPhase::Harvesting { node, .. } => {
                    targeted_nodes.push(*node);
                }
                _ => {}
            }
        }
    }

    for (entity, tf, mut unit_state, _carrying, _faction, move_target) in &mut workers {
        // Check immutably first to avoid triggering Changed<UnitState> for non-assigned units
        let Some(building_entity) = unit_state.assigned_processor_building() else {
            continue;
        };
        // Now access mutably — only workers that actually need phase updates trigger Changed
        let UnitState::AssignedGathering { ref mut phase, .. } = *unit_state else {
            unreachable!()
        };

        let Ok((
            _,
            building_tf,
            building_kind,
            processor,
            building_state,
            building_paused,
            building_fp,
            sawmill_yard,
        )) = processors.get(building_entity)
        else {
            // Building gone — handled by unit_state_executor
            continue;
        };

        let is_yard_building = sawmill_yard.is_some();

        if *building_state != BuildingState::Complete {
            continue;
        }

        // Skip phase state machine if building is paused — workers stay assigned but freeze
        if building_paused.is_some() {
            continue;
        }

        match phase {
            AssignedPhase::SeekingNode => {
                // Find nearest resource node within processor's harvest_radius not targeted by another worker.
                // Sawmills only target their own yard nodes.
                let mut best: Option<(Entity, f32)> = None;
                for (node_entity, node_tf, node_data, yard_tag) in &nodes {
                    if !processor.resource_types.contains(&node_data.resource_type) {
                        continue;
                    }
                    if is_yard_building {
                        match yard_tag {
                            Some(YardResourceNode(owner)) if *owner == building_entity => {}
                            _ => continue,
                        }
                    }
                    if node_data.amount_remaining == 0 {
                        continue;
                    }
                    let dist_to_building = building_tf.translation.distance(node_tf.translation);
                    if dist_to_building > processor.harvest_radius {
                        continue;
                    }
                    let already_targeted =
                        targeted_nodes.iter().filter(|&&n| n == node_entity).count();
                    if already_targeted >= 2 {
                        continue;
                    }
                    let dist = tf.translation.distance(node_tf.translation);
                    if best.is_none() || dist < best.unwrap().1 {
                        best = Some((node_entity, dist));
                    }
                }
                if let Some((node, _)) = best {
                    // Set MoveTarget so the worker physically walks to the node
                    if let Ok((_, node_tf, _, _)) = nodes.get(node) {
                        commands
                            .entity(entity)
                            .insert(MoveTarget(node_tf.translation));
                    }
                    *phase = AssignedPhase::MovingToNode(node);
                }
            }
            AssignedPhase::MovingToNode(node) => {
                let node = *node;
                let Ok((_, node_tf, node_data, _)) = nodes.get(node) else {
                    *phase = AssignedPhase::SeekingNode;
                    commands.entity(entity).remove::<MoveTarget>();
                    continue;
                };
                if node_data.amount_remaining == 0 {
                    *phase = AssignedPhase::SeekingNode;
                    commands.entity(entity).remove::<MoveTarget>();
                    continue;
                }
                // Check if worker arrived at node
                let dist = tf.translation.distance(node_tf.translation);
                if dist <= 3.0 {
                    commands.entity(entity).remove::<MoveTarget>();
                    *phase = AssignedPhase::Harvesting {
                        node,
                        timer_secs: 0.0,
                    };
                }
            }
            AssignedPhase::Harvesting {
                node,
                ref mut timer_secs,
            } => {
                let node = *node;
                if nodes.get(node).is_err()
                    || nodes
                        .get(node)
                        .map(|(_, _, n, _)| n.amount_remaining == 0)
                        .unwrap_or(true)
                {
                    *phase = AssignedPhase::SeekingNode;
                    continue;
                }
                *timer_secs += time.delta_secs();
                if *timer_secs >= 2.5 {
                    // Walk back to building edge, not center
                    let target = building_worker_interaction_target(
                        tf.translation,
                        building_tf,
                        building_fp.0,
                        *building_kind,
                    );
                    commands.entity(entity).insert(MoveTarget(target.position));
                    *phase = AssignedPhase::ReturningToBuilding;
                }
            }
            AssignedPhase::ReturningToBuilding => {
                let target = building_worker_interaction_target(
                    tf.translation,
                    building_tf,
                    building_fp.0,
                    *building_kind,
                );
                let dist = tf.translation.distance(target.position);
                let arrive_range = target.arrive_radius;
                let path_arrived = move_target.is_none();
                if dist <= arrive_range || path_arrived {
                    commands.entity(entity).remove::<MoveTarget>();
                    *phase = AssignedPhase::Depositing { timer_secs: 0.0 };
                }
            }
            AssignedPhase::Depositing { ref mut timer_secs } => {
                *timer_secs += time.delta_secs();
                if *timer_secs >= 0.5 {
                    // Deposit VFX at building
                    if let Some(ref vfx) = vfx_assets {
                        let deposit_pos = building_tf.translation + Vec3::Y * 2.0;
                        for i in 0..3 {
                            let angle = std::f32::consts::TAU * (i as f32 / 3.0);
                            let offset = Vec3::new(angle.cos() * 0.4, 0.3, angle.sin() * 0.4);
                            commands.spawn((
                                VfxFlash {
                                    timer: Timer::from_seconds(0.25, TimerMode::Once),
                                    start_scale: 0.12,
                                    end_scale: 0.0,
                                    rise_speed: 0.35,
                                },
                                FogHideable::Vfx,
                                Mesh3d(vfx.sphere_mesh.clone()),
                                MeshMaterial3d(vfx.deposit_material.clone()),
                                Transform::from_translation(deposit_pos + offset)
                                    .with_scale(Vec3::splat(0.12)),
                            ));
                        }
                    }
                    *phase = AssignedPhase::SeekingNode;
                }
            }
        }
    }
}

// ── Worker assignment helpers ──

/// Assigned processor workers should not remain directly selectable or hoverable.
pub(super) fn lock_assigned_workers_from_user_interaction(
    mut commands: Commands,
    locked_workers: Query<
        Entity,
        (
            With<BuildingAssignment>,
            Or<(With<Selected>, With<Hovered>)>,
        ),
    >,
) {
    for entity in &locked_workers {
        commands
            .entity(entity)
            .remove::<Selected>()
            .remove::<Hovered>();
    }
}

/// Repair worker/building-side assignment drift.
///
/// This keeps `BuildingAssignment`, `UnitState::AssignedGathering`, and the building's
/// `AssignedWorkers` list aligned even after save/load or network sync edge cases.
pub(super) fn reconcile_processor_assignments(
    mut commands: Commands,
    processors: Query<Entity, (With<Building>, With<ResourceProcessor>)>,
    workers: Query<(Entity, &UnitState, Option<&BuildingAssignment>), With<Unit>>,
    building_lists: Query<
        (Entity, Option<&AssignedWorkers>),
        (With<Building>, With<ResourceProcessor>),
    >,
) {
    let mut expected: HashMap<Entity, Vec<Entity>> = HashMap::new();

    for (worker, state, assignment) in &workers {
        let state_building = state.assigned_processor_building();
        let assignment_building = assignment.map(|a| a.0);

        match (state_building, assignment_building) {
            (Some(state_bld), Some(assignment_bld)) if state_bld != assignment_bld => {
                commands
                    .entity(worker)
                    .insert(BuildingAssignment(state_bld));
            }
            (Some(state_bld), None) => {
                commands
                    .entity(worker)
                    .insert(BuildingAssignment(state_bld));
            }
            (None, Some(_)) => {
                commands.entity(worker).remove::<BuildingAssignment>();
            }
            _ => {}
        }

        if let Some(building) = state_building.or(assignment_building) {
            if processors.contains(building) {
                expected.entry(building).or_default().push(worker);
            }
        }
    }

    for workers in expected.values_mut() {
        workers.sort_by_key(|entity| entity.to_bits());
        workers.dedup();
    }

    for (building, assigned) in &building_lists {
        let desired = expected.remove(&building).unwrap_or_default();
        let current = assigned.map(|aw| aw.workers.clone()).unwrap_or_default();
        if current != desired {
            commands
                .entity(building)
                .insert(AssignedWorkers { workers: desired });
        }
    }
}

pub use super::worker_fsm::{assign_worker_to_processor, unassign_worker_from_processor};

/// Safety net: eject excess workers when a building has more assigned than max_workers.
/// This handles deferred-command races where multiple systems assign workers in the same frame.
pub(super) fn enforce_processor_worker_limit(
    mut commands: Commands,
    processors: Query<(Entity, &ResourceProcessor, &AssignedWorkers), With<Building>>,
    worker_states: Query<&UnitState, With<Unit>>,
) {
    for (building_entity, processor, assigned) in &processors {
        let max = processor.max_workers as usize;
        if assigned.workers.len() <= max {
            continue;
        }
        // Eject excess workers (keep the first `max`, remove the rest)
        for &worker in assigned.workers.iter().skip(max) {
            // Only eject if the worker is actually assigned to this building
            if let Ok(state) = worker_states.get(worker) {
                if state.assigned_processor_building() == Some(building_entity) {
                    unassign_worker_from_processor(&mut commands, worker, Some(building_entity));
                }
            }
        }
    }
}
