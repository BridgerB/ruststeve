//! ledger: rebuild a run's ledger from its recording (issue 16e, phase F). The recording holds
//! every packet's bytes, so folding it with this build's decoders gives today's world facts
//! (tool wear, sounds, mobs' signs) even for a run recorded before they were decoded: an old
//! race is read with new eyes. The planner's half is its recorded notes (a recording from
//! before the task ledger has no task records).
//!
//! ```text
//! cargo run -p replay --bin ledger -- <file.rsr> [--force] [--from <ticks>] [--to <ticks>]
//! ```

use std::path::Path;
use std::process::ExitCode;

use record::{COMMIT, EventRecord, Folder, Record, Recording, check_commit};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ledger: {e}");
            ExitCode::FAILURE
        }
    }
}

fn arg(args: &[String], name: &str) -> Option<u64> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let force = args.iter().any(|a| a == "--force");
    let from = arg(&args, "--from").unwrap_or(0);
    let to = arg(&args, "--to").unwrap_or(u64::MAX);
    let file = args
        .iter()
        .enumerate()
        .find(|(i, a)| {
            !a.starts_with("--")
                && args
                    .get(i.wrapping_sub(1))
                    .is_none_or(|p| p != "--from" && p != "--to")
        })
        .map(|(_, a)| a.clone())
        .ok_or("usage: ledger <file.rsr> [--force] [--from <ticks>] [--to <ticks>]")?;
    let mut r = Recording::open(Path::new(&file))?;
    let header = match r.next_record()? {
        Some(Record::Header(h)) => h,
        Some(_) | None => return Err("the recording has no header".to_owned()),
    };
    check_commit(&header, COMMIT, force)?;
    println!(
        "ledger of {} (recorded at {}, folded at {COMMIT})",
        header.bot_name, header.commit
    );
    let mut folder = Folder::new();
    let mut last = observe::snapshot(&folder.world);
    let mut facts = 0u64;
    let mut notes = 0u64;
    while let Some(rec) = r.next_record()? {
        match rec {
            Record::Event(e) => {
                let tick = matches!(e, EventRecord::Tick { .. });
                folder.step(&e)?;
                if tick {
                    let mut snap = observe::snapshot(&folder.world);
                    for f in observe::diff(&last, &mut snap, &folder.world) {
                        if f.at >= from && f.at <= to {
                            println!("{:>8} {}", f.at, f.fact.line());
                        }
                        facts += 1;
                    }
                    last = snap;
                }
            }
            Record::Note(n) => {
                let at = folder.world.session.server_ticks;
                if at >= from && at <= to {
                    println!("{at:>8} note {n}");
                }
                notes += 1;
            }
            Record::Header(_) | Record::Effects(_) | Record::Checkpoint { .. } => {}
        }
        if folder.world.session.server_ticks > to {
            break;
        }
    }
    println!(
        "{facts} facts and {notes} notes to server tick {}",
        folder.world.session.server_ticks
    );
    Ok(())
}
