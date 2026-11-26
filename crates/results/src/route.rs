//! Issue 19 (agent Q9, docs/problems/eyes-route.md): a survey of the way home over a saved
//! Nether's region files, on a runner (never on the Mac, never in the bot: no X-ray). The world is
//! read once into a grid of block classes; each candidate route is the cheapest chain of feet
//! cells from a start to a goal under its own weights (a step, a dig, a block bridged, the open
//! lava within 2, the open drops beside the feet, a step under y 40), kept inside a corridor round
//! a polyline when one is given. Its report counts what the bot would meet on it: the cells of
//! open lava within 2 of the feet, the blocks of rock cut, the bridges, the open drops, the
//! biomes, and how near it comes to a fortress and a spawner. Development knowledge for choosing
//! `[home] way`; the moves are the travel tunnel's own (a level step, a stair up, a stair down).

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::path::Path;

const AIR: u8 = 0;
/// Not solid, no floor: fungus, roots, vines, fire.
const PLANT: u8 = 1;
const LAVA: u8 = 2;
/// Solid and diggable: netherrack, basalt, blackstone, soul sand and soil, nylium, ores.
const ROCK: u8 = 3;
/// Diggable, a floor that burns: magma.
const HOT: u8 = 4;
/// Gravel and sand: a floor, falls when its floor is cut.
const FALLS: u8 = 5;
/// A fortress block or a spawner: never cut (`plan::tunnel::FORTRESS_BLOCKS` on the way home),
/// may be stood on.
const FORT: u8 = 6;
/// Never cut: bedrock, obsidian, a portal, or a chunk not generated.
const WALL: u8 = 7;

fn class(b: &str) -> u8 {
    let n = b.strip_prefix("minecraft:").unwrap_or(b);
    if n.contains("nether_brick") {
        return FORT;
    }
    match n {
        "air" | "cave_air" | "void_air" => AIR,
        "lava" => LAVA,
        "magma_block" => HOT,
        "gravel" | "sand" | "red_sand" => FALLS,
        "bedrock" | "obsidian" | "crying_obsidian" | "nether_portal" | "?" => WALL,
        "spawner" | "chest" => FORT,
        "crimson_fungus"
        | "warped_fungus"
        | "crimson_roots"
        | "warped_roots"
        | "nether_sprouts"
        | "weeping_vines"
        | "weeping_vines_plant"
        | "twisting_vines"
        | "twisting_vines_plant"
        | "fire"
        | "soul_fire"
        | "brown_mushroom"
        | "red_mushroom" => PLANT,
        _ => ROCK,
    }
}

fn open(c: u8) -> bool {
    c == AIR || c == PLANT
}

fn diggable(c: u8) -> bool {
    c == ROCK || c == HOT || c == FALLS
}

/// The Nether read into classes inside a box.
pub struct Grid {
    lo: [i32; 3],
    dims: [i32; 3],
    cells: Vec<u8>,
    /// The biome of each column at `BIOME_Y` (an index into `biomes`).
    col_biome: Vec<u16>,
    biomes: Vec<String>,
    /// Columns holding a fortress block or a spawner.
    fort_cols: BTreeSet<(i32, i32)>,
    /// Exposed lava (a lava cell with an open face) and its count in the window round a feet
    /// cell: 2 each way on the plane, from 4 under the feet to the head.
    exposed: Vec<u8>,
    hazard: Vec<u16>,
}

/// The height a column's biome is read at: the band the routes run in.
const BIOME_Y: i32 = 50;
/// The window of open lava counted round the feet (the task's "within 2").
const NEAR: i32 = 2;
const UNDER: i32 = 4;

impl Grid {
    fn idx(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        let (dx, dy, dz) = (x - self.lo[0], y - self.lo[1], z - self.lo[2]);
        if dx < 0
            || dy < 0
            || dz < 0
            || dx >= self.dims[0]
            || dy >= self.dims[1]
            || dz >= self.dims[2]
        {
            return None;
        }
        Some(((dy * self.dims[2] + dz) * self.dims[0] + dx) as usize)
    }

