//! Environment systems: terrain sync, vegetation clearing, auras, storage, sawmill yard.

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

use crate::blueprints::{BlueprintRegistry, EntityKind};
use crate::types::*;
use crate::world::ground::{playable_half_map, HeightMap, TerrainSurfaceDirtyArea, TerrainSurfaceDirtyQueue};

use super::{
    rotate_local_xz, rotation_y_from_quat, sawmill_tree_slot_world, sawmill_yard_corners,
};

#[derive(Component)]
pub(crate) struct SawmillFencePiece {
    owner: Entity,
    local_anchor: Vec3,
    local_rotation_y: f32,
    y_offset: f32,
}

// ── Obstacle grid sync ──

pub(super) fn sync_obstacle_grid(
    mut grid: ResMut<ObstacleGrid>,
    trees: Query<&Transform, Or<(With<Sapling>, With<GrowingTree>, With<MatureTree>)>>,
    resource_nodes: Query<
        &Transform,
        (
            With<ResourceNode>,
            Without<Sapling>,
            Without<GrowingTree>,
            Without<MatureTree>,
        ),
    >,
    height_map: Res<HeightMap>,
) {
    grid.cells.clear();
    for tf in &trees {
        let (gx, gz) = WallGrid::world_to_grid(tf.translation);
        grid.cells.insert((gx, gz));
    }
    // Resource nodes (iron, copper, trees, etc.) also block floor/wall placement
    for tf in &resource_nodes {
        let (gx, gz) = WallGrid::world_to_grid(tf.translation);
        grid.cells.insert((gx, gz));
    }
    // Compute playable boundary from border hill settings
    grid.playable_half = playable_half_map(height_map.map_size);
}

pub(super) fn sync_environment_to_terrain_changes(
    mut commands: Commands,
    mut dirty_areas: ResMut<TerrainSurfaceDirtyQueue>,
    height_map: Res<HeightMap>,
    registry: Res<BlueprintRegistry>,
    mut terrain_followers: ParamSet<(
        Query<
            '_,
            '_,
            &mut Transform,
            (
                Or<(With<Sapling>, With<GrowingTree>, With<MatureTree>)>,
                Without<Building>,
            ),
        >,
        Query<
            '_,
            '_,
            (&mut Transform, &ResourceNode, Option<&TerrainHeightOffset>),
            (
                Without<Building>,
                Without<Sapling>,
                Without<GrowingTree>,
                Without<MatureTree>,
            ),
        >,
        Query<
            '_,
            '_,
            (
                &mut Transform,
                &GrowingResource,
                Option<&TerrainHeightOffset>,
            ),
            (
                Without<Building>,
                Without<Sapling>,
                Without<GrowingTree>,
                Without<MatureTree>,
            ),
        >,
        Query<
            '_,
            '_,
            (Entity, &DecoChunk, &Transform, &Mesh3d),
            (
                Without<Building>,
                Without<Sapling>,
                Without<GrowingTree>,
                Without<MatureTree>,
            ),
        >,
    )>,
    mut building_q: Query<(&mut Transform, &EntityKind, &BuildingFootprint), With<Building>>,
    grass_chunks: Query<(Entity, &GrassChunk, &Mesh3d)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    while let Some(area) = dirty_areas.pending.pop_front() {
        sync_building_heights_for_area(&height_map, &registry, area, &mut building_q);
        sync_tree_heights_for_area(&height_map, area, &mut terrain_followers.p0());
        sync_resource_node_heights_for_area(&height_map, area, &mut terrain_followers.p1());
        sync_growing_resource_heights_for_area(&height_map, area, &mut terrain_followers.p2());
        clear_vegetation_in_radius(
            &mut commands,
            &grass_chunks,
            &terrain_followers.p3(),
            &mut meshes,
            area.center.x,
            area.center.y,
            area.radius + 2.0,
        );
    }
}

