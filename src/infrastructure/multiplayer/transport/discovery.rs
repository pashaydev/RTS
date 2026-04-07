#[cfg(not(target_arch = "wasm32"))]
use std::io;
#[cfg(not(target_arch = "wasm32"))]
use std::net::UdpSocket;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
pub const DISCOVERY_PORT: u16 = 7877;

#[cfg(not(target_arch = "wasm32"))]
const DISCOVERY_MAGIC: &str = "rts-lan-discovery-v1";

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum DiscoveryMessage {
    Query {
        magic: String,
    },
    Announce {
        magic: String,
        name: String,
        session_code: String,
    },
}

#[cfg(not(target_arch = "wasm32"))]
pub fn discovery_listener_thread(
    socket: UdpSocket,
    host_name: String,
    session_code: String,
    shutdown: Arc<AtomicBool>,
) {
    let _ = socket.set_read_timeout(Some(Duration::from_millis(250)));
    let mut buf = [0u8; 1024];
    while !shutdown.load(Ordering::Relaxed) {
        match socket.recv_from(&mut buf) {
            Ok((len, addr)) => {
                let Ok(msg) = serde_json::from_slice::<DiscoveryMessage>(&buf[..len]) else {
                    continue;
                };
                let DiscoveryMessage::Query { magic } = msg else {
                    continue;
                };
                if magic != DISCOVERY_MAGIC {
                    continue;
                }

                let reply = DiscoveryMessage::Announce {
                    magic: DISCOVERY_MAGIC.to_string(),
                    name: host_name.clone(),
                    session_code: session_code.clone(),
                };
                if let Ok(data) = serde_json::to_vec(&reply) {
                    let _ = socket.send_to(&data, addr);
                }
            }
            Err(ref e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(e) => {
                bevy::log::warn!("LAN discovery listener stopped: {}", e);
                break;
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_broadcast_targets() -> Vec<std::net::SocketAddr> {
    let mut targets = std::collections::BTreeSet::new();
    targets.insert(std::net::SocketAddr::from((
        [255, 255, 255, 255],
        DISCOVERY_PORT,
    )));

    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            let ip = iface.addr.ip();
            let std::net::IpAddr::V4(ipv4) = ip else {
                continue;
            };
            let octets = ipv4.octets();
            targets.insert(std::net::SocketAddr::from((
                [octets[0], octets[1], octets[2], 255],
                DISCOVERY_PORT,
            )));
        }
    }

    targets.into_iter().collect()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn discover_lan_hosts(timeout: Duration) -> Vec<(String, String)> {
    let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
        return Vec::new();
    };
    if socket.set_broadcast(true).is_err() {
        return Vec::new();
    }
    if socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .is_err()
    {
        return Vec::new();
    }

    let query = DiscoveryMessage::Query {
        magic: DISCOVERY_MAGIC.to_string(),
    };
    let Ok(payload) = serde_json::to_vec(&query) else {
        return Vec::new();
    };

    for target in detect_broadcast_targets() {
        let _ = socket.send_to(&payload, target);
    }

    let start = std::time::Instant::now();
    let mut buf = [0u8; 1024];
    let mut hosts = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    while start.elapsed() < timeout {
        match socket.recv_from(&mut buf) {
            Ok((len, _addr)) => {
                let Ok(msg) = serde_json::from_slice::<DiscoveryMessage>(&buf[..len]) else {
                    continue;
                };
                let DiscoveryMessage::Announce {
                    magic,
                    name,
                    session_code,
                } = msg
                else {
                    continue;
                };
                if magic != DISCOVERY_MAGIC || !seen.insert(session_code.clone()) {
                    continue;
                }
                hosts.push((name, session_code));
            }
            Err(ref e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(_) => break,
        }
    }

    hosts.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    hosts
}
