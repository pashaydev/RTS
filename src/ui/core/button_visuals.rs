use bevy::prelude::*;

use crate::components::*;
use crate::menu::helpers::SelectedOption;
use crate::theme::Theme;

use super::interactions::{UiInteractPhase, UiInteractState};

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

pub fn button_hover_visual(
    theme: Res<Theme>,
    mut query: Query<
        (
            &UiInteractState,
            &mut BackgroundColor,
            Option<&mut BorderColor>,
            Has<UnitCardRef>,
        ),
        (
            Changed<UiInteractState>,
            With<StandardButton>,
            Without<ButtonAnimState>,
        ),
    >,
) {
    for (state, mut bg, border_color, is_mini_card) in &mut query {
        if is_mini_card {
            if let Some(mut bc) = border_color {
                match state.phase {
                    UiInteractPhase::Hovered => {
                        *bg = BackgroundColor(theme.colors.bg_elevated);
                        *bc = BorderColor::all(theme.colors.accent.with_alpha(0.5));
                    }
                    UiInteractPhase::Pressed => {
                        *bg = BackgroundColor(theme.colors.btn_pressed);
                        *bc = BorderColor::all(
                            theme
                                .colors
                                .accent
                                .with_alpha(0.7 + 0.3 * state.hold_progress),
                        );
                    }
                    UiInteractPhase::Idle | UiInteractPhase::Disabled => {
                        *bg = BackgroundColor(theme.colors.bg_surface);
                        *bc = BorderColor::all(Color::NONE);
                    }
                }
            }
        } else {
            *bg = match state.phase {
                UiInteractPhase::Pressed => BackgroundColor(theme.colors.btn_pressed),
                UiInteractPhase::Hovered => BackgroundColor(theme.colors.btn_hover),
                UiInteractPhase::Idle | UiInteractPhase::Disabled => {
                    BackgroundColor(theme.colors.btn_primary)
                }
            };
        }
    }
}

pub fn animated_button_chrome_system(
    theme: Res<Theme>,
    mut query: Query<
        (
            &UiInteractState,
            &ButtonStyle,
            Option<&mut BorderColor>,
            Option<&mut BoxShadow>,
            Has<SelectedOption>,
        ),
        With<ButtonAnimState>,
    >,
) {
    for (state, style, border_color, shadow, is_selected) in &mut query {
        // Selected buttons have their border/shadow managed by update_selector_visuals.
        if is_selected {
            continue;
        }
        if let Some(mut border) = border_color {
            *border = BorderColor::all(match state.phase {
                UiInteractPhase::Pressed => match style {
                    ButtonStyle::Filled => {
                        Color::srgba(0.62, 0.82, 1.0, 0.48 + 0.32 * state.hold_progress)
                    }
                    ButtonStyle::Ghost => {
                        Color::srgba(0.52, 0.78, 1.0, 0.42 + 0.30 * state.hold_progress)
                    }
                    ButtonStyle::Destructive => {
                        Color::srgba(1.0, 0.52, 0.52, 0.42 + 0.30 * state.hold_progress)
                    }
                    ButtonStyle::Accent => {
                        Color::srgba(0.91, 0.76, 0.46, 0.25 + 0.15 * state.hold_progress)
                    }
                },
                UiInteractPhase::Hovered => match style {
                    ButtonStyle::Filled => Color::srgba(0.55, 0.74, 0.95, 0.28),
                    ButtonStyle::Ghost => Color::srgba(0.42, 0.70, 1.0, 0.24),
                    ButtonStyle::Destructive => Color::srgba(0.95, 0.45, 0.45, 0.24),
                    ButtonStyle::Accent => Color::srgba(0.91, 0.76, 0.46, 0.14),
                },
                UiInteractPhase::Idle | UiInteractPhase::Disabled => match style {
                    ButtonStyle::Filled => {
                        Color::srgba(0.35, 0.48, 0.65, 0.14 + 0.10 * state.click_flash)
                    }
                    ButtonStyle::Ghost => theme
                        .colors
                        .accent
                        .with_alpha(0.10 + 0.12 * state.click_flash),
                    ButtonStyle::Destructive => {
                        Color::srgba(0.85, 0.32, 0.32, 0.10 + 0.12 * state.click_flash)
                    }
                    ButtonStyle::Accent => {
                        Color::srgba(0.91, 0.76, 0.46, 0.08 + 0.06 * state.click_flash)
                    }
                },
            });
        }

        if let Some(mut shadow) = shadow {
            let (alpha, blur, y) = match state.phase {
                UiInteractPhase::Pressed => (
                    0.24 + 0.28 * state.hold_progress,
                    10.0 + 10.0 * state.hold_progress,
                    1.0,
                ),
                UiInteractPhase::Hovered => (0.20, 14.0, 4.0),
                UiInteractPhase::Idle | UiInteractPhase::Disabled => (
                    0.10 + 0.18 * state.click_flash,
                    10.0 + 6.0 * state.click_flash,
                    2.0,
                ),
            };

            let tint = match style {
                ButtonStyle::Filled | ButtonStyle::Ghost => theme.colors.accent.with_alpha(alpha),
                ButtonStyle::Destructive => Color::srgba(0.85, 0.32, 0.32, alpha),
                ButtonStyle::Accent => theme.colors.prestige.with_alpha(alpha * 0.35),
            };
            *shadow = BoxShadow::new(tint, Val::Px(0.0), Val::Px(y), Val::Px(0.0), Val::Px(blur));
        }
    }
}

