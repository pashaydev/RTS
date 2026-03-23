use bevy::prelude::*;

use crate::components::*;
use crate::theme;

pub fn button_hover_visual(
    mut query: Query<
        (
            &Interaction,
            &mut BackgroundColor,
            Option<&mut BorderColor>,
            Has<UnitCardRef>,
        ),
        (
            Changed<Interaction>,
            With<StandardButton>,
            Without<ButtonAnimState>,
        ),
    >,
) {
    for (interaction, mut bg, border_color, is_mini_card) in &mut query {
        if is_mini_card {
            if let Some(mut bc) = border_color {
                match interaction {
                    Interaction::Hovered => {
                        *bg = BackgroundColor(theme::BG_ELEVATED);
                        *bc = BorderColor::all(Color::srgba(0.29, 0.62, 1.0, 0.5));
                    }
                    Interaction::Pressed => {
                        *bg = BackgroundColor(theme::BTN_PRESSED);
                        *bc = BorderColor::all(theme::ACCENT);
                    }
                    Interaction::None => {
                        *bg = BackgroundColor(theme::BG_SURFACE);
                        *bc = BorderColor::all(Color::NONE);
                    }
                }
            }
        } else {
            *bg = match interaction {
                Interaction::Pressed => BackgroundColor(theme::BTN_PRESSED),
                Interaction::Hovered => BackgroundColor(theme::BTN_HOVER),
                Interaction::None => BackgroundColor(theme::BTN_PRIMARY),
            };
        }
    }
}

/// Smooth lerp-based button animation
pub fn animated_button_hover_system(
    time: Res<Time>,
    mut query: Query<(
        &Interaction,
        &mut ButtonAnimState,
        &ButtonStyle,
        &mut BackgroundColor,
        &mut Transform,
    )>,
) {
    let dt = time.delta_secs();
    let speed = 14.0_f32;
    let alpha = 1.0 - (-speed * dt).exp();

    for (interaction, mut anim, style, mut bg, mut transform) in &mut query {
        match interaction {
            Interaction::Hovered => {
                anim.scale_target = 1.04;
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
            Interaction::Pressed => {
                anim.scale_target = 0.96;
                match style {
                    ButtonStyle::Filled => {
                        anim.bg_target = [0.12, 0.12, 0.12, 0.94];
                    }
                    ButtonStyle::Ghost => {
                        anim.bg_target = [0.29, 0.62, 1.0, 0.14];
                    }
                    ButtonStyle::Destructive => {
                        anim.bg_target = [0.80, 0.27, 0.27, 0.14];
                    }
                }
            }
            Interaction::None => {
                anim.scale_target = 1.0;
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

        *bg = BackgroundColor(Color::srgba(
            anim.bg_current[0],
            anim.bg_current[1],
            anim.bg_current[2],
            anim.bg_current[3],
        ));
        transform.scale = Vec3::splat(anim.scale_current);
    }
}
