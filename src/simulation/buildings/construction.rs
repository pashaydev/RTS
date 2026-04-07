//! Building construction: build orders, worker arrival, site preparation, progress.

use std::time::Duration;

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;

use crate::infrastructure::audio::{PlaySfx, SfxKind};
use crate::blueprints::{
    spawn_from_blueprint_with_faction, BlueprintRegistry, EntityKind, EntityVisualCache,
};
use crate::types::*;
use crate::world::ground::HeightMap;
use crate::presentation::model_assets::{BuildingConstructionAssets, BuildingModelAssets};
#[cfg(not(target_arch = "wasm32"))]
use bevy_mod_outline::{AsyncSceneInheritOutline, InheritOutline};

pub fn try_queue_build_order_authoritative(
    commands: &mut Commands,
    kind: EntityKind,
    build_pos: Vec3,
    faction: Faction,
    all_resources: &mut AllPlayerResources,
    base_state: &FactionBaseState,
    carried_totals: &CarriedResourceTotals,
    pending_drains: &mut PendingCarriedDrains,
    registry: &BlueprintRegistry,
    all_completed: &AllCompletedBuildings,
    biome_map: Option<&BiomeMap>,
    faction_ages: &crate::simulation::ages::FactionAges,
    height_map: &HeightMap,
    existing_buildings: &Query<
        (&Transform, &BuildingFootprint, &Faction, &EntityKind),
        (With<Building>, Without<GhostBuilding>),
    >,
    workers: &Query<
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
    obstacle_grid: &ObstacleGrid,
) -> Result<(), String> {
    if matches!(
        kind,
        EntityKind::WallSegment
            | EntityKind::WallPost
            | EntityKind::WallCorner
            | EntityKind::Gatehouse
            | EntityKind::Floor
    ) {
        return Err("This building uses a specialized placement flow.".to_string());
    }

    let bp = registry.get(kind);
    let new_footprint = super::footprint_for_kind(kind);
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

    if kind == EntityKind::Base && has_base_started {
        return Err("Base is already being founded.".to_string());
    }

    let prereq_met = if let Some(ref bd) = bp.building {
        match bd.prerequisite {
            None => true,
            Some(prereq_kind) => {
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
        return Err("Prerequisite not met.".to_string());
    }

    let required_age = crate::simulation::ages::required_age_for_building(kind);
    let current_age = faction_ages.get_age(&faction);
    if current_age < required_age {
        return Err(format!("Requires {}", required_age.display_name()));
    }

    if let Some(biome_map) = biome_map {
        if !super::is_biome_valid_for(kind, biome_map.get_biome(build_pos.x, build_pos.z)) {
            return Err(super::biome_requirement_text(kind)
                .unwrap_or("Invalid biome for building placement")
                .to_string());
        }
    }

    if !matches!(
        kind,
        EntityKind::WallSegment | EntityKind::WallPost | EntityKind::WallCorner
    ) {
        const MAX_BUILDING_SLOPE: f32 = 0.5;
        let slope = height_map.max_slope_under_footprint(build_pos.x, build_pos.z, new_footprint);
        if slope > MAX_BUILDING_SLOPE {
            return Err("Ground is too steep here.".to_string());
        }
    }

    for (building_tf, existing_fp, _, existing_kind) in existing_buildings {
        if !super::blocks_construction_overlap(*existing_kind) {
            continue;
        }
        let dx = building_tf.translation.x - build_pos.x;
        let dz = building_tf.translation.z - build_pos.z;
        if (dx * dx + dz * dz).sqrt() < existing_fp.0 + new_footprint {
            return Err("Building footprint is blocked.".to_string());
        }
    }

    if obstacle_grid.is_footprint_blocked(build_pos, new_footprint) {
        return Err("Blocked by trees.".to_string());
    }

    let half_map = height_map.half_map;
    if build_pos.x.abs() > half_map - 5.0 || build_pos.z.abs() > half_map - 5.0 {
        return Err("Too close to the edge of the map.".to_string());
    }

    // Find best worker using priority-based selection (includes processor-assigned workers)
    let worker_iter = workers
        .iter()
        .map(|(e, tf, state, fac, kind, _)| (e, tf, state, fac, kind));
    let Some((worker_entity, _worker_prio)) =
        super::find_best_worker_for_build(worker_iter, faction, build_pos)
    else {
        return Err("No workers available!".to_string());
    };

    let player_res = all_resources.get(&faction);
    let carried = carried_totals.get(&faction);
    if !bp.cost.can_afford_with_carried(player_res, carried) {
        return Err("Not enough resources.".to_string());
    }

    let player_res_mut = all_resources.get_mut(&faction);
    let deficits = bp.cost.deduct_with_carried(player_res_mut);
    let drain = SpendFromCarried {
        faction,
        amounts: deficits,
    };
    if drain.has_deficit() {
        pending_drains.drains.push(drain);
    }

    // Clean up any existing gathering assignment before reassigning
    if let Ok((_, _, w_state, _, _, _)) = workers.get(worker_entity) {
        super::cleanup_worker_assignment(commands, worker_entity, w_state);
    }

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
            rotation_y: 0.0,
        })
        .insert(MoveTarget(build_pos));
    commands
        .entity(worker_entity)
        .entry::<TaskQueue>()
        .and_modify(|mut tq| tq.queue.clear());

    Ok(())
}

