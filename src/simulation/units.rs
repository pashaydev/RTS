use bevy::prelude::*;
use std::collections::HashSet;

use crate::blueprints::{
    spawn_from_blueprint_with_faction, BlueprintRegistry, EntityKind, EntityVisualCache,
};
use crate::types::*;
use crate::world::ground::HeightMap;
use crate::presentation::model_assets::UnitModelAssets;
use crate::world::pathfinding::{NavDirect, NavGrid, NavPath, NavPending};
use crate::world::spatial::{SpatialHashGrid, WallSpatialGrid};
use std::f32::consts::PI;

pub struct UnitsPlugin;

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActivePlayer>()
            .init_resource::<AllPlayerResources>()
            .init_resource::<AllCompletedBuildings>()
            .init_resource::<FactionBaseState>()
            .init_resource::<TeamConfig>()
            .init_resource::<FactionColors>()
            .add_systems(OnEnter(AppState::InGame), apply_game_config)
            .add_systems(
                OnEnter(AppState::InGame),
                spawn_all_players
                    .after(crate::world::ground::spawn_ground)
                    .run_if(not(resource_exists::<crate::infrastructure::save_load::PendingLoad>)),
            )
            .add_systems(
                Update,
                (move_units, steer_avoidance)
                    .chain()
                    .in_set(GameFlowSet::Simulation)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                PostUpdate,
                snap_units_to_terrain.run_if(in_state(AppState::InGame)),
            );
    }
}

/// Applies GameSetupConfig to TeamConfig, AiControlledFactions, etc.
pub fn apply_game_config(
    config: Res<GameSetupConfig>,
    mut teams: ResMut<TeamConfig>,
    mut ai_controlled: ResMut<AiControlledFactions>,
) {
    let active = config.active_factions();
    let factions: Vec<Faction> = active.iter().map(|&i| Faction::PLAYERS[i]).collect();

    // Setup AI controlled factions
    let human_set: HashSet<usize> = config.human_faction_indices().into_iter().collect();
    let mut ai_facs = HashSet::new();
    for &idx in &active {
        if !human_set.contains(&idx) {
            ai_facs.insert(Faction::PLAYERS[idx]);
        }
    }
    ai_controlled.factions = ai_facs;

    // Setup teams
    let mut team_map = std::collections::HashMap::new();
    match config.team_mode {
        TeamMode::FFA => {
            for (i, &faction) in factions.iter().enumerate() {
                team_map.insert(faction, i as u8);
            }
        }
        TeamMode::Teams => {
            let count = factions.len();
            for (i, &faction) in factions.iter().enumerate() {
                team_map.insert(faction, if i < count / 2 { 0 } else { 1 });
            }
        }
        TeamMode::Custom => {
            for &idx in &active {
                team_map.insert(Faction::PLAYERS[idx], config.player_teams[idx]);
            }
        }
    }
    teams.teams = team_map.clone();
    info!(
        "apply_game_config: mode={:?}, factions={:?}, teams={:?}",
        config.team_mode, active, team_map
    );
}

pub fn y_offset_for(kind: EntityKind, registry: &BlueprintRegistry) -> f32 {
    let bp = registry.get(kind);
    bp.movement.as_ref().map(|m| m.y_offset).unwrap_or(0.8)
}

