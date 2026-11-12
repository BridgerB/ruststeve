//! Typed packets for what the bot uses; every other id is `S2c::Unknown`. A decoder reads only the
//! fields it models and ignores the rest of the frame (brain/wire/wire-packets-as-data.md); an
//! encoder is strict. Ids come from `data`'s generated tables, never literals.

use data::generated::packets::{
    ConfigurationClientbound as ConfIn, ConfigurationServerbound as ConfOut,
    HandshakeServerbound as HandOut, LoginClientbound as LoginIn, LoginServerbound as LoginOut,
    PlayClientbound as PlayIn, PlayServerbound as PlayOut,
};

use std::sync::Arc;

use crate::bytes::{Reader, Writer};
use crate::chunk::Column;
use crate::metadata::{MetaValue, read_metadata};
use crate::slot::{Slot, read_slot};
use crate::state::State;
use crate::{EncodeError, ParseError};

/// A packet from the server.
#[derive(Clone, Debug, PartialEq)]
pub enum S2c {
    // login
    LoginCompression {
        threshold: i32,
    },
    LoginFinished {
        uuid: [u8; 16],
        username: String,
    },
    LoginDisconnect {
        reason: String,
    },
    // configuration
    SelectKnownPacks,
    ConfigKeepAlive {
        id: i64,
    },
    ConfigPing {
        id: i32,
    },
    FinishConfiguration,
    /// The reason is a text component (NBT), kept as bytes.
    ConfigDisconnect {
        reason: Vec<u8>,
    },
    // play
    /// The play login: the player's entity id, the server's distances, and the start of
    /// `CommonPlayerSpawnInfo` (dimension type and name, game type); the rest is an unread tail.
    Login {
        entity_id: i32,
        view_distance: i32,
        simulation_distance: i32,
        dimension_type: i32,
        dimension: String,
        game_type: u8,
    },
    KeepAlive {
        id: i64,
    },
    Ping {
        id: i32,
    },
    PlayerPosition {
        teleport_id: i32,
        x: f64,
        y: f64,
        z: f64,
        yaw: f32,
        pitch: f32,
        flags: u32,
    },
    /// The clock, sent every 20 server ticks; its arrival is what matters.
    SetTime,
    /// A chunk column, its sections decoded on the reader thread (heightmaps and light skipped).
    LevelChunkWithLight {
        x: i32,
        z: i32,
        column: Arc<Column>,
    },
    /// A column the client should drop.
    ForgetLevelChunk {
        x: i32,
        z: i32,
    },
    /// One block changed: its position and new state id.
    BlockUpdate {
        pos: (i32, i32, i32),
        state: u32,
    },
    /// Several blocks in one section changed: `(world position, state id)` each.
    SectionBlocksUpdate {
        changes: Vec<((i32, i32, i32), u32)>,
    },
    /// A respawn or a dimension change: the start of `CommonPlayerSpawnInfo`.
    Respawn {
        dimension_type: i32,
        dimension: String,
    },
    SetHealth {
        health: f32,
        food: i32,
        saturation: f32,
    },
    /// An entity appeared: its id, uuid, type id and position (velocity and the rest: an
    /// ignored tail; field order from the game's ClientboundAddEntityPacket.write).
    AddEntity {
        id: i32,
        uuid: [u8; 16],
        kind: u32,
        x: f64,
        y: f64,
        z: f64,
    },
    RemoveEntities {
        ids: Vec<i32>,
    },
    /// A relative move (`move_entity_pos` or `move_entity_pos_rot`), in blocks.
    MoveEntity {
        id: i32,
        dx: f64,
        dy: f64,
        dz: f64,
    },
    /// A turn in place.
    MoveEntityRot {
        id: i32,
    },
    /// The server's absolute position for an entity.
    EntityPositionSync {
        id: i32,
        x: f64,
        y: f64,
        z: f64,
    },
    /// A long move; `flags` marks relative coordinates as `player_position`'s do.
    TeleportEntity {
        id: i32,
        x: f64,
        y: f64,
        z: f64,
        flags: u32,
    },
    /// Changed metadata, skimmed.
    SetEntityData {
        id: i32,
        entries: Vec<(u8, MetaValue)>,
    },
    /// A whole window. `slots` stops at a truncated slot; `carried` is `None` then.
    ContainerSetContent {
        window: i32,
        state_id: i32,
        slots: Vec<Slot>,
        carried: Option<Slot>,
    },
    ContainerSetSlot {
        window: i32,
        state_id: i32,
        slot: i16,
        item: Slot,
    },
    /// One slot of the player's inventory, by inventory index (0-8 hotbar, 9-35 main,
    /// 36-39 armour, 40 offhand).
    SetPlayerInventory {
        slot: i32,
        item: Slot,
    },
    ChunkBatchFinished {
        size: i32,
    },
    StartConfiguration,
    Disconnect {
        reason: Vec<u8>,
    },
    /// An id this layer does not model: counted, never an error.
    Unknown {
        state: State,
        id: u32,
        len: usize,
    },
}

