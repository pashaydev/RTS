use bevy::prelude::*;
use std::collections::HashMap;

use crate::components::*;

pub struct SpatialPlugin;

impl Plugin for SpatialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpatialHashGrid>()
            .init_resource::<WallSpatialGrid>()
            .add_systems(
                Update,
                (rebuild_spatial_grid, rebuild_wall_grid)
                    .before(crate::combat::player_auto_acquire_target)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Resource)]
pub struct SpatialHashGrid {
    pub inv_cell_size: f32,
    pub cells: HashMap<IVec2, Vec<(Entity, Vec3)>>,
}

impl Default for SpatialHashGrid {
    fn default() -> Self {
        Self {
            inv_cell_size: 1.0 / 15.0,
            cells: HashMap::new(),
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
}

#[derive(Resource)]
pub struct WallSpatialGrid {
    pub inv_cell_size: f32,
    pub cells: HashMap<IVec2, Vec<(Entity, Vec3, f32, Faction)>>, // entity, pos, footprint, faction
}

impl Default for WallSpatialGrid {
    fn default() -> Self {
        Self {
            inv_cell_size: 1.0 / 5.0,
            cells: HashMap::new(),
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
}

fn rebuild_spatial_grid(
    mut grid: ResMut<SpatialHashGrid>,
    units: Query<(Entity, &Transform), With<Unit>>,
    mobs: Query<(Entity, &Transform), (With<Mob>, Without<Unit>)>,
    buildings: Query<(Entity, &Transform), (With<Building>, Without<Unit>, Without<Mob>)>,
) {
    grid.cells.clear();
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

fn rebuild_wall_grid(
    mut grid: ResMut<WallSpatialGrid>,
    walls: Query<
        (Entity, &Transform, &BuildingFootprint, &Faction),
        (
            With<Building>,
            Or<(With<WallSegmentPiece>, With<WallPostPiece>)>,
        ),
    >,
) {
    grid.cells.clear();
    for (entity, tf, fp, faction) in &walls {
        let key = grid.cell_key(tf.translation);
        grid.cells
            .entry(key)
            .or_default()
            .push((entity, tf.translation, fp.0, *faction));
    }
}
