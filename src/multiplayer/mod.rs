//! LAN Multiplayer — host-as-server model with TCP transport.
//!
//! Host runs full simulation, clients receive state updates and send commands.

pub mod client_systems;
pub mod host_systems;
pub mod transport;

use bevy::prelude::*;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use game_state::message::{ClientMessage, ServerMessage};

use crate::components::{ActivePlayer, AiControlledFactions, AppState, Faction};
use transport::NewClientEvent;

// ── Net Role ────────────────────────────────────────────────────────────────

#[derive(Resource, Default, PartialEq, Eq, Clone, Debug)]
pub enum NetRole {
    #[default]
    Offline,
    Host,
    Client,
}

// ── Lobby ───────────────────────────────────────────────────────────────────

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub enum LobbyStatus {
    #[default]
    Waiting,
    Connecting,
    Connected,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct LobbyPlayer {
    pub name: String,
    pub faction: Faction,
    pub is_host: bool,
    pub connected: bool,
}

#[derive(Resource, Default)]
pub struct LobbyState {
    pub players: Vec<LobbyPlayer>,
    pub session_code: String,
    pub status: LobbyStatus,
}

// ── Host Net State ──────────────────────────────────────────────────────────

#[derive(Resource)]
pub struct HostNetState {
    pub incoming_commands: Mutex<Receiver<(u8, ClientMessage)>>,
    pub client_senders: Mutex<Vec<(u8, Sender<Vec<u8>>)>>,
    pub new_clients: Mutex<Receiver<NewClientEvent>>,
    pub disconnect_rx: Mutex<Receiver<u8>>,
    pub shutdown: Arc<AtomicBool>,
    pub seq: Mutex<u32>,
}

// ── Client Net State ────────────────────────────────────────────────────────

#[derive(Resource)]
pub struct ClientNetState {
    pub incoming: Mutex<Receiver<ServerMessage>>,
    pub outgoing: Sender<Vec<u8>>,
    pub shutdown: Arc<AtomicBool>,
    pub my_faction: Faction,
    pub seq: Mutex<u32>,
}

// ── Run conditions ──────────────────────────────────────────────────────────

pub fn is_host(role: Res<NetRole>) -> bool {
    *role == NetRole::Host
}

pub fn is_client(role: Res<NetRole>) -> bool {
    *role == NetRole::Client
}

#[allow(dead_code)]
pub fn is_not_client(role: Res<NetRole>) -> bool {
    *role != NetRole::Client
}

#[allow(dead_code)]
pub fn is_online(role: Res<NetRole>) -> bool {
    *role != NetRole::Offline
}

/// Runs on `OnEnter(InGame)` when online — sets ActivePlayer for the correct faction
/// and removes human-controlled factions from AI.
/// Must run after `apply_game_config` but before `spawn_camera`.
pub fn configure_multiplayer_ai(
    lobby: Res<LobbyState>,
    role: Res<NetRole>,
    mut ai_factions: ResMut<AiControlledFactions>,
    mut active_player: ResMut<ActivePlayer>,
) {
    // Set the active player based on role
    match *role {
        NetRole::Host => {
            // Host is always Player1 (first in lobby)
            if let Some(host_player) = lobby.players.iter().find(|p| p.is_host) {
                active_player.0 = host_player.faction;
                info!("Host playing as {:?}", host_player.faction);
            }
        }
        NetRole::Client => {
            // Client is the first non-host connected player
            if let Some(client_player) = lobby.players.iter().find(|p| !p.is_host && p.connected) {
                active_player.0 = client_player.faction;
                info!("Client playing as {:?}", client_player.faction);
            }
        }
        NetRole::Offline => {}
    }

    // Remove human-controlled factions from AI
    for player in &lobby.players {
        if player.connected {
            ai_factions.factions.remove(&player.faction);
            info!(
                "Multiplayer: {:?} controlled by human ({})",
                player.faction, player.name
            );
        }
    }
}

// ── Plugin ──────────────────────────────────────────────────────────────────

pub struct MultiplayerPlugin;

impl Plugin for MultiplayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NetRole>()
            .init_resource::<LobbyState>()
            .add_systems(
                Update,
                (
                    host_systems::host_process_client_commands,
                    host_systems::host_handle_disconnects,
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(is_host),
            )
            .add_systems(
                Update,
                (
                    client_systems::client_receive_commands,
                    client_systems::client_handle_disconnect,
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(is_client),
            )
            .add_systems(
                OnEnter(AppState::InGame),
                configure_multiplayer_ai
                    .after(crate::units::apply_game_config)
                    .run_if(is_online),
            );
    }
}
