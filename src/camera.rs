use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

use crate::components::RtsCamera;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, (camera_pan, camera_zoom, camera_rotate));
    }
}

fn spawn_camera(mut commands: Commands) {
    let pivot = Vec3::ZERO;
    let distance = 60.0;
    let angle = 0.0_f32;

    let offset = Vec3::new(angle.sin() * distance, distance, angle.cos() * distance);

    commands.spawn((
        RtsCamera {
            pivot,
            distance,
            angle,
        },
        Camera3d::default(),
        Transform::from_translation(pivot + offset).looking_at(pivot, Vec3::Y),
    ));
}

fn camera_pan(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut query: Query<(&mut RtsCamera, &mut Transform)>,
) {
    let Ok((mut cam, mut transform)) = query.get_single_mut() else {
        return;
    };

    let speed = 40.0 * time.delta_secs();

    let forward = Vec3::new(-cam.angle.sin(), 0.0, -cam.angle.cos());
    let right = Vec3::new(cam.angle.cos(), 0.0, -cam.angle.sin());

    let mut movement = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        movement += forward;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        movement -= forward;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        movement -= right;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        movement += right;
    }

    if movement.length_squared() > 0.0 {
        movement = movement.normalize() * speed;
        cam.pivot += movement;
        update_camera_transform(&cam, &mut transform);
    }
}

fn camera_zoom(
    mut scroll_events: EventReader<MouseWheel>,
    mut query: Query<(&mut RtsCamera, &mut Transform)>,
) {
    let Ok((mut cam, mut transform)) = query.get_single_mut() else {
        return;
    };

    for ev in scroll_events.read() {
        cam.distance -= ev.y * 4.0;
        cam.distance = cam.distance.clamp(10.0, 200.0);
    }

    update_camera_transform(&cam, &mut transform);
}

fn camera_rotate(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut query: Query<(&mut RtsCamera, &mut Transform)>,
) {
    let Ok((mut cam, mut transform)) = query.get_single_mut() else {
        return;
    };

    let rotate_speed = 1.5 * time.delta_secs();
    let mut rotated = false;

    if keyboard.pressed(KeyCode::KeyQ) {
        cam.angle -= rotate_speed;
        rotated = true;
    }
    if keyboard.pressed(KeyCode::KeyE) {
        cam.angle += rotate_speed;
        rotated = true;
    }

    if rotated {
        update_camera_transform(&cam, &mut transform);
    }
}

fn update_camera_transform(cam: &RtsCamera, transform: &mut Transform) {
    let offset = Vec3::new(
        cam.angle.sin() * cam.distance,
        cam.distance,
        cam.angle.cos() * cam.distance,
    );
    transform.translation = cam.pivot + offset;
    transform.look_at(cam.pivot, Vec3::Y);
}
