//! The End for the model (issue 20h, docs/problems/end-model.md): a world in `minecraft:the_end`
//! laid by hand, so the End's paths (the bridge over the void, the pillar and strike, the dig
//! down) are tested closed loop in minutes and not by a race. Not vanilla's generator: void
//! everywhere, and on it what a layout asks for:
//! - the arrival platform (`EndPlatformFeature`: obsidian 5 by 5 at y 48 round (100, 0), three
//!   cells of air over it; the player arrives at (100.5, 49, 0.5));
//! - the main island as an end stone disc (vanilla's is noise; a test sets its radius and top);
//! - spikes as `SpikeFeature` lays them (obsidian under `height` within the radius, bedrock at
//!   `height` on the centre, the crystal at `height + 1`, a cage of iron bars from `height` up 4
//!   when guarded, each bar's sides as worldgen sets them);
//! - the fountain (`EndPodiumFeature`, simplified: a bedrock floor of radius 3 one over the
//!   island's top, a rim of radius 3 to 3.5 a cell higher, the pillar 4 high on the centre).
//!
//! The crystals' rules live here too: any hit ends one, and it explodes with power 6
//! (`EndCrystal.hurtServer`); the blast's damage to the player follows `ServerExplosion`'s
//! exposure and distance, cut by armour. Blocks the blast would break are not modelled. The
//! dragon's flight is not modelled at all: the fight is judged real-only.

use std::collections::BTreeMap;

use data::generated::dimensions::DIMENSION_TYPES;
use data::{BlockKind, EntityKind};
use protocol::chunk::{Column, Paletted, Section};
use protocol::{EncodeError, S2c, State};

use crate::{ModelColumn, ModelWorld, Sim, Wire, wire};

/// The End's level name, as the play login and a respawn carry it.
pub const THE_END: &str = "minecraft:the_end";
/// Its dimension type and biome in the generated tables.
const END_TYPE: &str = "the_end";
const END_BIOME: &str = "the_end";
/// Where a player arriving through a portal stands (`ServerLevel.END_SPAWN_POINT` (100, 50, 0)
/// with the platform at y 48 under it: the feet at 49, the cell's middle).
pub const ARRIVAL: [f64; 3] = [100.5, 49.0, 0.5];
/// The platform's centre cell and its half side (`EndPlatformFeature`: 5 by 5).
pub const PLATFORM: (i32, i32, i32) = (100, 48, 0);
const PLATFORM_HALF: i32 = 2;
/// An end crystal's box (`EntityType.END_CRYSTAL`: 2.0 by 2.0) and its blast
/// (`EndCrystal.hurtServer`: `explode(..., 6.0F, ...)`).
pub const CRYSTAL_HALF: f64 = 1.0;
pub const CRYSTAL_HEIGHT: f64 = 2.0;
pub const CRYSTAL_POWER: f64 = 6.0;
/// The player's box (0.6 by 1.8) and eye.
const PLAYER_HALF: f64 = 0.3;
const PLAYER_HEIGHT: f64 = 1.8;
/// A ray is marched in steps this long against the cells' shapes (an iron bar's post is 2/16
/// thick: a step under half of that never jumps it).
const RAY_STEP: f64 = 0.05;

type Pos = (i32, i32, i32);

fn state(k: BlockKind) -> u32 {
    k.info().map_or(0, |i| i.default_state)
}

/// The End's index in the synced dimension-type registry (sorted by name).
pub fn end_type() -> i32 {
    DIMENSION_TYPES
        .iter()
        .position(|d| d.name == END_TYPE)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(0)
}

/// The End column's lowest y and its sections, from the generated dimension types.
fn shape() -> (i32, i32) {
    DIMENSION_TYPES
        .iter()
        .find(|d| d.name == END_TYPE)
        .map_or((0, 16), |d| {
            (d.min_y, i32::try_from(d.sections).unwrap_or(16))
        })
}

/// One obsidian spike as `SpikeFeature` lays it: its centre column, radius, `height` (the
/// bedrock's y; the obsidian ends under it) and whether it is caged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spike {
    pub centre: [i32; 2],
    pub radius: i32,
    pub height: i32,
    pub caged: bool,
}

impl Spike {
    /// Where its crystal stands: the centre of the cell over the bedrock.
    pub fn crystal(&self) -> [f64; 3] {
        [
            f64::from(self.centre[0]) + 0.5,
            f64::from(self.height + 1),
            f64::from(self.centre[1]) + 0.5,
        ]
    }
}

