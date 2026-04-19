//! Behaviors — transient status on target entities (slow, stun, DoT, buff).
//!
//! Replaces v1's `StatusEffects` + `ChargeBonus`. Each `BehaviorInstance` is owned by
//! the target entity via the `Behaviors` component and ticked in `FixedUpdate`.

use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy::time::Fixed;
use serde::{Deserialize, Serialize};

use crate::types::{
    ArmorType, AppState, CombatTuning, DamageApplied, DamageType, Health, ReservedIncomingDamage,
    SimSet,
};

use super::damage::apply_damage;

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BehaviorData {
    /// Multiplies unit movement speed by `factor` (0.0–1.0).
    Slow { factor: f32 },
    /// Blocks ability advance and movement orders.
    Stun,
    /// Blocks movement but allows attacks.
    Rooted,
    /// Multiplies outgoing damage by `mult` for the duration.
    ChargeBuff { mult: f32 },
    /// All incoming damage is absorbed while active.
    Invulnerable,
    /// Applies `dps` damage per second via `apply_damage` (sourceless).
    BurningDot { dps: f32, damage_type: DamageType },
    /// Marked by another entity — targeting-priority hint, no mechanical effect on its own.
    Marked,
}

impl BehaviorData {
    #[inline]
    pub fn kind(&self) -> BehaviorKind {
        match self {
            BehaviorData::Slow { .. } => BehaviorKind::Slow,
            BehaviorData::Stun => BehaviorKind::Stun,
            BehaviorData::Rooted => BehaviorKind::Rooted,
            BehaviorData::ChargeBuff { .. } => BehaviorKind::ChargeBuff,
            BehaviorData::Invulnerable => BehaviorKind::Invulnerable,
            BehaviorData::BurningDot { .. } => BehaviorKind::BurningDot,
            BehaviorData::Marked => BehaviorKind::Marked,
        }
    }
}

/// Stable enum tag used as BTreeMap key (Ord for deterministic iteration in lockstep).
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Serialize, Deserialize)]
pub enum BehaviorKind {
    Slow,
    Stun,
    Rooted,
    ChargeBuff,
    Invulnerable,
    BurningDot,
    Marked,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct BehaviorInstance {
    pub data: BehaviorData,
    pub source: Option<Entity>,
    pub expires_at: f64,
}

/// Replaces v1 `StatusEffects`. BTreeMap keyed by `BehaviorKind` — collisions overwrite
/// (re-applying Slow refreshes duration), which matches the current v1 intent.
#[allow(dead_code)]
#[derive(Component, Clone, Debug, Default)]
pub struct Behaviors {
    pub active: BTreeMap<BehaviorKind, BehaviorInstance>,
}

#[allow(dead_code)]
impl Behaviors {
    #[inline]
    pub fn has(&self, kind: BehaviorKind) -> bool {
        self.active.contains_key(&kind)
    }

    #[inline]
    pub fn get(&self, kind: BehaviorKind) -> Option<&BehaviorInstance> {
        self.active.get(&kind)
    }

    /// Product of all active `Slow` factors (≤ 1.0). 1.0 if none active.
    pub fn movement_multiplier(&self) -> f32 {
        if let Some(inst) = self.active.get(&BehaviorKind::Slow) {
            if let BehaviorData::Slow { factor } = inst.data {
                return factor.clamp(0.0, 1.0);
            }
        }
        1.0
    }

    #[inline]
    pub fn is_stunned(&self) -> bool {
        self.has(BehaviorKind::Stun)
    }

    #[inline]
    pub fn is_rooted(&self) -> bool {
        self.has(BehaviorKind::Rooted) || self.has(BehaviorKind::Stun)
    }
}

pub struct CombatBehaviorPlugin;

impl Plugin for CombatBehaviorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            tick_behaviors
                .in_set(SimSet::Combat)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Ticks behavior durations, applies DoTs (`BurningDot`), and removes expired
/// entries. Runs every fixed tick; iteration is deterministic (BTreeMap).
pub fn tick_behaviors(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    tuning: Res<CombatTuning>,
    mut damage_events: MessageWriter<DamageApplied>,
    mut targets: Query<(
        Entity,
        &mut Behaviors,
        &mut Health,
        Option<&ArmorType>,
        Option<&mut ReservedIncomingDamage>,
    )>,
) {
    let now = time.elapsed_secs_f64();
    let dt = time.delta_secs();
    let damage_memory = tuning.damage_memory_secs;

    for (entity, mut behaviors, mut health, opt_armor, opt_reserved) in &mut targets {
        // Apply DoT before expiry check so the final partial tick still hits.
        let dot = behaviors
            .active
            .get(&BehaviorKind::BurningDot)
            .map(|inst| (inst.source, inst.data.clone()));
        if let Some((source, BehaviorData::BurningDot { dps, damage_type })) = dot {
            let dealt = apply_damage(
                &mut commands,
                entity,
                source,
                dps * dt,
                damage_type,
                &mut health,
                opt_armor.copied(),
                opt_reserved.map(|r| r.into_inner()),
                now,
                damage_memory,
            );
            if dealt > 0.0 {
                damage_events.write(DamageApplied {
                    target: entity,
                    source,
                    amount: dealt,
                    damage_type,
                    now_secs: now,
                });
            }
        }

        // Drop expired.
        behaviors.active.retain(|_, inst| inst.expires_at > now);
    }
}
