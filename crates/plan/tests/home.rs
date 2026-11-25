//! Issue 19d, the way home (docs/problems/eyes-home.md), on hand-built worlds: Home begun with 12
//! pearls under the race, the top-up's two ends (the count and the clock), the eyes made first in
//! the kit, the far portal lookup over memory (past the loaded chunks), the trip toward it, the
//! walk into a lit portal, the climb from under rock, and Home over once fed on the surface.

use std::sync::Arc;

use data::{BlockKind, ItemKind};
use plan::endermen::EnderPhase;
use plan::home::{self, HomeRules};
use plan::{Aim, Plan, Task, make};
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

/// `solid` sections of `floor` (16 blocks each from y 0), air above.
fn column(floor: BlockKind, solid: usize) -> Column {
    let mut sections = vec![section(floor); solid];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 16 - solid));
    Column { min_y: 0, sections }
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

/// A world in `dim` with 7 by 7 chunk columns of `col` round the origin, the feet at `pos`.
fn world(dim: &str, type_id: i32, col: Column, pos: [f64; 3]) -> World {
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: type_id,
        dimension: dim.to_owned(),
        game_type: 0,
    }));
    let col = Arc::new(col);
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

/// The Nether: netherrack to y 47, warped nylium 48 to 63, the feet at y 64.
fn nether() -> World {
    let mut sections = vec![section(BlockKind::Netherrack); 3];
    sections.push(section(BlockKind::WarpedNylium));
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 12));
    world(
        home::NETHER,
        3,
        Column { min_y: 0, sections },
        [8.5, 64.0, 8.5],
    )
}

fn set(w: &mut World, c: (i32, i32, i32), k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: c,
        state: state(k),
    }));
}

/// A sighting on record at `c` in `dim`, as if seen with an exposed face long ago.
fn remember(w: &mut World, dim: &str, c: (i32, i32, i32), k: BlockKind) {
    let seen =
        memory::Seen::check(dim, c, state(k), [Some(0); 6]).unwrap_or_else(|| panic!("seen"));
    w.memory.record(seen, 1);
}

/// The frontier's kit: 12 pearls, 7 rods, an iron sword, a stone pickaxe, cobblestone.
fn kit(w: &mut World) {
    w.inventory.slots.insert(36, (ItemKind::IronSword, 1));
    w.inventory.slots.insert(37, (ItemKind::StonePickaxe, 1));
    w.inventory.slots.insert(38, (ItemKind::Cobblestone, 64));
    w.inventory.slots.insert(9, (ItemKind::EnderPearl, 12));
    w.inventory.slots.insert(10, (ItemKind::BlazeRod, 7));
    w.held = 0;
}

fn racer() -> Plan {
    Plan::with_table(Aim::Race, table())
}

fn notes(p: &Plan) -> Vec<String> {
    p.events.iter().map(|e| e.what.clone()).collect()
}

fn noted(p: &Plan, what: &str) -> bool {
    p.events.iter().any(|e| e.what.starts_with(what))
}

fn run(p: &mut Plan, w: &mut World, from: u64, ticks: u64) {
    for t in from..from + ticks {
        w.ticks = t;
        p.step(w);
        p.take_acts();
    }
}

#[test]
fn home_rules_match_the_config() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/goals.toml"
    ))
    .unwrap_or_default();
    let goals: make::Goals = toml::from_str(&text).unwrap_or_else(|e| panic!("goals.toml: {e}"));
    assert_eq!(goals.home, HomeRules::default());
}

#[test]
fn twelve_pearls_under_the_race_begin_home_and_the_top_up() {
    let mut w = nether();
    kit(&mut w);
    let mut p = racer();
    run(&mut p, &mut w, 100, 3);
    assert_eq!(p.ender.phase, EnderPhase::Home, "{:?}", notes(&p));
    assert!(noted(&p, "home: 12 pearls held"), "{:?}", notes(&p));
    assert!(!p.ender.home.top_up_over, "14 wanted, 12 held");
    assert_eq!(p.ender.home.since, Some(100));
}

#[test]
fn eleven_pearls_are_not_home() {
    let mut w = nether();
    kit(&mut w);
    w.inventory.slots.insert(9, (ItemKind::EnderPearl, 11));
    let mut p = racer();
    run(&mut p, &mut w, 100, 3);
    assert_ne!(p.ender.phase, EnderPhase::Home);
}

