//! Leash — auto-engaged units return to their leash origin when dragged
//! too far from it. Manual orders bypass leashing.

use bevy::prelude::*;

use crate::types::{AppState, CombatTuning, LeashOrigin, Mob, Unit, UnitStance};

use super::brain::{BrainState, CombatSet, Order, UnitBrain};
use crate::types::OrderSource;

pub struct CombatLeashPlugin;

impl Plugin for CombatLeashPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            leash_return
                .in_set(CombatSet::Orders)
                .after(super::brain::resolve_orders)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Neutral mobs have their own patrol/leash logic in `mobs.rs::mob_leash`
/// tuned for a much larger roam radius (40u) than the player-stance defaults
/// (12u Defensive). Running both leash systems on mobs causes them to abandon
/// the chase after moving ~12u and re-aggro on the next tick — the "orbit
/// and wander" behavior reported in playtests. Only player-owned units pass
/// through `CombatTuning`-driven leashing.
#[allow(clippy::type_complexity)]
pub fn leash_return(
    mut commands: Commands,
    tuning: Res<CombatTuning>,
    mut units: Query<
        (
            Entity,
            &Transform,
            &LeashOrigin,
            &mut UnitBrain,
            Option<&UnitStance>,
        ),
        (With<Unit>, Without<Mob>),
    >,
) {
    for (entity, tf, origin, mut brain, opt_stance) in &mut units {
        // Only auto-sourced engagements are leash-bounded.
        if brain.order_source != OrderSource::Auto {
            continue;
        }
        if !matches!(brain.order, Order::Attack(_) | Order::AttackMove(_)) {
            continue;
        }
        let stance = opt_stance.copied().unwrap_or_default();
        let leash_distance = tuning.leash_distance(stance);
        if leash_distance <= 0.0 {
            continue;
        }
        let dist = tf.translation.distance(origin.0);
        if dist <= leash_distance {
            continue;
        }
        // Past the leash: drop target, walk back.
        brain.order = Order::Move(origin.0);
        brain.target = None;
        brain.slot_claim = None;
        brain.chase_started_at = None;
        brain.state = BrainState::Idle;
        commands.entity(entity).remove::<LeashOrigin>();
    }
}