/// What a synthetic End holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// The island's end stone disc: its radius round (0, 0), its top y and how deep it goes.
    pub island_radius: i32,
    pub island_top: i32,
    pub island_depth: i32,
    pub spikes: Vec<Spike>,
    /// The arrival platform at `PLATFORM`.
    pub platform: bool,
    /// The fountain on the island's centre.
    pub fountain: bool,
}

impl Layout {
    /// A small End for tests: the platform, a void gap of 20 to an island of radius 80 is too
    /// far for a quick test, so the island here reaches x 78 (a gap of 20 from the platform's
    /// edge at 98), top y 60; one open spike and one caged, the fountain.
    pub fn small() -> Self {
        Self {
            island_radius: 78,
            island_top: 60,
            island_depth: 4,
            spikes: vec![
                Spike {
                    centre: [42, 0],
                    radius: 3,
                    height: 76,
                    caged: false,
                },
                Spike {
                    centre: [0, 42],
                    radius: 2,
                    height: 79,
                    caged: true,
                },
            ],
            platform: true,
            fountain: true,
        }
    }
}

/// An End being laid: void, with the cells a layout and a test set. `build` encodes it.
#[derive(Clone, Debug, PartialEq)]
pub struct EndStage {
    /// The chunk columns held, inclusive: (lowest x, highest x), (lowest z, highest z).
    columns: ((i32, i32), (i32, i32)),
    cells: BTreeMap<Pos, u32>,
    spawn: [f64; 3],
}

impl EndStage {
    /// Void over the chunk columns holding the block spans `x` and `z`; the player arrives on
    /// the platform (`ARRIVAL`).
    pub fn new(x: (i32, i32), z: (i32, i32)) -> Self {
        let chunks = |s: (i32, i32)| (s.0.min(s.1) >> 4, s.0.max(s.1) >> 4);
        Self {
            columns: (chunks(x), chunks(z)),
            cells: BTreeMap::new(),
            spawn: ARRIVAL,
        }
    }

    /// A layout's End: the columns from the island's west edge to past the platform.
    pub fn of(layout: &Layout) -> Self {
        let r = layout.island_radius.max(8);
        let mut s = Self::new((-r, PLATFORM.0 + 8), (-r, r));
        if layout.platform {
            s.platform();
        }
        s.island(layout.island_radius, layout.island_top, layout.island_depth);
        if layout.fountain {
            // Flush with the island's top, as the jar lays it (config/end.toml `[fountain]`:
            // the bowl's floor y 62, the end stone top at (0, 0) 62).
            s.fountain(layout.island_top);
        }
        for spike in &layout.spikes {
            s.spike(spike);
        }
        s
    }

    pub fn set_state(&mut self, pos: Pos, st: u32) -> &mut Self {
        self.cells.insert(pos, st);
        self
    }

    pub fn set(&mut self, pos: Pos, kind: BlockKind) -> &mut Self {
        self.set_state(pos, state(kind))
    }

    /// A box of one kind, each span inclusive.
    pub fn fill(
        &mut self,
        x: (i32, i32),
        y: (i32, i32),
        z: (i32, i32),
        kind: BlockKind,
    ) -> &mut Self {
        let s = state(kind);
        let r = |(a, b): (i32, i32)| a.min(b)..=a.max(b);
        for cx in r(x) {
            for cy in r(y) {
                for cz in r(z) {
                    self.cells.insert((cx, cy, cz), s);
                }
            }
        }
        self
    }

    /// Where the player's feet start: the middle of cell (x, y, z).
    pub fn spawn(&mut self, (x, y, z): Pos) -> &mut Self {
        self.spawn = [f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5];
        self
    }

    /// `EndPlatformFeature`: obsidian 5 by 5 at the platform's height, air three over it.
    pub fn platform(&mut self) -> &mut Self {
        let (x, y, z) = PLATFORM;
        let h = PLATFORM_HALF;
        self.fill(
            (x - h, x + h),
            (y + 1, y + 3),
            (z - h, z + h),
            BlockKind::Air,
        );
        self.fill((x - h, x + h), (y, y), (z - h, z + h), BlockKind::Obsidian)
    }

    /// The island: end stone within `radius` of (0, 0) on the plane, from `top` down `depth`.
    pub fn island(&mut self, radius: i32, top: i32, depth: i32) -> &mut Self {
        let s = state(BlockKind::EndStone);
        for x in -radius..=radius {
            for z in -radius..=radius {
                if x * x + z * z > radius * radius {
                    continue;
                }
                for y in (top - depth.max(1) + 1)..=top {
                    self.cells.insert((x, y, z), s);
                }
            }
        }
        self
    }