/// The server's packet `id` in `state`, decoded from its fields.
pub fn decode(state: State, id: u32, body: &[u8]) -> Result<S2c, ParseError> {
    let mut r = Reader::new(body);
    let unknown = S2c::Unknown {
        state,
        id,
        len: body.len(),
    };
    // `Some(_) | None` arms: the packets this layer does not model are Unknown by design.
    Ok(match state {
        State::Handshake => unknown,
        State::Login => match LoginIn::from_id(id) {
            Some(LoginIn::LoginCompression) => S2c::LoginCompression {
                threshold: r.varint()?,
            },
            Some(LoginIn::LoginFinished) => S2c::LoginFinished {
                uuid: r.uuid()?,
                username: r.string()?,
            },
            Some(LoginIn::LoginDisconnect) => S2c::LoginDisconnect {
                reason: r.string()?,
            },
            Some(_) | None => unknown,
        },
        State::Configuration => match ConfIn::from_id(id) {
            Some(ConfIn::SelectKnownPacks) => S2c::SelectKnownPacks,
            Some(ConfIn::KeepAlive) => S2c::ConfigKeepAlive { id: r.i64()? },
            Some(ConfIn::Ping) => S2c::ConfigPing { id: r.i32()? },
            Some(ConfIn::FinishConfiguration) => S2c::FinishConfiguration,
            Some(ConfIn::Disconnect) => S2c::ConfigDisconnect {
                reason: r.rest().to_vec(),
            },
            Some(_) | None => unknown,
        },
        State::Play => match PlayIn::from_id(id) {
            Some(PlayIn::Login) => {
                // Field order from the game's ClientboundLoginPacket.write, as clojurecraft's
                // spec has it.
                let entity_id = r.i32()?;
                let _hardcore = r.bool()?;
                let levels = r.length()?;
                for _ in 0..levels {
                    r.string()?;
                }
                let _max_players = r.varint()?;
                let view_distance = r.varint()?;
                let simulation_distance = r.varint()?;
                let _reduced_debug = r.bool()?;
                let _death_screen = r.bool()?;
                let _limited_crafting = r.bool()?;
                let dimension_type = r.varint()?;
                let dimension = r.string()?;
                let _seed = r.i64()?;
                let game_type = r.u8()?;
                S2c::Login {
                    entity_id,
                    view_distance,
                    simulation_distance,
                    dimension_type,
                    dimension,
                    game_type,
                }
            }
            Some(PlayIn::KeepAlive) => S2c::KeepAlive { id: r.i64()? },
            Some(PlayIn::Ping) => S2c::Ping { id: r.i32()? },
            Some(PlayIn::PlayerPosition) => {
                let teleport_id = r.varint()?;
                let (x, y, z) = (r.f64()?, r.f64()?, r.f64()?);
                let _velocity = (r.f64()?, r.f64()?, r.f64()?);
                let (yaw, pitch) = (r.f32()?, r.f32()?);
                S2c::PlayerPosition {
                    teleport_id,
                    x,
                    y,
                    z,
                    yaw,
                    pitch,
                    flags: r.u32()?,
                }
            }
            Some(PlayIn::SetTime) => S2c::SetTime,
            Some(PlayIn::LevelChunkWithLight) => {
                let (x, z) = (r.i32()?, r.i32()?);
                for _ in 0..r.length()? {
                    let _kind = r.varint()?;
                    let longs = r.length()?;
                    r.take(longs.saturating_mul(8))?;
                }
                let n = r.length()?;
                let column = Column::decode(r.take(n)?)?;
                S2c::LevelChunkWithLight {
                    x,
                    z,
                    column: Arc::new(column),
                }
            }
            Some(PlayIn::ForgetLevelChunk) => {
                // ChunkPos.toLong: x in the low 32 bits, z in the high.
                let v = r.i64()?;
                S2c::ForgetLevelChunk {
                    x: v as i32,
                    z: (v >> 32) as i32,
                }
            }
            Some(PlayIn::BlockUpdate) => S2c::BlockUpdate {
                pos: r.position()?,
                state: r.varint()? as u32,
            },
            Some(PlayIn::SectionBlocksUpdate) => {
                // SectionPos.asLong: x 22 bits, z 22 bits, y 20 bits; each change a varlong of
                // state << 12 | local x << 8 | local z << 4 | local y (clojurecraft's
                // terrain/section-update).
                let s = r.i64()?;
                let (sx, sz, sy) = (s >> 42, (s << 22) >> 42, (s << 44) >> 44);
                let n = r.length()?;
                let mut changes = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    let v = r.varlong()?;
                    let (lx, lz, ly) = ((v >> 8) & 15, (v >> 4) & 15, v & 15);
                    let pos = (
                        (sx * 16 + lx) as i32,
                        (sy * 16 + ly) as i32,
                        (sz * 16 + lz) as i32,
                    );
                    changes.push((pos, ((v as u64) >> 12) as u32));
                }
                S2c::SectionBlocksUpdate { changes }
            }
            Some(PlayIn::Respawn) => S2c::Respawn {
                dimension_type: r.varint()?,
                dimension: r.string()?,
            },
            Some(PlayIn::SetHealth) => S2c::SetHealth {
                health: r.f32()?,
                food: r.varint()?,
                saturation: r.f32()?,
            },
            Some(PlayIn::AddEntity) => S2c::AddEntity {
                id: r.varint()?,
                uuid: r.uuid()?,
                kind: r.varint()? as u32,
                x: finite(r.f64()?)?,
                y: finite(r.f64()?)?,
                z: finite(r.f64()?)?,
            },
            Some(PlayIn::RemoveEntities) => {
                let n = r.length()?;
                let mut ids = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    ids.push(r.varint()?);
                }
                S2c::RemoveEntities { ids }
            }
            Some(PlayIn::MoveEntityPos | PlayIn::MoveEntityPosRot) => {
                // Deltas in 1/4096 of a block (ClientboundMoveEntityPacket).
                let id = r.varint()?;
                let mut delta = || -> Result<f64, ParseError> { Ok(f64::from(r.i16()?) / 4096.0) };
                let (dx, dy, dz) = (delta()?, delta()?, delta()?);
                S2c::MoveEntity { id, dx, dy, dz }
            }
            Some(PlayIn::MoveEntityRot) => S2c::MoveEntityRot { id: r.varint()? },
            Some(PlayIn::EntityPositionSync) => S2c::EntityPositionSync {
                id: r.varint()?,
                x: finite(r.f64()?)?,
                y: finite(r.f64()?)?,
                z: finite(r.f64()?)?,
            },
            Some(PlayIn::TeleportEntity) => {
                let id = r.varint()?;
                let (x, y, z) = (finite(r.f64()?)?, finite(r.f64()?)?, finite(r.f64()?)?);
                let _velocity = (r.f64()?, r.f64()?, r.f64()?);
                let _look = (r.f32()?, r.f32()?);
                S2c::TeleportEntity {
                    id,
                    x,
                    y,
                    z,
                    flags: r.u32()?,
                }
            }
            Some(PlayIn::SetEntityData) => S2c::SetEntityData {
                id: r.varint()?,
                entries: read_metadata(&mut r)?,
            },
            Some(PlayIn::ContainerSetContent) => {
                let window = r.varint()?;
                let state_id = r.varint()?;
                let n = r.length()?;
                let mut slots = Vec::with_capacity(n.min(128));
                let mut cut = false;
                for _ in 0..n {
                    let s = read_slot(&mut r)?;
                    slots.push(s);
                    if s.truncated() {
                        cut = true;
                        break;
                    }
                }
                let carried = if cut { None } else { Some(read_slot(&mut r)?) };
                S2c::ContainerSetContent {
                    window,
                    state_id,
                    slots,
                    carried,
                }
            }
            Some(PlayIn::ContainerSetSlot) => S2c::ContainerSetSlot {
                window: r.varint()?,
                state_id: r.varint()?,
                slot: r.i16()?,
                item: read_slot(&mut r)?,
            },
            Some(PlayIn::SetPlayerInventory) => S2c::SetPlayerInventory {
                slot: r.varint()?,
                item: read_slot(&mut r)?,
            },
            Some(PlayIn::ChunkBatchFinished) => S2c::ChunkBatchFinished { size: r.varint()? },
            Some(PlayIn::StartConfiguration) => S2c::StartConfiguration,
            Some(PlayIn::Disconnect) => S2c::Disconnect {
                reason: r.rest().to_vec(),
            },
            Some(_) | None => unknown,
        },
    })
}

