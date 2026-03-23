use bevy::prelude::*;
use std::collections::HashSet;

use crate::blueprints::{
    spawn_from_blueprint_with_faction, BlueprintRegistry, EntityKind, EntityVisualCache,
};
use crate::components::*;
use crate::ground::HeightMap;
use crate::model_assets::UnitModelAssets;
use crate::pathfinding::{NavDirect, NavPath, NavPending};
use crate::spatial::{SpatialHashGrid, WallSpatialGrid};
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
                spawn_all_players.after(crate::ground::spawn_ground),
            )
            .add_systems(
                Update,
                (move_units, steer_avoidance)
                    .chain()
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
    net_role: Res<crate::multiplayer::NetRole>,
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
    if *net_role == crate::multiplayer::NetRole::Client {
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
fn target_building(state: &UnitState, attack_target: Option<&AttackTarget>) -> Option<Entity> {
    match state {
        UnitState::MovingToBuild(e) | UnitState::Building(e) => Some(*e),
        UnitState::ReturningToDeposit { depot, .. }
        | UnitState::Depositing { depot, .. }
        | UnitState::WaitingForStorage { depot, .. } => Some(*depot),
        UnitState::AssignedGathering { building, .. } => Some(*building),
        UnitState::Attacking(e) => Some(*e),
        _ => attack_target.map(|at| at.0),
    }
}

fn steer_avoidance(
    time: Res<Time>,
    spatial_grid: Res<SpatialHashGrid>,
    wall_grid: Res<WallSpatialGrid>,
    net_role: Res<crate::multiplayer::NetRole>,
    active_player: Res<ActivePlayer>,
    mut units: Query<
        (
            Entity,
            &mut Transform,
            Option<&MoveTarget>,
            &UnitState,
            Option<&AttackTarget>,
            &Faction,
        ),
        With<Unit>,
    >,
    buildings: Query<(&Transform, &BuildingFootprint), (With<Building>, Without<Unit>)>,
) {
    let unit_avoidance_radius = 2.5;
    let unit_strength = 10.0;
    let wall_avoidance_radius = 3.5;
    let wall_strength = 12.0;
    let building_avoidance_radius = 1.5; // extra margin beyond footprint
    let building_strength = 15.0;

    for (entity, mut transform, move_target, unit_state, attack_target, faction) in &mut units {
        // Client: only apply avoidance to local player's units; remote units positioned by state sync
        if *net_role == crate::multiplayer::NetRole::Client && *faction != active_player.0 {
            continue;
        }
        let my_pos = transform.translation;
        let mut separation = Vec3::ZERO;
        let is_moving = move_target.is_some();

        // Determine which building (if any) this unit is trying to reach
        let my_target_building = target_building(unit_state, attack_target);

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
            if dist > 0.01 && dist < unit_avoidance_radius {
                let weight = (unit_avoidance_radius - dist) / unit_avoidance_radius;
                separation += flat_diff.normalize() * weight;
            }
        }

        // ── Wall repulsion ── (push away from nearby walls)
        if is_moving {
            let nearby_walls = wall_grid.query_radius(my_pos, wall_avoidance_radius);
            for (_wall_entity, wall_pos, wall_fp, _wall_faction) in &nearby_walls {
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
        if is_moving {
            let nearby_buildings = spatial_grid.query_radius(my_pos, 8.0);
            for (b_entity, b_pos) in &nearby_buildings {
                if *b_entity == entity {
                    continue;
                }
                // Skip the building this unit is trying to interact with
                if my_target_building == Some(*b_entity) {
                    continue;
                }
                if let Ok((_, footprint)) = buildings.get(*b_entity) {
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
            transform.translation += separation * unit_strength * time.delta_secs();
        }
    }
}

fn move_units(
    mut commands: Commands,
    time: Res<Time>,
    teams: Res<TeamConfig>,
    wall_grid: Res<WallSpatialGrid>,
    net_role: Res<crate::multiplayer::NetRole>,
    active_player: Res<ActivePlayer>,
    mut query: Query<
        (
            Entity,
            &mut Transform,
            &MoveTarget,
            &UnitSpeed,
            &Faction,
            Option<&Carrying>,
            Option<&CarryCapacity>,
            Option<&AttackTarget>,
            Option<&mut NavPath>,
            Has<NavPending>,
            Option<&StatusEffects>,
            Option<&mut MovementSmoothing>,
        ),
        With<Unit>,
    >,
) {
    let dt = time.delta_secs();
    for (
        entity,
        mut transform,
        target,
        unit_speed,
        faction,
        carrying,
        capacity,
        attack_target,
        nav_path,
        is_pending,
        opt_status,
        opt_smoothing,
    ) in &mut query
    {
        // Client: only move local player's units; remote units are positioned by state sync
        if *net_role == crate::multiplayer::NetRole::Client && *faction != active_player.0 {
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
            // Advance waypoint or finish
            if let Some(mut nav) = nav_path {
                nav.current_index += 1;
                if nav.current_index >= nav.waypoints.len() {
                    // Path complete — reset smoothing speed and add arrival spread
                    if let Some(mut smoothing) = opt_smoothing {
                        smoothing.current_speed = 0.0;
                    }
                    // Small random offset to prevent units stacking on exact same point
                    let spread_x = ((entity.to_bits() % 97) as f32 / 97.0 - 0.5) * 0.6;
                    let spread_z = ((entity.to_bits() % 83) as f32 / 83.0 - 0.5) * 0.6;
                    transform.translation.x += spread_x;
                    transform.translation.z += spread_z;
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
                let spread_x = ((entity.to_bits() % 97) as f32 / 97.0 - 0.5) * 0.6;
                let spread_z = ((entity.to_bits() % 83) as f32 / 83.0 - 0.5) * 0.6;
                transform.translation.x += spread_x;
                transform.translation.z += spread_z;
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
            let base_max_speed = unit_speed.0 * speed_mult * slow_factor;

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
    net_role: Res<crate::multiplayer::NetRole>,
    active_player: Res<ActivePlayer>,
    mut units: Query<(&mut Transform, &EntityKind, &Faction), With<Unit>>,
) {
    for (mut transform, kind, faction) in &mut units {
        // Client: only snap local player's units; remote units get correct Y from state sync
        if *net_role == crate::multiplayer::NetRole::Client && *faction != active_player.0 {
            continue;
        }
        transform.translation.y = height_map
            .sample(transform.translation.x, transform.translation.z)
            + y_offset_for(*kind, &registry);
    }
}
