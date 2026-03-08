use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::EntityKind;
use crate::components::{IconAssets, ModelAssets};

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
            goblin: asset_server.load("icons/mobs/goblin.png"),
            skeleton: asset_server.load("icons/mobs/skeleton.png"),
            orc: asset_server.load("icons/mobs/orc.png"),
            demon: asset_server.load("icons/mobs/demon.png"),
        };
        app.insert_resource(icons);

        // Load building GLTF model assets eagerly so they're available to Startup systems
        let building_models = load_building_model_assets_eager(&asset_server);
        app.insert_resource(building_models);

        app.add_systems(Startup, load_model_assets);
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
        scale: 3.0, y_offset: 0.0, building_height: 8.0,
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
