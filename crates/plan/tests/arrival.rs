//! Issue 19, the overworld arrival (agent Q7, docs/problems/eyes-arrival.md), on hand-built
//! worlds: the shaft straight up digs the cell over the head in rock, refuses a cave, a fluid or a
//! falling block over or beside it, rises into a cell it opened, and is out under the sky with a
//! side to walk off by; the way home's climb begins a shaft from under rock and digs.

use std::sync::Arc;

use data::{BlockKind, ItemKind};
use plan::arrival::{self, Step};
use plan::home;
use plan::{Aim, Plan, make};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use world::{Act, Event, World};

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

/// Stone from y 0 to 95, air over it, 7 by 7 chunk columns round the origin, the feet at `pos`.
fn world(pos: [f64; 3]) -> World {
    let mut sections = vec![section(BlockKind::Stone); 6];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 10));
    let col = Arc::new(Column { min_y: 0, sections });
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 0,
        dimension: home::OVERWORLD.to_owned(),
        game_type: 0,
    }));
    for x in -3..=3 {
        for z in -3..=3 {
            w.step(&Event::Packet(S2c::LevelChunkWithLight {
                x,
                z,
                column: Arc::clone(&col),
            }));
        }
    }
    w.body = Some(physics::Body {
        pos,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(pos);
    w.player.health = Some(19.8);
    w.player.food = Some(10);
    w
}

fn set(w: &mut World, c: (i32, i32, i32), k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: c,
        state: state(k),
    }));
}

/// The feet in a cell cut at (8, 40, 8) in the rock.
fn in_rock() -> World {
    let mut w = world([8.5, 40.0, 8.5]);
    for y in 40..=41 {
        set(&mut w, (8, y, 8), BlockKind::Air);
    }
    w
}

const PICK: [ItemKind; 1] = [ItemKind::StonePickaxe];

#[test]
fn in_rock_the_shaft_digs_over_the_head() {
    let w = in_rock();
    assert_eq!(
        arrival::step(&w, (8, 40, 8), None, &PICK),
        Step::Dig((8, 42, 8))
    );
}

#[test]
fn the_shaft_refuses_a_cave_fluid_or_falling_block() {
    let refused = |w: &World| matches!(arrival::step(w, (8, 40, 8), None, &PICK), Step::Refused(_));
    // A cave beside the cell over the head.
    let mut w = in_rock();
    set(&mut w, (9, 42, 8), BlockKind::CaveAir);
    assert!(
        refused(&w),
        "{:?}",
        arrival::step(&w, (8, 40, 8), None, &PICK)
    );
    // Lava over it.
    let mut w = in_rock();
    set(&mut w, (8, 43, 8), BlockKind::Lava);
    assert!(refused(&w));
    // Water beside it.
    let mut w = in_rock();
    set(&mut w, (8, 42, 7), BlockKind::Water);
    assert!(refused(&w));
    // Gravel over it falls into the shaft.
    let mut w = in_rock();
    set(&mut w, (8, 43, 8), BlockKind::Gravel);
    assert!(refused(&w));
    // A cave over the head that this shaft did not open.
    let mut w = in_rock();
    set(&mut w, (8, 42, 8), BlockKind::CaveAir);
    assert!(refused(&w));
}

#[test]
fn the_shaft_rises_into_the_cell_it_opened() {
    let mut w = in_rock();
    set(&mut w, (8, 42, 8), BlockKind::Air);
    assert_eq!(
        arrival::step(&w, (8, 40, 8), Some((8, 42, 8)), &PICK),
        Step::Rise
    );
}

#[test]
fn under_the_sky_with_a_side_to_walk_off_the_shaft_is_out() {
    // The top stone is y 95: two under it the sky is over the head but no side is walkable, so
    // the shaft rises; at y 95 a side's stone has room over it to jump onto.
    let mut w = world([8.5, 94.0, 8.5]);
    for y in 94..=95 {
        set(&mut w, (8, y, 8), BlockKind::Air);
    }
    assert_eq!(arrival::step(&w, (8, 94, 8), None, &PICK), Step::Rise);
    // A block put into (8, 94, 8): the feet at y 95, level with the top stone's top face.
    set(&mut w, (8, 94, 8), BlockKind::Cobblestone);
    assert_eq!(arrival::step(&w, (8, 95, 8), None, &PICK), Step::Out);
}

#[test]
fn the_climb_from_under_rock_begins_a_shaft_and_digs() {
    let mut w = in_rock();
    w.inventory.slots.insert(36, (ItemKind::IronSword, 1));
    w.inventory.slots.insert(37, (ItemKind::StonePickaxe, 1));
    w.inventory
        .slots
        .insert(38, (ItemKind::CobbledDeepslate, 64));
    w.inventory.slots.insert(9, (ItemKind::EnderEye, 14));
    w.held = 0;
    w.stage_reached = Some(world::stage::Stage::Pearls12);
    let mut p = Plan::with_table(Aim::Race, table());
    let mut acts = Vec::new();
    for t in 100..140 {
        w.ticks = t;
        p.step(&w);
        acts.extend(p.take_acts());
    }
    let notes: Vec<String> = p.events.iter().map(|e| e.what.clone()).collect();
    assert!(
        notes
            .iter()
            .any(|n| n.starts_with("arrival: shaft up from (8, 40, 8)")),
        "{notes:?}"
    );
    // The pickaxe to hand, then the dig of the cell over the head.
    assert!(
        acts.iter()
            .any(|a| matches!(a, Act::Hold { .. } | Act::Click { .. } | Act::Dig { .. })),
        "{acts:?}"
    );
}

fn table() -> Arc<make::Table> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/goals.toml"
    ))
    .unwrap_or_default();
    let goals: make::Goals = toml::from_str(&text).unwrap_or_else(|e| panic!("goals.toml: {e}"));
    Arc::new(make::Table::new(&goals).unwrap_or_else(|e| panic!("table: {e}")))
}

#[test]
fn gravel_over_the_head_steps_aside_to_rock() {
    let mut w = in_rock();
    set(&mut w, (8, 42, 8), BlockKind::Gravel);
    assert!(matches!(
        arrival::step(&w, (8, 40, 8), None, &PICK),
        Step::Refused(_)
    ));
    assert_eq!(arrival::side_cell(&w, (8, 40, 8), &PICK), Some((9, 40, 8)));
    // Gravel over the east side too: another side.
    set(&mut w, (9, 42, 8), BlockKind::Gravel);
    assert_eq!(arrival::side_cell(&w, (8, 40, 8), &PICK), Some((7, 40, 8)));
    // A cave beside the side cell: not that side.
    set(&mut w, (6, 41, 8), BlockKind::CaveAir);
    let c = arrival::side_cell(&w, (8, 40, 8), &PICK);
    assert!(c != Some((7, 40, 8)) && c.is_some(), "{c:?}");
}
