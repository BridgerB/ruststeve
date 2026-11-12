//! Generated worlds (`sim::terrain`) (brain/ruststeve/design/server-model.md): seeded terrain for running the bot
//! against the model thousands of times. Not vanilla's generator: terrain with what the bot's
//! movement must handle (slopes, one-block steps, carpets, ledges, pillars, still water),
//! encoded as real 26.1.2 chunk columns that pass the same decoder as a server's. A world is a
//! pure function of its seed.

use std::collections::BTreeMap;

use data::BlockKind;
use protocol::EncodeError;
use protocol::chunk::{Column, Paletted, Section};

use crate::{ModelColumn, ModelWorld};

/// The water surface: cells below it that are not ground are water.
const SEA: i32 = 62;
/// The height of the land around the spawn.
const BASE: i32 = 66;

fn state(k: BlockKind) -> u32 {
    k.info().map_or(0, |i| i.default_state)
}

/// A seeded hash of a cell: the same seed and cell always give the same value.
fn hash(seed: u64, x: i32, z: i32, salt: u64) -> u64 {
    let mut h = seed ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    h ^= (x as u32 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h = h.rotate_left(31) ^ (z as u32 as u64).wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^ (h >> 31)
}

/// A value in [0, 1) from the hash.
fn unit(seed: u64, x: i32, z: i32, salt: u64) -> f64 {
    (hash(seed, x, z, salt) >> 11) as f64 / (1u64 << 53) as f64
}

/// Value noise on a lattice of `cell` blocks, smoothly interpolated: rolling ground.
fn noise(seed: u64, x: i32, z: i32, cell: i32, salt: u64) -> f64 {
    let (gx, gz) = (x.div_euclid(cell), z.div_euclid(cell));
    let (fx, fz) = (
        f64::from(x.rem_euclid(cell)) / f64::from(cell),
        f64::from(z.rem_euclid(cell)) / f64::from(cell),
    );
    let s = |t: f64| t * t * (3.0 - 2.0 * t);
    let (sx, sz) = (s(fx), s(fz));
    let v = |dx, dz| unit(seed, gx + dx, gz + dz, salt);
    let top = v(0, 0) + (v(1, 0) - v(0, 0)) * sx;
    let bottom = v(0, 1) + (v(1, 1) - v(0, 1)) * sx;
    top + (bottom - top) * sz
}

/// The ground height at a cell: rolling hills plus finer bumps, so whole-block steps, slopes
/// and the odd ledge all occur.
fn height(seed: u64, x: i32, z: i32) -> i32 {
    let hills = noise(seed, x, z, 24, 1) * 14.0 - 7.0;
    let bumps = noise(seed, x, z, 5, 2) * 3.0 - 1.5;
    BASE + (hills + bumps).round() as i32
}

/// The block at a world cell of a generated world.
fn block_at(seed: u64, x: i32, y: i32, z: i32) -> u32 {
    let h = height(seed, x, z);
    let pillar = unit(seed, x, z, 3) < 0.004 && h >= SEA;
    if y < h {
        return state(if y < h - 3 {
            BlockKind::Stone
        } else {
            BlockKind::Dirt
        });
    }
    if y == h {
        return state(if h < SEA {
            BlockKind::Sand
        } else {
            BlockKind::GrassBlock
        });
    }
    if y <= SEA && h < SEA {
        return state(BlockKind::Water);
    }
    if pillar && y <= h + 3 {
        return state(BlockKind::Cobblestone);
    }
    if y == h + 1 && h >= SEA && unit(seed, x, z, 4) < 0.02 {
        return state(BlockKind::WhiteCarpet);
    }
    state(BlockKind::Air)
}

/// One generated column at (cx, cz).
fn column(seed: u64, cx: i32, cz: i32) -> Column {
    let air = state(BlockKind::Air);
    let water = state(BlockKind::Water);
    let sections = (0..24)
        .map(|si| {
            let base = -64 + 16 * si;
            let cells: Vec<u32> = (0..4096i32)
                .map(|i| {
                    let (lx, lz, ly) = (i & 15, (i >> 4) & 15, i >> 8);
                    block_at(seed, cx * 16 + lx, base + ly, cz * 16 + lz)
                })
                .collect();
            let count = |pred: &dyn Fn(u32) -> bool| {
                i16::try_from(cells.iter().filter(|c| pred(**c)).count()).unwrap_or(i16::MAX)
            };
            Section {
                block_count: count(&|c| c != air),
                fluid_count: count(&|c| c == water),
                blocks: Paletted::pack(&cells, 4),
                biomes: Paletted::Single(0),
            }
        })
        .collect();
    Column {
        min_y: -64,
        sections,
    }
}

/// A generated world of `radius` columns around the origin each way, the player spawning on
/// the ground at the first dry cell found spiralling out from (0, 0).
pub fn world(seed: u64, radius: i32) -> Result<ModelWorld, EncodeError> {
    let mut columns = BTreeMap::new();
    for x in -radius..=radius {
        for z in -radius..=radius {
            columns.insert((x, z), ModelColumn::generated(x, z, column(seed, x, z))?);
        }
    }
    let dry = (0..=radius * 16)
        .flat_map(|r| (-r..=r).flat_map(move |d| [(d, -r), (d, r), (-r, d), (r, d)]))
        .find(|(x, z)| {
            let h = height(seed, *x, *z);
            h >= SEA
                && block_at(seed, *x, h + 1, *z) == state(BlockKind::Air)
                && block_at(seed, *x, h + 2, *z) == state(BlockKind::Air)
        })
        .unwrap_or((0, 0));
    let (x, z) = dry;
    Ok(ModelWorld {
        columns,
        spawn: [
            f64::from(x) + 0.5,
            f64::from(height(seed, x, z) + 1),
            f64::from(z) + 0.5,
        ],
        dimension: "minecraft:overworld".to_owned(),
        dimension_type: 0,
    })
}
