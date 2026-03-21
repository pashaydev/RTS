pub(crate) mod config;
pub(crate) mod lobby;
pub(crate) mod networking;
pub(crate) mod pages;

use crate::components::*;
use crate::multiplayer::LobbyState;

use bevy::prelude::*;

// ── Shared Types ──

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource)]
pub(crate) struct JoinDiscoveryScan {
    pub(super) rx: std::sync::Mutex<std::sync::mpsc::Receiver<Vec<crate::multiplayer::DiscoveredHost>>>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum JoinTarget {
    Tcp { host: String, port: u16 },
    WebSocket { url: String },
}

#[derive(Resource)]
pub(crate) struct PendingGameStart;

#[derive(Resource)]
pub(crate) struct PendingLobbyBroadcast;

// ── Constants ──

pub(super) const DEFAULT_PORT: u16 = 7878;
pub(super) const WEB_SESSION_WS_PATH_PREFIX: &str = "/session";

// ── Shared Helpers ──

pub(super) fn first_open_multiplayer_slot(config: &GameSetupConfig, lobby: &LobbyState) -> Option<usize> {
    (0..config.slots.len()).find(|&slot_index| {
        matches!(config.slots[slot_index], SlotOccupant::Open)
            && !lobby
                .players
                .iter()
                .any(|player| player.connected && player.faction == Faction::PLAYERS[slot_index])
    })
}

pub(super) fn sync_multiplayer_slots_from_lobby(config: &mut GameSetupConfig, lobby: &LobbyState) -> bool {
    let mut changed = false;
    for (slot_index, faction) in Faction::PLAYERS.iter().enumerate() {
        let occupied_by_human = lobby
            .players
            .iter()
            .any(|player| player.connected && player.faction == *faction);

        let desired = if occupied_by_human {
            SlotOccupant::Human
        } else if matches!(config.slots[slot_index], SlotOccupant::Human) {
            SlotOccupant::Open
        } else {
            continue;
        };

        if config.slots[slot_index] != desired {
            config.slots[slot_index] = desired;
            changed = true;
        }
    }

    changed
}

pub(crate) fn prepare_multiplayer_host_config(config: &mut GameSetupConfig) {
    config.slots[config.local_player_slot] = SlotOccupant::Human;
    for slot_index in 0..config.slots.len() {
        if slot_index == config.local_player_slot {
            continue;
        }
        if !matches!(config.slots[slot_index], SlotOccupant::Closed) {
            config.slots[slot_index] = SlotOccupant::Open;
        }
    }
}

// ── Re-exports ──

pub(crate) use networking::cleanup_network_on_enter_menu;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use networking::start_hosting;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use networking::stop_hosting;
pub(crate) use networking::stop_client;
pub(crate) use networking::connect_to_host_system;
pub(crate) use networking::refresh_lan_hosts_system;
pub(crate) use networking::poll_lan_discovery_results_system;
pub(crate) use networking::select_discovered_host_system;

pub(crate) use pages::spawn_multiplayer_page;
pub(crate) use pages::spawn_host_lobby_page;
pub(crate) use pages::spawn_join_lobby_page;

pub(crate) use lobby::update_lobby_ui;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use lobby::broadcast_lobby_update;
pub(crate) use lobby::update_web_client_url;
pub(crate) use lobby::copy_session_code_system;
pub(crate) use lobby::paste_code_system;
pub(crate) use lobby::clear_code_system;
pub(crate) use lobby::copy_reset_system;
pub(crate) use lobby::connection_timer_system;
pub(crate) use lobby::countdown_system;
pub(crate) use lobby::kick_player_system;
pub(crate) use lobby::lobby_ping_system;
