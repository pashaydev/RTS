use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::ecs::message::MessageReader;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::fog::FogTweakSettings;
use crate::fog_material::FogOfWarMaterial;
use bevy::light::{FogVolume, VolumetricFog};
use crate::lighting::{AtmosphericFogVolume, DayCycle, LightingOverrides, SunLight};
use crate::components::FogOverlay;

const DEBUG_CONFIG_PATH: &str = "config/debug_tweaks.json";

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugTweaks>()
            .init_resource::<DebugPanelState>()
            .init_resource::<FpsTracker>()
            .init_resource::<ActiveSlider>()
            .init_resource::<TweakStructureVersion>()
            .insert_resource(SaveConfigFeedback(Timer::from_seconds(0.0, TimerMode::Once)))
            .add_systems(Startup, spawn_debug_overlay)
            .add_systems(
                Update,
                (
                    toggle_debug_panel,
                    update_fps_tracker,
                    update_debug_texts,
                    handle_folder_collapse,
                    handle_toggle_click,
                    handle_slider_interaction,
                    handle_debug_scroll,
                    handle_save_config_click,
                    update_save_button_feedback,
                    apply_saved_config,
                    sync_lighting_tweaks,
                    sync_fog_tweaks,
                    rebuild_tweak_panel,
                    update_tweak_visuals,
                ),
            );
    }
}

// ── Tweak value types ──

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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TweakEntry {
    pub label: String,
    pub value: TweakValue,
}

// ── Central tweak registry ──

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

    fn set_float_if_changed(&mut self, folder: &str, label: &str, new_val: f32) {
        if let Some(entry) = self.get_mut(folder, label) {
            if let TweakValue::Float { value, .. } = &mut entry.value {
                if (*value - new_val).abs() > f32::EPSILON {
                    *value = new_val;
                }
            }
        }
    }

    fn set_readonly_if_changed(&mut self, folder: &str, label: &str, new_text: &str) {
        if let Some(entry) = self.get_mut(folder, label) {
            if let TweakValue::ReadOnly(ref old) = entry.value {
                if old != new_text {
                    entry.value = TweakValue::ReadOnly(new_text.to_string());
                }
            }
        }
    }
}

// ── Config serialization format: folder → { label → value } ──
// Only saves Float values and Bool values. ReadOnly entries are skipped.
// Min/max/step metadata stays in code only.

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum ConfigValue {
    Float(f32),
    Bool(bool),
}

type ConfigMap = BTreeMap<String, BTreeMap<String, ConfigValue>>;

