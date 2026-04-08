//! Rendering & visual types: camera, terrain, fog, VFX, grass, decorations, models.

use bevy::prelude::*;
use std::collections::HashMap;

use super::app::Faction;
use super::combat::{CombatFxKind, DamageType};
use super::economy::ResourceType;
use crate::blueprints::EntityKind;
use crate::presentation::materials::grass::GrassMaterial;
use crate::presentation::materials::tree_occlusion::TreeOcclusionMaterial;

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
    Grassland,
    Forest,
    Desert,
    Beach,
    Wetland,
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

// ── Frustum Culling ──

/// Marker added to entities outside the camera frustum.
#[derive(Component)]
pub struct FrustumCulled;

/// Marker for the camera whose frustum drives culling decisions.
#[derive(Component)]
pub struct CullingSourceCamera;

/// Per-entity bounding sphere for frustum culling.
#[derive(Component)]
pub struct CullingBounds {
    pub radius: f32,
    pub center_offset: Vec3,
}

impl CullingBounds {
    pub fn new(radius: f32) -> Self {
        Self {
            radius,
            center_offset: Vec3::ZERO,
        }
    }

    pub fn with_offset(radius: f32, center_offset: Vec3) -> Self {
        Self {
            radius,
            center_offset,
        }
    }
}

/// Tracks the reason an entity is hidden, for debug inspection.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CullReason {
    Visible,
    Frustum,
    Distance,
    Fog,
}

impl Default for CullReason {
    fn default() -> Self {
        Self::Visible
    }
}

/// Resource controlling frustum debug mode.
#[derive(Resource)]
pub struct FrustumDebugMode {
    pub enabled: bool,
    pub freeze_main_camera: bool,
    /// Frozen frustum data from the main camera (stored when freeze is activated).
    pub frozen_pivot: Vec3,
    pub frozen_distance: f32,
    pub frozen_angle: f32,
    pub frozen_pitch: f32,
    /// Observer fly-camera state.
    pub observer_pos: Vec3,
    pub observer_yaw: f32,
    pub observer_pitch: f32,
    pub observer_speed: f32,
}

impl Default for FrustumDebugMode {
    fn default() -> Self {
        Self {
            enabled: false,
            freeze_main_camera: true,
            frozen_pivot: Vec3::ZERO,
            frozen_distance: 60.0,
            frozen_angle: 0.0,
            frozen_pitch: 0.8,
            observer_pos: Vec3::new(0.0, 80.0, 0.0),
            observer_yaw: 0.0,
            observer_pitch: -std::f32::consts::FRAC_PI_4,
            observer_speed: 40.0,
        }
    }
}

