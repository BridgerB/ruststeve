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
const WATCHED: [BlockKind; 5] = [
    BlockKind::CraftingTable,
    BlockKind::Furnace,
    BlockKind::NetherBricks,
    BlockKind::Spawner,
    BlockKind::Obsidian,
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
        self.latest.insert(key, self.log.len());
        self.log.push(Sighting {
            dimension: seen.dimension,
            pos: seen.pos,
            state: seen.state,
            at_tick,
        });
        true
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

    /// Every sighting, oldest first.
    pub fn log(&self) -> &[Sighting] {
        &self.log
    }

    /// The latest sighting of every position on record in one chunk column.
    pub fn in_column(&self, dimension: &str, cx: i32, cz: i32) -> Vec<&Sighting> {
        self.by_column
            .get(&(dimension.to_owned(), cx, cz))
            .into_iter()
            .flatten()
            .filter_map(|p| self.latest(dimension, *p))
            .collect()
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(kind: BlockKind) -> u32 {
        kind.info().map(|i| i.default_state).unwrap()
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
