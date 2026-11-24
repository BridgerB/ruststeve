//! The heading (issue 17d, docs/problems/heading.md) on hand-built Nethers: it holds its axis over
//! open ground, tunnels a wall at the cost of a level cut, never walks back over a causeway it
//! cut, bridges a lava sea with the keep held after restocking from the wall, fails a leg that
//! stops gaining, turns for a crimson forest at the feet, and gives way to a known forest.

use std::collections::BTreeSet;

use data::{BlockKind, ItemKind};
use plan::heading::{self, Dir, Heading, Signs};
use plan::{Aim, Plan, Task};
use sim::nether::Stage;
use sim::{Config, Sim};

const CONFIG: Config = Config {
    view_distance: 4,
    simulation_distance: 3,
    peaceful: true,
};

fn table() -> std::sync::Arc<plan::make::Table> {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    std::sync::Arc::new(bot::goals_table(goals).unwrap())
}

struct Out {
    p: Plan,
    run: bot::lockstep::Run,
    notes: Vec<String>,
    /// The body's place every 20 ticks after the first heading note.
    track: Vec<[f64; 3]>,
}

impl Out {
    fn count(&self, s: &str) -> usize {
        self.notes.iter().filter(|n| n.contains(s)).count()
    }

    fn tail(&self) -> Vec<&String> {
        self.notes.iter().rev().take(40).rev().collect()
    }
}

/// The planner holding the aim `hold` (no goal of its own), its heading set to `dir` from where
/// the body stands, driven by `heading_with` whenever it is idle, for `ticks`.
fn drive(stage: &Stage, give: &[(ItemKind, i32)], dir: Dir, signs: Signs, ticks: u64) -> Out {
    let mut sim = Sim::new(CONFIG, stage.build().unwrap()).seeded(1);
    for g in give {
        sim.survival.give(*g);
    }
    let mut p = Plan::with_table(Aim::Hold, table());
    let mut notes = Vec::new();
    let mut track = Vec::new();
    let mut seen = 0;
    let mut over = false;
    let run = bot::lockstep::run_acting(sim, "rs-hd", 1, ticks, 20, |w, _| {
        let tick = w.ticks;
        if let Some(b) = w.body {
            if p.headed.is_none() && !over {
                let feet = plan::path::stand_cell(w, b.pos);
                p.headed = Some(Heading::new(dir, feet, tick, "set by the test".to_owned()));
            }
            if tick.is_multiple_of(20) {
                track.push(b.pos);
            }
            if matches!(p.task, Task::Idle) && !over {
                match p.heading_with(w, signs, tick) {
                    Some(t) => p.task = t,
                    None => over = true,
                }
            }
        }
        let c = p.step(w);
        notes.extend(
            p.events_since(seen)
                .iter()
                .map(|e| format!("{} {} ({})", e.tick, e.what, e.why)),
        );
        seen = p.event_count;
        (c, p.take_acts())
    });
    Out {
        p,
        run,
        notes,
        track,
    }
}

/// A flat Nether cavern over x `x`, z `z`: netherrack to y 63, air to 79, the bot at (0, 64, 0).
fn flat(x: (i32, i32), z: (i32, i32)) -> Stage {
    let mut s = Stage::over(x, z, 63, 80);
    s.spawn((0, 64, 0));
    s
}

/// Over open ground the heading holds its axis 200 blocks with no turn, walking.
#[test]
fn open_ground_holds_the_axis_two_hundred_blocks() {
    let stage = flat((-16, 260), (-24, 24));
    let out = drive(
        &stage,
        &[(ItemKind::Netherrack, 64)],
        (1, 0),
        Signs::default(),
        4000,
    );
    let x = out.run.world.body.map_or(0.0, |b| b.pos[0]);
    assert!(x >= 200.0, "at x {x}: {:#?}", out.tail());
    assert_eq!(out.count("heading turned"), 0, "{:#?}", out.tail());
    assert!(out.count("heading leg") >= 4, "{:#?}", out.tail());
    assert!(out.count("by walk") >= 4, "{:#?}", out.tail());
    let h = out.p.headed.as_ref().unwrap();
    assert_eq!(h.dir, (1, 0));
    assert_eq!(h.digs, 0);
    assert!(!out.run.died);
}

