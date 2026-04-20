//! Re-exports the client-side systems that drain incoming server commands,
//! heartbeat the host, and detect disconnects.

pub use crate::infrastructure::multiplayer::client_systems::{
    client_handle_disconnect, client_receive_commands, client_send_ping, ClientPingTimer,
};
