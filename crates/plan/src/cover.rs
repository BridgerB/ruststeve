//! Cover from shooters (issue 16, clojurecraft's `blaze.clj` `cover` and `unseen-from?`): a hurt
//! bot gets out of every shooter's line of sight and rests there until its health is back. Burning
//! cost clojurecraft 45% of the health its blaze fights cost, and nine of its twenty deaths came
//! fighting or backing off in the open (`brain/oracle/architecture/cc-blaze-hunt.md`).
//!
//! Where no cell out of sight can be walked to, the bot makes one: a held block into each cell a
//! shooter's line comes in through (`shelter_block`), until none sees it
//! (`docs/problems/spawner.md` wall 3).

use data::EntityKind;
use world::World;

use crate::path::{Cell, Cells, feet_cell};

/// Blocks within which a blaze's or a ghast's line of sight counts: a blaze follows and shoots
/// from 48 (its `FOLLOW_RANGE`), a ghast from 64. It was 16 until three armoured bots rested "out
/// of sight" on an open bridge with blazes 16 to 24 blocks off and were shot dead there (race
/// 45335627948).
pub const COVER_RANGE: f64 = 48.0;
/// Blocks within which a piglin's or a skeleton's counts: a crossbow and a bow are aimed from 8
/// to 15 (one of those bots took five arrows from a piglin 5 to 7 blocks off while resting).
pub const NEAR_RANGE: f64 = 16.0;
/// Cells each way a cover cell is looked for in.
pub const COVER_RADIUS: i32 = 8;
/// Shooters from `COVER_RANGE`, and from `NEAR_RANGE`.
pub const FAR_SHOOTERS: &[EntityKind] = &[EntityKind::Blaze, EntityKind::Ghast];
pub const NEAR_SHOOTERS: &[EntityKind] = &[EntityKind::Piglin, EntityKind::Skeleton];

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

/// The eyes of the shooters in range of the feet: `FAR_SHOOTERS` within `COVER_RANGE`,
/// `NEAR_SHOOTERS` within `NEAR_RANGE`.
pub fn shooters(w: &World) -> Vec<[f64; 3]> {
    let Some(body) = w.body else {
        return Vec::new();
    };
    w.entities
        .values()
        .filter(|e| !e.dead)
        .filter(|e| {
            let d = distance(e.pos, body.pos);
            (FAR_SHOOTERS.contains(&e.kind) && d <= COVER_RANGE)
                || (NEAR_SHOOTERS.contains(&e.kind) && d <= NEAR_RANGE)
        })
        .map(|e| [e.pos[0], e.pos[1] + 1.2, e.pos[2]])
        .collect()
}

/// The eye of a body whose feet stand in `c`.
pub fn eye_at(c: Cell) -> [f64; 3] {
    [
        f64::from(c.0) + 0.5,
        f64::from(c.1) + physics::EYE_HEIGHT,
        f64::from(c.2) + 0.5,
    ]
}

/// Whether any of `eyes` has a clear line (exact traversal, open cells only) to a body whose feet
/// stand in `c`, to its eye or its middle.
pub fn seen_at(w: &World, eyes: &[[f64; 3]], c: Cell) -> bool {
    let head = eye_at(c);
    let middle = [head[0], f64::from(c.1) + 0.9, head[2]];
    eyes.iter().any(|from| {
        crate::scoop::exact_ray(w, *from, head, (c.0, c.1 + 1, c.2))
            || crate::scoop::exact_ray(w, *from, middle, c)
    })
}

/// The standable cells within `COVER_RADIUS` that none of `eyes` sees, nearest the feet first.
pub fn cover_cells(w: &World, eyes: &[[f64; 3]]) -> Vec<Cell> {
    let Some(body) = w.body else {
        return Vec::new();
    };
    let f = feet_cell(body.pos);
    let mut cells = Cells::new(w);
    let mut out: Vec<(i32, Cell)> = Vec::new();
    for dx in -COVER_RADIUS..=COVER_RADIUS {
        for dy in -2..=2 {
            for dz in -COVER_RADIUS..=COVER_RADIUS {
                let c = (f.0 + dx, f.1 + dy, f.2 + dz);
                if cells.standable(c) && !seen_at(w, eyes, c) {
                    out.push((dx * dx + dy * dy + dz * dz, c));
                }
            }
        }
    }
    out.sort();
    out.into_iter().map(|(_, c)| c).collect()
}

