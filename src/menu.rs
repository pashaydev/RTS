use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use game_state::message::ClientMessage;
use rand::Rng;
use std::sync::atomic::Ordering;

use crate::components::*;
use crate::multiplayer::{
    self, ClientNetState, HostNetState, LobbyPlayer, LobbyState, LobbyStatus, NetRole, debug_tap,
};
use crate::theme;
use crate::ui::fonts::{self, UiFonts};

// ── Resources & Components ──

#[derive(Resource, Default, PartialEq, Eq)]
enum MenuPage {
    #[default]
    Title,
    NewGame,
    Options,
    Multiplayer,
    HostLobby,
    JoinLobby,
}

#[derive(Component)]
struct MenuRoot;

#[derive(Component)]
struct MenuCamera;

#[derive(Component)]
struct MenuPageContainer;

#[derive(Component)]
struct MenuButton(MenuAction);

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuAction {
    NewGame,
    Options,
    Quit,
    Back,
    StartGame,
    ApplySettings,
    Multiplayer,
    HostGame,
    JoinGame,
    ConnectToHost,
    StartMultiplayer,
    BackToMultiplayer,
    CopySessionCode,
    CancelHost,
    Disconnect,
}

#[derive(Component)]
struct MenuSelector {
    field: SelectorField,
    index: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum SelectorField {
    AiCount,
    AiDifficulty(usize),
    TeamMode,
    MapSize,
    ResourceDensity,
    DayCycle,
    StartingRes,
    MapSeed,
    Resolution,
    Fullscreen,
    Shadows,
    EntityLights,
    UiScale,
    PlayerColor,
}

#[derive(Component)]
struct SelectedOption;

#[derive(Component)]
struct AiCardContainer(usize);

#[derive(Component)]
struct SeedDisplay;

#[derive(Component)]
struct RandomizeSeedButton;

#[derive(Component)]
struct LobbyStatusText;

#[derive(Component)]
struct LobbyPlayerSlot(usize);

#[derive(Component)]
struct SessionCodeText;

#[derive(Component)]
struct SessionCodeInput;

#[derive(Component)]
struct CopyCodeButton;

#[derive(Component)]
struct CopyCodeLabel;

// ── Plugin ──

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuPage>()
            .add_systems(
                OnEnter(AppState::MainMenu),
                cleanup_network_on_enter_menu.before(spawn_menu),
            )
            .add_systems(OnEnter(AppState::MainMenu), spawn_menu)
            .add_systems(OnExit(AppState::MainMenu), cleanup_menu)
            .add_systems(
                Update,
                (handle_menu_buttons, handle_selector_clicks).run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                Update,
                (
                    update_selector_visuals,
                    update_ai_card_visibility,
                    page_transition_system,
                )
                    .run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                Update,
                (
                    text_input_system,
                    text_input_cursor_blink,
                    random_name_system,
                )
                    .run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                Update,
                (
                    ally_toggle_system,
                    update_ally_toggle_visuals,
                    menu_scroll_system,
                    randomize_seed_system,
                )
                    .run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(
                Update,
                (update_lobby_ui, connect_to_host_system, copy_session_code_system)
                    .run_if(in_state(AppState::MainMenu)),
            );
    }
}

