//! Biomes and ticking (issue 17b).
//!
//! **Biomes.** Bridger, 2026-11-24: the bot may know the biome at its own feet (as a player
//! knows it from the fog or F3) and biomes evidenced by blocks it has seen exposed (memory's
//! signs), never the biome of an unseen loaded column. So the only public read is
//! `World::feet_biome`, which takes the body, not a cell; `biome_at` stays private to the crate
//! and its tests.
//!
//! **Ticking.** An entity ticks only when its chunk is entity-ticking: a Chebyshev (square)
//! chunk distance of at most the simulation distance from the player's chunk (the 26.1.2 jar,
//! probe 45472254333: `DistanceManager`'s player ticket level 31 - simulation distance;
//! `ServerLevel` skips other entities, and `ChunkMap` sends no movement for them). A mob beyond
//! it is frozen: it neither moves nor despawns, and an enderman there is no use (issue 17).

use crate::{BlockPos, World};

/// The registry whose entry order a chunk's biome ids index.
pub const BIOME_REGISTRY: &str = "minecraft:worldgen/biome";

/// A cell's index in a section's 64-entry biome container (4×4×4 cells of 4 blocks): y, then z,
/// then x (`PalettedContainer` of `LevelChunkSection.biomes`, clojurecraft's `biome-index`).
fn biome_index(x: i32, y: i32, z: i32) -> usize {
    ((((y >> 2) & 3) << 4) | (((z >> 2) & 3) << 2) | ((x >> 2) & 3)) as usize
}

impl World {
    /// A biome id's name: the server's registry entry when it sent one, as the generated table
    /// spells it; else the generated table, sorted by name as the server syncs it.
    fn biome_name(&self, id: u32) -> Option<&'static str> {
        let table = &data::generated::registries::BIOME;
        let i = usize::try_from(id).ok()?;
        if self.biome_registry.is_empty() {
            return table.get(i).copied();
        }
        let name = self.biome_registry.get(i)?;
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        table.iter().copied().find(|n| *n == name)
    }

    /// The biome of the cell holding `(x, y, z)` in a loaded column. Private: the planner reads
    /// only its own feet (`feet_biome`).
    pub(crate) fn biome_at(&self, x: i32, y: i32, z: i32) -> Option<&'static str> {
        let column = self.columns.get(&BlockPos::new(x, y, z).chunk())?;
        let id = column.section_at(y)?.biomes.get(biome_index(x, y, z))?;
        self.biome_name(id)
    }

    /// The biome at the feet (`"warped_forest"`), `None` with no body or no column there.
    pub fn feet_biome(&self) -> Option<&'static str> {
        let b = self.body?;
        self.biome_at(
            b.pos[0].floor() as i32,
            b.pos[1].floor() as i32,
            b.pos[2].floor() as i32,
        )
    }
}

