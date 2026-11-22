//! why: fold a recording to a moment and print what the bot knew there and what its planner had
//! decided, with why (the planner's events, recorded as notes; brain/ruststeve/design/replay-and-why.md).
//!
//! ```text
//! cargo run -p why -- <file.rsr> --at <server ticks> [--force]
//! cargo run -p why -- <file.rsr> --event <n> [--force]
//! cargo run -p why -- <file.rsr> --event <n> --blocks <x,y,z> [--force]   # and the blocks round a cell
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
    let mut blocks: Option<[i32; 3]> = None;
    let mut it = args.iter().filter(|a| a.as_str() != "--force");
    while let Some(a) = it.next() {
        match a.as_str() {
            "--blocks" => {
                let v: Vec<i32> = it
                    .next()
                    .ok_or("--blocks needs x,y,z")?
                    .split(',')
                    .map(|s| s.trim().parse().map_err(|e| format!("--blocks: {e}")))
                    .collect::<Result<_, _>>()?;
                let [x, y, z] = v[..] else {
                    return Err("--blocks needs x,y,z".to_owned());
                };
                blocks = Some([x, y, z]);
            }
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
    // The planner's notes up to the moment: from the start, since a checkpoint holds the world,
    // not the notes.
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
    print_world(&folder.world, folder.events);
    if let Some(c) = blocks {
        print_blocks(&folder.world, c);
    }
    if notes.is_empty() {
        println!("planner: no decisions recorded (a run with no aim, or none made yet)");
    } else {
        println!(
            "planner, the last {} of {} decisions:",
            notes.len().min(8),
            notes.len()
        );
        for n in notes.iter().rev().take(8).rev() {
            println!("  {n}");
        }
    }
    Ok(())
}

/// The blocks round cell `c`, 7 by 7 on the plane, from 3 over it to 2 under, as the bot knew
/// them: the first six letters of each name, `?` for an unloaded cell. Rows run north to south (z
/// up), columns west to east (x up); the cell itself is marked with `*`.
fn print_blocks(w: &World, [cx, cy, cz]: [i32; 3]) {
    println!(
        "blocks round ({cx}, {cy}, {cz}), x {} to {} across:",
        cx - 3,
        cx + 3
    );
    for y in (cy - 2..=cy + 3).rev() {
        println!("y {y}");
        for z in cz - 3..=cz + 3 {
            let row: Vec<String> = (cx - 3..=cx + 3)
                .map(|x| {
                    let name = w
                        .block_at(BlockPos::new(x, y, z))
                        .and_then(data::BlockKind::name)
                        .unwrap_or("?");
                    let short: String = name.chars().take(6).collect();
                    let mark = if (x, y, z) == (cx, cy, cz) { '*' } else { ' ' };
                    format!("{mark}{short:<6}")
                })
                .collect();
            println!("  z {z:>5} {}", row.join(" "));
        }
    }
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
        match w.inventory.uses_left(*slot) {
            Some(u) => println!("  slot {slot}: {item:?} x{n}, {u} uses left"),
            None => println!("  slot {slot}: {item:?} x{n}"),
        }
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
