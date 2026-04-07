//! Age / Era system — gates buildings behind 3 tech ages (Settlement → Expansion → Conquest).

use bevy::prelude::*;
use std::collections::HashMap;

use crate::blueprints::EntityKind;
use crate::types::BuildingState;
use crate::types::*;
use crate::infrastructure::multiplayer::NetRole;
use crate::ui::event_log_widget::{EventCategory, GameEventLog, LogLevel};

pub struct AgesPlugin;

impl Plugin for AgesPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(FactionAges::default()).add_systems(
            Update,
            age_research_system.run_if(in_state(AppState::InGame)),
        );
    }
}

// ── Age Enum ──

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Age {
    Settlement = 1,
    Expansion = 2,
    Conquest = 3,
}

impl Age {
    pub fn display_name(self) -> &'static str {
        match self {
            Age::Settlement => "Age I: Settlement",
            Age::Expansion => "Age II: Expansion",
            Age::Conquest => "Age III: Conquest",
        }
    }
}

// ── Resources ──

pub struct AgeResearch {
    pub target_age: Age,
    pub timer: Timer,
    pub _building: Entity,
}

#[derive(Resource, Default)]
pub struct FactionAges {
    pub ages: HashMap<Faction, Age>,
    pub researching: HashMap<Faction, AgeResearch>,
}

impl FactionAges {
    pub fn get_age(&self, faction: &Faction) -> Age {
        self.ages.get(faction).copied().unwrap_or(Age::Settlement)
    }
}

// ── Building age requirements ──

/// Returns the minimum age required to construct this building.
pub fn required_age_for_building(kind: EntityKind) -> Age {
    match kind {
        // Age I: Settlement — basic economy and defense
        EntityKind::Base
        | EntityKind::Barracks
        | EntityKind::Sawmill
        | EntityKind::Mine
        | EntityKind::Storage
        | EntityKind::House
        | EntityKind::WatchTower
        | EntityKind::Outpost
        | EntityKind::WallSegment
        | EntityKind::WallPost
        | EntityKind::WallCorner
        | EntityKind::Floor => Age::Settlement,

        // Age II: Expansion — specialized military and processing
        EntityKind::Workshop
        | EntityKind::Stable
        | EntityKind::Smelter
        | EntityKind::GuardTower
        | EntityKind::OilRig
        | EntityKind::Gatehouse
        | EntityKind::Tower => Age::Expansion,

        // Age III: Conquest — elite units, siege, and advanced buildings
        EntityKind::SiegeWorks
        | EntityKind::MageTower
        | EntityKind::Temple
        | EntityKind::Alchemist
        | EntityKind::BallistaTower
        | EntityKind::BombardTower => Age::Conquest,

        // Non-buildings — no age requirement
        _ => Age::Settlement,
    }
}

// ── Systems ──

fn age_research_system(
    time: Res<Time>,
    _net_role: Res<NetRole>,
    mut ages: ResMut<FactionAges>,
    mut event_log: ResMut<GameEventLog>,
    buildings: Query<(&EntityKind, &Faction, &BuildingState), With<Building>>,
) {
    // Tick all active age researches
    let mut completed: Vec<Faction> = Vec::new();
    for (faction, research) in ages.researching.iter_mut() {
        research.timer.tick(time.delta());

        // Check if the researching base still exists and is complete
        let base_valid = buildings.iter().any(|(k, f, s)| {
            *k == EntityKind::Base && *f == *faction && *s == BuildingState::Complete
        });

        if !base_valid {
            // Base destroyed — research cancelled (will be removed below)
            event_log.push_with_level(
                time.elapsed_secs(),
                format!(
                    "{}: Age research cancelled — Base destroyed!",
                    faction.display_name()
                ),
                EventCategory::Alert,
                LogLevel::Warning,
                None,
                Some(*faction),
            );
            completed.push(*faction); // reuse vec for cleanup
            continue;
        }

        if research.timer.is_finished() {
            completed.push(*faction);
        }
    }

    for faction in completed {
        if let Some(research) = ages.researching.remove(&faction) {
            if research.timer.is_finished() {
                // Age advance successful
                ages.ages.insert(faction, research.target_age);
                event_log.push_with_level(
                    time.elapsed_secs(),
                    format!(
                        "{} advanced to {}!",
                        faction.display_name(),
                        research.target_age.display_name()
                    ),
                    EventCategory::Upgrade,
                    LogLevel::Warning,
                    None,
                    Some(faction),
                );
            }
            // If timer not finished, it was cancelled (base destroyed)
        }
    }
}
