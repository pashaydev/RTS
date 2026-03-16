//! Message envelope — all client↔server communication.

use crate::types::*;
use serde::{Deserialize, Serialize};

// ── Input types ─────────────────────────────────────────────────────────────

/// A single player input command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum InputCommand {
    /// Move to a world position.
    #[serde(rename = "move")]
    Move { target: Vec3 },
    /// Attack an entity.
    #[serde(rename = "attack")]
    Attack { target_id: EntityId },
    /// Use an ability by index.
    #[serde(rename = "ability")]
    UseAbility { ability_id: u8, target: Vec3 },
    /// Interact / pick up.
    #[serde(rename = "interact")]
    Interact { target_id: EntityId },
    /// Gather from a resource node.
    #[serde(rename = "gather")]
    Gather { target_id: EntityId },
    /// Place a building at a position.
    #[serde(rename = "build")]
    Build { kind: u16, position: Vec3 },
    /// Train a unit from a building.
    #[serde(rename = "train")]
    Train { building_id: EntityId, kind: u16 },
    /// Set rally point for a building.
    #[serde(rename = "rally")]
    SetRallyPoint { building_id: EntityId, position: Vec3 },
    /// Patrol between current position and target.
    #[serde(rename = "patrol")]
    Patrol { target: Vec3 },
    /// Attack-move toward a position.
    #[serde(rename = "attack_move")]
    AttackMove { target: Vec3 },
    /// Hold position — stop and defend.
    #[serde(rename = "hold")]
    HoldPosition,
    /// Change combat stance.
    #[serde(rename = "stance")]
    SetStance { stance: u8 },
}

/// Player input for a single tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerInput {
    pub player_id: EntityId,
    pub tick: u64,
    /// Selected unit/entity IDs this input applies to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_ids: Vec<EntityId>,
    pub commands: Vec<InputCommand>,
}

// ── Events ──────────────────────────────────────────────────────────────────

/// One-shot server events (not part of world state).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum GameEvent {
    #[serde(rename = "chat")]
    Chat {
        sender: EntityId,
        message: String,
    },
    #[serde(rename = "kill")]
    Kill {
        killer: EntityId,
        victim: EntityId,
    },
    #[serde(rename = "announce")]
    Announcement {
        text: String,
    },
    /// Host tells clients to start the game.
    #[serde(rename = "game_start")]
    GameStart {
        /// JSON-encoded game config so client can set up matching world.
        config_json: String,
    },
    /// Lobby player list update — sent when players join/leave.
    #[serde(rename = "lobby_update")]
    LobbyUpdate {
        /// List of (name, faction_index, is_host, connected).
        players: Vec<LobbyPlayerInfo>,
    },
}

/// Serializable lobby player info for network transmission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LobbyPlayerInfo {
    pub name: String,
    pub faction_index: u8,
    pub is_host: bool,
    pub connected: bool,
}

// ── Server → Client ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// One-shot event (chat, kill-feed, announcements).
    #[serde(rename = "event")]
    Event {
        seq: u32,
        timestamp: f64,
        events: Vec<GameEvent>,
    },

    /// Relayed player input from another client — execute locally.
    #[serde(rename = "relayed_input")]
    RelayedInput {
        seq: u32,
        timestamp: f64,
        player_id: u8,
        input: PlayerInput,
    },
}

// ── Client → Server ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Player commands for a tick.
    #[serde(rename = "input")]
    Input {
        seq: u32,
        timestamp: f64,
        input: PlayerInput,
    },

    /// Request to join the game session.
    #[serde(rename = "join")]
    JoinRequest {
        seq: u32,
        timestamp: f64,
        player_name: String,
    },

    /// Notify server of graceful disconnect.
    #[serde(rename = "leave")]
    LeaveNotice {
        seq: u32,
        timestamp: f64,
    },
}

// ── Helpers ─────────────────────────────────────────────────────────────────

impl ServerMessage {
    pub fn seq(&self) -> u32 {
        match self {
            Self::Event { seq, .. }
            | Self::RelayedInput { seq, .. } => *seq,
        }
    }
}

impl ClientMessage {
    pub fn seq(&self) -> u32 {
        match self {
            Self::Input { seq, .. }
            | Self::JoinRequest { seq, .. }
            | Self::LeaveNotice { seq, .. } => *seq,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_message_tagged_union() {
        let msg = ServerMessage::Event {
            seq: 1,
            timestamp: 42.0,
            events: vec![GameEvent::Announcement {
                text: "hello".into(),
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"event\""));

        let decoded: ServerMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn relayed_input_serialization() {
        let msg = ServerMessage::RelayedInput {
            seq: 10,
            timestamp: 50.0,
            player_id: 2,
            input: PlayerInput {
                player_id: 2,
                tick: 100,
                entity_ids: vec![5, 6],
                commands: vec![InputCommand::Move {
                    target: [1.0, 0.0, 3.0],
                }],
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"relayed_input\""));

        let decoded: ServerMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn client_input_serialization() {
        let msg = ClientMessage::Input {
            seq: 5,
            timestamp: 100.0,
            input: PlayerInput {
                player_id: 1,
                tick: 1040,
                entity_ids: vec![10, 11],
                commands: vec![
                    InputCommand::Move {
                        target: [10.0, 0.0, 5.0],
                    },
                    InputCommand::Attack { target_id: 2 },
                ],
            },
        };
        let json = serde_json::to_string_pretty(&msg).unwrap();
        assert!(json.contains("\"type\": \"input\""));
        assert!(json.contains("\"cmd\": \"move\""));

        let decoded: ClientMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }
}
