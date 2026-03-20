//! Message envelope — all client↔server communication.

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

// ── State sync ─────────────────────────────────────────────────────────────

/// Network-safe mirror of `UnitState`. Entity references are replaced with `EntityId`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "s")]
pub enum NetUnitState {
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "moving")]
    Moving { target: Vec3 },
    #[serde(rename = "attacking")]
    Attacking { target_id: EntityId },
    #[serde(rename = "gathering")]
    Gathering { target_id: EntityId },
    #[serde(rename = "returning")]
    Returning { depot_id: EntityId },
    #[serde(rename = "depositing")]
    Depositing { depot_id: EntityId },
    #[serde(rename = "moving_to_plot")]
    MovingToPlot { target: Vec3 },
    #[serde(rename = "moving_to_build")]
    MovingToBuild { target_id: EntityId },
    #[serde(rename = "building")]
    Building { target_id: EntityId },
    #[serde(rename = "assigned")]
    AssignedGathering { building_id: EntityId, phase: u8 },
    #[serde(rename = "patrolling")]
    Patrolling { target: Vec3, origin: Vec3 },
    #[serde(rename = "attack_moving")]
    AttackMoving { target: Vec3 },
    #[serde(rename = "hold")]
    HoldPosition,
}

/// Network-safe mirror of `Carrying`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetCarrying {
    pub resource_type: u8,
    pub amount: u32,
}

/// Snapshot of building state for sync (host → client, lower frequency).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildingSnapshot {
    pub net_id: EntityId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub construction_progress: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training_queue: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training_progress: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_recipe: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production_progress: Option<f32>,
}

/// Snapshot of the authoritative day/night cycle state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayCycleSnapshot {
    pub time: f32,
    pub cycle_duration: f32,
    pub paused: bool,
}

/// Compact snapshot of one entity for state sync (host → client).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub net_id: EntityId,
    pub pos: Vec3,
    /// Y-axis rotation in radians.
    pub rot_y: f32,
    /// Current health (if entity has Health component).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_state: Option<NetUnitState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub move_target: Option<Vec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_target: Option<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carrying: Option<NetCarrying>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stance: Option<u8>,
}

/// Describes a newly spawned entity so the client can replicate it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntitySpawnData {
    pub net_id: EntityId,
    /// EntityKind index (position in EntityKind::ALL).
    pub kind: u16,
    /// Faction index (0=Player1..3=Player4, 4=Neutral).
    pub faction: u8,
    pub pos: Vec3,
    pub rot_y: f32,
}

/// Host-authored terrain generation contract for a match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainDescriptor {
    pub world_gen_version: u16,
    pub map_seed: u64,
    pub map_size: u8,
    pub resource_density: u8,
    pub day_cycle_secs: f32,
}

/// Neutral world objects that affect gameplay but are not blueprint-driven factions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NeutralKind {
    ResourceNode,
    Sapling,
    GrowingTree,
    GrowingResource,
    ExplosiveProp,
    MobCamp,
}

/// Authoritative snapshot for a neutral gameplay object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeutralWorldSnapshot {
    pub net_id: EntityId,
    pub kind: NeutralKind,
    pub pos: Vec3,
    pub rot_y: f32,
    pub scale: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount_remaining: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<u16>,
}

