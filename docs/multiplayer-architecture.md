# Multiplayer Architecture

> Host-authoritative LAN multiplayer with TCP (native) and WebSocket (WASM) transport.
> Same-origin hosted-session routing has been added for the deployed web client via a native `session_router`.
> **MessagePack binary wire protocol** with 4-byte length-prefixed framing. Delta-compressed state sync at ~10Hz.
> JSON fallback for legacy clients. Numeric entity/faction discriminants. Reconnection with 30s grace period.

---

## System Topology

```mermaid
flowchart TB
    subgraph Host["HOST (Full Simulation)"]
        ECS["Bevy ECS\n(authoritative world)"]
        HS["Host Systems\n- process_client_commands\n- broadcast_state_sync\n- broadcast_entity_spawns\n- broadcast_building_sync\n- broadcast_resource_sync\n- broadcast_day_cycle_sync\n- broadcast_neutral_world_sync"]
        NB["Net Bridge\n- assign_network_ids\n- rebuild_entity_net_map"]
        HNS["HostNetState\n- incoming_commands\n- client_senders\n- disconnect_rx"]

        ECS <--> HS
        ECS <--> NB
        HS <--> HNS
    end

    subgraph Transport["TRANSPORT LAYER"]
        TCP["TCP Listener\n:7878"]
        WS["WebSocket Listener\n:7879"]
        TCP --- FRAME["4-byte length prefix\n+ MessagePack payload"]
        WS --- FRAME
    end

    subgraph Router["HOSTED SESSION ROUTER (Fly / Native)"]
        SR["session_router\n- POST /api/sessions\n- GET /api/sessions/:code\n- GET /session/:code/ws"]
        FR["Fly replay payload\n(target machine + path rewrite)"]
        SR --> FR
    end

    subgraph Client1["CLIENT (Native)"]
        CECS1["Bevy ECS\n(mirrored state)"]
        CS1["Client Systems\n- client_receive_commands\n- client_apply_entity_sync\n- client_apply_neutral_sync\n- client_interpolate_remote_units\n- client_send_ping"]
        CNS1["ClientNetState\n- incoming\n- outgoing\n- my_faction"]
        CECS1 <--> CS1
        CS1 <--> CNS1
    end

    subgraph Client2["CLIENT (WASM/Browser)"]
        CECS2["Bevy ECS\n(mirrored state)"]
        CS2["Client Systems\n(same as native)"]
        CNS2["ClientNetState\n+ WasmClientSocket"]
        CECS2 <--> CS2
        CS2 <--> CNS2
    end

    HNS -->|ServerMessages| TCP
    HNS -->|ServerMessages| WS
    TCP -->|ClientMessages| HNS
    WS -->|ClientMessages| HNS
    Router -->|same-origin hosted session path| WS

    TCP ---|"TCP stream\n(reader + writer threads)"| CNS1
    WS ---|"WebSocket\n(browser API)"| CNS2

    style Host fill:#1a3a1a,stroke:#4a8a4a,color:#fff
    style Client1 fill:#1a2a3a,stroke:#4a7a9a,color:#fff
    style Client2 fill:#2a1a3a,stroke:#7a4a9a,color:#fff
    style Transport fill:#3a2a1a,stroke:#9a7a4a,color:#fff
```

---

## Thread Architecture (Host)

```mermaid
flowchart LR
    subgraph MainThread["MAIN THREAD (Bevy)"]
        Systems["Host Systems\n(ECS)"]
    end

    subgraph TCPThreads["TCP THREAD POOL"]
        Listener["host_listener_thread\n(accept loop)"]
        R1["client_reader_thread\nPlayer 1"]
        W1["client_writer_thread\nPlayer 1"]
        R2["client_reader_thread\nPlayer 2"]
        W2["client_writer_thread\nPlayer 2"]
    end

    subgraph WSThreads["WEBSOCKET THREAD POOL"]
        WSListener["ws_host_listener_thread\n(accept loop)"]
        WS1["ws_client_handler\nPlayer 100+"]
    end

    Listener -->|"new_client_tx"| Systems
    WSListener -->|"new_ws_clients_tx"| Systems

    R1 -->|"cmd_tx\n(player_id, ClientMsg)"| Systems
    R2 -->|"cmd_tx"| Systems
    WS1 -->|"cmd_tx"| Systems

    Systems -->|"client_senders\n[player_id → tx]"| W1
    Systems -->|"client_senders"| W2
    Systems -->|"ws writer_tx"| WS1

    R1 -.->|"dc_tx (on error)"| Systems
    R2 -.->|"dc_tx"| Systems

    style MainThread fill:#1a3a1a,stroke:#4a8a4a,color:#fff
    style TCPThreads fill:#1a2a3a,stroke:#4a7a9a,color:#fff
    style WSThreads fill:#2a1a3a,stroke:#7a4a9a,color:#fff
```

