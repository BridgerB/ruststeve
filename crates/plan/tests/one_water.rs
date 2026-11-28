//! Issue 83 wall 6 (docs/problems/lava-portal.md): a cast holding one water bucket casts every
//! cell by pouring it, scooping it back and pouring it again, when the server's slot update
//! comes before its block updates. Race 46152526931 lane 1 (`why` at ticks 29,326 to 29,346):
//! the water pour was used at 29,338, the bucket showed empty at 29,344, the obsidian and the
//! water source over it at 29,346; in between the refill found no water and the cast ended "out
//! of water after 69 ops" with its water in the room.
//!
//! On a hand-built rock (stone to y 79), a frame at (0, 64, 0) facing south: every op but the
//! water's is worked to its done state as the server makes it (as `portal_shape.rs` does); the
//! water pours and scoops are the executor's, its body at the op's stand, and each use answered
//! the way the read saw it: the slot at once, the cell `LAG` ticks after.

use std::sync::Arc;

use data::{BlockKind, ItemKind};
use plan::build::{Facing, Frame, Op, OpKind, frame_plan, next_op, op_text};
use plan::cast::{ANSWER_TICKS, Cast, CastStatus, Site};
use plan::path::Cell;
use plan::portal_shape::{Axis, any_shape, find_empty};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use world::{Act, BlockPos, Event, World};

/// Ticks the cell's answer comes after the slot's: two in the read (29,344 against 29,346).
const LAG: u64 = 2;

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

/// Stone to y 79, air over it; chunks -3..=3 loaded; the bot holding one water bucket (hotbar
/// 0, held) and one empty bucket (hotbar 1), and nothing to fill another from.
fn rock() -> World {
    let mut sections = vec![section(BlockKind::Stone); 5];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 11));
    let col = Arc::new(Column { min_y: 0, sections });
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 0,
        dimension: plan::home::OVERWORLD.to_owned(),
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
    w.player.health = Some(20.0);
    w.player.food = Some(20);
    w.inventory.slots.insert(36, (ItemKind::WaterBucket, 1));
    w.inventory.slots.insert(37, (ItemKind::Bucket, 1));
    w.held = 0;
    w
}

fn set(w: &mut World, c: Cell, k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: c,
        state: state(k),
    }));
}

fn kind(w: &World, (x, y, z): Cell) -> Option<BlockKind> {
    w.block_at(BlockPos::new(x, y, z))
}

/// Water placed at `c`: each lava source it touches from above or beside turns to obsidian
/// (`LiquidBlock.shouldSpreadLiquid`, brain/game/mc-portal-cast.md).
fn water_on(w: &mut World, c: Cell) {
    set(w, c, BlockKind::Water);
    let (x, y, z) = c;
    for n in [
        (x, y - 1, z),
        (x - 1, y, z),
        (x + 1, y, z),
        (x, y, z - 1),
        (x, y, z + 1),
    ] {
        if kind(w, n) == Some(BlockKind::Lava) {
            set(w, n, BlockKind::Obsidian);
        }
    }
}

/// Flint and steel on the top face of `c`: the frame round the fire lit.
fn strike(w: &mut World, c: Cell) {
    let fire = (c.0, c.1 + 1, c.2);
    set(w, fire, BlockKind::Fire);
    if let Some(s) = find_empty(&|p| kind(w, p), fire, Axis::X, 0) {
        for p in s.inside() {
            set(w, p, BlockKind::NetherPortal);
        }
    }
}

/// The water's ops are the executor's; every other op is the server's at once.
fn waters(op: &Op) -> bool {
    match op.kind {
        OpKind::Pour => op.item == Some(ItemKind::WaterBucket),
        OpKind::Scoop => true,
        OpKind::Dig | OpKind::Place | OpKind::UseOn | OpKind::StandIn | OpKind::Verify => false,
    }
}

fn model(w: &mut World, op: &Op) {
    match op.kind {
        OpKind::Dig => set(w, op.pos.unwrap(), BlockKind::Air),
        OpKind::Place => set(w, op.pos.unwrap(), BlockKind::Cobblestone),
        OpKind::Pour => set(w, op.into.unwrap(), BlockKind::Lava),
        OpKind::UseOn => strike(w, op.pos.unwrap()),
        OpKind::Scoop | OpKind::StandIn | OpKind::Verify => {
            panic!("not the model's: {}", op_text(op))
        }
    }
}

