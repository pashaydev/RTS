# Multiplayer Architecture

> Host-authoritative multiplayer with **Matchbox WebRTC** transport (native + WASM).
> Embedded signaling server on the host — no external infrastructure needed for LAN play.
> WebRTC NAT traversal enables internet play without VPN.
> **MessagePack binary wire protocol** over reliable + unreliable WebRTC data channels. Delta-compressed state sync at ~10Hz with staged client application.
> Reconnection with 30s grace period.

---

## System Topology

```mermaid
flowchart TB
    subgraph Host["HOST (Full Simulation)"]
        ECS["Bevy ECS\n(authoritative world)"]
        SRV["server module\n- input\n- replication"]
        NB["Net Bridge\n- assign_network_ids\n- rebuild_entity_net_map"]
        MBX["transport module\n- MatchboxSocket\n- PeerMap\n- MatchboxInbox"]
        SIG["Embedded Signaling\nServer :3536\n(ClientServer topology)"]

        ECS <--> SRV
        ECS <--> NB
        SRV <--> MBX
    end

    subgraph Transport["TRANSPORT LAYER"]
        WEBRTC["WebRTC Data Channels\n(reliable + unreliable)"]
        HTTP["HTTP File Server\n:7880\n(serves dist/ for browsers)"]
        LAN["LAN Discovery + host helpers\n(still lives in transport.rs)"]
    end

    subgraph Client1["CLIENT (Native)"]
        CECS1["Bevy ECS\n(mirrored state)"]
        CR1["client::receive\n- client_receive_commands\n- client_send_ping"]
        CA1["client::apply\n- apply_world_baseline\n- apply_state_sync\n- apply_entity_sync\n- apply_neutral_sync"]
        CI1["client::interpolation\n- client_interpolate_remote_units"]
        CNS1["ClientNetState\n+ MatchboxSocket"]
        CECS1 <--> CR1
        CECS1 <--> CA1
        CECS1 <--> CI1
        CR1 <--> CNS1
    end

    subgraph Client2["CLIENT (WASM/Browser)"]
        CECS2["Bevy ECS\n(mirrored state)"]
        CS2["Same client module split\nas native"]
        CNS2["ClientNetState\n+ MatchboxSocket"]
        CECS2 <--> CS2
        CS2 <--> CNS2
    end

    MBX -->|ServerMessages| WEBRTC
    WEBRTC -->|ClientMessages| MBX

    WEBRTC ---|"WebRTC P2P\n(via signaling)"| CNS1
    WEBRTC ---|"WebRTC P2P\n(via signaling)"| CNS2

    SIG -.->|"signaling handshake"| WEBRTC

    style Host fill:#1a3a1a,stroke:#4a8a4a,color:#fff
    style Client1 fill:#1a2a3a,stroke:#4a7a9a,color:#fff
    style Client2 fill:#2a1a3a,stroke:#7a4a9a,color:#fff
    style Transport fill:#3a2a1a,stroke:#9a7a4a,color:#fff
```

### Current module split

- `transport`: Matchbox send/receive path, peer tracking, LAN discovery, and HTTP hosting helpers.
- `server::input`: host-side command validation/execution and disconnect handling.
- `server::replication`: host-side snapshot building and broadcast systems.
- `client::receive`: drains inbox and stages server messages into pending resources.
- `client::apply`: mutates ECS from staged baseline/delta data.
- `client::interpolation`: visual smoothing only.

---

## Transport Architecture

```mermaid
flowchart LR
    subgraph MainThread["MAIN THREAD (Bevy)"]
        Poll["poll_matchbox system\n(each frame)"]
        HS["server/client module systems"]
        Poll --> HS
    end

    subgraph MatchboxSocket["MatchboxSocket (Resource)"]
        CH0["Channel 0\n(reliable, ordered)"]
        CH1["Channel 1\n(unreliable, unordered)"]
    end

    subgraph Signaling["Embedded Signaling Server"]
        SIG["MatchboxServer\n:3536\nClientServer topology"]
    end

    subgraph Optional["OPTIONAL (LAN)"]
        HTTP["HTTP File Server\n:7880 serves dist/"]
        UDP["UDP LAN Discovery\n:7877 broadcast"]
    end

    Poll -->|"update_peers()\nchannel.receive()"| MatchboxSocket
    HS -->|"channel.send()"| MatchboxSocket
    MatchboxSocket ---|"WebRTC signaling"| SIG

    style MainThread fill:#1a3a1a,stroke:#4a8a4a,color:#fff
    style MatchboxSocket fill:#1a2a3a,stroke:#4a7a9a,color:#fff
    style Signaling fill:#2a1a3a,stroke:#7a4a9a,color:#fff
```

