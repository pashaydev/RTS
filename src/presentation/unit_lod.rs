//! LOD mesh swap for animated GLTF units. Watches camera distance and
//! despawns / respawns the `UnitSceneChild` subtree when a unit crosses a
//! tier boundary.
//!
//! Two tiers:
//!   - **High**     — full-poly skinned rig. Default everywhere close.
//!   - **Impostor** — billboard quad sampling a pre-baked atlas. Wired up
//!                    for `EntityKind::Goblin` only; other kinds stay on
//!                    High at all distances.
//!
//! Animations are re-discovered automatically on the High path because
//! the respawned child carries `UnitSceneChildPending`, which
//! `discover_animation_players` watches for. The parent's `AnimPlayerRef`
//! and `AnimationController` are stripped so discovery fires fresh on the
//! next frame. On the Impostor path we keep both stripped and drive the
//! sprite directly from gameplay components via `impostor_sync_system`.

use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy_mod_outline::{AsyncSceneInheritOutline, InheritOutline};

use crate::blueprints::EntityKind;
use crate::presentation::materials::impostor::{
    GoblinImpostor, ImpostorAnimState, ImpostorMaterial, ImpostorParams,
};
use crate::presentation::model_assets::UnitModelAssets;
use crate::types::*;
use crate::world::lighting::{DayCycle, EntityLightConfig, EntityLightGrid, SunLight};

pub struct UnitLodPlugin;

impl Plugin for UnitLodPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                unit_lod_swap_system
                    .before(crate::presentation::animation::discover_animation_players),
                impostor_sync_system.after(unit_lod_swap_system),
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// ── Hysteresis thresholds (squared world units) ──
// Entering the next tier uses the outer threshold; leaving uses the inner
// one. The 7u gap keeps the swap from thrashing at the edge of a camera
// glide.
/// Max swaps per frame. A zoom sweep across a 1000-goblin crowd drains
/// over ~63 frames at this budget; in a stable camera position the
/// steady-state swap count is zero.
const SWAP_BUDGET_PER_FRAME: usize = 16;

/// Marker on the child entity that holds the impostor billboard. Also
/// carries `UnitSceneChild` so the generic despawn-on-swap logic picks it
/// up without special-casing.
#[derive(Component)]
pub struct ImpostorSprite;

/// Handle attached to the PARENT unit entity while it's at Impostor tier.
/// `impostor_sync_system` uses this to locate the material asset to mutate
/// each frame without walking the children list.
#[derive(Component)]
pub struct ImpostorHandle(pub Handle<ImpostorMaterial>);

