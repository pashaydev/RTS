use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::EntityKind;
use crate::components::{AnimState, IconAssets, ModelAssets};

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

        // Load building GLTF model assets eagerly so they're available to Startup systems
        let building_models = load_building_model_assets_eager(&asset_server);
        app.insert_resource(building_models);

        // Load unit/mob character model assets eagerly
        let unit_models = load_unit_model_assets_eager(&asset_server);
        app.insert_resource(unit_models);

        app.add_systems(Startup, (load_model_assets, load_animation_assets));
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

    commands.insert_resource(ModelAssets {
        trees,
        dead_trees,
        rocks,
        bushes,
        grass,
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
    pub scenes: HashMap<(EntityKind, u8), Handle<Scene>>,
    pub calibration: HashMap<EntityKind, BuildingModelCalibration>,
}

const BUILDING_BASE_PATH: &str = "UltimateFantasyRTS/glTF";

fn load_building_model_assets_eager(asset_server: &AssetServer) -> BuildingModelAssets {
    let mut scenes = HashMap::new();

    let mappings: &[(EntityKind, &[&str; 3])] = &[
        (EntityKind::Base, &[
            "TownCenter_FirstAge_Level1",
            "TownCenter_FirstAge_Level2",
            "TownCenter_FirstAge_Level3",
        ]),
        (EntityKind::Barracks, &[
            "Barracks_FirstAge_Level1",
            "Barracks_FirstAge_Level2",
            "Barracks_FirstAge_Level3",
        ]),
        (EntityKind::Workshop, &[
            "Market_FirstAge_Level1",
            "Market_FirstAge_Level2",
            "Market_FirstAge_Level3",
        ]),
        (EntityKind::Tower, &[
            "WatchTower_FirstAge_Level1",
            "WatchTower_FirstAge_Level2",
            "WatchTower_FirstAge_Level3",
        ]),
        (EntityKind::Storage, &[
            "Storage_FirstAge_Level1",
            "Storage_FirstAge_Level2",
            "Storage_FirstAge_Leve3", // typo in asset filename
        ]),
        (EntityKind::MageTower, &[
            "Wonder_FirstAge_Level1",
            "Wonder_FirstAge_Level2",
            "Wonder_FirstAge_Level3",
        ]),
        (EntityKind::Temple, &[
            "Temple_FirstAge_Level1",
            "Temple_FirstAge_Level2",
            "Temple_FirstAge_Level3",
        ]),
        (EntityKind::Stable, &[
            "Houses_FirstAge_1_Level1",
            "Houses_FirstAge_1_Level2",
            "Houses_FirstAge_1_Level3",
        ]),
        (EntityKind::SiegeWorks, &[
            "Archery_FirstAge_Level1",
            "Archery_FirstAge_Level2",
            "Archery_FirstAge_Level3",
        ]),
    ];

    for (kind, names) in mappings {
        for (level_idx, name) in names.iter().enumerate() {
            let level = (level_idx + 1) as u8;
            let handle = asset_server.load(format!("{BUILDING_BASE_PATH}/{name}.gltf#Scene0"));
            scenes.insert((*kind, level), handle);
        }
    }

    let mut calibration = HashMap::new();
    calibration.insert(EntityKind::Base, BuildingModelCalibration {
        scale: 3.0, y_offset: 0.0, building_height: 4.0,
    });
    calibration.insert(EntityKind::Barracks, BuildingModelCalibration {
        scale: 3.0, y_offset: 0.0, building_height: 7.0,
    });
    calibration.insert(EntityKind::Workshop, BuildingModelCalibration {
        scale: 3.0, y_offset: 0.0, building_height: 7.0,
    });
    calibration.insert(EntityKind::Tower, BuildingModelCalibration {
        scale: 3.0, y_offset: 0.0, building_height: 12.0,
    });
    calibration.insert(EntityKind::Storage, BuildingModelCalibration {
        scale: 3.0, y_offset: 0.0, building_height: 5.0,
    });
    calibration.insert(EntityKind::MageTower, BuildingModelCalibration {
        scale: 3.0, y_offset: 0.0, building_height: 10.0,
    });
    calibration.insert(EntityKind::Temple, BuildingModelCalibration {
        scale: 3.0, y_offset: 0.0, building_height: 8.0,
    });
    calibration.insert(EntityKind::Stable, BuildingModelCalibration {
        scale: 3.0, y_offset: 0.0, building_height: 7.0,
    });
    calibration.insert(EntityKind::SiegeWorks, BuildingModelCalibration {
        scale: 3.0, y_offset: 0.0, building_height: 7.0,
    });

    BuildingModelAssets { scenes, calibration }
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

const ADVENTURERS_PATH: &str = "KayKit_Adventurers/Characters/gltf";
const SKELETONS_PATH: &str = "KayKit_Skeletons/characters/gltf";

fn load_unit_model_assets_eager(asset_server: &AssetServer) -> UnitModelAssets {
    let mut scenes = HashMap::new();

    // Player units
    let unit_mappings: &[(EntityKind, &str, &str)] = &[
        (EntityKind::Worker,    ADVENTURERS_PATH, "Barbarian.glb"),
        (EntityKind::Soldier,   ADVENTURERS_PATH, "Knight.glb"),
        (EntityKind::Archer,    ADVENTURERS_PATH, "Ranger.glb"),
        (EntityKind::Tank,      ADVENTURERS_PATH, "Barbarian.glb"),
        (EntityKind::Knight,    ADVENTURERS_PATH, "Knight.glb"),
        (EntityKind::Mage,      ADVENTURERS_PATH, "Mage.glb"),
        (EntityKind::Priest,    ADVENTURERS_PATH, "Rogue_Hooded.glb"),
        (EntityKind::Cavalry,   ADVENTURERS_PATH, "Rogue.glb"),
        // Mobs
        (EntityKind::Goblin,    SKELETONS_PATH, "Skeleton_Rogue.glb"),
        (EntityKind::Skeleton,  SKELETONS_PATH, "Skeleton_Warrior.glb"),
        (EntityKind::Orc,       SKELETONS_PATH, "Skeleton_Minion.glb"),
        (EntityKind::Demon,     SKELETONS_PATH, "Skeleton_Mage.glb"),
        // Summons
        (EntityKind::SkeletonMinion, SKELETONS_PATH, "Skeleton_Minion.glb"),
    ];

    for (kind, base_path, filename) in unit_mappings {
        let handle = asset_server.load(format!("{base_path}/{filename}#Scene0"));
        scenes.insert(*kind, handle);
    }

    let mut calibration = HashMap::new();

    // Player units
    calibration.insert(EntityKind::Worker, CharacterModelCalibration {
        scale: 0.3, y_offset: -0.8, facing_rotation: 0.0,
    });
    calibration.insert(EntityKind::Soldier, CharacterModelCalibration {
        scale: 0.35, y_offset: -0.9, facing_rotation: 0.0,
    });
    calibration.insert(EntityKind::Archer, CharacterModelCalibration {
        scale: 0.3, y_offset: -0.75, facing_rotation: 0.0,
    });
    calibration.insert(EntityKind::Tank, CharacterModelCalibration {
        scale: 0.42, y_offset: -1.25, facing_rotation: 0.0,
    });
    calibration.insert(EntityKind::Knight, CharacterModelCalibration {
        scale: 0.4, y_offset: -1.2, facing_rotation: 0.0,
    });
    calibration.insert(EntityKind::Mage, CharacterModelCalibration {
        scale: 0.3, y_offset: -0.8, facing_rotation: 0.0,
    });
    calibration.insert(EntityKind::Priest, CharacterModelCalibration {
        scale: 0.3, y_offset: -0.8, facing_rotation: 0.0,
    });
    calibration.insert(EntityKind::Cavalry, CharacterModelCalibration {
        scale: 0.35, y_offset: -1.1, facing_rotation: 0.0,
    });
    // Mobs
    calibration.insert(EntityKind::Goblin, CharacterModelCalibration {
        scale: 0.28, y_offset: -0.65, facing_rotation: 0.0,
    });
    calibration.insert(EntityKind::Skeleton, CharacterModelCalibration {
        scale: 0.3, y_offset: -0.78, facing_rotation: 0.0,
    });
    calibration.insert(EntityKind::Orc, CharacterModelCalibration {
        scale: 0.38, y_offset: -1.05, facing_rotation: 0.0,
    });
    calibration.insert(EntityKind::Demon, CharacterModelCalibration {
        scale: 0.42, y_offset: -1.15, facing_rotation: 0.0,
    });
    // Summons
    calibration.insert(EntityKind::SkeletonMinion, CharacterModelCalibration {
        scale: 0.28, y_offset: -0.7, facing_rotation: 0.0,
    });

    UnitModelAssets { scenes, calibration }
}

// ── Animation Assets ──

const ANIM_BASE_PATH: &str = "KayKit_Character_Animations/Animations/gltf/Rig_Medium";

#[derive(Resource)]
pub struct AnimationAssets {
    pub clips: HashMap<AnimState, Vec<Handle<AnimationClip>>>,
    pub graph: Handle<AnimationGraph>,
    pub node_indices: HashMap<AnimState, AnimationNodeIndex>,
}

fn load_animation_assets(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    // Load specific animation clips from the GLB files
    // Each GLB contains multiple animations; we load by label
    let idle_clip: Handle<AnimationClip> = asset_server.load(format!("{ANIM_BASE_PATH}/Rig_Medium_General.glb#Animation0"));
    let walk_clip: Handle<AnimationClip> = asset_server.load(format!("{ANIM_BASE_PATH}/Rig_Medium_MovementBasic.glb#Animation0"));
    let attack_clip: Handle<AnimationClip> = asset_server.load(format!("{ANIM_BASE_PATH}/Rig_Medium_CombatMelee.glb#Animation0"));
    let die_clip: Handle<AnimationClip> = asset_server.load(format!("{ANIM_BASE_PATH}/Rig_Medium_General.glb#Animation1"));

    let mut graph = AnimationGraph::new();
    let idle_node = graph.add_clip(idle_clip.clone(), 1.0, graph.root);
    let walk_node = graph.add_clip(walk_clip.clone(), 1.0, graph.root);
    let attack_node = graph.add_clip(attack_clip.clone(), 1.0, graph.root);
    let die_node = graph.add_clip(die_clip.clone(), 1.0, graph.root);

    let graph_handle = graphs.add(graph);

    let mut clips = HashMap::new();
    clips.insert(AnimState::Idle, vec![idle_clip]);
    clips.insert(AnimState::Walk, vec![walk_clip]);
    clips.insert(AnimState::Attack, vec![attack_clip]);
    clips.insert(AnimState::Die, vec![die_clip]);

    let mut node_indices = HashMap::new();
    node_indices.insert(AnimState::Idle, idle_node);
    node_indices.insert(AnimState::Walk, walk_node);
    node_indices.insert(AnimState::Attack, attack_node);
    node_indices.insert(AnimState::Die, die_node);

    commands.insert_resource(AnimationAssets {
        clips,
        graph: graph_handle,
        node_indices,
    });
}
