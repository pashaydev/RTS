//! Approach — move the unit into the active ability's attack band.
//!
//! Runs each tick in `CombatSet::Approach` *before* `advance_ability_phase`.
//! Reads the current ability (active or default) from `AbilityRegistry` to
//! determine `range`/`min_range`; writes `MoveTarget` when out of band and
//! clears it when in band. Promotes `BrainState` along `Chasing ↔ InRange`.

use bevy::prelude::*;
use bevy::time::Fixed;

use crate::types::{
    app::Faction, AppState, Building, BuildingFootprint, CombatTuning, Mob, MoveTarget, TeamConfig,
    Unit,
};
use crate::world::spatial::WallSpatialGrid;

use super::ability::{Abilities, AbilityRegistry};
use super::brain::{BrainState, CombatSet, Order, UnitBrain};

pub struct CombatApproachPlugin;

impl Plugin for CombatApproachPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            approach_target
                .in_set(CombatSet::Approach)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Shared helper: surface-to-surface 2D distance (ignores terrain Y).
#[inline]
pub fn attack_surface_distance(attacker_pos: Vec3, target_pos: Vec3, target_radius: f32) -> f32 {
    let dx = target_pos.x - attacker_pos.x;
    let dz = target_pos.z - attacker_pos.z;
    ((dx * dx + dz * dz).sqrt() - target_radius).max(0.0)
}

/// Surface distance is inside `[min_range - tolerance, max_range + tolerance]`.
#[inline]
pub fn is_in_attack_band(surface_dist: f32, max_range: f32, min_range: f32, tolerance: f32) -> bool {
    surface_dist >= (min_range - tolerance).max(0.0) && surface_dist <= max_range + tolerance
}

