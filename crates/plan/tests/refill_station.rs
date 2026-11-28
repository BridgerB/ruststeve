//! Issue 83 wall 4 (docs/problems/lava-portal.md): race 46144128199 lane 0's refill at (24, -54,
//! 43). The station's stand had twelve sources in reach, every one behind a wall of deepslate
//! level with the feet and the head, with lava under the wall's foot: from the stand no face was
//! in sight and no lid lay under the feet, so the station step ended in the tick it began, the
//! level tunnel turned off the lava and stopped on a walk onto air with nothing dug, and the
//! ladder went round four times in 46 ticks to "no route". Lane 1 filled at the same site once
//! its way dig had cut that wall. The stand now digs the wall's cell on its line to the source
//! (the station sight dig) and fills.
//!
//! The read's shape, hand-built: stone to y 63, air from y 64; the frame at (0, 63, 0) facing
//! south, so the h 1 stand in front of it is (0, 64, 4); a wall at x -1, y 64 and 65, along the
//! whole span the station searches; lava under the wall's foot at (-1, 63, 4); the pool's
//! sources at x -3 and -2, z 2 to 6, y 63, in flowing lava from x -14, open to the air above.

use std::sync::Arc;

use data::{BlockKind, EntityKind, ItemKind};
use plan::build::{Facing, Frame};
use plan::cast::{Cast, CastStatus, LavaFirst, SightHold, Site};
use plan::make::{Goals, Table};
use plan::station::Level;
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use world::{Act, Event, World};

type Cell = (i32, i32, i32);

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

fn set(w: &mut World, c: Cell, k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: c,
        state: state(k),
    }));
}

const STAND: Cell = (0, 64, 4);
/// The bucket's inventory slot (hotbar 2).
const BUCKET: i16 = 38;

fn frame() -> Frame {
    Frame {
        origin: (0, 63, 0),
        facing: Facing::South,
    }
}

fn site() -> Site {
    Site {
        frame: frame(),
        lava: (-2, 63, 4),
        h: 1,
    }
}

/// The pool's sources, a few at the wall (memory keeps `POOL_CELLS` a chunk column).
fn pool() -> Vec<Cell> {
    (2..=6).flat_map(|z| [(-3, 63, z), (-2, 63, z)]).collect()
}

/// Flowing lava round the sources, wide enough that no stand beyond the pool is within the
/// station's span (no firm floor): in the read the pool's far bank was lava too.
fn flowing() -> Vec<Cell> {
    (-4..=16)
        .flat_map(|z| (-14..=-2).map(move |x| (x, 63, z)))
        .collect()
}

