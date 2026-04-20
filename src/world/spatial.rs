//! `SpatialPlugin`: spatial hashing and broad-phase spatial queries used
//! by targeting, collision checks, and AI range scans.

use bevy::ecs::lifecycle::RemovedComponents;
use bevy::prelude::*;
use std::collections::HashMap;

use crate::infrastructure::net_bridge::NetworkId;
use crate::types::*;

pub struct SpatialPlugin;

impl Plugin for SpatialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpatialHashGrid>()
            .init_resource::<WallSpatialGrid>()
            .add_systems(
                OnEnter(AppState::InGame),
                (seed_spatial_grid, seed_wall_grid),
            )
            .add_systems(
                FixedUpdate,
                (
                    update_spatial_grid,
                    remove_spatial_grid_entities,
                    update_wall_grid,
                    remove_wall_grid_entities,
                )
                    .before(crate::simulation::combat::CombatSet::Approach)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Resource)]
pub struct SpatialHashGrid {
    pub inv_cell_size: f32,
    pub cells: HashMap<IVec2, Vec<(Entity, Vec3, u32)>>,
    pub entity_cells: HashMap<Entity, IVec2>,
}

impl Default for SpatialHashGrid {
    fn default() -> Self {
        Self {
            inv_cell_size: 1.0 / 15.0,
            cells: HashMap::new(),
            entity_cells: HashMap::new(),
        }
    }
}

impl SpatialHashGrid {
    fn cell_key(&self, pos: Vec3) -> IVec2 {
        IVec2::new(
            (pos.x * self.inv_cell_size).floor() as i32,
            (pos.z * self.inv_cell_size).floor() as i32,
        )
    }

    pub fn insert(&mut self, entity: Entity, pos: Vec3, stable_id: Option<u32>) {
        let key = self.cell_key(pos);
        self.cells
            .entry(key)
            .or_default()
            .push((entity, pos, stable_id.unwrap_or(u32::MAX)));
        self.entity_cells.insert(entity, key);
    }

    pub fn remove(&mut self, entity: Entity) {
        let Some(key) = self.entity_cells.remove(&entity) else {
            return;
        };
        if let Some(entries) = self.cells.get_mut(&key) {
            entries.retain(|(stored, _, _)| *stored != entity);
            if entries.is_empty() {
                self.cells.remove(&key);
            }
        }
    }

    pub fn upsert(&mut self, entity: Entity, pos: Vec3, stable_id: Option<u32>) {
        let stable_id = stable_id.unwrap_or(u32::MAX);
        let key = self.cell_key(pos);
        if let Some(current_key) = self.entity_cells.get(&entity).copied() {
            if current_key == key {
                if let Some(entries) = self.cells.get_mut(&key) {
                    if let Some((_, stored_pos, stored_id)) =
                        entries.iter_mut().find(|(stored, _, _)| *stored == entity)
                    {
                        *stored_pos = pos;
                        *stored_id = stable_id;
                        return;
                    }
                }
            } else {
                self.remove(entity);
            }
        }
        self.cells
            .entry(key)
            .or_default()
            .push((entity, pos, stable_id));
        self.entity_cells.insert(entity, key);
    }

    pub fn collect_radius(&self, pos: Vec3, radius: f32, out: &mut Vec<(Entity, Vec3)>) {
        let radius_sq = radius * radius;
        let min_x = ((pos.x - radius) * self.inv_cell_size).floor() as i32;
        let max_x = ((pos.x + radius) * self.inv_cell_size).floor() as i32;
        let min_z = ((pos.z - radius) * self.inv_cell_size).floor() as i32;
        let max_z = ((pos.z + radius) * self.inv_cell_size).floor() as i32;

        out.clear();
        for cx in min_x..=max_x {
            for cz in min_z..=max_z {
                if let Some(entries) = self.cells.get(&IVec2::new(cx, cz)) {
                    for &(entity, epos, _) in entries {
                        let dx = epos.x - pos.x;
                        let dz = epos.z - pos.z;
                        if dx * dx + dz * dz <= radius_sq {
                            out.push((entity, epos));
                        }
                    }
                }
            }
        }
        out.sort_by(|a, b| {
            stable_entity_order(a.0, a.1, b.0, b.1, &self.entity_cells, &self.cells)
        });
    }

