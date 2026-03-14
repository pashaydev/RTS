use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::blueprints::EntityKind;

// ── Map Seed ──

#[derive(Resource, Debug, Clone, Copy)]
pub struct MapSeed(pub u64);

// ── App State ──

#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    MainMenu,
    InGame,
}

// ── Game Setup Config ──

#[derive(Resource, Clone, Debug)]
pub struct GameSetupConfig {
    pub player_name: String,
    pub player_color_index: usize,
    pub num_ai_opponents: u8,
    pub ai_difficulties: [AiDifficulty; 3],
    pub team_mode: TeamMode,
    pub player_teams: [u8; 4],
    pub map_size: MapSize,
    pub resource_density: ResourceDensity,
    pub day_cycle_secs: f32,
    pub starting_resources_mult: f32,
    pub map_seed: u64, // 0 = random
}

impl Default for GameSetupConfig {
    fn default() -> Self {
        Self {
            player_name: "Commander".to_string(),
            player_color_index: 0,
            num_ai_opponents: 3,
            ai_difficulties: [AiDifficulty::Medium; 3],
            team_mode: TeamMode::default(),
            player_teams: [0, 1, 2, 3],
            map_size: MapSize::default(),
            resource_density: ResourceDensity::default(),
            day_cycle_secs: 600.0,
            starting_resources_mult: 1.0,
            map_seed: 0,
        }
    }
}

impl GameSetupConfig {
    pub fn spawn_positions(&self, seed: u64) -> Vec<(Faction, (f32, f32))> {
        let factions = [
            Faction::Player1,
            Faction::Player2,
            Faction::Player3,
            Faction::Player4,
        ];
        let count = (1 + self.num_ai_opponents as usize).min(4);
        let half_map = self.map_size.world_size() / 2.0;
        let radius = 0.6 * half_map;
        let rotation_offset = (seed % 360) as f32 * std::f32::consts::PI / 180.0;

        (0..count)
            .map(|i| {
                let angle = 2.0 * std::f32::consts::PI * i as f32 / count as f32 + rotation_offset;
                let x = angle.cos() * radius;
                let z = angle.sin() * radius;
                (factions[i], (x, z))
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TeamMode {
    #[default]
    FFA,
    Teams,
    Custom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MapSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl MapSize {
    pub fn world_size(&self) -> f32 {
        match self {
            MapSize::Small => 300.0,
            MapSize::Medium => 500.0,
            MapSize::Large => 700.0,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            MapSize::Small => "Small",
            MapSize::Medium => "Medium",
            MapSize::Large => "Large",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResourceDensity {
    Sparse,
    #[default]
    Normal,
    Dense,
}

impl ResourceDensity {
    pub fn multiplier(&self) -> f32 {
        match self {
            ResourceDensity::Sparse => 0.5,
            ResourceDensity::Normal => 1.0,
            ResourceDensity::Dense => 1.5,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ResourceDensity::Sparse => "Sparse",
            ResourceDensity::Normal => "Normal",
            ResourceDensity::Dense => "Dense",
        }
    }
}

// ── Graphics Settings ──

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ShadowQuality {
    Off,
    Low,
    #[default]
    High,
}

impl ShadowQuality {
    pub fn display_name(&self) -> &'static str {
        match self {
            ShadowQuality::Off => "Off",
            ShadowQuality::Low => "Low",
            ShadowQuality::High => "High",
        }
    }
}

#[derive(Resource, Clone, Debug, Serialize, Deserialize)]
pub struct GraphicsSettings {
    pub resolution: (u32, u32),
    pub fullscreen: bool,
    pub shadow_quality: ShadowQuality,
    pub entity_lights: bool,
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
}

fn default_ui_scale() -> f32 {
    1.0
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            resolution: (1280, 720),
            fullscreen: false,
            shadow_quality: ShadowQuality::High,
            entity_lights: true,
            ui_scale: 1.0,
        }
    }
}

impl GraphicsSettings {
    pub fn load_or_default() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::fs::read_to_string("config/graphics_settings.json")
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self::default()
        }
    }

    pub fn save(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = std::fs::create_dir_all("config");
            let _ = std::fs::write(
                "config/graphics_settings.json",
                serde_json::to_string_pretty(self).unwrap(),
            );
        }
    }
}

// ── Resource types ──

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    // Raw (0-4)
    Wood,
    Copper,
    Iron,
    Gold,
    Oil,
    // Processed (5-9)
    Planks,
    Charcoal,
    Bronze,
    Steel,
    Gunpowder,
}

impl ResourceType {
    pub const RAW: [ResourceType; 5] = [
        ResourceType::Wood,
        ResourceType::Copper,
        ResourceType::Iron,
        ResourceType::Gold,
        ResourceType::Oil,
    ];

    pub const PROCESSED: [ResourceType; 5] = [
        ResourceType::Planks,
        ResourceType::Charcoal,
        ResourceType::Bronze,
        ResourceType::Steel,
        ResourceType::Gunpowder,
    ];

    pub const ALL: [ResourceType; 10] = [
        ResourceType::Wood,
        ResourceType::Copper,
        ResourceType::Iron,
        ResourceType::Gold,
        ResourceType::Oil,
        ResourceType::Planks,
        ResourceType::Charcoal,
        ResourceType::Bronze,
        ResourceType::Steel,
        ResourceType::Gunpowder,
    ];

    pub const COUNT: usize = 10;

    pub fn index(self) -> usize {
        match self {
            Self::Wood => 0,
            Self::Copper => 1,
            Self::Iron => 2,
            Self::Gold => 3,
            Self::Oil => 4,
            Self::Planks => 5,
            Self::Charcoal => 6,
            Self::Bronze => 7,
            Self::Steel => 8,
            Self::Gunpowder => 9,
        }
    }

    pub fn is_raw(self) -> bool {
        self.index() < 5
    }

    pub fn is_processed(self) -> bool {
        self.index() >= 5
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Wood => "Wood",
            Self::Copper => "Copper",
            Self::Iron => "Iron",
            Self::Gold => "Gold",
            Self::Oil => "Oil",
            Self::Planks => "Planks",
            Self::Charcoal => "Charcoal",
            Self::Bronze => "Bronze",
            Self::Steel => "Steel",
            Self::Gunpowder => "Gunpowder",
        }
    }

    /// Short abbreviation for UI display.
    pub fn abbreviation(self) -> &'static str {
        match self {
            Self::Wood => "W",
            Self::Copper => "C",
            Self::Iron => "I",
            Self::Gold => "G",
            Self::Oil => "O",
            Self::Planks => "Pk",
            Self::Charcoal => "Ch",
            Self::Bronze => "Bz",
            Self::Steel => "St",
            Self::Gunpowder => "Gp",
        }
    }

    pub fn weight(self) -> f32 {
        match self {
            Self::Wood => 1.0,
            Self::Copper => 1.5,
            Self::Iron => 2.0,
            Self::Gold => 2.5,
            Self::Oil => 1.2,
            Self::Planks => 1.2,
            Self::Charcoal => 0.8,
            Self::Bronze => 2.5,
            Self::Steel => 3.0,
            Self::Gunpowder => 1.0,
        }
    }

    /// Relative per-second gather throughput for workers.
    /// Processed resources return 0.0 (not gatherable from nodes).
    pub fn gather_rate_multiplier(self) -> f32 {
        match self {
            Self::Wood => 1.0,
            Self::Copper => 0.9,
            Self::Iron => 0.65,
            Self::Gold => 0.45,
            Self::Oil => 0.85,
            // Processed resources are not gatherable from nodes
            Self::Planks | Self::Charcoal | Self::Bronze | Self::Steel | Self::Gunpowder => 0.0,
        }
    }

    pub fn carry_color(self) -> Color {
        match self {
            Self::Wood => Color::srgb(0.55, 0.35, 0.15),
            Self::Copper => Color::srgb(0.72, 0.45, 0.2),
            Self::Iron => Color::srgb(0.55, 0.55, 0.58),
            Self::Gold => Color::srgb(0.95, 0.8, 0.2),
            Self::Oil => Color::srgb(0.08, 0.08, 0.1),
            Self::Planks => Color::srgb(0.76, 0.60, 0.35),
            Self::Charcoal => Color::srgb(0.25, 0.25, 0.25),
            Self::Bronze => Color::srgb(0.80, 0.50, 0.20),
            Self::Steel => Color::srgb(0.55, 0.60, 0.70),
            Self::Gunpowder => Color::srgb(0.60, 0.20, 0.20),
        }
    }
}