fn sync_building_heights_for_area(
    height_map: &HeightMap,
    registry: &BlueprintRegistry,
    area: TerrainSurfaceDirtyArea,
    building_q: &mut Query<(&mut Transform, &EntityKind, &BuildingFootprint), With<Building>>,
) {
    let area_r2 = area.radius * area.radius;

    for (mut transform, kind, footprint) in building_q.iter_mut() {
        let dx = transform.translation.x - area.center.x;
        let dz = transform.translation.z - area.center.y;
        let reach = area.radius + footprint.0;
        if dx * dx + dz * dz > reach * reach {
            continue;
        }

        let bp = registry.get(*kind);
        let half_height =
            if bp.visual.mesh_kind.is_gltf() && !bp.visual.mesh_kind.is_gltf_character() {
                0.0
            } else {
                bp.building.as_ref().map(|b| b.half_height).unwrap_or(0.0)
            };
        let ground_y = if super::uses_terrain_foundation(*kind) {
            height_map.foundation_target_height_shaped(
                transform.translation.x,
                transform.translation.z,
                footprint.0,
            )
        } else {
            height_map.sample(transform.translation.x, transform.translation.z)
        };

        if (transform.translation.y - (ground_y + half_height)).abs() > 0.001
            || dx * dx + dz * dz <= area_r2
        {
            transform.translation.y = ground_y + half_height;
        }
    }
}

fn sync_tree_heights_for_area(
    height_map: &HeightMap,
    area: TerrainSurfaceDirtyArea,
    trees: &mut Query<
        &mut Transform,
        (
            Or<(With<Sapling>, With<GrowingTree>, With<MatureTree>)>,
            Without<Building>,
        ),
    >,
) {
    let area_r2 = area.radius * area.radius;

    for mut transform in trees.iter_mut() {
        let dx = transform.translation.x - area.center.x;
        let dz = transform.translation.z - area.center.y;
        if dx * dx + dz * dz > area_r2 {
            continue;
        }

        transform.translation.y =
            height_map.sample(transform.translation.x, transform.translation.z);
    }
}

fn sync_resource_node_heights_for_area(
    height_map: &HeightMap,
    area: TerrainSurfaceDirtyArea,
    resource_nodes: &mut Query<
        (&mut Transform, &ResourceNode, Option<&TerrainHeightOffset>),
        (
            Without<Building>,
            Without<Sapling>,
            Without<GrowingTree>,
            Without<MatureTree>,
        ),
    >,
) {
    let area_r2 = area.radius * area.radius;

    for (mut transform, node, offset) in resource_nodes.iter_mut() {
        let dx = transform.translation.x - area.center.x;
        let dz = transform.translation.z - area.center.y;
        if dx * dx + dz * dz > area_r2 {
            continue;
        }

        let terrain_offset = offset
            .map(|o| o.0)
            .unwrap_or_else(|| default_resource_height_offset(node.resource_type));
        transform.translation.y =
            height_map.sample(transform.translation.x, transform.translation.z) + terrain_offset;
    }
}

fn sync_growing_resource_heights_for_area(
    height_map: &HeightMap,
    area: TerrainSurfaceDirtyArea,
    growing_resources: &mut Query<
        (
            &mut Transform,
            &GrowingResource,
            Option<&TerrainHeightOffset>,
        ),
        (
            Without<Building>,
            Without<Sapling>,
            Without<GrowingTree>,
            Without<MatureTree>,
        ),
    >,
) {
    let area_r2 = area.radius * area.radius;

    for (mut transform, growing, offset) in growing_resources.iter_mut() {
        let dx = transform.translation.x - area.center.x;
        let dz = transform.translation.z - area.center.y;
        if dx * dx + dz * dz > area_r2 {
            continue;
        }

        let terrain_offset = offset
            .map(|o| o.0)
            .unwrap_or_else(|| default_resource_height_offset(growing.resource_type));
        transform.translation.y =
            height_map.sample(transform.translation.x, transform.translation.z) + terrain_offset;
    }
}

