use bevy::prelude::*;

use super::helpers::*;
use crate::types::*;
use crate::ui::fonts::{self, UiFonts};
use crate::ui::theme::{Theme, TEAM_COLORS};

#[allow(unused_imports)]
use super::*;

// ── New Game Page ──

#[derive(Component)]
pub(crate) struct ResourceDensityLabelText;

#[derive(Component)]
pub(crate) struct ResourceDensityValueText;

#[derive(Component)]
pub(crate) struct ResourceDensityBarFill;

#[derive(Component)]
pub(crate) struct ResourceDensityOptionDot(usize);

#[derive(Component)]
pub(crate) struct ResourceDensityOptionLabel(usize);

pub(crate) const HOST_LOBBY_SLOT_NAV_START: usize = 10;
pub(crate) const SLOT_NAV_ROWS_PER_CARD: usize = 3;

pub(crate) fn host_lobby_slot_nav(slot_index: usize) -> (usize, usize, usize) {
    let base = HOST_LOBBY_SLOT_NAV_START + slot_index * SLOT_NAV_ROWS_PER_CARD;
    (base, base + 1, base + 2)
}

pub(crate) fn spawn_new_game_page(
    commands: &mut Commands,
    container: Entity,
    config: &GameSetupConfig,
    fonts: &UiFonts,
    theme: &Theme,
) {
    let page = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(20.0),
            ..default()
        })
        .id();
    commands.entity(container).add_child(page);

    spawn_new_game_top_bar(commands, page, fonts, theme);

    let content = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(18.0),
            align_items: AlignItems::Stretch,
            ..default()
        })
        .id();
    commands.entity(page).add_child(content);

    let overview = commands
        .spawn(Node {
            width: Val::Percent(63.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::axes(Val::Px(6.0), Val::Px(10.0)),
            ..default()
        })
        .id();
    commands.entity(content).add_child(overview);

    spawn_new_game_overview_header(commands, overview, fonts, theme);
    spawn_command_profile_row(commands, overview, &config.player_name, theme);

    let slots_wrap = commands
        .spawn((
            SlotCardsContainer,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.0),
                margin: UiRect::top(Val::Px(12.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(overview).add_child(slots_wrap);

    for i in 0..4 {
        spawn_slot_card(commands, slots_wrap, i, config, false, None, None, theme);
    }

    let team_idx = match config.team_mode {
        TeamMode::FFA => 0,
        TeamMode::Teams => 1,
        TeamMode::Custom => 2,
    };
    spawn_overview_mode_row(
        commands,
        overview,
        "ENGAGEMENT FORMAT",
        &["FFA", "2V2", "CUSTOM"],
        team_idx,
        SelectorField::TeamMode,
        Some(1),
        theme,
    );

    let settings_panel = commands
        .spawn((
            Node {
                width: Val::Percent(37.0),
                min_height: Val::Px(640.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(16.0), Val::Px(18.0)),
                border: UiRect::left(Val::Px(1.0)),
                row_gap: Val::Px(18.0),
                ..default()
            },
            BackgroundColor(theme.colors.bg_surface.with_alpha(0.72)),
            BorderColor::all(theme.colors.separator.with_alpha(0.75)),
        ))
        .id();
    commands.entity(content).add_child(settings_panel);

    spawn_world_settings_panel(commands, settings_panel, config, fonts, theme);
}

fn spawn_new_game_top_bar(
    commands: &mut Commands,
    container: Entity,
    fonts: &UiFonts,
    theme: &Theme,
) {
    let bar = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Px(34.0),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::bottom(Val::Px(6.0)),
            border: UiRect::bottom(Val::Px(1.0)),
            ..default()
        })
        .with_children(|parent| {
            parent
                .spawn((
                    MenuButton(MenuAction::Back),
                    NavFocusable(0),
                    Button,
                    Node {
                        height: Val::Px(28.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(Val::Px(0.0), Val::Px(2.0)),
                        border: UiRect::bottom(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                    BorderColor::all(theme.colors.prestige.with_alpha(0.55)),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("BACK"),
                        fonts::body_emphasis(fonts, 13.0),
                        TextColor(theme.colors.prestige),
                        Pickable::IGNORE,
                    ));
                });

            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(10.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|icons| {
                    icons
                        .spawn((
                            MenuButton(MenuAction::Options),
                            Button,
                            Node {
                                padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(theme.colors.bg_menu.with_alpha(0.14)),
                            BorderColor::all(theme.colors.separator.with_alpha(0.6)),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("OPTIONS"),
                                TextFont {
                                    font_size: 9.0,
                                    ..default()
                                },
                                TextColor(theme.colors.text_secondary.with_alpha(0.9)),
                                Pickable::IGNORE,
                            ));
                        });

                    icons
                        .spawn((
                            MenuButton(MenuAction::Quit),
                            Button,
                            Node {
                                padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(theme.colors.bg_menu.with_alpha(0.14)),
                            BorderColor::all(theme.colors.separator.with_alpha(0.6)),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("QUIT"),
                                TextFont {
                                    font_size: 9.0,
                                    ..default()
                                },
                                TextColor(theme.colors.text_secondary.with_alpha(0.9)),
                                Pickable::IGNORE,
                            ));
                        });
                });
        })
        .id();
    commands.entity(container).add_child(bar);
}

