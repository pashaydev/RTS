use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::{
    BlueprintRegistry, EntityKind, EntityVisualCache, spawn_from_blueprint_with_faction,
};
use crate::buildings::{footprint_for_kind, start_upgrade};
use crate::components::*;
use crate::ground::HeightMap;
use crate::model_assets::BuildingModelAssets;

// ── Constants ──

const STRATEGY_TICK: f32 = 2.0;
const ECONOMY_TICK: f32 = 2.0;
const MILITARY_TICK: f32 = 2.0;
const TACTICAL_TICK: f32 = 0.5;
const SCOUT_TICK: f32 = 10.0;

const MAX_BUILD_QUEUE: usize = 3;
const DEFENSE_SQUAD_SIZE: usize = 4;
const UNDER_ATTACK_COOLDOWN: f32 = 15.0;
const ATTACK_MIN_INTERVAL: f32 = 30.0;
const THREAT_DECAY_SECS: f32 = 120.0;
const SCOUT_RADIUS: f32 = 150.0;
const BASE_THREAT_RADIUS: f32 = 50.0;
const MAP_HALF: f32 = 245.0;

// ── Enums ──

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum StrategyPhase {
    #[default]
    EarlyGame,
    MidGame,
    LateGame,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum TacticalPosture {
    #[default]
    Normal,
    UnderAttack,
    Retreating,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
enum SquadRole {
    GatherWood,
    GatherCopper,
    GatherIron,
    GatherGold,
    GatherOil,
    BuildConstruction,
    DefenseSquad,
    AttackSquad,
    Scout,
}

impl SquadRole {
    fn for_resource(rt: ResourceType) -> Self {
        match rt {
            ResourceType::Wood => SquadRole::GatherWood,
            ResourceType::Copper => SquadRole::GatherCopper,
            ResourceType::Iron => SquadRole::GatherIron,
            ResourceType::Gold => SquadRole::GatherGold,
            ResourceType::Oil => SquadRole::GatherOil,
        }
    }

    fn is_gather(&self) -> bool {
        matches!(
            self,
            SquadRole::GatherWood
                | SquadRole::GatherCopper
                | SquadRole::GatherIron
                | SquadRole::GatherGold
                | SquadRole::GatherOil
        )
    }

    fn resource_type(&self) -> Option<ResourceType> {
        match self {
            SquadRole::GatherWood => Some(ResourceType::Wood),
            SquadRole::GatherCopper => Some(ResourceType::Copper),
            SquadRole::GatherIron => Some(ResourceType::Iron),
            SquadRole::GatherGold => Some(ResourceType::Gold),
            SquadRole::GatherOil => Some(ResourceType::Oil),
            _ => None,
        }
    }
}

// ── Data Structs ──

#[derive(Clone, Debug)]
struct Squad {
    role: SquadRole,
    members: Vec<Entity>,
    rally_point: Option<Vec3>,
}

#[derive(Clone, Debug)]
struct ThreatEntry {
    position: Vec3,
    estimated_strength: f32,
    last_seen: f32,
    entity_count: u32,
}

#[derive(Clone, Debug)]
struct BuildRequest {
    kind: EntityKind,
    priority: u8,
    near_position: Option<Vec3>,
}

// ── Per-Faction AI Brain ──

#[derive(Default)]
struct AiFactionBrain {
    // Timers
    strategy_timer: f32,
    economy_timer: f32,
    military_timer: f32,
    tactical_timer: f32,
    scout_timer: f32,

    // State
    phase: StrategyPhase,
    posture: TacticalPosture,
    posture_cooldown: f32,
    game_time: f32,

    // Squads
    squads: Vec<Squad>,
    assigned_units: HashMap<Entity, SquadRole>,

    // Economy
    desired_workers: u8,
    build_queue: Vec<BuildRequest>,
    pending_builds: u8,

    // Military
    attack_ready: bool,
    last_attack_time: f32,

    // Intel
    known_threats: Vec<ThreatEntry>,
    next_scout_waypoint: usize,
    scout_route: Vec<Vec3>,

    // Cached
    base_position: Option<Vec3>,

    // Track previous health for damage detection
    prev_health: HashMap<Entity, f32>,
}

impl AiFactionBrain {
    fn new_with_offsets(offset: f32) -> Self {
        Self {
            strategy_timer: 0.0,
            economy_timer: offset,
            military_timer: offset * 0.5,
            tactical_timer: 0.0,
            desired_workers: 6,
            ..Default::default()
        }
    }

    fn get_squad(&self, role: SquadRole) -> Option<&Squad> {
        self.squads.iter().find(|s| s.role == role)
    }

    fn get_squad_mut(&mut self, role: SquadRole) -> Option<&mut Squad> {
        self.squads.iter_mut().find(|s| s.role == role)
    }

    fn ensure_squad(&mut self, role: SquadRole) -> &mut Squad {
        if !self.squads.iter().any(|s| s.role == role) {
            self.squads.push(Squad {
                role,
                members: Vec::new(),
                rally_point: None,
            });
        }
        self.squads.iter_mut().find(|s| s.role == role).unwrap()
    }

    fn squad_size(&self, role: SquadRole) -> usize {
        self.get_squad(role).map_or(0, |s| s.members.len())
    }

    fn add_to_squad(&mut self, entity: Entity, role: SquadRole) {
        self.ensure_squad(role).members.push(entity);
        self.assigned_units.insert(entity, role);
    }

    fn remove_from_squad(&mut self, entity: Entity) {
        if let Some(role) = self.assigned_units.remove(&entity) {
            if let Some(squad) = self.get_squad_mut(role) {
                squad.members.retain(|&e| e != entity);
            }
        }
    }

    fn prune_dead(&mut self, alive: &std::collections::HashSet<Entity>) {
        for squad in &mut self.squads {
            squad.members.retain(|e| alive.contains(e));
        }
        self.assigned_units.retain(|e, _| alive.contains(e));
        self.prev_health.retain(|e, _| alive.contains(e));
    }
}

// ── Resource ──

#[derive(Resource, Default)]
struct AiState {
    factions: HashMap<Faction, AiFactionBrain>,
}

// ── Plugin ──

pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiState>()
            .add_systems(Update, (ai_strategy_system, ai_economy_system))
            .add_systems(Update, (ai_military_system, ai_tactical_system));
    }
}

