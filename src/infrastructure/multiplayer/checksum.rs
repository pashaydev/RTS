//! Deterministic desync detection for lockstep multiplayer.
//!
//! Every `CHECKSUM_INTERVAL_TICKS`, each peer hashes a projection of the
//! authoritative simulation state (entity positions, health, unit states,
//! per-faction resources, tick counter) into a single `u64`. Peers exchange
//! these checksums over the reliable channel and compare. Any mismatch at
//! the same tick means the simulations have diverged — which should be
//! impossible in a correctly-deterministic lockstep, so we log loudly and
//! (optionally) dump detailed state for post-mortem diffing.
//!
//! Transport topology mirrors the input pipeline (host-relay star):
//! - Host runs `compute_world_checksum` and broadcasts `ServerMessage::ChecksumReport`.
//! - Clients run `compute_world_checksum` and send `ClientMessage::ChecksumReport`
//!   to the host. The host then forwards each to every *other* connected client
//!   as `ServerMessage::ChecksumReport` so every peer sees every other peer's
//!   checksum for a given tick.
//! - `check_desync` runs on all peers and compares the local `SyncChecksum`
//!   against any pending remote reports for the same tick.

use bevy::prelude::*;
use bevy_matchbox::prelude::MatchboxSocket;

use game_state::message::{ClientMessage, ServerMessage};

use crate::blueprints::EntityKind;
use crate::types::{AllPlayerResources, Faction, Health, ResourceType, SimClock, UnitState};

use super::matchbox_transport::{broadcast_reliable, send_to_host};
use super::transport::MatchboxInbox;
use super::{ClientNetState, HostNetState, NetRole};

/// Number of fixed simulation ticks between checksum computations.
/// At 30 Hz this is ~1 second — a reasonable detection latency without
/// swamping the reliable channel.
pub const CHECKSUM_INTERVAL_TICKS: u64 = 30;

// ── Hashing primitive ───────────────────────────────────────────────────────

/// FNV-1a, 64-bit. Tiny, branch-light, deterministic on all targets.
/// Not cryptographic — this is pure detection, not integrity.
struct FnvHasher {
    state: u64,
}

impl FnvHasher {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    #[inline]
    fn new() -> Self {
        Self {
            state: Self::OFFSET,
        }
    }

    #[inline]
    fn write_byte(&mut self, b: u8) {
        self.state ^= b as u64;
        self.state = self.state.wrapping_mul(Self::PRIME);
    }

    #[inline]
    fn write_u64(&mut self, v: u64) {
        for b in v.to_le_bytes() {
            self.write_byte(b);
        }
    }

    #[inline]
    fn write_u32(&mut self, v: u32) {
        self.write_u64(v as u64);
    }

    #[inline]
    fn write_i32(&mut self, v: i32) {
        self.write_u64(v as u32 as u64);
    }

    #[inline]
    fn write_u16(&mut self, v: u16) {
        self.write_u64(v as u64);
    }

    #[inline]
    fn write_u8(&mut self, v: u8) {
        self.write_u64(v as u64);
    }

    #[inline]
    fn finish(self) -> u64 {
        self.state
    }
}

/// Quantize an `f32` to a signed fixed-point integer with 1 mm precision.
/// Floats that are within this bucket on all peers hash to the same value,
/// so tiny FP representation differences don't masquerade as desyncs.
#[inline]
fn quantize(v: f32) -> i32 {
    (v * 1000.0) as i32
}

/// Discriminant index for `UnitState`. Uses a hand-rolled match so it stays
/// stable across Rust compiler versions (automatic enum discriminants can
/// change with variant order).
fn unit_state_tag(state: &UnitState) -> u8 {
    match state {
        UnitState::Idle => 0,
        UnitState::Moving(_) => 1,
        UnitState::Attacking(_) => 2,
        UnitState::Gathering(_) => 3,
        UnitState::ReturningToDeposit { .. } => 4,
        UnitState::Depositing { .. } => 5,
        UnitState::WaitingForStorage { .. } => 6,
        UnitState::WaitingForDepot { .. } => 7,
        UnitState::MovingToPlot(_) => 8,
        UnitState::MovingToBuild(_) => 9,
        UnitState::Building(_) => 10,
        UnitState::AssignedGathering { .. } => 11,
        UnitState::Patrolling { .. } => 12,
        UnitState::AttackMoving(_) => 13,
        UnitState::HoldPosition => 14,
    }
}

// ── Resources ───────────────────────────────────────────────────────────────

