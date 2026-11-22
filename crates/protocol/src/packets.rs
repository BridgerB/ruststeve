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
    /// The bot died: its entity id and the server's death message (a text component), read
    /// as clojurecraft's `text/death` does.
    PlayerCombatKill {
        player_id: i32,
        death: crate::nbt::Death,
    },
    /// An entity was hurt: the damage type (an id into `data::DAMAGE_TYPE`, the order the server
    /// syncs) and the entities that caused it and dealt it (`None`: no entity).
    DamageEvent {
        entity_id: i32,
        source_type: i32,
        cause: Option<i32>,
        direct: Option<i32>,
    },
    ChunkBatchFinished {
        size: i32,
    },
    /// The server has handled the client's block actions up to `sequence` (a dig's FINISH).
    BlockChangedAck {
        sequence: i32,
    },
    /// The server set the selected hotbar slot (0-8).
    SetHeldSlot {
        slot: i32,
    },
    /// An item entity was picked up: by whom, and how many of it.
    TakeItemEntity {
        item: i32,
        collector: i32,
        amount: i32,
    },
    /// The server's velocity for an entity, in blocks a tick (an LpVec3): knockback on the bot.
    SetEntityMotion {
        id: i32,
        velocity: [f64; 3],
    },
    /// An entity's status byte (3: a living entity's death; 9: the player finished using an
    /// item, an eat).
    EntityEvent {
        id: i32,
        status: i8,
    },
    /// A window opened: its id and its menu type (an index into `data::MENU`); the title is
    /// skipped.
    OpenScreen {
        window: i32,
        menu: i32,
    },
    /// One of a window's properties (a furnace's burn and cook progress: 0 lit time left, 1 lit
    /// total, 2 cook progress, 3 cook total; issue 13a).
    ContainerSetData {
        window: i32,
        property: i16,
        value: i16,
    },
    /// What the cursor holds now (sent when a click's claimed cursor differs).
    SetCursorItem {
        item: Slot,
    },
    /// The server closed a window.
    ContainerClose {
        window: i32,
    },
    StartConfiguration,
    Disconnect {
        reason: Vec<u8>,
    },
    /// An id this layer does not model: counted, never an error.
    // Hearing (issue 16e).
    /// A sound at a place: positions on the wire are eighths of a block.
    Sound {
        sound: SoundRef,
        source: i32,
        pos: [f64; 3],
        volume: f32,
        pitch: f32,
    },
    /// A sound on an entity (an enderman's stare and scream).
    SoundEntity {
        sound: SoundRef,
        source: i32,
        entity: i32,
        volume: f32,
        pitch: f32,
    },
    /// A level event (`data::generated::events::LEVEL_EVENTS`): 1018 a blaze shooting.
    LevelEvent {
        event: i32,
        pos: (i32, i32, i32),
        data: i32,
        global: bool,
    },
    StopSound {
        source: Option<i32>,
        sound: Option<String>,
    },
    // Sight of mobs (issue 16e).
    /// Where a mob's head points, in degrees (the wire's byte angle).
    RotateHead {
        id: i32,
        yaw: f32,
    },
    /// What a mob holds and wears: (slot, item) pairs; slot 0 main hand, 1 off hand, 2 to 5
    /// feet to head, 6 body, 7 saddle.
    SetEquipment {
        id: i32,
        items: Vec<(u8, Slot)>,
    },
    /// An arm swing or other animation (0 main hand, 3 off hand, 4 and 5 critical hits).
    Animate {
        id: i32,
        action: u8,
    },
    /// The bot's own hurt flash, with the direction the blow came from.
    HurtAnimation {
        id: i32,
        yaw: f32,
    },
    /// A fireball's acceleration power.
    ProjectilePower {
        id: i32,
        power: f64,
    },
    SetPassengers {
        vehicle: i32,
        passengers: Vec<i32>,
    },
    /// A blast: its centre, and the knockback it gives the bot when it does.
    Explode {
        center: [f64; 3],
        knockback: Option<[f64; 3]>,
    },
    /// A block entity's data: for a spawner, the mob it spawns (`SpawnData.entity.id`).
    BlockEntityData {
        pos: (i32, i32, i32),
        kind: i32,
        spawns: Option<String>,
    },
    /// A block cracking under someone's digging (stage 0 to 9; anything else clears it).
    BlockDestruction {
        breaker: i32,
        pos: (i32, i32, i32),
        stage: i8,
    },
    // The body, language and the world (issue 16e).
    UpdateMobEffect {
        id: i32,
        effect: i32,
        amplifier: i32,
        duration: i32,
    },
    RemoveMobEffect {
        id: i32,
        effect: i32,
    },
    /// Attribute values: (registry id, base, the sum of the add-value modifiers).
    UpdateAttributes {
        id: i32,
        attributes: Vec<(i32, f64, f64)>,
    },
    PlayerCombatEnter,
    PlayerCombatEnd {
        duration: i32,
    },
    SetExperience {
        progress: f32,
        level: i32,
        total: i32,
    },
    /// An item cooldown by its group (`minecraft:ender_pearl`), in ticks.
    Cooldown {
        group: String,
        ticks: i32,
    },
    /// A game event (`data::generated::events::GAME_EVENTS`): 4 the win, 1 rain.
    GameEvent {
        event: u8,
        param: f32,
    },
    /// Chat and the screen's text, as plain text: system chat (`overlay` is the action bar),
    /// the action bar, titles and subtitles.
    SystemChat {
        text: String,
        overlay: bool,
    },
    ActionBar {
        text: String,
    },
    Title {
        text: String,
    },
    Subtitle {
        text: String,
    },
    /// A boss bar: added (with its name and health), removed, or changed.
    BossEvent {
        uuid: [u8; 16],
        op: BossOp,
    },
    /// Chunks whose biomes changed (the data itself is not read yet).
    ChunksBiomes {
        chunks: Vec<(i32, i32)>,
    },
    SetChunkCacheCenter {
        x: i32,
        z: i32,
    },
    SetChunkCacheRadius {
        radius: i32,
    },
    SetSimulationDistance {
        distance: i32,
    },
    Unknown {
        state: State,
        id: u32,
        len: usize,
    },
}

