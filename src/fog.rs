use bevy::pbr::{FogVolume, VolumetricFog, VolumetricLight};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};

use crate::components::*;
use crate::ground::{terrain_height, GRID_SIZE, HALF_MAP, MAP_SIZE};

pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, (spawn_fog_overlay, setup_volumetric_fog))
            .add_systems(
                Update,
                (update_fog_visibility, apply_fog_to_mesh, fog_hide_enemies).chain(),
            );
    }
}

/// Spawn the fog-of-war overlay mesh and initialize the FogOfWarMap resource.
fn spawn_fog_overlay(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let grid_size = GRID_SIZE;
    let step = MAP_SIZE / (grid_size - 1) as f32;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(grid_size * grid_size);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(grid_size * grid_size);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(grid_size * grid_size);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(grid_size * grid_size);

    for iz in 0..grid_size {
        for ix in 0..grid_size {
            let x = -HALF_MAP + ix as f32 * step;
            let z = -HALF_MAP + iz as f32 * step;
            let y = terrain_height(x, z) + 0.3;

            positions.push([x, y, z]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([
                ix as f32 / (grid_size - 1) as f32,
                iz as f32 / (grid_size - 1) as f32,
            ]);
            colors.push([0.02, 0.02, 0.05, 1.0]);
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
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        VertexAttributeValues::Float32x4(colors),
    );
    mesh.insert_indices(Indices::U32(indices));

    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.02, 0.02, 0.05, 1.0),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    commands.spawn((
        FogOverlay,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(material),
        Transform::default(),
    ));

    // Initialize fog map
    let total = grid_size * grid_size;
    commands.insert_resource(FogOfWarMap {
        visibility: vec![0.0; total],
        grid_size,
        map_size: MAP_SIZE,
    });
}

/// Set up volumetric fog, distance fog, fog volume, and darken scene lighting.
fn setup_volumetric_fog(
    mut commands: Commands,
    camera_q: Query<Entity, With<RtsCamera>>,
    dir_light_q: Query<Entity, With<DirectionalLight>>,
    mut ambient: ResMut<AmbientLight>,
) {
    // Darken ambient light for moodier atmosphere
    ambient.brightness = 80.0;
    ambient.color = Color::srgb(0.6, 0.6, 0.75);

    // Add volumetric fog + distance fog to camera
    if let Ok(cam_entity) = camera_q.get_single() {
        commands.entity(cam_entity).insert((
            DistanceFog {
                color: Color::srgba(0.05, 0.05, 0.1, 1.0),
                falloff: FogFalloff::ExponentialSquared { density: 0.0015 },
                ..default()
            },
            VolumetricFog {
                ambient_color: Color::srgb(0.05, 0.05, 0.1),
                ambient_intensity: 0.02,
                step_count: 48,
                jitter: 0.5,
                ..default()
            },
        ));
    }

    // Enable volumetric light on the directional light (god rays)
    if let Ok(light_entity) = dir_light_q.get_single() {
        commands.entity(light_entity).insert(VolumetricLight);
    }

    // Large fog volume covering the whole map for atmospheric haze
    commands.spawn((
        FogVolume {
            fog_color: Color::srgba(0.08, 0.08, 0.15, 1.0),
            density_factor: 0.012,
            absorption: 0.04,
            scattering: 0.03,
            scattering_asymmetry: 0.7,
            light_tint: Color::srgb(0.6, 0.7, 1.0),
            light_intensity: 0.8,
            ..default()
        },
        Transform::from_xyz(0.0, 15.0, 0.0).with_scale(Vec3::new(MAP_SIZE, 30.0, MAP_SIZE)),
    ));
}

fn update_fog_visibility(
    mut fog_map: ResMut<FogOfWarMap>,
    all_units: Query<(&Transform, &VisionRange, &Faction), With<Unit>>,
    all_buildings: Query<(&Transform, &VisionRange, &Faction), With<Building>>,
) {
    let grid_size = fog_map.grid_size;
    let step = fog_map.map_size / (grid_size - 1) as f32;

    // Decay: visible → explored, explored stays, unexplored stays
    for v in fog_map.visibility.iter_mut() {
        if *v > 0.5 {
            *v = 0.5;
        }
    }

    // Collect player viewers
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

    // For each viewer, write smooth visibility with distance falloff
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
                    // Smooth falloff: 1.0 at center, fading toward edge
                    let t = (dist_sq / range_sq).sqrt(); // 0..1 from center to edge
                    let edge_fade = 1.0 - (t * t); // smooth quadratic falloff at edges
                    let vis = 0.5 + 0.5 * edge_fade; // ranges from 0.5 (edge) to 1.0 (center)
                    let idx = iz * grid_size + ix;
                    if vis > fog_map.visibility[idx] {
                        fog_map.visibility[idx] = vis;
                    }
                }
            }
        }
    }
}

fn apply_fog_to_mesh(
    fog_map: Res<FogOfWarMap>,
    fog_overlay: Query<&Mesh3d, With<FogOverlay>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Ok(mesh_handle) = fog_overlay.get_single() else {
        return;
    };
    let Some(mesh) = meshes.get_mut(&mesh_handle.0) else {
        return;
    };

    let grid_size = fog_map.grid_size;
    let total = grid_size * grid_size;

    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(total);
    for i in 0..total {
        let vis = fog_map.visibility[i];

        // Smooth alpha mapping:
        // vis 1.0 (center of vision) → alpha 0.0 (fully clear)
        // vis 0.5 (explored / vision edge) → alpha 0.5 (dim)
        // vis 0.0 (unexplored) → alpha 1.0 (fully opaque black)
        let alpha = if vis >= 1.0 {
            0.0
        } else if vis > 0.5 {
            // Smooth transition from clear to explored
            let t = (vis - 0.5) / 0.5; // 0..1 where 1 = fully visible
            let t_smooth = t * t * (3.0 - 2.0 * t); // smoothstep
            0.5 * (1.0 - t_smooth)
        } else if vis > 0.0 {
            // Explored → unexplored: 0.5 → 1.0
            let t = vis / 0.5; // 0..1 where 1 = explored edge
            1.0 - t * 0.5 // 1.0 → 0.5
        } else {
            1.0
        };

        // Slight blue tint in fog for atmosphere
        let r = 0.01;
        let g = 0.01;
        let b = 0.03;
        colors.push([r, g, b, alpha]);
    }

    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        VertexAttributeValues::Float32x4(colors),
    );
}

fn fog_hide_enemies(
    fog_map: Res<FogOfWarMap>,
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
    for (tf, mut vis) in mobs.iter_mut() {
        let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
        *vis = if v >= 0.8 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (tf, mut vis) in resource_nodes.iter_mut() {
        let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
        *vis = if v >= 0.4 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (tf, mut vis) in decorations.iter_mut() {
        let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
        *vis = if v >= 0.4 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (tf, mut vis) in projectiles.iter_mut() {
        let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
        *vis = if v >= 0.3 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (tf, mut vis) in vfx.iter_mut() {
        let v = fog_map.get_visibility(tf.translation.x, tf.translation.z);
        *vis = if v >= 0.3 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}
