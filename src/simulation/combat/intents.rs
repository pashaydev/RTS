//! Order-entry API for external call-sites (selection, AI, mobs, UI, multiplayer).
//!
//! External code holds `&mut Commands` but typically not `&mut Query<UnitBrain>`,
//! so these helpers deferred-mutate `UnitBrain` via `Commands::queue`, writing
//! the canonical `Order` + `OrderSource` + `target_lock_until` fields. They run
//! inline on `Commands::apply` — no extra system, no component mirroring.
//!
//! Also hosts `CombatIntentsPlugin`, which initialises the combat tuning /
//! budget / slot-cache resources shared across the combat pipeline.

use bevy::prelude::*;

use crate::types::{
    CombatBudget, CombatStatsDebug, CombatTuning, MeleeSlotCache, OrderSource, TaskSource,
};

use super::brain::{BrainState, CastTarget, Order, UnitBrain};

/// Per-tick budget counters used by AI/combat systems to cap re-path & slot refresh work.
#[derive(Resource, Default)]
pub struct CombatBudgetState {
    pub repath_requests_this_frame: usize,
    pub slot_refreshes_this_frame: usize,
    pub target_rescans_this_frame: usize,
}

/// Initialises shared combat resources (tuning, budget counters, slot cache,
/// debug stats) and the per-tick budget-reset system. Registered as the first
/// combat plugin so dependent plugins find the resources already in place.
pub struct CombatIntentsPlugin;

impl Plugin for CombatIntentsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatTuning>()
            .init_resource::<CombatBudget>()
            .init_resource::<CombatStatsDebug>()
            .init_resource::<CombatBudgetState>()
            .init_resource::<MeleeSlotCache>()
            .add_systems(
                FixedUpdate,
                reset_combat_budget_state
                    .in_set(crate::types::SimSet::Command)
                    .run_if(in_state(crate::types::AppState::InGame)),
            );
    }
}

fn reset_combat_budget_state(mut state: ResMut<CombatBudgetState>) {
    state.repath_requests_this_frame = 0;
    state.slot_refreshes_this_frame = 0;
    state.target_rescans_this_frame = 0;
}

const DEFAULT_MANUAL_TARGET_LOCK_SECS: f64 = 1.5;
const DEFAULT_AUTO_TARGET_LOCK_SECS: f64 = 1.1;

fn queue_set_order(
    commands: &mut Commands,
    entity: Entity,
    order: Order,
    source: OrderSource,
    issue_time: f64,
    lock_secs: f64,
) {
    let order_for_closure = order.clone();
    commands.queue(move |world: &mut World| {
        let target = match &order_for_closure {
            Order::Attack(t) | Order::Follow(t) => Some(*t),
            _ => None,
        };
        let anchor = match &order_for_closure {
            Order::Move(p) | Order::AttackMove(p) => Some(*p),
            _ => None,
        };
        let Some(mut brain) = world.get_mut::<UnitBrain>(entity) else {
            return;
        };
        brain.order = order_for_closure;
        brain.order_source = source;
        brain.issued_at = issue_time;
        brain.target = target;
        brain.anchor = anchor;
        if target.is_some() {
            brain.target_lock_until = issue_time + lock_secs;
        }
        // resolve_orders recomputes BrainState from Order; for stop/hold we
        // short-circuit to Idle so in-flight animation doesn't linger.
        if matches!(brain.order, Order::Stop | Order::Hold) {
            if !brain.is_action_blocked() && !matches!(brain.state, BrainState::Dying) {
                brain.state = BrainState::Idle;
            }
        }
    });
}

pub fn apply_manual_move_intent(
    commands: &mut Commands,
    entity: Entity,
    destination: Vec3,
    issue_time: f64,
) {
    commands.entity(entity).insert(TaskSource::Manual);
    queue_set_order(
        commands,
        entity,
        Order::Move(destination),
        OrderSource::Manual,
        issue_time,
        DEFAULT_MANUAL_TARGET_LOCK_SECS,
    );
}

pub fn apply_manual_attack_intent(
    commands: &mut Commands,
    entity: Entity,
    target: Entity,
    issue_time: f64,
) {
    commands.entity(entity).insert(TaskSource::Manual);
    queue_set_order(
        commands,
        entity,
        Order::Attack(target),
        OrderSource::Manual,
        issue_time,
        DEFAULT_MANUAL_TARGET_LOCK_SECS,
    );
}

pub fn apply_manual_cast_intent(
    commands: &mut Commands,
    entity: Entity,
    ability: super::ability::AbilityId,
    target: CastTarget,
    issue_time: f64,
) {
    commands.entity(entity).insert(TaskSource::Manual);
    queue_set_order(
        commands,
        entity,
        Order::Cast(ability, target),
        OrderSource::Manual,
        issue_time,
        DEFAULT_MANUAL_TARGET_LOCK_SECS,
    );
}

pub fn apply_manual_attack_move_intent(
    commands: &mut Commands,
    entity: Entity,
    destination: Vec3,
    issue_time: f64,
) {
    commands.entity(entity).insert(TaskSource::Manual);
    queue_set_order(
        commands,
        entity,
        Order::AttackMove(destination),
        OrderSource::Manual,
        issue_time,
        DEFAULT_MANUAL_TARGET_LOCK_SECS,
    );
}

pub fn apply_manual_hold_intent(commands: &mut Commands, entity: Entity, issue_time: f64) {
    commands.entity(entity).insert(TaskSource::Manual);
    queue_set_order(
        commands,
        entity,
        Order::Hold,
        OrderSource::Manual,
        issue_time,
        DEFAULT_MANUAL_TARGET_LOCK_SECS,
    );
}

pub fn clear_combat_intent(commands: &mut Commands, entity: Entity, issue_time: f64) {
    queue_set_order(
        commands,
        entity,
        Order::Stop,
        OrderSource::Manual,
        issue_time,
        0.0,
    );
}

pub fn reset_combat_state(commands: &mut Commands, entity: Entity) {
    queue_set_order(
        commands,
        entity,
        Order::Stop,
        OrderSource::Auto,
        0.0,
        0.0,
    );
}

pub fn apply_auto_move_intent(commands: &mut Commands, entity: Entity, destination: Vec3) {
    commands.entity(entity).insert(TaskSource::Auto);
    queue_set_order(
        commands,
        entity,
        Order::Move(destination),
        OrderSource::Auto,
        0.0,
        DEFAULT_AUTO_TARGET_LOCK_SECS,
    );
}

pub fn apply_auto_attack_intent(
    commands: &mut Commands,
    entity: Entity,
    target: Entity,
    anchor: Vec3,
    issue_time: f64,
) {
    commands
        .entity(entity)
        .insert(TaskSource::Auto)
        .insert(crate::types::LeashOrigin(anchor));
    queue_set_order(
        commands,
        entity,
        Order::Attack(target),
        OrderSource::Auto,
        issue_time,
        DEFAULT_AUTO_TARGET_LOCK_SECS,
    );
}

/// Retarget a unit *without* changing its current `Order`. Used by HoldPosition
/// and AttackMove auto-aggro scans that pick a new target but leave the standing
/// order intact.
pub fn set_auto_target(commands: &mut Commands, entity: Entity, target: Entity, issue_time: f64) {
    commands.queue(move |world: &mut World| {
        if let Some(mut brain) = world.get_mut::<UnitBrain>(entity) {
            brain.target = Some(target);
            brain.target_lock_until = issue_time + DEFAULT_AUTO_TARGET_LOCK_SECS;
        }
    });
}
