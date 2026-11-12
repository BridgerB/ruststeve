//! The connection states and the transitions between them (brain/wire/wire-connection-phases.md).

use crate::packets::{C2s, S2c};

/// The protocol state: the same packet id means different packets in different states.
#[derive(
    serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum State {
    Handshake,
    Login,
    Configuration,
    Play,
}

impl State {
    /// Every edge the connection may take. Status (a server-list ping) is never used.
    pub const EDGES: [(State, State); 4] = [
        (State::Handshake, State::Login),
        (State::Login, State::Configuration),
        (State::Configuration, State::Play),
        (State::Play, State::Configuration),
    ];

    pub fn is_edge(from: State, to: State) -> bool {
        Self::EDGES.contains(&(from, to))
    }

    /// The state the client writes in after sending `p`: the writer flips on the transition
    /// packet itself.
    pub fn after_send(self, p: &C2s) -> State {
        match p {
            C2s::Intention { .. } => State::Login,
            C2s::LoginAcknowledged | C2s::ConfigurationAcknowledged => State::Configuration,
            C2s::FinishConfiguration => State::Play,
            C2s::Hello { .. }
            | C2s::ClientInformation { .. }
            | C2s::SelectKnownPacks
            | C2s::ConfigKeepAlive { .. }
            | C2s::ConfigPong { .. }
            | C2s::KeepAlive { .. }
            | C2s::Pong { .. }
            | C2s::AcceptTeleportation { .. }
            | C2s::PlayerLoaded
            | C2s::ChunkBatchReceived { .. }
            | C2s::MovePlayerPosRot { .. }
            | C2s::MovePlayerStatusOnly { .. }
            | C2s::PlayerAction { .. }
            | C2s::SetCarriedItem { .. }
            | C2s::ContainerClick { .. }
            | C2s::ContainerClose { .. }
            | C2s::Swing { .. } => self,
        }
    }

    /// The state the reader decodes in after receiving `p`. The server's last packet in a state
    /// is the trigger (`login_finished`, `finish_configuration`, `start_configuration`), and it
    /// sends nothing in the next state until our acknowledgement arrives, so the reader can flip
    /// on the trigger by itself, with no word from the writer.
    pub fn after_receive(self, p: &S2c) -> State {
        // Each trigger moves only from its own state: a trigger met anywhere else (which `decode`
        // never produces) leaves the state alone rather than jump an edge the table lacks.
        match p {
            S2c::LoginFinished { .. } if self == State::Login => State::Configuration,
            S2c::FinishConfiguration if self == State::Configuration => State::Play,
            S2c::StartConfiguration if self == State::Play => State::Configuration,
            S2c::LoginFinished { .. } | S2c::FinishConfiguration | S2c::StartConfiguration => self,
            S2c::LoginCompression { .. }
            | S2c::LoginDisconnect { .. }
            | S2c::SelectKnownPacks
            | S2c::ConfigKeepAlive { .. }
            | S2c::ConfigPing { .. }
            | S2c::ConfigDisconnect { .. }
            | S2c::Login { .. }
            | S2c::KeepAlive { .. }
            | S2c::Ping { .. }
            | S2c::PlayerPosition { .. }
            | S2c::SetTime
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
            | S2c::ChunkBatchFinished { .. }
            | S2c::Disconnect { .. }
            | S2c::PlayerCombatKill { .. }
            | S2c::DamageEvent { .. }
            | S2c::BlockChangedAck { .. }
            | S2c::SetHeldSlot { .. }
            | S2c::TakeItemEntity { .. }
            | S2c::ContainerClose { .. }
            | S2c::SetCursorItem { .. }
            | S2c::Unknown { .. } => self,
        }
    }
}
