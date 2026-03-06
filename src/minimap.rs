use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::ui::RelativeCursorPosition;
use bevy::window::PrimaryWindow;

use crate::components::*;
use crate::ground::{HALF_MAP, MAP_SIZE};

const MINIMAP_TEX_SIZE: usize = 200;
const MINIMAP_UI_SIZE: f32 = 180.0;
const MINIMAP_MARGIN: f32 = 10.0;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MinimapSet;

#[derive(Resource)]
struct MinimapTexture {
    handle: Handle<Image>,
    base_pixels: Vec<[u8; 4]>,
}

#[derive(Resource, Default)]
pub struct MinimapInteraction {
    pub clicked: bool,
}

#[derive(Component)]
struct MinimapNode;

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MinimapInteraction>()
            .add_systems(PostStartup, setup_minimap)
            .add_systems(
                Update,
                (
                    reset_minimap_interaction,
                    handle_minimap_click,
                    update_minimap_texture,
                )
                    .chain()
                    .in_set(MinimapSet),
            );
    }
}

fn biome_color(biome: Biome) -> [u8; 4] {
    match biome {
        Biome::Forest => [40, 130, 30, 255],
        Biome::Desert => [210, 190, 110, 255],
        Biome::Mud => [120, 85, 50, 255],
        Biome::Water => [30, 60, 150, 255],
        Biome::Mountain => [160, 155, 145, 255],
    }
}

fn world_to_minimap(wx: f32, wz: f32) -> (usize, usize) {
    let px = ((wx + HALF_MAP) / MAP_SIZE * MINIMAP_TEX_SIZE as f32) as usize;
    let py = ((wz + HALF_MAP) / MAP_SIZE * MINIMAP_TEX_SIZE as f32) as usize;
    (
        px.min(MINIMAP_TEX_SIZE - 1),
        py.min(MINIMAP_TEX_SIZE - 1),
    )
}

fn minimap_to_world(px: f32, py: f32) -> (f32, f32) {
    let wx = (px / MINIMAP_TEX_SIZE as f32) * MAP_SIZE - HALF_MAP;
    let wz = (py / MINIMAP_TEX_SIZE as f32) * MAP_SIZE - HALF_MAP;
    (wx, wz)
}