/// The cells a body standing at `pos` is in: every column its box touches, two high.
pub fn occupied(pos: [f64; 3]) -> Vec<Cell> {
    let b = physics::aabb(pos);
    let e = 0.001;
    let f = |v: f64| v.floor() as i32;
    let y = f(pos[1] + e);
    let mut out = Vec::new();
    for x in f(b[0] + e)..=f(b[3] - e) {
        for z in f(b[2] + e)..=f(b[5] - e) {
            out.push((x, y, z));
            out.push((x, y + 1, z));
        }
    }
    out
}

fn open(w: &World, c: Cell) -> bool {
    w.block_at(world::BlockPos::new(c.0, c.1, c.2))
        .and_then(data::generated::shapes::collision)
        .is_some_and(|k| k == data::types::Collision::Empty)
}

fn full(w: &World, c: Cell) -> bool {
    w.block_at(world::BlockPos::new(c.0, c.1, c.2))
        .and_then(data::generated::shapes::collision)
        == Some(data::types::Collision::Full)
}

/// The two points of a body at `pos` a shooter aims at: its eye and its middle.
fn aims(pos: [f64; 3]) -> [[f64; 3]; 2] {
    [
        [pos[0], pos[1] + physics::EYE_HEIGHT, pos[2]],
        [pos[0], pos[1] + 0.9, pos[2]],
    ]
}

/// The cell the line from `eye` to `aim` comes into the body's own cells through, when every
/// cell before it is open: the one cell a block in would stop that line. `None` when the line is
/// stopped already, or starts inside the body's cells.
pub fn entry(w: &World, eye: [f64; 3], aim: [f64; 3], body: &[Cell]) -> Option<Cell> {
    let mut last = None;
    for c in crate::scoop::ray_cells(eye, aim) {
        if body.contains(&c) {
            return last;
        }
        if !open(w, c) {
            return None;
        }
        last = Some(c);
    }
    last
}

/// Whether any of `eyes` has a clear line to the eye or the middle of a body standing at `pos`
/// (where it stands, not the middle of its cell).
pub fn seen_body(w: &World, eyes: &[[f64; 3]], pos: [f64; 3]) -> bool {
    let body = occupied(pos);
    eyes.iter().any(|eye| {
        aims(pos)
            .iter()
            .any(|aim| entry(w, *eye, *aim, &body).is_some())
    })
}

/// A full block to click to put a block into `c`, and the face of it clicked (faces as the
/// protocol numbers them: 0 down, 1 up, 2 north, 3 south, 4 west, 5 east): the floor under `c`
/// first, then the block over it or beside it.
fn click_for(w: &World, (x, y, z): Cell) -> Option<(Cell, i8)> {
    [
        ((x, y - 1, z), 1),
        ((x, y + 1, z), 0),
        ((x + 1, y, z), 4),
        ((x - 1, y, z), 5),
        ((x, y, z + 1), 2),
        ((x, y, z - 1), 3),
    ]
    .into_iter()
    .find(|(c, _)| full(w, *c))
}

/// The block to place now so that `c` can be filled: `c` itself when a full block touches it,
/// else what it needs under or beside it first (a roof cell over the head has nothing to click
/// until a wall stands three high beside it), `depth` steps down the chain.
fn support(w: &World, c: Cell, body: &[Cell], depth: u32) -> Option<(Cell, Cell, i8)> {
    if body.contains(&c) || !open(w, c) {
        return None;
    }
    if let Some((click, face)) = click_for(w, c) {
        return Some((c, click, face));
    }
    if depth == 0 {
        return None;
    }
    let (x, y, z) = c;
    [
        (x, y - 1, z),
        (x + 1, y, z),
        (x - 1, y, z),
        (x, y, z + 1),
        (x, y, z - 1),
    ]
    .into_iter()
    .find_map(|n| support(w, n, body, depth - 1))
}

/// Steps down a support chain: a roof needs a wall cell three high beside the head.
pub const SUPPORT_DEPTH: u32 = 3;

/// The next block of a shelter round a body standing at `pos`: the cell to fill, the full block
/// to click and its face. The cell is where the first line of sight from `eyes` comes in (or what
/// that cell needs under it first); `None` when no eye sees the body, or nothing can be clicked.
/// Every cell read is one a line of sight passes or one beside the body: no hidden block.
pub fn shelter_block(w: &World, eyes: &[[f64; 3]], pos: [f64; 3]) -> Option<(Cell, Cell, i8)> {
    let body = occupied(pos);
    eyes.iter()
        .flat_map(|eye| aims(pos).into_iter().map(move |aim| (*eye, aim)))
        .filter_map(|(eye, aim)| entry(w, eye, aim, &body))
        .find_map(|c| support(w, c, &body, SUPPORT_DEPTH))
}
