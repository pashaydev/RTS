mod camera;
mod combat;
mod components;
mod ground;
mod mobs;
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
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            ground::GroundPlugin,
            camera::CameraPlugin,
            units::UnitsPlugin,
            selection::SelectionPlugin,
            ui::UiPlugin,
            resources::ResourcesPlugin,
            pathvis::PathVisPlugin,
            vfx::VfxPlugin,
            mobs::MobsPlugin,
            combat::CombatPlugin,
        ))
        .run();
}
