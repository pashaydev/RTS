//! LAN Multiplayer — host-as-server model with TCP transport.
//!
//! Host runs full simulation, clients receive state updates and send commands.

pub mod client_systems;
pub mod debug_tap;
pub mod ggrs_matchbox;
pub mod host_systems;
pub mod transport;

use bevy::prelude::*;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use game_state::message::{ClientMessage, ServerMessage};

use crate::components::{ActivePlayer, AiControlledFactions, AppState, Faction, FactionColors};
use transport::NewClientEvent;

// ── Net Role ────────────────────────────────────────────────────────────────

#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
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
    pub player_id: u8,
    pub name: String,
    pub seat_index: u8,
    pub faction: Faction,
    pub color_index: u8,
    pub is_host: bool,
    pub connected: bool,
}

#[derive(Resource, Default)]
pub struct LobbyState {
    pub players: Vec<LobbyPlayer>,
    pub session_code: String,
    pub status: LobbyStatus,
    /// All detected IPs (LAN + VPN/Hamachi) for display in host lobby.
    pub all_ips: Vec<(String, String, bool)>, // (ip, iface_name, is_vpn)
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
    pub player_id: u8,
    pub seat_index: u8,
    pub my_faction: Faction,
    pub color_index: u8,
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

/// Runs on `OnEnter(InGame)` when online — sets ActivePlayer for the correct faction,
/// removes human-controlled factions from AI, and builds the FactionColors map
/// from authoritative seat/color assignments.
/// Must run after `apply_game_config` but before `spawn_camera`.
pub fn configure_multiplayer_ai(
    lobby: Res<LobbyState>,
    role: Res<NetRole>,
    client_state: Option<Res<ClientNetState>>,
    mut ai_factions: ResMut<AiControlledFactions>,
    mut active_player: ResMut<ActivePlayer>,
    mut faction_colors: ResMut<FactionColors>,
) {
    // Build FactionColors from lobby seat assignments
    for player in &lobby.players {
        let color = FactionColors::from_index(player.color_index);
        faction_colors.colors.insert(player.faction, color);
        info!(
            "Multiplayer color: {:?} → {:?} (seat {}, color_index {})",
            player.faction, color, player.seat_index, player.color_index
        );
    }

    // Set the active player based on role
    match *role {
        NetRole::Host => {
            if let Some(host_player) = lobby.players.iter().find(|p| p.is_host) {
                active_player.0 = host_player.faction;
                info!("Host playing as {:?} (seat {})", host_player.faction, host_player.seat_index);
            }
        }
        NetRole::Client => {
            if let Some(client) = client_state.as_ref() {
                active_player.0 = client.my_faction;
                info!(
                    "Client playing as {:?} (seat {}, color_index {})",
                    client.my_faction, client.seat_index, client.color_index
                );
            } else if let Some(client_player) = lobby.players.iter().find(|p| !p.is_host && p.connected) {
                active_player.0 = client_player.faction;
                info!("Client playing as {:?} (fallback)", client_player.faction);
            }
        }
        NetRole::Offline => {}
    }

    // On the client, disable ALL AI — the host runs simulation and syncs state.
    // On the host, only remove human-controlled factions from AI.
    if *role == NetRole::Client {
        info!("Client: disabling all local AI (host is authoritative)");
        ai_factions.factions.clear();
    } else {
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
}

// ── Plugin ──────────────────────────────────────────────────────────────────

fn reset_multiplayer_sync(
    mut synced: ResMut<host_systems::SyncedEntitySet>,
    mut pending: ResMut<client_systems::PendingNetSpawns>,
) {
    synced.known.clear();
    pending.spawns.clear();
    pending.despawns.clear();
}

pub struct MultiplayerPlugin;

impl Plugin for MultiplayerPlugin {
    fn build(&self, app: &mut App) {
        debug_tap::ensure_started();
        app.init_resource::<NetRole>()
            .init_resource::<LobbyState>()
            .init_resource::<host_systems::StateSyncTimer>()
            .init_resource::<host_systems::SyncedEntitySet>()
            .init_resource::<client_systems::PendingNetSpawns>()
            .init_resource::<client_systems::ClientPingTimer>()
            .add_systems(
                Update,
                (
                    host_systems::host_process_client_commands,
                    host_systems::host_handle_disconnects,
                    host_systems::host_broadcast_state_sync,
                    host_systems::host_broadcast_entity_spawns
                        .after(host_systems::host_broadcast_state_sync),
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(is_host),
            )
            .add_systems(
                Update,
                (
                    client_systems::client_receive_commands,
                    client_systems::client_apply_entity_sync
                        .after(client_systems::client_receive_commands),
                    client_systems::client_handle_disconnect,
                    client_systems::client_send_ping,
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(is_client),
            )
            .add_systems(
                OnEnter(AppState::InGame),
                (
                    configure_multiplayer_ai
                        .after(crate::units::apply_game_config)
                        .run_if(is_online),
                    reset_multiplayer_sync,
                ),
            )
            .add_plugins(ggrs_matchbox::GgrsMatchboxPlugin);
    }
}
