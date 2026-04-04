use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use rand::Rng;

use super::helpers::*;
use crate::components::*;
use crate::database::{ActiveProfile, GameDatabase};
use crate::theme::Theme;
use crate::ui::core::interactions::UiClickEvent;
use crate::ui::fonts::UiFonts;

use super::multiplayer;
#[cfg(not(target_arch = "wasm32"))]
use super::multiplayer::start_hosting;
use super::*;
use crate::multiplayer::{ClientNetState, HostNetState, LobbyState, NetRole};

// ── Spawn / Cleanup ──

pub(crate) fn spawn_menu(
    mut commands: Commands,
    page: Res<MenuPage>,
    config: Res<GameSetupConfig>,
    graphics: Res<GraphicsSettings>,
    audio_settings: Res<crate::audio::AudioSettings>,
    fonts: Res<UiFonts>,
    restart: Option<Res<RestartRequested>>,
    pending_load: Option<Res<crate::save_load::PendingLoad>>,
    mut next_state: ResMut<NextState<AppState>>,
    lobby: Res<LobbyState>,
    net_role: Option<Res<NetRole>>,
    client_state: Option<Res<ClientNetState>>,
    theme: Res<Theme>,
    db: Res<GameDatabase>,
    profile: Res<ActiveProfile>,
) {
    if restart.is_some() {
        commands.remove_resource::<RestartRequested>();
        next_state.set(AppState::InGame);
        return;
    }

    // If PendingLoad exists, skip the menu and go straight to InGame
    if pending_load.is_some() {
        next_state.set(AppState::InGame);
        return;
    }

    commands.spawn((
        MenuCamera,
        DespawnOnExit(AppState::MainMenu),
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(theme.colors.bg_menu),
            ..default()
        },
    ));

    let root = commands
        .spawn((
            MenuRoot,
            DespawnOnExit(AppState::MainMenu),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(theme.colors.bg_menu),
        ))
        .id();

    let panel = spawn_menu_panel(&mut commands, &theme);
    let content = commands
        .spawn((
            MenuContentRoot,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .id();
    commands.entity(panel).add_child(content);
    commands.entity(root).add_child(panel);

    dispatch_page(
        &mut commands,
        content,
        &page,
        &config,
        &graphics,
        &audio_settings,
        &fonts,
        &lobby,
        &net_role,
        &client_state,
        &theme,
        &db,
        &profile,
    );
}

fn dispatch_page(
    commands: &mut Commands,
    container: Entity,
    page: &MenuPage,
    config: &GameSetupConfig,
    graphics: &GraphicsSettings,
    audio_settings: &crate::audio::AudioSettings,
    fonts: &UiFonts,
    lobby: &LobbyState,
    net_role: &Option<Res<NetRole>>,
    client_state: &Option<Res<ClientNetState>>,
    theme: &Theme,
    db: &GameDatabase,
    profile: &ActiveProfile,
) {
    match *page {
        MenuPage::Title => pages::spawn_title_page(commands, container, fonts, theme),
        MenuPage::NewGame => pages::spawn_new_game_page(commands, container, config, fonts, theme),
        MenuPage::Options => {
            pages::spawn_options_page(commands, container, graphics, audio_settings, fonts, theme)
        }
        MenuPage::Multiplayer => {
            multiplayer::spawn_multiplayer_page(commands, container, fonts, theme)
        }
        MenuPage::HostLobby => {
            multiplayer::spawn_host_lobby_page(commands, container, config, fonts, lobby, theme)
        }
        MenuPage::JoinLobby => {
            let role = net_role.as_ref().map(|r| **r).unwrap_or(NetRole::Offline);
            let my_faction = client_state.as_ref().map(|c| c.my_faction);
            multiplayer::spawn_join_lobby_page(
                commands, container, config, fonts, lobby, role, my_faction, theme,
            )
        }
        MenuPage::LoadGame => {
            pages::spawn_load_game_page(commands, container, fonts, theme, db, profile)
        }
    }
}

// ── Page Transition ──

pub(crate) fn refresh_menu_page(
    mut commands: Commands,
    content_roots: Query<(Entity, Option<&Children>), With<MenuContentRoot>>,
    page: Res<MenuPage>,
    config: Res<GameSetupConfig>,
    graphics: Res<GraphicsSettings>,
    audio_settings: Res<crate::audio::AudioSettings>,
    fonts: Res<UiFonts>,
    lobby: Res<LobbyState>,
    net_role: Option<Res<NetRole>>,
    client_state: Option<Res<ClientNetState>>,
    theme: Res<Theme>,
    db: Res<GameDatabase>,
    profile: Res<ActiveProfile>,
) {
    if !page.is_changed() {
        return;
    }

    let Ok((content_root, children)) = content_roots.single() else {
        return;
    };

    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }

    dispatch_page(
        &mut commands,
        content_root,
        &page,
        &config,
        &graphics,
        &audio_settings,
        &fonts,
        &lobby,
        &net_role,
        &client_state,
        &theme,
        &db,
        &profile,
    );
}

