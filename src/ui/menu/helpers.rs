//! Menu-specific UI spawn helpers: panels, buttons, selectors, input rows.

use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use bevy_inspector_egui::egui::Margin;

use crate::types::*;
use crate::ui::core::components as ui_components;
use crate::ui::core::fonts::{self, UiFonts};
use crate::ui::core::text_input::{spawn_text_input_children, ScrollablePanel};
use crate::ui::theme::{Theme, BG_ELEVATED, BG_SURFACE, HIGHLIGHT, HIGHLIGHT_SUBTLE, TEXT_PRIMARY};

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
    DayNightPreset,
    DayCycleSeconds,
    NightSpawnIntensity,
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
    ThemeMode,
    GameSpeed,
}

#[derive(Component)]
pub struct SelectedOption;

#[derive(Component)]
pub struct SeedInput;

/// Small hint text shown below a button to explain disabled state.
#[derive(Component)]
pub struct ButtonHintText;

#[derive(Component)]
pub struct RandomizeSeedButton;

/// Wrapper around the 4 slot cards so they can be rebuilt without
/// tearing down the entire menu page (avoids replaying panel animations).
#[derive(Component)]
pub struct SlotCardsContainer;

pub(crate) fn menu_focus_border(theme: &Theme) -> BorderColor {
    BorderColor::all(theme.colors.accent)
}

pub(crate) fn menu_focus_shadow() -> BoxShadow {
    BoxShadow::new(
        HIGHLIGHT,
        Val::Px(0.0),
        Val::Px(0.0),
        Val::Px(0.0),
        Val::Px(2.0),
    )
}

// ── Panel ──