fn spawn_new_game_overview_header(
    commands: &mut Commands,
    container: Entity,
    fonts: &UiFonts,
    theme: &Theme,
) {
    let header = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text::new("OVERVIEW"),
                fonts::heading(fonts, 42.0),
                TextColor(theme.colors.prestige.with_alpha(0.98)),
            ));

            parent.spawn((
                Node {
                    width: Val::Px(48.0),
                    height: Val::Px(2.0),
                    margin: UiRect::bottom(Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(theme.colors.prestige.with_alpha(0.95)),
            ));

            parent.spawn((
                Text::new("SELECT FACTION AND DEFINE OPPONENTS AND ALLIES"),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(theme.colors.text_disabled.with_alpha(0.95)),
            ));
        })
        .id();
    commands.entity(container).add_child(header);
}

pub(crate) fn spawn_command_profile_row(
    commands: &mut Commands,
    container: Entity,
    current_name: &str,
    theme: &Theme,
) {
    let row = commands
        .spawn(Node {
            width: Val::Px(420.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            margin: UiRect::top(Val::Px(14.0)),
            padding: UiRect::bottom(Val::Px(6.0)),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text::new("COMMAND PROFILE"),
                TextFont {
                    font_size: 9.0,
                    ..default()
                },
                TextColor(theme.colors.text_disabled.with_alpha(0.88)),
            ));

            parent
                .spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|content| {
                    content
                        .spawn((
                            TextInputField {
                                value: current_name.to_string(),
                                cursor_pos: current_name.len(),
                                selection_anchor: None,
                                max_len: 45,
                            },
                            Button,
                            Node {
                                width: Val::Px(290.0),
                                height: Val::Px(34.0),
                                padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                                align_items: AlignItems::Center,
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(theme.colors.bg_menu.with_alpha(0.72)),
                            BorderColor::all(theme.colors.separator.with_alpha(0.75)),
                        ))
                        .with_children(|input| {
                            crate::ui::core::text_input::spawn_text_input_children(
                                input,
                                current_name,
                                theme,
                            );
                        });

                    content
                        .spawn((
                            RandomNameButton,
                            Button,
                            Node {
                                height: Val::Px(34.0),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                padding: UiRect::horizontal(Val::Px(12.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BackgroundColor(theme.colors.bg_elevated.with_alpha(0.9)),
                            BorderColor::all(theme.colors.separator.with_alpha(0.75)),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new("RANDOMIZE"),
                                TextFont {
                                    font_size: 8.5,
                                    ..default()
                                },
                                TextColor(theme.colors.text_secondary),
                                Pickable::IGNORE,
                            ));
                        });
                });
        })
        .id();
    commands.entity(container).add_child(row);
}