// ── Unit markers ──

#[derive(Component)]
pub struct Unit;

#[derive(Component)]
pub struct Selected;

#[derive(Component)]
pub struct Hovered;

/// Bounding sphere radius for mouse picking (ray-sphere intersection).
#[derive(Component)]
pub struct PickRadius(pub f32);

#[derive(Resource, Default)]
pub struct UiClickedThisFrame(pub u8);

/// Set to true when a mouse press starts on UI; cleared on mouse release.
#[derive(Resource, Default)]
pub struct UiPressActive(pub bool);

/// True when the cursor is hovering any UI node (blocks camera input).
#[derive(Resource, Default)]
pub struct CursorOverUi(pub bool);

/// Active command mode for hotkey-based unit commands (A-click, P-click).
#[derive(Resource, Default, PartialEq, Eq, Debug, Clone, Copy)]
pub enum CommandMode {
    #[default]
    Normal,
    AttackMove,
    Patrol,
}

#[derive(Component)]
pub struct HoverRing;

#[derive(Component)]
pub struct HoverTooltip;

#[derive(Resource)]
pub struct HoverRingAssets {
    pub mesh: Handle<Mesh>,
}

#[derive(Component)]
pub struct MoveTarget(pub Vec3);

#[derive(Component)]
pub struct UnitSpeed(pub f32);

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

// ── Unit Stance & Role ──

/// Whether a unit's current state was set by the player or the AI.
#[derive(Component, Clone, Copy, PartialEq, Debug, Default)]
pub enum TaskSource {
    /// Player-ordered — AI must not override.
    Manual,
    /// AI-decided — can be replaced freely.
    #[default]
    Auto,
}

/// Combat stance — governs automatic threat response behavior.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum UnitStance {
    /// Never auto-engage. Only attacks when manually ordered.
    Passive,
    /// Auto-engage enemies within scan range, but don't chase far (leash).
    #[default]
    Defensive,
    /// Actively seek enemies at extended range. Chase aggressively.
    Aggressive,
}

impl UnitStance {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Passive => "Passive",
            Self::Defensive => "Defensive",
            Self::Aggressive => "Aggressive",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Self::Passive => Self::Defensive,
            Self::Defensive => Self::Aggressive,
            Self::Aggressive => Self::Passive,
        }
    }

    /// Scan range multiplier for auto-acquiring targets.
    pub fn scan_multiplier(self) -> f32 {
        match self {
            Self::Passive => 0.0,
            Self::Defensive => 1.5,
            Self::Aggressive => 2.5,
        }
    }

    /// Max chase distance before leashing back (0 = no leash).
    pub fn leash_distance(self) -> f32 {
        match self {
            Self::Passive => 0.0,
            Self::Defensive => 12.0,
            Self::Aggressive => 50.0,
        }
    }
}

/// High-level role hint for the AI decision layer.
#[derive(Component, Clone, Copy, PartialEq, Debug, Default)]
pub enum AutoRole {
    /// No automatic behavior — wait for orders.
    #[default]
    None,
    /// Gather resources and build.
    Economy,
    /// Explore the map and pick up loot.
    Explore,
    /// Defend a specific area.
    DefendArea(Vec3),
}

/// Remembers where a unit was when it started chasing a target (for leash return).
#[derive(Component, Clone, Copy, Debug)]
pub struct LeashOrigin(pub Vec3);

/// Timer that controls decision priority ticks (every 0.2s).
#[derive(Resource)]
pub struct DecisionTimer {
    pub timer: Timer,
}

impl Default for DecisionTimer {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(0.2, TimerMode::Repeating),
        }
    }
}

// ── Gathering ──

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

/// A task waiting in a unit's queue (shift+click).
#[derive(Clone, Debug)]
pub enum QueuedTask {
    Move(Vec3),
    AttackMove(Vec3),
    Attack(Entity),
    Gather(Entity),
    Build(Entity),
    Patrol(Vec3),
    AssignToProcessor(Entity),
    HoldPosition,
}

#[derive(Clone, Debug)]
pub struct TaskEntry {
    pub id: u64,
    pub task: QueuedTask,
}

/// Attached to a worker who is walking to a location to plot a new building.
/// When the worker arrives, the building is spawned and the worker transitions to Building state.
#[derive(Component)]
pub struct PendingBuildOrder {
    pub kind: crate::blueprints::EntityKind,
    pub position: Vec3,
    pub faction: Faction,
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

#[derive(Component)]
pub struct Carrying {
    pub amount: u32,
    pub weight: f32,
    pub resource_type: Option<ResourceType>,
}

impl Default for Carrying {
    fn default() -> Self {
        Self {
            amount: 0,
            weight: 0.0,
            resource_type: None,
        }
    }
}

#[derive(Component)]
pub struct GatherSpeed(pub f32);

#[derive(Component)]
pub struct CarryCapacity(pub f32);

#[derive(Component, Default)]
pub struct GatherAccumulator(pub f32);

#[derive(Component)]
pub struct DepositPoint;

#[derive(Component)]
pub struct StorageInventory {
    pub amounts: [u32; ResourceType::COUNT],
    /// Per-resource capacity limits. 0 means this resource type is NOT accepted.
    pub caps: [u32; ResourceType::COUNT],
    pub last_total: u32,
}

impl Default for StorageInventory {
    fn default() -> Self {
        Self {
            amounts: [0; ResourceType::COUNT],
            caps: [500; ResourceType::COUNT],
            last_total: 0,
        }
    }
}

impl StorageInventory {
    pub fn total(&self) -> u32 {
        self.amounts.iter().sum()
    }

    pub fn total_capacity(&self) -> u32 {
        self.caps.iter().sum()
    }

    pub fn cap_for(&self, rt: ResourceType) -> u32 {
        self.caps[rt.index()]
    }

    pub fn accepts(&self, rt: ResourceType) -> bool {
        self.caps[rt.index()] > 0
    }

    pub fn remaining_capacity(&self) -> u32 {
        ResourceType::ALL
            .iter()
            .map(|rt| self.remaining_capacity_for(*rt))
            .sum()
    }

    pub fn remaining_capacity_for(&self, rt: ResourceType) -> u32 {
        self.caps[rt.index()].saturating_sub(self.amounts[rt.index()])
    }

    pub fn get(&self, rt: ResourceType) -> u32 {
        self.amounts[rt.index()]
    }

    pub fn set_cap(&mut self, rt: ResourceType, cap: u32) {
        self.caps[rt.index()] = cap;
    }

    pub fn scale_caps(&mut self, factor: f32) {
        for cap in &mut self.caps {
            if *cap > 0 {
                *cap = (*cap as f32 * factor) as u32;
            }
        }
    }

    pub fn add_capped(&mut self, rt: ResourceType, amount: u32) -> u32 {
        let can_fit = self.remaining_capacity_for(rt).min(amount);
        if can_fit > 0 {
            self.amounts[rt.index()] += can_fit;
        }
        can_fit
    }

