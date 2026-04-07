pub mod culling;
pub mod fog;
pub mod ground;
pub mod lighting;
pub mod pathfinding;
pub mod spatial;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

pub struct WorldPlugins;

impl PluginGroup for WorldPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(ground::GroundPlugin)
            .add(fog::FogPlugin)
            .add(lighting::LightingPlugin)
            .add(culling::CullingPlugin)
            .add(spatial::SpatialPlugin)
            .add(pathfinding::PathfindingPlugin)
    }
}