/// A sound event as the wire holds it: a registry id, or one written inline by name.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundRef {
    Registry(u32),
    Inline(String),
}

impl SoundRef {
    /// The sound's name (`entity.blaze.shoot`); `sound:<id>` for an id the table lacks.
    pub fn name(&self) -> String {
        match self {
            SoundRef::Registry(id) => usize::try_from(*id)
                .ok()
                .and_then(|i| data::generated::registries::SOUND_EVENT.get(i))
                .map_or_else(|| format!("sound:{id}"), |n| (*n).to_owned()),
            SoundRef::Inline(n) => n.trim_start_matches("minecraft:").to_owned(),
        }
    }
}

/// What a boss-bar packet does.
#[derive(Clone, Debug, PartialEq)]
pub enum BossOp {
    Add {
        name: String,
        progress: f32,
    },
    Remove,
    Progress(f32),
    Name(String),
    /// Colour, overlay or flags changed: nothing the bot reads.
    Style,
}

/// A text component (network NBT) as one line of plain text.
fn read_text(r: &mut Reader<'_>) -> Result<String, ParseError> {
    Ok(crate::nbt::plain(&crate::nbt::read_root(r)?))
}

fn write_text(w: &mut Writer, s: &str) {
    w.bytes(&crate::nbt::write_root(&crate::nbt::Nbt::String(
        s.to_owned(),
    )));
}

/// A byte angle (256 to the turn) in degrees.
fn angle(b: u8) -> f32 {
    f32::from(b) * 360.0 / 256.0
}

/// `Holder<SoundEvent>`: a varint, 0 for an inline event (its id and an optional fixed range),
/// else the registry id plus one.
fn read_sound(r: &mut Reader<'_>) -> Result<SoundRef, ParseError> {
    let v = r.varint()?;
    if v == 0 {
        let name = r.string()?;
        if r.bool()? {
            r.f32()?;
        }
        Ok(SoundRef::Inline(name))
    } else {
        Ok(SoundRef::Registry((v - 1) as u32))
    }
}

