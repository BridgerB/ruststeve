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

use crate::path::{Cell, Cells, Class, feet_cell};

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

/// Blocks from a spawner beyond which a hurt bot rests, shelters and heals. A spawner runs only
/// with a player within 16 blocks of it (vanilla's `requiredPlayerRange`): bots resting 14 to 17
/// from the blaze spawner saw the shooters in range go 3, 5, 7, 8, 12 in one rest (races
/// 45337168104 and 45338756013). Two more than 16, for where in its cell the body stands.
pub const SPAWNER_QUIET: f64 = 18.0;

/// The spawners on record within `SPAWNER_QUIET` of `pos` (two chunk columns round the feet
/// hold every one that near).
pub fn spawners_awake(w: &World, pos: [f64; 3]) -> Vec<Cell> {
    world::stage::kind_near(w, data::BlockKind::Spawner, 2)
        .into_iter()
        .filter(|c| awake_at(*c, pos))
        .collect()
}

/// Whether a spawner in `c` is within `SPAWNER_QUIET` of a body at `pos`.
pub fn awake_at(c: Cell, pos: [f64; 3]) -> bool {
    let mid = [
        f64::from(c.0) + 0.5,
        f64::from(c.1) + 0.5,
        f64::from(c.2) + 0.5,
    ];
    distance(mid, pos) < SPAWNER_QUIET
}

/// Why standing work (a craft, a smelt, a table or a furnace placed) is not safe where the body
/// is, or `None` when it is: a spawner on record within `SPAWNER_QUIET`, or a shooter (`shooters`)
/// that sees the body. One bot made its shield on the spawner's deck in the whole-stage model,
/// a table and a furnace on the deck and 200 ticks of smelting beside the spawner with blazes
/// firing, and died at tick 1,140 (probe 45357767095).
pub fn unsafe_to_work(w: &World) -> Option<String> {
    let body = w.body?;
    if let Some(sp) = spawners_awake(w, body.pos).first() {
        return Some(format!("a spawner at {sp:?} within {SPAWNER_QUIET}"));
    }
    let eyes = shooters(w);
    seen_body(w, &eyes, body.pos).then(|| format!("seen by a shooter ({} in range)", eyes.len()))
}

/// A fall this deep or deeper beside a cell keeps it from being cover: three blocks of fall cost
/// no health, as the fight's own step rule has it.
pub const DROP_BESIDE: i32 = 4;

/// Whether an open cell beside `c` has no floor within `DROP_BESIDE` under it: the edge of a
/// bridge, or the top of its low wall. Two real bots heading for a cover cell at (128, 61, 183 to
/// 184) went over the bridge's side and fell 17 blocks, one to its death (race 45346418004).
pub fn beside_drop(cells: &mut Cells, (x, y, z): Cell) -> bool {
    [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dz)| {
        let n = (x + dx, y, z + dz);
        matches!(cells.at(n), Class::Clear | Class::Lava)
            && (1..=DROP_BESIDE).all(|d| cells.at((n.0, n.1 - d, n.2)) != Class::Solid)
    })
}