    fn at(&self, x: i32, y: i32, z: i32) -> u8 {
        self.idx(x, y, z).map_or(WALL, |i| self.cells[i])
    }

    fn pos(&self, i: usize) -> [i32; 3] {
        let i = i as i32;
        let x = i % self.dims[0];
        let z = (i / self.dims[0]) % self.dims[2];
        let y = i / (self.dims[0] * self.dims[2]);
        [x + self.lo[0], y + self.lo[1], z + self.lo[2]]
    }

    fn biome(&self, x: i32, z: i32) -> &str {
        let (dx, dz) = (x - self.lo[0], z - self.lo[2]);
        if dx < 0 || dz < 0 || dx >= self.dims[0] || dz >= self.dims[2] {
            return "";
        }
        let b = self.col_biome[(dz * self.dims[0] + dx) as usize];
        self.biomes
            .get(usize::from(b))
            .map_or("", |s| s.strip_prefix("minecraft:").unwrap_or(s))
    }

    /// Read every region file under `dir` into the box `x0..=x1, y0..=y1, z0..=z1`.
    pub fn load(dir: &Path, lo: [i32; 3], hi: [i32; 3]) -> Result<Self, String> {
        let dims = [hi[0] - lo[0] + 1, hi[1] - lo[1] + 1, hi[2] - lo[2] + 1];
        let n = (dims[0] * dims[1] * dims[2]) as usize;
        let mut g = Grid {
            lo,
            dims,
            cells: vec![WALL; n],
            col_biome: vec![u16::MAX; (dims[0] * dims[2]) as usize],
            biomes: Vec::new(),
            fort_cols: BTreeSet::new(),
            exposed: Vec::new(),
            hazard: Vec::new(),
        };
        let mut names: BTreeMap<String, u16> = BTreeMap::new();
        for r in crate::region::regions(dir) {
            for c in crate::region::read_region(&r)? {
                if c.cx * 16 + 15 < lo[0]
                    || c.cx * 16 > hi[0]
                    || c.cz * 16 + 15 < lo[2]
                    || c.cz * 16 > hi[2]
                {
                    continue;
                }
                for (p, b, biome) in c.cells() {
                    let Some(i) = g.idx(p[0], p[1], p[2]) else {
                        continue;
                    };
                    let k = class(b);
                    g.cells[i] = k;
                    if k == FORT {
                        g.fort_cols.insert((p[0], p[2]));
                    }
                    if p[1] == BIOME_Y {
                        let next = names.len() as u16;
                        let id = *names.entry(biome.to_owned()).or_insert(next);
                        g.col_biome[((p[2] - lo[2]) * dims[0] + (p[0] - lo[0])) as usize] = id;
                    }
                }
            }
        }
        let mut biomes = vec![String::new(); names.len()];
        for (name, id) in names {
            biomes[usize::from(id)] = name;
        }
        g.biomes = biomes;
        g.exposed = (0..n)
            .map(|i| {
                if g.cells[i] != LAVA {
                    return 0;
                }
                let [x, y, z] = g.pos(i);
                let faces = [
                    (1, 0, 0),
                    (-1, 0, 0),
                    (0, 1, 0),
                    (0, -1, 0),
                    (0, 0, 1),
                    (0, 0, -1),
                ];
                u8::from(
                    faces
                        .iter()
                        .any(|(a, b, c)| open(g.at(x + a, y + b, z + c))),
                )
            })
            .collect();
        g.hazard = g.window_sum();
        Ok(g)
    }

