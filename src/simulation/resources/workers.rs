//! Worker AI loop: seeking nodes, gathering accumulation, deposit
//! delivery to buildings, and carried-inventory persistence.

use bevy::ecs::system::SystemParam;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::time::Fixed;

use super::{emit_carrying_delta, CarryingDelta};
use crate::blueprints::EntityKind;
use crate::infrastructure::audio::{PlaySfx, SfxKind};
use crate::infrastructure::net_bridge::NetworkId;
use crate::types::*;
use crate::ui::theme;

#[derive(SystemParam)]
pub(super) struct WorkerAiParams<'w, 's> {
    all_resources: ResMut<'w, AllPlayerResources>,
    event_log: ResMut<'w, crate::ui::event_log_widget::GameEventLog>,
    vfx_assets: Option<Res<'w, VfxAssets>>,
    active_player: Res<'w, ActivePlayer>,
    workers: Query<
        'w,
        's,
        (
            Entity,
            &'static Transform,
            &'static mut UnitState,
            &'static mut TaskSource,
            &'static mut Carrying,
            &'static GatherSpeed,
            &'static CarryCapacity,
            &'static mut GatherAccumulator,
            &'static EntityKind,
            &'static Faction,
            Option<&'static MoveTarget>,
            &'static mut TaskQueue,
            Option<&'static ManualIdleSince>,
            Option<&'static PreferredResource>,
        ),
        With<Unit>,
    >,
    nodes: Query<
        'w,
        's,
        (
            &'static Transform,
            &'static mut ResourceNode,
            Option<&'static YardResourceNode>,
        ),
        Without<Unit>,
    >,
    deposit_points: Query<
        'w,
        's,
        (
            Entity,
            &'static Transform,
            &'static BuildingState,
            &'static Faction,
            &'static EntityKind,
            &'static BuildingFootprint,
            Option<&'static NetworkId>,
        ),
        (With<DepositPoint>, Without<Unit>),
    >,
    inventories: Query<
        'w,
        's,
        (&'static Transform, Option<&'static mut StorageInventory>),
        (With<DepositPoint>, Without<Unit>),
    >,
    all_nodes: Query<
        'w,
        's,
        (
            Entity,
            &'static Transform,
            Option<&'static YardResourceNode>,
            Option<&'static NetworkId>,
        ),
        (With<ResourceNode>, Without<Unit>),
    >,
    construction_sites: Query<
        'w,
        's,
        (
            Entity,
            &'static Transform,
            &'static BuildingState,
            &'static Faction,
            Option<&'static NetworkId>,
        ),
        (With<Building>, Without<Unit>, Without<ResourceNode>),
    >,
    storage_auras: Query<
        'w,
        's,
        (
            &'static Transform,
            &'static StorageAura,
            &'static BuildingState,
        ),
        With<Building>,
    >,
    carry_deltas: bevy::ecs::message::MessageWriter<'w, CarryingDelta>,
}

