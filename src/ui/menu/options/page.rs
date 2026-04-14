use bevy::prelude::*;

use super::resolution_index;
use super::resolution_label;
use super::ResolutionRow;
use crate::types::*;
use crate::ui::fonts::UiFonts;
use crate::ui::menu::helpers::*;
use crate::ui::menu::*;
use crate::ui::theme::Theme;

// ── Options Page ──

pub(crate) fn spawn_options_page(
    commands: &mut Commands,
    container: Entity,
    graphics: &GraphicsSettings,
    audio_settings: &crate::infrastructure::audio::AudioSettings,
    resolutions: &AvailableResolutions,
    fonts: &UiFonts,
    theme: &Theme,
) {
    spawn_page_header(
        commands,
        container,
        "OPTIONS",
        MenuButton(MenuAction::Back),
        fonts,
        theme,
    );

    // ── Graphics Section ──

    spawn_animated_section_divider(commands, container, "GRAPHICS", fonts, theme);

    let fs_idx = if graphics.fullscreen { 0 } else { 1 };
    spawn_selector_row(
        commands,
        container,
        "Fullscreen:",
        &["ON", "OFF"],
        fs_idx,
        SelectorField::Fullscreen,
        Some(0),
        theme,
    );

    // Resolution arrow selector — greyed out when fullscreen is ON
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
    commands.entity(container).add_child(res_row);

    spawn_arrow_selector(
        commands,
        res_row,
        "Resolution:",
        &res_label,
        res_idx,
        resolutions.0.len(),
        SelectorField::Resolution,
        Some(1),
        theme,
    );

    let vsync_idx = if graphics.vsync { 0 } else { 1 };
    spawn_selector_row(
        commands,
        container,
        "VSync:",
        &["ON", "OFF"],
        vsync_idx,
        SelectorField::Vsync,
        Some(2),
        theme,
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
        Some(3),
        theme,
    );

    let lights_idx = if graphics.entity_lights { 0 } else { 1 };
    spawn_selector_row(
        commands,
        container,
        "Lights:",
        &["ON", "OFF"],
        lights_idx,
        SelectorField::EntityLights,
        Some(4),
        theme,
    );

    let aa_idx = match graphics.anti_aliasing {
        AntiAliasingMode::Off => 0,
        AntiAliasingMode::Smaa => 1,
    };
    spawn_selector_row(
        commands,
        container,
        "Anti-Aliasing:",
        &["Off", "SMAA"],
        aa_idx,
        SelectorField::AntiAliasing,
        Some(5),
        theme,
    );

    let bloom_idx = match graphics.bloom {
        EffectQuality::Off => 0,
        EffectQuality::Low => 1,
        EffectQuality::Medium => 2,
        EffectQuality::High => 3,
    };
    spawn_selector_row(
        commands,
        container,
        "Bloom:",
        &["Off", "Low", "Medium", "High"],
        bloom_idx,
        SelectorField::Bloom,
        Some(6),
        theme,
    );

    let brightness_labels: Vec<&str> = BRIGHTNESS_OPTIONS.iter().map(|&(_, s)| s).collect();
    let brightness_idx = BRIGHTNESS_OPTIONS
        .iter()
        .position(|&(v, _)| (v - graphics.brightness).abs() < 0.01)
        .unwrap_or(2);
    spawn_selector_row(
        commands,
        container,
        "Brightness:",
        &brightness_labels,
        brightness_idx,
        SelectorField::Brightness,
        Some(7),
        theme,
    );

    let auto_exposure_idx = if graphics.auto_exposure { 0 } else { 1 };
    spawn_selector_row(
        commands,
        container,
        "Auto Exposure:",
        &["ON", "OFF"],
        auto_exposure_idx,
        SelectorField::AutoExposure,
        Some(8),
        theme,
    );

    let dof_idx = match graphics.depth_of_field {
        EffectQuality::Off => 0,
        EffectQuality::Low => 1,
        EffectQuality::Medium => 2,
        EffectQuality::High => 3,
    };
    spawn_selector_row(
        commands,
        container,
        "Depth of Field:",
        &["Off", "Low", "Medium", "High"],
        dof_idx,
        SelectorField::DepthOfField,
        Some(9),
        theme,
    );

    let chromatic_idx = match graphics.chromatic_aberration {
        EffectQuality::Off => 0,
        EffectQuality::Low => 1,
        EffectQuality::Medium => 2,
        EffectQuality::High => 3,
    };
    spawn_selector_row(
        commands,
        container,
        "Chromatic Aberration:",
        &["Off", "Low", "Medium", "High"],
        chromatic_idx,
        SelectorField::ChromaticAberration,
        Some(10),
        theme,
    );

    // ── UI Section ──

    spawn_animated_section_divider(commands, container, "UI", fonts, theme);

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
        Some(11),
        theme,
    );

    let reset_btn = spawn_styled_button(
        commands,
        "RESET WIDGET LAYOUT",
        MenuButton(MenuAction::ResetWidgetLayout),
        false,
        fonts,
        Some(12),
        theme,
    );
    commands.entity(container).add_child(reset_btn);

    // ── Audio Section ──

    spawn_animated_section_divider(commands, container, "AUDIO", fonts, theme);

    spawn_volume_slider(
        commands,
        container,
        "Music Volume:",
        audio_settings.music_volume,
        SelectorField::MusicVolume,
        Some(13),
        theme,
    );

    spawn_volume_slider(
        commands,
        container,
        "SFX Volume:",
        audio_settings.sfx_volume,
        SelectorField::SfxVolume,
        Some(14),
        theme,
    );

    // ── Save Button (hidden until settings change) ──

    let save_btn = spawn_styled_button(
        commands,
        "SAVE",
        (
            MenuButton(MenuAction::ApplySettings),
            super::super::SaveSettingsButton,
        ),
        true,
        fonts,
        Some(15),
        theme,
    );
    commands.entity(container).add_child(save_btn);
}