    /// The fountain, simplified from `EndPodiumFeature`: a bedrock floor within 3 at `floor`, a
    /// rim from 3 to 3.5 one higher, the pillar on the centre four high over the floor.
    pub fn fountain(&mut self, floor: i32) -> &mut Self {
        for x in -4..=4i32 {
            for z in -4..=4i32 {
                let d2 = x * x + z * z;
                if d2 <= 9 {
                    self.set((x, floor, z), BlockKind::Bedrock);
                } else if d2 <= 12 {
                    self.set((x, floor, z), BlockKind::Bedrock);
                    self.set((x, floor + 1, z), BlockKind::Bedrock);
                }
            }
        }
        self.fill((0, 0), (floor, floor + 4), (0, 0), BlockKind::Bedrock)
    }

    /// `SpikeFeature.placeSpike`: obsidian under `height` within the radius (distance squared
    /// up to r² + 1), bedrock at `height` on the centre, and a guarded spike's cage of iron bars.
    pub fn spike(&mut self, sp: &Spike) -> &mut Self {
        let [cx, cz] = sp.centre;
        let r = sp.radius;
        let obsidian = state(BlockKind::Obsidian);
        for x in (cx - r)..=(cx + r) {
            for z in (cz - r)..=(cz + r) {
                let (dx, dz) = (x - cx, z - cz);
                if dx * dx + dz * dz > r * r + 1 {
                    continue;
                }
                for y in 0..sp.height {
                    self.cells.insert((x, y, z), obsidian);
                }
            }
        }
        if sp.caged {
            for (pos, st) in cage(sp) {
                self.cells.insert(pos, st);
            }
        }
        self.set((cx, sp.height, cz), BlockKind::Bedrock)
    }

    /// The stage as a model world in the End.
    pub fn build(&self) -> Result<ModelWorld, EncodeError> {
        let (min_y, sections) = shape();
        let air = state(BlockKind::Air);
        let biome = crate::nether::biome_id(END_BIOME).unwrap_or(0);
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
                        let mut cells = vec![air; 4096];
                        for (i, s) in laid.get(&(cx, cz, si)).into_iter().flatten() {
                            if let Some(c) = cells.get_mut(*i) {
                                *c = *s;
                            }
                        }
                        let count = cells.iter().filter(|c| **c != air).count();
                        Section {
                            block_count: i16::try_from(count).unwrap_or(i16::MAX),
                            fluid_count: 0,
                            blocks: Paletted::pack(&cells, 4),
                            biomes: Paletted::Single(biome),
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
            dimension: THE_END.to_owned(),
            dimension_type: end_type(),
        })
    }
}

/// A guarded spike's cage (`SpikeFeature`): for x', z' in -2..=2 round the centre and y' in
/// 0..=3 over `height`, the cells on an edge or the roof (y' 3), each bar joined as worldgen
/// joins it: north while on an x edge or the roof and not z' -2, south not z' 2, west while on a
/// z edge or the roof and not x' -2, east not x' 2.
pub fn cage(sp: &Spike) -> Vec<(Pos, u32)> {
    let base = state(BlockKind::IronBars);
    let with = |st: u32, k: &str, on: bool| {
        crate::survival::with_property(st, k, if on { "true" } else { "false" }).unwrap_or(st)
    };
    let mut out = Vec::new();
    for xp in -2..=2i32 {
        for zp in -2..=2i32 {
            for yp in 0..=3i32 {
                let (xe, ze, top) = (xp.abs() == 2, zp.abs() == 2, yp == 3);
                if !(xe || ze || top) {
                    continue;
                }
                let (bx, bz) = (xe || top, ze || top);
                let st = with(base, "north", bx && zp != -2);
                let st = with(st, "south", bx && zp != 2);
                let st = with(st, "west", bz && xp != -2);
                let st = with(st, "east", bz && xp != 2);
                out.push(((sp.centre[0] + xp, sp.height + yp, sp.centre[1] + zp), st));
            }
        }
    }
    out
}

/// The boxes a block's shape holds for a blast's ray, in the cell's own frame: a pane's post
/// and arms (iron bars, from its sides), else the collision the model's physics reads.
fn ray_boxes(st: u32) -> Vec<[f64; 6]> {
    let kind = data::block_of_state(st);
    if kind == Some(BlockKind::IronBars) {
        let on = |k: &str| crate::survival::property(st, k) == Some("true");
        let (a, b) = (7.0 / 16.0, 9.0 / 16.0);
        let mut v = vec![[a, 0.0, a, b, 1.0, b]];
        if on("north") {
            v.push([a, 0.0, 0.0, b, 1.0, a]);
        }
        if on("south") {
            v.push([a, 0.0, b, b, 1.0, 1.0]);
        }
        if on("west") {
            v.push([0.0, 0.0, a, a, 1.0, b]);
        }
        if on("east") {
            v.push([b, 0.0, a, 1.0, 1.0, b]);
        }
        return v;
    }
    use data::types::Collision;
    match kind.and_then(data::generated::shapes::collision) {
        Some(Collision::Empty) => Vec::new(),
        Some(Collision::Partial { top }) => vec![[0.0, 0.0, 0.0, 1.0, top, 1.0]],
        Some(Collision::Full) | None => vec![[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]],
    }
}

