#[cfg(not(target_arch = "wasm32"))]
use std::io;
#[cfg(not(target_arch = "wasm32"))]
use std::net::{TcpListener, TcpStream};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{Receiver, Sender};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use game_state::codec;
#[cfg(not(target_arch = "wasm32"))]
use game_state::message::{ClientMessage, ServerMessage};
#[cfg(not(target_arch = "wasm32"))]
use socket2::SockRef;

#[cfg(not(target_arch = "wasm32"))]
use super::debug_tap;
#[cfg(not(target_arch = "wasm32"))]
use super::wire::{decode_server_payload_bytes, DecodedServerPayload};
#[cfg(not(target_arch = "wasm32"))]
use super::{
    client_msg_kind, decode_server_messages_bytes, recv_framed, send_framed, server_msg_kind,
    NET_TRAFFIC,
};

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

#[cfg(not(target_arch = "wasm32"))]
pub fn host_listener_thread(
    listener: TcpListener,
    new_client_tx: Sender<NewClientEvent>,
    shutdown: Arc<AtomicBool>,
) {
    listener
        .set_nonblocking(true)
        .expect("Failed to set listener non-blocking");

    let mut next_player_id: u8 = 1;

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
                let parsed = decode_server_payload_bytes(&data);
                let detail = match &parsed {
                    Ok(DecodedServerPayload::Message(msg)) => {
                        format!("host->client {}", server_msg_kind(msg))
                    }
                    Ok(DecodedServerPayload::Frame(frame)) => {
                        format!("host->client frame({})", frame.messages.len())
                    }
                    Err(_) => "host->client raw".to_string(),
                };
                let payload = parsed
                    .as_ref()
                    .ok()
                    .and_then(|decoded| decoded.to_json_text().ok())
                    .or_else(|| Some(debug_tap::payload_preview(&data)));

                if let Err(e) = send_framed(&mut stream, &data) {
                    debug_tap::record_error(
                        "host_client_writer",
                        format!("send failed ({} bytes): {}", data.len(), e),
                    );
                    break;
                }
                NET_TRAFFIC
                    .bytes_sent
                    .fetch_add(data.len() as u64 + 4, std::sync::atomic::Ordering::Relaxed);
                NET_TRAFFIC
                    .msgs_sent
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();

    while !shutdown.load(Ordering::Relaxed) {
        match recv_framed(&mut stream) {
            Ok(data) => {
                NET_TRAFFIC
                    .bytes_received
                    .fetch_add(data.len() as u64 + 4, std::sync::atomic::Ordering::Relaxed);
                NET_TRAFFIC
                    .msgs_received
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let parsed = codec::decode::<ClientMessage>(&data).or_else(|_| {
                    serde_json::from_slice::<ClientMessage>(&data)
                        .map_err(|e| rmp_serde::decode::Error::Uncategorized(e.to_string()))
                });
                match parsed {
                    Ok(msg) => {
                        let detail =
                            format!("player {} -> host {}", player_id, client_msg_kind(&msg));
                        let payload = Some(codec::to_debug_json(&msg));
                        debug_tap::record_rx("host_client_reader", detail, data.len(), payload);
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
                            format!("invalid client message from player {}: {}", player_id, e),
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
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
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

    debug_tap::record_info(
        "host_client_reader",
        format!("player {} disconnected", player_id),
    );
    let _ = disconnect_tx.send(player_id);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn client_reader_thread(
    mut stream: TcpStream,
    incoming_tx: Sender<ServerMessage>,
    shutdown: Arc<AtomicBool>,
) {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();

    while !shutdown.load(Ordering::Relaxed) {
        match recv_framed(&mut stream) {
            Ok(data) => {
                NET_TRAFFIC
                    .bytes_received
                    .fetch_add(data.len() as u64 + 4, std::sync::atomic::Ordering::Relaxed);
                NET_TRAFFIC
                    .msgs_received
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                match decode_server_messages_bytes(&data) {
                    Ok(messages) => {
                        for msg in messages {
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
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
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
                NET_TRAFFIC
                    .bytes_sent
                    .fetch_add(data.len() as u64 + 4, std::sync::atomic::Ordering::Relaxed);
                NET_TRAFFIC
                    .msgs_sent
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                debug_tap::record_tx("client_writer", detail, data.len(), payload);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}
