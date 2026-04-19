use bevy::prelude::*;
use bevy::window::Monitor;
use bevy::window::PrimaryMonitor;

use super::helpers::*;
use crate::infrastructure::database::{ActiveProfile, GameDatabase};
use crate::types::*;
use crate::ui::fonts::UiFonts;
use crate::ui::theme::{Theme, BG_ELEVATED, TEAM_COLORS, TEXT_PRIMARY};

use super::multiplayer;
use super::*;
use crate::infrastructure::multiplayer::{ClientNetState, LobbyState, NetRole};

// ── Spawn / Cleanup ──

pub(crate) fn spawn_menu(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    page: Res<MenuPage>,
    config: Res<GameSetupConfig>,
    graphics: Res<GraphicsSettings>,
    audio_settings: Res<crate::infrastructure::audio::AudioSettings>,
    resolutions: Res<AvailableResolutions>,
    fonts: Res<UiFonts>,
    early_exit: (
        Option<Res<RestartRequested>>,
        Option<Res<crate::infrastructure::save_load::PendingLoad>>,
        ResMut<NextState<AppState>>,
    ),
    lobby: Res<LobbyState>,
    net_role: Option<Res<NetRole>>,
    client_state: Option<Res<ClientNetState>>,
    theme: Res<Theme>,
    db: Res<GameDatabase>,
    profile: Res<ActiveProfile>,
) {
    let (restart, pending_load, mut next_state) = early_exit;

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

    // 3D camera for the scroll model
    commands.spawn((
        MenuCamera,
        DespawnOnExit(AppState::MainMenu),
        Camera3d::default(),
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
            BackgroundColor(Color::NONE),
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
        root,
        &page,
        &config,
        &graphics,
        &audio_settings,
        &resolutions,
        &fonts,
        &lobby,
        &net_role,
        &client_state,
        &theme,
        &db,
        &profile,
        &asset_server,
    );
}

fn dispatch_page(
    commands: &mut Commands,
    container: Entity,
    root: Entity,
    page: &MenuPage,
    config: &GameSetupConfig,
    graphics: &GraphicsSettings,
    audio_settings: &crate::infrastructure::audio::AudioSettings,
    resolutions: &AvailableResolutions,
    fonts: &UiFonts,
    lobby: &LobbyState,
    net_role: &Option<Res<NetRole>>,
    client_state: &Option<Res<ClientNetState>>,
    theme: &Theme,
    db: &GameDatabase,
    profile: &ActiveProfile,
    asset_server: &AssetServer,
) {
    match *page {
        MenuPage::Title => super::title::spawn_title_page(commands, container, root, fonts, theme),
        MenuPage::NewGame => {
            super::new_game::spawn_new_game_page(commands, container, config, fonts, theme)
        }
        MenuPage::Options => pages::spawn_options_page(
            commands,
            container,
            graphics,
            audio_settings,
            resolutions,
            fonts,
            theme,
        ),
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
        MenuPage::LoadGame => pages::spawn_load_game_page(
            commands,
            container,
            fonts,
            theme,
            db,
            profile,
            asset_server,
        ),
    }
}

// ── Page Transition ──

pub(crate) fn refresh_menu_page(
    mut commands: Commands,
    content_roots: Query<(Entity, Option<&Children>), With<MenuContentRoot>>,
    menu_roots: Query<Entity, With<MenuRoot>>,
    status_bars: Query<Entity, With<MenuStatusBar>>,
    page: Res<MenuPage>,
    config: Res<GameSetupConfig>,
    graphics: Res<GraphicsSettings>,
    audio_settings: Res<crate::infrastructure::audio::AudioSettings>,
    resolutions: Res<AvailableResolutions>,
    fonts: Res<UiFonts>,
    lobby: Res<LobbyState>,
    net_role: Option<Res<NetRole>>,
    client_state: Option<Res<ClientNetState>>,
    theme: Res<Theme>,
    extras: (Res<GameDatabase>, Res<ActiveProfile>, Res<AssetServer>),
) {
    let (db, profile, asset_server) = extras;

    if !page.is_changed() {
        return;
    }

    let Ok((content_root, children)) = content_roots.single() else {
        return;
    };

    // Despawn old status bars (they live under MenuRoot, not MenuContentRoot)
    for bar in status_bars.iter() {
        commands.entity(bar).try_despawn();
    }

    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }

    let Ok(root) = menu_roots.single() else {
        return;
    };

    dispatch_page(
        &mut commands,
        content_root,
        root,
        &page,
        &config,
        &graphics,
        &audio_settings,
        &resolutions,
        &fonts,
        &lobby,
        &net_role,
        &client_state,
        &theme,
        &db,
        &profile,
        &asset_server,
    );
}

