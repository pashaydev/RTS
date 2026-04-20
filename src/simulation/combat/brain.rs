//! Core `UnitBrain` FSM: the `Order` enum, `BrainState`, order resolver,
//! and `CombatSet` pipeline ordering consumed by targeting/approach/ability.

use bevy::prelude::*;
use bevy::time::Fixed;

use crate::types::{AppState, MoveTarget, OrderSource, SimSet, UnitState};

use super::ability::{AbilityId, Abilities};

/// What the player or AI has commanded this unit to do. One canonical field,
/// replaces v1's five parallel intent/order/engagement components.
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub enum Order {
    #[default]
    Stop,
    Move(Vec3),
    Attack(Entity),
    AttackMove(Vec3),
    Hold,
    Cast(AbilityId, CastTarget),
    Follow(Entity),
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum CastTarget {
    SelfOnly,
    Entity(Entity),
    Ground(Vec3),
}

/// FSM phase. Drives animation (Windup/Channeling) and system gates
/// (Stunned blocks advance; Dying blocks everything).
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BrainState {
    #[default]
    Idle,
    Chasing,
    InRange,
    Windup,
    Impact,
    Recovery,
    CastPrep,
    Channeling,
    Stunned,
    Dying,
}

impl BrainState {
    /// Stable discriminant used in the lockstep checksum (Phase 5).
    #[inline]
    pub fn tag(self) -> u8 {
        match self {
            BrainState::Idle => 0,
            BrainState::Chasing => 1,
            BrainState::InRange => 2,
            BrainState::Windup => 3,
            BrainState::Impact => 4,
            BrainState::Recovery => 5,
            BrainState::CastPrep => 6,
            BrainState::Channeling => 7,
            BrainState::Stunned => 8,
            BrainState::Dying => 9,
        }
    }
}

/// Stable discriminant for the current `Order` — used in the lockstep checksum.
/// Ignores inner payload (target entity, Vec3); the goal is catching divergence
/// in *what kind* of order peers chose, not exact values.
impl Order {
    #[inline]
    pub fn tag(&self) -> u8 {
        match self {
            Order::Stop => 0,
            Order::Move(_) => 1,
            Order::Attack(_) => 2,
            Order::AttackMove(_) => 3,
            Order::Hold => 4,
            Order::Cast(_, _) => 5,
            Order::Follow(_) => 6,
        }
    }
}

#[allow(dead_code)]
#[derive(Component, Clone, Debug, Default)]
pub struct UnitBrain {
    pub order: Order,
    pub order_source: OrderSource,
    pub issued_at: f64,

    pub state: BrainState,
    pub target: Option<Entity>,
    /// Leash origin for auto-aggro / AttackMove destination / Hold position.
    pub anchor: Option<Vec3>,
    pub target_lock_until: f64,

    /// Ability currently winding up / casting (None = nothing active).
    pub active_ability: Option<AbilityId>,
    /// Remaining seconds of the current phase (windup / cast / recovery).
    pub phase_timer: f32,
    /// Ensures the damage-point effect fires exactly once per swing.
    pub damage_fired_this_swing: bool,

    /// Start time of current chase (for chase-timeout via `OrderSource::chase_timeout_secs`).
    pub chase_started_at: Option<f64>,

    /// Melee slot assignment — (target_entity, slot_index). Replaces v1 `SlotClaim`.
    pub slot_claim: Option<(Entity, u16)>,
}

impl UnitBrain {
    /// True while the unit is mid-commit (windup/impact/cast/channel) and must not
    /// have its movement orders or target swapped out from under it.
    #[inline]
    pub fn is_committed(&self) -> bool {
        matches!(
            self.state,
            BrainState::Windup
                | BrainState::Impact
                | BrainState::CastPrep
                | BrainState::Channeling
        )
    }

    #[inline]
    pub fn is_action_blocked(&self) -> bool {
        matches!(self.state, BrainState::Stunned | BrainState::Dying)
    }
}

/// Systems that tick per-entity combat timers and translate `Order` intent
/// into target/state/MoveTarget writes. Registered separately from the ability
/// pipeline so system ordering can interleave with the rest of the combat set.
pub struct CombatBrainPlugin;

/// Sub-sets for the combat pipeline — chained in order.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CombatSet {
    /// Tick phase timers, cooldowns, behavior durations.
    Timers,
    /// Translate `Order` → target/state + initial MoveTarget for Move orders.
    Orders,
    /// Select targets for AttackMove; ports of auto-aggro / retaliation.
    Targeting,
    /// Melee slot assignment.
    Slots,
    /// Movement toward slot anchor or attack band.
    Approach,
    /// Advance ability windup/cast phases, dispatch effects.
    Advance,
    /// Projectile motion; on impact, resolve_effect fires.
    Projectiles,
    /// Death bookkeeping, dying anim tick.
    Death,
}

