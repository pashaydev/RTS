use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use game_state::message::{ClientMessage, InputCommand, PlayerInput, ServerMessage};

use crate::blueprints::{BlueprintRegistry, EntityKind};
use crate::camera;
use crate::components::*;
use crate::ground::HeightMap;
use crate::minimap::MinimapInteraction;
use crate::multiplayer::host_systems::execute_input_command;
use crate::multiplayer::{ClientNetState, HostNetState, NetRole};
use crate::net_bridge::EntityNetMap;
use crate::audio::{PlaySfx, SfxKind};
use crate::combat::{
    apply_manual_attack_intent, apply_manual_attack_move_intent, apply_manual_hold_intent,
    apply_manual_move_intent, clear_combat_intent,
};

use super::picking::{ray_aabb_dist, ray_sphere_dist};
use super::{enqueue_task, clear_task_queue, set_current_task};

// ── Right-click move / contextual actions ──

pub(crate) fn handle_right_click_move(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_task_id: ResMut<NextTaskId>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
    ),
    selected_units: Query<
        (Entity, &EntityKind, &Faction, &Transform),
        (With<Unit>, With<Selected>),
    >,
    mut task_queues: Query<&mut TaskQueue, With<Unit>>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    formation: Res<ActiveFormation>,
    pickables: Query<(Entity, &GlobalTransform, &PickRadius, &InheritedVisibility)>,
    target_queries: (
        Query<Entity, With<Mob>>,
        Query<Entity, With<ResourceNode>>,
        Query<(Entity, &GlobalTransform), (With<Building>, With<ConstructionProgress>)>,
        Query<(Entity, &ResourceProcessor, &BuildingState, &Faction), With<Building>>,
    ),
    // Queries for contextual right-click: enemy units, all units with faction, buildings with faction
    enemy_detect: (
        Query<(Entity, &Faction), (With<Unit>, Without<Selected>)>,
        Query<(Entity, &Faction, &BuildingState), (With<Building>, Without<ConstructionProgress>)>,
    ),
    picking_extra: (
        Query<&AssignedWorkers>,
        Query<(&BuildingFootprint, &BuildingHeight), With<Building>>,
        Res<HeightMap>,
        Res<GraphicsSettings>,
    ),
    ui_flags: (
        Res<MinimapInteraction>,
        Res<UiClickedThisFrame>,
        Res<UiPressActive>,
        bevy::ecs::message::MessageWriter<PlaySfx>,
    ),
    net_params: (
        Res<NetRole>,
        Option<Res<ClientNetState>>,
        Option<Res<HostNetState>>,
        Res<BlueprintRegistry>,
        Option<Res<EntityNetMap>>,
        Res<Time>,
        Query<&mut UnitState>,
        Query<(&mut TrainingQueue, &EntityKind, Option<&BuildingLevel>), With<Building>>,
        Query<&GlobalTransform>,
        Option<ResMut<bevy_matchbox::prelude::MatchboxSocket>>,
    ),
) {
    let (camera_q, windows) = viewport;
    let (mobs, resource_nodes, construction_q, processor_buildings) =
        target_queries;
    let (other_units, other_buildings) = enemy_detect;
    let (assigned_workers_q, building_aabb_q, height_map, graphics) = picking_extra;
    let (minimap_interaction, ui_clicked, ui_press, mut sfx) = ui_flags;
    let (
        net_role,
        client_net,
        host_net,
        registry,
        net_map,
        time,
        mut unit_states_q,
        mut training_buildings,
        all_transforms,
        mut matchbox_socket,
    ) = net_params;

    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }

    if minimap_interaction.clicked || ui_clicked.0 > 0 || ui_press.0 {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return;
    };
    let Some(ray) = camera::viewport_ray_from_window_cursor(camera, cam_gt, window, &graphics) else {
        return;
    };

    let units_vec: Vec<(Entity, EntityKind)> = selected_units
        .iter()
        .filter(|(_, _, faction, _)| **faction == active_player.0)
        .map(|(e, k, _, _)| (e, *k))
        .collect();
    if units_vec.is_empty() {
        return;
    }

    // Contextual right-click action types
    #[derive(Clone, Copy, PartialEq)]
    enum RClickAction {
        AttackEnemy,     // enemy unit or mob
        GatherResource,  // resource node (workers)
        AssistBuild,     // construction site (workers)
        AssignProcessor, // processor building (workers)
        MoveToAlly,      // allied building — just move near it
    }

    // Find ALL hits and evaluate by depth + priority
    struct RClickHit {
        entity: Entity,
        dist: f32,
        action: RClickAction,
    }

    let mut hits: Vec<RClickHit> = Vec::new();
    for (entity, gt, pick_r, inherited_vis) in &pickables {
        if !inherited_vis.get() {
            continue;
        }

        let center = gt.translation();
        let dist = if let Ok((footprint, bld_h)) = building_aabb_q.get(entity) {
            let terrain_y = height_map.sample(center.x, center.z);
            ray_aabb_dist(&ray, center, footprint.0, terrain_y, terrain_y + bld_h.0)
        } else {
            ray_sphere_dist(&ray, center, pick_r.0)
        };
        let Some(dist) = dist else {
            continue;
        };

        // Determine the best action for this hit
        let action = if mobs.contains(entity) {
            // Mobs are always hostile
            Some(RClickAction::AttackEnemy)
        } else if let Ok((_, unit_faction)) = other_units.get(entity) {
            // Another unit — attack if hostile, ignore if allied (can't interact)
            if teams.is_hostile(&active_player.0, unit_faction) {
                Some(RClickAction::AttackEnemy)
            } else {
                None // Allied unit — no right-click action
            }
        } else if construction_q.contains(entity) {
            Some(RClickAction::AssistBuild)
        } else if processor_buildings.contains(entity) {
            // Check if it's our processor or enemy building
            if let Ok((_, _, _, proc_faction)) = processor_buildings.get(entity) {
                if teams.is_hostile(&active_player.0, proc_faction) {
                    Some(RClickAction::AttackEnemy)
                } else {
                    Some(RClickAction::AssignProcessor)
                }
            } else {
                None
            }
        } else if resource_nodes.contains(entity) {
            Some(RClickAction::GatherResource)
        } else if let Ok((_, bld_faction, bld_state)) = other_buildings.get(entity) {
            // Completed building — attack if hostile, move-to if allied
            if *bld_state != BuildingState::Complete {
                None
            } else if teams.is_hostile(&active_player.0, bld_faction) {
                Some(RClickAction::AttackEnemy)
            } else {
                Some(RClickAction::MoveToAlly)
            }
        } else {
            None
        };

        if let Some(action) = action {
            hits.push(RClickHit {
                entity,
                dist,
                action,
            });
        }
    }

    // Sort hits by depth
    hits.sort_by(|a, b| {
        a.dist
            .partial_cmp(&b.dist)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let target_action = if !hits.is_empty() {
        let closest_dist = hits[0].dist;
        let threshold = closest_dist + 2.0;
        let close_hits: Vec<_> = hits.into_iter().filter(|h| h.dist <= threshold).collect();

        // Priority tie-breaker: attack enemy > resource > construction > processor > ally
        if let Some(h) = close_hits
            .iter()
            .find(|h| h.action == RClickAction::AttackEnemy)
        {
            Some((h.entity, h.action))
        }  else if let Some(h) = close_hits
            .iter()
            .find(|h| h.action == RClickAction::GatherResource)
        {
            Some((h.entity, h.action))
        } else if let Some(h) = close_hits
            .iter()
            .find(|h| h.action == RClickAction::AssistBuild)
        {
            Some((h.entity, h.action))
        } else if let Some(h) = close_hits
            .iter()
            .find(|h| h.action == RClickAction::AssignProcessor)
        {
            Some((h.entity, h.action))
        } else if let Some(h) = close_hits
            .iter()
            .find(|h| h.action == RClickAction::MoveToAlly)
        {
            Some((h.entity, h.action))
        } else {
            None
        }
    } else {
        None
    };

    let ground_point = height_map.raycast(ray);

    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    let input_player_id = client_net
        .as_ref()
        .map(|client| client.player_id as u32)
        .unwrap_or(0);

    // Build one network input packet for right-click actions supported by current relay path.
    let make_network_input = || -> Option<PlayerInput> {
        let net_map = net_map.as_ref()?;
        let entity_ids: Vec<u32> = units_vec
            .iter()
            .filter_map(|(entity, _)| net_map.to_net.get(entity).copied())
            .collect();
        if entity_ids.is_empty() {
            return None;
        }

        let mut commands = Vec::new();
        match target_action {
            Some((target_entity, RClickAction::AttackEnemy)) => {
                let target_id = *net_map.to_net.get(&target_entity)?;
                commands.push(InputCommand::Attack { target_id });
            }
            Some((target_entity, RClickAction::GatherResource)) => {
                let target_id = *net_map.to_net.get(&target_entity)?;
                commands.push(InputCommand::Gather { target_id });
            }
            Some(_) => {
                // Unsupported complex right-click action for network relay yet.
                return None;
            }
            None => {
                let point = ground_point?;
                commands.push(InputCommand::Move {
                    target: [point.x, point.y, point.z],
                    formation: Some(formation.formation.to_net_u8()),
                });
            }
        }

        Some(PlayerInput {
            player_id: input_player_id,
            tick: 0,
            entity_ids,
            commands,
        })
    };

    if *net_role == NetRole::Client {
        if let (Some(client), Some(ref mut socket)) =
            (client_net.as_ref(), matchbox_socket.as_mut())
        {
            if let Some(input) = make_network_input() {
                let seq = {
                    let mut s = client.seq.lock().unwrap();
                    *s += 1;
                    *s
                };
                let msg = ClientMessage::Input {
                    seq,
                    timestamp: time.elapsed_secs_f64(),
                    input,
                };
                crate::multiplayer::matchbox_transport::send_to_host(socket, &msg);
            }
        }
        // Client no longer mutates local gameplay state directly; host relays supported inputs.
        return;
    }

    if *net_role == NetRole::Host {
        if let Some(host) = host_net.as_ref() {
            if let Some(input) = make_network_input() {
                let seq = {
                    let mut s = host.seq.lock().unwrap();
                    *s += 1;
                    *s
                };
                let relay = ServerMessage::RelayedInput {
                    seq,
                    timestamp: time.elapsed_secs_f64(),
                    player_id: 0,
                    input: input.clone(),
                };
                if let Some(ref mut socket) = matchbox_socket {
                    crate::multiplayer::matchbox_transport::broadcast_reliable(socket, &relay);
                }

                // Execute locally through the same code path as client commands,
                // so host and clients have identical component setup.
                if let Some(ref nm) = net_map {
                    execute_input_command(
                        &mut commands,
                        &input,
                        time.elapsed_secs_f64(),
                        nm,
                        &mut unit_states_q,
                        &mut task_queues,
                        &mut training_buildings,
                        &mut next_task_id,
                        &all_transforms,
                        &registry,
                    );
                }
                return;
            }
        }
    }

    if let Some((target_entity, action)) = target_action {
        match action {
            RClickAction::AttackEnemy => {
                for (entity, _kind) in &units_vec {
                    if shift {
                        enqueue_task(
                            &mut task_queues,
                            &mut next_task_id,
                            *entity,
                            QueuedTask::Attack(target_entity),
                        );
                    } else {
                        apply_manual_attack_intent(
                            &mut commands,
                            *entity,
                            target_entity,
                            time.elapsed_secs_f64(),
                        );
                        let mut ec = commands.entity(*entity);
                        ec.remove::<MoveTarget>()
                            .insert(AttackTarget(target_entity))
                            .insert(UnitState::Attacking(target_entity));
                        set_current_task(
                            &mut task_queues,
                            &mut next_task_id,
                            *entity,
                            QueuedTask::Attack(target_entity),
                        );
                    }
                }
                sfx.write(PlaySfx { kind: SfxKind::UnitAttack, position: None });
            }
            RClickAction::AssignProcessor => {
                if let Ok((_, processor, state, proc_faction)) =
                    processor_buildings.get(target_entity)
                {
                    if *state == BuildingState::Complete
                        && *proc_faction == active_player.0
                        && processor.max_workers > 0
                    {
                        let current_count = assigned_workers_q
                            .get(target_entity)
                            .map(|aw| aw.workers.len())
                            .unwrap_or(0);
                        let mut assigned = 0;
                        for (entity, kind) in &units_vec {
                            if *kind == EntityKind::Worker
                                && current_count + assigned < processor.max_workers as usize
                            {
                                if shift {
                                    enqueue_task(
                                        &mut task_queues,
                                        &mut next_task_id,
                                        *entity,
                                        QueuedTask::AssignToProcessor(target_entity),
                                    );
                                } else {
                                    if let Ok(gt) =
                                        pickables.get(target_entity).map(|(_, gt, _, _)| gt)
                                    {
                                        clear_combat_intent(
                                            &mut commands,
                                            *entity,
                                            time.elapsed_secs_f64(),
                                        );
                                        commands
                                            .entity(*entity)
                                            .remove::<AttackTarget>()
                                            .insert(MoveTarget(gt.translation()))
                                            .insert(UnitState::AssignedGathering {
                                                building: target_entity,
                                                phase: AssignedPhase::SeekingNode,
                                            })
                                            .insert(BuildingAssignment(target_entity))
                                            .insert(TaskSource::Manual);
                                        set_current_task(
                                            &mut task_queues,
                                            &mut next_task_id,
                                            *entity,
                                            QueuedTask::AssignToProcessor(target_entity),
                                        );
                                    }
                                }
                                assigned += 1;
                            } else if let Ok(gt) =
                                pickables.get(target_entity).map(|(_, gt, _, _)| gt)
                            {
                                apply_manual_move_intent(
                                    &mut commands,
                                    *entity,
                                    gt.translation(),
                                    time.elapsed_secs_f64(),
                                );
                                commands
                                    .entity(*entity)
                                    .remove::<AttackTarget>()
                                    .insert(MoveTarget(gt.translation()))
                                    .insert(UnitState::Moving(gt.translation()))
                                    .insert(TaskSource::Manual);
                            }
                        }
                    }
                }
            }
            RClickAction::AssistBuild => {
                let construction_pos = pickables
                    .get(target_entity)
                    .map(|(_, gt, _, _)| gt.translation());
                for (entity, kind) in &units_vec {
                    if *kind == EntityKind::Worker {
                        if shift {
                            enqueue_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Build(target_entity),
                            );
                        } else {
                            clear_combat_intent(
                                &mut commands,
                                *entity,
                                time.elapsed_secs_f64(),
                            );
                            commands
                                .entity(*entity)
                                .remove::<AttackTarget>()
                                .remove::<MoveTarget>()
                                .insert(UnitState::MovingToBuild(target_entity))
                                .insert(TaskSource::Manual);
                            set_current_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Build(target_entity),
                            );
                        }
                    } else if let Ok(pos) = construction_pos {
                        apply_manual_move_intent(
                            &mut commands,
                            *entity,
                            pos,
                            time.elapsed_secs_f64(),
                        );
                        commands
                            .entity(*entity)
                            .remove::<AttackTarget>()
                            .insert(MoveTarget(pos))
                            .insert(UnitState::Moving(pos))
                            .insert(TaskSource::Manual);
                    }
                }
            }
            RClickAction::GatherResource => {
                for (entity, kind) in &units_vec {
                    if *kind == EntityKind::Worker {
                        if shift {
                            enqueue_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Gather(target_entity),
                            );
                        } else {
                            if let Ok(gt) = pickables.get(target_entity).map(|(_, gt, _, _)| gt) {
                                clear_combat_intent(
                                    &mut commands,
                                    *entity,
                                    time.elapsed_secs_f64(),
                                );
                                commands
                                    .entity(*entity)
                                    .remove::<AttackTarget>()
                                    .insert(MoveTarget(gt.translation()))
                                    .insert(UnitState::Gathering(target_entity))
                                    .insert(TaskSource::Manual);
                                set_current_task(
                                    &mut task_queues,
                                    &mut next_task_id,
                                    *entity,
                                    QueuedTask::Gather(target_entity),
                                );
                            }
                        }
                    } else if let Ok(gt) = pickables.get(target_entity).map(|(_, gt, _, _)| gt) {
                        apply_manual_move_intent(
                            &mut commands,
                            *entity,
                            gt.translation(),
                            time.elapsed_secs_f64(),
                        );
                        commands
                            .entity(*entity)
                            .remove::<AttackTarget>()
                            .insert(MoveTarget(gt.translation()))
                            .insert(UnitState::Moving(gt.translation()))
                            .insert(TaskSource::Manual);
                    }
                }
            }
            RClickAction::MoveToAlly => {
                // Move units toward the allied building
                if let Ok(gt) = pickables.get(target_entity).map(|(_, gt, _, _)| gt) {
                    let pos = gt.translation();
                    let n = units_vec.len();
                    let spacing = 1.5;
                    let radius = if n > 1 {
                        (spacing * n as f32 / std::f32::consts::TAU).max(1.0)
                    } else {
                        0.0
                    };
                    for (i, (entity, _kind)) in units_vec.iter().enumerate() {
                        let offset = if n > 1 {
                            let angle = i as f32 / n as f32 * std::f32::consts::TAU;
                            Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius)
                        } else {
                            Vec3::ZERO
                        };
                        let dest = pos + offset;
                        if shift {
                            enqueue_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Move(dest),
                            );
                        } else {
                            apply_manual_move_intent(
                                &mut commands,
                                *entity,
                                dest,
                                time.elapsed_secs_f64(),
                            );
                            commands
                                .entity(*entity)
                                .remove::<AttackTarget>()
                                .insert(MoveTarget(dest))
                                .insert(UnitState::Moving(dest))
                                .insert(TaskSource::Manual);
                            set_current_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Move(dest),
                            );
                        }
                    }
                }
            }
        }
    } else {
        if let Some(point) = height_map.raycast(ray) {

            // Ground fallback check for clicking slightly outside construction bounds
            let nearby_construction_radius = 5.0;
            let mut nearest_site: Option<(Entity, f32)> = None;
            for (site_entity, site_gt) in &construction_q {
                let site_pos = site_gt.translation();
                let d = point.distance(Vec3::new(site_pos.x, point.y, site_pos.z));
                if d < nearby_construction_radius {
                    if nearest_site.is_none() || d < nearest_site.unwrap().1 {
                        nearest_site = Some((site_entity, d));
                    }
                }
            }

            let has_workers = units_vec.iter().any(|(_, k)| *k == EntityKind::Worker);
            if let Some((site_entity, _)) = nearest_site.filter(|_| has_workers) {
                for (entity, kind) in &units_vec {
                    if *kind == EntityKind::Worker {
                        if shift {
                            enqueue_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Build(site_entity),
                            );
                        } else {
                            clear_combat_intent(
                                &mut commands,
                                *entity,
                                time.elapsed_secs_f64(),
                            );
                            commands
                                .entity(*entity)
                                .remove::<AttackTarget>()
                                .remove::<MoveTarget>()
                                .insert(UnitState::MovingToBuild(site_entity))
                                .insert(TaskSource::Manual);
                            set_current_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Build(site_entity),
                            );
                        }
                    } else {
                        apply_manual_move_intent(
                            &mut commands,
                            *entity,
                            point,
                            time.elapsed_secs_f64(),
                        );
                        commands
                            .entity(*entity)
                            .remove::<AttackTarget>()
                            .insert(MoveTarget(point))
                            .insert(UnitState::Moving(point))
                            .insert(TaskSource::Manual);
                    }
                }
            } else {
                // Ground move
                let n = units_vec.len();
                if n == 1 {
                    let (ent, _kind) = &units_vec[0];
                    if shift {
                        enqueue_task(
                            &mut task_queues,
                            &mut next_task_id,
                            *ent,
                            QueuedTask::Move(point),
                        );
                    } else {
                        apply_manual_move_intent(
                            &mut commands,
                            *ent,
                            point,
                            time.elapsed_secs_f64(),
                        );
                        commands
                            .entity(*ent)
                            .remove::<AttackTarget>()
                            .insert(MoveTarget(point))
                            .insert(UnitState::Moving(point))
                            .insert(TaskSource::Manual);
                        set_current_task(
                            &mut task_queues,
                            &mut next_task_id,
                            *ent,
                            QueuedTask::Move(point),
                        );
                    }
                } else if n > 1 {
                    // Calculate centroid of selected units
                    let centroid = selected_units
                        .iter()
                        .filter(|(_, _, f, _)| **f == active_player.0)
                        .map(|(_, _, _, tf)| tf.translation)
                        .fold(Vec3::ZERO, |a, b| a + b)
                        / n as f32;
                    let facing =
                        Vec2::new(point.x - centroid.x, point.z - centroid.z).normalize_or_zero();

                    let offsets = formation_offsets(formation.formation, n, facing);

                    for (i, (entity, _kind)) in units_vec.iter().enumerate() {
                        let off = offsets.get(i).copied().unwrap_or(Vec2::ZERO);
                        let dest = point + Vec3::new(off.x, 0.0, off.y);
                        if shift {
                            enqueue_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Move(dest),
                            );
                        } else {
                            apply_manual_move_intent(
                                &mut commands,
                                *entity,
                                dest,
                                time.elapsed_secs_f64(),
                            );
                            commands
                                .entity(*entity)
                                .remove::<AttackTarget>()
                                .insert(MoveTarget(dest))
                                .insert(UnitState::Moving(dest))
                                .insert(TaskSource::Manual);
                            set_current_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Move(dest),
                            );
                        }
                    }
                }
                sfx.write(PlaySfx { kind: SfxKind::UnitMove, position: Some(point) });
            }
        }
    }
}

