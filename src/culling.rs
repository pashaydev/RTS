use bevy::camera::primitives::{Frustum, Sphere as FrustumSphere};
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::prelude::*;

use crate::components::*;

pub struct CullingPlugin;

impl Plugin for CullingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (sync_frustum_culling, pause_culled_animations, resume_unculled_animations)
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Padding in world units beyond frustum edge before culling kicks in.
/// Prevents pop-in at screen edges.
const FRUSTUM_PADDING: f32 = 15.0;

/// Tests entity positions against the camera frustum and adds/removes `FrustumCulled`.
fn sync_frustum_culling(
    mut commands: Commands,
    camera_q: Query<&Frustum, With<RtsCamera>>,
    entities: Query<
        (Entity, &GlobalTransform, Has<FrustumCulled>),
        Or<(
            With<Unit>,
            With<Mob>,
            With<Building>,
            With<ResourceNode>,
            With<Decoration>,
            With<Sapling>,
            With<GrowingTree>,
            With<GrowingResource>,
        )>,
    >,
) {
    let Ok(frustum) = camera_q.single() else {
        return;
    };

    for (entity, gtf, is_culled) in &entities {
        let pos = gtf.translation();
        // Use sphere intersection with padding as radius
        let sphere = FrustumSphere {
            center: pos.into(),
            radius: FRUSTUM_PADDING,
        };
        let in_view = frustum.intersects_sphere(&sphere, true);

        if in_view && is_culled {
            commands.entity(entity).remove::<FrustumCulled>();
        } else if !in_view && !is_culled {
            commands.entity(entity).insert(FrustumCulled);
        }
    }
}

/// Pause AnimationPlayers on entities that just got culled.
fn pause_culled_animations(
    culled: Query<&AnimPlayerRef, Added<FrustumCulled>>,
    mut players: Query<&mut AnimationPlayer>,
) {
    for anim_ref in &culled {
        if let Ok(mut player) = players.get_mut(anim_ref.0) {
            player.pause_all();
        }
    }
}

/// Resume AnimationPlayers on entities that just re-entered the frustum.
fn resume_unculled_animations(
    mut removed: RemovedComponents<FrustumCulled>,
    anim_refs: Query<&AnimPlayerRef>,
    mut players: Query<&mut AnimationPlayer>,
) {
    for entity in removed.read() {
        if let Ok(anim_ref) = anim_refs.get(entity) {
            if let Ok(mut player) = players.get_mut(anim_ref.0) {
                player.resume_all();
            }
        }
    }
}
