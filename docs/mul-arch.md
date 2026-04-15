# Multiplayer Architecture

> **Deterministic lockstep** multiplayer with **Matchbox WebRTC** transport (native only).
> Embedded signaling server on the host — no external infrastructure needed for LAN play.
> WebRTC NAT traversal enables internet play without VPN.
> **MessagePack binary wire protocol** over reliable WebRTC data channel. All peers run the full simulation; only player inputs are exchanged.
> FNV-1a checksum-based desync detection every ~1 second.
> Reconnection with 30s grace period.

---

## System Topology

```mermaid
flowchart TB
    subgraph Peer1["PEER 1 (Host)"]
        ECS1["Bevy ECS\n(full simulation)"]
        LS1["lockstep module\n- input buffer\n- tick gate\n- checksum"]
        MBX1["transport module\n- MatchboxSocket\n- PeerMap"]
        SIG["Embedded Signaling\nServer :3536\n(ClientServer topology)"]

        ECS1 <--> LS1
        LS1 <--> MBX1
    end

    subgraph Transport["TRANSPORT LAYER"]
        WEBRTC["WebRTC Data Channel\n(reliable, ordered)"]
        LAN["LAN Discovery\n:7877 broadcast"]
    end

    subgraph Peer2["PEER 2 (Client)"]
        ECS2["Bevy ECS\n(full simulation)"]
        LS2["lockstep module\n- input buffer\n- tick gate\n- checksum"]
        MBX2["transport module\n- MatchboxSocket"]

        ECS2 <--> LS2
        LS2 <--> MBX2
    end

    MBX1 <-->|"InputBroadcast\nChecksumReport"| WEBRTC
    WEBRTC <-->|"InputBroadcast\nChecksumReport"| MBX2

    SIG -.->|"signaling handshake"| WEBRTC

    style Peer1 fill:#1a3a1a,stroke:#4a8a4a,color:#fff
    style Peer2 fill:#1a2a3a,stroke:#4a7a9a,color:#fff
    style Transport fill:#3a2a1a,stroke:#9a7a4a,color:#fff
```

### Module split

All multiplayer modules live under `src/infrastructure/multiplayer/`:

- `lockstep`: `LockstepInputBuffer`, tick gate, input application — the core deterministic sync loop.
- `checksum`: `SyncChecksum`, `DesyncDetected`, FNV-1a world state hashing every 30 ticks.
- `transport` / `matchbox_transport`: Matchbox WebRTC wrapper, peer tracking, LAN discovery.
- `host_systems`: host-side lobby management, input relay to other peers.
- `client_systems`: client-side input capture, connection to host signaling.
- `server/`: host networking (join/leave handling, event broadcast).
- `client/`: client networking (message receive, ping).
- `debug_tap`: network debugging utilities.

The multiplayer menu UI lives under `src/ui/menu/multiplayer/`.

---

## Transport Architecture

```mermaid
flowchart LR
    subgraph MainThread["MAIN THREAD (Bevy)"]
        Poll["poll_matchbox system\n(each frame)"]
        LS["lockstep systems"]
        Poll --> LS
    end

    subgraph MatchboxSocket["MatchboxSocket (Resource)"]
        CH0["Channel 0\n(reliable, ordered)"]
    end

    subgraph Signaling["Embedded Signaling Server"]
        SIG["MatchboxServer\n:3536\nClientServer topology"]
    end

    subgraph Optional["OPTIONAL (LAN)"]
        UDP["UDP LAN Discovery\n:7877 broadcast"]
    end

    Poll -->|"update_peers()\nchannel.receive()"| MatchboxSocket
    LS -->|"channel.send()"| MatchboxSocket
    MatchboxSocket ---|"WebRTC signaling"| SIG

    style MainThread fill:#1a3a1a,stroke:#4a8a4a,color:#fff
    style MatchboxSocket fill:#1a2a3a,stroke:#4a7a9a,color:#fff
    style Signaling fill:#2a1a3a,stroke:#7a4a9a,color:#fff
```

