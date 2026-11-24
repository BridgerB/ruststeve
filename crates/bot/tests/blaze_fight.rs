//! The blaze fight (docs/problems/blaze-fight.md walls 2 and 4): the reach a strike is measured
//! by and the steps a shooter's fight may take, on a hand-built world.

use data::BlockKind;
use sim::{Config, Sim, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

fn state(k: BlockKind) -> u32 {
    k.info().unwrap().default_state
}

/// The bot stands at (0.5, 64, 0.5) on a flat floor at y 63: fire one cell east, lava in the floor
/// one cell north, a hole four deep two cells west, and open floor south.
fn hazards() -> world::World {
    let mut m = flat_world(2, 63).unwrap();
    m.set_block(1, 64, 0, state(BlockKind::Fire)).unwrap();
    m.set_block(0, 63, -1, state(BlockKind::Lava)).unwrap();
    m.set_block(-2, 63, 0, state(BlockKind::Air)).unwrap();
    m.set_block(-2, 62, 0, state(BlockKind::Air)).unwrap();
    m.set_block(-2, 61, 0, state(BlockKind::Air)).unwrap();
    m.set_block(-2, 60, 0, state(BlockKind::Air)).unwrap();
    bot::lockstep::run(Sim::new(CONFIG, m), "rs-bf", 1, 30, 20, |_, _| {
        physics::Controls::default()
    })
    .world
}

#[test]
fn a_shooters_fight_steps_onto_floor_only() {
    let w = hazards();
    let pos = [0.5, 64.0, 0.5];
    // Yaw as the game counts it: 0 south (+z), 90 west (-x), 180 north, 270 east.
    assert!(plan::defend::safe_step(&w, pos, 0.0), "open floor south");
    assert!(!plan::defend::safe_step(&w, pos, 270.0), "fire east");
    assert!(!plan::defend::safe_step(&w, pos, 180.0), "lava floor north");
    assert!(!plan::defend::safe_step(&w, pos, 90.0), "a hole west");
}

#[test]
fn reach_is_measured_to_the_box_not_the_middle() {
    // An eye at 62.62 and a blaze standing on a floor at 64, 2 blocks off: its feet are 2.4 away
    // (in reach), its middle 3.0 (not, by the old measure).
    let eye = [0.5, 62.62, 0.5];
    let blaze = [2.8, 64.0, 0.5];
    let d = plan::defend::box_distance(eye, blaze);
    assert!(d < 2.5 && d > 2.3, "{d}");
    // Inside the box the distance is 0.
    assert!(plan::defend::box_distance([2.8, 64.9, 0.5], blaze) < 1e-9);
}

/// Wall 2: a blaze hovering with its feet 4 over the bot's and a block aside is in reach by its
/// box (2.5 from the eye) and out of it by its middle (3.4): the fight strikes it where it stands.
#[test]
fn a_blaze_overhead_is_struck_by_its_box() {
    use data::{EntityKind, ItemKind};
    use plan::{Aim, Plan};
    let config = Config {
        peaceful: false,
        ..CONFIG
    };
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut p = Plan::with_table(Aim::Hold, table);
    let mut m = flat_world(2, 63).unwrap();
    m.spawn = [0.5, 64.0, 0.5];
    let mut sim = Sim::new(config, m).seeded(3);
    sim.health = 20.0;
    sim.survival.give((ItemKind::IronSword, 1));
    sim.summon(EntityKind::Blaze, [1.5, 68.0, 0.5]);
    let run = bot::lockstep::run_acting(sim, "rs-bf", 1, 400, 20, |w, _| {
        let c = p.step(w);
        (c, p.take_acts())
    });
    assert!(run.world.kills >= 1, "no kill: {:#?}", p.events);
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}

/// A cell beside a fall is no cover (a bot took cover on a bridge rail's top and fell from it):
/// the cell beside the hole is an edge, one on open floor is not.
#[test]
fn a_cell_beside_a_fall_is_an_edge() {
    let w = hazards();
    assert!(plan::defend::edge(&w, (-1, 64, 0)), "beside the hole");
    assert!(!plan::defend::edge(&w, (0, 64, 4)), "open floor");
}