/// Walk the unit query, compare camera-distance band to the stored LOD,
/// and swap the scene child when the desired tier differs.
pub fn unit_lod_swap_system(
    mut commands: Commands,
    camera_q: Query<&GlobalTransform, With<CullingSourceCamera>>,
    assets: Option<Res<UnitModelAssets>>,
    goblin_impostor: Option<Res<GoblinImpostor>>,
    mut impostor_materials: Option<ResMut<Assets<ImpostorMaterial>>>,
    mut units: Query<(
        Entity,
        &GlobalTransform,
        &Transform,
        &EntityKind,
        &mut UnitLod,
        Option<&Children>,
    )>,
    scene_children: Query<(), With<UnitSceneChild>>,
) {
    let Some(assets) = assets else { return };
    let Ok(cam_gtf) = camera_q.single() else {
        return;
    };
    let cam_pos = cam_gtf.translation();

    let mut done = 0usize;
    for (parent, gtf, local_tf, kind, mut lod, children) in &mut units {
        if done >= SWAP_BUDGET_PER_FRAME {
            break;
        }
        let d_sq = (cam_pos - gtf.translation()).length_squared();
        let has_impostor = *kind == EntityKind::Goblin && goblin_impostor.is_some();

        let desired = match lod.current {
            LodTier::High if d_sq > UNIT_IMPOSTOR_ENTER_DISTANCE_SQ && has_impostor => {
                LodTier::Impostor
            }
            LodTier::Impostor if d_sq < UNIT_IMPOSTOR_EXIT_DISTANCE_SQ => LodTier::High,
            _ => continue,
        };

        // Resolve the desired-tier asset before mutating the world so we
        // can bail cleanly if nothing's registered.
        let tier_ok = match desired {
            LodTier::High => assets.lod_scenes.contains_key(&(*kind, desired)),
            LodTier::Impostor => has_impostor,
        };
        if !tier_ok {
            continue;
        }

        // Despawn all existing scene-child visuals (both SceneRoot children
        // and prior impostor sprites carry `UnitSceneChild`).
        if let Some(children) = children {
            for child in children.iter() {
                if scene_children.contains(child) {
                    commands.entity(child).despawn();
                }
            }
        }

        // Strip anim state + stale team color + any prior impostor handle
        // so each tier's re-spawn code starts from a clean slate.
        commands
            .entity(parent)
            .remove::<AnimPlayerRef>()
            .remove::<AnimationController>()
            .remove::<TeamColorApplied>()
            .remove::<ImpostorHandle>();

        match desired {
            LodTier::High => {
                let scene = assets.lod_scenes.get(&(*kind, desired)).cloned().unwrap();
                let cal = assets.calibration.get(kind);
                let scale = cal.map(|c| c.scale).unwrap_or(2.0);
                let y_off = cal.map(|c| c.y_offset).unwrap_or(0.0);
                let facing = cal.map(|c| c.facing_rotation).unwrap_or(0.0);

                let mut child = commands.spawn((
                    SceneRoot(scene),
                    UnitSceneChild,
                    UnitSceneChildPending,
                    Transform::from_scale(Vec3::splat(scale))
                        .with_translation(Vec3::new(0.0, y_off, 0.0))
                        .with_rotation(Quat::from_rotation_y(facing)),
                ));
                #[cfg(not(target_arch = "wasm32"))]
                child.insert((InheritOutline, AsyncSceneInheritOutline::default()));
                let child_id = child.id();
                commands.entity(parent).add_child(child_id);
            }
            LodTier::Impostor => {
                // Safe to unwrap: we gated on `has_impostor` above.
                let gi = goblin_impostor.as_ref().unwrap();
                let mats = impostor_materials.as_mut().unwrap();

                // Per-instance material so we can mutate (yaw, state, ...).
                // Identical atlas handle + shared quad mesh mean only the
                // uniform bind group differs between instances; Bevy will
                // still batch draws where adjacent materials match.
                let (row, frame_count) = gi.lookup(ImpostorAnimState::Idle);
                let (size, feet_y) = impostor_size_and_anchor(kind, &gi.meta);
                let yaw0 = yaw_from_rotation(local_tf.rotation);
                // Hash the entity to a stable per-instance phase in [0, 1)
                // so crowd idle/walk cycles don't lockstep.
                let phase = (parent.to_bits().wrapping_mul(0x9E3779B97F4A7C15) >> 33) as f32
                    / (1u64 << 31) as f32;

                let handle = mats.add(ImpostorMaterial {
                    params: ImpostorParams {
                        time: 0.0,
                        time_phase: phase,
                        yaw_facing: yaw0,
                        size,
                        state_row_offset: row,
                        frame_count,
                        total_rows: gi.meta.total_rows(),
                        directions: gi.meta.directions,
                        fps: gi.meta.fps as f32,
                        ..default()
                    },
                    atlas: gi.atlas.clone(),
                });

                let child_id = commands
                    .spawn((
                        Mesh3d(gi.quad_mesh.clone()),
                        MeshMaterial3d(handle.clone()),
                        Transform::from_translation(Vec3::new(0.0, feet_y, 0.0)),
                        UnitSceneChild,
                        ImpostorSprite,
                    ))
                    .id();
                commands.entity(parent).add_child(child_id);
                commands.entity(parent).insert(ImpostorHandle(handle));
            }
        }

        lod.current = desired;
        lod.last_switch_dist_sq = d_sq;
        done += 1;
    }
}

/// Extract the Y-axis rotation (yaw) from a quaternion in the convention
/// the impostor shader expects: 0 = Transform::forward() pointing -Z,
/// positive = CCW around +Y (matches Bevy's standard rotation direction).
fn yaw_from_rotation(rot: Quat) -> f32 {
    // Rotate (0, 0, -1) by `rot`, project to XZ, take atan2.
    // This is cheaper than a full Euler decomposition and is robust to
    // pitch/roll contamination (ignores any non-yaw component).
    let fwd = rot * Vec3::NEG_Z;
    fwd.x.atan2(fwd.z) + std::f32::consts::PI
    // Offset by π because atan2(x, z) measures from +Z but our "forward"
    // reference is -Z. Shifting by π normalizes so yaw=0 means facing -Z.
}

