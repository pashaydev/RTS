//! Multiplayer — host-authoritative model with Matchbox WebRTC transport.
//!
//! Host runs full simulation, clients receive state updates and send commands.
//! Uses WebRTC data channels via `bevy_matchbox` for both native and WASM.

pub mod client_systems;
pub mod debug_tap;
pub mod host_systems;
pub mod matchbox_transport;
pub mod transport;

use bevy::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::components::{ActivePlayer, AiControlledFactions, AppState, Faction, FactionColors};

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

#[cfg(test)]
pub(crate) static NET_TRAFFIC_TEST_LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredHost {
    pub name: String,
    pub session_code: String,
}

#[derive(Resource, Default)]
pub struct LobbyState {
    pub players: Vec<LobbyPlayer>,
    pub session_code: String,
    pub status: LobbyStatus,
    /// All detected IPs (LAN + VPN/Hamachi) for display in host lobby.
    pub all_ips: Vec<(String, String, bool)>, // (ip, iface_name, is_vpn)
    pub discovered_hosts: Vec<DiscoveredHost>,
    pub discovery_status: String,
    /// The session code the client used to connect (persisted for display after page rebuilds).
    pub client_session_code: String,
}

// ── Reconnection ────────────────────────────────────────────────────────────

/// Tracks a disconnected player during the reconnection grace period.
#[derive(Debug, Clone)]
pub struct DisconnectedPlayer {
    pub session_token: u64,
    pub player_id: u8,
    pub faction: Faction,
    pub seat_index: u8,
    pub color_index: u8,
    pub name: String,
    /// Game-time when disconnection occurred.
    pub disconnect_time: f32,
}

/// Grace period (seconds) before a disconnected player's faction is converted to AI.
pub const RECONNECT_GRACE_PERIOD: f32 = 30.0;

/// Tracks session tokens for reconnection.
#[derive(Resource, Default)]
pub struct SessionTokens {
    /// Maps session_token → player_id for active sessions.
    pub tokens: std::collections::HashMap<u64, u8>,
    /// Players in the reconnection grace period.
    pub disconnected: Vec<DisconnectedPlayer>,
    /// Counter for generating unique tokens.
    counter: u64,
}

impl SessionTokens {
    pub fn generate(&mut self, player_id: u8) -> u64 {
        self.counter += 1;
        let token = self.counter ^ 0x5A3C_F7E1_9B2D_4A6E; // simple obfuscation
        self.tokens.insert(token, player_id);
        token
    }
}

// ── Host Net State ──────────────────────────────────────────────────────────

/// Lightweight host state — transport is handled by MatchboxSocket + PeerMap.
#[derive(Resource)]
pub struct HostNetState {
    pub seq: Mutex<u32>,
}

impl Default for HostNetState {
    fn default() -> Self {
        Self {
            seq: Mutex::new(0),
        }
    }
}

// ── Client Net State ────────────────────────────────────────────────────────

/// Client-side state — transport is handled by MatchboxSocket.
#[derive(Resource)]
pub struct ClientNetState {
    pub player_id: u8,
    pub seat_index: u8,
    pub my_faction: Faction,
    pub color_index: u8,
    pub seq: Mutex<u32>,
    /// Session token for reconnection (assigned by host via JoinAccepted).
    pub session_token: u64,
    /// Set to true when the host disconnects (triggers return to menu).
    pub disconnected: std::sync::atomic::AtomicBool,
}