pub(super) fn pending_build_arrival_system(
    mut commands: Commands,
    mut workers: Query<(Entity, &Transform, &UnitState, &PendingBuildOrder), With<Unit>>,
    registry: Res<BlueprintRegistry>,
    existing_buildings: Query<
        (&Transform, &BuildingFootprint, &EntityKind),
        (With<Building>, Without<GhostBuilding>),
    >,
    mut all_resources: ResMut<AllPlayerResources>,
) {
    let plot_range = 4.0;

    for (w_entity, w_tf, w_state, pending) in &mut workers {
        // Only act when in MovingToPlot state
        let UnitState::MovingToPlot(_) = *w_state else {
            continue;
        };

        let flat_dist = Vec2::new(
            w_tf.translation.x - pending.position.x,
            w_tf.translation.z - pending.position.z,
        )
        .length();
        if flat_dist > plot_range {
            continue; // Still walking
        }

        let kind = pending.kind;
        let build_pos = pending.position;
        let new_footprint = super::footprint_for_kind(kind);

        // Final collision check — another building may have been placed in the meantime
        let blocked = existing_buildings
            .iter()
            .any(|(building_tf, existing_fp, existing_kind)| {
                if !super::blocks_construction_overlap(*existing_kind) {
                    return false;
                }
                let check_pos = Vec3::new(build_pos.x, building_tf.translation.y, build_pos.z);
                building_tf.translation.distance(check_pos) < existing_fp.0 + new_footprint
            });

        if blocked {
            let bp = registry.get(kind);
            let res = all_resources.get_mut(&pending.faction);
            for (rt, amt) in bp.cost.cost_entries() {
                res.add(rt, amt);
            }

            commands
                .entity(w_entity)
                .remove::<PendingBuildOrder>()
                .remove::<MoveTarget>()
                .insert(UnitState::Idle)
                .insert(TaskSource::Auto);
            continue;
        }

        commands
            .entity(w_entity)
            .remove::<PendingBuildOrder>()
            .insert(BuildSitePreparation {
                kind,
                position: build_pos,
                faction: pending.faction,
                rotation_y: pending.rotation_y,
                prep_timer: Timer::from_seconds(1.25, TimerMode::Once),
                vfx_timer: Timer::from_seconds(0.12, TimerMode::Repeating),
                burst_count: 0,
            })
            .insert(UnitState::MovingToPlot(build_pos))
            .insert(TaskSource::Manual);
    }
}

