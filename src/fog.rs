use bevy::image::ImageSampler;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

use crate::components::*;
use crate::fog_material::{FogOfWarMaterial, FogSettings};
use crate::ground::{HeightMap, GRID_SIZE, HALF_MAP, MAP_SIZE};

// ── Resources ──

/// Handles to the two GPU textures (visible + explored).
#[derive(Resource)]
pub struct FogTextures {
    pub visible: Handle<Image>,
    pub explored: Handle<Image>,
}

/// Tweakable gameplay thresholds for fog of war.
#[derive(Resource)]
pub struct FogTweakSettings {
    pub mob_threshold: f32,
    pub object_threshold: f32,
    pub vfx_threshold: f32,
    pub transition_speed: f32,
    pub enable_los: bool,
    pub los_ray_count: usize,
}

impl Default for FogTweakSettings {
    fn default() -> Self {
        Self {
            mob_threshold: 0.8,
            object_threshold: 0.4,
            vfx_threshold: 0.3,
            transition_speed: 4.0,
            enable_los: true,
            los_ray_count: 48,
        }
    }
}

// ── Plugin ──

pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<FogOfWarMaterial>::default())
            .init_resource::<FogTweakSettings>()
            .add_systems(PostStartup, (spawn_fog_overlay, register_fog_tweaks))
            .add_systems(
                Update,
                (
                    update_fog_visibility,
                    update_fog_display,
                    update_fog_textures,
                    update_fog_material_time,
                    fog_hide_entities,
                )
                    .chain(),
            );
    }
}

// ── Debug Tweaks Registration ──