impl Sim {
    /// Whether a blast's ray from `from` to `to` reaches it with no block's shape between
    /// (`level.clip` with the collider shapes, marched in `RAY_STEP`s).
    pub fn ray_clear(&self, from: [f64; 3], to: [f64; 3]) -> bool {
        let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let n = (len / RAY_STEP).ceil().max(1.0) as u32;
        (1..n).all(|i| {
            let t = f64::from(i) / f64::from(n);
            let p = [from[0] + d[0] * t, from[1] + d[1] * t, from[2] + d[2] * t];
            let (cx, cy, cz) = (p[0].floor(), p[1].floor(), p[2].floor());
            let Some(st) = self.state_at(cx as i32, cy as i32, cz as i32) else {
                return true;
            };
            let l = [p[0] - cx, p[1] - cy, p[2] - cz];
            !ray_boxes(st)
                .iter()
                .any(|b| (0..3).all(|a| l[a] >= b[a] && l[a] <= b[a + 3]))
        })
    }

    /// `ServerExplosion.getSeenPercent`: the share of the sample points spread over the
    /// player's box whose ray to the blast's centre no block's shape cuts.
    pub fn seen_percent(&self, centre: [f64; 3]) -> f64 {
        let lo = [
            self.pos[0] - PLAYER_HALF,
            self.pos[1],
            self.pos[2] - PLAYER_HALF,
        ];
        let size = [PLAYER_HALF * 2.0, PLAYER_HEIGHT, PLAYER_HALF * 2.0];
        let step = size.map(|s| 1.0 / (s * 2.0 + 1.0));
        let off = |i: usize| (1.0 - (1.0 / step[i]).floor() * step[i]) / 2.0;
        let (ox, oz) = (off(0), off(2));
        let mut seen = 0u32;
        let mut all = 0u32;
        let mut fx = 0.0;
        while fx <= 1.0 {
            let mut fy = 0.0;
            while fy <= 1.0 {
                let mut fz = 0.0;
                while fz <= 1.0 {
                    let p = [
                        lo[0] + size[0] * fx + ox,
                        lo[1] + size[1] * fy,
                        lo[2] + size[2] * fz + oz,
                    ];
                    all += 1;
                    if self.ray_clear(p, centre) {
                        seen += 1;
                    }
                    fz += step[2];
                }
                fy += step[1];
            }
            fx += step[0];
        }
        if all == 0 {
            0.0
        } else {
            f64::from(seen) / f64::from(all)
        }
    }

    /// A crystal summoned at `pos` (its feet): a mob of no brain the model ends on any hit.
    pub fn summon_crystal(&mut self, pos: [f64; 3]) -> i32 {
        let id = self.summon(EntityKind::EndCrystal, pos);
        if let Some(m) = self.survival.life.mobs.get_mut(&id) {
            m.loot = false;
        }
        id
    }

