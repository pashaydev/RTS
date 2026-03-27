use bevy::prelude::*;

use crate::debug::model::{DebugTweaks, TweakValue};
use crate::debug::state::{DebugPanelState, FpsTracker, SaveConfigFeedback};
use crate::debug::ui::components::{
    ColorPreview, DebugDayCycleText, DebugEntityCountText, DebugFpsText, SaveConfigButtonText,
    TweakCycleText, TweakReadOnlyText, TweakSliderFill, TweakSliderKnob, TweakSliderValueText,
    TweakToggle, TweakToggleText,
};
use crate::debug::ui::style::{
    debug_active_surface, debug_control_surface, debug_hover_surface, debug_pressed_surface,
    format_tweak_float,
};
use crate::lighting::DayCycle;

pub fn update_debug_texts(
    tracker: Res<FpsTracker>,
    cycle: Res<DayCycle>,
    entities: Query<Entity>,
    mut fps_q: Query<
        &mut Text,
        (
            With<DebugFpsText>,
            Without<DebugEntityCountText>,
            Without<DebugDayCycleText>,
        ),
    >,
    mut ent_q: Query<
        &mut Text,
        (
            With<DebugEntityCountText>,
            Without<DebugFpsText>,
            Without<DebugDayCycleText>,
        ),
    >,
    mut day_q: Query<
        &mut Text,
        (
            With<DebugDayCycleText>,
            Without<DebugFpsText>,
            Without<DebugEntityCountText>,
        ),
    >,
) {
    if let Ok(mut t) = fps_q.single_mut() {
        let warning = if tracker.fps >= 55.0 {
            ""
        } else if tracker.fps >= 30.0 {
            " (!)"
        } else {
            " (!!)"
        };
        **t = format!(
            "FPS: {:.0}  |  {:.1}ms{}",
            tracker.fps, tracker.frame_time_ms, warning
        );
    }

    if let Ok(mut t) = ent_q.single_mut() {
        **t = format!("Entities: {}", entities.iter().count());
    }

    if let Ok(mut t) = day_q.single_mut() {
        **t = format!(
            "Day: {:.3} ({:?})  |  {:.0}s cycle",
            cycle.time, cycle.phase, cycle.cycle_duration
        );
    }
}

pub fn update_tweak_visuals(
    state: Res<DebugPanelState>,
    tweaks: Res<DebugTweaks>,
    mut fill_q: Query<(&TweakSliderFill, &mut Node), Without<TweakSliderKnob>>,
    mut knob_q: Query<(&TweakSliderKnob, &mut Node), Without<TweakSliderFill>>,
    mut val_text_q: Query<(&TweakSliderValueText, &mut Text), Without<TweakToggleText>>,
    mut toggle_q: Query<
        (&TweakToggle, &Interaction, &mut BackgroundColor),
        Without<TweakSliderFill>,
    >,
    mut toggle_text_q: Query<(&TweakToggleText, &mut Text), Without<TweakSliderValueText>>,
    mut readonly_q: Query<
        (&TweakReadOnlyText, &mut Text),
        (Without<TweakSliderValueText>, Without<TweakToggleText>),
    >,
    mut color_q: Query<(&ColorPreview, &mut BackgroundColor), Without<TweakToggle>>,
    mut cycle_text_q: Query<
        (&TweakCycleText, &mut Text),
        (
            Without<TweakSliderValueText>,
            Without<TweakToggleText>,
            Without<TweakReadOnlyText>,
        ),
    >,
) {
    if !state.tweaks_expanded {
        return;
    }

    for (ct, mut text) in &mut cycle_text_q {
        if let Some(entries) = tweaks.folders.get(&ct.folder) {
            if let Some(entry) = entries.iter().find(|e| e.label == ct.label) {
                if let TweakValue::CycleEnum { options, selected } = &entry.value {
                    let new_text = options.get(*selected).map(|s| s.as_str()).unwrap_or("--");
                    if **text != new_text {
                        **text = new_text.to_string();
                    }
                }
            }
        }
    }

    for (fill, mut node) in &mut fill_q {
        if let Some(entries) = tweaks.folders.get(&fill.folder) {
            if let Some(entry) = entries.iter().find(|e| e.label == fill.label) {
                if let TweakValue::Float {
                    value, min, max, ..
                } = &entry.value
                {
                    let pct = ((value - min) / (max - min)).clamp(0.0, 1.0) * 100.0;
                    node.width = Val::Percent(pct);
                }
            }
        }
    }

    for (knob, mut node) in &mut knob_q {
        if let Some(entries) = tweaks.folders.get(&knob.folder) {
            if let Some(entry) = entries.iter().find(|e| e.label == knob.label) {
                if let TweakValue::Float {
                    value, min, max, ..
                } = &entry.value
                {
                    let pct = ((value - min) / (max - min)).clamp(0.0, 1.0) * 100.0;
                    node.left = Val::Percent(pct);
                }
            }
        }
    }

    for (vt, mut text) in &mut val_text_q {
        if let Some(entries) = tweaks.folders.get(&vt.folder) {
            if let Some(entry) = entries.iter().find(|e| e.label == vt.label) {
                if let TweakValue::Float { value, .. } = &entry.value {
                    let new_text = format_tweak_float(*value);
                    if **text != new_text {
                        **text = new_text;
                    }
                }
            }
        }
    }

    for (tog, interaction, mut bg) in &mut toggle_q {
        if let Some(v) = tweaks.get_bool(&tog.folder, &tog.label) {
            let target = match (*interaction, v) {
                (Interaction::Pressed, true) => Color::srgba(1.0, 1.0, 1.0, 0.28),
                (Interaction::Hovered, true) => Color::srgba(1.0, 1.0, 1.0, 0.22),
                (_, true) => debug_active_surface(),
                (Interaction::Pressed, false) => debug_pressed_surface(),
                (Interaction::Hovered, false) => debug_hover_surface(),
                (_, false) => debug_control_surface(),
            };
            bg.0 = target;
        }
    }

    for (tog, mut text) in &mut toggle_text_q {
        if let Some(v) = tweaks.get_bool(&tog.folder, &tog.label) {
            let new_text = if v { "ON" } else { "OFF" };
            if **text != new_text {
                **text = new_text.to_string();
            }
        }
    }

    for (ro, mut text) in &mut readonly_q {
        if let Some(entries) = tweaks.folders.get(&ro.folder) {
            if let Some(entry) = entries.iter().find(|e| e.label == ro.label) {
                if let TweakValue::ReadOnly(ref new_text) = entry.value {
                    if **text != *new_text {
                        **text = new_text.clone();
                    }
                }
            }
        }
    }

    for (cp, mut bg) in &mut color_q {
        let r = tweaks
            .get_float(&cp.folder, &format!("{} R", cp.prefix))
            .unwrap_or(0.0);
        let g = tweaks
            .get_float(&cp.folder, &format!("{} G", cp.prefix))
            .unwrap_or(0.0);
        let b = tweaks
            .get_float(&cp.folder, &format!("{} B", cp.prefix))
            .unwrap_or(0.0);
        bg.0 = Color::srgb(r, g, b);
    }
}

pub fn update_save_button_feedback(
    time: Res<Time>,
    mut feedback: ResMut<SaveConfigFeedback>,
    mut text_q: Query<&mut Text, With<SaveConfigButtonText>>,
) {
    if feedback.0.tick(time.delta()).just_finished() {
        if let Ok(mut text) = text_q.single_mut() {
            **text = "Save".to_string();
        }
    }
}
