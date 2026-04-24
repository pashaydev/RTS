//! Goal-aware resource picker and utility functions shared across the AI
//! economy/military/tactical subsystems.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::{
    spawn_from_blueprint_with_faction, BlueprintRegistry, EntityKind, EntityVisualCache,
};
use crate::presentation::model_assets::BuildingModelAssets;
use crate::simulation::buildings::footprint_for_kind;
use crate::types::*;
use crate::world::ground::HeightMap;

use super::types::*;

/// Goal-aware resource picker: prioritize the resource with largest deficit
/// relative to the next build goal's cost, falling back to state-based weights.
pub fn pick_goal_aware_resource(
    res: &PlayerResources,
    goal: Option<&ResourceGoal>,
    state: AiTopState,
) -> ResourceType {
    if let Some(goal) = goal {
        // Compute deficit per resource type
        let deficits = [
            (
                ResourceType::Wood,
                goal.wood.saturating_sub(res.get(ResourceType::Wood)) as f32,
            ),
            (
                ResourceType::Copper,
                goal.copper.saturating_sub(res.get(ResourceType::Copper)) as f32,
            ),
            (
                ResourceType::Iron,
                goal.iron.saturating_sub(res.get(ResourceType::Iron)) as f32,
            ),
            (
                ResourceType::Gold,
                goal.gold.saturating_sub(res.get(ResourceType::Gold)) as f32,
            ),
            (
                ResourceType::Oil,
                goal.oil.saturating_sub(res.get(ResourceType::Oil)) as f32,
            ),
        ];

        let max_deficit = deficits.iter().map(|(_, d)| *d).fold(0.0f32, f32::max);
        if max_deficit > 0.0 {
            return deficits
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .map(|(rt, _)| *rt)
                .unwrap_or(ResourceType::Wood);
        }
    }

    // Fallback to state-based weights
    let weights: [(ResourceType, f32); 5] = match state {
        AiTopState::Founding | AiTopState::EarlyEconomy => [
            (ResourceType::Wood, 3.0),
            (ResourceType::Copper, 2.0),
            (ResourceType::Iron, 1.0),
            (ResourceType::Gold, 0.2),
            (ResourceType::Oil, 0.0),
        ],
        AiTopState::Militarize | AiTopState::Defending => [
            (ResourceType::Wood, 2.5),
            (ResourceType::Copper, 2.5),
            (ResourceType::Iron, 1.5),
            (ResourceType::Gold, 0.5),
            (ResourceType::Oil, 0.0),
        ],
        AiTopState::Expanding | AiTopState::Attacking => [
            (ResourceType::Wood, 2.0),
            (ResourceType::Copper, 2.0),
            (ResourceType::Iron, 2.0),
            (ResourceType::Gold, 1.0),
            (ResourceType::Oil, 0.5),
        ],
        AiTopState::LateGame => [
            (ResourceType::Wood, 1.0),
            (ResourceType::Copper, 1.5),
            (ResourceType::Iron, 2.0),
            (ResourceType::Gold, 2.0),
            (ResourceType::Oil, 1.5),
        ],
    };

    let mut best_rt = ResourceType::Wood;
    let mut best_score = f32::MIN;
    for (rt, weight) in &weights {
        if *weight <= 0.0 {
            continue;
        }
        let amount = res.get(*rt) as f32;
        let score = weight / (amount + 50.0);
        if score > best_score {
            best_score = score;
            best_rt = *rt;
        }
    }
    best_rt
}

pub fn find_resource_biome_pos(
    kind: EntityKind,
    base_pos: Vec3,
    biome_map: &BiomeMap,
    height_map: &HeightMap,
) -> Option<Vec3> {
    let target_biome = match kind {
        EntityKind::Sawmill => Some(Biome::Forest),
        EntityKind::Mine => Some(Biome::Wetland),
        EntityKind::OilRig => Some(Biome::Water),
        _ => None,
    };

    let target_biome = target_biome?;

    for ring in 2..15 {
        let r = ring as f32 * 8.0;
        let steps = (ring * 8).max(8);
        for i in 0..steps {
            let angle = i as f32 / steps as f32 * std::f32::consts::TAU;
            let x = base_pos.x + angle.cos() * r;
            let z = base_pos.z + angle.sin() * r;

            if x.abs() > MAP_HALF || z.abs() > MAP_HALF {
                continue;
            }

            let biome = biome_map.get_biome(x, z);
            if biome == target_biome {
                if kind == EntityKind::OilRig {
                    let dir = (base_pos - Vec3::new(x, 0.0, z)).normalize_or_zero();
                    let adj_x = x + dir.x * 5.0;
                    let adj_z = z + dir.z * 5.0;
                    return Some(Vec3::new(adj_x, height_map.sample(adj_x, adj_z), adj_z));
                }
                return Some(Vec3::new(x, height_map.sample(x, z), z));
            }
        }
    }

    None
}