fn stand_at(w: &mut World, (x, y, z): Cell) {
    let pos = [f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5];
    w.body = Some(physics::Body {
        pos,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(pos);
}

fn held(w: &World) -> Option<ItemKind> {
    w.inventory
        .slots
        .get(&(36 + i16::from(w.held)))
        .map(|(k, _)| *k)
}

fn frame() -> Frame {
    Frame {
        origin: (0, 64, 0),
        facing: Facing::South,
    }
}

fn site() -> Site {
    Site {
        frame: frame(),
        // Never refilled from: every lava pour is the model's.
        lava: (2, 63, 9),
        h: 0,
    }
}

/// A block answer due at a tick: the water poured into a cell, or a scooped cell emptied.
enum Due {
    Water(Cell),
    Empty(Cell),
}

#[test]
fn one_water_bucket_pours_scoops_back_and_lights() {
    let mut w = rock();
    let plan = frame_plan(frame(), false, 0);
    let mut cast = Cast::new(site(), 0);
    let mut due: Vec<(u64, Due)> = Vec::new();
    let (mut pours, mut scoops) = (0, 0);
    let mut t = 0u64;
    for _ in 0..20_000 {
        let (now, later): (Vec<_>, Vec<_>) = std::mem::take(&mut due)
            .into_iter()
            .partition(|(at, _)| *at <= t);
        due = later;
        for (_, d) in now {
            match d {
                Due::Water(c) => water_on(&mut w, c),
                Due::Empty(c) => set(&mut w, c, BlockKind::Air),
            }
        }
        let Some(op) = next_op(&w, &plan).cloned() else {
            break;
        };
        t += 1;
        if !waters(&op) {
            model(&mut w, &op);
            continue;
        }
        stand_at(&mut w, op.stand);
        let (_, act, status) = cast.step(&w, t);
        assert_eq!(
            status,
            CastStatus::Running,
            "tick {t} at {}: {}; {}",
            op_text(&op),
            cast.refill_text(),
            cast.refused_text()
        );
        match act {
            None => {}
            Some(Act::Hold { slot }) => w.held = slot,
            Some(Act::UseItem) => {
                let slot = 36 + i16::from(w.held);
                match (op.kind, held(&w)) {
                    (OpKind::Pour, Some(ItemKind::WaterBucket)) => {
                        pours += 1;
                        w.inventory.slots.insert(slot, (ItemKind::Bucket, 1));
                        due.push((t + LAG, Due::Water(op.into.unwrap())));
                    }
                    (OpKind::Scoop, Some(ItemKind::Bucket)) => {
                        scoops += 1;
                        w.inventory.slots.insert(slot, (ItemKind::WaterBucket, 1));
                        due.push((t + LAG, Due::Empty(op.pos.unwrap())));
                    }
                    (k, h) => panic!("tick {t}: a use of {h:?} at {k:?} ({})", op_text(&op)),
                }
            }
            Some(a) => panic!("tick {t}: {a:?} at {}", op_text(&op)),
        }
    }
    assert!(next_op(&w, &plan).is_none(), "the plan did not finish");
    // Ten cells, each one pour of the one bucket and its scoop back.
    assert_eq!((pours, scoops), (10, 10));
    assert_eq!(cast.answer_waits, 10, "every pour's answer waited on");
    assert_eq!(cast.refill_text(), "no refill");
    assert!(
        cast.refused_text().ends_with("pour answers awaited 10"),
        "{}",
        cast.refused_text()
    );
    let (what, _) = cast.take_note().expect("the wait's note");
    assert!(
        what.starts_with("pour answer awaited: water pour"),
        "{what}"
    );
    assert_eq!(
        w.inventory.slots.get(&36),
        Some(&(ItemKind::WaterBucket, 1)),
        "the water kept"
    );
    let at = |p: Cell| kind(&w, p);
    let s = any_shape(&at, frame().at((1, 1, 0)), Axis::X, 0).expect("PortalShape takes it");
    assert_eq!((s.width, s.height, s.portal_blocks), (2, 3, 6));
}

#[test]
fn a_pour_never_answered_takes_its_own_water_first() {
    // The pour's water lands in the room and no obsidian forms (wall 28): past `ANSWER_TICKS` the
    // refill looks for the cast's own water before any out-of-water ending.
    let mut w = rock();
    let plan = frame_plan(frame(), false, 0);
    let mut cast = Cast::new(site(), 0);
    let mut t = 0u64;
    let op = loop {
        let op = next_op(&w, &plan).cloned().expect("a water pour");
        if waters(&op) {
            break op;
        }
        model(&mut w, &op);
        t += 1;
    };
    assert_eq!(op.kind, OpKind::Pour);
    stand_at(&mut w, op.stand);
    let sent = loop {
        t += 1;
        let (_, act, status) = cast.step(&w, t);
        assert_eq!(status, CastStatus::Running);
        if act == Some(Act::UseItem) {
            break t;
        }
        assert!(t < 100, "no use");
    };
    w.inventory.slots.insert(36, (ItemKind::Bucket, 1));
    // The water stands in the cell over the hole and the hole's lava made no obsidian.
    set(&mut w, op.into.unwrap(), BlockKind::Water);
    for k in 1..ANSWER_TICKS {
        let (_, act, status) = cast.step(&w, sent + k);
        assert_eq!(status, CastStatus::Running, "waiting, {k} ticks after");
        assert_eq!(act, None);
    }
    assert_eq!(cast.refill_text(), "no refill", "no refill while waiting");
    let _ = cast.step(&w, sent + ANSWER_TICKS);
    let trail = cast.refill_text();
    assert!(trail.contains("not answered in 60 ticks"), "{trail}");
    assert!(trail.contains("own water 1"), "{trail}");
}
