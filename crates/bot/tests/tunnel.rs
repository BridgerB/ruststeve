//! The travel tunnel (docs/problems/nether-tunnel.md): its route search on hand-built worlds (a
//! long route climbs over the lava sea's level first, goes round a chasm and seen lava, cuts
//! through what rock hides and leaves that to the staircase's refusal), and the planner cutting
//! one to nether bricks no walk reaches, round a lava pocket it could not see.

use std::collections::BTreeSet;

use data::BlockKind;
use plan::tunnel::{self, SAFE_Y};
use plan::{Aim, Plan};
use sim::{Config, ModelWorld, Sim, flat_world};

const CONFIG: Config = Config {
    view_distance: 4,
    simulation_distance: 3,
    peaceful: true,
};

type Cell = (i32, i32, i32);

fn state(k: BlockKind) -> u32 {
    k.info().unwrap().default_state
}

/// Stone to y 62 and grass at 63 for four chunks round the origin, with `set` in it.
fn model(set: &[(Cell, BlockKind)]) -> ModelWorld {
    let mut m = flat_world(4, 63).unwrap();
    for ((x, y, z), k) in set {
        m.set_block(*x, *y, *z, state(*k)).unwrap();
    }
    m
}

/// The world a bot that joined `set` knows.
fn world_with(set: &[(Cell, BlockKind)]) -> world::World {
    let run = bot::lockstep::run(Sim::new(CONFIG, model(set)), "rs-tu", 1, 30, 20, |_, _| {
        physics::Controls::default()
    });
    run.world
}

fn filled(x: [i32; 2], y: [i32; 2], z: [i32; 2], k: BlockKind) -> Vec<(Cell, BlockKind)> {
    (x[0]..=x[1])
        .flat_map(|x| (y[0]..=y[1]).flat_map(move |y| (z[0]..=z[1]).map(move |z| ((x, y, z), k))))
        .collect()
}

fn none() -> BTreeSet<Cell> {
    BTreeSet::new()
}