pub(super) fn worker_ai_system(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut params: WorkerAiParams,
    mut sfx_state: (bevy::ecs::message::MessageWriter<PlaySfx>, Local<f32>),
) {
    let gather_range = 3.0;
    let auto_scan_range = 20.0;

    const GATHER_SFX_INTERVAL: f32 = 2.0;
    *sfx_state.1 += time.delta_secs();
    let play_gather = *sfx_state.1 >= GATHER_SFX_INTERVAL;
    if play_gather {
        *sfx_state.1 = 0.0;
    }

    for (
        entity,
        tf,
        mut state,
        mut source,
        mut carrying,
        speed,
        capacity,
        mut gather_accumulator,
        kind,
        worker_faction,
        _move_target,
        mut task_queue,
        manual_idle_since,
        preferred_resource,
    ) in &mut params.workers
    {
        if *kind != EntityKind::Worker {
            continue;
        }

        if !matches!(*state, UnitState::Gathering(_)) {
            gather_accumulator.0 = 0.0;
        }

        match *state {
            UnitState::Moving(_) => {
                // Player issued a manual move — handled by unit_state_executor
                continue;
            }
            UnitState::Idle => {
                // Only auto-decide if source is Auto and queue is empty
                if *source == TaskSource::Manual
                    || task_queue.current.is_some()
                    || !task_queue.queue.is_empty()
                {
                    continue;
                }
                // Respect manual placement grace period (5 seconds)
                let now = time.elapsed_secs_f64();
                if let Some(since) = manual_idle_since {
                    if now - since.0 < 5.0 {
                        continue;
                    }
                    commands.entity(entity).remove::<ManualIdleSince>();
                }

                // If carrying resources, find depot to deposit (resource-type aware)
                if carrying.amount > 0 {
                    let new_state = return_to_depot_or_wait(
                        &mut commands,
                        entity,
                        &tf.translation,
                        worker_faction,
                        &carrying,
                        None,
                        &params.deposit_points,
                        Some(&params.inventories),
                    );
                    if !matches!(new_state, UnitState::Idle) {
                        *state = new_state;
                        continue;
                    }
                    // No valid depot exists yet. Keep the worker available for
                    // bootstrapping tasks like constructing the first base.
                }
                // Check for nearby unfinished buildings first (prioritize construction)
                let auto_build_range = 20.0;
                let mut site_candidates: Vec<(i32, u32, Entity)> = Vec::new();
                for (site_entity, site_tf, site_state, site_faction, site_net_id) in
                    &params.construction_sites
                {
                    if *site_state != BuildingState::UnderConstruction
                        || site_faction != worker_faction
                    {
                        continue;
                    }
                    let dist = tf.translation.distance(site_tf.translation);
                    if dist >= auto_build_range {
                        continue;
                    }
                    let net_id = site_net_id.map(|id| id.0).unwrap_or(u32::MAX);
                    site_candidates.push((quantize_distance(dist), net_id, site_entity));
                }
                site_candidates.sort_by_key(|&(q, id, _)| (q, id));
                if let Some(&(_, _, site)) = site_candidates.first() {
                    *state = UnitState::MovingToBuild(site);
                } else if let Some(node) = preferred_resource.and_then(|pref| {
                    find_nearest_node_of_type(
                        &tf.translation,
                        pref.0,
                        &params.all_nodes,
                        &params.nodes,
                        worker_faction,
                        &params.deposit_points,
                        &params.inventories,
                    )
                }) {
                    *state = UnitState::Gathering(node);
                } else if let Some(node) = find_nearest_node(
                    &tf.translation,
                    auto_scan_range,
                    &params.all_nodes,
                    &params.nodes,
                    Some(worker_faction),
                    Some(&params.deposit_points),
                    Some(&params.inventories),
                ) {
                    *state = UnitState::Gathering(node);
                }
            }

            UnitState::Gathering(node) => {
                // Interrupt auto-gathering for nearby construction (only if not carrying)
                if *source == TaskSource::Auto && carrying.amount == 0 {
                    let auto_build_range = 20.0;
                    let mut site_candidates: Vec<(i32, u32, Entity)> = Vec::new();
                    for (site_entity, site_tf, site_state, site_faction, site_net_id) in
                        &params.construction_sites
                    {
                        if *site_state != BuildingState::UnderConstruction
                            || site_faction != worker_faction
                        {
                            continue;
                        }
                        let dist = tf.translation.distance(site_tf.translation);
                        if dist >= auto_build_range {
                            continue;
                        }
                        let net_id = site_net_id.map(|id| id.0).unwrap_or(u32::MAX);
                        site_candidates.push((quantize_distance(dist), net_id, site_entity));
                    }
                    site_candidates.sort_by_key(|&(q, id, _)| (q, id));
                    if let Some(&(_, _, site)) = site_candidates.first() {
                        commands.entity(entity).remove::<MoveTarget>();
                        *state = UnitState::MovingToBuild(site);
                        continue;
                    }
                }

                let Ok((node_tf, mut node_data, yard_tag)) = params.nodes.get_mut(node) else {
                    // Node gone
                    commands.entity(entity).remove::<MoveTarget>();
                    let new_state = return_to_depot_or_wait(
                        &mut commands,
                        entity,
                        &tf.translation,
                        worker_faction,
                        &carrying,
                        None,
                        &params.deposit_points,
                        Some(&params.inventories),
                    );
                    *state = new_state;
                    if matches!(*state, UnitState::Idle) {
                        *source = TaskSource::Auto;
                        task_queue.current = None;
                    }
                    continue;
                };

                if yard_tag.is_some() {
                    commands.entity(entity).remove::<MoveTarget>();
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                    continue;
                }

                if node_data.amount_remaining == 0 {
                    let new_state = return_to_depot_or_wait(
                        &mut commands,
                        entity,
                        &tf.translation,
                        worker_faction,
                        &carrying,
                        None,
                        &params.deposit_points,
                        Some(&params.inventories),
                    );
                    *state = new_state;
                    if matches!(*state, UnitState::Idle) {
                        *source = TaskSource::Auto;
                        task_queue.current = None;
                    }
                    continue;
                }

                // If pushed away by avoidance, go back
                let dist = tf.translation.distance(node_tf.translation);
                if dist > gather_range {
                    // MovingToResource equivalent: just set MoveTarget, stay in Gathering
                    commands
                        .entity(entity)
                        .insert(MoveTarget(node_tf.translation));
                    continue;
                }

                // Remove any stale MoveTarget
                commands.entity(entity).remove::<MoveTarget>();

                let rt = node_data.resource_type;
                let node_pos = node_tf.translation;

                // If carrying a different resource type, return to deposit first.
                if carrying.amount > 0 {
                    if let Some(carried_rt) = carrying.resource_type {
                        if carried_rt != rt {
                            let new_state = return_to_depot_or_wait(
                                &mut commands,
                                entity,
                                &tf.translation,
                                worker_faction,
                                &carrying,
                                Some(node),
                                &params.deposit_points,
                                Some(&params.inventories),
                            );
                            *state = new_state;
                            if matches!(*state, UnitState::Idle) {
                                *source = TaskSource::Auto;
                                task_queue.current = None;
                            }
                            continue;
                        }
                    }
                }

                // Frame-rate independent gather tick (with storage aura + per-resource modifier).
                let unit_weight = rt.weight();
                let aura_bonus = crate::simulation::buildings::storage_aura_bonus(
                    tf.translation,
                    &params.storage_auras,
                );
                let effective_speed = speed.0 * (1.0 + aura_bonus) * rt.gather_rate_multiplier();
                gather_accumulator.0 += effective_speed * time.delta_secs();
                let mut amount = gather_accumulator.0.floor() as u32;
                if amount == 0 {
                    continue;
                }
                gather_accumulator.0 -= amount as f32;
                amount = amount.min(node_data.amount_remaining);

                let remaining_capacity = (capacity.0 - carrying.weight).max(0.0);
                let can_carry = (remaining_capacity / unit_weight).floor() as u32;
                if can_carry == 0 {
                    let new_state = return_to_depot_or_wait(
                        &mut commands,
                        entity,
                        &tf.translation,
                        worker_faction,
                        &carrying,
                        Some(node),
                        &params.deposit_points,
                        Some(&params.inventories),
                    );
                    *state = new_state;
                    if matches!(*state, UnitState::Idle) {
                        *source = TaskSource::Auto;
                        task_queue.current = None;
                    }
                    continue;
                }

                let actual = amount.min(can_carry);
                if actual == 0 {
                    continue;
                }

                node_data.amount_remaining -= actual;
                carrying.amount += actual;
                carrying.weight += actual as f32 * unit_weight;
                carrying.resource_type = Some(rt);
                emit_carrying_delta(&mut params.carry_deltas, *worker_faction, rt, actual as i32);

                if let Some(ref vfx) = params.vfx_assets {
                    spawn_gather_vfx(&mut commands, vfx, tf.translation, node_pos, rt, actual);
                }

                if play_gather && *worker_faction == params.active_player.0 {
                    sfx_state.0.write(PlaySfx {
                        kind: SfxKind::WorkerGather,
                        position: Some(tf.translation),
                    });
                }

                // If effectively full for this resource type, head to a depot.
                let remaining_capacity_after = (capacity.0 - carrying.weight).max(0.0);
                if remaining_capacity_after < unit_weight {
                    let new_state = return_to_depot_or_wait(
                        &mut commands,
                        entity,
                        &tf.translation,
                        worker_faction,
                        &carrying,
                        Some(node),
                        &params.deposit_points,
                        Some(&params.inventories),
                    );
                    *state = new_state;
                    if matches!(*state, UnitState::Idle) {
                        *source = TaskSource::Auto;
                        task_queue.current = None;
                    }
                }
            }

            UnitState::ReturningToDeposit { depot, gather_node } => {
                // Check depot still exists
                let Ok((_, depot_tf, _, _, depot_kind, depot_fp, _)) =
                    params.deposit_points.get(depot)
                else {
                    commands.entity(entity).remove::<MoveTarget>();
                    let new_state = return_to_depot_or_wait(
                        &mut commands,
                        entity,
                        &tf.translation,
                        worker_faction,
                        &carrying,
                        gather_node,
                        &params.deposit_points,
                        Some(&params.inventories),
                    );
                    *state = new_state;
                    if matches!(*state, UnitState::Idle) {
                        *source = TaskSource::Auto;
                        task_queue.current = None;
                    }
                    continue;
                };

                let target = building_worker_interaction_target(
                    tf.translation,
                    depot_tf,
                    depot_fp.0,
                    *depot_kind,
                );
                let dist = tf.translation.distance(target.position);
                let deposit_range = target.arrive_radius;

                if dist <= deposit_range {
                    commands.entity(entity).remove::<MoveTarget>();
                    *state = UnitState::Depositing { depot, gather_node };
                } else if _move_target.is_none() {
                    // Worker has no MoveTarget — either just arrived or path was
                    // consumed. Re-issue a move toward the building edge.
                    // But if we're only slightly outside range, just deposit
                    // rather than sending the worker on another trip.
                    if dist <= deposit_range + 2.0 {
                        *state = UnitState::Depositing { depot, gather_node };
                    } else {
                        commands.entity(entity).insert(MoveTarget(target.position));
                    }
                }
            }

            UnitState::Depositing { depot, gather_node } => {
                // Transfer resources (capacity-limited)
                let mut deposit_info: Option<(ResourceType, u32)> = None;
                if let Some(rt) = carrying.resource_type {
                    let mut deposited = carrying.amount;

                    // Check storage capacity
                    if let Ok((_, inventory)) = params.inventories.get_mut(depot) {
                        if let Some(mut inv) = inventory {
                            deposited = inv.add_capped(rt, carrying.amount);
                        }
                    }

                    if deposited == 0 {
                        // Storage full — wait nearby
                        params.event_log.push(
                            time.elapsed_secs(),
                            "Storage full — worker waiting".into(),
                            crate::ui::event_log_widget::EventCategory::Alert,
                            Some(tf.translation),
                            Some(*worker_faction),
                        );
                        commands.entity(entity).remove::<MoveTarget>();
                        *state = UnitState::WaitingForStorage { depot, gather_node };
                        continue;
                    }

                    // Add deposited amount to global resources
                    params
                        .all_resources
                        .get_mut(worker_faction)
                        .add(rt, deposited);
                    deposit_info = Some((rt, deposited));

                    // Update carrying with leftover
                    let leftover = carrying.amount - deposited;
                    if leftover > 0 {
                        carrying.amount = leftover;
                        carrying.weight = leftover as f32 * rt.weight();
                        emit_carrying_delta(
                            &mut params.carry_deltas,
                            *worker_faction,
                            rt,
                            -(deposited as i32),
                        );
                        // Worker still has resources — wait for capacity
                        commands.entity(entity).remove::<MoveTarget>();
                        *state = UnitState::WaitingForStorage { depot, gather_node };

                        // Still spawn VFX for partial deposit
                        if let Some(ref vfx) = params.vfx_assets {
                            if let Ok((depot_tf, _)) = params.inventories.get(depot) {
                                let deposit_pos = depot_tf.translation + Vec3::Y * 2.0;
                                spawn_deposit_vfx(&mut commands, &vfx, deposit_pos, 4, 0.15, 0.3);
                                spawn_resource_popup(
                                    &mut commands,
                                    deposit_pos + Vec3::Y * 1.5,
                                    rt,
                                    deposited,
                                );
                            }
                        }
                        continue;
                    }
                }

                // Spawn deposit VFX + resource popup
                if let Some(ref vfx) = params.vfx_assets {
                    if let Ok((depot_tf, _)) = params.inventories.get(depot) {
                        let deposit_pos = depot_tf.translation + Vec3::Y * 2.0;
                        spawn_deposit_vfx(&mut commands, &vfx, deposit_pos, 4, 0.15, 0.3);
                        if let Some((rt, amount)) = deposit_info {
                            spawn_resource_popup(
                                &mut commands,
                                deposit_pos + Vec3::Y * 1.5,
                                rt,
                                amount,
                            );
                        }
                    }
                }

                // Clear carrying
                if let Some(rt) = carrying.resource_type {
                    emit_carrying_delta(
                        &mut params.carry_deltas,
                        *worker_faction,
                        rt,
                        -(carrying.amount as i32),
                    );
                }
                carrying.amount = 0;
                carrying.weight = 0.0;
                carrying.resource_type = None;
                commands.entity(entity).remove::<MoveTarget>();

                // Return to gather node if it still has resources
                if let Some(gn) = gather_node {
                    if let Ok((_, node_data, yard_tag)) = params.nodes.get(gn) {
                        if yard_tag.is_some() {
                            *state = UnitState::Idle;
                            *source = TaskSource::Auto;
                            task_queue.current = None;
                            continue;
                        }
                        if node_data.amount_remaining > 0 {
                            *state = UnitState::Gathering(gn);
                            continue;
                        }
                    }
                }
                // No gather node or depleted — scan broadly for next resource
                if let Some(node) = find_nearest_node(
                    &tf.translation,
                    auto_scan_range,
                    &params.all_nodes,
                    &params.nodes,
                    Some(worker_faction),
                    Some(&params.deposit_points),
                    Some(&params.inventories),
                ) {
                    *state = UnitState::Gathering(node);
                } else {
                    *state = UnitState::Idle;
                    *source = TaskSource::Auto;
                    task_queue.current = None;
                }
            }

            UnitState::WaitingForStorage { depot, gather_node } => {
                let carried_rt = carrying.resource_type;

                // Periodically check if depot has capacity for the carried resource type
                let has_space = if let Ok((_, inventory)) = params.inventories.get(depot) {
                    inventory.map_or(true, |inv| {
                        if let Some(rt) = carried_rt {
                            inv.accepts(rt) && inv.remaining_capacity_for(rt) > 0
                        } else {
                            inv.remaining_capacity() > 0
                        }
                    })
                } else {
                    false
                };

                if has_space {
                    *state = UnitState::Depositing { depot, gather_node };
                    continue;
                }

                // Try a different depot that accepts our resource type and has space
                let mut best_depot: Option<(Entity, f32, u32)> = None;
                for (dp_entity, dp_tf, dp_state, dp_faction, _dp_kind, _dp_fp, dp_net_id) in
                    &params.deposit_points
                {
                    if dp_faction != worker_faction || *dp_state != BuildingState::Complete {
                        continue;
                    }
                    // Check this depot accepts and has capacity for the carried resource
                    if let Ok((_, inv_opt)) = params.inventories.get(dp_entity) {
                        if let Some(inv) = inv_opt {
                            if let Some(rt) = carried_rt {
                                if !inv.accepts(rt) || inv.remaining_capacity_for(rt) == 0 {
                                    continue;
                                }
                            } else if inv.remaining_capacity() == 0 {
                                continue;
                            }
                        }
                    }
                    let dist = tf.translation.distance(dp_tf.translation);
                    let candidate = (
                        dp_entity,
                        dist,
                        dp_net_id.map(|id| id.0).unwrap_or(u32::MAX),
                    );
                    if best_depot.as_ref().is_none_or(|current| {
                        worker_candidate_cmp(candidate.1, candidate.2, current.1, current.2).is_lt()
                    }) {
                        best_depot = Some(candidate);
                    }
                }

                if let Some((new_depot, _, _)) = best_depot {
                    let (_, depot_tf, _, _, depot_kind, depot_fp, _) =
                        params.deposit_points.get(new_depot).unwrap();
                    let target = building_worker_interaction_target(
                        tf.translation,
                        depot_tf,
                        depot_fp.0,
                        *depot_kind,
                    );
                    commands.entity(entity).insert(MoveTarget(target.position));
                    *state = UnitState::ReturningToDeposit {
                        depot: new_depot,
                        gather_node,
                    };
                }
                // Otherwise keep waiting at current depot
            }

            UnitState::WaitingForDepot { gather_node } => {
                let new_state = return_to_depot_or_wait(
                    &mut commands,
                    entity,
                    &tf.translation,
                    worker_faction,
                    &carrying,
                    gather_node,
                    &params.deposit_points,
                    Some(&params.inventories),
                );
                if !matches!(new_state, UnitState::WaitingForDepot { .. }) {
                    *state = new_state;
                }
            }

            // Building/MovingToBuild states are handled by unit_state_executor
            // AssignedGathering handled by processor_worker_visual_system + unit_state_executor
            _ => {}
        }
    }
}