pub(super) fn build_site_preparation_system(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    ghost_mats: Res<BuildingGhostMaterials>,
    height_map: Res<HeightMap>,
    building_models: Option<Res<BuildingModelAssets>>,
    vfx_assets: Option<Res<VfxAssets>>,
    mut workers: Query<(Entity, &Transform, &UnitState, &mut BuildSitePreparation), With<Unit>>,
    existing_buildings: Query<
        (&Transform, &BuildingFootprint, &EntityKind),
        (With<Building>, Without<GhostBuilding>),
    >,
    mut all_resources: ResMut<AllPlayerResources>,
) {
    for (worker_entity, worker_tf, worker_state, mut prep) in &mut workers {
        if !matches!(*worker_state, UnitState::MovingToPlot(_)) {
            continue;
        }

        let flat_dist = Vec2::new(
            worker_tf.translation.x - prep.position.x,
            worker_tf.translation.z - prep.position.z,
        )
        .length();
        if flat_dist > 4.0 {
            commands
                .entity(worker_entity)
                .insert(MoveTarget(prep.position));
            continue;
        }

        prep.prep_timer.tick(time.delta());
        prep.vfx_timer.tick(time.delta());

        if prep.vfx_timer.just_finished() {
            spawn_foundation_prep_vfx(
                &mut commands,
                vfx_assets.as_deref(),
                &height_map,
                prep.position,
                super::footprint_for_kind(prep.kind),
                prep.burst_count,
            );
            prep.burst_count = prep.burst_count.wrapping_add(1);
        }

        if !prep.prep_timer.is_finished() {
            commands
                .entity(worker_entity)
                .insert(MoveTarget(prep.position));
            continue;
        }

        let new_footprint = super::footprint_for_kind(prep.kind);
        let blocked = existing_buildings
            .iter()
            .any(|(building_tf, existing_fp, existing_kind)| {
                if !super::blocks_construction_overlap(*existing_kind) {
                    return false;
                }
                let check_pos =
                    Vec3::new(prep.position.x, building_tf.translation.y, prep.position.z);
                building_tf.translation.distance(check_pos) < existing_fp.0 + new_footprint
            });

        if blocked {
            let bp = registry.get(prep.kind);
            let res = all_resources.get_mut(&prep.faction);
            for (rt, amt) in bp.cost.cost_entries() {
                res.add(rt, amt);
            }

            commands
                .entity(worker_entity)
                .remove::<BuildSitePreparation>()
                .remove::<MoveTarget>()
                .insert(UnitState::Idle)
                .insert(TaskSource::Auto);
            continue;
        }

        let bp = registry.get(prep.kind);
        let is_gltf = bp.visual.mesh_kind.is_gltf();
        let rot_y = prep.rotation_y;
        let building_entity = spawn_from_blueprint_with_faction(
            &mut commands,
            &cache,
            prep.kind,
            prep.position,
            &registry,
            building_models.as_deref(),
            None,
            &height_map,
            prep.faction,
        );

        commands
            .entity(building_entity)
            .entry::<Transform>()
            .and_modify(move |mut tf| tf.rotation = Quat::from_rotation_y(rot_y));

        if !is_gltf {
            commands
                .entity(building_entity)
                .insert(MeshMaterial3d(ghost_mats.under_construction.clone()));
        }

        commands
            .entity(worker_entity)
            .remove::<BuildSitePreparation>()
            .remove::<MoveTarget>()
            .insert(UnitState::Building(building_entity))
            .insert(TaskSource::Manual);
    }
}

