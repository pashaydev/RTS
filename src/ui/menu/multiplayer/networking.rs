//! Lobby-level network glue: forwards Matchbox socket state into the
//! lobby UI and relays lobby actions back to the transport.

use bevy::prelude::*;
use bevy_matchbox::prelude::*;

use super::super::*;
use super::JoinDiscoveryScan;
use super::PendingGameStart;
use crate::infrastructure::multiplayer::{
    self,
    matchbox_transport::{self, MatchboxInbox, PeerMap, SIGNALING_PORT},
    ClientNetState, HostNetState, LobbyPlayer, LobbyState, LobbyStatus, NetRole,
};
use crate::types::*;

// ── Network Cleanup ──

pub(crate) fn cleanup_network_on_enter_menu(
    mut commands: Commands,
    client_state: Option<Res<ClientNetState>>,
    mut socket: Option<ResMut<MatchboxSocket>>,
) {
    // Send leave notice before closing
    if let (Some(client), Some(ref mut socket)) = (client_state.as_ref(), socket.as_mut()) {
        let seq = {
            let mut s = client.seq.lock().unwrap();
            *s += 1;
            *s
        };
        let leave_msg = game_state::message::ClientMessage::LeaveNotice {
            seq,
            timestamp: 0.0,
        };
        matchbox_transport::send_to_host(socket, &leave_msg);
    }

    commands.close_socket();
    commands.stop_server();
    commands.remove_resource::<HostNetState>();
    commands.remove_resource::<ClientNetState>();
    commands.remove_resource::<PendingGameStart>();
    commands.remove_resource::<JoinDiscoveryScan>();
    commands.insert_resource(NetRole::Offline);
    commands.insert_resource(LobbyState::default());
    commands.insert_resource(PeerMap::default());
    commands.insert_resource(MatchboxInbox::default());
    commands.remove_resource::<ConnectionTimer>();
    commands.remove_resource::<CopyResetTimer>();
    commands.remove_resource::<CountdownState>();
    commands.remove_resource::<LobbyPingTimer>();
    commands.insert_resource(PreferredFaction::default());
}

// ── Hosting ──

pub(crate) fn start_hosting(commands: &mut Commands, config: &GameSetupConfig) {
    use crate::infrastructure::multiplayer::transport;
    use std::net::Ipv4Addr;

    let all_ips = transport::detect_all_ips();
    let primary_ip = if let Some(vpn) = all_ips.iter().find(|ip| ip.is_likely_vpn) {
        vpn.ip.clone()
    } else {
        transport::detect_lan_ip().unwrap_or_else(|| "127.0.0.1".to_string())
    };

    let host_name = if config.player_name.trim().is_empty() {
        "Host".to_string()
    } else {
        config.player_name.trim().to_string()
    };

    // Start embedded signaling server (ClientServer topology: first peer = host)
    let signaling_builder =
        MatchboxServer::client_server_builder((Ipv4Addr::UNSPECIFIED, SIGNALING_PORT));
    commands.start_server(signaling_builder);

    // Open the host's own socket connecting to the local signaling server
    let room_url = format!("ws://127.0.0.1:{}/rts_room", SIGNALING_PORT);
    commands.open_socket(matchbox_transport::build_socket(&room_url));

    let session_code = format!("ws://{}:{}/rts_room", primary_ip, SIGNALING_PORT);
    info!(
        "Hosting on {} (signaling port {})",
        session_code, SIGNALING_PORT
    );
    for detected in &all_ips {
        let vpn_tag = if detected.is_likely_vpn { " [VPN]" } else { "" };
        info!(
            "  Available IP: {} ({}{})",
            detected.ip, detected.name, vpn_tag
        );
    }

    // LAN discovery (UDP broadcast) — advertise ourselves to other LAN peers.
    let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let discovery_addr = format!("0.0.0.0:{}", transport::DISCOVERY_PORT);
    match std::net::UdpSocket::bind(&discovery_addr) {
        Ok(discovery_socket) => {
            let discovery_session_code = session_code.clone();
            let discovery_shutdown = shutdown.clone();
            std::thread::spawn(move || {
                transport::discovery_listener_thread(
                    discovery_socket,
                    host_name,
                    discovery_session_code,
                    discovery_shutdown,
                );
            });
        }
        Err(e) => {
            warn!(
                "Failed to bind LAN discovery socket on {}: {}",
                discovery_addr, e
            );
        }
    }

    commands.insert_resource(HostNetState::default());
    commands.insert_resource(PeerMap::default());
    commands.insert_resource(MatchboxInbox::default());

    commands.insert_resource(NetRole::Host);
    let all_ips_data: Vec<(String, String, bool)> = all_ips
        .iter()
        .map(|d| (d.ip.clone(), d.name.clone(), d.is_likely_vpn))
        .collect();
    commands.insert_resource(LobbyState {
        players: vec![LobbyPlayer {
            player_id: 0,
            name: "Host".to_string(),
            seat_index: 0,
            faction: Faction::PLAYERS[config.local_player_slot],
            color_index: config.local_player_slot as u8,
            is_host: true,
            connected: true,
        }],
        session_code,
        status: LobbyStatus::Waiting,
        all_ips: all_ips_data,
        discovered_hosts: Vec::new(),
        discovery_status: String::new(),
        client_session_code: String::new(),
    });
}

