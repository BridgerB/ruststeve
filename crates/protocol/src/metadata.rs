//! Entity metadata, skimmed (brain/wire/wire-entity-metadata.md): `[index u8][serializer
//! varint][value]` until index 255. Serializer ids are looked up by name in the generated
//! `ENTITY_DATA_SERIALIZERS` (the game's registration order, never typed here). At the first
//! serializer without a reader the skim stops and keeps what it has, so it can never desync.

use data::generated::metadata::ENTITY_DATA_SERIALIZERS;

use crate::ParseError;
use crate::bytes::Reader;
use crate::slot::{Slot, read_slot};

/// The end of the entries.
const END: u8 = 0xff;

/// One metadata value, for the serializers the bot reads (the ones clojurecraft reads).
#[derive(Clone, Debug, PartialEq)]
pub enum MetaValue {
    Byte(u8),
    Int(i32),
    Long(i64),
    Float(f32),
    String(String),
    Item(Slot),
    Bool(bool),
    Direction(i32),
    /// An optional block state: `None` is the empty block.
    BlockState(Option<u32>),
    Pose(i32),
}

fn serializer_name(id: i32) -> Option<&'static str> {
    let id = u32::try_from(id).ok()?;
    ENTITY_DATA_SERIALIZERS
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.name)
}

/// Every entry up to the end or the first value this layer cannot read.
pub fn read_metadata(r: &mut Reader<'_>) -> Result<Vec<(u8, MetaValue)>, ParseError> {
    let mut out = Vec::new();
    loop {
        let index = r.u8()?;
        if index == END {
            return Ok(out);
        }
        let serializer = r.varint()?;
        let value = match serializer_name(serializer) {
            Some("BYTE") => MetaValue::Byte(r.u8()?),
            Some("INT") => MetaValue::Int(r.varint()?),
            Some("LONG") => MetaValue::Long(r.varlong()?),
            Some("FLOAT") => MetaValue::Float(r.f32()?),
            Some("STRING") => MetaValue::String(r.string()?),
            Some("BOOLEAN") => MetaValue::Bool(r.bool()?),
            Some("DIRECTION") => MetaValue::Direction(r.varint()?),
            Some("POSE") => MetaValue::Pose(r.varint()?),
            Some("OPTIONAL_BLOCK_STATE") => {
                let v = r.varint()?;
                MetaValue::BlockState((v != 0).then_some(v as u32))
            }
            Some("ITEM_STACK") => {
                let slot = read_slot(r)?;
                out.push((index, MetaValue::Item(slot)));
                if slot.truncated() {
                    return Ok(out);
                }
                continue;
            }
            // A serializer with no reader here: its width is unknown, so nothing after it can
            // be read.
            Some(_) | None => return Ok(out),
        };
        out.push((index, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::Writer;

    fn id(name: &str) -> i32 {
        ENTITY_DATA_SERIALIZERS
            .iter()
            .find(|s| s.name == name)
            .and_then(|s| i32::try_from(s.id).ok())
            .unwrap()
    }

    #[test]
    fn reads_flags_and_health() {
        let mut w = Writer::new();
        w.u8(0);
        w.varint(id("BYTE"));
        w.u8(0x01);
        w.u8(9);
        w.varint(id("FLOAT"));
        w.f32(14.5);
        w.u8(0xff);
        let got = read_metadata(&mut Reader::new(&w.buf)).unwrap();
        assert_eq!(
            got,
            vec![(0, MetaValue::Byte(1)), (9, MetaValue::Float(14.5))]
        );
    }

    #[test]
    fn stops_at_a_serializer_it_cannot_read() {
        let mut w = Writer::new();
        w.u8(0);
        w.varint(id("BYTE"));
        w.u8(0);
        w.u8(2);
        w.varint(id("OPTIONAL_COMPONENT"));
        w.bytes(&[1, 2, 3, 4, 5]);
        let got = read_metadata(&mut Reader::new(&w.buf)).unwrap();
        assert_eq!(got, vec![(0, MetaValue::Byte(0))]);
    }
}
