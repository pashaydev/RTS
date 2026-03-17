use bevy::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::blueprints::EntityKind;
use crate::components::{
    AnimState, AttentionIconAssets, GrassGltfHandle, GrassInstanceAssets, IconAssets, ModelAssets,
    TeamColor,
};

pub struct ModelAssetsPlugin;

impl Plugin for ModelAssetsPlugin {
    fn build(&self, app: &mut App) {
        let asset_server = app.world().resource::<AssetServer>().clone();

        // Load icon assets eagerly so they're available to all Startup systems
        let icons = IconAssets {
            wood: asset_server.load("icons/resources/wood.png"),
            copper: asset_server.load("icons/resources/copper.png"),
            iron: asset_server.load("icons/resources/iron.png"),
            gold: asset_server.load("icons/resources/gold.png"),
            oil: asset_server.load("icons/resources/oil.png"),
            planks: asset_server.load("icons/resources/planks.png"),
            charcoal: asset_server.load("icons/resources/charcoal.png"),
            bronze: asset_server.load("icons/resources/bronze.png"),
            steel: asset_server.load("icons/resources/steel.png"),
            gunpowder: asset_server.load("icons/resources/gunpowder.png"),
            base: asset_server.load("icons/buildings/base.png"),
            barracks: asset_server.load("icons/buildings/barracks.png"),
            workshop: asset_server.load("icons/buildings/workshop.png"),
            tower: asset_server.load("icons/buildings/tower.png"),
            storage: asset_server.load("icons/buildings/storage.png"),
            worker: asset_server.load("icons/units/worker.png"),
            soldier: asset_server.load("icons/units/soldier.png"),
            archer: asset_server.load("icons/units/archer.png"),
            tank: asset_server.load("icons/units/tank.png"),
            mage_tower: asset_server.load("icons/buildings/mage_tower.png"),
            temple: asset_server.load("icons/buildings/temple.png"),
            stable: asset_server.load("icons/buildings/stable.png"),
            siege_works: asset_server.load("icons/buildings/siege_works.png"),
            smelter: asset_server.load("icons/buildings/smelter.png"),
            alchemist: asset_server.load("icons/buildings/alchemist.png"),
            knight: asset_server.load("icons/units/knight.png"),
            mage: asset_server.load("icons/units/mage.png"),
            priest: asset_server.load("icons/units/priest.png"),
            cavalry: asset_server.load("icons/units/cavalry.png"),
            catapult: asset_server.load("icons/units/catapult.png"),
            battering_ram: asset_server.load("icons/units/battering_ram.png"),
            goblin: asset_server.load("icons/mobs/goblin.png"),
            skeleton: asset_server.load("icons/mobs/skeleton.png"),
            orc: asset_server.load("icons/mobs/orc.png"),
            demon: asset_server.load("icons/mobs/demon.png"),
            skeleton_minion: asset_server.load("icons/summons/skeleton_minion.png"),
            spirit_wolf: asset_server.load("icons/summons/spirit_wolf.png"),
            fire_elemental: asset_server.load("icons/summons/fire_elemental.png"),
        };
        app.insert_resource(icons);

        // Load attention icon assets (CC BY 3.0, game-icons.net by Lorc)
        let attention_icons = AttentionIconAssets {
            under_attack: asset_server.load("icons/attention/under_attack.png"),
            gathering: asset_server.load("icons/attention/gathering.png"),
            attacking: asset_server.load("icons/attention/attacking.png"),
            building: asset_server.load("icons/attention/building.png"),
        };
        app.insert_resource(attention_icons);

        // Load building GLTF model assets eagerly so they're available to Startup systems
        let building_models = load_building_model_assets_eager(&asset_server);
        app.insert_resource(building_models);

        // Load building construction stage assets
        let construction_assets = load_building_construction_assets(&asset_server);
        app.insert_resource(construction_assets);

        // Load unit/mob character model assets eagerly
        let unit_models = load_unit_model_assets_eager(&asset_server);
        app.insert_resource(unit_models);

        // Load TTP raw GLTF handles for animation extraction
        let ttp_gltf_handles = load_ttp_gltf_handles(&asset_server);
        app.insert_resource(ttp_gltf_handles);

        // Load team color textures
        let team_colors = load_team_color_textures(&asset_server);
        app.insert_resource(team_colors);
        app.init_resource::<TeamColorMaterialCache>();

        // Load grass GLTF for dense instanced grass
        let grass_gltf: Handle<bevy::gltf::Gltf> =
            asset_server.load(format!("{BASE_PATH}/Grass_2_D_Color1.gltf"));
        app.insert_resource(GrassGltfHandle(grass_gltf));

        app.add_systems(Startup, (load_model_assets, load_animation_assets))
            .add_systems(
                Update,
                (
                    extract_grass_instance_assets,
                    extract_ttp_animations,
                    apply_team_color_textures,
                ),
            );
    }
}

