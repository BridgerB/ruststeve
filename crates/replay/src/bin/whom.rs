//! whom: endermen angry at whom, in the End (issue 80, `docs/problems/pumpkin.md`), judged on a
//! recording as the planner judges them (`plan::endermen::whom_fold` and `judge`, the same pure
//! functions, every server tick, every angry enderman within `APPROACH_FROM`). For each enderman's
//! first anger it prints the first verdict ("at the bot: <reason>" or "on the dragon: <its last
//! hurt>") and every hit it then dealt the bot (the player's damage events whose cause or direct
//! source is that enderman), with the verdict it held the tick before. The rule is wrong if any
//! enderman held "on the dragon" when it hit the bot.
//!
//! ```text
//! cargo run -p replay --bin whom -- <file.rsr>... [--force]
//! ```

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use plan::endermen::{WhomRules, WhomTrack, Whose, creepy, judge, nearest_angry, whom_fold};
use protocol::S2c;
use record::{COMMIT, EventRecord, Folder, Record, Recording, check_commit};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("whom: {e}");
            ExitCode::FAILURE
        }
    }
}

fn verdict(v: Whose) -> String {
    match v {
        Whose::Bots(why) => format!("at the bot: {}", why.name()),
        Whose::Dragons { at, by } => format!("on the dragon: hurt at {at} by entity {by:?}"),
    }
}

/// One enderman's story in the recording.
#[derive(Default)]
struct Story {
    first: Option<(u64, String, bool)>,
    /// The verdict held at the last tick judged.
    last: Option<Whose>,
    /// Hits on the bot: the tick and the verdict held before it.
    hits: Vec<(u64, Option<Whose>)>,
}

/// The totals over every file.
#[derive(Default)]
struct Totals {
    angers: u64,
    first_bots: BTreeMap<String, u64>,
    first_dragon: u64,
    first_dragon_hit: u64,
    dragon_at_hit: u64,
    hits: u64,
}

fn one(file: &str, force: bool, rules: &WhomRules, t: &mut Totals) -> Result<(), String> {
    let mut r = Recording::open(Path::new(file))?;
    let header = match r.next_record()? {
        Some(Record::Header(h)) => h,
        Some(_) | None => return Err(format!("{file}: no header")),
    };
    check_commit(&header, COMMIT, force)?;
    println!(
        "== {file} ({}, recorded at {})",
        header.bot_name, header.commit
    );
    let mut folder = Folder::new();
    let mut tracks: Vec<WhomTrack> = Vec::new();
    let mut stories: BTreeMap<i32, Story> = BTreeMap::new();
    let mut judged_at: Option<u64> = None;
    while let Some(rec) = r.next_record()? {
        match rec {
            Record::Event(e) => {
                // A hit on the bot by an enderman, from the packet itself (the world keeps the
                // cause's kind only).
                if let EventRecord::Packet { state, id, body } = &e
                    && let Ok(S2c::DamageEvent {
                        entity_id,
                        cause,
                        direct,
                        ..
                    }) = protocol::decode(*state, *id, body)
                    && Some(entity_id) == folder.world.player.entity_id
                {
                    let now = folder.world.session.server_ticks;
                    for who in [cause, direct].into_iter().flatten() {
                        if let Some(s) = stories.get_mut(&who) {
                            if s.hits.last().is_some_and(|h| h.0 == now) {
                                continue;
                            }
                            s.hits.push((now, s.last));
                        }
                    }
                }
                let tick = matches!(e, EventRecord::Tick { .. });
                folder.step(&e)?;
                let w = &folder.world;
                let now = w.session.server_ticks;
                if !tick || !plan::end::in_end(w) || judged_at == Some(now) {
                    continue;
                }
                judged_at = Some(now);
                let Some(body) = w.body else {
                    continue;
                };
                let _ = whom_fold(&mut tracks, w, rules);
                let melee = nearest_angry(w, body.pos, rules.melee);
                let ids: Vec<i32> = tracks.iter().map(|x| x.id).collect();
                for id in ids {
                    let v = judge(&mut tracks, w, id, &[], melee);
                    let s = stories.entry(id).or_default();
                    if s.first.is_none() {
                        let dragon = matches!(v, Whose::Dragons { .. });
                        s.first = Some((now, verdict(v), dragon));
                    }
                    s.last = Some(v);
                }
                // A calm one's verdict is not held into its next anger.
                for (id, s) in &mut stories {
                    let angry = w.entities.get(id).is_some_and(|e| !e.dead && creepy(e));
                    if !angry {
                        s.last = None;
                    }
                }
            }
            Record::Checkpoint {
                event_index, world, ..
            } => {
                folder.take_start(event_index, &world);
            }
            Record::Header(_) | Record::Note(_) | Record::Effects(_) => {}
        }
    }
    for (id, s) in &stories {
        let Some((at, v, dragon)) = &s.first else {
            continue;
        };
        t.angers += 1;
        t.hits += s.hits.len() as u64;
        if *dragon {
            t.first_dragon += 1;
            if !s.hits.is_empty() {
                t.first_dragon_hit += 1;
            }
        } else {
            *t.first_bots.entry(v.clone()).or_default() += 1;
        }
        let bad = s
            .hits
            .iter()
            .filter(|h| matches!(h.1, Some(Whose::Dragons { .. })))
            .count();
        t.dragon_at_hit += bad as u64;
        let hits: Vec<String> = s
            .hits
            .iter()
            .map(|(h, v)| format!("{h} ({})", v.map_or_else(|| "unjudged".to_owned(), verdict)))
            .collect();
        println!(
            "{at:>8} enderman#{id} first {v}; hit the bot {} times{}{}",
            s.hits.len(),
            if hits.is_empty() { "" } else { ": " },
            hits.join(", ")
        );
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let force = args.iter().any(|a| a == "--force");
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if files.is_empty() {
        return Err("usage: whom <file.rsr>... [--force]".to_owned());
    }
    let rules = WhomRules::of(&plan::end_portal::EndKitRules::default());
    let mut t = Totals::default();
    for f in files {
        if let Err(e) = one(f, force, &rules, &mut t) {
            println!("== {f}: {e}");
        }
    }
    println!(
        "first angers {}: on the dragon {} ({} of them later hit the bot); at the bot {}",
        t.angers,
        t.first_dragon,
        t.first_dragon_hit,
        t.first_bots.values().sum::<u64>()
    );
    for (why, n) in &t.first_bots {
        println!("  {why}: {n}");
    }
    println!(
        "hits on the bot by endermen {}; held \"on the dragon\" at the hit: {} (must be 0)",
        t.hits, t.dragon_at_hit
    );
    Ok(())
}
