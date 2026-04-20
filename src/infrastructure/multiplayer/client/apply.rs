//! Re-exports the client-side systems that apply server events (lobby,
//! chat, announcements) and the pending-event/input queues.

pub use crate::infrastructure::multiplayer::client_systems::{
    client_apply_server_events, PendingInputBroadcasts, PendingNetEvents,
};
