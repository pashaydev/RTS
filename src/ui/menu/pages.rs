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
    asset_server: &AssetServer,
) {
    let saves = db.list_saves(&profile.id);

    // Outer bordered panel
    let panel = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                max_width: Val::Px(1060.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(32.0)),
                border: UiRect::all(Val::Px(1.0)),
                row_gap: Val::Px(24.0),
                ..default()
            },
            BackgroundColor(theme.colors.bg_surface),
            BorderColor::all(theme.colors.separator),
        ))
        .id();
    commands.entity(container).add_child(panel);

    // Title
    let title = commands
        .spawn((
            Text::new("SAVED GAMES"),
            TextFont {
                font: fonts.heading.clone(),
                font_size: theme.typography.display,
                ..default()
            },
            TextColor(theme.colors.text_primary),
        ))
        .id();
    commands.entity(panel).add_child(title);

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
                    margin: UiRect::bottom(Val::Px(16.0)),
                    ..default()
                },
            ))
            .id();
        commands.entity(panel).add_child(empty_text);
    } else {
        // Scrollable save list
        let list_container = commands
            .spawn(Node {
                width: Val::Percent(100.0),
                max_height: Val::Px(420.0),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
                row_gap: Val::Px(10.0),
                ..default()
            })
            .id();
        commands.entity(panel).add_child(list_container);

        let trash_icon: Handle<Image> = asset_server.load("icons/trash_can.png");
        let load_icon: Handle<Image> = asset_server.load("icons/load.png");

        for (i, save) in saves.iter().enumerate() {
            let row = spawn_save_entry_row(commands, save, i, fonts, theme, &trash_icon, &load_icon);
            commands.entity(list_container).add_child(row);
        }
    }

    // Back button (centered)
    let back_wrapper = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .id();
    commands.entity(panel).add_child(back_wrapper);

    let back_btn = spawn_styled_button(
        commands,
        "BACK",
        MenuButton(MenuAction::Back),
        false,
        fonts,
        Some(saves.len()),
        theme,
    );
    commands.entity(back_wrapper).add_child(back_btn);

    // Decorative status bar
    let status_bar = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            margin: UiRect::top(Val::Px(16.0)),
            padding: UiRect::top(Val::Px(12.0)),
            border: UiRect::top(Val::Px(1.0)),
            ..default()
        })
        .insert(BorderColor::all(theme.colors.separator))
        .id();
    commands.entity(panel).add_child(status_bar);

    let status_segments = [
        "ACTIVE STORAGE: SOLID-STATE COMMAND BUFFER",
        "ENCRYPTION: AES-256 IRON-CLAD",
        "SYSTEM CLOCK: 14:42:01 UTC",
    ];
    for seg in &status_segments {
        let txt = commands
            .spawn((
                Text::new(*seg),
                TextFont {
                    font: fonts.body.clone(),
                    font_size: theme.typography.tiny,
                    ..default()
                },
                TextColor(theme.colors.text_disabled),
                Pickable::IGNORE,
            ))
            .id();
        commands.entity(status_bar).add_child(txt);
    }
}