fn default_resource_height_offset(resource_type: ResourceType) -> f32 {
    match resource_type {
        ResourceType::Oil => 0.6,
        _ => 0.0,
    }
}

// ── Aura Systems ──

pub(super) fn healing_aura_system(
    time: Res<Time>,
    teams: Res<TeamConfig>,
    auras: Query<(&Transform, &HealingAura, &BuildingState, &Faction), With<Building>>,
    mut healable: Query<(&Transform, &mut Health, &Faction), Without<Building>>,
) {
    for (aura_tf, aura, state, aura_faction) in &auras {
        if *state != BuildingState::Complete {
            continue;
        }
        for (unit_tf, mut health, faction) in &mut healable {
            if !teams.is_allied(aura_faction, faction) {
                continue;
            }
            let dist = aura_tf.translation.distance(unit_tf.translation);
            if dist <= aura.range && health.current < health.max {
                health.current =
                    (health.current + aura.heal_per_sec * time.delta_secs()).min(health.max);
            }
        }
    }
}

/// Returns the highest gather speed bonus from any StorageAura in range of the given position.
pub fn storage_aura_bonus(
    worker_pos: Vec3,
    auras: &Query<(&Transform, &StorageAura, &BuildingState), With<Building>>,
) -> f32 {
    let mut bonus = 0.0f32;
    for (aura_tf, aura, state) in auras {
        if *state != BuildingState::Complete {
            continue;
        }
        let dist = aura_tf.translation.distance(worker_pos);
        if dist <= aura.range {
            bonus = bonus.max(aura.gather_speed_bonus); // Don't stack, take highest
        }
    }
    bonus
}

// ── Sync Storage on Spend ──

pub(super) fn sync_storage_on_spend(
    all_resources: Res<AllPlayerResources>,
    mut storages: Query<(&Faction, &mut StorageInventory), (With<Building>, With<DepositPoint>)>,
) {
    // For each faction, sum up all storage inventories per resource type.
    // If the total exceeds AllPlayerResources (meaning player spent some),
    // drain from the largest inventory first.
    use std::collections::HashMap;

    // Collect per-faction storage totals
    let mut faction_totals: HashMap<Faction, [u32; ResourceType::COUNT]> = HashMap::new();
    for (faction, inv) in &storages {
        let totals = faction_totals
            .entry(*faction)
            .or_insert([0; ResourceType::COUNT]);
        for rt in ResourceType::ALL {
            totals[rt.index()] += inv.get(rt);
        }
    }

    // For each faction, check if inventories exceed player resources
    for (faction, totals) in &faction_totals {
        let player_res = all_resources.get(faction);
        let mut excess = [0u32; ResourceType::COUNT];
        for rt in ResourceType::ALL {
            excess[rt.index()] = totals[rt.index()].saturating_sub(player_res.get(rt));
        }

        if excess.iter().all(|&e| e == 0) {
            continue;
        }

        // Drain excess from inventories (proportionally)
        let mut remaining = excess;
        for (f, mut inv) in &mut storages {
            if f != faction {
                continue;
            }
            for rt in ResourceType::ALL {
                let i = rt.index();
                let drain = remaining[i].min(inv.get(rt));
                inv.amounts[i] -= drain;
                remaining[i] -= drain;
            }
        }
    }
}

// ── Storage Pile Visuals ──

