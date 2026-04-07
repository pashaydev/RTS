use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::prelude::*;

use super::helpers::*;
use super::multiplayer;
#[cfg(not(target_arch = "wasm32"))]
use super::multiplayer::start_hosting;
use super::*;
use crate::types::*;
use crate::infrastructure::database::{ActiveProfile, GameDatabase};
use crate::infrastructure::multiplayer::{ClientNetState, HostNetState, LobbyState};
use crate::ui::theme::Theme;
use crate::ui::core::interactions::UiClickEvent;

// ── Button Clicks ──

pub(crate) fn handle_menu_buttons(
    mut click_events: MessageReader<UiClickEvent>,
    buttons: Query<&MenuButton>,
    mut next_state: ResMut<NextState<AppState>>,
    mut page: ResMut<MenuPage>,
    mut config: ResMut<GameSetupConfig>,
    mut graphics: ResMut<GraphicsSettings>,
    mut audio_settings: ResMut<crate::infrastructure::audio::AudioSettings>,
    mut theme: ResMut<crate::ui::theme::Theme>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
    mut windows: Query<&mut Window>,
    host_state: Option<Res<HostNetState>>,
    client_state: Option<Res<ClientNetState>>,
    db_and_profile: (Res<GameDatabase>, Res<ActiveProfile>),
    options_state: (
        ResMut<crate::ui::core::framework::WidgetRegistry>,
        Option<Res<super::OptionsSnapshot>>,
        ResMut<super::ConfirmPopupState>,
    ),
) {
    let (db, profile) = db_and_profile;
    let (mut widget_registry, snapshot, mut popup_state) = options_state;
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
                // Update snapshot so Save button hides
                commands.insert_resource(super::OptionsSnapshot {
                    graphics: graphics.clone(),
                    audio: audio_settings.clone(),
                });
            }
            MenuAction::SaveAndLeave => {
                theme.set_mode(graphics.theme_mode);
                if let Ok(mut window) = windows.single_mut() {
                    super::options::apply_graphics_settings(&graphics, &mut window);
                }
                popup_state.active = false;
                commands.remove_resource::<super::OptionsSnapshot>();
                *page = MenuPage::Title;
            }
            MenuAction::DiscardSettings => {
                // Revert settings to snapshot
                if let Some(ref snap) = snapshot {
                    *graphics = snap.graphics.clone();
                    *audio_settings = snap.audio.clone();
                    theme.set_mode(snap.graphics.theme_mode);
                }
                popup_state.active = false;
                commands.remove_resource::<super::OptionsSnapshot>();
                *page = MenuPage::Title;
            }
            MenuAction::CancelPopup => {
                popup_state.active = false;
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
                    match rmp_serde::from_slice::<crate::infrastructure::save_load::SaveData>(&blob) {
                        Ok(save_data) => {
                            info!("Loading save id={save_id}");
                            crate::infrastructure::save_load::restore_config_from_save(&mut config, &save_data);
                            commands.insert_resource(crate::infrastructure::save_load::PendingLoad { save_data });
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

// ── Selector Clicks ──

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
            super::new_game::spawn_slot_card(commands, container, i, config, is_multiplayer, theme);
        }
    }
}

pub(crate) fn handle_selector_clicks(
    interactions: Query<(&Interaction, &MenuSelector), Changed<Interaction>>,
    mut click_events: MessageReader<UiClickEvent>,
    all_selectors: Query<&MenuSelector>,
    mut config: ResMut<GameSetupConfig>,
    mut graphics: ResMut<GraphicsSettings>,
    resolutions: Res<AvailableResolutions>,
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
        // Ignore resolution changes while fullscreen is active
        if selector.field == SelectorField::Resolution && graphics.fullscreen {
            continue;
        }
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

                        rebuild_slot_cards(
                            &mut commands,
                            &slots_container,
                            &config,
                            *page == MenuPage::HostLobby,
                            &theme,
                        );
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

                    rebuild_slot_cards(
                        &mut commands,
                        &slots_container,
                        &config,
                        *page == MenuPage::HostLobby,
                        &theme,
                    );
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
                    &resolutions,
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
