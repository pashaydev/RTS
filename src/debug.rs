mod config;
mod model;
mod state;
mod ui;

use bevy::dev_tools::fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin};
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::light::cluster::{ClusterConfig, ClusterZConfig};
use bevy::prelude::*;

use crate::blueprints::{spawn_from_blueprint, BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::components::{
    AiControlledFactions, AiFactionSettings, AllyNotifications, AllyNotifyKind, AppState,
    AttackTarget, CullReason, CullingBounds, Faction, FrustumCulled, FrustumDebugMode, GameFlowSet,
    GameSetupConfig, GameWorld, Health, MoveTarget, RtsCamera, Selected, UiPressActive, UnitSpeed,
};
use crate::fog::FogTweakSettings;
use crate::ground::HeightMap;
use crate::lighting::{
    DayCycle, EntityClusterLight, EntityLightConfig, EntityLightGrid, LightingOverrides, SunLight,
};
use crate::model_assets::{BuildingModelAssets, UnitModelAssets};
use crate::pathfinding::{NavPath, PathRequestQueue};
use crate::ui::core::hud::MainHudRoot;
use crate::ui::fonts::UiFonts;
use bevy::window::PrimaryWindow;
pub use model::DebugTweaks;
pub use ui::build::spawn_debug_content;

use config::apply_saved_config;
use state::{
    ActiveSlider, DebugButtonPressed, DebugPanelState, DebugSpawnState, DebugViewState, FpsTracker,
    SaveConfigFeedback, TweakStructureVersion,
};
use ui::build::rebuild_tweak_panel;
use ui::interactions::{
    handle_button_click, handle_cycle_click, handle_expand_button, handle_folder_collapse,
    handle_save_config_click, handle_slider_interaction, handle_toggle_click,
    initialize_debug_folder_defaults,
};
use ui::update::{update_debug_texts, update_save_button_feedback, update_tweak_visuals};

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        let mut fps_overlay_config = FpsOverlayConfig::default();
        fps_overlay_config.enabled = false;
        fps_overlay_config.frame_time_graph_config.enabled = false;

        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            FpsOverlayPlugin {
                config: fps_overlay_config,
            },
        ))
        .init_resource::<DebugViewState>()
        .add_systems(Update, toggle_debug_views)
        .add_systems(
            Update,
            apply_debug_view_state.in_set(GameFlowSet::Diagnostics),
        );

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
                    // Frustum debug chain: tweak sync → camera spawn → fly input → viewport update
                    (
                        sync_debug_view_tweaks,
                        sync_frustum_debug_camera,
                        frustum_debug_fly_camera,
                        update_frustum_debug_camera,
                        sync_frustum_debug_tweaks,
                    )
                        .chain(),
                    sync_debug_flow_tweaks,
                    sync_entity_spawn_tweaks,
                    sync_entity_selected_tweaks,
                    sync_runtime_debug_tweaks,
                    sync_ai_debug_tweaks,
                    sync_network_debug_tweaks,
                    initialize_debug_folder_defaults,
                    rebuild_tweak_panel,
                    update_tweak_visuals,
                )
                    .in_set(GameFlowSet::Diagnostics)
                    .run_if(in_state(AppState::InGame)),
            );

        app.add_systems(
            Update,
            draw_debug_gizmos
                .in_set(GameFlowSet::Diagnostics)
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            spawn_inspector_overlay
                .in_set(GameFlowSet::Diagnostics)
                .run_if(in_state(AppState::InGame))
                .run_if(any_with_component::<MainHudRoot>),
        )
        .add_systems(
            Update,
            update_inspector_overlay
                .in_set(GameFlowSet::Diagnostics)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Component)]
struct DebugInspectorOverlay;

#[derive(Component)]
struct DebugInspectorText;

#[derive(Component)]
struct FrustumDebugObserverCamera;

#[derive(Component)]
struct FrustumDebugLabel;
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

