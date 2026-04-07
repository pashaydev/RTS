use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use bevy::window::PresentMode;

use super::{ResolutionRow, resolution_index, resolution_label};
use crate::components::*;
use crate::menu::helpers::*;
use crate::menu::{BRIGHTNESS_OPTIONS, UI_SCALE_OPTIONS};
use crate::theme::{TEXT_PRIMARY, BG_ELEVATED, TEXT_DISABLED, HIGHLIGHT, HIGHLIGHT_SUBTLE};
use crate::theme::Theme;

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
                    TEXT_PRIMARY
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
                        BG_ELEVATED.with_alpha(0.1)
                    } else {
                        HIGHLIGHT_SUBTLE
                    };
                }
                if let Ok(mut bc) = border_colors.get_mut(entity) {
                    *bc = BorderColor::all(if disabled {
                        TEXT_DISABLED.with_alpha(0.2)
                    } else {
                        HIGHLIGHT
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

// ── Volume Slider Interaction ──

/// Handles click and drag on volume slider tracks.
///
/// Uses `RelativeCursorPosition` to map cursor to 0.0–1.0 within the track.
pub(crate) fn volume_slider_system(
    mouse: Res<ButtonInput<MouseButton>>,
    sliders: Query<(Entity, &RangeSlider, &Interaction, &RelativeCursorPosition)>,
    mut fills: Query<(&ChildOf, &mut Node), With<RangeSliderFill>>,
    mut labels: Query<(&RangeSliderLabel, &mut Text)>,
    mut audio_settings: ResMut<crate::audio::AudioSettings>,
    mut drag: ResMut<SliderDragState>,
) {
    // On release, stop dragging.
    if mouse.just_released(MouseButton::Left) {
        if drag.active.is_some() {
            drag.active = None;
        }
        return;
    }

    // Determine which slider is active.
    let active_slider = if let Some(active) = drag.active {
        if mouse.pressed(MouseButton::Left) {
            Some(active)
        } else {
            None
        }
    } else if mouse.just_pressed(MouseButton::Left) {
        sliders
            .iter()
            .find(|(_, _, interaction, _)| **interaction == Interaction::Pressed)
            .map(|(entity, _, _, _)| entity)
    } else {
        None
    };

    let Some(slider_entity) = active_slider else {
        return;
    };
    drag.active = Some(slider_entity);

    let Ok((_, slider, _, rel_cursor)) = sliders.get(slider_entity) else {
        return;
    };

    // RelativeCursorPosition: (0,0) = center, (-0.5,-0.5) = top-left, (0.5,0.5) = bottom-right.
    // Convert to 0.0–1.0 range: add 0.5 to the x component.
    let Some(normalized) = rel_cursor.normalized else {
        return;
    };
    let t = (normalized.x + 0.5).clamp(0.0, 1.0);

    let (pct, value_label) = match slider.field {
        SelectorField::MusicVolume => {
            let value = (t * 100.0).round() / 100.0;
            audio_settings.music_volume = value;
            let pct = value * 100.0;
            (pct, format!("{pct:.0}%"))
        }
        SelectorField::SfxVolume => {
            let value = (t * 100.0).round() / 100.0;
            audio_settings.sfx_volume = value;
            let pct = value * 100.0;
            (pct, format!("{pct:.0}%"))
        }
        _ => return,
    };

    // Update fill bar width and label.
    for (parent, mut node) in fills.iter_mut() {
        if parent.parent() == slider_entity {
            node.width = Val::Percent(pct);
        }
    }
    let field = slider.field;
    for (lbl, mut text) in labels.iter_mut() {
        if lbl.0 == field {
            **text = value_label.clone();
        }
    }
}

pub(crate) fn sync_range_slider_visuals(
    audio_settings: Res<crate::audio::AudioSettings>,
    sliders: Query<(Entity, &RangeSlider)>,
    mut fills: Query<(&ChildOf, &mut Node), With<RangeSliderFill>>,
    mut labels: Query<(&RangeSliderLabel, &mut Text)>,
) {
    if !audio_settings.is_changed() {
        return;
    }

    for (slider_entity, slider) in &sliders {
        let (pct, value_label) = match slider.field {
            SelectorField::MusicVolume => {
                let pct = (audio_settings.music_volume * 100.0).round();
                (pct, format!("{pct:.0}%"))
            }
            SelectorField::SfxVolume => {
                let pct = (audio_settings.sfx_volume * 100.0).round();
                (pct, format!("{pct:.0}%"))
            }
            _ => continue,
        };

        for (parent, mut node) in fills.iter_mut() {
            if parent.parent() == slider_entity {
                node.width = Val::Percent(pct);
            }
        }

        for (label, mut text) in labels.iter_mut() {
            if label.0 == slider.field {
                **text = value_label.clone();
            }
        }
    }
}