    pub fn query_radius(&self, pos: Vec3, radius: f32) -> Vec<(Entity, Vec3)> {
        let mut results = Vec::new();
        self.collect_radius(pos, radius, &mut results);
        results
    }

    /// Radius query with a caller-supplied predicate evaluated per candidate.
    ///
    /// Intended as the back-end for faction-filtered targeting, perception,
    /// and aura queries — callers look up the faction (or any other tag)
    /// for each candidate via a sidecar [`bevy::ecs::system::Query`] and
    /// keep or skip it. Using this helper instead of iterating a raw
    /// `Query<&Transform, With<…>>` avoids an O(N) scan per targeting
    /// system and is the canonical entry point for Pass 8-style spatial
    /// migrations.
    pub fn collect_radius_filter<F>(
        &self,
        pos: Vec3,
        radius: f32,
        out: &mut Vec<(Entity, Vec3)>,
        mut keep: F,
    ) where
        F: FnMut(Entity) -> bool,
    {
        out.clear();
        let radius_sq = radius * radius;
        let min_x = ((pos.x - radius) * self.inv_cell_size).floor() as i32;
        let max_x = ((pos.x + radius) * self.inv_cell_size).floor() as i32;
        let min_z = ((pos.z - radius) * self.inv_cell_size).floor() as i32;
        let max_z = ((pos.z + radius) * self.inv_cell_size).floor() as i32;
        for cx in min_x..=max_x {
            for cz in min_z..=max_z {
                if let Some(entries) = self.cells.get(&IVec2::new(cx, cz)) {
                    for &(entity, epos, _) in entries {
                        let dx = epos.x - pos.x;
                        let dz = epos.z - pos.z;
                        if dx * dx + dz * dz > radius_sq {
                            continue;
                        }
                        if keep(entity) {
                            out.push((entity, epos));
                        }
                    }
                }
            }
        }
        out.sort_by(|a, b| {
            stable_entity_order(a.0, a.1, b.0, b.1, &self.entity_cells, &self.cells)
        });
    }

    pub fn collect_radius_limited(
        &self,
        pos: Vec3,
        radius: f32,
        limit: usize,
        out: &mut Vec<(Entity, Vec3)>,
    ) {
        self.collect_radius(pos, radius, out);
        out.sort_by(|a, b| {
            let da = (a.1.x - pos.x).powi(2) + (a.1.z - pos.z).powi(2);
            let db = (b.1.x - pos.x).powi(2) + (b.1.z - pos.z).powi(2);
            da.total_cmp(&db).then_with(|| {
                stable_entity_order(a.0, a.1, b.0, b.1, &self.entity_cells, &self.cells)
            })
        });
        out.truncate(limit);
    }

    pub fn query_radius_limited(
        &self,
        pos: Vec3,
        radius: f32,
        limit: usize,
    ) -> Vec<(Entity, Vec3)> {
        let mut results = Vec::new();
        self.collect_radius_limited(pos, radius, limit, &mut results);
        results
    }

    pub fn collect_corridor_limited(
        &self,
        from: Vec3,
        to: Vec3,
        half_width: f32,
        limit: usize,
        out: &mut Vec<(Entity, Vec3)>,
        scratch: &mut Vec<(Entity, Vec3)>,
    ) {
        let delta = Vec2::new(to.x - from.x, to.z - from.z);
        let length = delta.length().max(0.001);
        let dir = delta / length;
        self.collect_radius_limited(
            from.lerp(to, 0.5),
            length * 0.5 + half_width,
            limit * 3,
            scratch,
        );
        out.clear();
        for &(entity, pos) in scratch.iter() {
            let rel = Vec2::new(pos.x - from.x, pos.z - from.z);
            let forward = rel.dot(dir);
            if forward < -half_width || forward > length + half_width {
                continue;
            }
            let closest = dir * forward;
            let lateral = (rel - closest).length();
            if lateral <= half_width {
                out.push((entity, pos));
            }
        }
        out.sort_by(|a, b| {
            let da = (a.1.x - from.x).powi(2) + (a.1.z - from.z).powi(2);
            let db = (b.1.x - from.x).powi(2) + (b.1.z - from.z).powi(2);
            da.total_cmp(&db).then_with(|| {
                stable_entity_order(a.0, a.1, b.0, b.1, &self.entity_cells, &self.cells)
            })
        });
        out.truncate(limit);
    }

