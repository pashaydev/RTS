use bevy::prelude::*;

use crate::components::*;
use crate::multiplayer::{LobbyState, LobbyStatus, NetRole};
use crate::theme;
use crate::ui::fonts::{self, UiFonts};
use crate::ui::menu_helpers::*;

use super::super::*;
use super::super::pages;

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
    lobby: &LobbyState,
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
            let initial_code = if lobby.session_code.is_empty() {
                "Starting...".to_string()
            } else {
                lobby.session_code.clone()
            };
            parent.spawn((
                SessionCodeText,
                Text::new(initial_code),
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
            HostIpListPopulated,
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                margin: UiRect::bottom(Val::Px(12.0)),
                row_gap: Val::Px(2.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            for (ip, name, is_vpn) in &lobby.all_ips {
                let label = if *is_vpn {
                    format!("{} ({}) [VPN]", ip, name)
                } else {
                    format!("{} ({})", ip, name)
                };
                let color = if *is_vpn {
                    Color::srgb(0.4, 0.9, 0.4)
                } else {
                    theme::TEXT_SECONDARY
                };
                parent.spawn((
                    Text::new(label),
                    TextFont { font_size: theme::FONT_SMALL, ..default() },
                    TextColor(color),
                ));
            }
        })
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
            let mut dot = parent.spawn((
                ConnectionDotAnim,
                Node {
                    width: Val::Px(10.0),
                    height: Val::Px(10.0),
                    border_radius: BorderRadius::all(Val::Px(5.0)),
                    ..default()
                },
                BackgroundColor(banner_dot_color),
            ));
            if is_connecting {
                dot.insert(UiGlowPulse {
                    color: theme::WARNING,
                    intensity: 0.8,
                });
            }
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

    // ── Preferred slot selector (only when not connected) ──
    if !is_connected && !is_connecting {
        spawn_selector_row(
            commands,
            container,
            "Preferred Slot:",
            &["Any", "1", "2", "3", "4"],
            0,
            SelectorField::PreferredFaction,
        );
    }

    // ── Conditional CONNECT vs DISCONNECT ──
    if is_connected {
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
        let connect_btn = spawn_styled_button(
            commands,
            "CONNECT",
            MenuButton(MenuAction::ConnectToHost),
            true,
            fonts,
        );
        commands.entity(container).add_child(connect_btn);
    }

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

    // Elapsed time indicator when connecting
    if is_connecting {
        let elapsed = commands
            .spawn((
                ConnectionElapsedText,
                Text::new("Elapsed: 0s"),
                TextFont {
                    font_size: theme::FONT_SMALL,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
                Node {
                    margin: UiRect::bottom(Val::Px(8.0)),
                    ..default()
                },
            ))
            .id();
        commands.entity(container).add_child(elapsed);
    }

    // Ping display (client only, when connected)
    if is_connected {
        let ping = commands
            .spawn((
                LobbyPingText,
                Text::new("Ping: --ms"),
                TextFont {
                    font_size: theme::FONT_SMALL,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
                Node {
                    margin: UiRect::bottom(Val::Px(8.0)),
                    ..default()
                },
            ))
            .id();
        commands.entity(container).add_child(ping);
    }

    spawn_animated_section_divider(commands, container, "FACTIONS", fonts);

    for i in 0..4 {
        spawn_client_slot_card(commands, container, i, config, lobby, my_faction);
    }
}

// ── Client Slot Card ──

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
            card.spawn((
                Node {
                    width: Val::Px(16.0),
                    height: Val::Px(16.0),
                    border_radius: BorderRadius::all(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(faction_color),
            ));
            card.spawn((
                Text::new(display_name),
                TextFont { font_size: theme::FONT_MEDIUM, ..default() },
                TextColor(if is_me { theme::ACCENT } else { faction_color }),
            ));
            card.spawn((
                Text::new(type_label),
                TextFont { font_size: theme::FONT_SMALL, ..default() },
                TextColor(theme::TEXT_SECONDARY),
            ));
            card.spawn(Node { flex_grow: 1.0, ..default() });
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
