//! Unit types: state machines, abilities, formations, veterancy, movement.

use bevy::prelude::*;
use std::collections::{HashMap, VecDeque};

use super::app::Faction;
use super::economy::AssignedPhase;
use crate::blueprints::EntityKind;

// ── Unit markers ──

#[derive(Component)]
pub struct Unit;

#[derive(Component, Clone, Debug)]
pub struct UnitDisplayName(pub String);

#[derive(Component)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            current: 100.0,
            max: 100.0,
        }
    }
}

#[derive(Component)]
pub struct MoveTarget(pub Vec3);

#[derive(Component)]
pub struct UnitSpeed(pub f32);

// ── Unit State Machine ──

/// Unified unit state machine — replaces WorkerTask and ad-hoc combat states.
#[derive(Component, Clone, Copy, PartialEq, Debug, Default)]
pub enum UnitState {
    #[default]
    Idle,
    Moving(Vec3),
    Attacking(Entity),
    Gathering(Entity),
    ReturningToDeposit {
        depot: Entity,
        gather_node: Option<Entity>,
    },
    Depositing {
        depot: Entity,
        gather_node: Option<Entity>,
    },
    WaitingForStorage {
        depot: Entity,
        gather_node: Option<Entity>,
    },
    WaitingForDepot {
        gather_node: Option<Entity>,
    },
    /// Worker moving to a location to plot (start) a new building
    MovingToPlot(Vec3),
    MovingToBuild(Entity),
    Building(Entity),
    /// Worker assigned to a processing building (visible, walking between nodes and building)
    AssignedGathering {
        building: Entity,
        phase: AssignedPhase,
    },
    Patrolling {
        target: Vec3,
        origin: Vec3,
    },
    AttackMoving(Vec3),
    HoldPosition,
}

impl UnitState {
    pub fn assigned_processor_building(&self) -> Option<Entity> {
        match self {
            Self::AssignedGathering { building, .. } => Some(*building),
            _ => None,
        }
    }
}

/// A task waiting in a unit's queue (shift+click).
#[derive(Clone, Debug)]
pub enum QueuedTask {
    Move(Vec3),
    AttackMove(Vec3),
    Attack(Entity),
    Gather(Entity),
    Build(Entity),
    Patrol(Vec3),
    HoldPosition,
}

#[derive(Clone, Debug)]
pub struct TaskEntry {
    pub id: u64,
    pub task: QueuedTask,
}

/// Attached to a worker who is walking to a location to plot a new building.
#[derive(Component)]
pub struct PendingBuildOrder {
    pub kind: EntityKind,
    pub position: Vec3,
    pub faction: Faction,
    pub rotation_y: f32,
}

#[derive(Component)]
pub struct BuildSitePreparation {
    pub kind: EntityKind,
    pub position: Vec3,
    pub faction: Faction,
    pub rotation_y: f32,
    pub prep_timer: Timer,
    pub vfx_timer: Timer,
    pub burst_count: u8,
}

/// Task queue for shift+click command queuing.
#[derive(Component, Default)]
pub struct TaskQueue {
    pub current: Option<TaskEntry>,
    pub queue: VecDeque<TaskEntry>,
}

impl TaskQueue {
    pub fn clear(&mut self) {
        self.current = None;
        self.queue.clear();
    }

    pub fn clear_queued(&mut self) {
        self.queue.clear();
    }

    pub fn remove_by_id(&mut self, id: u64) -> Option<TaskEntry> {
        let idx = self.queue.iter().position(|entry| entry.id == id)?;
        self.queue.remove(idx)
    }
}

#[derive(Resource, Default)]
pub struct NextTaskId(pub u64);

/// Tracks which workers are assigned inside a building (for UI display).
#[derive(Component, Default)]
pub struct AssignedWorkers {
    pub workers: Vec<Entity>,
}

/// Button to unassign a specific worker from a processor building.
#[derive(Component)]
pub struct UnassignSpecificWorkerButton(pub Entity);

/// Floating world-space overlay showing workers assigned to a building.
#[derive(Component)]
pub struct WorkerOverlay {
    pub building: Entity,
}

// ── Formation System ──

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FormationType {
    Line,
    #[default]
    Grid,
    Chess,
}

impl FormationType {
    pub const ALL: [Self; 3] = [Self::Line, Self::Grid, Self::Chess];

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::Grid => "Grid",
            Self::Chess => "Chess",
        }
    }

    pub fn tooltip_text(self) -> &'static str {
        match self {
            Self::Line => "Wide front line for direct pushes",
            Self::Grid => "Compact block for default group movement",
            Self::Chess => "Staggered checker pattern for cleaner spacing",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Self::Line => Self::Grid,
            Self::Grid => Self::Chess,
            Self::Chess => Self::Line,
        }
    }

    pub fn to_net_u8(self) -> u8 {
        match self {
            Self::Line => 0,
            Self::Grid => 1,
            Self::Chess => 2,
        }
    }

    pub fn from_net_u8(value: u8) -> Self {
        match value {
            0 => Self::Line,
            2 => Self::Chess,
            _ => Self::Grid,
        }
    }
}