**Key differences from the old TCP/WS architecture:**
- No background reader/writer threads — all I/O is polled from the main Bevy thread via `poll_matchbox` system
- Unified transport for native and WASM — no `#[cfg(target_arch)]` branching in connection code
- WebRTC NAT traversal via ICE/STUN — internet play without VPN
- Two channels: reliable (commands, events, spawns) and unreliable (high-frequency state sync)
- `transport.rs` still contains legacy LAN discovery / HTTP host helpers, so the file is broader than just Matchbox runtime transport

---

## Connection Lifecycle

```mermaid
sequenceDiagram
    participant UI as Menu UI
    participant Host as Host
    participant Signaling as Signaling Server
    participant Client as Client

    Note over UI: HOST GAME clicked
    UI->>Host: start_hosting()
    Host->>Signaling: Start embedded signaling (:3536)
    Host->>Host: Open MatchboxSocket (ws://127.0.0.1:3536/rts_room)
    Host->>Host: Insert HostNetState, PeerMap, NetRole::Host
    Host->>UI: Show HostLobby (signaling URL + web URL)

    Note over UI: CLIENT: JOIN GAME
    UI->>Client: User enters session code
    Client->>Client: Open MatchboxSocket (ws://host:3536/rts_room)
    Client->>Signaling: WebRTC signaling handshake
    Signaling->>Host: Peer connection established
    Signaling->>Client: Peer connection established

    Note over Host,Client: WebRTC data channels open

    Client->>Host: JoinRequest { player_name }
    Host->>Host: Assign seat_index, faction, color via PeerMap
    Host->>Client: Event::JoinAccepted { player_id, seat, faction, color }
    Host-->>Client: Event::LobbyUpdate { players[] }

    Note over UI: HOST clicks START GAME
    Host->>Host: PendingGameStart (next frame)
    Host->>Host: Build SerializableGameConfig
    Host->>Client: Event::GameStart { config_json }
    Host->>Host: Transition → AppState::InGame
    Client->>Client: Deserialize config, Transition → InGame

    Note over Host,Client: === IN-GAME SYNC LOOP ===

    loop Every 100ms
        Host->>Client: StateSync (unreliable channel)
        Host->>Client: EntitySpawn / EntityDespawn (reliable channel)
    end

    loop Every 500ms
        Host->>Client: WorldBaseline (reliable, periodic neutral-world baseline)
        Host->>Client: BuildingSync (reliable)
        Host->>Client: NeutralWorldDelta (reliable)
    end

    loop Every 1s
        Host->>Client: ResourceSync (reliable)
    end

    loop Every 250ms
        Host->>Client: DayCycleSync (reliable)
    end

    Client->>Host: Input { PlayerInput } (reliable)
    Host->>Host: Validate & execute command
    Host->>Client: RelayedInput (reliable, to all other clients)

    loop Every 5s
        Client->>Host: Ping (reliable)
        Host->>Client: Pong (RTT measurement)
    end

    Note over Host,Client: === DISCONNECT ===
    Note over Host: PeerState::Disconnected detected
    Host->>Host: Start 30s reconnect grace period
    Host-->>Client: Announcement "Player disconnected — waiting for reconnection"

    Note over Host,Client: === GRACE PERIOD EXPIRED ===
    Host->>Host: Convert faction to AI permanently
    Host-->>Client: Announcement "AI taking over"
```

### Client apply pipeline

The in-game client path is now two-stage:

1. `client_receive_commands` drains `MatchboxInbox` and stores incoming data in pending resources.
2. Follow-up apply systems mutate ECS in deterministic order:
   - `client_apply_world_baseline`
   - `client_apply_relayed_inputs`
   - `client_apply_state_sync`
   - `client_apply_building_sync`
   - `client_apply_resource_sync`
   - `client_apply_day_cycle_sync`
   - `client_apply_server_events`
   - `client_apply_entity_sync`
   - `client_apply_neutral_sync`
3. `client_interpolate_remote_units` performs visual smoothing after authoritative state is staged.

---

## Message Protocol

### Wire Format

Messages are sent as MessagePack-encoded bytes directly over WebRTC data channels (no length-prefix framing needed — WebRTC is message-oriented).

