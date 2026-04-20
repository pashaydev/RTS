//! Message envelope — lockstep peer communication.

use crate::types::*;
use serde::{Deserialize, Serialize};

fn default_lobby_slots() -> [u8; 4] {
    [0, 2, 2, 2] // Human, AiMedium, AiMedium, AiMedium
}
fn default_lobby_teams() -> [u8; 4] {
    [0, 1, 2, 3] // FFA
}

// ── Input types ─────────────────────────────────────────────────────────────

/// A single player input command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum InputCommand {
    /// Move to a world position.
    #[serde(rename = "move")]
    Move {
        target: Vec3,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        formation: Option<u8>,
    },
    /// Attack an entity.
    #[serde(rename = "attack")]
    Attack { target_id: EntityId },
    /// Use an ability by index.
    #[serde(rename = "ability")]
    UseAbility { ability_id: u8, target: Vec3 },
    /// Scuttle selected units.
    #[serde(rename = "scuttle")]
    Scuttle,
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
    /// Stop — cancel all orders and return to idle.
    #[serde(rename = "stop")]
    Stop,
    /// Drop carried resources and reset hauling state.
    #[serde(rename = "drop_cargo")]
    DropCargo,
    /// Change combat stance.
    #[serde(rename = "stance")]
    SetStance { stance: u8 },
    /// Set or clear worker preferred resource. `255` means "off".
    #[serde(rename = "prefer_resource")]
    SetPreferredResource { resource: u8 },
    /// Toggle the paused state of a production building.
    #[serde(rename = "pause_building")]
    TogglePauseBuilding { building_id: EntityId },
    /// Toggle auto-attack on a defensive tower.
    #[serde(rename = "auto_attack")]
    ToggleAutoAttack { building_id: EntityId },
    /// Place a run of wall grid cells. Cells are in `WallGrid` coordinates.
    #[serde(rename = "build_wall")]
    BuildWall { cells: Vec<[i32; 2]> },
    /// Convert an existing owned wall segment at `cell` into a gatehouse.
    #[serde(rename = "build_gate")]
    BuildGate { cell: [i32; 2] },
    /// Paint a single floor tile at `cell`. Brush mode issues one per input.
    #[serde(rename = "build_floor")]
    BuildFloor { cell: [i32; 2] },
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

/// One-shot lobby / match events that need to be broadcast peer-to-peer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum GameEvent {
    #[serde(rename = "chat")]
    Chat {
        sender: EntityId,
        message: String,
    },
    #[serde(rename = "announce")]
    Announcement {
        text: String,
    },
    /// Host is counting down to game start (3-2-1).
    #[serde(rename = "countdown_start")]
    CountdownStart,
    /// Host cancelled the countdown.
    #[serde(rename = "countdown_cancel")]
    CountdownCancel,
    /// Host tells clients to start the game.
    #[serde(rename = "game_start")]
    GameStart {
        /// JSON-encoded game config so client can set up matching world.
        config_json: String,
    },
    /// Lobby player list update — sent when players join/leave or host changes config.
    #[serde(rename = "lobby_update")]
    LobbyUpdate {
        players: Vec<LobbyPlayerInfo>,
        /// Slot occupants: 0=Human, 1=AiEasy, 2=AiMedium, 3=AiHard, 4=Closed, 5=Open.
        #[serde(default = "default_lobby_slots")]
        slots: [u8; 4],
        /// Team assignment per slot.
        #[serde(default = "default_lobby_teams")]
        player_teams: [u8; 4],
    },
    /// Host acknowledged join and assigned network player/faction identity.
    #[serde(rename = "join_accepted")]
    JoinAccepted {
        player_id: u8,
        seat_index: u8,
        faction_index: u8,
        color_index: u8,
        /// Opaque token for reconnection. Client stores this to rejoin if disconnected.
        #[serde(default)]
        session_token: u64,
    },
    /// Host ended the active match and is returning everyone to menu.
    #[serde(rename = "host_shutdown")]
    HostShutdown {
        reason: String,
    },
}

/// Serializable lobby player info for network transmission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LobbyPlayerInfo {
    pub player_id: u8,
    pub name: String,
    pub seat_index: u8,
    pub faction_index: u8,
    pub color_index: u8,
    pub is_host: bool,
    pub connected: bool,
}