/// Spawn a floating "+N" resource popup UI node at a world position.
pub(super) fn spawn_resource_popup(
    commands: &mut Commands,
    world_pos: Vec3,
    rt: ResourceType,
    amount: u32,
) {
    let color = rt.carry_color();
    let srgba = color.to_srgba();
    commands.spawn((
        ResourcePopup {
            lifetime: Timer::from_seconds(1.2, TimerMode::Once),
            world_pos,
            resource_type: rt,
            amount,
        },
        WorldOverlayBackItem,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(-1000.0),
            top: Val::Px(-1000.0),
            ..default()
        },
        Text::new(format!("+{}", amount)),
        TextFont {
            font_size: theme::FONT_LARGE,
            ..default()
        },
        TextColor(Color::srgb(
            srgba.red.max(0.4),
            srgba.green.max(0.6),
            srgba.blue.max(0.3),
        )),
        TextLayout::new_with_justify(Justify::Center),
        Pickable::IGNORE,
    ));
}

/// Tick resource popup lifetimes, animate font size, fade out, and despawn when done.
pub(super) fn update_resource_popups(
    mut commands: Commands,
    time: Res<Time>,
    mut popups: Query<(Entity, &mut ResourcePopup, &mut TextColor, &mut TextFont)>,
) {
    for (entity, mut popup, mut color, mut font) in &mut popups {
        popup.lifetime.tick(time.delta());
        let frac = popup.lifetime.fraction();

        // Pop-in scale animation
        let scale = if frac < 0.1 {
            0.5 + (frac / 0.1) * 0.7
        } else if frac < 0.25 {
            1.2 - ((frac - 0.1) / 0.15) * 0.2
        } else {
            1.0
        };
        font.font_size = theme::FONT_LARGE * scale;

        // Rise in world Y
        popup.world_pos.y += 30.0 * time.delta_secs() * 0.02;

        // Fade out in the last 40%
        let alpha = if frac > 0.6 {
            1.0 - (frac - 0.6) / 0.4
        } else {
            1.0
        };
        let base = color.0.to_srgba();
        color.0 = Color::srgba(base.red, base.green, base.blue, alpha);

        if popup.lifetime.is_finished() {
            commands.entity(entity).try_despawn();
        }
    }
}

