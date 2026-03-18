//! TCP transport layer — length-prefixed framing and background I/O threads.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use game_state::message::{ClientMessage, ServerMessage};
use serde::Deserialize;
use socket2::SockRef;

use super::debug_tap;

// ── Wire format: 4-byte big-endian length prefix + JSON payload ─────────────

pub fn send_framed(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    let len = data.len() as u32;
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(data)?;
    stream.flush()
}

/// Timeout-safe read that retries on WouldBlock/TimedOut/Interrupted without
/// losing partial data. Returns `TimedOut` if no progress is made at all
/// (caller should check shutdown and retry), or a real error if the connection
/// is broken.
fn read_exact_timeout(stream: &mut TcpStream, buf: &mut [u8]) -> io::Result<()> {
    let mut offset = 0;
    let mut idle_rounds = 0;
    while offset < buf.len() {
        match stream.read(&mut buf[offset..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "connection closed",
                ));
            }
            Ok(n) => {
                offset += n;
                idle_rounds = 0;
            }
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut
                    || e.kind() == io::ErrorKind::Interrupted =>
            {
                if offset == 0 {
                    // No data read at all — signal caller to check shutdown
                    idle_rounds += 1;
                    if idle_rounds > 1 {
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "no data"));
                    }
                }
                // Partial data read — must keep going to avoid protocol corruption
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Receive a length-prefixed frame. Returns:
/// - `Ok(bytes)` on success
/// - `Err(TimedOut)` if no data available (caller should check shutdown)
/// - `Err(other)` on connection error
pub fn recv_framed(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    read_exact_timeout(stream, &mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;

    // Sanity check: reject frames > 16 MB
    if len > 16 * 1024 * 1024 {
        if len_buf[0] == b'{' {
            bevy::log::warn!(
                "Detected unframed JSON payload on framed socket; using legacy fallback"
            );
            let recovered = recv_legacy_json_payload(stream, len_buf)?;
            debug_tap::record_rx(
                "transport_legacy",
                "recovered unframed JSON payload".to_string(),
                recovered.len(),
                Some(debug_tap::payload_preview(&recovered)),
            );
            return Ok(recovered);
        }
        let ascii = len_buf
            .iter()
            .map(|b| {
                let c = *b as char;
                if c.is_ascii_graphic() || c == ' ' {
                    c
                } else {
                    '.'
                }
            })
            .collect::<String>();
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "frame too large: {} bytes (prefix=0x{:02X}{:02X}{:02X}{:02X} ascii='{}')",
                len, len_buf[0], len_buf[1], len_buf[2], len_buf[3], ascii
            ),
        ));
    }

    let mut buf = vec![0u8; len];
    read_exact_timeout(stream, &mut buf)?;
    Ok(buf)
}

/// Compatibility fallback for peers that accidentally send raw JSON without a
/// length prefix. This path should be rare and is bounded to avoid unbounded
/// reads on malformed data.
fn recv_legacy_json_payload(stream: &mut TcpStream, first4: [u8; 4]) -> io::Result<Vec<u8>> {
    const MAX_LEGACY_JSON_BYTES: usize = 64 * 1024;
    let mut data = first4.to_vec();
    let mut chunk = [0u8; 1024];

    while data.len() <= MAX_LEGACY_JSON_BYTES {
        if is_complete_json_value(&data) {
            return Ok(data);
        }
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut
                    || e.kind() == io::ErrorKind::Interrupted =>
            {
                if is_complete_json_value(&data) {
                    return Ok(data);
                }
                break;
            }
            Err(e) => return Err(e),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "failed to recover unframed JSON payload ({} bytes buffered)",
            data.len()
        ),
    ))
}

fn is_complete_json_value(buf: &[u8]) -> bool {
    let mut de = serde_json::Deserializer::from_slice(buf);
    match serde::de::IgnoredAny::deserialize(&mut de) {
        Ok(_) => de.end().is_ok(),
        Err(_) => false,
    }
}

// ── TCP keepalive ────────────────────────────────────────────────────────────

/// Enable TCP keepalive on a stream so VPN/Hamachi tunnels don't silently drop.
/// Sends a keepalive probe every 10 seconds after 15 seconds of idle.
pub fn configure_keepalive(stream: &TcpStream) {
    let sock = SockRef::from(stream);
    let keepalive = socket2::TcpKeepalive::new()
        .with_time(Duration::from_secs(15))
        .with_interval(Duration::from_secs(10));
    if let Err(e) = sock.set_tcp_keepalive(&keepalive) {
        bevy::log::warn!("Failed to set TCP keepalive: {}", e);
    }
}

// ── LAN IP detection ────────────────────────────────────────────────────────

