//! The vanilla server as a pure reducer (brain/ruststeve/design/server-model.md): a client packet
//! or a server tick in, the server's packets out as the wire carries them. No I/O, no clock: the
//! loop that drives it decides when a tick happens. It is clojurecraft's `sim` grown per issue:
//! each issue adds the part of the server its gameplay meets, with rows in the parity table
//! (brain/ruststeve/design/parity-table.md) before any sim number counts.
//!
//! Issue 08a models: login and configuration, the play login with the race's distances, the
//! first teleport, health, chunks around the player sent in batches and forgotten as it moves,
//! `set_time` every 20 ticks, keep-alives every 15 s of ticks and the kick when one goes
//! unanswered.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use protocol::chunk::Column;
use protocol::{C2s, EncodeError, S2c, State};

/// Ticks between `set_time` packets (MinecraftServer synchronises the clock every 20 ticks).
pub const SET_TIME_EVERY: u64 = 20;
/// Ticks between keep-alives: vanilla sends one every 15 s (ServerCommonPacketListenerImpl).
pub const KEEP_ALIVE_EVERY: u64 = 300;
/// Ticks a keep-alive may go unanswered before the kick: vanilla's 15 s limit.
pub const KEEP_ALIVE_LIMIT: u64 = 300;
/// The entity id the model gives the player.
const PLAYER_ID: i32 = 1;

/// What the model is told: a client packet, or one server tick.
#[derive(Clone, Debug, PartialEq)]
pub enum In {
    Packet(C2s),
    Tick,
}

/// A packet as the wire carries it: the state the client reads it in, its id and its fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wire {
    pub state: State,
    pub id: u32,
    pub body: Arc<Vec<u8>>,
}

/// The race config's server settings the model reads (`config/race.toml`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub view_distance: i32,
    pub simulation_distance: i32,
}

/// One chunk column the model can send: the bytes a server sent (a captured world) or the
/// encoding of a generated one, and the column decoded for the model's own rules.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelColumn {
    pub body: Arc<Vec<u8>>,
    pub column: Arc<Column>,
}

impl ModelColumn {
    /// A generated column at (x, z), encoded as a model world sends it.
    pub fn generated(x: i32, z: i32, column: Column) -> Result<Self, EncodeError> {
        let body = column.packet_body(x, z)?;
        Ok(Self {
            body: Arc::new(body),
            column: Arc::new(column),
        })
    }
}

/// The world the model holds: its columns, where the player spawns, its dimension.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelWorld {
    pub columns: BTreeMap<(i32, i32), ModelColumn>,
    pub spawn: [f64; 3],
    pub dimension: String,
    /// The dimension type's index in the synced registry (0: the overworld).
    pub dimension_type: i32,
}

/// Where the connection is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Login,
    Configuration,
    Play,
    /// Kicked or closed: nothing more is sent.
    Closed,
}

/// The model.
#[derive(Clone, Debug)]
pub struct Sim {
    config: Config,
    world: ModelWorld,
    phase: Phase,
    /// Server ticks since play began.
    pub ticks: u64,
    pub pos: [f64; 3],
    /// The teleport the client has not accepted yet.
    teleport_pending: Option<i32>,
    next_teleport: i32,
    /// Columns sent and not forgotten.
    sent: BTreeSet<(i32, i32)>,
    /// The keep-alive waiting for its answer: its id and the tick it went out.
    keep_alive: Option<(i64, u64)>,
    /// Why the model closed the connection, when it did.
    pub closed: Option<String>,
    /// What the client said that a real server would refuse or kick for; counted, never hidden.
    pub violations: Vec<String>,
}

fn wire(state: State, p: &S2c) -> Result<Wire, EncodeError> {
    let (id, body) = p.encode()?;
    Ok(Wire {
        state,
        id,
        body: Arc::new(body),
    })
}

/// The column holding a block position.
fn column_of(x: f64, z: f64) -> (i32, i32) {
    ((x.floor() as i32) >> 4, (z.floor() as i32) >> 4)
}

impl Sim {
    pub fn new(config: Config, world: ModelWorld) -> Self {
        let pos = world.spawn;
        Self {
            config,
            world,
            phase: Phase::Login,
            ticks: 0,
            pos,
            teleport_pending: None,
            next_teleport: 1,
            sent: BTreeSet::new(),
            keep_alive: None,
            closed: None,
            violations: Vec::new(),
        }
    }

    /// Whether the connection is open.
    pub fn open(&self) -> bool {
        self.phase != Phase::Closed
    }

