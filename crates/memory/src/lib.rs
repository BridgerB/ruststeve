//! Memory as facts with time (brain/ruststeve/design/memory-sightings.md): sightings of watched
//! blocks, appended when what is at a position changes, never retracted, indexed by position and
//! by chunk column. A port of clojurecraft's `memory.clj` sightings.
//!
//! **No X-ray, by construction.** A `Sighting` can only come from a `Seen`, and a `Seen` can only
//! be made by `Seen::check`, which needs an exposed face: a loaded neighbour whose collision is not
//! a full cube. A block buried in rock can be in a chunk the server sent and still never enter
//! memory (brain/history/hist-no-xray.md).

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use data::BlockKind;
use data::generated::shapes::collision;
use data::generated::tags::block_tag;
use data::types::Collision;

/// A block position.
pub type Cell = (i32, i32, i32);

/// The kinds that start a record, besides the `logs` tag: what the plan looks for (tables,
/// furnaces, a fortress's bricks, a spawner, a portal's obsidian). A position on record is then
/// followed through every change. Water and lava are not cells here: clojurecraft found one fact
/// per fluid cell froze a forest run (600,000 water cells); fluids become per-chunk pools later.
const WATCHED: [BlockKind; 6] = [
    BlockKind::CraftingTable,
    BlockKind::Furnace,
    BlockKind::NetherBricks,
    BlockKind::Spawner,
    BlockKind::Obsidian,
    // A lit portal, walked back to and stepped into (issue 15c).
    BlockKind::NetherPortal,
];

/// Whether a block state is a kind memory watches.
pub fn watched(state: u32) -> bool {
    let Some(kind) = data::block_of_state(state) else {
        return false;
    };
    WATCHED.contains(&kind) || block_tag("logs").is_some_and(|logs| logs.contains(&kind))
}

/// Whether a neighbouring cell lets a face be seen: loaded, and not a full cube (air, water,
/// glass, a slab). An unloaded neighbour does not count.
fn see_through(neighbour: Option<u32>) -> bool {
    neighbour
        .and_then(data::block_of_state)
        .and_then(collision)
        .is_some_and(|c| c != Collision::Full)
}

/// A fluid memory keeps as pools (issue 14b).
#[derive(
    serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum Fluid {
    Water,
    Lava,
}

/// Exposed sources kept per chunk column per fluid: one fact per fluid cell froze clojurecraft's
/// forest run (600,000 water cells), so a pool keeps at most this many, the first seen.
pub const POOL_CELLS: usize = 32;

fn source_of(state: u32) -> Option<Fluid> {
    let kind = data::block_of_state(state)?;
    let fluid = match kind {
        BlockKind::Water => Fluid::Water,
        BlockKind::Lava => Fluid::Lava,
        _ => return None,
    };
    // Level 0 is a source; the fluid's states run level 0 to 15 from its default.
    (kind.info().map(|i| i.default_state) == Some(state)).then_some(fluid)
}

/// Whether a cell may hold a fluid source memory keeps: water or lava, level 0.
pub fn fluid_source(state: u32) -> bool {
    source_of(state).is_some()
}

fn air(state: Option<u32>) -> bool {
    state
        .and_then(data::block_of_state)
        .is_some_and(|k| matches!(k, BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir))
}

/// A fluid source seen exposed (issue 14b): water with air straight over it (a surface a bucket
/// reaches; cave water under a ceiling is the aquifer trap), lava with an air neighbour. Only
/// `SeenSource::check` makes one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeenSource {
    dimension: String,
    pos: Cell,
    fluid: Fluid,
}

impl SeenSource {
    /// `neighbours` in `neighbours`'s order: down, up, north, south, west, east.
    pub fn check(
        dimension: &str,
        pos: Cell,
        state: u32,
        neighbours: [Option<u32>; 6],
    ) -> Option<Self> {
        let fluid = source_of(state)?;
        let exposed = match fluid {
            Fluid::Water => air(neighbours[1]),
            Fluid::Lava => neighbours.iter().any(|n| air(*n)),
        };
        exposed.then(|| Self {
            dimension: dimension.to_owned(),
            pos,
            fluid,
        })
    }
}