    /// A strike on an end crystal (`EndCrystal.hurtServer`): in reach of its 2 by 2 box, any
    /// damage removes it and it explodes with power 6 at its feet. The blast's damage to the
    /// player (`ServerExplosion.hurtEntities`): with `d` the feet's distance over twice the
    /// power and `seen` the exposure, impact = (1 - d) × seen and damage = (impact² + impact) / 2
    /// × 7 × 12 + 1 (so a fully hidden player within 12 still takes 1), cut by armour; the
    /// knockback (1 - d) × seen along the eye's line from the centre.
    pub(crate) fn strike_crystal(&mut self, entity: i32) -> Result<Vec<Wire>, EncodeError> {
        let eye = [self.pos[0], self.pos[1] + physics::EYE_HEIGHT, self.pos[2]];
        let Some(at) = self
            .survival
            .life
            .mobs
            .get(&entity)
            .filter(|m| m.dead_at.is_none())
            .map(|m| m.pos)
        else {
            return Ok(Vec::new());
        };
        let axis = |e: f64, lo: f64, hi: f64| (lo - e).max(0.0).max(e - hi);
        let gap = [
            axis(eye[0], at[0] - CRYSTAL_HALF, at[0] + CRYSTAL_HALF),
            axis(eye[1], at[1], at[1] + CRYSTAL_HEIGHT),
            axis(eye[2], at[2] - CRYSTAL_HALF, at[2] + CRYSTAL_HALF),
        ];
        if gap.iter().map(|v| v * v).sum::<f64>() > crate::life::MELEE_REACH.powi(2) {
            self.violations
                .push(format!("attack {entity} out of reach"));
            return Ok(Vec::new());
        }
        if let Some(m) = self.survival.life.mobs.get_mut(&entity) {
            m.dead_at = Some(self.ticks);
        }
        self.survival.life.crystals_struck += 1;
        let mut out = vec![wire(
            State::Play,
            &S2c::RemoveEntities { ids: vec![entity] },
        )?];
        let r = CRYSTAL_POWER * 2.0;
        let feet = self.pos;
        let dist = (0..3)
            .map(|i| (feet[i] - at[i]).powi(2))
            .sum::<f64>()
            .sqrt()
            / r;
        if dist > 1.0 {
            out.push(wire(
                State::Play,
                &S2c::Explode {
                    center: at,
                    knockback: None,
                },
            )?);
            return Ok(out);
        }
        let seen = self.seen_percent(at);
        let impact = (1.0 - dist) * seen;
        let damage = ((impact * impact + impact) / 2.0 * 7.0 * r + 1.0) as f32;
        let dir = [eye[0] - at[0], eye[1] - at[1], eye[2] - at[2]];
        let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        let knock = (len > 0.0).then(|| dir.map(|v| v / len * impact));
        out.push(wire(
            State::Play,
            &S2c::Explode {
                center: at,
                knockback: knock,
            },
        )?);
        self.survival.life.blasts.push((self.ticks, seen, damage));
        let dealt = self.after_armor(damage);
        out.extend(self.hurt(dealt, "player_explosion", "death.attack.explosion.player")?);
        Ok(out)
    }
}

// The dragon (issue 20h), as config/end.toml's `[dragon]` reads the jar.
/// `EnderDragon`'s max health and the share of it taken while sitting that ends a perch
/// (`AbstractDragonSittingPhase`: 0.25 of 200).
pub const DRAGON_HEALTH: f32 = 200.0;
pub const SITTING_SHARE: f32 = 0.25;
/// Its parts are its id + 1 to + 8 (head, neck, body, three tails, two wings).
pub const DRAGON_PARTS: i32 = 8;
/// Part offsets from its feet along its facing (sin yaw, -cos yaw), perched: the head 6.5 out and
/// 1 down, 1 by 1; the neck 5.5 out, 3 by 3 (its height taken as the head's: the table gives
/// none); the body 0.5 out, 5 wide and 3 high. The dragon's own box is 16 by 8.
const HEAD_OUT: f64 = 6.5;
const HEAD_DOWN: f64 = 1.0;
const HEAD_BOX: f64 = 1.0;
const NECK_OUT: f64 = 5.5;
const NECK_BOX: f64 = 3.0;
const BODY_OUT: f64 = 0.5;
const BODY_WIDE: f64 = 5.0;
const BODY_HIGH: f64 = 3.0;
const DRAGON_WIDE: f64 = 16.0;
const DRAGON_HIGH: f64 = 8.0;
/// The head's and neck's blow: 10 (`mobAttack`) to a player in their boxes grown by 1, while the
/// dragon was not hurt in the last 10 ticks (its `hurtTime`).
const BLOW: f32 = 10.0;
const BLOW_GROW: f64 = 1.0;
const HURT_TIME: u64 = 10;
/// `EnderDragonPhase` ids: 0 holding pattern, 4 takeoff, 6 sitting scanning, 9 dying.
pub const PHASE_HOLDING: i32 = 0;
pub const PHASE_SITTING: i32 = 6;
pub const PHASE_DYING: i32 = 9;
/// The dragon's phase is its metadata index 16, its health index 9.
const PHASE_INDEX: u8 = 16;
const HEALTH_INDEX: u8 = 9;
/// `tickDeath`: 200 ticks from the death to the removal and the exit portal.
pub const DEATH_TICKS: u64 = 200;
/// The model's flight between perches: not the AI's circling (minutes over the spikes); it
/// leaves for this long, 40 up and 40 out over the island, then sits again.
pub const FLY_TICKS: u64 = 400;
const FLY_UP: f64 = 40.0;
const FLY_OUT: f64 = 40.0;
/// The exit portal fills the basin within 2.5 of the centre at the floor's height plus one
/// (`EndPodiumFeature`, active), the pillar's column left.
const PORTAL_RADIUS2: i32 = 6;
/// `win_game`'s game event, and its parameter for a first win (the credits are shown).
const WIN_GAME: u8 = 4;