    /// Fold one input; the packets the server sends in answer, in order.
    pub fn step(&mut self, input: &In) -> Result<Vec<Wire>, EncodeError> {
        match input {
            In::Packet(p) => self.on_packet(p),
            In::Tick => self.tick(),
        }
    }

    fn on_packet(&mut self, p: &C2s) -> Result<Vec<Wire>, EncodeError> {
        match (self.phase, p) {
            (Phase::Closed, _) => Ok(Vec::new()),
            (Phase::Login, C2s::Intention { .. }) => Ok(Vec::new()),
            (Phase::Login, C2s::Hello { username, .. }) => Ok(vec![wire(
                State::Login,
                &S2c::LoginFinished {
                    // Offline mode: the server derives the uuid from the name.
                    uuid: protocol::uuid::offline_uuid(username),
                    username: username.clone(),
                },
            )?]),
            (Phase::Login, C2s::LoginAcknowledged) => {
                self.phase = Phase::Configuration;
                Ok(vec![wire(State::Configuration, &S2c::SelectKnownPacks)?])
            }
            (Phase::Configuration, C2s::ClientInformation { .. })
            | (Phase::Configuration, C2s::ConfigKeepAlive { .. })
            | (Phase::Configuration, C2s::ConfigPong { .. }) => Ok(Vec::new()),
            // The client knows no packs: a server sends every registry here. The bot reads none
            // of them, so the model finishes configuration at once.
            (Phase::Configuration, C2s::SelectKnownPacks) => {
                Ok(vec![wire(State::Configuration, &S2c::FinishConfiguration)?])
            }
            (Phase::Configuration, C2s::FinishConfiguration) => {
                self.phase = Phase::Play;
                self.join()
            }
            (Phase::Play, C2s::AcceptTeleportation { teleport_id }) => {
                if self.teleport_pending == Some(*teleport_id) {
                    self.teleport_pending = None;
                } else {
                    self.violations
                        .push(format!("accepted teleport {teleport_id} not pending"));
                }
                Ok(Vec::new())
            }
            (Phase::Play, C2s::KeepAlive { id }) => {
                match self.keep_alive {
                    Some((want, _)) if want == *id => self.keep_alive = None,
                    _ => {
                        // Vanilla kicks a keep-alive it did not send.
                        return self.kick("keep-alive with an unknown id");
                    }
                }
                Ok(Vec::new())
            }
            (Phase::Play, C2s::MovePlayerPosRot { x, y, z, .. }) => {
                // Until a teleport is accepted the server ignores the client's moves (vanilla's
                // awaitingPositionFromClient).
                if self.teleport_pending.is_some() {
                    return Ok(Vec::new());
                }
                let to = [*x, *y, *z];
                if !to.iter().all(|v| v.is_finite()) {
                    return self.kick("invalid move player packet received");
                }
                self.pos = to;
                self.chunks_around()
            }
            (Phase::Play, C2s::PlayerLoaded)
            | (Phase::Play, C2s::ChunkBatchReceived { .. })
            | (Phase::Play, C2s::MovePlayerStatusOnly { .. })
            | (Phase::Play, C2s::Pong { .. }) => Ok(Vec::new()),
            (phase, other) => {
                self.violations.push(format!("{other:?} in {phase:?}"));
                Ok(Vec::new())
            }
        }
    }

    /// The play login, the first teleport, health, the first chunks.
    fn join(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = vec![wire(
            State::Play,
            &S2c::Login {
                entity_id: PLAYER_ID,
                view_distance: self.config.view_distance,
                simulation_distance: self.config.simulation_distance,
                dimension_type: self.world.dimension_type,
                dimension: self.world.dimension.clone(),
                game_type: 0,
            },
        )?];
        out.extend(self.chunks_around()?);
        out.push(self.teleport(self.pos)?);
        out.push(wire(
            State::Play,
            &S2c::SetHealth {
                health: 20.0,
                food: 20,
                saturation: 5.0,
            },
        )?);
        Ok(out)
    }

    fn teleport(&mut self, to: [f64; 3]) -> Result<Wire, EncodeError> {
        let teleport_id = self.next_teleport;
        self.next_teleport += 1;
        self.teleport_pending = Some(teleport_id);
        self.pos = to;
        wire(
            State::Play,
            &S2c::PlayerPosition {
                teleport_id,
                x: to[0],
                y: to[1],
                z: to[2],
                yaw: 0.0,
                pitch: 0.0,
                flags: 0,
            },
        )
    }

