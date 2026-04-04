use bevy::prelude::*;

use crate::components::*;

pub struct CombatBudgetPlugin;

#[derive(Resource, Default, Clone, Debug)]
pub struct CombatBudgetState {
    pub target_rescans_this_frame: usize,
    pub slot_refreshes_this_frame: usize,
    pub repath_requests_this_frame: usize,
    pub blocked_resolutions_this_frame: usize,
}

impl Plugin for CombatBudgetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatBudgetState>()
            .add_systems(
                First,
                reset_combat_budget_state.run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Last,
                publish_combat_budget_stats.run_if(in_state(AppState::InGame)),
            );
    }
}

fn reset_combat_budget_state(mut state: ResMut<CombatBudgetState>) {
    *state = CombatBudgetState::default();
}

fn publish_combat_budget_stats(state: Res<CombatBudgetState>, mut stats: ResMut<CombatStatsDebug>) {
    stats.target_rescans_this_frame = state.target_rescans_this_frame;
    stats.slot_refreshes_this_frame = state.slot_refreshes_this_frame;
    stats.repath_requests_this_frame = state.repath_requests_this_frame;
    stats.blocked_resolutions_this_frame = state.blocked_resolutions_this_frame;
}