fn save_debug_config(tweaks: &DebugTweaks) {
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

fn load_debug_config() -> Option<ConfigMap> {
    let data = std::fs::read_to_string(DEBUG_CONFIG_PATH).ok()?;
    serde_json::from_str(&data).ok()
}

fn apply_config_to_tweaks(tweaks: &mut DebugTweaks, config: &ConfigMap) {
    for (folder, entries) in config {
        if let Some(tweak_entries) = tweaks.folders.get_mut(folder) {
            for entry in tweak_entries.iter_mut() {
                if let Some(saved) = entries.get(&entry.label) {
                    match (&mut entry.value, saved) {
                        (TweakValue::Float { value, min, max, .. }, ConfigValue::Float(v)) => {
                            *value = v.clamp(*min, *max);
                        }
                        (TweakValue::Bool(ref mut b), ConfigValue::Bool(v)) => {
                            *b = *v;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

// ── Panel state ──

#[derive(Resource, Default)]
pub struct DebugPanelState {
    pub visible: bool,
    pub collapsed_folders: Vec<String>,
}

// ── FPS tracker ──

#[derive(Resource)]
pub struct FpsTracker {
    pub frame_count: u32,
    pub elapsed: f32,
    pub fps: f32,
    pub frame_time_ms: f32,
}

impl Default for FpsTracker {
    fn default() -> Self {
        Self {
            frame_count: 0,
            elapsed: 0.0,
            fps: 0.0,
            frame_time_ms: 0.0,
        }
    }
}

// ── Structural version: only incremented when folders/entries are added/removed ──

#[derive(Resource, Default)]
struct TweakStructureVersion {
    version: u64,
    last_folder_count: usize,
    last_entry_counts: Vec<usize>,
}

// ── Active slider drag state ──

#[derive(Resource, Default)]
struct ActiveSlider {
    folder: Option<String>,
    label: Option<String>,
}

// ── UI marker components ──

#[derive(Component)]
struct DebugOverlayRoot;

#[derive(Component)]
struct DebugFpsText;

#[derive(Component)]
struct DebugEntityCountText;

#[derive(Component)]
struct DebugDayCycleText;

#[derive(Component)]
struct DebugTweakPanel;

#[derive(Component)]
struct TweakPanelBuiltVersion(u64);

#[derive(Component)]
struct FolderHeader(String);

#[derive(Component)]
struct TweakSlider {
    folder: String,
    label: String,
}

#[derive(Component)]
struct TweakSliderFill {
    folder: String,
    label: String,
}

#[derive(Component)]
struct TweakSliderValueText {
    folder: String,
    label: String,
}

#[derive(Component)]
struct TweakToggle {
    folder: String,
    label: String,
}

#[derive(Component)]
struct TweakToggleText {
    folder: String,
    label: String,
}

#[derive(Component)]
struct TweakReadOnlyText {
    folder: String,
    label: String,
}

#[derive(Component)]
struct SaveConfigButton;

#[derive(Component)]
struct SaveConfigButtonText;

#[derive(Resource)]
struct SaveConfigFeedback(Timer);

/// Color preview swatch — shows combined RGB from 3 sibling sliders.
#[derive(Component)]
struct ColorPreview {
    folder: String,
    prefix: String, // e.g. "color" matches "color R", "color G", "color B"
}

// ── Spawn the debug overlay (hidden by default) ──

fn spawn_debug_overlay(mut commands: Commands) {
    commands
        .spawn((
            DebugOverlayRoot,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(8.0),
                top: Val::Px(8.0),
                width: Val::Px(320.0),
                max_height: Val::Percent(85.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(10.0)),
                row_gap: Val::Px(4.0),
                overflow: Overflow::scroll_y(),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.05, 0.88)),
            Visibility::Hidden,
            GlobalZIndex(100),
        ))
        .with_children(|panel| {
            // Title row with Save button
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    width: Val::Percent(100.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new("Debug Panel (F3)"),
                        TextFont {
                            font_size: 15.0,
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.85, 0.2)),
                    ));

                    row.spawn((
                        SaveConfigButton,
                        Interaction::default(),
                        Button,
                        Node {
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                            border_radius: BorderRadius::all(Val::Px(3.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.3, 0.6, 1.0, 0.3)),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            SaveConfigButtonText,
                            Pickable::IGNORE,
                            Text::new("Save"),
                            TextFont {
                                font_size: 11.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });
                });

            spawn_separator(panel);

            // FPS
            panel.spawn((
                DebugFpsText,
                Text::new("FPS: --"),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 1.0, 0.6)),
            ));

            // Entity count
            panel.spawn((
                DebugEntityCountText,
                Text::new("Entities: --"),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.8, 1.0)),
            ));

            // Day cycle
            panel.spawn((
                DebugDayCycleText,
                Text::new("Day: --"),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.9, 0.7)),
            ));

            spawn_separator(panel);

            // Tweak panel container
            panel.spawn((
                DebugTweakPanel,
                TweakPanelBuiltVersion(0),
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    width: Val::Percent(100.0),
                    ..default()
                },
            ));
        });
}

fn spawn_separator(parent: &mut ChildSpawnerCommands) {
    parent.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Px(1.0),
            margin: UiRect::axes(Val::ZERO, Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.15)),
    ));
}

// ── Toggle panel with F3 ──

