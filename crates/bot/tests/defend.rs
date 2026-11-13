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
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut p = Plan::with_table(Aim::Hold, table);
    let mut m = flat_world(2, 63).unwrap();
    m.spawn = [0.5, 64.0, 0.5];
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
