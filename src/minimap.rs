use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::widget::NodeImageMode;
use bevy::ui::RelativeCursorPosition;
use bevy::window::PrimaryWindow;

use crate::components::*;
use crate::fog::FogTweakSettings;
use crate::ui::core::hud::MainHudRoot;

const MINIMAP_TEX_SIZE: usize = 200;
const MINIMAP_REFRESH_HZ: f32 = 4.0;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MinimapSet;

#[derive(Resource)]
struct MinimapTexture {
    handle: Handle<Image>,
    base_pixels: Vec<[u8; 4]>,
    scratch_pixels: Vec<[u8; 4]>,
    map_size: f32,
    half_map: f32,
    refresh_timer: Timer,
}

#[derive(Resource, Default)]
pub struct MinimapInteraction {
    pub clicked: bool,
}

#[derive(Component)]
pub struct MinimapNode;

/// Marker for the widget content area that holds the minimap image
#[derive(Component)]
pub struct MinimapWidgetContent;

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MinimapInteraction>()
            .add_systems(
                Update,
                setup_minimap
                    .after(crate::ground::spawn_ground)
                    .run_if(in_state(AppState::InGame))
                    .run_if(any_with_component::<MainHudRoot>),
            )
            .add_systems(
                Update,
                (
                    reset_minimap_interaction,
                    handle_minimap_click,
                    update_minimap_texture,
                )
                    .chain()
                    .in_set(MinimapSet)
                    .in_set(GameFlowSet::Input)
                    .run_if(in_state(AppState::InGame))
                    .run_if(resource_exists::<MinimapTexture>),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_minimap_state);
    }
}

fn biome_color(biome: Biome) -> [u8; 4] {
    match biome {
        Biome::Grassland => [65, 140, 35, 255],
        Biome::Forest => [40, 130, 30, 255],
        Biome::Desert => [210, 190, 110, 255],
        Biome::Beach => [205, 190, 140, 255],
        Biome::Wetland => [75, 105, 50, 255],
        Biome::Water => [30, 60, 150, 255],
        Biome::Mountain => [160, 155, 145, 255],
    }
}

fn world_to_minimap(wx: f32, wz: f32, map_size: f32, half_map: f32) -> (usize, usize) {
    let px = ((wx + half_map) / map_size * MINIMAP_TEX_SIZE as f32) as usize;
    let py = ((wz + half_map) / map_size * MINIMAP_TEX_SIZE as f32) as usize;
    (px.min(MINIMAP_TEX_SIZE - 1), py.min(MINIMAP_TEX_SIZE - 1))
}

fn minimap_to_world(px: f32, py: f32, map_size: f32, half_map: f32) -> (f32, f32) {
    let wx = (px / MINIMAP_TEX_SIZE as f32) * map_size - half_map;
    let wz = (py / MINIMAP_TEX_SIZE as f32) * map_size - half_map;
    (wx, wz)
}

fn draw_dot(buf: &mut [[u8; 4]], cx: usize, cy: usize, radius: usize, color: [u8; 4]) {
    let r = radius as isize;
    for dy in -r..=r {
        for dx in -r..=r {
            let px = cx as isize + dx;
            let py = cy as isize + dy;
            if px >= 0
                && py >= 0
                && (px as usize) < MINIMAP_TEX_SIZE
                && (py as usize) < MINIMAP_TEX_SIZE
            {
                buf[py as usize * MINIMAP_TEX_SIZE + px as usize] = color;
            }
        }
    }
}

fn draw_line(buf: &mut [[u8; 4]], x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut cx = x0;
    let mut cy = y0;

    loop {
        if cx >= 0
            && cy >= 0
            && (cx as usize) < MINIMAP_TEX_SIZE
            && (cy as usize) < MINIMAP_TEX_SIZE
        {
            buf[cy as usize * MINIMAP_TEX_SIZE + cx as usize] = color;
        }
        if cx == x1 && cy == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            cx += sx;
        }
        if e2 <= dx {
            err += dx;
            cy += sy;
        }
    }
}