/// A packet to the server.
#[derive(Clone, Debug, PartialEq)]
pub enum C2s {
    // handshake
    Intention {
        protocol: i32,
        host: String,
        port: u16,
        /// 1 status, 2 login, 3 transfer.
        next_state: i32,
    },
    // login
    Hello {
        username: String,
        uuid: [u8; 16],
    },
    LoginAcknowledged,
    // configuration
    ClientInformation {
        locale: String,
        view_distance: i8,
    },
    /// The packs the client already has: none, so the server sends every registry in full.
    SelectKnownPacks,
    ConfigKeepAlive {
        id: i64,
    },
    ConfigPong {
        id: i32,
    },
    FinishConfiguration,
    // play
    KeepAlive {
        id: i64,
    },
    Pong {
        id: i32,
    },
    AcceptTeleportation {
        teleport_id: i32,
    },
    PlayerLoaded,
    ChunkBatchReceived {
        chunks_per_tick: f32,
    },
    ConfigurationAcknowledged,
    /// The player's position and look; flags bit 0 on the ground, bit 1 against a wall.
    MovePlayerPosRot {
        x: f64,
        y: f64,
        z: f64,
        yaw: f32,
        pitch: f32,
        flags: u8,
    },
    /// Nothing moved: the flags only, sent once a second.
    MovePlayerStatusOnly {
        flags: u8,
    },
}

