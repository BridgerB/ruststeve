//! Recordings (brain/ruststeve/design/recordings.md): every run as a file that replays exactly,
//! at its own commit. A zstd stream of length-prefixed MessagePack records with named fields
//! (brain/ruststeve/decisions/recording-format.md). The format grows by accretion: add variants
//! and fields, never rename or repurpose one.
//!
//! What is recorded: each server packet as received (state, id, bytes; replay decodes it again),
//! each tick, and each step's effects as encoded packets; a checkpoint of the whole `World` every
//! 1,200 server ticks.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use protocol::{Out, S2c, State, decode};
use serde::{Deserialize, Serialize};
use world::{Effect, Event, World};

/// The recording format's version. A reader refuses a version it does not know.
pub const FORMAT_VERSION: u32 = 1;

/// The commit this code was built from (`build.rs`), and whether the tree had local changes.
pub const COMMIT: &str = env!("RUSTSTEVE_COMMIT");
pub const DIRTY: &str = env!("RUSTSTEVE_DIRTY");
pub const TARGET: &str = env!("RUSTSTEVE_TARGET");

/// Server ticks between checkpoints: a minute of game time.
pub const CHECKPOINT_TICKS: u64 = 1200;

/// zstd level: the default, a good ratio at a few hundred MB/s.
const ZSTD_LEVEL: i32 = 3;

/// The first record of every file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub format_version: u32,
    /// The full sha of the code that made the recording.
    pub commit: String,
    /// Whether that code had uncommitted changes.
    pub dirty: bool,
    pub minecraft_version: String,
    pub protocol: u32,
    pub bot_name: String,
    /// `host:port`, a label only.
    pub server: String,
    /// The shell's seed for this run.
    pub seed: u64,
    pub target_triple: String,
    /// The file this one continues, after a reconnect.
    #[serde(default)]
    pub previous_segment: Option<String>,
}

impl Header {
    /// A header for a run of this build.
    pub fn new(bot_name: &str, server: &str, seed: u64) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            commit: COMMIT.to_owned(),
            dirty: DIRTY == "true",
            minecraft_version: data::VERSION.id.to_owned(),
            protocol: data::VERSION.protocol,
            bot_name: bot_name.to_owned(),
            server: server.to_owned(),
            seed,
            target_triple: TARGET.to_owned(),
            previous_segment: None,
        }
    }
}

/// An input to the world, as it arrived.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum EventRecord {
    /// A server packet, decompressed: the state it was read in, its id and its fields.
    Packet {
        state: State,
        id: u32,
        #[serde(with = "serde_bytes")]
        body: Vec<u8>,
    },
    /// The loop's clock and a random seed.
    Tick { now_ms: u64, seed: u64 },
    /// What to press from now on (added in issue 06; older files simply have none).
    Controls(physics::Controls),
}

/// An output of the world: a packet as it is written on the wire, or the compression switch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectRecord {
    Send {
        id: u32,
        #[serde(with = "serde_bytes")]
        body: Vec<u8>,
    },
    SetCompression(i32),
}

impl EffectRecord {
    /// The effect as recorded: a packet by its encoded bytes, so `verify` compares exactly what
    /// went on the wire.
    pub fn of(effect: &Effect) -> Result<Self, String> {
        Ok(match effect {
            Effect::Send(p) => {
                let (id, body) = p.encode().map_err(|e| e.to_string())?;
                EffectRecord::Send { id, body }
            }
            Effect::SetCompression(t) => EffectRecord::SetCompression(*t),
        })
    }
}

/// One record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Record {
    Header(Header),
    Event(EventRecord),
    /// What the step of the event just before produced, in order.
    Effects(Vec<EffectRecord>),
    /// The whole world after `event_index` events (counted from 0).
    Checkpoint {
        event_index: u64,
        server_ticks: u64,
        world: Box<World>,
    },
    Note(String),
}

/// Writes a recording. Every record is length-prefixed MessagePack inside one zstd stream; a
/// zstd block is flushed at every checkpoint and at least once a second, so a killed run loses
/// at most a second of records.
pub struct Recorder {
    out: zstd::Encoder<'static, BufWriter<File>>,
    last_flush: Instant,
}

