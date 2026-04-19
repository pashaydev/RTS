//! Ability — castable action, loaded from RON at startup.
//!
//! Every unit carries `Abilities { default_ability, castable, cooldowns }`. Auto-attack
//! IS the default ability; abilities UI buttons trigger `castable` entries. The runtime
//! resolver (Phase 2+) reads `Ability.windup_frames` etc. to drive `UnitBrain` phases.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::time::Fixed;
use serde::{Deserialize, Serialize};

use crate::types::{
    app::Faction, AppState, CombatTuning, DamageApplied, Health, TeamConfig,
};
use crate::world::spatial::SpatialHashGrid;

use super::approach::is_in_attack_band;
use super::behavior::Behaviors;
use super::brain::{BrainState, CombatSet, UnitBrain};
use super::effect::{
    resolve_effect, BehaviorsQ, Effect, EffectTarget, FactionQ, HealthQ, TargetFilter, TransformQ,
};

/// String-keyed ability id. Loaded from RON files; key = filename stem.
/// Serialized transparently as a plain string so RON reads `id: "auto_melee_sword"`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AbilityId(pub String);

impl AbilityId {
    #[inline]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for AbilityId {
    fn from(s: &str) -> Self {
        AbilityId::new(s.to_owned())
    }
}

impl From<String> for AbilityId {
    fn from(s: String) -> Self {
        AbilityId::new(s)
    }
}

impl std::fmt::Display for AbilityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub enum TargetType {
    #[default]
    None,
    SelfOnly,
    Unit,
    Ground,
}

/// Authoring FPS for frame-based windup timing. Chosen to match the `FixedUpdate`
/// cadence (30 Hz) so authored frames map 1:1 to sim ticks. Animation clips are
/// time-scaled at playback so the authored frame visually lines up.
pub const ANIM_FPS: f32 = 30.0;

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ability {
    pub id: AbilityId,
    pub display_name: String,

    /// Max attack/cast range in world units.
    #[serde(default)]
    pub range: f32,
    /// Catapult-style min range (0.0 = no minimum).
    #[serde(default)]
    pub min_range: f32,

    /// Windup animation length in frames.
    #[serde(default = "default_one_u16")]
    pub windup_frames: u16,
    /// Recovery animation length in frames (post-damage).
    #[serde(default)]
    pub recovery_frames: u16,
    /// Frame within windup at which the damage fires (0..=windup_frames).
    /// Authoring convenience: "sword connects at frame 12 of 30".
    #[serde(default)]
    pub damage_point_frame: u16,

    /// Cooldown before this ability can be cast again on this unit.
    #[serde(default)]
    pub cooldown_secs: f32,
    /// Pre-windup cast time (for channelled/ritual abilities). 0 for instant/auto-attack.
    #[serde(default)]
    pub cast_time_secs: f32,

    #[serde(default)]
    pub target_type: TargetType,
    /// Filters candidates when auto-picking a target or validating a cast.
    #[serde(default = "default_filter")]
    pub target_filter: TargetFilter,

    /// Map of resource name → cost (e.g. `"Gold": 15`). Empty = free.
    #[serde(default)]
    pub resource_cost: BTreeMap<String, u32>,

    /// Effect tree fired at `damage_point_frame`.
    pub effect: Effect,
}

fn default_one_u16() -> u16 {
    1
}
fn default_filter() -> TargetFilter {
    TargetFilter::Hostile
}

#[allow(dead_code)]
impl Ability {
    #[inline]
    pub fn windup_secs(&self) -> f32 {
        self.windup_frames as f32 / ANIM_FPS
    }

    #[inline]
    pub fn recovery_secs(&self) -> f32 {
        self.recovery_frames as f32 / ANIM_FPS
    }

    /// Fraction of windup at which damage fires. 1.0 = end of windup.
    #[inline]
    pub fn damage_point_frac(&self) -> f32 {
        if self.windup_frames == 0 {
            1.0
        } else {
            (self.damage_point_frame as f32 / self.windup_frames as f32).clamp(0.0, 1.0)
        }
    }
}

/// Per-unit ability loadout. Replaces v1 `UnitAbilities`.
#[allow(dead_code)]
#[derive(Component, Clone, Debug)]
pub struct Abilities {
    /// The unit's auto-attack — fired in `BrainState::Idle/InRange` when a target is in band.
    pub default_ability: AbilityId,
    /// Player-triggered castable abilities (shown in UI buttons).
    pub castable: Vec<AbilityId>,
    /// Remaining cooldown per ability. BTreeMap for deterministic iteration (lockstep).
    pub cooldowns: BTreeMap<AbilityId, f32>,
}

#[allow(dead_code)]
impl Abilities {
    pub fn new(default_ability: AbilityId, castable: Vec<AbilityId>) -> Self {
        Self {
            default_ability,
            castable,
            cooldowns: BTreeMap::new(),
        }
    }

    #[inline]
    pub fn is_ready(&self, id: &AbilityId) -> bool {
        self.cooldowns.get(id).copied().unwrap_or(0.0) <= 0.0
    }

