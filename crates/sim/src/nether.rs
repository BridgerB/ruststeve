//! Nether stages for the model (docs/problems/nether-testbed.md): a world in
//! `minecraft:the_nether` built by hand, so a Nether crossing is tested closed loop in minutes
//! and not by a race. Not vanilla's generator: a netherrack floor and ceiling with bedrock at
//! the bottom and the roof, and what a test lays in it (lava lakes, soul sand, soul soil, magma,
//! gravel, nether bricks), encoded as real 26.1.2 chunk columns of the Nether's 16 sections.
//!
//! The model reads the dimension for its rules: Nether lava ticks every 10 and runs 7 blocks
//! (`fluid`); magma burns the feet (`life`).

use std::collections::BTreeMap;

use data::BlockKind;
use data::generated::dimensions::DIMENSION_TYPES;
use protocol::EncodeError;
use protocol::chunk::{Column, Paletted, Section};

use crate::{ModelColumn, ModelWorld};

/// The Nether's level name, as the play login and a respawn carry it.
pub const NETHER: &str = "minecraft:the_nether";
/// The Nether's dimension type in the generated table (`data/minecraft/dimension_type`).
const NETHER_TYPE: &str = "the_nether";
/// The top cell of the Nether's lava sea: its noise settings' `sea_level` is 32, so open cells
/// under y 32 generate as lava.
pub const LAVA_SEA: i32 = 31;
/// The bedrock roof: the Nether's logical height is 128, so its last built layer is y 127.
pub const ROOF: i32 = 127;

type Pos = (i32, i32, i32);
/// An inclusive range on one axis, either end first.
pub type Span = (i32, i32);

fn state(k: BlockKind) -> u32 {
    k.info().map_or(0, |i| i.default_state)
}

/// The Nether's index in the synced dimension-type registry: the registry is sent sorted by
/// name and the client reads the index (`world::Dimension`), 3 on 26.1.2.
pub fn nether_type() -> i32 {
    DIMENSION_TYPES
        .iter()
        .position(|d| d.name == NETHER_TYPE)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(0)
}

/// Sections of a Nether column, and its lowest y, from the generated dimension types.
fn shape() -> (i32, i32) {
    DIMENSION_TYPES
        .iter()
        .find(|d| d.name == NETHER_TYPE)
        .map_or((0, 16), |d| {
            (d.min_y, i32::try_from(d.sections).unwrap_or(16))
        })
}

impl ModelWorld {
    /// Whether lava here is the Nether's: `LavaFluid` reads the dimension for its tick delay,
    /// drop-off and slope search (`ultrawarm` before 1.21.11, the `fast_lava` attribute since),
    /// and of the vanilla dimension types only the Nether sets it.
    pub fn fast_lava(&self) -> bool {
        usize::try_from(self.dimension_type)
            .ok()
            .and_then(|i| DIMENSION_TYPES.get(i))
            .is_some_and(|d| d.name == NETHER_TYPE)
    }
}

/// A Nether stage being laid: an open cavern between a netherrack floor and ceiling, with the
/// cells a test sets over it. `build` encodes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Stage {
    /// The chunk columns held, inclusive: (lowest x, lowest z) and (highest x, highest z).
    columns: (Span, Span),
    /// The floor's top netherrack layer and the ceiling's lowest.
    floor: i32,
    ceiling: i32,
    cells: BTreeMap<Pos, u32>,
    spawn: [f64; 3],
}

fn ordered((a, b): Span) -> std::ops::RangeInclusive<i32> {
    a.min(b)..=a.max(b)
}

impl Stage {
    /// A cavern of `radius` chunk columns each way round the origin: netherrack up to `floor`,
    /// air to under `ceiling`, netherrack from `ceiling` to the bedrock roof. The player spawns
    /// on the floor at (0.5, floor + 1, 0.5). A `floor` at or over `ceiling` is solid netherrack.
    pub fn new(radius: i32, floor: i32, ceiling: i32) -> Self {
        Self {
            columns: ((-radius, radius), (-radius, radius)),
            floor,
            ceiling,
            cells: BTreeMap::new(),
            spawn: [0.5, f64::from(floor + 1), 0.5],
        }
    }

