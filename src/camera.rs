use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::RenderTarget;
use bevy::image::ImageSampler;
use bevy::ecs::message::MessageReader;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::light::cluster::{ClusterConfig, ClusterZConfig};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::post_process::{
    auto_exposure::AutoExposure,
    bloom::Bloom,
    dof::{DepthOfField, DepthOfFieldMode},
    effect_stack::ChromaticAberration,
};
use bevy::render::view::Hdr;
use bevy::window::PrimaryWindow;

use crate::components::{
    ActivePlayer, AntiAliasingMode, AppState, CameraZoomLevel, CullingSourceCamera, CursorOverUi,
    DragState, EffectQuality, FrustumDebugMode, GameFlowSet, GameSetupConfig, GameWorld,
    GraphicsSettings, MapSeed, RtsCamera, UiMode,
};

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
pub(crate) const PRESENTATION_LAYER: usize = 1;

#[derive(Resource, Clone)]
pub struct InternalRenderTarget {
    pub image: Handle<Image>,
    pub size: UVec2,
}

#[derive(Component)]
struct PresentationSprite;

#[derive(Component)]
struct PresentationCamera;

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
            .init_resource::<FrustumDebugMode>()
            .add_systems(
                OnEnter(AppState::InGame),
                setup_internal_render_target.before(spawn_camera),
            )
            .add_systems(
                OnEnter(AppState::InGame),
                spawn_camera
                    .after(crate::ground::spawn_ground)
                    .after(crate::multiplayer::configure_multiplayer_ai),
            )
            .add_systems(
                Update,
                (
                    update_presentation_display,
                    sync_camera_post_process_settings,
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
                    .in_set(GameFlowSet::Presentation)
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
    graphics: Res<GraphicsSettings>,
    map_seed: Res<MapSeed>,
    active_player: Res<ActivePlayer>,
    render_target: Res<InternalRenderTarget>,
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

    let mut entity = commands.spawn((
        GameWorld,
        CullingSourceCamera,
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
        Camera {
            order: -1,
            ..default()
        },
        RenderTarget::Image(render_target.image.clone().into()),
        Hdr,
        bevy::core_pipeline::tonemapping::Tonemapping::TonyMcMapface,
        bevy::core_pipeline::tonemapping::DebandDither::Enabled,
        brightness_exposure(graphics.brightness),
        Transform::from_translation(pivot + offset).looking_at(pivot, Vec3::Y),
        // Top-down RTS camera: flatten Z-slices to 1 since all action is on a single plane
        ClusterConfig::FixedZ {
            total: 4096,
            z_slices: 1,
            z_config: ClusterZConfig::default(),
            dynamic_resizing: true,
        },
        Msaa::Off,
    ));

    #[cfg(not(target_arch = "wasm32"))]
    if graphics.anti_aliasing == AntiAliasingMode::Smaa {
        entity.insert(bevy::anti_alias::smaa::Smaa::default());
    }

    insert_post_process_effects(entity.id(), &mut commands, &graphics);
}

fn create_internal_render_image(images: &mut Assets<Image>, size: UVec2) -> Handle<Image> {
    let extent = Extent3d {
        width: size.x.max(1),
        height: size.y.max(1),
        depth_or_array_layers: 1,
    };
    let mut image = Image::new_fill(
        extent,
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
        | TextureUsages::COPY_DST
        | TextureUsages::RENDER_ATTACHMENT;
    image.sampler = ImageSampler::linear();
    images.add(image)
}

fn setup_internal_render_target(
    mut commands: Commands,
    graphics: Res<GraphicsSettings>,
    mut images: ResMut<Assets<Image>>,
) {
    let size = internal_render_size(&graphics);
    let image = create_internal_render_image(&mut images, size);

    commands.insert_resource(InternalRenderTarget {
        image: image.clone(),
        size,
    });

    commands.spawn((
        GameWorld,
        PresentationSprite,
        Sprite::from_image(image),
        RenderLayers::layer(PRESENTATION_LAYER),
        Transform::default(),
    ));

    commands.spawn((
        GameWorld,
        PresentationCamera,
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        Msaa::Off,
        RenderLayers::layer(PRESENTATION_LAYER),
    ));
}

fn update_presentation_display(
    graphics: Res<GraphicsSettings>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    mut render_target: ResMut<InternalRenderTarget>,
    mut sprites: Query<(&mut Sprite, &mut Transform), With<PresentationSprite>>,
    mut camera_targets: Query<&mut RenderTarget, (With<RtsCamera>, With<Camera3d>)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let desired_size = internal_render_size(&graphics);
    if render_target.size != desired_size {
        let new_image = create_internal_render_image(&mut images, desired_size);
        render_target.image = new_image.clone();
        render_target.size = desired_size;

        for (mut sprite, _) in &mut sprites {
            sprite.image = new_image.clone();
        }
        for mut target in &mut camera_targets {
            *target = RenderTarget::Image(new_image.clone().into());
        }
    }

    let rect = presentation_rect(window, &graphics);
    let window_size = Vec2::new(window.width().max(1.0), window.height().max(1.0));
    let center = rect.center();
    let world_x = center.x - window_size.x * 0.5;
    let world_y = window_size.y * 0.5 - center.y;

    for (mut sprite, mut transform) in &mut sprites {
        sprite.custom_size = Some(rect.size());
        transform.translation = Vec3::new(world_x, world_y, 0.0);
    }
}

pub(crate) fn internal_render_size(graphics: &GraphicsSettings) -> UVec2 {
    UVec2::new(graphics.resolution.0.max(1), graphics.resolution.1.max(1))
}

pub(crate) fn presentation_rect(window: &Window, graphics: &GraphicsSettings) -> Rect {
    let target = internal_render_size(graphics).as_vec2();
    let window_size = Vec2::new(window.width().max(1.0), window.height().max(1.0));
    let scale = (window_size.x / target.x).min(window_size.y / target.y);
    let size = target * scale.max(0.0001);
    let min = (window_size - size) * 0.5;
    Rect {
        min,
        max: min + size,
    }
}

pub(crate) fn window_to_render_position(
    window: &Window,
    graphics: &GraphicsSettings,
    window_position: Vec2,
) -> Option<Vec2> {
    let rect = presentation_rect(window, graphics);
    if window_position.x < rect.min.x
        || window_position.x > rect.max.x
        || window_position.y < rect.min.y
        || window_position.y > rect.max.y
    {
        return None;
    }

    let uv = (window_position - rect.min) / rect.size();
    Some(uv * internal_render_size(graphics).as_vec2())
}

pub(crate) fn render_to_window_position(
    window: &Window,
    graphics: &GraphicsSettings,
    render_position: Vec2,
) -> Vec2 {
    let rect = presentation_rect(window, graphics);
    let uv = render_position / internal_render_size(graphics).as_vec2();
    rect.min + uv * rect.size()
}

pub(crate) fn viewport_ray_from_window_cursor(
    camera: &Camera,
    cam_gt: &GlobalTransform,
    window: &Window,
    graphics: &GraphicsSettings,
) -> Option<Ray3d> {
    let cursor = window.cursor_position()?;
    let render_cursor = window_to_render_position(window, graphics, cursor)?;
    camera.viewport_to_world(cam_gt, render_cursor).ok()
}

pub(crate) fn world_to_window_viewport(
    camera: &Camera,
    cam_gt: &GlobalTransform,
    world_pos: Vec3,
    window: &Window,
    graphics: &GraphicsSettings,
) -> Option<Vec2> {
    let render_pos = camera.world_to_viewport(cam_gt, world_pos).ok()?;
    Some(render_to_window_position(window, graphics, render_pos))
}

fn brightness_exposure(brightness: f32) -> bevy::camera::Exposure {
    bevy::camera::Exposure {
        ev100: bevy::camera::Exposure::EV100_BLENDER - brightness.max(0.01).log2(),
    }
}

fn bloom_for_quality(quality: EffectQuality) -> Option<Bloom> {
    match quality {
        EffectQuality::Off => None,
        EffectQuality::Low => Some(Bloom {
            intensity: 0.08,
            ..Bloom::NATURAL
        }),
        EffectQuality::Medium => Some(Bloom::NATURAL),
        EffectQuality::High => Some(Bloom {
            intensity: 0.25,
            low_frequency_boost: 0.8,
            ..Bloom::NATURAL
        }),
    }
}

fn depth_of_field_for_quality(quality: EffectQuality) -> Option<DepthOfField> {
    match quality {
        EffectQuality::Off => None,
        EffectQuality::Low => Some(DepthOfField {
            mode: DepthOfFieldMode::Gaussian,
            focal_distance: 60.0,
            sensor_height: 0.01866,
            aperture_f_stops: 4.0,
            max_circle_of_confusion_diameter: 4.0,
            max_depth: 200.0,
        }),
        EffectQuality::Medium => Some(DepthOfField {
            mode: DepthOfFieldMode::Gaussian,
            focal_distance: 60.0,
            sensor_height: 0.01866,
            aperture_f_stops: 2.8,
            max_circle_of_confusion_diameter: 6.0,
            max_depth: 220.0,
        }),
        EffectQuality::High => Some(DepthOfField {
            mode: DepthOfFieldMode::Gaussian,
            focal_distance: 60.0,
            sensor_height: 0.01866,
            aperture_f_stops: 2.0,
            max_circle_of_confusion_diameter: 8.0,
            max_depth: 240.0,
        }),
    }
}

fn chromatic_aberration_for_quality(quality: EffectQuality) -> Option<ChromaticAberration> {
    match quality {
        EffectQuality::Off => None,
        EffectQuality::Low => Some(ChromaticAberration {
            intensity: 0.0075,
            max_samples: 4,
            ..default()
        }),
        EffectQuality::Medium => Some(ChromaticAberration::default()),
        EffectQuality::High => Some(ChromaticAberration {
            intensity: 0.035,
            max_samples: 12,
            ..default()
        }),
    }
}

fn insert_post_process_effects(
    entity: Entity,
    commands: &mut Commands,
    graphics: &GraphicsSettings,
) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(bloom) = bloom_for_quality(graphics.bloom) {
            commands.entity(entity).insert(bloom);
        }
        if graphics.auto_exposure {
            commands.entity(entity).insert(AutoExposure::default());
        }
    }

    if let Some(depth_of_field) = depth_of_field_for_quality(graphics.depth_of_field) {
        commands.entity(entity).insert(depth_of_field);
    }
    if let Some(chromatic_aberration) =
        chromatic_aberration_for_quality(graphics.chromatic_aberration)
    {
        commands.entity(entity).insert(chromatic_aberration);
    }
}

