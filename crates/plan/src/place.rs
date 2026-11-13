//! The place task (brain/ruststeve/design/placing.md): put a held block down on the floor beside
//! the feet (or a block up or down), the item held first (`hold`), then `use_item_on` the top of the floor block, then the
//! block update that shows it there. A cell the body is in, or one with nothing solid under it,
//! is never chosen; a placement the server refused (the update shows something else) is tried in
//! another cell, and the third refusal fails.

use data::{BlockKind, ItemKind};
use world::{Act, BlockPos, World};

use crate::dig::{Dig, DigStatus};
use crate::hold::{self, Hold};
use crate::path::Cell;

/// Ticks to wait for the block update before a placement counts as refused.
pub const CONFIRM_TICKS: u64 = 40;
/// Tries before the task fails.
pub const MAX_TRIES: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlaceFailure {
    /// The item is not held and not in the inventory.
    Missing,
    /// No floor cell beside the feet takes it.
    NoRoom,
    /// Three placements were refused.
    Refused,
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceStatus {
    Running,
    Done(Cell),
    Failed(PlaceFailure),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Place {
    pub block: BlockKind,
    pub item: ItemKind,
    /// The cell tried, and when the click went.
    pub at: Option<(Cell, u64)>,
    pub tried: Vec<Cell>,
    pub started: u64,
    /// A wall cell being dug to make room (issue 13b: a furnace in a one-wide staircase whose
    /// only free cell the table took), and how many have been.
    #[serde(default)]
    pub clearing: Option<Dig>,
    #[serde(default)]
    pub cleared: u32,
}

/// Wall cells dug to make room before the task fails `NoRoom`.
pub const MAX_CLEARED: u32 = 2;

fn kind(w: &World, (x, y, z): Cell) -> Option<BlockKind> {
    w.block_at(BlockPos::new(x, y, z))
}

fn empty(k: Option<BlockKind>) -> bool {
    matches!(
        k,
        Some(BlockKind::Air | BlockKind::CaveAir | BlockKind::ShortGrass | BlockKind::Fern)
    )
}

/// Blocks a right-click opens instead of placing against (the server opens their menu): never a
/// floor to place on (issue 13b: a furnace clicked onto the table opened it).
const OPENS: &[BlockKind] = &[
    BlockKind::CraftingTable,
    BlockKind::Furnace,
    BlockKind::BlastFurnace,
    BlockKind::Smoker,
    BlockKind::Chest,
    BlockKind::TrappedChest,
    BlockKind::Barrel,
];

fn full(k: Option<BlockKind>) -> bool {
    k.and_then(data::generated::shapes::collision) == Some(data::types::Collision::Full)
}

impl Place {
    pub fn new(block: BlockKind, tick: u64) -> Option<Self> {
        let item = block.info().and_then(|i| ItemKind::from_name(i.name))?;
        Some(Self {
            block,
            item,
            at: None,
            tried: Vec::new(),
            started: tick,
            clearing: None,
            cleared: 0,
        })
    }

    /// A floor cell beside the feet (straight neighbours first, then diagonals) that is empty,
    /// has a full block under it and is not one the body's box is in.
    fn choose(&self, w: &World) -> Option<Cell> {
        let body = w.body?;
        let (fx, fy, fz) = crate::path::feet_cell(body.pos);
        let b = physics::aabb(body.pos);
        let in_body = |(x, y, z): Cell| {
            let c = [
                f64::from(x),
                f64::from(y),
                f64::from(z),
                f64::from(x) + 1.0,
                f64::from(y) + 1.0,
                f64::from(z) + 1.0,
            ];
            (0..3).all(|a| physics::overlaps(&b, &c, a))
        };
        [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ]
        .into_iter()
        // At the feet's height, then a block up (the stair just come down, in a staircase
        // whose walls are solid at the feet: found by issue 11b's sim measure), then a block down.
        .flat_map(|(dx, dz)| [0, 1, -1].map(|dy| (fx + dx, fy + dy, fz + dz)))
        .find(|c| {
            !self.tried.contains(c)
                && empty(kind(w, *c))
                && full(kind(w, (c.0, c.1 - 1, c.2)))
                && !kind(w, (c.0, c.1 - 1, c.2)).is_some_and(|k| OPENS.contains(&k))
                && !in_body(*c)
        })
    }

    /// A wall cell beside the feet to dig for room: a full block at the feet's height with a full
    /// block under it, nothing above it at head height but air or another wall, no fluid near.
    fn wall(&self, w: &World) -> Option<Cell> {
        let body = w.body?;
        let (fx, fy, fz) = crate::path::feet_cell(body.pos);
        [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .map(|(dx, dz)| (fx + dx, fy, fz + dz))
            .find(|c| {
                full(kind(w, *c))
                    && full(kind(w, (c.0, c.1 - 1, c.2)))
                    && crate::stairs::refusal(w, &[*c], (c.0, c.1 - 1, c.2)).is_none()
            })
    }

    pub fn step(&mut self, w: &World, tick: u64) -> (Option<Act>, PlaceStatus) {
        if w.body.is_none() {
            return (None, PlaceStatus::Failed(PlaceFailure::Lost));
        }
        if let Some(d) = &mut self.clearing {
            let (_, act, status) = d.step(w, tick);
            match status {
                DigStatus::Running => return (act, PlaceStatus::Running),
                DigStatus::Done | DigStatus::Failed(_) => {
                    self.clearing = None;
                    if act.is_some() {
                        return (act, PlaceStatus::Running);
                    }
                }
            }
        }
        if let Some((c, sent)) = self.at {
            if kind(w, c) == Some(self.block) {
                return (None, PlaceStatus::Done(c));
            }
            if tick.saturating_sub(sent) < CONFIRM_TICKS {
                return (None, PlaceStatus::Running);
            }
            self.at = None;
            if self.tried.len() >= MAX_TRIES as usize {
                return (None, PlaceStatus::Failed(PlaceFailure::Refused));
            }
        }
        match hold::toward(w, self.item) {
            Hold::Held => {}
            Hold::Act(a) => return (Some(a), PlaceStatus::Running),
            Hold::Wait => return (None, PlaceStatus::Running),
            Hold::Missing => return (None, PlaceStatus::Failed(PlaceFailure::Missing)),
        }
        let Some(c) = self.choose(w) else {
            if self.cleared < MAX_CLEARED
                && let Some(wall) = self.wall(w)
            {
                self.cleared += 1;
                self.clearing = Some(Dig::new(w, wall, tick));
                return (None, PlaceStatus::Running);
            }
            return (None, PlaceStatus::Failed(PlaceFailure::NoRoom));
        };
        self.tried.push(c);
        self.at = Some((c, tick));
        let act = Act::UseOn {
            pos: BlockPos::new(c.0, c.1 - 1, c.2),
            face: 1,
        };
        (Some(act), PlaceStatus::Running)
    }
}