fn register_fog_tweaks(mut tweaks: ResMut<crate::debug::DebugTweaks>) {
    let s = FogSettings::default();

    tweaks.add_float("Visuals/FoW Shader", "Noise Scale", s.noise_scale, 0.0, 30.0, 0.5);
    tweaks.add_float("Visuals/FoW Shader", "Edge Glow Width", s.edge_glow_width, 0.0, 0.5, 0.01);
    tweaks.add_float("Visuals/FoW Shader", "Edge Glow Intensity", s.edge_glow_intensity, 0.0, 2.0, 0.05);
    tweaks.add_float("Visuals/FoW Shader", "Fog Color R", s.fog_color.x, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/FoW Shader", "Fog Color G", s.fog_color.y, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/FoW Shader", "Fog Color B", s.fog_color.z, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/FoW Shader", "Fog Color A", s.fog_color.w, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/FoW Shader", "Glow Color R", s.glow_color.x, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/FoW Shader", "Glow Color G", s.glow_color.y, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/FoW Shader", "Glow Color B", s.glow_color.z, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/FoW Shader", "Glow Color A", s.glow_color.w, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/FoW Shader", "Explored Tint R", s.explored_tint.x, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/FoW Shader", "Explored Tint G", s.explored_tint.y, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/FoW Shader", "Explored Tint B", s.explored_tint.z, 0.0, 1.0, 0.01);
    tweaks.add_float("Visuals/FoW Shader", "Explored Tint A", s.explored_tint.w, 0.0, 1.0, 0.01);

    // Unexplored fog noise pattern
    tweaks.add_float("Visuals/FoW Fog Noise", "Scale", s.fog_noise_scale, 1.0, 20.0, 0.5);
    tweaks.add_float("Visuals/FoW Fog Noise", "Speed", s.fog_noise_speed, 0.0, 0.1, 0.005);
    tweaks.add_float("Visuals/FoW Fog Noise", "Warp", s.fog_noise_warp, 0.0, 3.0, 0.1);
    tweaks.add_float("Visuals/FoW Fog Noise", "Contrast", s.fog_noise_contrast, 0.0, 1.0, 0.05);
    tweaks.add_float("Visuals/FoW Fog Noise", "Octaves", s.fog_noise_octaves, 1.0, 6.0, 1.0);
    tweaks.add_float("Visuals/FoW Fog Noise", "Tendril Scale", s.fog_tendril_scale, 1.0, 20.0, 0.5);
    tweaks.add_float("Visuals/FoW Fog Noise", "Tendril Strength", s.fog_tendril_strength, 0.0, 2.0, 0.05);
    tweaks.add_float("Visuals/FoW Fog Noise", "Warp Speed", s.fog_warp_speed, 0.0, 3.0, 0.1);

    let t = FogTweakSettings::default();
    tweaks.add_float("Visuals/FoW Gameplay", "Mob Threshold", t.mob_threshold, 0.0, 1.0, 0.05);
    tweaks.add_float("Visuals/FoW Gameplay", "Object Threshold", t.object_threshold, 0.0, 1.0, 0.05);
    tweaks.add_float("Visuals/FoW Gameplay", "VFX Threshold", t.vfx_threshold, 0.0, 1.0, 0.05);
    tweaks.add_float("Visuals/FoW Gameplay", "Transition Speed", t.transition_speed, 0.5, 20.0, 0.5);
    tweaks.add_float("Visuals/FoW Gameplay", "Enable LOS", if t.enable_los { 1.0 } else { 0.0 }, 0.0, 1.0, 1.0);
    tweaks.add_float("Visuals/FoW Gameplay", "LOS Ray Count", t.los_ray_count as f32, 8.0, 128.0, 8.0);
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
    let grid_size = GRID_SIZE;
    let step = MAP_SIZE / (grid_size - 1) as f32;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(grid_size * grid_size);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(grid_size * grid_size);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(grid_size * grid_size);

    for iz in 0..grid_size {
        for ix in 0..grid_size {
            let x = -HALF_MAP + ix as f32 * step;
            let z = -HALF_MAP + iz as f32 * step;
            let y = height_map.sample(x, z) + 1.5;
            positions.push([x, y, z]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([
                ix as f32 / (grid_size - 1) as f32,
                iz as f32 / (grid_size - 1) as f32,
            ]);
        }
    }

    let mut indices: Vec<u32> = Vec::with_capacity((grid_size - 1) * (grid_size - 1) * 6);
    for iz in 0..(grid_size - 1) {
        for ix in 0..(grid_size - 1) {
            let tl = (iz * grid_size + ix) as u32;
            let tr = tl + 1;
            let bl = tl + grid_size as u32;
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

    let vis_handle = create_r8_texture(&mut images, grid_size);
    let exp_handle = create_r8_texture(&mut images, grid_size);

    let material = fog_materials.add(FogOfWarMaterial {
        settings: FogSettings::default(),
        visible_texture: Some(vis_handle.clone()),
        explored_texture: Some(exp_handle.clone()),
    });

    commands.spawn((
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

    let total = grid_size * grid_size;
    commands.insert_resource(FogOfWarMap {
        visible: vec![0.0; total],
        explored: vec![false; total],
        display: vec![0.0; total],
        grid_size,
        map_size: MAP_SIZE,
    });
}

// ── Visibility Update (with terrain LOS) ──

fn update_fog_visibility(
    mut fog_map: ResMut<FogOfWarMap>,
    fog_settings: Res<FogTweakSettings>,
    height_map: Res<HeightMap>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    all_units: Query<(&Transform, &VisionRange, &Faction), With<Unit>>,
    all_buildings: Query<(&Transform, &VisionRange, &Faction), With<Building>>,
) {
    let grid_size = fog_map.grid_size;
    let step = fog_map.map_size / (grid_size - 1) as f32;

    // Clear visible layer each frame
    for v in fog_map.visible.iter_mut() {
        *v = 0.0;
    }

    // Collect viewers for the active player's team (own + allied factions)
    let active_faction = active_player.0;
    let mut viewers: Vec<(Vec3, f32)> = Vec::new();
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

    for (pos, range) in &viewers {
        let range_sq = range * range;
        let viewer_height = pos.y + 2.0; // eye height above ground

        let min_x = ((pos.x - range + HALF_MAP) / step).floor().max(0.0) as usize;
        let max_x = ((pos.x + range + HALF_MAP) / step).ceil().min((grid_size - 1) as f32) as usize;
        let min_z = ((pos.z - range + HALF_MAP) / step).floor().max(0.0) as usize;
        let max_z = ((pos.z + range + HALF_MAP) / step).ceil().min((grid_size - 1) as f32) as usize;

        if enable_los {
            // Terrain-aware LOS using elevation angle raycasting.
            // For each angular sector, cast a ray outward tracking max elevation angle.
            // Cells whose angle is below the running max are occluded by terrain.
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

                    // Convert to grid indices
                    let fix = ((wx + HALF_MAP) / step).round();
                    let fiz = ((wz + HALF_MAP) / step).round();
                    if fix < 0.0 || fiz < 0.0 {
                        continue;
                    }
                    let ix = fix as usize;
                    let iz = fiz as usize;
                    if ix >= grid_size || iz >= grid_size {
                        break;
                    }

                    let terrain_h = height_map.sample(wx, wz);
                    let elevation_angle = (terrain_h - viewer_height) / dist;

                    if elevation_angle > max_angle {
                        max_angle = elevation_angle;

                        // This cell is visible — compute edge fade
                        let t = dist / range;
                        let edge_fade = 1.0 - t * t;
                        let vis = 0.5 + 0.5 * edge_fade;

                        let idx = iz * grid_size + ix;
                        if vis > fog_map.visible[idx] {
                            fog_map.visible[idx] = vis;
                        }
                    }
                    // If angle <= max_angle, terrain occludes this cell — skip it
                }
            }

            // Also mark the viewer's own cell as fully visible
            let vix = ((pos.x + HALF_MAP) / step).round() as usize;
            let viz = ((pos.z + HALF_MAP) / step).round() as usize;
            if vix < grid_size && viz < grid_size {
                fog_map.visible[viz * grid_size + vix] = 1.0;
            }
        } else {
            // Simple radial distance (no terrain occlusion) — original behavior
            for iz in min_z..=max_z {
                for ix in min_x..=max_x {
                    let wx = -HALF_MAP + ix as f32 * step;
                    let wz = -HALF_MAP + iz as f32 * step;
                    let dx = wx - pos.x;
                    let dz = wz - pos.z;
                    let dist_sq = dx * dx + dz * dz;

                    if dist_sq <= range_sq {
                        let t = (dist_sq / range_sq).sqrt();
                        let edge_fade = 1.0 - t * t;
                        let vis = 0.5 + 0.5 * edge_fade;
                        let idx = iz * grid_size + ix;
                        if vis > fog_map.visible[idx] {
                            fog_map.visible[idx] = vis;
                        }
                    }
                }
            }
        }
    }

    // Update explored layer (permanent, write-once)
    for i in 0..fog_map.visible.len() {
        if fog_map.visible[i] > 0.01 {
            fog_map.explored[i] = true;
        }
    }
}

// ── Smooth Display Interpolation ──

fn update_fog_display(
    mut fog_map: ResMut<FogOfWarMap>,
    fog_settings: Res<FogTweakSettings>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let speed = fog_settings.transition_speed;
    let lerp_factor = (speed * dt).min(1.0);

    for i in 0..fog_map.visible.len() {
        let target = if fog_map.visible[i] > 0.01 {
            // Currently visible: use the raw visible value (0.5–1.0 range)
            fog_map.visible[i]
        } else if fog_map.explored[i] {
            // Explored but not currently visible
            0.35
        } else {
            // Never seen
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
    mut images: ResMut<Assets<Image>>,
) {
    let total = fog_map.grid_size * fog_map.grid_size;

    // Upload visible layer (smooth display values)
    if let Some(image) = images.get_mut(&fog_tex.visible) {
        if let Some(ref mut data) = image.data {
            for i in 0..total {
                data[i] = (fog_map.display[i].clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }

    // Upload explored layer
    if let Some(image) = images.get_mut(&fog_tex.explored) {
        if let Some(ref mut data) = image.data {
            for i in 0..total {
                data[i] = if fog_map.explored[i] { 255 } else { 0 };
            }
        }
    }
}

// ── Shader Time Update ──

fn update_fog_material_time(
    time: Res<Time>,
    tweaks: Res<crate::debug::DebugTweaks>,
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
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Noise Scale") { mat.settings.noise_scale = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Edge Glow Width") { mat.settings.edge_glow_width = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Edge Glow Intensity") { mat.settings.edge_glow_intensity = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Fog Color R") { mat.settings.fog_color.x = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Fog Color G") { mat.settings.fog_color.y = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Fog Color B") { mat.settings.fog_color.z = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Fog Color A") { mat.settings.fog_color.w = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Glow Color R") { mat.settings.glow_color.x = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Glow Color G") { mat.settings.glow_color.y = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Glow Color B") { mat.settings.glow_color.z = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Glow Color A") { mat.settings.glow_color.w = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Explored Tint R") { mat.settings.explored_tint.x = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Explored Tint G") { mat.settings.explored_tint.y = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Explored Tint B") { mat.settings.explored_tint.z = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Shader", "Explored Tint A") { mat.settings.explored_tint.w = v; }

    // Apply fog noise tweaks
    if let Some(v) = tweaks.get_float("Visuals/FoW Fog Noise", "Scale") { mat.settings.fog_noise_scale = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Fog Noise", "Speed") { mat.settings.fog_noise_speed = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Fog Noise", "Warp") { mat.settings.fog_noise_warp = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Fog Noise", "Contrast") { mat.settings.fog_noise_contrast = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Fog Noise", "Octaves") { mat.settings.fog_noise_octaves = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Fog Noise", "Tendril Scale") { mat.settings.fog_tendril_scale = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Fog Noise", "Tendril Strength") { mat.settings.fog_tendril_strength = v; }
    if let Some(v) = tweaks.get_float("Visuals/FoW Fog Noise", "Warp Speed") { mat.settings.fog_warp_speed = v; }
}

// ── Unified Entity Hiding ──

fn fog_hide_entities(
    fog_map: Res<FogOfWarMap>,
    fog_settings: Res<FogTweakSettings>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    mut hideables: Query<(&Transform, &mut Visibility, &FogHideable)>,
    mut enemy_units: Query<
        (&Transform, &mut Visibility, &Faction, &UnitState),
        (With<Unit>, Without<FogHideable>),
    >,
    mut enemy_buildings: Query<
        (&Transform, &mut Visibility, &Faction),
        (With<Building>, Without<FogHideable>, Without<Unit>),
    >,
) {
    // Original FogHideable logic (mobs, objects, vfx)
    for (tf, mut vis, hideable) in hideables.iter_mut() {
        let threshold = match hideable {
            FogHideable::Mob => fog_settings.mob_threshold,
            FogHideable::Object => fog_settings.object_threshold,
            FogHideable::Vfx => fog_settings.vfx_threshold,
        };

        let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
        *vis = if v >= threshold {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    // Hide enemy player units outside fog vision (skip workers inside processors)
    for (tf, mut vis, faction, unit_state) in enemy_units.iter_mut() {
        if matches!(unit_state, UnitState::InsideProcessor(_)) {
            *vis = Visibility::Hidden;
            continue;
        }
        if teams.is_allied(&active_player.0, faction) {
            *vis = Visibility::Inherited;
        } else {
            let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
            *vis = if v >= fog_settings.mob_threshold {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }

    // Hide enemy player buildings outside fog vision
    for (tf, mut vis, faction) in enemy_buildings.iter_mut() {
        if teams.is_allied(&active_player.0, faction) {
            *vis = Visibility::Inherited;
        } else {
            let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
            *vis = if v >= fog_settings.mob_threshold {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}
