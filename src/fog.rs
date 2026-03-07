use bevy::image::ImageSampler;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

use crate::components::*;
use crate::fog_material::{FogOfWarMaterial, FogSettings};
use crate::ground::{terrain_height, GRID_SIZE, HALF_MAP, MAP_SIZE};

/// Resource holding the handle to the GPU visibility texture.
#[derive(Resource)]
pub struct FogVisibilityTexture(pub Handle<Image>);

/// Tweakable gameplay thresholds for fog of war.
#[derive(Resource)]
pub struct FogTweakSettings {
    pub mob_threshold: f32,
    pub object_threshold: f32,
    pub vfx_threshold: f32,
    pub decay_value: f32,
}

impl Default for FogTweakSettings {
    fn default() -> Self {
        Self {
            mob_threshold: 0.8,
            object_threshold: 0.4,
            vfx_threshold: 0.3,
            decay_value: 0.5,
        }
    }
}

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
                    update_fog_texture,
                    update_fog_material_time,
                    fog_hide_enemies,
                )
                    .chain(),
            );
    }
}

fn register_fog_tweaks(mut tweaks: ResMut<crate::debug::DebugTweaks>) {
    let s = FogSettings::default();

    // FoW Shader folder
    tweaks.add_float("FoW Shader", "Noise Scale", s.noise_scale, 0.0, 30.0, 0.5);
    tweaks.add_float("FoW Shader", "Edge Glow Width", s.edge_glow_width, 0.0, 0.5, 0.01);
    tweaks.add_float("FoW Shader", "Edge Glow Intensity", s.edge_glow_intensity, 0.0, 2.0, 0.05);
    tweaks.add_float("FoW Shader", "Fog Color R", s.fog_color.x, 0.0, 1.0, 0.01);
    tweaks.add_float("FoW Shader", "Fog Color G", s.fog_color.y, 0.0, 1.0, 0.01);
    tweaks.add_float("FoW Shader", "Fog Color B", s.fog_color.z, 0.0, 1.0, 0.01);
    tweaks.add_float("FoW Shader", "Fog Color A", s.fog_color.w, 0.0, 1.0, 0.01);
    tweaks.add_float("FoW Shader", "Glow Color R", s.glow_color.x, 0.0, 1.0, 0.01);
    tweaks.add_float("FoW Shader", "Glow Color G", s.glow_color.y, 0.0, 1.0, 0.01);
    tweaks.add_float("FoW Shader", "Glow Color B", s.glow_color.z, 0.0, 1.0, 0.01);
    tweaks.add_float("FoW Shader", "Glow Color A", s.glow_color.w, 0.0, 1.0, 0.01);
    tweaks.add_float("FoW Shader", "Explored Tint R", s.explored_tint.x, 0.0, 1.0, 0.01);
    tweaks.add_float("FoW Shader", "Explored Tint G", s.explored_tint.y, 0.0, 1.0, 0.01);
    tweaks.add_float("FoW Shader", "Explored Tint B", s.explored_tint.z, 0.0, 1.0, 0.01);
    tweaks.add_float("FoW Shader", "Explored Tint A", s.explored_tint.w, 0.0, 1.0, 0.01);

    // FoW Gameplay folder
    let t = FogTweakSettings::default();
    tweaks.add_float("FoW Gameplay", "Mob Threshold", t.mob_threshold, 0.0, 1.0, 0.05);
    tweaks.add_float("FoW Gameplay", "Object Threshold", t.object_threshold, 0.0, 1.0, 0.05);
    tweaks.add_float("FoW Gameplay", "VFX Threshold", t.vfx_threshold, 0.0, 1.0, 0.05);
    tweaks.add_float("FoW Gameplay", "Decay Value", t.decay_value, 0.0, 1.0, 0.05);
}

