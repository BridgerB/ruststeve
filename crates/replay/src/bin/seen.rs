//! seen: every block of a kind a run saw with an exposed face, and when (issue 80: how often a
//! pumpkin is in view on an overworld leg, `brain/game/mc-pumpkin.md`). Folds the recording and,
//! every `--every` server ticks in the overworld, looks through the loaded columns within
//! `--chunks` of the feet for the kinds, each cell passing the planner's own no-X-ray test
//! (`plan::cast::seen`: a see-through neighbour). A section whose palette cannot hold the kind is
//! skipped unread. Prints each cell the first time it is seen (the tick, the bot's distance then,
//! and its distance from `--at x,y,z`, else from the feet at the first look), then a summary.
//!
//! ```text
//! cargo run -p replay --bin seen -- <file.rsr> [--kinds pumpkin,carved_pumpkin] [--every 200]
//!     [--chunks 6] [--at x,y,z] [--force]
//! ```

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use data::BlockKind;
use record::{COMMIT, EventRecord, Folder, Record, Recording, check_commit};
use world::{BlockPos, World};

/// The overworld's dimension name.
const OVERWORLD: &str = "minecraft:overworld";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("seen: {e}");
            ExitCode::FAILURE
        }
    }
}

fn value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn dist(a: [f64; 3], c: (i32, i32, i32)) -> f64 {
    let p = [
        f64::from(c.0) + 0.5,
        f64::from(c.1) + 0.5,
        f64::from(c.2) + 0.5,
    ];
    (0..3).map(|i| (p[i] - a[i]).powi(2)).sum::<f64>().sqrt()
}

