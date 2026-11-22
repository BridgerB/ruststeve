//! Entity metadata, skimmed (brain/wire/wire-entity-metadata.md): `[index u8][serializer
//! varint][value]` until index 255. Serializer ids are looked up by name in the generated
//! `ENTITY_DATA_SERIALIZERS` (the game's registration order, never typed here). At the first
//! serializer without a reader the skim stops, keeps what it has and ends with an `Unread`
//! entry naming that serializer, so it can never desync and coverage can count the stop
//! (issue 16e: before, a stop was silent).

use data::generated::metadata::ENTITY_DATA_SERIALIZERS;

use crate::ParseError;
use crate::bytes::{Reader, pack_position};
use crate::nbt;
use crate::slot::{Slot, read_slot};

/// The end of the entries.
const END: u8 = 0xff;

/// One metadata value.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub enum MetaValue {
    Byte(u8),
    Int(i32),
    Long(i64),
    Float(f32),
    String(String),
    Item(Slot),
    Bool(bool),
    Direction(i32),
    /// An optional block state: `None` is the empty block (also a plain `BLOCK_STATE`).
    BlockState(Option<u32>),
    Pose(i32),
    /// A text component (`COMPONENT`, `OPTIONAL_COMPONENT`), as plain text.
    Text(Option<String>),
    /// Pitch, yaw, roll in degrees (an armour stand's limbs).
    Rotations([f32; 3]),
    /// A block position (`BLOCK_POS`, `OPTIONAL_BLOCK_POS`).
    BlockPos(Option<(i32, i32, i32)>),
    /// An optional entity UUID (`OPTIONAL_LIVING_ENTITY_REFERENCE`: a tamed mob's owner).
    EntityRef(Option<[u8; 16]>),
    /// An optional unsigned int (0 on the wire is absent).
    OptUnsigned(Option<u32>),
    /// A villager's type, profession and level.
    Villager([i32; 3]),
    /// A small enum by ordinal (sniffer, armadillo, copper golem states; the main arm).
    Enum(i32),
    /// A data-driven mob variant or sound variant, by its registry id.
    Variant(i32),
    /// A dimension and a position (a recovery compass's death spot).
    GlobalPos(Option<(String, (i32, i32, i32))>),
    Vector3([f32; 3]),
    Quaternion([f32; 4]),
    /// The serializer the read stopped at: nothing after it in this packet was read.
    Unread(String),
}

fn serializer_name(id: i32) -> Option<&'static str> {
    let id = u32::try_from(id).ok()?;
    ENTITY_DATA_SERIALIZERS
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.name)
}

fn text(r: &mut Reader<'_>) -> Result<String, ParseError> {
    Ok(nbt::plain(&nbt::read_root(r)?))
}

fn floats<const N: usize>(r: &mut Reader<'_>) -> Result<[f32; N], ParseError> {
    let mut out = [0.0; N];
    for v in &mut out {
        *v = r.f32()?;
    }
    Ok(out)
}