    pub fn accepted_types(&self) -> Vec<ResourceType> {
        ResourceType::ALL
            .iter()
            .filter(|rt| self.caps[rt.index()] > 0)
            .copied()
            .collect()
    }
}

// ── Resource Processing Buildings ──

/// Marks a building as a resource processor that auto-harvests nearby nodes.
#[derive(Component)]
pub struct ResourceProcessor {
    /// Which resource types this building can harvest
    pub resource_types: Vec<ResourceType>,
    /// Radius to claim nearby resource nodes
    pub harvest_radius: f32,
    /// Base harvest rate (units per tick)
    pub harvest_rate: f32,
    /// Max workers that can be assigned to boost output
    pub max_workers: u8,
    /// Internal buffer before transfer to storage
    pub buffer: u32,
    /// Max buffer size
    pub buffer_capacity: u32,
    /// Each worker adds this fraction of base rate (default 0.5 = 50%)
    pub worker_rate_bonus: f32,
    /// Timer that controls harvest tick interval
    pub harvest_timer: Timer,
    /// Fractional accumulator for sub-1.0 rates
    pub harvest_accumulator: f32,
}

/// Floating "+N resource" popup that appears above buildings when resources are gathered.
#[derive(Component)]
pub struct ResourcePopup {
    pub lifetime: Timer,
    pub world_pos: Vec3,
    pub resource_type: ResourceType,
    pub amount: u32,
}

/// Sub-phases for workers assigned to processing buildings.
/// Workers stay visible on the map and physically walk between nodes and buildings.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum AssignedPhase {
    #[default]
    SeekingNode,
    MovingToNode(Entity),
    Harvesting {
        node: Entity,
        timer_secs: f32,
    },
    ReturningToBuilding,
    Depositing {
        timer_secs: f32,
    },
    /// For production buildings: fetching input from storage
    FetchingInput {
        from_depot: Entity,
        resource: ResourceType,
    },
    /// Delivering fetched input to the production building
    DeliveringInput,
}

/// Marker: this worker is assigned to a building. The building entity is stored here
/// for quick lookup without needing to check UnitState.
#[derive(Component)]
pub struct BuildingAssignment(pub Entity);

// ── Production chain ──

/// A recipe that converts input resources into output resources.
#[derive(Clone, Debug)]
pub struct ProductionRecipe {
    pub name: &'static str,
    pub inputs: Vec<(ResourceType, u32)>,
    pub outputs: Vec<(ResourceType, u32)>,
    pub cycle_secs: f32,
    pub requires_level: u8,
}

/// Tracks the production state of a building that converts resources.
#[derive(Component)]
pub struct ProductionState {
    pub recipes: Vec<ProductionRecipe>,
    pub active_recipe: Option<usize>,
    pub progress_timer: Timer,
    pub input_buffer: [u32; ResourceType::COUNT],
    pub output_buffer: [u32; ResourceType::COUNT],
    pub auto_repeat: bool,
}

impl ProductionState {
    pub fn new(recipes: Vec<ProductionRecipe>) -> Self {
        Self {
            recipes,
            active_recipe: Some(0),
            progress_timer: Timer::from_seconds(1.0, TimerMode::Once),
            input_buffer: [0; ResourceType::COUNT],
            output_buffer: [0; ResourceType::COUNT],
            auto_repeat: true,
        }
    }

    /// Check if input buffer has enough for the active recipe.
    pub fn has_inputs_for_active(&self) -> bool {
        let Some(idx) = self.active_recipe else {
            return false;
        };
        let recipe = &self.recipes[idx];
        recipe
            .inputs
            .iter()
            .all(|(rt, amt)| self.input_buffer[rt.index()] >= *amt)
    }

    /// Consume inputs for the active recipe from the input buffer.
    pub fn consume_inputs(&mut self) {
        let Some(idx) = self.active_recipe else {
            return;
        };
        let recipe = &self.recipes[idx];
        for (rt, amt) in &recipe.inputs {
            self.input_buffer[rt.index()] -= amt;
        }
    }

    /// Add outputs for the active recipe to the output buffer.
    pub fn produce_outputs(&mut self) {
        let Some(idx) = self.active_recipe else {
            return;
        };
        let recipe = &self.recipes[idx];
        for (rt, amt) in &recipe.outputs {
            self.output_buffer[rt.index()] += amt;
        }
    }
}

/// Config for resource respawn around processing buildings
#[derive(Component)]
pub struct ResourceRespawnConfig {
    pub resource_types: Vec<ResourceType>,
    pub respawn_timer: Timer,
    pub respawn_radius: f32,
    pub max_nodes: u8,
    pub amount_per_node: u32,
}

/// Growing resource node (ore/oil emerging near a processing building)
#[derive(Component)]
pub struct GrowingResource {
    pub timer: Timer,
    pub target_scale: f32,
    pub resource_type: ResourceType,
    pub amount: u32,
}

#[derive(Component)]
pub struct CarryVisual(pub Entity);

#[derive(Component)]
pub struct ResourcePileVisuals {
    pub entities: Vec<Entity>,
}

#[derive(Resource)]
pub struct CarryVisualAssets {
    pub cube_mesh: Handle<Mesh>,
    pub sphere_mesh: Handle<Mesh>,
    pub materials: std::collections::HashMap<ResourceType, Handle<StandardMaterial>>,
}

#[derive(Resource)]
pub struct StoragePileAssets {
    pub cube_mesh: Handle<Mesh>,
    pub sphere_mesh: Handle<Mesh>,
    pub cylinder_mesh: Handle<Mesh>,
    pub materials: std::collections::HashMap<ResourceType, Handle<StandardMaterial>>,
}

// ── Carried resource totals (cached per-faction sum of workers' carried resources) ──

#[derive(Resource, Default)]
pub struct CarriedResourceTotals {
    pub per_faction: std::collections::HashMap<Faction, PlayerResources>,
}

impl CarriedResourceTotals {
    pub fn get(&self, faction: &Faction) -> &PlayerResources {
        static DEFAULT: std::sync::LazyLock<PlayerResources> =
            std::sync::LazyLock::new(PlayerResources::empty);
        self.per_faction.get(faction).unwrap_or(&DEFAULT)
    }
}

/// Queue of pending carry-drain requests, consumed each frame.
#[derive(Resource, Default)]
pub struct PendingCarriedDrains {
    pub drains: Vec<SpendFromCarried>,
}

/// Queued request to drain resources from workers' carried amounts.
pub struct SpendFromCarried {
    pub faction: Faction,
    pub amounts: [u32; ResourceType::COUNT],
}

impl SpendFromCarried {
    pub fn has_deficit(&self) -> bool {
        self.amounts.iter().any(|&a| a > 0)
    }

    pub fn get(&self, rt: ResourceType) -> u32 {
        self.amounts[rt.index()]
    }

    pub fn sub(&mut self, rt: ResourceType, amount: u32) {
        self.amounts[rt.index()] = self.amounts[rt.index()].saturating_sub(amount);
    }
}

// ── Resource nodes ──

#[derive(Component)]
pub struct ResourceNode {
    pub resource_type: ResourceType,
    pub amount_remaining: u32,
}

// ── Global resources ──

#[derive(Resource, Serialize, Deserialize)]
pub struct PlayerResources {
    pub amounts: [u32; ResourceType::COUNT],
}

impl Default for PlayerResources {
    fn default() -> Self {
        let mut amounts = [0; ResourceType::COUNT];
        amounts[ResourceType::Wood.index()] = 220;
        amounts[ResourceType::Copper.index()] = 20;
        amounts[ResourceType::Iron.index()] = 40;
        Self { amounts }
    }
}

impl PlayerResources {
    pub fn empty() -> Self {
        Self {
            amounts: [0; ResourceType::COUNT],
        }
    }

    pub fn add(&mut self, rt: ResourceType, amount: u32) {
        self.amounts[rt.index()] += amount;
    }

    pub fn get(&self, rt: ResourceType) -> u32 {
        self.amounts[rt.index()]
    }

    pub fn can_afford_cost(&self, cost: &crate::blueprints::ResourceCost) -> bool {
        cost.amounts
            .iter()
            .enumerate()
            .all(|(i, need)| self.amounts[i] >= *need)
    }

    pub fn subtract_cost(&mut self, cost: &crate::blueprints::ResourceCost) {
        for (amount, need) in self.amounts.iter_mut().zip(cost.amounts.iter()) {
            *amount = amount.saturating_sub(*need);
        }
    }
}

#[derive(Resource)]
pub struct LastPlayerResources {
    pub amounts: [u32; ResourceType::COUNT],
}