/// Smooth lerp-based button animation
pub fn animated_button_hover_system(
    time: Res<Time>,
    mut query: Query<(
        &UiInteractState,
        &mut ButtonAnimState,
        &ButtonStyle,
        &mut BackgroundColor,
        &mut Transform,
        Has<SelectedOption>,
    )>,
) {
    let dt = time.delta_secs();
    let speed = 14.0_f32;
    let alpha = 1.0 - (-speed * dt).exp();

    for (state, mut anim, style, mut bg, mut transform, is_selected) in &mut query {
        match state.phase {
            UiInteractPhase::Hovered => {
                anim.scale_target = 1.02;
                anim.lift_target = 1.5;
                // Don't override bg_target for selected buttons — their color is
                // managed by update_selector_visuals to keep the accent highlight.
                if !is_selected {
                    match style {
                        ButtonStyle::Filled => {
                            anim.bg_target = [0.30, 0.35, 0.45, 0.25];
                        }
                        ButtonStyle::Ghost => {
                            anim.bg_target = [0.29, 0.62, 1.0, 0.10];
                        }
                        ButtonStyle::Destructive => {
                            anim.bg_target = [0.80, 0.27, 0.27, 0.10];
                        }
                        ButtonStyle::Accent => {
                            anim.bg_target = [0.12, 0.12, 0.14, 0.85];
                        }
                    }
                }
            }
            UiInteractPhase::Pressed => {
                anim.scale_target = 0.97 + 0.02 * state.hold_progress;
                anim.lift_target = 0.0;
                if !is_selected {
                    match style {
                        ButtonStyle::Filled => {
                            anim.bg_target = [
                                lerp(0.35, 0.40, state.hold_progress),
                                lerp(0.45, 0.50, state.hold_progress),
                                lerp(0.60, 0.65, state.hold_progress),
                                0.35,
                            ];
                        }
                        ButtonStyle::Ghost => {
                            anim.bg_target =
                                [0.29, 0.62, 1.0, lerp(0.18, 0.28, state.hold_progress)];
                        }
                        ButtonStyle::Destructive => {
                            anim.bg_target =
                                [0.80, 0.27, 0.27, lerp(0.18, 0.28, state.hold_progress)];
                        }
                        ButtonStyle::Accent => {
                            anim.bg_target = [
                                lerp(0.10, 0.14, state.hold_progress),
                                lerp(0.10, 0.13, state.hold_progress),
                                lerp(0.12, 0.16, state.hold_progress),
                                0.90,
                            ];
                        }
                    }
                }
            }
            UiInteractPhase::Idle | UiInteractPhase::Disabled => {
                anim.scale_target = 1.0 + 0.025 * state.click_flash;
                anim.lift_target = 1.5 * state.click_flash;
                // Don't override bg_target for selected buttons.
                if !is_selected {
                    match style {
                        ButtonStyle::Filled => {
                            anim.bg_target = [0.15, 0.15, 0.15, 0.0];
                        }
                        ButtonStyle::Ghost | ButtonStyle::Destructive => {
                            anim.bg_target = [0.0, 0.0, 0.0, 0.0];
                        }
                        ButtonStyle::Accent => {
                            anim.bg_target = [0.08, 0.08, 0.09, 0.75];
                        }
                    }
                }
            }
        }

        for i in 0..4 {
            anim.bg_current[i] += (anim.bg_target[i] - anim.bg_current[i]) * alpha;
        }
        anim.scale_current += (anim.scale_target - anim.scale_current) * alpha;
        anim.lift_current += (anim.lift_target - anim.lift_current) * alpha;

        *bg = BackgroundColor(Color::srgba(
            anim.bg_current[0],
            anim.bg_current[1],
            anim.bg_current[2],
            anim.bg_current[3],
        ));
        transform.scale = Vec3::splat(anim.scale_current);
        transform.translation.y = anim.lift_current;
    }
}
