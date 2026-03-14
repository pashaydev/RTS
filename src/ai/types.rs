use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::EntityKind;
use crate::components::*;

// ── Constants ──

pub const STRATEGY_TICK: f32 = 2.0;
pub const ECONOMY_TICK: f32 = 2.0;
pub const MILITARY_TICK: f32 = 2.0;
pub const TACTICAL_TICK: f32 = 0.5;
pub const SCOUT_TICK: f32 = 10.0;

pub const DEFENSE_SQUAD_SIZE: usize = 4;
pub const UNDER_ATTACK_COOLDOWN: f32 = 15.0;
pub const ATTACK_MIN_INTERVAL: f32 = 30.0;
pub const THREAT_DECAY_SECS: f32 = 120.0;
pub const SCOUT_RADIUS: f32 = 150.0;
pub const BASE_THREAT_RADIUS: f32 = 50.0;
pub const MAP_HALF: f32 = 245.0;
pub const COOPERATION_CHECK_INTERVAL: f32 = 5.0;
pub const ALLY_SUPPORT_DISTANCE: f32 = 60.0;

/// How long an attack run lasts before returning to Expanding
pub const ATTACK_DURATION: f32 = 60.0;
/// Below this avg HP ratio, retreat from attack
pub const RETREAT_HP_THRESHOLD: f32 = 0.40;
/// Minimum enemies near base to trigger defense interrupt
pub const DEFENSE_INTERRUPT_COUNT: u32 = 3;

/// Number of consecutive ticks a state-transition condition must hold
/// before actually transitioning (hysteresis).
pub const HYSTERESIS_TICKS: u8 = 2;

// ── Enums ──

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AiTopState {
    #[default]
    Founding,
    EarlyEconomy,
    Militarize,
    Expanding,
    Attacking,
    Defending,
    LateGame,
}

