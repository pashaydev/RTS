use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::blueprints::{spawn_from_blueprint, BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::components::{
    AiControlledFactions, AiFactionSettings, AppState, Faction, GameSetupConfig, Health,
    RtsCamera, Selected, UiPressActive, UnitSpeed,
};
use crate::fog::FogTweakSettings;
use crate::ground::HeightMap;
use crate::lighting::{
    DayCycle, EntityClusterLight, EntityLightConfig, EntityLightGrid,
    LightingOverrides, SunLight,
};
use crate::model_assets::{BuildingModelAssets, UnitModelAssets};
use crate::theme;
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
            .add_systems(Startup, register_entity_debug_tweaks)
            .add_systems(
                Update,
                (
                    update_fps_tracker,
                    update_debug_texts,
                    handle_folder_collapse,
                    handle_expand_button,
                    handle_toggle_click,
                    handle_cycle_click,
                    handle_button_click,
                    handle_slider_interaction,
                    handle_save_config_click,
                    update_save_button_feedback,
                    apply_saved_config,
                    sync_lighting_tweaks,
                    #[cfg(not(target_arch = "wasm32"))]
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
                    sync_runtime_debug_tweaks,
                    sync_save_load_tweaks,
                    sync_ai_debug_tweaks,
                    sync_network_debug_tweaks,
                    initialize_debug_folder_defaults,
                    rebuild_tweak_panel,
                    update_tweak_visuals,
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
    pub tweaks_expanded: bool,
    pub collapsed_folders: Vec<String>,
    pub seen_folders: Vec<String>,
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
struct DebugExpandButton;

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
struct TweakSliderKnob {
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

fn debug_control_surface() -> Color {
    Color::srgba(0.06, 0.06, 0.06, 0.96)
}

fn debug_control_border() -> Color {
    Color::srgba(1.0, 1.0, 1.0, 0.14)
}

fn debug_hover_surface() -> Color {
    Color::srgba(0.12, 0.12, 0.12, 0.98)
}

fn debug_pressed_surface() -> Color {
    Color::srgba(0.18, 0.18, 0.18, 0.98)
}

fn debug_active_surface() -> Color {
    Color::srgba(1.0, 1.0, 1.0, 0.14)
}

fn debug_slider_fill() -> Color {
    Color::srgba(0.92, 0.92, 0.92, 0.96)
}

fn debug_text_primary() -> Color {
    Color::srgb(0.94, 0.94, 0.94)
}

fn debug_text_secondary() -> Color {
    Color::srgb(0.64, 0.64, 0.64)
}

fn debug_inverse_text() -> Color {
    Color::srgb(0.05, 0.05, 0.05)
}

fn debug_separator() -> Color {
    Color::srgba(1.0, 1.0, 1.0, 0.10)
}

fn debug_emphasis_border() -> Color {
    Color::srgba(1.0, 1.0, 1.0, 0.30)
}

fn debug_card_node() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(4.0),
        width: Val::Percent(100.0),
        padding: UiRect::all(Val::Px(6.0)),
        border: UiRect::all(Val::Px(1.0)),
        border_radius: BorderRadius::all(Val::Px(4.0)),
        ..default()
    }
}

fn debug_row_node() -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::SpaceBetween,
        column_gap: Val::Px(10.0),
        width: Val::Percent(100.0),
        padding: UiRect::axes(Val::Px(6.0), Val::Px(5.0)),
        border: UiRect::all(Val::Px(1.0)),
        border_radius: BorderRadius::all(Val::Px(4.0)),
        ..default()
    }
}

fn debug_pill_node() -> Node {
    Node {
        padding: UiRect::axes(Val::Px(6.0), Val::Px(1.0)),
        border_radius: BorderRadius::all(Val::Px(999.0)),
        border: UiRect::all(Val::Px(1.0)),
        ..default()
    }
}

/// Tracks which buttons were pressed this frame.
#[derive(Resource, Default)]
pub struct DebugButtonPressed {
    pub pressed: Vec<(String, String)>, // (folder, label)
}

// ── Populate the debug widget content area ──