// ── Server → Client ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// One-shot event (chat, kill-feed, announcements, lobby updates).
    #[serde(rename = "event")]
    Event {
        seq: u32,
        timestamp: f64,
        events: Vec<GameEvent>,
    },

    /// Deterministic lockstep: a peer's input scheduled for a specific future
    /// tick. All peers apply this at `input.tick` to stay in sync. Host
    /// forwards `ClientMessage::InputBroadcast` from one client to all other
    /// clients as this variant.
    #[serde(rename = "input_broadcast")]
    InputBroadcast {
        seq: u32,
        timestamp: f64,
        player_id: u8,
        input: PlayerInput,
    },

    /// Application-level keepalive pong (reply to client Ping).
    #[serde(rename = "pong")]
    Pong {
        seq: u32,
        timestamp: f64,
    },

    /// Deterministic lockstep desync detection: a peer's world-state checksum
    /// for a specific sim tick. Originated by the host directly, or forwarded
    /// by the host from a `ClientMessage::ChecksumReport`.
    #[serde(rename = "checksum_report")]
    ChecksumReport {
        seq: u32,
        timestamp: f64,
        player_id: u8,
        tick: u64,
        checksum: u64,
    },
}

// ── Client → Server ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Lockstep: tick-stamped input from this client, forwarded to every
    /// other peer by the host as `ServerMessage::InputBroadcast`.
    #[serde(rename = "input_broadcast")]
    InputBroadcast {
        seq: u32,
        timestamp: f64,
        player_id: u8,
        input: PlayerInput,
    },

    /// Request to join the game session.
    #[serde(rename = "join")]
    JoinRequest {
        seq: u32,
        timestamp: f64,
        player_name: String,
        preferred_faction_index: Option<u8>,
    },

    /// Notify server of graceful disconnect.
    #[serde(rename = "leave")]
    LeaveNotice {
        seq: u32,
        timestamp: f64,
    },

    /// Application-level keepalive ping.
    #[serde(rename = "ping")]
    Ping {
        seq: u32,
        timestamp: f64,
    },

    /// Reconnect to an existing session using a token from a prior JoinAccepted.
    #[serde(rename = "reconnect")]
    Reconnect {
        seq: u32,
        timestamp: f64,
        session_token: u64,
    },

    /// Chat message from client to host (host relays to all).
    #[serde(rename = "chat")]
    Chat {
        seq: u32,
        timestamp: f64,
        message: String,
    },

    /// Player name update from client to host (lobby only).
    #[serde(rename = "name_update")]
    NameUpdate {
        seq: u32,
        timestamp: f64,
        player_name: String,
    },

    /// Deterministic lockstep desync detection: this client's world-state
    /// checksum for a specific sim tick. Host forwards to all other clients
    /// as `ServerMessage::ChecksumReport`.
    #[serde(rename = "checksum_report")]
    ChecksumReport {
        seq: u32,
        timestamp: f64,
        player_id: u8,
        tick: u64,
        checksum: u64,
    },
}

// ── Server Frame (batched messages per tick) ────────────────────────────────

/// A batch of server messages sent in a single wire frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerFrame {
    pub tick: u32,
    pub timestamp: f64,
    pub messages: Vec<ServerMessage>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

impl ServerMessage {
    pub fn seq(&self) -> u32 {
        match self {
            Self::Event { seq, .. }
            | Self::InputBroadcast { seq, .. }
            | Self::Pong { seq, .. }
            | Self::ChecksumReport { seq, .. } => *seq,
        }
    }
}