/// Rebuild menu content when `MenuDirty` is inserted (without requiring a page change).
pub(crate) fn rebuild_dirty_menu(
    dirty: Option<Res<MenuDirty>>,
    mut commands: Commands,
    content_roots: Query<(Entity, Option<&Children>), With<MenuContentRoot>>,
    page: Res<MenuPage>,
    config: Res<GameSetupConfig>,
    graphics: Res<GraphicsSettings>,
    audio_settings: Res<crate::audio::AudioSettings>,
    fonts: Res<UiFonts>,
    lobby: Res<LobbyState>,
    net_role: Option<Res<NetRole>>,
    client_state: Option<Res<ClientNetState>>,
    theme: Res<Theme>,
    db: Res<GameDatabase>,
    profile: Res<ActiveProfile>,
) {
    if dirty.is_none() {
        return;
    }
    commands.remove_resource::<MenuDirty>();

    let Ok((content_root, children)) = content_roots.single() else {
        return;
    };

    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }

    dispatch_page(
        &mut commands,
        content_root,
        &page,
        &config,
        &graphics,
        &audio_settings,
        &fonts,
        &lobby,
        &net_role,
        &client_state,
        &theme,
        &db,
        &profile,
    );
}

// ── Menu Button Handler ──

pub(crate) fn handle_menu_buttons(
    mut click_events: MessageReader<UiClickEvent>,
    buttons: Query<&MenuButton>,
    mut next_state: ResMut<NextState<AppState>>,
    mut page: ResMut<MenuPage>,
    mut config: ResMut<GameSetupConfig>,
    graphics: Res<GraphicsSettings>,
    mut theme: ResMut<crate::theme::Theme>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
    mut windows: Query<&mut Window>,
    host_state: Option<Res<HostNetState>>,
    client_state: Option<Res<ClientNetState>>,
    db: Res<GameDatabase>,
    profile: Res<ActiveProfile>,
    mut widget_registry: ResMut<crate::ui::core::framework::WidgetRegistry>,
) {
    for event in click_events.read() {
        let Ok(btn) = buttons.get(event.entity) else {
            continue;
        };
        match btn.0 {
            MenuAction::NewGame => {
                *page = MenuPage::NewGame;
            }
            MenuAction::LoadGame => {
                *page = MenuPage::LoadGame;
            }
            MenuAction::Options => {
                *page = MenuPage::Options;
            }
            MenuAction::Quit => {
                exit.write(AppExit::Success);
            }
            MenuAction::Back => {
                *page = MenuPage::Title;
            }
            MenuAction::StartGame => {
                next_state.set(AppState::InGame);
            }
            MenuAction::ApplySettings => {
                theme.set_mode(graphics.theme_mode);
                if let Ok(mut window) = windows.single_mut() {
                    super::options::apply_graphics_settings(&graphics, &mut window);
                }
                *page = MenuPage::Title;
            }
            MenuAction::Multiplayer => {
                *page = MenuPage::Multiplayer;
            }
            MenuAction::HostGame => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    multiplayer::prepare_multiplayer_host_config(&mut config);
                    start_hosting(&mut commands, &config);
                    *page = MenuPage::HostLobby;
                }
            }
            MenuAction::JoinGame => {
                *page = MenuPage::JoinLobby;
            }
            MenuAction::ConnectToHost => {
                // Handled by connect_to_host_system
            }
            MenuAction::RefreshLanHosts => {
                // Handled by refresh_lan_hosts_system
            }
            MenuAction::StartMultiplayer => {
                commands.insert_resource(super::CountdownState {
                    timer: Timer::from_seconds(3.0, TimerMode::Once),
                    current_digit: 3,
                    broadcast_sent: false,
                });
            }
            MenuAction::BackToMultiplayer => {
                *page = MenuPage::Multiplayer;
            }
            MenuAction::CancelHost => {
                #[cfg(not(target_arch = "wasm32"))]
                multiplayer::stop_hosting(&mut commands, &host_state);
                *page = MenuPage::Multiplayer;
            }
            MenuAction::Disconnect => {
                multiplayer::stop_client(&mut commands, &client_state);
                *page = MenuPage::JoinLobby;
            }
            MenuAction::LoadSave(save_id) => {
                if let Some(blob) = db.load_save(save_id) {
                    match rmp_serde::from_slice::<crate::save_load::SaveData>(&blob) {
                        Ok(save_data) => {
                            info!("Loading save id={save_id}");
                            // Restore GameSetupConfig from save
                            config.map_seed = save_data.map_seed;
                            // Parse map_size, resource_density, etc. from saved strings
                            restore_config_from_save(&mut config, &save_data.game_config);
                            commands.insert_resource(crate::save_load::PendingLoad { save_data });
                            next_state.set(AppState::InGame);
                        }
                        Err(e) => {
                            error!("Failed to deserialize save: {e}");
                        }
                    }
                }
            }
            MenuAction::DeleteSave(save_id) => {
                db.delete_save(save_id);
                // Refresh the page
                commands.insert_resource(MenuDirty);
            }
            MenuAction::ResetWidgetLayout => {
                *widget_registry = crate::ui::core::framework::WidgetRegistry::default();
                info!("Widget layout reset to defaults");
            }
        }
    }
}