const BASE_PATH: &str = "KayKit_Forest_Nature/Assets/gltf";

fn load_gltf_scenes(asset_server: &AssetServer, names: &[&str]) -> Vec<Handle<Scene>> {
    names
        .iter()
        .map(|name| asset_server.load(format!("{BASE_PATH}/{name}#Scene0")))
        .collect()
}

fn load_model_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    let trees = load_gltf_scenes(
        &asset_server,
        &[
            "Tree_1_A_Color1.gltf",
            "Tree_1_B_Color1.gltf",
            "Tree_1_C_Color1.gltf",
            "Tree_2_A_Color1.gltf",
            "Tree_2_B_Color1.gltf",
            "Tree_2_C_Color1.gltf",
            "Tree_2_D_Color1.gltf",
            "Tree_2_E_Color1.gltf",
            "Tree_3_A_Color1.gltf",
            "Tree_3_B_Color1.gltf",
            "Tree_3_C_Color1.gltf",
            "Tree_4_A_Color1.gltf",
            "Tree_4_B_Color1.gltf",
            "Tree_4_C_Color1.gltf",
        ],
    );

    let dead_trees = load_gltf_scenes(
        &asset_server,
        &[
            "Tree_Bare_1_A_Color1.gltf",
            "Tree_Bare_1_B_Color1.gltf",
            "Tree_Bare_1_C_Color1.gltf",
            "Tree_Bare_2_A_Color1.gltf",
            "Tree_Bare_2_B_Color1.gltf",
            "Tree_Bare_2_C_Color1.gltf",
        ],
    );

    let rocks = load_gltf_scenes(
        &asset_server,
        &[
            "Rock_1_A_Color1.gltf",
            "Rock_1_B_Color1.gltf",
            "Rock_1_C_Color1.gltf",
            "Rock_1_D_Color1.gltf",
            "Rock_1_E_Color1.gltf",
            "Rock_2_A_Color1.gltf",
            "Rock_2_B_Color1.gltf",
            "Rock_2_C_Color1.gltf",
            "Rock_3_A_Color1.gltf",
            "Rock_3_B_Color1.gltf",
        ],
    );

    let bushes = load_gltf_scenes(
        &asset_server,
        &[
            "Bush_1_A_Color1.gltf",
            "Bush_1_B_Color1.gltf",
            "Bush_1_C_Color1.gltf",
            "Bush_2_A_Color1.gltf",
            "Bush_2_B_Color1.gltf",
            "Bush_2_C_Color1.gltf",
            "Bush_3_A_Color1.gltf",
            "Bush_3_B_Color1.gltf",
        ],
    );

    let grass = load_gltf_scenes(
        &asset_server,
        &[
            "Grass_1_A_Color1.gltf",
            "Grass_1_B_Color1.gltf",
            "Grass_1_C_Color1.gltf",
            "Grass_1_D_Color1.gltf",
        ],
    );

    let mountains = [
        "UltimateFantasyRTS/glTF/Mountain_Group_1.gltf#Scene0",
        "UltimateFantasyRTS/glTF/Mountain_Group_2.gltf#Scene0",
        "UltimateFantasyRTS/glTF/MountainLarge_Single.gltf#Scene0",
        "UltimateFantasyRTS/glTF/Mountain_Single.gltf#Scene0",
    ]
    .iter()
    .map(|path| asset_server.load(*path))
    .collect();

    commands.insert_resource(ModelAssets {
        trees,
        dead_trees,
        rocks,
        bushes,
        grass,
        mountains,
    });
}

// ── Building GLTF Model Assets ──

pub struct BuildingModelCalibration {
    pub scale: f32,
    pub y_offset: f32,
    pub building_height: f32,
}

#[derive(Resource)]
pub struct BuildingModelAssets {
    pub scenes: HashMap<(EntityKind, u8), Vec<Handle<Scene>>>,
    pub calibration: HashMap<EntityKind, BuildingModelCalibration>,
}

impl BuildingModelAssets {
    pub fn scene_for(&self, kind: EntityKind, level: u8, world_pos: Vec3) -> Option<Handle<Scene>> {
        let variants = self.scenes.get(&(kind, level))?;
        if variants.is_empty() {
            return None;
        }

        let variant_index = if variants.len() == 1 {
            0
        } else {
            building_variant_index(kind, world_pos, variants.len())
        };

        variants.get(variant_index).cloned()
    }
}