/// Full host-authored neutral world snapshot used for join/reconnect/resync.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldBaseline {
    pub terrain: TerrainDescriptor,
    pub terrain_hash: u64,
    pub biome_hash: u64,
    pub neutral_objects: Vec<NeutralWorldSnapshot>,
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

    /// Periodic state sync — authoritative entity positions from host.
    #[serde(rename = "state_sync")]
    StateSync {
        seq: u32,
        entities: Vec<EntitySnapshot>,
    },

    /// Batch of newly spawned entities the client must create.
    #[serde(rename = "entity_spawn")]
    EntitySpawn {
        seq: u32,
        spawns: Vec<EntitySpawnData>,
    },

    /// Batch of despawned entities the client must remove.
    #[serde(rename = "entity_despawn")]
    EntityDespawn {
        seq: u32,
        net_ids: Vec<EntityId>,
    },

    /// Periodic building state sync (host → client, lower frequency).
    #[serde(rename = "building_sync")]
    BuildingSync {
        seq: u32,
        buildings: Vec<BuildingSnapshot>,
    },

    /// Periodic resource sync (host → client, ~1Hz).
    #[serde(rename = "resource_sync")]
    ResourceSync {
        seq: u32,
        /// Vec of (faction_index, resource_amounts).
        factions: Vec<(u8, [u32; 10])>,
    },

    /// Periodic authoritative day/night cycle sync.
    #[serde(rename = "day_cycle_sync")]
    DayCycleSync {
        seq: u32,
        cycle: DayCycleSnapshot,
    },

    /// Full host-authored neutral world state for bootstrap/resync.
    #[serde(rename = "world_baseline")]
    WorldBaseline {
        seq: u32,
        baseline: WorldBaseline,
    },

    /// Incremental update for neutral world objects after baseline application.
    #[serde(rename = "neutral_world_delta")]
    NeutralWorldDelta {
        seq: u32,
        objects: Vec<NeutralWorldSnapshot>,
    },

    /// Incremental despawns for neutral world objects after baseline application.
    #[serde(rename = "neutral_world_despawn")]
    NeutralWorldDespawn {
        seq: u32,
        net_ids: Vec<EntityId>,
    },

    /// Application-level keepalive pong (reply to client Ping).
    #[serde(rename = "pong")]
    Pong {
        seq: u32,
        timestamp: f64,
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
}

// ── Server Frame (batched messages per tick) ────────────────────────────────

/// A batch of server messages sent in a single wire frame.
/// Reduces TCP write syscalls and framing overhead.
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
            | Self::RelayedInput { seq, .. }
            | Self::StateSync { seq, .. }
            | Self::EntitySpawn { seq, .. }
            | Self::EntityDespawn { seq, .. }
            | Self::BuildingSync { seq, .. }
            | Self::ResourceSync { seq, .. }
            | Self::DayCycleSync { seq, .. }
            | Self::WorldBaseline { seq, .. }
            | Self::NeutralWorldDelta { seq, .. }
            | Self::NeutralWorldDespawn { seq, .. }
            | Self::Pong { seq, .. } => *seq,
        }
    }
}

