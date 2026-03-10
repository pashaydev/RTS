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

const DEFENSE_SQUAD_SIZE: usize = 4;
const UNDER_ATTACK_COOLDOWN: f32 = 15.0;
const ATTACK_MIN_INTERVAL: f32 = 30.0;
const THREAT_DECAY_SECS: f32 = 120.0;
const SCOUT_RADIUS: f32 = 150.0;
const BASE_THREAT_RADIUS: f32 = 50.0;
const MAP_HALF: f32 = 245.0;
const COOPERATION_CHECK_INTERVAL: f32 = 5.0;
const ALLY_SUPPORT_DISTANCE: f32 = 60.0;

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
    Raider,
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

    // Personality & relation
    personality: AiPersonality,
    relation: AiRelation,
    difficulty: AiDifficulty,

    // Cooperation (friendly AI)
    ally_attack_target: Option<Vec3>,
    last_cooperation_check: f32,
    raid_cooldown: f32,

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
    fn new_with_offsets(offset: f32, relation: AiRelation, personality: AiPersonality, difficulty: AiDifficulty) -> Self {
        Self {
            strategy_timer: 0.0,
            economy_timer: offset,
            military_timer: offset * 0.5,
            tactical_timer: 0.0,
            desired_workers: 6,
            relation,
            personality,
            difficulty,
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

    fn max_build_queue(&self) -> usize {
        self.difficulty.max_concurrent_builds()
    }

    fn effective_tick(&self, base_tick: f32) -> f32 {
        base_tick * self.difficulty.tick_multiplier()
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
            .init_resource::<AiControlledFactions>()
            .init_resource::<AllyNotifications>()
            .init_resource::<AiFactionSettings>()
            .add_systems(Update, (ai_strategy_system, ai_economy_system))
            .add_systems(Update, (ai_military_system, ai_tactical_system))
            .add_systems(Update, sync_ai_settings);
    }
}

// ════════════════════════════════════════════════════════════════════
// System 1: Strategy — Phase transitions & build queue planning
// ════════════════════════════════════════════════════════════════════

fn ai_strategy_system(
    time: Res<Time>,
    active_player: Res<ActivePlayer>,
    teams: Res<TeamConfig>,
    ai_controlled: Res<AiControlledFactions>,
    mut ai_state: ResMut<AiState>,
    all_completed: Res<AllCompletedBuildings>,
    buildings_q: Query<(&Faction, &EntityKind, &BuildingState), With<Building>>,
    units_q: Query<(&Faction, &EntityKind), With<Unit>>,
) {
    let dt = time.delta_secs();

    for &faction in &ai_controlled.factions {
        // Skip factions the human is currently playing
        if faction == active_player.0 {
            continue;
        }

        let relation = if teams.is_allied(&faction, &active_player.0) {
            AiRelation::Friendly
        } else {
            AiRelation::Enemy
        };

        let brain = ai_state.factions.entry(faction).or_insert_with(|| {
            let idx = Faction::PLAYERS.iter().position(|f| *f == faction).unwrap_or(0);
            let personality = match relation {
                AiRelation::Friendly => AiPersonality::Supportive,
                AiRelation::Enemy => match idx % 3 {
                    0 => AiPersonality::Balanced,
                    1 => AiPersonality::Aggressive,
                    _ => AiPersonality::Defensive,
                },
            };
            AiFactionBrain::new_with_offsets(idx as f32 * 0.3, relation, personality, AiDifficulty::Medium)
        });

        // Update relation dynamically (teams can change in debug)
        brain.relation = relation;

        brain.strategy_timer -= dt;
        if brain.strategy_timer > 0.0 {
            continue;
        }
        brain.strategy_timer = brain.effective_tick(STRATEGY_TICK);
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

        // Phase transitions — friendly AI transitions slightly faster
        let phase_speed_factor = if brain.relation == AiRelation::Friendly { 0.8 } else { 1.0 };
        match brain.phase {
            StrategyPhase::EarlyGame => {
                let worker_threshold = if brain.relation == AiRelation::Friendly { 5 } else { 6 };
                if worker_count >= worker_threshold
                    && all_completed.has(&faction, EntityKind::Barracks)
                    && all_completed.has(&faction, EntityKind::Storage)
                    && brain.game_time > 120.0 * phase_speed_factor
                {
                    brain.phase = StrategyPhase::MidGame;
                }
            }
            StrategyPhase::MidGame => {
                let mil_threshold = if brain.relation == AiRelation::Friendly { 10 } else { 12 };
                if military_count >= mil_threshold
                    && (all_completed.has(&faction, EntityKind::Workshop)
                        || all_completed.has(&faction, EntityKind::Stable))
                    && brain.game_time > 360.0 * phase_speed_factor
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

        // Set desired workers (adjusted by difficulty)
        let base_workers: i32 = match brain.phase {
            StrategyPhase::EarlyGame => 6,
            StrategyPhase::MidGame => 10,
            StrategyPhase::LateGame => 8,
        };
        brain.desired_workers = (base_workers + brain.difficulty.worker_offset()).max(3) as u8;

        // Build queue — personality-driven
        brain.build_queue.clear();
        let tc = &building_counts;

        match brain.personality {
            AiPersonality::Aggressive => build_queue_aggressive(brain, tc),
            AiPersonality::Defensive => build_queue_defensive(brain, tc),
            AiPersonality::Economic => build_queue_economic(brain, tc),
            AiPersonality::Supportive => {
                // Check what player has to complement
                let mut player_buildings: HashMap<EntityKind, usize> = HashMap::new();
                for (f, kind, state) in buildings_q.iter() {
                    if *f == active_player.0 && *state == BuildingState::Complete {
                        *player_buildings.entry(*kind).or_default() += 1;
                    }
                }
                build_queue_supportive(brain, tc, &player_buildings);
            }
            AiPersonality::Balanced => build_queue_balanced(brain, tc),
        }

        // Sort by priority
        brain.build_queue.sort_by_key(|r| r.priority);

        // Attack readiness — varies by personality and difficulty
        let base_threshold: i32 = match brain.phase {
            StrategyPhase::EarlyGame => 999,
            StrategyPhase::MidGame => match brain.personality {
                AiPersonality::Aggressive => 5,
                AiPersonality::Supportive => 6,
                _ => 8,
            },
            StrategyPhase::LateGame => match brain.personality {
                AiPersonality::Aggressive => 8,
                AiPersonality::Defensive => 15,
                AiPersonality::Supportive => 10,
                _ => 12,
            },
        };
        let attack_threshold = (base_threshold + brain.difficulty.attack_threshold_offset()).max(3) as usize;
        let attack_squad_size = brain.squad_size(SquadRole::AttackSquad);
        brain.attack_ready = attack_squad_size >= attack_threshold;
    }
}

// ── Personality-driven build queues ──

fn build_queue_balanced(brain: &mut AiFactionBrain, tc: &HashMap<EntityKind, usize>) {
    match brain.phase {
        StrategyPhase::EarlyGame => {
            push_if_missing(brain, tc, EntityKind::Barracks, 1, 0);
            push_if_missing(brain, tc, EntityKind::Storage, 1, 1);
            push_if_missing(brain, tc, EntityKind::Sawmill, 1, 2);
            push_if_missing(brain, tc, EntityKind::Tower, 1, 3);
        }
        StrategyPhase::MidGame => {
            push_if_missing(brain, tc, EntityKind::Barracks, 2, 0);
            push_if_missing(brain, tc, EntityKind::Workshop, 1, 1);
            push_if_missing(brain, tc, EntityKind::Stable, 1, 2);
            push_if_missing(brain, tc, EntityKind::Tower, 2, 1);
            push_if_missing(brain, tc, EntityKind::Mine, 1, 3);
            push_if_missing(brain, tc, EntityKind::MageTower, 1, 3);
        }
        StrategyPhase::LateGame => {
            push_if_missing(brain, tc, EntityKind::SiegeWorks, 1, 0);
            push_if_missing(brain, tc, EntityKind::Temple, 1, 1);
            push_if_missing(brain, tc, EntityKind::Tower, 4, 2);
            push_if_missing(brain, tc, EntityKind::OilRig, 1, 2);
            push_if_missing(brain, tc, EntityKind::Storage, 2, 3);
        }
    }
}

fn build_queue_aggressive(brain: &mut AiFactionBrain, tc: &HashMap<EntityKind, usize>) {
    match brain.phase {
        StrategyPhase::EarlyGame => {
            push_if_missing(brain, tc, EntityKind::Barracks, 2, 0); // 2 barracks early
            push_if_missing(brain, tc, EntityKind::Storage, 1, 1);
            push_if_missing(brain, tc, EntityKind::Sawmill, 1, 2);
            // No tower for aggressive
        }
        StrategyPhase::MidGame => {
            push_if_missing(brain, tc, EntityKind::Barracks, 3, 0);
            push_if_missing(brain, tc, EntityKind::Stable, 1, 1);
            push_if_missing(brain, tc, EntityKind::Workshop, 1, 2);
            push_if_missing(brain, tc, EntityKind::Mine, 1, 3);
        }
        StrategyPhase::LateGame => {
            push_if_missing(brain, tc, EntityKind::SiegeWorks, 1, 0);
            push_if_missing(brain, tc, EntityKind::Tower, 2, 2);
            push_if_missing(brain, tc, EntityKind::OilRig, 1, 2);
        }
    }
}

fn build_queue_defensive(brain: &mut AiFactionBrain, tc: &HashMap<EntityKind, usize>) {
    match brain.phase {
        StrategyPhase::EarlyGame => {
            push_if_missing(brain, tc, EntityKind::Tower, 2, 0); // Early towers
            push_if_missing(brain, tc, EntityKind::Barracks, 1, 1);
            push_if_missing(brain, tc, EntityKind::Storage, 1, 1);
            push_if_missing(brain, tc, EntityKind::Sawmill, 1, 2);
        }
        StrategyPhase::MidGame => {
            push_if_missing(brain, tc, EntityKind::Tower, 4, 0);
            push_if_missing(brain, tc, EntityKind::Barracks, 2, 1);
            push_if_missing(brain, tc, EntityKind::Workshop, 1, 2);
            push_if_missing(brain, tc, EntityKind::Mine, 1, 3);
            push_if_missing(brain, tc, EntityKind::MageTower, 1, 3);
        }
        StrategyPhase::LateGame => {
            push_if_missing(brain, tc, EntityKind::Temple, 1, 0);
            push_if_missing(brain, tc, EntityKind::SiegeWorks, 1, 1);
            push_if_missing(brain, tc, EntityKind::Storage, 2, 2);
            push_if_missing(brain, tc, EntityKind::OilRig, 1, 3);
        }
    }
}

fn build_queue_economic(brain: &mut AiFactionBrain, tc: &HashMap<EntityKind, usize>) {
    match brain.phase {
        StrategyPhase::EarlyGame => {
            push_if_missing(brain, tc, EntityKind::Sawmill, 1, 0); // Economy first
            push_if_missing(brain, tc, EntityKind::Mine, 1, 1);
            push_if_missing(brain, tc, EntityKind::Storage, 1, 1);
            push_if_missing(brain, tc, EntityKind::Barracks, 1, 2);
            push_if_missing(brain, tc, EntityKind::Tower, 1, 3);
        }
        StrategyPhase::MidGame => {
            push_if_missing(brain, tc, EntityKind::Storage, 2, 0);
            push_if_missing(brain, tc, EntityKind::Barracks, 2, 1);
            push_if_missing(brain, tc, EntityKind::Workshop, 1, 2);
            push_if_missing(brain, tc, EntityKind::Stable, 1, 2);
            push_if_missing(brain, tc, EntityKind::Tower, 2, 3);
        }
        StrategyPhase::LateGame => {
            push_if_missing(brain, tc, EntityKind::OilRig, 1, 0);
            push_if_missing(brain, tc, EntityKind::SiegeWorks, 1, 1);
            push_if_missing(brain, tc, EntityKind::Temple, 1, 2);
            push_if_missing(brain, tc, EntityKind::Tower, 4, 3);
        }
    }
}

fn build_queue_supportive(brain: &mut AiFactionBrain, tc: &HashMap<EntityKind, usize>, player_buildings: &HashMap<EntityKind, usize>) {
    // Complement what the player has built
    let player_has_barracks = player_buildings.get(&EntityKind::Barracks).copied().unwrap_or(0) >= 2;
    let player_has_workshop = player_buildings.get(&EntityKind::Workshop).copied().unwrap_or(0) > 0;

    match brain.phase {
        StrategyPhase::EarlyGame => {
            push_if_missing(brain, tc, EntityKind::Storage, 1, 0);
            push_if_missing(brain, tc, EntityKind::Barracks, 1, 1);
            push_if_missing(brain, tc, EntityKind::Sawmill, 1, 2);
            push_if_missing(brain, tc, EntityKind::Tower, 1, 3);
        }
        StrategyPhase::MidGame => {
            // If player has barracks, focus on Workshop/Stable instead
            if player_has_barracks {
                push_if_missing(brain, tc, EntityKind::Workshop, 1, 0);
                push_if_missing(brain, tc, EntityKind::Stable, 1, 1);
            } else {
                push_if_missing(brain, tc, EntityKind::Barracks, 2, 0);
            }
            if !player_has_workshop {
                push_if_missing(brain, tc, EntityKind::Workshop, 1, 1);
            }
            push_if_missing(brain, tc, EntityKind::Tower, 2, 2);
            push_if_missing(brain, tc, EntityKind::Mine, 1, 3);
            push_if_missing(brain, tc, EntityKind::MageTower, 1, 3);
        }
        StrategyPhase::LateGame => {
            push_if_missing(brain, tc, EntityKind::Temple, 1, 0);
            push_if_missing(brain, tc, EntityKind::SiegeWorks, 1, 1);
            push_if_missing(brain, tc, EntityKind::Storage, 2, 2);
            push_if_missing(brain, tc, EntityKind::OilRig, 1, 3);
        }
    }
}

fn push_if_missing(brain: &mut AiFactionBrain, tc: &HashMap<EntityKind, usize>, kind: EntityKind, max: usize, priority: u8) {
    if tc.get(&kind).copied().unwrap_or(0) < max {
        brain.build_queue.push(BuildRequest {
            kind,
            priority,
            near_position: None,
        });
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
    ai_controlled: Res<AiControlledFactions>,
    mut ai_state: ResMut<AiState>,
    mut all_resources: ResMut<AllPlayerResources>,
    all_completed: Res<AllCompletedBuildings>,
    carried_totals: Res<CarriedResourceTotals>,
    mut pending_drains: ResMut<PendingCarriedDrains>,
    registry: Res<BlueprintRegistry>,
    cache: Res<EntityVisualCache>,
    building_models: Option<Res<BuildingModelAssets>>,
    height_map: Res<HeightMap>,
    biome_map: Res<BiomeMap>,
    queries: (
        Query<(Entity, &Faction, &Transform, &UnitState), (With<Unit>, With<GatherSpeed>)>,
        Query<(Entity, &Transform, &ResourceNode), Without<Unit>>,
        Query<(Entity, &Faction, &EntityKind, &Transform, &BuildingState), With<Building>>,
        Query<(&Faction, &EntityKind, &BuildingLevel, Entity, &BuildingState), With<Building>>,
        Query<(&Faction, &ConstructionWorkers, &BuildingState), With<Building>>,
        Query<(&Faction, &EntityKind, &mut TrainingQueue), With<Building>>,
        Query<&BuildingFootprint>,
        Query<(Entity, &Faction, &ResourceProcessor, &BuildingState), With<Building>>,
        Query<&AssignedWorkers>,
    ),
) {
    let dt = time.delta_secs();
    let (workers_q, resource_nodes_q, buildings_q, building_levels_q, construction_workers_q, mut train_queues, footprints_q, processor_q, assigned_workers_q) = queries;

    for &faction in &ai_controlled.factions {
        if faction == active_player.0 {
            continue;
        }

        let is_friendly = teams.is_allied(&faction, &active_player.0);

        let brain = match ai_state.factions.get_mut(&faction) {
            Some(b) => b,
            None => continue,
        };

        brain.economy_timer -= dt;
        if brain.economy_timer > 0.0 {
            continue;
        }
        brain.economy_timer = brain.effective_tick(ECONOMY_TICK);

        // Apply resource bonus for Hard difficulty
        if brain.difficulty.resource_bonus() > 0.0 {
            let bonus = brain.difficulty.resource_bonus();
            let res = all_resources.get_mut(&faction);
            // Small trickle bonus each economy tick
            let trickle = (5.0 * bonus) as u32;
            res.wood += trickle;
            res.copper += trickle;
        }

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

        // Get player base pos for friendly AI resource avoidance
        let player_base_pos = if is_friendly {
            buildings_q.iter()
                .find(|(_, f, kind, _, _)| **f == active_player.0 && **kind == EntityKind::Base)
                .map(|(_, _, _, tf, _)| tf.translation)
        } else {
            None
        };

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
            let carried = carried_totals.get(&faction);
            if bp.cost.can_afford_with_carried(all_resources.get(&faction), carried) {
                if try_train(&mut train_queues, &faction, EntityKind::Worker, &registry) {
                    let (dw, dc, di, dg, do_) = bp.cost.deduct_with_carried(all_resources.get_mut(&faction));
                    let drain = SpendFromCarried { faction, wood: dw, copper: dc, iron: di, gold: dg, oil: do_ };
                    if drain.has_deficit() { pending_drains.drains.push(drain); }
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
            if *task == UnitState::Idle && !brain.assigned_units.contains_key(&entity) {
                idle_workers.push((entity, tf.translation));
            }
        }

        for (entity, pos) in &idle_workers {
            let needed = pick_most_needed_resource(&player_res, phase);
            let role = SquadRole::for_resource(needed);

            // Find nearest resource node — friendly AI avoids player's territory
            if let Some(node_entity) = find_nearest_resource_node_with_avoidance(
                *pos,
                needed,
                &resource_nodes_q,
                200.0,
                player_base_pos,
                is_friendly,
            ) {
                commands
                    .entity(*entity)
                    .insert(UnitState::Gathering(node_entity));
                brain.add_to_squad(*entity, role);
            }
        }

        // ── Assign idle workers to processor buildings with open slots ──
        for (proc_entity, proc_faction, processor, proc_state) in processor_q.iter() {
            if *proc_faction != faction || *proc_state != BuildingState::Complete {
                continue;
            }
            if processor.max_workers == 0 {
                continue;
            }
            let current_count = assigned_workers_q.get(proc_entity)
                .map(|aw| aw.workers.len())
                .unwrap_or(0);
            if current_count >= processor.max_workers as usize {
                continue;
            }
            let slots = processor.max_workers as usize - current_count;
            let mut assigned = 0;
            for (w_entity, w_f, _, w_task) in workers_q.iter() {
                if *w_f != faction || *w_task != UnitState::Idle {
                    continue;
                }
                if brain.assigned_units.contains_key(&w_entity) {
                    continue;
                }
                if assigned >= slots {
                    break;
                }
                crate::resources::assign_worker_to_processor(&mut commands, w_entity, proc_entity);
                // Also add to AssignedWorkers on the building
                commands.entity(proc_entity).entry::<AssignedWorkers>().and_modify(move |mut aw| {
                    if !aw.workers.contains(&w_entity) {
                        aw.workers.push(w_entity);
                    }
                }).or_insert(AssignedWorkers { workers: vec![w_entity] });
                brain.add_to_squad(w_entity, SquadRole::GatherCopper); // Generic resource role
                assigned += 1;
            }
        }

        // ── Assign workers to construction ──
        for (entity, f, _kind, tf, state) in buildings_q.iter() {
            if *f != faction || *state != BuildingState::UnderConstruction {
                continue;
            }
            let cw = construction_workers_q
                .get(entity)
                .map(|(_, cw, _)| cw.0)
                .unwrap_or(0);

            if cw < 2 {
                let mut best: Option<(Entity, f32)> = None;
                for (w_entity, w_f, w_tf, w_task) in workers_q.iter() {
                    if *w_f != faction {
                        continue;
                    }
                    if *w_task != UnitState::Idle {
                        continue;
                    }
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
                        .insert(UnitState::MovingToBuild(entity));
                }
            }
        }

        // ── Execute build queue ──
        let pending = brain.pending_builds;
        let max_builds = brain.max_build_queue();
        if (pending as usize) < max_builds {
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

                let carried = carried_totals.get(&faction);
                if !bp.cost.can_afford_with_carried(all_resources.get(&faction), carried) {
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

                let (dw, dc, di, dg, do_) = bp.cost.deduct_with_carried(all_resources.get_mut(&faction));
                let drain = SpendFromCarried { faction, wood: dw, copper: dc, iron: di, gold: dg, oil: do_ };
                if drain.has_deficit() { pending_drains.drains.push(drain); }
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
                    let carried = carried_totals.get(&faction);
                    if start_upgrade(
                        &mut commands,
                        entity,
                        level.0,
                        *kind,
                        &registry,
                        &mut res,
                        faction,
                        carried,
                        &mut pending_drains,
                    ) {
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
    ai_controlled: Res<AiControlledFactions>,
    mut ai_state: ResMut<AiState>,
    mut all_resources: ResMut<AllPlayerResources>,
    carried_totals: Res<CarriedResourceTotals>,
    mut pending_drains: ResMut<PendingCarriedDrains>,
    registry: Res<BlueprintRegistry>,
    mut notifications: ResMut<AllyNotifications>,
    queries: (
        Query<(Entity, &Faction, &EntityKind, &Transform), (With<Unit>, Without<Building>)>,
        Query<(Entity, &Faction, &EntityKind, &Transform, &UnitState), (With<Unit>, Without<AttackTarget>, Without<MoveTarget>, Without<Building>)>,
        Query<&Health>,
        Query<(&Faction, &Transform), With<Building>>,
        Query<(&Faction, &EntityKind, &mut TrainingQueue), With<Building>>,
    ),
) {
    let (units_q, idle_military_q, health_q, enemy_buildings_q, mut train_queues) = queries;
    let dt = time.delta_secs();

    for &faction in &ai_controlled.factions {
        if faction == active_player.0 {
            continue;
        }

        let is_friendly = teams.is_allied(&faction, &active_player.0);

        let brain = match ai_state.factions.get_mut(&faction) {
            Some(b) => b,
            None => continue,
        };

        brain.military_timer -= dt;
        if brain.military_timer > 0.0 {
            continue;
        }
        brain.military_timer = brain.effective_tick(MILITARY_TICK);

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
        let personality = brain.personality;

        // ── Composition-driven training (personality-aware) ──
        let desired_composition: Vec<(EntityKind, usize)> = get_desired_composition(phase, personality, is_friendly, &units_q, &active_player);

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
            let carried = carried_totals.get(&faction);
            if bp.cost.can_afford_with_carried(all_resources.get(&faction), carried) {
                if try_train(&mut train_queues, &faction, unit_kind, &registry) {
                    let (dw, dc, di, dg, do_) = bp.cost.deduct_with_carried(all_resources.get_mut(&faction));
                    let drain = SpendFromCarried { faction, wood: dw, copper: dc, iron: di, gold: dg, oil: do_ };
                    if drain.has_deficit() { pending_drains.drains.push(drain); }
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
            let attack_members: Vec<Entity> = brain
                .get_squad(SquadRole::AttackSquad)
                .map(|s| s.members.clone())
                .unwrap_or_default();

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

        // ── Harassment raids (Aggressive personality, MidGame+, enemy only) ──
        if !is_friendly
            && personality == AiPersonality::Aggressive
            && matches!(phase, StrategyPhase::MidGame | StrategyPhase::LateGame)
        {
            brain.raid_cooldown -= MILITARY_TICK;
            if brain.raid_cooldown <= 0.0 && brain.squad_size(SquadRole::Raider) == 0 {
                // Pick 2-3 fast units for raiding
                let attack_members: Vec<Entity> = brain
                    .get_squad(SquadRole::AttackSquad)
                    .map(|s| s.members.clone())
                    .unwrap_or_default();

                let mut raiders: Vec<Entity> = Vec::new();
                // Prefer cavalry
                for &e in &attack_members {
                    if raiders.len() >= 3 { break; }
                    if units_q.get(e).map_or(false, |(_, _, k, _)| *k == EntityKind::Cavalry) {
                        raiders.push(e);
                    }
                }
                // Fill with any available
                for &e in &attack_members {
                    if raiders.len() >= 2 { break; }
                    if !raiders.contains(&e) {
                        raiders.push(e);
                    }
                }

                if raiders.len() >= 2 {
                    for &e in &raiders {
                        brain.remove_from_squad(e);
                        brain.add_to_squad(e, SquadRole::Raider);
                    }
                    // Send to enemy resource area
                    if let Some(target) = find_enemy_resource_area(&enemy_buildings_q, &teams, &faction) {
                        for &e in &raiders {
                            commands.entity(e).insert(MoveTarget(target));
                        }
                    }
                    brain.raid_cooldown = 30.0;
                }
            }
        }

        // ── Friendly AI: Cooperative behavior ──
        if is_friendly {
            brain.last_cooperation_check -= MILITARY_TICK;
            if brain.last_cooperation_check <= 0.0 {
                brain.last_cooperation_check = COOPERATION_CHECK_INTERVAL;

                // Scan player's military positions
                let mut player_army_center = Vec3::ZERO;
                let mut player_army_count = 0u32;
                let mut player_base = base_pos; // fallback
                for (_, f, kind, tf) in units_q.iter() {
                    if *f == active_player.0 {
                        if *kind != EntityKind::Worker {
                            player_army_center += tf.translation;
                            player_army_count += 1;
                        }
                    }
                }
                // Find player base
                for (f, tf) in enemy_buildings_q.iter() {
                    if *f == active_player.0 {
                        player_base = tf.translation;
                        break;
                    }
                }

                if player_army_count > 0 {
                    player_army_center /= player_army_count as f32;

                    // If player army is pushing (far from their base), support them
                    let dist_from_player_base = player_army_center.distance(player_base);
                    if dist_from_player_base > ALLY_SUPPORT_DISTANCE {
                        brain.ally_attack_target = Some(player_army_center);
                    } else {
                        brain.ally_attack_target = None;
                    }
                }
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
            let target = if is_friendly {
                // Friendly AI: support player's push, or attack known threats
                brain.ally_attack_target.or_else(|| {
                    pick_attack_target(
                        base_pos,
                        &brain.known_threats,
                        &enemy_buildings_q,
                        &teams,
                        &faction,
                    )
                })
            } else {
                pick_attack_target(
                    base_pos,
                    &brain.known_threats,
                    &enemy_buildings_q,
                    &teams,
                    &faction,
                )
            };

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

                // Notify player if ally is attacking
                if is_friendly {
                    notifications.push(
                        AllyNotifyKind::Attacking,
                        "Ally is launching an attack!".to_string(),
                        Some(target_pos),
                        game_time,
                    );
                }
            }
        }

        // Notify when ally is ready to attack
        if is_friendly && attack_ready && (game_time - last_attack_time) > ATTACK_MIN_INTERVAL * 0.8 {
            notifications.push(
                AllyNotifyKind::ReadyToAttack,
                "Ally army ready to push!".to_string(),
                None,
                game_time,
            );
        }

        // ── Retreat behavior: check attack squad avg HP ──
        if !is_friendly && brain.posture == TacticalPosture::Normal {
            let attack_members: Vec<Entity> = brain
                .get_squad(SquadRole::AttackSquad)
                .map(|s| s.members.clone())
                .unwrap_or_default();

            if attack_members.len() >= 3 {
                let mut total_hp_pct = 0.0;
                let mut count = 0u32;
                for &e in &attack_members {
                    if let Ok(h) = health_q.get(e) {
                        total_hp_pct += h.current / h.max;
                        count += 1;
                    }
                }
                if count > 0 {
                    let avg_hp_pct = total_hp_pct / count as f32;
                    if avg_hp_pct < 0.35 {
                        // Pull back
                        brain.posture = TacticalPosture::Retreating;
                        brain.posture_cooldown = 20.0;
                        for &e in &attack_members {
                            commands.entity(e).insert(MoveTarget(base_pos));
                        }
                    }
                }
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
    ai_controlled: Res<AiControlledFactions>,
    mut ai_state: ResMut<AiState>,
    mut notifications: ResMut<AllyNotifications>,
    own_entities_q: Query<
        (Entity, &Faction, &Transform, &Health),
        Or<(With<Unit>, With<Building>)>,
    >,
    enemy_units_q: Query<
        (Entity, &Faction, &Transform, &Health, Option<&AttackDamage>),
        Or<(With<Unit>, With<Mob>)>,
    >,
    buildings_q: Query<(&Faction, &Transform), With<Building>>,
) {
    let dt = time.delta_secs();

    for &faction in &ai_controlled.factions {
        if faction == active_player.0 {
            continue;
        }

        let is_friendly = teams.is_allied(&faction, &active_player.0);

        let brain = match ai_state.factions.get_mut(&faction) {
            Some(b) => b,
            None => continue,
        };

        brain.tactical_timer -= dt;
        if brain.tactical_timer > 0.0 {
            continue;
        }
        brain.tactical_timer = brain.effective_tick(TACTICAL_TICK);

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

        // ── Friendly AI: also detect threats near player's base ──
        let mut player_base_pos = None;
        if is_friendly {
            for (f, tf) in buildings_q.iter() {
                if *f == active_player.0 {
                    player_base_pos = Some(tf.translation);
                    break;
                }
            }

            if let Some(pbp) = player_base_pos {
                for (_, ef, etf, health, damage) in enemy_units_q.iter() {
                    if !teams.is_hostile(&faction, ef) {
                        continue;
                    }
                    let pos = etf.translation;
                    if pos.distance(pbp) < BASE_THREAT_RADIUS * 1.5 {
                        let strength = health.current * damage.map_or(5.0, |d| d.0);
                        update_threat(&mut brain.known_threats, pos, strength, game_time);
                        threats_detected = true;
                        threat_positions.push(pos);

                        // Notify player that ally spotted enemies near their base
                        notifications.push(
                            AllyNotifyKind::EnemySpotted,
                            "Ally spotted enemies near your base!".to_string(),
                            Some(pos),
                            game_time,
                        );
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

            if is_friendly {
                let threat_center = if !threat_positions.is_empty() {
                    let sum: Vec3 = threat_positions.iter().copied().sum();
                    sum / threat_positions.len() as f32
                } else {
                    base_pos
                };
                notifications.push(
                    AllyNotifyKind::UnderAttack,
                    "Ally is under attack!".to_string(),
                    Some(threat_center),
                    game_time,
                );
            }
        }

        // ── Defensive response ──
        if brain.posture == TacticalPosture::UnderAttack {
            let threat_center = if !threat_positions.is_empty() {
                let sum: Vec3 = threat_positions.iter().copied().sum();
                sum / threat_positions.len() as f32
            } else {
                base_pos
            };

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

            // Friendly AI: also defend player's base area
            if is_friendly {
                if let Some(pbp) = player_base_pos {
                    let player_threats: Vec<Vec3> = threat_positions.iter()
                        .filter(|p| p.distance(pbp) < BASE_THREAT_RADIUS * 2.0)
                        .copied()
                        .collect();

                    if !player_threats.is_empty() {
                        let player_threat_center: Vec3 = player_threats.iter().copied().sum::<Vec3>() / player_threats.len() as f32;
                        // Send defense squad to help player
                        if let Some(squad) = brain.get_squad(SquadRole::DefenseSquad) {
                            for &entity in &squad.members {
                                commands.entity(entity).insert(MoveTarget(player_threat_center));
                            }
                        }
                    }
                }
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

fn get_desired_composition(
    phase: StrategyPhase,
    personality: AiPersonality,
    is_friendly: bool,
    units_q: &Query<(Entity, &Faction, &EntityKind, &Transform), (With<Unit>, Without<Building>)>,
    active_player: &ActivePlayer,
) -> Vec<(EntityKind, usize)> {
    // For friendly/supportive AI, check what the player has to complement
    if is_friendly || personality == AiPersonality::Supportive {
        let mut player_melee = 0usize;
        let mut player_ranged = 0usize;
        for (_, f, kind, _) in units_q.iter() {
            if *f != active_player.0 || *kind == EntityKind::Worker { continue; }
            match kind {
                EntityKind::Soldier | EntityKind::Knight | EntityKind::Cavalry => player_melee += 1,
                EntityKind::Archer | EntityKind::Mage => player_ranged += 1,
                _ => {}
            }
        }
        let player_prefers_melee = player_melee > player_ranged;

        return match phase {
            StrategyPhase::EarlyGame => {
                if player_prefers_melee {
                    vec![(EntityKind::Archer, 3), (EntityKind::Soldier, 2)]
                } else {
                    vec![(EntityKind::Soldier, 3), (EntityKind::Archer, 2)]
                }
            }
            StrategyPhase::MidGame => {
                if player_prefers_melee {
                    vec![(EntityKind::Archer, 4), (EntityKind::Mage, 2), (EntityKind::Soldier, 2), (EntityKind::Priest, 1)]
                } else {
                    vec![(EntityKind::Soldier, 3), (EntityKind::Knight, 2), (EntityKind::Archer, 2), (EntityKind::Priest, 1)]
                }
            }
            StrategyPhase::LateGame => {
                if player_prefers_melee {
                    vec![(EntityKind::Archer, 4), (EntityKind::Mage, 3), (EntityKind::Priest, 2), (EntityKind::Soldier, 2), (EntityKind::Catapult, 1)]
                } else {
                    vec![(EntityKind::Knight, 3), (EntityKind::Cavalry, 2), (EntityKind::Soldier, 3), (EntityKind::Priest, 2), (EntityKind::BatteringRam, 1)]
                }
            }
        };
    }

    match personality {
        AiPersonality::Aggressive => match phase {
            StrategyPhase::EarlyGame => vec![(EntityKind::Soldier, 4), (EntityKind::Archer, 1)],
            StrategyPhase::MidGame => vec![(EntityKind::Soldier, 5), (EntityKind::Knight, 3), (EntityKind::Archer, 2)],
            StrategyPhase::LateGame => vec![(EntityKind::Soldier, 4), (EntityKind::Knight, 3), (EntityKind::Cavalry, 3), (EntityKind::Catapult, 2)],
        },
        AiPersonality::Defensive => match phase {
            StrategyPhase::EarlyGame => vec![(EntityKind::Soldier, 2), (EntityKind::Archer, 3)],
            StrategyPhase::MidGame => vec![(EntityKind::Soldier, 3), (EntityKind::Archer, 4), (EntityKind::Mage, 2), (EntityKind::Priest, 1)],
            StrategyPhase::LateGame => vec![(EntityKind::Soldier, 4), (EntityKind::Archer, 4), (EntityKind::Mage, 3), (EntityKind::Priest, 2), (EntityKind::Catapult, 1)],
        },
        AiPersonality::Economic => match phase {
            StrategyPhase::EarlyGame => vec![(EntityKind::Soldier, 2), (EntityKind::Archer, 1)],
            StrategyPhase::MidGame => vec![(EntityKind::Soldier, 4), (EntityKind::Archer, 3), (EntityKind::Knight, 2)],
            StrategyPhase::LateGame => vec![(EntityKind::Soldier, 4), (EntityKind::Knight, 3), (EntityKind::Mage, 3), (EntityKind::Cavalry, 2), (EntityKind::Catapult, 2), (EntityKind::BatteringRam, 1)],
        },
        _ => match phase { // Balanced
            StrategyPhase::EarlyGame => vec![(EntityKind::Soldier, 3), (EntityKind::Archer, 2)],
            StrategyPhase::MidGame => vec![(EntityKind::Soldier, 4), (EntityKind::Archer, 3), (EntityKind::Knight, 2), (EntityKind::Mage, 1)],
            StrategyPhase::LateGame => vec![(EntityKind::Soldier, 3), (EntityKind::Archer, 3), (EntityKind::Knight, 3), (EntityKind::Mage, 2), (EntityKind::Cavalry, 2), (EntityKind::Catapult, 1), (EntityKind::BatteringRam, 1)],
        },
    }
}

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

fn find_nearest_resource_node_with_avoidance(
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

        // Friendly AI: add distance penalty for nodes near player's base
        if is_friendly {
            if let Some(pbp) = player_base {
                let dist_to_player = tf.translation.distance(pbp);
                if dist_to_player < 40.0 {
                    d += 80.0; // Strong penalty to avoid stealing player's resources
                }
            }
        }

        if best.is_none() || d < best.unwrap().1 {
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

fn compute_scout_route(base_pos: Vec3) -> Vec<Vec3> {
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

fn update_threat(threats: &mut Vec<ThreatEntry>, pos: Vec3, strength: f32, game_time: f32) {
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

fn find_enemy_resource_area(
    buildings: &Query<(&Faction, &Transform), With<Building>>,
    teams: &TeamConfig,
    faction: &Faction,
) -> Option<Vec3> {
    // Find nearest enemy building and offset to their resource area
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
    // Offset slightly from base toward center to target resource gathering areas
    best.map(|(pos, _)| {
        let to_center = (Vec3::ZERO - pos).normalize_or_zero();
        pos + to_center * 30.0
    })
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

// ════════════════════════════════════════════════════════════════════
// Sync AI settings between internal brain state and public resource
// ════════════════════════════════════════════════════════════════════

fn sync_ai_settings(
    mut ai_state: ResMut<AiState>,
    mut settings: ResMut<AiFactionSettings>,
    ai_controlled: Res<AiControlledFactions>,
) {
    for &faction in &ai_controlled.factions {
        // Read settings from public resource (set by debug panel)
        if let Some(config) = settings.settings.get(&faction) {
            if let Some(brain) = ai_state.factions.get_mut(&faction) {
                brain.difficulty = config.difficulty;
                brain.personality = config.personality;
            }
        }

        // Write brain state back to public resource
        if let Some(brain) = ai_state.factions.get(&faction) {
            let config = settings.settings.entry(faction).or_default();
            config.difficulty = brain.difficulty;
            config.personality = brain.personality;
            config.relation = brain.relation;
            config.phase_name = format!("{:?}", brain.phase);
            config.posture_name = format!("{:?}", brain.posture);
            config.attack_squad_size = brain.squad_size(SquadRole::AttackSquad);
            config.defense_squad_size = brain.squad_size(SquadRole::DefenseSquad);
        }
    }
}