impl ClientMessage {
    pub fn seq(&self) -> u32 {
        match self {
            Self::InputBroadcast { seq, .. }
            | Self::JoinRequest { seq, .. }
            | Self::LeaveNotice { seq, .. }
            | Self::Ping { seq, .. }
            | Self::Reconnect { seq, .. }
            | Self::Chat { seq, .. }
            | Self::NameUpdate { seq, .. }
            | Self::ChecksumReport { seq, .. } => *seq,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec;

    #[test]
    fn server_message_json_roundtrip() {
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
    fn server_message_msgpack_roundtrip() {
        let msg = ServerMessage::Event {
            seq: 1,
            timestamp: 42.0,
            events: vec![GameEvent::Announcement {
                text: "hello".into(),
            }],
        };
        let bytes = codec::encode(&msg).unwrap();
        let decoded: ServerMessage = codec::decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
        let json = serde_json::to_vec(&msg).unwrap();
        assert!(bytes.len() < json.len(), "msgpack {} >= json {}", bytes.len(), json.len());
    }

    #[test]
    fn input_broadcast_server_roundtrip() {
        let msg = ServerMessage::InputBroadcast {
            seq: 7,
            timestamp: 25.0,
            player_id: 2,
            input: PlayerInput {
                player_id: 2,
                tick: 512,
                entity_ids: vec![4, 5],
                commands: vec![InputCommand::Move {
                    target: [1.0, 0.0, 2.0],
                    formation: None,
                }],
            },
        };
        let bytes = codec::encode(&msg).unwrap();
        let decoded: ServerMessage = codec::decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn input_broadcast_client_roundtrip() {
        let msg = ClientMessage::InputBroadcast {
            seq: 8,
            timestamp: 26.0,
            player_id: 3,
            input: PlayerInput {
                player_id: 3,
                tick: 1024,
                entity_ids: vec![10],
                commands: vec![InputCommand::Stop],
            },
        };
        let bytes = codec::encode(&msg).unwrap();
        let decoded: ClientMessage = codec::decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn client_ping_msgpack_roundtrip() {
        let msg = ClientMessage::Ping {
            seq: 42,
            timestamp: 123.456,
        };
        let bytes = codec::encode(&msg).unwrap();
        let decoded: ClientMessage = codec::decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn all_client_message_variants_roundtrip() {
        let messages = vec![
            ClientMessage::InputBroadcast {
                seq: 1,
                timestamp: 10.0,
                player_id: 2,
                input: PlayerInput {
                    player_id: 2,
                    tick: 100,
                    entity_ids: vec![1, 2],
                    commands: vec![InputCommand::Move {
                        target: [1.0, 0.0, 3.0],
                        formation: Some(2),
                    }],
                },
            },
            ClientMessage::JoinRequest {
                seq: 2,
                timestamp: 20.0,
                player_name: "Test".to_string(),
                preferred_faction_index: Some(0),
            },
            ClientMessage::LeaveNotice {
                seq: 3,
                timestamp: 30.0,
            },
            ClientMessage::Ping {
                seq: 4,
                timestamp: 40.0,
            },
            ClientMessage::Reconnect {
                seq: 5,
                timestamp: 50.0,
                session_token: 12345,
            },
            ClientMessage::NameUpdate {
                seq: 6,
                timestamp: 60.0,
                player_name: "NewName".to_string(),
            },
            ClientMessage::ChecksumReport {
                seq: 7,
                timestamp: 70.0,
                player_id: 2,
                tick: 900,
                checksum: 0xDEAD_BEEF_CAFE_BABE,
            },
        ];
        for msg in &messages {
            let bytes = codec::encode(msg).unwrap();
            let decoded: ClientMessage = codec::decode(&bytes).unwrap();
            assert_eq!(*msg, decoded, "Failed roundtrip for {:?}", msg);
        }
    }

    #[test]
    fn build_wall_gate_floor_roundtrip() {
        let inputs = vec![
            InputCommand::BuildWall {
                cells: vec![[1, 2], [3, 4], [-5, 0]],
            },
            InputCommand::BuildGate { cell: [7, -3] },
            InputCommand::BuildFloor { cell: [0, 0] },
        ];
        for cmd in inputs {
            let wrapped = PlayerInput {
                player_id: 1,
                tick: 10,
                entity_ids: Vec::new(),
                commands: vec![cmd.clone()],
            };
            let bytes = codec::encode(&wrapped).unwrap();
            let decoded: PlayerInput = codec::decode(&bytes).unwrap();
            assert_eq!(decoded.commands.len(), 1);
            assert_eq!(decoded.commands[0], cmd);
        }
    }

    #[test]
    fn all_server_message_variants_roundtrip() {
        let messages = vec![
            ServerMessage::Pong { seq: 1, timestamp: 10.0 },
            ServerMessage::Event {
                seq: 8,
                timestamp: 80.0,
                events: vec![GameEvent::Announcement { text: "test".into() }],
            },
            ServerMessage::ChecksumReport {
                seq: 9,
                timestamp: 90.0,
                player_id: 3,
                tick: 1200,
                checksum: 0x1234_5678_9ABC_DEF0,
            },
        ];
        for msg in &messages {
            let bytes = codec::encode(msg).unwrap();
            let decoded: ServerMessage = codec::decode(&bytes).unwrap();
            assert_eq!(*msg, decoded, "Failed roundtrip for {:?}", msg);
        }
    }
}
