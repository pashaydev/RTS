use std::collections::{HashMap, VecDeque};

use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::types::RtsCamera;
use crate::world::fog::FogTextures;
use crate::world::lighting::SunLight;
use crate::presentation::materials::water::WaterMaterial;

use super::data::WATER_LEVEL;

/// Marker for water plane entities.
#[derive(Component)]
pub struct WaterPlane;

pub fn update_water_time(
    time: Res<Time>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    query: Query<&MeshMaterial3d<WaterMaterial>, With<WaterPlane>>,
    camera_q: Query<&Transform, With<RtsCamera>>,
    sun_q: Query<&Transform, (With<SunLight>, Without<RtsCamera>)>,
) {
    let cam_pos = camera_q
        .iter()
        .next()
        .map(|transform| transform.translation)
        .unwrap_or(Vec3::new(0.0, 50.0, 0.0));

    let sun_dir = sun_q
        .iter()
        .next()
        .map(|transform| {
            let forward = transform.forward().as_vec3();
            Vec4::new(-forward.x, -forward.y, -forward.z, 0.0)
        })
        .unwrap_or(Vec4::new(0.5, 0.7, 0.3, 0.0));

    for material_handle in &query {
        if let Some(material) = materials.get_mut(&material_handle.0) {
            material.settings.time = time.elapsed_secs();
            material.settings.camera_position = Vec4::new(cam_pos.x, cam_pos.y, cam_pos.z, 0.0);
            material.settings.sun_direction = sun_dir;
        }
    }
}

/// Once fog textures exist, patch all water materials to reference them.
pub fn patch_water_fog_textures(
    fog_tex: Option<Res<FogTextures>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    query: Query<&MeshMaterial3d<WaterMaterial>, With<WaterPlane>>,
) {
    let Some(fog_tex) = fog_tex else {
        return;
    };

    for material_handle in &query {
        if let Some(material) = materials.get_mut(&material_handle.0) {
            if material.fog_visible_texture.is_none() {
                material.fog_visible_texture = Some(fog_tex.visible.clone());
                material.fog_explored_texture = Some(fog_tex.explored.clone());
            }
        }
    }
}

/// Flood-fill on the grid-cell (quad) level to find connected water regions.
pub fn find_water_bodies(heights: &[f32], grid_size: usize) -> Vec<Vec<(usize, usize)>> {
    let cells = grid_size - 1;
    let mut visited = vec![false; cells * cells];
    let mut bodies = Vec::new();

    let is_wet = |ix: usize, iz: usize| -> bool {
        let tl = iz * grid_size + ix;
        let tr = tl + 1;
        let bl = tl + grid_size;
        let br = bl + 1;
        heights[tl] < WATER_LEVEL
            || heights[tr] < WATER_LEVEL
            || heights[bl] < WATER_LEVEL
            || heights[br] < WATER_LEVEL
    };

    for sz in 0..cells {
        for sx in 0..cells {
            let idx = sz * cells + sx;
            if visited[idx] || !is_wet(sx, sz) {
                continue;
            }

            let mut body = Vec::new();
            let mut queue = VecDeque::new();
            visited[idx] = true;
            queue.push_back((sx, sz));

            while let Some((cx, cz)) = queue.pop_front() {
                body.push((cx, cz));
                for (dx, dz) in [(-1i32, 0), (1, 0), (0, -1), (0, 1)] {
                    let nx = cx as i32 + dx;
                    let nz = cz as i32 + dz;
                    if nx < 0 || nz < 0 || nx >= cells as i32 || nz >= cells as i32 {
                        continue;
                    }

                    let (nx, nz) = (nx as usize, nz as usize);
                    let neighbor_index = nz * cells + nx;
                    if !visited[neighbor_index] && is_wet(nx, nz) {
                        visited[neighbor_index] = true;
                        queue.push_back((nx, nz));
                    }
                }
            }

            if body.len() >= 4 {
                bodies.push(body);
            }
        }
    }

    bodies
}

pub fn water_body_bounds(cells: &[(usize, usize)], step: f32, half_map: f32) -> (Vec3, f32) {
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_z = f32::MAX;
    let mut max_z = f32::MIN;

    for &(cx, cz) in cells {
        let x0 = -half_map + cx as f32 * step;
        let x1 = -half_map + (cx + 1) as f32 * step;
        let z0 = -half_map + cz as f32 * step;
        let z1 = -half_map + (cz + 1) as f32 * step;
        min_x = min_x.min(x0);
        max_x = max_x.max(x1);
        min_z = min_z.min(z0);
        max_z = max_z.max(z1);
    }

    let center = Vec3::new((min_x + max_x) * 0.5, WATER_LEVEL, (min_z + max_z) * 0.5);
    let half_w = (max_x - min_x) * 0.5;
    let half_h = (max_z - min_z) * 0.5;
    let radius = (half_w * half_w + half_h * half_h).sqrt();
    (center, radius)
}

pub fn build_water_body_mesh(
    cells: &[(usize, usize)],
    grid_size: usize,
    step: f32,
    half_map: f32,
    center: Vec3,
) -> Option<Mesh> {
    if cells.is_empty() {
        return None;
    }

    let mut vert_remap = HashMap::new();
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();

    let mut get_or_insert = |vertex_index: usize| -> u32 {
        if let Some(&mapped_index) = vert_remap.get(&vertex_index) {
            return mapped_index;
        }

        let iz = vertex_index / grid_size;
        let ix = vertex_index % grid_size;
        let x = -half_map + ix as f32 * step - center.x;
        let z = -half_map + iz as f32 * step - center.z;
        let mapped_index = positions.len() as u32;

        positions.push([x, WATER_LEVEL - center.y, z]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([
            ix as f32 / (grid_size - 1) as f32,
            iz as f32 / (grid_size - 1) as f32,
        ]);
        vert_remap.insert(vertex_index, mapped_index);
        mapped_index
    };

    let mut indices = Vec::with_capacity(cells.len() * 6);
    for &(cx, cz) in cells {
        let tl = cz * grid_size + cx;
        let tr = tl + 1;
        let bl = tl + grid_size;
        let br = bl + 1;

        let i_tl = get_or_insert(tl);
        let i_tr = get_or_insert(tr);
        let i_bl = get_or_insert(bl);
        let i_br = get_or_insert(br);
        indices.extend_from_slice(&[i_tl, i_bl, i_tr, i_tr, i_bl, i_br]);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}
