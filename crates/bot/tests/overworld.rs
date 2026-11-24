//! The overworld leg (docs/problems/overworld-leg.md wall 1): the lava hunt's own stairs take the
//! heading whose cut holds the fewest open cells, so a cave seen that way turns them.

use data::BlockKind;
use plan::{SPIRAL, leg_cells, own_leg};
use sim::{Config, Sim, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

/// The bot on the grass at (0.5, 64, 0.5) of a flat world, with `set` changed.
fn world_with(set: &[((i32, i32, i32), BlockKind)]) -> world::World {
    let mut m = flat_world(2, 63).unwrap();
    for ((x, y, z), k) in set {
        m.set_block(*x, *y, *z, k.info().unwrap().default_state)
            .unwrap();
    }
    let run = bot::lockstep::run(Sim::new(CONFIG, m), "rs-ow", 1, 30, 20, |_, _| {
        physics::Controls::default()
    });
    run.world
}

#[test]
fn a_leg_opens_three_cells_a_step() {
    let cells = leg_cells((0, 64, 0), (1, 0));
    assert_eq!(cells.len(), 18);
    assert_eq!(&cells[..3], &[(1, 65, 0), (1, 64, 0), (1, 63, 0)]);
    assert_eq!(cells[17], (6, 58, 0));
}

#[test]
fn own_stairs_turn_from_a_cave_seen_open() {
    let feet = (0, 64, 0);
    // Every heading alike on open ground: the first preferred wins.
    let plain = world_with(&[]);
    let (dir, open) = own_leg(&plain, feet, &SPIRAL);
    assert_eq!(dir, (1, 0));
    // A cave under the ground to the east, in the leg's way: the next heading is taken, and it
    // holds fewer open cells than the east one does now.
    let cave: Vec<_> = (3..=5)
        .flat_map(|x| (59..=61).map(move |y| ((x, y, 0), BlockKind::Air)))
        .collect();
    let caved = world_with(&cave);
    let (turned, fewer) = own_leg(&caved, feet, &SPIRAL);
    assert_eq!(turned, (0, 1), "the leg did not turn from the cave");
    assert_eq!(fewer, open, "the turned leg is as closed as plain ground");
    // Told to head east first for a pool, it still turns.
    let (toward, _) = own_leg(&caved, feet, &[(1, 0), (-1, 0), (0, 1), (0, -1)]);
    assert_eq!(toward, (-1, 0));
}
