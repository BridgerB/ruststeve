//! why: fold a recording to a moment and print what the bot knew there. It grows as the planner
//! does: today the world, later every planner branch's answer (brain/ruststeve/design/replay-and-why.md).
//!
//! ```text
//! cargo run -p why -- <file.rsr> --at <server ticks> [--force]
//! cargo run -p why -- <file.rsr> --event <n> [--force]
//! ```
//!
//! It starts from the last checkpoint before the moment, so a moment late in a long run is quick.

use std::path::Path;
use std::process::ExitCode;

use data::block_of_state;
use record::{COMMIT, Folder, Record, Recording, check_commit};
use world::{BlockPos, World};

/// Where to stop.
enum At {
    Ticks(u64),
    Event(u64),
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("why: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let force = args.iter().any(|a| a == "--force");
    let mut file = None;
    let mut at = None;
    let mut it = args.iter().filter(|a| a.as_str() != "--force");
    while let Some(a) = it.next() {
        match a.as_str() {
            "--at" => {
                at = Some(At::Ticks(
                    it.next()
                        .ok_or("--at needs ticks")?
                        .parse()
                        .map_err(|e| format!("--at: {e}"))?,
                ))
            }
            "--event" => {
                at = Some(At::Event(
                    it.next()
                        .ok_or("--event needs a number")?
                        .parse()
                        .map_err(|e| format!("--event: {e}"))?,
                ))
            }
            other => file = Some(other.to_owned()),
        }
    }
    let file = file.ok_or("usage: why <file> --at <ticks> | --event <n> [--force]")?;
    let at = at.ok_or("say --at <server ticks> or --event <n>")?;
    let (header, records) = Recording::read_all(Path::new(&file))?;
    check_commit(&header, COMMIT, force)?;

    let reached = |f: &Folder| match at {
        At::Ticks(t) => f.world.session.server_ticks >= t,
        At::Event(n) => f.events >= n,
    };
    // The last checkpoint at or before the moment.
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
    for rec in records.iter().skip(start) {
        if reached(&folder) {
            break;
        }
        if let Record::Event(e) = rec {
            folder.step(e)?;
        }
    }
    print_world(&folder.world, folder.events);
    println!("planner: none yet (issue 09 adds it; then every branch's answer prints here)");
    Ok(())
}

fn print_world(w: &World, events: u64) {
    println!(
        "after {events} events, {} server ticks",
        w.session.server_ticks
    );
    println!(
        "dimension {}",
        w.dimension.as_ref().map_or("-", |d| d.name.as_str())
    );
    let Some([x, y, z]) = w.player.pos else {
        println!("player: not placed yet");
        return;
    };
    println!(
        "player at ({x:.2}, {y:.2}, {z:.2}), health {:?}, food {:?}, standing on {:?}",
        w.player.health,
        w.player.food,
        w.block_under_player()
    );
    let mut near: Vec<(f64, &world::Entity)> = w
        .entities
        .values()
        .map(|e| {
            let d =
                ((e.pos[0] - x).powi(2) + (e.pos[1] - y).powi(2) + (e.pos[2] - z).powi(2)).sqrt();
            (d, e)
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    println!("{} entities; nearest:", w.entities.len());
    for (d, e) in near.iter().take(10) {
        println!("  {:?} at {d:.1} blocks, health {:?}", e.kind, e.health());
    }
    println!("inventory: {} stacks", w.inventory.slots.len());
    for (slot, (item, n)) in &w.inventory.slots {
        println!("  slot {slot}: {item:?} x{n}");
    }
    if let Some(dim) = w.dimension.as_ref().map(|d| d.name.as_str()) {
        let at = BlockPos::containing(x, y, z).chunk();
        let mut seen = Vec::new();
        for dx in -1..=1 {
            for dz in -1..=1 {
                seen.extend(w.memory.in_column(dim, at.x + dx, at.z + dz));
            }
        }
        println!(
            "{} sightings in the 3x3 columns around the player:",
            seen.len()
        );
        for s in seen.iter().take(10) {
            println!(
                "  {:?} at {:?} since tick {}",
                block_of_state(s.state),
                s.pos,
                s.at_tick
            );
        }
    }
}