    pub fn query_corridor_limited(
        &self,
        from: Vec3,
        to: Vec3,
        half_width: f32,
        limit: usize,
    ) -> Vec<(Entity, Vec3)> {
        let mut results = Vec::new();
        let mut scratch = Vec::new();
        self.collect_corridor_limited(from, to, half_width, limit, &mut results, &mut scratch);
        results
    }
}

#[derive(Resource)]
pub struct WallSpatialGrid {
    pub inv_cell_size: f32,
    pub cells: HashMap<IVec2, Vec<(Entity, Vec3, f32, Faction, u32)>>, // entity, pos, footprint, faction, stable id
    pub entity_cells: HashMap<Entity, IVec2>,
}

impl Default for WallSpatialGrid {
    fn default() -> Self {
        Self {
            inv_cell_size: 1.0 / 5.0,
            cells: HashMap::new(),
            entity_cells: HashMap::new(),
        }
    }
}

impl WallSpatialGrid {
    fn cell_key(&self, pos: Vec3) -> IVec2 {
        IVec2::new(
            (pos.x * self.inv_cell_size).floor() as i32,
            (pos.z * self.inv_cell_size).floor() as i32,
        )
    }

    pub fn collect_radius(
        &self,
        pos: Vec3,
        radius: f32,
        out: &mut Vec<(Entity, Vec3, f32, Faction)>,
    ) {
        let radius_sq = radius * radius;
        let min_x = ((pos.x - radius) * self.inv_cell_size).floor() as i32;
        let max_x = ((pos.x + radius) * self.inv_cell_size).floor() as i32;
        let min_z = ((pos.z - radius) * self.inv_cell_size).floor() as i32;
        let max_z = ((pos.z + radius) * self.inv_cell_size).floor() as i32;

        out.clear();
        for cx in min_x..=max_x {
            for cz in min_z..=max_z {
                if let Some(entries) = self.cells.get(&IVec2::new(cx, cz)) {
                    for &(entity, epos, fp, faction, _) in entries {
                        let dx = epos.x - pos.x;
                        let dz = epos.z - pos.z;
                        if dx * dx + dz * dz <= radius_sq {
                            out.push((entity, epos, fp, faction));
                        }
                    }
                }
            }
        }
        out.sort_by(|a, b| {
            stable_wall_order(
                (a.0, a.1, a.2, a.3),
                (b.0, b.1, b.2, b.3),
                &self.entity_cells,
                &self.cells,
            )
        });
    }

    pub fn query_radius(&self, pos: Vec3, radius: f32) -> Vec<(Entity, Vec3, f32, Faction)> {
        let mut results = Vec::new();
        self.collect_radius(pos, radius, &mut results);
        results
    }

    pub fn upsert(
        &mut self,
        entity: Entity,
        pos: Vec3,
        footprint: f32,
        faction: Faction,
        stable_id: Option<u32>,
    ) {
        let stable_id = stable_id.unwrap_or(u32::MAX);
        let key = self.cell_key(pos);
        if let Some(current_key) = self.entity_cells.get(&entity).copied() {
            if current_key == key {
                if let Some(entries) = self.cells.get_mut(&key) {
                    if let Some((_, stored_pos, stored_fp, stored_faction, stored_id)) = entries
                        .iter_mut()
                        .find(|(stored, _, _, _, _)| *stored == entity)
                    {
                        *stored_pos = pos;
                        *stored_fp = footprint;
                        *stored_faction = faction;
                        *stored_id = stable_id;
                        return;
                    }
                }
            } else {
                self.remove(entity);
            }
        }
        self.cells
            .entry(key)
            .or_default()
            .push((entity, pos, footprint, faction, stable_id));
        self.entity_cells.insert(entity, key);
    }