/// The world of the read, the wall of `wall` (none when `Air`); the feet on the stand, the kit
/// held with an empty bucket for the lava.
fn world(wall: BlockKind) -> World {
    let mut sections = vec![section(BlockKind::Stone); 4];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 12));
    let col = Arc::new(Column { min_y: 0, sections });
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 0,
        dimension: "minecraft:overworld".to_owned(),
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
    // Level 1: the state after the source's (the block's states run by level).
    for c in flowing() {
        w.step(&Event::Packet(S2c::BlockUpdate {
            pos: c,
            state: state(BlockKind::Lava) + 1,
        }));
    }
    for c in pool() {
        set(&mut w, c, BlockKind::Lava);
    }
    set(&mut w, (-1, 63, 4), BlockKind::Lava);
    if wall != BlockKind::Air {
        for z in -12..=20 {
            set(&mut w, (-1, 64, z), wall);
            set(&mut w, (-1, 65, z), wall);
        }
    }
    let pos = [0.5, 64.0, 4.5];
    w.body = Some(physics::Body {
        pos,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(pos);
    w.player.health = Some(20.0);
    w.player.food = Some(20);
    w.inventory.slots.insert(36, (ItemKind::IronPickaxe, 1));
    w.inventory.slots.insert(37, (ItemKind::WaterBucket, 1));
    w.inventory.slots.insert(BUCKET, (ItemKind::Bucket, 1));
    w.inventory.slots.insert(39, (ItemKind::FlintAndSteel, 1));
    w.inventory.slots.insert(40, (ItemKind::Cobblestone, 32));
    w.held = 0;
    w
}

fn sources(w: &World) -> Vec<Cell> {
    plan::cast::lava_sources(w)
}

fn eye(c: Cell) -> [f64; 3] {
    [
        f64::from(c.0) + 0.5,
        f64::from(c.1) + physics::EYE_HEIGHT,
        f64::from(c.2) + 0.5,
    ]
}

#[test]
fn the_read_s_stand_sees_nothing_until_the_wall_is_dug() {
    let w = world(BlockKind::Stone);
    let srcs = sources(&w);
    assert!(pool().iter().all(|c| srcs.contains(c)), "{srcs:?}");
    // The site chooser's station: the stand by reach alone.
    let st =
        plan::station::station_among(&w, &srcs, frame(), &[], Some(1)).expect("a station by reach");
    assert_eq!(st.stand, STAND);
    let kept = plan::station::kept(frame());
    // From the stand no face is in the exact line and no lid lies under the feet: the steps the
    // stand had before ended in the tick they began.
    for s in &st.sources {
        assert_eq!(
            plan::scoop::aim_at_use(&w, eye(STAND), *s, plan::scoop::REACH),
            None,
            "{s:?}"
        );
        assert_eq!(
            plan::station::sight_block(&w, eye(STAND), *s, STAND.1, &kept),
            None,
            "{s:?}"
        );
    }
    // The wall's cell on the line, level with the head, a sight dig may take: lava under the
    // wall's foot never flows up.
    let (block, level) =
        plan::station::level_sight_block(&w, eye(STAND), (-2, 63, 4), STAND.1, &kept, &[])
            .expect("a level sight block");
    assert_eq!((block, level), ((-1, 65, 4), Level::Head));
    assert!(plan::station::serves(&w, STAND, (-2, 63, 4), &kept));
}

#[test]
fn behind_an_obsidian_wall_no_stand_serves() {
    // Obsidian is never dug: the stands behind it are counted blind and there is no station.
    // Taller than a stand on its top reaches down from (h 3 on a two-high wall saw the pool).
    let mut w = world(BlockKind::Obsidian);
    for z in -12..=20 {
        for y in 66..=70 {
            set(&mut w, (-1, y, z), BlockKind::Obsidian);
        }
    }
    let kept = plan::station::kept(frame());
    let (st, blind) =
        plan::station::serving_station(&w, &sources(&w), frame(), &[], Some(1), &kept);
    assert_eq!(st, None);
    assert!(blind > 0, "the stands passed over are counted: {blind}");
}

#[test]
fn a_wall_with_lava_beside_it_at_its_height_is_not_dug() {
    // Lava level with the wall's cell would run in: no sight dig takes it.
    let mut w = world(BlockKind::Stone);
    for z in -12..=20 {
        set(&mut w, (-2, 64, z), BlockKind::Lava);
        set(&mut w, (-2, 65, z), BlockKind::Lava);
    }
    let kept = plan::station::kept(frame());
    assert_eq!(
        plan::station::level_sight_block(&w, eye(STAND), (-2, 63, 4), STAND.1, &kept, &[]),
        None
    );
}

#[test]
fn with_the_wall_gone_the_stand_serves() {
    let w = world(BlockKind::Air);
    let kept = plan::station::kept(frame());
    let (st, _) = plan::station::serving_station(&w, &sources(&w), frame(), &[], Some(1), &kept);
    let st = st.expect("a serving station");
    assert_eq!(st.stand, STAND);
    assert!(
        plan::scoop::aim_at_use(&w, eye(STAND), st.sources[0], plan::scoop::REACH).is_some()
            || st.sources.iter().any(|s| plan::scoop::aim_at_use(
                &w,
                eye(STAND),
                *s,
                plan::scoop::REACH
            )
            .is_some())
    );
}

#[test]
fn a_stand_in_the_rock_serves_once_opened() {
    // The stand and its head stone: the station's way digs them on the way in, so the sight is
    // judged with them open.
    let mut w = world(BlockKind::Air);
    set(&mut w, STAND, BlockKind::Stone);
    set(&mut w, (STAND.0, STAND.1 + 1, STAND.2), BlockKind::Stone);
    let kept = plan::station::kept(frame());
    assert!(plan::station::serves(&w, STAND, (-2, 63, 4), &kept));
}

#[test]
fn the_read_s_refill_digs_the_wall_and_fills() {
    let mut w = world(BlockKind::Stone);
    let mut k = Cast::fresh(site(), 0);
    // The fill from the stand where the bot is (the station's step), not the walk onto the
    // landing first (wall 7).
    k.lava_first = LavaFirst::Filling { since: 0 };
    let mut notes = Vec::new();
    let mut filled_at = None;
    for t in 0..400 {
        let (_, act, status) = k.step(&w, t);
        assert_eq!(status, CastStatus::Running, "tick {t}: {}", k.refill_text());
        notes.extend(k.take_note());
        if let LavaFirst::Filled { at, .. } = k.lava_first {
            filled_at = Some(at);
            break;
        }
        // The server's side, by hand: the slot held, the dug block gone, the bucket filled.
        match act {
            Some(Act::Hold { slot }) => w.held = slot,
            Some(Act::Dig { pos, .. }) => set(&mut w, (pos.x, pos.y, pos.z), BlockKind::Air),
            Some(Act::UseItem) => {
                w.inventory.slots.insert(BUCKET, (ItemKind::LavaBucket, 1));
            }
            _ => {}
        }
    }
    let trail = k.refill_text();
    assert!(filled_at.is_some(), "no fill: {trail}");
    // The wall's cells on the line, x -1, dug from the stand (which ones depends on the source
    // the station takes and the cells the plan places, which are never dug).
    assert!(trail.contains("station sight dig at (-1, 6"), "{trail}");
    assert!(trail.contains("toward"), "{trail}");
    assert!(!trail.contains("station missed"), "{trail}");
    assert!(!trail.contains("tunnel"), "{trail}");
    assert!(k.level_digs >= 1);
    assert!(
        notes
            .iter()
            .any(|(what, _)| what.starts_with("station sight dig at ")),
        "{notes:?}"
    );
    assert!(k.uses_text().contains("station sight digs "));
}

#[test]
fn the_dug_cell_over_lava_is_never_walked_into() {
    // The read's feet-level dig: (23, -54, 43) left open beside the stand, lava straight under it.
    // Here (-1, 64, 4) and its head, over the lava at (-1, 63, 4).
    let mut w = world(BlockKind::Stone);
    let dug = (-1, 64, 4);
    set(&mut w, dug, BlockKind::Air);
    set(&mut w, (-1, 65, 4), BlockKind::Air);
    let mut cells = plan::path::Cells::new(&w);
    // Lava under it: neither a node nor a cell any move's body passes (`clear` reads lava under
    // and beside), so no route crosses it and no drop lands in it. The stand stays a node.
    assert!(!cells.clear(dug));
    assert!(!cells.standable(dug));
    assert!(cells.standable(STAND));
    let budget = plan::path::Budget::Far.expansions();
    for goal in [
        plan::path::Goal::Block { pos: dug },
        // Every cell within 1 of (-2, 64, 4) but the dug one lies over the pool.
        plan::path::Goal::Near {
            pos: (-2, 64, 4),
            range: 1.0,
        },
    ] {
        let r = plan::path::plan(&w, STAND, &goal, budget);
        assert_ne!(r.status, plan::path::Status::Found, "{goal:?}: {r:?}");
        assert!(!r.waypoints.contains(&dug), "{goal:?}: {r:?}");
    }
    // A walk along the wall, past the hole, keeps out of it.
    let r = plan::path::plan(
        &w,
        STAND,
        &plan::path::Goal::Block { pos: (0, 64, -2) },
        budget,
    );
    assert!(!r.waypoints.contains(&dug), "{r:?}");
    // No scoop stands in it.
    for src in [(-2, 63, 4), (-2, 63, 3), (-2, 63, 5)] {
        assert!(!plan::scoop::stands(&w, src).contains(&dug), "{src:?}");
    }
}

fn table() -> Table {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let goals: Goals = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    Table::new(&goals).unwrap()
}

#[test]
fn a_feet_level_sight_dig_is_held_while_a_hostile_is_near() {
    // A zombie three blocks off the stand, within defend's own range (`defend_radius` of
    // config/goals.toml): the hole beside the stand with lava under it is not opened, and the
    // station counts a miss.
    let rules = table().survival;
    assert!(rules.defend_radius >= 3.0, "{}", rules.defend_radius);
    let mut w = world(BlockKind::Stone);
    w.step(&Event::Packet(S2c::AddEntity {
        id: 41,
        uuid: [0; 16],
        kind: EntityKind::Zombie.id(),
        x: 3.5,
        y: 64.0,
        z: 4.5,
        velocity: [0.0; 3],
    }));
    let mut k = Cast::fresh(site(), 0).with_sight_hold(SightHold::of(&rules));
    // The fill from the stand, as above (wall 7's walk onto the landing skipped).
    k.lava_first = LavaFirst::Filling { since: 0 };
    let mut notes = Vec::new();
    for t in 0..60 {
        let (_, act, status) = k.step(&w, t);
        notes.extend(k.take_note());
        match act {
            Some(Act::Hold { slot }) => w.held = slot,
            Some(Act::Dig { pos, .. }) => {
                // A head-level wall cell may go; one level with the feet never does.
                assert!(
                    !(pos.x == -1 && pos.y == STAND.1),
                    "tick {t}: dug {pos:?}: {}",
                    k.refill_text()
                );
                set(&mut w, (pos.x, pos.y, pos.z), BlockKind::Air);
            }
            _ => {}
        }
        if status != CastStatus::Running {
            break;
        }
    }
    let trail = k.refill_text();
    assert!(
        trail.contains("station missed: station sight dig held: hostile Zombie at 3.0"),
        "{trail}"
    );
    assert!(
        notes
            .iter()
            .any(|(what, _)| what.starts_with("station sight dig held: hostile Zombie")),
        "{notes:?}"
    );
}