pub(super) fn update_storage_piles(
    mut commands: Commands,
    pile_assets: Option<Res<StoragePileAssets>>,
    height_map: Res<HeightMap>,
    mut storages: Query<
        (
            Entity,
            &Transform,
            &mut StorageInventory,
            Option<&ResourcePileVisuals>,
        ),
        (With<Building>, With<DepositPoint>),
    >,
) {
    let Some(assets) = pile_assets else { return };

    for (entity, transform, mut inventory, pile_visuals) in &mut storages {
        let new_total = inventory.total();
        if new_total == inventory.last_total {
            continue;
        }
        inventory.last_total = new_total;

        // Despawn old pile visuals
        if let Some(piles) = pile_visuals {
            for pile_entity in &piles.entities {
                commands.entity(*pile_entity).try_despawn();
            }
        }

        let mut pile_entities = Vec::new();

        // Collect accepted resource types that have items stored
        let accepted = inventory.accepted_types();
        let stored: Vec<ResourceType> = accepted
            .iter()
            .copied()
            .filter(|rt| inventory.get(*rt) > 0)
            .collect();

        if stored.is_empty() {
            commands.entity(entity).insert(ResourcePileVisuals {
                entities: pile_entities,
            });
            continue;
        }

        // Place all piles on one side (East) in an inner grid layout
        let side_offset = 4.0; // distance from building center to pile side
        let grid_spacing = 1.2; // spacing between piles in the grid
        let max_cols = 3;

        for (idx, rt) in stored.iter().enumerate() {
            let amount = inventory.get(*rt);
            let cap = inventory.cap_for(*rt);
            let fill_ratio = (amount as f32 / cap.max(1) as f32).min(1.0);
            let scale = fill_ratio * 0.8 + 0.2;
            let half_pile_height = scale * 0.5;

            // Grid position: row and column within the side
            let col = (idx % max_cols) as f32;
            let row = (idx / max_cols) as f32;
            let grid_width = (stored.len().min(max_cols) as f32 - 1.0) * grid_spacing;
            let local_x = side_offset;
            let local_z = col * grid_spacing - grid_width * 0.5 + row * grid_spacing * 0.5;

            let (mesh, mat) = match rt {
                ResourceType::Wood => (
                    assets.cube_mesh.clone(),
                    assets.materials.get(rt).cloned().unwrap_or_default(),
                ),
                ResourceType::Gold => (
                    assets.sphere_mesh.clone(),
                    assets.materials.get(rt).cloned().unwrap_or_default(),
                ),
                ResourceType::Oil => (
                    assets.cylinder_mesh.clone(),
                    assets.materials.get(rt).cloned().unwrap_or_default(),
                ),
                _ => (
                    assets.cube_mesh.clone(),
                    assets.materials.get(rt).cloned().unwrap_or_default(),
                ),
            };

            let world_x = transform.translation.x + local_x;
            let world_z = transform.translation.z + local_z;
            let ground_y = height_map.sample(world_x, world_z);

            let pile = commands
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(mat),
                    Transform::from_translation(Vec3::new(
                        world_x,
                        ground_y + half_pile_height,
                        world_z,
                    ))
                    .with_scale(Vec3::splat(scale)),
                    NotShadowCaster,
                    NotShadowReceiver,
                ))
                .id();
            pile_entities.push(pile);
        }

        commands.entity(entity).insert(ResourcePileVisuals {
            entities: pile_entities,
        });
    }
}

// ── Sawmill Tree Yard ──

pub const SAWMILL_YARD_OFFSET: Vec3 = Vec3::new(5.5, 0.0, 0.0);
const SAWMILL_MINI_TREE_SCALE: f32 = 0.06;

fn sawmill_yard_max_trees(level: u8) -> u8 {
    // Level 1: 2 trees, Level 2: 3, Level 3: 4
    1 + level
}

pub const SAWMILL_TREE_SLOTS: [Vec3; 4] = [
    Vec3::new(-0.8, 0.0, -0.8),
    Vec3::new(0.8, 0.0, -0.8),
    Vec3::new(-0.8, 0.0, 0.8),
    Vec3::new(0.8, 0.0, 0.8),
];

