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

fn joined(m: sim::ModelWorld) -> world::World {
    bot::lockstep::run(Sim::new(CONFIG, m), "rs-cv", 1, 30, 20, |_, _| {
        physics::Controls::default()
    })
    .world
}

fn with_mob(mut w: world::World, id: i32, kind: data::EntityKind, at: [f64; 3]) -> world::World {
    w.step(&world::Event::Packet(protocol::S2c::AddEntity {
        id,
        uuid: [0; 16],
        kind: kind.id(),
        x: at[0],
        y: at[1],
        z: at[2],
    }));
    w
}

/// Spawner wall 3 (race 45335627948): three hurt bots rested "out of sight" on an open bridge
/// with blazes 16 to 24 blocks off, and a piglin's crossbow 5 to 7 off, and were shot dead. A
/// blaze counts from 48 blocks, a piglin from 16.
#[test]
fn a_blaze_twenty_blocks_off_is_a_shooter() {
    let w = joined(flat_world(2, 63).unwrap());
    let w = with_mob(w, 9, data::EntityKind::Blaze, [20.5, 66.0, 0.5]);
    let w = with_mob(w, 10, data::EntityKind::Piglin, [0.5, 64.0, 6.5]);
    let w = with_mob(w, 11, data::EntityKind::Piglin, [0.5, 64.0, 30.5]);
    let w = with_mob(w, 12, data::EntityKind::Blaze, [60.5, 66.0, 0.5]);
    let eyes = plan::cover::shooters(&w);
    assert_eq!(eyes.len(), 2, "the blaze at 20 and the piglin at 6");
    let pos = w.body.unwrap().pos;
    assert!(plan::cover::seen_body(&w, &eyes, pos), "on open ground");
}

/// A bot seen on open ground builds its cover: the block that stops the line to its middle
/// first, then the one over it for the line to its eye, each clicked on the block under it;
/// behind both it is out of sight.
#[test]
fn a_shelter_is_built_where_the_line_of_sight_comes_in() {
    // A shooter's eye level with the bot's, eight blocks east.
    let eyes = [[8.5, 65.0, 0.5]];
    let w = joined(flat_world(2, 63).unwrap());
    let pos = w.body.unwrap().pos;
    assert_eq!(
        plan::cover::shelter_block(&w, &eyes, pos),
        Some(((1, 64, 0), (1, 63, 0), 1)),
        "the head's line comes in over a cell with nothing under it: the foot of the wall first"
    );
    let mut m = flat_world(2, 63).unwrap();
    m.set_block(1, 64, 0, state(BlockKind::Stone)).unwrap();
    let w = joined(m);
    assert_eq!(
        plan::cover::shelter_block(&w, &eyes, pos),
        Some(((1, 65, 0), (1, 64, 0), 1))
    );
    let mut m = flat_world(2, 63).unwrap();
    m.set_block(1, 64, 0, state(BlockKind::Stone)).unwrap();
    m.set_block(1, 65, 0, state(BlockKind::Stone)).unwrap();
    let w = joined(m);
    assert!(!plan::cover::seen_body(&w, &eyes, pos));
    assert_eq!(plan::cover::shelter_block(&w, &eyes, pos), None);
}

/// A blaze overhead: the roof cell has nothing to click until a wall stands three high beside
/// the head, so the wall's foot comes first and the roof last.
#[test]
fn a_roof_hangs_on_a_wall_three_high() {
    let eyes = [[0.5, 75.0, 0.5]];
    let w = joined(flat_world(2, 63).unwrap());
    let pos = w.body.unwrap().pos;
    assert_eq!(
        plan::cover::shelter_block(&w, &eyes, pos),
        Some(((1, 64, 0), (1, 63, 0), 1))
    );
    let mut m = flat_world(2, 63).unwrap();
    for y in 64..=66 {
        m.set_block(1, y, 0, state(BlockKind::Stone)).unwrap();
    }
    let w = joined(m);
    assert_eq!(
        plan::cover::shelter_block(&w, &eyes, pos),
        Some(((0, 66, 0), (1, 66, 0), 4)),
        "the roof, on the west face of the wall's top"
    );
}

/// A spawner runs with a player within 16 blocks: a rest 14 to 17 from the blaze spawner saw its
/// shooters go from 3 to 12 (races 45337168104 and 45338756013). Rest is 18 or more from one.
#[test]
fn a_spawner_within_eighteen_blocks_is_awake() {
    let spawner = (129, 64, 159);
    assert!(plan::cover::awake_at(spawner, [129.5, 61.0, 175.5]));
    assert!(!plan::cover::awake_at(spawner, [129.5, 61.0, 178.5]));
}

/// A cover cell is one a body standing in it is not seen in, by the test the rest is judged
/// by: behind the wall yes, in the open no, and never the top of the wall.
#[test]
fn cover_cells_are_out_of_sight_for_a_standing_body() {
    let w = walled();
    // A shooter's eye east of the wall, a little over it.
    let eyes = [[8.5, 66.5, 0.5]];
    let cells = plan::cover::cover_cells(&w, &eyes);
    assert!(
        cells.contains(&(2, 64, 0)),
        "right behind the wall: {cells:?}"
    );
    assert!(!cells.contains(&(5, 64, 0)), "in the open");
    assert!(!cells.contains(&(3, 67, 0)), "on the wall");
    for c in &cells {
        let pos = [f64::from(c.0) + 0.5, f64::from(c.1), f64::from(c.2) + 0.5];
        assert!(!plan::cover::seen_body(&w, &eyes, pos), "{c:?}");
    }
}