impl Recorder {
    pub fn create(path: &Path, header: &Header) -> Result<Self, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        }
        let file = File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
        let out =
            zstd::Encoder::new(BufWriter::new(file), ZSTD_LEVEL).map_err(|e| e.to_string())?;
        let mut r = Self {
            out,
            last_flush: Instant::now(),
        };
        r.write(&Record::Header(header.clone()))?;
        Ok(r)
    }

    pub fn write(&mut self, record: &Record) -> Result<(), String> {
        let bytes = rmp_serde::to_vec_named(record).map_err(|e| e.to_string())?;
        let len = u32::try_from(bytes.len()).map_err(|_| "a record over 4 GiB".to_owned())?;
        self.out
            .write_all(&len.to_be_bytes())
            .and_then(|()| self.out.write_all(&bytes))
            .map_err(|e| e.to_string())?;
        if matches!(record, Record::Checkpoint { .. })
            || self.last_flush.elapsed() >= Duration::from_secs(1)
        {
            self.out.flush().map_err(|e| e.to_string())?;
            self.last_flush = Instant::now();
        }
        Ok(())
    }

    /// End the zstd stream.
    pub fn finish(self) -> Result<(), String> {
        let mut inner = self.out.finish().map_err(|e| e.to_string())?;
        inner.flush().map_err(|e| e.to_string())
    }
}

/// Reads a recording, record by record.
pub struct Recording {
    input: zstd::Decoder<'static, BufReader<File>>,
}

impl Recording {
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
        let input = zstd::Decoder::new(file).map_err(|e| e.to_string())?;
        Ok(Self { input })
    }

    /// The next record; `Ok(None)` at the end. A file cut short (a killed run) ends cleanly at
    /// the last whole record.
    pub fn next_record(&mut self) -> Result<Option<Record>, String> {
        let mut len = [0u8; 4];
        match self.input.read_exact(&mut len) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.to_string()),
        }
        let mut bytes = vec![0u8; u32::from_be_bytes(len) as usize];
        match self.input.read_exact(&mut bytes) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.to_string()),
        }
        rmp_serde::from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("a record that will not decode: {e}"))
    }

    /// Every record, the header first (checked for a known format version).
    pub fn read_all(path: &Path) -> Result<(Header, Vec<Record>), String> {
        let mut r = Self::open(path)?;
        let header = match r.next_record()? {
            Some(Record::Header(h)) => h,
            Some(other) => return Err(format!("the first record is not a header: {other:?}")),
            None => return Err("an empty recording".to_owned()),
        };
        if header.format_version != FORMAT_VERSION {
            return Err(format!(
                "recording format {} (this build reads {FORMAT_VERSION})",
                header.format_version
            ));
        }
        let mut records = Vec::new();
        while let Some(rec) = r.next_record()? {
            records.push(rec);
        }
        Ok((header, records))
    }
}

/// Refuse a recording from another commit unless forced: a recording folded with newer code
/// drifts, because the bot's own choices move it (the handoff's lesson 4.5).
pub fn check_commit(header: &Header, own: &str, force: bool) -> Result<(), String> {
    if header.commit == own || force {
        return Ok(());
    }
    Err(format!(
        "this recording was made at commit {} and this build is {own}; check out its commit to \
         replay it, or pass --force to fold it with this code anyway",
        header.commit
    ))
}

/// Folds a recording's events through a world, decoding each packet again in the state the
/// net reader would have decoded it in.
pub struct Folder {
    pub world: World,
    /// The decode state: moves exactly as the reader's does (`State::after_receive`).
    pub state: State,
    pub events: u64,
}

impl Folder {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            state: State::Login,
            events: 0,
        }
    }

    /// Resume from a checkpointed world: its session's state is the reader's state.
    pub fn from_checkpoint(world: World, events: u64) -> Self {
        let state = world.session.state;
        Self {
            world,
            state,
            events,
        }
    }

    /// Fold one event; return the effects, as recorded.
    pub fn step(&mut self, e: &EventRecord) -> Result<Vec<EffectRecord>, String> {
        let event = match e {
            EventRecord::Tick { now_ms, seed } => Event::Tick {
                now_ms: *now_ms,
                seed: *seed,
            },
            EventRecord::Controls(c) => Event::Controls(*c),
            EventRecord::Packet { state, id, body } => {
                if *state != self.state {
                    return Err(format!(
                        "event {} was read in {state:?}, the fold is in {:?}",
                        self.events, self.state
                    ));
                }
                let p: S2c = decode(*state, *id, body)
                    .map_err(|err| format!("event {} no longer decodes: {err}", self.events))?;
                self.state = self.state.after_receive(&p);
                Event::Packet(p)
            }
        };
        self.events += 1;
        self.world
            .step(&event)
            .iter()
            .map(EffectRecord::of)
            .collect()
    }
}

impl Default for Folder {
    fn default() -> Self {
        Self::new()
    }
}

/// A world effect as the shell performs it (the net writer's `Out`).
pub fn as_out(e: &Effect) -> Out {
    match e {
        Effect::Send(p) => Out::Send(p.clone()),
        Effect::SetCompression(t) => Out::SetCompression(*t),
    }
}
