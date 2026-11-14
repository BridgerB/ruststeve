//! Issue 12b: the always-rows cut whatever runs and get the bot out of water, lava and fire, in
//! a flat model world the bot has joined (brain/ruststeve/design/escapes.md).

use data::BlockKind;
use plan::escape::TrapKind;
use plan::{Aim, Plan};
use sim::{Config, Sim, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

fn state(k: BlockKind) -> u32 {
    k.info().unwrap().default_state
}

struct Ended {
    plan: Plan,
    run: bot::lockstep::Run,
    /// The tick the trap last cleared, if it did.
    out_at: Option<u64>,
}

/// The bot spawns at `spawn` in the flat world (grass at y 63) with `set` laid, a planner for
/// the table aim (nothing to gather on a flat world, so it explores) and the goal table's
/// always-rows, run for `ticks`.
fn escape(spawn: [f64; 3], set: &[((i32, i32, i32), BlockKind)], ticks: u64) -> Ended {
    escape_with(spawn, set, &[], ticks)
}

fn escape_with(
    spawn: [f64; 3],
    set: &[((i32, i32, i32), BlockKind)],
    give: &[(data::ItemKind, i32)],
    ticks: u64,
) -> Ended {
    let mut m = flat_world(2, 63).unwrap();
    for ((x, y, z), k) in set {
        m.set_block(*x, *y, *z, state(*k)).unwrap();
    }
    m.spawn = spawn;
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut p = Plan::with_table(Aim::Table, table);
    let mut out_at = None;
    let mut was = false;
    let mut sim = Sim::new(CONFIG, m);
    for g in give {
        sim.survival.give(*g);
    }
    let run = bot::lockstep::run_acting(sim, "rs-esc", 1, ticks, 0, |w, t| {
        let c = p.step(w);
        let now = p.traps.kind.is_some();
        if was && !now {
            out_at = Some(t);
        }
        was = now;
        (c, p.take_acts())
    });
    Ended {
        plan: p,
        run,
        out_at,
    }
}

fn cells(
    xs: std::ops::RangeInclusive<i32>,
    ys: std::ops::RangeInclusive<i32>,
    zs: std::ops::RangeInclusive<i32>,
    k: BlockKind,
) -> Vec<((i32, i32, i32), BlockKind)> {
    xs.flat_map(|x| {
        let zs = zs.clone();
        ys.clone()
            .flat_map(move |y| zs.clone().map(move |z| ((x, y, z), k)))
    })
    .collect()
}

fn assert_out(e: &Ended, kind: TrapKind) {
    let w = &e.run.world;
    // The escape counter, not the events: the planner keeps only its last 64 events, and one
    // that goes on after the escape (relocating, issue 13b) scrolls it out.
    assert!(
        e.plan.escapes >= 1,
        "never escaped from {}: {:#?}",
        kind.name(),
        e.plan.events
    );
    assert!(
        e.out_at.is_some(),
        "still trapped at {:?}: {:#?} {:#?}",
        e.run.world.body.map(|b| b.pos),
        e.plan.task,
        e.plan.events
    );
    assert!(!e.run.died, "died: {:#?}", e.plan.events);
    let body = w.body.unwrap();
    assert!(
        plan::escape::dry_land(w, &body),
        "not on dry land at {:?}",
        body.pos
    );
    assert!(
        e.run.sim.violations.is_empty(),
        "{:?}",
        e.run.sim.violations
    );
}

#[test]
fn a_pond_is_swum_out_of_onto_its_bank() {
    // A pool 7 wide and 4 deep, the bot at its bottom.
    let e = escape(
        [0.5, 60.0, 0.5],
        &cells(-3..=3, 60..=63, -3..=3, BlockKind::Water),
        600,
    );
    assert_out(&e, TrapKind::Water);
}

#[test]
fn a_shaft_under_a_lid_is_dug_out_of() {
    // A water shaft one wide, 6 deep, capped by dirt over the grass line: the bank beside the
    // lid is reached only once the lid is dug.
    let mut set = cells(0..=0, 58..=63, 0..=0, BlockKind::Water);
    set.push(((0, 64, 0), BlockKind::Dirt));
    let e = escape([0.5, 58.0, 0.5], &set, 1200);
    assert_out(&e, TrapKind::Water);
    assert_eq!(
        e.run.world.block_at(world::BlockPos::new(0, 64, 0)),
        Some(BlockKind::Air),
        "the lid was not dug"
    );
}

#[test]
fn lava_is_fled_at_once() {
    // A lava sheet one deep and 3 wide, the bot standing in its middle.
    let mut set = cells(-1..=1, 63..=63, -1..=1, BlockKind::Lava);
    set.extend(cells(-1..=1, 62..=62, -1..=1, BlockKind::Stone));
    let e = escape([0.5, 63.0, 0.5], &set, 400);
    assert_out(&e, TrapKind::Lava);
    assert!(e.out_at.unwrap() < 60, "out only at {:?}", e.out_at);
}

#[test]
fn a_fire_block_is_stepped_out_of() {
    let set = [
        ((0, 63, 0), BlockKind::Netherrack),
        ((0, 64, 0), BlockKind::Fire),
    ];
    let e = escape([0.5, 64.0, 0.5], &set, 200);
    assert_out(&e, TrapKind::Fire);
    assert!(e.out_at.unwrap() < 40, "out only at {:?}", e.out_at);
}

#[test]
fn a_pit_with_no_bank_is_notched() {
    // Water three wide at the bottom of a pit cut into the stone, its walls rising four over the
    // water: no bank is in reach, so a notch two high is cut just over the surface, with the
    // wooden pickaxe the race holds by then.
    let mut set = cells(-1..=1, 55..=58, -1..=1, BlockKind::Water);
    set.extend(cells(-1..=1, 59..=63, -1..=1, BlockKind::Air));
    let e = escape_with(
        [0.5, 56.0, 0.5],
        &set,
        &[(data::ItemKind::WoodenPickaxe, 1)],
        4000,
    );
    assert_out(&e, TrapKind::Water);
    assert!(
        e.run.world.body.unwrap().pos[1] < 61.0,
        "climbed out instead"
    );
}

#[test]
fn a_pit_with_no_bank_is_notched_by_hand() {
    // The same pit early in a race, nothing held: stone by hand from the surface, off the ground,
    // takes about 860 ticks a block.
    let mut set = cells(-1..=1, 55..=58, -1..=1, BlockKind::Water);
    set.extend(cells(-1..=1, 59..=63, -1..=1, BlockKind::Air));
    let e = escape([0.5, 56.0, 0.5], &set, 4000);
    assert_out(&e, TrapKind::Water);
}

#[test]
fn a_block_at_the_eyes_is_dug_out() {
    // Gravel in the eyes' cell (fallen into a tunnel): in_wall damage until it is dug.
    // With the stone pickaxe a miner holds: by hand, off the ground in a block, it takes 865.
    let e = escape_with(
        [0.5, 64.0, 0.5],
        &[((0, 65, 0), BlockKind::Stone)],
        &[(data::ItemKind::StonePickaxe, 1)],
        300,
    );
    assert!(e.plan.escapes >= 1, "never escaped: {:#?}", e.plan.events);
    assert!(!e.run.died);
    // Out of it, whether by the dig or by being pushed clear of the block.
    let body = e.run.world.body.unwrap();
    assert!(
        !plan::escape::eye_in_wall(&e.run.world, &body),
        "still in the wall at {:?}: {:#?}",
        body.pos,
        e.plan.task
    );
}
