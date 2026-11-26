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
    // Never back into a portal once over: with 12 pearls and 7 rods held the eyes are makeable,
    // so the eyes' phase takes the planner (Q4) and its first task is the eyes' craft (60 ticks:
    // past a `RETRY_TICKS` wait set as home ended).
    run(&mut p, &mut w, 104, 60);
    assert!(!matches!(p.task, Task::Enter(_)), "{:?}", notes(&p));
    assert!(
        p.events.iter().any(|e| e.what.starts_with("eyes: craft")),
        "{:?}",
        notes(&p)
    );
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
    // Netherrack held (hotbar 0): the test world answers no swap.
    w.inventory.slots.insert(36, (ItemKind::Netherrack, 64));
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
    // The world never answers: the seal is tried again, one a SHELTER_EVERY_TICKS, never into
    // any other cell.
    assert!(p.ender.home.seals >= 1, "{}", p.ender.home.seals);
    assert!(
        notes(&p)
            .iter()
            .filter(|n| n.starts_with("home: lava sealed"))
            .all(|n| n == "home: lava sealed at (11, 64, 9)"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn the_entry_portals_south_face_is_sealed_round_its_standing_cell() {
    // The frontier's entry portal (Q1's probe 45688112814) shifted by (-11, 30, 28): cells x 11
    // to 12, y 64 to 66, z 8; the south face z 9 holds cobblestone at (11, 64, 9) under the one
    // dry standing cell (11, 65, 9), lava at (10, 64..65, 9), (12, 64, 9), (13, 64..65, 9).
    let mut w = nether();
    kit(&mut w);
    // Netherrack held (hotbar 0): the test world answers no swap.
    w.inventory.slots.insert(36, (ItemKind::Netherrack, 64));
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
    // The bot in its tunnel behind the face: the row behind it netherrack (z -18 at the entry
    // portal, solid at x 19 to 26, y 33 to 35), the tunnel cell and its head cut.
    for x in 8..=15 {
        for y in 64..=66 {
            if x != 11 || y == 64 {
                set(&mut w, (x, y, 10), BlockKind::Netherrack);
            }
        }
    }
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

/// The entry portal shifted by (-11, 30, 28): cells x 11 to 12, y 64 to 66, z 8 over obsidian;
/// the south face (z 9) with cobblestone under the standing cell (11, 65, 9) and lava round it;
/// the north face's floor (y 63, z 7) lava, as the lava sea under the north face.
fn entry_portal(w: &mut World) -> Vec<(i32, i32, i32)> {
    let mut lit = Vec::new();
    for x in 11..=12 {
        set(w, (x, 63, 8), BlockKind::Obsidian);
        for y in 64..=66 {
            set(w, (x, y, 8), BlockKind::NetherPortal);
            remember(w, home::NETHER, (x, y, 8), BlockKind::NetherPortal);
            lit.push((x, y, 8));
        }
    }
    set(w, (11, 64, 9), BlockKind::Cobblestone);
    for c in [(10, 64, 9), (12, 64, 9), (13, 64, 9)] {
        set(w, c, BlockKind::Lava);
    }
    for x in 8..=15 {
        for z in 4..=7 {
            set(w, (x, 63, z), BlockKind::Lava);
        }
    }
    lit
}

#[test]
fn the_north_face_without_a_floor_is_never_the_front() {
    let mut w = nether();
    kit(&mut w);
    let lit = entry_portal(&mut w);
    // The bot nearer the north face than the south one: the south is still the front.
    for at in [[11.5, 64.0, 2.5], [11.5, 65.0, 10.5], [20.5, 64.0, 8.5]] {
        let f = home::portal_front(&w, &lit, at);
        assert_eq!(
            f,
            Some(home::Front {
                stand: (11, 65, 9),
                tunnel: (11, 65, 10),
                normal: (0, 1),
            }),
            "from {at:?}"
        );
    }
}

#[test]
fn from_the_north_side_the_portal_is_not_walked_into() {
    let mut w = nether();
    kit(&mut w);
    // Netherrack held (hotbar 0): the test world answers no swap.
    w.inventory.slots.insert(36, (ItemKind::Netherrack, 64));
    entry_portal(&mut w);
    w.body = w.body.map(|b| physics::Body {
        pos: [11.5, 64.0, 2.5],
        ..b
    });
    w.player.pos = Some([11.5, 64.0, 2.5]);
    let mut p = racer();
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    run(&mut p, &mut w, 100, 30);
    let n = notes(&p);
    assert!(!noted(&p, "home: enter"), "{n:?}");
    assert!(!n.iter().any(|s| s.contains("far walk")), "{n:?}");
    assert!(
        n.iter()
            .filter(|s| s.starts_with("home: approach") && s.contains("tunnel"))
            .all(|s| s.ends_with("(11, 65, 10)")),
        "{n:?}"
    );
}

#[test]
fn the_climbs_stairs_turn_from_a_cave_ahead() {
    // Stone to y 95; the feet in a cell cut at y 40; a cave east, where the first stair would cut.
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
    for y in 41..=43 {
        set(&mut w, (9, y, 8), BlockKind::CaveAir);
    }
    // The climb began far off: the rock rule holds here.
    let mut s = plan::stairs::Stairs::up((1, 0), 0);
    s.rock = Some((40, 40, 40));
    let _ = s.step(&w, 1);
    assert!(
        s.cave_refusals() >= 1 && s.dir != (1, 0),
        "{:?} {:?}",
        s.refusals,
        s.dir
    );
    // Within ROCK_FROM of where it began (the portal's room) the open cells are no cave.
    let mut s = plan::stairs::Stairs::up((1, 0), 0);
    s.rock = Some((8, 40, 8));
    let _ = s.step(&w, 1);
    assert_eq!(s.cave_refusals(), 0, "{:?}", s.refusals);
}

#[test]
fn warped_wood_burns_as_sticks_and_the_cook_asks_for_them() {
    let mut c = make::Counts::new();
    c.insert(ItemKind::WarpedStem, 8);
    c.insert(ItemKind::WarpedPlanks, 12);
    // A stem is 8 sticks of 100 ticks, a warped plank 2: no item of warped wood burns itself.
    assert_eq!(home::fuel_ticks_held(&c), 8 * 800 + 12 * 200);
    c.insert(ItemKind::Beef, 6);
    let t = table();
    let row = plan::cook_goal(&c, &t.survival).unwrap_or_else(|| panic!("a cook row"));
    assert_eq!(row.id, "cook-fuel", "{row:?}");
    assert_eq!(row.provides[0].0.key, "item:stick", "{row:?}");
    // Six pieces at 200 ticks: 12 sticks.
    assert_eq!(row.provides[0].1, 12, "{row:?}");
    // With a log that burns held, its planks are the fuel.
    c.insert(ItemKind::OakLog, 2);
    let row = plan::cook_goal(&c, &t.survival).unwrap_or_else(|| panic!("a cook row"));
    assert_eq!(row.provides[0].0.key, "item:oak_planks", "{row:?}");
    // Sticks over the reserve held: they count.
    c.remove(&ItemKind::OakLog);
    c.insert(ItemKind::Stick, 14);
    let row = plan::cook_goal(&c, &t.survival).unwrap_or_else(|| panic!("a cook row"));
    assert_eq!(row.id, "cook", "{row:?}");
}

#[test]
fn a_heading_with_a_point_turns_and_chooses_toward_it() {
    use plan::heading::{choose_toward, turn_toward};
    // Race 45698618366 lane 3: at (450, 40, 51), the portal at (22, -20): -x and -z.
    let feet = (450, 40, 51);
    let p = (22, -20);
    assert_eq!(turn_toward((-1, 0), feet, p), Some((0, -1)));
    assert_eq!(turn_toward((0, 1), feet, p), Some((-1, 0)));
    // The fresh choice: the longer axis toward it first, never away (east was chosen).
    assert_eq!(choose_toward(feet, p, &[]), Some((-1, 0)));
    assert_eq!(choose_toward(feet, p, &[(-1, 0)]), Some((0, -1)));
    assert_eq!(choose_toward(feet, p, &[(-1, 0), (0, -1)]), None);
    // Nothing left on the turned-to axis: the exploring turn decides.
    assert_eq!(turn_toward((-1, 0), (450, 40, -20), p), None);
}

fn add_cube(w: &mut World, id: i32, at: [f64; 3]) {
    w.step(&Event::Packet(S2c::AddEntity {
        id,
        uuid: [0; 16],
        kind: data::EntityKind::MagmaCube.id(),
        x: at[0],
        y: at[1],
        z: at[2],
        velocity: [0.0; 3],
    }));
}

#[test]
fn two_magma_cubes_near_on_the_trip_seal_a_pocket_and_clear_it() {
    let mut w = nether();
    kit(&mut w);
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    add_cube(&mut w, 50, [11.5, 64.0, 8.5]);
    add_cube(&mut w, 51, [8.5, 64.0, 12.5]);
    let mut p = racer();
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    run(&mut p, &mut w, 100, 5);
    assert!(
        noted(&p, "home: pocket from magma cubes"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.ender.home.cube_pockets, 1);
    // Both gone: held CUBE_CLEAR_TICKS, then on.
    w.step(&Event::Packet(S2c::RemoveEntities { ids: vec![50, 51] }));
    run(&mut p, &mut w, 105, 230);
    assert!(noted(&p, "home: out of the cube pocket"), "{:?}", notes(&p));
    assert_eq!(p.ender.home.cube_pockets, 1);
}

#[test]
fn a_basalt_floor_with_no_wall_restocks_beside_the_feet() {
    // The feet at (8, 64, 8) on a delta floor of basalt, no wall in reach.
    let mut w = nether();
    for x in 5..=11 {
        for z in 5..=11 {
            set(&mut w, (x, 63, z), BlockKind::Basalt);
        }
    }
    let cells = plan::heading::restock_cells(&w, Some((1, 0)), plan::heading::RESTOCK_LOOK);
    assert!(!cells.is_empty(), "a floor cell to dig");
    assert!(cells.iter().all(|c| c.1 == 63), "{cells:?}");
    // Never under the feet, nor under the next step east.
    assert!(!cells.contains(&(8, 63, 8)), "{cells:?}");
    assert!(!cells.contains(&(9, 63, 8)), "{cells:?}");
    // Lava beside a cell refuses it.
    set(&mut w, (7, 62, 8), BlockKind::Lava);
    let cells = plan::heading::restock_cells(&w, Some((1, 0)), plan::heading::RESTOCK_LOOK);
    assert!(!cells.contains(&(7, 63, 8)), "{cells:?}");
}

#[test]
fn cubes_that_stay_end_the_pocket_after_its_hold() {
    let mut w = nether();
    kit(&mut w);
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    add_cube(&mut w, 50, [11.5, 64.0, 8.5]);
    add_cube(&mut w, 51, [8.5, 64.0, 12.5]);
    let mut p = racer();
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    run(&mut p, &mut w, 100, home::CUBE_HOLD_MAX + 20);
    assert!(
        noted(&p, "home: out of the cube pocket, the cubes stay"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(
        p.ender.home.cube_pockets, 1,
        "no second pocket while it rests"
    );
    assert_eq!(p.ender.home.cube_breakouts, 1);
}

#[test]
fn the_cube_tunnel_runs_toward_the_point_away_from_the_cubes() {
    let feet = (404, 38, 171);
    // Cubes outside the east wall, the next point west: west.
    let east = [[407.5, 38.0, 171.5], [408.0, 38.0, 170.0]];
    assert_eq!(home::cube_tunnel_dir(feet, (226, 194), &east), (-1, 0));
    // Cubes west, the point west: the other axis toward the point (south, +z).
    let west = [[401.0, 38.0, 171.5]];
    assert_eq!(home::cube_tunnel_dir(feet, (226, 194), &west), (0, 1));
    // The pocket's breakout takes the tunnel and notes it.
    let mut w = nether();
    kit(&mut w);
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    add_cube(&mut w, 50, [11.5, 64.0, 8.5]);
    add_cube(&mut w, 51, [12.5, 64.0, 9.5]);
    let mut p = racer();
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    run(&mut p, &mut w, 100, home::CUBE_HOLD_MAX + 20);
    assert!(
        noted(&p, "home: left the cube pocket by tunnel"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.ender.home.cube_tunnels, 1);
}

#[test]
fn a_stack_left_on_the_cursor_is_put_down() {
    // Race 45715674818 lane 0: netherrack 33 on the cursor, stairs stood 1,200 ticks.
    let mut w = nether();
    kit(&mut w);
    w.inventory.carried = Some((ItemKind::Netherrack, 33));
    let mut p = racer();
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    let mut put = false;
    for t in 100..260 {
        w.ticks = t;
        w.session.server_ticks = t;
        p.step(&w);
        put |= p.take_acts().iter().any(|a| {
            matches!(
                a,
                world::Act::Click {
                    button: 0,
                    mode: 0,
                    ..
                }
            )
        });
    }
    assert!(
        noted(&p, "cursor put down (Netherrack 33)"),
        "{:?}",
        notes(&p)
    );
    assert!(put, "a click puts it in a free slot");
    assert_eq!(p.cursor_puts, 1);
}

#[test]
fn a_laid_bridge_block_is_kept_from_its_note() {
    assert_eq!(home::parse_cell("(293, 37, 188)"), Some((293, 37, 188)));
    assert_eq!(home::parse_cell("(-3, 37, -188)"), Some((-3, 37, -188)));
    assert_eq!(home::parse_cell("(1, 2)"), None);
    assert_eq!(home::parse_cell("(1, 2, 3, 4)"), None);
}

#[test]
fn resumed_mid_trip_with_the_eyes_made_skips_the_top_up_and_points_behind() {
    // Race 45725323112: a snapshot on the trip held 14 eyes and the kit, Home began afresh and
    // walked 258 back to the hunt point.
    let mut w = nether();
    kit(&mut w);
    w.inventory.slots.remove(&9);
    w.inventory.slots.insert(9, (ItemKind::EnderEye, 14));
    w.inventory.slots.insert(11, (ItemKind::Stick, 9));
    w.inventory.slots.insert(12, (ItemKind::CraftingTable, 1));
    let mut p = racer();
    run(&mut p, &mut w, 100, 2);
    assert_eq!(p.ender.phase, EnderPhase::Home, "{:?}", notes(&p));
    assert!(
        noted(&p, "home: resumed with the eyes made"),
        "{:?}",
        notes(&p)
    );
    assert!(p.ender.home.top_up_over && p.ender.home.kit_over);
    // At (8, 64, 8) the nearest points are the last by the portal: every one before them is behind.
    let way = home::HomeRules::default().way;
    let passed = &p.ender.home.ways_passed;
    assert!(passed.len() + 2 >= way.len(), "{passed:?}");
    assert!(
        passed.contains(&(way[0][0] >> 4, way[0][1] >> 4)),
        "{passed:?}"
    );
}

#[test]
fn no_way_point_or_leg_comes_within_24_of_the_blaze_spawner() {
    // The spawner by the fortress, about (130, 61, 167) (race 45350933958 lane 0's senses).
    let s = (130.0_f64, 167.0_f64);
    let way = home::HomeRules::default().way;
    let near = |a: [i32; 2], b: [i32; 2]| {
        // The least distance on the plane from the spawner to the leg from a to b.
        let (ax, az) = (f64::from(a[0]), f64::from(a[1]));
        let (dx, dz) = (f64::from(b[0]) - ax, f64::from(b[1]) - az);
        let len = dx * dx + dz * dz;
        let t = if len == 0.0 {
            0.0
        } else {
            (((s.0 - ax) * dx + (s.1 - az) * dz) / len).clamp(0.0, 1.0)
        };
        ((ax + t * dx - s.0).powi(2) + (az + t * dz - s.1).powi(2)).sqrt()
    };
    for p in &way {
        assert!(near(*p, *p) >= 24.0, "{p:?} is {:.1} off", near(*p, *p));
    }
    for leg in way.windows(2) {
        let d = near(leg[0], leg[1]);
        assert!(d >= 24.0, "{:?} to {:?} comes {d:.1} near", leg[0], leg[1]);
    }
}

#[test]
fn a_blaze_in_sight_on_the_trip_is_never_struck() {
    let mut w = nether();
    kit(&mut w);
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    w.step(&Event::Packet(S2c::AddEntity {
        id: 60,
        uuid: [0; 16],
        kind: data::EntityKind::Blaze.id(),
        x: 14.5,
        y: 66.0,
        z: 8.5,
        velocity: [0.0; 3],
    }));
    let mut p = racer();
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    run(&mut p, &mut w, 100, 40);
    let n = notes(&p);
    assert!(
        !n.iter().any(|s| s.starts_with("defend Blaze")),
        "never a fight with a blaze in Home: {n:?}"
    );
    assert!(!matches!(p.task, Task::Defend(_)), "{}", p.task.name());
}

#[test]
fn a_skeleton_in_sight_on_the_trip_is_a_shooter() {
    let mut w = nether();
    kit(&mut w);
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    w.step(&Event::Packet(S2c::AddEntity {
        id: 61,
        uuid: [0; 16],
        kind: data::EntityKind::Skeleton.id(),
        x: 16.5,
        y: 64.0,
        z: 8.5,
        velocity: [0.0; 3],
    }));
    assert!(home::skeleton_sees(&w));
    let mut p = racer();
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    run(&mut p, &mut w, 100, 40);
    assert!(p.ender.home.shooter_seen_at.is_some());
    assert!(
        !notes(&p).iter().any(|s| s.starts_with("defend Skeleton")),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn a_ledge_is_air_beside_and_under() {
    let mut w = nether();
    assert!(!home::on_ledge(&w), "the flat floor is no ledge");
    set(&mut w, (9, 63, 8), BlockKind::Air);
    assert!(home::on_ledge(&w), "air beside the feet and under it");
}

#[test]
fn the_gap_ahead_is_measured_to_the_next_floor() {
    let mut w = nether();
    assert_eq!(plan::heading::gap_ahead(&w, (8, 64, 8), (1, 0)), 0);
    for x in 10..=21 {
        set(&mut w, (x, 63, 8), BlockKind::Air);
    }
    assert_eq!(plan::heading::gap_ahead(&w, (8, 64, 8), (1, 0)), 12);
}

#[test]
fn a_cube_near_with_lava_beside_the_feet_lays_a_rail_first() {
    // Race 45733362772 lane 0: knocked into lava by a magma cube 560 ticks after the resume.
    let mut w = nether();
    kit(&mut w);
    w.inventory.slots.insert(36, (ItemKind::Netherrack, 64));
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    set(&mut w, (9, 63, 8), BlockKind::Lava);
    add_cube(&mut w, 50, [8.5, 64.0, 13.5]);
    let mut p = racer();
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    run(&mut p, &mut w, 100, 3);
    assert!(
        noted(&p, "home: rail beside the feet at (9, 63, 8)"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.ender.home.rails, 1);
    assert!(matches!(p.task, Task::Idle), "{}", p.task.name());
}

#[test]
fn the_old_bridges_step_is_built_and_walked_west() {
    // A copy of the chain's bridge step at z 194 (Q1's probe 45734754593) moved by (-300, +27,
    // -186): tops x 11 at 63, x 10 at 62, x 9 at 65, x 8 at 64, x 7 to 1 at 67, along z 8.
    let mut w = nether();
    kit(&mut w);
    w.inventory.slots.insert(36, (ItemKind::Netherrack, 64));
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    set(&mut w, (10, 63, 8), BlockKind::Air);
    for y in 64..=65 {
        set(&mut w, (9, y, 8), BlockKind::Basalt);
    }
    set(&mut w, (8, 64, 8), BlockKind::Basalt);
    for x in 1..=7 {
        for y in 64..=67 {
            set(&mut w, (x, y, 8), BlockKind::Netherrack);
        }
    }
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/goals.toml"
    ))
    .unwrap_or_default();
    let mut goals: make::Goals =
        toml::from_str(&text).unwrap_or_else(|e| panic!("goals.toml: {e}"));
    goals.home.bridges = vec![[15, 11, 64, 8], [7, 1, 68, 8]];
    goals.home.way = vec![[-200, 8]];
    let t = Arc::new(make::Table::new(&goals).unwrap_or_else(|e| panic!("table: {e}")));
    let mut p = Plan::with_table(Aim::Race, t);
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    w.body = w.body.map(|b| physics::Body {
        pos: [11.5, 64.0, 8.5],
        ..b
    });
    w.player.pos = Some([11.5, 64.0, 8.5]);
    // The world answers each placement on the clicked block's top.
    for t in 100..260 {
        w.ticks = t;
        p.step(&w);
        for a in p.take_acts() {
            if let world::Act::UseOn { pos, face: 1 } = a {
                set(&mut w, (pos.x, pos.y + 1, pos.z), BlockKind::Netherrack);
            }
        }
    }
    for c in ["(10, 63, 8)", "(10, 64, 8)", "(8, 65, 8)", "(8, 66, 8)"] {
        assert!(
            noted(&p, &format!("home: step block at {c}")),
            "{c}: {:?}",
            notes(&p)
        );
    }
    assert!(
        noted(&p, "home: on the old bridge to (7, 68, 8)"),
        "the walk up onto the west span: {:?}",
        notes(&p)
    );
}

#[test]
fn a_known_span_is_never_a_restock_cell() {
    // Q1's probe 45740087576: the bot's own restock dug the old bridge's step and 7 cells of it.
    let span = [400, 311, 37, 194];
    assert!(home::near_span(span, (320, 36, 194)), "the span's floor");
    assert!(
        home::near_span(span, (315, 37, 195)),
        "beside its line at its height"
    );
    assert!(
        home::near_span(span, (310, 36, 194)),
        "one past its west end"
    );
    assert!(!home::near_span(span, (320, 36, 197)), "three off the line");
    assert!(!home::near_span(span, (320, 30, 194)), "well under it");
}

#[test]
fn a_lava_fall_beside_the_portal_is_sealed_from_the_top() {
    // Q6's race 45742933645: the foot of a fall sealed, the fall spread over it into the
    // standing cell.
    let mut w = nether();
    kit(&mut w);
    w.inventory.slots.insert(36, (ItemKind::Netherrack, 64));
    for y in 64..=65 {
        set(&mut w, (11, y, 8), BlockKind::NetherPortal);
        remember(&mut w, home::NETHER, (11, y, 8), BlockKind::NetherPortal);
    }
    set(&mut w, (11, 63, 8), BlockKind::Obsidian);
    // A fall beside the portal's foot: lava at (11, 64, 9) with lava over it at (11, 65, 9).
    set(&mut w, (11, 64, 9), BlockKind::Lava);
    set(&mut w, (11, 65, 9), BlockKind::Lava);
    set(&mut w, (11, 66, 9), BlockKind::Netherrack);
    let mut p = racer();
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    // Q6's door box ("portal way: door boxed") runs before the seal; whatever goes first, the
    // fall's foot is never sealed while lava stands over it.
    run(&mut p, &mut w, 100, 40);
    assert!(
        !noted(&p, "home: lava sealed at (11, 64, 9)"),
        "never the foot of the fall: {:?}",
        notes(&p)
    );
}

#[test]
fn the_join_west_of_a_span_is_never_a_restock_cell() {
    // Race 45749508716 lane 2 dug its own step at (309, 34, 194) and (310, 34, 194).
    let span = [400, 311, 37, 194];
    assert!(home::near_span(span, (309, 34, 194)));
    assert!(home::near_span(span, (310, 34, 194)));
    assert!(home::near_span(span, (308, 39, 194)));
}

/// A Home trip plan, the top-up and kit behind it, from `t` (the default table, or `t`).
fn tripper(t: Option<Arc<make::Table>>) -> Plan {
    let mut p = Plan::with_table(Aim::Race, t.unwrap_or_else(table));
    p.ender.phase = EnderPhase::Home;
    p.ender.home.since = Some(0);
    p.ender.home.top_up_over = true;
    p.ender.home.kit_begun = true;
    p.ender.home.kit_over = true;
    p
}

/// A cube of `size` 2 blocks east of the feet at (8, 64, 8), lava beside the feet (a span's
/// sides), no filler to rail or guard with; how long the plan defends in 20 ticks.
fn defends_a_cube_on_an_edge(size: i32) -> (u32, Plan) {
    let mut w = nether();
    kit(&mut w);
    w.inventory.slots.remove(&38);
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    set(&mut w, (8, 63, 7), BlockKind::Lava);
    set(&mut w, (8, 63, 9), BlockKind::Lava);
    add_cube(&mut w, 50, [10.0, 64.0, 8.5]);
    if size > 1 {
        w.step(&Event::Packet(S2c::SetEntityData {
            id: 50,
            entries: vec![(16, protocol::metadata::MetaValue::Int(size))],
        }));
    }
    assert_eq!(home::cube_size(&w.entities[&50]), size);
    assert!(home::at_lava_edge(&w));
    let mut p = tripper(None);
    let mut defending = 0;
    for t in 100..120 {
        w.ticks = t;
        p.step(&w);
        p.take_acts();
        if matches!(p.task, Task::Defend(_)) {
            defending += 1;
        }
    }
    (defending, p)
}

#[test]
fn a_large_cube_by_a_lava_edge_is_never_struck() {
    // Race 45749508716 lane 3 struck a large one on the west span at x 253; it split into four
    // and knocked the bot into the lava 40 ticks later.
    let (defending, p) = defends_a_cube_on_an_edge(4);
    assert_eq!(defending, 0, "{:?}", notes(&p));
    assert!(
        noted(&p, "home: a large cube not struck by a lava edge"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.ender.home.big_cubes_spared, 1);
    // A medium one too; a small one (no split) is fought as before.
    assert_eq!(defends_a_cube_on_an_edge(2).0, 0);
    assert!(
        defends_a_cube_on_an_edge(1).0 > 0,
        "a small cube on the bot is fought"
    );
}

#[test]
fn rails_are_laid_under_the_keep_down_to_none() {
    // Race 45749508716 lane 3 held 10 filler, under the keep of 24, and laid no rail.
    let mut w = nether();
    kit(&mut w);
    w.inventory.slots.insert(38, (ItemKind::Cobblestone, 10));
    set(&mut w, (9, 63, 8), BlockKind::Lava);
    add_cube(&mut w, 50, [8.5, 64.0, 13.5]);
    assert!(home::rail_wanted(&w).is_some(), "10 held: a rail");
    w.inventory.slots.insert(38, (ItemKind::Cobblestone, 1));
    assert!(home::rail_wanted(&w).is_some(), "1 held: a rail");
    w.inventory.slots.remove(&38);
    assert!(home::rail_wanted(&w).is_none(), "none held: none");
}

#[test]
fn a_span_walked_out_is_never_walked_back_to() {
    // Race 45753006452 lane 0 paced between the west span's end (201, 41, 194) and a restock walk
    // 4 east for 2,500 ticks: the span's walk sent it back to the end each time.
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/goals.toml"
    ))
    .unwrap_or_default();
    let mut goals: make::Goals =
        toml::from_str(&text).unwrap_or_else(|e| panic!("goals.toml: {e}"));
    goals.home.bridges = vec![[15, 8, 64, 8]];
    goals.home.way = vec![[-200, 8]];
    let t = Arc::new(make::Table::new(&goals).unwrap_or_else(|e| panic!("table: {e}")));
    let mut w = nether();
    kit(&mut w);
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    let mut p = tripper(Some(t));
    run(&mut p, &mut w, 100, 2);
    assert!(
        noted(&p, "home: the old bridges walked out at (8, 64, 8)"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.ender.home.spans_out, vec![0]);
    // Back on the span 4 east: no walk along it to its end again.
    w.body = w.body.map(|b| physics::Body {
        pos: [12.5, 64.0, 8.5],
        ..b
    });
    w.player.pos = Some([12.5, 64.0, 8.5]);
    p.task = Task::Idle;
    run(&mut p, &mut w, 102, 2);
    assert!(!noted(&p, "home: on the old bridge to"), "{:?}", notes(&p));
}

#[test]
fn on_the_way_home_a_tunnel_never_cuts_nether_bricks() {
    // Race 45753006452 lane 1 tunnelled through a fortress's bricks into a corridor at (72, 61,
    // 150): two wither skeleton blows and the wither, 19.8 to 5.6.
    let mut w = nether();
    for x in 2..=14 {
        for y in 50..=70 {
            set(&mut w, (x, y, 14), BlockKind::NetherBricks);
        }
    }
    let none = std::collections::BTreeSet::new();
    let route = |bricks_wall| {
        plan::tunnel::route_keeping(
            &w,
            (8, 52, 8),
            &[(8, 52, 20)],
            &none,
            false,
            plan::tunnel::MAX_NODES,
            plan::tunnel::Limits {
                bricks_wall,
                ..plan::tunnel::Limits::default()
            },
        )
    };
    let brick =
        |c: &(i32, i32, i32)| (2..=14).contains(&c.0) && (50..=70).contains(&c.1) && c.2 == 14;
    let through = route(false).unwrap_or_else(|| panic!("no route"));
    assert!(through.found);
    assert!(
        through.cells.iter().any(brick),
        "straight through: {:?}",
        through.cells
    );
    let round = route(true).unwrap_or_else(|| panic!("no route"));
    assert!(round.found, "round the wall: {:?}", round.cells);
    assert!(!round.cells.iter().any(brick), "{:?}", round.cells);
    // The head cell over each feet cell too: never a brick.
    assert!(
        !round.cells.iter().any(|c| brick(&(c.0, c.1 + 1, c.2))),
        "{:?}",
        round.cells
    );
}

/// The feet at (8, 64, 8) on a one-wide line along x: the cells beside it at z 7 and 9 open down
/// to lava at y 61 (3 under the feet's level), for x 0 to 16.
fn span_over_lava() -> World {
    let mut w = nether();
    for x in 0..=16 {
        for z in [7, 9] {
            set(&mut w, (x, 63, z), BlockKind::Air);
            set(&mut w, (x, 62, z), BlockKind::Air);
            set(&mut w, (x, 61, z), BlockKind::Lava);
        }
    }
    w
}

#[test]
fn the_guard_lays_a_rail_then_a_block_at_the_feet_on_each_lava_side() {
    // Race 45758211666 lane 1: a cube spawned beside it on the west span, 9 over the sea, and a
    // hit put it in the lava. The guard is laid mob or no mob.
    let mut w = span_over_lava();
    kit(&mut w);
    let axis = Some((-1, 0));
    assert_eq!(
        home::guard_wanted(&w, axis),
        Some(((8, 63, 7), (8, 63, 8), 2)),
        "the rail first, on the side of the block stood on"
    );
    set(&mut w, (8, 63, 7), BlockKind::Cobblestone);
    assert_eq!(
        home::guard_wanted(&w, axis),
        Some(((8, 64, 7), (8, 63, 7), 1)),
        "then the guard on top of it"
    );
    set(&mut w, (8, 64, 7), BlockKind::Cobblestone);
    assert_eq!(
        home::guard_wanted(&w, axis).map(|g| g.0),
        Some((8, 63, 9)),
        "then the other side"
    );
    // The next cell west: clicked onto the guard laid beside it, no rail.
    set(&mut w, (8, 64, 7), BlockKind::Air);
    set(&mut w, (9, 64, 7), BlockKind::Cobblestone);
    assert_eq!(
        home::guard_wanted(&w, axis),
        Some(((8, 64, 7), (9, 64, 7), 4))
    );
    // No axis, or no filler: none (Bridger: down to 0).
    assert_eq!(home::guard_wanted(&w, None), None);
    w.inventory.slots.insert(38, (ItemKind::Cobblestone, 1));
    assert!(home::guard_wanted(&w, axis).is_some(), "one held: laid");
    w.inventory.slots.remove(&38);
    assert_eq!(home::guard_wanted(&w, axis), None);
}

#[test]
fn on_the_trip_the_guard_is_laid_with_no_mob_near() {
    let mut w = span_over_lava();
    kit(&mut w);
    w.inventory.slots.insert(36, (ItemKind::Netherrack, 64));
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    let mut p = tripper(None);
    p.ender.home.axis = Some((-1, 0));
    p.ender.home.last_feet = Some((8, 64, 8));
    run(&mut p, &mut w, 100, 3);
    assert!(
        noted(&p, "home: guard laid at (8, 63, 7)"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.ender.home.guards, 1);
}

#[test]
fn the_restock_never_digs_the_floor_near_over_the_lava_sea() {
    // Race 45758211666 lane 2 dug the island by the join down to y 32 and 33 over the sea at 31.
    let mut w = nether();
    for x in 5..=11 {
        for z in 5..=11 {
            set(&mut w, (x, 63, z), BlockKind::Basalt);
        }
    }
    assert!(
        !plan::heading::restock_cells(&w, Some((1, 0)), plan::heading::RESTOCK_LOOK).is_empty()
    );
    // The sea's top 2 under the floor, 6 off on the plane.
    set(&mut w, (14, 61, 8), BlockKind::Lava);
    let cells = plan::heading::restock_cells(&w, Some((1, 0)), plan::heading::RESTOCK_LOOK);
    assert!(cells.iter().all(|c| c.1 >= 64), "{cells:?}");
}

#[test]
fn by_a_lava_edge_over_the_danger_line_no_cube_pocket_is_sealed() {
    // Race 45758211666 lanes 3 and 4 sat 4,000 to 16,000 ticks in pockets on the join's island.
    let mut w = span_over_lava();
    kit(&mut w);
    w.inventory.slots.remove(&38);
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    add_cube(&mut w, 50, [11.5, 64.0, 8.5]);
    add_cube(&mut w, 51, [5.5, 64.0, 8.5]);
    let mut p = tripper(None);
    run(&mut p, &mut w, 100, 5);
    assert!(
        noted(
            &p,
            "home: no cube pocket by a lava edge, on behind the guard"
        ),
        "{:?}",
        notes(&p)
    );
    assert!(p.ender.home.cube_pocket.is_none());
    assert_eq!(p.ender.home.cube_pockets, 0);
}

#[test]
fn a_guard_the_server_never_answers_is_given_up() {
    // Race 45760537530: one guard clicked 221 to 411 times, filler unchanged, under blazes.
    let mut w = span_over_lava();
    kit(&mut w);
    w.inventory.slots.insert(36, (ItemKind::Netherrack, 64));
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    let mut p = tripper(None);
    p.ender.home.axis = Some((-1, 0));
    p.ender.home.last_feet = Some((8, 64, 8));
    // The world never answers: every click is refused.
    run(&mut p, &mut w, 100, 200);
    assert!(
        noted(&p, "home: guard given up at (8, 63, 7)"),
        "{:?}",
        notes(&p)
    );
    assert!(p.ender.home.guard_refused.contains(&(8, 63, 7)));
    assert!(
        p.ender.home.guards <= 2 * (home::GUARD_TRIES + 1),
        "{} clicks",
        p.ender.home.guards
    );
}

#[test]
fn no_guard_while_a_restock_is_in_hand() {
    // Race 45764179359: the guard filled the cells the restock dug, filler flat for 4,000 ticks.
    let mut w = span_over_lava();
    kit(&mut w);
    w.inventory.slots.insert(36, (ItemKind::Netherrack, 64));
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    let mut p = tripper(None);
    p.ender.home.axis = Some((-1, 0));
    p.ender.home.last_feet = Some((8, 64, 8));
    let mut h = plan::heading::Heading::new((-1, 0), (8, 64, 8), 0, "test".to_owned());
    h.restock = Some(plan::heading::Restock {
        to: 152,
        ..plan::heading::Restock::default()
    });
    p.headed = Some(h);
    // The trip already begun (its start drops the heading), one tick with the restock in hand.
    p.ender.home.trip_begun = true;
    run(&mut p, &mut w, 100, 1);
    assert_eq!(p.ender.home.guards, 0, "{:?}", notes(&p));
}

#[test]
fn two_below_a_span_a_step_is_laid_up_onto_it() {
    // Q10's races 45764230971 and 45764234506: 8 of 8 lanes at y 34 under the span top at 36,
    // "tunnel route ... 0 steps, ends within reach", every tick.
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/goals.toml"
    ))
    .unwrap_or_default();
    let mut goals: make::Goals =
        toml::from_str(&text).unwrap_or_else(|e| panic!("goals.toml: {e}"));
    goals.home.bridges = vec![[15, 1, 66, 10]];
    goals.home.way = vec![[-200, 10]];
    let t = Arc::new(make::Table::new(&goals).unwrap_or_else(|e| panic!("table: {e}")));
    let mut w = nether();
    kit(&mut w);
    w.inventory.slots.insert(36, (ItemKind::Netherrack, 64));
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    // The span: a one-wide line of blocks at y 64 and 65 along z 10, its top at 66.
    for x in 1..=15 {
        for y in 64..=65 {
            set(&mut w, (x, y, 10), BlockKind::Netherrack);
        }
    }
    let mut p = tripper(Some(t));
    run(&mut p, &mut w, 100, 5);
    assert!(
        noted(&p, "home: step up toward the old bridge at (8, 64, 9)"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn a_provoked_enderman_on_the_way_home_is_held_from_at_most_a_minute() {
    // Q9's race 45764756002 lane 2: provoked in the tick the top-up ended on its clock, then a
    // 0-block pocket in the forest from 6,040 past 23,440 at 19.8 health, the trip never begun.
    let mut w = nether();
    kit(&mut w);
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    w.step(&Event::Packet(S2c::AddEntity {
        id: 70,
        uuid: [0; 16],
        kind: data::EntityKind::Enderman.id(),
        x: 11.5,
        y: 64.0,
        z: 8.5,
        velocity: [0.0; 3],
    }));
    w.step(&Event::Packet(S2c::SetEntityData {
        id: 70,
        entries: vec![(
            plan::endermen::CREEPY_INDEX,
            protocol::metadata::MetaValue::Bool(true),
        )],
    }));
    assert_eq!(plan::endermen::angry(&w), Some(70));
    let mut p = tripper(None);
    run(&mut p, &mut w, 100, home::ENDER_HOLD_MAX + 40);
    assert!(
        noted(&p, "home: left the pocket from enderman 70"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.ender.home.enders_left, vec![70]);
    // Then the trip: no new hold from it.
    assert!(noted(&p, "home: the trip begun"), "{:?}", notes(&p));
}

#[test]
fn a_full_pack_on_the_way_home_frees_a_slot_and_drops_the_nether_junk() {
    // Q11's race 45766983995: 448 netherrack and 50 soul soil, 0 main slots free, every wooden
    // pickaxe craft failed StaleWindow.
    let mut w = nether();
    kit(&mut w);
    for s in 9..=35 {
        w.inventory.slots.insert(s, (ItemKind::Netherrack, 64));
    }
    w.inventory.slots.insert(39, (ItemKind::SoulSoil, 50));
    for s in 40..=44 {
        w.inventory.slots.insert(s, (ItemKind::Netherrack, 64));
    }
    assert_eq!(plan::base::free_slots(&w), 0);
    let (slot, kind) = plan::base::home_free_slot(&w).unwrap_or_else(|| panic!("a slot"));
    assert_eq!(kind, ItemKind::Netherrack, "the kind held most");
    assert!((9..=44).contains(&slot));
    // The trip drops the leftovers: soul soil goes (keep 0), netherrack down to its 128.
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    let mut p = tripper(None);
    let mut dropped = Vec::new();
    for t in 100..140 {
        w.ticks = t;
        p.step(&w);
        for a in p.take_acts() {
            if let world::Act::Click { slot, mode: 4, .. } = a {
                dropped.push(w.inventory.slots.get(&slot).map(|(k, _)| *k));
                w.inventory.slots.remove(&slot);
            }
        }
    }
    assert!(!dropped.is_empty(), "{:?}", notes(&p));
    assert!(plan::base::free_slots(&w) > 0);
}

#[test]
fn on_the_join_between_spans_the_next_span_is_the_target() {
    // Race 45768618919: four lanes stood on the join's island at x 309 and 310, in no span's
    // reach, the heading bridging the sea instead.
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/goals.toml"
    ))
    .unwrap_or_default();
    let mut goals: make::Goals =
        toml::from_str(&text).unwrap_or_else(|e| panic!("goals.toml: {e}"));
    goals.home.bridges = vec![[15, 11, 64, 8], [7, 1, 68, 8]];
    goals.home.way = vec![[-200, 8]];
    let t = Arc::new(make::Table::new(&goals).unwrap_or_else(|e| panic!("table: {e}")));
    let mut w = nether();
    kit(&mut w);
    w.inventory.slots.insert(36, (ItemKind::Netherrack, 64));
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    w.body = w.body.map(|b| physics::Body {
        pos: [9.5, 64.0, 12.5],
        ..b
    });
    w.player.pos = Some([9.5, 64.0, 12.5]);
    let mut p = tripper(Some(t));
    run(&mut p, &mut w, 100, 3);
    // The next span, 4 up and 4 north: a stair toward it, its first step at the feet's level.
    let why: Vec<String> = p.events.iter().map(|e| e.why.clone()).collect();
    assert!(
        noted(&p, "home: step up toward the old bridge at (9, 64, 11)")
            && why.iter().any(|y| y.contains("(7, 68, 8)")),
        "{:?} {why:?}",
        notes(&p)
    );
}

#[test]
fn four_below_a_span_one_off_its_line_the_stair_runs_along_it() {
    // Race 45768618919: lanes at y 34 to 37 beside and under the west span (top 41), one off its
    // line: no room to climb toward it, so the stair runs west along it.
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/goals.toml"
    ))
    .unwrap_or_default();
    let mut goals: make::Goals =
        toml::from_str(&text).unwrap_or_else(|e| panic!("goals.toml: {e}"));
    goals.home.bridges = vec![[15, 1, 68, 9]];
    goals.home.way = vec![[-200, 9]];
    let t = Arc::new(make::Table::new(&goals).unwrap_or_else(|e| panic!("table: {e}")));
    let mut w = nether();
    kit(&mut w);
    w.inventory.slots.insert(36, (ItemKind::Netherrack, 64));
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    for x in 1..=15 {
        for y in 66..=67 {
            set(&mut w, (x, y, 9), BlockKind::Netherrack);
        }
    }
    let mut p = tripper(Some(t));
    run(&mut p, &mut w, 100, 5);
    assert!(
        noted(&p, "home: step up toward the old bridge at (7, 64, 8)"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn short_of_pickaxes_a_stem_seen_near_is_gathered_by_hand() {
    // Q10's race 45768707240 lane 2: 17 off the portal at 15.6 health with 0 pickaxe uses, the
    // kit's stems spent, a crimson forest round it.
    let mut w = nether();
    kit(&mut w);
    w.inventory.slots.remove(&37);
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    set(&mut w, (14, 64, 8), BlockKind::CrimsonStem);
    remember(&mut w, home::NETHER, (14, 64, 8), BlockKind::CrimsonStem);
    let mut p = tripper(None);
    p.ender.home.trip_begun = true;
    run(&mut p, &mut w, 100, 3);
    assert!(
        noted(&p, "home: wood by hand at (14, 64, 8)"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.ender.home.wood_walks, 1);
}

#[test]
fn a_ghast_that_sees_the_bot_cuts_the_walk_for_cover() {
    // Q10's race 45768707240 lane 1: three ghast fireballs in a level tunnel, no cover note.
    let mut w = nether();
    kit(&mut w);
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    w.step(&Event::Packet(S2c::AddEntity {
        id: 80,
        uuid: [0; 16],
        kind: data::EntityKind::Ghast.id(),
        x: 8.5,
        y: 72.0,
        z: 40.5,
        velocity: [0.0; 3],
    }));
    assert!(home::ghast_sees(&w));
    let mut p = tripper(None);
    p.ender.home.trip_begun = true;
    p.task = Task::Explore(plan::walk::Walk::new(
        (-20, 64, 8),
        plan::path::Goal::Near {
            pos: (-20, 64, 8),
            range: 1.0,
        },
        plan::walk::Arrive::Goal,
        100,
    ));
    run(&mut p, &mut w, 100, 1);
    assert!(noted(&p, "home: explore cut for cover"), "{:?}", notes(&p));
    assert_eq!(p.ender.home.shooter_cuts, 1);
}

#[test]
fn the_trips_pickaxe_run_refuses_a_far_table() {
    // Q9's race 45770564199: each run walked 35 to 50 back east to the forest's table though the
    // bot carried one.
    let mut w = nether();
    kit(&mut w);
    remember(&mut w, home::NETHER, (-300, 50, 8), BlockKind::NetherPortal);
    set(&mut w, (40, 64, 8), BlockKind::CraftingTable);
    remember(&mut w, home::NETHER, (40, 64, 8), BlockKind::CraftingTable);
    w.inventory.slots.insert(12, (ItemKind::CraftingTable, 1));
    let mut p = tripper(None);
    p.ender.home.trip_begun = true;
    p.base = plan::base::BaseState {
        steps: home::TRIP_WOOD_STEPS.to_vec(),
        ..plan::base::BaseState::default()
    };
    run(&mut p, &mut w, 100, 2);
    assert!(
        noted(&p, "arrival: the table at (40, 64, 8) is far"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn arrived_in_the_portals_own_cell_the_bot_steps_out_south_first() {
    // Q11's race 45771600648: three bots stood in the portal at (125, -52, -108), "no stairs up
    // from here", the shaft never begun.
    let mut w = world(
        home::OVERWORLD,
        0,
        column(BlockKind::Stone, 6),
        [8.5, 40.0, 8.5],
    );
    kit(&mut w);
    // A portal along x: x 8 to 9, y 40 to 42, z 8, its frame round it; a room cut south of it.
    for y in 40..=42 {
        for x in 8..=9 {
            set(&mut w, (x, y, 8), BlockKind::NetherPortal);
        }
    }
    for x in 7..=10 {
        for y in 40..=41 {
            set(&mut w, (x, y, 9), BlockKind::Air);
        }
    }
    w.stage_reached = Some(world::stage::Stage::Pearls12);
    let mut p = racer();
    run(&mut p, &mut w, 100, 3);
    assert!(
        noted(&p, "home: out of the portal to (8, 40, 9)"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn a_tunnel_to_a_standing_cell_ends_with_the_feet_in_it() {
    // Q10's race 45774326055: six lanes at (22, 39, -16), 4 over the front's tunnel cell
    // (22, 35, -18): "0 steps (ends within reach)", 10,000 ticks idle at the door.
    let mut w = nether();
    set(&mut w, (8, 52, 8), BlockKind::Air);
    set(&mut w, (8, 53, 8), BlockKind::Air);
    let w = w;
    let none = std::collections::BTreeSet::new();
    let route = |feet_in| {
        plan::tunnel::route_keeping(
            &w,
            (8, 52, 8),
            &[(8, 48, 10)],
            &none,
            false,
            plan::tunnel::MAX_NODES,
            plan::tunnel::Limits {
                feet_in,
                ..plan::tunnel::Limits::default()
            },
        )
    };
    let near = route(false).unwrap_or_else(|| panic!("no route"));
    assert!(
        near.cells.len() <= 1,
        "within reach already: {:?}",
        near.cells
    );
    let r = route(true).unwrap_or_else(|| panic!("no route"));
    assert!(r.found, "{:?}", r.cells);
    assert!(r.cells.len() > 1, "{:?}", r.cells);
    assert_eq!(r.cells.last(), Some(&(8, 48, 10)), "{:?}", r.cells);
}
