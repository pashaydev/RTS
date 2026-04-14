//! Building placement preview, ghost materials, and confirmation systems.

use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::blueprints::{
    BlueprintRegistry, EntityCategory, EntityKind, EntityVisualCache, LevelBonus,
};
use crate::infrastructure::audio::{PlaySfx, SfxKind};
use crate::presentation::camera;
use crate::presentation::model_assets::{
    BuildingConstructionAssets, BuildingModelAssets, UnitModelAssets,
};
use crate::types::*;
use crate::world::ground::{
    apply_terrain_shape_op, foundation_radii, sync_ground_mesh_partial, HeightMap,
    TerrainSurfaceDirtyArea, TerrainSurfaceDirtyQueue,
};
use game_state::message::TerrainShapeOp;

use super::{
    auto_tile_piece, biome_requirement_text, blocks_construction_overlap,
    building_area_blocked_by_obstacles, building_blocks_area, cleanup_worker_assignment,
    find_best_worker_for_build, footprint_for_kind, has_available_worker_for_build,
    is_biome_valid_for, piece_kind_to_entity_kind, rotate_local_xz, rotation_y_from_quat,
    sawmill_preview_plane_size, sawmill_yard_corners, spawn_floor_grid_cells,
    spawn_wall_grid_cells, uses_terrain_foundation,
};

#[derive(SystemParam)]
pub(crate) struct PlacementOnlineParams<'w> {
    net_role: Res<'w, crate::infrastructure::multiplayer::NetRole>,
    client_state: Option<Res<'w, crate::infrastructure::multiplayer::ClientNetState>>,
    matchbox_socket: Option<ResMut<'w, bevy_matchbox::prelude::MatchboxSocket>>,
    time: Res<'w, Time>,
}

// ── Asset creation (ghost materials only) ──

