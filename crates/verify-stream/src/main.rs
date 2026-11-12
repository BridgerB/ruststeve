//! verify-stream: log in to a vanilla 26.1.2 server in offline mode and check that every packet
//! id it sends, in every connection state, is in `data`'s generated packet tables (issue 02's
//! done-condition: a handshake-to-play stream with zero unknown ids).
//!
//! ```text
//! cargo run -p verify-stream -- [host] [port] [seconds in play]
//! ```
//!
//! Runs on the runners only, against a server the workflow starts. It answers just enough to stay
//! in: known packs, keep-alives, pings, the teleport, chunk batches. This is scaffolding for the
//! table check; the bot's real protocol layer is issue 03 (`protocol` and `net`).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use data::generated::packets::{
    ConfigurationClientbound, ConfigurationServerbound, HandshakeServerbound, LoginClientbound,
    LoginServerbound, PlayClientbound, PlayServerbound,
};
use flate2::read::ZlibDecoder;
use md5::{Digest, Md5};

/// The player name the check logs in as.
const NAME: &str = "verify01";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    Login,
    Configuration,
    Play,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("verify-stream: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn write_varint(out: &mut Vec<u8>, value: i32) {
    let mut v = value as u32;
    loop {
        if v & !0x7f == 0 {
            out.push(v as u8);
            return;
        }
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
}

fn read_varint(r: &mut impl Read) -> Result<i32> {
    let mut value: u32 = 0;
    for i in 0..5 {
        let mut b = [0u8; 1];
        r.read_exact(&mut b)?;
        value |= u32::from(b[0] & 0x7f) << (7 * i);
        if b[0] & 0x80 == 0 {
            return Ok(value as i32);
        }
    }
    bail!("varint longer than 5 bytes")
}

fn write_string(out: &mut Vec<u8>, s: &str) -> Result<()> {
    write_varint(out, i32::try_from(s.len())?);
    out.extend_from_slice(s.as_bytes());
    Ok(())
}

/// The offline-mode UUID: MD5 of `OfflinePlayer:<name>`, as a version-3 UUID.
fn offline_uuid(name: &str) -> [u8; 16] {
    let mut h: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    h[6] = (h[6] & 0x0f) | 0x30;
    h[8] = (h[8] & 0x3f) | 0x80;
    h
}

struct Conn {
    stream: TcpStream,
    /// Set once the server sends login_compression.
    compression: Option<i32>,
}

impl Conn {
    fn send(&mut self, id: u32, body: &[u8]) -> Result<()> {
        let mut packet = Vec::new();
        write_varint(&mut packet, i32::try_from(id)?);
        packet.extend_from_slice(body);
        let mut frame = Vec::new();
        if self.compression.is_some() {
            // Our packets are all under the threshold, so they go uncompressed: data length 0.
            let mut inner = Vec::new();
            write_varint(&mut inner, 0);
            inner.extend_from_slice(&packet);
            write_varint(&mut frame, i32::try_from(inner.len())?);
            frame.extend_from_slice(&inner);
        } else {
            write_varint(&mut frame, i32::try_from(packet.len())?);
            frame.extend_from_slice(&packet);
        }
        self.stream.write_all(&frame)?;
        Ok(())
    }

    /// The next packet: its id and body.
    fn recv(&mut self) -> Result<(u32, Vec<u8>)> {
        let len = usize::try_from(read_varint(&mut self.stream)?)?;
        let mut frame = vec![0u8; len];
        self.stream.read_exact(&mut frame)?;
        let mut cursor = &frame[..];
        let packet = if self.compression.is_some() {
            let data_len = usize::try_from(read_varint(&mut cursor)?)?;
            if data_len == 0 {
                cursor.to_vec()
            } else {
                let mut out = Vec::with_capacity(data_len);
                ZlibDecoder::new(cursor).read_to_end(&mut out)?;
                out
            }
        } else {
            cursor.to_vec()
        };
        let mut body = &packet[..];
        let id = u32::try_from(read_varint(&mut body)?)?;
        Ok((id, body.to_vec()))
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let host = args.next().unwrap_or_else(|| "127.0.0.1".to_owned());
    let port: u16 = args.next().map_or(Ok(25565), |p| p.parse())?;
    let play_secs: u64 = args.next().map_or(Ok(20), |s| s.parse())?;

    let stream = TcpStream::connect((host.as_str(), port))
        .with_context(|| format!("connect {host}:{port}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut conn = Conn {
        stream,
        compression: None,
    };

    // Handshake: protocol, address, port, next state 2 (login).
    let mut body = Vec::new();
    write_varint(&mut body, i32::try_from(data::VERSION.protocol)?);
    write_string(&mut body, &host)?;
    body.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut body, 2);
    conn.send(HandshakeServerbound::Intention.id(), &body)?;

    let mut body = Vec::new();
    write_string(&mut body, NAME)?;
    body.extend_from_slice(&offline_uuid(NAME));
    conn.send(LoginServerbound::Hello.id(), &body)?;

    let mut state = State::Login;
    let mut seen: BTreeMap<(State, &'static str), u32> = BTreeMap::new();
    let mut unknown: BTreeMap<(State, u32), u32> = BTreeMap::new();
    let mut play_since: Option<Instant> = None;

    loop {
        if let Some(t) = play_since
            && t.elapsed() >= Duration::from_secs(play_secs)
        {
            break;
        }
        let (id, body) = conn.recv().with_context(|| format!("read in {state:?}"))?;
        let name = match state {
            State::Login => LoginClientbound::from_id(id).map(LoginClientbound::name),
            State::Configuration => {
                ConfigurationClientbound::from_id(id).map(ConfigurationClientbound::name)
            }
            State::Play => PlayClientbound::from_id(id).map(PlayClientbound::name),
        };
        let Some(name) = name else {
            *unknown.entry((state, id)).or_default() += 1;
            continue;
        };
        *seen.entry((state, name)).or_default() += 1;

        match state {
            State::Login => match LoginClientbound::from_id(id) {
                Some(LoginClientbound::LoginCompression) => {
                    conn.compression = Some(read_varint(&mut &body[..])?);
                }
                Some(LoginClientbound::LoginFinished) => {
                    conn.send(LoginServerbound::LoginAcknowledged.id(), &[])?;
                    state = State::Configuration;
                }
                Some(LoginClientbound::LoginDisconnect) => {
                    bail!("disconnected in login: {}", String::from_utf8_lossy(&body))
                }
                _ => {}
            },
            State::Configuration => match ConfigurationClientbound::from_id(id) {
                Some(ConfigurationClientbound::SelectKnownPacks) => {
                    // No packs known: the server sends every registry in full.
                    conn.send(ConfigurationServerbound::SelectKnownPacks.id(), &[0])?;
                }
                Some(ConfigurationClientbound::KeepAlive) => {
                    conn.send(ConfigurationServerbound::KeepAlive.id(), &body)?;
                }
                Some(ConfigurationClientbound::Ping) => {
                    conn.send(ConfigurationServerbound::Pong.id(), &body)?;
                }
                Some(ConfigurationClientbound::FinishConfiguration) => {
                    conn.send(ConfigurationServerbound::FinishConfiguration.id(), &[])?;
                    state = State::Play;
                    play_since = Some(Instant::now());
                }
                Some(ConfigurationClientbound::Disconnect) => {
                    bail!("disconnected in configuration")
                }
                _ => {}
            },
            State::Play => match PlayClientbound::from_id(id) {
                Some(PlayClientbound::KeepAlive) => {
                    conn.send(PlayServerbound::KeepAlive.id(), &body)?;
                }
                Some(PlayClientbound::Ping) => {
                    conn.send(PlayServerbound::Pong.id(), &body)?;
                }
                Some(PlayClientbound::PlayerPosition) => {
                    // The teleport id is the packet's first field; echo it back.
                    let teleport = read_varint(&mut &body[..])?;
                    let mut ack = Vec::new();
                    write_varint(&mut ack, teleport);
                    conn.send(PlayServerbound::AcceptTeleportation.id(), &ack)?;
                    conn.send(PlayServerbound::PlayerLoaded.id(), &[])?;
                }
                Some(PlayClientbound::ChunkBatchFinished) => {
                    conn.send(
                        PlayServerbound::ChunkBatchReceived.id(),
                        &20.0f32.to_be_bytes(),
                    )?;
                }
                Some(PlayClientbound::StartConfiguration) => {
                    conn.send(PlayServerbound::ConfigurationAcknowledged.id(), &[])?;
                    state = State::Configuration;
                }
                Some(PlayClientbound::Disconnect) => bail!("disconnected in play"),
                _ => {}
            },
        }
    }

    let mut report = String::new();
    for ((state, name), n) in &seen {
        report.push_str(&format!("{state:?} {name} {n}\n"));
    }
    for ((state, id), n) in &unknown {
        report.push_str(&format!("UNKNOWN {state:?} id {id} seen {n}\n"));
    }
    let total: u32 = seen.values().sum();
    report.push_str(&format!(
        "packets {total}, kinds {}, unknown ids {}, play {play_secs} s\n",
        seen.len(),
        unknown.len()
    ));
    print!("{report}");
    std::fs::write("stream-report.txt", &report)?;
    ensure!(unknown.is_empty(), "{} unknown packet ids", unknown.len());
    Ok(())
}