/// A block seen with an exposed face. Only `Seen::check` makes one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    dimension: String,
    pos: Cell,
    state: u32,
}

impl Seen {
    /// The block at `pos` is seen if any of its six neighbours (their states; `None` where not
    /// loaded) is see-through.
    pub fn check(
        dimension: &str,
        pos: Cell,
        state: u32,
        neighbours: [Option<u32>; 6],
    ) -> Option<Self> {
        neighbours.iter().any(|n| see_through(*n)).then(|| Self {
            dimension: dimension.to_owned(),
            pos,
            state,
        })
    }

    pub fn pos(&self) -> Cell {
        self.pos
    }
}

/// One fact: what was at a position, in a dimension, from a server tick on.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Sighting {
    pub dimension: String,
    pub pos: Cell,
    pub state: u32,
    pub at_tick: u64,
}

/// The six face neighbours of a cell, in a fixed order.
pub fn neighbours(pos: Cell) -> [Cell; 6] {
    let (x, y, z) = pos;
    [
        (x, y - 1, z),
        (x, y + 1, z),
        (x, y, z - 1),
        (x, y, z + 1),
        (x - 1, y, z),
        (x + 1, y, z),
    ]
}

/// The sightings and their indexes.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    /// Every sighting, in the order it was made. Never edited or shortened.
    log: Vec<Sighting>,
    /// The latest sighting at each position: an index into `log`.
    latest: BTreeMap<(String, Cell), usize>,
    /// Positions on record, by dimension and chunk column.
    by_column: BTreeMap<(String, i32, i32), BTreeSet<Cell>>,
    /// Every block kind ever sighted, in any dimension: answers "seen nether bricks?" without a
    /// scan of the log (the stage ladder asks every tick). Added in issue 07; older
    /// checkpoints read it as empty.
    #[serde(default)]
    kinds: BTreeSet<BlockKind>,
    /// Exposed fluid sources by dimension, chunk column and fluid, at most `POOL_CELLS` each
    /// (issue 14b). A source gone (scooped, turned to obsidian) is dropped when its cell is seen
    /// again.
    #[serde(default)]
    pools: BTreeMap<(String, i32, i32, Fluid), BTreeSet<Cell>>,
    /// The positions whose latest sighting is one of `INDEXED`, by dimension, chunk column and
    /// kind: the stage ladder and the planner look for spawners, nether bricks and portals in 25
    /// to 289 columns, and a scan of every sighting there took about 130 ms a look inside a
    /// fortress (a real bot fell behind the server and was dropped: race 45350933958 lane 1).
    #[serde(default)]
    by_kind: BTreeMap<(String, i32, i32, BlockKind), BTreeSet<Cell>>,
    /// Set on a store read from before `by_kind` (its index is empty): `indexed_in_column` then
    /// answers `None` and the caller scans, as before.
    #[serde(default = "unindexed")]
    unindexed: bool,
    /// Biome signs seen exposed (issue 17b), by dimension, chunk column and sign: at most
    /// `SIGNS_KEPT` cells each, so a soul sand valley's thousands of faces stay a small record.
    /// Never retracted: a sign is evidence of the biome where it grew, dug or not.
    #[serde(default)]
    signs: BTreeMap<(String, i32, i32, Sign), BTreeSet<Cell>>,
    /// Nether columns with `FOREST_SIGNS` warped signs seen, in the order each became known.
    #[serde(default)]
    forests: Vec<(i32, i32)>,
    /// Nether columns with `SOUL_SIGNS` soul sand or soul soil seen, in the order each became known.
    #[serde(default)]
    soul_valleys: Vec<(i32, i32)>,
}

/// Kinds kept in `Memory::by_kind`.
pub const INDEXED: &[BlockKind] = &[
    BlockKind::Spawner,
    BlockKind::NetherBricks,
    BlockKind::NetherPortal,
];

