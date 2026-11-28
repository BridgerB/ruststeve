//! Item stacks on the wire (brain/wire/wire-slots.md): a count, an item id, then a data-component
//! patch. A component this layer cannot step over ends what can be read of the frame, as
//! clojurecraft's `read-slot` does; which one is kept, so coverage can name the next to read.
//! The tool's wear (`damage`, `max_damage`) is kept, not stepped over (issue 16e: a pickaxe
//! wearing out was invisible to the planner, blaze wall 7).

use data::generated::registries::DATA_COMPONENT_TYPE;

use crate::ParseError;
use crate::bytes::Reader;

/// One slot.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Empty,
    /// An item and how many (always at least 1), with its wear when the server sent it.
    Item {
        item: u32,
        count: i32,
        #[serde(default)]
        wear: Wear,
    },
    /// An item whose components could not all be read: the item and count are right, the wear
    /// is what was read before `stopped_at` (the component id with no reader), and the rest of
    /// the frame is unreadable after it.
    Truncated {
        item: u32,
        count: i32,
        #[serde(default)]
        wear: Wear,
        #[serde(default)]
        stopped_at: Option<i32>,
    },
}

/// A stack's wear as its components say: `damage` used of `max_damage` (an absent component is
/// the item's default: no damage, the generated `item_props::max_damage`).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wear {
    pub damage: Option<i32>,
    pub max_damage: Option<i32>,
}

impl Slot {
    /// An item with no components, as most stacks are.
    pub fn item(item: u32, count: i32) -> Self {
        Slot::Item {
            item,
            count,
            wear: Wear::default(),
        }
    }

    /// The item and count, if the slot holds anything.
    pub fn stack(self) -> Option<(u32, i32)> {
        match self {
            Slot::Empty => None,
            Slot::Item { item, count, .. } | Slot::Truncated { item, count, .. } => {
                Some((item, count))
            }
        }
    }

    /// The stack's wear, if the slot holds anything.
    pub fn wear(self) -> Option<Wear> {
        match self {
            Slot::Empty => None,
            Slot::Item { wear, .. } | Slot::Truncated { wear, .. } => Some(wear),
        }
    }

    pub fn truncated(self) -> bool {
        matches!(self, Slot::Truncated { .. })
    }

    /// The component that cut the slot short, by name.
    pub fn stopped_at(self) -> Option<&'static str> {
        match self {
            Slot::Truncated {
                stopped_at: Some(id),
                ..
            } => Some(component_name(id)),
            Slot::Truncated { .. } | Slot::Empty | Slot::Item { .. } => None,
        }
    }
}

/// A component id's name (`unknown` when the generated table lacks it).
pub fn component_name(id: i32) -> &'static str {
    usize::try_from(id)
        .ok()
        .and_then(|i| DATA_COMPONENT_TYPE.get(i))
        .copied()
        .unwrap_or("unknown")
}

/// How a data component the bot can step over is written: a varint, or nothing (a unit). These
/// are clojurecraft's `component-codecs`, read from the game's `DataComponents` stream codecs.
/// `potion_contents` (issue 77) is `PotionContents.STREAM_CODEC` from the 26.1.2 jar (javap,
/// probe 46151140850): a composite of an optional potion (`Potion.STREAM_CODEC`, a registry id
/// as a varint), an optional custom colour (`ByteBufCodecs.INT`, four bytes), the custom
/// effects (`MobEffectInstance.STREAM_CODEC` as a list) and an optional custom name
/// (`STRING_UTF8`). A witch's potion (race 46140744043) carries no custom effects.
enum Codec {
    VarInt,
    Unit,
    PotionContents,
}

fn codec(component_id: i32) -> Option<Codec> {
    match component_name(component_id) {
        "max_stack_size" | "max_damage" | "damage" | "repair_cost" => Some(Codec::VarInt),
        "unbreakable" => Some(Codec::Unit),
        "potion_contents" => Some(Codec::PotionContents),
        _ => None,
    }
}

/// Steps over a `potion_contents` value; `false` when it holds custom effects. A
/// `MobEffectInstance` is its effect then its `Details` (a recursive record whose stream codec
/// the jar read here has not shown), so a list that is not empty is left unread: the slot ends
/// as `Truncated` there, counted, never guessed.
fn skip_potion_contents(r: &mut Reader<'_>) -> Result<bool, ParseError> {
    if r.bool()? {
        r.varint()?;
    }
    if r.bool()? {
        r.i32()?;
    }
    if r.varint()? != 0 {
        return Ok(false);
    }
    if r.bool()? {
        r.string()?;
    }
    Ok(true)
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
    let mut wear = Wear::default();
    for _ in 0..added {
        let id = r.varint()?;
        match codec(id) {
            Some(Codec::VarInt) => {
                let v = r.varint()?;
                match component_name(id) {
                    "damage" => wear.damage = Some(v),
                    "max_damage" => wear.max_damage = Some(v),
                    _ => {}
                }
            }
            Some(Codec::Unit) => {}
            Some(Codec::PotionContents) => {
                if !skip_potion_contents(r)? {
                    return Ok(Slot::Truncated {
                        item,
                        count,
                        wear,
                        stopped_at: Some(id),
                    });
                }
            }
            None => {
                return Ok(Slot::Truncated {
                    item,
                    count,
                    wear,
                    stopped_at: Some(id),
                });
            }
        }
    }
    for _ in 0..removed {
        r.varint()?;
    }
    Ok(Slot::Item { item, count, wear })
}

