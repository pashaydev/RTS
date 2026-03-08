use bevy::prelude::*;

use crate::components::*;
use crate::model_assets::AnimationAssets;

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                discover_animation_players,
                drive_animations,
                face_movement_direction,
            ),
        );
    }
}

/// Walk the hierarchy of entities with `UnitSceneChild` to find the `AnimationPlayer`
/// deep in the GLTF scene graph. Once found, insert `AnimPlayerRef` and `AnimationController`
/// on the parent unit entity and start the idle animation.
fn discover_animation_players(
    mut commands: Commands,
    scene_children: Query<(Entity, &ChildOf), With<UnitSceneChild>>,
    parents_without_anim: Query<Entity, (With<Unit>, Without<AnimPlayerRef>)>,
    mob_parents_without_anim: Query<Entity, (With<Mob>, Without<AnimPlayerRef>)>,
    children_q: Query<&Children>,
    anim_players: Query<Entity, With<AnimationPlayer>>,
    anim_assets: Option<Res<AnimationAssets>>,
    mut anim_player_mut: Query<&mut AnimationPlayer>,
) {
    let Some(ref assets) = anim_assets else {
        return;
    };

    for (scene_entity, child_of) in &scene_children {
        let parent = child_of.parent();

        // Check if parent is a unit or mob that doesn't have AnimPlayerRef yet
        let is_unit = parents_without_anim.contains(parent);
        let is_mob = mob_parents_without_anim.contains(parent);
        if !is_unit && !is_mob {
            continue;
        }

        // Walk the hierarchy of the scene child to find AnimationPlayer
        if let Some(player_entity) = find_animation_player(scene_entity, &children_q, &anim_players) {
            commands.entity(parent).insert((
                AnimPlayerRef(player_entity),
                AnimationController {
                    current_state: AnimState::Idle,
                },
            ));

            // Insert the animation graph on the player entity and start idle
            commands.entity(player_entity).insert((
                AnimationGraphHandle(assets.graph.clone()),
                AnimationTransitions::new(),
            ));

            if let Ok(mut player) = anim_player_mut.get_mut(player_entity) {
                if let Some(&node_idx) = assets.node_indices.get(&AnimState::Idle) {
                    player.play(node_idx).repeat();
                }
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
fn drive_animations(
    mut anim_controllers: Query<(
        &mut AnimationController,
        &AnimPlayerRef,
        &Health,
        Option<&MoveTarget>,
        Option<&AttackTarget>,
        Option<&AttackRange>,
        &Transform,
    )>,
    target_transforms: Query<&Transform, Without<AnimationController>>,
    anim_assets: Option<Res<AnimationAssets>>,
    mut anim_players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    let Some(ref assets) = anim_assets else {
        return;
    };

    for (mut controller, anim_ref, health, move_target, attack_target, attack_range, my_tf) in &mut anim_controllers {
        let desired = if health.current <= 0.0 {
            AnimState::Die
        } else if let Some(at) = attack_target {
            if let Ok(target_tf) = target_transforms.get(at.0) {
                let dist = my_tf.translation.distance(target_tf.translation);
                let range = attack_range.map(|r| r.0).unwrap_or(2.0);
                if dist <= range * 1.5 {
                    AnimState::Attack
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

            if let Some(&node_idx) = assets.node_indices.get(&desired) {
                if let Ok((mut player, mut transitions)) = anim_players.get_mut(anim_ref.0) {
                    let transition = transitions
                        .play(&mut player, node_idx, std::time::Duration::from_millis(200));
                    if desired != AnimState::Die {
                        transition.repeat();
                    }
                }
            }
        }
    }
}

/// Rotate the parent entity to face toward MoveTarget or AttackTarget.
fn face_movement_direction(
    time: Res<Time>,
    mut query: Query<(
        &mut Transform,
        Option<&MoveTarget>,
        Option<&AttackTarget>,
    ), Or<(With<Unit>, With<Mob>)>>,
    target_transforms: Query<&Transform, (Without<Unit>, Without<Mob>)>,
) {
    let rate = 8.0;

    for (mut transform, move_target, attack_target) in &mut query {
        let target_pos = if let Some(at) = attack_target {
            if let Ok(target_tf) = target_transforms.get(at.0) {
                Some(target_tf.translation)
            } else {
                None
            }
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