    /// The count of exposed lava in the window round each feet cell, by three separable sums.
    fn window_sum(&self) -> Vec<u16> {
        let [nx, ny, nz] = self.dims;
        let at = |x: i32, y: i32, z: i32| ((y * nz + z) * nx + x) as usize;
        let mut a = vec![0u16; self.exposed.len()];
        for y in 0..ny {
            for z in 0..nz {
                for x in 0..nx {
                    a[at(x, y, z)] = (x - NEAR..=x + NEAR)
                        .filter(|&q| q >= 0 && q < nx)
                        .map(|q| u16::from(self.exposed[at(q, y, z)]))
                        .sum();
                }
            }
        }
        let mut b = vec![0u16; a.len()];
        for y in 0..ny {
            for z in 0..nz {
                for x in 0..nx {
                    b[at(x, y, z)] = (z - NEAR..=z + NEAR)
                        .filter(|&q| q >= 0 && q < nz)
                        .map(|q| a[at(x, y, q)])
                        .sum();
                }
            }
        }
        let mut c = vec![0u16; b.len()];
        for y in 0..ny {
            for z in 0..nz {
                for x in 0..nx {
                    c[at(x, y, z)] = (y - UNDER..=y + 1)
                        .filter(|&q| q >= 0 && q < ny)
                        .map(|q| b[at(x, q, z)])
                        .sum();
                }
            }
        }
        c
    }

    fn drops(&self, x: i32, y: i32, z: i32) -> u32 {
        [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .iter()
            .filter(|(a, b)| {
                open(self.at(x + a, y, z + b))
                    && open(self.at(x + a, y - 1, z + b))
                    && matches!(self.at(x + a, y - 2, z + b), AIR | PLANT | LAVA)
            })
            .count() as u32
    }
}

/// A candidate route's weights, tenths of a plain step each.
#[derive(Clone, Copy, Debug)]
pub struct Weights {
    pub dig: u32,
    pub bridge: u32,
    pub lava: u32,
    pub drop: u32,
    pub low: u32,
}

/// One candidate: a name, the feet's height band, its weights, and a corridor (a polyline and a
/// half width) the route stays in, when given.
pub struct Cand {
    pub name: String,
    pub min_y: i32,
    pub max_y: i32,
    pub w: Weights,
    pub radius: f64,
    pub line: Vec<[i32; 2]>,
}

/// `name;miny;maxy;dig,bridge,lava,drop,low;radius;x,z x,z ...` (the last two may be empty).
pub fn parse_cand(s: &str) -> Result<Cand, String> {
    let parts: Vec<&str> = s.split(';').collect();
    let num =
        |t: &str| -> Result<i32, String> { t.trim().parse().map_err(|e| format!("{s}: {e}")) };
    let [name, miny, maxy, ws, rest @ ..] = &parts[..] else {
        return Err(format!(
            "{s}: a candidate is name;miny;maxy;weights[;radius;line]"
        ));
    };
    let w: Vec<u32> = ws
        .split(',')
        .map(|t| t.trim().parse().map_err(|e| format!("{s}: {e}")))
        .collect::<Result<_, String>>()?;
    let [dig, bridge, lava, drop, low] = w[..] else {
        return Err(format!("{s}: weights are dig,bridge,lava,drop,low"));
    };
    let radius = match rest.first() {
        Some(r) if !r.trim().is_empty() => r.trim().parse().map_err(|e| format!("{s}: {e}"))?,
        _ => 0.0,
    };
    let line = match rest.get(1) {
        Some(l) => l
            .split_whitespace()
            .map(|p| {
                let (a, b) = p.split_once(',').ok_or(format!("{p}: a point is x,z"))?;
                Ok([num(a)?, num(b)?])
            })
            .collect::<Result<_, String>>()?,
        None => Vec::new(),
    };
    Ok(Cand {
        name: (*name).to_owned(),
        min_y: num(miny)?,
        max_y: num(maxy)?,
        w: Weights {
            dig,
            bridge,
            lava,
            drop,
            low,
        },
        radius,
        line,
    })
}

fn seg_dist(p: [f64; 2], a: [i32; 2], b: [i32; 2]) -> f64 {
    let (ax, az, bx, bz) = (
        f64::from(a[0]),
        f64::from(a[1]),
        f64::from(b[0]),
        f64::from(b[1]),
    );
    let (dx, dz) = (bx - ax, bz - az);
    let len2 = dx * dx + dz * dz;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p[0] - ax) * dx + (p[1] - az) * dz) / len2).clamp(0.0, 1.0)
    };
    (p[0] - ax - t * dx).hypot(p[1] - az - t * dz)
}