pub(super) fn sawmill_yard_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    height_map: Res<HeightMap>,
    model_assets: Option<Res<ModelAssets>>,
    new_sawmills: Query<
        (Entity, &Transform, &BuildingLevel, &BuildingState),
        (With<Building>, Without<SawmillYard>),
    >,
    mut upgraded_sawmills: Query<
        (Entity, &Transform, &BuildingLevel, &mut SawmillYard),
        (With<Building>, Changed<BuildingLevel>),
    >,
    kind_q: Query<&EntityKind>,
) {
    let fence_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.35, 0.22, 0.1),
        perceptual_roughness: 0.9,
        ..default()
    });

    // Spawn yard for newly completed sawmills
    for (entity, transform, level, state) in &new_sawmills {
        if *state != BuildingState::Complete {
            continue;
        }
        let Ok(kind) = kind_q.get(entity) else {
            continue;
        };
        if *kind != EntityKind::Sawmill {
            continue;
        }

        let rotation_y = rotation_y_from_quat(transform.rotation);
        let mut fence_entities = Vec::new();
        let corners = sawmill_yard_corners(transform.translation, rotation_y);

        let post_mesh = meshes.add(Cylinder::new(0.05, 0.8));
        let rail_meshes = [
            meshes.add(Cuboid::new(4.0, 0.06, 0.06)),
            meshes.add(Cuboid::new(4.0, 0.06, 0.06)),
            meshes.add(Cuboid::new(4.0, 0.06, 0.06)),
            meshes.add(Cuboid::new(4.0, 0.06, 0.06)),
        ];

        for world_pos in corners.iter() {
            let ground_y = height_map.sample(world_pos.x, world_pos.z);
            let local_anchor = rotate_local_xz(*world_pos - transform.translation, -rotation_y);
            let post = commands
                .spawn((
                    Mesh3d(post_mesh.clone()),
                    MeshMaterial3d(fence_mat.clone()),
                    Transform::from_translation(Vec3::new(
                        world_pos.x,
                        ground_y + 0.4,
                        world_pos.z,
                    )),
                    NotShadowCaster,
                    NotShadowReceiver,
                    GameWorld,
                    SawmillFencePiece {
                        owner: entity,
                        local_anchor,
                        local_rotation_y: 0.0,
                        y_offset: 0.4,
                    },
                ))
                .id();
            fence_entities.push(post);
        }

        let rail_height_offsets = [0.25, 0.55];
        for i in 0..4 {
            let a = corners[i];
            let b = corners[(i + 1) % 4];
            let mid = (a + b) * 0.5;
            let span = (b - a).length();
            let ground_y = height_map.sample(mid.x, mid.z);
            let local_anchor = rotate_local_xz(mid - transform.translation, -rotation_y);
            let local_dir = rotate_local_xz(b - a, -rotation_y);
            let local_angle = local_dir.z.atan2(local_dir.x);

            for &h in &rail_height_offsets {
                let rail = commands
                    .spawn((
                        Mesh3d(rail_meshes[i].clone()),
                        MeshMaterial3d(fence_mat.clone()),
                        Transform::from_translation(Vec3::new(
                            mid.x,
                            ground_y + h,
                            mid.z,
                        ))
                        .with_rotation(Quat::from_rotation_y(rotation_y + local_angle))
                        .with_scale(Vec3::new(span / 4.0, 1.0, 1.0)),
                        NotShadowCaster,
                        NotShadowReceiver,
                        GameWorld,
                        SawmillFencePiece {
                            owner: entity,
                            local_anchor,
                            local_rotation_y: local_angle,
                            y_offset: h,
                        },
                    ))
                    .id();
                fence_entities.push(rail);
            }
        }

        // Spawn initial mini trees (as harvestable ResourceNodes)
        let max_trees = sawmill_yard_max_trees(level.0);
        let mut tree_entities = Vec::new();
        spawn_mini_trees(
            &mut commands,
            &height_map,
            model_assets.as_deref(),
            transform.translation,
            rotation_y,
            0,
            max_trees,
            &mut tree_entities,
            entity,
        );

        commands.entity(entity).insert(SawmillYard {
            fence_entities,
            tree_entities,
            current_tree_count: max_trees,
        });
    }

    // Handle level upgrades — add more trees
    for (entity, transform, level, mut yard) in &mut upgraded_sawmills {
        let Ok(kind) = kind_q.get(entity) else {
            continue;
        };
        if *kind != EntityKind::Sawmill {
            continue;
        }

        let new_max = sawmill_yard_max_trees(level.0);
        if new_max <= yard.current_tree_count {
            continue;
        }

        let rotation_y = rotation_y_from_quat(transform.rotation);
        spawn_mini_trees(
            &mut commands,
            &height_map,
            model_assets.as_deref(),
            transform.translation,
            rotation_y,
            yard.current_tree_count,
            new_max,
            &mut yard.tree_entities,
            entity,
        );
        yard.current_tree_count = new_max;
    }
}

