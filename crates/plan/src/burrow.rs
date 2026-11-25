//! The burrow (issue 18e, `docs/problems/pearls-hunt.md`): a level burrow one wide and two high,
//! built of held filler on the warped forest's floor, that an enderman cannot reach into.
//!
//! ```text
//!    side view (x along the burrow)          top view
//!    roof:   ##########                      #  #  #
//!            #  B  M |   O                   #  B  M  O   <- the mouth faces open floor
//!    floor:  ##########_______               #  #  #
//! ```
//!
//! B is the bot's cell, M the mouth cell, O the cell outside where the enderman stands. An
//! enderman is 2.9 tall, so its box cannot enter M (roof at 2) and its centre stays 0.3 or more
//! outside the M/O face; its reach (its box grown 0.83 sideways, 1.43 centre to centre with the
//! player's half width) stops 0.37 short of the bot's centre in B, 1.5 behind the face, while the
//! bot's 3.0 reaches its box. From B the roof cuts the line from the bot's eye to the enderman's
//! at O, so no stare can freeze it there; a teleport needs 3 free cells over a floor and B and M
//! are 2 high (the plan's section 3, from the 26.1.2 jar).
//!
//! Pure: the site and the missing blocks are read from the world; nothing here acts.

use data::BlockKind;
use memory::Memory;
use world::World;

use crate::path::{Cell, Cells};

/// The site, chosen by the cell the bot stands on (issue 17's valley lesson).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Burrow {
    /// The bot's cell (feet).
    pub b: [i32; 3],
    /// The mouth cell, `b + dir`.
    pub m: [i32; 3],
    /// The cell outside, `m + dir`, where the enderman stands.
    pub o: [i32; 3],
    /// From B toward the mouth on the plane: a unit step on x or z.
    pub dir: [i32; 2],
}

/// Blocks round the feet a site is looked for in (the plan's 18e: nearest the feet within 24).
pub const SITE_RADIUS: i32 = 24;
/// Blocks above and below the feet a site's floor may lie: a forest floor's slope within a walk.
pub const SITE_DY: i32 = 4;
/// Cells over O (and over its five outward neighbours) that must be clear: an enderman is 2.9
/// tall and stands and teleports only where three cells are free (jar, `EnderMan.teleport`).
pub const OPEN_OVER_O: i32 = 3;
/// Walkable cells beyond O on the burrow's line: the way in (clojurecraft: a mouth under a cap
/// or up stairs drew no enderman, 6 of 6 fights timed out).
pub const OPEN_BEYOND: i32 = 4;
/// No lava within this of O.
pub const LAVA_NEAR: i32 = 4;
/// No drop deeper than `DROP_MAX` within `DROP_NEAR` of O (a knocked-back enderman, a pickup
/// step).
pub const DROP_NEAR: i32 = 3;
pub const DROP_MAX: i32 = 2;
/// No nether brick seen within this of B (a fortress: wither skeletons, blazes).
pub const BRICKS_NEAR: i32 = 8;

/// The four directions on the plane, in the order sites are tried (east, west, south, north).
const DIRS: [[i32; 2]; 4] = [[1, 0], [-1, 0], [0, 1], [0, -1]];

fn add(c: [i32; 3], dx: i32, dy: i32, dz: i32) -> [i32; 3] {
    [c[0] + dx, c[1] + dy, c[2] + dz]
}

fn cell(c: [i32; 3]) -> Cell {
    (c[0], c[1], c[2])
}

impl Burrow {
    /// The burrow with B at `b`, its mouth toward `dir`.
    pub fn at(b: [i32; 3], dir: [i32; 2]) -> Self {
        let m = add(b, dir[0], 0, dir[1]);
        let o = add(m, dir[0], 0, dir[1]);
        Self { b, m, o, dir }
    }

    /// The side step on the plane (a quarter turn of `dir`).
    pub fn side(&self) -> [i32; 2] {
        [-self.dir[1], self.dir[0]]
    }

    /// The cells the built burrow is made of: the back wall, the side walls of B and M at the
    /// feet and the head, the roof over B and M (12 blocks at most). The floors are `floors`.
    pub fn shell(&self) -> Vec<[i32; 3]> {
        let s = self.side();
        let mut v = Vec::new();
        for y in 0..2 {
            v.push(add(self.b, -self.dir[0], y, -self.dir[1]));
            for c in [self.b, self.m] {
                v.push(add(c, s[0], y, s[1]));
                v.push(add(c, -s[0], y, -s[1]));
            }
        }
        v.push(add(self.b, 0, 2, 0));
        v.push(add(self.m, 0, 2, 0));
        v
    }