/// The chunk of a position.
fn chunk_of(pos: [f64; 3]) -> (i32, i32) {
    ((pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4)
}

/// Whether an entity at `pos` ticks: its chunk within the simulation distance of the player's
/// chunk on both axes. False with no body or no simulation distance known.
pub fn ticks(w: &World, pos: [f64; 3]) -> bool {
    let (Some(body), Some(sim)) = (w.body, w.simulation_distance) else {
        return false;
    };
    let (px, pz) = chunk_of([body.pos[0], body.pos[1], body.pos[2]]);
    let (ex, ez) = chunk_of(pos);
    (ex - px).abs().max((ez - pz).abs()) <= sim
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use protocol::S2c;
    use protocol::chunk::Column;

    use super::*;
    use crate::{ChunkPos, Event};

    /// The Nether biomes: every cell of a real Nether column must read as one of these. A wrong
    /// index order or table reads overworld names (the table is mostly overworld biomes).
    const NETHER: [&str; 5] = [
        "basalt_deltas",
        "crimson_forest",
        "nether_wastes",
        "soul_sand_valley",
        "warped_forest",
    ];

    fn nether_world() -> World {
        let mut w = World::new();
        w.step(&Event::Packet(S2c::Respawn {
            dimension_type: 3,
            dimension: "minecraft:the_nether".to_owned(),
        }));
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let stem = path.file_stem().unwrap().to_str().unwrap().to_owned();
            let Some((cx, cz)) = stem.strip_prefix("nether_").and_then(|r| r.split_once('_'))
            else {
                continue;
            };
            let at = ChunkPos {
                x: cx.parse().unwrap(),
                z: cz.parse().unwrap(),
            };
            let column = Column::decode(&std::fs::read(&path).unwrap()).unwrap();
            w.columns.insert(at, Arc::new(column));
        }
        w
    }

    /// Real Nether columns (clojurecraft run 43143461648's chunk bytes, `tests/fixtures`): every
    /// sampled cell's biome is a Nether biome, by the generated table and by a registry sent in
    /// the same order; a registry in another order names the same id otherwise.
    #[test]
    fn a_real_nether_column_reads_nether_biomes() {
        let mut w = nether_world();
        assert!(!w.columns.is_empty());
        let cells: Vec<(i32, i32, i32)> = w
            .columns
            .keys()
            .flat_map(|c| {
                (0..128).step_by(8).flat_map(move |y| {
                    [
                        (c.x * 16 + 3, y, c.z * 16 + 9),
                        (c.x * 16 + 14, y, c.z * 16),
                    ]
                })
            })
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        for (x, y, z) in &cells {
            let b = w.biome_at(*x, *y, *z).unwrap();
            assert!(NETHER.contains(&b), "{b} at {x},{y},{z}");
            seen.insert(b);
        }
        assert!(!seen.is_empty());
        let table = data::generated::registries::BIOME;
        w.biome_registry = table.iter().map(|n| format!("minecraft:{n}")).collect();
        for (x, y, z) in &cells {
            assert!(NETHER.contains(&w.biome_at(*x, *y, *z).unwrap()));
        }
        w.biome_registry.reverse();
        let (x, y, z) = cells[0];
        assert!(!NETHER.contains(&w.biome_at(x, y, z).unwrap_or("none")));
    }

    #[test]
    fn the_biome_index_is_y_then_z_then_x() {
        assert_eq!(biome_index(0, 0, 0), 0);
        assert_eq!(biome_index(4, 0, 0), 1);
        assert_eq!(biome_index(0, 0, 4), 4);
        assert_eq!(biome_index(0, 4, 0), 16);
        assert_eq!(biome_index(15, 15, 15), 63);
        assert_eq!(biome_index(-1, -1, -1), 63);
    }

    /// The `registry_data` packet keeps the biome names, in order.
    #[test]
    fn the_biome_registry_is_kept_from_registry_data() {
        let mut w = World::new();
        w.step(&Event::Packet(S2c::RegistryData {
            registry: "minecraft:dimension_type".to_owned(),
            entries: vec!["minecraft:overworld".to_owned()],
        }));
        assert!(w.biome_registry.is_empty());
        w.step(&Event::Packet(S2c::RegistryData {
            registry: BIOME_REGISTRY.to_owned(),
            entries: vec!["minecraft:warped_forest".to_owned()],
        }));
        assert_eq!(w.biome_registry, vec!["minecraft:warped_forest".to_owned()]);
        assert_eq!(w.biome_name(0), Some("warped_forest"));
        assert_eq!(w.biome_name(1), None);
    }

    /// A Nether column with a warped nylium floor at y 40 under air: the exposed top faces are
    /// signs, and with more than `FOREST_SIGNS` of them the column is a known forest, kept after
    /// the column is forgotten. Nylium under netherrack (buried) is never a sign.
    #[test]
    fn an_exposed_nylium_floor_makes_a_known_forest() {
        use protocol::chunk::{Column, Paletted, Section};
        let state = |k: data::BlockKind| k.info().map(|i| i.default_state).unwrap();
        let (nylium, rack) = (
            state(data::BlockKind::WarpedNylium),
            state(data::BlockKind::Netherrack),
        );
        let mut w = World::new();
        w.step(&Event::Packet(S2c::Respawn {
            dimension_type: 3,
            dimension: "minecraft:the_nether".to_owned(),
        }));
        let section = |cells: &dyn Fn(usize) -> u32| Section {
            block_count: 0,
            fluid_count: 0,
            blocks: Paletted::pack(&(0..4096).map(cells).collect::<Vec<_>>(), 4),
            biomes: Paletted::Single(0),
        };
        let air = section(&|_| 0);
        // Section 2 (y 32 to 47): y 32 to 39 netherrack, y 40 nylium, air above. Section 0
        // (y 0 to 15): nylium at y 5 under netherrack at y 6, buried.
        let floor = section(&|i| match i >> 8 {
            0..=7 => rack,
            8 => nylium,
            _ => 0,
        });
        let buried = section(&|i| match i >> 8 {
            5 => nylium,
            _ => rack,
        });
        let mut sections = vec![air; 16];
        sections[2] = floor;
        sections[0] = buried;
        w.step(&Event::Packet(S2c::LevelChunkWithLight {
            x: 3,
            z: -2,
            column: Arc::new(Column { min_y: 0, sections }),
        }));
        let signs = w.memory.biome_signs_in_column(3, -2);
        assert_eq!(signs.warped, memory::SIGNS_KEPT);
        assert_eq!(w.memory.forest_known(), Some((3, -2)));
        w.step(&Event::Packet(S2c::ForgetLevelChunk { x: 3, z: -2 }));
        assert_eq!(
            w.memory.forest_known(),
            Some((3, -2)),
            "kept after it unloads"
        );
    }
}