fn setup_minimap(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    biome_map: Res<BiomeMap>,
    existing_texture: Option<Res<MinimapTexture>>,
    mut content_q: Query<(Entity, &mut Node), With<MinimapWidgetContent>>,
) {
    if existing_texture.is_some() {
        return;
    }

    // Wait until the widget content entity exists (spawned by ExternalWidgetFramesPlugin)
    if content_q.is_empty() {
        return;
    }

    let map_size = biome_map.map_size;
    let half_map = map_size * 0.5;

    // Pre-compute biome base pixels
    let mut base_pixels = vec![[0u8; 4]; MINIMAP_TEX_SIZE * MINIMAP_TEX_SIZE];
    for py in 0..MINIMAP_TEX_SIZE {
        for px in 0..MINIMAP_TEX_SIZE {
            let (wx, wz) = minimap_to_world(px as f32 + 0.5, py as f32 + 0.5, map_size, half_map);
            let biome = biome_map.get_biome(wx, wz);
            base_pixels[py * MINIMAP_TEX_SIZE + px] = biome_color(biome);
        }
    }

    // Create 200x200 RGBA texture
    let size = Extent3d {
        width: MINIMAP_TEX_SIZE as u32,
        height: MINIMAP_TEX_SIZE as u32,
        depth_or_array_layers: 1,
    };
    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    let handle = images.add(image);

    commands.insert_resource(MinimapTexture {
        handle: handle.clone(),
        base_pixels,
        scratch_pixels: vec![[0u8; 4]; MINIMAP_TEX_SIZE * MINIMAP_TEX_SIZE],
        map_size,
        half_map,
        refresh_timer: Timer::from_seconds(1.0 / MINIMAP_REFRESH_HZ, TimerMode::Repeating),
    });

    // Spawn minimap image inside the widget content area
    if content_q.is_empty() {
        warn!("Minimap: MinimapWidgetContent entity not found — HUD may not have spawned yet");
    }
    if let Ok((content_entity, mut content_node)) = content_q.single_mut() {
        info!(
            "Minimap: spawning image node inside widget content entity {:?}",
            content_entity
        );
        content_node.padding = UiRect::ZERO;
        content_node.overflow = Overflow::clip();

        let minimap_image = commands
            .spawn((
                GameWorld,
                MinimapNode,
                Interaction::default(),
                RelativeCursorPosition::default(),
                ImageNode::new(handle).with_mode(NodeImageMode::Stretch),
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(content_entity).add_child(minimap_image);
    }
}

fn reset_minimap_interaction(mut interaction: ResMut<MinimapInteraction>) {
    interaction.clicked = false;
}

fn cleanup_minimap_state(mut commands: Commands) {
    commands.remove_resource::<MinimapTexture>();
}

/// Pre-computed mapping from minimap pixel index to fog grid index.
/// Built once, reused every frame to avoid per-pixel coordinate math.
#[derive(Default)]
struct MinimapFogIndexCache {
    /// For each minimap pixel (MINIMAP_TEX_SIZE²), the corresponding fog grid
    /// index, or usize::MAX if out of bounds.
    indices: Vec<usize>,
    /// Cached fog grid_size used to detect when the cache needs rebuilding.
    fog_grid_size: usize,
}

fn update_minimap_texture(
    time: Res<Time>,
    mut minimap_tex: ResMut<MinimapTexture>,
    minimap_interaction: Res<MinimapInteraction>,
    mut images: ResMut<Assets<Image>>,
    fog_map: Option<Res<FogOfWarMap>>,
    fog_settings: Res<FogTweakSettings>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    units: Query<(&Transform, &Faction), With<Unit>>,
    buildings: Query<(&Transform, &Faction), (With<Building>, Without<FloorTile>)>,
    mobs: Query<&Transform, With<Mob>>,
    resource_nodes: Query<&Transform, With<ResourceNode>>,
    camera_q: Query<(&Camera, &GlobalTransform, &RtsCamera)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut fog_idx_cache: Local<MinimapFogIndexCache>,
) {
    minimap_tex.refresh_timer.tick(time.delta());
    if !minimap_tex.refresh_timer.just_finished() && !minimap_interaction.clicked {
        return;
    }

    let handle = minimap_tex.handle.clone();
    let map_size = minimap_tex.map_size;
    let half_map = minimap_tex.half_map;

    let Some(image) = images.get_mut(&handle) else {
        warn_once!("Minimap: image handle not found in assets");
        return;
    };
    let Some(ref mut data) = image.data else {
        warn_once!("Minimap: image.data is None — texture may have been moved to GPU only");
        return;
    };

    // Work with a pixel buffer for convenience
    let (base_pixels, buf) = {
        let tex = &mut *minimap_tex;
        (&tex.base_pixels, &mut tex.scratch_pixels)
    };
    buf.copy_from_slice(base_pixels);

    // Apply fog of war with color blending (matching in-game fog aesthetic).
    // Uses a pre-computed index cache to avoid per-pixel coordinate math.
    if !fog_settings.reveal_all {
        if let Some(ref fog) = fog_map {
            // Rebuild index cache if fog grid changed or not yet built.
            let pixel_count = MINIMAP_TEX_SIZE * MINIMAP_TEX_SIZE;
            if fog_idx_cache.indices.len() != pixel_count
                || fog_idx_cache.fog_grid_size != fog.grid_size
            {
                fog_idx_cache.indices.resize(pixel_count, usize::MAX);
                fog_idx_cache.fog_grid_size = fog.grid_size;
                for py in 0..MINIMAP_TEX_SIZE {
                    for px in 0..MINIMAP_TEX_SIZE {
                        let (wx, wz) = minimap_to_world(
                            px as f32 + 0.5,
                            py as f32 + 0.5,
                            map_size,
                            half_map,
                        );
                        let ix = ((wx + fog.half_map) / fog.step).round() as usize;
                        let iz = ((wz + fog.half_map) / fog.step).round() as usize;
                        let fog_idx = if ix < fog.grid_size && iz < fog.grid_size {
                            iz * fog.grid_size + ix
                        } else {
                            usize::MAX
                        };
                        fog_idx_cache.indices[py * MINIMAP_TEX_SIZE + px] = fog_idx;
                    }
                }
            }

            let fog_color: [f32; 3] = [0.02, 0.02, 0.06];
            let explored_tint: [f32; 3] = [0.12, 0.10, 0.18];

            for i in 0..pixel_count {
                let fog_idx = fog_idx_cache.indices[i];
                let vis = if fog_idx != usize::MAX {
                    let v = fog.display[fog_idx];
                    if v > 0.01 {
                        0.5 + 0.5 * v
                    } else if fog.explored[fog_idx] != 0 {
                        0.5
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

                let base = [
                    buf[i][0] as f32 / 255.0,
                    buf[i][1] as f32 / 255.0,
                    buf[i][2] as f32 / 255.0,
                ];

                let blended = if vis < 0.01 {
                    let t = 0.05;
                    [
                        base[0] * t + fog_color[0] * (1.0 - t),
                        base[1] * t + fog_color[1] * (1.0 - t),
                        base[2] * t + fog_color[2] * (1.0 - t),
                    ]
                } else if vis < 0.6 {
                    let t = vis / 0.6;
                    let tinted = [
                        base[0] * 0.4 + explored_tint[0] * 0.6,
                        base[1] * 0.4 + explored_tint[1] * 0.6,
                        base[2] * 0.4 + explored_tint[2] * 0.6,
                    ];
                    let near_fog = [
                        base[0] * 0.2 + fog_color[0] * 0.8,
                        base[1] * 0.2 + fog_color[1] * 0.8,
                        base[2] * 0.2 + fog_color[2] * 0.8,
                    ];
                    [
                        near_fog[0] + (tinted[0] - near_fog[0]) * t,
                        near_fog[1] + (tinted[1] - near_fog[1]) * t,
                        near_fog[2] + (tinted[2] - near_fog[2]) * t,
                    ]
                } else {
                    let t = ((vis - 0.6) / 0.4).min(1.0);
                    let tinted = [
                        base[0] * 0.4 + explored_tint[0] * 0.6,
                        base[1] * 0.4 + explored_tint[1] * 0.6,
                        base[2] * 0.4 + explored_tint[2] * 0.6,
                    ];
                    [
                        tinted[0] + (base[0] - tinted[0]) * t,
                        tinted[1] + (base[1] - tinted[1]) * t,
                        tinted[2] + (base[2] - tinted[2]) * t,
                    ]
                };

                buf[i][0] = (blended[0] * 255.0).clamp(0.0, 255.0) as u8;
                buf[i][1] = (blended[1] * 255.0).clamp(0.0, 255.0) as u8;
                buf[i][2] = (blended[2] * 255.0).clamp(0.0, 255.0) as u8;
            }
        }
    }

    // Draw resource nodes (yellow, only if fog-visible)
    for tf in &resource_nodes {
        if !fog_settings.reveal_all {
            if let Some(ref fog) = fog_map {
                if fog.get_visibility(tf.translation.x, tf.translation.z) < 0.6 {
                    continue;
                }
            }
        }
        let (px, py) = world_to_minimap(
            tf.translation.x,
            tf.translation.z,
            map_size,
            half_map,
        );
        draw_dot(buf, px, py, 1, [255, 220, 50, 255]);
    }

    // Draw mobs (red, only if fog-visible)
    for tf in &mobs {
        if !fog_settings.reveal_all {
            if let Some(ref fog) = fog_map {
                if fog.get_visibility(tf.translation.x, tf.translation.z) < 0.6 {
                    continue;
                }
            }
        }
        let (px, py) = world_to_minimap(
            tf.translation.x,
            tf.translation.z,
            map_size,
            half_map,
        );
        draw_dot(buf, px, py, 1, [220, 40, 40, 255]);
    }

    // Draw buildings (allied = always visible, enemy = fog-check)
    for (tf, faction) in &buildings {
        if !fog_settings.reveal_all && !teams.is_allied(&active_player.0, faction) {
            if let Some(ref fog) = fog_map {
                if fog.get_visibility(tf.translation.x, tf.translation.z) < 0.6 {
                    continue;
                }
            }
        }
        let color = match faction {
            Faction::Player1 => [60, 120, 255, 255],
            Faction::Player2 => [255, 80, 50, 255],
            Faction::Player3 => [180, 80, 230, 255],
            Faction::Player4 => [50, 200, 80, 255],
            Faction::Neutral => [220, 40, 40, 255],
        };
        let (px, py) = world_to_minimap(
            tf.translation.x,
            tf.translation.z,
            map_size,
            half_map,
        );
        draw_dot(buf, px, py, 2, color);
    }

    // Draw units (allied = always visible, enemy = fog-check)
    for (tf, faction) in &units {
        if !fog_settings.reveal_all && !teams.is_allied(&active_player.0, faction) {
            if let Some(ref fog) = fog_map {
                if fog.get_visibility(tf.translation.x, tf.translation.z) < 0.6 {
                    continue;
                }
            }
        }
        let color = match faction {
            Faction::Player1 => [50, 220, 50, 255],
            Faction::Player2 => [255, 100, 50, 255],
            Faction::Player3 => [180, 100, 255, 255],
            Faction::Player4 => [50, 255, 80, 255],
            Faction::Neutral => [220, 40, 40, 255],
        };
        let (px, py) = world_to_minimap(
            tf.translation.x,
            tf.translation.z,
            map_size,
            half_map,
        );
        draw_dot(buf, px, py, 1, color);
    }

    // Draw camera viewport rectangle
    if let Ok((camera, cam_gt, _)) = camera_q.single() {
        if let Ok(window) = windows.single() {
            let w = window.width();
            let h = window.height();
            let corners = [
                Vec2::new(0.0, 0.0),
                Vec2::new(w, 0.0),
                Vec2::new(w, h),
                Vec2::new(0.0, h),
            ];

            let mut minimap_corners: Vec<(i32, i32)> = Vec::new();
            for corner in &corners {
                if let Ok(ray) = camera.viewport_to_world(cam_gt, *corner) {
                    if ray.direction.y.abs() > 0.001 {
                        let t = -ray.origin.y / ray.direction.y;
                        if t > 0.0 {
                            let hit = ray.origin + ray.direction * t;
                            let (px, py) = world_to_minimap(
                                hit.x,
                                hit.z,
                                map_size,
                                half_map,
                            );
                            minimap_corners.push((px as i32, py as i32));
                        }
                    }
                }
            }

            let white = [255, 255, 255, 200];
            let n = minimap_corners.len();
            if n >= 2 {
                for i in 0..n {
                    let (x0, y0) = minimap_corners[i];
                    let (x1, y1) = minimap_corners[(i + 1) % n];
                    draw_line(buf, x0, y0, x1, y1, white);
                }
            }
        }
    }

    // Write buffer to image data
    for (i, pixel) in buf.iter().enumerate() {
        let offset = i * 4;
        data[offset] = pixel[0];
        data[offset + 1] = pixel[1];
        data[offset + 2] = pixel[2];
        data[offset + 3] = pixel[3];
    }
}

fn handle_minimap_click(
    mouse: Res<ButtonInput<MouseButton>>,
    minimap_tex: Res<MinimapTexture>,
    minimap_nodes: Query<(&Interaction, &RelativeCursorPosition), With<MinimapNode>>,
    mut camera_q: Query<&mut RtsCamera>,
    mut minimap_interaction: ResMut<MinimapInteraction>,
) {
    if !mouse.pressed(MouseButton::Left) {
        return;
    }

    let Ok((interaction, rel_cursor)) = minimap_nodes.single() else {
        return;
    };

    if *interaction != Interaction::Pressed {
        return;
    }

    minimap_interaction.clicked = true;

    // RelativeCursorPosition.normalized: (-0.5,-0.5) = top-left, (0.5,0.5) = bottom-right
    if let Some(norm) = rel_cursor.normalized {
        let uv_x = norm.x + 0.5;
        let uv_y = norm.y + 0.5;
        let (wx, wz) = minimap_to_world(
            uv_x * MINIMAP_TEX_SIZE as f32,
            uv_y * MINIMAP_TEX_SIZE as f32,
            minimap_tex.map_size,
            minimap_tex.half_map,
        );

        if let Ok(mut cam) = camera_q.single_mut() {
            cam.target_pivot.x = wx;
            cam.target_pivot.z = wz;
        }
    }
}
