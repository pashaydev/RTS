//! Transport layer — legacy TCP/WS code (mostly unused) + LAN discovery + HTTP file server.
#![allow(dead_code)]

mod wire;
mod discovery;
mod ip;
mod tcp;

pub use super::matchbox_transport::*;
pub use discovery::{discover_lan_hosts, discovery_listener_thread, DISCOVERY_PORT};
pub use ip::{detect_all_ips, detect_lan_ip};
#[allow(unused_imports)]
pub use ip::DetectedIp;
#[allow(unused_imports)]
pub use tcp::{
    client_reader_thread, client_writer_thread, client_writer_thread_fn, configure_keepalive,
    host_client_reader_thread, host_listener_thread, NewClientEvent,
};
pub use wire::{decode_server_messages_bytes, recv_framed, send_framed};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use game_state::codec;
use game_state::message::{ClientMessage, ServerMessage};
#[cfg(not(target_arch = "wasm32"))]
use std::io::{self, Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::net::{TcpListener, TcpStream};
#[cfg(not(target_arch = "wasm32"))]
use wire::{decode_server_payload_bytes, encode_ws_server_payload, DecodedServerPayload, WsClientEncoding};

#[cfg(not(target_arch = "wasm32"))]
use super::debug_tap;
use super::NET_TRAFFIC;

/// WebSocket new client — reader/writer threads already spawned.
pub struct WsNewClientEvent {
    pub player_id: u8,
    pub writer_tx: Sender<Vec<u8>>,
}

pub(super) fn client_msg_kind(msg: &ClientMessage) -> &'static str {
    match msg {
        ClientMessage::Input { .. } => "input",
        ClientMessage::JoinRequest { .. } => "join",
        ClientMessage::LeaveNotice { .. } => "leave",
        ClientMessage::Ping { .. } => "ping",
        ClientMessage::Reconnect { .. } => "reconnect",
        ClientMessage::Chat { .. } => "chat",
    }
}

pub(super) fn server_msg_kind(msg: &ServerMessage) -> &'static str {
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

// ── Host static file server for WASM clients (native only) ──────────────────

/// HTTP port offset from the TCP game port (e.g., 7878 + 2 = 7880).
pub const HTTP_PORT_OFFSET: u16 = 2;