- **Channel 0 (reliable, ordered):** Commands, events, entity spawns/despawns, building sync, resource sync, day cycle sync
- **Channel 1 (unreliable, unordered):** High-frequency `StateSync` with entity positions (falls back to reliable if payload > 16KB)
- **Codec:** MessagePack (`rmp-serde`) — ~2-4x smaller than JSON, self-describing binary format

### Client → Server Messages

```mermaid
classDiagram
    class ClientMessage {
        +seq: u64
        +timestamp: f64
    }
    class Input {
        +input: PlayerInput
    }
    class JoinRequest {
        +player_name: String
        +preferred_faction_index: Option~u8~
    }
    class LeaveNotice
    class Ping

    ClientMessage <|-- Input
    ClientMessage <|-- JoinRequest
    ClientMessage <|-- LeaveNotice
    ClientMessage <|-- Ping

    class PlayerInput {
        +player_id: EntityId
        +tick: u64
        +entity_ids: Vec~EntityId~
        +commands: Vec~InputCommand~
    }

    Input --> PlayerInput

    class InputCommand {
        <<enumeration>>
        Move(target: Vec3)
        Attack(target_id: EntityId)
        Gather(target_id: EntityId)
        Patrol(target: Vec3)
        AttackMove(target: Vec3)
        HoldPosition
        SetStance(stance: u8)
        Build / Train / Rally
    }

    PlayerInput --> InputCommand
```

### Server → Client Messages

```mermaid
classDiagram
    class ServerMessage {
        +seq: u64
    }

    class StateSync {
        +entities: Vec~EntitySnapshot~
    }
    class EntitySpawn {
        +spawns: Vec~EntitySpawnData~
    }
    class EntityDespawn {
        +net_ids: Vec~EntityId~
    }
    class BuildingSync {
        +buildings: Vec~BuildingSnapshot~
    }
    class ResourceSync {
        +factions: Vec~(u8, u32[10])~
    }
    class DayCycleSync {
        +cycle: DayCycleSnapshot
    }
    class RelayedInput {
        +player_id: u8
        +input: PlayerInput
    }
    class Event {
        +timestamp: f64
        +events: Vec~GameEvent~
    }
    class NeutralWorldDelta {
        +objects: Vec~NeutralWorldSnapshot~
    }
    class WorldBaseline {
        +terrain: TerrainDescriptor
        +neutral_objects: Vec~NeutralWorldSnapshot~
    }
    class Pong {
        +timestamp: f64
    }

    ServerMessage <|-- StateSync
    ServerMessage <|-- EntitySpawn
    ServerMessage <|-- EntityDespawn
    ServerMessage <|-- BuildingSync
    ServerMessage <|-- ResourceSync
    ServerMessage <|-- DayCycleSync
    ServerMessage <|-- NeutralWorldDelta
    ServerMessage <|-- WorldBaseline
    ServerMessage <|-- RelayedInput
    ServerMessage <|-- Event
    ServerMessage <|-- Pong
```

### Game Events (inside `Event` message)

```mermaid
classDiagram
    class GameEvent {
        <<enumeration>>
    }
    class Chat {
        +sender: String
        +message: String
    }
    class Kill {
        +killer: EntityId
        +victim: EntityId
    }
    class Announcement {
        +text: String
    }
    class GameStart {
        +config_json: String
    }
    class LobbyUpdate {
        +players: Vec~LobbyPlayerInfo~
    }
    class JoinAccepted {
        +player_id: u8
        +seat_index: u8
        +faction_index: u8
        +color_index: u8
    }
    class HostShutdown {
        +reason: String
    }

    GameEvent <|-- Chat
    GameEvent <|-- Kill
    GameEvent <|-- Announcement
    GameEvent <|-- GameStart
    GameEvent <|-- LobbyUpdate
    GameEvent <|-- JoinAccepted
    GameEvent <|-- HostShutdown
```

---

## State Sync Strategy

