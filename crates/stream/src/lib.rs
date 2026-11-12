//! The live snapshot stream (schema/snapshot-v1.md, brain/ruststeve/design/snapshot-stream.md):
//! a bot's world, pushed out to steve's local relay as MessagePack frames over one outbound
//! WebSocket, for watching it live. An observer only: the loop hands it bodies after each step
//! and never waits on it; it never feeds anything back.
//!
//! The sender thread owns the socket. It keeps the latest `self` and every loaded chunk, so a
//! reconnect (backoff 1 s doubling to 30 s) sends `hello`, `self` and the chunks again. A full
//! channel drops the frame (counted); a frame with no connection is counted as failed.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// The schema version every frame carries.
pub const VERSION: u16 = 1;
/// Frames the loop may get ahead of the sender before frames are dropped.
const CHANNEL: usize = 1024;
const BACKOFF_FIRST: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// A task on the bot's stack, top first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub name: String,
    pub why: String,
}

/// An entity as the bot sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Seen {
    pub id: i32,
    pub kind: String,
    pub pos: [f64; 3],
    pub hostile: bool,
}

/// What a frame says, by kind (the envelope is added by the sender).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Body {
    Hello {
        commit: String,
        mc_version: String,
        seed: u64,
        lane: u32,
        start: String,
        stage: String,
        source: String,
        server: String,
    },
    SelfState {
        pos: [f64; 3],
        yaw: f32,
        pitch: f32,
        dim: String,
        on_ground: bool,
        health: f32,
        food: i32,
        air: i32,
        held: String,
        inv: BTreeMap<String, i64>,
        stage: String,
        tasks: Vec<Task>,
        reason: String,
    },
    Chunk {
        x: i32,
        z: i32,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
    Entities {
        list: Vec<Seen>,
    },
    Block {
        x: i32,
        y: i32,
        z: i32,
        state: u32,
    },
    Event {
        name: String,
        reason: String,
    },
    End {
        ending: String,
        killer: String,
        stage_reached: String,
        case: String,
    },
    Unload {
        x: i32,
        z: i32,
    },
    Gone {
        id: i32,
    },
}

impl Body {
    pub fn kind(&self) -> &'static str {
        match self {
            Body::Hello { .. } => "hello",
            Body::SelfState { .. } => "self",
            Body::Chunk { .. } => "chunk",
            Body::Unload { .. } => "unload",
            Body::Block { .. } => "block",
            Body::Entities { .. } => "entities",
            Body::Gone { .. } => "gone",
            Body::Event { .. } => "event",
            Body::End { .. } => "end",
        }
    }
}

/// A whole frame: the envelope and its body's fields, one map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub v: u16,
    pub kind: String,
    pub bot: String,
    pub run: String,
    pub tick: u64,
    pub seq: u64,
    #[serde(flatten)]
    pub body: Body,
}

impl Frame {
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        rmp_serde::to_vec_named(self).map_err(|e| e.to_string())
    }
}

/// What the sink counts, for the run's case.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub sent: u64,
    pub dropped: u64,
    pub failed: u64,
    pub connects: u64,
}

#[derive(Default)]
struct Counters {
    sent: AtomicU64,
    dropped: AtomicU64,
    failed: AtomicU64,
    connects: AtomicU64,
}

/// A message to the sender thread.
enum Msg {
    Body { tick: u64, body: Body },
    Stop,
}

/// The loop's handle on the stream.
pub struct Sink {
    tx: SyncSender<Msg>,
    counters: Arc<Counters>,
    thread: Option<JoinHandle<()>>,
}

/// Who is streaming: the envelope's fixed fields and the hello.
#[derive(Clone, Debug)]
pub struct Identity {
    pub bot: String,
    pub run: String,
    /// The `hello` body, sent first on every connection.
    pub hello: Body,
}

impl Sink {
    /// Start streaming to `relay` (`ws://localhost:8787`) at `/ingest/<run>-<bot>`.
    pub fn start(relay: &str, id: Identity) -> Self {
        let (tx, rx) = sync_channel(CHANNEL);
        let counters = Arc::new(Counters::default());
        let url = format!(
            "{}/ingest/{}-{}",
            relay.trim_end_matches('/'),
            id.run,
            id.bot
        );
        let c = Arc::clone(&counters);
        let thread = std::thread::spawn(move || sender(&url, &id, &rx, &c));
        Self {
            tx,
            counters,
            thread: Some(thread),
        }
    }