/// Open ground ends in solid netherrack at x 30: the heading tunnels on its axis, level, at two
/// digs a metre at most, and never turns.
#[test]
fn a_wall_is_tunnelled_on_the_axis_at_two_digs_a_metre() {
    let mut stage = flat((-16, 140), (-24, 24));
    stage.fill((30, 140), (64, 79), (-24, 24), BlockKind::Netherrack);
    let out = drive(
        &stage,
        &[
            (ItemKind::Netherrack, 64),
            (ItemKind::IronPickaxe, 1),
            (ItemKind::IronPickaxe, 1),
        ],
        (1, 0),
        Signs::default(),
        9000,
    );
    let x = out.run.world.body.map_or(0.0, |b| b.pos[0]);
    assert!(x >= 80.0, "at x {x}: {:#?}", out.tail());
    assert_eq!(out.count("heading turned"), 0, "{:#?}", out.tail());
    assert!(
        out.count("tunnel level toward the heading") >= 1,
        "{:#?}",
        out.tail()
    );
    let h = out.p.headed.as_ref().unwrap();
    let rock = (x - 30.0).max(1.0);
    assert!(
        f64::from(h.digs) <= 2.0 * rock + 4.0,
        "{} digs for {rock:.0} m of rock: {:#?}",
        h.digs,
        out.tail()
    );
    assert!(h.digs > 0, "the digs were counted: {:#?}", out.tail());
    assert!(!out.run.died);
}

/// The 1-wide causeway with a dead end (clojurecraft's `heading-no-pace` case). West ground, a lava
/// sea east of x 0, a causeway on z 0 to x 60, and land north of the sea from x -16 to 80. The
/// first leg walks the causeway; the second has no way east (the sea past x 60 needs more blocks
/// than the keep spares), so the heading turns north. A walk back along the causeway and round by
/// the west ground is a plain route north; it runs over cut cells and is refused, so the far walk
/// bridges north. The causeway is never walked back.
#[test]
fn a_dead_end_causeway_is_turned_from_and_never_walked_back() {
    let mut stage = Stage::over((-16, 120), (-90, 30), 49, 80);
    stage
        .fill((1, 120), (50, 63), (-90, 30), BlockKind::Lava)
        .fill((-16, 0), (50, 63), (-90, 30), BlockKind::Netherrack)
        .fill((1, 60), (63, 63), (0, 0), BlockKind::Netherrack)
        .fill((-16, 80), (50, 63), (-90, -14), BlockKind::Netherrack)
        .spawn((-5, 64, 0));
    let out = drive(
        &stage,
        &[(ItemKind::Netherrack, 50)],
        (1, 0),
        Signs::default(),
        5000,
    );
    let tail = out.tail();
    assert_eq!(out.count("heading turned to (0, -1)"), 1, "{tail:#?}");
    assert_eq!(out.count("heading turned"), 1, "{tail:#?}");
    let turned = out
        .notes
        .iter()
        .find(|n| n.contains("heading turned"))
        .and_then(|n| n.split(' ').next())
        .and_then(|t| t.parse::<u64>().ok())
        .unwrap();
    // Every place after the turn: never back west along the causeway.
    let after: Vec<[f64; 3]> = out
        .track
        .iter()
        .skip(usize::try_from(turned / 20).unwrap())
        .copied()
        .collect();
    let min_x = after.iter().map(|p| p[0]).fold(f64::MAX, f64::min);
    assert!(min_x >= 35.0, "walked back to x {min_x}: {tail:#?}");
    let z = out.run.world.body.map_or(0.0, |b| b.pos[2]);
    assert!(z <= -20.0, "not north at z {z}: {tail:#?}");
    assert!(out.count("no way") >= 1, "the turn says why: {tail:#?}");
    assert!(!out.run.died);
}