fn unindexed() -> bool {
    true
}
/// The Nether, where biome signs are read.
const NETHER: &str = "minecraft:the_nether";

/// A biome a block is a sign of (issue 17b; Bridger 2026-11-24: the bot may know biomes
/// evidenced by blocks it has seen exposed, never the biome of an unseen column). Each kind
/// grows naturally only in its biome's surface and features (the 26.1.2 biome JSONs'
/// `features`: `warped_forest_vegetation`, `crimson_forest_vegetation`; the surface rules'
/// nylium, soul sand and soil, basalt and blackstone).
#[derive(
    serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum Sign {
    /// Warped forest: warped nylium, warped wart block, warped stem and hyphae.
    Warped,
    /// Crimson forest: crimson nylium, nether wart block.
    Crimson,
    /// Soul sand valley: soul sand, soul soil.
    Soul,
    /// Basalt deltas: basalt, blackstone.
    Basalt,
}

/// The biome sign a block kind is, if any.
pub fn sign_of(kind: BlockKind) -> Option<Sign> {
    match kind {
        BlockKind::WarpedNylium
        | BlockKind::WarpedWartBlock
        | BlockKind::WarpedStem
        | BlockKind::WarpedHyphae => Some(Sign::Warped),
        BlockKind::CrimsonNylium | BlockKind::NetherWartBlock => Some(Sign::Crimson),
        BlockKind::SoulSand | BlockKind::SoulSoil => Some(Sign::Soul),
        BlockKind::Basalt | BlockKind::Blackstone => Some(Sign::Basalt),
        _ => None,
    }
}

/// Whether a state is a biome sign (the cheap palette test before a section scan).
pub fn is_sign(state: u32) -> bool {
    data::block_of_state(state).and_then(sign_of).is_some()
}

/// Warped signs seen in one chunk column that make it a known forest: clojurecraft's threshold
/// (12 `warped_nylium` seen in a chunk, `pearls-rules.edn` `:forest-min-nylium`), kept until a
/// race says otherwise.
pub const FOREST_SIGNS: usize = 12;
/// Cells kept per column and sign: more than `FOREST_SIGNS`, few enough that a column of
/// soul soil costs at most 64 cells.
pub const SIGNS_KEPT: usize = 64;
/// Soul signs seen in one Nether column that make it a known soul sand valley: the forest's
/// threshold, by analogy (soul soil and soul sand also lie under fortresses and in nether wastes
/// patches; 12 exposed in one column is a floor, not a stray block), kept until a race says
/// otherwise.
pub const SOUL_SIGNS: usize = 12;

/// The signs seen in one column, counted (capped at `SIGNS_KEPT` each).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BiomeSigns {
    pub warped: usize,
    pub crimson: usize,
    pub soul: usize,
    pub basalt: usize,
}