pub fn spawn_debug_content(commands: &mut Commands, parent: Entity) {
    let stats_header = commands
        .spawn((
            Text::new("RUNTIME"),
            TextFont {
                font_size: theme::FONT_SMALL,
                ..default()
            },
            TextColor(debug_text_secondary()),
        ))
        .id();
    commands.entity(parent).add_child(stats_header);

    let fps = commands
        .spawn((
            DebugFpsText,
            Text::new("FPS: --"),
            TextFont {
                font_size: theme::FONT_BODY,
                ..default()
            },
            TextColor(debug_text_primary()),
            Node {
                padding: UiRect::axes(Val::Px(6.0), Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .id();
    commands.entity(parent).add_child(fps);

    let ent_count = commands
        .spawn((
            DebugEntityCountText,
            Text::new("Entities: --"),
            TextFont {
                font_size: theme::FONT_BODY,
                ..default()
            },
            TextColor(debug_text_primary()),
            Node {
                padding: UiRect::axes(Val::Px(6.0), Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .id();
    commands.entity(parent).add_child(ent_count);

    let day_cycle = commands
        .spawn((
            DebugDayCycleText,
            Text::new("Day: --"),
            TextFont {
                font_size: theme::FONT_BODY,
                ..default()
            },
            TextColor(debug_text_primary()),
            Node {
                padding: UiRect::axes(Val::Px(6.0), Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .id();
    commands.entity(parent).add_child(day_cycle);

    // Separator
    let sep = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(1.0),
                margin: UiRect::axes(Val::ZERO, Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(debug_separator()),
        ))
        .id();
    commands.entity(parent).add_child(sep);

    // Button row: Expand/Collapse + Save Config
    let btn_row = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(4.0),
            width: Val::Percent(100.0),
            ..default()
        })
        .id();
    commands.entity(parent).add_child(btn_row);

    // Expand/Collapse tweaks button
    let expand_btn = commands
        .spawn((
            DebugExpandButton,
            Interaction::default(),
            Button,
            Node {
                padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                flex_grow: 1.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .id();
    let expand_text = commands
        .spawn((
            Pickable::IGNORE,
            Text::new("Inspect"),
            TextFont {
                font_size: theme::FONT_BODY,
                ..default()
            },
            TextColor(debug_text_primary()),
        ))
        .id();
    commands.entity(expand_btn).add_child(expand_text);
    commands.entity(btn_row).add_child(expand_btn);

    // Save config button
    let save_btn = commands
        .spawn((
            SaveConfigButton,
            Interaction::default(),
            Button,
            Node {
                padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .id();
    let save_text = commands
        .spawn((
            SaveConfigButtonText,
            Pickable::IGNORE,
            Text::new("Save"),
            TextFont {
                font_size: theme::FONT_BODY,
                ..default()
            },
            TextColor(debug_text_primary()),
        ))
        .id();
    commands.entity(save_btn).add_child(save_text);
    commands.entity(btn_row).add_child(save_btn);

    // Tweak panel container (hidden by default, expanded via F3)
    let tweak_panel = commands
        .spawn((
            DebugTweakPanel,
            TweakPanelBuiltVersion(0),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                width: Val::Percent(100.0),
                ..default()
            },
            Visibility::Hidden,
        ))
        .id();
    commands.entity(parent).add_child(tweak_panel);
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

// ── Rebuild tweak panel ONLY on structural changes ──

fn rebuild_tweak_panel(
    tweaks: Res<DebugTweaks>,
    panel_state: Res<DebugPanelState>,
    mut structure: ResMut<TweakStructureVersion>,
    mut commands: Commands,
    mut panel_q: Query<
        (Entity, &mut TweakPanelBuiltVersion, &mut Visibility),
        With<DebugTweakPanel>,
    >,
    children_q: Query<&Children>,
) {
    let Ok((panel_entity, mut built_ver, mut panel_vis)) = panel_q.single_mut() else {
        return;
    };

    // Toggle visibility based on expanded state
    let target_vis = if panel_state.tweaks_expanded {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *panel_vis != target_vis {
        *panel_vis = target_vis;
    }

    if !panel_state.tweaks_expanded {
        return;
    }

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
    mut fill_q: Query<(&TweakSliderFill, &mut Node), Without<TweakSliderKnob>>,
    mut knob_q: Query<(&TweakSliderKnob, &mut Node), Without<TweakSliderFill>>,
    mut val_text_q: Query<(&TweakSliderValueText, &mut Text), Without<TweakToggleText>>,
    mut toggle_q: Query<(&TweakToggle, &Interaction, &mut BackgroundColor), Without<TweakSliderFill>>,
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
                padding: UiRect::new(Val::Px(6.0), Val::Px(6.0), Val::Px(8.0), Val::Px(3.0)),
                margin: UiRect::top(Val::Px(8.0)),
                width: Val::Percent(100.0),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(debug_separator()),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(section.to_uppercase()),
                TextFont {
                    font_size: theme::FONT_SMALL,
                    ..default()
                },
                TextColor(debug_text_secondary()),
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
                padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                margin: UiRect::top(Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(debug_control_surface()),
            BorderColor::all(if collapsed {
                debug_control_border()
            } else {
                debug_emphasis_border()
            }),
        ))
        .with_children(|header| {
            header.spawn((
                Text::new(format!("{} {}", arrow, display_name.to_uppercase())),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(debug_text_primary()),
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
        .spawn((
            debug_card_node(),
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .with_children(|card| {
            card.spawn(Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                width: Val::Percent(100.0),
                ..default()
            })
            .with_children(|top| {
                top.spawn((
                    Text::new(label),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(debug_text_primary()),
                ));

                top.spawn((
                    TweakSliderValueText {
                        folder: folder.to_string(),
                        label: label.to_string(),
                    },
                    Text::new(format_tweak_float(value)),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(debug_inverse_text()),
                    debug_pill_node(),
                    BackgroundColor(debug_slider_fill()),
                    BorderColor::all(debug_slider_fill()),
                ));
            });

            card.spawn((
                TweakSlider {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(12.0),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(999.0)),
                    overflow: Overflow::clip(),
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.06)),
                BorderColor::all(debug_control_border()),
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
                        border_radius: BorderRadius::all(Val::Px(999.0)),
                        ..default()
                    },
                    BackgroundColor(debug_slider_fill()),
                ));

                track.spawn((
                    TweakSliderKnob {
                        folder: folder.to_string(),
                        label: label.to_string(),
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(pct),
                        width: Val::Px(12.0),
                        height: Val::Px(12.0),
                        margin: UiRect::left(Val::Px(-6.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(999.0)),
                        ..default()
                    },
                    BackgroundColor(debug_text_primary()),
                    BorderColor::all(Color::BLACK),
                ));
            });
        });
}

fn spawn_toggle_row(parent: &mut ChildSpawnerCommands, folder: &str, label: &str, value: bool) {
    parent
        .spawn((
            debug_row_node(),
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(debug_text_primary()),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            ));

            let bg = if value {
                debug_active_surface()
            } else {
                debug_control_surface()
            };
            let text = if value { "ON" } else { "OFF" };

            row.spawn((
                TweakToggle {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Interaction::default(),
                Button,
                Node {
                    min_width: Val::Px(56.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(999.0)),
                    ..default()
                },
                BackgroundColor(bg),
                BorderColor::all(if value {
                    debug_emphasis_border()
                } else {
                    debug_control_border()
                }),
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
                    TextColor(debug_text_primary()),
                ));
            });
        });
}

fn spawn_readonly_row(parent: &mut ChildSpawnerCommands, folder: &str, label: &str, text: &str) {
    parent
        .spawn((
            debug_row_node(),
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(debug_text_secondary()),
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
                TextColor(debug_text_primary()),
            ));
        });
}

fn spawn_color_preview(parent: &mut ChildSpawnerCommands, folder: &str, prefix: &str) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                width: Val::Percent(100.0),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(5.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new("Preview"),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(debug_text_secondary()),
            ));

            row.spawn((
                ColorPreview {
                    folder: folder.to_string(),
                    prefix: prefix.to_string(),
                },
                Node {
                    width: Val::Px(88.0),
                    height: Val::Px(18.0),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(999.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.5, 0.5, 0.5)),
                BorderColor::all(debug_control_border()),
            ));
        });
}

fn spawn_cycle_row(parent: &mut ChildSpawnerCommands, folder: &str, label: &str, display: &str) {
    parent
        .spawn((
            debug_row_node(),
            BackgroundColor(debug_control_surface()),
            BorderColor::all(debug_control_border()),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont {
                    font_size: theme::FONT_BODY,
                    ..default()
                },
                TextColor(debug_text_primary()),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            ));

            row.spawn((
                TweakCycleEnum {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Interaction::default(),
                Button,
                Node {
                    min_width: Val::Px(124.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(2.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(999.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.04)),
                BorderColor::all(debug_control_border()),
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
                    TextColor(debug_text_primary()),
                ));
            });
        });
}

fn spawn_button_row(parent: &mut ChildSpawnerCommands, folder: &str, label: &str, text: &str) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                TweakButton {
                    folder: folder.to_string(),
                    label: label.to_string(),
                },
                Interaction::default(),
                Button,
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(debug_control_surface()),
                BorderColor::all(debug_emphasis_border()),
            ))
            .with_children(|btn| {
                btn.spawn((
                    Pickable::IGNORE,
                    Text::new(text.to_uppercase()),
                    TextFont {
                        font_size: theme::FONT_BODY,
                        ..default()
                    },
                    TextColor(debug_text_primary()),
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

fn initialize_debug_folder_defaults(
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

fn handle_expand_button(
    mut state: ResMut<DebugPanelState>,
    btn_q: Query<&Interaction, (Changed<Interaction>, With<DebugExpandButton>)>,
) {
    for interaction in &btn_q {
        if *interaction == Interaction::Pressed {
            state.tweaks_expanded = !state.tweaks_expanded;
        }
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
) {
    sync_time_of_day_tweaks(&mut tweaks, &active, &mut cycle);
    sync_sunlight_tweaks(&mut tweaks, &active, &mut overrides, &sun_q);
    sync_ambient_tweaks(&mut tweaks, &active, &mut overrides, &ambient);
    sync_sky_color_tweaks(&mut tweaks, &active, &mut overrides, &clear);
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
const RUNTIME_FOLDER: &str = "Game/Runtime";
const AI_FOLDER: &str = "Game/AI Settings";
const SAVE_FOLDER: &str = "Game/Save & Load";
const NET_CONN_FOLDER: &str = "Network/Connection";
const NET_TRAFFIC_FOLDER: &str = "Network/Traffic";

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

    // Runtime inspection folder
    tweaks.add_readonly(RUNTIME_FOLDER, "Camera Pivot", "--");
    tweaks.add_readonly(RUNTIME_FOLDER, "Camera Distance", "--");
    tweaks.add_readonly(RUNTIME_FOLDER, "Cursor World", "--");
    tweaks.add_readonly(RUNTIME_FOLDER, "UI Capture", "--");

    // AI Settings folder
    for prefix in ["P2", "P3", "P4"] {
        tweaks.add_readonly(AI_FOLDER, &format!("{prefix} AI Enabled"), "--");
        tweaks.add_readonly(AI_FOLDER, &format!("{prefix} Difficulty"), "--");
        tweaks.add_readonly(AI_FOLDER, &format!("{prefix} Personality"), "--");
        tweaks.add_readonly(AI_FOLDER, &format!("{prefix} State"), "--");
    }

    // Save & Load folder
    tweaks.add_button(SAVE_FOLDER, "Save Game");
    tweaks.add_button(SAVE_FOLDER, "Load Game");
    tweaks.add_readonly(SAVE_FOLDER, "Status", "Ready");

    // Network folders — driven by the field table in multiplayer::mod
    for field in crate::multiplayer::NET_STAT_FIELDS {
        let folder = net_folder(field.folder_key);
        tweaks.add_readonly(folder, field.label, "--");
    }
    tweaks.add_readonly(NET_CONN_FOLDER, "Tap API", "--");
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

fn format_debug_vec3(v: Vec3) -> String {
    format!("{:.1}, {:.1}, {:.1}", v.x, v.y, v.z)
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
        && panel_state.tweaks_expanded
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

fn sync_runtime_debug_tweaks(
    mut tweaks: ResMut<DebugTweaks>,
    camera_q: Query<&RtsCamera>,
    cam_query: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_press: Res<UiPressActive>,
) {
    if let Ok(camera) = camera_q.single() {
        tweaks.set_readonly_if_changed(
            RUNTIME_FOLDER,
            "Camera Pivot",
            &format_debug_vec3(camera.pivot),
        );
        tweaks.set_readonly_if_changed(
            RUNTIME_FOLDER,
            "Camera Distance",
            &format!("{:.1}", camera.distance),
        );
    }

    let cursor_text = cursor_ground_pos(&cam_query, &windows)
        .map(format_debug_vec3)
        .unwrap_or_else(|| "--".to_string());
    tweaks.set_readonly_if_changed(RUNTIME_FOLDER, "Cursor World", &cursor_text);
    tweaks.set_readonly_if_changed(
        RUNTIME_FOLDER,
        "UI Capture",
        if ui_press.0 { "Dragging UI" } else { "Free" },
    );
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

fn sync_ai_debug_tweaks(
    mut tweaks: ResMut<DebugTweaks>,
    game_config: Res<GameSetupConfig>,
    ai_controlled: Res<AiControlledFactions>,
    ai_settings: Res<AiFactionSettings>,
) {
    let rows = [
        ("P2", Faction::Player2),
        ("P3", Faction::Player3),
        ("P4", Faction::Player4),
    ];
    for (prefix, faction) in rows {
        let configured = crate::ai::types::faction_uses_ai(&game_config, faction);
        let running = ai_controlled.factions.contains(&faction) && configured;

        tweaks.set_readonly_if_changed(
            AI_FOLDER,
            &format!("{prefix} AI Enabled"),
            if running {
                "Yes"
            } else if configured {
                "Configured"
            } else {
                "No"
            },
        );

        if let Some(config) = ai_settings.settings.get(&faction) {
            tweaks.set_readonly_if_changed(
                AI_FOLDER,
                &format!("{prefix} Difficulty"),
                &format!("{:?}", config.difficulty),
            );
            tweaks.set_readonly_if_changed(
                AI_FOLDER,
                &format!("{prefix} Personality"),
                &format!("{:?}", config.personality),
            );

            let status = if running {
                format!(
                    "{} {} | Str:{:.1} W:{} M:{} Atk:{} Def:{}",
                    config.phase_name,
                    config.posture_name,
                    config.relative_strength,
                    config.worker_count,
                    config.military_count,
                    config.attack_squad_size,
                    config.defense_squad_size
                )
            } else if configured {
                "Pending brain sync".to_string()
            } else {
                "Disabled".to_string()
            };
            tweaks.set_readonly_if_changed(AI_FOLDER, &format!("{prefix} State"), &status);
        } else {
            tweaks.set_readonly_if_changed(
                AI_FOLDER,
                &format!("{prefix} Difficulty"),
                if configured { "Unknown" } else { "--" },
            );
            tweaks.set_readonly_if_changed(
                AI_FOLDER,
                &format!("{prefix} Personality"),
                if configured { "Unknown" } else { "--" },
            );
            tweaks.set_readonly_if_changed(
                AI_FOLDER,
                &format!("{prefix} State"),
                if configured {
                    "Pending brain sync"
                } else {
                    "Disabled"
                },
            );
        }
    }
}

/// Maps folder key shorthand → folder constant.
fn net_folder(key: &str) -> &'static str {
    match key {
        "conn" => NET_CONN_FOLDER,
        "traffic" => NET_TRAFFIC_FOLDER,
        _ => NET_CONN_FOLDER,
    }
}

fn sync_network_debug_tweaks(
    mut tweaks: ResMut<DebugTweaks>,
    net_stats: Option<Res<crate::multiplayer::NetStats>>,
    role: Res<crate::multiplayer::NetRole>,
    lobby: Option<Res<crate::multiplayer::LobbyState>>,
) {
    use crate::multiplayer::{NetRole, NetStatVisibility, NET_STAT_FIELDS};

    // "Status" comes from LobbyState, not NetStats — handle it separately
    let status = match (*role, &lobby) {
        (NetRole::Offline, _) => "Offline".to_string(),
        (_, Some(lobby)) => format!("{:?}", lobby.status),
        _ => "--".to_string(),
    };
    tweaks.set_readonly_if_changed(NET_CONN_FOLDER, "Status", &status);
    let tap_api = crate::multiplayer::debug_tap::http_addr()
        .map(|addr| format!("http://{addr}/events"))
        .unwrap_or_else(|| "--".to_string());
    tweaks.set_readonly_if_changed(NET_CONN_FOLDER, "Tap API", &tap_api);

    // Default stats for when resource isn't present yet
    let default_stats = crate::multiplayer::NetStats::default();
    let stats = net_stats.as_deref().unwrap_or(&default_stats);

    for field in NET_STAT_FIELDS {
        if field.label == "Status" {
            continue; // handled above
        }

        let folder = net_folder(field.folder_key);
        let visible = match field.visibility {
            NetStatVisibility::Always => *role != NetRole::Offline,
            NetStatVisibility::HostOnly => *role == NetRole::Host,
            NetStatVisibility::ClientOnly => *role == NetRole::Client,
        };

        let display = if visible {
            stats.display_value(field.label, &role).unwrap_or_else(|| "--".to_string())
        } else {
            "--".to_string()
        };
        tweaks.set_readonly_if_changed(folder, field.label, &display);
    }
}