/// The same causeway with too few blocks to bridge north: the only way is back over the cut, the
/// note says so ("paced"), and the bot never walks back.
#[test]
fn a_way_only_back_over_the_cut_is_refused_and_said() {
    let mut stage = Stage::over((-16, 120), (-90, 30), 49, 80);
    stage
        .fill((1, 120), (50, 63), (-90, 30), BlockKind::Lava)
        .fill((-16, 0), (50, 63), (-90, 30), BlockKind::Netherrack)
        .fill((1, 60), (63, 63), (0, 0), BlockKind::Netherrack)
        .fill((-16, 80), (50, 63), (-90, -14), BlockKind::Netherrack)
        .spawn((-5, 64, 0));
    let out = drive(
        &stage,
        &[(ItemKind::Netherrack, 36)],
        (1, 0),
        Signs::default(),
        3000,
    );
    let tail = out.tail();
    assert!(out.count("refused for cut (paced)") >= 1, "{tail:#?}");
    let max_x = out.track.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
    let last_x = out.run.world.body.map_or(0.0, |b| b.pos[0]);
    assert!(max_x >= 40.0, "the causeway was walked: {tail:#?}");
    assert!(
        last_x >= max_x - 6.0,
        "walked back to x {last_x}: {tail:#?}"
    );
}

/// A lava sea 10 wide across the axis, the bot holding 20 blocks (under the keep of 32) beside a
/// netherrack wall: it restocks from the wall first, then bridges the sea and keeps 32.
#[test]
fn a_lava_sea_is_bridged_with_the_keep_held_after_a_restock() {
    let mut stage = Stage::over((-16, 120), (-24, 24), 49, 80);
    stage
        .fill((-16, 120), (50, 63), (-24, 24), BlockKind::Netherrack)
        .fill((11, 20), (50, 63), (-24, 24), BlockKind::Lava)
        .fill((-6, -2), (64, 67), (-3, 3), BlockKind::Netherrack)
        .spawn((0, 64, 0));
    let out = drive(
        &stage,
        &[(ItemKind::Netherrack, 20), (ItemKind::StonePickaxe, 1)],
        (1, 0),
        Signs::default(),
        6000,
    );
    let tail = out.tail();
    assert!(out.count("restock dig") >= 12, "{tail:#?}");
    assert!(out.count("restock over") >= 1, "{tail:#?}");
    assert!(out.count("bridge block laid") >= 10, "{tail:#?}");
    let x = out.run.world.body.map_or(0.0, |b| b.pos[0]);
    assert!(x >= 40.0, "at x {x}: {tail:#?}");
    let held = plan::path::bridge_blocks(&out.run.world);
    assert!(held >= heading::KEEP, "{held} blocks held: {tail:#?}");
    assert_eq!(out.count("heading turned"), 0, "{tail:#?}");
    assert!(!out.run.died);
}

/// The world a bot that joined a flat Nether knows, and the heading at its feet.
fn joined() -> (world::World, Plan, (i32, i32, i32)) {
    let stage = flat((-16, 120), (-40, 40));
    let sim = Sim::new(CONFIG, stage.build().unwrap());
    let run = bot::lockstep::run(sim, "rs-hd", 1, 30, 20, |_, _| physics::Controls::default());
    let w = run.world;
    let feet = plan::path::stand_cell(&w, w.body.unwrap().pos);
    let mut p = Plan::with_table(Aim::Hold, table());
    p.headed = Some(Heading::new((1, 0), feet, 0, "set by the test".to_owned()));
    (w, p, feet)
}

fn said(p: &Plan) -> Vec<String> {
    p.events
        .iter()
        .map(|e| format!("{} {} ({})", e.tick, e.what, e.why))
        .collect()
}

/// The projection invariant: a leg whose best has not grown `PACE_GAIN` in `PACE_TICKS` fails
/// `NoProgress` with a note, and the heading turns.
#[test]
fn a_leg_with_no_gain_for_twelve_hundred_ticks_fails() {
    let (w, mut p, feet) = joined();
    let first = p.heading_with(&w, Signs::default(), 100);
    assert!(matches!(first, Some(Task::Explore(_))), "{:#?}", said(&p));
    assert!(said(&p).iter().any(|n| n.contains("heading leg 1 toward")));
    // The body never moved: at 1,200 ticks the leg is still going.
    let _ = p.heading_with(&w, Signs::default(), 100 + heading::PACE_TICKS);
    assert!(
        !said(&p).iter().any(|n| n.contains("NoProgress")),
        "{:#?}",
        said(&p)
    );
    let _ = p.heading_with(&w, Signs::default(), 101 + heading::PACE_TICKS);
    let notes = said(&p);
    assert!(
        notes
            .iter()
            .any(|n| n.contains("heading leg 1 failed (NoProgress")),
        "{notes:#?}"
    );
    assert!(
        notes.iter().any(|n| n.contains("heading turned to")),
        "{notes:#?}"
    );
    let h = p.headed.as_ref().unwrap();
    assert_ne!(h.dir, (1, 0));
    assert_eq!(h.origin, feet, "a fresh projection from the feet");
    assert!(h.cut.contains(&feet), "the cut is kept");
}