pub fn compute_scout_route(base_pos: Vec3) -> Vec<Vec3> {
    let center = Vec3::ZERO;
    let mut route = Vec::new();

    let base_angle = (base_pos.z - center.z).atan2(base_pos.x - center.x);

    for i in 0..8 {
        let angle = base_angle + i as f32 / 8.0 * std::f32::consts::TAU;
        let x = center.x + angle.cos() * SCOUT_RADIUS;
        let z = center.z + angle.sin() * SCOUT_RADIUS;
        let x = x.clamp(-MAP_HALF, MAP_HALF);
        let z = z.clamp(-MAP_HALF, MAP_HALF);
        route.push(Vec3::new(x, 0.0, z));
    }

    route
}

pub fn update_threat(threats: &mut Vec<ThreatEntry>, pos: Vec3, strength: f32, game_time: f32) {
    for threat in threats.iter_mut() {
        if threat.position.distance(pos) < 20.0 {
            threat.position = (threat.position + pos) * 0.5;
            threat.estimated_strength += strength;
            threat.last_seen = game_time;
            threat.entity_count += 1;
            return;
        }
    }
    threats.push(ThreatEntry {
        position: pos,
        estimated_strength: strength,
        last_seen: game_time,
        entity_count: 1,
    });
}

/// Improved strategic target picker with prioritization:
/// 1. Threats near base (immediate danger)
/// 2. Enemy production buildings (barracks, workshops)
/// 3. Enemy economy (sawmills, mines)
/// 4. Enemy base
pub fn pick_strategic_target(
    base_pos: Vec3,
    threats: &[ThreatEntry],
    enemy_buildings: &Query<
        (
            &Faction,
            &Transform,
            Option<&crate::infrastructure::net_bridge::NetworkId>,
        ),
        (With<Building>, Without<FloorTile>),
    >,
    teams: &TeamConfig,
    faction: &Faction,
) -> Option<Vec3> {
    // Priority 1: active threats near base (most recent, closest).
    // Tie-break on position bits so two equidistant threats always resolve
    // the same way on every peer.
    let mut near_threats: Vec<&ThreatEntry> = threats
        .iter()
        .filter(|t| {
            t.position.distance(base_pos) < BASE_THREAT_RADIUS * 3.0 && t.estimated_strength > 0.0
        })
        .collect();
    near_threats.sort_by(|a, b| {
        let da = (a.position.distance(base_pos) * 1000.0) as i64;
        let db = (b.position.distance(base_pos) * 1000.0) as i64;
        da.cmp(&db)
            .then_with(|| {
                (
                    a.position.x.to_bits(),
                    a.position.y.to_bits(),
                    a.position.z.to_bits(),
                )
                    .cmp(&(
                        b.position.x.to_bits(),
                        b.position.y.to_bits(),
                        b.position.z.to_bits(),
                    ))
            })
    });
    if let Some(threat) = near_threats.first() {
        return Some(threat.position);
    }

    // Priority 2: known threat clusters (weakest first)
    let mut valid_threats: Vec<&ThreatEntry> = threats
        .iter()
        .filter(|t| t.estimated_strength > 0.0)
        .collect();
    valid_threats.sort_by(|a, b| {
        let sa = (a.estimated_strength * 1000.0) as i64;
        let sb = (b.estimated_strength * 1000.0) as i64;
        sa.cmp(&sb).then_with(|| {
            (
                a.position.x.to_bits(),
                a.position.y.to_bits(),
                a.position.z.to_bits(),
            )
                .cmp(&(
                    b.position.x.to_bits(),
                    b.position.y.to_bits(),
                    b.position.z.to_bits(),
                ))
        })
    });
    if let Some(threat) = valid_threats.first() {
        return Some(threat.position);
    }

    // Priority 3: nearest enemy building, with stable tie-break.
    let mut best_key: Option<(i64, u32)> = None;
    let mut best_pos: Option<Vec3> = None;
    for (f, tf, net_id) in enemy_buildings.iter() {
        if !teams.is_hostile(faction, f) || *f == Faction::Neutral {
            continue;
        }
        let d = base_pos.distance(tf.translation);
        let quantized = (d * 1000.0).round() as i64;
        let nid = net_id.map(|id| id.0).unwrap_or(u32::MAX);
        let key = (quantized, nid);
        if best_key.map_or(true, |b| key < b) {
            best_key = Some(key);
            best_pos = Some(tf.translation);
        }
    }
    best_pos
}

