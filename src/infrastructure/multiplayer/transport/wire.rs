use game_state::codec;
use game_state::message::{ServerFrame, ServerMessage};

#[cfg(not(target_arch = "wasm32"))]
use serde::Deserialize;
#[cfg(not(target_arch = "wasm32"))]
use std::io::{self, Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::net::TcpStream;

#[cfg(not(target_arch = "wasm32"))]
use super::debug_tap;

#[derive(Debug, Clone)]
pub(super) enum DecodedServerPayload {
    Message(ServerMessage),
    Frame(ServerFrame),
}

impl DecodedServerPayload {
    fn into_messages(self) -> Vec<ServerMessage> {
        match self {
            Self::Message(msg) => vec![msg],
            Self::Frame(frame) => frame.messages,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn to_json_text(&self) -> Result<String, String> {
        match self {
            Self::Message(msg) => serde_json::to_string(msg).map_err(|err| err.to_string()),
            Self::Frame(frame) => serde_json::to_string(frame).map_err(|err| err.to_string()),
        }
    }
}

pub(super) fn decode_server_payload_bytes(data: &[u8]) -> Result<DecodedServerPayload, String> {
    codec::decode::<ServerMessage>(data)
        .map(DecodedServerPayload::Message)
        .or_else(|_| codec::decode::<ServerFrame>(data).map(DecodedServerPayload::Frame))
        .or_else(|_| {
            serde_json::from_slice::<ServerMessage>(data)
                .map(DecodedServerPayload::Message)
                .map_err(|err| err.to_string())
        })
        .or_else(|_| {
            serde_json::from_slice::<ServerFrame>(data)
                .map(DecodedServerPayload::Frame)
                .map_err(|err| err.to_string())
        })
        .map_err(|err| err.to_string())
}

pub fn decode_server_messages_bytes(data: &[u8]) -> Result<Vec<ServerMessage>, String> {
    decode_server_payload_bytes(data).map(DecodedServerPayload::into_messages)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn send_framed(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    let len = data.len() as u32;
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
        if (len_buf[0] & 0xE0 == 0x80)
            || len_buf[0] == 0xDE
            || len_buf[0] == 0xDF
            || len_buf[0] == 0xDC
            || len_buf[0] == 0xDD
        {
            bevy::log::warn!(
                "Detected unframed msgpack payload on framed socket; recovering \
                (prefix=0x{:02X}{:02X}{:02X}{:02X})",
                len_buf[0],
                len_buf[1],
                len_buf[2],
                len_buf[3]
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

        if rmp_serde::from_slice::<serde::de::IgnoredAny>(&data).is_ok() {
            bevy::log::info!("Recovered unframed msgpack payload: {} bytes", data.len());
            return Ok(data);
        }

        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => data.extend_from_slice(&chunk[..n]),
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(e) => return Err(e),
        }
    }

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

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WsClientEncoding {
    Unknown,
    BinaryMsgpack,
    TextJson,
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn encode_ws_server_payload(
    data: &[u8],
    encoding: WsClientEncoding,
) -> Result<tungstenite::Message, String> {
    match encoding {
        WsClientEncoding::BinaryMsgpack => Ok(tungstenite::Message::Binary(data.to_vec().into())),
        WsClientEncoding::Unknown | WsClientEncoding::TextJson => decode_server_payload_bytes(data)
            .and_then(|payload| payload.to_json_text())
            .map(|text| tungstenite::Message::Text(text.into())),
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use game_state::message::{ClientMessage, ServerFrame, ServerMessage};
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};

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
    fn decode_server_messages_accepts_batched_frame() {
        let msg = ServerMessage::Pong {
            seq: 9,
            timestamp: 1.25,
        };
        let frame = ServerFrame {
            tick: 3,
            timestamp: 8.5,
            messages: vec![msg.clone()],
        };
        let bytes = codec::encode(&frame).unwrap();

        let decoded = decode_server_messages_bytes(&bytes).unwrap();
        assert_eq!(decoded, vec![msg]);
    }

    #[test]
    fn encode_ws_server_payload_uses_json_text_for_unknown_clients() {
        let msg = ServerMessage::Pong {
            seq: 5,
            timestamp: 2.0,
        };
        let bytes = codec::encode(&msg).unwrap();

        let encoded = encode_ws_server_payload(&bytes, WsClientEncoding::Unknown).unwrap();
        match encoded {
            tungstenite::Message::Text(text) => {
                let decoded: ServerMessage = serde_json::from_str(text.as_ref()).unwrap();
                assert_eq!(decoded, msg);
            }
            other => panic!("expected text payload, got {:?}", other),
        }
    }
}