/// A slot as a server writes it, for the server model: count, item, and a component patch of
/// its wear when it has any (the inverse of `read_slot` for the stacks the model makes).
pub fn write_slot(w: &mut crate::bytes::Writer, s: Slot) {
    match s.stack() {
        None => w.varint(0),
        Some((item, count)) => {
            w.varint(count);
            w.varint(item as i32);
            let wear = s.wear().unwrap_or_default();
            let added: Vec<(&str, i32)> =
                [("damage", wear.damage), ("max_damage", wear.max_damage)]
                    .into_iter()
                    .filter_map(|(n, v)| Some((n, v?)))
                    .collect();
            w.varint(i32::try_from(added.len()).unwrap_or(0));
            w.varint(0);
            for (name, v) in added {
                w.varint(component_id(name).unwrap_or(-1));
                w.varint(v);
            }
        }
    }
}

/// A component's id by name, from the generated table.
pub fn component_id(name: &str) -> Option<i32> {
    DATA_COMPONENT_TYPE
        .iter()
        .position(|n| *n == name)
        .and_then(|i| i32::try_from(i).ok())
}

/// A slot as a client writes it in a click (a "hashed" slot): present, item, count, no
/// component hashes. The bot always sends it empty: it predicts nothing
/// (brain/wire/wire-container-click.md).
pub fn write_hashed_slot(w: &mut crate::bytes::Writer, s: Slot) {
    match s.stack() {
        None => w.bool(false),
        Some((item, count)) => {
            w.bool(true);
            w.varint(item as i32);
            w.varint(count);
            w.varint(0);
            w.varint(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::Writer;

    fn component(name: &str) -> i32 {
        component_id(name).unwrap()
    }

    /// Blaze wall 7: a stone pickaxe's damage is kept, not stepped over.
    #[test]
    fn a_worn_tool_reads_whole_and_keeps_its_wear() {
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
                count: 1,
                wear: Wear {
                    damage: Some(17),
                    max_damage: None
                },
            })
        );
        assert_eq!(r.u8(), Ok(0xAB));
    }

    #[test]
    fn an_unknown_component_truncates_and_is_named() {
        let mut w = Writer::new();
        w.varint(3);
        w.varint(5);
        w.varint(1);
        w.varint(0);
        w.varint(component("custom_name"));
        let mut r = Reader::new(&w.buf);
        let s = read_slot(&mut r).unwrap();
        assert_eq!(
            s,
            Slot::Truncated {
                item: 5,
                count: 3,
                wear: Wear::default(),
                stopped_at: Some(component("custom_name")),
            }
        );
        assert_eq!(s.stopped_at(), Some("custom_name"));
    }

    /// Race 46140744043 lane 2 (rs-l02-b13), a witch's held potion in `set_equipment` (0x66),
    /// as the cuts probe 46150486488 printed it: entity 983, slot 0, then this stack. Its
    /// `potion_contents` (component 51) holds potion 13, no colour, no effects, no name.
    #[test]
    fn a_witchs_potion_reads_whole() {
        let packet: [u8; 14] = [
            0xd7, 0x07, 0x00, 0x01, 0xe2, 0x08, 0x01, 0x00, 0x33, 0x01, 0x0d, 0x00, 0x00, 0x00,
        ];
        assert_eq!(component_name(0x33), "potion_contents");
        let mut r = Reader::new(&packet);
        assert_eq!(r.varint(), Ok(983));
        assert_eq!(r.u8(), Ok(0));
        assert_eq!(read_slot(&mut r), Ok(Slot::item(1122, 1)));
        assert_eq!(r.remaining(), 0, "the stack read to the packet's end");
    }

    /// A colour and a name are stepped over; custom effects stop the slot, counted.
    #[test]
    fn potion_contents_steps_its_colour_and_name_and_stops_at_effects() {
        let potion = |effects: i32| {
            let mut w = Writer::new();
            w.varint(1);
            w.varint(1122);
            w.varint(1);
            w.varint(0);
            w.varint(component("potion_contents"));
            w.bool(false);
            w.bool(true);
            w.i32(0x00ff_0000);
            w.varint(effects);
            if effects == 0 {
                w.bool(true);
                w.string("Splash").unwrap();
            }
            w.u8(0xAB);
            w.buf
        };
        let whole = potion(0);
        let mut r = Reader::new(&whole);
        assert_eq!(read_slot(&mut r), Ok(Slot::item(1122, 1)));
        assert_eq!(r.u8(), Ok(0xAB));
        let effects = potion(1);
        let s = read_slot(&mut Reader::new(&effects)).unwrap();
        assert!(s.truncated());
        assert_eq!(s.stopped_at(), Some("potion_contents"));
    }

    #[test]
    fn a_written_worn_slot_reads_back() {
        let s = Slot::Item {
            item: 820,
            count: 1,
            wear: Wear {
                damage: Some(40),
                max_damage: Some(131),
            },
        };
        let mut w = Writer::new();
        write_slot(&mut w, s);
        assert_eq!(read_slot(&mut Reader::new(&w.buf)), Ok(s));
        let plain = Slot::item(7, 64);
        let mut w = Writer::new();
        write_slot(&mut w, plain);
        assert_eq!(read_slot(&mut Reader::new(&w.buf)), Ok(plain));
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