    /// The same cavern over the chunk columns that hold the block spans `x` and `z`.
    pub fn over(x: Span, z: Span, floor: i32, ceiling: i32) -> Self {
        let chunks = |s: Span| (s.0.min(s.1) >> 4, s.0.max(s.1) >> 4);
        Self {
            columns: (chunks(x), chunks(z)),
            floor,
            ceiling,
            cells: BTreeMap::new(),
            spawn: [
                f64::from(x.0.min(x.1)) + 0.5,
                f64::from(floor + 1),
                f64::from(z.0.min(z.1)) + 0.5,
            ],
        }
    }

    /// What the cavern holds at a height before anything is laid.
    fn base(&self, y: i32) -> BlockKind {
        if y == 0 || y == ROOF {
            BlockKind::Bedrock
        } else if y > ROOF {
            BlockKind::Air
        } else if y <= self.floor || y >= self.ceiling {
            BlockKind::Netherrack
        } else {
            BlockKind::Air
        }
    }

    /// The floor's top layer and the ceiling's lowest.
    pub fn floor(&self) -> i32 {
        self.floor
    }

    pub fn ceiling(&self) -> i32 {
        self.ceiling
    }

    /// One cell, by block state.
    pub fn set_state(&mut self, pos: Pos, state: u32) -> &mut Self {
        self.cells.insert(pos, state);
        self
    }

    /// One cell, the kind's default state.
    pub fn set(&mut self, pos: Pos, kind: BlockKind) -> &mut Self {
        self.set_state(pos, state(kind))
    }

    /// A box of one kind, each span inclusive.
    pub fn fill(&mut self, x: Span, y: Span, z: Span, kind: BlockKind) -> &mut Self {
        let s = state(kind);
        for cx in ordered(x) {
            for cy in ordered(y) {
                for cz in ordered(z) {
                    self.cells.insert((cx, cy, cz), s);
                }
            }
        }
        self
    }

    /// A lava lake: still sources over `x` and `z`, `depth` deep with its top cell at `top`.
    /// Sources at rest schedule nothing: the lake flows only once a cell beside it changes
    /// (`LiquidBlock.neighborChanged`), as a real one does when the bot digs its bank.
    pub fn lava_lake(&mut self, x: Span, z: Span, top: i32, depth: i32) -> &mut Self {
        self.fill(x, (top - depth.max(1) + 1, top), z, BlockKind::Lava)
    }

    /// One layer of soul sand (a 14/16 collision box: the walk's `Awkward`, never its floor).
    pub fn soul_sand(&mut self, x: Span, z: Span, y: i32) -> &mut Self {
        self.fill(x, (y, y), z, BlockKind::SoulSand)
    }

    /// One layer of soul soil (a full cube).
    pub fn soul_soil(&mut self, x: Span, z: Span, y: i32) -> &mut Self {
        self.fill(x, (y, y), z, BlockKind::SoulSoil)
    }

    /// One layer of magma blocks (they burn the feet: `life`).
    pub fn magma(&mut self, x: Span, z: Span, y: i32) -> &mut Self {
        self.fill(x, (y, y), z, BlockKind::MagmaBlock)
    }

    /// One layer of gravel (it falls when what is under it goes; the model does not drop it).
    pub fn gravel(&mut self, x: Span, z: Span, y: i32) -> &mut Self {
        self.fill(x, (y, y), z, BlockKind::Gravel)
    }

    /// A solid box of nether bricks: a fortress wall or pier, what `world::stage::at_fortress`
    /// is met by once seen with an exposed face.
    pub fn bricks(&mut self, x: Span, y: Span, z: Span) -> &mut Self {
        self.fill(x, y, z, BlockKind::NetherBricks)
    }

