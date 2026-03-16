//! TCP transport layer — length-prefixed framing and background I/O threads.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use game_state::message::{ClientMessage, ServerMessage};

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
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame too large: {} bytes", len),
        ));
    }

    let mut buf = vec![0u8; len];
    read_exact_timeout(stream, &mut buf)?;
    Ok(buf)
}

// ── LAN IP detection ────────────────────────────────────────────────────────

/// Detect local LAN IP by querying the OS routing table (no packets sent).
pub fn detect_lan_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

// ── New client event ────────────────────────────────────────────────────────

pub struct NewClientEvent {
    pub player_id: u8,
    pub stream: TcpStream,
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
                stream.set_nodelay(true).ok();
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
                if send_framed(&mut stream, &data).is_err() {
                    break;
                }
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
                if let Ok(msg) = serde_json::from_slice::<ClientMessage>(&data) {
                    if incoming_tx.send((player_id, msg)).is_err() {
                        break;
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
                break;
            }
        }
    }

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
                if let Ok(msg) = serde_json::from_slice::<ServerMessage>(&data) {
                    if incoming_tx.send(msg).is_err() {
                        break;
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
                if send_framed(&mut stream, &data).is_err() {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}
