//! World column manager — block/light/biome access in world coordinates, with
//! a pluggable chunk provider and a drained event queue (instead of typecraft's
//! callback listeners). Port of typecraft's `world/world.ts`.

use std::collections::{HashMap, HashSet};

use crate::block::{get_block, set_block, BlockInfo};
use crate::chunk::ChunkColumn;
use crate::registry::Registry;
use crate::vec3::Vec3;

/// Loads/saves columns from a backing store (e.g. an anvil world).
pub trait ChunkProvider {
    fn load(&mut self, chunk_x: i32, chunk_z: i32) -> std::io::Result<Option<ChunkColumn>>;
    fn save(&mut self, chunk_x: i32, chunk_z: i32, column: &ChunkColumn) -> std::io::Result<()>;
}

/// Emitted world events, drained by the caller via [`World::take_events`].
#[derive(Debug, Clone)]
pub enum WorldEvent {
    BlockUpdate {
        pos: Vec3,
        old_state: u32,
        new_state: u32,
    },
    ChunkColumnLoad(i32, i32),
    ChunkColumnUnload(i32, i32),
}

pub struct World<'a> {
    pub columns: HashMap<(i32, i32), ChunkColumn>,
    pub registry: &'a Registry,
    provider: Option<Box<dyn ChunkProvider + 'a>>,
    #[allow(clippy::type_complexity)]
    generator: Option<Box<dyn FnMut(i32, i32) -> ChunkColumn + 'a>>,
    saving_queue: HashSet<(i32, i32)>,
    events: Vec<WorldEvent>,
    /// Positions of "interesting" blocks (gravel, lava, ores) keyed by block id,
    /// maintained incrementally on chunk load/unload/block-update. Lets tasks find the
    /// nearest one at ANY range with an O(k) lookup instead of scanning millions of
    /// blocks per search — a big synchronous scan froze the single async loop and got
    /// the bot kicked ("Timed out"). This just organizes the chunk data the bot already
    /// has; it is not X-ray for ore (ore-finding still goes through observe_blocks).
    block_index: HashMap<i32, Vec<(i32, i32, i32)>>,
    /// state id -> block id, only for the indexed block types (built once from registry).
    indexed_states: HashMap<u32, i32>,
}

/// Block types worth indexing — resources tasks hunt for. Kept small so the index
/// stays cheap; add a name to make it findable at any range via `nearest_indexed`.
const INDEXED_BLOCKS: &[&str] = &[
    "gravel",
    "lava",
    "coal_ore",
    "deepslate_coal_ore",
    "iron_ore",
    "deepslate_iron_ore",
    "gold_ore",
    "deepslate_gold_ore",
    "diamond_ore",
    "deepslate_diamond_ore",
];

fn build_indexed_states(registry: &Registry) -> HashMap<u32, i32> {
    let mut m = HashMap::new();
    for name in INDEXED_BLOCKS {
        if let Some(def) = registry.blocks_by_name.get(*name) {
            for s in def.min_state_id..=def.max_state_id {
                m.insert(s, def.id);
            }
        }
    }
    m
}

fn to_chunk(c: f64) -> i32 {
    (c.floor() as i32) >> 4
}

fn to_local(c: f64) -> i32 {
    (c.floor() as i32).rem_euclid(16)
}

/// A get/set accessor pair that forwards to the chunk column's same-named method:
/// the getter resolves the column read-only, the setter resolves it mutably and
/// queues a save. block-light, sky-light and biome id share this shape exactly.
macro_rules! col_accessor {
    ($get:ident, $set:ident, $ty:ty) => {
        pub fn $get(&self, pos: Vec3) -> Option<$ty> {
            let col = self.columns.get(&(to_chunk(pos.x), to_chunk(pos.z)))?;
            Some(col.$get(to_local(pos.x), pos.y.floor() as i32, to_local(pos.z)))
        }
        pub fn $set(&mut self, pos: Vec3, value: $ty) {
            let (lx, y, lz) = (to_local(pos.x), pos.y.floor() as i32, to_local(pos.z));
            let (cx, cz) = (to_chunk(pos.x), to_chunk(pos.z));
            if let Some(col) = self.column_at(pos) {
                col.$set(lx, y, lz, value);
            }
            self.queue_save(cx, cz);
        }
    };
}

