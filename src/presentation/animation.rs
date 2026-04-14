use bevy::prelude::*;

use crate::blueprints::EntityKind;
use crate::presentation::model_assets::{ttp_anim_set, AnimationAssets, UnitAnimationRegistry};
use crate::types::*;
use crate::world::pathfinding::NavPath;

const FIDGET_DURATION: f32 = 1.2;
const BREATHING_RATE: f32 = 1.5;
const BREATHING_AMPLITUDE: f32 = 0.008;

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                discover_animation_players,
                drive_animations,
                idle_fidget_system,
                face_movement_direction,
                idle_breathing_system,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Walk the hierarchy of entities with `UnitSceneChild` to find the `AnimationPlayer`
/// deep in the GLTF scene graph. Once found, insert `AnimPlayerRef` and `AnimationController`
/// on the parent unit entity and start the idle animation.
///
/// Uses per-unit-type animation graphs from `UnitAnimationRegistry` for TTP units,
/// and falls back to the legacy shared graph for KayKit mobs.
fn discover_animation_players(
    mut commands: Commands,
    scene_children: Query<(Entity, &ChildOf), With<UnitSceneChild>>,
    parents_without_anim: Query<Entity, (With<Unit>, Without<AnimPlayerRef>)>,
    mob_parents_without_anim: Query<Entity, (With<Mob>, Without<AnimPlayerRef>)>,
    kind_q: Query<&EntityKind>,
    children_q: Query<&Children>,
    anim_players: Query<Entity, With<AnimationPlayer>>,
    registry: Option<Res<UnitAnimationRegistry>>,
    legacy_assets: Option<Res<AnimationAssets>>,
) {
    for (scene_entity, child_of) in &scene_children {
        let parent = child_of.parent();

        let is_unit = parents_without_anim.contains(parent);
        let is_mob = mob_parents_without_anim.contains(parent);
        if !is_unit && !is_mob {
            continue;
        }

        // Walk the hierarchy of the scene child to find AnimationPlayer
        let Some(player_entity) = find_animation_player(scene_entity, &children_q, &anim_players)
        else {
            continue;
        };

        // Determine which animation graph to use
        let kind = kind_q.get(parent).ok().copied();

        // Skip TTP units if the registry isn't ready yet — they need their
        // per-unit-type graph, not the legacy KayKit one.
        let is_ttp = kind.map_or(false, |k| ttp_anim_set(k).is_some());
        if is_ttp && registry.is_none() {
            continue;
        }

        let (graph_handle, idle_node) = if let Some(ref reg) = registry {
            // Try per-unit-type graph first (TTP units)
            if let Some(kind) = kind {
                if let Some(anim_data) = reg.data.get(&kind) {
                    let idle = anim_data.node_indices.get(&AnimState::Idle).copied();
                    (Some(anim_data.graph.clone()), idle)
                } else if let Some(ref legacy) = reg.legacy {
                    // Fallback to legacy graph (mobs/summons)
                    let idle = legacy.node_indices.get(&AnimState::Idle).copied();
                    (Some(legacy.graph.clone()), idle)
                } else {
                    (None, None)
                }
            } else if let Some(ref legacy) = reg.legacy {
                let idle = legacy.node_indices.get(&AnimState::Idle).copied();
                (Some(legacy.graph.clone()), idle)
            } else {
                (None, None)
            }
        } else if let Some(ref assets) = legacy_assets {
            // Registry not ready yet, use legacy assets (mobs only — TTP units skipped above)
            let idle = assets.node_indices.get(&AnimState::Idle).copied();
            (Some(assets.graph.clone()), idle)
        } else {
            (None, None)
        };

        let Some(graph) = graph_handle else {
            continue;
        };

        // Insert AnimPlayerRef on parent, and the graph + transitions on the
        // animation player entity.  Both are deferred commands so they apply
        // at the same time.
        //
        // Set initial controller state to `DeathB` — a sentinel that will
        // never match the first real `desired` state, guaranteeing an
        // immediate transition on the next `drive_animations` tick.
        //
        // NOTE: Do **not** call `player.play()` here.  The AnimationGraphHandle
        // is inserted via deferred commands, so the AnimationPlayer does not
        // have a graph yet on this frame.  Calling play() without a graph
        // causes undefined behaviour that manifests as broken animations on
        // Windows/WASM.  Instead, we let `drive_animations` handle the first
        // transition on the next frame, once the graph is available.
        commands.entity(parent).insert((
            AnimPlayerRef(player_entity),
            AnimationController {
                current_state: AnimState::DeathB,
                change_cooldown: 0.0,
            },
        ));

        commands
            .entity(player_entity)
            .insert((AnimationGraphHandle(graph), AnimationTransitions::new()));
    }
}

