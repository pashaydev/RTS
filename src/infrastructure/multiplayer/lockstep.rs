//! Deterministic lockstep input synchronization.
//!
//! All peers run identical simulation at 30 Hz. Each peer stamps its local
//! input for `SimClock.tick + input_delay` and broadcasts it. A tick only
//! advances once every connected player's input for that tick is in the
//! buffer — so all peers apply the same commands in the same order and stay
//! bit-for-bit identical.
//!
//! Transport topology is host-relay (star): clients send
//! `ClientMessage::InputBroadcast` to the host, and the host forwards them
//! as `ServerMessage::InputBroadcast` to every other client. The host is
//! NOT authoritative for simulation — it just forwards packets.

use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy_matchbox::prelude::*;
use std::collections::BTreeMap;

use game_state::message::{ClientMessage, InputCommand, PlayerInput, ServerMessage};

use crate::blueprints::EntityKind;
use crate::infrastructure::net_bridge::EntityNetMap;
use crate::types::*;

use super::host_systems::{execute_input_command, BuildWorkerSnapshot};
use super::transport::MatchboxInbox;
use super::{ClientNetState, HostNetState, LobbyState, NetRole};

/// Input delay in fixed ticks — local input is scheduled for
/// `SimClock.tick + INPUT_DELAY`. At 30 Hz, 3 ticks ≈ 100 ms.
pub const DEFAULT_INPUT_DELAY: u64 = 3;

/// Per-tick, per-player input buffer for deterministic lockstep.
#[derive(Resource, Debug)]
pub struct LockstepInputBuffer {
    /// `tick -> player_id -> commands for that tick`. BTreeMap gives
    /// deterministic iteration order across peers.
    pub pending: BTreeMap<u64, BTreeMap<u8, PlayerInput>>,
    /// Highest tick for which EVERY connected player's input is confirmed.
    /// The gate permits advancing into `confirmed_tick` (inclusive).
    pub confirmed_tick: u64,
    /// Tick offset between local capture and scheduled execution. Gives
    /// remote peers time to receive the input before their gate blocks on
    /// it.
    pub input_delay: u64,
    /// Number of connected players (including self) whose input must be
    /// present for a tick to become confirmed.
    pub expected_players: u8,
    /// Latched gate decision for the current FixedUpdate iteration.
    /// Computed once per iteration in `FixedFirst` so both the sim-clock
    /// advance and the sim set see a consistent value.
    pub advance_allowed: bool,
    /// Highest tick that `lockstep_apply_tick_inputs` has already drained.
    /// Prevents re-applying the same commands if FixedUpdate re-runs
    /// without an advance.
    pub last_applied_tick: Option<u64>,
    /// Highest local tick already inserted/broadcast by this peer.
    /// This bootstraps the initial delayed ticks so the gate can advance.
    pub last_local_tick_sent: Option<u64>,
    /// Local sequence counter for outgoing `InputBroadcast` messages.
    #[allow(dead_code)]
    seq: u32,
}

impl Default for LockstepInputBuffer {
    fn default() -> Self {
        Self {
            pending: BTreeMap::new(),
            confirmed_tick: 0,
            input_delay: DEFAULT_INPUT_DELAY,
            expected_players: 1,
            advance_allowed: true,
            last_applied_tick: None,
            last_local_tick_sent: None,
            seq: 0,
        }
    }
}

impl LockstepInputBuffer {
    /// Reset all per-match state. Call on `OnEnter(InGame)` to clear any
    /// leftovers from a prior session.
    pub fn reset(&mut self) {
        self.pending.clear();
        self.confirmed_tick = 0;
        self.advance_allowed = true;
        self.last_applied_tick = None;
        self.last_local_tick_sent = None;
        self.seq = 0;
    }

    /// Insert a per-tick input. Returns true if the slot was new (i.e. this
    /// player had no prior input for that tick).
    pub fn insert(&mut self, player_id: u8, input: PlayerInput) -> bool {
        let tick = input.tick;
        let slot = self.pending.entry(tick).or_default();
        slot.insert(player_id, input).is_none()
    }

