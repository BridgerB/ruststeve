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
/// Blocks above and below the feet a site's floor may lie: a forest floor's slope within a walk,
/// and the floor from a bot on a fungus's cap (race 45534591345 lane 0 stood at y 56 over a floor
/// at 43 and found no site within 4).
pub const SITE_DY: i32 = 8;
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
/// Ground of the basalt deltas a burrow never stands on.
pub const DELTA_GROUND: &[BlockKind] = &[
    BlockKind::Basalt,
    BlockKind::PolishedBasalt,
    BlockKind::SmoothBasalt,
    BlockKind::Blackstone,
    BlockKind::MagmaBlock,
];
/// Blocks that hurt an enderman standing by them (and teleport it: a hurt from no one).
pub const HURTS: &[BlockKind] = &[
    BlockKind::MagmaBlock,
    BlockKind::Lava,
    BlockKind::Fire,
    BlockKind::SoulFire,
];
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

    /// The cells the built burrow is made of, in build order: the walls round B and M at the feet
    /// and the head (sides, back, back corners), the roof over B and M, then the outer ring two
    /// out on each side and behind the back wall: a solid block 4 long and 5 wide with the 1x2
    /// tunnel in it (38 blocks at most). For a mob 2.9 tall O is then the one standing cell 2
    /// from B, every other at sqrt(5) or more: with the cells beside B and behind its back wall
    /// open, an enderman held O about 200 ticks, then walked round to them, a tie for its partial
    /// path at 2, and paced there out of strike reach (P4's real parity run 45535279839; race
    /// 45534591345 lane 1). The floors are `floors`.
    pub fn shell(&self) -> Vec<[i32; 3]> {
        let s = self.side();
        let d = self.dir;
        // (along, aside): along 0 is B, 1 is M, -1 the back wall, -2 behind it.
        let at = |along: i32, aside: i32, y: i32| {
            add(
                self.b,
                d[0] * along + s[0] * aside,
                y,
                d[1] * along + s[1] * aside,
            )
        };
        let mut v = Vec::new();
        for y in 0..2 {
            for along in [0, 1] {
                v.push(at(along, 1, y));
                v.push(at(along, -1, y));
            }
            v.push(at(-1, 0, y));
            v.push(at(-1, 1, y));
            v.push(at(-1, -1, y));
        }
        v.push(add(self.b, 0, 2, 0));
        v.push(add(self.m, 0, 2, 0));
        for y in 0..2 {
            for along in -2..=1 {
                v.push(at(along, 2, y));
                v.push(at(along, -2, y));
            }
            for aside in -1..=1 {
                v.push(at(-2, aside, y));
            }
        }
        v
    }

    /// Whether `feet` stand on the burrow's top (over its roof or its walls): an enderman there is
    /// at 2 or so from B, over the roof, out of strike reach.
    pub fn on_top(&self, feet: [i32; 3]) -> bool {
        let rel = [feet[0] - self.b[0], feet[2] - self.b[2]];
        let s = self.side();
        let along = rel[0] * self.dir[0] + rel[1] * self.dir[1];
        let aside = rel[0] * s[0] + rel[1] * s[1];
        feet[1] >= self.b[1] + 2 && (-2..=1).contains(&along) && (-2..=2).contains(&aside)
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
    // The forest's own ground: no floor of the basalt deltas (magma cubes, ghasts; magma blocks
    // hurt endermen and teleport them: the frontier race 45544744907's one provoke on delta ground
    // stared at one that magma was hurting), and nylium under one of B, M, O unless the feet
    // stand in a warped forest.
    let floors = [b.b, b.m, b.o].map(|c| kind(w, add(c, 0, -1, 0)));
    if floors
        .iter()
        .any(|k| k.is_some_and(|k| DELTA_GROUND.contains(&k)))
    {
        return Some("delta ground");
    }
    if !floors.contains(&Some(BlockKind::WarpedNylium)) && w.feet_biome() != Some("warped_forest") {
        return Some("no forest floor");
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
                let k = kind(w, add(b.o, dx, dy, dz));
                if k == Some(BlockKind::Lava) {
                    return Some("lava near O");
                }
                if k.is_some_and(|k| HURTS.contains(&k)) && dx.abs() <= 2 && dz.abs() <= 2 {
                    return Some("magma or fire near O");
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
    site_toward(w, mem, feet, avoid, away, &[])
}

/// Blocks within which a sighting counts toward a mouth's side (the provoke's reach, 36).
pub const SIGHT_RANGE: f64 = 36.0;

/// How many of `seen` (ticking endermen's feet) lie within `SIGHT_RANGE` of B on the mouth's
/// side of the plane through B.
pub fn side_score(b: [i32; 3], dir: [i32; 2], seen: &[[f64; 3]]) -> usize {
    let c = Burrow::centre(b);
    seen.iter()
        .filter(|p| {
            let (dx, dz) = (p[0] - c[0], p[2] - c[2]);
            (dx * dx + dz * dz).sqrt() <= SIGHT_RANGE
                && dx * f64::from(dir[0]) + dz * f64::from(dir[1]) > 0.0
        })
        .count()
}

/// `site_avoiding`, the nearest B taken with its mouth toward the side the most of `seen` lie on
/// (endermen seen ticking lately: the mouth sees one way only, and three bots waited 3,300 to
/// 4,000 ticks with endermen 15 to 30 off out of its line, race 45534591345); ties east, west,
/// south, north.
pub fn site_toward(
    w: &World,
    mem: &Memory,
    feet: [i32; 3],
    avoid: &[[i32; 3]],
    away: i32,
    seen: &[[f64; 3]],
) -> (Option<Burrow>, Vec<(&'static str, u32)>) {
    site_toward_skipping(w, mem, feet, avoid, away, &[], seen)
}

/// `site_toward`, never a B within 1 of a cell of `skip` (sites refused): P4's closed loop
/// (seed 23) found the one refused site nearest the feet every search, and the hunt stood
/// "refused before" from tick 30,000 to the cap.
pub fn site_toward_skipping(
    w: &World,
    mem: &Memory,
    feet: [i32; 3],
    avoid: &[[i32; 3]],
    away: i32,
    skip: &[[i32; 3]],
    seen: &[[f64; 3]],
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
                }) && skip.iter().all(|a| {
                    (a[0] - c[0]).abs() > 1 || (a[1] - c[1]).abs() > 1 || (a[2] - c[2]).abs() > 1
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
        let mut dirs = DIRS;
        dirs.sort_by_key(|d| std::cmp::Reverse(side_score(c, *d, seen)));
        for dir in dirs {
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