/// Every entry up to the end, or up to the first value this layer cannot read, which ends the
/// list as `Unread`.
pub fn read_metadata(r: &mut Reader<'_>) -> Result<Vec<(u8, MetaValue)>, ParseError> {
    let mut out = Vec::new();
    loop {
        let index = r.u8()?;
        if index == END {
            return Ok(out);
        }
        let serializer = r.varint()?;
        let name = serializer_name(serializer);
        let value = match name {
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
            Some("BLOCK_STATE") => MetaValue::BlockState(Some(r.varint()? as u32)),
            Some("COMPONENT") => MetaValue::Text(Some(text(r)?)),
            Some("OPTIONAL_COMPONENT") => {
                MetaValue::Text(if r.bool()? { Some(text(r)?) } else { None })
            }
            Some("ROTATIONS") => MetaValue::Rotations(floats(r)?),
            Some("BLOCK_POS") => MetaValue::BlockPos(Some(r.position()?)),
            Some("OPTIONAL_BLOCK_POS") => {
                MetaValue::BlockPos(if r.bool()? { Some(r.position()?) } else { None })
            }
            Some("OPTIONAL_LIVING_ENTITY_REFERENCE") => {
                MetaValue::EntityRef(if r.bool()? { Some(r.uuid()?) } else { None })
            }
            Some("OPTIONAL_UNSIGNED_INT") => {
                let v = r.varint()?;
                MetaValue::OptUnsigned((v != 0).then(|| (v - 1) as u32))
            }
            Some("VILLAGER_DATA") => MetaValue::Villager([r.varint()?, r.varint()?, r.varint()?]),
            // Data-driven mob variants and their sound variants are registry holders sent by id
            // only (`ByteBufCodecs.holderRegistry`): one varint. Race 45000585321 stopped 906
            // entity-data reads at four of them. The painting variant may be inline: not read.
            Some(
                "CAT_VARIANT"
                | "CAT_SOUND_VARIANT"
                | "COW_VARIANT"
                | "COW_SOUND_VARIANT"
                | "WOLF_VARIANT"
                | "WOLF_SOUND_VARIANT"
                | "FROG_VARIANT"
                | "PIG_VARIANT"
                | "PIG_SOUND_VARIANT"
                | "CHICKEN_VARIANT"
                | "CHICKEN_SOUND_VARIANT"
                | "ZOMBIE_NAUTILUS_VARIANT",
            ) => MetaValue::Variant(r.varint()?),
            Some(
                "SNIFFER_STATE"
                | "ARMADILLO_STATE"
                | "COPPER_GOLEM_STATE"
                | "WEATHERING_COPPER_STATE"
                | "HUMANOID_ARM",
            ) => MetaValue::Enum(r.varint()?),
            Some("OPTIONAL_GLOBAL_POS") => MetaValue::GlobalPos(if r.bool()? {
                Some((r.string()?, r.position()?))
            } else {
                None
            }),
            Some("VECTOR3") => MetaValue::Vector3(floats(r)?),
            Some("QUATERNION") => MetaValue::Quaternion(floats(r)?),
            Some("ITEM_STACK") => {
                let slot = read_slot(r)?;
                out.push((index, MetaValue::Item(slot)));
                if slot.truncated() {
                    out.push((index, MetaValue::Unread("ITEM_STACK".to_owned())));
                    return Ok(out);
                }
                continue;
            }
            // A serializer with no reader here (a particle, a profile, the mob variants held
            // by registry or inline): its width is unknown, so nothing after it can be read.
            Some(other) => {
                out.push((index, MetaValue::Unread(other.to_owned())));
                return Ok(out);
            }
            None => {
                out.push((index, MetaValue::Unread(format!("serializer {serializer}"))));
                return Ok(out);
            }
        };
        out.push((index, value));
    }
}

/// The serializer a read stopped at, if it stopped early.
pub fn unread(entries: &[(u8, MetaValue)]) -> Option<&str> {
    entries.iter().find_map(|(_, v)| {
        if let MetaValue::Unread(s) = v {
            Some(s.as_str())
        } else {
            None
        }
    })
}

fn serializer_id(name: &str) -> i32 {
    ENTITY_DATA_SERIALIZERS
        .iter()
        .find(|s| s.name == name)
        .and_then(|s| i32::try_from(s.id).ok())
        .unwrap_or(-1)
}

fn write_text(w: &mut crate::bytes::Writer, s: &str) {
    w.bytes(&nbt::write_root(&nbt::Nbt::String(s.to_owned())));
}

