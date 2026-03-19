//! Transport layer — TCP (native) and WebSocket (WASM) framing + background I/O.

#[cfg(not(target_arch = "wasm32"))]
use std::io::{self, Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

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
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(data)?;
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

#[cfg(not(target_arch = "wasm32"))]
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
        // ── Read ──
        match ws.read() {
            Ok(tungstenite::Message::Text(text)) => {
                NET_TRAFFIC.bytes_received.fetch_add(text.len() as u64, std::sync::atomic::Ordering::Relaxed);
                NET_TRAFFIC.msgs_received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(msg) => {
                        debug_tap::record_rx(
                            "ws_reader",
                            format!("player {} -> host ws {}", player_id, client_msg_kind(&msg)),
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
                            format!("player {} invalid ws msg: {}", player_id, e),
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
            Ok(_) => {} // Binary, Pong, Frame — ignore
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

        // ── Write — drain outgoing queue ──
        while let Ok(data) = writer_rx.try_recv() {
            let text = String::from_utf8_lossy(&data).to_string();
            let text_len = text.len() as u64;
            if ws.send(tungstenite::Message::Text(text.into())).is_err() {
                break;
            }
            NET_TRAFFIC.bytes_sent.fetch_add(text_len, std::sync::atomic::Ordering::Relaxed);
            NET_TRAFFIC.msgs_sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    debug_tap::record_info("ws_handler", format!("player {} WS disconnected", player_id));
    let _ = dc_tx.send(player_id);
}

// ── WASM WebSocket client ───────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

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

    // onopen — send JoinRequest once connected
    let ws_for_open = ws.clone();
    let onopen = Closure::wrap(Box::new(move |_: JsValue| {
        bevy::log::info!("WebSocket connected to host");
        let join = ClientMessage::JoinRequest {
            seq: 0,
            timestamp: 0.0,
            player_name: player_name.clone(),
            preferred_faction_index: None,
        };
        if let Ok(json) = serde_json::to_string(&join) {
            let _ = ws_for_open.send_with_str(&json);
        }
    }) as Box<dyn FnMut(JsValue)>);
    ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();

    // onmessage — parse JSON and queue
    let onmessage = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
        if let Some(text) = e.data().as_string() {
            match serde_json::from_str::<ServerMessage>(&text) {
                Ok(msg) => {
                    let _ = incoming_tx.send(msg);
                }
                Err(err) => {
                    bevy::log::warn!("WS parse error: {}", err);
                }
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
