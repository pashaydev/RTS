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
// 3. OrderSource — priority/origin of a UnitBrain.order
// ---------------------------------------------------------------------------

/// Priority/origin of the current `UnitBrain.order`. Drives manual-vs-auto
/// preemption rules (chase timeouts, lock TTLs, retaliation windows).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum OrderSource {
    /// Direct player command. Highest priority; cannot be preempted by auto.
    Manual,
    /// Autonomous AI decision (idle scan, squad coordination, mob aggro).
    #[default]
    Auto,
    /// Reflexive retaliation to incoming damage. Priority sits between Auto and Manual.
    Retaliate,
}

impl OrderSource {
    /// Upper bound on how long this order is allowed to chase a fleeing target
    /// without ever closing to attack range. approach_target resets the timer
    /// whenever the unit enters the attack band, so these numbers only fire
    /// when the unit is genuinely stuck — blocked by terrain, losing a foot
    /// race, or pathing around obstacles without converging.
    pub fn chase_timeout_secs(self) -> f32 {
        match self {
            OrderSource::Manual => 20.0,
            OrderSource::Retaliate => 15.0,
            OrderSource::Auto => 12.0,
        }
    }
}

// ---------------------------------------------------------------------------
// 4. CombatThinkTimer
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Component, Clone, Copy, PartialEq, Debug, Default)]
pub struct CombatThinkTimer {
    pub next_think_at: f64,
    pub interval_secs: f32,
}

