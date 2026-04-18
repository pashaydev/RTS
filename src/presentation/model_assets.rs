use bevy::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::blueprints::EntityKind;
use crate::presentation::materials::grass::{GrassExtension, GrassMaterial};
use crate::presentation::materials::tree_occlusion::{
    TreeOcclusionMaterial, TreeOcclusionMaterialCache, TreeOcclusionUniform,
};
use crate::types::{
    AnimState, AppState, AttentionIconAssets, DayCycleIconAssets, DecoGltfHandles,
    DecorationInstanceAssets, GrassInstanceAssets, GrassRenderSettings, IconAssets, LodTier,
    ModelAssets, TeamColor,
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
            stone: asset_server.load("icons/resources/stone.png"),
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
            house: asset_server.load("icons/buildings/house.png"),
            floor: asset_server.load("icons/buildings/floor.png"),
            sawmill: asset_server.load("icons/buildings/sawmill.png"),
            wall: asset_server.load("icons/buildings/wall.png"),
            outpost: asset_server.load("icons/buildings/outpost.png"),
            gatehouse: asset_server.load("icons/buildings/gatehouse.png"),
            guard_tower: asset_server.load("icons/buildings/guard_tower.png"),
            ballista_tower: asset_server.load("icons/buildings/ballista_tower.png"),
            bombard_tower: asset_server.load("icons/buildings/bombard_tower.png"),
            mine: asset_server.load("icons/buildings/mine.png"),
            oil_rig: asset_server.load("icons/buildings/oil_rig.png"),
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

        // Load day cycle phase icons (CC BY 3.0, game-icons.net)
        let daycycle_icons = DayCycleIconAssets {
            dawn: asset_server.load("icons/daycycle/dawn.png"),
            day: asset_server.load("icons/daycycle/day.png"),
            dusk: asset_server.load("icons/daycycle/dusk.png"),
            night: asset_server.load("icons/daycycle/night.png"),
        };
        app.insert_resource(daycycle_icons);

        // Load building GLTF model assets eagerly so they're available to Startup systems
        let building_models = load_building_model_assets_eager(&asset_server);
        app.insert_resource(building_models);

        // Load building construction stage assets
        let construction_assets = load_building_construction_assets(&asset_server);
        app.insert_resource(construction_assets);

        // Load unit/mob character model assets eagerly
        let unit_models = load_unit_model_assets_eager(&asset_server);
        app.insert_resource(unit_models);

        // Load projectile model assets
        let projectile_models = load_projectile_model_assets(&asset_server);
        app.insert_resource(projectile_models);

        // Load TTP raw GLTF handles for animation extraction
        let ttp_gltf_handles = load_ttp_gltf_handles(&asset_server);
        app.insert_resource(ttp_gltf_handles);

        // Load team color textures
        let team_colors = load_team_color_textures(&asset_server);
        app.insert_resource(team_colors);
        app.init_resource::<TeamColorMaterialCache>();

        app.init_resource::<TreeOcclusionMaterialCache>()
            .init_resource::<TreeOcclusionUniform>()
            .add_plugins((
                bevy::pbr::MaterialPlugin::<GrassMaterial>::default(),
                bevy::pbr::MaterialPlugin::<TreeOcclusionMaterial>::default(),
            ))
            .add_systems(Startup, (load_model_assets, load_animation_assets))
            .add_systems(Startup, create_grass_instance_assets)
            .add_systems(Update, update_grass_time.run_if(in_state(AppState::InGame)))
            .add_systems(
                Update,
                (
                    extract_decoration_instance_assets,
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

fn load_scenes_from(asset_server: &AssetServer, base: &str, names: &[&str]) -> Vec<Handle<Scene>> {
    names
        .iter()
        .map(|name| asset_server.load(format!("{base}/{name}#Scene0")))
        .collect()
}

fn load_gltf_handles(asset_server: &AssetServer, names: &[&str]) -> Vec<Handle<bevy::gltf::Gltf>> {
    names
        .iter()
        .map(|name| asset_server.load(format!("{BASE_PATH}/{name}")))
        .collect()
}

fn load_gltf_handles_from(
    asset_server: &AssetServer,
    paths: &[&str],
) -> Vec<Handle<bevy::gltf::Gltf>> {
    paths
        .iter()
        .map(|path| asset_server.load(path.to_string()))
        .collect()
}

const NEW_TREE_PATH: &str = "trees_compressed";
const NEW_TREE_SCALE: f32 = 0.4;

fn create_grass_instance_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GrassMaterial>>,
    render_settings: Res<GrassRenderSettings>,
) {
    let mesh = meshes.add(build_low_poly_grass_mesh());
    let material = materials.add(GrassMaterial {
        base: StandardMaterial {
            perceptual_roughness: 0.9,
            metallic: 0.0,
            reflectance: 0.1,
            cull_mode: None,
            double_sided: true,
            ..default()
        },
        extension: GrassExtension {
            settings: crate::presentation::materials::grass::GrassSettings::from_render_settings(
                &render_settings,
                0.0,
            ),
        },
    });
    commands.insert_resource(GrassInstanceAssets { mesh, material });
}

fn update_grass_time(
    time: Res<Time>,
    grass_assets: Option<Res<GrassInstanceAssets>>,
    render_settings: Res<GrassRenderSettings>,
    mut materials: ResMut<Assets<GrassMaterial>>,
) {
    if let Some(assets) = grass_assets {
        if let Some(mat) = materials.get_mut(&assets.material) {
            mat.extension
                .settings
                .apply_render_settings(&render_settings, time.elapsed_secs());
        }
    }
}

fn build_low_poly_grass_mesh() -> Mesh {
    use bevy::mesh::Indices;

    // Single triangle blade — shader reconstructs full geometry from UVs + per-blade data
    // V0: bottom-left, V1: bottom-right, V2: tip
    let positions = vec![
        [-0.03_f32, 0.0, 0.0], // bottom-left (root)
        [0.03, 0.0, 0.0],      // bottom-right (root)
        [0.0, 0.5, 0.0],       // tip
    ];
    let normals = vec![[0.0_f32, 1.0, 0.0]; 3]; // placeholder — shader computes real normals
    let uvs = vec![
        [0.0_f32, 0.0], // left root
        [1.0, 0.0],     // right root
        [0.5, 1.0],     // center tip
    ];
    // Placeholder UV_1 — vertex shader writes actual values via out.uv_b
    let uv1s = vec![[0.0_f32, 0.0]; 3];
    let indices = vec![0_u32, 2, 1];

    let mut mesh = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv1s);
    // No ATTRIBUTE_COLOR — merge function packs per-blade data into colors instead
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn load_model_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    let trees = load_scenes_from(
        &asset_server,
        NEW_TREE_PATH,
        &[
            "tree (1).glb",
            "tree (2).glb",
            "tree (3).glb",
            "tree (4).glb",
            "tree (7).glb",
            "tree (8).glb",
        ],
    );
    let tree_base_scales = vec![NEW_TREE_SCALE; trees.len()];

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

    let bushes = load_scenes_from(
        &asset_server,
        NEW_TREE_PATH,
        &["bush (1).glb", "bush (2).glb"],
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
        tree_base_scales,
        dead_trees,
        rocks,
        bushes,
        grass,
        mountains,
    });

    // Load GLTF handles for decoration chunk instancing (mesh/material extraction).
    // These are the same files as above but loaded as Gltf (not Scene) so we can
    // access mesh primitives for CPU vertex merging.
    let deco_bush_gltfs = load_gltf_handles_from(
        &asset_server,
        &[
            "trees_compressed/bush (1).glb",
            "trees_compressed/bush (2).glb",
        ],
    );
    let deco_rock_gltfs = load_gltf_handles(
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
    let deco_grass_gltfs = load_gltf_handles(
        &asset_server,
        &[
            "Grass_1_A_Color1.gltf",
            "Grass_1_B_Color1.gltf",
            "Grass_1_C_Color1.gltf",
            "Grass_1_D_Color1.gltf",
        ],
    );
    let deco_mountain_gltfs = load_gltf_handles_from(
        &asset_server,
        &[
            "UltimateFantasyRTS/glTF/Mountain_Group_1.gltf",
            "UltimateFantasyRTS/glTF/Mountain_Group_2.gltf",
            "UltimateFantasyRTS/glTF/MountainLarge_Single.gltf",
            "UltimateFantasyRTS/glTF/Mountain_Single.gltf",
        ],
    );
    commands.insert_resource(DecoGltfHandles {
        bushes: deco_bush_gltfs,
        rocks: deco_rock_gltfs,
        grass: deco_grass_gltfs,
        mountains: deco_mountain_gltfs,
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

    pub fn child_transform(&self, kind: EntityKind, fallback_scale: f32) -> Transform {
        let cal = self.calibration.get(&kind);
        let scale = cal.map(|c| c.scale).unwrap_or(fallback_scale);
        let y_offset = cal.map(|c| c.y_offset).unwrap_or(0.0);
        Transform::from_scale(Vec3::splat(scale)).with_translation(Vec3::new(0.0, y_offset, 0.0))
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
        EntityKind::WallCorner => "Wall_A_corner",
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
        EntityKind::WallCorner,
    ];

    // TTP buildings have no level variants — same model for L1/L2/L3
    for kind in building_kinds {
        let glb = ttp_building_glb(kind);
        let handle = asset_server.load(format!("{TTP_BUILDINGS_PATH}/{glb}.glb#Scene0"));
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
        (EntityKind::WallCorner, 1.0, 0.0, 4.0),
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
        EntityKind::WallCorner,
    ];

    for kind in building_kinds {
        let glb = ttp_building_glb(kind);
        for stage in 0..=1u8 {
            let handle =
                asset_server.load(format!("{TTP_CONSTRUCTION_PATH}/{glb}_{stage}.glb#Scene0"));
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
    /// Per-LOD scene handles. Every kind has a `LodTier::High` entry (mirror
    /// of `scenes`). Additional tiers are populated only when a distinct
    /// asset is baked — the swap system skips kinds that lack the requested
    /// tier, so this resource can be extended without touching the swap
    /// logic.
    pub lod_scenes: HashMap<(EntityKind, LodTier), Handle<Scene>>,
}

const TTP_UNITS_PATH: &str = "ToonyTinyPeople/models/units";
const TTP_MACHINES_PATH: &str = "ToonyTinyPeople/models/units/machines";
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
        (EntityKind::Scout, "TT_Crossbowman.glb"),
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

    // Summons reuse spare animated TTP characters (mobs now use procedural cubes)
    let mob_mappings: &[(EntityKind, &str)] =
        &[(EntityKind::SkeletonMinion, "TT_Light_Infantry.glb")];
    for (kind, filename) in mob_mappings {
        let handle = asset_server.load(format!("{TTP_UNITS_PATH}/{filename}#Scene0"));
        scenes.insert(*kind, handle);
    }

    // Goblin mob (standalone GLB model — currently using KayKit Orc rig)
    let goblin_scene = asset_server.load("models/Orc.glb#Scene0");
    scenes.insert(EntityKind::Goblin, goblin_scene);

    // (kind, scale, y_offset, facing_rotation)
    // calibration y_offset must be the exact negative of blueprint y_offset
    // so the model's feet sit at terrain level
    let calibration_data: &[(EntityKind, f32, f32, f32)] = &[
        // TTP player units
        (EntityKind::Worker, 0.45, -0.8, 0.0),
        (EntityKind::Soldier, 0.45, -0.9, 0.0),
        (EntityKind::Archer, 0.45, -0.75, 0.0),
        (EntityKind::Tank, 0.48, -1.25, 0.0),
        (EntityKind::Knight, 0.48, -1.2, 0.0),
        (EntityKind::Mage, 0.45, -0.8, 0.0),
        (EntityKind::Priest, 0.45, -0.8, 0.0),
        (EntityKind::Cavalry, 0.45, -1.1, 0.0),
        (EntityKind::Scout, 0.42, -0.7, 0.0),
        // TTP siege machines
        (EntityKind::Catapult, 0.4, -0.9, 0.0),
        (EntityKind::BatteringRam, 0.4, -0.8, 0.0),
        // Summons on TTP rigs
        (EntityKind::SkeletonMinion, 0.4, -0.7, 0.0),
        // Goblin mob (standalone GLB; KayKit Orc rig)
        // Evaluated world height ≈ 2.97u with feet at y=-0.17u; pick scale
        // 1.15 so rendered height is ~3.4u (matches old goblin silhouette),
        // then y_offset cancels blueprint y_offset (0.8) + scaled feet
        // (-0.17 × 1.15 ≈ -0.20) so feet land at terrain level.
        (EntityKind::Goblin, 1.15, -0.6, 0.0),
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

    // Seed per-LOD table with High = base scene for every kind. The swap
    // system falls back to the High tier when no higher-distance entry
    // (currently only `LodTier::Impostor` for the goblin) is registered.
    let mut lod_scenes: HashMap<(EntityKind, LodTier), Handle<Scene>> = HashMap::new();
    for (kind, handle) in scenes.iter() {
        lod_scenes.insert((*kind, LodTier::High), handle.clone());
    }

    UnitModelAssets {
        scenes,
        calibration,
        lod_scenes,
    }
}

// ── Projectile Model Assets ──

/// Which visual model a projectile should use.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjectileVisualKind {
    Arrow,
    Bolt,
    CatapultRock,
}

#[derive(Resource)]
pub struct ProjectileModelAssets {
    pub arrows: Vec<Handle<Scene>>,
    pub bolts: Vec<Handle<Scene>>,
    pub catapult_rocks: Vec<Handle<Scene>>,
}

impl ProjectileModelAssets {
    /// Pick the scene handle for a given visual kind, cycling through variants.
    pub fn scene_for(&self, kind: ProjectileVisualKind, variant: usize) -> Handle<Scene> {
        match kind {
            ProjectileVisualKind::Arrow => self.arrows[variant % self.arrows.len()].clone(),
            ProjectileVisualKind::Bolt => self.bolts[variant % self.bolts.len()].clone(),
            ProjectileVisualKind::CatapultRock => {
                self.catapult_rocks[variant % self.catapult_rocks.len()].clone()
            }
        }
    }
}

/// Map an EntityKind to its projectile visual (if it has a 3D model projectile).
pub fn projectile_visual_for(kind: EntityKind) -> Option<ProjectileVisualKind> {
    match kind {
        EntityKind::Archer | EntityKind::Scout => Some(ProjectileVisualKind::Arrow),
        EntityKind::Tower | EntityKind::WatchTower | EntityKind::GuardTower => {
            Some(ProjectileVisualKind::Arrow)
        }
        EntityKind::BallistaTower => Some(ProjectileVisualKind::Bolt),
        EntityKind::Catapult | EntityKind::BombardTower => Some(ProjectileVisualKind::CatapultRock),
        _ => None, // Mage, Priest, etc. keep sphere VFX
    }
}

const TTP_PROJECTILES_PATH: &str = "ToonyTinyPeople/models/extras/projectiles";

fn load_projectile_model_assets(asset_server: &AssetServer) -> ProjectileModelAssets {
    let arrows = ["Arrow_A.glb", "Arrow_B.glb"]
        .iter()
        .map(|f| asset_server.load(format!("{TTP_PROJECTILES_PATH}/{f}#Scene0")))
        .collect();

    let bolts = ["Bolt_lvl1.glb", "Bolt_lvl2.glb", "Bolt_lvl3.glb"]
        .iter()
        .map(|f| asset_server.load(format!("{TTP_PROJECTILES_PATH}/{f}#Scene0")))
        .collect();

    let catapult_rocks = [
        "Catapult_rock_lvl1.glb",
        "Catapult_rock_lvl2.glb",
        "Catapult_rock_lvl3.glb",
    ]
    .iter()
    .map(|f| asset_server.load(format!("{TTP_PROJECTILES_PATH}/{f}#Scene0")))
    .collect();

    ProjectileModelAssets {
        arrows,
        bolts,
        catapult_rocks,
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
    GoblinMob,
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
        EntityKind::Scout => Some(TtpAnimSet::Infantry),
        EntityKind::Catapult => Some(TtpAnimSet::Machine),
        EntityKind::BatteringRam => Some(TtpAnimSet::Machine),
        EntityKind::SkeletonMinion => Some(TtpAnimSet::Infantry),
        EntityKind::Goblin => Some(TtpAnimSet::GoblinMob),
        _ => None,
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
        TtpAnimSet::GoblinMob => vec![
            ("CharacterArmature|Idle", AnimState::Idle),
            ("CharacterArmature|Walk", AnimState::Walk),
            ("CharacterArmature|Run", AnimState::Run),
            ("CharacterArmature|Punch", AnimState::AttackA),
            ("CharacterArmature|Weapon", AnimState::AttackB),
            ("CharacterArmature|HitReact", AnimState::Damage),
            ("CharacterArmature|Death", AnimState::DeathA),
            ("CharacterArmature|Death", AnimState::DeathB),
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
        (EntityKind::Scout, TTP_UNITS_PATH, "TT_Crossbowman.glb"),
        (EntityKind::Catapult, TTP_MACHINES_PATH, "catapult.glb"),
        (EntityKind::BatteringRam, TTP_MACHINES_PATH, "ram.glb"),
        (
            EntityKind::SkeletonMinion,
            TTP_UNITS_PATH,
            "TT_Light_Infantry.glb",
        ),
    ];

    for (kind, base_path, filename) in ttp_units {
        let handle: Handle<bevy::gltf::Gltf> = asset_server.load(format!("{base_path}/{filename}"));
        units.insert(*kind, handle);
    }

    // Goblin mob (standalone GLB — KayKit Orc rig)
    let goblin_gltf: Handle<bevy::gltf::Gltf> = asset_server.load("models/Orc.glb");
    units.insert(EntityKind::Goblin, goblin_gltf);

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
    asset_server: Res<AssetServer>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    existing_registry: Option<Res<UnitAnimationRegistry>>,
    legacy_assets: Option<Res<AnimationAssets>>,
) {
    if existing_registry.is_some() {
        return;
    }

    // Wait until all TTP GLTFs are loaded or failed.
    // Skip assets that failed to load so one missing file doesn't block
    // the entire animation registry.
    let mut failed_kinds = Vec::new();
    for (kind, handle) in &ttp_handles.units {
        match asset_server.get_load_state(handle.id()) {
            Some(bevy::asset::LoadState::Failed(_)) => {
                failed_kinds.push(*kind);
            }
            Some(bevy::asset::LoadState::Loaded) => {}
            _ => return, // still loading, try next frame
        }
    }
    for kind in &failed_kinds {
        warn!("Failed to load GLTF for {:?}, skipping animations", kind);
    }

    let mut data = HashMap::new();

    for (kind, handle) in &ttp_handles.units {
        let Some(gltf) = gltf_assets.get(handle) else {
            continue; // failed to load
        };
        let anim_set = match ttp_anim_set(*kind) {
            Some(s) => s,
            None => continue,
        };

        let clip_mapping = ttp_clip_mapping(anim_set);
        let mut graph = AnimationGraph::new();
        let mut node_indices = HashMap::new();

        let mut missing_clips: Vec<&str> = Vec::new();
        for (clip_name, anim_state) in &clip_mapping {
            if let Some(clip_handle) = gltf.named_animations.get(*clip_name) {
                let node = graph.add_clip(clip_handle.clone(), 1.0, graph.root);
                node_indices.insert(*anim_state, node);
            } else {
                missing_clips.push(clip_name);
            }
        }

        if !missing_clips.is_empty() {
            warn!(
                "{:?}: missing animation clips {:?} (available: {:?})",
                kind,
                missing_clips,
                gltf.named_animations.keys().collect::<Vec<_>>()
            );
        }

        if node_indices.is_empty() {
            warn!(
                "No animations found for {:?} — unit will have no animations!",
                kind,
            );
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

    for (kind, anim_data) in &data {
        let states: Vec<_> = anim_data.node_indices.keys().collect();
        info!("  {:?}: {} animations {:?}", kind, states.len(), states);
    }
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
            asset_server.load(format!(
                "ToonyTinyPeople/textures/units/color/TT_RTS_Units_{name}.png"
            )),
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

/// Extracts mesh + material from decoration GLTF handles for chunk instancing.
/// Runs every frame until all handles are loaded, then inserts `DecorationInstanceAssets`.
fn extract_decoration_instance_assets(
    mut commands: Commands,
    handles: Option<Res<DecoGltfHandles>>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
    gltf_meshes: Res<Assets<bevy::gltf::GltfMesh>>,
    existing: Option<Res<DecorationInstanceAssets>>,
) {
    if existing.is_some() {
        return;
    }
    let Some(handles) = handles else { return };

    // Try to extract all categories. If any handle isn't loaded yet, bail and retry next frame.
    let Some((bushes, bush_model_sizes)) =
        extract_gltf_all_primitives(&handles.bushes, &gltf_assets, &gltf_meshes)
    else {
        return;
    };
    let Some(rocks) = extract_gltf_primitives(&handles.rocks, &gltf_assets, &gltf_meshes) else {
        return;
    };
    let Some(grass) = extract_gltf_primitives(&handles.grass, &gltf_assets, &gltf_meshes) else {
        return;
    };
    let Some(mountains) = extract_gltf_primitives(&handles.mountains, &gltf_assets, &gltf_meshes)
    else {
        return;
    };

    info!(
        "Extracted decoration instance assets: {} bushes, {} rocks, {} grass, {} mountains",
        bushes.len(),
        rocks.len(),
        grass.len(),
        mountains.len()
    );
    commands.insert_resource(DecorationInstanceAssets {
        bushes,
        bush_model_sizes,
        rocks,
        grass,
        _mountains: mountains,
    });
}

/// Extract (mesh, material) from each GLTF handle's first mesh primitive.
/// Returns None if any handle isn't loaded yet.
fn extract_gltf_primitives(
    handles: &[Handle<bevy::gltf::Gltf>],
    gltf_assets: &Assets<bevy::gltf::Gltf>,
    gltf_meshes: &Assets<bevy::gltf::GltfMesh>,
) -> Option<Vec<(Handle<Mesh>, Handle<StandardMaterial>)>> {
    let mut result = Vec::with_capacity(handles.len());
    for handle in handles {
        let gltf = gltf_assets.get(handle)?;
        let gltf_mesh_handle = gltf.meshes.first()?;
        let gltf_mesh = gltf_meshes.get(gltf_mesh_handle)?;
        let primitive = gltf_mesh.primitives.first()?;
        result.push((
            primitive.mesh.clone(),
            primitive.material.clone().unwrap_or_default(),
        ));
    }
    Some(result)
}

/// Extract ALL (mesh, material) pairs from each GLTF handle (all meshes, all primitives).
/// Returns the flat list and how many primitives came from each GLTF model.
fn extract_gltf_all_primitives(
    handles: &[Handle<bevy::gltf::Gltf>],
    gltf_assets: &Assets<bevy::gltf::Gltf>,
    gltf_meshes: &Assets<bevy::gltf::GltfMesh>,
) -> Option<(Vec<(Handle<Mesh>, Handle<StandardMaterial>)>, Vec<usize>)> {
    let mut result = Vec::new();
    let mut model_sizes = Vec::with_capacity(handles.len());
    for handle in handles {
        let gltf = gltf_assets.get(handle)?;
        let mut count = 0usize;
        for gltf_mesh_handle in &gltf.meshes {
            let gltf_mesh = gltf_meshes.get(gltf_mesh_handle)?;
            for primitive in &gltf_mesh.primitives {
                result.push((
                    primitive.mesh.clone(),
                    primitive.material.clone().unwrap_or_default(),
                ));
                count += 1;
            }
        }
        model_sizes.push(count);
    }
    Some((result, model_sizes))
}

// ── Team Color Texture Application ──

use crate::types::{BuildingSceneChild, Faction, FactionColors, TeamColorApplied, UnitSceneChild};

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

        // Neutral mobs can use standalone GLB materials that should stay untouched.
        if *faction == Faction::Neutral {
            continue;
        }

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

        commands.entity(entity).insert(MeshMaterial3d(new_handle));
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