    /// A fortress bridge along x or z: a brick deck one thick at `y` over the spans, with two
    /// cells of air over it so it is walked and seen.
    pub fn bridge(&mut self, x: Span, z: Span, y: i32) -> &mut Self {
        self.fill(x, (y + 1, y + 2), z, BlockKind::Air);
        self.bricks(x, (y, y), z)
    }

    /// Open a box to air (a cave, a cut, the room a portal stands in).
    pub fn air(&mut self, x: Span, y: Span, z: Span) -> &mut Self {
        self.fill(x, y, z, BlockKind::Air)
    }

    /// Where the player spawns: feet at the centre of the cell (x, y, z).
    pub fn spawn(&mut self, (x, y, z): Pos) -> &mut Self {
        self.spawn = [f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5];
        self
    }

    /// The block state the stage holds at a cell.
    pub fn state_at(&self, pos: Pos) -> u32 {
        self.cells
            .get(&pos)
            .copied()
            .unwrap_or_else(|| state(self.base(pos.1)))
    }

    /// The stage as a model world in the Nether.
    pub fn build(&self) -> Result<ModelWorld, EncodeError> {
        let (min_y, sections) = shape();
        let air = state(BlockKind::Air);
        let fluid = |s: u32| crate::fluid::fluid_of(s).is_some();
        // The laid cells by section, so a section nothing is laid in is filled from its heights.
        let mut laid: BTreeMap<(i32, i32, i32), Vec<(usize, u32)>> = BTreeMap::new();
        for (&(x, y, z), &s) in &self.cells {
            laid.entry((x >> 4, z >> 4, (y - min_y) >> 4))
                .or_default()
                .push((protocol::chunk::block_index(x, y, z), s));
        }
        let ((x0, x1), (z0, z1)) = self.columns;
        let mut columns = BTreeMap::new();
        for cx in x0..=x1 {
            for cz in z0..=z1 {
                let built = (0..sections)
                    .map(|si| {
                        let base = min_y + 16 * si;
                        let mut cells: Vec<u32> = (0..4096i32)
                            .map(|i| state(self.base(base + (i >> 8))))
                            .collect();
                        for (i, s) in laid.get(&(cx, cz, si)).into_iter().flatten() {
                            if let Some(c) = cells.get_mut(*i) {
                                *c = *s;
                            }
                        }
                        let count = |pred: &dyn Fn(u32) -> bool| {
                            i16::try_from(cells.iter().filter(|c| pred(**c)).count())
                                .unwrap_or(i16::MAX)
                        };
                        Section {
                            block_count: count(&|c| c != air),
                            fluid_count: count(&fluid),
                            blocks: Paletted::pack(&cells, 4),
                            biomes: Paletted::Single(0),
                        }
                    })
                    .collect();
                let column = Column {
                    min_y,
                    sections: built,
                };
                columns.insert((cx, cz), ModelColumn::generated(cx, cz, column)?);
            }
        }
        Ok(ModelWorld {
            columns,
            spawn: self.spawn,
            dimension: NETHER.to_owned(),
            dimension_type: nether_type(),
        })
    }
}

/// An empty Nether cavern (`Stage::new`), built: `radius` chunk columns each way, netherrack to
/// `floor`, air to under `ceiling`. `flat_world`'s like in the Nether; a test lays more with
/// `ModelWorld::set_block`, or builds a `Stage` for anything large.
pub fn nether_world(radius: i32, floor: i32, ceiling: i32) -> Result<ModelWorld, EncodeError> {
    Stage::new(radius, floor, ceiling).build()
}

