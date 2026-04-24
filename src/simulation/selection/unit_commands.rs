//! Player right-click commands: move, formation, ability targeting,
//! build orders, and task queueing dispatch into the brain pipeline.

use bevy::prelude::*;
use bevy::time::Fixed;
use bevy::window::PrimaryWindow;
use game_state::message::InputCommand;

use crate::blueprints::{BlueprintRegistry, EntityKind};
use crate::infrastructure::audio::{PlaySfx, SfxKind};
use crate::infrastructure::multiplayer::lockstep::{GameplayInputSubmit, PendingLocalInput};
use crate::infrastructure::multiplayer::{ClientNetState, HostNetState, NetRole};
use crate::infrastructure::net_bridge::EntityNetMap;
use crate::presentation::camera;
use crate::presentation::minimap::MinimapInteraction;
use crate::simulation::combat::{
    apply_manual_attack_intent, apply_manual_attack_move_intent, apply_manual_cast_intent,
    apply_manual_hold_intent, apply_manual_move_intent, clear_combat_intent, AbilityRegistry,
    CastTarget, TargetFilter, TargetType,
};
use crate::simulation::items::ItemPickup;
use crate::types::*;
use crate::world::ground::HeightMap;

use super::picking::{ray_aabb_dist, ray_sphere_dist};
use super::{clear_task_queue, enqueue_task, set_current_task};

fn matches_cast_filter(
    caster_faction: &Faction,
    target_faction: Option<&Faction>,
    is_building: bool,
    is_mobile: bool,
    filter: TargetFilter,
) -> bool {
    match filter {
        TargetFilter::AnyUnit => true,
        TargetFilter::SelfOnly => false,
        TargetFilter::Hostile => target_faction.is_some_and(|f| f != caster_faction),
        TargetFilter::Ally => target_faction == Some(caster_faction),
        TargetFilter::StructuresOnly => is_building,
        TargetFilter::MobileOnly => is_mobile,
    }
}