fn toggle_debug_panel(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<DebugPanelState>,
    mut root_q: Query<&mut Visibility, With<DebugOverlayRoot>>,
) {
    if keys.just_pressed(KeyCode::F3) {
        state.visible = !state.visible;
        if let Ok(mut vis) = root_q.single_mut() {
            *vis = if state.visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

// ── FPS tracking ──

fn update_fps_tracker(mut tracker: ResMut<FpsTracker>, time: Res<Time>) {
    tracker.frame_count += 1;
    tracker.elapsed += time.delta_secs();
    if tracker.elapsed >= 0.5 {
        tracker.fps = tracker.frame_count as f32 / tracker.elapsed;
        tracker.frame_time_ms = tracker.elapsed * 1000.0 / tracker.frame_count as f32;
        tracker.frame_count = 0;
        tracker.elapsed = 0.0;
    }
}

// ── Update metric text nodes ──

fn update_debug_texts(
    state: Res<DebugPanelState>,
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
    if !state.visible {
        return;
    }

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

// ── Rebuild tweak panel ONLY on structural changes ──

fn rebuild_tweak_panel(
    tweaks: Res<DebugTweaks>,
    panel_state: Res<DebugPanelState>,
    mut structure: ResMut<TweakStructureVersion>,
    mut commands: Commands,
    mut panel_q: Query<(Entity, &mut TweakPanelBuiltVersion), With<DebugTweakPanel>>,
    children_q: Query<&Children>,
) {
    // Detect structural changes: folder count or entry counts changed
    let folder_count = tweaks.folders.len();
    let entry_counts: Vec<usize> = tweaks.folders.values().map(|v| v.len()).collect();
    let structure_changed = folder_count != structure.last_folder_count
        || entry_counts != structure.last_entry_counts
        || panel_state.is_changed();

    if !structure_changed {
        return;
    }

    structure.last_folder_count = folder_count;
    structure.last_entry_counts = entry_counts;
    structure.version += 1;

    let Ok((panel_entity, mut built_ver)) = panel_q.single_mut() else {
        return;
    };
    built_ver.0 = structure.version;

    // Despawn old children
    if let Ok(children) = children_q.get(panel_entity) {
        for child in children {
            commands.entity(*child).despawn();
        }
    }

    // Rebuild
    commands.entity(panel_entity).with_children(|panel| {
        for (folder_name, entries) in &tweaks.folders {
            let collapsed = panel_state.collapsed_folders.contains(folder_name);
            spawn_folder_header(panel, folder_name, collapsed);

            if collapsed {
                continue;
            }

            // Track if we have color R/G/B groups to add a preview after B
            let mut color_prefix: Option<String> = None;

            for entry in entries {
                match &entry.value {
                    TweakValue::Float {
                        value, min, max, ..
                    } => {
                        spawn_slider_row(panel, folder_name, &entry.label, *value, *min, *max);

                        // Detect color groups: "X R", "X G", "X B"
                        if entry.label.ends_with(" R") {
                            color_prefix =
                                Some(entry.label.trim_end_matches(" R").to_string());
                        } else if entry.label.ends_with(" B") {
                            if let Some(ref prefix) = color_prefix {
                                let expected_b =
                                    format!("{} B", prefix);
                                if entry.label == expected_b {
                                    spawn_color_preview(panel, folder_name, prefix);
                                }
                            }
                            color_prefix = None;
                        }
                    }
                    TweakValue::Bool(v) => {
                        spawn_toggle_row(panel, folder_name, &entry.label, *v);
                        color_prefix = None;
                    }
                    TweakValue::ReadOnly(text) => {
                        spawn_readonly_row(panel, folder_name, &entry.label, text);
                        color_prefix = None;
                    }
                }
            }
        }
    });
}

// ── Update visuals in-place (no rebuild needed) ──

fn update_tweak_visuals(
    state: Res<DebugPanelState>,
    tweaks: Res<DebugTweaks>,
    mut fill_q: Query<(&TweakSliderFill, &mut Node)>,
    mut val_text_q: Query<(&TweakSliderValueText, &mut Text), Without<TweakToggleText>>,
    mut toggle_q: Query<(&TweakToggle, &mut BackgroundColor), Without<TweakSliderFill>>,
    mut toggle_text_q: Query<(&TweakToggleText, &mut Text), Without<TweakSliderValueText>>,
    mut readonly_q: Query<(&TweakReadOnlyText, &mut Text), (Without<TweakSliderValueText>, Without<TweakToggleText>)>,
    mut color_q: Query<(&ColorPreview, &mut BackgroundColor), Without<TweakToggle>>,
) {
    if !state.visible {
        return;
    }

    // Update slider fills
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

    // Update slider value texts
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

    // Update toggle button colors
    for (tog, mut bg) in &mut toggle_q {
        if let Some(v) = tweaks.get_bool(&tog.folder, &tog.label) {
            let target = if v {
                Color::srgb(0.15, 0.6, 0.25)
            } else {
                Color::srgba(1.0, 1.0, 1.0, 0.12)
            };
            bg.0 = target;
        }
    }

    // Update toggle text
    for (tog, mut text) in &mut toggle_text_q {
        if let Some(v) = tweaks.get_bool(&tog.folder, &tog.label) {
            let new_text = if v { "ON" } else { "OFF" };
            if **text != new_text {
                **text = new_text.to_string();
            }
        }
    }

    // Update readonly texts
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

    // Update color preview swatches
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

// ── UI spawn helpers ──

fn spawn_folder_header(parent: &mut ChildSpawnerCommands, name: &str, collapsed: bool) {
    let arrow = if collapsed { ">" } else { "v" };
    parent
        .spawn((
            FolderHeader(name.to_string()),
            Interaction::default(),
            Button,
            Node {
                padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                margin: UiRect::top(Val::Px(6.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                width: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.08)),
        ))
        .with_children(|header| {
            header.spawn((
                Text::new(format!("{} {}", arrow, name)),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.85, 0.3)),
            ));
        });
}

fn spawn_slider_row(
    parent: &mut ChildSpawnerCommands,
    folder: &str,
    label: &str,
    value: f32,
    min: f32,
    max: f32,
) {
    let pct = ((value - min) / (max - min)).clamp(0.0, 1.0) * 100.0;

    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            width: Val::Percent(100.0),
            padding: UiRect::left(Val::Px(10.0)),
            ..default()
        })
        .with_children(|row| {
            // Label
            row.spawn((
                Text::new(label),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.65, 0.65, 0.65)),
                Node {
                    width: Val::Px(95.0),
                    ..default()
                },
            ));

            // Slider track
            row.spawn((
                TweakSlider {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Node {
                    width: Val::Px(120.0),
                    height: Val::Px(14.0),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.15)),
            ))
            .with_children(|track| {
                track.spawn((
                    TweakSliderFill {
                        folder: folder.to_string(),
                        label: label.to_string(),
                    },
                    Node {
                        width: Val::Percent(pct),
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(3.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.25, 0.6, 1.0)),
                ));
            });

            // Value text
            row.spawn((
                TweakSliderValueText {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Text::new(format_tweak_float(value)),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                Node {
                    width: Val::Px(55.0),
                    ..default()
                },
            ));
        });
}

fn spawn_toggle_row(parent: &mut ChildSpawnerCommands, folder: &str, label: &str, value: bool) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            width: Val::Percent(100.0),
            padding: UiRect::left(Val::Px(10.0)),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.65, 0.65, 0.65)),
                Node {
                    width: Val::Px(95.0),
                    ..default()
                },
            ));

            let (bg, text) = if value {
                (Color::srgb(0.15, 0.6, 0.25), "ON")
            } else {
                (Color::srgba(1.0, 1.0, 1.0, 0.12), "OFF")
            };

            row.spawn((
                TweakToggle {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Interaction::default(),
                Button,
                Node {
                    width: Val::Px(42.0),
                    height: Val::Px(18.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(bg),
            ))
            .with_children(|btn| {
                btn.spawn((
                    TweakToggleText {
                        folder: folder.to_string(),
                        label: label.to_string(),
                    },
                    Pickable::IGNORE,
                    Text::new(text),
                    TextFont {
                        font_size: 11.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
        });
}

fn spawn_readonly_row(
    parent: &mut ChildSpawnerCommands,
    folder: &str,
    label: &str,
    text: &str,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            width: Val::Percent(100.0),
            padding: UiRect::left(Val::Px(10.0)),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.65, 0.65, 0.65)),
                Node {
                    width: Val::Px(95.0),
                    ..default()
                },
            ));

            row.spawn((
                TweakReadOnlyText {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Text::new(text),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.5, 0.5, 0.5)),
            ));
        });
}