fn cleanup_network_on_enter_menu(
    mut commands: Commands,
    host_state: Option<Res<HostNetState>>,
    client_state: Option<Res<ClientNetState>>,
    host_factory: Option<Res<HostConnectionFactory>>,
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
        if let Ok(json) = serde_json::to_vec(&leave_msg) {
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
    if let Some(factory) = host_factory {
        factory.shutdown.store(true, Ordering::Relaxed);
    }

    commands.remove_resource::<HostNetState>();
    commands.remove_resource::<ClientNetState>();
    commands.remove_resource::<HostConnectionFactory>();
    commands.remove_resource::<PendingGameStart>();
    commands.insert_resource(NetRole::Offline);
    commands.insert_resource(LobbyState::default());
}

// ── Constants ──

const RANDOM_NAMES: &[&str] = &[
    "Commander",
    "General",
    "Warlord",
    "Captain",
    "Marshal",
    "Overlord",
    "Strategist",
    "Vanguard",
    "Centurion",
    "Paladin",
    "Sentinel",
    "Arbiter",
    "Conqueror",
    "Vindicator",
    "Sovereign",
    "Crusader",
    "Phantom",
    "Templar",
    "Warmaster",
    "Executor",
    "Pathfinder",
    "Nomad",
    "Ironclad",
    "Stormcaller",
];
const DAY_CYCLE_OPTIONS: &[(f32, &str)] = &[(300.0, "5min"), (600.0, "10min"), (1200.0, "20min")];
const STARTING_RES_OPTIONS: &[(f32, &str)] = &[(0.5, "0.5x"), (1.0, "1x"), (2.0, "2x")];
const RESOLUTION_OPTIONS: &[(u32, u32)] = &[(1280, 720), (1920, 1080)];
const UI_SCALE_OPTIONS: &[(f32, &str)] = &[
    (0.75, "75%"),
    (0.85, "85%"),
    (1.0, "100%"),
    (1.15, "115%"),
    (1.25, "125%"),
    (1.5, "150%"),
];


// ── Spawn Menu ──

fn spawn_menu(
    mut commands: Commands,
    page: Res<MenuPage>,
    config: Res<GameSetupConfig>,
    graphics: Res<GraphicsSettings>,
    fonts: Res<UiFonts>,
    restart: Option<Res<RestartRequested>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    // If restart was requested, skip the menu entirely and go back to InGame
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
    match *page {
        MenuPage::Title => spawn_title_page(&mut commands, container, &fonts),
        MenuPage::NewGame => spawn_new_game_page(&mut commands, container, &config, &fonts),
        MenuPage::Options => spawn_options_page(&mut commands, container, &graphics, &fonts),
        MenuPage::Multiplayer => spawn_multiplayer_page(&mut commands, container, &fonts),
        MenuPage::HostLobby => spawn_host_lobby_page(&mut commands, container, &fonts),
        MenuPage::JoinLobby => spawn_join_lobby_page(&mut commands, container, &fonts),
    }
}

fn cleanup_menu(
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


// ── Title Page ──

fn spawn_title_page(commands: &mut Commands, container: Entity, fonts: &UiFonts) {
    // Title with shimmer + scale-in
    let title = commands
        .spawn((
            TitleShimmer { phase_offset: 0.0 },
            Text::new("RTS PROTOTYPE"),
            fonts::heading(fonts, theme::FONT_DISPLAY),
            TextColor(Color::WHITE),
            Node {
                margin: UiRect::bottom(Val::Px(8.0)),
                ..default()
            },
            UiScaleIn {
                from: 0.9,
                timer: Timer::from_seconds(0.5, TimerMode::Once),
                elastic: true,
            },
        ))
        .id();
    commands.entity(container).add_child(title);

    // Subtitle
    let subtitle = commands
        .spawn((
            Text::new("COMMAND YOUR EMPIRE"),
            fonts::body_emphasis(fonts, theme::FONT_BODY),
            TextColor(theme::TEXT_SECONDARY),
            Node {
                margin: UiRect::bottom(Val::Px(16.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(subtitle);

    // Separator line — expands from center
    let sep = commands
        .spawn((
            UiLineExpand {
                target_width: 280.0,
                timer: Timer::from_seconds(0.4, TimerMode::Once),
            },
            Node {
                width: Val::Px(0.0),
                height: Val::Px(1.0),
                margin: UiRect::bottom(Val::Px(28.0)),
                align_self: AlignSelf::Center,
                ..default()
            },
            BackgroundColor(theme::ACCENT),
        ))
        .id();
    commands.entity(container).add_child(sep);

    // Buttons
    for (label, action) in [
        ("NEW GAME", MenuAction::NewGame),
        ("MULTIPLAYER", MenuAction::Multiplayer),
        ("OPTIONS", MenuAction::Options),
        ("QUIT", MenuAction::Quit),
    ] {
        let btn = spawn_menu_button(commands, label, action, false, fonts);
        commands.entity(container).add_child(btn);
    }

    // Version
    let ver = commands
        .spawn((
            Text::new("v0.1"),
            fonts::body(fonts, theme::FONT_BODY),
            TextColor(theme::TEXT_SECONDARY),
            Node {
                margin: UiRect::top(Val::Px(40.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(ver);
}

// ── New Game Page ──

fn spawn_new_game_page(
    commands: &mut Commands,
    container: Entity,
    config: &GameSetupConfig,
    fonts: &UiFonts,
) {
    spawn_page_header(commands, container, "NEW GAME", fonts);

    spawn_animated_section_divider(commands, container, "PLAYER", fonts);

    let name_row = spawn_name_input_row(commands, &config.player_name);
    commands.entity(container).add_child(name_row);

    let color_row = spawn_color_picker(commands, config.player_color_index);
    commands.entity(container).add_child(color_row);

    spawn_animated_section_divider(commands, container, "OPPONENTS", fonts);

    spawn_selector_row(
        commands,
        container,
        "Count:",
        &["1", "2", "3"],
        (config.num_ai_opponents - 1) as usize,
        SelectorField::AiCount,
    );

    for i in 0..3 {
        let visible = i < config.num_ai_opponents as usize;
        spawn_ai_card(commands, container, i, config, visible);
    }

    let team_idx = match config.team_mode {
        TeamMode::FFA => 0,
        TeamMode::Teams => 1,
        TeamMode::Custom => 2,
    };
    spawn_selector_row(
        commands,
        container,
        "Teams:",
        &["FFA", "2v2", "Custom"],
        team_idx,
        SelectorField::TeamMode,
    );

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

    // Seed row
    let seed_text = if config.map_seed == 0 {
        "Random".to_string()
    } else {
        format!("{}", config.map_seed)
    };
    let seed_row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(6.0)),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text::new("Seed:"),
                TextFont {
                    font_size: theme::FONT_MEDIUM,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
                Node {
                    width: Val::Px(120.0),
                    ..default()
                },
            ));
            parent.spawn((
                SeedDisplay,
                Text::new(seed_text),
                TextFont {
                    font_size: theme::FONT_MEDIUM,
                    ..default()
                },
                TextColor(theme::TEXT_PRIMARY),
                Node {
                    width: Val::Px(140.0),
                    ..default()
                },
            ));
            parent
                .spawn((
                    RandomizeSeedButton,
                    Button,
                    ButtonAnimState::new(theme::BTN_PRIMARY.to_srgba().to_f32_array()),
                    ButtonStyle::Filled,
                    Node {
                        padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                        margin: UiRect::horizontal(Val::Px(2.0)),
                        // border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(theme::BTN_PRIMARY),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("Randomize"),
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
    commands.entity(container).add_child(seed_row);

    // Start Game button with glow pulse
    let start_btn = commands
        .spawn((
            MenuButton(MenuAction::StartGame),
            Button,
            ButtonAnimState::new(theme::ACCENT.to_srgba().to_f32_array()),
            ButtonStyle::Filled,
            UiGlowPulse {
                color: theme::ACCENT,
                intensity: 0.6,
            },
            Node {
                width: Val::Px(280.0),
                height: Val::Px(50.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                margin: UiRect {
                    top: Val::Px(20.0),
                    bottom: Val::Px(4.0),
                    ..default()
                },
                // border_radius: BorderRadius::all(Val::Px(6.0)),
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

// ── Options Page ──

fn spawn_options_page(
    commands: &mut Commands,
    container: Entity,
    graphics: &GraphicsSettings,
    fonts: &UiFonts,
) {
    spawn_page_header(commands, container, "OPTIONS", fonts);

    spawn_animated_section_divider(commands, container, "GRAPHICS", fonts);

    let res_idx = RESOLUTION_OPTIONS
        .iter()
        .position(|&r| r == graphics.resolution)
        .unwrap_or(0);
    spawn_selector_row(
        commands,
        container,
        "Resolution:",
        &["1280x720", "1920x1080"],
        res_idx,
        SelectorField::Resolution,
    );

    let fs_idx = if graphics.fullscreen { 0 } else { 1 };
    spawn_selector_row(
        commands,
        container,
        "Fullscreen:",
        &["ON", "OFF"],
        fs_idx,
        SelectorField::Fullscreen,
    );

    let shadow_idx = match graphics.shadow_quality {
        ShadowQuality::Off => 0,
        ShadowQuality::Low => 1,
        ShadowQuality::High => 2,
    };
    spawn_selector_row(
        commands,
        container,
        "Shadows:",
        &["Off", "Low", "High"],
        shadow_idx,
        SelectorField::Shadows,
    );

    let lights_idx = if graphics.entity_lights { 0 } else { 1 };
    spawn_selector_row(
        commands,
        container,
        "Lights:",
        &["ON", "OFF"],
        lights_idx,
        SelectorField::EntityLights,
    );

    let scale_labels: Vec<&str> = UI_SCALE_OPTIONS.iter().map(|&(_, s)| s).collect();
    let scale_idx = UI_SCALE_OPTIONS
        .iter()
        .position(|&(v, _)| (v - graphics.ui_scale).abs() < 0.01)
        .unwrap_or(2);
    spawn_selector_row(
        commands,
        container,
        "UI Scale:",
        &scale_labels,
        scale_idx,
        SelectorField::UiScale,
    );

    let apply_btn = spawn_menu_button(commands, "APPLY", MenuAction::ApplySettings, true, fonts);
    commands.entity(container).add_child(apply_btn);
}

// ── UI Helpers ──

/// Panel with fade-in + subtle scale-in.
fn spawn_menu_panel(commands: &mut Commands) -> Entity {
    commands
        .spawn((
            MenuPageContainer,
            Interaction::None,
            ScrollPosition::default(),
            Node {
                width: Val::Px(560.0),
                max_height: Val::Percent(90.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(24.0)),
                overflow: Overflow::scroll_y(),
                border: UiRect::all(Val::Px(1.0)),
                // border_radius: BorderRadius::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.07, 0.07, 0.07, 0.0)),
            BorderColor::all(theme::SEPARATOR),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.6),
                Val::Px(0.0),
                Val::Px(4.0),
                Val::Px(0.0),
                Val::Px(24.0),
            ),
            UiFadeIn {
                timer: Timer::from_seconds(0.3, TimerMode::Once),
            },
            UiScaleIn {
                from: 0.96,
                timer: Timer::from_seconds(0.3, TimerMode::Once),
                elastic: false,
            },
        ))
        .id()
}

fn spawn_menu_button(
    commands: &mut Commands,
    label: &str,
    action: MenuAction,
    accent: bool,
    fonts: &UiFonts,
) -> Entity {
    let bg = if accent {
        theme::ACCENT
    } else {
        theme::BTN_PRIMARY
    };

    let mut entity_commands = commands.spawn((
        MenuButton(action),
        Button,
        ButtonAnimState::new(bg.to_srgba().to_f32_array()),
        ButtonStyle::Filled,
        Node {
            width: Val::Px(240.0),
            height: Val::Px(44.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(4.0)),
            // border_radius: BorderRadius::all(Val::Px(6.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(bg),
        BorderColor::all(Color::NONE),
    ));
    if accent {
        entity_commands.insert((
            UiGlowPulse {
                color: theme::ACCENT,
                intensity: 0.5,
            },
            BoxShadow::new(
                Color::srgba(0.29, 0.62, 1.0, 0.2),
                Val::Px(0.0),
                Val::Px(0.0),
                Val::Px(0.0),
                Val::Px(6.0),
            ),
        ));
    }
    entity_commands.with_children(|parent| {
        parent.spawn((
            Text::new(label),
            fonts::heading(fonts, theme::FONT_BUTTON),
            TextColor(Color::WHITE),
            Pickable::IGNORE,
        ));
    });
    entity_commands.id()
}

fn spawn_page_header(commands: &mut Commands, container: Entity, title: &str, fonts: &UiFonts) {
    let row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            margin: UiRect::bottom(Val::Px(16.0)),
            ..default()
        })
        .with_children(|parent| {
            parent
                .spawn((
                    MenuButton(MenuAction::Back),
                    Button,
                    ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
                    ButtonStyle::Ghost,
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                        // border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("<< BACK"),
                        fonts::body_emphasis(fonts, theme::FONT_MEDIUM),
                        TextColor(theme::TEXT_SECONDARY),
                        Pickable::IGNORE,
                    ));
                });

            parent.spawn((
                Text::new(title),
                fonts::heading(fonts, theme::FONT_HEADING),
                TextColor(Color::WHITE),
            ));
        })
        .id();
    commands.entity(container).add_child(row);
}

/// Section divider with expanding line animation.
fn spawn_animated_section_divider(
    commands: &mut Commands,
    container: Entity,
    label: &str,
    fonts: &UiFonts,
) {
    let row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(10.0)),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                UiLineExpand {
                    target_width: 40.0,
                    timer: Timer::from_seconds(0.4, TimerMode::Once),
                },
                Node {
                    width: Val::Px(0.0),
                    height: Val::Px(1.0),
                    margin: UiRect::right(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(theme::SEPARATOR),
            ));

            parent.spawn((
                Text::new(label),
                fonts::heading(fonts, theme::FONT_SMALL),
                TextColor(theme::TEXT_SECONDARY),
                Node {
                    margin: UiRect::horizontal(Val::Px(4.0)),
                    ..default()
                },
            ));

            parent.spawn((
                UiLineExpand {
                    target_width: 400.0,
                    timer: Timer::from_seconds(0.5, TimerMode::Once),
                },
                Node {
                    width: Val::Px(0.0),
                    height: Val::Px(1.0),
                    flex_grow: 1.0,
                    margin: UiRect::left(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(theme::SEPARATOR),
            ));
        })
        .id();
    commands.entity(container).add_child(row);
}

fn spawn_selector_row(
    commands: &mut Commands,
    container: Entity,
    label: &str,
    options: &[&str],
    selected: usize,
    field: SelectorField,
) {
    let row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(6.0)),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text::new(label),
                TextFont {
                    font_size: theme::FONT_MEDIUM,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
                Node {
                    width: Val::Px(120.0),
                    ..default()
                },
            ));

            for (i, &opt) in options.iter().enumerate() {
                let is_selected = i == selected;
                let bg = if is_selected {
                    theme::ACCENT
                } else {
                    theme::BTN_PRIMARY
                };
                let text_color = if is_selected {
                    Color::WHITE
                } else {
                    theme::TEXT_SECONDARY
                };

                let mut btn = parent.spawn((
                    MenuSelector { field, index: i },
                    Button,
                    ButtonAnimState::new(bg.to_srgba().to_f32_array()),
                    ButtonStyle::Filled,
                    Node {
                        padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                        margin: UiRect::horizontal(Val::Px(2.0)),
                        // border_radius: BorderRadius::all(Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(bg),
                    BorderColor::all(if is_selected {
                        Color::srgba(0.29, 0.62, 1.0, 0.3)
                    } else {
                        Color::NONE
                    }),
                ));
                if is_selected {
                    btn.insert(SelectedOption);
                }
                btn.with_children(|btn_parent| {
                    btn_parent.spawn((
                        Text::new(opt),
                        TextFont {
                            font_size: theme::FONT_MEDIUM,
                            ..default()
                        },
                        TextColor(text_color),
                        Pickable::IGNORE,
                    ));
                });
            }
        })
        .id();
    commands.entity(container).add_child(row);
}

// ── Player Name Input Row ──

fn spawn_name_input_row(commands: &mut Commands, current_name: &str) -> Entity {
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(6.0)),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text::new("Name:"),
                TextFont {
                    font_size: theme::FONT_MEDIUM,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
                Node {
                    width: Val::Px(120.0),
                    ..default()
                },
            ));

            parent
                .spawn((
                    TextInputField {
                        value: current_name.to_string(),
                        cursor_pos: current_name.len(),
                        max_len: 20,
                    },
                    Button,
                    Node {
                        width: Val::Px(280.0),
                        height: Val::Px(32.0),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        // border_radius: BorderRadius::all(Val::Px(4.0)),
                        align_items: AlignItems::Center,
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(theme::INPUT_BG),
                    BorderColor::all(theme::INPUT_BORDER),
                ))
                .with_children(|input| {
                    input.spawn((
                        Text::new(current_name),
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

            parent
                .spawn((
                    RandomNameButton,
                    Button,
                    ButtonAnimState::new(theme::BTN_PRIMARY.to_srgba().to_f32_array()),
                    ButtonStyle::Ghost,
                    Node {
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                        margin: UiRect::left(Val::Px(6.0)),
                        // border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("Random"),
                        TextFont {
                            font_size: theme::FONT_MEDIUM,
                            ..default()
                        },
                        TextColor(theme::ACCENT),
                        Pickable::IGNORE,
                    ));
                });
        })
        .id()
}

// ── Color Picker ──

fn spawn_color_picker(commands: &mut Commands, selected: usize) -> Entity {
    let colors = [
        Faction::Player1.color(),
        Faction::Player2.color(),
        Faction::Player3.color(),
        Faction::Player4.color(),
    ];

    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(6.0)),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text::new("Color:"),
                TextFont {
                    font_size: theme::FONT_MEDIUM,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
                Node {
                    width: Val::Px(120.0),
                    ..default()
                },
            ));

            for (i, &color) in colors.iter().enumerate() {
                let is_selected = i == selected;
                let size = if is_selected { 36.0 } else { 32.0 };
                let border_color = if is_selected {
                    Color::WHITE
                } else {
                    Color::NONE
                };
                let border_width = if is_selected { 3.0 } else { 2.0 };

                let mut dot = parent.spawn((
                    MenuSelector {
                        field: SelectorField::PlayerColor,
                        index: i,
                    },
                    Button,
                    Node {
                        width: Val::Px(size),
                        height: Val::Px(size),
                        margin: UiRect::horizontal(Val::Px(5.0)),
                        // border_radius: BorderRadius::all(Val::Px(size / 4.0)),
                        border: UiRect::all(Val::Px(border_width)),
                        ..default()
                    },
                    BackgroundColor(color),
                    BorderColor::all(border_color),
                ));
                if is_selected {
                    let glow_color = color.to_srgba();
                    dot.insert((
                        BoxShadow::new(
                            Color::srgba(glow_color.red, glow_color.green, glow_color.blue, 0.5),
                            Val::Px(0.0),
                            Val::Px(0.0),
                            Val::Px(0.0),
                            Val::Px(8.0),
                        ),
                        SelectedOption,
                        UiGlowPulse {
                            color,
                            intensity: 0.8,
                        },
                    ));
                }
            }
        })
        .id()
}

// ── AI Player Card ──

fn spawn_ai_card(
    commands: &mut Commands,
    container: Entity,
    ai_index: usize,
    config: &GameSetupConfig,
    visible: bool,
) {
    let display = if visible {
        Display::Flex
    } else {
        Display::None
    };
    let faction_idx = ai_index + 1;
    let faction_color = match faction_idx {
        1 => Faction::Player2.color(),
        2 => Faction::Player3.color(),
        _ => Faction::Player4.color(),
    };
    let is_ally = config.player_teams[faction_idx] == config.player_teams[0];
    let diff_idx = match config.ai_difficulties[ai_index] {
        AiDifficulty::Easy => 0,
        AiDifficulty::Medium => 1,
        AiDifficulty::Hard => 2,
    };

    let card = commands
        .spawn((
            AiCardContainer(ai_index),
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(10.0)),
                margin: UiRect::vertical(Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                // border_radius: BorderRadius::all(Val::Px(6.0)),
                display,
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(theme::BG_SURFACE),
            BorderColor::all(theme::SEPARATOR),
        ))
        .with_children(|card| {
            // Top row: AI label + faction color dot + ally/enemy toggle
            card.spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new(format!("AI {}", ai_index + 1)),
                    TextFont {
                        font_size: theme::FONT_MEDIUM,
                        ..default()
                    },
                    TextColor(theme::TEXT_PRIMARY),
                    Node {
                        margin: UiRect::right(Val::Px(8.0)),
                        ..default()
                    },
                ));

                row.spawn((
                    Node {
                        width: Val::Px(16.0),
                        height: Val::Px(16.0),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        margin: UiRect::right(Val::Px(12.0)),
                        ..default()
                    },
                    BackgroundColor(faction_color),
                    BoxShadow::new(
                        {
                            let c = faction_color.to_srgba();
                            Color::srgba(c.red, c.green, c.blue, 0.4)
                        },
                        Val::Px(0.0),
                        Val::Px(0.0),
                        Val::Px(0.0),
                        Val::Px(4.0),
                    ),
                ));

                row.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });

                let (toggle_bg, toggle_text) = if is_ally {
                    (theme::SUCCESS, "ALLY")
                } else {
                    (theme::DESTRUCTIVE, "ENEMY")
                };
                row.spawn((
                    AllyToggleButton { ai_index },
                    Button,
                    ButtonAnimState::new(toggle_bg.to_srgba().to_f32_array()),
                    ButtonStyle::Filled,
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                        // border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(toggle_bg),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new(toggle_text),
                        TextFont {
                            font_size: theme::FONT_BODY,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                        Pickable::IGNORE,
                    ));
                });
            });

            // Bottom row: Difficulty selector
            card.spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new("Difficulty:"),
                    TextFont {
                        font_size: theme::FONT_MEDIUM,
                        ..default()
                    },
                    TextColor(theme::TEXT_SECONDARY),
                    Node {
                        width: Val::Px(80.0),
                        margin: UiRect::right(Val::Px(10.0)),
                        ..default()
                    },
                ));

                let diff_names = ["Easy", "Medium", "Hard"];
                for (i, &opt) in diff_names.iter().enumerate() {
                    let is_selected = i == diff_idx;
                    let bg = if is_selected {
                        theme::ACCENT
                    } else {
                        theme::BTN_PRIMARY
                    };
                    let text_color = if is_selected {
                        Color::WHITE
                    } else {
                        theme::TEXT_SECONDARY
                    };

                    let mut btn = row.spawn((
                        MenuSelector {
                            field: SelectorField::AiDifficulty(ai_index),
                            index: i,
                        },
                        Button,
                        ButtonAnimState::new(bg.to_srgba().to_f32_array()),
                        ButtonStyle::Filled,
                        Node {
                            padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                            margin: UiRect::horizontal(Val::Px(2.0)),
                            // border_radius: BorderRadius::all(Val::Px(4.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(bg),
                        BorderColor::all(if is_selected {
                            Color::srgba(0.29, 0.62, 1.0, 0.3)
                        } else {
                            Color::NONE
                        }),
                    ));
                    if is_selected {
                        btn.insert(SelectedOption);
                    }
                    btn.with_children(|btn_parent| {
                        btn_parent.spawn((
                            Text::new(opt),
                            TextFont {
                                font_size: theme::FONT_MEDIUM,
                                ..default()
                            },
                            TextColor(text_color),
                            Pickable::IGNORE,
                        ));
                    });
                }
            });
        })
        .id();
    commands.entity(container).add_child(card);
}

