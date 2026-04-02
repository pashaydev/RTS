use bevy::prelude::*;
use bevy::window::PresentMode;

use super::helpers::*;
use super::*;
use crate::components::*;
use crate::theme::Theme;
use crate::ui::fonts::UiFonts;

// ── Resolution Options ──

pub(crate) const RESOLUTION_OPTIONS: &[(u32, u32)] = &[
    (1280, 720),
    (1366, 768),
    (1600, 900),
    (1920, 1080),
    (2560, 1440),
    (3440, 1440),
    (3840, 2160),
];

fn resolution_label(w: u32, h: u32) -> String {
    format!("{w}x{h}")
}

pub(crate) fn resolution_index(resolution: (u32, u32)) -> usize {
    RESOLUTION_OPTIONS
        .iter()
        .position(|&r| r == resolution)
        .unwrap_or(3)
}

pub(crate) fn resolution_slider_value(index: usize) -> f32 {
    if RESOLUTION_OPTIONS.len() <= 1 {
        0.0
    } else {
        index.min(RESOLUTION_OPTIONS.len() - 1) as f32 / (RESOLUTION_OPTIONS.len() - 1) as f32
    }
}

pub(crate) fn step_resolution_index(current_index: usize, delta: isize) -> usize {
    let max_index = RESOLUTION_OPTIONS.len().saturating_sub(1) as isize;
    (current_index as isize + delta).clamp(0, max_index) as usize
}

// ── Options Page ──

