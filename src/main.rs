mod ai;
mod animation;
mod attention;
mod blueprints;
mod buildings;
mod camera;
mod combat;
mod components;
mod debug;
mod fog;
mod fog_material;
mod ground;
mod hover_material;
mod lighting;
mod menu;
mod minimap;
mod mobs;
mod model_assets;
mod pathvis;
mod resources;
mod save;
mod selection;
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
    let graphics = GraphicsSettings::load_or_default();
    let (w, h) = graphics.resolution;

    App::new()
        .set_error_handler(error::warn)
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "RTS Prototype".to_string(),
                resolution: (w, h).into(),
                mode: if graphics.fullscreen {
                    bevy::window::WindowMode::BorderlessFullscreen(MonitorSelection::Current)
                } else {
                    bevy::window::WindowMode::Windowed
                },
                ..default()
            }),
            ..default()
        }))
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
        .add_plugins(save::SavePlugin)
        .add_plugins(animation::AnimationPlugin)
        .add_plugins(minimap::MinimapPlugin)
        .add_plugins(attention::AttentionPlugin)
        .add_plugins(ai::AiPlugin)
        .add_plugins(unit_ai::UnitAiPlugin)
        .run();
}
