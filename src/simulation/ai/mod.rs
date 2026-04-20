//! `AiPlugin`: orchestrates the enemy AI by aggregating snapshot,
//! strategy, economy, military, and tactical subsystems with dirty flags.

mod economy;
mod helpers;
mod military;
mod strategy;
mod tactical;
pub mod types;

use bevy::prelude::*;
use bevy::time::Fixed;
use std::collections::{HashMap, HashSet};

use crate::blueprints::EntityKind;
use crate::simulation::resources::CarriedTotalsDirty;
use crate::types::*;
use types::*;

// ── Plugin ──

pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiState>()
            .init_resource::<AiWorldSnapshot>()
            .init_resource::<AiFactionDirty>()
            .init_resource::<AiSnapshotIndex>()
            .init_resource::<AiControlledFactions>()
            .init_resource::<AllyNotifications>()
            .init_resource::<AiFactionSettings>()
            .add_systems(
                FixedPreUpdate,
                (collect_ai_snapshot_dirty, build_ai_world_snapshot)
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                strategy::ai_strategy_system
                    .in_set(SimSet::Ai)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                economy::ai_economy_system
                    .in_set(SimSet::Ai)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (military::ai_military_system, tactical::ai_tactical_system)
                    .in_set(SimSet::Ai)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                sync_ai_settings
                    .in_set(GameFlowSet::Diagnostics)
                    .run_if(in_state(AppState::InGame)),
            );
        #[cfg(debug_assertions)]
        app.init_resource::<AiDebugValidationTimer>().add_systems(
            FixedLast,
            validate_ai_snapshot_integrity
                .after(crate::simulation::resources::sync_carried_totals)
                .run_if(in_state(AppState::InGame)),
        );
    }
}

#[derive(Clone, Copy, Default)]
struct AiDirtyFlags {
    units: bool,
    buildings: bool,
    resources: bool,
    military_center: bool,
}

impl AiDirtyFlags {
    fn any(self) -> bool {
        self.units || self.buildings || self.resources || self.military_center
    }

    fn mark_units(&mut self) {
        self.units = true;
        self.military_center = true;
    }

    fn mark_buildings(&mut self) {
        self.buildings = true;
    }
}

#[derive(Resource)]
struct AiFactionDirty {
    per_faction: HashMap<Faction, AiDirtyFlags>,
    resource_nodes: bool,
    force_full: bool,
}

impl Default for AiFactionDirty {
    fn default() -> Self {
        Self {
            per_faction: HashMap::new(),
            resource_nodes: true,
            force_full: true,
        }
    }
}

impl AiFactionDirty {
    fn mark_units(&mut self, faction: Faction) {
        self.per_faction.entry(faction).or_default().mark_units();
    }

    fn mark_buildings(&mut self, faction: Faction) {
        self.per_faction
            .entry(faction)
            .or_default()
            .mark_buildings();
    }
}

#[derive(Resource, Default)]
struct AiSnapshotIndex {
    units: HashMap<Entity, Faction>,
    buildings: HashMap<Entity, Faction>,
    resource_nodes: HashSet<Entity>,
}

#[derive(Clone, Default)]
pub struct FactionWorldSnapshot {
    pub base_position: Option<Vec3>,
    pub unit_count: u32,
    pub worker_count: usize,
    pub military_count: usize,
    pub military_center: Option<Vec3>,
    pub building_counts: HashMap<EntityKind, usize>,
    pub completed_building_counts: HashMap<EntityKind, usize>,
    pub unit_counts: HashMap<EntityKind, usize>,
    pub unit_entities: Vec<Entity>,
    pub worker_entities: Vec<(Entity, Vec3)>,
    pub building_positions: Vec<Vec3>,
    pub military_entities: Vec<(Entity, EntityKind, Vec3)>,
    pub under_construction_count: u8,
}

#[derive(Resource, Default)]
pub struct AiWorldSnapshot {
    pub factions: HashMap<Faction, FactionWorldSnapshot>,
    pub resource_nodes_by_type: HashMap<ResourceType, Vec<(Entity, Vec3)>>,
}

