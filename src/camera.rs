use bevy::ecs::message::MessageReader;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::light::cluster::{ClusterConfig, ClusterZConfig};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::components::{ActivePlayer, AppState, CameraZoomLevel, CursorOverUi, DragState, GameSetupConfig, GameWorld, MapSeed, RtsCamera, UiMode};

// ── Tuning constants ──

const PAN_SMOOTH: f32 = 10.0;
const ZOOM_SMOOTH: f32 = 6.0;
const ROTATE_SMOOTH: f32 = 8.0;
const FRICTION: f32 = 8.0;
const PAN_SPEED_SCALE: f32 = 0.1;
const ZOOM_SENSITIVITY: f32 = 0.07;
const EDGE_THRESHOLD: f32 = 0.06;
const PITCH_MIN: f32 = 0.4;
const PITCH_MAX: f32 = 1.1;
const DISTANCE_MIN: f32 = 10.0;
const DISTANCE_MAX: f32 = 100.0;

#[derive(Resource, Default)]
pub struct LastSelection {
    pub entities: Vec<Entity>,
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CursorOverUi>()
            .init_resource::<LastSelection>()
            .init_resource::<CameraZoomLevel>()
            .add_systems(
                OnEnter(AppState::InGame),
                spawn_camera
                    .after(crate::ground::spawn_ground)
                    .after(crate::multiplayer::configure_multiplayer_ai),
            )
            .add_systems(
                Update,
                (
                    update_cursor_over_ui,
                    track_last_selection,
                    camera_focus_selection,
                    camera_pan_input,
                    camera_edge_scroll,
                    camera_zoom_input,
                    camera_rotate_input,
                    camera_smooth_update,
                    update_zoom_level,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn update_cursor_over_ui(
    interactions: Query<&Interaction, With<Node>>,
    mut cursor_over_ui: ResMut<CursorOverUi>,
) {
    cursor_over_ui.0 = interactions
        .iter()
        .any(|i| *i == Interaction::Hovered || *i == Interaction::Pressed);
}

fn spawn_camera(
    mut commands: Commands,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
    active_player: Res<ActivePlayer>,
) {
    // Start camera at the active player's spawn position
    let positions = config.spawn_positions(map_seed.0);
    let (sx, sz) = positions
        .iter()
        .find(|(f, _)| *f == active_player.0)
        .map(|(_, pos)| *pos)
        .unwrap_or(positions[0].1);
    let pivot = Vec3::new(sx, 0.0, sz);
    let distance = 60.0_f32;
    let angle = 0.0_f32;

    // Compute initial pitch from distance
    let t = ((distance - DISTANCE_MIN) / (DISTANCE_MAX - DISTANCE_MIN)).clamp(0.0, 1.0);
    let t_smooth = t * t * (3.0 - 2.0 * t);
    let pitch = PITCH_MIN + (PITCH_MAX - PITCH_MIN) * t_smooth;

    let h_dist = distance * pitch.cos();
    let height = distance * pitch.sin();
    let offset = Vec3::new(angle.sin() * h_dist, height, angle.cos() * h_dist);

    commands.spawn((
        GameWorld,
        RtsCamera {
            pivot,
            distance,
            angle,
            pitch,
            target_pivot: pivot,
            target_distance: distance,
            target_angle: angle,
            pan_velocity: Vec3::ZERO,
        },
        Camera3d::default(),
        Transform::from_translation(pivot + offset).looking_at(pivot, Vec3::Y),
        // Top-down RTS camera: flatten Z-slices to 1 since all action is on a single plane
        ClusterConfig::FixedZ {
            total: 4096,
            z_slices: 1,
            z_config: ClusterZConfig::default(),
            dynamic_resizing: true,
        },
        Msaa::Off,
        #[cfg(not(target_arch = "wasm32"))]
        bevy::anti_alias::smaa::Smaa::default(),
    ));
}

fn camera_pan_input(keyboard: Res<ButtonInput<KeyCode>>, mut query: Query<&mut RtsCamera>) {
    let Ok(mut cam) = query.single_mut() else {
        return;
    };

    let speed = PAN_SPEED_SCALE * cam.distance;

    let forward = Vec3::new(-cam.angle.sin(), 0.0, -cam.angle.cos());
    let right = Vec3::new(cam.angle.cos(), 0.0, -cam.angle.sin());

    let mut dir = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        dir += forward;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        dir -= forward;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        dir -= right;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        dir += right;
    }

    if dir.length_squared() > 0.0 {
        cam.pan_velocity += dir.normalize() * speed;
    }
}

fn camera_edge_scroll(
    windows: Query<&Window, With<PrimaryWindow>>,
    drag: Res<DragState>,
    cursor_over_ui: Res<CursorOverUi>,
    mut query: Query<&mut RtsCamera>,
) {
    if drag.dragging || cursor_over_ui.0 {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };

    let Some(cursor) = window.cursor_position() else {
        return;
    };

    if !window.focused {
        return;
    }

    let w = window.width();
    let h = window.height();
    let ex = EDGE_THRESHOLD * w;
    let ey = EDGE_THRESHOLD * h;

    let mut dir = Vec3::ZERO;

    let Ok(mut cam) = query.single_mut() else {
        return;
    };

    let forward = Vec3::new(-cam.angle.sin(), 0.0, -cam.angle.cos());
    let right = Vec3::new(cam.angle.cos(), 0.0, -cam.angle.sin());

    if cursor.x < ex {
        dir -= right;
    }
    if cursor.x > w - ex {
        dir += right;
    }
    if cursor.y < ey {
        dir += forward;
    }
    if cursor.y > h - ey {
        dir -= forward;
    }

    if dir.length_squared() > 0.0 {
        let speed = PAN_SPEED_SCALE * cam.distance / 3.0;
        cam.pan_velocity += dir.normalize() * speed;
    }
}

fn camera_zoom_input(
    mut scroll_events: MessageReader<MouseWheel>,
    keyboard: Res<ButtonInput<KeyCode>>,
    cursor_over_ui: Res<CursorOverUi>,
    time: Res<Time>,
    mut query: Query<&mut RtsCamera>,
) {
    let Ok(mut cam) = query.single_mut() else {
        return;
    };

    if !cursor_over_ui.0 {
        for ev in scroll_events.read() {
            let scroll = match ev.unit {
                MouseScrollUnit::Line => ev.y,
                MouseScrollUnit::Pixel => ev.y / 16.0,
            };
            cam.target_distance *= 1.0 - scroll * ZOOM_SENSITIVITY;
        }
    }

    let key_zoom_speed = 2.0 * time.delta_secs();
    if keyboard.pressed(KeyCode::Equal) || keyboard.pressed(KeyCode::NumpadAdd) {
        cam.target_distance *= 1.0 - key_zoom_speed;
    }
    if keyboard.pressed(KeyCode::Minus) || keyboard.pressed(KeyCode::NumpadSubtract) {
        cam.target_distance *= 1.0 + key_zoom_speed;
    }

    cam.target_distance = cam.target_distance.clamp(DISTANCE_MIN, DISTANCE_MAX);
}

fn camera_rotate_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut query: Query<&mut RtsCamera>,
) {
    let Ok(mut cam) = query.single_mut() else {
        return;
    };

    let rotate_speed = 1.5 * time.delta_secs();

    if keyboard.pressed(KeyCode::KeyQ) {
        cam.target_angle -= rotate_speed;
    }
    if keyboard.pressed(KeyCode::KeyE) {
        cam.target_angle += rotate_speed;
    }
}

fn camera_smooth_update(time: Res<Time>, mut query: Query<(&mut RtsCamera, &mut Transform)>) {
    let Ok((mut cam, mut transform)) = query.single_mut() else {
        return;
    };

    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    // a) Momentum decay (frame-rate independent)
    cam.pan_velocity *= (-FRICTION * dt).exp();
    let vel = cam.pan_velocity * dt;
    cam.target_pivot += vel;

    // b) Exponential smoothing toward targets
    let alpha_pan = 1.0 - (-PAN_SMOOTH * dt).exp();
    let alpha_zoom = 1.0 - (-ZOOM_SMOOTH * dt).exp();
    let alpha_rotate = 1.0 - (-ROTATE_SMOOTH * dt).exp();

    cam.pivot = cam.pivot.lerp(cam.target_pivot, alpha_pan);
    cam.distance = cam.distance + (cam.target_distance - cam.distance) * alpha_zoom;

    // c) Angle wrapping fix — take the short path
    let mut angle_diff = cam.target_angle - cam.angle;
    if angle_diff > std::f32::consts::PI {
        cam.target_angle -= std::f32::consts::TAU;
    } else if angle_diff < -std::f32::consts::PI {
        cam.target_angle += std::f32::consts::TAU;
    }
    angle_diff = cam.target_angle - cam.angle;
    cam.angle += angle_diff * alpha_rotate;

    // d) Zoom-dependent pitch
    let t = ((cam.distance - DISTANCE_MIN) / (DISTANCE_MAX - DISTANCE_MIN)).clamp(0.0, 1.0);
    let t_smooth = t * t * (3.0 - 2.0 * t); // smoothstep
    cam.pitch = PITCH_MIN + (PITCH_MAX - PITCH_MIN) * t_smooth;

    // e) Write Transform using pitch
    let h_dist = cam.distance * cam.pitch.cos();
    let height = cam.distance * cam.pitch.sin();
    let offset = Vec3::new(cam.angle.sin() * h_dist, height, cam.angle.cos() * h_dist);
    transform.translation = cam.pivot + offset;
    transform.look_at(cam.pivot, Vec3::Y);
}

fn track_last_selection(
    ui_mode: Res<UiMode>,
    mut last: ResMut<LastSelection>,
) {
    if let UiMode::SelectedUnits(ref entities) = *ui_mode {
        if !entities.is_empty() {
            last.entities = entities.clone();
        }
    } else if let UiMode::SelectedBuilding(entity) = *ui_mode {
        last.entities = vec![entity];
    }
}

fn camera_focus_selection(
    keyboard: Res<ButtonInput<KeyCode>>,
    ui_mode: Res<UiMode>,
    last: Res<LastSelection>,
    transforms: Query<&GlobalTransform>,
    mut cam_query: Query<&mut RtsCamera>,
) {
    if !keyboard.pressed(KeyCode::Space) {
        return;
    }

    let Ok(mut cam) = cam_query.single_mut() else {
        return;
    };

    let entities: &[Entity] = match &*ui_mode {
        UiMode::SelectedUnits(ref e) if !e.is_empty() => e,
        UiMode::SelectedBuilding(e) => std::slice::from_ref(e),
        _ => &last.entities,
    };

    if entities.is_empty() {
        return;
    }

    let mut center = Vec3::ZERO;
    let mut count = 0u32;
    for &entity in entities {
        if let Ok(gt) = transforms.get(entity) {
            center += gt.translation();
            count += 1;
        }
    }

    if count > 0 {
        center /= count as f32;
        center.y = 0.0;
        cam.target_pivot = center;
        cam.pan_velocity = Vec3::ZERO;
    }
}

fn update_zoom_level(
    camera_q: Query<&RtsCamera>,
    mut zoom_level: ResMut<CameraZoomLevel>,
) {
    if let Ok(cam) = camera_q.single() {
        *zoom_level = CameraZoomLevel::from_distance(cam.distance);
    }
}
