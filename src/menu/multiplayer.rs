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
        "Join a game hosted on a native client"
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
            Text::new("Share this code with players on your network\nFor VPN/Hamachi: use the VPN IP shown below"),
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

    spawn_animated_section_divider(commands, container, "PLAYERS", fonts);

    // Host color picker
    let host_color_row = spawn_color_picker(commands, config.player_color_index, SelectorField::HostPlayerColor);
    commands.entity(container).add_child(host_color_row);

    for i in 0..4 {
        let label = if i == 0 {
            "Host (You)"
        } else {
            "Waiting..."
        };
        let color = if i == 0 {
            Faction::PLAYERS[config.player_color_index].color()
        } else {
            theme::TEXT_SECONDARY
        };
        let slot = commands
            .spawn((
                LobbyPlayerSlot(i),
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                    margin: UiRect::vertical(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.5)),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Node {
                        width: Val::Px(8.0),
                        height: Val::Px(8.0),
                        margin: UiRect::right(Val::Px(10.0)),
                        ..default()
                    },
                    BackgroundColor(color),
                ));
                parent.spawn((
                    Text::new(label),
                    TextFont {
                        font_size: theme::FONT_MEDIUM,
                        ..default()
                    },
                    TextColor(color),
                ));
            })
            .id();
        commands.entity(container).add_child(slot);
    }

    // ── AI Opponents ──

    spawn_animated_section_divider(commands, container, "AI OPPONENTS", fonts);

    spawn_selector_row(
        commands,
        container,
        "Count:",
        &["0", "1", "2", "3"],
        config.num_ai_opponents as usize,
        SelectorField::HostAiCount,
    );

    for i in 0..3 {
        let visible = i < config.num_ai_opponents as usize;
        pages::spawn_ai_card(commands, container, i, config, visible);
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
                        max_len: 21,
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
            Text::new("Enter the host's session code and press CONNECT"),
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

    spawn_animated_section_divider(commands, container, "PLAYERS", fonts);

    for i in 0..4 {
        let slot = commands
            .spawn((
                LobbyPlayerSlot(i),
                Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                    margin: UiRect::vertical(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.5)),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Node {
                        width: Val::Px(8.0),
                        height: Val::Px(8.0),
                        margin: UiRect::right(Val::Px(10.0)),
                        ..default()
                    },
                    BackgroundColor(theme::TEXT_SECONDARY),
                ));
                parent.spawn((
                    Text::new("—"),
                    TextFont {
                        font_size: theme::FONT_MEDIUM,
                        ..default()
                    },
                    TextColor(theme::TEXT_SECONDARY),
                ));
            })
            .id();
        commands.entity(container).add_child(slot);
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

// ── Networking helpers ──