    /// Columns within the view distance of the player that the model holds and has not sent go
    /// out as one batch; sent columns now outside it are forgotten.
    fn chunks_around(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let (cx, cz) = column_of(self.pos[0], self.pos[2]);
        let r = self.config.view_distance;
        let near = |(x, z): (i32, i32)| (x - cx).abs() <= r && (z - cz).abs() <= r;
        let mut out = Vec::new();
        let gone: Vec<(i32, i32)> = self.sent.iter().copied().filter(|c| !near(*c)).collect();
        for (x, z) in gone {
            self.sent.remove(&(x, z));
            out.push(wire(State::Play, &S2c::ForgetLevelChunk { x, z })?);
        }
        let new: Vec<(i32, i32)> = self
            .world
            .columns
            .keys()
            .copied()
            .filter(|c| near(*c) && !self.sent.contains(c))
            .collect();
        if new.is_empty() {
            return Ok(out);
        }
        let start = data::generated::packets::PlayClientbound::ChunkBatchStart.id();
        out.push(Wire {
            state: State::Play,
            id: start,
            body: Arc::new(Vec::new()),
        });
        let level_chunk = data::generated::packets::PlayClientbound::LevelChunkWithLight.id();
        for c in &new {
            if let Some(col) = self.world.columns.get(c) {
                out.push(Wire {
                    state: State::Play,
                    id: level_chunk,
                    body: Arc::clone(&col.body),
                });
                self.sent.insert(*c);
            }
        }
        let size = i32::try_from(new.len()).unwrap_or(i32::MAX);
        out.push(wire(State::Play, &S2c::ChunkBatchFinished { size })?);
        Ok(out)
    }

    fn tick(&mut self) -> Result<Vec<Wire>, EncodeError> {
        if self.phase != Phase::Play {
            return Ok(Vec::new());
        }
        self.ticks += 1;
        let mut out = Vec::new();
        if self.ticks.is_multiple_of(SET_TIME_EVERY) {
            out.push(wire(State::Play, &S2c::SetTime)?);
        }
        if let Some((_, at)) = self.keep_alive
            && self.ticks - at >= KEEP_ALIVE_LIMIT
        {
            return self.kick("timed out");
        }
        if self.ticks.is_multiple_of(KEEP_ALIVE_EVERY) && self.keep_alive.is_none() {
            let id = i64::try_from(self.ticks).unwrap_or(i64::MAX);
            self.keep_alive = Some((id, self.ticks));
            out.push(wire(State::Play, &S2c::KeepAlive { id })?);
        }
        Ok(out)
    }

    fn kick(&mut self, why: &str) -> Result<Vec<Wire>, EncodeError> {
        self.phase = Phase::Closed;
        self.closed = Some(why.to_owned());
        // The reason as an NBT string tag, as a server sends a plain one.
        let mut reason = vec![8];
        let n = u16::try_from(why.len()).unwrap_or(u16::MAX);
        reason.extend_from_slice(&n.to_be_bytes());
        reason.extend_from_slice(why.as_bytes());
        Ok(vec![wire(State::Play, &S2c::Disconnect { reason })?])
    }
}

/// A flat world for tests: `radius` columns around the origin each way, stone to `ground_y - 1`
/// and grass on top, the player spawning on it at (0.5, ground_y + 1, 0.5). The real generator
/// (`sim::gen`, issue 08b) makes terrain worth walking.
pub fn flat_world(radius: i32, ground_y: i32) -> Result<ModelWorld, EncodeError> {
    use data::BlockKind;
    use protocol::chunk::{Paletted, Section};
    let state = |k: BlockKind| k.info().map_or(0, |i| i.default_state);
    let (stone, grass, air) = (
        state(BlockKind::Stone),
        state(BlockKind::GrassBlock),
        state(BlockKind::Air),
    );
    let section = |base: i32| {
        let cells: Vec<u32> = (0..4096i32)
            .map(|i| {
                let y = base + (i >> 8);
                if y < ground_y {
                    stone
                } else if y == ground_y {
                    grass
                } else {
                    air
                }
            })
            .collect();
        let count = cells.iter().filter(|c| **c != air).count();
        Section {
            block_count: i16::try_from(count).unwrap_or(i16::MAX),
            fluid_count: 0,
            blocks: Paletted::pack(&cells, 4),
            biomes: Paletted::Single(0),
        }
    };
    let column = Column {
        min_y: -64,
        sections: (0..24).map(|s| section(-64 + 16 * s)).collect(),
    };
    let mut columns = BTreeMap::new();
    for x in -radius..=radius {
        for z in -radius..=radius {
            columns.insert((x, z), ModelColumn::generated(x, z, column.clone())?);
        }
    }
    Ok(ModelWorld {
        columns,
        spawn: [0.5, f64::from(ground_y + 1), 0.5],
        dimension: "minecraft:overworld".to_owned(),
        dimension_type: 0,
    })
}
