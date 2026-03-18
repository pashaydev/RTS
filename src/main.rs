mod ai;
mod animation;
mod attention;
mod blueprints;
mod buildings;
mod camera;
mod combat;
mod components;
mod culling;
mod debug;
mod fog;
mod fog_material;
mod ground;
mod hover_material;
mod lighting;
mod menu;
mod minimap;
mod multiplayer;
mod net_bridge;
mod mobs;
mod model_assets;
mod orders;
mod pathfinding;
mod pause_menu;
mod pathvis;
mod resources;
mod roads;
mod save;
mod selection;
mod spatial;
mod theme;
mod ui;
mod unit_ai;
mod units;
mod vfx;

use bevy::ecs::error;
use bevy::prelude::*;
use bevy_mod_outline::OutlinePlugin;

use components::{AppState, GameSetupConfig, GraphicsSettings};

fn main() {
    // Resolve the executable's directory so assets/config/saves are found
    // correctly in distribution builds (especially Windows).
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    if let Some(ref dir) = exe_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Build an absolute asset path from the exe directory so Bevy's
    // AssetServer works even when CWD is unexpected (Windows shortcuts,
    // UNC paths, etc.).
    let asset_path = exe_dir
        .as_ref()
        .map(|d| d.join("assets").to_string_lossy().into_owned())
        .unwrap_or_else(|| "assets".to_string());

    let graphics = GraphicsSettings::load_or_default();
    let (w, h) = graphics.resolution;

    App::new()
        .set_error_handler(error::warn)
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "RTS Prototype".to_string(),
                        resolution: (w, h).into(),
                        mode: if graphics.fullscreen {
                            bevy::window::WindowMode::BorderlessFullscreen(
                                MonitorSelection::Current,
                            )
                        } else {
                            bevy::window::WindowMode::Windowed
                        },
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: asset_path,
                    meta_check: bevy::asset::AssetMetaCheck::Never,
                    ..default()
                }),
        )
        .add_plugins(OutlinePlugin)
        .init_state::<AppState>()
        .insert_resource(GameSetupConfig::default())
        .insert_resource(graphics)
        .add_plugins(menu::MenuPlugin)
        .add_plugins(blueprints::BlueprintPlugin)
        .add_plugins((
            debug::DebugPlugin,
            model_assets::ModelAssetsPlugin,
            ground::GroundPlugin,
            camera::CameraPlugin,
            lighting::LightingPlugin,
            units::UnitsPlugin,
            selection::SelectionPlugin,
            ui::UiPlugin,
            resources::ResourcesPlugin,
            buildings::BuildingsPlugin,
            pathvis::PathVisPlugin,
            vfx::VfxPlugin,
            mobs::MobsPlugin,
            combat::CombatPlugin,
            fog::FogPlugin,
        ))
        .add_plugins(spatial::SpatialPlugin)
        .add_plugins(pathfinding::PathfindingPlugin)
        .add_plugins(roads::RoadPlugin)
        .add_plugins(save::SavePlugin)
        .add_plugins(culling::CullingPlugin)
        .add_plugins(animation::AnimationPlugin)
        .add_plugins(minimap::MinimapPlugin)
        .add_plugins(attention::AttentionPlugin)
        .add_plugins(ai::AiPlugin)
        .add_plugins(unit_ai::UnitAiPlugin)
        .add_plugins(pause_menu::PauseMenuPlugin)
        .add_plugins(net_bridge::NetBridgePlugin)
        .add_plugins(multiplayer::MultiplayerPlugin)
        .run();
}