    pub fn set_cooldown(&mut self, id: AbilityId, secs: f32) {
        self.cooldowns.insert(id, secs.max(0.0));
    }
}

#[derive(Resource, Default)]
pub struct AbilityRegistry {
    abilities: BTreeMap<AbilityId, Arc<Ability>>,
}

#[allow(dead_code)]
impl AbilityRegistry {
    #[inline]
    pub fn get(&self, id: &AbilityId) -> Option<Arc<Ability>> {
        self.abilities.get(id).cloned()
    }

    #[inline]
    pub fn contains(&self, id: &AbilityId) -> bool {
        self.abilities.contains_key(id)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.abilities.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.abilities.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&AbilityId, &Arc<Ability>)> {
        self.abilities.iter()
    }
}

pub struct AbilityRegistryPlugin;

impl Plugin for AbilityRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AbilityRegistry>()
            .add_systems(Startup, load_abilities_from_ron);
    }
}

const ABILITY_DIR: &str = "assets/combat/abilities";

fn load_abilities_from_ron(mut registry: ResMut<AbilityRegistry>) {
    let dir = std::path::Path::new(ABILITY_DIR);
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            warn!("combat/abilities RON dir not found at {:?}: {}", dir, err);
            return;
        }
    };

    // Sort paths for deterministic load order.
    let mut paths: Vec<std::path::PathBuf> = entries
        .filter_map(|r| r.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ron"))
        .collect();
    paths.sort();

    for path in paths {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read ability RON {:?}: {}", path, e));
        let ability: Ability = ron::from_str(&text)
            .unwrap_or_else(|e| panic!("parse ability RON {:?}: {}", path, e));
        let id = ability.id.clone();
        if registry
            .abilities
            .insert(id.clone(), Arc::new(ability))
            .is_some()
        {
            panic!("duplicate ability id {:?} (file {:?})", id, path);
        }
    }

    info!("Combat: loaded {} abilities from RON", registry.abilities.len());
}

// ─────────────────────────────────────────────────────────────────────────────
// Runtime: ability phase advance + deferred effect application
// ─────────────────────────────────────────────────────────────────────────────

/// Emitted when an ability's `damage_point_frame` is crossed. Consumed by
/// `apply_pending_effects` same-tick, which walks the Effect tree.
///
/// Lives as a message (not a direct call) so `advance_ability_phase` can keep
/// its write queries light; the resolver owns the heavy Health/Behaviors mutations.
#[derive(Message, Clone, Debug)]
pub struct PendingEffect {
    pub caster: Entity,
    pub caster_faction: Option<Faction>,
    pub target: EffectTarget,
    pub effect: Effect,
}

pub struct CombatAbilityPlugin;