/// One wall of the Nether crossing laid as a stage (docs/problems/nether-testbed.md): where the
/// bot starts, the bricks it is to reach, and what is set once the model runs.
#[derive(Clone, Debug, PartialEq)]
pub struct Wall {
    pub name: &'static str,
    pub stage: Stage,
    /// The cell the bot's feet start in (on soul sand the body sinks an eighth into the cell
    /// under it).
    pub start: Pos,
    /// The nether-brick cell nearest the start.
    pub bricks: Pos,
    /// The blaze spawner, on the walls that have one.
    pub spawner: Option<Pos>,
    /// Blocks set with their updates before the first tick (`Sim::setblock`): lava put here
    /// runs, where a lake laid in the stage rests.
    pub wake: Vec<(Pos, BlockKind)>,
}

impl Wall {
    /// The wall's world, the player spawning at `start`.
    pub fn world(&self) -> Result<ModelWorld, EncodeError> {
        let mut stage = self.stage.clone();
        stage.spawn(self.start);
        stage.build()
    }

    /// A model of the wall, its `wake` blocks set.
    pub fn sim(&self, config: crate::Config) -> Result<crate::Sim, EncodeError> {
        let mut sim = crate::Sim::new(config, self.world()?);
        for (pos, kind) in &self.wake {
            sim.setblock(*pos, state(*kind))?;
        }
        Ok(sim)
    }
}

/// The walls of blaze walls 22 and 23 and the stop after them, each as small as it can be laid:
/// what the route (soul sand, lava beside the path, a bridge) and the tunnel (over the lava sea,
/// level to the bricks, sealed behind) must each get a bot through.
pub mod walls {
    use super::{BlockKind, LAVA_SEA, Stage, Wall};

    /// The floor of the open walls: two over the lava sea, as the portal's ledge is.
    const FLOOR: i32 = LAVA_SEA + 2;
    /// Their ceiling: a cavern 14 high.
    const CEILING: i32 = FLOOR + 15;

    /// A flat of soul sand from wall to wall, bricks 30 blocks east. Nothing is in the way but
    /// the floor itself: the walk holds soul sand `Awkward`, never a floor, so no search starts.
    pub fn soul_sand_flat() -> Wall {
        let mut stage = Stage::new(3, FLOOR, CEILING);
        stage.soul_sand((-48, 63), (-48, 63), FLOOR).bricks(
            (30, 32),
            (FLOOR + 1, FLOOR + 4),
            (-2, 2),
        );
        Wall {
            name: "a soul sand flat",
            stage,
            start: (0, FLOOR + 1, 0),
            bricks: (30, FLOOR + 1, 0),
            wake: Vec::new(),
            spawner: None,
        }
    }

    /// The same flat in netherrack: the control, walked today.
    pub fn netherrack_flat() -> Wall {
        let mut stage = Stage::new(3, FLOOR, CEILING);
        stage.bricks((30, 32), (FLOOR + 1, FLOOR + 4), (-2, 2));
        Wall {
            name: "a netherrack flat",
            stage,
            start: (0, FLOOR + 1, 0),
            bricks: (30, FLOOR + 1, 0),
            wake: Vec::new(),
            spawner: None,
        }
    }

    /// A netherrack flat cut from wall to wall by a lava moat 4 wide and 2 deep, 10 blocks
    /// east; bricks 6 past its far bank. There is no way round: over it (a bridge of held
    /// blocks) or under it.
    pub fn lava_moat() -> Wall {
        let mut stage = Stage::new(3, FLOOR, CEILING);
        stage.lava_lake((10, 13), (-48, 63), FLOOR, 2).bricks(
            (20, 22),
            (FLOOR + 1, FLOOR + 4),
            (-2, 2),
        );
        Wall {
            name: "a lava moat",
            stage,
            start: (0, FLOOR + 1, 0),
            bricks: (20, FLOOR + 1, 0),
            wake: Vec::new(),
            spawner: None,
        }
    }