    pub fn remove(&mut self, entity: Entity) {
        let Some(key) = self.entity_cells.remove(&entity) else {
            return;
        };
        if let Some(entries) = self.cells.get_mut(&key) {
            entries.retain(|(stored, _, _, _, _)| *stored != entity);
            if entries.is_empty() {
                self.cells.remove(&key);
            }
        }
    }
}

fn ordered_bits(value: f32) -> u32 {
    let bits = value.to_bits();
    if bits & 0x8000_0000 == 0 {
        bits | 0x8000_0000
    } else {
        !bits
    }
}

fn stable_entity_key(
    entity: Entity,
    pos: Vec3,
    entity_cells: &HashMap<Entity, IVec2>,
    cells: &HashMap<IVec2, Vec<(Entity, Vec3, u32)>>,
) -> (u32, u32, u32, u32) {
    let stable_id = entity_cells
        .get(&entity)
        .and_then(|key| cells.get(key))
        .and_then(|entries| {
            entries
                .iter()
                .find(|(stored, _, _)| *stored == entity)
                .map(|(_, _, stable_id)| *stable_id)
        })
        .unwrap_or(u32::MAX);
    (
        stable_id,
        ordered_bits(pos.x),
        ordered_bits(pos.y),
        ordered_bits(pos.z),
    )
}

fn stable_entity_order(
    left_entity: Entity,
    left_pos: Vec3,
    right_entity: Entity,
    right_pos: Vec3,
    entity_cells: &HashMap<Entity, IVec2>,
    cells: &HashMap<IVec2, Vec<(Entity, Vec3, u32)>>,
) -> std::cmp::Ordering {
    stable_entity_key(left_entity, left_pos, entity_cells, cells).cmp(&stable_entity_key(
        right_entity,
        right_pos,
        entity_cells,
        cells,
    ))
}

fn stable_wall_key(
    entity: Entity,
    pos: Vec3,
    footprint: f32,
    faction: Faction,
    entity_cells: &HashMap<Entity, IVec2>,
    cells: &HashMap<IVec2, Vec<(Entity, Vec3, f32, Faction, u32)>>,
) -> (u32, u8, u32, u32, u32, u32) {
    let stable_id = entity_cells
        .get(&entity)
        .and_then(|key| cells.get(key))
        .and_then(|entries| {
            entries
                .iter()
                .find(|(stored, _, _, _, _)| *stored == entity)
                .map(|(_, _, _, _, stable_id)| *stable_id)
        })
        .unwrap_or(u32::MAX);
    (
        stable_id,
        faction.to_net_index(),
        ordered_bits(pos.x),
        ordered_bits(pos.y),
        ordered_bits(pos.z),
        ordered_bits(footprint),
    )
}

fn stable_wall_order(
    left: (Entity, Vec3, f32, Faction),
    right: (Entity, Vec3, f32, Faction),
    entity_cells: &HashMap<Entity, IVec2>,
    cells: &HashMap<IVec2, Vec<(Entity, Vec3, f32, Faction, u32)>>,
) -> std::cmp::Ordering {
    stable_wall_key(left.0, left.1, left.2, left.3, entity_cells, cells).cmp(&stable_wall_key(
        right.0,
        right.1,
        right.2,
        right.3,
        entity_cells,
        cells,
    ))
}

fn seed_spatial_grid(
    mut grid: ResMut<SpatialHashGrid>,
    units: Query<(Entity, &Transform, Option<&NetworkId>), With<Unit>>,
    mobs: Query<(Entity, &Transform, Option<&NetworkId>), (With<Mob>, Without<Unit>)>,
    buildings: Query<
        (Entity, &Transform, Option<&NetworkId>),
        (
            With<Building>,
            Without<Unit>,
            Without<Mob>,
            Without<FloorTile>,
        ),
    >,
) {
    grid.cells.clear();
    grid.entity_cells.clear();
    for (entity, tf, network_id) in &units {
        grid.insert(entity, tf.translation, network_id.map(|id| id.0));
    }
    for (entity, tf, network_id) in &mobs {
        grid.insert(entity, tf.translation, network_id.map(|id| id.0));
    }
    for (entity, tf, network_id) in &buildings {
        grid.insert(entity, tf.translation, network_id.map(|id| id.0));
    }
}

