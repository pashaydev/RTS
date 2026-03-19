//! LAN Multiplayer — host-as-server model with TCP transport (native) and WebSocket (WASM).
//!
//! Host runs full simulation, clients receive state updates and send commands.

pub mod client_systems;
pub mod debug_tap;
pub mod ggrs_matchbox;
pub mod host_systems;
pub mod transport;

use bevy::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use game_state::message::{ClientMessage, ServerMessage};

use crate::components::{ActivePlayer, AiControlledFactions, AppState, Faction, FactionColors};
use transport::NewClientEvent;

// ── Net Stats ───────────────────────────────────────────────────────────────

#[derive(Resource, Default)]
pub struct NetStats {
    // RTT (client only)
    pub rtt_ms: f32,
    pub rtt_smoothed_ms: f32,
    pub last_ping_sent_at: f64,

    // Cumulative totals
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub total_msgs_sent: u64,
    pub total_msgs_received: u64,

    // Per-second rate snapshots
    pub bytes_sent_last_sec: u64,
    pub bytes_received_last_sec: u64,

    // Accumulator for per-second rate calc
    pub rate_timer: f32,
    pub bytes_sent_accum: u64,
    pub bytes_recv_accum: u64,

    // Host-only: connected client count
    pub connected_clients: u8,

    // Sync stats
    pub last_sync_entity_count: u32,
    pub net_map_size: u32,
    pub pending_spawns: u32,
}

/// Which roles a stat is relevant for.
#[derive(Clone, Copy)]
pub enum NetStatVisibility {
    /// Shown for all online roles (Host + Client).
    Always,
    /// Only meaningful for the host.
    HostOnly,
    /// Only meaningful for the client.
    ClientOnly,
}

/// A single debug-panel entry: (folder_key, label, visibility).
/// Folder key: "conn" → NET_CONN_FOLDER, "traffic" → NET_TRAFFIC_FOLDER.
pub struct NetStatField {
    pub folder_key: &'static str,
    pub label: &'static str,
    pub visibility: NetStatVisibility,
}

/// Declarative table of all network debug entries. Registration and sync both
/// iterate this, so adding a new stat is a one-line change.
pub const NET_STAT_FIELDS: &[NetStatField] = &[
    // ── Connection ──
    NetStatField { folder_key: "conn", label: "Role",         visibility: NetStatVisibility::Always },
    NetStatField { folder_key: "conn", label: "Status",       visibility: NetStatVisibility::Always },
    NetStatField { folder_key: "conn", label: "Ping",         visibility: NetStatVisibility::ClientOnly },
    NetStatField { folder_key: "conn", label: "Smoothed RTT", visibility: NetStatVisibility::ClientOnly },
    NetStatField { folder_key: "conn", label: "Clients",      visibility: NetStatVisibility::HostOnly },
    // ── Traffic ──
    NetStatField { folder_key: "traffic", label: "Sent/s",          visibility: NetStatVisibility::Always },
    NetStatField { folder_key: "traffic", label: "Received/s",      visibility: NetStatVisibility::Always },
    NetStatField { folder_key: "traffic", label: "Total Sent",      visibility: NetStatVisibility::Always },
    NetStatField { folder_key: "traffic", label: "Total Received",  visibility: NetStatVisibility::Always },
    NetStatField { folder_key: "traffic", label: "Msgs Sent",       visibility: NetStatVisibility::Always },
    NetStatField { folder_key: "traffic", label: "Msgs Received",   visibility: NetStatVisibility::Always },
    NetStatField { folder_key: "traffic", label: "Sync Entities",   visibility: NetStatVisibility::Always },
    NetStatField { folder_key: "traffic", label: "Net Map Size",    visibility: NetStatVisibility::Always },
    NetStatField { folder_key: "traffic", label: "Pending Spawns",  visibility: NetStatVisibility::Always },
];