fn spawn_overview_mode_row(
    commands: &mut Commands,
    container: Entity,
    label: &str,
    options: &[&str],
    selected: usize,
    field: SelectorField,
    nav_index: Option<usize>,
    theme: &Theme,
) {
    let mut row = commands.spawn((
        Node {
            width: Val::Px(380.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            margin: UiRect::top(Val::Px(12.0)),
            padding: UiRect::axes(Val::Px(0.0), Val::Px(10.0)),
            ..default()
        },
        BorderColor::all(Color::NONE),
    ));
    if let Some(idx) = nav_index {
        row.insert(NavFocusable(idx));
    }
    let entity = row
        .with_children(|parent| {
            parent.spawn((
                Text::new(label),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(theme.colors.text_disabled.with_alpha(0.9)),
            ));

            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|buttons| {
                    for (i, option) in options.iter().enumerate() {
                        let active = i == selected;
                        buttons
                            .spawn((
                                MenuSelector { field, index: i },
                                Button,
                                Node {
                                    min_width: Val::Px(72.0),
                                    height: Val::Px(26.0),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    padding: UiRect::horizontal(Val::Px(12.0)),
                                    border: UiRect::all(Val::Px(1.0)),
                                    ..default()
                                },
                                BackgroundColor(if active {
                                    theme.colors.accent.with_alpha(0.18)
                                } else {
                                    theme.colors.bg_menu.with_alpha(0.28)
                                }),
                                BorderColor::all(if active {
                                    theme.colors.accent.with_alpha(0.75)
                                } else {
                                    theme.colors.separator.with_alpha(0.6)
                                }),
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new(*option),
                                    TextFont {
                                        font_size: 10.0,
                                        ..default()
                                    },
                                    TextColor(if active {
                                        Color::srgb(0.10, 0.16, 0.18)
                                    } else {
                                        theme.colors.text_secondary.with_alpha(0.8)
                                    }),
                                    Pickable::IGNORE,
                                ));
                            });
                    }
                });
        })
        .id();
    commands.entity(container).add_child(entity);
}

fn spawn_world_settings_panel(
    commands: &mut Commands,
    container: Entity,
    config: &GameSetupConfig,
    fonts: &UiFonts,
    theme: &Theme,
) {
    let map_idx = match config.map_size {
        MapSize::Small => 0,
        MapSize::Medium => 1,
        MapSize::Large => 2,
        MapSize::ExtraLarge => 3,
    };
    let res_idx = match config.resource_density {
        ResourceDensity::Sparse => 0,
        ResourceDensity::Normal => 1,
        ResourceDensity::Dense => 2,
    };
    let start_idx = STARTING_RES_OPTIONS
        .iter()
        .position(|&(v, _)| (v - config.starting_resources_mult).abs() < 0.01)
        .unwrap_or(1);

    let panel = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(16.0),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text::new("GEOSPATIAL CONFIGURATION"),
                TextFont {
                    font_size: 8.0,
                    ..default()
                },
                TextColor(theme.colors.text_disabled.with_alpha(0.9)),
            ));

            parent.spawn((
                Text::new("WORLD SETTINGS"),
                fonts::heading(fonts, 24.0),
                TextColor(theme.colors.text_primary),
                Node {
                    margin: UiRect::bottom(Val::Px(6.0)),
                    ..default()
                },
            ));
        })
        .id();
    commands.entity(container).add_child(panel);

    spawn_settings_group_label(commands, container, "THEATER SCALE", theme);
    spawn_reference_segmented_selector(
        commands,
        container,
        &["SMALL", "MEDIUM", "LARGE", "EPIC"],
        map_idx,
        SelectorField::MapSize,
        Some(2),
        theme,
    );

    spawn_settings_group_label(commands, container, "RESOURCE DENSITY", theme);
    spawn_resource_density_block(commands, container, res_idx, theme);

    spawn_settings_group_label(commands, container, "STARTING ALLOCATION", theme);
    spawn_reference_segmented_selector(
        commands,
        container,
        &["0.5X", "1.0X", "2.0X"],
        start_idx,
        SelectorField::StartingRes,
        Some(5),
        theme,
    );

    spawn_settings_group_label(commands, container, "CHRONICLE SEED", theme);
    spawn_seed_block(commands, container, config, theme);

    let start_btn = commands
        .spawn((
            MenuButton(MenuAction::StartGame),
            NavFocusable(6),
            Button,
            ButtonAnimState::new([0.78, 0.87, 0.88, 1.0]),
            ButtonStyle::Filled,
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(68.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                margin: UiRect::top(Val::Px(26.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.79, 0.88, 0.89)),
            BorderColor::all(Color::srgba(0.79, 0.88, 0.89, 0.35)),
        ))
        .with_children(|parent| {
            parent.spawn((
                BeginCampaignText,
                Text::new("BEGIN CAMPAIGN  >"),
                fonts::heading(fonts, 20.0),
                TextColor(Color::srgb(0.17, 0.25, 0.28)),
                Pickable::IGNORE,
            ));
        })
        .id();
    commands.entity(container).add_child(start_btn);
}

