//! `PresentationPlugins`: rendering layer composing camera, animation,
//! LOD, VFX, pathvis, entity labels, minimap, cursor, and custom materials.

pub mod animation;
pub mod camera;
pub mod cursor;
pub mod entity_labels;
pub mod materials;
pub mod minimap;
pub mod model_assets;
pub mod pathvis;
pub mod unit_lod;
pub mod vfx;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

pub struct PresentationPlugins;

impl PluginGroup for PresentationPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(model_assets::ModelAssetsPlugin)
            .add(materials::impostor::ImpostorMaterialPlugin)
            .add(camera::CameraPlugin)
            .add(animation::AnimationPlugin)
            .add(unit_lod::UnitLodPlugin)
            .add(vfx::VfxPlugin)
            .add(pathvis::PathVisPlugin)
            .add(entity_labels::EntityLabelPlugin)
            .add(minimap::MinimapPlugin)
            .add(cursor::CursorPlugin)
    }
}
