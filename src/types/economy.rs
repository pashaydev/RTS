//! Resource economy types: resources, gathering, production, storage.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::app::Faction;
use crate::blueprints::{EntityKind, ResourceCost};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum ResourceType {
    // Raw (0-5)
    Wood,
    Copper,
    Iron,
    Gold,
    Oil,
    Stone,
    // Processed (6-10)
    Planks,
    Charcoal,
    Bronze,
    Steel,
    Gunpowder,
}

impl ResourceType {
    pub const RAW: [ResourceType; 6] = [
        ResourceType::Wood,
        ResourceType::Copper,
        ResourceType::Iron,
        ResourceType::Gold,
        ResourceType::Oil,
        ResourceType::Stone,
    ];

    pub const PROCESSED: [ResourceType; 5] = [
        ResourceType::Planks,
        ResourceType::Charcoal,
        ResourceType::Bronze,
        ResourceType::Steel,
        ResourceType::Gunpowder,
    ];

    pub const ALL: [ResourceType; 11] = [
        ResourceType::Wood,
        ResourceType::Copper,
        ResourceType::Iron,
        ResourceType::Gold,
        ResourceType::Oil,
        ResourceType::Stone,
        ResourceType::Planks,
        ResourceType::Charcoal,
        ResourceType::Bronze,
        ResourceType::Steel,
        ResourceType::Gunpowder,
    ];

    pub const COUNT: usize = 11;

    pub fn index(self) -> usize {
        match self {
            Self::Wood => 0,
            Self::Copper => 1,
            Self::Iron => 2,
            Self::Gold => 3,
            Self::Oil => 4,
            Self::Stone => 5,
            Self::Planks => 6,
            Self::Charcoal => 7,
            Self::Bronze => 8,
            Self::Steel => 9,
            Self::Gunpowder => 10,
        }
    }

    #[allow(dead_code)]
    pub fn is_processed(self) -> bool {
        self.index() >= 6
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Wood => "Wood",
            Self::Copper => "Copper",
            Self::Iron => "Iron",
            Self::Gold => "Gold",
            Self::Oil => "Oil",
            Self::Stone => "Stone",
            Self::Planks => "Planks",
            Self::Charcoal => "Charcoal",
            Self::Bronze => "Bronze",
            Self::Steel => "Steel",
            Self::Gunpowder => "Gunpowder",
        }
    }

    pub fn abbreviation(self) -> &'static str {
        match self {
            Self::Wood => "W",
            Self::Copper => "C",
            Self::Iron => "I",
            Self::Gold => "G",
            Self::Oil => "O",
            Self::Stone => "Sn",
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
            Self::Stone => 2.0,
            Self::Planks => 1.2,
            Self::Charcoal => 0.8,
            Self::Bronze => 2.5,
            Self::Steel => 3.0,
            Self::Gunpowder => 1.0,
        }
    }

    pub fn gather_rate_multiplier(self) -> f32 {
        match self {
            Self::Wood => 1.0,
            Self::Copper => 0.9,
            Self::Iron => 0.65,
            Self::Gold => 0.45,
            Self::Oil => 0.85,
            Self::Stone => 0.75,
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
            Self::Stone => Color::srgb(0.60, 0.60, 0.58),
            Self::Planks => Color::srgb(0.76, 0.60, 0.35),
            Self::Charcoal => Color::srgb(0.25, 0.25, 0.25),
            Self::Bronze => Color::srgb(0.80, 0.50, 0.20),
            Self::Steel => Color::srgb(0.55, 0.60, 0.70),
            Self::Gunpowder => Color::srgb(0.60, 0.20, 0.20),
        }
    }
}

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

    pub fn can_afford_cost(&self, cost: &ResourceCost) -> bool {
        cost.amounts
            .iter()
            .enumerate()
            .all(|(i, need)| self.amounts[i] >= *need)
    }

    pub fn subtract_cost(&mut self, cost: &ResourceCost) {
        for (amount, need) in self.amounts.iter_mut().zip(cost.amounts.iter()) {
            *amount = amount.saturating_sub(*need);
        }
    }
}

/// Per-faction resource storage.
#[derive(Resource, Default)]
pub struct AllPlayerResources {
    pub resources: HashMap<Faction, PlayerResources>,
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
    pub per_faction: HashMap<Faction, Vec<EntityKind>>,
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
    pub founded: HashMap<Faction, bool>,
}

impl FactionBaseState {
    pub fn is_founded(&self, faction: &Faction) -> bool {
        self.founded.get(faction).copied().unwrap_or(false)
    }

    pub fn set_founded(&mut self, faction: Faction, founded: bool) {
        self.founded.insert(faction, founded);
    }
}

#[derive(Component)]
pub struct ResourceNode {
    pub resource_type: ResourceType,
    pub amount_remaining: u32,
}

/// Vertical offset to preserve when snapping an entity back onto the terrain.
#[derive(Component, Clone, Copy, Default)]
pub struct TerrainHeightOffset(pub f32);

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

/// Marks a building as a resource processor that auto-harvests nearby nodes.
#[derive(Component)]
pub struct ResourceProcessor {
    pub resource_types: Vec<ResourceType>,
    pub harvest_radius: f32,
    pub harvest_rate: f32,
    pub max_workers: u8,
    pub buffer: u32,
    pub worker_rate_bonus: f32,
    pub harvest_timer: Timer,
    pub harvest_accumulator: f32,
}

/// Floating "+N resource" popup that appears above buildings when resources are gathered.
#[derive(Component)]
pub struct ResourcePopup {
    pub lifetime: Timer,
    pub world_pos: Vec3,
    #[allow(dead_code)]
    pub resource_type: ResourceType,
    #[allow(dead_code)]
    pub amount: u32,
}

/// Sub-phases for workers assigned to processing buildings.
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
}

/// Marker: this worker is assigned to a building.
#[derive(Component)]
pub struct BuildingAssignment(#[allow(dead_code)] pub Entity);

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

    pub fn consume_inputs(&mut self) {
        let Some(idx) = self.active_recipe else {
            return;
        };
        let recipe = &self.recipes[idx];
        for (rt, amt) in &recipe.inputs {
            self.input_buffer[rt.index()] -= amt;
        }
    }

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

#[derive(Component)]
pub struct SawmillYard {
    pub fence_entities: Vec<Entity>,
    pub tree_entities: Vec<Entity>,
    pub current_tree_count: u8,
}

/// Marks a resource node as belonging to a specific sawmill's yard.
#[derive(Component)]
pub struct YardResourceNode(pub Entity);

#[derive(Resource)]
pub struct CarryVisualAssets {
    pub cube_mesh: Handle<Mesh>,
    pub sphere_mesh: Handle<Mesh>,
    pub materials: HashMap<ResourceType, Handle<StandardMaterial>>,
}

#[derive(Resource)]
pub struct StoragePileAssets {
    pub cube_mesh: Handle<Mesh>,
    pub sphere_mesh: Handle<Mesh>,
    pub cylinder_mesh: Handle<Mesh>,
    pub materials: HashMap<ResourceType, Handle<StandardMaterial>>,
}

#[derive(Resource, Default)]
pub struct CarriedResourceTotals {
    pub per_faction: HashMap<Faction, PlayerResources>,
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

#[derive(Resource)]
pub struct ResourceNodeMaterials {
    pub wood: Handle<StandardMaterial>,
    pub copper: Handle<StandardMaterial>,
    pub iron: Handle<StandardMaterial>,
    pub _gold: Handle<StandardMaterial>,
    pub oil: Handle<StandardMaterial>,
    pub stone: Handle<StandardMaterial>,
}