fn spawn_settings_group_label(
    commands: &mut Commands,
    container: Entity,
    label: &str,
    theme: &Theme,
) {
    let entity = commands
        .spawn((
            Text::new(label),
            TextFont {
                font_size: 9.0,
                ..default()
            },
            TextColor(theme.colors.prestige.with_alpha(0.92)),
            Node {
                margin: UiRect::bottom(Val::Px(-4.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(entity);
}

fn spawn_reference_segmented_selector(
    commands: &mut Commands,
    container: Entity,
    options: &[&str],
    selected: usize,
    field: SelectorField,
    nav_index: Option<usize>,
    theme: &Theme,
) {
    let mut row = commands.spawn((
        Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(8.0),
            flex_wrap: FlexWrap::Wrap,
            padding: UiRect::bottom(Val::Px(6.0)),
            ..default()
        },
        BorderColor::all(Color::NONE),
    ));
    if let Some(idx) = nav_index {
        row.insert(NavFocusable(idx));
    }
    let entity = row
        .with_children(|parent| {
            for (i, option) in options.iter().enumerate() {
                let active = i == selected;
                parent
                    .spawn((
                        MenuSelector { field, index: i },
                        Button,
                        Node {
                            min_width: Val::Px(68.0),
                            height: Val::Px(34.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            padding: UiRect::horizontal(Val::Px(10.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(if active {
                            theme.colors.accent.with_alpha(0.18)
                        } else {
                            theme.colors.bg_menu.with_alpha(0.12)
                        }),
                        BorderColor::all(if active {
                            theme.colors.accent.with_alpha(0.82)
                        } else {
                            theme.colors.separator.with_alpha(0.75)
                        }),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new(*option),
                            TextFont {
                                font_size: 8.0,
                                ..default()
                            },
                            TextColor(if active {
                                Color::srgb(0.10, 0.16, 0.18)
                            } else {
                                theme.colors.text_secondary.with_alpha(0.72)
                            }),
                            Pickable::IGNORE,
                        ));
                    });
            }
        })
        .id();
    commands.entity(container).add_child(entity);
}

fn spawn_resource_density_block(
    commands: &mut Commands,
    container: Entity,
    selected: usize,
    theme: &Theme,
) {
    let (selected_label, pct) = resource_density_display(selected);
    let entity = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            padding: UiRect::bottom(Val::Px(6.0)),
            ..default()
        })
        .with_children(|parent| {
            parent
                .spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        ResourceDensityLabelText,
                        Text::new(selected_label),
                        TextFont {
                            font_size: 10.0,
                            ..default()
                        },
                        TextColor(theme.colors.text_primary.with_alpha(0.9)),
                    ));
                    row.spawn((
                        ResourceDensityValueText,
                        Text::new(format!("{pct:.0}%")),
                        TextFont {
                            font_size: 10.0,
                            ..default()
                        },
                        TextColor(theme.colors.text_secondary.with_alpha(0.8)),
                    ));
                });

            parent
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(8.0),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(theme.colors.bg_menu.with_alpha(0.4)),
                ))
                .with_children(|track| {
                    track.spawn((
                        ResourceDensityBarFill,
                        Node {
                            width: Val::Percent(pct),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(theme.colors.accent.with_alpha(0.82)),
                    ));
                });

            parent
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(10.0),
                        ..default()
                    },
                    BorderColor::all(Color::NONE),
                    NavFocusable(3),
                ))
                .with_children(|row| {
                    for (i, label) in ["SPARSE", "STANDARD", "DENSE"].iter().enumerate() {
                        let active = i == selected;
                        row.spawn((
                            MenuSelector {
                                field: SelectorField::ResourceDensity,
                                index: i,
                            },
                            Button,
                            Node {
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(6.0),
                                ..default()
                            },
                            BackgroundColor(Color::NONE),
                            BorderColor::all(Color::NONE),
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                ResourceDensityOptionDot(i),
                                Node {
                                    width: Val::Px(10.0),
                                    height: Val::Px(10.0),
                                    border: UiRect::all(Val::Px(1.0)),
                                    ..default()
                                },
                                BackgroundColor(if active {
                                    theme.colors.accent
                                } else {
                                    Color::NONE
                                }),
                                BorderColor::all(if active {
                                    theme.colors.accent
                                } else {
                                    theme.colors.text_secondary.with_alpha(0.6)
                                }),
                                Pickable::IGNORE,
                            ));
                            btn.spawn((
                                ResourceDensityOptionLabel(i),
                                Text::new(*label),
                                TextFont {
                                    font_size: 9.0,
                                    ..default()
                                },
                                TextColor(if active {
                                    theme.colors.text_primary
                                } else {
                                    theme.colors.text_secondary.with_alpha(0.7)
                                }),
                                Pickable::IGNORE,
                            ));
                        });
                    }
                });
        })
        .id();
    commands.entity(container).add_child(entity);
}

