//! The blaze fight (docs/problems/blaze-fight.md walls 2 and 3): what a shooter sees, the cover a
//! retreat from one walks to, and the reach a strike is measured by, on a hand-built world.

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
    bot::lockstep::run(Sim::new(CONFIG, m), "rs-bf", 1, 30, 20, |_, _| {
        physics::Controls::default()
    })
    .world
}

#[test]
fn a_shooter_sees_the_open_and_not_behind_a_full_block() {
    let w = walled();
    // A blaze's eye east of the wall.
    let eyes = [[8.5, 65.5, 0.5]];
    assert!(
        !plan::defend::seen_by(&w, &eyes, (1, 64, 0)),
        "behind the wall"
    );
    assert!(plan::defend::seen_by(&w, &eyes, (5, 64, 0)), "in the open");
}

#[test]
fn cover_from_a_shooter_is_the_near_side_of_the_wall() {
    let w = walled();
    // A blaze's eye west of the bot: the bot at (0, 64, 0) is seen, and the nearest cells it
    // does not see are east of the wall.
    let eyes = [[-6.5, 65.5, 0.5]];
    assert!(plan::defend::seen_by(&w, &eyes, (0, 64, 0)));
    let cells = plan::defend::cover_cells(&w, &eyes);
    let first = cells.first().copied().expect("a cover cell");
    assert!(first.0 >= 4, "east of the wall: {first:?}");
    assert!(
        cells.iter().all(|c| !plan::defend::seen_by(&w, &eyes, *c)),
        "every cover cell is unseen"
    );
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
