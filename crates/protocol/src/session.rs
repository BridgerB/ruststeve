//! The protocol-upkeep reducer: what keeps a session alive, answered as soon as the packet arrives
//! and never waiting on planning (brain/wire/wire-boundary-parsing.md). Pure: a packet in, the
//! replies out.

use crate::packets::{C2s, S2c};
use crate::state::State;
use crate::uuid::offline_uuid;

/// What the session asks the shell to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    Send(C2s),
    /// Compress every frame written after this (the server's `login_compression`).
    SetCompression(i32),
}

/// Ticks the server advances between two `set_time` packets (MinecraftServer synchronises time
/// every 20 ticks): the bot's game clock counts them (brain/ruststeve/decisions/sync-core-and-time.md).
pub const TICKS_PER_SET_TIME: u64 = 20;

/// A view distance to ask the server for: the race servers' own (simulation distance 3 there).
const VIEW_DISTANCE: i8 = 8;

/// The rate the client reports for chunk batches, chunks per tick (vanilla's default target).
const CHUNKS_PER_TICK: f32 = 20.0;

/// The connection as the upkeep sees it.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Session {
    /// The state the next packet from the server is read in.
    pub state: State,
    /// Server ticks seen in play: 20 per `set_time`.
    pub server_ticks: u64,
    pub keep_alives_answered: u64,
    pub teleports: u64,
    pub entity_id: Option<i32>,
    pub dimension: Option<String>,
    /// Whether `player_loaded` has been sent for this play session.
    loaded: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// The two packets that open a connection: the handshake into login, and the hello.
pub fn opening(protocol: i32, host: &str, port: u16, name: &str) -> [C2s; 2] {
    [
        C2s::Intention {
            protocol,
            host: host.to_owned(),
            port,
            next_state: 2,
        },
        C2s::Hello {
            username: name.to_owned(),
            uuid: offline_uuid(name),
        },
    ]
}

impl Session {
    /// A session whose `opening` has been sent: the server answers in login.
    pub fn new() -> Self {
        Self {
            state: State::Login,
            server_ticks: 0,
            keep_alives_answered: 0,
            teleports: 0,
            entity_id: None,
            dimension: None,
            loaded: false,
        }
    }

    /// Fold one packet from the server; return what must be sent back, in order.
    pub fn on_packet(&mut self, p: &S2c) -> Vec<Out> {
        let out = match p {
            S2c::LoginCompression { threshold } => vec![Out::SetCompression(*threshold)],
            S2c::LoginFinished { .. } => vec![
                Out::Send(C2s::LoginAcknowledged),
                Out::Send(C2s::ClientInformation {
                    locale: "en_us".to_owned(),
                    view_distance: VIEW_DISTANCE,
                }),
            ],
            S2c::SelectKnownPacks => vec![Out::Send(C2s::SelectKnownPacks)],
            S2c::ConfigKeepAlive { id } => {
                self.keep_alives_answered += 1;
                vec![Out::Send(C2s::ConfigKeepAlive { id: *id })]
            }
            S2c::ConfigPing { id } => vec![Out::Send(C2s::ConfigPong { id: *id })],
            S2c::FinishConfiguration => {
                self.loaded = false;
                vec![Out::Send(C2s::FinishConfiguration)]
            }
            S2c::Login {
                entity_id,
                dimension,
                ..
            } => {
                self.entity_id = Some(*entity_id);
                self.dimension = Some(dimension.clone());
                Vec::new()
            }
            S2c::KeepAlive { id } => {
                self.keep_alives_answered += 1;
                vec![Out::Send(C2s::KeepAlive { id: *id })]
            }
            S2c::Ping { id } => vec![Out::Send(C2s::Pong { id: *id })],
            S2c::PlayerPosition { teleport_id, .. } => {
                self.teleports += 1;
                let mut out = vec![Out::Send(C2s::AcceptTeleportation {
                    teleport_id: *teleport_id,
                })];
                // The client tells the server it has loaded the world once, after the first
                // teleport of a play session, as the vanilla client does.
                if !self.loaded {
                    self.loaded = true;
                    out.push(Out::Send(C2s::PlayerLoaded));
                }
                out
            }
            S2c::SetTime => {
                self.server_ticks += TICKS_PER_SET_TIME;
                Vec::new()
            }
            S2c::ChunkBatchFinished { .. } => vec![Out::Send(C2s::ChunkBatchReceived {
                chunks_per_tick: CHUNKS_PER_TICK,
            })],
            S2c::StartConfiguration => vec![Out::Send(C2s::ConfigurationAcknowledged)],
            S2c::LoginDisconnect { .. }
            | S2c::ConfigDisconnect { .. }
            | S2c::Disconnect { .. }
            | S2c::LevelChunkWithLight { .. }
            | S2c::ForgetLevelChunk { .. }
            | S2c::BlockUpdate { .. }
            | S2c::SectionBlocksUpdate { .. }
            | S2c::Respawn { .. }
            | S2c::SetHealth { .. }
            | S2c::AddEntity { .. }
            | S2c::RemoveEntities { .. }
            | S2c::MoveEntity { .. }
            | S2c::MoveEntityRot { .. }
            | S2c::EntityPositionSync { .. }
            | S2c::TeleportEntity { .. }
            | S2c::SetEntityData { .. }
            | S2c::ContainerSetContent { .. }
            | S2c::ContainerSetSlot { .. }
            | S2c::SetPlayerInventory { .. }
            | S2c::PlayerCombatKill { .. }
            | S2c::DamageEvent { .. }
            | S2c::BlockChangedAck { .. }
            | S2c::SetHeldSlot { .. }
            | S2c::TakeItemEntity { .. }
            | S2c::ContainerClose { .. }
            | S2c::SetCursorItem { .. }
            | S2c::OpenScreen { .. }
            | S2c::Unknown { .. } => Vec::new(),
        };
        self.state = self.state.after_receive(p);
        out
    }
}
