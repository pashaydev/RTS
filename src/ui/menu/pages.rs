use bevy::prelude::*;

use super::helpers::*;
use crate::types::*;
use crate::infrastructure::database::{ActiveProfile, GameDatabase, SaveEntry};
use crate::ui::theme::Theme;
use crate::ui::fonts::UiFonts;

#[allow(unused_imports)]
use super::*;

// ── Options Page (delegated to options.rs) ──

pub(crate) use super::options::spawn_options_page;

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
    let back_btn = spawn_styled_button(
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
                // border_radius: BorderRadius::all(Val::Px(8.0)),
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
                // border_radius: BorderRadius::all(Val::Px(6.0)),
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
