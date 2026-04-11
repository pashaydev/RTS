//! Combat system types: damage, targeting, stances, phases, armor.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::app::Faction;

// ---------------------------------------------------------------------------
// 1. TaskSource
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, PartialEq, Debug, Default)]
pub enum TaskSource {
    Manual,
    #[default]
    Auto,
}

// ---------------------------------------------------------------------------
// 2. ManualIdleSince
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct ManualIdleSince(pub f64);

// ---------------------------------------------------------------------------
// 3. IntentSource
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum IntentSource {
    Manual,
    #[default]
    Auto,
}

// ---------------------------------------------------------------------------
// 4. CombatIntent
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, PartialEq, Debug, Default)]
pub enum CombatIntent {
    #[default]
    None,
    Move(Vec3),
    Attack(Entity, IntentSource),
    AttackMove(Vec3, IntentSource),
    Hold,
}

// ---------------------------------------------------------------------------
// 4b. CombatOrder / CombatGoal / OrderSource — future unified state (Step 3/4)
// ---------------------------------------------------------------------------
//
// `CombatOrder` is the forward-looking unified entry-state component. It is
// written by every order-entry path (manual clicks, auto-aggro, ability casts,
// damage retaliation) and is intended to replace `CombatIntent` + `Engagement`
// + `CombatTargetLock` once the pipeline migrates to read it directly (Step 4).
//
// For now it is *mirrored* alongside the legacy components so that:
//   - downstream combat systems continue to work unchanged
//   - save_load can migrate incrementally
//   - Step 4 can start reading `CombatOrder` without churning entry-sites again

/// What a unit is ordered to do.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum CombatGoal {
    #[default]
    Stop,
    Move(Vec3),
    Attack(Entity),
    AttackMove(Vec3),
    Hold,
}

/// Priority/origin of a `CombatOrder`. Used to unify the manual-vs-auto rules
/// (chase timeouts, grace windows, lock TTLs) that are currently duplicated
/// across `unit_ai.rs`, `intents.rs`, and `combat/core.rs`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum OrderSource {
    /// Direct player command. Highest priority; cannot be preempted by auto.
    Manual,
    /// Autonomous AI decision (idle scan, squad coordination, mob aggro).
    #[default]
    Auto,
    /// Reflexive retaliation to incoming damage. Priority sits between Auto and Manual.
    Retaliate,
    /// Triggered by ability casts (e.g. Knight Charge locking a target).
    Ability,
}

impl OrderSource {
    /// Upper bound on how long this order is allowed to chase a fleeing target
    /// before giving up. Replaces the manual-10s / auto-6s hard-coded branch
    /// in `core.rs` approach logic.
    pub fn chase_timeout_secs(self) -> f32 {
        match self {
            OrderSource::Manual => 10.0,
            OrderSource::Retaliate => 8.0,
            OrderSource::Ability => 6.0,
            OrderSource::Auto => 6.0,
        }
    }
}

/// Unified combat order. Set by every order-entry site; consumed by the
/// combat pipeline (post Step 4). Until Step 4 lands this is mirrored alongside
/// the legacy `CombatIntent` / `Engagement` / `CombatTargetLock` components.
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct CombatOrder {
    pub goal: CombatGoal,
    pub source: OrderSource,
    /// Origin anchor for leashing (AttackMove destination, Hold position,
    /// mob camp center, auto-aggro start point).
    pub anchor: Option<Vec3>,
    /// Wall-clock time this order was issued. Used for grace windows and
    /// staleness checks.
    pub issued_at: f64,
}

impl CombatOrder {
    #[inline]
    pub fn new(goal: CombatGoal, source: OrderSource, issued_at: f64) -> Self {
        Self {
            goal,
            source,
            anchor: None,
            issued_at,
        }
    }

