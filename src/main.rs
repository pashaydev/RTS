mod buildings;
mod camera;
mod combat;
mod components;
mod fog;
mod fog_material;
mod ground;
mod lighting;
mod mobs;
mod model_assets;
mod pathvis;
mod resources;
mod selection;
mod ui;
mod units;
mod vfx;

use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "RTS Prototype".to_string(),
                resolution: (1280u32, 720u32).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
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
        .run();
}