pub fn formation_offsets(formation: FormationType, count: usize, facing: Vec2) -> Vec<Vec2> {
    let spacing = 2.0;
    let forward = facing.normalize_or_zero();
    let forward = if forward.length_squared() > 0.0 {
        forward
    } else {
        Vec2::Y
    };
    let perp = Vec2::new(-forward.y, forward.x);

    match formation {
        FormationType::Line => (0..count)
            .map(|i| {
                let offset = (i as f32 - (count as f32 - 1.0) / 2.0) * spacing;
                perp * offset
            })
            .collect(),
        FormationType::Grid => {
            let cols = (count as f32).sqrt().ceil() as usize;
            let spacing_val = spacing * 1.25;
            (0..count)
                .map(|i| {
                    let col = i % cols;
                    let row = i / cols;
                    let x = (col as f32 - (cols as f32 - 1.0) / 2.0) * spacing_val;
                    let y = -(row as f32) * spacing_val;
                    perp * x + forward * y
                })
                .collect()
        }
        FormationType::Chess => {
            let cols = (count as f32).sqrt().ceil() as usize;
            let spacing_val = spacing * 1.15;
            (0..count)
                .map(|i| {
                    let col = i % cols;
                    let row = i / cols;
                    let row_shift = if row % 2 == 0 { 0.0 } else { spacing_val * 0.5 };
                    let x = (col as f32 - (cols as f32 - 1.0) / 2.0) * spacing_val + row_shift;
                    let y = -(row as f32) * spacing_val;
                    perp * x + forward * y
                })
                .collect()
        }
    }
}

#[derive(Resource, Default)]
pub struct ActiveFormation {
    pub formation: FormationType,
}

// ── Unit Veterancy ──

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VeterancyLevel {
    #[default]
    Recruit,
    Veteran,
    Elite,
}

impl VeterancyLevel {
    pub fn damage_mult(self) -> f32 {
        match self {
            Self::Recruit => 1.0,
            Self::Veteran => 1.15,
            Self::Elite => 1.30,
        }
    }

    pub fn hp_mult(self) -> f32 {
        match self {
            Self::Recruit => 1.0,
            Self::Veteran => 1.10,
            Self::Elite => 1.25,
        }
    }

    pub fn speed_mult(self) -> f32 {
        match self {
            Self::Recruit => 1.0,
            Self::Veteran => 1.0,
            Self::Elite => 1.10,
        }
    }

    pub fn next(self) -> Option<(Self, u32)> {
        match self {
            Self::Recruit => Some((Self::Veteran, 100)),
            Self::Veteran => Some((Self::Elite, 300)),
            Self::Elite => None,
        }
    }

    pub fn star_count(self) -> u8 {
        match self {
            Self::Recruit => 0,
            Self::Veteran => 1,
            Self::Elite => 2,
        }
    }
}

#[derive(Component, Default)]
pub struct Experience {
    pub current: u32,
    pub level: VeterancyLevel,
}

/// Tracks the last applied veterancy level to avoid re-applying bonuses.
#[derive(Component)]
pub struct VeterancyApplied(pub VeterancyLevel);

/// Marker for veterancy star indicator entities.
#[derive(Component)]
pub struct VeterancyIndicator;

// ── Active Abilities ──

/// Identifies an ability by a unique enum variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum AbilityId {
    KnightCharge,
    MageFireball,
    MageFrostNova,
    PriestHeal,
    PriestHolySmite,
    CatapultAoeBoulder,
}