impl NetStats {
    /// Returns the display string for a given field label, or `None` if the
    /// stat is not applicable to the current role.
    pub fn display_value(&self, label: &str, role: &NetRole) -> Option<String> {
        match label {
            "Role" => Some(match role {
                NetRole::Host => "Host",
                NetRole::Client => "Client",
                NetRole::Offline => "Offline",
            }.to_string()),
            "Ping" => Some(if self.rtt_ms > 0.0 {
                format!("{:.1} ms", self.rtt_ms)
            } else {
                "--".to_string()
            }),
            "Smoothed RTT" => Some(if self.rtt_smoothed_ms > 0.0 {
                format!("{:.1} ms", self.rtt_smoothed_ms)
            } else {
                "--".to_string()
            }),
            "Clients" => Some(self.connected_clients.to_string()),
            "Sent/s" => Some(format_bytes_per_sec(self.bytes_sent_last_sec)),
            "Received/s" => Some(format_bytes_per_sec(self.bytes_received_last_sec)),
            "Total Sent" => Some(format_bytes(self.total_bytes_sent)),
            "Total Received" => Some(format_bytes(self.total_bytes_received)),
            "Msgs Sent" => Some(self.total_msgs_sent.to_string()),
            "Msgs Received" => Some(self.total_msgs_received.to_string()),
            "Sync Entities" => Some(self.last_sync_entity_count.to_string()),
            "Net Map Size" => Some(self.net_map_size.to_string()),
            "Pending Spawns" => Some(self.pending_spawns.to_string()),
            _ => None, // "Status" is derived from LobbyState, handled externally
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn format_bytes_per_sec(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB/s", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB/s", bytes as f64 / 1024.0)
    } else {
        format!("{} B/s", bytes)
    }
}

// ── Global atomic traffic counters (thread-safe, incremented from transport) ─

pub struct NetTrafficCounters {
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub msgs_sent: AtomicU64,
    pub msgs_received: AtomicU64,
}

impl Default for NetTrafficCounters {
    fn default() -> Self {
        Self {
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            msgs_sent: AtomicU64::new(0),
            msgs_received: AtomicU64::new(0),
        }
    }
}

pub static NET_TRAFFIC: std::sync::LazyLock<NetTrafficCounters> =
    std::sync::LazyLock::new(NetTrafficCounters::default);

/// Drains atomic traffic counters into the `NetStats` resource each frame,
/// and computes per-second byte rates.
fn update_net_stats(time: Res<Time>, mut stats: ResMut<NetStats>) {
    // Drain atomics
    let sent = NET_TRAFFIC.bytes_sent.swap(0, Ordering::Relaxed);
    let recv = NET_TRAFFIC.bytes_received.swap(0, Ordering::Relaxed);
    let msgs_sent = NET_TRAFFIC.msgs_sent.swap(0, Ordering::Relaxed);
    let msgs_recv = NET_TRAFFIC.msgs_received.swap(0, Ordering::Relaxed);

    stats.total_bytes_sent += sent;
    stats.total_bytes_received += recv;
    stats.total_msgs_sent += msgs_sent;
    stats.total_msgs_received += msgs_recv;

    stats.bytes_sent_accum += sent;
    stats.bytes_recv_accum += recv;

    // Per-second rate snapshot
    stats.rate_timer += time.delta_secs();
    if stats.rate_timer >= 1.0 {
        stats.bytes_sent_last_sec = stats.bytes_sent_accum;
        stats.bytes_received_last_sec = stats.bytes_recv_accum;
        stats.bytes_sent_accum = 0;
        stats.bytes_recv_accum = 0;
        stats.rate_timer = 0.0;
    }
}

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
    /// Receiver for WebSocket new client events (reader/writer already spawned).
    pub new_ws_clients: Mutex<Receiver<transport::WsNewClientEvent>>,
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

// ── WASM WebSocket client resource ──────────────────────────────────────────

/// Holds the browser WebSocket and outgoing message receiver for WASM clients.
/// A Bevy system drains `outgoing_rx` and sends via the WebSocket.
#[cfg(target_arch = "wasm32")]
#[derive(Resource)]
pub struct WasmClientSocket {
    pub ws: web_sys::WebSocket,
    pub outgoing_rx: Mutex<Receiver<Vec<u8>>>,
}

/// Drains queued outgoing messages and sends them via the browser WebSocket.
#[cfg(target_arch = "wasm32")]
pub fn wasm_flush_outgoing(socket: Option<Res<WasmClientSocket>>) {
    let Some(socket) = socket else { return };
    if socket.ws.ready_state() != 1 {
        return; // Not OPEN yet
    }
    let rx = socket.outgoing_rx.lock().unwrap();
    for _ in 0..256 {
        match rx.try_recv() {
            Ok(data) => {
                let text = String::from_utf8_lossy(&data).to_string();
                let _ = socket.ws.send_with_str(&text);
            }
            Err(_) => break,
        }
    }
}

// ── System sets ─────────────────────────────────────────────────────────────

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MultiplayerSet {
    ClientReceive,
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
    mut prev_snapshots: ResMut<host_systems::PreviousSnapshots>,
) {
    synced.known.clear();
    synced.full_resync_counter = 0;
    pending.spawns.clear();
    pending.despawns.clear();
    prev_snapshots.snapshots.clear();
    prev_snapshots.full_sync_counter = 0;
}

pub struct MultiplayerPlugin;

impl Plugin for MultiplayerPlugin {
    fn build(&self, app: &mut App) {
        debug_tap::ensure_started();
        app.init_resource::<NetRole>()
            .init_resource::<LobbyState>()
            .init_resource::<NetStats>()
            .init_resource::<host_systems::StateSyncTimer>()
            .init_resource::<host_systems::SyncedEntitySet>()
            .init_resource::<host_systems::PreviousSnapshots>()
            .init_resource::<host_systems::BuildingSyncTimer>()
            .init_resource::<host_systems::ResourceSyncTimer>()
            .init_resource::<host_systems::DayCycleSyncTimer>()
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
                    host_systems::host_broadcast_building_sync,
                    host_systems::host_broadcast_resource_sync,
                    host_systems::host_broadcast_day_cycle_sync,
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(is_host),
            )
            .add_systems(
                Update,
                client_systems::client_receive_commands
                    .in_set(MultiplayerSet::ClientReceive)
                    .run_if(in_state(AppState::InGame))
                    .run_if(is_client),
            )
            .add_systems(
                Update,
                (
                    client_systems::client_apply_entity_sync
                        .after(MultiplayerSet::ClientReceive),
                    client_systems::client_interpolate_remote_units
                        .after(MultiplayerSet::ClientReceive),
                    client_systems::client_handle_disconnect,
                    client_systems::client_send_ping,
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(is_client),
            )
            .add_systems(
                Update,
                update_net_stats.run_if(in_state(AppState::InGame)),
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

        // WASM: flush outgoing WebSocket messages each frame
        #[cfg(target_arch = "wasm32")]
        app.add_systems(
            Update,
            wasm_flush_outgoing
                .run_if(in_state(AppState::InGame))
                .run_if(is_client),
        );
    }
}
