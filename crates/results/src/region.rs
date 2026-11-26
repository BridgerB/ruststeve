//! Issue 19a's measurements, read from a saved world's region files on a runner (never on the
//! Mac, never in the bot): where a block kind lies (the stronghold's `end_portal_frame` ring, a
//! portal's obsidian) and what tops a column (a strip map's surface and biome). Development
//! knowledge only; no bot code reads a world file (no X-ray).
//!
//! The Anvil format: a region file `r.<rx>.<rz>.mca` holds 32 x 32 chunks; its first 4,096 bytes
//! are 1,024 locations (a 3-byte sector offset, a 1-byte sector count). At the offset: a 4-byte
//! length, a compression byte (1 gzip, 2 zlib, 3 none) and the chunk's named-root NBT. A chunk
//! keeps `sections`, each with `Y`, `block_states {palette, data}` and `biomes {palette, data}`;
//! indices pack `max(4, bits)` (blocks) or `bits` (biomes) a value, never across longs.

use std::io::Read;
use std::path::Path;

use protocol::bytes::Reader;
use protocol::nbt::{self, Nbt};

/// One section's blocks and biomes, unpacked.
struct Section {
    y: i32,
    /// 4,096 block names, index `y * 256 + z * 16 + x`.
    blocks: Vec<String>,
    /// 64 biome names, index `(y / 4) * 16 + (z / 4) * 4 + x / 4`.
    biomes: Vec<String>,
}

/// One chunk: its column and sections, lowest first.
pub struct Chunk {
    pub cx: i32,
    pub cz: i32,
    sections: Vec<Section>,
}

/// A named root (a file's NBT) read as the network's anonymous root.
fn named_root(raw: &[u8]) -> Result<Nbt, String> {
    let (&tag, rest) = raw.split_first().ok_or("an empty chunk")?;
    if tag != 10 {
        return Err(format!("the root tag is {tag}, not a compound"));
    }
    let len = match rest {
        [a, b, ..] => usize::from(u16::from_be_bytes([*a, *b])),
        _ => return Err("no root name".to_owned()),
    };
    let body = rest
        .get(2 + len..)
        .ok_or("the root name runs past the end")?;
    let anon: Vec<u8> = std::iter::once(10).chain(body.iter().copied()).collect();
    nbt::read_root(&mut Reader::new(&anon)).map_err(|e| format!("{e:?}"))
}

fn int(v: Option<&Nbt>) -> Option<i32> {
    match v? {
        Nbt::Int(i) => Some(*i),
        Nbt::Byte(b) => Some(i32::from(*b)),
        Nbt::Short(s) => Some(i32::from(*s)),
        _ => None,
    }
}

/// Unpack `n` indices of `bits` each from longs that never split a value.
fn unpack(data: &[i64], bits: usize, n: usize) -> Vec<usize> {
    if bits == 0 {
        return vec![0; n];
    }
    let per = 64 / bits;
    let mask = (1u64 << bits) - 1;
    (0..n)
        .map(|i| {
            let word = data.get(i / per).copied().unwrap_or(0) as u64;
            usize::try_from((word >> ((i % per) * bits)) & mask).unwrap_or(0)
        })
        .collect()
}

fn bits_for(len: usize) -> usize {
    if len <= 1 {
        0
    } else {
        (usize::BITS - (len - 1).leading_zeros()) as usize
    }
}

/// A paletted container's names: `n` entries, `min_bits` the format's floor (4 for blocks).
fn container(c: Option<&Nbt>, n: usize, min_bits: usize, name: fn(&Nbt) -> String) -> Vec<String> {
    let Some(c) = c else {
        return vec!["?".to_owned(); n];
    };
    let palette: Vec<String> = match c.get("palette") {
        Some(Nbt::List(p)) => p.iter().map(name).collect(),
        _ => Vec::new(),
    };
    if palette.len() <= 1 {
        let one = palette.first().cloned().unwrap_or_else(|| "?".to_owned());
        return vec![one; n];
    }
    let bits = bits_for(palette.len()).max(min_bits);
    let data: &[i64] = match c.get("data") {
        Some(Nbt::LongArray(d)) => d,
        _ => &[],
    };
    unpack(data, bits, n)
        .into_iter()
        .map(|i| palette.get(i).cloned().unwrap_or_else(|| "?".to_owned()))
        .collect()
}

