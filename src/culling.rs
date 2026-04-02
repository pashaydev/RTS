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
                hide_nearby_trees,
                update_tree_alpha_for_distance,
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

/// Trees are hidden earlier than general entities — their leaf canopy overdraw
/// is the main GPU cost when looking top-down, and at 130 units they're tiny.
const TREE_HIDE_DISTANCE_SQ: f32 = 130.0 * 130.0;

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
            With<DecoRevealed>,
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
            Has<DecoChunk>,
            Has<FrustumCulled>,
            Has<MatureTree>,
            Has<GrowingTree>,
            Has<Sapling>,
            Has<WaterPlane>,
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
            With<DecoRevealed>,
        )>,
    >,
) {
    let Ok(cam_gtf) = camera_q.single() else {
        return;
    };
    let cam_pos = cam_gtf.translation();

    for (gtf, mut visibility, is_decoration, is_deco_chunk, is_frustum_culled, is_mature_tree, is_growing_tree, is_sapling, is_water, bounds, cull_reason) in
        &mut entities
    {
        if is_frustum_culled {
            continue;
        }

        // Water planes are large terrain features — skip distance LOD entirely.
        // They are already handled by frustum culling with proper bounding spheres.
        if is_water {
            continue;
        }

        let pos = if let Some(b) = bounds {
            gtf.translation() + b.center_offset
        } else {
            gtf.translation()
        };
        let dist_sq = (cam_pos - pos).length_squared();
        let is_tree = is_mature_tree || is_growing_tree || is_sapling;
        let threshold = if is_decoration || is_deco_chunk {
            DECO_HIDE_DISTANCE_SQ
        } else if is_tree {
            TREE_HIDE_DISTANCE_SQ
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

/// Hide trees that are too close to the camera (camera inside the canopy).
/// Prevents the player from drowning in leaf geometry at extreme close zoom.
const TREE_NEAR_HIDE_DIST_SQ: f32 = 10.0 * 10.0;

fn hide_nearby_trees(
    camera_q: Query<&GlobalTransform, With<CullingSourceCamera>>,
    mut trees: Query<
        (&GlobalTransform, &mut Visibility),
        (
            Or<(With<MatureTree>, With<GrowingTree>, With<Sapling>)>,
            Without<FrustumCulled>,
        ),
    >,
) {
    let Ok(cam_gtf) = camera_q.single() else {
        return;
    };
    let cam_pos = cam_gtf.translation();
    for (tree_gtf, mut vis) in &mut trees {
        let dist_sq = (cam_pos - tree_gtf.translation()).length_squared();
        if dist_sq < TREE_NEAR_HIDE_DIST_SQ {
            *vis = Visibility::Hidden;
        }
    }
}

/// Adjusts tree leaf material alpha cutoff based on camera zoom level.
/// Close: 0.6 (slightly thinned), Medium: 0.7, Far: 0.85 (heavily thinned).
/// Only runs when `CameraZoomLevel` actually changes (detail level or distance drift).
fn update_tree_alpha_for_distance(
    zoom_level: Res<CameraZoomLevel>,
    leaf_mats: Res<TreeLeafMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !zoom_level.is_changed() {
        return;
    }
    let cutoff = match zoom_level.detail {
        DetailLevel::Close => 0.6,
        DetailLevel::Medium => 0.7,
        DetailLevel::Far => 0.85,
    };
    for handle in &leaf_mats.0 {
        if let Some(mat) = materials.get_mut(handle) {
            mat.alpha_mode = AlphaMode::Mask(cutoff);
        }
    }
}
