//! Effect trees — composable damage/heal/status nodes applied by ability resolution.
//!
//! Effects are *data*, authored inline inside ability RON files. At runtime (Phase 2+),
//! `resolve_effect()` walks the tree, spawns projectiles, calls `apply_damage()`,
//! inserts `Behaviors` instances, etc. Every damage path still funnels through
//! `combat::damage::apply_damage` — the tree is a description, not a side-channel.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::types::{
    app::Faction, ArmorType, CombatFxKind, DamageApplied, DamageType, Health, Projectile,
    ReservedIncomingDamage, TeamConfig,
};
use crate::world::spatial::SpatialHashGrid;

use super::behavior::{BehaviorData, BehaviorInstance, BehaviorKind, Behaviors};
use super::damage::apply_damage;

/// Attached to projectiles spawned by `Effect::SpawnProjectile`. On impact,
/// `projectiles::tick_projectiles` emits a `PendingEffect` with this effect tree
/// so AoE / status / conditional damage still runs through `resolve_effect`.
#[derive(Component, Clone, Debug)]
pub struct ProjectileOnImpact(pub Effect);

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Effect {
    /// Apply a single damage instance to the current target.
    Damage {
        base: f32,
        damage_type: DamageType,
        #[serde(default)]
        falloff: bool,
    },
    /// Restore HP on the current target.
    Heal { amount: f32 },
    /// Insert a `Behavior` on the current target for `duration_secs`.
    ApplyBehavior {
        behavior: BehaviorData,
        duration_secs: f32,
    },
    /// Strip matching behaviors from the current target.
    Dispel { filter: DispelFilter },
    /// Apply positional displacement (knockback / pull / dash) on the current target.
    Displace {
        style: DisplaceStyle,
        strength: f32,
    },
    /// Replace the current target scope with every entity inside `radius` of the
    /// current target position that passes `filter`, then apply `child` to each.
    AreaSearch {
        radius: f32,
        filter: TargetFilter,
        child: Box<Effect>,
    },
    /// Apply effects in order. Fire-and-continue semantics.
    Sequence(Vec<Effect>),
    /// Apply `then` if `predicate` holds on the current target, else `otherwise`.
    Conditional {
        predicate: Predicate,
        then: Box<Effect>,
        #[serde(default)]
        otherwise: Option<Box<Effect>>,
    },
    /// Fire a travelling projectile at the current target; `on_impact` fires at hit.
    SpawnProjectile {
        speed: f32,
        #[serde(default)]
        orient_to_velocity: bool,
        on_impact: Box<Effect>,
    },
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum DispelFilter {
    All,
    Buffs,
    Debuffs,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum DisplaceStyle {
    Knockback,
    Pull,
    Dash,
}

/// Target filtering is unit-type based, not explicit Air/Ground tags (per project decision).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum TargetFilter {
    Hostile,
    Ally,
    AnyUnit,
    StructuresOnly,
    MobileOnly,
    SelfOnly,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Predicate {
    TargetIsUndead,
    TargetIsStructure,
    TargetBelowHpPct(u8),
}

/// The "current target" scope of an effect node. AreaSearch replaces the scope
/// per-candidate during recursion; Damage/Heal/ApplyBehavior apply to whatever
/// scope is active at their node.
#[derive(Clone, Copy, Debug)]
pub enum EffectTarget {
    Entity(Entity),
    Ground(Vec3),
    SelfOnly,
}

/// Bevy query aliases used by `resolve_effect`. Named so the recursive walker
/// can accept them as plain `&mut` parameters without a context struct (which
/// tangles `'world`/`'state` lifetimes awkwardly).
pub type HealthQ<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Health,
        Option<&'static ArmorType>,
        Option<&'static mut ReservedIncomingDamage>,
    ),
>;
pub type BehaviorsQ<'w, 's> = Query<'w, 's, &'static mut Behaviors>;
pub type TransformQ<'w, 's> = Query<'w, 's, &'static Transform>;
pub type FactionQ<'w, 's> = Query<'w, 's, &'static Faction>;

/// Walks the effect tree rooted at `effect` with the given scope, dispatching
/// each leaf to the appropriate backend. Same-tick, recursive. Damage always
/// routes through `apply_damage` — no side channel.
#[allow(clippy::too_many_arguments)]
pub fn resolve_effect(
    caster: Entity,
    caster_faction: Option<Faction>,
    scope: EffectTarget,
    effect: &Effect,
    commands: &mut Commands,
    now_secs: f64,
    damage_memory_secs: f32,
    teams: &TeamConfig,
    spatial: &SpatialHashGrid,
    damage_events: &mut MessageWriter<DamageApplied>,
    health_q: &mut HealthQ,
    behaviors_q: &mut BehaviorsQ,
    transform_q: &TransformQ,
    faction_q: &FactionQ,
) {
    match effect {
        Effect::Damage {
            base,
            damage_type,
            falloff: _,
        } => {
            let EffectTarget::Entity(target) = scope else {
                return;
            };
            let Ok((mut health, opt_armor, opt_reserved)) = health_q.get_mut(target) else {
                return;
            };
            let dealt = apply_damage(
                commands,
                target,
                Some(caster),
                *base,
                *damage_type,
                &mut health,
                opt_armor.copied(),
                opt_reserved.map(|r| r.into_inner()),
                now_secs,
                damage_memory_secs,
            );
            if dealt > 0.0 {
                damage_events.write(DamageApplied {
                    target,
                    source: Some(caster),
                    amount: dealt,
                    damage_type: *damage_type,
                    now_secs,
                });
            }
        }

        Effect::Heal { amount } => {
            let EffectTarget::Entity(target) = scope else {
                return;
            };
            if let Ok((mut health, _, _)) = health_q.get_mut(target) {
                health.current = (health.current + amount).min(health.max);
            }
        }

        Effect::ApplyBehavior {
            behavior,
            duration_secs,
        } => {
            let EffectTarget::Entity(target) = scope else {
                return;
            };
            let instance = BehaviorInstance {
                data: behavior.clone(),
                source: Some(caster),
                expires_at: now_secs + *duration_secs as f64,
            };
            let kind = behavior.kind();
            if let Ok(mut behaviors) = behaviors_q.get_mut(target) {
                behaviors.active.insert(kind, instance);
            } else {
                let mut behaviors = Behaviors::default();
                behaviors.active.insert(kind, instance);
                commands.entity(target).insert(behaviors);
            }
        }

        Effect::AreaSearch {
            radius,
            filter,
            child,
        } => {
            let center = match scope {
                EffectTarget::Entity(e) => transform_q.get(e).ok().map(|t| t.translation),
                EffectTarget::Ground(pos) => Some(pos),
                EffectTarget::SelfOnly => transform_q.get(caster).ok().map(|t| t.translation),
            };
            let Some(center) = center else { return };
            let candidates = spatial.query_radius(center, *radius);
            for (candidate, _pos) in &candidates {
                if *candidate == caster && !matches!(filter, TargetFilter::SelfOnly) {
                    continue;
                }
                if !passes_filter(caster, caster_faction.as_ref(), *candidate, *filter, teams, faction_q) {
                    continue;
                }
                resolve_effect(
                    caster,
                    caster_faction,
                    EffectTarget::Entity(*candidate),
                    child,
                    commands,
                    now_secs,
                    damage_memory_secs,
                    teams,
                    spatial,
                    damage_events,
                    health_q,
                    behaviors_q,
                    transform_q,
                    faction_q,
                );
            }
        }

        Effect::Sequence(steps) => {
            for step in steps {
                resolve_effect(
                    caster,
                    caster_faction,
                    scope,
                    step,
                    commands,
                    now_secs,
                    damage_memory_secs,
                    teams,
                    spatial,
                    damage_events,
                    health_q,
                    behaviors_q,
                    transform_q,
                    faction_q,
                );
            }
        }

        Effect::Conditional {
            predicate,
            then,
            otherwise,
        } => {
            let matched = eval_predicate(scope, predicate, health_q);
            let branch = if matched {
                Some(then.as_ref())
            } else {
                otherwise.as_deref()
            };
            if let Some(b) = branch {
                resolve_effect(
                    caster,
                    caster_faction,
                    scope,
                    b,
                    commands,
                    now_secs,
                    damage_memory_secs,
                    teams,
                    spatial,
                    damage_events,
                    health_q,
                    behaviors_q,
                    transform_q,
                    faction_q,
                );
            }
        }

        Effect::Dispel { filter } => {
            let EffectTarget::Entity(target) = scope else {
                return;
            };
            if let Ok(mut behaviors) = behaviors_q.get_mut(target) {
                behaviors
                    .active
                    .retain(|kind, _| !dispel_matches(*kind, *filter));
            }
        }

        Effect::Displace { style: _, strength: _ } => {
            // Wired with ability migration (Knight Charge).
        }

        Effect::SpawnProjectile {
            speed,
            orient_to_velocity,
            on_impact,
        } => {
            let caster_pos = match transform_q.get(caster) {
                Ok(t) => t.translation,
                Err(_) => return,
            };
            let (target_entity, target_pos) = match scope {
                EffectTarget::Entity(e) => match transform_q.get(e) {
                    Ok(t) => (e, t.translation),
                    Err(_) => return,
                },
                EffectTarget::Ground(pos) => (caster, pos),
                EffectTarget::SelfOnly => (caster, caster_pos),
            };
            let dir = target_pos - caster_pos;
            let dist = dir.length().max(0.001);
            let velocity = (dir / dist) * *speed;
            let lifetime = dist / speed.max(0.001) + 0.5;
            let proj_transform = Transform::from_translation(caster_pos + Vec3::Y * 0.8);
            commands.spawn((
                Projectile {
                    source: caster,
                    target: target_entity,
                    velocity,
                    speed: *speed,
                    damage: 0.0,
                    damage_type: DamageType::Pierce,
                    fx_kind: CombatFxKind::Pierce,
                    impact_scale: 1.0,
                    lifetime_secs: lifetime,
                    orient_to_velocity: *orient_to_velocity,
                },
                ProjectileOnImpact((**on_impact).clone()),
                proj_transform,
                Visibility::Hidden,
            ));
        }
    }
}

fn passes_filter(
    caster: Entity,
    caster_faction: Option<&Faction>,
    candidate: Entity,
    filter: TargetFilter,
    teams: &TeamConfig,
    faction_q: &FactionQ,
) -> bool {
    match filter {
        TargetFilter::AnyUnit => true,
        TargetFilter::SelfOnly => candidate == caster,
        TargetFilter::Hostile => match (caster_faction, faction_q.get(candidate).ok()) {
            (Some(my), Some(their)) => teams.is_hostile(my, their),
            _ => false,
        },
        TargetFilter::Ally => match (caster_faction, faction_q.get(candidate).ok()) {
            (Some(my), Some(their)) => my == their,
            _ => false,
        },
        // Structure/Mobile require component markers not yet declared; fall through.
        TargetFilter::StructuresOnly | TargetFilter::MobileOnly => true,
    }
}

fn eval_predicate(scope: EffectTarget, predicate: &Predicate, health_q: &HealthQ) -> bool {
    let EffectTarget::Entity(target) = scope else {
        return false;
    };
    match predicate {
        Predicate::TargetBelowHpPct(pct) => {
            if let Ok((health, _, _)) = health_q.get(target) {
                (health.current / health.max.max(1.0)) * 100.0 < *pct as f32
            } else {
                false
            }
        }
        Predicate::TargetIsUndead | Predicate::TargetIsStructure => false,
    }
}

#[inline]
fn dispel_matches(kind: BehaviorKind, filter: DispelFilter) -> bool {
    match filter {
        DispelFilter::All => true,
        DispelFilter::Buffs => matches!(
            kind,
            BehaviorKind::ChargeBuff | BehaviorKind::Invulnerable
        ),
        DispelFilter::Debuffs => matches!(
            kind,
            BehaviorKind::Slow | BehaviorKind::Stun | BehaviorKind::Rooted | BehaviorKind::BurningDot
        ),
    }
}