const TTP_BUILDINGS_PATH: &str = "ToonyTinyPeople/models/buildings";
const TTP_CONSTRUCTION_PATH: &str = "ToonyTinyPeople/models/buildings/construction";

fn building_variant_index(kind: EntityKind, world_pos: Vec3, variant_count: usize) -> usize {
    let mut hasher = DefaultHasher::new();
    kind.hash(&mut hasher);
    ((world_pos.x * 10.0).round() as i32).hash(&mut hasher);
    ((world_pos.z * 10.0).round() as i32).hash(&mut hasher);
    (hasher.finish() as usize) % variant_count
}

/// Maps EntityKind to the TTP building GLB filename (without extension).
fn ttp_building_glb(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Base => "Castle",
        EntityKind::Barracks => "Barracks",
        EntityKind::Workshop => "Workshop",
        EntityKind::Tower => "Tower_A",
        EntityKind::WatchTower => "Tower_B",
        EntityKind::GuardTower => "Tower_C",
        EntityKind::BallistaTower => "Tower_A",
        EntityKind::BombardTower => "Tower_B",
        EntityKind::Storage => "Granary",
        EntityKind::House => "House",
        EntityKind::MageTower => "MageTower",
        EntityKind::Temple => "Temple",
        EntityKind::Stable => "Stables",
        EntityKind::SiegeWorks => "Blacksmith",
        EntityKind::Sawmill => "LumberMill",
        EntityKind::Mine => "Market",
        EntityKind::OilRig => "Farm",
        EntityKind::Smelter => "Keep",
        EntityKind::Alchemist => "Library",
        EntityKind::Outpost => "BeastLair",
        EntityKind::Gatehouse => "Wall_A_gate",
        EntityKind::WallSegment => "Wall_A_wall",
        EntityKind::WallPost => "Wall_A_1x1",
        _ => "House", // fallback
    }
}

fn load_building_model_assets_eager(asset_server: &AssetServer) -> BuildingModelAssets {
    let mut scenes = HashMap::new();

    let building_kinds = [
        EntityKind::Base,
        EntityKind::Barracks,
        EntityKind::Workshop,
        EntityKind::Tower,
        EntityKind::WatchTower,
        EntityKind::GuardTower,
        EntityKind::BallistaTower,
        EntityKind::BombardTower,
        EntityKind::Storage,
        EntityKind::House,
        EntityKind::MageTower,
        EntityKind::Temple,
        EntityKind::Stable,
        EntityKind::SiegeWorks,
        EntityKind::Sawmill,
        EntityKind::Mine,
        EntityKind::OilRig,
        EntityKind::Smelter,
        EntityKind::Alchemist,
        EntityKind::Outpost,
        EntityKind::Gatehouse,
        EntityKind::WallSegment,
        EntityKind::WallPost,
    ];

    // TTP buildings have no level variants — same model for L1/L2/L3
    for kind in building_kinds {
        let glb = ttp_building_glb(kind);
        let handle =
            asset_server.load(format!("{TTP_BUILDINGS_PATH}/{glb}.glb#Scene0"));
        for level in 1..=3u8 {
            scenes.insert((kind, level), vec![handle.clone()]);
        }
    }

    // (kind, scale, y_offset, building_height)
    let calibration_data: &[(EntityKind, f32, f32, f32)] = &[
        (EntityKind::Base, 0.75, 0.0, 6.0),
        (EntityKind::Barracks, 0.75, 0.0, 5.0),
        (EntityKind::Workshop, 0.75, 0.0, 5.0),
        (EntityKind::Tower, 0.75, 0.0, 10.0),
        (EntityKind::WatchTower, 0.75, 0.0, 10.0),
        (EntityKind::GuardTower, 0.75, 0.0, 10.0),
        (EntityKind::BallistaTower, 0.75, 0.0, 10.0),
        (EntityKind::BombardTower, 0.75, 0.0, 10.0),
        (EntityKind::Outpost, 1.0, 0.0, 6.0),
        (EntityKind::Gatehouse, 0.75, 0.0, 8.0),
        (EntityKind::WallSegment, 1.0, 0.0, 4.0),
        (EntityKind::WallPost, 1.0, 0.0, 4.0),
        (EntityKind::Storage, 0.75, 0.0, 4.0),
        (EntityKind::House, 0.75, 0.0, 4.0),
        (EntityKind::MageTower, 0.75, 0.0, 8.0),
        (EntityKind::Temple, 0.75, 0.0, 6.0),
        (EntityKind::Stable, 0.75, 0.0, 5.0),
        (EntityKind::SiegeWorks, 0.75, 0.0, 5.0),
        (EntityKind::Sawmill, 0.75, 0.0, 5.0),
        (EntityKind::Mine, 0.75, 0.0, 4.0),
        (EntityKind::OilRig, 0.75, 0.0, 4.0),
        (EntityKind::Smelter, 0.75, 0.0, 6.0),
        (EntityKind::Alchemist, 0.75, 0.0, 5.0),
    ];
    let calibration: HashMap<_, _> = calibration_data
        .iter()
        .map(|&(kind, scale, y_offset, building_height)| {
            (
                kind,
                BuildingModelCalibration {
                    scale,
                    y_offset,
                    building_height,
                },
            )
        })
        .collect();

    BuildingModelAssets {
        scenes,
        calibration,
    }
}

