pub mod animation;
pub mod camera;
pub mod entity_labels;
pub mod materials;
pub mod minimap;
pub mod model_assets;
pub mod pathvis;
pub mod procedural_mobs;
pub mod vfx;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

pub struct PresentationPlugins;

impl PluginGroup for PresentationPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(model_assets::ModelAssetsPlugin)
            .add(camera::CameraPlugin)
            .add(animation::AnimationPlugin)
            .add(vfx::VfxPlugin)
            .add(pathvis::PathVisPlugin)
            .add(procedural_mobs::ProceduralMobsPlugin)
            .add(entity_labels::EntityLabelPlugin)
            .add(minimap::MinimapPlugin)
    }
}