fn block_name(v: &Nbt) -> String {
    v.get("Name")
        .and_then(Nbt::as_str)
        .unwrap_or("?")
        .to_owned()
}

fn biome_name(v: &Nbt) -> String {
    v.as_str().unwrap_or("?").to_owned()
}

fn chunk(root: &Nbt) -> Option<Chunk> {
    let cx = int(root.get("xPos"))?;
    let cz = int(root.get("zPos"))?;
    let mut sections: Vec<Section> = match root.get("sections") {
        Some(Nbt::List(s)) => s
            .iter()
            .filter_map(|s| {
                Some(Section {
                    y: int(s.get("Y"))?,
                    blocks: container(s.get("block_states"), 4096, 4, block_name),
                    biomes: container(s.get("biomes"), 64, 0, biome_name),
                })
            })
            .collect(),
        _ => Vec::new(),
    };
    sections.sort_by_key(|s| s.y);
    Some(Chunk { cx, cz, sections })
}

/// Every generated chunk of a region file (`status` full or not: a strip point forceloaded is
/// full).
pub fn read_region(path: &Path) -> Result<Vec<Chunk>, String> {
    let raw = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    // A region file the server has made but not yet written (or written empty) has no header:
    // it holds no chunk (probe 45688112814: three such files under a forceloaded strip).
    if raw.len() < 8192 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for i in 0..1024 {
        let loc = raw.get(i * 4..i * 4 + 4).ok_or("a short header")?;
        let offset =
            (usize::from(loc[0]) << 16 | usize::from(loc[1]) << 8 | usize::from(loc[2])) * 4096;
        if offset == 0 {
            continue;
        }
        let Some(head) = raw.get(offset..offset + 5) else {
            continue;
        };
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let Some(body) = raw.get(offset + 5..offset + 4 + len) else {
            continue;
        };
        let mut bytes = Vec::new();
        let ok = match head[4] {
            1 => flate2::read::GzDecoder::new(body)
                .read_to_end(&mut bytes)
                .is_ok(),
            2 => flate2::read::ZlibDecoder::new(body)
                .read_to_end(&mut bytes)
                .is_ok(),
            3 => {
                bytes.extend_from_slice(body);
                true
            }
            // 128+: the chunk is in an external `.mcc` file; 4 is LZ4. Neither is read here.
            _ => false,
        };
        if !ok {
            continue;
        }
        if let Some(c) = named_root(&bytes).ok().as_ref().and_then(chunk) {
            out.push(c);
        }
    }
    Ok(out)
}