impl Default for LastPlayerResources {
    fn default() -> Self {
        let mut amounts = [0; ResourceType::COUNT];
        amounts[ResourceType::Wood.index()] = 220;
        amounts[ResourceType::Copper.index()] = 20;
        amounts[ResourceType::Iron.index()] = 40;
        Self { amounts }
    }
}

// ── Model assets (3D models loaded from .glb files) ──

#[derive(Resource, Default)]
pub struct ModelAssets {
    pub trees: Vec<Handle<Scene>>,
    pub dead_trees: Vec<Handle<Scene>>,
    pub rocks: Vec<Handle<Scene>>,
    pub bushes: Vec<Handle<Scene>>,
    pub grass: Vec<Handle<Scene>>,
    pub mountains: Vec<Handle<Scene>>,
}

#[derive(Component)]
pub struct Decoration;

#[derive(Component)]
pub struct DenseGrass;

#[derive(Component)]
pub struct GrassChunk {
    pub chunk_x: i32,
    pub chunk_z: i32,
}

#[derive(Resource, Default)]
pub struct GrassChunkMap(pub std::collections::HashMap<(i32, i32), Entity>);

#[derive(Resource)]
pub struct GrassGltfHandle(pub Handle<bevy::gltf::Gltf>);

#[derive(Resource, Clone)]
pub struct GrassInstanceAssets {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct ExplosiveProp {
    pub damage: f32,
    pub radius: f32,
}

// ── Resource node materials ──

#[derive(Resource)]
pub struct ResourceNodeMaterials {
    pub wood: Handle<StandardMaterial>,
    pub copper: Handle<StandardMaterial>,
    pub iron: Handle<StandardMaterial>,
    pub gold: Handle<StandardMaterial>,
    pub oil: Handle<StandardMaterial>,
}

// ── Path visualization ──

#[derive(Component)]
pub struct PathDash {
    pub owner: Entity,
}

#[derive(Component)]
pub struct PathRing {
    pub owner: Entity,
}

#[derive(Component)]
pub struct PathVisEntities {
    pub entities: Vec<Entity>,
    pub last_pos: Vec3,
    pub target: Vec3,
}

#[derive(Resource)]
pub struct PathVisAssets {
    pub dash_mesh: Handle<Mesh>,
    pub dash_material: Handle<StandardMaterial>,
    pub ring_mesh: Handle<Mesh>,
    pub ring_material: Handle<StandardMaterial>,
}

// ── Camera ──

#[derive(Component)]
pub struct RtsCamera {
    pub pivot: Vec3,
    pub distance: f32,
    pub angle: f32,
    pub pitch: f32,
    pub target_pivot: Vec3,
    pub target_distance: f32,
    pub target_angle: f32,
    pub pan_velocity: Vec3,
}

// ── Ground ──

#[derive(Component)]
pub struct Ground;

// ── Biome system ──

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Biome {
    Forest,
    Desert,
    Mud,
    Water,
    Mountain,
}

#[derive(Resource)]
pub struct BiomeMap {
    pub data: Vec<Biome>,
    pub grid_size: usize,
    pub map_size: f32,
}

impl BiomeMap {
    pub fn get_biome(&self, x: f32, z: f32) -> Biome {
        let half = self.map_size / 2.0;
        let step = self.map_size / (self.grid_size - 1) as f32;
        let ix = ((x + half) / step).round() as usize;
        let iz = ((z + half) / step).round() as usize;
        let ix = ix.min(self.grid_size - 1);
        let iz = iz.min(self.grid_size - 1);
        self.data[iz * self.grid_size + ix]
    }
}

// ── Mob marker ──

#[derive(Component)]
pub struct Mob;

// ── Combat components ──

#[derive(Component)]
pub struct AttackTarget(pub Entity);

#[derive(Component)]
pub struct AttackCooldown {
    pub timer: Timer,
}

#[derive(Component)]
pub struct AttackDamage(pub f32);

#[derive(Component)]
pub struct AttackRange(pub f32);

#[derive(Component)]
pub struct AggroRange(pub f32);

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Faction {
    Player1,
    Player2,
    Player3,
    Player4,
    Neutral,
}

impl Faction {
    pub const PLAYERS: [Faction; 4] = [
        Faction::Player1,
        Faction::Player2,
        Faction::Player3,
        Faction::Player4,
    ];

    pub fn display_name(&self) -> &'static str {
        match self {
            Faction::Player1 => "Player 1",
            Faction::Player2 => "Player 2",
            Faction::Player3 => "Player 3",
            Faction::Player4 => "Player 4",
            Faction::Neutral => "Neutral",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Faction::Player1 => Color::srgb(0.2, 0.6, 1.0), // Blue
            Faction::Player2 => Color::srgb(1.0, 0.3, 0.2), // Red
            Faction::Player3 => Color::srgb(0.7, 0.3, 0.9), // Purple
            Faction::Player4 => Color::srgb(0.2, 0.8, 0.3), // Green
            Faction::Neutral => Color::srgb(0.8, 0.2, 0.2),
        }
    }
}

/// Team configuration — factions with the same team number are allied.
#[derive(Resource)]
pub struct TeamConfig {
    pub teams: std::collections::HashMap<Faction, u8>,
}

impl Default for TeamConfig {
    fn default() -> Self {
        // Default: FFA — each faction on its own team
        let mut teams = std::collections::HashMap::new();
        teams.insert(Faction::Player1, 0);
        teams.insert(Faction::Player2, 1);
        teams.insert(Faction::Player3, 2);
        teams.insert(Faction::Player4, 3);
        Self { teams }
    }
}

impl TeamConfig {
    /// Two factions are allied if they share the same team number.
    pub fn is_allied(&self, a: &Faction, b: &Faction) -> bool {
        if a == b {
            return true;
        }
        if *a == Faction::Neutral || *b == Faction::Neutral {
            return false;
        }
        match (self.teams.get(a), self.teams.get(b)) {
            (Some(ta), Some(tb)) => ta == tb,
            _ => false,
        }
    }

    /// Two factions are hostile if they are NOT allied.
    pub fn is_hostile(&self, a: &Faction, b: &Faction) -> bool {
        !self.is_allied(a, b)
    }

    /// All factions on the same team as `faction` (including itself).
    pub fn allies_of(&self, faction: &Faction) -> Vec<Faction> {
        if *faction == Faction::Neutral {
            return vec![Faction::Neutral];
        }
        let team = self.teams.get(faction).copied();
        match team {
            Some(t) => self
                .teams
                .iter()
                .filter(|(_, &team_num)| team_num == t)
                .map(|(&f, _)| f)
                .collect(),
            None => vec![*faction],
        }
    }
}

/// Which factions are AI-controlled (not human players).
#[derive(Resource)]
pub struct AiControlledFactions {
    pub factions: HashSet<Faction>,
}

impl Default for AiControlledFactions {
    fn default() -> Self {
        let mut factions = HashSet::new();
        factions.insert(Faction::Player2);
        factions.insert(Faction::Player3);
        factions.insert(Faction::Player4);
        Self { factions }
    }
}

/// AI difficulty level — affects tick speed, resource bonuses, and thresholds.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AiDifficulty {
    Easy,
    #[default]
    Medium,
    Hard,
}

impl AiDifficulty {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Easy => "Easy",
            Self::Medium => "Medium",
            Self::Hard => "Hard",
        }
    }

    pub fn tick_multiplier(&self) -> f32 {
        match self {
            Self::Easy => 1.5,
            Self::Medium => 1.0,
            Self::Hard => 0.75,
        }
    }

    pub fn resource_bonus(&self) -> f32 {
        match self {
            Self::Easy => 0.0,
            Self::Medium => 0.0,
            Self::Hard => 0.15,
        }
    }

    pub fn attack_threshold_offset(&self) -> i32 {
        match self {
            Self::Easy => 4,
            Self::Medium => 0,
            Self::Hard => -2,
        }
    }

    pub fn max_concurrent_builds(&self) -> usize {
        match self {
            Self::Easy => 2,
            Self::Medium => 3,
            Self::Hard => 4,
        }
    }

    pub fn worker_offset(&self) -> i32 {
        match self {
            Self::Easy => -2,
            Self::Medium => 0,
            Self::Hard => 2,
        }
    }
}