/// Create the R8Unorm visibility texture for GPU sampling.
fn create_visibility_texture(images: &mut Assets<Image>, grid_size: usize) -> Handle<Image> {
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

/// Spawn the fog-of-war overlay mesh and initialize resources.
fn spawn_fog_overlay(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut fog_materials: ResMut<Assets<FogOfWarMaterial>>,
    mut images: ResMut<Assets<Image>>,
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
            let y = terrain_height(x, z) + 1.5;

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

    // Create visibility texture for the shader
    let vis_handle = create_visibility_texture(&mut images, grid_size);

    let material = fog_materials.add(FogOfWarMaterial {
        settings: FogSettings::default(),
        visibility_texture: Some(vis_handle.clone()),
    });

    commands.spawn((
        FogOverlay,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        Transform::default(),
    ));

    commands.insert_resource(FogVisibilityTexture(vis_handle));

    let total = grid_size * grid_size;
    commands.insert_resource(FogOfWarMap {
        visibility: vec![0.0; total],
        grid_size,
        map_size: MAP_SIZE,
    });
}

fn update_fog_visibility(
    mut fog_map: ResMut<FogOfWarMap>,
    fog_settings: Res<FogTweakSettings>,
    all_units: Query<(&Transform, &VisionRange, &Faction), With<Unit>>,
    all_buildings: Query<(&Transform, &VisionRange, &Faction), With<Building>>,
) {
    let grid_size = fog_map.grid_size;
    let step = fog_map.map_size / (grid_size - 1) as f32;
    let decay = fog_settings.decay_value;

    for v in fog_map.visibility.iter_mut() {
        if *v > decay {
            *v = decay;
        }
    }

    let mut viewers: Vec<(Vec3, f32)> = Vec::new();
    for (tf, vr, faction) in all_units.iter() {
        if *faction == Faction::Player {
            viewers.push((tf.translation, vr.0));
        }
    }
    for (tf, vr, faction) in all_buildings.iter() {
        if *faction == Faction::Player {
            viewers.push((tf.translation, vr.0));
        }
    }

    for (pos, range) in &viewers {
        let range_sq = range * range;

        let min_x = ((pos.x - range + HALF_MAP) / step).floor().max(0.0) as usize;
        let max_x =
            ((pos.x + range + HALF_MAP) / step).ceil().min((grid_size - 1) as f32) as usize;
        let min_z = ((pos.z - range + HALF_MAP) / step).floor().max(0.0) as usize;
        let max_z =
            ((pos.z + range + HALF_MAP) / step).ceil().min((grid_size - 1) as f32) as usize;

        for iz in min_z..=max_z {
            for ix in min_x..=max_x {
                let wx = -HALF_MAP + ix as f32 * step;
                let wz = -HALF_MAP + iz as f32 * step;
                let dx = wx - pos.x;
                let dz = wz - pos.z;
                let dist_sq = dx * dx + dz * dz;

                if dist_sq <= range_sq {
                    let t = (dist_sq / range_sq).sqrt();
                    let edge_fade = 1.0 - (t * t);
                    let vis = 0.5 + 0.5 * edge_fade;
                    let idx = iz * grid_size + ix;
                    if vis > fog_map.visibility[idx] {
                        fog_map.visibility[idx] = vis;
                    }
                }
            }
        }
    }
}

/// Bake the CPU visibility grid into the GPU texture each frame.
fn update_fog_texture(
    fog_map: Res<FogOfWarMap>,
    fog_tex: Res<FogVisibilityTexture>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(image) = images.get_mut(&fog_tex.0) else {
        return;
    };
    let Some(ref mut data) = image.data else {
        return;
    };
    let total = fog_map.grid_size * fog_map.grid_size;
    for i in 0..total {
        let vis = fog_map.visibility[i].clamp(0.0, 1.0);
        data[i] = (vis * 255.0) as u8;
    }
}

/// Push elapsed time into the material uniform for shader animation.
fn update_fog_material_time(
    time: Res<Time>,
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
}

fn fog_hide_enemies(
    fog_map: Res<FogOfWarMap>,
    fog_settings: Res<FogTweakSettings>,
    mut mobs: Query<(&Transform, &mut Visibility), With<Mob>>,
    mut resource_nodes: Query<(&Transform, &mut Visibility), (With<ResourceNode>, Without<Mob>)>,
    mut decorations: Query<
        (&Transform, &mut Visibility),
        (
            With<Decoration>,
            Without<Mob>,
            Without<ResourceNode>,
        ),
    >,
    mut projectiles: Query<
        (&Transform, &mut Visibility),
        (
            With<Projectile>,
            Without<Mob>,
            Without<ResourceNode>,
            Without<Decoration>,
        ),
    >,
    mut vfx: Query<
        (&Transform, &mut Visibility),
        (
            With<VfxFlash>,
            Without<Mob>,
            Without<ResourceNode>,
            Without<Decoration>,
            Without<Projectile>,
        ),
    >,
) {
    let mob_t = fog_settings.mob_threshold;
    let obj_t = fog_settings.object_threshold;
    let vfx_t = fog_settings.vfx_threshold;

    for (tf, mut vis) in mobs.iter_mut() {
        let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
        *vis = if v >= mob_t {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (tf, mut vis) in resource_nodes.iter_mut() {
        let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
        *vis = if v >= obj_t {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (tf, mut vis) in decorations.iter_mut() {
        let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
        *vis = if v >= obj_t {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (tf, mut vis) in projectiles.iter_mut() {
        let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
        *vis = if v >= vfx_t {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (tf, mut vis) in vfx.iter_mut() {
        let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
        *vis = if v >= vfx_t {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}
