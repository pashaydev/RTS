use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use rand::Rng;

use crate::components::*;
use crate::theme;
use crate::ui::fonts::{self, UiFonts};

// ── Resources & Components ──

#[derive(Resource, Default, PartialEq, Eq)]
enum MenuPage {
    #[default]
    Title,
    NewGame,
    Options,
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

// ── Plugin ──

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuPage>()
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
            );
    }
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
    }
}

fn cleanup_menu(
    mut commands: Commands,
    roots: Query<Entity, With<MenuRoot>>,
    cameras: Query<Entity, With<MenuCamera>>,
) {
    for e in &roots {
        commands.entity(e).despawn();
    }
    for e in &cameras {
        commands.entity(e).despawn();
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
        }
    }
}

fn rebuild_menu(commands: &mut Commands, roots: &Query<Entity, With<MenuRoot>>) {
    for e in roots {
        commands.entity(e).despawn();
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
    )>,
    mut commands: Commands,
    mut config: ResMut<GameSetupConfig>,
    mut keyboard_events: MessageReader<KeyboardInput>,
    mut text_query: Query<&mut Text, Without<TextInputCursor>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let mut clicked_entity: Option<Entity> = None;
    for (entity, _, interaction, _, _) in &inputs {
        if *interaction == Interaction::Pressed {
            clicked_entity = Some(entity);
        }
    }

    if let Some(clicked) = clicked_entity {
        for (entity, _, _, _, focused) in &inputs {
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

    for (entity, mut field, _, children, focused) in &mut inputs {
        if focused.is_none() {
            continue;
        }
        for event in &events {
            if !event.state.is_pressed() {
                continue;
            }

            let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

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

        config.player_name = field.value.clone();
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
        KeyCode::Digit0 => '0',
        KeyCode::Digit1 => '1',
        KeyCode::Digit2 => '2',
        KeyCode::Digit3 => '3',
        KeyCode::Digit4 => '4',
        KeyCode::Digit5 => '5',
        KeyCode::Digit6 => '6',
        KeyCode::Digit7 => '7',
        KeyCode::Digit8 => '8',
        KeyCode::Digit9 => '9',
        KeyCode::Minus => '-',
        KeyCode::Period => '.',
        _ => return None,
    };
    if shift && ch.is_ascii_alphabetic() {
        Some(ch.to_ascii_uppercase())
    } else {
        Some(ch)
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
