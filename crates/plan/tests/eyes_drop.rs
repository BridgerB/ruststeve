//! Issue 19c (docs/problems/eyes-throw.md): the column a thrown eye drops through, on hand-built
//! overworld worlds. Q5's parity 45688030470 put the drop 12 out on the bearing and about 9 over
//! the feet; a canopy or a cliff in that column holds the eye out of reach.

use std::sync::Arc;

use data::BlockKind;
use plan::eyes::{EyeRules, apex_blocked, first_blocked, open_side, ring_blocked, trees_near};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use world::{Event, World};

/// The feet: on a stone floor that tops out at y 63.
const FEET: [f64; 3] = [8.5, 64.0, 8.5];
/// East.
const EAST: [f64; 2] = [1.0, 0.0];

fn state(k: BlockKind) -> u32 {
    k.info().map(|i| i.default_state).unwrap_or(0)
}

fn section(k: BlockKind) -> Section {
    Section {
        block_count: if k == BlockKind::Air { 0 } else { 4096 },
        fluid_count: 0,
        blocks: Paletted::Single(state(k)),
        biomes: Paletted::Single(0),
    }
}

/// Stone to y 63, air above.
fn plain() -> World {
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 0,
        dimension: "minecraft:overworld".to_owned(),
        game_type: 0,
    }));
    let mut sections = vec![section(BlockKind::Stone); 4];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 12));
    let col = Arc::new(Column { min_y: 0, sections });
    for x in -3..=3 {
        for z in -3..=3 {
            w.step(&Event::Packet(S2c::LevelChunkWithLight {
                x,
                z,
                column: Arc::clone(&col),
            }));
        }
    }
    w
}

fn set(w: &mut World, c: (i32, i32, i32), k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: c,
        state: state(k),
    }));
}

#[test]
fn an_open_plain_lets_the_eye_down() {
    assert_eq!(
        apex_blocked(&plain(), FEET, EAST, &EyeRules::default()),
        None
    );
}

#[test]
fn a_canopy_where_the_eye_comes_down_blocks_the_throw() {
    let mut w = plain();
    // 12 out east: x 20. Leaves at feet +4, where the parity's third drop came to rest.
    set(&mut w, (20, 68, 8), BlockKind::OakLeaves);
    assert_eq!(
        apex_blocked(&w, FEET, EAST, &EyeRules::default()),
        Some((20, 68, 8))
    );
    // The same canopy off the bearing (west) is no matter.
    assert_eq!(
        apex_blocked(&w, FEET, [-1.0, 0.0], &EyeRules::default()),
        None
    );
}

#[test]
fn a_cliff_blocks_from_the_feet_level_and_the_sky_over_the_drop_does_not() {
    let mut w = plain();
    for y in 64..=70 {
        set(&mut w, (20, y, 8), BlockKind::Stone);
    }
    assert_eq!(
        apex_blocked(&w, FEET, EAST, &EyeRules::default()),
        Some((20, 64, 8))
    );
    // A block over the drop point (64 + 9.1 tops out at y 73) is above the eye's fall.
    let mut high = plain();
    set(&mut high, (20, 75, 8), BlockKind::Stone);
    assert_eq!(apex_blocked(&high, FEET, EAST, &EyeRules::default()), None);
}

#[test]
fn no_bearing_checks_nothing() {
    let mut w = plain();
    set(&mut w, (20, 68, 8), BlockKind::OakLeaves);
    assert_eq!(
        apex_blocked(&w, FEET, [0.0, 0.0], &EyeRules::default()),
        None
    );
}

#[test]
fn the_first_throw_sees_a_tree_near_and_walks_to_the_open_side() {
    let r = EyeRules::default();
    // Open plain: nothing near.
    assert_eq!(trees_near(&plain(), FEET, &r), None);
    // Leaves 4 east at feet +3, and more to the east: the walk goes to a side with none near.
    let mut w = plain();
    for (x, z) in [(12, 8), (13, 9), (14, 12), (12, 12), (20, 8), (22, 14)] {
        set(&mut w, (x, 67, z), BlockKind::OakLeaves);
    }
    assert_eq!(trees_near(&w, FEET, &r), Some((12, 67, 8)));
    let to = open_side(&w, FEET, &r);
    assert!(to.0 <= 8, "{to:?}");
    let there = [f64::from(to.0) + 0.5, FEET[1], f64::from(to.2) + 0.5];
    assert_eq!(trees_near(&w, there, &r), None, "{to:?}");
    // A log at the feet's own level is not over it: only blocks above the feet count.
    let mut low = plain();
    set(&mut low, (12, 64, 8), BlockKind::OakLog);
    assert_eq!(trees_near(&low, FEET, &r), None);
    // Past 6 on the plane is no matter.
    let mut far = plain();
    set(&mut far, (15, 67, 8), BlockKind::OakLeaves);
    assert_eq!(trees_near(&far, FEET, &r), None);
}

/// Q4's race 45698008920: throw 1's eye rested 9 up on a canopy about 12 out, past the 6 checked
/// for trees. The ring catches it, and the walk goes where no side of the ring is blocked.
#[test]
fn a_canopy_12_east_blocks_throw_1_and_the_walk_leaves_it() {
    let r = EyeRules::default();
    assert!(ring_blocked(&plain(), FEET, &r).is_empty());
    assert_eq!(first_blocked(&plain(), FEET, &r), None);
    let mut w = plain();
    // A canopy at 12 east (x 20), 9 over the feet, 3 wide: nothing within 6.
    for z in 7..=9 {
        set(&mut w, (20, 73, z), BlockKind::OakLeaves);
    }
    assert_eq!(trees_near(&w, FEET, &r), None);
    assert_eq!(ring_blocked(&w, FEET, &r), vec![(20, 73, 8)]);
    assert_eq!(first_blocked(&w, FEET, &r), Some((20, 73, 8)));
    let to = open_side(&w, FEET, &r);
    let there = [f64::from(to.0) + 0.5, FEET[1], f64::from(to.2) + 0.5];
    assert!(ring_blocked(&w, there, &r).is_empty(), "{to:?}");
    assert_eq!(first_blocked(&w, there, &r), None, "{to:?}");
}
