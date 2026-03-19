//! Transport layer — TCP (native) and WebSocket (WASM) framing + background I/O.

#[cfg(not(target_arch = "wasm32"))]
use std::io::{self, Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use game_state::codec;
use game_state::message::{ClientMessage, ServerMessage};
#[cfg(not(target_arch = "wasm32"))]
use serde::Deserialize;
#[cfg(not(target_arch = "wasm32"))]
use socket2::SockRef;

#[cfg(not(target_arch = "wasm32"))]
use super::debug_tap;
use super::NET_TRAFFIC;

// ── Wire format: 4-byte big-endian length prefix + JSON payload ─────────────

#[cfg(not(target_arch = "wasm32"))]
pub fn send_framed(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    let len = data.len() as u32;
    // Combine header + payload into a single buffer to guarantee atomic framing.
    // This prevents any possibility of writing the header without the payload.
    let mut frame = Vec::with_capacity(4 + data.len());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(data);
    stream.write_all(&frame)?;
    stream.flush()
}

#[cfg(not(target_arch = "wasm32"))]
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
                    idle_rounds += 1;
                    if idle_rounds > 1 {
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "no data"));
                    }
                }
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn recv_framed(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    read_exact_timeout(stream, &mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > 16 * 1024 * 1024 {
        // Legacy JSON fallback: detect unframed `{...}` payloads
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
        // Msgpack fallback: detect unframed msgpack payloads (fixmap 0x80-0x8F, fixarray 0x90-0x9F)
        if (len_buf[0] & 0xE0 == 0x80) || len_buf[0] == 0xDE || len_buf[0] == 0xDF
            || len_buf[0] == 0xDC || len_buf[0] == 0xDD
        {
            bevy::log::warn!(
                "Detected unframed msgpack payload on framed socket; recovering \
                (prefix=0x{:02X}{:02X}{:02X}{:02X})",
                len_buf[0], len_buf[1], len_buf[2], len_buf[3]
            );
            let recovered = recv_unframed_msgpack_payload(stream, len_buf)?;
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

    if len == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame with zero length",
        ));
    }

    let mut buf = vec![0u8; len];
    read_exact_timeout(stream, &mut buf)?;
    Ok(buf)
}

/// Recover an unframed msgpack payload from the stream.
/// Reads bytes until we have a complete msgpack value, checked via rmp_serde deserialization.
#[cfg(not(target_arch = "wasm32"))]
fn recv_unframed_msgpack_payload(stream: &mut TcpStream, first4: [u8; 4]) -> io::Result<Vec<u8>> {
    const MAX_UNFRAMED_BYTES: usize = 256 * 1024;
    let mut data = first4.to_vec();
    let mut chunk = [0u8; 4096];

    loop {
        if data.len() > MAX_UNFRAMED_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unframed msgpack payload too large ({} bytes)", data.len()),
            ));
        }

        // Check if we have a complete msgpack value
        if rmp_serde::from_slice::<serde::de::IgnoredAny>(&data).is_ok() {
            bevy::log::info!(
                "Recovered unframed msgpack payload: {} bytes",
                data.len()
            );
            return Ok(data);
        }

        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(e) => return Err(e),
        }
    }

    // Final check
    if rmp_serde::from_slice::<serde::de::IgnoredAny>(&data).is_ok() {
        bevy::log::info!("Recovered unframed msgpack payload: {} bytes", data.len());
        return Ok(data);
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "failed to recover unframed msgpack payload ({} bytes buffered)",
            data.len()
        ),
    ))
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
fn is_complete_json_value(buf: &[u8]) -> bool {
    let mut de = serde_json::Deserializer::from_slice(buf);
    match serde::de::IgnoredAny::deserialize(&mut de) {
        Ok(_) => de.end().is_ok(),
        Err(_) => false,
    }
}

// ── TCP keepalive (native only) ─────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub fn configure_keepalive(stream: &TcpStream) {
    let sock = SockRef::from(stream);
    let keepalive = socket2::TcpKeepalive::new()
        .with_time(Duration::from_secs(15))
        .with_interval(Duration::from_secs(10));
    if let Err(e) = sock.set_tcp_keepalive(&keepalive) {
        bevy::log::warn!("Failed to set TCP keepalive: {}", e);
    }
}

