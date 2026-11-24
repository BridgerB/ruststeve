//! why: fold a recording to a moment and print what the bot knew there and what its planner had
//! decided, with why (the planner's events, recorded as notes; brain/ruststeve/design/replay-and-why.md).
//!
//! ```text
//! cargo run -p why -- <file.rsr> --at <server ticks> [--force]
//! cargo run -p why -- <file.rsr> --event <n> [--force]
//! cargo run -p why -- <file.rsr> --event <n> --blocks <x,y,z> [--force]   # and the blocks round a cell
//! cargo run -p why -- <file.rsr> --at <ticks> --blocks <x,y,z> --radius 15 --ys 29,36 [--force]
//! ```
//!
//! `--blocks` prints the cells round a cell as the bot knew them, 3 each way and from 2 under to
//! 3 over unless `--radius` (cells each way on the plane) and `--ys lo,hi` (heights) say more: a
//! 31 by 31 slice is `--radius 15`. Every block is named in full in the legend under the layers.
//!
//! It starts from the last checkpoint before the moment, so a moment late in a long run is quick.

use std::path::Path;
use std::process::ExitCode;

use data::block_of_state;
use record::{COMMIT, Recording, check_commit};
use replay::At;
use world::{BlockPos, World};

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
    let mut radius: i32 = 3;
    let mut ys: Option<(i32, i32)> = None;
    let mut it = args.iter().filter(|a| a.as_str() != "--force");
    while let Some(a) = it.next() {
        match a.as_str() {
            "--blocks" => {
                blocks = Some(replay::parse_cell(
                    it.next().ok_or("--blocks needs x,y,z")?,
                )?);
            }
            "--radius" => {
                radius = it
                    .next()
                    .ok_or("--radius needs a number")?
                    .parse()
                    .map_err(|e| format!("--radius: {e}"))?;
            }
            "--ys" => {
                let v = it.next().ok_or("--ys needs lo,hi")?;
                let (lo, hi) = v.split_once(',').ok_or("--ys needs lo,hi")?;
                let n = |s: &str| s.trim().parse::<i32>().map_err(|e| format!("--ys: {e}"));
                ys = Some((n(lo)?, n(hi)?));
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

    let (folder, notes) = replay::fold_to(&records, at)?;
    print_world(&folder.world, folder.events);
    if let Some(c) = blocks {
        let w = &folder.world;
        let name_at = |[x, y, z]: [i32; 3]| {
            w.block_at(BlockPos::new(x, y, z))
                .and_then(data::BlockKind::name)
                .map(str::to_owned)
        };
        let feet: Vec<[i32; 3]> = w
            .body
            .map(|b| b.pos)
            .or(w.player.pos)
            .map(|[x, y, z]| [x.floor() as i32, y.floor() as i32, z.floor() as i32])
            .into_iter()
            .collect();
        let ys = ys.unwrap_or((c[1] - 2, c[1] + 3));
        print!("{}", replay::slice(&name_at, c, radius, ys, &feet));
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