/// If a worker with a PendingBuildOrder dies or is reassigned, refund the building cost.
pub(super) fn pending_build_cleanup_system(
    mut commands: Commands,
    removed: Query<(Entity, &PendingBuildOrder, &UnitState), With<Unit>>,
    preparing: Query<(Entity, &BuildSitePreparation, &UnitState), With<Unit>>,
    mut all_resources: ResMut<AllPlayerResources>,
    registry: Res<BlueprintRegistry>,
) {
    for (entity, pending, state) in &removed {
        // If the worker is no longer in MovingToPlot state, the order was interrupted
        if !matches!(state, UnitState::MovingToPlot(_)) {
            let bp = registry.get(pending.kind);
            let res = all_resources.get_mut(&pending.faction);
            for (rt, amt) in bp.cost.cost_entries() {
                res.add(rt, amt);
            }

            commands.entity(entity).remove::<PendingBuildOrder>();
        }
    }

    for (entity, prep, state) in &preparing {
        if matches!(state, UnitState::MovingToPlot(_)) {
            continue;
        }

        let bp = registry.get(prep.kind);
        let res = all_resources.get_mut(&prep.faction);
        for (rt, amt) in bp.cost.cost_entries() {
            res.add(rt, amt);
        }

        commands.entity(entity).remove::<BuildSitePreparation>();
    }
}

