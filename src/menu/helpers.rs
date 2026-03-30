//! Menu-specific UI spawn helpers: panels, buttons, selectors, input rows.

use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;

use crate::components::*;
use crate::theme;
use crate::ui::core::components as ui_components;
use crate::ui::core::fonts::{self, UiFonts};
use crate::ui::core::text_input::{spawn_text_input_children, ScrollablePanel};

// ── Shared Components ──

#[derive(Component, Clone, Copy)]
pub struct MenuSelector {
    pub field: SelectorField,
    pub index: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SelectorField {
    SlotType(usize),
    SlotDifficulty(usize),
    SlotTeam(usize),
    TeamMode,
    MapSize,
    ResourceDensity,
    DayCycle,
    StartingRes,
    MapSeed,
    Resolution,
    Fullscreen,
    Vsync,
    Shadows,
    EntityLights,
    AntiAliasing,
    Bloom,
    Brightness,
    AutoExposure,
    DepthOfField,
    ChromaticAberration,
    UiScale,
    MusicVolume,
    SfxVolume,
    PreferredFaction,
}

#[derive(Component)]
pub struct SelectedOption;

#[derive(Component)]
pub struct SeedDisplay;

#[derive(Component)]
pub struct RandomizeSeedButton;

/// Wrapper around the 4 slot cards so they can be rebuilt without
/// tearing down the entire menu page (avoids replaying panel animations).
#[derive(Component)]
pub struct SlotCardsContainer;

// ── Panel ──

pub fn spawn_menu_panel(commands: &mut Commands) -> Entity {
    commands
        .spawn((
            ScrollablePanel,
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
                ..default()
            },
            BackgroundColor(Color::srgba(0.07, 0.07, 0.07, 0.0)),
            BorderColor::all(theme::SEPARATOR),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.6),
                Val::Px(0.0),
                Val::Px(4.0),
                Val::Px(0.0),
                Val::Px(8.0),
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

// ── Styled Button ──

pub fn spawn_styled_button(
    commands: &mut Commands,
    label: &str,
    marker: impl Bundle,
    accent: bool,
    fonts: &UiFonts,
) -> Entity {
    spawn_styled_button_nav(commands, label, marker, accent, fonts, None)
}

pub fn spawn_styled_button_nav(
    commands: &mut Commands,
    label: &str,
    marker: impl Bundle,
    accent: bool,
    fonts: &UiFonts,
    nav_index: Option<usize>,
) -> Entity {
    let bg = if accent {
        ui_components::UiTone::Accent
    } else {
        ui_components::UiTone::Neutral
    };

    let mut node = ui_components::button_node(240.0, 44.0);
    node.border = UiRect::all(Val::Px(2.0));

    let mut entity_commands = commands.spawn((
        marker,
        Button,
        node,
        ui_components::filled_button_chrome(bg),
    ));
    if let Some(idx) = nav_index {
        entity_commands.insert(NavFocusable(idx));
    }
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

/// Spawns a controls hint row at the bottom-left of the screen.
pub fn spawn_controls_hint(commands: &mut Commands, container: Entity, fonts: &UiFonts) {
    let hint = commands
        .spawn((
            ControlsHint,
            Node {
                width: Val::Percent(100.0),
                margin: UiRect::top(Val::Px(20.0)),
                justify_content: JustifyContent::Start,
                align_items: AlignItems::Center,
                column_gap: Val::Px(16.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            let items = [
                ("W/S", "Navigate"),
                ("A/D", "Change"),
                ("Enter", "Confirm"),
                ("Esc", "Back"),
            ];
            for (key, action) in items {
                parent
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(4.0),
                        ..default()
                    })
                    .with_children(|row| {
                        // Key badge
                        row.spawn((
                            Node {
                                padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                border_radius: BorderRadius::all(Val::Px(3.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.9)),
                            BorderColor::all(theme::TEXT_DISABLED),
                        ))
                        .with_children(|badge| {
                            badge.spawn((
                                Text::new(key),
                                TextFont {
                                    font: fonts.body_emphasis.clone(),
                                    font_size: theme::FONT_TINY,
                                    ..default()
                                },
                                TextColor(theme::TEXT_SECONDARY),
                            ));
                        });
                        // Action label
                        row.spawn((
                            Text::new(action),
                            TextFont {
                                font: fonts.body.clone(),
                                font_size: theme::FONT_TINY,
                                ..default()
                            },
                            TextColor(theme::TEXT_DISABLED),
                        ));
                    });
            }
        })
        .id();
    commands.entity(container).add_child(hint);
}

// ── Page Header ──

pub fn spawn_page_header<B: Bundle>(
    commands: &mut Commands,
    container: Entity,
    title: &str,
    back_marker: B,
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
                    back_marker,
                    Button,
                    ui_components::compact_button_node(12.0, 6.0),
                    ui_components::ghost_button_chrome(ui_components::UiTone::Neutral),
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

// ── Section Divider ──

pub fn spawn_animated_section_divider(
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

// ── Selector Row ──

pub fn spawn_selector_row(
    commands: &mut Commands,
    container: Entity,
    label: &str,
    options: &[&str],
    selected: usize,
    field: SelectorField,
) {
    spawn_selector_row_nav(commands, container, label, options, selected, field, None);
}

pub fn spawn_selector_row_nav(
    commands: &mut Commands,
    container: Entity,
    label: &str,
    options: &[&str],
    selected: usize,
    field: SelectorField,
    nav_index: Option<usize>,
) {
    let mut ec = commands.spawn(Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        margin: UiRect::vertical(Val::Px(6.0)),
        padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
        border: UiRect::left(Val::Px(2.0)),
        ..default()
    });
    ec.insert(BorderColor::all(Color::NONE));
    if let Some(idx) = nav_index {
        ec.insert(NavFocusable(idx));
    }
    let row = ec.with_children(|parent| {
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
                let text_color = if is_selected {
                    Color::WHITE
                } else {
                    theme::TEXT_SECONDARY
                };

                let mut btn = parent.spawn((
                    MenuSelector { field, index: i },
                    Button,
                    ui_components::compact_button_node_with_margin(14.0, 7.0, 2.0),
                    if is_selected {
                        ui_components::filled_button_chrome(ui_components::UiTone::Accent)
                    } else {
                        ui_components::filled_button_chrome(ui_components::UiTone::Neutral)
                    },
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

// ── Volume Slider ──

/// Marker on the slider track container. Stores which field it controls.
#[derive(Component, Clone, Copy)]
pub struct VolumeSlider(pub SelectorField);

/// The filled portion of the slider bar.
#[derive(Component)]
pub struct VolumeSliderFill;

/// The percentage label to the right of the slider.
#[derive(Component)]
pub struct VolumeSliderLabel(pub SelectorField);

/// State for active slider drag.
#[derive(Resource, Default)]
pub struct SliderDragState {
    pub active: Option<Entity>,
}

/// Spawns a volume slider row: `[Label:] [====■-----------] [50%]`
pub fn spawn_volume_slider(
    commands: &mut Commands,
    container: Entity,
    label: &str,
    value: f32,
    field: SelectorField,
    nav_index: Option<usize>,
) {
    let pct = (value * 100.0).round();

    let mut ec = commands.spawn(Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        margin: UiRect::vertical(Val::Px(6.0)),
        padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
        border: UiRect::left(Val::Px(2.0)),
        ..default()
    });
    ec.insert(BorderColor::all(Color::NONE));
    if let Some(idx) = nav_index {
        ec.insert(NavFocusable(idx));
    }
    let row = ec
        .with_children(|parent| {
            // Label
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

            // Slider track (clickable area)
            parent
                .spawn((
                    VolumeSlider(field),
                    Button,
                    Interaction::None,
                    RelativeCursorPosition::default(),
                    Node {
                        width: Val::Px(260.0),
                        height: Val::Px(20.0),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.10, 0.10, 0.10, 0.94)),
                    BorderColor::all(theme::SEPARATOR),
                ))
                .with_children(|track| {
                    // Filled bar
                    track.spawn((
                        VolumeSliderFill,
                        Node {
                            width: Val::Percent(pct),
                            height: Val::Percent(100.0),
                            border_radius: BorderRadius::all(Val::Px(3.0)),
                            ..default()
                        },
                        BackgroundColor(theme::ACCENT),
                        Pickable::IGNORE,
                    ));
                });

            // Percentage label
            parent.spawn((
                VolumeSliderLabel(field),
                Text::new(format!("{pct:.0}%")),
                TextFont {
                    font_size: theme::FONT_MEDIUM,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    width: Val::Px(45.0),
                    margin: UiRect::left(Val::Px(8.0)),
                    ..default()
                },
            ));
        })
        .id();
    commands.entity(container).add_child(row);
}

// ── Name Input Row ──

pub fn spawn_name_input_row(commands: &mut Commands, current_name: &str) -> Entity {
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
                        selection_anchor: None,
                        max_len: 45,
                    },
                    Button,
                    ui_components::input_node(280.0, 32.0),
                    ui_components::input_chrome(),
                ))
                .with_children(|input| {
                    spawn_text_input_children(input, current_name);
                });

            parent
                .spawn((
                    RandomNameButton,
                    Button,
                    {
                        let mut node = ui_components::compact_button_node(10.0, 6.0);
                        node.margin = UiRect::left(Val::Px(6.0));
                        node
                    },
                    ui_components::ghost_button_chrome(ui_components::UiTone::Neutral),
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