const DEFAULT_PORT: u16 = 7878;

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
            faction: Faction::PLAYERS[config.player_color_index],
            color_index: config.player_color_index as u8,
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
) {
    for (interaction, btn) in &interactions {
        if *interaction != Interaction::Pressed || btn.0 != MenuAction::ConnectToHost {
            continue;
        }

        let code = if let Ok(input) = text_inputs.single() {
            input.value.trim().to_string()
        } else {
            continue;
        };

        if code.is_empty() {
            for mut text in &mut status_texts {
                **text = "Please enter a session code (IP:port)".to_string();
            }
            continue;
        }

        // Parse host and port from the session code
        let (host, port) = if code.contains(':') {
            let parts: Vec<&str> = code.splitn(2, ':').collect();
            (parts[0].to_string(), parts[1].parse::<u16>().unwrap_or(DEFAULT_PORT))
        } else {
            (code.clone(), DEFAULT_PORT)
        };

        // ── Native: connect via TCP ──
        #[cfg(not(target_arch = "wasm32"))]
        {
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
            let ws_port = port + 1; // WS port is TCP port + 1
            let ws_url = format!("ws://{}:{}", host, ws_port);
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
    slot_texts: Query<(&LobbyPlayerSlot, &Children)>,
    mut child_texts: Query<
        &mut Text,
        (
            Without<LobbyStatusText>,
            Without<SessionCodeText>,
            Without<LobbyPlayerSlot>,
        ),
    >,
    mut child_bgs: Query<&mut BackgroundColor, Without<LobbyPlayerSlot>>,
    mut config: ResMut<GameSetupConfig>,
    ip_list_q: Query<Entity, (With<HostIpList>, Without<HostIpListPopulated>)>,
    mut session_tokens: ResMut<multiplayer::SessionTokens>,
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
            // Auto-cap AI count when lobby changes (player join/leave)
            let human_count = lobby.players.iter().filter(|p| p.connected).count() as u8;
            let max_ai = 4u8.saturating_sub(human_count).min(3);
            if config.num_ai_opponents > max_ai {
                config.num_ai_opponents = max_ai;
            }
            broadcast_lobby_update(&lobby, host);
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

            let human_count = lobby.players.iter().filter(|p| p.connected).count() as u8;
            config.human_count = human_count;
            // Build human_faction_indices from lobby players
            config.human_faction_indices = lobby
                .players
                .iter()
                .filter(|p| p.connected)
                .map(|p| {
                    Faction::PLAYERS
                        .iter()
                        .position(|f| *f == p.faction)
                        .unwrap_or(0)
                })
                .collect();
            let max_ai = 4u8.saturating_sub(human_count).min(3);
            if config.num_ai_opponents > max_ai {
                config.num_ai_opponents = max_ai;
            }
            // Recalculate AI factions to avoid collisions with human factions
            config.recalculate_ai_factions();
            info!(
                "Multiplayer: {} humans ({:?}), {} AI opponents ({:?})",
                human_count, config.human_faction_indices,
                config.num_ai_opponents, config.ai_faction_indices
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
                            game_state::message::GameEvent::LobbyUpdate { players } => {
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
                                lobby.status = LobbyStatus::Connected;
                                for mut text in &mut status_texts {
                                    **text = format!(
                                        "Connected — {} player(s) in lobby",
                                        lobby.players.len()
                                    );
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

    // ── Update player slot UI for both host and client ──
    update_player_slot_ui(&lobby, &slot_texts, &mut child_texts, &mut child_bgs);
}

fn update_player_slot_ui(
    lobby: &LobbyState,
    slot_texts: &Query<(&LobbyPlayerSlot, &Children)>,
    child_texts: &mut Query<
        &mut Text,
        (
            Without<LobbyStatusText>,
            Without<SessionCodeText>,
            Without<LobbyPlayerSlot>,
        ),
    >,
    child_bgs: &mut Query<&mut BackgroundColor, Without<LobbyPlayerSlot>>,
) {
    for (slot, children) in slot_texts {
        let idx = slot.0;
        let (label, color) = if let Some(player) = lobby.players.get(idx) {
            let c = player.faction.color();
            let suffix = if player.is_host {
                " (Host)"
            } else if !player.connected {
                " (disconnected)"
            } else {
                ""
            };
            (format!("{}{}", player.name, suffix), c)
        } else {
            ("Waiting...".to_string(), theme::TEXT_SECONDARY)
        };

        let mut child_iter = children.iter();
        if let Some(dot_entity) = child_iter.next() {
            if let Ok(mut bg) = child_bgs.get_mut(dot_entity) {
                *bg = BackgroundColor(color);
            }
        }
        if let Some(text_entity) = child_iter.next() {
            if let Ok(mut text) = child_texts.get_mut(text_entity) {
                if **text != label {
                    **text = label;
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn broadcast_lobby_update(lobby: &LobbyState, host: &HostNetState) {
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
        events: vec![GameEvent::LobbyUpdate { players }],
    };

    if let Ok(json) = codec::encode(&msg) {
        let senders = host.client_senders.lock().unwrap();
        for (_id, sender) in senders.iter() {
            let _ = sender.send(json.clone());
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
    pub num_ai_opponents: u8,
    pub ai_difficulties: [u8; 3],
    pub team_mode: u8,
    pub player_teams: [u8; 4],
    pub map_size: u8,
    pub resource_density: u8,
    pub day_cycle_secs: f32,
    pub starting_resources_mult: f32,
    pub seat_assignments: Vec<SeatAssignment>,
    #[serde(default)]
    pub human_factions: Vec<u8>,
    #[serde(default = "default_ai_faction_indices")]
    pub ai_faction_indices: [usize; 3],
    #[serde(default)]
    pub player_color_index: usize,
    #[serde(default = "default_human_count")]
    pub human_count: u8,
}

fn default_ai_faction_indices() -> [usize; 3] {
    [1, 2, 3]
}

fn default_human_count() -> u8 {
    1
}

impl SerializableGameConfig {
    pub(crate) fn from_config(config: &GameSetupConfig, lobby: &LobbyState) -> Self {
        let mut seat_assignments: Vec<SeatAssignment> = lobby
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

        // Add AI seat assignments using config.ai_faction_indices
        let human_count = seat_assignments.len() as u8;
        for i in 0..config.num_ai_opponents as usize {
            let faction_index = config.ai_faction_indices[i] as u8;
            seat_assignments.push(SeatAssignment {
                player_id: 100 + i as u8,
                seat_index: human_count + i as u8,
                faction_index,
                color_index: faction_index,
                is_human: false,
            });
        }

        let human_factions: Vec<u8> = seat_assignments
            .iter()
            .filter(|s| s.is_human)
            .map(|s| s.faction_index)
            .collect();

        Self {
            map_seed: config.map_seed,
            num_ai_opponents: config.num_ai_opponents,
            ai_difficulties: config.ai_difficulties.map(|d| match d {
                AiDifficulty::Easy => 0,
                AiDifficulty::Medium => 1,
                AiDifficulty::Hard => 2,
            }),
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
            human_factions,
            ai_faction_indices: config.ai_faction_indices,
            player_color_index: config.player_color_index,
            human_count: config.human_count,
        }
    }

    pub(crate) fn apply_to_config(&self, config: &mut GameSetupConfig) {
        config.map_seed = self.map_seed;
        config.num_ai_opponents = self.num_ai_opponents;
        config.ai_difficulties = self.ai_difficulties.map(|d| match d {
            0 => AiDifficulty::Easy,
            1 => AiDifficulty::Medium,
            _ => AiDifficulty::Hard,
        });
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
        config.ai_faction_indices = self.ai_faction_indices;
        config.player_color_index = self.player_color_index;
        config.human_count = self.human_count;
        config.human_faction_indices = self.human_factions.iter().map(|&i| i as usize).collect();
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