/// AI personality — governs build order & army composition.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AiPersonality {
    #[default]
    Balanced,
    Aggressive,
    Defensive,
    Economic,
    Supportive,
}

impl AiPersonality {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Balanced => "Balanced",
            Self::Aggressive => "Aggressive",
            Self::Defensive => "Defensive",
            Self::Economic => "Economic",
            Self::Supportive => "Supportive",
        }
    }
}

/// Relation of an AI faction to the human player.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AiRelation {
    Friendly,
    #[default]
    Enemy,
}

/// A single ally notification event.
#[derive(Clone, Debug)]
pub struct AllyNotification {
    pub message: String,
    pub world_pos: Option<Vec3>,
    pub timestamp: f32,
    pub kind: AllyNotifyKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum AllyNotifyKind {
    UnderAttack,
    Attacking,
    NeedHelp,
    ReadyToAttack,
    EnemySpotted,
}

impl AllyNotifyKind {
    pub fn color(&self) -> Color {
        match self {
            Self::UnderAttack | Self::NeedHelp => Color::srgb(1.0, 0.6, 0.2),
            Self::Attacking | Self::ReadyToAttack => Color::srgb(0.3, 0.8, 1.0),
            Self::EnemySpotted => Color::srgb(0.9, 0.9, 0.3),
        }
    }
}

/// Active ally notifications (displayed as toasts).
#[derive(Resource, Default)]
pub struct AllyNotifications {
    pub active: Vec<AllyNotification>,
    pub last_per_kind: std::collections::HashMap<AllyNotifyKind, f32>,
}

impl AllyNotifications {
    pub fn push(
        &mut self,
        kind: AllyNotifyKind,
        message: String,
        world_pos: Option<Vec3>,
        game_time: f32,
    ) {
        // Throttle: max 1 per kind per 10s
        if let Some(&last) = self.last_per_kind.get(&kind) {
            if game_time - last < 10.0 {
                return;
            }
        }
        self.last_per_kind.insert(kind, game_time);
        self.active.push(AllyNotification {
            message,
            world_pos,
            timestamp: game_time,
            kind,
        });
        // Keep max 5
        while self.active.len() > 5 {
            self.active.remove(0);
        }
    }
}

/// Per-faction AI settings (public interface for debug panel).
#[derive(Resource, Default)]
pub struct AiFactionSettings {
    pub settings: std::collections::HashMap<Faction, AiFactionConfig>,
}

#[derive(Clone, Debug)]
pub struct AiFactionConfig {
    pub difficulty: AiDifficulty,
    pub personality: AiPersonality,
    pub relation: AiRelation,
    pub phase_name: String,
    pub posture_name: String,
    pub attack_squad_size: usize,
    pub defense_squad_size: usize,
    pub relative_strength: f32,
    pub worker_count: u8,
    pub military_count: u8,
}

impl Default for AiFactionConfig {
    fn default() -> Self {
        Self {
            difficulty: AiDifficulty::Medium,
            personality: AiPersonality::Balanced,
            relation: AiRelation::Enemy,
            phase_name: "Founding".to_string(),
            posture_name: "Normal".to_string(),
            attack_squad_size: 0,
            defense_squad_size: 0,
            relative_strength: 0.0,
            worker_count: 0,
            military_count: 0,
        }
    }
}

/// Which faction the human player is currently controlling.
#[derive(Resource)]
pub struct ActivePlayer(pub Faction);

impl Default for ActivePlayer {
    fn default() -> Self {
        Self(Faction::Player1)
    }
}

/// Per-faction resource storage.
#[derive(Resource, Default)]
pub struct AllPlayerResources {
    pub resources: std::collections::HashMap<Faction, PlayerResources>,
}

impl AllPlayerResources {
    pub fn get(&self, faction: &Faction) -> &PlayerResources {
        static DEFAULT: std::sync::LazyLock<PlayerResources> =
            std::sync::LazyLock::new(PlayerResources::empty);
        self.resources.get(faction).unwrap_or(&DEFAULT)
    }

    pub fn get_mut(&mut self, faction: &Faction) -> &mut PlayerResources {
        self.resources
            .entry(*faction)
            .or_insert_with(PlayerResources::empty)
    }
}

/// Per-faction completed buildings tracker.
#[derive(Resource, Default)]
pub struct AllCompletedBuildings {
    pub per_faction: std::collections::HashMap<Faction, Vec<EntityKind>>,
}

impl AllCompletedBuildings {
    pub fn has(&self, faction: &Faction, kind: EntityKind) -> bool {
        self.per_faction
            .get(faction)
            .map_or(false, |v| v.contains(&kind))
    }

    pub fn completed_for(&self, faction: &Faction) -> &[EntityKind] {
        static EMPTY: Vec<EntityKind> = Vec::new();
        self.per_faction
            .get(faction)
            .map_or(&EMPTY, |v| v.as_slice())
    }
}

/// Tracks whether each faction has completed its first base.
#[derive(Resource, Default)]
pub struct FactionBaseState {
    pub founded: std::collections::HashMap<Faction, bool>,
}

impl FactionBaseState {
    pub fn is_founded(&self, faction: &Faction) -> bool {
        self.founded.get(faction).copied().unwrap_or(false)
    }

