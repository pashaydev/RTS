use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::ecs::message::MessageReader;

use crate::components::{DragState, RtsCamera};

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
const DISTANCE_MAX: f32 = 50.0;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera).add_systems(
            Update,
            (
                camera_pan_input,
                camera_edge_scroll,
                camera_zoom_input,
                camera_rotate_input,
                camera_smooth_update,
            )
                .chain(),
        );
    }
}

fn spawn_camera(mut commands: Commands) {
    let pivot = Vec3::ZERO;
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
    ));
}

fn camera_pan_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<&mut RtsCamera>,
) {
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
    mut query: Query<&mut RtsCamera>,
) {
    if drag.dragging {
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
    time: Res<Time>,
    mut query: Query<&mut RtsCamera>,
) {
    let Ok(mut cam) = query.single_mut() else {
        return;
    };

    for ev in scroll_events.read() {
        let scroll = match ev.unit {
            MouseScrollUnit::Line => ev.y,
            MouseScrollUnit::Pixel => ev.y / 16.0,
        };
        cam.target_distance *= 1.0 - scroll * ZOOM_SENSITIVITY;
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

fn camera_smooth_update(
    time: Res<Time>,
    mut query: Query<(&mut RtsCamera, &mut Transform)>,
) {
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
    let offset = Vec3::new(
        cam.angle.sin() * h_dist,
        height,
        cam.angle.cos() * h_dist,
    );
    transform.translation = cam.pivot + offset;
    transform.look_at(cam.pivot, Vec3::Y);
}
