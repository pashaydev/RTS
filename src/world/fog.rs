use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

use crate::types::*;
use crate::presentation::materials::fog::{FogOfWarMaterial, FogSettings};
use crate::world::ground::HeightMap;

// ── Resources ──

const FOG_OVERLAY_VERTEX_STRIDE: usize = 4;

/// Handles to the two GPU textures (visible + explored).
#[derive(Resource)]
pub struct FogTextures {
    pub visible: Handle<Image>,
    pub explored: Handle<Image>,
}

#[derive(Resource, Default)]
pub struct FogTextureUploadState {
    pub explored_dirty: bool,
}

/// Controls fog update frequency. Heavy systems only run on tick frames.
#[derive(Resource)]
struct FogTickTimer {
    timer: Timer,
    accumulated_dt: f32,
    ticked_this_frame: bool,
}

impl Default for FogTickTimer {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(1.0 / 12.0, TimerMode::Repeating),
            accumulated_dt: 0.0,
            ticked_this_frame: false,
        }
    }
}

/// Tweakable gameplay thresholds for fog of war.
#[derive(Resource)]
pub struct FogTweakSettings {
    pub mob_threshold: f32,
    pub object_threshold: f32,
    pub vfx_threshold: f32,
    pub transition_speed: f32,
    pub reveal_all: bool,
    pub enable_los: bool,
    pub los_ray_count: usize,
    // Performance toggles
    pub enable_visibility_update: bool,
    pub enable_display_lerp: bool,
    pub enable_texture_upload: bool,
    pub enable_entity_hiding: bool,
    pub tick_rate_hz: f32,
    pub shader_quality: f32,
}

impl Default for FogTweakSettings {
    fn default() -> Self {
        Self {
            mob_threshold: 0.8,
            object_threshold: 0.4,
            vfx_threshold: 0.3,
            transition_speed: 4.0,
            reveal_all: false,
            enable_los: true,
            los_ray_count: 48,
            enable_visibility_update: true,
            enable_display_lerp: true,
            enable_texture_upload: true,
            enable_entity_hiding: true,
            tick_rate_hz: 12.0,
            shader_quality: 2.0,
        }
    }
}

// ── Plugin ──

pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<FogOfWarMaterial>::default())
            .init_resource::<FogTweakSettings>()
            .init_resource::<FogTickTimer>()
            .add_systems(
                OnEnter(AppState::InGame),
                (spawn_fog_overlay, register_fog_tweaks).after(crate::world::ground::spawn_ground),
            )
            .add_systems(
                Update,
                (
                    tick_fog_timer,
                    update_fog_overlay_visibility,
                    update_fog_visibility,
                    update_fog_display,
                    update_fog_textures,
                    update_fog_material_time,
                    fog_hide_entities,
                )
                    .chain()
                    .after(crate::world::culling::CullingSet)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// ── Debug Tweaks Registration ──

fn register_fog_tweaks(mut tweaks: ResMut<crate::infrastructure::debug::DebugTweaks>) {
    let s = FogSettings::default();

    tweaks.add_float(
        "Visuals/FoW Shader",
        "Noise Scale",
        s.noise_scale,
        0.0,
        30.0,
        0.5,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Edge Glow Width",
        s.edge_glow_width,
        0.0,
        0.5,
        0.01,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Edge Glow Intensity",
        s.edge_glow_intensity,
        0.0,
        2.0,
        0.05,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Fog Color R",
        s.fog_color.x,
        0.0,
        1.0,
        0.01,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Fog Color G",
        s.fog_color.y,
        0.0,
        1.0,
        0.01,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Fog Color B",
        s.fog_color.z,
        0.0,
        1.0,
        0.01,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Fog Color A",
        s.fog_color.w,
        0.0,
        1.0,
        0.01,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Glow Color R",
        s.glow_color.x,
        0.0,
        1.0,
        0.01,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Glow Color G",
        s.glow_color.y,
        0.0,
        1.0,
        0.01,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Glow Color B",
        s.glow_color.z,
        0.0,
        1.0,
        0.01,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Glow Color A",
        s.glow_color.w,
        0.0,
        1.0,
        0.01,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Explored Tint R",
        s.explored_tint.x,
        0.0,
        1.0,
        0.01,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Explored Tint G",
        s.explored_tint.y,
        0.0,
        1.0,
        0.01,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Explored Tint B",
        s.explored_tint.z,
        0.0,
        1.0,
        0.01,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Explored Tint A",
        s.explored_tint.w,
        0.0,
        1.0,
        0.01,
    );

    tweaks.add_float(
        "Visuals/FoW Shader",
        "Scale",
        s.fog_noise_scale,
        1.0,
        20.0,
        0.5,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Speed",
        s.fog_noise_speed,
        0.0,
        0.1,
        0.005,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Warp",
        s.fog_noise_warp,
        0.0,
        3.0,
        0.1,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Contrast",
        s.fog_noise_contrast,
        0.0,
        1.0,
        0.05,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Octaves",
        s.fog_noise_octaves,
        1.0,
        6.0,
        1.0,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Tendril Scale",
        s.fog_tendril_scale,
        1.0,
        20.0,
        0.5,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Tendril Strength",
        s.fog_tendril_strength,
        0.0,
        2.0,
        0.05,
    );
    tweaks.add_float(
        "Visuals/FoW Shader",
        "Warp Speed",
        s.fog_warp_speed,
        0.0,
        3.0,
        0.1,
    );

    let t = FogTweakSettings::default();
    tweaks.add_float(
        "Visuals/FoW Gameplay",
        "Mob Threshold",
        t.mob_threshold,
        0.0,
        1.0,
        0.05,
    );
    tweaks.add_float(
        "Visuals/FoW Gameplay",
        "Object Threshold",
        t.object_threshold,
        0.0,
        1.0,
        0.05,
    );
    tweaks.add_float(
        "Visuals/FoW Gameplay",
        "VFX Threshold",
        t.vfx_threshold,
        0.0,
        1.0,
        0.05,
    );
    tweaks.add_float(
        "Visuals/FoW Gameplay",
        "Transition Speed",
        t.transition_speed,
        0.5,
        20.0,
        0.5,
    );
    tweaks.add_bool("Visuals/FoW Gameplay", "Reveal Full Map", t.reveal_all);
    tweaks.add_bool("Visuals/FoW Gameplay", "Enable LOS", t.enable_los);
    tweaks.add_float(
        "Visuals/FoW Gameplay",
        "LOS Ray Count",
        t.los_ray_count as f32,
        8.0,
        128.0,
        8.0,
    );
    tweaks.add_bool(
        "Visuals/FoW Performance",
        "Visibility Update",
        t.enable_visibility_update,
    );
    tweaks.add_bool(
        "Visuals/FoW Performance",
        "Display Lerp",
        t.enable_display_lerp,
    );
    tweaks.add_bool(
        "Visuals/FoW Performance",
        "Texture Upload",
        t.enable_texture_upload,
    );
    tweaks.add_bool(
        "Visuals/FoW Performance",
        "Entity Hiding",
        t.enable_entity_hiding,
    );
    tweaks.add_float(
        "Visuals/FoW Performance",
        "Tick Rate Hz",
        t.tick_rate_hz,
        4.0,
        60.0,
        1.0,
    );
    tweaks.add_float(
        "Visuals/FoW Performance",
        "Shader Quality",
        t.shader_quality,
        0.0,
        2.0,
        1.0,
    );
}

// ── Texture Creation ──

fn create_r8_texture(images: &mut Assets<Image>, grid_size: usize) -> Handle<Image> {
    let size = Extent3d {
        width: grid_size as u32,
        height: grid_size as u32,
        depth_or_array_layers: 1,
    };
    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0u8],
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
    images.add(image)
}

