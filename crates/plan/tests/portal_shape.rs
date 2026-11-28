//! Issue 83: `plan::portal_shape`, vanilla's `PortalShape` (26.1.2, brain/game/mc-portal-cast.md),
//! on hand-built frames: ten obsidian with no corners are a frame, nine are not, crying obsidian
//! is not frame, the size limits, the axis order. Then the done condition: on a hand-built pool,
//! `build::frame_plan`'s ops worked to their done states build a frame `PortalShape` accepts, for
//! every `Facing`.

use std::collections::BTreeMap;
use std::sync::Arc;

use data::BlockKind;
use plan::build::{FRAME_CELLS, Facing, Frame, OpKind, frame_plan, next_op, op_text};
use plan::path::Cell;
use plan::portal_shape::{Axis, Shape, any_shape, find_empty};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use world::{BlockPos, Event, World};

type Blocks = BTreeMap<Cell, BlockKind>;

/// Air everywhere but the blocks set.
fn look(b: &Blocks) -> impl Fn(Cell) -> Option<BlockKind> + '_ {
    |c| Some(b.get(&c).copied().unwrap_or(BlockKind::Air))
}

/// The smallest frame on the X axis: inside x 1..=2, y 1..=3 at z 0; `corners` adds the four.
fn small_frame(corners: bool) -> Blocks {
    let mut b = Blocks::new();
    for (x, y) in FRAME_CELLS {
        b.insert((x, y, 0), BlockKind::Obsidian);
    }
    if corners {
        for c in [(0, 0, 0), (3, 0, 0), (0, 4, 0), (3, 4, 0)] {
            b.insert(c, BlockKind::Obsidian);
        }
    }
    b
}

/// A frame round an inside `w` wide and `h` high from (0, 0, 0), along X or Z, no corners.
fn frame_of(axis: Axis, w: i32, h: i32) -> Blocks {
    let cell = |i: i32, y: i32| match axis {
        Axis::X => (i, y, 0),
        Axis::Z => (0, y, i),
    };
    let mut b = Blocks::new();
    for i in 0..w {
        b.insert(cell(i, -1), BlockKind::Obsidian);
        b.insert(cell(i, h), BlockKind::Obsidian);
    }
    for y in 0..h {
        b.insert(cell(-1, y), BlockKind::Obsidian);
        b.insert(cell(w, y), BlockKind::Obsidian);
    }
    b
}

fn sorted(mut v: Vec<Cell>) -> Vec<Cell> {
    v.sort_unstable();
    v
}

const SMALL_INSIDE: [Cell; 6] = [
    (1, 1, 0),
    (1, 2, 0),
    (1, 3, 0),
    (2, 1, 0),
    (2, 2, 0),
    (2, 3, 0),
];

#[test]
fn ten_obsidian_with_no_corners_are_a_frame() {
    let b = small_frame(false);
    let s = find_empty(&look(&b), (1, 1, 0), Axis::X, -64).unwrap();
    assert_eq!(
        (s.axis, s.width, s.height, s.portal_blocks),
        (Axis::X, 2, 3, 0)
    );
    // Right is west on X: the bottom-left is the east cell.
    assert_eq!(s.bottom_left, (2, 1, 0));
    assert_eq!(sorted(s.inside()), SMALL_INSIDE.to_vec());
    let mut frame: Vec<Cell> = FRAME_CELLS.iter().map(|(x, y)| (*x, *y, 0)).collect();
    frame.sort_unstable();
    assert_eq!(sorted(s.frame()), frame);
}

#[test]
fn corners_change_nothing() {
    let b = small_frame(true);
    let s = find_empty(&look(&b), (1, 1, 0), Axis::X, -64).unwrap();
    assert_eq!((s.width, s.height), (2, 3));
}

#[test]
fn nine_obsidian_are_not_a_frame() {
    for gone in FRAME_CELLS {
        let mut b = small_frame(false);
        b.remove(&(gone.0, gone.1, 0));
        for from in SMALL_INSIDE {
            assert_eq!(
                find_empty(&look(&b), from, Axis::X, -64),
                None,
                "{gone:?} missing, asked at {from:?}"
            );
        }
    }
}

#[test]
fn crying_obsidian_is_not_frame() {
    for swapped in FRAME_CELLS {
        let mut b = small_frame(false);
        b.insert((swapped.0, swapped.1, 0), BlockKind::CryingObsidian);
        assert_eq!(find_empty(&look(&b), (1, 1, 0), Axis::X, -64), None);
    }
}

#[test]
fn any_inside_cell_finds_the_same_frame() {
    let b = small_frame(false);
    let want = find_empty(&look(&b), (1, 1, 0), Axis::X, -64);
    for from in SMALL_INSIDE {
        assert_eq!(find_empty(&look(&b), from, Axis::X, -64), want, "{from:?}");
    }
}

#[test]
fn a_block_inside_or_outside_the_box_is_not_a_frame() {
    // A stone in the inside, and a frame of stone.
    let mut b = small_frame(false);
    b.insert((2, 2, 0), BlockKind::Stone);
    assert_eq!(find_empty(&look(&b), (1, 1, 0), Axis::X, -64), None);
    let stone: Blocks = small_frame(false)
        .into_keys()
        .map(|c| (c, BlockKind::Stone))
        .collect();
    assert_eq!(find_empty(&look(&stone), (1, 1, 0), Axis::X, -64), None);
}