pub(crate) fn find_animation_player(
    entity: Entity,
    children_q: &Query<&Children>,
    anim_players: &Query<Entity, With<AnimationPlayer>>,
) -> Option<Entity> {
    if anim_players.contains(entity) {
        return Some(entity);
    }
    if let Ok(children) = children_q.get(entity) {
        for child in children.iter() {
            if let Some(found) = find_animation_player(child, children_q, anim_players) {
                return Some(found);
            }
        }
    }
    None
}

/// Minimum time (seconds) between non-critical animation state changes.
/// Prevents rapid oscillation at high frame rates (e.g. Walk↔Run flickering).
const ANIM_CHANGE_COOLDOWN: f32 = 0.25;

/// Walk→Run speed threshold (fraction of max speed).
const RUN_THRESHOLD_UP: f32 = 0.6;
/// Run→Walk speed threshold — lower than up-threshold to prevent oscillation.
const RUN_THRESHOLD_DOWN: f32 = 0.45;

/// Returns true if this state is critical (must transition instantly, ignoring cooldown).
fn is_critical_state(state: AnimState) -> bool {
    matches!(
        state,
        AnimState::DeathA
            | AnimState::DeathB
            | AnimState::Damage
            | AnimState::AttackA
            | AnimState::AttackB
            | AnimState::CastA
            | AnimState::CastB
    )
}

