use bevy::ecs::message::MessageWriter;
use bevy::prelude::*;
use rand::Rng;

use crate::components::*;
use crate::theme;
use crate::ui::fonts::UiFonts;
use crate::ui::menu_helpers::*;

use super::*;
use crate::multiplayer::{ClientNetState, HostNetState, LobbyState};
#[cfg(not(target_arch = "wasm32"))]
use super::multiplayer::start_hosting;
use super::multiplayer;

// ── Spawn / Cleanup ──

pub(crate) fn spawn_menu(
    mut commands: Commands,
    page: Res<MenuPage>,
    config: Res<GameSetupConfig>,
    graphics: Res<GraphicsSettings>,
    fonts: Res<UiFonts>,
    restart: Option<Res<RestartRequested>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if restart.is_some() {
        commands.remove_resource::<RestartRequested>();
        next_state.set(AppState::InGame);
        return;
    }

    commands.spawn((
        MenuCamera,
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(theme::BG_MENU),
            ..default()
        },
    ));

    let root = commands
        .spawn((
            MenuRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(theme::BG_MENU),
        ))
        .id();

    let container = spawn_menu_panel(&mut commands);
    commands.entity(root).add_child(container);
    dispatch_page(&mut commands, container, &page, &config, &graphics, &fonts);
}

pub(crate) fn cleanup_menu(
    mut commands: Commands,
    roots: Query<Entity, With<MenuRoot>>,
    cameras: Query<Entity, With<MenuCamera>>,
) {
    for e in &roots {
        commands.entity(e).try_despawn();
    }
    for e in &cameras {
        commands.entity(e).try_despawn();
    }
}

fn dispatch_page(
    commands: &mut Commands,
    container: Entity,
    page: &MenuPage,
    config: &GameSetupConfig,
    graphics: &GraphicsSettings,
    fonts: &UiFonts,
) {
    match *page {
        MenuPage::Title => pages::spawn_title_page(commands, container, fonts),
        MenuPage::NewGame => pages::spawn_new_game_page(commands, container, config, fonts),
        MenuPage::Options => pages::spawn_options_page(commands, container, graphics, fonts),
        MenuPage::Multiplayer => {
            multiplayer::spawn_multiplayer_page(commands, container, fonts)
        }
        MenuPage::HostLobby => {
            multiplayer::spawn_host_lobby_page(commands, container, config, fonts)
        }
        MenuPage::JoinLobby => {
            multiplayer::spawn_join_lobby_page(commands, container, config, fonts)
        }
    }
}

// ── Page Transition ──

pub(crate) fn page_transition_system(
    mut commands: Commands,
    roots: Query<Entity, With<MenuRoot>>,
    page: Res<MenuPage>,
    config: Res<GameSetupConfig>,
    graphics: Res<GraphicsSettings>,
    fonts: Res<UiFonts>,
) {
    if roots.iter().next().is_none() {
        let root = commands
            .spawn((
                MenuRoot,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                BackgroundColor(theme::BG_MENU),
            ))
            .id();

        let container = spawn_menu_panel(&mut commands);
        commands.entity(root).add_child(container);
        dispatch_page(&mut commands, container, &page, &config, &graphics, &fonts);
    }
}

// ── Menu Button Handler ──

