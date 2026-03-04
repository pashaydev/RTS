use bevy::prelude::*;

// ── Resource types ──

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ResourceType {
    Wood,
    Copper,
    Iron,
    Gold,
    Oil,
}

// ── Unit types ──

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum UnitType {
    Worker,
    Soldier,
    Archer,
    Tank,
}

// ── Unit markers ──

#[derive(Component)]
pub struct Unit;

#[derive(Component)]
pub struct Selected;

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

#[derive(Component)]
pub struct GatherTarget(pub Entity);

#[derive(Component)]
pub struct Carrying {
    pub amount: u32,
    pub resource_type: Option<ResourceType>,
}

impl Default for Carrying {
    fn default() -> Self {
        Self {
            amount: 0,
            resource_type: None,
        }
    }
}

#[derive(Component)]
pub struct GatherSpeed(pub f32);

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
            wood: 150,
            copper: 30,
            iron: 0,
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

// ── Unit materials & meshes ──

#[derive(Resource)]
pub struct UnitMaterials {
    pub worker_default: Handle<StandardMaterial>,
    pub worker_selected: Handle<StandardMaterial>,
    pub soldier_default: Handle<StandardMaterial>,
    pub soldier_selected: Handle<StandardMaterial>,
    pub archer_default: Handle<StandardMaterial>,
    pub archer_selected: Handle<StandardMaterial>,
    pub tank_default: Handle<StandardMaterial>,
    pub tank_selected: Handle<StandardMaterial>,
}

impl UnitMaterials {
    pub fn default_for(&self, ut: UnitType) -> Handle<StandardMaterial> {
        match ut {
            UnitType::Worker => self.worker_default.clone(),
            UnitType::Soldier => self.soldier_default.clone(),
            UnitType::Archer => self.archer_default.clone(),
            UnitType::Tank => self.tank_default.clone(),
        }
    }

    pub fn selected_for(&self, ut: UnitType) -> Handle<StandardMaterial> {
        match ut {
            UnitType::Worker => self.worker_selected.clone(),
            UnitType::Soldier => self.soldier_selected.clone(),
            UnitType::Archer => self.archer_selected.clone(),
            UnitType::Tank => self.tank_selected.clone(),
        }
    }
}

#[derive(Resource)]
pub struct UnitMeshes {
    pub worker: Handle<Mesh>,
    pub soldier: Handle<Mesh>,
    pub archer: Handle<Mesh>,
    pub tank: Handle<Mesh>,
}

impl UnitMeshes {
    pub fn mesh_for(&self, ut: UnitType) -> Handle<Mesh> {
        match ut {
            UnitType::Worker => self.worker.clone(),
            UnitType::Soldier => self.soldier.clone(),
            UnitType::Archer => self.archer.clone(),
            UnitType::Tank => self.tank.clone(),
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
pub struct PreviousMoveTarget(pub Vec3);

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
    // Current smoothed state
    pub pivot: Vec3,
    pub distance: f32,
    pub angle: f32,
    pub pitch: f32,

    // Target state (inputs write here, current lerps toward these)
    pub target_pivot: Vec3,
    pub target_distance: f32,
    pub target_angle: f32,

    // Momentum
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

// ── Mob types ──

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MobType {
    Goblin,
    Skeleton,
    Orc,
    Demon,
}

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
    pub melee_material: Handle<StandardMaterial>,
    pub projectile_material: Handle<StandardMaterial>,
    pub impact_material: Handle<StandardMaterial>,
}

// ── Mob asset resources ──

#[derive(Resource)]
pub struct MobMaterials {
    pub goblin: Handle<StandardMaterial>,
    pub skeleton: Handle<StandardMaterial>,
    pub orc: Handle<StandardMaterial>,
    pub demon: Handle<StandardMaterial>,
}

#[derive(Resource)]
pub struct MobMeshes {
    pub goblin: Handle<Mesh>,
    pub skeleton: Handle<Mesh>,
    pub orc: Handle<Mesh>,
    pub demon: Handle<Mesh>,
}

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

    pub fn building_icon(&self, bt: BuildingType) -> Handle<Image> {
        match bt {
            BuildingType::Base => self.base.clone(),
            BuildingType::Barracks => self.barracks.clone(),
            BuildingType::Workshop => self.workshop.clone(),
            BuildingType::Tower => self.tower.clone(),
            BuildingType::Storage => self.storage.clone(),
        }
    }

    pub fn unit_icon(&self, ut: UnitType) -> Handle<Image> {
        match ut {
            UnitType::Worker => self.worker.clone(),
            UnitType::Soldier => self.soldier.clone(),
            UnitType::Archer => self.archer.clone(),
            UnitType::Tank => self.tank.clone(),
        }
    }
}

// ── UI markers ──

#[derive(Component)]
pub struct ResourceText(pub ResourceType);

#[derive(Component)]
pub struct SelectedUnitsPanel;

#[derive(Component)]
pub struct SelectedUnitsSummaryText;

#[derive(Component)]
pub struct ActionBarInner;

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

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum BuildingType {
    Base,
    Barracks,
    Workshop,
    Tower,
    Storage,
}

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

#[derive(Component)]
pub struct TrainingQueue {
    pub queue: Vec<UnitType>,
    pub timer: Option<Timer>,
}

#[derive(Component)]
pub struct BuildButton(pub BuildingType);

#[derive(Component)]
pub struct TrainButton(pub UnitType);


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlacementMode {
    None,
    Placing(BuildingType),
}

#[derive(Resource)]
pub struct BuildingPlacementState {
    pub mode: PlacementMode,
    pub preview_entity: Option<Entity>,
}

impl Default for BuildingPlacementState {
    fn default() -> Self {
        Self {
            mode: PlacementMode::None,
            preview_entity: None,
        }
    }
}

#[derive(Resource, Default)]
pub struct CompletedBuildings {
    pub has_base: bool,
    pub has_barracks: bool,
    pub has_workshop: bool,
}

#[derive(Resource)]
pub struct BuildingMeshes {
    pub base: Handle<Mesh>,
    pub barracks: Handle<Mesh>,
    pub workshop: Handle<Mesh>,
    pub tower: Handle<Mesh>,
    pub storage: Handle<Mesh>,
}

impl BuildingMeshes {
    pub fn mesh_for(&self, bt: BuildingType) -> Handle<Mesh> {
        match bt {
            BuildingType::Base => self.base.clone(),
            BuildingType::Barracks => self.barracks.clone(),
            BuildingType::Workshop => self.workshop.clone(),
            BuildingType::Tower => self.tower.clone(),
            BuildingType::Storage => self.storage.clone(),
        }
    }
}

#[derive(Resource)]
pub struct BuildingMaterials {
    pub base: Handle<StandardMaterial>,
    pub barracks: Handle<StandardMaterial>,
    pub workshop: Handle<StandardMaterial>,
    pub tower: Handle<StandardMaterial>,
    pub storage: Handle<StandardMaterial>,
    pub ghost_valid: Handle<StandardMaterial>,
    pub ghost_invalid: Handle<StandardMaterial>,
    pub under_construction: Handle<StandardMaterial>,
}

impl BuildingMaterials {
    pub fn material_for(&self, bt: BuildingType) -> Handle<StandardMaterial> {
        match bt {
            BuildingType::Base => self.base.clone(),
            BuildingType::Barracks => self.barracks.clone(),
            BuildingType::Workshop => self.workshop.clone(),
            BuildingType::Tower => self.tower.clone(),
            BuildingType::Storage => self.storage.clone(),
        }
    }
}

#[derive(Component)]
pub struct GhostBuilding;