fn write_sound(w: &mut Writer, s: &SoundRef) -> Result<(), EncodeError> {
    match s {
        SoundRef::Registry(id) => w.varint(*id as i32 + 1),
        SoundRef::Inline(name) => {
            w.varint(0);
            w.string(name)?;
            w.bool(false);
        }
    }
    Ok(())
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
            Some(PlayIn::PlayerCombatKill) => S2c::PlayerCombatKill {
                player_id: r.varint()?,
                death: crate::nbt::death(&crate::nbt::read_root(&mut r)?),
            },
            Some(PlayIn::DamageEvent) => {
                // ClientboundDamageEventPacket: the cause and direct ids are sent plus one, 0 for
                // none; an optional source position follows (an unread tail).
                let entity_id = r.varint()?;
                let source_type = r.varint()?;
                let plus_one = |v: i32| (v > 0).then(|| v - 1);
                let cause = plus_one(r.varint()?);
                let direct = plus_one(r.varint()?);
                S2c::DamageEvent {
                    entity_id,
                    source_type,
                    cause,
                    direct,
                }
            }
            Some(PlayIn::ChunkBatchFinished) => S2c::ChunkBatchFinished { size: r.varint()? },
            Some(PlayIn::BlockChangedAck) => S2c::BlockChangedAck {
                sequence: r.varint()?,
            },
            Some(PlayIn::SetHeldSlot) => S2c::SetHeldSlot { slot: r.varint()? },
            Some(PlayIn::TakeItemEntity) => S2c::TakeItemEntity {
                item: r.varint()?,
                collector: r.varint()?,
                amount: r.varint()?,
            },
            Some(PlayIn::SetEntityMotion) => S2c::SetEntityMotion {
                id: r.varint()?,
                velocity: r.lpvec3()?,
            },
            Some(PlayIn::EntityEvent) => S2c::EntityEvent {
                id: r.i32()?,
                status: r.i8()?,
            },
            Some(PlayIn::OpenScreen) => S2c::OpenScreen {
                window: r.varint()?,
                menu: r.varint()?,
            },
            Some(PlayIn::SetCursorItem) => S2c::SetCursorItem {
                item: read_slot(&mut r)?,
            },
            Some(PlayIn::ContainerSetData) => S2c::ContainerSetData {
                window: r.varint()?,
                property: r.i16()?,
                value: r.i16()?,
            },
            Some(PlayIn::ContainerClose) => S2c::ContainerClose {
                window: r.varint()?,
            },
            Some(PlayIn::Sound) => S2c::Sound {
                sound: read_sound(&mut r)?,
                source: r.varint()?,
                pos: {
                    let (x, y, z) = (r.i32()?, r.i32()?, r.i32()?);
                    [f64::from(x) / 8.0, f64::from(y) / 8.0, f64::from(z) / 8.0]
                },
                volume: r.f32()?,
                pitch: r.f32()?,
            },
            Some(PlayIn::SoundEntity) => S2c::SoundEntity {
                sound: read_sound(&mut r)?,
                source: r.varint()?,
                entity: r.varint()?,
                volume: r.f32()?,
                pitch: r.f32()?,
            },
            Some(PlayIn::LevelEvent) => S2c::LevelEvent {
                event: r.i32()?,
                pos: r.position()?,
                data: r.i32()?,
                global: r.bool()?,
            },
            Some(PlayIn::StopSound) => {
                let flags = r.u8()?;
                let source = if flags & 1 != 0 {
                    Some(r.varint()?)
                } else {
                    None
                };
                let sound = if flags & 2 != 0 {
                    Some(r.string()?)
                } else {
                    None
                };
                S2c::StopSound { source, sound }
            }
            Some(PlayIn::RotateHead) => S2c::RotateHead {
                id: r.varint()?,
                yaw: angle(r.u8()?),
            },
            Some(PlayIn::SetEquipment) => {
                let id = r.varint()?;
                let mut items = Vec::new();
                loop {
                    let slot = r.u8()?;
                    let item = read_slot(&mut r)?;
                    items.push((slot & 0x7f, item));
                    // A cut-short stack ends what can be read; the top bit says more follow.
                    if item.truncated() || slot & 0x80 == 0 {
                        break;
                    }
                }
                S2c::SetEquipment { id, items }
            }
            Some(PlayIn::Animate) => S2c::Animate {
                id: r.varint()?,
                action: r.u8()?,
            },
            Some(PlayIn::HurtAnimation) => S2c::HurtAnimation {
                id: r.varint()?,
                yaw: r.f32()?,
            },
            Some(PlayIn::ProjectilePower) => S2c::ProjectilePower {
                id: r.varint()?,
                power: r.f64()?,
            },
            Some(PlayIn::SetPassengers) => {
                let vehicle = r.varint()?;
                let n = r.length()?;
                let mut passengers = Vec::with_capacity(n.min(16));
                for _ in 0..n {
                    passengers.push(r.varint()?);
                }
                S2c::SetPassengers {
                    vehicle,
                    passengers,
                }
            }
            // The centre and the knockback; the particles and sound after them are not read.
            Some(PlayIn::Explode) => S2c::Explode {
                center: [r.f64()?, r.f64()?, r.f64()?],
                knockback: if r.bool()? {
                    Some([r.f64()?, r.f64()?, r.f64()?])
                } else {
                    None
                },
            },
            Some(PlayIn::BlockEntityData) => {
                let pos = r.position()?;
                let kind = r.varint()?;
                let tag = crate::nbt::read_root(&mut r)?;
                let spawns = tag
                    .get("SpawnData")
                    .and_then(|s| s.get("entity"))
                    .and_then(|e| e.get("id"))
                    .and_then(crate::nbt::Nbt::as_str)
                    .map(|s| s.trim_start_matches("minecraft:").to_owned());
                S2c::BlockEntityData { pos, kind, spawns }
            }
            Some(PlayIn::BlockDestruction) => S2c::BlockDestruction {
                breaker: r.varint()?,
                pos: r.position()?,
                stage: r.i8()?,
            },
            Some(PlayIn::UpdateMobEffect) => {
                let id = r.varint()?;
                let effect = r.varint()?;
                let amplifier = r.varint()?;
                let duration = r.varint()?;
                S2c::UpdateMobEffect {
                    id,
                    effect,
                    amplifier,
                    duration,
                }
            }
            Some(PlayIn::RemoveMobEffect) => S2c::RemoveMobEffect {
                id: r.varint()?,
                effect: r.varint()?,
            },
            Some(PlayIn::UpdateAttributes) => {
                let id = r.varint()?;
                let n = r.length()?;
                let mut attributes = Vec::with_capacity(n.min(64));
                for _ in 0..n {
                    let attr = r.varint()?;
                    let base = r.f64()?;
                    let mods = r.length()?;
                    let mut add = 0.0;
                    for _ in 0..mods {
                        let _id = r.string()?;
                        let amount = r.f64()?;
                        // Operation 0 adds its amount; the multiplying ones are left out, as
                        // clojurecraft's hear.clj does.
                        if r.varint()? == 0 {
                            add += amount;
                        }
                    }
                    attributes.push((attr, base, add));
                }
                S2c::UpdateAttributes { id, attributes }
            }
            Some(PlayIn::PlayerCombatEnter) => S2c::PlayerCombatEnter,
            Some(PlayIn::PlayerCombatEnd) => S2c::PlayerCombatEnd {
                duration: r.varint()?,
            },
            Some(PlayIn::SetExperience) => S2c::SetExperience {
                progress: r.f32()?,
                level: r.varint()?,
                total: r.varint()?,
            },
            Some(PlayIn::Cooldown) => S2c::Cooldown {
                group: r.string()?,
                ticks: r.varint()?,
            },
            Some(PlayIn::GameEvent) => S2c::GameEvent {
                event: r.u8()?,
                param: r.f32()?,
            },
            Some(PlayIn::SystemChat) => S2c::SystemChat {
                text: read_text(&mut r)?,
                overlay: r.bool()?,
            },
            Some(PlayIn::SetActionBarText) => S2c::ActionBar {
                text: read_text(&mut r)?,
            },
            Some(PlayIn::SetTitleText) => S2c::Title {
                text: read_text(&mut r)?,
            },
            Some(PlayIn::SetSubtitleText) => S2c::Subtitle {
                text: read_text(&mut r)?,
            },
            Some(PlayIn::BossEvent) => {
                let uuid = r.uuid()?;
                let op = match r.varint()? {
                    0 => BossOp::Add {
                        name: read_text(&mut r)?,
                        progress: r.f32()?,
                    },
                    1 => BossOp::Remove,
                    2 => BossOp::Progress(r.f32()?),
                    3 => BossOp::Name(read_text(&mut r)?),
                    _ => BossOp::Style,
                };
                S2c::BossEvent { uuid, op }
            }
            Some(PlayIn::ChunksBiomes) => {
                let n = r.length()?;
                let mut chunks = Vec::with_capacity(n.min(64));
                for _ in 0..n {
                    // ChunkPos as a long: x in the low 32 bits, z in the high.
                    let packed = r.i64()?;
                    chunks.push((packed as i32, (packed >> 32) as i32));
                    let len = r.length()?;
                    r.take(len)?;
                }
                S2c::ChunksBiomes { chunks }
            }
            Some(PlayIn::SetChunkCacheCenter) => S2c::SetChunkCacheCenter {
                x: r.varint()?,
                z: r.varint()?,
            },
            Some(PlayIn::SetChunkCacheRadius) => S2c::SetChunkCacheRadius {
                radius: r.varint()?,
            },
            Some(PlayIn::SetSimulationDistance) => S2c::SetSimulationDistance {
                distance: r.varint()?,
            },
            Some(PlayIn::StartConfiguration) => S2c::StartConfiguration,
            Some(PlayIn::Disconnect) => S2c::Disconnect {
                reason: r.rest().to_vec(),
            },
            Some(_) | None => unknown,
        },
    })
}