- All I/O is polled from the main Bevy thread via `poll_matchbox`
- Native-only transport (WASM support removed)
- WebRTC NAT traversal via ICE/STUN for internet play
- Single reliable channel — no unreliable channel needed (no high-frequency state sync)
- Codec: MessagePack (`rmp-serde`)

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
    Host->>UI: Show HostLobby (signaling URL)

    Note over UI: CLIENT: JOIN GAME
    UI->>Client: User enters session code
    Client->>Client: Open MatchboxSocket (ws://host:3536/rts_room)
    Client->>Signaling: WebRTC signaling handshake
    Signaling->>Host: Peer connection established
    Signaling->>Client: Peer connection established

    Note over Host,Client: WebRTC data channel open

    Client->>Host: JoinRequest { player_name }
    Host->>Host: Assign seat_index, faction, color via PeerMap
    Host->>Client: Event::JoinAccepted { player_id, seat, faction, color }
    Host-->>Client: Event::LobbyUpdate { players[] }

    Note over UI: HOST clicks START GAME
    Host->>Host: PendingGameStart (next frame)
    Host->>Host: Build SerializableGameConfig
    Host->>Client: Event::GameStart { config_json }
    Host->>Host: Transition to AppState::InGame
    Client->>Client: Deserialize config, Transition to InGame

    Note over Host,Client: === DETERMINISTIC LOCKSTEP ===

    loop Every FixedUpdate tick (30 Hz)
        Host->>Client: InputBroadcast { player_id, tick, commands }
        Client->>Host: InputBroadcast { player_id, tick, commands }
        Note over Host,Client: Both peers wait for all inputs,<br/>then advance SimClock and simulate
    end

    loop Every 30 ticks (~1s)
        Host->>Client: ChecksumReport { tick, checksum }
        Client->>Host: ChecksumReport { tick, checksum }
        Note over Host,Client: Compare checksums — flag desync if mismatch
    end

    loop Every 5s
        Client->>Host: Ping (reliable)
        Host->>Client: Pong (RTT measurement)
    end

    Note over Host,Client: === DISCONNECT ===
    Note over Host: PeerState::Disconnected detected
    Host->>Host: Start 30s reconnect grace period
    Host-->>Client: Announcement "Player disconnected"

    Note over Host,Client: === GRACE PERIOD EXPIRED ===
    Host->>Host: Convert faction to AI permanently
    Host-->>Client: Announcement "AI taking over"
```

---

## Lockstep Synchronization

```mermaid
flowchart TD
    subgraph InputPhase["INPUT PHASE (GameFlowSet::Input)"]
        CAPTURE["Capture local player commands"]
        STAMP["Stamp with tick = SimClock.tick + input_delay"]
        SEND["Broadcast InputBroadcast to all peers"]
        CAPTURE --> STAMP --> SEND
    end

    subgraph ReceivePhase["RECEIVE PHASE (GameFlowSet::NetworkReceive)"]
        RECV["Receive InputBroadcast from peers"]
        BUFFER["Insert into LockstepInputBuffer"]
        CHECK{"All players' inputs<br/>for next tick received?"}
        RECV --> BUFFER --> CHECK
        CHECK -->|Yes| ALLOW["advance_allowed = true"]
        CHECK -->|No| STALL["advance_allowed = false<br/>(FixedUpdate stalls)"]
    end

    subgraph SimPhase["SIMULATION PHASE (FixedUpdate)"]
        GATE{"lockstep_advance_allowed?"}
        GATE -->|Yes| APPLY["Apply all inputs for current tick<br/>(sorted by player_id via BTreeMap)"]
        GATE -->|No| SKIP["Skip — wait for inputs"]
        APPLY --> EXEC["execute_input_command()<br/>for each player's commands"]
        EXEC --> SIM["Run full simulation:<br/>AI → Command → Movement → Combat → Economy → Spatial"]
        SIM --> TICK["SimClock.tick += 1"]
    end

    SEND --> RECV
    ALLOW --> GATE

    style InputPhase fill:#1a3a1a,stroke:#4a8a4a,color:#fff
    style ReceivePhase fill:#1a2a3a,stroke:#4a7a9a,color:#fff
    style SimPhase fill:#2a1a3a,stroke:#7a4a9a,color:#fff
```

### LockstepInputBuffer

```rust
pub struct LockstepInputBuffer {
    pub pending: BTreeMap<u64, BTreeMap<u8, PlayerInput>>,
    pub confirmed_tick: u64,
    pub input_delay: u64,         // default: 3 ticks (~100ms at 30 Hz)
    pub expected_players: u8,
    pub advance_allowed: bool,
    pub last_applied_tick: Option<u64>,
    pub last_local_tick_sent: Option<u64>,
    seq: u32,
}
```

- `BTreeMap` ensures deterministic iteration order (sorted by tick, then by player_id)
- `input_delay` of 3 ticks at 30 Hz = ~100ms — enough to absorb typical LAN jitter
- Simulation stalls if any peer's input is missing for the next tick

### Determinism guarantees

| Concern | Solution |
|---------|----------|
| Frame-rate independence | All simulation in `FixedUpdate` at 30 Hz, using `Time<Fixed>` |
| System ordering | `GameFlowSet` chain: Input → NetworkReceive → Simulation → NetworkBroadcast; `SimSet` chain: Ai → Command → Movement → Combat → Economy → Spatial |
| RNG | `GameRng` resource: `StdRng` seeded from `map_seed`, with `fork(tag)` for subsystems |
| Collection iteration | `BTreeMap` in lockstep buffer and combat slot assignment; spatial queries sorted by Entity |
| Entity spawn order | Deterministic via `SimSet` ordering — same spawn sequence on all peers |
| Tick counter | `SimClock.tick` increments once per `FixedUpdate`, shared by all peers |

---

## Desync Detection

```mermaid
flowchart LR
    subgraph Compute["compute_world_checksum (every 30 ticks)"]
        QUERY["Query all gameplay entities:<br/>Transform, CombatStats, UnitState, Faction"]
        SORT["Sort by (EntityKind, Faction, quantized_pos)"]
        HASH["FNV-1a hash:<br/>positions (x1000 → i32), health,<br/>unit state, faction, resources"]
        STORE["Store SyncChecksum { tick, checksum }"]
        QUERY --> SORT --> HASH --> STORE
    end

    subgraph Exchange["Peer exchange"]
        SEND["Send ChecksumReport { tick, checksum }"]
        RECV["Receive remote ChecksumReport"]
        CMP{"Checksums match?"}
        RECV --> CMP
        CMP -->|Yes| OK["All good"]
        CMP -->|No| DESYNC["Set DesyncDetected resource<br/>Log error + dump state to file"]
    end

    STORE --> SEND
    SEND --> RECV

    style Compute fill:#1a3a1a,stroke:#4a8a4a,color:#fff
    style Exchange fill:#3a1a1a,stroke:#9a4a4a,color:#fff
```

- Positions quantized to 1mm precision (multiply by 1000, cast to i32) before hashing
- Only gameplay state is hashed — visual state (animations, particles, interpolation) is excluded
- On desync: entity state dump written to `desync_dump_tick_{N}.txt` for manual diffing

---

## Message Protocol

### Wire Format

Messages are MessagePack-encoded bytes over a single reliable, ordered WebRTC data channel.

### Client → Host Messages

```mermaid
classDiagram
    class ClientMessage {
        +seq: u32
        +timestamp: f64
    }
    class InputBroadcast {
        +player_id: u8
        +tick: u64
        +commands: Vec~InputCommand~
    }
    class ChecksumReport {
        +tick: u64
        +checksum: u64
    }
    class JoinRequest {
        +player_name: String
    }
    class LeaveNotice
    class Ping
    class Reconnect {
        +session_token: u64
    }
    class Chat {
        +message: String
    }
    class NameUpdate {
        +name: String
    }

    ClientMessage <|-- InputBroadcast
    ClientMessage <|-- ChecksumReport
    ClientMessage <|-- JoinRequest
    ClientMessage <|-- LeaveNotice
    ClientMessage <|-- Ping
    ClientMessage <|-- Reconnect
    ClientMessage <|-- Chat
    ClientMessage <|-- NameUpdate
```

### Host → Client Messages

```mermaid
classDiagram
    class ServerMessage {
        +seq: u32
    }
    class InputBroadcast {
        +player_id: u8
        +tick: u64
        +commands: Vec~InputCommand~
    }
    class ChecksumReport {
        +tick: u64
        +checksum: u64
    }
    class Event {
        +timestamp: f64
        +events: Vec~GameEvent~
    }
    class Pong {
        +timestamp: f64
    }

    ServerMessage <|-- InputBroadcast
    ServerMessage <|-- ChecksumReport
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
        +sender: EntityId
        +message: String
    }
    class Announcement {
        +text: String
    }
    class CountdownStart
    class CountdownCancel
    class GameStart {
        +config_json: String
    }
    class LobbyUpdate {
        +players: Vec~LobbyPlayerInfo~
        +slots: u8[4]
        +player_teams: u8[4]
    }
    class JoinAccepted {
        +player_id: u8
        +seat_index: u8
        +faction_index: u8
        +color_index: u8
        +session_token: u64
    }
    class HostShutdown {
        +reason: String
    }
    class FactionEliminated {
        +faction_index: u8
    }
    class Victory {
        +winner_faction: u8
        +winner_team: Option~u8~
    }

    GameEvent <|-- Chat
    GameEvent <|-- Announcement
    GameEvent <|-- CountdownStart
    GameEvent <|-- CountdownCancel
    GameEvent <|-- GameStart
    GameEvent <|-- LobbyUpdate
    GameEvent <|-- JoinAccepted
    GameEvent <|-- HostShutdown
    GameEvent <|-- FactionEliminated
    GameEvent <|-- Victory
