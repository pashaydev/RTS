use bevy::prelude::*;
use bevy::window::{Monitor, PresentMode, PrimaryMonitor};
use std::collections::BTreeSet;

use super::helpers::*;
use super::*;
use crate::components::*;
use crate::theme::Theme;
use crate::ui::fonts::UiFonts;

// ── Resolution Helpers ──

fn resolution_label(w: u32, h: u32) -> String {
    format!("{w}x{h}")
}

pub(crate) fn resolution_index(resolutions: &[(u32, u32)], resolution: (u32, u32)) -> usize {
    resolutions
        .iter()
        .position(|&r| r == resolution)
        .unwrap_or_else(|| {
            // Find closest by pixel count
            let target = resolution.0 as u64 * resolution.1 as u64;
            resolutions
                .iter()
                .enumerate()
                .min_by_key(|(_, &(w, h))| {
                    ((w as u64 * h as u64) as i64 - target as i64).unsigned_abs()
                })
                .map(|(i, _)| i)
                .unwrap_or(0)
        })
}

pub(crate) fn step_resolution_index(
    resolutions: &[(u32, u32)],
    current_index: usize,
    delta: isize,
) -> usize {
    let max_index = resolutions.len().saturating_sub(1) as isize;
    (current_index as isize + delta).clamp(0, max_index) as usize
}

/// Check if a resolution has a standard display aspect ratio (16:9, 16:10, 21:9, 4:3, 5:4, 32:9).
fn is_standard_aspect_ratio(w: u32, h: u32) -> bool {
    if h == 0 {
        return false;
    }
    let ratio = w as f64 / h as f64;
    const STANDARD_RATIOS: &[f64] = &[
        16.0 / 9.0,  // 1.778 — 1920x1080, 2560x1440, 3840x2160
        16.0 / 10.0, // 1.600 — 1920x1200, 2560x1600
        21.0 / 9.0,  // 2.333 — 3440x1440, 2560x1080
        4.0 / 3.0,   // 1.333 — 1024x768, 1600x1200
        5.0 / 4.0,   // 1.250 — 1280x1024
        32.0 / 9.0,  // 3.556 — 5120x1440 (super ultrawide)
    ];
    // Allow ~3% tolerance to account for rounding after scale factor division
    STANDARD_RATIOS.iter().any(|&std| (ratio - std).abs() / std < 0.03)
}

// ── Marker for the resolution row (greyed out when fullscreen) ──

#[derive(Component)]
pub(crate) struct ResolutionRow;

/// Inserted after monitor resolutions have been queried to avoid re-running.
#[derive(Resource)]
pub(crate) struct ResolutionsPopulated;

// ── Populate Available Resolutions from Monitor ──

pub(crate) fn populate_available_resolutions(
    mut commands: Commands,
    monitors: Query<&Monitor, With<PrimaryMonitor>>,
    mut resolutions: ResMut<AvailableResolutions>,
) {
    let Ok(monitor) = monitors.single() else {
        return;
    };

    commands.insert_resource(ResolutionsPopulated);

    let scale = monitor.scale_factor.max(1.0);
    let native_w = (monitor.physical_width as f64 / scale).round() as u32;
    let native_h = (monitor.physical_height as f64 / scale).round() as u32;

    let mut res_set: BTreeSet<(u64, u32, u32)> = BTreeSet::new();

    // Collect unique resolutions from monitor video modes, converting to logical pixels.
    for vm in &monitor.video_modes {
        let w = (vm.physical_size.x as f64 / scale).round() as u32;
        let h = (vm.physical_size.y as f64 / scale).round() as u32;
        if w >= 1024 && h >= 720 && is_standard_aspect_ratio(w, h) {
            res_set.insert((w as u64 * h as u64, w, h));
        }
    }

    // Always include the native logical resolution (skip aspect ratio check).
    if native_w >= 1024 && native_h >= 720 {
        res_set.insert((native_w as u64 * native_h as u64, native_w, native_h));
    }

    // Add common resolutions that fit within the native size as fallbacks.
    // On HiDPI displays, video modes divided by scale factor often produce
    // non-standard values, leaving very few options. These ensure a usable list.
    const COMMON: &[(u32, u32)] = &[
        (1280, 720),
        (1280, 800),
        (1366, 768),
        (1440, 900),
        (1600, 900),
        (1600, 1000),
        (1680, 1050),
        (1920, 1080),
        (1920, 1200),
        (2560, 1440),
        (2560, 1600),
        (3440, 1440),
        (3840, 2160),
    ];
    for &(w, h) in COMMON {
        if w <= native_w && h <= native_h {
            res_set.insert((w as u64 * h as u64, w, h));
        }
    }

    if !res_set.is_empty() {
        resolutions.0 = res_set.into_iter().map(|(_, w, h)| (w, h)).collect();
    }
}

// ── Detect native resolution for first-launch default ──

pub(crate) fn detect_native_resolution(
    monitors: Query<&Monitor, With<PrimaryMonitor>>,
    mut graphics: ResMut<GraphicsSettings>,
    resolutions: Res<AvailableResolutions>,
) {
    // Only override the default 1280x720 — if the user already changed it, respect that
    if graphics.resolution != (1280, 720) {
        return;
    }

    let Ok(monitor) = monitors.single() else {
        return;
    };

    let scale = monitor.scale_factor.max(1.0);
    let native_w = (monitor.physical_width as f64 / scale).round() as u32;
    let native_h = (monitor.physical_height as f64 / scale).round() as u32;

    // Pick the closest available resolution to the native one
    let idx = resolution_index(&resolutions.0, (native_w, native_h));
    if idx < resolutions.0.len() {
        graphics.resolution = resolutions.0[idx];
    }
}

// ── Options Page ──

