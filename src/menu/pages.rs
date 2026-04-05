use bevy::prelude::*;

use super::helpers::*;
use crate::components::*;
use crate::database::{ActiveProfile, GameDatabase, SaveEntry};
use crate::theme::{self, Theme};
use crate::ui::core::components as ui_components;
use crate::ui::fonts::{self, UiFonts};

#[allow(unused_imports)]
use super::*;

// ── Title Page ──

pub(crate) fn spawn_title_page(
    commands: &mut Commands,
    container: Entity,
    fonts: &UiFonts,
    theme: &Theme,
) {
    for (i, (label, action)) in [
        ("NEW GAME", MenuAction::NewGame),
        ("LOAD GAME", MenuAction::LoadGame),
        ("MULTIPLAYER", MenuAction::Multiplayer),
        ("OPTIONS", MenuAction::Options),
        ("QUIT", MenuAction::Quit),
    ]
    .iter()
    .enumerate()
    {
        let btn = spawn_styled_button_nav(
            commands,
            label,
            MenuButton(*action),
            false,
            fonts,
            Some(i),
            theme,
        );
        commands.entity(container).add_child(btn);
    }

    // Controls hint
    spawn_controls_hint(commands, container, fonts, theme);

    let ver = commands
        .spawn((
            Text::new("v0.1"),
            fonts::body(fonts, theme.typography.body),
            TextColor(theme.colors.text_secondary),
            Node {
                margin: UiRect::top(Val::Px(20.0)),
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
    theme: &Theme,
) {
    spawn_page_header(
        commands,
        container,
        "NEW GAME",
        MenuButton(MenuAction::Back),
        fonts,
        theme,
    );

    spawn_animated_section_divider(commands, container, "PLAYER", fonts, theme);

    let name_row = spawn_name_input_row(commands, &config.player_name, theme);
    commands.entity(container).add_child(name_row);

    spawn_animated_section_divider(commands, container, "FACTIONS", fonts, theme);

    let slots_wrap = commands
        .spawn((
            SlotCardsContainer,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(slots_wrap);

    for i in 0..4 {
        spawn_slot_card(commands, slots_wrap, i, config, false, theme);
    }

    let team_idx = match config.team_mode {
        TeamMode::FFA => 0,
        TeamMode::Teams => 1,
        TeamMode::Custom => 2,
    };
    spawn_selector_row_nav(
        commands,
        container,
        "Teams:",
        &["FFA", "2v2", "Custom"],
        team_idx,
        SelectorField::TeamMode,
        Some(1),
        theme,
    );

    spawn_animated_section_divider(commands, container, "WORLD", fonts, theme);

    let map_idx = match config.map_size {
        MapSize::Small => 0,
        MapSize::Medium => 1,
        MapSize::Large => 2,
    };
    spawn_selector_row_nav(
        commands,
        container,
        "Map Size:",
        &["Small", "Medium", "Large"],
        map_idx,
        SelectorField::MapSize,
        Some(2),
        theme,
    );

    let res_idx = match config.resource_density {
        ResourceDensity::Sparse => 0,
        ResourceDensity::Normal => 1,
        ResourceDensity::Dense => 2,
    };
    spawn_selector_row_nav(
        commands,
        container,
        "Resources:",
        &["Sparse", "Normal", "Dense"],
        res_idx,
        SelectorField::ResourceDensity,
        Some(3),
        theme,
    );

    let day_idx = DAY_CYCLE_OPTIONS
        .iter()
        .position(|&(v, _)| (v - config.day_cycle_secs).abs() < 1.0)
        .unwrap_or(1);
    let day_labels: Vec<&str> = DAY_CYCLE_OPTIONS.iter().map(|&(_, l)| l).collect();
    spawn_selector_row_nav(
        commands,
        container,
        "Day Cycle:",
        &day_labels,
        day_idx,
        SelectorField::DayCycle,
        Some(4),
        theme,
    );

    let start_idx = STARTING_RES_OPTIONS
        .iter()
        .position(|&(v, _)| (v - config.starting_resources_mult).abs() < 0.01)
        .unwrap_or(1);
    let start_labels: Vec<&str> = STARTING_RES_OPTIONS.iter().map(|&(_, l)| l).collect();
    spawn_selector_row_nav(
        commands,
        container,
        "Start Res:",
        &start_labels,
        start_idx,
        SelectorField::StartingRes,
        Some(5),
        theme,
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
                    font_size: theme.typography.medium,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
                Node {
                    width: Val::Px(120.0),
                    ..default()
                },
            ));
            parent.spawn((
                SeedDisplay,
                Text::new(seed_text),
                TextFont {
                    font_size: theme.typography.medium,
                    ..default()
                },
                TextColor(theme.colors.text_primary),
                Node {
                    width: Val::Px(140.0),
                    ..default()
                },
            ));
            parent
                .spawn((
                    RandomizeSeedButton,
                    Button,
                    ButtonAnimState::new(theme.colors.btn_primary.to_srgba().to_f32_array()),
                    ButtonStyle::Filled,
                    Node {
                        padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                        margin: UiRect::horizontal(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(theme.colors.btn_primary),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("Randomize"),
                        TextFont {
                            font_size: theme.typography.medium,
                            ..default()
                        },
                        TextColor(theme.colors.text_secondary),
                        Pickable::IGNORE,
                    ));
                });
        })
        .id();
    commands.entity(container).add_child(seed_row);

    // Start Game button
    let start_btn = commands
        .spawn((
            MenuButton(MenuAction::StartGame),
            NavFocusable(6),
            Button,
            ButtonAnimState::new(theme.colors.accent.to_srgba().to_f32_array()),
            ButtonStyle::Filled,
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
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(theme.colors.accent),
            BorderColor::all(Color::NONE),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("START GAME"),
                fonts::heading(fonts, theme.typography.button),
                TextColor(Color::WHITE),
                Pickable::IGNORE,
            ));
        })
        .id();
    commands.entity(container).add_child(start_btn);
}