fn sync_time_of_day_tweaks(tweaks: &mut DebugTweaks, active: &ActiveSlider, cycle: &mut DayCycle) {
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
    if let Some(v) = tweaks.get_bool("Visuals/FoW Gameplay", "Reveal Full Map") {
        fog_settings.reveal_all = v;
    }
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

// ══════════════════════════════════════════════════════════════════════
// Entity Debug Tool
// ══════════════════════════════════════════════════════════════════════

const SPAWN_FOLDER: &str = "Entities/Spawn";
const SELECTED_FOLDER: &str = "Entities/Selected";
const RUNTIME_FOLDER: &str = "Game/Runtime";
const FLOW_FOLDER: &str = "Game/Flow";
const AI_FOLDER: &str = "Game/AI Settings";
const SAVE_FOLDER: &str = "Game/Save & Load";
const FRUSTUM_FOLDER: &str = "Game/Frustum Debug";
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
    tweaks.add_readonly(RUNTIME_FOLDER, "Culled Entities", "0");
    tweaks.add_readonly(
        RUNTIME_FOLDER,
        "Debug Hotkeys",
        "Ctrl+[ FPS | Ctrl+\\ Gizmos | Ctrl+] Inspector",
    );
    tweaks.add_bool(RUNTIME_FOLDER, "FPS Overlay", false);
    tweaks.add_bool(RUNTIME_FOLDER, "World Inspector", false);
    tweaks.add_bool(RUNTIME_FOLDER, "Selection Gizmos", true);
    // Frustum debug folder
    tweaks.add_bool(FRUSTUM_FOLDER, "Enabled", false);
    tweaks.add_bool(FRUSTUM_FOLDER, "Freeze Main Camera", true);
    tweaks.add_readonly(FRUSTUM_FOLDER, "Main Cam Pos", "--");
    tweaks.add_readonly(FRUSTUM_FOLDER, "Main Cam Angle", "--");
    tweaks.add_readonly(FRUSTUM_FOLDER, "Observer Pos", "--");
    tweaks.add_readonly(FRUSTUM_FOLDER, "Observer Speed", "--");
    tweaks.add_readonly(FRUSTUM_FOLDER, "Tracked Entities", "0");
    tweaks.add_readonly(FRUSTUM_FOLDER, "Visible", "0");
    tweaks.add_readonly(FRUSTUM_FOLDER, "Frustum Hidden", "0");
    tweaks.add_readonly(FRUSTUM_FOLDER, "Distance Hidden", "0");
    tweaks.add_readonly(FRUSTUM_FOLDER, "Fog Hidden", "0");
    tweaks.add_readonly(
        FRUSTUM_FOLDER,
        "Controls",
        "WASD=move | RMB=look | Scroll=speed | Space/Shift=up/down | F=focus",
    );

    tweaks.add_readonly(
        FLOW_FOLDER,
        "Pipeline",
        "Input > Net Rx > Sim > Net Tx > UI > Present",
    );
    tweaks.add_readonly(FLOW_FOLDER, "Bindings", "--");
    tweaks.add_readonly(FLOW_FOLDER, "Selected Units", "0");
    tweaks.add_readonly(FLOW_FOLDER, "Move Targets", "0");
    tweaks.add_readonly(FLOW_FOLDER, "Attack Targets", "0");
    tweaks.add_readonly(FLOW_FOLDER, "Queued Paths", "0");

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
    camera_q: &Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
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

fn viewport_ground_hit(
    camera: &Camera,
    cam_gt: &GlobalTransform,
    viewport_pos: Vec2,
) -> Option<Vec3> {
    let ray = camera.viewport_to_world(cam_gt, viewport_pos).ok()?;
    let dist = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
    Some(ray.get_point(dist))
}

fn camera_ground_corners(
    camera: &Camera,
    cam_gt: &GlobalTransform,
    window: &Window,
) -> Option<[Vec3; 4]> {
    let rect = camera.logical_viewport_rect().unwrap_or(Rect {
        min: Vec2::ZERO,
        max: Vec2::new(window.width(), window.height()),
    });
    let corners = [
        rect.min,
        Vec2::new(rect.max.x, rect.min.y),
        rect.max,
        Vec2::new(rect.min.x, rect.max.y),
    ];
    Some([
        viewport_ground_hit(camera, cam_gt, corners[0])?,
        viewport_ground_hit(camera, cam_gt, corners[1])?,
        viewport_ground_hit(camera, cam_gt, corners[2])?,
        viewport_ground_hit(camera, cam_gt, corners[3])?,
    ])
}

fn toggle_debug_views(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<DebugViewState>,
    time: Res<Time>,
    mut notifications: Option<ResMut<AllyNotifications>>,
    tweaks: Option<ResMut<DebugTweaks>>,
) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let mut changed = false;
    if ctrl && keys.just_pressed(KeyCode::BracketLeft) {
        state.fps_overlay = !state.fps_overlay;
        info!(
            "Debug FPS overlay {}",
            if state.fps_overlay {
                "enabled"
            } else {
                "disabled"
            }
        );
        if let Some(notifications) = notifications.as_mut() {
            notifications.push(
                AllyNotifyKind::UnderAttack,
                format!(
                    "FPS overlay {}",
                    if state.fps_overlay {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
                None,
                time.elapsed_secs(),
            );
        }
        changed = true;
    }
    if ctrl && keys.just_pressed(KeyCode::Backslash) {
        state.gizmos = !state.gizmos;
        info!(
            "Debug gizmos {}",
            if state.gizmos { "enabled" } else { "disabled" }
        );
        if let Some(notifications) = notifications.as_mut() {
            notifications.push(
                AllyNotifyKind::Attacking,
                format!(
                    "Debug gizmos {}",
                    if state.gizmos { "enabled" } else { "disabled" }
                ),
                None,
                time.elapsed_secs(),
            );
        }
        changed = true;
    }
    if ctrl && keys.just_pressed(KeyCode::BracketRight) {
        state.inspector = !state.inspector;
        info!(
            "World inspector {}",
            if state.inspector {
                "enabled"
            } else {
                "disabled"
            }
        );
        if let Some(notifications) = notifications.as_mut() {
            notifications.push(
                AllyNotifyKind::ReadyToAttack,
                format!(
                    "World inspector {}",
                    if state.inspector {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
                None,
                time.elapsed_secs(),
            );
        }
        changed = true;
    }

    if changed {
        if let Some(mut tweaks) = tweaks {
            tweaks.set_bool_if_changed(RUNTIME_FOLDER, "FPS Overlay", state.fps_overlay);
            tweaks.set_bool_if_changed(RUNTIME_FOLDER, "World Inspector", state.inspector);
            tweaks.set_bool_if_changed(RUNTIME_FOLDER, "Selection Gizmos", state.gizmos);
        }
    }
}

fn apply_debug_view_state(state: Res<DebugViewState>, mut fps_overlay: ResMut<FpsOverlayConfig>) {
    fps_overlay.enabled = state.fps_overlay;
    fps_overlay.frame_time_graph_config.enabled = state.fps_overlay;
}

fn spawn_inspector_overlay(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    root_q: Query<Entity, Added<MainHudRoot>>,
) {
    let Ok(hud_root) = root_q.single() else {
        return;
    };

    let panel = commands
        .spawn((
            DebugInspectorOverlay,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                top: Val::Px(12.0),
                width: Val::Px(320.0),
                max_height: Val::Px(420.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.05, 0.07, 0.94)),
            Visibility::Hidden,
        ))
        .insert(BorderColor::all(Color::srgba(0.35, 0.6, 0.95, 0.75)))
        .insert(GlobalZIndex(95))
        .insert(Pickable::IGNORE)
        .with_children(|parent| {
            parent.spawn((
                Text::new("Inspector"),
                TextFont {
                    font: fonts.heading.clone(),
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.92, 1.0)),
            ));
            parent.spawn((
                DebugInspectorText,
                Text::new(""),
                TextFont {
                    font: fonts.body.clone(),
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.86, 0.88, 0.9)),
            ));
        })
        .id();

    commands.entity(hud_root).add_child(panel);
}

fn update_inspector_overlay(
    state: Res<DebugViewState>,
    mut overlay_q: Query<&mut Visibility, With<DebugInspectorOverlay>>,
    mut text_q: Query<&mut Text, With<DebugInspectorText>>,
    entities: Query<Entity>,
    selected_q: Query<(Entity, &EntityKind, Option<&Health>), With<Selected>>,
    move_targets: Query<(), With<MoveTarget>>,
    attack_targets: Query<(), With<AttackTarget>>,
    path_queue: Option<Res<PathRequestQueue>>,
    role: Res<crate::multiplayer::NetRole>,
    lobby: Option<Res<crate::multiplayer::LobbyState>>,
    active_player: Option<Res<crate::components::ActivePlayer>>,
) {
    let Ok(mut visibility) = overlay_q.single_mut() else {
        return;
    };
    *visibility = if state.inspector {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };

    if !state.inspector {
        return;
    }

    let selected_count = selected_q.iter().count();
    let selected_summary = selected_q
        .iter()
        .take(6)
        .map(|(entity, kind, health)| {
            let hp = health
                .map(|hp| format!("{:.0}/{:.0}", hp.current, hp.max))
                .unwrap_or_else(|| "--".to_string());
            format!("#{:?} {} hp {}", entity, kind.display_name(), hp)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let lobby_status = lobby
        .as_ref()
        .map(|l| format!("{:?}", l.status))
        .unwrap_or_else(|| "--".to_string());
    let active_player = active_player
        .as_ref()
        .map(|p| format!("{:?}", p.0))
        .unwrap_or_else(|| "--".to_string());

    if let Ok(mut text) = text_q.single_mut() {
        **text = format!(
            "Ctrl+] toggles this panel\n\nEntities: {}\nSelected: {}\nMove targets: {}\nAttack targets: {}\nQueued paths: {}\nNet role: {:?}\nLobby: {}\nActive player: {}\n\n{}",
            entities.iter().count(),
            selected_count,
            move_targets.iter().count(),
            attack_targets.iter().count(),
            path_queue.as_ref().map(|q| q.requests.len()).unwrap_or_default(),
            *role,
            lobby_status,
            active_player,
            if selected_summary.is_empty() {
                "No selected entities".to_string()
            } else {
                format!("Selected details:\n{}", selected_summary)
            }
        );
    }
}

fn sync_debug_view_tweaks(tweaks: ResMut<DebugTweaks>, mut state: ResMut<DebugViewState>) {
    if let Some(enabled) = tweaks.get_bool(RUNTIME_FOLDER, "FPS Overlay") {
        state.fps_overlay = enabled;
    }
    if let Some(enabled) = tweaks.get_bool(RUNTIME_FOLDER, "World Inspector") {
        state.inspector = enabled;
    }
    if let Some(enabled) = tweaks.get_bool(RUNTIME_FOLDER, "Selection Gizmos") {
        state.gizmos = enabled;
    }
    if let Some(enabled) = tweaks.get_bool(FRUSTUM_FOLDER, "Enabled") {
        state.frustum_culling = enabled;
    }
}

fn sync_frustum_debug_camera(
    mut commands: Commands,
    state: Res<DebugViewState>,
    mut debug_mode: ResMut<FrustumDebugMode>,
    main_camera_q: Query<(Entity, &RtsCamera)>,
    mut main_camera_toggle: Query<
        &mut Camera,
        (With<RtsCamera>, Without<FrustumDebugObserverCamera>),
    >,
    observer_q: Query<Entity, With<FrustumDebugObserverCamera>>,
    label_q: Query<Entity, With<FrustumDebugLabel>>,
) {
    let was_enabled = debug_mode.enabled;
    debug_mode.enabled = state.frustum_culling;

    if state.frustum_culling {
        // Disable main camera rendering (observer takes over)
        if let Ok(mut main_cam) = main_camera_toggle.single_mut() {
            main_cam.is_active = false;
        }

        // Snapshot main camera state when first enabling
        if !was_enabled {
            if let Ok((_, rts_cam)) = main_camera_q.single() {
                debug_mode.frozen_pivot = rts_cam.pivot;
                debug_mode.frozen_distance = rts_cam.distance;
                debug_mode.frozen_angle = rts_cam.angle;
                debug_mode.frozen_pitch = rts_cam.pitch;
                // Start observer elevated and pulled back so the view is noticeably different
                debug_mode.observer_pos =
                    rts_cam.pivot + Vec3::new(0.0, rts_cam.distance * 1.8, rts_cam.distance * 0.8);
                debug_mode.observer_yaw = rts_cam.angle;
                debug_mode.observer_pitch = -0.75;
                debug_mode.freeze_main_camera = true;
            }
        }

        if observer_q.is_empty() {
            // Initial position: near the frozen main camera
            let init_pos = debug_mode.observer_pos;
            let look_dir = Vec3::new(
                -debug_mode.observer_yaw.sin() * debug_mode.observer_pitch.cos(),
                debug_mode.observer_pitch.sin(),
                -debug_mode.observer_yaw.cos() * debug_mode.observer_pitch.cos(),
            )
            .normalize_or_zero();
            let init_target = init_pos + look_dir * 10.0;

            commands.spawn((
                GameWorld,
                FrustumDebugObserverCamera,
                Camera3d::default(),
                Camera {
                    order: 2,
                    clear_color: bevy::camera::ClearColorConfig::Default,
                    ..default()
                },
                ClusterConfig::FixedZ {
                    total: 4096,
                    z_slices: 1,
                    z_config: ClusterZConfig::default(),
                    dynamic_resizing: true,
                },
                Transform::from_translation(init_pos).looking_at(init_target, Vec3::Y),
            ));
        }

        // Spawn overlay label
        if label_q.is_empty() {
            commands.spawn((
                GameWorld,
                FrustumDebugLabel,
                Text::new("FRUSTUM DEBUG — WASD fly, RMB look, Scroll speed, F focus"),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.85, 0.3)),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
                Node {
                    position_type: PositionType::Absolute,
                    top: Val::Px(8.0),
                    left: Val::Percent(25.0),
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                    ..default()
                },
            ));
        }
    } else {
        // Re-enable main camera rendering
        if let Ok(mut main_cam) = main_camera_toggle.single_mut() {
            main_cam.is_active = true;
        }
        for entity in &observer_q {
            commands.entity(entity).despawn();
        }
        for entity in &label_q {
            commands.entity(entity).despawn();
        }
    }
}

fn frustum_debug_fly_camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut scroll_events: bevy::ecs::message::MessageReader<bevy::input::mouse::MouseWheel>,
    mut motion_events: bevy::ecs::message::MessageReader<bevy::input::mouse::MouseMotion>,
    time: Res<Time>,
    mut observer_q: Query<&mut Transform, With<FrustumDebugObserverCamera>>,
    mut mode: ResMut<FrustumDebugMode>,
) {
    if !mode.enabled {
        // Drain events
        for _ in scroll_events.read() {}
        for _ in motion_events.read() {}
        return;
    }

    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    // Speed adjustment via scroll wheel
    for ev in scroll_events.read() {
        let scroll = match ev.unit {
            bevy::input::mouse::MouseScrollUnit::Line => ev.y,
            bevy::input::mouse::MouseScrollUnit::Pixel => ev.y / 16.0,
        };
        mode.observer_speed = (mode.observer_speed * (1.0 + scroll * 0.15)).clamp(5.0, 200.0);
    }

    // Mouse look (right mouse button held)
    if mouse.pressed(MouseButton::Right) {
        for ev in motion_events.read() {
            mode.observer_yaw -= ev.delta.x * 0.003;
            mode.observer_pitch = (mode.observer_pitch - ev.delta.y * 0.003).clamp(
                -std::f32::consts::FRAC_PI_2 + 0.05,
                std::f32::consts::FRAC_PI_2 - 0.05,
            );
        }
    } else {
        for _ in motion_events.read() {}
    }

    // WASD + Space/Shift movement
    let forward = Vec3::new(
        mode.observer_yaw.sin() * mode.observer_pitch.cos(),
        mode.observer_pitch.sin(),
        mode.observer_yaw.cos() * mode.observer_pitch.cos(),
    )
    .normalize_or_zero();
    let right = Vec3::new(mode.observer_yaw.cos(), 0.0, -mode.observer_yaw.sin());
    let up = Vec3::Y;

    let mut dir = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        dir -= forward;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        dir += forward;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        dir -= right;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        dir += right;
    }
    if keyboard.pressed(KeyCode::Space) {
        dir += up;
    }
    if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
        dir -= up;
    }

    if dir.length_squared() > 0.0 {
        let speed = mode.observer_speed;
        mode.observer_pos += dir.normalize() * speed * dt;
    }

    // F key: snap to main camera pivot
    if keyboard.just_pressed(KeyCode::KeyF) {
        mode.observer_pos = mode.frozen_pivot + Vec3::new(0.0, 80.0, 40.0);
        mode.observer_pitch = -0.6;
    }

    // Apply transform to observer camera
    let Ok(mut transform) = observer_q.single_mut() else {
        return;
    };
    transform.translation = mode.observer_pos;
    let look_dir = Vec3::new(
        -mode.observer_yaw.sin() * mode.observer_pitch.cos(),
        mode.observer_pitch.sin(),
        -mode.observer_yaw.cos() * mode.observer_pitch.cos(),
    )
    .normalize_or_zero();
    let target = mode.observer_pos + look_dir * 10.0;
    transform.look_at(target, Vec3::Y);
}