pub(crate) fn handle_menu_buttons(
    interactions: Query<(&Interaction, &MenuButton), Changed<Interaction>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut page: ResMut<MenuPage>,
    config: Res<GameSetupConfig>,
    graphics: Res<GraphicsSettings>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
    roots: Query<Entity, With<MenuRoot>>,
    mut windows: Query<&mut Window>,
    host_state: Option<Res<HostNetState>>,
    client_state: Option<Res<ClientNetState>>,
    #[cfg(not(target_arch = "wasm32"))] host_factory: Option<Res<multiplayer::HostConnectionFactory>>,
) {
    for (interaction, btn) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match btn.0 {
            MenuAction::NewGame => {
                *page = MenuPage::NewGame;
                rebuild_menu(&mut commands, &roots);
            }
            MenuAction::Options => {
                *page = MenuPage::Options;
                rebuild_menu(&mut commands, &roots);
            }
            MenuAction::Quit => {
                exit.write(AppExit::Success);
            }
            MenuAction::Back => {
                *page = MenuPage::Title;
                rebuild_menu(&mut commands, &roots);
            }
            MenuAction::StartGame => {
                next_state.set(AppState::InGame);
            }
            MenuAction::ApplySettings => {
                graphics.save();
                if let Ok(mut window) = windows.single_mut() {
                    let (w, h) = graphics.resolution;
                    window.resolution = (w, h).into();
                    window.mode = if graphics.fullscreen {
                        bevy::window::WindowMode::BorderlessFullscreen(MonitorSelection::Current)
                    } else {
                        bevy::window::WindowMode::Windowed
                    };
                }
                *page = MenuPage::Title;
                rebuild_menu(&mut commands, &roots);
            }
            MenuAction::Multiplayer => {
                *page = MenuPage::Multiplayer;
                rebuild_menu(&mut commands, &roots);
            }
            MenuAction::HostGame => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    start_hosting(&mut commands, &config);
                    *page = MenuPage::HostLobby;
                    rebuild_menu(&mut commands, &roots);
                }
            }
            MenuAction::JoinGame => {
                *page = MenuPage::JoinLobby;
                rebuild_menu(&mut commands, &roots);
            }
            MenuAction::ConnectToHost => {
                // Handled by connect_to_host_system
            }
            MenuAction::StartMultiplayer => {
                commands.insert_resource(multiplayer::PendingGameStart);
            }
            MenuAction::BackToMultiplayer => {
                *page = MenuPage::Multiplayer;
                rebuild_menu(&mut commands, &roots);
            }
            MenuAction::CopySessionCode => {
                // Copy session code — handled separately
            }
            MenuAction::CancelHost => {
                #[cfg(not(target_arch = "wasm32"))]
                multiplayer::stop_hosting(&mut commands, &host_state, &host_factory);
                *page = MenuPage::Multiplayer;
                rebuild_menu(&mut commands, &roots);
            }
            MenuAction::Disconnect => {
                multiplayer::stop_client(&mut commands, &client_state);
                *page = MenuPage::Multiplayer;
                rebuild_menu(&mut commands, &roots);
            }
        }
    }
}

fn rebuild_menu(commands: &mut Commands, roots: &Query<Entity, With<MenuRoot>>) {
    for e in roots {
        commands.entity(e).try_despawn();
    }
}

// ── Selector Clicks ──

