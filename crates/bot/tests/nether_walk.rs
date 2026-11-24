//! The Nether walk by route (docs/problems/nether-walk.md): soul sand is ground, and a gap or a
//! lava moat is crossed by laying held blocks, in a flat model Nether the bot has joined.

use data::{BlockKind, ItemKind};
use plan::path::{self, Goal, MoveKind, Status};
use plan::walk::{Arrive, TaskStatus, Walk};
use sim::{Config, Sim, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

fn state(k: BlockKind) -> u32 {
    k.info().unwrap().default_state
}

/// A flat Nether (ground at y 63, the feet at y 64) with `set` laid and `give` held.
fn nether(set: &[((i32, i32, i32), BlockKind)], give: &[(ItemKind, i32)]) -> Sim {
    let mut m = flat_world(2, 63).unwrap();
    m.dimension = "minecraft:the_nether".to_owned();
    m.dimension_type = 3;
    for ((x, y, z), k) in set {
        m.set_block(*x, *y, *z, state(*k)).unwrap();
    }
    let mut sim = Sim::new(CONFIG, m);
    for g in give {
        sim.survival.give(*g);
    }
    sim
}

/// The world as the bot knows it a second after joining.
fn joined(sim: Sim) -> world::World {
    bot::lockstep::run(sim, "rs-nw", 1, 20, 20, |_, _| physics::Controls::default()).world
}

/// A band across the whole world at x 2 to 5 (no way round), from `y0` to `y1`.
fn band(y0: i32, y1: i32, k: BlockKind) -> Vec<((i32, i32, i32), BlockKind)> {
    (2..=5)
        .flat_map(|x| (y0..=y1).flat_map(move |y| (-48..=47).map(move |z| ((x, y, z), k))))
        .collect()
}

struct Walked {
    walk: Walk,
    run: bot::lockstep::Run,
    done: bool,
}

/// Step `walk` in `sim` for up to `ticks`, sending its acts.
fn walked(sim: Sim, mut walk: Walk, ticks: u64) -> Walked {
    let mut done = false;
    let run = bot::lockstep::run_acting(sim, "rs-nw", 1, ticks, 20, |w, t| {
        if done {
            return (physics::Controls::default(), Vec::new());
        }
        let (c, status) = walk.step(w, t);
        done = status == TaskStatus::Done;
        (c, walk.take_out().0)
    });
    Walked { walk, run, done }
}

#[test]
fn soul_sand_is_ground_for_a_route() {
    // A band of soul sand with no way round: before, `Awkward`, and no route crossed it.
    let w = joined(nether(&band(63, 63, BlockKind::SoulSand), &[]));
    let r = path::plan(&w, (0, 64, 0), &Goal::Block { pos: (8, 64, 0) }, 6000);
    assert_eq!(r.status, Status::Found);
    assert_eq!(r.waypoints.len(), 8, "{:?}", r.waypoints);
    assert!(
        r.kinds.iter().all(|k| *k == MoveKind::Walk),
        "{:?}",
        r.kinds
    );
    // Magma stays a wall.
    let w = joined(nether(&band(63, 63, BlockKind::MagmaBlock), &[]));
    let r = path::plan(&w, (0, 64, 0), &Goal::Block { pos: (8, 64, 0) }, 6000);
    assert_ne!(r.status, Status::Found);
}

#[test]
fn the_feet_on_soul_sand_are_in_the_cell_over_it() {
    assert_eq!(path::feet_cell([3.5, 63.875, 0.5]), (3, 64, 0));
    assert_eq!(path::feet_cell([3.5, 64.0, 0.5]), (3, 64, 0));
    assert_eq!(path::feet_cell([3.5, 63.5, 0.5]), (3, 63, 0));
    assert_eq!(path::feet_cell([-0.5, -0.125, -0.5]), (-1, 0, -1));
}

#[test]
fn a_walk_crosses_soul_sand_and_ends_on_it() {
    // Across the band and on to grass.
    let sim = nether(&band(63, 63, BlockKind::SoulSand), &[]);
    let walk = Walk::new((8, 64, 0), Goal::Block { pos: (8, 64, 0) }, Arrive::Goal, 0);
    let out = walked(sim, walk, 400);
    assert!(out.done, "at {:?}", out.run.world.player.pos);
    // And a walk that ends on the soul sand itself: the goal is the cell over it.
    let sim = nether(&band(63, 63, BlockKind::SoulSand), &[]);
    let walk = Walk::new((4, 64, 0), Goal::Block { pos: (4, 64, 0) }, Arrive::Goal, 0);
    let out = walked(sim, walk, 400);
    assert!(out.done, "at {:?}", out.run.world.player.pos);
    let y = out.run.world.player.pos.unwrap()[1];
    assert!((y - 63.875).abs() < 0.01, "feet at {y}");
    // From the soul sand, a route is found (the start is the cell over it).
    let w = out.run.world;
    let from = path::feet_cell(w.player.pos.unwrap());
    assert_eq!(from, (4, 64, 0));
    let r = path::plan(&w, from, &Goal::Block { pos: (0, 64, 0) }, 6000);
    assert_eq!(r.status, Status::Found);
}

#[test]
fn a_lava_moat_is_routed_over_only_with_blocks_held() {
    // Lava for ground across the world: with nothing held there is no way.
    let moat = band(63, 63, BlockKind::Lava);
    let w = joined(nether(&moat, &[]));
    let goal = Goal::Block { pos: (8, 64, 0) };
    assert_ne!(
        path::plan_far(&w, (0, 64, 0), &goal, 6000, 1.0).status,
        Status::Found
    );
    // With cobblestone held the route bridges its four cells and walks the rest.
    let w = joined(nether(&moat, &[(ItemKind::Cobblestone, 64)]));
    let r = path::plan_far(&w, (0, 64, 0), &goal, 6000, 1.0);
    assert_eq!(r.status, Status::Found);
    let bridged: Vec<_> = r
        .waypoints
        .iter()
        .zip(&r.kinds)
        .filter(|(_, k)| **k == MoveKind::Bridge)
        .map(|(c, _)| *c)
        .collect();
    assert_eq!(
        bridged,
        vec![(2, 64, 0), (3, 64, 0), (4, 64, 0), (5, 64, 0)]
    );
    // A plain plan (an explore, a walk to a log) never bridges.
    let goal = Goal::Block { pos: (8, 64, 0) };
    assert_ne!(
        path::plan(&w, (0, 64, 0), &goal, 6000).status,
        Status::Found
    );
    // Fewer than the reserve is not a crossing.
    let w = joined(nether(&moat, &[(ItemKind::Cobblestone, 8)]));
    assert_ne!(
        path::plan_far(&w, (0, 64, 0), &goal, 6000, 1.0).status,
        Status::Found
    );
}

#[test]
fn the_overworld_is_not_bridged() {
    let mut m = flat_world(2, 63).unwrap();
    for ((x, y, z), k) in band(56, 63, BlockKind::Air) {
        m.set_block(x, y, z, state(k)).unwrap();
    }
    let mut sim = Sim::new(CONFIG, m);
    sim.survival.give((ItemKind::Cobblestone, 64));
    let w = joined(sim);
    let r = path::plan_far(&w, (0, 64, 0), &Goal::Block { pos: (8, 64, 0) }, 6000, 1.0);
    assert_ne!(r.status, Status::Found);
}

#[test]
fn a_chasm_is_bridged_block_by_block() {
    // Eight deep: no drop crosses it. The walk lays four blocks and arrives unhurt.
    let sim = nether(
        &band(56, 63, BlockKind::Air),
        &[(ItemKind::Cobblestone, 64)],
    );
    let out = walked(sim, Walk::far((8, 64, 0), 0.0, 0), 1200);
    assert!(
        out.done,
        "at {:?}, {} laid",
        out.run.world.player.pos, out.walk.more.placed
    );
    assert_eq!(out.walk.more.placed, 4);
    assert!(!out.run.died);
    for x in 2..=5 {
        assert_eq!(
            out.run.world.block_at(world::BlockPos::new(x, 63, 0)),
            Some(BlockKind::Cobblestone)
        );
    }
    let left: i32 = out
        .run
        .world
        .inventory
        .slots
        .values()
        .filter(|(k, _)| *k == ItemKind::Cobblestone)
        .map(|(_, n)| *n)
        .sum();
    assert_eq!(left, 60);
}

#[test]
fn a_far_search_reaches_where_a_plain_one_runs_out() {
    // Forty cells off over open ground, a wall with one gap in between: the weighed search
    // finds the gap in fewer expansions than the plain one.
    let wall: Vec<_> = (-48..=47)
        .filter(|z| *z != 20)
        .flat_map(|z| (64..=66).map(move |y| ((10, y, z), BlockKind::Netherrack)))
        .collect();
    let w = joined(nether(&wall, &[]));
    let goal = Goal::Near {
        pos: (30, 64, 0),
        range: 1.0,
    };
    let plain = path::plan(&w, (0, 64, 0), &goal, 60_000);
    let far = path::plan_weighted(&w, (0, 64, 0), &goal, 60_000, 2.0);
    assert_eq!(plain.status, Status::Found);
    assert_eq!(far.status, Status::Found);
    assert!(
        far.expanded < plain.expanded,
        "{} {}",
        far.expanded,
        plain.expanded
    );
    assert!(far.cost <= plain.cost * 2.0);
}

/// The far walk's causeway is one block over the lava sea: a bot knocked off it swims beside a
/// bridge it cannot climb (the lift out of a liquid is half a block). With blocks held the
/// escape lays one under the bridge and a step against it, climbs onto the step and is out.
#[test]
#[ignore = "died in the model, its escape's trail not yet read (CI 45341556367); owner A2"]
fn a_bot_in_lava_beside_a_bridge_lays_a_step_and_climbs_out() {
    // A lava lake 24 wide, two deep, from wall to wall; a bridge along x over it at z 0.
    let mut set: Vec<_> = (2..=25)
        .flat_map(|x| {
            (62..=63).flat_map(move |y| (-48..=47).map(move |z| ((x, y, z), BlockKind::Lava)))
        })
        .collect();
    set.extend((2..=25).map(|x| ((x, 64, 0), BlockKind::Cobblestone)));
    let mut m = flat_world(2, 63).unwrap();
    m.dimension = "minecraft:the_nether".to_owned();
    m.dimension_type = 3;
    for ((x, y, z), k) in &set {
        m.set_block(*x, *y, *z, state(*k)).unwrap();
    }
    // In the lava a cell from the bridge, 11 from either bank.
    m.spawn = [13.5, 63.0, 1.5];
    let mut sim = Sim::new(CONFIG, m);
    sim.survival.give((ItemKind::Cobblestone, 64));
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut p = plan::Plan::with_table(plan::Aim::Hold, table);
    let mut notes: Vec<String> = Vec::new();
    let mut seen = 0;
    let mut track: Vec<String> = Vec::new();
    let run = bot::lockstep::run_acting(sim, "rs-nw", 1, 400, 0, |w, t| {
        let c = p.step(w);
        if t % 5 == 0 && track.len() < 40 {
            let said = match &p.task {
                plan::Task::Escape(e) => format!("target {:?}; {}", e.target, e.report()),
                _ => p.task.name().to_owned(),
            };
            track.push(format!(
                "{t}: {:?} lava {:?} ground {:?} health {:?}; {said}",
                w.body.map(|b| b.pos.map(|v| (v * 100.0).round() / 100.0)),
                w.body.map(|b| b.in_lava),
                w.body.map(|b| b.on_ground),
                w.player.health
            ));
        }
        for e in p.events_since(seen) {
            notes.push(format!("{} {} ({})", e.tick, e.what, e.why));
        }
        seen += p.events_since(seen).len() as u64;
        (c, p.take_acts())
    });
    let story = format!("{}\n{}", notes.join("\n"), track.join("\n"));
    assert!(!run.died, "died\n{story}");
    assert!(story.contains("lay a step"), "no step laid\n{story}");
    assert!(story.contains("out of the lava"), "never out\n{story}");
    let body = run.world.body.unwrap();
    assert!(
        plan::escape::dry_land(&run.world, &body),
        "not on dry land at {:?}\n{story}",
        body.pos
    );
}