fn resource_density_display(selected: usize) -> (String, f32) {
    let pct = match selected {
        0 => 35.0,
        1 => 63.0,
        _ => 88.0,
    };
    let label = ["Sparse", "Standard", "Dense"]
        .get(selected)
        .copied()
        .unwrap_or("Standard")
        .to_uppercase();
    (label, pct)
}

pub(crate) fn sync_resource_density_block(
    config: Res<GameSetupConfig>,
    mut label_texts: Query<
        &mut Text,
        (
            With<ResourceDensityLabelText>,
            Without<ResourceDensityValueText>,
        ),
    >,
    mut value_texts: Query<
        &mut Text,
        (
            With<ResourceDensityValueText>,
            Without<ResourceDensityLabelText>,
        ),
    >,
    mut fill_nodes: Query<&mut Node, With<ResourceDensityBarFill>>,
    mut dots: Query<(
        &ResourceDensityOptionDot,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    mut option_labels: Query<
        (&ResourceDensityOptionLabel, &mut TextColor),
        Without<ResourceDensityLabelText>,
    >,
    theme: Res<Theme>,
) {
    let selected = match config.resource_density {
        ResourceDensity::Sparse => 0,
        ResourceDensity::Normal => 1,
        ResourceDensity::Dense => 2,
    };
    let (label, pct) = resource_density_display(selected);

    for mut text in &mut label_texts {
        **text = label.clone();
    }
    for mut text in &mut value_texts {
        **text = format!("{pct:.0}%");
    }
    for mut node in &mut fill_nodes {
        node.width = Val::Percent(pct);
    }
    for (dot, mut bg, mut border) in &mut dots {
        let active = dot.0 == selected;
        *bg = BackgroundColor(if active {
            theme.colors.accent
        } else {
            Color::NONE
        });
        *border = BorderColor::all(if active {
            theme.colors.accent
        } else {
            theme.colors.text_secondary.with_alpha(0.6)
        });
    }
    for (option, mut color) in &mut option_labels {
        color.0 = if option.0 == selected {
            theme.colors.text_primary
        } else {
            theme.colors.text_secondary.with_alpha(0.7)
        };
    }
}

fn spawn_seed_block(
    commands: &mut Commands,
    container: Entity,
    config: &GameSetupConfig,
    theme: &Theme,
) {
    let seed_text = if config.map_seed == 0 {
        String::new()
    } else {
        format!("{}", config.map_seed)
    };

    let entity = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            },
            BorderColor::all(Color::NONE),
            NavFocusable(5),
        ))
        .with_children(|parent| {
            parent
                .spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        SeedInput,
                        TextInputField {
                            value: seed_text.clone(),
                            cursor_pos: seed_text.len(),
                            selection_anchor: None,
                            max_len: 20,
                        },
                        Button,
                        Node {
                            flex_grow: 1.0,
                            height: Val::Px(40.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            padding: UiRect::horizontal(Val::Px(12.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(theme.colors.bg_menu.with_alpha(0.78)),
                        BorderColor::all(theme.colors.separator.with_alpha(0.8)),
                    ))
                    .with_children(|input| {
                        crate::ui::core::text_input::spawn_text_input_children(
                            input, &seed_text, theme,
                        );
                    });

                    row.spawn((
                        RandomizeSeedButton,
                        Button,
                        ButtonAnimState::new(theme.colors.bg_elevated.to_srgba().to_f32_array()),
                        ButtonStyle::Filled,
                        Node {
                            width: Val::Px(40.0),
                            height: Val::Px(40.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(theme.colors.bg_elevated),
                        BorderColor::all(theme.colors.separator.with_alpha(0.8)),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("R"),
                            TextFont {
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(theme.colors.text_secondary),
                            Pickable::IGNORE,
                        ));
                    });
                });

            parent.spawn((
                Text::new("TYPE A SEED OR LEAVE EMPTY FOR RANDOM — PRESS R TO ROLL"),
                TextFont {
                    font_size: 6.5,
                    ..default()
                },
                TextColor(theme.colors.text_disabled.with_alpha(0.55)),
            ));
        })
        .id();
    commands.entity(container).add_child(entity);
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
    lobby_players: Option<&[crate::infrastructure::multiplayer::LobbyPlayer]>,
    nav_rows: Option<(usize, usize, usize)>,
    theme: &Theme,
) {
    let slot = config.slots[slot_index];
    let (type_nav, difficulty_nav, team_nav) = nav_rows
        .map(|(type_nav, difficulty_nav, team_nav)| {
            (Some(type_nav), Some(difficulty_nav), Some(team_nav))
        })
        .unwrap_or((None, None, None));
    let is_you = is_multiplayer
        && slot_index == config.local_player_slot
        && matches!(slot, SlotOccupant::Human);
    let is_local = slot_index == config.local_player_slot && matches!(slot, SlotOccupant::Human);

    let (type_options, type_idx) = if is_multiplayer {
        (
            vec!["HUMAN", "OPEN", "AI", "NONE"],
            match slot {
                SlotOccupant::Human => 0,
                SlotOccupant::Open => 1,
                SlotOccupant::Ai(_) => 2,
                SlotOccupant::Closed => 3,
            },
        )
    } else {
        (
            vec!["HUMAN", "AI", "NONE"],
            match slot {
                SlotOccupant::Human => 0,
                SlotOccupant::Ai(_) => 1,
                SlotOccupant::Closed | SlotOccupant::Open => 2,
            },
        )
    };

    let card_border = if is_local || is_you {
        theme.colors.accent.with_alpha(0.95)
    } else {
        theme.colors.separator.with_alpha(0.65)
    };
    let card_bg = if is_local || is_you {
        theme.colors.bg_surface.with_alpha(0.96)
    } else if matches!(slot, SlotOccupant::Closed) {
        theme.colors.bg_menu.with_alpha(0.48)
    } else {
        theme.colors.bg_surface.with_alpha(0.82)
    };
    let slot_meta = match slot_index {
        0 => "COMMANDER",
        1 => "VANGUARD",
        2 => "SUPPORT",
        _ => "RESERVE",
    };
    let commander_name = match slot {
        SlotOccupant::Human if is_local => config.player_name.clone(),
        SlotOccupant::Human => {
            let faction = Faction::PLAYERS[slot_index];
            lobby_players
                .and_then(|players| players.iter().find(|p| p.connected && p.faction == faction))
                .map(|p| p.name.clone())
                .unwrap_or_else(|| format!("Connected Player {}", slot_index + 1))
        }
        SlotOccupant::Ai(_) => config.ai_names[slot_index].clone(),
        SlotOccupant::Open => "Open Multiplayer Slot".to_string(),
        SlotOccupant::Closed => "Inactive Slot".to_string(),
    };
    let faction_name = Faction::PLAYERS[slot_index].display_name().to_uppercase();
    let state_badge = match slot {
        SlotOccupant::Human if is_local => "LOCAL PLAYER",
        SlotOccupant::Human => "PLAYER",
        SlotOccupant::Ai(AiDifficulty::Easy) => "AI · EASY",
        SlotOccupant::Ai(AiDifficulty::Medium) => "AI · STANDARD",
        SlotOccupant::Ai(AiDifficulty::Hard) => "AI · HARD",
        SlotOccupant::Open => "WAITING FOR PLAYER",
        SlotOccupant::Closed => "DISABLED",
    };
    let icon_label = match slot {
        SlotOccupant::Human => match slot_index {
            0 => "P1",
            1 => "P2",
            2 => "P3",
            _ => "P4",
        },
        SlotOccupant::Ai(_) => "AI",
        SlotOccupant::Open => "OP",
        SlotOccupant::Closed => "+",
    };
    let faction_tint = if matches!(slot, SlotOccupant::Closed) {
        theme.colors.text_disabled.with_alpha(0.5)
    } else if is_local || is_you {
        theme.colors.prestige
    } else {
        theme.colors.text_primary.with_alpha(0.72)
    };

    let card = commands
        .spawn((
            SlotCardContainer(slot_index),
            Node {
                width: Val::Percent(100.0),
                min_height: Val::Px(96.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                padding: UiRect::axes(Val::Px(10.0), Val::Px(10.0)),
                border: UiRect::all(Val::Px(1.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(card_bg),
            BorderColor::all(card_border),
        ))
        .with_children(|card| {
            if is_local || is_you {
                card.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        bottom: Val::Px(0.0),
                        width: Val::Px(2.0),
                        ..default()
                    },
                    BackgroundColor(theme.colors.accent.with_alpha(0.95)),
                    Pickable::IGNORE,
                ));
            }

            card.spawn(Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(12.0),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Node {
                        width: Val::Px(42.0),
                        height: Val::Px(42.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(theme.colors.bg_elevated.with_alpha(0.9)),
                    BorderColor::all(theme.colors.separator.with_alpha(0.8)),
                ))
                .with_children(|icon| {
                    icon.spawn((
                        Text::new(icon_label),
                        TextFont {
                            font_size: 11.0,
                            ..default()
                        },
                        TextColor(if matches!(slot, SlotOccupant::Closed) {
                            theme.colors.text_disabled.with_alpha(0.6)
                        } else {
                            theme.colors.text_secondary
                        }),
                        Pickable::IGNORE,
                    ));
                });

                row.spawn(Node {
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    ..default()
                })
                .with_children(|info| {
                    info.spawn((
                        Text::new(format!("SLOT {:02} · {}", slot_index + 1, slot_meta)),
                        TextFont {
                            font_size: 7.5,
                            ..default()
                        },
                        TextColor(theme.colors.text_disabled.with_alpha(0.8)),
                    ));

                    info.spawn((
                        Text::new(commander_name),
                        TextFont {
                            font_size: 21.0,
                            ..default()
                        },
                        TextColor(if matches!(slot, SlotOccupant::Closed) {
                            theme.colors.text_disabled.with_alpha(0.7)
                        } else {
                            theme.colors.text_primary
                        }),
                    ));

                    let mut type_row = info.spawn((
                        Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(6.0),
                            flex_wrap: FlexWrap::Wrap,
                            padding: UiRect::vertical(Val::Px(1.0)),
                            border: UiRect::left(Val::Px(2.0)),
                            ..default()
                        },
                        BorderColor::all(Color::NONE),
                    ));
                    if let Some(idx) = type_nav {
                        type_row.insert(NavFocusable(idx));
                    }
                    type_row.with_children(|tags| {
                        for (i, opt) in type_options.iter().enumerate() {
                            let active = i == type_idx;
                            tags.spawn((
                                MenuSelector {
                                    field: SelectorField::SlotType(slot_index),
                                    index: i,
                                },
                                Button,
                                Node {
                                    min_width: Val::Px(if active { 54.0 } else { 34.0 }),
                                    height: Val::Px(18.0),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    padding: UiRect::horizontal(Val::Px(7.0)),
                                    border: UiRect::all(Val::Px(1.0)),
                                    ..default()
                                },
                                BackgroundColor(if active {
                                    if *opt == "HUMAN" {
                                        theme.colors.accent.with_alpha(0.9)
                                    } else {
                                        theme.colors.bg_elevated.with_alpha(0.9)
                                    }
                                } else {
                                    Color::NONE
                                }),
                                BorderColor::all(if active {
                                    if *opt == "HUMAN" {
                                        theme.colors.accent
                                    } else {
                                        theme.colors.separator.with_alpha(0.95)
                                    }
                                } else {
                                    theme.colors.separator.with_alpha(0.2)
                                }),
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    Text::new(*opt),
                                    TextFont {
                                        font_size: 7.5,
                                        ..default()
                                    },
                                    TextColor(if active && *opt == "HUMAN" {
                                        Color::srgb(0.10, 0.18, 0.20)
                                    } else if active {
                                        Color::srgb(0.10, 0.16, 0.18)
                                    } else {
                                        theme.colors.text_disabled.with_alpha(0.48)
                                    }),
                                    Pickable::IGNORE,
                                ));
                            });
                        }
                    });

                    if let SlotOccupant::Ai(difficulty) = slot {
                        let diff_idx = match difficulty {
                            AiDifficulty::Easy => 0,
                            AiDifficulty::Medium => 1,
                            AiDifficulty::Hard => 2,
                        };
                        let mut diff_row = info.spawn((
                            Node {
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(6.0),
                                flex_wrap: FlexWrap::Wrap,
                                padding: UiRect::vertical(Val::Px(1.0)),
                                border: UiRect::left(Val::Px(2.0)),
                                ..default()
                            },
                            BorderColor::all(Color::NONE),
                        ));
                        if let Some(idx) = difficulty_nav {
                            diff_row.insert(NavFocusable(idx));
                        }
                        diff_row.with_children(|tags| {
                            for (i, opt) in ["EASY", "STANDARD", "HARD"].iter().enumerate() {
                                let active = i == diff_idx;
                                tags.spawn((
                                    MenuSelector {
                                        field: SelectorField::SlotDifficulty(slot_index),
                                        index: i,
                                    },
                                    Button,
                                    Node {
                                        min_width: Val::Px(if active { 60.0 } else { 38.0 }),
                                        height: Val::Px(18.0),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        padding: UiRect::horizontal(Val::Px(7.0)),
                                        border: UiRect::all(Val::Px(1.0)),
                                        ..default()
                                    },
                                    BackgroundColor(if active {
                                        theme.colors.bg_elevated.with_alpha(0.95)
                                    } else {
                                        Color::NONE
                                    }),
                                    BorderColor::all(if active {
                                        theme.colors.separator.with_alpha(0.85)
                                    } else {
                                        theme.colors.separator.with_alpha(0.2)
                                    }),
                                ))
                                .with_children(|btn| {
                                    btn.spawn((
                                        Text::new(*opt),
                                        TextFont {
                                            font_size: 7.5,
                                            ..default()
                                        },
                                        TextColor(if active {
                                            Color::srgb(0.10, 0.16, 0.18)
                                        } else {
                                            theme.colors.text_disabled.with_alpha(0.4)
                                        }),
                                        Pickable::IGNORE,
                                    ));
                                });
                            }
                        });
                    } else {
                        info.spawn((
                            Text::new(state_badge),
                            TextFont {
                                font_size: 8.0,
                                ..default()
                            },
                            TextColor(theme.colors.text_disabled.with_alpha(0.7)),
                        ));
                    }
                });

                row.spawn(Node {
                    width: Val::Px(144.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexEnd,
                    row_gap: Val::Px(5.0),
                    ..default()
                })
                .with_children(|right| {
                    right.spawn((
                        Text::new("FACTION"),
                        TextFont {
                            font_size: 7.0,
                            ..default()
                        },
                        TextColor(theme.colors.text_disabled.with_alpha(0.58)),
                    ));

                    right.spawn((
                        Text::new(faction_name),
                        TextFont {
                            font_size: 15.0,
                            ..default()
                        },
                        TextColor(faction_tint),
                    ));

                    if !matches!(slot, SlotOccupant::Closed | SlotOccupant::Open) {
                        let mut team_row = right.spawn((
                            Node {
                                flex_direction: FlexDirection::Row,
                                column_gap: Val::Px(5.0),
                                margin: UiRect::top(Val::Px(2.0)),
                                padding: UiRect::left(Val::Px(4.0)),
                                border: UiRect::left(Val::Px(2.0)),
                                ..default()
                            },
                            BorderColor::all(Color::NONE),
                        ));
                        if let Some(idx) = team_nav {
                            team_row.insert(NavFocusable(idx));
                        }
                        team_row
                            .with_children(|teams| {
                                let current_team = config.player_teams[slot_index] as usize;
                                for ti in 0..4 {
                                    let active = ti == current_team;
                                    teams.spawn((
                                        MenuSelector {
                                            field: SelectorField::SlotTeam(slot_index),
                                            index: ti,
                                        },
                                        Button,
                                        Node {
                                            width: Val::Px(13.0),
                                            height: Val::Px(13.0),
                                            border: UiRect::all(Val::Px(1.0)),
                                            ..default()
                                        },
                                        BackgroundColor(if active {
                                            TEAM_COLORS[ti]
                                        } else {
                                            Color::NONE
                                        }),
                                        BorderColor::all(if active {
                                            TEAM_COLORS[ti]
                                        } else {
                                            theme.colors.separator.with_alpha(0.45)
                                        }),
                                    ));
                                }
                            });
                    }
                });

                if is_multiplayer && !is_local && matches!(slot, SlotOccupant::Human) {
                    row.spawn((
                        super::KickPlayerButton(slot_index),
                        Button,
                        Node {
                            width: Val::Px(20.0),
                            height: Val::Px(20.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(Val::Px(1.0)),
                            margin: UiRect::left(Val::Px(6.0)),
                            ..default()
                        },
                        BackgroundColor(theme.colors.destructive.with_alpha(0.12)),
                        BorderColor::all(theme.colors.destructive.with_alpha(0.35)),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("X"),
                            TextFont {
                                font_size: 8.0,
                                ..default()
                            },
                            TextColor(theme.colors.destructive),
                            Pickable::IGNORE,
                        ));
                    });
                }
            });
        })
        .id();
    commands.entity(container).add_child(card);
}
