//! Client-side systems: receive relayed commands from host, handle disconnect.

use bevy::prelude::*;
use std::sync::atomic::Ordering;
use std::sync::mpsc::TryRecvError;

use game_state::message::ServerMessage;

use crate::components::*;
use crate::net_bridge::EntityNetMap;

use super::{ClientNetState, NetRole};
use super::host_systems::execute_input_command;

/// Polls incoming `ServerMessage`s from the host and applies relayed commands.
pub fn client_receive_commands(
    client: Res<ClientNetState>,
    net_map: Res<EntityNetMap>,
    mut unit_states: Query<&mut UnitState>,
) {
    let rx = client.incoming.lock().unwrap();
    for _ in 0..64 {
        match rx.try_recv() {
            Ok(msg) => match &msg {
                ServerMessage::RelayedInput { input, .. } => {
                    execute_input_command(input, &net_map, &mut unit_states);
                }
                ServerMessage::Event { events, .. } => {
                    for event in events {
                        if let game_state::message::GameEvent::Announcement { text } = event {
                            info!("Server announcement: {}", text);
                        }
                    }
                }
            },
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                client.shutdown.store(true, Ordering::Relaxed);
                break;
            }
        }
    }
}

/// Detect host disconnect and return to main menu.
pub fn client_handle_disconnect(
    client: Res<ClientNetState>,
    mut next_state: ResMut<NextState<AppState>>,
    mut net_role: ResMut<NetRole>,
) {
    if client.shutdown.load(Ordering::Relaxed) {
        warn!("Host disconnected — returning to main menu");
        *net_role = NetRole::Offline;
        next_state.set(AppState::MainMenu);
    }
}