---

## Connection Lifecycle

```mermaid
sequenceDiagram
    participant UI as Menu UI
    participant Host as Host
    participant Transport as Transport Layer
    participant Client as Client

    Note over UI: HOST GAME clicked
    UI->>Host: start_hosting()
    Host->>Transport: Bind TCP :7878
    Host->>Transport: Bind WS :7879
    Host->>Host: Insert HostNetState, NetRole::Host
    Host->>UI: Show HostLobby (session code)

    Note over UI: CLIENT: JOIN GAME
    UI->>Client: User enters session code
    alt Native LAN/VPN join
        Client->>Transport: TcpStream::connect (or direct WS)
    else Hosted web join
        Client->>Transport: GET /session/:code/ws
        Transport->>Client: Fly replay response
        Client->>Transport: WebSocket upgrade to target machine
    end
    Transport->>Host: new_client_tx (NewClientEvent)
    Host->>Host: Spawn reader + writer threads

    Client->>Host: JoinRequest { player_name }
    Host->>Host: Assign seat_index, faction, color
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
        Host->>Client: StateSync (positions, health, states)
        Host->>Client: EntitySpawn / EntityDespawn
    end

    loop Every 500ms
        Host->>Client: BuildingSync
        Host->>Client: NeutralWorldDelta (resource node amounts)
    end

    loop Every 1s
        Host->>Client: ResourceSync
    end

    loop Every 250ms
        Host->>Client: DayCycleSync
    end

    Client->>Host: Input { PlayerInput (move/attack/...) }
    Host->>Host: Validate & execute command
    Host->>Client: RelayedInput (to all other clients)

    loop Every 5s
        Client->>Host: Ping
        Host->>Client: Pong (RTT measurement)
    end

    Note over Host,Client: === DISCONNECT ===
    Client->>Host: LeaveNotice (or connection drop)
    Host->>Host: Start 30s reconnect grace period
    Host-->>Client: Announcement "Player disconnected — waiting for reconnection"

    Note over Host,Client: === RECONNECTION (within 30s) ===
    Client->>Transport: TcpStream::connect (or WS)
    Client->>Host: Reconnect { session_token }
    Host->>Host: Validate token, restore faction from AI
    Host->>Client: JoinAccepted + full state resync

    Note over Host,Client: === GRACE PERIOD EXPIRED ===
    Host->>Host: Convert faction to AI permanently
    Host-->>Client: Announcement "AI taking over"
```

---

## Message Protocol

### Wire Format

```
┌──────────────────┬──────────────────────────┐
│  4 bytes (BE)    │  N bytes                 │
│  payload length  │  MessagePack payload     │
└──────────────────┴──────────────────────────┘
```

- **Codec**: MessagePack (rmp-serde) — ~2-4x smaller than JSON, self-describing binary format
- **Fallback**: Reader threads try MessagePack first, then JSON for legacy clients
- **WebSocket**: Binary frames (MessagePack), with Text frame fallback (JSON)
- TCP keepalive: 15s interval, 10s timeout
- Read timeout: 2s per recv attempt

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

    SEND -->|"100ms timer"| RECV

    style Host fill:#1a3a1a,stroke:#4a8a4a,color:#fff
    style Client fill:#1a2a3a,stroke:#4a7a9a,color:#fff
```

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

    CNet->>HNet: Send Input { PlayerInput }

    HNet->>HNet: Validate ownership\n(entity faction == sender faction)
    HNet->>HExec: execute_input_command()

    Note over HExec: Set UnitState::Moving\nInsert MoveTarget\nCircular formation for groups

    HExec->>HNet: Command applied on host ECS
    HNet->>Other: RelayedInput { player_id, input }

    Other->>Other: execute_input_command()\n(same logic as host)
```

---

## Sync Cadence Table

| Data Type | Interval | System | Delta Compressed |
|-----------|----------|--------|-----------------|
| Entity positions, health, state | 100ms (~10Hz) | `host_broadcast_state_sync` | Yes (Δ pos>0.05, rot>0.02) |
| Entity spawns/despawns | 100ms | `host_broadcast_entity_spawns` | Yes (new/removed only) |
| Building state | 500ms | `host_broadcast_building_sync` | Yes (level/progress/queue Δ) |
| Resource node amounts | 500ms (~2Hz) | `host_broadcast_neutral_world_sync` | Yes (amount_remaining Δ) |
| Player resources | 1000ms | `host_broadcast_resource_sync` | No (full) |
| Day/night cycle | 250ms | `host_broadcast_day_cycle_sync` | No (full) |
| Full resync (all data) | ~5s (tick%50) | Same systems | No (forced full) |
| Ping/Pong (keepalive) | 5s | `client_send_ping` | N/A |