fn collect_ai_snapshot_dirty(
    mut dirty: ResMut<AiFactionDirty>,
    mut index: ResMut<AiSnapshotIndex>,
    changed_units: Query<
        (Entity, &Faction),
        (
            With<Unit>,
            Or<(
                Added<Unit>,
                Changed<Faction>,
                Changed<Transform>,
                Changed<EntityKind>,
            )>,
        ),
    >,
    changed_buildings: Query<
        (Entity, &Faction),
        (
            With<Building>,
            Or<(
                Added<Building>,
                Changed<Faction>,
                Changed<Transform>,
                Changed<EntityKind>,
                Changed<BuildingState>,
            )>,
        ),
    >,
    changed_nodes: Query<
        Entity,
        (
            With<ResourceNode>,
            Or<(
                Added<ResourceNode>,
                Changed<ResourceNode>,
                Changed<Transform>,
            )>,
        ),
    >,
    mut removed_units: RemovedComponents<Unit>,
    mut removed_buildings: RemovedComponents<Building>,
    mut removed_nodes: RemovedComponents<ResourceNode>,
) {
    for (entity, faction) in &changed_units {
        if let Some(previous) = index.units.insert(entity, *faction) {
            if previous != *faction {
                dirty.mark_units(previous);
            }
        }
        dirty.mark_units(*faction);
    }

    for entity in removed_units.read() {
        if let Some(previous) = index.units.remove(&entity) {
            dirty.mark_units(previous);
        }
    }

    for (entity, faction) in &changed_buildings {
        if let Some(previous) = index.buildings.insert(entity, *faction) {
            if previous != *faction {
                dirty.mark_buildings(previous);
            }
        }
        dirty.mark_buildings(*faction);
    }

    for entity in removed_buildings.read() {
        if let Some(previous) = index.buildings.remove(&entity) {
            dirty.mark_buildings(previous);
        }
    }

    for entity in &changed_nodes {
        index.resource_nodes.insert(entity);
        dirty.resource_nodes = true;
    }

    for entity in removed_nodes.read() {
        if index.resource_nodes.remove(&entity) {
            dirty.resource_nodes = true;
        }
    }
}

fn build_ai_world_snapshot(
    mut snapshot: ResMut<AiWorldSnapshot>,
    mut dirty: ResMut<AiFactionDirty>,
    mut index: ResMut<AiSnapshotIndex>,
    units_q: Query<(Entity, &Faction, &EntityKind, &Transform), With<Unit>>,
    buildings_q: Query<(Entity, &Faction, &EntityKind, &Transform, &BuildingState), With<Building>>,
    resource_nodes_q: Query<
        (Entity, &Transform, &ResourceNode, Option<&YardResourceNode>),
        Without<Unit>,
    >,
) {
    if dirty.force_full {
        rebuild_ai_world_snapshot_full(
            &mut snapshot,
            &mut index,
            &units_q,
            &buildings_q,
            &resource_nodes_q,
        );
        dirty.per_faction.clear();
        dirty.resource_nodes = false;
        dirty.force_full = false;
        return;
    }

    let mut dirty_factions: Vec<Faction> = dirty
        .per_faction
        .iter()
        .filter_map(|(faction, flags)| flags.any().then_some(*faction))
        .collect();
    // Sort for deterministic rebuild order across peers.
    dirty_factions.sort();

    if dirty_factions.is_empty() && !dirty.resource_nodes {
        return;
    }

    for faction in dirty_factions {
        rebuild_faction_snapshot(faction, &mut snapshot, &mut index, &units_q, &buildings_q);
        dirty.per_faction.remove(&faction);
    }

    if dirty.resource_nodes {
        rebuild_resource_node_snapshot(&mut snapshot, &mut index, &resource_nodes_q);
        dirty.resource_nodes = false;
    }
}

fn rebuild_ai_world_snapshot_full(
    snapshot: &mut AiWorldSnapshot,
    index: &mut AiSnapshotIndex,
    units_q: &Query<(Entity, &Faction, &EntityKind, &Transform), With<Unit>>,
    buildings_q: &Query<
        (Entity, &Faction, &EntityKind, &Transform, &BuildingState),
        With<Building>,
    >,
    resource_nodes_q: &Query<
        (Entity, &Transform, &ResourceNode, Option<&YardResourceNode>),
        Without<Unit>,
    >,
) {
    snapshot.factions.clear();
    snapshot.resource_nodes_by_type.clear();
    index.units.clear();
    index.buildings.clear();
    index.resource_nodes.clear();

    for (entity, faction, kind, tf, state) in buildings_q.iter() {
        let entry = snapshot.factions.entry(*faction).or_default();
        entry.building_positions.push(tf.translation);
        *entry.building_counts.entry(*kind).or_default() += 1;
        if *state == BuildingState::Complete {
            *entry.completed_building_counts.entry(*kind).or_default() += 1;
        }
        if *state == BuildingState::UnderConstruction {
            entry.under_construction_count = entry.under_construction_count.saturating_add(1);
        }
        if *kind == EntityKind::Base && entry.base_position.is_none() {
            entry.base_position = Some(tf.translation);
        }
        index.buildings.insert(entity, *faction);
    }

    for (entity, faction, kind, tf) in units_q.iter() {
        push_unit_snapshot(entity, faction, kind, tf, snapshot, index);
    }

    for faction_snapshot in snapshot.factions.values_mut() {
        update_military_center(faction_snapshot);
    }

    rebuild_resource_node_snapshot(snapshot, index, resource_nodes_q);
}