/// Rebuild menu content when `MenuDirty` is inserted (without requiring a page change).
pub(crate) fn rebuild_dirty_menu(
    dirty: Option<Res<MenuDirty>>,
    mut commands: Commands,
    content_roots: Query<(Entity, Option<&Children>), With<MenuContentRoot>>,
    menu_roots: Query<Entity, With<MenuRoot>>,
    status_bars: Query<Entity, With<MenuStatusBar>>,
    page: Res<MenuPage>,
    config: Res<GameSetupConfig>,
    graphics: Res<GraphicsSettings>,
    audio_settings: Res<crate::infrastructure::audio::AudioSettings>,
    resolutions: Res<AvailableResolutions>,
    fonts: Res<UiFonts>,
    lobby: Res<LobbyState>,
    net_role: Option<Res<NetRole>>,
    client_state: Option<Res<ClientNetState>>,
    theme: Res<Theme>,
    extras: (Res<GameDatabase>, Res<ActiveProfile>, Res<AssetServer>),
) {
    let (db, profile, asset_server) = extras;

    if dirty.is_none() {
        return;
    }
    commands.remove_resource::<MenuDirty>();

    let Ok((content_root, children)) = content_roots.single() else {
        return;
    };

    // Despawn old status bars
    for bar in status_bars.iter() {
        commands.entity(bar).try_despawn();
    }

    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).try_despawn();
        }
    }

    let Ok(root) = menu_roots.single() else {
        return;
    };

    dispatch_page(
        &mut commands,
        content_root,
        root,
        &page,
        &config,
        &graphics,
        &audio_settings,
        &resolutions,
        &fonts,
        &lobby,
        &net_role,
        &client_state,
        &theme,
        &db,
        &profile,
        &asset_server,
    );
}

// ��─ Menu Button Handler ──

pub(crate) fn apply_window_settings_on_menu_enter(
    graphics: Res<GraphicsSettings>,
    mut windows: Query<&mut Window>,
    monitors: Query<&Monitor, With<PrimaryMonitor>>,
) {
    if let Ok(mut window) = windows.single_mut() {
        super::options::apply_graphics_settings_capped(
            &graphics,
            &mut window,
            monitors.single().ok(),
        );
    }
}

// ── Selector Visuals ──

