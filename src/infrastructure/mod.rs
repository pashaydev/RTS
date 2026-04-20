//! `InfraPlugins`: composes session-level plumbing — logging, database,
//! debug tweak panel, save/load, network bridge, multiplayer, and audio.

pub mod audio;
pub mod database;
pub mod debug;
pub mod logging;
pub mod multiplayer;
pub mod net_bridge;
pub mod save_load;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

pub struct InfraPlugins;

impl PluginGroup for InfraPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(logging::SessionLogPlugin)
            .add(database::DatabasePlugin)
            .add(debug::DebugPlugin)
            .add(save_load::SaveLoadPlugin)
            .add(net_bridge::NetBridgePlugin)
            .add(multiplayer::MultiplayerPlugin)
            .add(audio::GameAudioPlugin)
    }
}
