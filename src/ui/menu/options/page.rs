//! Options page layout: System Config style with sidebar tabs, ID-tagged
//! setting cards, and a bottom action bar for graphics/audio/gameplay.

use bevy::prelude::*;

use super::resolution_index;
use super::resolution_label;
use super::ResolutionRow;
use crate::types::*;
use crate::ui::core::components as ui_components;
use crate::ui::core::fonts::{self, UiFonts};
use crate::ui::menu::helpers::*;
use crate::ui::menu::*;
use crate::ui::theme::Theme;

// ── Options Page ──

pub(crate) fn spawn_options_page(
    commands: &mut Commands,
    container: Entity,
    graphics: &GraphicsSettings,
    audio_settings: &crate::infrastructure::audio::AudioSettings,
    gameplay: &GameplaySettings,
    resolutions: &AvailableResolutions,
    fonts: &UiFonts,
    theme: &Theme,
    active_tab: OptionsTab,
) {
    // Root container filling the panel — a column with header / body / footer.
    let root = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .id();
    commands.entity(container).add_child(root);

    spawn_header_strip(commands, root, fonts, theme);

    // Main body: sidebar + content.
    let body = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Stretch,
            min_height: Val::Px(520.0),
            ..default()
        })
        .id();
    commands.entity(root).add_child(body);

    spawn_sidebar(commands, body, active_tab, fonts, theme);

    let content = commands
        .spawn(Node {
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            padding: UiRect::axes(Val::Px(28.0), Val::Px(24.0)),
            row_gap: Val::Px(20.0),
            ..default()
        })
        .id();
    commands.entity(body).add_child(content);

    match active_tab {
        OptionsTab::Graphics => spawn_graphics_tab(commands, content, graphics, resolutions, fonts, theme),
        OptionsTab::Audio => spawn_audio_tab(commands, content, audio_settings, fonts, theme),
        OptionsTab::Gameplay => spawn_gameplay_tab(commands, content, gameplay, fonts, theme),
    }

    spawn_action_bar(commands, root, fonts, theme);
}

// ── Header strip ──

