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
    /// EntityKind discriminant serialized by name (e.g. "Worker", "Base").
    pub kind: String,
    /// Faction name (e.g. "Player1", "Neutral").
    pub faction: String,
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
    /// Lobby player list update — sent when players join/leave.
    #[serde(rename = "lobby_update")]
    LobbyUpdate {
        /// List of (name, faction_index, is_host, connected).
        players: Vec<LobbyPlayerInfo>,
    },
    /// Host acknowledged join and assigned network player/faction identity.
    #[serde(rename = "join_accepted")]
    JoinAccepted {
        player_id: u8,
        seat_index: u8,
        faction_index: u8,
        color_index: u8,
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
        factions: Vec<(String, [u32; 10])>,
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
            | Self::Ping { seq, .. } => *seq,
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
