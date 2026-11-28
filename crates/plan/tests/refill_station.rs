//! Issue 83 wall 4 (docs/problems/lava-portal.md): race 46144128199 lane 0's refill at (24, -54,
//! 43). The station's stand had twelve sources in reach, every one behind a wall of deepslate
//! level with the feet and the head, with lava under the wall's foot: from the stand no face was
//! in sight and no lid lay under the feet, so the station step ended in the tick it began, the
//! level tunnel turned off the lava and stopped on a walk onto air with nothing dug, and the
//! ladder went round four times in 46 ticks to "no route".
//!
//! The read's shape, hand-built: stone to y 63, air from y 64; the frame at (0, 63, 0) facing
//! south, so the h 1 stand in front of it is (0, 64, 4); a wall of stone at x -1, y 64 and 65,
//! along the whole span the station searches; lava under the wall's foot at (-1, 63, 4); the pool
//! at x -14 to -2, y 63, open to the air above it.

use std::sync::Arc;

use data::{BlockKind, ItemKind};
use plan::build::{Facing, Frame};
use plan::cast::{Cast, Site};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use world::{Event, World};

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

fn frame() -> Frame {
    Frame {
        origin: (0, 63, 0),
        facing: Facing::South,
    }
}

/// The pool, wide enough that no stand on its far side is within the station's span: in the
/// read the pool's far bank was lava floor too.
fn pool() -> Vec<Cell> {
    (-4..=16)
        .flat_map(|z| (-14..=-2).map(move |x| (x, 63, z)))
        .collect()
}

/// The world of the read, with the wall when `wall`; the feet on the stand, the kit held with
/// an empty bucket for the lava.
fn world(wall: bool) -> World {
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
    for c in pool() {
        set(&mut w, c, BlockKind::Lava);
    }
    set(&mut w, (-1, 63, 4), BlockKind::Lava);
    if wall {
        for z in -12..=20 {
            set(&mut w, (-1, 64, z), BlockKind::Stone);
            set(&mut w, (-1, 65, z), BlockKind::Stone);
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
    w.inventory.slots.insert(38, (ItemKind::Bucket, 1));
    w.inventory.slots.insert(39, (ItemKind::FlintAndSteel, 1));
    w.inventory.slots.insert(40, (ItemKind::Cobblestone, 32));
    w.held = 0;
    w
}

fn sources(w: &World) -> Vec<Cell> {
    plan::cast::lava_sources(w)
}

#[test]
fn the_read_s_stand_is_a_station_by_reach_alone() {
    let w = world(true);
    let srcs = sources(&w);
    assert!(pool().iter().all(|c| srcs.contains(c)), "{srcs:?}");
    // The site chooser's station: a stand with sources in reach, sight not asked.
    let st =
        plan::station::station_among(&w, &srcs, frame(), &[], Some(1)).expect("a station by reach");
    assert!(!st.sources.is_empty());
    assert!(st.stand.0 >= 0, "on the room's side of the wall: {st:?}");
    // From that stand no face is in sight and no lid lies under the feet: the step there ends in
    // the tick it begins.
    let kept = plan::station::kept(frame());
    assert!(
        st.sources
            .iter()
            .all(|s| !plan::station::serves(&w, st.stand, *s, &kept)),
        "{st:?}"
    );
}

#[test]
fn behind_the_wall_no_stand_serves() {
    let w = world(true);
    let kept = plan::station::kept(frame());
    let (st, blind) =
        plan::station::serving_station(&w, &sources(&w), frame(), &[], Some(1), &kept);
    assert_eq!(st, None);
    assert!(blind > 0, "the stands passed over are counted: {blind}");
}

#[test]
fn with_the_wall_gone_the_stand_serves() {
    let w = world(false);
    let kept = plan::station::kept(frame());
    let (st, _) = plan::station::serving_station(&w, &sources(&w), frame(), &[], Some(1), &kept);
    let st = st.expect("a serving station");
    assert!(!st.sources.is_empty());
    assert!(
        st.sources
            .iter()
            .all(|s| plan::station::serves(&w, st.stand, *s, &kept))
    );
}

#[test]
fn a_stand_in_the_rock_serves_once_opened() {
    // The stand and its head stone: the station's way digs them on the way in, so the sight is
    // judged with them open.
    let mut w = world(false);
    set(&mut w, STAND, BlockKind::Stone);
    set(&mut w, (STAND.0, STAND.1 + 1, STAND.2), BlockKind::Stone);
    let kept = plan::station::kept(frame());
    assert!(plan::station::serves(&w, STAND, (-2, 63, 4), &kept));
}

#[test]
fn the_ladder_says_no_station_and_never_walks_to_the_blind_stand() {
    let w = world(true);
    let site = Site {
        frame: frame(),
        lava: (-2, 63, 4),
        h: 1,
    };
    let mut k = Cast::fresh(site, 0);
    let _ = k.step(&w, 0);
    let trail = k.refill_text();
    assert!(
        trail.contains("no station (") && trail.contains("none in sight)"),
        "{trail}"
    );
    assert!(!trail.contains("station at"), "{trail}");
}
