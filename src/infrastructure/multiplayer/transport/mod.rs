//! Transport layer — Matchbox WebRTC re-exports + LAN discovery + IP detection.

mod discovery;
mod ip;

pub use super::matchbox_transport::*;
pub use discovery::{discover_lan_hosts, discovery_listener_thread, DISCOVERY_PORT};
#[allow(unused_imports)]
pub use ip::DetectedIp;
pub use ip::{detect_all_ips, detect_lan_ip};
