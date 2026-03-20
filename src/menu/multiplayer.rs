use bevy::prelude::*;
use game_state::codec;
use game_state::message::ClientMessage;
use std::sync::atomic::Ordering;

use crate::components::*;
use crate::multiplayer::{
    self, ClientNetState, HostNetState, LobbyPlayer, LobbyState, LobbyStatus, NetRole, debug_tap,
};
use crate::theme;
use crate::ui::fonts::{self, UiFonts};
use crate::ui::menu_helpers::*;

use super::*;
use super::pages;

/// Pick the first available faction not already taken by any lobby player.
fn next_available_faction(lobby: &LobbyState) -> (Faction, u8) {
    let taken: std::collections::HashSet<Faction> =
        lobby.players.iter().map(|p| p.faction).collect();
    for (i, &f) in Faction::PLAYERS.iter().enumerate() {
        if !taken.contains(&f) {
            return (f, i as u8);
        }
    }
    // Fallback (lobby full) — shouldn't happen with 4-player cap
    (Faction::Player2, 1)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum JoinTarget {
    Tcp { host: String, port: u16 },
    WebSocket { url: String },
}

// ── Network Cleanup ──

pub(crate) fn cleanup_network_on_enter_menu(
    mut commands: Commands,
    host_state: Option<Res<HostNetState>>,
    client_state: Option<Res<ClientNetState>>,
    #[cfg(not(target_arch = "wasm32"))] host_factory: Option<Res<HostConnectionFactory>>,
) {
    if let Some(host) = host_state {
        host.shutdown.store(true, Ordering::Relaxed);
    }
    if let Some(client) = client_state {
        let seq = {
            let mut s = client.seq.lock().unwrap();
            *s += 1;
            *s
        };
        let leave_msg = ClientMessage::LeaveNotice {
            seq,
            timestamp: 0.0,
        };
        if let Ok(json) = codec::encode(&leave_msg) {
            match client.outgoing.send(json) {
                Ok(_) => debug_tap::record_info(
                    "menu_cleanup",
                    format!("queued client leave notice seq={}", seq),
                ),
                Err(e) => debug_tap::record_error(
                    "menu_cleanup",
                    format!("failed to queue leave notice seq={}: {}", seq, e),
                ),
            }
        }
        client.shutdown.store(true, Ordering::Relaxed);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(factory) = host_factory {
        factory.shutdown.store(true, Ordering::Relaxed);
    }

    commands.remove_resource::<HostNetState>();
    commands.remove_resource::<ClientNetState>();
    #[cfg(not(target_arch = "wasm32"))]
    commands.remove_resource::<HostConnectionFactory>();
    #[cfg(target_arch = "wasm32")]
    commands.remove_resource::<multiplayer::WasmClientSocket>();
    commands.remove_resource::<PendingGameStart>();
    commands.insert_resource(NetRole::Offline);
    commands.insert_resource(LobbyState::default());
}

// ── Multiplayer Page ──

pub(crate) fn spawn_multiplayer_page(
    commands: &mut Commands,
    container: Entity,
    fonts: &UiFonts,
) {
    spawn_page_header(commands, container, "MULTIPLAYER", MenuButton(MenuAction::Back), fonts);

    spawn_animated_section_divider(commands, container, "NETWORK GAME", fonts);

    let desc_text = if cfg!(target_arch = "wasm32") {
        "Join a hosted session from the web client"
    } else {
        "Play with others on your network or via VPN"
    };
    let desc = commands
        .spawn((
            Text::new(desc_text),
            TextFont {
                font_size: theme::FONT_MEDIUM,
                ..default()
            },
            TextColor(theme::TEXT_SECONDARY),
            Node {
                margin: UiRect::bottom(Val::Px(20.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(desc);

    #[cfg(not(target_arch = "wasm32"))]
    {
        let host_btn = spawn_styled_button(
            commands,
            "HOST GAME",
            MenuButton(MenuAction::HostGame),
            true,
            fonts,
        );
        commands.entity(container).add_child(host_btn);
    }

    let join_btn = spawn_styled_button(
        commands,
        "JOIN GAME",
        MenuButton(MenuAction::JoinGame),
        false,
        fonts,
    );
    commands.entity(container).add_child(join_btn);
}

// ── Host Lobby Page ──

pub(crate) fn spawn_host_lobby_page(
    commands: &mut Commands,
    container: Entity,
    config: &GameSetupConfig,
    fonts: &UiFonts,
) {
    spawn_page_header(commands, container, "HOST LOBBY", MenuButton(MenuAction::CancelHost), fonts);

    spawn_animated_section_divider(commands, container, "SESSION CODE", fonts);

    let code_row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            column_gap: Val::Px(12.0),
            margin: UiRect::vertical(Val::Px(8.0)),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                SessionCodeText,
                Text::new("Starting..."),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(theme::ACCENT),
            ));
            parent
                .spawn((
                    CopyCodeButton,
                    Button,
                    ButtonAnimState::new(theme::BTN_PRIMARY.to_srgba().to_f32_array()),
                    ButtonStyle::Filled,
                    Node {
                        padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                        ..default()
                    },
                    BackgroundColor(theme::BTN_PRIMARY),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        CopyCodeLabel,
                        Text::new("COPY"),
                        TextFont {
                            font_size: theme::FONT_MEDIUM,
                            ..default()
                        },
                        TextColor(theme::TEXT_SECONDARY),
                        Pickable::IGNORE,
                    ));
                });
        })
        .id();
    commands.entity(container).add_child(code_row);

    let hint = commands
        .spawn((
            Text::new("Share this code with native players on your network\nFor VPN/Hamachi: use the VPN IP shown below"),
            TextFont {
                font_size: theme::FONT_SMALL,
                ..default()
            },
            TextColor(theme::TEXT_SECONDARY),
            Node {
                margin: UiRect::bottom(Val::Px(4.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(hint);

    // Web client URL (if dist/ is available)
    let web_hint = commands
        .spawn((
            WebClientUrlText,
            Text::new(""),
            TextFont {
                font_size: theme::FONT_MEDIUM,
                ..default()
            },
            TextColor(Color::srgb(0.4, 0.9, 0.4)),
            Node {
                margin: UiRect::bottom(Val::Px(4.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(web_hint);

    let ip_list = commands
        .spawn((
            HostIpList,
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                margin: UiRect::bottom(Val::Px(12.0)),
                row_gap: Val::Px(2.0),
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(ip_list);

    spawn_animated_section_divider(commands, container, "FACTIONS", fonts);

    for i in 0..4 {
        pages::spawn_slot_card(commands, container, i, config, true);
    }

    // ── World Settings ──

    spawn_animated_section_divider(commands, container, "WORLD", fonts);

    let map_idx = match config.map_size {
        MapSize::Small => 0,
        MapSize::Medium => 1,
        MapSize::Large => 2,
    };
    spawn_selector_row(
        commands,
        container,
        "Map Size:",
        &["Small", "Medium", "Large"],
        map_idx,
        SelectorField::MapSize,
    );

    let res_idx = match config.resource_density {
        ResourceDensity::Sparse => 0,
        ResourceDensity::Normal => 1,
        ResourceDensity::Dense => 2,
    };
    spawn_selector_row(
        commands,
        container,
        "Resources:",
        &["Sparse", "Normal", "Dense"],
        res_idx,
        SelectorField::ResourceDensity,
    );

    let day_idx = DAY_CYCLE_OPTIONS
        .iter()
        .position(|&(v, _)| (v - config.day_cycle_secs).abs() < 1.0)
        .unwrap_or(1);
    let day_labels: Vec<&str> = DAY_CYCLE_OPTIONS.iter().map(|&(_, l)| l).collect();
    spawn_selector_row(
        commands,
        container,
        "Day Cycle:",
        &day_labels,
        day_idx,
        SelectorField::DayCycle,
    );

    let start_idx = STARTING_RES_OPTIONS
        .iter()
        .position(|&(v, _)| (v - config.starting_resources_mult).abs() < 0.01)
        .unwrap_or(1);
    let start_labels: Vec<&str> = STARTING_RES_OPTIONS.iter().map(|&(_, l)| l).collect();
    spawn_selector_row(
        commands,
        container,
        "Start Res:",
        &start_labels,
        start_idx,
        SelectorField::StartingRes,
    );

    spawn_animated_section_divider(commands, container, "", fonts);

    let status = commands
        .spawn((
            LobbyStatusText,
            Text::new("Waiting for players..."),
            TextFont {
                font_size: theme::FONT_MEDIUM,
                ..default()
            },
            TextColor(theme::TEXT_SECONDARY),
            Node {
                margin: UiRect::vertical(Val::Px(8.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(status);

    let start_btn = commands
        .spawn((
            MenuButton(MenuAction::StartMultiplayer),
            Button,
            ButtonAnimState::new(theme::ACCENT.to_srgba().to_f32_array()),
            ButtonStyle::Filled,
            UiGlowPulse {
                color: theme::ACCENT,
                intensity: 0.6,
            },
            Node {
                width: Val::Px(280.0),
                height: Val::Px(80.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                margin: UiRect::top(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(theme::ACCENT),
            BoxShadow::new(
                Color::srgba(0.29, 0.62, 1.0, 0.3),
                Val::Px(0.0),
                Val::Px(0.0),
                Val::Px(0.0),
                Val::Px(8.0),
            ),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("START GAME"),
                fonts::heading(fonts, theme::FONT_BUTTON),
                TextColor(Color::WHITE),
                Pickable::IGNORE,
            ));
        })
        .id();
    commands.entity(container).add_child(start_btn);
}

// ── Join Lobby Page ──

pub(crate) fn spawn_join_lobby_page(
    commands: &mut Commands,
    container: Entity,
    config: &GameSetupConfig,
    fonts: &UiFonts,
) {
    spawn_page_header(
        commands,
        container,
        "JOIN GAME",
        MenuButton(MenuAction::BackToMultiplayer),
        fonts,
    );

    spawn_animated_section_divider(commands, container, "SESSION CODE", fonts);

    let input_row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(6.0)),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text::new("Code:"),
                TextFont {
                    font_size: theme::FONT_MEDIUM,

                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
                Node {
                    width: Val::Px(80.0),
                    ..default()
                },
            ));

            parent
                .spawn((
                    SessionCodeInput,
                    TextInputField {
                        value: String::new(),
                        cursor_pos: 0,
                        max_len: 45,
                    },
                    Button,
                    Node {
                        width: Val::Px(280.0),
                        height: Val::Px(32.0),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        align_items: AlignItems::Center,
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(theme::INPUT_BG),
                    BorderColor::all(theme::INPUT_BORDER),
                ))
                .with_children(|input| {
                    input.spawn((
                        Text::new(""),
                        TextFont {
                            font_size: theme::FONT_MEDIUM,
                            ..default()
                        },
                        TextColor(theme::TEXT_PRIMARY),
                        Pickable::IGNORE,
                    ));
                    input.spawn((
                        TextInputCursor,
                        Text::new("|"),
                        TextFont {
                            font_size: theme::FONT_MEDIUM,
                            ..default()
                        },
                        TextColor(Color::NONE),
                        Pickable::IGNORE,
                    ));
                });
        })
        .id();
    commands.entity(container).add_child(input_row);

    let connect_btn = spawn_styled_button(
        commands,
        "CONNECT",
        MenuButton(MenuAction::ConnectToHost),
        true,
        fonts,
    );
    commands.entity(container).add_child(connect_btn);

    spawn_animated_section_divider(commands, container, "STATUS", fonts);

    let status = commands
        .spawn((
            LobbyStatusText,
            Text::new(if cfg!(target_arch = "wasm32") {
                "Enter a hosted session code and press CONNECT"
            } else {
                "Enter the host's session code and press CONNECT"
            }),
            TextFont {
                font_size: theme::FONT_MEDIUM,
                ..default()
            },
            TextColor(theme::TEXT_SECONDARY),
            Node {
                margin: UiRect::vertical(Val::Px(8.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(status);

    spawn_animated_section_divider(commands, container, "FACTIONS", fonts);

    for i in 0..4 {
        spawn_client_slot_card(commands, container, i, config);
    }

    let dc_btn = spawn_styled_button(
        commands,
        "DISCONNECT",
        MenuButton(MenuAction::Disconnect),
        false,
        fonts,
    );
    commands.entity(container).add_child(dc_btn);
}

/// Read-only slot card for the client join lobby — shows faction config from host.
fn spawn_client_slot_card(
    commands: &mut Commands,
    container: Entity,
    slot_index: usize,
    config: &GameSetupConfig,
) {
    let slot = config.slots[slot_index];
    let faction_color = Faction::PLAYERS[slot_index].color();
    let team = config.player_teams[slot_index];

    let type_label = match slot {
        SlotOccupant::Human => "Human",
        SlotOccupant::Ai(AiDifficulty::Easy) => "AI Easy",
        SlotOccupant::Ai(AiDifficulty::Medium) => "AI Medium",
        SlotOccupant::Ai(AiDifficulty::Hard) => "AI Hard",
        SlotOccupant::Closed => "None",
        SlotOccupant::Open => "Open",
    };

    let team_colors = [
        Color::srgb(0.9, 0.75, 0.2),
        Color::srgb(0.2, 0.75, 0.85),
        Color::srgb(0.85, 0.3, 0.65),
        Color::srgb(0.95, 0.5, 0.15),
    ];
    let team_color = team_colors.get(team as usize).copied().unwrap_or(team_colors[0]);

    let card = commands
        .spawn((
            SlotCardContainer(slot_index),
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(10.0)),
                margin: UiRect::vertical(Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                column_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(theme::BG_SURFACE),
            BorderColor::all(theme::SEPARATOR),
        ))
        .with_children(|card| {
            // Faction color dot
            card.spawn((
                Node {
                    width: Val::Px(16.0),
                    height: Val::Px(16.0),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(faction_color),
            ));
            // Faction label
            card.spawn((
                Text::new(format!("Player {}", slot_index + 1)),
                TextFont { font_size: theme::FONT_MEDIUM, ..default() },
                TextColor(faction_color),
            ));
            // Type label
            card.spawn((
                Text::new(type_label),
                TextFont { font_size: theme::FONT_SMALL, ..default() },
                TextColor(theme::TEXT_SECONDARY),
            ));
            card.spawn(Node { flex_grow: 1.0, ..default() });
            // Team badge
            if !matches!(slot, SlotOccupant::Closed | SlotOccupant::Open) {
                card.spawn((
                    Node {
                        width: Val::Px(22.0),
                        height: Val::Px(22.0),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(team_color),
                ))
                .with_children(|badge| {
                    badge.spawn((
                        Text::new(format!("{}", team + 1)),
                        TextFont { font_size: 10.0, ..default() },
                        TextColor(Color::WHITE),
                        Pickable::IGNORE,
                    ));
                });
            }
        })
        .id();
    commands.entity(container).add_child(card);
}

// ── Networking helpers ──

const DEFAULT_PORT: u16 = 7878;
const WEB_SESSION_WS_PATH_PREFIX: &str = "/session";

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

    // Validate that the host looks like a valid IPv4 address or hostname
    if host.contains('.') {
        // Looks like an IPv4 address — must be exactly 4 numeric octets
        let octets: Vec<&str> = host.split('.').collect();
        if octets.len() != 4 || !octets.iter().all(|o| o.parse::<u8>().is_ok()) {
            return Err(format!(
                "Invalid IP address '{}'. Expected format: 1.2.3.4:port",
                host
            ));
        }
    }

    Ok((host, port))
}

fn is_valid_hosted_session_code(code: &str) -> bool {
    let trimmed = code.trim();
    let len_ok = (4..=32).contains(&trimmed.len());
    len_ok
        && trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn resolve_native_join_target(code: &str) -> Result<JoinTarget, String> {
    let (host, port) = parse_direct_host_port(code, DEFAULT_PORT)?;
    Ok(JoinTarget::Tcp { host, port })
}

fn resolve_web_join_target(
    code: &str,
    page_protocol: &str,
    page_host: &str,
) -> Result<JoinTarget, String> {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return Err("Please enter a session code".to_string());
    }
    if page_host.trim().is_empty() {
        return Err("Browser origin is unavailable".to_string());
    }

    if trimmed.contains(':') {
        if page_protocol == "https:" {
            return Err(
                "Direct IP session codes are blocked on HTTPS. Use a hosted session code."
                    .to_string(),
            );
        }

        let (host, port) = parse_direct_host_port(trimmed, DEFAULT_PORT)?;
        return Ok(JoinTarget::WebSocket {
            url: format!("ws://{}:{}", host, port + 1),
        });
    }

    if !is_valid_hosted_session_code(trimmed) {
        return Err(
            "Hosted session codes must be 4-32 characters using letters, numbers, - or _."
                .to_string(),
        );
    }

    let ws_scheme = match page_protocol {
        "https:" => "wss",
        "http:" => "ws",
        other => {
            return Err(format!(
                "Unsupported page protocol for WebSocket connection: {}",
                other
            ))
        }
    };

    Ok(JoinTarget::WebSocket {
        url: format!(
            "{}://{}{}/{}/ws",
            ws_scheme, page_host, WEB_SESSION_WS_PATH_PREFIX, trimmed
        ),
    })
}

#[cfg(target_arch = "wasm32")]
fn current_browser_origin() -> Result<(String, String), String> {
    let window = web_sys::window().ok_or_else(|| "Browser window unavailable".to_string())?;
    let location = window.location();
    let protocol = location
        .protocol()
        .map_err(|_| "Failed to read browser protocol".to_string())?;
    let host = location
        .host()
        .map_err(|_| "Failed to read browser host".to_string())?;
    Ok((protocol, host))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn start_hosting(commands: &mut Commands, config: &GameSetupConfig) {
    use crate::multiplayer::transport;
    use std::net::TcpListener;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;
    use std::sync::Arc;

    let all_ips = transport::detect_all_ips();
    let primary_ip = if let Some(vpn) = all_ips.iter().find(|ip| ip.is_likely_vpn) {
        vpn.ip.clone()
    } else {
        transport::detect_lan_ip().unwrap_or_else(|| "127.0.0.1".to_string())
    };
    let addr = format!("0.0.0.0:{}", DEFAULT_PORT);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            warn!("Failed to bind TCP listener on {}: {}", addr, e);
            return;
        }
    };

    let session_code = format!("{}:{}", primary_ip, DEFAULT_PORT);
    info!("Hosting on {}", session_code);
    for detected in &all_ips {
        let vpn_tag = if detected.is_likely_vpn { " [VPN]" } else { "" };
        info!(
            "  Available IP: {} ({}{})",
            detected.ip, detected.name, vpn_tag
        );
    }

    let shutdown = Arc::new(AtomicBool::new(false));
    let (new_client_tx, new_client_rx) = mpsc::channel();
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (dc_tx, dc_rx) = mpsc::channel();

    let cmd_tx_clone = cmd_tx.clone();
    let dc_tx_clone = dc_tx.clone();
    let shutdown_clone = shutdown.clone();

    let shutdown_listener = shutdown.clone();
    std::thread::spawn(move || {
        transport::host_listener_thread(listener, new_client_tx, shutdown_listener);
    });

    // ── WebSocket listener for WASM browser clients ──
    let (ws_client_tx, ws_rx) = mpsc::channel();
    let ws_addr = format!("0.0.0.0:{}", DEFAULT_PORT + transport::WS_PORT_OFFSET);
    match TcpListener::bind(&ws_addr) {
        Ok(ws_listener) => {
            info!("WebSocket listener on {}", ws_addr);
            let ws_cmd_tx = cmd_tx.clone();
            let ws_dc_tx = dc_tx.clone();
            let ws_shutdown = shutdown.clone();
            std::thread::spawn(move || {
                transport::ws_host_listener_thread(
                    ws_listener,
                    ws_cmd_tx,
                    ws_dc_tx,
                    ws_client_tx,
                    ws_shutdown,
                );
            });
        }
        Err(e) => {
            warn!("Failed to bind WebSocket listener on {}: {} — WASM clients won't be able to connect", ws_addr, e);
        }
    }

    // ── HTTP file server for direct LAN web clients ──
    let http_port = DEFAULT_PORT + transport::HTTP_PORT_OFFSET;
    let http_addr = format!("0.0.0.0:{}", http_port);
    let dist_dir = std::env::var("DIST_DIR").unwrap_or_else(|_| "dist".to_string());
    if std::path::Path::new(&dist_dir).is_dir() {
        match TcpListener::bind(&http_addr) {
            Ok(http_listener) => {
                info!(
                    "Serving WASM client at http://{}:{}/",
                    primary_ip, http_port
                );
                let http_shutdown = shutdown.clone();
                std::thread::spawn(move || {
                    transport::host_file_server_thread(http_listener, dist_dir, http_shutdown);
                });
            }
            Err(e) => {
                warn!(
                    "Failed to bind HTTP file server on {}: {} — web clients must load the page elsewhere",
                    http_addr, e
                );
            }
        }
    }

    commands.insert_resource(HostNetState {
        incoming_commands: std::sync::Mutex::new(cmd_rx),
        client_senders: std::sync::Mutex::new(Vec::new()),
        new_clients: std::sync::Mutex::new(new_client_rx),
        new_ws_clients: std::sync::Mutex::new(ws_rx),
        disconnect_rx: std::sync::Mutex::new(dc_rx),
        shutdown: shutdown.clone(),
        seq: std::sync::Mutex::new(0),
    });

    commands.insert_resource(HostConnectionFactory {
        cmd_tx: cmd_tx_clone,
        dc_tx: dc_tx_clone,
        shutdown: shutdown_clone,
    });

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
    });
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource)]
pub(crate) struct HostConnectionFactory {
    cmd_tx: std::sync::mpsc::Sender<(u8, game_state::message::ClientMessage)>,
    dc_tx: std::sync::mpsc::Sender<u8>,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn stop_hosting(
    commands: &mut Commands,
    host_state: &Option<Res<HostNetState>>,
    host_factory: &Option<Res<HostConnectionFactory>>,
) {
    if let Some(host) = host_state {
        host.shutdown.store(true, Ordering::Relaxed);
    }
    if let Some(factory) = host_factory {
        factory.shutdown.store(true, Ordering::Relaxed);
    }
    commands.remove_resource::<HostNetState>();
    commands.remove_resource::<HostConnectionFactory>();
    commands.insert_resource(NetRole::Offline);
    commands.insert_resource(LobbyState::default());
}

pub(crate) fn stop_client(
    commands: &mut Commands,
    client_state: &Option<Res<ClientNetState>>,
) {
    if let Some(client) = client_state {
        let seq = {
            let mut s = client.seq.lock().unwrap();
            *s += 1;
            *s
        };
        let leave_msg = ClientMessage::LeaveNotice {
            seq,
            timestamp: 0.0,
        };
        if let Ok(json) = codec::encode(&leave_msg) {
            let _ = client.outgoing.send(json);
        }
        client.shutdown.store(true, Ordering::Relaxed);
    }
    commands.remove_resource::<ClientNetState>();
    #[cfg(target_arch = "wasm32")]
    commands.remove_resource::<multiplayer::WasmClientSocket>();
    commands.insert_resource(NetRole::Offline);
    commands.insert_resource(LobbyState::default());
}

#[derive(Resource)]
pub(crate) struct PendingGameStart;

pub(crate) fn connect_to_host_system(
    interactions: Query<(&Interaction, &MenuButton), Changed<Interaction>>,
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

        // ── Native: connect via TCP ──
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (host, port) = match resolve_native_join_target(&code) {
                Ok(JoinTarget::Tcp { host, port }) => (host, port),
                Ok(JoinTarget::WebSocket { .. }) => unreachable!(),
                Err(e) => {
                    lobby.status = LobbyStatus::Failed(e.clone());
                    for mut text in &mut status_texts {
                        **text = e.clone();
                    }
                    continue;
                }
            };

            let addr = format!("{}:{}", host, port);
            for mut text in &mut status_texts {
                **text = format!("Connecting to {}...", addr);
            }

            match std::net::TcpStream::connect_timeout(
                &addr.parse().unwrap_or_else(|_| {
                    std::net::SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT))
                }),
                std::time::Duration::from_secs(10),
            ) {
                Ok(stream) => {
                    stream.set_nodelay(true).ok();
                    multiplayer::transport::configure_keepalive(&stream);

                    let shutdown =
                        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let (incoming_tx, incoming_rx) = std::sync::mpsc::channel();
                    let (outgoing_tx, outgoing_rx) = std::sync::mpsc::channel();

                    let read_stream =
                        stream.try_clone().expect("Failed to clone TCP stream");
                    let write_stream = stream;

                    let shutdown_r = shutdown.clone();
                    std::thread::spawn(move || {
                        multiplayer::transport::client_reader_thread(
                            read_stream,
                            incoming_tx,
                            shutdown_r,
                        );
                    });

                    let shutdown_w = shutdown.clone();
                    std::thread::spawn(move || {
                        multiplayer::transport::client_writer_thread_fn(
                            write_stream,
                            outgoing_rx,
                            shutdown_w,
                        );
                    });

                    let join_msg = game_state::message::ClientMessage::JoinRequest {
                        seq: 0,
                        timestamp: 0.0,
                        player_name: "Client".to_string(),
                        preferred_faction_index: None,
                    };
                    if let Ok(json) = codec::encode(&join_msg) {
                        let _ = outgoing_tx.send(json);
                    }

                    commands.insert_resource(ClientNetState {
                        incoming: std::sync::Mutex::new(incoming_rx),
                        outgoing: outgoing_tx,
                        shutdown,
                        player_id: 0,
                        seat_index: 0,
                        my_faction: Faction::Player2,
                        color_index: 0,
                        seq: std::sync::Mutex::new(0),
                        session_token: 0,
                    });
                    commands.insert_resource(NetRole::Client);

                    lobby.status = LobbyStatus::Connected;
                    for mut text in &mut status_texts {
                        **text = "Connected! Waiting for host to start...".to_string();
                    }
                }
                Err(e) => {
                    lobby.status = LobbyStatus::Failed(e.to_string());
                    for mut text in &mut status_texts {
                        **text = format!("Failed: {}", e);
                    }
                }
            }
        }

        // ── WASM: connect via WebSocket ──
        #[cfg(target_arch = "wasm32")]
        {
            let (page_protocol, page_host) = match current_browser_origin() {
                Ok(parts) => parts,
                Err(e) => {
                    lobby.status = LobbyStatus::Failed(e.clone());
                    for mut text in &mut status_texts {
                        **text = e.clone();
                    }
                    continue;
                }
            };

            let ws_url = match resolve_web_join_target(&code, &page_protocol, &page_host) {
                Ok(JoinTarget::WebSocket { url }) => url,
                Ok(JoinTarget::Tcp { .. }) => unreachable!(),
                Err(e) => {
                    lobby.status = LobbyStatus::Failed(e.clone());
                    for mut text in &mut status_texts {
                        **text = e.clone();
                    }
                    continue;
                }
            };

            for mut text in &mut status_texts {
                **text = format!("Connecting via WebSocket to {}...", ws_url);
            }

            let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let (incoming_tx, incoming_rx) = std::sync::mpsc::channel();
            let (outgoing_tx, outgoing_rx) = std::sync::mpsc::channel();

            match multiplayer::transport::wasm_ws_connect(
                &ws_url,
                incoming_tx,
                shutdown.clone(),
                "WebClient".to_string(),
            ) {
                Ok(ws) => {
                    commands.insert_resource(multiplayer::WasmClientSocket {
                        ws,
                        outgoing_rx: std::sync::Mutex::new(outgoing_rx),
                    });

                    commands.insert_resource(ClientNetState {
                        incoming: std::sync::Mutex::new(incoming_rx),
                        outgoing: outgoing_tx,
                        shutdown,
                        player_id: 0,
                        seat_index: 0,
                        my_faction: Faction::Player2,
                        color_index: 0,
                        seq: std::sync::Mutex::new(0),
                        session_token: 0,
                    });
                    commands.insert_resource(NetRole::Client);

                    lobby.status = LobbyStatus::Connecting;
                    for mut text in &mut status_texts {
                        **text = "Connecting via WebSocket...".to_string();
                    }
                }
                Err(e) => {
                    lobby.status = LobbyStatus::Failed(e.clone());
                    for mut text in &mut status_texts {
                        **text = format!("WebSocket failed: {}", e);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_direct_host_port_defaults_native_port() {
        assert_eq!(
            parse_direct_host_port("10.0.0.5", DEFAULT_PORT),
            Ok(("10.0.0.5".to_string(), DEFAULT_PORT))
        );
    }

    #[test]
    fn parse_direct_host_port_accepts_explicit_port() {
        assert_eq!(
            parse_direct_host_port("10.0.0.5:9000", DEFAULT_PORT),
            Ok(("10.0.0.5".to_string(), 9000))
        );
    }

    #[test]
    fn parse_direct_host_port_rejects_invalid_port() {
        assert_eq!(
            parse_direct_host_port("10.0.0.5:notaport", DEFAULT_PORT),
            Err("Session port must be a valid number".to_string())
        );
    }

    #[test]
    fn resolve_web_join_target_builds_same_origin_wss_url() {
        assert_eq!(
            resolve_web_join_target("ABCD12", "https:", "rts-game.fly.dev"),
            Ok(JoinTarget::WebSocket {
                url: "wss://rts-game.fly.dev/session/ABCD12/ws".to_string(),
            })
        );
    }

    #[test]
    fn resolve_web_join_target_rejects_direct_ip_code_on_https() {
        assert_eq!(
            resolve_web_join_target("100.103.13.12:7878", "https:", "rts-game.fly.dev"),
            Err(
                "Direct IP session codes are blocked on HTTPS. Use a hosted session code."
                    .to_string(),
            )
        );
    }

    #[test]
    fn resolve_web_join_target_allows_direct_ws_on_http_for_local_dev() {
        assert_eq!(
            resolve_web_join_target("127.0.0.1:7878", "http:", "localhost:8080"),
            Ok(JoinTarget::WebSocket {
                url: "ws://127.0.0.1:7879".to_string(),
            })
        );
    }

    #[test]
    fn parse_direct_host_port_rejects_five_octet_ip() {
        assert!(parse_direct_host_port("100.103.13.12.7878:7878", DEFAULT_PORT).is_err());
    }

    #[test]
    fn parse_direct_host_port_rejects_malformed_ip_no_port() {
        assert!(parse_direct_host_port("100.103.13.12.7878", DEFAULT_PORT).is_err());
    }

    #[test]
    fn resolve_web_join_target_rejects_invalid_hosted_code_characters() {
        assert_eq!(
            resolve_web_join_target("bad/code", "https:", "rts-game.fly.dev"),
            Err(
                "Hosted session codes must be 4-32 characters using letters, numbers, - or _."
                    .to_string(),
            )
        );
    }
}

pub(crate) fn copy_session_code_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<CopyCodeButton>)>,
    lobby: Res<LobbyState>,
    mut labels: Query<&mut Text, With<CopyCodeLabel>>,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed && !lobby.session_code.is_empty() {
            clipboard_write(&lobby.session_code);
            for mut text in &mut labels {
                **text = "COPIED!".to_string();
            }
        }
    }
}

pub(crate) fn update_lobby_ui(
    page: Res<MenuPage>,
    mut lobby: ResMut<LobbyState>,
    host_state: Option<Res<HostNetState>>,
    #[cfg(not(target_arch = "wasm32"))] host_factory: Option<Res<HostConnectionFactory>>,
    client_state: Option<ResMut<ClientNetState>>,
    pending_start: Option<Res<PendingGameStart>>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
    mut session_code_texts: Query<&mut Text, With<SessionCodeText>>,
    mut status_texts: Query<
        &mut Text,
        (With<LobbyStatusText>, Without<SessionCodeText>),
    >,
    mut config: ResMut<GameSetupConfig>,
    ip_list_q: Query<Entity, (With<HostIpList>, Without<HostIpListPopulated>)>,
    mut session_tokens: ResMut<multiplayer::SessionTokens>,
    roots: Query<Entity, With<MenuRoot>>,
) {
    // Update session code display
    if *page == MenuPage::HostLobby {
        for mut text in &mut session_code_texts {
            if **text != lobby.session_code && !lobby.session_code.is_empty() {
                **text = lobby.session_code.clone();
            }
        }
        if !lobby.all_ips.is_empty() {
            for ip_list_entity in &ip_list_q {
                commands
                    .entity(ip_list_entity)
                    .insert(HostIpListPopulated);
                for (ip, iface_name, is_vpn) in &lobby.all_ips {
                    let label = if *is_vpn {
                        format!("{} ({}) [VPN]", ip, iface_name)
                    } else {
                        format!("{} ({})", ip, iface_name)
                    };
                    let color = if *is_vpn {
                        Color::srgb(0.4, 0.9, 0.4)
                    } else {
                        theme::TEXT_SECONDARY
                    };
                    let child = commands
                        .spawn((
                            Text::new(label),
                            TextFont {
                                font_size: theme::FONT_SMALL,
                                ..default()
                            },
                            TextColor(color),
                        ))
                        .id();
                    commands.entity(ip_list_entity).add_child(child);
                }
            }
        }
    }

    // ── Host: check for new clients (native only — WASM can't host) ──
    #[cfg(not(target_arch = "wasm32"))]
    if let (Some(host), Some(factory)) = (host_state.as_ref(), host_factory.as_ref()) {
        let new_clients_rx = host.new_clients.lock().unwrap();
        let mut lobby_changed = false;
        loop {
            match new_clients_rx.try_recv() {
                Ok(event) => {
                    let player_id = event.player_id;
                    info!("New client {} in lobby", player_id);

                    let seat_index = lobby.players.len().min(3) as u8;
                    let (faction, faction_idx) = next_available_faction(&lobby);
                    let color_index = faction_idx;

                    lobby.players.push(LobbyPlayer {
                        player_id,
                        name: format!("Player {}", player_id),
                        seat_index,
                        faction,
                        color_index,
                        is_host: false,
                        connected: true,
                    });

                    let read_stream = event
                        .stream
                        .try_clone()
                        .expect("Failed to clone client stream");
                    let write_stream = event.stream;

                    let (writer_tx, writer_rx) = std::sync::mpsc::channel();

                    let cmd_tx = factory.cmd_tx.clone();
                    let dc_tx = factory.dc_tx.clone();
                    let shutdown = factory.shutdown.clone();

                    std::thread::spawn(move || {
                        multiplayer::transport::host_client_reader_thread(
                            read_stream,
                            cmd_tx,
                            dc_tx,
                            player_id,
                            shutdown,
                        );
                    });

                    let shutdown_w = factory.shutdown.clone();
                    std::thread::spawn(move || {
                        multiplayer::transport::client_writer_thread(
                            write_stream,
                            writer_rx,
                            shutdown_w,
                        );
                    });

                    host.client_senders
                        .lock()
                        .unwrap()
                        .push((player_id, writer_tx));
                    lobby_changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }

        // ── Check for new WebSocket clients (WASM browser clients) ──
        let new_ws_rx = host.new_ws_clients.lock().unwrap();
        loop {
            match new_ws_rx.try_recv() {
                Ok(event) => {
                    let player_id = event.player_id;
                    info!("New WebSocket client {} in lobby", player_id);

                    let seat_index = lobby.players.len().min(3) as u8;
                    let (faction, faction_idx) = next_available_faction(&lobby);
                    let color_index = faction_idx;

                    lobby.players.push(LobbyPlayer {
                        player_id,
                        name: format!("Player {}", player_id),
                        seat_index,
                        faction,
                        color_index,
                        is_host: false,
                        connected: true,
                    });

                    // WS client handler already spawned read/write threads —
                    // just register the writer channel
                    host.client_senders
                        .lock()
                        .unwrap()
                        .push((player_id, event.writer_tx));
                    lobby_changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }

        let incoming_commands = host.incoming_commands.lock().unwrap();
        loop {
            match incoming_commands.try_recv() {
                Ok((
                    player_id,
                    game_state::message::ClientMessage::JoinRequest {
                        player_name, ..
                    },
                )) => {
                    if let Some(player) =
                        lobby.players.iter_mut().find(|p| p.player_id == player_id)
                    {
                        if !player_name.trim().is_empty() {
                            player.name = player_name;
                        }

                        let faction_index = Faction::PLAYERS
                            .iter()
                            .position(|f| *f == player.faction)
                            .unwrap_or(0) as u8;
                        let seat_index = player.seat_index;
                        let color_index = player.color_index;

                        let seq = {
                            let mut s = host.seq.lock().unwrap();
                            *s += 1;
                            *s
                        };
                        let token = session_tokens.generate(player_id);
                        let msg = game_state::message::ServerMessage::Event {
                            seq,
                            timestamp: 0.0,
                            events: vec![game_state::message::GameEvent::JoinAccepted {
                                player_id,
                                seat_index,
                                faction_index,
                                color_index,
                                session_token: token,
                            }],
                        };
                        if let Ok(json) = codec::encode(&msg) {
                            let senders = host.client_senders.lock().unwrap();
                            if let Some((_, sender)) =
                                senders.iter().find(|(id, _)| *id == player_id)
                            {
                                let _ = sender.send(json);
                            }
                        }
                        lobby_changed = true;
                    }
                }
                Ok((
                    player_id,
                    game_state::message::ClientMessage::LeaveNotice { .. },
                )) => {
                    if let Some(player) =
                        lobby.players.iter_mut().find(|p| p.player_id == player_id)
                    {
                        player.connected = false;
                        lobby_changed = true;
                    }
                }
                Ok((_player_id, game_state::message::ClientMessage::Input { .. })) => {}
                Ok((_player_id, game_state::message::ClientMessage::Ping { .. })) => {}
                Ok((_player_id, game_state::message::ClientMessage::Reconnect { .. })) => {
                    // Reconnection during lobby phase — not yet supported, treat as new join
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }

        if lobby_changed {
            // Auto-close AI/Open slots that conflict with new human players
            for player in &lobby.players {
                if player.connected {
                    let idx = Faction::PLAYERS
                        .iter()
                        .position(|f| *f == player.faction)
                        .unwrap_or(0);
                    if !matches!(config.slots[idx], SlotOccupant::Human) {
                        config.slots[idx] = SlotOccupant::Human;
                    }
                }
            }
            broadcast_lobby_update(&lobby, host, &config);
        }

        if *page == MenuPage::HostLobby {
            let connected = lobby.players.iter().filter(|p| p.connected).count();
            for mut text in &mut status_texts {
                **text = format!(
                    "{} player(s) in lobby{}",
                    connected,
                    if connected >= 2 {
                        " — ready to start!"
                    } else {
                        ""
                    }
                );
            }
        }

        // ── Host: handle PendingGameStart ──
        if pending_start.is_some() {
            if config.map_seed == 0 {
                config.map_seed = rand::random::<u64>();
                info!("Host resolved random map seed: {}", config.map_seed);
            }

            // Sync slots from lobby: connected human players override slot occupants
            for player in &lobby.players {
                if player.connected {
                    let faction_idx = Faction::PLAYERS
                        .iter()
                        .position(|f| *f == player.faction)
                        .unwrap_or(0);
                    config.slots[faction_idx] = SlotOccupant::Human;
                    if player.is_host {
                        config.local_player_slot = faction_idx;
                    }
                }
            }
            info!(
                "Multiplayer start: slots={:?}, local_player_slot={}",
                config.slots, config.local_player_slot
            );

            let config_json =
                serde_json::to_string(&SerializableGameConfig::from_config(&config, &lobby))
                    .unwrap_or_default();

            let start_event = game_state::message::ServerMessage::Event {
                seq: 0,
                timestamp: 0.0,
                events: vec![game_state::message::GameEvent::GameStart { config_json }],
            };
            if let Ok(json) = codec::encode(&start_event) {
                let senders = host.client_senders.lock().unwrap();
                for (_id, sender) in senders.iter() {
                    let _ = sender.send(json.clone());
                }
            }

            commands.remove_resource::<PendingGameStart>();
            next_state.set(AppState::InGame);
        }
    }

    // ── Client: detect dead connection (async WebSocket failure) ──
    if let Some(ref client) = client_state {
        if client
            .shutdown
            .load(std::sync::atomic::Ordering::Relaxed)
            && !matches!(lobby.status, LobbyStatus::Connected)
        {
            lobby.status =
                LobbyStatus::Failed("Connection lost — WebSocket closed".to_string());
            for mut text in &mut status_texts {
                **text = "Connection failed — host unreachable".to_string();
            }
            commands.remove_resource::<ClientNetState>();
            commands.remove_resource::<NetRole>();
            #[cfg(target_arch = "wasm32")]
            commands.remove_resource::<multiplayer::WasmClientSocket>();
            return;
        }
    }

    // ── Client: poll incoming for lobby updates and game start ──
    if let Some(mut client) = client_state {
        let mut incoming = Vec::new();
        {
            let rx = client.incoming.lock().unwrap();
            loop {
                match rx.try_recv() {
                    Ok(msg) => incoming.push(msg),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                }
            }
        }

        for msg in incoming {
            match msg {
                game_state::message::ServerMessage::Event { events, .. } => {
                    for event in &events {
                        match event {
                            game_state::message::GameEvent::JoinAccepted {
                                player_id,
                                seat_index,
                                faction_index,
                                color_index,
                                session_token,
                            } => {
                                client.player_id = *player_id;
                                client.seat_index = *seat_index;
                                client.my_faction = Faction::PLAYERS
                                    .get(*faction_index as usize)
                                    .copied()
                                    .unwrap_or(Faction::Player2);
                                client.color_index = *color_index;
                                client.session_token = *session_token;
                                info!(
                                    "Join accepted: player_id={}, seat={}, faction={:?}, color={}, token={}",
                                    client.player_id,
                                    client.seat_index,
                                    client.my_faction,
                                    client.color_index,
                                    client.session_token,
                                );
                            }
                            game_state::message::GameEvent::LobbyUpdate {
                                players,
                                slots,
                                player_teams,
                            } => {
                                lobby.players.clear();
                                for p in players {
                                    lobby.players.push(LobbyPlayer {
                                        player_id: p.player_id,
                                        name: p.name.clone(),
                                        seat_index: p.seat_index,
                                        faction: Faction::PLAYERS
                                            .get(p.faction_index as usize)
                                            .copied()
                                            .unwrap_or(Faction::Neutral),
                                        color_index: p.color_index,
                                        is_host: p.is_host,
                                        connected: p.connected,
                                    });
                                }
                                // Apply slot config and teams from host
                                config.slots = slots.map(|s| match s {
                                    0 => SlotOccupant::Human,
                                    1 => SlotOccupant::Ai(AiDifficulty::Easy),
                                    2 => SlotOccupant::Ai(AiDifficulty::Medium),
                                    3 => SlotOccupant::Ai(AiDifficulty::Hard),
                                    4 => SlotOccupant::Closed,
                                    _ => SlotOccupant::Open,
                                });
                                config.player_teams = *player_teams;
                                config.team_mode = TeamMode::Custom;
                                lobby.status = LobbyStatus::Connected;
                                // Rebuild page to show updated slot cards
                                for e in &roots {
                                    commands.entity(e).try_despawn();
                                }
                            }
                            game_state::message::GameEvent::GameStart { config_json } => {
                                info!("Received GameStart from host");
                                if let Ok(net_config) =
                                    serde_json::from_str::<SerializableGameConfig>(config_json)
                                {
                                    net_config.apply_to_config(&mut config);
                                    net_config.apply_to_lobby(&mut lobby);
                                    info!(
                                        "Applied host config: seed={}, map_size={}, {} seats",
                                        config.map_seed,
                                        net_config.map_size,
                                        net_config.seat_assignments.len()
                                    );
                                }
                                next_state.set(AppState::InGame);
                                return;
                            }
                            _ => {}
                        }
                    }
                }
                game_state::message::ServerMessage::RelayedInput { .. } => {}
                game_state::message::ServerMessage::StateSync { .. } => {}
                game_state::message::ServerMessage::EntitySpawn { .. } => {}
                game_state::message::ServerMessage::EntityDespawn { .. } => {}
                game_state::message::ServerMessage::BuildingSync { .. } => {}
                game_state::message::ServerMessage::ResourceSync { .. } => {}
                game_state::message::ServerMessage::DayCycleSync { .. } => {}
                game_state::message::ServerMessage::WorldBaseline { .. } => {}
                game_state::message::ServerMessage::NeutralWorldDelta { .. } => {}
                game_state::message::ServerMessage::NeutralWorldDespawn { .. } => {}
                game_state::message::ServerMessage::Pong { .. } => {}
            }
        }
    }

}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn broadcast_lobby_update(
    lobby: &LobbyState,
    host: &HostNetState,
    config: &GameSetupConfig,
) {
    use game_state::message::{GameEvent, LobbyPlayerInfo, ServerMessage};

    let players: Vec<LobbyPlayerInfo> = lobby
        .players
        .iter()
        .map(|p| LobbyPlayerInfo {
            player_id: p.player_id,
            name: p.name.clone(),
            seat_index: p.seat_index,
            faction_index: Faction::PLAYERS
                .iter()
                .position(|f| *f == p.faction)
                .unwrap_or(0) as u8,
            color_index: p.color_index,
            is_host: p.is_host,
            connected: p.connected,
        })
        .collect();

    let msg = ServerMessage::Event {
        seq: 0,
        timestamp: 0.0,
        events: vec![GameEvent::LobbyUpdate {
            players,
            slots: config.slots.map(|s| match s {
                SlotOccupant::Human => 0,
                SlotOccupant::Ai(AiDifficulty::Easy) => 1,
                SlotOccupant::Ai(AiDifficulty::Medium) => 2,
                SlotOccupant::Ai(AiDifficulty::Hard) => 3,
                SlotOccupant::Closed => 4,
                SlotOccupant::Open => 5,
            }),
            player_teams: config.player_teams,
        }],
    };

    if let Ok(json) = codec::encode(&msg) {
        let senders = host.client_senders.lock().unwrap();
        for (_id, sender) in senders.iter() {
            let _ = sender.send(json.clone());
        }
    }
}

pub(crate) fn update_web_client_url(
    lobby: Res<LobbyState>,
    mut texts: Query<&mut Text, With<WebClientUrlText>>,
) {
    // Build web URL from the first non-VPN IP (or VPN IP as fallback)
    let dist_exists = std::path::Path::new(
        &std::env::var("DIST_DIR").unwrap_or_else(|_| "dist".to_string()),
    )
    .is_dir();

    let display = if dist_exists && !lobby.all_ips.is_empty() {
        let ip = lobby
            .all_ips
            .iter()
            .find(|(_, _, vpn)| !vpn)
            .or_else(|| lobby.all_ips.first())
            .map(|(ip, _, _)| ip.as_str())
            .unwrap_or("127.0.0.1");
        let http_port = DEFAULT_PORT + crate::multiplayer::transport::HTTP_PORT_OFFSET;
        format!("Web clients: http://{}:{}", ip, http_port)
    } else {
        String::new()
    };

    for mut text in &mut texts {
        if **text != display {
            **text = display.clone();
        }
    }
}

// ── Serializable Config ──

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(crate) struct SeatAssignment {
    pub player_id: u8,
    pub seat_index: u8,
    pub faction_index: u8,
    pub color_index: u8,
    pub is_human: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct SerializableGameConfig {
    pub map_seed: u64,
    /// Slot occupants encoded: 0=Human, 1=AiEasy, 2=AiMedium, 3=AiHard, 4=Closed, 5=Open
    pub slots: [u8; 4],
    pub local_player_slot: usize,
    pub team_mode: u8,
    pub player_teams: [u8; 4],
    pub map_size: u8,
    pub resource_density: u8,
    pub day_cycle_secs: f32,
    pub starting_resources_mult: f32,
    pub seat_assignments: Vec<SeatAssignment>,
}

impl SerializableGameConfig {
    pub(crate) fn from_config(config: &GameSetupConfig, lobby: &LobbyState) -> Self {
        let seat_assignments: Vec<SeatAssignment> = lobby
            .players
            .iter()
            .map(|p| {
                let faction_index = Faction::PLAYERS
                    .iter()
                    .position(|f| *f == p.faction)
                    .unwrap_or(0) as u8;
                SeatAssignment {
                    player_id: p.player_id,
                    seat_index: p.seat_index,
                    faction_index,
                    color_index: p.color_index,
                    is_human: p.connected,
                }
            })
            .collect();

        let slots = config.slots.map(|s| match s {
            SlotOccupant::Human => 0,
            SlotOccupant::Ai(AiDifficulty::Easy) => 1,
            SlotOccupant::Ai(AiDifficulty::Medium) => 2,
            SlotOccupant::Ai(AiDifficulty::Hard) => 3,
            SlotOccupant::Closed => 4,
            SlotOccupant::Open => 5,
        });

        Self {
            map_seed: config.map_seed,
            slots,
            local_player_slot: config.local_player_slot,
            team_mode: match config.team_mode {
                TeamMode::FFA => 0,
                TeamMode::Teams => 1,
                TeamMode::Custom => 2,
            },
            player_teams: config.player_teams,
            map_size: match config.map_size {
                MapSize::Small => 0,
                MapSize::Medium => 1,
                MapSize::Large => 2,
            },
            resource_density: match config.resource_density {
                ResourceDensity::Sparse => 0,
                ResourceDensity::Normal => 1,
                ResourceDensity::Dense => 2,
            },
            day_cycle_secs: config.day_cycle_secs,
            starting_resources_mult: config.starting_resources_mult,
            seat_assignments,
        }
    }

    pub(crate) fn apply_to_config(&self, config: &mut GameSetupConfig) {
        config.map_seed = self.map_seed;
        config.slots = self.slots.map(|s| match s {
            0 => SlotOccupant::Human,
            1 => SlotOccupant::Ai(AiDifficulty::Easy),
            2 => SlotOccupant::Ai(AiDifficulty::Medium),
            3 => SlotOccupant::Ai(AiDifficulty::Hard),
            4 => SlotOccupant::Closed,
            _ => SlotOccupant::Open,
        });
        config.local_player_slot = self.local_player_slot;
        config.team_mode = match self.team_mode {
            0 => TeamMode::FFA,
            1 => TeamMode::Teams,
            _ => TeamMode::Custom,
        };
        config.player_teams = self.player_teams;
        config.map_size = match self.map_size {
            0 => MapSize::Small,
            1 => MapSize::Medium,
            _ => MapSize::Large,
        };
        config.resource_density = match self.resource_density {
            0 => ResourceDensity::Sparse,
            1 => ResourceDensity::Normal,
            _ => ResourceDensity::Dense,
        };
        config.day_cycle_secs = self.day_cycle_secs;
        config.starting_resources_mult = self.starting_resources_mult;
    }

    pub(crate) fn apply_to_lobby(&self, lobby: &mut LobbyState) {
        lobby.players.clear();
        for sa in &self.seat_assignments {
            lobby.players.push(LobbyPlayer {
                player_id: sa.player_id,
                name: format!("Player {}", sa.player_id),
                seat_index: sa.seat_index,
                faction: Faction::PLAYERS
                    .get(sa.faction_index as usize)
                    .copied()
                    .unwrap_or(Faction::Neutral),
                color_index: sa.color_index,
                is_host: sa.seat_index == 0,
                connected: sa.is_human,
            });
        }
    }
}
