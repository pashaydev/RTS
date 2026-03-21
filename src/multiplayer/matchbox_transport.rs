//! Matchbox WebRTC transport layer — replaces TCP/WebSocket with WebRTC data channels.
//!
//! Uses `bevy_matchbox` for both native and WASM targets, with an embedded
//! signaling server on the host side. The host-authoritative model is unchanged:
//! the host runs the full simulation, clients receive state sync and send commands.

use bevy::prelude::*;
use bevy_matchbox::prelude::*;
use std::collections::HashMap;

use game_state::codec;
use game_state::message::{ClientMessage, ServerFrame, ServerMessage};

use super::debug_tap;
use super::NET_TRAFFIC;

/// Channel indices for the dual-channel WebRTC setup.
pub const RELIABLE_CH: usize = 0;
pub const UNRELIABLE_CH: usize = 1;

/// Port for the embedded signaling server (host-side).
pub const SIGNALING_PORT: u16 = 3536;

/// Maps Matchbox `PeerId`s to game `player_id`s and vice versa.
#[derive(Resource, Default, Debug)]
pub struct PeerMap {
    pub peer_to_player: HashMap<PeerId, u8>,
    pub player_to_peer: HashMap<u8, PeerId>,
    next_player_id: u8,
}

impl PeerMap {
    /// Assign the next available player_id (1+) for a newly connected peer.
    pub fn assign(&mut self, peer: PeerId) -> u8 {
        self.next_player_id += 1;
        let id = self.next_player_id;
        self.peer_to_player.insert(peer, id);
        self.player_to_peer.insert(id, peer);
        id
    }

    /// Remove a peer from the map and return its player_id, if any.
    pub fn remove_peer(&mut self, peer: &PeerId) -> Option<u8> {
        if let Some(id) = self.peer_to_player.remove(peer) {
            self.player_to_peer.remove(&id);
            Some(id)
        } else {
            None
        }
    }

    /// Get the player_id for a peer.
    pub fn player_id(&self, peer: &PeerId) -> Option<u8> {
        self.peer_to_player.get(peer).copied()
    }

    /// Get the PeerId for a player_id.
    pub fn peer_id(&self, player_id: u8) -> Option<PeerId> {
        self.player_to_peer.get(&player_id).copied()
    }
}

/// Queued incoming messages from all peers, drained by game systems each frame.
#[derive(Resource, Default)]
pub struct MatchboxInbox {
    /// Host: incoming commands from clients (player_id, message).
    pub client_commands: Vec<(u8, ClientMessage)>,
    /// Client: incoming server messages.
    pub server_messages: Vec<ServerMessage>,
    /// Peer connect events (PeerId of newly connected peers).
    pub connected: Vec<PeerId>,
    /// Peer disconnect events (PeerId of disconnected peers).
    pub disconnected: Vec<PeerId>,
}

impl MatchboxInbox {
    pub fn clear(&mut self) {
        self.client_commands.clear();
        self.server_messages.clear();
        self.connected.clear();
        self.disconnected.clear();
    }
}

/// Decoded payload from a peer — either a single message or a batched frame.
enum DecodedServerPayload {
    Message(ServerMessage),
    Frame(ServerFrame),
}

impl DecodedServerPayload {
    fn describe(&self) -> String {
        match self {
            Self::Message(msg) => format!("host -> client {}", server_msg_kind(msg)),
            Self::Frame(frame) => format!("host -> client frame({})", frame.messages.len()),
        }
    }

    fn to_debug_json(&self) -> String {
        match self {
            Self::Message(msg) => codec::to_debug_json(msg),
            Self::Frame(frame) => serde_json::to_string(frame)
                .unwrap_or_else(|_| debug_tap::payload_preview(&codec::encode(frame).unwrap_or_default())),
        }
    }
}