pub(crate) fn spawn_options_page(
    commands: &mut Commands,
    container: Entity,
    graphics: &GraphicsSettings,
    audio_settings: &crate::audio::AudioSettings,
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

    let fs_idx = if graphics.fullscreen { 0 } else { 1 };
    spawn_selector_row_nav(
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

    // ── Gameplay Section ──

    spawn_animated_section_divider(commands, container, "GAMEPLAY", fonts, theme);

    let reset_btn = spawn_styled_button_nav(
        commands,
        "RESET WIDGET LAYOUT",
        MenuButton(MenuAction::ResetWidgetLayout),
        false,
        fonts,
        Some(14),
        theme,
    );
    commands.entity(container).add_child(reset_btn);

    // ── Apply Button ──

    let apply_btn = spawn_styled_button_nav(
        commands,
        "APPLY",
        MenuButton(MenuAction::ApplySettings),
        true,
        fonts,
        Some(15),
        theme,
    );
    commands.entity(container).add_child(apply_btn);
}

// ── Apply Settings ──

pub(crate) fn apply_graphics_settings(graphics: &GraphicsSettings, window: &mut Window) {
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

// ── Toggle resolution row visibility when fullscreen changes ──

pub(crate) fn toggle_resolution_row_visibility(
    graphics: Res<GraphicsSettings>,
    res_rows: Query<Entity, With<ResolutionRow>>,
    children_q: Query<&Children>,
    selectors: Query<&MenuSelector>,
    arrow_labels: Query<&ArrowSelectorLabel>,
    mut text_colors: Query<&mut TextColor>,
    mut bg_colors: Query<&mut BackgroundColor>,
    mut border_colors: Query<&mut BorderColor>,
    value_bgs: Query<&ArrowSelectorValueBg>,
    theme: Res<Theme>,
) {
    if !graphics.is_changed() {
        return;
    }
    let disabled = graphics.fullscreen;
    for row_entity in &res_rows {
        // Recursively update text colors on all children to show greyed-out state
        let mut stack = vec![row_entity];
        while let Some(entity) = stack.pop() {
            if let Ok(mut tc) = text_colors.get_mut(entity) {
                tc.0 = if disabled {
                    theme.colors.text_disabled
                } else if arrow_labels.get(entity).is_ok() {
                    // Value label between arrows
                    Color::WHITE
                } else if selectors.get(entity).is_ok() {
                    // Arrow button text
                    theme.colors.text_secondary
                } else {
                    theme.colors.text_secondary
                };
            }
            // Update value background opacity when disabled
            if value_bgs.get(entity).is_ok() {
                if let Ok(mut bg) = bg_colors.get_mut(entity) {
                    bg.0 = if disabled {
                        Color::srgba(0.15, 0.15, 0.15, 0.1)
                    } else {
                        Color::srgba(0.18, 0.40, 0.85, 0.15)
                    };
                }
                if let Ok(mut bc) = border_colors.get_mut(entity) {
                    *bc = BorderColor::all(if disabled {
                        Color::srgba(0.3, 0.3, 0.3, 0.2)
                    } else {
                        Color::srgba(0.29, 0.62, 1.0, 0.3)
                    });
                }
            }
            if let Ok(children) = children_q.get(entity) {
                for child in children.iter() {
                    stack.push(child);
                }
            }
        }
    }
}

// ── Sync Arrow Selector for Resolution ──

pub(crate) fn sync_resolution_arrow_selector(
    graphics: Res<GraphicsSettings>,
    resolutions: Res<AvailableResolutions>,
    mut labels: Query<(&ArrowSelectorLabel, &mut Text)>,
    mut selectors: Query<&mut MenuSelector>,
    arrow_parents: Query<&Children>,
    res_rows: Query<&Children, With<ResolutionRow>>,
) {
    if !graphics.is_changed() {
        return;
    }

    let total = resolutions.0.len();
    if total == 0 {
        return;
    }
    let current_idx = resolution_index(&resolutions.0, graphics.resolution);
    let (w, h) = resolutions.0.get(current_idx).copied().unwrap_or(graphics.resolution);

    // Update label text
    for (lbl, mut text) in &mut labels {
        if lbl.0 == SelectorField::Resolution {
            **text = resolution_label(w, h);
        }
    }

    // Update arrow button indices — find MenuSelector children in ResolutionRow
    for row_children in &res_rows {
        for child in row_children.iter() {
            // The actual arrow buttons are nested inside the NavFocusable row
            if let Ok(inner_children) = arrow_parents.get(child) {
                let mut arrow_selectors: Vec<Entity> = Vec::new();
                for inner_child in inner_children.iter() {
                    if let Ok(sel) = selectors.get(inner_child) {
                        if sel.field == SelectorField::Resolution {
                            arrow_selectors.push(inner_child);
                        }
                    }
                }
                // First arrow = prev, second arrow = next (clamped — no wrap)
                if arrow_selectors.len() == 2 {
                    let prev_idx = current_idx.saturating_sub(1);
                    let next_idx = (current_idx + 1).min(total.saturating_sub(1));
                    if let Ok(mut sel) = selectors.get_mut(arrow_selectors[0]) {
                        sel.index = prev_idx;
                    }
                    if let Ok(mut sel) = selectors.get_mut(arrow_selectors[1]) {
                        sel.index = next_idx;
                    }
                }
            }
        }
    }
}

// ── Selector Handling ──

pub(crate) fn apply_selector_change(
    field: &SelectorField,
    index: usize,
    graphics: &mut GraphicsSettings,
    resolutions: &AvailableResolutions,
) {
    match field {
        SelectorField::Resolution => {
            if index < resolutions.0.len() {
                graphics.resolution = resolutions.0[index];
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