    pub fn set_founded(&mut self, faction: Faction, founded: bool) {
        self.founded.insert(faction, founded);
    }
}

/// Spawn positions for each faction (map corners, avoiding mob camps).
pub const SPAWN_POSITIONS: [(Faction, (f32, f32)); 4] = [
    (Faction::Player1, (-200.0, -200.0)),
    (Faction::Player2, (200.0, -200.0)),
    (Faction::Player3, (-200.0, 200.0)),
    (Faction::Player4, (200.0, 200.0)),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PatrolStateKind {
    Idle,
    Patrolling,
    Chasing,
    Attacking,
    Returning,
}

#[derive(Component)]
pub struct PatrolState {
    pub state: PatrolStateKind,
    pub center: Vec3,
    pub radius: f32,
    pub patrol_target: Option<Vec3>,
    /// How long (seconds) this mob has been chasing a target.
    pub chase_elapsed: f32,
}

// ── Fog of War ──

#[derive(Component)]
pub struct VisionRange(pub f32);

/// Two-layer fog of war map: `visible` (per-frame) + `explored` (permanent).
#[derive(Resource)]
pub struct FogOfWarMap {
    /// Currently visible intensity per cell (0.0–1.0). Cleared each frame.
    pub visible: Vec<f32>,
    /// Permanent explored flag per cell. Once `true`, never reverts.
    pub explored: Vec<bool>,
    /// Smoothly interpolated display value for rendering/entity hiding.
    pub display: Vec<f32>,
    pub grid_size: usize,
    pub map_size: f32,
}

impl FogOfWarMap {
    fn world_to_idx(&self, x: f32, z: f32) -> Option<usize> {
        let half = self.map_size / 2.0;
        let step = self.map_size / (self.grid_size - 1) as f32;
        let ix = ((x + half) / step).round() as usize;
        let iz = ((z + half) / step).round() as usize;
        if ix >= self.grid_size || iz >= self.grid_size {
            return None;
        }
        Some(iz * self.grid_size + ix)
    }

    /// Current-frame visibility (0.0–1.0). Use for gameplay logic.
    pub fn get_visible(&self, x: f32, z: f32) -> f32 {
        self.world_to_idx(x, z)
            .map(|i| self.visible[i])
            .unwrap_or(0.0)
    }

    /// Has this cell ever been seen?
    pub fn is_explored(&self, x: f32, z: f32) -> bool {
        self.world_to_idx(x, z)
            .map(|i| self.explored[i])
            .unwrap_or(false)
    }

    /// Smoothed display value for rendering (0.0–1.0).
    pub fn get_display(&self, x: f32, z: f32) -> f32 {
        self.world_to_idx(x, z)
            .map(|i| self.display[i])
            .unwrap_or(0.0)
    }

    /// Backward-compatible: 1.0 if visible, 0.5 if explored, 0.0 if unexplored.
    pub fn get_visibility(&self, x: f32, z: f32) -> f32 {
        self.world_to_idx(x, z)
            .map(|i| {
                if self.visible[i] > 0.01 {
                    0.5 + 0.5 * self.visible[i]
                } else if self.explored[i] {
                    0.5
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0)
    }
}

/// Determines how visible an entity must be before it's shown through fog.
#[derive(Component, Clone, Copy)]
pub enum FogHideable {
    /// Enemies — high threshold (0.8), only show when clearly visible
    Mob,
    /// Resources, decorations, trees — medium threshold (0.4)
    Object,
    /// Projectiles, VFX — low threshold (0.3)
    Vfx,
}

#[derive(Component)]
pub struct FogOverlay;

// ── VFX components ──

#[derive(Component)]
pub struct Projectile {
    pub target: Entity,
    pub speed: f32,
    pub damage: f32,
}

#[derive(Component)]
pub struct VfxFlash {
    pub timer: Timer,
    pub start_scale: f32,
    pub end_scale: f32,
}

#[derive(Resource)]
pub struct VfxAssets {
    pub sphere_mesh: Handle<Mesh>,
    pub cube_mesh: Handle<Mesh>,
    pub melee_material: Handle<StandardMaterial>,
    pub projectile_material: Handle<StandardMaterial>,
    pub impact_material: Handle<StandardMaterial>,
    pub deposit_material: Handle<StandardMaterial>,
    pub dust_material: Handle<StandardMaterial>,
    pub resource_particle_materials:
        std::collections::HashMap<ResourceType, Handle<StandardMaterial>>,
}

#[derive(Component)]
pub struct GatherParticle {
    pub timer: Timer,
    pub velocity: Vec3,
    pub start_scale: f32,
}

#[derive(Component)]
pub struct FootstepDust {
    pub timer: Timer,
    pub velocity: Vec3,
}

#[derive(Component)]
pub struct FootstepTimer(pub Timer);

// ── Icon assets ──

#[derive(Resource)]
pub struct IconAssets {
    // Resources
    pub wood: Handle<Image>,
    pub copper: Handle<Image>,
    pub iron: Handle<Image>,
    pub gold: Handle<Image>,
    pub oil: Handle<Image>,
    // Processed resources
    pub planks: Handle<Image>,
    pub charcoal: Handle<Image>,
    pub bronze: Handle<Image>,
    pub steel: Handle<Image>,
    pub gunpowder: Handle<Image>,
    // Buildings
    pub base: Handle<Image>,
    pub barracks: Handle<Image>,
    pub workshop: Handle<Image>,
    pub tower: Handle<Image>,
    pub storage: Handle<Image>,
    // Units
    pub worker: Handle<Image>,
    pub soldier: Handle<Image>,
    pub archer: Handle<Image>,
    pub tank: Handle<Image>,
    // Additional buildings
    pub mage_tower: Handle<Image>,
    pub temple: Handle<Image>,
    pub stable: Handle<Image>,
    pub siege_works: Handle<Image>,
    // Production chain buildings
    pub smelter: Handle<Image>,
    pub alchemist: Handle<Image>,
    // Additional units
    pub knight: Handle<Image>,
    pub mage: Handle<Image>,
    pub priest: Handle<Image>,
    pub cavalry: Handle<Image>,
    // Siege
    pub catapult: Handle<Image>,
    pub battering_ram: Handle<Image>,
    // Mobs
    pub goblin: Handle<Image>,
    pub skeleton: Handle<Image>,
    pub orc: Handle<Image>,
    pub demon: Handle<Image>,
    // Summons
    pub skeleton_minion: Handle<Image>,
    pub spirit_wolf: Handle<Image>,
    pub fire_elemental: Handle<Image>,
}

impl IconAssets {
    pub fn resource_icon(&self, rt: ResourceType) -> Handle<Image> {
        match rt {
            ResourceType::Wood => self.wood.clone(),
            ResourceType::Copper => self.copper.clone(),
            ResourceType::Iron => self.iron.clone(),
            ResourceType::Gold => self.gold.clone(),
            ResourceType::Oil => self.oil.clone(),
            // Processed resources
            ResourceType::Planks => self.planks.clone(),
            ResourceType::Charcoal => self.charcoal.clone(),
            ResourceType::Bronze => self.bronze.clone(),
            ResourceType::Steel => self.steel.clone(),
            ResourceType::Gunpowder => self.gunpowder.clone(),
        }
    }

    pub fn entity_icon(&self, kind: EntityKind) -> Handle<Image> {
        match kind {
            // Buildings
            EntityKind::Base => self.base.clone(),
            EntityKind::Barracks => self.barracks.clone(),
            EntityKind::Workshop => self.workshop.clone(),
            EntityKind::Tower => self.tower.clone(),
            EntityKind::WatchTower => self.tower.clone(),
            EntityKind::GuardTower => self.tower.clone(),
            EntityKind::BallistaTower => self.tower.clone(),
            EntityKind::BombardTower => self.tower.clone(),
            EntityKind::Outpost => self.tower.clone(),
            EntityKind::Gatehouse => self.tower.clone(),
            EntityKind::WallSegment => self.tower.clone(),
            EntityKind::WallPost => self.tower.clone(),
            EntityKind::Storage => self.storage.clone(),
            EntityKind::House => self.storage.clone(),
            // Units
            EntityKind::Worker => self.worker.clone(),
            EntityKind::Soldier => self.soldier.clone(),
            EntityKind::Archer => self.archer.clone(),
            EntityKind::Tank => self.tank.clone(),
            EntityKind::Knight => self.knight.clone(),
            EntityKind::Mage => self.mage.clone(),
            EntityKind::Priest => self.priest.clone(),
            EntityKind::Cavalry => self.cavalry.clone(),
            // Siege
            EntityKind::Catapult => self.catapult.clone(),
            EntityKind::BatteringRam => self.battering_ram.clone(),
            // Buildings
            EntityKind::MageTower => self.mage_tower.clone(),
            EntityKind::Temple => self.temple.clone(),
            EntityKind::Stable => self.stable.clone(),
            EntityKind::SiegeWorks => self.siege_works.clone(),
            // Resource processing buildings (reuse storage icon for now)
            EntityKind::Sawmill => self.storage.clone(),
            EntityKind::Mine => self.workshop.clone(),
            EntityKind::OilRig => self.workshop.clone(),
            // Production chain buildings
            EntityKind::Smelter => self.smelter.clone(),
            EntityKind::Alchemist => self.alchemist.clone(),
            // Mobs
            EntityKind::Goblin => self.goblin.clone(),
            EntityKind::Skeleton => self.skeleton.clone(),
            EntityKind::Orc => self.orc.clone(),
            EntityKind::Demon => self.demon.clone(),
            // Summons
            EntityKind::SkeletonMinion => self.skeleton_minion.clone(),
            EntityKind::SpiritWolf => self.spirit_wolf.clone(),
            EntityKind::FireElemental => self.fire_elemental.clone(),
        }
    }
}

// ── Tree growth ──

#[derive(Component)]
pub struct Sapling {
    pub timer: Timer,
    pub target_scale: f32,
}

#[derive(Component)]
pub struct GrowingTree {
    pub stage: u8,
    pub timer: Timer,
    pub target_scale: f32,
}

#[derive(Component)]
pub struct MatureTree;

#[derive(Resource)]
pub struct TreeGrowthConfig {
    pub spawn_timer: Timer,
    pub sapling_duration: f32,
    pub growth_stage_duration: f32,
    pub max_saplings: u32,
    pub max_growing: u32,
    pub spawn_radius: f32,
    pub mature_wood_amount: u32,
}

impl Default for TreeGrowthConfig {
    fn default() -> Self {
        Self {
            spawn_timer: Timer::from_seconds(5.0, TimerMode::Repeating),
            sapling_duration: 30.0,
            growth_stage_duration: 20.0,
            max_saplings: 30,
            max_growing: 30,
            spawn_radius: 15.0,
            mature_wood_amount: 200,
        }
    }
}

// ── UI markers ──

#[derive(Component)]
pub struct ResourceText(pub ResourceType);

#[derive(Component)]
pub struct ActionBarInner;

// ── Selection info panel markers ──

#[derive(Resource, Default)]
pub struct InspectedEnemy {
    pub entity: Option<Entity>,
}

#[derive(Component)]
pub struct Boss;

#[derive(Component)]
pub struct SelectionInfoPanel;

/// Single source of truth for which UI mode is active.
/// Both the selection info panel and the action bar read this.
#[derive(Resource, Clone, PartialEq, Debug)]
pub enum UiMode {
    /// Default: no selection, show building grid
    Idle,
    /// One or more own units selected
    SelectedUnits(Vec<Entity>),
    /// One own building selected
    SelectedBuilding(Entity),
    /// Placing a building from a card/grid
    PlacingBuilding(EntityKind),
}

impl Default for UiMode {
    fn default() -> Self {
        UiMode::Idle
    }
}

#[derive(Component)]
pub struct UnitCardGrid;

#[derive(Component)]
pub struct UnitCardRef(pub Entity);

#[derive(Component)]
pub struct HpBarFill(pub Entity);

#[derive(Component)]
pub struct EnemyInspectPanel;

// ── Selection state ──

#[derive(Resource, Default)]
pub struct DragState {
    pub start: Option<Vec2>,
    pub current: Option<Vec2>,
    pub dragging: bool,
}

#[derive(Component)]
pub struct SelectionBox;

// ── Building system ──

#[derive(Component)]
pub struct Building;

/// Radius from building center that the building claims on the ground.
#[derive(Component)]
pub struct BuildingFootprint(pub f32);

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BuildingState {
    UnderConstruction,
    Complete,
}

#[derive(Component)]
pub struct ConstructionProgress {
    pub timer: Timer,
}

#[derive(Component, Default)]
pub struct ConstructionWorkers(pub u8);

#[derive(Component)]
pub struct TrainingQueue {
    pub queue: Vec<EntityKind>,
    pub timer: Option<Timer>,
    /// Running counter used to scatter spawn positions (golden-angle offset).
    pub total_trained: u32,
}

#[derive(Component)]
pub struct BuildButton(pub EntityKind);

#[derive(Component)]
pub struct TrainButton(pub EntityKind);

#[derive(Component)]
pub struct WallSegmentPiece;

#[derive(Component)]
pub struct WallPostPiece;

#[derive(Component)]
pub struct GatePiece;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PlacementMode {
    None,
    Placing(EntityKind),
    PlotBase,
    PlotWall { start: Vec3 },
    PlotGate,
}

#[derive(Resource)]
pub struct BuildingPlacementState {
    pub mode: PlacementMode,
    pub preview_entity: Option<Entity>,
    pub awaiting_release: bool,
    /// Feedback text shown during placement (e.g. biome requirement hint)
    pub hint_text: Option<String>,
}

impl Default for BuildingPlacementState {
    fn default() -> Self {
        Self {
            mode: PlacementMode::None,
            preview_entity: None,
            awaiting_release: false,
            hint_text: None,
        }
    }
}

#[derive(Resource, Default)]
pub struct WallPlotPreview {
    pub start: Option<Vec3>,
    pub snapped_points: Vec<Vec3>,
    pub ghost_entities: Vec<Entity>,
    pub total_cost: crate::blueprints::ResourceCost,
    pub valid: bool,
}

#[derive(Resource, Default)]
pub struct CompletedBuildings {
    pub completed: Vec<EntityKind>,
}

impl CompletedBuildings {
    pub fn has(&self, kind: EntityKind) -> bool {
        self.completed.contains(&kind)
    }
}

// ── Building upgrades & interactions ──

#[derive(Component)]
pub struct BuildingLevel(pub u8);

#[derive(Component)]
pub struct UpgradeProgress {
    pub timer: Timer,
    pub target_level: u8,
}

#[derive(Component)]
pub struct DemolishAnimation {
    pub timer: Timer,
    pub original_scale: Vec3,
}

#[derive(Component)]
pub struct RallyPoint(pub Vec3);

#[derive(Component)]
pub struct BuildingScaleAnim {
    pub timer: Timer,
    pub from: Vec3,
    pub to: Vec3,
}

#[derive(Component)]
pub struct LevelIndicator {
    pub building: Entity,
}

pub const DEFAULT_UNIT_CAP: u32 = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnitCapStats {
    pub used: u32,
    pub queued: u32,
    pub cap: u32,
}

impl UnitCapStats {
    pub fn reserved(self) -> u32 {
        self.used + self.queued
    }

    pub fn has_room(self, amount: u32) -> bool {
        self.reserved().saturating_add(amount) <= self.cap
    }
}

pub fn unit_capacity_bonus_for_building(kind: EntityKind, level: u8) -> u32 {
    match kind {
        EntityKind::House => 4 + 2 * u32::from(level.saturating_sub(1)),
        _ => 0,
    }
}

pub fn count_faction_units<'a>(
    faction: Faction,
    unit_factions: impl IntoIterator<Item = &'a Faction>,
) -> u32 {
    unit_factions
        .into_iter()
        .filter(|unit_faction| **unit_faction == faction)
        .count() as u32
}

pub fn count_faction_queued_units<'a>(
    faction: Faction,
    queues: impl IntoIterator<Item = (&'a Faction, &'a TrainingQueue)>,
) -> u32 {
    queues
        .into_iter()
        .filter(|(queue_faction, _)| **queue_faction == faction)
        .map(|(_, queue)| queue.queue.len() as u32)
        .sum()
}

pub fn faction_unit_cap<'a>(
    faction: Faction,
    buildings: impl IntoIterator<Item = (&'a Faction, &'a EntityKind, &'a BuildingState, &'a BuildingLevel)>,
) -> u32 {
    DEFAULT_UNIT_CAP
        + buildings
            .into_iter()
            .filter(|(building_faction, _, state, _)| {
                **building_faction == faction && **state == BuildingState::Complete
            })
            .map(|(_, kind, _, level)| unit_capacity_bonus_for_building(*kind, level.0))
            .sum::<u32>()
}

pub fn faction_unit_cap_stats<'a>(
    faction: Faction,
    unit_factions: impl IntoIterator<Item = &'a Faction>,
    queues: impl IntoIterator<Item = (&'a Faction, &'a TrainingQueue)>,
    buildings: impl IntoIterator<Item = (&'a Faction, &'a EntityKind, &'a BuildingState, &'a BuildingLevel)>,
) -> UnitCapStats {
    UnitCapStats {
        used: count_faction_units(faction, unit_factions),
        queued: count_faction_queued_units(faction, queues),
        cap: faction_unit_cap(faction, buildings),
    }
}

