//! Replay's pure part: verifying a recording's records (brain/ruststeve/design/replay-and-why.md).
//!
//! One fold from the start compares every step's effects with the recorded ones and the world at
//! every checkpoint with the recorded world. A checkpoint that matches proves a fold from it
//! would go as the fold already running does, so only two are folded again (the middle one and
//! the last), to prove `Folder::from_checkpoint` restores the reader's state. The work is linear
//! in the recording: the first version folded again from every checkpoint, quadratic, and a
//! three-hour recording took the lane's last hour (docs/problems/race-tooling.md).

use record::{EffectRecord, Folder, Record};

/// What one fold found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub events: u64,
    pub effects: u64,
    pub checkpoints: u64,
    pub mismatches: u64,
}

/// A fold started again from a checkpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refold {
    pub event_index: u64,
    pub server_ticks: u64,
    pub tally: Tally,
}

/// What `verify` found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub full: Tally,
    /// The recording begins with its start state (a checkpoint before the first event): a run
    /// resumed from a snapshot.
    pub start_taken: bool,
    /// The first checkpoint differed from the fold in the memory alone and was taken as the
    /// truth: a resumed run recorded before the start state was kept. Not a mismatch.
    pub memory_only: bool,
    pub refolds: Vec<Refold>,
    /// The first few differences, in words.
    pub lines: Vec<String>,
}

impl Report {
    pub fn mismatches(&self) -> u64 {
        self.full.mismatches + self.refolds.iter().map(|r| r.tally.mismatches).sum::<u64>()
    }

    /// Events stepped in all folds: the work done (the full fold's, plus each refold's).
    pub fn events_stepped(&self) -> u64 {
        self.full.events + self.refolds.iter().map(|r| r.tally.events).sum::<u64>()
    }
}

/// Differences kept in words, per fold.
const LINES_PER_FOLD: u64 = 3;