fn restore_config_from_save(
    config: &mut GameSetupConfig,
    saved: &crate::save_load::SavedGameConfig,
) {
    config.player_name = saved.player_name.clone();
    config.local_player_slot = saved.local_player_slot;
    config.player_teams = saved.player_teams;
    config.day_cycle_secs = saved.day_cycle_secs;
    config.starting_resources_mult = saved.starting_resources_mult;
    config.map_seed = saved.map_seed;

    // Parse map_size
    config.map_size = match saved.map_size.as_str() {
        "Small" => MapSize::Small,
        "Large" => MapSize::Large,
        _ => MapSize::Medium,
    };

    // Parse resource_density
    config.resource_density = match saved.resource_density.as_str() {
        "Sparse" => ResourceDensity::Sparse,
        "Dense" => ResourceDensity::Dense,
        _ => ResourceDensity::Normal,
    };

    // Parse team_mode
    config.team_mode = match saved.team_mode.as_str() {
        "Teams" => TeamMode::Teams,
        _ => TeamMode::FFA,
    };

    // Parse slots
    for (i, slot_str) in saved.slots.iter().enumerate() {
        if i >= config.slots.len() {
            break;
        }
        config.slots[i] = if slot_str == "Human" {
            SlotOccupant::Human
        } else if slot_str == "Open" {
            SlotOccupant::Open
        } else if slot_str == "Closed" {
            SlotOccupant::Closed
        } else if let Some(diff_str) = slot_str.strip_prefix("Ai:") {
            let diff = match diff_str {
                "Easy" => AiDifficulty::Easy,
                "Hard" => AiDifficulty::Hard,
                _ => AiDifficulty::Medium,
            };
            SlotOccupant::Ai(diff)
        } else {
            SlotOccupant::Closed
        };
    }
}

pub(crate) fn apply_window_settings_on_menu_enter(
    graphics: Res<GraphicsSettings>,
    mut windows: Query<&mut Window>,
) {
    if let Ok(mut window) = windows.single_mut() {
        super::options::apply_graphics_settings(&graphics, &mut window);
    }
}

/// Rebuild only the slot cards inside their wrapper, avoiding a full page rebuild
/// (which would replay panel fade-in / section-divider animations).
fn rebuild_slot_cards(
    commands: &mut Commands,
    slots_q: &Query<(Entity, &Children), With<SlotCardsContainer>>,
    config: &GameSetupConfig,
    is_multiplayer: bool,
    theme: &Theme,
) {
    if let Ok((container, children)) = slots_q.single() {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
        for i in 0..4 {
            pages::spawn_slot_card(commands, container, i, config, is_multiplayer, theme);
        }
    }
}

// ── Selector Clicks ──