impl Default for ClientNetState {
    fn default() -> Self {
        Self {
            player_id: 0,
            seat_index: 0,
            my_faction: Faction::Player2,
            color_index: 0,
            seq: Mutex::new(0),
            session_token: 0,
            disconnected: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

// ── System sets ─────────────────────────────────────────────────────────────

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum NetSet {
    /// Host: process incoming client commands.
    /// Client: receive and apply server messages.
    Receive,
    /// Host: broadcast state/entity/building/resource/day-cycle sync.
    Broadcast,
}

// Keep old name as alias for backwards compat within the crate
#[allow(dead_code)]
pub type MultiplayerSet = NetSet;

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
    mut pending_neutral: ResMut<client_systems::PendingNeutralUpdates>,
    mut prev_snapshots: ResMut<host_systems::PreviousSnapshots>,
    mut prev_buildings: ResMut<host_systems::PreviousBuildingSnapshots>,
    mut prev_neutral: ResMut<host_systems::PreviousNeutralSnapshots>,
    mut pending_frame: ResMut<host_systems::PendingServerFrame>,
) {
    synced.known.clear();
    synced.full_resync_counter = 0;
    pending.spawns.clear();
    pending.despawns.clear();
    pending_neutral.deltas.clear();
    pending_neutral.despawns.clear();
    prev_snapshots.snapshots.clear();
    prev_snapshots.full_sync_counter = 0;
    prev_buildings.snapshots.clear();
    prev_neutral.amounts.clear();
    pending_frame.messages.clear();
}

pub struct MultiplayerPlugin;

impl Plugin for MultiplayerPlugin {
    fn build(&self, app: &mut App) {
        debug_tap::ensure_started();

        // Configure system set ordering: Receive runs before Broadcast
        app.configure_sets(
            Update,
            (
                NetSet::Receive,
                NetSet::Broadcast.after(NetSet::Receive),
            ),
        );

        app.init_resource::<NetRole>()
            .init_resource::<LobbyState>()
            .init_resource::<NetStats>()
            .init_resource::<SessionTokens>()
            .init_resource::<matchbox_transport::PeerMap>()
            .init_resource::<matchbox_transport::MatchboxInbox>()
            .init_resource::<host_systems::StateSyncTimer>()
            .init_resource::<host_systems::SyncedEntitySet>()
            .init_resource::<host_systems::PreviousSnapshots>()
            .init_resource::<host_systems::PendingServerFrame>()
            .init_resource::<host_systems::PreviousBuildingSnapshots>()
            .init_resource::<host_systems::BuildingSyncTimer>()
            .init_resource::<host_systems::ResourceSyncTimer>()
            .init_resource::<host_systems::DayCycleSyncTimer>()
            .init_resource::<host_systems::NeutralWorldSyncTimer>()
            .init_resource::<host_systems::PreviousNeutralSnapshots>()
            .init_resource::<client_systems::PendingNetSpawns>()
            .init_resource::<client_systems::PendingNeutralUpdates>()
            .init_resource::<client_systems::ClientPingTimer>()
            // Poll matchbox socket each frame (before game systems)
            .add_systems(
                Update,
                matchbox_transport::poll_matchbox
                    .run_if(resource_exists::<bevy_matchbox::prelude::MatchboxSocket>)
                    .run_if(in_state(AppState::InGame))
                    .run_if(is_online)
                    .before(NetSet::Receive),
            )
            // Host: receive commands
            .add_systems(
                Update,
                (
                    host_systems::host_process_client_commands,
                    host_systems::host_handle_disconnects,
                )
                    .in_set(NetSet::Receive)
                    .run_if(in_state(AppState::InGame))
                    .run_if(is_host),
            )
            // Host: broadcast sync data
            .add_systems(
                Update,
                (
                    host_systems::host_broadcast_state_sync,
                    host_systems::host_broadcast_entity_spawns
                        .after(host_systems::host_broadcast_state_sync),
                    host_systems::host_broadcast_building_sync,
                    host_systems::host_broadcast_resource_sync,
                    host_systems::host_broadcast_day_cycle_sync,
                    host_systems::host_broadcast_neutral_world_sync,
                )
                    .in_set(NetSet::Broadcast)
                    .run_if(in_state(AppState::InGame))
                    .run_if(is_host),
            )
            // Client: receive and apply server messages
            .add_systems(
                Update,
                client_systems::client_receive_commands
                    .in_set(NetSet::Receive)
                    .run_if(in_state(AppState::InGame))
                    .run_if(is_client),
            )
            // Client: post-receive processing
            .add_systems(
                Update,
                (
                    client_systems::client_apply_entity_sync,
                    client_systems::client_apply_neutral_sync,
                    client_systems::client_interpolate_remote_units,
                    client_systems::client_handle_disconnect,
                    client_systems::client_send_ping,
                )
                    .in_set(NetSet::Broadcast)
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
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn lobby_player(
        player_id: u8,
        name: &str,
        faction: Faction,
        color_index: u8,
        is_host: bool,
        connected: bool,
    ) -> LobbyPlayer {
        LobbyPlayer {
            player_id,
            name: name.to_string(),
            seat_index: player_id,
            faction,
            color_index,
            is_host,
            connected,
        }
    }

    #[test]
    fn display_value_formats_expected_fields() {
        let stats = NetStats {
            rtt_ms: 12.34,
            rtt_smoothed_ms: 8.76,
            total_bytes_sent: 1_536,
            total_bytes_received: 2_097_152,
            total_msgs_sent: 7,
            total_msgs_received: 9,
            bytes_sent_last_sec: 512,
            bytes_received_last_sec: 3_072,
            connected_clients: 2,
            last_sync_entity_count: 42,
            net_map_size: 77,
            pending_spawns: 3,
            ..Default::default()
        };

        assert_eq!(stats.display_value("Role", &NetRole::Host), Some("Host".to_string()));
        assert_eq!(stats.display_value("Ping", &NetRole::Client), Some("12.3 ms".to_string()));
        assert_eq!(
            stats.display_value("Smoothed RTT", &NetRole::Client),
            Some("8.8 ms".to_string())
        );
        assert_eq!(stats.display_value("Clients", &NetRole::Host), Some("2".to_string()));
        assert_eq!(stats.display_value("Sent/s", &NetRole::Host), Some("512 B/s".to_string()));
        assert_eq!(
            stats.display_value("Received/s", &NetRole::Host),
            Some("3.0 KB/s".to_string())
        );
        assert_eq!(stats.display_value("Total Sent", &NetRole::Host), Some("1.5 KB".to_string()));
        assert_eq!(
            stats.display_value("Total Received", &NetRole::Host),
            Some("2.0 MB".to_string())
        );
        assert_eq!(stats.display_value("Msgs Sent", &NetRole::Host), Some("7".to_string()));
        assert_eq!(stats.display_value("Msgs Received", &NetRole::Host), Some("9".to_string()));
        assert_eq!(stats.display_value("Sync Entities", &NetRole::Host), Some("42".to_string()));
        assert_eq!(stats.display_value("Net Map Size", &NetRole::Host), Some("77".to_string()));
        assert_eq!(stats.display_value("Pending Spawns", &NetRole::Host), Some("3".to_string()));
        assert_eq!(stats.display_value("Status", &NetRole::Host), None);
    }

    #[test]
    fn display_value_uses_placeholders_for_zero_rtt() {
        let stats = NetStats::default();
        assert_eq!(stats.display_value("Ping", &NetRole::Client), Some("--".to_string()));
        assert_eq!(
            stats.display_value("Smoothed RTT", &NetRole::Client),
            Some("--".to_string())
        );
    }

    #[test]
    fn session_tokens_generate_unique_obfuscated_tokens() {
        let mut tokens = SessionTokens::default();

        let first = tokens.generate(3);
        let second = tokens.generate(5);

        assert_ne!(first, second);
        assert_eq!(tokens.tokens.get(&first), Some(&3));
        assert_eq!(tokens.tokens.get(&second), Some(&5));
    }

    #[test]
    fn configure_multiplayer_ai_sets_host_player_and_removes_human_ai() {
        let mut world = World::new();
        world.insert_resource(LobbyState {
            players: vec![
                lobby_player(0, "Host", Faction::Player3, 1, true, true),
                lobby_player(1, "Guest", Faction::Player2, 0, false, true),
            ],
            ..Default::default()
        });
        world.insert_resource(NetRole::Host);
        world.insert_resource(AiControlledFactions::default());
        world.insert_resource(ActivePlayer::default());
        world.insert_resource(FactionColors::default());

        let mut system = IntoSystem::into_system(configure_multiplayer_ai);
        system.initialize(&mut world);
        let _ = system.run((), &mut world);

        let active_player = world.resource::<ActivePlayer>();
        let ai = world.resource::<AiControlledFactions>();
        let colors = world.resource::<FactionColors>();
        assert_eq!(active_player.0, Faction::Player3);
        assert!(!ai.factions.contains(&Faction::Player3));
        assert!(!ai.factions.contains(&Faction::Player2));
        assert_eq!(colors.get(&Faction::Player3), FactionColors::from_index(1));
        assert_eq!(colors.get(&Faction::Player2), FactionColors::from_index(0));
    }

    #[test]
    fn configure_multiplayer_ai_for_client_disables_all_ai_and_uses_client_faction() {
        let mut world = World::new();
        world.insert_resource(LobbyState {
            players: vec![lobby_player(0, "Host", Faction::Player1, 0, true, true)],
            ..Default::default()
        });
        world.insert_resource(NetRole::Client);
        world.insert_resource(ClientNetState {
            player_id: 2,
            seat_index: 1,
            my_faction: Faction::Player4,
            color_index: 3,
            ..Default::default()
        });
        world.insert_resource(AiControlledFactions::default());
        world.insert_resource(ActivePlayer::default());
        world.insert_resource(FactionColors::default());

        let mut system = IntoSystem::into_system(configure_multiplayer_ai);
        system.initialize(&mut world);
        let _ = system.run((), &mut world);

        let active_player = world.resource::<ActivePlayer>();
        let ai = world.resource::<AiControlledFactions>();
        assert_eq!(active_player.0, Faction::Player4);
        assert!(ai.factions.is_empty());
    }

    #[test]
    fn update_net_stats_drains_counters_and_rolls_rate_snapshot() {
        let _guard = NET_TRAFFIC_TEST_LOCK.lock().unwrap();
        NET_TRAFFIC.bytes_sent.store(0, Ordering::Relaxed);
        NET_TRAFFIC.bytes_received.store(0, Ordering::Relaxed);
        NET_TRAFFIC.msgs_sent.store(0, Ordering::Relaxed);
        NET_TRAFFIC.msgs_received.store(0, Ordering::Relaxed);
        NET_TRAFFIC.bytes_sent.store(256, Ordering::Relaxed);
        NET_TRAFFIC.bytes_received.store(128, Ordering::Relaxed);
        NET_TRAFFIC.msgs_sent.store(3, Ordering::Relaxed);
        NET_TRAFFIC.msgs_received.store(2, Ordering::Relaxed);

        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.insert_resource(NetStats::default());
        app.add_systems(Update, update_net_stats);

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(0.4));
        app.update();

        {
            let stats = app.world().resource::<NetStats>();
            assert_eq!(stats.total_bytes_sent, 256);
            assert_eq!(stats.total_bytes_received, 128);
            assert_eq!(stats.total_msgs_sent, 3);
            assert_eq!(stats.total_msgs_received, 2);
            assert_eq!(stats.bytes_sent_last_sec, 0);
            assert_eq!(stats.bytes_received_last_sec, 0);
            assert_eq!(stats.bytes_sent_accum, 256);
            assert_eq!(stats.bytes_recv_accum, 128);
        }

        NET_TRAFFIC.bytes_sent.store(64, Ordering::Relaxed);
        NET_TRAFFIC.bytes_received.store(32, Ordering::Relaxed);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(0.7));
        app.update();

        let stats = app.world().resource::<NetStats>();
        assert_eq!(stats.total_bytes_sent, 320);
        assert_eq!(stats.total_bytes_received, 160);
        assert_eq!(stats.bytes_sent_last_sec, 320);
        assert_eq!(stats.bytes_received_last_sec, 160);
        assert_eq!(stats.bytes_sent_accum, 0);
        assert_eq!(stats.bytes_recv_accum, 0);
        assert_eq!(stats.rate_timer, 0.0);

        NET_TRAFFIC.bytes_sent.store(0, Ordering::Relaxed);
        NET_TRAFFIC.bytes_received.store(0, Ordering::Relaxed);
        NET_TRAFFIC.msgs_sent.store(0, Ordering::Relaxed);
        NET_TRAFFIC.msgs_received.store(0, Ordering::Relaxed);
    }
}