// ── Options Page (delegated to options.rs) ──

pub(crate) use super::options::spawn_options_page;

// ── Unified Slot Card ──

/// Spawn a faction slot card (used in both NewGame and HostLobby).
/// `is_multiplayer` controls whether "Open" is offered as a slot type.
pub(crate) fn spawn_slot_card(
    commands: &mut Commands,
    container: Entity,
    slot_index: usize,
    config: &GameSetupConfig,
    is_multiplayer: bool,
    theme: &Theme,
) {
    let slot = config.slots[slot_index];
    let faction_color = Faction::PLAYERS[slot_index].color();
    let is_you = is_multiplayer
        && slot_index == config.local_player_slot
        && matches!(slot, SlotOccupant::Human);
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
        theme.colors.accent
    } else {
        theme.colors.separator
    };

    let card = commands
        .spawn((
            SlotCardContainer(slot_index),
            ui_components::card_node(),
            ui_components::card_chrome(theme, border_col),
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
                    {
                        let mut node = ui_components::badge_node(20.0, 10.0);
                        node.margin = UiRect::right(Val::Px(6.0));
                        node
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
                        font_size: theme.typography.medium,
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
                    let text_color = if is_selected {
                        Color::WHITE
                    } else {
                        theme.colors.text_secondary
                    };

                    let mut btn = row.spawn((
                        MenuSelector {
                            field: SelectorField::SlotType(slot_index),
                            index: i,
                        },
                        Button,
                        ui_components::compact_button_node_with_margin(10.0, 5.0, 1.0),
                        if is_selected {
                            ui_components::filled_button_chrome(
                                theme,
                                ui_components::UiTone::Accent,
                            )
                        } else {
                            ui_components::filled_button_chrome(
                                theme,
                                ui_components::UiTone::Neutral,
                            )
                        },
                    ));
                    if is_selected {
                        btn.insert(SelectedOption);
                    }
                    btn.with_children(|btn_parent| {
                        btn_parent.spawn((
                            Text::new(opt),
                            TextFont {
                                font_size: theme.typography.small,
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
                        Color::srgb(0.2, 0.75, 0.85), // T2: Cyan
                        Color::srgb(0.85, 0.3, 0.65), // T3: Pink
                        Color::srgb(0.95, 0.5, 0.15), // T4: Orange
                    ];
                    for ti in 0..4 {
                        let is_sel = ti == current_team;
                        let color = team_colors[ti];
                        let size = if is_sel { 24.0 } else { 20.0 };
                        let border_color = if is_sel { Color::WHITE } else { Color::NONE };
                        let mut dot = row.spawn((
                            MenuSelector {
                                field: SelectorField::SlotTeam(slot_index),
                                index: ti,
                            },
                            Button,
                            {
                                let mut node = ui_components::badge_node(size, 4.0);
                                node.margin = UiRect::horizontal(Val::Px(2.0));
                                node.border = UiRect::all(Val::Px(if is_sel { 2.0 } else { 1.0 }));
                                node
                            },
                            BackgroundColor(if is_sel {
                                color
                            } else {
                                Color::srgba(0.15, 0.15, 0.15, 0.8)
                            }),
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
                        {
                            let mut node = ui_components::icon_button_node(24.0);
                            node.margin = UiRect::left(Val::Px(4.0));
                            node
                        },
                        ui_components::filled_button_chrome(
                            theme,
                            ui_components::UiTone::Destructive,
                        ),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("X"),
                            TextFont {
                                font_size: theme.typography.tiny,
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
                            font_size: theme.typography.medium,
                            ..default()
                        },
                        TextColor(theme.colors.text_secondary),
                        Node {
                            width: Val::Px(80.0),
                            margin: UiRect::right(Val::Px(10.0)),
                            ..default()
                        },
                    ));

                    let diff_names = ["Easy", "Medium", "Hard"];
                    for (i, &opt) in diff_names.iter().enumerate() {
                        let is_selected = i == diff_idx;
                        let text_color = if is_selected {
                            Color::WHITE
                        } else {
                            theme.colors.text_secondary
                        };

                        let mut btn = row.spawn((
                            MenuSelector {
                                field: SelectorField::SlotDifficulty(slot_index),
                                index: i,
                            },
                            Button,
                            ui_components::compact_button_node_with_margin(14.0, 7.0, 2.0),
                            if is_selected {
                                ui_components::filled_button_chrome(
                                    theme,
                                    ui_components::UiTone::Accent,
                                )
                            } else {
                                ui_components::filled_button_chrome(
                                    theme,
                                    ui_components::UiTone::Neutral,
                                )
                            },
                        ));
                        if is_selected {
                            btn.insert(SelectedOption);
                        }
                        btn.with_children(|btn_parent| {
                            btn_parent.spawn((
                                Text::new(opt),
                                TextFont {
                                    font_size: theme.typography.medium,
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

// ── Load Game Page ──

pub(crate) fn spawn_load_game_page(
    commands: &mut Commands,
    container: Entity,
    fonts: &UiFonts,
    theme: &Theme,
    db: &GameDatabase,
    profile: &ActiveProfile,
) {
    // Title
    let title = commands
        .spawn((
            Text::new("LOAD GAME"),
            TextFont {
                font: fonts.heading.clone(),
                font_size: theme.typography.display,
                ..default()
            },
            TextColor(theme.colors.text_primary),
            Node {
                margin: UiRect::bottom(Val::Px(24.0)),
                ..default()
            },
        ))
        .id();
    commands.entity(container).add_child(title);

    let saves = db.list_saves(&profile.id);

    if saves.is_empty() {
        let empty_text = commands
            .spawn((
                Text::new("No saved games found."),
                TextFont {
                    font: fonts.body.clone(),
                    font_size: theme.typography.body,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
                Node {
                    margin: UiRect::bottom(Val::Px(24.0)),
                    ..default()
                },
            ))
            .id();
        commands.entity(container).add_child(empty_text);
    } else {
        // Scrollable save list
        let list_container = commands
            .spawn(Node {
                width: Val::Percent(100.0),
                max_height: Val::Px(400.0),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
                margin: UiRect::bottom(Val::Px(16.0)),
                row_gap: Val::Px(8.0),
                ..default()
            })
            .id();
        commands.entity(container).add_child(list_container);

        for (i, save) in saves.iter().enumerate() {
            let row = spawn_save_entry_row(commands, save, i, fonts, theme);
            commands.entity(list_container).add_child(row);
        }
    }

    // Back button
    let back_btn = spawn_styled_button_nav(
        commands,
        "BACK",
        MenuButton(MenuAction::Back),
        false,
        fonts,
        Some(saves.len()),
        theme,
    );
    commands.entity(container).add_child(back_btn);
}

fn spawn_save_entry_row(
    commands: &mut Commands,
    save: &SaveEntry,
    index: usize,
    fonts: &UiFonts,
    theme: &Theme,
) -> Entity {
    let label = save
        .label
        .clone()
        .unwrap_or_else(|| format!("Save #{}", save.id));

    let elapsed_mins = (save.elapsed_secs / 60.0) as u32;
    let elapsed_secs = (save.elapsed_secs % 60.0) as u32;
    let elapsed_str = format!("{elapsed_mins}:{elapsed_secs:02}");

    let row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            padding: UiRect::all(Val::Px(8.0)),
            column_gap: Val::Px(12.0),
            ..default()
        })
        .id();

    // Load button (the main row)
    let load_btn = commands
        .spawn((
            MenuButton(MenuAction::LoadSave(save.id)),
            Button,
            ButtonAnimState::new(theme.colors.btn_primary.to_srgba().to_f32_array()),
            ButtonStyle::Filled,
            NavFocusable(index),
            Node {
                flex_grow: 1.0,
                height: Val::Px(52.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
                column_gap: Val::Px(16.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(theme.colors.btn_primary),
            BorderColor::all(theme.colors.separator),
        ))
        .with_children(|parent| {
            // Label
            parent.spawn((
                Text::new(&label),
                TextFont {
                    font: fonts.body_emphasis.clone(),
                    font_size: theme.typography.body,
                    ..default()
                },
                TextColor(theme.colors.text_primary),
                Pickable::IGNORE,
                Node {
                    width: Val::Px(160.0),
                    ..default()
                },
            ));
            // Map size
            parent.spawn((
                Text::new(&save.map_size),
                TextFont {
                    font: fonts.body.clone(),
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
                Pickable::IGNORE,
                Node {
                    width: Val::Px(80.0),
                    ..default()
                },
            ));
            // Elapsed time
            parent.spawn((
                Text::new(&elapsed_str),
                TextFont {
                    font: fonts.body.clone(),
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
                Pickable::IGNORE,
                Node {
                    width: Val::Px(60.0),
                    ..default()
                },
            ));
            // Date
            parent.spawn((
                Text::new(&save.created_at),
                TextFont {
                    font: fonts.body.clone(),
                    font_size: theme.typography.tiny,
                    ..default()
                },
                TextColor(theme.colors.text_disabled),
                Pickable::IGNORE,
            ));
        })
        .id();

    commands.entity(row).add_child(load_btn);

    // Delete button
    let del_btn = commands
        .spawn((
            MenuButton(MenuAction::DeleteSave(save.id)),
            Button,
            ButtonAnimState::new(theme.colors.destructive.to_srgba().to_f32_array()),
            ButtonStyle::Filled,
            Node {
                width: Val::Px(36.0),
                height: Val::Px(36.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(theme.colors.destructive.with_alpha(0.15)),
            BorderColor::all(theme.colors.destructive.with_alpha(0.3)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("X"),
                TextFont {
                    font: fonts.body_emphasis.clone(),
                    font_size: theme.typography.small,
                    ..default()
                },
                TextColor(theme.colors.destructive),
                Pickable::IGNORE,
            ));
        })
        .id();

    commands.entity(row).add_child(del_btn);

    row
}
