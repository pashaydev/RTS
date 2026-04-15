//! Save/load system — serializes game world state to SQLite and reconstructs it.
//!
//! Save format: MessagePack blob stored in `game_saves` table.
//! Entity cross-references use `save_id: u32` indices resolved in a fixup pass after loading.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::blueprints::{BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::infrastructure::database::{ActiveProfile, GameDatabase};
use crate::infrastructure::multiplayer::NetRole;
use crate::presentation::model_assets::{BuildingModelAssets, UnitModelAssets};
use crate::simulation::ai::types::{
    AiFactionBrain, AiState, AiTopState, BuildRequest, ResourceGoal, Squad, SquadRole,
    TacticalPosture, ThreatEntry, WallPlan,
};
use crate::simulation::items::{
    ItemKind, ItemPickup, ItemRuntimeState, PickupCollectVfx, UnitInventory,
};
use crate::simulation::victory::{FactionStatus as VictFactionStatus, VictoryState};
use crate::types::*;
use crate::ui::widgets::group_hotkeys_widget::ControlGroups;
use crate::world::fog::FogTextureUploadState;
use crate::world::ground::{HeightMap, TerrainShapeSyncState};
use crate::world::lighting::{DayCycle, DayPhase};

use bevy::light::NotShadowCaster;
use rand::Rng;

// ── Plugin ──────────────────────────────────────────────────────────────────

pub struct SaveLoadPlugin;