/// Try to send a worker to the nearest depot, or enter a blocked state if none found.
/// Returns the new state to assign.
fn return_to_depot_or_wait(
    commands: &mut Commands,
    entity: Entity,
    pos: &Vec3,
    faction: &Faction,
    carrying: &Carrying,
    gather_node: Option<Entity>,
    deposit_points: &Query<
        (
            Entity,
            &Transform,
            &BuildingState,
            &Faction,
            &EntityKind,
            &BuildingFootprint,
            Option<&NetworkId>,
        ),
        (With<DepositPoint>, Without<Unit>),
    >,
    inventories: Option<
        &Query<(&Transform, Option<&mut StorageInventory>), (With<DepositPoint>, Without<Unit>)>,
    >,
) -> UnitState {
    if carrying.amount > 0 {
        if let Some(depot) = find_nearest_deposit_for(
            pos,
            faction,
            carrying.resource_type,
            deposit_points,
            inventories,
        ) {
            let (_, depot_tf, _, _, depot_kind, depot_fp, _) = deposit_points.get(depot).unwrap();
            let target =
                building_worker_interaction_target(*pos, depot_tf, depot_fp.0, *depot_kind);
            commands.entity(entity).insert(MoveTarget(target.position));
            return UnitState::ReturningToDeposit { depot, gather_node };
        }
        commands.entity(entity).remove::<MoveTarget>();
        return UnitState::WaitingForDepot { gather_node };
    }
    UnitState::Idle
}