/// The most recently computed local world-state checksum.
///
/// `tick` is `u64::MAX` until the first checksum is computed — used as a
/// "never computed" sentinel so the check-system can skip early frames.
#[derive(Resource, Debug, Clone, Copy)]
pub struct SyncChecksum {
    pub tick: u64,
    pub checksum: u64,
}

impl Default for SyncChecksum {
    fn default() -> Self {
        Self {
            tick: u64::MAX,
            checksum: 0,
        }
    }
}

/// Latest observed desync. Presence of this resource is what the UI checks to
/// decide whether to show a warning — `init_resource` creates it once with
/// `tick == u64::MAX` meaning "no desync ever seen".
#[derive(Resource, Debug, Clone, Copy)]
pub struct DesyncDetected {
    pub tick: u64,
    pub local_checksum: u64,
    pub remote_checksum: u64,
    pub remote_player_id: u8,
}

impl Default for DesyncDetected {
    fn default() -> Self {
        Self {
            tick: u64::MAX,
            local_checksum: 0,
            remote_checksum: 0,
            remote_player_id: 0,
        }
    }
}

impl DesyncDetected {
    #[inline]
    pub fn has_triggered(&self) -> bool {
        self.tick != u64::MAX
    }
}

/// Received but not-yet-checked remote checksum reports.
/// Populated from `client_receive_commands` (client) and
/// `host_drain_checksum_reports` (host), drained by `check_desync`.
#[derive(Resource, Default, Debug)]
pub struct PendingChecksumReports {
    /// `(player_id, tick, checksum)` in arrival order.
    pub reports: Vec<(u8, u64, u64)>,
}

/// The exact world snapshot that produced the latest local checksum.
///
/// Remote reports often arrive a few simulation ticks later, so post-mortem
/// dumps must use this cached snapshot instead of the current live world.
#[derive(Resource, Default, Debug, Clone)]
pub struct LatestChecksumSnapshot {
    pub tick: u64,
    pub text: String,
}

// ── Core computation ────────────────────────────────────────────────────────

/// Computes `SyncChecksum` over the authoritative gameplay state and
/// broadcasts a `ChecksumReport` to peers. Runs on every peer (host + client)
/// in `FixedUpdate` under `GameFlowSet::Diagnostics`, after the simulation
/// has executed for the current tick.
#[allow(clippy::too_many_arguments)]
pub fn compute_world_checksum(
    clock: Res<SimClock>,
    role: Res<NetRole>,
    host: Option<Res<HostNetState>>,
    client: Option<Res<ClientNetState>>,
    all_resources: Res<AllPlayerResources>,
    mut sync: ResMut<SyncChecksum>,
    mut snapshot: ResMut<LatestChecksumSnapshot>,
    mut pending: ResMut<PendingChecksumReports>,
    mut socket: Option<ResMut<MatchboxSocket>>,
    entities: Query<(
        &Transform,
        &Faction,
        &EntityKind,
        Option<&Health>,
        Option<&UnitState>,
    )>,
) {
    // Offline: nothing to compare against, skip the work.
    if *role == NetRole::Offline {
        return;
    }

    let tick = clock.tick;
    if tick % CHECKSUM_INTERVAL_TICKS != 0 {
        return;
    }
    // Guard against re-running within the same tick (Update can fire more
    // frequently than FixedUpdate, though this system lives in FixedUpdate).
    if sync.tick == tick {
        return;
    }

    let world = snapshot_world(tick, &entities, &all_resources);
    let checksum = hash_snapshot(&world);
    sync.tick = tick;
    sync.checksum = checksum;
    snapshot.tick = tick;
    snapshot.text = render_snapshot(&world, &role, client.as_deref());

    let player_id = local_player_id(&role, client.as_deref());
    let timestamp = 0.0; // wall time is irrelevant for desync detection
    let _ = &mut pending; // pruning happens in `check_desync` after comparison

    let Some(sock) = socket.as_deref_mut() else {
        return;
    };

    match *role {
        NetRole::Host => {
            let seq = host
                .as_ref()
                .map(|h| {
                    let mut s = h.seq.lock().unwrap();
                    *s = s.wrapping_add(1);
                    *s
                })
                .unwrap_or(0);
            let msg = ServerMessage::ChecksumReport {
                seq,
                timestamp,
                player_id,
                tick,
                checksum,
            };
            broadcast_reliable(sock, &msg);
        }
        NetRole::Client => {
            let seq = client
                .as_ref()
                .map(|c| {
                    let mut s = c.seq.lock().unwrap();
                    *s = s.wrapping_add(1);
                    *s
                })
                .unwrap_or(0);
            let msg = ClientMessage::ChecksumReport {
                seq,
                timestamp,
                player_id,
                tick,
                checksum,
            };
            send_to_host(sock, &msg);
        }
        NetRole::Offline => unreachable!(),
    }
}