    /// The floor cells under B and M.
    pub fn floors(&self) -> [[i32; 3]; 2] {
        [add(self.b, 0, -1, 0), add(self.m, 0, -1, 0)]
    }

    /// The mouth's two cells (a seal fills them).
    pub fn mouth(&self) -> [[i32; 3]; 2] {
        [self.m, add(self.m, 0, 1, 0)]
    }

    /// Whether a feet cell is inside the burrow (B or M).
    pub fn inside(&self, feet: [i32; 3]) -> bool {
        feet == self.b || feet == self.m
    }

    /// The point at the middle of a cell's floor.
    pub fn centre(c: [i32; 3]) -> [f64; 3] {
        [
            f64::from(c[0]) + 0.5,
            f64::from(c[1]),
            f64::from(c[2]) + 0.5,
        ]
    }

    /// The yaw that looks out of the mouth (Notchian: south 0, west +90).
    pub fn out_yaw(&self) -> f64 {
        (-f64::from(self.dir[0]))
            .atan2(f64::from(self.dir[1]))
            .to_degrees()
    }
}

fn kind(w: &World, c: [i32; 3]) -> Option<BlockKind> {
    w.block_at(world::BlockPos::new(c[0], c[1], c[2]))
}

fn full(k: Option<BlockKind>) -> bool {
    k.and_then(data::generated::shapes::collision) == Some(data::types::Collision::Full)
}

/// Whether an enderman can carry the block away (`#enderman_holdable`, the 26.1.2 jar's tag):
/// warped nylium, dirt, gravel and fungus are; cobblestone, netherrack and deepslate are not.
pub fn holdable(k: BlockKind) -> bool {
    data::generated::tags::block_tag("enderman_holdable").is_some_and(|t| t.contains(&k))
}

/// Whether a shell cell can be (or already is) a burrow wall: a full block no enderman can
/// take, or a cell a held block can be put into (air, roots, sprouts: `#replaceable`).
fn wall_ok(w: &World, c: [i32; 3]) -> bool {
    match kind(w, c) {
        None => false,
        Some(k) if full(Some(k)) => !holdable(k),
        Some(k) => data::generated::tags::block_tag("replaceable").is_some_and(|t| t.contains(&k)),
    }
}

/// The blocks the burrow lacks, in build order (floors, then the walls nearest the mouth's
/// sides, then the back, then the roof): every shell cell not a full block, every floor not
/// solid. Read from the world each time: a block an enderman took or a creeper broke shows here.
pub fn missing(w: &World, b: &Burrow) -> Vec<[i32; 3]> {
    let floors = b.floors().into_iter().filter(|c| !full(kind(w, *c)));
    let shell = b.shell().into_iter().filter(|c| !full(kind(w, *c)));
    floors.chain(shell).collect()
}