pub fn spawn_menu_panel(commands: &mut Commands, _theme: &Theme) -> Entity {
    commands
        .spawn((
            ScrollablePanel,
            Interaction::None,
            ScrollPosition::default(),
            Node {
                width: Val::Percent(96.0),
                max_width: Val::Px(1160.0),
                max_height: Val::Percent(90.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(24.0)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::NONE),
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
    nav_index: Option<usize>,
    theme: &Theme,
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
        ui_components::filled_button_chrome(theme, bg),
    ));
    if let Some(idx) = nav_index {
        entity_commands.insert(NavFocusable(idx));
    }
    entity_commands.with_children(|parent| {
        parent.spawn((
            Text::new(label),
            fonts::heading(fonts, theme.typography.button),
            TextColor(TEXT_PRIMARY),
            Pickable::IGNORE,
        ));
    });
    entity_commands.id()
}

/// Spawns a small hint text below a button (initially hidden).
pub fn spawn_button_hint(commands: &mut Commands, container: Entity, theme: &Theme) -> Entity {
    let hint = commands
        .spawn((
            ButtonHintText,
            Text::new(""),
            TextFont {
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.warning.with_alpha(0.85)),
            Node {
                margin: UiRect::top(Val::Px(4.0)),
                display: Display::None,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(container).add_child(hint);
    hint
}

/// Update hint text visibility based on nearby ButtonDisabled state.
pub fn update_button_hints(
    disabled_buttons: Query<&ButtonDisabled, With<super::MenuButton>>,
    mut hints: Query<(&mut Text, &mut Node, &mut TextColor), With<ButtonHintText>>,
    theme: Res<Theme>,
) {
    // Find the first disabled button with a reason (simple approach — one hint per page)
    let reason = disabled_buttons.iter().find_map(|d| d.0.as_ref());

    for (mut text, mut node, mut color) in &mut hints {
        if let Some(reason) = reason {
            **text = reason.clone();
            node.display = Display::Flex;
            *color = TextColor(theme.colors.warning.with_alpha(0.85));
        } else {
            node.display = Display::None;
        }
    }
}

/// Spawns a controls hint row at the bottom-left of the screen.
pub fn spawn_controls_hint(
    commands: &mut Commands,
    container: Entity,
    fonts: &UiFonts,
    theme: &Theme,
) {
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
                                // border_radius: BorderRadius::all(Val::Px(3.0)),
                                ..default()
                            },
                            BackgroundColor(BG_ELEVATED.with_alpha(0.9)),
                            BorderColor::all(theme.colors.text_disabled),
                        ))
                        .with_children(|badge| {
                            badge.spawn((
                                Text::new(key),
                                TextFont {
                                    font: fonts.body_emphasis.clone(),
                                    font_size: theme.typography.tiny,
                                    ..default()
                                },
                                TextColor(theme.colors.text_secondary),
                            ));
                        });
                        // Action label
                        row.spawn((
                            Text::new(action),
                            TextFont {
                                font: fonts.body.clone(),
                                font_size: theme.typography.tiny,
                                ..default()
                            },
                            TextColor(theme.colors.text_disabled),
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
    nav_index: Option<usize>,
    fonts: &UiFonts,
    theme: &Theme,
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
            let mut back_btn = parent.spawn((
                    back_marker,
                    Button,
                    ui_components::compact_button_node(12.0, 6.0),
                    ui_components::ghost_button_chrome(theme, ui_components::UiTone::Neutral),
                ));
            if let Some(idx) = nav_index {
                back_btn.insert(NavFocusable(idx));
            }
            back_btn.with_children(|btn| {
                btn.spawn((
                    Text::new("<< BACK"),
                    fonts::body_emphasis(fonts, theme.typography.medium),
                    TextColor(theme.colors.text_secondary),
                    Pickable::IGNORE,
                ));
            });

            parent.spawn((
                Text::new(title),
                fonts::heading(fonts, theme.typography.heading),
                TextColor(TEXT_PRIMARY),
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
    theme: &Theme,
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
                Node {
                    width: Val::Px(40.0),
                    height: Val::Px(1.0),
                    margin: UiRect::right(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(theme.colors.separator),
            ));

            parent.spawn((
                Text::new(label),
                fonts::heading(fonts, theme.typography.small),
                TextColor(theme.colors.text_secondary),
                Node {
                    margin: UiRect::horizontal(Val::Px(4.0)),
                    ..default()
                },
            ));

            parent.spawn((
                Node {
                    height: Val::Px(1.0),
                    flex_grow: 1.0,
                    margin: UiRect::left(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(theme.colors.separator),
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
    nav_index: Option<usize>,
    theme: &Theme,
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
    let row = ec
        .with_children(|parent| {
            parent.spawn((
                Text::new(label),
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

            for (i, &opt) in options.iter().enumerate() {
                let is_selected = i == selected;
                let text_color = if is_selected {
                    Color::srgb(0.10, 0.16, 0.18)
                } else {
                    theme.colors.text_secondary
                };

                let mut btn = parent.spawn((
                    MenuSelector { field, index: i },
                    Button,
                    ui_components::compact_button_node_with_margin(14.0, 7.0, 2.0),
                    if is_selected {
                        ui_components::filled_button_chrome(theme, ui_components::UiTone::Accent)
                    } else {
                        ui_components::filled_button_chrome(theme, ui_components::UiTone::Neutral)
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
        })
        .id();
    commands.entity(container).add_child(row);
}

// ── Arrow Selector (compact < value > for many options) ──

/// Marker for the value label in an arrow selector.
#[derive(Component)]
pub struct ArrowSelectorLabel(pub SelectorField);

/// Marker for the value background container in an arrow selector.
#[derive(Component)]
pub struct ArrowSelectorValueBg;

/// Spawns a compact arrow selector: `[Label:] [<] [value] [>]`
/// Used when there are too many options to show as individual buttons.
pub fn spawn_arrow_selector(
    commands: &mut Commands,
    container: Entity,
    label: &str,
    value_text: &str,
    selected_index: usize,
    total_options: usize,
    field: SelectorField,
    nav_index: Option<usize>,
    theme: &Theme,
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
    let row = ec
        .with_children(|parent| {
            // Label
            parent.spawn((
                Text::new(label),
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

            // Left arrow button (clamped — no wrap)
            let prev_idx = selected_index.saturating_sub(1);
            parent
                .spawn((
                    MenuSelector {
                        field,
                        index: prev_idx,
                    },
                    Button,
                    ui_components::compact_button_node_with_margin(10.0, 5.0, 2.0),
                    ui_components::ghost_button_chrome(theme, ui_components::UiTone::Neutral),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("<"),
                        TextFont {
                            font_size: theme.typography.medium,
                            ..default()
                        },
                        TextColor(theme.colors.text_secondary),
                        Pickable::IGNORE,
                    ));
                });

            // Current value display with selected-state highlight
            parent
                .spawn((
                    ArrowSelectorValueBg,
                    Node {
                        width: Val::Px(120.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        // border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(HIGHLIGHT_SUBTLE),
                    BorderColor::all(HIGHLIGHT),
                    Pickable::IGNORE,
                ))
                .with_children(|val_bg| {
                    val_bg.spawn((
                        ArrowSelectorLabel(field),
                        Text::new(value_text),
                        TextFont {
                            font_size: theme.typography.medium,
                            ..default()
                        },
                        TextColor(TEXT_PRIMARY),
                        Pickable::IGNORE,
                    ));
                });

            // Right arrow button (clamped — no wrap)
            let next_idx = (selected_index + 1).min(total_options.saturating_sub(1));
            parent
                .spawn((
                    MenuSelector {
                        field,
                        index: next_idx,
                    },
                    Button,
                    ui_components::compact_button_node_with_margin(10.0, 5.0, 2.0),
                    ui_components::ghost_button_chrome(theme, ui_components::UiTone::Neutral),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new(">"),
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
    commands.entity(container).add_child(row);
}

// ── Range Slider ──

/// Marker on the slider track container. Stores which field it controls.
#[derive(Component, Clone, Copy)]
pub struct RangeSlider {
    pub field: SelectorField,
    pub steps: Option<usize>,
}

/// The filled portion of the slider bar.
#[derive(Component)]
pub struct RangeSliderFill;

/// The value label to the right of the slider.
#[derive(Component)]
pub struct RangeSliderLabel(pub SelectorField);

/// State for active slider drag.
#[derive(Resource, Default)]
pub struct SliderDragState {
    pub active: Option<Entity>,
}

fn slider_fill_percent(value: f32, steps: Option<usize>) -> f32 {
    match steps {
        Some(steps) if steps > 1 => {
            (value.clamp(0.0, 1.0) * (steps - 1) as f32).round() / (steps - 1) as f32 * 100.0
        }
        _ => value.clamp(0.0, 1.0) * 100.0,
    }
}

/// Spawns a range slider row: `[Label:] [====-----------] [Value]`
pub fn spawn_range_slider(
    commands: &mut Commands,
    container: Entity,
    label: &str,
    value: f32,
    value_label: impl Into<String>,
    field: SelectorField,
    steps: Option<usize>,
    nav_index: Option<usize>,
    theme: &Theme,
) {
    let pct = slider_fill_percent(value, steps);
    let value_label = value_label.into();

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
                    font_size: theme.typography.medium,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
                Node {
                    width: Val::Px(120.0),
                    ..default()
                },
            ));

            // Slider track (clickable area)
            parent
                .spawn((
                    RangeSlider { field, steps },
                    Button,
                    Interaction::None,
                    RelativeCursorPosition::default(),
                    Node {
                        width: Val::Px(260.0),
                        height: Val::Px(20.0),
                        border: UiRect::all(Val::Px(1.0)),
                        // border_radius: BorderRadius::all(Val::Px(4.0)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(BG_SURFACE.with_alpha(0.94)),
                    BorderColor::all(theme.colors.separator),
                ))
                .with_children(|track| {
                    // Filled bar
                    track.spawn((
                        RangeSliderFill,
                        Node {
                            width: Val::Percent(pct),
                            height: Val::Percent(100.0),
                            // border_radius: BorderRadius::all(Val::Px(3.0)),
                            ..default()
                        },
                        BackgroundColor(theme.colors.accent),
                        Pickable::IGNORE,
                    ));
                });

            // Value label
            parent.spawn((
                RangeSliderLabel(field),
                Text::new(value_label),
                TextFont {
                    font_size: theme.typography.medium,
                    ..default()
                },
                TextColor(TEXT_PRIMARY),
                Node {
                    width: Val::Px(110.0),
                    margin: UiRect::left(Val::Px(8.0)),
                    ..default()
                },
            ));
        })
        .id();
    commands.entity(container).add_child(row);
}

/// Spawns a volume slider row: `[Label:] [====-----------] [50%]`
pub fn spawn_volume_slider(
    commands: &mut Commands,
    container: Entity,
    label: &str,
    value: f32,
    field: SelectorField,
    nav_index: Option<usize>,
    theme: &Theme,
) {
    let pct = (value * 100.0).round();
    spawn_range_slider(
        commands,
        container,
        label,
        value,
        format!("{pct:.0}%"),
        field,
        None,
        nav_index,
        theme,
    );
}

// ── Name Input Row ──

pub fn spawn_name_input_row(commands: &mut Commands, current_name: &str, theme: &Theme) -> Entity {
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
                    font_size: theme.typography.medium,
                    ..default()
                },
                TextColor(theme.colors.text_secondary),
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
                    ui_components::input_chrome(theme),
                ))
                .with_children(|input| {
                    spawn_text_input_children(input, current_name, theme);
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
                    ui_components::ghost_button_chrome(theme, ui_components::UiTone::Neutral),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("Random"),
                        TextFont {
                            font_size: theme.typography.medium,
                            ..default()
                        },
                        TextColor(theme.colors.accent),
                        Pickable::IGNORE,
                    ));
                });
        })
        .id()
}

// ── Title Treatment ──