---

## Network Statistics (`NetStats`)

```mermaid
flowchart LR
    subgraph Threads["I/O Threads"]
        RT["Reader threads"]
        WT["Writer threads"]
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

    RT -->|"fetch_add"| BR
    RT -->|"fetch_add"| MR
    WT -->|"fetch_add"| BS
    WT -->|"fetch_add"| MS
    Atomics -->|"swap(0) drain"| ECS

    style Threads fill:#1a2a3a,stroke:#4a7a9a,color:#fff
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
        Listening --> PlayerJoined: new_client_tx
        PlayerJoined --> Listening: LobbyUpdate broadcast
        Listening --> PendingStart: START GAME clicked
        PendingStart --> ConfigSent: Send GameStart event
    }

    state JoinLobby {
        [*] --> InputCode
        InputCode --> Connecting: CONNECT clicked
        Connecting --> Connected: JoinAccepted
        Connecting --> Failed: timeout/error
        Connected --> WaitingForStart: LobbyUpdate
        WaitingForStart --> ConfigReceived: GameStart event
        Failed --> InputCode: retry
    }

    ConfigSent --> InGame: transition to InGame
    ConfigReceived --> InGame: transition to InGame

    InGame --> MainMenu: Disconnect / Leave
```

**Session code formats:**
- Native LAN/VPN: `IP:PORT` (e.g. `192.168.1.5:7878`)
- Hosted web path: opaque session code resolved on the same origin as `/session/<code>/ws`

**Player ID assignment:**
- Host: `player_id = 0`
- TCP clients: `1, 2, 3, ...`
- WebSocket clients: `100, 101, 102, ...` (avoids collision)

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

---

## Known Limitations

- **No rollback/prediction:** Client commands are fire-and-forget; no reconciliation if host rejects
- **WorldBaseline:** Message type defined but not yet wired (needed for late joiners / reconnect full resync)
- **No NAT traversal:** LAN/VPN only (no STUN/TURN)
- **Max 4 players** (hardcoded faction count)
- **Reconnection is partial:** Grace period and session tokens work host-side, but the client-side reconnect UI flow (auto-retry + `Reconnect` message) is not yet wired
- **Hosted-session routing is partial:** The router and same-origin path exist, but session-host registration and replay-to-live-machine bootstrapping are not wired into the host flow yet

---

## Known Remaining Work

- **Hosted session registration**: Generate opaque codes and register live host machines with `POST /api/sessions`
- **Fly machine targeting**: Populate router registrations with real `app`, `machine_id`, `region`, and target WS path metadata
- **Host bootstrap**: Run a real session host process that exposes the replay target WebSocket endpoint (currently expected to be `/ws`)
- **Message batching**: Wire `PendingServerFrame` to batch all host broadcast systems into a single `ServerFrame` per tick (`ServerFrame` type and `PendingServerFrame` resource exist but aren't used yet)
- **Client prediction**: Prediction buffer + server seq stamping + reconciliation loop (currently fire-and-forget, 1 RTT visual delay)
- **Reconnect UI**: Client-side auto-retry flow (detect disconnect → reconnect with `Reconnect { session_token }`) — host-side grace period + tokens are done
- **WorldBaseline wiring**: Send full entity + neutral world state to newly connected/reconnected clients
- **Optional**: LZ4 compression for large frames, `crossbeam-channel` migration, Bevy observers for connect/disconnect events

---

## Source Files

| File | Purpose | Lines |
|------|---------|-------|
| `src/multiplayer/mod.rs` | Plugin, resources, system sets, NetStats, SessionTokens | ~780 |
| `src/multiplayer/transport.rs` | TCP/WS framing, threads, IP detection, msgpack codec | ~780 |
| `src/multiplayer/host_systems.rs` | Host broadcast, command execution, delta sync, neutral world sync, reconnect grace | ~1590 |
| `src/multiplayer/client_systems.rs` | Client receive, interpolation, deterministic entity sync, neutral world apply | ~900 |
| `src/multiplayer/debug_tap.rs` | HTTP debug server, TX/RX event recording | ~300 |
| `src/multiplayer/ggrs_matchbox.rs` | GGRS rollback scaffolding (unused) | ~50 |
| `src/session_router.rs` | Hosted-session registry, route shape, Fly replay payload model | ~200 |
| `src/bin/session_router.rs` | Native HTTP router serving `dist/` plus hosted-session endpoints | ~240 |
| `src/net_bridge.rs` | NetworkId assignment (entities + neutral objects), EntityNetMap | ~220 |
| `src/menu/multiplayer.rs` | Lobby UI, connection flow, config serialization | ~1250 |
| `game_state/src/message.rs` | All network message types + ServerFrame | ~520 |
| `game_state/src/codec.rs` | MessagePack encode/decode helpers | ~25 |
