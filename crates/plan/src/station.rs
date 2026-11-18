//! A cast's lava station (issue 14c, docs/problems/portal.md wall 18), after clojurecraft's
//! `station-among`, `refill`, `stance-block` and `scoop-at-station` (build.clj at be85a92).
//!
//! The station is a stand at a height `h` over the room's floor (one to `MAX_ACCESS`), four to
//! nine in front of the frame and up to eight aside of the landing's column, with lava sources
//! below the feet and within a bucket's reach. The room's access steps climb to `h`; the way to
//! the stand runs from there, walked when it is open, dug when it is rock. Every refill takes the
//! same shape: to the stand, then the scoop. Strict no X-ray (Bridger, 2026-11-15): a cell the
//! eye has not seen counts as rock (firm, diggable, dry); only seen fluid refuses.

use data::BlockKind;
use world::{BlockPos, World};

use crate::Cell;
use crate::build::{Facing, Frame, MAX_ACCESS, landing};

/// How far in front of the frame a stand may be, and how far aside of the landing's column.
pub const DEPTH_FROM: i32 = 4;
pub const DEPTH_TO: i32 = 9;
pub const SPAN: i32 = 8;
/// A source is in reach when the eye at the stand's middle is this near its centre (the game's
/// 4.5 less a margin).
pub const REACH: f64 = 4.3;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Station {
    pub stand: Cell,
    pub h: i32,
    /// The sources in reach, nearest the eye first.
    pub sources: Vec<Cell>,
}

fn kind(w: &World, (x, y, z): Cell) -> Option<BlockKind> {
    w.block_at(BlockPos::new(x, y, z))
}

fn full(k: BlockKind) -> bool {
    data::generated::shapes::collision(k) == Some(data::types::Collision::Full)
}

fn fluid(k: BlockKind) -> bool {
    matches!(
        k,
        BlockKind::Water | BlockKind::Lava | BlockKind::BubbleColumn
    )
}

/// Firm to stand on: seen solid and not falling, or unseen (rock).
fn firm(w: &World, c: Cell) -> bool {
    crate::cast::seen(w, c).is_none_or(|k| {
        full(k) && !matches!(k, BlockKind::Gravel | BlockKind::Sand | BlockKind::RedSand)
    })
}

/// A stand or a way may be opened through it: loaded, not seen holding fluid, never obsidian.
pub fn openable(w: &World, c: Cell) -> bool {
    kind(w, c).is_some()
        && crate::cast::seen(w, c).is_none_or(|k| !fluid(k) && k != BlockKind::Obsidian)
}

fn neighbours((x, y, z): Cell) -> [Cell; 6] {
    [
        (x + 1, y, z),
        (x - 1, y, z),
        (x, y + 1, z),
        (x, y - 1, z),
        (x, y, z + 1),
        (x, y, z - 1),
    ]
}

/// No fluid seen in any of `cells`' neighbours.
fn dry_round(w: &World, cells: &[Cell]) -> bool {
    cells
        .iter()
        .flat_map(|c| neighbours(*c))
        .all(|n| crate::cast::seen(w, n).is_none_or(|k| !fluid(k)))
}

fn eye_of((x, y, z): Cell) -> [f64; 3] {
    [
        f64::from(x) + 0.5,
        f64::from(y) + physics::EYE_HEIGHT,
        f64::from(z) + 0.5,
    ]
}

fn centre((x, y, z): Cell) -> [f64; 3] {
    [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5]
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

/// The sources of `sources` a bot at `stand` reaches: below the feet, within `REACH` of the eye,
/// nearest first (`reach-sources`).
pub fn reach_sources(sources: &[Cell], stand: Cell) -> Vec<Cell> {
    let eye = eye_of(stand);
    let mut out: Vec<(f64, Cell)> = sources
        .iter()
        .filter(|s| s.1 < stand.1)
        .map(|s| (dist(eye, centre(*s)), *s))
        .filter(|(d, _)| *d <= REACH)
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    out.into_iter().map(|(_, s)| s).collect()
}

/// The frame's rock and the row behind it, which a way never digs (`frame-box`).
pub fn kept(frame: Frame) -> Vec<Cell> {
    let mut out = Vec::new();
    for x in -1..=4 {
        for y in -1..=5 {
            for z in [-1, 0] {
                out.push(frame.at((x, y, z)));
            }
        }
    }
    out
}

/// The station for the frame (`station-among`): `prefer` first, then every height one to
/// `MAX_ACCESS`; nearest the landing's column first; a stand with a firm floor, it and the head
/// openable and dry round, outside the room, with sources in reach.
pub fn station_among(
    w: &World,
    sources: &[Cell],
    frame: Frame,
    footprint: &[Cell],
    prefer: Option<i32>,
) -> Option<Station> {
    let heights = prefer
        .into_iter()
        .chain(1..=MAX_ACCESS)
        .filter(|h| (1..=MAX_ACCESS).contains(h));
    let mut tried = Vec::new();
    for h in heights {
        if tried.contains(&h) {
            continue;
        }
        tried.push(h);
        let (lx, _, _) = landing(h);
        let mut xs: Vec<i32> = (lx - SPAN..=lx + SPAN).collect();
        xs.sort_by_key(|x| ((x - lx).abs(), *x));
        for dz in DEPTH_FROM..=DEPTH_TO {
            for dx in &xs {
                let stand = frame.at((*dx, h, dz));
                let head = (stand.0, stand.1 + 1, stand.2);
                let floor = (stand.0, stand.1 - 1, stand.2);
                if !(firm(w, floor)
                    && openable(w, stand)
                    && openable(w, head)
                    && dry_round(w, &[stand, head])
                    && !footprint.contains(&stand))
                {
                    continue;
                }
                let srcs = reach_sources(sources, stand);
                if !srcs.is_empty() {
                    return Some(Station {
                        stand,
                        h,
                        sources: srcs,
                    });
                }
            }
        }
    }
    None
}

/// The columns of the straight way from `from` to `to`, x first then z (or z first), ends
/// included (`way-columns`).
fn way_columns(from: Cell, to: Cell, z_first: bool) -> Vec<(i32, i32)> {
    let span = |a: i32, b: i32| -> Vec<i32> {
        if a <= b {
            (a..=b).collect()
        } else {
            (b..=a).rev().collect()
        }
    };
    if z_first {
        let mut out: Vec<(i32, i32)> = span(from.2, to.2)
            .into_iter()
            .map(|z| (from.0, z))
            .collect();
        out.extend(span(from.0, to.0).into_iter().skip(1).map(|x| (x, to.2)));
        out
    } else {
        let mut out: Vec<(i32, i32)> = span(from.0, to.0)
            .into_iter()
            .map(|x| (x, from.2))
            .collect();
        out.extend(span(from.2, to.2).into_iter().skip(1).map(|z| (to.0, z)));
        out
    }
}

/// Whether a way may dig `c`: seen or unseen solid, not obsidian, not the frame's rock, no lava
/// seen in it or beside it.
fn diggable(w: &World, c: Cell, kept: &[Cell]) -> bool {
    kind(w, c).is_some_and(full)
        && kind(w, c) != Some(BlockKind::Obsidian)
        && !kept.contains(&c)
        && std::iter::once(c)
            .chain(neighbours(c))
            .all(|n| crate::cast::seen(w, n) != Some(BlockKind::Lava))
}

/// The straight way to a stand at its height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    /// No solid cell on it.
    Open,
    /// The first solid cell on it, which a way may dig.
    Dig(Cell),
    /// Both lines cross a cell a way may not dig.
    Barred,
}