/// Every step of a route is one the staircase can cut: a level step or a stair, one block along.
fn steps_are_cuts(cells: &[Cell]) {
    for pair in cells.windows(2) {
        assert!(
            tunnel::opened(pair[0], pair[1]).is_some(),
            "{:?} to {:?}",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn through_rock_the_route_is_level_and_ends_within_reach() {
    let w = world_with(&[]);
    let r = tunnel::route(&w, (0, 50, 0), (30, 50, 0), &none(), 20_000).unwrap();
    assert!(r.found);
    steps_are_cuts(&r.cells);
    assert!(
        r.cells.iter().all(|c| c.1 == 50 && c.2 == 0),
        "{:?}",
        r.cells
    );
    assert_eq!(r.cells.len(), 27, "26 steps to within 5 of the block");
    assert!(r.expanded < 200, "{} nodes for a straight way", r.expanded);
}

/// From the lava sea's level a long route climbs to `SAFE_Y` first, runs level there and comes
/// down only at the far end, to its target's height.
#[test]
fn a_long_route_from_low_climbs_first_and_comes_down_at_the_end() {
    let w = world_with(&[]);
    let r = tunnel::route(&w, (0, 34, 0), (40, 32, 0), &none(), 20_000).unwrap();
    assert!(r.found);
    steps_are_cuts(&r.cells);
    let top = r.cells.iter().map(|c| c.1).max().unwrap();
    assert_eq!(top, SAFE_Y);
    let up = r.cells.iter().position(|c| c.1 == SAFE_Y).unwrap();
    assert_eq!(up, usize::try_from(SAFE_Y - 34).unwrap(), "stairs up first");
    let level = r.cells.iter().filter(|c| c.1 == SAFE_Y).count();
    assert!(
        level >= 20,
        "{level} cells at the travel height: {:?}",
        r.cells
    );
    let end = r.cells.last().unwrap();
    assert!(end.1 < SAFE_Y, "down toward the target at the end: {end:?}");
}

/// A chasm with no floor across the line is gone round, in rock.
#[test]
fn a_chasm_is_gone_round() {
    let w = world_with(&filled([10, 14], [30, 70], [-6, 6], BlockKind::Air));
    let r = tunnel::route(&w, (0, 50, 0), (30, 50, 0), &none(), 40_000).unwrap();
    assert!(r.found);
    steps_are_cuts(&r.cells);
    assert!(
        r.cells
            .iter()
            .all(|c| !((10..=14).contains(&c.0) && (-6..=6).contains(&c.2))),
        "{:?}",
        r.cells
    );
}

/// Lava with an open cell over it is known and kept a block away from; a pocket shut in rock is
/// not known (no X-ray): the route runs into it, the staircase refuses the step beside it, and
/// with that cell blocked the next route goes round.
#[test]
fn seen_lava_is_avoided_and_hidden_lava_is_left_to_the_refusal() {
    let mut lake = filled([12, 13], [50, 51], [-1, 1], BlockKind::Lava);
    lake.extend(filled([12, 13], [52, 52], [-1, 1], BlockKind::Air));
    let w = world_with(&lake);
    let r = tunnel::route(&w, (0, 50, 0), (30, 50, 0), &none(), 40_000).unwrap();
    assert!(r.found);
    let lava = |c: Cell| (12..=13).contains(&c.0) && (50..=51).contains(&c.1) && c.2.abs() <= 1;
    for c in &r.cells {
        let beside = [0, 1].iter().any(|up| {
            [
                (1, 0, 0),
                (-1, 0, 0),
                (0, 1, 0),
                (0, -1, 0),
                (0, 0, 1),
                (0, 0, -1),
            ]
            .iter()
            .any(|d| lava((c.0 + d.0, c.1 + up + d.1, c.2 + d.2)))
        });
        assert!(
            !beside && !lava(*c),
            "{c:?} is beside the lake: {:?}",
            r.cells
        );
    }

    let w = world_with(&filled([12, 12], [50, 51], [0, 0], BlockKind::Lava));
    let r = tunnel::route(&w, (0, 50, 0), (30, 50, 0), &none(), 40_000).unwrap();
    assert!(r.cells.contains(&(12, 50, 0)), "the pocket is not known");
    let (cells, floor) = tunnel::opened((10, 50, 0), (11, 50, 0)).unwrap();
    let why = plan::stairs::refusal(&w, &cells, floor).unwrap();
    assert!(why.starts_with("lava beside"), "{why}");
    let blocked: BTreeSet<Cell> = [(11, 50, 0)].into();
    let r = tunnel::route(&w, (10, 50, 0), (30, 50, 0), &blocked, 40_000).unwrap();
    assert!(r.found && !r.cells.contains(&(11, 50, 0)));
}

/// A portal's own blocks are never stepped into (a step in is a trip to the other dimension).
#[test]
fn a_portal_is_never_stepped_into() {
    let mut set = filled([5, 5], [64, 66], [-1, 1], BlockKind::NetherPortal);
    set.extend(filled([5, 5], [63, 63], [-1, 1], BlockKind::Obsidian));
    let w = world_with(&set);
    let r = tunnel::route(&w, (0, 64, 0), (30, 64, 0), &none(), 40_000).unwrap();
    assert!(r.found);
    assert!(
        r.cells
            .iter()
            .all(|c| !(c.0 == 5 && (-1..=1).contains(&c.2) && c.1 >= 63)),
        "{:?}",
        r.cells
    );
}

/// The planner, closed loop: nether bricks in a pocket under the ground 30 blocks off, no walk to
/// them, and a wall of lava shut in the rock between. The tunnel is cut, the steps beside the
/// lava are refused and blocked, the route goes round, and the fortress is met alive.
#[test]
fn the_planner_tunnels_to_bricks_no_walk_reaches_round_hidden_lava() {
    let mut set = vec![
        ((30, 50, 0), BlockKind::NetherBricks),
        ((31, 50, 0), BlockKind::Air),
        ((31, 51, 0), BlockKind::Air),
    ];
    set.extend(filled([15, 15], [44, 62], [-2, 2], BlockKind::Lava));
    let mut m = model(&set);
    m.dimension = "minecraft:the_nether".to_owned();
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut p = Plan::with_table(Aim::Fortress, table);
    let mut sim = Sim::new(CONFIG, m).seeded(1);
    sim.survival.give((data::ItemKind::StonePickaxe, 1));
    sim.survival.give((data::ItemKind::StonePickaxe, 1));
    let mut notes = Vec::new();
    let mut seen = 0;
    let run = bot::lockstep::run_acting(sim, "rs-tu", 1, 9000, 20, |w, _| {
        let c = p.step(w);
        notes.extend(
            p.events_since(seen)
                .iter()
                .map(|e| format!("{} ({})", e.what, e.why)),
        );
        seen = p.event_count;
        (c, p.take_acts())
    });
    let count = |s: &str| notes.iter().filter(|n| n.contains(s)).count();
    assert!(
        p.met,
        "not met at {:?}: {notes:#?}",
        run.world.body.map(|b| b.pos)
    );
    assert!(!run.died);
    assert!(
        count("tunnel route 1 toward the fortress") == 1,
        "{notes:#?}"
    );
    assert!(
        count("lava beside") >= 1,
        "the hidden lava refused a step: {notes:#?}"
    );
    assert!(count("tunnel step to") >= 1, "{notes:#?}");
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}