// ── Building Construction Stage Assets ──

#[derive(Resource)]
pub struct BuildingConstructionAssets {
    /// (EntityKind, stage) -> Scene handle; stage 0 = foundation, 1 = partial
    pub stages: HashMap<(EntityKind, u8), Handle<Scene>>,
}

fn load_building_construction_assets(asset_server: &AssetServer) -> BuildingConstructionAssets {
    let mut stages = HashMap::new();

    let building_kinds = [
        EntityKind::Base,
        EntityKind::Barracks,
        EntityKind::Workshop,
        EntityKind::Tower,
        EntityKind::WatchTower,
        EntityKind::GuardTower,
        EntityKind::BallistaTower,
        EntityKind::BombardTower,
        EntityKind::Storage,
        EntityKind::House,
        EntityKind::MageTower,
        EntityKind::Temple,
        EntityKind::Stable,
        EntityKind::SiegeWorks,
        EntityKind::Sawmill,
        EntityKind::Mine,
        EntityKind::OilRig,
        EntityKind::Smelter,
        EntityKind::Alchemist,
        // Outpost uses BeastLair model which has no construction stage files
        EntityKind::Gatehouse,
        EntityKind::WallSegment,
        EntityKind::WallPost,
    ];

    for kind in building_kinds {
        let glb = ttp_building_glb(kind);
        for stage in 0..=1u8 {
            let handle = asset_server.load(format!(
                "{TTP_CONSTRUCTION_PATH}/{glb}_{stage}.glb#Scene0"
            ));
            stages.insert((kind, stage), handle);
        }
    }

    BuildingConstructionAssets { stages }
}

// ── Unit/Mob Character Model Assets ──

pub struct CharacterModelCalibration {
    pub scale: f32,
    pub y_offset: f32,
    pub facing_rotation: f32, // radians around Y axis
}

#[derive(Resource)]
pub struct UnitModelAssets {
    pub scenes: HashMap<EntityKind, Handle<Scene>>,
    pub calibration: HashMap<EntityKind, CharacterModelCalibration>,
}

const TTP_UNITS_PATH: &str = "ToonyTinyPeople/models/units";
const TTP_MACHINES_PATH: &str = "ToonyTinyPeople/models/units/machines";
const SKELETONS_PATH: &str = "KayKit_Skeletons/characters/gltf";