const YARD_TREE_WOOD_AMOUNT: u32 = 80;

fn spawn_mini_trees(
    commands: &mut Commands,
    height_map: &HeightMap,
    model_assets: Option<&ModelAssets>,
    building_pos: Vec3,
    rotation_y: f32,
    from_slot: u8,
    to_slot: u8,
    tree_entities: &mut Vec<Entity>,
    sawmill_entity: Entity,
) {
    let Some(assets) = model_assets else { return };
    if assets.trees.is_empty() {
        return;
    }

    for slot_idx in from_slot..to_slot.min(SAWMILL_TREE_SLOTS.len() as u8) {
        let world_pos = sawmill_tree_slot_world(building_pos, rotation_y, slot_idx as usize);
        let ground_y = height_map.sample(world_pos.x, world_pos.z);

        // Pick a random tree variant based on slot index for determinism
        let tree_idx = slot_idx as usize % assets.trees.len();
        let scene = assets.trees[tree_idx].clone();
        let y_rotation = (slot_idx as f32) * 1.57; // vary rotation per slot

        let tree = commands
            .spawn((
                SceneRoot(scene),
                Transform::from_translation(Vec3::new(world_pos.x, ground_y, world_pos.z))
                    .with_rotation(Quat::from_rotation_y(rotation_y + y_rotation))
                    .with_scale(Vec3::splat(SAWMILL_MINI_TREE_SCALE)),
                NotShadowCaster,
                GameWorld,
                ResourceNode {
                    resource_type: ResourceType::Wood,
                    amount_remaining: YARD_TREE_WOOD_AMOUNT,
                },
                YardResourceNode(sawmill_entity),
            ))
            .id();
        tree_entities.push(tree);
    }
}

pub(super) fn sync_sawmill_fence_pieces(
    height_map: Res<HeightMap>,
    sawmills: Query<&Transform, (With<Building>, With<SawmillYard>, Without<SawmillFencePiece>)>,
    mut fence_pieces: Query<(&SawmillFencePiece, &mut Transform), Without<Building>>,
) {
    for (piece, mut transform) in &mut fence_pieces {
        let Ok(owner_tf) = sawmills.get(piece.owner) else {
            continue;
        };
        let rotation_y = rotation_y_from_quat(owner_tf.rotation);
        let world_anchor = owner_tf.translation + rotate_local_xz(piece.local_anchor, rotation_y);
        let ground_y = height_map.sample(world_anchor.x, world_anchor.z);
        transform.translation = Vec3::new(world_anchor.x, ground_y + piece.y_offset, world_anchor.z);
        transform.rotation = Quat::from_rotation_y(rotation_y + piece.local_rotation_y);
    }
}

// ── Yard tree regrowth ──

const YARD_TREE_REGROW_SECS: f32 = 20.0;

