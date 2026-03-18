use bevy::ecs::message::MessageWriter;
use bevy::prelude::*;
use rand::Rng;

use crate::components::*;
use crate::theme;
use crate::ui::fonts::UiFonts;
use crate::ui::menu_helpers::*;

use super::*;
use crate::multiplayer::{ClientNetState, HostNetState};
#[cfg(not(target_arch = "wasm32"))]
use super::multiplayer::start_hosting;

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
            multiplayer::spawn_host_lobby_page(commands, container, fonts)
        }
        MenuPage::JoinLobby => {
            multiplayer::spawn_join_lobby_page(commands, container, fonts)
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
                    start_hosting(&mut commands);
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
) {
    for (interaction, selector) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }

        match selector.field {
            SelectorField::PlayerColor => {
                config.player_color_index = selector.index;
            }
            SelectorField::AiCount => {
                config.num_ai_opponents = (selector.index + 1) as u8;
            }
            SelectorField::AiDifficulty(ai_idx) => {
                if ai_idx < 3 {
                    config.ai_difficulties[ai_idx] = match selector.index {
                        0 => AiDifficulty::Easy,
                        1 => AiDifficulty::Medium,
                        _ => AiDifficulty::Hard,
                    };
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
        let should_be_selected = match selector.field {
            SelectorField::PlayerColor => selector.index == config.player_color_index,
            SelectorField::AiCount => selector.index == (config.num_ai_opponents - 1) as usize,
            SelectorField::AiDifficulty(ai_idx) => {
                let diff_idx = match config.ai_difficulties[ai_idx] {
                    AiDifficulty::Easy => 0,
                    AiDifficulty::Medium => 1,
                    AiDifficulty::Hard => 2,
                };
                selector.index == diff_idx
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
            SelectorField::MapSeed => false,
        };

        // Color picker dots
        if selector.field == SelectorField::PlayerColor {
            let border = if should_be_selected {
                Color::WHITE
            } else {
                Color::NONE
            };
            commands.entity(entity).insert(BorderColor::all(border));
            if should_be_selected {
                let color = match selector.index {
                    0 => Faction::Player1.color(),
                    1 => Faction::Player2.color(),
                    2 => Faction::Player3.color(),
                    _ => Faction::Player4.color(),
                };
                let srgba = color.to_srgba();
                commands.entity(entity).insert((
                    BoxShadow::new(
                        Color::srgba(srgba.red, srgba.green, srgba.blue, 0.5),
                        Val::Px(0.0),
                        Val::Px(0.0),
                        Val::Px(0.0),
                        Val::Px(8.0),
                    ),
                    UiGlowPulse {
                        color,
                        intensity: 0.8,
                    },
                ));
            } else {
                commands.entity(entity).remove::<BoxShadow>();
                commands.entity(entity).remove::<UiGlowPulse>();
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

// ── AI Card Visibility ──

pub(crate) fn update_ai_card_visibility(
    config: Res<GameSetupConfig>,
    mut cards: Query<(&AiCardContainer, &mut Node)>,
) {
    for (card, mut node) in &mut cards {
        let visible = card.0 < config.num_ai_opponents as usize;
        node.display = if visible {
            Display::Flex
        } else {
            Display::None
        };
    }
}

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

// ── Ally Toggle ──

pub(crate) fn ally_toggle_system(
    interactions: Query<(&Interaction, &AllyToggleButton), Changed<Interaction>>,
    mut config: ResMut<GameSetupConfig>,
) {
    for (interaction, toggle) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let faction_idx = toggle.ai_index + 1;
        let player_team = config.player_teams[0];

        if config.player_teams[faction_idx] == player_team {
            let mut next_team = 0u8;
            loop {
                if !config.player_teams.contains(&next_team) || next_team > 10 {
                    break;
                }
                next_team += 1;
            }
            config.player_teams[faction_idx] = next_team;
        } else {
            config.player_teams[faction_idx] = player_team;
        }

        config.team_mode = TeamMode::Custom;
    }
}

pub(crate) fn update_ally_toggle_visuals(
    config: Res<GameSetupConfig>,
    mut toggles: Query<(
        &AllyToggleButton,
        &mut BackgroundColor,
        &mut ButtonAnimState,
        &Children,
    )>,
    mut text_colors: Query<(&mut TextColor, &mut Text), Without<AllyToggleButton>>,
) {
    for (toggle, mut bg, mut anim, children) in &mut toggles {
        let faction_idx = toggle.ai_index + 1;
        let is_ally = config.player_teams[faction_idx] == config.player_teams[0];
        let (color, label) = if is_ally {
            (theme::SUCCESS, "ALLY")
        } else {
            (theme::DESTRUCTIVE, "ENEMY")
        };

        *bg = BackgroundColor(color);
        anim.bg_current = color.to_srgba().to_f32_array();

        for child in children.iter() {
            if let Ok((mut tc, mut text)) = text_colors.get_mut(child) {
                tc.0 = Color::WHITE;
                **text = label.to_string();
            }
        }
    }
}
