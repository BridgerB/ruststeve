//! The primitives every packet is built from, big-endian (brain/wire/wire-primitives.md): a
//! `Reader` cursor whose every read is a `Result`, and a `Writer`.

use crate::{EncodeError, ParseError};

/// The longest varint (5 bytes) and varlong (10 bytes); anything longer is malformed.
const VARINT_MAX: usize = 5;
const VARLONG_MAX: usize = 10;

/// A cursor over a packet body. Reads past the end are `ParseError::Eof`, never a panic.
#[derive(Clone, Debug)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Bytes not yet read.
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    /// How many bytes have been read.
    pub fn consumed(&self) -> usize {
        self.pos
    }

    /// The unread rest, consumed.
    pub fn rest(&mut self) -> &'a [u8] {
        let rest = self.buf.get(self.pos..).unwrap_or(&[]);
        self.pos = self.buf.len();
        rest
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        let eof = ParseError::Eof {
            at: self.pos,
            need: n.saturating_sub(self.remaining()),
        };
        let end = self.pos.checked_add(n).ok_or(eof.clone())?;
        let bytes = self.buf.get(self.pos..end).ok_or(eof)?;
        self.pos = end;
        Ok(bytes)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ParseError> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }

    pub fn u8(&mut self) -> Result<u8, ParseError> {
        Ok(self.array::<1>()?[0])
    }

    pub fn i8(&mut self) -> Result<i8, ParseError> {
        Ok(i8::from_be_bytes(self.array()?))
    }

    /// Any nonzero byte is true.
    pub fn bool(&mut self) -> Result<bool, ParseError> {
        Ok(self.u8()? != 0)
    }

    pub fn u16(&mut self) -> Result<u16, ParseError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    pub fn i32(&mut self) -> Result<i32, ParseError> {
        Ok(i32::from_be_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32, ParseError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    pub fn i64(&mut self) -> Result<i64, ParseError> {
        Ok(i64::from_be_bytes(self.array()?))
    }

    pub fn f32(&mut self) -> Result<f32, ParseError> {
        Ok(f32::from_be_bytes(self.array()?))
    }

    pub fn f64(&mut self) -> Result<f64, ParseError> {
        Ok(f64::from_be_bytes(self.array()?))
    }

    /// LEB128, at most 5 bytes, sign-extended to i32.
    pub fn varint(&mut self) -> Result<i32, ParseError> {
        let mut value: u32 = 0;
        for i in 0..VARINT_MAX {
            let b = self.u8()?;
            value |= u32::from(b & 0x7f) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(value as i32);
            }
        }
        Err(ParseError::VarIntTooLong { max: VARINT_MAX })
    }

    /// LEB128, at most 10 bytes, as i64.
    pub fn varlong(&mut self) -> Result<i64, ParseError> {
        let mut value: u64 = 0;
        for i in 0..VARLONG_MAX {
            let b = self.u8()?;
            value |= u64::from(b & 0x7f) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(value as i64);
            }
        }
        Err(ParseError::VarIntTooLong { max: VARLONG_MAX })
    }

    /// A varint length that must not be negative.
    pub fn length(&mut self) -> Result<usize, ParseError> {
        let n = self.varint()?;
        usize::try_from(n).map_err(|_| ParseError::NegativeLength(i64::from(n)))
    }

    /// A varint **byte** length, then UTF-8.
    pub fn string(&mut self) -> Result<String, ParseError> {
        let n = self.length()?;
        let bytes = self.take(n)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| ParseError::BadUtf8)
    }

    /// 16 raw bytes, most significant first.
    pub fn uuid(&mut self) -> Result<[u8; 16], ParseError> {
        self.array()
    }

    /// One i64: x 26 bits, z 26 bits, y 12 bits, each signed.
    pub fn position(&mut self) -> Result<(i32, i32, i32), ParseError> {
        Ok(unpack_position(self.i64()?))
    }
}

/// `x << 38 | z << 12 | y`, each field signed: arithmetic shifts bring negatives back.
pub fn unpack_position(v: i64) -> (i32, i32, i32) {
    let x = v >> 38;
    let y = (v << 52) >> 52;
    let z = (v << 26) >> 38;
    (x as i32, y as i32, z as i32)
}

pub fn pack_position(x: i32, y: i32, z: i32) -> i64 {
    ((i64::from(x) & 0x3ff_ffff) << 38)
        | ((i64::from(z) & 0x3ff_ffff) << 12)
        | (i64::from(y) & 0xfff)
}

/// A packet body being written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn i8(&mut self, v: i8) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn bool(&mut self, v: bool) {
        self.buf.push(u8::from(v));
    }

    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn f64(&mut self, v: f64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    /// A negative goes out as its unsigned 32-bit form, 5 bytes.
    pub fn varint(&mut self, v: i32) {
        write_varint(&mut self.buf, v);
    }

    pub fn string(&mut self, s: &str) -> Result<(), EncodeError> {
        let n = i32::try_from(s.len()).map_err(|_| EncodeError::TooLong)?;
        self.varint(n);
        self.buf.extend_from_slice(s.as_bytes());
        Ok(())
    }

    pub fn uuid(&mut self, u: &[u8; 16]) {
        self.buf.extend_from_slice(u);
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
}

pub fn write_varint(out: &mut Vec<u8>, v: i32) {
    let mut u = v as u32;
    loop {
        if u & !0x7f == 0 {
            out.push(u as u8);
            return;
        }
        out.push((u as u8 & 0x7f) | 0x80);
        u >>= 7;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint_bytes(v: i32) -> Vec<u8> {
        let mut out = Vec::new();
        write_varint(&mut out, v);
        out
    }

    #[test]
    fn varint_table() {
        // The protocol's own examples.
        assert_eq!(varint_bytes(0), [0x00]);
        assert_eq!(varint_bytes(1), [0x01]);
        assert_eq!(varint_bytes(127), [0x7f]);
        assert_eq!(varint_bytes(128), [0x80, 0x01]);
        assert_eq!(varint_bytes(255), [0xff, 0x01]);
        assert_eq!(varint_bytes(2_147_483_647), [0xff, 0xff, 0xff, 0xff, 0x07]);
        assert_eq!(varint_bytes(-1), [0xff, 0xff, 0xff, 0xff, 0x0f]);
        for v in [0, 1, 127, 128, 255, 2_147_483_647, -1, i32::MIN] {
            assert_eq!(Reader::new(&varint_bytes(v)).varint(), Ok(v));
        }
    }

    #[test]
    fn varint_too_long_is_an_error() {
        let bytes = [0x80, 0x80, 0x80, 0x80, 0x80, 0x01];
        assert_eq!(
            Reader::new(&bytes).varint(),
            Err(ParseError::VarIntTooLong { max: 5 })
        );
    }

    #[test]
    fn positions_keep_their_signs() {
        for (x, y, z) in [
            (0, 0, 0),
            (-1, -64, -1),
            (33_554_431, 2047, -33_554_432),
            (100, 70, -200),
        ] {
            assert_eq!(unpack_position(pack_position(x, y, z)), (x, y, z));
        }
    }

    #[test]
    fn reading_past_the_end_is_eof() {
        let mut r = Reader::new(&[1, 2]);
        assert_eq!(r.i32(), Err(ParseError::Eof { at: 0, need: 2 }));
    }
}