fn spawn_header_strip(commands: &mut Commands, parent: Entity, fonts: &UiFonts, theme: &Theme) {
    let row = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::axes(Val::Px(20.0), Val::Px(12.0)),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(theme.colors.bg_recessed.with_alpha(0.6)),
            BorderColor::all(theme.colors.separator),
        ))
        .id();
    commands.entity(parent).add_child(row);

    // Left: brand label
    let brand = commands
        .spawn((
            Text::new(""),
            TextFont {
                font: fonts.body_emphasis.clone(),
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.accent),
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(row).add_child(brand);

    // Right: close "X" back button
    let close_btn = commands
        .spawn((
            MenuButton(MenuAction::Back),
            Button,
            NavFocusable(0),
            Node {
                width: Val::Px(32.0),
                height: Val::Px(32.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            ui_components::ghost_button_chrome(theme, ui_components::UiTone::Neutral),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("X"),
                TextFont {
                    font: fonts.body_emphasis.clone(),
                    font_size: theme.typography.medium,
                    ..default()
                },
                TextColor(theme.colors.accent),
                Pickable::IGNORE,
            ));
        })
        .id();
    commands.entity(row).add_child(close_btn);
}

// ── Sidebar ──

fn spawn_sidebar(
    commands: &mut Commands,
    parent: Entity,
    active_tab: OptionsTab,
    fonts: &UiFonts,
    theme: &Theme,
) {
    let sidebar = commands
        .spawn((
            Node {
                width: Val::Px(220.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(20.0), Val::Px(24.0)),
                row_gap: Val::Px(6.0),
                border: UiRect::right(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(theme.colors.bg_recessed.with_alpha(0.35)),
            BorderColor::all(theme.colors.separator),
        ))
        .id();
    commands.entity(parent).add_child(sidebar);

    // Title block
    let title_block = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            margin: UiRect::bottom(Val::Px(24.0)),
            ..default()
        })
        .with_children(|col| {
            col.spawn((
                Text::new("SYSTEM\nCONFIG"),
                fonts::heading(fonts, theme.typography.heading),
                TextColor(theme.colors.text_primary),
                Pickable::IGNORE,
            ));
            col.spawn((
                Text::new("V.2.4.0-STABLE"),
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
    commands.entity(sidebar).add_child(title_block);

    spawn_sidebar_tab(commands, sidebar, "GRAPHICS", OptionsTab::Graphics, active_tab, 1, fonts, theme);
    spawn_sidebar_tab(commands, sidebar, "AUDIO", OptionsTab::Audio, active_tab, 2, fonts, theme);
    spawn_sidebar_tab(commands, sidebar, "GAMEPLAY", OptionsTab::Gameplay, active_tab, 3, fonts, theme);
}

fn spawn_sidebar_tab(
    commands: &mut Commands,
    parent: Entity,
    label: &str,
    tab: OptionsTab,
    active: OptionsTab,
    nav_index: usize,
    fonts: &UiFonts,
    theme: &Theme,
) {
    let is_active = tab == active;
    let bg = if is_active {
        theme.colors.accent.with_alpha(0.18)
    } else {
        Color::NONE
    };
    let border = if is_active {
        theme.colors.accent
    } else {
        Color::NONE
    };
    let text_color = if is_active {
        theme.colors.text_primary
    } else {
        theme.colors.text_secondary
    };

    let btn = commands
        .spawn((
            OptionsTabButton(tab),
            Button,
            NavFocusable(nav_index),
            ButtonAnimState::new(bg.to_srgba().to_f32_array()),
            ButtonStyle::Ghost,
            Node {
                width: Val::Percent(100.0),
                padding: UiRect::axes(Val::Px(14.0), Val::Px(12.0)),
                align_items: AlignItems::Center,
                border: UiRect::left(Val::Px(3.0)),
                column_gap: Val::Px(10.0),
                ..default()
            },
            BackgroundColor(bg),
            BorderColor::all(border),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont {
                    font: fonts.body_emphasis.clone(),
                    font_size: theme.typography.medium,
                    ..default()
                },
                TextColor(text_color),
                Pickable::IGNORE,
            ));
        })
        .id();
    commands.entity(parent).add_child(btn);
}

// ── Section headers and cards ──

/// "■ LABEL" small accent text above a section heading.
fn spawn_section_subtitle(commands: &mut Commands, parent: Entity, label: &str, fonts: &UiFonts, theme: &Theme) {
    let text = format!("\u{25A0} {label}");
    let entity = commands
        .spawn((
            Text::new(text),
            TextFont {
                font: fonts.body_emphasis.clone(),
                font_size: theme.typography.small,
                ..default()
            },
            TextColor(theme.colors.accent),
            Node {
                margin: UiRect::bottom(Val::Px(2.0)),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(parent).add_child(entity);
}

fn spawn_section_heading(commands: &mut Commands, parent: Entity, text: &str, fonts: &UiFonts, theme: &Theme) {
    let entity = commands
        .spawn((
            Text::new(text),
            fonts::heading(fonts, theme.typography.heading * 0.82),
            TextColor(theme.colors.text_primary),
            Node {
                margin: UiRect::bottom(Val::Px(8.0)),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(parent).add_child(entity);
}

/// Spawns a bordered settings card with an "ID:" tag at the top-left corner.
/// Returns the inner content entity where rows should be appended.
fn spawn_settings_card(
    commands: &mut Commands,
    parent: Entity,
    id_tag: &str,
    fonts: &UiFonts,
    theme: &Theme,
) -> Entity {
    let card = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect {
                    left: Val::Px(20.0),
                    right: Val::Px(20.0),
                    top: Val::Px(22.0),
                    bottom: Val::Px(16.0),
                },
                border: UiRect::all(Val::Px(1.0)),
                row_gap: Val::Px(4.0),
                // Relative so the ID badge can be absolutely positioned on the border.
                position_type: PositionType::Relative,
                ..default()
            },
            BackgroundColor(theme.colors.bg_recessed.with_alpha(0.45)),
            BorderColor::all(theme.colors.separator),
        ))
        .id();
    commands.entity(parent).add_child(card);

    // ID tag — sits on the top border, left side.
    let tag = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(-8.0),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(theme.colors.bg_panel),
            Pickable::IGNORE,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(format!("ID: {id_tag}")),
                TextFont {
                    font: fonts.body_emphasis.clone(),
                    font_size: theme.typography.tiny,
                    ..default()
                },
                TextColor(theme.colors.text_disabled),
                Pickable::IGNORE,
            ));
        })
        .id();
    commands.entity(card).add_child(tag);

    card
}

// ── Bottom action bar ──

fn spawn_action_bar(commands: &mut Commands, parent: Entity, fonts: &UiFonts, theme: &Theme) {
    let bar = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::axes(Val::Px(20.0), Val::Px(14.0)),
                border: UiRect::top(Val::Px(1.0)),
                column_gap: Val::Px(10.0),
                ..default()
            },
            BackgroundColor(theme.colors.bg_recessed.with_alpha(0.6)),
            BorderColor::all(theme.colors.separator),
        ))
        .id();
    commands.entity(parent).add_child(bar);

    // Left side: status indicator text (decorative)
    let status = commands
        .spawn((
            Text::new("\u{25CF} SYSTEM READY"),
            TextFont {
                font: fonts.body_emphasis.clone(),
                font_size: theme.typography.tiny,
                ..default()
            },
            TextColor(theme.colors.success),
            Pickable::IGNORE,
        ))
        .id();
    commands.entity(bar).add_child(status);

    // Right side: discard + apply buttons
    let right = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0),
            ..default()
        })
        .id();
    commands.entity(bar).add_child(right);

    let discard_btn = commands
        .spawn((
            MenuButton(MenuAction::DiscardSettings),
            DiscardSettingsButton,
            Button,
            Visibility::Hidden,
            {
                let mut n = ui_components::button_node(140.0, 40.0);
                n.border = UiRect::all(Val::Px(1.0));
                n
            },
            ui_components::ghost_button_chrome(theme, ui_components::UiTone::Neutral),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new("DISCARD"),
                fonts::heading(fonts, theme.typography.small),
                TextColor(theme.colors.text_secondary),
                Pickable::IGNORE,
            ));
        })
        .id();
    commands.entity(right).add_child(discard_btn);

    let apply_btn = commands
        .spawn((
            MenuButton(MenuAction::ApplySettings),
            SaveSettingsButton,
            Button,
            Visibility::Hidden,
            {
                let mut n = ui_components::button_node(160.0, 40.0);
                n.border = UiRect::all(Val::Px(1.0));
                n
            },
            ui_components::filled_button_chrome(theme, ui_components::UiTone::Accent),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new("APPLY CHANGES"),
                fonts::heading(fonts, theme.typography.small),
                TextColor(theme.colors.bg_panel),
                Pickable::IGNORE,
            ));
        })
        .id();
    commands.entity(right).add_child(apply_btn);
}

