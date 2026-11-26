//! Issue 19, the way home under cover (agent Q9, docs/problems/eyes-route.md), on a hand-built
//! Nether: solid netherrack to y 111 round the covered route's points by the soul sand valley
//! ((143, 69, 99), (130, 63, 108), (114, 62, 100)), the entry portal remembered at its real cells
//! (x 22 to 23, y 34 to 36, z -20): the trip begins the covered way at the nearest point, marks
//! the earlier ones passed, and tunnels to the next.

use std::sync::Arc;

use data::{BlockKind, ItemKind};
use plan::endermen::EnderPhase;
use plan::{Aim, Plan, Task, covered_way, home, make};
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

fn table() -> Arc<make::Table> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/goals.toml"
    ))
    .unwrap_or_default();
    let goals: make::Goals = toml::from_str(&text).unwrap_or_else(|e| panic!("goals.toml: {e}"));
    Arc::new(make::Table::new(&goals).unwrap_or_else(|e| panic!("table: {e}")))
}

fn set(w: &mut World, c: (i32, i32, i32), k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: c,
        state: state(k),
    }));
}

/// Rock to y 111 over chunks x 4 to 11, z 0 to 7; the feet at `pos` in a dug cell; the portal in
/// memory.
fn nether(pos: [f64; 3]) -> World {
    let mut sections = vec![section(BlockKind::Netherrack); 7];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 9));
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
    for x in 4..=11 {
        for z in 0..=7 {
            w.step(&Event::Packet(S2c::LevelChunkWithLight {
                x,
                z,
                column: Arc::clone(&col),
            }));
        }
    }
    let feet = (
        pos[0].floor() as i32,
        pos[1].floor() as i32,
        pos[2].floor() as i32,
    );
    set(&mut w, feet, BlockKind::Air);
    set(&mut w, (feet.0, feet.1 + 1, feet.2), BlockKind::Air);
    for x in 22..=23 {
        for y in 34..=36 {
            let seen = memory::Seen::check(
                home::NETHER,
                (x, y, -20),
                state(BlockKind::NetherPortal),
                [Some(0); 6],
            )
            .unwrap_or_else(|| panic!("seen"));
            w.memory.record(seen, 1);
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
    w.inventory.slots.insert(36, (ItemKind::Netherrack, 64));
    w.inventory.slots.insert(37, (ItemKind::StonePickaxe, 1));
    w.inventory.slots.insert(9, (ItemKind::EnderEye, 14));
    w.held = 0;
    w
}

fn homeward() -> Plan {
    let mut p = Plan::with_table(Aim::Race, table());
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    p.ender.home.trip_begun = true;
    p
}

fn notes(p: &Plan) -> Vec<String> {
    p.events.iter().map(|e| e.what.clone()).collect()
}

fn run(p: &mut Plan, w: &mut World) {
    for t in 100..104 {
        w.ticks = t;
        p.step(w);
        p.take_acts();
    }
}

#[test]
fn the_config_holds_the_default_covered_points() {
    assert_eq!(table().home.covered, covered_way::default_covered());
    // Every covered point is on the plane among `[home] way`'s, in order (the heading's fallback).
    let way = home::HomeRules::default().way;
    let flat: Vec<[i32; 2]> = covered_way::default_covered()
        .iter()
        .map(|p| [p[0], p[2]])
        .collect();
    assert_eq!(&way[..flat.len()], &flat[..]);
}

#[test]
fn resumed_by_a_point_the_covered_way_tunnels_to_the_next() {
    // 7 on the plane from (130, 63, 108), the nearest point: the 21 before it passed.
    let mut w = nether([136.5, 63.0, 104.5]);
    let mut p = homeward();
    run(&mut p, &mut w);
    let n = notes(&p);
    assert!(
        n.iter()
            .any(|s| s.starts_with("covered way: begun at point 21 of 25")),
        "{n:?}"
    );
    assert!(
        n.iter()
            .any(|s| s.starts_with("covered way: tunnel to point 21 at (130, 63, 108)")),
        "{n:?}"
    );
    assert!(!n.iter().any(|s| s.starts_with("heading leg")), "{n:?}");
    assert!(matches!(p.task, Task::Stairs(_)), "{}", p.task.name());
    assert_eq!(p.ender.home.covered.passed, 21);
    // The first point's column is passed for the heading's fallback too.
    assert!(p.ender.home.ways_passed.contains(&(479 >> 4, 19 >> 4)));
}

#[test]
fn at_a_point_it_is_passed_and_the_next_aimed_at() {
    let mut w = nether([130.5, 63.0, 108.5]);
    let mut p = homeward();
    run(&mut p, &mut w);
    let n = notes(&p);
    assert!(
        n.iter()
            .any(|s| s.starts_with("covered way: point 21 of 25 passed at [130, 63, 108, 0]")),
        "{n:?}"
    );
    assert!(
        n.iter()
            .any(|s| s.starts_with("covered way: tunnel to point 22 at (114, 62, 100)")),
        "{n:?}"
    );
}

#[test]
fn another_portal_is_not_this_route() {
    // The portal elsewhere (a test world's, or another world's): the covered way stands down.
    // The way's `[home] portal` moved off the remembered one.
    let mut w = nether([136.5, 63.0, 104.5]);
    let mut t = (*table()).clone();
    t.home.portal = Some([300, 34, 300]);
    let mut p = homeward();
    p.table = Some(Arc::new(t));
    run(&mut p, &mut w);
    assert!(
        !notes(&p).iter().any(|s| s.starts_with("covered way")),
        "{:?}",
        notes(&p)
    );
}