/// Economic denial targeting for Aggressive raids.
///
/// Priority ladder (deterministic — sorted by position bits on ties):
/// 1. Isolated enemy workers (>30u from any enemy military).
/// 2. Under-construction enemy buildings at <50% progress.
/// 3. Unprotected enemy Outposts (no GuardTower/WatchTower/BallistaTower/BombardTower within 20u).
/// 4. Falls back to caller's default via `None`.
pub fn select_harass_target(
    teams: &TeamConfig,
    faction: &Faction,
    enemy_workers: &[(Vec3, u64)],
    enemy_military: &[Vec3],
    enemy_buildings: &[HarassBuilding],
) -> Option<Vec3> {
    // ── 1. Isolated workers ──
    let mut isolated: Vec<(Vec3, u64)> = Vec::new();
    for (pos, bits) in enemy_workers {
        let min_military_dist = enemy_military
            .iter()
            .map(|mp| mp.distance(*pos))
            .fold(f32::INFINITY, f32::min);
        if min_military_dist > 30.0 {
            isolated.push((*pos, *bits));
        }
    }
    if !isolated.is_empty() {
        // Pick farthest-from-military worker for most exposed target,
        // tie-break by position bits + entity bits.
        isolated.sort_by(|a, b| {
            let da = enemy_military
                .iter()
                .map(|mp| mp.distance(a.0))
                .fold(f32::INFINITY, f32::min);
            let db = enemy_military
                .iter()
                .map(|mp| mp.distance(b.0))
                .fold(f32::INFINITY, f32::min);
            let ka = ((-da * 1000.0) as i64, a.0.x.to_bits(), a.0.z.to_bits(), a.1);
            let kb = ((-db * 1000.0) as i64, b.0.x.to_bits(), b.0.z.to_bits(), b.1);
            ka.cmp(&kb)
        });
        return Some(isolated[0].0);
    }

    // ── 2. Half-built buildings ──
    let mut incomplete: Vec<&HarassBuilding> = enemy_buildings
        .iter()
        .filter(|b| {
            teams.is_hostile(faction, &b.faction)
                && b.faction != Faction::Neutral
                && b.construction_frac.map_or(false, |f| f < 0.5)
        })
        .collect();
    incomplete.sort_by(|a, b| {
        (a.position.x.to_bits(), a.position.z.to_bits())
            .cmp(&(b.position.x.to_bits(), b.position.z.to_bits()))
    });
    if let Some(b) = incomplete.first() {
        return Some(b.position);
    }

    // ── 3. Unprotected Outposts ──
    let tower_kinds = [
        EntityKind::GuardTower,
        EntityKind::WatchTower,
        EntityKind::BallistaTower,
        EntityKind::BombardTower,
        EntityKind::MageTower,
    ];
    let mut outposts: Vec<&HarassBuilding> = enemy_buildings
        .iter()
        .filter(|b| {
            teams.is_hostile(faction, &b.faction)
                && b.faction != Faction::Neutral
                && b.kind == EntityKind::Outpost
        })
        .collect();
    outposts.sort_by(|a, b| {
        (a.position.x.to_bits(), a.position.z.to_bits())
            .cmp(&(b.position.x.to_bits(), b.position.z.to_bits()))
    });
    for op in outposts {
        let has_nearby_tower = enemy_buildings.iter().any(|b| {
            b.faction == op.faction
                && tower_kinds.contains(&b.kind)
                && b.position.distance(op.position) < 20.0
        });
        if !has_nearby_tower {
            return Some(op.position);
        }
    }

    None
}

/// Snapshot of a building for `select_harass_target`.
#[derive(Clone)]
pub struct HarassBuilding {
    pub faction: Faction,
    pub kind: EntityKind,
    pub position: Vec3,
    pub construction_frac: Option<f32>,
}