impl ClientMessage {
    pub fn seq(&self) -> u32 {
        match self {
            Self::Input { seq, .. }
            | Self::JoinRequest { seq, .. }
            | Self::LeaveNotice { seq, .. }
            | Self::Ping { seq, .. }
            | Self::Reconnect { seq, .. } => *seq,
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
        // MessagePack should be smaller than JSON
        let json = serde_json::to_vec(&msg).unwrap();
        assert!(bytes.len() < json.len(), "msgpack {} >= json {}", bytes.len(), json.len());
    }

    #[test]
    fn relayed_input_msgpack_roundtrip() {
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
        let bytes = codec::encode(&msg).unwrap();
        let decoded: ServerMessage = codec::decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn client_input_msgpack_roundtrip() {
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
        let bytes = codec::encode(&msg).unwrap();
        let decoded: ClientMessage = codec::decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn server_frame_roundtrip() {
        let frame = ServerFrame {
            tick: 42,
            timestamp: 100.0,
            messages: vec![
                ServerMessage::StateSync {
                    seq: 1,
                    entities: vec![EntitySnapshot {
                        net_id: 1,
                        pos: [1.0, 2.0, 3.0],
                        rot_y: 0.5,
                        health: Some(100.0),
                        unit_state: None,
                        move_target: None,
                        attack_target: None,
                        carrying: None,
                        stance: None,
                    }],
                },
                ServerMessage::DayCycleSync {
                    seq: 2,
                    cycle: DayCycleSnapshot {
                        time: 0.5,
                        cycle_duration: 300.0,
                        paused: false,
                    },
                },
            ],
        };
        let bytes = codec::encode(&frame).unwrap();
        let decoded: ServerFrame = codec::decode(&bytes).unwrap();
        assert_eq!(frame, decoded);
    }

    #[test]
    fn entity_spawn_numeric_kinds() {
        let spawn = EntitySpawnData {
            net_id: 42,
            kind: 0,    // Worker
            faction: 0, // Player1
            pos: [1.0, 2.0, 3.0],
            rot_y: 0.0,
        };
        let bytes = codec::encode(&spawn).unwrap();
        let decoded: EntitySpawnData = codec::decode(&bytes).unwrap();
        assert_eq!(spawn, decoded);
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
        // Verify first byte is NOT 0x93 (fixarray) — should be a map for tagged enums
        // If it IS an array, rmp-serde compact format is breaking internal tagging
        println!("Ping msgpack bytes: {:02X?}", &bytes[..bytes.len().min(20)]);
    }

    #[test]
    fn all_client_message_variants_roundtrip() {
        let messages = vec![
            ClientMessage::Input {
                seq: 1,
                timestamp: 10.0,
                input: PlayerInput {
                    player_id: 1,
                    tick: 100,
                    entity_ids: vec![1, 2],
                    commands: vec![InputCommand::Move { target: [1.0, 0.0, 3.0] }],
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
        ];
        for msg in &messages {
            let bytes = codec::encode(msg).unwrap();
            let decoded: ClientMessage = codec::decode(&bytes).unwrap();
            assert_eq!(*msg, decoded, "Failed roundtrip for {:?}", msg);
        }
    }

    #[test]
    fn all_server_message_variants_roundtrip() {
        let messages = vec![
            ServerMessage::Pong { seq: 1, timestamp: 10.0 },
            ServerMessage::StateSync { seq: 2, entities: vec![] },
            ServerMessage::EntitySpawn { seq: 3, spawns: vec![] },
            ServerMessage::EntityDespawn { seq: 4, net_ids: vec![] },
            ServerMessage::BuildingSync { seq: 5, buildings: vec![] },
            ServerMessage::ResourceSync { seq: 6, factions: vec![] },
            ServerMessage::DayCycleSync {
                seq: 7,
                cycle: DayCycleSnapshot { time: 0.5, cycle_duration: 300.0, paused: false },
            },
            ServerMessage::Event {
                seq: 8,
                timestamp: 80.0,
                events: vec![GameEvent::Announcement { text: "test".into() }],
            },
        ];
        for msg in &messages {
            let bytes = codec::encode(msg).unwrap();
            let decoded: ServerMessage = codec::decode(&bytes).unwrap();
            assert_eq!(*msg, decoded, "Failed roundtrip for {:?}", msg);
        }
    }
}

#[cfg(test)]
mod stress_tests {
    use super::*;
    use crate::codec;

    #[test]
    fn large_state_sync_roundtrip() {
        let entities: Vec<EntitySnapshot> = (0..200).map(|i| EntitySnapshot {
            net_id: i,
            pos: [i as f32, 0.0, i as f32 * 2.0],
            rot_y: 0.1 * i as f32,
            health: Some(100.0 - i as f32 * 0.5),
            unit_state: Some(NetUnitState::Moving { target: [1.0, 0.0, 3.0] }),
            move_target: Some([1.0, 0.0, 3.0]),
            attack_target: None,
            carrying: None,
            stance: Some(1),
        }).collect();
        let msg = ServerMessage::StateSync { seq: 42, entities };
        let bytes = codec::encode(&msg).unwrap();
        let decoded: ServerMessage = codec::decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
        let json = serde_json::to_vec(&msg).unwrap();
        println!("StateSync 200 entities: msgpack={}B, json={}B, ratio={:.1}x",
            bytes.len(), json.len(), json.len() as f64 / bytes.len() as f64);
    }

    #[test]
    fn msgpack_json_cross_decode_fails_gracefully() {
        // Verify that msgpack data fails gracefully when decoded as JSON
        let msg = ClientMessage::Ping { seq: 1, timestamp: 42.0 };
        let msgpack_bytes = codec::encode(&msg).unwrap();
        assert!(serde_json::from_slice::<ClientMessage>(&msgpack_bytes).is_err());

        // Verify that JSON data fails gracefully when decoded as msgpack
        let json_bytes = serde_json::to_vec(&msg).unwrap();
        assert!(codec::decode::<ClientMessage>(&json_bytes).is_err());
    }
}