fn draw_dot(buf: &mut [[u8; 4]], cx: usize, cy: usize, radius: usize, color: [u8; 4]) {
    let r = radius as isize;
    for dy in -r..=r {
        for dx in -r..=r {
            let px = cx as isize + dx;
            let py = cy as isize + dy;
            if px >= 0 && py >= 0 && (px as usize) < MINIMAP_TEX_SIZE && (py as usize) < MINIMAP_TEX_SIZE {
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
        if cx >= 0 && cy >= 0 && (cx as usize) < MINIMAP_TEX_SIZE && (cy as usize) < MINIMAP_TEX_SIZE {
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
) {
    // Pre-compute biome base pixels
    let mut base_pixels = vec![[0u8; 4]; MINIMAP_TEX_SIZE * MINIMAP_TEX_SIZE];
    for py in 0..MINIMAP_TEX_SIZE {
        for px in 0..MINIMAP_TEX_SIZE {
            let (wx, wz) = minimap_to_world(px as f32 + 0.5, py as f32 + 0.5);
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
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
    let handle = images.add(image);

    commands.insert_resource(MinimapTexture {
        handle: handle.clone(),
        base_pixels,
    });

    // Spawn UI: container in bottom-right
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(MINIMAP_MARGIN),
                bottom: Val::Px(MINIMAP_MARGIN),
                width: Val::Px(MINIMAP_UI_SIZE + 6.0),
                height: Val::Px(MINIMAP_UI_SIZE + 6.0),
                padding: UiRect::all(Val::Px(3.0)),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.05, 0.1, 0.85)),
            BorderColor::all(Color::srgba(0.3, 0.35, 0.5, 0.8)),
        ))
        .with_children(|parent| {
            parent.spawn((
                MinimapNode,
                Interaction::default(),
                RelativeCursorPosition::default(),
                ImageNode::new(handle),
                Node {
                    width: Val::Px(MINIMAP_UI_SIZE),
                    height: Val::Px(MINIMAP_UI_SIZE),
                    ..default()
                },
            ));
        });
}

fn reset_minimap_interaction(mut interaction: ResMut<MinimapInteraction>) {
    interaction.clicked = false;
}

fn update_minimap_texture(
    minimap_tex: Res<MinimapTexture>,
    mut images: ResMut<Assets<Image>>,
    fog_map: Option<Res<FogOfWarMap>>,
    units: Query<(&Transform, &Faction), With<Unit>>,
    buildings: Query<(&Transform, &Faction), With<Building>>,
    mobs: Query<&Transform, With<Mob>>,
    resource_nodes: Query<&Transform, With<ResourceNode>>,
    camera_q: Query<(&Camera, &GlobalTransform, &RtsCamera)>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Some(image) = images.get_mut(&minimap_tex.handle) else {
        return;
    };
    let Some(ref mut data) = image.data else {
        return;
    };

    // Work with a pixel buffer for convenience
    let total = MINIMAP_TEX_SIZE * MINIMAP_TEX_SIZE;
    let mut buf = vec![[0u8; 4]; total];
    buf.copy_from_slice(&minimap_tex.base_pixels);

    // Apply fog of war with color blending (matching in-game fog aesthetic)
    if let Some(ref fog) = fog_map {
        // Fog colors matching the in-game FogSettings
        let fog_color: [f32; 3] = [0.02, 0.02, 0.06]; // dark navy for unexplored
        let explored_tint: [f32; 3] = [0.12, 0.10, 0.18]; // muted purple for explored

        for py in 0..MINIMAP_TEX_SIZE {
            for px in 0..MINIMAP_TEX_SIZE {
                let (wx, wz) = minimap_to_world(px as f32 + 0.5, py as f32 + 0.5);
                let vis = fog.get_visibility(wx, wz);
                let idx = py * MINIMAP_TEX_SIZE + px;
                let base = [
                    buf[idx][0] as f32 / 255.0,
                    buf[idx][1] as f32 / 255.0,
                    buf[idx][2] as f32 / 255.0,
                ];

                let blended = if vis < 0.01 {
                    // Unexplored: ~85% fog color overlay
                    let t = 0.15;
                    [
                        base[0] * t + fog_color[0] * (1.0 - t),
                        base[1] * t + fog_color[1] * (1.0 - t),
                        base[2] * t + fog_color[2] * (1.0 - t),
                    ]
                } else if vis < 0.6 {
                    // Explored but not visible: blend between fog-tinted and explored
                    let t = vis / 0.6; // 0.0 at edge of explored, 1.0 near visible
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
                    // Visible: smooth fade from explored tint to full color
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

                buf[idx][0] = (blended[0] * 255.0).clamp(0.0, 255.0) as u8;
                buf[idx][1] = (blended[1] * 255.0).clamp(0.0, 255.0) as u8;
                buf[idx][2] = (blended[2] * 255.0).clamp(0.0, 255.0) as u8;
            }
        }
    }

    // Draw resource nodes (yellow, only if fog-visible)
    for tf in &resource_nodes {
        if let Some(ref fog) = fog_map {
            if fog.get_visibility(tf.translation.x, tf.translation.z) < 0.6 {
                continue;
            }
        }
        let (px, py) = world_to_minimap(tf.translation.x, tf.translation.z);
        draw_dot(&mut buf, px, py, 1, [255, 220, 50, 255]);
    }

    // Draw mobs (red, only if fog-visible)
    for tf in &mobs {
        if let Some(ref fog) = fog_map {
            if fog.get_visibility(tf.translation.x, tf.translation.z) < 0.6 {
                continue;
            }
        }
        let (px, py) = world_to_minimap(tf.translation.x, tf.translation.z);
        draw_dot(&mut buf, px, py, 1, [220, 40, 40, 255]);
    }

    // Draw buildings
    for (tf, faction) in &buildings {
        let color = match faction {
            Faction::Player => [60, 120, 255, 255],
            Faction::Enemy => [220, 40, 40, 255],
        };
        let (px, py) = world_to_minimap(tf.translation.x, tf.translation.z);
        draw_dot(&mut buf, px, py, 2, color);
    }

    // Draw units
    for (tf, faction) in &units {
        let color = match faction {
            Faction::Player => [50, 220, 50, 255],
            Faction::Enemy => [220, 40, 40, 255],
        };
        let (px, py) = world_to_minimap(tf.translation.x, tf.translation.z);
        draw_dot(&mut buf, px, py, 1, color);
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
                            let (px, py) = world_to_minimap(hit.x, hit.z);
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
                    draw_line(&mut buf, x0, y0, x1, y1, white);
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
    minimap_nodes: Query<
        (&Interaction, &RelativeCursorPosition),
        With<MinimapNode>,
    >,
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
        );

        if let Ok(mut cam) = camera_q.single_mut() {
            cam.target_pivot.x = wx;
            cam.target_pivot.z = wz;
        }
    }
}
