//! Chunk columns as received: every section's paletted containers kept packed, read cell by cell
//! on demand (brain/wire/wire-chunk-sections.md). A port of clojurecraft's `chunk.clj`
//! (`read-container`, `read-section-with-biomes`, `decode`, `section-get`, `block-index`),
//! checked against it at 200 positions (`crates/world/tests/oracle.rs`).

use crate::ParseError;
use crate::bytes::Reader;

/// Block containers: 4096 cells (16³), a palette up to 8 bits, above it direct state ids.
const BLOCK_ENTRIES: usize = 4096;
const BLOCK_MAX_BITS: u8 = 8;
/// Biome containers: 64 cells (4×4×4 blocks each), a palette up to 3 bits.
const BIOME_ENTRIES: usize = 64;
const BIOME_MAX_BITS: u8 = 3;

/// One paletted container, straight off the wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Paletted {
    /// Bits 0: one value for every cell.
    Single(u32),
    /// A palette, cells index into it.
    Indirect {
        bits: u8,
        palette: Vec<u32>,
        longs: Vec<u64>,
    },
    /// Above the max bits: cells hold the ids themselves.
    Direct { bits: u8, longs: Vec<u64> },
}

impl Paletted {
    /// One container. 26.1 sends no long count: it is `ceil(entries / floor(64 / bits))`.
    fn read(r: &mut Reader<'_>, max_bits: u8, entries: usize) -> Result<Self, ParseError> {
        let bits = r.u8()?;
        if bits == 0 {
            return Ok(Self::Single(r.varint()? as u32));
        }
        if bits > 32 {
            return Err(ParseError::BadBits(bits));
        }
        let palette = if bits <= max_bits {
            let n = r.length()?;
            let mut p = Vec::with_capacity(n.min(256));
            for _ in 0..n {
                p.push(r.varint()? as u32);
            }
            Some(p)
        } else {
            None
        };
        let per_long = 64 / usize::from(bits);
        let n = entries.div_ceil(per_long);
        let mut longs = Vec::with_capacity(n);
        for _ in 0..n {
            longs.push(r.i64()? as u64);
        }
        Ok(match palette {
            Some(palette) => Self::Indirect {
                bits,
                palette,
                longs,
            },
            None => Self::Direct { bits, longs },
        })
    }

    /// The value at cell `idx`, through the palette when there is one. Entries never straddle
    /// longs. `None` for a cell past the data or a palette index past the palette (malformed).
    pub fn get(&self, idx: usize) -> Option<u32> {
        let (bits, longs) = match self {
            Self::Single(v) => return Some(*v),
            Self::Indirect { bits, longs, .. } | Self::Direct { bits, longs } => (*bits, longs),
        };
        let bits = usize::from(bits);
        let per_long = 64 / bits;
        let long = longs.get(idx / per_long)?;
        let v = (long >> (bits * (idx % per_long))) & ((1u64 << bits) - 1);
        match self {
            Self::Indirect { palette, .. } => palette.get(usize::try_from(v).ok()?).copied(),
            Self::Direct { .. } | Self::Single(_) => u32::try_from(v).ok(),
        }
    }

    /// Whether any cell may hold a value `pred` accepts: exact for a single value or a palette,
    /// always true for direct ids. The cheap test before scanning 4096 cells.
    pub fn may_contain(&self, pred: impl Fn(u32) -> bool) -> bool {
        match self {
            Self::Single(v) => pred(*v),
            Self::Indirect { palette, .. } => palette.iter().any(|v| pred(*v)),
            Self::Direct { .. } => true,
        }
    }
}

/// One 16-block section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// Non-air blocks, as the server counts them.
    pub block_count: i16,
    /// Fluid blocks: new in 26.1; forgetting it shifts every later byte.
    pub fluid_count: i16,
    pub blocks: Paletted,
    pub biomes: Paletted,
}

/// A chunk column: its sections bottom to top, from `min_y`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    pub min_y: i32,
    pub sections: Vec<Section>,
}

/// The lowest block y of a column with this many sections: the dimension types with that many
/// all start at the same y (24: the overworld at -64; 16: the Nether and the End at 0), so a
/// column says its own height by its size (clojurecraft's `dimension/min-y-of-sections`).
pub fn min_y_of_sections(n: usize) -> Option<i32> {
    match n {
        24 => Some(-64),
        16 => Some(0),
        _ => None,
    }
}

/// Index of local cell (x, y, z), each 0-15, inside a section: y, then z, then x.
pub fn block_index(x: i32, y: i32, z: i32) -> usize {
    (((y & 15) << 8) | ((z & 15) << 4) | (x & 15)) as usize
}

impl Column {
    /// The chunk-data bytes of `level_chunk_with_light` as a column: every section the bytes
    /// hold, and the lowest y that many sections start at.
    pub fn decode(data: &[u8]) -> Result<Self, ParseError> {
        let mut r = Reader::new(data);
        let mut sections = Vec::new();
        while r.remaining() > 0 {
            sections.push(Section {
                block_count: r.i16()?,
                fluid_count: r.i16()?,
                blocks: Paletted::read(&mut r, BLOCK_MAX_BITS, BLOCK_ENTRIES)?,
                biomes: Paletted::read(&mut r, BIOME_MAX_BITS, BIOME_ENTRIES)?,
            });
        }
        let min_y =
            min_y_of_sections(sections.len()).ok_or(ParseError::BadSectionCount(sections.len()))?;
        Ok(Self { min_y, sections })
    }

    /// The highest block y + 1.
    pub fn max_y(&self) -> i32 {
        self.min_y + 16 * self.sections.len() as i32
    }

    /// The section holding world y, if the column reaches it.
    pub fn section_at(&self, y: i32) -> Option<&Section> {
        let i = usize::try_from((y - self.min_y) >> 4).ok()?;
        self.sections.get(i)
    }

    /// The block state id at world (x, y, z) in this column (x and z are taken mod 16);
    /// `None` above or below it.
    pub fn state_at(&self, x: i32, y: i32, z: i32) -> Option<u32> {
        self.section_at(y)?.blocks.get(block_index(x, y, z))
    }
}