/// Determine desired AnimState from entity state and transition if changed.
/// Uses per-unit-type animation graphs from UnitAnimationRegistry.
///
/// Applies a cooldown between non-critical transitions (Idle/Walk/Run) to
/// prevent rapid oscillation at high frame rates, and uses hysteresis for
/// the Walk↔Run speed threshold.
fn drive_animations(
    time: Res<Time>,
    mut anim_controllers: Query<
        (
            Entity,
            &mut AnimationController,
            &AnimPlayerRef,
            &Health,
            &EntityKind,
            Option<&UnitState>,
            Option<&MoveTarget>,
            Option<&AttackTarget>,
            Option<&AttackRange>,
            Option<&PatrolState>,
            Option<&AttackWindup>,
            Option<&AttackRecovery>,
            Option<&CastingAbility>,
            &Transform,
            Option<&MovementSmoothing>,
        ),
        Without<FrustumCulled>,
    >,
    hit_reactions: Query<(), With<HitReaction>>,
    unit_speeds: Query<&UnitSpeed>,
    target_transforms: Query<&Transform, Without<AnimationController>>,
    registry: Option<Res<UnitAnimationRegistry>>,
    legacy_assets: Option<Res<AnimationAssets>>,
    anim_graph_check: Query<(), With<AnimationGraphHandle>>,
    mut anim_players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    let dt = time.delta_secs();

    for (
        anim_entity,
        mut controller,
        anim_ref,
        health,
        kind,
        unit_state,
        move_target,
        attack_target,
        attack_range,
        patrol_state,
        attack_windup,
        attack_recovery,
        casting_ability,
        my_tf,
        opt_smoothing,
    ) in &mut anim_controllers
    {
        // Tick cooldown
        controller.change_cooldown = (controller.change_cooldown - dt).max(0.0);

        let patrol_kind = patrol_state.map(|p| p.state);
        let desired = if health.current <= 0.0 {
            // Randomize death variant based on entity index for variety
            if anim_ref.0.to_bits() % 2 == 0 {
                AnimState::DeathA
            } else {
                AnimState::DeathB
            }
        } else if hit_reactions.get(anim_entity).is_ok() {
            // Brief hit reaction flinch
            AnimState::Damage
        } else if casting_ability.is_some() {
            // Currently casting an ability — use CastA for casters, AttackA for melee
            if matches!(kind, EntityKind::Mage | EntityKind::Priest) {
                AnimState::CastA
            } else {
                AnimState::AttackA
            }
        } else if unit_state.map_or(false, |s| matches!(s, UnitState::Building(_))) {
            AnimState::AttackA
        } else if unit_state.map_or(false, |s| matches!(s, UnitState::Gathering(_))) {
            // Walk while moving to resource; swing when arrived
            if move_target.is_some() {
                AnimState::Walk
            } else {
                AnimState::AttackA
            }
        } else if let Some(UnitState::AssignedGathering { phase, .. }) = unit_state {
            // Only play work animation when actually harvesting/depositing
            match phase {
                AssignedPhase::Harvesting { .. } | AssignedPhase::Depositing { .. } => {
                    AnimState::AttackA
                }
                _ => {
                    if move_target.is_some() {
                        AnimState::Walk
                    } else {
                        AnimState::Idle
                    }
                }
            }
        } else if unit_state.map_or(false, |s| {
            matches!(
                s,
                UnitState::MovingToBuild(_)
                    | UnitState::MovingToPlot(_)
                    | UnitState::ReturningToDeposit { .. }
            )
        }) {
            AnimState::Walk
        } else if matches!(
            patrol_kind,
            Some(
                PatrolStateKind::Patrolling | PatrolStateKind::Chasing | PatrolStateKind::Returning
            )
        ) {
            AnimState::Walk
        } else if attack_windup.is_some() {
            if matches!(
                kind,
                EntityKind::Mage | EntityKind::Priest | EntityKind::Demon
            ) {
                AnimState::CastA
            } else {
                AnimState::AttackA
            }
        } else if attack_recovery.is_some() {
            AnimState::Idle
        } else if matches!(patrol_kind, Some(PatrolStateKind::Attacking)) {
            if matches!(kind, EntityKind::Demon) {
                AnimState::CastA
            } else {
                AnimState::AttackA
            }
        } else if let Some(at) = attack_target {
            if let Ok(target_tf) = target_transforms.get(at.0) {
                let dist = my_tf.translation.distance(target_tf.translation);
                let range = attack_range.map(|r| r.0).unwrap_or(2.0);
                if dist <= range * 1.5 {
                    // Use CastA for staff-type casters, AttackA for others
                    if matches!(kind, EntityKind::Mage | EntityKind::Priest) {
                        AnimState::CastA
                    } else {
                        AnimState::AttackA
                    }
                } else {
                    AnimState::Walk
                }
            } else {
                AnimState::Walk
            }
        } else if move_target.is_some() {
            // Pick Walk vs Run with hysteresis to prevent oscillation
            let opt_unit_speed = unit_speeds.get(anim_entity).ok();
            let threshold = if controller.current_state == AnimState::Run {
                RUN_THRESHOLD_DOWN
            } else {
                RUN_THRESHOLD_UP
            };
            let use_run = opt_smoothing
                .zip(opt_unit_speed)
                .map_or(false, |(sm, us)| sm.current_speed > us.0 * threshold);
            if use_run {
                AnimState::Run
            } else {
                AnimState::Walk
            }
        } else {
            AnimState::Idle
        };

        if desired != controller.current_state {
            // Guard: skip if the animation graph hasn't been attached yet.
            // This happens on the first frame after `discover_animation_players`
            // inserts components via deferred commands.  We intentionally do NOT
            // update `controller.current_state` here so the mismatch persists
            // and the transition fires on the next frame once the graph exists.
            if !anim_graph_check.contains(anim_ref.0) {
                continue;
            }

            // Critical states (death, attack, damage) bypass cooldown.
            // Non-critical transitions (Idle/Walk/Run) respect cooldown to
            // prevent rapid oscillation at high frame rates.
            if !is_critical_state(desired)
                && !is_critical_state(controller.current_state)
                && controller.change_cooldown > 0.0
            {
                continue;
            }

            controller.current_state = desired;
            controller.change_cooldown = ANIM_CHANGE_COOLDOWN;

            // Look up node index from per-unit-type graph or legacy
            let node_idx = if let Some(ref reg) = registry {
                reg.data
                    .get(kind)
                    .and_then(|d| d.node_indices.get(&desired).copied())
                    .or_else(|| {
                        reg.legacy
                            .as_ref()
                            .and_then(|l| l.node_indices.get(&desired).copied())
                    })
            } else {
                legacy_assets
                    .as_ref()
                    .and_then(|a| a.node_indices.get(&desired).copied())
            };

            if let Some(node_idx) = node_idx {
                if let Ok((mut player, mut transitions)) = anim_players.get_mut(anim_ref.0) {
                    let transition = transitions.play(
                        &mut player,
                        node_idx,
                        std::time::Duration::from_millis(200),
                    );
                    if !matches!(desired, AnimState::DeathA | AnimState::DeathB) {
                        transition.repeat();
                    }
                }
            }
        }
    }
}