/// Entries as a server writes them, for the server model (the inverse of `read_metadata`). An
/// `Unread` entry is never written: the model makes none.
pub fn write_metadata(
    w: &mut crate::bytes::Writer,
    entries: &[(u8, MetaValue)],
) -> Result<(), crate::EncodeError> {
    for (index, value) in entries {
        if matches!(value, MetaValue::Unread(_)) {
            continue;
        }
        w.u8(*index);
        match value {
            MetaValue::Byte(b) => {
                w.varint(serializer_id("BYTE"));
                w.u8(*b);
            }
            MetaValue::Int(v) => {
                w.varint(serializer_id("INT"));
                w.varint(*v);
            }
            MetaValue::Long(v) => {
                w.varint(serializer_id("LONG"));
                w.varlong(*v);
            }
            MetaValue::Float(v) => {
                w.varint(serializer_id("FLOAT"));
                w.f32(*v);
            }
            MetaValue::String(s) => {
                w.varint(serializer_id("STRING"));
                w.string(s)?;
            }
            MetaValue::Item(s) => {
                w.varint(serializer_id("ITEM_STACK"));
                crate::slot::write_slot(w, *s);
            }
            MetaValue::Bool(b) => {
                w.varint(serializer_id("BOOLEAN"));
                w.bool(*b);
            }
            MetaValue::Direction(d) => {
                w.varint(serializer_id("DIRECTION"));
                w.varint(*d);
            }
            MetaValue::BlockState(s) => {
                w.varint(serializer_id("OPTIONAL_BLOCK_STATE"));
                w.varint(s.map_or(0, |v| v as i32));
            }
            MetaValue::Pose(p) => {
                w.varint(serializer_id("POSE"));
                w.varint(*p);
            }
            MetaValue::Text(t) => {
                w.varint(serializer_id("OPTIONAL_COMPONENT"));
                w.bool(t.is_some());
                if let Some(t) = t {
                    write_text(w, t);
                }
            }
            MetaValue::Rotations(v) => {
                w.varint(serializer_id("ROTATIONS"));
                v.iter().for_each(|f| w.f32(*f));
            }
            MetaValue::BlockPos(p) => {
                w.varint(serializer_id("OPTIONAL_BLOCK_POS"));
                w.bool(p.is_some());
                if let Some((x, y, z)) = p {
                    w.i64(pack_position(*x, *y, *z));
                }
            }
            MetaValue::EntityRef(u) => {
                w.varint(serializer_id("OPTIONAL_LIVING_ENTITY_REFERENCE"));
                w.bool(u.is_some());
                if let Some(u) = u {
                    w.uuid(u);
                }
            }
            MetaValue::OptUnsigned(v) => {
                w.varint(serializer_id("OPTIONAL_UNSIGNED_INT"));
                w.varint(v.map_or(0, |v| v as i32 + 1));
            }
            MetaValue::Villager(v) => {
                w.varint(serializer_id("VILLAGER_DATA"));
                v.iter().for_each(|x| w.varint(*x));
            }
            MetaValue::Enum(v) => {
                w.varint(serializer_id("HUMANOID_ARM"));
                w.varint(*v);
            }
            MetaValue::Variant(v) => {
                w.varint(serializer_id("COW_VARIANT"));
                w.varint(*v);
            }
            MetaValue::GlobalPos(g) => {
                w.varint(serializer_id("OPTIONAL_GLOBAL_POS"));
                w.bool(g.is_some());
                if let Some((dim, (x, y, z))) = g {
                    w.string(dim)?;
                    w.i64(pack_position(*x, *y, *z));
                }
            }
            MetaValue::Vector3(v) => {
                w.varint(serializer_id("VECTOR3"));
                v.iter().for_each(|f| w.f32(*f));
            }
            MetaValue::Quaternion(v) => {
                w.varint(serializer_id("QUATERNION"));
                v.iter().for_each(|f| w.f32(*f));
            }
            MetaValue::Unread(_) => {}
        }
    }
    w.u8(END);
    Ok(())
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

    /// A serializer with no reader ends the list with an `Unread` entry naming it (issue 16e:
    /// the stop is counted, not silent).
    #[test]
    fn stops_at_a_serializer_it_cannot_read_and_names_it() {
        let mut w = Writer::new();
        w.u8(0);
        w.varint(id("BYTE"));
        w.u8(0);
        w.u8(2);
        w.varint(id("PARTICLE"));
        w.bytes(&[1, 2, 3, 4, 5]);
        let got = read_metadata(&mut Reader::new(&w.buf)).unwrap();
        assert_eq!(
            got,
            vec![
                (0, MetaValue::Byte(0)),
                (2, MetaValue::Unread("PARTICLE".to_owned()))
            ]
        );
        assert_eq!(unread(&got), Some("PARTICLE"));
    }

    /// The serializers added in issue 16e read back what the model writes, and the read goes on
    /// past them to the next entry.
    #[test]
    fn the_new_serializers_round_trip() {
        let entries = vec![
            (2, MetaValue::Text(Some("Steve".to_owned()))),
            (3, MetaValue::Text(None)),
            (4, MetaValue::Rotations([1.0, 2.5, -3.0])),
            (5, MetaValue::BlockPos(Some((-12, 64, 300)))),
            (6, MetaValue::BlockPos(None)),
            (7, MetaValue::EntityRef(Some([7; 16]))),
            (8, MetaValue::OptUnsigned(Some(4))),
            (9, MetaValue::Villager([1, 2, 3])),
            (10, MetaValue::Enum(1)),
            (15, MetaValue::Variant(3)),
            (
                11,
                MetaValue::GlobalPos(Some(("minecraft:the_nether".to_owned(), (1, 2, 3)))),
            ),
            (12, MetaValue::Vector3([0.5, 0.25, 1.0])),
            (13, MetaValue::Quaternion([0.0, 0.0, 0.0, 1.0])),
            (14, MetaValue::Float(9.0)),
        ];
        let mut w = Writer::new();
        write_metadata(&mut w, &entries).unwrap();
        assert_eq!(read_metadata(&mut Reader::new(&w.buf)).unwrap(), entries);
    }
}