#[test]
fn the_top_up_ends_at_fourteen_pearls() {
    let mut w = nether();
    kit(&mut w);
    let mut p = racer();
    run(&mut p, &mut w, 100, 2);
    w.inventory.slots.insert(9, (ItemKind::EnderPearl, 14));
    run(&mut p, &mut w, 102, 2);
    assert!(p.ender.home.top_up_over, "{:?}", notes(&p));
    let e = p
        .events
        .iter()
        .find(|e| e.what == "home: pearl top-up over")
        .unwrap_or_else(|| panic!("{:?}", notes(&p)));
    assert!(e.why.starts_with("the count"), "{}", e.why);
    assert!(!p.met, "the pearls-done row never ends the race in Home");
}

#[test]
fn the_top_up_ends_on_its_clock_with_twelve() {
    let mut w = nether();
    kit(&mut w);
    let mut p = racer();
    run(&mut p, &mut w, 100, 2);
    run(&mut p, &mut w, 100 + 6000, 2);
    assert!(p.ender.home.top_up_over, "{:?}", notes(&p));
    let e = p
        .events
        .iter()
        .find(|e| e.what == "home: pearl top-up over")
        .unwrap_or_else(|| panic!("{:?}", notes(&p)));
    assert!(e.why.starts_with("the clock"), "{}", e.why);
    assert!(!p.met);
}