fn update_frustum_debug_camera(
    state: Res<DebugViewState>,
    mut observer_q: Query<&mut Camera, With<FrustumDebugObserverCamera>>,
) {
    // Observer renders full-screen (no viewport needed — main camera is disabled).
    // Just ensure it stays active while debug mode is on.
    if let Ok(mut cam) = observer_q.single_mut() {
        cam.is_active = state.frustum_culling;
    }
}

fn sync_frustum_debug_tweaks(
    mut tweaks: ResMut<DebugTweaks>,
    mut debug_mode: ResMut<FrustumDebugMode>,
    state: Res<DebugViewState>,
    main_camera_q: Query<&RtsCamera>,
    cullables: Query<
        Option<&CullReason>,
        Or<(
            With<crate::components::Unit>,
            With<crate::components::Mob>,
            With<crate::components::Building>,
            With<crate::components::ResourceNode>,
            With<crate::components::Decoration>,
            With<crate::components::Sapling>,
            With<crate::components::GrowingTree>,
            With<crate::components::GrowingResource>,
        )>,
    >,
) {
    if !state.frustum_culling {
        return;
    }

    // Sync freeze toggle from tweak panel
    if let Some(freeze) = tweaks.get_bool(FRUSTUM_FOLDER, "Freeze Main Camera") {
        debug_mode.freeze_main_camera = freeze;
    }

    // Main camera info
    if let Ok(rts_cam) = main_camera_q.single() {
        tweaks.set_readonly_if_changed(
            FRUSTUM_FOLDER,
            "Main Cam Pos",
            &format_debug_vec3(rts_cam.pivot),
        );
        tweaks.set_readonly_if_changed(
            FRUSTUM_FOLDER,
            "Main Cam Angle",
            &format!(
                "angle={:.1}° dist={:.1} pitch={:.1}°",
                rts_cam.angle.to_degrees(),
                rts_cam.distance,
                rts_cam.pitch.to_degrees()
            ),
        );
    }

    // Observer info
    tweaks.set_readonly_if_changed(
        FRUSTUM_FOLDER,
        "Observer Pos",
        &format_debug_vec3(debug_mode.observer_pos),
    );
    tweaks.set_readonly_if_changed(
        FRUSTUM_FOLDER,
        "Observer Speed",
        &format!("{:.0}", debug_mode.observer_speed),
    );

    // Cull reason counters
    let mut total = 0u32;
    let mut visible = 0u32;
    let mut frustum_hidden = 0u32;
    let mut distance_hidden = 0u32;
    let mut fog_hidden = 0u32;

    for reason in &cullables {
        total += 1;
        match reason {
            Some(CullReason::Visible) | None => visible += 1,
            Some(CullReason::Frustum) => frustum_hidden += 1,
            Some(CullReason::Distance) => distance_hidden += 1,
            Some(CullReason::Fog) => fog_hidden += 1,
        }
    }

    tweaks.set_readonly_if_changed(FRUSTUM_FOLDER, "Tracked Entities", &total.to_string());
    tweaks.set_readonly_if_changed(FRUSTUM_FOLDER, "Visible", &visible.to_string());
    tweaks.set_readonly_if_changed(
        FRUSTUM_FOLDER,
        "Frustum Hidden",
        &frustum_hidden.to_string(),
    );
    tweaks.set_readonly_if_changed(
        FRUSTUM_FOLDER,
        "Distance Hidden",
        &distance_hidden.to_string(),
    );
    tweaks.set_readonly_if_changed(FRUSTUM_FOLDER, "Fog Hidden", &fog_hidden.to_string());
}

