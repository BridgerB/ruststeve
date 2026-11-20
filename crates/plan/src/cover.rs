//! Cover from shooters (issue 16, clojurecraft's `blaze.clj` `cover` and `unseen-from?`): a hurt
//! bot gets out of every blaze's line of sight and rests there until its health is back. Burning
//! cost clojurecraft 45% of the health its blaze fights cost, and nine of its twenty deaths came
//! fighting or backing off in the open (`brain/oracle/architecture/cc-blaze-hunt.md`).

use data::EntityKind;
use world::World;

use crate::path::{Cell, Cells, feet_cell};

/// Blocks within which a shooter's line of sight counts (clojurecraft's `cover-range`: they shoot
/// that far).
pub const COVER_RANGE: f64 = 16.0;
/// Cells each way a cover cell is looked for in.
pub const COVER_RADIUS: i32 = 6;

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

/// The eyes of the shooters of `kinds` within `COVER_RANGE` of the feet.
pub fn shooters(w: &World, kinds: &[EntityKind]) -> Vec<[f64; 3]> {
    let Some(body) = w.body else {
        return Vec::new();
    };
    w.entities
        .values()
        .filter(|e| !e.dead && kinds.contains(&e.kind))
        .filter(|e| distance(e.pos, body.pos) <= COVER_RANGE)
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