pub(crate) fn update_selector_visuals(
    config: Res<GameSetupConfig>,
    graphics: Res<GraphicsSettings>,
    resolutions: Res<AvailableResolutions>,
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
        // Arrow selectors (Resolution) have their own visual handling via
        // sync_resolution_arrow_selector — skip them to avoid overriding ghost styling.
        if selector.field == SelectorField::Resolution {
            continue;
        }

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
                        MapSize::ExtraLarge => 3,
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
            SelectorField::Resolution => unreachable!("skipped above"),
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
                        crate::ui::theme::ThemeMode::Dark => 0,
                        crate::ui::theme::ThemeMode::Light => 1,
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
            let color = TEAM_COLORS
                .get(selector.index)
                .copied()
                .unwrap_or(TEAM_COLORS[0]);
            let new_bg = if should_be_selected {
                color
            } else {
                BG_ELEVATED.with_alpha(0.8)
            };
            *bg = BackgroundColor(new_bg);
            commands
                .entity(entity)
                .insert(BorderColor::all(if should_be_selected {
                    TEXT_PRIMARY
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
                            TEXT_PRIMARY
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
            Color::srgb(0.10, 0.16, 0.18)
        } else {
            theme.colors.text_secondary
        };

        let selection_changed = should_be_selected != was_selected.is_some();

        // Update background via ButtonAnimState to avoid fighting with
        // animated_button_hover_system (which also writes BackgroundColor each frame).
        if let Some(mut anim) = anim_state {
            if should_be_selected {
                // Selected: override the animated hover system's target with accent color.
                let bg_arr = new_bg.to_srgba().to_f32_array();
                anim.bg_target = bg_arr;
                if selection_changed {
                    // Snap immediately on selection change for responsive feel.
                    anim.bg_current = bg_arr;
                }
            } else if selection_changed {
                // Newly deselected: immediately reset bg so the accent doesn't
                // linger for a frame while deferred SelectedOption removal is pending.
                let bg_arr = new_bg.to_srgba().to_f32_array();
                anim.bg_target = bg_arr;
                anim.bg_current = bg_arr;
            }
            // Steady unselected: don't touch bg_target — let animated_button_hover_system
            // handle idle/hover/pressed states naturally.
        } else {
            // No ButtonAnimState — write BackgroundColor directly (fallback).
            *bg = BackgroundColor(new_bg);
        }

        if should_be_selected {
            commands
                .entity(entity)
                .insert((menu_focus_border(&theme), menu_focus_shadow()));
        } else {
            commands
                .entity(entity)
                .insert(BorderColor::all(Color::NONE));
            commands.entity(entity).remove::<BoxShadow>();
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

/// Track the last known player name so we only rebuild slot cards when it actually changes.
#[derive(Resource, Default)]
pub(crate) struct LastKnownPlayerName(pub String);

/// Rebuild slot cards when `GameSetupConfig.player_name` changes (e.g. text input edits).
pub(crate) fn sync_slot_cards_to_config(
    config: Res<GameSetupConfig>,
    mut last_name: ResMut<LastKnownPlayerName>,
    mut commands: Commands,
    slots_container: Query<(Entity, &Children), With<SlotCardsContainer>>,
    page: Res<MenuPage>,
    lobby: Option<Res<crate::infrastructure::multiplayer::LobbyState>>,
    theme: Res<Theme>,
) {
    if config.player_name == last_name.0 {
        return;
    }
    last_name.0 = config.player_name.clone();
    let is_multiplayer = matches!(*page, MenuPage::HostLobby | MenuPage::JoinLobby);
    super::input::rebuild_slot_cards(
        &mut commands,
        &slots_container,
        &config,
        is_multiplayer,
        lobby.as_ref().map(|l| l.players.as_slice()),
        &theme,
    );
}

// ── Randomize Seed ──

pub(crate) fn randomize_seed_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<RandomizeSeedButton>)>,
    mut config: ResMut<GameSetupConfig>,
    mut seed_inputs: Query<&mut TextInputField, With<SeedInput>>,
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

    if config.map_seed == 0 {
        config.map_seed = rand::random::<u64>();
    } else {
        config.map_seed = 0;
    }

    let seed_text = if config.map_seed == 0 {
        String::new()
    } else {
        format!("{}", config.map_seed)
    };

    for mut field in &mut seed_inputs {
        field.value = seed_text.clone();
        field.cursor_pos = seed_text.len();
        field.selection_anchor = None;
    }
}

// ── Random Name ──

pub(crate) fn random_name_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<RandomNameButton>)>,
    mut config: ResMut<GameSetupConfig>,
    mut profile: ResMut<crate::infrastructure::database::ActiveProfile>,
    db: Res<crate::infrastructure::database::GameDatabase>,
    mut inputs: Query<&mut TextInputField>,
    mut commands: Commands,
    role: Option<Res<crate::infrastructure::multiplayer::NetRole>>,
    mut socket: Option<ResMut<bevy_matchbox::prelude::MatchboxSocket>>,
    mut lobby: Option<ResMut<crate::infrastructure::multiplayer::LobbyState>>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let name = crate::types::random_commander_name();
        config.player_name = name.clone();
        profile.name = name.clone();
        db.update_profile_name(&profile.id, &name);

        for mut field in &mut inputs {
            field.value = name.clone();
            field.cursor_pos = name.len();
            field.selection_anchor = None;
        }

        // Sync name to multiplayer lobby if connected
        if let Some(ref role) = role {
            use crate::infrastructure::multiplayer::NetRole;
            match **role {
                NetRole::Client => {
                    if let Some(ref mut socket) = socket {
                        let msg = game_state::message::ClientMessage::NameUpdate {
                            seq: 0,
                            timestamp: 0.0,
                            player_name: name,
                        };
                        crate::infrastructure::multiplayer::matchbox_transport::send_to_host(
                            socket, &msg,
                        );
                    }
                }
                NetRole::Host => {
                    if let Some(ref mut lobby) = lobby {
                        if let Some(host_player) = lobby.players.iter_mut().find(|p| p.is_host) {
                            host_player.name = name;
                        }
                        commands.insert_resource(super::multiplayer::PendingLobbyBroadcast);
                    }
                }
                NetRole::Offline => {}
            }
        }
    }
}
