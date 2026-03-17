//! Local network debug tap with a tiny HTTP API for Postman polling.
//!
//! Endpoints on 127.0.0.1:8787:
//! - GET /health
//! - GET /events
//! - GET /events?since=<id>
//! - POST /clear

use bevy::log::{info, warn};
use serde::Serialize;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEBUG_HTTP_HOST: &str = "127.0.0.1";
const DEBUG_HTTP_DEFAULT_PORT: u16 = 8787;
const DEBUG_HTTP_MAX_PORT: u16 = 8795;
const MAX_EVENTS: usize = 2_000;
const MAX_PAYLOAD_PREVIEW: usize = 2_048;

static DEBUG_STATE: OnceLock<Arc<DebugState>> = OnceLock::new();

#[derive(Clone, Serialize)]
pub struct NetDebugEvent {
    pub id: u64,
    pub ts_ms: u128,
    pub lane: String,
    pub direction: String,
    pub detail: String,
    pub bytes: usize,
    pub payload: Option<String>,
}

struct DebugState {
    http_addr: Option<String>,
    events: Mutex<VecDeque<NetDebugEvent>>,
    next_id: AtomicU64,
}

#[derive(Serialize)]
struct EventsResponse {
    since: u64,
    latest_id: u64,
    count: usize,
    events: Vec<NetDebugEvent>,
}

#[derive(Serialize)]
struct HealthResponse<'a> {
    status: &'a str,
    addr: Option<&'a str>,
    latest_id: u64,
}

/// Start the debug tap exactly once.
pub fn ensure_started() {
    let state = DEBUG_STATE.get_or_init(|| {
        let (listener, http_addr) = bind_debug_listener();
        let state = Arc::new(DebugState {
            http_addr,
            events: Mutex::new(VecDeque::with_capacity(MAX_EVENTS)),
            next_id: AtomicU64::new(0),
        });
        if let Some(listener) = listener {
            spawn_http_server(listener, state.clone());
        }
        state
    });
    if let Some(addr) = &state.http_addr {
        info!("Net debug tap ready: http://{}/events", addr);
    } else {
        warn!("Net debug tap disabled: could not bind any local debug port");
    }
}

pub fn record_tx(
    lane: &str,
    detail: impl Into<String>,
    bytes: usize,
    payload: Option<String>,
) {
    record("tx", lane, detail.into(), bytes, payload);
}

pub fn record_rx(
    lane: &str,
    detail: impl Into<String>,
    bytes: usize,
    payload: Option<String>,
) {
    record("rx", lane, detail.into(), bytes, payload);
}

pub fn record_info(lane: &str, detail: impl Into<String>) {
    record("info", lane, detail.into(), 0, None);
}

pub fn record_error(lane: &str, detail: impl Into<String>) {
    record("error", lane, detail.into(), 0, None);
}

pub fn payload_preview(raw: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(raw).to_string();
    if text.len() > MAX_PAYLOAD_PREVIEW {
        text.truncate(MAX_PAYLOAD_PREVIEW);
        text.push_str("…");
    }
    text
}

fn record(direction: &str, lane: &str, detail: String, bytes: usize, payload: Option<String>) {
    let Some(state) = DEBUG_STATE.get() else {
        return;
    };
    let id = state.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let mut events = state.events.lock().unwrap();
    events.push_back(NetDebugEvent {
        id,
        ts_ms,
        lane: lane.to_string(),
        direction: direction.to_string(),
        detail,
        bytes,
        payload,
    });
    while events.len() > MAX_EVENTS {
        events.pop_front();
    }
}

fn bind_debug_listener() -> (Option<TcpListener>, Option<String>) {
    if let Ok(port_str) = std::env::var("RTS_NET_DEBUG_PORT") {
        if let Ok(port) = port_str.parse::<u16>() {
            let addr = format!("{}:{}", DEBUG_HTTP_HOST, port);
            match TcpListener::bind(&addr) {
                Ok(listener) => return (Some(listener), Some(addr)),
                Err(e) => {
                    warn!(
                        "Net debug tap failed to bind RTS_NET_DEBUG_PORT={} ({}): {}",
                        port, addr, e
                    );
                    return (None, None);
                }
            }
        } else {
            warn!(
                "Net debug tap ignored invalid RTS_NET_DEBUG_PORT value: {}",
                port_str
            );
        }
    }

    for port in DEBUG_HTTP_DEFAULT_PORT..=DEBUG_HTTP_MAX_PORT {
        let addr = format!("{}:{}", DEBUG_HTTP_HOST, port);
        if let Ok(listener) = TcpListener::bind(&addr) {
            return (Some(listener), Some(addr));
        }
    }

    (None, None)
}

fn spawn_http_server(listener: TcpListener, state: Arc<DebugState>) {
    std::thread::spawn(move || {
        listener.set_nonblocking(true).ok();
        if let Some(addr) = &state.http_addr {
            info!("Net debug tap listening on http://{}", addr);
        }

        loop {
            match listener.accept() {
                Ok((mut stream, _addr)) => {
                    let _ = handle_http_connection(&mut stream, &state);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    warn!("Net debug tap accept error: {}", e);
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }
    });
}

fn handle_http_connection(stream: &mut std::net::TcpStream, state: &Arc<DebugState>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(500))).ok();

    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Ok(());
    }
    let req = String::from_utf8_lossy(&buf[..n]);
    let mut lines = req.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");
    let (path, query) = split_path_query(target);

    match (method, path) {
        ("GET", "/health") => {
            let body = serde_json::to_vec(&HealthResponse {
                status: "ok",
                addr: state.http_addr.as_deref(),
                latest_id: state.next_id.load(Ordering::Relaxed),
            })
            .unwrap_or_else(|_| b"{\"status\":\"ok\"}".to_vec());
            write_json(stream, 200, &body)?;
        }
        ("GET", "/events") => {
            let since = query_param_u64(query, "since").unwrap_or(0);
            let (latest_id, events) = snapshot_since(state, since);
            let body = serde_json::to_vec(&EventsResponse {
                since,
                latest_id,
                count: events.len(),
                events,
            })
            .unwrap_or_else(|_| b"{\"count\":0,\"events\":[]}".to_vec());
            write_json(stream, 200, &body)?;
        }
        ("POST", "/clear") => {
            clear_events(state);
            write_json(stream, 200, b"{\"ok\":true}")?;
        }
        _ => {
            write_json(stream, 404, b"{\"error\":\"not found\"}")?;
        }
    }

    Ok(())
}

fn split_path_query(target: &str) -> (&str, &str) {
    if let Some((path, query)) = target.split_once('?') {
        (path, query)
    } else {
        (target, "")
    }
}

fn query_param_u64(query: &str, key: &str) -> Option<u64> {
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=')?;
        if k == key {
            if let Ok(parsed) = v.parse::<u64>() {
                return Some(parsed);
            }
        }
    }
    None
}

fn snapshot_since(state: &Arc<DebugState>, since: u64) -> (u64, Vec<NetDebugEvent>) {
    let events = state.events.lock().unwrap();
    let latest_id = events.back().map(|e| e.id).unwrap_or(0);
    let collected = events
        .iter()
        .filter(|e| e.id > since)
        .cloned()
        .collect::<Vec<_>>();
    (latest_id, collected)
}

fn clear_events(state: &Arc<DebugState>) {
    let mut events = state.events.lock().unwrap();
    events.clear();
}

fn write_json(stream: &mut std::net::TcpStream, status: u16, body: &[u8]) -> std::io::Result<()> {
    let status_text = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "OK",
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        status_text,
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}