fn line_dist(p: [f64; 2], line: &[[i32; 2]]) -> f64 {
    match line {
        [] => 0.0,
        [a] => seg_dist(p, *a, *a),
        _ => line
            .windows(2)
            .map(|s| seg_dist(p, s[0], s[1]))
            .fold(f64::INFINITY, f64::min),
    }
}

/// The survey: every candidate's route from `from` to within 2 of `to`, columns within `avoid`'s
/// radius of its point refused. Returns the report as text.
pub fn survey(
    g: &Grid,
    from: [i32; 3],
    to: [i32; 3],
    avoid: &[([i32; 2], f64)],
    cands: &[Cand],
    map: bool,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "grid {:?} to {:?}, {} fortress columns, biomes {:?}\n",
        g.lo,
        [
            g.lo[0] + g.dims[0] - 1,
            g.lo[1] + g.dims[1] - 1,
            g.lo[2] + g.dims[2] - 1
        ],
        g.fort_cols.len(),
        g.biomes
    ));
    let mut routes = Vec::new();
    for c in cands {
        let (text, path) = one(g, from, to, avoid, c);
        out.push_str(&text);
        routes.push((&c.name, path, &c.line));
    }
    if map {
        for (name, path, line) in &routes {
            out.push_str(&format!(
                "\n## map: {name} (* the route, o its corridor's points)\n"
            ));
            out.push_str(&draw(g, path, line, from, to, avoid));
        }
    }
    out
}