fn show(effects: &[EffectRecord]) -> String {
    effects
        .iter()
        .map(|e| match e {
            EffectRecord::Send { id, body } => format!("send id {id} ({} bytes)", body.len()),
            EffectRecord::SetCompression(t) => format!("compression {t}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Fold `records[start..]` on from `folder`, comparing as it goes and adding to `t`.
fn fold(
    folder: &mut Folder,
    records: &[Record],
    start: usize,
    label: &str,
    report: &mut Report,
    t: &mut Tally,
) -> Result<(), String> {
    let mut i = start;
    while let Some(rec) = records.get(i) {
        match rec {
            Record::Event(e) => {
                let produced = folder.step(e)?;
                t.events += 1;
                match records.get(i + 1) {
                    Some(Record::Effects(recorded)) => {
                        t.effects += recorded.len() as u64;
                        if &produced != recorded {
                            t.mismatches += 1;
                            if t.mismatches <= LINES_PER_FOLD {
                                report.lines.push(format!(
                                    "{label}: event {} ({e:?}) at {} server ticks: produced {}, recorded {}",
                                    folder.events - 1,
                                    folder.world.session.server_ticks,
                                    show(&produced),
                                    show(recorded)
                                ));
                            }
                        }
                        i += 2;
                        continue;
                    }
                    // A run killed between an event and its effects ends here.
                    None => break,
                    Some(other) => {
                        return Err(format!(
                            "record {} after an event is not its effects: {other:?}",
                            i + 1
                        ));
                    }
                }
            }
            Record::Checkpoint {
                event_index, world, ..
            } => {
                if folder.take_start(*event_index, world) {
                    report.start_taken = true;
                    i += 1;
                    continue;
                }
                t.checkpoints += 1;
                if folder.events != *event_index {
                    t.mismatches += 1;
                    if t.mismatches <= LINES_PER_FOLD {
                        report.lines.push(format!(
                            "{label}: the fold is at event {}, the checkpoint says event {event_index}",
                            folder.events
                        ));
                    }
                } else if folder.world != **world {
                    // An older resumed run: its seeded memory was never recorded, so the first
                    // checkpoint holds sightings the fold cannot have. Everything else equal,
                    // the checkpoint is the truth and the fold goes on from it.
                    let memory_only = t.checkpoints == 1 && !report.start_taken && {
                        let mut same = folder.world.clone();
                        same.memory = world.memory.clone();
                        same == **world
                    };
                    if memory_only {
                        report.memory_only = true;
                    } else {
                        t.mismatches += 1;
                        if t.mismatches <= LINES_PER_FOLD {
                            report.lines.push(format!(
                                "{label}: the world folded to event {event_index} differs from its checkpoint"
                            ));
                        }
                    }
                    // Either way the fold goes on from the recorded world, so one cause is
                    // counted once and not again at every later checkpoint.
                    folder.world = (**world).clone();
                }
            }
            Record::Effects(_) => {
                return Err(format!("record {i}: effects with no event before them"));
            }
            Record::Header(_) | Record::Note(_) => {}
        }
        i += 1;
    }
    Ok(())
}

/// Verify a recording's records (the header already read): one fold from the start, then a fold
/// from the middle checkpoint and one from the last.
pub fn verify(records: &[Record]) -> Result<Report, String> {
    let mut report = Report::default();
    let mut full = Tally::default();
    fold(
        &mut Folder::new(),
        records,
        0,
        "from the start",
        &mut report,
        &mut full,
    )?;
    report.full = full;
    // Checkpoints a fold can start from: every one after the first event.
    let at: Vec<usize> = records
        .iter()
        .enumerate()
        .filter(|(_, r)| matches!(r, Record::Checkpoint { event_index, .. } if *event_index > 0))
        .map(|(i, _)| i)
        .collect();
    let mut picks: Vec<usize> = Vec::new();
    if let Some(mid) = at.get(at.len() / 2) {
        picks.push(*mid);
    }
    if let Some(last) = at.last()
        && !picks.contains(last)
    {
        picks.push(*last);
    }
    for i in picks {
        if let Some(Record::Checkpoint {
            event_index,
            server_ticks,
            world,
        }) = records.get(i)
        {
            let label = format!("from checkpoint at event {event_index} ({server_ticks} ticks)");
            let mut tally = Tally::default();
            fold(
                &mut Folder::from_checkpoint((**world).clone(), *event_index),
                records,
                i + 1,
                &label,
                &mut report,
                &mut tally,
            )?;
            report.refolds.push(Refold {
                event_index: *event_index,
                server_ticks: *server_ticks,
                tally,
            });
        }
    }
    Ok(report)
}

/// `verify` for a recording of any length: the records are taken one at a time and held only
/// from one checkpoint to the next, so a five-hour recording (250 checkpoints, each a whole
/// world) needs the memory of one. The fold from the start is the same; the one fold again is
/// from the last checkpoint. Returns the report and how many records were read.
pub fn verify_stream(
    mut next: impl FnMut() -> Result<Option<Record>, String>,
) -> Result<(Report, u64), String> {
    let mut report = Report::default();
    let mut full = Tally::default();
    let mut folder = Folder::new();
    let mut window: Vec<Record> = Vec::new();
    let mut last: Option<Record> = None;
    let mut read = 0u64;
    while let Some(rec) = next()? {
        read += 1;
        let ends_window =
            matches!(&rec, Record::Checkpoint { event_index, .. } if *event_index > 0);
        window.push(rec);
        if ends_window {
            fold(
                &mut folder,
                &window,
                0,
                "from the start",
                &mut report,
                &mut full,
            )?;
            last = window.pop();
            window.clear();
        }
    }
    fold(
        &mut folder,
        &window,
        0,
        "from the start",
        &mut report,
        &mut full,
    )?;
    report.full = full;
    if let Some(Record::Checkpoint {
        event_index,
        server_ticks,
        world,
    }) = last
    {
        let label = format!("from checkpoint at event {event_index} ({server_ticks} ticks)");
        let mut tally = Tally::default();
        fold(
            &mut Folder::from_checkpoint(*world, event_index),
            &window,
            0,
            &label,
            &mut report,
            &mut tally,
        )?;
        report.refolds.push(Refold {
            event_index,
            server_ticks,
            tally,
        });
    }
    Ok((report, read))
}

/// A moment of a recording to fold to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum At {
    /// The first event at or after this many server ticks.
    Ticks(u64),
    /// After this many events.
    Event(u64),
}

/// Fold a recording to a moment, from the last checkpoint at or before it: the folder there and
/// the planner's notes up to it (from the start: a checkpoint holds the world, not the notes).
pub fn fold_to(records: &[Record], at: At) -> Result<(Folder, Vec<String>), String> {
    let reached = |f: &Folder| match at {
        At::Ticks(t) => f.world.session.server_ticks >= t,
        At::Event(n) => f.events >= n,
    };
    let mut folder = Folder::new();
    let mut start = 0;
    for (i, rec) in records.iter().enumerate() {
        if let Record::Checkpoint {
            event_index,
            server_ticks,
            world,
        } = rec
        {
            let before = match at {
                At::Ticks(t) => *server_ticks <= t,
                At::Event(n) => *event_index <= n,
            };
            if before {
                folder = Folder::from_checkpoint((**world).clone(), *event_index);
                start = i + 1;
            }
        }
    }
    let mut notes = Vec::new();
    for (i, rec) in records.iter().enumerate() {
        if i >= start && reached(&folder) {
            break;
        }
        match rec {
            Record::Note(n) => notes.push(n.clone()),
            Record::Event(e) if i >= start => folder.step(e).map(|_| ())?,
            _ => {}
        }
    }
    Ok((folder, notes))
}

/// `x,y,z` as three whole numbers.
pub fn parse_cell(s: &str) -> Result<[i32; 3], String> {
    let v: Vec<i32> = s
        .split(',')
        .map(|p| p.trim().parse().map_err(|e| format!("{s}: {e}")))
        .collect::<Result<_, _>>()?;
    match v[..] {
        [x, y, z] => Ok([x, y, z]),
        _ => Err(format!("{s}: not x,y,z")),
    }
}

/// The two-character code of the `n`th block name of a slice: `a` to `z`, `A` to `Z`, then
/// `aa`, `ab` and on.
fn code(n: usize) -> String {
    const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let letter = |i: usize| char::from(LETTERS[i % LETTERS.len()]);
    if n < LETTERS.len() {
        letter(n).to_string()
    } else {
        let n = n - LETTERS.len();
        format!("{}{}", letter(n / LETTERS.len()), letter(n))
    }
}

/// The blocks round a cell as the bot knew them: one layer for each height from `ys.1` down to
/// `ys.0`, rows north to south (z up), columns west to east (x up), `radius` cells each way. Each
/// cell is a short code; the legend under the layers gives every code's full block name and how
/// many cells hold it. `.` is air, `?` a cell not loaded, `*` marks the centre, `@` the cells
/// in `marks` (the bot's feet).
pub fn slice(
    name_at: &dyn Fn([i32; 3]) -> Option<String>,
    [cx, cy, cz]: [i32; 3],
    radius: i32,
    ys: (i32, i32),
    marks: &[[i32; 3]],
) -> String {
    use std::collections::BTreeMap;
    let mut codes: BTreeMap<String, (String, u32)> = BTreeMap::new();
    let mut out = format!(
        "blocks round ({cx}, {cy}, {cz}): x {} to {} across, z {} to {} down, y {} down to {}\n",
        cx - radius,
        cx + radius,
        cz - radius,
        cz + radius,
        ys.1,
        ys.0
    );
    let mut air = 0u32;
    let mut unknown = 0u32;
    for y in (ys.0..=ys.1).rev() {
        out.push_str(&format!("y {y}\n"));
        for z in cz - radius..=cz + radius {
            let mut row = format!("  z {z:>5} ");
            for x in cx - radius..=cx + radius {
                let cell = match name_at([x, y, z]) {
                    None => {
                        unknown += 1;
                        "?".to_owned()
                    }
                    Some(n) if n == "air" || n == "cave_air" || n == "void_air" => {
                        air += 1;
                        ".".to_owned()
                    }
                    Some(n) => {
                        let next = codes.len();
                        let entry = codes.entry(n).or_insert_with(|| (code(next), 0));
                        entry.1 += 1;
                        entry.0.clone()
                    }
                };
                let mark = if [x, y, z] == [cx, cy, cz] {
                    '*'
                } else if marks.contains(&[x, y, z]) {
                    '@'
                } else {
                    ' '
                };
                row.push_str(&format!("{mark}{cell:<2}"));
            }
            out.push_str(row.trim_end());
            out.push('\n');
        }
    }
    out.push_str("legend:\n");
    out.push_str(&format!("  .  air ({air})\n  ?  not loaded ({unknown})\n"));
    let mut legend: Vec<(&String, &(String, u32))> = codes.iter().collect();
    legend.sort_by(|a, b| a.1.0.len().cmp(&b.1.0.len()).then(a.1.0.cmp(&b.1.0)));
    for (name, (c, n)) in legend {
        out.push_str(&format!("  {c:<2} {name} ({n})\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use record::EventRecord;
    use world::World;

    /// A recording made the way the bot makes one: `ticks` tick events from `start`, each with
    /// its effects, and a checkpoint every `every` events.
    fn recording(start: &World, seeded: bool, ticks: u64, every: u64) -> Vec<Record> {
        let mut folder = Folder::new();
        folder.world = start.clone();
        let mut records = Vec::new();
        if seeded {
            records.push(Record::Checkpoint {
                event_index: 0,
                server_ticks: 0,
                world: Box::new(start.clone()),
            });
        }
        for n in 0..ticks {
            let e = EventRecord::Tick {
                now_ms: 50 * n,
                seed: n + 1,
            };
            let effects = folder.step(&e).unwrap();
            records.push(Record::Event(e));
            records.push(Record::Effects(effects));
            if (n + 1) % every == 0 {
                records.push(Record::Checkpoint {
                    event_index: folder.events,
                    server_ticks: folder.world.session.server_ticks,
                    world: Box::new(folder.world.clone()),
                });
            }
        }
        records
    }

    /// A world that remembers one block, as a snapshot's memory does.
    fn remembering() -> World {
        let mut w = World::new();
        let seen = memory::Seen::check("minecraft:the_nether", (61, 33, 95), 1, [Some(0); 6])
            .expect("a block beside air is seen");
        assert!(w.memory.record(seen, 7));
        w
    }

    #[test]
    fn a_clean_recording_verifies_in_linear_work() {
        let records = recording(&World::new(), false, 1000, 50);
        let r = verify(&records).unwrap();
        assert_eq!(r.mismatches(), 0, "{:?}", r.lines);
        assert_eq!(r.full.events, 1000);
        assert_eq!(r.full.checkpoints, 20);
        // The middle checkpoint and the last: never one fold a checkpoint (which here would be
        // 1000 + 950 + 900 + ... = 10,500 events).
        assert_eq!(r.refolds.len(), 2);
        assert!(r.events_stepped() <= 2 * r.full.events);
        assert!(!r.start_taken && !r.memory_only);
    }

    #[test]
    fn a_resumed_run_with_its_start_recorded_verifies() {
        let records = recording(&remembering(), true, 200, 50);
        let r = verify(&records).unwrap();
        assert!(r.start_taken);
        assert!(!r.memory_only);
        assert_eq!(r.mismatches(), 0, "{:?}", r.lines);
        assert_eq!(r.full.checkpoints, 4);
    }

    /// Races before this fix: the seeded memory was not in the recording, and every checkpoint
    /// of every resumed run was counted a mismatch.
    #[test]
    fn an_old_resumed_run_is_not_a_mismatch() {
        let records = recording(&remembering(), false, 200, 50);
        let r = verify(&records).unwrap();
        assert!(r.memory_only);
        assert_eq!(r.mismatches(), 0, "{:?}", r.lines);
    }

    #[test]
    fn a_wrong_checkpoint_is_counted_once() {
        let mut records = recording(&World::new(), false, 200, 50);
        // The last checkpoint's world is made wrong in more than its memory.
        let last = records
            .iter_mut()
            .filter_map(|r| match r {
                Record::Checkpoint { world, .. } => Some(world),
                _ => None,
            })
            .last()
            .unwrap();
        last.ticks += 1;
        let r = verify(&records).unwrap();
        assert_eq!(r.full.mismatches, 1, "{:?}", r.lines);
        assert!(!r.memory_only);
    }

    #[test]
    fn wrong_effects_are_mismatches() {
        let mut records = recording(&World::new(), false, 10, 5);
        let first = records
            .iter_mut()
            .find_map(|r| match r {
                Record::Effects(e) => Some(e),
                _ => None,
            })
            .unwrap();
        first.push(EffectRecord::SetCompression(1));
        let r = verify(&records).unwrap();
        assert_eq!(r.full.mismatches, 1);
        assert_eq!(r.lines.len(), 1);
    }

    /// The streamed verify finds what the whole one finds, a window at a time.
    #[test]
    fn the_streamed_verify_agrees_with_the_whole_one() {
        let stream = |records: Vec<Record>| {
            let mut it = records.into_iter();
            verify_stream(|| Ok(it.next())).unwrap()
        };
        let clean = recording(&World::new(), false, 230, 50);
        let (r, read) = stream(clean.clone());
        assert_eq!(read, clean.len() as u64);
        assert_eq!(r.full, verify(&clean).unwrap().full);
        assert_eq!(r.mismatches(), 0, "{:?}", r.lines);
        // One fold again, from the last checkpoint (event 200), over the 30 events after it.
        assert_eq!(r.refolds.len(), 1);
        assert_eq!(r.refolds[0].event_index, 200);
        assert_eq!(r.refolds[0].tally.events, 30);

        let (seeded, _) = stream(recording(&remembering(), true, 200, 50));
        assert!(seeded.start_taken);
        assert_eq!(seeded.mismatches(), 0, "{:?}", seeded.lines);
        let (old, _) = stream(recording(&remembering(), false, 200, 50));
        assert!(old.memory_only);
        assert_eq!(old.mismatches(), 0, "{:?}", old.lines);

        let mut wrong = recording(&World::new(), false, 200, 50);
        if let Some(Record::Effects(e)) = wrong.iter_mut().find(|r| matches!(r, Record::Effects(_)))
        {
            e.push(EffectRecord::SetCompression(1));
        }
        assert_eq!(stream(wrong).0.full.mismatches, 1);
    }

    #[test]
    fn a_moment_is_folded_to_from_the_checkpoint_before_it() {
        let records = recording(&World::new(), false, 200, 50);
        let (at_event, _) = fold_to(&records, At::Event(120)).unwrap();
        assert_eq!(at_event.events, 120);
        let mut whole = Folder::new();
        for r in &records {
            if whole.events == 120 {
                break;
            }
            if let Record::Event(e) = r {
                whole.step(e).unwrap();
            }
        }
        assert!(at_event.world == whole.world);
    }

    #[test]
    fn a_slice_names_every_block_in_full() {
        let name_at = |[x, y, _z]: [i32; 3]| match (x, y) {
            (_, 31) => Some("soul_sand".to_owned()),
            (5, _) => None,
            (4, 32) => Some("lava".to_owned()),
            _ => Some("air".to_owned()),
        };
        let text = slice(&name_at, [3, 32, 0], 2, (31, 32), &[[2, 32, 0]]);
        assert!(text.contains("y 32\n"), "{text}");
        // Row z 0 of y 32: x 1 air, the bot at x 2, the centre at x 3, lava at x 4, x 5 unloaded.
        assert!(text.contains("  z     0  . @. *.  a  ?\n"), "{text}");
        assert!(text.contains("  a  lava (5)\n"), "{text}");
        assert!(text.contains("  b  soul_sand (25)\n"), "{text}");
        assert!(text.contains("  ?  not loaded (5)\n"), "{text}");
        assert_eq!(code(0), "a");
        assert_eq!(code(51), "Z");
        assert_eq!(code(52), "aa");
    }

    #[test]
    fn cells_parse() {
        assert_eq!(parse_cell("61, 33,95"), Ok([61, 33, 95]));
        assert!(parse_cell("61,33").is_err());
    }
}
