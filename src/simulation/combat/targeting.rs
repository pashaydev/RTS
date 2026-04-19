//! Targeting — scoring math (moved unchanged from the old core), auto-aggro
//! scanning, and AttackMove target acquisition.
//!
//! Writes `UnitBrain.order = Order::Attack(t)` with `OrderSource::Auto` when a
//! scan picks up a hostile candidate. Manual orders are never overwritten.

use bevy::prelude::*;
use bevy::time::Fixed;

use crate::types::{
    app::Faction, AppState, ArmorType, Building, CombatTuning, DamageType, Health, Mob, Unit,
    UnitStance, TacticalRole, TargetingProfile, ThreatValue, ReservedIncomingDamage, TeamConfig,
};
use crate::world::spatial::SpatialHashGrid;

use super::ability::{Abilities, AbilityRegistry};
use super::brain::{BrainState, CombatSet, Order, UnitBrain};
use crate::types::OrderSource;

pub struct CombatTargetingPlugin;

impl Plugin for CombatTargetingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            auto_aggro_and_attackmove
                .in_set(CombatSet::Targeting)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// ── Scored target selection (moved verbatim from old combat core) ─────────────

pub struct TargetScoreInput<'a> {
    pub profile: &'a TargetingProfile,
    pub attacker_pos: Vec3,
    pub attacker_damage_type: DamageType,
    pub scan_range: f32,
    pub target_pos: Vec3,
    pub target_health: &'a Health,
    pub target_armor: ArmorType,
    pub target_threat: f32,
    pub target_is_building: bool,
    pub target_reserved_damage: f32,
}

/// Pure scoring function for target selection. Lower = better. `None` = reject.
pub fn target_score(input: &TargetScoreInput) -> Option<f32> {
    if input.target_health.current <= 0.0 {
        return None;
    }
    let dx = input.target_pos.x - input.attacker_pos.x;
    let dz = input.target_pos.z - input.attacker_pos.z;
    let dist = (dx * dx + dz * dz).sqrt();
    if dist > input.scan_range * 1.5 {
        return None;
    }
    let dist_norm = (dist / input.scan_range.max(0.1)).clamp(0.0, 1.5);
    let hp_frac = (input.target_health.current / input.target_health.max.max(1.0)).clamp(0.0, 1.0);
    let multiplier = input.attacker_damage_type.multiplier_vs(input.target_armor);
    let overkill_frac = if input.target_health.current > 0.0 {
        (input.target_reserved_damage / input.target_health.current).min(2.0)
    } else {
        2.0
    };

    let distance_term = input.profile.distance_weight * dist_norm;
    let low_hp_term = input.profile.low_hp_weight * hp_frac;
    let threat_term = -input.profile.threat_weight * input.target_threat;
    let counter_term = -input.profile.counter_weight * (multiplier - 1.0);
    let building_term = if input.target_is_building {
        input.profile.building_penalty
    } else {
        0.0
    };
    let reserved_term = input.profile.reserved_damage_penalty * overkill_frac;

    Some(distance_term + low_hp_term + threat_term + counter_term + building_term + reserved_term)
}

// ── Auto-aggro + AttackMove target acquisition ──────────────────────────────

