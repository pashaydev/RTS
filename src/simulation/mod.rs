pub mod abilities;
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
            .add(combat::CombatPlugin)
            .add(combat::CombatIntentsPlugin)
            .add(combat::CombatBudgetPlugin)
            .add(combat::CombatProjectilesPlugin)
            .add(combat::CombatSlotsPlugin)
            .add(abilities::AbilitiesPlugin)
            .add(mobs::MobsPlugin)
            .add(items::ItemsPlugin)
            .add(ai::AiPlugin)
            .add(unit_ai::UnitAiPlugin)
            .add(victory::VictoryPlugin)
            .add(ages::AgesPlugin)
    }
}
