//! Building system: placement, construction, upgrades, walls, training, environment.

mod construction;
mod environment;
mod placement;
mod training;
mod upgrades;
mod walls;

use bevy::prelude::*;

use crate::blueprints::EntityKind;
use crate::types::*;

// ── Public re-exports ──

pub use construction::try_queue_build_order_authoritative;
pub use environment::{storage_aura_bonus, SAWMILL_TREE_SLOTS, SAWMILL_YARD_OFFSET};
pub use upgrades::{start_demolish, start_upgrade};
pub use walls::{
    auto_tile_piece, piece_kind_to_entity_kind, spawn_floor_grid_cells, spawn_wall_grid_cells,
    spawn_wall_line,
};

// ── Plugin ──

pub struct BuildingsPlugin;

impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildingPlacementState>()
            .init_resource::<WallPlotPreview>()
            .init_resource::<FloorPlotPreview>()
            .init_resource::<WallGrid>()
            .init_resource::<FloorGrid>()
            .init_resource::<ObstacleGrid>()
            .add_systems(Startup, placement::create_ghost_materials)
            .add_systems(
                Update,
                environment::sync_obstacle_grid
                    .in_set(GameFlowSet::Simulation)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    placement::update_placement_preview,
                    placement::update_wall_plot_preview,
                    placement::update_gate_plot_preview,
                    placement::update_floor_plot_preview,
                    placement::apply_ghost_materials,
                    placement::animate_placement_preview_vfx,
                    placement::confirm_placement,
                    placement::confirm_wall_plot,
                    placement::confirm_gate_plot,
                    placement::confirm_floor_plot,
                    placement::cancel_placement,
                )
                    .chain()
                    .in_set(GameFlowSet::Input)
                    .run_if(in_state(AppState::InGame))
                    .run_if(player_can_command),
            )
            .add_systems(
                Update,
                (
                    construction::pending_build_arrival_system,
                    construction::build_site_preparation_system,
                    construction::pending_build_cleanup_system,
                    construction::construction_progress_system,
                    training::eject_units_from_buildings,
                    walls::wall_auto_tile_system,
                    walls::floor_auto_tile_system,
                    training::tower_auto_attack,
                    training::training_queue_system,
                    training::update_completed_buildings_tracker,
                )
                    .chain()
                    .in_set(GameFlowSet::Simulation)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    upgrades::building_upgrade_system,
                    upgrades::demolish_system,
                    upgrades::building_scale_anim_system,
                    environment::healing_aura_system,
                    upgrades::level_indicator_system,
                    environment::sync_storage_on_spend,
                    environment::update_storage_piles,
                    environment::sawmill_yard_system,
                    environment::yard_tree_regrowth_system,
                    environment::sync_environment_to_terrain_changes,
                    environment::clear_vegetation_around_buildings,
                )
                    .in_set(GameFlowSet::Simulation)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// ── Shared helpers (used by multiple submodules) ──