fn spawn_all_players(
    mut commands: Commands,
    net_role: Res<crate::infrastructure::multiplayer::NetRole>,
    cache: Res<EntityVisualCache>,
    registry: Res<BlueprintRegistry>,
    unit_models: Option<Res<UnitModelAssets>>,
    mut base_state: ResMut<FactionBaseState>,
    mut all_resources: ResMut<AllPlayerResources>,
    height_map: Res<HeightMap>,
    biome_map: Res<BiomeMap>,
    config: Res<GameSetupConfig>,
    map_seed: Res<MapSeed>,
) {
    if *net_role == crate::infrastructure::multiplayer::NetRole::Client {
        return;
    }

    let mut positions = config.spawn_positions(map_seed.0);

    // Biome validation: nudge spawn positions away from Water/Mountain
    let half_map = config.map_size.world_size() / 2.0;
    let radius = 0.6 * half_map;
    let count = positions.len();
    let rotation_offset = (map_seed.0 % 360) as f32 * PI / 180.0;

    for (i, (_faction, (ref mut x, ref mut z))) in positions.iter_mut().enumerate() {
        let base_angle = 2.0 * PI * i as f32 / count as f32 + rotation_offset;
        let biome = biome_map.get_biome(*x, *z);
        if biome == Biome::Water || biome == Biome::Mountain {
            // Nudge angle by ±5° increments until valid
            for nudge in 1..=36 {
                for sign in &[1.0_f32, -1.0] {
                    let angle = base_angle + sign * nudge as f32 * 5.0 * PI / 180.0;
                    let nx = angle.cos() * radius;
                    let nz = angle.sin() * radius;
                    let nb = biome_map.get_biome(nx, nz);
                    if nb != Biome::Water && nb != Biome::Mountain {
                        *x = nx;
                        *z = nz;
                        break;
                    }
                }
                let b = biome_map.get_biome(*x, *z);
                if b != Biome::Water && b != Biome::Mountain {
                    break;
                }
            }
        }
    }

    for &(faction, (sx, sz)) in &positions {
        let spawn_pos = Vec3::new(sx, 0.0, sz);
        base_state.set_founded(faction, false);

        // Initialize resources for this faction with starting multiplier
        let mut res = PlayerResources::empty();
        res.add(
            ResourceType::Wood,
            (220.0 * config.starting_resources_mult) as u32,
        );
        res.add(
            ResourceType::Copper,
            (20.0 * config.starting_resources_mult) as u32,
        );
        res.add(
            ResourceType::Iron,
            (40.0 * config.starting_resources_mult) as u32,
        );
        res.add(
            ResourceType::Stone,
            (30.0 * config.starting_resources_mult) as u32,
        );
        all_resources.resources.insert(faction, res);

        // Spawn 2 workers near the starting settlement area.
        let worker_offsets = [Vec3::new(3.0, 0.0, 0.0), Vec3::new(-3.0, 0.0, 2.0)];
        for offset in worker_offsets {
            spawn_from_blueprint_with_faction(
                &mut commands,
                &cache,
                EntityKind::Worker,
                spawn_pos + offset,
                &registry,
                None,
                unit_models.as_deref(),
                &height_map,
                faction,
            );
        }
    }
}

/// Extract the building entity a unit is currently targeting/interacting with.
///
/// `BuildingAssignment` is used as a fallback so transient worker-state desyncs
/// don't make assigned workers behave like fully idle bodies in avoidance.
fn target_building(
    state: &UnitState,
    attack_target: Option<&AttackTarget>,
    building_assignment: Option<&BuildingAssignment>,
) -> Option<Entity> {
    match state {
        UnitState::MovingToBuild(e) | UnitState::Building(e) => Some(*e),
        UnitState::ReturningToDeposit { depot, .. }
        | UnitState::Depositing { depot, .. }
        | UnitState::WaitingForStorage { depot, .. } => Some(*depot),
        UnitState::AssignedGathering { building, .. } => Some(*building),
        UnitState::Attacking(e) => Some(*e),
        _ => attack_target
            .map(|at| at.0)
            .or_else(|| building_assignment.map(|assignment| assignment.0)),
    }
}

