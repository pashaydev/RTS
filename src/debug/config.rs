use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::debug::model::{DebugTweaks, TweakValue};
use crate::debug::state::ConfigApplied;

pub const DEBUG_CONFIG_PATH: &str = "config/debug_tweaks.json";

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum ConfigValue {
    Float(f32),
    Bool(bool),
}

type ConfigMap = BTreeMap<String, BTreeMap<String, ConfigValue>>;

pub fn save_debug_config(tweaks: &DebugTweaks) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut map: ConfigMap = BTreeMap::new();
        for (folder, entries) in &tweaks.folders {
            let folder_map = map.entry(folder.clone()).or_default();
            for entry in entries {
                match &entry.value {
                    TweakValue::Float { value, .. } => {
                        folder_map.insert(entry.label.clone(), ConfigValue::Float(*value));
                    }
                    TweakValue::Bool(v) => {
                        folder_map.insert(entry.label.clone(), ConfigValue::Bool(*v));
                    }
                    TweakValue::ReadOnly(_) => {}
                    TweakValue::CycleEnum { .. } => {}
                    TweakValue::Button { .. } => {}
                }
            }
        }

        if let Some(parent) = std::path::Path::new(DEBUG_CONFIG_PATH).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(&map) {
            Ok(json) => {
                if let Err(e) = std::fs::write(DEBUG_CONFIG_PATH, json) {
                    warn!("Failed to save debug config: {}", e);
                }
            }
            Err(e) => warn!("Failed to serialize debug config: {}", e),
        }
    }
}

fn load_debug_config() -> Option<ConfigMap> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let data = std::fs::read_to_string(DEBUG_CONFIG_PATH).ok()?;
        serde_json::from_str(&data).ok()
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

fn apply_config_to_tweaks(tweaks: &mut DebugTweaks, config: &ConfigMap) {
    for (folder, entries) in config {
        if let Some(tweak_entries) = tweaks.folders.get_mut(folder) {
            for entry in tweak_entries.iter_mut() {
                if let Some(saved) = entries.get(&entry.label) {
                    match (&mut entry.value, saved) {
                        (
                            TweakValue::Float {
                                value, min, max, ..
                            },
                            ConfigValue::Float(v),
                        ) => {
                            *value = v.clamp(*min, *max);
                        }
                        (TweakValue::Bool(b), ConfigValue::Bool(v)) => {
                            *b = *v;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

pub fn apply_saved_config(
    mut commands: bevy::prelude::Commands,
    mut tweaks: bevy::prelude::ResMut<DebugTweaks>,
    applied: Option<bevy::prelude::Res<ConfigApplied>>,
) {
    if applied.is_some() {
        return;
    }
    if tweaks.folders.is_empty() {
        return;
    }
    commands.insert_resource(ConfigApplied);

    if let Some(config) = load_debug_config() {
        info!("Loaded debug config from {}", DEBUG_CONFIG_PATH);
        apply_config_to_tweaks(&mut tweaks, &config);
    } else {
        info!(
            "No debug config found, saving defaults to {}",
            DEBUG_CONFIG_PATH
        );
        save_debug_config(&tweaks);
    }
}