// ── Systems ──

fn handle_menu_buttons(
    interactions: Query<(&Interaction, &MenuButton), Changed<Interaction>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut page: ResMut<MenuPage>,
    graphics: Res<GraphicsSettings>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
    roots: Query<Entity, With<MenuRoot>>,
    mut windows: Query<&mut Window>,
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
                start_hosting(&mut commands);
                *page = MenuPage::HostLobby;
                rebuild_menu(&mut commands, &roots);
            }
            MenuAction::JoinGame => {
                *page = MenuPage::JoinLobby;
                rebuild_menu(&mut commands, &roots);
            }
            MenuAction::ConnectToHost => {
                // Handled by connect_to_host_system
            }
            MenuAction::StartMultiplayer => {
                // Insert marker — update_lobby_ui sends GameStart to clients, then transitions
                commands.insert_resource(PendingGameStart);
            }
            MenuAction::BackToMultiplayer => {
                *page = MenuPage::Multiplayer;
                rebuild_menu(&mut commands, &roots);
            }
            MenuAction::CopySessionCode => {
                // Copy session code — handled separately
            }
            MenuAction::CancelHost => {
                stop_hosting(&mut commands);
                *page = MenuPage::Multiplayer;
                rebuild_menu(&mut commands, &roots);
            }
            MenuAction::Disconnect => {
                stop_client(&mut commands);
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

fn page_transition_system(
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

        match *page {
            MenuPage::Title => spawn_title_page(&mut commands, container, &fonts),
            MenuPage::NewGame => spawn_new_game_page(&mut commands, container, &config, &fonts),
            MenuPage::Options => spawn_options_page(&mut commands, container, &graphics, &fonts),
            MenuPage::Multiplayer => spawn_multiplayer_page(&mut commands, container, &fonts),
            MenuPage::HostLobby => spawn_host_lobby_page(&mut commands, container, &fonts),
            MenuPage::JoinLobby => spawn_join_lobby_page(&mut commands, container, &fonts),
        }
    }
}

