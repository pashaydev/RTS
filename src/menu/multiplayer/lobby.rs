use std::sync::atomic::Ordering;

use bevy::prelude::*;
use bevy_matchbox::prelude::*;
use game_state::codec;

use crate::components::*;
use crate::multiplayer::{
    self, ClientNetState, HostNetState, LobbyPlayer, LobbyState, LobbyStatus, NetRole,
    matchbox_transport::{self, MatchboxInbox, PeerMap},
};
use crate::theme;
use crate::ui::menu_helpers::*;

use super::super::*;
use super::{
    PendingGameStart, PendingLobbyBroadcast, DEFAULT_PORT,
    first_open_multiplayer_slot, sync_multiplayer_slots_from_lobby,
};
use super::config::SerializableGameConfig;

// ── Update Lobby UI ──

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
    pending: (Option<Res<PendingGameStart>>, Option<Res<PendingLobbyBroadcast>>),
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
    texts: (
        Query<&mut Text, With<SessionCodeText>>,
        Query<&mut Text, (With<LobbyStatusText>, Without<SessionCodeText>)>,
    ),
    mut config: ResMut<GameSetupConfig>,
    lists: (
        Query<Entity, (With<HostIpList>, Without<HostIpListPopulated>)>,
        Query<Entity, (With<DiscoveredHostsList>, Without<DiscoveredHostsListPopulated>)>,
    ),
    mut session_tokens: ResMut<multiplayer::SessionTokens>,
    roots: Query<Entity, With<MenuRoot>>,
    extra: (
        Option<Res<PreferredFaction>>,
        Option<ResMut<CountdownState>>,
    ),
) {
    let (mut socket, mut peer_map, mut inbox) = matchbox;
    let (pending_start, pending_broadcast) = pending;
    let (mut session_code_texts, mut status_texts) = texts;
    let (ip_list_q, discovered_list_q) = lists;
    let (preferred_faction, mut countdown) = extra;

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

            let Some(slot_index) = first_open_multiplayer_slot(&config, &lobby) else {
                warn!("Rejecting peer {:?}: no open multiplayer slots remain", peer);
                continue;
            };
            let seat_index = slot_index as u8;
            let faction = Faction::PLAYERS[slot_index];
            let color_index = slot_index as u8;

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
                    player_name, preferred_faction_index, ..
                } => {
                    if let Some(player) =
                        lobby.players.iter_mut().find(|p| p.player_id == player_id)
                    {
                        if !player_name.trim().is_empty() {
                            player.name = player_name;
                        }
                    }

                    // Honor preferred faction if the slot is open
                    if let Some(pref_idx) = preferred_faction_index {
                        let pref = pref_idx as usize;
                        let slot_available = pref < 4
                            && matches!(config.slots[pref], SlotOccupant::Open)
                            && !lobby.players.iter().any(|p| {
                                p.connected
                                    && p.player_id != player_id
                                    && p.faction == Faction::PLAYERS[pref]
                            });
                        if slot_available {
                            if let Some(player) = lobby.players.iter_mut().find(|p| p.player_id == player_id) {
                                player.seat_index = pref as u8;
                                player.faction = Faction::PLAYERS[pref];
                                player.color_index = pref as u8;
                            }
                        }
                    }

                    if let Some(player) =
                        lobby.players.iter().find(|p| p.player_id == player_id)
                    {
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
                game_state::message::ClientMessage::Reconnect { .. } => {}
                game_state::message::ClientMessage::Chat { .. } => {}
            }
        }

        if lobby_changed {
            sync_multiplayer_slots_from_lobby(&mut config, &lobby);
            lobby.players.retain(|p| p.connected);
            broadcast_lobby_update_matchbox(&lobby, socket, &config);
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

        if pending_broadcast.is_some() {
            broadcast_lobby_update_matchbox(&lobby, socket, &config);
            commands.remove_resource::<PendingLobbyBroadcast>();
        }

        // ── Host: broadcast countdown start to clients ──
        if let Some(ref mut cd) = countdown {
            if !cd.broadcast_sent {
                cd.broadcast_sent = true;
                let event = game_state::message::ServerMessage::Event {
                    seq: 0,
                    timestamp: 0.0,
                    events: vec![game_state::message::GameEvent::CountdownStart],
                };
                matchbox_transport::broadcast_reliable(socket, &event);
            }
        }

        // ── Host: handle PendingGameStart ──
        if pending_start.is_some() {
            if config.map_seed == 0 {
                config.map_seed = rand::random::<u64>();
                info!("Host resolved random map seed: {}", config.map_seed);
            }

            sync_multiplayer_slots_from_lobby(&mut config, &lobby);
            for player in &lobby.players {
                if player.connected && player.is_host {
                    let faction_idx = Faction::PLAYERS
                        .iter()
                        .position(|f| *f == player.faction)
                        .unwrap_or(0);
                    config.local_player_slot = faction_idx;
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
            .load(Ordering::Relaxed)
            && !matches!(lobby.status, LobbyStatus::Connected)
        {
            let error_msg = if matches!(lobby.status, LobbyStatus::Connecting) {
                "Could not reach host. Check your session code and ensure the host is running."
            } else {
                "Host disconnected. The game session may have ended."
            };
            lobby.status = LobbyStatus::Failed(error_msg.to_string());
            for mut text in &mut status_texts {
                **text = format!("Failed: {}", error_msg);
            }
            commands.close_socket();
            commands.remove_resource::<ClientNetState>();
            commands.remove_resource::<ConnectionTimer>();
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
                        commands.remove_resource::<ConnectionTimer>();
                        commands.insert_resource(LobbyPingTimer(Timer::from_seconds(2.0, TimerMode::Repeating)));
                        for e in &roots {
                            commands.entity(e).try_despawn();
                        }
                        // Send JoinRequest
                        let player_name = if config.player_name.trim().is_empty() {
                            "Client".to_string()
                        } else {
                            config.player_name.clone()
                        };
                        let preferred = preferred_faction.as_ref()
                            .and_then(|pf| pf.0);
                        let join_msg = game_state::message::ClientMessage::JoinRequest {
                            seq: 0,
                            timestamp: 0.0,
                            player_name,
                            preferred_faction_index: preferred,
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
                            game_state::message::GameEvent::Chat { .. } => {}
                            game_state::message::GameEvent::CountdownStart => {
                                commands.insert_resource(CountdownState {
                                    timer: Timer::from_seconds(3.0, TimerMode::Once),
                                    current_digit: 3,
                                    broadcast_sent: true,
                                });
                            }
                            game_state::message::GameEvent::CountdownCancel => {
                                commands.remove_resource::<CountdownState>();
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

// ── Broadcast ──

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn broadcast_lobby_update(
    lobby: &LobbyState,
    host: &HostNetState,
    config: &GameSetupConfig,
) {
    let _ = (lobby, host, config);
}

/// Broadcast lobby update to all connected peers via Matchbox.
pub(super) fn broadcast_lobby_update_matchbox(
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

// ── Web Client URL ──

pub(crate) fn update_web_client_url(
    lobby: Res<LobbyState>,
    mut texts: Query<&mut Text, With<WebClientUrlText>>,
) {
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

// ── Session Code Clipboard ──

pub(crate) fn copy_session_code_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<CopyCodeButton>)>,
    lobby: Res<LobbyState>,
    mut labels: Query<&mut Text, With<CopyCodeLabel>>,
    mut commands: Commands,
) {
    for interaction in &interactions {
        if *interaction == Interaction::Pressed && !lobby.session_code.is_empty() {
            clipboard_write(&lobby.session_code);
            for mut text in &mut labels {
                **text = "COPIED!".to_string();
            }
            commands.insert_resource(CopyResetTimer(Timer::from_seconds(2.0, TimerMode::Once)));
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

pub(crate) fn copy_reset_system(
    timer: Option<ResMut<CopyResetTimer>>,
    mut commands: Commands,
    mut labels: Query<&mut Text, With<CopyCodeLabel>>,
    time: Res<Time>,
) {
    let Some(mut timer) = timer else { return };
    timer.0.tick(time.delta());
    if timer.0.just_finished() {
        for mut text in &mut labels {
            **text = "COPY".to_string();
        }
        commands.remove_resource::<CopyResetTimer>();
    }
}

// ── Connection Timer ──

pub(crate) fn connection_timer_system(
    timer: Option<ResMut<ConnectionTimer>>,
    mut status_texts: Query<&mut Text, (With<LobbyStatusText>, Without<ConnectionElapsedText>)>,
    mut elapsed_texts: Query<&mut Text, (With<ConnectionElapsedText>, Without<LobbyStatusText>)>,
    lobby: Res<LobbyState>,
    time: Res<Time>,
) {
    let Some(mut timer) = timer else { return };
    if !matches!(lobby.status, LobbyStatus::Connecting) {
        return;
    }

    timer.started += time.delta_secs_f64();
    timer.dot_timer += time.delta_secs();

    if timer.dot_timer >= 0.4 {
        timer.dot_timer = 0.0;
        timer.dot_phase = (timer.dot_phase + 1) % 3;
    }

    let dots = match timer.dot_phase {
        0 => ".",
        1 => "..",
        _ => "...",
    };

    let elapsed_secs = timer.started as u32;

    if elapsed_secs >= 15 {
        for mut text in &mut status_texts {
            **text = format!("Connection is taking longer than expected{}", dots);
        }
    } else {
        for mut text in &mut status_texts {
            **text = format!("Connecting via WebRTC{}", dots);
        }
    }

    for mut text in &mut elapsed_texts {
        **text = format!("Elapsed: {}s", elapsed_secs);
    }
}

// ── Countdown ──

pub(crate) fn countdown_system(
    state: Option<ResMut<CountdownState>>,
    mut commands: Commands,
    time: Res<Time>,
    mut overlay_texts: Query<&mut Text, With<CountdownOverlay>>,
    mut start_texts: Query<&mut Text, (With<StartButtonText>, Without<CountdownOverlay>)>,
) {
    let Some(mut state) = state else { return };
    state.timer.tick(time.delta());

    let remaining = (state.timer.remaining_secs() + 0.99) as u8;
    if remaining != state.current_digit && remaining > 0 {
        state.current_digit = remaining;
        for mut text in &mut overlay_texts {
            **text = format!("{}", remaining);
        }
    }

    for mut text in &mut start_texts {
        **text = "CANCEL".to_string();
    }

    if state.timer.just_finished() {
        commands.remove_resource::<CountdownState>();
        commands.insert_resource(PendingGameStart);
        for mut text in &mut overlay_texts {
            **text = "GO!".to_string();
        }
    }
}

// ── Kick Player ──

pub(crate) fn kick_player_system(
    interactions: Query<(&Interaction, &KickPlayerButton), Changed<Interaction>>,
    mut lobby: ResMut<LobbyState>,
    mut config: ResMut<GameSetupConfig>,
    mut socket: Option<ResMut<MatchboxSocket>>,
    mut commands: Commands,
    roots: Query<Entity, With<MenuRoot>>,
) {
    for (interaction, kick_btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let slot_index = kick_btn.0;
        let faction = Faction::PLAYERS[slot_index];

        if let Some(player) = lobby.players.iter_mut().find(|p| p.faction == faction && p.connected) {
            let player_id = player.player_id;
            player.connected = false;
            info!("Host kicked player {} from slot {}", player_id, slot_index);
        }

        lobby.players.retain(|p| p.connected);
        sync_multiplayer_slots_from_lobby(&mut config, &lobby);

        if let Some(ref mut socket) = socket {
            broadcast_lobby_update_matchbox(&lobby, socket, &config);
        }

        for e in &roots {
            commands.entity(e).try_despawn();
        }
    }
}

// ── Lobby Ping ──

pub(crate) fn lobby_ping_system(
    mut ping_timer: Option<ResMut<LobbyPingTimer>>,
    mut commands: Commands,
    time: Res<Time>,
    lobby: Res<LobbyState>,
    client_state: Option<Res<ClientNetState>>,
    mut socket: Option<ResMut<MatchboxSocket>>,
) {
    if !matches!(lobby.status, LobbyStatus::Connected) {
        return;
    }
    let Some(ref client) = client_state else { return };

    if ping_timer.is_none() {
        commands.insert_resource(LobbyPingTimer(Timer::from_seconds(2.0, TimerMode::Repeating)));
        return;
    }

    let Some(ref mut timer) = ping_timer else { return };
    timer.0.tick(time.delta());
    if timer.0.just_finished() {
        if let Some(ref mut socket) = socket {
            let seq = {
                let mut s = client.seq.lock().unwrap();
                *s += 1;
                *s
            };
            let ping = game_state::message::ClientMessage::Ping {
                seq,
                timestamp: time.elapsed_secs_f64(),
            };
            matchbox_transport::send_to_host(socket, &ping);
        }
    }
}
