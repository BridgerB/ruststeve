//! The lane seed (generality item A): on one hand-built world, a fixed tick seed gives the same
//! choices every time, two seeds that draw apart break the explore heading's tie apart, and with
//! no seed (0, every older test's world) the old default stands.

use std::sync::Arc;

use data::BlockKind;
use plan::lane_seed::{self, DEFAULT_START, Draw};
use plan::{Aim, Plan, home};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use world::{Event, World};

fn section(k: BlockKind) -> Section {
    Section {
        block_count: if k == BlockKind::Air { 0 } else { 4096 },
        fluid_count: 0,
        blocks: Paletted::Single(k.info().map(|i| i.default_state).unwrap_or(0)),
        biomes: Paletted::Single(0),
    }
}

/// Stone from y 0 to 63, air over it, 7 by 7 chunk columns, the feet on the stone at (8, 64, 8):
/// no log anywhere, so a log aim explores, and every heading is as open as every other.
fn flat() -> World {
    let mut sections = vec![section(BlockKind::Stone); 4];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 12));
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
    let pos = [8.5, 64.0, 8.5];
    w.body = Some(physics::Body {
        pos,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(pos);
    w.player.health = Some(20.0);
    w.player.food = Some(20);
    w
}

/// The notes of a log aim's first ticks on the flat world with this tick seed, a new one each
/// tick as the shell sends them (xorshift from `seed`), or 0 throughout for `seed` 0.
fn run(seed: u64) -> (Vec<String>, Plan) {
    let mut w = flat();
    let mut p = Plan::new(Aim::ReachLog { min_distance: 0.0 });
    let mut s = seed;
    for t in 100..120 {
        if s != 0 {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
        }
        w.seed = s;
        w.ticks = t;
        w.session.server_ticks = t;
        p.step(&w);
        p.take_acts();
    }
    (p.events.iter().map(|e| e.what.clone()).collect(), p)
}

fn explore(notes: &[String]) -> Option<String> {
    notes
        .iter()
        .find(|n| n.starts_with("explore toward"))
        .cloned()
}

/// Two shell start seeds whose first tick seeds give different start quarters.
fn two_apart() -> (u64, u64) {
    let first_tick = |s: u64| {
        let mut x = s;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let a = 1;
    let ha = lane_seed::start(first_tick(a)).heading;
    let b = (2..1000)
        .find(|b| lane_seed::start(first_tick(*b)).heading != ha)
        .unwrap();
    (a, b)
}

#[test]
fn a_fixed_seed_gives_the_same_choices() {
    let (a, b) = (run(0x5EED), run(0x5EED));
    assert_eq!(a.0, b.0);
    assert_eq!(a.1.heading, b.1.heading);
    assert!(explore(&a.0).is_some(), "{:?}", a.0);
}

#[test]
fn two_seeds_break_the_heading_tie_apart() {
    let (a, b) = two_apart();
    let (na, pa) = run(a);
    let (nb, pb) = run(b);
    assert_ne!(pa.lane.seed, pb.lane.seed);
    let (ea, eb) = (explore(&na), explore(&nb));
    assert!(ea.is_some() && eb.is_some(), "{na:?} {nb:?}");
    assert_ne!(ea, eb, "the explore targets differ");
    // Each run notes the tie once, with the heading it took.
    for notes in [&na, &nb] {
        let ties: Vec<_> = notes
            .iter()
            .filter(|n| n.starts_with("tie broken by the lane seed"))
            .collect();
        assert_eq!(ties.len(), 1, "{notes:?}");
    }
}

#[test]
fn with_no_seed_the_old_default_stands() {
    let (notes, p) = run(0);
    assert_eq!(p.lane, Draw::default());
    assert!(
        !notes
            .iter()
            .any(|n| n.starts_with("tie broken by the lane seed")),
        "{notes:?}"
    );
    assert_eq!(
        explore(&notes).as_deref(),
        Some("explore toward (8, 64, 40)"),
        "heading 0 is +z, 32 blocks"
    );
    assert_eq!(Draw::default().turn(), DEFAULT_START.turn);
}

#[test]
fn the_seed_is_held_from_its_first_tick() {
    let mut d = Draw::default();
    assert_eq!(d.from_tick(0), None, "0 is no seed");
    let s = d.from_tick(77);
    assert_eq!(s, Some(lane_seed::start(77)));
    assert_eq!(d.from_tick(78), None, "a later tick's seed changes nothing");
    assert_eq!(d.seed, Some(77));
    assert_eq!(d.turn(), lane_seed::start(77).turn);
}

#[test]
fn both_turn_sides_and_all_quarters_are_drawn() {
    let starts: Vec<_> = (1..400).map(lane_seed::start).collect();
    for q in [0.0, 90.0, 180.0, 270.0] {
        assert!(starts.iter().any(|s| s.heading == q), "quarter {q}");
    }
    for t in [90.0, 270.0] {
        assert!(starts.iter().any(|s| s.turn == t), "turn {t}");
    }
}

#[test]
fn a_race_seed_is_fixed_by_run_lane_and_bot() {
    let s = lane_seed::derive("race-46127598383", 2, 0);
    assert_eq!(s, lane_seed::derive("race-46127598383", 2, 0));
    let lanes: std::collections::BTreeSet<u64> = (0..16)
        .map(|l| lane_seed::derive("race-46127598383", l, 0))
        .collect();
    assert_eq!(lanes.len(), 16, "every lane its own seed");
}