fn one(
    g: &Grid,
    from: [i32; 3],
    to: [i32; 3],
    avoid: &[([i32; 2], f64)],
    c: &Cand,
) -> (String, Vec<[i32; 3]>) {
    let [nx, _, nz] = g.dims;
    // The columns a candidate may stand in.
    let allowed: Vec<bool> = (0..nz)
        .flat_map(|dz| (0..nx).map(move |dx| (dx, dz)))
        .map(|(dx, dz)| {
            let p = [f64::from(dx + g.lo[0]) + 0.5, f64::from(dz + g.lo[2]) + 0.5];
            let off = avoid
                .iter()
                .any(|(a, r)| (p[0] - f64::from(a[0])).hypot(p[1] - f64::from(a[1])) < *r);
            !off && (c.line.is_empty() || line_dist(p, &c.line) <= c.radius)
        })
        .collect();
    let col_ok = |x: i32, z: i32| {
        let (dx, dz) = (x - g.lo[0], z - g.lo[2]);
        dx >= 0 && dz >= 0 && dx < nx && dz < nz && allowed[(dz * nx + dx) as usize]
    };
    let lava_face = |x: i32, y: i32, z: i32| {
        [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, 0, 1), (0, 0, -1)]
            .iter()
            .any(|(a, b, d)| g.at(x + a, y + b, z + d) == LAVA)
    };
    // The start: the nearest standing cell to `from`.
    let stands = |x: i32, y: i32, z: i32| {
        open(g.at(x, y, z)) && open(g.at(x, y + 1, z)) && !open(g.at(x, y - 1, z))
    };
    let start = (-3..=3)
        .flat_map(|dy| (-3..=3).flat_map(move |dx| (-3..=3).map(move |dz| (dx, dy, dz))))
        .filter(|&(dx, dy, dz)| stands(from[0] + dx, from[1] + dy, from[2] + dz))
        .min_by_key(|&(dx, dy, dz)| dx * dx + dy * dy + dz * dz)
        .map(|(dx, dy, dz)| [from[0] + dx, from[1] + dy, from[2] + dz])
        .unwrap_or(from);
    let Some(si) = g.idx(start[0], start[1], start[2]) else {
        return (
            format!("\n# {}: the start {start:?} is outside the grid\n", c.name),
            Vec::new(),
        );
    };
    let n = g.cells.len();
    let mut dist = vec![u32::MAX; n];
    let mut prev = vec![u32::MAX; n];
    let mut heap = BinaryHeap::new();
    dist[si] = 0;
    heap.push(Reverse((0u32, si)));
    let goal = |p: [i32; 3]| {
        f64::from(p[0] - to[0]).hypot(f64::from(p[2] - to[2])) <= 2.0 && (p[1] - to[1]).abs() <= 2
    };
    let mut found = None;
    let mut best: Option<(f64, usize)> = None;
    let mut expanded = 0u64;
    while let Some(Reverse((d, i))) = heap.pop() {
        if d > dist[i] {
            continue;
        }
        expanded += 1;
        let [x, y, z] = g.pos(i);
        if goal([x, y, z]) {
            found = Some(i);
            break;
        }
        let left = f64::from(x - to[0]).hypot(f64::from(z - to[2]));
        if best.is_none_or(|(b, _)| left < b) {
            best = Some((left, i));
        }
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (tx, tz) = (x + dx, z + dz);
            if !col_ok(tx, tz) {
                continue;
            }
            for dy in [0, 1, -1] {
                let ty = y + dy;
                if ty < c.min_y || ty > c.max_y {
                    continue;
                }
                let clear: [(i32, i32, i32); 3] = match dy {
                    1 => [(x, y + 2, z), (tx, ty, tz), (tx, ty + 1, tz)],
                    -1 => [(tx, ty, tz), (tx, ty + 1, tz), (tx, ty + 2, tz)],
                    _ => [(tx, ty, tz), (tx, ty + 1, tz), (tx, ty + 1, tz)],
                };
                let mut digs = 0u32;
                let mut ok = true;
                let mut seen: [(i32, i32, i32); 3] = [(i32::MIN, 0, 0); 3];
                for (k, &(cx, cy, cz)) in clear.iter().enumerate() {
                    if seen[..k].contains(&(cx, cy, cz)) {
                        continue;
                    }
                    seen[k] = (cx, cy, cz);
                    let cl = g.at(cx, cy, cz);
                    if open(cl) {
                        continue;
                    }
                    if !diggable(cl) || lava_face(cx, cy, cz) {
                        ok = false;
                        break;
                    }
                    digs += 1;
                }
                if !ok {
                    continue;
                }
                let floor = g.at(tx, ty - 1, tz);
                let bridge = u32::from(open(floor) || floor == LAVA);
                let Some(ti) = g.idx(tx, ty, tz) else {
                    continue;
                };
                let cost = 10
                    + c.w.dig * digs
                    + c.w.bridge * bridge
                    + c.w.lava * u32::from(g.hazard[ti])
                    + c.w.drop * g.drops(tx, ty, tz)
                    + if floor == HOT { 20 } else { 0 }
                    + if ty < 40 { c.w.low } else { 0 };
                let nd = d + cost;
                if nd < dist[ti] {
                    dist[ti] = nd;
                    prev[ti] = i as u32;
                    heap.push(Reverse((nd, ti)));
                }
            }
        }
    }
    let (end, reached) = match (found, best) {
        (Some(e), _) => (e, true),
        (None, Some((_, e))) => (e, false),
        (None, None) => (si, false),
    };
    let mut path = vec![g.pos(end)];
    let mut at = end;
    while prev[at] != u32::MAX {
        at = prev[at] as usize;
        path.push(g.pos(at));
    }
    path.reverse();
    (
        report(g, c, &path, reached, expanded, dist[end], avoid),
        path,
    )
}