// ── Camera Zoom Detail Level ──

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailLevel {
    Close,  // distance < 25.0
    Medium, // 25.0 .. 55.0
    Far,    // > 55.0
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct CameraZoomLevel {
    pub distance: f32,
    pub detail: DetailLevel,
}

impl Default for CameraZoomLevel {
    fn default() -> Self {
        Self {
            distance: 60.0,
            detail: DetailLevel::Far,
        }
    }
}

impl CameraZoomLevel {
    pub fn from_distance(d: f32) -> Self {
        let detail = if d < 25.0 {
            DetailLevel::Close
        } else if d < 55.0 {
            DetailLevel::Medium
        } else {
            DetailLevel::Far
        };
        Self {
            distance: d,
            detail,
        }
    }
}

// ── Fog of War ──

#[derive(Component)]
pub struct VisionRange(pub f32);

/// Two-layer fog of war map: `visible` (per-frame) + `explored` (permanent).
#[derive(Resource)]
pub struct FogOfWarMap {
    pub visible: Vec<f32>,
    #[allow(dead_code)]
    pub visible_next: Vec<f32>,
    pub explored: Vec<u8>,
    pub display: Vec<f32>,
    pub grid_size: usize,
    pub step: f32,
    pub half_map: f32,
}

impl FogOfWarMap {
    fn world_to_idx(&self, x: f32, z: f32) -> Option<usize> {
        let ix = ((x + self.half_map) / self.step).round() as usize;
        let iz = ((z + self.half_map) / self.step).round() as usize;
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
            .map(|i| self.explored[i] != 0)
            .unwrap_or(false)
    }

    /// Backward-compatible: 1.0 if visible, 0.5 if explored, 0.0 if unexplored.
    pub fn get_visibility(&self, x: f32, z: f32) -> f32 {
        self.world_to_idx(x, z)
            .map(|i| {
                if self.visible[i] > 0.01 {
                    0.5 + 0.5 * self.visible[i]
                } else if self.explored[i] != 0 {
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
    pub source: Entity,
    pub target: Entity,
    pub speed: f32,
    pub damage: f32,
    pub damage_type: DamageType,
    pub fx_kind: CombatFxKind,
    pub impact_scale: f32,
    /// When true, the projectile rotates to face its travel direction (arrows/bolts).
    pub orient_to_velocity: bool,
}

#[derive(Component)]
pub struct VfxFlash {
    pub timer: Timer,
    pub start_scale: f32,
    pub end_scale: f32,
    pub rise_speed: f32,
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
    pub resource_particle_materials: HashMap<ResourceType, Handle<StandardMaterial>>,
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
pub struct CombatDust {
    pub timer: Timer,
    pub velocity: Vec3,
    pub start_scale: f32,
}

/// Visual effect when a resource node is fully depleted.
#[derive(Component)]
pub struct DepletionAnimation {
    pub timer: Timer,
    pub kind: DepletionKind,
    pub initial_scale: Vec3,
}

#[derive(Clone, Copy)]
pub enum DepletionKind {
    /// Trees: tip over like felled timber
    TreeFall { fall_direction: Vec3 },
    /// Mines (Iron, Stone, Copper, Gold): dust/rock puff, shrink into ground
    MinePuff,
    /// Oil: dark particle spray, sink
    OilSpray,
}

/// Marks a unit as dying — plays death animation then despawns.
#[derive(Component)]
pub struct Dying {
    pub timer: Timer,
    pub _killed_by: Option<Faction>,
    pub original_scale: Vec3,
}

/// Marks a freshly spawned entity for scale-in animation.
#[derive(Component)]
pub struct SpawnAnimation {
    pub timer: Timer,
    pub target_scale: Vec3,
}

/// Enhanced VFX for summoned creatures (SpiritWolf, FireElemental).
#[derive(Component)]
pub struct SummonVfx {
    pub color: Color,
    pub emissive: LinearRgba,
    pub _pulse_speed: f32,
    pub particle_timer: Timer,
    pub light_entity: Option<Entity>,
}

// ── Procedural Mob Visuals ──

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MobVisualKind {
    Goblin,
    Skeleton,
    Orc,
    Demon,
}

#[derive(Component)]
pub struct ProceduralMob {
    pub visual_kind: MobVisualKind,
    pub phase: f32,
    pub base_y_offset: f32,
    pub base_scale: Vec3,
    pub base_translation: Vec3,
    pub attack_timer: Option<Timer>,
    pub initialized: bool,
    pub pulse_ring_spawned: bool,
    pub dying_progress: f32,
}

#[derive(Component)]
pub struct DemonPulseRing {
    pub timer: Timer,
}

// ── Model assets ──

#[derive(Resource, Default)]
pub struct ModelAssets {
    pub trees: Vec<Handle<Scene>>,
    pub tree_base_scales: Vec<f32>,
    pub dead_trees: Vec<Handle<Scene>>,
    pub rocks: Vec<Handle<Scene>>,
    pub bushes: Vec<Handle<Scene>>,
    pub grass: Vec<Handle<Scene>>,
    pub mountains: Vec<Handle<Scene>>,
}

#[derive(Component)]
pub struct Decoration;

// ── Grass chunk instancing ──

#[derive(Component)]
pub struct GrassChunk {
    pub chunk_x: i32,
    pub chunk_z: i32,
}

/// Stores a reduced-density mesh for distant grass chunks.
/// The culling system swaps between the full mesh (on entity) and this LOD mesh.
#[derive(Component)]
pub struct GrassChunkLod {
    pub full_mesh: Handle<Mesh>,
    pub reduced_mesh: Handle<Mesh>,
}

/// Distance² threshold where grass switches from full to reduced density.
pub const GRASS_LOD_SWITCH_SQ: f32 = 50.0 * 50.0;

#[derive(Resource, Default)]
pub struct GrassChunkMap(pub HashMap<(i32, i32), Entity>);

#[derive(Resource, Clone)]
pub struct GrassInstanceAssets {
    pub mesh: Handle<Mesh>,
    pub material: Handle<GrassMaterial>,
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct GrassDebugSettings {
    pub enabled: bool,
    pub spacing: f32,
    pub row_step_factor: f32,
    pub jitter: f32,
    pub density_threshold: f32,
    pub grassland_weight: f32,
    pub wetland_weight: f32,
    pub forest_weight: f32,
    pub scale_min: f32,
    pub scale_max: f32,
    pub lean_strength: f32,
}

impl Default for GrassDebugSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            spacing: 0.15,
            row_step_factor: 0.86,
            jitter: 0.4,
            density_threshold: 0.51,
            grassland_weight: 1.0,
            wetland_weight: 0.45,
            forest_weight: 0.18,
            scale_min: 1.5,
            scale_max: 3.1,
            lean_strength: 1.0,
        }
    }
}

#[derive(Resource, Debug)]
pub struct GrassRebuildState {
    pub dirty: bool,
    pub chunk_count: usize,
    pub instance_count: u32,
}

impl Default for GrassRebuildState {
    fn default() -> Self {
        Self {
            dirty: true,
            chunk_count: 0,
            instance_count: 0,
        }
    }
}

#[derive(Resource, Clone, Debug)]
pub struct GrassRenderSettings {
    pub base_color: Vec4,
    pub tip_color: Vec4,
    pub wind_strength: f32,
    pub wind_speed: f32,
    pub wind_direction: Vec4,
    pub random_lean: f32,
    pub blade_width: f32,
    pub blade_height: f32,
    pub width_thicken: f32,
    pub normal_up_bias: f32,
    pub normal_blend_start: f32,
    pub normal_blend_end: f32,
}

impl Default for GrassRenderSettings {
    fn default() -> Self {
        Self {
            base_color: Vec4::new(0.12, 0.28, 0.04, 1.0),
            tip_color: Vec4::new(0.45, 0.65, 0.15, 1.0),
            wind_strength: 0.08,
            wind_speed: 1.2,
            wind_direction: Vec4::new(1.0, 0.0, 0.6, 0.0),
            random_lean: 0.35,
            blade_width: 0.06,
            blade_height: 0.5,
            width_thicken: 0.6,
            normal_up_bias: 0.4,
            normal_blend_start: 40.0,
            normal_blend_end: 120.0,
        }
    }
}

// ── Decoration chunk instancing ──

/// Raw GLTF handles for decoration models, used by extraction system.
#[derive(Resource)]
pub struct DecoGltfHandles {
    pub bushes: Vec<Handle<bevy::gltf::Gltf>>,
    pub rocks: Vec<Handle<bevy::gltf::Gltf>>,
    pub grass: Vec<Handle<bevy::gltf::Gltf>>,
    pub mountains: Vec<Handle<bevy::gltf::Gltf>>,
}

/// Extracted mesh+material per decoration variant, ready for CPU vertex merging.
#[derive(Resource, Default)]
pub struct DecorationInstanceAssets {
    pub bushes: Vec<(Handle<Mesh>, Handle<StandardMaterial>)>,
    pub bush_model_sizes: Vec<usize>,
    pub rocks: Vec<(Handle<Mesh>, Handle<StandardMaterial>)>,
    pub grass: Vec<(Handle<Mesh>, Handle<StandardMaterial>)>,
    pub _mountains: Vec<(Handle<Mesh>, Handle<StandardMaterial>)>,
}

/// Chunk of merged decoration geometry.
#[derive(Component)]
pub struct DecoChunk {
    pub chunk_x: i32,
    pub chunk_z: i32,
}

/// Marker added to DecoChunk entities once the chunk area has been explored.
#[derive(Component)]
pub struct DecoRevealed;

/// Marker added to GrassChunk entities once the chunk area has been explored.
#[derive(Component)]
pub struct GrassRevealed;

/// Maps chunk coords to decoration chunk entities.
#[derive(Resource, Default)]
pub struct DecoChunkMap(pub HashMap<(i32, i32), Vec<Entity>>);

/// Pending decoration placements collected at spawn time, consumed once assets are extracted.
#[derive(Resource)]
pub struct PendingDecorationPlacements {
    pub bushes: Vec<(usize, Vec3, f32, f32)>,
    pub rocks: Vec<(usize, Vec3, f32, f32)>,
    pub grass: Vec<(usize, Vec3, f32, f32)>,
}

// ── Path visualization ──

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum PathVisCategory {
    #[default]
    Move,
    Attack,
    Patrol,
    Gather,
    Build,
}

impl PathVisCategory {
    pub const ALL: [PathVisCategory; 5] = [
        PathVisCategory::Move,
        PathVisCategory::Attack,
        PathVisCategory::Patrol,
        PathVisCategory::Gather,
        PathVisCategory::Build,
    ];

    /// Returns (base_color, emissive, ring_base_color, ring_emissive)
    pub fn colors(&self) -> (Color, LinearRgba, Color, LinearRgba) {
        match self {
            PathVisCategory::Move => (
                Color::srgba(1.0, 0.8, 0.2, 0.8),
                LinearRgba::new(1.2, 0.8, 0.2, 1.0),
                Color::srgba(0.9, 0.7, 0.15, 0.65),
                LinearRgba::new(0.7, 0.45, 0.05, 1.0),
            ),
            PathVisCategory::Attack => (
                Color::srgba(1.0, 0.2, 0.15, 0.85),
                LinearRgba::new(1.5, 0.15, 0.1, 1.0),
                Color::srgba(1.0, 0.15, 0.1, 0.7),
                LinearRgba::new(1.5, 0.1, 0.05, 1.0),
            ),
            PathVisCategory::Patrol => (
                Color::srgba(0.2, 0.75, 1.0, 0.75),
                LinearRgba::new(0.2, 0.9, 1.4, 1.0),
                Color::srgba(0.15, 0.65, 0.9, 0.6),
                LinearRgba::new(0.15, 0.7, 1.2, 1.0),
            ),
            PathVisCategory::Gather => (
                Color::srgba(0.3, 0.9, 0.35, 0.75),
                LinearRgba::new(0.2, 1.0, 0.25, 1.0),
                Color::srgba(0.25, 0.8, 0.3, 0.6),
                LinearRgba::new(0.15, 0.8, 0.2, 1.0),
            ),
            PathVisCategory::Build => (
                Color::srgba(1.0, 0.65, 0.15, 0.8),
                LinearRgba::new(1.2, 0.6, 0.1, 1.0),
                Color::srgba(0.9, 0.55, 0.1, 0.65),
                LinearRgba::new(1.0, 0.5, 0.08, 1.0),
            ),
        }
    }

    pub fn dash_scale(&self) -> f32 {
        match self {
            PathVisCategory::Attack => 1.2,
            PathVisCategory::Gather => 0.85,
            _ => 1.0,
        }
    }

    pub fn spacing_mult(&self) -> f32 {
        match self {
            PathVisCategory::Attack => 0.8,
            _ => 1.0,
        }
    }

    pub fn flow_speed(&self) -> f32 {
        match self {
            PathVisCategory::Attack => 2.5,
            PathVisCategory::Patrol => 1.2,
            _ => 1.5,
        }
    }
}

#[derive(Component)]
pub struct PathDash {
    pub owner: Entity,
}

#[derive(Component)]
pub struct PathDashMeta {
    pub frac: f32,
    pub category: PathVisCategory,
    pub base_y: f32,
    pub base_scale: f32,
}

#[derive(Component)]
pub struct PathRing {
    pub owner: Entity,
    pub category: PathVisCategory,
    pub base_y: f32,
    pub base_scale: f32,
}

#[derive(Component)]
pub struct PathVisEntities {
    pub entities: Vec<Entity>,
    pub last_pos: Vec3,
    pub target: Vec3,
    pub category: PathVisCategory,
}

#[derive(Resource)]
pub struct PathVisAssets {
    pub dash_mesh: Handle<Mesh>,
    pub ring_mesh: Handle<Mesh>,
    pub crosshair_h_mesh: Handle<Mesh>,
    pub crosshair_v_mesh: Handle<Mesh>,
    pub dash_materials: HashMap<PathVisCategory, Handle<StandardMaterial>>,
    pub ring_materials: HashMap<PathVisCategory, Handle<StandardMaterial>>,
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

/// Marker: tree scene materials have been converted from Blend→Mask.
#[derive(Component)]
pub struct TreeAlphaFixed;

/// Deduplicated handles to tree leaf materials.
#[derive(Resource, Default)]
pub struct TreeLeafMaterials(pub Vec<Handle<TreeOcclusionMaterial>>);

#[derive(Resource)]
pub struct TreeGrowthConfig {
    pub spawn_timer: Timer,
    pub sapling_duration: f32,
    pub growth_stage_duration: f32,
    pub max_saplings: u32,
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
            spawn_radius: 15.0,
            mature_wood_amount: 200,
        }
    }
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
    pub stone: Handle<Image>,
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
            ResourceType::Stone => self.stone.clone(),
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
            EntityKind::WallCorner => self.tower.clone(),
            EntityKind::Floor => self.storage.clone(),
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
            EntityKind::Scout => self.archer.clone(),
            // Siege
            EntityKind::Catapult => self.catapult.clone(),
            EntityKind::BatteringRam => self.battering_ram.clone(),
            // Buildings
            EntityKind::MageTower => self.mage_tower.clone(),
            EntityKind::Temple => self.temple.clone(),
            EntityKind::Stable => self.stable.clone(),
            EntityKind::SiegeWorks => self.siege_works.clone(),
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