pub(crate) fn handle_selector_clicks(
    interactions: Query<(&Interaction, &MenuSelector), Changed<Interaction>>,
    mut config: ResMut<GameSetupConfig>,
    mut graphics: ResMut<GraphicsSettings>,
    mut lobby: Option<ResMut<LobbyState>>,
    host_state: Option<Res<HostNetState>>,
    mut commands: Commands,
    roots: Query<Entity, With<MenuRoot>>,
) {
    for (interaction, selector) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match selector.field {
            SelectorField::SlotType(slot_idx) => {
                if slot_idx < 4 {
                    let new_occupant = match selector.index {
                        0 => SlotOccupant::Human,
                        1 => SlotOccupant::Ai(AiDifficulty::Medium),
                        _ => SlotOccupant::Closed,
                    };

                    // If setting to Human in single-player, move the previous human to AI
                    if matches!(new_occupant, SlotOccupant::Human) && !lobby.is_some() {
                        let old_local = config.local_player_slot;
                        if old_local != slot_idx {
                            config.slots[old_local] = SlotOccupant::Ai(AiDifficulty::Medium);
                        }
                        config.local_player_slot = slot_idx;
                    }
                    // Preserve existing difficulty if switching to AI and slot was already AI
                    let occupant = if matches!(new_occupant, SlotOccupant::Ai(_)) {
                        if let SlotOccupant::Ai(d) = config.slots[slot_idx] {
                            SlotOccupant::Ai(d)
                        } else {
                            new_occupant
                        }
                    } else {
                        new_occupant
                    };
                    config.slots[slot_idx] = occupant;

                    // For multiplayer: update lobby and broadcast
                    if let Some(ref mut lobby) = lobby {
                        #[cfg(not(target_arch = "wasm32"))]
                        if let Some(ref host) = host_state {
                            multiplayer::broadcast_lobby_update(lobby, host, &config);
                        }
                    }

                    // Rebuild the menu page to reflect structural changes (difficulty row)
                    rebuild_menu(&mut commands, &roots);
                }
            }
            SelectorField::SlotDifficulty(slot_idx) => {
                if slot_idx < 4 {
                    if matches!(config.slots[slot_idx], SlotOccupant::Ai(_)) {
                        config.slots[slot_idx] = SlotOccupant::Ai(match selector.index {
                            0 => AiDifficulty::Easy,
                            1 => AiDifficulty::Medium,
                            _ => AiDifficulty::Hard,
                        });
                        #[cfg(not(target_arch = "wasm32"))]
                        if let (Some(ref mut lobby), Some(ref host)) = (&mut lobby, &host_state) {
                            multiplayer::broadcast_lobby_update(lobby, host, &config);
                        }
                    }
                }
            }
            SelectorField::SlotTeam(slot_idx) => {
                if slot_idx < 4 && selector.index < 4 {
                    config.player_teams[slot_idx] = selector.index as u8;
                    config.team_mode = TeamMode::Custom;
                    #[cfg(not(target_arch = "wasm32"))]
                    if let (Some(ref mut lobby), Some(ref host)) = (&mut lobby, &host_state) {
                        multiplayer::broadcast_lobby_update(lobby, host, &config);
                    }
                    rebuild_menu(&mut commands, &roots);
                }
            }
            SelectorField::TeamMode => {
                config.team_mode = match selector.index {
                    0 => {
                        config.player_teams = [0, 1, 2, 3];
                        TeamMode::FFA
                    }
                    1 => {
                        config.player_teams = [0, 0, 1, 1];
                        TeamMode::Teams
                    }
                    _ => TeamMode::Custom,
                };
                #[cfg(not(target_arch = "wasm32"))]
                if let (Some(ref mut lobby), Some(ref host)) = (&mut lobby, &host_state) {
                    multiplayer::broadcast_lobby_update(lobby, host, &config);
                }
                // Rebuild to update team buttons
                rebuild_menu(&mut commands, &roots);
            }
            SelectorField::MapSize => {
                config.map_size = match selector.index {
                    0 => MapSize::Small,
                    1 => MapSize::Medium,
                    _ => MapSize::Large,
                };
            }
            SelectorField::ResourceDensity => {
                config.resource_density = match selector.index {
                    0 => ResourceDensity::Sparse,
                    1 => ResourceDensity::Normal,
                    _ => ResourceDensity::Dense,
                };
            }
            SelectorField::DayCycle => {
                if selector.index < DAY_CYCLE_OPTIONS.len() {
                    config.day_cycle_secs = DAY_CYCLE_OPTIONS[selector.index].0;
                }
            }
            SelectorField::StartingRes => {
                if selector.index < STARTING_RES_OPTIONS.len() {
                    config.starting_resources_mult = STARTING_RES_OPTIONS[selector.index].0;
                }
            }
            SelectorField::Resolution => {
                if selector.index < RESOLUTION_OPTIONS.len() {
                    graphics.resolution = RESOLUTION_OPTIONS[selector.index];
                }
            }
            SelectorField::Fullscreen => {
                graphics.fullscreen = selector.index == 0;
            }
            SelectorField::Shadows => {
                graphics.shadow_quality = match selector.index {
                    0 => ShadowQuality::Off,
                    1 => ShadowQuality::Low,
                    _ => ShadowQuality::High,
                };
            }
            SelectorField::EntityLights => {
                graphics.entity_lights = selector.index == 0;
            }
            SelectorField::UiScale => {
                if selector.index < UI_SCALE_OPTIONS.len() {
                    graphics.ui_scale = UI_SCALE_OPTIONS[selector.index].0;
                }
            }
            SelectorField::MapSeed => {
                // Handled by randomize_seed_system
            }
        }
    }
}