// ════════════════════════════════════════════════════════════════════
// System 1: Strategy — Phase transitions & build queue planning
// ════════════════════════════════════════════════════════════════════

fn ai_strategy_system(
    time: Res<Time>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    mut ai_state: ResMut<AiState>,
    all_completed: Res<AllCompletedBuildings>,
    buildings_q: Query<(&Faction, &EntityKind, &BuildingState), With<Building>>,
    units_q: Query<(&Faction, &EntityKind), With<Unit>>,
) {
    let dt = time.delta_secs();

    for &faction in &Faction::PLAYERS {
        if teams.is_allied(&faction, &active_player.0) {
            continue;
        }

        let brain = ai_state.factions.entry(faction).or_insert_with(|| {
            let idx = Faction::PLAYERS.iter().position(|f| *f == faction).unwrap_or(0);
            AiFactionBrain::new_with_offsets(idx as f32 * 0.3)
        });

        brain.strategy_timer -= dt;
        if brain.strategy_timer > 0.0 {
            continue;
        }
        brain.strategy_timer = STRATEGY_TICK;
        brain.game_time += STRATEGY_TICK;

        // Cache counts
        let mut building_counts: HashMap<EntityKind, usize> = HashMap::new();
        let mut completed_building_counts: HashMap<EntityKind, usize> = HashMap::new();
        for (f, kind, state) in buildings_q.iter() {
            if *f != faction {
                continue;
            }
            *building_counts.entry(*kind).or_default() += 1;
            if *state == BuildingState::Complete {
                *completed_building_counts.entry(*kind).or_default() += 1;
            }
        }

        let mut worker_count = 0usize;
        let mut military_count = 0usize;
        for (f, kind) in units_q.iter() {
            if *f != faction {
                continue;
            }
            if *kind == EntityKind::Worker {
                worker_count += 1;
            } else {
                military_count += 1;
            }
        }

        // Count buildings under construction
        let mut under_construction = 0u8;
        for (f, _, state) in buildings_q.iter() {
            if *f == faction && *state == BuildingState::UnderConstruction {
                under_construction += 1;
            }
        }
        brain.pending_builds = under_construction;

        // Phase transitions
        match brain.phase {
            StrategyPhase::EarlyGame => {
                if worker_count >= 6
                    && all_completed.has(&faction, EntityKind::Barracks)
                    && all_completed.has(&faction, EntityKind::Storage)
                    && brain.game_time > 120.0
                {
                    brain.phase = StrategyPhase::MidGame;
                }
            }
            StrategyPhase::MidGame => {
                if military_count >= 12
                    && (all_completed.has(&faction, EntityKind::Workshop)
                        || all_completed.has(&faction, EntityKind::Stable))
                    && brain.game_time > 360.0
                {
                    brain.phase = StrategyPhase::LateGame;
                }
            }
            StrategyPhase::LateGame => {}
        }

        // Posture management
        if brain.posture == TacticalPosture::Retreating && military_count >= 6 {
            brain.posture = TacticalPosture::Normal;
        }
        if brain.posture != TacticalPosture::Normal {
            brain.posture_cooldown -= STRATEGY_TICK;
        }

        // Set desired workers
        brain.desired_workers = match brain.phase {
            StrategyPhase::EarlyGame => 6,
            StrategyPhase::MidGame => 10,
            StrategyPhase::LateGame => 8,
        };

        // Build queue
        brain.build_queue.clear();
        let tc = &building_counts; // total (including under construction)

        match brain.phase {
            StrategyPhase::EarlyGame => {
                if tc.get(&EntityKind::Barracks).copied().unwrap_or(0) == 0 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::Barracks,
                        priority: 0,
                        near_position: None,
                    });
                }
                if tc.get(&EntityKind::Storage).copied().unwrap_or(0) == 0 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::Storage,
                        priority: 1,
                        near_position: None,
                    });
                }
                if tc.get(&EntityKind::Sawmill).copied().unwrap_or(0) == 0 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::Sawmill,
                        priority: 2,
                        near_position: None, // will be resolved in economy system
                    });
                }
                if tc.get(&EntityKind::Tower).copied().unwrap_or(0) == 0 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::Tower,
                        priority: 3,
                        near_position: None,
                    });
                }
            }
            StrategyPhase::MidGame => {
                if tc.get(&EntityKind::Barracks).copied().unwrap_or(0) < 2 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::Barracks,
                        priority: 0,
                        near_position: None,
                    });
                }
                if tc.get(&EntityKind::Workshop).copied().unwrap_or(0) == 0 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::Workshop,
                        priority: 1,
                        near_position: None,
                    });
                }
                if tc.get(&EntityKind::Stable).copied().unwrap_or(0) == 0 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::Stable,
                        priority: 2,
                        near_position: None,
                    });
                }
                if tc.get(&EntityKind::Tower).copied().unwrap_or(0) < 2 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::Tower,
                        priority: 1,
                        near_position: None,
                    });
                }
                if tc.get(&EntityKind::Mine).copied().unwrap_or(0) == 0 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::Mine,
                        priority: 3,
                        near_position: None,
                    });
                }
                if tc.get(&EntityKind::MageTower).copied().unwrap_or(0) == 0 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::MageTower,
                        priority: 3,
                        near_position: None,
                    });
                }
            }
            StrategyPhase::LateGame => {
                if tc.get(&EntityKind::SiegeWorks).copied().unwrap_or(0) == 0 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::SiegeWorks,
                        priority: 0,
                        near_position: None,
                    });
                }
                if tc.get(&EntityKind::Temple).copied().unwrap_or(0) == 0 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::Temple,
                        priority: 1,
                        near_position: None,
                    });
                }
                if tc.get(&EntityKind::Tower).copied().unwrap_or(0) < 4 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::Tower,
                        priority: 2,
                        near_position: None,
                    });
                }
                if tc.get(&EntityKind::OilRig).copied().unwrap_or(0) == 0 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::OilRig,
                        priority: 2,
                        near_position: None,
                    });
                }
                // Second storage for capacity
                if tc.get(&EntityKind::Storage).copied().unwrap_or(0) < 2 {
                    brain.build_queue.push(BuildRequest {
                        kind: EntityKind::Storage,
                        priority: 3,
                        near_position: None,
                    });
                }
            }
        }

        // Sort by priority
        brain.build_queue.sort_by_key(|r| r.priority);

        // Attack readiness
        let attack_threshold = match brain.phase {
            StrategyPhase::EarlyGame => 999,
            StrategyPhase::MidGame => 8,
            StrategyPhase::LateGame => 12,
        };
        let attack_squad_size = brain.squad_size(SquadRole::AttackSquad);
        brain.attack_ready = attack_squad_size >= attack_threshold;
    }
}