/// Detect local LAN IP by querying the OS routing table (no packets sent).
pub fn detect_lan_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

/// Detected network interface with its IP address and a human-readable label.
#[derive(Debug, Clone)]
pub struct DetectedIp {
    pub ip: String,
    pub name: String,
    pub is_likely_vpn: bool,
}

/// Enumerate all non-loopback IPv4 addresses across all network interfaces.
/// Flags interfaces that look like VPN/Hamachi adapters (name or IP range heuristics).
pub fn detect_all_ips() -> Vec<DetectedIp> {
    let mut results = Vec::new();
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            let addr = iface.addr.ip();
            if !addr.is_ipv4() {
                continue;
            }
            let ip = addr.to_string();
            let name_lower = iface.name.to_lowercase();
            // Heuristics for VPN/virtual adapters:
            // - Hamachi uses 25.x.x.x or 5.x.x.x ranges and adapters named "ham0", "Hamachi"
            // - ZeroTier uses "zt*" interface names
            // - OpenVPN uses "tun*", "tap*"
            // - WireGuard uses "wg*"
            // - Generic VPN adapters often use 10.x.x.x ranges on virtual interfaces
            let is_likely_vpn = name_lower.contains("ham")
                || name_lower.contains("tun")
                || name_lower.contains("tap")
                || name_lower.starts_with("zt")
                || name_lower.starts_with("wg")
                || name_lower.contains("vpn")
                || ip.starts_with("25.")
                || ip.starts_with("5.");
            results.push(DetectedIp {
                ip,
                name: iface.name.clone(),
                is_likely_vpn,
            });
        }
    }
    // Sort: VPN adapters first (more relevant for remote play), then by name
    results.sort_by(|a, b| b.is_likely_vpn.cmp(&a.is_likely_vpn).then(a.name.cmp(&b.name)));
    results
}

// ── New client event ────────────────────────────────────────────────────────

pub struct NewClientEvent {
    pub player_id: u8,
    pub stream: TcpStream,
}

fn client_msg_kind(msg: &ClientMessage) -> &'static str {
    match msg {
        ClientMessage::Input { .. } => "input",
        ClientMessage::JoinRequest { .. } => "join",
        ClientMessage::LeaveNotice { .. } => "leave",
        ClientMessage::Ping { .. } => "ping",
    }
}

fn server_msg_kind(msg: &ServerMessage) -> &'static str {
    match msg {
        ServerMessage::Event { .. } => "event",
        ServerMessage::RelayedInput { .. } => "relayed_input",
        ServerMessage::StateSync { .. } => "state_sync",
        ServerMessage::EntitySpawn { .. } => "entity_spawn",
        ServerMessage::EntityDespawn { .. } => "entity_despawn",
        ServerMessage::Pong { .. } => "pong",
    }
}

// ── Host threads ────────────────────────────────────────────────────────────

/// Accepts incoming TCP connections and sends them on `new_client_tx`.
pub fn host_listener_thread(
    listener: TcpListener,
    new_client_tx: Sender<NewClientEvent>,
    shutdown: Arc<AtomicBool>,
) {
    listener
        .set_nonblocking(true)
        .expect("Failed to set listener non-blocking");

    let mut next_player_id: u8 = 1; // 0 is host

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, addr)) => {
                bevy::log::info!("New client connected from {}", addr);
                debug_tap::record_info(
                    "host_listener",
                    format!("accepted {} as player {}", addr, next_player_id),
                );
                stream.set_nodelay(true).ok();
                configure_keepalive(&stream);
                let _ = new_client_tx.send(NewClientEvent {
                    player_id: next_player_id,
                    stream,
                });
                next_player_id = next_player_id.wrapping_add(1).max(1);
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                if !shutdown.load(Ordering::Relaxed) {
                    bevy::log::warn!("Listener error: {}", e);
                    debug_tap::record_error("host_listener", format!("accept error: {}", e));
                }
                break;
            }
        }
    }
}

