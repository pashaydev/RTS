use bevy::prelude::*;
use bevy_matchbox::prelude::*;
use game_state::codec;
use game_state::message::ClientMessage;
use std::sync::atomic::Ordering;

use crate::components::*;
use crate::multiplayer::{
    self, ClientNetState, HostNetState, LobbyPlayer, LobbyState, LobbyStatus, NetRole,
    matchbox_transport::{self, MatchboxInbox, PeerMap, SIGNALING_PORT},
};
use crate::theme;
use crate::ui::fonts::{self, UiFonts};
use crate::ui::menu_helpers::*;

use super::*;
use super::pages;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource)]
pub(crate) struct JoinDiscoveryScan {
    rx: std::sync::Mutex<std::sync::mpsc::Receiver<Vec<multiplayer::DiscoveredHost>>>,
}

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

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
enum JoinTarget {
    Tcp { host: String, port: u16 },
    WebSocket { url: String },
}

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
        let leave_msg = ClientMessage::LeaveNotice {
            seq,
            timestamp: 0.0,
        };
        matchbox_transport::send_to_host(socket, &leave_msg);
    }

    commands.close_socket();
    #[cfg(not(target_arch = "wasm32"))]
    commands.stop_server();
    commands.remove_resource::<HostNetState>();
    commands.remove_resource::<ClientNetState>();
    commands.remove_resource::<PendingGameStart>();
    #[cfg(not(target_arch = "wasm32"))]
    commands.remove_resource::<JoinDiscoveryScan>();
    commands.insert_resource(NetRole::Offline);
    commands.insert_resource(LobbyState::default());
    commands.insert_resource(PeerMap::default());
    commands.insert_resource(MatchboxInbox::default());
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
    lobby: &LobbyState,
    role: NetRole,
    my_faction: Option<Faction>,
) {
    let is_connected = matches!(lobby.status, LobbyStatus::Connected) || role == NetRole::Client;
    let is_connecting = matches!(lobby.status, LobbyStatus::Connecting);
    let is_failed = matches!(lobby.status, LobbyStatus::Failed(_));

    spawn_page_header(
        commands,
        container,
        "JOIN GAME",
        MenuButton(MenuAction::BackToMultiplayer),
        fonts,
    );

    // ── Connection state banner ──
    let (banner_dot_color, banner_text, banner_text_color, banner_bg) = if is_connected {
        (
            theme::SUCCESS,
            "CONNECTED".to_string(),
            theme::SUCCESS,
            Color::srgba(0.15, 0.35, 0.15, 0.4),
        )
    } else if is_connecting {
        (
            theme::WARNING,
            "CONNECTING...".to_string(),
            theme::WARNING,
            Color::srgba(0.35, 0.25, 0.1, 0.4),
        )
    } else if is_failed {
        (
            theme::DESTRUCTIVE,
            "DISCONNECTED".to_string(),
            theme::DESTRUCTIVE,
            Color::srgba(0.35, 0.15, 0.15, 0.4),
        )
    } else {
        (
            theme::TEXT_SECONDARY,
            "NOT CONNECTED".to_string(),
            theme::TEXT_SECONDARY,
            Color::srgba(0.2, 0.2, 0.2, 0.4),
        )
    };

    let banner = commands
        .spawn((
            ConnectionStateBanner,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(8.0),
                padding: UiRect::axes(Val::Px(16.0), Val::Px(10.0)),
                margin: UiRect::vertical(Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(banner_bg),
            BorderColor::all(banner_dot_color.with_alpha(0.3)),
        ))
        .with_children(|parent| {
            // Colored dot
            parent.spawn((
                Node {
                    width: Val::Px(10.0),
                    height: Val::Px(10.0),
                    border_radius: BorderRadius::all(Val::Px(5.0)),
                    ..default()
                },
                BackgroundColor(banner_dot_color),
            ));
            parent.spawn((
                Text::new(banner_text),
                TextFont {
                    font_size: theme::FONT_MEDIUM,
                    ..default()
                },
                TextColor(banner_text_color),
                Pickable::IGNORE,
            ));
        })
        .id();
    commands.entity(container).add_child(banner);

    spawn_animated_section_divider(commands, container, "SESSION CODE", fonts);

    // ── Conditional input vs read-only display ──
    if is_connected || is_connecting {
        // Read-only display of session code
        let code_display = if !lobby.client_session_code.is_empty() {
            lobby.client_session_code.clone()
        } else {
            lobby.session_code.clone()
        };
        let display_row = commands
            .spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                margin: UiRect::vertical(Val::Px(6.0)),
                ..default()
            })
            .with_children(|parent| {
                parent.spawn((
                    Text::new("Session:"),
                    TextFont {
                        font_size: theme::FONT_MEDIUM,
                        ..default()
                    },
                    TextColor(theme::TEXT_SECONDARY),
                ));
                parent.spawn((
                    Text::new(code_display),
                    TextFont {
                        font_size: theme::FONT_MEDIUM,
                        ..default()
                    },
                    TextColor(theme::ACCENT),
                ));
            })
            .id();
        commands.entity(container).add_child(display_row);
    } else {
        // Full editable input row
        let input_row = commands
            .spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
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
                            width: Val::Px(240.0),
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

                // Paste button
                parent
                    .spawn((
                        PasteCodeButton,
                        Button,
                        ButtonAnimState::new(theme::BTN_PRIMARY.to_srgba().to_f32_array()),
                        ButtonStyle::Filled,
                        Node {
                            padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                            ..default()
                        },
                        BackgroundColor(theme::BTN_PRIMARY),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("PASTE"),
                            TextFont {
                                font_size: theme::FONT_SMALL,
                                ..default()
                            },
                            TextColor(theme::TEXT_PRIMARY),
                            Pickable::IGNORE,
                        ));
                    });

                // Clear button
                parent
                    .spawn((
                        ClearCodeButton,
                        Button,
                        ButtonAnimState::new(theme::BTN_PRIMARY.to_srgba().to_f32_array()),
                        ButtonStyle::Ghost,
                        Node {
                            padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("CLEAR"),
                            TextFont {
                                font_size: theme::FONT_SMALL,
                                ..default()
                            },
                            TextColor(theme::TEXT_SECONDARY),
                            Pickable::IGNORE,
                        ));
                    });
            })
            .id();
        commands.entity(container).add_child(input_row);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let discover_btn = commands
                .spawn((
                    DiscoverLanHostsButton,
                    MenuButton(MenuAction::RefreshLanHosts),
                    Button,
                    ButtonAnimState::new(theme::BTN_PRIMARY.to_srgba().to_f32_array()),
                    ButtonStyle::Ghost,
                    Node {
                        width: Val::Px(120.0),
                        align_content: AlignContent::Center,
                        align_items: AlignItems::Center,
                        padding: UiRect::all(Val::Px(8.0)),
                        margin: UiRect::bottom(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(theme::BTN_PRIMARY),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("FIND LAN HOSTS"),
                        fonts::heading(fonts, theme::FONT_MEDIUM),
                        TextColor(theme::TEXT_PRIMARY),
                        Pickable::IGNORE,
                    ));
                })
                .id();
            commands.entity(container).add_child(discover_btn);

            let discovered_list = commands
                .spawn((
                    DiscoveredHostsList,
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Stretch,
                        row_gap: Val::Px(6.0),
                        margin: UiRect::bottom(Val::Px(8.0)),
                        ..default()
                    },
                ))
                .id();
            commands.entity(container).add_child(discovered_list);
        }
    }

    // ── Conditional CONNECT vs DISCONNECT ──
    if is_connected {
        // Show DISCONNECT button with destructive styling
        let dc_btn = commands
            .spawn((
                MenuButton(MenuAction::Disconnect),
                Button,
                ButtonAnimState::new(theme::DESTRUCTIVE.to_srgba().to_f32_array()),
                ButtonStyle::Filled,
                Node {
                    width: Val::Px(220.0),
                    height: Val::Px(44.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    margin: UiRect::vertical(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(theme::DESTRUCTIVE),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Text::new("DISCONNECT"),
                    fonts::heading(fonts, theme::FONT_BUTTON),
                    TextColor(Color::WHITE),
                    Pickable::IGNORE,
                ));
            })
            .id();
        commands.entity(container).add_child(dc_btn);
    } else if !is_connecting {
        // Show CONNECT button
        let connect_btn = spawn_styled_button(
            commands,
            "CONNECT",
            MenuButton(MenuAction::ConnectToHost),
            true,
            fonts,
        );
        commands.entity(container).add_child(connect_btn);
    }
    // When connecting: show neither button

    spawn_animated_section_divider(commands, container, "STATUS", fonts);

    // ── Color-coded status text ──
    let (status_text, status_color) = match &lobby.status {
        LobbyStatus::Connected => (
            "Connected! Waiting for host to start...".to_string(),
            theme::SUCCESS,
        ),
        LobbyStatus::Connecting => (
            "Connecting...".to_string(),
            theme::WARNING,
        ),
        LobbyStatus::Failed(e) => (
            format!("Failed: {}", e),
            theme::DESTRUCTIVE,
        ),
        LobbyStatus::Waiting => (
            if cfg!(target_arch = "wasm32") {
                "Enter a hosted session code and press CONNECT".to_string()
            } else {
                "Enter the host's session code or scan your LAN and press CONNECT".to_string()
            },
            theme::TEXT_SECONDARY,
        ),
    };

    let status = commands
        .spawn((
            LobbyStatusText,
            Text::new(status_text),
            TextFont {
                font_size: theme::FONT_MEDIUM,
                ..default()
            },
            TextColor(status_color),
            Node {
                margin: UiRect::vertical(Val::Px(8.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(status);

    spawn_animated_section_divider(commands, container, "FACTIONS", fonts);

    for i in 0..4 {
        spawn_client_slot_card(commands, container, i, config, lobby, my_faction);
    }
}

/// Read-only slot card for the client join lobby — shows faction config from host.
fn spawn_client_slot_card(
    commands: &mut Commands,
    container: Entity,
    slot_index: usize,
    config: &GameSetupConfig,
    lobby: &LobbyState,
    my_faction: Option<Faction>,
) {
    let slot = config.slots[slot_index];
    let faction = Faction::PLAYERS[slot_index];
    let faction_color = faction.color();
    let team = config.player_teams[slot_index];

    // Find matching lobby player for this slot
    let lobby_player = lobby.players.iter().find(|p| p.faction == faction);
    let is_me = my_faction.map_or(false, |f| f == faction);

    let type_label = match slot {
        SlotOccupant::Human => "Human",
        SlotOccupant::Ai(AiDifficulty::Easy) => "AI Easy",
        SlotOccupant::Ai(AiDifficulty::Medium) => "AI Medium",
        SlotOccupant::Ai(AiDifficulty::Hard) => "AI Hard",
        SlotOccupant::Closed => "None",
        SlotOccupant::Open => "Open",
    };

    // Use player name from lobby data if available
    let display_name = if let Some(player) = lobby_player {
        if is_me {
            format!("{} (YOU)", player.name)
        } else {
            player.name.clone()
        }
    } else if is_me {
        format!("Player {} (YOU)", slot_index + 1)
    } else {
        format!("Player {}", slot_index + 1)
    };

    let team_colors = [
        Color::srgb(0.9, 0.75, 0.2),
        Color::srgb(0.2, 0.75, 0.85),
        Color::srgb(0.85, 0.3, 0.65),
        Color::srgb(0.95, 0.5, 0.15),
    ];
    let team_color = team_colors.get(team as usize).copied().unwrap_or(team_colors[0]);

    // Highlight border for "your" slot
    let border_color = if is_me {
        theme::ACCENT
    } else {
        theme::SEPARATOR
    };

    let card = commands
        .spawn((
            SlotCardContainer(slot_index),
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(10.0)),
                margin: UiRect::vertical(Val::Px(3.0)),
                border: UiRect::all(Val::Px(if is_me { 2.0 } else { 1.0 })),
                column_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(theme::BG_SURFACE),
            BorderColor::all(border_color),
        ))
        .with_children(|card| {
            // Connection indicator dot (for human players with lobby data)
            if let Some(player) = lobby_player {
                let dot_color = if player.connected {
                    theme::SUCCESS
                } else {
                    theme::DESTRUCTIVE
                };
                card.spawn((
                    Node {
                        width: Val::Px(8.0),
                        height: Val::Px(8.0),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(dot_color),
                ));
            }
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
            // Player name / faction label
            card.spawn((
                Text::new(display_name),
                TextFont { font_size: theme::FONT_MEDIUM, ..default() },
                TextColor(if is_me { theme::ACCENT } else { faction_color }),
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

    // If the host is numeric-dot notation, treat it as IPv4 and validate it.
    // Otherwise allow standard hostnames such as ngrok TCP endpoints.
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
    let signaling_builder = MatchboxServer::client_server_builder(
        (Ipv4Addr::UNSPECIFIED, SIGNALING_PORT),
    );
    commands.start_server(signaling_builder);

    // Open the host's own socket connecting to the local signaling server
    let room_url = format!("ws://127.0.0.1:{}/rts_room", SIGNALING_PORT);
    commands.open_socket(matchbox_transport::build_socket(&room_url));

    let session_code = format!("ws://{}:{}/rts_room", primary_ip, SIGNALING_PORT);
    info!("Hosting on {} (signaling port {})", session_code, SIGNALING_PORT);
    for detected in &all_ips {
        let vpn_tag = if detected.is_likely_vpn { " [VPN]" } else { "" };
        info!(
            "  Available IP: {} ({}{})",
            detected.ip, detected.name, vpn_tag
        );
    }

    // ── LAN discovery (UDP broadcast) ──
    #[cfg(not(target_arch = "wasm32"))]
    {
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
                warn!("Failed to bind LAN discovery socket on {}: {}", discovery_addr, e);
            }
        }

        // ── HTTP file server for direct LAN web clients ──
        let http_port = DEFAULT_PORT + transport::HTTP_PORT_OFFSET;
        let http_addr = format!("0.0.0.0:{}", http_port);
        let dist_dir = std::env::var("DIST_DIR").unwrap_or_else(|_| "dist".to_string());
        if std::path::Path::new(&dist_dir).is_dir() {
            match std::net::TcpListener::bind(&http_addr) {
                Ok(http_listener) => {
                    info!("Serving WASM client at http://{}:{}/", primary_ip, http_port);
                    let http_shutdown = shutdown.clone();
                    std::thread::spawn(move || {
                        transport::host_file_server_thread(http_listener, dist_dir, http_shutdown);
                    });
                }
                Err(e) => {
                    warn!("Failed to bind HTTP file server on {}: {}", http_addr, e);
                }
            }
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

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn stop_hosting(
    commands: &mut Commands,
    _host_state: &Option<Res<HostNetState>>,
) {
    commands.close_socket();
    commands.stop_server();
    commands.remove_resource::<HostNetState>();
    commands.insert_resource(NetRole::Offline);
    commands.insert_resource(LobbyState::default());
}

pub(crate) fn stop_client(
    commands: &mut Commands,
    _client_state: &Option<Res<ClientNetState>>,
) {
    commands.close_socket();
    commands.remove_resource::<ClientNetState>();
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

        // Store session code for display after page rebuilds
        lobby.client_session_code = code.clone();

        // Resolve session code to a signaling URL
        let signaling_url = resolve_signaling_url(&code);

        for mut text in &mut status_texts {
            **text = format!("Connecting to {}...", signaling_url);
        }

        // Unified: works for both native and WASM via Matchbox WebRTC
        commands.open_socket(matchbox_transport::build_socket(&signaling_url));
        commands.insert_resource(ClientNetState::default());
        commands.insert_resource(PeerMap::default());
        commands.insert_resource(MatchboxInbox::default());
        commands.insert_resource(NetRole::Client);

        lobby.status = LobbyStatus::Connecting;
        for mut text in &mut status_texts {
            **text = "Connecting via WebRTC...".to_string();
        }
    }
}

/// Resolve a session code to a Matchbox signaling URL.
/// - If it starts with `ws://` or `wss://`, use as-is.
/// - If it's an IP:PORT or IP, build `ws://IP:3536/rts_room`.
fn resolve_signaling_url(code: &str) -> String {
    let trimmed = code.trim();
    if trimmed.starts_with("ws://") || trimmed.starts_with("wss://") {
        return trimmed.to_string();
    }
    // Parse as host:port or just host
    if let Some((host, _port_str)) = trimmed.split_once(':') {
        format!("ws://{}:{}/rts_room", host, SIGNALING_PORT)
    } else {
        format!("ws://{}:{}/rts_room", trimmed, SIGNALING_PORT)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn refresh_lan_hosts_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<DiscoverLanHostsButton>)>,
    mut commands: Commands,
    mut lobby: ResMut<LobbyState>,
    roots: Query<Entity, With<MenuRoot>>,
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
        let hosts = multiplayer::transport::discover_lan_hosts(std::time::Duration::from_millis(900))
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
    for e in &roots {
        commands.entity(e).try_despawn();
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn refresh_lan_hosts_system() {}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn poll_lan_discovery_results_system(
    scan: Option<Res<JoinDiscoveryScan>>,
    mut commands: Commands,
    mut lobby: ResMut<LobbyState>,
    roots: Query<Entity, With<MenuRoot>>,
) {
    let Some(scan) = scan else { return };
    let rx = scan.rx.lock().unwrap();
    match rx.try_recv() {
        Ok(hosts) => {
            lobby.discovered_hosts = hosts;
            lobby.discovery_status = if lobby.discovered_hosts.is_empty() {
                "No LAN hosts found. Use direct IP:port for VPN or manual join.".to_string()
            } else {
                format!("Found {} LAN host(s). Select one to autofill the code.", lobby.discovered_hosts.len())
            };
            commands.remove_resource::<JoinDiscoveryScan>();
            for e in &roots {
                commands.entity(e).try_despawn();
            }
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            lobby.discovery_status = "LAN scan failed.".to_string();
            commands.remove_resource::<JoinDiscoveryScan>();
            for e in &roots {
                commands.entity(e).try_despawn();
            }
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {}
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn poll_lan_discovery_results_system() {}

pub(crate) fn select_discovered_host_system(
    interactions: Query<(&Interaction, &DiscoveredHostButton), Changed<Interaction>>,
    lobby: Res<LobbyState>,
    mut inputs: Query<(&mut TextInputField, &Children), With<SessionCodeInput>>,
    mut text_query: Query<&mut Text, Without<TextInputCursor>>,
) {
    for (interaction, button) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(host) = lobby.discovered_hosts.get(button.0) else {
            continue;
        };
        let Ok((mut field, children)) = inputs.single_mut() else {
            continue;
        };
        field.value = host.session_code.clone();
        field.cursor_pos = field.value.len();
        for child in children.iter() {
            if let Ok(mut text) = text_query.get_mut(child) {
                **text = field.value.clone();
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
    fn parse_direct_host_port_accepts_dotted_hostname() {
        assert_eq!(
            parse_direct_host_port("0.tcp.eu.ngrok.io:17167", DEFAULT_PORT),
            Ok(("0.tcp.eu.ngrok.io".to_string(), 17167))
        );
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
    matchbox: (
        Option<ResMut<MatchboxSocket>>,
        Option<ResMut<PeerMap>>,
        Option<ResMut<MatchboxInbox>>,
    ),
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
    discovered_list_q: Query<Entity, (With<DiscoveredHostsList>, Without<DiscoveredHostsListPopulated>)>,
    mut session_tokens: ResMut<multiplayer::SessionTokens>,
    roots: Query<Entity, With<MenuRoot>>,
) {
    let (mut socket, mut peer_map, mut inbox) = matchbox;

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

    if *page == MenuPage::JoinLobby {
        for list_entity in &discovered_list_q {
            commands
                .entity(list_entity)
                .insert(DiscoveredHostsListPopulated);

            if !lobby.discovery_status.is_empty() {
                let status = commands
                    .spawn((
                        Text::new(lobby.discovery_status.clone()),
                        TextFont {
                            font_size: theme::FONT_SMALL,
                            ..default()
                        },
                        TextColor(theme::TEXT_SECONDARY),
                    ))
                    .id();
                commands.entity(list_entity).add_child(status);
            }

            for (index, host) in lobby.discovered_hosts.iter().enumerate() {
                let button = commands
                    .spawn((
                        DiscoveredHostButton(index),
                        Button,
                        Node {
                            width: Val::Percent(100.0),
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Center,
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(theme::BG_SURFACE),
                        BorderColor::all(theme::SEPARATOR),
                    ))
                    .with_children(|parent| {
                        parent.spawn((
                            Text::new(host.name.clone()),
                            TextFont {
                                font_size: theme::FONT_MEDIUM,
                                ..default()
                            },
                            TextColor(theme::TEXT_PRIMARY),
                            Pickable::IGNORE,
                        ));
                        parent.spawn((
                            Text::new(host.session_code.clone()),
                            TextFont {
                                font_size: theme::FONT_SMALL,
                                ..default()
                            },
                            TextColor(theme::ACCENT),
                            Pickable::IGNORE,
                        ));
                    })
                    .id();
                commands.entity(list_entity).add_child(button);
            }
        }
    }

    // ── Host: poll matchbox for new peer connections and lobby messages ──
    if let (Some(host), Some(ref mut socket), Some(ref mut peer_map), Some(ref mut inbox)) =
        (host_state.as_ref(), socket.as_mut(), peer_map.as_mut(), inbox.as_mut())
    {
        // Update peers
        if let Ok(changes) = socket.try_update_peers() {
            for (peer, state) in &changes {
                match state {
                    PeerState::Connected => inbox.connected.push(*peer),
                    PeerState::Disconnected => inbox.disconnected.push(*peer),
                }
            }
        }

        // Drain reliable channel for lobby messages
        if let Ok(channel) = socket.get_channel_mut(matchbox_transport::RELIABLE_CH) {
            for (peer, packet) in channel.receive() {
                if let Ok(msg) = codec::decode::<game_state::message::ClientMessage>(&packet) {
                    let player_id = peer_map.player_id(&peer).unwrap_or(0);
                    inbox.client_commands.push((player_id, msg));
                }
            }
        }

        let mut lobby_changed = false;

        // Process new peer connections
        let connected = std::mem::take(&mut inbox.connected);
        for peer in connected {
            let player_id = peer_map.assign(peer);
            info!("New peer {:?} assigned player_id {} in lobby", peer, player_id);

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
            lobby_changed = true;
        }

        // Process peer disconnections
        let disconnected = std::mem::take(&mut inbox.disconnected);
        for peer in disconnected {
            if let Some(player_id) = peer_map.remove_peer(&peer) {
                info!("Player {} disconnected from lobby", player_id);
                if let Some(player) =
                    lobby.players.iter_mut().find(|p| p.player_id == player_id)
                {
                    player.connected = false;
                    lobby_changed = true;
                }
            }
        }

        // Process lobby messages
        let client_commands = std::mem::take(&mut inbox.client_commands);
        for (player_id, msg) in client_commands {
            match msg {
                game_state::message::ClientMessage::JoinRequest {
                    player_name, ..
                } => {
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
                        matchbox_transport::send_to_player(socket, peer_map, player_id, &msg);
                        lobby_changed = true;
                    }
                }
                game_state::message::ClientMessage::LeaveNotice { .. } => {
                    if let Some(player) =
                        lobby.players.iter_mut().find(|p| p.player_id == player_id)
                    {
                        player.connected = false;
                        lobby_changed = true;
                    }
                }
                game_state::message::ClientMessage::Input { .. } => {}
                game_state::message::ClientMessage::Ping { .. } => {}
                game_state::message::ClientMessage::Reconnect { .. } => {
                    // Reconnection during lobby phase — not yet supported
                }
            }
        }

        if lobby_changed {
            // Revert slots for disconnected players back to Open
            for player in &lobby.players {
                let idx = Faction::PLAYERS
                    .iter()
                    .position(|f| *f == player.faction)
                    .unwrap_or(0);
                if player.connected {
                    if !matches!(config.slots[idx], SlotOccupant::Human) {
                        config.slots[idx] = SlotOccupant::Human;
                    }
                } else if matches!(config.slots[idx], SlotOccupant::Human) {
                    config.slots[idx] = SlotOccupant::Open;
                }
            }
            // Remove fully disconnected players from lobby
            lobby.players.retain(|p| p.connected);
            broadcast_lobby_update_matchbox(&lobby, socket, &config);
            // Rebuild the page to reflect updated slot cards
            for e in &roots {
                commands.entity(e).try_despawn();
            }
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
            matchbox_transport::broadcast_reliable(socket, &start_event);

            commands.remove_resource::<PendingGameStart>();
            next_state.set(AppState::InGame);
        }
    }

    // ── Client: detect dead connection ──
    if let Some(ref client) = client_state {
        if client
            .disconnected
            .load(std::sync::atomic::Ordering::Relaxed)
            && !matches!(lobby.status, LobbyStatus::Connected)
        {
            lobby.status =
                LobbyStatus::Failed("Connection lost".to_string());
            for mut text in &mut status_texts {
                **text = "Connection failed — host unreachable".to_string();
            }
            commands.close_socket();
            commands.remove_resource::<ClientNetState>();
            commands.insert_resource(NetRole::Offline);
            return;
        }
    }

    // ── Client: poll matchbox for lobby updates and game start ──
    if let (Some(mut client), Some(ref mut socket)) = (client_state, socket.as_mut()) {
        // Update peers to detect connection/disconnection
        if let Ok(changes) = socket.try_update_peers() {
            for (peer, state) in &changes {
                match state {
                    PeerState::Connected => {
                        info!("Client connected to host peer {:?}", peer);
                        lobby.status = LobbyStatus::Connected;
                        // Send JoinRequest
                        let join_msg = game_state::message::ClientMessage::JoinRequest {
                            seq: 0,
                            timestamp: 0.0,
                            player_name: "Client".to_string(),
                            preferred_faction_index: None,
                        };
                        matchbox_transport::send_to_host(socket, &join_msg);
                    }
                    PeerState::Disconnected => {
                        client.disconnected.store(true, Ordering::Relaxed);
                    }
                }
            }
        }

        // Drain reliable channel for lobby messages
        let mut incoming = Vec::new();
        if let Ok(channel) = socket.get_channel_mut(matchbox_transport::RELIABLE_CH) {
            for (_peer, packet) in channel.receive() {
                if let Ok(msg) = codec::decode::<game_state::message::ServerMessage>(&packet) {
                    incoming.push(msg);
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
                                lobby.discovery_status.clear();
                                lobby.discovered_hosts.clear();
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

    // Legacy: kept for reference but no longer called
    let _ = (host, &msg);
}

/// Broadcast lobby update to all connected peers via Matchbox.
fn broadcast_lobby_update_matchbox(
    lobby: &LobbyState,
    socket: &mut MatchboxSocket,
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
    matchbox_transport::broadcast_reliable(socket, &msg);
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

pub(crate) fn paste_code_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<PasteCodeButton>)>,
    mut inputs: Query<(&mut TextInputField, &Children), With<SessionCodeInput>>,
    mut text_query: Query<&mut Text, Without<TextInputCursor>>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(clip) = clipboard_read() else {
            continue;
        };
        let clip = clip.trim().to_string();
        if clip.is_empty() {
            continue;
        }
        let Ok((mut field, children)) = inputs.single_mut() else {
            continue;
        };
        field.value = clip[..clip.len().min(field.max_len)].to_string();
        field.cursor_pos = field.value.len();
        for child in children.iter() {
            if let Ok(mut text) = text_query.get_mut(child) {
                **text = field.value.clone();
            }
        }
    }
}

pub(crate) fn clear_code_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<ClearCodeButton>)>,
    mut inputs: Query<(&mut TextInputField, &Children), With<SessionCodeInput>>,
    mut text_query: Query<&mut Text, Without<TextInputCursor>>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Ok((mut field, children)) = inputs.single_mut() else {
            continue;
        };
        field.value.clear();
        field.cursor_pos = 0;
        for child in children.iter() {
            if let Ok(mut text) = text_query.get_mut(child) {
                **text = String::new();
            }
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