fn rebuild_faction_snapshot(
    faction: Faction,
    snapshot: &mut AiWorldSnapshot,
    index: &mut AiSnapshotIndex,
    units_q: &Query<(Entity, &Faction, &EntityKind, &Transform), With<Unit>>,
    buildings_q: &Query<
        (Entity, &Faction, &EntityKind, &Transform, &BuildingState),
        With<Building>,
    >,
) {
    snapshot.factions.remove(&faction);
    index
        .units
        .retain(|_, indexed_faction| *indexed_faction != faction);
    index
        .buildings
        .retain(|_, indexed_faction| *indexed_faction != faction);

    for (entity, building_faction, kind, tf, state) in buildings_q.iter() {
        if *building_faction != faction {
            continue;
        }

        let entry = snapshot.factions.entry(faction).or_default();
        entry.building_positions.push(tf.translation);
        *entry.building_counts.entry(*kind).or_default() += 1;
        if *state == BuildingState::Complete {
            *entry.completed_building_counts.entry(*kind).or_default() += 1;
        }
        if *state == BuildingState::UnderConstruction {
            entry.under_construction_count = entry.under_construction_count.saturating_add(1);
        }
        if *kind == EntityKind::Base && entry.base_position.is_none() {
            entry.base_position = Some(tf.translation);
        }
        index.buildings.insert(entity, faction);
    }

    for (entity, unit_faction, kind, tf) in units_q.iter() {
        if *unit_faction != faction {
            continue;
        }
        push_unit_snapshot(entity, unit_faction, kind, tf, snapshot, index);
    }

    if let Some(faction_snapshot) = snapshot.factions.get_mut(&faction) {
        update_military_center(faction_snapshot);
    }
}

fn rebuild_resource_node_snapshot(
    snapshot: &mut AiWorldSnapshot,
    index: &mut AiSnapshotIndex,
    resource_nodes_q: &Query<
        (Entity, &Transform, &ResourceNode, Option<&YardResourceNode>),
        Without<Unit>,
    >,
) {
    snapshot.resource_nodes_by_type.clear();
    index.resource_nodes.clear();

    for (entity, tf, node, yard_tag) in resource_nodes_q.iter() {
        if yard_tag.is_some() {
            continue;
        }
        index.resource_nodes.insert(entity);
        if node.amount_remaining == 0 {
            continue;
        }
        snapshot
            .resource_nodes_by_type
            .entry(node.resource_type)
            .or_default()
            .push((entity, tf.translation));
    }
}

fn push_unit_snapshot(
    entity: Entity,
    faction: &Faction,
    kind: &EntityKind,
    tf: &Transform,
    snapshot: &mut AiWorldSnapshot,
    index: &mut AiSnapshotIndex,
) {
    let entry = snapshot.factions.entry(*faction).or_default();
    entry.unit_count += 1;
    entry.unit_entities.push(entity);
    *entry.unit_counts.entry(*kind).or_default() += 1;

    if *kind == EntityKind::Worker {
        entry.worker_count += 1;
        entry.worker_entities.push((entity, tf.translation));
    } else {
        entry.military_count += 1;
        entry
            .military_entities
            .push((entity, *kind, tf.translation));
    }

    index.units.insert(entity, *faction);
}

fn update_military_center(faction_snapshot: &mut FactionWorldSnapshot) {
    faction_snapshot.military_center = if faction_snapshot.military_entities.is_empty() {
        None
    } else {
        let sum = faction_snapshot
            .military_entities
            .iter()
            .fold(Vec3::ZERO, |acc, (_, _, pos)| acc + *pos);
        Some(sum / faction_snapshot.military_entities.len() as f32)
    };
}

#[cfg(debug_assertions)]
#[derive(Resource)]
struct AiDebugValidationTimer(Timer);

#[cfg(debug_assertions)]
impl Default for AiDebugValidationTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(5.0, TimerMode::Repeating))
    }
}