fn steer_avoidance(
    time: Res<Time>,
    spatial_grid: Res<SpatialHashGrid>,
    wall_grid: Res<WallSpatialGrid>,
    nav_grid: Option<Res<NavGrid>>,
    net_role: Res<crate::infrastructure::multiplayer::NetRole>,
    active_player: Res<ActivePlayer>,
    mut units: Query<
        (
            Entity,
            &mut Transform,
            Option<&MoveTarget>,
            &UnitState,
            Option<&AttackTarget>,
            Option<&BuildingAssignment>,
            &Faction,
        ),
        (Or<(With<Unit>, With<Mob>)>, Without<Building>),
    >,
    buildings: Query<
        (Entity, &Transform, &BuildingFootprint),
        (With<Building>, Without<Unit>, Without<FloorTile>),
    >,
) {
    let moving_avoidance_radius = 2.6;
    let idle_avoidance_radius = 3.2;
    let unit_strength = 8.5;
    let idle_strength = 12.5;
    let hard_push_radius = 0.9;
    let wall_avoidance_radius = 3.5;
    let wall_strength = 12.0;
    let building_avoidance_radius = 1.5; // extra margin beyond footprint
    let building_strength = 15.0;

    for (entity, mut transform, move_target, unit_state, attack_target, building_assignment, faction) in &mut units {
        // Client: only apply avoidance to local player's units; remote units positioned by state sync
        if *net_role == crate::infrastructure::multiplayer::NetRole::Client && *faction != active_player.0 {
            continue;
        }
        let my_pos = transform.translation;
        let mut separation = Vec3::ZERO;
        let is_moving = move_target.is_some();
        let unit_avoidance_radius = if is_moving {
            moving_avoidance_radius
        } else {
            idle_avoidance_radius
        };
        let effective_strength = if is_moving {
            unit_strength
        } else {
            idle_strength
        };

        // Determine which building (if any) this unit is trying to reach
        let my_target_building = target_building(unit_state, attack_target, building_assignment);

        // ── Unit-to-unit avoidance ──
        let nearby = spatial_grid.query_radius(my_pos, unit_avoidance_radius);
        for (other_e, other_pos) in &nearby {
            if *other_e == entity {
                continue;
            }
            // Skip buildings in spatial grid
            if buildings.get(*other_e).is_ok() {
                continue;
            }
            let diff = my_pos - *other_pos;
            let flat_diff = Vec3::new(diff.x, 0.0, diff.z);
            let dist = flat_diff.length();
            if dist < 0.01 {
                // Nearly perfectly overlapping — push in a deterministic direction based on entity IDs
                let angle = (entity.to_bits().wrapping_sub(other_e.to_bits()) % 360) as f32
                    * std::f32::consts::TAU
                    / 360.0;
                separation += Vec3::new(angle.cos(), 0.0, angle.sin()) * 1.4;
            } else if dist < hard_push_radius {
                // Very close — strong quadratic push to prevent stacking
                let weight = ((hard_push_radius - dist) / hard_push_radius).powi(2) * 2.2 + 0.8;
                separation += flat_diff.normalize() * weight;
            } else if dist < unit_avoidance_radius {
                let weight = ((unit_avoidance_radius - dist) / unit_avoidance_radius).powi(2);
                separation += flat_diff.normalize() * weight * 0.5;
            }
        }

        // ── Wall repulsion ── (push away from nearby walls)
        if is_moving {
            let nearby_walls = wall_grid.query_radius(my_pos, wall_avoidance_radius);
            for (wall_entity, wall_pos, wall_fp, _wall_faction) in &nearby_walls {
                // Let builders approach the wall piece they are assigned to.
                if my_target_building == Some(*wall_entity) {
                    continue;
                }
                // Repel from all walls (not just hostile) to avoid clipping
                let diff = my_pos - *wall_pos;
                let flat_diff = Vec3::new(diff.x, 0.0, diff.z);
                let dist = flat_diff.length();
                let min_dist = wall_fp + 1.0;
                if dist > 0.01 && dist < min_dist + 1.5 {
                    let weight = (min_dist + 1.5 - dist) / 1.5;
                    separation += flat_diff.normalize() * weight * (wall_strength / unit_strength);
                }
            }
        }

        // ── Building repulsion ── (avoid walking through buildings)
        // Always active — idle units inside a footprint must also be pushed out.
        {
            let nearby_buildings = spatial_grid.query_radius(my_pos, 8.0);
            for (b_entity, b_pos) in &nearby_buildings {
                if *b_entity == entity {
                    continue;
                }
                // Skip the building this unit is trying to interact with
                if my_target_building == Some(*b_entity) {
                    continue;
                }
                if let Ok((_, _, footprint)) = buildings.get(*b_entity) {
                    let diff = my_pos - *b_pos;
                    let flat_diff = Vec3::new(diff.x, 0.0, diff.z);
                    let dist = flat_diff.length();
                    let min_dist = footprint.0 + building_avoidance_radius;
                    if dist > 0.01 && dist < min_dist {
                        let weight = (min_dist - dist) / building_avoidance_radius;
                        separation +=
                            flat_diff.normalize() * weight * (building_strength / unit_strength);
                    }
                }
            }
        }

        if separation.length_squared() > 0.0 {
            // Cap separation to avoid teleporting
            let max_sep = if is_moving { 6.0 } else { 9.0 } * time.delta_secs();
            let sep_vec = separation * effective_strength * time.delta_secs();
            let applied_sep = if sep_vec.length() > max_sep {
                sep_vec.clamp_length_max(max_sep)
            } else {
                sep_vec
            };

            // If the unit is already inside a blocked cell, relax NavGrid checks
            // so avoidance can push it out rather than keeping it trapped.
            let current_blocked = nav_grid
                .as_ref()
                .is_some_and(|grid| !grid.is_world_passable(my_pos.x, my_pos.z));

            let is_blocked = |pos: Vec3| -> bool {
                if !current_blocked {
                    if nav_grid
                        .as_ref()
                        .is_some_and(|grid| !grid.is_world_passable(pos.x, pos.z))
                    {
                        return true;
                    }
                }

                let nearby_walls = wall_grid.query_radius(pos, 3.0);
                if nearby_walls.iter().any(|(_wall_entity, wall_pos, wall_fp, _wall_faction)| {
                    let a = Vec2::new(pos.x, pos.z);
                    let b = Vec2::new(wall_pos.x, wall_pos.z);
                    a.distance(b) < wall_fp + 0.6
                }) {
                    return true;
                }

                if !current_blocked && nav_grid.is_none() {
                    for (building_entity, building_tf, footprint) in &buildings {
                        if my_target_building == Some(building_entity) {
                            continue;
                        }
                        let a = Vec2::new(pos.x, pos.z);
                        let b = Vec2::new(building_tf.translation.x, building_tf.translation.z);
                        if a.distance(b) < footprint.0 + 0.8 {
                            return true;
                        }
                    }
                }

                false
            };

            let candidate = transform.translation + applied_sep;
            if !is_blocked(candidate) {
                transform.translation = candidate;
            } else {
                let slide_x = transform.translation + Vec3::new(applied_sep.x, 0.0, 0.0);
                let slide_z = transform.translation + Vec3::new(0.0, 0.0, applied_sep.z);
                if applied_sep.x.abs() > 0.001 && !is_blocked(slide_x) {
                    transform.translation = slide_x;
                } else if applied_sep.z.abs() > 0.001 && !is_blocked(slide_z) {
                    transform.translation = slide_z;
                }
            }
        }
    }
}

