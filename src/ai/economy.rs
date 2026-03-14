use bevy::prelude::*;

use crate::blueprints::{BlueprintRegistry, EntityKind, EntityVisualCache, ResourceCost};
use crate::buildings::{spawn_wall_line, start_upgrade};
use crate::components::*;
use crate::ground::HeightMap;
use crate::model_assets::BuildingModelAssets;

use super::helpers::*;
use super::types::*;

// ════════════════════════════════════════════════════════════════════
// System 2: Economy — Workers, construction, building placement, walls
// ════════════════════════════════════════════════════════════════════

pub fn ai_economy_system(
    mut commands: Commands,
    time: Res<Time>,
    context: (
        Res<ActivePlayer>,
        Res<TeamConfig>,
        Res<AiControlledFactions>,
        Res<FactionBaseState>,
    ),
    mut ai_state: ResMut<AiState>,
    mut all_resources: ResMut<AllPlayerResources>,
    all_completed: Res<AllCompletedBuildings>,
    carried_totals: Res<CarriedResourceTotals>,
    mut pending_drains: ResMut<PendingCarriedDrains>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    building_models: Option<Res<BuildingModelAssets>>,
    height_map: Res<HeightMap>,
    biome_map: Res<BiomeMap>,
    queries: (
        Query<(Entity, &Faction, &Transform, &UnitState), (With<Unit>, With<GatherSpeed>)>,
        Query<(Entity, &Transform, &ResourceNode), Without<Unit>>,
        Query<(Entity, &Faction, &EntityKind, &Transform, &BuildingState), With<Building>>,
        Query<
            (
                &Faction,
                &EntityKind,
                &BuildingLevel,
                Entity,
                &BuildingState,
            ),
            With<Building>,
        >,
        Query<(&Faction, &ConstructionWorkers, &BuildingState), With<Building>>,
        Query<(&Faction, &EntityKind, &mut TrainingQueue), With<Building>>,
        Query<&BuildingFootprint>,
        Query<(Entity, &Faction, &ResourceProcessor, &BuildingState), With<Building>>,
        Query<&AssignedWorkers>,
    ),
) {
    let dt = time.delta_secs();
    let (active_player, teams, ai_controlled, base_state) = context;
    let (
        workers_q,
        resource_nodes_q,
        buildings_q,
        building_levels_q,
        construction_workers_q,
        mut train_queues,
        footprints_q,
        processor_q,
        assigned_workers_q,
    ) = queries;

    for &faction in &ai_controlled.factions {
        if faction == active_player.0 {
            continue;
        }

        let is_friendly = teams.is_allied(&faction, &active_player.0);
        let is_founded =
            base_state.is_founded(&faction) || all_completed.has(&faction, EntityKind::Base);

        let brain = match ai_state.factions.get_mut(&faction) {
            Some(b) => b,
            None => continue,
        };

        brain.economy_timer -= dt;
        if brain.economy_timer > 0.0 {
            continue;
        }
        brain.economy_timer = brain.effective_tick(ECONOMY_TICK);

        // Apply resource bonus for Hard difficulty
        if brain.difficulty.resource_bonus() > 0.0 {
            let bonus = brain.difficulty.resource_bonus();
            let res = all_resources.get_mut(&faction);
            let trickle = (5.0 * bonus) as u32;
            res.add(ResourceType::Wood, trickle);
            res.add(ResourceType::Copper, trickle);
        }

        // ── Track income rates ──
        {
            let current_res = all_resources.get(&faction);
            for (i, rt) in ResourceType::ALL.iter().enumerate() {
                let current = current_res.get(*rt);
                let prev = brain.last_resource_snapshot[i];
                if prev > 0 {
                    let delta = current as f32 - prev as f32;
                    brain.income_rates[i] = delta / ECONOMY_TICK;
                }
                brain.last_resource_snapshot[i] = current;
            }
        }

        // Update resource goal from first build queue item
        if let Some(first) = brain.build_queue.first() {
            let bp = registry.get(first.kind);
            brain.resource_goal = Some(ResourceGoal {
                wood: bp.cost.get(ResourceType::Wood),
                copper: bp.cost.get(ResourceType::Copper),
                iron: bp.cost.get(ResourceType::Iron),
                gold: bp.cost.get(ResourceType::Gold),
                oil: bp.cost.get(ResourceType::Oil),
            });
        } else {
            brain.resource_goal = None;
        }

        // Cache base position
        let mut base_pos = None;
        let mut our_building_positions: Vec<Vec3> = Vec::new();
        for (_, f, kind, tf, _) in buildings_q.iter() {
            if *f == faction {
                our_building_positions.push(tf.translation);
                if *kind == EntityKind::Base {
                    base_pos = Some(tf.translation);
                }
            }
        }
        let worker_positions: Vec<Vec3> = workers_q
            .iter()
            .filter(|(_, f, _, _)| **f == faction)
            .map(|(_, _, tf, _)| tf.translation)
            .collect();

        let fallback_pos = if worker_positions.is_empty() {
            None
        } else {
            let sum = worker_positions
                .iter()
                .copied()
                .fold(Vec3::ZERO, |acc, p| acc + p);
            Some(sum / worker_positions.len() as f32)
        };

        let base_pos = match base_pos.or(fallback_pos) {
            Some(p) => p,
            None => continue,
        };
        brain.base_position = Some(base_pos);

        if !is_founded {
            let bp = registry.get(EntityKind::Base);
            let carried = carried_totals.get(&faction);
            if bp
                .cost
                .can_afford_with_carried(all_resources.get(&faction), carried)
            {
                let deficits =
                    bp.cost.deduct_with_carried(all_resources.get_mut(&faction));
                let drain = SpendFromCarried {
                    faction,
                    amounts: deficits,
                };
                if drain.has_deficit() {
                    pending_drains.drains.push(drain);
                }

                let pos = find_build_pos(
                    base_pos,
                    &our_building_positions,
                    EntityKind::Base,
                    &footprints_q,
                    &height_map,
                    Some(base_pos),
                );
                spawn_ai_building(
                    &mut commands,
                    &cache,
                    EntityKind::Base,
                    pos,
                    &registry,
                    building_models.as_deref(),
                    &height_map,
                    faction,
                );
                brain.pending_builds += 1;
            }
            continue;
        }

        // Get player base pos for friendly AI resource avoidance
        let player_base_pos = if is_friendly {
            buildings_q
                .iter()
                .find(|(_, f, kind, _, _)| **f == active_player.0 && **kind == EntityKind::Base)
                .map(|(_, _, _, tf, _)| tf.translation)
        } else {
            None
        };

        // Count workers
        let mut worker_count = 0usize;
        for (_, f, _, _) in workers_q.iter() {
            if *f == faction {
                worker_count += 1;
            }
        }

        // ── Train workers if needed ──
        let desired = brain.desired_workers as usize;
        if worker_count < desired {
            let bp = registry.get(EntityKind::Worker);
            let carried = carried_totals.get(&faction);
            if bp
                .cost
                .can_afford_with_carried(all_resources.get(&faction), carried)
            {
                if try_train(&mut train_queues, &faction, EntityKind::Worker, &registry) {
                    let deficits =
                        bp.cost.deduct_with_carried(all_resources.get_mut(&faction));
                    let drain = SpendFromCarried {
                        faction,
                        amounts: deficits,
                    };
                    if drain.has_deficit() {
                        pending_drains.drains.push(drain);
                    }
                }
            }
        }

        // ── Assign idle workers — goal-aware resource picking ──
        let pr = all_resources.get(&faction);
        let player_res = PlayerResources {
            amounts: pr.amounts,
        };
        let mut idle_workers: Vec<(Entity, Vec3)> = Vec::new();
        for (entity, f, tf, task) in workers_q.iter() {
            if *f != faction {
                continue;
            }
            if *task == UnitState::Idle && !brain.assigned_units.contains_key(&entity) {
                idle_workers.push((entity, tf.translation));
            }
        }

        let resource_goal = brain.resource_goal.clone();
        let top_state = brain.top_state;
        for (entity, pos) in &idle_workers {
            let needed = pick_goal_aware_resource(&player_res, resource_goal.as_ref(), top_state);
            let role = SquadRole::for_resource(needed);

            if let Some(node_entity) = find_nearest_resource_node_with_avoidance(
                *pos,
                needed,
                &resource_nodes_q,
                200.0,
                player_base_pos,
                is_friendly,
            ) {
                commands
                    .entity(*entity)
                    .insert(UnitState::Gathering(node_entity));
                brain.add_to_squad(*entity, role);
            }
        }

        // ── Assign idle workers to processor buildings with open slots ──
        for (proc_entity, proc_faction, processor, proc_state) in processor_q.iter() {
            if *proc_faction != faction || *proc_state != BuildingState::Complete {
                continue;
            }
            if processor.max_workers == 0 {
                continue;
            }
            let current_count = assigned_workers_q
                .get(proc_entity)
                .map(|aw| aw.workers.len())
                .unwrap_or(0);
            if current_count >= processor.max_workers as usize {
                continue;
            }
            let slots = processor.max_workers as usize - current_count;
            let mut assigned = 0;
            for (w_entity, w_f, _, w_task) in workers_q.iter() {
                if *w_f != faction || *w_task != UnitState::Idle {
                    continue;
                }
                if brain.assigned_units.contains_key(&w_entity) {
                    continue;
                }
                if assigned >= slots {
                    break;
                }
                crate::resources::assign_worker_to_processor(&mut commands, w_entity, proc_entity);
                commands
                    .entity(proc_entity)
                    .entry::<AssignedWorkers>()
                    .and_modify(move |mut aw| {
                        if !aw.workers.contains(&w_entity) {
                            aw.workers.push(w_entity);
                        }
                    })
                    .or_insert(AssignedWorkers {
                        workers: vec![w_entity],
                    });
                brain.add_to_squad(w_entity, SquadRole::GatherCopper);
                assigned += 1;
            }
        }

        // ── Assign workers to construction ──
        for (entity, f, _kind, tf, state) in buildings_q.iter() {
            if *f != faction || *state != BuildingState::UnderConstruction {
                continue;
            }
            let cw = construction_workers_q
                .get(entity)
                .map(|(_, cw, _)| cw.0)
                .unwrap_or(0);

            if cw < 2 {
                let mut best: Option<(Entity, f32)> = None;
                for (w_entity, w_f, w_tf, w_task) in workers_q.iter() {
                    if *w_f != faction {
                        continue;
                    }
                    if *w_task != UnitState::Idle {
                        continue;
                    }
                    let role = brain.assigned_units.get(&w_entity);
                    if role.is_some() && !role.unwrap().is_gather() {
                        continue;
                    }
                    let d = w_tf.translation.distance(tf.translation);
                    if best.is_none() || d < best.unwrap().1 {
                        best = Some((w_entity, d));
                    }
                }
                if let Some((w_entity, _)) = best {
                    brain.remove_from_squad(w_entity);
                    brain.add_to_squad(w_entity, SquadRole::BuildConstruction);
                    commands
                        .entity(w_entity)
                        .insert(UnitState::MovingToBuild(entity));
                }
            }
        }

        // ── Execute build queue ──
        let pending = brain.pending_builds;
        let max_builds = brain.max_build_queue();
        if (pending as usize) < max_builds && !brain.build_queue.is_empty() {
            let build_queue = brain.build_queue.clone();
            let mut built_one = false;
            for (idx, request) in build_queue.iter().enumerate() {
                let bp = registry.get(request.kind);

                // Check prerequisite
                if let Some(ref bd) = bp.building {
                    if let Some(prereq) = bd.prerequisite {
                        if !all_completed.has(&faction, prereq) {
                            continue;
                        }
                    }
                }

                let carried = carried_totals.get(&faction);
                if !bp
                    .cost
                    .can_afford_with_carried(all_resources.get(&faction), carried)
                {
                    continue;
                }

                // Find position
                let near = request.near_position.or_else(|| {
                    find_resource_biome_pos(request.kind, base_pos, &biome_map, &height_map)
                });
                let pos = find_build_pos(
                    base_pos,
                    &our_building_positions,
                    request.kind,
                    &footprints_q,
                    &height_map,
                    near,
                );

                let deficits =
                    bp.cost.deduct_with_carried(all_resources.get_mut(&faction));
                let drain = SpendFromCarried {
                    faction,
                    amounts: deficits,
                };
                if drain.has_deficit() {
                    pending_drains.drains.push(drain);
                }
                spawn_ai_building(
                    &mut commands,
                    &cache,
                    request.kind,
                    pos,
                    &registry,
                    building_models.as_deref(),
                    &height_map,
                    faction,
                );
                brain.pending_builds += 1;
                // Remove from persistent queue
                brain.build_queue.remove(idx);
                built_one = true;
                break;
            }
            let _ = built_one;
        }

        // ── Wall building (Militarize+ states) ──
        if matches!(
            brain.top_state,
            AiTopState::Militarize
                | AiTopState::Expanding
                | AiTopState::Defending
                | AiTopState::LateGame
        ) {
            // Generate wall plan if we don't have one
            if brain.wall_plan.is_none() {
                brain.wall_plan = Some(generate_wall_plan(base_pos, brain.personality));
            }

            if let Some(ref mut wall_plan) = brain.wall_plan.clone() {
                // Build one uncompleted side per tick
                for i in 0..4 {
                    if wall_plan.completed[i] {
                        continue;
                    }
                    let (start, end) = wall_plan.sides[i];
                    // Check if we can afford a wall segment
                    let wall_post_bp = registry.get(EntityKind::WallPost);
                    let wall_seg_bp = registry.get(EntityKind::WallSegment);
                    let est_wood = wall_post_bp.cost.get(ResourceType::Wood) * 2 + wall_seg_bp.cost.get(ResourceType::Wood);
                    let est_copper = wall_post_bp.cost.get(ResourceType::Copper) * 2 + wall_seg_bp.cost.get(ResourceType::Copper);
                    let pr_check = all_resources.get(&faction);
                    if pr_check.get(ResourceType::Wood) < est_wood
                        || pr_check.get(ResourceType::Copper) < est_copper
                    {
                        break;
                    }

                    let points = generate_wall_points(start, end, &height_map);
                    if points.len() < 2 {
                        wall_plan.completed[i] = true;
                        continue;
                    }

                    let num_posts = points.len() as u32;
                    let num_segs = (points.len() as u32).saturating_sub(1);
                    let mut total_cost = ResourceCost::default();
                    for rt in ResourceType::ALL.iter() {
                        let amt = wall_post_bp.cost.get(*rt) * num_posts + wall_seg_bp.cost.get(*rt) * num_segs;
                        if amt > 0 {
                            total_cost.set(*rt, amt);
                        }
                    }

                    let res = all_resources.get_mut(&faction);
                    res.subtract_cost(&total_cost);

                    spawn_wall_line(
                        &mut commands,
                        &cache,
                        &registry,
                        building_models.as_deref(),
                        &height_map,
                        faction,
                        &points,
                    );

                    wall_plan.completed[i] = true;
                    if let Some(ref mut bp) = brain.wall_plan {
                        bp.completed[i] = true;
                    }
                    break;
                }
            }
        }

        // ── Building upgrades (Expanding+) ──
        if matches!(
            brain.top_state,
            AiTopState::Expanding | AiTopState::LateGame
        ) {
            let upgrade_priorities = [
                EntityKind::GuardTower,
                EntityKind::WatchTower,
                EntityKind::Barracks,
                EntityKind::Storage,
            ];
            for target_kind in &upgrade_priorities {
                for (f, kind, level, entity, state) in building_levels_q.iter() {
                    if *f != faction
                        || kind != target_kind
                        || *state != BuildingState::Complete
                        || level.0 >= 3
                    {
                        continue;
                    }
                    let mut res = PlayerResources {
                        amounts: all_resources.get(&faction).amounts,
                    };
                    let carried = carried_totals.get(&faction);
                    if start_upgrade(
                        &mut commands,
                        entity,
                        level.0,
                        *kind,
                        &registry,
                        &mut res,
                        faction,
                        carried,
                        &mut pending_drains,
                    ) {
                        *all_resources.get_mut(&faction) = res;
                        break;
                    }
                }
            }
        }
    }
}