fn sync_camera_post_process_settings(
    graphics: Res<GraphicsSettings>,
    camera_q: Query<Entity, (With<RtsCamera>, With<Camera3d>)>,
    mut commands: Commands,
    mut applied: Local<Option<(Entity, GraphicsSettings)>>,
) {
    let Ok(entity) = camera_q.single() else {
        *applied = None;
        return;
    };

    if applied
        .as_ref()
        .is_some_and(|(last_entity, last_settings)| *last_entity == entity && *last_settings == *graphics)
    {
        return;
    }

    *applied = Some((entity, graphics.clone()));

    commands.entity(entity).insert((
        Hdr,
        bevy::core_pipeline::tonemapping::Tonemapping::TonyMcMapface,
        bevy::core_pipeline::tonemapping::DebandDither::Enabled,
        brightness_exposure(graphics.brightness),
    ));

    #[cfg(not(target_arch = "wasm32"))]
    match graphics.anti_aliasing {
        AntiAliasingMode::Off => {
            commands.entity(entity).remove::<bevy::anti_alias::smaa::Smaa>();
        }
        AntiAliasingMode::Smaa => {
            commands
                .entity(entity)
                .insert(bevy::anti_alias::smaa::Smaa::default());
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    match bloom_for_quality(graphics.bloom) {
        Some(bloom) => {
            commands.entity(entity).insert(bloom);
        }
        None => {
            commands.entity(entity).remove::<Bloom>();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    if graphics.auto_exposure {
        commands.entity(entity).insert(AutoExposure::default());
    } else {
        commands.entity(entity).remove::<AutoExposure>();
    }

    match depth_of_field_for_quality(graphics.depth_of_field) {
        Some(depth_of_field) => {
            commands.entity(entity).insert(depth_of_field);
        }
        None => {
            commands.entity(entity).remove::<DepthOfField>();
        }
    }

    match chromatic_aberration_for_quality(graphics.chromatic_aberration) {
        Some(chromatic_aberration) => {
            commands.entity(entity).insert(chromatic_aberration);
        }
        None => {
            commands.entity(entity).remove::<ChromaticAberration>();
        }
    }
}

fn camera_pan_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut query: Query<&mut RtsCamera>,
    debug_mode: Res<FrustumDebugMode>,
) {
    if debug_mode.enabled && debug_mode.freeze_main_camera {
        return;
    }
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
    debug_mode: Res<FrustumDebugMode>,
) {
    if debug_mode.enabled && debug_mode.freeze_main_camera {
        return;
    }
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
    windows: Query<&Window, With<PrimaryWindow>>,
    widget_q: Query<
        (&ComputedNode, &UiGlobalTransform),
        With<crate::ui::core::framework::Widget>,
    >,
    time: Res<Time>,
    mut query: Query<&mut RtsCamera>,
    debug_mode: Res<FrustumDebugMode>,
) {
    let Ok(mut cam) = query.single_mut() else {
        return;
    };
    if debug_mode.enabled && debug_mode.freeze_main_camera {
        // Don't drain scroll events — the fly camera system needs them
        return;
    }

    let cursor_in_widget = windows
        .single()
        .ok()
        .and_then(|window| window.physical_cursor_position())
        .is_some_and(|cursor_phys| {
            widget_q
                .iter()
                .any(|(computed, ui_tf)| computed.contains_point(*ui_tf, cursor_phys))
        });

    for ev in scroll_events.read() {
        let scroll = match ev.unit {
            MouseScrollUnit::Line => ev.y,
            MouseScrollUnit::Pixel => ev.y / 16.0,
        };
        if !cursor_over_ui.0 && !cursor_in_widget {
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
    debug_mode: Res<FrustumDebugMode>,
) {
    if debug_mode.enabled && debug_mode.freeze_main_camera {
        return;
    }
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

fn track_last_selection(ui_mode: Res<UiMode>, mut last: ResMut<LastSelection>) {
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
    debug_mode: Res<FrustumDebugMode>,
) {
    if debug_mode.enabled && debug_mode.freeze_main_camera {
        return;
    }
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

fn update_zoom_level(camera_q: Query<&RtsCamera>, mut zoom_level: ResMut<CameraZoomLevel>) {
    if let Ok(cam) = camera_q.single() {
        let new = CameraZoomLevel::from_distance(cam.distance);
        // Only write when detail level changes or distance drifts significantly,
        // so that Res::is_changed() gating in downstream systems works correctly.
        if new.detail != zoom_level.detail || (new.distance - zoom_level.distance).abs() > 0.5 {
            *zoom_level = new;
        }
    }
}
