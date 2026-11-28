//! The iron search (docs/problems/iron-search.md) on hand-built worlds: each choice of
//! `plan::iron` once. A heading into the bot's own tunnel is turned from, a strip turns after
//! its length and after stalling, a strip the feet left is ended, a box of open cells has no fresh
//! heading, a cave beyond view is found and a one-wide tunnel is not, the rise under the band and
//! the stairs' way with no fresh rock, and `[iron]` in config/goals.toml equal to the defaults.

use std::sync::Arc;

use data::BlockKind;
use plan::iron::{self, IronRules, IronSearch, StripPlan};
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

/// Stone from y 0 to 63, air over it, 7 by 7 chunk columns round the origin; the feet at
/// (0, 20, 0) in a carved cell.
fn world() -> World {
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
    let pos = [0.5, 20.0, 0.5];
    w.body = Some(physics::Body {
        pos,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(pos);
    carve(&mut w, (0, 20, 0));
    carve(&mut w, (0, 21, 0));
    w
}

fn set(w: &mut World, (x, y, z): (i32, i32, i32), k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: (x, y, z),
        state: state(k),
    }));
}

fn carve(w: &mut World, c: (i32, i32, i32)) {
    set(w, c, BlockKind::Air);
}

/// A one-wide, two-high tunnel along x from `x0` to `x1` at z, feet at y.
fn tunnel_x(w: &mut World, x0: i32, x1: i32, y: i32, z: i32) {
    for x in x0..=x1 {
        carve(w, (x, y, z));
        carve(w, (x, y + 1, z));
    }
}

#[test]
fn a_heading_into_its_own_tunnel_is_turned_from() {
    let mut w = world();
    // The bot's own tunnel runs west from the feet: walking it again cuts nothing.
    tunnel_x(&mut w, -8, 0, 20, 0);
    let feet = (0, 20, 0);
    assert!(!iron::fresh(&w, feet, (-1, 0), 3));
    assert!(iron::fresh(&w, feet, (1, 0), 3));
    let dir = iron::fresh_heading(&w, feet, (-1, 0), 3);
    assert_eq!(dir, Some((0, -1)), "a quarter right of west, into rock");
}

#[test]
fn a_strip_turns_after_its_length() {
    let mut w = world();
    let rules = IronRules::default();
    let mut s = IronSearch::default();
    let (plan, notes) = s.next_strip(&w, (0, 20, 0), (1, 0), 0, &rules);
    assert_eq!(plan, StripPlan::Cut((1, 0)));
    assert!(
        notes
            .iter()
            .any(|(what, _)| what.starts_with("strip begun"))
    );
    // The strip cut 24 blocks east and showed nothing.
    tunnel_x(&mut w, 0, 24, 20, 0);
    let (plan, notes) = s.next_strip(&w, (24, 20, 0), (1, 0), 0, &rules);
    assert_eq!(plan, StripPlan::Cut((0, 1)), "a quarter right of east");
    assert!(
        notes
            .iter()
            .any(|(what, _)| what == "strip turned after 24 blocks")
    );
    assert_eq!(s.counts.turned_empty, 1);
    // The next turn is the other way: strips step across new ground, never round a square.
    assert!(s.left);
}

#[test]
fn a_stalled_strip_turns() {
    let w = world();
    let rules = IronRules::default();
    let mut s = IronSearch::default();
    let feet = (0, 20, 0);
    assert_eq!(
        s.next_strip(&w, feet, (1, 0), 0, &rules).0,
        StripPlan::Cut((1, 0))
    );
    // One tunnel no further along: kept.
    assert_eq!(
        s.next_strip(&w, feet, (1, 0), 0, &rules).0,
        StripPlan::Cut((1, 0))
    );
    // Two: turned.
    let (plan, notes) = s.next_strip(&w, feet, (1, 0), 0, &rules);
    assert_eq!(plan, StripPlan::Cut((0, 1)));
    assert!(
        notes
            .iter()
            .any(|(what, _)| what.starts_with("strip turned, stalled"))
    );
    assert_eq!(s.counts.turned_stalled, 1);
}

#[test]
fn a_strip_the_feet_left_is_ended_with_its_iron() {
    let mut w = world();
    let rules = IronRules::default();
    let mut s = IronSearch::default();
    s.next_strip(&w, (0, 20, 0), (1, 0), 2, &rules);
    // The bot walked off to ore five blocks to the side and came back with three more.
    carve(&mut w, (0, 20, 5));
    carve(&mut w, (0, 21, 5));
    let (plan, notes) = s.next_strip(&w, (0, 20, 5), (1, 0), 5, &rules);
    assert_eq!(plan, StripPlan::Cut((1, 0)));
    assert!(notes.iter().any(
        |(what, why)| what == "strip left after 0 blocks" && why.contains("it gave 3 raw iron")
    ));
    assert_eq!(s.strip.map(|st| st.from), Some((0, 20, 5)));
}

