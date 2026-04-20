//! `TweakValue` and `TweakEntry` — the data model for the debug parameter
//! hierarchy surfaced by the debug panel and persisted by config.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TweakValue {
    Float {
        value: f32,
        min: f32,
        max: f32,
        step: f32,
    },
    Bool(bool),
    ReadOnly(String),
    #[serde(skip)]
    CycleEnum {
        options: Vec<String>,
        selected: usize,
    },
    #[serde(skip)]
    Button {
        text: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TweakEntry {
    pub label: String,
    pub value: TweakValue,
}

#[derive(Resource, Default)]
pub struct DebugTweaks {
    pub folders: BTreeMap<String, Vec<TweakEntry>>,
}

impl DebugTweaks {
    pub fn add_float(
        &mut self,
        folder: &str,
        label: &str,
        value: f32,
        min: f32,
        max: f32,
        step: f32,
    ) {
        self.folders
            .entry(folder.to_string())
            .or_default()
            .push(TweakEntry {
                label: label.to_string(),
                value: TweakValue::Float {
                    value,
                    min,
                    max,
                    step,
                },
            });
    }

    pub fn add_bool(&mut self, folder: &str, label: &str, value: bool) {
        self.folders
            .entry(folder.to_string())
            .or_default()
            .push(TweakEntry {
                label: label.to_string(),
                value: TweakValue::Bool(value),
            });
    }

    pub fn add_readonly(&mut self, folder: &str, label: &str, text: &str) {
        self.folders
            .entry(folder.to_string())
            .or_default()
            .push(TweakEntry {
                label: label.to_string(),
                value: TweakValue::ReadOnly(text.to_string()),
            });
    }

    pub fn add_cycle_enum(
        &mut self,
        folder: &str,
        label: &str,
        options: Vec<String>,
        selected: usize,
    ) {
        self.folders
            .entry(folder.to_string())
            .or_default()
            .push(TweakEntry {
                label: label.to_string(),
                value: TweakValue::CycleEnum { options, selected },
            });
    }

    pub fn add_button(&mut self, folder: &str, label: &str) {
        self.folders
            .entry(folder.to_string())
            .or_default()
            .push(TweakEntry {
                label: label.to_string(),
                value: TweakValue::Button {
                    text: label.to_string(),
                },
            });
    }

    pub fn get_cycle_selected(&self, folder: &str, label: &str) -> Option<usize> {
        self.folders.get(folder)?.iter().find_map(|e| {
            if e.label == label {
                if let TweakValue::CycleEnum { selected, .. } = &e.value {
                    return Some(*selected);
                }
            }
            None
        })
    }

    pub fn get_float(&self, folder: &str, label: &str) -> Option<f32> {
        self.folders.get(folder)?.iter().find_map(|e| {
            if e.label == label {
                if let TweakValue::Float { value, .. } = &e.value {
                    return Some(*value);
                }
            }
            None
        })
    }

    pub fn get_bool(&self, folder: &str, label: &str) -> Option<bool> {
        self.folders.get(folder)?.iter().find_map(|e| {
            if e.label == label {
                if let TweakValue::Bool(v) = &e.value {
                    return Some(*v);
                }
            }
            None
        })
    }

    pub fn get_mut(&mut self, folder: &str, label: &str) -> Option<&mut TweakEntry> {
        self.folders
            .get_mut(folder)?
            .iter_mut()
            .find(|e| e.label == label)
    }

    pub fn set_float_if_changed(&mut self, folder: &str, label: &str, new_val: f32) {
        if let Some(entry) = self.get_mut(folder, label) {
            if let TweakValue::Float { value, .. } = &mut entry.value {
                if (*value - new_val).abs() > f32::EPSILON {
                    *value = new_val;
                }
            }
        }
    }

    pub fn set_readonly_if_changed(&mut self, folder: &str, label: &str, new_text: &str) {
        if let Some(entry) = self.get_mut(folder, label) {
            if let TweakValue::ReadOnly(ref old) = entry.value {
                if old != new_text {
                    entry.value = TweakValue::ReadOnly(new_text.to_string());
                }
            }
        }
    }

    pub fn set_bool_if_changed(&mut self, folder: &str, label: &str, new_value: bool) {
        if let Some(entry) = self.get_mut(folder, label) {
            if let TweakValue::Bool(value) = &mut entry.value {
                if *value != new_value {
                    *value = new_value;
                }
            }
        }
    }

    pub fn set_cycle_selected_if_changed(
        &mut self,
        folder: &str,
        label: &str,
        new_selected: usize,
    ) {
        if let Some(entry) = self.get_mut(folder, label) {
            if let TweakValue::CycleEnum { selected, options } = &mut entry.value {
                if *selected != new_selected && new_selected < options.len() {
                    *selected = new_selected;
                }
            }
        }
    }

    pub fn get_color_rgb(&self, folder: &str) -> Option<[f32; 3]> {
        match (
            self.get_float(folder, "Color R"),
            self.get_float(folder, "Color G"),
            self.get_float(folder, "Color B"),
        ) {
            (Some(r), Some(g), Some(b)) => Some([r, g, b]),
            _ => None,
        }
    }

    pub fn sync_color_rgb_back(
        &mut self,
        folder: &str,
        color: &Srgba,
        active: &crate::infrastructure::debug::state::ActiveSlider,
    ) {
        if !active.is_dragging(folder, "Color R") {
            self.set_float_if_changed(folder, "Color R", color.red);
        }
        if !active.is_dragging(folder, "Color G") {
            self.set_float_if_changed(folder, "Color G", color.green);
        }
        if !active.is_dragging(folder, "Color B") {
            self.set_float_if_changed(folder, "Color B", color.blue);
        }
    }
}