fn handle_selector_clicks(
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

fn randomize_seed_system(
    interactions: Query<&Interaction, (Changed<Interaction>, With<RandomizeSeedButton>)>,
    mut config: ResMut<GameSetupConfig>,
    mut seed_displays: Query<&mut Text, With<SeedDisplay>>,
) {
    for interaction in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        // Toggle between random (0) and a specific seed
        if config.map_seed == 0 {
            config.map_seed = rand::random::<u64>();
        } else {
            config.map_seed = 0;
        }
    }

    // Update seed display text
    let seed_text = if config.map_seed == 0 {
        "Random".to_string()
    } else {
        format!("{}", config.map_seed)
    };
    for mut text in &mut seed_displays {
        **text = seed_text.clone();
    }
}

fn update_selector_visuals(
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

fn update_ai_card_visibility(
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

// ── Menu Scroll System ──

const MENU_SCROLL_LINE_HEIGHT: f32 = 24.0;

fn menu_scroll_system(
    mut mouse_wheel: MessageReader<MouseWheel>,
    windows: Query<&Window>,
    mut panel_q: Query<
        (&mut ScrollPosition, &ComputedNode, &UiGlobalTransform),
        With<MenuPageContainer>,
    >,
) {
    let mut dy = 0.0;
    for ev in mouse_wheel.read() {
        dy += match ev.unit {
            MouseScrollUnit::Line => -ev.y * MENU_SCROLL_LINE_HEIGHT,
            MouseScrollUnit::Pixel => -ev.y,
        };
    }

    if dy.abs() < 0.001 {
        return;
    }

    let Some(cursor_phys) = windows
        .single()
        .ok()
        .and_then(|w| w.physical_cursor_position())
    else {
        return;
    };

    for (mut scroll_pos, computed, ui_tf) in &mut panel_q {
        if !computed.contains_point(*ui_tf, cursor_phys) {
            continue;
        }
        let max_scroll = (computed.content_size().y - computed.size().y).max(0.0)
            * computed.inverse_scale_factor();
        scroll_pos.y = (scroll_pos.y + dy).clamp(0.0, max_scroll);
    }
}

// ── Text Input System ──

fn text_input_system(
    mut inputs: Query<(
        Entity,
        &mut TextInputField,
        &Interaction,
        &Children,
        Option<&TextInputFocused>,
        Option<&SessionCodeInput>,
    )>,
    mut commands: Commands,
    mut config: ResMut<GameSetupConfig>,
    mut keyboard_events: MessageReader<KeyboardInput>,
    mut text_query: Query<&mut Text, Without<TextInputCursor>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let mut clicked_entity: Option<Entity> = None;
    for (entity, _, interaction, _, _, _) in &inputs {
        if *interaction == Interaction::Pressed {
            clicked_entity = Some(entity);
        }
    }

    if let Some(clicked) = clicked_entity {
        for (entity, _, _, _, focused, _) in &inputs {
            if entity == clicked {
                if focused.is_none() {
                    commands.entity(entity).insert(TextInputFocused);
                    commands
                        .entity(entity)
                        .insert(BorderColor::all(theme::INPUT_BORDER_FOCUSED));
                }
            } else if focused.is_some() {
                commands.entity(entity).remove::<TextInputFocused>();
                commands
                    .entity(entity)
                    .insert(BorderColor::all(theme::INPUT_BORDER));
            }
        }
    }

    let events: Vec<_> = keyboard_events.read().cloned().collect();
    if events.is_empty() {
        return;
    }

    let cmd_key = keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight)
        || keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight);

    for (entity, mut field, _, children, focused, is_session_code) in &mut inputs {
        if focused.is_none() {
            continue;
        }
        for event in &events {
            if !event.state.is_pressed() {
                continue;
            }

            let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

            // Cmd/Ctrl+V — paste from clipboard
            if cmd_key && event.key_code == KeyCode::KeyV {
                if let Some(clip) = clipboard_read() {
                    for ch in clip.chars() {
                        if field.value.len() >= field.max_len {
                            break;
                        }
                        if ch.is_ascii_graphic() || ch == ' ' {
                            let pos = field.cursor_pos;
                            field.value.insert(pos, ch);
                            field.cursor_pos += 1;
                        }
                    }
                }
                continue;
            }

            // Cmd/Ctrl+C — copy field value
            if cmd_key && event.key_code == KeyCode::KeyC {
                clipboard_write(&field.value);
                continue;
            }

            // Cmd/Ctrl+A — select all (move cursor to end for now)
            if cmd_key && event.key_code == KeyCode::KeyA {
                field.cursor_pos = field.value.len();
                continue;
            }

            match event.key_code {
                KeyCode::Backspace => {
                    if field.cursor_pos > 0 {
                        field.cursor_pos -= 1;
                        let pos = field.cursor_pos;
                        field.value.remove(pos);
                    }
                }
                KeyCode::Delete => {
                    let pos = field.cursor_pos;
                    if pos < field.value.len() {
                        field.value.remove(pos);
                    }
                }
                KeyCode::ArrowLeft => {
                    if field.cursor_pos > 0 {
                        field.cursor_pos -= 1;
                    }
                }
                KeyCode::ArrowRight => {
                    if field.cursor_pos < field.value.len() {
                        field.cursor_pos += 1;
                    }
                }
                KeyCode::Home => {
                    field.cursor_pos = 0;
                }
                KeyCode::End => {
                    field.cursor_pos = field.value.len();
                }
                KeyCode::Enter | KeyCode::Escape => {
                    commands.entity(entity).remove::<TextInputFocused>();
                    commands
                        .entity(entity)
                        .insert(BorderColor::all(theme::INPUT_BORDER));
                }
                KeyCode::Space => {
                    if field.value.len() < field.max_len {
                        let pos = field.cursor_pos;
                        field.value.insert(pos, ' ');
                        field.cursor_pos += 1;
                    }
                }
                code => {
                    if let Some(ch) = keycode_to_char(code, shift) {
                        if field.value.len() < field.max_len {
                            let pos = field.cursor_pos;
                            field.value.insert(pos, ch);
                            field.cursor_pos += 1;
                        }
                    }
                }
            }
        }

        // Only update player_name for the name input, not session code inputs
        if is_session_code.is_none() {
            config.player_name = field.value.clone();
        }
        for child in children.iter() {
            if let Ok(mut text) = text_query.get_mut(child) {
                **text = field.value.clone();
            }
        }
    }
}