    /// Walk forward from `confirmed_tick + 1`, bumping `confirmed_tick` for
    /// every tick where all expected players have an input. Stops at the
    /// first gap.
    pub fn recompute_confirmed(&mut self) {
        if self.expected_players == 0 {
            return;
        }
        loop {
            let next = self.confirmed_tick + 1;
            let Some(slot) = self.pending.get(&next) else {
                break;
            };
            if (slot.len() as u8) < self.expected_players {
                break;
            }
            self.confirmed_tick = next;
        }
    }

    /// Drain all inputs scheduled for `tick` in deterministic (player_id)
    /// order. Returns an empty `Vec` if nothing is queued for this tick.
    pub fn drain_tick(&mut self, tick: u64) -> Vec<PlayerInput> {
        let Some(slot) = self.pending.remove(&tick) else {
            return Vec::new();
        };
        // BTreeMap iteration is already sorted by key (player_id).
        slot.into_values().collect()
    }
}

/// Queue of local input commands captured this frame but not yet broadcast.
/// UI/right-click handlers push into this and
/// `collect_and_broadcast_local_input` drains it once per frame.
#[derive(Resource, Default, Debug)]
pub struct PendingLocalInput {
    pub batches: Vec<LocalInputBatch>,
}

#[derive(Debug, Clone)]
pub struct LocalInputBatch {
    pub entity_ids: Vec<u32>,
    pub commands: Vec<InputCommand>,
}

impl PendingLocalInput {
    pub fn push(&mut self, entity_ids: Vec<u32>, commands: Vec<InputCommand>) {
        if commands.is_empty() {
            return;
        }
        self.batches.push(LocalInputBatch {
            entity_ids,
            commands,
        });
    }
}

#[derive(SystemParam)]
pub struct GameplayInputSubmit<'w> {
    net_role: Res<'w, NetRole>,
    net_map: Option<Res<'w, EntityNetMap>>,
    pending_local_input: ResMut<'w, PendingLocalInput>,
}

impl<'w> GameplayInputSubmit<'w> {
    pub fn is_online(&self) -> bool {
        *self.net_role != NetRole::Offline
    }

    pub fn entity_ids_for<I>(&self, entities: I) -> Option<Vec<u32>>
    where
        I: IntoIterator<Item = Entity>,
    {
        let net_map = self.net_map.as_ref()?;
        let entity_ids: Vec<u32> = entities
            .into_iter()
            .filter_map(|entity| net_map.to_net.get(&entity).copied())
            .collect();
        Some(entity_ids)
    }

    pub fn submit<I>(&mut self, entities: I, commands: Vec<InputCommand>) -> bool
    where
        I: IntoIterator<Item = Entity>,
    {
        if !self.is_online() {
            return false;
        }
        let Some(entity_ids) = self.entity_ids_for(entities) else {
            return false;
        };
        if entity_ids.is_empty() {
            return false;
        }
        self.pending_local_input.push(entity_ids, commands);
        true
    }

    /// Push a raw input directly without going through the entity-id
    /// resolution path. Used when the caller needs to supply a building/unit
    /// id inside the command payload but has no selection entity list.
    pub fn push_raw(&mut self, entity_ids: Vec<u32>, commands: Vec<InputCommand>) {
        self.pending_local_input.push(entity_ids, commands);
    }
}

// ── Run conditions ──────────────────────────────────────────────────────────

pub fn lockstep_advance_allowed(buffer: Res<LockstepInputBuffer>) -> bool {
    buffer.advance_allowed
}

