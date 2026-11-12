//! Minecraft 26.1.2 (protocol 775) on the wire, as pure functions: primitives, framing and
//! compression, typed packets, the connection states, and the upkeep reducer that keeps a session
//! alive (keep-alives, pings, the teleport acknowledgement).
//!
//! No I/O, no clock: bytes in, values out. The socket lives in `net`
//! (brain/ruststeve/design/connection.md).

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod bytes;
pub mod chunk;
pub mod frame;
pub mod metadata;
pub mod packets;
pub mod session;
pub mod slot;
pub mod state;
pub mod uuid;

pub use bytes::{Reader, Writer};
pub use frame::{FrameDecoder, encode_frame, split_id};
pub use packets::{C2s, S2c, decode};
pub use session::{Out, Session};
pub use state::State;

/// Why bytes from the server could not be read. A boundary error: a value, never a panic, and
/// never silently dropped (brain/wire/wire-boundary-parsing.md).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("needed {need} more bytes at offset {at}")]
    Eof { at: usize, need: usize },
    #[error("a varint longer than {max} bytes")]
    VarIntTooLong { max: usize },
    #[error("a negative length {0}")]
    NegativeLength(i64),
    #[error("a frame of {0} bytes is over the limit")]
    FrameTooLarge(u64),
    #[error("a string that is not UTF-8")]
    BadUtf8,
    #[error("zlib: {0}")]
    Inflate(String),
    #[error("inflated {got} bytes, the frame said {want}")]
    InflateLength { want: usize, got: usize },
    #[error("a paletted container of {0} bits")]
    BadBits(u8),
    #[error("a chunk column of {0} sections, which no dimension has")]
    BadSectionCount(usize),
    #[error("a coordinate that is not finite")]
    NonFinite,
}

/// Why a packet could not be written: only compression can fail, and only in theory.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    #[error("zlib: {0}")]
    Deflate(String),
    #[error("a string or list too long for a varint length")]
    TooLong,
}
