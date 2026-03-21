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

    spawn_animated_section_divider(commands, container, "FACTIONS", fonts);

    for i in 0..4 {
        spawn_slot_card(commands, container, i, config, false);
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

// ── Unified Slot Card ──

/// Spawn a faction slot card (used in both NewGame and HostLobby).
/// `is_multiplayer` controls whether "Open" is offered as a slot type.
pub(crate) fn spawn_slot_card(
    commands: &mut Commands,
    container: Entity,
    slot_index: usize,
    config: &GameSetupConfig,
    is_multiplayer: bool,
) {
    let slot = config.slots[slot_index];
    let faction_color = Faction::PLAYERS[slot_index].color();
    let is_you = is_multiplayer && slot_index == config.local_player_slot && matches!(slot, SlotOccupant::Human);
    let faction_label = if is_you {
        format!("Player {} (YOU)", slot_index + 1)
    } else {
        format!("Player {}", slot_index + 1)
    };

    // Determine slot type index for the selector
    let (type_options, type_idx) = if is_multiplayer {
        (
            vec!["Human", "Open", "AI", "None"],
            match slot {
                SlotOccupant::Human => 0,
                SlotOccupant::Open => 1,
                SlotOccupant::Ai(_) => 2,
                SlotOccupant::Closed => 3,
            },
        )
    } else {
        (
            vec!["Human", "AI", "None"],
            match slot {
                SlotOccupant::Human => 0,
                SlotOccupant::Ai(_) => 1,
                SlotOccupant::Closed | SlotOccupant::Open => 2,
            },
        )
    };

    let is_local = slot_index == config.local_player_slot && matches!(slot, SlotOccupant::Human);

    let border_col = if is_local || is_you {
        theme::ACCENT
    } else {
        theme::SEPARATOR
    };

    let card = commands
        .spawn((
            SlotCardContainer(slot_index),
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(10.0)),
                margin: UiRect::vertical(Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(theme::BG_SURFACE),
            BorderColor::all(border_col),
        ))
        .with_children(|card| {
            // Row 1: faction dot + label + type selector + team toggle
            card.spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|row| {
                // Faction color dot
                row.spawn((
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(20.0),
                        border_radius: BorderRadius::all(Val::Px(10.0)),
                        margin: UiRect::right(Val::Px(6.0)),
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

                // Faction label
                row.spawn((
                    Text::new(faction_label),
                    TextFont {
                        font_size: theme::FONT_MEDIUM,
                        ..default()
                    },
                    TextColor(faction_color),
                    Node {
                        margin: UiRect::right(Val::Px(8.0)),
                        ..default()
                    },
                ));

                // Type selector buttons
                let type_strs: Vec<&str> = type_options.iter().map(|s| *s).collect();
                for (i, &opt) in type_strs.iter().enumerate() {
                    let is_selected = i == type_idx;
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
                            field: SelectorField::SlotType(slot_index),
                            index: i,
                        },
                        Button,
                        ButtonAnimState::new(bg.to_srgba().to_f32_array()),
                        ButtonStyle::Filled,
                        Node {
                            padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
                            margin: UiRect::horizontal(Val::Px(1.0)),
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
                                font_size: theme::FONT_SMALL,
                                ..default()
                            },
                            TextColor(text_color),
                            Pickable::IGNORE,
                        ));
                    });
                }

                row.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });

                // Team selector (not shown for closed/none slots)
                if !matches!(slot, SlotOccupant::Closed | SlotOccupant::Open) {
                    let current_team = config.player_teams[slot_index] as usize;
                    let team_colors = [
                        Color::srgb(0.9, 0.75, 0.2),  // T1: Gold
                        Color::srgb(0.2, 0.75, 0.85),  // T2: Cyan
                        Color::srgb(0.85, 0.3, 0.65),  // T3: Pink
                        Color::srgb(0.95, 0.5, 0.15),  // T4: Orange
                    ];
                    for ti in 0..4 {
                        let is_sel = ti == current_team;
                        let color = team_colors[ti];
                        let size = if is_sel { 24.0 } else { 20.0 };
                        let border_color = if is_sel {
                            Color::WHITE
                        } else {
                            Color::NONE
                        };
                        let mut dot = row.spawn((
                            MenuSelector {
                                field: SelectorField::SlotTeam(slot_index),
                                index: ti,
                            },
                            Button,
                            Node {
                                width: Val::Px(size),
                                height: Val::Px(size),
                                margin: UiRect::horizontal(Val::Px(2.0)),
                                border: UiRect::all(Val::Px(if is_sel { 2.0 } else { 1.0 })),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BackgroundColor(if is_sel { color } else { Color::srgba(0.15, 0.15, 0.15, 0.8) }),
                            BorderColor::all(border_color),
                        ));
                        if is_sel {
                            let c = color.to_srgba();
                            dot.insert((
                                BoxShadow::new(
                                    Color::srgba(c.red, c.green, c.blue, 0.5),
                                    Val::Px(0.0),
                                    Val::Px(0.0),
                                    Val::Px(0.0),
                                    Val::Px(3.0),
                                ),
                                SelectedOption,
                            ));
                        }
                        dot.with_children(|btn| {
                            btn.spawn((
                                Text::new(format!("{}", ti + 1)),
                                TextFont {
                                    font_size: 10.0,
                                    ..default()
                                },
                                TextColor(if is_sel { Color::WHITE } else { color }),
                                Pickable::IGNORE,
                            ));
                        });
                    }
                }

                // Kick button (multiplayer, non-local human slots)
                if is_multiplayer && !is_local && matches!(slot, SlotOccupant::Human) {
                    row.spawn((
                        super::KickPlayerButton(slot_index),
                        Button,
                        ButtonAnimState::new(theme::DESTRUCTIVE.to_srgba().to_f32_array()),
                        ButtonStyle::Filled,
                        Node {
                            width: Val::Px(24.0),
                            height: Val::Px(24.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            margin: UiRect::left(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(theme::DESTRUCTIVE),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("X"),
                            TextFont {
                                font_size: theme::FONT_TINY,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                            Pickable::IGNORE,
                        ));
                    });
                }
            });

            // Row 2: Difficulty selector (only for AI slots)
            if let SlotOccupant::Ai(difficulty) = slot {
                let diff_idx = match difficulty {
                    AiDifficulty::Easy => 0,
                    AiDifficulty::Medium => 1,
                    AiDifficulty::Hard => 2,
                };
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
                                field: SelectorField::SlotDifficulty(slot_index),
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
            }
        })
        .id();
    commands.entity(container).add_child(card);
}