// ── LAN IP detection (native only) ──────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub fn detect_lan_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub struct DetectedIp {
    pub ip: String,
    pub name: String,
    pub is_likely_vpn: bool,
}

#[cfg(not(target_arch = "wasm32"))]
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
    results.sort_by(|a, b| b.is_likely_vpn.cmp(&a.is_likely_vpn).then(a.name.cmp(&b.name)));
    results
}

// ── New client events ────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub struct NewClientEvent {
    pub player_id: u8,
    pub stream: TcpStream,
}

/// Stub for WASM — never instantiated, exists only so HostNetState compiles.
#[cfg(target_arch = "wasm32")]
pub struct NewClientEvent {
    pub player_id: u8,
}

/// WebSocket new client — reader/writer threads already spawned.
pub struct WsNewClientEvent {
    pub player_id: u8,
    pub writer_tx: Sender<Vec<u8>>,
}

fn client_msg_kind(msg: &ClientMessage) -> &'static str {
    match msg {
        ClientMessage::Input { .. } => "input",
        ClientMessage::JoinRequest { .. } => "join",
        ClientMessage::LeaveNotice { .. } => "leave",
        ClientMessage::Ping { .. } => "ping",
        ClientMessage::Reconnect { .. } => "reconnect",
    }
}

