//! Building system types: construction, placement, walls, grids, upgrades.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use super::app::Faction;
use crate::blueprints::{EntityKind, ResourceCost};

#[derive(Component)]
pub struct Building;

/// Radius from building center that the building claims on the ground.
#[derive(Component)]
pub struct BuildingFootprint(pub f32);

#[derive(Clone, Copy, Debug)]
pub struct WorkerInteractionTarget {
    pub position: Vec3,
    pub arrive_radius: f32,
}

/// Approximate total height of a building above terrain (for AABB picking).
#[derive(Component)]
pub struct BuildingHeight(pub f32);

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
pub struct WallCornerPiece;

#[derive(Component)]
pub struct GatePiece;

#[derive(Component)]
pub struct FloorTile;

/// Grid coordinate of a wall entity in the WallGrid.
#[derive(Component, Clone, Copy)]
pub struct WallGridCoord(pub i32, pub i32);

/// Grid coordinate of a floor tile entity in the FloorGrid.
#[derive(Component, Clone, Copy)]
pub struct FloorGridCoord(pub i32, pub i32);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PlacementMode {
    None,
    Placing(EntityKind),
    PlotBase,
    PlotWall { start: Vec3 },
    PlotGate,
    PlotFloor,
}

#[derive(Resource)]
pub struct BuildingPlacementState {
    pub mode: PlacementMode,
    pub preview_entity: Option<Entity>,
    pub awaiting_release: bool,
    /// Feedback text shown during placement (e.g. biome requirement hint)
    pub hint_text: Option<String>,
    /// Y-axis rotation in radians for the building being placed (H/J to rotate).
    pub rotation_y: f32,
    /// Entity for the green grid plane shown under the ghost building.
    pub grid_plane_entity: Option<Entity>,
}

impl Default for BuildingPlacementState {
    fn default() -> Self {
        Self {
            mode: PlacementMode::None,
            preview_entity: None,
            awaiting_release: false,
            hint_text: None,
            rotation_y: 0.0,
            grid_plane_entity: None,
        }
    }
}

#[derive(Resource, Default)]
pub struct WallPlotPreview {
    pub start: Option<Vec3>,
    pub snapped_points: Vec<Vec3>,
    pub ghost_entities: Vec<Entity>,
    pub total_cost: ResourceCost,
    pub valid: bool,
}

#[derive(Resource, Default)]
pub struct FloorPlotPreview {
    pub start: Option<Vec3>,
    pub cells: Vec<(i32, i32)>,
    pub ghost_entities: Vec<Entity>,
    pub total_cost: ResourceCost,
    pub valid: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum FloorPieceKind {
    Isolated,
    End,
    Straight,
    Corner,
    Tee,
    Cross,
}

// ── Wall Grid Auto-Tiling ──

pub const WALL_CELL_SIZE: f32 = 3.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WallPieceKind {
    Post,
    Straight,
    Corner,
    Gate,
}

#[derive(Clone)]
pub struct WallGridCell {
    pub entity: Entity,
    pub _faction: Faction,
    pub piece_kind: WallPieceKind,
    pub is_gate: bool,
    pub rotation_y: f32,
}

#[derive(Resource, Default)]
pub struct WallGrid {
    pub cells: HashMap<(i32, i32), WallGridCell>,
    pub dirty: Vec<(i32, i32)>,
}

impl WallGrid {
    pub fn world_to_grid(pos: Vec3) -> (i32, i32) {
        (
            (pos.x / WALL_CELL_SIZE).round() as i32,
            (pos.z / WALL_CELL_SIZE).round() as i32,
        )
    }

    pub fn grid_to_world(gx: i32, gz: i32) -> Vec3 {
        Vec3::new(gx as f32 * WALL_CELL_SIZE, 0.0, gz as f32 * WALL_CELL_SIZE)
    }

    /// Returns cardinal neighbor coords: [North, East, South, West]
    pub fn cardinal_neighbors(gx: i32, gz: i32) -> [(i32, i32); 4] {
        [
            (gx, gz - 1), // North (-Z)
            (gx + 1, gz), // East (+X)
            (gx, gz + 1), // South (+Z)
            (gx - 1, gz), // West (-X)
        ]
    }