    #[inline]
    pub fn with_anchor(mut self, anchor: Vec3) -> Self {
        self.anchor = Some(anchor);
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EngageMode {
    #[default]
    Direct,
    AttackMove,
    Hold,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EngageStatus {
    #[default]
    Acquiring,
    Approaching,
    InBand,
    Windup,
    Recovery,
    Blocked,
}

/// Authoritative runtime combat state for a unit or mob.
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct Engagement {
    pub target: Option<Entity>,
    pub source: IntentSource,
    pub mode: EngageMode,
    pub anchor: Vec3,
    pub acquired_at: f64,
    pub last_confirmed_at: f64,
    pub persistence_until: f64,
    pub status: EngageStatus,
}

// ---------------------------------------------------------------------------
// 5. BufferedCommandKind
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BufferedCommandKind {
    Move(Vec3),
    Attack(Entity),
    AttackMove(Vec3),
    Hold,
    Stop,
}

// ---------------------------------------------------------------------------
// 6. BufferedCommand
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct BufferedCommand {
    pub kind: BufferedCommandKind,
    pub issue_time: f64,
    pub expires_at: Option<f64>,
}

// ---------------------------------------------------------------------------
// 7. CombatPhase
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CombatPhase {
    #[default]
    Idle,
    Pursuing,
    SeekingSlot,
    InRange,
    Windup,
    Committed,
    Recovery,
    Blocked,
}

// ---------------------------------------------------------------------------
// 8. CombatTargetLock
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct CombatTargetLock {
    pub target: Entity,
    pub locked_until: f64,
    pub source: IntentSource,
}

// ---------------------------------------------------------------------------
// 9. AttackCommit
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct AttackCommit {
    pub target: Entity,
    pub commit_time: f64,
    pub release_time: f64,
}

// ---------------------------------------------------------------------------
// 10. EngagementAnchor
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct EngagementAnchor(pub Vec3);

// ---------------------------------------------------------------------------
// 11. PathBlockedTimer
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, PartialEq, Debug, Default)]
pub struct PathBlockedTimer {
    pub blocked_since: Option<f64>,
    pub retry_after: Option<f64>,
}

// ---------------------------------------------------------------------------
// 12. CombatThinkTimer
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Component, Clone, Copy, PartialEq, Debug, Default)]
pub struct CombatThinkTimer {
    pub next_think_at: f64,
    pub interval_secs: f32,
}

// ---------------------------------------------------------------------------
// 13. SlotClaim
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct SlotClaim {
    pub target: Entity,
    pub slot_index: u16,
    pub last_valid_time: f64,
}

// ---------------------------------------------------------------------------
// 14. MeleeSlotProfile
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct MeleeSlotProfile {
    pub personal_radius: f32,
    pub preferred_ring: u8,
    pub slot_weight: f32,
}

// ---------------------------------------------------------------------------
// 15. CombatTelemetrySample
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Component, Clone, Debug, Default)]
pub struct CombatTelemetrySample {
    pub think_bucket: u8,
    pub last_intent_refresh_time: Option<f64>,
    pub last_target_acquire_time: Option<f64>,
    pub rescan_count: u32,
    pub slot_refresh_count: u32,
}

// ---------------------------------------------------------------------------
// 16. UnitStance
// ---------------------------------------------------------------------------

/// Controls how a unit auto-engages enemies.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum UnitStance {
    /// Never auto-engage, only manual orders.
    Passive,
    /// Auto-engage within scan range, leash back if too far.
    #[default]
    Defensive,
    /// Actively seek enemies, chase aggressively.
    Aggressive,
}

impl UnitStance {
    #[allow(dead_code)]
    pub fn cycle(self) -> Self {
        match self {
            UnitStance::Passive => UnitStance::Defensive,
            UnitStance::Defensive => UnitStance::Aggressive,
            UnitStance::Aggressive => UnitStance::Passive,
        }
    }

    #[allow(dead_code)]
    pub fn scan_multiplier(self) -> f32 {
        match self {
            UnitStance::Passive => 0.0,
            UnitStance::Defensive => 1.5,
            UnitStance::Aggressive => 2.5,
        }
    }

