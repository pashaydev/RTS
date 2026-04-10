use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::IsRanged;
use crate::types::*;

use super::{attack_surface_distance, CombatBudgetState, CombatCoreSet};

pub struct CombatSlotsPlugin;

impl Plugin for CombatSlotsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (cleanup_stale_slot_claims, assign_melee_slot_claims)
                .chain()
                .in_set(GameFlowSet::Simulation)
                .before(CombatCoreSet::Approach)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

fn intended_attack_target(
    engagement: Option<&Engagement>,
    intent: Option<&CombatIntent>,
    target_lock: Option<&CombatTargetLock>,
) -> Option<Entity> {
    if let Some(engagement) = engagement {
        if let Some(target) = engagement.target {
            return Some(target);
        }
    }
    match intent {
        Some(CombatIntent::Attack(target, _)) => Some(*target),
        Some(CombatIntent::AttackMove(_, _)) => target_lock.map(|lock| lock.target),
        _ => None,
    }
}

pub fn slot_anchor(
    attacker: Entity,
    target_pos: Vec3,
    target_radius: f32,
    slot_index: u16,
    slot_count: usize,
    preferred_range: f32,
) -> Vec3 {
    let angle = slot_index as f32 / slot_count.max(1) as f32 * std::f32::consts::TAU
        + (attacker.to_bits() % 17) as f32 * 0.013;
    let stand_off = target_radius + preferred_range.max(0.9);
    target_pos + Vec3::new(angle.cos() * stand_off, 0.0, angle.sin() * stand_off)
}

fn cleanup_stale_slot_claims(
    mut commands: Commands,
    all_entities: Query<()>,
    attackers: Query<(
        Entity,
        &SlotClaim,
        Option<&Engagement>,
        Option<&CombatIntent>,
        Option<&CombatTargetLock>,
    )>,
) {
    for (entity, claim, engagement, intent, lock) in &attackers {
        let intended = intended_attack_target(engagement, intent, lock);
        if intended != Some(claim.target) || !all_entities.contains(claim.target) {
            commands.entity(entity).remove::<SlotClaim>();
        }
    }
}

fn assign_melee_slot_claims(
    mut commands: Commands,
    time: Res<Time>,
    budget: Res<CombatBudget>,
    mut budget_state: ResMut<CombatBudgetState>,
    mut slot_cache: ResMut<MeleeSlotCache>,
    attackers: Query<
        (
            Entity,
            &Transform,
            &AttackRange,
            Option<&AttackTiming>,
            Option<&Engagement>,
            Option<&CombatIntent>,
            Option<&CombatTargetLock>,
            Option<&SlotClaim>,
            Option<&MeleeContact>,
        ),
        (
            Or<(With<Unit>, With<Mob>)>,
            Without<IsRanged>,
            Without<AttackWindup>,
            Without<AttackRecovery>,
        ),
    >,
    target_positions: Query<&Transform>,
    target_footprints: Query<&BuildingFootprint, With<Building>>,
) {
    let now = time.elapsed_secs_f64();
    let mut per_target: HashMap<Entity, Vec<(Entity, Vec3, f32, Option<u16>)>> = HashMap::new();

    for (entity, tf, range, timing, engagement, intent, lock, claim, contact) in &attackers {
        let Some(target) = intended_attack_target(engagement, intent, lock) else {
            continue;
        };
        if contact.is_some_and(|contact| contact.target == target && now < contact.sticky_until) {
            continue;
        }
        if let Ok(target_tf) = target_positions.get(target) {
            let preferred_range = timing
                .map(|timing| timing.minimum_range.max(range.0 * 0.85))
                .unwrap_or(range.0.max(0.9));
            per_target.entry(target).or_default().push((
                entity,
                tf.translation,
                preferred_range,
                claim.map(|claim| claim.slot_index),
            ));
            slot_cache
                .active_targets
                .entry(target)
                .or_insert_with(|| MeleeSlotCacheEntry {
                    target_position: target_tf.translation,
                    last_update_time: now,
                    slot_count: 0,
                    occupancy_revision: 0,
                });
        }
    }

    for (target, attackers_for_target) in per_target {
        if budget_state.slot_refreshes_this_frame >= budget.max_slot_refreshes_per_frame {
            break;
        }
        let Ok(target_tf) = target_positions.get(target) else {
            continue;
        };
        let target_radius = target_footprints
            .get(target)
            .map_or(0.75, |fp| fp.0.max(0.75));
        let slot_count = ((((target_radius + 1.1) * std::f32::consts::TAU) / 1.15).round()
            as usize)
            .clamp(6, 24);
        let mut sorted = attackers_for_target;
        sorted.sort_by_key(|(entity, pos, _, existing)| {
            let existing_bias = existing.unwrap_or(u16::MAX);
            (
                existing_bias,
                ((pos.x - target_tf.translation.x).powi(2)
                    + (pos.z - target_tf.translation.z).powi(2)) as i32,
                entity.to_bits(),
            )
        });

        for (index, (entity, attacker_pos, preferred_range, _)) in sorted.iter().enumerate() {
            if index >= slot_count {
                break;
            }
            let slot_index = index as u16;
            let anchor = slot_anchor(
                *entity,
                target_tf.translation,
                target_radius,
                slot_index,
                slot_count,
                *preferred_range,
            );
            let surface_dist =
                attack_surface_distance(*attacker_pos, target_tf.translation, target_radius);
            if surface_dist > *preferred_range + 0.2 {
                commands.entity(*entity).insert(MoveTarget(anchor));
            }
            commands.entity(*entity).insert(SlotClaim {
                target,
                slot_index,
                last_valid_time: now,
            });
        }

        let entry = slot_cache.active_targets.entry(target).or_default();
        entry.target_position = target_tf.translation;
        entry.last_update_time = now;
        entry.slot_count = slot_count;
        entry.occupancy_revision = entry.occupancy_revision.wrapping_add(1);
        budget_state.slot_refreshes_this_frame += 1;
    }

    slot_cache.active_targets.retain(|target, entry| {
        target_positions.contains(*target) && now - entry.last_update_time < 2.0
    });
}