```

---

## Network Bandwidth

| Data | Frequency | Channel | Size |
|------|-----------|---------|------|
| InputBroadcast (per player) | Every tick (30 Hz) | Reliable | ~20-200 bytes (depends on commands) |
| ChecksumReport | Every 30 ticks (~1 Hz) | Reliable | 16 bytes |
| Ping/Pong | Every 5s | Reliable | 8 bytes |
| Event (lobby/chat) | On demand | Reliable | Variable |

Lockstep bandwidth is minimal compared to the old state sync — typically <5 KB/s per peer.

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
        NS["NetStats resource\n- rtt_ms / rtt_smoothed_ms\n- bytes_sent_total / per_sec\n- bytes_recv_total / per_sec\n- msgs_sent_total / per_sec\n- connected_clients"]
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

**Player ID assignment:**
- Host: `player_id = 0`
- Clients: assigned incrementally (1, 2, 3, ...) via `PeerMap` when peers connect

---

## Peer Responsibility Split

| Responsibility | Host | Client |
|---------------|------|--------|
| World simulation (AI, combat, economy) | Full | Full (identical) |
| Entity spawning/despawning | Local (deterministic) | Local (deterministic) |
| Player input | Captures + broadcasts | Captures + broadcasts |
| Input relay to other clients | Relays peer inputs | N/A |
| AI opponents | Runs all AI logic | Runs all AI logic (identical) |
| Lobby management | Accept/reject, assign seats | Display only |
| Signaling server | Runs embedded on :3536 | Connects to host's signaling |
| Desync detection | Computes + exchanges checksums | Computes + exchanges checksums |
| Disconnect handling | Starts 30s grace period | Notified via announcement |

In lockstep, Host and Client are functionally identical for simulation purposes. The "Host" designation only controls lobby management and acts as the relay point for input distribution.