// ════════════════════════════════════════════════════════════════════
// System 2: Economy — Workers, construction, building placement
// ════════════════════════════════════════════════════════════════════

fn ai_economy_system(
    mut commands: Commands,
    time: Res<Time>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    mut ai_state: ResMut<AiState>,
    mut all_resources: ResMut<AllPlayerResources>,
    all_completed: Res<AllCompletedBuildings>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    building_models: Option<Res<BuildingModelAssets>>,
    height_map: Res<HeightMap>,
    biome_map: Res<BiomeMap>,
    queries: (
        Query<(Entity, &Faction, &Transform, &WorkerTask), (With<Unit>, With<GatherSpeed>)>,
        Query<(Entity, &Transform, &ResourceNode), Without<Unit>>,
        Query<(Entity, &Faction, &EntityKind, &Transform, &BuildingState), With<Building>>,
        Query<(&Faction, &EntityKind, &BuildingLevel, Entity, &BuildingState), With<Building>>,
        Query<(&Faction, &ConstructionWorkers, &BuildingState), With<Building>>,
        Query<(&Faction, &EntityKind, &mut TrainingQueue), With<Building>>,
        Query<&BuildingFootprint>,
    ),
) {
    let dt = time.delta_secs();
    let (workers_q, resource_nodes_q, buildings_q, building_levels_q, construction_workers_q, mut train_queues, footprints_q) = queries;

    for &faction in &Faction::PLAYERS {
        if teams.is_allied(&faction, &active_player.0) {
            continue;
        }

        let brain = match ai_state.factions.get_mut(&faction) {
            Some(b) => b,
            None => continue,
        };

        brain.economy_timer -= dt;
        if brain.economy_timer > 0.0 {
            continue;
        }
        brain.economy_timer = ECONOMY_TICK;

        // Cache base position
        let mut base_pos = None;
        let mut our_building_positions: Vec<Vec3> = Vec::new();
        for (_, f, kind, tf, _) in buildings_q.iter() {
            if *f == faction {
                our_building_positions.push(tf.translation);
                if *kind == EntityKind::Base {
                    base_pos = Some(tf.translation);
                }
            }
        }
        let base_pos = match base_pos {
            Some(p) => p,
            None => continue,
        };
        brain.base_position = Some(base_pos);

        // Count workers
        let mut worker_count = 0usize;
        for (_, f, _, _) in workers_q.iter() {
            if *f == faction {
                worker_count += 1;
            }
        }

        // ── Train workers if needed ──
        let phase = brain.phase;
        let desired = brain.desired_workers as usize;
        if worker_count < desired {
            let bp = registry.get(EntityKind::Worker);
            if bp.cost.can_afford(all_resources.get(&faction)) {
                if try_train(&mut train_queues, &faction, EntityKind::Worker, &registry) {
                    bp.cost.deduct(all_resources.get_mut(&faction));
                }
            }
        }

        // ── Assign idle workers to gather squads ──
        let pr = all_resources.get(&faction);
        let player_res = PlayerResources {
            wood: pr.wood, copper: pr.copper, iron: pr.iron, gold: pr.gold, oil: pr.oil,
        };
        let mut idle_workers: Vec<(Entity, Vec3)> = Vec::new();
        for (entity, f, tf, task) in workers_q.iter() {
            if *f != faction {
                continue;
            }
            if *task == WorkerTask::Idle && !brain.assigned_units.contains_key(&entity) {
                idle_workers.push((entity, tf.translation));
            }
        }

        for (entity, pos) in &idle_workers {
            let needed = pick_most_needed_resource(&player_res, phase);
            let role = SquadRole::for_resource(needed);

            // Find nearest resource node of that type
            if let Some(node_entity) = find_nearest_resource_node(
                *pos,
                needed,
                &resource_nodes_q,
                200.0,
            ) {
                commands
                    .entity(*entity)
                    .insert(WorkerTask::MovingToResource(node_entity));
                brain.add_to_squad(*entity, role);
            }
        }

        // ── Assign workers to construction ──
        for (entity, f, _kind, tf, state) in buildings_q.iter() {
            if *f != faction || *state != BuildingState::UnderConstruction {
                continue;
            }
            // Check how many workers are assigned to build this
            let cw = construction_workers_q
                .get(entity)
                .map(|(_, cw, _)| cw.0)
                .unwrap_or(0);

            if cw < 2 {
                // Find nearest unassigned idle worker
                let mut best: Option<(Entity, f32)> = None;
                for (w_entity, w_f, w_tf, w_task) in workers_q.iter() {
                    if *w_f != faction {
                        continue;
                    }
                    if *w_task != WorkerTask::Idle {
                        continue;
                    }
                    // Prefer unassigned, but also allow reassigning gather workers
                    let role = brain.assigned_units.get(&w_entity);
                    if role.is_some() && !role.unwrap().is_gather() {
                        continue;
                    }
                    let d = w_tf.translation.distance(tf.translation);
                    if best.is_none() || d < best.unwrap().1 {
                        best = Some((w_entity, d));
                    }
                }
                if let Some((w_entity, _)) = best {
                    brain.remove_from_squad(w_entity);
                    brain.add_to_squad(w_entity, SquadRole::BuildConstruction);
                    commands
                        .entity(w_entity)
                        .insert(WorkerTask::MovingToBuild(entity));
                }
            }
        }

        // ── Execute build queue ──
        let pending = brain.pending_builds;
        if pending < MAX_BUILD_QUEUE as u8 {
            let build_queue = brain.build_queue.clone();
            for request in &build_queue {
                let bp = registry.get(request.kind);

                // Check prerequisite
                if let Some(ref bd) = bp.building {
                    if let Some(prereq) = bd.prerequisite {
                        if !all_completed.has(&faction, prereq) {
                            continue;
                        }
                    }
                }

                if !bp.cost.can_afford(all_resources.get(&faction)) {
                    continue;
                }

                // Find position
                let near = request.near_position.or_else(|| {
                    find_resource_biome_pos(request.kind, base_pos, &biome_map, &height_map)
                });
                let pos = find_build_pos(
                    base_pos,
                    &our_building_positions,
                    request.kind,
                    &footprints_q,
                    &height_map,
                    near,
                );

                bp.cost.deduct(all_resources.get_mut(&faction));
                spawn_ai_building(
                    &mut commands,
                    &cache,
                    request.kind,
                    pos,
                    &registry,
                    building_models.as_deref(),
                    &height_map,
                    faction,
                );
                brain.pending_builds += 1;
                break; // One building per tick
            }
        }

        // ── Building upgrades (MidGame+) ──
        if matches!(phase, StrategyPhase::MidGame | StrategyPhase::LateGame) {
            let upgrade_priorities = [
                EntityKind::Tower,
                EntityKind::Barracks,
                EntityKind::Storage,
            ];
            for target_kind in &upgrade_priorities {
                for (f, kind, level, entity, state) in building_levels_q.iter() {
                    if *f != faction
                        || kind != target_kind
                        || *state != BuildingState::Complete
                        || level.0 >= 3
                    {
                        continue;
                    }
                    let mut res = PlayerResources {
                        wood: all_resources.get(&faction).wood,
                        copper: all_resources.get(&faction).copper,
                        iron: all_resources.get(&faction).iron,
                        gold: all_resources.get(&faction).gold,
                        oil: all_resources.get(&faction).oil,
                    };
                    if start_upgrade(
                        &mut commands,
                        entity,
                        level.0,
                        *kind,
                        &registry,
                        &mut res,
                        faction,
                    ) {
                        // Write back the deducted resources
                        *all_resources.get_mut(&faction) = res;
                        break;
                    }
                }
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// System 3: Military — Army composition, squads, attacks, scouting
// ════════════════════════════════════════════════════════════════════

fn ai_military_system(
    mut commands: Commands,
    time: Res<Time>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    mut ai_state: ResMut<AiState>,
    mut all_resources: ResMut<AllPlayerResources>,
    _all_completed: Res<AllCompletedBuildings>,
    registry: Res<BlueprintRegistry>,
    units_q: Query<
        (Entity, &Faction, &EntityKind, &Transform),
        (With<Unit>, Without<Building>),
    >,
    idle_military_q: Query<
        (Entity, &Faction, &EntityKind, &Transform),
        (
            With<Unit>,
            Without<AttackTarget>,
            Without<WorkerTask>,
            Without<MoveTarget>,
            Without<Building>,
        ),
    >,
    enemy_buildings_q: Query<(&Faction, &Transform), With<Building>>,
    mut train_queues: Query<(&Faction, &EntityKind, &mut TrainingQueue), With<Building>>,
) {
    let dt = time.delta_secs();

    for &faction in &Faction::PLAYERS {
        if teams.is_allied(&faction, &active_player.0) {
            continue;
        }

        let brain = match ai_state.factions.get_mut(&faction) {
            Some(b) => b,
            None => continue,
        };

        brain.military_timer -= dt;
        if brain.military_timer > 0.0 {
            continue;
        }
        brain.military_timer = MILITARY_TICK;

        let base_pos = match brain.base_position {
            Some(p) => p,
            None => continue,
        };

        // Prune dead entities
        let alive: std::collections::HashSet<Entity> = units_q
            .iter()
            .filter(|(_, f, _, _)| **f == faction)
            .map(|(e, _, _, _)| e)
            .collect();
        brain.prune_dead(&alive);

        // Count our units by type
        let mut unit_counts: HashMap<EntityKind, usize> = HashMap::new();
        let mut military_count = 0usize;
        for (_, f, kind, _) in units_q.iter() {
            if *f != faction {
                continue;
            }
            *unit_counts.entry(*kind).or_default() += 1;
            if *kind != EntityKind::Worker {
                military_count += 1;
            }
        }

        // Check for retreating posture
        if brain.posture == TacticalPosture::Normal && military_count < 4 && brain.game_time > 120.0 {
            brain.posture = TacticalPosture::Retreating;
            brain.posture_cooldown = 20.0;
        }

        let phase = brain.phase;

        // ── Composition-driven training ──
        let desired_composition: Vec<(EntityKind, usize)> = match phase {
            StrategyPhase::EarlyGame => vec![
                (EntityKind::Soldier, 3),
                (EntityKind::Archer, 2),
            ],
            StrategyPhase::MidGame => vec![
                (EntityKind::Soldier, 4),
                (EntityKind::Archer, 3),
                (EntityKind::Knight, 2),
                (EntityKind::Mage, 1),
            ],
            StrategyPhase::LateGame => vec![
                (EntityKind::Soldier, 3),
                (EntityKind::Archer, 3),
                (EntityKind::Knight, 3),
                (EntityKind::Mage, 2),
                (EntityKind::Cavalry, 2),
                (EntityKind::Catapult, 1),
                (EntityKind::BatteringRam, 1),
            ],
        };

        // Find most under-represented unit type and train it
        let mut best_deficit: Option<(EntityKind, f32)> = None;
        for (kind, desired) in &desired_composition {
            let current = unit_counts.get(kind).copied().unwrap_or(0);
            if current < *desired {
                let deficit = (*desired - current) as f32 / *desired as f32;
                if best_deficit.is_none() || deficit > best_deficit.unwrap().1 {
                    best_deficit = Some((*kind, deficit));
                }
            }
        }

        if let Some((unit_kind, _)) = best_deficit {
            let bp = registry.get(unit_kind);
            if bp.cost.can_afford(all_resources.get(&faction)) {
                if try_train(&mut train_queues, &faction, unit_kind, &registry) {
                    bp.cost.deduct(all_resources.get_mut(&faction));
                }
            }
        }

        // ── Assign unassigned military to squads ──
        let mut unassigned: Vec<(Entity, EntityKind, Vec3)> = Vec::new();
        for (entity, f, kind, tf) in units_q.iter() {
            if *f != faction || *kind == EntityKind::Worker {
                continue;
            }
            if !brain.assigned_units.contains_key(&entity) {
                unassigned.push((entity, *kind, tf.translation));
            }
        }

        for (entity, _, _) in &unassigned {
            let defense_size = brain.squad_size(SquadRole::DefenseSquad);
            if defense_size < DEFENSE_SQUAD_SIZE {
                brain.add_to_squad(*entity, SquadRole::DefenseSquad);
                commands.entity(*entity).insert(MoveTarget(base_pos));
            } else {
                brain.add_to_squad(*entity, SquadRole::AttackSquad);
            }
        }

        // ── Scouting (MidGame+) ──
        if matches!(phase, StrategyPhase::MidGame | StrategyPhase::LateGame)
            && brain.squad_size(SquadRole::Scout) == 0
        {
            // Pick a fast unit from attack squad
            let attack_members: Vec<Entity> = brain
                .get_squad(SquadRole::AttackSquad)
                .map(|s| s.members.clone())
                .unwrap_or_default();

            // Prefer cavalry, then any available
            let scout_candidate = attack_members.iter().find(|&&e| {
                units_q.get(e).map_or(false, |(_, _, k, _)| *k == EntityKind::Cavalry)
            }).or_else(|| attack_members.first());

            if let Some(&scout_entity) = scout_candidate {
                brain.remove_from_squad(scout_entity);
                brain.add_to_squad(scout_entity, SquadRole::Scout);

                if brain.scout_route.is_empty() {
                    brain.scout_route = compute_scout_route(base_pos);
                }
            }
        }

        // Move scout
        brain.scout_timer -= MILITARY_TICK;
        if brain.scout_timer <= 0.0 {
            brain.scout_timer = SCOUT_TICK;
            let route = brain.scout_route.clone();
            let waypoint_idx = brain.next_scout_waypoint;
            if !route.is_empty() {
                if let Some(squad) = brain.get_squad(SquadRole::Scout) {
                    for &entity in &squad.members {
                        let wp = route[waypoint_idx % route.len()];
                        commands.entity(entity).insert(MoveTarget(wp));
                    }
                }
                brain.next_scout_waypoint = (waypoint_idx + 1) % route.len().max(1);
            }
        }

        // ── Attack decision ──
        let posture = brain.posture;
        let attack_ready = brain.attack_ready;
        let last_attack_time = brain.last_attack_time;
        let game_time = brain.game_time;

        if posture == TacticalPosture::Normal
            && attack_ready
            && (game_time - last_attack_time) > ATTACK_MIN_INTERVAL
        {
            // Pick target: nearest enemy building
            let target = pick_attack_target(
                base_pos,
                &brain.known_threats,
                &enemy_buildings_q,
                &teams,
                &faction,
            );

            if let Some(target_pos) = target {
                let attack_members: Vec<Entity> = brain
                    .get_squad(SquadRole::AttackSquad)
                    .map(|s| s.members.clone())
                    .unwrap_or_default();

                for entity in &attack_members {
                    commands.entity(*entity).insert(MoveTarget(target_pos));
                }
                brain.last_attack_time = game_time;
                brain.attack_ready = false;
            }
        }

        // ── Rally idle attack units ──
        if posture == TacticalPosture::Normal && !attack_ready {
            let rally = base_pos + (Vec3::ZERO - base_pos).normalize_or_zero() * 30.0;
            let attack_members: Vec<Entity> = brain
                .get_squad(SquadRole::AttackSquad)
                .map(|s| s.members.clone())
                .unwrap_or_default();

            for &entity in &attack_members {
                // Only rally idle units
                if idle_military_q.get(entity).is_ok() {
                    commands.entity(entity).insert(MoveTarget(rally));
                }
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// System 4: Tactical — Fast reactions, threat detection, defense
// ════════════════════════════════════════════════════════════════════

fn ai_tactical_system(
    mut commands: Commands,
    time: Res<Time>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    mut ai_state: ResMut<AiState>,
    own_entities_q: Query<
        (Entity, &Faction, &Transform, &Health),
        Or<(With<Unit>, With<Building>)>,
    >,
    enemy_units_q: Query<
        (Entity, &Faction, &Transform, &Health, Option<&AttackDamage>),
        Or<(With<Unit>, With<Mob>)>,
    >,
) {
    let dt = time.delta_secs();

    for &faction in &Faction::PLAYERS {
        if teams.is_allied(&faction, &active_player.0) {
            continue;
        }

        let brain = match ai_state.factions.get_mut(&faction) {
            Some(b) => b,
            None => continue,
        };

        brain.tactical_timer -= dt;
        if brain.tactical_timer > 0.0 {
            continue;
        }
        brain.tactical_timer = TACTICAL_TICK;

        let base_pos = match brain.base_position {
            Some(p) => p,
            None => continue,
        };

        let game_time = brain.game_time;

        // ── Detect damage on own entities ──
        let mut threats_detected = false;
        let mut threat_positions: Vec<Vec3> = Vec::new();

        for (entity, f, tf, health) in own_entities_q.iter() {
            if *f != faction {
                continue;
            }
            let prev = brain.prev_health.get(&entity).copied();
            brain.prev_health.insert(entity, health.current);

            if let Some(prev_hp) = prev {
                if health.current < prev_hp {
                    // We're taking damage — find nearby enemies
                    let pos = tf.translation;
                    for (_, ef, etf, _, _) in enemy_units_q.iter() {
                        if !teams.is_hostile(&faction, ef) {
                            continue;
                        }
                        if etf.translation.distance(pos) < 25.0 {
                            threat_positions.push(etf.translation);
                            threats_detected = true;
                        }
                    }
                }
            }
        }

        // ── Update threat map from visible enemies near base ──
        for (_, ef, etf, health, damage) in enemy_units_q.iter() {
            if !teams.is_hostile(&faction, ef) {
                continue;
            }
            let pos = etf.translation;
            // Only track enemies somewhat near our territory
            if pos.distance(base_pos) < 100.0 {
                let strength = health.current * damage.map_or(5.0, |d| d.0);
                update_threat(&mut brain.known_threats, pos, strength, game_time);
                if pos.distance(base_pos) < BASE_THREAT_RADIUS {
                    threats_detected = true;
                    threat_positions.push(pos);
                }
            }
        }

        // ── Trigger UnderAttack ──
        if threats_detected && brain.posture == TacticalPosture::Normal {
            brain.posture = TacticalPosture::UnderAttack;
            brain.posture_cooldown = UNDER_ATTACK_COOLDOWN;
        }

        // ── Defensive response ──
        if brain.posture == TacticalPosture::UnderAttack {
            let threat_center = if !threat_positions.is_empty() {
                let sum: Vec3 = threat_positions.iter().copied().sum();
                sum / threat_positions.len() as f32
            } else {
                base_pos
            };

            // Recall defense + attack squads
            let mut recall_entities: Vec<Entity> = Vec::new();
            if let Some(squad) = brain.get_squad(SquadRole::DefenseSquad) {
                recall_entities.extend(&squad.members);
            }
            if let Some(squad) = brain.get_squad(SquadRole::AttackSquad) {
                recall_entities.extend(&squad.members);
            }

            for entity in &recall_entities {
                commands.entity(*entity).insert(MoveTarget(threat_center));
            }
        }

        // ── Posture cooldown ──
        if brain.posture == TacticalPosture::UnderAttack {
            brain.posture_cooldown -= TACTICAL_TICK;
            if brain.posture_cooldown <= 0.0 && !threats_detected {
                brain.posture = TacticalPosture::Normal;
            }
        }

        // ── Decay old threats ──
        brain
            .known_threats
            .retain(|t| game_time - t.last_seen < THREAT_DECAY_SECS);
    }
}

// ════════════════════════════════════════════════════════════════════
// Helper Functions
// ════════════════════════════════════════════════════════════════════

fn pick_most_needed_resource(res: &PlayerResources, phase: StrategyPhase) -> ResourceType {
    let weights: [(ResourceType, f32); 5] = match phase {
        StrategyPhase::EarlyGame => [
            (ResourceType::Wood, 3.0),
            (ResourceType::Copper, 2.0),
            (ResourceType::Iron, 1.0),
            (ResourceType::Gold, 0.2),
            (ResourceType::Oil, 0.0),
        ],
        StrategyPhase::MidGame => [
            (ResourceType::Wood, 2.0),
            (ResourceType::Copper, 2.0),
            (ResourceType::Iron, 2.0),
            (ResourceType::Gold, 1.0),
            (ResourceType::Oil, 0.5),
        ],
        StrategyPhase::LateGame => [
            (ResourceType::Wood, 1.0),
            (ResourceType::Copper, 1.5),
            (ResourceType::Iron, 2.0),
            (ResourceType::Gold, 2.0),
            (ResourceType::Oil, 1.5),
        ],
    };

    let get_amount = |rt: ResourceType| -> u32 {
        match rt {
            ResourceType::Wood => res.wood,
            ResourceType::Copper => res.copper,
            ResourceType::Iron => res.iron,
            ResourceType::Gold => res.gold,
            ResourceType::Oil => res.oil,
        }
    };

    let mut best_rt = ResourceType::Wood;
    let mut best_score = f32::MIN;
    for (rt, weight) in &weights {
        if *weight <= 0.0 {
            continue;
        }
        let amount = get_amount(*rt) as f32;
        let score = weight / (amount + 50.0);
        if score > best_score {
            best_score = score;
            best_rt = *rt;
        }
    }
    best_rt
}

fn find_nearest_resource_node(
    pos: Vec3,
    resource_type: ResourceType,
    nodes: &Query<(Entity, &Transform, &ResourceNode), Without<Unit>>,
    max_range: f32,
) -> Option<Entity> {
    let mut best: Option<(Entity, f32)> = None;
    for (entity, tf, node) in nodes.iter() {
        if node.resource_type != resource_type || node.amount_remaining == 0 {
            continue;
        }
        let d = pos.distance(tf.translation);
        if d < max_range && (best.is_none() || d < best.unwrap().1) {
            best = Some((entity, d));
        }
    }
    best.map(|(e, _)| e)
}

fn find_resource_biome_pos(
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

    // Search in expanding rings from base for the target biome
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
                // For OilRig near water, place just at the edge (not in water)
                if kind == EntityKind::OilRig {
                    // Step back toward base slightly to avoid placing in deep water
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

fn compute_scout_route(base_pos: Vec3) -> Vec<Vec3> {
    let center = Vec3::ZERO;
    let mut route = Vec::new();

    // Start from quadrant nearest to base, go clockwise
    let base_angle = (base_pos.z - center.z).atan2(base_pos.x - center.x);

    for i in 0..8 {
        let angle = base_angle + i as f32 / 8.0 * std::f32::consts::TAU;
        let x = center.x + angle.cos() * SCOUT_RADIUS;
        let z = center.z + angle.sin() * SCOUT_RADIUS;
        let x = x.clamp(-MAP_HALF, MAP_HALF);
        let z = z.clamp(-MAP_HALF, MAP_HALF);
        route.push(Vec3::new(x, 0.0, z)); // Y will be corrected by movement system
    }

    route
}

fn update_threat(threats: &mut Vec<ThreatEntry>, pos: Vec3, strength: f32, game_time: f32) {
    // Merge with nearby existing threat
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

fn pick_attack_target(
    base_pos: Vec3,
    threats: &[ThreatEntry],
    enemy_buildings: &Query<(&Faction, &Transform), With<Building>>,
    teams: &TeamConfig,
    faction: &Faction,
) -> Option<Vec3> {
    // Priority 1: known threat clusters (weakest first)
    let mut valid_threats: Vec<&ThreatEntry> = threats
        .iter()
        .filter(|t| t.estimated_strength > 0.0)
        .collect();
    valid_threats.sort_by(|a, b| a.estimated_strength.partial_cmp(&b.estimated_strength).unwrap());

    if let Some(threat) = valid_threats.first() {
        return Some(threat.position);
    }

    // Priority 2: nearest enemy building
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

fn try_train(
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

fn find_build_pos(
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

fn spawn_ai_building(
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
        commands, cache, kind, pos, registry, building_models, None, height_map, faction,
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
