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
/// one cell north, a hole two deep two cells west, and open floor south.
fn hazards() -> world::World {
    let mut m = flat_world(2, 63).unwrap();
    m.set_block(1, 64, 0, state(BlockKind::Fire)).unwrap();
    m.set_block(0, 63, -1, state(BlockKind::Lava)).unwrap();
    m.set_block(-2, 63, 0, state(BlockKind::Air)).unwrap();
    m.set_block(-2, 62, 0, state(BlockKind::Air)).unwrap();
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