#[derive(Component)]
pub struct StorageAura {
    pub gather_speed_bonus: f32,
    pub range: f32,
}

#[derive(Component)]
pub struct HealingAura {
    pub heal_per_sec: f32,
    pub range: f32,
}

#[derive(Component)]
pub struct TowerAutoAttackEnabled(pub bool);

// UI markers for building interactions
#[derive(Component)]
pub struct UpgradeButton;

#[derive(Component)]
pub struct DemolishButton;

#[derive(Component)]
pub struct RallyPointButton;

#[derive(Component)]
pub struct ScuttleUnitButton;

#[derive(Component)]
pub struct DropCargoButton;

#[derive(Component)]
pub struct ConfirmDemolishButton;

#[derive(Component)]
pub struct CancelDemolishButton;

#[derive(Component)]
pub struct DemolishConfirmPanel;

#[derive(Component)]
pub struct AssignWorkerButton;

#[derive(Component)]
pub struct UnassignWorkerButton;

#[derive(Component)]
pub struct TrainingQueueDisplay;

#[derive(Component)]
pub struct TrainingProgressBar;

#[derive(Component)]
pub struct ConstructionProgressBar;

#[derive(Component)]
pub struct ConstructionWorkerCountText;

#[derive(Component)]
pub struct UpgradeProgressBar;