```mermaid
flowchart TD
    subgraph Host
        TICK["Frame Tick"]
        TICK --> CHECK{"tick % 50 == 0?\n(every ~5s)"}

        CHECK -->|Yes| FULL["FULL RESYNC\nSend ALL entity snapshots\n+ ALL entity spawns"]
        CHECK -->|No| DELTA["DELTA SYNC\nOnly changed entities"]

        DELTA --> COMPARE["Compare vs PreviousSnapshots"]
        COMPARE --> POS["Position Δ > 0.05m?"]
        COMPARE --> ROT["Rotation Δ > 0.02 rad?"]
        COMPARE --> HP["Health changed?"]
        COMPARE --> STATE["UnitState changed?"]

        POS --> SEND["Include in StateSync"]
        ROT --> SEND
        HP --> SEND
        STATE --> SEND
    end

    subgraph Client
        RECV["Receive StateSync"]
        RECV --> OWN{"My faction's\nunits?"}
        OWN -->|"Yes (skip)"| LOCAL["Keep local state\n(client-predicted)"]
        OWN -->|No| DIST{"Distance > 10m?"}
        DIST -->|Yes| SNAP["Teleport (snap)"]
        DIST -->|No| INTERP["Set interpolation target\nblend = 0.0"]

        INTERP --> LERP["client_interpolate_remote_units\nlerp rate = 10.0\n~0.1s to reach target"]
    end

    SEND -->|"100ms timer\n(unreliable channel)"| RECV

    style Host fill:#1a3a1a,stroke:#4a8a4a,color:#fff
    style Client fill:#1a2a3a,stroke:#4a7a9a,color:#fff
```

### Baseline vs delta

- `StateSync`, `EntitySpawn`, `EntityDespawn`, `BuildingSync`, `ResourceSync`, `DayCycleSync`, and `NeutralWorldDelta` remain the main runtime delta path.
- `WorldBaseline` is now actively emitted by the host and applied by clients, but it currently covers:
  - terrain metadata (`TerrainDescriptor`)
  - neutral world objects
- `WorldBaseline` does **not** yet replace entity bootstrap. Faction/unit/building entity bootstrap still depends on `EntitySpawn` plus periodic full resync behavior.
- Practically, the baseline path is now a neutral-world bootstrap/resync path, not a full-world snapshot path.

---

## Entity Replication

```mermaid
flowchart TD
    subgraph HostSide["Host: Entity Lifecycle"]
        SPAWN["Entity spawned in ECS"]
        SPAWN --> MARK["mark_replicated_entities()\nAdd ReplicatedNetEntity marker"]
        MARK --> ASSIGN["assign_network_ids()\nSort by (Kind, Faction, Pos)\nAssign monotonic NetworkId(u32)"]
        ASSIGN --> MAP["rebuild_entity_net_map()\nEntity ↔ NetworkId bidirectional"]
        MAP --> TRACK["SyncedEntitySet\nTrack known set"]
        TRACK --> DIFF{"New entity?"}
        DIFF -->|Yes| BROADCAST["Send EntitySpawn\n{net_id, kind, faction, pos, rot}"]
        DIFF -->|"Removed from ECS"| DESPAWN["Send EntityDespawn\n{net_ids}"]
    end

    subgraph ClientSide["Client: Deterministic Entity Sync"]
        RECEIVE["Receive EntitySpawn"]
        RECEIVE --> KNOWN{"NetworkId\nalready exists?"}
        KNOWN -->|Yes| SKIP["SKIP (already synced)"]
        KNOWN -->|No| CREATE["SPAWN: Create fresh from\nblueprint + NetworkId\n(no distance heuristic)"]

        RECEIVE2["Receive EntityDespawn"]
        RECEIVE2 --> LOOKUP["EntityNetMap lookup"]
        LOOKUP --> REMOVE["despawn() entity"]
    end

    BROADCAST --> RECEIVE
    DESPAWN --> RECEIVE2

    style HostSide fill:#1a3a1a,stroke:#4a8a4a,color:#fff
    style ClientSide fill:#1a2a3a,stroke:#4a7a9a,color:#fff
```

**Replicated entity types:** `EntityKind`, `ResourceNode`, `Sapling`, `GrowingTree`, `GrowingResource`, `MatureTree`, `ExplosiveProp`

---

## Command Flow (Player Input)

```mermaid
sequenceDiagram
    participant CLocal as Client (Local ECS)
    participant CNet as Client (Net)
    participant HNet as Host (Net)
    participant HExec as Host (execute_input_command)
    participant Other as Other Clients

    Note over CLocal: Player right-clicks → Move
    CLocal->>CLocal: Apply command locally (prediction)
    CLocal->>CNet: Queue ClientMessage::Input

    CNet->>HNet: Send Input { PlayerInput } (reliable channel)

    HNet->>HNet: Validate ownership\n(entity faction == sender faction)
    HNet->>HExec: execute_input_command()

    Note over HExec: Set UnitState::Moving\nInsert MoveTarget\nCircular formation for groups

    HExec->>HNet: Command applied on host ECS
    HNet->>Other: RelayedInput { player_id, input } (reliable channel)

    Other->>Other: execute_input_command()\n(same logic as host)
```