fn load_unit_model_assets_eager(asset_server: &AssetServer) -> UnitModelAssets {
    let mut scenes = HashMap::new();

    // TTP player units + siege
    let ttp_units: &[(EntityKind, &str)] = &[
        (EntityKind::Worker, "TT_Peasant.glb"),
        (EntityKind::Soldier, "TT_Swordman.glb"),
        (EntityKind::Archer, "TT_Archer.glb"),
        (EntityKind::Tank, "TT_Heavy_Infantry.glb"),
        (EntityKind::Knight, "TT_HeavySwordman.glb"),
        (EntityKind::Mage, "TT_Mage.glb"),
        (EntityKind::Priest, "TT_Priest.glb"),
        (EntityKind::Cavalry, "TT_Light_Cavalry.glb"),
    ];
    for (kind, filename) in ttp_units {
        let handle = asset_server.load(format!("{TTP_UNITS_PATH}/{filename}#Scene0"));
        scenes.insert(*kind, handle);
    }

    // TTP siege machines
    let ttp_machines: &[(EntityKind, &str)] = &[
        (EntityKind::Catapult, "catapult.glb"),
        (EntityKind::BatteringRam, "ram.glb"),
    ];
    for (kind, filename) in ttp_machines {
        let handle = asset_server.load(format!("{TTP_MACHINES_PATH}/{filename}#Scene0"));
        scenes.insert(*kind, handle);
    }

    // Mobs & summons keep KayKit skeleton models
    let mob_mappings: &[(EntityKind, &str)] = &[
        (EntityKind::Goblin, "Skeleton_Rogue.glb"),
        (EntityKind::Skeleton, "Skeleton_Warrior.glb"),
        (EntityKind::Orc, "Skeleton_Minion.glb"),
        (EntityKind::Demon, "Skeleton_Mage.glb"),
        (EntityKind::SkeletonMinion, "Skeleton_Minion.glb"),
    ];
    for (kind, filename) in mob_mappings {
        let handle = asset_server.load(format!("{SKELETONS_PATH}/{filename}#Scene0"));
        scenes.insert(*kind, handle);
    }

    // (kind, scale, y_offset, facing_rotation)
    // calibration y_offset must be the exact negative of blueprint y_offset
    // so the model's feet sit at terrain level
    let calibration_data: &[(EntityKind, f32, f32, f32)] = &[
        // TTP player units
        (EntityKind::Worker, 0.35, -0.8, 0.0),
        (EntityKind::Soldier, 0.35, -0.9, 0.0),
        (EntityKind::Archer, 0.35, -0.75, 0.0),
        (EntityKind::Tank, 0.38, -1.25, 0.0),
        (EntityKind::Knight, 0.38, -1.2, 0.0),
        (EntityKind::Mage, 0.35, -0.8, 0.0),
        (EntityKind::Priest, 0.35, -0.8, 0.0),
        (EntityKind::Cavalry, 0.35, -1.1, 0.0),
        // TTP siege machines
        (EntityKind::Catapult, 0.4, -0.9, 0.0),
        (EntityKind::BatteringRam, 0.4, -0.8, 0.0),
        // KayKit mobs (unchanged)
        (EntityKind::Goblin, 0.28, -0.65, 0.0),
        (EntityKind::Skeleton, 0.3, -0.78, 0.0),
        (EntityKind::Orc, 0.38, -1.05, 0.0),
        (EntityKind::Demon, 0.42, -1.15, 0.0),
        (EntityKind::SkeletonMinion, 0.28, -0.7, 0.0),
    ];
    let calibration: HashMap<_, _> = calibration_data
        .iter()
        .map(|&(kind, scale, y_offset, facing_rotation)| {
            (
                kind,
                CharacterModelCalibration {
                    scale,
                    y_offset,
                    facing_rotation,
                },
            )
        })
        .collect();

    UnitModelAssets {
        scenes,
        calibration,
    }
}

// ── TTP Raw GLTF Handles (for animation extraction) ──

#[derive(Resource)]
pub struct TtpGltfHandles {
    pub units: HashMap<EntityKind, Handle<bevy::gltf::Gltf>>,
}

/// Which animation set a TTP unit uses (determines clip name mapping).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TtpAnimSet {
    Infantry,
    Shield,
    TwoHanded,
    Staff,
    Cavalry,
    Machine,
}

pub fn ttp_anim_set(kind: EntityKind) -> Option<TtpAnimSet> {
    match kind {
        EntityKind::Worker => Some(TtpAnimSet::Infantry),
        EntityKind::Soldier => Some(TtpAnimSet::Shield),
        EntityKind::Archer => Some(TtpAnimSet::Infantry),
        EntityKind::Tank => Some(TtpAnimSet::Shield),
        EntityKind::Knight => Some(TtpAnimSet::TwoHanded),
        EntityKind::Mage => Some(TtpAnimSet::Staff),
        EntityKind::Priest => Some(TtpAnimSet::Staff),
        EntityKind::Cavalry => Some(TtpAnimSet::Cavalry),
        EntityKind::Catapult => Some(TtpAnimSet::Machine),
        EntityKind::BatteringRam => Some(TtpAnimSet::Machine),
        _ => None, // mobs use legacy
    }
}