fn spawn_save_entry_row(
    commands: &mut Commands,
    save: &SaveEntry,
    index: usize,
    fonts: &UiFonts,
    theme: &Theme,
    trash_icon: &Handle<Image>,
    load_icon: &Handle<Image>,
) -> Entity {
    let label = save
        .label
        .clone()
        .unwrap_or_else(|| format!("Save #{}", save.id));

    let elapsed_hours = (save.elapsed_secs / 3600.0) as u32;
    let elapsed_mins = ((save.elapsed_secs % 3600.0) / 60.0) as u32;
    let elapsed_secs = (save.elapsed_secs % 60.0) as u32;
    let elapsed_str = format!("{elapsed_hours}:{elapsed_mins:02}:{elapsed_secs:02}");

    // Row card
    let row = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(20.0), Val::Px(14.0)),
                column_gap: Val::Px(16.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(theme.colors.bg_elevated),
            BorderColor::all(theme.colors.separator),
        ))
        .id();

    // Save name (left, large)
    let name_text = commands
        .spawn((
            Text::new(&label),
            TextFont {
                font: fonts.body_emphasis.clone(),
                font_size: theme.typography.large,
                ..default()
            },
            TextColor(theme.colors.text_primary),
            Pickable::IGNORE,
            Node {
                width: Val::Px(180.0),
                ..default()
            },
        ))
        .id();
    commands.entity(row).add_child(name_text);

    // Map magnitude column
    let map_col = spawn_labeled_column(
        commands,
        "MAP MAGNITUDE",
        &save.map_size.to_uppercase(),
        fonts,
        theme,
        140.0,
    );
    commands.entity(row).add_child(map_col);

    // Elapsed time column
    let time_col = spawn_labeled_column(
        commands,
        "ELAPSED TIME",
        &elapsed_str,
        fonts,
        theme,
        120.0,
    );
    commands.entity(row).add_child(time_col);

    // Timestamp column
    let ts_col = spawn_labeled_column(
        commands,
        "TIMESTAMP",
        &save.created_at,
        fonts,
        theme,
        140.0,
    );
    commands.entity(row).add_child(ts_col);

    // Delete button (trash icon)
    let del_btn = commands
        .spawn((
            MenuButton(MenuAction::DeleteSave(save.id)),
            Button,
            ButtonAnimState::new(theme.colors.destructive.to_srgba().to_f32_array()),
            ButtonStyle::Filled,
            Node {
                width: Val::Px(40.0),
                height: Val::Px(40.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(theme.colors.destructive.with_alpha(0.10)),
            BorderColor::all(theme.colors.destructive.with_alpha(0.2)),
        ))
        .with_children(|parent| {
            parent.spawn((
                ImageNode::new(trash_icon.clone()),
                Node {
                    width: Val::Px(20.0),
                    height: Val::Px(20.0),
                    ..default()
                },
                Pickable::IGNORE,
            ));
        })
        .id();
    commands.entity(row).add_child(del_btn);

    // Load button (accent teal with icon)
    let load_btn = commands
        .spawn((
            MenuButton(MenuAction::LoadSave(save.id)),
            Button,
            ButtonAnimState::new(theme.colors.accent.with_alpha(0.2).to_srgba().to_f32_array()),
            ButtonStyle::Filled,
            NavFocusable(index),
            Node {
                width: Val::Px(120.0),
                height: Val::Px(40.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(theme.colors.accent.with_alpha(0.15)),
            BorderColor::all(theme.colors.accent.with_alpha(0.35)),
        ))
        .with_children(|parent| {
            parent.spawn((
                ImageNode::new(load_icon.clone()),
                Node {
                    width: Val::Px(18.0),
                    height: Val::Px(18.0),
                    ..default()
                },
                Pickable::IGNORE,
            ));
            parent.spawn((
                Text::new("LOAD"),
                TextFont {
                    font: fonts.body_emphasis.clone(),
                    font_size: theme.typography.button,
                    ..default()
                },
                TextColor(theme.colors.text_primary),
                Pickable::IGNORE,
            ));
        })
        .id();
    commands.entity(row).add_child(load_btn);

    row
}

/// Spawns a vertical column with a small label on top and a value below.
fn spawn_labeled_column(
    commands: &mut Commands,
    label: &str,
    value: &str,
    fonts: &UiFonts,
    theme: &Theme,
    width: f32,
) -> Entity {
    let col = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            width: Val::Px(width),
            ..default()
        })
        .id();

    let lbl = commands
        .spawn((
            Text::new(label),
            TextFont {
                font: fonts.body.clone(),
                font_size: theme.typography.tiny,
                ..default()
            },
            TextColor(theme.colors.text_disabled),
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(col).add_child(lbl);

    let val = commands
        .spawn((
            Text::new(value),
            TextFont {
                font: fonts.body_emphasis.clone(),
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.text_primary),
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(col).add_child(val);

    col
}
