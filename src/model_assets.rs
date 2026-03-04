use bevy::prelude::*;

use crate::components::ModelAssets;

pub struct ModelAssetsPlugin;

impl Plugin for ModelAssetsPlugin {
    fn build(&self, app: &mut App) {
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