/// Every region file under a directory.
pub fn regions(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "mca"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// A found block: its name, cell and biome.
pub type Found = (String, [i32; 3], String);

fn is_air(b: &str) -> bool {
    matches!(
        b,
        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" | "?"
    )
}

/// Every cell of a block name in a region directory, with its biome, inside `within` (two
/// corners, inclusive) when given. The name `*` is any block but air.
pub fn find(
    dir: &Path,
    names: &[String],
    within: Option<([i32; 3], [i32; 3])>,
) -> Result<Vec<Found>, String> {
    let any = names.iter().any(|n| n == "minecraft:*" || n == "*");
    let inside = |p: [i32; 3]| {
        within.is_none_or(|(a, b)| (0..3).all(|k| p[k] >= a[k].min(b[k]) && p[k] <= a[k].max(b[k])))
    };
    let mut out = Vec::new();
    for r in regions(dir) {
        for c in read_region(&r)? {
            if let Some((a, b)) = within {
                let (x0, x1) = (a[0].min(b[0]) >> 4, a[0].max(b[0]) >> 4);
                let (z0, z1) = (a[2].min(b[2]) >> 4, a[2].max(b[2]) >> 4);
                if c.cx < x0 || c.cx > x1 || c.cz < z0 || c.cz > z1 {
                    continue;
                }
            }
            for s in &c.sections {
                for (i, b) in s.blocks.iter().enumerate() {
                    let wanted = if any {
                        !is_air(b)
                    } else {
                        names.iter().any(|n| n == b)
                    };
                    if !wanted {
                        continue;
                    }
                    let (lx, ly, lz) = (i % 16, i / 256, (i / 16) % 16);
                    let x = c.cx * 16 + lx as i32;
                    let z = c.cz * 16 + lz as i32;
                    let y = s.y * 16 + ly as i32;
                    if !inside([x, y, z]) {
                        continue;
                    }
                    let biome = s
                        .biomes
                        .get((ly / 4) * 16 + (lz / 4) * 4 + lx / 4)
                        .cloned()
                        .unwrap_or_default();
                    out.push((b.clone(), [x, y, z], biome));
                }
            }
        }
    }
    out.sort_by_key(|(n, p, _)| (n.clone(), p[1], p[0], p[2]));
    Ok(out)
}

/// The top of a column: the highest block that is not air (water counts), its y and biome.
pub fn top(chunks: &[Chunk], x: i32, z: i32) -> Option<(String, i32, String)> {
    let c = chunks
        .iter()
        .find(|c| c.cx == x.div_euclid(16) && c.cz == z.div_euclid(16))?;
    let (lx, lz) = (x.rem_euclid(16) as usize, z.rem_euclid(16) as usize);
    c.sections.iter().rev().find_map(|s| {
        (0..16usize).rev().find_map(|ly| {
            let b = s.blocks.get(ly * 256 + lz * 16 + lx)?;
            if is_air(b) {
                return None;
            }
            let biome = s
                .biomes
                .get((ly / 4) * 16 + (lz / 4) * 4 + lx / 4)
                .cloned()
                .unwrap_or_default();
            Some((b.clone(), s.y * 16 + ly as i32, biome))
        })
    })
}

impl Chunk {
    /// Every cell of the chunk as `(cell, block, biome)`, lowest section first (issue 19, Q9's
    /// route survey: `crate::route` reads a region a file at a time into its own grid).
    pub fn cells(&self) -> impl Iterator<Item = ([i32; 3], &str, &str)> + '_ {
        self.sections.iter().flat_map(move |s| {
            s.blocks.iter().enumerate().map(move |(i, b)| {
                let (lx, ly, lz) = (i % 16, i / 256, (i / 16) % 16);
                let biome = s
                    .biomes
                    .get((ly / 4) * 16 + (lz / 4) * 4 + lx / 4)
                    .map_or("", String::as_str);
                (
                    [
                        self.cx * 16 + lx as i32,
                        s.y * 16 + ly as i32,
                        self.cz * 16 + lz as i32,
                    ],
                    b.as_str(),
                    biome,
                )
            })
        })
    }
}

/// Every chunk in a region directory.
pub fn all_chunks(dir: &Path) -> Result<Vec<Chunk>, String> {
    let mut out = Vec::new();
    for r in regions(dir) {
        out.extend(read_region(&r)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpack_reads_values_that_never_span_longs() {
        // 5 bits: 12 values a long, the top 4 bits unused.
        let word: u64 = (1..=12u64).fold(0, |w, i| w | (i << ((i - 1) * 5)));
        let v = unpack(&[word as i64, 13], 5, 13);
        assert_eq!(v, (1..=13).collect::<Vec<usize>>());
    }

    #[test]
    fn bits_cover_the_palette() {
        assert_eq!(bits_for(1), 0);
        assert_eq!(bits_for(2), 1);
        assert_eq!(bits_for(16), 4);
        assert_eq!(bits_for(17), 5);
    }
}
