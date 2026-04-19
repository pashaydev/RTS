//! Title page: hero treatment, main menu buttons, and bottom status bar.

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;

use crate::types::*;
use crate::ui::core::components as ui_components;
use crate::ui::core::fonts::{self, UiFonts};
use crate::ui::theme::{Theme, BG_ELEVATED, TEXT_PRIMARY};

use super::{MenuAction, MenuButton, MenuStatusBar};

// ── Constants ──

const MENU_BUTTON_WIDTH: f32 = 400.0;
const MENU_BUTTON_HEIGHT: f32 = 48.0;
const STATUS_BAR_HEIGHT: f32 = 36.0;
const STATUS_ICON_SIZE: f32 = 24.0;
const STATUS_ICON_INNER: f32 = 16.0;
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// ── Title Page ──

pub(crate) fn spawn_title_page(
    commands: &mut Commands,
    container: Entity,
    root: Entity,
    fonts: &UiFonts,
    theme: &Theme,
) {
    spawn_title_treatment(commands, container, fonts, theme);

    for (i, (label, action)) in [
        ("NEW GAME", MenuAction::NewGame),
        ("LOAD GAME", MenuAction::LoadGame),
        ("MULTIPLAYER", MenuAction::Multiplayer),
        ("OPTIONS", MenuAction::Options),
    ]
    .iter()
    .enumerate()
    {
        let btn =
            spawn_accent_menu_button(commands, label, MenuButton(*action), Some(i), fonts, theme);
        commands.entity(container).add_child(btn);
    }

    let quit_btn = spawn_quit_menu_button(
        commands,
        "QUIT",
        MenuButton(MenuAction::Quit),
        Some(4),
        fonts,
        theme,
    );
    commands.entity(container).add_child(quit_btn);

    spawn_status_bar(commands, root, fonts, theme);
}

// ── Title Treatment ──

fn spawn_title_treatment(
    commands: &mut Commands,
    container: Entity,
    fonts: &UiFonts,
    theme: &Theme,
) {
    let title_block = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            margin: UiRect::bottom(Val::Px(40.0)),
            ..default()
        })
        .with_children(|parent| {
            // "RTS" — large bold gold
            parent.spawn((
                Text::new("RTS"),
                fonts::heading(fonts, 72.0),
                TextColor(theme.colors.text_primary),
                Node {
                    margin: UiRect::bottom(Val::Px(-16.0)),
                    ..default()
                },
            ));
            // "COMMAND" — very large bold white
            parent.spawn((
                Text::new("COMMAND"),
                fonts::heading(fonts, 96.0),
                TextColor(theme.colors.prestige),
            ));
        })
        .id();
    commands.entity(container).add_child(title_block);
}

// ── Accent Menu Button (with gold left bar) ──

pub(crate) fn spawn_accent_menu_button(
    commands: &mut Commands,
    label: &str,
    marker: impl Bundle,
    nav_index: Option<usize>,
    fonts: &UiFonts,
    theme: &Theme,
) -> Entity {
    let mut node = ui_components::button_node(MENU_BUTTON_WIDTH, MENU_BUTTON_HEIGHT);
    node.border = UiRect::all(Val::Px(1.0));
    node.overflow = Overflow::clip();

    let mut ec = commands.spawn((
        marker,
        Button,
        node,
        ui_components::accent_button_chrome(theme),
    ));
    if let Some(idx) = nav_index {
        ec.insert(NavFocusable(idx));
    }
    ec.with_children(|parent| {
        parent.spawn((
            Text::new(label),
            fonts::heading(fonts, theme.typography.button),
            TextColor(TEXT_PRIMARY),
            Pickable::IGNORE,
        ));
    });
    ec.id()
}

// ── Quit Menu Button ──

