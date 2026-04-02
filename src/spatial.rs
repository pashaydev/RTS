use bevy::ecs::lifecycle::RemovedComponents;
use bevy::prelude::*;
use std::collections::HashMap;

use crate::components::*;

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
                Update,
                (
                    update_spatial_grid,
                    remove_spatial_grid_entities,
                    update_wall_grid,
                    remove_wall_grid_entities,
                )
                    .before(crate::combat::approach_attack_target)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Resource)]
pub struct SpatialHashGrid {
    pub inv_cell_size: f32,
    pub cells: HashMap<IVec2, Vec<(Entity, Vec3)>>,
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

    pub fn insert(&mut self, entity: Entity, pos: Vec3) {
        let key = self.cell_key(pos);
        self.cells.entry(key).or_default().push((entity, pos));
        self.entity_cells.insert(entity, key);
    }

    pub fn remove(&mut self, entity: Entity) {
        let Some(key) = self.entity_cells.remove(&entity) else {
            return;
        };
        if let Some(entries) = self.cells.get_mut(&key) {
            entries.retain(|(stored, _)| *stored != entity);
            if entries.is_empty() {
                self.cells.remove(&key);
            }
        }
    }

    pub fn upsert(&mut self, entity: Entity, pos: Vec3) {
        self.remove(entity);
        self.insert(entity, pos);
    }

    pub fn query_radius(&self, pos: Vec3, radius: f32) -> Vec<(Entity, Vec3)> {
        let radius_sq = radius * radius;
        let min_x = ((pos.x - radius) * self.inv_cell_size).floor() as i32;
        let max_x = ((pos.x + radius) * self.inv_cell_size).floor() as i32;
        let min_z = ((pos.z - radius) * self.inv_cell_size).floor() as i32;
        let max_z = ((pos.z + radius) * self.inv_cell_size).floor() as i32;

        let mut results = Vec::new();
        for cx in min_x..=max_x {
            for cz in min_z..=max_z {
                if let Some(entries) = self.cells.get(&IVec2::new(cx, cz)) {
                    for &(entity, epos) in entries {
                        let dx = epos.x - pos.x;
                        let dz = epos.z - pos.z;
                        if dx * dx + dz * dz <= radius_sq {
                            results.push((entity, epos));
                        }
                    }
                }
            }
        }
        results
    }

    pub fn query_radius_limited(
        &self,
        pos: Vec3,
        radius: f32,
        limit: usize,
    ) -> Vec<(Entity, Vec3)> {
        let mut results = self.query_radius(pos, radius);
        results.sort_by(|a, b| {
            let da = (a.1.x - pos.x).powi(2) + (a.1.z - pos.z).powi(2);
            let db = (b.1.x - pos.x).powi(2) + (b.1.z - pos.z).powi(2);
            da.total_cmp(&db)
        });
        results.truncate(limit);
        results
    }

    pub fn query_corridor_limited(
        &self,
        from: Vec3,
        to: Vec3,
        half_width: f32,
        limit: usize,
    ) -> Vec<(Entity, Vec3)> {
        let delta = Vec2::new(to.x - from.x, to.z - from.z);
        let length = delta.length().max(0.001);
        let dir = delta / length;
        let mut results = Vec::new();
        for (entity, pos) in self.query_radius_limited(from.lerp(to, 0.5), length * 0.5 + half_width, limit * 3) {
            let rel = Vec2::new(pos.x - from.x, pos.z - from.z);
            let forward = rel.dot(dir);
            if forward < -half_width || forward > length + half_width {
                continue;
            }
            let closest = dir * forward;
            let lateral = (rel - closest).length();
            if lateral <= half_width {
                results.push((entity, pos));
            }
        }
        results.sort_by(|a, b| {
            let da = (a.1.x - from.x).powi(2) + (a.1.z - from.z).powi(2);
            let db = (b.1.x - from.x).powi(2) + (b.1.z - from.z).powi(2);
            da.total_cmp(&db)
        });
        results.truncate(limit);
        results
    }
}

#[derive(Resource)]
pub struct WallSpatialGrid {
    pub inv_cell_size: f32,
    pub cells: HashMap<IVec2, Vec<(Entity, Vec3, f32, Faction)>>, // entity, pos, footprint, faction
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

