use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::{
    spawn_from_blueprint_with_faction, BlueprintRegistry, EntityKind, EntityVisualCache,
};
use crate::buildings::footprint_for_kind;
use crate::components::*;
use crate::ground::HeightMap;
use crate::model_assets::BuildingModelAssets;

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
            (ResourceType::Wood, goal.wood.saturating_sub(res.get(ResourceType::Wood)) as f32),
            (ResourceType::Copper, goal.copper.saturating_sub(res.get(ResourceType::Copper)) as f32),
            (ResourceType::Iron, goal.iron.saturating_sub(res.get(ResourceType::Iron)) as f32),
            (ResourceType::Gold, goal.gold.saturating_sub(res.get(ResourceType::Gold)) as f32),
            (ResourceType::Oil, goal.oil.saturating_sub(res.get(ResourceType::Oil)) as f32),
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

pub fn find_nearest_resource_node_with_avoidance(
    pos: Vec3,
    resource_type: ResourceType,
    nodes: &Query<(Entity, &Transform, &ResourceNode), Without<Unit>>,
    max_range: f32,
    player_base: Option<Vec3>,
    is_friendly: bool,
) -> Option<Entity> {
    let mut best: Option<(Entity, f32)> = None;
    for (entity, tf, node) in nodes.iter() {
        if node.resource_type != resource_type || node.amount_remaining == 0 {
            continue;
        }
        let mut d = pos.distance(tf.translation);
        if d >= max_range {
            continue;
        }

        if is_friendly {
            if let Some(pbp) = player_base {
                let dist_to_player = tf.translation.distance(pbp);
                if dist_to_player < 40.0 {
                    d += 80.0;
                }
            }
        }

        if best.is_none() || d < best.unwrap().1 {
            best = Some((entity, d));
        }
    }
    best.map(|(e, _)| e)
}

pub fn find_resource_biome_pos(
    kind: EntityKind,
    base_pos: Vec3,
    biome_map: &BiomeMap,
    height_map: &HeightMap,
) -> Option<Vec3> {
    let target_biome = match kind {
        EntityKind::Sawmill => Some(Biome::Forest),
        EntityKind::Mine => Some(Biome::Mud),
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
    enemy_buildings: &Query<(&Faction, &Transform), With<Building>>,
    teams: &TeamConfig,
    faction: &Faction,
) -> Option<Vec3> {
    // Priority 1: active threats near base (most recent, closest)
    let mut near_threats: Vec<&ThreatEntry> = threats
        .iter()
        .filter(|t| {
            t.position.distance(base_pos) < BASE_THREAT_RADIUS * 3.0
                && t.estimated_strength > 0.0
        })
        .collect();
    near_threats.sort_by(|a, b| {
        a.position
            .distance(base_pos)
            .partial_cmp(&b.position.distance(base_pos))
            .unwrap()
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
        a.estimated_strength
            .partial_cmp(&b.estimated_strength)
            .unwrap()
    });
    if let Some(threat) = valid_threats.first() {
        return Some(threat.position);
    }

    // Priority 3: nearest enemy building
    let mut best: Option<(Vec3, f32)> = None;
    for (f, tf) in enemy_buildings.iter() {
        if !teams.is_hostile(faction, f) || *f == Faction::Neutral {
            continue;
        }
        let d = base_pos.distance(tf.translation);
        if best.is_none() || d < best.unwrap().1 {
            best = Some((tf.translation, d));
        }
    }
    best.map(|(pos, _)| pos)
}

pub fn find_enemy_resource_area(
    buildings: &Query<(&Faction, &Transform), With<Building>>,
    teams: &TeamConfig,
    faction: &Faction,
) -> Option<Vec3> {
    let mut best: Option<(Vec3, f32)> = None;
    let origin = Vec3::ZERO;
    for (f, tf) in buildings.iter() {
        if !teams.is_hostile(faction, f) || *f == Faction::Neutral {
            continue;
        }
        let d = origin.distance(tf.translation);
        if best.is_none() || d < best.unwrap().1 {
            best = Some((tf.translation, d));
        }
    }
    best.map(|(pos, _)| {
        let to_center = (Vec3::ZERO - pos).normalize_or_zero();
        pos + to_center * 30.0
    })
}

pub fn try_train(
    train_queues: &mut Query<(&Faction, &EntityKind, &mut TrainingQueue), With<Building>>,
    faction: &Faction,
    unit_kind: EntityKind,
    registry: &BlueprintRegistry,
) -> bool {
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
    let radius = match personality {
        AiPersonality::Defensive => 45.0,
        _ => 35.0,
    };

    // 4 corners of the rectangle
    let corners = [
        Vec3::new(base_pos.x - radius, 0.0, base_pos.z - radius),
        Vec3::new(base_pos.x + radius, 0.0, base_pos.z - radius),
        Vec3::new(base_pos.x + radius, 0.0, base_pos.z + radius),
        Vec3::new(base_pos.x - radius, 0.0, base_pos.z + radius),
    ];

    // Leave a gate opening on the side facing map center
    let to_center = (Vec3::ZERO - base_pos).normalize_or_zero();
    let gate_side = if to_center.x.abs() > to_center.z.abs() {
        if to_center.x > 0.0 { 1 } else { 3 }
    } else {
        if to_center.z > 0.0 { 2 } else { 0 }
    };

    let sides = [
        (corners[0], corners[1]),
        (corners[1], corners[2]),
        (corners[2], corners[3]),
        (corners[3], corners[0]),
    ];

    // Skip the gate side
    let gate_opening = 10.0;
    let (a, b) = sides[gate_side];
    let dir = (b - a).normalize_or_zero();
    let len = a.distance(b);
    let half_opening = gate_opening / 2.0;
    let mid = len / 2.0;
    let _ = (dir, half_opening, mid);

    let mut completed = [false; 4];
    completed[gate_side] = true;

    WallPlan { sides, completed }
}

/// Generate evenly-spaced wall points between two positions
pub fn generate_wall_points(start: Vec3, end: Vec3, height_map: &HeightMap) -> Vec<Vec3> {
    let dist = start.distance(end);
    let segment_len = 4.0;
    let num_posts = (dist / segment_len).ceil() as usize + 1;
    let num_posts = num_posts.max(2).min(20);

    let mut points = Vec::with_capacity(num_posts);
    for i in 0..num_posts {
        let t = i as f32 / (num_posts - 1).max(1) as f32;
        let p = start.lerp(end, t);
        let y = height_map.sample(p.x, p.z);
        points.push(Vec3::new(p.x, y, p.z));
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