/// Serves the WASM `dist/` folder over HTTP so LAN browsers can load the game
/// client directly from the host without a separate web server or session router.
/// Players open `http://<host-ip>:<port>` in their browser.
#[cfg(not(target_arch = "wasm32"))]
pub fn host_file_server_thread(listener: TcpListener, dist_dir: String, shutdown: Arc<AtomicBool>) {
    listener
        .set_nonblocking(true)
        .expect("Failed to set HTTP listener non-blocking");

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let dist = dist_dir.clone();
                std::thread::spawn(move || serve_http_request(stream, &dist));
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                if !shutdown.load(Ordering::Relaxed) {
                    bevy::log::warn!("HTTP file server error: {}", e);
                }
                break;
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn serve_http_request(mut stream: TcpStream, dist_dir: &str) {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

    let mut buf = [0u8; 4096];
    let n = match stream.read(&mut buf) {
        Ok(n) if n > 0 => n,
        _ => return,
    };
    let request = String::from_utf8_lossy(&buf[..n]);
    let first_line = request.lines().next().unwrap_or("");

    let path = first_line.split_whitespace().nth(1).unwrap_or("/");

    let clean_path = path.trim_start_matches('/').replace("..", "");
    let clean_path = if clean_path.is_empty() {
        "index.html"
    } else {
        &clean_path
    };

    let file_path = std::path::Path::new(dist_dir).join(clean_path);

    let file_path = if file_path.is_dir() {
        file_path.join("index.html")
    } else {
        file_path
    };

    match std::fs::read(&file_path) {
        Ok(body) => {
            let mime = mime_for_path(&file_path);
            let header = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: {}\r\n\
                 Content-Length: {}\r\n\
                 Access-Control-Allow-Origin: *\r\n\
                 Cache-Control: no-cache\r\n\
                 Connection: close\r\n\r\n",
                mime,
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
        }
        Err(_) => {
            let body = b"404 Not Found";
            let header = format!(
                "HTTP/1.1 404 Not Found\r\n\
                 Content-Type: text/plain\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(body);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn mime_for_path(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
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

#[cfg(not(target_arch = "wasm32"))]
fn ws_client_handler(
    stream: TcpStream,
    cmd_tx: Sender<(u8, ClientMessage)>,
    dc_tx: Sender<u8>,
    ws_client_tx: Sender<WsNewClientEvent>,
    player_id: u8,
    shutdown: Arc<AtomicBool>,
) {
    let mut outgoing_encoding = WsClientEncoding::Unknown;

    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();

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

    ws.get_mut()
        .set_read_timeout(Some(Duration::from_millis(50)))
        .ok();

    let (writer_tx, writer_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let _ = ws_client_tx.send(WsNewClientEvent {
        player_id,
        writer_tx,
    });

    while !shutdown.load(Ordering::Relaxed) {
        match ws.read() {
            Ok(tungstenite::Message::Binary(data)) => {
                outgoing_encoding = WsClientEncoding::BinaryMsgpack;
                NET_TRAFFIC
                    .bytes_received
                    .fetch_add(data.len() as u64, std::sync::atomic::Ordering::Relaxed);
                NET_TRAFFIC
                    .msgs_received
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
                outgoing_encoding = WsClientEncoding::TextJson;
                NET_TRAFFIC
                    .bytes_received
                    .fetch_add(text.len() as u64, std::sync::atomic::Ordering::Relaxed);
                NET_TRAFFIC
                    .msgs_received
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(msg) => {
                        debug_tap::record_rx(
                            "ws_reader",
                            format!(
                                "player {} -> host ws(json) {}",
                                player_id,
                                client_msg_kind(&msg)
                            ),
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
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
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

        while let Ok(data) = writer_rx.try_recv() {
            let data_len = data.len() as u64;
            let parsed = decode_server_payload_bytes(&data);
            let detail = match &parsed {
                Ok(DecodedServerPayload::Message(msg)) => {
                    format!("host->ws {}", server_msg_kind(msg))
                }
                Ok(DecodedServerPayload::Frame(frame)) => {
                    format!("host->ws frame({})", frame.messages.len())
                }
                Err(_) => "host->ws raw".to_string(),
            };
            let payload = parsed
                .as_ref()
                .ok()
                .and_then(|decoded| decoded.to_json_text().ok())
                .or_else(|| Some(debug_tap::payload_preview(&data)));

            let Ok(message) = encode_ws_server_payload(&data, outgoing_encoding) else {
                debug_tap::record_error(
                    "ws_writer",
                    format!("player {} failed to encode outgoing WS payload", player_id),
                );
                continue;
            };

            if let Err(err) = ws.send(message) {
                bevy::log::warn!("WS handler: send failed for player {}: {}", player_id, err);
                debug_tap::record_error(
                    "ws_writer",
                    format!("player {} ws send failed: {}", player_id, err),
                );
                break;
            }
            NET_TRAFFIC
                .bytes_sent
                .fetch_add(data_len, std::sync::atomic::Ordering::Relaxed);
            NET_TRAFFIC
                .msgs_sent
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            debug_tap::record_tx("ws_writer", detail, data_len as usize, payload);
        }
    }

    debug_tap::record_info(
        "ws_handler",
        format!("player {} WS disconnected", player_id),
    );
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
fn wasm_decode_server_payload(data: &JsValue) -> Result<Vec<ServerMessage>, String> {
    if let Some(abuf) = data.dyn_ref::<js_sys::ArrayBuffer>() {
        let arr = js_sys::Uint8Array::new(abuf);
        let bytes = arr.to_vec();
        decode_server_messages_bytes(&bytes)
    } else if let Some(arr) = data.dyn_ref::<js_sys::Uint8Array>() {
        decode_server_messages_bytes(&arr.to_vec())
    } else if let Some(text) = data.as_string() {
        decode_server_messages_bytes(text.as_bytes())
    } else {
        Err("unsupported websocket payload type".to_string())
    }
}

#[cfg(target_arch = "wasm32")]
pub fn wasm_ws_connect(
    url: &str,
    incoming_tx: Sender<ServerMessage>,
    shutdown: Arc<AtomicBool>,
    player_name: String,
) -> Result<web_sys::WebSocket, String> {
    let ws = web_sys::WebSocket::new(url).map_err(|e| format!("{:?}", e))?;
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

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

    let onmessage = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
        let data = e.data();
        match wasm_decode_server_payload(&data) {
            Ok(messages) => {
                for msg in messages {
                    bevy::log::info!(
                        "WS received server message: {:?}",
                        std::mem::discriminant(&msg)
                    );
                    let _ = incoming_tx.send(msg);
                }
            }
            Err(err) => {
                bevy::log::warn!("WS payload parse error: {}", err);
            }
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    let shutdown_err = shutdown.clone();
    let onerror = Closure::wrap(Box::new(move |_: web_sys::ErrorEvent| {
        bevy::log::warn!("WebSocket error");
        shutdown_err.store(true, Ordering::Relaxed);
    }) as Box<dyn FnMut(web_sys::ErrorEvent)>);
    ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    onerror.forget();

    let shutdown_close = shutdown.clone();
    let onclose = Closure::wrap(Box::new(move |e: web_sys::CloseEvent| {
        bevy::log::info!(
            "WebSocket closed: code={}, reason='{}', clean={}",
            e.code(),
            e.reason(),
            e.was_clean()
        );
        shutdown_close.store(true, Ordering::Relaxed);
    }) as Box<dyn FnMut(web_sys::CloseEvent)>);
    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();

    Ok(ws)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use game_state::message::{ClientMessage, PlayerInput, ServerMessage};
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

        let handle =
            thread::spawn(move || client_writer_thread(stream_tx, outgoing_rx, thread_shutdown));

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

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    use super::*;
    use game_state::message::ServerFrame;
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
        assert_eq!(decoded, vec![msg]);
    }

    #[wasm_bindgen_test]
    fn wasm_decode_server_payload_accepts_legacy_json_text() {
        let msg = sample_server_message();
        let text = serde_json::to_string(&msg).unwrap();

        let decoded = wasm_decode_server_payload(&JsValue::from_str(&text)).unwrap();
        assert_eq!(decoded, vec![msg]);
    }

    #[wasm_bindgen_test]
    fn wasm_decode_server_payload_accepts_batched_frames() {
        let msg = sample_server_message();
        let frame = game_state::message::ServerFrame {
            tick: 7,
            timestamp: 1.5,
            messages: vec![msg.clone()],
        };
        let bytes = game_state::codec::encode(&frame).unwrap();
        let array = js_sys::Uint8Array::from(bytes.as_slice());
        let payload = JsValue::from(array.buffer());

        let decoded = wasm_decode_server_payload(&payload).unwrap();
        assert_eq!(decoded, vec![msg]);
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
