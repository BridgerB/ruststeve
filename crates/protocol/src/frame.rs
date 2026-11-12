//! Frames: a varint length, then the packet; once compression is on, a varint data length (0 =
//! raw) before a zlib payload (brain/wire/wire-framing.md).

use std::io::{Read, Write};

use crate::bytes::{Reader, write_varint};
use crate::{EncodeError, ParseError};

/// The largest frame or inflated packet accepted (8 MiB, the vanilla client's own limit); a
/// bigger length is malformed, not a reason to allocate.
pub const MAX_FRAME: usize = 8 * 1024 * 1024;

/// Bytes in, complete packets out: partial frames wait for the rest.
#[derive(Clone, Debug, Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
    /// Set by `login_compression`: from then on every frame carries a data length.
    compression: bool,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Turn compression on (a negative threshold turns it off), as `login_compression` says.
    pub fn set_compression(&mut self, threshold: i32) {
        self.compression = threshold >= 0;
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Bytes received but not yet a whole frame.
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    /// The next complete packet (id and fields), inflated if it was compressed; `Ok(None)` until
    /// a whole frame has arrived. An error means the stream is malformed and cannot continue.
    pub fn next_packet(&mut self) -> Result<Option<Vec<u8>>, ParseError> {
        let mut r = Reader::new(&self.buf);
        let len = match r.varint() {
            Ok(n) => n,
            Err(ParseError::Eof { .. }) => return Ok(None),
            Err(e) => return Err(e),
        };
        let len = usize::try_from(len).map_err(|_| ParseError::NegativeLength(i64::from(len)))?;
        if len > MAX_FRAME {
            return Err(ParseError::FrameTooLarge(len as u64));
        }
        let header = r.consumed();
        if r.remaining() < len {
            return Ok(None);
        }
        let frame: Vec<u8> = self.buf.drain(..header + len).skip(header).collect();
        if !self.compression {
            return Ok(Some(frame));
        }
        let mut r = Reader::new(&frame);
        let data_len = r.length()?;
        let payload = r.rest();
        if data_len == 0 {
            return Ok(Some(payload.to_vec()));
        }
        if data_len > MAX_FRAME {
            return Err(ParseError::FrameTooLarge(data_len as u64));
        }
        let mut out = Vec::with_capacity(data_len);
        flate2::read::ZlibDecoder::new(payload)
            .take(data_len as u64 + 1)
            .read_to_end(&mut out)
            .map_err(|e| ParseError::Inflate(e.to_string()))?;
        if out.len() != data_len {
            return Err(ParseError::InflateLength {
                want: data_len,
                got: out.len(),
            });
        }
        Ok(Some(out))
    }
}

/// A packet's id and the bytes of its fields.
pub fn split_id(packet: &[u8]) -> Result<(u32, &[u8]), ParseError> {
    let mut r = Reader::new(packet);
    let id = r.varint()?;
    let id = u32::try_from(id).map_err(|_| ParseError::NegativeLength(i64::from(id)))?;
    Ok((id, r.rest()))
}

/// One frame for a packet: raw while compression is off; with it on, `0` + raw under the
/// threshold, else the length + zlib.
pub fn encode_frame(
    id: u32,
    body: &[u8],
    compression: Option<i32>,
) -> Result<Vec<u8>, EncodeError> {
    let mut packet = Vec::with_capacity(body.len() + 5);
    write_varint(
        &mut packet,
        i32::try_from(id).map_err(|_| EncodeError::TooLong)?,
    );
    packet.extend_from_slice(body);
    let inner = match compression {
        Some(threshold) if threshold >= 0 => {
            let mut inner = Vec::new();
            if packet.len() < usize::try_from(threshold).unwrap_or(usize::MAX) {
                write_varint(&mut inner, 0);
                inner.extend_from_slice(&packet);
            } else {
                write_varint(
                    &mut inner,
                    i32::try_from(packet.len()).map_err(|_| EncodeError::TooLong)?,
                );
                let mut z =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                z.write_all(&packet)
                    .map_err(|e| EncodeError::Deflate(e.to_string()))?;
                inner.extend(
                    z.finish()
                        .map_err(|e| EncodeError::Deflate(e.to_string()))?,
                );
            }
            inner
        }
        Some(_) | None => packet,
    };
    let mut frame = Vec::with_capacity(inner.len() + 5);
    write_varint(
        &mut frame,
        i32::try_from(inner.len()).map_err(|_| EncodeError::TooLong)?,
    );
    frame.extend(inner);
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(id: u32, body: &[u8], threshold: Option<i32>) {
        let frame = encode_frame(id, body, threshold).unwrap();
        let mut d = FrameDecoder::new();
        if let Some(t) = threshold {
            d.set_compression(t);
        }
        // Byte by byte: a partial frame must wait, not fail.
        for (i, b) in frame.iter().enumerate() {
            d.push(&[*b]);
            if i + 1 < frame.len() {
                assert_eq!(d.next_packet(), Ok(None));
            }
        }
        let packet = d.next_packet().unwrap().unwrap();
        assert_eq!(split_id(&packet).unwrap(), (id, body));
        assert_eq!(d.buffered(), 0);
    }

    #[test]
    fn frames_round_trip() {
        round_trip(0x1b, &[1, 2, 3], None);
        round_trip(0x1b, &[1, 2, 3], Some(256));
        round_trip(0x27, &[7; 1000], Some(256));
        round_trip(0, &[], Some(0));
    }

    #[test]
    fn an_oversized_frame_is_an_error() {
        let mut d = FrameDecoder::new();
        let mut len = Vec::new();
        write_varint(&mut len, 100_000_000);
        d.push(&len);
        assert_eq!(d.next_packet(), Err(ParseError::FrameTooLarge(100_000_000)));
    }
}