pub(crate) fn spawn_quit_menu_button(
    commands: &mut Commands,
    label: &str,
    marker: impl Bundle,
    nav_index: Option<usize>,
    fonts: &UiFonts,
    theme: &Theme,
) -> Entity {
    let bg = theme.colors.bg_menu.with_alpha(0.10);
    let mut node = ui_components::button_node(MENU_BUTTON_WIDTH, MENU_BUTTON_HEIGHT);
    node.border = UiRect::all(Val::Px(1.0));
    node.overflow = Overflow::clip();

    let mut ec = commands.spawn((
        marker,
        Button,
        node,
        ButtonAnimState::new(bg.to_srgba().to_f32_array()),
        ButtonStyle::Destructive,
        BackgroundColor(bg),
        BorderColor::all(theme.colors.accent.with_alpha(0.20)),
    ));
    if let Some(idx) = nav_index {
        ec.insert(NavFocusable(idx));
    }
    ec.with_children(|parent| {
        parent.spawn((
            Text::new(label),
            fonts::heading(fonts, theme.typography.button),
            TextColor(theme.colors.accent),
            Pickable::IGNORE,
        ));
    });
    ec.id()
}

// ── Bottom Status Bar ──

pub(crate) fn spawn_status_bar(
    commands: &mut Commands,
    root: Entity,
    fonts: &UiFonts,
    theme: &Theme,
) {
    let bar = commands
        .spawn((
            MenuStatusBar,
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(STATUS_BAR_HEIGHT),
                position_type: PositionType::Absolute,
                bottom: Val::Px(0.0),
                left: Val::Px(0.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::axes(Val::Px(16.0), Val::Px(0.0)),
                border: UiRect::top(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(theme.colors.bg_menu.with_alpha(0.9)),
            BorderColor::all(theme.colors.separator),
        ))
        .with_children(|parent| {
            // LEFT: icon buttons
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(6.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|left| {
                    for icon_handle in [fonts.sliders_icon.clone(), fonts.terminal_icon.clone()] {
                        spawn_status_icon(left, icon_handle, theme);
                    }
                });

            // CENTER: key hints
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(20.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|center| {
                    spawn_key_hint_pair(center, "ENTER", "CONFIRM", fonts, theme);
                    spawn_key_hint_pair(center, "ESC", "DENY", fonts, theme);
                });

            // RIGHT: credits + version
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(12.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|right| {
                    right.spawn((
                        Text::new("Pasha Yakubovsky \u{00A9} 2026"),
                        TextFont {
                            font: fonts.body_emphasis.clone(),
                            font_size: 11.0,
                            ..default()
                        },
                        TextColor(theme.colors.text_disabled.with_alpha(0.5)),
                    ));
                    right.spawn((
                        Text::new(format!("v{APP_VERSION}")),
                        TextFont {
                            font: fonts.body.clone(),
                            font_size: 9.0,
                            ..default()
                        },
                        TextColor(theme.colors.text_disabled.with_alpha(0.5)),
                    ));
                });
        })
        .id();
    commands.entity(root).add_child(bar);
}

fn spawn_status_icon(parent: &mut ChildSpawnerCommands, icon_handle: Handle<Image>, theme: &Theme) {
    parent
        .spawn((
            Node {
                width: Val::Px(STATUS_ICON_SIZE),
                height: Val::Px(STATUS_ICON_SIZE),
                border: UiRect::all(Val::Px(1.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(theme.colors.bg_elevated),
            BorderColor::all(theme.colors.separator),
        ))
        .with_children(|btn| {
            btn.spawn((
                ImageNode::new(icon_handle),
                Node {
                    width: Val::Px(STATUS_ICON_INNER),
                    height: Val::Px(STATUS_ICON_INNER),
                    ..default()
                },
            ));
        });
}

fn spawn_key_hint_pair(
    parent: &mut ChildSpawnerCommands,
    key: &str,
    label: &str,
    fonts: &UiFonts,
    theme: &Theme,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Node {
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                    border: UiRect::all(Val::Px(1.0)),
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
                        font_size: 9.0,
                        ..default()
                    },
                    TextColor(theme.colors.text_secondary),
                ));
            });
            row.spawn((
                Text::new(label),
                TextFont {
                    font: fonts.body.clone(),
                    font_size: 9.0,
                    ..default()
                },
                TextColor(theme.colors.text_disabled),
            ));
        });
}
