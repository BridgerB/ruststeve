//! replan: ask the planner a question on a recorded world, offline (issue 16,
//! docs/problems/race-tooling.md). The recording is folded to a moment, as `why` does, and then
//! either one route is planned from the bot's feet or a fresh planner is stepped on that world.
//!
//! ```text
//! cargo run -p replay --bin replan -- <file.rsr> --at <ticks> --route <x,y,z> [--range 2] [--nodes 6000] [--force]
//! cargo run -p replay --bin replan -- <file.rsr> --at <ticks> --steps <n> [--goal race] [--goals config/goals.toml] [--force]
//! ```
//!
//! `--route` prints `plan::path::plan` from the feet to within `--range` of the cell (0 is the
//! cell itself): how it ended, what it spent, its waypoints, why the feet's own cell is or is not
//! a start, and, when it did not arrive, the moves the route's end has and the cells beside it
//! that no move enters, each with the reason. `--steps` runs `Plan::step` that many times with a
//! new planner (the recording does not hold the planner) and prints its notes: open loop, the
//! body moved by the client's own physics under the controls, the blocks as they were (no
//! server answers a dig or a click).
//!
//! A tool for reading a recording: it prints blocks the bot had loaded, seen or not. Nothing
//! here reaches the bot.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use plan::path::{self, Cells, Class, Goal, Status};
use record::{COMMIT, EventRecord, Record, Recording, check_commit};
use replay::At;
use world::{BlockPos, Event, World};

type Cell = (i32, i32, i32);

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("replan: {e}");
            ExitCode::FAILURE
        }
    }
}

struct Args {
    file: String,
    at: At,
    force: bool,
    route: Option<[i32; 3]>,
    range: f64,
    nodes: u32,
    steps: Option<u64>,
    goal: String,
    goals: String,
}

fn args() -> Result<Args, String> {
    let mut file = None;
    let mut at = None;
    let mut a = Args {
        file: String::new(),
        at: At::Event(0),
        force: false,
        route: None,
        range: 2.0,
        nodes: path::Budget::Far.expansions(),
        steps: None,
        goal: "race".to_owned(),
        goals: "config/goals.toml".to_owned(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match flag.as_str() {
            "--force" => a.force = true,
            "--at" => at = Some(At::Ticks(num(&value("--at")?)?)),
            "--event" => at = Some(At::Event(num(&value("--event")?)?)),
            "--route" => a.route = Some(replay::parse_cell(&value("--route")?)?),
            "--range" => {
                a.range = value("--range")?
                    .parse()
                    .map_err(|e| format!("--range: {e}"))?;
            }
            "--nodes" => {
                a.nodes = u32::try_from(num(&value("--nodes")?)?).map_err(|e| e.to_string())?;
            }
            "--steps" => a.steps = Some(num(&value("--steps")?)?),
            "--goal" => a.goal = value("--goal")?,
            "--goals" => a.goals = value("--goals")?,
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other => file = Some(other.to_owned()),
        }
    }
    a.file = file.ok_or(
        "usage: replan <file.rsr> --at <ticks> (--route <x,y,z> [--range r] [--nodes n] | --steps <n> [--goal race])",
    )?;
    a.at = at.ok_or("say --at <server ticks> or --event <n>")?;
    if a.route.is_none() && a.steps.is_none() {
        return Err("say --route <x,y,z> or --steps <n>".to_owned());
    }
    Ok(a)
}

fn num(s: &str) -> Result<u64, String> {
    s.parse().map_err(|e| format!("{s}: {e}"))
}

fn run() -> Result<(), String> {
    let a = args()?;
    let (header, records) = Recording::read_all(Path::new(&a.file))?;
    check_commit(&header, COMMIT, a.force)?;
    let (folder, _) = replay::fold_to(&records, a.at)?;
    // The clock of the last tick folded: the steps' ticks go on from it.
    let clock = records
        .iter()
        .filter_map(|r| match r {
            Record::Event(e) => Some(e),
            _ => None,
        })
        .take(usize::try_from(folder.events).unwrap_or(usize::MAX))
        .filter_map(|e| match e {
            EventRecord::Tick { now_ms, .. } => Some(*now_ms),
            _ => None,
        })
        .last()
        .unwrap_or(0);
    let mut world = folder.world;
    // A recording from before the memory's kind index: build it, so the steps run at today's
    // speed.
    world.memory.reindex();
    let pos = world
        .body
        .map(|b| b.pos)
        .or(world.player.pos)
        .ok_or("the bot is not in the world at that moment")?;
    println!(
        "{} at {} server ticks ({} events), in {}: the bot at ({:.2}, {:.2}, {:.2}), feet cell {:?}",
        header.bot_name,
        world.session.server_ticks,
        folder.events,
        world.dimension.as_ref().map_or("-", |d| d.name.as_str()),
        pos[0],
        pos[1],
        pos[2],
        path::feet_cell(pos)
    );
    if let Some(to) = a.route {
        route(&world, path::feet_cell(pos), (to[0], to[1], to[2]), &a);
    }
    if let Some(n) = a.steps {
        steps(world, n, clock, &a)?;
    }
    Ok(())
}

fn name(w: &World, (x, y, z): Cell) -> &'static str {
    w.block_at(BlockPos::new(x, y, z))
        .and_then(data::BlockKind::name)
        .unwrap_or("not loaded")
}

