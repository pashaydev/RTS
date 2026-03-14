use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::ecs::message::MessageReader;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::blueprints::{spawn_from_blueprint, BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::components::{
    ActivePlayer, AiControlledFactions, AiDifficulty, AiFactionSettings, AiPersonality, AppState,
    Faction, Health, InspectedEnemy, RtsCamera, Selected, TeamConfig, UiClickedThisFrame,
    UiPressActive, UnitSpeed,
};
use crate::fog::FogTweakSettings;
use crate::ground::HeightMap;
use crate::lighting::{
    AtmosphericFogVolume, DayCycle, EntityClusterLight, EntityLightConfig, EntityLightGrid,
    LightingOverrides, SunLight,
};
use crate::model_assets::{BuildingModelAssets, UnitModelAssets};
use crate::theme;
#[cfg(not(target_arch = "wasm32"))]
use bevy::light::{FogVolume, VolumetricFog};
use bevy::window::PrimaryWindow;

const DEBUG_CONFIG_PATH: &str = "config/debug_tweaks.json";

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugTweaks>()
            .init_resource::<DebugPanelState>()
            .init_resource::<FpsTracker>()
            .init_resource::<ActiveSlider>()
            .init_resource::<TweakStructureVersion>()
            .init_resource::<DebugButtonPressed>()
            .init_resource::<DebugSpawnState>()
            .insert_resource(SaveConfigFeedback(Timer::from_seconds(
                0.0,
                TimerMode::Once,
            )))
            .add_systems(Startup, (spawn_debug_overlay, register_entity_debug_tweaks))
            .add_systems(
                Update,
                (
                    toggle_debug_panel,
                    update_fps_tracker,
                    update_debug_texts,
                    handle_folder_collapse,
                    handle_toggle_click,
                    handle_cycle_click,
                    handle_button_click,
                    handle_slider_interaction,
                    handle_debug_scroll,
                    handle_save_config_click,
                    update_save_button_feedback,
                    apply_saved_config,
                    sync_lighting_tweaks,
                    sync_entity_light_tweaks,
                    sync_fog_tweaks,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    sync_entity_spawn_tweaks,
                    sync_entity_selected_tweaks,
                    sync_save_load_tweaks,
                    sync_player_control_tweaks,
                    sync_ai_debug_tweaks,
                    rebuild_tweak_panel,
                    update_tweak_visuals,
                    block_input_over_debug_panel,
                )
                    .run_if(in_state(AppState::InGame)),
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

    fn get_color_rgb(&self, folder: &str) -> Option<[f32; 3]> {
        match (
            self.get_float(folder, "Color R"),
            self.get_float(folder, "Color G"),
            self.get_float(folder, "Color B"),
        ) {
            (Some(r), Some(g), Some(b)) => Some([r, g, b]),
            _ => None,
        }
    }

    fn sync_color_rgb_back(&mut self, folder: &str, color: &Srgba, active: &ActiveSlider) {
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

fn save_debug_config(_tweaks: &DebugTweaks) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut map: ConfigMap = BTreeMap::new();
        for (folder, entries) in &_tweaks.folders {
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

impl ActiveSlider {
    fn is_dragging(&self, folder: &str, label: &str) -> bool {
        self.folder.as_deref() == Some(folder) && self.label.as_deref() == Some(label)
    }
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

#[derive(Component)]
struct TweakCycleEnum {
    folder: String,
    label: String,
}

#[derive(Component)]
struct TweakCycleText {
    folder: String,
    label: String,
}

#[derive(Component)]
struct TweakButton {
    folder: String,
    label: String,
}

/// Tracks which buttons were pressed this frame.
#[derive(Resource, Default)]
pub struct DebugButtonPressed {
    pub pressed: Vec<(String, String)>, // (folder, label)
}

// ── Spawn the debug overlay (hidden by default) ──

fn spawn_debug_overlay(mut commands: Commands) {
    commands
        .spawn((
            DebugOverlayRoot,
            Interaction::default(),
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
                            font_size: theme::FONT_LARGE,
                            ..default()
                        },
                        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.9)),
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
                        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.15)),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            SaveConfigButtonText,
                            Pickable::IGNORE,
                            Text::new("Save"),
                            TextFont {
                                font_size: theme::FONT_BODY,
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
                    font_size: theme::FONT_MEDIUM,
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.7)),
            ));

            // Entity count
            panel.spawn((
                DebugEntityCountText,
                Text::new("Entities: --"),
                TextFont {
                    font_size: theme::FONT_MEDIUM,
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.7)),
            ));

            // Day cycle
            panel.spawn((
                DebugDayCycleText,
                Text::new("Day: --"),
                TextFont {
                    font_size: theme::FONT_MEDIUM,
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.7)),
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

// ── Block gameplay input when cursor is over the debug panel ──

fn block_input_over_debug_panel(
    state: Res<DebugPanelState>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
    mut ui_press: ResMut<UiPressActive>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    panel_q: Query<(&ComputedNode, &UiGlobalTransform), With<DebugOverlayRoot>>,
) {
    if !state.visible {
        return;
    }

    let Some(cursor_phys) = windows
        .single()
        .ok()
        .and_then(|w| w.physical_cursor_position())
    else {
        return;
    };

    for (computed, ui_tf) in &panel_q {
        if computed.contains_point(*ui_tf, cursor_phys) {
            // Cursor is over the debug panel — block all gameplay input
            if mouse.pressed(MouseButton::Left) || mouse.pressed(MouseButton::Right) {
                ui_press.0 = true;
            }
            if mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right) {
                ui_clicked.0 = 2;
            }
        }
    }
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
        let mut current_section: Option<String> = None;
        for (folder_name, entries) in &tweaks.folders {
            // Detect section prefix (e.g., "Visuals/Sunlight" → section "Visuals")
            let (section, display_name) = if let Some(idx) = folder_name.find('/') {
                (Some(&folder_name[..idx]), &folder_name[idx + 1..])
            } else {
                (None, folder_name.as_str())
            };

            // Render section header when section changes
            let section_str = section.map(|s| s.to_string());
            if section_str != current_section {
                if let Some(ref sec) = section_str {
                    spawn_section_header(panel, sec);
                }
                current_section = section_str;
            }

            let collapsed = panel_state.collapsed_folders.contains(folder_name);
            spawn_folder_header(panel, folder_name, display_name, collapsed);

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
                            color_prefix = Some(entry.label.trim_end_matches(" R").to_string());
                        } else if entry.label.ends_with(" B") {
                            if let Some(ref prefix) = color_prefix {
                                let expected_b = format!("{} B", prefix);
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
                    TweakValue::CycleEnum { options, selected } => {
                        let display = options.get(*selected).map(|s| s.as_str()).unwrap_or("--");
                        spawn_cycle_row(panel, folder_name, &entry.label, display);
                        color_prefix = None;
                    }
                    TweakValue::Button { text } => {
                        spawn_button_row(panel, folder_name, &entry.label, text);
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
    if !state.visible {
        return;
    }

    // Update cycle enum texts
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
                Color::srgba(1.0, 1.0, 1.0, 0.35)
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

fn spawn_section_header(parent: &mut ChildSpawnerCommands, section: &str) {
    parent
        .spawn((
            Node {
                padding: UiRect::axes(Val::Px(4.0), Val::Px(2.0)),
                margin: UiRect::top(Val::Px(10.0)),
                width: Val::Percent(100.0),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.2)),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(section.to_uppercase()),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(Color::srgba(0.6, 0.8, 1.0, 0.7)),
            ));
        });
}

fn spawn_folder_header(
    parent: &mut ChildSpawnerCommands,
    key: &str,
    display_name: &str,
    collapsed: bool,
) {
    let arrow = if collapsed { ">" } else { "v" };
    parent
        .spawn((
            FolderHeader(key.to_string()),
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
                Text::new(format!("{} {}", arrow, display_name)),
                TextFont {
                    font_size: theme::FONT_MEDIUM,
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
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
                    font_size: theme::FONT_BODY,
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
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.5)),
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
                    font_size: theme::FONT_BODY,
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
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(Color::srgb(0.65, 0.65, 0.65)),
                Node {
                    width: Val::Px(95.0),
                    ..default()
                },
            ));

            let (bg, text) = if value {
                (Color::srgba(1.0, 1.0, 1.0, 0.35), "ON")
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
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
        });
}