    /// Hand a body to the sender; never blocks. A full channel drops it.
    pub fn send(&self, tick: u64, body: Body) {
        match self.tx.try_send(Msg::Body { tick, body }) {
            Ok(()) => {}
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.counters.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn counts(&self) -> Counts {
        Counts {
            sent: self.counters.sent.load(Ordering::Relaxed),
            dropped: self.counters.dropped.load(Ordering::Relaxed),
            failed: self.counters.failed.load(Ordering::Relaxed),
            connects: self.counters.connects.load(Ordering::Relaxed),
        }
    }

    /// Send what is queued (the `end` frame last) and stop; the counts at the end.
    pub fn finish(mut self) -> Counts {
        // Blocking here is fine: the run is over.
        let _ = self.tx.send(Msg::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        self.counts()
    }
}

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

/// The sender's memory: what a fresh connection must be told again.
#[derive(Default)]
struct Mirror {
    last_self: Option<(u64, Body)>,
    chunks: BTreeMap<(i32, i32), (u64, Body)>,
}

impl Mirror {
    fn note(&mut self, tick: u64, body: &Body) {
        match body {
            Body::SelfState { .. } => self.last_self = Some((tick, body.clone())),
            Body::Chunk { x, z, .. } => {
                self.chunks.insert((*x, *z), (tick, body.clone()));
            }
            Body::Unload { x, z } => {
                self.chunks.remove(&(*x, *z));
            }
            _ => {}
        }
    }
}

struct Conn {
    ws: Socket,
    seq: u64,
}

fn write(conn: &mut Conn, id: &Identity, tick: u64, body: Body) -> Result<(), String> {
    let frame = Frame {
        v: VERSION,
        kind: body.kind().to_owned(),
        bot: id.bot.clone(),
        run: id.run.clone(),
        tick,
        seq: conn.seq,
        body,
    };
    let bytes = frame.encode()?;
    conn.ws
        .send(Message::Binary(bytes.into()))
        .map_err(|e| e.to_string())?;
    conn.seq += 1;
    Ok(())
}

/// Connect and send `hello`, the latest `self` and every loaded chunk.
fn connect(url: &str, id: &Identity, mirror: &Mirror, c: &Counters) -> Result<Conn, String> {
    let (ws, _) = tungstenite::connect(url).map_err(|e| e.to_string())?;
    let mut conn = Conn { ws, seq: 0 };
    let tick = mirror.last_self.as_ref().map_or(0, |(t, _)| *t);
    write(&mut conn, id, tick, id.hello.clone())?;
    let replay = mirror.last_self.iter().chain(mirror.chunks.values());
    for (t, body) in replay {
        write(&mut conn, id, *t, body.clone())?;
    }
    c.connects.fetch_add(1, Ordering::Relaxed);
    c.sent.fetch_add(
        1 + mirror.chunks.len() as u64 + u64::from(mirror.last_self.is_some()),
        Ordering::Relaxed,
    );
    Ok(conn)
}

fn sender(url: &str, id: &Identity, rx: &Receiver<Msg>, c: &Counters) {
    let mut mirror = Mirror::default();
    let mut conn: Option<Conn> = None;
    let mut backoff = BACKOFF_FIRST;
    let mut next_try = Instant::now();
    loop {
        if conn.is_none() && Instant::now() >= next_try {
            match connect(url, id, &mirror, c) {
                Ok(k) => {
                    conn = Some(k);
                    backoff = BACKOFF_FIRST;
                }
                Err(_) => {
                    next_try = Instant::now() + backoff;
                    backoff = (backoff * 2).min(BACKOFF_MAX);
                }
            }
        }
        let msg = match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(m) => m,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => Msg::Stop,
        };
        let (tick, body) = match msg {
            Msg::Body { tick, body } => (tick, body),
            Msg::Stop => {
                if let Some(mut k) = conn.take() {
                    let _ = k.ws.close(None);
                    let _ = k.ws.flush();
                }
                return;
            }
        };
        mirror.note(tick, &body);
        match conn.as_mut() {
            Some(k) => match write(k, id, tick, body) {
                Ok(()) => {
                    c.sent.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    c.failed.fetch_add(1, Ordering::Relaxed);
                    conn = None;
                    next_try = Instant::now() + backoff;
                }
            },
            None => {
                c.failed.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}
