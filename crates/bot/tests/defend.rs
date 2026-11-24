//! Issue 12d: the `defend` always-row against the model on a flat world: a zombie within four
//! blocks is fought with a stone sword, swings paced by its cooldown; under the danger line, and
//! against a creeper at any health, the bot backs away instead
//! (brain/ruststeve/design/combat-and-pacing.md). Mobs in the model stand still and do not hit.

use data::{EntityKind, ItemKind};
use plan::{Aim, Plan};
use sim::{Config, Sim, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: false,
};

fn run(health: f32, mob: EntityKind, ticks: u64) -> (Plan, bot::lockstep::Run, i32) {
    run_in(health, mob, ticks, &[])
}

fn run_in(
    health: f32,
    mob: EntityKind,
    ticks: u64,
    set: &[((i32, i32, i32), data::BlockKind)],
) -> (Plan, bot::lockstep::Run, i32) {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut p = Plan::with_table(Aim::Hold, table);
    let mut m = flat_world(2, 63).unwrap();
    m.spawn = [0.5, 64.0, 0.5];
    for ((x, y, z), k) in set {
        m.set_block(*x, *y, *z, k.info().unwrap().default_state)
            .unwrap();
    }
    let mut sim = Sim::new(CONFIG, m).seeded(3);
    sim.health = health;
    sim.survival.give((ItemKind::StoneSword, 1));
    let id = sim.summon(mob, [2.5, 64.0, 0.5]);
    let run = bot::lockstep::run_acting(sim, "rs-def", 1, ticks, 20, |w, _| {
        let c = p.step(w);
        (c, p.take_acts())
    });
    (p, run, id)
}

#[test]
fn a_zombie_near_is_fought_and_killed() {
    let (p, run, _) = run(20.0, EntityKind::Zombie, 400);
    assert!(run.world.kills >= 1, "no kill: {:#?}", p.events);
    assert!(
        p.events.iter().any(|e| e.what.starts_with("defend Zombie")),
        "{:#?}",
        p.events
    );
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}

#[test]
fn under_the_danger_line_it_backs_away_instead() {
    let (p, run, _) = run(6.0, EntityKind::Zombie, 200);
    assert_eq!(
        run.world.kills, 0,
        "fought under the danger line: {:#?}",
        p.events
    );
    let x = run.world.body.unwrap().pos[0];
    assert!(x < -3.0, "did not back away: x {x} {:#?}", p.events);
}

#[test]
fn a_creeper_is_never_fought() {
    let (p, run, _) = run(20.0, EntityKind::Creeper, 200);
    assert_eq!(run.world.kills, 0, "fought a creeper: {:#?}", p.events);
    assert!(run.world.body.unwrap().pos[0] < -3.0, "{:#?}", p.events);
}

#[test]
fn a_creeper_is_fled_round_a_tunnel_corner() {
    // An L-shaped tunnel two high in stone: the creeper east of the bot along one arm, the bot at
    // the corner, a wall straight west of it; the way out is the arm going south (wall 28).
    use data::BlockKind::{Air, Stone};
    let mut set = Vec::new();
    for x in -4..=6 {
        for z in -4..=24 {
            for y in 64..=66 {
                set.push(((x, y, z), Stone));
            }
        }
    }
    for y in 64..=65 {
        for x in 0..=4 {
            set.push(((x, y, 0), Air));
        }
        for z in 0..=22 {
            set.push(((0, y, z), Air));
        }
    }
    let (p, run, _) = run_in(20.0, EntityKind::Creeper, 400, &set);
    assert_eq!(run.world.kills, 0, "fought a creeper: {:#?}", p.events);
    let z = run.world.body.unwrap().pos[2];
    assert!(
        z > 10.0,
        "did not flee down the tunnel: z {z} {:#?}",
        p.events
    );
}

/// Never fight under danger health: in a dead end with the zombie in its face, the retreat of a
/// bot under the danger line stays a retreat and sends no swing.
#[test]
fn a_cornered_retreat_under_the_danger_line_never_strikes() {
    use data::BlockKind::Stone;
    // A corridor one wide and two long, closed at both ends: the bot at x 0, the zombie at x 2.
    let mut set = Vec::new();
    for y in 64..=65 {
        for x in -1..=3 {
            set.push(((x, y, -1), Stone));
            set.push(((x, y, 1), Stone));
        }
        set.push(((-1, y, 0), Stone));
        set.push(((3, y, 0), Stone));
    }
    let (p, run, _) = run_in(6.0, EntityKind::Zombie, 300, &set);
    assert!(
        p.events
            .iter()
            .any(|e| e.what.starts_with("defend Zombie") && e.what.contains("Retreat")),
        "{:#?}",
        p.events
    );
    assert_eq!(p.swings_sent, 0, "struck under the danger line");
    assert_eq!(run.world.kills, 0);
}
