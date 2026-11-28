//! Vanilla's `PortalShape` (26.1.2, `net.minecraft.world.level.portal.PortalShape`, read with
//! `javap` in probe 46139819290; brain/game/mc-portal-cast.md): whether an empty Nether portal
//! frame stands round a cell, and its inside. Pure: the world is a function from a cell to its
//! block (`None` for a cell not loaded, which is neither empty nor frame).
//!
//! The rule as the server checks it: the inside is 2 to 21 wide and 3 to 21 high, every inside
//! cell empty (air, fire or a portal block), a frame block (obsidian only) under each bottom
//! cell, over each top cell, and beside each end of every row. The four corners are never read:
//! ten obsidian make the smallest frame.

use data::BlockKind;

use crate::Cell;

/// `PortalShape.MIN_WIDTH` / `MAX_WIDTH`: the inside's width in blocks.
pub const MIN_WIDTH: i32 = 2;
pub const MAX_WIDTH: i32 = 21;
/// `PortalShape.MIN_HEIGHT` / `MAX_HEIGHT`: the inside's height in blocks.
pub const MIN_HEIGHT: i32 = 3;
pub const MAX_HEIGHT: i32 = 21;

/// The horizontal axis a portal's plane runs along (`Direction.Axis`, the block's `axis`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Axis {
    X,
    Z,
}

impl Axis {
    /// The other horizontal axis.
    pub fn other(self) -> Axis {
        match self {
            Axis::X => Axis::Z,
            Axis::Z => Axis::X,
        }
    }

    /// `rightDir`: west along X, south along Z, as `(dx, dz)`.
    fn right(self) -> (i32, i32) {
        match self {
            Axis::X => (-1, 0),
            Axis::Z => (0, 1),
        }
    }

    /// The block state's `axis` value.
    pub fn name(self) -> &'static str {
        match self {
            Axis::X => "x",
            Axis::Z => "z",
        }
    }
}

/// A valid frame's inside: its bottom-left cell (left as seen with `right` to the right), its
/// size, and how many of its cells are portal blocks already.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    pub axis: Axis,
    pub bottom_left: Cell,
    pub width: i32,
    pub height: i32,
    pub portal_blocks: i32,
}

impl Shape {
    /// The inside cells, bottom row first, left to right (`createPortalBlocks`' box).
    pub fn inside(&self) -> Vec<Cell> {
        let (rx, rz) = self.axis.right();
        let (x, y, z) = self.bottom_left;
        (0..self.height)
            .flat_map(|up| (0..self.width).map(move |i| (x + rx * i, y + up, z + rz * i)))
            .collect()
    }

    /// The frame cells the server reads: under and over each column, beside each row. No
    /// corners.
    pub fn frame(&self) -> Vec<Cell> {
        let (rx, rz) = self.axis.right();
        let (x, y, z) = self.bottom_left;
        let at = |i: i32, up: i32| (x + rx * i, y + up, z + rz * i);
        let mut out: Vec<Cell> = (0..self.width).map(|i| at(i, -1)).collect();
        out.extend((0..self.width).map(|i| at(i, self.height)));
        for up in 0..self.height {
            out.push(at(-1, up));
            out.push(at(self.width, up));
        }
        out
    }
}

/// `PortalShape.isEmpty`: air (`isAir`), the `fire` tag (fire, soul fire) or a portal block.
pub fn empty(k: Option<BlockKind>) -> bool {
    matches!(
        k,
        Some(
            BlockKind::Air
                | BlockKind::CaveAir
                | BlockKind::VoidAir
                | BlockKind::Fire
                | BlockKind::SoulFire
                | BlockKind::NetherPortal
        )
    )
}

/// `PortalShape.FRAME`: obsidian, and nothing else (crying obsidian is not frame).
pub fn frame(k: Option<BlockKind>) -> bool {
    k == Some(BlockKind::Obsidian)
}

fn step((x, y, z): Cell, (dx, dz): (i32, i32), n: i32) -> Cell {
    (x + dx * n, y, z + dz * n)
}

/// `getDistanceUntilEdgeAboveFrame`: the empty run from `pos` along `dir` over frame, ended by a
/// frame block (its distance) or by anything else (0).
fn until_edge(at: &impl Fn(Cell) -> Option<BlockKind>, pos: Cell, dir: (i32, i32)) -> i32 {
    for i in 0..=MAX_WIDTH {
        let p = step(pos, dir, i);
        let k = at(p);
        if !empty(k) {
            return if frame(k) { i } else { 0 };
        }
        if !frame(at((p.0, p.1 - 1, p.2))) {
            return 0;
        }
    }
    0
}

/// `PortalShape.findAnyShape` with `isValid`: the frame round `pos` on `axis`, if valid. `min_y`
/// is the world's lowest y (`level.getMinY()`).
pub fn any_shape(
    at: &impl Fn(Cell) -> Option<BlockKind>,
    pos: Cell,
    axis: Axis,
    min_y: i32,
) -> Option<Shape> {
    let right = axis.right();
    let left = (-right.0, -right.1);
    // calculateBottomLeft: down through empty cells, at most 21.
    let floor = min_y.max(pos.1 - MAX_HEIGHT);
    let mut p = pos;
    while p.1 > floor && empty(at((p.0, p.1 - 1, p.2))) {
        p.1 -= 1;
    }
    let edge = until_edge(at, p, left) - 1;
    if edge < 0 {
        return None;
    }
    let bottom_left = step(p, left, edge);
    // calculateWidth
    let width = until_edge(at, bottom_left, right);
    if !(MIN_WIDTH..=MAX_WIDTH).contains(&width) {
        return None;
    }
    // calculateHeight: getDistanceUntilTop counts the portal blocks on the way.
    let mut portal_blocks = 0;
    let mut height = MAX_HEIGHT;
    'rows: for h in 0..MAX_HEIGHT {
        let row = (bottom_left.0, bottom_left.1 + h, bottom_left.2);
        if !frame(at(step(row, right, -1))) || !frame(at(step(row, right, width))) {
            height = h;
            break;
        }
        for i in 0..width {
            let k = at(step(row, right, i));
            if !empty(k) {
                height = h;
                break 'rows;
            }
            if k == Some(BlockKind::NetherPortal) {
                portal_blocks += 1;
            }
        }
    }
    if !(MIN_HEIGHT..=MAX_HEIGHT).contains(&height) {
        return None;
    }
    // hasTopFrame
    let top = (bottom_left.0, bottom_left.1 + height, bottom_left.2);
    if !(0..width).all(|i| frame(at(step(top, right, i)))) {
        return None;
    }
    Some(Shape {
        axis,
        bottom_left,
        width,
        height,
        portal_blocks,
    })
}

/// `PortalShape.findEmptyPortalShape`: a valid frame with no portal block in it, on `preferred`
/// first and then the other axis. Fire placed at `pos` lights this shape (`BaseFireBlock.onPlace`
/// asks with X preferred).
pub fn find_empty(
    at: &impl Fn(Cell) -> Option<BlockKind>,
    pos: Cell,
    preferred: Axis,
    min_y: i32,
) -> Option<Shape> {
    [preferred, preferred.other()]
        .into_iter()
        .find_map(|a| any_shape(at, pos, a, min_y).filter(|s| s.portal_blocks == 0))
}
