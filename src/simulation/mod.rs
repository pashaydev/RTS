//! `SimulationPlugins`: deterministic gameplay rules — units, selection,
//! resources, buildings, combat, mobs, items, AI, victory, ages progression.

pub mod ages;
pub mod ai;
pub mod buildings;
pub mod combat;
pub mod items;
pub mod mobs;
pub mod orders;
pub mod resources;
pub mod selection;
pub mod unit_ai;
pub mod units;
pub mod victory;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

pub struct SimulationPlugins;

impl PluginGroup for SimulationPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(units::UnitsPlugin)
            .add(selection::SelectionPlugin)
            .add(resources::ResourcesPlugin)
            .add(buildings::BuildingsPlugin)
            .add(combat::CombatIntentsPlugin)
            .add(combat::CombatProjectilesPlugin)
            .add(combat::AbilityRegistryPlugin)
            .add(combat::CombatBrainPlugin)
            .add(combat::CombatAbilityPlugin)
            .add(combat::CombatBehaviorPlugin)
            .add(combat::CombatApproachPlugin)
            .add(combat::CombatTargetingPlugin)
            .add(combat::CombatRetaliationPlugin)
            .add(combat::CombatLeashPlugin)
            .add(combat::CombatDeathPlugin)
            .add(mobs::MobsPlugin)
            .add(items::ItemsPlugin)
            .add(ai::AiPlugin)
            .add(unit_ai::UnitAiPlugin)
            .add(victory::VictoryPlugin)
            .add(ages::AgesPlugin)
    }
}