fn spawn_readonly_row(parent: &mut ChildSpawnerCommands, folder: &str, label: &str, text: &str) {
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
                    font_size: theme::FONT_BODY,
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
                    font_size: theme::FONT_BODY,
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
                    font_size: theme::FONT_BODY,
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

fn spawn_cycle_row(parent: &mut ChildSpawnerCommands, folder: &str, label: &str, display: &str) {
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
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(Color::srgb(0.65, 0.65, 0.65)),
                Node {
                    width: Val::Px(95.0),
                    ..default()
                },
            ));

            // Clickable cycle button
            row.spawn((
                TweakCycleEnum {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Interaction::default(),
                Button,
                Node {
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    min_width: Val::Px(120.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.3, 0.5, 1.0, 0.25)),
            ))
            .with_children(|btn| {
                btn.spawn((
                    TweakCycleText {
                        folder: folder.to_string(),
                        label: label.to_string(),
                    },
                    Pickable::IGNORE,
                    Text::new(display),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
        });
}

fn spawn_button_row(parent: &mut ChildSpawnerCommands, folder: &str, label: &str, text: &str) {
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
            // Spacer to align with other rows
            row.spawn(Node {
                width: Val::Px(95.0),
                ..default()
            });

            // Button
            row.spawn((
                TweakButton {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Interaction::default(),
                Button,
                Node {
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(3.0)),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 0.6, 0.2, 0.3)),
            ))
            .with_children(|btn| {
                btn.spawn((
                    Pickable::IGNORE,
                    Text::new(text),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
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
        (&mut ScrollPosition, &ComputedNode, &UiGlobalTransform),
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
        let max_scroll = (computed.content_size().y - computed.size().y).max(0.0)
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

fn handle_cycle_click(
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

fn handle_button_click(
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

fn handle_slider_interaction(
    mut tweaks: ResMut<DebugTweaks>,
    mut active: ResMut<ActiveSlider>,
    mut ui_press: ResMut<UiPressActive>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    slider_q: Query<(&TweakSlider, &ComputedNode, &UiGlobalTransform)>,
) {
    // Release when mouse not pressed
    if !mouse.pressed(MouseButton::Left) {
        if active.folder.is_some() {
            active.folder = None;
            active.label = None;
            ui_press.0 = false;
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
                ui_press.0 = true;
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

// ── Sync: Lighting ↔ DebugTweaks ──

fn sync_lighting_tweaks(
    mut tweaks: ResMut<DebugTweaks>,
    active: Res<ActiveSlider>,
    mut cycle: ResMut<DayCycle>,
    mut overrides: ResMut<LightingOverrides>,
    sun_q: Query<(&DirectionalLight, &Transform), With<SunLight>>,
    ambient: Res<GlobalAmbientLight>,
    clear: Res<ClearColor>,
    #[cfg(not(target_arch = "wasm32"))] mut fog_vol_q: Query<
        &mut FogVolume,
        With<AtmosphericFogVolume>,
    >,
    #[cfg(not(target_arch = "wasm32"))] mut cam_fog_q: Query<&mut VolumetricFog>,
) {
    sync_time_of_day_tweaks(&mut tweaks, &active, &mut cycle);
    sync_sunlight_tweaks(&mut tweaks, &active, &mut overrides, &sun_q);
    sync_ambient_tweaks(&mut tweaks, &active, &mut overrides, &ambient);
    sync_sky_color_tweaks(&mut tweaks, &active, &mut overrides, &clear);
    #[cfg(not(target_arch = "wasm32"))]
    sync_volumetric_fog_tweaks(
        &mut tweaks,
        &active,
        &mut overrides,
        &mut fog_vol_q,
        &mut cam_fog_q,
    );
}

fn sync_time_of_day_tweaks(
    tweaks: &mut DebugTweaks,
    active: &ActiveSlider,
    cycle: &mut DayCycle,
) {
    if let Some(v) = tweaks.get_float("Visuals/Time of Day", "Cycle Duration") {
        if (cycle.cycle_duration - v).abs() > f32::EPSILON {
            cycle.cycle_duration = v;
        }
    }
    if let Some(v) = tweaks.get_bool("Visuals/Time of Day", "Paused") {
        if cycle.paused != v {
            cycle.paused = v;
        }
    }
    if let Some(v) = tweaks.get_float("Visuals/Time of Day", "Time") {
        if cycle.paused && (cycle.time - v).abs() > 0.001 {
            cycle.time = v;
        }
    }
    tweaks.set_readonly_if_changed(
        "Visuals/Time of Day",
        "Phase",
        &format!("{:?}", cycle.phase),
    );
    if !cycle.paused && !active.is_dragging("Visuals/Time of Day", "Time") {
        tweaks.set_float_if_changed("Visuals/Time of Day", "Time", cycle.time);
    }
}

fn sync_sunlight_tweaks(
    tweaks: &mut DebugTweaks,
    active: &ActiveSlider,
    overrides: &mut LightingOverrides,
    sun_q: &Query<(&DirectionalLight, &Transform), With<SunLight>>,
) {
    let sun_override = tweaks
        .get_bool("Visuals/Sunlight", "Override")
        .unwrap_or(false);
    if sun_override {
        overrides.sun_illuminance = tweaks.get_float("Visuals/Sunlight", "Illuminance");
        overrides.sun_color = tweaks.get_color_rgb("Visuals/Sunlight");
        overrides.sun_pitch = tweaks.get_float("Visuals/Sunlight", "Pitch");
        overrides.sun_yaw = tweaks.get_float("Visuals/Sunlight", "Yaw");
        overrides.shadows_enabled = tweaks.get_bool("Visuals/Sunlight", "Shadows");
    } else {
        overrides.sun_illuminance = None;
        overrides.sun_color = None;
        overrides.sun_pitch = None;
        overrides.sun_yaw = None;

        if let Ok((sun, sun_tf)) = sun_q.single() {
            if !active.is_dragging("Visuals/Sunlight", "Illuminance") {
                tweaks.set_float_if_changed("Visuals/Sunlight", "Illuminance", sun.illuminance);
            }
            tweaks.sync_color_rgb_back("Visuals/Sunlight", &sun.color.to_srgba(), active);

            let (pitch, yaw, _) = sun_tf.rotation.to_euler(EulerRot::XYZ);
            if !active.is_dragging("Visuals/Sunlight", "Pitch") {
                tweaks.set_float_if_changed("Visuals/Sunlight", "Pitch", pitch);
            }
            if !active.is_dragging("Visuals/Sunlight", "Yaw") {
                tweaks.set_float_if_changed("Visuals/Sunlight", "Yaw", yaw);
            }
        }

        overrides.shadows_enabled = tweaks.get_bool("Visuals/Sunlight", "Shadows");
    }
}

fn sync_ambient_tweaks(
    tweaks: &mut DebugTweaks,
    active: &ActiveSlider,
    overrides: &mut LightingOverrides,
    ambient: &GlobalAmbientLight,
) {
    let amb_override = tweaks
        .get_bool("Visuals/Ambient Light", "Override")
        .unwrap_or(false);
    if amb_override {
        overrides.ambient_brightness = tweaks.get_float("Visuals/Ambient Light", "Brightness");
        overrides.ambient_color = tweaks.get_color_rgb("Visuals/Ambient Light");
    } else {
        overrides.ambient_brightness = None;
        overrides.ambient_color = None;

        if !active.is_dragging("Visuals/Ambient Light", "Brightness") {
            tweaks.set_float_if_changed("Visuals/Ambient Light", "Brightness", ambient.brightness);
        }
        tweaks.sync_color_rgb_back("Visuals/Ambient Light", &ambient.color.to_srgba(), active);
    }
}

fn sync_sky_color_tweaks(
    tweaks: &mut DebugTweaks,
    active: &ActiveSlider,
    overrides: &mut LightingOverrides,
    clear: &ClearColor,
) {
    let fog_override = tweaks
        .get_bool("Visuals/Sky Color", "Override")
        .unwrap_or(false);
    if fog_override {
        overrides.fog_color = tweaks.get_color_rgb("Visuals/Sky Color");
    } else {
        overrides.fog_color = None;
        tweaks.sync_color_rgb_back("Visuals/Sky Color", &clear.0.to_srgba(), active);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn sync_volumetric_fog_tweaks(
    tweaks: &mut DebugTweaks,
    active: &ActiveSlider,
    overrides: &mut LightingOverrides,
    fog_vol_q: &mut Query<&mut FogVolume, With<AtmosphericFogVolume>>,
    cam_fog_q: &mut Query<&mut VolumetricFog>,
) {
    let vol_enabled = tweaks
        .get_bool("Visuals/Volumetric Fog", "Enabled")
        .unwrap_or(true);
    let vol_override = tweaks
        .get_bool("Visuals/Volumetric Fog", "Override")
        .unwrap_or(false);

    if let Ok(mut fog_vol) = fog_vol_q.single_mut() {
        if !vol_enabled {
            fog_vol.density_factor = 0.0;
        }
    }

    if let Ok(mut vol_fog) = cam_fog_q.single_mut() {
        if !vol_enabled {
            vol_fog.step_count = 0;
        } else if let Some(sc) = tweaks.get_float("Visuals/Volumetric Fog", "Step Count") {
            vol_fog.step_count = sc as u32;
        }
    }

    if vol_override {
        overrides.vol_density = tweaks.get_float("Visuals/Volumetric Fog", "Density");
        overrides.vol_color = tweaks.get_color_rgb("Visuals/Volumetric Fog");
        overrides.vol_ambient_intensity =
            tweaks.get_float("Visuals/Volumetric Fog", "Ambient Intensity");
        overrides.vol_light_intensity =
            tweaks.get_float("Visuals/Volumetric Fog", "Light Intensity");

        if let Ok(mut fog_vol) = fog_vol_q.single_mut() {
            if let Some(s) = tweaks.get_float("Visuals/Volumetric Fog", "Scattering") {
                fog_vol.scattering = s;
            }
            if let Some(a) = tweaks.get_float("Visuals/Volumetric Fog", "Absorption") {
                fog_vol.absorption = a;
            }
        }
    } else {
        overrides.vol_density = None;
        overrides.vol_color = None;
        overrides.vol_ambient_intensity = None;
        overrides.vol_light_intensity = None;

        if let Ok(fog_vol) = fog_vol_q.single() {
            if !active.is_dragging("Visuals/Volumetric Fog", "Density") {
                tweaks.set_float_if_changed(
                    "Visuals/Volumetric Fog",
                    "Density",
                    fog_vol.density_factor,
                );
            }
            tweaks.sync_color_rgb_back(
                "Visuals/Volumetric Fog",
                &fog_vol.fog_color.to_srgba(),
                active,
            );
            if !active.is_dragging("Visuals/Volumetric Fog", "Light Intensity") {
                tweaks.set_float_if_changed(
                    "Visuals/Volumetric Fog",
                    "Light Intensity",
                    fog_vol.light_intensity,
                );
            }
            if !active.is_dragging("Visuals/Volumetric Fog", "Scattering") {
                tweaks.set_float_if_changed(
                    "Visuals/Volumetric Fog",
                    "Scattering",
                    fog_vol.scattering,
                );
            }
            if !active.is_dragging("Visuals/Volumetric Fog", "Absorption") {
                tweaks.set_float_if_changed(
                    "Visuals/Volumetric Fog",
                    "Absorption",
                    fog_vol.absorption,
                );
            }
        }

        if let Ok(vol_fog) = cam_fog_q.single() {
            if !active.is_dragging("Visuals/Volumetric Fog", "Ambient Intensity") {
                tweaks.set_float_if_changed(
                    "Visuals/Volumetric Fog",
                    "Ambient Intensity",
                    vol_fog.ambient_intensity,
                );
            }
            if !active.is_dragging("Visuals/Volumetric Fog", "Step Count") {
                tweaks.set_float_if_changed(
                    "Visuals/Volumetric Fog",
                    "Step Count",
                    vol_fog.step_count as f32,
                );
            }
        }
    }
}

// ── Sync: Entity Lights ↔ DebugTweaks ──

fn sync_entity_light_tweaks(
    mut tweaks: ResMut<DebugTweaks>,
    mut config: ResMut<EntityLightConfig>,
    mut grid: ResMut<EntityLightGrid>,
    cluster_lights: Query<&EntityClusterLight>,
) {
    if let Some(v) = tweaks.get_bool("Visuals/Entity Lights", "Enabled") {
        config.enabled = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/Entity Lights", "Cell Size") {
        grid.cell_size = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/Entity Lights", "Max Lights") {
        grid.max_lights = v as usize;
    }
    if let Some(v) = tweaks.get_float("Visuals/Entity Lights", "Building Intensity") {
        config.building_base_intensity = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/Entity Lights", "Unit Intensity") {
        config.unit_base_intensity = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/Entity Lights", "Night Factor") {
        config.night_factor = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/Entity Lights", "Day Factor") {
        config.day_factor = v;
    }

    let count = cluster_lights.iter().count();
    tweaks.set_readonly_if_changed("Visuals/Entity Lights", "Active Lights", &count.to_string());
}

// ── Sync: Fog ↔ DebugTweaks ──
// Owns "Visuals/FoW Gameplay" folder. Shader params ("Visuals/FoW Shader") are
// synced in fog.rs::update_fog_material_time.

fn sync_fog_tweaks(tweaks: Res<DebugTweaks>, mut fog_settings: ResMut<FogTweakSettings>) {
    // Shader tweaks are now applied directly in fog.rs update_fog_material_time.
    // Only sync gameplay settings here.

    // ── FoW Gameplay folder → FogTweakSettings ──
    if let Some(v) = tweaks.get_float("Visuals/FoW Gameplay", "Mob Threshold") {
        fog_settings.mob_threshold = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Gameplay", "Object Threshold") {
        fog_settings.object_threshold = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Gameplay", "VFX Threshold") {
        fog_settings.vfx_threshold = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Gameplay", "Transition Speed") {
        fog_settings.transition_speed = v;
    }
    if let Some(v) = tweaks.get_bool("Visuals/FoW Gameplay", "Enable LOS") {
        fog_settings.enable_los = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Gameplay", "LOS Ray Count") {
        fog_settings.los_ray_count = v as usize;
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
        info!(
            "No debug config found, saving defaults to {}",
            DEBUG_CONFIG_PATH
        );
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

// ══════════════════════════════════════════════════════════════════════
// Entity Debug Tool
// ══════════════════════════════════════════════════════════════════════

#[derive(Resource, Default)]
pub struct DebugSpawnState {
    pub click_to_spawn: bool,
    pub status_text: String,
    pub status_timer: f32,
}

const SPAWN_FOLDER: &str = "Entities/Spawn";
const SELECTED_FOLDER: &str = "Entities/Selected";
const PLAYER_FOLDER: &str = "Game/Player Control";
const AI_FOLDER: &str = "Game/AI Settings";
const SAVE_FOLDER: &str = "Game/Save & Load";

fn register_entity_debug_tweaks(mut tweaks: ResMut<DebugTweaks>) {
    // Spawn folder
    let entity_names: Vec<String> = EntityKind::ALL
        .iter()
        .map(|k| k.display_name().to_string())
        .collect();
    tweaks.add_cycle_enum(SPAWN_FOLDER, "Entity Type", entity_names, 0);
    tweaks.add_cycle_enum(
        SPAWN_FOLDER,
        "Faction",
        vec![
            "Player 1".to_string(),
            "Player 2".to_string(),
            "Player 3".to_string(),
            "Player 4".to_string(),
            "Neutral".to_string(),
        ],
        0,
    );
    tweaks.add_button(SPAWN_FOLDER, "Spawn at Camera");
    tweaks.add_bool(SPAWN_FOLDER, "Click to Place", false);
    tweaks.add_readonly(SPAWN_FOLDER, "Status", "Ready");

    // Selected entity manipulation folder
    tweaks.add_readonly(SELECTED_FOLDER, "Count", "0");
    tweaks.add_readonly(SELECTED_FOLDER, "Type", "--");
    tweaks.add_float(SELECTED_FOLDER, "Set HP %", 100.0, 0.0, 100.0, 1.0);
    tweaks.add_float(SELECTED_FOLDER, "Set Speed", 5.0, 0.0, 20.0, 0.5);
    tweaks.add_button(SELECTED_FOLDER, "Kill Selected");
    tweaks.add_button(SELECTED_FOLDER, "Delete Selected");

    // Player control folder
    tweaks.add_cycle_enum(
        PLAYER_FOLDER,
        "Active Player",
        vec![
            "Player 1".to_string(),
            "Player 2".to_string(),
            "Player 3".to_string(),
            "Player 4".to_string(),
        ],
        0,
    );

    // AI Settings folder
    for prefix in ["P2", "P3", "P4"] {
        tweaks.add_bool(AI_FOLDER, &format!("{prefix} AI Enabled"), true);
        tweaks.add_cycle_enum(
            AI_FOLDER,
            &format!("{prefix} Difficulty"),
            vec!["Easy".into(), "Medium".into(), "Hard".into()],
            1,
        );
        tweaks.add_cycle_enum(
            AI_FOLDER,
            &format!("{prefix} Personality"),
            vec!["Balanced".into(), "Aggressive".into(), "Defensive".into(), "Economic".into(), "Supportive".into()],
            0,
        );
        tweaks.add_readonly(AI_FOLDER, &format!("{prefix} State"), "--");
    }

    // Save & Load folder
    tweaks.add_button(SAVE_FOLDER, "Save Game");
    tweaks.add_button(SAVE_FOLDER, "Load Game");
    tweaks.add_readonly(SAVE_FOLDER, "Status", "Ready");
}

fn cursor_ground_pos(
    camera_q: &Query<(&Camera, &GlobalTransform)>,
    windows: &Query<&Window, With<PrimaryWindow>>,
) -> Option<Vec3> {
    let Ok(window) = windows.single() else {
        return None;
    };
    let cursor = window.cursor_position()?;
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return None;
    };
    let Ok(ray) = camera.viewport_to_world(cam_gt, cursor) else {
        return None;
    };
    let dist = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
    Some(ray.get_point(dist))
}

fn get_selected_kind_and_faction(tweaks: &DebugTweaks) -> (EntityKind, Faction) {
    let kind_idx = tweaks
        .get_cycle_selected(SPAWN_FOLDER, "Entity Type")
        .unwrap_or(0);
    let faction_idx = tweaks
        .get_cycle_selected(SPAWN_FOLDER, "Faction")
        .unwrap_or(0);
    let kind = EntityKind::ALL
        .get(kind_idx)
        .copied()
        .unwrap_or(EntityKind::Worker);
    let faction = match faction_idx {
        0 => Faction::Player1,
        1 => Faction::Player2,
        2 => Faction::Player3,
        3 => Faction::Player4,
        _ => Faction::Neutral,
    };
    (kind, faction)
}

fn sync_entity_spawn_tweaks(
    mut commands: Commands,
    mut tweaks: ResMut<DebugTweaks>,
    mut spawn_state: ResMut<DebugSpawnState>,
    pressed: Res<DebugButtonPressed>,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    building_models: Option<Res<BuildingModelAssets>>,
    unit_models: Option<Res<UnitModelAssets>>,
    height_map: Option<Res<HeightMap>>,
    camera_q: Query<&RtsCamera>,
    cam_query: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    panel_state: Res<DebugPanelState>,
    ui_press: Res<UiPressActive>,
) {
    // Update click-to-spawn from toggle
    if let Some(v) = tweaks.get_bool(SPAWN_FOLDER, "Click to Place") {
        spawn_state.click_to_spawn = v;
    }

    // Update status timer
    if spawn_state.status_timer > 0.0 {
        spawn_state.status_timer -= time.delta_secs();
        if spawn_state.status_timer <= 0.0 {
            spawn_state.status_text = if spawn_state.click_to_spawn {
                "Click to place...".to_string()
            } else {
                "Ready".to_string()
            };
        }
    }

    // Update status text
    if spawn_state.status_timer <= 0.0 {
        let status = if spawn_state.click_to_spawn {
            "Click to place..."
        } else {
            "Ready"
        };
        tweaks.set_readonly_if_changed(SPAWN_FOLDER, "Status", status);
    } else {
        tweaks.set_readonly_if_changed(SPAWN_FOLDER, "Status", &spawn_state.status_text);
    }

    let hm = match &height_map {
        Some(h) => h,
        None => return,
    };

    // Handle "Spawn at Camera" button
    for (folder, label) in &pressed.pressed {
        if folder == SPAWN_FOLDER && label == "Spawn at Camera" {
            let (kind, faction) = get_selected_kind_and_faction(&tweaks);
            let pivot = camera_q
                .iter()
                .next()
                .map(|c| c.pivot)
                .unwrap_or(Vec3::ZERO);
            let entity = spawn_from_blueprint(
                &mut commands,
                &cache,
                kind,
                pivot,
                &registry,
                building_models.as_deref(),
                unit_models.as_deref(),
                hm,
            );
            commands.entity(entity).insert(faction);
            spawn_state.status_text = format!("Spawned {}!", kind.display_name());
            spawn_state.status_timer = 1.5;
        }
    }

    // Handle click-to-spawn
    if spawn_state.click_to_spawn
        && mouse.just_pressed(MouseButton::Left)
        && !ui_press.0
        && panel_state.visible
    {
        if let Some(world_pos) = cursor_ground_pos(&cam_query, &windows) {
            let (kind, faction) = get_selected_kind_and_faction(&tweaks);
            let entity = spawn_from_blueprint(
                &mut commands,
                &cache,
                kind,
                world_pos,
                &registry,
                building_models.as_deref(),
                unit_models.as_deref(),
                hm,
            );
            commands.entity(entity).insert(faction);
            spawn_state.status_text = format!("Placed {}!", kind.display_name());
            spawn_state.status_timer = 1.0;
        }
    }
}

fn sync_entity_selected_tweaks(
    mut commands: Commands,
    mut tweaks: ResMut<DebugTweaks>,
    pressed: Res<DebugButtonPressed>,
    active: Res<ActiveSlider>,
    selected_q: Query<(Entity, &EntityKind), With<Selected>>,
    mut health_q: Query<(Entity, &mut Health), With<Selected>>,
    mut speed_q: Query<&mut UnitSpeed, With<Selected>>,
) {
    // Update count
    let count = selected_q.iter().count();
    tweaks.set_readonly_if_changed(SELECTED_FOLDER, "Count", &count.to_string());

    // Update type display
    if count == 0 {
        tweaks.set_readonly_if_changed(SELECTED_FOLDER, "Type", "--");
    } else {
        let mut kinds: Vec<&str> = selected_q.iter().map(|(_, k)| k.display_name()).collect();
        kinds.dedup();
        if kinds.len() == 1 {
            tweaks.set_readonly_if_changed(SELECTED_FOLDER, "Type", kinds[0]);
        } else {
            tweaks.set_readonly_if_changed(SELECTED_FOLDER, "Type", "Mixed");
        }
    }

    // Handle buttons
    for (folder, label) in &pressed.pressed {
        if folder != SELECTED_FOLDER {
            continue;
        }
        match label.as_str() {
            "Kill Selected" => {
                for (_, mut hp) in &mut health_q {
                    hp.current = 0.0;
                }
            }
            "Delete Selected" => {
                for (entity, _) in &selected_q {
                    commands.entity(entity).despawn();
                }
            }
            _ => {}
        }
    }

    // Only apply HP%/Speed sliders when actively dragging them
    if count > 0 {
        if active.is_dragging(SELECTED_FOLDER, "Set HP %") {
            if let Some(hp_pct) = tweaks.get_float(SELECTED_FOLDER, "Set HP %") {
                for (_, mut hp) in &mut health_q {
                    let target = hp.max * hp_pct / 100.0;
                    hp.current = target;
                }
            }
        }
        if active.is_dragging(SELECTED_FOLDER, "Set Speed") {
            if let Some(spd) = tweaks.get_float(SELECTED_FOLDER, "Set Speed") {
                for mut s in &mut speed_q {
                    s.0 = spd;
                }
            }
        }
    }
}

fn sync_save_load_tweaks(
    mut tweaks: ResMut<DebugTweaks>,
    mut save_req: ResMut<crate::save::SaveRequested>,
    mut load_req: ResMut<crate::save::LoadRequested>,
    status: Res<crate::save::SaveLoadStatus>,
    button_pressed: Res<DebugButtonPressed>,
) {
    // Handle button presses
    for (folder, label) in &button_pressed.pressed {
        if folder == SAVE_FOLDER {
            match label.as_str() {
                "Save Game" => save_req.0 = true,
                "Load Game" => load_req.0 = true,
                _ => {}
            }
        }
    }

    // Update status display
    if !status.message.is_empty() {
        tweaks.set_readonly_if_changed(SAVE_FOLDER, "Status", &status.message);
    } else {
        tweaks.set_readonly_if_changed(SAVE_FOLDER, "Status", "Ready");
    }
}

fn sync_player_control_tweaks(
    mut commands: Commands,
    tweaks: Res<DebugTweaks>,
    mut active_player: ResMut<ActivePlayer>,
    _team_config: ResMut<TeamConfig>,
    mut camera_q: Query<&mut RtsCamera>,
    selected_entities: Query<Entity, With<Selected>>,
    mut inspected_enemy: ResMut<InspectedEnemy>,
) {
    // Active Player switching
    let selected = tweaks
        .get_cycle_selected(PLAYER_FOLDER, "Active Player")
        .unwrap_or(0);
    let new_faction = match selected {
        0 => Faction::Player1,
        1 => Faction::Player2,
        2 => Faction::Player3,
        3 => Faction::Player4,
        _ => Faction::Player1,
    };

    if active_player.0 != new_faction {
        active_player.0 = new_faction;

        // Deselect all units/buildings from the old faction
        for entity in &selected_entities {
            commands.entity(entity).remove::<Selected>();
        }
        inspected_enemy.entity = None;

        // Move camera to the new faction's base position
        if let Some((_, (sx, sz))) = crate::components::SPAWN_POSITIONS
            .iter()
            .find(|(f, _)| *f == new_faction)
        {
            if let Ok(mut cam) = camera_q.single_mut() {
                cam.target_pivot = Vec3::new(*sx, 0.0, *sz);
            }
        }
    }
}

fn sync_ai_debug_tweaks(
    mut tweaks: ResMut<DebugTweaks>,
    mut ai_controlled: ResMut<AiControlledFactions>,
    mut ai_settings: ResMut<AiFactionSettings>,
) {
    // AI enable/disable toggles
    let factions = [
        ("P2 AI Enabled", Faction::Player2),
        ("P3 AI Enabled", Faction::Player3),
        ("P4 AI Enabled", Faction::Player4),
    ];
    for (label, faction) in &factions {
        if let Some(enabled) = tweaks.get_bool(AI_FOLDER, label) {
            if enabled {
                ai_controlled.factions.insert(*faction);
            } else {
                ai_controlled.factions.remove(faction);
            }
        }
    }

    // Difficulty per faction
    let diff_factions = [
        ("P2 Difficulty", Faction::Player2),
        ("P3 Difficulty", Faction::Player3),
        ("P4 Difficulty", Faction::Player4),
    ];
    for (label, faction) in &diff_factions {
        if let Some(selected) = tweaks.get_cycle_selected(AI_FOLDER, label) {
            let difficulty = match selected {
                0 => AiDifficulty::Easy,
                2 => AiDifficulty::Hard,
                _ => AiDifficulty::Medium,
            };
            let config = ai_settings.settings.entry(*faction).or_default();
            config.difficulty = difficulty;
        }
    }

    // Personality per faction
    let pers_factions = [
        ("P2 Personality", Faction::Player2),
        ("P3 Personality", Faction::Player3),
        ("P4 Personality", Faction::Player4),
    ];
    for (label, faction) in &pers_factions {
        if let Some(selected) = tweaks.get_cycle_selected(AI_FOLDER, label) {
            let personality = match selected {
                1 => AiPersonality::Aggressive,
                2 => AiPersonality::Defensive,
                3 => AiPersonality::Economic,
                4 => AiPersonality::Supportive,
                _ => AiPersonality::Balanced,
            };
            let config = ai_settings.settings.entry(*faction).or_default();
            config.personality = personality;
        }
    }

    // Update readonly state displays
    let state_factions = [
        ("P2 State", Faction::Player2),
        ("P3 State", Faction::Player3),
        ("P4 State", Faction::Player4),
    ];
    for (label, faction) in &state_factions {
        if let Some(config) = ai_settings.settings.get(faction) {
            let status = format!(
                "{} {} | Str:{:.1} W:{} M:{} Atk:{} Def:{}",
                config.phase_name,
                config.posture_name,
                config.relative_strength,
                config.worker_count,
                config.military_count,
                config.attack_squad_size,
                config.defense_squad_size
            );
            tweaks.set_readonly_if_changed(AI_FOLDER, label, &status);
        }
    }
}
