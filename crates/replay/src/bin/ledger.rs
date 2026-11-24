//! ledger: rebuild a run's ledger from its recording (issue 16e, phase F). The recording holds
//! every packet's bytes, so folding it with this build's decoders gives today's world facts
//! (tool wear, sounds, mobs' signs, the cursor's stack) even for a run recorded before they were
//! decoded: an old race is read with new eyes. The planner's half is its recorded notes (a
//! recording holds no task records).
//!
//! The text is the race's own (`results::Ledger::render`): a run of the same note is one line
//! with its count, one second's changes to an item are one line. `--raw` prints one line a fact
//! and a note, as they come. `--senses <every>` adds the senses rows (the feet's cell, what it
//! stands on, lava and water against the body), `--story` the case's head and story.
//! `--packets` prints each damage event, hurt flash and health packet as it was received, with
//! the server tick and the client tick, for a fall in health the ledger cannot name.
//!
//! ```text
//! cargo run -p replay --bin ledger -- <file.rsr> [--force] [--from <ticks>] [--to <ticks>]
//!     [--raw] [--senses <every>] [--story] [--packets]
//! ```

use std::path::Path;
use std::process::ExitCode;

use protocol::S2c;
use record::{COMMIT, EventRecord, Folder, Record, Recording, check_commit};
use results::TaskRecord;

/// Client ticks between senses rows: the bot's own (`bot::SENSES_EVERY_TICKS`), one a second.
const SENSES_EVERY_TICKS: u64 = 20;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ledger: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Flags that take a value: the word after one is not the file.
const VALUED: [&str; 3] = ["--from", "--to", "--senses"];

fn arg(args: &[String], name: &str) -> Option<u64> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

/// A recorded note without its planner tick: `plan tick 812: what (why)` is `what (why)`.
fn note_text(n: &str) -> &str {
    n.strip_prefix("plan tick ")
        .and_then(|rest| rest.split_once(": "))
        .map_or(n, |(_, text)| text)
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |name: &str| args.iter().any(|a| a == name);
    let force = has("--force");
    let raw = has("--raw");
    let from = arg(&args, "--from").unwrap_or(0);
    let to = arg(&args, "--to").unwrap_or(u64::MAX);
    let senses_every = arg(&args, "--senses");
    let packets = has("--packets");
    let file = args
        .iter()
        .enumerate()
        .find(|(i, a)| {
            !a.starts_with("--")
                && args
                    .get(i.wrapping_sub(1))
                    .is_none_or(|p| !VALUED.contains(&p.as_str()))
        })
        .map(|(_, a)| a.clone())
        .ok_or(
            "usage: ledger <file.rsr> [--force] [--from <ticks>] [--to <ticks>] [--raw] [--senses <every>] [--story]",
        )?;
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
    let mut ticks = 0u64;
    let mut last_note = String::new();
    let mut ledger = results::Ledger {
        format_version: results::FORMAT_VERSION,
        run: "recording".to_owned(),
        bot: header.bot_name.clone(),
        commit: header.commit.clone(),
        tasks: Vec::new(),
        dropped: 0,
        facts: Vec::new(),
    };
    let mut rows = Vec::new();
    while let Some(rec) = r.next_record()? {
        match rec {
            Record::Event(e) => {
                let tick = matches!(e, EventRecord::Tick { .. });
                let at = folder.world.session.server_ticks;
                if packets
                    && at >= from
                    && at <= to
                    && let EventRecord::Packet { state, id, body } = &e
                    && let Ok(
                        p @ (S2c::DamageEvent { .. }
                        | S2c::HurtAnimation { .. }
                        | S2c::SetHealth { .. }),
                    ) = protocol::decode(*state, *id, body)
                {
                    println!(
                        "{at:>8} packet (client tick {ticks}, the bot is entity {:?}) {p:?}",
                        folder.world.player.entity_id
                    );
                }
                folder.step(&e)?;
                if tick {
                    ticks += 1;
                    let mut snap = observe::snapshot(&folder.world);
                    for f in observe::diff(&last, &mut snap, &folder.world) {
                        if f.at >= from && f.at <= to {
                            if raw {
                                println!("{:>8} {}", f.at, f.fact.line());
                            }
                            ledger.facts.push(f);
                        }
                        facts += 1;
                    }
                    last = snap;
                    let at = folder.world.session.server_ticks;
                    if ticks.is_multiple_of(SENSES_EVERY_TICKS) && at >= from && at <= to {
                        // The planner is not in a recording: the row's task is its last note.
                        rows.push(observe::senses(
                            &folder.world,
                            "last note",
                            &last_note,
                            None,
                        ));
                    }
                }
            }
            Record::Note(n) => {
                let at = folder.world.session.server_ticks;
                let text = note_text(&n);
                if at >= from && at <= to {
                    if raw {
                        println!("{at:>8} note {text}");
                    }
                    ledger.tasks.push(TaskRecord::Note {
                        at,
                        what: text.to_owned(),
                        why: String::new(),
                    });
                }
                text.clone_into(&mut last_note);
                notes += 1;
            }
            Record::Header(_) | Record::Effects(_) | Record::Checkpoint { .. } => {}
        }
        if folder.world.session.server_ticks > to {
            break;
        }
    }
    if !raw {
        print!("{}", ledger.render());
    }
    println!(
        "{facts} facts and {notes} notes to server tick {}",
        folder.world.session.server_ticks
    );
    if let Some(every) = senses_every {
        let senses = results::SensesFile {
            format_version: results::FORMAT_VERSION,
            run: "recording".to_owned(),
            bot: header.bot_name.clone(),
            rows: rows.clone(),
        };
        print!("{}", senses.render(usize::try_from(every).unwrap_or(1)));
    }
    if has("--story") {
        let story = results::story::story(&ledger, &rows);
        println!("where:");
        for l in &story.standing {
            println!("  {l}");
        }
        for l in story.render() {
            println!("{l}");
        }
    }
    Ok(())
}