impl Memory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a seen block. Appended only when it differs from the latest sighting there;
    /// returns whether it was.
    pub fn record(&mut self, seen: Seen, at_tick: u64) -> bool {
        let key = (seen.dimension.clone(), seen.pos);
        if let Some(i) = self.latest.get(&key)
            && self.log.get(*i).is_some_and(|s| s.state == seen.state)
        {
            return false;
        }
        let (x, _, z) = seen.pos;
        self.by_column
            .entry((seen.dimension.clone(), x >> 4, z >> 4))
            .or_default()
            .insert(seen.pos);
        if let Some(kind) = data::block_of_state(seen.state) {
            self.kinds.insert(kind);
        }
        let was = self
            .latest
            .get(&key)
            .and_then(|i| self.log.get(*i))
            .and_then(|s| data::block_of_state(s.state));
        let now = data::block_of_state(seen.state);
        if was != now {
            if let Some(k) = was.filter(|k| INDEXED.contains(k))
                && let Some(set) =
                    self.by_kind
                        .get_mut(&(seen.dimension.clone(), x >> 4, z >> 4, k))
            {
                set.remove(&seen.pos);
            }
            if let Some(k) = now.filter(|k| INDEXED.contains(k)) {
                self.by_kind
                    .entry((seen.dimension.clone(), x >> 4, z >> 4, k))
                    .or_default()
                    .insert(seen.pos);
            }
        }
        self.latest.insert(key, self.log.len());
        self.log.push(Sighting {
            dimension: seen.dimension,
            pos: seen.pos,
            state: seen.state,
            at_tick,
        });
        true
    }

    /// Keep an exposed source in its pool (issue 14b); returns whether it was added.
    pub fn record_source(&mut self, seen: SeenSource) -> bool {
        let (x, _, z) = seen.pos;
        let pool = self
            .pools
            .entry((seen.dimension, x >> 4, z >> 4, seen.fluid))
            .or_default();
        if pool.len() >= POOL_CELLS {
            return false;
        }
        pool.insert(seen.pos)
    }

    /// Whether a column's pool of a fluid is full.
    pub fn pool_full(&self, dimension: &str, cx: i32, cz: i32, fluid: Fluid) -> bool {
        self.pools
            .get(&(dimension.to_owned(), cx, cz, fluid))
            .is_some_and(|p| p.len() >= POOL_CELLS)
    }

    /// The fluid of a source state, if it is one.
    pub fn source_fluid(state: u32) -> Option<Fluid> {
        source_of(state)
    }

    /// A cell no longer holds the source it was kept for: out of every pool.
    pub fn forget_source(&mut self, dimension: &str, pos: Cell) {
        let (x, _, z) = pos;
        for fluid in [Fluid::Water, Fluid::Lava] {
            if let Some(pool) = self
                .pools
                .get_mut(&(dimension.to_owned(), x >> 4, z >> 4, fluid))
            {
                pool.remove(&pos);
            }
        }
    }

    /// The kept sources of a fluid in the chunk columns within `radius` columns of `(cx, cz)`.
    pub fn sources_near(
        &self,
        dimension: &str,
        fluid: Fluid,
        cx: i32,
        cz: i32,
        radius: i32,
    ) -> Vec<Cell> {
        let mut out = Vec::new();
        for x in cx - radius..=cx + radius {
            for z in cz - radius..=cz + radius {
                if let Some(pool) = self.pools.get(&(dimension.to_owned(), x, z, fluid)) {
                    out.extend(pool.iter().copied());
                }
            }
        }
        out
    }

    /// Whether a position is on record (so every later change there is followed).
    pub fn on_record(&self, dimension: &str, pos: Cell) -> bool {
        self.latest.contains_key(&(dimension.to_owned(), pos))
    }

    /// The latest sighting at a position.
    pub fn latest(&self, dimension: &str, pos: Cell) -> Option<&Sighting> {
        let i = self.latest.get(&(dimension.to_owned(), pos))?;
        self.log.get(*i)
    }

    /// Whether a block of this kind was ever sighted.
    pub fn has_seen(&self, kind: BlockKind) -> bool {
        self.kinds.contains(&kind)
    }

    /// Every sighting, oldest first.
    pub fn log(&self) -> &[Sighting] {
        &self.log
    }

    /// The latest sighting of every position on record in one chunk column.
    ///
    /// One key is made and reused for every lookup: `latest` makes a `String` a call, and a
    /// fortress column holds thousands of sightings; the spawner and fortress scans of 25 columns
    /// cost about 100 ms (callgrind of a replay of race 45350933958 lane 1, 90% in
    /// `Memory::latest`), and a real bot fell behind the server and was dropped.
    pub fn in_column(&self, dimension: &str, cx: i32, cz: i32) -> Vec<&Sighting> {
        let mut key = (dimension.to_owned(), (0, 0, 0));
        self.by_column
            .get(&(dimension.to_owned(), cx, cz))
            .into_iter()
            .flatten()
            .filter_map(|p| {
                key.1 = *p;
                self.latest.get(&key).and_then(|i| self.log.get(*i))
            })
            .collect()
    }

    /// Build the kind index from the latest sightings, for a store read from before it (a replay
    /// of an old recording asked about the planner's speed today).
    pub fn reindex(&mut self) {
        let mut by_kind: BTreeMap<(String, i32, i32, BlockKind), BTreeSet<Cell>> = BTreeMap::new();
        for ((dim, pos), i) in &self.latest {
            if let Some(k) = self
                .log
                .get(*i)
                .and_then(|s| data::block_of_state(s.state))
                .filter(|k| INDEXED.contains(k))
            {
                by_kind
                    .entry((dim.clone(), pos.0 >> 4, pos.2 >> 4, k))
                    .or_default()
                    .insert(*pos);
            }
        }
        self.by_kind = by_kind;
        self.unindexed = false;
    }

    /// The positions in one chunk column whose latest sighting is `kind`, from the index: `None`
    /// when `kind` is not one of `INDEXED` or the store predates the index (scan `in_column`).
    pub fn indexed_in_column(
        &self,
        dimension: &str,
        cx: i32,
        cz: i32,
        kind: BlockKind,
    ) -> Option<Vec<Cell>> {
        if self.unindexed || !INDEXED.contains(&kind) {
            return None;
        }
        Some(
            self.by_kind
                .get(&(dimension.to_owned(), cx, cz, kind))
                .map(|set| set.iter().copied().collect())
                .unwrap_or_default(),
        )
    }

    /// The latest index rebuilt by folding the log: equal to `latest` when the store is sound.
    pub fn refold_latest(&self) -> BTreeMap<(String, Cell), usize> {
        self.log
            .iter()
            .enumerate()
            .map(|(i, s)| ((s.dimension.clone(), s.pos), i))
            .collect()
    }

    /// The latest index as kept.
    pub fn latest_index(&self) -> &BTreeMap<(String, Cell), usize> {
        &self.latest
    }

    /// Whether a column's record of a sign is full (a scan of its sections can skip it).
    pub fn signs_full(&self, dimension: &str, cx: i32, cz: i32, sign: Sign) -> bool {
        self.signs
            .get(&(dimension.to_owned(), cx, cz, sign))
            .is_some_and(|s| s.len() >= SIGNS_KEPT)
    }

    /// Record a biome sign. Only a `Seen` (an exposed face) can be one: no X-ray. Returns
    /// whether the cell was new; a Nether column reaching `FOREST_SIGNS` warped signs becomes a
    /// known forest.
    pub fn record_sign(&mut self, seen: &Seen) -> bool {
        let Some(sign) = data::block_of_state(seen.state).and_then(sign_of) else {
            return false;
        };
        let (x, _, z) = seen.pos;
        let (cx, cz) = (x >> 4, z >> 4);
        let cells = self
            .signs
            .entry((seen.dimension.clone(), cx, cz, sign))
            .or_default();
        if cells.len() >= SIGNS_KEPT || !cells.insert(seen.pos) {
            return false;
        }
        if sign == Sign::Warped
            && seen.dimension == NETHER
            && cells.len() >= FOREST_SIGNS
            && !self.forests.contains(&(cx, cz))
        {
            self.forests.push((cx, cz));
        }
        if sign == Sign::Soul
            && seen.dimension == NETHER
            && cells.len() >= SOUL_SIGNS
            && !self.soul_valleys.contains(&(cx, cz))
        {
            self.soul_valleys.push((cx, cz));
        }
        true
    }

    /// The signs seen in a Nether chunk column.
    pub fn biome_signs_in_column(&self, cx: i32, cz: i32) -> BiomeSigns {
        let n = |sign| {
            self.signs
                .get(&(NETHER.to_owned(), cx, cz, sign))
                .map_or(0, BTreeSet::len)
        };
        BiomeSigns {
            warped: n(Sign::Warped),
            crimson: n(Sign::Crimson),
            soul: n(Sign::Soul),
            basalt: n(Sign::Basalt),
        }
    }

    /// The first Nether column known as a warped forest from signs seen, kept after it unloads.
    pub fn forest_known(&self) -> Option<(i32, i32)> {
        self.forests.first().copied()
    }

    /// Every Nether column known as a warped forest, in the order each became known.
    pub fn forests(&self) -> &[(i32, i32)] {
        &self.forests
    }

    /// The first Nether column known as a soul sand valley from signs seen (issue 17: the valley
    /// 32 blocks from the frontier's spawner spawns endermen, 1 in 71 of its monster weight).
    pub fn soul_valley_known(&self) -> Option<(i32, i32)> {
        self.soul_valleys.first().copied()
    }

    /// Every Nether column known as a soul sand valley, in the order each became known.
    pub fn soul_valleys(&self) -> &[(i32, i32)] {
        &self.soul_valleys
    }

    /// The chunk columns with any position on record in a dimension, in key order: a range of
    /// the column index, never a scan of the sightings (issue 17d's heading counts seen columns).
    pub fn seen_columns(&self, dimension: &str) -> Vec<(i32, i32)> {
        let lo = (dimension.to_owned(), i32::MIN, i32::MIN);
        let hi = (dimension.to_owned(), i32::MAX, i32::MAX);
        self.by_column
            .range(lo..=hi)
            .filter(|(_, cells)| !cells.is_empty())
            .map(|((_, cx, cz), _)| (*cx, *cz))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(kind: BlockKind) -> u32 {
        kind.info().map(|i| i.default_state).unwrap()
    }

    /// The kind index follows the latest sighting: a spawner seen is listed in its column, the
    /// same cell seen as air is not; a store from before the index answers `None`.
    #[test]
    fn the_kind_index_follows_the_latest_sighting() {
        let nether = "minecraft:the_nether";
        let seen = |pos: Cell, kind: BlockKind| Seen {
            dimension: nether.to_owned(),
            pos,
            state: state(kind),
        };
        let mut m = Memory::new();
        assert!(m.record(seen((129, 64, 159), BlockKind::Spawner), 1));
        assert!(m.record(seen((130, 64, 159), BlockKind::NetherBricks), 1));
        let col = |m: &Memory, k| m.indexed_in_column(nether, 8, 9, k);
        assert_eq!(col(&m, BlockKind::Spawner), Some(vec![(129, 64, 159)]));
        assert_eq!(col(&m, BlockKind::Stone), None, "not an indexed kind");
        assert!(m.record(seen((129, 64, 159), BlockKind::Air), 2));
        assert_eq!(col(&m, BlockKind::Spawner), Some(vec![]));
        assert_eq!(col(&m, BlockKind::NetherBricks), Some(vec![(130, 64, 159)]));
        let old = Memory {
            unindexed: true,
            ..m.clone()
        };
        assert_eq!(
            col(&old, BlockKind::NetherBricks),
            None,
            "scan the old store"
        );
    }

    /// Twelve warped signs seen exposed in one Nether column make it a known forest, kept with
    /// no column loaded; signs are capped per column and counted by biome; an overworld column
    /// never becomes one.
    #[test]
    fn warped_signs_seen_make_a_known_forest() {
        let nether = "minecraft:the_nether";
        let air = Some(state(BlockKind::Air));
        let seen = |dim: &str, pos: Cell, kind: BlockKind| {
            Seen::check(dim, pos, state(kind), [air; 6]).unwrap()
        };
        let mut m = Memory::new();
        for x in 0..11 {
            assert!(m.record_sign(&seen(nether, (x, 40, 5), BlockKind::WarpedNylium)));
        }
        assert!(
            !m.record_sign(&seen(nether, (0, 40, 5), BlockKind::WarpedNylium)),
            "once a cell"
        );
        assert_eq!(m.forest_known(), None, "11 signs are not a forest");
        assert!(m.record_sign(&seen(nether, (3, 41, 5), BlockKind::WarpedWartBlock)));
        assert_eq!(m.forest_known(), Some((0, 0)));
        assert!(
            !m.record_sign(&seen(nether, (5, 40, 5), BlockKind::Netherrack)),
            "not a sign"
        );
        for z in 0..16 {
            for x in 0..16 {
                m.record_sign(&seen(nether, (x + 32, 30, z), BlockKind::SoulSoil));
            }
        }
        let signs = m.biome_signs_in_column(2, 0);
        assert_eq!(signs.soul, SIGNS_KEPT, "capped");
        assert_eq!(m.soul_valley_known(), Some((2, 0)));
        assert_eq!(m.soul_valleys(), &[(2, 0)]);
        assert_eq!(m.biome_signs_in_column(0, 0).warped, 12);
        for x in 0..16 {
            m.record_sign(&seen(
                "minecraft:overworld",
                (x, 70, 100),
                BlockKind::WarpedNylium,
            ));
        }
        assert_eq!(m.forests(), &[(0, 0)], "the overworld has no warped forest");
        let mut buried = [Some(state(BlockKind::Netherrack)); 6];
        assert_eq!(
            Seen::check(nether, (0, 0, 0), state(BlockKind::WarpedNylium), buried),
            None
        );
        buried[1] = air;
        assert!(Seen::check(nether, (0, 0, 0), state(BlockKind::WarpedNylium), buried).is_some());
    }

    /// The columns on record in one dimension, from the column index's range only.
    #[test]
    fn seen_columns_are_one_dimensions() {
        let rec = |m: &mut Memory, dim: &str, pos: Cell| {
            m.record(
                Seen {
                    dimension: dim.to_owned(),
                    pos,
                    state: state(BlockKind::NetherBricks),
                },
                1,
            )
        };
        let mut m = Memory::new();
        rec(&mut m, "minecraft:the_nether", (-1, 64, 0));
        rec(&mut m, "minecraft:the_nether", (40, 64, -20));
        rec(&mut m, "minecraft:the_nether", (41, 64, -21));
        rec(&mut m, "minecraft:overworld", (500, 64, 500));
        assert_eq!(
            m.seen_columns("minecraft:the_nether"),
            vec![(-1, 0), (2, -2)]
        );
        assert_eq!(m.seen_columns("minecraft:overworld"), vec![(31, 31)]);
        assert!(m.seen_columns("minecraft:the_end").is_empty());
    }

    #[test]
    fn a_buried_block_is_not_seen() {
        let stone = Some(state(BlockKind::Stone));
        assert_eq!(
            Seen::check(
                "minecraft:overworld",
                (0, 0, 0),
                state(BlockKind::OakLog),
                [stone; 6]
            ),
            None
        );
        // An unloaded neighbour is not a face either.
        let mut n = [stone; 6];
        n[1] = None;
        assert_eq!(
            Seen::check(
                "minecraft:overworld",
                (0, 0, 0),
                state(BlockKind::OakLog),
                n
            ),
            None
        );
    }

    #[test]
    fn one_air_face_is_enough() {
        let mut n = [Some(state(BlockKind::Stone)); 6];
        n[1] = Some(state(BlockKind::Air));
        assert!(
            Seen::check(
                "minecraft:overworld",
                (0, 0, 0),
                state(BlockKind::OakLog),
                n
            )
            .is_some()
        );
    }

    #[test]
    fn only_changes_are_appended() {
        let mut n = [Some(state(BlockKind::Stone)); 6];
        n[1] = Some(state(BlockKind::Air));
        let log = state(BlockKind::OakLog);
        let air = state(BlockKind::Air);
        let mut m = Memory::new();
        let seen = |s| Seen::check("minecraft:overworld", (1, 64, 1), s, n).unwrap();
        assert!(m.record(seen(log), 0));
        assert!(!m.record(seen(log), 20));
        assert!(m.record(seen(air), 40));
        assert_eq!(m.log().len(), 2);
        assert_eq!(
            m.latest("minecraft:overworld", (1, 64, 1)).map(|s| s.state),
            Some(air)
        );
        assert_eq!(m.refold_latest(), *m.latest_index());
        assert_eq!(m.in_column("minecraft:overworld", 0, 0).len(), 1);
    }

    #[test]
    fn logs_and_tables_are_watched_stone_is_not() {
        assert!(watched(state(BlockKind::OakLog)));
        assert!(watched(state(BlockKind::CraftingTable)));
        assert!(!watched(state(BlockKind::Stone)));
    }
}