impl Plugin for CombatBrainPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            FixedUpdate,
            (
                CombatSet::Timers,
                CombatSet::Orders,
                CombatSet::Targeting,
                CombatSet::Slots,
                CombatSet::Approach,
                CombatSet::Advance,
                CombatSet::Projectiles,
                CombatSet::Death,
            )
                .chain()
                .in_set(SimSet::Combat),
        )
        .add_systems(
            FixedUpdate,
            (
                tick_brain_timers.in_set(CombatSet::Timers),
                resolve_orders.in_set(CombatSet::Orders),
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Decrements `UnitBrain.phase_timer` and every `Abilities.cooldowns` entry.
/// Other timers on the unit (behavior expiry, target_lock_until) are absolute
/// deadlines compared against `now`, not ticked per frame.
pub fn tick_brain_timers(
    time: Res<Time<Fixed>>,
    mut brains: Query<(&mut UnitBrain, Option<&mut Abilities>)>,
) {
    let dt = time.delta_secs();
    for (mut brain, opt_abilities) in &mut brains {
        if brain.phase_timer > 0.0 {
            brain.phase_timer = (brain.phase_timer - dt).max(0.0);
        }
        if let Some(mut abilities) = opt_abilities {
            for cd in abilities.cooldowns.values_mut() {
                if *cd > 0.0 {
                    *cd = (*cd - dt).max(0.0);
                }
            }
        }
    }
}

/// Translates `Order` into `UnitBrain.target` + initial `MoveTarget` for pure
/// movement orders. Does NOT write approach positions for attack orders —
/// that's the `approach_target` system's job (runs later in `CombatSet::Approach`).
///
/// Non-attack `UnitState` variants (Gathering, Building, Depositing, …) own
/// their own movement; this system leaves them alone even when a combat Order
/// is present. A manual attack order breaks out of that by clearing the
/// non-combat state at the order entry point (not here).
pub fn resolve_orders(
    mut commands: Commands,
    all_entities: Query<()>,
    mut units: Query<(
        Entity,
        &mut UnitBrain,
        Option<&MoveTarget>,
        &Transform,
        Option<&UnitState>,
    )>,
) {
    for (entity, mut brain, current_move_target, transform, unit_state) in &mut units {
        // Don't disturb commits or dying units.
        if brain.is_committed() || matches!(brain.state, BrainState::Dying) {
            continue;
        }

        // Non-combat UnitState variants (Gather/Build/Deposit/AssignedGathering/...)
        // own their own movement via worker_fsm. Leave them alone unless the player
        // issues a combat order (Attack/AttackMove/Cast), in which case the order
        // entry point is expected to have already cleared the UnitState.
        if unit_state.map_or(false, |s| {
            !matches!(s, UnitState::Idle | UnitState::Moving(_) | UnitState::Attacking(_))
        }) && !matches!(
            brain.order,
            Order::Attack(_) | Order::AttackMove(_) | Order::Cast(_, _) | Order::Follow(_)
        ) {
            continue;
        }

        // If the current target vanished, drop it — transitions below will react.
        if let Some(target) = brain.target {
            if !all_entities.contains(target) {
                brain.target = None;
                brain.slot_claim = None;
                if matches!(brain.state, BrainState::Chasing | BrainState::InRange) {
                    brain.state = BrainState::Idle;
                }
            }
        }

        match brain.order.clone() {
            Order::Stop => {
                brain.target = None;
                brain.anchor = None;
                brain.slot_claim = None;
                if !matches!(brain.state, BrainState::Stunned) {
                    brain.state = BrainState::Idle;
                }
                if current_move_target.is_some() {
                    commands.entity(entity).remove::<MoveTarget>();
                }
            }
            Order::Move(dest) => {
                brain.target = None;
                brain.slot_claim = None;
                brain.state = BrainState::Idle;
                // Arrival check uses entity Transform, not MoveTarget presence
                // (the movement system removes MoveTarget once the unit stops).
                let dx = transform.translation.x - dest.x;
                let dz = transform.translation.z - dest.z;
                let dist_to_dest = (dx * dx + dz * dz).sqrt();
                if dist_to_dest < 0.8 {
                    // Reached destination — stop issuing move orders so the
                    // movement system doesn't re-path on every tick.
                    brain.order = Order::Stop;
                    if current_move_target.is_some() {
                        commands.entity(entity).remove::<MoveTarget>();
                    }
                } else if current_move_target.map_or(true, |m| m.0.distance(dest) > 0.6) {
                    commands.entity(entity).insert(MoveTarget(dest));
                }
            }
            Order::Attack(target) => {
                if all_entities.contains(target) {
                    brain.target = Some(target);
                    brain.anchor = None;
                    if matches!(brain.state, BrainState::Idle) {
                        brain.state = BrainState::Chasing;
                    }
                    // Approach + advance systems handle MoveTarget / state transitions.
                } else {
                    // Target died between order-write and resolve.
                    brain.target = None;
                    brain.order = Order::Stop;
                    brain.state = BrainState::Idle;
                }
            }
            Order::AttackMove(dest) => {
                brain.anchor = Some(dest);
                if brain.target.is_none() {
                    brain.state = BrainState::Idle;
                    // Roam toward dest; targeting system acquires enemies along the way.
                    if current_move_target.map_or(true, |m| m.0.distance(dest) > 0.6) {
                        commands.entity(entity).insert(MoveTarget(dest));
                    }
                } else if matches!(brain.state, BrainState::Idle) {
                    brain.state = BrainState::Chasing;
                }
            }
            Order::Hold => {
                brain.target = None;
                brain.slot_claim = None;
                brain.state = BrainState::Idle;
                if current_move_target.is_some() {
                    commands.entity(entity).remove::<MoveTarget>();
                }
            }
            Order::Cast(ability_id, cast_target) => {
                brain.active_ability = Some(ability_id);
                brain.state = BrainState::CastPrep;
                match cast_target {
                    CastTarget::Entity(e) if all_entities.contains(e) => {
                        brain.target = Some(e);
                    }
                    CastTarget::SelfOnly => {
                        brain.target = Some(entity);
                    }
                    CastTarget::Ground(pos) => {
                        brain.target = None;
                        brain.anchor = Some(pos);
                    }
                    CastTarget::Entity(_) => {
                        // Target gone; abort cast.
                        brain.order = Order::Stop;
                        brain.active_ability = None;
                        brain.state = BrainState::Idle;
                    }
                }
            }
            Order::Follow(target) => {
                if all_entities.contains(target) {
                    brain.target = Some(target);
                    if matches!(brain.state, BrainState::Idle) {
                        brain.state = BrainState::Chasing;
                    }
                } else {
                    brain.target = None;
                    brain.order = Order::Stop;
                    brain.state = BrainState::Idle;
                }
            }
        }
    }
}