// ── Selector Visuals ──

pub(crate) fn update_selector_visuals(
    config: Res<GameSetupConfig>,
    graphics: Res<GraphicsSettings>,
    lobby: Option<Res<crate::multiplayer::LobbyState>>,
    mut selectors: Query<(
        &MenuSelector,
        &mut BackgroundColor,
        Option<&Children>,
        Entity,
        Option<&SelectedOption>,
        Option<&mut ButtonAnimState>,
    )>,
    mut text_colors: Query<&mut TextColor>,
    mut commands: Commands,
) {
    for (selector, mut bg, children, entity, was_selected, anim_state) in &mut selectors {
        let is_multiplayer = lobby.is_some();
        let should_be_selected = match selector.field {
            SelectorField::SlotType(slot_idx) => {
                if slot_idx < 4 {
                    let slot = config.slots[slot_idx];
                    let expected_idx = match slot {
                        SlotOccupant::Human | SlotOccupant::Open => 0,
                        SlotOccupant::Ai(_) => 1,
                        SlotOccupant::Closed => 2,
                    };
                    selector.index == expected_idx
                } else {
                    false
                }
            }
            SelectorField::SlotDifficulty(slot_idx) => {
                if slot_idx < 4 {
                    if let SlotOccupant::Ai(d) = config.slots[slot_idx] {
                        let diff_idx = match d {
                            AiDifficulty::Easy => 0,
                            AiDifficulty::Medium => 1,
                            AiDifficulty::Hard => 2,
                        };
                        selector.index == diff_idx
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            SelectorField::TeamMode => {
                selector.index
                    == match config.team_mode {
                        TeamMode::FFA => 0,
                        TeamMode::Teams => 1,
                        TeamMode::Custom => 2,
                    }
            }
            SelectorField::MapSize => {
                selector.index
                    == match config.map_size {
                        MapSize::Small => 0,
                        MapSize::Medium => 1,
                        MapSize::Large => 2,
                    }
            }
            SelectorField::ResourceDensity => {
                selector.index
                    == match config.resource_density {
                        ResourceDensity::Sparse => 0,
                        ResourceDensity::Normal => 1,
                        ResourceDensity::Dense => 2,
                    }
            }
            SelectorField::DayCycle => DAY_CYCLE_OPTIONS
                .get(selector.index)
                .map_or(false, |&(v, _)| (v - config.day_cycle_secs).abs() < 1.0),
            SelectorField::StartingRes => STARTING_RES_OPTIONS
                .get(selector.index)
                .map_or(false, |&(v, _)| {
                    (v - config.starting_resources_mult).abs() < 0.01
                }),
            SelectorField::Resolution => RESOLUTION_OPTIONS
                .get(selector.index)
                .map_or(false, |&r| r == graphics.resolution),
            SelectorField::Fullscreen => (selector.index == 0) == graphics.fullscreen,
            SelectorField::Shadows => {
                selector.index
                    == match graphics.shadow_quality {
                        ShadowQuality::Off => 0,
                        ShadowQuality::Low => 1,
                        ShadowQuality::High => 2,
                    }
            }
            SelectorField::EntityLights => (selector.index == 0) == graphics.entity_lights,
            SelectorField::UiScale => UI_SCALE_OPTIONS
                .get(selector.index)
                .map_or(false, |&(v, _)| (v - graphics.ui_scale).abs() < 0.01),
            SelectorField::SlotTeam(slot_idx) => {
                slot_idx < 4 && selector.index == config.player_teams[slot_idx] as usize
            }
            SelectorField::MapSeed => false,
        };

        // Team buttons use custom colors per team
        if let SelectorField::SlotTeam(_) = selector.field {
            let team_colors = [
                Color::srgb(0.9, 0.75, 0.2),
                Color::srgb(0.2, 0.75, 0.85),
                Color::srgb(0.85, 0.3, 0.65),
                Color::srgb(0.95, 0.5, 0.15),
            ];
            let color = team_colors.get(selector.index).copied().unwrap_or(team_colors[0]);
            let new_bg = if should_be_selected {
                color
            } else {
                Color::srgba(0.15, 0.15, 0.15, 0.8)
            };
            *bg = BackgroundColor(new_bg);
            commands.entity(entity).insert(BorderColor::all(if should_be_selected {
                Color::WHITE
            } else {
                Color::NONE
            }));
            if should_be_selected {
                let c = color.to_srgba();
                commands.entity(entity).insert(BoxShadow::new(
                    Color::srgba(c.red, c.green, c.blue, 0.5),
                    Val::Px(0.0), Val::Px(0.0), Val::Px(0.0), Val::Px(3.0),
                ));
            } else {
                commands.entity(entity).remove::<BoxShadow>();
            }
            if let Some(children) = children {
                for child in children.iter() {
                    if let Ok(mut tc) = text_colors.get_mut(child) {
                        tc.0 = if should_be_selected { Color::WHITE } else { color };
                    }
                }
            }
            if should_be_selected && was_selected.is_none() {
                commands.entity(entity).insert(SelectedOption);
            } else if !should_be_selected && was_selected.is_some() {
                commands.entity(entity).remove::<SelectedOption>();
            }
            continue;
        }

        let new_bg = if should_be_selected {
            theme::ACCENT
        } else {
            theme::BTN_PRIMARY
        };
        let text_col = if should_be_selected {
            Color::WHITE
        } else {
            theme::TEXT_SECONDARY
        };

        *bg = BackgroundColor(new_bg);

        commands
            .entity(entity)
            .insert(BorderColor::all(if should_be_selected {
                Color::srgba(0.29, 0.62, 1.0, 0.3)
            } else {
                Color::NONE
            }));

        if let Some(mut anim) = anim_state {
            anim.bg_current = new_bg.to_srgba().to_f32_array();
        }

        if let Some(children) = children {
            for child in children.iter() {
                if let Ok(mut tc) = text_colors.get_mut(child) {
                    tc.0 = text_col;
                }
            }
        }

        if should_be_selected && was_selected.is_none() {
            commands.entity(entity).insert(SelectedOption);
        } else if !should_be_selected && was_selected.is_some() {
            commands.entity(entity).remove::<SelectedOption>();
        }
    }
}

// Slot card rebuild is handled inline in handle_selector_clicks via rebuild_menu.

// ── Randomize Seed ──

pub(crate) fn randomize_seed_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<RandomizeSeedButton>)>,
    mut config: ResMut<GameSetupConfig>,
    mut seed_displays: Query<&mut Text, With<SeedDisplay>>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if config.map_seed == 0 {
            config.map_seed = rand::random::<u64>();
        } else {
            config.map_seed = 0;
        }
    }

    let seed_text = if config.map_seed == 0 {
        "Random".to_string()
    } else {
        format!("{}", config.map_seed)
    };
    for mut text in &mut seed_displays {
        **text = seed_text.clone();
    }
}

// ── Random Name ──

pub(crate) fn random_name_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<RandomNameButton>)>,
    mut config: ResMut<GameSetupConfig>,
    mut inputs: Query<(&mut TextInputField, &Children)>,
    mut text_query: Query<&mut Text, Without<TextInputCursor>>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let mut rng = rand::rng();
        let name = RANDOM_NAMES[rng.random_range(0..RANDOM_NAMES.len())].to_string();
        config.player_name = name.clone();

        for (mut field, children) in &mut inputs {
            field.value = name.clone();
            field.cursor_pos = name.len();
            for child in children.iter() {
                if let Ok(mut text) = text_query.get_mut(child) {
                    **text = name.clone();
                }
            }
        }
    }
}