#[derive(Component)]
pub struct ToggleAutoAttackButton;

#[derive(Component)]
pub struct CancelTrainButton(pub usize);

#[derive(Component)]
pub struct CancelTrainQueueItemButton {
    pub building: Entity,
    pub index: usize,
}

#[derive(Component)]
pub struct CancelUnitTaskButton {
    pub unit: Entity,
    pub task_id: Option<u64>,
    pub is_current: bool,
}

#[derive(Component)]
pub struct AttackMoveButton;

#[derive(Component)]
pub struct PatrolButton;

#[derive(Component)]
pub struct HoldPositionButton;

#[derive(Component)]
pub struct StopButton;

#[derive(Component)]
pub struct CycleStanceButton;

#[derive(Component)]
pub struct ActionTooltip {
    pub owner: Entity,
}

#[derive(Component)]
pub struct ActionTooltipTrigger {
    pub text: String,
}

#[derive(Component)]
pub struct BuildingHpBarFill;

/// Marker for the child entity holding a building's GLTF scene.
#[derive(Component)]
pub struct BuildingSceneChild;

/// Marker for the child entity holding a unit/mob character GLTF scene.
#[derive(Component)]
pub struct UnitSceneChild;

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
    Attack,
    Die,
}

#[derive(Resource, Default)]
pub struct RallyPointMode(pub bool);

// ── Building materials (ghost, construction) ──

#[derive(Resource)]
pub struct BuildingGhostMaterials {
    pub ghost_valid: Handle<StandardMaterial>,
    pub ghost_invalid: Handle<StandardMaterial>,
    pub under_construction: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct GhostBuilding;

#[derive(Component)]
pub struct GhostValid(pub bool);

/// Marker for mesh entities under the ghost whose materials have been overridden.
#[derive(Component)]
pub struct GhostMaterialApplied;

// ── Build Card (legacy, kept for grid button compatibility) ──

#[derive(Component)]
pub struct BuildCard {
    pub building_kind: EntityKind,
    pub index: usize,
    pub total: usize,
    pub enabled: bool,
}

/// Marker for standard (non-ghost) buttons that receive hover/press visuals.
#[derive(Component)]
pub struct StandardButton;

/// Smooth lerp-based button animation state (replaces instant StandardButton color swaps).
#[derive(Component)]
pub struct ButtonAnimState {
    pub bg_current: [f32; 4],
    pub bg_target: [f32; 4],
    pub scale_current: f32,
    pub scale_target: f32,
}

impl ButtonAnimState {
    pub fn new(rest_bg: [f32; 4]) -> Self {
        Self {
            bg_current: rest_bg,
            bg_target: rest_bg,
            scale_current: 1.0,
            scale_target: 1.0,
        }
    }
}

/// Which visual style a ButtonAnimState button uses.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    /// Filled background (train buttons)
    Filled,
    /// Ghost/outline style (upgrade, rally, demolish)
    Ghost,
    /// Destructive ghost (demolish)
    Destructive,
}

/// Marks an action bar child for fade-out removal.
#[derive(Component)]
pub struct ActionBarFadeOut {
    pub timer: Timer,
    pub initial_offset: f32,
}

/// Marks an action bar child for fade-in entrance.
#[derive(Component)]
pub struct ActionBarFadeIn {
    pub timer: Timer,
    pub delay: Timer,
    pub started: bool,
}

// ── Generic UI Animations ──

/// Fades a UI node in over its duration (opacity 0 → 1).
#[derive(Component)]
pub struct UiFadeIn {
    pub timer: Timer,
}

/// Fades a UI node out over its duration (opacity 1 → 0), then despawns.
#[derive(Component)]
pub struct UiFadeOut {
    pub timer: Timer,
}

/// Slides a UI node in from an offset over its duration.
#[derive(Component)]
pub struct UiSlideIn {
    pub offset: Vec2,
    pub timer: Timer,
}

/// Scales a UI node in from a start scale to 1.0 with optional elastic overshoot.
#[derive(Component)]
pub struct UiScaleIn {
    pub from: f32,
    pub timer: Timer,
    pub elastic: bool,
}

/// Expands a separator line from zero width to full width.
#[derive(Component)]
pub struct UiLineExpand {
    pub target_width: f32,
    pub timer: Timer,
}

/// Floating ambient particle on the menu background.
#[derive(Component)]
pub struct MenuParticle {
    pub velocity: Vec2,
    pub base_alpha: f32,
    pub phase: f32,
}

/// Shimmer effect on title text — cycles hue/brightness.
#[derive(Component)]
pub struct TitleShimmer {
    pub phase_offset: f32,
}

/// Pulsing glow border on focused/hovered elements.
#[derive(Component)]
pub struct UiGlowPulse {
    pub color: Color,
    pub intensity: f32,
}

/// Queue count badge on a train button.
#[derive(Component)]
pub struct TrainButtonQueueBadge(pub EntityKind);

/// Tracks what text entity belongs to a train button's cost text (for coloring).
#[derive(Component)]
pub struct TrainCostText {
    pub kind: EntityKind,
}

// ── Attention & Damage Popup components ──

/// Tracks previous frame's health to detect damage without modifying combat code.
#[derive(Component)]
pub struct PreviousHealth(pub f32);

/// Timer reset whenever a unit takes damage; drives the "under attack" icon.
#[derive(Component)]
pub struct UnderAttackTimer(pub Timer);

/// Floating damage/heal number anchored to a world position.
#[derive(Component)]
pub struct DamagePopup {
    pub timer: Timer,
    pub amount: f32,
    pub is_damage: bool,
    pub world_pos: Vec3,
    pub offset_x: f32,
}

/// State icon displayed above a unit.
#[derive(Component)]
pub struct AttentionIcon {
    pub owner: Entity,
    pub kind: AttentionKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttentionKind {
    UnderAttack,
    Gathering,
    Attacking,
    Building,
}

#[derive(Resource)]
pub struct AttentionIconAssets {
    pub under_attack: Handle<Image>,
    pub gathering: Handle<Image>,
    pub attacking: Handle<Image>,
    pub building: Handle<Image>,
}

// ── Text Input ──

#[derive(Component)]
pub struct TextInputField {
    pub value: String,
    pub cursor_pos: usize,
    pub max_len: usize,
}

#[derive(Component)]
pub struct TextInputFocused;

#[derive(Component)]
pub struct TextInputCursor;

// ── Ally/Enemy Toggle ──

#[derive(Component)]
pub struct AllyToggleButton {
    pub ai_index: usize,
}

#[derive(Component)]
pub struct RandomNameButton;

// ── In-Game Overlay ──

#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum InGameOverlay {
    #[default]
    None,
    PauseMenu,
    PauseOptions,
    DeathScreen,
    Spectating,
}

/// Run-condition: returns true only when no overlay is active (player can issue commands).
pub fn player_can_command(overlay: Res<InGameOverlay>) -> bool {
    *overlay == InGameOverlay::None
}

#[derive(Resource, Default)]
pub struct FactionStats {
    pub stats: HashMap<Faction, FactionStatus>,
}

#[derive(Default, Clone)]
pub struct FactionStatus {
    pub unit_count: u32,
    pub building_count: u32,
    pub eliminated: bool,
}

/// Inserted as a resource when Restart is requested; menu reads & removes it.
#[derive(Resource)]
pub struct RestartRequested;

// ── Overlay UI markers ──

#[derive(Component)]
pub struct PauseOverlayRoot;

#[derive(Component)]
pub struct DeathScreenRoot;

#[derive(Component)]
pub struct SpectatorHudRoot;

/// Marks all in-game entities for cleanup on exit.
#[derive(Component)]
pub struct GameWorld;

#[derive(Component)]
pub struct PauseMenuButton(pub PauseAction);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PauseAction {
    Continue,
    Restart,
    MainMenu,
    Options,
    Quit,
    BackFromOptions,
    ApplySettings,
    Spectate,
}

#[derive(Component)]
pub struct SpectatorStatsText;