/// Scans for hostile candidates around each unit. Writes `Order::Attack(t)`
/// (Auto) when the unit is idle + not holding + stance allows, or when an
/// AttackMove order has no active target.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn auto_aggro_and_attackmove(
    time: Res<Time<Fixed>>,
    tuning: Res<CombatTuning>,
    teams: Res<TeamConfig>,
    registry: Res<AbilityRegistry>,
    spatial: Res<SpatialHashGrid>,
    mut attackers: Query<
        (
            Entity,
            &Transform,
            &Faction,
            &mut UnitBrain,
            &Abilities,
            Option<&UnitStance>,
            Option<&TacticalRole>,
            Option<&TargetingProfile>,
        ),
        Or<(With<Unit>, With<Mob>)>,
    >,
    candidate_data: Query<(
        &Transform,
        &Health,
        Option<&ArmorType>,
        Option<&Faction>,
        Option<&ThreatValue>,
        Option<&ReservedIncomingDamage>,
        Option<&Building>,
    )>,
) {
    let now = time.elapsed_secs_f64();

    for (entity, tf, faction, mut brain, abilities, opt_stance, _role, opt_profile) in &mut attackers
    {
        // Manual orders are sacrosanct until they complete or are replaced by the player.
        if brain.order_source == OrderSource::Manual && !matches!(brain.order, Order::Stop) {
            continue;
        }
        // Don't yank target from a committed swing or a cast.
        if brain.is_committed() || brain.is_action_blocked() {
            continue;
        }
        // Respect lock window.
        if brain.target_lock_until > now {
            continue;
        }

        let stance = opt_stance.copied().unwrap_or_default();
        let scan_multiplier = tuning.scan_multiplier(stance);
        if scan_multiplier <= 0.0 {
            // Passive: no auto-engage.
            continue;
        }

        let ability_id = abilities.default_ability.clone();
        let Some(ability) = registry.get(&ability_id) else {
            continue;
        };
        let scan_range = (ability.range * scan_multiplier).max(4.0);

        // Two triggers for scanning:
        //   (1) We have no target and the order is Stop/Move/Hold — opportunistic aggro.
        //   (2) AttackMove order with no active target — AM target acquisition.
        let should_scan = match brain.order {
            Order::Stop | Order::Hold | Order::Move(_) => {
                brain.target.is_none() && matches!(brain.state, BrainState::Idle)
            }
            Order::AttackMove(_) => brain.target.is_none(),
            _ => false,
        };
        if !should_scan {
            continue;
        }

        let profile = opt_profile.copied().unwrap_or(TargetingProfile {
            distance_weight: 1.0,
            low_hp_weight: 0.2,
            threat_weight: 0.3,
            counter_weight: 0.2,
            building_penalty: 1.5,
            reserved_damage_penalty: 0.5,
        });
        let my_damage_type = DamageType::Melee; // refined when Ability carries damage_type tag

        let nearby = spatial.query_radius(tf.translation, scan_range);
        let mut best: Option<(Entity, f32)> = None;
        for (candidate, _cand_pos) in &nearby {
            if *candidate == entity {
                continue;
            }
            let Ok((cand_tf, cand_health, cand_armor, cand_faction, cand_threat, cand_reserved, cand_building)) =
                candidate_data.get(*candidate)
            else {
                continue;
            };
            let Some(cand_faction) = cand_faction else {
                continue;
            };
            if !teams.is_hostile(faction, cand_faction) {
                continue;
            }
            let armor = cand_armor.copied().unwrap_or(ArmorType::Light);
            let reserved = cand_reserved.map_or(0.0, |r| r.total());
            let threat = cand_threat.map_or(1.0, |t| t.0);
            let input = TargetScoreInput {
                profile: &profile,
                attacker_pos: tf.translation,
                attacker_damage_type: my_damage_type,
                scan_range,
                target_pos: cand_tf.translation,
                target_health: cand_health,
                target_armor: armor,
                target_threat: threat,
                target_is_building: cand_building.is_some(),
                target_reserved_damage: reserved,
            };
            if let Some(score) = target_score(&input) {
                if best.map_or(true, |(_, bs)| score < bs) {
                    best = Some((*candidate, score));
                }
            }
        }

        if let Some((target, _)) = best {
            brain.target = Some(target);
            brain.order_source = OrderSource::Auto;
            // If the standing order is Stop/Move/Hold, promote to Attack.
            // AttackMove keeps the original destination anchor.
            if !matches!(brain.order, Order::AttackMove(_)) {
                brain.order = Order::Attack(target);
                brain.issued_at = now;
            }
            brain.target_lock_until = now + tuning.auto_target_lock_secs as f64;
            brain.state = BrainState::Chasing;
            brain.chase_started_at = Some(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_health(current: f32, max: f32) -> Health {
        Health { current, max }
    }
    fn base_profile() -> TargetingProfile {
        TargetingProfile {
            distance_weight: 1.0,
            low_hp_weight: 1.0,
            threat_weight: 1.0,
            counter_weight: 1.0,
            building_penalty: 3.0,
            reserved_damage_penalty: 1.0,
        }
    }

    #[test]
    fn rejects_dead_target() {
        let profile = base_profile();
        let health = make_health(0.0, 100.0);
        assert!(target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 10.0,
            target_pos: Vec3::new(3.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Light,
            target_threat: 1.0,
            target_is_building: false,
            target_reserved_damage: 0.0,
        })
        .is_none());
    }

    #[test]
    fn prefers_closer_target() {
        let profile = base_profile();
        let health = make_health(80.0, 100.0);
        let close = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 15.0,
            target_pos: Vec3::new(2.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Light,
            target_threat: 1.0,
            target_is_building: false,
            target_reserved_damage: 0.0,
        })
        .unwrap();
        let far = target_score(&TargetScoreInput {
            profile: &profile,
            attacker_pos: Vec3::ZERO,
            attacker_damage_type: DamageType::Melee,
            scan_range: 15.0,
            target_pos: Vec3::new(12.0, 0.0, 0.0),
            target_health: &health,
            target_armor: ArmorType::Light,
            target_threat: 1.0,
            target_is_building: false,
            target_reserved_damage: 0.0,
        })
        .unwrap();
        assert!(close < far);
    }
}