fn keycode_to_char(code: KeyCode, shift: bool) -> Option<char> {
    let ch = match code {
        KeyCode::KeyA => 'a',
        KeyCode::KeyB => 'b',
        KeyCode::KeyC => 'c',
        KeyCode::KeyD => 'd',
        KeyCode::KeyE => 'e',
        KeyCode::KeyF => 'f',
        KeyCode::KeyG => 'g',
        KeyCode::KeyH => 'h',
        KeyCode::KeyI => 'i',
        KeyCode::KeyJ => 'j',
        KeyCode::KeyK => 'k',
        KeyCode::KeyL => 'l',
        KeyCode::KeyM => 'm',
        KeyCode::KeyN => 'n',
        KeyCode::KeyO => 'o',
        KeyCode::KeyP => 'p',
        KeyCode::KeyQ => 'q',
        KeyCode::KeyR => 'r',
        KeyCode::KeyS => 's',
        KeyCode::KeyT => 't',
        KeyCode::KeyU => 'u',
        KeyCode::KeyV => 'v',
        KeyCode::KeyW => 'w',
        KeyCode::KeyX => 'x',
        KeyCode::KeyY => 'y',
        KeyCode::KeyZ => 'z',
        KeyCode::Digit0 => return if shift { Some(')') } else { Some('0') },
        KeyCode::Digit1 => return if shift { Some('!') } else { Some('1') },
        KeyCode::Digit2 => return if shift { Some('@') } else { Some('2') },
        KeyCode::Digit3 => return if shift { Some('#') } else { Some('3') },
        KeyCode::Digit4 => return if shift { Some('$') } else { Some('4') },
        KeyCode::Digit5 => return if shift { Some('%') } else { Some('5') },
        KeyCode::Digit6 => return if shift { Some('^') } else { Some('6') },
        KeyCode::Digit7 => return if shift { Some('&') } else { Some('7') },
        KeyCode::Digit8 => return if shift { Some('*') } else { Some('8') },
        KeyCode::Digit9 => return if shift { Some('(') } else { Some('9') },
        KeyCode::Minus => return if shift { Some('_') } else { Some('-') },
        KeyCode::Period => return if shift { Some('>') } else { Some('.') },
        KeyCode::Semicolon => return if shift { Some(':') } else { Some(';') },
        KeyCode::Slash => return if shift { Some('?') } else { Some('/') },
        KeyCode::BracketLeft => return if shift { Some('{') } else { Some('[') },
        KeyCode::BracketRight => return if shift { Some('}') } else { Some(']') },
        KeyCode::Backquote => return if shift { Some('~') } else { Some('`') },
        KeyCode::Equal => return if shift { Some('+') } else { Some('=') },
        KeyCode::Backslash => return if shift { Some('|') } else { Some('\\') },
        KeyCode::Quote => return if shift { Some('"') } else { Some('\'') },
        KeyCode::Comma => return if shift { Some('<') } else { Some(',') },
        _ => return None,
    };
    if shift && ch.is_ascii_alphabetic() {
        Some(ch.to_ascii_uppercase())
    } else {
        Some(ch)
    }
}

