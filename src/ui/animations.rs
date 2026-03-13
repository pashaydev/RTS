use bevy::prelude::*;

use crate::components::*;
use crate::theme;

// ── Easing Functions ──

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

fn ease_out_elastic(t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let c4 = (2.0 * std::f32::consts::PI) / 3.0;
    2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
}

fn ease_in_out_quart(t: f32) -> f32 {
    if t < 0.5 {
        8.0 * t * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(4) / 2.0
    }
}

// ── Fade System ──

/// Ticks UiFadeIn components, adjusting BackgroundColor alpha.
/// When complete, removes the component.
pub fn ui_fade_system(
    mut commands: Commands,
    time: Res<Time>,
    mut fade_ins: Query<(Entity, &mut UiFadeIn, &mut BackgroundColor), Without<UiFadeOut>>,
    mut fade_outs: Query<(Entity, &mut UiFadeOut, &mut BackgroundColor)>,
) {
    for (entity, mut fade, mut bg) in &mut fade_ins {
        fade.timer.tick(time.delta());
        let t = ease_out_cubic(fade.timer.fraction());
        let mut color = bg.0;
        color.set_alpha(t);
        *bg = BackgroundColor(color);

        if fade.timer.is_finished() {
            let mut color = bg.0;
            color.set_alpha(1.0);
            *bg = BackgroundColor(color);
            commands.entity(entity).remove::<UiFadeIn>();
        }
    }

    for (entity, mut fade, mut bg) in &mut fade_outs {
        fade.timer.tick(time.delta());
        let t = 1.0 - ease_out_cubic(fade.timer.fraction());
        let mut color = bg.0;
        color.set_alpha(t.max(0.0));
        *bg = BackgroundColor(color);

        if fade.timer.is_finished() {
            commands.entity(entity).try_despawn();
        }
    }
}

// ── Slide System ──

/// Ticks UiSlideIn components, adjusting Node margin offset.
/// Uses ease_out_back for a subtle overshoot feel.
pub fn ui_slide_system(
    mut commands: Commands,
    time: Res<Time>,
    mut slides: Query<(Entity, &mut UiSlideIn, &mut Node)>,
) {
    for (entity, mut slide, mut node) in &mut slides {
        slide.timer.tick(time.delta());
        let t = ease_out_back(slide.timer.fraction());
        let remaining = 1.0 - t;

        node.margin.left = Val::Px(slide.offset.x * remaining);
        node.margin.top = Val::Px(slide.offset.y * remaining);

        if slide.timer.is_finished() {
            node.margin.left = Val::Px(0.0);
            node.margin.top = Val::Px(0.0);
            commands.entity(entity).remove::<UiSlideIn>();
        }
    }
}

// ── Scale In System ──

/// Scales a UI node from `from` to 1.0 with optional elastic easing.
pub fn ui_scale_in_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut UiScaleIn, &mut Transform)>,
) {
    for (entity, mut scale_in, mut transform) in &mut query {
        scale_in.timer.tick(time.delta());
        let t = if scale_in.elastic {
            ease_out_elastic(scale_in.timer.fraction())
        } else {
            ease_out_back(scale_in.timer.fraction())
        };
        let s = scale_in.from + (1.0 - scale_in.from) * t;
        transform.scale = Vec3::splat(s);

        if scale_in.timer.is_finished() {
            transform.scale = Vec3::ONE;
            commands.entity(entity).remove::<UiScaleIn>();
        }
    }
}

// ── Line Expand System ──

/// Expands separator lines from 0 to target width.
pub fn ui_line_expand_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut UiLineExpand, &mut Node)>,
) {
    for (entity, mut expand, mut node) in &mut query {
        expand.timer.tick(time.delta());
        let t = ease_in_out_quart(expand.timer.fraction());
        node.width = Val::Px(expand.target_width * t);

        if expand.timer.is_finished() {
            node.width = Val::Px(expand.target_width);
            commands.entity(entity).remove::<UiLineExpand>();
        }
    }
}

// ── Menu Particle System ──

/// Animates floating background particles with drift and alpha pulsing.
pub fn menu_particle_system(
    time: Res<Time>,
    mut query: Query<(&mut Node, &mut BackgroundColor, &MenuParticle)>,
) {
    let t = time.elapsed_secs();
    for (mut node, mut bg, particle) in &mut query {
        // Drift
        let dx = particle.velocity.x * time.delta_secs();
        let dy = particle.velocity.y * time.delta_secs();
        if let Val::Px(ref mut x) = node.left {
            *x += dx;
        }
        if let Val::Px(ref mut y) = node.top {
            *y += dy;
        }

        // Wrap around screen edges
        if let Val::Px(x) = node.left {
            if x > 110.0 {
                node.left = Val::Percent(-10.0);
            } else if x < -15.0 {
                node.left = Val::Percent(110.0);
            }
        }

        // Alpha pulse
        let alpha = particle.base_alpha
            * (0.5 + 0.5 * (t * 0.8 + particle.phase).sin());
        let srgba = theme::ACCENT.to_srgba();
        bg.0 = Color::srgba(srgba.red, srgba.green, srgba.blue, alpha * 0.3);
    }
}

// ── Title Shimmer System ──

/// Cycles title text color through a subtle shimmer.
pub fn title_shimmer_system(
    time: Res<Time>,
    mut query: Query<(&mut TextColor, &TitleShimmer)>,
) {
    let t = time.elapsed_secs();
    for (mut color, shimmer) in &mut query {
        let phase = t * 1.2 + shimmer.phase_offset;
        // Subtle blue-white shimmer
        let r = 0.85 + 0.15 * (phase * 0.7).sin();
        let g = 0.88 + 0.12 * (phase * 0.9 + 0.5).sin();
        let b = 0.92 + 0.08 * (phase * 1.1 + 1.0).sin();
        let alpha = 0.8 + 0.2 * (phase * std::f32::consts::PI * 0.5).sin().abs();
        color.0 = Color::srgba(r, g, b, alpha);
    }
}

// ── Glow Pulse System ──

/// Pulses a BoxShadow glow on entities with UiGlowPulse.
pub fn ui_glow_pulse_system(
    time: Res<Time>,
    mut query: Query<(&UiGlowPulse, &mut BoxShadow)>,
) {
    let t = time.elapsed_secs();
    for (glow, mut shadow) in &mut query {
        let pulse = 0.4 + 0.6 * (t * 2.0).sin().abs();
        let srgba = glow.color.to_srgba();
        *shadow = BoxShadow::new(
            Color::srgba(
                srgba.red,
                srgba.green,
                srgba.blue,
                srgba.alpha * pulse * glow.intensity,
            ),
            Val::Px(0.0),
            Val::Px(0.0),
            Val::Px(0.0),
            Val::Px(8.0 + 4.0 * pulse),
        );
    }
}