/// The server's packet `id` in `state`, named `state/name` as the generated tables spell it
/// (`play/sound`); an id the tables lack is `state/<id>`. What coverage counts unknown packets by.
pub fn packet_name(state: State, id: u32) -> String {
    let (prefix, name) = match state {
        State::Handshake => ("handshake", None),
        State::Login => ("login", LoginIn::from_id(id).map(LoginIn::name)),
        State::Configuration => ("configuration", ConfIn::from_id(id).map(ConfIn::name)),
        State::Play => ("play", PlayIn::from_id(id).map(PlayIn::name)),
    };
    match name {
        Some(n) => format!("{prefix}/{n}"),
        None => format!("{prefix}/{id}"),
    }
}

/// Whether this layer decodes the server's packet `id` in `state`: anything else is
/// `S2c::Unknown`. An empty body is enough to tell: a modelled packet fails to read or decodes,
/// an unmodelled one is Unknown before any field is read.
pub fn decodes(state: State, id: u32) -> bool {
    !matches!(decode(state, id, &[]), Ok(S2c::Unknown { .. }))
}

/// Every clientbound packet the generated tables know, as `(state, id)`, in state then id order.
pub fn clientbound() -> Vec<(State, u32)> {
    let login = LoginIn::ALL.iter().map(|p| (State::Login, p.id()));
    let conf = ConfIn::ALL.iter().map(|p| (State::Configuration, p.id()));
    let play = PlayIn::ALL.iter().map(|p| (State::Play, p.id()));
    login.chain(conf).chain(play).collect()
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
    /// A dig: status 0 START, 1 ABORT, 2 FINISH; the block, the face, and the client's
    /// sequence number the server acknowledges.
    PlayerAction {
        status: i32,
        pos: (i32, i32, i32),
        face: i8,
        sequence: i32,
    },
    /// Select a hotbar slot (0-8).
    SetCarriedItem {
        slot: i16,
    },
    /// `client_command`: action 0 asks to respawn after a death (the marathon's revive).
    ClientCommand {
        action: i32,
    },
    /// A click in a window, predicting nothing: no changed slots and an empty cursor, so the
    /// server answers with authoritative slots (brain/wire/wire-container-click.md).
    ContainerClick {
        window: i32,
        state_id: i32,
        slot: i16,
        button: i8,
        mode: i32,
    },
    ContainerClose {
        window: i32,
    },
    /// The arm swing: 0 the main hand.
    Swing {
        hand: i32,
    },
    /// Right-click a block face with a hand: place the held block against it, or open the block
    /// (a crafting table). Field order from the game's `ServerboundUseItemOnPacket`; the cursor is
    /// where on the face, 0 to 1 on each axis.
    UseItemOn {
        hand: i32,
        pos: (i32, i32, i32),
        face: i32,
        cursor: [f32; 3],
        inside: bool,
        sequence: i32,
    },
    /// Melee (issue 12a): 775's dedicated packet, the entity id alone (`interact` is right-click
    /// only; brain/wire/wire-actions.md).
    Attack {
        entity: i32,
    },
    /// Right-click aimed by look with a hand (eating starts here): hand, sequence, yaw, pitch.
    UseItem {
        hand: i32,
        sequence: i32,
        yaw: f32,
        pitch: f32,
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
            | C2s::PlayerAction { .. }
            | C2s::SetCarriedItem { .. }
            | C2s::ClientCommand { .. }
            | C2s::UseItemOn { .. }
            | C2s::Attack { .. }
            | C2s::UseItem { .. }
            | C2s::ContainerClick { .. }
            | C2s::ContainerClose { .. }
            | C2s::Swing { .. }
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
            C2s::PlayerAction {
                status,
                pos,
                face,
                sequence,
            } => {
                w.varint(*status);
                w.i64(crate::bytes::pack_position(pos.0, pos.1, pos.2));
                w.i8(*face);
                w.varint(*sequence);
                PlayOut::PlayerAction.id()
            }
            C2s::SetCarriedItem { slot } => {
                w.i16(*slot);
                PlayOut::SetCarriedItem.id()
            }
            C2s::ClientCommand { action } => {
                w.varint(*action);
                PlayOut::ClientCommand.id()
            }
            C2s::ContainerClick {
                window,
                state_id,
                slot,
                button,
                mode,
            } => {
                w.varint(*window);
                w.varint(*state_id);
                w.i16(*slot);
                w.i8(*button);
                w.varint(*mode);
                w.varint(0); // no changed slots: nothing predicted
                crate::slot::write_hashed_slot(&mut w, crate::slot::Slot::Empty); // nor the cursor
                PlayOut::ContainerClick.id()
            }
            C2s::ContainerClose { window } => {
                w.varint(*window);
                PlayOut::ContainerClose.id()
            }
            C2s::Swing { hand } => {
                w.varint(*hand);
                PlayOut::Swing.id()
            }
            C2s::UseItemOn {
                hand,
                pos,
                face,
                cursor,
                inside,
                sequence,
            } => {
                w.varint(*hand);
                w.i64(crate::bytes::pack_position(pos.0, pos.1, pos.2));
                w.varint(*face);
                w.f32(cursor[0]);
                w.f32(cursor[1]);
                w.f32(cursor[2]);
                w.bool(*inside);
                w.bool(false); // not past the world border
                w.varint(*sequence);
                PlayOut::UseItemOn.id()
            }
            C2s::Attack { entity } => {
                w.varint(*entity);
                PlayOut::Attack.id()
            }
            C2s::UseItem {
                hand,
                sequence,
                yaw,
                pitch,
            } => {
                w.varint(*hand);
                w.varint(*sequence);
                w.f32(*yaw);
                w.f32(*pitch);
                PlayOut::UseItem.id()
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

impl S2c {
    /// The packet id and its fields, for the server model (brain/ruststeve/design/server-model.md):
    /// the inverse of `decode` for every packet the model sends, so the bot reads the model's
    /// bytes exactly as a server's. Field layouts are the game's own `write` methods, as
    /// clojurecraft's spec has them; fields the decoder skips are written with vanilla's
    /// defaults. A packet the model does not send yet is `NotModelled`, never invented.
    pub fn encode(&self) -> Result<(u32, Vec<u8>), EncodeError> {
        let mut w = Writer::new();
        let id = match self {
            S2c::LoginFinished { uuid, username } => {
                w.uuid(uuid);
                w.string(username)?;
                w.varint(0); // no profile properties
                LoginIn::LoginFinished.id()
            }
            S2c::SelectKnownPacks => {
                w.varint(0);
                ConfIn::SelectKnownPacks.id()
            }
            S2c::ConfigKeepAlive { id } => {
                w.i64(*id);
                ConfIn::KeepAlive.id()
            }
            S2c::FinishConfiguration => ConfIn::FinishConfiguration.id(),
            S2c::Login {
                entity_id,
                view_distance,
                simulation_distance,
                dimension_type,
                dimension,
                game_type,
            } => {
                w.i32(*entity_id);
                w.bool(false); // hardcore
                w.varint(1);
                w.string(dimension)?; // the levels the server has; this one is enough
                w.varint(20); // max players
                w.varint(*view_distance);
                w.varint(*simulation_distance);
                w.bool(false); // reduced debug info
                w.bool(true); // show the death screen
                w.bool(false); // limited crafting
                // CommonPlayerSpawnInfo
                w.varint(*dimension_type);
                w.string(dimension)?;
                w.i64(0); // hashed seed
                w.u8(*game_type);
                w.i8(-1); // no previous game type
                w.bool(false); // debug world
                w.bool(false); // flat world
                w.bool(false); // no last death location
                w.varint(0); // portal cooldown
                w.varint(63); // sea level
                w.bool(false); // enforces secure chat
                PlayIn::Login.id()
            }
            S2c::KeepAlive { id } => {
                w.i64(*id);
                PlayIn::KeepAlive.id()
            }
            S2c::PlayerPosition {
                teleport_id,
                x,
                y,
                z,
                yaw,
                pitch,
                flags,
            } => {
                w.varint(*teleport_id);
                w.f64(*x);
                w.f64(*y);
                w.f64(*z);
                w.f64(0.0);
                w.f64(0.0);
                w.f64(0.0); // velocity
                w.f32(*yaw);
                w.f32(*pitch);
                w.i32(*flags as i32);
                PlayIn::PlayerPosition.id()
            }
            S2c::SetTime => {
                // The bot counts the packet, not its fields; game time and day time, ticking.
                w.i64(0);
                w.i64(0);
                w.bool(true);
                PlayIn::SetTime.id()
            }
            S2c::ForgetLevelChunk { x, z } => {
                // ChunkPos.toLong: x in the low 32 bits, z in the high.
                w.i64((i64::from(*z) << 32) | i64::from(*x as u32));
                PlayIn::ForgetLevelChunk.id()
            }
            S2c::BlockUpdate { pos, state } => {
                w.i64(crate::bytes::pack_position(pos.0, pos.1, pos.2));
                w.varint(*state as i32);
                PlayIn::BlockUpdate.id()
            }
            S2c::SetHealth {
                health,
                food,
                saturation,
            } => {
                w.f32(*health);
                w.varint(*food);
                w.f32(*saturation);
                PlayIn::SetHealth.id()
            }
            S2c::ChunkBatchFinished { size } => {
                w.varint(*size);
                PlayIn::ChunkBatchFinished.id()
            }
            S2c::LevelChunkWithLight { x, z, column } => {
                w.bytes(&column.packet_body(*x, *z)?);
                PlayIn::LevelChunkWithLight.id()
            }
            S2c::LoginCompression { .. } => {
                return Err(EncodeError::NotModelled("login_compression"));
            }
            S2c::LoginDisconnect { .. } => {
                return Err(EncodeError::NotModelled("login_disconnect"));
            }
            S2c::ConfigPing { .. } => return Err(EncodeError::NotModelled("config ping")),
            S2c::ConfigDisconnect { .. } => {
                return Err(EncodeError::NotModelled("config disconnect"));
            }
            S2c::Ping { .. } => return Err(EncodeError::NotModelled("ping")),
            S2c::SectionBlocksUpdate { .. } => {
                return Err(EncodeError::NotModelled("section_blocks_update"));
            }
            S2c::Respawn { .. } => return Err(EncodeError::NotModelled("respawn")),
            S2c::AddEntity {
                id,
                uuid,
                kind,
                x,
                y,
                z,
            } => {
                // ClientboundAddEntityPacket.write: then the velocity (an LpVec3, 0 is one zero
                // byte), pitch, yaw and head yaw as bytes, and the type's data.
                w.varint(*id);
                w.uuid(uuid);
                w.varint(*kind as i32);
                w.f64(*x);
                w.f64(*y);
                w.f64(*z);
                w.u8(0);
                w.u8(0);
                w.u8(0);
                w.u8(0);
                w.varint(0);
                PlayIn::AddEntity.id()
            }
            S2c::RemoveEntities { ids } => {
                w.varint(i32::try_from(ids.len()).map_err(|_| EncodeError::TooLong)?);
                for id in ids {
                    w.varint(*id);
                }
                PlayIn::RemoveEntities.id()
            }
            S2c::EntityPositionSync { id, x, y, z } => {
                // PositionMoveRotation then on-ground: the velocity, yaw and pitch the model
                // does not send are zero.
                w.varint(*id);
                w.f64(*x);
                w.f64(*y);
                w.f64(*z);
                w.f64(0.0);
                w.f64(0.0);
                w.f64(0.0);
                w.f32(0.0);
                w.f32(0.0);
                w.bool(true);
                PlayIn::EntityPositionSync.id()
            }
            S2c::SetEntityData { id, entries } => {
                w.varint(*id);
                crate::metadata::write_metadata(&mut w, entries)?;
                PlayIn::SetEntityData.id()
            }
            S2c::MoveEntity { .. } | S2c::MoveEntityRot { .. } | S2c::TeleportEntity { .. } => {
                return Err(EncodeError::NotModelled("entity moves"));
            }
            S2c::ContainerSetContent {
                window,
                state_id,
                slots,
                carried,
            } => {
                w.varint(*window);
                w.varint(*state_id);
                w.varint(i32::try_from(slots.len()).map_err(|_| EncodeError::TooLong)?);
                for s in slots {
                    crate::slot::write_slot(&mut w, *s);
                }
                crate::slot::write_slot(&mut w, carried.unwrap_or(crate::slot::Slot::Empty));
                PlayIn::ContainerSetContent.id()
            }
            S2c::ContainerSetSlot {
                window,
                state_id,
                slot,
                item,
            } => {
                w.varint(*window);
                w.varint(*state_id);
                w.i16(*slot);
                crate::slot::write_slot(&mut w, *item);
                PlayIn::ContainerSetSlot.id()
            }
            S2c::SetPlayerInventory { slot, item } => {
                w.varint(*slot);
                crate::slot::write_slot(&mut w, *item);
                PlayIn::SetPlayerInventory.id()
            }
            S2c::BlockChangedAck { sequence } => {
                w.varint(*sequence);
                PlayIn::BlockChangedAck.id()
            }
            S2c::SetHeldSlot { slot } => {
                w.varint(*slot);
                PlayIn::SetHeldSlot.id()
            }
            S2c::TakeItemEntity {
                item,
                collector,
                amount,
            } => {
                w.varint(*item);
                w.varint(*collector);
                w.varint(*amount);
                PlayIn::TakeItemEntity.id()
            }
            S2c::SetCursorItem { item } => {
                crate::slot::write_slot(&mut w, *item);
                PlayIn::SetCursorItem.id()
            }
            S2c::ContainerSetData {
                window,
                property,
                value,
            } => {
                w.varint(*window);
                w.i16(*property);
                w.i16(*value);
                PlayIn::ContainerSetData.id()
            }
            S2c::SetEntityMotion { id, velocity } => {
                w.varint(*id);
                w.lpvec3(*velocity);
                PlayIn::SetEntityMotion.id()
            }
            S2c::EntityEvent { id, status } => {
                w.i32(*id);
                w.i8(*status);
                PlayIn::EntityEvent.id()
            }
            S2c::OpenScreen { window, menu } => {
                w.varint(*window);
                w.varint(*menu);
                // The title: an NBT string tag, as a server writes a plain text component.
                let title = b"Crafting";
                w.u8(8);
                w.bytes(&u16::try_from(title.len()).unwrap_or(0).to_be_bytes());
                w.bytes(title);
                PlayIn::OpenScreen.id()
            }
            S2c::ContainerClose { window } => {
                w.varint(*window);
                PlayIn::ContainerClose.id()
            }
            S2c::PlayerCombatKill { player_id, death } => {
                // The message as the server builds it; a death without a key goes as its text.
                let component = match &death.key {
                    Some(key) => crate::nbt::death_component(
                        key,
                        death.victim.as_deref().unwrap_or_default(),
                        death.by.as_deref(),
                    ),
                    None => crate::nbt::Nbt::String(death.text.clone()),
                };
                w.varint(*player_id);
                w.bytes(&crate::nbt::write_root(&component));
                PlayIn::PlayerCombatKill.id()
            }
            S2c::DamageEvent {
                entity_id,
                source_type,
                cause,
                direct,
            } => {
                w.varint(*entity_id);
                w.varint(*source_type);
                w.varint(cause.map_or(0, |c| c + 1));
                w.varint(direct.map_or(0, |d| d + 1));
                w.bool(false); // no source position
                PlayIn::DamageEvent.id()
            }
            S2c::StartConfiguration => return Err(EncodeError::NotModelled("start_configuration")),
            S2c::Disconnect { reason } => {
                w.bytes(reason);
                PlayIn::Disconnect.id()
            }
            S2c::Sound {
                sound,
                source,
                pos,
                volume,
                pitch,
            } => {
                write_sound(&mut w, sound)?;
                w.varint(*source);
                for v in pos {
                    w.i32((v * 8.0).floor() as i32);
                }
                w.f32(*volume);
                w.f32(*pitch);
                w.i64(0); // the seed: the bot draws nothing from it
                PlayIn::Sound.id()
            }
            S2c::SoundEntity {
                sound,
                source,
                entity,
                volume,
                pitch,
            } => {
                write_sound(&mut w, sound)?;
                w.varint(*source);
                w.varint(*entity);
                w.f32(*volume);
                w.f32(*pitch);
                w.i64(0);
                PlayIn::SoundEntity.id()
            }
            S2c::LevelEvent {
                event,
                pos,
                data,
                global,
            } => {
                w.i32(*event);
                w.i64(crate::bytes::pack_position(pos.0, pos.1, pos.2));
                w.i32(*data);
                w.bool(*global);
                PlayIn::LevelEvent.id()
            }
            S2c::StopSound { source, sound } => {
                w.u8(u8::from(source.is_some()) | (u8::from(sound.is_some()) << 1));
                if let Some(s) = source {
                    w.varint(*s);
                }
                if let Some(s) = sound {
                    w.string(s)?;
                }
                PlayIn::StopSound.id()
            }
            S2c::RotateHead { id, yaw } => {
                w.varint(*id);
                w.u8((yaw.rem_euclid(360.0) * 256.0 / 360.0).round() as u8);
                PlayIn::RotateHead.id()
            }
            S2c::SetEquipment { id, items } => {
                w.varint(*id);
                for (i, (slot, item)) in items.iter().enumerate() {
                    let more = if i + 1 < items.len() { 0x80 } else { 0 };
                    w.u8(slot | more);
                    crate::slot::write_slot(&mut w, *item);
                }
                PlayIn::SetEquipment.id()
            }
            S2c::Animate { id, action } => {
                w.varint(*id);
                w.u8(*action);
                PlayIn::Animate.id()
            }
            S2c::HurtAnimation { id, yaw } => {
                w.varint(*id);
                w.f32(*yaw);
                PlayIn::HurtAnimation.id()
            }
            S2c::ProjectilePower { id, power } => {
                w.varint(*id);
                w.f64(*power);
                PlayIn::ProjectilePower.id()
            }
            S2c::SetPassengers {
                vehicle,
                passengers,
            } => {
                w.varint(*vehicle);
                w.varint(i32::try_from(passengers.len()).unwrap_or(0));
                for p in passengers {
                    w.varint(*p);
                }
                PlayIn::SetPassengers.id()
            }
            // The model writes the fields the bot reads; a real server follows them with the
            // particles and sound, which the decoder does not read.
            S2c::Explode { center, knockback } => {
                center.iter().for_each(|v| w.f64(*v));
                w.bool(knockback.is_some());
                if let Some(k) = knockback {
                    k.iter().for_each(|v| w.f64(*v));
                }
                PlayIn::Explode.id()
            }
            S2c::BlockEntityData { pos, kind, spawns } => {
                use crate::nbt::Nbt;
                w.i64(crate::bytes::pack_position(pos.0, pos.1, pos.2));
                w.varint(*kind);
                let tag = match spawns {
                    Some(m) => Nbt::Compound(vec![(
                        "SpawnData".to_owned(),
                        Nbt::Compound(vec![(
                            "entity".to_owned(),
                            Nbt::Compound(vec![(
                                "id".to_owned(),
                                Nbt::String(format!("minecraft:{m}")),
                            )]),
                        )]),
                    )]),
                    None => Nbt::Compound(Vec::new()),
                };
                w.bytes(&crate::nbt::write_root(&tag));
                PlayIn::BlockEntityData.id()
            }
            S2c::BlockDestruction {
                breaker,
                pos,
                stage,
            } => {
                w.varint(*breaker);
                w.i64(crate::bytes::pack_position(pos.0, pos.1, pos.2));
                w.i8(*stage);
                PlayIn::BlockDestruction.id()
            }
            S2c::UpdateMobEffect {
                id,
                effect,
                amplifier,
                duration,
            } => {
                w.varint(*id);
                w.varint(*effect);
                w.varint(*amplifier);
                w.varint(*duration);
                w.u8(0); // flags: not ambient, no particles, no icon
                PlayIn::UpdateMobEffect.id()
            }
            S2c::RemoveMobEffect { id, effect } => {
                w.varint(*id);
                w.varint(*effect);
                PlayIn::RemoveMobEffect.id()
            }
            S2c::UpdateAttributes { id, attributes } => {
                w.varint(*id);
                w.varint(i32::try_from(attributes.len()).unwrap_or(0));
                for (attr, base, add) in attributes {
                    w.varint(*attr);
                    w.f64(*base);
                    if *add == 0.0 {
                        w.varint(0);
                    } else {
                        w.varint(1);
                        w.string("ruststeve:model")?;
                        w.f64(*add);
                        w.varint(0);
                    }
                }
                PlayIn::UpdateAttributes.id()
            }
            S2c::PlayerCombatEnter => PlayIn::PlayerCombatEnter.id(),
            S2c::PlayerCombatEnd { duration } => {
                w.varint(*duration);
                PlayIn::PlayerCombatEnd.id()
            }
            S2c::SetExperience {
                progress,
                level,
                total,
            } => {
                w.f32(*progress);
                w.varint(*level);
                w.varint(*total);
                PlayIn::SetExperience.id()
            }
            S2c::Cooldown { group, ticks } => {
                w.string(group)?;
                w.varint(*ticks);
                PlayIn::Cooldown.id()
            }
            S2c::GameEvent { event, param } => {
                w.u8(*event);
                w.f32(*param);
                PlayIn::GameEvent.id()
            }
            S2c::SystemChat { text, overlay } => {
                write_text(&mut w, text);
                w.bool(*overlay);
                PlayIn::SystemChat.id()
            }
            S2c::ActionBar { text } => {
                write_text(&mut w, text);
                PlayIn::SetActionBarText.id()
            }
            S2c::Title { text } => {
                write_text(&mut w, text);
                PlayIn::SetTitleText.id()
            }
            S2c::Subtitle { text } => {
                write_text(&mut w, text);
                PlayIn::SetSubtitleText.id()
            }
            S2c::BossEvent { uuid, op } => {
                w.uuid(uuid);
                match op {
                    BossOp::Add { name, progress } => {
                        w.varint(0);
                        write_text(&mut w, name);
                        w.f32(*progress);
                        w.varint(0); // colour
                        w.varint(0); // overlay
                        w.u8(0); // flags
                    }
                    BossOp::Remove => w.varint(1),
                    BossOp::Progress(p) => {
                        w.varint(2);
                        w.f32(*p);
                    }
                    BossOp::Name(n) => {
                        w.varint(3);
                        write_text(&mut w, n);
                    }
                    BossOp::Style => {
                        w.varint(5);
                        w.u8(0);
                    }
                }
                PlayIn::BossEvent.id()
            }
            S2c::ChunksBiomes { chunks } => {
                w.varint(i32::try_from(chunks.len()).unwrap_or(0));
                for (x, z) in chunks {
                    w.i64((i64::from(*x) & 0xffff_ffff) | (i64::from(*z) << 32));
                    w.varint(0);
                }
                PlayIn::ChunksBiomes.id()
            }
            S2c::SetChunkCacheCenter { x, z } => {
                w.varint(*x);
                w.varint(*z);
                PlayIn::SetChunkCacheCenter.id()
            }
            S2c::SetChunkCacheRadius { radius } => {
                w.varint(*radius);
                PlayIn::SetChunkCacheRadius.id()
            }
            S2c::SetSimulationDistance { distance } => {
                w.varint(*distance);
                PlayIn::SetSimulationDistance.id()
            }
            S2c::Unknown { .. } => return Err(EncodeError::NotModelled("an unknown packet")),
        };
        Ok((id, w.buf))
    }
}