/// Why a route does not stand in a cell, in the planner's own terms (`Cells::standable`: a floor
/// under, the feet and the head clear, and no lava beside or under either); `None` when it does.
fn refusal(w: &World, cells: &mut Cells<'_>, c: Cell) -> Option<String> {
    if cells.standable(c) {
        return None;
    }
    let (x, y, z) = c;
    let mut why = Vec::new();
    let under = (x, y - 1, z);
    if !cells.floor(under) {
        why.push(format!(
            "no floor: {} under it is {:?}",
            name(w, under),
            cells.at(under)
        ));
    }
    for (part, cell) in [("feet", c), ("head", (x, y + 1, z))] {
        if cells.at(cell) != Class::Clear {
            why.push(format!(
                "the {part} cell is {} ({:?})",
                name(w, cell),
                cells.at(cell)
            ));
        } else if !cells.clear(cell) {
            let lava: Vec<String> = [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, -1, 0)]
                .iter()
                .map(|(dx, dy, dz)| (cell.0 + dx, cell.1 + dy, cell.2 + dz))
                .filter(|n| cells.at(*n) == Class::Lava)
                .map(|n| format!("{n:?}"))
                .collect();
            why.push(format!(
                "lava beside or under the {part} at {}",
                lava.join(" ")
            ));
        }
    }
    Some(why.join("; "))
}

fn flat(a: Cell, b: Cell) -> f64 {
    path::flat_distance(a, b)
}

fn route(w: &World, from: Cell, to: Cell, a: &Args) {
    let goal = if a.range <= 0.0 {
        Goal::Block { pos: to }
    } else {
        Goal::Near {
            pos: to,
            range: a.range,
        }
    };
    println!(
        "route from {from:?} to {goal:?}, at most {} nodes: {:.1} blocks on the plane, {} up",
        a.nodes,
        flat(from, to),
        to.1 - from.1
    );
    println!(
        "  the goal cell is {}, {} under it",
        name(w, to),
        name(w, (to.0, to.1 - 1, to.2))
    );
    let mut cells = Cells::new(w);
    // The start: the feet's own cell, or the nearest cell round it that stands.
    match refusal(w, &mut cells, from) {
        None => println!("  start: the feet cell {from:?} stands"),
        Some(why) => {
            println!("  start: the feet cell {from:?} is refused: {why}");
            let (x, y, z) = from;
            let mut round = Vec::new();
            for dy in [0, -1, 1] {
                for dx in [0, -1, 1] {
                    for dz in [0, -1, 1] {
                        round.push((x + dx, y + dy, z + dz));
                    }
                }
            }
            let standing: Vec<Cell> = round
                .iter()
                .copied()
                .filter(|c| cells.standable(*c))
                .collect();
            if standing.is_empty() {
                println!(
                    "  start: none of the 27 cells round the feet stands (the body's own box may \
                     still rest on one: see the route's `from`)"
                );
                for c in round.iter().filter(|c| c.1 == y) {
                    if let Some(why) = refusal(w, &mut cells, *c) {
                        println!("    {c:?}: {why}");
                    }
                }
            } else {
                println!("  start: cells round the feet that stand: {standing:?}");
            }
        }
    }
    let r = path::plan(w, from, &goal, a.nodes);
    println!(
        "  result: {:?}, {} waypoints, cost {:.1}, {} nodes expanded, from {:?}",
        r.status,
        r.waypoints.len(),
        r.cost,
        r.expanded,
        r.from
    );
    let end = r.waypoints.last().copied().or(r.from);
    if !r.waypoints.is_empty() {
        let head: Vec<String> = r
            .waypoints
            .iter()
            .take(12)
            .map(|c| format!("{c:?}"))
            .collect();
        println!("  waypoints: {}", head.join(" "));
        if r.waypoints.len() > 12
            && let Some(e) = end
        {
            println!("  ... to {e:?}");
        }
    }
    let Some(end) = end else {
        println!("  no start cell: the planner returns no route (`plan` → Status::None)");
        return;
    };
    if r.status == Status::Found {
        return;
    }
    println!(
        "  the route ends at {end:?}, {:.1} blocks from the goal on the plane, {} under it",
        flat(end, to),
        to.1 - end.1
    );
    let mut cells = Cells::new(w);
    let moves = path::neighbors(&mut cells, end);
    println!("  moves from there: {}", moves.len());
    for m in &moves {
        println!(
            "    {:?} to {:?} ({:.1} from the goal)",
            m.kind,
            m.node,
            flat(m.node, to)
        );
    }
    // The cells beside the end, a step up, level and a step down, that no move enters and that
    // are nearer the goal: the first refusals in the way.
    println!("  cells beside it nearer the goal that no move enters:");
    let mut shown = 0;
    for dy in [0, 1, -1] {
        for (dx, dz) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let c = (end.0 + dx, end.1 + dy, end.2 + dz);
            if flat(c, to) >= flat(end, to) || moves.iter().any(|m| m.node == c) {
                continue;
            }
            if let Some(why) = refusal(w, &mut cells, c) {
                println!("    {c:?}: {why}");
                shown += 1;
            } else {
                println!("    {c:?}: stands, but no move reaches it from {end:?}");
                shown += 1;
            }
        }
    }
    if shown == 0 {
        println!("    none");
    }
}

