//! One control request: a TCP connection per request, a 16-byte header, the token, the JSON
//! body; the reply has the same header with a status byte and a body.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;

pub const MAX_CONTROL_BYTES: usize = 64 * 1024;
pub const WIRE_SCHEME: &str = "qnc-player+tcp://";
const WIRE_MAGIC: &[u8; 8] = b"QNCPB01\0";
const WIRE_VERSION: u8 = 1;
const WIRE_KIND_CONTROL: u8 = 1;
const WIRE_STATUS_OK: u8 = 0;
const WIRE_HEADER_BYTES: usize = 16;
const WIRE_MAX_TOKEN_BYTES: usize = 4096;
const TIMEOUT: Duration = Duration::from_secs(2);

/// Sends one request to the player's control wire and reads its reply.
pub fn post<Q: Serialize, R: DeserializeOwned>(wire_url: &str, token: &str, request: &Q) -> Result<R, String> {
    if token.is_empty() || token.len() > WIRE_MAX_TOKEN_BYTES || !token.bytes().all(|b| (33..=126).contains(&b)) {
        return Err("invalid player wire token".into());
    }
    let address = wire_url.strip_prefix(WIRE_SCHEME).ok_or("unsupported player wire protocol")?;
    let body = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    if body.len() > MAX_CONTROL_BYTES {
        return Err("player wire request exceeds limit".into());
    }
    let mut header = [0u8; WIRE_HEADER_BYTES];
    header[..8].copy_from_slice(WIRE_MAGIC);
    header[8] = WIRE_VERSION;
    header[9] = WIRE_KIND_CONTROL;
    header[10..12].copy_from_slice(&(token.len() as u16).to_le_bytes());
    header[12..16].copy_from_slice(&(body.len() as u32).to_le_bytes());

    let mut stream = TcpStream::connect(address).map_err(|e| io_error("connect", e))?;
    stream.set_nodelay(true).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(TIMEOUT)).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(TIMEOUT)).map_err(|e| e.to_string())?;
    let mut request = Vec::with_capacity(WIRE_HEADER_BYTES + token.len() + body.len());
    request.extend_from_slice(&header);
    request.extend_from_slice(token.as_bytes());
    request.extend_from_slice(&body);
    stream.write_all(&request).map_err(|e| io_error("write", e))?;
    stream.flush().map_err(|e| io_error("flush", e))?;

    let mut response = [0u8; WIRE_HEADER_BYTES];
    stream.read_exact(&mut response).map_err(|e| io_error("read header", e))?;
    if &response[..8] != WIRE_MAGIC || response[8] != WIRE_VERSION {
        return Err("player wire protocol mismatch".into());
    }
    let status = response[9];
    let body_len = u32::from_le_bytes(response[12..16].try_into().expect("4 bytes")) as usize;
    if body_len > MAX_CONTROL_BYTES {
        return Err("player wire response exceeds limit".into());
    }
    let mut reply = vec![0u8; body_len];
    stream.read_exact(&mut reply).map_err(|e| io_error("read body", e))?;
    if status != WIRE_STATUS_OK {
        let message = String::from_utf8_lossy(&reply).trim().to_string();
        return Err(if message.is_empty() { format!("player wire status {status}") } else { message });
    }
    serde_json::from_slice(&reply).map_err(|e| e.to_string())
}

fn io_error(op: &str, error: std::io::Error) -> String {
    match (error.kind(), error.raw_os_error()) {
        (_, Some(10053 | 10054 | 104)) => format!("player control connection closed ({op})"),
        (std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock, _) => format!("player control {op} timed out"),
        _ => format!("player control {op} failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A listener that answers one framed request as the player does.
    fn answer_once(status: u8, body: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut header = [0u8; WIRE_HEADER_BYTES];
            stream.read_exact(&mut header).unwrap();
            assert_eq!(&header[..8], WIRE_MAGIC);
            let token_len = u16::from_le_bytes([header[10], header[11]]) as usize;
            let body_len = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;
            let mut rest = vec![0u8; token_len + body_len];
            stream.read_exact(&mut rest).unwrap();
            assert_eq!(&rest[..token_len], b"tok");
            let mut reply = header;
            reply[9] = status;
            reply[12..16].copy_from_slice(&(body.len() as u32).to_le_bytes());
            stream.write_all(&reply).unwrap();
            stream.write_all(body).unwrap();
        });
        format!("{WIRE_SCHEME}{address}")
    }

    #[test]
    fn a_request_is_framed_with_the_token_and_a_status_other_than_ok_is_the_error() {
        let url = answer_once(WIRE_STATUS_OK, br#"{"ok":true}"#);
        let reply: serde_json::Value = post(&url, "tok", &serde_json::json!({"State": 1})).unwrap();
        assert_eq!(reply["ok"], true);
        let url = answer_once(1, b"Player socket access denied.");
        let refused: Result<serde_json::Value, String> = post(&url, "tok", &serde_json::json!({}));
        assert_eq!(refused.unwrap_err(), "Player socket access denied.");
        assert!(post::<_, serde_json::Value>("http://x", "tok", &1).is_err());
        assert!(post::<_, serde_json::Value>("qnc-player+tcp://127.0.0.1:1", "", &1).is_err());
    }
}