pub(crate) fn handle_selector_clicks(
    interactions: Query<(&Interaction, &MenuSelector), Changed<Interaction>>,
    mut click_events: MessageReader<UiClickEvent>,
    all_selectors: Query<&MenuSelector>,
    mut config: ResMut<GameSetupConfig>,
    mut graphics: ResMut<GraphicsSettings>,
    mut lobby: Option<ResMut<LobbyState>>,
    host_state: Option<Res<HostNetState>>,
    page: Res<MenuPage>,
    mut commands: Commands,
    slots_container: Query<(Entity, &Children), With<SlotCardsContainer>>,
    theme: Res<Theme>,
) {
    // Collect selectors to process from both mouse interactions and keyboard events
    let mut to_process: Vec<MenuSelector> = Vec::new();

    for (interaction, selector) in &interactions {
        if *interaction == Interaction::Pressed {
            to_process.push(*selector);
        }
    }
    for event in click_events.read() {
        if let Ok(selector) = all_selectors.get(event.entity) {
            to_process.push(*selector);
        }
    }

    for selector in &to_process {
        match selector.field {
            SelectorField::SlotType(slot_idx) => {
                if slot_idx < 4 {
                    let new_occupant = if *page == MenuPage::HostLobby {
                        match selector.index {
                            0 => SlotOccupant::Human,
                            1 => SlotOccupant::Open,
                            2 => SlotOccupant::Ai(AiDifficulty::Medium),
                            _ => SlotOccupant::Closed,
                        }
                    } else {
                        match selector.index {
                            0 => SlotOccupant::Human,
                            1 => SlotOccupant::Ai(AiDifficulty::Medium),
                            _ => SlotOccupant::Closed,
                        }
                    };

                    // If setting to Human in single-player, move the previous human to AI
                    if matches!(new_occupant, SlotOccupant::Human) && *page == MenuPage::NewGame {
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
                            commands.insert_resource(multiplayer::PendingLobbyBroadcast);
                        }
                    }

                    // Rebuild only the slot cards (difficulty row may appear/disappear)
                    rebuild_slot_cards(
                        &mut commands,
                        &slots_container,
                        &config,
                        *page == MenuPage::HostLobby,
                        &theme,
                    );
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
                            commands.insert_resource(multiplayer::PendingLobbyBroadcast);
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
                        commands.insert_resource(multiplayer::PendingLobbyBroadcast);
                    }
                    // Visuals handled by update_selector_visuals — no rebuild needed
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
                    commands.insert_resource(multiplayer::PendingLobbyBroadcast);
                }
                // Visuals handled by update_selector_visuals — no rebuild needed
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
            SelectorField::Resolution
            | SelectorField::Fullscreen
            | SelectorField::Vsync
            | SelectorField::Shadows
            | SelectorField::EntityLights
            | SelectorField::AntiAliasing
            | SelectorField::Bloom
            | SelectorField::Brightness
            | SelectorField::AutoExposure
            | SelectorField::DepthOfField
            | SelectorField::ChromaticAberration
            | SelectorField::UiScale
            | SelectorField::ThemeMode => {
                super::options::apply_selector_change(
                    &selector.field,
                    selector.index,
                    &mut graphics,
                );
            }
            SelectorField::MusicVolume | SelectorField::SfxVolume => {
                // Handled by volume_slider_system.
            }
            SelectorField::MapSeed => {
                // Handled by randomize_seed_system
            }
            SelectorField::PreferredFaction => {
                let preferred = if selector.index == 0 {
                    None
                } else {
                    Some((selector.index - 1) as u8)
                };
                commands.insert_resource(super::PreferredFaction(preferred));
            }
        }
    }
}

// ── Selector Visuals ──

