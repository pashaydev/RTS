use bevy::prelude::*;

use crate::blueprints::EntityKind;

// ── Resource types ──

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
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
    MovingToResource(Entity),
    Gathering(Entity),
    ReturningToDeposit { depot: Entity, gather_node: Option<Entity> },
    Depositing { depot: Entity, gather_node: Option<Entity> },
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

    pub fn get(&self, rt: ResourceType) -> u32 {
        match rt {
            ResourceType::Wood => self.wood,
            ResourceType::Copper => self.copper,
            ResourceType::Iron => self.iron,
            ResourceType::Gold => self.gold,
            ResourceType::Oil => self.oil,
        }
    }
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

// ── Resource nodes ──

#[derive(Component)]
pub struct ResourceNode {
    pub resource_type: ResourceType,
    pub amount_remaining: u32,
}

// ── Global resources ──

#[derive(Resource)]
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

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Faction {
    Player,
    Enemy,
}

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

#[derive(Resource)]
pub struct FogOfWarMap {
    pub visibility: Vec<f32>,
    pub grid_size: usize,
    pub map_size: f32,
}

impl FogOfWarMap {
    pub fn get_visibility(&self, x: f32, z: f32) -> f32 {
        let half = self.map_size / 2.0;
        let step = self.map_size / (self.grid_size - 1) as f32;
        let ix = ((x + half) / step).round() as usize;
        let iz = ((z + half) / step).round() as usize;
        if ix >= self.grid_size || iz >= self.grid_size {
            return 0.0;
        }
        self.visibility[iz * self.grid_size + ix]
    }
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
    // Mobs
    pub goblin: Handle<Image>,
    pub skeleton: Handle<Image>,
    pub orc: Handle<Image>,
    pub demon: Handle<Image>,
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
            // Mobs
            EntityKind::Goblin => self.goblin.clone(),
            EntityKind::Skeleton => self.skeleton.clone(),
            EntityKind::Orc => self.orc.clone(),
            EntityKind::Demon => self.demon.clone(),
            // Fallback — use worker icon for new types without icons yet
            _ => self.worker.clone(),
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

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
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
}

impl CardAnimState {
    pub fn new(rotation_deg: f32, offset_y: f32) -> Self {
        Self {
            offset_y: -200.0,
            scale: 0.5,
            rotation_deg,
            opacity: 0.0,
            target_offset_y: offset_y,
            target_scale: 1.0,
            target_rotation_deg: rotation_deg,
            target_opacity: 1.0,
        }
    }
}

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
    let rotation_deg = centered * 10.0;
    let y_offset = centered.abs() * 20.0;
    (rotation_deg, y_offset)
}
