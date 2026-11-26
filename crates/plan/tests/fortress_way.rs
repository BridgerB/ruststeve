//! Issue 19, the way home's fortress stretch (docs/problems/eyes-fortress.md), on hand-built
//! worlds: the guard never aims at a cell the body's box is in, and no fortress turn while the way
//! steers home.

use std::sync::Arc;

use data::{BlockKind, ItemKind};
use plan::endermen::EnderPhase;
use plan::fortress_way;
use plan::home;
use plan::{Aim, Plan, make};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use world::{Event, World};

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

fn table() -> make::Table {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/goals.toml"
    ))
    .unwrap_or_default();
    let goals: make::Goals = toml::from_str(&text).unwrap_or_else(|e| panic!("goals.toml: {e}"));
    make::Table::new(&goals).unwrap_or_else(|e| panic!("table: {e}"))
}

/// The Nether: netherrack to y 63 in 7 by 7 chunk columns round the origin, the feet at `pos`.
fn nether(pos: [f64; 3]) -> World {
    let mut sections = vec![section(BlockKind::Netherrack); 4];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 12));
    let col = Arc::new(Column { min_y: 0, sections });
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 3,
        dimension: home::NETHER.to_owned(),
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
    w.inventory.slots.insert(36, (ItemKind::IronSword, 1));
    w.inventory.slots.insert(37, (ItemKind::StonePickaxe, 1));
    w.inventory.slots.insert(38, (ItemKind::Netherrack, 64));
    w.inventory.slots.insert(9, (ItemKind::EnderEye, 14));
    w.held = 0;
    w
}

fn set(w: &mut World, c: (i32, i32, i32), k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: c,
        state: state(k),
    }));
}

fn remember(w: &mut World, c: (i32, i32, i32), k: BlockKind) {
    let seen = memory::Seen::check(home::NETHER, c, state(k), [Some(0); 6])
        .unwrap_or_else(|| panic!("seen"));
    w.memory.record(seen, 1);
}

/// A one-wide line along x at y 63: the cells beside it at z 7 and 9 open 3 down.
fn line(pos: [f64; 3]) -> World {
    let mut w = nether(pos);
    for x in 0..=16 {
        for z in [7, 9] {
            for y in 61..=63 {
                set(&mut w, (x, y, z), BlockKind::Air);
            }
        }
    }
    w
}

#[test]
fn the_body_box_is_in_a_cell_it_overhangs() {
    // Race 45760537530 lane 0: x 98.8 is a box to 99.1, so (99, 73, 182) is the body's own.
    let w = nether([98.8, 73.0, 182.6]);
    assert!(fortress_way::body_in_cell(&w, (99, 73, 182)));
    assert!(fortress_way::body_in_cell(&w, (98, 74, 182)), "the head");
    assert!(!fortress_way::body_in_cell(&w, (97, 73, 182)));
    assert!(!fortress_way::body_in_cell(&w, (99, 72, 182)), "the floor");
    let w = nether([98.5, 73.0, 182.5]);
    assert!(!fortress_way::body_in_cell(&w, (99, 73, 182)), "centred");
    // Lane 2 on soul sand: y 69.9, z 168.1 (box from 167.8) is in (104, 69, 167).
    let w = nether([104.5, 69.9, 168.1]);
    assert!(fortress_way::body_in_cell(&w, (104, 69, 167)));
}

#[test]
fn the_guard_skips_the_side_the_body_overhangs() {
    let axis = Some((-1, 0));
    // Centred: the rail on z 7 first, as ever.
    let w = line([8.5, 64.0, 8.5]);
    assert_eq!(
        home::guard_wanted(&w, axis),
        Some(((8, 63, 7), (8, 63, 8), 2))
    );
    // Over the z 7 side (box from 7.95): its rail still, never the guard in the box.
    let mut w = line([8.5, 64.0, 8.25]);
    assert_eq!(
        home::guard_wanted(&w, axis),
        Some(((8, 63, 7), (8, 63, 8), 2))
    );
    set(&mut w, (8, 63, 7), BlockKind::Netherrack);
    assert_eq!(
        home::guard_wanted(&w, axis).map(|g| g.0),
        Some((8, 63, 9)),
        "the z 7 guard skipped, the other side's rail"
    );
    assert_eq!(
        fortress_way::guard_sides_in_body(&w, axis),
        vec![(8, 64, 7)]
    );
    set(&mut w, (8, 63, 9), BlockKind::Netherrack);
    set(&mut w, (8, 64, 9), BlockKind::Netherrack);
    assert_eq!(
        home::guard_wanted(&w, axis),
        None,
        "only the body's own cell left"
    );
}

fn tripper(t: make::Table) -> Plan {
    let mut p = Plan::with_table(Aim::Race, Arc::new(t));
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    p
}

#[test]
fn no_fortress_turn_while_the_way_steers_home() {
    // Race 45760537530: at a point reached, nether bricks ahead turned all three lanes 48 south.
    let mut w = nether([8.5, 64.0, 8.5]);
    remember(&mut w, (-300, 50, 8), BlockKind::NetherPortal);
    // A fortress's bricks west, ahead on the way, seen with an exposed face.
    for x in -40..=-5 {
        remember(&mut w, (x, 60, 8), BlockKind::NetherBricks);
    }
    let mut t = table();
    t.home.way = vec![[9, 8], [-60, 8]];
    let mut p = tripper(t);
    for tick in 100..104 {
        w.ticks = tick;
        p.step(&w);
        p.take_acts();
    }
    let notes: Vec<String> = p.events.iter().map(|e| e.what.clone()).collect();
    assert!(
        !notes.iter().any(|n| n.contains("a fortress seen ahead")),
        "{notes:?}"
    );
    assert!(fortress_way::home_way_steers(&p, false));
}
