//! A synthetic stronghold for closed-loop tests (issue 19g, docs/problems/eyes-model.md): the
//! portal room laid by `StrongholdPieces$PortalRoom.postProcess` as the 26.1.2 jar has it (probe
//! 45685968956), a corridor from its door, infested bricks in their walls, buried in an
//! overworld's rock. Not vanilla's generator: one room in the piece's own orientation (its door
//! to the north, its frames to the south), the bricks' kinds and the infested ones on a fixed
//! pattern rather than the selector's random draw, the door's grates simplified.
//!
//! Piece-local cells (x 0..10, y 0..7, z 0..15) from `origin`, the shell's lowest north-west
//! corner: the feet's level is y 1 on the y 0 floor; frames at y 3 round (5, 3, 10); the lava
//! pool (4..6, 1, 9..11) under the ring in a platform (3..7, 1, 8..12); side lava (1, 1, 1..3)
//! and (9, 1, 1..3) in ledges (1..2, 1, 1..4) and (8..9, 1, 1..4); stairs up southward at z 4 to
//! 6; the silverfish spawner (5, 3, 6) at their top; the door at (4..6, 1..3, 0).

use std::collections::BTreeMap;

use data::BlockKind;
use protocol::EncodeError;
use protocol::chunk::{Paletted, block_index};

use crate::survival::with_property;
use crate::{ModelColumn, ModelWorld};

type Pos = (i32, i32, i32);

fn state(k: BlockKind) -> u32 {
    k.info().map_or(0, |i| i.default_state)
}

fn facing(k: BlockKind, to: &str) -> u32 {
    with_property(state(k), "facing", to).unwrap_or_else(|| state(k))
}

/// What a test reads of the room it laid, in world cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Room {
    /// The shell's lowest north-west corner (piece-local (0, 0, 0)).
    pub origin: Pos,
    /// The twelve frames, in the template's order.
    pub frames: Vec<Pos>,
    /// The ring's centre at the frames' height (piece-local (5, 3, 10)): the portal's middle.
    pub centre: Pos,
    /// The feet's level on the room's floor (piece-local y 1).
    pub floor_y: i32,
    /// Lava cells: the pool under the ring and the two side pools.
    pub lava: Vec<Pos>,
    pub spawner: Pos,
    /// The door's middle at the feet's level (piece-local (5, 1, 0)).
    pub door: Pos,
    /// Infested bricks laid in the shell and the corridor.
    pub infested: Vec<Pos>,
}

/// A shell brick's kind by a fixed hash of its cell, in the selector's proportions
/// (`SmoothStoneSelector`: cracked under 0.2, mossy under 0.5, infested under 0.55, of 100).
fn brick(p: Pos) -> BlockKind {
    let h = (p.0.wrapping_mul(73_856_093)
        ^ p.1.wrapping_mul(19_349_663)
        ^ p.2.wrapping_mul(83_492_791))
    .rem_euclid(100);
    match h {
        0..20 => BlockKind::CrackedStoneBricks,
        20..50 => BlockKind::MossyStoneBricks,
        50..55 => BlockKind::InfestedStoneBricks,
        _ => BlockKind::StoneBricks,
    }
}

/// Cells laid before they are written into the world's columns, the last write of a cell wins.
#[derive(Default)]
struct Lay {
    cells: BTreeMap<Pos, u32>,
}

impl Lay {
    fn set(&mut self, p: Pos, s: u32) {
        self.cells.insert(p, s);
    }

    /// A box: its edges (the outermost cells on any axis) of bricks, its inside of air.
    fn shell(&mut self, lo: Pos, hi: Pos) {
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                for z in lo.2..=hi.2 {
                    let edge =
                        x == lo.0 || x == hi.0 || y == lo.1 || y == hi.1 || z == lo.2 || z == hi.2;
                    let s = if edge {
                        state(brick((x, y, z)))
                    } else {
                        state(BlockKind::Air)
                    };
                    self.set((x, y, z), s);
                }
            }
        }
    }

    fn fill(&mut self, lo: Pos, hi: Pos, s: u32) {
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                for z in lo.2..=hi.2 {
                    self.set((x, y, z), s);
                }
            }
        }
    }

    /// A box of bricks (the selector's kinds).
    fn bricks(&mut self, lo: Pos, hi: Pos) {
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                for z in lo.2..=hi.2 {
                    self.set((x, y, z), state(brick((x, y, z))));
                }
            }
        }
    }

    /// Write every laid cell into the world, one re-encode a column.
    fn write(&self, world: &mut ModelWorld) -> Result<(), EncodeError> {
        let mut by_column: BTreeMap<(i32, i32), Vec<(Pos, u32)>> = BTreeMap::new();
        for (&p, &s) in &self.cells {
            by_column
                .entry((p.0 >> 4, p.2 >> 4))
                .or_default()
                .push((p, s));
        }
        let air = state(BlockKind::Air);
        for (key, cells) in by_column {
            let Some(mc) = world.columns.get(&key) else {
                continue;
            };
            let mut column = (*mc.column).clone();
            let mut sections: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
            for ((x, y, z), s) in cells {
                let Ok(si) = usize::try_from((y - column.min_y) >> 4) else {
                    continue;
                };
                let Some(section) = column.sections.get(si) else {
                    continue;
                };
                let held = sections.entry(si).or_insert_with(|| {
                    (0..4096)
                        .map(|i| section.blocks.get(i).unwrap_or(0))
                        .collect()
                });
                if let Some(c) = held.get_mut(block_index(x, y, z)) {
                    *c = s;
                }
            }
            for (si, held) in sections {
                if let Some(section) = column.sections.get_mut(si) {
                    section.block_count = i16::try_from(held.iter().filter(|c| **c != air).count())
                        .unwrap_or(i16::MAX);
                    section.blocks = Paletted::pack(&held, 4);
                }
            }
            world
                .columns
                .insert(key, ModelColumn::generated(key.0, key.1, column)?);
        }
        Ok(())
    }
}

