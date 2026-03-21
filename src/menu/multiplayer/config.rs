use crate::components::*;
use crate::multiplayer::{LobbyPlayer, LobbyState};

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(crate) struct SeatAssignment {
    pub player_id: u8,
    pub seat_index: u8,
    pub faction_index: u8,
    pub color_index: u8,
    pub is_human: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct SerializableGameConfig {
    pub map_seed: u64,
    /// Slot occupants encoded: 0=Human, 1=AiEasy, 2=AiMedium, 3=AiHard, 4=Closed, 5=Open
    pub slots: [u8; 4],
    pub local_player_slot: usize,
    pub team_mode: u8,
    pub player_teams: [u8; 4],
    pub map_size: u8,
    pub resource_density: u8,
    pub day_cycle_secs: f32,
    pub starting_resources_mult: f32,
    pub seat_assignments: Vec<SeatAssignment>,
}

impl SerializableGameConfig {
    pub(crate) fn from_config(config: &GameSetupConfig, lobby: &LobbyState) -> Self {
        let seat_assignments: Vec<SeatAssignment> = lobby
            .players
            .iter()
            .map(|p| {
                let faction_index = Faction::PLAYERS
                    .iter()
                    .position(|f| *f == p.faction)
                    .unwrap_or(0) as u8;
                SeatAssignment {
                    player_id: p.player_id,
                    seat_index: p.seat_index,
                    faction_index,
                    color_index: p.color_index,
                    is_human: p.connected,
                }
            })
            .collect();

        let slots = config.slots.map(|s| match s {
            SlotOccupant::Human => 0,
            SlotOccupant::Ai(AiDifficulty::Easy) => 1,
            SlotOccupant::Ai(AiDifficulty::Medium) => 2,
            SlotOccupant::Ai(AiDifficulty::Hard) => 3,
            SlotOccupant::Closed => 4,
            SlotOccupant::Open => 5,
        });

        Self {
            map_seed: config.map_seed,
            slots,
            local_player_slot: config.local_player_slot,
            team_mode: match config.team_mode {
                TeamMode::FFA => 0,
                TeamMode::Teams => 1,
                TeamMode::Custom => 2,
            },
            player_teams: config.player_teams,
            map_size: match config.map_size {
                MapSize::Small => 0,
                MapSize::Medium => 1,
                MapSize::Large => 2,
            },
            resource_density: match config.resource_density {
                ResourceDensity::Sparse => 0,
                ResourceDensity::Normal => 1,
                ResourceDensity::Dense => 2,
            },
            day_cycle_secs: config.day_cycle_secs,
            starting_resources_mult: config.starting_resources_mult,
            seat_assignments,
        }
    }

    pub(crate) fn apply_to_config(&self, config: &mut GameSetupConfig) {
        config.map_seed = self.map_seed;
        config.slots = self.slots.map(|s| match s {
            0 => SlotOccupant::Human,
            1 => SlotOccupant::Ai(AiDifficulty::Easy),
            2 => SlotOccupant::Ai(AiDifficulty::Medium),
            3 => SlotOccupant::Ai(AiDifficulty::Hard),
            4 => SlotOccupant::Closed,
            _ => SlotOccupant::Open,
        });
        config.local_player_slot = self.local_player_slot;
        config.team_mode = match self.team_mode {
            0 => TeamMode::FFA,
            1 => TeamMode::Teams,
            _ => TeamMode::Custom,
        };
        config.player_teams = self.player_teams;
        config.map_size = match self.map_size {
            0 => MapSize::Small,
            1 => MapSize::Medium,
            _ => MapSize::Large,
        };
        config.resource_density = match self.resource_density {
            0 => ResourceDensity::Sparse,
            1 => ResourceDensity::Normal,
            _ => ResourceDensity::Dense,
        };
        config.day_cycle_secs = self.day_cycle_secs;
        config.starting_resources_mult = self.starting_resources_mult;
    }

    pub(crate) fn apply_to_lobby(&self, lobby: &mut LobbyState) {
        lobby.players.clear();
        for sa in &self.seat_assignments {
            lobby.players.push(LobbyPlayer {
                player_id: sa.player_id,
                name: format!("Player {}", sa.player_id),
                seat_index: sa.seat_index,
                faction: Faction::PLAYERS
                    .get(sa.faction_index as usize)
                    .copied()
                    .unwrap_or(Faction::Neutral),
                color_index: sa.color_index,
                is_host: sa.seat_index == 0,
                connected: sa.is_human,
            });
        }
    }
}