/// Scan for the nearest resource node within range.
/// When `faction` is provided, skips nodes whose resource type has no
/// accepting depot — this prevents workers from gathering resources they
/// cannot deposit, which previously caused them to get stuck carrying.
fn find_nearest_node(
    pos: &Vec3,
    range: f32,
    all_nodes: &Query<
        (
            Entity,
            &Transform,
            Option<&YardResourceNode>,
            Option<&NetworkId>,
        ),
        (With<ResourceNode>, Without<Unit>),
    >,
    node_data_q: &Query<(&Transform, &mut ResourceNode, Option<&YardResourceNode>), Without<Unit>>,
    faction: Option<&Faction>,
    deposit_points: Option<
        &Query<
            (
                Entity,
                &Transform,
                &BuildingState,
                &Faction,
                &EntityKind,
                &BuildingFootprint,
                Option<&NetworkId>,
            ),
            (With<DepositPoint>, Without<Unit>),
        >,
    >,
    inventories: Option<
        &Query<(&Transform, Option<&mut StorageInventory>), (With<DepositPoint>, Without<Unit>)>,
    >,
) -> Option<Entity> {
    let mut candidates: Vec<(i32, u32, Entity)> = Vec::new();
    for (node_entity, node_tf, yard_tag, node_net_id) in all_nodes {
        if yard_tag.is_some() {
            continue;
        }
        let dist = pos.distance(node_tf.translation);
        if dist >= range {
            continue;
        }
        // When depot info is available, skip resource types with no accepting depot
        if let (Some(f), Some(dp)) = (faction, deposit_points) {
            if let Ok((_, node_data, _)) = node_data_q.get(node_entity) {
                if find_nearest_deposit_for(pos, f, Some(node_data.resource_type), dp, inventories)
                    .is_none()
                {
                    continue;
                }
            }
        }
        debug_assert!(
            node_net_id.is_some(),
            "ResourceNode missing NetworkId during worker scan"
        );
        let net_id = node_net_id.map(|id| id.0).unwrap_or(u32::MAX);
        candidates.push((quantize_distance(dist), net_id, node_entity));
    }
    candidates.sort_by_key(|&(q, id, _)| (q, id));
    candidates.first().map(|&(_, _, e)| e)
}