#[derive(SystemParam)]
pub struct LockstepApplyContext<'w, 's> {
    lobby: Option<Res<'w, LobbyState>>,
    net_map: Option<Res<'w, EntityNetMap>>,
    registry: Res<'w, crate::blueprints::BlueprintRegistry>,
    all_resources: ResMut<'w, AllPlayerResources>,
    carried_totals: Res<'w, CarriedResourceTotals>,
    carried_totals_dirty: ResMut<'w, crate::simulation::resources::CarriedTotalsDirty>,
    pending_drains: ResMut<'w, PendingCarriedDrains>,
    unit_state_queries: ParamSet<
        'w,
        's,
        (
            Query<'w, 's, &'static mut UnitState>,
            Query<
                'w,
                's,
                (
                    Entity,
                    &'static Transform,
                    &'static UnitState,
                    &'static Faction,
                    &'static EntityKind,
                    Option<&'static PendingBuildOrder>,
                ),
                With<Unit>,
            >,
        ),
    >,
    carrying: Query<'w, 's, &'static mut Carrying, With<Unit>>,
    health: Query<'w, 's, &'static mut Health, With<Unit>>,
    unit_abilities: Query<'w, 's, &'static mut UnitAbilities, With<Unit>>,
    worker_assignments: Query<'w, 's, &'static BuildingAssignment, With<Unit>>,
    task_queues: Query<'w, 's, &'static mut TaskQueue, With<Unit>>,
    training_buildings: Query<
        'w,
        's,
        (
            &'static mut TrainingQueue,
            &'static EntityKind,
            Option<&'static BuildingLevel>,
        ),
        With<Building>,
    >,
    next_task_id: ResMut<'w, NextTaskId>,
    transforms: Query<'w, 's, &'static GlobalTransform>,
    existing_buildings: Query<
        'w,
        's,
        (&'static Transform, &'static BuildingFootprint, &'static EntityKind),
        (With<Building>, Without<GhostBuilding>),
    >,
    building_state: Query<
        'w,
        's,
        (&'static Faction, Has<BuildingPaused>),
        With<Building>,
    >,
    tower_auto_attack: Query<'w, 's, &'static mut TowerAutoAttackEnabled, With<Building>>,
    obstacle_grid: Res<'w, ObstacleGrid>,
}

// ── Systems ─────────────────────────────────────────────────────────────────

/// Latches the gate decision for this FixedUpdate iteration. Runs in
/// `FixedFirst` BEFORE `advance_sim_clock`.
///
/// Formula: we're about to execute `tick = SimClock.tick + 1`, so we allow
/// the advance iff all players have confirmed input for at least that tick.
/// Offline bypasses the gate entirely.
pub fn lockstep_check_gate(
    role: Res<NetRole>,
    clock: Res<SimClock>,
    lobby: Option<Res<LobbyState>>,
    mut buffer: ResMut<LockstepInputBuffer>,
) {
    if *role == NetRole::Offline {
        buffer.advance_allowed = true;
        return;
    }

    // Refresh expected player count from the lobby so disconnects unblock
    // the gate promptly.
    if let Some(lobby) = lobby.as_ref() {
        let connected = lobby.players.iter().filter(|p| p.connected).count() as u8;
        buffer.expected_players = connected.max(1);
    }

    buffer.recompute_confirmed();

    let target_tick = clock.tick + 1;
    buffer.advance_allowed = buffer.confirmed_tick >= target_tick;
}

/// Early-phase system in the simulation set that drains the buffer for
/// `SimClock.tick` and applies every queued command via
/// `execute_input_command`, deterministically ordered by player_id.
/// Runs only when the lockstep gate is open.
#[allow(clippy::too_many_arguments)]
pub fn lockstep_apply_tick_inputs(
    mut commands: Commands,
    clock: Res<SimClock>,
    time: Res<Time>,
    mut buffer: ResMut<LockstepInputBuffer>,
    mut exec: LockstepApplyContext,
) {
    if buffer.last_applied_tick == Some(clock.tick) {
        return;
    }
    let Some(lobby) = exec.lobby.as_ref() else {
        return;
    };
    let Some(net_map) = exec.net_map.as_ref() else {
        return;
    };
    let inputs = buffer.drain_tick(clock.tick);
    for input in &inputs {
        let workers: Vec<BuildWorkerSnapshot> = exec
            .unit_state_queries
            .p1()
            .iter()
            .map(|(entity, transform, unit_state, faction, kind, _)| BuildWorkerSnapshot {
                entity,
                translation: transform.translation,
                state: *unit_state,
                faction: *faction,
                kind: *kind,
            })
            .collect();
        let mut unit_states = exec.unit_state_queries.p0();
        execute_input_command(
            &mut commands,
            input,
            time.elapsed_secs_f64(),
            lobby,
            net_map,
            &mut exec.all_resources,
            &exec.carried_totals,
            &mut exec.carried_totals_dirty,
            &mut exec.pending_drains,
            &mut unit_states,
            &mut exec.carrying,
            &mut exec.health,
            &mut exec.unit_abilities,
            &exec.worker_assignments,
            &mut exec.task_queues,
            &mut exec.training_buildings,
            &mut exec.next_task_id,
            &exec.transforms,
            &exec.existing_buildings,
            &exec.building_state,
            &mut exec.tower_auto_attack,
            &workers,
            &exec.obstacle_grid,
            &exec.registry,
        );
    }
    buffer.last_applied_tick = Some(clock.tick);
}

/// Resolves the local player's network id. Host is always player_id 0 by
/// convention (`HostNetState` is implicit), clients carry their own id in
/// `ClientNetState`.
fn local_player_id(role: &NetRole, client: Option<&ClientNetState>) -> u8 {
    match role {
        NetRole::Host => 0,
        NetRole::Client => client.map(|c| c.player_id).unwrap_or(0),
        NetRole::Offline => 0,
    }
}

/// Captures queued local input, stamps tick = `SimClock.tick + input_delay`,
/// inserts into the local buffer, and broadcasts to peers over the reliable
/// channel. Runs in `GameFlowSet::Input`.
#[allow(clippy::too_many_arguments)]
pub fn collect_and_broadcast_local_input(
    role: Res<NetRole>,
    clock: Res<SimClock>,
    time: Res<Time>,
    client: Option<Res<ClientNetState>>,
    host: Option<Res<HostNetState>>,
    mut pending: ResMut<PendingLocalInput>,
    mut buffer: ResMut<LockstepInputBuffer>,
    mut socket: Option<ResMut<MatchboxSocket>>,
) {
    if *role == NetRole::Offline {
        // Offline: nothing to coordinate with peers, drop the pending queue.
        // Right-click handlers still drive gameplay directly in Offline.
        pending.batches.clear();
        return;
    }

    let player_id = local_player_id(&role, client.as_deref());
    let scheduled_tick = clock.tick + buffer.input_delay.max(1);
    let timestamp = time.elapsed_secs_f64();
    let next_unsent_tick = buffer
        .last_local_tick_sent
        .map(|tick| tick.saturating_add(1))
        .unwrap_or(1)
        .max(clock.tick.saturating_add(1));

    for tick in next_unsent_tick..=scheduled_tick {
        let batches = if tick == scheduled_tick {
            if pending.batches.is_empty() {
                vec![LocalInputBatch {
                    entity_ids: Vec::new(),
                    commands: Vec::new(),
                }]
            } else {
                std::mem::take(&mut pending.batches)
            }
        } else {
            vec![LocalInputBatch {
                entity_ids: Vec::new(),
                commands: Vec::new(),
            }]
        };

        for batch in batches {
            let input = PlayerInput {
                player_id: player_id as u32,
                tick,
                entity_ids: batch.entity_ids,
                commands: batch.commands,
            };
            buffer.insert(player_id, input.clone());

            let seq = match *role {
                NetRole::Host => host.as_ref().map(|h| {
                    let mut s = h.seq.lock().unwrap();
                    *s = s.wrapping_add(1);
                    *s
                }),
                NetRole::Client => client.as_ref().map(|c| {
                    let mut s = c.seq.lock().unwrap();
                    *s = s.wrapping_add(1);
                    *s
                }),
                NetRole::Offline => None,
            };
            let Some(seq) = seq else {
                warn!(
                    "lockstep: no {:?} net state, skipping broadcast for tick {}",
                    *role, tick
                );
                continue;
            };

            let Some(sock) = socket.as_deref_mut() else {
                continue;
            };
            match *role {
                NetRole::Host => {
                    let msg = ServerMessage::InputBroadcast {
                        seq,
                        timestamp,
                        player_id,
                        input,
                    };
                    super::matchbox_transport::broadcast_reliable(sock, &msg);
                }
                NetRole::Client => {
                    let msg = ClientMessage::InputBroadcast {
                        seq,
                        timestamp,
                        player_id,
                        input,
                    };
                    super::matchbox_transport::send_to_host(sock, &msg);
                }
                NetRole::Offline => unreachable!(),
            }
        }

        buffer.last_local_tick_sent = Some(tick);
    }

    buffer.recompute_confirmed();
}

/// Host-side: drain inbox `ClientMessage::InputBroadcast` messages, forward
/// them to all OTHER connected clients (as `ServerMessage::InputBroadcast`),
/// and insert them into the local buffer. Runs in `GameFlowSet::NetworkReceive`
/// under `is_host`.
pub fn host_receive_remote_inputs(
    mut inbox: ResMut<MatchboxInbox>,
    host: Res<HostNetState>,
    mut buffer: ResMut<LockstepInputBuffer>,
    mut socket: Option<ResMut<MatchboxSocket>>,
    time: Res<Time>,
) {
    if inbox.client_commands.is_empty() {
        return;
    }

    // Extract InputBroadcast packets, leaving other ClientMessage variants in
    // place for the rest of the host pipeline (join/leave/ping/chat/etc.).
    let mut kept = Vec::with_capacity(inbox.client_commands.len());
    let mut forwards = Vec::new();

    for (sender_id, msg) in std::mem::take(&mut inbox.client_commands) {
        match msg {
            ClientMessage::InputBroadcast {
                player_id, input, ..
            } => {
                let effective_player_id = if player_id == 0 { sender_id } else { player_id };
                forwards.push((effective_player_id, input));
            }
            other => kept.push((sender_id, other)),
        }
    }
    inbox.client_commands = kept;

    for (player_id, input) in forwards {
        buffer.insert(player_id, input.clone());

        let seq = {
            let mut s = host.seq.lock().unwrap();
            *s = s.wrapping_add(1);
            *s
        };
        let forward = ServerMessage::InputBroadcast {
            seq,
            timestamp: time.elapsed_secs_f64(),
            player_id,
            input,
        };
        if let Some(sock) = socket.as_deref_mut() {
            super::matchbox_transport::broadcast_reliable(sock, &forward);
        }
    }

    buffer.recompute_confirmed();
}

/// Client-side: drain inbox `ServerMessage::InputBroadcast` messages (already
/// populated by `client_receive_commands`) and insert them into the local
/// buffer. Runs after `client_receive_commands` in
/// `GameFlowSet::NetworkReceive`.
pub fn client_receive_remote_inputs(
    mut pending: ResMut<super::client::apply::PendingInputBroadcasts>,
    mut buffer: ResMut<LockstepInputBuffer>,
) {
    if pending.inputs.is_empty() {
        return;
    }
    for (player_id, input) in std::mem::take(&mut pending.inputs) {
        buffer.insert(player_id, input);
    }
    buffer.recompute_confirmed();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_input(player_id: u8, tick: u64) -> PlayerInput {
        PlayerInput {
            player_id: player_id as u32,
            tick,
            entity_ids: vec![1],
            commands: vec![InputCommand::Stop],
        }
    }

    #[test]
    fn confirmed_advances_when_all_players_present() {
        let mut buf = LockstepInputBuffer {
            expected_players: 2,
            ..Default::default()
        };
        buf.insert(0, sample_input(0, 1));
        buf.recompute_confirmed();
        assert_eq!(buf.confirmed_tick, 0, "single player should not confirm");

        buf.insert(1, sample_input(1, 1));
        buf.recompute_confirmed();
        assert_eq!(buf.confirmed_tick, 1);

        buf.insert(0, sample_input(0, 2));
        buf.insert(1, sample_input(1, 2));
        buf.recompute_confirmed();
        assert_eq!(buf.confirmed_tick, 2);
    }

    #[test]
    fn confirmed_stalls_on_gap() {
        let mut buf = LockstepInputBuffer {
            expected_players: 2,
            ..Default::default()
        };
        // Tick 1 missing player 1, tick 2 complete — should NOT jump to 2.
        buf.insert(0, sample_input(0, 1));
        buf.insert(0, sample_input(0, 2));
        buf.insert(1, sample_input(1, 2));
        buf.recompute_confirmed();
        assert_eq!(buf.confirmed_tick, 0);

        buf.insert(1, sample_input(1, 1));
        buf.recompute_confirmed();
        assert_eq!(buf.confirmed_tick, 2);
    }

    #[test]
    fn drain_tick_removes_and_returns_sorted_by_player_id() {
        let mut buf = LockstepInputBuffer {
            expected_players: 3,
            ..Default::default()
        };
        buf.insert(2, sample_input(2, 5));
        buf.insert(0, sample_input(0, 5));
        buf.insert(1, sample_input(1, 5));

        let drained = buf.drain_tick(5);
        let ids: Vec<u32> = drained.iter().map(|i| i.player_id).collect();
        assert_eq!(ids, vec![0, 1, 2]);
        assert!(buf.pending.get(&5).is_none());
    }

    #[test]
    fn gate_check_offline_always_allows() {
        let mut buf = LockstepInputBuffer::default();
        buf.advance_allowed = false;

        let mut world = World::new();
        world.insert_resource(NetRole::Offline);
        world.insert_resource(SimClock { tick: 100 });
        world.insert_resource(buf);
        let mut system = IntoSystem::into_system(lockstep_check_gate);
        system.initialize(&mut world);
        let _ = system.run((), &mut world);

        assert!(world.resource::<LockstepInputBuffer>().advance_allowed);
    }

    fn test_lobby(connected_count: usize) -> LobbyState {
        use crate::infrastructure::multiplayer::LobbyPlayer;
        let factions = [
            Faction::Player1,
            Faction::Player2,
            Faction::Player3,
            Faction::Player4,
        ];
        let players = (0..connected_count)
            .map(|i| LobbyPlayer {
                player_id: i as u8,
                name: format!("p{}", i),
                seat_index: i as u8,
                faction: factions[i],
                color_index: i as u8,
                is_host: i == 0,
                connected: true,
            })
            .collect();
        LobbyState {
            players,
            ..Default::default()
        }
    }

    #[test]
    fn gate_check_host_blocks_when_inputs_missing() {
        let mut buf = LockstepInputBuffer::default();
        buf.insert(0, sample_input(0, 1));

        let mut world = World::new();
        world.insert_resource(NetRole::Host);
        world.insert_resource(SimClock { tick: 0 });
        world.insert_resource(test_lobby(2));
        world.insert_resource(buf);
        let mut system = IntoSystem::into_system(lockstep_check_gate);
        system.initialize(&mut world);
        let _ = system.run((), &mut world);

        // Missing player 1 for tick 1 → cannot advance past tick 0.
        let buffer = world.resource::<LockstepInputBuffer>();
        assert_eq!(buffer.expected_players, 2);
        assert!(!buffer.advance_allowed);
    }

    #[test]
    fn gate_check_host_allows_when_all_inputs_present() {
        let mut buf = LockstepInputBuffer::default();
        buf.insert(0, sample_input(0, 1));
        buf.insert(1, sample_input(1, 1));

        let mut world = World::new();
        world.insert_resource(NetRole::Host);
        world.insert_resource(SimClock { tick: 0 });
        world.insert_resource(test_lobby(2));
        world.insert_resource(buf);
        let mut system = IntoSystem::into_system(lockstep_check_gate);
        system.initialize(&mut world);
        let _ = system.run((), &mut world);

        let buffer = world.resource::<LockstepInputBuffer>();
        assert_eq!(buffer.confirmed_tick, 1);
        assert!(buffer.advance_allowed);
    }

    #[test]
    fn insert_replaces_existing_input_for_same_tick() {
        let mut buf = LockstepInputBuffer::default();
        assert!(buf.insert(0, sample_input(0, 1)));
        assert!(!buf.insert(0, sample_input(0, 1)));
    }

    #[test]
    fn collect_bootstraps_initial_delayed_ticks() {
        let mut world = World::new();
        world.insert_resource(NetRole::Host);
        world.insert_resource(SimClock { tick: 0 });
        world.insert_resource(Time::<()>::default());
        world.insert_resource(HostNetState::default());
        world.insert_resource(PendingLocalInput::default());
        world.insert_resource(LockstepInputBuffer::default());

        let mut system = IntoSystem::into_system(collect_and_broadcast_local_input);
        system.initialize(&mut world);
        let _ = system.run((), &mut world);

        let buffer = world.resource::<LockstepInputBuffer>();
        assert_eq!(buffer.last_local_tick_sent, Some(DEFAULT_INPUT_DELAY));
        for tick in 1..=DEFAULT_INPUT_DELAY {
            let slot = buffer.pending.get(&tick).expect("missing startup tick");
            let input = slot.get(&0).expect("missing host input");
            assert_eq!(input.tick, tick);
            assert!(input.commands.is_empty());
        }
    }
}