impl Plugin for CombatAbilityPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PendingEffect>().add_systems(
            FixedUpdate,
            (
                advance_ability_phase.in_set(CombatSet::Advance),
                apply_pending_effects
                    .in_set(CombatSet::Advance)
                    .after(advance_ability_phase),
            )
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Core FSM advance: starts a swing when idle-and-ready, ticks windup, fires
/// the effect at `damage_point_frac`, transitions into recovery, and returns
/// to Idle. Also handles `CastPrep` pre-windup for abilities with cast time.
pub fn advance_ability_phase(
    registry: Res<AbilityRegistry>,
    tuning: Res<CombatTuning>,
    mut pending: MessageWriter<PendingEffect>,
    mut attackers: Query<(
        Entity,
        &Transform,
        &Faction,
        &mut UnitBrain,
        &mut Abilities,
        Option<&Behaviors>,
    )>,
    targets: Query<(&Transform, &Health)>,
    target_footprints: Query<&crate::types::BuildingFootprint, With<crate::types::Building>>,
) {
    let tolerance = tuning.range_stay_buffer.max(0.5);
    for (entity, tf, faction, mut brain, mut abilities, opt_behaviors) in &mut attackers {
        // Stun / dying: no advance.
        if brain.is_action_blocked() || opt_behaviors.map_or(false, |b| b.is_stunned()) {
            continue;
        }

        match brain.state {
            BrainState::CastPrep => {
                if brain.phase_timer <= 0.0 {
                    if let Some(ability_id) = brain.active_ability.clone() {
                        if let Some(ability) = registry.get(&ability_id) {
                            brain.state = BrainState::Windup;
                            brain.phase_timer = ability.windup_secs();
                            brain.damage_fired_this_swing = false;
                        } else {
                            brain.state = BrainState::Idle;
                            brain.active_ability = None;
                        }
                    } else {
                        brain.state = BrainState::Idle;
                    }
                }
            }

            BrainState::Windup => {
                let Some(ability_id) = brain.active_ability.clone() else {
                    brain.state = BrainState::Idle;
                    continue;
                };
                let Some(ability) = registry.get(&ability_id) else {
                    brain.state = BrainState::Idle;
                    brain.active_ability = None;
                    continue;
                };

                let windup = ability.windup_secs();
                let elapsed = (windup - brain.phase_timer).max(0.0);
                let frac = if windup > 0.0 { elapsed / windup } else { 1.0 };

                let mut should_fire = !brain.damage_fired_this_swing
                    && frac >= ability.damage_point_frac();

                // Windup expiring without having fired (edge: damage_point_frac == 1.0
                // and we crossed 0 before matching ≥). Force-fire.
                if brain.phase_timer <= 0.0 && !brain.damage_fired_this_swing {
                    should_fire = true;
                }

                if should_fire {
                    let effect_target = if let Some(t) = brain.target {
                        // Confirm target alive before we fire. Missing/dead = abort swing.
                        if targets.get(t).ok().map_or(true, |(_, h)| h.current <= 0.0) {
                            None
                        } else {
                            Some(EffectTarget::Entity(t))
                        }
                    } else if let Some(pos) = brain.anchor {
                        Some(EffectTarget::Ground(pos))
                    } else {
                        None
                    };

                    if let Some(et) = effect_target {
                        pending.write(PendingEffect {
                            caster: entity,
                            caster_faction: Some(*faction),
                            target: et,
                            effect: ability.effect.clone(),
                        });
                    }
                    brain.damage_fired_this_swing = true;
                }

                if brain.phase_timer <= 0.0 {
                    brain.state = BrainState::Recovery;
                    brain.phase_timer = ability.recovery_secs();
                    brain.active_ability = None;
                }
            }

            BrainState::Recovery => {
                if brain.phase_timer <= 0.0 {
                    brain.state = BrainState::Idle;
                    brain.damage_fired_this_swing = false;
                }
            }

            BrainState::Idle | BrainState::InRange | BrainState::Chasing => {
                // Swing start gate: target alive, ability ready, in range.
                let Some(target) = brain.target else { continue };
                let Ok((target_tf, target_health)) = targets.get(target) else {
                    brain.target = None;
                    continue;
                };
                if target_health.current <= 0.0 {
                    brain.target = None;
                    continue;
                }

                // Active ability (from Order::Cast) overrides default auto-attack.
                let ability_id = brain
                    .active_ability
                    .clone()
                    .unwrap_or_else(|| abilities.default_ability.clone());

                if !abilities.is_ready(&ability_id) {
                    continue;
                }
                let Some(ability) = registry.get(&ability_id) else {
                    continue;
                };

                // 2D surface-to-surface distance check (ignore terrain height).
                // Accounts for the target's building footprint so attackers can
                // swing at a wall without needing to stand inside it.
                let dx = target_tf.translation.x - tf.translation.x;
                let dz = target_tf.translation.z - tf.translation.z;
                let center_dist = (dx * dx + dz * dz).sqrt();
                let target_radius = target_footprints.get(target).map_or(0.0, |fp| fp.0);
                let surface_dist = (center_dist - target_radius).max(0.0);

                // Match approach_target's attack-band tolerance so the two systems
                // never disagree about whether the unit is in range. approach_target
                // owns `Chasing ↔ InRange` transitions; advance only reads state
                // and either starts a swing (in band, ready) or waits (out of band).
                if !is_in_attack_band(surface_dist, ability.range, ability.min_range, tolerance) {
                    continue;
                }

                // In range: mark InRange and start the swing.
                brain.state = if ability.cast_time_secs > 0.0 {
                    BrainState::CastPrep
                } else {
                    BrainState::Windup
                };
                brain.phase_timer = if ability.cast_time_secs > 0.0 {
                    ability.cast_time_secs
                } else {
                    ability.windup_secs()
                };
                brain.active_ability = Some(ability_id.clone());
                brain.damage_fired_this_swing = false;
                abilities.set_cooldown(ability_id, ability.cooldown_secs);
            }

            _ => {}
        }
    }
}

/// Drains `PendingEffect` messages and walks each effect tree same-tick.
/// Runs after `advance_ability_phase` in the `Advance` set.
pub fn apply_pending_effects(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    tuning: Res<CombatTuning>,
    teams: Res<TeamConfig>,
    spatial: Res<SpatialHashGrid>,
    mut damage_events: MessageWriter<DamageApplied>,
    mut pending: MessageReader<PendingEffect>,
    mut health_q: HealthQ,
    mut behaviors_q: BehaviorsQ,
    transform_q: TransformQ,
    faction_q: FactionQ,
) {
    let now = time.elapsed_secs_f64();
    let mem = tuning.damage_memory_secs;

    // MessageReader borrows mutably, so snapshot first.
    let msgs: Vec<PendingEffect> = pending.read().cloned().collect();
    for msg in msgs {
        resolve_effect(
            msg.caster,
            msg.caster_faction,
            msg.target,
            &msg.effect,
            &mut commands,
            now,
            mem,
            &teams,
            &spatial,
            &mut damage_events,
            &mut health_q,
            &mut behaviors_q,
            &transform_q,
            &faction_q,
        );
    }
}