fn spawn_color_preview(parent: &mut ChildSpawnerCommands, folder: &str, prefix: &str) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            width: Val::Percent(100.0),
            padding: UiRect::left(Val::Px(10.0)),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new("preview"),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.45, 0.45, 0.45)),
                Node {
                    width: Val::Px(95.0),
                    ..default()
                },
            ));

            row.spawn((
                ColorPreview {
                    folder: folder.to_string(),
                    prefix: prefix.to_string(),
                },
                Node {
                    width: Val::Px(120.0),
                    height: Val::Px(14.0),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.5, 0.5, 0.5)),
            ));
        });
}

fn format_tweak_float(v: f32) -> String {
    if v.abs() >= 100.0 {
        format!("{:.0}", v)
    } else if v.abs() >= 10.0 {
        format!("{:.1}", v)
    } else {
        format!("{:.3}", v)
    }
}

// ── Interaction handlers ──

fn handle_folder_collapse(
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

const SCROLL_LINE_HEIGHT: f32 = 24.0;

fn handle_debug_scroll(
    state: Res<DebugPanelState>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    windows: Query<&Window>,
    mut panel_q: Query<
        (
            &mut ScrollPosition,
            &ComputedNode,
            &UiGlobalTransform,
        ),
        With<DebugOverlayRoot>,
    >,
) {
    if !state.visible {
        return;
    }

    let mut dy = 0.0;
    for ev in mouse_wheel.read() {
        dy += match ev.unit {
            MouseScrollUnit::Line => -ev.y * SCROLL_LINE_HEIGHT,
            MouseScrollUnit::Pixel => -ev.y,
        };
    }

    if dy.abs() < 0.001 {
        return;
    }

    let Some(cursor_phys) = windows
        .single()
        .ok()
        .and_then(|w| w.physical_cursor_position())
    else {
        return;
    };

    for (mut scroll_pos, computed, ui_tf) in &mut panel_q {
        if !computed.contains_point(*ui_tf, cursor_phys) {
            continue;
        }
        let max_scroll = (computed.content_size().y - computed.size().y)
            .max(0.0)
            * computed.inverse_scale_factor();
        scroll_pos.y = (scroll_pos.y + dy).clamp(0.0, max_scroll);
    }
}

fn handle_toggle_click(
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

fn handle_slider_interaction(
    mut tweaks: ResMut<DebugTweaks>,
    mut active: ResMut<ActiveSlider>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    slider_q: Query<(&TweakSlider, &ComputedNode, &UiGlobalTransform)>,
) {
    // Release when mouse not pressed
    if !mouse.pressed(MouseButton::Left) {
        if active.folder.is_some() {
            active.folder = None;
            active.label = None;
        }
        return;
    }

    // Bevy's UI uses physical_cursor_position + UiGlobalTransform for hit-testing
    let Some(cursor_phys) = windows
        .single()
        .ok()
        .and_then(|w| w.physical_cursor_position())
    else {
        return;
    };

    // On fresh click, find which slider the cursor is over
    if mouse.just_pressed(MouseButton::Left) {
        for (slider, computed, ui_tf) in &slider_q {
            if computed.contains_point(*ui_tf, cursor_phys) {
                active.folder = Some(slider.folder.clone());
                active.label = Some(slider.label.clone());
                break;
            }
        }
    }

    // Update the active slider's value
    let (Some(ref folder), Some(ref label)) = (&active.folder, &active.label) else {
        return;
    };

    for (slider, computed, ui_tf) in &slider_q {
        if slider.folder != *folder || slider.label != *label {
            continue;
        }
        // normalize_point returns (-0.5..0.5) centered, so shift to 0..1
        let Some(norm) = computed.normalize_point(*ui_tf, cursor_phys) else {
            // Cursor outside node during drag — clamp to edges
            if let Some(inv) = ui_tf.try_inverse() {
                let local = inv.transform_point2(cursor_phys);
                let size = computed.size();
                if size.x > 0.0 {
                    let t = ((local.x / size.x) + 0.5).clamp(0.0, 1.0);
                    if let Some(entry) = tweaks.get_mut(folder, label) {
                        if let TweakValue::Float {
                            value, min, max, step,
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

// ── Sync: Lighting ↔ DebugTweaks ──

fn sync_lighting_tweaks(
    mut tweaks: ResMut<DebugTweaks>,
    active: Res<ActiveSlider>,
    mut cycle: ResMut<DayCycle>,
    mut overrides: ResMut<LightingOverrides>,
    sun_q: Query<(&DirectionalLight, &Transform), With<SunLight>>,
    ambient: Res<GlobalAmbientLight>,
    clear: Res<ClearColor>,
    mut fog_vol_q: Query<&mut FogVolume, With<AtmosphericFogVolume>>,
    mut cam_fog_q: Query<&mut VolumetricFog>,
) {
    // Helper: check if a specific entry is actively being dragged
    let is_dragging = |folder: &str, label: &str| -> bool {
        active.folder.as_deref() == Some(folder) && active.label.as_deref() == Some(label)
    };

    // ── Time of Day folder ──
    if let Some(v) = tweaks.get_float("Time of Day", "Cycle Duration") {
        if (cycle.cycle_duration - v).abs() > f32::EPSILON {
            cycle.cycle_duration = v;
        }
    }
    if let Some(v) = tweaks.get_bool("Time of Day", "Paused") {
        if cycle.paused != v {
            cycle.paused = v;
        }
    }
    if let Some(v) = tweaks.get_float("Time of Day", "Time") {
        if cycle.paused && (cycle.time - v).abs() > 0.001 {
            cycle.time = v;
        }
    }
    tweaks.set_readonly_if_changed("Time of Day", "Phase", &format!("{:?}", cycle.phase));
    if !cycle.paused && !is_dragging("Time of Day", "Time") {
        tweaks.set_float_if_changed("Time of Day", "Time", cycle.time);
    }

    // ── Sunlight folder ──
    let sun_override = tweaks.get_bool("Sunlight", "Override").unwrap_or(false);
    if sun_override {
        overrides.sun_illuminance = tweaks.get_float("Sunlight", "Illuminance");
        overrides.sun_color = match (
            tweaks.get_float("Sunlight", "Color R"),
            tweaks.get_float("Sunlight", "Color G"),
            tweaks.get_float("Sunlight", "Color B"),
        ) {
            (Some(r), Some(g), Some(b)) => Some([r, g, b]),
            _ => None,
        };
        overrides.sun_pitch = tweaks.get_float("Sunlight", "Pitch");
        overrides.sun_yaw = tweaks.get_float("Sunlight", "Yaw");
        overrides.shadows_enabled = tweaks.get_bool("Sunlight", "Shadows");
    } else {
        overrides.sun_illuminance = None;
        overrides.sun_color = None;
        overrides.sun_pitch = None;
        overrides.sun_yaw = None;

        if let Ok((sun, sun_tf)) = sun_q.single() {
            if !is_dragging("Sunlight", "Illuminance") {
                tweaks.set_float_if_changed("Sunlight", "Illuminance", sun.illuminance);
            }
            let c = sun.color.to_srgba();
            if !is_dragging("Sunlight", "Color R") {
                tweaks.set_float_if_changed("Sunlight", "Color R", c.red);
            }
            if !is_dragging("Sunlight", "Color G") {
                tweaks.set_float_if_changed("Sunlight", "Color G", c.green);
            }
            if !is_dragging("Sunlight", "Color B") {
                tweaks.set_float_if_changed("Sunlight", "Color B", c.blue);
            }

            let (pitch, yaw, _) = sun_tf.rotation.to_euler(EulerRot::XYZ);
            if !is_dragging("Sunlight", "Pitch") {
                tweaks.set_float_if_changed("Sunlight", "Pitch", pitch);
            }
            if !is_dragging("Sunlight", "Yaw") {
                tweaks.set_float_if_changed("Sunlight", "Yaw", yaw);
            }
        }

        overrides.shadows_enabled = tweaks.get_bool("Sunlight", "Shadows");
    }

    // ── Ambient Light folder ──
    let amb_override = tweaks.get_bool("Ambient Light", "Override").unwrap_or(false);
    if amb_override {
        overrides.ambient_brightness = tweaks.get_float("Ambient Light", "Brightness");
        overrides.ambient_color = match (
            tweaks.get_float("Ambient Light", "Color R"),
            tweaks.get_float("Ambient Light", "Color G"),
            tweaks.get_float("Ambient Light", "Color B"),
        ) {
            (Some(r), Some(g), Some(b)) => Some([r, g, b]),
            _ => None,
        };
    } else {
        overrides.ambient_brightness = None;
        overrides.ambient_color = None;

        if !is_dragging("Ambient Light", "Brightness") {
            tweaks.set_float_if_changed("Ambient Light", "Brightness", ambient.brightness);
        }
        let c = ambient.color.to_srgba();
        if !is_dragging("Ambient Light", "Color R") {
            tweaks.set_float_if_changed("Ambient Light", "Color R", c.red);
        }
        if !is_dragging("Ambient Light", "Color G") {
            tweaks.set_float_if_changed("Ambient Light", "Color G", c.green);
        }
        if !is_dragging("Ambient Light", "Color B") {
            tweaks.set_float_if_changed("Ambient Light", "Color B", c.blue);
        }
    }

    // ── Sky Color folder ──
    let fog_override = tweaks.get_bool("Sky Color", "Override").unwrap_or(false);
    if fog_override {
        overrides.fog_color = match (
            tweaks.get_float("Sky Color", "Color R"),
            tweaks.get_float("Sky Color", "Color G"),
            tweaks.get_float("Sky Color", "Color B"),
        ) {
            (Some(r), Some(g), Some(b)) => Some([r, g, b]),
            _ => None,
        };
    } else {
        overrides.fog_color = None;

        let c = clear.0.to_srgba();
        if !is_dragging("Sky Color", "Color R") {
            tweaks.set_float_if_changed("Sky Color", "Color R", c.red);
        }
        if !is_dragging("Sky Color", "Color G") {
            tweaks.set_float_if_changed("Sky Color", "Color G", c.green);
        }
        if !is_dragging("Sky Color", "Color B") {
            tweaks.set_float_if_changed("Sky Color", "Color B", c.blue);
        }
    }

    // ── Volumetric Fog folder ──
    let vol_enabled = tweaks.get_bool("Volumetric Fog", "Enabled").unwrap_or(true);
    let vol_override = tweaks.get_bool("Volumetric Fog", "Override").unwrap_or(false);

    // Toggle visibility of the fog volume
    if let Ok(mut fog_vol) = fog_vol_q.single_mut() {
        if !vol_enabled {
            fog_vol.density_factor = 0.0;
        }
    }

    // Toggle camera volumetric fog step_count to 0 to disable
    if let Ok(mut vol_fog) = cam_fog_q.single_mut() {
        if !vol_enabled {
            vol_fog.step_count = 0;
        } else if let Some(sc) = tweaks.get_float("Volumetric Fog", "Step Count") {
            vol_fog.step_count = sc as u32;
        }
    }

    if vol_override {
        overrides.vol_density = tweaks.get_float("Volumetric Fog", "Density");
        overrides.vol_color = match (
            tweaks.get_float("Volumetric Fog", "Color R"),
            tweaks.get_float("Volumetric Fog", "Color G"),
            tweaks.get_float("Volumetric Fog", "Color B"),
        ) {
            (Some(r), Some(g), Some(b)) => Some([r, g, b]),
            _ => None,
        };
        overrides.vol_ambient_intensity = tweaks.get_float("Volumetric Fog", "Ambient Intensity");
        overrides.vol_light_intensity = tweaks.get_float("Volumetric Fog", "Light Intensity");

        // Apply scattering/absorption directly
        if let Ok(mut fog_vol) = fog_vol_q.single_mut() {
            if let Some(s) = tweaks.get_float("Volumetric Fog", "Scattering") {
                fog_vol.scattering = s;
            }
            if let Some(a) = tweaks.get_float("Volumetric Fog", "Absorption") {
                fog_vol.absorption = a;
            }
        }
    } else {
        overrides.vol_density = None;
        overrides.vol_color = None;
        overrides.vol_ambient_intensity = None;
        overrides.vol_light_intensity = None;

        if let Ok(fog_vol) = fog_vol_q.single() {
            if !is_dragging("Volumetric Fog", "Density") {
                tweaks.set_float_if_changed("Volumetric Fog", "Density", fog_vol.density_factor);
            }
            let c = fog_vol.fog_color.to_srgba();
            if !is_dragging("Volumetric Fog", "Color R") {
                tweaks.set_float_if_changed("Volumetric Fog", "Color R", c.red);
            }
            if !is_dragging("Volumetric Fog", "Color G") {
                tweaks.set_float_if_changed("Volumetric Fog", "Color G", c.green);
            }
            if !is_dragging("Volumetric Fog", "Color B") {
                tweaks.set_float_if_changed("Volumetric Fog", "Color B", c.blue);
            }
            if !is_dragging("Volumetric Fog", "Light Intensity") {
                tweaks.set_float_if_changed("Volumetric Fog", "Light Intensity", fog_vol.light_intensity);
            }
            if !is_dragging("Volumetric Fog", "Scattering") {
                tweaks.set_float_if_changed("Volumetric Fog", "Scattering", fog_vol.scattering);
            }
            if !is_dragging("Volumetric Fog", "Absorption") {
                tweaks.set_float_if_changed("Volumetric Fog", "Absorption", fog_vol.absorption);
            }
        }

        if let Ok(vol_fog) = cam_fog_q.single() {
            if !is_dragging("Volumetric Fog", "Ambient Intensity") {
                tweaks.set_float_if_changed(
                    "Volumetric Fog",
                    "Ambient Intensity",
                    vol_fog.ambient_intensity,
                );
            }
            if !is_dragging("Volumetric Fog", "Step Count") {
                tweaks.set_float_if_changed("Volumetric Fog", "Step Count", vol_fog.step_count as f32);
            }
        }
    }
}

// ── Sync: Fog ↔ DebugTweaks ──

fn sync_fog_tweaks(
    tweaks: Res<DebugTweaks>,
    mut fog_settings: ResMut<FogTweakSettings>,
    fog_overlay: Query<&MeshMaterial3d<FogOfWarMaterial>, With<FogOverlay>>,
    mut materials: ResMut<Assets<FogOfWarMaterial>>,
) {
    // ── FoW Shader folder → material settings ──
    let Ok(mat_handle) = fog_overlay.single() else {
        return;
    };
    if let Some(mat) = materials.get_mut(&mat_handle.0) {
        if let Some(v) = tweaks.get_float("FoW Shader", "Noise Scale") {
            mat.settings.noise_scale = v;
        }
        if let Some(v) = tweaks.get_float("FoW Shader", "Edge Glow Width") {
            mat.settings.edge_glow_width = v;
        }
        if let Some(v) = tweaks.get_float("FoW Shader", "Edge Glow Intensity") {
            mat.settings.edge_glow_intensity = v;
        }
        if let (Some(r), Some(g), Some(b), Some(a)) = (
            tweaks.get_float("FoW Shader", "Fog Color R"),
            tweaks.get_float("FoW Shader", "Fog Color G"),
            tweaks.get_float("FoW Shader", "Fog Color B"),
            tweaks.get_float("FoW Shader", "Fog Color A"),
        ) {
            mat.settings.fog_color = Vec4::new(r, g, b, a);
        }
        if let (Some(r), Some(g), Some(b), Some(a)) = (
            tweaks.get_float("FoW Shader", "Glow Color R"),
            tweaks.get_float("FoW Shader", "Glow Color G"),
            tweaks.get_float("FoW Shader", "Glow Color B"),
            tweaks.get_float("FoW Shader", "Glow Color A"),
        ) {
            mat.settings.glow_color = Vec4::new(r, g, b, a);
        }
        if let (Some(r), Some(g), Some(b), Some(a)) = (
            tweaks.get_float("FoW Shader", "Explored Tint R"),
            tweaks.get_float("FoW Shader", "Explored Tint G"),
            tweaks.get_float("FoW Shader", "Explored Tint B"),
            tweaks.get_float("FoW Shader", "Explored Tint A"),
        ) {
            mat.settings.explored_tint = Vec4::new(r, g, b, a);
        }
    }

    // ── FoW Gameplay folder → FogTweakSettings ──
    if let Some(v) = tweaks.get_float("FoW Gameplay", "Mob Threshold") {
        fog_settings.mob_threshold = v;
    }
    if let Some(v) = tweaks.get_float("FoW Gameplay", "Object Threshold") {
        fog_settings.object_threshold = v;
    }
    if let Some(v) = tweaks.get_float("FoW Gameplay", "VFX Threshold") {
        fog_settings.vfx_threshold = v;
    }
    if let Some(v) = tweaks.get_float("FoW Gameplay", "Decay Value") {
        fog_settings.decay_value = v;
    }
}

// ── Apply saved config on startup (runs once) ──

#[derive(Resource)]
struct ConfigApplied;

fn apply_saved_config(
    mut commands: Commands,
    mut tweaks: ResMut<DebugTweaks>,
    applied: Option<Res<ConfigApplied>>,
) {
    if applied.is_some() {
        return;
    }
    // Only run once all folders have been registered (not empty)
    if tweaks.folders.is_empty() {
        return;
    }
    commands.insert_resource(ConfigApplied);

    if let Some(config) = load_debug_config() {
        info!("Loaded debug config from {}", DEBUG_CONFIG_PATH);
        apply_config_to_tweaks(&mut tweaks, &config);
    } else {
        info!("No debug config found, saving defaults to {}", DEBUG_CONFIG_PATH);
        save_debug_config(&tweaks);
    }
}

// ── Save config button click handler ──

fn handle_save_config_click(
    tweaks: Res<DebugTweaks>,
    mut feedback: ResMut<SaveConfigFeedback>,
    btn_q: Query<&Interaction, (Changed<Interaction>, With<SaveConfigButton>)>,
    mut text_q: Query<&mut Text, With<SaveConfigButtonText>>,
) {
    for interaction in &btn_q {
        if *interaction == Interaction::Pressed {
            save_debug_config(&tweaks);
            feedback.0 = Timer::from_seconds(1.0, TimerMode::Once);
            if let Ok(mut text) = text_q.single_mut() {
                **text = "Saved!".to_string();
            }
        }
    }
}

fn update_save_button_feedback(
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