/// Rotate the parent entity to face toward its immediate navigation or combat target.
/// Also supports idle fidget look targets and applies a subtle turn lean.
fn face_movement_direction(
    time: Res<Time>,
    zoom_level: Res<CameraZoomLevel>,
    mut queries: ParamSet<(
        Query<(Entity, &Transform), (Or<(With<Unit>, With<Mob>)>, Without<FrustumCulled>)>,
        Query<
            (
                Entity,
                &mut Transform,
                Option<&MoveTarget>,
                Option<&NavPath>,
                Option<&AttackTarget>,
                Option<&PatrolState>,
                Option<&IdleBehavior>,
                Option<&UnitState>,
            ),
            (Or<(With<Unit>, With<Mob>)>, Without<FrustumCulled>),
        >,
    )>,
    non_unit_transforms: Query<&Transform, (Without<Unit>, Without<Mob>)>,
) {
    let rate = 8.0;
    let apply_lean = zoom_level.detail == DetailLevel::Close;

    // First pass: collect positions of all units/mobs for unit-vs-unit facing
    let unit_positions: std::collections::HashMap<Entity, Vec3> = queries
        .p0()
        .iter()
        .map(|(e, tf)| (e, tf.translation))
        .collect();

    for (
        entity,
        mut transform,
        move_target,
        nav_path,
        attack_target,
        patrol_state,
        idle_behavior,
        unit_state,
    ) in &mut queries.p1()
    {
        let target_pos = if let Some(at) = attack_target {
            if at.0 != entity {
                // Try unit positions first, then non-unit (buildings, etc.)
                unit_positions
                    .get(&at.0)
                    .copied()
                    .or_else(|| non_unit_transforms.get(at.0).ok().map(|tf| tf.translation))
            } else {
                None
            }
        } else if let Some(UnitState::Building(building)) = unit_state {
            // Workers building should face the construction site
            non_unit_transforms
                .get(*building)
                .ok()
                .map(|tf| tf.translation)
        } else if let Some(patrol) = patrol_state {
            match patrol.state {
                PatrolStateKind::Patrolling => patrol.patrol_target,
                PatrolStateKind::Returning => Some(patrol.center),
                PatrolStateKind::Chasing | PatrolStateKind::Attacking => {
                    patrol.patrol_target.or(Some(patrol.center))
                }
                _ => None,
            }
        } else if let Some(nav) = nav_path {
            nav.waypoints
                .get(nav.current_index)
                .copied()
                .or_else(|| move_target.map(|mt| mt.0))
        } else if let Some(mt) = move_target {
            Some(mt.0)
        } else if let Some(idle) = idle_behavior {
            // Idle fidget: look toward a random direction periodically
            idle.fidget_look_target
        } else {
            None
        };

        if let Some(target) = target_pos {
            let dir = Vec3::new(
                target.x - transform.translation.x,
                0.0,
                target.z - transform.translation.z,
            );
            if dir.length_squared() > 0.01 {
                let target_rot = Quat::from_rotation_y(dir.x.atan2(dir.z));

                // Compute angular delta for turn lean
                let prev_rot = transform.rotation;
                transform.rotation =
                    prev_rot.slerp(target_rot, (rate * time.delta_secs()).min(1.0));

                // Apply subtle Z-axis lean into turns (only when close zoom and actively moving)
                if apply_lean && move_target.is_some() {
                    let (_, prev_y, _) = prev_rot.to_euler(EulerRot::YXZ);
                    let (_, new_y, _) = transform.rotation.to_euler(EulerRot::YXZ);
                    let angular_delta = (new_y - prev_y).clamp(-0.12, 0.12);
                    if angular_delta.abs() > 0.005 {
                        transform.rotation *= Quat::from_rotation_z(-angular_delta * 2.0);
                    }
                }
            }
        }
    }
}

