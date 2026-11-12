//! A Source RCON client (the protocol vanilla's `enable-rcon` speaks): the race uses it on its
//! own server only, to give a start's kit and place a bot (brain/ruststeve/design/race-and-results.md).
//! Never on a shared server, never to change a world it does not own.
//!
//! A packet is a little-endian i32 length, then an i32 request id, an i32 type, the body and two
//! zero bytes. Login is type 3, a command type 2; the server answers with type 0 (or 2 for the
//! login), echoing the id, or -1 for a refused login.

#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const LOGIN: i32 = 3;
const COMMAND: i32 = 2;
/// Vanilla refuses packets over 1446 bytes of body; a command is far shorter.
const MAX_BODY: usize = 1446;
/// A reply larger than this is not RCON.
const MAX_REPLY: usize = 1 << 20;

pub struct Rcon {
    stream: TcpStream,
    next_id: i32,
}

fn encode(id: i32, kind: i32, body: &str) -> Vec<u8> {
    let len = i32::try_from(4 + 4 + body.len() + 2).unwrap_or(i32::MAX);
    let mut out = Vec::with_capacity(body.len() + 14);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(body.as_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

/// One reply: its id, type and body text.
fn decode(buf: &[u8]) -> Option<(i32, i32, String)> {
    let id = i32::from_le_bytes(buf.get(0..4)?.try_into().ok()?);
    let kind = i32::from_le_bytes(buf.get(4..8)?.try_into().ok()?);
    let body = buf.get(8..buf.len().checked_sub(2)?)?;
    Some((id, kind, String::from_utf8_lossy(body).into_owned()))
}

impl Rcon {
    /// Connect and log in.
    pub fn connect(addr: &str, password: &str) -> Result<Self, String> {
        let stream = TcpStream::connect(addr).map_err(|e| format!("rcon connect {addr}: {e}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(|e| e.to_string())?;
        let mut r = Self { stream, next_id: 1 };
        let (id, _, _) = r.exchange(LOGIN, password)?;
        if id == -1 {
            return Err("rcon: the password was refused".to_owned());
        }
        Ok(r)
    }

    /// Run a command; the server's reply text.
    pub fn command(&mut self, command: &str) -> Result<String, String> {
        let (_, _, body) = self.exchange(COMMAND, command)?;
        Ok(body)
    }

    fn exchange(&mut self, kind: i32, body: &str) -> Result<(i32, i32, String), String> {
        if body.len() > MAX_BODY {
            return Err(format!("rcon: a body of {} bytes is too long", body.len()));
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.stream
            .write_all(&encode(id, kind, body))
            .map_err(|e| format!("rcon write: {e}"))?;
        let mut len = [0u8; 4];
        self.stream
            .read_exact(&mut len)
            .map_err(|e| format!("rcon read: {e}"))?;
        let n = usize::try_from(i32::from_le_bytes(len)).map_err(|_| "rcon: a negative length")?;
        if !(10..=MAX_REPLY).contains(&n) {
            return Err(format!("rcon: a reply of {n} bytes"));
        }
        let mut buf = vec![0u8; n];
        self.stream
            .read_exact(&mut buf)
            .map_err(|e| format!("rcon read: {e}"))?;
        decode(&buf).ok_or_else(|| "rcon: a malformed reply".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packet_round_trips() {
        let p = encode(7, COMMAND, "give rs-001 minecraft:oak_log 4");
        assert_eq!(
            usize::try_from(i32::from_le_bytes([p[0], p[1], p[2], p[3]])).ok(),
            Some(p.len() - 4)
        );
        assert_eq!(
            decode(&p[4..]),
            Some((7, COMMAND, "give rs-001 minecraft:oak_log 4".to_owned()))
        );
    }

    #[test]
    fn a_short_reply_is_refused() {
        assert_eq!(decode(&[1, 0, 0]), None);
    }
}
