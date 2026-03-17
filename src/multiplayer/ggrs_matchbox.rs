//! Matchbox + GGRS integration scaffolding.
//!
//! This module holds resources/events for the upcoming rollback migration.
#![allow(dead_code)]

use bevy::prelude::*;
use bevy_matchbox::prelude::MatchboxSocket;

/// Matchbox signaling + room setup used by the rollback transport path.
#[derive(Resource, Debug, Clone)]
pub struct MatchboxSettings {
    pub signaling_url: String,
    pub room_id: String,
    pub expected_players: usize,
}

impl Default for MatchboxSettings {
    fn default() -> Self {
        Self {
            signaling_url: "ws://127.0.0.1:3536".to_string(),
            room_id: "rts?next=2".to_string(),
            expected_players: 2,
        }
    }
}

/// High-level state for the rollback/matchbox path.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum RollbackNetState {
    #[default]
    Disabled,
    Lobby,
    Connecting,
    Running,
}

/// Holds a live matchbox socket once created.
#[derive(Resource, Default)]
pub struct MatchboxSocketResource {
    pub socket: Option<MatchboxSocket>,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct StartRollbackSession;

pub struct GgrsMatchboxPlugin;

impl Plugin for GgrsMatchboxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MatchboxSettings>()
            .init_resource::<RollbackNetState>()
            .init_resource::<MatchboxSocketResource>()
            .add_message::<StartRollbackSession>();
    }
}