fn resolve_cast_target(
    caster: Entity,
    caster_faction: &Faction,
    ability: &crate::simulation::combat::Ability,
    point: Vec3,
    targets: &Query<(
        Entity,
        &Transform,
        Option<&Faction>,
        Has<Dying>,
        Has<Unit>,
        Has<Building>,
        Has<Mob>,
        Option<&PickRadius>,
    )>,
) -> Option<CastTarget> {
    match ability.target_type {
        TargetType::None | TargetType::SelfOnly => Some(CastTarget::SelfOnly),
        TargetType::Ground => Some(CastTarget::Ground(point)),
        TargetType::Unit => {
            let mut best: Option<(Entity, f32)> = None;
            for (entity, tf, faction, is_dying, is_unit, is_building, is_mob, pick_radius) in targets
            {
                if entity == caster || is_dying {
                    continue;
                }
                let is_mobile = is_unit || is_mob;
                if !matches_cast_filter(
                    caster_faction,
                    faction,
                    is_building,
                    is_mobile,
                    ability.target_filter,
                ) {
                    continue;
                }
                let hit_radius = pick_radius.map_or(1.25, |r| r.0.max(0.6));
                let dist = Vec2::new(tf.translation.x - point.x, tf.translation.z - point.z).length();
                if dist > hit_radius + 0.75 {
                    continue;
                }
                if best.map_or(true, |(_, best_dist)| dist < best_dist) {
                    best = Some((entity, dist));
                }
            }
            best.map(|(entity, _)| CastTarget::Entity(entity))
        }
    }
}

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
        Query<Entity, (With<ResourceNode>, Without<YardResourceNode>)>,
        Query<(Entity, &GlobalTransform), (With<Building>, With<ConstructionProgress>)>,
        Query<(Entity, &ResourceProcessor, &BuildingState, &Faction), With<Building>>,
        Query<Entity, With<ItemPickup>>,
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
        Res<Time<Fixed>>,
        ResMut<PendingLocalInput>,
    ),
) {
    let (camera_q, windows) = viewport;
    let (mobs, resource_nodes, construction_q, processor_buildings, pickups) = target_queries;
    let (other_units, other_buildings) = enemy_detect;
    let (_assigned_workers_q, building_aabb_q, height_map, graphics) = picking_extra;
    let (minimap_interaction, ui_clicked, ui_press, mut sfx) = ui_flags;
    let (net_role, _client_net, _host_net, _registry, net_map, time, mut pending_local_input) =
        net_params;

    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }

    let units_vec: Vec<(Entity, EntityKind)> = selected_units
        .iter()
        .filter(|(_, _, faction, _)| **faction == active_player.0)
        .map(|(e, k, _, _)| (e, *k))
        .collect();
    if units_vec.is_empty() {
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
    let Some(ray) = camera::viewport_ray_from_window_cursor(camera, cam_gt, window, &graphics)
    else {
        return;
    };

    // Contextual right-click action types
    #[derive(Clone, Copy, PartialEq)]
    enum RClickAction {
        AttackEnemy,    // enemy unit or mob
        GatherResource, // resource node (workers)
        AssistBuild,    // construction site (workers)
        MoveToAlly,     // allied building — just move near it
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
        let action = if pickups.contains(entity) {
            None
        } else if mobs.contains(entity) {
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
                    Some(RClickAction::MoveToAlly)
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

        // Priority tie-breaker: pickup > attack enemy > resource > construction > processor > ally
        if let Some(h) = close_hits
            .iter()
            .find(|h| h.action == RClickAction::AttackEnemy)
        {
            Some((h.entity, h.action))
        } else if let Some(h) = close_hits
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

    // Build a lockstep-friendly input packet for right-click actions that
    // already round-trip through `execute_input_command`. Anything else
    // still falls through to the direct offline path below so single-player
    // gameplay stays unchanged.
    let make_network_input = || -> Option<(Vec<u32>, Vec<InputCommand>)> {
        let net_map = net_map.as_ref()?;
        let entity_ids: Vec<u32> = units_vec
            .iter()
            .filter_map(|(entity, _)| net_map.to_net.get(entity).copied())
            .collect();
        if entity_ids.is_empty() {
            return None;
        }

        let mut cmds = Vec::new();
        match target_action {
            Some((target_entity, RClickAction::AttackEnemy)) => {
                let target_id = *net_map.to_net.get(&target_entity)?;
                cmds.push(InputCommand::Attack { target_id });
            }
            Some((target_entity, RClickAction::GatherResource)) => {
                let target_id = *net_map.to_net.get(&target_entity)?;
                cmds.push(InputCommand::Gather { target_id });
            }
            Some(_) => {
                // Unsupported complex right-click action for lockstep — let
                // the legacy direct path below handle it.
                return None;
            }
            None => {
                let point = ground_point?;
                cmds.push(InputCommand::Move {
                    target: [point.x, point.y, point.z],
                    formation: Some(formation.formation.to_net_u8()),
                });
            }
        }

        Some((entity_ids, cmds))
    };

    if *net_role != NetRole::Offline {
        if let Some((entity_ids, cmds)) = make_network_input() {
            pending_local_input.push(entity_ids, cmds);
            return;
        }
        // Fall through to the direct path for unsupported actions — that
        // matches the old behaviour where a handful of right-click cases
        // were only ever applied locally.
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
                            .insert(UnitState::Attacking(target_entity));
                        set_current_task(
                            &mut task_queues,
                            &mut next_task_id,
                            *entity,
                            QueuedTask::Attack(target_entity),
                        );
                    }
                }
                sfx.write(PlaySfx {
                    kind: SfxKind::UnitAttack,
                    position: None,
                });
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
                            clear_combat_intent(&mut commands, *entity, time.elapsed_secs_f64());
                            commands
                                .entity(*entity)
                                .remove::<MoveTarget>()
                                .insert(UnitState::MovingToBuild(target_entity))
                                .insert(TaskSource::Manual)
                                .remove::<ManualIdleSince>();
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
                            .insert(MoveTarget(pos))
                            .insert(UnitState::Moving(pos))
                            .insert(TaskSource::Manual)
                            .remove::<ManualIdleSince>();
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
                                    .insert(MoveTarget(gt.translation()))
                                    .insert(UnitState::Gathering(target_entity))
                                    .insert(TaskSource::Manual)
                                    .remove::<ManualIdleSince>();
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
                            .insert(MoveTarget(gt.translation()))
                            .insert(UnitState::Moving(gt.translation()))
                            .insert(TaskSource::Manual)
                            .remove::<ManualIdleSince>();
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
                                .insert(MoveTarget(dest))
                                .insert(UnitState::Moving(dest))
                                .insert(TaskSource::Manual)
                                .remove::<ManualIdleSince>();
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
                            clear_combat_intent(&mut commands, *entity, time.elapsed_secs_f64());
                            commands
                                .entity(*entity)
                                .remove::<MoveTarget>()
                                .insert(UnitState::MovingToBuild(site_entity))
                                .insert(TaskSource::Manual)
                                .remove::<ManualIdleSince>();
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
                            .insert(MoveTarget(point))
                            .insert(UnitState::Moving(point))
                            .insert(TaskSource::Manual)
                            .remove::<ManualIdleSince>();
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
                            .insert(MoveTarget(point))
                            .insert(UnitState::Moving(point))
                            .insert(TaskSource::Manual)
                            .remove::<ManualIdleSince>();
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
                                .insert(MoveTarget(dest))
                                .insert(UnitState::Moving(dest))
                                .insert(TaskSource::Manual)
                                .remove::<ManualIdleSince>();
                            set_current_task(
                                &mut task_queues,
                                &mut next_task_id,
                                *entity,
                                QueuedTask::Move(dest),
                            );
                        }
                    }
                }
                sfx.write(PlaySfx {
                    kind: SfxKind::UnitMove,
                    position: Some(point),
                });
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
    mut online_submit: GameplayInputSubmit,
    mut cmd_mode: ResMut<CommandMode>,
    mut label_visibility: ResMut<EntityLabelVisibility>,
    mut next_task_id: ResMut<NextTaskId>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    selected_units: Query<
        (
            Entity,
            &EntityKind,
            &Faction,
            Option<&BuildingAssignment>,
            Option<&UnitStance>,
        ),
        (With<Unit>, With<Selected>),
    >,
    mut task_queues: Query<&mut TaskQueue, With<Unit>>,
    combat: (
        Res<AbilityRegistry>,
        Query<&crate::simulation::combat::Abilities>,
        Query<(
            Entity,
            &Transform,
            Option<&Faction>,
            Has<Dying>,
            Has<Unit>,
            Has<Building>,
            Has<Mob>,
            Option<&PickRadius>,
        )>,
    ),
    active_player: Res<ActivePlayer>,
    mut formation: ResMut<ActiveFormation>,
    time: Res<Time<Fixed>>,
    ui_state: (
        Res<UiClickedThisFrame>,
        Res<UiPressActive>,
        Res<BuildingPlacementState>,
    ),
) {
    let (ui_clicked, ui_press, placement) = ui_state;
    let (ref camera_q, ref windows, ref graphics) = viewport;
    let (combat_registry, combat_abilities, cast_targets) = combat;
    if placement.mode != PlacementMode::None {
        return;
    }

    let has_selected = selected_units
        .iter()
        .any(|(_, _, f, _, _)| *f == active_player.0);

    // Escape cancels command mode
    if keys.just_pressed(KeyCode::Escape) {
        *cmd_mode = CommandMode::Normal;
        return;
    }

    if keys.just_pressed(KeyCode::KeyL) {
        label_visibility.show_unit_labels = !label_visibility.show_unit_labels;
        return;
    }

    if has_selected {
        let selected_player_entities = || {
            selected_units
                .iter()
                .filter(|(_, _, f, _, _)| **f == active_player.0)
                .map(|(entity, _, _, _, _)| entity)
        };
        let selected_player_workers = || {
            selected_units
                .iter()
                .filter(|(_, kind, faction, _, _)| {
                    **faction == active_player.0 && **kind == EntityKind::Worker
                })
                .map(|(entity, _, _, _, _)| entity)
        };

        // --- 1. Instant Commands (Hold & Stop) ---
        if keys.just_pressed(KeyCode::KeyH) {
            if online_submit.submit(selected_player_entities(), vec![InputCommand::HoldPosition]) {
                *cmd_mode = CommandMode::Normal;
                return;
            }
            for (entity, _, faction, _, _) in &selected_units {
                if *faction != active_player.0 {
                    continue;
                }
                apply_manual_hold_intent(&mut commands, entity, time.elapsed_secs_f64());
                commands
                    .entity(entity)
                    .remove::<MoveTarget>()
                    .insert(UnitState::HoldPosition)
                    .insert(TaskSource::Manual)
                    .remove::<ManualIdleSince>();
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
            if online_submit.submit(selected_player_workers(), vec![InputCommand::Scuttle]) {
                *cmd_mode = CommandMode::Normal;
                return;
            }
            for (entity, kind, faction, assignment, _) in &selected_units {
                if *faction != active_player.0 {
                    continue;
                }
                clear_combat_intent(&mut commands, entity, time.elapsed_secs_f64());
                let grace = ManualIdleSince(time.elapsed_secs_f64());
                if *kind == EntityKind::Worker {
                    crate::simulation::resources::unassign_worker_from_processor(
                        &mut commands,
                        entity,
                        assignment.map(|assignment| assignment.0),
                    );
                    commands.entity(entity).insert(grace);
                } else {
                    commands
                        .entity(entity)
                        .remove::<MoveTarget>()
                        .insert(UnitState::Idle)
                        .insert(TaskSource::Auto)
                        .insert(grace);
                }
                clear_task_queue(&mut task_queues, entity);
            }
            *cmd_mode = CommandMode::Normal;
            return;
        }

        // --- 2. Stance Cycle (V) ---
        if keys.just_pressed(KeyCode::KeyV) {
            let next_stance = selected_units
                .iter()
                .find(|(_, _, faction, _, _)| **faction == active_player.0)
                .map(|(_, _, _, _, stance)| {
                    stance.copied().unwrap_or(UnitStance::Defensive).cycle()
                });
            if let Some(stance) = next_stance {
                if online_submit.submit(
                    selected_player_entities(),
                    vec![InputCommand::SetStance {
                        stance: stance.to_u8(),
                    }],
                ) {
                    return;
                }
            }
            for (entity, _, faction, _, _) in &selected_units {
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

        // --- Ability hotkeys (T = first ability, Y = second ability) ---
        let ability_hotkey = if keys.just_pressed(KeyCode::KeyT) {
            Some(0usize)
        } else if keys.just_pressed(KeyCode::KeyY) {
            Some(1usize)
        } else {
            None
        };
        if let Some(slot) = ability_hotkey {
            // Find first selected unit with abilities
            for (entity, _kind, faction, _, _) in &selected_units {
                if *faction != active_player.0 {
                    continue;
                }
                if let Ok(abilities) = combat_abilities.get(entity) {
                    if let Some(ability) = abilities
                        .castable
                        .get(slot)
                        .and_then(AbilityId::from_combat_id)
                    {
                        let combat_id = ability.combat_id();
                        if ability.targeting() == AbilityTargeting::NoTarget {
                            if online_submit.submit(
                                selected_units
                                    .iter()
                                    .filter(|(_, _, selected_faction, _, _)| {
                                        **selected_faction == active_player.0
                                    })
                                    .filter_map(|(selected_entity, _, _, _, _)| {
                                        combat_abilities
                                            .get(selected_entity)
                                            .ok()
                                            .filter(|ab| {
                                                ab.castable.contains(&combat_id)
                                                    && ab.is_ready(&combat_id)
                                            })
                                            .map(|_| selected_entity)
                                    }),
                                vec![InputCommand::UseAbility {
                                    ability_id: ability.to_u8(),
                                    target: [0.0, 0.0, 0.0],
                                }],
                            ) {
                                *cmd_mode = CommandMode::Normal;
                                return;
                            }
                            if let Some(target) = combat_registry
                                .get(&combat_id)
                                .and_then(|meta| {
                                    resolve_cast_target(
                                        entity,
                                        faction,
                                        &meta,
                                        Vec3::ZERO,
                                        &cast_targets,
                                    )
                                })
                            {
                                apply_manual_cast_intent(
                                    &mut commands,
                                    entity,
                                    combat_id,
                                    target,
                                    time.elapsed_secs_f64(),
                                );
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
    let Some(ray) = camera::viewport_ray_from_window_cursor(camera, cam_gt, window, &graphics)
    else {
        return;
    };
    let Some(dist) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) else {
        return;
    };
    let point = ray.get_point(dist);

    let units_vec: Vec<(Entity, EntityKind)> = selected_units
        .iter()
        .filter(|(_, _, f, _, _)| **f == active_player.0)
        .map(|(e, k, _, _, _)| (e, *k))
        .collect();

    if units_vec.is_empty() {
        *cmd_mode = CommandMode::Normal;
        return;
    }

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    match *cmd_mode {
        CommandMode::AttackMove => {
            if online_submit.submit(
                units_vec.iter().map(|(entity, _)| *entity),
                vec![InputCommand::AttackMove {
                    target: [point.x, point.y, point.z],
                }],
            ) {
                *cmd_mode = CommandMode::Normal;
                return;
            }
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
                        .insert(MoveTarget(dest))
                        .insert(UnitState::AttackMoving(dest))
                        .insert(TaskSource::Manual)
                        .remove::<ManualIdleSince>();
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
            if online_submit.submit(
                units_vec.iter().map(|(entity, _)| *entity),
                vec![InputCommand::Patrol {
                    target: [point.x, point.y, point.z],
                }],
            ) {
                *cmd_mode = CommandMode::Normal;
                return;
            }
            for (entity, _kind) in &units_vec {
                if shift {
                    enqueue_task(
                        &mut task_queues,
                        &mut next_task_id,
                        *entity,
                        QueuedTask::Patrol(point),
                    );
                } else {
                    clear_combat_intent(&mut commands, *entity, time.elapsed_secs_f64());
                    commands
                        .entity(*entity)
                        .insert(MoveTarget(point))
                        .insert(UnitState::Patrolling {
                            target: point,
                            origin: point,
                        })
                        .insert(TaskSource::Manual)
                        .remove::<ManualIdleSince>();
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
            let combat_id = ability.combat_id();
            if online_submit.submit(
                units_vec.iter().filter_map(|(entity, _)| {
                    combat_abilities
                        .get(*entity)
                        .ok()
                        .filter(|abilities| {
                            abilities.castable.contains(&combat_id) && abilities.is_ready(&combat_id)
                        })
                        .map(|_| *entity)
                }),
                vec![InputCommand::UseAbility {
                    ability_id: ability.to_u8(),
                    target: [point.x, point.y, point.z],
                }],
            ) {
                *cmd_mode = CommandMode::Normal;
                return;
            }
            let Some(ability_meta) = combat_registry.get(&combat_id) else {
                *cmd_mode = CommandMode::Normal;
                return;
            };
            for (entity, _kind) in &units_vec {
                let Ok(abilities) = combat_abilities.get(*entity) else {
                    continue;
                };
                if !abilities.castable.contains(&combat_id) || !abilities.is_ready(&combat_id) {
                    continue;
                }
                let Ok((_, _, faction, _, _)) = selected_units.get(*entity) else {
                    continue;
                };
                if let Some(target) =
                    resolve_cast_target(*entity, faction, &ability_meta, point, &cast_targets)
                {
                    apply_manual_cast_intent(
                        &mut commands,
                        *entity,
                        combat_id.clone(),
                        target,
                        time.elapsed_secs_f64(),
                    );
                }
            }
        }
        CommandMode::Normal => {}
    }

    *cmd_mode = CommandMode::Normal;
}