/// Polls the Matchbox socket for peer state changes and incoming messages.
/// Fills `MatchboxInbox` for consumption by host/client game systems.
pub fn poll_matchbox(
    mut socket: ResMut<MatchboxSocket>,
    mut inbox: ResMut<MatchboxInbox>,
    peer_map: Res<PeerMap>,
    role: Res<super::NetRole>,
) {
    inbox.clear();

    // Update peer states — detect connects/disconnects
    match socket.try_update_peers() {
        Ok(changes) => {
            for (peer, state) in changes {
                match state {
                    PeerState::Connected => {
                        debug_tap::record_info("matchbox_peer", format!("peer {:?} connected", peer));
                        inbox.connected.push(peer);
                    }
                    PeerState::Disconnected => {
                        debug_tap::record_info(
                            "matchbox_peer",
                            format!("peer {:?} disconnected", peer),
                        );
                        inbox.disconnected.push(peer);
                    }
                }
            }
        }
        Err(_) => {
            // Socket closed
            return;
        }
    }

    // Drain both channels
    for ch in [RELIABLE_CH, UNRELIABLE_CH] {
        let Ok(channel) = socket.get_channel_mut(ch) else {
            continue;
        };
        let messages = channel.receive();
        for (peer, packet) in messages {
            let bytes = &packet[..];
            NET_TRAFFIC
                .bytes_received
                .fetch_add(bytes.len() as u64, std::sync::atomic::Ordering::Relaxed);
            NET_TRAFFIC
                .msgs_received
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            if *role == super::NetRole::Host {
                // Host receives ClientMessages
                if let Ok(msg) = codec::decode::<ClientMessage>(bytes) {
                    let player_id = peer_map.player_id(&peer).unwrap_or(0);
                    debug_tap::record_rx(
                        "matchbox_host_rx",
                        format!("player {} -> host {}", player_id, client_msg_kind(&msg)),
                        bytes.len(),
                        Some(codec::to_debug_json(&msg)),
                    );
                    inbox.client_commands.push((player_id, msg));
                } else {
                    debug_tap::record_error(
                        "matchbox_host_rx",
                        format!("failed to decode ClientMessage from peer {:?}", peer),
                    );
                    debug_tap::record_rx(
                        "matchbox_host_rx",
                        format!("peer {:?} -> host raw_invalid", peer),
                        bytes.len(),
                        Some(debug_tap::payload_preview(bytes)),
                    );
                    warn!("Host: failed to decode ClientMessage from peer {:?}", peer);
                }
            } else {
                // Client receives ServerMessages or ServerFrames
                if let Some(payload) = decode_server_payload(bytes) {
                    debug_tap::record_rx(
                        "matchbox_client_rx",
                        payload.describe(),
                        bytes.len(),
                        Some(payload.to_debug_json()),
                    );
                    match payload {
                        DecodedServerPayload::Message(msg) => {
                            inbox.server_messages.push(msg);
                        }
                        DecodedServerPayload::Frame(frame) => {
                            inbox.server_messages.extend(frame.messages);
                        }
                    }
                } else {
                    debug_tap::record_error(
                        "matchbox_client_rx",
                        format!("failed to decode server payload from peer {:?}", peer),
                    );
                    debug_tap::record_rx(
                        "matchbox_client_rx",
                        format!("peer {:?} -> client raw_invalid", peer),
                        bytes.len(),
                        Some(debug_tap::payload_preview(bytes)),
                    );
                    warn!("Client: failed to decode server payload from peer {:?}", peer);
                }
            }
        }
    }
}

fn decode_server_payload(data: &[u8]) -> Option<DecodedServerPayload> {
    codec::decode::<ServerMessage>(data)
        .map(DecodedServerPayload::Message)
        .or_else(|_| codec::decode::<ServerFrame>(data).map(DecodedServerPayload::Frame))
        .ok()
}

// ── Send helpers ─────────────────────────────────────────────────────────────