/// Returns (clip_name, AnimState) pairs for a given animation set.
fn ttp_clip_mapping(anim_set: TtpAnimSet) -> Vec<(&'static str, AnimState)> {
    match anim_set {
        TtpAnimSet::Infantry | TtpAnimSet::Shield | TtpAnimSet::TwoHanded => vec![
            ("idle", AnimState::Idle),
            ("walk", AnimState::Walk),
            ("run", AnimState::Run),
            ("attack_A", AnimState::AttackA),
            ("attack_B", AnimState::AttackB),
            ("damage", AnimState::Damage),
            ("death_A", AnimState::DeathA),
            ("death_B", AnimState::DeathB),
        ],
        TtpAnimSet::Staff => vec![
            ("idle", AnimState::Idle),
            ("walk", AnimState::Walk),
            ("run", AnimState::Run),
            ("attack_A", AnimState::AttackA),
            ("attack_B", AnimState::AttackB),
            ("cast_A", AnimState::CastA),
            ("cast_B", AnimState::CastB),
            ("damage", AnimState::Damage),
            ("death_A", AnimState::DeathA),
            ("death_B", AnimState::DeathB),
        ],
        TtpAnimSet::Cavalry => vec![
            ("idle", AnimState::Idle),
            ("walk", AnimState::Walk),
            ("run", AnimState::Run),
            ("attack", AnimState::AttackA),
            ("damage", AnimState::Damage),
            ("death_A", AnimState::DeathA),
            ("death_B", AnimState::DeathB),
        ],
        TtpAnimSet::Machine => vec![
            ("idle", AnimState::Idle),
            ("move", AnimState::Walk),
            ("attack", AnimState::AttackA),
            ("damage", AnimState::Damage),
            ("death", AnimState::DeathA),
        ],
    }
}

fn load_ttp_gltf_handles(asset_server: &AssetServer) -> TtpGltfHandles {
    let mut units = HashMap::new();

    // Load raw GLTF handles (not #Scene0) so we can access named_animations
    let ttp_units: &[(EntityKind, &str, &str)] = &[
        (EntityKind::Worker, TTP_UNITS_PATH, "TT_Peasant.glb"),
        (EntityKind::Soldier, TTP_UNITS_PATH, "TT_Swordman.glb"),
        (EntityKind::Archer, TTP_UNITS_PATH, "TT_Archer.glb"),
        (EntityKind::Tank, TTP_UNITS_PATH, "TT_Heavy_Infantry.glb"),
        (EntityKind::Knight, TTP_UNITS_PATH, "TT_HeavySwordman.glb"),
        (EntityKind::Mage, TTP_UNITS_PATH, "TT_Mage.glb"),
        (EntityKind::Priest, TTP_UNITS_PATH, "TT_Priest.glb"),
        (EntityKind::Cavalry, TTP_UNITS_PATH, "TT_Light_Cavalry.glb"),
        (EntityKind::Catapult, TTP_MACHINES_PATH, "catapult.glb"),
        (EntityKind::BatteringRam, TTP_MACHINES_PATH, "ram.glb"),
    ];

    for (kind, base_path, filename) in ttp_units {
        let handle: Handle<bevy::gltf::Gltf> =
            asset_server.load(format!("{base_path}/{filename}"));
        units.insert(*kind, handle);
    }

    TtpGltfHandles { units }
}

// ── Animation Assets ──

/// Animation data for a single unit type, built from its GLB's named animations.
pub struct UnitAnimationData {
    pub graph: Handle<AnimationGraph>,
    pub node_indices: HashMap<AnimState, AnimationNodeIndex>,
}

/// Per-unit-type animation graphs.
#[derive(Resource)]
pub struct UnitAnimationRegistry {
    pub data: HashMap<EntityKind, UnitAnimationData>,
    /// Fallback for mobs still using KayKit shared animations.
    pub legacy: Option<UnitAnimationData>,
}

/// Legacy shared animation resource (kept for KayKit mob compatibility).
#[derive(Resource)]
pub struct AnimationAssets {
    pub graph: Handle<AnimationGraph>,
    pub node_indices: HashMap<AnimState, AnimationNodeIndex>,
}

const ANIM_BASE_PATH: &str = "KayKit_Character_Animations/Animations/gltf/Rig_Medium";

fn load_animation_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    // Legacy KayKit animations for mobs
    let idle_clip: Handle<AnimationClip> = asset_server.load(format!(
        "{ANIM_BASE_PATH}/Rig_Medium_General.glb#Animation0"
    ));
    let walk_clip: Handle<AnimationClip> = asset_server.load(format!(
        "{ANIM_BASE_PATH}/Rig_Medium_MovementBasic.glb#Animation0"
    ));
    let attack_clip: Handle<AnimationClip> = asset_server.load(format!(
        "{ANIM_BASE_PATH}/Rig_Medium_CombatMelee.glb#Animation0"
    ));
    let die_clip: Handle<AnimationClip> = asset_server.load(format!(
        "{ANIM_BASE_PATH}/Rig_Medium_General.glb#Animation1"
    ));

    let mut graph = AnimationGraph::new();
    let idle_node = graph.add_clip(idle_clip.clone(), 1.0, graph.root);
    let walk_node = graph.add_clip(walk_clip.clone(), 1.0, graph.root);
    let attack_node = graph.add_clip(attack_clip.clone(), 1.0, graph.root);
    let die_node = graph.add_clip(die_clip.clone(), 1.0, graph.root);

    let graph_handle = graphs.add(graph);

    let mut node_indices = HashMap::new();
    node_indices.insert(AnimState::Idle, idle_node);
    node_indices.insert(AnimState::Walk, walk_node);
    node_indices.insert(AnimState::AttackA, attack_node);
    node_indices.insert(AnimState::DeathA, die_node);

    commands.insert_resource(AnimationAssets {
        graph: graph_handle,
        node_indices,
    });
}

