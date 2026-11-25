//! Snapshots (Bridger, 2026-11-19, `brain/ruststeve/decisions/one-bot-first.md`): when a bot first
//! reaches a stage, the race's server world is copied with what the bot holds in memory, so every
//! later attempt resumes from that real moment instead of from spawn or a made-up checkpoint. A
//! resumed bot takes the snapshot bot's player file (items, position, dimension, health) under its
//! own offline UUID.

use std::path::Path;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

/// What `snap.toml` says about a snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snap {
    /// The stage reached (a `Stage` name).
    pub stage: String,
    pub bot: String,
    /// The bot's offline UUID: its player file is `world/players/data/<uuid>.dat` (26.1.2).
    pub uuid: String,
    pub server_ticks: u64,
    pub commit: String,
    pub run: String,
    pub lane: u32,
    pub pos: [f64; 3],
    pub dimension: String,
    /// What the bot held, by item name (issue 16a: a snapshot's food is read without unpacking).
    #[serde(default)]
    pub items: std::collections::BTreeMap<String, i64>,
}

/// The UUID an offline-mode server gives `name` (vanilla's `UUIDUtil.createOfflinePlayerUUID`:
/// version 3, MD5 of `OfflinePlayer:<name>`), as the player file names it.
pub fn offline_uuid(name: &str) -> String {
    let mut b: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    b[6] = (b[6] & 0x0f) | 0x30;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// The bot's memory to a file: MessagePack with named fields, zstd (the recordings' encoding).
pub fn write_memory(path: &Path, memory: &memory::Memory) -> Result<(), String> {
    let bytes = rmp_serde::to_vec_named(memory).map_err(|e| e.to_string())?;
    let packed = zstd::encode_all(bytes.as_slice(), 3).map_err(|e| e.to_string())?;
    std::fs::write(path, packed).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn read_memory(path: &Path) -> Result<memory::Memory, String> {
    let packed = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes = zstd::decode_all(packed.as_slice()).map_err(|e| e.to_string())?;
    rmp_serde::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// A snapshot's memory as text (issue 19a.2: is the way home in memory?): per dimension, the
/// positions on record by their latest kind and the seen columns' extent; every portal and
/// obsidian position with its latest kind; every sighting ever of a kind in `list` (by name,
/// such as `nether_portal`), so a portal lit then broken shows both.
pub fn memory_report(m: &memory::Memory, list: &[String]) -> String {
    use std::collections::BTreeMap;
    use std::fmt::Write as _;
    let name = |state: u32| {
        data::block_of_state(state)
            .and_then(|k| k.info())
            .map_or("?", |i| i.name)
    };
    let mut out = String::new();
    let mut kinds: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let mut keep: BTreeMap<(&str, &str), Vec<memory::Cell>> = BTreeMap::new();
    for ((dim, pos), i) in m.latest_index() {
        let Some(s) = m.log().get(*i) else { continue };
        let n = name(s.state);
        *kinds.entry((dim.as_str(), n)).or_default() += 1;
        if matches!(n, "nether_portal" | "obsidian" | "end_portal_frame") {
            keep.entry((dim.as_str(), n)).or_default().push(*pos);
        }
    }
    let _ = writeln!(
        out,
        "sightings {} ever, {} positions on record",
        m.log().len(),
        m.latest_index().len()
    );
    let dims: std::collections::BTreeSet<&str> = kinds.keys().map(|(d, _)| *d).collect();
    for d in dims {
        let cols = m.seen_columns(d);
        let (x0, x1) = cols
            .iter()
            .fold((i32::MAX, i32::MIN), |(a, b), c| (a.min(c.0), b.max(c.0)));
        let (z0, z1) = cols
            .iter()
            .fold((i32::MAX, i32::MIN), |(a, b), c| (a.min(c.1), b.max(c.1)));
        let _ = writeln!(
            out,
            "\n[{d}] {} columns seen, chunk x {x0}..{x1}, z {z0}..{z1}",
            cols.len()
        );
        for ((_, k), n) in kinds.range((d, "")..=(d, "\u{10ffff}")) {
            let _ = writeln!(out, "  {k}: {n}");
        }
        for ((_, k), cells) in keep.range((d, "")..=(d, "\u{10ffff}")) {
            let _ = writeln!(out, "  latest {k} cells ({}):", cells.len());
            for c in cells {
                let _ = writeln!(out, "    {} {} {}", c.0, c.1, c.2);
            }
        }
    }
    if !list.is_empty() {
        let _ = writeln!(out, "\nevery sighting of {}:", list.join(", "));
        for s in m.log() {
            let n = name(s.state);
            let short = n.trim_start_matches("minecraft:");
            if list.iter().any(|l| l == short || l == n) {
                let _ = writeln!(
                    out,
                    "  {} {} {} {} {n} state {} at tick {}",
                    s.dimension, s.pos.0, s.pos.1, s.pos.2, s.state, s.at_tick
                );
            }
        }
    }
    out
}

/// The plan's eyes of ender state (issue 19c: the lines thrown, the fix, the throws' tally) to
/// `eyes.rsw` beside `memory.rsw`, in the same encoding.
pub fn write_eyes(path: &Path, kept: &plan::eyes::Kept) -> Result<(), String> {
    let bytes = rmp_serde::to_vec_named(kept).map_err(|e| e.to_string())?;
    let packed = zstd::encode_all(bytes.as_slice(), 3).map_err(|e| e.to_string())?;
    std::fs::write(path, packed).map_err(|e| format!("{}: {e}", path.display()))
}

/// The eyes state a snapshot kept; a snapshot from before issue 19c has no `eyes.rsw` and reads
/// as the default (no lines, no fix).
pub fn read_eyes(path: &Path) -> Result<plan::eyes::Kept, String> {
    if !path.exists() {
        return Ok(plan::eyes::Kept::default());
    }
    let packed = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes = zstd::decode_all(packed.as_slice()).map_err(|e| e.to_string())?;
    rmp_serde::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Copy a directory tree, leaving out the server's `session.lock` (held while it runs).
pub fn copy_dir(from: &Path, to: &Path) -> Result<u64, String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let mut n = 0;
    for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if name == "session.lock" {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(&name));
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            n += copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| format!("{}: {e}", src.display()))?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The widely published offline UUID for Notch.
    #[test]
    fn the_offline_uuid_matches_vanilla() {
        assert_eq!(
            offline_uuid("Notch"),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }

    /// Issue 19c: a plan's eyes state goes out to `eyes.rsw` and comes back into a fresh plan
    /// whole; a snapshot without the file reads as no lines and no fix.
    #[test]
    fn the_eyes_round_trip_through_the_snapshot_files() {
        use plan::eyes::{EyeState, Fix, Kept, Line};
        let dir = std::env::temp_dir().join(format!("rs-snap-eyes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("eyes.rsw");
        let kept = Kept {
            fix: Some(Fix {
                point: [1_204.5, -873.25],
                err_along: 61.0,
                err_across: 9.5,
            }),
            throws: vec![
                Line {
                    from: [10.5, 20.5],
                    dir: [0.6, -0.8],
                    spread: 0.004,
                },
                Line {
                    from: [180.0, 25.0],
                    dir: [0.0, 0.0],
                    spread: 0.0,
                },
            ],
            state: EyeState {
                thrown: 3,
                kept: 2,
                lost: 0,
                shattered: 1,
                spots: vec![[10.5, 20.5], [180.0, 25.0]],
                noted_at: Some(4_400),
            },
        };
        let mut from = plan::Plan::new(plan::Aim::Hold);
        from.restore_eyes(kept.clone());
        write_eyes(&path, &from.eyes_kept()).expect("write");
        let mut to = plan::Plan::new(plan::Aim::Hold);
        to.restore_eyes(read_eyes(&path).expect("read"));
        assert_eq!(to.eyes_kept(), kept);
        assert_eq!(
            read_eyes(&dir.join("missing.rsw")).expect("missing"),
            Kept::default()
        );
        std::fs::remove_dir_all(&dir).expect("clean");
    }

    #[test]
    fn a_snap_round_trips() {
        let s = Snap {
            stage: "nether".to_owned(),
            bot: "rs-l00-b03".to_owned(),
            uuid: offline_uuid("rs-l00-b03"),
            server_ticks: 41_200,
            commit: "abc1234".to_owned(),
            run: "local".to_owned(),
            lane: 0,
            pos: [1.5, 64.0, -3.5],
            dimension: "minecraft:the_nether".to_owned(),
            items: [("cooked_beef".to_owned(), 6)].into_iter().collect(),
        };
        let text = toml::to_string(&s).expect("toml");
        assert_eq!(toml::from_str::<Snap>(&text).expect("back"), s);
    }
}
