use bevy::prelude::*;

use crate::components::*;

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
        let t = fade.timer.fraction();
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
        let t = 1.0 - fade.timer.fraction();
        let mut color = bg.0;
        color.set_alpha(t.max(0.0));
        *bg = BackgroundColor(color);

        if fade.timer.is_finished() {
            commands.entity(entity).try_despawn();
        }
    }
}

/// Ticks UiSlideIn components, adjusting Node margin offset.
/// When complete, removes the component.
pub fn ui_slide_system(
    mut commands: Commands,
    time: Res<Time>,
    mut slides: Query<(Entity, &mut UiSlideIn, &mut Node)>,
) {
    for (entity, mut slide, mut node) in &mut slides {
        slide.timer.tick(time.delta());
        let t = ease_out_cubic(slide.timer.fraction());
        let remaining = 1.0 - t;

        // Apply offset as margin that decreases to 0
        node.margin.left = Val::Px(slide.offset.x * remaining);
        node.margin.top = Val::Px(slide.offset.y * remaining);

        if slide.timer.is_finished() {
            node.margin.left = Val::Px(0.0);
            node.margin.top = Val::Px(0.0);
            commands.entity(entity).remove::<UiSlideIn>();
        }
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}