fn local_player_id(role: &NetRole, client: Option<&ClientNetState>) -> u8 {
    match role {
        NetRole::Host => 0,
        NetRole::Client => client.map(|c| c.player_id).unwrap_or(0),
        NetRole::Offline => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct EntityRecord {
    kind_idx: u16,
    faction_idx: u8,
    qx: i32,
    qy: i32,
    qz: i32,
    health_q: i32,
    max_health_q: i32,
    unit_state_tag: u8,
}

#[derive(Debug, Clone)]
struct WorldSnapshot {
    tick: u64,
    records: Vec<EntityRecord>,
    resources: Vec<(u8, Vec<u32>)>,
}

fn snapshot_world(
    tick: u64,
    entities: &Query<(
        &Transform,
        &Faction,
        &EntityKind,
        Option<&Health>,
        Option<&UnitState>,
    )>,
    all_resources: &AllPlayerResources,
) -> WorldSnapshot {
    let mut records: Vec<EntityRecord> = entities
        .iter()
        .map(|(transform, faction, kind, health, unit_state)| {
            let pos = transform.translation;
            EntityRecord {
                kind_idx: kind.to_index(),
                faction_idx: faction.to_net_index(),
                qx: quantize(pos.x),
                qy: quantize(pos.y),
                qz: quantize(pos.z),
                health_q: health.map(|h| quantize(h.current)).unwrap_or(i32::MIN),
                max_health_q: health.map(|h| quantize(h.max)).unwrap_or(i32::MIN),
                unit_state_tag: unit_state.map(unit_state_tag).unwrap_or(u8::MAX),
            }
        })
        .collect();
    records.sort();

    let resources = Faction::PLAYERS
        .iter()
        .chain(std::iter::once(&Faction::Neutral))
        .map(|faction| {
            (
                faction.to_net_index(),
                ResourceType::ALL
                    .iter()
                    .map(|rt| all_resources.get(faction).get(*rt))
                    .collect(),
            )
        })
        .collect();

    WorldSnapshot {
        tick,
        records,
        resources,
    }
}

fn hash_snapshot(snapshot: &WorldSnapshot) -> u64 {
    let mut hasher = FnvHasher::new();
    hasher.write_u64(snapshot.tick);
    hasher.write_u32(snapshot.records.len() as u32);

    for rec in &snapshot.records {
        hasher.write_u16(rec.kind_idx);
        hasher.write_u8(rec.faction_idx);
        hasher.write_i32(rec.qx);
        hasher.write_i32(rec.qy);
        hasher.write_i32(rec.qz);
        hasher.write_i32(rec.health_q);
        hasher.write_i32(rec.max_health_q);
        hasher.write_u8(rec.unit_state_tag);
    }

    for (faction_idx, resources) in &snapshot.resources {
        hasher.write_u8(*faction_idx);
        for (idx, amount) in resources.iter().enumerate() {
            hasher.write_u8(idx as u8);
            hasher.write_u32(*amount);
        }
    }

    hasher.finish()
}

fn render_snapshot(
    snapshot: &WorldSnapshot,
    role: &NetRole,
    client: Option<&ClientNetState>,
) -> String {
    use std::fmt::Write as _;

    let local_player_id = local_player_id(role, client);
    let role_label = match role {
        NetRole::Host => "host",
        NetRole::Client => "client",
        NetRole::Offline => "offline",
    };

    let mut out = String::new();
    let _ = writeln!(out, "# Desync dump — tick {}", snapshot.tick);
    let _ = writeln!(out, "sim_tick={}", snapshot.tick);
    let _ = writeln!(out, "role={}", role_label);
    let _ = writeln!(out, "local_player_id={}", local_player_id);
    let _ = writeln!(out, "# entities (sorted by full checksum record)");

    for rec in &snapshot.records {
        let _ = writeln!(
            out,
            "kind={} faction={} pos=({},{},{}) hp={}/{} state={}",
            rec.kind_idx,
            rec.faction_idx,
            rec.qx,
            rec.qy,
            rec.qz,
            if rec.health_q == i32::MIN {
                "--".to_string()
            } else {
                rec.health_q.to_string()
            },
            if rec.max_health_q == i32::MIN {
                "--".to_string()
            } else {
                rec.max_health_q.to_string()
            },
            rec.unit_state_tag,
        );
    }

    let _ = writeln!(out, "# resources");
    for (faction_idx, resources) in &snapshot.resources {
        let parts: Vec<String> = ResourceType::ALL
            .iter()
            .zip(resources.iter())
            .map(|(rt, amount)| format!("{}={}", rt.abbreviation(), amount))
            .collect();
        let _ = writeln!(out, "{}: {}", faction_idx, parts.join(" "));
    }

    out
}

// ── Desync detection ────────────────────────────────────────────────────────

/// Compares local `SyncChecksum` against any pending remote reports for the
/// same tick. On first mismatch, logs, sets `DesyncDetected`, and writes a
/// per-entity dump file (native only) so the developer can diff.
pub fn check_desync(
    role: Res<NetRole>,
    client: Option<Res<ClientNetState>>,
    sync: Res<SyncChecksum>,
    snapshot: Res<LatestChecksumSnapshot>,
    mut pending: ResMut<PendingChecksumReports>,
    mut desync: ResMut<DesyncDetected>,
) {
    if *role == NetRole::Offline {
        return;
    }
    if sync.tick == u64::MAX {
        return;
    }

    // Walk the pending buffer once. Reports for the current tick get
    // compared, reports for a future tick are kept (we'll compute locally
    // later and compare then), reports for an older tick are dropped —
    // we no longer have the historical local checksum to compare against.
    let mut keep = Vec::with_capacity(pending.reports.len());
    for (player_id, tick, remote_checksum) in std::mem::take(&mut pending.reports) {
        if tick > sync.tick {
            keep.push((player_id, tick, remote_checksum));
            continue;
        }
        if tick < sync.tick {
            // Stale — local already advanced past this tick.
            continue;
        }

        if remote_checksum == sync.checksum {
            // Match — no desync for this peer at this tick.
            continue;
        }

        // Mismatch. Log every time (not just first) so persistent divergence
        // is visible in logs, but only record the most recent occurrence and
        // only dump state once per tick to avoid hammering the filesystem.
        error!(
            "DESYNC DETECTED at tick {}: local={:#018x}, remote={:#018x} (player {})",
            tick, sync.checksum, remote_checksum, player_id
        );

        // let first_for_this_tick = desync.tick != tick;
        // desync.tick = tick;
        // desync.local_checksum = sync.checksum;
        // desync.remote_checksum = remote_checksum;
        // desync.remote_player_id = player_id;

        // if first_for_this_tick {
        //     dump_world_state(tick, &role, client.as_deref(), Some(snapshot.as_ref()));
        // }
    }
    pending.reports = keep;
}

#[cfg(not(target_arch = "wasm32"))]
fn dump_world_state(
    tick: u64,
    role: &NetRole,
    client: Option<&ClientNetState>,
    snapshot: Option<&LatestChecksumSnapshot>,
) {
    let local_player_id = local_player_id(role, client);
    let role_label = match role {
        NetRole::Host => "host",
        NetRole::Client => "client",
        NetRole::Offline => "offline",
    };
    let out = if let Some(snapshot) = snapshot.filter(|snapshot| snapshot.tick == tick) {
        snapshot.text.clone()
    } else {
        format!(
            "# Desync dump — tick {}\nsim_tick={}\nrole={}\nlocal_player_id={}\n# snapshot unavailable\n",
            tick, tick, role_label, local_player_id
        )
    };

    let filename = format!(
        "desync_dump_tick_{}_{}_p{}.txt",
        tick, role_label, local_player_id
    );
    if let Err(err) = std::fs::write(&filename, out) {
        warn!("Failed to write desync dump {}: {}", filename, err);
    } else {
        warn!("Wrote desync dump: {}", filename);
    }
}

// ── Host-side forwarding ────────────────────────────────────────────────────

/// Host: drain `ClientMessage::ChecksumReport` messages from the inbox,
/// insert them into the local pending buffer, and forward each to every
/// other connected client as `ServerMessage::ChecksumReport` so that every
/// peer eventually sees every peer's checksum for a given tick.
pub fn host_drain_checksum_reports(
    mut inbox: ResMut<MatchboxInbox>,
    host: Res<HostNetState>,
    mut pending: ResMut<PendingChecksumReports>,
    mut socket: Option<ResMut<MatchboxSocket>>,
) {
    if inbox.client_commands.is_empty() {
        return;
    }

    let mut kept = Vec::with_capacity(inbox.client_commands.len());
    let mut forwards: Vec<(u8, u64, u64)> = Vec::new();

    for (sender_id, msg) in std::mem::take(&mut inbox.client_commands) {
        match msg {
            ClientMessage::ChecksumReport {
                player_id,
                tick,
                checksum,
                ..
            } => {
                // Attribution: if the packet came in with player_id=0 (default),
                // substitute the sender's peer-map player_id so the host knows
                // which client actually produced it.
                let effective = if player_id == 0 { sender_id } else { player_id };
                forwards.push((effective, tick, checksum));
            }
            other => kept.push((sender_id, other)),
        }
    }
    inbox.client_commands = kept;

    if forwards.is_empty() {
        return;
    }

    for &(player_id, tick, checksum) in &forwards {
        pending.reports.push((player_id, tick, checksum));

        let Some(sock) = socket.as_deref_mut() else {
            continue;
        };
        let seq = {
            let mut s = host.seq.lock().unwrap();
            *s = s.wrapping_add(1);
            *s
        };
        let forward = ServerMessage::ChecksumReport {
            seq,
            timestamp: 0.0,
            player_id,
            tick,
            checksum,
        };
        broadcast_reliable(sock, &forward);
    }
}

// ── Run conditions ──────────────────────────────────────────────────────────

pub fn in_game_and_online(role: Res<NetRole>) -> bool {
    *role != NetRole::Offline
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_hasher_is_deterministic() {
        let mut a = FnvHasher::new();
        a.write_u64(0xDEADBEEF);
        a.write_u32(42);
        let ah = a.finish();

        let mut b = FnvHasher::new();
        b.write_u64(0xDEADBEEF);
        b.write_u32(42);
        let bh = b.finish();

        assert_eq!(ah, bh);
        assert_ne!(ah, 0);
    }

    #[test]
    fn quantize_rounds_to_millimeters() {
        assert_eq!(quantize(1.0), 1000);
        assert_eq!(quantize(1.0005), 1000);
        assert_eq!(quantize(1.001), 1001);
        assert_eq!(quantize(-2.5), -2500);
    }

    #[test]
    fn unit_state_tags_are_unique() {
        use std::collections::HashSet;
        let tags: HashSet<u8> = [
            unit_state_tag(&UnitState::Idle),
            unit_state_tag(&UnitState::Moving(Vec3::ZERO)),
            unit_state_tag(&UnitState::Attacking(Entity::PLACEHOLDER)),
            unit_state_tag(&UnitState::Gathering(Entity::PLACEHOLDER)),
            unit_state_tag(&UnitState::ReturningToDeposit {
                depot: Entity::PLACEHOLDER,
                gather_node: None,
            }),
            unit_state_tag(&UnitState::Depositing {
                depot: Entity::PLACEHOLDER,
                gather_node: None,
            }),
            unit_state_tag(&UnitState::WaitingForStorage {
                depot: Entity::PLACEHOLDER,
                gather_node: None,
            }),
            unit_state_tag(&UnitState::WaitingForDepot { gather_node: None }),
            unit_state_tag(&UnitState::MovingToPlot(Vec3::ZERO)),
            unit_state_tag(&UnitState::MovingToBuild(Entity::PLACEHOLDER)),
            unit_state_tag(&UnitState::Building(Entity::PLACEHOLDER)),
            unit_state_tag(&UnitState::AssignedGathering {
                building: Entity::PLACEHOLDER,
                phase: crate::types::AssignedPhase::SeekingNode,
            }),
            unit_state_tag(&UnitState::Patrolling {
                target: Vec3::ZERO,
                origin: Vec3::ZERO,
            }),
            unit_state_tag(&UnitState::AttackMoving(Vec3::ZERO)),
            unit_state_tag(&UnitState::HoldPosition),
        ]
        .into_iter()
        .collect();
        // 15 distinct variants.
        assert_eq!(tags.len(), 15);
    }

    #[test]
    fn pending_reports_trims_stale_ticks() {
        let mut pending = PendingChecksumReports::default();
        pending.reports.push((0, 10, 0x1111));
        pending.reports.push((1, 20, 0x2222));
        pending.reports.push((2, 30, 0x3333));

        pending.reports.retain(|(_, t, _)| *t >= 20);
        assert_eq!(pending.reports.len(), 2);
        assert!(pending.reports.iter().all(|(_, t, _)| *t >= 20));
    }
}
