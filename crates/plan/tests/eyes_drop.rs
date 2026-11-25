//! Issue 19c (docs/problems/eyes-throw.md): the column a thrown eye drops through, on hand-built
//! overworld worlds. Q5's parity 45688030470 put the drop 12 out on the bearing and about 9 over
//! the feet; a canopy or a cliff in that column holds the eye out of reach.

use std::sync::Arc;

use data::BlockKind;
use plan::eyes::{EyeRules, apex_blocked};
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