// ── Clipboard helpers ──

fn clipboard_read() -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::process::Command::new("pbpaste")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    String::from_utf8(o.stdout).ok()
                } else {
                    // Fallback for Linux: try xclip
                    std::process::Command::new("xclip")
                        .args(["-selection", "clipboard", "-o"])
                        .output()
                        .ok()
                        .and_then(|o2| String::from_utf8(o2.stdout).ok())
                }
            })
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

fn clipboard_write(text: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::io::Write;
        if let Ok(mut child) = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        } else {
            // Fallback for Linux: try xclip
            if let Ok(mut child) = std::process::Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(std::process::Stdio::piped())
                .spawn()
            {
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = text;
    }
}

fn text_input_cursor_blink(
    time: Res<Time>,
    focused: Query<&Children, With<TextInputFocused>>,
    not_focused: Query<&Children, (With<TextInputField>, Without<TextInputFocused>)>,
    mut cursors: Query<&mut TextColor, With<TextInputCursor>>,
) {
    for children in &focused {
        for child in children.iter() {
            if let Ok(mut color) = cursors.get_mut(child) {
                let t = time.elapsed_secs();
                let blink = (t * 3.0).sin() * 0.5 + 0.5;
                let c = theme::ACCENT.to_srgba();
                color.0 = Color::srgba(c.red, c.green, c.blue, blink);
            }
        }
    }
    for children in &not_focused {
        for child in children.iter() {
            if let Ok(mut color) = cursors.get_mut(child) {
                color.0 = Color::NONE;
            }
        }
    }
}