fn sync_debug_flow_tweaks(
    mut tweaks: ResMut<DebugTweaks>,
    path_queue: Option<Res<PathRequestQueue>>,
    selected_units: Query<Entity, (With<Selected>, With<crate::components::Unit>)>,
    move_targets: Query<Entity, With<MoveTarget>>,
    attack_targets: Query<Entity, With<AttackTarget>>,
) {
    tweaks.set_readonly_if_changed(
        FLOW_FOLDER,
        "Bindings",
        "Minimap/Selection=Input | NetSet=Net Rx/Tx | Units/Buildings/Resources/AI/Combat/Path=Sim | UI=UiCore",
    );
    tweaks.set_readonly_if_changed(
        FLOW_FOLDER,
        "Selected Units",
        &selected_units.iter().count().to_string(),
    );
    tweaks.set_readonly_if_changed(
        FLOW_FOLDER,
        "Move Targets",
        &move_targets.iter().count().to_string(),
    );
    tweaks.set_readonly_if_changed(
        FLOW_FOLDER,
        "Attack Targets",
        &attack_targets.iter().count().to_string(),
    );
    tweaks.set_readonly_if_changed(
        FLOW_FOLDER,
        "Queued Paths",
        &path_queue
            .as_ref()
            .map(|queue| queue.requests.len())
            .unwrap_or_default()
            .to_string(),
    );
}

