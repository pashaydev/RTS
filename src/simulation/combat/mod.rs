//! Combat pipeline.
//!
//! Every unit carries a `UnitBrain` that holds the current `Order` + `BrainState`
//! FSM. External call-sites (selection clicks, AI, mobs, UI, multiplayer replay)
//! issue orders via the helpers in `intents.rs`, which deferred-mutate
//! `UnitBrain` through `Commands::queue`. Internal combat systems own direct
//! `&mut UnitBrain` queries and run in the `CombatSet` chain configured by
//! `CombatBrainPlugin`:
//!
//!   Timers → Orders → Targeting → Slots → Approach → Advance → Projectiles → Death
//!
//! Files:
//!   - `brain.rs`       — `UnitBrain`, `Order`, `BrainState`, `CombatSet`, order resolver
//!   - `intents.rs`     — order-entry helpers for call-sites + combat resource init plugin
//!   - `ability.rs`     — `Ability` RON assets + windup/impact/recovery dispatch
//!   - `behavior.rs`    — status-effect / buff container
//!   - `effect.rs`      — declarative effect execution (damage/heal/displace/dispel)
//!   - `targeting.rs`   — scoring + auto-aggro / AttackMove target acquisition
//!   - `approach.rs`    — melee slot claim + movement toward the attack band
//!   - `retaliation.rs` — "when hit, hit back" reflex
//!   - `leash.rs`       — return to anchor when a chase goes stale
//!   - `projectiles.rs` — in-flight projectile motion + on-impact effect dispatch
//!   - `damage.rs`      — shared `apply_damage` helper
//!   - `death.rs`       — Dying state + cleanup bookkeeping

mod ability;
mod approach;
mod behavior;
mod brain;
mod damage;
mod death;
mod effect;
mod intents;
mod leash;
mod projectiles;
mod retaliation;
mod targeting;

// Order-entry helpers — the canonical way external code issues combat orders.
// All functions deferred-mutate `UnitBrain` via `Commands::queue`.
pub use intents::{
    apply_auto_attack_intent, apply_auto_move_intent, apply_manual_attack_intent,
    apply_manual_attack_move_intent, apply_manual_hold_intent, apply_manual_move_intent,
    clear_combat_intent, reset_combat_state, set_auto_target, CombatBudgetState,
    CombatIntentsPlugin,
};

// Core brain: order/state machine + pipeline ordering.
pub use brain::{BrainState, CastTarget, CombatBrainPlugin, CombatSet, Order, UnitBrain};

// Ability pipeline: definitions, registry, runtime windup/impact/recovery.
pub use ability::{
    Abilities, Ability, AbilityId, AbilityRegistry, AbilityRegistryPlugin, CombatAbilityPlugin,
    PendingEffect, TargetType,
};
pub use behavior::{BehaviorData, BehaviorInstance, BehaviorKind, Behaviors, CombatBehaviorPlugin};
pub use effect::{DispelFilter, DisplaceStyle, Effect, EffectTarget, Predicate, TargetFilter};

// Targeting / approach / reflex / leash / projectiles / death.
pub use approach::{attack_surface_distance, is_in_attack_band, CombatApproachPlugin};
pub use damage::apply_damage;
pub use death::CombatDeathPlugin;
pub use leash::CombatLeashPlugin;
pub use projectiles::CombatProjectilesPlugin;
pub use retaliation::CombatRetaliationPlugin;
pub use targeting::{target_score, CombatTargetingPlugin, TargetScoreInput};