/// Unbounded scan for the nearest node of a specific resource type, filtered
/// to types with an accepting depot. Used by workers with `PreferredResource`.
fn find_nearest_node_of_type(
    pos: &Vec3,
    target: ResourceType,
    all_nodes: &Query<
        (
            Entity,
            &Transform,
            Option<&YardResourceNode>,
            Option<&NetworkId>,
        ),
        (With<ResourceNode>, Without<Unit>),
    >,
    node_data_q: &Query<(&Transform, &mut ResourceNode, Option<&YardResourceNode>), Without<Unit>>,
    faction: &Faction,
    deposit_points: &Query<
        (
            Entity,
            &Transform,
            &BuildingState,
            &Faction,
            &EntityKind,
            &BuildingFootprint,
            Option<&NetworkId>,
        ),
        (With<DepositPoint>, Without<Unit>),
    >,
    inventories: &Query<
        (&Transform, Option<&mut StorageInventory>),
        (With<DepositPoint>, Without<Unit>),
    >,
) -> Option<Entity> {
    let mut candidates: Vec<(i32, u32, Entity)> = Vec::new();
    for (node_entity, node_tf, yard_tag, node_net_id) in all_nodes {
        if yard_tag.is_some() {
            continue;
        }
        let Ok((_, node_data, _)) = node_data_q.get(node_entity) else {
            continue;
        };
        if node_data.resource_type != target || node_data.amount_remaining == 0 {
            continue;
        }
        if find_nearest_deposit_for(
            pos,
            faction,
            Some(target),
            deposit_points,
            Some(inventories),
        )
        .is_none()
        {
            continue;
        }
        let dist = pos.distance(node_tf.translation);
        debug_assert!(
            node_net_id.is_some(),
            "ResourceNode missing NetworkId during worker scan"
        );
        let net_id = node_net_id.map(|id| id.0).unwrap_or(u32::MAX);
        candidates.push((quantize_distance(dist), net_id, node_entity));
    }
    candidates.sort_by_key(|&(q, id, _)| (q, id));
    candidates.first().map(|&(_, _, e)| e)
}

