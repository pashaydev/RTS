use bevy::prelude::*;

use crate::components::*;
use crate::theme;
use crate::ui::fonts::{self, UiFonts};
use crate::ui::menu_helpers::*;

use super::*;

// ── Title Page ──

pub(crate) fn spawn_title_page(commands: &mut Commands, container: Entity, fonts: &UiFonts) {
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

    for (label, action) in [
        ("NEW GAME", MenuAction::NewGame),
        ("MULTIPLAYER", MenuAction::Multiplayer),
        ("OPTIONS", MenuAction::Options),
        ("QUIT", MenuAction::Quit),
    ] {
        let btn = spawn_styled_button(commands, label, MenuButton(action), false, fonts);
        commands.entity(container).add_child(btn);
    }

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

pub(crate) fn spawn_new_game_page(
    commands: &mut Commands,
    container: Entity,
    config: &GameSetupConfig,
    fonts: &UiFonts,
) {
    spawn_page_header(commands, container, "NEW GAME", MenuButton(MenuAction::Back), fonts);

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
                height: Val::Px(80.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                margin: UiRect {
                    top: Val::Px(20.0),
                    bottom: Val::Px(4.0),
                    ..default()
                },
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

pub(crate) fn spawn_options_page(
    commands: &mut Commands,
    container: Entity,
    graphics: &GraphicsSettings,
    fonts: &UiFonts,
) {
    spawn_page_header(commands, container, "OPTIONS", MenuButton(MenuAction::Back), fonts);

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

    let apply_btn = spawn_styled_button(
        commands,
        "APPLY",
        MenuButton(MenuAction::ApplySettings),
        true,
        fonts,
    );
    commands.entity(container).add_child(apply_btn);
}

// ── AI Player Card ──

pub(crate) fn spawn_ai_card(
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
                display,
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(theme::BG_SURFACE),
            BorderColor::all(theme::SEPARATOR),
        ))
        .with_children(|card| {
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
