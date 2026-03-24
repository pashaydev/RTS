use bevy::prelude::*;

use crate::components::*;
use crate::theme;

use super::interactions::{UiInteractPhase, UiInteractState};

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

pub fn button_hover_visual(
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
                        *bg = BackgroundColor(theme::BG_ELEVATED);
                        *bc = BorderColor::all(Color::srgba(0.29, 0.62, 1.0, 0.5));
                    }
                    UiInteractPhase::Pressed => {
                        *bg = BackgroundColor(theme::BTN_PRESSED);
                        *bc = BorderColor::all(Color::srgba(
                            0.29,
                            0.62,
                            1.0,
                            0.7 + 0.3 * state.hold_progress,
                        ));
                    }
                    UiInteractPhase::Idle | UiInteractPhase::Disabled => {
                        *bg = BackgroundColor(theme::BG_SURFACE);
                        *bc = BorderColor::all(Color::NONE);
                    }
                }
            }
        } else {
            *bg = match state.phase {
                UiInteractPhase::Pressed => BackgroundColor(theme::BTN_PRESSED),
                UiInteractPhase::Hovered => BackgroundColor(theme::BTN_HOVER),
                UiInteractPhase::Idle | UiInteractPhase::Disabled => {
                    BackgroundColor(theme::BTN_PRIMARY)
                }
            };
        }
    }
}

pub fn animated_button_chrome_system(
    mut query: Query<
        (
            &UiInteractState,
            &ButtonStyle,
            Option<&mut BorderColor>,
            Option<&mut BoxShadow>,
        ),
        With<ButtonAnimState>,
    >,
) {
    for (state, style, border_color, shadow) in &mut query {
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
                },
                UiInteractPhase::Hovered => match style {
                    ButtonStyle::Filled => Color::srgba(0.55, 0.74, 0.95, 0.28),
                    ButtonStyle::Ghost => Color::srgba(0.42, 0.70, 1.0, 0.24),
                    ButtonStyle::Destructive => Color::srgba(0.95, 0.45, 0.45, 0.24),
                },
                UiInteractPhase::Idle | UiInteractPhase::Disabled => match style {
                    ButtonStyle::Filled => Color::srgba(0.35, 0.48, 0.65, 0.14 + 0.10 * state.click_flash),
                    ButtonStyle::Ghost => Color::srgba(0.29, 0.62, 1.0, 0.10 + 0.12 * state.click_flash),
                    ButtonStyle::Destructive => {
                        Color::srgba(0.85, 0.32, 0.32, 0.10 + 0.12 * state.click_flash)
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
                UiInteractPhase::Idle | UiInteractPhase::Disabled => {
                    (0.10 + 0.18 * state.click_flash, 10.0 + 6.0 * state.click_flash, 2.0)
                }
            };

            let tint = match style {
                ButtonStyle::Filled | ButtonStyle::Ghost => Color::srgba(0.29, 0.62, 1.0, alpha),
                ButtonStyle::Destructive => Color::srgba(0.85, 0.32, 0.32, alpha),
            };
            *shadow = BoxShadow::new(
                tint,
                Val::Px(0.0),
                Val::Px(y),
                Val::Px(0.0),
                Val::Px(blur),
            );
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
    )>,
) {
    let dt = time.delta_secs();
    let speed = 14.0_f32;
    let alpha = 1.0 - (-speed * dt).exp();

    for (state, mut anim, style, mut bg, mut transform) in &mut query {
        match state.phase {
            UiInteractPhase::Hovered => {
                anim.scale_target = 1.04;
                anim.lift_target = 2.0;
                match style {
                    ButtonStyle::Filled => {
                        anim.bg_target = [0.25, 0.25, 0.25, 0.94];
                    }
                    ButtonStyle::Ghost => {
                        anim.bg_target = [0.29, 0.62, 1.0, 0.08];
                    }
                    ButtonStyle::Destructive => {
                        anim.bg_target = [0.80, 0.27, 0.27, 0.08];
                    }
                }
            }
            UiInteractPhase::Pressed => {
                anim.scale_target = 0.96 + 0.02 * state.hold_progress;
                anim.lift_target = 0.0;
                match style {
                    ButtonStyle::Filled => {
                        anim.bg_target = [
                            lerp(0.12, 0.18, state.hold_progress),
                            lerp(0.12, 0.18, state.hold_progress),
                            lerp(0.12, 0.22, state.hold_progress),
                            0.94,
                        ];
                    }
                    ButtonStyle::Ghost => {
                        anim.bg_target = [0.29, 0.62, 1.0, lerp(0.14, 0.24, state.hold_progress)];
                    }
                    ButtonStyle::Destructive => {
                        anim.bg_target =
                            [0.80, 0.27, 0.27, lerp(0.14, 0.24, state.hold_progress)];
                    }
                }
            }
            UiInteractPhase::Idle | UiInteractPhase::Disabled => {
                anim.scale_target = 1.0 + 0.025 * state.click_flash;
                anim.lift_target = 1.5 * state.click_flash;
                match style {
                    ButtonStyle::Filled => {
                        anim.bg_target = [0.17, 0.17, 0.17, 0.94];
                    }
                    ButtonStyle::Ghost | ButtonStyle::Destructive => {
                        anim.bg_target = [0.0, 0.0, 0.0, 0.0];
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
