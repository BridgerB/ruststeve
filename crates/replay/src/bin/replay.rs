//! replay: fold a recording at its own commit (brain/ruststeve/design/replay-and-why.md).
//!
//! ```text
//! cargo run -p replay -- verify <file.rsr> [--force]
//! cargo run -p replay -- export <file.rsr>
//! ```
//!
//! `verify` folds every event from the start and compares each step's effects, byte for byte,
//! with the recorded ones, and the world at each checkpoint with the recorded world; then it folds
//! again from the last checkpoint (`replay::verify_stream`: linear, a record at a time). Any
//! difference fails it. A recording from another
//! commit is refused unless `--force`: a fold with newer code drifts (the handoff's lesson 4.5).

use std::path::Path;
use std::process::ExitCode;

use record::{COMMIT, EffectRecord, Record, Recording, check_commit};

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

fn verify(path: &Path, force: bool) -> Result<(), String> {
    // Read a record at a time: a long recording's checkpoints are never all in memory.
    let mut recording = Recording::open(path)?;
    let header = match recording.next_record()? {
        Some(Record::Header(h)) => h,
        Some(other) => return Err(format!("the first record is not a header: {other:?}")),
        None => return Err("an empty recording".to_owned()),
    };
    if header.format_version != record::FORMAT_VERSION {
        return Err(format!(
            "recording format {} (this build reads {})",
            header.format_version,
            record::FORMAT_VERSION
        ));
    }
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
        "replay: {} ({} {} as {}, commit {})",
        path.display(),
        header.minecraft_version,
        header.server,
        header.bot_name,
        header.commit
    );
    let from = std::time::Instant::now();
    let (report, records) = replay::verify_stream(|| recording.next_record())?;
    println!("{records} records");
    for line in &report.lines {
        eprintln!("{line}");
    }
    let full = &report.full;
    println!(
        "from the start: {} events, {} effects, {} checkpoints, {} mismatches",
        full.events, full.effects, full.checkpoints, full.mismatches
    );
    if report.start_taken {
        println!("the start state is recorded (a run resumed from a snapshot)");
    }
    if report.memory_only {
        println!(
            "the first checkpoint differs from the fold in its memory alone: a resumed run \
             recorded before the start state was kept; taken as the truth, not a mismatch"
        );
    }
    for r in &report.refolds {
        println!(
            "from checkpoint at event {} ({} ticks): {} events, {} mismatches",
            r.event_index, r.server_ticks, r.tally.events, r.tally.mismatches
        );
    }
    println!(
        "{} events stepped in {:.1} s",
        report.events_stepped(),
        from.elapsed().as_secs_f64()
    );
    let total = report.mismatches();
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
            Record::Event(record::EventRecord::Controls(c)) => {
                println!("event {event} controls {c:?}");
                event += 1;
            }
            Record::Event(record::EventRecord::Act(a)) => {
                println!("event {event} act {a:?}");
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