/// Runs once when all TTP GLTFs are loaded. Builds per-unit AnimationGraphs
/// from named_animations and inserts `UnitAnimationRegistry`.
fn extract_ttp_animations(
    mut commands: Commands,
    ttp_handles: Res<TtpGltfHandles>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    existing_registry: Option<Res<UnitAnimationRegistry>>,
    legacy_assets: Option<Res<AnimationAssets>>,
) {
    if existing_registry.is_some() {
        return;
    }

    // Wait until ALL TTP GLTFs are loaded
    for handle in ttp_handles.units.values() {
        if gltf_assets.get(handle).is_none() {
            return;
        }
    }

    let mut data = HashMap::new();

    for (kind, handle) in &ttp_handles.units {
        let gltf = gltf_assets.get(handle).unwrap();
        let anim_set = match ttp_anim_set(*kind) {
            Some(s) => s,
            None => continue,
        };

        let clip_mapping = ttp_clip_mapping(anim_set);
        let mut graph = AnimationGraph::new();
        let mut node_indices = HashMap::new();

        for (clip_name, anim_state) in &clip_mapping {
            if let Some(clip_handle) = gltf.named_animations.get(*clip_name) {
                let node = graph.add_clip(clip_handle.clone(), 1.0, graph.root);
                node_indices.insert(*anim_state, node);
            }
        }

        if node_indices.is_empty() {
            warn!("No animations found for {:?}, named_animations keys: {:?}",
                kind, gltf.named_animations.keys().collect::<Vec<_>>());
            continue;
        }

        let graph_handle = graphs.add(graph);
        data.insert(
            *kind,
            UnitAnimationData {
                graph: graph_handle,
                node_indices,
            },
        );
    }

    // Build legacy fallback from AnimationAssets
    let legacy = legacy_assets.map(|assets| UnitAnimationData {
        graph: assets.graph.clone(),
        node_indices: assets.node_indices.clone(),
    });

    info!(
        "TTP animation registry ready: {} unit types, legacy={}",
        data.len(),
        legacy.is_some()
    );
    commands.insert_resource(UnitAnimationRegistry { data, legacy });
}

// ── Team Color Textures ──

#[derive(Resource)]
pub struct TeamColorTextures {
    pub unit_textures: HashMap<TeamColor, Handle<Image>>,
    pub building_textures: HashMap<TeamColor, Handle<Image>>,
}

#[derive(Resource, Default)]
pub struct TeamColorMaterialCache {
    pub cache: HashMap<(AssetId<StandardMaterial>, TeamColor), Handle<StandardMaterial>>,
}

fn load_team_color_textures(asset_server: &AssetServer) -> TeamColorTextures {
    let color_names: &[(TeamColor, &str)] = &[
        (TeamColor::Blue, "blue"),
        (TeamColor::Red, "red"),
        (TeamColor::Purple, "purple"),
        (TeamColor::Green, "green"),
        (TeamColor::Black, "black"),
    ];

    let mut unit_textures = HashMap::new();
    let mut building_textures = HashMap::new();

    for (color, name) in color_names {
        unit_textures.insert(
            *color,
            asset_server.load(format!("ToonyTinyPeople/textures/units/color/TT_RTS_Units_{name}.png")),
        );
        building_textures.insert(
            *color,
            asset_server.load(format!(
                "ToonyTinyPeople/textures/buildings/color/TT_RTS_Buildings_{name}.png"
            )),
        );
    }

    TeamColorTextures {
        unit_textures,
        building_textures,
    }
}

