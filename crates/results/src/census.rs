//! Issue 17a's measurements, read from files on a runner (never on the Mac):
//! - `player_dat`: a snapshot's player file (gunzipped first) as text, so its health, food,
//!   saturation and each tool's damage are read from the server's own record;
//! - `EndermanCensus`: over every senses file of past races, the seconds a bot had an enderman
//!   within 128 (`Senses::endermen`) and within 32 (a hostile row; endermen are monsters), by
//!   dimension and, in the overworld, by cave or surface and night or day. At the race's
//!   simulation distance 3 an enderman within 32 always ticks: its chunk is at most 2 chunks
//!   from the player's on either axis (`floor((p + 32) / 16) <= 2` for `p` in 0 to 15).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use data::EntityKind;
use protocol::bytes::Reader;
use protocol::nbt::{self, Nbt};

/// An uncompressed NBT file (a player `.dat` after `gunzip`) as indented text. A file's root is
/// a named compound: the name is dropped and the rest read as the network's anonymous root.
pub fn player_dat(raw: &[u8]) -> Result<String, String> {
    let (&tag, rest) = raw.split_first().ok_or("an empty file")?;
    if tag != 10 {
        return Err(format!("the root tag is {tag}, not a compound"));
    }
    let len = match rest {
        [a, b, ..] => usize::from(u16::from_be_bytes([*a, *b])),
        _ => return Err("no root name".to_owned()),
    };
    let body = rest
        .get(2 + len..)
        .ok_or("the root name runs past the end")?;
    let anon: Vec<u8> = std::iter::once(10).chain(body.iter().copied()).collect();
    let root = nbt::read_root(&mut Reader::new(&anon)).map_err(|e| format!("{e:?}"))?;
    let mut out = String::new();
    render(&root, 0, &mut out);
    Ok(out)
}

fn render(v: &Nbt, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    match v {
        Nbt::Compound(entries) => {
            out.push_str("{\n");
            for (k, e) in entries {
                let _ = write!(out, "{pad}  {k}: ");
                render(e, depth + 1, out);
            }
            let _ = writeln!(out, "{pad}}}");
        }
        Nbt::List(items) => {
            out.push_str("[\n");
            for e in items {
                let _ = write!(out, "{pad}  ");
                render(e, depth + 1, out);
            }
            let _ = writeln!(out, "{pad}]");
        }
        Nbt::String(s) => {
            let _ = writeln!(out, "{s:?}");
        }
        Nbt::ByteArray(a) => {
            let _ = writeln!(out, "bytes {a:?}");
        }
        Nbt::IntArray(a) => {
            let _ = writeln!(out, "ints {a:?}");
        }
        Nbt::LongArray(a) => {
            let _ = writeln!(out, "longs {a:?}");
        }
        other => {
            let _ = writeln!(out, "{}", nbt::plain(other));
        }
    }
}

/// Where a senses row was: the Nether, the overworld under y 50 (a cave, by height only: the
/// row carries no sky light) or above, the End, or unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Place {
    Nether,
    OverworldCave,
    OverworldSurface,
    End,
    Unknown,
}

/// Overworld rows under this y count as a cave (sea level is 63; a surface under 50 is rare).
const CAVE_BELOW: f64 = 50.0;

/// Night as the monster spawner sees it: the sky light is low enough from about 13,000 to
/// 23,000 (wiki "Daylight cycle"); rows with no clock are `None`.
fn night(day_time: Option<i64>) -> Option<bool> {
    day_time.map(|t| (13_000..23_000).contains(&t.rem_euclid(24_000)))
}

pub fn place(s: &::observe::Senses) -> Place {
    let y = s.pos.map_or(64.0, |p| p[1]);
    match s.dimension.as_deref() {
        Some("minecraft:the_nether") => Place::Nether,
        Some("minecraft:the_end") => Place::End,
        Some("minecraft:overworld") if y < CAVE_BELOW => Place::OverworldCave,
        Some("minecraft:overworld") => Place::OverworldSurface,
        _ => Place::Unknown,
    }
}

/// Seconds (rows) by place and night: all, with an enderman within 128, within 32.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Count {
    pub rows: u64,
    pub within_128: u64,
    pub within_32: u64,
}

#[derive(Clone, Debug, Default)]
pub struct EndermanCensus {
    pub by: BTreeMap<(Place, Option<bool>), Count>,
    /// Runs (files) with any row of an enderman within 32, by place.
    pub runs_within_32: BTreeMap<Place, u64>,
    pub runs: u64,
    pub unreadable: u64,
}

impl EndermanCensus {
    pub fn add(&mut self, rows: &[::observe::Senses]) {
        self.runs += 1;
        let mut near: BTreeMap<Place, bool> = BTreeMap::new();
        for s in rows {
            let p = place(s);
            let c = self.by.entry((p, night(s.day_time))).or_default();
            c.rows += 1;
            c.within_128 += u64::from(s.endermen > 0);
            let close = s
                .hostiles
                .iter()
                .any(|h| h.kind == EntityKind::Enderman && h.d <= 32.0);
            c.within_32 += u64::from(close);
            *near.entry(p).or_default() |= close;
        }
        for (p, close) in near {
            *self.runs_within_32.entry(p).or_default() += u64::from(close);
        }
    }

    pub fn render(&self) -> String {
        let mut s = format!(
            "{} runs read, {} files unreadable\n\n| place | night | seconds | enderman within 128 | within 32 |\n|---|---|---|---|---|\n",
            self.runs, self.unreadable
        );
        for ((p, n), c) in &self.by {
            let n = match n {
                Some(true) => "night",
                Some(false) => "day",
                None => "no clock",
            };
            let pct = |k: u64| 100.0 * k as f64 / c.rows.max(1) as f64;
            let _ = writeln!(
                s,
                "| {p:?} | {n} | {} | {} ({:.2}%) | {} ({:.2}%) |",
                c.rows,
                c.within_128,
                pct(c.within_128),
                c.within_32,
                pct(c.within_32)
            );
        }
        s.push_str("\n| place | runs with an enderman within 32 |\n|---|---|\n");
        for (p, k) in &self.runs_within_32 {
            let _ = writeln!(s, "| {p:?} | {k} |");
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_named_root_reads_as_text() {
        // {"": {Health: 17.5f}} with a root name "x".
        let mut raw = vec![10, 0, 1, b'x', 5, 0, 6];
        raw.extend_from_slice(b"Health");
        raw.extend_from_slice(&17.5f32.to_be_bytes());
        raw.push(0);
        let text = player_dat(&raw).unwrap_or_default();
        assert!(text.contains("Health: 17.5"), "{text}");
    }

    #[test]
    fn night_is_the_spawning_dark() {
        assert_eq!(night(Some(18_000)), Some(true));
        assert_eq!(night(Some(6_000)), Some(false));
        assert_eq!(night(None), None);
    }
}