/// Billboard dimensions for each kind, in world units.
///
/// Returns `(size, feet_y)`:
/// - `size`     — side length of the square billboard quad.
/// - `feet_y`   — child transform Y so the sprite's feet land on the
///               in-game goblin's feet (which sit below parent.y because
///               the scene-child calibration offset in `UnitModelAssets`
///               translates the rig down by `y_offset`).
///
/// The atlas bake adds a `margin_px / cell_size` padding on each side of
/// the sprite (≈7.8% for the default 10 px / 128 px cell). We size the
/// billboard so the goblin-portion matches the 3D rig's in-game height
/// (calibration scale × raw Blender height = 0.7 × 4.76u ≈ 3.33u) and
/// offset it downward so the sprite's feet row sits at the rig's feet.
fn impostor_size_and_anchor(
    kind: &EntityKind,
    meta: &crate::presentation::materials::impostor::ImpostorAtlasMeta,
) -> (f32, f32) {
    // Atlas padding: symmetric top/bottom margin in UV space.
    let margin_frac = (meta.cell_height as f32 - sprite_fill_px(meta)) * 0.5
        / meta.cell_height.max(1) as f32;
    let fill = 1.0 - 2.0 * margin_frac;

    match kind {
        EntityKind::Goblin => {
            // Target the 3D rig's in-game silhouette: raw GLB height 4.76u
            // × calibration scale 0.7 ≈ 3.33u feet-to-head.
            let target_height = 3.33_f32;
            let size = target_height / fill.max(0.05);
            // 3D rig's feet are at y_offset + (aabb_z_min × scale) relative
            // to parent: -0.8 + (0.058 × 0.7) ≈ -0.759u. Lower the billboard
            // so its sprite-feet row (at `margin_frac × size` above billboard
            // bottom) lands there.
            let rig_feet_y = -0.759_f32;
            let sprite_feet_offset = margin_frac * size;
            (size, rig_feet_y - sprite_feet_offset)
        }
        _ => (3.0, 0.0),
    }
}

/// Conservative estimate of the sprite's filled pixel height inside a
/// cell. Assumes the baker used symmetric 10px margins (matches
/// `bake_goblin_atlas.py`'s default); treats any future override as still
/// symmetric. Called at LOD-swap time, so one allocation-free read.
fn sprite_fill_px(meta: &crate::presentation::materials::impostor::ImpostorAtlasMeta) -> f32 {
    // The JSON doesn't record the bake's margin_px directly. 10px matches
    // the default; keep in sync with `bake_goblin_atlas.py --margin-px`.
    let assumed_margin = 10_u32.min(meta.cell_height / 4);
    (meta.cell_height.saturating_sub(2 * assumed_margin)) as f32
}

/// Derive the impostor anim state from gameplay components. Minimal
/// priority chain — no AnimationController needed because the state is
/// stripped on LOD swap.
fn derive_impostor_state(
    health: Option<&Health>,
    attack_target: Option<&AttackTarget>,
    move_target: Option<&MoveTarget>,
) -> ImpostorAnimState {
    if health.map_or(false, |h| h.current <= 0.0) {
        return ImpostorAnimState::Death;
    }
    if attack_target.is_some() {
        return ImpostorAnimState::Attack;
    }
    if move_target.is_some() {
        return ImpostorAnimState::Walk;
    }
    ImpostorAnimState::Idle
}

/// Per-frame sync: update every impostor's uniform block with current
/// time, yaw, and state-derived (row_offset, frame_count).
pub fn impostor_sync_system(
    time: Res<Time>,
    cycle: Res<DayCycle>,
    entity_light_grid: Res<EntityLightGrid>,
    entity_light_config: Res<EntityLightConfig>,
    goblin_impostor: Option<Res<GoblinImpostor>>,
    ambient: Res<GlobalAmbientLight>,
    mut mats: ResMut<Assets<ImpostorMaterial>>,
    sun_q: Query<(&DirectionalLight, &Transform), With<SunLight>>,
    units: Query<(
        &Transform,
        &GlobalTransform,
        &ImpostorHandle,
        Option<&Health>,
        Option<&AttackTarget>,
        Option<&MoveTarget>,
        Option<&MobTier>,
    )>,
) {
    let Some(gi) = goblin_impostor else { return };
    let t = time.elapsed_secs();
    let (light_tint, shadow_tint, sun_direction, light_mix, top_light, bottom_light) =
        impostor_lighting_params(&ambient, sun_q.single().ok());

    for (tf, gtf, handle, health, attack, move_target, opt_tier) in &units {
        let state = derive_impostor_state(health, attack, move_target);
        let (row, fcount) = gi.lookup(state);
        let yaw = yaw_from_rotation(tf.rotation);
        let local_visibility = impostor_local_visibility(
            gtf.translation(),
            &cycle,
            &entity_light_grid,
            &entity_light_config,
        );

        let tier = opt_tier.copied().unwrap_or_default();
        let tier_rgb = tier.tint_rgb();
        let tier_tint = Vec4::new(tier_rgb[0], tier_rgb[1], tier_rgb[2], 1.0);

        if let Some(mat) = mats.get_mut(&handle.0) {
            mat.params.time = t;
            mat.params.yaw_facing = yaw;
            mat.params.state_row_offset = row;
            mat.params.frame_count = fcount;
            mat.params.light_tint = light_tint;
            mat.params.shadow_tint = shadow_tint;
            mat.params.sun_direction = sun_direction;
            mat.params.light_mix = light_mix;
            mat.params.top_light = top_light;
            mat.params.bottom_light = bottom_light;
            mat.params.local_visibility = local_visibility;
            mat.params.wrap_amount = 0.35;
            mat.params.rim_strength = 0.14 + local_visibility * 0.24;
            mat.params.tier_tint = tier_tint;
        }
    }
}

