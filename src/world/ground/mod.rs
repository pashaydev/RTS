//! `GroundPlugin`: procedural terrain mesh, water plane, height map,
//! biome map, mountain border, and terrain texture atlas.

mod border;
mod data;
mod generation;
mod noise;
mod terrain_updates;
mod water;

use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;

use crate::presentation::materials::terrain::TerrainMaterial;
use crate::presentation::materials::water::WaterMaterial;
use crate::types::{AppState, GameFlowSet};

#[allow(unused_imports)]
pub use border::spawn_mountain_border;
#[allow(unused_imports)]
pub use data::{
    edge_distance_to_square, foundation_radii, is_in_mountain_border, playable_half_map,
    BorderSettings, HeightMap, TerrainShapeOp, TerrainShapeSyncState, TerrainShapeUpdateQueue,
    TerrainSurfaceDirtyArea, TerrainSurfaceDirtyQueue, WATER_LEVEL,
};
#[allow(unused_imports)]
pub use generation::spawn_ground;
#[allow(unused_imports)]
pub use generation::TerrainTextures;
#[allow(unused_imports)]
pub use noise::resolve_map_seed;
#[allow(unused_imports)]
pub use terrain_updates::{
    apply_terrain_shape_op, paint_floor_blend_on_ground, sync_ground_mesh_partial,
};
#[allow(unused_imports)]
pub use water::WaterPlane;

use generation::{configure_terrain_texture_samplers, load_terrain_textures};
use terrain_updates::{enqueue_building_terrain_updates, process_terrain_shape_update_queue};
use water::{patch_water_fog_textures, update_water_time};

pub struct GroundPlugin;

impl Plugin for GroundPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            MaterialPlugin::<WaterMaterial>::default(),
            MaterialPlugin::<TerrainMaterial>::default(),
        ))
        .init_resource::<TerrainShapeUpdateQueue>()
        .init_resource::<TerrainShapeSyncState>()
        .init_resource::<TerrainSurfaceDirtyQueue>()
        .add_systems(Startup, load_terrain_textures)
        .add_systems(
            OnEnter(AppState::InGame),
            (resolve_map_seed, spawn_ground, spawn_mountain_border).chain(),
        )
        .add_systems(Update, configure_terrain_texture_samplers)
        .add_systems(
            Update,
            (update_water_time, patch_water_fog_textures).run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            FixedUpdate,
            (
                enqueue_building_terrain_updates,
                process_terrain_shape_update_queue,
            )
                .chain()
                .in_set(GameFlowSet::Simulation)
                .run_if(in_state(AppState::InGame)),
        );
    }
}