// ── GRAPHICS TAB ──

fn spawn_graphics_tab(
    commands: &mut Commands,
    parent: Entity,
    graphics: &GraphicsSettings,
    resolutions: &AvailableResolutions,
    fonts: &UiFonts,
    theme: &Theme,
) {
    // ── DISPLAY OUTPUT ──
    spawn_section_subtitle(commands, parent, "DISPLAY OUTPUT", fonts, theme);
    spawn_section_heading(commands, parent, "Graphics Configuration", fonts, theme);
    let card = spawn_settings_card(commands, parent, "DSP-01", fonts, theme);

    let fs_idx = if graphics.fullscreen { 0 } else { 1 };
    spawn_selector_row(commands, card, "Fullscreen", &["ON", "OFF"], fs_idx, SelectorField::Fullscreen, Some(10), theme);

    let res_idx = resolution_index(&resolutions.0, graphics.resolution);
    let res_label = if let Some(&(w, h)) = resolutions.0.get(res_idx) {
        resolution_label(w, h)
    } else {
        resolution_label(graphics.resolution.0, graphics.resolution.1)
    };
    let res_row = commands
        .spawn((
            ResolutionRow,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
        ))
        .id();
    commands.entity(card).add_child(res_row);
    spawn_arrow_selector(commands, res_row, "Resolution", &res_label, res_idx, resolutions.0.len(), SelectorField::Resolution, Some(11), theme);

    let vsync_idx = if graphics.vsync { 0 } else { 1 };
    spawn_selector_row(commands, card, "Vertical Sync", &["ON", "OFF"], vsync_idx, SelectorField::Vsync, Some(12), theme);

    // ── RENDER PIPELINE ──
    spawn_section_subtitle(commands, parent, "RENDER PIPELINE", fonts, theme);
    spawn_section_heading(commands, parent, "Quality Settings", fonts, theme);
    let card = spawn_settings_card(commands, parent, "RND-02", fonts, theme);

    let shadow_idx = match graphics.shadow_quality {
        ShadowQuality::Off => 0,
        ShadowQuality::Low => 1,
        ShadowQuality::High => 2,
    };
    spawn_selector_row(commands, card, "Shadow Quality", &["Off", "Low", "High"], shadow_idx, SelectorField::Shadows, Some(20), theme);

    let lights_idx = if graphics.entity_lights { 0 } else { 1 };
    spawn_selector_row(commands, card, "Entity Lights", &["ON", "OFF"], lights_idx, SelectorField::EntityLights, Some(21), theme);

    let aa_idx = match graphics.anti_aliasing {
        AntiAliasingMode::Off => 0,
        AntiAliasingMode::Smaa => 1,
    };
    spawn_selector_row(commands, card, "Anti-Aliasing", &["Off", "SMAA"], aa_idx, SelectorField::AntiAliasing, Some(22), theme);

    let bloom_idx = match graphics.bloom {
        EffectQuality::Off => 0,
        EffectQuality::Low => 1,
        EffectQuality::Medium => 2,
        EffectQuality::High => 3,
    };
    spawn_selector_row(commands, card, "Bloom", &["Off", "Low", "Medium", "High"], bloom_idx, SelectorField::Bloom, Some(23), theme);

    // ── POST-PROCESSING ──
    spawn_section_subtitle(commands, parent, "POST-PROCESSING", fonts, theme);
    spawn_section_heading(commands, parent, "Color & Effects", fonts, theme);
    let card = spawn_settings_card(commands, parent, "FX-03", fonts, theme);

    let brightness_labels: Vec<&str> = BRIGHTNESS_OPTIONS.iter().map(|&(_, s)| s).collect();
    let brightness_idx = BRIGHTNESS_OPTIONS
        .iter()
        .position(|&(v, _)| (v - graphics.brightness).abs() < 0.01)
        .unwrap_or(2);
    spawn_selector_row(commands, card, "Brightness", &brightness_labels, brightness_idx, SelectorField::Brightness, Some(30), theme);

    let auto_exposure_idx = if graphics.auto_exposure { 0 } else { 1 };
    spawn_selector_row(commands, card, "Auto Exposure", &["ON", "OFF"], auto_exposure_idx, SelectorField::AutoExposure, Some(31), theme);

    let dof_idx = match graphics.depth_of_field {
        EffectQuality::Off => 0,
        EffectQuality::Low => 1,
        EffectQuality::Medium => 2,
        EffectQuality::High => 3,
    };
    spawn_selector_row(commands, card, "Depth of Field", &["Off", "Low", "Medium", "High"], dof_idx, SelectorField::DepthOfField, Some(32), theme);

    let chromatic_idx = match graphics.chromatic_aberration {
        EffectQuality::Off => 0,
        EffectQuality::Low => 1,
        EffectQuality::Medium => 2,
        EffectQuality::High => 3,
    };
    spawn_selector_row(commands, card, "Chromatic Aberration", &["Off", "Low", "Medium", "High"], chromatic_idx, SelectorField::ChromaticAberration, Some(33), theme);

    // ── INTERFACE ──
    spawn_section_subtitle(commands, parent, "INTERFACE", fonts, theme);
    spawn_section_heading(commands, parent, "UI Settings", fonts, theme);
    let card = spawn_settings_card(commands, parent, "UI-04", fonts, theme);

    let scale_labels: Vec<&str> = UI_SCALE_OPTIONS.iter().map(|&(_, s)| s).collect();
    let scale_idx = UI_SCALE_OPTIONS
        .iter()
        .position(|&(v, _)| (v - graphics.ui_scale).abs() < 0.01)
        .unwrap_or(2);
    spawn_selector_row(commands, card, "UI Scale", &scale_labels, scale_idx, SelectorField::UiScale, Some(40), theme);

    let reset_row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(8.0)),
            ..default()
        })
        .id();
    commands.entity(card).add_child(reset_row);
    let reset_btn = spawn_styled_button(
        commands,
        "RESET WIDGET LAYOUT",
        MenuButton(MenuAction::ResetWidgetLayout),
        false,
        fonts,
        Some(41),
        theme,
    );
    commands.entity(reset_row).add_child(reset_btn);
}