    #[allow(dead_code)]
    pub fn leash_distance(self) -> f32 {
        match self {
            UnitStance::Passive => 0.0,
            UnitStance::Defensive => 12.0,
            UnitStance::Aggressive => 50.0,
        }
    }
}

// ---------------------------------------------------------------------------
// 17. TacticalRole
// ---------------------------------------------------------------------------

/// Determines AI behavior and targeting priorities for a unit.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TacticalRole {
    /// No special behavior.
    #[default]
    Standard,
    /// Kites enemies, maintains distance.
    RangedKiter,
    /// Seeks the front line, absorbs damage.
    Frontline,
    /// Prioritises healing allies.
    Healer,
    /// Attempts to flank and hit vulnerable targets.
    Flanker,
    /// Stays behind the line, focuses structures.
    SiegeSupport,
}

// ---------------------------------------------------------------------------
// 18. CombatHotspots
// ---------------------------------------------------------------------------

/// Tracks locations of active combat for AI awareness.
#[derive(Resource, Default)]
pub struct CombatHotspots {
    pub spots: Vec<(Vec3, Entity, Faction)>,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct RecentCombatDamage {
    pub attacker: Entity,
    pub observed_at: f64,
    pub expires_at: f64,
}

// ---------------------------------------------------------------------------
// 19. LeashOrigin
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, Debug)]
pub struct LeashOrigin(pub Vec3);

// ---------------------------------------------------------------------------
// 20. CombatTuning
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Resource, Clone, Debug)]
pub struct CombatTuning {
    pub passive_scan_multiplier: f32,
    pub defensive_scan_multiplier: f32,
    pub aggressive_scan_multiplier: f32,
    pub passive_leash_distance: f32,
    pub defensive_leash_distance: f32,
    pub aggressive_leash_distance: f32,
    pub manual_override_freshness_secs: f32,
    pub command_buffer_ttl_secs: f32,
    pub manual_target_lock_secs: f32,
    pub auto_target_lock_secs: f32,
    pub retarget_grace_secs: f32,
    pub retarget_score_margin: f32,
    pub range_stay_buffer: f32,
    pub minimum_range_stay_buffer: f32,
    pub melee_contact_sticky_secs: f32,
    pub blocked_repath_interval: f32,
    pub squad_focus_hold_secs: f32,
    pub damage_memory_secs: f32,
    pub ally_alert_assist_range: f32,
}

impl Default for CombatTuning {
    fn default() -> Self {
        Self {
            passive_scan_multiplier: 0.0,
            defensive_scan_multiplier: 1.5,
            aggressive_scan_multiplier: 2.5,
            passive_leash_distance: 0.0,
            defensive_leash_distance: 12.0,
            aggressive_leash_distance: 50.0,
            manual_override_freshness_secs: 1.5,
            command_buffer_ttl_secs: 0.35,
            manual_target_lock_secs: 0.75,
            auto_target_lock_secs: 0.5,
            retarget_grace_secs: 0.8,
            retarget_score_margin: 0.35,
            range_stay_buffer: 0.45,
            minimum_range_stay_buffer: 0.25,
            melee_contact_sticky_secs: 0.5,
            blocked_repath_interval: 0.2,
            squad_focus_hold_secs: 1.2,
            damage_memory_secs: 1.6,
            ally_alert_assist_range: 16.0,
        }
    }
}

// ---------------------------------------------------------------------------
// 21. CombatBudget
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Resource, Clone, Debug)]
pub struct CombatBudget {
    pub max_target_rescans_per_frame: usize,
    pub max_slot_refreshes_per_frame: usize,
    pub max_repath_requests_per_frame: usize,
    pub max_blocked_resolutions_per_frame: usize,
}

impl Default for CombatBudget {
    fn default() -> Self {
        Self {
            max_target_rescans_per_frame: 64,
            max_slot_refreshes_per_frame: 32,
            max_repath_requests_per_frame: 48,
            max_blocked_resolutions_per_frame: 32,
        }
    }
}

