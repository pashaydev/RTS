use bevy::camera::primitives::{Frustum, Sphere as FrustumSphere};
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::prelude::*;

use crate::components::*;

/// System set for all culling systems. Fog-of-war ordering depends on this
/// so that fog can override visibility after culling has finished.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CullingSet;

pub struct CullingPlugin;

impl Plugin for CullingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                sync_frustum_culling,
                pause_culled_animations,
                resume_unculled_animations,
                distance_lod_system,
            )
                .chain()
                .in_set(CullingSet)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Padding in world units beyond frustum edge before culling kicks in.
/// Prevents pop-in at screen edges.
const FRUSTUM_PADDING: f32 = 15.0;

/// Entities beyond this distance from the camera are hidden entirely.
/// At typical RTS zoom (60+ units height), objects beyond ~200 units are sub-pixel.
const LOD_HIDE_DISTANCE: f32 = 180.0;
const LOD_HIDE_DISTANCE_SQ: f32 = LOD_HIDE_DISTANCE * LOD_HIDE_DISTANCE;

/// Decorations are hidden at a shorter distance since they're small props.
const DECO_HIDE_DISTANCE_SQ: f32 = 120.0 * 120.0;

/// Tests entity positions against the camera frustum and adds/removes `FrustumCulled`.
///
/// Sets `Visibility::Hidden` on culled entities so the GPU skips them entirely.
/// When an entity re-enters the frustum, visibility is restored to `Inherited`.
/// The fog-of-war system runs later and may override visible entities back to `Hidden`
/// if they are in unexplored territory — that is intentional.
fn sync_frustum_culling(
    mut commands: Commands,
    camera_q: Query<&Frustum, With<RtsCamera>>,
    mut entities: Query<
        (Entity, &GlobalTransform, &mut Visibility, Has<FrustumCulled>),
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

    for (entity, gtf, mut visibility, is_culled) in &mut entities {
        let pos = gtf.translation();
        let sphere = FrustumSphere {
            center: pos.into(),
            radius: FRUSTUM_PADDING,
        };
        let in_view = frustum.intersects_sphere(&sphere, true);

        if in_view && is_culled {
            commands.entity(entity).remove::<FrustumCulled>();
            *visibility = Visibility::Inherited;
        } else if !in_view && !is_culled {
            commands.entity(entity).insert(FrustumCulled);
            *visibility = Visibility::Hidden;
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

/// Hide entities that are too far from the camera to be meaningfully visible.
/// Decorations use a shorter threshold since they're smaller.
/// Only checks entities that passed the frustum test (not already FrustumCulled).
fn distance_lod_system(
    camera_q: Query<&GlobalTransform, With<RtsCamera>>,
    mut entities: Query<
        (
            &GlobalTransform,
            &mut Visibility,
            Has<Decoration>,
            Has<FrustumCulled>,
        ),
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
    let Ok(cam_gtf) = camera_q.single() else {
        return;
    };
    let cam_pos = cam_gtf.translation();

    for (gtf, mut visibility, is_decoration, is_frustum_culled) in &mut entities {
        if is_frustum_culled {
            continue;
        }

        let pos = gtf.translation();
        let dist_sq = (cam_pos - pos).length_squared();
        let threshold = if is_decoration {
            DECO_HIDE_DISTANCE_SQ
        } else {
            LOD_HIDE_DISTANCE_SQ
        };

        if dist_sq > threshold {
            *visibility = Visibility::Hidden;
        } else if *visibility == Visibility::Hidden {
            *visibility = Visibility::Inherited;
        }
    }
}
