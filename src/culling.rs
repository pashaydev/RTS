use bevy::camera::primitives::{Frustum, Sphere as FrustumSphere};
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::prelude::*;

use crate::components::*;
use crate::ground::WaterPlane;

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

/// Default padding in world units beyond frustum edge before culling kicks in.
/// Used when an entity has no `CullingBounds` component.
const FRUSTUM_PADDING: f32 = 15.0;

/// Entities beyond this distance from the camera are hidden entirely.
/// At typical RTS zoom (60+ units height), objects beyond ~200 units are sub-pixel.
const LOD_HIDE_DISTANCE: f32 = 180.0;
const LOD_HIDE_DISTANCE_SQ: f32 = LOD_HIDE_DISTANCE * LOD_HIDE_DISTANCE;

/// Decorations are hidden at a shorter distance since they're small props.
const DECO_HIDE_DISTANCE_SQ: f32 = 120.0 * 120.0;

/// Tests entity positions against the camera frustum and adds/removes `FrustumCulled`.
///
/// Uses `CullingSourceCamera` to determine whose frustum to test against.
/// When an entity has `CullingBounds`, its radius is used; otherwise `FRUSTUM_PADDING`.
/// Sets `Visibility::Hidden` on culled entities so the GPU skips them entirely.
/// When an entity re-enters the frustum, visibility is restored to `Inherited`.
/// The fog-of-war system runs later and may override visible entities back to `Hidden`
/// if they are in unexplored territory — that is intentional.
fn sync_frustum_culling(
    mut commands: Commands,
    camera_q: Query<&Frustum, With<CullingSourceCamera>>,
    mut entities: Query<
        (
            Entity,
            &GlobalTransform,
            &mut Visibility,
            Has<FrustumCulled>,
            Option<&CullingBounds>,
            Option<&mut CullReason>,
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
            With<WaterPlane>,
        )>,
    >,
) {
    let Ok(frustum) = camera_q.single() else {
        return;
    };

    for (entity, gtf, mut visibility, is_culled, bounds, cull_reason) in &mut entities {
        let pos = gtf.translation();
        let (center, radius) = if let Some(b) = bounds {
            (pos + b.center_offset, b.radius)
        } else {
            (pos, FRUSTUM_PADDING)
        };
        let sphere = FrustumSphere {
            center: center.into(),
            radius,
        };
        let in_view = frustum.intersects_sphere(&sphere, true);

        if in_view && is_culled {
            commands.entity(entity).remove::<FrustumCulled>();
            *visibility = Visibility::Inherited;
            if let Some(mut reason) = cull_reason {
                *reason = CullReason::Visible;
            }
        } else if !in_view && !is_culled {
            commands.entity(entity).insert(FrustumCulled);
            *visibility = Visibility::Hidden;
            if let Some(mut reason) = cull_reason {
                *reason = CullReason::Frustum;
            }
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
/// Uses `CullingSourceCamera` for distance reference.
fn distance_lod_system(
    camera_q: Query<&GlobalTransform, With<CullingSourceCamera>>,
    mut entities: Query<
        (
            &GlobalTransform,
            &mut Visibility,
            Has<Decoration>,
            Has<FrustumCulled>,
            Option<&mut CullReason>,
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
            With<WaterPlane>,
        )>,
    >,
) {
    let Ok(cam_gtf) = camera_q.single() else {
        return;
    };
    let cam_pos = cam_gtf.translation();

    for (gtf, mut visibility, is_decoration, is_frustum_culled, cull_reason) in &mut entities {
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
            if let Some(mut reason) = cull_reason {
                *reason = CullReason::Distance;
            }
        } else if *visibility == Visibility::Hidden {
            *visibility = Visibility::Inherited;
            if let Some(mut reason) = cull_reason {
                *reason = CullReason::Visible;
            }
        }
    }
}