impl<'a> World<'a> {
    pub fn new(registry: &'a Registry) -> World<'a> {
        World {
            columns: HashMap::new(),
            registry,
            provider: None,
            generator: None,
            saving_queue: HashSet::new(),
            events: Vec::new(),
            block_index: HashMap::new(),
            indexed_states: build_indexed_states(registry),
        }
    }

    pub fn with_provider(mut self, provider: Box<dyn ChunkProvider + 'a>) -> World<'a> {
        self.provider = Some(provider);
        self
    }

    pub fn with_generator(
        mut self,
        generator: Box<dyn FnMut(i32, i32) -> ChunkColumn + 'a>,
    ) -> World<'a> {
        self.generator = Some(generator);
        self
    }

    /// Drain accumulated events.
    pub fn take_events(&mut self) -> Vec<WorldEvent> {
        std::mem::take(&mut self.events)
    }

    // ── Column management ──

    /// Get a column from memory, the provider, or the generator (loading it).
    pub fn get_column(
        &mut self,
        chunk_x: i32,
        chunk_z: i32,
    ) -> std::io::Result<Option<&ChunkColumn>> {
        let key = (chunk_x, chunk_z);
        if !self.columns.contains_key(&key) {
            let mut column = match &mut self.provider {
                Some(p) => p.load(chunk_x, chunk_z)?,
                None => None,
            };
            if column.is_none() {
                if let Some(gen) = &mut self.generator {
                    column = Some(gen(chunk_x, chunk_z));
                }
            }
            if let Some(column) = column {
                self.set_column(chunk_x, chunk_z, column);
            }
        }
        Ok(self.columns.get(&key))
    }

    pub fn get_loaded_column(&self, chunk_x: i32, chunk_z: i32) -> Option<&ChunkColumn> {
        self.columns.get(&(chunk_x, chunk_z))
    }

    /// Number of chunk columns currently resident in the local world.
    pub fn loaded_chunk_count(&self) -> usize {
        self.columns.len()
    }

    pub fn set_column(&mut self, chunk_x: i32, chunk_z: i32, column: ChunkColumn) {
        if self.columns.contains_key(&(chunk_x, chunk_z)) {
            self.deindex_column(chunk_x, chunk_z); // replacing a reload — drop stale entries
        }
        self.index_column(chunk_x, chunk_z, &column);
        self.columns.insert((chunk_x, chunk_z), column);
        self.events
            .push(WorldEvent::ChunkColumnLoad(chunk_x, chunk_z));
    }

    /// Scan a column's non-empty sections for indexed block types and record their
    /// world positions. Runs once per chunk load; empty (all-air) sections are skipped.
    fn index_column(&mut self, cx: i32, cz: i32, column: &ChunkColumn) {
        if self.indexed_states.is_empty() {
            return;
        }
        for (si, section) in column.sections.iter().enumerate() {
            if section.is_empty() {
                continue;
            }
            let base_y = column.min_y + (si as i32) * 16;
            for ly in 0usize..16 {
                for lz in 0usize..16 {
                    for lx in 0usize..16 {
                        let s = section.get_block(lx, ly, lz);
                        if let Some(&bid) = self.indexed_states.get(&s) {
                            self.block_index.entry(bid).or_default().push((
                                cx * 16 + lx as i32,
                                base_y + ly as i32,
                                cz * 16 + lz as i32,
                            ));
                        }
                    }
                }
            }
        }
    }

    /// Drop all indexed positions inside chunk (cx, cz) — on unload/reload.
    fn deindex_column(&mut self, cx: i32, cz: i32) {
        for positions in self.block_index.values_mut() {
            positions.retain(|&(x, _, z)| (x >> 4) != cx || (z >> 4) != cz);
        }
    }

    /// Keep the index current when a single block changes (mine/place/fluid flow).
    fn index_update(&mut self, wp: (i32, i32, i32), old_state: u32, new_state: u32) {
        if old_state == new_state {
            return;
        }
        if let Some(&bid) = self.indexed_states.get(&old_state) {
            if let Some(v) = self.block_index.get_mut(&bid) {
                if let Some(i) = v.iter().position(|&p| p == wp) {
                    v.swap_remove(i);
                }
            }
        }
        if let Some(&bid) = self.indexed_states.get(&new_state) {
            self.block_index.entry(bid).or_default().push(wp);
        }
    }

    /// Nearest indexed block of `block_id` to `from` within `max_dist` — O(k), any range.
    pub fn nearest_indexed(
        &self,
        block_id: i32,
        from: (i32, i32, i32),
        max_dist: i32,
    ) -> Option<(i32, i32, i32)> {
        let positions = self.block_index.get(&block_id)?;
        let md2 = (max_dist as i64) * (max_dist as i64);
        positions
            .iter()
            .copied()
            .filter_map(|p| {
                let (dx, dy, dz) = ((p.0 - from.0) as i64, (p.1 - from.1) as i64, (p.2 - from.2) as i64);
                let d = dx * dx + dy * dy + dz * dz;
                (d <= md2).then_some((d, p))
            })
            .min_by_key(|(d, _)| *d)
            .map(|(_, p)| p)
    }

    /// Save (if a provider is queued) and unload a column.
    pub fn unload_column(&mut self, chunk_x: i32, chunk_z: i32) -> std::io::Result<()> {
        let key = (chunk_x, chunk_z);
        if !self.columns.contains_key(&key) {
            return Ok(());
        }
        if self.saving_queue.contains(&key) {
            if let Some(p) = &mut self.provider {
                let column = self.columns.get(&key).unwrap();
                p.save(chunk_x, chunk_z, column)?;
            }
            self.saving_queue.remove(&key);
        }
        self.deindex_column(chunk_x, chunk_z);
        self.columns.remove(&key);
        self.events
            .push(WorldEvent::ChunkColumnUnload(chunk_x, chunk_z));
        Ok(())
    }

    // ── Block access (world coordinates) ──

    fn column_at(&mut self, pos: Vec3) -> Option<&mut ChunkColumn> {
        self.columns.get_mut(&(to_chunk(pos.x), to_chunk(pos.z)))
    }

    pub fn get_block_state_id(&self, pos: Vec3) -> Option<u32> {
        let col = self.columns.get(&(to_chunk(pos.x), to_chunk(pos.z)))?;
        Some(col.get_block_state_id(to_local(pos.x), pos.y.floor() as i32, to_local(pos.z)))
    }

    pub fn set_block_state_id(&mut self, pos: Vec3, state_id: u32) {
        let (lx, y, lz) = (to_local(pos.x), pos.y.floor() as i32, to_local(pos.z));
        let (cx, cz) = (to_chunk(pos.x), to_chunk(pos.z));
        let old = match self.column_at(pos) {
            Some(col) => {
                let old = col.get_block_state_id(lx, y, lz);
                col.set_block_state_id(lx, y, lz, state_id);
                old
            }
            None => return,
        };
        self.index_update((pos.x.floor() as i32, y, pos.z.floor() as i32), old, state_id);
        self.queue_save(cx, cz);
        self.events.push(WorldEvent::BlockUpdate {
            pos,
            old_state: old,
            new_state: state_id,
        });
    }

    pub fn get_block(&self, pos: Vec3) -> Option<BlockInfo> {
        let col = self.columns.get(&(to_chunk(pos.x), to_chunk(pos.z)))?;
        Some(get_block(
            col,
            to_local(pos.x),
            pos.y.floor() as i32,
            to_local(pos.z),
            self.registry,
        ))
    }

    pub fn set_block(
        &mut self,
        pos: Vec3,
        name: &str,
        properties: Option<&HashMap<String, String>>,
    ) {
        let (lx, y, lz) = (to_local(pos.x), pos.y.floor() as i32, to_local(pos.z));
        let (cx, cz) = (to_chunk(pos.x), to_chunk(pos.z));
        let registry = self.registry;
        let (old, new) = match self.column_at(pos) {
            Some(col) => {
                let old = col.get_block_state_id(lx, y, lz);
                set_block(col, lx, y, lz, registry, name, properties);
                (old, col.get_block_state_id(lx, y, lz))
            }
            None => return,
        };
        self.index_update((pos.x.floor() as i32, y, pos.z.floor() as i32), old, new);
        self.queue_save(cx, cz);
        self.events.push(WorldEvent::BlockUpdate {
            pos,
            old_state: old,
            new_state: new,
        });
    }

    // ── Light / biome access ──

    col_accessor!(get_block_light, set_block_light, u32);
    col_accessor!(get_sky_light, set_sky_light, u32);
    col_accessor!(get_biome_id, set_biome_id, u32);

    // ── Saving ──

    fn queue_save(&mut self, chunk_x: i32, chunk_z: i32) {
        if self.provider.is_some() {
            self.saving_queue.insert((chunk_x, chunk_z));
        }
    }

    pub fn save_all(&mut self) -> std::io::Result<()> {
        let Some(provider) = self.provider.as_mut() else {
            return Ok(());
        };
        for &(cx, cz) in &self.saving_queue {
            if let Some(column) = self.columns.get(&(cx, cz)) {
                provider.save(cx, cz, column)?;
            }
        }
        self.saving_queue.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::ChunkColumnOptions;
    use crate::registry::{BlockCollisionShapes, Registry};
    use crate::vec3::vec3;

    fn registry() -> Registry {
        Registry::build(
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            BlockCollisionShapes::default(),
            std::collections::HashMap::new(),
            "26.1.2",
        )
    }

    fn column() -> ChunkColumn {
        ChunkColumn::new(ChunkColumnOptions {
            min_y: None,
            world_height: None,
            max_bits_per_block: 15,
            max_bits_per_biome: 7,
        })
    }

    #[test]
    fn block_access_and_events() {
        let reg = registry();
        let mut world = World::new(&reg);
        world.set_column(0, 0, column());
        // chunkColumnLoad event emitted
        let events = world.take_events();
        assert!(matches!(events[0], WorldEvent::ChunkColumnLoad(0, 0)));

        let pos = vec3(5.0, 10.0, 7.0);
        assert_eq!(world.get_block_state_id(pos), Some(0));
        world.set_block_state_id(pos, 42);
        assert_eq!(world.get_block_state_id(pos), Some(42));

        let events = world.take_events();
        assert!(matches!(
            events[0],
            WorldEvent::BlockUpdate {
                old_state: 0,
                new_state: 42,
                ..
            }
        ));
    }

    #[test]
    fn negative_coords_local_mapping() {
        let reg = registry();
        let mut world = World::new(&reg);
        world.set_column(-1, -1, column());
        let pos = vec3(-3.0, 64.0, -5.0);
        world.set_block_state_id(pos, 9);
        assert_eq!(world.get_block_state_id(pos), Some(9));
    }

    #[test]
    fn unloaded_column_returns_none() {
        let reg = registry();
        let world = World::new(&reg);
        assert_eq!(world.get_block_state_id(vec3(0.0, 0.0, 0.0)), None);
    }
}
