use bevy::prelude::*;

use super::super::helpers::*;
use crate::infrastructure::multiplayer::{LobbyState, LobbyStatus, NetRole};
use crate::types::*;
use crate::ui::core::components as ui_components;
use crate::ui::fonts::{self, UiFonts};
use crate::ui::theme::{Theme, SUCCESS, TEAM_COLORS, TEXT_PRIMARY};

use super::super::new_game;
use super::super::*;

// ── Multiplayer Page ──

pub(crate) fn spawn_multiplayer_page(
    commands: &mut Commands,
    container: Entity,
    fonts: &UiFonts,
    theme: &Theme,
) {
    spawn_page_header(
        commands,
        container,
        "MULTIPLAYER",
        MenuButton(MenuAction::Back),
        fonts,
        theme,
    );

    spawn_animated_section_divider(commands, container, "NETWORK GAME", fonts, theme);

    let desc_text = if cfg!(target_arch = "wasm32") {
        "Join a hosted session from the web client"
    } else {
        "Play with others on your network or via VPN"
    };
    let desc = commands
        .spawn((
            Text::new(desc_text),
            TextFont {
                font_size: theme.typography.medium,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
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
            None,
            theme,
        );
        commands.entity(container).add_child(host_btn);
    }

    let join_btn = spawn_styled_button(
        commands,
        "JOIN GAME",
        MenuButton(MenuAction::JoinGame),
        false,
        fonts,
        None,
        theme,
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
    theme: &Theme,
) {
    spawn_page_header(
        commands,
        container,
        "HOST LOBBY",
        MenuButton(MenuAction::CancelHost),
        fonts,
        theme,
    );

    // ── Command Profile (host name input) ──
    new_game::spawn_command_profile_row(commands, container, &config.player_name, theme);

    spawn_animated_section_divider(commands, container, "SESSION CODE", fonts, theme);

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
                TextColor(theme.colors.accent),
            ));
            parent
                .spawn((
                    CopyCodeButton,
                    Button,
                    ui_components::compact_button_node(14.0, 7.0),
                    ui_components::filled_button_chrome(theme, ui_components::UiTone::Neutral),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        CopyCodeLabel,
                        Text::new("COPY"),
                        TextFont {
                            font_size: theme.typography.medium,
                            ..default()
                        },
                        TextColor(theme.colors.text_secondary),
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
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
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
                    SUCCESS
                } else {
                    theme.colors.text_secondary
                };
                parent.spawn((
                    Text::new(label),
                    TextFont {
                        font_size: theme.typography.small,
                        ..default()
                    },
                    TextColor(color),
                ));
            }
        })
        .id();
    commands.entity(container).add_child(ip_list);

    spawn_animated_section_divider(commands, container, "FACTIONS", fonts, theme);

    let slots_wrap = commands
        .spawn((
            SlotCardsContainer,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(slots_wrap);

    for i in 0..4 {
        new_game::spawn_slot_card(
            commands,
            slots_wrap,
            i,
            config,
            true,
            Some(&lobby.players),
            theme,
        );
    }

    // ── World Settings ──

    spawn_animated_section_divider(commands, container, "WORLD", fonts, theme);

    let map_idx = match config.map_size {
        MapSize::Small => 0,
        MapSize::Medium => 1,
        MapSize::Large => 2,
        MapSize::ExtraLarge => 3,
    };
    spawn_selector_row(
        commands,
        container,
        "Map Size:",
        &["Small", "Medium", "Large", "EPIC"],
        map_idx,
        SelectorField::MapSize,
        None,
        theme,
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
        None,
        theme,
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
        None,
        theme,
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
        None,
        theme,
    );

    spawn_animated_section_divider(commands, container, "", fonts, theme);

    let status = commands
        .spawn((
            LobbyStatusText,
            Text::new("Waiting for players..."),
            TextFont {
                font_size: theme.typography.medium,
                ..default()
            },
            TextColor(theme.colors.text_secondary),
            Node {
                margin: UiRect::vertical(Val::Px(8.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(status);

    let connected_count = lobby.players.iter().filter(|p| p.connected).count();
    let mut start_cmd = commands.spawn((
        MenuButton(MenuAction::StartMultiplayer),
        Button,
        ButtonAnimState::new(theme.colors.accent.to_srgba().to_f32_array()),
        ButtonStyle::Filled,
        Node {
            width: Val::Px(280.0),
            height: Val::Px(80.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            margin: UiRect::top(Val::Px(12.0)),
            ..default()
        },
        BackgroundColor(theme.colors.accent),
    ));
    if connected_count <= 1 {
        start_cmd.insert(ButtonDisabled(Some(
            "Waiting for players to join...".to_string(),
        )));
    }
    start_cmd.with_children(|parent| {
        parent.spawn((
            StartButtonText,
            Text::new("START GAME"),
            fonts::heading(fonts, theme.typography.button),
            TextColor(TEXT_PRIMARY),
            Pickable::IGNORE,
        ));
    });
    let start_btn = start_cmd.id();
    commands.entity(container).add_child(start_btn);
    spawn_button_hint(commands, container, theme);
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
    theme: &Theme,
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
        theme,
    );

    // ── Command Profile (player name input) ──
    new_game::spawn_command_profile_row(commands, container, &config.player_name, theme);

    // ── Connection state banner ──
    let (banner_dot_color, banner_text, banner_text_color, banner_bg) = if is_connected {
        (
            theme.colors.success,
            "CONNECTED".to_string(),
            theme.colors.success,
            Color::srgba(0.15, 0.35, 0.15, 0.4),
        )
    } else if is_connecting {
        (
            theme.colors.warning,
            "CONNECTING...".to_string(),
            theme.colors.warning,
            Color::srgba(0.35, 0.25, 0.1, 0.4),
        )
    } else if is_failed {
        (
            theme.colors.destructive,
            "DISCONNECTED".to_string(),
            theme.colors.destructive,
            Color::srgba(0.35, 0.15, 0.15, 0.4),
        )
    } else {
        (
            theme.colors.text_secondary,
            "NOT CONNECTED".to_string(),
            theme.colors.text_secondary,
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
                // border_radius: BorderRadius::all(Val::Px(6.0)),
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
                    // border_radius: BorderRadius::all(Val::Px(5.0)),
                    ..default()
                },
                BackgroundColor(banner_dot_color),
            ));
            let _ = &dot; // dot may pulse in the future
            parent.spawn((
                Text::new(banner_text),
                TextFont {
                    font_size: theme.typography.medium,
                    ..default()
                },
                TextColor(banner_text_color),
                Pickable::IGNORE,
            ));
        })
        .id();
    commands.entity(container).add_child(banner);

    spawn_animated_section_divider(commands, container, "SESSION CODE", fonts, theme);

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
                        font_size: theme.typography.medium,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
                ));
                parent.spawn((
                    Text::new(code_display),
                    TextFont {
                        font_size: theme.typography.medium,
                        ..default()
                    },
                    TextColor(theme.colors.accent),
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
                        font_size: theme.typography.medium,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
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
                            selection_anchor: None,
                            max_len: 45,
                        },
                        Button,
                        ui_components::input_node(240.0, 32.0),
                        ui_components::input_chrome(theme),
                    ))
                    .with_children(|input| {
                        crate::ui::core::text_input::spawn_text_input_children(input, "", theme);
                    });

                // Paste button
                parent
                    .spawn((
                        PasteCodeButton,
                        Button,
                        ui_components::compact_button_node(10.0, 6.0),
                        ui_components::filled_button_chrome(theme, ui_components::UiTone::Neutral),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("PASTE"),
                            TextFont {
                                font_size: theme.typography.small,
                                ..default()
                            },
                            TextColor(theme.colors.text_primary),
                            Pickable::IGNORE,
                        ));
                    });

                // Clear button
                parent
                    .spawn((
                        ClearCodeButton,
                        Button,
                        ui_components::compact_button_node(10.0, 6.0),
                        ui_components::ghost_button_chrome(theme, ui_components::UiTone::Neutral),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("CLEAR"),
                            TextFont {
                                font_size: theme.typography.small,
                                ..default()
                            },
                            TextColor(theme.colors.text_secondary),
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
                    {
                        let mut node = ui_components::compact_button_node(8.0, 8.0);
                        node.width = Val::Px(120.0);
                        node.align_content = AlignContent::Center;
                        node.align_items = AlignItems::Center;
                        node.margin = UiRect::bottom(Val::Px(6.0));
                        node.border_radius = BorderRadius::all(Val::Px(8.0));
                        node
                    },
                    ui_components::ghost_button_chrome(theme, ui_components::UiTone::Neutral),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("FIND LAN HOSTS"),
                        fonts::heading(fonts, theme.typography.medium),
                        TextColor(theme.colors.text_primary),
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
            None,
            theme,
        );
    }

    // ── Conditional CONNECT vs DISCONNECT ──
    if is_connected {
        let dc_btn = commands
            .spawn((
                MenuButton(MenuAction::Disconnect),
                Button,
                ButtonAnimState::new(theme.colors.destructive.to_srgba().to_f32_array()),
                ButtonStyle::Filled,
                Node {
                    width: Val::Px(220.0),
                    height: Val::Px(44.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    margin: UiRect::vertical(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(theme.colors.destructive),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Text::new("DISCONNECT"),
                    fonts::heading(fonts, theme.typography.button),
                    TextColor(TEXT_PRIMARY),
                    Pickable::IGNORE,
                ));
            })
            .id();
        commands.entity(container).add_child(dc_btn);
    } else {
        let label = if is_connecting {
            "CONNECTING..."
        } else {
            "CONNECT"
        };
        let connect_btn = spawn_styled_button(
            commands,
            label,
            MenuButton(MenuAction::ConnectToHost),
            true,
            fonts,
            None,
            theme,
        );
        // Disable when connecting or when session code is empty
        let has_code = lobby.client_session_code.trim().len() > 0;
        if is_connecting || !has_code {
            let reason = if is_connecting {
                "Connecting to host...".to_string()
            } else {
                "Enter a session code or IP address".to_string()
            };
            commands
                .entity(connect_btn)
                .insert(ButtonDisabled(Some(reason)));
        }
        commands.entity(container).add_child(connect_btn);
        spawn_button_hint(commands, container, theme);
    }

    spawn_animated_section_divider(commands, container, "STATUS", fonts, theme);

    // ── Color-coded status text ──
    let (status_text, status_color) = match &lobby.status {
        LobbyStatus::Connected => (
            "Connected! Waiting for host to start...".to_string(),
            theme.colors.success,
        ),
        LobbyStatus::Connecting => ("Connecting...".to_string(), theme.colors.warning),
        LobbyStatus::Failed(e) => (format!("Failed: {}", e), theme.colors.destructive),
        LobbyStatus::Waiting => (
            if cfg!(target_arch = "wasm32") {
                "Enter a hosted session code and press CONNECT".to_string()
            } else {
                "Enter the host's session code or scan your LAN and press CONNECT".to_string()
            },
            theme.colors.text_secondary,
        ),
    };

    let status = commands
        .spawn((
            LobbyStatusText,
            Text::new(status_text),
            TextFont {
                font_size: theme.typography.medium,
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
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
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
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
                Node {
                    margin: UiRect::bottom(Val::Px(8.0)),
                    ..default()
                },
            ))
            .id();
        commands.entity(container).add_child(ping);
    }

    spawn_animated_section_divider(commands, container, "FACTIONS", fonts, theme);

    for i in 0..4 {
        spawn_client_slot_card(commands, container, i, config, lobby, my_faction, theme);
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
    theme: &Theme,
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

    let team_color = TEAM_COLORS
        .get(team as usize)
        .copied()
        .unwrap_or(TEAM_COLORS[0]);

    let border_color = if is_me {
        theme.colors.accent
    } else {
        theme.colors.separator
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
            ui_components::card_chrome(theme, border_color),
        ))
        .with_children(|card| {
            if let Some(player) = lobby_player {
                let dot_color = if player.connected {
                    theme.colors.success
                } else {
                    theme.colors.destructive
                };
                card.spawn((
                    ui_components::badge_node(8.0, 4.0),
                    BackgroundColor(dot_color),
                ));
            }
            card.spawn((
                ui_components::badge_node(16.0, 8.0),
                BackgroundColor(faction_color),
            ));
            card.spawn((
                Text::new(display_name),
                TextFont {
                    font_size: theme.typography.medium,
                    ..default()
                },
                TextColor(if is_me {
                    theme.colors.accent
                } else {
                    faction_color
                }),
            ));
            card.spawn((
                Text::new(type_label),
                TextFont {
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
            ));
            card.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            if !matches!(slot, SlotOccupant::Closed | SlotOccupant::Open) {
                card.spawn((
                    ui_components::badge_node(22.0, 4.0),
                    BackgroundColor(team_color),
                ))
                .with_children(|badge| {
                    badge.spawn((
                        Text::new(format!("{}", team + 1)),
                        TextFont {
                            font_size: 10.0,
                            ..default()
                        },
                        TextColor(TEXT_PRIMARY),
                        Pickable::IGNORE,
                    ));
                });
            }
        })
        .id();
    commands.entity(container).add_child(card);
}