fn move_units(
    mut commands: Commands,
    time: Res<Time>,
    teams: Res<TeamConfig>,
    wall_grid: Res<WallSpatialGrid>,
    floor_grid: Res<FloorGrid>,
    nav_grid: Option<Res<NavGrid>>,
    net_role: Res<crate::infrastructure::multiplayer::NetRole>,
    active_player: Res<ActivePlayer>,
    mut query: Query<
        (
            Entity,
            &mut Transform,
            &MoveTarget,
            &UnitSpeed,
            &Faction,
            Has<Unit>,
            Option<&Carrying>,
            Option<&CarryCapacity>,
            Option<&AttackTarget>,
            Option<&mut NavPath>,
            Has<NavPending>,
            Option<&StatusEffects>,
            Option<&mut MovementSmoothing>,
            Option<&UnitState>,
        ),
        Or<(With<Unit>, With<Mob>)>,
    >,
) {
    let dt = time.delta_secs();
    for (
        entity,
        mut transform,
        target,
        unit_speed,
        faction,
        is_unit,
        carrying,
        capacity,
        attack_target,
        nav_path,
        is_pending,
        opt_status,
        opt_smoothing,
        opt_unit_state,
    ) in &mut query
    {
        // Client: only move local player's units; remote units are positioned by state sync
        if *net_role == crate::infrastructure::multiplayer::NetRole::Client && *faction != active_player.0 {
            continue;
        }
        // Stunned units cannot move
        if opt_status.map_or(false, |s| s.is_stunned()) {
            continue;
        }
        // Wait for path computation — don't walk blindly
        if is_pending {
            continue;
        }

        // Determine immediate move target: next waypoint or MoveTarget directly
        let immediate_target = if let Some(ref nav) = nav_path {
            if nav.current_index < nav.waypoints.len() {
                nav.waypoints[nav.current_index]
            } else {
                target.0
            }
        } else {
            target.0
        };

        let direction = immediate_target - transform.translation;
        let flat_dir = Vec3::new(direction.x, 0.0, direction.z);
        let distance = flat_dir.length();

        // Check if this is the final waypoint (for deceleration)
        let is_final_waypoint = nav_path
            .as_ref()
            .map_or(true, |n| n.current_index + 1 >= n.waypoints.len());

        // Waypoint arrival threshold (tighter for intermediate waypoints)
        let arrival_dist = if !is_final_waypoint {
            1.8 // intermediate waypoint
        } else {
            0.5 // final destination
        };

        if distance < arrival_dist {
            // Skip arrival spread for workers heading to a building (deposit/build/gather)
            // — they need precise positioning for the deposit check to succeed.
            let skip_spread = opt_unit_state.is_some_and(|s| {
                matches!(
                    s,
                    UnitState::ReturningToDeposit { .. }
                        | UnitState::MovingToBuild(_)
                        | UnitState::AssignedGathering { .. }
                )
            });

            // Advance waypoint or finish
            if let Some(mut nav) = nav_path {
                nav.current_index += 1;
                if nav.current_index >= nav.waypoints.len() {
                    // Path complete — reset smoothing speed
                    if let Some(mut smoothing) = opt_smoothing {
                        smoothing.current_speed = 0.0;
                    }
                    if !skip_spread {
                        // Random offset to prevent units stacking on exact same point
                        let spread_x = ((entity.to_bits() % 97) as f32 / 97.0 - 0.5) * 3.5;
                        let spread_z = ((entity.to_bits() % 83) as f32 / 83.0 - 0.5) * 3.5;
                        transform.translation.x += spread_x;
                        transform.translation.z += spread_z;
                    }
                    commands
                        .entity(entity)
                        .remove::<MoveTarget>()
                        .remove::<NavPath>()
                        .remove::<NavDirect>();
                }
            } else {
                if let Some(mut smoothing) = opt_smoothing {
                    smoothing.current_speed = 0.0;
                }
                if !skip_spread {
                    let spread_x = ((entity.to_bits() % 97) as f32 / 97.0 - 0.5) * 3.5;
                    let spread_z = ((entity.to_bits() % 83) as f32 / 83.0 - 0.5) * 3.5;
                    transform.translation.x += spread_x;
                    transform.translation.z += spread_z;
                }
                commands
                    .entity(entity)
                    .remove::<MoveTarget>()
                    .remove::<NavDirect>();
            }
        } else {
            // Encumbrance: slow down when carrying heavy loads
            let speed_mult = if let (Some(carry), Some(cap)) = (carrying, capacity) {
                if cap.0 > 0.0 && carry.weight > 0.0 {
                    let load_fraction = (carry.weight / cap.0).min(1.0);
                    1.0 - load_fraction * 0.4 // 40% slower at full load
                } else {
                    1.0
                }
            } else {
                1.0
            };

            let slow_factor = opt_status.map_or(1.0, |s| s.slow_factor());
            let floor_speed_mult = if is_unit {
                let current_cell = WallGrid::world_to_grid(transform.translation);
                let next_cell = WallGrid::world_to_grid(immediate_target);
                if floor_grid.cells.contains_key(&current_cell)
                    || floor_grid.cells.contains_key(&next_cell)
                {
                    1.35
                } else {
                    1.0
                }
            } else {
                1.0
            };
            let base_max_speed = unit_speed.0 * speed_mult * slow_factor * floor_speed_mult;

            // Compute effective speed with acceleration/deceleration smoothing
            let effective_speed = if let Some(mut smoothing) = opt_smoothing {
                let variation = smoothing.speed_variation;
                let mut target_speed = base_max_speed * variation;

                // Decelerate near final destination for smooth stopping
                if is_final_waypoint && distance < 3.0 {
                    target_speed *= (distance / 3.0).clamp(0.15, 1.0);
                }

                // Ramp current_speed toward target_speed
                if smoothing.current_speed < target_speed {
                    smoothing.current_speed =
                        (smoothing.current_speed + smoothing.acceleration * dt).min(target_speed);
                } else {
                    smoothing.current_speed =
                        (smoothing.current_speed - smoothing.deceleration * dt).max(target_speed);
                }

                smoothing.current_speed * dt
            } else {
                // Fallback for units without MovementSmoothing
                base_max_speed * dt
            };

            let move_dir = flat_dir.normalize();
            let step = move_dir * effective_speed;
            let candidate = transform.translation + step;
            let ignore_wall = attack_target.map(|at| at.0);

            // Wall collision check helper
            let is_blocked = |pos: Vec3| -> bool {
                if nav_grid
                    .as_ref()
                    .is_some_and(|grid| !grid.is_world_passable(pos.x, pos.z))
                {
                    return true;
                }

                let nearby_walls = wall_grid.query_radius(pos, 3.0);
                nearby_walls
                    .iter()
                    .any(|(wall_entity, wall_pos, wall_fp, wall_faction)| {
                        if Some(*wall_entity) == ignore_wall {
                            return false;
                        }
                        if !teams.is_hostile(faction, wall_faction) {
                            return false;
                        }
                        let a = Vec2::new(pos.x, pos.z);
                        let b = Vec2::new(wall_pos.x, wall_pos.z);
                        a.distance(b) < wall_fp + 0.6
                    })
            };

            if !is_blocked(candidate) {
                transform.translation = candidate;
            } else {
                // Wall sliding: try moving along X or Z axis only
                let slide_x = transform.translation + Vec3::new(step.x, 0.0, 0.0);
                let slide_z = transform.translation + Vec3::new(0.0, 0.0, step.z);
                if step.x.abs() > 0.001 && !is_blocked(slide_x) {
                    transform.translation = slide_x;
                } else if step.z.abs() > 0.001 && !is_blocked(slide_z) {
                    transform.translation = slide_z;
                }
                // If both axes blocked, unit stays put (avoidance steering will push it)
            }
        }
    }
}

/// Snaps ALL units to terrain height every frame.
/// Runs after both movement and avoidance so Y is always correct
/// regardless of what modified XZ position.
fn snap_units_to_terrain(
    registry: Res<BlueprintRegistry>,
    height_map: Res<HeightMap>,
    net_role: Res<crate::infrastructure::multiplayer::NetRole>,
    active_player: Res<ActivePlayer>,
    mut units: Query<(&mut Transform, &EntityKind, &Faction), Or<(With<Unit>, With<Mob>)>>,
) {
    for (mut transform, kind, faction) in &mut units {
        // Client: only snap local player's units; remote units get correct Y from state sync
        if *net_role == crate::infrastructure::multiplayer::NetRole::Client && *faction != active_player.0 {
            continue;
        }
        transform.translation.y = height_map
            .sample(transform.translation.x, transform.translation.z)
            + y_offset_for(*kind, &registry);
    }
}