// ---------------------------------------------------------------------------
// 7. MeleeSlotProfile
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
    pub fn cycle(self) -> Self {
        match self {
            UnitStance::Passive => UnitStance::Defensive,
            UnitStance::Defensive => UnitStance::Aggressive,
            UnitStance::Aggressive => UnitStance::Passive,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            UnitStance::Passive => 0,
            UnitStance::Defensive => 1,
            UnitStance::Aggressive => 2,
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => UnitStance::Passive,
            2 => UnitStance::Aggressive,
            _ => UnitStance::Defensive,
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

#[derive(Resource, Clone, Debug)]
pub struct CombatTuning {
    pub scan_multipliers: [f32; 3],
    pub leash_distances: [f32; 3],
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
            scan_multipliers: [0.0, 1.5, 2.5],
            leash_distances: [0.0, 12.0, 50.0],
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

impl CombatTuning {
    pub fn scan_multiplier(&self, stance: UnitStance) -> f32 {
        self.scan_multipliers[stance.to_u8() as usize]
    }

    pub fn leash_distance(&self, stance: UnitStance) -> f32 {
        self.leash_distances[stance.to_u8() as usize]
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
            max_target_rescans_per_frame: usize::MAX,
            max_slot_refreshes_per_frame: usize::MAX,
            max_repath_requests_per_frame: usize::MAX,
            max_blocked_resolutions_per_frame: usize::MAX,
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
// 25. AttackCooldown
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

/// Emitted whenever damage is actually applied to a target.
///
/// Single source-of-truth observable for post-damage systems (replication,
/// combat stats, sound/UI feedback, AI threat tracking). Producers that
/// mutate `Health` directly should instead call through
/// [`crate::simulation::combat::apply_damage`], which is the canonical
/// choke point, and emit this message alongside it.
#[derive(Message, Clone, Copy, Debug)]
pub struct DamageApplied {
    pub target: Entity,
    pub source: Option<Entity>,
    pub amount: f32,
    pub damage_type: DamageType,
    pub now_secs: f64,
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

/// Stat tier applied at spawn time. The blueprint defines the base Goblin
/// stats; the tier multiplies HP / damage / move speed and tints the impostor.
/// Visual differentiation is the only reason `Veteran` and `Champion` exist as
/// separate variants — there is one mob kind (`EntityKind::Goblin`).
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MobTier {
    #[default]
    Runner,
    Veteran,
    Champion,
}

impl MobTier {
    pub fn hp_mul(self) -> f32 {
        match self {
            MobTier::Runner => 1.0,
            MobTier::Veteran => 1.8,
            MobTier::Champion => 3.0,
        }
    }
    pub fn damage_mul(self) -> f32 {
        match self {
            MobTier::Runner => 1.0,
            MobTier::Veteran => 1.4,
            MobTier::Champion => 2.0,
        }
    }
    pub fn move_mul(self) -> f32 {
        match self {
            MobTier::Runner => 1.05,
            MobTier::Veteran => 1.0,
            MobTier::Champion => 0.9,
        }
    }
    /// Visual scale multiplier applied to the impostor billboard.
    pub fn visual_scale_mul(self) -> f32 {
        match self {
            MobTier::Runner => 1.0,
            MobTier::Veteran => 1.05,
            MobTier::Champion => 1.3,
        }
    }
    /// Multiplicative RGB tint applied to the impostor sample.
    pub fn tint_rgb(self) -> [f32; 3] {
        match self {
            MobTier::Runner => [1.0, 1.0, 1.0],
            MobTier::Veteran => [0.85, 0.78, 0.72], // darker
            MobTier::Champion => [1.15, 0.78, 0.65], // redder
        }
    }
}

/// Whether a mob's primary engagement target is a building (preferred) or a unit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EngagementTargetKind {
    Building,
    Unit,
}

/// What a mob committed to when it spawned. The combat layer drives the actual
/// fight; this component re-asserts the original goal once a detour
/// (retaliation, intercepted unit) ends.
///
/// `rescans_used` is capped at 2 across the mob's lifetime — when the original
/// target dies, the mob gets one rescan; if the second target also dies, the
/// mob is "stranded" and walks off the map edge.
#[derive(Component, Clone, Copy, Debug)]
pub struct MobEngagement {
    pub primary: Entity,
    pub primary_kind: EngagementTargetKind,
    pub rescans_used: u8,
}

/// Marker for a mob that is retreating off-map at Dawn. Despawn-on-edge is
/// driven by `mob_retreat_despawn_system`.
#[derive(Component, Clone, Copy, Debug)]
pub struct RetreatingMob {
    /// Tick when the retreat order was issued — used to enforce the hard
    /// despawn timeout if the mob never makes it to the edge.
    pub started_at_tick: u64,
}

/// Cardinal direction the wave concentrates around. `All` means the wave
/// scatters around the perimeter without bias.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WaveDirection {
    North,
    East,
    South,
    West,
    All,
}

impl WaveDirection {
    pub fn label(self) -> &'static str {
        match self {
            WaveDirection::North => "north",
            WaveDirection::East => "east",
            WaveDirection::South => "south",
            WaveDirection::West => "west",
            WaveDirection::All => "all sides",
        }
    }
    /// Angle in radians (0 = +X = east, π/2 = +Z = south in our convention).
    /// Returned as `Some` when there is a primary direction; `None` for All.
    pub fn primary_angle(self) -> Option<f32> {
        use std::f32::consts::PI;
        match self {
            WaveDirection::East => Some(0.0),
            WaveDirection::South => Some(0.5 * PI),
            WaveDirection::West => Some(PI),
            WaveDirection::North => Some(1.5 * PI),
            WaveDirection::All => None,
        }
    }
}

/// Pre-night warning emitted on the Day→Dusk transition. The wave banner
/// widget renders this; the event log records it; sim systems may use the
/// `night` field to coordinate.
#[derive(Message, Clone, Copy, Debug)]
pub struct WaveAlert {
    pub night: u32,
    pub incoming_count: u32,
    pub direction: WaveDirection,
}

/// Day→Dusk transition message. Wave compose runs on receipt.
#[derive(Message, Clone, Copy, Debug)]
pub struct DuskBegan;

/// Night→Dawn transition message. Drives surviving-mob retreat and the
/// post-night summary toast.
#[derive(Message, Clone, Copy, Debug)]
pub struct DawnBegan;

// ---------------------------------------------------------------------------
// 48. ExplosiveProp
// ---------------------------------------------------------------------------

#[derive(Component, Clone, Copy, Debug)]
pub struct ExplosiveProp {
    pub damage: f32,
    pub radius: f32,
}