#[test]
fn open_cells_on_every_side_leave_no_fresh_heading() {
    let mut w = world();
    tunnel_x(&mut w, -3, 3, 20, 0);
    for z in -3..=3 {
        carve(&mut w, (0, 20, z));
        carve(&mut w, (0, 21, z));
    }
    let rules = IronRules::default();
    let mut s = IronSearch::default();
    let (plan, notes) = s.next_strip(&w, (0, 20, 0), (1, 0), 0, &rules);
    assert_eq!(plan, StripPlan::NoFresh);
    assert!(
        notes
            .iter()
            .any(|(what, _)| what == "no fresh rock on any heading")
    );
    // Then stairs to another level: down from the depth, up from just over the rise line.
    assert!(iron::no_fresh_down((0, 16, 0), Some(16), &rules));
    assert!(!iron::no_fresh_down((0, 5, 0), Some(16), &rules));
}

/// The eye over the feet at (0, 20, 0).
const EYE: [f64; 3] = [0.5, 21.62, 0.5];

/// A room 5 by 5 by 3 (feet y 20 to 22) round (cx, 20, cz).
fn room(w: &mut World, cx: i32, cz: i32) {
    for x in cx - 2..=cx + 2 {
        for z in cz - 2..=cz + 2 {
            for y in 20..=22 {
                carve(w, (x, y, z));
            }
        }
    }
}

#[test]
fn a_cave_sealed_behind_rock_is_not_sought() {
    let mut w = world();
    let rules = IronRules::default();
    // A room 30 south, in the chunk data, behind 27 blocks of rock no face of which was seen.
    room(&mut w, 0, 30);
    assert!(iron::cave_cell(&w, (0, 20, 30)), "a cave by its shape");
    assert!(!iron::cave_seen(&w, EYE, (0, 20, 30)), "but not seen");
    assert_eq!(iron::cave_near(&w, (0, 20, 0), EYE, &rules, &[]), None);
}

#[test]
fn a_cave_seen_beyond_view_is_found_and_a_tunnel_is_not() {
    let mut w = world();
    let rules = IronRules::default();
    // A one-wide tunnel 30 east: no cross of open cells, not a cave.
    tunnel_x(&mut w, 26, 34, 20, 0);
    assert_eq!(iron::cave_near(&w, (0, 20, 0), EYE, &rules, &[]), None);
    // A room 30 south, open to the feet along a corridor: seen.
    room(&mut w, 0, 30);
    for z in 0..=28 {
        carve(&mut w, (0, 20, z));
        carve(&mut w, (0, 21, z));
    }
    // A second room 30 west, sealed: never chosen, though as near.
    room(&mut w, -30, 0);
    assert!(iron::cave_seen(&w, EYE, (0, 20, 30)));
    assert!(!iron::cave_seen(&w, EYE, (-30, 20, 0)));
    assert!(!iron::cave_cell(&w, (30, 20, 0)));
    assert_eq!(
        iron::cave_near(&w, (0, 20, 0), EYE, &rules, &[]),
        Some((0, 20, 30))
    );
    // Taken already: not sought again.
    assert_eq!(
        iron::cave_near(&w, (0, 20, 0), EYE, &rules, &[(0, 20, 31)]),
        None
    );
    // Nearer than `cave_from`: its walls are already in view, not sought.
    let eye = [0.5, 21.62, 20.5];
    assert_eq!(iron::cave_near(&w, (0, 20, 20), eye, &rules, &[]), None);
}

#[test]
fn unseen_air_ahead_is_rock_to_a_strip() {
    let mut w = world();
    // A pocket two blocks east, sealed: never seen, so no reason to turn from it.
    for y in 20..=21 {
        carve(&mut w, (2, y, 0));
        carve(&mut w, (3, y, 0));
    }
    assert!(iron::fresh(&w, (0, 20, 0), (1, 0), 3));
    // Opened to the feet, it is seen, and the strip turns from it.
    carve(&mut w, (1, 20, 0));
    carve(&mut w, (1, 21, 0));
    assert!(!iron::fresh(&w, (0, 20, 0), (1, 0), 3));
}

#[test]
fn under_the_band_the_search_rises() {
    let rules = IronRules::default();
    assert!(iron::below_band((0, -23, 0), &rules));
    assert!(!iron::below_band((0, 0, 0), &rules));
    assert!(iron::is_iron_row(&[
        BlockKind::IronOre,
        BlockKind::DeepslateIronOre
    ]));
    assert!(!iron::is_iron_row(&[BlockKind::CoalOre]));
}

#[test]
fn the_config_holds_the_defaults() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/goals.toml"
    ))
    .unwrap_or_default();
    let v: toml::Value = toml::from_str(&text).unwrap_or_else(|e| panic!("goals.toml: {e}"));
    let r: IronRules = v
        .get("iron")
        .cloned()
        .unwrap_or_else(|| panic!("no [iron] in goals.toml"))
        .try_into()
        .unwrap_or_else(|e| panic!("[iron]: {e}"));
    assert_eq!(r, IronRules::default());
}