pub fn find_enemy_resource_area(
    buildings: &Query<
        (
            &Faction,
            &Transform,
            Option<&crate::infrastructure::net_bridge::NetworkId>,
        ),
        (With<Building>, Without<FloorTile>),
    >,
    teams: &TeamConfig,
    faction: &Faction,
) -> Option<Vec3> {
    let mut best_key: Option<(i64, u32)> = None;
    let mut best_pos: Option<Vec3> = None;
    let origin = Vec3::ZERO;
    for (f, tf, net_id) in buildings.iter() {
        if !teams.is_hostile(faction, f) || *f == Faction::Neutral {
            continue;
        }
        let d = origin.distance(tf.translation);
        let quantized = (d * 1000.0).round() as i64;
        let nid = net_id.map(|id| id.0).unwrap_or(u32::MAX);
        let key = (quantized, nid);
        if best_key.map_or(true, |b| key < b) {
            best_key = Some(key);
            best_pos = Some(tf.translation);
        }
    }
    best_pos.map(|pos| {
        let to_center = (Vec3::ZERO - pos).normalize_or_zero();
        pos + to_center * 30.0
    })
}

pub fn try_train(
    train_queues: &mut Query<(&Faction, &EntityKind, &mut TrainingQueue), With<Building>>,
    faction: &Faction,
    unit_kind: EntityKind,
    registry: &BlueprintRegistry,
    unit_factions: &Query<&Faction, With<Unit>>,
    building_levels: &Query<
        (&Faction, &EntityKind, &BuildingState, &BuildingLevel),
        With<Building>,
    >,
) -> bool {
    let queued = train_queues
        .iter_mut()
        .filter(|(queue_faction, _, _)| **queue_faction == *faction)
        .map(|(_, _, queue)| queue.queue.len() as u32)
        .sum();
    let unit_cap = UnitCapStats {
        used: count_faction_units(*faction, unit_factions.iter()),
        queued,
        cap: faction_unit_cap(*faction, building_levels.iter()),
    };
    if !unit_cap.has_room(1) {
        return false;
    }

    for (f, building_kind, mut queue) in train_queues.iter_mut() {
        if *f != *faction {
            continue;
        }
        let bp = registry.get(*building_kind);
        if let Some(ref bd) = bp.building {
            if bd.trains.contains(&unit_kind) && queue.queue.len() < 5 {
                queue.queue.push(unit_kind);
                return true;
            }
        }
    }
    false
}

pub fn find_build_pos(
    base_pos: Vec3,
    existing_positions: &[Vec3],
    kind: EntityKind,
    _footprints: &Query<&BuildingFootprint>,
    height_map: &HeightMap,
    near_position: Option<Vec3>,
    obstacle_grid: &ObstacleGrid,
) -> Vec3 {
    let footprint = footprint_for_kind(kind);
    let spacing = footprint * 2.5;
    let center = near_position.unwrap_or(base_pos);

    for ring in 1..10 {
        let r = spacing * ring as f32;
        let steps = (ring * 6).max(6);
        for i in 0..steps {
            let angle = i as f32 / steps as f32 * std::f32::consts::TAU;
            let x = center.x + angle.cos() * r;
            let z = center.z + angle.sin() * r;

            let too_close = existing_positions.iter().any(|p| {
                let dx = p.x - x;
                let dz = p.z - z;
                (dx * dx + dz * dz).sqrt() < spacing * 0.8
            });
            if too_close {
                continue;
            }

            if x.abs() > MAP_HALF || z.abs() > MAP_HALF {
                continue;
            }

            if obstacle_grid.is_footprint_blocked(Vec3::new(x, 0.0, z), footprint) {
                continue;
            }

            return Vec3::new(x, height_map.sample(x, z), z);
        }
    }

    Vec3::new(
        base_pos.x + 10.0,
        height_map.sample(base_pos.x + 10.0, base_pos.z + 10.0),
        base_pos.z + 10.0,
    )
}

pub fn spawn_ai_building(
    commands: &mut Commands,
    cache: &EntityVisualCache,
    kind: EntityKind,
    pos: Vec3,
    registry: &BlueprintRegistry,
    building_models: Option<&BuildingModelAssets>,
    height_map: &HeightMap,
    faction: Faction,
) {
    let entity = spawn_from_blueprint_with_faction(
        commands,
        cache,
        kind,
        pos,
        registry,
        building_models,
        None,
        height_map,
        faction,
    );

    let bp = registry.get(kind);
    let construction_time = bp
        .building
        .as_ref()
        .map(|b| b.construction_time_secs)
        .unwrap_or(10.0);

    commands.entity(entity).insert(ConstructionProgress {
        timer: Timer::from_seconds(construction_time, TimerMode::Once),
    });
}

