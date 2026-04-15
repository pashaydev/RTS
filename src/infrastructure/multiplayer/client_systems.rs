//! Client-side systems: drain incoming server messages into pending
//! queues, apply lobby/chat/announcement events, send keepalive pings,
//! and detect host disconnects.

use bevy::prelude::*;
use bevy_matchbox::prelude::*;
use std::sync::atomic::Ordering;

use game_state::message::{GameEvent, PlayerInput, ServerMessage};

use crate::types::*;

use super::debug_tap;
use super::transport::{self, MatchboxInbox};
use super::{ClientNetState, NetRole, NetStats};
use crate::ui::event_log_widget::{EventCategory, GameEventLog, LogLevel};

/// Timer for sending periodic pings to the host (keeps VPN/Hamachi tunnels alive
/// and feeds the RTT estimator).
#[derive(Resource)]
pub struct ClientPingTimer(pub Timer);

impl Default for ClientPingTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(5.0, TimerMode::Repeating))
    }
}

/// Lockstep: `ServerMessage::InputBroadcast` messages drained from the inbox
/// and waiting to be inserted into the `LockstepInputBuffer` by
/// `client_receive_remote_inputs`.
#[derive(Resource, Default)]
pub struct PendingInputBroadcasts {
    /// `(player_id, input)` tuples in arrival order.
    pub inputs: Vec<(u8, PlayerInput)>,
}

/// Lobby / chat / announcement events queued by the receiver for
/// `client_apply_server_events` to consume.
#[derive(Resource, Default)]
pub struct PendingNetEvents {
    pub events: Vec<GameEvent>,
}

/// Polls incoming `ServerMessage`s from the host and stages them for
/// follow-up apply systems (input broadcasts, lobby events, ping replies,
/// checksum reports).
pub fn client_receive_commands(
    client: Res<ClientNetState>,
    mut inbox: ResMut<MatchboxInbox>,
    mut pending_input_broadcasts: ResMut<PendingInputBroadcasts>,
    mut pending_events: ResMut<PendingNetEvents>,
    mut pending_checksums: ResMut<super::checksum::PendingChecksumReports>,
    mut net_stats: ResMut<NetStats>,
    time: Res<Time>,
) {
    if !inbox.disconnected.is_empty() {
        inbox.disconnected.clear();
        client.disconnected.store(true, Ordering::Relaxed);
    }

    let messages = std::mem::take(&mut inbox.server_messages);
    for msg in &messages {
        match msg {
            ServerMessage::InputBroadcast {
                player_id, input, ..
            } => {
                pending_input_broadcasts
                    .inputs
                    .push((*player_id, input.clone()));
            }
            ServerMessage::Pong { .. } => {
                let rtt =
                    ((time.elapsed_secs_f64() - net_stats.last_ping_sent_at) * 1000.0) as f32;
                if rtt.is_finite() && rtt >= 0.0 {
                    net_stats.rtt_ms = rtt;
                    if net_stats.rtt_smoothed_ms == 0.0 {
                        net_stats.rtt_smoothed_ms = rtt;
                    } else {
                        net_stats.rtt_smoothed_ms = net_stats.rtt_smoothed_ms * 0.8 + rtt * 0.2;
                    }
                }
            }
            ServerMessage::Event { events, .. } => {
                pending_events.events.extend(events.iter().cloned());
            }
            ServerMessage::ChecksumReport {
                player_id,
                tick,
                checksum,
                ..
            } => {
                pending_checksums
                    .reports
                    .push((*player_id, *tick, *checksum));
            }
        }
    }
}

