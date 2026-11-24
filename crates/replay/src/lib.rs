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

/// Fold `records[start..]` from `folder`, comparing as it goes.
fn fold(
    mut folder: Folder,
    records: &[Record],
    start: usize,
    label: &str,
    report: &mut Report,
) -> Result<Tally, String> {
    let mut t = Tally::default();
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
    Ok(t)
}

/// Verify a recording's records (the header already read): one fold from the start, then a fold
/// from the middle checkpoint and one from the last.
pub fn verify(records: &[Record]) -> Result<Report, String> {
    let mut report = Report::default();
    let full = fold(Folder::new(), records, 0, "from the start", &mut report)?;
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
            let tally = fold(
                Folder::from_checkpoint((**world).clone(), *event_index),
                records,
                i + 1,
                &label,
                &mut report,
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
}