// ── Hotkey-based unit commands ──

/// Hotkey-based unit commands:
/// - `A` → enter attack-move mode (next left-click issues attack-move)
/// - `P` → enter patrol mode (next left-click issues patrol to position)
/// - `H` → hold position (instant, clears move/attack targets)
/// - `S` → stop (instant, clears all orders)
/// - `X` → scuttle selected workers (instant self-destruct)
/// - `Escape` → cancel command mode
///
/// In attack-move/patrol mode, left-click on ground executes the command.
pub(crate) fn handle_unit_command_hotkeys(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cmd_mode: ResMut<CommandMode>,
    mut next_task_id: ResMut<NextTaskId>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    selected_units: Query<(Entity, &EntityKind, &Faction), (With<Unit>, With<Selected>)>,
    mut task_queues: Query<&mut TaskQueue, With<Unit>>,
    mut unit_abilities: Query<&mut UnitAbilities>,
    active_player: Res<ActivePlayer>,
    mut formation: ResMut<ActiveFormation>,
    time: Res<Time>,
    ui_clicked: Res<UiClickedThisFrame>,
    ui_press: Res<UiPressActive>,
    placement: Res<BuildingPlacementState>,
) {
    let (ref camera_q, ref windows, ref graphics) = viewport;
    if placement.mode != PlacementMode::None {
        return;
    }

    let has_selected = selected_units.iter().any(|(_, _, f)| *f == active_player.0);

    // Escape cancels command mode
    if keys.just_pressed(KeyCode::Escape) {
        *cmd_mode = CommandMode::Normal;
        return;
    }

    if has_selected {
        // --- 1. Instant Commands (Hold & Stop) ---
        if keys.just_pressed(KeyCode::KeyH) {
            for (entity, _, faction) in &selected_units {
                if *faction != active_player.0 {
                    continue;
                }
                apply_manual_hold_intent(&mut commands, entity, time.elapsed_secs_f64());
                commands
                    .entity(entity)
                    .remove::<MoveTarget>()
                    .remove::<AttackTarget>()
                    .insert(UnitState::HoldPosition)
                    .insert(TaskSource::Manual);
                set_current_task(
                    &mut task_queues,
                    &mut next_task_id,
                    entity,
                    QueuedTask::HoldPosition,
                );
            }
            *cmd_mode = CommandMode::Normal;
            return;
        }

        if keys.just_pressed(KeyCode::KeyX) {
            for (entity, _kind, faction) in &selected_units {
                if *faction != active_player.0 {
                    continue;
                }
                clear_combat_intent(&mut commands, entity, time.elapsed_secs_f64());
                commands
                    .entity(entity)
                    .remove::<MoveTarget>()
                    .remove::<AttackTarget>()
                    .insert(UnitState::Idle)
                    .insert(TaskSource::Auto);
                clear_task_queue(&mut task_queues, entity);
            }
            *cmd_mode = CommandMode::Normal;
            return;
        }

        // --- 2. Stance Cycle (V) ---
        if keys.just_pressed(KeyCode::KeyV) {
            for (entity, _, faction) in &selected_units {
                if *faction != active_player.0 {
                    continue;
                }
                commands
                    .entity(entity)
                    .entry::<UnitStance>()
                    .and_modify(|mut stance| {
                        *stance = stance.cycle();
                    });
            }
            return;
        }

        // --- Formation toggle (G) ---
        if keys.just_pressed(KeyCode::KeyG) {
            formation.formation = formation.formation.cycle();
            return;
        }

        // --- 3. Enter Command Modes ---
        if keys.just_pressed(KeyCode::KeyF) {
            *cmd_mode = CommandMode::AttackMove;
            return;
        }

        if keys.just_pressed(KeyCode::KeyP) {
            *cmd_mode = CommandMode::Patrol;
            return;
        }

        // --- Ability hotkeys (Q = first ability, W = second ability) ---
        let ability_hotkey = if keys.just_pressed(KeyCode::KeyQ) {
            Some(0usize)
        } else if keys.just_pressed(KeyCode::KeyW) {
            Some(1usize)
        } else {
            None
        };
        if let Some(slot) = ability_hotkey {
            // Find first selected unit with abilities
            for (entity, _kind, faction) in &selected_units {
                if *faction != active_player.0 {
                    continue;
                }
                if let Ok(abilities) = unit_abilities.get(entity) {
                    if let Some(&ability) = abilities.abilities.get(slot) {
                        if ability.targeting() == AbilityTargeting::NoTarget {
                            // Execute immediately
                            if let Ok(mut ab) = unit_abilities.get_mut(entity) {
                                if ab.is_ready(ability) {
                                    ab.trigger_cooldown(ability);
                                    commands.entity(entity).insert(CastingAbility {
                                        ability,
                                        target_pos: None,
                                        target_entity: None,
                                        cast_timer: Timer::from_seconds(0.3, TimerMode::Once),
                                    });
                                }
                            }
                        } else {
                            *cmd_mode = CommandMode::AbilityTarget(ability);
                        }
                        return;
                    }
                }
            }
        }
    }

    // --- 3. Execution (Left-Click in Command Mode) ---
    // Execution does NOT require Alt to be held, only that we are already in a mode.
    if *cmd_mode == CommandMode::Normal || !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    if ui_clicked.0 > 0 || ui_press.0 {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return;
    };
    let Some(ray) = camera::viewport_ray_from_window_cursor(camera, cam_gt, window, &graphics) else {
        return;
    };
    let Some(dist) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) else {
        return;
    };
    let point = ray.get_point(dist);

    let units_vec: Vec<(Entity, EntityKind)> = selected_units
        .iter()
        .filter(|(_, _, f)| **f == active_player.0)
        .map(|(e, k, _)| (e, *k))
        .collect();

    if units_vec.is_empty() {
        *cmd_mode = CommandMode::Normal;
        return;
    }

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    match *cmd_mode {
        CommandMode::AttackMove => {
            let n = units_vec.len();
            let spacing = 1.5;
            let radius = if n > 1 {
                (spacing * n as f32 / std::f32::consts::TAU).max(1.0)
            } else {
                0.0
            };
            for (i, (entity, _kind)) in units_vec.iter().enumerate() {
                let offset = if n > 1 {
                    let angle = i as f32 / n as f32 * std::f32::consts::TAU;
                    Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius)
                } else {
                    Vec3::ZERO
                };
                let dest = point + offset;
                if shift {
                    enqueue_task(
                        &mut task_queues,
                        &mut next_task_id,
                        *entity,
                        QueuedTask::AttackMove(dest),
                    );
                } else {
                    apply_manual_attack_move_intent(
                        &mut commands,
                        *entity,
                        dest,
                        time.elapsed_secs_f64(),
                    );
                    commands
                        .entity(*entity)
                        .remove::<AttackTarget>()
                        .insert(MoveTarget(dest))
                        .insert(UnitState::AttackMoving(dest))
                        .insert(TaskSource::Manual);
                    set_current_task(
                        &mut task_queues,
                        &mut next_task_id,
                        *entity,
                        QueuedTask::AttackMove(dest),
                    );
                }
            }
        }
        CommandMode::Patrol => {
            for (entity, _kind) in &units_vec {
                if shift {
                    enqueue_task(
                        &mut task_queues,
                        &mut next_task_id,
                        *entity,
                        QueuedTask::Patrol(point),
                    );
                } else {
                    clear_combat_intent(
                        &mut commands,
                        *entity,
                        time.elapsed_secs_f64(),
                    );
                    commands
                        .entity(*entity)
                        .remove::<AttackTarget>()
                        .insert(MoveTarget(point))
                        .insert(UnitState::Patrolling {
                            target: point,
                            origin: point,
                        })
                        .insert(TaskSource::Manual);
                    set_current_task(
                        &mut task_queues,
                        &mut next_task_id,
                        *entity,
                        QueuedTask::Patrol(point),
                    );
                }
            }
        }
        CommandMode::AbilityTarget(ability) => {
            for (entity, _kind) in &units_vec {
                if let Ok(mut abilities) = unit_abilities.get_mut(*entity) {
                    if abilities.is_ready(ability) {
                        abilities.trigger_cooldown(ability);
                        commands.entity(*entity).insert(CastingAbility {
                            ability,
                            target_pos: Some(point),
                            target_entity: None,
                            cast_timer: Timer::from_seconds(0.3, TimerMode::Once),
                        });
                    }
                }
            }
        }
        CommandMode::Normal => {}
    }

    *cmd_mode = CommandMode::Normal;
}