/// Spawn a short burst of gather particles when a worker extracts resources from a node.
pub(super) fn spawn_gather_vfx(
    commands: &mut Commands,
    vfx: &VfxAssets,
    worker_pos: Vec3,
    node_pos: Vec3,
    rt: ResourceType,
    amount: u32,
) {
    let mesh = match rt {
        ResourceType::Wood => vfx.cube_mesh.clone(),
        _ => vfx.sphere_mesh.clone(),
    };
    let material = vfx
        .resource_particle_materials
        .get(&rt)
        .cloned()
        .unwrap_or_else(|| vfx.impact_material.clone());
    let base_count = match rt {
        ResourceType::Wood => 2,
        ResourceType::Copper => 3,
        ResourceType::Iron => 3,
        ResourceType::Gold => 4,
        ResourceType::Oil => 2,
        // Processed resources use moderate particle counts
        _ => 3,
    };
    let count = (base_count + amount.min(4)).min(8).max(1);
    let base_scale = match rt {
        ResourceType::Wood => 0.09,
        ResourceType::Oil => 0.07,
        ResourceType::Gold => 0.08,
        _ => 0.075,
    };
    let toward_worker = (worker_pos - node_pos).normalize_or_zero();

    for i in 0..count {
        let frac = i as f32 / count as f32;
        let angle = std::f32::consts::TAU * frac;
        let lateral = Vec3::new(angle.cos() * 0.35, 0.0, angle.sin() * 0.35);
        let jitter = Vec3::new(
            (frac * 11.3).sin() * 0.12,
            0.2 + frac * 0.18,
            (frac * 7.7).cos() * 0.12,
        );
        let velocity =
            toward_worker * (1.1 + frac * 0.9) + lateral * 0.9 + Vec3::Y * (1.2 + frac * 0.8);

        commands.spawn((
            GatherParticle {
                timer: Timer::from_seconds(0.42, TimerMode::Once),
                velocity,
                start_scale: base_scale * (1.0 + frac * 0.35),
            },
            FogHideable::Vfx,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(node_pos + Vec3::Y * 0.7 + jitter)
                .with_scale(Vec3::splat(base_scale)),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

/// Spawn deposit VFX particles around a position.
pub(super) fn spawn_deposit_vfx(
    commands: &mut Commands,
    vfx: &VfxAssets,
    deposit_pos: Vec3,
    count: u32,
    scale: f32,
    duration: f32,
) {
    for i in 0..count {
        let angle = std::f32::consts::TAU * (i as f32 / count as f32);
        let offset = Vec3::new(angle.cos() * 0.5, 0.5, angle.sin() * 0.5);
        commands.spawn((
            VfxFlash {
                timer: Timer::from_seconds(duration, TimerMode::Once),
                start_scale: scale,
                end_scale: 0.0,
                rise_speed: 0.5,
            },
            FogHideable::Vfx,
            Mesh3d(vfx.sphere_mesh.clone()),
            MeshMaterial3d(vfx.deposit_material.clone()),
            Transform::from_translation(deposit_pos + offset).with_scale(Vec3::splat(scale)),
        ));
    }
}

/// Compute a point on the building edge facing the worker, so pathfinding
/// targets the building perimeter instead of its center.
pub(super) fn depot_approach_point(worker_pos: Vec3, depot_pos: Vec3, footprint: f32) -> Vec3 {
    let dir = worker_pos - depot_pos;
    let flat = Vec3::new(dir.x, 0.0, dir.z);
    let len = flat.length();
    if len < 0.1 {
        // Worker nearly on top of building — pick an arbitrary offset
        return depot_pos + Vec3::new(footprint + 0.5, 0.0, 0.0);
    }
    // Target a point just outside the footprint radius
    depot_pos + flat / len * (footprint + 0.5)
}

/// Find nearest deposit that accepts the given resource type.
/// If `resource_type` is Some, only depots that accept that type AND have remaining capacity are returned.
/// If `inventories` is None, capacity checks are skipped.
pub(super) fn find_nearest_deposit_for(
    pos: &Vec3,
    faction: &Faction,
    resource_type: Option<ResourceType>,
    deposit_points: &Query<
        (
            Entity,
            &Transform,
            &BuildingState,
            &Faction,
            &EntityKind,
            &BuildingFootprint,
            Option<&NetworkId>,
        ),
        (With<DepositPoint>, Without<Unit>),
    >,
    inventories: Option<
        &Query<(&Transform, Option<&mut StorageInventory>), (With<DepositPoint>, Without<Unit>)>,
    >,
) -> Option<Entity> {
    let mut closest: Option<(Entity, f32, u32)> = None;
    for (entity, tf, state, depot_faction, _, _, net_id) in deposit_points {
        if *state != BuildingState::Complete || depot_faction != faction {
            continue;
        }
        // If we have a resource type, check that the depot accepts it and has capacity
        if let Some(rt) = resource_type {
            if let Some(inv_query) = inventories {
                if let Ok((_, inv_opt)) = inv_query.get(entity) {
                    if let Some(inv) = inv_opt {
                        if !inv.accepts(rt) || inv.remaining_capacity_for(rt) == 0 {
                            continue;
                        }
                    }
                }
            }
        }
        let dist = pos.distance(tf.translation);
        let candidate = (entity, dist, net_id.map(|id| id.0).unwrap_or(u32::MAX));
        if closest.as_ref().is_none_or(|current| {
            worker_candidate_cmp(candidate.1, candidate.2, current.1, current.2).is_lt()
        }) {
            closest = Some(candidate);
        }
    }
    closest.map(|(entity, _, _)| entity)
}

fn worker_candidate_cmp(
    left_dist: f32,
    left_id: u32,
    right_dist: f32,
    right_id: u32,
) -> std::cmp::Ordering {
    left_dist
        .total_cmp(&right_dist)
        .then_with(|| left_id.cmp(&right_id))
}

/// Quantize a distance in world units to 1 mm precision as an `i32`. Matches
/// the checksum quantization so that worker picks sort identically across
/// peers even when floating-point paths differ by a ULP.
#[inline]
fn quantize_distance(dist: f32) -> i32 {
    (dist * 1000.0) as i32
}

pub(super) fn building_worker_interaction_target(
    worker_pos: Vec3,
    building_tf: &Transform,
    footprint: f32,
    kind: EntityKind,
) -> WorkerInteractionTarget {
    if kind == EntityKind::Sawmill {
        let rotation_y = crate::simulation::buildings::rotation_y_from_quat(building_tf.rotation);
        return WorkerInteractionTarget {
            position: crate::simulation::buildings::sawmill_yard_center(
                building_tf.translation,
                rotation_y,
            ),
            arrive_radius: 2.5,
        };
    }

    WorkerInteractionTarget {
        position: depot_approach_point(worker_pos, building_tf.translation, footprint),
        arrive_radius: 2.5,
    }
}