// ── Spawn ──

fn spawn_fog_overlay(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut fog_materials: ResMut<Assets<FogOfWarMaterial>>,
    mut images: ResMut<Assets<Image>>,
    height_map: Res<HeightMap>,
) {
    // Fog data grid uses a coarser resolution than the terrain grid.
    // GPU bilinear texture sampling smooths the visual result.
    let fog_step = height_map.step * 2.0;
    let fog_grid_size = ((height_map.map_size / fog_step).ceil() as usize + 1).min(256);

    // Overlay mesh uses the terrain grid with its own vertex stride
    let grid_size = height_map.grid_size;
    let step = height_map.step;
    let half_map = height_map.half_map;
    let overlay_cells = (grid_size - 1).div_ceil(FOG_OVERLAY_VERTEX_STRIDE);
    let overlay_grid_size = overlay_cells + 1;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(overlay_grid_size * overlay_grid_size);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(overlay_grid_size * overlay_grid_size);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(overlay_grid_size * overlay_grid_size);

    for iz in 0..overlay_grid_size {
        for ix in 0..overlay_grid_size {
            let src_ix = (ix * FOG_OVERLAY_VERTEX_STRIDE).min(grid_size - 1);
            let src_iz = (iz * FOG_OVERLAY_VERTEX_STRIDE).min(grid_size - 1);
            let x = -half_map + src_ix as f32 * step;
            let z = -half_map + src_iz as f32 * step;
            let y = height_map.sample(x, z) + 4.0;
            positions.push([x, y, z]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([
                src_ix as f32 / (grid_size - 1) as f32,
                src_iz as f32 / (grid_size - 1) as f32,
            ]);
        }
    }

    let mut indices: Vec<u32> =
        Vec::with_capacity((overlay_grid_size - 1) * (overlay_grid_size - 1) * 6);
    for iz in 0..(overlay_grid_size - 1) {
        for ix in 0..(overlay_grid_size - 1) {
            let tl = (iz * overlay_grid_size + ix) as u32;
            let tr = tl + 1;
            let bl = tl + overlay_grid_size as u32;
            let br = bl + 1;
            indices.push(tl);
            indices.push(bl);
            indices.push(tr);
            indices.push(tr);
            indices.push(bl);
            indices.push(br);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));

    let vis_handle = create_r8_texture(&mut images, fog_grid_size);
    let exp_handle = create_r8_texture(&mut images, fog_grid_size);

    let material = fog_materials.add(FogOfWarMaterial {
        settings: FogSettings::default(),
        visible_texture: Some(vis_handle.clone()),
        explored_texture: Some(exp_handle.clone()),
    });

    commands.spawn((
        GameWorld,
        FogOverlay,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        Transform::default(),
        NotShadowCaster,
        NotShadowReceiver,
    ));

    commands.insert_resource(FogTextures {
        visible: vis_handle,
        explored: exp_handle,
    });
    commands.insert_resource(FogTextureUploadState {
        explored_dirty: true,
    });

    let total = fog_grid_size * fog_grid_size;
    commands.insert_resource(FogOfWarMap {
        visible: vec![0.0; total],
        visible_next: vec![0.0; total],
        explored: vec![0; total],
        display: vec![0.0; total],
        grid_size: fog_grid_size,
        step: fog_step,
        half_map,
    });
}

#[inline]
fn reveal_fog_cell(fog_map: &mut FogOfWarMap, idx: usize, vis: f32, explored_dirty: &mut bool) {
    if vis > fog_map.visible[idx] {
        fog_map.visible[idx] = vis;
    }
    if fog_map.explored[idx] == 0 {
        fog_map.explored[idx] = u8::MAX;
        *explored_dirty = true;
    }
}

// ── Tick Timer ──

fn tick_fog_timer(
    mut fog_timer: ResMut<FogTickTimer>,
    fog_settings: Res<FogTweakSettings>,
    time: Res<Time>,
) {
    // Sync tick rate from tweaks
    let desired_duration =
        std::time::Duration::from_secs_f32(1.0 / fog_settings.tick_rate_hz.max(1.0));
    if fog_timer.timer.duration() != desired_duration {
        fog_timer.timer.set_duration(desired_duration);
    }

    fog_timer.accumulated_dt += time.delta_secs();
    fog_timer.timer.tick(time.delta());
    fog_timer.ticked_this_frame = fog_timer.timer.just_finished();
}

// ── Overlay Visibility ──

fn update_fog_overlay_visibility(
    fog_settings: Res<FogTweakSettings>,
    mut fog_overlay: Query<&mut Visibility, With<FogOverlay>>,
) {
    let should_hide = fog_settings.reveal_all
        || (!fog_settings.enable_visibility_update
            && !fog_settings.enable_display_lerp
            && !fog_settings.enable_texture_upload);

    if let Ok(mut vis) = fog_overlay.single_mut() {
        let target = if should_hide {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *vis != target {
            *vis = target;
        }
    }
}

// ── Visibility Update (with terrain LOS) ──

fn update_fog_visibility(
    mut fog_map: ResMut<FogOfWarMap>,
    fog_settings: Res<FogTweakSettings>,
    fog_timer: Res<FogTickTimer>,
    mut upload_state: ResMut<FogTextureUploadState>,
    height_map: Res<HeightMap>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    all_units: Query<(&Transform, &VisionRange, &Faction), With<Unit>>,
    all_buildings: Query<(&Transform, &VisionRange, &Faction), With<Building>>,
    mut viewers: Local<Vec<(Vec3, f32)>>,
) {
    if !fog_settings.enable_visibility_update || !fog_timer.ticked_this_frame {
        return;
    }

    if fog_settings.reveal_all {
        fog_map.visible.fill(1.0);
        if fog_map.explored.iter().any(|&v| v == 0) {
            fog_map.explored.fill(u8::MAX);
            upload_state.explored_dirty = true;
        }
        return;
    }

    let grid_size = fog_map.grid_size;
    let step = fog_map.step;
    let half_map = fog_map.half_map;
    let mut explored_dirty = false;

    // Clear visible layer each frame
    fog_map.visible.fill(0.0);

    // Collect viewers for the active player's team (own + allied factions)
    let active_faction = active_player.0;
    viewers.clear();
    viewers.reserve(all_units.iter().len() + all_buildings.iter().len());
    for (tf, vr, faction) in all_units.iter() {
        if teams.is_allied(&active_faction, faction) {
            viewers.push((tf.translation, vr.0));
        }
    }
    for (tf, vr, faction) in all_buildings.iter() {
        if teams.is_allied(&active_faction, faction) {
            viewers.push((tf.translation, vr.0));
        }
    }

    let enable_los = fog_settings.enable_los;
    let ray_count = fog_settings.los_ray_count;
    // Terrain height sampling uses the terrain grid (finer resolution)
    let terrain_heights = &height_map.heights;
    let terrain_step = height_map.step;
    let terrain_grid_size = height_map.grid_size;

    for (pos, range) in &*viewers {
        let range_sq = range * range;
        let viewer_height = pos.y + 2.0; // eye height above ground

        let min_x = ((pos.x - range + half_map) / step).floor().max(0.0) as usize;
        let max_x = ((pos.x + range + half_map) / step)
            .ceil()
            .min((grid_size - 1) as f32) as usize;
        let min_z = ((pos.z - range + half_map) / step).floor().max(0.0) as usize;
        let max_z = ((pos.z + range + half_map) / step)
            .ceil()
            .min((grid_size - 1) as f32) as usize;

        if enable_los {
            // Terrain-aware LOS using elevation angle raycasting.
            // Ray steps use fog grid step (coarser = fewer iterations).
            // Terrain height is sampled from the finer terrain grid for accuracy.
            let max_steps = (*range / step).ceil() as usize + 1;

            for ray_i in 0..ray_count {
                let angle = std::f32::consts::TAU * ray_i as f32 / ray_count as f32;
                let dir_x = angle.cos();
                let dir_z = angle.sin();

                let mut max_angle = f32::NEG_INFINITY;

                for s in 1..=max_steps {
                    let dist = s as f32 * step;
                    if dist * dist > range_sq {
                        break;
                    }

                    let wx = pos.x + dir_x * dist;
                    let wz = pos.z + dir_z * dist;

                    // Fog grid index (coarser)
                    let fix = ((wx + half_map) / step).round();
                    let fiz = ((wz + half_map) / step).round();
                    if fix < 0.0 || fiz < 0.0 {
                        continue;
                    }
                    let ix = fix as usize;
                    let iz = fiz as usize;
                    if ix >= grid_size || iz >= grid_size {
                        break;
                    }

                    // Terrain height from the finer terrain grid
                    let tix = ((wx + half_map) / terrain_step)
                        .round()
                        .clamp(0.0, (terrain_grid_size - 1) as f32)
                        as usize;
                    let tiz = ((wz + half_map) / terrain_step)
                        .round()
                        .clamp(0.0, (terrain_grid_size - 1) as f32)
                        as usize;
                    let terrain_h = terrain_heights[tiz * terrain_grid_size + tix];
                    let elevation_angle = (terrain_h - viewer_height) / dist;

                    if elevation_angle > max_angle {
                        max_angle = elevation_angle;

                        let t = dist / range;
                        let edge_fade = 1.0 - t * t;
                        let vis = 0.5 + 0.5 * edge_fade;
                        let fog_idx = iz * grid_size + ix;
                        reveal_fog_cell(&mut fog_map, fog_idx, vis, &mut explored_dirty);
                    }
                }
            }

            // Also mark the viewer's own cell as fully visible
            let vix = ((pos.x + half_map) / step).round() as usize;
            let viz = ((pos.z + half_map) / step).round() as usize;
            if vix < grid_size && viz < grid_size {
                reveal_fog_cell(
                    &mut fog_map,
                    viz * grid_size + vix,
                    1.0,
                    &mut explored_dirty,
                );
            }
        } else {
            // Simple radial distance (no terrain occlusion)
            for iz in min_z..=max_z {
                for ix in min_x..=max_x {
                    let wx = -half_map + ix as f32 * step;
                    let wz = -half_map + iz as f32 * step;
                    let dx = wx - pos.x;
                    let dz = wz - pos.z;
                    let dist_sq = dx * dx + dz * dz;

                    if dist_sq <= range_sq {
                        let t = (dist_sq / range_sq).sqrt();
                        let edge_fade = 1.0 - t * t;
                        let vis = 0.5 + 0.5 * edge_fade;
                        let idx = iz * grid_size + ix;
                        reveal_fog_cell(&mut fog_map, idx, vis, &mut explored_dirty);
                    }
                }
            }
        }
    }
    if explored_dirty {
        upload_state.explored_dirty = true;
    }
}

// ── Smooth Display Interpolation ──

fn update_fog_display(
    mut fog_map: ResMut<FogOfWarMap>,
    fog_settings: Res<FogTweakSettings>,
    mut fog_timer: ResMut<FogTickTimer>,
) {
    if !fog_settings.enable_display_lerp || !fog_timer.ticked_this_frame {
        return;
    }

    if fog_settings.reveal_all {
        for v in fog_map.display.iter_mut() {
            *v = 1.0;
        }
        fog_timer.accumulated_dt = 0.0;
        return;
    }

    // Use accumulated dt since last tick for correct lerp compensation
    let dt = fog_timer.accumulated_dt;
    fog_timer.accumulated_dt = 0.0;
    let speed = fog_settings.transition_speed;
    let lerp_factor = (speed * dt).min(1.0);

    for i in 0..fog_map.visible.len() {
        let target = if fog_map.visible[i] > 0.01 {
            fog_map.visible[i]
        } else if fog_map.explored[i] != 0 {
            0.35
        } else {
            0.0
        };

        let current = fog_map.display[i];
        fog_map.display[i] = current + (target - current) * lerp_factor;
    }
}

// ── Texture Upload ──

fn update_fog_textures(
    fog_map: Res<FogOfWarMap>,
    fog_tex: Res<FogTextures>,
    fog_settings: Res<FogTweakSettings>,
    fog_timer: Res<FogTickTimer>,
    mut upload_state: ResMut<FogTextureUploadState>,
    mut images: ResMut<Assets<Image>>,
) {
    if !fog_settings.enable_texture_upload || !fog_timer.ticked_this_frame {
        return;
    }

    // Upload visible layer (smooth display values)
    if let Some(image) = images.get_mut(&fog_tex.visible) {
        if let Some(ref mut data) = image.data {
            for (dst, src) in data.iter_mut().zip(fog_map.display.iter()) {
                *dst = (src.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }

    // Upload explored layer only when new cells are discovered.
    if upload_state.explored_dirty {
        if let Some(image) = images.get_mut(&fog_tex.explored) {
            if let Some(ref mut data) = image.data {
                data[..fog_map.explored.len()].copy_from_slice(&fog_map.explored);
            }
        }
        upload_state.explored_dirty = false;
    }
}

// ── Shader Time Update ──
// Owns "Visuals/FoW Shader" folder (shader + noise params). Gameplay params
// ("Visuals/FoW Gameplay") are synced in debug.rs::sync_fog_tweaks.

fn update_fog_material_time(
    time: Res<Time>,
    tweaks: Res<crate::infrastructure::debug::DebugTweaks>,
    fog_settings: Res<FogTweakSettings>,
    fog_overlay: Query<&MeshMaterial3d<FogOfWarMaterial>, With<FogOverlay>>,
    mut materials: ResMut<Assets<FogOfWarMaterial>>,
) {
    let Ok(mat_handle) = fog_overlay.single() else {
        return;
    };
    let Some(mat) = materials.get_mut(&mat_handle.0) else {
        return;
    };
    mat.settings.time = time.elapsed_secs();

    // Apply shader tweaks
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Noise Scale") {
        mat.settings.noise_scale = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Edge Glow Width") {
        mat.settings.edge_glow_width = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Edge Glow Intensity") {
        mat.settings.edge_glow_intensity = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Fog Color R") {
        mat.settings.fog_color.x = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Fog Color G") {
        mat.settings.fog_color.y = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Fog Color B") {
        mat.settings.fog_color.z = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Fog Color A") {
        mat.settings.fog_color.w = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Glow Color R") {
        mat.settings.glow_color.x = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Glow Color G") {
        mat.settings.glow_color.y = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Glow Color B") {
        mat.settings.glow_color.z = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Glow Color A") {
        mat.settings.glow_color.w = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Explored Tint R") {
        mat.settings.explored_tint.x = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Explored Tint G") {
        mat.settings.explored_tint.y = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Explored Tint B") {
        mat.settings.explored_tint.z = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Explored Tint A") {
        mat.settings.explored_tint.w = v;
    }

    // Apply fog noise tweaks
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Scale") {
        mat.settings.fog_noise_scale = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Speed") {
        mat.settings.fog_noise_speed = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Warp") {
        mat.settings.fog_noise_warp = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Contrast") {
        mat.settings.fog_noise_contrast = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Octaves") {
        mat.settings.fog_noise_octaves = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Tendril Scale") {
        mat.settings.fog_tendril_scale = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Tendril Strength") {
        mat.settings.fog_tendril_strength = v;
    }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Warp Speed") {
        mat.settings.fog_warp_speed = v;
    }

    // Shader quality from performance tweaks
    mat.settings.quality_level = fog_settings.shader_quality;
}

// ── Unified Entity Hiding ──

#[inline]
fn set_visibility_if_needed(
    vis: &mut Visibility,
    cull_reason: Option<Mut<CullReason>>,
    target: Visibility,
    reason: CullReason,
) {
    if *vis != target {
        *vis = target;
    }
    if let Some(mut current_reason) = cull_reason {
        if *current_reason != reason {
            *current_reason = reason;
        }
    }
}

fn fog_hide_entities(
    fog_map: Res<FogOfWarMap>,
    fog_settings: Res<FogTweakSettings>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    mut hideables: Query<(
        &Transform,
        &mut Visibility,
        &FogHideable,
        Has<FrustumCulled>,
        Option<&mut CullReason>,
    )>,
    mut enemy_units: Query<
        (
            &Transform,
            &mut Visibility,
            &Faction,
            &UnitState,
            Has<FrustumCulled>,
            Option<&mut CullReason>,
        ),
        (With<Unit>, Without<FogHideable>),
    >,
    mut enemy_buildings: Query<
        (
            &Transform,
            &mut Visibility,
            &Faction,
            Has<FrustumCulled>,
            Option<&mut CullReason>,
        ),
        (With<Building>, Without<FogHideable>, Without<Unit>),
    >,
) {
    if !fog_settings.enable_entity_hiding {
        return;
    }

    if fog_settings.reveal_all {
        // When fog is disabled, restore visibility — but skip frustum-culled entities
        // so we don't override the culling system's Visibility::Hidden.
        for (_, mut vis, _, is_culled, cull_reason) in hideables.iter_mut() {
            if !is_culled {
                set_visibility_if_needed(
                    &mut vis,
                    cull_reason,
                    Visibility::Inherited,
                    CullReason::Visible,
                );
            }
        }
        for (_, mut vis, _, _, is_culled, cull_reason) in enemy_units.iter_mut() {
            if !is_culled {
                set_visibility_if_needed(
                    &mut vis,
                    cull_reason,
                    Visibility::Inherited,
                    CullReason::Visible,
                );
            }
        }
        for (_, mut vis, _, is_culled, cull_reason) in enemy_buildings.iter_mut() {
            if !is_culled {
                set_visibility_if_needed(
                    &mut vis,
                    cull_reason,
                    Visibility::Inherited,
                    CullReason::Visible,
                );
            }
        }
        return;
    }

    // FogHideable logic (mobs, objects, decorations, mountains, vfx)
    for (tf, mut vis, hideable, is_culled, cull_reason) in hideables.iter_mut() {
        // Frustum-culled entities are already hidden by culling — skip them.
        if is_culled {
            continue;
        }

        let threshold = match hideable {
            FogHideable::Mob => fog_settings.mob_threshold,
            FogHideable::Object => fog_settings.object_threshold,
            FogHideable::Vfx => fog_settings.vfx_threshold,
        };

        let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
        if v >= threshold {
            set_visibility_if_needed(
                &mut vis,
                cull_reason,
                Visibility::Inherited,
                CullReason::Visible,
            );
        } else {
            set_visibility_if_needed(&mut vis, cull_reason, Visibility::Hidden, CullReason::Fog);
        }
    }

    // Hide enemy player units outside fog vision
    for (tf, mut vis, faction, _unit_state, is_culled, cull_reason) in enemy_units.iter_mut() {
        if is_culled {
            continue;
        }
        if teams.is_allied(&active_player.0, faction) {
            set_visibility_if_needed(
                &mut vis,
                cull_reason,
                Visibility::Inherited,
                CullReason::Visible,
            );
        } else {
            let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
            if v >= fog_settings.mob_threshold {
                set_visibility_if_needed(
                    &mut vis,
                    cull_reason,
                    Visibility::Inherited,
                    CullReason::Visible,
                );
            } else {
                set_visibility_if_needed(
                    &mut vis,
                    cull_reason,
                    Visibility::Hidden,
                    CullReason::Fog,
                );
            }
        }
    }

    // Hide enemy player buildings outside fog vision
    for (tf, mut vis, faction, is_culled, cull_reason) in enemy_buildings.iter_mut() {
        if is_culled {
            continue;
        }
        if teams.is_allied(&active_player.0, faction) {
            set_visibility_if_needed(
                &mut vis,
                cull_reason,
                Visibility::Inherited,
                CullReason::Visible,
            );
        } else {
            let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
            if v >= fog_settings.mob_threshold {
                set_visibility_if_needed(
                    &mut vis,
                    cull_reason,
                    Visibility::Inherited,
                    CullReason::Visible,
                );
            } else {
                set_visibility_if_needed(
                    &mut vis,
                    cull_reason,
                    Visibility::Hidden,
                    CullReason::Fog,
                );
            }
        }
    }
}

// use bevy::asset::RenderAssetUsages;
// use bevy::image::ImageSampler;
// use bevy::light::{NotShadowCaster, NotShadowReceiver};
// use bevy::mesh::{Indices, PrimitiveTopology};
// use bevy::prelude::*;
// use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

// use crate::types::*;
// use crate::presentation::materials::fog::{FogOfWarMaterial, FogSettings};
// use crate::world::ground::HeightMap;

// // ── Resources ──

// const FOG_OVERLAY_VERTEX_STRIDE: usize = 4;

// /// Handles to the two GPU textures (visible + explored).
// #[derive(Resource)]
// pub struct FogTextures {
//     pub visible: Handle<Image>,
//     pub explored: Handle<Image>,
// }

// #[derive(Resource, Default)]
// struct FogTextureUploadState {
//     explored_dirty: bool,
// }

// /// Controls fog update frequency. Heavy systems only run on tick frames.
// #[derive(Resource)]
// struct FogTickTimer {
//     timer: Timer,
//     ticked_this_frame: bool,
//     effective_tick_rate_hz: f32,
//     effective_los_ray_count: usize,
// }

// impl Default for FogTickTimer {
//     fn default() -> Self {
//         Self {
//             timer: Timer::from_seconds(1.0 / 12.0, TimerMode::Repeating),
//             ticked_this_frame: false,
//             effective_tick_rate_hz: 12.0,
//             effective_los_ray_count: 48,
//         }
//     }
// }

// /// Tweakable gameplay thresholds for fog of war.
// #[derive(Resource)]
// pub struct FogTweakSettings {
//     pub mob_threshold: f32,
//     pub object_threshold: f32,
//     pub vfx_threshold: f32,
//     pub transition_speed: f32,
//     pub reveal_all: bool,
//     pub enable_los: bool,
//     pub los_ray_count: usize,
//     // Performance toggles
//     pub enable_visibility_update: bool,
//     pub enable_display_lerp: bool,
//     pub enable_texture_upload: bool,
//     pub enable_entity_hiding: bool,
//     pub tick_rate_hz: f32,
//     pub shader_quality: f32,
// }

// impl Default for FogTweakSettings {
//     fn default() -> Self {
//         Self {
//             mob_threshold: 0.8,
//             object_threshold: 0.4,
//             vfx_threshold: 0.3,
//             transition_speed: 4.0,
//             reveal_all: false,
//             enable_los: true,
//             los_ray_count: 48,
//             enable_visibility_update: true,
//             enable_display_lerp: true,
//             enable_texture_upload: true,
//             enable_entity_hiding: true,
//             tick_rate_hz: 12.0,
//             shader_quality: 2.0,
//         }
//     }
// }

// // ── Plugin ──

// pub struct FogPlugin;

// impl Plugin for FogPlugin {
//     fn build(&self, app: &mut App) {
//         app.add_plugins(MaterialPlugin::<FogOfWarMaterial>::default())
//             .init_resource::<FogTweakSettings>()
//             .init_resource::<FogTickTimer>()
//             .add_systems(
//                 OnEnter(AppState::InGame),
//                 (spawn_fog_overlay, register_fog_tweaks).after(crate::world::ground::spawn_ground),
//             )
//             .add_systems(
//                 Update,
//                 (
//                     tick_fog_timer,
//                     update_fog_overlay_visibility,
//                     update_fog_visibility,
//                     update_fog_display,
//                     update_fog_textures,
//                     update_fog_material_time,
//                     fog_hide_entities,
//                 )
//                     .chain()
//                     .after(crate::world::culling::CullingSet)
//                     .run_if(in_state(AppState::InGame)),
//             );
//     }
// }

// // ── Debug Tweaks Registration ──

// fn register_fog_tweaks(mut tweaks: ResMut<crate::infrastructure::debug::DebugTweaks>) {
//     let s = FogSettings::default();

//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Noise Scale",
//         s.noise_scale,
//         0.0,
//         30.0,
//         0.5,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Edge Glow Width",
//         s.edge_glow_width,
//         0.0,
//         0.5,
//         0.01,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Edge Glow Intensity",
//         s.edge_glow_intensity,
//         0.0,
//         2.0,
//         0.05,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Fog Color R",
//         s.fog_color.x,
//         0.0,
//         1.0,
//         0.01,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Fog Color G",
//         s.fog_color.y,
//         0.0,
//         1.0,
//         0.01,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Fog Color B",
//         s.fog_color.z,
//         0.0,
//         1.0,
//         0.01,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Fog Color A",
//         s.fog_color.w,
//         0.0,
//         1.0,
//         0.01,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Glow Color R",
//         s.glow_color.x,
//         0.0,
//         1.0,
//         0.01,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Glow Color G",
//         s.glow_color.y,
//         0.0,
//         1.0,
//         0.01,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Glow Color B",
//         s.glow_color.z,
//         0.0,
//         1.0,
//         0.01,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Glow Color A",
//         s.glow_color.w,
//         0.0,
//         1.0,
//         0.01,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Explored Tint R",
//         s.explored_tint.x,
//         0.0,
//         1.0,
//         0.01,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Explored Tint G",
//         s.explored_tint.y,
//         0.0,
//         1.0,
//         0.01,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Explored Tint B",
//         s.explored_tint.z,
//         0.0,
//         1.0,
//         0.01,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Explored Tint A",
//         s.explored_tint.w,
//         0.0,
//         1.0,
//         0.01,
//     );

//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Scale",
//         s.fog_noise_scale,
//         1.0,
//         20.0,
//         0.5,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Speed",
//         s.fog_noise_speed,
//         0.0,
//         0.1,
//         0.005,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Warp",
//         s.fog_noise_warp,
//         0.0,
//         3.0,
//         0.1,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Contrast",
//         s.fog_noise_contrast,
//         0.0,
//         1.0,
//         0.05,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Octaves",
//         s.fog_noise_octaves,
//         1.0,
//         6.0,
//         1.0,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Tendril Scale",
//         s.fog_tendril_scale,
//         1.0,
//         20.0,
//         0.5,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Tendril Strength",
//         s.fog_tendril_strength,
//         0.0,
//         2.0,
//         0.05,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Shader",
//         "Warp Speed",
//         s.fog_warp_speed,
//         0.0,
//         3.0,
//         0.1,
//     );

//     let t = FogTweakSettings::default();
//     tweaks.add_float(
//         "Visuals/FoW Gameplay",
//         "Mob Threshold",
//         t.mob_threshold,
//         0.0,
//         1.0,
//         0.05,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Gameplay",
//         "Object Threshold",
//         t.object_threshold,
//         0.0,
//         1.0,
//         0.05,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Gameplay",
//         "VFX Threshold",
//         t.vfx_threshold,
//         0.0,
//         1.0,
//         0.05,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Gameplay",
//         "Transition Speed",
//         t.transition_speed,
//         0.5,
//         20.0,
//         0.5,
//     );
//     tweaks.add_bool("Visuals/FoW Gameplay", "Reveal Full Map", t.reveal_all);
//     tweaks.add_bool("Visuals/FoW Gameplay", "Enable LOS", t.enable_los);
//     tweaks.add_float(
//         "Visuals/FoW Gameplay",
//         "LOS Ray Count",
//         t.los_ray_count as f32,
//         8.0,
//         128.0,
//         8.0,
//     );
//     tweaks.add_bool(
//         "Visuals/FoW Performance",
//         "Visibility Update",
//         t.enable_visibility_update,
//     );
//     tweaks.add_bool(
//         "Visuals/FoW Performance",
//         "Display Lerp",
//         t.enable_display_lerp,
//     );
//     tweaks.add_bool(
//         "Visuals/FoW Performance",
//         "Texture Upload",
//         t.enable_texture_upload,
//     );
//     tweaks.add_bool(
//         "Visuals/FoW Performance",
//         "Entity Hiding",
//         t.enable_entity_hiding,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Performance",
//         "Tick Rate Hz",
//         t.tick_rate_hz,
//         4.0,
//         60.0,
//         1.0,
//     );
//     tweaks.add_float(
//         "Visuals/FoW Performance",
//         "Shader Quality",
//         t.shader_quality,
//         0.0,
//         2.0,
//         1.0,
//     );
// }

// // ── Texture Creation ──

// fn create_r8_texture(images: &mut Assets<Image>, grid_size: usize) -> Handle<Image> {
//     let size = Extent3d {
//         width: grid_size as u32,
//         height: grid_size as u32,
//         depth_or_array_layers: 1,
//     };
//     let mut image = Image::new_fill(
//         size,
//         TextureDimension::D2,
//         &[0u8],
//         TextureFormat::R8Unorm,
//         RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
//     );
//     image.sampler = ImageSampler::linear();
//     image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
//     images.add(image)
// }

// // ── Spawn ──

// fn spawn_fog_overlay(
//     mut commands: Commands,
//     mut meshes: ResMut<Assets<Mesh>>,
//     mut fog_materials: ResMut<Assets<FogOfWarMaterial>>,
//     mut images: ResMut<Assets<Image>>,
//     height_map: Res<HeightMap>,
// ) {
//     // Fog data grid uses a coarser resolution than the terrain grid.
//     // GPU bilinear texture sampling smooths the visual result.
//     let fog_step = height_map.step * 2.0;
//     let fog_grid_size = ((height_map.map_size / fog_step).ceil() as usize + 1).min(256);

//     // Overlay mesh uses the terrain grid with its own vertex stride
//     let grid_size = height_map.grid_size;
//     let step = height_map.step;
//     let half_map = height_map.half_map;
//     let overlay_cells = (grid_size - 1).div_ceil(FOG_OVERLAY_VERTEX_STRIDE);
//     let overlay_grid_size = overlay_cells + 1;

//     let mut positions: Vec<[f32; 3]> =
//         Vec::with_capacity(overlay_grid_size * overlay_grid_size);
//     let mut normals: Vec<[f32; 3]> = Vec::with_capacity(overlay_grid_size * overlay_grid_size);
//     let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(overlay_grid_size * overlay_grid_size);

//     for iz in 0..overlay_grid_size {
//         for ix in 0..overlay_grid_size {
//             let src_ix = (ix * FOG_OVERLAY_VERTEX_STRIDE).min(grid_size - 1);
//             let src_iz = (iz * FOG_OVERLAY_VERTEX_STRIDE).min(grid_size - 1);
//             let x = -half_map + src_ix as f32 * step;
//             let z = -half_map + src_iz as f32 * step;
//             let y = height_map.sample(x, z) + 1.5;
//             positions.push([x, y, z]);
//             normals.push([0.0, 1.0, 0.0]);
//             uvs.push([
//                 src_ix as f32 / (grid_size - 1) as f32,
//                 src_iz as f32 / (grid_size - 1) as f32,
//             ]);
//         }
//     }

//     let mut indices: Vec<u32> =
//         Vec::with_capacity((overlay_grid_size - 1) * (overlay_grid_size - 1) * 6);
//     for iz in 0..(overlay_grid_size - 1) {
//         for ix in 0..(overlay_grid_size - 1) {
//             let tl = (iz * overlay_grid_size + ix) as u32;
//             let tr = tl + 1;
//             let bl = tl + overlay_grid_size as u32;
//             let br = bl + 1;
//             indices.push(tl);
//             indices.push(bl);
//             indices.push(tr);
//             indices.push(tr);
//             indices.push(bl);
//             indices.push(br);
//         }
//     }

//     let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
//     mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
//     mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
//     mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
//     mesh.insert_indices(Indices::U32(indices));

//     let vis_handle = create_r8_texture(&mut images, fog_grid_size);
//     let exp_handle = create_r8_texture(&mut images, fog_grid_size);

//     let material = fog_materials.add(FogOfWarMaterial {
//         settings: FogSettings::default(),
//         visible_texture: Some(vis_handle.clone()),
//         explored_texture: Some(exp_handle.clone()),
//     });

//     commands.spawn((
//         GameWorld,
//         FogOverlay,
//         Mesh3d(meshes.add(mesh)),
//         MeshMaterial3d(material),
//         Transform::default(),
//         NotShadowCaster,
//         NotShadowReceiver,
//     ));

//     commands.insert_resource(FogTextures {
//         visible: vis_handle,
//         explored: exp_handle,
//     });
//     commands.insert_resource(FogTextureUploadState {
//         explored_dirty: true,
//     });

//     let total = fog_grid_size * fog_grid_size;
//     commands.insert_resource(FogOfWarMap {
//         visible: vec![0.0; total],
//         visible_next: vec![0.0; total],
//         explored: vec![0; total],
//         display: vec![0.0; total],
//         grid_size: fog_grid_size,
//         step: fog_step,
//         half_map,
//     });
// }

// #[inline]
// fn reveal_fog_cell(fog_map: &mut FogOfWarMap, idx: usize, vis: f32, explored_dirty: &mut bool) {
//     if vis > fog_map.visible_next[idx] {
//         fog_map.visible_next[idx] = vis;
//     }
//     if fog_map.explored[idx] == 0 {
//         fog_map.explored[idx] = u8::MAX;
//         *explored_dirty = true;
//     }
// }

// // ── Tick Timer ──

// fn tick_fog_timer(
//     mut fog_timer: ResMut<FogTickTimer>,
//     fog_settings: Res<FogTweakSettings>,
//     time: Res<Time>,
//     height_map: Res<HeightMap>,
//     active_player: Res<ActivePlayer>,
//     teams: Res<TeamConfig>,
//     all_units: Query<(&Transform, &VisionRange, &Faction), With<Unit>>,
//     all_buildings: Query<(&Transform, &VisionRange, &Faction), With<Building>>,
// ) {
//     let active_faction = active_player.0;
//     let mut viewer_count = 0usize;
//     for (_, _, faction) in all_units.iter() {
//         if teams.is_allied(&active_faction, faction) {
//             viewer_count += 1;
//         }
//     }
//     for (_, _, faction) in all_buildings.iter() {
//         if teams.is_allied(&active_faction, faction) {
//             viewer_count += 1;
//         }
//     }

//     let mut effective_tick_rate_hz = fog_settings.tick_rate_hz;
//     let mut effective_los_ray_count = fog_settings.los_ray_count;
//     if height_map.map_size >= 700.0 || viewer_count >= 140 {
//         effective_tick_rate_hz = effective_tick_rate_hz.min(4.0);
//         effective_los_ray_count = effective_los_ray_count.min(16);
//     } else if height_map.map_size >= 500.0 || viewer_count >= 80 {
//         effective_tick_rate_hz = effective_tick_rate_hz.min(6.0);
//         effective_los_ray_count = effective_los_ray_count.min(24);
//     } else if viewer_count >= 40 {
//         effective_tick_rate_hz = effective_tick_rate_hz.min(8.0);
//         effective_los_ray_count = effective_los_ray_count.min(32);
//     }

//     fog_timer.effective_tick_rate_hz = effective_tick_rate_hz.max(1.0);
//     fog_timer.effective_los_ray_count = effective_los_ray_count.max(8);

//     // Sync tick rate from tweaks/adaptive profile
//     let desired_duration =
//         std::time::Duration::from_secs_f32(1.0 / fog_timer.effective_tick_rate_hz);
//     if fog_timer.timer.duration() != desired_duration {
//         fog_timer.timer.set_duration(desired_duration);
//     }

//     fog_timer.timer.tick(time.delta());
//     fog_timer.ticked_this_frame = fog_timer.timer.just_finished();
// }

// // ── Overlay Visibility ──

// fn update_fog_overlay_visibility(
//     fog_settings: Res<FogTweakSettings>,
//     mut fog_overlay: Query<&mut Visibility, With<FogOverlay>>,
// ) {
//     let should_hide = fog_settings.reveal_all
//         || (!fog_settings.enable_visibility_update
//             && !fog_settings.enable_display_lerp
//             && !fog_settings.enable_texture_upload);

//     if let Ok(mut vis) = fog_overlay.single_mut() {
//         let target = if should_hide {
//             Visibility::Hidden
//         } else {
//             Visibility::Inherited
//         };
//         if *vis != target {
//             *vis = target;
//         }
//     }
// }

// // ── Visibility Update (with terrain LOS) ──

// /// Maximum number of frames over which to spread viewer raycasting.
// const FOG_AMORTIZE_FRAMES: usize = 6;

// fn update_fog_visibility(
//     mut fog_map: ResMut<FogOfWarMap>,
//     fog_settings: Res<FogTweakSettings>,
//     fog_timer: Res<FogTickTimer>,
//     mut upload_state: ResMut<FogTextureUploadState>,
//     height_map: Res<HeightMap>,
//     active_player: Res<ActivePlayer>,
//     teams: Res<TeamConfig>,
//     all_units: Query<(&Transform, &VisionRange, &Faction), With<Unit>>,
//     all_buildings: Query<(&Transform, &VisionRange, &Faction), With<Building>>,
//     mut viewers: Local<Vec<(Vec3, f32)>>,
//     mut chunk_offset: Local<usize>,
//     mut cycle_total: Local<usize>,
// ) {
//     if !fog_settings.enable_visibility_update {
//         return;
//     }

//     if fog_settings.reveal_all {
//         if fog_timer.ticked_this_frame {
//             fog_map.visible.fill(1.0);
//             if fog_map.explored.iter().any(|&v| v == 0) {
//                 fog_map.explored.fill(u8::MAX);
//                 upload_state.explored_dirty = true;
//             }
//         }
//         return;
//     }

//     // On fog tick: snapshot viewers and start a new amortization cycle.
//     if fog_timer.ticked_this_frame {
//         let active_faction = active_player.0;
//         viewers.clear();
//         viewers.reserve(all_units.iter().len() + all_buildings.iter().len());
//         for (tf, vr, faction) in all_units.iter() {
//             if teams.is_allied(&active_faction, faction) {
//                 viewers.push((tf.translation, vr.0));
//             }
//         }
//         for (tf, vr, faction) in all_buildings.iter() {
//             if teams.is_allied(&active_faction, faction) {
//                 viewers.push((tf.translation, vr.0));
//             }
//         }

//         // Clear back-buffer at the start of a new cycle.
//         fog_map.visible_next.fill(0.0);
//         *chunk_offset = 0;
//         *cycle_total = viewers.len();
//     }

//     // Nothing to process if no viewers or cycle already finished.
//     if viewers.is_empty() || *chunk_offset >= *cycle_total {
//         return;
//     }

//     // Determine this frame's viewer chunk.
//     let remaining = *cycle_total - *chunk_offset;
//     let frames_left = FOG_AMORTIZE_FRAMES.max(1);
//     let chunk_size = (remaining + frames_left - 1) / frames_left; // ceil division
//     let chunk_start = *chunk_offset;
//     let chunk_end = (chunk_start + chunk_size).min(*cycle_total);
//     *chunk_offset = chunk_end;

//     let cycle_complete = chunk_end >= *cycle_total;

//     let grid_size = fog_map.grid_size;
//     let step = fog_map.step;
//     let half_map = fog_map.half_map;
//     let mut explored_dirty = false;

//     let enable_los = fog_settings.enable_los;
//     let ray_count = fog_timer.effective_los_ray_count;
//     let terrain_heights = &height_map.heights;
//     let terrain_step = height_map.step;
//     let terrain_grid_size = height_map.grid_size;

//     for (pos, range) in &viewers[chunk_start..chunk_end] {
//         let range_sq = range * range;
//         let viewer_height = pos.y + 2.0;

//         let min_x = ((pos.x - range + half_map) / step).floor().max(0.0) as usize;
//         let max_x = ((pos.x + range + half_map) / step)
//             .ceil()
//             .min((grid_size - 1) as f32) as usize;
//         let min_z = ((pos.z - range + half_map) / step).floor().max(0.0) as usize;
//         let max_z = ((pos.z + range + half_map) / step)
//             .ceil()
//             .min((grid_size - 1) as f32) as usize;

//         if enable_los {
//             let max_steps = (*range / step).ceil() as usize + 1;

//             for ray_i in 0..ray_count {
//                 let angle = std::f32::consts::TAU * ray_i as f32 / ray_count as f32;
//                 let dir_x = angle.cos();
//                 let dir_z = angle.sin();

//                 let mut max_angle = f32::NEG_INFINITY;

//                 for s in 1..=max_steps {
//                     let dist = s as f32 * step;
//                     if dist * dist > range_sq {
//                         break;
//                     }

//                     let wx = pos.x + dir_x * dist;
//                     let wz = pos.z + dir_z * dist;

//                     let fix = ((wx + half_map) / step).round();
//                     let fiz = ((wz + half_map) / step).round();
//                     if fix < 0.0 || fiz < 0.0 {
//                         continue;
//                     }
//                     let ix = fix as usize;
//                     let iz = fiz as usize;
//                     if ix >= grid_size || iz >= grid_size {
//                         break;
//                     }

//                     let tix = ((wx + half_map) / terrain_step)
//                         .round()
//                         .clamp(0.0, (terrain_grid_size - 1) as f32)
//                         as usize;
//                     let tiz = ((wz + half_map) / terrain_step)
//                         .round()
//                         .clamp(0.0, (terrain_grid_size - 1) as f32)
//                         as usize;
//                     let terrain_h = terrain_heights[tiz * terrain_grid_size + tix];
//                     let elevation_angle = (terrain_h - viewer_height) / dist;

//                     if elevation_angle > max_angle {
//                         max_angle = elevation_angle;

//                         let t = dist / range;
//                         let edge_fade = 1.0 - t * t;
//                         let vis = 0.5 + 0.5 * edge_fade;
//                         let fog_idx = iz * grid_size + ix;
//                         reveal_fog_cell(&mut fog_map, fog_idx, vis, &mut explored_dirty);
//                     }
//                 }
//             }

//             let vix = ((pos.x + half_map) / step).round() as usize;
//             let viz = ((pos.z + half_map) / step).round() as usize;
//             if vix < grid_size && viz < grid_size {
//                 reveal_fog_cell(
//                     &mut fog_map,
//                     viz * grid_size + vix,
//                     1.0,
//                     &mut explored_dirty,
//                 );
//             }
//         } else {
//             for iz in min_z..=max_z {
//                 for ix in min_x..=max_x {
//                     let wx = -half_map + ix as f32 * step;
//                     let wz = -half_map + iz as f32 * step;
//                     let dx = wx - pos.x;
//                     let dz = wz - pos.z;
//                     let dist_sq = dx * dx + dz * dz;

//                     if dist_sq <= range_sq {
//                         let t = (dist_sq / range_sq).sqrt();
//                         let edge_fade = 1.0 - t * t;
//                         let vis = 0.5 + 0.5 * edge_fade;
//                         let idx = iz * grid_size + ix;
//                         reveal_fog_cell(&mut fog_map, idx, vis, &mut explored_dirty);
//                     }
//                 }
//             }
//         }
//     }
//     if explored_dirty {
//         upload_state.explored_dirty = true;
//     }

//     // When the cycle is complete, swap back-buffer into visible so readers
//     // always see a full, consistent visibility frame (no partial flickering).
//     if cycle_complete {
//         let next_visible = std::mem::take(&mut fog_map.visible_next);
//         fog_map.visible_next = std::mem::replace(&mut fog_map.visible, next_visible);
//     }
// }

// // ── Smooth Display Interpolation ──

// fn update_fog_display(
//     mut fog_map: ResMut<FogOfWarMap>,
//     fog_settings: Res<FogTweakSettings>,
//     time: Res<Time>,
// ) {
//     if !fog_settings.enable_display_lerp {
//         return;
//     }

//     if fog_settings.reveal_all {
//         for v in fog_map.display.iter_mut() {
//             *v = 1.0;
//         }
//         return;
//     }

//     // Run every frame for smooth transitions (cheap: ~0.05ms for 65K cells).
//     let dt = time.delta_secs();
//     let speed = fog_settings.transition_speed;
//     let lerp_factor = (speed * dt).min(1.0);

//     for i in 0..fog_map.visible.len() {
//         let target = if fog_map.visible[i] > 0.01 {
//             fog_map.visible[i]
//         } else if fog_map.explored[i] != 0 {
//             0.35
//         } else {
//             0.0
//         };

//         let current = fog_map.display[i];
//         fog_map.display[i] = current + (target - current) * lerp_factor;
//     }
// }

// // ── Texture Upload ──

// fn update_fog_textures(
//     fog_map: Res<FogOfWarMap>,
//     fog_tex: Res<FogTextures>,
//     fog_settings: Res<FogTweakSettings>,
//     fog_timer: Res<FogTickTimer>,
//     mut upload_state: ResMut<FogTextureUploadState>,
//     mut images: ResMut<Assets<Image>>,
// ) {
//     if !fog_settings.enable_texture_upload || !fog_timer.ticked_this_frame {
//         return;
//     }

//     // Upload visible layer (smooth display values)
//     if let Some(image) = images.get_mut(&fog_tex.visible) {
//         if let Some(ref mut data) = image.data {
//             for (dst, src) in data.iter_mut().zip(fog_map.display.iter()) {
//                 *dst = (src.clamp(0.0, 1.0) * 255.0) as u8;
//             }
//         }
//     }

//     // Upload explored layer only when new cells are discovered.
//     if upload_state.explored_dirty {
//         if let Some(image) = images.get_mut(&fog_tex.explored) {
//             if let Some(ref mut data) = image.data {
//                 data[..fog_map.explored.len()].copy_from_slice(&fog_map.explored);
//             }
//         }
//         upload_state.explored_dirty = false;
//     }
// }

// // ── Shader Time Update ──
// // Owns "Visuals/FoW Shader" folder (shader + noise params). Gameplay params
// // ("Visuals/FoW Gameplay") are synced in debug.rs::sync_fog_tweaks.

// fn update_fog_material_time(
//     time: Res<Time>,
//     tweaks: Res<crate::infrastructure::debug::DebugTweaks>,
//     fog_settings: Res<FogTweakSettings>,
//     fog_overlay: Query<&MeshMaterial3d<FogOfWarMaterial>, With<FogOverlay>>,
//     mut materials: ResMut<Assets<FogOfWarMaterial>>,
// ) {
//     let Ok(mat_handle) = fog_overlay.single() else {
//         return;
//     };
//     let Some(mat) = materials.get_mut(&mat_handle.0) else {
//         return;
//     };
//     mat.settings.time = time.elapsed_secs();

//     // Apply shader tweaks
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Noise Scale") {
//         mat.settings.noise_scale = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Edge Glow Width") {
//         mat.settings.edge_glow_width = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Edge Glow Intensity") {
//         mat.settings.edge_glow_intensity = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Fog Color R") {
//         mat.settings.fog_color.x = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Fog Color G") {
//         mat.settings.fog_color.y = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Fog Color B") {
//         mat.settings.fog_color.z = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Fog Color A") {
//         mat.settings.fog_color.w = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Glow Color R") {
//         mat.settings.glow_color.x = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Glow Color G") {
//         mat.settings.glow_color.y = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Glow Color B") {
//         mat.settings.glow_color.z = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Glow Color A") {
//         mat.settings.glow_color.w = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Explored Tint R") {
//         mat.settings.explored_tint.x = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Explored Tint G") {
//         mat.settings.explored_tint.y = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Explored Tint B") {
//         mat.settings.explored_tint.z = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Explored Tint A") {
//         mat.settings.explored_tint.w = v;
//     }

//     // Apply fog noise tweaks
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Scale") {
//         mat.settings.fog_noise_scale = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Speed") {
//         mat.settings.fog_noise_speed = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Warp") {
//         mat.settings.fog_noise_warp = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Contrast") {
//         mat.settings.fog_noise_contrast = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Octaves") {
//         mat.settings.fog_noise_octaves = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Tendril Scale") {
//         mat.settings.fog_tendril_scale = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Tendril Strength") {
//         mat.settings.fog_tendril_strength = v;
//     }
//     if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Warp Speed") {
//         mat.settings.fog_warp_speed = v;
//     }

//     // Shader quality from performance tweaks
//     mat.settings.quality_level = fog_settings.shader_quality;
// }

// // ── Unified Entity Hiding ──

// #[inline]
// fn set_visibility_if_needed(
//     vis: &mut Visibility,
//     cull_reason: Option<Mut<CullReason>>,
//     target: Visibility,
//     reason: CullReason,
// ) {
//     if *vis != target {
//         *vis = target;
//     }
//     if let Some(mut current_reason) = cull_reason {
//         if *current_reason != reason {
//             *current_reason = reason;
//         }
//     }
// }

// fn fog_hide_entities(
//     fog_map: Res<FogOfWarMap>,
//     fog_settings: Res<FogTweakSettings>,
//     fog_timer: Res<FogTickTimer>,
//     active_player: Res<ActivePlayer>,
//     teams: Res<TeamConfig>,
//     mut ran_once: Local<bool>,
//     mut hideables: Query<(
//         &Transform,
//         &mut Visibility,
//         &FogHideable,
//         Has<FrustumCulled>,
//         Option<&mut CullReason>,
//     )>,
//     mut enemy_units: Query<
//         (
//             &Transform,
//             &mut Visibility,
//             &Faction,
//             &UnitState,
//             Has<FrustumCulled>,
//             Option<&mut CullReason>,
//         ),
//         (With<Unit>, Without<FogHideable>),
//     >,
//     mut enemy_buildings: Query<
//         (
//             &Transform,
//             &mut Visibility,
//             &Faction,
//             Has<FrustumCulled>,
//             Option<&mut CullReason>,
//         ),
//         (With<Building>, Without<FogHideable>, Without<Unit>),
//     >,
// ) {
//     if !fog_settings.enable_entity_hiding {
//         return;
//     }

//     // Only run on fog tick frames (fog data doesn't change between ticks).
//     // Ensure at least one initial run so entities start hidden.
//     if *ran_once && !fog_timer.ticked_this_frame {
//         return;
//     }
//     *ran_once = true;

//     if fog_settings.reveal_all {
//         // When fog is disabled, restore visibility — but skip frustum-culled entities
//         // so we don't override the culling system's Visibility::Hidden.
//         for (_, mut vis, _, is_culled, cull_reason) in hideables.iter_mut() {
//             if !is_culled {
//                 set_visibility_if_needed(
//                     &mut vis,
//                     cull_reason,
//                     Visibility::Inherited,
//                     CullReason::Visible,
//                 );
//             }
//         }
//         for (_, mut vis, _, _, is_culled, cull_reason) in enemy_units.iter_mut() {
//             if !is_culled {
//                 set_visibility_if_needed(
//                     &mut vis,
//                     cull_reason,
//                     Visibility::Inherited,
//                     CullReason::Visible,
//                 );
//             }
//         }
//         for (_, mut vis, _, is_culled, cull_reason) in enemy_buildings.iter_mut() {
//             if !is_culled {
//                 set_visibility_if_needed(
//                     &mut vis,
//                     cull_reason,
//                     Visibility::Inherited,
//                     CullReason::Visible,
//                 );
//             }
//         }
//         return;
//     }

//     // FogHideable logic (mobs, objects, decorations, mountains, vfx)
//     for (tf, mut vis, hideable, is_culled, cull_reason) in hideables.iter_mut() {
//         // Frustum-culled entities are already hidden by culling — skip them.
//         if is_culled {
//             continue;
//         }

//         let threshold = match hideable {
//             FogHideable::Mob => fog_settings.mob_threshold,
//             FogHideable::Object => fog_settings.object_threshold,
//             FogHideable::Vfx => fog_settings.vfx_threshold,
//         };

//         let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
//         if v >= threshold {
//             set_visibility_if_needed(&mut vis, cull_reason, Visibility::Inherited, CullReason::Visible);
//         } else {
//             set_visibility_if_needed(&mut vis, cull_reason, Visibility::Hidden, CullReason::Fog);
//         }
//     }

//     // Hide enemy player units outside fog vision
//     for (tf, mut vis, faction, _unit_state, is_culled, cull_reason) in enemy_units.iter_mut() {
//         if is_culled {
//             continue;
//         }
//         if teams.is_allied(&active_player.0, faction) {
//             set_visibility_if_needed(&mut vis, cull_reason, Visibility::Inherited, CullReason::Visible);
//         } else {
//             let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
//             if v >= fog_settings.mob_threshold {
//                 set_visibility_if_needed(&mut vis, cull_reason, Visibility::Inherited, CullReason::Visible);
//             } else {
//                 set_visibility_if_needed(&mut vis, cull_reason, Visibility::Hidden, CullReason::Fog);
//             }
//         }
//     }

//     // Hide enemy player buildings outside fog vision
//     for (tf, mut vis, faction, is_culled, cull_reason) in enemy_buildings.iter_mut() {
//         if is_culled {
//             continue;
//         }
//         if teams.is_allied(&active_player.0, faction) {
//             set_visibility_if_needed(&mut vis, cull_reason, Visibility::Inherited, CullReason::Visible);
//         } else {
//             let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
//             if v >= fog_settings.mob_threshold {
//                 set_visibility_if_needed(&mut vis, cull_reason, Visibility::Inherited, CullReason::Visible);
//             } else {
//                 set_visibility_if_needed(&mut vis, cull_reason, Visibility::Hidden, CullReason::Fog);
//             }
//         }
//     }
// }