/// A crimson forest at the feet turns the heading, once; a second look in it does not turn again.
#[test]
fn a_crimson_forest_at_the_feet_turns_the_heading_once() {
    let (w, mut p, _) = joined();
    let crimson = Signs {
        feet_biome: Some("crimson_forest"),
        forest: None,
    };
    let _ = p.heading_with(&w, crimson, 100);
    let turned = |p: &Plan| {
        said(p)
            .iter()
            .filter(|n| n.contains("heading turned to") && n.contains("crimson_forest"))
            .count()
    };
    assert_eq!(turned(&p), 1, "{:#?}", said(&p));
    let dir = p.headed.as_ref().unwrap().dir;
    assert_ne!(dir, (1, 0));
    p.task = Task::Idle;
    let _ = p.heading_with(&w, crimson, 140);
    assert_eq!(turned(&p), 1, "{:#?}", said(&p));
}

/// A forest known from warped blocks seen replaces the axis: the leg runs to it. A warped forest
/// at the feet ends the heading (`None`: the endermen task takes over).
#[test]
fn a_known_forest_replaces_the_heading_and_its_biome_ends_it() {
    let (w, mut p, feet) = joined();
    let forest = Signs {
        feet_biome: None,
        forest: Some((-2, 1)),
    };
    let t = p.heading_with(&w, forest, 100);
    assert!(t.is_some(), "{:#?}", said(&p));
    let notes = said(&p);
    assert!(
        notes
            .iter()
            .any(|n| n.contains("heading to the warped forest at column (-2, 1)")),
        "{notes:#?}"
    );
    let leg = p.headed.as_ref().unwrap().leg.clone().unwrap();
    assert_eq!(leg.target, (-24, feet.1, 24), "the forest's column centre");
    let warped = Signs {
        feet_biome: Some("warped_forest"),
        forest: None,
    };
    p.task = Task::Idle;
    assert!(p.heading_with(&w, warped, 140).is_none());
    assert!(p.headed.as_ref().unwrap().over);
    assert!(
        said(&p)
            .iter()
            .any(|n| n.contains("heading ended: warped forest")),
        "{:#?}",
        said(&p)
    );
}

/// The axis chosen points away from what is explored (a fortress's columns west of the feet), and
/// with nothing to tell, to the fewest seen columns ahead.
#[test]
fn the_axis_points_away_from_the_explored_and_else_to_the_unseen() {
    let fortress: BTreeSet<(i32, i32)> = (-12..=-6)
        .flat_map(|x| (-3..=3).map(move |z| (x, z)))
        .collect();
    let (dir, why) = heading::choose_dir((0, 64, 0), &fortress, &[]).unwrap();
    assert_eq!(dir, (1, 0), "{why}");
    // Never an avoided axis.
    let (dir, _) = heading::choose_dir((0, 64, 0), &fortress, &[(1, 0)]).unwrap();
    assert_ne!(dir, (1, 0));
    // Seen all round the feet but a strip south: the centroid is near, so the strip decides.
    let round: BTreeSet<(i32, i32)> = (-16..=16)
        .flat_map(|x| (-16..=16).map(move |z| (x, z)))
        .filter(|(x, z): &(i32, i32)| !(*z >= 7 && x.abs() <= 2))
        .collect();
    let (dir, why) = heading::choose_dir((8, 64, 8), &round, &[]).unwrap();
    assert_eq!(dir, (0, 1), "{why}");
    // The turn goes to the side with fewer seen columns, a tie away from the last side.
    assert_eq!(heading::turn_dir((1, 0), (0, 0), &round, 0).0, (0, 1));
    let none = BTreeSet::new();
    assert_eq!(heading::turn_dir((1, 0), (0, 0), &none, 0), ((0, -1), 1));
    assert_eq!(heading::turn_dir((1, 0), (0, 0), &none, 1), ((0, 1), -1));
}