/// Picks a standoff position along the straight attacker→target axis, aiming
/// conservatively into the attack band so small positional jitter (arrival
/// threshold, avoidance push) doesn't drop the unit out of range. Multiple
/// attackers jostle for the same anchor — `steer_avoidance` pushes them apart
/// laterally; the combat layer stays out of that business.
fn desired_approach_pos(
    attacker_pos: Vec3,
    target_pos: Vec3,
    target_radius: f32,
    max_range: f32,
    min_range: f32,
    attacker: Entity,
) -> Vec3 {
    let away = Vec2::new(attacker_pos.x - target_pos.x, attacker_pos.z - target_pos.z);
    let dir = if away.length_squared() > 0.0001 {
        away.normalize()
    } else {
        let angle = (attacker.to_bits() % 360) as f32 * std::f32::consts::TAU / 360.0;
        Vec2::new(angle.cos(), angle.sin())
    };
    // Stand well inside the attack band. For abilities with a minimum range
    // (catapults), pick the midpoint between min and max. For melee, stand at
    // half range so avoidance jitter doesn't drop us out. For ranged, prefer
    // ~75% of max range — close enough to re-engage quickly if the target
    // moves, far enough to not be walked over by chargers.
    let desired_surface = if min_range > 0.0 {
        (min_range + max_range) * 0.5
    } else if max_range > 3.0 {
        (max_range * 0.75).max(1.0)
    } else {
        (max_range * 0.5).max(0.4)
    };
    let radial = target_radius + desired_surface;
    target_pos + Vec3::new(dir.x, 0.0, dir.y) * radial
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn approach_target(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    tuning: Res<CombatTuning>,
    teams: Res<TeamConfig>,
    registry: Res<AbilityRegistry>,
    wall_grid: Res<WallSpatialGrid>,
    mut attackers: Query<
        (
            Entity,
            &Transform,
            &Faction,
            &mut UnitBrain,
            &Abilities,
            Option<&MoveTarget>,
        ),
        Or<(With<Unit>, With<Mob>)>,
    >,
    target_transforms: Query<&Transform>,
    building_footprints: Query<&BuildingFootprint, With<Building>>,
) {
    let now = time.elapsed_secs_f64();
    // Unified attack-band tolerance, shared with advance_ability_phase. Must be
    // wide enough to absorb avoidance jitter (≈0.5u) and the movement arrival
    // threshold (0.7u) — otherwise units flip Chasing↔InRange every tick.
    let tolerance = tuning.range_stay_buffer.max(0.5);

    for (entity, tf, faction, mut brain, abilities, opt_move_target) in &mut attackers {
        // Commit / dying / stunned: approach does not touch movement.
        if brain.is_committed() || brain.is_action_blocked() {
            continue;
        }
        let Some(target) = brain.target else {
            brain.chase_started_at = None;
            continue;
        };
        let Ok(target_tf) = target_transforms.get(target) else {
            // Target vanished — resolve_orders will clean up next tick.
            brain.target = None;
            if matches!(brain.state, BrainState::Chasing | BrainState::InRange) {
                brain.state = BrainState::Idle;
            }
            continue;
        };

        // Pick the ability that governs our current approach band.
        let ability_id = brain
            .active_ability
            .clone()
            .unwrap_or_else(|| abilities.default_ability.clone());
        let Some(ability) = registry.get(&ability_id) else {
            continue;
        };
        let max_range = ability.range;
        let min_range = ability.min_range;

        let target_radius = building_footprints.get(target).map_or(0.0, |fp| fp.0);
        let surface_dist = attack_surface_distance(tf.translation, target_tf.translation, target_radius);

        // Sticky hysteresis: once InRange we tolerate more drift before flipping
        // back to Chasing. Keeps the attacker committed through a swing even if
        // avoidance nudges it to the edge of the band.
        let leave_tolerance = if matches!(brain.state, BrainState::InRange) {
            tolerance * 2.0
        } else {
            tolerance
        };
        let in_band = is_in_attack_band(surface_dist, max_range, min_range, leave_tolerance);

        if in_band {
            if matches!(brain.state, BrainState::Idle | BrainState::Chasing) {
                brain.state = BrainState::InRange;
            }
            if opt_move_target.is_some() {
                commands.entity(entity).remove::<MoveTarget>();
            }
            brain.chase_started_at = None;
        } else {
            if matches!(brain.state, BrainState::Idle | BrainState::InRange) {
                brain.state = BrainState::Chasing;
            }

            // Wall redirect: hostile wall blocking the line → retarget it.
            let mut redirected = false;
            {
                let from = Vec2::new(tf.translation.x, tf.translation.z);
                let to = Vec2::new(target_tf.translation.x, target_tf.translation.z);
                let delta = to - from;
                let line_len = delta.length();
                if line_len > 0.5 {
                    let dir = delta / line_len;
                    let mid = tf.translation.lerp(target_tf.translation, 0.5);
                    let search_radius = line_len * 0.5 + 2.0;
                    let nearby_walls = wall_grid.query_radius(mid, search_radius);
                    let mut best: Option<(Entity, f32)> = None;
                    for (wall_entity, wall_pos_3d, wall_fp, wall_faction) in &nearby_walls {
                        if !teams.is_hostile(faction, wall_faction) {
                            continue;
                        }
                        let wall_pos = Vec2::new(wall_pos_3d.x, wall_pos_3d.z);
                        let rel = wall_pos - from;
                        let t = rel.dot(dir);
                        if t <= 0.3 || t >= line_len - 0.3 {
                            continue;
                        }
                        let closest = from + dir * t;
                        let perp_dist = wall_pos.distance(closest);
                        if perp_dist <= wall_fp + 0.35 && best.map_or(true, |(_, bt)| t < bt) {
                            best = Some((*wall_entity, t));
                        }
                    }
                    if let Some((wall, _)) = best {
                        brain.target = Some(wall);
                        brain.order = Order::Attack(wall);
                        brain.target_lock_until = now + 0.5;
                        redirected = true;
                    }
                }
            }
            if redirected {
                continue;
            }

            let approach = desired_approach_pos(
                tf.translation,
                target_tf.translation,
                target_radius,
                max_range,
                min_range,
                entity,
            );
            if opt_move_target.map_or(true, |m| m.0.distance(approach) > 0.9) {
                commands.entity(entity).insert(MoveTarget(approach));
            }

            // Chase-timeout: use `OrderSource`-derived budget. Sliding start.
            if brain.chase_started_at.is_none() {
                brain.chase_started_at = Some(now);
            }
            if let Some(start) = brain.chase_started_at {
                let budget = brain.order_source.chase_timeout_secs() as f64;
                if now - start > budget {
                    brain.target = None;
                    brain.slot_claim = None;
                    brain.chase_started_at = None;
                    brain.order = Order::Stop;
                    brain.state = BrainState::Idle;
                    commands.entity(entity).remove::<MoveTarget>();
                }
            }
        }
    }
}