#[test]
fn the_kit_makes_eyes_first() {
    let mut w = nether();
    kit(&mut w);
    let mut p = racer();
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    run(&mut p, &mut w, 100, 3);
    assert!(noted(&p, "home: the kit begun"), "{:?}", notes(&p));
    assert!(noted(&p, "base: eyes begun"), "{:?}", notes(&p));
    // The chain's first craft: blaze powder from a rod, in the 2x2 grid.
    assert!(
        p.events
            .iter()
            .any(|e| e.what.starts_with("base: craft") && e.what.contains("blaze_powder")),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn the_far_lookup_finds_a_portal_past_the_loaded_chunks() {
    let mut w = nether();
    // The entry portal's inside, 475 blocks west: three cells high, two wide.
    for y in 35..=37 {
        for x in -467..=-466 {
            remember(&mut w, home::NETHER, (x, y, -20), BlockKind::NetherPortal);
        }
    }
    // Not seen in a chunk the world holds.
    assert!(w.block_at(world::BlockPos::new(-467, 35, -20)).is_none());
    let found = home::portal_far(&w, home::NETHER, [8.5, 64.0, 8.5], 48);
    assert_eq!(
        found.map(|c| (c.1, c.2)),
        Some((35, -20)),
        "lowered to its bottom"
    );
    assert!(matches!(found.map(|c| c.0), Some(-467 | -466)));
    // Out of the ring: none.
    assert_eq!(
        home::portal_far(&w, home::NETHER, [8.5, 64.0, 8.5], 16),
        None
    );
    // Another dimension's record is not this one's.
    assert_eq!(
        home::portal_far(&w, home::OVERWORLD, [8.5, 64.0, 8.5], 48),
        None
    );
}

#[test]
fn the_trip_aims_at_the_remembered_portal() {
    let mut w = nether();
    kit(&mut w);
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    let mut p = racer();
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    run(&mut p, &mut w, 100, 3);
    assert!(
        noted(&p, "home: the portal at (-300, 50, 8) (memory)"),
        "{:?}",
        notes(&p)
    );
    assert!(noted(&p, "home: the trip begun"), "{:?}", notes(&p));
    assert!(
        noted(&p, "heading"),
        "the heading's legs run the trip: {:?}",
        notes(&p)
    );
}

#[test]
fn a_lit_portal_near_is_walked_into() {
    let mut w = nether();
    kit(&mut w);
    // A lit portal 6 east on the floor: obsidian under it, portal blocks 64 and 65.
    for y in 64..=65 {
        set(&mut w, (14, y, 8), BlockKind::NetherPortal);
        remember(&mut w, home::NETHER, (14, y, 8), BlockKind::NetherPortal);
    }
    set(&mut w, (14, 63, 8), BlockKind::Obsidian);
    let mut p = racer();
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    run(&mut p, &mut w, 100, 2);
    assert!(
        noted(&p, "home: enter the portal at (14, 64, 8)"),
        "{:?}",
        notes(&p)
    );
    assert!(matches!(p.task, Task::Enter(_)), "{}", p.task.name());
}

#[test]
fn the_overworld_under_rock_is_climbed_by_stairs() {
    // Stone to y 95; the feet in a cell cut at y 40.
    let mut w = world(
        home::OVERWORLD,
        0,
        column(BlockKind::Stone, 6),
        [8.5, 40.0, 8.5],
    );
    kit(&mut w);
    for y in 40..=41 {
        set(&mut w, (8, y, 8), BlockKind::Air);
    }
    w.stage_reached = Some(world::stage::Stage::Pearls12);
    let mut p = racer();
    run(&mut p, &mut w, 100, 3);
    assert_eq!(p.ender.phase, EnderPhase::Home, "{:?}", notes(&p));
    assert!(
        noted(&p, "home: resumed in the overworld"),
        "{:?}",
        notes(&p)
    );
    assert!(noted(&p, "home: the climb begun"), "{:?}", notes(&p));
    assert!(!matches!(p.task, Task::Enter(_)));
}

#[test]
fn fed_on_the_surface_home_is_over() {
    let mut w = world(
        home::OVERWORLD,
        0,
        column(BlockKind::Stone, 4),
        [8.5, 64.0, 8.5],
    );
    kit(&mut w);
    w.player.food = Some(19);
    w.player.health = Some(20.0);
    w.stage_reached = Some(world::stage::Stage::Pearls12);
    let mut p = racer();
    run(&mut p, &mut w, 100, 4);
    assert!(noted(&p, "home: on the surface"), "{:?}", notes(&p));
    assert!(
        noted(&p, "home: over, fed on the surface"),
        "{:?}",
        notes(&p)
    );
    assert!(p.home_done());
    // Never back into a portal once over.
    run(&mut p, &mut w, 104, 20);
    assert!(matches!(p.task, Task::Idle));
}

#[test]
fn hungry_on_the_surface_with_meat_held_eats_first() {
    let mut w = world(
        home::OVERWORLD,
        0,
        column(BlockKind::Stone, 4),
        [8.5, 64.0, 8.5],
    );
    kit(&mut w);
    w.inventory.slots.insert(11, (ItemKind::Beef, 4));
    w.player.food = Some(15);
    w.stage_reached = Some(world::stage::Stage::Pearls12);
    let mut p = racer();
    run(&mut p, &mut w, 100, 3);
    assert!(noted(&p, "home: eat"), "{:?}", notes(&p));
    assert!(!p.home_done());
}

#[test]
fn lava_beside_the_portal_is_sealed_before_the_walk_in() {
    let mut w = nether();
    kit(&mut w);
    w.inventory.slots.insert(39, (ItemKind::Netherrack, 64));
    // A lit portal 3 east, lava in front of it (south), as at the entry portal's ledge.
    for y in 64..=65 {
        set(&mut w, (11, y, 8), BlockKind::NetherPortal);
        remember(&mut w, home::NETHER, (11, y, 8), BlockKind::NetherPortal);
    }
    set(&mut w, (11, 63, 8), BlockKind::Obsidian);
    set(&mut w, (11, 64, 9), BlockKind::Lava);
    let mut p = racer();
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    run(&mut p, &mut w, 100, 30);
    assert!(
        noted(&p, "home: lava sealed at (11, 64, 9)"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(
        p.ender.home.seals, 1,
        "one seal: the world never answered it"
    );
}

#[test]
fn the_entry_portals_south_face_is_sealed_round_its_standing_cell() {
    // The frontier's entry portal (Q1's probe 45688112814) shifted by (-11, 30, 28): cells x 11
    // to 12, y 64 to 66, z 8; the south face z 9 holds cobblestone at (11, 64, 9) under the one
    // dry standing cell (11, 65, 9), lava at (10, 64..65, 9), (12, 64, 9), (13, 64..65, 9).
    let mut w = nether();
    kit(&mut w);
    w.inventory.slots.insert(39, (ItemKind::Netherrack, 64));
    for x in 11..=12 {
        set(&mut w, (x, 63, 8), BlockKind::Obsidian);
        for y in 64..=66 {
            set(&mut w, (x, y, 8), BlockKind::NetherPortal);
            remember(&mut w, home::NETHER, (x, y, 8), BlockKind::NetherPortal);
        }
    }
    set(&mut w, (11, 64, 9), BlockKind::Cobblestone);
    for c in [
        (10, 64, 9),
        (10, 65, 9),
        (12, 64, 9),
        (13, 64, 9),
        (13, 65, 9),
    ] {
        set(&mut w, c, BlockKind::Lava);
    }
    // The bot in its tunnel behind the face, on netherrack.
    set(&mut w, (11, 64, 10), BlockKind::Netherrack);
    w.body = w.body.map(|b| physics::Body {
        pos: [11.5, 65.0, 10.5],
        ..b
    });
    w.player.pos = Some([11.5, 65.0, 10.5]);
    let mut p = racer();
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    // The world answers each placement: the clicked face's neighbour becomes netherrack.
    for t in 100..200 {
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
    for c in ["(10, 65, 9)", "(12, 64, 9)", "(13, 65, 9)"] {
        assert!(
            noted(&p, &format!("home: lava sealed at {c}")),
            "{c}: {:?}",
            notes(&p)
        );
    }
}
