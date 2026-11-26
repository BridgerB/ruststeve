//! Issue 19, the way home's last leg (agent Q6, docs/problems/eyes-portal.md), on hand-built
//! worlds: the entry portal shifted by (-11, 30, 28) (cells x 11 to 12, y 64 to 66, z 8; the
//! standing cell (11, 65, 9) on cobblestone with lava beside it; the tunnel cell (11, 65, 10)),
//! with a hill of netherrack to its south as the real portal has: the last leg tunnels from the
//! hill's top and cuts exactly to the tunnel cell, never into the standing cell or the lava.

use std::sync::Arc;

use data::{BlockKind, ItemKind};
use plan::endermen::EnderPhase;
use plan::home;
use plan::{Aim, Plan, Task, make, portal_way};
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

/// The Nether: netherrack to y 63, air from y 64; the feet at `pos`.
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
    w.inventory.slots.insert(36, (ItemKind::Netherrack, 64));
    w.inventory.slots.insert(37, (ItemKind::StonePickaxe, 1));
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

/// The entry portal, shifted, with its south hill: netherrack x 4 to 19, y 64 to `top`, z 10 to
/// 24 (the row behind the standing cell solid, as z -18 is at the real portal), the standing cell
/// on cobblestone with lava beside it at (10, 65, 9) and under its neighbour at (12, 64, 9), the
/// lava sea to the north at y 63.
fn portal_and_hill(w: &mut World, top: i32) {
    for x in 4..=19 {
        for y in 64..=top {
            for z in 10..=24 {
                set(w, (x, y, z), BlockKind::Netherrack);
            }
        }
    }
    for x in 10..=13 {
        set(w, (x, 63, 8), BlockKind::Obsidian);
        set(w, (x, 67, 8), BlockKind::Obsidian);
    }
    for y in 64..=66 {
        set(w, (10, y, 8), BlockKind::Obsidian);
        set(w, (13, y, 8), BlockKind::Obsidian);
    }
    for x in 11..=12 {
        for y in 64..=66 {
            set(w, (x, y, 8), BlockKind::NetherPortal);
            let seen = memory::Seen::check(
                home::NETHER,
                (x, y, 8),
                state(BlockKind::NetherPortal),
                [Some(0); 6],
            )
            .unwrap_or_else(|| panic!("seen"));
            w.memory.record(seen, 1);
        }
    }
    set(w, (11, 64, 9), BlockKind::Cobblestone);
    set(w, (10, 64, 9), BlockKind::Netherrack);
    for c in [(10, 65, 9), (12, 64, 9), (13, 64, 9), (13, 65, 9)] {
        set(w, c, BlockKind::Lava);
    }
    for x in 4..=19 {
        for z in 0..=7 {
            set(w, (x, 63, z), BlockKind::Lava);
        }
    }
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

#[test]
fn the_cut_ends_exactly_at_the_tunnel_cell() {
    let mut w = nether([11.5, 80.0, 20.5]);
    portal_and_hill(&mut w, 79);
    let lit: Vec<_> = (64..=66).flat_map(|y| [(11, y, 8), (12, y, 8)]).collect();
    let front = home::portal_front(&w, &lit, [11.5, 80.0, 20.5]);
    assert_eq!(
        front,
        Some(home::Front {
            stand: (11, 65, 9),
            tunnel: (11, 65, 10),
            normal: (0, 1),
        })
    );
    // From inside the hill, 4 up and 4 out: stairs down to the tunnel cell itself.
    let from = (11, 69, 14);
    let mut keep_out = lit.clone();
    keep_out.push((11, 65, 9));
    let route = portal_way::cut(&w, from, (11, 65, 10), &keep_out, 12, 6000)
        .unwrap_or_else(|| panic!("no cut"));
    assert_eq!(route.first(), Some(&from));
    assert_eq!(route.last(), Some(&(11, 65, 10)));
    assert!(route.iter().all(|c| !keep_out.contains(c)), "{route:?}");
    // Each step one stair or level step.
    for p in route.windows(2) {
        let (a, b) = (p[0], p[1]);
        assert_eq!((a.0 - b.0).abs() + (a.2 - b.2).abs(), 1, "{a:?} {b:?}");
        assert!((a.1 - b.1).abs() <= 1, "{a:?} {b:?}");
    }
}

#[test]
fn in_the_open_over_the_portal_the_last_leg_digs_in_first() {
    // On the hill's top in the open, 15 up and 12 out: the heading stands down and the bot digs
    // straight down where it stands, out of the skeletons' line.
    let mut w = nether([11.5, 80.0, 20.5]);
    portal_and_hill(&mut w, 79);
    let mut p = homeward();
    for t in 100..104 {
        w.ticks = t;
        p.step(&w);
        p.take_acts();
    }
    let n = notes(&p);
    assert!(
        n.iter()
            .any(|s| s.starts_with("portal way: the last leg to (11, 65, 10)")),
        "{n:?}"
    );
    assert!(
        n.iter()
            .any(|s| s.starts_with("portal way: dig in at (11, 79, 20)")),
        "{n:?}"
    );
    assert!(!n.iter().any(|s| s.starts_with("heading leg")), "{n:?}");
    assert!(matches!(p.task, Task::Dig(_)), "{}", p.task.name());
}

#[test]
fn under_cover_the_last_leg_tunnels_not_heads() {
    // Inside the hill, 3 of rock over the head, 15 up and 10 out: the tunnel to the front.
    let mut w = nether([11.5, 80.0, 18.5]);
    portal_and_hill(&mut w, 84);
    for y in 80..=81 {
        set(&mut w, (11, y, 18), BlockKind::Air);
    }
    let mut p = homeward();
    for t in 100..104 {
        w.ticks = t;
        p.step(&w);
        p.take_acts();
    }
    let n = notes(&p);
    assert!(
        n.iter()
            .any(|s| s.starts_with("portal way: tunnel to (11, 65, 10)")),
        "{n:?}"
    );
    assert!(
        !n.iter().any(|s| s.starts_with("portal way: dig in")),
        "{n:?}"
    );
    assert!(!n.iter().any(|s| s.starts_with("heading leg")), "{n:?}");
    assert!(matches!(p.task, Task::Stairs(_)), "{}", p.task.name());
}

#[test]
fn a_covered_route_keeps_rock_over_the_head() {
    // From the hill's top to the tunnel cell: with cover, no step of the route past its first
    // four (the stairs down from the top) has open air in the two cells over the head.
    let mut w = nether([11.5, 80.0, 22.5]);
    portal_and_hill(&mut w, 79);
    let none = std::collections::BTreeSet::new();
    let r = plan::tunnel::route_keeping(
        &w,
        (11, 80, 22),
        &[(11, 65, 10)],
        &none,
        false,
        plan::tunnel::MAX_NODES,
        plan::tunnel::Limits {
            cover: true,
            ..plan::tunnel::Limits::default()
        },
    )
    .unwrap_or_else(|| panic!("no route"));
    let open = |c: (i32, i32, i32)| c.1 + 2 >= 80 || c.1 + 3 >= 80;
    let exposed = r.cells.iter().filter(|c| open(**c)).count();
    assert!(exposed <= 4, "{exposed} exposed: {:?}", r.cells);
}

#[test]
fn a_pocket_on_a_ledge_digs_down_first() {
    // Health at the danger line, nothing to eat, on the edge of a drop: dig down one first.
    let mut w = nether([11.5, 64.0, 11.5]);
    w.player.health = Some(6.0);
    for y in 60..=63 {
        set(&mut w, (12, y, 11), BlockKind::Air);
    }
    let mut p = homeward();
    for t in 100..104 {
        w.ticks = t;
        p.step(&w);
        p.take_acts();
    }
    let n = notes(&p);
    assert!(
        n.iter()
            .any(|s| s.starts_with("pocket on a ledge: dig down at (11, 63, 11)")),
        "{n:?}"
    );
}

#[test]
fn near_the_front_the_last_leg_cuts_to_the_tunnel_cell() {
    // Inside the hill 4 out and 4 up (a tunnel's end within its reach of 5 would be nearer).
    let mut w = nether([11.5, 69.0, 14.5]);
    portal_and_hill(&mut w, 79);
    for y in 69..=70 {
        set(&mut w, (11, y, 14), BlockKind::Air);
    }
    let mut p = homeward();
    for t in 100..104 {
        w.ticks = t;
        p.step(&w);
        p.take_acts();
    }
    let n = notes(&p);
    assert!(
        n.iter()
            .any(|s| s.starts_with("portal way: cut to the front at (11, 65, 10)")),
        "{n:?}"
    );
    assert!(matches!(p.task, Task::Stairs(_)), "{}", p.task.name());
}

#[test]
fn at_the_tunnel_cell_the_approach_seals_and_goes_in() {
    // In the tunnel cell, cut: the last leg hands over, and the approach seals the lava round
    // the standing cell (then walks in: `home::tests`, crates/plan/tests/home.rs).
    let mut w = nether([11.5, 65.0, 10.5]);
    portal_and_hill(&mut w, 79);
    for y in 65..=66 {
        set(&mut w, (11, y, 10), BlockKind::Air);
    }
    let mut p = homeward();
    for t in 100..140 {
        w.ticks = t;
        p.step(&w);
        for a in p.take_acts() {
            if let world::Act::UseOn { pos, face } = a {
                let (dx, dy, dz) = match face {
                    0 => (0, -1, 0),
                    1 => (0, 1, 0),
                    2 => (0, 0, -1),
                    3 => (0, 0, 1),
                    4 => (-1, 0, 0),
                    _ => (1, 0, 0),
                };
                set(
                    &mut w,
                    (pos.x + dx, pos.y + dy, pos.z + dz),
                    BlockKind::Netherrack,
                );
            }
        }
    }
    let n = notes(&p);
    assert!(!n.iter().any(|s| s.starts_with("portal way: cut")), "{n:?}");
    assert!(
        n.iter()
            .any(|s| s.starts_with("home: lava sealed at (10, 65, 9)")),
        "{n:?}"
    );
    assert!(
        n.iter().any(|s| s.starts_with("home: enter the portal at")),
        "{n:?}"
    );
}

#[test]
fn the_rules_default_to_the_config() {
    assert_eq!(table().portal_way, portal_way::WayRules::default());
}