impl C2s {
    /// The state this packet is sent in.
    pub fn state(&self) -> State {
        match self {
            C2s::Intention { .. } => State::Handshake,
            C2s::Hello { .. } | C2s::LoginAcknowledged => State::Login,
            C2s::ClientInformation { .. }
            | C2s::SelectKnownPacks
            | C2s::ConfigKeepAlive { .. }
            | C2s::ConfigPong { .. }
            | C2s::FinishConfiguration => State::Configuration,
            C2s::KeepAlive { .. }
            | C2s::Pong { .. }
            | C2s::AcceptTeleportation { .. }
            | C2s::PlayerLoaded
            | C2s::ChunkBatchReceived { .. }
            | C2s::MovePlayerPosRot { .. }
            | C2s::MovePlayerStatusOnly { .. }
            | C2s::ConfigurationAcknowledged => State::Play,
        }
    }

    /// The packet id and its encoded fields.
    pub fn encode(&self) -> Result<(u32, Vec<u8>), EncodeError> {
        let mut w = Writer::new();
        let id = match self {
            C2s::Intention {
                protocol,
                host,
                port,
                next_state,
            } => {
                w.varint(*protocol);
                w.string(host)?;
                w.u16(*port);
                w.varint(*next_state);
                HandOut::Intention.id()
            }
            C2s::Hello { username, uuid } => {
                w.string(username)?;
                w.uuid(uuid);
                LoginOut::Hello.id()
            }
            C2s::LoginAcknowledged => LoginOut::LoginAcknowledged.id(),
            C2s::ClientInformation {
                locale,
                view_distance,
            } => {
                // locale, view distance, chat mode (0 enabled), chat colours, skin parts (all
                // 0x7f), main hand (1 right), text filtering, server listing, particle status
                // (0 all): the layout clojurecraft's client-information spec has.
                w.string(locale)?;
                w.i8(*view_distance);
                w.varint(0);
                w.bool(true);
                w.u8(0x7f);
                w.varint(1);
                w.bool(false);
                w.bool(true);
                w.varint(0);
                ConfOut::ClientInformation.id()
            }
            C2s::SelectKnownPacks => {
                w.varint(0);
                ConfOut::SelectKnownPacks.id()
            }
            C2s::ConfigKeepAlive { id } => {
                w.i64(*id);
                ConfOut::KeepAlive.id()
            }
            C2s::ConfigPong { id } => {
                w.i32(*id);
                ConfOut::Pong.id()
            }
            C2s::FinishConfiguration => ConfOut::FinishConfiguration.id(),
            C2s::KeepAlive { id } => {
                w.i64(*id);
                PlayOut::KeepAlive.id()
            }
            C2s::Pong { id } => {
                w.i32(*id);
                PlayOut::Pong.id()
            }
            C2s::AcceptTeleportation { teleport_id } => {
                w.varint(*teleport_id);
                PlayOut::AcceptTeleportation.id()
            }
            C2s::PlayerLoaded => PlayOut::PlayerLoaded.id(),
            C2s::ChunkBatchReceived { chunks_per_tick } => {
                w.f32(*chunks_per_tick);
                PlayOut::ChunkBatchReceived.id()
            }
            C2s::ConfigurationAcknowledged => PlayOut::ConfigurationAcknowledged.id(),
            C2s::MovePlayerPosRot {
                x,
                y,
                z,
                yaw,
                pitch,
                flags,
            } => {
                w.f64(*x);
                w.f64(*y);
                w.f64(*z);
                w.f32(*yaw);
                w.f32(*pitch);
                w.u8(*flags);
                PlayOut::MovePlayerPosRot.id()
            }
            C2s::MovePlayerStatusOnly { flags } => {
                w.u8(*flags);
                PlayOut::MovePlayerStatusOnly.id()
            }
        };
        Ok((id, w.buf))
    }
}

/// A coordinate the core may hold: NaN or infinity from a bad frame is a boundary error, never
/// a value (brain/ruststeve/design/determinism-rules.md).
fn finite(v: f64) -> Result<f64, ParseError> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(ParseError::NonFinite)
    }
}