---

## Sync Cadence Table

| Data Type | Interval | System | Channel | Delta Compressed |
|-----------|----------|--------|---------|-----------------|
| Entity positions, health, state | 100ms (~10Hz) | `host_broadcast_state_sync` | Unreliable | Yes (Δ pos>0.05, rot>0.02) |
| Entity spawns/despawns | 100ms | `host_broadcast_entity_spawns` | Reliable | Yes (new/removed only) |
| Building state | 500ms | `host_broadcast_building_sync` | Reliable | Yes (level/progress/queue Δ) |
| Neutral-world baseline | first tick, then periodic (~5s via 500ms timer * 10) | `host_broadcast_neutral_world_sync` | Reliable | No (full neutral snapshot) |
| Resource node amounts | 500ms (~2Hz) | `host_broadcast_neutral_world_sync` | Reliable | Yes (amount_remaining Δ) |
| Player resources | 1000ms | `host_broadcast_resource_sync` | Reliable | No (full) |
| Day/night cycle | 250ms | `host_broadcast_day_cycle_sync` | Reliable | No (full) |
| Full resync (all data) | ~5s (tick%50) | Same systems | Both | No (forced full) |
| Ping/Pong (keepalive) | 5s | `client_send_ping` | Reliable | N/A |

---

## Network Statistics (`NetStats`)

```mermaid
flowchart LR
    subgraph MainThread["MAIN THREAD (poll_matchbox)"]
        Poll["poll_matchbox\n+ send helpers"]
    end

    subgraph Atomics["NET_TRAFFIC (LazyLock)"]
        BS["bytes_sent: AtomicU64"]
        BR["bytes_recv: AtomicU64"]
        MS["msgs_sent: AtomicU64"]
        MR["msgs_recv: AtomicU64"]
    end

    subgraph ECS["update_net_stats (each frame)"]
        NS["NetStats resource\n- rtt_ms / rtt_smoothed_ms\n- bytes_sent_total / per_sec\n- bytes_recv_total / per_sec\n- msgs_sent_total / per_sec\n- last_sync_entity_count\n- net_map_size\n- pending_spawns\n- connected_clients"]
    end

    Poll -->|"fetch_add on send"| BS
    Poll -->|"fetch_add on send"| MS
    Poll -->|"fetch_add on receive"| BR
    Poll -->|"fetch_add on receive"| MR
    Atomics -->|"swap(0) drain"| ECS

    style MainThread fill:#1a2a3a,stroke:#4a7a9a,color:#fff
    style Atomics fill:#3a2a1a,stroke:#9a7a4a,color:#fff
    style ECS fill:#1a3a1a,stroke:#4a8a4a,color:#fff
```

**RTT calculation (client only):**
- Send `Ping { timestamp }` every 5s
- Host replies `Pong { timestamp }` (echo back)
- `rtt_ms = now - timestamp`
- `rtt_smoothed = 0.8 * old + 0.2 * new` (exponential moving average)

---

## Lobby & Session Management

```mermaid
stateDiagram-v2
    [*] --> MultiplayerMain: Open Multiplayer Menu

    MultiplayerMain --> HostLobby: HOST GAME
    MultiplayerMain --> JoinLobby: JOIN GAME
    MultiplayerMain --> MainMenu: BACK

    state HostLobby {
        [*] --> Listening
        Listening --> PlayerJoined : peer connected
        PlayerJoined --> Listening : lobby update broadcast
        Listening --> PendingStart : start game clicked
        PendingStart --> ConfigSent : send GameStart event
    }

    state JoinLobby {
        [*] --> InputCode
        InputCode --> Connecting : connect clicked
        Connecting --> Connected : join accepted
        Connecting --> Failed : timeout or error
        Connected --> WaitingForStart : lobby update
        WaitingForStart --> ConfigReceived : game start event
        Failed --> InputCode : retry
    }

    ConfigSent --> InGame : transition to InGame
    ConfigReceived --> InGame : transition to InGame

    InGame --> MainMenu : disconnect or leave
```