fn random_name_system(
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

// ── Ally Toggle System ──

fn ally_toggle_system(
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

fn update_ally_toggle_visuals(
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

// ── Multiplayer Page ──

fn spawn_multiplayer_page(commands: &mut Commands, container: Entity, fonts: &UiFonts) {
    spawn_page_header(commands, container, "MULTIPLAYER", fonts);

    spawn_animated_section_divider(commands, container, "LAN GAME", fonts);

    let desc = commands
        .spawn((
            Text::new("Play with others on your local network"),
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

    let host_btn = spawn_menu_button(commands, "HOST GAME", MenuAction::HostGame, true, fonts);
    commands.entity(container).add_child(host_btn);

    let join_btn = spawn_menu_button(commands, "JOIN GAME", MenuAction::JoinGame, false, fonts);
    commands.entity(container).add_child(join_btn);
}

// ── Host Lobby Page ──

fn spawn_host_lobby_page(commands: &mut Commands, container: Entity, fonts: &UiFonts) {
    spawn_page_header_with_action(commands, container, "HOST LOBBY", MenuAction::CancelHost, fonts);

    spawn_animated_section_divider(commands, container, "SESSION CODE", fonts);

    // Session code display + copy button
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
            // Copy button
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
            Text::new("Share this code with players on your LAN"),
            TextFont {
                font_size: theme::FONT_SMALL,
                ..default()
            },
            TextColor(theme::TEXT_SECONDARY),
            Node {
                margin: UiRect::bottom(Val::Px(12.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(hint);

    spawn_animated_section_divider(commands, container, "PLAYERS", fonts);

    // Player slots
    for i in 0..4 {
        let label = if i == 0 {
            "Host (You)"
        } else {
            "Waiting..."
        };
        let color = if i == 0 {
            theme::SUCCESS
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
                // Status dot
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

    spawn_animated_section_divider(commands, container, "", fonts);

    // Status text
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

    // Start Game button
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
                height: Val::Px(50.0),
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

fn spawn_join_lobby_page(commands: &mut Commands, container: Entity, fonts: &UiFonts) {
    spawn_page_header_with_action(commands, container, "JOIN GAME", MenuAction::BackToMultiplayer, fonts);

    spawn_animated_section_divider(commands, container, "SESSION CODE", fonts);

    // Text input for session code (IP:port)
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

    // Connect button
    let connect_btn =
        spawn_menu_button(commands, "CONNECT", MenuAction::ConnectToHost, true, fonts);
    commands.entity(container).add_child(connect_btn);

    spawn_animated_section_divider(commands, container, "STATUS", fonts);

    // Status text
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

    // Player slots (updated from host lobby broadcasts)
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

    // Disconnect button
    let dc_btn = spawn_menu_button(commands, "DISCONNECT", MenuAction::Disconnect, false, fonts);
    commands.entity(container).add_child(dc_btn);
}

fn spawn_page_header_with_action(
    commands: &mut Commands,
    container: Entity,
    title: &str,
    back_action: MenuAction,
    fonts: &UiFonts,
) {
    let row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            margin: UiRect::bottom(Val::Px(16.0)),
            ..default()
        })
        .with_children(|parent| {
            parent
                .spawn((
                    MenuButton(back_action),
                    Button,
                    ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
                    ButtonStyle::Ghost,
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("<< BACK"),
                        fonts::body_emphasis(fonts, theme::FONT_MEDIUM),
                        TextColor(theme::TEXT_SECONDARY),
                        Pickable::IGNORE,
                    ));
                });

            parent.spawn((
                Text::new(title),
                fonts::heading(fonts, theme::FONT_HEADING),
                TextColor(Color::WHITE),
            ));
        })
        .id();
    commands.entity(container).add_child(row);
}

// ── Networking helpers (menu-side) ──

const DEFAULT_PORT: u16 = 7878;

fn start_hosting(commands: &mut Commands) {
    use crate::multiplayer::transport;
    use std::net::TcpListener;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;
    use std::sync::Arc;

    let ip = transport::detect_lan_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    let addr = format!("0.0.0.0:{}", DEFAULT_PORT);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            warn!("Failed to bind TCP listener on {}: {}", addr, e);
            return;
        }
    };

    let session_code = format!("{}:{}", ip, DEFAULT_PORT);
    info!("Hosting on {}", session_code);

    let shutdown = Arc::new(AtomicBool::new(false));
    let (new_client_tx, new_client_rx) = mpsc::channel();
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (dc_tx, dc_rx) = mpsc::channel();

    // Store cmd_tx for spawning reader threads later
    let cmd_tx_clone = cmd_tx.clone();
    let dc_tx_clone = dc_tx.clone();
    let shutdown_clone = shutdown.clone();

    // Start listener thread
    let shutdown_listener = shutdown.clone();
    std::thread::spawn(move || {
        transport::host_listener_thread(listener, new_client_tx, shutdown_listener);
    });

    commands.insert_resource(HostNetState {
        incoming_commands: std::sync::Mutex::new(cmd_rx),
        client_senders: std::sync::Mutex::new(Vec::new()),
        new_clients: std::sync::Mutex::new(new_client_rx),
        disconnect_rx: std::sync::Mutex::new(dc_rx),
        shutdown: shutdown.clone(),
        seq: std::sync::Mutex::new(0),
    });

    // Store the command sender for spawning reader threads — we'll use a side-channel
    commands.insert_resource(HostConnectionFactory {
        cmd_tx: cmd_tx_clone,
        dc_tx: dc_tx_clone,
        shutdown: shutdown_clone,
    });

    commands.insert_resource(NetRole::Host);
    commands.insert_resource(LobbyState {
        players: vec![LobbyPlayer {
            player_id: 0,
            name: "Host".to_string(),
            seat_index: 0,
            faction: Faction::Player1,
            color_index: 0, // seat-defaulted: Blue
            is_host: true,
            connected: true,
        }],
        session_code,
        status: LobbyStatus::Waiting,
    });
}

/// Temporary resource to allow spawning reader/writer threads for new clients.
#[derive(Resource)]
struct HostConnectionFactory {
    cmd_tx: std::sync::mpsc::Sender<(u8, game_state::message::ClientMessage)>,
    dc_tx: std::sync::mpsc::Sender<u8>,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

fn stop_hosting(commands: &mut Commands) {
    // Removing the resources causes channel disconnection, which signals threads to stop.
    commands.remove_resource::<HostNetState>();
    commands.remove_resource::<HostConnectionFactory>();
    commands.insert_resource(NetRole::Offline);
    commands.insert_resource(LobbyState::default());
}

fn stop_client(commands: &mut Commands) {
    commands.remove_resource::<ClientNetState>();
    commands.insert_resource(NetRole::Offline);
    commands.insert_resource(LobbyState::default());
}

/// Marker resource: host pressed "Start Game", lobby system should send event and transition.
#[derive(Resource)]
struct PendingGameStart;

fn connect_to_host_system(
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

        // Read the session code from the text input
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

        // Parse and connect
        let addr = if code.contains(':') {
            code.clone()
        } else {
            format!("{}:{}", code, DEFAULT_PORT)
        };

        for mut text in &mut status_texts {
            **text = format!("Connecting to {}...", addr);
        }

        match std::net::TcpStream::connect_timeout(
            &addr.parse().unwrap_or_else(|_| {
                std::net::SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT))
            }),
            std::time::Duration::from_secs(3),
        ) {
            Ok(stream) => {
                stream.set_nodelay(true).ok();

                let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let (incoming_tx, incoming_rx) = std::sync::mpsc::channel();
                let (outgoing_tx, outgoing_rx) = std::sync::mpsc::channel();

                // Clone stream for writer thread
                let read_stream = stream.try_clone().expect("Failed to clone TCP stream");
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

                // Send join request
                let join_msg = game_state::message::ClientMessage::JoinRequest {
                    seq: 0,
                    timestamp: 0.0,
                    player_name: "Client".to_string(),
                    preferred_faction_index: None,
                };
                if let Ok(json) = serde_json::to_vec(&join_msg) {
                    let _ = outgoing_tx.send(json);
                }

                commands.insert_resource(ClientNetState {
                    incoming: std::sync::Mutex::new(incoming_rx),
                    outgoing: outgoing_tx,
                    shutdown,
                    player_id: 0,
                    seat_index: 0,     // assigned by host via JoinAccepted
                    my_faction: Faction::Player2, // assigned by host via JoinAccepted
                    color_index: 0,    // assigned by host via JoinAccepted
                    seq: std::sync::Mutex::new(0),
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
}

fn copy_session_code_system(
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

/// Update lobby UI — polls channels for new connections (host) or lobby updates (client).
fn update_lobby_ui(
    page: Res<MenuPage>,
    mut lobby: ResMut<LobbyState>,
    host_state: Option<Res<HostNetState>>,
    host_factory: Option<Res<HostConnectionFactory>>,
    client_state: Option<ResMut<ClientNetState>>,
    pending_start: Option<Res<PendingGameStart>>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
    mut session_code_texts: Query<&mut Text, With<SessionCodeText>>,
    mut status_texts: Query<&mut Text, (With<LobbyStatusText>, Without<SessionCodeText>)>,
    slot_texts: Query<(&LobbyPlayerSlot, &Children)>,
    mut child_texts: Query<&mut Text, (Without<LobbyStatusText>, Without<SessionCodeText>, Without<LobbyPlayerSlot>)>,
    mut child_bgs: Query<&mut BackgroundColor, Without<LobbyPlayerSlot>>,
    mut config: ResMut<GameSetupConfig>,
) {
    // Update session code display
    if *page == MenuPage::HostLobby {
        for mut text in &mut session_code_texts {
            if **text != lobby.session_code && !lobby.session_code.is_empty() {
                **text = lobby.session_code.clone();
            }
        }
    }

    // ── Host: check for new clients ──
    if let (Some(host), Some(factory)) = (host_state.as_ref(), host_factory.as_ref()) {
        let new_clients_rx = host.new_clients.lock().unwrap();
        let mut lobby_changed = false;
        loop {
            match new_clients_rx.try_recv() {
                Ok(event) => {
                    let player_id = event.player_id;
                    info!("New client {} in lobby", player_id);

                    let seat_index = lobby.players.len().min(3) as u8;
                    let faction = Faction::PLAYERS[seat_index as usize];
                    let color_index = seat_index; // seat-defaulted

                    lobby.players.push(LobbyPlayer {
                        player_id,
                        name: format!("Player {}", player_id),
                        seat_index,
                        faction,
                        color_index,
                        is_host: false,
                        connected: true,
                    });

                    // Spawn reader/writer threads for this client
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

                    host.client_senders.lock().unwrap().push((player_id, writer_tx));
                    lobby_changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }

        let incoming_commands = host.incoming_commands.lock().unwrap();
        loop {
            match incoming_commands.try_recv() {
                Ok((player_id, game_state::message::ClientMessage::JoinRequest { player_name, .. })) => {
                    if let Some(player) = lobby.players.iter_mut().find(|p| p.player_id == player_id) {
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
                        let msg = game_state::message::ServerMessage::Event {
                            seq,
                            timestamp: 0.0,
                            events: vec![game_state::message::GameEvent::JoinAccepted {
                                player_id,
                                seat_index,
                                faction_index,
                                color_index,
                            }],
                        };
                        if let Ok(json) = serde_json::to_vec(&msg) {
                            let senders = host.client_senders.lock().unwrap();
                            if let Some((_, sender)) = senders.iter().find(|(id, _)| *id == player_id) {
                                let _ = sender.send(json);
                            }
                        }
                        lobby_changed = true;
                    }
                }
                Ok((player_id, game_state::message::ClientMessage::LeaveNotice { .. })) => {
                    if let Some(player) = lobby.players.iter_mut().find(|p| p.player_id == player_id) {
                        player.connected = false;
                        lobby_changed = true;
                    }
                }
                Ok((_player_id, game_state::message::ClientMessage::Input { .. })) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }

        if lobby_changed {
            broadcast_lobby_update(&lobby, host);
        }

        // Update host lobby status text
        if *page == MenuPage::HostLobby {
            let connected = lobby.players.iter().filter(|p| p.connected).count();
            for mut text in &mut status_texts {
                **text = format!(
                    "{} player(s) in lobby{}",
                    connected,
                    if connected >= 2 { " — ready to start!" } else { "" }
                );
            }
        }

        // ── Host: handle PendingGameStart ──
        if pending_start.is_some() {
            // Resolve seed now so host and client use the same one
            if config.map_seed == 0 {
                config.map_seed = rand::random::<u64>();
                info!("Host resolved random map seed: {}", config.map_seed);
            }

            // Adjust AI count: total 4 factions, subtract human players
            let human_count = lobby.players.iter().filter(|p| p.connected).count() as u8;
            config.num_ai_opponents = (4u8.saturating_sub(human_count)).min(3);
            info!(
                "Multiplayer: {} humans, {} AI opponents",
                human_count, config.num_ai_opponents
            );

            let config_json = serde_json::to_string(&SerializableGameConfig::from_config(&config, &lobby))
                .unwrap_or_default();

            let start_event = game_state::message::ServerMessage::Event {
                seq: 0,
                timestamp: 0.0,
                events: vec![game_state::message::GameEvent::GameStart { config_json }],
            };
            if let Ok(json) = serde_json::to_vec(&start_event) {
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
                            } => {
                                client.player_id = *player_id;
                                client.seat_index = *seat_index;
                                client.my_faction = Faction::PLAYERS
                                    .get(*faction_index as usize)
                                    .copied()
                                    .unwrap_or(Faction::Player2);
                                client.color_index = *color_index;
                                info!(
                                    "Join accepted: player_id={}, seat={}, faction={:?}, color={}",
                                    client.player_id, client.seat_index, client.my_faction, client.color_index
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
                                // Apply host's config so we generate the same world
                                if let Ok(net_config) = serde_json::from_str::<SerializableGameConfig>(config_json) {
                                    net_config.apply_to_config(&mut config);
                                    // Rebuild lobby from authoritative seat assignments
                                    net_config.apply_to_lobby(&mut lobby);
                                    info!(
                                        "Applied host config: seed={}, map_size={}, {} seats",
                                        config.map_seed, net_config.map_size,
                                        net_config.seat_assignments.len()
                                    );
                                }
                                // ActivePlayer is set by configure_multiplayer_ai on OnEnter(InGame)
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
            }
        }
    }

    // ── Update player slot UI for both host and client ──
    update_player_slot_ui(&lobby, &slot_texts, &mut child_texts, &mut child_bgs);
}

fn update_player_slot_ui(
    lobby: &LobbyState,
    slot_texts: &Query<(&LobbyPlayerSlot, &Children)>,
    child_texts: &mut Query<&mut Text, (Without<LobbyStatusText>, Without<SessionCodeText>, Without<LobbyPlayerSlot>)>,
    child_bgs: &mut Query<&mut BackgroundColor, Without<LobbyPlayerSlot>>,
) {
    for (slot, children) in slot_texts {
        let idx = slot.0;
        let (label, color) = if let Some(player) = lobby.players.get(idx) {
            let c = if player.connected {
                if player.is_host {
                    theme::SUCCESS
                } else {
                    theme::ACCENT
                }
            } else {
                theme::DESTRUCTIVE
            };
            let l = if player.is_host {
                format!("{} (Host)", player.name)
            } else if player.connected {
                player.name.clone()
            } else {
                format!("{} (disconnected)", player.name)
            };
            (l, c)
        } else {
            ("Waiting...".to_string(), theme::TEXT_SECONDARY)
        };

        // children[0] = status dot (BackgroundColor), children[1] = text
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

fn broadcast_lobby_update(lobby: &LobbyState, host: &HostNetState) {
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

    if let Ok(json) = serde_json::to_vec(&msg) {
        let senders = host.client_senders.lock().unwrap();
        for (_id, sender) in senders.iter() {
            let _ = sender.send(json.clone());
        }
    }
}

/// Seat assignment info serialized for network transmission.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct SeatAssignment {
    pub player_id: u8,
    pub seat_index: u8,
    pub faction_index: u8,
    pub color_index: u8,
    pub is_human: bool,
}

/// All world-affecting config fields for network transmission.
#[derive(serde::Serialize, serde::Deserialize)]
struct SerializableGameConfig {
    pub map_seed: u64,
    pub num_ai_opponents: u8,
    pub ai_difficulties: [u8; 3],
    pub team_mode: u8,
    pub player_teams: [u8; 4],
    pub map_size: u8,
    pub resource_density: u8,
    pub day_cycle_secs: f32,
    pub starting_resources_mult: f32,
    /// Authoritative seat assignments from the lobby.
    pub seat_assignments: Vec<SeatAssignment>,
    /// Legacy: faction indices that are human-controlled (kept for backwards compat).
    #[serde(default)]
    pub human_factions: Vec<u8>,
}

impl SerializableGameConfig {
    fn from_config(config: &GameSetupConfig, lobby: &LobbyState) -> Self {
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
        }
    }

    fn apply_to_config(&self, config: &mut GameSetupConfig) {
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
    }

    /// Rebuild lobby player list from authoritative seat assignments.
    fn apply_to_lobby(&self, lobby: &mut LobbyState) {
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