/// Apply chat / announcement / host-shutdown events queued by
/// `client_receive_commands`. Victory and elimination are detected
/// independently on every peer by the deterministic simulation, so those
/// don't round-trip through the wire anymore.
pub fn client_apply_server_events(
    client: Res<ClientNetState>,
    mut pending_events: ResMut<PendingNetEvents>,
    mut event_log: ResMut<GameEventLog>,
    time: Res<Time>,
) {
    let events = std::mem::take(&mut pending_events.events);
    for event in &events {
        match event {
            GameEvent::Announcement { text } => {
                info!("Server announcement: {}", text);
                debug_tap::record_info("client_game_events", format!("announcement: {}", text));
                event_log.push_with_level(
                    time.elapsed_secs(),
                    text.clone(),
                    EventCategory::Network,
                    LogLevel::Info,
                    None,
                    None,
                );
            }
            GameEvent::HostShutdown { reason } => {
                warn!("Host ended match: {}", reason);
                debug_tap::record_info("client_game_events", format!("host_shutdown: {}", reason));
                event_log.push_with_level(
                    time.elapsed_secs(),
                    format!("Host ended match: {}", reason),
                    EventCategory::Network,
                    LogLevel::Error,
                    None,
                    None,
                );
                client.disconnected.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

/// Detect host disconnect and return to main menu.
pub fn client_handle_disconnect(
    client: Res<ClientNetState>,
    mut next_state: ResMut<NextState<AppState>>,
    mut net_role: ResMut<NetRole>,
    mut event_log: ResMut<GameEventLog>,
    time: Res<Time>,
    mut victory_state: Option<ResMut<crate::simulation::victory::VictoryState>>,
) {
    if client.disconnected.load(Ordering::Relaxed) {
        warn!("Host disconnected — returning to main menu");
        debug_tap::record_info("client_state", "host disconnected -> main menu");
        event_log.push_with_level(
            time.elapsed_secs(),
            "Host disconnected — returning to menu".to_string(),
            EventCategory::Network,
            LogLevel::Error,
            None,
            None,
        );
        if let Some(ref mut victory_state) = victory_state {
            **victory_state = crate::simulation::victory::VictoryState::default();
        }
        *net_role = NetRole::Offline;
        next_state.set(AppState::MainMenu);
    }
}

/// Periodically send Ping to the host to keep connections alive and measure RTT.
pub fn client_send_ping(
    client: Res<ClientNetState>,
    mut socket: ResMut<MatchboxSocket>,
    time: Res<Time>,
    mut ping_timer: ResMut<ClientPingTimer>,
    mut net_stats: ResMut<NetStats>,
) {
    ping_timer.0.tick(time.delta());
    if !ping_timer.0.just_finished() {
        return;
    }
    let seq = {
        let mut s = client.seq.lock().unwrap();
        *s += 1;
        *s
    };
    net_stats.last_ping_sent_at = time.elapsed_secs_f64();
    let ping = game_state::message::ClientMessage::Ping {
        seq,
        timestamp: time.elapsed_secs_f64(),
    };
    transport::send_to_host(&mut socket, &ping);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AppState;
    use crate::ui::event_log_widget::GameEventLog;

    #[test]
    fn client_handle_disconnect_returns_to_menu_and_clears_role() {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.init_state::<AppState>();
        let client = ClientNetState {
            player_id: 1,
            seat_index: 0,
            my_faction: Faction::Player2,
            color_index: 1,
            ..Default::default()
        };
        client.disconnected.store(true, Ordering::Relaxed);
        app.insert_resource(client);
        app.insert_resource(NetRole::Client);
        app.insert_resource(GameEventLog::default());
        app.insert_resource(Time::<()>::default());
        let mut victory_state = crate::simulation::victory::VictoryState::default();
        victory_state.game_over = true;
        victory_state.overlay_spawned = true;
        app.insert_resource(victory_state);
        app.add_systems(Update, client_handle_disconnect);

        app.update();

        assert_eq!(*app.world().resource::<NetRole>(), NetRole::Offline);
        assert_eq!(
            app.world().resource::<State<AppState>>().get(),
            &AppState::MainMenu
        );
        assert_eq!(app.world().resource::<GameEventLog>().entries.len(), 1);
        let victory_state = app
            .world()
            .resource::<crate::simulation::victory::VictoryState>();
        assert!(!victory_state.game_over);
        assert!(!victory_state.overlay_spawned);
    }
}