impl AiTopState {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Founding => "Founding",
            Self::EarlyEconomy => "EarlyEconomy",
            Self::Militarize => "Militarize",
            Self::Expanding => "Expanding",
            Self::Attacking => "Attacking",
            Self::Defending => "Defending",
            Self::LateGame => "LateGame",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TacticalPosture {
    #[default]
    Normal,
    UnderAttack,
    Retreating,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum SquadRole {
    GatherWood,
    GatherCopper,
    GatherIron,
    GatherGold,
    GatherOil,
    BuildConstruction,
    DefenseSquad,
    AttackSquad,
    Scout,
    Raider,
}

impl SquadRole {
    pub fn for_resource(rt: ResourceType) -> Self {
        match rt {
            ResourceType::Wood => SquadRole::GatherWood,
            ResourceType::Copper => SquadRole::GatherCopper,
            ResourceType::Iron => SquadRole::GatherIron,
            ResourceType::Gold => SquadRole::GatherGold,
            ResourceType::Oil => SquadRole::GatherOil,
            // Processed resources don't have dedicated gather squads
            _ => SquadRole::GatherWood,
        }
    }

    pub fn is_gather(&self) -> bool {
        matches!(
            self,
            SquadRole::GatherWood
                | SquadRole::GatherCopper
                | SquadRole::GatherIron
                | SquadRole::GatherGold
                | SquadRole::GatherOil
        )
    }

    pub fn resource_type(&self) -> Option<ResourceType> {
        match self {
            SquadRole::GatherWood => Some(ResourceType::Wood),
            SquadRole::GatherCopper => Some(ResourceType::Copper),
            SquadRole::GatherIron => Some(ResourceType::Iron),
            SquadRole::GatherGold => Some(ResourceType::Gold),
            SquadRole::GatherOil => Some(ResourceType::Oil),
            _ => None,
        }
    }
}

// ── Data Structs ──

#[derive(Clone, Debug)]
pub struct Squad {
    pub role: SquadRole,
    pub members: Vec<Entity>,
    pub rally_point: Option<Vec3>,
}

#[derive(Clone, Debug)]
pub struct ThreatEntry {
    pub position: Vec3,
    pub estimated_strength: f32,
    pub last_seen: f32,
    pub entity_count: u32,
}

#[derive(Clone, Debug)]
pub struct BuildRequest {
    pub kind: EntityKind,
    pub priority: u8,
    pub near_position: Option<Vec3>,
}

/// Wall plan: 4 sides of a rectangle around the base
#[derive(Clone, Debug)]
pub struct WallPlan {
    /// (start, end) for each of 4 wall sides
    pub sides: [(Vec3, Vec3); 4],
    pub completed: [bool; 4],
}

/// Resource goal for economy planning
#[derive(Clone, Debug)]
pub struct ResourceGoal {
    pub wood: u32,
    pub copper: u32,
    pub iron: u32,
    pub gold: u32,
    pub oil: u32,
}

// ── Per-Faction AI Brain ──

#[derive(Default)]
pub struct AiFactionBrain {
    // Timers
    pub strategy_timer: f32,
    pub economy_timer: f32,
    pub military_timer: f32,
    pub tactical_timer: f32,
    pub scout_timer: f32,

    // State machine
    pub top_state: AiTopState,
    pub state_entered_at: f32,
    pub posture: TacticalPosture,
    pub posture_cooldown: f32,
    pub game_time: f32,

    // Hysteresis: tracks how many consecutive ticks a candidate transition has been valid
    pub pending_transition: Option<AiTopState>,
    pub pending_transition_ticks: u8,

    // Personality & relation
    pub personality: AiPersonality,
    pub relation: AiRelation,
    pub difficulty: AiDifficulty,

    // Cooperation (friendly AI)
    pub ally_attack_target: Option<Vec3>,
    pub last_cooperation_check: f32,
    pub raid_cooldown: f32,

    // Squads
    pub squads: Vec<Squad>,
    pub assigned_units: HashMap<Entity, SquadRole>,

    // Economy — goal-oriented
    pub desired_workers: u8,
    pub build_queue: Vec<BuildRequest>,
    pub pending_builds: u8,
    pub resource_goal: Option<ResourceGoal>,
    pub income_rates: [f32; ResourceType::COUNT],
    pub last_resource_snapshot: [u32; ResourceType::COUNT],

    // Military intelligence
    pub attack_ready: bool,
    pub last_attack_time: f32,
    pub attack_started_at: f32,
    pub enemy_composition: HashMap<EntityKind, u32>,
    pub enemy_strength: f32,
    pub relative_strength: f32,
    pub defense_interrupt: bool,

    // Intel
    pub known_threats: Vec<ThreatEntry>,
    pub next_scout_waypoint: usize,
    pub scout_route: Vec<Vec3>,

    // Wall building
    pub wall_plan: Option<WallPlan>,

    // Cached
    pub base_position: Option<Vec3>,

    // Track previous health for damage detection
    pub prev_health: HashMap<Entity, f32>,
}

impl AiFactionBrain {
    pub fn new_with_offsets(
        offset: f32,
        relation: AiRelation,
        personality: AiPersonality,
        difficulty: AiDifficulty,
    ) -> Self {
        Self {
            strategy_timer: 0.0,
            economy_timer: offset,
            military_timer: offset * 0.5,
            tactical_timer: 0.0,
            desired_workers: 2,
            relation,
            personality,
            difficulty,
            ..Default::default()
        }
    }

    pub fn get_squad(&self, role: SquadRole) -> Option<&Squad> {
        self.squads.iter().find(|s| s.role == role)
    }

    pub fn get_squad_mut(&mut self, role: SquadRole) -> Option<&mut Squad> {
        self.squads.iter_mut().find(|s| s.role == role)
    }

    pub fn ensure_squad(&mut self, role: SquadRole) -> &mut Squad {
        if !self.squads.iter().any(|s| s.role == role) {
            self.squads.push(Squad {
                role,
                members: Vec::new(),
                rally_point: None,
            });
        }
        self.squads.iter_mut().find(|s| s.role == role).unwrap()
    }

    pub fn squad_size(&self, role: SquadRole) -> usize {
        self.get_squad(role).map_or(0, |s| s.members.len())
    }

    pub fn add_to_squad(&mut self, entity: Entity, role: SquadRole) {
        self.ensure_squad(role).members.push(entity);
        self.assigned_units.insert(entity, role);
    }

    pub fn remove_from_squad(&mut self, entity: Entity) {
        if let Some(role) = self.assigned_units.remove(&entity) {
            if let Some(squad) = self.get_squad_mut(role) {
                squad.members.retain(|&e| e != entity);
            }
        }
    }

    pub fn prune_dead(&mut self, alive: &std::collections::HashSet<Entity>) {
        for squad in &mut self.squads {
            squad.members.retain(|e| alive.contains(e));
        }
        self.assigned_units.retain(|e, _| alive.contains(e));
        self.prev_health.retain(|e, _| alive.contains(e));
    }

    pub fn max_build_queue(&self) -> usize {
        self.difficulty.max_concurrent_builds()
    }

    pub fn effective_tick(&self, base_tick: f32) -> f32 {
        base_tick * self.difficulty.tick_multiplier()
    }

    /// Attempt a state transition with hysteresis.
    /// The transition only fires if the same target state is requested
    /// for HYSTERESIS_TICKS consecutive strategy ticks.
    /// Defense transitions bypass hysteresis (immediate).
    pub fn try_transition_to(&mut self, new_state: AiTopState) {
        // Defense is always immediate
        if new_state == AiTopState::Defending {
            self.transition_to(new_state);
            return;
        }

        if self.pending_transition == Some(new_state) {
            self.pending_transition_ticks += 1;
            if self.pending_transition_ticks >= HYSTERESIS_TICKS {
                self.transition_to(new_state);
                self.pending_transition = None;
                self.pending_transition_ticks = 0;
            }
        } else {
            self.pending_transition = Some(new_state);
            self.pending_transition_ticks = 1;
        }
    }

    pub fn transition_to(&mut self, new_state: AiTopState) {
        self.top_state = new_state;
        self.state_entered_at = self.game_time;
        // Clear build queue on state change so per-state planner regenerates
        self.build_queue.clear();
        self.pending_transition = None;
        self.pending_transition_ticks = 0;
    }

    /// Attack strength threshold based on personality
    pub fn attack_strength_threshold(&self) -> f32 {
        match self.personality {
            AiPersonality::Aggressive => 0.8,
            AiPersonality::Balanced => 1.2,
            AiPersonality::Economic => 1.5,
            AiPersonality::Defensive => 1.8,
            AiPersonality::Supportive => 1.0,
        }
    }

    /// Minimum army size before considering attack
    pub fn min_attack_army(&self) -> usize {
        match self.personality {
            AiPersonality::Aggressive => 4,
            AiPersonality::Balanced => 6,
            AiPersonality::Defensive => 8,
            AiPersonality::Economic => 6,
            AiPersonality::Supportive => 5,
        }
    }
}

// ── Resource ──

#[derive(Resource, Default)]
pub struct AiState {
    pub factions: HashMap<Faction, AiFactionBrain>,
}