/// The first solid cell on the way from `feet` to `stand` at the stand's height (the body, then
/// the head), the line along x first, else z first when x first has a cell a way may not dig
/// (`stance-block`).
pub fn stance_block(w: &World, feet: Cell, stand: Cell, kept: &[Cell]) -> Way {
    let sy = stand.1;
    let line = |z_first: bool| -> Vec<Cell> {
        way_columns(feet, stand, z_first)
            .into_iter()
            .flat_map(|(x, z)| [(x, sy, z), (x, sy + 1, z)])
            .collect()
    };
    let solid = |c: &Cell| kind(w, *c).is_some_and(full);
    let barred = |cells: &[Cell]| cells.iter().any(|c| solid(c) && !diggable(w, *c, kept));
    let (a, b) = (line(false), line(true));
    let chosen = if !barred(&a) {
        a
    } else if !barred(&b) {
        b
    } else {
        return Way::Barred;
    };
    chosen
        .into_iter()
        .find(|c| solid(c))
        .map_or(Way::Open, Way::Dig)
}

/// Whether a sight dig may take `c`: solid, not obsidian, not kept, and no lava seen beside it at
/// its height or over it: lava never flows up, so a lid over lava opens safely, while lava level
/// with the block or higher would run in (clojurecraft's `safe-source?`; wall 24).
fn sight_diggable(w: &World, c: Cell, kept: &[Cell]) -> bool {
    kind(w, c).is_some_and(full)
        && kind(w, c) != Some(BlockKind::Obsidian)
        && !kept.contains(&c)
        && neighbours(c)
            .iter()
            .filter(|n| n.1 >= c.1)
            .all(|n| crate::cast::seen(w, *n) != Some(BlockKind::Lava))
}

/// The first solid block on the eye's ray to `src`'s centre that a refill may dig to open its
/// sight (`source-in-sight`): below the feet's height only (lava let in there stays under the
/// stand), never the frame's rock.
pub fn sight_block(
    w: &World,
    eye: [f64; 3],
    src: Cell,
    feet_y: i32,
    kept: &[Cell],
) -> Option<Cell> {
    let to = centre(src);
    let steps = 40;
    let mut last = None;
    for i in 1..steps {
        let t = f64::from(i) / f64::from(steps);
        let p = [
            eye[0] + (to[0] - eye[0]) * t,
            eye[1] + (to[1] - eye[1]) * t,
            eye[2] + (to[2] - eye[2]) * t,
        ];
        let c = (
            p[0].floor() as i32,
            p[1].floor() as i32,
            p[2].floor() as i32,
        );
        if c == src || Some(c) == last {
            continue;
        }
        last = Some(c);
        if kind(w, c).is_some_and(full) {
            // Never the floor the body stands on: a block one under the feet whose column the
            // box overlaps (the eye's x and z are the body's). Place 0 of kitlava race
            // 44236062785 dug (26, -53, -72) from (26.5, -52, -71.44), fell three blocks and
            // died in the lava sea's cave, in 13 of 16 lanes (wall 35).
            let half = physics::HALF_WIDTH;
            let under = c.1 == feet_y - 1
                && eye[0] + half > f64::from(c.0)
                && eye[0] - half < f64::from(c.0) + 1.0
                && eye[2] + half > f64::from(c.2)
                && eye[2] - half < f64::from(c.2) + 1.0;
            return (c.1 < feet_y && !under && sight_diggable(w, c, kept)).then_some(c);
        }
    }
    None
}

/// The stand's cell relative to the frame, for the notes.
pub fn facing_name(f: Facing) -> &'static str {
    f.name()
}
