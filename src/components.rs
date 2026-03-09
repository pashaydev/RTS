use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::blueprints::EntityKind;

// ── Resource types ──

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    Wood,
    Copper,
    Iron,
    Gold,
    Oil,
}

impl ResourceType {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Wood => "Wood",
            Self::Copper => "Copper",
            Self::Iron => "Iron",
            Self::Gold => "Gold",
            Self::Oil => "Oil",
        }
    }

    pub fn weight(self) -> f32 {
        match self {
            Self::Wood => 1.0,
            Self::Copper => 1.5,
            Self::Iron => 2.0,
            Self::Gold => 2.5,
            Self::Oil => 1.2,
        }
    }

    pub fn carry_color(self) -> Color {
        match self {
            Self::Wood => Color::srgb(0.55, 0.35, 0.15),
            Self::Copper => Color::srgb(0.72, 0.45, 0.2),
            Self::Iron => Color::srgb(0.55, 0.55, 0.58),
            Self::Gold => Color::srgb(0.95, 0.8, 0.2),
            Self::Oil => Color::srgb(0.08, 0.08, 0.1),
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

// ── Gathering ──

#[derive(Component, Clone, Copy, PartialEq, Debug, Default)]
pub enum WorkerTask {
    #[default]
    Idle,
    /// Player issued a manual move command — do NOT auto-gather until arrival.
    ManualMove,
    MovingToResource(Entity),
    Gathering(Entity),
    ReturningToDeposit { depot: Entity, gather_node: Option<Entity> },
    Depositing { depot: Entity, gather_node: Option<Entity> },
    WaitingForStorage { depot: Entity, gather_node: Option<Entity> },
    MovingToBuild(Entity),
    Building(Entity),
    /// Worker is assigned to a processor building (visual work loop)
    AssignedToBuilding(Entity),
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

#[derive(Component)]
pub struct DepositPoint;

#[derive(Component)]
pub struct StorageInventory {
    pub wood: u32,
    pub copper: u32,
    pub iron: u32,
    pub gold: u32,
    pub oil: u32,
    pub capacity: u32,
    pub last_total: u32,
}

impl Default for StorageInventory {
    fn default() -> Self {
        Self {
            wood: 0, copper: 0, iron: 0, gold: 0, oil: 0,
            capacity: 500,
            last_total: 0,
        }
    }
}

impl StorageInventory {
    pub fn total(&self) -> u32 {
        self.wood + self.copper + self.iron + self.gold + self.oil
    }

    pub fn remaining_capacity(&self) -> u32 {
        self.capacity.saturating_sub(self.total())
    }

    pub fn get(&self, rt: ResourceType) -> u32 {
        match rt {
            ResourceType::Wood => self.wood,
            ResourceType::Copper => self.copper,
            ResourceType::Iron => self.iron,
            ResourceType::Gold => self.gold,
            ResourceType::Oil => self.oil,
        }
    }

    /// Add resources up to capacity. Returns amount actually stored.
    pub fn add_capped(&mut self, rt: ResourceType, amount: u32) -> u32 {
        let can_fit = self.remaining_capacity().min(amount);
        if can_fit == 0 {
            return 0;
        }
        match rt {
            ResourceType::Wood => self.wood += can_fit,
            ResourceType::Copper => self.copper += can_fit,
            ResourceType::Iron => self.iron += can_fit,
            ResourceType::Gold => self.gold += can_fit,
            ResourceType::Oil => self.oil += can_fit,
        }
        can_fit
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
    /// Base harvest rate (units per second)
    pub harvest_rate: f32,
    /// Max workers that can be assigned to boost output
    pub max_workers: u8,
    /// Internal buffer before transfer to storage
    pub buffer: u32,
    /// Max buffer size
    pub buffer_capacity: u32,
    /// Each worker adds this fraction of base rate (default 0.5 = 50%)
    pub worker_rate_bonus: f32,
}

/// Worker assigned to work inside a resource processing building
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct AssignedToProcessor(pub Entity);

/// Sub-state for workers assigned to processor buildings (visual work loop)
#[derive(Component, Clone, Copy, PartialEq, Debug, Default)]
pub enum ProcessorWorkerState {
    #[default]
    Idle,
    MovingToNode(Entity),
    Harvesting { node: Entity, timer_secs: f32 },
    ReturningToBuilding,
    Depositing { timer_secs: f32 },
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
        static DEFAULT: std::sync::LazyLock<PlayerResources> = std::sync::LazyLock::new(PlayerResources::empty);
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
    pub wood: u32,
    pub copper: u32,
    pub iron: u32,
    pub gold: u32,
    pub oil: u32,
}

impl SpendFromCarried {
    pub fn has_deficit(&self) -> bool {
        self.wood > 0 || self.copper > 0 || self.iron > 0 || self.gold > 0 || self.oil > 0
    }

    pub fn get(&self, rt: ResourceType) -> u32 {
        match rt {
            ResourceType::Wood => self.wood,
            ResourceType::Copper => self.copper,
            ResourceType::Iron => self.iron,
            ResourceType::Gold => self.gold,
            ResourceType::Oil => self.oil,
        }
    }

    pub fn sub(&mut self, rt: ResourceType, amount: u32) {
        match rt {
            ResourceType::Wood => self.wood = self.wood.saturating_sub(amount),
            ResourceType::Copper => self.copper = self.copper.saturating_sub(amount),
            ResourceType::Iron => self.iron = self.iron.saturating_sub(amount),
            ResourceType::Gold => self.gold = self.gold.saturating_sub(amount),
            ResourceType::Oil => self.oil = self.oil.saturating_sub(amount),
        }
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
    pub wood: u32,
    pub copper: u32,
    pub iron: u32,
    pub gold: u32,
    pub oil: u32,
}

impl Default for PlayerResources {
    fn default() -> Self {
        Self {
            wood: 300,
            copper: 60,
            iron: 20,
            gold: 0,
            oil: 0,
        }
    }
}

impl PlayerResources {
    pub fn empty() -> Self {
        Self { wood: 0, copper: 0, iron: 0, gold: 0, oil: 0 }
    }

    pub fn add(&mut self, rt: ResourceType, amount: u32) {
        match rt {
            ResourceType::Wood => self.wood += amount,
            ResourceType::Copper => self.copper += amount,
            ResourceType::Iron => self.iron += amount,
            ResourceType::Gold => self.gold += amount,
            ResourceType::Oil => self.oil += amount,
        }
    }

    pub fn get(&self, rt: ResourceType) -> u32 {
        match rt {
            ResourceType::Wood => self.wood,
            ResourceType::Copper => self.copper,
            ResourceType::Iron => self.iron,
            ResourceType::Gold => self.gold,
            ResourceType::Oil => self.oil,
        }
    }

    pub fn can_afford(&self, wood: u32, copper: u32, iron: u32, gold: u32, oil: u32) -> bool {
        self.wood >= wood
            && self.copper >= copper
            && self.iron >= iron
            && self.gold >= gold
            && self.oil >= oil
    }

    pub fn subtract(&mut self, wood: u32, copper: u32, iron: u32, gold: u32, oil: u32) {
        self.wood -= wood;
        self.copper -= copper;
        self.iron -= iron;
        self.gold -= gold;
        self.oil -= oil;
    }
}

#[derive(Resource)]
pub struct LastPlayerResources {
    pub wood: u32,
    pub copper: u32,
    pub iron: u32,
    pub gold: u32,
    pub oil: u32,
}

impl Default for LastPlayerResources {
    fn default() -> Self {
        Self {
            wood: 300,
            copper: 60,
            iron: 20,
            gold: 0,
            oil: 0,
        }
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
}

#[derive(Component)]
pub struct Decoration;

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
    pub const PLAYERS: [Faction; 4] = [Faction::Player1, Faction::Player2, Faction::Player3, Faction::Player4];

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
            Faction::Player1 => Color::srgb(0.2, 0.6, 1.0),   // Blue
            Faction::Player2 => Color::srgb(1.0, 0.3, 0.2),   // Red
            Faction::Player3 => Color::srgb(0.7, 0.3, 0.9),   // Purple
            Faction::Player4 => Color::srgb(0.2, 0.8, 0.3),   // Green
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
        // Default: 2v2 — P1+P2 (team 0) vs P3+P4 (team 1)
        let mut teams = std::collections::HashMap::new();
        teams.insert(Faction::Player1, 0);
        teams.insert(Faction::Player2, 0);
        teams.insert(Faction::Player3, 1);
        teams.insert(Faction::Player4, 1);
        Self { teams }
    }
}

impl TeamConfig {
    /// Two factions are allied if they share the same team number.
    pub fn is_allied(&self, a: &Faction, b: &Faction) -> bool {
        if a == b { return true; }
        if *a == Faction::Neutral || *b == Faction::Neutral { return false; }
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
        if *faction == Faction::Neutral { return vec![Faction::Neutral]; }
        let team = self.teams.get(faction).copied();
        match team {
            Some(t) => self.teams.iter()
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
    pub fn push(&mut self, kind: AllyNotifyKind, message: String, world_pos: Option<Vec3>, game_time: f32) {
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
}

impl Default for AiFactionConfig {
    fn default() -> Self {
        Self {
            difficulty: AiDifficulty::Medium,
            personality: AiPersonality::Balanced,
            relation: AiRelation::Enemy,
            phase_name: "EarlyGame".to_string(),
            posture_name: "Normal".to_string(),
            attack_squad_size: 0,
            defense_squad_size: 0,
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
        static DEFAULT: std::sync::LazyLock<PlayerResources> = std::sync::LazyLock::new(PlayerResources::empty);
        self.resources.get(faction).unwrap_or(&DEFAULT)
    }

    pub fn get_mut(&mut self, faction: &Faction) -> &mut PlayerResources {
        self.resources.entry(*faction).or_insert_with(PlayerResources::empty)
    }
}

/// Per-faction completed buildings tracker.
#[derive(Resource, Default)]
pub struct AllCompletedBuildings {
    pub per_faction: std::collections::HashMap<Faction, Vec<EntityKind>>,
}

impl AllCompletedBuildings {
    pub fn has(&self, faction: &Faction, kind: EntityKind) -> bool {
        self.per_faction.get(faction).map_or(false, |v| v.contains(&kind))
    }

    pub fn completed_for(&self, faction: &Faction) -> &[EntityKind] {
        static EMPTY: Vec<EntityKind> = Vec::new();
        self.per_faction.get(faction).map_or(&EMPTY, |v| v.as_slice())
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
    pub resource_particle_materials: std::collections::HashMap<ResourceType, Handle<StandardMaterial>>,
}

#[derive(Component)]
pub struct GatherParticle {
    pub timer: Timer,
    pub velocity: Vec3,
}

#[derive(Component)]
pub struct FootstepDust {
    pub timer: Timer,
    pub velocity: Vec3,
}

#[derive(Component)]
pub struct GatherParticleTimer(pub Timer);

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
        }
    }

    pub fn entity_icon(&self, kind: EntityKind) -> Handle<Image> {
        match kind {
            // Buildings
            EntityKind::Base => self.base.clone(),
            EntityKind::Barracks => self.barracks.clone(),
            EntityKind::Workshop => self.workshop.clone(),
            EntityKind::Tower => self.tower.clone(),
            EntityKind::Storage => self.storage.clone(),
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
}

#[derive(Component)]
pub struct BuildButton(pub EntityKind);

#[derive(Component)]
pub struct TrainButton(pub EntityKind);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlacementMode {
    None,
    Placing(EntityKind),
}

#[derive(Resource)]
pub struct BuildingPlacementState {
    pub mode: PlacementMode,
    pub preview_entity: Option<Entity>,
    pub awaiting_release: bool,
}

impl Default for BuildingPlacementState {
    fn default() -> Self {
        Self {
            mode: PlacementMode::None,
            preview_entity: None,
            awaiting_release: false,
        }
    }
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
pub struct ActionTooltip;

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