/// A step at least this slow is printed (a tick is 50 ms).
const SLOW_MS: f64 = 10.0;

fn steps(mut world: World, n: u64, clock: u64, a: &Args) -> Result<(), String> {
    let aim = plan::Aim::parse(&a.goal).ok_or_else(|| format!("--goal: no aim {}", a.goal))?;
    let mut p = if aim.is_goal() {
        let text = std::fs::read_to_string(&a.goals).map_err(|e| format!("{}: {e}", a.goals))?;
        let goals: plan::make::Goals =
            toml::from_str(&text).map_err(|e| format!("{}: {e}", a.goals))?;
        plan::Plan::with_table(aim, Arc::new(plan::make::Table::new(&goals)?))
    } else {
        plan::Plan::new(aim)
    };
    println!(
        "{n} steps of a new planner for --goal {} (open loop: the body moves, the blocks do not):",
        a.goal
    );
    let mut seen = 0u64;
    let mut last_controls = None;
    let mut times: Vec<f64> = Vec::new();
    for i in 0..n {
        // How long the step took (a fight's planner step ran 128 ms at p99 in race 45350933958
        // lane 1 and the bot was dropped): slow steps are printed with the task they ran.
        let t0 = std::time::Instant::now();
        let c = p.step(&world);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        times.push(ms);
        if ms >= SLOW_MS {
            println!(
                "  step {i:>5} SLOW {ms:.1} ms, task {} ({})",
                p.task.name(),
                p.why()
            );
        }
        let acts = p.take_acts();
        for e in p.events_since(seen) {
            println!("  step {i:>5} tick {}: {} ({})", e.tick, e.what, e.why);
        }
        seen = p.event_count;
        if Some(c) != last_controls {
            last_controls = Some(c);
            world.step(&Event::Controls(c));
        }
        for act in acts {
            println!("  step {i:>5} act: {act:?}");
            world.step(&Event::Act(act));
        }
        world.step(&Event::Tick {
            now_ms: clock + 50 * (i + 1),
            seed: i + 1,
        });
    }
    times.sort_by(f64::total_cmp);
    let at = |q: f64| {
        times
            .get(((times.len() as f64 - 1.0) * q) as usize)
            .copied()
            .unwrap_or(0.0)
    };
    println!(
        "step time ms: p50 {:.2}, p99 {:.2}, max {:.2} over {} steps",
        at(0.5),
        at(0.99),
        at(1.0),
        times.len()
    );
    let pos = world.body.map(|b| b.pos).or(world.player.pos);
    println!(
        "after {n} steps: task {} ({}), the bot at {pos:?}, {} notes",
        p.task.name(),
        p.why(),
        p.event_count
    );
    Ok(())
}