/// Why a site at B toward `dir` is refused, or `None` when it is a site. Cheap tests first.
pub fn refuse(w: &World, mem: &Memory, cells: &mut Cells<'_>, b: &Burrow) -> Option<&'static str> {
    // B, M and O stand on one level of solid floor; B and M are clear two high.
    if !cells.standable(cell(b.b)) || !cells.standable(cell(b.m)) {
        return Some("no standing at B or M");
    }
    let s = b.side();
    // O and its five outward neighbours: a floor under O and three clear cells over each.
    if !cells.floor(cell(add(b.o, 0, -1, 0))) {
        return Some("no floor at O");
    }
    let outward = [
        b.o,
        add(b.o, b.dir[0], 0, b.dir[1]),
        add(b.o, s[0], 0, s[1]),
        add(b.o, -s[0], 0, -s[1]),
        add(b.o, b.dir[0] + s[0], 0, b.dir[1] + s[1]),
        add(b.o, b.dir[0] - s[0], 0, b.dir[1] - s[1]),
    ];
    if !outward
        .iter()
        .all(|c| (0..OPEN_OVER_O).all(|y| cells.clear(cell(add(*c, 0, y, 0)))))
    {
        return Some("under 3 clear over O");
    }
    // The way in: walkable cells beyond O on the line, three high.
    if !(1..=OPEN_BEYOND).all(|k| {
        let c = add(b.o, b.dir[0] * k, 0, b.dir[1] * k);
        cells.floor(cell(add(c, 0, -1, 0)))
            && (0..OPEN_OVER_O).all(|y| cells.clear(cell(add(c, 0, y, 0))))
    }) {
        return Some("no open floor beyond O");
    }
    // The walls and the roof: each a block no enderman can take, or room to put one.
    if !b.shell().iter().all(|c| wall_ok(w, *c)) {
        return Some("a wall cell holds a block that cannot stay");
    }
    // No drop of more than two within three of O.
    for dx in -DROP_NEAR..=DROP_NEAR {
        for dz in -DROP_NEAR..=DROP_NEAR {
            let c = add(b.o, dx, 0, dz);
            if cells.at(cell(c)) != crate::path::Class::Clear {
                continue;
            }
            if !(1..=DROP_MAX + 1).any(|d| cells.floor(cell(add(c, 0, -d, 0)))) {
                return Some("a drop near O");
            }
        }
    }
    // No lava within four of O.
    for dx in -LAVA_NEAR..=LAVA_NEAR {
        for dy in -LAVA_NEAR..=LAVA_NEAR {
            for dz in -LAVA_NEAR..=LAVA_NEAR {
                if kind(w, add(b.o, dx, dy, dz)) == Some(BlockKind::Lava) {
                    return Some("lava near O");
                }
            }
        }
    }
    if bricks_within(w, mem, b.b, BRICKS_NEAR) {
        return Some("nether bricks near");
    }
    None
}

/// Whether a nether brick on record lies within `r` of `c` (memory's brick index for the
/// columns round it).
pub fn bricks_within(w: &World, mem: &Memory, c: [i32; 3], r: i32) -> bool {
    let Some(dim) = w.dimension.as_ref() else {
        return false;
    };
    let (x0, x1) = ((c[0] - r) >> 4, (c[0] + r) >> 4);
    let (z0, z1) = ((c[2] - r) >> 4, (c[2] + r) >> 4);
    (x0..=x1).any(|cx| {
        (z0..=z1).any(|cz| {
            mem.indexed_in_column(&dim.name, cx, cz, BlockKind::NetherBricks)
                .unwrap_or_default()
                .iter()
                .any(|b| {
                    (b.0 - c[0]).abs() <= r && (b.1 - c[1]).abs() <= r && (b.2 - c[2]).abs() <= r
                })
        })
    })
}

/// The site nearest the feet within `SITE_RADIUS` (`site_avoiding` with nothing avoided).
pub fn site(w: &World, mem: &Memory, feet: [i32; 3]) -> Option<Burrow> {
    site_avoiding(w, mem, feet, &[], 0).0
}

/// The site nearest the feet within `SITE_RADIUS` whose B is farther than `away` (on the plane)
/// from every cell of `avoid` (sites refused or moved from), and how many were refused, by
/// reason. The search reads cells beside each candidate only; it is run once a
/// `PearlRules::site_every` (the plan's risk 8: the CPU).
pub fn site_avoiding(
    w: &World,
    mem: &Memory,
    feet: [i32; 3],
    avoid: &[[i32; 3]],
    away: i32,
) -> (Option<Burrow>, Vec<(&'static str, u32)>) {
    let mut cand: Vec<(i32, [i32; 3])> = Vec::new();
    for dx in -SITE_RADIUS..=SITE_RADIUS {
        for dz in -SITE_RADIUS..=SITE_RADIUS {
            if dx * dx + dz * dz > SITE_RADIUS * SITE_RADIUS {
                continue;
            }
            for dy in -SITE_DY..=SITE_DY {
                let c = add(feet, dx, dy, dz);
                let far = avoid.iter().all(|a| {
                    let (ax, az) = (a[0] - c[0], a[2] - c[2]);
                    ax * ax + az * az > away * away
                });
                if far {
                    cand.push((dx * dx + dy * dy + dz * dz, c));
                }
            }
        }
    }
    cand.sort();
    let mut cells = Cells::new(w);
    let mut why: Vec<(&'static str, u32)> = Vec::new();
    for (_, c) in cand {
        // Most cells are rock or air: a standing cell first, before any direction.
        if !cells.standable(cell(c)) {
            continue;
        }
        for dir in DIRS {
            let b = Burrow::at(c, dir);
            match refuse(w, mem, &mut cells, &b) {
                None => return (Some(b), why),
                Some(r) => match why.iter_mut().find(|(k, _)| *k == r) {
                    Some(e) => e.1 += 1,
                    None => why.push((r, 1)),
                },
            }
        }
    }
    (None, why)
}
