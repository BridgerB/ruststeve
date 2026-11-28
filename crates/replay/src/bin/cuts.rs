//! Where a recording's item stacks were cut short (issue 77, brain/wire/wire-slots.md): every
//! slot the protocol layer could not read whole, in entity data (`ITEM_STACK`), windows, the
//! cursor, the player's inventory and equipment, counted by where it was and the data component
//! it stopped at; every entity-data read that stopped early, by its serializer (the coverage
//! row's own count); and every packet that would not decode, by id. The first packets of each
//! are printed in hex, so a decoder test can be made of the real bytes. Counts are given a file
//! too, so they can be held against a lane's coverage.
//!
//!     cuts <recording.rsr> [<more.rsr> ...] [--examples <n>]
//!
//! It reads the recorded bytes with today's decoders; it never runs the bot.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use protocol::metadata::MetaValue;
use protocol::slot::Slot;
use protocol::{S2c, decode};
use record::{EventRecord, Record, Recording};

/// The packets kept in hex for each component, unless `--examples` says otherwise.
const EXAMPLES: usize = 3;

#[derive(Default)]
struct Cut {
    count: u64,
    /// (where, item name, packet hex)
    examples: Vec<(String, String, String)>,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn item_name(item: u32) -> String {
    data::ItemKind::from_id(item)
        .name()
        .map_or_else(|| format!("item {item}"), str::to_owned)
}

/// The cut slots of one packet: where each was, and the slot.
fn cut_slots(p: &S2c) -> Vec<(&'static str, Slot)> {
    let cut = |s: &Slot| s.truncated();
    match p {
        S2c::SetEntityData { entries, .. } => entries
            .iter()
            .filter_map(|(_, v)| match v {
                MetaValue::Item(s) if cut(s) => Some(("entity data", *s)),
                _ => None,
            })
            .collect(),
        S2c::ContainerSetContent { slots, .. } => slots
            .iter()
            .filter(|s| cut(s))
            .map(|s| ("window content", *s))
            .collect(),
        S2c::ContainerSetSlot { item, .. } if cut(item) => vec![("window slot", *item)],
        S2c::SetPlayerInventory { item, .. } if cut(item) => vec![("player inventory", *item)],
        S2c::SetCursorItem { item } if cut(item) => vec![("cursor", *item)],
        S2c::SetEquipment { items, .. } => items
            .iter()
            .filter(|(_, s)| cut(s))
            .map(|(_, s)| ("equipment", *s))
            .collect(),
        _ => Vec::new(),
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let examples = args
        .iter()
        .position(|a| a == "--examples")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(EXAMPLES);
    let files: Vec<&String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| {
            !a.starts_with("--")
                && args
                    .get(i.wrapping_sub(1))
                    .is_none_or(|p| p != "--examples")
        })
        .map(|(_, a)| a)
        .collect();
    if files.is_empty() {
        return Err("usage: cuts <recording.rsr> ... [--examples <n>]".to_owned());
    }
    let mut cuts: BTreeMap<String, Cut> = BTreeMap::new();
    let mut packets = 0u64;
    let keep = |cuts: &mut BTreeMap<String, Cut>, key: String, at: String, item: String, h| {
        let c = cuts.entry(key).or_default();
        c.count += 1;
        if c.examples.len() < examples {
            c.examples.push((at, item, h));
        }
    };
    for f in files {
        let (_, records) = Recording::read_all(Path::new(f))?;
        let mut here: BTreeMap<String, u64> = BTreeMap::new();
        for rec in &records {
            let Record::Event(EventRecord::Packet { state, id, body }) = rec else {
                continue;
            };
            packets += 1;
            let at = format!("{f} packet {id:#04x}");
            let p = match decode(*state, *id, body) {
                Ok(p) => p,
                Err(e) => {
                    let key = format!("decode error, packet {id:#04x}: {e:?}");
                    *here.entry(key.clone()).or_default() += 1;
                    keep(&mut cuts, key, at, String::new(), hex(body));
                    continue;
                }
            };
            if let S2c::SetEntityData { entries, .. } = &p
                && let Some(serializer) = protocol::metadata::unread(entries)
            {
                let key = format!("entity data stopped at {serializer}");
                *here.entry(key.clone()).or_default() += 1;
                keep(&mut cuts, key, at.clone(), String::new(), hex(body));
            }
            for (place, slot) in cut_slots(&p) {
                let component = slot.stopped_at().unwrap_or("unnamed");
                let key = format!("{place} slot cut at {component}");
                *here.entry(key.clone()).or_default() += 1;
                let item = slot.stack().map_or_else(String::new, |(i, _)| item_name(i));
                keep(&mut cuts, key, format!("{at} ({place})"), item, hex(body));
            }
        }
        for (key, n) in &here {
            println!("{f}: {key}: {n}");
        }
    }
    println!("packets read: {packets}");
    for (component, c) in &cuts {
        println!("\n== {component}: {}", c.count);
        for (place, item, h) in &c.examples {
            println!("- {place}, {item}\n  {h}");
        }
    }
    if cuts.is_empty() {
        println!("no slot cut short");
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cuts: {e}");
            ExitCode::FAILURE
        }
    }
}