/// The cells of `kinds` seen exposed in the loaded columns within `chunks` of the feet.
fn look(w: &World, feet: [f64; 3], chunks: i32, kinds: &[BlockKind]) -> Vec<(i32, i32, i32)> {
    let is = |s: u32| data::block_of_state(s).is_some_and(|k| kinds.contains(&k));
    let at = BlockPos::containing(feet[0], feet[1], feet[2]).chunk();
    let mut out = Vec::new();
    for (pos, col) in w
        .columns
        .range(..)
        .filter(|(p, _)| (p.x - at.x).abs() <= chunks && (p.z - at.z).abs() <= chunks)
    {
        for (i, s) in col.sections.iter().enumerate() {
            if !s.blocks.may_contain(is) {
                continue;
            }
            let base = col.min_y + 16 * i32::try_from(i).unwrap_or(0);
            for idx in 0..4096i32 {
                let (y, z, x) = (
                    base + (idx >> 8),
                    pos.z * 16 + ((idx >> 4) & 15),
                    pos.x * 16 + (idx & 15),
                );
                out.push((x, y, z));
            }
        }
        // A block the server changed since the column came is in the overlay, not the column.
        if let Some(o) = w.overlay.get(pos) {
            out.extend(
                o.iter()
                    .filter(|(_, s)| is(**s))
                    .map(|(p, _)| (p.x, p.y, p.z)),
            );
        }
    }
    out.sort_unstable();
    out.dedup();
    out.into_iter()
        .filter(|c| {
            w.block_at(BlockPos::new(c.0, c.1, c.2))
                .is_some_and(|k| kinds.contains(&k))
                && plan::cast::seen(w, *c).is_some()
        })
        .collect()
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let force = args.iter().any(|a| a == "--force");
    let takes = ["--kinds", "--every", "--chunks", "--at"];
    let file = args
        .iter()
        .enumerate()
        .find(|(i, a)| {
            !a.starts_with("--")
                && args
                    .get(i.wrapping_sub(1))
                    .is_none_or(|p| !takes.contains(&p.as_str()))
        })
        .map(|(_, a)| a.clone())
        .ok_or("usage: seen <file.rsr> [--kinds a,b] [--every ticks] [--chunks n] [--at x,y,z] [--force]")?;
    let kinds: Vec<BlockKind> = value(&args, "--kinds")
        .unwrap_or("pumpkin,carved_pumpkin")
        .split(',')
        .map(|n| BlockKind::from_name(n.trim()).ok_or(format!("no block {n}")))
        .collect::<Result<_, _>>()?;
    let every: u64 = value(&args, "--every")
        .map_or(Ok(200), str::parse)
        .map_err(|e| format!("--every: {e}"))?;
    let chunks: i32 = value(&args, "--chunks")
        .map_or(Ok(6), str::parse)
        .map_err(|e| format!("--chunks: {e}"))?;
    let mut reference: Option<[f64; 3]> = match value(&args, "--at") {
        Some(v) => {
            let n: Vec<f64> = v
                .split(',')
                .map(|s| s.trim().parse::<f64>().map_err(|e| format!("--at: {e}")))
                .collect::<Result<_, _>>()?;
            match n.as_slice() {
                [x, y, z] => Some([*x, *y, *z]),
                _ => return Err("--at needs x,y,z".to_owned()),
            }
        }
        None => None,
    };
    let mut r = Recording::open(Path::new(&file))?;
    let header = match r.next_record()? {
        Some(Record::Header(h)) => h,
        Some(_) | None => return Err("the recording has no header".to_owned()),
    };
    check_commit(&header, COMMIT, force)?;
    println!(
        "seen {kinds:?} in {} (recorded at {}, folded at {COMMIT}), every {every} ticks, {chunks} chunks",
        header.bot_name, header.commit
    );
    let mut folder = Folder::new();
    let mut next = 0u64;
    let mut looks = 0u32;
    let mut overworld_ticks = (None::<u64>, 0u64);
    let mut first: BTreeMap<(i32, i32, i32), (u64, f64)> = BTreeMap::new();
    while let Some(rec) = r.next_record()? {
        match rec {
            Record::Event(e) => {
                let tick = matches!(e, EventRecord::Tick { .. });
                folder.step(&e)?;
                let w = &folder.world;
                let at = w.session.server_ticks;
                let over = w.dimension.as_ref().is_some_and(|d| d.name == OVERWORLD);
                if !tick || at < next || !over {
                    continue;
                }
                let Some(b) = w.body else { continue };
                next = at + every;
                looks += 1;
                overworld_ticks = (overworld_ticks.0.or(Some(at)), at);
                let feet = b.pos;
                let refp = *reference.get_or_insert(feet);
                for c in look(w, feet, chunks, &kinds) {
                    if first.contains_key(&c) {
                        continue;
                    }
                    let kind = w.block_at(BlockPos::new(c.0, c.1, c.2));
                    let (d, dr) = (dist(feet, c), dist(refp, c));
                    println!(
                        "{at:>8} SEEN {kind:?} {},{},{} | bot {d:.1} away at {:.1},{:.1},{:.1} | {dr:.1} from the reference",
                        c.0, c.1, c.2, feet[0], feet[1], feet[2]
                    );
                    first.insert(c, (at, dr));
                }
            }
            Record::Checkpoint {
                event_index, world, ..
            } => {
                folder.take_start(event_index, &world);
            }
            Record::Header(_) | Record::Effects(_) | Record::Note(_) => {}
        }
    }
    let nearest = first
        .iter()
        .min_by(|a, b| a.1.1.total_cmp(&b.1.1))
        .map(|(c, (t, d))| {
            format!(
                "{},{},{} at tick {t}, {d:.1} from the reference",
                c.0, c.1, c.2
            )
        });
    let earliest = first
        .iter()
        .min_by_key(|(_, (t, _))| *t)
        .map(|(c, (t, d))| {
            format!(
                "{},{},{} at tick {t}, {d:.1} from the reference",
                c.0, c.1, c.2
            )
        });
    println!(
        "SUMMARY {} cells seen exposed in {looks} looks (overworld ticks {:?} to {}); reference {:?}; earliest {}; nearest {}; within 32 {}, 64 {}, 96 {}",
        first.len(),
        overworld_ticks.0,
        overworld_ticks.1,
        reference.map(|p| [p[0].round(), p[1].round(), p[2].round()]),
        earliest.unwrap_or_else(|| "none".to_owned()),
        nearest.unwrap_or_else(|| "none".to_owned()),
        first.values().filter(|(_, d)| *d <= 32.0).count(),
        first.values().filter(|(_, d)| *d <= 64.0).count(),
        first.values().filter(|(_, d)| *d <= 96.0).count(),
    );
    Ok(())
}