fn report(
    g: &Grid,
    c: &Cand,
    path: &[[i32; 3]],
    reached: bool,
    expanded: u64,
    cost: u32,
    avoid: &[([i32; 2], f64)],
) -> String {
    let mut digs = 0;
    let mut bridges = 0;
    let mut bridges_lava = 0;
    let mut lava_steps = 0;
    let mut drop_steps = 0;
    let mut low_steps = 0;
    let mut lava_cells: BTreeSet<usize> = BTreeSet::new();
    let mut by_biome: BTreeMap<String, u32> = BTreeMap::new();
    let mut rows = String::new();
    for (k, p) in path.iter().enumerate() {
        let [x, y, z] = *p;
        let dug = |q: (i32, i32, i32)| u32::from(diggable(g.at(q.0, q.1, q.2)));
        if let Some(prev) = k.checked_sub(1).and_then(|j| path.get(j)) {
            let d = match y - prev[1] {
                1 => dug((prev[0], prev[1] + 2, prev[2])) + dug((x, y, z)) + dug((x, y + 1, z)),
                -1 => dug((x, y, z)) + dug((x, y + 1, z)) + dug((x, y + 2, z)),
                _ => dug((x, y, z)) + dug((x, y + 1, z)),
            };
            digs += d;
        }
        let floor = g.at(x, y - 1, z);
        if open(floor) || floor == LAVA {
            bridges += 1;
            if floor == LAVA || g.at(x, y - 2, z) == LAVA {
                bridges_lava += 1;
            }
        }
        let h = g.idx(x, y, z).map_or(0, |i| g.hazard[i]);
        if h > 0 {
            lava_steps += 1;
            for wy in y - UNDER..=y + 1 {
                for wz in z - NEAR..=z + NEAR {
                    for wx in x - NEAR..=x + NEAR {
                        if let Some(i) = g.idx(wx, wy, wz)
                            && g.exposed[i] == 1
                        {
                            lava_cells.insert(i);
                        }
                    }
                }
            }
        }
        let dr = g.drops(x, y, z);
        if dr > 0 {
            drop_steps += 1;
        }
        if y < 40 {
            low_steps += 1;
        }
        *by_biome.entry(g.biome(x, z).to_owned()).or_default() += 1;
        if k % 8 == 0 || k + 1 == path.len() {
            rows.push_str(&format!(
                "  {k:>5} ({x}, {y}, {z}) {} lava{h} drops{dr} digs{digs}\n",
                g.biome(x, z)
            ));
        }
    }
    // The nearest fortress column and avoided point on the plane, every fourth step.
    let fort: Vec<(i32, i32)> = g.fort_cols.iter().copied().collect();
    let near_fort = path
        .iter()
        .step_by(4)
        .map(|p| {
            fort.iter()
                .map(|f| f64::from(f.0 - p[0]).hypot(f64::from(f.1 - p[2])))
                .fold(f64::INFINITY, f64::min)
        })
        .fold(f64::INFINITY, f64::min);
    let near_avoid = path
        .iter()
        .map(|p| {
            avoid
                .iter()
                .map(|(a, _)| f64::from(a[0] - p[0]).hypot(f64::from(a[1] - p[2])))
                .fold(f64::INFINITY, f64::min)
        })
        .fold(f64::INFINITY, f64::min);
    // Way points: one every 24 steps on the plane, and the end.
    let mut way: Vec<[i32; 2]> = path.iter().step_by(24).map(|p| [p[0], p[2]]).collect();
    if let Some(l) = path.last() {
        way.push([l[0], l[2]]);
    }
    let ys: Vec<i32> = path.iter().map(|p| p[1]).collect();
    let mut s = format!(
        "\n# {}: {} (y {} to {}, weights {:?}, corridor {} round {} points)\n",
        c.name,
        if reached {
            "reached"
        } else {
            "NOT reached (nearest kept)"
        },
        c.min_y,
        c.max_y,
        c.w,
        c.radius,
        c.line.len()
    );
    s.push_str(&format!(
        "steps {} cost {cost} expanded {expanded}; feet y {} to {}; blocks cut {digs} (about {:.0} pickaxe uses at 2.4 a block); bridges {bridges} ({bridges_lava} over lava); open lava cells within 2: {} ({lava_steps} steps beside them); steps with an open drop beside: {drop_steps}; steps under y 40: {low_steps}; nearest fortress column {near_fort:.0}; nearest avoided point {near_avoid:.0}\n",
        path.len(),
        ys.iter().min().copied().unwrap_or(0),
        ys.iter().max().copied().unwrap_or(0),
        f64::from(digs) * 2.4,
        lava_cells.len()
    ));
    s.push_str(&format!("biomes (steps): {by_biome:?}\n"));
    s.push_str("way = [");
    s.push_str(
        &way.iter()
            .map(|w| format!("[{}, {}]", w[0], w[1]))
            .collect::<Vec<_>>()
            .join(", "),
    );
    s.push_str("]\n");
    s.push_str(&rows);
    s
}