#[cfg(debug_assertions)]
fn validate_ai_snapshot_integrity(
    time: Res<Time<Fixed>>,
    mut timer: ResMut<AiDebugValidationTimer>,
    snapshot: Res<AiWorldSnapshot>,
    carried_totals: Res<CarriedResourceTotals>,
    mut carried_totals_dirty: ResMut<CarriedTotalsDirty>,
    workers: Query<(Entity, &Carrying, &Faction, Option<&BuildingAssignment>), With<Unit>>,
    assigned_workers_q: Query<(Entity, &AssignedWorkers), With<Building>>,
    buildings_q: Query<(&Faction, &BuildingState), With<Building>>,
    units_q: Query<(&Faction, &EntityKind), With<Unit>>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    let mut expected_assignments: HashMap<Entity, HashSet<Entity>> = HashMap::new();
    for (worker, _, _, assignment) in &workers {
        if let Some(building) = assignment {
            expected_assignments
                .entry(building.0)
                .or_default()
                .insert(worker);
        }
    }

    for (building, assigned) in &assigned_workers_q {
        let expected = expected_assignments.remove(&building).unwrap_or_default();
        let actual: HashSet<Entity> = assigned.workers.iter().copied().collect();
        if actual != expected {
            warn!("AssignedWorkers mismatch for {:?}", building);
        }
    }
    if !expected_assignments.is_empty() {
        warn!("Worker-side BuildingAssignment has no matching AssignedWorkers entry");
    }

    let mut forced_totals = CarriedResourceTotals::default();
    for (_, carrying, faction, _) in &workers {
        if let Some(rt) = carrying.resource_type {
            if carrying.amount > 0 {
                forced_totals
                    .per_faction
                    .entry(*faction)
                    .or_insert_with(PlayerResources::empty)
                    .amounts[rt as usize] += carrying.amount;
            }
        }
    }
    let factions_to_check: HashSet<Faction> = carried_totals
        .per_faction
        .keys()
        .copied()
        .chain(forced_totals.per_faction.keys().copied())
        .collect();
    for faction in factions_to_check {
        let actual = carried_totals.get(&faction).amounts;
        let expected = forced_totals.get(&faction).amounts;
        if actual != expected {
            warn!(
                "Carried totals drifted from one-shot recompute for {:?}: actual={:?} expected={:?}",
                faction, actual, expected
            );
            carried_totals_dirty.0 = true;
        }
    }

    let mut expected_factions: HashMap<Faction, (u32, usize, usize, u8)> = HashMap::new();
    for (faction, kind) in &units_q {
        let entry = expected_factions.entry(*faction).or_default();
        entry.0 += 1;
        if *kind == EntityKind::Worker {
            entry.1 += 1;
        } else {
            entry.2 += 1;
        }
    }
    for (faction, state) in &buildings_q {
        if *state == BuildingState::UnderConstruction {
            let entry = expected_factions.entry(*faction).or_default();
            entry.3 = entry.3.saturating_add(1);
        }
    }

    for (faction, counts) in expected_factions {
        let snap = snapshot.factions.get(&faction).cloned().unwrap_or_default();
        if snap.unit_count != counts.0 {
            warn!("AI snapshot unit count mismatch for {:?}", faction);
        }
        if snap.worker_count != counts.1 {
            warn!("AI snapshot worker count mismatch for {:?}", faction);
        }
        if snap.military_count != counts.2 {
            warn!("AI snapshot military count mismatch for {:?}", faction);
        }
        if snap.under_construction_count != counts.3 {
            warn!(
                "AI snapshot under-construction count mismatch for {:?}",
                faction
            );
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// Sync AI settings between internal brain state and public resource
// ════════════════════════════════════════════════════════════════════

fn sync_ai_settings(
    config: Res<GameSetupConfig>,
    mut ai_state: ResMut<AiState>,
    mut settings: ResMut<AiFactionSettings>,
    ai_controlled: Res<AiControlledFactions>,
) {
    for &faction in &ai_controlled.factions {
        if !faction_uses_ai(&config, faction) {
            continue;
        }
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
            config.phase_name = brain.top_state.display_name().to_string();
            config.posture_name = format!("{:?}", brain.posture);
            config.attack_squad_size = brain.squad_size(SquadRole::AttackSquad);
            config.defense_squad_size = brain.squad_size(SquadRole::DefenseSquad);
            config.relative_strength = brain.relative_strength;

            config.worker_count = brain
                .squads
                .iter()
                .filter(|s| s.role.is_gather() || s.role == SquadRole::BuildConstruction)
                .map(|s| s.members.len())
                .sum::<usize>()
                .min(255) as u8;
            config.military_count = brain
                .squads
                .iter()
                .filter(|s| {
                    matches!(
                        s.role,
                        SquadRole::DefenseSquad
                            | SquadRole::AttackSquad
                            | SquadRole::Scout
                            | SquadRole::Raider
                    )
                })
                .map(|s| s.members.len())
                .sum::<usize>()
                .min(255) as u8;
        }
    }
}
