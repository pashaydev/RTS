mod economy;
mod helpers;
mod military;
mod strategy;
mod tactical;
pub mod types;

use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::EntityKind;
use crate::components::*;
use types::*;

// ── Plugin ──

/// Timer that gates how often the AI world snapshot is rebuilt.
/// Matches the fastest AI consumer (TACTICAL_TICK = 0.5s).
#[derive(Resource)]
struct AiSnapshotTimer(Timer);

impl Default for AiSnapshotTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(0.5, TimerMode::Repeating))
    }
}

pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiState>()
            .init_resource::<AiWorldSnapshot>()
            .init_resource::<AiSnapshotTimer>()
            .init_resource::<AiControlledFactions>()
            .init_resource::<AllyNotifications>()
            .init_resource::<AiFactionSettings>()
            .add_systems(
                PreUpdate,
                build_ai_world_snapshot.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                strategy::ai_strategy_system
                    .in_set(GameFlowSet::Simulation)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                economy::ai_economy_system
                    .in_set(GameFlowSet::Simulation)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (military::ai_military_system, tactical::ai_tactical_system)
                    .in_set(GameFlowSet::Simulation)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                sync_ai_settings
                    .in_set(GameFlowSet::Diagnostics)
                    .run_if(in_state(AppState::InGame)),
            );
    }
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

fn build_ai_world_snapshot(
    mut snapshot: ResMut<AiWorldSnapshot>,
    mut timer: ResMut<AiSnapshotTimer>,
    time: Res<Time>,
    units_q: Query<(Entity, &Faction, &EntityKind, &Transform), With<Unit>>,
    buildings_q: Query<(&Faction, &EntityKind, &Transform, &BuildingState), With<Building>>,
    resource_nodes_q: Query<(Entity, &Transform, &ResourceNode), Without<Unit>>,
) {
    timer.0.tick(time.delta());
    if !timer.0.just_finished() {
        return;
    }

    // Retain map capacity by clearing values, not the maps themselves.
    for faction_snap in snapshot.factions.values_mut() {
        *faction_snap = FactionWorldSnapshot::default();
    }
    for nodes in snapshot.resource_nodes_by_type.values_mut() {
        nodes.clear();
    }

    for (faction, kind, tf, state) in &buildings_q {
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
    }

    for (entity, faction, kind, tf) in &units_q {
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
    }

    for (entity, tf, node) in &resource_nodes_q {
        if node.amount_remaining == 0 {
            continue;
        }
        snapshot
            .resource_nodes_by_type
            .entry(node.resource_type)
            .or_default()
            .push((entity, tf.translation));
    }

    for faction_snapshot in snapshot.factions.values_mut() {
        if !faction_snapshot.military_entities.is_empty() {
            let sum = faction_snapshot
                .military_entities
                .iter()
                .fold(Vec3::ZERO, |acc, (_, _, pos)| acc + *pos);
            faction_snapshot.military_center =
                Some(sum / faction_snapshot.military_entities.len() as f32);
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