/// Worker availability priority for building placement.
/// Lower number = preferred (idle workers are picked before assigned ones).
fn worker_availability_priority(
    state: &UnitState,
    build_counts: &std::collections::HashMap<Entity, u32>,
) -> Option<u8> {
    match state {
        UnitState::Idle | UnitState::Moving(_) => Some(0),
        UnitState::Gathering(_)
        | UnitState::ReturningToDeposit { .. }
        | UnitState::Depositing { .. }
        | UnitState::WaitingForStorage { .. } => Some(1),
        UnitState::AssignedGathering { .. } => Some(2),
        UnitState::MovingToBuild(building) | UnitState::Building(building) => {
            let count = build_counts.get(building).copied().unwrap_or(1);
            if count > 1 {
                Some(3)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Find the best available worker for a build task at `build_pos`.
fn find_best_worker_for_build<'a>(
    workers: impl Iterator<
        Item = (
            Entity,
            &'a Transform,
            &'a UnitState,
            &'a Faction,
            &'a EntityKind,
        ),
    >,
    faction: Faction,
    build_pos: Vec3,
) -> Option<(Entity, u8)> {
    let mut faction_workers: Vec<(Entity, Vec3, &UnitState)> = Vec::new();
    let mut build_counts: std::collections::HashMap<Entity, u32> = std::collections::HashMap::new();

    for (w_entity, w_tf, w_state, w_faction, w_kind) in workers {
        if *w_kind != EntityKind::Worker || *w_faction != faction {
            continue;
        }
        faction_workers.push((w_entity, w_tf.translation, w_state));
        match w_state {
            UnitState::MovingToBuild(building) | UnitState::Building(building) => {
                *build_counts.entry(*building).or_insert(0) += 1;
            }
            _ => {}
        }
    }

    let mut best: Option<(Entity, u8, f32)> = None;
    for &(w_entity, w_pos, w_state) in &faction_workers {
        let Some(prio) = worker_availability_priority(w_state, &build_counts) else {
            continue;
        };
        let dist = w_pos.distance(build_pos);
        let dominated = best.map_or(false, |(_, best_prio, best_dist)| {
            prio > best_prio || (prio == best_prio && dist >= best_dist)
        });
        if !dominated {
            best = Some((w_entity, prio, dist));
        }
    }
    best.map(|(e, prio, _)| (e, prio))
}

fn has_available_worker_for_build<'a>(
    workers: impl Iterator<
        Item = (
            Entity,
            &'a Transform,
            &'a UnitState,
            &'a Faction,
            &'a EntityKind,
        ),
    >,
    faction: Faction,
    build_pos: Vec3,
) -> bool {
    find_best_worker_for_build(workers, faction, build_pos).is_some()
}

/// When reassigning a worker away from their current task, clean up the old building's
/// worker list and remove `BuildingAssignment`.
fn cleanup_worker_assignment(commands: &mut Commands, worker: Entity, state: &UnitState) {
    match state {
        UnitState::AssignedGathering { building, .. }
        | UnitState::MovingToBuild(building)
        | UnitState::Building(building) => {
            let building_entity = *building;
            commands
                .entity(building_entity)
                .entry::<AssignedWorkers>()
                .and_modify(move |mut aw| {
                    aw.workers.retain(|w| *w != worker);
                });
        }
        _ => {}
    }
    commands.entity(worker).remove::<BuildingAssignment>();
}

// ── Building metadata helpers ──

pub fn footprint_for_kind(kind: EntityKind) -> f32 {
    match kind {
        EntityKind::Outpost => 4.5,
        EntityKind::Base | EntityKind::Smelter => 3.5,
        EntityKind::Barracks
        | EntityKind::Workshop
        | EntityKind::MageTower
        | EntityKind::Temple
        | EntityKind::Stable
        | EntityKind::SiegeWorks
        | EntityKind::Sawmill
        | EntityKind::Mine
        | EntityKind::OilRig
        | EntityKind::Alchemist
        | EntityKind::Gatehouse => 3.0,
        EntityKind::Tower
        | EntityKind::WatchTower
        | EntityKind::GuardTower
        | EntityKind::BallistaTower
        | EntityKind::BombardTower
        | EntityKind::Storage
        | EntityKind::House => 2.0,
        EntityKind::Floor => 2.2,
        EntityKind::WallSegment | EntityKind::WallPost | EntityKind::WallCorner => 1.5,
        _ => 2.5,
    }
}

pub fn building_height_for_kind(kind: EntityKind) -> f32 {
    match kind {
        EntityKind::Tower
        | EntityKind::WatchTower
        | EntityKind::GuardTower
        | EntityKind::BallistaTower
        | EntityKind::BombardTower => 10.0,
        EntityKind::Outpost => 8.0,
        EntityKind::Base | EntityKind::Smelter => 7.0,
        EntityKind::Barracks
        | EntityKind::Workshop
        | EntityKind::MageTower
        | EntityKind::Temple
        | EntityKind::Stable
        | EntityKind::SiegeWorks
        | EntityKind::Sawmill
        | EntityKind::Mine
        | EntityKind::OilRig
        | EntityKind::Alchemist
        | EntityKind::Gatehouse => 6.0,
        EntityKind::House | EntityKind::Storage => 5.0,
        EntityKind::WallSegment | EntityKind::WallPost | EntityKind::WallCorner => 4.0,
        EntityKind::Floor => 0.5,
        _ => 5.0,
    }
}

pub fn is_wall_like_kind(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::WallSegment
            | EntityKind::WallPost
            | EntityKind::WallCorner
            | EntityKind::Gatehouse
    )
}

pub fn is_floor_kind(kind: EntityKind) -> bool {
    kind == EntityKind::Floor
}

fn blocks_construction_overlap(kind: EntityKind) -> bool {
    !is_floor_kind(kind)
}

pub fn uses_terrain_foundation(kind: EntityKind) -> bool {
    !matches!(
        kind,
        EntityKind::WallSegment
            | EntityKind::WallPost
            | EntityKind::WallCorner
            | EntityKind::OilRig
            | EntityKind::Floor
    )
}

/// Returns the allowed biomes for a building kind, or `None` for default (any non-water).
pub fn allowed_biomes(kind: EntityKind) -> Option<&'static [Biome]> {
    match kind {
        EntityKind::Sawmill => Some(&[Biome::Forest, Biome::Grassland]),
        EntityKind::Mine => Some(&[Biome::Mountain, Biome::Wetland, Biome::Desert]),
        EntityKind::OilRig => Some(&[Biome::Water]),
        _ => None,
    }
}

/// Checks if a building kind can be placed on the given biome.
pub fn is_biome_valid_for(kind: EntityKind, biome: Biome) -> bool {
    match allowed_biomes(kind) {
        Some(allowed) => allowed.contains(&biome),
        None => biome != Biome::Water,
    }
}

/// Returns a human-readable biome requirement hint for placement feedback.
pub fn biome_requirement_text(kind: EntityKind) -> Option<&'static str> {
    match kind {
        EntityKind::Sawmill => Some("Sawmill must be placed on Forest or Grassland"),
        EntityKind::Mine => Some("Mine must be placed on Mountain, Wetland, or Desert"),
        EntityKind::OilRig => Some("Oil Rig must be placed on Water"),
        _ => Some("Cannot place on Water"),
    }
}