fn impostor_lighting_params(
    ambient: &GlobalAmbientLight,
    sun: Option<(&DirectionalLight, &Transform)>,
) -> (Vec4, Vec4, Vec4, f32, f32, f32) {
    let ambient_rgba = ambient.color.to_linear().to_f32_array();
    let ambient_rgb = Vec3::new(ambient_rgba[0], ambient_rgba[1], ambient_rgba[2]);
    let ambient_strength = (ambient.brightness / 300.0).clamp(0.15, 1.25);

    let (sun_rgb, sun_dir, sun_strength, sun_height) = match sun {
        Some((sun, tf)) => {
            let sun_rgba = sun.color.to_linear().to_f32_array();
            let dir = -tf.forward().as_vec3();
            (
                Vec3::new(sun_rgba[0], sun_rgba[1], sun_rgba[2]),
                dir,
                (sun.illuminance / 6000.0).clamp(0.0, 1.1),
                dir.y.clamp(0.0, 1.0),
            )
        }
        None => (Vec3::ONE, Vec3::new(0.5, 0.7, 0.3), 0.0, 0.0),
    };

    // Keep impostors responsive to time-of-day while staying compatible
    // with baked sprite shading. Ambient dominates at night; sun warmth
    // pushes daytime and sunset coloration.
    let daylight = (ambient_strength * 0.45 + sun_strength * 0.55).clamp(0.18, 1.0);
    let lit_tint = (ambient_rgb * ambient_strength * 0.55 + sun_rgb * sun_strength * 0.45)
        .clamp(Vec3::splat(0.18), Vec3::splat(1.15));
    let shadow_tint = (ambient_rgb * (0.60 + ambient_strength * 0.20))
        .clamp(Vec3::splat(0.12), Vec3::splat(0.95));

    let top_light = 1.02 + sun_height * 0.12;
    let bottom_light = 0.82 + ambient_strength * 0.10;

    (
        lit_tint.extend(1.0),
        shadow_tint.extend(1.0),
        sun_dir.normalize_or_zero().extend(0.0),
        daylight,
        top_light,
        bottom_light.min(top_light),
    )
}

fn impostor_local_visibility(
    world_pos: Vec3,
    cycle: &DayCycle,
    grid: &EntityLightGrid,
    config: &EntityLightConfig,
) -> f32 {
    if !config.enabled {
        return 0.0;
    }

    let global_factor = entity_light_factor_for_impostors(cycle.time, config);
    if global_factor <= 0.01 {
        return 0.0;
    }

    let max_range = 25.0_f32;
    let radius_cells = (max_range / grid.cell_size).ceil() as i32;
    let cell = IVec2::new(
        (world_pos.x / grid.cell_size).floor() as i32,
        (world_pos.z / grid.cell_size).floor() as i32,
    );

    let mut visibility = 0.0_f32;
    for dz in -radius_cells..=radius_cells {
        for dx in -radius_cells..=radius_cells {
            let Some(cluster) = grid.cells.get(&(cell + IVec2::new(dx, dz))) else {
                continue;
            };

            let (range, weight) = if cluster.has_building {
                (25.0_f32, 1.0_f32)
            } else if cluster.has_unit {
                (15.0_f32, 0.8_f32)
            } else {
                continue;
            };

            let dist = world_pos.distance(cluster.centroid);
            if dist >= range {
                continue;
            }

            let falloff = 1.0 - dist / range;
            visibility = visibility.max(falloff * falloff * weight);
        }
    }

    // Light pockets matter most when the scene is dark. During daytime
    // they should barely change the result.
    let darkness = (1.0 - global_factor).clamp(0.0, 1.0);
    (visibility * darkness * 1.35).clamp(0.0, 1.0)
}

fn entity_light_factor_for_impostors(day_time: f32, config: &EntityLightConfig) -> f32 {
    let night = config.night_factor;
    let day = config.day_factor;
    match day_time {
        t if t < 0.20 => night,
        t if t < 0.30 => {
            let s = (t - 0.20) / 0.10;
            night - smoothstep_impostor(s) * (night - day)
        }
        t if t < 0.70 => day,
        t if t < 0.80 => {
            let s = (t - 0.70) / 0.10;
            day + smoothstep_impostor(s) * (night - day)
        }
        _ => night,
    }
}

fn smoothstep_impostor(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