/// Regrows depleted yard trees over time so the sawmill has a renewable wood supply.
pub(super) fn yard_tree_regrowth_system(
    time: Res<Time>,
    sawmills: Query<(&SawmillYard, &BuildingState), With<Building>>,
    mut nodes: Query<&mut ResourceNode, With<YardResourceNode>>,
    mut timers: Local<std::collections::HashMap<Entity, f32>>,
) {
    for (yard, state) in &sawmills {
        if *state != BuildingState::Complete {
            continue;
        }
        for &tree_entity in &yard.tree_entities {
            let Ok(mut node) = nodes.get_mut(tree_entity) else {
                continue;
            };
            if node.amount_remaining > 0 {
                timers.remove(&tree_entity);
                continue;
            }
            let elapsed = timers.entry(tree_entity).or_insert(0.0);
            *elapsed += time.delta_secs();
            if *elapsed >= YARD_TREE_REGROW_SECS {
                node.amount_remaining = YARD_TREE_WOOD_AMOUNT;
                timers.remove(&tree_entity);
            }
        }
    }
}

// ── Vegetation clearing around buildings ──

/// Removes grass and decoration geometry within a building's footprint.
///
/// Operates on merged chunk meshes: for each triangle whose centroid falls
/// inside the clear radius, the triangle's indices are dropped and the mesh
/// is rebuilt.  Empty chunks are despawned entirely.
/// Max buildings to clear vegetation for per frame — spreads GPU re-upload
/// cost when many buildings spawn at once (e.g. AI wall lines).
const VEGETATION_CLEAR_BUDGET: usize = 2;
/// Chunk world-space size used for AABB pre-filtering (must match resources.rs).
const VEG_CHUNK_SIZE: f32 = 32.0;

pub(super) fn clear_vegetation_around_buildings(
    mut commands: Commands,
    new_buildings: Query<
        (Entity, &Transform, &BuildingFootprint),
        (With<Building>, Without<VegetationCleared>),
    >,
    grass_chunks: Query<(Entity, &GrassChunk, &Mesh3d)>,
    deco_chunks: Query<
        (Entity, &DecoChunk, &Transform, &Mesh3d),
        (
            Without<Building>,
            Without<Sapling>,
            Without<GrowingTree>,
            Without<MatureTree>,
        ),
    >,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mut processed = 0;
    for (building_entity, building_tf, footprint) in &new_buildings {
        if processed >= VEGETATION_CLEAR_BUDGET {
            break;
        }
        processed += 1;

        let bx = building_tf.translation.x;
        let bz = building_tf.translation.z;
        // Clear a bit beyond the footprint so there is a visible gap.
        clear_vegetation_in_radius(
            &mut commands,
            &grass_chunks,
            &deco_chunks,
            &mut meshes,
            bx,
            bz,
            footprint.0 + 2.0,
        );

        commands.entity(building_entity).insert(VegetationCleared);
    }
}