/// Pulls serialized bytes from `outgoing_rx` and writes them to the client socket.
pub fn client_writer_thread(
    mut stream: TcpStream,
    outgoing_rx: Receiver<Vec<u8>>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match outgoing_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(data) => {
                let parsed = serde_json::from_slice::<ServerMessage>(&data);
                let detail = match &parsed {
                    Ok(msg) => format!("host->client {}", server_msg_kind(msg)),
                    Err(_) => "host->client raw".to_string(),
                };
                let payload = parsed
                    .as_ref()
                    .ok()
                    .and_then(|msg| serde_json::to_string(msg).ok())
                    .or_else(|| Some(debug_tap::payload_preview(&data)));

                if let Err(e) = send_framed(&mut stream, &data) {
                    debug_tap::record_error(
                        "host_client_writer",
                        format!("send failed ({} bytes): {}", data.len(), e),
                    );
                    break;
                }
                debug_tap::record_tx("host_client_writer", detail, data.len(), payload);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Reads `ClientMessage` frames from a client socket and sends them on `incoming_tx`.
pub fn host_client_reader_thread(
    mut stream: TcpStream,
    incoming_tx: Sender<(u8, ClientMessage)>,
    disconnect_tx: Sender<u8>,
    player_id: u8,
    shutdown: Arc<AtomicBool>,
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .ok();

    while !shutdown.load(Ordering::Relaxed) {
        match recv_framed(&mut stream) {
            Ok(data) => {
                match serde_json::from_slice::<ClientMessage>(&data) {
                    Ok(msg) => {
                        let detail =
                            format!("player {} -> host {}", player_id, client_msg_kind(&msg));
                        let payload =
                            serde_json::to_string(&msg).ok().or_else(|| Some(debug_tap::payload_preview(&data)));
                        debug_tap::record_rx(
                            "host_client_reader",
                            detail,
                            data.len(),
                            payload,
                        );
                        if incoming_tx.send((player_id, msg)).is_err() {
                            debug_tap::record_error(
                                "host_client_reader",
                                format!("incoming channel closed for player {}", player_id),
                            );
                            break;
                        }
                    }
                    Err(e) => {
                        debug_tap::record_error(
                            "host_client_reader",
                            format!(
                                "invalid client message from player {}: {}",
                                player_id, e
                            ),
                        );
                        debug_tap::record_rx(
                            "host_client_reader",
                            format!("player {} -> host raw_invalid", player_id),
                            data.len(),
                            Some(debug_tap::payload_preview(&data)),
                        );
                    }
                }
            }
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) => {
                bevy::log::warn!("Client {} reader error: {}", player_id, e);
                debug_tap::record_error(
                    "host_client_reader",
                    format!("player {} reader error: {}", player_id, e),
                );
                break;
            }
        }
    }

    debug_tap::record_info("host_client_reader", format!("player {} disconnected", player_id));
    let _ = disconnect_tx.send(player_id);
}

// ── Client threads ──────────────────────────────────────────────────────────

/// Client-side: reads `ServerMessage` frames from the host.
pub fn client_reader_thread(
    mut stream: TcpStream,
    incoming_tx: Sender<ServerMessage>,
    shutdown: Arc<AtomicBool>,
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .ok();

    while !shutdown.load(Ordering::Relaxed) {
        match recv_framed(&mut stream) {
            Ok(data) => {
                match serde_json::from_slice::<ServerMessage>(&data) {
                    Ok(msg) => {
                        let detail = format!("host -> client {}", server_msg_kind(&msg));
                        let payload =
                            serde_json::to_string(&msg).ok().or_else(|| Some(debug_tap::payload_preview(&data)));
                        debug_tap::record_rx("client_reader", detail, data.len(), payload);
                        if incoming_tx.send(msg).is_err() {
                            debug_tap::record_error(
                                "client_reader",
                                "incoming channel closed".to_string(),
                            );
                            break;
                        }
                    }
                    Err(e) => {
                        debug_tap::record_error(
                            "client_reader",
                            format!("invalid server message: {}", e),
                        );
                        debug_tap::record_rx(
                            "client_reader",
                            "host -> client raw_invalid".to_string(),
                            data.len(),
                            Some(debug_tap::payload_preview(&data)),
                        );
                    }
                }
            }
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) => {
                bevy::log::warn!("Client reader error: {}", e);
                debug_tap::record_error("client_reader", format!("reader error: {}", e));
                break;
            }
        }
    }
}

/// Client-side: sends serialized `ClientMessage` bytes to the host.
pub fn client_writer_thread_fn(
    mut stream: TcpStream,
    outgoing_rx: Receiver<Vec<u8>>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match outgoing_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(data) => {
                let parsed = serde_json::from_slice::<ClientMessage>(&data);
                let detail = match &parsed {
                    Ok(msg) => format!("client->host {}", client_msg_kind(msg)),
                    Err(_) => "client->host raw".to_string(),
                };
                let payload = parsed
                    .as_ref()
                    .ok()
                    .and_then(|msg| serde_json::to_string(msg).ok())
                    .or_else(|| Some(debug_tap::payload_preview(&data)));

                if let Err(e) = send_framed(&mut stream, &data) {
                    debug_tap::record_error(
                        "client_writer",
                        format!("send failed ({} bytes): {}", data.len(), e),
                    );
                    break;
                }
                debug_tap::record_tx("client_writer", detail, data.len(), payload);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}