pub(crate) fn stop_hosting(commands: &mut Commands, _host_state: &Option<Res<HostNetState>>) {
    commands.close_socket();
    commands.stop_server();
    commands.remove_resource::<HostNetState>();
    commands.insert_resource(NetRole::Offline);
    commands.insert_resource(LobbyState::default());
}

pub(crate) fn stop_client(commands: &mut Commands, _client_state: &Option<Res<ClientNetState>>) {
    commands.close_socket();
    commands.remove_resource::<ClientNetState>();
    commands.insert_resource(NetRole::Offline);
    commands.insert_resource(LobbyState::default());
}

// ── Connect to Host ──

pub(crate) fn connect_to_host_system(
    interactions: Query<
        (&Interaction, &MenuButton),
        (Changed<Interaction>, Without<ButtonDisabled>),
    >,
    text_inputs: Query<&TextInputField, With<SessionCodeInput>>,
    mut commands: Commands,
    mut lobby: ResMut<LobbyState>,
    mut status_texts: Query<&mut Text, With<LobbyStatusText>>,
    existing_role: Option<Res<NetRole>>,
) {
    // Already online — don't create a second connection.
    if existing_role
        .as_ref()
        .is_some_and(|role| matches!(**role, NetRole::Host | NetRole::Client))
    {
        return;
    }

    for (interaction, btn) in &interactions {
        if *interaction != Interaction::Pressed || btn.0 != MenuAction::ConnectToHost {
            continue;
        }

        let code = if let Ok(input) = text_inputs.single() {
            input.value.trim().to_string()
        } else {
            continue;
        };

        // Store session code for display after page rebuilds
        lobby.client_session_code = code.clone();

        // Resolve session code to a signaling URL
        let signaling_url = resolve_signaling_url(&code);

        for mut text in &mut status_texts {
            **text = format!("Connecting to {}...", signaling_url);
        }

        commands.open_socket(matchbox_transport::build_socket(&signaling_url));
        commands.insert_resource(ClientNetState::default());
        commands.insert_resource(PeerMap::default());
        commands.insert_resource(MatchboxInbox::default());
        commands.insert_resource(NetRole::Client);

        lobby.status = LobbyStatus::Connecting;
        for mut text in &mut status_texts {
            **text = "Connecting via WebRTC...".to_string();
        }
        commands.insert_resource(ConnectionTimer {
            started: 0.0,
            dot_phase: 0,
            dot_timer: 0.0,
        });
    }
}

// ── URL Resolution ──

/// Resolve a session code to a Matchbox signaling URL.
/// - If it starts with `ws://` or `wss://`, use as-is.
/// - If it's `http(s)://host[:port][/path]`, keep the host and map to the signaling port.
/// - If it's `host:port`, preserve the explicit port.
/// - If it's `host`/`ip`, default to the signaling port.
pub(super) fn resolve_signaling_url(code: &str) -> String {
    let trimmed = code.trim();
    if trimmed.starts_with("ws://") || trimmed.starts_with("wss://") {
        return trimmed.to_string();
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let without_scheme = trimmed
            .strip_prefix("http://")
            .or_else(|| trimmed.strip_prefix("https://"))
            .unwrap_or(trimmed);
        let host_port = without_scheme.split('/').next().unwrap_or(without_scheme);
        let host = if host_port.starts_with('[') {
            host_port
                .split_once(']')
                .map(|(addr, _)| format!("{addr}]"))
                .unwrap_or_else(|| host_port.to_string())
        } else {
            host_port
                .split_once(':')
                .map(|(host, _)| host.to_string())
                .unwrap_or_else(|| host_port.to_string())
        };
        return format!("ws://{}:{}/rts_room", host, SIGNALING_PORT);
    }

    match parse_direct_host_port(trimmed, SIGNALING_PORT) {
        Ok((host, port)) => format!("ws://{}:{}/rts_room", host, port),
        Err(_) => format!("ws://{}:{}/rts_room", trimmed, SIGNALING_PORT),
    }
}

fn parse_direct_host_port(code: &str, default_port: u16) -> Result<(String, u16), String> {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return Err("Please enter a session code".to_string());
    }
    if trimmed.contains("://") || trimmed.contains('/') || trimmed.contains('?') {
        return Err("Session code must be a host[:port] or hosted session code".to_string());
    }

    let (host, port) = if let Some((h, port_str)) = trimmed.split_once(':') {
        let h = h.trim();
        if h.is_empty() {
            return Err("Session host is missing".to_string());
        }
        let port = port_str
            .trim()
            .parse::<u16>()
            .map_err(|_| "Session port must be a valid number".to_string())?;
        (h.to_string(), port)
    } else {
        (trimmed.to_string(), default_port)
    };

    // If the host is numeric-dot notation, treat it as IPv4 and validate it.
    if host.contains('.') {
        let octets: Vec<&str> = host.split('.').collect();
        let all_numeric = octets
            .iter()
            .all(|octet| !octet.is_empty() && octet.chars().all(|ch| ch.is_ascii_digit()));
        if all_numeric
            && (octets.len() != 4 || !octets.iter().all(|octet| octet.parse::<u8>().is_ok()))
        {
            return Err(format!(
                "Invalid IP address '{}'. Expected format: 1.2.3.4:port",
                host
            ));
        }
    }

    Ok((host, port))
}

