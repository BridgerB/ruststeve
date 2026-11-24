//! navmap: fold a recording to a moment, then print the terrain the bot knew as layer maps and
//! the route today's pathfinder plans over it (docs/problems/nether-walk.md). A probe's tool: the
//! recorded world is the fixture, `plan::path::plan` the pure function under test.
//!
//! ```text
//! navmap <file.rsr> --at <server ticks> [--force]
//!     [--map x0,z0,x1,z1 --y y0,y1]       # one map a layer, a letter a cell
//!     [--route gx,gy,gz,range [--from x,y,z] [--nodes n]]
//! ```

use std::path::Path;
use std::process::ExitCode;

use data::BlockKind;
use plan::path::{self, Class, Goal};
use record::{COMMIT, Folder, Record, Recording, check_commit};
use world::{BlockPos, World};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("navmap: {e}");
            ExitCode::FAILURE
        }
    }
}

fn numbers(s: Option<&String>, what: &str) -> Result<Vec<i32>, String> {
    s.ok_or(format!("{what} needs numbers"))?
        .split(',')
        .map(|v| v.trim().parse().map_err(|e| format!("{what}: {e}")))
        .collect()
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let force = args.iter().any(|a| a == "--force");
    let (mut file, mut at) = (None, None);
    let (mut map, mut ys, mut route, mut from, mut nodes) = (None, vec![30, 36], None, None, 6000);
    let mut it = args.iter().filter(|a| a.as_str() != "--force");
    while let Some(a) = it.next() {
        match a.as_str() {
            "--at" => {
                at = Some(
                    it.next()
                        .ok_or("--at needs ticks")?
                        .parse::<u64>()
                        .map_err(|e| format!("--at: {e}"))?,
                );
            }
            "--map" => map = Some(numbers(it.next(), "--map")?),
            "--y" => ys = numbers(it.next(), "--y")?,
            "--route" => route = Some(numbers(it.next(), "--route")?),
            "--from" => from = Some(numbers(it.next(), "--from")?),
            "--nodes" => {
                nodes = it
                    .next()
                    .ok_or("--nodes needs a count")?
                    .parse()
                    .map_err(|e| format!("--nodes: {e}"))?;
            }
            other => file = Some(other.to_owned()),
        }
    }
    let file = file.ok_or("usage: navmap <file> --at <ticks> [--map ...] [--route ...]")?;
    let at = at.ok_or("say --at <server ticks>")?;
    let (header, records) = Recording::read_all(Path::new(&file))?;
    check_commit(&header, COMMIT, force)?;
    let mut folder = Folder::new();
    let mut start = 0;
    for (i, rec) in records.iter().enumerate() {
        if let Record::Checkpoint {
            event_index,
            server_ticks,
            world,
        } = rec
            && *server_ticks <= at
        {
            folder = Folder::from_checkpoint((**world).clone(), *event_index);
            start = i + 1;
        }
    }
    for rec in records.iter().skip(start) {
        if folder.world.session.server_ticks >= at {
            break;
        }
        if let Record::Event(e) = rec {
            folder.step(e).map(|_| ())?;
        }
    }
    let w = &folder.world;
    println!(
        "tick {}, dimension {}, player {:?}, {} columns",
        w.session.server_ticks,
        w.dimension.as_ref().map_or("-", |d| d.name.as_str()),
        w.player.pos,
        w.columns.len()
    );
    let bricks = world::stage::bricks_near(w, 16);
    println!(
        "{} nether bricks seen; nearest {:?}",
        bricks.len(),
        bricks.iter().take(5).collect::<Vec<_>>()
    );
    if let Some(m) = map {
        let [x0, z0, x1, z1] = m[..] else {
            return Err("--map needs x0,z0,x1,z1".to_owned());
        };
        let [y0, y1] = ys[..] else {
            return Err("--y needs y0,y1".to_owned());
        };
        print_map(w, (x0, z0, x1, z1), (y0, y1));
    }
    if let Some(r) = route {
        let [gx, gy, gz, range] = r[..] else {
            return Err("--route needs gx,gy,gz,range".to_owned());
        };
        let start = match from.as_deref() {
            Some([x, y, z]) => (*x, *y, *z),
            Some(_) => return Err("--from needs x,y,z".to_owned()),
            None => w.player.pos.map(path::feet_cell).ok_or("no player")?,
        };
        let goal = Goal::Near {
            pos: (gx, gy, gz),
            range: f64::from(range),
        };
        let r = path::plan(w, start, &goal, nodes);
        let end = r.waypoints.last().copied().or(r.from);
        println!(
            "route from {start:?} toward ({gx}, {gy}, {gz}) within {range}: {:?}, {} waypoints, cost {:.1}, {} expanded, searched from {:?}, ends {:?} ({:.1} left)",
            r.status,
            r.waypoints.len(),
            r.cost,
            r.expanded,
            r.from,
            end,
            end.map_or(f64::NAN, |e| goal.heuristic(e)),
        );
        for chunk in r.waypoints.chunks(8) {
            println!(
                "  {}",
                chunk
                    .iter()
                    .map(|c| format!("{c:?}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
    }
    Ok(())
}

/// A letter a cell: `.` clear, `#` netherrack, `~` lava, `w` water, `s` soul sand, `o` soul soil,
/// `m` magma, `g` gravel, `B` nether bricks, `O` obsidian, `P` portal, `f` fire, `X` any other
/// full block, `a` any other awkward one, `?` not loaded.
fn letter(w: &World, x: i32, y: i32, z: i32) -> char {
    let pos = BlockPos::new(x, y, z);
    let Some(kind) = w.block_at(pos) else {
        return '?';
    };
    match kind {
        BlockKind::Netherrack => '#',
        BlockKind::Lava => '~',
        BlockKind::Water => 'w',
        BlockKind::SoulSand => 's',
        BlockKind::SoulSoil => 'o',
        BlockKind::MagmaBlock => 'm',
        BlockKind::Gravel => 'g',
        BlockKind::NetherBricks => 'B',
        BlockKind::Obsidian => 'O',
        BlockKind::NetherPortal => 'P',
        BlockKind::Fire | BlockKind::SoulFire => 'f',
        _ => match path::classify(w.state_at(pos)) {
            Class::Clear => '.',
            Class::Solid => 'X',
            Class::Awkward => 'a',
            Class::Water => 'w',
            Class::Lava => '~',
            Class::Unknown => '?',
        },
    }
}

/// One map a layer, top layer first: rows run north to south (z up), columns west to east (x
/// up), a ruler every ten cells; the player's column is marked `@` on its feet layer.
fn print_map(w: &World, (x0, z0, x1, z1): (i32, i32, i32, i32), (y0, y1): (i32, i32)) {
    let feet = w.player.pos.map(path::feet_cell);
    let ruler: String = (x0..=x1)
        .map(|x| if x.rem_euclid(10) == 0 { '|' } else { ' ' })
        .collect();
    for y in (y0..=y1).rev() {
        println!("y {y}, x {x0} to {x1} (| at each x of ten)");
        println!("       {ruler}");
        for z in z0..=z1 {
            let row: String = (x0..=x1)
                .map(|x| {
                    if feet == Some((x, y, z)) {
                        '@'
                    } else {
                        letter(w, x, y, z)
                    }
                })
                .collect();
            println!("{z:>6} {row}");
        }
    }
}