fn server_msg_kind(msg: &ServerMessage) -> &'static str {
    match msg {
        ServerMessage::Event { .. } => "event",
        ServerMessage::RelayedInput { .. } => "relayed_input",
        ServerMessage::StateSync { .. } => "state_sync",
        ServerMessage::EntitySpawn { .. } => "entity_spawn",
        ServerMessage::EntityDespawn { .. } => "entity_despawn",
        ServerMessage::BuildingSync { .. } => "building_sync",
        ServerMessage::ResourceSync { .. } => "resource_sync",
        ServerMessage::DayCycleSync { .. } => "day_cycle_sync",
        ServerMessage::WorldBaseline { .. } => "world_baseline",
        ServerMessage::NeutralWorldDelta { .. } => "neutral_world_delta",
        ServerMessage::NeutralWorldDespawn { .. } => "neutral_world_despawn",
        ServerMessage::Pong { .. } => "pong",
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use game_state::message::{ClientMessage, PlayerInput, ServerMessage};
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn tcp_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

    #[test]
    fn send_and_receive_framed_round_trip() {
        let (mut sender, mut receiver) = tcp_pair();
        let payload = b"hello framed world".to_vec();

        send_framed(&mut sender, &payload).unwrap();
        let actual = recv_framed(&mut receiver).unwrap();

        assert_eq!(actual, payload);
    }

    #[test]
    fn recv_framed_recovers_legacy_json_payload() {
        let (mut sender, mut receiver) = tcp_pair();
        let msg = serde_json::to_vec(&ClientMessage::Ping {
            seq: 7,
            timestamp: 42.5,
        })
        .unwrap();

        sender.write_all(&msg).unwrap();
        sender.flush().unwrap();

        let actual = recv_framed(&mut receiver).unwrap();
        assert_eq!(actual, msg);
    }

    #[test]
    fn recv_framed_rejects_oversized_non_json_prefix() {
        let (mut sender, mut receiver) = tcp_pair();
        sender.write_all(&[0xFF, 0xFF, 0xFF, 0xFF]).unwrap();
        sender.flush().unwrap();

        let err = recv_framed(&mut receiver).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("frame too large"));
    }

    #[test]
    fn json_value_detection_requires_complete_value() {
        assert!(is_complete_json_value(br#"{"a":1,"b":[2,3]}"#));
        assert!(!is_complete_json_value(br#"{"a":1"#));
        assert!(!is_complete_json_value(br#"{"a":1} trailing"#));
    }

    #[test]
    fn message_kind_helpers_cover_core_variants() {
        let client_msg = ClientMessage::Input {
            seq: 1,
            timestamp: 0.0,
            input: PlayerInput {
                player_id: 0,
                tick: 1,
                entity_ids: vec![],
                commands: vec![],
            },
        };
        let server_msg = ServerMessage::Pong {
            seq: 9,
            timestamp: 1.25,
        };

        assert_eq!(client_msg_kind(&client_msg), "input");
        assert_eq!(server_msg_kind(&server_msg), "pong");
    }

    #[test]
    fn client_writer_thread_writes_framed_messages() {
        let _guard = super::super::NET_TRAFFIC_TEST_LOCK.lock().unwrap();
        NET_TRAFFIC.bytes_sent.store(0, Ordering::Relaxed);
        NET_TRAFFIC.bytes_received.store(0, Ordering::Relaxed);
        NET_TRAFFIC.msgs_sent.store(0, Ordering::Relaxed);
        NET_TRAFFIC.msgs_received.store(0, Ordering::Relaxed);

        let (stream_tx, mut stream_rx) = tcp_pair();
        let (outgoing_tx, outgoing_rx) = std::sync::mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = shutdown.clone();

        let handle = thread::spawn(move || client_writer_thread(stream_tx, outgoing_rx, thread_shutdown));

        outgoing_tx.send(vec![1, 2, 3, 4]).unwrap();
        let actual = recv_framed(&mut stream_rx).unwrap();
        shutdown.store(true, Ordering::Relaxed);
        drop(outgoing_tx);
        handle.join().unwrap();

        assert_eq!(actual, vec![1, 2, 3, 4]);
        NET_TRAFFIC.bytes_sent.store(0, Ordering::Relaxed);
        NET_TRAFFIC.bytes_received.store(0, Ordering::Relaxed);
        NET_TRAFFIC.msgs_sent.store(0, Ordering::Relaxed);
        NET_TRAFFIC.msgs_received.store(0, Ordering::Relaxed);
    }
}

// ── Host TCP threads (native only) ──────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
pub fn client_writer_thread(
    mut stream: TcpStream,
    outgoing_rx: Receiver<Vec<u8>>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match outgoing_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(data) => {
                let parsed = codec::decode::<ServerMessage>(&data);
                let detail = match &parsed {
                    Ok(msg) => format!("host->client {}", server_msg_kind(msg)),
                    Err(_) => "host->client raw".to_string(),
                };
                let payload = parsed
                    .as_ref()
                    .ok()
                    .map(|msg| codec::to_debug_json(msg))
                    .or_else(|| Some(debug_tap::payload_preview(&data)));

                if let Err(e) = send_framed(&mut stream, &data) {
                    debug_tap::record_error(
                        "host_client_writer",
                        format!("send failed ({} bytes): {}", data.len(), e),
                    );
                    break;
                }
                NET_TRAFFIC.bytes_sent.fetch_add(data.len() as u64 + 4, std::sync::atomic::Ordering::Relaxed);
                NET_TRAFFIC.msgs_sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                debug_tap::record_tx("host_client_writer", detail, data.len(), payload);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
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
                NET_TRAFFIC.bytes_received.fetch_add(data.len() as u64 + 4, std::sync::atomic::Ordering::Relaxed);
                NET_TRAFFIC.msgs_received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                // Try MessagePack first, fall back to JSON for legacy clients
                let parsed = codec::decode::<ClientMessage>(&data)
                    .or_else(|_| serde_json::from_slice::<ClientMessage>(&data).map_err(|e| {
                        rmp_serde::decode::Error::Uncategorized(e.to_string())
                    }));
                match parsed {
                    Ok(msg) => {
                        let detail =
                            format!("player {} -> host {}", player_id, client_msg_kind(&msg));
                        let payload = Some(codec::to_debug_json(&msg));
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

// ── Client TCP threads (native only) ────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
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
                NET_TRAFFIC.bytes_received.fetch_add(data.len() as u64 + 4, std::sync::atomic::Ordering::Relaxed);
                NET_TRAFFIC.msgs_received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                // Try MessagePack first (new protocol), fall back to JSON (legacy)
                let parsed = codec::decode::<ServerMessage>(&data)
                    .or_else(|_| serde_json::from_slice::<ServerMessage>(&data).map_err(|e| {
                        rmp_serde::decode::Error::Uncategorized(e.to_string())
                    }));
                match parsed {
                    Ok(msg) => {
                        let detail = format!("host -> client {}", server_msg_kind(&msg));
                        let payload = Some(codec::to_debug_json(&msg));
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

#[cfg(not(target_arch = "wasm32"))]
pub fn client_writer_thread_fn(
    mut stream: TcpStream,
    outgoing_rx: Receiver<Vec<u8>>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match outgoing_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(data) => {
                let parsed = codec::decode::<ClientMessage>(&data);
                let detail = match &parsed {
                    Ok(msg) => format!("client->host {}", client_msg_kind(msg)),
                    Err(_) => "client->host raw".to_string(),
                };
                let payload = parsed
                    .as_ref()
                    .ok()
                    .map(|msg| codec::to_debug_json(msg))
                    .or_else(|| Some(debug_tap::payload_preview(&data)));

                if let Err(e) = send_framed(&mut stream, &data) {
                    debug_tap::record_error(
                        "client_writer",
                        format!("send failed ({} bytes): {}", data.len(), e),
                    );
                    break;
                }
                NET_TRAFFIC.bytes_sent.fetch_add(data.len() as u64 + 4, std::sync::atomic::Ordering::Relaxed);
                NET_TRAFFIC.msgs_sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                debug_tap::record_tx("client_writer", detail, data.len(), payload);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

// ── Host WebSocket server (native only) ─────────────────────────────────────

/// WebSocket port offset from the TCP port.
#[cfg(not(target_arch = "wasm32"))]
pub const WS_PORT_OFFSET: u16 = 1;

/// Host-side WebSocket listener for WASM browser clients.
/// Accepts WebSocket connections, does the handshake, then runs a combined
/// read/write I/O loop per client on a single thread.
#[cfg(not(target_arch = "wasm32"))]
pub fn ws_host_listener_thread(
    listener: TcpListener,
    cmd_tx: Sender<(u8, ClientMessage)>,
    dc_tx: Sender<u8>,
    ws_client_tx: Sender<WsNewClientEvent>,
    shutdown: Arc<AtomicBool>,
) {
    listener
        .set_nonblocking(true)
        .expect("Failed to set WS listener non-blocking");

    // WS player IDs start at 100 to avoid collision with TCP player IDs
    let mut next_ws_player_id: u8 = 100;

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, addr)) => {
                bevy::log::info!("WebSocket client connecting from {}", addr);
                debug_tap::record_info(
                    "ws_listener",
                    format!("accepted WS {} as player {}", addr, next_ws_player_id),
                );
                stream.set_nodelay(true).ok();
                configure_keepalive(&stream);

                let player_id = next_ws_player_id;
                next_ws_player_id = next_ws_player_id.wrapping_add(1).max(100);

                let cmd_tx = cmd_tx.clone();
                let dc_tx = dc_tx.clone();
                let ws_client_tx = ws_client_tx.clone();
                let shutdown = shutdown.clone();

                // Spawn a handler thread per WS client (handshake is blocking)
                std::thread::spawn(move || {
                    ws_client_handler(stream, cmd_tx, dc_tx, ws_client_tx, player_id, shutdown);
                });
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                if !shutdown.load(Ordering::Relaxed) {
                    bevy::log::warn!("WS listener error: {}", e);
                    debug_tap::record_error("ws_listener", format!("accept error: {}", e));
                }
                break;
            }
        }
    }
}

/// Handles one WebSocket client: handshake then combined read/write I/O loop.
#[cfg(not(target_arch = "wasm32"))]
fn ws_client_handler(
    stream: TcpStream,
    cmd_tx: Sender<(u8, ClientMessage)>,
    dc_tx: Sender<u8>,
    ws_client_tx: Sender<WsNewClientEvent>,
    player_id: u8,
    shutdown: Arc<AtomicBool>,
) {
    // Blocking handshake (stream is not yet non-blocking at this point)
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .ok();

    let mut ws = match tungstenite::accept(stream) {
        Ok(ws) => {
            bevy::log::info!("WebSocket handshake completed for player {}", player_id);
            debug_tap::record_info(
                "ws_handler",
                format!("player {} WS handshake OK", player_id),
            );
            ws
        }
        Err(e) => {
            bevy::log::warn!("WebSocket handshake failed for player {}: {}", player_id, e);
            debug_tap::record_error(
                "ws_handler",
                format!("player {} WS handshake failed: {}", player_id, e),
            );
            return;
        }
    };

    // Set short read timeout for the I/O loop
    ws.get_mut()
        .set_read_timeout(Some(Duration::from_millis(50)))
        .ok();

    // Create the writer channel and notify the host about this new WS client
    let (writer_tx, writer_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let _ = ws_client_tx.send(WsNewClientEvent {
        player_id,
        writer_tx,
    });

    // Combined read/write loop on a single thread
    while !shutdown.load(Ordering::Relaxed) {
        // ── Read ── (accept both Binary/msgpack and Text/JSON)
        match ws.read() {
            Ok(tungstenite::Message::Binary(data)) => {
                NET_TRAFFIC.bytes_received.fetch_add(data.len() as u64, std::sync::atomic::Ordering::Relaxed);
                NET_TRAFFIC.msgs_received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                match codec::decode::<ClientMessage>(&data) {
                    Ok(msg) => {
                        debug_tap::record_rx(
                            "ws_reader",
                            format!("player {} -> host ws {}", player_id, client_msg_kind(&msg)),
                            data.len(),
                            Some(codec::to_debug_json(&msg)),
                        );
                        if cmd_tx.send((player_id, msg)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        debug_tap::record_error(
                            "ws_reader",
                            format!("player {} invalid ws binary msg: {}", player_id, e),
                        );
                    }
                }
            }
            Ok(tungstenite::Message::Text(text)) => {
                // Legacy JSON fallback for older WASM clients
                NET_TRAFFIC.bytes_received.fetch_add(text.len() as u64, std::sync::atomic::Ordering::Relaxed);
                NET_TRAFFIC.msgs_received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(msg) => {
                        debug_tap::record_rx(
                            "ws_reader",
                            format!("player {} -> host ws(json) {}", player_id, client_msg_kind(&msg)),
                            text.len(),
                            Some(text.to_string()),
                        );
                        if cmd_tx.send((player_id, msg)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        debug_tap::record_error(
                            "ws_reader",
                            format!("player {} invalid ws text msg: {}", player_id, e),
                        );
                    }
                }
            }
            Ok(tungstenite::Message::Close(_)) => {
                bevy::log::info!("WS player {} sent close frame", player_id);
                break;
            }
            Ok(tungstenite::Message::Ping(data)) => {
                let _ = ws.send(tungstenite::Message::Pong(data));
            }
            Ok(_) => {} // Pong, Frame — ignore
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut =>
            {
                // No data available — fall through to write
            }
            Err(
                tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed,
            ) => {
                break;
            }
            Err(e) => {
                bevy::log::warn!("WS player {} read error: {}", player_id, e);
                debug_tap::record_error(
                    "ws_reader",
                    format!("player {} ws read error: {}", player_id, e),
                );
                break;
            }
        }

        // ── Write — drain outgoing queue (binary msgpack frames) ──
        while let Ok(data) = writer_rx.try_recv() {
            let data_len = data.len() as u64;
            if ws.send(tungstenite::Message::Binary(data.into())).is_err() {
                break;
            }
            NET_TRAFFIC.bytes_sent.fetch_add(data_len, std::sync::atomic::Ordering::Relaxed);
            NET_TRAFFIC.msgs_sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    debug_tap::record_info("ws_handler", format!("player {} WS disconnected", player_id));
    let _ = dc_tx.send(player_id);
}

// ── WASM WebSocket client ───────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
fn wasm_join_request_bytes(player_name: &str) -> Result<Vec<u8>, String> {
    let join = ClientMessage::JoinRequest {
        seq: 0,
        timestamp: 0.0,
        player_name: player_name.to_string(),
        preferred_faction_index: None,
    };
    game_state::codec::encode(&join).map_err(|err| err.to_string())
}

#[cfg(target_arch = "wasm32")]
fn wasm_decode_server_payload(data: &JsValue) -> Result<ServerMessage, String> {
    if let Some(abuf) = data.dyn_ref::<js_sys::ArrayBuffer>() {
        let arr = js_sys::Uint8Array::new(abuf);
        let bytes = arr.to_vec();
        game_state::codec::decode::<ServerMessage>(&bytes).map_err(|err| err.to_string())
    } else if let Some(text) = data.as_string() {
        serde_json::from_str::<ServerMessage>(&text).map_err(|err| err.to_string())
    } else {
        Err("unsupported websocket payload type".to_string())
    }
}

/// Connect to a host via WebSocket from a WASM browser client.
/// Returns the WebSocket object on success. The `incoming_tx` channel will
/// receive parsed `ServerMessage`s from the `onmessage` callback.
/// The `shutdown` flag is set on error/close.
#[cfg(target_arch = "wasm32")]
pub fn wasm_ws_connect(
    url: &str,
    incoming_tx: Sender<ServerMessage>,
    shutdown: Arc<AtomicBool>,
    player_name: String,
) -> Result<web_sys::WebSocket, String> {
    let ws = web_sys::WebSocket::new(url).map_err(|e| format!("{:?}", e))?;
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

    // onopen — send JoinRequest once connected (as binary msgpack)
    let ws_for_open = ws.clone();
    let onopen = Closure::wrap(Box::new(move |_: JsValue| {
        bevy::log::info!("WebSocket connected to host");
        if let Ok(bytes) = wasm_join_request_bytes(&player_name) {
            let arr = js_sys::Uint8Array::from(bytes.as_slice());
            let _ = ws_for_open.send_with_array_buffer(&arr.buffer());
        }
    }) as Box<dyn FnMut(JsValue)>);
    ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();

    // onmessage — parse binary msgpack (with JSON text fallback)
    let onmessage = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
        let data = e.data();
        match wasm_decode_server_payload(&data) {
            Ok(msg) => {
                let _ = incoming_tx.send(msg);
            }
            Err(err) => {
                bevy::log::warn!("WS payload parse error: {}", err);
            }
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    // onerror
    let shutdown_err = shutdown.clone();
    let onerror = Closure::wrap(Box::new(move |_: web_sys::ErrorEvent| {
        bevy::log::warn!("WebSocket error");
        shutdown_err.store(true, Ordering::Relaxed);
    }) as Box<dyn FnMut(web_sys::ErrorEvent)>);
    ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    onerror.forget();

    // onclose
    let shutdown_close = shutdown.clone();
    let onclose = Closure::wrap(Box::new(move |_: web_sys::CloseEvent| {
        bevy::log::info!("WebSocket closed");
        shutdown_close.store(true, Ordering::Relaxed);
    }) as Box<dyn FnMut(web_sys::CloseEvent)>);
    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();

    Ok(ws)
}

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    use super::*;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_test::*;

    fn sample_server_message() -> ServerMessage {
        ServerMessage::Pong {
            seq: 42,
            timestamp: 123.5,
        }
    }

    #[wasm_bindgen_test]
    fn wasm_join_request_bytes_encode_expected_message() {
        let bytes = wasm_join_request_bytes("Commander").unwrap();
        let decoded: ClientMessage = game_state::codec::decode(&bytes).unwrap();
        assert_eq!(
            decoded,
            ClientMessage::JoinRequest {
                seq: 0,
                timestamp: 0.0,
                player_name: "Commander".to_string(),
                preferred_faction_index: None,
            }
        );
    }

    #[wasm_bindgen_test]
    fn wasm_decode_server_payload_accepts_binary_msgpack() {
        let msg = sample_server_message();
        let bytes = game_state::codec::encode(&msg).unwrap();
        let array = js_sys::Uint8Array::from(bytes.as_slice());
        let payload = JsValue::from(array.buffer());

        let decoded = wasm_decode_server_payload(&payload).unwrap();
        assert_eq!(decoded, msg);
    }

    #[wasm_bindgen_test]
    fn wasm_decode_server_payload_accepts_legacy_json_text() {
        let msg = sample_server_message();
        let text = serde_json::to_string(&msg).unwrap();

        let decoded = wasm_decode_server_payload(&JsValue::from_str(&text)).unwrap();
        assert_eq!(decoded, msg);
    }

    #[wasm_bindgen_test]
    fn wasm_decode_server_payload_rejects_invalid_payloads() {
        let invalid_binary = JsValue::from(js_sys::Uint8Array::from(&[1u8, 2, 3][..]).buffer());
        assert!(wasm_decode_server_payload(&invalid_binary).is_err());

        let invalid_text = JsValue::from_str("{not valid json");
        assert!(wasm_decode_server_payload(&invalid_text).is_err());

        assert!(wasm_decode_server_payload(&JsValue::TRUE).is_err());
    }
}