/// A plan map, a character every 4 by 4 columns: F a fortress, ~ open lava at y 30 to 45, the
/// biome's letter (b deltas, s soul sand valley, w warped, c crimson, . wastes), * the route, o
/// the corridor's points, S start, P goal, X an avoided point.
fn draw(
    g: &Grid,
    path: &[[i32; 3]],
    line: &[[i32; 2]],
    from: [i32; 3],
    to: [i32; 3],
    avoid: &[([i32; 2], f64)],
) -> String {
    const S: i32 = 4;
    let (w, h) = ((g.dims[0] + S - 1) / S, (g.dims[2] + S - 1) / S);
    let mut grid = vec![vec![' '; w as usize]; h as usize];
    for (r, row) in grid.iter_mut().enumerate() {
        for (q, ch) in row.iter_mut().enumerate() {
            let (x0, z0) = (g.lo[0] + q as i32 * S, g.lo[2] + r as i32 * S);
            let cols = (0..S).flat_map(|a| (0..S).map(move |b| (x0 + a, z0 + b)));
            let mut fort = false;
            let mut sea = false;
            let mut made = false;
            for (x, z) in cols {
                fort |= g.fort_cols.contains(&(x, z));
                sea |= (30..=45).any(|y| g.at(x, y, z) == LAVA && open(g.at(x, y + 1, z)));
                made |= g.at(x, BIOME_Y, z) != WALL || !g.biome(x, z).is_empty();
            }
            let biome = g.biome(x0 + 2, z0 + 2);
            *ch = if fort {
                'F'
            } else if sea {
                '~'
            } else if !made || biome.is_empty() {
                ' '
            } else if biome.contains("delta") {
                'b'
            } else if biome.contains("soul") {
                's'
            } else if biome.contains("warped") {
                'w'
            } else if biome.contains("crimson") {
                'c'
            } else {
                '.'
            };
        }
    }
    let mut put = |x: i32, z: i32, ch: char| {
        let (q, r) = ((x - g.lo[0]).div_euclid(S), (z - g.lo[2]).div_euclid(S));
        if q >= 0 && r >= 0 && q < w && r < h {
            grid[r as usize][q as usize] = ch;
        }
    };
    for p in path {
        put(p[0], p[2], '*');
    }
    for p in line {
        put(p[0], p[1], 'o');
    }
    for (a, _) in avoid {
        put(a[0], a[1], 'X');
    }
    put(from[0], from[2], 'S');
    put(to[0], to[2], 'P');
    let mut s = format!(
        "x from {} by {S} a column; z from {} by {S} a row\n",
        g.lo[0], g.lo[2]
    );
    for (r, row) in grid.iter().enumerate() {
        s.push_str(&format!("{:>5} ", g.lo[2] + r as i32 * S));
        s.extend(row.iter());
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_candidate_reads_its_weights_and_corridor() {
        let c = parse_cand("deltas;32;80;24,40,30,20,30;6;477,152 399,194").unwrap();
        assert_eq!((c.min_y, c.max_y, c.w.lava, c.radius), (32, 80, 30, 6.0));
        assert_eq!(c.line, vec![[477, 152], [399, 194]]);
        assert!(parse_cand("open;32;80;1,1,1,1,1").unwrap().line.is_empty());
        assert!(parse_cand("bad;32;80;1,1").is_err());
    }

    #[test]
    fn the_corridor_is_the_nearest_segment() {
        let line = [[0, 0], [10, 0], [10, 10]];
        assert!((line_dist([5.0, 3.0], &line) - 3.0).abs() < 1e-9);
        assert!((line_dist([13.0, 5.0], &line) - 3.0).abs() < 1e-9);
    }
}
