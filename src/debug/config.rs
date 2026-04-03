use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::database::GameDatabase;
use crate::debug::model::{DebugTweaks, TweakValue};
use crate::debug::state::ConfigApplied;

const DEBUG_SETTINGS_CATEGORY: &str = "debug";

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum ConfigValue {
    Float(f32),
    Bool(bool),
}

type ConfigMap = BTreeMap<String, BTreeMap<String, ConfigValue>>;

pub fn save_debug_config(tweaks: &DebugTweaks, db: &GameDatabase) {
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

    if let Ok(json) = serde_json::to_string_pretty(&map) {
        db.save_setting(DEBUG_SETTINGS_CATEGORY, "_blob", &json);
    }
}

fn load_debug_config(db: &GameDatabase) -> Option<ConfigMap> {
    let json = db.load_setting(DEBUG_SETTINGS_CATEGORY, "_blob")?;
    serde_json::from_str(&json).ok()
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
    mut commands: Commands,
    mut tweaks: ResMut<DebugTweaks>,
    applied: Option<Res<ConfigApplied>>,
    db: Res<GameDatabase>,
) {
    if applied.is_some() {
        return;
    }
    if tweaks.folders.is_empty() {
        return;
    }
    commands.insert_resource(ConfigApplied);

    if let Some(config) = load_debug_config(&db) {
        info!("Loaded debug config from database");
        apply_config_to_tweaks(&mut tweaks, &config);
    } else {
        info!("No debug config found, saving defaults to database");
        save_debug_config(&tweaks, &db);
    }
}
