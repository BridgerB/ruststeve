//! replay: fold a recording at its own commit (brain/ruststeve/design/replay-and-why.md).
//!
//! ```text
//! cargo run -p replay -- verify <file.rsr> [--force]
//! cargo run -p replay -- export <file.rsr>
//! ```
//!
//! `verify` folds every event from the start and compares each step's effects, byte for byte,
//! with the recorded ones, and the world at each checkpoint with the recorded world; then it folds
//! again from every checkpoint to the end. Any difference fails it. A recording from another
//! commit is refused unless `--force`: a fold with newer code drifts (the handoff's lesson 4.5).

use std::path::Path;
use std::process::ExitCode;

use record::{COMMIT, EffectRecord, Folder, Record, Recording, check_commit};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let force = args.iter().any(|a| a == "--force");
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let result = match positional.as_slice() {
        [task, file] if task.as_str() == "verify" => verify(Path::new(file), force),
        [task, file] if task.as_str() == "export" => export(Path::new(file)),
        _ => Err("usage: replay verify <file> [--force] | replay export <file>".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("replay: {e}");
            ExitCode::FAILURE
        }
    }
}

/// What one fold found.
#[derive(Default)]
struct Tally {
    events: u64,
    effects: u64,
    checkpoints: u64,
    mismatches: u64,
}

/// Fold `records[start..]` from `folder`, comparing as it goes. Prints the first few mismatches.
fn fold(
    mut folder: Folder,
    records: &[Record],
    start: usize,
    label: &str,
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
                            if t.mismatches <= 3 {
                                eprintln!(
                                    "{label}: event {} ({e:?}) at {} server ticks: produced {}, recorded {}",
                                    folder.events - 1,
                                    folder.world.session.server_ticks,
                                    show(&produced),
                                    show(recorded)
                                );
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
                t.checkpoints += 1;
                if folder.events != *event_index || folder.world != **world {
                    t.mismatches += 1;
                    if t.mismatches <= 3 {
                        eprintln!(
                            "{label}: the world folded to event {} differs from checkpoint at event {event_index}",
                            folder.events
                        );
                    }
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

fn verify(path: &Path, force: bool) -> Result<(), String> {
    let (header, records) = Recording::read_all(path)?;
    check_commit(&header, COMMIT, force)?;
    if header.dirty {
        eprintln!("replay: warning: the recording was made from a tree with uncommitted changes");
    }
    if header.commit != COMMIT {
        eprintln!(
            "replay: --force: folding a {} recording with {COMMIT}",
            header.commit
        );
    }
    println!(
        "replay: {} ({} {} as {}, commit {}), {} records",
        path.display(),
        header.minecraft_version,
        header.server,
        header.bot_name,
        header.commit,
        records.len()
    );
    let full = fold(Folder::new(), &records, 0, "from the start")?;
    println!(
        "from the start: {} events, {} effects, {} checkpoints, {} mismatches",
        full.events, full.effects, full.checkpoints, full.mismatches
    );
    let mut total = full.mismatches;
    for (i, rec) in records.iter().enumerate() {
        if let Record::Checkpoint {
            event_index,
            server_ticks,
            world,
        } = rec
        {
            let label = format!("from checkpoint at event {event_index} ({server_ticks} ticks)");
            let t = fold(
                Folder::from_checkpoint((**world).clone(), *event_index),
                &records,
                i + 1,
                &label,
            )?;
            println!("{label}: {} events, {} mismatches", t.events, t.mismatches);
            total += t.mismatches;
        }
    }
    if total == 0 {
        println!("replay: verified, 0 mismatches");
        Ok(())
    } else {
        Err(format!("{total} mismatches"))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn export(path: &Path) -> Result<(), String> {
    let (header, records) = Recording::read_all(path)?;
    println!("header {header:?}");
    let mut event = 0u64;
    for rec in &records {
        match rec {
            Record::Event(record::EventRecord::Packet { state, id, body }) => {
                println!(
                    "event {event} packet {state:?} id {id} {} bytes {}",
                    body.len(),
                    hex(body)
                );
                event += 1;
            }
            Record::Event(record::EventRecord::Tick { now_ms, seed }) => {
                println!("event {event} tick now_ms {now_ms} seed {seed}");
                event += 1;
            }
            Record::Effects(e) => {
                let list: Vec<String> = e
                    .iter()
                    .map(|x| match x {
                        EffectRecord::Send { id, body } => format!("send id {id} {}", hex(body)),
                        EffectRecord::SetCompression(t) => format!("compression {t}"),
                    })
                    .collect();
                println!("effects [{}]", list.join(", "));
            }
            Record::Checkpoint {
                event_index,
                server_ticks,
                world,
            } => println!(
                "checkpoint at event {event_index}, {server_ticks} server ticks: {} columns, {} entities, {} sightings",
                world.columns.len(),
                world.entities.len(),
                world.memory.log().len()
            ),
            Record::Note(n) => println!("note {n}"),
            Record::Header(h) => println!("header {h:?}"),
        }
    }
    Ok(())
}