fn update_spatial_grid(
    mut grid: ResMut<SpatialHashGrid>,
    units: Query<
        (Entity, &Transform, Option<&NetworkId>),
        (
            With<Unit>,
            Or<(
                Added<Unit>,
                Changed<Transform>,
                Added<NetworkId>,
                Changed<NetworkId>,
            )>,
        ),
    >,
    mobs: Query<
        (Entity, &Transform, Option<&NetworkId>),
        (
            (With<Mob>, Without<Unit>),
            Or<(
                Added<Mob>,
                Changed<Transform>,
                Added<NetworkId>,
                Changed<NetworkId>,
            )>,
        ),
    >,
    buildings: Query<
        (Entity, &Transform, Option<&NetworkId>),
        (
            With<Building>,
            Without<Unit>,
            Without<Mob>,
            Without<FloorTile>,
            Or<(
                Added<Building>,
                Changed<Transform>,
                Added<NetworkId>,
                Changed<NetworkId>,
            )>,
        ),
    >,
) {
    for (entity, tf, network_id) in &units {
        grid.upsert(entity, tf.translation, network_id.map(|id| id.0));
    }
    for (entity, tf, network_id) in &mobs {
        grid.upsert(entity, tf.translation, network_id.map(|id| id.0));
    }
    for (entity, tf, network_id) in &buildings {
        grid.upsert(entity, tf.translation, network_id.map(|id| id.0));
    }
}

fn remove_spatial_grid_entities(
    mut grid: ResMut<SpatialHashGrid>,
    mut removed_units: RemovedComponents<Unit>,
    mut removed_mobs: RemovedComponents<Mob>,
    mut removed_buildings: RemovedComponents<Building>,
) {
    for entity in removed_units.read() {
        grid.remove(entity);
    }
    for entity in removed_mobs.read() {
        grid.remove(entity);
    }
    for entity in removed_buildings.read() {
        grid.remove(entity);
    }
}

fn seed_wall_grid(
    mut grid: ResMut<WallSpatialGrid>,
    walls: Query<
        (
            Entity,
            &Transform,
            &BuildingFootprint,
            &Faction,
            Option<&NetworkId>,
        ),
        (
            With<Building>,
            Or<(
                With<WallSegmentPiece>,
                With<WallPostPiece>,
                With<WallCornerPiece>,
            )>,
        ),
    >,
) {
    grid.cells.clear();
    grid.entity_cells.clear();
    for (entity, tf, fp, faction, network_id) in &walls {
        grid.upsert(
            entity,
            tf.translation,
            fp.0,
            *faction,
            network_id.map(|id| id.0),
        );
    }
}

fn update_wall_grid(
    mut grid: ResMut<WallSpatialGrid>,
    walls: Query<
        (
            Entity,
            &Transform,
            &BuildingFootprint,
            &Faction,
            Option<&NetworkId>,
        ),
        (
            With<Building>,
            Or<(
                With<WallSegmentPiece>,
                With<WallPostPiece>,
                With<WallCornerPiece>,
            )>,
            Or<(
                Added<Building>,
                Changed<Transform>,
                Changed<BuildingFootprint>,
                Added<NetworkId>,
                Changed<NetworkId>,
            )>,
        ),
    >,
) {
    for (entity, tf, fp, faction, network_id) in &walls {
        grid.upsert(
            entity,
            tf.translation,
            fp.0,
            *faction,
            network_id.map(|id| id.0),
        );
    }
}

fn remove_wall_grid_entities(
    mut grid: ResMut<WallSpatialGrid>,
    mut removed_buildings: RemovedComponents<Building>,
    mut removed_wall_segments: RemovedComponents<WallSegmentPiece>,
    mut removed_wall_posts: RemovedComponents<WallPostPiece>,
    mut removed_wall_corners: RemovedComponents<WallCornerPiece>,
) {
    for entity in removed_buildings.read() {
        grid.remove(entity);
    }
    for entity in removed_wall_segments.read() {
        grid.remove(entity);
    }
    for entity in removed_wall_posts.read() {
        grid.remove(entity);
    }
    for entity in removed_wall_corners.read() {
        grid.remove(entity);
    }
}