    /// The tunnel's case: the bot on a shore at lava-sea level, a lake of the sea 49 wide from
    /// wall to wall before it, bricks 60 blocks east on the far shore, and four blocks of air
    /// over it all under a netherrack hill that goes up to the roof. Through the hill, over
    /// the lava, is rock all the way.
    pub fn lake_under_a_hill() -> Wall {
        let mut stage = Stage::new(4, LAVA_SEA, LAVA_SEA + 5);
        stage.lava_lake((6, 54), (-64, 79), LAVA_SEA, 3).bricks(
            (60, 62),
            (LAVA_SEA + 1, LAVA_SEA + 4),
            (-3, 3),
        );
        Wall {
            name: "a lava lake under a hill",
            stage,
            start: (0, LAVA_SEA + 1, 0),
            bricks: (60, LAVA_SEA + 1, 0),
            wake: Vec::new(),
            spawner: None,
        }
    }

    /// The seal's case, blaze wall 22 to scale: solid netherrack, the bot at the foot of its own
    /// one-wide stairs (three steps south and down from a two-high doorway), and a lava source
    /// set beside the doorway as the run begins. It reaches the cell beside the bot's head in
    /// about 50 ticks and its feet in 70. Bricks stand in a pocket 13 blocks on, the way the
    /// bot was digging.
    pub fn stair_cut() -> Wall {
        let mut stage = Stage::new(2, super::ROOF - 1, super::ROOF);
        let foot = LAVA_SEA;
        stage
            .air((0, 0), (foot + 3, foot + 4), (-3, -3))
            .air((0, 0), (foot + 2, foot + 4), (-2, -2))
            .air((0, 0), (foot + 1, foot + 3), (-1, -1))
            .air((0, 0), (foot, foot + 2), (0, 0))
            .air((0, 0), (foot, foot + 2), (12, 12))
            .bricks((0, 0), (foot, foot + 2), (13, 13));
        Wall {
            name: "a stair cut with lava at the door",
            stage,
            start: (0, foot, 0),
            bricks: (0, foot, 13),
            wake: vec![((0, foot + 3, -4), BlockKind::Lava)],
            spawner: None,
        }
    }

    /// Where the bridged walk of race 45337580375 ended, to scale: the bot on a shelf of
    /// cobblestone it laid at the foot of a brick pillar that stands in the lava sea, already
    /// "at the fortress" (within 6 of bricks). The fortress proper is up and away: the pillar
    /// carries a bridge 32 over the sea that runs 40 east and then 40 south to a platform with
    /// the blaze spawner (the real one was 68 east and 30 up). Nothing joins the shelf to the
    /// deck but the pillar's face and what the bot builds or cuts.
    pub fn pillar_in_the_sea() -> Wall {
        let sea = LAVA_SEA;
        let deck = sea + 32;
        let mut stage = Stage::over((-32, 79), (-32, 79), sea - 3, sea + 70);
        stage
            .lava_lake((-32, 79), (-32, 79), sea, 3)
            .bricks((0, 2), (sea - 3, deck), (0, 2))
            .fill((-3, -1), (sea, sea), (0, 2), BlockKind::Cobblestone)
            .bridge((0, 42), (0, 2), deck)
            .bridge((40, 42), (0, 44), deck)
            .bridge((37, 45), (42, 50), deck)
            .bricks((41, 41), (deck + 1, deck + 1), (46, 46))
            .set((41, deck + 2, 46), BlockKind::Spawner);
        Wall {
            name: "a brick pillar in the lava sea",
            stage,
            start: (-2, sea + 1, 1),
            bricks: (0, sea + 1, 1),
            wake: Vec::new(),
            spawner: Some((41, deck + 2, 46)),
        }
    }

    /// Every wall of the walk to the fortress, the control first (`pillar_in_the_sea` is the
    /// next leg's: it starts at the fortress).
    pub fn all() -> Vec<Wall> {
        vec![
            netherrack_flat(),
            soul_sand_flat(),
            lava_moat(),
            lake_under_a_hill(),
            stair_cut(),
        ]
    }
}
