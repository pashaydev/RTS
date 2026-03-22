mod economy;
mod helpers;
mod military;
mod strategy;
mod tactical;
pub mod types;

use bevy::prelude::*;

use crate::components::*;
use types::*;

// ── Plugin ──

pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiState>()
            .init_resource::<AiControlledFactions>()
            .init_resource::<AllyNotifications>()
            .init_resource::<AiFactionSettings>()
            .add_systems(
                Update,
                (strategy::ai_strategy_system, economy::ai_economy_system)
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (military::ai_military_system, tactical::ai_tactical_system)
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(Update, sync_ai_settings.run_if(in_state(AppState::InGame)));
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