    /// Mark a cell and its 4 cardinal neighbors as dirty for re-evaluation.
    pub fn mark_dirty(&mut self, gx: i32, gz: i32) {
        self.dirty.push((gx, gz));
        for (nx, nz) in Self::cardinal_neighbors(gx, gz) {
            self.dirty.push((nx, nz));
        }
    }

    /// Compute 4-bit neighbor mask for a cell. Bit 0=N, 1=E, 2=S, 3=W.
    pub fn neighbor_mask(&self, gx: i32, gz: i32) -> u8 {
        let mut mask = 0u8;
        for (i, (nx, nz)) in Self::cardinal_neighbors(gx, gz).iter().enumerate() {
            if self.cells.contains_key(&(*nx, *nz)) {
                mask |= 1 << i;
            }
        }
        mask
    }
}

#[derive(Resource, Default)]
pub struct FloorGrid {
    pub cells: HashMap<(i32, i32), FloorGridCell>,
    pub dirty: Vec<(i32, i32)>,
}

#[derive(Clone)]
pub struct FloorGridCell {
    pub entity: Entity,
    pub _faction: Faction,
    pub piece_kind: FloorPieceKind,
    pub rotation_y: f32,
}

impl FloorGrid {
    pub fn mark_dirty(&mut self, gx: i32, gz: i32) {
        self.dirty.push((gx, gz));
        for (nx, nz) in WallGrid::cardinal_neighbors(gx, gz) {
            self.dirty.push((nx, nz));
        }
    }

    pub fn neighbor_mask(&self, gx: i32, gz: i32) -> u8 {
        let mut mask = 0u8;
        for (i, (nx, nz)) in WallGrid::cardinal_neighbors(gx, gz).iter().enumerate() {
            if self.cells.contains_key(&(*nx, *nz)) {
                mask |= 1 << i;
            }
        }
        mask
    }
}

// ── Obstacle grid (trees / natural blockers) ──

/// Sparse grid of cells occupied by natural obstacles (trees) plus border margin.
#[derive(Resource, Default)]
pub struct ObstacleGrid {
    pub cells: HashSet<(i32, i32)>,
    /// Half-size of the playable area (excludes border hills). 0 = not yet initialised.
    pub playable_half: f32,
}

impl ObstacleGrid {
    /// Is a world-space position inside the border hills (unbuildable edge)?
    fn is_in_border(&self, x: f32, z: f32) -> bool {
        self.playable_half > 0.0 && (x.abs() > self.playable_half || z.abs() > self.playable_half)
    }

    /// Is a single grid cell blocked by an obstacle or border?
    pub fn is_cell_blocked(&self, gx: i32, gz: i32) -> bool {
        if self.cells.contains(&(gx, gz)) {
            return true;
        }
        let world = WallGrid::grid_to_world(gx, gz);
        self.is_in_border(world.x, world.z)
    }

    /// Is a world-space point blocked?
    pub fn is_blocked(&self, pos: Vec3) -> bool {
        if self.is_in_border(pos.x, pos.z) {
            return true;
        }
        let (gx, gz) = WallGrid::world_to_grid(pos);
        self.cells.contains(&(gx, gz))
    }

    /// Does a circular footprint centered at `pos` with `radius` overlap any obstacle cell
    /// or extend into the border?
    pub fn is_footprint_blocked(&self, pos: Vec3, radius: f32) -> bool {
        if self.is_in_border(pos.x - radius, pos.z)
            || self.is_in_border(pos.x + radius, pos.z)
            || self.is_in_border(pos.x, pos.z - radius)
            || self.is_in_border(pos.x, pos.z + radius)
        {
            return true;
        }
        let cells_needed = (radius / WALL_CELL_SIZE).ceil() as i32 + 1;
        let (cx, cz) = WallGrid::world_to_grid(pos);
        let r_sq = radius * radius;
        for dx in -cells_needed..=cells_needed {
            for dz in -cells_needed..=cells_needed {
                let gx = cx + dx;
                let gz = cz + dz;
                if !self.cells.contains(&(gx, gz)) {
                    continue;
                }
                let cell_world = WallGrid::grid_to_world(gx, gz);
                let ddx = cell_world.x - pos.x;
                let ddz = cell_world.z - pos.z;
                if ddx * ddx + ddz * ddz < r_sq {
                    return true;
                }
            }
        }
        false
    }