fn track_send(bytes_len: usize) {
    NET_TRAFFIC
        .bytes_sent
        .fetch_add(bytes_len as u64, std::sync::atomic::Ordering::Relaxed);
    NET_TRAFFIC
        .msgs_sent
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

fn client_msg_kind(msg: &ClientMessage) -> &'static str {
    match msg {
        ClientMessage::Input { .. } => "input",
        ClientMessage::JoinRequest { .. } => "join",
        ClientMessage::LeaveNotice { .. } => "leave",
        ClientMessage::Ping { .. } => "ping",
        ClientMessage::Reconnect { .. } => "reconnect",
        ClientMessage::Chat { .. } => "chat",
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

/// Broadcast a ServerMessage to all connected peers on the reliable channel.
pub fn broadcast_reliable(socket: &mut MatchboxSocket, msg: &ServerMessage) {
    let Ok(bytes) = codec::encode(msg) else {
        error!("Failed to encode ServerMessage for broadcast");
        return;
    };
    let packet: Box<[u8]> = bytes.into();
    let peers: Vec<PeerId> = socket.connected_peers().collect();
    if peers.is_empty() {
        return;
    }
    let detail = format!("host -> peers reliable {}", server_msg_kind(msg));
    let payload = Some(codec::to_debug_json(msg));
    for peer in peers {
        track_send(packet.len());
        let _ = socket.channel_mut(RELIABLE_CH).try_send(packet.clone(), peer);
        debug_tap::record_tx("matchbox_host_tx", detail.clone(), packet.len(), payload.clone());
    }
}

/// Broadcast a ServerFrame to all connected peers on the unreliable channel.
/// Falls back to reliable if the frame is too large for unreliable delivery.
pub fn broadcast_unreliable(socket: &mut MatchboxSocket, frame: &ServerFrame) {
    let Ok(bytes) = codec::encode(frame) else {
        error!("Failed to encode ServerFrame for broadcast");
        return;
    };
    let packet: Box<[u8]> = bytes.into();
    let peers: Vec<PeerId> = socket.connected_peers().collect();
    if peers.is_empty() {
        return;
    }
    // WebRTC data channels typically support up to ~256KB but practical limit
    // for unreliable is ~16KB. Fall back to reliable for large payloads.
    let ch = if packet.len() > 16_000 {
        RELIABLE_CH
    } else {
        UNRELIABLE_CH
    };
    let lane = if ch == RELIABLE_CH {
        "matchbox_host_tx"
    } else {
        "matchbox_host_tx_unreliable"
    };
    let detail = format!(
        "host -> peers {} frame({})",
        if ch == RELIABLE_CH {
            "reliable"
        } else {
            "unreliable"
        },
        frame.messages.len()
    );
    let payload = Some(
        serde_json::to_string(frame)
            .unwrap_or_else(|_| debug_tap::payload_preview(packet.as_ref())),
    );
    for peer in peers {
        track_send(packet.len());
        let _ = socket.channel_mut(ch).try_send(packet.clone(), peer);
        debug_tap::record_tx(lane, detail.clone(), packet.len(), payload.clone());
    }
}

/// Send a ServerMessage to a specific player on the reliable channel.
pub fn send_to_player(
    socket: &mut MatchboxSocket,
    peer_map: &PeerMap,
    player_id: u8,
    msg: &ServerMessage,
) {
    let Some(peer) = peer_map.peer_id(player_id) else {
        return;
    };
    let Ok(bytes) = codec::encode(msg) else {
        return;
    };
    let bytes_len = bytes.len();
    let packet: Box<[u8]> = bytes.into();
    track_send(packet.len());
    let _ = socket.channel_mut(RELIABLE_CH).try_send(packet, peer);
    debug_tap::record_tx(
        "matchbox_host_tx",
        format!("host -> player {} {}", player_id, server_msg_kind(msg)),
        bytes_len,
        Some(codec::to_debug_json(msg)),
    );
}

/// Send a ClientMessage to the host (first connected peer) on the reliable channel.
pub fn send_to_host(socket: &mut MatchboxSocket, msg: &ClientMessage) {
    let Ok(bytes) = codec::encode(msg) else {
        return;
    };
    let bytes_len = bytes.len();
    let packet: Box<[u8]> = bytes.into();
    let Some(host_peer) = socket.connected_peers().next() else {
        return;
    };
    track_send(packet.len());
    let _ = socket.channel_mut(RELIABLE_CH).try_send(packet, host_peer);
    debug_tap::record_tx(
        "matchbox_client_tx",
        format!("client -> host {}", client_msg_kind(msg)),
        bytes_len,
        Some(codec::to_debug_json(msg)),
    );
}

/// Build the standard 2-channel WebRTC socket builder for a given room URL.
pub fn build_socket(room_url: &str) -> WebRtcSocketBuilder {
    WebRtcSocketBuilder::new(room_url)
        .add_channel(ChannelConfig::reliable())
        .add_channel(ChannelConfig::unreliable())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_peer(n: u8) -> PeerId {
        let mut bytes = [0u8; 16];
        bytes[0] = n;
        PeerId(uuid::Uuid::from_bytes(bytes))
    }

    #[test]
    fn peer_map_assign_and_remove() {
        let mut map = PeerMap::default();
        let peer = test_peer(1);
        let id = map.assign(peer);
        assert_eq!(id, 1);
        assert_eq!(map.player_id(&peer), Some(1));
        assert_eq!(map.peer_id(1), Some(peer));

        let removed = map.remove_peer(&peer);
        assert_eq!(removed, Some(1));
        assert_eq!(map.player_id(&peer), None);
        assert_eq!(map.peer_id(1), None);
    }

    #[test]
    fn peer_map_assigns_incrementing_ids() {
        let mut map = PeerMap::default();
        let p1 = test_peer(1);
        let p2 = test_peer(2);
        assert_eq!(map.assign(p1), 1);
        assert_eq!(map.assign(p2), 2);
    }

    #[test]
    fn inbox_clear_empties_all_vecs() {
        let mut inbox = MatchboxInbox {
            client_commands: vec![(1, ClientMessage::Ping { seq: 0, timestamp: 0.0 })],
            server_messages: vec![ServerMessage::Pong { seq: 0, timestamp: 0.0 }],
            connected: vec![test_peer(1)],
            disconnected: vec![test_peer(2)],
        };
        inbox.clear();
        assert!(inbox.client_commands.is_empty());
        assert!(inbox.server_messages.is_empty());
        assert!(inbox.connected.is_empty());
        assert!(inbox.disconnected.is_empty());
    }
}