/// Periodically pick a random direction for idle units to glance toward.
fn idle_fidget_system(
    time: Res<Time>,
    zoom_level: Res<CameraZoomLevel>,
    mut query: Query<
        (&mut IdleBehavior, &Transform),
        (
            With<Unit>,
            Without<MoveTarget>,
            Without<AttackTarget>,
            Without<FrustumCulled>,
        ),
    >,
) {
    // Only run fidget at Close detail
    if zoom_level.detail != DetailLevel::Close {
        // Clear any active fidgets when zooming out
        for (mut idle, _) in &mut query {
            idle.fidget_look_target = None;
            idle.fidget_elapsed = 0.0;
        }
        return;
    }

    let dt = time.delta_secs();
    for (mut idle, transform) in &mut query {
        // If currently fidgeting, tick the elapsed time
        if idle.fidget_look_target.is_some() {
            idle.fidget_elapsed += dt;
            if idle.fidget_elapsed >= FIDGET_DURATION {
                idle.fidget_look_target = None;
                idle.fidget_elapsed = 0.0;
            }
            continue;
        }

        idle.fidget_timer
            .tick(std::time::Duration::from_secs_f32(dt));
        if idle.fidget_timer.just_finished() {
            // Pick a random direction to look at (3 units away)
            let angle = idle.breathing_phase * 7.3 + time.elapsed_secs() * 2.1;
            let look_offset = Vec3::new(angle.cos() * 3.0, 0.0, angle.sin() * 3.0);
            idle.fidget_look_target = Some(transform.translation + look_offset);
            idle.fidget_elapsed = 0.0;
        }
    }
}

/// Apply subtle breathing/scale oscillation to idle units when zoomed in close.
fn idle_breathing_system(
    time: Res<Time>,
    zoom_level: Res<CameraZoomLevel>,
    mut query: Query<
        (&mut IdleBehavior, &mut Transform),
        (
            With<Unit>,
            Without<MoveTarget>,
            Without<AttackTarget>,
            Without<FrustumCulled>,
        ),
    >,
) {
    if zoom_level.detail != DetailLevel::Close {
        return;
    }

    let dt = time.delta_secs();
    for (mut idle, mut transform) in &mut query {
        idle.breathing_phase += dt * BREATHING_RATE;
        if idle.breathing_phase > std::f32::consts::TAU {
            idle.breathing_phase -= std::f32::consts::TAU;
        }

        // Subtle Y-axis oscillation simulating breathing
        let breath = 1.0 + idle.breathing_phase.sin() * BREATHING_AMPLITUDE;
        // Apply to Y scale: base_scale * breath
        // Use X scale as reference for base (X is not modified by breathing)
        transform.scale.y = transform.scale.x * breath;
    }
}
