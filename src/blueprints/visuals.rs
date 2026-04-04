use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::blueprints::types::*;
use crate::components::{FloorPieceKind, WALL_CELL_SIZE};

fn build_floor_piece_mesh(kind: FloorPieceKind) -> Mesh {
    let half = WALL_CELL_SIZE * 0.5; // 1.5
    let bot_y = -0.5_f32; // extend well below surface to hide terrain
    let top_y = 0.06_f32; // surface height
    let lip_y = 0.12_f32; // raised lip on exposed edges
    let lip_inset = 0.06_f32;
    let lip_width = 0.12_f32;

    // Determine which edges have a neighbor (connected = flush, no side wall)
    let (n_conn, e_conn, s_conn, w_conn) = match kind {
        FloorPieceKind::Isolated => (false, false, false, false),
        FloorPieceKind::End => (true, false, false, false),
        FloorPieceKind::Straight => (true, false, true, false),
        FloorPieceKind::Corner => (true, true, false, false),
        FloorPieceKind::Tee => (true, true, false, true),
        FloorPieceKind::Cross => (true, true, true, true),
    };
    let connected = [n_conn, e_conn, s_conn, w_conn];

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    let push_quad = |pos: &mut Vec<[f32; 3]>,
                     nrm: &mut Vec<[f32; 3]>,
                     uv: &mut Vec<[f32; 2]>,
                     col: &mut Vec<[f32; 4]>,
                     idx: &mut Vec<u32>,
                     verts: [[f32; 3]; 4],
                     normal: [f32; 3],
                     uv_rect: [[f32; 2]; 4],
                     vert_colors: [[f32; 4]; 4]| {
        let base = pos.len() as u32;
        pos.extend_from_slice(&verts);
        nrm.extend_from_slice(&[normal; 4]);
        uv.extend_from_slice(&uv_rect);
        col.extend_from_slice(&vert_colors);
        idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    // Compute per-vertex edge-distance alpha for the top face.
    // Alpha = min distance to any *exposed* (unconnected) edge, normalized 0..1.
    // connected[0]=N(-Z), [1]=E(+X), [2]=S(+Z), [3]=W(-X)
    let cell_size = half * 2.0;
    let corner_positions = [
        (-half, -half), // TL: x=-half, z=-half
        (half, -half),  // TR: x=+half, z=-half
        (half, half),   // BR: x=+half, z=+half
        (-half, half),  // BL: x=-half, z=+half
    ];
    let mut top_colors = [[1.0_f32; 4]; 4];
    for (vi, &(vx, vz)) in corner_positions.iter().enumerate() {
        let mut min_dist = 1.0_f32;
        // North edge (z = -half): distance is (vz - (-half)) / cell_size
        if !connected[0] {
            min_dist = min_dist.min((vz + half) / cell_size);
        }
        // East edge (x = +half): distance is (half - vx) / cell_size
        if !connected[1] {
            min_dist = min_dist.min((half - vx) / cell_size);
        }
        // South edge (z = +half): distance is (half - vz) / cell_size
        if !connected[2] {
            min_dist = min_dist.min((half - vz) / cell_size);
        }
        // West edge (x = -half): distance is (vx + half) / cell_size
        if !connected[3] {
            min_dist = min_dist.min((vx + half) / cell_size);
        }
        top_colors[vi] = [0.5, 0.5, 0.5, min_dist];
    }

    let side_color = [0.5, 0.5, 0.5, 0.0_f32]; // sides are at the edge
                                               // ── Top face (full cell) ──
    push_quad(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut colors,
        &mut indices,
        [
            [-half, top_y, -half],
            [half, top_y, -half],
            [half, top_y, half],
            [-half, top_y, half],
        ],
        [0.0, 1.0, 0.0],
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        top_colors,
    );

    // ── Bottom face (full cell) ──
    push_quad(
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut colors,
        &mut indices,
        [
            [-half, bot_y, half],
            [half, bot_y, half],
            [half, bot_y, -half],
            [-half, bot_y, -half],
        ],
        [0.0, -1.0, 0.0],
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        [side_color; 4],
    );

    // Edge definitions: (normal, 4 verts for side wall, lip top quad, lip side quad)
    // Order: North(-Z), East(+X), South(+Z), West(-X)
    struct EdgeDef {
        side_verts: [[f32; 3]; 4],
        side_normal: [f32; 3],
        lip_top_verts: [[f32; 3]; 4],
        lip_outer_verts: [[f32; 3]; 4],
        lip_outer_normal: [f32; 3],
        lip_inner_verts: [[f32; 3]; 4],
        lip_inner_normal: [f32; 3],
        lip_end_a_verts: [[f32; 3]; 4],
        lip_end_a_normal: [f32; 3],
        lip_end_b_verts: [[f32; 3]; 4],
        lip_end_b_normal: [f32; 3],
    }

    let li = lip_inset;
    let lw = lip_width;
    let edges = [
        // North (-Z edge, z = -half)
        EdgeDef {
            side_verts: [
                [-half, bot_y, -half],
                [half, bot_y, -half],
                [half, top_y, -half],
                [-half, top_y, -half],
            ],
            side_normal: [0.0, 0.0, -1.0],
            lip_top_verts: [
                [-half + li, lip_y, -half + li],
                [half - li, lip_y, -half + li],
                [half - li, lip_y, -half + li + lw],
                [-half + li, lip_y, -half + li + lw],
            ],
            lip_outer_verts: [
                [-half + li, top_y, -half + li],
                [half - li, top_y, -half + li],
                [half - li, lip_y, -half + li],
                [-half + li, lip_y, -half + li],
            ],
            lip_outer_normal: [0.0, 0.0, -1.0],
            lip_inner_verts: [
                [half - li, top_y, -half + li + lw],
                [-half + li, top_y, -half + li + lw],
                [-half + li, lip_y, -half + li + lw],
                [half - li, lip_y, -half + li + lw],
            ],
            lip_inner_normal: [0.0, 0.0, 1.0],
            lip_end_a_verts: [
                [-half + li, top_y, -half + li],
                [-half + li, top_y, -half + li + lw],
                [-half + li, lip_y, -half + li + lw],
                [-half + li, lip_y, -half + li],
            ],
            lip_end_a_normal: [-1.0, 0.0, 0.0],
            lip_end_b_verts: [
                [half - li, top_y, -half + li + lw],
                [half - li, top_y, -half + li],
                [half - li, lip_y, -half + li],
                [half - li, lip_y, -half + li + lw],
            ],
            lip_end_b_normal: [1.0, 0.0, 0.0],
        },
        // East (+X edge, x = half)
        EdgeDef {
            side_verts: [
                [half, bot_y, -half],
                [half, bot_y, half],
                [half, top_y, half],
                [half, top_y, -half],
            ],
            side_normal: [1.0, 0.0, 0.0],
            lip_top_verts: [
                [half - li - lw, lip_y, -half + li],
                [half - li, lip_y, -half + li],
                [half - li, lip_y, half - li],
                [half - li - lw, lip_y, half - li],
            ],
            lip_outer_verts: [
                [half - li, top_y, -half + li],
                [half - li, top_y, half - li],
                [half - li, lip_y, half - li],
                [half - li, lip_y, -half + li],
            ],
            lip_outer_normal: [1.0, 0.0, 0.0],
            lip_inner_verts: [
                [half - li - lw, top_y, half - li],
                [half - li - lw, top_y, -half + li],
                [half - li - lw, lip_y, -half + li],
                [half - li - lw, lip_y, half - li],
            ],
            lip_inner_normal: [-1.0, 0.0, 0.0],
            lip_end_a_verts: [
                [half - li - lw, top_y, -half + li],
                [half - li, top_y, -half + li],
                [half - li, lip_y, -half + li],
                [half - li - lw, lip_y, -half + li],
            ],
            lip_end_a_normal: [0.0, 0.0, -1.0],
            lip_end_b_verts: [
                [half - li, top_y, half - li],
                [half - li - lw, top_y, half - li],
                [half - li - lw, lip_y, half - li],
                [half - li, lip_y, half - li],
            ],
            lip_end_b_normal: [0.0, 0.0, 1.0],
        },
        // South (+Z edge, z = half)
        EdgeDef {
            side_verts: [
                [half, bot_y, half],
                [-half, bot_y, half],
                [-half, top_y, half],
                [half, top_y, half],
            ],
            side_normal: [0.0, 0.0, 1.0],
            lip_top_verts: [
                [half - li, lip_y, half - li - lw],
                [-half + li, lip_y, half - li - lw],
                [-half + li, lip_y, half - li],
                [half - li, lip_y, half - li],
            ],
            lip_outer_verts: [
                [half - li, top_y, half - li],
                [-half + li, top_y, half - li],
                [-half + li, lip_y, half - li],
                [half - li, lip_y, half - li],
            ],
            lip_outer_normal: [0.0, 0.0, 1.0],
            lip_inner_verts: [
                [-half + li, top_y, half - li - lw],
                [half - li, top_y, half - li - lw],
                [half - li, lip_y, half - li - lw],
                [-half + li, lip_y, half - li - lw],
            ],
            lip_inner_normal: [0.0, 0.0, -1.0],
            lip_end_a_verts: [
                [half - li, top_y, half - li - lw],
                [half - li, top_y, half - li],
                [half - li, lip_y, half - li],
                [half - li, lip_y, half - li - lw],
            ],
            lip_end_a_normal: [1.0, 0.0, 0.0],
            lip_end_b_verts: [
                [-half + li, top_y, half - li],
                [-half + li, top_y, half - li - lw],
                [-half + li, lip_y, half - li - lw],
                [-half + li, lip_y, half - li],
            ],
            lip_end_b_normal: [-1.0, 0.0, 0.0],
        },
        // West (-X edge, x = -half)
        EdgeDef {
            side_verts: [
                [-half, bot_y, half],
                [-half, bot_y, -half],
                [-half, top_y, -half],
                [-half, top_y, half],
            ],
            side_normal: [-1.0, 0.0, 0.0],
            lip_top_verts: [
                [-half + li, lip_y, -half + li],
                [-half + li + lw, lip_y, -half + li],
                [-half + li + lw, lip_y, half - li],
                [-half + li, lip_y, half - li],
            ],
            lip_outer_verts: [
                [-half + li, top_y, half - li],
                [-half + li, top_y, -half + li],
                [-half + li, lip_y, -half + li],
                [-half + li, lip_y, half - li],
            ],
            lip_outer_normal: [-1.0, 0.0, 0.0],
            lip_inner_verts: [
                [-half + li + lw, top_y, -half + li],
                [-half + li + lw, top_y, half - li],
                [-half + li + lw, lip_y, half - li],
                [-half + li + lw, lip_y, -half + li],
            ],
            lip_inner_normal: [1.0, 0.0, 0.0],
            lip_end_a_verts: [
                [-half + li, top_y, -half + li],
                [-half + li + lw, top_y, -half + li],
                [-half + li + lw, lip_y, -half + li],
                [-half + li, lip_y, -half + li],
            ],
            lip_end_a_normal: [0.0, 0.0, -1.0],
            lip_end_b_verts: [
                [-half + li + lw, top_y, half - li],
                [-half + li, top_y, half - li],
                [-half + li, lip_y, half - li],
                [-half + li + lw, lip_y, half - li],
            ],
            lip_end_b_normal: [0.0, 0.0, 1.0],
        },
    ];

    let uv_full = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    for (i, edge) in edges.iter().enumerate() {
        if connected[i] {
            continue; // neighbor present — no side wall, no lip
        }
        // Side wall — at the edge, alpha = 0
        push_quad(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut colors,
            &mut indices,
            edge.side_verts,
            edge.side_normal,
            uv_full,
            [side_color; 4],
        );
        // Lip: top, outer side, inner side, two end caps — all near edge
        push_quad(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut colors,
            &mut indices,
            edge.lip_top_verts,
            [0.0, 1.0, 0.0],
            uv_full,
            [side_color; 4],
        );
        push_quad(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut colors,
            &mut indices,
            edge.lip_outer_verts,
            edge.lip_outer_normal,
            uv_full,
            [side_color; 4],
        );
        push_quad(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut colors,
            &mut indices,
            edge.lip_inner_verts,
            edge.lip_inner_normal,
            uv_full,
            [side_color; 4],
        );
        push_quad(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut colors,
            &mut indices,
            edge.lip_end_a_verts,
            edge.lip_end_a_normal,
            uv_full,
            [side_color; 4],
        );
        push_quad(
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut colors,
            &mut indices,
            edge.lip_end_b_verts,
            edge.lip_end_b_normal,
            uv_full,
            [side_color; 4],
        );
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        VertexAttributeValues::Float32x4(colors),
    );
    mesh.insert_indices(bevy::mesh::Indices::U32(indices));
    mesh
}

// ── Build the registry ──

pub fn build_visual_cache(
    registry: &BlueprintRegistry,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> EntityVisualCache {
    let mut cache = EntityVisualCache::default();

    for (kind, bp) in &registry.blueprints {
        let mesh = match *kind {
            EntityKind::Floor => meshes.add(build_floor_piece_mesh(FloorPieceKind::Cross)),
            _ => match bp.visual.mesh_kind {
                MeshKind::Capsule { radius, length } => meshes.add(Capsule3d::new(radius, length)),
                MeshKind::Cuboid { x, y, z } => meshes.add(Cuboid::new(x, y, z)),
                MeshKind::GltfScene { .. } => meshes.add(Cuboid::new(4.0, 0.3, 4.0)),
                MeshKind::GltfCharacter { .. } => meshes.add(Cuboid::new(0.5, 0.1, 0.5)),
                MeshKind::ProceduralMob { .. } => match *kind {
                    EntityKind::Goblin => meshes.add(Cuboid::new(0.8, 0.8, 0.8)),
                    EntityKind::Skeleton => meshes.add(Cuboid::new(0.6, 1.6, 0.6)),
                    EntityKind::Orc => meshes.add(Cuboid::new(1.4, 1.2, 1.4)),
                    EntityKind::Demon => meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
                    _ => meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
                },
            },
        };

        let emissive = match *kind {
            EntityKind::Demon => LinearRgba::new(0.8, 0.1, 0.1, 1.0),
            EntityKind::Skeleton if bp.visual.mesh_kind.is_procedural_mob() => {
                LinearRgba::new(0.1, 0.1, 0.15, 1.0)
            }
            _ => LinearRgba::NONE,
        };
        let mut default_material = StandardMaterial {
            base_color: bp.visual.color,
            emissive,
            ..default()
        };
        let mut selected_material = StandardMaterial {
            base_color: bp.visual.selected_color,
            emissive: bp.visual.selected_emissive,
            ..default()
        };
        let mut hovered_material = StandardMaterial {
            base_color: bp.visual.color,
            emissive: LinearRgba::new(
                bp.visual.selected_emissive.red * 0.35,
                bp.visual.selected_emissive.green * 0.35,
                bp.visual.selected_emissive.blue * 0.35,
                bp.visual.selected_emissive.alpha,
            ),
            ..default()
        };

        if *kind == EntityKind::Floor {
            default_material.perceptual_roughness = 0.95;
            default_material.metallic = 0.0;
            selected_material.perceptual_roughness = 0.9;
            hovered_material.perceptual_roughness = 0.93;
        }

        let mat_default = materials.add(default_material);

        let mat_selected = materials.add(selected_material);
        let mat_hovered = materials.add(hovered_material);

        cache.meshes.insert(*kind, mesh);
        cache.materials_default.insert(*kind, mat_default);
        cache.materials_selected.insert(*kind, mat_selected);
        cache.materials_hovered.insert(*kind, mat_hovered);
    }

    for piece in [
        FloorPieceKind::Isolated,
        FloorPieceKind::End,
        FloorPieceKind::Straight,
        FloorPieceKind::Corner,
        FloorPieceKind::Tee,
        FloorPieceKind::Cross,
    ] {
        cache
            .floor_piece_meshes
            .insert(piece, meshes.add(build_floor_piece_mesh(piece)));
    }

    // Flat plane mesh for the floor brush indicator (no side walls)
    cache.floor_brush_indicator = Some(
        meshes.add(
            Plane3d::default()
                .mesh()
                .size(WALL_CELL_SIZE, WALL_CELL_SIZE),
        ),
    );

    cache
}

// ── Plugin ──
