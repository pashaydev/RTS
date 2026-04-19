//! Retaliation — when a unit gets hit, flip to attacking the attacker unless
//! a manual order or commit is in progress. Reads `RecentCombatDamage` written
//! by `apply_damage`.

use bevy::prelude::*;
use bevy::time::Fixed;

use crate::types::{
    app::Faction, AppState, CombatTuning, OrderSource, RecentCombatDamage, TeamConfig,
    UnitStance,
};

use super::brain::{BrainState, CombatSet, Order, UnitBrain};

pub struct CombatRetaliationPlugin;

impl Plugin for CombatRetaliationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            retaliate_from_damage_memory
                .in_set(CombatSet::Targeting)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

pub fn retaliate_from_damage_memory(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    tuning: Res<CombatTuning>,
    teams: Res<TeamConfig>,
    mut attackers: Query<(
        Entity,
        &Faction,
        &RecentCombatDamage,
        &mut UnitBrain,
        Option<&UnitStance>,
    )>,
    attacker_factions: Query<&Faction>,
) {
    let now = time.elapsed_secs_f64();
    for (entity, my_faction, memory, mut brain, opt_stance) in &mut attackers {
        // Expired memory → drop it.
        if memory.expires_at <= now {
            commands.entity(entity).remove::<RecentCombatDamage>();
            continue;
        }
        // Passive units don't retaliate.
        let stance = opt_stance.copied().unwrap_or_default();
        if matches!(stance, UnitStance::Passive) {
            continue;
        }
        // Manual order overrides retaliation.
        if brain.order_source == OrderSource::Manual && !matches!(brain.order, Order::Stop) {
            continue;
        }
        if brain.is_committed() || brain.is_action_blocked() {
            continue;
        }
        // Lock window: don't thrash targets.
        if brain.target_lock_until > now {
            continue;
        }
        // If we're already attacking this or another valid target, keep it.
        if matches!(brain.order, Order::Attack(t) if t == memory.attacker) {
            continue;
        }
        // Attacker must still be hostile and known.
        let Ok(attacker_faction) = attacker_factions.get(memory.attacker) else {
            continue;
        };
        if !teams.is_hostile(my_faction, attacker_faction) {
            continue;
        }

        brain.order = Order::Attack(memory.attacker);
        brain.target = Some(memory.attacker);
        brain.order_source = OrderSource::Retaliate;
        brain.issued_at = now;
        brain.target_lock_until = now + tuning.retarget_grace_secs as f64;
        if matches!(brain.state, BrainState::Idle) {
            brain.state = BrainState::Chasing;
        }
        brain.chase_started_at = Some(now);
    }
}