fn spawn_foundation_prep_vfx(
    commands: &mut Commands,
    vfx_assets: Option<&VfxAssets>,
    height_map: &HeightMap,
    position: Vec3,
    footprint: f32,
    burst_count: u8,
) {
    let Some(vfx) = vfx_assets else {
        return;
    };

    let burst_seed = burst_count as f32 * 0.73;
    let particle_count = 6usize;
    for idx in 0..particle_count {
        let t = idx as f32 / particle_count as f32;
        let angle = burst_seed + t * std::f32::consts::TAU;
        let radius = footprint * (0.35 + 0.45 * ((burst_count as f32 * 0.27 + t).fract()));
        let world_x = position.x + angle.cos() * radius;
        let world_z = position.z + angle.sin() * radius;
        let ground_y = height_map.sample(world_x, world_z);
        let outward = Vec3::new(angle.cos(), 0.0, angle.sin());
        let lift = 1.6 + t * 0.6;

        commands.spawn((
            GatherParticle {
                velocity: outward * 1.6 + Vec3::Y * lift,
                timer: Timer::from_seconds(0.55 + t * 0.2, TimerMode::Once),
                start_scale: 0.16 + t * 0.06,
            },
            FogHideable::Vfx,
            Mesh3d(vfx.cube_mesh.clone()),
            MeshMaterial3d(vfx.dust_material.clone()),
            Transform::from_translation(Vec3::new(world_x, ground_y + 0.15, world_z)),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    }
}

pub(super) fn construction_progress_system(
    mut commands: Commands,
    time: Res<Time>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    building_models: Res<BuildingModelAssets>,
    construction_assets: Res<BuildingConstructionAssets>,
    mut base_state: ResMut<FactionBaseState>,
    mut buildings: Query<(
        Entity,
        &EntityKind,
        &mut BuildingState,
        &mut ConstructionProgress,
        Option<&mut ConstructionStage>,
        &mut Transform,
        &Faction,
    )>,
    workers: Query<&UnitState, With<Unit>>,
    children_q: Query<&Children>,
    scene_child_q: Query<Entity, With<BuildingSceneChild>>,
    mut event_log: ResMut<crate::ui::event_log_widget::GameEventLog>,
    mut sfx: bevy::ecs::message::MessageWriter<PlaySfx>,
    mut hammer_timer: Local<f32>,
) {
    const HAMMER_INTERVAL: f32 = 1.5;
    *hammer_timer += time.delta_secs();
    let play_hammer = *hammer_timer >= HAMMER_INTERVAL;
    if play_hammer {
        *hammer_timer = 0.0;
    }

    for (entity, kind, mut state, mut progress, construction_stage, mut transform, faction) in
        &mut buildings
    {
        if *state != BuildingState::UnderConstruction {
            continue;
        }

        // Count workers actively building this entity
        let builder_count = workers
            .iter()
            .filter(|state| matches!(state, UnitState::Building(e) if *e == entity))
            .count();

        if builder_count == 0 {
            progress.timer.pause();
            continue;
        }

        // Unpause when workers are present
        progress.timer.unpause();
        let mut speed_mult = 1.0 + 0.5 * (builder_count as f32 - 1.0);
        if super::is_wall_like_kind(*kind) {
            speed_mult *= 2.0;
        }

        let bp = registry.get(*kind);
        let base_scale = bp.visual.scale;
        let is_gltf = bp.visual.mesh_kind.is_gltf();

        progress
            .timer
            .tick(Duration::from_secs_f32(time.delta_secs() * speed_mult));

        let fraction = progress.timer.fraction();

        if play_hammer && !progress.timer.is_finished() {
            sfx.write(PlaySfx {
                kind: SfxKind::ConstructionHammer,
                position: Some(transform.translation),
            });
        }

        // For GLTF buildings: swap construction stage models at thresholds
        if is_gltf {
            let desired_stage = if fraction >= 1.0 {
                2 // complete
            } else if fraction >= 0.5 {
                1 // partial
            } else {
                0 // foundation
            };

            let current = construction_stage.as_ref().map(|s| s.0).unwrap_or(255);
            if current != desired_stage && desired_stage < 2 {
                // Swap scene child to construction stage model
                if let Some(stage_scene) = construction_assets.stages.get(&(*kind, desired_stage)) {
                    // Remove old scene child
                    if let Ok(children) = children_q.get(entity) {
                        for child in children.iter() {
                            if scene_child_q.contains(child) {
                                commands.entity(child).try_despawn();
                            }
                        }
                    }

                    let mut child = commands.spawn((
                        SceneRoot(stage_scene.clone()),
                        BuildingSceneChild,
                        building_models.child_transform(*kind, base_scale),
                    ));
                    #[cfg(not(target_arch = "wasm32"))]
                    child.insert((InheritOutline, AsyncSceneInheritOutline::default()));
                    let child = child.id();
                    commands.entity(entity).add_child(child);
                    commands
                        .entity(entity)
                        .insert(ConstructionStage(desired_stage));
                }
            }
        } else {
            // Non-GLTF: legacy scale lerp
            let current_scale = 0.3 * base_scale + (base_scale - 0.3 * base_scale) * fraction;
            transform.scale = Vec3::splat(current_scale);
        }

        if progress.timer.is_finished() {
            *state = BuildingState::Complete;

            if is_gltf {
                // Swap to final complete building model
                if let Ok(children) = children_q.get(entity) {
                    for child in children.iter() {
                        if scene_child_q.contains(child) {
                            commands.entity(child).try_despawn();
                        }
                    }
                }

                if let Some(complete_scene) =
                    building_models.scene_for(*kind, 1, transform.translation)
                {
                    let mut child = commands.spawn((
                        SceneRoot(complete_scene),
                        BuildingSceneChild,
                        building_models.child_transform(*kind, base_scale),
                    ));
                    #[cfg(not(target_arch = "wasm32"))]
                    child.insert((InheritOutline, AsyncSceneInheritOutline::default()));
                    let child = child.id();
                    commands.entity(entity).add_child(child);
                }
                commands
                    .entity(entity)
                    .insert(ConstructionStage(2))
                    .remove::<TeamColorApplied>();
            } else {
                transform.scale = Vec3::splat(base_scale);
                if let Some(mat) = cache.materials_default.get(kind) {
                    commands.entity(entity).insert(MeshMaterial3d(mat.clone()));
                }
            }

            if *kind == EntityKind::Base {
                base_state.set_founded(*faction, true);
            }

            commands
                .entity(entity)
                .remove::<ConstructionProgress>()
                .remove::<ConstructionWorkers>();

            // Add training queue for production buildings
            if let Some(ref bd) = bp.building {
                if !bd.trains.is_empty() {
                    commands.entity(entity).insert(TrainingQueue {
                        queue: vec![],
                        timer: None,
                        total_trained: 0,
                    });
                }
            }

            sfx.write(PlaySfx {
                kind: SfxKind::ConstructionComplete,
                position: Some(transform.translation),
            });

            // Log construction complete event
            event_log.push(
                time.elapsed_secs(),
                format!("{} construction complete", kind.display_name()),
                crate::ui::event_log_widget::EventCategory::Construction,
                Some(transform.translation),
                Some(*faction),
            );
        }
    }
}
