mod config;
mod model;
mod state;
mod ui;

#[cfg(not(target_arch = "wasm32"))]
use bevy::dev_tools::fps_overlay::{FpsOverlayConfig, FpsOverlayPlugin};
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::light::cluster::{ClusterConfig, ClusterZConfig};
use bevy::prelude::*;

use crate::blueprints::{spawn_from_blueprint, BlueprintRegistry, EntityKind, EntityVisualCache};
use crate::components::{
    AiControlledFactions, AiFactionSettings, AllPlayerResources, AllyNotifications, AllyNotifyKind,
    AppState, AttackTarget, CullReason, Faction, FrustumCulled, FrustumDebugMode,
    GameFlowSet, GameSetupConfig, GameWorld, Health, MoveTarget, ResourceType, RtsCamera, Selected,
    UiPressActive, UnitSpeed,
};
use crate::fog::FogTweakSettings;
use crate::ground::HeightMap;
use crate::lighting::{
    DayCycle, EntityClusterLight, EntityLightConfig, EntityLightGrid, LightingOverrides, SunLight,
};
use crate::model_assets::{BuildingModelAssets, UnitModelAssets};
use crate::pathfinding::PathRequestQueue;
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
        app.add_plugins(FrameTimeDiagnosticsPlugin::default());

        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut fps_overlay_config = FpsOverlayConfig::default();
            fps_overlay_config.enabled = false;
            fps_overlay_config.frame_time_graph_config.enabled = false;
            app.add_plugins(FpsOverlayPlugin {
                config: fps_overlay_config,
            });
        }

        app
        .init_resource::<DebugViewState>()
        .add_systems(Update, toggle_debug_views);

        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(
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
                    sync_resource_debug_tweaks,
                    sync_ai_debug_tweaks,
                    sync_network_debug_tweaks,
                    initialize_debug_folder_defaults,
                    rebuild_tweak_panel,
                    update_tweak_visuals,
                )
                    .in_set(GameFlowSet::Diagnostics)
                    .run_if(in_state(AppState::InGame)),
            );


    }
}

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

    // ── FoW Performance folder ──
    if let Some(v) = tweaks.get_bool("Visuals/FoW Performance", "Visibility Update") {
        fog_settings.enable_visibility_update = v;
    }
    if let Some(v) = tweaks.get_bool("Visuals/FoW Performance", "Display Lerp") {
        fog_settings.enable_display_lerp = v;
    }
    if let Some(v) = tweaks.get_bool("Visuals/FoW Performance", "Texture Upload") {
        fog_settings.enable_texture_upload = v;
    }
    if let Some(v) = tweaks.get_bool("Visuals/FoW Performance", "Entity Hiding") {
        fog_settings.enable_entity_hiding = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Performance", "Tick Rate Hz") {
        fog_settings.tick_rate_hz = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Performance", "Shader Quality") {
        fog_settings.shader_quality = v;
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
const RESOURCES_FOLDER: &str = "Game/Resources";
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
        "Ctrl+[ FPS | Ctrl+\\] Inspector",
    );
    tweaks.add_bool(RUNTIME_FOLDER, "FPS Overlay", false);
    tweaks.add_bool(RUNTIME_FOLDER, "World Inspector", false);
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

    // Resources folder
    tweaks.add_cycle_enum(
        RESOURCES_FOLDER,
        "Faction",
        vec![
            "Player 1".to_string(),
            "Player 2".to_string(),
            "Player 3".to_string(),
            "Player 4".to_string(),
        ],
        0,
    );
    tweaks.add_float(RESOURCES_FOLDER, "Amount", 500.0, 50.0, 5000.0, 50.0);
    for rt in ResourceType::ALL.iter() {
        tweaks.add_button(RESOURCES_FOLDER, &format!("Add {}", rt.display_name()));
    }
    tweaks.add_button(RESOURCES_FOLDER, "Add All Resources");
    tweaks.add_readonly(RESOURCES_FOLDER, "Status", "Ready");

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
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn apply_debug_view_state(state: Res<DebugViewState>, mut fps_overlay: ResMut<FpsOverlayConfig>) {
    fps_overlay.enabled = state.fps_overlay;
    fps_overlay.frame_time_graph_config.enabled = state.fps_overlay;
}

fn sync_debug_view_tweaks(tweaks: ResMut<DebugTweaks>, mut state: ResMut<DebugViewState>) {
    if let Some(enabled) = tweaks.get_bool(RUNTIME_FOLDER, "FPS Overlay") {
        state.fps_overlay = enabled;
    }
    if let Some(enabled) = tweaks.get_bool(RUNTIME_FOLDER, "World Inspector") {
        state.inspector = enabled;
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
                    #[cfg(not(target_arch = "wasm32"))]
                    total: 4096,
                    #[cfg(target_arch = "wasm32")]
                    total: 256,
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
            commands.entity(entity).try_despawn();
        }
        for entity in &label_q {
            commands.entity(entity).try_despawn();
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
            With<crate::components::DecoRevealed>,
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

fn sync_resource_debug_tweaks(
    mut tweaks: ResMut<DebugTweaks>,
    pressed: Res<DebugButtonPressed>,
    mut all_resources: ResMut<AllPlayerResources>,
) {
    let faction_idx = tweaks
        .get_cycle_selected(RESOURCES_FOLDER, "Faction")
        .unwrap_or(0);
    let faction = match faction_idx {
        0 => Faction::Player1,
        1 => Faction::Player2,
        2 => Faction::Player3,
        _ => Faction::Player4,
    };
    let amount = tweaks
        .get_float(RESOURCES_FOLDER, "Amount")
        .unwrap_or(500.0) as u32;

    for (folder, label) in &pressed.pressed {
        if folder != RESOURCES_FOLDER {
            continue;
        }
        if label == "Add All Resources" {
            let res = all_resources.get_mut(&faction);
            for rt in ResourceType::ALL.iter() {
                res.add(*rt, amount);
            }
            tweaks.set_readonly_if_changed(
                RESOURCES_FOLDER,
                "Status",
                &format!("+{amount} all to {faction:?}"),
            );
        } else if let Some(rt_name) = label.strip_prefix("Add ") {
            if let Some(rt) = ResourceType::ALL
                .iter()
                .find(|rt| rt.display_name() == rt_name)
            {
                all_resources.get_mut(&faction).add(*rt, amount);
                tweaks.set_readonly_if_changed(
                    RESOURCES_FOLDER,
                    "Status",
                    &format!("+{amount} {rt_name} to {faction:?}"),
                );
            }
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
                    commands.entity(entity).try_despawn();
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
