use bevy::prelude::*;
use serde::{Deserialize, Serialize};

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
}

/// Worker assigned to work inside a resource processing building
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub struct AssignedToProcessor(pub Entity);

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

// ── Card-Hand UI components ──

#[derive(Component)]
#[allow(dead_code)]
pub struct CardHand;

#[derive(Component)]
pub struct BuildCard {
    pub building_kind: EntityKind,
    pub index: usize,
    pub total: usize,
    pub enabled: bool,
}

#[derive(Component)]
pub struct CardAnimState {
    pub offset_y: f32,
    pub scale: f32,
    pub rotation_deg: f32,
    pub opacity: f32,
    pub target_offset_y: f32,
    pub target_scale: f32,
    pub target_rotation_deg: f32,
    pub target_opacity: f32,
    /// Per-card sine offset for idle breathing animation
    pub idle_phase: f32,
    /// Current pseudo-3D tilt (degrees)
    pub tilt_x_deg: f32,
    /// Target tilt toward cursor (degrees)
    pub target_tilt_x_deg: f32,
    /// Accumulated time for glow pulsing
    pub glow_pulse: f32,
}

impl CardAnimState {
    pub fn new(rotation_deg: f32, offset_y: f32, index: usize) -> Self {
        Self {
            offset_y: -250.0,
            scale: 0.2,
            rotation_deg,
            opacity: 0.0,
            target_offset_y: offset_y,
            target_scale: 0.58,
            target_rotation_deg: rotation_deg,
            target_opacity: 1.0,
            idle_phase: index as f32 * 1.3,
            tilt_x_deg: 0.0,
            target_tilt_x_deg: 0.0,
            glow_pulse: 0.0,
        }
    }
}

#[derive(Component)]
pub struct CardDragState {
    pub dragging: bool,
    pub screen_pos: Vec2,
    pub velocity: Vec2,
    pub pickup_origin: Vec2,
    pub drag_distance: f32,
}

impl Default for CardDragState {
    fn default() -> Self {
        Self {
            dragging: false,
            screen_pos: Vec2::ZERO,
            velocity: Vec2::ZERO,
            pickup_origin: Vec2::ZERO,
            drag_distance: 0.0,
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CardTier {
    Common,
    Uncommon,
    Rare,
    Epic,
}

impl CardTier {
    pub fn from_total_cost(total: u32) -> Self {
        match total {
            0..100 => CardTier::Common,
            100..250 => CardTier::Uncommon,
            250..500 => CardTier::Rare,
            _ => CardTier::Epic,
        }
    }
}

#[derive(Component)]
pub struct CardBorderGlow;

#[derive(Component)]
pub struct CardShineEffect;

#[derive(Component)]
pub struct CardIconContainer;

#[derive(Component)]
pub struct CardDealIn {
    pub delay_timer: Timer,
    pub anim_timer: Timer,
    pub started: bool,
}

#[derive(Component)]
pub struct CardPlayOut {
    pub timer: Timer,
}

#[derive(Component)]
pub struct CardSpringBack {
    pub timer: Timer,
}

#[derive(Component)]
pub struct CardGlow;

#[derive(Component)]
pub struct CardCostEntry {
    pub resource_type: ResourceType,
    pub amount: u32,
}

#[derive(Component)]
pub struct CardNameText;

#[derive(Component)]
pub struct CardTooltip;

/// Returns (rotation_deg, y_offset) for a card at `index` out of `total` in the fan arc.
pub fn fan_params(index: usize, total: usize) -> (f32, f32) {
    if total <= 1 {
        return (0.0, 0.0);
    }
    let t = index as f32 / (total - 1) as f32;
    let centered = t - 0.5;
    let rotation_deg = centered * 22.0;
    let y_offset = centered.abs() * 40.0;
    (rotation_deg, y_offset)
}

/// Marker for standard (non-ghost) buttons that receive hover/press visuals.
#[derive(Component)]
pub struct StandardButton;

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