/// The cells to rest in: standable on a full block, within `COVER_RADIUS`, where none of `eyes`
/// sees a body standing (`seen_body`, the same test the rest itself is judged by) and no spawner
/// on record is within `SPAWNER_QUIET`, and no drop beside (`beside_drop`); nearest the feet
/// first. The floor must be a full block
/// (on a stair the body stands higher than its cell says). One bot walked eleven times to a cell
/// on a bridge's low wall that it was still seen in (race 45338756013): the planner also refuses
/// a cover cell once it has walked to it.
pub fn cover_cells(w: &World, eyes: &[[f64; 3]]) -> Vec<Cell> {
    let Some(body) = w.body else {
        return Vec::new();
    };
    let f = feet_cell(body.pos);
    let mut cells = Cells::new(w);
    // Every spawner that could be within the quiet range of a cell in the search.
    let spawners = world::stage::kind_near(w, data::BlockKind::Spawner, 3);
    let mut out: Vec<(i32, Cell)> = Vec::new();
    for dx in -COVER_RADIUS..=COVER_RADIUS {
        for dy in -2..=2 {
            for dz in -COVER_RADIUS..=COVER_RADIUS {
                let c = (f.0 + dx, f.1 + dy, f.2 + dz);
                let pos = [f64::from(c.0) + 0.5, f64::from(c.1), f64::from(c.2) + 0.5];
                if cells.standable(c)
                    && full(w, (c.0, c.1 - 1, c.2))
                    && !beside_drop(&mut cells, c)
                    && !seen_body(w, eyes, pos)
                    && !spawners.iter().any(|s| awake_at(*s, pos))
                {
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

/// Any block with a collision shape: one a block can be placed against, and a floor (soul sand,
/// 14/16 high, is both; issue 17's nook in a soul sand valley had nothing to click on).
fn solid(w: &World, c: Cell) -> bool {
    w.block_at(world::BlockPos::new(c.0, c.1, c.2))
        .and_then(data::generated::shapes::collision)
        .is_some_and(|k| k != data::types::Collision::Empty)
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
    // Never a furnace or table: a right-click opens it and places nothing (overworld-leg wall
    // 11: a hut block clicked onto the cook's furnace opened it, and the hut waited for ever).
    .find(|(c, _)| {
        solid(w, *c)
            && !w
                .block_at(world::BlockPos::new(c.0, c.1, c.2))
                .is_some_and(|k| crate::place::OPENS.contains(&k))
    })
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

/// The next block of a hut round a body standing at `pos` (overworld-leg wall 11: the cook walled
/// in on the surface): the cells beside the body at the feet and the head, then the roof over the
/// head, each open one filled; the cell to fill, the full block to click and its face. `None` when
/// the hut is closed or nothing can be clicked. Only cells beside the body are read.
pub fn hut_block(w: &World, pos: [f64; 3]) -> Option<(Cell, Cell, i8)> {
    pocket_block(w, pos, None)
}

/// `hut_block` with the walls nearest `threat` first (issue 17: a provoked enderman walks in
/// while the pocket goes up, and the side it comes from is shut before the far side); the roof
/// is always last.
pub fn pocket_block(
    w: &World,
    pos: [f64; 3],
    threat: Option<[f64; 3]>,
) -> Option<(Cell, Cell, i8)> {
    shell_block(w, pos, threat, None)
}

/// A nook (issue 17, the soul sand valley wait): the pocket with the side `open` (a step on the
/// plane, as `open_side` gives it) left open at the feet and the head, so the bot looks out over
/// open ground with a roof and three walls between it and the arrows and fireballs from the rest.
pub fn nook_block(w: &World, pos: [f64; 3], open: (i32, i32)) -> Option<(Cell, Cell, i8)> {
    shell_block(w, pos, None, Some(open))
}

/// Cells along each side of the feet counted for `open_side`.
pub const OPEN_RUN: i32 = 8;

/// Each side of `feet` (east, west, south, north) with its run of open standing cells (both
/// cells open, a full floor), up to `OPEN_RUN`. Only cells in a straight line from the feet are
/// read.
pub fn open_runs(w: &World, feet: Cell) -> [((i32, i32), usize); 4] {
    let run = |(dx, dz): (i32, i32)| {
        (1..=OPEN_RUN)
            .take_while(|k| {
                let c = (feet.0 + dx * k, feet.1, feet.2 + dz * k);
                open(w, c) && open(w, (c.0, c.1 + 1, c.2)) && solid(w, (c.0, c.1 - 1, c.2))
            })
            .count()
    };
    [(1, 0), (-1, 0), (0, 1), (0, -1)].map(|d| (d, run(d)))
}

/// The side of `feet` with the longest open run; ties in the order east, west, south, north.
pub fn open_side(w: &World, feet: Cell) -> (i32, i32) {
    open_runs(w, feet)
        .into_iter()
        .rev()
        .max_by_key(|(_, n)| *n)
        .map_or((1, 0), |(d, _)| d)
}

fn shell_block(
    w: &World,
    pos: [f64; 3],
    threat: Option<[f64; 3]>,
    open: Option<(i32, i32)>,
) -> Option<(Cell, Cell, i8)> {
    let body = occupied(pos);
    let top = body.iter().map(|c| c.1).max()?;
    let low = body.iter().map(|c| c.1).min()?;
    let mut shell: Vec<Cell> = Vec::new();
    for y in low..=top {
        for (x, _, z) in body.iter().filter(|c| c.1 == y) {
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if open == Some((dx, dz)) {
                    continue;
                }
                let c = (x + dx, y, z + dz);
                if !body.contains(&c) && !shell.contains(&c) {
                    shell.push(c);
                }
            }
        }
    }
    if let Some(t) = threat {
        let d = |c: &Cell| {
            let (dx, dz) = (f64::from(c.0) + 0.5 - t[0], f64::from(c.2) + 0.5 - t[2]);
            dx * dx + dz * dz
        };
        shell.sort_by(|a, b| d(a).total_cmp(&d(b)).then(a.cmp(b)));
    }
    for (x, _, z) in body.iter().filter(|c| c.1 == top) {
        let c = (*x, top + 1, *z);
        if !shell.contains(&c) {
            shell.push(c);
        }
    }
    shell
        .into_iter()
        .find_map(|c| support(w, c, &body, SUPPORT_DEPTH))
}

/// The next block toward filling `cells` (issue 18e, the burrow: its missing shell, a mended
/// wall, the mouth's seal) with the body at `pos`: the first cell in order that can be filled now,
/// or what it needs under or beside it first (a roof cell over the head waits on a wall three
/// high). The cell to fill, the full block to click and its face; `None` when none can be.
pub fn fill_block(w: &World, cells: &[Cell], pos: [f64; 3]) -> Option<(Cell, Cell, i8)> {
    let body = occupied(pos);
    cells
        .iter()
        .find_map(|c| support(w, *c, &body, SUPPORT_DEPTH))
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