impl AbilityId {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::KnightCharge => "Charge",
            Self::MageFireball => "Fireball",
            Self::MageFrostNova => "Frost Nova",
            Self::PriestHeal => "Heal",
            Self::PriestHolySmite => "Holy Smite",
            Self::CatapultAoeBoulder => "Boulder",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::KnightCharge => "Dash forward dealing 2x damage on next attack.",
            Self::MageFireball => "Launch an AoE fireball at target area.",
            Self::MageFrostNova => "Freeze nearby enemies, dealing damage and slowing them.",
            Self::PriestHeal => "Heal a friendly unit for 40 HP.",
            Self::PriestHolySmite => "Deal 25 holy damage (50 to undead).",
            Self::CatapultAoeBoulder => "Launch a boulder that deals AoE damage.",
        }
    }

    pub fn cooldown_secs(self) -> f32 {
        match self {
            Self::KnightCharge => 12.0,
            Self::MageFireball => 8.0,
            Self::MageFrostNova => 15.0,
            Self::PriestHeal => 6.0,
            Self::PriestHolySmite => 10.0,
            Self::CatapultAoeBoulder => 0.0, // uses normal attack cooldown
        }
    }

    pub fn targeting(self) -> AbilityTargeting {
        match self {
            Self::KnightCharge => AbilityTargeting::PointTarget,
            Self::MageFireball => AbilityTargeting::PointTarget,
            Self::MageFrostNova => AbilityTargeting::NoTarget,
            Self::PriestHeal => AbilityTargeting::UnitTarget,
            Self::PriestHolySmite => AbilityTargeting::UnitTarget,
            Self::CatapultAoeBoulder => AbilityTargeting::PointTarget,
        }
    }

    pub fn hotkey(self) -> &'static str {
        match self {
            Self::KnightCharge
            | Self::MageFireball
            | Self::PriestHeal
            | Self::CatapultAoeBoulder => "Q",
            Self::MageFrostNova | Self::PriestHolySmite => "W",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AbilityTargeting {
    NoTarget,
    PointTarget,
    UnitTarget,
}

/// Component: tracks abilities and their cooldowns for a unit.
#[derive(Component)]
pub struct UnitAbilities {
    pub abilities: Vec<AbilityId>,
    pub cooldowns: HashMap<AbilityId, f32>,
}

impl UnitAbilities {
    pub fn new(abilities: Vec<AbilityId>) -> Self {
        let cooldowns = abilities.iter().map(|&id| (id, 0.0_f32)).collect();
        Self {
            abilities,
            cooldowns,
        }
    }

    pub fn is_ready(&self, id: AbilityId) -> bool {
        self.cooldowns.get(&id).map_or(false, |&cd| cd <= 0.0)
    }

    pub fn trigger_cooldown(&mut self, id: AbilityId) {
        if let Some(cd) = self.cooldowns.get_mut(&id) {
            *cd = id.cooldown_secs();
        }
    }
}

/// Component: marks a unit as currently casting an ability.
#[derive(Component)]
pub struct CastingAbility {
    pub ability: AbilityId,
    pub target_pos: Option<Vec3>,
    pub target_entity: Option<Entity>,
    pub cast_timer: Timer,
}

/// Temporary bonus from Knight Charge — next attack deals 2x damage.
#[derive(Component)]
pub struct ChargeBonus {
    pub timer: Timer,
    pub damage_mult: f32,
}

/// AoE splash damage on projectile impact.
#[derive(Component)]
pub struct AoeSplash {
    pub radius: f32,
    pub falloff: bool,
}

// ── Status Effects ──

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusEffectKind {
    Slow,
    Stun,
    Burning,
}

#[derive(Clone)]
pub struct ActiveStatusEffect {
    pub kind: StatusEffectKind,
    pub remaining: f32,
    pub strength: f32,
}

/// Component: active status effects on a unit.
#[derive(Component, Default)]
pub struct StatusEffects {
    pub effects: Vec<ActiveStatusEffect>,
}

impl StatusEffects {
    pub fn has(&self, kind: StatusEffectKind) -> bool {
        self.effects.iter().any(|e| e.kind == kind)
    }

    pub fn slow_factor(&self) -> f32 {
        self.effects
            .iter()
            .filter(|e| e.kind == StatusEffectKind::Slow)
            .map(|e| 1.0 - e.strength)
            .fold(1.0, |acc, f| acc * f)
    }

    pub fn is_stunned(&self) -> bool {
        self.has(StatusEffectKind::Stun)
    }
}

#[derive(Component)]
pub struct FootstepTimer(pub Timer);

/// Brief dampening period after a unit arrives at its destination.
/// Reduces avoidance forces to prevent the "shoved backward" visual glitch.
#[derive(Component)]
pub struct JustArrived(pub Timer);

// ── Animation ──

/// Tracks which animation state a unit is currently playing.
#[derive(Component)]
pub struct AnimationController {
    pub current_state: AnimState,
}

/// Reference to the entity that owns the AnimationPlayer (deep in the GLTF hierarchy).
#[derive(Component)]
pub struct AnimPlayerRef(pub Entity);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum AnimState {
    #[default]
    Idle,
    Walk,
    Run,
    AttackA,
    AttackB,
    CastA,
    CastB,
    Damage,
    DeathA,
    DeathB,
}

/// Marker for the child entity holding a unit/mob character GLTF scene.
#[derive(Component)]
pub struct UnitSceneChild;

// ── Movement Smoothing ──

#[derive(Component)]
pub struct MovementSmoothing {
    pub current_speed: f32,
    pub acceleration: f32,
    pub deceleration: f32,
    /// Per-unit random multiplier (0.93..1.07) set on spawn for natural group movement.
    pub speed_variation: f32,
}

impl Default for MovementSmoothing {
    fn default() -> Self {
        Self {
            current_speed: 0.0,
            acceleration: 30.0,
            deceleration: 20.0,
            speed_variation: 1.0,
        }
    }
}

/// Tracks how long a moving unit has been stuck in place.
#[derive(Component)]
pub struct StuckTimer {
    pub last_position: Vec3,
    pub stuck_secs: f32,
    /// How many times we've repathed without making progress.
    pub repath_count: u8,
}

// ── Idle Behavior ──

#[derive(Component)]
pub struct IdleBehavior {
    pub fidget_timer: Timer,
    pub fidget_look_target: Option<Vec3>,
    pub fidget_elapsed: f32,
    pub breathing_phase: f32,
}
