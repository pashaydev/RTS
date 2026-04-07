use bevy::prelude::*;

use crate::types::UiPressActive;
use crate::infrastructure::debug::config::save_debug_config;
use crate::infrastructure::debug::model::{DebugTweaks, TweakValue};
use crate::infrastructure::debug::state::{ActiveSlider, DebugButtonPressed, DebugPanelState, SaveConfigFeedback};
use crate::infrastructure::debug::ui::components::{
    DebugExpandButton, FolderHeader, SaveConfigButton, SaveConfigButtonText, TweakButton,
    TweakCycleEnum, TweakSlider, TweakToggle,
};

pub fn initialize_debug_folder_defaults(
    tweaks: Res<DebugTweaks>,
    mut state: ResMut<DebugPanelState>,
) {
    if tweaks.folders.is_empty() {
        return;
    }

    let mut changed = false;
    for folder in tweaks.folders.keys() {
        if !state.seen_folders.iter().any(|seen| seen == folder) {
            state.seen_folders.push(folder.clone());
            state.collapsed_folders.push(folder.clone());
            changed = true;
        }
    }

    if !changed {
        return;
    }
}

pub fn handle_folder_collapse(
    mut state: ResMut<DebugPanelState>,
    folder_q: Query<(&FolderHeader, &Interaction), Changed<Interaction>>,
) {
    for (header, interaction) in &folder_q {
        if *interaction == Interaction::Pressed {
            if let Some(pos) = state.collapsed_folders.iter().position(|f| *f == header.0) {
                state.collapsed_folders.remove(pos);
            } else {
                state.collapsed_folders.push(header.0.clone());
            }
        }
    }
}

pub fn handle_expand_button(
    mut state: ResMut<DebugPanelState>,
    btn_q: Query<&Interaction, (Changed<Interaction>, With<DebugExpandButton>)>,
) {
    for interaction in &btn_q {
        if *interaction == Interaction::Pressed {
            state.tweaks_expanded = !state.tweaks_expanded;
        }
    }
}

pub fn handle_toggle_click(
    mut tweaks: ResMut<DebugTweaks>,
    toggle_q: Query<(&TweakToggle, &Interaction), Changed<Interaction>>,
) {
    for (toggle, interaction) in &toggle_q {
        if *interaction == Interaction::Pressed {
            if let Some(entry) = tweaks.get_mut(&toggle.folder, &toggle.label) {
                if let TweakValue::Bool(ref mut v) = entry.value {
                    *v = !*v;
                }
            }
        }
    }
}

pub fn handle_cycle_click(
    mut tweaks: ResMut<DebugTweaks>,
    cycle_q: Query<(&TweakCycleEnum, &Interaction), Changed<Interaction>>,
) {
    for (cycle, interaction) in &cycle_q {
        if *interaction == Interaction::Pressed {
            if let Some(entry) = tweaks.get_mut(&cycle.folder, &cycle.label) {
                if let TweakValue::CycleEnum { options, selected } = &mut entry.value {
                    if !options.is_empty() {
                        *selected = (*selected + 1) % options.len();
                    }
                }
            }
        }
    }
}

pub fn handle_button_click(
    mut pressed: ResMut<DebugButtonPressed>,
    button_q: Query<(&TweakButton, &Interaction), Changed<Interaction>>,
) {
    pressed.pressed.clear();
    for (button, interaction) in &button_q {
        if *interaction == Interaction::Pressed {
            pressed
                .pressed
                .push((button.folder.clone(), button.label.clone()));
        }
    }
}

pub fn handle_slider_interaction(
    mut tweaks: ResMut<DebugTweaks>,
    mut active: ResMut<ActiveSlider>,
    mut ui_press: ResMut<UiPressActive>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    slider_q: Query<(&TweakSlider, &ComputedNode, &UiGlobalTransform)>,
) {
    if !mouse.pressed(MouseButton::Left) {
        if active.folder.is_some() {
            active.folder = None;
            active.label = None;
            ui_press.0 = false;
        }
        return;
    }

    let Some(cursor_phys) = windows
        .single()
        .ok()
        .and_then(|w| w.physical_cursor_position())
    else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left) {
        for (slider, computed, ui_tf) in &slider_q {
            if computed.contains_point(*ui_tf, cursor_phys) {
                active.folder = Some(slider.folder.clone());
                active.label = Some(slider.label.clone());
                ui_press.0 = true;
                break;
            }
        }
    }

    let (Some(ref folder), Some(ref label)) = (&active.folder, &active.label) else {
        return;
    };

    for (slider, computed, ui_tf) in &slider_q {
        if slider.folder != *folder || slider.label != *label {
            continue;
        }

        let Some(norm) = computed.normalize_point(*ui_tf, cursor_phys) else {
            if let Some(inv) = ui_tf.try_inverse() {
                let local = inv.transform_point2(cursor_phys);
                let size = computed.size();
                if size.x > 0.0 {
                    let t = ((local.x / size.x) + 0.5).clamp(0.0, 1.0);
                    if let Some(entry) = tweaks.get_mut(folder, label) {
                        if let TweakValue::Float {
                            value,
                            min,
                            max,
                            step,
                        } = &mut entry.value
                        {
                            let raw = *min + t * (*max - *min);
                            *value = if *step > 0.0 {
                                (*step * (raw / *step).round()).clamp(*min, *max)
                            } else {
                                raw.clamp(*min, *max)
                            };
                        }
                    }
                }
            }
            break;
        };
        let t = (norm.x + 0.5).clamp(0.0, 1.0);

        if let Some(entry) = tweaks.get_mut(folder, label) {
            if let TweakValue::Float {
                value,
                min,
                max,
                step,
            } = &mut entry.value
            {
                let raw = *min + t * (*max - *min);
                let snapped = if *step > 0.0 {
                    (*step * (raw / *step).round()).clamp(*min, *max)
                } else {
                    raw.clamp(*min, *max)
                };
                *value = snapped;
            }
        }
        break;
    }
}

pub fn handle_save_config_click(
    tweaks: Res<DebugTweaks>,
    db: Res<crate::infrastructure::database::GameDatabase>,
    mut feedback: ResMut<SaveConfigFeedback>,
    btn_q: Query<&Interaction, (Changed<Interaction>, With<SaveConfigButton>)>,
    mut text_q: Query<&mut Text, With<SaveConfigButtonText>>,
) {
    for interaction in &btn_q {
        if *interaction == Interaction::Pressed {
            save_debug_config(&tweaks, &db);
            feedback.0 = Timer::from_seconds(1.0, TimerMode::Once);
            if let Ok(mut text) = text_q.single_mut() {
                **text = "Saved!".to_string();
            }
        }
    }
}