/// The dragon as the model plays it: scripted, not `EnderDragon`'s AI. It sits on the fountain's
/// pillar facing `yaw` (sitting scanning), takes a player's hits by the part struck (the head
/// the blow, any other part blow / 4 + min(blow, 1)), and blows a player in its head's or neck's
/// box. A perch ends once a quarter of its health is taken sitting: it flies `FLY_TICKS` and sits
/// again. At 0 it dies (entity event 3, phase dying), and `DEATH_TICKS` later it is removed and the
/// exit portal fills the basin. A player whose feet enter it is sent `win_game`, and its
/// `client_command` 0 after the credits is counted. Not modelled: its flight paths, breath,
/// wings, charges, the crystals' heal, and the overworld the respawn goes to.
#[derive(Clone, Debug, PartialEq)]
pub struct Dragon {
    pub id: i32,
    pub yaw: f32,
    pub phase: i32,
    /// Its feet on the pillar's top.
    pub perch: [f64; 3],
    /// The basin's floor (the fountain's `floor`): the portal goes in one over it.
    pub floor: i32,
    pub sitting_damage: f32,
    /// While it flies, the tick it sits again.
    pub lands_at: Option<u64>,
    pub hurt_at: Option<u64>,
    pub died_at: Option<u64>,
    pub portal_lit: bool,
    pub won_at: Option<u64>,
    pub respawns_asked: u32,
    pub perches: u32,
    /// Hits that landed: (tick, part, damage dealt), part 0 the dragon's own box.
    pub hits: Vec<(u64, i32, f32)>,
    pub blows: u32,
}

/// The dragon's feet on a fountain laid at `floor` (`EndStage::fountain`): on its pillar's top.
pub fn perch_of(floor: i32) -> [f64; 3] {
    [0.5, f64::from(floor + 5), 0.5]
}

/// A box (min x, y, z, max x, y, z) `wide` across and `high` from `y` centred on (x, z).
fn part_box(x: f64, y: f64, z: f64, wide: f64, high: f64) -> [f64; 6] {
    let h = wide / 2.0;
    [x - h, y, z - h, x + h, y + high, z + h]
}