// ── AUDIO TAB ──

fn spawn_audio_tab(
    commands: &mut Commands,
    parent: Entity,
    audio_settings: &crate::infrastructure::audio::AudioSettings,
    fonts: &UiFonts,
    theme: &Theme,
) {
    spawn_section_subtitle(commands, parent, "SOUND OUTPUT", fonts, theme);
    spawn_section_heading(commands, parent, "Audio Configuration", fonts, theme);
    let card = spawn_settings_card(commands, parent, "AUD-01", fonts, theme);

    spawn_volume_slider(commands, card, "Music Volume", audio_settings.music_volume, SelectorField::MusicVolume, Some(10), theme);
    spawn_volume_slider(commands, card, "SFX Volume", audio_settings.sfx_volume, SelectorField::SfxVolume, Some(11), theme);
}

// ── GAMEPLAY TAB ──

fn spawn_gameplay_tab(
    commands: &mut Commands,
    parent: Entity,
    gameplay: &GameplaySettings,
    fonts: &UiFonts,
    theme: &Theme,
) {
    spawn_section_subtitle(commands, parent, "SIMULATION", fonts, theme);
    spawn_section_heading(commands, parent, "Game Settings", fonts, theme);
    let card = spawn_settings_card(commands, parent, "GPL-01", fonts, theme);

    let speed_labels: Vec<&str> = GAME_SPEED_OPTIONS.iter().map(|&(_, s)| s).collect();
    let speed_idx = GAME_SPEED_OPTIONS
        .iter()
        .position(|&(v, _)| (v - gameplay.game_speed).abs() < 0.01)
        .unwrap_or(4);
    spawn_selector_row(commands, card, "Game Speed", &speed_labels, speed_idx, SelectorField::GameSpeed, Some(10), theme);
}
