//! Item stacks on the wire (brain/wire/wire-slots.md): a count, an item id, then a data-component
//! patch. A component this layer cannot step over ends what can be read of the frame, as
//! clojurecraft's `read-slot` does.

use data::generated::registries::DATA_COMPONENT_TYPE;

use crate::ParseError;
use crate::bytes::Reader;

/// One slot.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Empty,
    /// An item and how many (always at least 1).
    Item {
        item: u32,
        count: i32,
    },
    /// An item whose components could not all be read: the item and count are right, but the
    /// rest of the frame is unreadable after it.
    Truncated {
        item: u32,
        count: i32,
    },
}

impl Slot {
    /// The item and count, if the slot holds anything.
    pub fn stack(self) -> Option<(u32, i32)> {
        match self {
            Slot::Empty => None,
            Slot::Item { item, count } | Slot::Truncated { item, count } => Some((item, count)),
        }
    }

    pub fn truncated(self) -> bool {
        matches!(self, Slot::Truncated { .. })
    }
}

/// How a data component the bot can step over is written: a varint, or nothing (a unit). These
/// are clojurecraft's `component-codecs`, read from the game's `DataComponents` stream codecs.
enum Codec {
    VarInt,
    Unit,
}

fn codec(component_id: i32) -> Option<Codec> {
    let name = usize::try_from(component_id)
        .ok()
        .and_then(|i| DATA_COMPONENT_TYPE.get(i))?;
    match *name {
        "max_stack_size" | "max_damage" | "damage" | "repair_cost" => Some(Codec::VarInt),
        "unbreakable" => Some(Codec::Unit),
        _ => None,
    }
}

/// A slot: count (≤ 0 is empty), item, then the component patch (added count, removed count,
/// each added type with its value, each removed type).
pub fn read_slot(r: &mut Reader<'_>) -> Result<Slot, ParseError> {
    let count = r.varint()?;
    if count <= 0 {
        return Ok(Slot::Empty);
    }
    let item = r.varint()? as u32;
    let added = r.length()?;
    let removed = r.length()?;
    for _ in 0..added {
        match codec(r.varint()?) {
            Some(Codec::VarInt) => {
                r.varint()?;
            }
            Some(Codec::Unit) => {}
            None => return Ok(Slot::Truncated { item, count }),
        }
    }
    for _ in 0..removed {
        r.varint()?;
    }
    Ok(Slot::Item { item, count })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::Writer;

    fn component(name: &str) -> i32 {
        DATA_COMPONENT_TYPE
            .iter()
            .position(|n| *n == name)
            .and_then(|i| i32::try_from(i).ok())
            .unwrap()
    }

    #[test]
    fn a_worn_tool_reads_whole() {
        let mut w = Writer::new();
        w.varint(1);
        w.varint(820);
        w.varint(1);
        w.varint(0);
        w.varint(component("damage"));
        w.varint(17);
        w.u8(0xAB);
        let mut r = Reader::new(&w.buf);
        assert_eq!(
            read_slot(&mut r),
            Ok(Slot::Item {
                item: 820,
                count: 1
            })
        );
        assert_eq!(r.u8(), Ok(0xAB));
    }

    #[test]
    fn an_unknown_component_truncates() {
        let mut w = Writer::new();
        w.varint(3);
        w.varint(5);
        w.varint(1);
        w.varint(0);
        w.varint(component("custom_name"));
        let mut r = Reader::new(&w.buf);
        assert_eq!(read_slot(&mut r), Ok(Slot::Truncated { item: 5, count: 3 }));
    }

    #[test]
    fn zero_or_negative_counts_are_empty() {
        for n in [0, -4] {
            let mut w = Writer::new();
            w.varint(n);
            assert_eq!(read_slot(&mut Reader::new(&w.buf)), Ok(Slot::Empty));
        }
    }
}