fn clear_vegetation_in_radius(
    commands: &mut Commands,
    grass_chunks: &Query<(Entity, &GrassChunk, &Mesh3d)>,
    deco_chunks: &Query<
        (Entity, &DecoChunk, &Transform, &Mesh3d),
        (
            Without<Building>,
            Without<Sapling>,
            Without<GrowingTree>,
            Without<MatureTree>,
        ),
    >,
    meshes: &mut Assets<Mesh>,
    bx: f32,
    bz: f32,
    clear_radius: f32,
) {
    let clear_r2 = clear_radius * clear_radius;

    // Grass chunks store vertices directly in world space.
    for (chunk_entity, chunk, mesh_handle) in grass_chunks.iter() {
        let chunk_cx = (chunk.chunk_x as f32 + 0.5) * VEG_CHUNK_SIZE;
        let chunk_cz = (chunk.chunk_z as f32 + 0.5) * VEG_CHUNK_SIZE;
        let half = VEG_CHUNK_SIZE * 0.5 + clear_radius;
        if (bx - chunk_cx).abs() > half || (bz - chunk_cz).abs() > half {
            continue;
        }

        let needs_strip = {
            let Some(mesh) = meshes.get(&mesh_handle.0) else {
                continue;
            };
            has_triangles_in_radius(mesh, bx, bz, clear_r2, 0.0, 0.0)
        };
        if !needs_strip {
            continue;
        }

        let Some(mesh) = meshes.get_mut(&mesh_handle.0) else {
            continue;
        };
        if strip_triangles_in_radius(mesh, bx, bz, clear_r2, 0.0, 0.0) {
            commands.entity(chunk_entity).despawn();
        }
    }

    // Deco chunks store vertices relative to the chunk transform.
    for (chunk_entity, chunk, chunk_tf, mesh_handle) in deco_chunks.iter() {
        let ox = chunk_tf.translation.x;
        let oz = chunk_tf.translation.z;
        let chunk_cx = (chunk.chunk_x as f32 + 0.5) * VEG_CHUNK_SIZE;
        let chunk_cz = (chunk.chunk_z as f32 + 0.5) * VEG_CHUNK_SIZE;
        let half = VEG_CHUNK_SIZE * 0.5 + clear_radius;
        if (bx - chunk_cx).abs() > half || (bz - chunk_cz).abs() > half {
            continue;
        }

        let needs_strip = {
            let Some(mesh) = meshes.get(&mesh_handle.0) else {
                continue;
            };
            has_triangles_in_radius(mesh, bx, bz, clear_r2, ox, oz)
        };
        if !needs_strip {
            continue;
        }

        let Some(mesh) = meshes.get_mut(&mesh_handle.0) else {
            continue;
        };
        if strip_triangles_in_radius(mesh, bx, bz, clear_r2, ox, oz) {
            commands.entity(chunk_entity).despawn();
        }
    }
}

/// Read-only check: are there any triangles whose centroid falls within radius?
/// Does NOT clone data or mutate the mesh — much cheaper than strip_triangles_in_radius.
fn has_triangles_in_radius(
    mesh: &Mesh,
    cx: f32,
    cz: f32,
    radius_sq: f32,
    offset_x: f32,
    offset_z: f32,
) -> bool {
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return false;
    };
    let Some(bevy::mesh::Indices::U32(indices)) = mesh.indices() else {
        return false;
    };
    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
            continue;
        }
        let ax = (positions[i0][0] + positions[i1][0] + positions[i2][0]) / 3.0 + offset_x;
        let az = (positions[i0][2] + positions[i1][2] + positions[i2][2]) / 3.0 + offset_z;
        let dx = ax - cx;
        let dz = az - cz;
        if dx * dx + dz * dz <= radius_sq {
            return true;
        }
    }
    false
}

fn strip_triangles_in_radius(
    mesh: &mut Mesh,
    cx: f32,
    cz: f32,
    radius_sq: f32,
    offset_x: f32,
    offset_z: f32,
) -> bool {
    let positions: Vec<[f32; 3]> = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(bevy::mesh::VertexAttributeValues::Float32x3(v)) => v.clone(),
        _ => return false,
    };

    let old_indices: Vec<u32> = match mesh.indices() {
        Some(bevy::mesh::Indices::U32(v)) => v.clone(),
        _ => return false,
    };

    if old_indices.len() % 3 != 0 {
        return false;
    }

    let mut new_indices = Vec::with_capacity(old_indices.len());
    for tri in old_indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
            new_indices.extend_from_slice(tri);
            continue;
        }
        let ax = (positions[i0][0] + positions[i1][0] + positions[i2][0]) / 3.0 + offset_x;
        let az = (positions[i0][2] + positions[i1][2] + positions[i2][2]) / 3.0 + offset_z;
        let dx = ax - cx;
        let dz = az - cz;
        if dx * dx + dz * dz > radius_sq {
            new_indices.extend_from_slice(tri);
        }
    }

    if new_indices.is_empty() {
        return true;
    }

    mesh.insert_indices(bevy::mesh::Indices::U32(new_indices));
    false
}
