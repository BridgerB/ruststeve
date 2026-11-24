//! survey: the blocks a recorded bot had on record at a moment, wider than `why --blocks`: level
//! slabs of a box (one letter a cell) and an upright section along a line between two columns.
//! For reading terrain when a wall is written (docs/problems/nether-tunnel.md); it never runs in
//! the bot.
//!
//! ```text
//! survey <file.rsr> --at <server ticks> --slab x0,z0,x1,z1 --y y0,y1 [--force]
//! survey <file.rsr> --at <server ticks> --line x0,z0,x1,z1 --y y0,y1 [--force]
//! survey <file.rsr> --at <server ticks> --route x,y,z,tx,ty,tz [--force]
//! ```
//!
//! `--route` asks the travel tunnel's search (`plan::tunnel::route`, a pure function) for its way
//! from a feet cell to a target on that world, and prints it with the time it took.
//!
//! Letters: `#` netherrack, `.` air, `L` lava, `m` magma, `s` soul sand or soil, `g` gravel,
//! `B` nether bricks, `o` any other full block, `,` any other block, `?` not loaded.

use std::path::Path;
use std::process::ExitCode;

use data::BlockKind;
use record::{COMMIT, Folder, Record, Recording, check_commit};
use world::{BlockPos, World};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("survey: {e}");
            ExitCode::FAILURE
        }
    }
}

fn numbers(s: Option<&String>, n: usize, what: &str) -> Result<Vec<i32>, String> {
    let v: Vec<i32> = s
        .ok_or(format!("{what} needs {n} numbers"))?
        .split(',')
        .map(|p| p.trim().parse().map_err(|e| format!("{what}: {e}")))
        .collect::<Result<_, _>>()?;
    if v.len() == n {
        Ok(v)
    } else {
        Err(format!("{what} needs {n} numbers"))
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let force = args.iter().any(|a| a == "--force");
    let (mut file, mut at) = (None, None);
    let (mut slab, mut line, mut ys) = (None, None, vec![28, 60]);
    let mut route = None;
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
            "--slab" => slab = Some(numbers(it.next(), 4, "--slab")?),
            "--line" => line = Some(numbers(it.next(), 4, "--line")?),
            "--route" => route = Some(numbers(it.next(), 6, "--route")?),
            "--y" => ys = numbers(it.next(), 2, "--y")?,
            other => file = Some(other.to_owned()),
        }
    }
    let file =
        file.ok_or("usage: survey <file> --at <ticks> --slab|--line x0,z0,x1,z1 --y y0,y1")?;
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
        "at {} server ticks, dimension {}, player {:?}",
        w.session.server_ticks,
        w.dimension.as_ref().map_or("-", |d| d.name.as_str()),
        w.player.pos
    );
    if let Some(s) = slab {
        slabs(w, [s[0], s[1], s[2], s[3]], [ys[0], ys[1]]);
    }
    if let Some(l) = line {
        section(w, [l[0], l[1], l[2], l[3]], [ys[0], ys[1]]);
    }
    if let Some(r) = route {
        tunnel(w, (r[0], r[1], r[2]), (r[3], r[4], r[5]));
    }
    Ok(())
}

fn letter(w: &World, x: i32, y: i32, z: i32) -> char {
    let Some(k) = w.block_at(BlockPos::new(x, y, z)) else {
        return '?';
    };
    match k {
        BlockKind::Netherrack => '#',
        BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir => '.',
        BlockKind::Lava => 'L',
        BlockKind::MagmaBlock => 'm',
        BlockKind::SoulSand | BlockKind::SoulSoil => 's',
        BlockKind::Gravel => 'g',
        BlockKind::NetherBricks => 'B',
        other => {
            if data::generated::shapes::collision(other) == Some(data::types::Collision::Full) {
                'o'
            } else {
                ','
            }
        }
    }
}

/// The travel tunnel's route from `from` to `target`, a cell a line with the step it takes.
fn tunnel(w: &World, from: (i32, i32, i32), target: (i32, i32, i32)) {
    let began = std::time::Instant::now();
    let found = plan::tunnel::route(
        w,
        from,
        target,
        &std::collections::BTreeSet::new(),
        plan::tunnel::MAX_NODES,
    );
    let took = began.elapsed();
    let Some(r) = found else {
        println!("route from {from:?} to {target:?}: none, in {took:?}");
        return;
    };
    println!(
        "route from {from:?} to {target:?}: {} steps, {}, {} nodes, in {took:?}",
        r.cells.len().saturating_sub(1),
        if r.found {
            "within reach"
        } else {
            "ends short"
        },
        r.expanded
    );
    for pair in r.cells.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let refused = plan::tunnel::opened(a, b)
            .and_then(|(cells, floor)| plan::stairs::refusal(w, &cells, floor));
        println!(
            "  {b:?} {}{}",
            match b.1 - a.1 {
                1 => "up",
                -1 => "down",
                _ => "level",
            },
            refused.map_or_else(String::new, |why| format!("  REFUSED on this world: {why}"))
        );
    }
}

/// Level slabs, the highest first; rows run north to south (z up), columns west to east (x up).
fn slabs(w: &World, [x0, z0, x1, z1]: [i32; 4], [y0, y1]: [i32; 2]) {
    for y in (y0..=y1).rev() {
        println!("y {y}, x {x0} to {x1} across, z {z0} to {z1} down");
        for z in z0..=z1 {
            let row: String = (x0..=x1).map(|x| letter(w, x, y, z)).collect();
            println!("{z:>5} {row}");
        }
    }
}

/// An upright section along the line from (x0, z0) to (x1, z1), a column a step, the highest row
/// first, and under it each column's x and z.
fn section(w: &World, [x0, z0, x1, z1]: [i32; 4], [y0, y1]: [i32; 2]) {
    let n = (x1 - x0).abs().max((z1 - z0).abs()).max(1);
    let cols: Vec<(i32, i32)> = (0..=n)
        .map(|i| {
            let t = f64::from(i) / f64::from(n);
            (
                (f64::from(x0) + t * f64::from(x1 - x0)).round() as i32,
                (f64::from(z0) + t * f64::from(z1 - z0)).round() as i32,
            )
        })
        .collect();
    println!(
        "section from ({x0}, {z0}) to ({x1}, {z1}), {} columns",
        cols.len()
    );
    for y in (y0..=y1).rev() {
        let row: String = cols.iter().map(|(x, z)| letter(w, *x, y, *z)).collect();
        println!("{y:>5} {row}");
    }
    for (i, (x, z)) in cols.iter().enumerate() {
        if i % 10 == 0 {
            println!("column {i}: ({x}, {z})");
        }
    }
}
