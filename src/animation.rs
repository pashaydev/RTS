use bevy::prelude::*;

use crate::blueprints::EntityKind;
use crate::components::*;
use crate::model_assets::{ttp_anim_set, AnimationAssets, UnitAnimationRegistry};
use crate::pathfinding::NavPath;

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                discover_animation_players,
                drive_animations,
                face_movement_direction,
            )
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
    mut anim_player_mut: Query<&mut AnimationPlayer>,
) {
    for (scene_entity, child_of) in &scene_children {
        let parent = child_of.parent();

        let is_unit = parents_without_anim.contains(parent);
        let is_mob = mob_parents_without_anim.contains(parent);
        if !is_unit && !is_mob {
            continue;
        }

        // Walk the hierarchy of the scene child to find AnimationPlayer
        let Some(player_entity) =
            find_animation_player(scene_entity, &children_q, &anim_players)
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

        commands.entity(parent).insert((
            AnimPlayerRef(player_entity),
            AnimationController {
                current_state: AnimState::Idle,
            },
        ));

        commands.entity(player_entity).insert((
            AnimationGraphHandle(graph),
            AnimationTransitions::new(),
        ));

        if let Some(node_idx) = idle_node {
            if let Ok(mut player) = anim_player_mut.get_mut(player_entity) {
                player.play(node_idx).repeat();
            }
        }
    }
}

fn find_animation_player(
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

/// Determine desired AnimState from entity state and transition if changed.
/// Uses per-unit-type animation graphs from UnitAnimationRegistry.
fn drive_animations(
    mut anim_controllers: Query<
        (
            &mut AnimationController,
            &AnimPlayerRef,
            &Health,
            &EntityKind,
            Option<&UnitState>,
            Option<&MoveTarget>,
            Option<&AttackTarget>,
            Option<&AttackRange>,
            &Transform,
        ),
        Without<FrustumCulled>,
    >,
    target_transforms: Query<&Transform, Without<AnimationController>>,
    registry: Option<Res<UnitAnimationRegistry>>,
    legacy_assets: Option<Res<AnimationAssets>>,
    mut anim_players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for (
        mut controller,
        anim_ref,
        health,
        kind,
        unit_state,
        move_target,
        attack_target,
        attack_range,
        my_tf,
    ) in &mut anim_controllers
    {
        let desired = if health.current <= 0.0 {
            AnimState::DeathA
        } else if unit_state.map_or(false, |s| matches!(s, UnitState::Gathering(_))) {
            AnimState::AttackA
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
            AnimState::Walk
        } else {
            AnimState::Idle
        };

        if desired != controller.current_state {
            controller.current_state = desired;

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
fn face_movement_direction(
    time: Res<Time>,
    mut query: Query<
        (
            &mut Transform,
            Option<&MoveTarget>,
            Option<&NavPath>,
            Option<&AttackTarget>,
        ),
        (Or<(With<Unit>, With<Mob>)>, Without<FrustumCulled>),
    >,
    target_transforms: Query<&Transform, (Without<Unit>, Without<Mob>)>,
) {
    let rate = 8.0;

    for (mut transform, move_target, nav_path, attack_target) in &mut query {
        let target_pos = if let Some(at) = attack_target {
            if let Ok(target_tf) = target_transforms.get(at.0) {
                Some(target_tf.translation)
            } else {
                None
            }
        } else if let Some(nav) = nav_path {
            nav.waypoints.get(nav.current_index).copied().or_else(|| {
                move_target.map(|mt| mt.0)
            })
        } else if let Some(mt) = move_target {
            Some(mt.0)
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
                transform.rotation = transform
                    .rotation
                    .slerp(target_rot, (rate * time.delta_secs()).min(1.0));
            }
        }
    }
}