#[test]
fn fire_inside_is_empty_and_portal_blocks_are_counted() {
    let mut b = small_frame(false);
    b.insert((1, 1, 0), BlockKind::Fire);
    b.insert((2, 1, 0), BlockKind::SoulFire);
    assert!(find_empty(&look(&b), (1, 2, 0), Axis::X, -64).is_some());
    let mut lit = small_frame(false);
    for c in SMALL_INSIDE {
        lit.insert(c, BlockKind::NetherPortal);
    }
    let s = any_shape(&look(&lit), (1, 1, 0), Axis::X, -64).unwrap();
    assert_eq!(s.portal_blocks, 6);
    // A lit frame is not empty: fire there lights nothing more.
    assert_eq!(find_empty(&look(&lit), (1, 1, 0), Axis::X, -64), None);
}

#[test]
fn the_size_limits() {
    for (w, h, ok) in [
        (1, 3, false),
        (2, 2, false),
        (2, 3, true),
        (3, 4, true),
        (21, 21, true),
        (22, 3, false),
        (2, 22, false),
    ] {
        let b = frame_of(Axis::X, w, h);
        let got = find_empty(&look(&b), (0, 0, 0), Axis::X, -64);
        assert_eq!(got.is_some(), ok, "{w} by {h}");
        if let Some(s) = got {
            assert_eq!((s.width, s.height), (w, h));
        }
    }
}

#[test]
fn x_is_asked_first_then_z() {
    let b = frame_of(Axis::Z, 2, 3);
    let s = find_empty(&look(&b), (0, 0, 0), Axis::X, -64).unwrap();
    assert_eq!(s.axis, Axis::Z);
    // Right is south on Z: the bottom-left is the north cell.
    assert_eq!(s.bottom_left, (0, 0, 0));
    assert_eq!(any_shape(&look(&b), (0, 0, 0), Axis::X, -64), None);
}

// ---------------------------------------------------------------- the done condition

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

/// Stone to y 79, air over it; chunks -3..=3 loaded.
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
/// (`LiquidBlock.shouldSpreadLiquid`: the lava's up and four sides are read, never under it).
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

/// Flint and steel on the top face of `c`: fire over it, and the frame round the fire lit
/// (`BaseFireBlock.onPlace`: `findEmptyPortalShape` with X preferred).
fn strike(w: &mut World, c: Cell) {
    let fire = (c.0, c.1 + 1, c.2);
    assert_eq!(
        kind(w, fire),
        Some(BlockKind::Air),
        "no air to light at {fire:?}"
    );
    set(w, fire, BlockKind::Fire);
    let shape = find_empty(&|p| kind(w, p), fire, Axis::X, 0);
    if let Some(s) = shape {
        for p in s.inside() {
            set(w, p, BlockKind::NetherPortal);
        }
    }
}

/// The portal plan worked op by op to its done states, each op's effect as the server makes it.
fn work(w: &mut World, f: Frame, h: i32) {
    let plan = frame_plan(f, false, h);
    let mut last = None;
    for _ in 0..plan.len() * 2 {
        let Some(op) = next_op(&*w, &plan).cloned() else {
            return;
        };
        assert_ne!(last.as_ref(), Some(&op), "stuck at {}", op_text(&op));
        match op.kind {
            OpKind::Dig | OpKind::Scoop => set(w, op.pos.unwrap(), BlockKind::Air),
            OpKind::Place => set(w, op.pos.unwrap(), BlockKind::Cobblestone),
            OpKind::Pour if op.item == Some(data::ItemKind::LavaBucket) => {
                set(w, op.into.unwrap(), BlockKind::Lava);
            }
            OpKind::Pour => water_on(w, op.into.unwrap()),
            OpKind::UseOn => strike(w, op.pos.unwrap()),
            OpKind::StandIn | OpKind::Verify => panic!("not in the portal plan: {}", op_text(&op)),
        }
        last = Some(op);
    }
    panic!("the plan did not finish");
}

#[test]
fn the_cast_plan_builds_a_frame_portal_shape_accepts() {
    for facing in Facing::ALL {
        for h in [0, 3, 6] {
            let f = Frame {
                origin: (0, 64, 0),
                facing,
            };
            let mut w = rock();
            // The pool the lava comes from: three by two sources sunk in the floor ahead.
            for dx in 1..=3 {
                for dz in 8..=9 {
                    set(&mut w, f.at((dx, -1, dz)), BlockKind::Lava);
                }
            }
            work(&mut w, f, h);
            let at = |p: Cell| kind(&w, p);
            for c in FRAME_CELLS {
                assert_eq!(
                    at(f.at((c.0, c.1, 0))),
                    Some(BlockKind::Obsidian),
                    "{facing:?} {c:?}"
                );
            }
            let axis = match facing {
                Facing::South | Facing::North => Axis::X,
                Facing::East | Facing::West => Axis::Z,
            };
            let lit = f.at((1, 1, 0));
            let s: Shape = any_shape(&at, lit, axis, 0)
                .unwrap_or_else(|| panic!("{facing:?} h {h}: PortalShape refuses the frame"));
            assert_eq!(
                (s.width, s.height, s.portal_blocks),
                (2, 3, 6),
                "{facing:?} h {h}"
            );
            let inside: Vec<Cell> = [1, 2]
                .iter()
                .flat_map(|dx| [1, 2, 3].map(|dy| f.at((*dx, dy, 0))))
                .collect();
            assert_eq!(sorted(s.inside()), sorted(inside), "{facing:?} h {h}");
            // The other axis sees no frame: the plane is the one the plan meant.
            assert_eq!(
                any_shape(&at, lit, axis.other(), 0),
                None,
                "{facing:?} h {h}"
            );
        }
    }
}