    pub fn query_radius(&self, pos: Vec3, radius: f32) -> Vec<(Entity, Vec3, f32, Faction)> {
        let radius_sq = radius * radius;
        let min_x = ((pos.x - radius) * self.inv_cell_size).floor() as i32;
        let max_x = ((pos.x + radius) * self.inv_cell_size).floor() as i32;
        let min_z = ((pos.z - radius) * self.inv_cell_size).floor() as i32;
        let max_z = ((pos.z + radius) * self.inv_cell_size).floor() as i32;

        let mut results = Vec::new();
        for cx in min_x..=max_x {
            for cz in min_z..=max_z {
                if let Some(entries) = self.cells.get(&IVec2::new(cx, cz)) {
                    for &(entity, epos, fp, faction) in entries {
                        let dx = epos.x - pos.x;
                        let dz = epos.z - pos.z;
                        if dx * dx + dz * dz <= radius_sq {
                            results.push((entity, epos, fp, faction));
                        }
                    }
                }
            }
        }
        results
    }

    pub fn upsert(&mut self, entity: Entity, pos: Vec3, footprint: f32, faction: Faction) {
        self.remove(entity);
        let key = self.cell_key(pos);
        self.cells
            .entry(key)
            .or_default()
            .push((entity, pos, footprint, faction));
        self.entity_cells.insert(entity, key);
    }

    pub fn remove(&mut self, entity: Entity) {
        let Some(key) = self.entity_cells.remove(&entity) else {
            return;
        };
        if let Some(entries) = self.cells.get_mut(&key) {
            entries.retain(|(stored, _, _, _)| *stored != entity);
            if entries.is_empty() {
                self.cells.remove(&key);
            }
        }
    }
}

fn seed_spatial_grid(
    mut grid: ResMut<SpatialHashGrid>,
    units: Query<(Entity, &Transform), With<Unit>>,
    mobs: Query<(Entity, &Transform), (With<Mob>, Without<Unit>)>,
    buildings: Query<
        (Entity, &Transform),
        (With<Building>, Without<Unit>, Without<Mob>, Without<FloorTile>),
    >,
) {
    grid.cells.clear();
    grid.entity_cells.clear();
    for (entity, tf) in &units {
        grid.insert(entity, tf.translation);
    }
    for (entity, tf) in &mobs {
        grid.insert(entity, tf.translation);
    }
    for (entity, tf) in &buildings {
        grid.insert(entity, tf.translation);
    }
}

fn update_spatial_grid(
    mut grid: ResMut<SpatialHashGrid>,
    units: Query<(Entity, &Transform), (With<Unit>, Or<(Added<Unit>, Changed<Transform>)>)>,
    mobs: Query<
        (Entity, &Transform),
        ((With<Mob>, Without<Unit>), Or<(Added<Mob>, Changed<Transform>)>),
    >,
    buildings: Query<
        (Entity, &Transform),
        (
            With<Building>,
            Without<Unit>,
            Without<Mob>,
            Without<FloorTile>,
            Or<(Added<Building>, Changed<Transform>)>,
        ),
    >,
) {
    for (entity, tf) in &units {
        grid.upsert(entity, tf.translation);
    }
    for (entity, tf) in &mobs {
        grid.upsert(entity, tf.translation);
    }
    for (entity, tf) in &buildings {
        grid.upsert(entity, tf.translation);
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
        (Entity, &Transform, &BuildingFootprint, &Faction),
        (
            With<Building>,
            Or<(With<WallSegmentPiece>, With<WallPostPiece>, With<WallCornerPiece>)>,
        ),
    >,
) {
    grid.cells.clear();
    grid.entity_cells.clear();
    for (entity, tf, fp, faction) in &walls {
        grid.upsert(entity, tf.translation, fp.0, *faction);
    }
}

fn update_wall_grid(
    mut grid: ResMut<WallSpatialGrid>,
    walls: Query<
        (Entity, &Transform, &BuildingFootprint, &Faction),
        (
            With<Building>,
            Or<(With<WallSegmentPiece>, With<WallPostPiece>, With<WallCornerPiece>)>,
            Or<(Added<Building>, Changed<Transform>, Changed<BuildingFootprint>)>,
        ),
    >,
) {
    for (entity, tf, fp, faction) in &walls {
        grid.upsert(entity, tf.translation, fp.0, *faction);
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