/// Waits for the grass GLTF to load, then extracts mesh + material handles
/// and inserts `GrassInstanceAssets`. Removes itself after running once.
fn extract_grass_instance_assets(
    mut commands: Commands,
    grass_handle: Option<Res<GrassGltfHandle>>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
    gltf_meshes: Res<Assets<bevy::gltf::GltfMesh>>,
    existing: Option<Res<GrassInstanceAssets>>,
) {
    // Already extracted
    if existing.is_some() {
        return;
    }
    let Some(handle) = grass_handle else { return };
    let Some(gltf) = gltf_assets.get(&handle.0) else {
        return;
    };
    let Some(gltf_mesh_handle) = gltf.meshes.first() else {
        warn!("Grass GLTF has no meshes");
        return;
    };
    let Some(gltf_mesh) = gltf_meshes.get(gltf_mesh_handle) else {
        return;
    };
    let Some(primitive) = gltf_mesh.primitives.first() else {
        warn!("Grass GLTF mesh has no primitives");
        return;
    };

    commands.insert_resource(GrassInstanceAssets {
        mesh: primitive.mesh.clone(),
        material: primitive.material.clone().unwrap_or_default(),
    });
    info!("Extracted grass instance assets for dense rendering");
}

// ── Team Color Texture Application ──

use crate::components::{
    BuildingSceneChild, Faction, FactionColors, TeamColorApplied, UnitSceneChild,
};

/// Applies team-color textures to TTP scene meshes after they're instantiated.
/// Walks the entity hierarchy to find StandardMaterial meshes and swaps the base_color_texture.
fn apply_team_color_textures(
    mut commands: Commands,
    team_textures: Option<Res<TeamColorTextures>>,
    faction_colors: Res<FactionColors>,
    mut mat_cache: ResMut<TeamColorMaterialCache>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    // Scene children that need coloring (unit or building), where parent lacks TeamColorApplied
    scene_children: Query<
        (Entity, &ChildOf),
        (
            Or<(With<UnitSceneChild>, With<BuildingSceneChild>)>,
            Without<TeamColorApplied>,
        ),
    >,
    faction_q: Query<&Faction>,
    already_colored: Query<(), With<TeamColorApplied>>,
    children_q: Query<&Children>,
    mesh_mat_q: Query<&MeshMaterial3d<StandardMaterial>>,
    is_unit_scene: Query<(), With<UnitSceneChild>>,
) {
    let Some(ref textures) = team_textures else {
        return;
    };

    for (scene_entity, child_of) in &scene_children {
        let parent = child_of.parent();

        // Skip if parent already has TeamColorApplied
        if already_colored.contains(parent) {
            continue;
        }

        let Ok(faction) = faction_q.get(parent) else {
            continue;
        };

        let team_color = faction_colors.get(faction);

        // Determine if this is a unit or building scene for texture selection
        let is_unit = is_unit_scene.contains(scene_entity);
        let texture = if is_unit {
            textures.unit_textures.get(&team_color)
        } else {
            textures.building_textures.get(&team_color)
        };

        let Some(target_texture) = texture else {
            continue;
        };

        // Walk hierarchy to find all mesh material entities
        let mut found_any = false;
        apply_color_recursive(
            scene_entity,
            target_texture,
            team_color,
            &children_q,
            &mesh_mat_q,
            &mut materials,
            &mut mat_cache,
            &mut commands,
            &mut found_any,
        );

        if found_any {
            commands.entity(parent).insert(TeamColorApplied);
        }
    }
}

fn apply_color_recursive(
    entity: Entity,
    target_texture: &Handle<Image>,
    team_color: TeamColor,
    children_q: &Query<&Children>,
    mesh_mat_q: &Query<&MeshMaterial3d<StandardMaterial>>,
    materials: &mut Assets<StandardMaterial>,
    cache: &mut TeamColorMaterialCache,
    commands: &mut Commands,
    found_any: &mut bool,
) {
    if let Ok(mat_handle) = mesh_mat_q.get(entity) {
        let mat_id = mat_handle.0.id();
        let cache_key = (mat_id, team_color);

        let new_handle = if let Some(cached) = cache.cache.get(&cache_key) {
            cached.clone()
        } else if let Some(original_mat) = materials.get(mat_id) {
            let mut cloned = original_mat.clone();
            cloned.base_color_texture = Some(target_texture.clone());
            let new_handle = materials.add(cloned);
            cache.cache.insert(cache_key, new_handle.clone());
            new_handle
        } else {
            return;
        };

        commands
            .entity(entity)
            .insert(MeshMaterial3d(new_handle));
        *found_any = true;
    }

    if let Ok(children) = children_q.get(entity) {
        for child in children.iter() {
            apply_color_recursive(
                child,
                target_texture,
                team_color,
                children_q,
                mesh_mat_q,
                materials,
                cache,
                commands,
                found_any,
            );
        }
    }
}
