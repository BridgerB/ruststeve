//! The socket. A reader thread turns bytes into typed packets (framing, inflating and decoding
//! off the loop thread, brain/wire/wire-framing.md) and a writer thread turns `Out`s into frames.
//! Both talk to the loop over bounded channels. No async runtime.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread::JoinHandle;

use protocol::{FrameDecoder, Out, ParseError, S2c, State, decode, encode_frame, split_id};

/// Packets the reader may have decoded ahead of the loop: a burst of chunks fits; past it the
/// reader waits rather than buffer without bound.
const INBOUND_CAPACITY: usize = 4096;
/// Packets the loop may queue for the writer.
const OUTBOUND_CAPACITY: usize = 1024;
/// Bytes read from the socket at a time.
const READ_CHUNK: usize = 64 * 1024;

/// A packet as it arrived: the state it was read in, its id and its fields (decompressed). What
/// a recording keeps, so replay decodes it again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Raw {
    pub state: State,
    pub id: u32,
    pub body: Vec<u8>,
}

/// What the reader thread reports. Errors are values: nothing is dropped silently.
#[derive(Debug)]
pub enum Inbound {
    Packet {
        packet: S2c,
        raw: Raw,
    },
    /// A frame that would not decode as its modelled packet; the stream continues.
    DecodeError {
        state: State,
        id: u32,
        error: ParseError,
    },
    /// The connection is over: end of stream, a socket error, or a malformed stream.
    Closed(String),
}

/// A live connection: packets from the server, and a queue to it.
pub struct Connection {
    pub inbound: Receiver<Inbound>,
    pub outbound: SyncSender<Out>,
    pub reader: JoinHandle<()>,
    pub writer: JoinHandle<()>,
}

/// Connect to `addr` (`host:port`) and start the reader and writer threads. The reader decodes
/// in login state: the caller's first packets must be the handshake and the hello.
pub fn connect(addr: &str) -> std::io::Result<Connection> {
    let stream = TcpStream::connect(addr)?;
    stream.set_nodelay(true)?;
    let read_half = stream.try_clone()?;
    let (in_tx, inbound) = sync_channel(INBOUND_CAPACITY);
    let (outbound, out_rx) = sync_channel::<Out>(OUTBOUND_CAPACITY);
    let reader = std::thread::Builder::new()
        .name("net-reader".to_owned())
        .spawn(move || read_loop(read_half, &in_tx))?;
    let writer = std::thread::Builder::new()
        .name("net-writer".to_owned())
        .spawn(move || write_loop(stream, &out_rx))?;
    Ok(Connection {
        inbound,
        outbound,
        reader,
        writer,
    })
}

fn read_loop(mut stream: TcpStream, tx: &SyncSender<Inbound>) {
    let reason = read_packets(&mut stream, tx);
    let _ = stream.shutdown(Shutdown::Both);
    let _ = tx.send(Inbound::Closed(reason));
}

/// Read until the stream ends; return why it ended.
fn read_packets(stream: &mut TcpStream, tx: &SyncSender<Inbound>) -> String {
    let mut decoder = FrameDecoder::new();
    // The server answers the hello in login state.
    let mut state = State::Login;
    let mut buf = vec![0u8; READ_CHUNK];
    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) => return "the server closed the connection".to_owned(),
            Ok(n) => n,
            Err(e) => return format!("read: {e}"),
        };
        decoder.push(buf.get(..n).unwrap_or_default());
        loop {
            let packet = match decoder.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => break,
                Err(e) => return format!("malformed stream in {state:?}: {e}"),
            };
            let (id, body) = match split_id(&packet) {
                Ok(x) => x,
                Err(e) => return format!("malformed packet id in {state:?}: {e}"),
            };
            let msg = match decode(state, id, body) {
                Ok(p) => {
                    let raw = Raw {
                        state,
                        id,
                        body: body.to_vec(),
                    };
                    // Compression applies to every frame after this one, including frames
                    // already buffered, so it is switched before the next is taken.
                    if let S2c::LoginCompression { threshold } = p {
                        decoder.set_compression(threshold);
                    }
                    state = state.after_receive(&p);
                    Inbound::Packet { packet: p, raw }
                }
                Err(error) => Inbound::DecodeError { state, id, error },
            };
            if tx.send(msg).is_err() {
                return "the loop stopped listening".to_owned();
            }
        }
    }
}

fn write_loop(mut stream: TcpStream, rx: &Receiver<Out>) {
    let mut compression = None;
    for out in rx {
        match out {
            Out::SetCompression(t) => compression = (t >= 0).then_some(t),
            Out::Send(packet) => {
                let frame = packet
                    .encode()
                    .and_then(|(id, body)| encode_frame(id, &body, compression));
                let frame = match frame {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("net: cannot encode {packet:?}: {e}");
                        break;
                    }
                };
                if let Err(e) = stream.write_all(&frame) {
                    eprintln!("net: write: {e}");
                    break;
                }
            }
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}