    pub fn is_oriented_rect_blocked(
        &self,
        center: Vec3,
        half_x: f32,
        half_z: f32,
        rotation_y: f32,
    ) -> bool {
        let corners = [
            Vec2::new(-half_x, -half_z),
            Vec2::new(half_x, -half_z),
            Vec2::new(half_x, half_z),
            Vec2::new(-half_x, half_z),
        ];
        for corner in corners {
            let rotated = Quat::from_rotation_y(rotation_y) * Vec3::new(corner.x, 0.0, corner.y);
            let world = center + rotated;
            if self.is_in_border(world.x, world.z) {
                return true;
            }
        }

        let search_radius = half_x.hypot(half_z);
        let cells_needed = (search_radius / WALL_CELL_SIZE).ceil() as i32 + 1;
        let (cx, cz) = WallGrid::world_to_grid(center);
        let inv = Quat::from_rotation_y(-rotation_y);

        for dx in -cells_needed..=cells_needed {
            for dz in -cells_needed..=cells_needed {
                let gx = cx + dx;
                let gz = cz + dz;
                if !self.cells.contains(&(gx, gz)) {
                    continue;
                }
                let cell_world = WallGrid::grid_to_world(gx, gz);
                let local =
                    inv * Vec3::new(cell_world.x - center.x, 0.0, cell_world.z - center.z);
                if local.x.abs() < half_x && local.z.abs() < half_z {
                    return true;
                }
            }
        }

        false
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
    buildings: impl IntoIterator<
        Item = (
            &'a Faction,
            &'a EntityKind,
            &'a BuildingState,
            &'a BuildingLevel,
        ),
    >,
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
    buildings: impl IntoIterator<
        Item = (
            &'a Faction,
            &'a EntityKind,
            &'a BuildingState,
            &'a BuildingLevel,
        ),
    >,
) -> UnitCapStats {
    UnitCapStats {
        used: count_faction_units(faction, unit_factions),
        queued: count_faction_queued_units(faction, queues),
        cap: faction_unit_cap(faction, buildings),
    }
}

/// Returns the income modifier for a faction based on their total unit count.
pub fn income_modifier_for_population(unit_count: u32) -> f32 {
    match unit_count {
        0..=20 => 1.0,
        21..=40 => 0.85,
        41..=60 => 0.70,
        _ => 0.50,
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

/// Marker: building harvesting/production is paused.
#[derive(Component)]
pub struct BuildingPaused;

/// Tracks which construction stage model is currently shown (0=foundation, 1=partial, 2=complete).
#[derive(Component)]
pub struct ConstructionStage(pub u8);

#[derive(Resource, Default)]
pub struct RallyPointMode(pub bool);

// ── Building materials (ghost, construction) ──

#[derive(Resource)]
pub struct BuildingGhostMaterials {
    pub ghost_valid: Handle<StandardMaterial>,
    pub ghost_invalid: Handle<StandardMaterial>,
    pub under_construction: Handle<StandardMaterial>,
    pub grid_plane: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct GhostBuilding;

/// Marker for the green grid plane shown under building ghost during placement.
#[derive(Component)]
pub struct GhostGridPlane;

/// Marker: vegetation around this building has already been cleared.
#[derive(Component)]
pub struct VegetationCleared;

#[derive(Component)]
pub struct GhostValid(pub bool);

/// Marker for mesh entities under the ghost whose materials have been overridden.
#[derive(Component)]
pub struct GhostMaterialApplied;

/// Marker for the child entity holding a building's GLTF scene.
#[derive(Component)]
pub struct BuildingSceneChild;