// ---------------------------------------------------------------------------
// 22. MeleeSlotCacheEntry
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Default, Clone, Debug)]
pub struct MeleeSlotCacheEntry {
    pub target_position: Vec3,
    pub last_update_time: f64,
    pub slot_count: usize,
    pub occupancy_revision: u32,
}

// ---------------------------------------------------------------------------
// 23. MeleeSlotCache
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Resource, Default, Clone, Debug)]
pub struct MeleeSlotCache {
    pub active_targets: HashMap<Entity, MeleeSlotCacheEntry>,
}

// ---------------------------------------------------------------------------
// 24. CombatStatsDebug
// ---------------------------------------------------------------------------

#[derive(Resource, Default, Clone, Debug)]
pub struct CombatStatsDebug {
    pub buffered_commands_active: usize,
    pub target_locks_active: usize,
    pub slot_claims_active: usize,
    pub expired_buffered_commands: u64,
    pub expired_target_locks: u64,
    pub target_rescans_this_frame: usize,
    pub slot_refreshes_this_frame: usize,
    pub repath_requests_this_frame: usize,
    pub blocked_resolutions_this_frame: usize,
}

// ---------------------------------------------------------------------------
// 25. AttackTarget
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct AttackTarget(pub Entity);

// ---------------------------------------------------------------------------
// 26. AttackCooldown
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct AttackCooldown {
    pub ready_in: f32,
    pub interval: f32,
}

// ---------------------------------------------------------------------------
// 27. AttackDamage
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct AttackDamage(pub f32);

// ---------------------------------------------------------------------------
// 28. AttackRange
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct AttackRange(pub f32);

// ---------------------------------------------------------------------------
// 29. AggroRange
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct AggroRange(pub f32);

// ---------------------------------------------------------------------------
// 30. ThreatValue
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, Debug)]
pub struct ThreatValue(pub f32);

// ---------------------------------------------------------------------------
// 31. TargetingProfile
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, Debug)]
pub struct TargetingProfile {
    pub distance_weight: f32,
    pub low_hp_weight: f32,
    pub threat_weight: f32,
    pub counter_weight: f32,
    pub building_penalty: f32,
    pub reserved_damage_penalty: f32,
}

// ---------------------------------------------------------------------------
// 32. AttackTiming
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, Debug)]
pub struct AttackTiming {
    pub _attack_point_secs: f32,
    pub _backswing_secs: f32,
    pub _turn_rate_rad_per_sec: f32,
    pub minimum_range: f32,
    pub _can_move_during_backswing: bool,
}

// ---------------------------------------------------------------------------
// 33. AttackProfile
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, Debug)]
pub struct AttackProfile {
    pub windup_secs: f32,
    pub recovery_secs: f32,
    pub projectile_speed: f32,
    pub projectile_scale: f32,
    pub impact_scale: f32,
}

impl AttackProfile {
    /// A unit is considered ranged when it fires a projectile (i.e. has a projectile speed).
    /// This is the single source of truth — the legacy `IsRanged` marker component is derived from here.
    #[inline]
    pub fn is_ranged(&self) -> bool {
        self.projectile_speed > 0.0
    }
}

// ---------------------------------------------------------------------------
// 34. CombatFxKind
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CombatFxKind {
    Slash,
    Pierce,
    Arcane,
    Siege,
    Shadow,
}

// ---------------------------------------------------------------------------
// 35. ReservedIncomingDamage
// ---------------------------------------------------------------------------

/// Tracks predicted incoming damage from committed attacks so that
/// targeting systems can avoid overkill.
#[derive(Component, Clone, Debug, Default)]
pub struct ReservedIncomingDamage {
    /// Each entry is `(attacker, damage, expiry_time)`.
    pub reservations: Vec<(Entity, f32, f32)>,
}

impl ReservedIncomingDamage {
    pub fn total(&self) -> f32 {
        self.reservations.iter().map(|(_, d, _)| d).sum()
    }
}

