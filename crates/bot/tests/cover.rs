//! Issue 16 (blaze wall 3): a cell behind a wall is out of a shooter's sight, a cell in the open is
//! not (`plan::cover`). Checked on a flat model world the bot has joined.

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

/// The bot stands at (0.5, 64, 0.5); a stone wall three high runs along x = 3, z -3 to 3.
fn walled() -> world::World {
    let mut m = flat_world(2, 63).unwrap();
    for y in 64..=66 {
        for z in -3..=3 {
            m.set_block(3, y, z, state(BlockKind::Stone)).unwrap();
        }
    }
    bot::lockstep::run(Sim::new(CONFIG, m), "rs-cv", 1, 30, 20, |_, _| {
        physics::Controls::default()
    })
    .world
}

#[test]
fn a_cell_behind_a_wall_is_out_of_sight() {
    let w = walled();
    // A blaze's eye east of the wall, at (8.5, 65.2, 0.5).
    let eyes = [[8.5, 65.2, 0.5]];
    assert!(
        !plan::cover::seen_at(&w, &eyes, (1, 64, 0)),
        "behind the wall"
    );
    assert!(plan::cover::seen_at(&w, &eyes, (5, 64, 0)), "in the open");
}