pub(crate) fn spawn_options_page(
    commands: &mut Commands,
    container: Entity,
    graphics: &GraphicsSettings,
    audio_settings: &crate::audio::AudioSettings,
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

    // ── Theme Section ──

    spawn_animated_section_divider(commands, container, "THEME", fonts, theme);

    let theme_idx = match graphics.theme_mode {
        crate::theme::ThemeMode::Dark => 0,
        crate::theme::ThemeMode::Light => 1,
    };
    spawn_selector_row(
        commands,
        container,
        "Color Mode:",
        &["Dark", "Light"],
        theme_idx,
        SelectorField::ThemeMode,
        theme,
    );

    // ── Graphics Section ──

    spawn_animated_section_divider(commands, container, "GRAPHICS", fonts, theme);

    let res_idx = resolution_index(graphics.resolution);
    spawn_range_slider(
        commands,
        container,
        "Resolution:",
        resolution_slider_value(res_idx),
        resolution_label(graphics.resolution.0, graphics.resolution.1),
        SelectorField::Resolution,
        Some(RESOLUTION_OPTIONS.len()),
        Some(0),
        theme,
    );

    let fs_idx = if graphics.fullscreen { 0 } else { 1 };
    spawn_selector_row_nav(
        commands,
        container,
        "Fullscreen:",
        &["ON", "OFF"],
        fs_idx,
        SelectorField::Fullscreen,
        Some(1),
        theme,
    );

    let vsync_idx = if graphics.vsync { 0 } else { 1 };
    spawn_selector_row_nav(
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
    spawn_selector_row_nav(
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
    spawn_selector_row_nav(
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
    spawn_selector_row_nav(
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
    spawn_selector_row_nav(
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
    spawn_selector_row_nav(
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
    spawn_selector_row_nav(
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
    spawn_selector_row_nav(
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
    spawn_selector_row_nav(
        commands,
        container,
        "Chromatic Aberration:",
        &["Off", "Low", "Medium", "High"],
        chromatic_idx,
        SelectorField::ChromaticAberration,
        Some(10),
        theme,
    );

    let scale_labels: Vec<&str> = UI_SCALE_OPTIONS.iter().map(|&(_, s)| s).collect();
    let scale_idx = UI_SCALE_OPTIONS
        .iter()
        .position(|&(v, _)| (v - graphics.ui_scale).abs() < 0.01)
        .unwrap_or(2);
    spawn_selector_row_nav(
        commands,
        container,
        "UI Scale:",
        &scale_labels,
        scale_idx,
        SelectorField::UiScale,
        Some(11),
        theme,
    );

    // ── Audio Section ──

    spawn_animated_section_divider(commands, container, "AUDIO", fonts, theme);

    spawn_volume_slider(
        commands,
        container,
        "Music Volume:",
        audio_settings.music_volume,
        SelectorField::MusicVolume,
        Some(12),
        theme,
    );

    spawn_volume_slider(
        commands,
        container,
        "SFX Volume:",
        audio_settings.sfx_volume,
        SelectorField::SfxVolume,
        Some(13),
        theme,
    );

    // ── Apply Button ──

    let apply_btn = spawn_styled_button_nav(
        commands,
        "APPLY",
        MenuButton(MenuAction::ApplySettings),
        true,
        fonts,
        Some(14),
        theme,
    );
    commands.entity(container).add_child(apply_btn);
}

// ── Apply Settings ──

pub(crate) fn apply_graphics_settings(
    graphics: &GraphicsSettings,
    window: &mut Window,
) {
    let (w, h) = graphics.resolution;

    window.mode = if graphics.fullscreen {
        bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Current)
    } else {
        bevy::window::WindowMode::Windowed
    };

    // Set logical resolution so the window size matches what the user selected,
    // regardless of display scale factor (Retina 2x, Windows HiDPI 1.5x, etc.).
    // set() takes logical dimensions; Bevy multiplies by scale_factor for physical.
    window.resolution.set(w as f32, h as f32);
    window.present_mode = if graphics.vsync {
        PresentMode::AutoVsync
    } else {
        PresentMode::AutoNoVsync
    };
}

// ── Selector Handling ──

pub(crate) fn apply_selector_change(
    field: &SelectorField,
    index: usize,
    graphics: &mut GraphicsSettings,
) {
    match field {
        SelectorField::Resolution => {
            if index < RESOLUTION_OPTIONS.len() {
                graphics.resolution = RESOLUTION_OPTIONS[index];
            }
        }
        SelectorField::Fullscreen => {
            graphics.fullscreen = index == 0;
        }
        SelectorField::Vsync => {
            graphics.vsync = index == 0;
        }
        SelectorField::Shadows => {
            graphics.shadow_quality = match index {
                0 => ShadowQuality::Off,
                1 => ShadowQuality::Low,
                _ => ShadowQuality::High,
            };
        }
        SelectorField::EntityLights => {
            graphics.entity_lights = index == 0;
        }
        SelectorField::AntiAliasing => {
            graphics.anti_aliasing = match index {
                0 => AntiAliasingMode::Off,
                _ => AntiAliasingMode::Smaa,
            };
        }
        SelectorField::Bloom => {
            graphics.bloom = match index {
                0 => EffectQuality::Off,
                1 => EffectQuality::Low,
                2 => EffectQuality::Medium,
                _ => EffectQuality::High,
            };
        }
        SelectorField::Brightness => {
            if index < BRIGHTNESS_OPTIONS.len() {
                graphics.brightness = BRIGHTNESS_OPTIONS[index].0;
            }
        }
        SelectorField::AutoExposure => {
            graphics.auto_exposure = index == 0;
        }
        SelectorField::DepthOfField => {
            graphics.depth_of_field = match index {
                0 => EffectQuality::Off,
                1 => EffectQuality::Low,
                2 => EffectQuality::Medium,
                _ => EffectQuality::High,
            };
        }
        SelectorField::ChromaticAberration => {
            graphics.chromatic_aberration = match index {
                0 => EffectQuality::Off,
                1 => EffectQuality::Low,
                2 => EffectQuality::Medium,
                _ => EffectQuality::High,
            };
        }
        SelectorField::UiScale => {
            if index < UI_SCALE_OPTIONS.len() {
                graphics.ui_scale = UI_SCALE_OPTIONS[index].0;
            }
        }
        SelectorField::ThemeMode => {
            graphics.theme_mode = match index {
                0 => crate::theme::ThemeMode::Dark,
                _ => crate::theme::ThemeMode::Light,
            };
        }
        _ => {}
    }
}