// ── LAN Discovery ──

pub(crate) fn refresh_lan_hosts_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<DiscoverLanHostsButton>)>,
    mut commands: Commands,
    mut lobby: ResMut<LobbyState>,
) {
    let mut pressed = false;
    for interaction in &interactions {
        if *interaction == Interaction::Pressed {
            pressed = true;
        }
    }
    if !pressed {
        return;
    }

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let hosts =
            multiplayer::transport::discover_lan_hosts(std::time::Duration::from_millis(900))
                .into_iter()
                .map(|(name, session_code)| multiplayer::DiscoveredHost { name, session_code })
                .collect();
        let _ = tx.send(hosts);
    });

    lobby.discovered_hosts.clear();
    lobby.discovery_status = "Scanning LAN for hosts...".to_string();
    commands.insert_resource(JoinDiscoveryScan {
        rx: std::sync::Mutex::new(rx),
    });
    commands.insert_resource(MenuDirty);
}

pub(crate) fn poll_lan_discovery_results_system(
    scan: Option<Res<JoinDiscoveryScan>>,
    mut commands: Commands,
    mut lobby: ResMut<LobbyState>,
) {
    let Some(scan) = scan else { return };
    let rx = scan.rx.lock().unwrap();
    match rx.try_recv() {
        Ok(hosts) => {
            lobby.discovered_hosts = hosts;
            lobby.discovery_status = if lobby.discovered_hosts.is_empty() {
                "No LAN hosts found. Use direct IP:port for VPN or manual join.".to_string()
            } else {
                format!(
                    "Found {} LAN host(s). Select one to autofill the code.",
                    lobby.discovered_hosts.len()
                )
            };
            commands.remove_resource::<JoinDiscoveryScan>();
            commands.insert_resource(MenuDirty);
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            lobby.discovery_status = "LAN scan failed.".to_string();
            commands.remove_resource::<JoinDiscoveryScan>();
            commands.insert_resource(MenuDirty);
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {}
    }
}

pub(crate) fn select_discovered_host_system(
    interactions: Query<(&Interaction, &DiscoveredHostButton), Changed<Interaction>>,
    lobby: Res<LobbyState>,
    mut inputs: Query<&mut TextInputField, With<SessionCodeInput>>,
) {
    for (interaction, button) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(host) = lobby.discovered_hosts.get(button.0) else {
            continue;
        };
        let Ok(mut field) = inputs.single_mut() else {
            continue;
        };
        field.value = host.session_code.clone();
        field.cursor_pos = field.value.len();
        field.selection_anchor = None;
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_signaling_url_accepts_http_host_page_url() {
        assert_eq!(
            resolve_signaling_url("http://192.168.1.5:7880"),
            "ws://192.168.1.5:3536/rts_room"
        );
    }

    #[test]
    fn resolve_signaling_url_accepts_http_host_page_url_with_path() {
        assert_eq!(
            resolve_signaling_url("http://192.168.1.5:7880/index.html"),
            "ws://192.168.1.5:3536/rts_room"
        );
    }

    #[test]
    fn resolve_signaling_url_preserves_explicit_direct_port() {
        assert_eq!(
            resolve_signaling_url("10.0.4.127:4545"),
            "ws://10.0.4.127:4545/rts_room"
        );
    }

    #[test]
    fn parse_direct_host_port_defaults_native_port() {
        assert_eq!(
            parse_direct_host_port("10.0.0.5", SIGNALING_PORT),
            Ok(("10.0.0.5".to_string(), SIGNALING_PORT))
        );
    }

    #[test]
    fn parse_direct_host_port_accepts_explicit_port() {
        assert_eq!(
            parse_direct_host_port("10.0.0.5:9000", SIGNALING_PORT),
            Ok(("10.0.0.5".to_string(), 9000))
        );
    }

    #[test]
    fn parse_direct_host_port_rejects_invalid_port() {
        assert_eq!(
            parse_direct_host_port("10.0.0.5:notaport", SIGNALING_PORT),
            Err("Session port must be a valid number".to_string())
        );
    }

    #[test]
    fn parse_direct_host_port_rejects_five_octet_ip() {
        assert!(parse_direct_host_port("100.103.13.12.7878:7878", SIGNALING_PORT).is_err());
    }

    #[test]
    fn parse_direct_host_port_rejects_malformed_ip_no_port() {
        assert!(parse_direct_host_port("100.103.13.12.7878", SIGNALING_PORT).is_err());
    }

    #[test]
    fn parse_direct_host_port_accepts_dotted_hostname() {
        assert_eq!(
            parse_direct_host_port("0.tcp.eu.ngrok.io:17167", SIGNALING_PORT),
            Ok(("0.tcp.eu.ngrok.io".to_string(), 17167))
        );
    }
}