pub(crate) fn create_ghost_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Create a procedural grid texture for the placement ground plane.
    let grid_tex = {
        use bevy::image::{ImageAddressMode, ImageSamplerDescriptor};
        const TEX_SIZE: u32 = 64;
        const CELLS: f32 = 4.0;
        let mut data = vec![0u8; (TEX_SIZE * TEX_SIZE * 4) as usize];

        for y in 0..TEX_SIZE {
            for x in 0..TEX_SIZE {
                let u = x as f32 / TEX_SIZE as f32;
                let v = y as f32 / TEX_SIZE as f32;

                // Grid lines: distance to nearest cell edge
                let fx = (u * CELLS).fract();
                let fy = (v * CELLS).fract();
                let dx = fx.min(1.0 - fx) * CELLS * TEX_SIZE as f32 / CELLS;
                let dy = fy.min(1.0 - fy) * CELLS * TEX_SIZE as f32 / CELLS;
                let edge_dist = dx.min(dy);
                let line = (1.5 - edge_dist).clamp(0.0, 1.0);

                let idx = ((y * TEX_SIZE + x) * 4) as usize;
                let r = (40.0 + line * 60.0).clamp(0.0, 255.0) as u8;
                let g = (120.0 + line * 100.0).clamp(0.0, 255.0) as u8;
                let b = (100.0 + line * 90.0).clamp(0.0, 255.0) as u8;
                let a = (10.0 + line * 180.0).clamp(0.0, 255.0) as u8;

                data[idx] = r;
                data[idx + 1] = g;
                data[idx + 2] = b;
                data[idx + 3] = a;
            }
        }
        let mut img = Image::new_fill(
            bevy::render::render_resource::Extent3d {
                width: TEX_SIZE,
                height: TEX_SIZE,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            &[255, 255, 255, 255],
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        img.data = Some(data);
        img.sampler = bevy::image::ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            ..default()
        });
        images.add(img)
    };

    commands.insert_resource(BuildingGhostMaterials {
        ghost_valid: materials.add(StandardMaterial {
            base_color: Color::srgba(0.35, 0.98, 0.72, 0.34),
            emissive: LinearRgba::new(0.18, 0.72, 0.45, 1.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        ghost_invalid: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.38, 0.22, 0.34),
            emissive: LinearRgba::new(0.82, 0.16, 0.08, 1.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
        under_construction: materials.add(StandardMaterial {
            base_color: Color::srgba(0.7, 0.65, 0.5, 0.6),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        grid_plane: materials.add(StandardMaterial {
            base_color: Color::srgba(0.82, 1.0, 0.96, 0.9),
            base_color_texture: Some(grid_tex),
            emissive: LinearRgba::new(0.18, 0.56, 0.42, 1.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        }),
    });
}

#[derive(Component)]
pub(crate) struct GhostSawmillFencePiece {
    local_anchor: Vec3,
    local_rotation_y: f32,
    y_offset: f32,
}

fn spawn_sawmill_fence_preview(ghost_cmds: &mut EntityCommands, meshes: &mut Assets<Mesh>) {
    let corners = sawmill_yard_corners(Vec3::ZERO, 0.0);
    let post_mesh = meshes.add(Cylinder::new(0.05, 0.8));

    for corner in corners {
        ghost_cmds.with_child((
            Mesh3d(post_mesh.clone()),
            Transform::from_translation(corner),
            GhostSawmillFencePiece {
                local_anchor: corner,
                local_rotation_y: 0.0,
                y_offset: 0.4,
            },
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }

    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let local_mid = (a + b) * 0.5;
        let span = (b - a).length();
        let local_angle = (b.z - a.z).atan2(b.x - a.x);
        let rail_mesh = meshes.add(Cuboid::new(span, 0.06, 0.06));

        for &y_offset in &[0.25, 0.55] {
            ghost_cmds.with_child((
                Mesh3d(rail_mesh.clone()),
                Transform::from_translation(Vec3::new(local_mid.x, y_offset, local_mid.z))
                    .with_rotation(Quat::from_rotation_y(local_angle)),
                GhostSawmillFencePiece {
                    local_anchor: local_mid,
                    local_rotation_y: local_angle,
                    y_offset,
                },
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
    }
}

// ── Placement preview ──

fn cursor_ground_pos(
    camera_q: &Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
    windows: &Query<&Window, With<PrimaryWindow>>,
    graphics: &GraphicsSettings,
    height_map: &HeightMap,
) -> Option<Vec3> {
    let Ok(window) = windows.single() else {
        return None;
    };
    let Ok((camera, cam_gt)) = camera_q.single() else {
        return None;
    };
    let ray = camera::viewport_ray_from_window_cursor(camera, cam_gt, window, graphics)?;
    cursor_terrain_hit(ray, height_map)
}

fn cursor_terrain_hit(ray: Ray3d, height_map: &HeightMap) -> Option<Vec3> {
    height_map.raycast(ray)
}

fn is_pointer_over_ui(ui_interactions: &Query<&Interaction, With<Node>>) -> bool {
    ui_interactions
        .iter()
        .any(|interaction| matches!(*interaction, Interaction::Hovered | Interaction::Pressed))
}

/// Compute grid cells for a wall between two world positions.
/// Walks axis-aligned: first along X, then along Z (L-shape).
fn wall_layout_grid_cells(start: Vec3, end: Vec3) -> Vec<(i32, i32)> {
    let (sx, sz) = WallGrid::world_to_grid(start);
    let (ex, ez) = WallGrid::world_to_grid(end);

    let mut cells = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Walk along X first
    let dx = if ex > sx { 1 } else { -1 };
    let mut cx = sx;
    loop {
        if seen.insert((cx, sz)) {
            cells.push((cx, sz));
        }
        if cx == ex {
            break;
        }
        cx += dx;
    }

    // Then walk along Z
    let dz = if ez > sz { 1 } else { -1 };
    let mut cz = sz;
    loop {
        if seen.insert((ex, cz)) {
            cells.push((ex, cz));
        }
        if cz == ez {
            break;
        }
        cz += dz;
    }

    cells
}

/// Compute the cost for placing walls at the given grid cells.
/// Uses a merged grid (existing + proposed) to determine piece types.
fn wall_cost_from_cells(
    proposed: &[(i32, i32)],
    existing_grid: &WallGrid,
    registry: &BlueprintRegistry,
) -> crate::blueprints::ResourceCost {
    use crate::blueprints::ResourceCost;

    let mut total = ResourceCost::default();
    if proposed.is_empty() {
        return total;
    }

    // Build temporary merged set for neighbor lookups
    let mut merged: std::collections::HashSet<(i32, i32)> =
        existing_grid.cells.keys().copied().collect();
    for &cell in proposed {
        merged.insert(cell);
    }

    for &(gx, gz) in proposed {
        // Skip cells already in the grid
        if existing_grid.cells.contains_key(&(gx, gz)) {
            continue;
        }

        // Compute neighbor mask from merged set
        let mut mask = 0u8;
        for (i, (nx, nz)) in WallGrid::cardinal_neighbors(gx, gz).iter().enumerate() {
            if merged.contains(&(*nx, *nz)) {
                mask |= 1 << i;
            }
        }

        let (piece_kind, _) = auto_tile_piece(mask, false);
        let kind = piece_kind_to_entity_kind(piece_kind);
        let cost = &registry.get(kind).cost;

        for rt in ResourceType::ALL.iter() {
            total.set(*rt, total.get(*rt) + cost.get(*rt));
        }
    }
    total
}

fn clear_wall_preview(commands: &mut Commands, wall_preview: &mut WallPlotPreview) {
    for entity in wall_preview.ghost_entities.drain(..) {
        commands.entity(entity).try_despawn();
    }
    wall_preview.start = None;
    wall_preview.snapped_points.clear();
    wall_preview.total_cost = crate::blueprints::ResourceCost::default();
    wall_preview.valid = false;
}

fn floor_layout_grid_cells(start: Vec3, end: Vec3) -> Vec<(i32, i32)> {
    let (sx, sz) = WallGrid::world_to_grid(start);
    let (ex, ez) = WallGrid::world_to_grid(end);
    let min_x = sx.min(ex);
    let max_x = sx.max(ex);
    let min_z = sz.min(ez);
    let max_z = sz.max(ez);

    let mut cells = Vec::new();
    for gz in min_z..=max_z {
        for gx in min_x..=max_x {
            cells.push((gx, gz));
        }
    }
    cells
}

fn floor_cost_from_cells(
    cells: &[(i32, i32)],
    floor_grid: &FloorGrid,
    registry: &BlueprintRegistry,
) -> crate::blueprints::ResourceCost {
    use crate::blueprints::ResourceCost;

    let mut total = ResourceCost::default();
    let cost = &registry.get(EntityKind::Floor).cost;
    for cell in cells {
        if floor_grid.cells.contains_key(cell) {
            continue;
        }
        for rt in ResourceType::ALL.iter() {
            total.set(*rt, total.get(*rt) + cost.get(*rt));
        }
    }
    total
}

fn clear_floor_preview(commands: &mut Commands, floor_preview: &mut FloorPlotPreview) {
    for entity in floor_preview.ghost_entities.drain(..) {
        commands.entity(entity).try_despawn();
    }
    floor_preview.start = None;
    floor_preview.cells.clear();
    floor_preview.total_cost = crate::blueprints::ResourceCost::default();
    floor_preview.valid = false;
}

fn placement_kind(mode: PlacementMode) -> Option<EntityKind> {
    match mode {
        PlacementMode::Placing(kind) => Some(kind),
        PlacementMode::PlotBase => Some(EntityKind::Base),
        PlacementMode::None
        | PlacementMode::PlotWall { .. }
        | PlacementMode::PlotGate
        | PlacementMode::PlotFloor => None,
    }
}

pub(crate) fn update_placement_preview(
    mut commands: Commands,
    mut placement: ResMut<BuildingPlacementState>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    ghost_mats: Res<BuildingGhostMaterials>,
    building_models: Option<Res<BuildingModelAssets>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut meshes: ResMut<Assets<Mesh>>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    mut transform_sets: ParamSet<(
        Query<'_, '_, &mut Transform, (With<GhostBuilding>, Without<Building>)>,
        Query<
            '_,
            '_,
            (&GhostSawmillFencePiece, &mut Transform),
            (Without<GhostBuilding>, Without<Building>),
        >,
        Query<
            '_,
            '_,
            (
                &'static Transform,
                &'static BuildingFootprint,
                &'static EntityKind,
            ),
            (With<Building>, Without<GhostBuilding>),
        >,
        Query<
            '_,
            '_,
            (
                Entity,
                &'static Transform,
                &'static UnitState,
                &'static Faction,
                &'static EntityKind,
            ),
            (
                With<Unit>,
                Without<GhostBuilding>,
                Without<Building>,
                Without<GhostSawmillFencePiece>,
            ),
        >,
    )>,
    mut ghost_valid_q: Query<&mut GhostValid, With<GhostBuilding>>,
    active_player: Res<ActivePlayer>,
    environment: (Option<Res<BiomeMap>>, Res<HeightMap>, Res<ObstacleGrid>),
) {
    let (biome_map, height_map, obstacle_grid) = environment;
    let (camera_q, windows, graphics) = viewport;
    let Some(kind) = placement_kind(placement.mode) else {
        return;
    };

    // Handle rotation input (H = rotate left, J = rotate right) — 90 degree steps
    if keyboard.pressed(KeyCode::KeyH) {
        placement.rotation_y += 0.01;
    }
    if keyboard.pressed(KeyCode::KeyJ) {
        placement.rotation_y -= 0.01;
    }

    let bp = registry.get(kind);
    let is_gltf = bp.visual.mesh_kind.is_gltf();
    let half_h = if is_gltf {
        0.0
    } else {
        bp.building.as_ref().map(|b| b.half_height).unwrap_or(1.0)
    };
    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) else {
        return;
    };
    let new_footprint = footprint_for_kind(kind);

    // Spawn ghost if it doesn't exist
    if placement.preview_entity.is_none() {
        let ghost = if is_gltf {
            // Use actual GLTF building model for the ghost
            let mut ghost_cmds = commands.spawn((
                GhostBuilding,
                GhostValid(true),
                Transform::from_translation(Vec3::new(0.0, -100.0, 0.0)),
                Visibility::default(),
                NotShadowCaster,
                NotShadowReceiver,
            ));
            // Attach the GLTF scene as a child
            if let Some(ref models) = building_models {
                if let Some(scene_handle) =
                    models.scene_for(kind, 1, Vec3::new(world_pos.x, 0.0, world_pos.z))
                {
                    ghost_cmds.with_child((
                        SceneRoot(scene_handle),
                        models.child_transform(kind, 1.0),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
            if kind == EntityKind::Sawmill {
                spawn_sawmill_fence_preview(&mut ghost_cmds, &mut meshes);
            }
            ghost_cmds.id()
        } else {
            // Non-GLTF: use cache mesh with ghost material directly
            let mesh = cache.meshes.get(&kind).expect("Missing mesh").clone();
            commands
                .spawn((
                    GhostBuilding,
                    GhostValid(true),
                    Mesh3d(mesh),
                    MeshMaterial3d(ghost_mats.ghost_valid.clone()),
                    Transform::from_translation(Vec3::new(0.0, -100.0, 0.0)),
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id()
        };
        placement.preview_entity = Some(ghost);
    }

    // Spawn grid plane if it doesn't exist
    if placement.grid_plane_entity.is_none() {
        let grid_size = if kind == EntityKind::Sawmill {
            sawmill_preview_plane_size()
        } else {
            new_footprint * 2.5
        };
        // Number of grid cells across the plane (UV tiling)
        let uv_tiles = (grid_size / 2.0).max(2.0);
        let plane_mesh = meshes.add(build_grid_plane_mesh(grid_size, uv_tiles, &height_map));
        let grid_entity = commands
            .spawn((
                GhostGridPlane,
                Mesh3d(plane_mesh),
                MeshMaterial3d(ghost_mats.grid_plane.clone()),
                Transform::from_translation(Vec3::new(0.0, -100.0, 0.0)),
                NotShadowCaster,
                NotShadowReceiver,
            ))
            .id();
        placement.grid_plane_entity = Some(grid_entity);
    }

    let ground_y = if uses_terrain_foundation(kind) {
        height_map.foundation_target_height_shaped(world_pos.x, world_pos.z, new_footprint)
    } else {
        height_map.sample(world_pos.x, world_pos.z)
    };
    let y = ground_y + half_h;
    let ghost_translation = Vec3::new(world_pos.x, y, world_pos.z);
    let ghost_rotation = Quat::from_rotation_y(placement.rotation_y);

    let Some(ghost_entity) = placement.preview_entity else {
        return;
    };
    {
        let mut ghosts = transform_sets.p0();
        let Ok(mut ghost_tf) = ghosts.get_mut(ghost_entity) else {
            return;
        };
        ghost_tf.translation = ghost_translation;
        ghost_tf.rotation = ghost_rotation;
    }

    if kind == EntityKind::Sawmill {
        let mut fence_preview = transform_sets.p1();
        for (piece, mut transform) in &mut fence_preview {
            let world_anchor =
                ghost_translation + rotate_local_xz(piece.local_anchor, placement.rotation_y);
            let ground_y = height_map.sample(world_anchor.x, world_anchor.z);
            transform.translation = Vec3::new(
                piece.local_anchor.x,
                ground_y + piece.y_offset - ghost_translation.y,
                piece.local_anchor.z,
            );
            transform.rotation = Quat::from_rotation_y(piece.local_rotation_y);
        }
    }

    // Update grid plane position & mesh to align with terrain
    if let Some(grid_entity) = placement.grid_plane_entity {
        let grid_size = if kind == EntityKind::Sawmill {
            sawmill_preview_plane_size()
        } else {
            new_footprint * 2.5
        };
        let uv_tiles = (grid_size / 2.0).max(2.0);
        let new_mesh = meshes.add(build_grid_plane_mesh_at(
            world_pos.x,
            world_pos.z,
            grid_size,
            uv_tiles,
            &height_map,
        ));
        commands.entity(grid_entity).insert((
            Mesh3d(new_mesh),
            Transform::from_translation(Vec3::new(world_pos.x, 0.0, world_pos.z)),
        ));
    }

    let mut valid = true;
    let mut hint: Option<String> = None;

    if let Some(ref bm) = biome_map {
        let biome = bm.get_biome(world_pos.x, world_pos.z);
        if !is_biome_valid_for(kind, biome) {
            valid = false;
            hint = biome_requirement_text(kind).map(ToOwned::to_owned);
        }
    }

    // Slope validation (walls are exempt — they're designed for varied terrain)
    if !matches!(
        kind,
        EntityKind::WallSegment | EntityKind::WallPost | EntityKind::WallCorner
    ) {
        const MAX_BUILDING_SLOPE: f32 = 0.5; // ~27 degrees
        let slope = height_map.max_slope_under_footprint(world_pos.x, world_pos.z, new_footprint);
        if slope > MAX_BUILDING_SLOPE {
            valid = false;
            if hint.is_none() {
                hint = Some("Terrain too steep".to_owned());
            }
        }
    }

    {
        let existing_buildings = transform_sets.p2();
        for (building_tf, existing_footprint, existing_kind) in &existing_buildings {
            if !blocks_construction_overlap(*existing_kind) {
                continue;
            }
            if building_blocks_area(
                kind,
                ghost_translation,
                placement.rotation_y,
                new_footprint,
                *existing_kind,
                building_tf.translation,
                rotation_y_from_quat(building_tf.rotation),
                existing_footprint.0,
            ) {
                valid = false;
                break;
            }
        }
    }

    if building_area_blocked_by_obstacles(
        kind,
        Vec3::new(world_pos.x, 0.0, world_pos.z),
        placement.rotation_y,
        new_footprint,
        &obstacle_grid,
    ) {
        valid = false;
        if hint.is_none() {
            hint = Some("Blocked by trees".to_owned());
        }
    }

    let half_map = height_map.half_map;
    if world_pos.x.abs() > half_map - 5.0 || world_pos.z.abs() > half_map - 5.0 {
        valid = false;
    }

    if valid {
        let build_pos = Vec3::new(world_pos.x, 0.0, world_pos.z);
        let workers = transform_sets.p3();
        let worker_iter = workers
            .iter()
            .map(|(e, tf, state, fac, kind)| (e, tf, state, fac, kind));
        if !has_available_worker_for_build(worker_iter, active_player.0, build_pos) {
            valid = false;
            hint = Some("All workers are busy".to_owned());
        }
    }

    placement.hint_text = if !valid { hint } else { None };

    if let Ok(mut gv) = ghost_valid_q.get_mut(ghost_entity) {
        gv.0 = valid;
    }
}

/// Build a flat grid plane mesh centered at origin. Used as initial mesh (position set later).
fn build_grid_plane_mesh(size: f32, uv_tiles: f32, _height_map: &HeightMap) -> Mesh {
    build_grid_plane_mesh_at(0.0, 0.0, size, uv_tiles, _height_map)
}

/// Build a grid plane mesh that conforms to the terrain at the given world position.
/// The mesh is centered at (0,0,0) — the Transform positions it in world space.
fn build_grid_plane_mesh_at(
    cx: f32,
    cz: f32,
    size: f32,
    uv_tiles: f32,
    height_map: &HeightMap,
) -> Mesh {
    // Subdivide the plane into a grid so it follows terrain contour
    let subdivisions: u32 = 12;
    let verts_per_side = subdivisions + 1;
    let total_verts = (verts_per_side * verts_per_side) as usize;

    let mut positions = Vec::with_capacity(total_verts);
    let mut normals = Vec::with_capacity(total_verts);
    let mut uvs = Vec::with_capacity(total_verts);

    let half = size / 2.0;
    let step = size / subdivisions as f32;

    for iz in 0..verts_per_side {
        for ix in 0..verts_per_side {
            let lx = -half + ix as f32 * step;
            let lz = -half + iz as f32 * step;
            let wx = cx + lx;
            let wz = cz + lz;
            let wy = height_map.sample(wx, wz) + 0.15; // slight offset above terrain
            positions.push([lx, wy, lz]);
            normals.push([0.0, 1.0, 0.0]);
            let u = ix as f32 / subdivisions as f32 * uv_tiles;
            let v = iz as f32 / subdivisions as f32 * uv_tiles;
            uvs.push([u, v]);
        }
    }

    let mut indices = Vec::with_capacity((subdivisions * subdivisions * 6) as usize);
    for iz in 0..subdivisions {
        for ix in 0..subdivisions {
            let tl = iz * verts_per_side + ix;
            let tr = tl + 1;
            let bl = (iz + 1) * verts_per_side + ix;
            let br = bl + 1;
            indices.push(tl);
            indices.push(bl);
            indices.push(tr);
            indices.push(tr);
            indices.push(bl);
            indices.push(br);
        }
    }

    Mesh::new(
        bevy::render::render_resource::PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(bevy::mesh::Indices::U32(indices))
}

pub(crate) fn update_wall_plot_preview(
    mut commands: Commands,
    mut placement: ResMut<BuildingPlacementState>,
    mut wall_preview: ResMut<WallPlotPreview>,
    active_player: Res<ActivePlayer>,
    wall_grid: Res<WallGrid>,
    registry: Res<BlueprintRegistry>,
    ghost_mats: Res<BuildingGhostMaterials>,
    building_models: Option<Res<BuildingModelAssets>>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    existing_buildings: Query<
        (&Transform, &BuildingFootprint, &EntityKind),
        (With<Building>, Without<GhostBuilding>),
    >,
    workers: Query<
        (Entity, &Transform, &UnitState, &Faction, &EntityKind),
        (With<Unit>, Without<GhostBuilding>),
    >,
    height_map: Res<HeightMap>,
    obstacle_grid: Res<ObstacleGrid>,
) {
    let (camera_q, windows, graphics) = viewport;
    if !matches!(placement.mode, PlacementMode::PlotWall { .. }) {
        if wall_preview.start.is_some() || !wall_preview.ghost_entities.is_empty() {
            clear_wall_preview(&mut commands, &mut wall_preview);
        }
        return;
    }

    clear_wall_preview(&mut commands, &mut wall_preview);

    let start = match placement.mode {
        PlacementMode::PlotWall { start } if start != Vec3::ZERO => start,
        _ => {
            placement.hint_text = Some("Click ground to start wall".to_string());
            return;
        }
    };

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) else {
        return;
    };

    let cells = wall_layout_grid_cells(start, Vec3::new(world_pos.x, 0.0, world_pos.z));
    if cells.is_empty() {
        placement.hint_text = Some("Move cursor to plot wall".to_string());
        return;
    }

    // Filter out cells already occupied in the grid or blocked by obstacles
    let new_cells: Vec<(i32, i32)> = cells
        .iter()
        .copied()
        .filter(|c| !wall_grid.cells.contains_key(c) && !obstacle_grid.is_cell_blocked(c.0, c.1))
        .collect();

    if new_cells.is_empty() {
        placement.hint_text = Some("All cells already have walls".to_string());
        return;
    }

    // Store snapped world points for the confirm system
    wall_preview.start = Some(start);
    wall_preview.snapped_points = new_cells
        .iter()
        .map(|&(gx, gz)| WallGrid::grid_to_world(gx, gz))
        .collect();
    wall_preview.total_cost = wall_cost_from_cells(&new_cells, &wall_grid, &registry);
    wall_preview.valid = true;

    // Build merged set for neighbor lookups (existing grid + proposed cells)
    let mut merged: std::collections::HashSet<(i32, i32)> =
        wall_grid.cells.keys().copied().collect();
    for &cell in &new_cells {
        merged.insert(cell);
    }

    // Validate each cell and spawn ghost
    let half_map = height_map.half_map;
    for &(gx, gz) in &new_cells {
        let world = WallGrid::grid_to_world(gx, gz);
        let y = height_map.sample(world.x, world.z);

        // Check map bounds
        if world.x.abs() > half_map - 5.0 || world.z.abs() > half_map - 5.0 {
            wall_preview.valid = false;
        }

        // Check collision with non-wall buildings
        let fp = footprint_for_kind(EntityKind::WallPost);
        let blocked = existing_buildings
            .iter()
            .any(|(building_tf, existing_fp, existing_kind)| {
                if !blocks_construction_overlap(*existing_kind) {
                    return false;
                }
                let check_pos = Vec3::new(world.x, building_tf.translation.y, world.z);
                building_tf.translation.distance(check_pos) < existing_fp.0 + fp
            });
        if blocked {
            wall_preview.valid = false;
        }

        // Determine auto-tiled piece for this cell
        let mut mask = 0u8;
        for (i, (nx, nz)) in WallGrid::cardinal_neighbors(gx, gz).iter().enumerate() {
            if merged.contains(&(*nx, *nz)) {
                mask |= 1 << i;
            }
        }
        let (piece_kind, rotation_y) = auto_tile_piece(mask, false);
        let kind = piece_kind_to_entity_kind(piece_kind);

        // Spawn GLTF ghost with correct model and rotation
        let _ghost_mat = if wall_preview.valid {
            ghost_mats.ghost_valid.clone()
        } else {
            ghost_mats.ghost_invalid.clone()
        };

        let mut ghost_cmds = commands.spawn((
            GhostBuilding,
            GhostValid(wall_preview.valid),
            Transform::from_translation(Vec3::new(world.x, y, world.z))
                .with_rotation(Quat::from_rotation_y(rotation_y)),
            Visibility::default(),
            NotShadowCaster,
            NotShadowReceiver,
        ));

        if let Some(ref models) = building_models {
            if let Some(scene_handle) = models.scene_for(kind, 1, world) {
                ghost_cmds.with_child((
                    SceneRoot(scene_handle),
                    models.child_transform(kind, 1.0),
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
        }

        wall_preview.ghost_entities.push(ghost_cmds.id());
    }

    let count = new_cells.len();
    let cost = &wall_preview.total_cost;
    let wood = cost.get(ResourceType::Wood);
    let stone = cost.get(ResourceType::Stone);
    if wall_preview.valid {
        let wall_pos = wall_preview.snapped_points[0];
        let worker_iter = workers
            .iter()
            .map(|(e, tf, state, fac, kind)| (e, tf, state, fac, kind));
        if !has_available_worker_for_build(worker_iter, active_player.0, wall_pos) {
            wall_preview.valid = false;
        }
    }
    placement.hint_text = Some(if wall_preview.valid {
        format!("Wall: {count} pieces | Cost: {wood}W {stone}S")
    } else {
        "Wall path blocked or all workers are busy".to_string()
    });
}

pub(crate) fn update_gate_plot_preview(
    mut commands: Commands,
    mut placement: ResMut<BuildingPlacementState>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    ghost_mats: Res<BuildingGhostMaterials>,
    building_models: Option<Res<BuildingModelAssets>>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    mut ghosts: Query<&mut Transform, With<GhostBuilding>>,
    mut ghost_valid_q: Query<&mut GhostValid, With<GhostBuilding>>,
    wall_segments: Query<
        (&Transform, &Faction),
        (
            With<WallSegmentPiece>,
            With<Building>,
            Without<GhostBuilding>,
        ),
    >,
    workers: Query<
        (Entity, &Transform, &UnitState, &Faction, &EntityKind),
        (With<Unit>, Without<GhostBuilding>),
    >,
    active_player: Res<ActivePlayer>,
    height_map: Res<HeightMap>,
) {
    let (camera_q, windows, graphics) = viewport;
    if placement.mode != PlacementMode::PlotGate {
        return;
    }

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) else {
        return;
    };

    let kind = EntityKind::Gatehouse;
    let bp = registry.get(kind);
    let is_gltf = bp.visual.mesh_kind.is_gltf();
    if placement.preview_entity.is_none() {
        let ghost = if is_gltf {
            let mut ghost_cmds = commands.spawn((
                GhostBuilding,
                GhostValid(false),
                Transform::from_translation(Vec3::new(0.0, -100.0, 0.0)),
                Visibility::default(),
                NotShadowCaster,
                NotShadowReceiver,
            ));
            if let Some(ref models) = building_models {
                if let Some(scene_handle) =
                    models.scene_for(kind, 1, Vec3::new(world_pos.x, 0.0, world_pos.z))
                {
                    ghost_cmds.with_child((
                        SceneRoot(scene_handle),
                        models.child_transform(kind, 1.0),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
            ghost_cmds.id()
        } else {
            let mesh = cache.meshes.get(&kind).expect("Missing mesh").clone();
            commands
                .spawn((
                    GhostBuilding,
                    GhostValid(false),
                    Mesh3d(mesh),
                    MeshMaterial3d(ghost_mats.ghost_invalid.clone()),
                    Transform::from_translation(Vec3::new(0.0, -100.0, 0.0)),
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id()
        };
        placement.preview_entity = Some(ghost);
    }
    let nearest = wall_segments
        .iter()
        .filter(|(_, faction)| **faction == active_player.0)
        .filter_map(|(tf, _)| {
            let d = tf
                .translation
                .distance(Vec3::new(world_pos.x, tf.translation.y, world_pos.z));
            (d <= 6.0).then_some((tf, d))
        })
        .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let Some(ghost_entity) = placement.preview_entity else {
        return;
    };
    let Ok(mut ghost_tf) = ghosts.get_mut(ghost_entity) else {
        return;
    };

    if let Some((segment_tf, _)) = nearest {
        ghost_tf.translation = segment_tf.translation;
        ghost_tf.rotation = segment_tf.rotation;
        let worker_iter = workers
            .iter()
            .map(|(e, tf, state, fac, kind)| (e, tf, state, fac, kind));
        let has_worker =
            has_available_worker_for_build(worker_iter, active_player.0, segment_tf.translation);
        if let Ok(mut gv) = ghost_valid_q.get_mut(ghost_entity) {
            gv.0 = has_worker;
        }
        placement.hint_text = Some(if has_worker {
            "Click to replace wall segment with Gatehouse".to_string()
        } else {
            "All workers are busy".to_string()
        });
    } else {
        ghost_tf.translation = Vec3::new(world_pos.x, -100.0, world_pos.z);
        if let Ok(mut gv) = ghost_valid_q.get_mut(ghost_entity) {
            gv.0 = false;
        }
        placement.hint_text = Some("Gatehouse must replace an owned wall segment".to_string());
    }
}

pub(crate) fn update_floor_plot_preview(
    mut commands: Commands,
    mut placement: ResMut<BuildingPlacementState>,
    mut floor_preview: ResMut<FloorPlotPreview>,
    floor_grid: Res<FloorGrid>,
    registry: Res<BlueprintRegistry>,
    ghost_mats: Res<BuildingGhostMaterials>,
    cache: Res<EntityVisualCache>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    height_map: Res<HeightMap>,
    obstacle_grid: Res<ObstacleGrid>,
) {
    let (camera_q, windows, graphics) = viewport;
    if !matches!(placement.mode, PlacementMode::PlotFloor) {
        if floor_preview.start.is_some() || !floor_preview.ghost_entities.is_empty() {
            clear_floor_preview(&mut commands, &mut floor_preview);
        }
        return;
    }

    // Clear old brush indicator each frame
    clear_floor_preview(&mut commands, &mut floor_preview);

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) else {
        return;
    };

    let (gx, gz) = WallGrid::world_to_grid(Vec3::new(world_pos.x, 0.0, world_pos.z));
    let world = WallGrid::grid_to_world(gx, gz);

    let already_placed = floor_grid.cells.contains_key(&(gx, gz));
    let blocked = obstacle_grid.is_cell_blocked(gx, gz);
    let half_map = height_map.half_map;
    let out_of_bounds = world.x.abs() > half_map - 5.0 || world.z.abs() > half_map - 5.0;
    let valid = !already_placed && !blocked && !out_of_bounds;

    let y = height_map.sample(world.x, world.z) + 0.08;

    // Flat plane brush indicator — no side walls, no shadow artifacts
    let Some(mesh_handle) = cache.floor_brush_indicator.clone() else {
        return;
    };

    let mat = if valid {
        ghost_mats.ghost_valid.clone()
    } else {
        ghost_mats.ghost_invalid.clone()
    };

    let ghost = commands
        .spawn((
            GhostBuilding,
            GhostValid(valid),
            Mesh3d(mesh_handle),
            MeshMaterial3d(mat),
            Transform::from_translation(Vec3::new(world.x, y, world.z)),
            NotShadowCaster,
            NotShadowReceiver,
        ))
        .id();
    floor_preview.ghost_entities.push(ghost);

    if valid {
        let cost = registry.get(EntityKind::Floor).cost.clone();
        let wood = cost.get(ResourceType::Wood);
        let stone = cost.get(ResourceType::Stone);
        placement.hint_text = Some(format!("Floor brush | Cost per tile: {wood}W {stone}S"));
    } else if already_placed {
        placement.hint_text = Some("Already has floor".to_string());
    } else if blocked {
        placement.hint_text = Some("Blocked by resource".to_string());
    } else {
        placement.hint_text = Some("Out of bounds".to_string());
    }
}

/// Overrides materials on all mesh descendants of ghost buildings to ghost_valid/ghost_invalid.
pub(crate) fn apply_ghost_materials(
    mut commands: Commands,
    ghost_mats: Res<BuildingGhostMaterials>,
    ghosts: Query<(Entity, &GhostValid), With<GhostBuilding>>,
    children_q: Query<&Children>,
    mesh_q: Query<Entity, (With<Mesh3d>, Without<GhostMaterialApplied>)>,
    mut applied_q: Query<
        (Entity, &mut MeshMaterial3d<StandardMaterial>),
        With<GhostMaterialApplied>,
    >,
) {
    for (ghost_entity, ghost_valid) in &ghosts {
        let mat = if ghost_valid.0 {
            ghost_mats.ghost_valid.clone()
        } else {
            ghost_mats.ghost_invalid.clone()
        };

        // Walk all descendants and apply ghost material to mesh entities
        let mut stack = vec![ghost_entity];
        while let Some(entity) = stack.pop() {
            // New mesh entities that haven't been tagged yet
            if mesh_q.get(entity).is_ok() {
                commands.entity(entity).insert((
                    MeshMaterial3d(mat.clone()),
                    GhostMaterialApplied,
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
            // Already-tagged mesh entities: update material if validity changed
            if let Ok((_, mut existing_mat)) = applied_q.get_mut(entity) {
                existing_mat.0 = mat.clone();
            }
            // Recurse into children
            if let Ok(children) = children_q.get(entity) {
                for child in children {
                    stack.push(*child);
                }
            }
        }
    }
}

pub(crate) fn animate_placement_preview_vfx(
    time: Res<Time>,
    ghost_mats: Res<BuildingGhostMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    ghost_valid_q: Query<&GhostValid, With<GhostBuilding>>,
    mut grid_planes: Query<&mut Transform, With<GhostGridPlane>>,
) {
    let pulse = (time.elapsed_secs() * 3.6).sin() * 0.5 + 0.5;
    let intensity = 0.82 + pulse * 0.18;
    let invalid_active = ghost_valid_q.iter().any(|valid| !valid.0);

    if let Some(material) = materials.get_mut(&ghost_mats.ghost_valid) {
        material.base_color = Color::srgba(0.35, 0.98, 0.72, 0.28 + pulse * 0.1);
        material.emissive =
            LinearRgba::new(0.14 * intensity, 0.72 * intensity, 0.44 * intensity, 1.0);
    }

    if let Some(material) = materials.get_mut(&ghost_mats.ghost_invalid) {
        material.base_color = Color::srgba(1.0, 0.38, 0.22, 0.28 + pulse * 0.12);
        material.emissive =
            LinearRgba::new(0.82 * intensity, 0.18 * intensity, 0.08 * intensity, 1.0);
    }

    if let Some(material) = materials.get_mut(&ghost_mats.grid_plane) {
        let (base_color, emissive) = if invalid_active {
            (
                Color::srgba(1.0, 0.8, 0.72, 0.94),
                LinearRgba::new(0.58 * intensity, 0.17 * intensity, 0.08 * intensity, 1.0),
            )
        } else {
            (
                Color::srgba(0.82, 1.0, 0.96, 0.9),
                LinearRgba::new(0.16 * intensity, 0.56 * intensity, 0.42 * intensity, 1.0),
            )
        };
        material.base_color = base_color;
        material.emissive = emissive;
    }

    for mut transform in &mut grid_planes {
        transform.translation.y = 0.02 + pulse * 0.035;
    }
}

pub(crate) fn confirm_placement(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut online: PlacementOnlineParams,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    base_state: Res<FactionBaseState>,
    carried_totals: Res<CarriedResourceTotals>,
    mut pending_drains: ResMut<PendingCarriedDrains>,
    registry: Res<BlueprintRegistry>,
    extras: (
        Res<AllCompletedBuildings>,
        Option<Res<BiomeMap>>,
        Res<crate::simulation::ages::FactionAges>,
    ),
    height_map: Res<HeightMap>,
    queries: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
        Query<&Interaction, With<Node>>,
        Query<
            (&Transform, &BuildingFootprint, &Faction, &EntityKind),
            (With<Building>, Without<GhostBuilding>),
        >,
        Query<
            (
                Entity,
                &Transform,
                &UnitState,
                &Faction,
                &EntityKind,
                Option<&PendingBuildOrder>,
            ),
            With<Unit>,
        >,
    ),
    obstacle_grid: Res<ObstacleGrid>,
) {
    let (all_completed, biome_map, faction_ages) = extras;
    let (camera_q, windows, graphics, ui_interactions, existing_buildings, workers) = queries;
    let mode = placement.mode;
    let Some(kind) = placement_kind(mode) else {
        return;
    };

    let new_footprint = footprint_for_kind(kind);

    // Phase 1: awaiting initial mouse release
    if placement.awaiting_release {
        if mouse.just_released(MouseButton::Left) {
            placement.awaiting_release = false;
            return;
        } else {
            return;
        }
    } else if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    if is_pointer_over_ui(&ui_interactions) {
        return;
    }

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) else {
        return;
    };

    let bp = registry.get(kind);

    let faction = active_player.0;
    let has_base_started = base_state.is_founded(&faction)
        || existing_buildings
            .iter()
            .any(|(_, _, building_faction, building_kind)| {
                *building_faction == faction && *building_kind == EntityKind::Base
            })
        || workers
            .iter()
            .any(|(_, _, _, worker_faction, _, pending_order)| {
                *worker_faction == faction
                    && pending_order.is_some_and(|order| order.kind == EntityKind::Base)
            });

    let is_initial_base_plot =
        matches!(mode, PlacementMode::PlotBase) && kind == EntityKind::Base && !has_base_started;

    if matches!(mode, PlacementMode::PlotBase) && kind == EntityKind::Base && has_base_started {
        placement.hint_text = Some("Base is already being founded.".to_string());
        return;
    }

    // Check prerequisite
    let prereq_met = if let Some(ref bd) = bp.building {
        match (is_initial_base_plot, bd.prerequisite) {
            (true, _) => true,
            (false, None) => true,
            (false, Some(prereq_kind)) => {
                if prereq_kind == EntityKind::Base {
                    base_state.is_founded(&faction) || all_completed.has(&faction, prereq_kind)
                } else {
                    all_completed.has(&faction, prereq_kind)
                }
            }
        }
    } else {
        true
    };
    if !prereq_met {
        let prereq_name = bp
            .building
            .as_ref()
            .and_then(|bd| bd.prerequisite)
            .map(|k| k.display_name())
            .unwrap_or("prerequisite");
        placement.hint_text = Some(format!("Requires {prereq_name}"));
        return;
    }

    // Check age requirement
    let required_age = crate::simulation::ages::required_age_for_building(kind);
    let current_age = faction_ages.get_age(&faction);
    if current_age < required_age {
        placement.hint_text = Some(format!("Requires {}", required_age.display_name()));
        return;
    }

    // Check biome validity
    if let Some(ref bm) = biome_map {
        if !is_biome_valid_for(kind, bm.get_biome(world_pos.x, world_pos.z)) {
            return;
        }
    }
    // Slope validation (walls exempt)
    if !matches!(
        kind,
        EntityKind::WallSegment | EntityKind::WallPost | EntityKind::WallCorner
    ) {
        const MAX_BUILDING_SLOPE: f32 = 0.5;
        let slope = height_map.max_slope_under_footprint(world_pos.x, world_pos.z, new_footprint);
        if slope > MAX_BUILDING_SLOPE {
            return;
        }
    }
    for (building_tf, existing_fp, _, existing_kind) in &existing_buildings {
        if !blocks_construction_overlap(*existing_kind) {
            continue;
        }
        let dx = building_tf.translation.x - world_pos.x;
        let dz = building_tf.translation.z - world_pos.z;
        if (dx * dx + dz * dz).sqrt() < existing_fp.0 + new_footprint {
            return;
        }
    }
    if obstacle_grid.is_footprint_blocked(Vec3::new(world_pos.x, 0.0, world_pos.z), new_footprint) {
        return;
    }
    let half_map = height_map.half_map;
    if world_pos.x.abs() > half_map - 5.0 || world_pos.z.abs() > half_map - 5.0 {
        return;
    }

    // Find best available worker using priority-based selection
    let build_pos = Vec3::new(world_pos.x, 0.0, world_pos.z);
    let worker_iter = workers
        .iter()
        .map(|(e, tf, state, fac, kind, _)| (e, tf, state, fac, kind));
    let Some((worker_entity, _)) = find_best_worker_for_build(worker_iter, faction, build_pos)
    else {
        placement.hint_text = Some("No workers available!".to_string());
        return;
    };

    // Check affordability (stored + carried)
    let player_res = all_resources.get(&faction);
    let carried = carried_totals.get(&faction);
    if !bp.cost.can_afford_with_carried(player_res, carried) {
        placement.hint_text = Some("Not enough resources".to_string());
        return;
    }

    if *online.net_role == crate::infrastructure::multiplayer::NetRole::Client {
        let (Some(client), Some(ref mut socket)) = (
            online.client_state.as_ref(),
            online.matchbox_socket.as_mut(),
        ) else {
            return;
        };
        let seq = {
            let mut s = client.seq.lock().unwrap();
            *s += 1;
            *s
        };
        let msg = game_state::message::ClientMessage::Input {
            seq,
            timestamp: online.time.elapsed_secs_f64(),
            input: game_state::message::PlayerInput {
                player_id: client.player_id as u32,
                tick: 0,
                entity_ids: Vec::new(),
                commands: vec![game_state::message::InputCommand::Build {
                    kind: kind.to_index(),
                    position: [build_pos.x, build_pos.y, build_pos.z],
                }],
            },
        };
        crate::infrastructure::multiplayer::matchbox_transport::send_to_host(socket, &msg);

        if let Some(ghost) = placement.preview_entity {
            commands.entity(ghost).try_despawn();
        }
        if let Some(grid) = placement.grid_plane_entity {
            commands.entity(grid).try_despawn();
        }
        placement.mode = PlacementMode::None;
        placement.preview_entity = None;
        placement.grid_plane_entity = None;
        placement.hint_text = None;
        placement.rotation_y = 0.0;
        return;
    }

    // Deduct from stored first, queue carried drain for any deficit
    let player_res_mut = all_resources.get_mut(&faction);
    let deficits = bp.cost.deduct_with_carried(player_res_mut);
    let drain = SpendFromCarried {
        faction,
        amounts: deficits,
    };
    if drain.has_deficit() {
        pending_drains.drains.push(drain);
    }

    let rotation_y = placement.rotation_y;

    // Despawn ghost & grid plane
    if let Some(ghost) = placement.preview_entity {
        commands.entity(ghost).try_despawn();
    }
    if let Some(grid) = placement.grid_plane_entity {
        commands.entity(grid).try_despawn();
    }

    // Clean up any existing gathering assignment before reassigning
    if let Ok((_, _, w_state, _, _, _)) = workers.get(worker_entity) {
        cleanup_worker_assignment(&mut commands, worker_entity, w_state);
    }

    // Clear any stale combat order so resolve_combat_intents doesn't overwrite MoveTarget
    crate::simulation::combat::reset_combat_state(&mut commands, worker_entity);

    // Assign worker to move to the build site (building spawns on arrival)
    commands
        .entity(worker_entity)
        .remove::<MoveTarget>()
        .remove::<AttackTarget>()
        .insert(UnitState::MovingToPlot(build_pos))
        .insert(TaskSource::Manual)
        .insert(PendingBuildOrder {
            kind,
            position: build_pos,
            faction,
            rotation_y,
        })
        .insert(MoveTarget(build_pos));
    // Clear any queued tasks
    commands
        .entity(worker_entity)
        .entry::<TaskQueue>()
        .and_modify(|mut tq| tq.queue.clear());

    // Reset placement
    placement.mode = PlacementMode::None;
    placement.preview_entity = None;
    placement.grid_plane_entity = None;
    placement.hint_text = None;
    placement.rotation_y = 0.0;
}

pub(crate) fn confirm_wall_plot(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut wall_preview: ResMut<WallPlotPreview>,
    mut wall_grid: ResMut<WallGrid>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    ui_interactions: Query<&Interaction, With<Node>>,
    height_map: Res<HeightMap>,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    building_models: Option<Res<BuildingModelAssets>>,
    workers: Query<(Entity, &Transform, &UnitState, &Faction, &EntityKind), With<Unit>>,
    obstacle_grid: Res<ObstacleGrid>,
) {
    let (camera_q, windows, graphics) = viewport;
    if !matches!(placement.mode, PlacementMode::PlotWall { .. }) || placement.awaiting_release {
        return;
    }

    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    if is_pointer_over_ui(&ui_interactions) {
        return;
    }

    if let PlacementMode::PlotWall { start } = placement.mode {
        if start == Vec3::ZERO {
            if let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map)
            {
                // Snap start to grid
                let (gx, gz) = WallGrid::world_to_grid(Vec3::new(world_pos.x, 0.0, world_pos.z));
                let first = WallGrid::grid_to_world(gx, gz);
                wall_preview.start = Some(first);
                placement.mode = PlacementMode::PlotWall { start: first };
                placement.hint_text =
                    Some("Move cursor and click again to confirm wall".to_string());
            }
            return;
        }
    }

    if wall_preview.snapped_points.is_empty() || !wall_preview.valid {
        return;
    }

    let faction = active_player.0;
    let wall_pos = wall_preview.snapped_points[0];
    let worker_iter = workers
        .iter()
        .map(|(e, tf, state, fac, kind)| (e, tf, state, fac, kind));
    let Some((worker_entity, _)) = find_best_worker_for_build(worker_iter, faction, wall_pos)
    else {
        placement.hint_text = Some("All workers are busy".to_string());
        return;
    };

    let player_res = all_resources.get(&faction);
    if !wall_preview.total_cost.can_afford(player_res) {
        placement.hint_text = Some("Not enough resources for wall".to_string());
        return;
    }
    wall_preview
        .total_cost
        .deduct(all_resources.get_mut(&faction));

    // Convert snapped world points back to grid cells, filtering out any newly blocked
    let cells: Vec<(i32, i32)> = wall_preview
        .snapped_points
        .iter()
        .map(|p| WallGrid::world_to_grid(*p))
        .filter(|(gx, gz)| !obstacle_grid.is_cell_blocked(*gx, *gz))
        .collect();
    if cells.is_empty() {
        clear_wall_preview(&mut commands, &mut wall_preview);
        placement.mode = PlacementMode::None;
        placement.hint_text = Some("Wall path blocked by trees".to_string());
        return;
    }

    let spawned_entities = spawn_wall_grid_cells(
        &mut commands,
        &cache,
        &registry,
        building_models.as_deref(),
        &height_map,
        &mut wall_grid,
        faction,
        &cells,
    );

    if !spawned_entities.is_empty() {
        // Clean up any existing gathering assignment before reassigning
        if let Ok((_, _, w_state, _, _)) = workers.get(worker_entity) {
            cleanup_worker_assignment(&mut commands, worker_entity, w_state);
        }
        // Clear any stale combat order so resolve_combat_intents doesn't overwrite MoveTarget
        crate::simulation::combat::reset_combat_state(&mut commands, worker_entity);
        let target_building = spawned_entities[0];
        commands
            .entity(worker_entity)
            .remove::<AttackTarget>()
            .remove::<MoveTarget>()
            .insert(UnitState::MovingToBuild(target_building))
            .insert(TaskSource::Manual);
        crate::simulation::buildings::add_assigned_worker(
            &mut commands,
            target_building,
            worker_entity,
        );
    }

    clear_wall_preview(&mut commands, &mut wall_preview);
    placement.mode = PlacementMode::None;
    placement.preview_entity = None;
    placement.hint_text = None;
}

pub(crate) fn confirm_gate_plot(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut wall_grid: ResMut<WallGrid>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    ui_interactions: Query<&Interaction, With<Node>>,
    height_map: Res<HeightMap>,
    wall_segments: Query<
        (Entity, &Transform, &Faction, &WallGridCoord),
        (With<WallSegmentPiece>, With<Building>),
    >,
    registry: Res<BlueprintRegistry>,
    workers: Query<(Entity, &Transform, &UnitState, &Faction, &EntityKind), With<Unit>>,
) {
    let (camera_q, windows, graphics) = viewport;
    if placement.mode != PlacementMode::PlotGate || !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    if is_pointer_over_ui(&ui_interactions) {
        return;
    }

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) else {
        return;
    };

    // Find nearest owned straight wall segment
    let Some((segment_entity, segment_tf, faction, grid_coord)) = wall_segments
        .iter()
        .filter(|(_, _, faction, _)| **faction == active_player.0)
        .filter_map(|(entity, tf, faction, coord)| {
            let d = tf
                .translation
                .distance(Vec3::new(world_pos.x, tf.translation.y, world_pos.z));
            (d <= 6.0).then_some((entity, tf, faction, coord, d))
        })
        .min_by(|(_, _, _, _, a), (_, _, _, _, b)| {
            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(e, tf, faction, coord, _)| (e, tf, faction, coord))
    else {
        placement.hint_text = Some("Gatehouse must replace an owned wall segment".to_string());
        return;
    };

    let bp = registry.get(EntityKind::Gatehouse);
    let worker_entity = workers
        .iter()
        .filter(|(_, _, state, worker_faction, kind)| {
            **kind == EntityKind::Worker
                && **worker_faction == *faction
                && matches!(
                    state,
                    UnitState::Idle
                        | UnitState::Gathering(_)
                        | UnitState::ReturningToDeposit { .. }
                        | UnitState::Depositing { .. }
                        | UnitState::WaitingForStorage { .. }
                        | UnitState::WaitingForDepot { .. }
                        | UnitState::Moving(_)
                )
        })
        .min_by(|(_, a_tf, _, _, _), (_, b_tf, _, _, _)| {
            let a_dist = a_tf.translation.distance(segment_tf.translation);
            let b_dist = b_tf.translation.distance(segment_tf.translation);
            a_dist
                .partial_cmp(&b_dist)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(worker, _, _, _, _)| worker);
    let Some(worker_entity) = worker_entity else {
        placement.hint_text = Some("All workers are busy".to_string());
        return;
    };

    let player_res = all_resources.get(faction);
    if !bp.cost.can_afford(player_res) {
        placement.hint_text = Some("Not enough resources for Gatehouse".to_string());
        return;
    }
    bp.cost.deduct(all_resources.get_mut(faction));

    // Mark the grid cell as a gate — auto-tile system will swap the model
    let (gx, gz) = (grid_coord.0, grid_coord.1);
    if let Some(cell) = wall_grid.cells.get_mut(&(gx, gz)) {
        cell.is_gate = true;
    }
    wall_grid.mark_dirty(gx, gz);

    if let Some(preview) = placement.preview_entity.take() {
        commands.entity(preview).try_despawn();
    }
    // Clear any stale combat order so resolve_combat_intents doesn't overwrite MoveTarget
    crate::simulation::combat::reset_combat_state(&mut commands, worker_entity);
    commands
        .entity(worker_entity)
        .remove::<AttackTarget>()
        .remove::<MoveTarget>()
        .insert(UnitState::MovingToBuild(segment_entity))
        .insert(TaskSource::Manual);
    crate::simulation::buildings::add_assigned_worker(&mut commands, segment_entity, worker_entity);

    placement.mode = PlacementMode::None;
    placement.awaiting_release = false;
    placement.hint_text = None;
}

pub(crate) fn confirm_floor_plot(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut floor_preview: ResMut<FloorPlotPreview>,
    mut floor_grid: ResMut<FloorGrid>,
    mut all_resources: ResMut<AllPlayerResources>,
    active_player: Res<ActivePlayer>,
    viewport: (
        Query<(&Camera, &GlobalTransform), With<RtsCamera>>,
        Query<&Window, With<PrimaryWindow>>,
        Res<GraphicsSettings>,
    ),
    ui_interactions: Query<&Interaction, With<Node>>,
    mut height_map: ResMut<HeightMap>,
    resources_and_queries: (
        Res<EntityVisualCache>,
        Res<BlueprintRegistry>,
        Res<ObstacleGrid>,
    ),
    ground_q: Query<&Mesh3d, With<Ground>>,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_and_decos: (
        ResMut<crate::world::ground::TerrainShapeSyncState>,
        ResMut<TerrainSurfaceDirtyQueue>,
    ),
    bush_decorations: Query<(Entity, &Transform), With<Decoration>>,
) {
    let (cache, registry, obstacle_grid) = resources_and_queries;
    let (mut sync_state, mut dirty_areas) = terrain_and_decos;
    let (camera_q, windows, graphics) = viewport;
    if !matches!(placement.mode, PlacementMode::PlotFloor) || placement.awaiting_release {
        return;
    }

    // Brush mode: paint while holding left mouse button
    if !mouse.pressed(MouseButton::Left) {
        return;
    }

    if is_pointer_over_ui(&ui_interactions) {
        return;
    }

    let Some(world_pos) = cursor_ground_pos(&camera_q, &windows, &graphics, &height_map) else {
        return;
    };

    let (gx, gz) = WallGrid::world_to_grid(Vec3::new(world_pos.x, 0.0, world_pos.z));
    let world = WallGrid::grid_to_world(gx, gz);

    // Skip if already placed, blocked, or out of bounds
    if floor_grid.cells.contains_key(&(gx, gz)) {
        return;
    }
    if obstacle_grid.is_cell_blocked(gx, gz) {
        return;
    }
    let half_map = height_map.half_map;
    if world.x.abs() > half_map - 5.0 || world.z.abs() > half_map - 5.0 {
        return;
    }

    let faction = active_player.0;
    let cells = vec![(gx, gz)];
    let cell_cost = floor_cost_from_cells(&cells, &floor_grid, &registry);
    let player_res = all_resources.get(&faction);
    if !cell_cost.can_afford(player_res) {
        placement.hint_text = Some("Not enough resources".to_string());
        return;
    }
    cell_cost.deduct(all_resources.get_mut(&faction));

    let cx = world.x;
    let cz = world.z;
    let footprint = footprint_for_kind(EntityKind::Floor);
    let shared_height = height_map.foundation_target_height_shaped(cx, cz, footprint);

    // Flatten terrain for single cell
    let op = TerrainShapeOp {
        center: [cx, cz],
        footprint,
        target_height: shared_height,
    };
    let (_, outer_radius) = foundation_radii(op.footprint, height_map.step);

    let changed = apply_terrain_shape_op(&mut height_map, &op);
    sync_state.applied_history.insert(op.clone());
    sync_state.applied_history_ordered.push(op.clone());
    sync_state.pending_network.push(op);

    if changed {
        if let Ok(ground_mesh) = ground_q.single() {
            if let Some(mesh) = meshes.get_mut(&ground_mesh.0) {
                let op_min_x = (((cx - outer_radius) + half_map) / height_map.step)
                    .floor()
                    .max(0.0) as usize;
                let op_max_x = (((cx + outer_radius) + half_map) / height_map.step)
                    .ceil()
                    .min((height_map.grid_size - 1) as f32) as usize;
                let op_min_z = (((cz - outer_radius) + half_map) / height_map.step)
                    .floor()
                    .max(0.0) as usize;
                let op_max_z = (((cz + outer_radius) + half_map) / height_map.step)
                    .ceil()
                    .min((height_map.grid_size - 1) as f32) as usize;
                let norm_min_x = op_min_x.saturating_sub(1);
                let norm_max_x = (op_max_x + 1).min(height_map.grid_size - 1);
                let norm_min_z = op_min_z.saturating_sub(1);
                let norm_max_z = (op_max_z + 1).min(height_map.grid_size - 1);
                sync_ground_mesh_partial(
                    mesh,
                    &height_map,
                    norm_min_x,
                    norm_max_x,
                    norm_min_z,
                    norm_max_z,
                );
            }
        }
    }

    dirty_areas.pending.push_back(TerrainSurfaceDirtyArea {
        center: Vec2::new(cx, cz),
        radius: outer_radius,
    });

    spawn_floor_grid_cells(
        &mut commands,
        &cache,
        &height_map,
        &mut floor_grid,
        faction,
        &cells,
        Some(shared_height),
    );

    // Paint floor stone texture on the ground mesh
    if let Ok(ground_mesh) = ground_q.single() {
        if let Some(mesh) = meshes.get_mut(&ground_mesh.0) {
            let floor_cell_half = WALL_CELL_SIZE * 0.5;
            let transition = WALL_CELL_SIZE * 0.8;
            crate::world::ground::paint_floor_blend_on_ground(
                mesh,
                &height_map,
                cx,
                cz,
                floor_cell_half,
                transition,
            );
        }
    }

    // Despawn bush decorations inside the painted cell
    let clear_r2 = (footprint + 2.0) * (footprint + 2.0);
    for (deco_entity, deco_tf) in &bush_decorations {
        let dx = deco_tf.translation.x - cx;
        let dz = deco_tf.translation.z - cz;
        if dx * dx + dz * dz <= clear_r2 {
            commands.entity(deco_entity).try_despawn();
        }
    }
}

pub(crate) fn cancel_placement(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut placement: ResMut<BuildingPlacementState>,
    mut wall_preview: ResMut<WallPlotPreview>,
    mut floor_preview: ResMut<FloorPlotPreview>,
) {
    if placement.mode == PlacementMode::None {
        return;
    }

    if mouse.just_pressed(MouseButton::Right) || keyboard.just_pressed(KeyCode::Escape) {
        if let Some(preview) = placement.preview_entity {
            commands.entity(preview).try_despawn();
        }
        if let Some(grid) = placement.grid_plane_entity {
            commands.entity(grid).try_despawn();
        }
        clear_wall_preview(&mut commands, &mut wall_preview);
        clear_floor_preview(&mut commands, &mut floor_preview);
        placement.mode = PlacementMode::None;
        placement.preview_entity = None;
        placement.grid_plane_entity = None;
        placement.awaiting_release = false;
        placement.hint_text = None;
        placement.rotation_y = 0.0;
    }
}