/// Generate a rectangular wall plan around the base
pub fn generate_wall_plan(base_pos: Vec3, personality: AiPersonality) -> WallPlan {
    let radius_cells = match personality {
        AiPersonality::Defensive => 15,
        _ => 12,
    };
    let gate_cells = 3;
    let (base_gx, base_gz) = WallGrid::world_to_grid(base_pos);
    let min_x = base_gx - radius_cells;
    let max_x = base_gx + radius_cells;
    let min_z = base_gz - radius_cells;
    let max_z = base_gz + radius_cells;

    // Leave a gate opening on the side facing map center
    let to_center = (Vec3::ZERO - base_pos).normalize_or_zero();
    let gate_side = if to_center.x.abs() > to_center.z.abs() {
        if to_center.x > 0.0 {
            1
        } else {
            3
        }
    } else {
        if to_center.z > 0.0 {
            2
        } else {
            0
        }
    };

    let mut runs = Vec::with_capacity(5);
    let push_run = |runs: &mut Vec<(Vec3, Vec3)>, start: (i32, i32), end: (i32, i32)| {
        runs.push((
            WallGrid::grid_to_world(start.0, start.1),
            WallGrid::grid_to_world(end.0, end.1),
        ));
    };

    for side in 0..4 {
        if side != gate_side {
            match side {
                0 => push_run(&mut runs, (min_x, min_z), (max_x, min_z)),
                1 => push_run(&mut runs, (max_x, min_z), (max_x, max_z)),
                2 => push_run(&mut runs, (max_x, max_z), (min_x, max_z)),
                3 => push_run(&mut runs, (min_x, max_z), (min_x, min_z)),
                _ => unreachable!(),
            }
            continue;
        }

        match side {
            0 | 2 => {
                let z = if side == 0 { min_z } else { max_z };
                let mid_x = (min_x + max_x) / 2;
                let gate_start_x = (mid_x - gate_cells / 2).max(min_x + 1);
                let gate_end_x = (gate_start_x + gate_cells - 1).min(max_x - 1);
                push_run(&mut runs, (min_x, z), (gate_start_x - 1, z));
                push_run(&mut runs, (gate_end_x + 1, z), (max_x, z));
            }
            1 | 3 => {
                let x = if side == 1 { max_x } else { min_x };
                let mid_z = (min_z + max_z) / 2;
                let gate_start_z = (mid_z - gate_cells / 2).max(min_z + 1);
                let gate_end_z = (gate_start_z + gate_cells - 1).min(max_z - 1);
                push_run(&mut runs, (x, min_z), (x, gate_start_z - 1));
                push_run(&mut runs, (x, gate_end_z + 1), (x, max_z));
            }
            _ => unreachable!(),
        }
    }

    let completed = vec![false; runs.len()];
    WallPlan { runs, completed }
}

/// Generate evenly-spaced wall points between two positions
pub fn generate_wall_points(start: Vec3, end: Vec3, height_map: &HeightMap) -> Vec<Vec3> {
    let (start_gx, start_gz) = WallGrid::world_to_grid(start);
    let (end_gx, end_gz) = WallGrid::world_to_grid(end);

    let dx = end_gx - start_gx;
    let dz = end_gz - start_gz;
    let steps = dx.abs().max(dz.abs()) as usize;
    if steps == 0 {
        let world = WallGrid::grid_to_world(start_gx, start_gz);
        let y = height_map.sample(world.x, world.z);
        return vec![Vec3::new(world.x, y, world.z)];
    }

    let step_x = dx.signum();
    let step_z = dz.signum();
    let mut points = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let gx = start_gx + step_x * i as i32;
        let gz = start_gz + step_z * i as i32;
        let world = WallGrid::grid_to_world(gx, gz);
        let y = height_map.sample(world.x, world.z);
        points.push(Vec3::new(world.x, y, world.z));
    }
    points
}

pub fn push_if_missing(
    brain: &mut AiFactionBrain,
    tc: &HashMap<EntityKind, usize>,
    kind: EntityKind,
    max: usize,
    priority: u8,
) {
    if tc.get(&kind).copied().unwrap_or(0) < max {
        brain.build_queue.push(BuildRequest {
            kind,
            priority,
            near_position: None,
        });
    }
}