pub(crate) fn update_selector_visuals(
    config: Res<GameSetupConfig>,
    graphics: Res<GraphicsSettings>,
    page: Res<MenuPage>,
    preferred_faction: Option<Res<super::PreferredFaction>>,
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
    theme: Res<Theme>,
) {
    for (selector, mut bg, children, entity, was_selected, anim_state) in &mut selectors {
        let is_multiplayer = matches!(*page, MenuPage::HostLobby | MenuPage::JoinLobby);
        let should_be_selected = match selector.field {
            SelectorField::SlotType(slot_idx) => {
                if slot_idx < 4 {
                    let slot = config.slots[slot_idx];
                    let expected_idx = if is_multiplayer {
                        match slot {
                            SlotOccupant::Human => 0,
                            SlotOccupant::Open => 1,
                            SlotOccupant::Ai(_) => 2,
                            SlotOccupant::Closed => 3,
                        }
                    } else {
                        match slot {
                            SlotOccupant::Human => 0,
                            SlotOccupant::Ai(_) => 1,
                            SlotOccupant::Closed | SlotOccupant::Open => 2,
                        }
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
            SelectorField::Resolution => super::options::RESOLUTION_OPTIONS
                .get(selector.index)
                .map_or(false, |&r| r == graphics.resolution),
            SelectorField::Fullscreen => (selector.index == 0) == graphics.fullscreen,
            SelectorField::Vsync => (selector.index == 0) == graphics.vsync,
            SelectorField::Shadows => {
                selector.index
                    == match graphics.shadow_quality {
                        ShadowQuality::Off => 0,
                        ShadowQuality::Low => 1,
                        ShadowQuality::High => 2,
                    }
            }
            SelectorField::EntityLights => (selector.index == 0) == graphics.entity_lights,
            SelectorField::AntiAliasing => {
                selector.index
                    == match graphics.anti_aliasing {
                        AntiAliasingMode::Off => 0,
                        AntiAliasingMode::Smaa => 1,
                    }
            }
            SelectorField::Bloom => {
                selector.index
                    == match graphics.bloom {
                        EffectQuality::Off => 0,
                        EffectQuality::Low => 1,
                        EffectQuality::Medium => 2,
                        EffectQuality::High => 3,
                    }
            }
            SelectorField::Brightness => BRIGHTNESS_OPTIONS
                .get(selector.index)
                .map_or(false, |&(v, _)| (v - graphics.brightness).abs() < 0.01),
            SelectorField::AutoExposure => (selector.index == 0) == graphics.auto_exposure,
            SelectorField::DepthOfField => {
                selector.index
                    == match graphics.depth_of_field {
                        EffectQuality::Off => 0,
                        EffectQuality::Low => 1,
                        EffectQuality::Medium => 2,
                        EffectQuality::High => 3,
                    }
            }
            SelectorField::ChromaticAberration => {
                selector.index
                    == match graphics.chromatic_aberration {
                        EffectQuality::Off => 0,
                        EffectQuality::Low => 1,
                        EffectQuality::Medium => 2,
                        EffectQuality::High => 3,
                    }
            }
            SelectorField::UiScale => UI_SCALE_OPTIONS
                .get(selector.index)
                .map_or(false, |&(v, _)| (v - graphics.ui_scale).abs() < 0.01),
            SelectorField::ThemeMode => {
                selector.index
                    == match graphics.theme_mode {
                        crate::theme::ThemeMode::Dark => 0,
                        crate::theme::ThemeMode::Light => 1,
                    }
            }
            SelectorField::MusicVolume | SelectorField::SfxVolume => false,
            SelectorField::SlotTeam(slot_idx) => {
                slot_idx < 4 && selector.index == config.player_teams[slot_idx] as usize
            }
            SelectorField::MapSeed => false,
            SelectorField::PreferredFaction => {
                let pref_idx = preferred_faction
                    .as_ref()
                    .map(|pf| pf.0.map_or(0, |v| v as usize + 1))
                    .unwrap_or(0);
                selector.index == pref_idx
            }
        };

        // Team buttons use custom colors per team
        if let SelectorField::SlotTeam(_) = selector.field {
            let team_colors = [
                Color::srgb(0.9, 0.75, 0.2),
                Color::srgb(0.2, 0.75, 0.85),
                Color::srgb(0.85, 0.3, 0.65),
                Color::srgb(0.95, 0.5, 0.15),
            ];
            let color = team_colors
                .get(selector.index)
                .copied()
                .unwrap_or(team_colors[0]);
            let new_bg = if should_be_selected {
                color
            } else {
                Color::srgba(0.15, 0.15, 0.15, 0.8)
            };
            *bg = BackgroundColor(new_bg);
            commands
                .entity(entity)
                .insert(BorderColor::all(if should_be_selected {
                    Color::WHITE
                } else {
                    Color::NONE
                }));
            if should_be_selected {
                let c = color.to_srgba();
                commands.entity(entity).insert(BoxShadow::new(
                    Color::srgba(c.red, c.green, c.blue, 0.5),
                    Val::Px(0.0),
                    Val::Px(0.0),
                    Val::Px(0.0),
                    Val::Px(3.0),
                ));
            } else {
                commands.entity(entity).remove::<BoxShadow>();
            }
            if let Some(children) = children {
                for child in children.iter() {
                    if let Ok(mut tc) = text_colors.get_mut(child) {
                        tc.0 = if should_be_selected {
                            Color::WHITE
                        } else {
                            color
                        };
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
            theme.colors.accent
        } else {
            theme.colors.btn_primary
        };
        let text_col = if should_be_selected {
            Color::WHITE
        } else {
            theme.colors.text_secondary
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
    mut profile: ResMut<crate::database::ActiveProfile>,
    db: Res<crate::database::GameDatabase>,
    mut inputs: Query<&mut TextInputField>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let mut rng = rand::rng();
        let name = RANDOM_NAMES[rng.random_range(0..RANDOM_NAMES.len())].to_string();
        config.player_name = name.clone();
        profile.name = name.clone();
        db.update_profile_name(&profile.id, &name);

        for mut field in &mut inputs {
            field.value = name.clone();
            field.cursor_pos = name.len();
            field.selection_anchor = None;
        }
    }
}

// ── Menu Keyboard Navigation ──

pub(crate) fn menu_keyboard_nav(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut nav: ResMut<MenuNavFocus>,
    mut click_events: MessageWriter<UiClickEvent>,
    focusables: Query<(Entity, &NavFocusable)>,
    mut commands: Commands,
    focused_q: Query<Entity, With<NavFocused>>,
    text_focus: Query<&TextInputFocused>,
    menu_btns: Query<&MenuButton>,
    mut page: ResMut<MenuPage>,
    host_state: Option<Res<HostNetState>>,
    client_state: Option<Res<ClientNetState>>,
) {
    // Don't navigate if a text input is focused
    if text_focus.iter().next().is_some() {
        return;
    }

    // Escape → go back
    if keyboard.just_pressed(KeyCode::Escape) {
        let new_page = match *page {
            MenuPage::NewGame | MenuPage::Options | MenuPage::Multiplayer => Some(MenuPage::Title),
            MenuPage::HostLobby => {
                #[cfg(not(target_arch = "wasm32"))]
                multiplayer::stop_hosting(&mut commands, &host_state);
                Some(MenuPage::Multiplayer)
            }
            MenuPage::JoinLobby => {
                multiplayer::stop_client(&mut commands, &client_state);
                Some(MenuPage::Multiplayer)
            }
            _ => None,
        };
        if let Some(p) = new_page {
            *page = p;
            return;
        }
    }

    let mut items: Vec<(Entity, usize)> = focusables.iter().map(|(e, nf)| (e, nf.0)).collect();
    if items.is_empty() {
        return;
    }
    items.sort_by_key(|&(_, order)| order);
    let count = items.len();

    let up = keyboard.just_pressed(KeyCode::ArrowUp) || keyboard.just_pressed(KeyCode::KeyW);
    let down = keyboard.just_pressed(KeyCode::ArrowDown) || keyboard.just_pressed(KeyCode::KeyS);
    let confirm =
        keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter);

    if up {
        nav.index = if nav.index == 0 {
            count - 1
        } else {
            nav.index - 1
        };
    }
    if down {
        nav.index = (nav.index + 1) % count;
    }

    // Clamp in case buttons changed
    nav.index = nav.index.min(count - 1);

    // Update NavFocused marker
    if up || down {
        for e in &focused_q {
            commands.entity(e).remove::<NavFocused>();
        }
        let (entity, _) = items[nav.index];
        commands.entity(entity).insert(NavFocused);
    }

    // Ensure focus marker exists even without input (first frame)
    if focused_q.is_empty() {
        let (entity, _) = items[nav.index];
        commands.entity(entity).insert(NavFocused);
    }

    // Enter on an action button (MenuButton) → emit click
    if confirm {
        let (entity, _) = items[nav.index];
        if menu_btns.get(entity).is_ok() {
            click_events.write(UiClickEvent { entity });
        }
    }
}

/// Handles Left/Right (A/D) to change the selected option within a focused selector row.
pub(crate) fn menu_selector_keyboard_nav(
    keyboard: Res<ButtonInput<KeyCode>>,
    focused: Query<(Entity, &Children), With<NavFocused>>,
    selectors: Query<(Entity, &MenuSelector, Option<&SelectedOption>)>,
    sliders: Query<&RangeSlider>,
    mut click_events: MessageWriter<UiClickEvent>,
    mut graphics: ResMut<GraphicsSettings>,
    mut audio_settings: ResMut<crate::audio::AudioSettings>,
    menu_btns: Query<&MenuButton>,
    text_focus: Query<&TextInputFocused>,
) {
    if text_focus.iter().next().is_some() {
        return;
    }

    let left = keyboard.just_pressed(KeyCode::ArrowLeft) || keyboard.just_pressed(KeyCode::KeyA);
    let right = keyboard.just_pressed(KeyCode::ArrowRight) || keyboard.just_pressed(KeyCode::KeyD);
    let confirm =
        keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter);

    if !left && !right && !confirm {
        return;
    }

    for (entity, children) in &focused {
        // Skip action buttons — those are handled by menu_keyboard_nav
        if menu_btns.get(entity).is_ok() {
            continue;
        }

        // Collect selector children of this row
        let mut child_selectors: Vec<(Entity, usize, bool)> = Vec::new();
        for child in children.iter() {
            if let Ok((e, sel, selected)) = selectors.get(child) {
                child_selectors.push((e, sel.index, selected.is_some()));
            }
        }
        child_selectors.sort_by_key(|&(_, idx, _)| idx);

        if child_selectors.is_empty() {
            let mut handled_slider = false;
            for child in children.iter() {
                let Ok(slider) = sliders.get(child) else {
                    continue;
                };

                match slider.field {
                    SelectorField::Resolution => {
                        let current_index = super::options::resolution_index(graphics.resolution);
                        let new_index = if left {
                            super::options::step_resolution_index(current_index, -1)
                        } else {
                            super::options::step_resolution_index(current_index, 1)
                        };
                        if new_index != current_index {
                            super::options::apply_selector_change(
                                &SelectorField::Resolution,
                                new_index,
                                &mut graphics,
                            );
                        }
                        handled_slider = true;
                    }
                    SelectorField::MusicVolume => {
                        let delta = if left { -0.01 } else { 0.01 };
                        audio_settings.music_volume =
                            (audio_settings.music_volume + delta).clamp(0.0, 1.0);
                        handled_slider = true;
                    }
                    SelectorField::SfxVolume => {
                        let delta = if left { -0.01 } else { 0.01 };
                        audio_settings.sfx_volume =
                            (audio_settings.sfx_volume + delta).clamp(0.0, 1.0);
                        handled_slider = true;
                    }
                    _ => {}
                }
            }

            if handled_slider {
                continue;
            }

            continue;
        }

        let current = child_selectors
            .iter()
            .position(|&(_, _, sel)| sel)
            .unwrap_or(0);

        let new = if left {
            if current == 0 {
                child_selectors.len() - 1
            } else {
                current - 1
            }
        } else if right || confirm {
            (current + 1) % child_selectors.len()
        } else {
            continue;
        };

        if new != current {
            let (target_entity, _, _) = child_selectors[new];
            click_events.write(UiClickEvent {
                entity: target_entity,
            });
        }
    }
}

/// Applies visual highlight to the keyboard-focused item (button or selector row).
pub(crate) fn menu_nav_focus_visuals(
    focused: Query<Entity, Added<NavFocused>>,
    all_focusable: Query<(Entity, Option<&MenuButton>), With<NavFocusable>>,
    nav_focused_q: Query<(Entity, Option<&MenuButton>), With<NavFocused>>,
    mut commands: Commands,
    children_q: Query<&Children>,
    mut text_colors: Query<&mut TextColor>,
    theme: Res<Theme>,
) {
    if focused.is_empty() {
        return;
    }

    // Style newly focused items
    for entity in &focused {
        commands.entity(entity).insert((
            BorderColor::all(theme.colors.accent),
            BoxShadow::new(
                Color::srgba(0.29, 0.62, 1.0, 0.4),
                Val::Px(0.0),
                Val::Px(0.0),
                Val::Px(0.0),
                Val::Px(10.0),
            ),
        ));
        if let Ok(children) = children_q.get(entity) {
            for child in children.iter() {
                if let Ok(mut tc) = text_colors.get_mut(child) {
                    tc.0 = Color::WHITE;
                }
            }
        }
    }

    // Reset unfocused items
    for (entity, is_btn) in &all_focusable {
        if nav_focused_q.iter().any(|(e, _)| e == entity) {
            continue;
        }
        // Selector rows: reset left border
        if is_btn.is_none() {
            commands
                .entity(entity)
                .insert(BorderColor::all(Color::NONE));
        } else {
            commands
                .entity(entity)
                .insert(BorderColor::all(Color::NONE));
        }
        commands.entity(entity).remove::<BoxShadow>();
    }
}

/// Resets nav focus index when the menu page changes.
pub(crate) fn reset_nav_focus_on_page_change(
    page: Res<MenuPage>,
    mut nav: ResMut<MenuNavFocus>,
    focused_q: Query<Entity, With<NavFocused>>,
    mut commands: Commands,
) {
    if page.is_changed() {
        nav.index = 0;
        for e in &focused_q {
            commands.entity(e).remove::<NavFocused>();
        }
    }
}

// ── Volume Slider Interaction ──

/// Handles click and drag on volume slider tracks.
///
/// Uses `RelativeCursorPosition` to map cursor to 0.0–1.0 within the track.
pub(crate) fn volume_slider_system(
    mouse: Res<ButtonInput<MouseButton>>,
    sliders: Query<(Entity, &RangeSlider, &Interaction, &RelativeCursorPosition)>,
    mut fills: Query<(&ChildOf, &mut Node), With<RangeSliderFill>>,
    mut labels: Query<(&RangeSliderLabel, &mut Text)>,
    mut graphics: ResMut<GraphicsSettings>,
    mut audio_settings: ResMut<crate::audio::AudioSettings>,
    mut drag: ResMut<SliderDragState>,
) {
    // On release, stop dragging.
    if mouse.just_released(MouseButton::Left) {
        if drag.active.is_some() {
            drag.active = None;
        }
        return;
    }

    // Determine which slider is active.
    let active_slider = if let Some(active) = drag.active {
        if mouse.pressed(MouseButton::Left) {
            Some(active)
        } else {
            None
        }
    } else if mouse.just_pressed(MouseButton::Left) {
        sliders
            .iter()
            .find(|(_, _, interaction, _)| **interaction == Interaction::Pressed)
            .map(|(entity, _, _, _)| entity)
    } else {
        None
    };

    let Some(slider_entity) = active_slider else {
        return;
    };
    drag.active = Some(slider_entity);

    let Ok((_, slider, _, rel_cursor)) = sliders.get(slider_entity) else {
        return;
    };

    // RelativeCursorPosition: (0,0) = center, (-0.5,-0.5) = top-left, (0.5,0.5) = bottom-right.
    // Convert to 0.0–1.0 range: add 0.5 to the x component.
    let Some(normalized) = rel_cursor.normalized else {
        return;
    };
    let t = (normalized.x + 0.5).clamp(0.0, 1.0);

    let (pct, value_label) = match slider.field {
        SelectorField::Resolution => {
            let steps = slider
                .steps
                .unwrap_or(super::options::RESOLUTION_OPTIONS.len());
            if steps == 0 {
                return;
            }
            let max_index = steps.saturating_sub(1) as f32;
            let index = (t * max_index).round() as usize;
            super::options::apply_selector_change(&SelectorField::Resolution, index, &mut graphics);
            let (w, h) = graphics.resolution;
            let pct = super::options::resolution_slider_value(super::options::resolution_index(
                graphics.resolution,
            )) * 100.0;
            (pct, format!("{w}x{h}"))
        }
        SelectorField::MusicVolume => {
            let value = (t * 100.0).round() / 100.0;
            audio_settings.music_volume = value;
            let pct = value * 100.0;
            (pct, format!("{pct:.0}%"))
        }
        SelectorField::SfxVolume => {
            let value = (t * 100.0).round() / 100.0;
            audio_settings.sfx_volume = value;
            let pct = value * 100.0;
            (pct, format!("{pct:.0}%"))
        }
        _ => return,
    };

    // Update fill bar width and label.
    for (parent, mut node) in fills.iter_mut() {
        if parent.parent() == slider_entity {
            node.width = Val::Percent(pct);
        }
    }
    let field = slider.field;
    for (lbl, mut text) in labels.iter_mut() {
        if lbl.0 == field {
            **text = value_label.clone();
        }
    }
}

pub(crate) fn sync_range_slider_visuals(
    graphics: Res<GraphicsSettings>,
    audio_settings: Res<crate::audio::AudioSettings>,
    sliders: Query<(Entity, &RangeSlider)>,
    mut fills: Query<(&ChildOf, &mut Node), With<RangeSliderFill>>,
    mut labels: Query<(&RangeSliderLabel, &mut Text)>,
) {
    if !graphics.is_changed() && !audio_settings.is_changed() {
        return;
    }

    for (slider_entity, slider) in &sliders {
        let (pct, value_label) = match slider.field {
            SelectorField::Resolution => {
                let index = super::options::resolution_index(graphics.resolution);
                let pct = super::options::resolution_slider_value(index) * 100.0;
                let (w, h) = graphics.resolution;
                (pct, format!("{w}x{h}"))
            }
            SelectorField::MusicVolume => {
                let pct = (audio_settings.music_volume * 100.0).round();
                (pct, format!("{pct:.0}%"))
            }
            SelectorField::SfxVolume => {
                let pct = (audio_settings.sfx_volume * 100.0).round();
                (pct, format!("{pct:.0}%"))
            }
            _ => continue,
        };

        for (parent, mut node) in fills.iter_mut() {
            if parent.parent() == slider_entity {
                node.width = Val::Percent(pct);
            }
        }

        for (label, mut text) in labels.iter_mut() {
            if label.0 == slider.field {
                **text = value_label.clone();
            }
        }
    }
}