impl Plugin for SaveLoadPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (handle_quicksave, handle_quickload).run_if(in_state(AppState::InGame)),
        )
        .add_systems(Update, handle_save_game_exclusive)
        .add_systems(
            OnEnter(AppState::InGame),
            load_saved_game
                .after(crate::world::ground::spawn_ground)
                .run_if(resource_exists::<PendingLoad>),
        )
        .add_systems(
            Update,
            restore_fog_on_load
                .run_if(resource_exists::<PendingFogRestore>)
                .run_if(resource_exists::<FogOfWarMap>)
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            restore_production_states.run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            restore_load_visuals
                .run_if(resource_exists::<PendingLoadVisuals>)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

// ── Events & Resources ──────────────────────────────────────────────────────

/// Resource-based trigger for saving. Inserted by pause menu or quicksave.
/// Consumed on next frame by handle_save_trigger system.
#[derive(Resource)]
pub struct SaveTrigger {
    pub label: Option<String>,
}

/// Inserted as a resource when loading a saved game.
/// Prevents normal spawn systems from running; the load system reads this instead.
#[derive(Resource)]
pub struct PendingLoad {
    pub save_data: SaveData,
}

/// Deferred fog of war restoration — inserted during load, consumed by `restore_fog_on_load`.
#[derive(Resource)]
struct PendingFogRestore {
    data: SavedFogOfWar,
}

/// Deferred production state restoration — inserted on building entities during load.
#[derive(Component, Resource)]
struct PendingLoadVisuals;

#[derive(Component)]
struct PendingProductionRestore {
    active_recipe: Option<usize>,
    progress_timer: SavedTimer,
    input_buffer: Vec<u32>,
    output_buffer: Vec<u32>,
    auto_repeat: bool,
}

/// Brief feedback shown after saving.
#[allow(dead_code)]
#[derive(Resource)]
pub struct SaveFeedback {
    pub timer: Timer,
    pub message: String,
}

// ── Save Data Structures ────────────────────────────────────────────────────

const SAVE_VERSION: u32 = 3;

#[derive(Serialize, Deserialize, Clone)]
pub struct SaveData {
    pub version: u32,
    pub saved_at: String,
    pub elapsed_secs: f64,
    pub game_config: SavedGameConfig,
    pub map_seed: u64,
    pub resources: HashMap<u8, Vec<u32>>,
    pub day_cycle: SavedDayCycle,
    pub victory: SavedVictoryState,
    pub active_player: u8,
    pub ai_controlled: Vec<u8>,
    pub team_config: HashMap<u8, u8>,
    pub faction_base_state: HashMap<u8, bool>,
    pub terrain_ops: Vec<SavedTerrainOp>,
    pub wall_grid: Vec<SavedWallGridCell>,
    pub floor_grid: Vec<SavedFloorGridCell>,
    pub entities: Vec<SavedEntity>,
    pub ai_brains: Vec<(u8, SavedAiBrain)>,
    #[serde(default)]
    pub fog_of_war: Option<SavedFogOfWar>,
    #[serde(default)]
    pub control_groups: Vec<Vec<u32>>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedFogOfWar {
    pub grid_size: usize,
    pub step: f32,
    pub half_map: f32,
    pub explored: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedGameConfig {
    pub player_name: String,
    pub slots: Vec<String>,
    pub local_player_slot: usize,
    pub team_mode: String,
    pub player_teams: [u8; 4],
    pub map_size: String,
    pub resource_density: String,
    pub day_cycle_secs: f32,
    pub starting_resources_mult: f32,
    pub map_seed: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedDayCycle {
    pub time: f32,
    pub cycle_duration: f32,
    pub paused: bool,
    pub phase: u8,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedVictoryState {
    pub faction_status: HashMap<u8, SavedFactionVictoryStatus>,
    pub game_over: bool,
    pub winner: Option<u8>,
    pub winner_team: Option<u8>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedFactionVictoryStatus {
    pub variant: u8, // 0=Alive, 1=GracePeriod, 2=Eliminated
    pub grace_remaining: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedTerrainOp {
    pub center: [f32; 2],
    pub footprint: f32,
    pub target_height: f32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedWallGridCell {
    pub gx: i32,
    pub gz: i32,
    pub entity_save_id: u32,
    pub faction: u8,
    pub piece_kind: u8,
    pub is_gate: bool,
    pub rotation_y: f32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedFloorGridCell {
    pub gx: i32,
    pub gz: i32,
    pub entity_save_id: u32,
    pub faction: u8,
}

// ── Entity serialization ────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedEntity {
    pub save_id: u32,
    pub kind: u16,
    pub faction: Option<u8>,
    pub pos: [f32; 3],
    pub rot_y: f32,
    pub health: Option<[f32; 2]>,
    #[serde(default)]
    pub scale: Option<f32>,
    pub entity_type: SavedEntityType,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum SavedEntityType {
    Unit(SavedUnitData),
    Building(SavedBuildingData),
    ResourceNode(SavedResourceNodeData),
    Mob(SavedMobData),
    Projectile(SavedProjectileData),
    Dying(SavedDyingData),
    Tree(SavedTreeData),
    ItemPickup(SavedItemPickupData),
    GrowingResource(SavedGrowingResourceData),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedUnitData {
    pub state: SavedUnitState,
    pub stance: u8,
    pub speed: f32,
    pub carrying: Option<SavedCarrying>,
    pub experience: Option<[u32; 2]>, // [current, level_as_u8]
    pub move_target: Option<[f32; 3]>,
    pub attack_target_id: Option<u32>,
    pub attack_damage: f32,
    pub attack_range: f32,
    pub attack_cooldown: Option<[f32; 2]>, // [ready_in, interval]
    pub aggro_range: Option<f32>,
    pub building_assignment_id: Option<u32>,
    pub gather_speed: Option<f32>,
    pub carry_capacity: Option<f32>,
    pub gather_accumulator: f32,
    pub abilities: Vec<u8>,
    pub ability_cooldowns: Vec<(u8, f32)>,
    pub display_name: String,
    pub combat_intent: SavedCombatIntent,
    pub task_source: u8,
    #[serde(default)]
    pub inventory_items: Vec<u8>,
    #[serde(default)]
    pub item_states: Vec<SavedItemStateEntry>,
    #[serde(default)]
    pub status_effects: Vec<SavedStatusEffect>,
    #[serde(default)]
    pub veterancy_applied: Option<u8>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedBuildingData {
    pub state: u8, // 0=UnderConstruction, 1=Complete
    pub level: u8,
    pub footprint: f32,
    pub height: f32,
    pub construction_progress: Option<SavedTimer>,
    pub construction_workers: u8,
    pub upgrade_progress: Option<(SavedTimer, u8)>,
    pub rally_point: Option<[f32; 3]>,
    pub training_queue: SavedTrainingQueue,
    pub assigned_worker_ids: Vec<u32>,
    pub resource_processor: Option<SavedResourceProcessor>,
    pub storage_inventory: Option<SavedStorageInventory>,
    pub production_state: Option<SavedProductionState>,
    pub attack_damage: Option<f32>,
    pub attack_range: Option<f32>,
    pub attack_cooldown: Option<[f32; 2]>,
    pub aggro_range: Option<f32>,
    pub attack_target_id: Option<u32>,
    #[serde(default)]
    pub tower_auto_attack: Option<bool>,
    #[serde(default)]
    pub paused: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedResourceNodeData {
    pub resource_type: u8,
    pub amount_remaining: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedMobData {
    pub state: Option<SavedUnitState>,
    pub stance: Option<u8>,
    pub attack_target_id: Option<u32>,
    pub attack_damage: f32,
    pub attack_range: f32,
    pub attack_cooldown: Option<[f32; 2]>,
    pub aggro_range: Option<f32>,
    #[serde(default)]
    pub status_effects: Vec<SavedStatusEffect>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedProjectileData {
    pub source_id: u32,
    pub target_id: u32,
    pub speed: f32,
    pub damage: f32,
    pub damage_type: u8,
    pub fx_kind: u8,
    pub impact_scale: f32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedDyingData {
    pub timer_elapsed: f32,
    pub timer_duration: f32,
    pub original_scale: [f32; 3],
}

#[derive(Serialize, Deserialize, Clone)]
pub enum SavedTreeData {
    Sapling {
        timer_elapsed: f32,
        timer_duration: f32,
        target_scale: f32,
    },
    Growing {
        stage: u8,
        timer_elapsed: f32,
        timer_duration: f32,
        target_scale: f32,
    },
    Mature,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SavedStatusEffect {
    pub kind: u8, // 0=Slow, 1=Stun, 2=Burning
    pub remaining: f32,
    pub strength: f32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedItemStateEntry {
    pub item: u8, // ItemKind index
    pub enabled: bool,
    pub cooldown_remaining: f32,
    pub active_toggled: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedItemPickupData {
    pub item_kind: u8, // ItemKind index
    pub owner_faction: Option<u8>,
    pub expires_at: f32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedGrowingResourceData {
    pub resource_type: u8,
    pub amount: u32,
    pub timer_elapsed: f32,
    pub timer_duration: f32,
    pub target_scale: f32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedTimer {
    pub elapsed: f32,
    pub duration: f32,
    pub mode: u8, // 0=Once, 1=Repeating
    pub finished: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedCarrying {
    pub amount: u32,
    pub weight: f32,
    pub resource_type: Option<u8>,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum SavedUnitState {
    Idle,
    Moving([f32; 3]),
    Attacking(u32),
    Gathering(u32),
    ReturningToDeposit {
        depot: u32,
        gather_node: Option<u32>,
    },
    Depositing {
        depot: u32,
        gather_node: Option<u32>,
    },
    WaitingForStorage {
        depot: u32,
        gather_node: Option<u32>,
    },
    WaitingForDepot {
        gather_node: Option<u32>,
    },
    MovingToPlot([f32; 3]),
    MovingToBuild(u32),
    Building(u32),
    AssignedGathering {
        building: u32,
        phase: SavedAssignedPhase,
    },
    Patrolling {
        target: [f32; 3],
        origin: [f32; 3],
    },
    AttackMoving([f32; 3]),
    HoldPosition,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum SavedAssignedPhase {
    SeekingNode,
    MovingToNode(u32),
    Harvesting { node: u32, timer_secs: f32 },
    ReturningToBuilding,
    Depositing { timer_secs: f32 },
}

#[derive(Serialize, Deserialize, Clone)]
pub enum SavedCombatIntent {
    None,
    Move([f32; 3]),
    Attack(u32, u8),
    AttackMove([f32; 3], u8),
    Hold,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedTrainingQueue {
    pub queue: Vec<u16>,
    pub timer: Option<SavedTimer>,
    pub total_trained: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedResourceProcessor {
    pub resource_types: Vec<u8>,
    pub harvest_radius: f32,
    pub harvest_rate: f32,
    pub max_workers: u8,
    pub buffer: u32,
    pub worker_rate_bonus: f32,
    pub harvest_timer: SavedTimer,
    pub harvest_accumulator: f32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedStorageInventory {
    pub amounts: Vec<u32>,
    pub caps: Vec<u32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedProductionState {
    pub active_recipe: Option<usize>,
    pub progress_timer: SavedTimer,
    pub input_buffer: Vec<u32>,
    pub output_buffer: Vec<u32>,
    pub auto_repeat: bool,
}

// ── AI Brain serialization ──────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedAiBrain {
    pub strategy_timer: f32,
    pub economy_timer: f32,
    pub military_timer: f32,
    pub tactical_timer: f32,
    pub scout_timer: f32,
    pub top_state: u8,
    pub state_entered_at: f32,
    pub posture: u8,
    pub posture_cooldown: f32,
    pub game_time: f32,
    pub pending_transition: Option<u8>,
    pub pending_transition_ticks: u8,
    pub personality: u8,
    pub relation: u8,
    pub difficulty: u8,
    pub ally_attack_target: Option<[f32; 3]>,
    pub last_cooperation_check: f32,
    pub raid_cooldown: f32,
    pub squads: Vec<SavedSquad>,
    pub assigned_units: Vec<(u32, u8)>, // (save_id, role)
    pub desired_workers: u8,
    pub build_queue: Vec<SavedBuildRequest>,
    pub pending_builds: u8,
    pub resource_goal: Option<[u32; 5]>,
    pub income_rates: Vec<f32>,
    pub last_resource_snapshot: Vec<u32>,
    pub attack_ready: bool,
    pub last_attack_time: f32,
    pub attack_started_at: f32,
    pub enemy_composition: Vec<(u16, u32)>,
    pub enemy_strength: f32,
    pub relative_strength: f32,
    pub defense_interrupt: bool,
    pub known_threats: Vec<SavedThreatEntry>,
    pub next_scout_waypoint: usize,
    pub scout_route: Vec<[f32; 3]>,
    pub wall_plan: Option<SavedWallPlan>,
    pub base_position: Option<[f32; 3]>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedSquad {
    pub role: u8,
    pub member_ids: Vec<u32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedBuildRequest {
    pub kind: u16,
    pub priority: u8,
    pub near_position: Option<[f32; 3]>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedThreatEntry {
    pub position: [f32; 3],
    pub estimated_strength: f32,
    pub last_seen: f32,
    pub entity_count: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedWallPlan {
    pub runs: Vec<([f32; 3], [f32; 3])>,
    pub completed: Vec<bool>,
}

// ── Conversion helpers ──────────────────────────────────────────────────────

fn resource_type_from_index(i: usize) -> ResourceType {
    ResourceType::ALL
        .get(i)
        .copied()
        .unwrap_or(ResourceType::Wood)
}

fn faction_to_u8(f: &Faction) -> u8 {
    f.to_net_index() as u8
}

fn u8_to_faction(v: u8) -> Faction {
    Faction::from_net_index(v).unwrap_or(Faction::Player1)
}

fn vec3_to_arr(v: Vec3) -> [f32; 3] {
    [v.x, v.y, v.z]
}

fn arr_to_vec3(a: [f32; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

fn save_timer(t: &Timer) -> SavedTimer {
    SavedTimer {
        elapsed: t.elapsed_secs(),
        duration: t.duration().as_secs_f32(),
        mode: if t.mode() == TimerMode::Repeating {
            1
        } else {
            0
        },
        finished: t.is_finished(),
    }
}

fn restore_timer(s: &SavedTimer) -> Timer {
    let mode = if s.mode == 1 {
        TimerMode::Repeating
    } else {
        TimerMode::Once
    };
    let mut t = Timer::from_seconds(s.duration, mode);
    t.tick(std::time::Duration::from_secs_f32(s.elapsed));
    t
}

fn entity_kind_to_u16(k: EntityKind) -> u16 {
    k.to_index() as u16
}

fn u16_to_entity_kind(v: u16) -> EntityKind {
    EntityKind::from_index(v).unwrap_or(EntityKind::Worker)
}

fn unit_state_to_saved(state: &UnitState, entity_map: &HashMap<Entity, u32>) -> SavedUnitState {
    match state {
        UnitState::Idle => SavedUnitState::Idle,
        UnitState::Moving(v) => SavedUnitState::Moving(vec3_to_arr(*v)),
        UnitState::Attacking(e) => {
            SavedUnitState::Attacking(*entity_map.get(e).unwrap_or(&u32::MAX))
        }
        UnitState::Gathering(e) => {
            SavedUnitState::Gathering(*entity_map.get(e).unwrap_or(&u32::MAX))
        }
        UnitState::ReturningToDeposit { depot, gather_node } => {
            SavedUnitState::ReturningToDeposit {
                depot: *entity_map.get(depot).unwrap_or(&u32::MAX),
                gather_node: gather_node.and_then(|e| entity_map.get(&e).copied()),
            }
        }
        UnitState::Depositing { depot, gather_node } => SavedUnitState::Depositing {
            depot: *entity_map.get(depot).unwrap_or(&u32::MAX),
            gather_node: gather_node.and_then(|e| entity_map.get(&e).copied()),
        },
        UnitState::WaitingForStorage { depot, gather_node } => SavedUnitState::WaitingForStorage {
            depot: *entity_map.get(depot).unwrap_or(&u32::MAX),
            gather_node: gather_node.and_then(|e| entity_map.get(&e).copied()),
        },
        UnitState::WaitingForDepot { gather_node } => SavedUnitState::WaitingForDepot {
            gather_node: gather_node.and_then(|e| entity_map.get(&e).copied()),
        },
        UnitState::MovingToPlot(v) => SavedUnitState::MovingToPlot(vec3_to_arr(*v)),
        UnitState::MovingToBuild(e) => {
            SavedUnitState::MovingToBuild(*entity_map.get(e).unwrap_or(&u32::MAX))
        }
        UnitState::Building(e) => SavedUnitState::Building(*entity_map.get(e).unwrap_or(&u32::MAX)),
        UnitState::AssignedGathering { building, phase } => SavedUnitState::AssignedGathering {
            building: *entity_map.get(building).unwrap_or(&u32::MAX),
            phase: match phase {
                AssignedPhase::SeekingNode => SavedAssignedPhase::SeekingNode,
                AssignedPhase::MovingToNode(e) => {
                    SavedAssignedPhase::MovingToNode(*entity_map.get(e).unwrap_or(&u32::MAX))
                }
                AssignedPhase::Harvesting { node, timer_secs } => SavedAssignedPhase::Harvesting {
                    node: *entity_map.get(node).unwrap_or(&u32::MAX),
                    timer_secs: *timer_secs,
                },
                AssignedPhase::ReturningToBuilding => SavedAssignedPhase::ReturningToBuilding,
                AssignedPhase::Depositing { timer_secs } => SavedAssignedPhase::Depositing {
                    timer_secs: *timer_secs,
                },
            },
        },
        UnitState::Patrolling { target, origin } => SavedUnitState::Patrolling {
            target: vec3_to_arr(*target),
            origin: vec3_to_arr(*origin),
        },
        UnitState::AttackMoving(v) => SavedUnitState::AttackMoving(vec3_to_arr(*v)),
        UnitState::HoldPosition => SavedUnitState::HoldPosition,
    }
}

fn saved_to_unit_state(state: &SavedUnitState, id_map: &HashMap<u32, Entity>) -> UnitState {
    let resolve = |id: &u32| -> Entity { id_map.get(id).copied().unwrap_or(Entity::PLACEHOLDER) };
    match state {
        SavedUnitState::Idle => UnitState::Idle,
        SavedUnitState::Moving(v) => UnitState::Moving(arr_to_vec3(*v)),
        SavedUnitState::Attacking(id) => UnitState::Attacking(resolve(id)),
        SavedUnitState::Gathering(id) => UnitState::Gathering(resolve(id)),
        SavedUnitState::ReturningToDeposit { depot, gather_node } => {
            UnitState::ReturningToDeposit {
                depot: resolve(depot),
                gather_node: gather_node.map(|id| resolve(&id)),
            }
        }
        SavedUnitState::Depositing { depot, gather_node } => UnitState::Depositing {
            depot: resolve(depot),
            gather_node: gather_node.map(|id| resolve(&id)),
        },
        SavedUnitState::WaitingForStorage { depot, gather_node } => UnitState::WaitingForStorage {
            depot: resolve(depot),
            gather_node: gather_node.map(|id| resolve(&id)),
        },
        SavedUnitState::WaitingForDepot { gather_node } => UnitState::WaitingForDepot {
            gather_node: gather_node.map(|id| resolve(&id)),
        },
        SavedUnitState::MovingToPlot(v) => UnitState::MovingToPlot(arr_to_vec3(*v)),
        SavedUnitState::MovingToBuild(id) => UnitState::MovingToBuild(resolve(id)),
        SavedUnitState::Building(id) => UnitState::Building(resolve(id)),
        SavedUnitState::AssignedGathering { building, phase } => UnitState::AssignedGathering {
            building: resolve(building),
            phase: match phase {
                SavedAssignedPhase::SeekingNode => AssignedPhase::SeekingNode,
                SavedAssignedPhase::MovingToNode(id) => AssignedPhase::MovingToNode(resolve(id)),
                SavedAssignedPhase::Harvesting { node, timer_secs } => AssignedPhase::Harvesting {
                    node: resolve(node),
                    timer_secs: *timer_secs,
                },
                SavedAssignedPhase::ReturningToBuilding => AssignedPhase::ReturningToBuilding,
                SavedAssignedPhase::Depositing { timer_secs } => AssignedPhase::Depositing {
                    timer_secs: *timer_secs,
                },
            },
        },
        SavedUnitState::Patrolling { target, origin } => UnitState::Patrolling {
            target: arr_to_vec3(*target),
            origin: arr_to_vec3(*origin),
        },
        SavedUnitState::AttackMoving(v) => UnitState::AttackMoving(arr_to_vec3(*v)),
        SavedUnitState::HoldPosition => UnitState::HoldPosition,
    }
}

fn combat_intent_to_saved(
    intent: &CombatIntent,
    entity_map: &HashMap<Entity, u32>,
) -> SavedCombatIntent {
    match intent {
        CombatIntent::None => SavedCombatIntent::None,
        CombatIntent::Move(v) => SavedCombatIntent::Move(vec3_to_arr(*v)),
        CombatIntent::Attack(e, src) => SavedCombatIntent::Attack(
            *entity_map.get(e).unwrap_or(&u32::MAX),
            match src {
                IntentSource::Manual => 0,
                IntentSource::Auto => 1,
            },
        ),
        CombatIntent::AttackMove(v, src) => SavedCombatIntent::AttackMove(
            vec3_to_arr(*v),
            match src {
                IntentSource::Manual => 0,
                IntentSource::Auto => 1,
            },
        ),
        CombatIntent::Hold => SavedCombatIntent::Hold,
    }
}


fn veterancy_to_u8(v: &VeterancyLevel) -> u8 {
    match v {
        VeterancyLevel::Recruit => 0,
        VeterancyLevel::Veteran => 1,
        VeterancyLevel::Elite => 2,
    }
}

fn ability_id_to_u8(a: &AbilityId) -> u8 {
    match a {
        AbilityId::KnightCharge => 0,
        AbilityId::MageFireball => 1,
        AbilityId::MageFrostNova => 2,
        AbilityId::PriestHeal => 3,
        AbilityId::PriestHolySmite => 4,
        AbilityId::CatapultAoeBoulder => 5,
    }
}

fn u8_to_ability_id(v: u8) -> AbilityId {
    match v {
        0 => AbilityId::KnightCharge,
        1 => AbilityId::MageFireball,
        2 => AbilityId::MageFrostNova,
        3 => AbilityId::PriestHeal,
        4 => AbilityId::PriestHolySmite,
        _ => AbilityId::CatapultAoeBoulder,
    }
}

fn item_kind_to_u8(k: &ItemKind) -> u8 {
    match k {
        ItemKind::PaddedVest => 0,
        ItemKind::BronzeCuirass => 1,
        ItemKind::PlateCuirass => 2,
        ItemKind::CrusaderHelm => 3,
        ItemKind::KettleHelm => 4,
        ItemKind::VikingHelm => 5,
        ItemKind::JewelRing => 6,
        ItemKind::PlainBand => 7,
        ItemKind::WeddingBand => 8,
        ItemKind::GoldenBand => 9,
        ItemKind::TwinRings => 10,
        ItemKind::LinkedRings => 11,
        ItemKind::ArmingSword => 12,
        ItemKind::VikingBlade => 13,
        ItemKind::BattleStaff => 14,
        ItemKind::MageCrozier => 15,
        ItemKind::YewLongbow => 16,
        ItemKind::WarBow => 17,
    }
}

fn u8_to_item_kind(v: u8) -> ItemKind {
    match v {
        0 => ItemKind::PaddedVest,
        1 => ItemKind::BronzeCuirass,
        2 => ItemKind::PlateCuirass,
        3 => ItemKind::CrusaderHelm,
        4 => ItemKind::KettleHelm,
        5 => ItemKind::VikingHelm,
        6 => ItemKind::JewelRing,
        7 => ItemKind::PlainBand,
        8 => ItemKind::WeddingBand,
        9 => ItemKind::GoldenBand,
        10 => ItemKind::TwinRings,
        11 => ItemKind::LinkedRings,
        12 => ItemKind::ArmingSword,
        13 => ItemKind::VikingBlade,
        14 => ItemKind::BattleStaff,
        15 => ItemKind::MageCrozier,
        16 => ItemKind::YewLongbow,
        _ => ItemKind::WarBow,
    }
}

fn status_effect_to_saved(effects: &StatusEffects) -> Vec<SavedStatusEffect> {
    effects
        .effects
        .iter()
        .map(|e| SavedStatusEffect {
            kind: match e.kind {
                StatusEffectKind::Slow => 0,
                StatusEffectKind::Stun => 1,
                StatusEffectKind::Burning => 2,
            },
            remaining: e.remaining,
            strength: e.strength,
        })
        .collect()
}

fn saved_to_status_effects(saved: &[SavedStatusEffect]) -> StatusEffects {
    StatusEffects {
        effects: saved
            .iter()
            .map(|s| ActiveStatusEffect {
                kind: match s.kind {
                    0 => StatusEffectKind::Slow,
                    1 => StatusEffectKind::Stun,
                    _ => StatusEffectKind::Burning,
                },
                remaining: s.remaining,
                strength: s.strength,
            })
            .collect(),
    }
}

fn ai_top_state_to_u8(s: &AiTopState) -> u8 {
    match s {
        AiTopState::Founding => 0,
        AiTopState::EarlyEconomy => 1,
        AiTopState::Militarize => 2,
        AiTopState::Expanding => 3,
        AiTopState::Attacking => 4,
        AiTopState::Defending => 5,
        AiTopState::LateGame => 6,
    }
}

fn u8_to_ai_top_state(v: u8) -> AiTopState {
    match v {
        0 => AiTopState::Founding,
        1 => AiTopState::EarlyEconomy,
        2 => AiTopState::Militarize,
        3 => AiTopState::Expanding,
        4 => AiTopState::Attacking,
        5 => AiTopState::Defending,
        _ => AiTopState::LateGame,
    }
}

fn squad_role_to_u8(r: &SquadRole) -> u8 {
    match r {
        SquadRole::GatherWood => 0,
        SquadRole::GatherCopper => 1,
        SquadRole::GatherIron => 2,
        SquadRole::GatherGold => 3,
        SquadRole::GatherOil => 4,
        SquadRole::BuildConstruction => 5,
        SquadRole::DefenseSquad => 6,
        SquadRole::AttackSquad => 7,
        SquadRole::Scout => 8,
        SquadRole::Raider => 9,
    }
}

fn u8_to_squad_role(v: u8) -> SquadRole {
    match v {
        0 => SquadRole::GatherWood,
        1 => SquadRole::GatherCopper,
        2 => SquadRole::GatherIron,
        3 => SquadRole::GatherGold,
        4 => SquadRole::GatherOil,
        5 => SquadRole::BuildConstruction,
        6 => SquadRole::DefenseSquad,
        7 => SquadRole::AttackSquad,
        8 => SquadRole::Scout,
        _ => SquadRole::Raider,
    }
}

// ── Save helpers ───────────────────────────────────────────────────────────

struct BaseEntityFields {
    save_id: u32,
    kind: u16,
    faction: Option<u8>,
    pos: [f32; 3],
    rot_y: f32,
    health: Option<[f32; 2]>,
    scale: Option<f32>,
}

fn collect_base_fields(world: &World, entity: Entity, save_id: u32) -> Option<BaseEntityFields> {
    let transform = world.get::<Transform>(entity)?;
    let kind = world.get::<EntityKind>(entity);
    let faction = world.get::<Faction>(entity);
    let health = world.get::<Health>(entity);

    let scale = if (transform.scale - Vec3::ONE).length_squared() > 0.001 {
        Some(transform.scale.x)
    } else {
        None
    };

    Some(BaseEntityFields {
        save_id,
        kind: kind.map(|k| entity_kind_to_u16(*k)).unwrap_or(0),
        faction: faction.map(faction_to_u8),
        pos: vec3_to_arr(transform.translation),
        rot_y: transform.rotation.to_euler(EulerRot::YXZ).0,
        health: health.map(|h| [h.current, h.max]),
        scale,
    })
}

fn collect_combat_components(
    world: &World,
    entity: Entity,
    emap: &HashMap<Entity, u32>,
) -> (f32, f32, Option<[f32; 2]>, Option<f32>, Option<u32>) {
    let attack_damage = world
        .get::<AttackDamage>(entity)
        .map(|d| d.0)
        .unwrap_or(0.0);
    let attack_range = world.get::<AttackRange>(entity).map(|r| r.0).unwrap_or(0.0);
    let attack_cooldown = world
        .get::<AttackCooldown>(entity)
        .map(|c| [c.ready_in, c.interval]);
    let aggro_range = world.get::<AggroRange>(entity).map(|a| a.0);
    let attack_target_id = world
        .get::<AttackTarget>(entity)
        .and_then(|t| emap.get(&t.0).copied());
    (
        attack_damage,
        attack_range,
        attack_cooldown,
        aggro_range,
        attack_target_id,
    )
}

fn restore_combat_components(
    commands: &mut Commands,
    entity: Entity,
    attack_damage: f32,
    attack_range: f32,
    attack_cooldown: Option<[f32; 2]>,
    aggro_range: Option<f32>,
) {
    commands.entity(entity).insert(AttackDamage(attack_damage));
    commands.entity(entity).insert(AttackRange(attack_range));
    if let Some([ready, interval]) = attack_cooldown {
        commands.entity(entity).insert(AttackCooldown {
            ready_in: ready,
            interval,
        });
    }
    if let Some(aggro) = aggro_range {
        commands.entity(entity).insert(AggroRange(aggro));
    }
}

fn spawn_and_setup_base(
    commands: &mut Commands,
    cache: &EntityVisualCache,
    registry: &BlueprintRegistry,
    building_models: Option<&BuildingModelAssets>,
    unit_models: Option<&UnitModelAssets>,
    height_map: &HeightMap,
    saved: &SavedEntity,
) -> Entity {
    let kind = u16_to_entity_kind(saved.kind);
    let faction = u8_to_faction(saved.faction.unwrap_or(0));
    let pos = arr_to_vec3(saved.pos);
    let rot = Quat::from_rotation_y(saved.rot_y);

    let e = crate::blueprints::spawn_from_blueprint_with_faction(
        commands,
        cache,
        kind,
        pos,
        registry,
        building_models,
        unit_models,
        height_map,
        faction,
    );
    commands.entity(e).insert(Transform {
        translation: pos,
        rotation: rot,
        ..default()
    });
    if let Some([cur, max]) = saved.health {
        commands.entity(e).insert(Health { current: cur, max });
    }
    e
}

// ── SAVE SYSTEM ─────────────────────────────────────────────────────────────

/// Exclusive system wrapper that avoids Bevy's 16-param limit.
fn handle_save_game_exclusive(world: &mut World) {
    // Check if there's a save trigger
    let Some(trigger) = world.remove_resource::<SaveTrigger>() else {
        return;
    };

    // Check state
    let state = world.resource::<State<AppState>>();
    if *state.get() != AppState::InGame {
        return;
    }

    let net_role = world.resource::<NetRole>();
    if *net_role != NetRole::Offline {
        warn!("Save is only available in single-player mode");
        return;
    }

    let event_label = trigger.label;
    handle_save_game_event(world, event_label);
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn handle_save_game_event(world: &mut World, event_label: Option<String>) {
    info!("Saving game...");

    // Extract resources from world
    let time = world.resource::<Time>();
    let match_start = world.get_resource::<MatchStartTime>();
    let elapsed = match_start
        .map(|ms| time.elapsed_secs_f64() - ms.0)
        .unwrap_or(0.0);

    let config = world.resource::<GameSetupConfig>().clone();
    let map_seed = world.resource::<MapSeed>().0;
    let mut res_map = HashMap::new();
    for (faction, pr) in &world.resource::<AllPlayerResources>().resources {
        res_map.insert(faction_to_u8(faction), pr.amounts.to_vec());
    }
    let active_player_faction = world.resource::<ActivePlayer>().0;
    let ai_controlled_factions: Vec<u8> = world
        .resource::<AiControlledFactions>()
        .factions
        .iter()
        .map(faction_to_u8)
        .collect();
    let team_config_data: HashMap<u8, u8> = world
        .resource::<TeamConfig>()
        .teams
        .iter()
        .map(|(f, t)| (faction_to_u8(f), *t))
        .collect();
    let faction_base_data: HashMap<u8, bool> = world
        .resource::<FactionBaseState>()
        .founded
        .iter()
        .map(|(f, b)| (faction_to_u8(f), *b))
        .collect();

    let day_cycle_data = world
        .get_resource::<DayCycle>()
        .map(|dc| SavedDayCycle {
            time: dc.time,
            cycle_duration: dc.cycle_duration,
            paused: dc.paused,
            phase: match dc.phase {
                DayPhase::Night => 0,
                DayPhase::Dawn => 1,
                DayPhase::Day => 2,
                DayPhase::Dusk => 3,
            },
        })
        .unwrap_or(SavedDayCycle {
            time: 0.25,
            cycle_duration: 600.0,
            paused: false,
            phase: 2,
        });

    let saved_victory = world
        .get_resource::<VictoryState>()
        .map(|vs| SavedVictoryState {
            faction_status: vs
                .faction_status
                .iter()
                .map(|(f, s)| {
                    (
                        faction_to_u8(f),
                        match s {
                            VictFactionStatus::Alive => SavedFactionVictoryStatus {
                                variant: 0,
                                grace_remaining: None,
                            },
                            VictFactionStatus::GracePeriod { remaining } => {
                                SavedFactionVictoryStatus {
                                    variant: 1,
                                    grace_remaining: Some(*remaining),
                                }
                            }
                            VictFactionStatus::Eliminated => SavedFactionVictoryStatus {
                                variant: 2,
                                grace_remaining: None,
                            },
                        },
                    )
                })
                .collect(),
            game_over: vs.game_over,
            winner: vs.winner.map(|f| faction_to_u8(&f)),
            winner_team: vs.winner_team,
        })
        .unwrap_or(SavedVictoryState {
            faction_status: HashMap::new(),
            game_over: false,
            winner: None,
            winner_team: None,
        });

    let saved_terrain_ops: Vec<SavedTerrainOp> = world
        .get_resource::<TerrainShapeSyncState>()
        .map(|ts| {
            ts.applied_history_ordered
                .iter()
                .map(|op| SavedTerrainOp {
                    center: op.center,
                    footprint: op.footprint,
                    target_height: op.target_height,
                })
                .collect()
        })
        .unwrap_or_default();

    let saved_config = SavedGameConfig {
        player_name: config.player_name.clone(),
        slots: config
            .slots
            .iter()
            .map(|s| match s {
                SlotOccupant::Human => "Human".to_string(),
                SlotOccupant::Ai(d) => format!("Ai:{:?}", d),
                SlotOccupant::Open => "Open".to_string(),
                SlotOccupant::Closed => "Closed".to_string(),
            })
            .collect(),
        local_player_slot: config.local_player_slot,
        team_mode: format!("{:?}", config.team_mode),
        player_teams: config.player_teams,
        map_size: format!("{:?}", config.map_size),
        resource_density: format!("{:?}", config.resource_density),
        day_cycle_secs: config.day_cycle_secs,
        starting_resources_mult: config.starting_resources_mult,
        map_seed,
    };

    // Collect all game entities using component-based queries
    let mut entity_to_save_id: HashMap<Entity, u32> = HashMap::new();
    let mut next_id: u32 = 0;
    let mut all_game_entities: Vec<Entity> = Vec::new();

    // Collect entities by type marker
    let mut unit_entities: Vec<Entity> = Vec::new();
    let mut building_entities: Vec<Entity> = Vec::new();
    let mut node_entities: Vec<Entity> = Vec::new();
    let mut mob_entities: Vec<Entity> = Vec::new();

    {
        let mut q = world.query_filtered::<Entity, (With<Unit>, Without<Mob>, Without<Dying>)>();
        for e in q.iter(world) {
            entity_to_save_id.insert(e, next_id);
            next_id += 1;
            unit_entities.push(e);
            all_game_entities.push(e);
        }
    }
    {
        let mut q =
            world.query_filtered::<Entity, (With<Building>, Without<FloorTile>, Without<Dying>)>();
        for e in q.iter(world) {
            entity_to_save_id.insert(e, next_id);
            next_id += 1;
            building_entities.push(e);
            all_game_entities.push(e);
        }
    }
    {
        let mut q = world.query_filtered::<Entity, (With<ResourceNode>, Without<Dying>)>();
        for e in q.iter(world) {
            entity_to_save_id.insert(e, next_id);
            next_id += 1;
            node_entities.push(e);
            all_game_entities.push(e);
        }
    }
    {
        let mut q = world.query_filtered::<Entity, (With<Mob>, Without<Dying>)>();
        for e in q.iter(world) {
            entity_to_save_id.insert(e, next_id);
            next_id += 1;
            mob_entities.push(e);
            all_game_entities.push(e);
        }
    }

    let emap = &entity_to_save_id;
    let mut saved_entities = Vec::new();

    // Helper macro-like closure to get components from world
    macro_rules! get {
        ($entity:expr, $T:ty) => {
            world.get::<$T>($entity)
        };
    }

    // Units
    for &entity in &unit_entities {
        let Some(base) = collect_base_fields(world, entity, emap[&entity]) else {
            warn!("Skipping unit entity {entity:?}: missing Transform");
            continue;
        };
        let Some(state) = get!(entity, UnitState) else {
            continue;
        };
        let (atk_dmg, atk_rng, atk_cd, aggro, atk_target) =
            collect_combat_components(world, entity, emap);

        saved_entities.push(SavedEntity {
            save_id: base.save_id,
            kind: base.kind,
            faction: base.faction,
            pos: base.pos,
            rot_y: base.rot_y,
            health: base.health,
            scale: base.scale,
            entity_type: SavedEntityType::Unit(SavedUnitData {
                state: unit_state_to_saved(state, emap),
                stance: get!(entity, UnitStance).map(|s| s.to_u8()).unwrap_or(2),
                speed: get!(entity, UnitSpeed).map(|s| s.0).unwrap_or(0.0),
                carrying: get!(entity, Carrying).map(|c| SavedCarrying {
                    amount: c.amount,
                    weight: c.weight,
                    resource_type: c.resource_type.map(|r| r.index() as u8),
                }),
                experience: get!(entity, Experience)
                    .map(|e| [e.current, veterancy_to_u8(&e.level) as u32]),
                move_target: get!(entity, MoveTarget).map(|m| vec3_to_arr(m.0)),
                attack_target_id: atk_target,
                attack_damage: atk_dmg,
                attack_range: atk_rng,
                attack_cooldown: atk_cd,
                aggro_range: aggro,
                building_assignment_id: get!(entity, BuildingAssignment)
                    .and_then(|b| emap.get(&b.0).copied()),
                gather_speed: get!(entity, GatherSpeed).map(|g| g.0),
                carry_capacity: get!(entity, CarryCapacity).map(|c| c.0),
                gather_accumulator: get!(entity, GatherAccumulator).map(|g| g.0).unwrap_or(0.0),
                abilities: get!(entity, UnitAbilities)
                    .map(|a| a.abilities.iter().map(ability_id_to_u8).collect())
                    .unwrap_or_default(),
                ability_cooldowns: get!(entity, UnitAbilities)
                    .map(|a| {
                        a.cooldowns
                            .iter()
                            .map(|(id, cd)| (ability_id_to_u8(id), *cd))
                            .collect()
                    })
                    .unwrap_or_default(),
                display_name: get!(entity, UnitDisplayName)
                    .map(|d| d.0.clone())
                    .unwrap_or_default(),
                combat_intent: get!(entity, CombatIntent)
                    .map(|c| combat_intent_to_saved(c, emap))
                    .unwrap_or(SavedCombatIntent::None),
                task_source: match get!(entity, TaskSource) {
                    Some(TaskSource::Manual) => 0,
                    _ => 1,
                },
                inventory_items: get!(entity, UnitInventory)
                    .map(|inv| inv.items.iter().map(item_kind_to_u8).collect())
                    .unwrap_or_default(),
                item_states: get!(entity, ItemRuntimeState)
                    .map(|rt| {
                        rt.items
                            .iter()
                            .map(|s| SavedItemStateEntry {
                                item: item_kind_to_u8(&s.item),
                                enabled: s.enabled,
                                cooldown_remaining: s.cooldown_remaining,
                                active_toggled: s.active_toggled,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                status_effects: get!(entity, StatusEffects)
                    .map(|s| status_effect_to_saved(s))
                    .unwrap_or_default(),
                veterancy_applied: get!(entity, VeterancyApplied).map(|v| veterancy_to_u8(&v.0)),
            }),
        });
    }

    // Buildings
    for &entity in &building_entities {
        let Some(base) = collect_base_fields(world, entity, emap[&entity]) else {
            warn!("Skipping building entity {entity:?}: missing Transform");
            continue;
        };
        let Some(state) = get!(entity, BuildingState) else {
            continue;
        };

        saved_entities.push(SavedEntity {
            save_id: base.save_id,
            kind: base.kind,
            faction: base.faction,
            pos: base.pos,
            rot_y: base.rot_y,
            health: base.health,
            scale: base.scale,
            entity_type: SavedEntityType::Building(SavedBuildingData {
                state: match state {
                    BuildingState::UnderConstruction => 0,
                    BuildingState::Complete => 1,
                },
                level: get!(entity, BuildingLevel).map(|l| l.0).unwrap_or(1),
                footprint: get!(entity, BuildingFootprint).map(|f| f.0).unwrap_or(3.0),
                height: get!(entity, BuildingHeight).map(|h| h.0).unwrap_or(4.0),
                construction_progress: get!(entity, ConstructionProgress)
                    .map(|c| save_timer(&c.timer)),
                construction_workers: get!(entity, ConstructionWorkers).map(|c| c.0).unwrap_or(0),
                upgrade_progress: get!(entity, UpgradeProgress)
                    .map(|u| (save_timer(&u.timer), u.target_level)),
                rally_point: get!(entity, RallyPoint).map(|r| vec3_to_arr(r.0)),
                training_queue: get!(entity, TrainingQueue)
                    .map(|t| SavedTrainingQueue {
                        queue: t.queue.iter().map(|k| entity_kind_to_u16(*k)).collect(),
                        timer: t.timer.as_ref().map(save_timer),
                        total_trained: t.total_trained,
                    })
                    .unwrap_or(SavedTrainingQueue {
                        queue: Vec::new(),
                        timer: None,
                        total_trained: 0,
                    }),
                assigned_worker_ids: get!(entity, AssignedWorkers)
                    .map(|a| {
                        a.workers
                            .iter()
                            .filter_map(|e| emap.get(e).copied())
                            .collect()
                    })
                    .unwrap_or_default(),
                resource_processor: get!(entity, ResourceProcessor).map(|p| {
                    SavedResourceProcessor {
                        resource_types: p.resource_types.iter().map(|r| r.index() as u8).collect(),
                        harvest_radius: p.harvest_radius,
                        harvest_rate: p.harvest_rate,
                        max_workers: p.max_workers,
                        buffer: p.buffer,
                        worker_rate_bonus: p.worker_rate_bonus,
                        harvest_timer: save_timer(&p.harvest_timer),
                        harvest_accumulator: p.harvest_accumulator,
                    }
                }),
                storage_inventory: get!(entity, StorageInventory).map(|s| SavedStorageInventory {
                    amounts: s.amounts.to_vec(),
                    caps: s.caps.to_vec(),
                }),
                production_state: get!(entity, ProductionState).map(|p| SavedProductionState {
                    active_recipe: p.active_recipe,
                    progress_timer: save_timer(&p.progress_timer),
                    input_buffer: p.input_buffer.to_vec(),
                    output_buffer: p.output_buffer.to_vec(),
                    auto_repeat: p.auto_repeat,
                }),
                attack_damage: get!(entity, AttackDamage).map(|d| d.0),
                attack_range: get!(entity, AttackRange).map(|r| r.0),
                attack_cooldown: get!(entity, AttackCooldown).map(|c| [c.ready_in, c.interval]),
                aggro_range: get!(entity, AggroRange).map(|a| a.0),
                attack_target_id: get!(entity, AttackTarget).and_then(|t| emap.get(&t.0).copied()),
                tower_auto_attack: get!(entity, TowerAutoAttackEnabled).map(|t| t.0),
                paused: world.get::<BuildingPaused>(entity).is_some(),
            }),
        });
    }

    // Resource nodes
    for &entity in &node_entities {
        let Some(base) = collect_base_fields(world, entity, emap[&entity]) else {
            warn!("Skipping resource node entity {entity:?}: missing Transform");
            continue;
        };
        let Some(node) = get!(entity, ResourceNode) else {
            continue;
        };
        saved_entities.push(SavedEntity {
            save_id: base.save_id,
            kind: 0,
            faction: None,
            pos: base.pos,
            rot_y: base.rot_y,
            health: None,
            scale: base.scale,
            entity_type: SavedEntityType::ResourceNode(SavedResourceNodeData {
                resource_type: node.resource_type.index() as u8,
                amount_remaining: node.amount_remaining,
            }),
        });
    }

    // Mobs
    for &entity in &mob_entities {
        let Some(base) = collect_base_fields(world, entity, emap[&entity]) else {
            warn!("Skipping mob entity {entity:?}: missing Transform");
            continue;
        };
        let (atk_dmg, atk_rng, atk_cd, aggro, atk_target) =
            collect_combat_components(world, entity, emap);

        saved_entities.push(SavedEntity {
            save_id: base.save_id,
            kind: base.kind,
            faction: base.faction,
            pos: base.pos,
            rot_y: base.rot_y,
            health: base.health,
            scale: base.scale,
            entity_type: SavedEntityType::Mob(SavedMobData {
                state: get!(entity, UnitState).map(|s| unit_state_to_saved(s, emap)),
                stance: get!(entity, UnitStance).map(|s| s.to_u8()),
                attack_target_id: atk_target,
                attack_damage: atk_dmg,
                attack_range: atk_rng,
                attack_cooldown: atk_cd,
                aggro_range: aggro,
                status_effects: get!(entity, StatusEffects)
                    .map(|s| status_effect_to_saved(s))
                    .unwrap_or_default(),
            }),
        });
    }

    // Skip projectiles, dying entities, and trees — they're ephemeral

    // Item pickups on the ground
    {
        let mut q = world.query_filtered::<Entity, (With<ItemPickup>, Without<PickupCollectVfx>)>();
        let pickup_entities: Vec<Entity> = q.iter(world).collect();
        for entity in pickup_entities {
            let Some(tf) = world.get::<Transform>(entity) else {
                continue;
            };
            let Some(pickup) = world.get::<ItemPickup>(entity) else {
                continue;
            };
            saved_entities.push(SavedEntity {
                save_id: next_id,
                kind: 0,
                faction: None,
                pos: vec3_to_arr(tf.translation),
                rot_y: 0.0,
                health: None,
                scale: None,
                entity_type: SavedEntityType::ItemPickup(SavedItemPickupData {
                    item_kind: item_kind_to_u8(&pickup.item),
                    owner_faction: pickup.owner.as_ref().map(faction_to_u8),
                    expires_at: pickup.expires_at,
                }),
            });
            next_id += 1;
        }
    }

    // Growing resources (mid-respawn)
    {
        let mut q =
            world.query_filtered::<Entity, (With<GrowingResource>, Without<ResourceNode>)>();
        let growing_entities: Vec<Entity> = q.iter(world).collect();
        for entity in growing_entities {
            let Some(tf) = world.get::<Transform>(entity) else {
                continue;
            };
            let Some(gr) = world.get::<GrowingResource>(entity) else {
                continue;
            };
            saved_entities.push(SavedEntity {
                save_id: next_id,
                kind: 0,
                faction: None,
                pos: vec3_to_arr(tf.translation),
                rot_y: tf.rotation.to_euler(EulerRot::YXZ).0,
                health: None,
                scale: None,
                entity_type: SavedEntityType::GrowingResource(SavedGrowingResourceData {
                    resource_type: gr.resource_type.index() as u8,
                    amount: gr.amount,
                    timer_elapsed: gr.timer.elapsed_secs(),
                    timer_duration: gr.timer.duration().as_secs_f32(),
                    target_scale: gr.target_scale,
                }),
            });
            next_id += 1;
        }
    }

    // Wall grid
    let saved_wall_grid: Vec<SavedWallGridCell> = world
        .get_resource::<WallGrid>()
        .map(|wg| {
            wg.cells
                .iter()
                .map(|((gx, gz), cell)| SavedWallGridCell {
                    gx: *gx,
                    gz: *gz,
                    entity_save_id: *emap.get(&cell.entity).unwrap_or(&u32::MAX),
                    faction: faction_to_u8(&cell._faction),
                    piece_kind: cell.piece_kind as u8,
                    is_gate: cell.is_gate,
                    rotation_y: cell.rotation_y,
                })
                .collect()
        })
        .unwrap_or_default();

    // Floor grid
    let saved_floor_grid: Vec<SavedFloorGridCell> = world
        .get_resource::<FloorGrid>()
        .map(|fg| {
            fg.cells
                .iter()
                .map(|((gx, gz), cell)| SavedFloorGridCell {
                    gx: *gx,
                    gz: *gz,
                    entity_save_id: *emap.get(&cell.entity).unwrap_or(&u32::MAX),
                    faction: faction_to_u8(&cell._faction),
                })
                .collect()
        })
        .unwrap_or_default();

    // AI brains
    let saved_ai: Vec<(u8, SavedAiBrain)> = world
        .get_resource::<AiState>()
        .map(|ai| {
            ai.factions
                .iter()
                .map(|(faction, brain)| (faction_to_u8(faction), save_ai_brain(brain, emap)))
                .collect()
        })
        .unwrap_or_default();

    // Fog of war
    let saved_fog = world
        .get_resource::<FogOfWarMap>()
        .map(|fog| SavedFogOfWar {
            grid_size: fog.grid_size,
            step: fog.step,
            half_map: fog.half_map,
            explored: fog.explored.clone(),
        });

    // Control groups
    let saved_control_groups: Vec<Vec<u32>> = world
        .get_resource::<ControlGroups>()
        .map(|cg| {
            cg.groups
                .iter()
                .map(|group| group.iter().filter_map(|e| emap.get(e).copied()).collect())
                .collect()
        })
        .unwrap_or_else(|| vec![Vec::new(); 9]);

    // Build SaveData
    let save_data = SaveData {
        version: SAVE_VERSION,
        saved_at: chrono_now(),
        elapsed_secs: elapsed,
        game_config: saved_config,
        map_seed,
        resources: res_map,
        day_cycle: day_cycle_data,
        victory: saved_victory,
        active_player: faction_to_u8(&active_player_faction),
        ai_controlled: ai_controlled_factions,
        team_config: team_config_data,
        faction_base_state: faction_base_data,
        terrain_ops: saved_terrain_ops,
        wall_grid: saved_wall_grid,
        floor_grid: saved_floor_grid,
        entities: saved_entities,
        ai_brains: saved_ai,
        fog_of_war: saved_fog,
        control_groups: saved_control_groups,
    };

    // Serialize to MessagePack
    let blob = match rmp_serde::to_vec(&save_data) {
        Ok(b) => b,
        Err(e) => {
            error!("Failed to serialize save data: {e}");
            return;
        }
    };

    let num_players = config
        .slots
        .iter()
        .filter(|s| !matches!(s, SlotOccupant::Closed))
        .count() as i32;

    // Write to database
    let db = world.resource::<GameDatabase>();
    let profile = world.resource::<ActiveProfile>();
    if let Some(save_id) = db.save_game(
        &profile.id,
        event_label.as_deref(),
        elapsed,
        &format!("{:?}", config.map_size),
        config.map_seed as i64,
        num_players,
        &blob,
    ) {
        info!("Game saved (id={save_id}, {} bytes)", blob.len());
        world.insert_resource(SaveFeedback {
            timer: Timer::from_seconds(2.0, TimerMode::Once),
            message: "Game Saved!".to_string(),
        });
    } else {
        error!("Failed to write save to database");
    }
}

fn save_ai_brain(brain: &AiFactionBrain, emap: &HashMap<Entity, u32>) -> SavedAiBrain {
    SavedAiBrain {
        strategy_timer: brain.strategy_timer,
        economy_timer: brain.economy_timer,
        military_timer: brain.military_timer,
        tactical_timer: brain.tactical_timer,
        scout_timer: brain.scout_timer,
        top_state: ai_top_state_to_u8(&brain.top_state),
        state_entered_at: brain.state_entered_at,
        posture: match brain.posture {
            TacticalPosture::Normal => 0,
            TacticalPosture::UnderAttack => 1,
            TacticalPosture::Retreating => 2,
        },
        posture_cooldown: brain.posture_cooldown,
        game_time: brain.game_time,
        pending_transition: brain.pending_transition.as_ref().map(ai_top_state_to_u8),
        pending_transition_ticks: brain.pending_transition_ticks,
        personality: brain.personality as u8,
        relation: brain.relation as u8,
        difficulty: brain.difficulty as u8,
        ally_attack_target: brain.ally_attack_target.map(vec3_to_arr),
        last_cooperation_check: brain.last_cooperation_check,
        raid_cooldown: brain.raid_cooldown,
        squads: brain
            .squads
            .iter()
            .map(|s| SavedSquad {
                role: squad_role_to_u8(&s.role),
                member_ids: s
                    .members
                    .iter()
                    .filter_map(|e| emap.get(e).copied())
                    .collect(),
            })
            .collect(),
        assigned_units: brain
            .assigned_units
            .iter()
            .filter_map(|(e, role)| emap.get(e).map(|id| (*id, squad_role_to_u8(role))))
            .collect(),
        desired_workers: brain.desired_workers,
        build_queue: brain
            .build_queue
            .iter()
            .map(|br| SavedBuildRequest {
                kind: entity_kind_to_u16(br.kind),
                priority: br.priority,
                near_position: br.near_position.map(vec3_to_arr),
            })
            .collect(),
        pending_builds: brain.pending_builds,
        resource_goal: brain
            .resource_goal
            .as_ref()
            .map(|rg| [rg.wood, rg.copper, rg.iron, rg.gold, rg.oil]),
        income_rates: brain.income_rates.to_vec(),
        last_resource_snapshot: brain.last_resource_snapshot.to_vec(),
        attack_ready: brain.attack_ready,
        last_attack_time: brain.last_attack_time,
        attack_started_at: brain.attack_started_at,
        enemy_composition: brain
            .enemy_composition
            .iter()
            .map(|(k, v)| (entity_kind_to_u16(*k), *v))
            .collect(),
        enemy_strength: brain.enemy_strength,
        relative_strength: brain.relative_strength,
        defense_interrupt: brain.defense_interrupt,
        known_threats: brain
            .known_threats
            .iter()
            .map(|t| SavedThreatEntry {
                position: vec3_to_arr(t.position),
                estimated_strength: t.estimated_strength,
                last_seen: t.last_seen,
                entity_count: t.entity_count,
            })
            .collect(),
        next_scout_waypoint: brain.next_scout_waypoint,
        scout_route: brain.scout_route.iter().map(|v| vec3_to_arr(*v)).collect(),
        wall_plan: brain.wall_plan.as_ref().map(|wp| SavedWallPlan {
            runs: wp
                .runs
                .iter()
                .map(|(a, b)| (vec3_to_arr(*a), vec3_to_arr(*b)))
                .collect(),
            completed: wp.completed.clone(),
        }),
        base_position: brain.base_position.map(vec3_to_arr),
    }
}

fn chrono_now() -> String {
    // Simple timestamp without chrono dependency.
    #[cfg(target_arch = "wasm32")]
    let secs = (js_sys::Date::now() / 1000.0) as u64;
    #[cfg(not(target_arch = "wasm32"))]
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Convert to rough datetime
    format!("{}", secs)
}

// ── QUICKSAVE / QUICKLOAD ───────────────────────────────────────────────────

fn handle_quicksave(
    keyboard: Res<ButtonInput<KeyCode>>,
    net_role: Res<NetRole>,
    mut commands: Commands,
    overlay: Res<InGameOverlay>,
) {
    if *net_role != NetRole::Offline {
        return;
    }
    if *overlay != InGameOverlay::None && *overlay != InGameOverlay::PauseMenu {
        return;
    }
    if keyboard.just_pressed(KeyCode::F5) {
        commands.insert_resource(SaveTrigger {
            label: Some("Quicksave".to_string()),
        });
    }
}

/// Restore `GameSetupConfig` from save data so that `resolve_map_seed` and `spawn_ground`
/// regenerate exactly the same terrain. Uses the resolved `save.map_seed` (not the
/// potentially-zero value in `game_config.map_seed`) so random seeds are reproduced.
pub fn restore_config_from_save(config: &mut GameSetupConfig, save: &SaveData) {
    let saved = &save.game_config;
    config.player_name = saved.player_name.clone();
    config.local_player_slot = saved.local_player_slot;
    config.player_teams = saved.player_teams;
    config.day_cycle_secs = saved.day_cycle_secs;
    config.starting_resources_mult = saved.starting_resources_mult;
    // Use the resolved seed from MapSeed resource, not the config seed (which may be 0/random).
    config.map_seed = save.map_seed;

    config.map_size = match saved.map_size.as_str() {
        "Small" => MapSize::Small,
        "Large" => MapSize::Large,
        "ExtraLarge" => MapSize::ExtraLarge,
        _ => MapSize::Medium,
    };
    config.resource_density = match saved.resource_density.as_str() {
        "Sparse" => ResourceDensity::Sparse,
        "Dense" => ResourceDensity::Dense,
        _ => ResourceDensity::Normal,
    };
    config.team_mode = match saved.team_mode.as_str() {
        "Teams" => TeamMode::Teams,
        _ => TeamMode::FFA,
    };

    for (i, slot_str) in saved.slots.iter().enumerate() {
        if i >= config.slots.len() {
            break;
        }
        config.slots[i] = if slot_str == "Human" {
            SlotOccupant::Human
        } else if slot_str == "Open" {
            SlotOccupant::Open
        } else if slot_str == "Closed" {
            SlotOccupant::Closed
        } else if let Some(diff_str) = slot_str.strip_prefix("Ai:") {
            let diff = match diff_str {
                "Easy" => AiDifficulty::Easy,
                "Hard" => AiDifficulty::Hard,
                _ => AiDifficulty::Medium,
            };
            SlotOccupant::Ai(diff)
        } else {
            SlotOccupant::Closed
        };
    }
}

fn handle_quickload(
    keyboard: Res<ButtonInput<KeyCode>>,
    net_role: Res<NetRole>,
    db: Res<GameDatabase>,
    profile: Res<ActiveProfile>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
    overlay: Res<InGameOverlay>,
    mut config: ResMut<GameSetupConfig>,
) {
    if *net_role != NetRole::Offline {
        return;
    }
    if *overlay != InGameOverlay::None && *overlay != InGameOverlay::PauseMenu {
        return;
    }
    if keyboard.just_pressed(KeyCode::F9) {
        let saves = db.list_saves(&profile.id);
        if let Some(most_recent) = saves.first() {
            if let Some(blob) = db.load_save(most_recent.id) {
                match rmp_serde::from_slice::<SaveData>(&blob) {
                    Ok(save_data) => {
                        info!("Quickloading save id={}", most_recent.id);
                        // Restore config (especially map_seed) so resolve_map_seed
                        // regenerates the exact same terrain on OnEnter(InGame).
                        restore_config_from_save(&mut config, &save_data);
                        commands.insert_resource(PendingLoad { save_data });
                        next_state.set(AppState::MainMenu);
                        // The menu system will detect PendingLoad and immediately
                        // transition to InGame, where load_saved_game runs.
                    }
                    Err(e) => {
                        error!("Failed to deserialize save: {e}");
                    }
                }
            }
        }
    }
}

// ── LOAD SYSTEM ─────────────────────────────────────────────────────────────

/// Runs on `OnEnter(AppState::InGame)` when `PendingLoad` exists.
/// Reconstructs the full game world from saved data.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn load_saved_game(
    mut commands: Commands,
    pending: Option<Res<PendingLoad>>,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    building_models: Option<Res<BuildingModelAssets>>,
    unit_models: Option<Res<UnitModelAssets>>,
    height_map: Res<HeightMap>,
    _biome_map: Res<BiomeMap>,
    time: Res<Time>,
) {
    let Some(pending) = pending else { return };
    let save = &pending.save_data;

    info!(
        "Loading saved game (version={}, {} entities)...",
        save.version,
        save.entities.len()
    );

    if save.version > SAVE_VERSION {
        error!(
            "Save version {} is newer than supported version {}. Cannot load.",
            save.version, SAVE_VERSION
        );
        commands.remove_resource::<PendingLoad>();
        return;
    }

    // 1. Restore resources
    let mut all_res = AllPlayerResources::default();
    for (fi, amounts) in &save.resources {
        let faction = u8_to_faction(*fi);
        let mut pr = PlayerResources::empty();
        for (i, &val) in amounts.iter().enumerate() {
            if i < pr.amounts.len() {
                pr.amounts[i] = val;
            }
        }
        all_res.resources.insert(faction, pr);
    }
    commands.insert_resource(all_res);

    // 2. Restore config
    commands.insert_resource(MapSeed(save.map_seed));
    commands.insert_resource(ActivePlayer(u8_to_faction(save.active_player)));

    // Restore AI controlled factions
    let mut ai_factions = AiControlledFactions::default();
    for &fi in &save.ai_controlled {
        ai_factions.factions.insert(u8_to_faction(fi));
    }
    commands.insert_resource(ai_factions);

    // Restore team config
    let mut team_cfg = TeamConfig::default();
    for (&fi, &team) in &save.team_config {
        team_cfg.teams.insert(u8_to_faction(fi), team);
    }
    commands.insert_resource(team_cfg);

    // Restore faction base state
    let mut fbs = FactionBaseState::default();
    for (&fi, &founded) in &save.faction_base_state {
        fbs.founded.insert(u8_to_faction(fi), founded);
    }
    commands.insert_resource(fbs);

    // 3. Restore day cycle
    commands.insert_resource(DayCycle {
        time: save.day_cycle.time,
        cycle_duration: save.day_cycle.cycle_duration,
        paused: save.day_cycle.paused,
        phase: match save.day_cycle.phase {
            0 => DayPhase::Night,
            1 => DayPhase::Dawn,
            3 => DayPhase::Dusk,
            _ => DayPhase::Day,
        },
    });

    // 4. Restore victory state
    let mut vs = VictoryState::default();
    for (&fi, status) in &save.victory.faction_status {
        let faction = u8_to_faction(fi);
        let fs = match status.variant {
            1 => VictFactionStatus::GracePeriod {
                remaining: status.grace_remaining.unwrap_or(30.0),
            },
            2 => VictFactionStatus::Eliminated,
            _ => VictFactionStatus::Alive,
        };
        vs.faction_status.insert(faction, fs);
    }
    vs.game_over = save.victory.game_over;
    vs.winner = save.victory.winner.map(u8_to_faction);
    vs.winner_team = save.victory.winner_team;
    commands.insert_resource(vs);

    // 5. Restore terrain operations
    {
        use game_state::message::TerrainShapeOp;
        let mut terrain_sync = TerrainShapeSyncState::default();
        for op_data in &save.terrain_ops {
            let op = TerrainShapeOp {
                center: op_data.center,
                footprint: op_data.footprint,
                target_height: op_data.target_height,
            };
            terrain_sync.applied_history.insert(op.clone());
            terrain_sync.applied_history_ordered.push(op.clone());
            // Also add to pending so the terrain system replays them
            terrain_sync.pending_network.push(op);
        }
        commands.insert_resource(terrain_sync);
    }

    // 6. Spawn all entities using blueprint system, then override components
    let bm = building_models.as_deref();
    let um = unit_models.as_deref();
    let mut id_to_entity: HashMap<u32, Entity> = HashMap::new();
    // We also collect entity metadata for the fixup pass
    let mut fixup_data: Vec<(u32, SavedEntity)> = Vec::new();

    for saved in &save.entities {
        let pos = arr_to_vec3(saved.pos);
        let rot = Quat::from_rotation_y(saved.rot_y);

        let entity = match &saved.entity_type {
            SavedEntityType::Unit(unit_data) => {
                let e = spawn_and_setup_base(
                    &mut commands,
                    &cache,
                    &registry,
                    bm,
                    um,
                    &height_map,
                    saved,
                );
                commands.entity(e).insert(UnitStance::from_u8(unit_data.stance));
                commands.entity(e).insert(UnitSpeed(unit_data.speed));
                if let Some(ref c) = unit_data.carrying {
                    commands.entity(e).insert(Carrying {
                        amount: c.amount,
                        weight: c.weight,
                        resource_type: c
                            .resource_type
                            .map(|i| resource_type_from_index(i as usize)),
                    });
                    commands
                        .insert_resource(crate::simulation::resources::CarriedTotalsDirty(true));
                }
                if let Some([cur, lvl]) = unit_data.experience {
                    commands.entity(e).insert(Experience {
                        current: cur,
                        level: match lvl {
                            1 => VeterancyLevel::Veteran,
                            2 => VeterancyLevel::Elite,
                            _ => VeterancyLevel::Recruit,
                        },
                    });
                }
                if let Some(mt) = unit_data.move_target {
                    commands.entity(e).insert(MoveTarget(arr_to_vec3(mt)));
                }
                restore_combat_components(
                    &mut commands,
                    e,
                    unit_data.attack_damage,
                    unit_data.attack_range,
                    unit_data.attack_cooldown,
                    unit_data.aggro_range,
                );
                if let Some(gs) = unit_data.gather_speed {
                    commands.entity(e).insert(GatherSpeed(gs));
                }
                if let Some(cc) = unit_data.carry_capacity {
                    commands.entity(e).insert(CarryCapacity(cc));
                }
                commands
                    .entity(e)
                    .insert(GatherAccumulator(unit_data.gather_accumulator));
                if !unit_data.abilities.is_empty() {
                    commands.entity(e).insert(UnitAbilities {
                        abilities: unit_data
                            .abilities
                            .iter()
                            .map(|a| u8_to_ability_id(*a))
                            .collect(),
                        cooldowns: unit_data
                            .ability_cooldowns
                            .iter()
                            .map(|(a, cd)| (u8_to_ability_id(*a), *cd))
                            .collect(),
                    });
                }
                if !unit_data.display_name.is_empty() {
                    commands
                        .entity(e)
                        .insert(UnitDisplayName(unit_data.display_name.clone()));
                }
                // Restore inventory
                if !unit_data.inventory_items.is_empty() {
                    commands.entity(e).insert(UnitInventory {
                        capacity: crate::simulation::items::inferred_inventory_capacity(
                            u16_to_entity_kind(saved.kind),
                        ),
                        items: unit_data
                            .inventory_items
                            .iter()
                            .map(|i| u8_to_item_kind(*i))
                            .collect(),
                    });
                }
                if !unit_data.item_states.is_empty() {
                    commands.entity(e).insert(ItemRuntimeState {
                        items: unit_data
                            .item_states
                            .iter()
                            .map(|s| crate::simulation::items::ItemStateEntry {
                                item: u8_to_item_kind(s.item),
                                enabled: s.enabled,
                                disabled_reason: None,
                                cooldown_remaining: s.cooldown_remaining,
                                active_toggled: s.active_toggled,
                            })
                            .collect(),
                    });
                }
                // Restore status effects
                if !unit_data.status_effects.is_empty() {
                    commands
                        .entity(e)
                        .insert(saved_to_status_effects(&unit_data.status_effects));
                }
                // Restore veterancy applied marker
                if let Some(v) = unit_data.veterancy_applied {
                    commands.entity(e).insert(VeterancyApplied(match v {
                        1 => VeterancyLevel::Veteran,
                        2 => VeterancyLevel::Elite,
                        _ => VeterancyLevel::Recruit,
                    }));
                }
                e
            }
            SavedEntityType::Building(bld_data) => {
                let e = spawn_and_setup_base(
                    &mut commands,
                    &cache,
                    &registry,
                    bm,
                    um,
                    &height_map,
                    saved,
                );
                let state = if bld_data.state == 1 {
                    BuildingState::Complete
                } else {
                    BuildingState::UnderConstruction
                };
                commands.entity(e).insert(state);
                commands.entity(e).insert(BuildingLevel(bld_data.level));
                commands
                    .entity(e)
                    .insert(BuildingFootprint(bld_data.footprint));
                commands.entity(e).insert(BuildingHeight(bld_data.height));

                if let Some(ref cp) = bld_data.construction_progress {
                    commands.entity(e).insert(ConstructionProgress {
                        timer: restore_timer(cp),
                    });
                } else if state == BuildingState::Complete {
                    // Remove construction progress if building is complete
                    commands.entity(e).remove::<ConstructionProgress>();
                }

                commands
                    .entity(e)
                    .insert(ConstructionWorkers(bld_data.construction_workers));

                if let Some((ref timer, target)) = bld_data.upgrade_progress {
                    commands.entity(e).insert(UpgradeProgress {
                        timer: restore_timer(timer),
                        target_level: target,
                    });
                }

                if let Some(rp) = bld_data.rally_point {
                    commands.entity(e).insert(RallyPoint(arr_to_vec3(rp)));
                }

                // Training queue
                let tq = &bld_data.training_queue;
                commands.entity(e).insert(TrainingQueue {
                    queue: tq.queue.iter().map(|k| u16_to_entity_kind(*k)).collect(),
                    timer: tq.timer.as_ref().map(restore_timer),
                    total_trained: tq.total_trained,
                });

                // Resource processor
                if let Some(ref rp) = bld_data.resource_processor {
                    commands.entity(e).insert(ResourceProcessor {
                        resource_types: rp
                            .resource_types
                            .iter()
                            .map(|i| resource_type_from_index(*i as usize))
                            .collect(),
                        harvest_radius: rp.harvest_radius,
                        harvest_rate: rp.harvest_rate,
                        max_workers: rp.max_workers,
                        buffer: rp.buffer,
                        worker_rate_bonus: rp.worker_rate_bonus,
                        harvest_timer: restore_timer(&rp.harvest_timer),
                        harvest_accumulator: rp.harvest_accumulator,
                    });
                }

                // Storage inventory
                if let Some(ref si) = bld_data.storage_inventory {
                    let mut amounts = [0u32; ResourceType::COUNT];
                    let mut caps = [0u32; ResourceType::COUNT];
                    for (i, &v) in si.amounts.iter().enumerate() {
                        if i < amounts.len() {
                            amounts[i] = v;
                        }
                    }
                    for (i, &v) in si.caps.iter().enumerate() {
                        if i < caps.len() {
                            caps[i] = v;
                        }
                    }
                    commands.entity(e).insert(StorageInventory {
                        amounts,
                        caps,
                        last_total: amounts.iter().sum(),
                    });
                }

                // Production state — deferred restore via PendingProductionRestore
                if let Some(ref ps) = bld_data.production_state {
                    commands.entity(e).insert(PendingProductionRestore {
                        active_recipe: ps.active_recipe,
                        progress_timer: ps.progress_timer.clone(),
                        input_buffer: ps.input_buffer.clone(),
                        output_buffer: ps.output_buffer.clone(),
                        auto_repeat: ps.auto_repeat,
                    });
                }

                // Attack components for towers
                if let Some(dmg) = bld_data.attack_damage {
                    restore_combat_components(
                        &mut commands,
                        e,
                        dmg,
                        bld_data.attack_range.unwrap_or(0.0),
                        bld_data.attack_cooldown,
                        bld_data.aggro_range,
                    );
                }

                // Tower auto-attack toggle
                if let Some(auto_atk) = bld_data.tower_auto_attack {
                    commands.entity(e).insert(TowerAutoAttackEnabled(auto_atk));
                }
                // Building paused state
                if bld_data.paused {
                    commands.entity(e).insert(BuildingPaused);
                }

                // For complete buildings, set scale to full (blueprint starts at construction scale)
                if state == BuildingState::Complete {
                    commands.entity(e).insert(Transform {
                        translation: pos,
                        rotation: rot,
                        scale: Vec3::ONE,
                    });
                }

                e
            }
            SavedEntityType::ResourceNode(node_data) => {
                let rt = resource_type_from_index(node_data.resource_type as usize);
                let default_scale = if rt == ResourceType::Wood { 0.4 } else { 1.0 };
                let scale = saved.scale.unwrap_or(default_scale);
                let e = commands
                    .spawn((
                        GameWorld,
                        ResourceNode {
                            resource_type: rt,
                            amount_remaining: node_data.amount_remaining,
                        },
                        Transform {
                            translation: pos,
                            rotation: rot,
                            scale: Vec3::splat(scale),
                            ..default()
                        },
                    ))
                    .id();
                e
            }
            SavedEntityType::Mob(mob_data) => {
                let e = spawn_and_setup_base(
                    &mut commands,
                    &cache,
                    &registry,
                    bm,
                    um,
                    &height_map,
                    saved,
                );
                if let Some(stance) = mob_data.stance {
                    commands.entity(e).insert(UnitStance::from_u8(stance));
                }
                restore_combat_components(
                    &mut commands,
                    e,
                    mob_data.attack_damage,
                    mob_data.attack_range,
                    mob_data.attack_cooldown,
                    mob_data.aggro_range,
                );
                if !mob_data.status_effects.is_empty() {
                    commands
                        .entity(e)
                        .insert(saved_to_status_effects(&mob_data.status_effects));
                }
                e
            }
            SavedEntityType::Projectile(_proj_data) => {
                // Skip projectiles on load — they're ephemeral and will just
                // cause entity reference issues. They'll naturally be re-created
                // by the combat system.
                continue;
            }
            SavedEntityType::Dying(_dying_data) => {
                // Skip dying entities — they're about to be removed anyway
                continue;
            }
            SavedEntityType::Tree(tree_data) => {
                // Trees are decorations — spawn minimal entities
                // The decoration system normally handles these
                let e = commands
                    .spawn((
                        GameWorld,
                        Transform {
                            translation: pos,
                            rotation: rot,
                            ..default()
                        },
                    ))
                    .id();
                match tree_data {
                    SavedTreeData::Sapling {
                        timer_elapsed,
                        timer_duration,
                        target_scale,
                    } => {
                        let mut timer = Timer::from_seconds(*timer_duration, TimerMode::Once);
                        timer.tick(std::time::Duration::from_secs_f32(*timer_elapsed));
                        commands.entity(e).insert(Sapling {
                            timer,
                            target_scale: *target_scale,
                        });
                    }
                    SavedTreeData::Growing {
                        stage,
                        timer_elapsed,
                        timer_duration,
                        target_scale,
                    } => {
                        let mut timer = Timer::from_seconds(*timer_duration, TimerMode::Once);
                        timer.tick(std::time::Duration::from_secs_f32(*timer_elapsed));
                        commands.entity(e).insert(GrowingTree {
                            stage: *stage,
                            timer,
                            target_scale: *target_scale,
                        });
                    }
                    SavedTreeData::Mature => {
                        commands.entity(e).insert(MatureTree);
                    }
                }
                e
            }
            SavedEntityType::ItemPickup(pickup_data) => {
                let remaining = pickup_data.expires_at - save.elapsed_secs as f32;
                if remaining <= 0.0 {
                    continue; // Already expired
                }
                let item = u8_to_item_kind(pickup_data.item_kind);
                let owner = pickup_data.owner_faction.map(u8_to_faction);
                let e = commands
                    .spawn((
                        GameWorld,
                        ItemPickup {
                            item,
                            owner,
                            expires_at: time.elapsed_secs() + remaining,
                        },
                        Transform::from_translation(pos),
                        Visibility::Visible,
                    ))
                    .id();
                e
            }
            SavedEntityType::GrowingResource(gr_data) => {
                let rt = resource_type_from_index(gr_data.resource_type as usize);
                let mut timer = Timer::from_seconds(gr_data.timer_duration, TimerMode::Once);
                timer.tick(std::time::Duration::from_secs_f32(gr_data.timer_elapsed));
                let e = commands
                    .spawn((
                        GameWorld,
                        GrowingResource {
                            timer,
                            target_scale: gr_data.target_scale,
                            resource_type: rt,
                            amount: gr_data.amount,
                        },
                        Transform {
                            translation: pos,
                            rotation: rot,
                            scale: Vec3::splat(
                                gr_data.target_scale
                                    * (gr_data.timer_elapsed / gr_data.timer_duration)
                                        .clamp(0.0, 1.0),
                            ),
                        },
                    ))
                    .id();
                e
            }
        };

        id_to_entity.insert(saved.save_id, entity);
        fixup_data.push((saved.save_id, saved.clone()));
    }

    // 7. Fixup pass: resolve entity cross-references
    for (save_id, saved) in &fixup_data {
        let Some(&entity) = id_to_entity.get(save_id) else {
            continue;
        };
        match &saved.entity_type {
            SavedEntityType::Unit(unit_data) => {
                // Resolve UnitState entity references
                let state = saved_to_unit_state(&unit_data.state, &id_to_entity);
                commands.entity(entity).insert(state);

                // Resolve attack target
                if let Some(target_id) = unit_data.attack_target_id {
                    if let Some(&target) = id_to_entity.get(&target_id) {
                        commands.entity(entity).insert(AttackTarget(target));
                    }
                }

                // Resolve building assignment
                if let Some(bld_id) = unit_data.building_assignment_id {
                    if let Some(&bld) = id_to_entity.get(&bld_id) {
                        commands.entity(entity).insert(BuildingAssignment(bld));
                    }
                }

                // Resolve combat intent
                match &unit_data.combat_intent {
                    SavedCombatIntent::Attack(target_id, src) => {
                        if let Some(&target) = id_to_entity.get(target_id) {
                            let source = if *src == 0 {
                                IntentSource::Manual
                            } else {
                                IntentSource::Auto
                            };
                            commands
                                .entity(entity)
                                .insert(CombatIntent::Attack(target, source));
                        }
                    }
                    SavedCombatIntent::Move(v) => {
                        commands
                            .entity(entity)
                            .insert(CombatIntent::Move(arr_to_vec3(*v)));
                    }
                    SavedCombatIntent::AttackMove(v, src) => {
                        let source = if *src == 0 {
                            IntentSource::Manual
                        } else {
                            IntentSource::Auto
                        };
                        commands
                            .entity(entity)
                            .insert(CombatIntent::AttackMove(arr_to_vec3(*v), source));
                    }
                    SavedCombatIntent::Hold => {
                        commands.entity(entity).insert(CombatIntent::Hold);
                    }
                    SavedCombatIntent::None => {
                        commands.entity(entity).insert(CombatIntent::None);
                    }
                }
            }
            SavedEntityType::Building(bld_data) => {
                // Resolve assigned workers
                if !bld_data.assigned_worker_ids.is_empty() {
                    let workers: Vec<Entity> = bld_data
                        .assigned_worker_ids
                        .iter()
                        .filter_map(|id| id_to_entity.get(id).copied())
                        .collect();
                    commands.entity(entity).insert(AssignedWorkers { workers });
                }

                // Resolve attack target
                if let Some(target_id) = bld_data.attack_target_id {
                    if let Some(&target) = id_to_entity.get(&target_id) {
                        commands.entity(entity).insert(AttackTarget(target));
                    }
                }
            }
            SavedEntityType::Mob(mob_data) => {
                if let Some(ref saved_state) = mob_data.state {
                    let state = saved_to_unit_state(saved_state, &id_to_entity);
                    commands.entity(entity).insert(state);
                }

                if let Some(target_id) = mob_data.attack_target_id {
                    if let Some(&target) = id_to_entity.get(&target_id) {
                        commands.entity(entity).insert(AttackTarget(target));
                    }
                }
            }
            _ => {}
        }
    }

    // 8. Restore wall grid
    let mut wall_grid = WallGrid::default();
    for cell in &save.wall_grid {
        if let Some(&entity) = id_to_entity.get(&cell.entity_save_id) {
            wall_grid.cells.insert(
                (cell.gx, cell.gz),
                WallGridCell {
                    entity,
                    _faction: u8_to_faction(cell.faction),
                    piece_kind: match cell.piece_kind {
                        0 => WallPieceKind::Post,
                        1 => WallPieceKind::Straight,
                        2 => WallPieceKind::Corner,
                        _ => WallPieceKind::Gate,
                    },
                    is_gate: cell.is_gate,
                    rotation_y: cell.rotation_y,
                },
            );
        }
    }
    commands.insert_resource(wall_grid);

    // 9. Restore floor grid — spawn floor tile entities from grid cells
    let mut floor_grid_res = FloorGrid::default();
    let footprint = crate::simulation::buildings::footprint_for_kind(EntityKind::Floor);
    for cell in &save.floor_grid {
        let faction = u8_to_faction(cell.faction);
        let world_pos = WallGrid::grid_to_world(cell.gx, cell.gz);
        let ground_y =
            height_map.foundation_target_height_shaped(world_pos.x, world_pos.z, footprint);
        let entity = commands
            .spawn((
                GameWorld,
                EntityKind::Floor,
                faction,
                Building,
                FloorTile,
                FloorGridCoord(cell.gx, cell.gz),
                BuildingFootprint(footprint),
                VegetationCleared,
                Transform::from_translation(Vec3::new(world_pos.x, ground_y, world_pos.z)),
            ))
            .id();
        floor_grid_res.cells.insert(
            (cell.gx, cell.gz),
            FloorGridCell {
                entity,
                _faction: faction,
                piece_kind: FloorPieceKind::Isolated,
                rotation_y: 0.0,
            },
        );
        floor_grid_res.mark_dirty(cell.gx, cell.gz);
    }
    commands.insert_resource(floor_grid_res);

    // 10. Restore AI state
    let mut ai_state = AiState::default();
    for (fi, saved_brain) in &save.ai_brains {
        let faction = u8_to_faction(*fi);
        let brain = restore_ai_brain(saved_brain, &id_to_entity);
        ai_state.factions.insert(faction, brain);
    }
    commands.insert_resource(ai_state);

    // 11. Queue fog of war restoration (deferred — FogOfWarMap may not exist yet)
    if let Some(ref fog_data) = save.fog_of_war {
        commands.insert_resource(PendingFogRestore {
            data: fog_data.clone(),
        });
    }

    // 12. Insert MatchStartTime so game clock continues from saved elapsed time
    commands.insert_resource(MatchStartTime(time.elapsed_secs_f64() - save.elapsed_secs));

    // 13. Restore control groups
    if !save.control_groups.is_empty() {
        let mut groups: [Vec<Entity>; 9] = Default::default();
        for (i, group_ids) in save.control_groups.iter().enumerate() {
            if i >= 9 {
                break;
            }
            groups[i] = group_ids
                .iter()
                .filter_map(|id| id_to_entity.get(id).copied())
                .collect();
        }
        commands.insert_resource(ControlGroups { groups });
    }

    // 14. Queue visual restoration for resource nodes, trees, etc.
    commands.insert_resource(PendingLoadVisuals);

    // 15. Remove PendingLoad to signal completion
    commands.remove_resource::<PendingLoad>();

    info!(
        "Game loaded successfully ({} entities)",
        save.entities.len()
    );
}

fn restore_ai_brain(saved: &SavedAiBrain, id_map: &HashMap<u32, Entity>) -> AiFactionBrain {
    let resolve = |id: &u32| -> Entity { id_map.get(id).copied().unwrap_or(Entity::PLACEHOLDER) };

    AiFactionBrain {
        strategy_timer: saved.strategy_timer,
        economy_timer: saved.economy_timer,
        military_timer: saved.military_timer,
        tactical_timer: saved.tactical_timer,
        scout_timer: saved.scout_timer,
        top_state: u8_to_ai_top_state(saved.top_state),
        state_entered_at: saved.state_entered_at,
        posture: match saved.posture {
            1 => TacticalPosture::UnderAttack,
            2 => TacticalPosture::Retreating,
            _ => TacticalPosture::Normal,
        },
        posture_cooldown: saved.posture_cooldown,
        game_time: saved.game_time,
        pending_transition: saved.pending_transition.map(u8_to_ai_top_state),
        pending_transition_ticks: saved.pending_transition_ticks,
        personality: match saved.personality {
            1 => crate::types::AiPersonality::Aggressive,
            2 => crate::types::AiPersonality::Defensive,
            3 => crate::types::AiPersonality::Economic,
            4 => crate::types::AiPersonality::Supportive,
            _ => crate::types::AiPersonality::Balanced,
        },
        relation: match saved.relation {
            0 => AiRelation::Friendly,
            _ => AiRelation::Enemy,
        },
        difficulty: match saved.difficulty {
            0 => AiDifficulty::Easy,
            2 => AiDifficulty::Hard,
            _ => AiDifficulty::Medium,
        },
        ally_attack_target: saved.ally_attack_target.map(arr_to_vec3),
        last_cooperation_check: saved.last_cooperation_check,
        raid_cooldown: saved.raid_cooldown,
        squads: saved
            .squads
            .iter()
            .map(|s| Squad {
                role: u8_to_squad_role(s.role),
                members: s.member_ids.iter().map(|id| resolve(id)).collect(),
            })
            .collect(),
        assigned_units: saved
            .assigned_units
            .iter()
            .map(|(id, role)| (resolve(id), u8_to_squad_role(*role)))
            .collect(),
        desired_workers: saved.desired_workers,
        build_queue: saved
            .build_queue
            .iter()
            .map(|br| BuildRequest {
                kind: u16_to_entity_kind(br.kind),
                priority: br.priority,
                near_position: br.near_position.map(arr_to_vec3),
            })
            .collect(),
        pending_builds: saved.pending_builds,
        resource_goal: saved.resource_goal.map(|rg| ResourceGoal {
            wood: rg[0],
            copper: rg[1],
            iron: rg[2],
            gold: rg[3],
            oil: rg[4],
        }),
        income_rates: {
            let mut arr = [0.0f32; ResourceType::COUNT];
            for (i, &v) in saved.income_rates.iter().enumerate() {
                if i < arr.len() {
                    arr[i] = v;
                }
            }
            arr
        },
        last_resource_snapshot: {
            let mut arr = [0u32; ResourceType::COUNT];
            for (i, &v) in saved.last_resource_snapshot.iter().enumerate() {
                if i < arr.len() {
                    arr[i] = v;
                }
            }
            arr
        },
        attack_ready: saved.attack_ready,
        last_attack_time: saved.last_attack_time,
        attack_started_at: saved.attack_started_at,
        enemy_composition: saved
            .enemy_composition
            .iter()
            .map(|(k, v)| (u16_to_entity_kind(*k), *v))
            .collect(),
        enemy_strength: saved.enemy_strength,
        relative_strength: saved.relative_strength,
        defense_interrupt: saved.defense_interrupt,
        known_threats: saved
            .known_threats
            .iter()
            .map(|t| ThreatEntry {
                position: arr_to_vec3(t.position),
                estimated_strength: t.estimated_strength,
                last_seen: t.last_seen,
                entity_count: t.entity_count,
            })
            .collect(),
        next_scout_waypoint: saved.next_scout_waypoint,
        scout_route: saved.scout_route.iter().map(|v| arr_to_vec3(*v)).collect(),
        wall_plan: saved.wall_plan.as_ref().map(|wp| WallPlan {
            runs: wp
                .runs
                .iter()
                .map(|(a, b)| (arr_to_vec3(*a), arr_to_vec3(*b)))
                .collect(),
            completed: wp.completed.clone(),
        }),
        base_position: saved.base_position.map(arr_to_vec3),
        prev_health: HashMap::new(),
    }
}

// ── Deferred restoration systems ───────────────────────────────────────────

fn restore_fog_on_load(
    mut commands: Commands,
    mut fog_map: ResMut<FogOfWarMap>,
    mut upload_state: ResMut<FogTextureUploadState>,
    pending: Res<PendingFogRestore>,
) {
    let data = &pending.data;
    if data.grid_size != fog_map.grid_size
        || (data.step - fog_map.step).abs() > 0.01
        || (data.half_map - fog_map.half_map).abs() > 0.01
    {
        warn!(
            "Fog grid mismatch (save: {}x{} step={}, current: {}x{} step={}). Skipping fog restore.",
            data.grid_size, data.grid_size, data.step,
            fog_map.grid_size, fog_map.grid_size, fog_map.step,
        );
    } else if data.explored.len() == fog_map.explored.len() {
        fog_map.explored.copy_from_slice(&data.explored);
        upload_state.explored_dirty = true;
        info!(
            "Restored fog of war explored state ({} cells)",
            data.explored.len()
        );
    } else {
        warn!(
            "Fog explored data length mismatch (save={}, current={}). Skipping fog restore.",
            data.explored.len(),
            fog_map.explored.len(),
        );
    }
    commands.remove_resource::<PendingFogRestore>();
}

fn restore_production_states(
    mut commands: Commands,
    mut query: Query<(Entity, &PendingProductionRestore, &mut ProductionState)>,
) {
    for (entity, pending, mut ps) in query.iter_mut() {
        ps.active_recipe = pending.active_recipe;
        ps.progress_timer = restore_timer(&pending.progress_timer);
        for (i, &v) in pending.input_buffer.iter().enumerate() {
            if i < ps.input_buffer.len() {
                ps.input_buffer[i] = v;
            }
        }
        for (i, &v) in pending.output_buffer.iter().enumerate() {
            if i < ps.output_buffer.len() {
                ps.output_buffer[i] = v;
            }
        }
        ps.auto_repeat = pending.auto_repeat;
        commands.entity(entity).remove::<PendingProductionRestore>();
    }
}

/// Attaches visual components (meshes, scene roots, materials) to resource nodes,
/// trees, and growing resources that were spawned without visuals during game load.
fn restore_load_visuals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    model_assets: Res<ModelAssets>,
    node_mats: Res<ResourceNodeMaterials>,
    resource_nodes: Query<
        (Entity, &ResourceNode, &Transform),
        (Without<Mesh3d>, Without<SceneRoot>),
    >,
    saplings: Query<(Entity, &Sapling), Without<SceneRoot>>,
    growing_trees: Query<(Entity, &GrowingTree), Without<SceneRoot>>,
    growing_resources: Query<
        (Entity, &GrowingResource, &Transform),
        (Without<Mesh3d>, Without<SceneRoot>),
    >,
) {
    let has_tree_models = !model_assets.trees.is_empty();
    let has_rock_models = !model_assets.rocks.is_empty();
    let mut rng = rand::rng();

    // ── Resource nodes ──
    let oil_mesh = meshes.add(Cylinder::new(0.5, 1.2));

    for (entity, node, _tf) in &resource_nodes {
        let rt = node.resource_type;

        if rt == ResourceType::Wood && has_tree_models {
            let idx = rng.random_range(0..model_assets.trees.len());
            let scene_handle = model_assets.trees[idx].clone();
            commands.entity(entity).insert((
                MatureTree,
                FogHideable::Object,
                PickRadius(3.0),
                SceneRoot(scene_handle),
                NotShadowCaster,
                TerrainHeightOffset(0.0),
            ));
        } else if matches!(
            rt,
            ResourceType::Copper | ResourceType::Iron | ResourceType::Gold | ResourceType::Stone
        ) && has_rock_models
        {
            let scene_handle =
                model_assets.rocks[rng.random_range(0..model_assets.rocks.len())].clone();
            commands.entity(entity).insert((
                FogHideable::Object,
                PickRadius(1.8),
                SceneRoot(scene_handle),
                NotShadowCaster,
                TerrainHeightOffset(0.0),
            ));
        } else {
            // Oil or fallback primitive mesh
            let (mesh, mat, half_h) = match rt {
                ResourceType::Oil => (oil_mesh.clone(), node_mats.oil.clone(), 0.6),
                ResourceType::Copper => (
                    meshes.add(Cuboid::new(1.0, 0.8, 1.0)),
                    node_mats.copper.clone(),
                    0.4,
                ),
                ResourceType::Iron => (
                    meshes.add(Cuboid::new(1.0, 0.8, 1.0)),
                    node_mats.iron.clone(),
                    0.4,
                ),
                ResourceType::Stone => (
                    meshes.add(Cuboid::new(0.9, 0.7, 0.9)),
                    node_mats.stone.clone(),
                    0.35,
                ),
                _ => (
                    meshes.add(Cuboid::new(0.6, 2.5, 0.6)),
                    node_mats.wood.clone(),
                    1.25,
                ),
            };
            commands.entity(entity).insert((
                FogHideable::Object,
                PickRadius(half_h * 1.5),
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                NotShadowCaster,
                TerrainHeightOffset(half_h),
            ));
        }
    }

    // ── Saplings (growing trees) ──
    for (entity, _sapling) in &saplings {
        if has_tree_models {
            let idx = rng.random_range(0..model_assets.trees.len());
            let scene_handle = model_assets.trees[idx].clone();
            commands
                .entity(entity)
                .insert((FogHideable::Object, SceneRoot(scene_handle)));
        }
    }

    // ── Growing trees ──
    for (entity, _growing) in &growing_trees {
        if has_tree_models {
            let idx = rng.random_range(0..model_assets.trees.len());
            let scene_handle = model_assets.trees[idx].clone();
            commands
                .entity(entity)
                .insert((FogHideable::Object, SceneRoot(scene_handle)));
        }
    }

    // ── Growing resources (ore/oil emerging near buildings) ──
    for (entity, res, _tf) in &growing_resources {
        let rt = res.resource_type;
        if matches!(
            rt,
            ResourceType::Copper | ResourceType::Iron | ResourceType::Gold | ResourceType::Stone
        ) && has_rock_models
        {
            let scene_handle =
                model_assets.rocks[rng.random_range(0..model_assets.rocks.len())].clone();
            commands.entity(entity).insert((
                FogHideable::Object,
                SceneRoot(scene_handle),
                TerrainHeightOffset(0.0),
            ));
        } else {
            // Oil or fallback
            let mesh = meshes.add(Cylinder::new(0.5, 1.2));
            let mat = node_mats.oil.clone();
            commands.entity(entity).insert((
                FogHideable::Object,
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                TerrainHeightOffset(0.6),
            ));
        }
    }

    // Note: ItemPickup visuals are not restored — they're ephemeral entities
    // that expire quickly and use complex multi-child visual hierarchies.

    commands.remove_resource::<PendingLoadVisuals>();

    let node_count = resource_nodes.iter().count();
    let tree_count = saplings.iter().count() + growing_trees.iter().count();
    let growing_count = growing_resources.iter().count();
    if node_count + tree_count + growing_count > 0 {
        info!(
            "Restored load visuals: {} resource nodes, {} trees, {} growing resources",
            node_count, tree_count, growing_count,
        );
    }
}