**Session code format:** Signaling URL (e.g., `ws://192.168.1.5:3536/rts_room`) or just the host IP (auto-expanded to `ws://IP:3536/rts_room`)

**Web client access:** The host serves the WASM build at `http://<host-ip>:7880` when a `dist/` directory is present. Browser players open that URL, then enter the session code to join.

**Player ID assignment:**
- Host: `player_id = 0`
- Clients: assigned incrementally (1, 2, 3, ...) via `PeerMap` when peers connect

---

## Host/Client Responsibility Split

| Responsibility | Host | Client |
|---------------|------|--------|
| World simulation (physics, AI, combat) | Authoritative | Read-only mirror |
| Entity spawn/despawn | Creates + broadcasts | Receives + spawns locally |
| NetworkId assignment | Assigns (sorted, monotonic) to entities + neutral objects | Receives via EntitySpawn / NeutralWorldDelta |
| Player commands | Validates + executes + relays | Sends input, applies relayed |
| Resource tracking (player totals) | Authoritative | Synced every 1s |
| Resource node amounts (world) | Authoritative | Synced every 500ms (NeutralWorldDelta) |
| Building construction/training | Runs timers + logic | Synced every 500ms |
| Day/night cycle | Runs timer | Synced every 250ms |
| AI opponents | Runs all AI logic | No AI systems (cleared) |
| Lobby management | Accept/reject, assign seats | Display only |
| Signaling server | Runs embedded on :3536 | Connects to host's signaling |

---

## Known Limitations

- **No rollback/prediction:** Client commands are fire-and-forget; no reconciliation if host rejects
- **WorldBaseline is partial:** It is now wired, but only for terrain metadata + neutral world objects; full entity/bootstrap state still depends on `EntitySpawn` plus periodic full resync behavior
- **Max 4 players** (hardcoded faction count)
- **Reconnection is partial:** Grace period and session tokens work host-side, but the client-side reconnect UI flow (auto-retry + `Reconnect` message) is not yet wired
- **No TURN relay:** WebRTC STUN works for most NATs, but symmetric NAT requires a TURN server (not yet configured)

---

## Known Remaining Work

- **TURN relay**: Configure a TURN server for symmetric NAT traversal (currently STUN-only)
- **Message batching**: Wire `PendingServerFrame` to batch all host broadcast systems into a single `ServerFrame` per tick (`ServerFrame` type and `PendingServerFrame` resource exist but aren't used yet)
- **Client prediction**: Prediction buffer + server seq stamping + reconciliation loop (currently fire-and-forget, 1 RTT visual delay)
- **Reconnect UI**: Client-side auto-retry flow (detect disconnect → reconnect with `Reconnect { session_token }`) — host-side grace period + tokens are done
- **Full baseline coverage**: Extend `WorldBaseline` or add a true full-world bootstrap message for entity/unit/building state on late join and reconnect
- **Standalone signaling server**: For production internet play, extract signaling into a deployable binary

---

## Source Files

| File | Purpose |
|------|---------|
| `src/multiplayer/mod.rs` | Plugin wiring, shared resources, run conditions, NetStats, SessionTokens |
| `src/multiplayer/transport.rs` | Matchbox transport re-exports plus LAN discovery (UDP :7877), HTTP file server (:7880), IP detection, and legacy transport helpers |
| `src/multiplayer/server/input.rs` | Server-side input/command handling re-exports |
| `src/multiplayer/server/replication.rs` | Server-side replication/broadcast re-exports |
| `src/multiplayer/host_systems.rs` | Host command execution, snapshot building, delta sync, neutral baseline/delta emission, reconnect grace |
| `src/multiplayer/client/receive.rs` | Client receive/staging re-exports |
| `src/multiplayer/client/apply.rs` | Client apply-system re-exports |
| `src/multiplayer/client/interpolation.rs` | Client interpolation re-exports |
| `src/multiplayer/client_systems.rs` | Staged client receive/apply implementation, interpolation, neutral world apply |
| `src/multiplayer/debug_tap.rs` | HTTP debug server, TX/RX event recording |
| `src/net_bridge.rs` | NetworkId assignment (entities + neutral objects), EntityNetMap |
| `src/menu/multiplayer.rs` | Lobby UI, connection flow (start_hosting, connect_to_host_system, update_lobby_ui), config serialization |
| `game_state/src/message.rs` | All network message types + ServerFrame |
| `game_state/src/codec.rs` | MessagePack encode/decode helpers |