/// The eye's distance to a box.
fn reach(eye: [f64; 3], b: [f64; 6]) -> f64 {
    (0..3)
        .map(|i| (b[i] - eye[i]).max(0.0).max(eye[i] - b[i + 3]).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn overlap(a: [f64; 6], b: [f64; 6]) -> bool {
    (0..3).all(|i| a[i] < b[i + 3] && b[i] < a[i + 3])
}

impl Dragon {
    /// Where its feet are now.
    fn pos(&self, mob: [f64; 3]) -> [f64; 3] {
        mob
    }

    /// The box of part `part` (0 its own box, 1 the head, 2 the neck, 3 the body) with its feet
    /// at `at`; `None` for a part the model does not place (tails, wings).
    pub fn part(&self, part: i32, at: [f64; 3]) -> Option<[f64; 6]> {
        let r = f64::from(self.yaw).to_radians();
        let (fx, fz) = (r.sin(), -r.cos());
        let along = |d: f64| (at[0] + fx * d, at[2] + fz * d);
        match part {
            0 => Some(part_box(at[0], at[1], at[2], DRAGON_WIDE, DRAGON_HIGH)),
            1 => {
                let (x, z) = along(HEAD_OUT);
                Some(part_box(x, at[1] - HEAD_DOWN, z, HEAD_BOX, HEAD_BOX))
            }
            2 => {
                let (x, z) = along(NECK_OUT);
                Some(part_box(x, at[1] - HEAD_DOWN, z, NECK_BOX, NECK_BOX))
            }
            3 => {
                let (x, z) = along(BODY_OUT);
                Some(part_box(x, at[1], z, BODY_WIDE, BODY_HIGH))
            }
            _ => None,
        }
    }

    fn sitting(&self) -> bool {
        self.lands_at.is_none() && self.died_at.is_none()
    }
}

/// `TeleportEntity`, written here (the protocol crate decodes it and encodes no entity moves):
/// the id, the position, no velocity, the yaw and pitch, absolute flags, on the ground.
fn teleport_entity(id: i32, at: [f64; 3], yaw: f32) -> Wire {
    let mut w = protocol::bytes::Writer::new();
    w.varint(id);
    for v in at {
        w.f64(v);
    }
    for _ in 0..3 {
        w.f64(0.0);
    }
    w.f32(yaw);
    w.f32(0.0);
    w.i32(0);
    w.bool(false);
    Wire {
        state: State::Play,
        id: data::generated::packets::PlayClientbound::TeleportEntity.id(),
        body: std::sync::Arc::new(w.buf),
    }
}

impl Sim {
    /// The dragon summoned perched on the pillar of a fountain laid at `floor`, facing `yaw`,
    /// sitting scanning, full health, its parts' ids kept.
    pub fn summon_dragon(&mut self, floor: i32, yaw: f32) -> i32 {
        let perch = perch_of(floor);
        let id = self.summon(EntityKind::EnderDragon, perch);
        for _ in 0..DRAGON_PARTS {
            self.survival.next_id();
        }
        if let Some(m) = self.survival.life.mobs.get_mut(&id) {
            m.loot = false;
            m.health = DRAGON_HEALTH;
        }
        self.dragon = Some(Dragon {
            id,
            yaw,
            phase: PHASE_SITTING,
            perch,
            floor,
            sitting_damage: 0.0,
            lands_at: None,
            hurt_at: None,
            died_at: None,
            portal_lit: false,
            won_at: None,
            respawns_asked: 0,
            perches: 1,
            hits: Vec::new(),
            blows: 0,
        });
        id
    }

    /// The part of the dragon an attacked id is (0 the dragon itself, 1 to 8 its parts).
    pub(crate) fn dragon_part(&self, entity: i32) -> Option<i32> {
        let d = self.dragon.as_ref()?;
        (d.id..=d.id + DRAGON_PARTS)
            .contains(&entity)
            .then_some(entity - d.id)
    }

    fn dragon_data(&self) -> Result<Vec<Wire>, EncodeError> {
        let Some(d) = &self.dragon else {
            return Ok(Vec::new());
        };
        let Some(m) = self.survival.life.mobs.get(&d.id) else {
            return Ok(Vec::new());
        };
        Ok(vec![
            wire(
                State::Play,
                &S2c::SetEntityData {
                    id: d.id,
                    entries: vec![
                        (HEALTH_INDEX, protocol::metadata::MetaValue::Float(m.health)),
                        (PHASE_INDEX, protocol::metadata::MetaValue::Int(d.phase)),
                    ],
                },
            )?,
            teleport_entity(d.id, d.pos(m.pos), d.yaw),
        ])
    }

    /// At the join, after its spawn: its phase, health and yaw.
    pub(crate) fn dragon_joined(&self) -> Result<Vec<Wire>, EncodeError> {
        self.dragon_data()
    }

    /// A player's hit on the dragon or a part, in reach of that part's box.
    pub(crate) fn strike_dragon(&mut self, entity: i32) -> Result<Vec<Wire>, EncodeError> {
        let (Some(part), Some(d)) = (self.dragon_part(entity), self.dragon.clone()) else {
            return Ok(Vec::new());
        };
        let Some(at) = self
            .survival
            .life
            .mobs
            .get(&d.id)
            .filter(|m| m.dead_at.is_none() && d.died_at.is_none())
            .map(|m| m.pos)
        else {
            return Ok(Vec::new());
        };
        let Some(b) = d.part(part, at) else {
            self.violations
                .push(format!("attack on dragon part {part}, not modelled"));
            return Ok(Vec::new());
        };
        let eye = [self.pos[0], self.pos[1] + physics::EYE_HEIGHT, self.pos[2]];
        if reach(eye, b) > crate::life::MELEE_REACH {
            self.violations
                .push(format!("attack on dragon part {part} out of reach"));
            return Ok(Vec::new());
        }
        let swing = self.swing_damage();
        let life = &mut self.survival.life;
        life.attack_ticker = 0;
        life.exhaustion += crate::life::EXHAUSTION_ATTACK;
        // `EnderDragon.hurt(part)`: the head takes the blow, any other part a quarter and one.
        let damage = if part == 1 {
            swing
        } else {
            swing / 4.0 + swing.min(1.0)
        };
        let Some(m) = life.mobs.get_mut(&d.id) else {
            return Ok(Vec::new());
        };
        if damage < 0.01 {
            return Ok(Vec::new());
        }
        let dealt = if m.invulnerable > crate::life::INVULNERABLE_HALF {
            if damage <= m.last_hurt {
                return Ok(Vec::new());
            }
            damage - m.last_hurt
        } else {
            m.invulnerable = crate::life::INVULNERABLE_TICKS;
            damage
        };
        m.last_hurt = damage;
        m.health = (m.health - dealt).max(0.0);
        let health = m.health;
        let mut out = self.wear_held(1)?;
        let now = self.ticks;
        let Some(dm) = self.dragon.as_mut() else {
            return Ok(out);
        };
        dm.hits.push((now, part, dealt));
        dm.hurt_at = Some(now);
        if health <= 0.0 {
            dm.died_at = Some(now);
            dm.phase = PHASE_DYING;
            out.extend(self.dragon_data()?);
            out.push(wire(
                State::Play,
                &S2c::EntityEvent {
                    id: d.id,
                    status: 3,
                },
            )?);
            return Ok(out);
        }
        if dm.sitting() {
            dm.sitting_damage += dealt;
            if dm.sitting_damage > SITTING_SHARE * DRAGON_HEALTH {
                // Takeoff, and away over the island until it sits again.
                dm.sitting_damage = 0.0;
                dm.phase = PHASE_HOLDING;
                dm.lands_at = Some(now + FLY_TICKS);
                let away = [dm.perch[0], dm.perch[1] + FLY_UP, dm.perch[2] + FLY_OUT];
                if let Some(m) = self.survival.life.mobs.get_mut(&d.id) {
                    m.pos = away;
                }
            }
        }
        out.extend(self.dragon_data()?);
        Ok(out)
    }

    /// The dragon's tick: its blow on a player in its head's or neck's box while sitting; a
    /// flight ended, sat again; a death's end, the removal and the exit portal; a player in the
    /// portal sent the credits.
    pub(crate) fn tick_dragon(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let Some(d) = self.dragon.clone() else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        let now = self.ticks;
        let at = self
            .survival
            .life
            .mobs
            .get(&d.id)
            .map_or(d.perch, |m| m.pos);
        if d.sitting() && self.health > 0.0 && d.hurt_at.is_none_or(|t| now >= t + HURT_TIME) {
            let me = part_box(
                self.pos[0],
                self.pos[1],
                self.pos[2],
                PLAYER_HALF * 2.0,
                PLAYER_HEIGHT,
            );
            let grow = |b: [f64; 6]| {
                [
                    b[0] - BLOW_GROW,
                    b[1] - BLOW_GROW,
                    b[2] - BLOW_GROW,
                    b[3] + BLOW_GROW,
                    b[4] + BLOW_GROW,
                    b[5] + BLOW_GROW,
                ]
            };
            let blown = [1, 2]
                .iter()
                .filter_map(|p| d.part(*p, at))
                .any(|b| overlap(grow(b), me));
            if blown {
                self.survival.life.blow_cause = Some(d.id);
                let blow = self.blow_from(d.id, BLOW, at, 0);
                self.survival.life.blow_cause = None;
                let (landed, o) = blow?;
                if landed && let Some(dm) = self.dragon.as_mut() {
                    dm.blows += 1;
                }
                out.extend(o);
            }
        }
        if let Some(t) = d.lands_at
            && now >= t
        {
            if let Some(m) = self.survival.life.mobs.get_mut(&d.id) {
                m.pos = d.perch;
            }
            if let Some(dm) = self.dragon.as_mut() {
                dm.lands_at = None;
                dm.phase = PHASE_SITTING;
                dm.perches += 1;
            }
            out.extend(self.dragon_data()?);
        }
        if let Some(t) = d.died_at
            && now >= t + DEATH_TICKS
            && !d.portal_lit
        {
            if let Some(m) = self.survival.life.mobs.get_mut(&d.id) {
                m.dead_at = Some(now);
            }
            out.push(wire(State::Play, &S2c::RemoveEntities { ids: vec![d.id] })?);
            let portal = state(BlockKind::EndPortal);
            let y = d.floor + 1;
            for x in -2..=2i32 {
                for z in -2..=2i32 {
                    if (x, z) != (0, 0) && x * x + z * z <= PORTAL_RADIUS2 {
                        out.extend(self.set_and_update((x, y, z), portal)?);
                    }
                }
            }
            if let Some(dm) = self.dragon.as_mut() {
                dm.portal_lit = true;
            }
        }
        let feet = (
            self.pos[0].floor() as i32,
            self.pos[1].floor() as i32,
            self.pos[2].floor() as i32,
        );
        if d.portal_lit
            && d.won_at.is_none()
            && self
                .state_at(feet.0, feet.1, feet.2)
                .and_then(data::block_of_state)
                == Some(BlockKind::EndPortal)
        {
            if let Some(dm) = self.dragon.as_mut() {
                dm.won_at = Some(now);
            }
            out.push(wire(
                State::Play,
                &S2c::GameEvent {
                    event: WIN_GAME,
                    param: 1.0,
                },
            )?);
        }
        Ok(out)
    }

    /// `client_command`: 0 after the credits asks to respawn (counted; the overworld it goes
    /// to is not modelled). Anything else a real server would answer the model does not.
    pub(crate) fn on_client_command(&mut self, action: i32) -> Result<Vec<Wire>, EncodeError> {
        match self.dragon.as_mut() {
            Some(d) if action == 0 && d.won_at.is_some() => d.respawns_asked += 1,
            Some(_) | None => self
                .violations
                .push(format!("client_command {action} not modelled")),
        }
        Ok(Vec::new())
    }
}