fn draw_debug_gizmos(
    state: Res<DebugViewState>,
    _debug_mode: Res<FrustumDebugMode>,
    mut gizmos: Gizmos,
    camera_q: Query<(&Camera, &GlobalTransform, &RtsCamera)>,
    cam_query: Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    selected: Query<
        (
            &GlobalTransform,
            Option<&MoveTarget>,
            Option<&NavPath>,
            Option<&AttackTarget>,
        ),
        With<Selected>,
    >,
    targets: Query<&GlobalTransform>,
    cullables: Query<
        (
            &GlobalTransform,
            Has<FrustumCulled>,
            Option<&CullReason>,
            Option<&CullingBounds>,
        ),
        Or<(
            With<crate::components::Unit>,
            With<crate::components::Mob>,
            With<crate::components::Building>,
            With<crate::components::ResourceNode>,
            With<crate::components::Decoration>,
            With<crate::components::Sapling>,
            With<crate::components::GrowingTree>,
            With<crate::components::GrowingResource>,
        )>,
    >,
) {
    if !state.gizmos && !state.frustum_culling {
        return;
    }

    if let Ok((main_camera, main_cam_gt, camera)) = camera_q.single() {
        let pivot = camera.pivot + Vec3::Y * 0.15;
        if state.gizmos {
            gizmos.circle(pivot, 1.2, Color::srgb(1.0, 0.45, 0.1));
            gizmos.line(
                pivot + Vec3::X * 1.4,
                pivot - Vec3::X * 1.4,
                Color::srgb(1.0, 0.45, 0.1),
            );
            gizmos.line(
                pivot + Vec3::Z * 1.4,
                pivot - Vec3::Z * 1.4,
                Color::srgb(1.0, 0.45, 0.1),
            );
        }

        if state.frustum_culling {
            let cam_pos = main_cam_gt.translation();
            // Main camera position sphere (orange)
            gizmos.sphere(cam_pos, 1.2, Color::srgb(1.0, 0.55, 0.2));

            if let Ok(window) = windows.single() {
                // 3D frustum wireframe: near plane corners projected to world
                if let Some(corners) = camera_ground_corners(main_camera, main_cam_gt, window) {
                    // Lines from camera to ground corners (frustum edges)
                    let frustum_color = Color::srgba(1.0, 0.65, 0.2, 0.7);
                    for corner in corners {
                        gizmos.line(cam_pos, corner + Vec3::Y * 0.2, frustum_color);
                    }
                    // Ground footprint quad (cyan)
                    let footprint_color = Color::srgb(0.15, 0.85, 1.0);
                    for edge in 0..corners.len() {
                        gizmos.line(
                            corners[edge] + Vec3::Y * 0.18,
                            corners[(edge + 1) % corners.len()] + Vec3::Y * 0.18,
                            footprint_color,
                        );
                    }

                    // Draw an elevated near-plane wireframe for 3D frustum visualization
                    let near_corners: Vec<Vec3> = corners
                        .iter()
                        .map(|c| {
                            let dir = (*c - cam_pos).normalize_or_zero();
                            cam_pos + dir * 15.0 + Vec3::Y * 0.1
                        })
                        .collect();
                    let near_color = Color::srgba(0.9, 0.9, 0.2, 0.6);
                    for i in 0..near_corners.len() {
                        gizmos.line(
                            near_corners[i],
                            near_corners[(i + 1) % near_corners.len()],
                            near_color,
                        );
                    }

                    // Draw a mid-plane wireframe
                    let mid_corners: Vec<Vec3> = corners
                        .iter()
                        .map(|c| {
                            let mid = cam_pos.lerp(*c, 0.5);
                            mid + Vec3::Y * 0.1
                        })
                        .collect();
                    let mid_color = Color::srgba(0.5, 0.7, 1.0, 0.3);
                    for i in 0..mid_corners.len() {
                        gizmos.line(
                            mid_corners[i],
                            mid_corners[(i + 1) % mid_corners.len()],
                            mid_color,
                        );
                    }
                }
            }

            // Color-coded entity markers by cull reason:
            // green = visible, red = frustum-culled, yellow = distance-hidden, blue = fog-hidden
            let color_visible = Color::srgba(0.2, 1.0, 0.45, 0.35);
            let color_frustum = Color::srgb(1.0, 0.2, 0.2);
            let color_distance = Color::srgb(1.0, 0.85, 0.15);
            let color_fog = Color::srgb(0.3, 0.5, 1.0);

            for (gtf, is_culled, cull_reason, bounds) in &cullables {
                let pos = gtf.translation() + Vec3::Y * 0.35;
                let reason = cull_reason.copied().unwrap_or(if is_culled {
                    CullReason::Frustum
                } else {
                    CullReason::Visible
                });

                match reason {
                    CullReason::Visible => {
                        gizmos.circle(pos, 0.35, color_visible);
                    }
                    CullReason::Frustum => {
                        gizmos.cross(pos, 1.4, color_frustum);
                    }
                    CullReason::Distance => {
                        gizmos.cross(pos, 1.0, color_distance);
                    }
                    CullReason::Fog => {
                        gizmos.cross(pos, 1.0, color_fog);
                    }
                }

                // Show bounding radius for entities with CullingBounds
                if let Some(b) = bounds {
                    let center = gtf.translation() + b.center_offset + Vec3::Y * 0.1;
                    gizmos.circle(center, b.radius, Color::srgba(0.8, 0.8, 0.2, 0.2));
                }
            }
        }
    }

    if state.gizmos {
        if let Some(cursor) = cursor_ground_pos(&cam_query, &windows) {
            let cursor = cursor + Vec3::Y * 0.08;
            gizmos.circle(cursor, 0.5, Color::srgb(0.25, 1.0, 0.4));
            gizmos.line(
                cursor + Vec3::X * 0.7,
                cursor - Vec3::X * 0.7,
                Color::srgb(0.25, 1.0, 0.4),
            );
            gizmos.line(
                cursor + Vec3::Z * 0.7,
                cursor - Vec3::Z * 0.7,
                Color::srgb(0.25, 1.0, 0.4),
            );
        }
    }

    if state.gizmos {
        for (transform, move_target, nav_path, attack_target) in &selected {
            let origin = transform.translation() + Vec3::Y * 0.15;
            gizmos.circle(origin, 0.9, Color::srgb(0.2, 0.95, 0.8));

            if let Some(move_target) = move_target {
                let destination = move_target.0 + Vec3::Y * 0.1;
                gizmos.line(origin, destination, Color::srgb(0.95, 0.85, 0.2));
                gizmos.cross(destination, 0.45, Color::srgb(0.95, 0.85, 0.2));
            }

            if let Some(nav_path) = nav_path {
                let mut previous = origin;
                for point in nav_path.waypoints.iter().skip(nav_path.current_index) {
                    let next = *point + Vec3::Y * 0.12;
                    gizmos.line(previous, next, Color::srgb(0.2, 0.75, 1.0));
                    previous = next;
                }
            }

            if let Some(attack_target) = attack_target {
                if let Ok(target) = targets.get(attack_target.0) {
                    gizmos.line(
                        origin,
                        target.translation() + Vec3::Y * 0.2,
                        Color::srgb(1.0, 0.25, 0.25),
                    );
                }
            }
        }
    }
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
    cam_query: Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
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
    cam_query: Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_press: Res<UiPressActive>,
    culled_q: Query<(), With<FrustumCulled>>,
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
    tweaks.set_readonly_if_changed(
        RUNTIME_FOLDER,
        "Culled Entities",
        &culled_q.iter().count().to_string(),
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
            stats
                .display_value(field.label, &role)
                .unwrap_or_else(|| "--".to_string())
        } else {
            "--".to_string()
        };
        tweaks.set_readonly_if_changed(folder, field.label, &display);
    }
}
