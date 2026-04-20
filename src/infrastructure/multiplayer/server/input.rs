//! Re-exports the host-side systems that process incoming client commands
//! and clean up on client disconnects.

pub use crate::infrastructure::multiplayer::host_systems::{
    host_handle_disconnects, host_process_client_commands,
};