/// Lay the portal room with its shell's lowest north-west corner at `origin`, and a corridor
/// `corridor` long running north from its door (a 5 wide, 5 high brick tube, 3 by 3 inside, at
/// the door's height). The rock round them is the world's.
pub fn portal_room(
    world: &mut ModelWorld,
    origin: Pos,
    corridor: i32,
) -> Result<Room, EncodeError> {
    let at = |x: i32, y: i32, z: i32| (origin.0 + x, origin.1 + y, origin.2 + z);
    let mut lay = Lay::default();
    let lava = state(BlockKind::Lava);
    // The shell and its inside.
    lay.shell(at(0, 0, 0), at(10, 7, 15));
    // The ledges at y 6 round the walls.
    lay.bricks(at(1, 6, 1), at(1, 6, 14));
    lay.bricks(at(9, 6, 1), at(9, 6, 14));
    lay.bricks(at(2, 6, 1), at(8, 6, 2));
    lay.bricks(at(2, 6, 14), at(8, 6, 14));
    // The side ledges and their lava.
    lay.bricks(at(1, 1, 1), at(2, 1, 4));
    lay.bricks(at(8, 1, 1), at(9, 1, 4));
    lay.fill(at(1, 1, 1), at(1, 1, 3), lava);
    lay.fill(at(9, 1, 1), at(9, 1, 3), lava);
    // The platform and the pool under the ring.
    lay.bricks(at(3, 1, 8), at(7, 1, 12));
    lay.fill(at(4, 1, 9), at(6, 1, 11), lava);
    // Iron bars in the side walls and the south wall.
    let bars = state(BlockKind::IronBars);
    for z in (3..14).step_by(2) {
        lay.fill(at(0, 3, z), at(0, 4, z), bars);
        lay.fill(at(10, 3, z), at(10, 4, z), bars);
    }
    for x in (2..9).step_by(2) {
        lay.fill(at(x, 3, 15), at(x, 4, 15), bars);
    }
    // The stairs up to the south, their fill under them.
    lay.bricks(at(4, 1, 5), at(6, 1, 7));
    lay.bricks(at(4, 2, 6), at(6, 2, 7));
    lay.bricks(at(4, 3, 7), at(6, 3, 7));
    let stairs = facing(BlockKind::StoneBrickStairs, "south");
    for x in 4..=6 {
        lay.set(at(x, 1, 4), stairs);
        lay.set(at(x, 2, 5), stairs);
        lay.set(at(x, 3, 6), stairs);
    }
    // The frames, facing into the ring, no eyes.
    let frame = |to: &str| facing(BlockKind::EndPortalFrame, to);
    let frames: Vec<Pos> = [
        (4, 8, "south"),
        (5, 8, "south"),
        (6, 8, "south"),
        (4, 12, "north"),
        (5, 12, "north"),
        (6, 12, "north"),
    ]
    .into_iter()
    .map(|(x, z, f)| (at(x, 3, z), f))
    .chain(
        [(3, "east"), (7, "west")]
            .into_iter()
            .flat_map(|(x, f)| (9..=11).map(move |z| (at(x, 3, z), f))),
    )
    .map(|(p, f)| {
        lay.set(p, frame(f));
        p
    })
    .collect();
    // The spawner at the stairs' top.
    let spawner = at(5, 3, 6);
    lay.set(spawner, state(BlockKind::Spawner));
    // The door: grates round an opening one wide and two high.
    lay.fill(at(4, 1, 0), at(6, 3, 0), bars);
    lay.fill(at(5, 1, 0), at(5, 2, 0), state(BlockKind::Air));
    // The corridor north from the door.
    if corridor > 0 {
        lay.shell(at(3, 0, -corridor), at(7, 4, -1));
        lay.fill(at(4, 1, -1), at(6, 3, -1), state(BlockKind::Air));
    }
    lay.write(world)?;
    let infested = lay
        .cells
        .iter()
        .filter(|(_, s)| data::block_of_state(**s) == Some(BlockKind::InfestedStoneBricks))
        .map(|(p, _)| *p)
        .collect();
    let lava_cells = lay
        .cells
        .iter()
        .filter(|(_, s)| **s == lava)
        .map(|(p, _)| *p)
        .collect();
    Ok(Room {
        origin,
        frames,
        centre: at(5, 3, 10),
        floor_y: origin.1 + 1,
        lava: lava_cells,
        spawner,
        door: at(5, 1, 0),
        infested,
    })
}

/// An overworld of flat ground (`flat_world`: stone to `ground_y - 1`, grass on top) with the
/// portal room buried at `origin` and a corridor north of it; the player spawns on the grass at
/// (0.5, ground_y + 1, 0.5).
pub fn stronghold_world(
    radius: i32,
    ground_y: i32,
    origin: Pos,
    corridor: i32,
) -> Result<(ModelWorld, Room), EncodeError> {
    let mut world = crate::flat_world(radius, ground_y)?;
    let room = portal_room(&mut world, origin, corridor)?;
    Ok((world, room))
}