// ---------------------------------------------------------------------------
// 36. AttackWindup
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, Debug)]
pub struct AttackWindup {
    pub target: Entity,
    pub remaining_secs: f32,
}

// ---------------------------------------------------------------------------
// 37. AttackRecovery
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, Debug)]
pub struct AttackRecovery {
    pub remaining_secs: f32,
}

// ---------------------------------------------------------------------------
// 38. ChaseTimer
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Debug)]
pub struct ChaseTimer {
    pub elapsed: f32,
    pub max_secs: f32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct MeleeContact {
    pub target: Entity,
    pub sticky_until: f64,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct ProjectileImpactEvent {
    pub position: Vec3,
    pub target: Entity,
    pub damage: f32,
    pub fx_kind: CombatFxKind,
    pub impact_scale: f32,
    pub is_aoe: bool,
    pub direction: Vec3,
}

// ---------------------------------------------------------------------------
// 39. AttackLunge
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Debug)]
pub struct AttackLunge {
    pub direction: Vec3,
    pub timer: Timer,
    pub strength: f32,
    pub applied_offset: Vec3,
}

// ---------------------------------------------------------------------------
// 40. HitRecoil
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Debug)]
pub struct HitRecoil {
    pub direction: Vec3,
    pub timer: Timer,
    pub strength: f32,
    pub lift: f32,
    pub base_scale: Vec3,
    pub applied_offset: Vec3,
}

// ---------------------------------------------------------------------------
// 41. HitReaction
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Debug)]
pub struct HitReaction(pub Timer);

// ---------------------------------------------------------------------------
// 42. CameraShake
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Debug)]
pub struct CameraShake {
    pub timer: Timer,
    pub intensity: f32,
}

// ---------------------------------------------------------------------------
// 43. ArmorType
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ArmorType {
    Light,
    Heavy,
    Siege,
    Structure,
}

// ---------------------------------------------------------------------------
// 44. DamageType
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum DamageType {
    Melee,
    Pierce,
    Magic,
    SiegeDmg,
}

impl DamageType {
    pub fn multiplier_vs(self, armor: ArmorType) -> f32 {
        match (self, armor) {
            (DamageType::Melee, ArmorType::Light) => 1.0,
            (DamageType::Melee, ArmorType::Heavy) => 0.75,
            (DamageType::Melee, ArmorType::Siege) => 1.5,
            (DamageType::Melee, ArmorType::Structure) => 0.5,

            (DamageType::Pierce, ArmorType::Light) => 1.0,
            (DamageType::Pierce, ArmorType::Heavy) => 0.5,
            (DamageType::Pierce, ArmorType::Siege) => 0.25,
            (DamageType::Pierce, ArmorType::Structure) => 0.5,

            (DamageType::Magic, ArmorType::Light) => 1.25,
            (DamageType::Magic, ArmorType::Heavy) => 1.25,
            (DamageType::Magic, ArmorType::Siege) => 0.5,
            (DamageType::Magic, ArmorType::Structure) => 0.75,

            (DamageType::SiegeDmg, ArmorType::Light) => 0.5,
            (DamageType::SiegeDmg, ArmorType::Heavy) => 0.75,
            (DamageType::SiegeDmg, ArmorType::Siege) => 0.5,
            (DamageType::SiegeDmg, ArmorType::Structure) => 3.0,
        }
    }
}

// ---------------------------------------------------------------------------
// 45. Mob
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct Mob;

// ---------------------------------------------------------------------------
// 46. Boss
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct Boss;

// ---------------------------------------------------------------------------
// 47. CampReward
// ---------------------------------------------------------------------------

#[derive(Component, Clone)]
pub struct CampReward {
    pub resources: crate::blueprints::ResourceCost,
}

// ---------------------------------------------------------------------------
// 48. ExplosiveProp
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, Debug)]
pub struct ExplosiveProp {
    pub damage: f32,
    pub radius: f32,
}
