//! Water and lava in the model (issue 14a, brain/ruststeve/design/buckets-and-fluids.md): the
//! server's fluid ticks, buckets used along the look, lava turned to obsidian by water, and a
//! frame lit by flint and steel. Each rule is read from the 26.1.2 jar (CFR on `FlowingFluid`,
//! `WaterFluid`, `LavaFluid`, `LiquidBlock`, `BucketItem`, `FlintAndSteelItem`, `BaseFireBlock`,
//! `PortalShape`).
//!
//! Left out, and why: waterlogged blocks hold no fluid here (none is in the cast's rock); a
//! partial shape never occludes a flow (vanilla's `mergedFaceOccludes`); a block a flow
//! replaces drops nothing; lava's random ticks (fire) and the fizz; a plant's outline is a full
//! cube to the ray (vanilla aims at the outline, which is smaller).

use std::collections::{BTreeMap, BTreeSet};

use data::BlockKind;
use data::types::Collision;
use protocol::EncodeError;

use crate::survival::{property, with_property};
use crate::{Sim, Wire};

type Pos = (i32, i32, i32);

/// The player's reach for using an item along the look (`blockInteractionRange`).
pub const USE_RANGE: f64 = 4.5;
/// `WaterFluid.getTickDelay`, `LavaFluid.getTickDelay` outside the Nether.
const WATER_DELAY: u64 = 5;
const LAVA_DELAY: u64 = 30;
/// `LavaFluid.getTickDelay`, `getDropOff` and `getSlopeFindDistance` where the dimension has
/// fast lava (the Nether alone: `ultrawarm` before 1.21.11, the `fast_lava` attribute since):
/// 10, 1 and 4, against 30, 2 and 2 elsewhere. The wiki's numbers agree (Lava, "Flow": a block
/// every 10 game ticks and 7 blocks from its source in the Nether, every 30 and 3 blocks in
/// the Overworld and the End).
const NETHER_LAVA_DELAY: u64 = 10;
const NETHER_LAVA_DROP_OFF: i32 = 1;
const NETHER_LAVA_SLOPE_FIND: i32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Water,
    Lava,
}

impl Kind {
    fn block(self) -> BlockKind {
        match self {
            Kind::Water => BlockKind::Water,
            Kind::Lava => BlockKind::Lava,
        }
    }
    /// `getTickDelay`; `fast` is the dimension's fast lava (`ModelWorld::fast_lava`).
    fn delay(self, fast: bool) -> u64 {
        match self {
            Kind::Water => WATER_DELAY,
            Kind::Lava if fast => NETHER_LAVA_DELAY,
            Kind::Lava => LAVA_DELAY,
        }
    }
    /// `getDropOff`.
    fn drop_off(self, fast: bool) -> i32 {
        match self {
            Kind::Water => 1,
            Kind::Lava if fast => NETHER_LAVA_DROP_OFF,
            Kind::Lava => 2,
        }
    }
    /// `getSlopeFindDistance`.
    fn slope_find(self, fast: bool) -> i32 {
        match self {
            Kind::Water => 4,
            Kind::Lava if fast => NETHER_LAVA_SLOPE_FIND,
            Kind::Lava => 2,
        }
    }
    /// `canConvertToSource`: the game rules' defaults (water yes, lava no).
    fn converts(self) -> bool {
        self == Kind::Water
    }
    fn bucket(self) -> data::ItemKind {
        match self {
            Kind::Water => data::ItemKind::WaterBucket,
            Kind::Lava => data::ItemKind::LavaBucket,
        }
    }
}

/// A fluid state: its kind, amount (1 to 8), whether it is a source, and whether it falls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FluidState {
    pub kind: Kind,
    pub amount: i32,
    pub source: bool,
    pub falling: bool,
}

impl FluidState {
    fn source(kind: Kind) -> Self {
        Self {
            kind,
            amount: 8,
            source: true,
            falling: false,
        }
    }
    fn flowing(kind: Kind, amount: i32, falling: bool) -> Self {
        Self {
            kind,
            amount,
            source: false,
            falling,
        }
    }
    /// `FlowingFluid.getLegacyLevel`: the block's `level` property.
    fn level(self) -> i32 {
        if self.source {
            0
        } else {
            8 - self.amount.min(8) + if self.falling { 8 } else { 0 }
        }
    }
    /// The block state that holds it (`createLegacyBlock`).
    pub fn state(self) -> Option<u32> {
        let base = self.kind.block().info()?.default_state;
        with_property(base, "level", &self.level().to_string())
    }
}

/// The fluid in a block state (`LiquidBlock.getFluidState`): level 0 a source, 1 to 7 flowing
/// with 8 less the level, 8 and over falling with 8.
pub fn fluid_of(state: u32) -> Option<FluidState> {
    let kind = match data::block_of_state(state)? {
        BlockKind::Water => Kind::Water,
        BlockKind::Lava => Kind::Lava,
        _ => return None,
    };
    let level: i32 = property(state, "level")?.parse().ok()?;
    Some(match level {
        0 => FluidState::source(kind),
        1..=7 => FluidState::flowing(kind, 8 - level, false),
        _ => FluidState::flowing(kind, 8, true),
    })
}

fn kind_of(state: u32) -> Option<BlockKind> {
    data::block_of_state(state)
}

fn collision(state: u32) -> Collision {
    kind_of(state)
        .and_then(data::generated::shapes::collision)
        .unwrap_or(Collision::Full)
}

fn is_air(state: u32) -> bool {
    matches!(
        kind_of(state),
        Some(BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir)
    )
}

/// `BlockState.blocksMotion`: a collision shape (cobwebs and bamboo saplings aside).
fn blocks_motion(state: u32) -> bool {
    !matches!(collision(state), Collision::Empty)
        && !matches!(
            kind_of(state),
            Some(BlockKind::Cobweb | BlockKind::BambooSapling)
        )
}

/// `canHoldAnyFluid`.
fn holds_any(state: u32) -> bool {
    if blocks_motion(state) {
        return false;
    }
    let name = kind_of(state).and_then(BlockKind::name).unwrap_or("");
    !(name.ends_with("_door")
        || name.ends_with("_sign")
        || matches!(
            kind_of(state),
            Some(
                BlockKind::Ladder
                    | BlockKind::SugarCane
                    | BlockKind::BubbleColumn
                    | BlockKind::NetherPortal
                    | BlockKind::EndPortal
                    | BlockKind::EndGateway
                    | BlockKind::StructureVoid
            )
        ))
}

/// `canPassThroughWall`, with partial shapes never occluding (see the module's note).
fn passes_wall(source: u32, target: u32) -> bool {
    !matches!(collision(target), Collision::Full) && !matches!(collision(source), Collision::Full)
}

/// `FluidState.getHeight`: full when the same fluid is above, else the amount in ninths.
fn height(f: Option<FluidState>, above: Option<FluidState>) -> f32 {
    match f {
        None => 0.0,
        Some(f) if above.is_some_and(|a| a.kind == f.kind) => 1.0,
        Some(f) => f.amount as f32 / 9.0,
    }
}

const SIDES: [(i32, i32); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)]; // north, south, west, east

fn side_of(p: Pos, (dx, dz): (i32, i32)) -> Pos {
    (p.0 + dx, p.1, p.2 + dz)
}

/// The model's scheduled fluid ticks (`LevelTicks`): due tick, then order scheduled.
#[derive(Clone, Debug, Default)]
pub struct Fluids {
    queue: BTreeMap<(u64, u64), (Pos, Kind)>,
    scheduled: BTreeSet<(Pos, Kind)>,
    next: u64,
}

impl Sim {
    fn st(&self, (x, y, z): Pos) -> u32 {
        self.state_at(x, y, z).unwrap_or(0)
    }

    fn fluid_at(&self, p: Pos) -> Option<FluidState> {
        fluid_of(self.st(p))
    }

    fn schedule(&mut self, p: Pos, kind: Kind, delay: u64) {
        if !self.fluids.scheduled.insert((p, kind)) {
            return;
        }
        let at = self.ticks + delay.max(1);
        let n = self.fluids.next;
        self.fluids.next += 1;
        self.fluids.queue.insert((at, n), (p, kind));
    }

    /// `Level.setBlock` with updates: the block changed and sent, then the liquid blocks it
    /// touches told (`neighborChanged` and `updateShape`), and its own `onPlace`.
    pub(crate) fn set_and_update(&mut self, p: Pos, state: u32) -> Result<Vec<Wire>, EncodeError> {
        let old = self.st(p);
        if old == state {
            return Ok(Vec::new());
        }
        self.world.set_block(p.0, p.1, p.2, state)?;
        let mut out = vec![self.block_update_at(p)?];
        out.extend(self.liquid_placed(p)?);
        if kind_of(state) == Some(BlockKind::Fire) && kind_of(old) != Some(BlockKind::Fire) {
            out.extend(self.fire_placed(p)?);
            // A fire that did not become a portal burns by its own clock (life.rs).
            if kind_of(self.st(p)) == Some(BlockKind::Fire) {
                self.fire_lit(p);
            }
        }
        let around = [
            (p.0, p.1 - 1, p.2),
            (p.0, p.1 + 1, p.2),
            (p.0, p.1, p.2 - 1),
            (p.0, p.1, p.2 + 1),
            (p.0 - 1, p.1, p.2),
            (p.0 + 1, p.1, p.2),
        ];
        out.extend(self.fall_onto(p)?);
        for n in around {
            let Some(f) = self.fluid_at(n) else {
                continue;
            };
            // `neighborChanged` (flag 3 updates every neighbour): lava meeting water converts,
            // else the neighbour's tick is scheduled.
            if let Some(converted) = self.spread_or_convert(n)? {
                out.extend(converted);
                continue;
            }
            self.schedule(n, f.kind, f.kind.delay(self.world.fast_lava()));
        }
        Ok(out)
    }

    /// `FallingBlock`: gravel or sand over a cell that has just come free falls into it, down
    /// to the first cell with something under it, and what stood on that gravel follows. In
    /// the game the block waits 2 ticks and falls as an entity; here it lands at once.
    fn fall_onto(&mut self, p: Pos) -> Result<Vec<Wire>, EncodeError> {
        let free = |sim: &Self, c: Pos| {
            sim.state_at(c.0, c.1, c.2)
                .is_some_and(|s| is_air(s) || fluid_of(s).is_some())
        };
        let above = (p.0, p.1 + 1, p.2);
        let falls = matches!(
            kind_of(self.st(above)),
            Some(BlockKind::Gravel | BlockKind::Sand | BlockKind::RedSand)
        );
        if !falls || !free(self, p) {
            return Ok(Vec::new());
        }
        let mut land = p;
        while free(self, (land.0, land.1 - 1, land.2)) {
            land.1 -= 1;
        }
        let block = self.st(above);
        let air = BlockKind::Air.info().map_or(0, |i| i.default_state);
        let mut out = self.set_and_update(land, block)?;
        out.extend(self.set_and_update(above, air)?);
        Ok(out)
    }

    fn block_update_at(&self, (x, y, z): Pos) -> Result<Wire, EncodeError> {
        crate::wire(
            protocol::State::Play,
            &protocol::S2c::BlockUpdate {
                pos: (x, y, z),
                state: self.state_at(x, y, z).unwrap_or(0),
            },
        )
    }

    /// `LiquidBlock.onPlace` for a fluid just set at `p`.
    fn liquid_placed(&mut self, p: Pos) -> Result<Vec<Wire>, EncodeError> {
        let Some(f) = self.fluid_at(p) else {
            return Ok(Vec::new());
        };
        if let Some(out) = self.spread_or_convert(p)? {
            return Ok(out);
        }
        self.schedule(p, f.kind, f.kind.delay(self.world.fast_lava()));
        Ok(Vec::new())
    }

    /// `LiquidBlock.shouldSpreadLiquid`: lava with water above it or beside it becomes obsidian
    /// (a source) or cobblestone (flowing). `Some` with the packets when it converted.
    fn spread_or_convert(&mut self, p: Pos) -> Result<Option<Vec<Wire>>, EncodeError> {
        let Some(f) = self.fluid_at(p) else {
            return Ok(None);
        };
        if f.kind != Kind::Lava {
            return Ok(None);
        }
        let watered = [
            (p.0, p.1 + 1, p.2),
            (p.0, p.1, p.2 - 1),
            (p.0, p.1, p.2 + 1),
            (p.0 - 1, p.1, p.2),
            (p.0 + 1, p.1, p.2),
        ]
        .into_iter()
        .any(|n| self.fluid_at(n).is_some_and(|w| w.kind == Kind::Water));
        if !watered {
            return Ok(None);
        }
        let to = if f.source {
            BlockKind::Obsidian
        } else {
            BlockKind::Cobblestone
        };
        let state = to.info().map_or(0, |i| i.default_state);
        Ok(Some(self.set_and_update(p, state)?))
    }

    /// The fluid ticks due now, in order (`LevelTicks.tick`), in the chunks that tick: those
    /// within the simulation distance of the player. A tick due in a chunk farther off waits
    /// there until the player comes near, as a server's does.
    pub(crate) fn tick_fluids(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let (cx, cz) = crate::column_of(self.pos[0], self.pos[2]);
        let r = self.config.simulation_distance;
        let due: Vec<((u64, u64), (Pos, Kind))> = self
            .fluids
            .queue
            .range(..=(self.ticks, u64::MAX))
            .filter(|(_, (p, _))| ((p.0 >> 4) - cx).abs() <= r && ((p.2 >> 4) - cz).abs() <= r)
            .map(|(k, v)| (*k, *v))
            .collect();
        for (key, (p, kind)) in due {
            self.fluids.queue.remove(&key);
            self.fluids.scheduled.remove(&(p, kind));
            if self.fluid_at(p).is_some_and(|f| f.kind == kind) {
                out.extend(self.fluid_tick(p)?);
            }
        }
        Ok(out)
    }

    /// Wake the world's fluids: a tick scheduled for every fluid cell that is flowing, or has
    /// under it or beside it a cell it could run into. A generated chunk's lava falls and
    /// springs run when a player first brings the chunk to tick; columns captured as a server
    /// sent them on arrival hold that lava still, and without this it would stay so (the lava
    /// that came down on the Nether portal's ledge 700 ticks after each bot arrived:
    /// docs/problems/nether-walk.md). How many cells were woken.
    pub fn wake_fluids(&mut self) -> usize {
        let mut cells: Vec<(Pos, Kind)> = Vec::new();
        for (&(cx, cz), col) in &self.world.columns {
            let column = &col.column;
            for (si, section) in column.sections.iter().enumerate() {
                if !section.blocks.may_contain(|s| fluid_of(s).is_some()) {
                    continue;
                }
                let base = column.min_y + 16 * i32::try_from(si).unwrap_or(0);
                for i in 0..4096usize {
                    let Some(f) = section.blocks.get(i).and_then(fluid_of) else {
                        continue;
                    };
                    let (lx, lz, ly) = ((i & 15) as i32, ((i >> 4) & 15) as i32, (i >> 8) as i32);
                    cells.push(((cx * 16 + lx, base + ly, cz * 16 + lz), f.kind));
                }
            }
        }
        let fast = self.world.fast_lava();
        let mut woken = 0;
        for (p, kind) in cells {
            let open = |n: Pos| {
                // Only inside the world: a cell of a column not held is no room to run into.
                self.state_at(n.0, n.1, n.2)
                    .is_some_and(|s| holds_any(s) && fluid_of(s).is_none_or(|f| f.kind != kind))
            };
            let flowing = self.fluid_at(p).is_some_and(|f| !f.source);
            let room = open((p.0, p.1 - 1, p.2)) || SIDES.iter().any(|d| open(side_of(p, *d)));
            if flowing || room {
                self.schedule(p, kind, kind.delay(fast));
                woken += 1;
            }
        }
        woken
    }

    /// `FlowingFluid.tick`.
    fn fluid_tick(&mut self, p: Pos) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let Some(mut f) = self.fluid_at(p) else {
            return Ok(out);
        };
        if !f.source {
            let new = self.new_liquid(f.kind, p);
            match new {
                None => {
                    let air = BlockKind::Air.info().map_or(0, |i| i.default_state);
                    out.extend(self.set_and_update(p, air)?);
                    return Ok(out);
                }
                Some(n) if n != f => {
                    f = n;
                    if let Some(s) = n.state() {
                        out.extend(self.set_and_update(p, s)?);
                    }
                    self.schedule(p, n.kind, n.kind.delay(self.world.fast_lava()));
                }
                Some(_) => {}
            }
        }
        out.extend(self.spread(p, f)?);
        Ok(out)
    }

    /// `FlowingFluid.getNewLiquid`.
    fn new_liquid(&self, kind: Kind, p: Pos) -> Option<FluidState> {
        let here = self.st(p);
        let mut highest = 0;
        let mut sources = 0;
        for d in SIDES {
            let n = side_of(p, d);
            let ns = self.st(n);
            let Some(nf) = fluid_of(ns).filter(|nf| nf.kind == kind) else {
                continue;
            };
            if !passes_wall(here, ns) {
                continue;
            }
            if nf.source {
                sources += 1;
            }
            highest = highest.max(nf.amount);
        }
        if sources >= 2 && kind.converts() {
            let below = self.st((p.0, p.1 - 1, p.2));
            let below_source = fluid_of(below).is_some_and(|b| b.kind == kind && b.source);
            if !matches!(collision(below), Collision::Empty) || below_source {
                return Some(FluidState::source(kind));
            }
        }
        let above = self.st((p.0, p.1 + 1, p.2));
        if fluid_of(above).is_some_and(|a| a.kind == kind) && passes_wall(here, above) {
            return Some(FluidState::flowing(kind, 8, true));
        }
        let amount = highest - kind.drop_off(self.world.fast_lava());
        (amount > 0).then(|| FluidState::flowing(kind, amount, false))
    }

    /// `canMaybePassThrough` then `canHoldSpecificFluid` (always, with no containers).
    fn can_pass(&self, kind: Kind, from: Pos, to: Pos) -> bool {
        let ts = self.st(to);
        let source_here = fluid_of(ts).is_some_and(|t| t.kind == kind && t.source);
        !source_here && holds_any(ts) && passes_wall(self.st(from), ts)
    }

    /// `isWaterHole`: the fluid can fall from `top` into `bottom`.
    fn is_hole(&self, kind: Kind, top: Pos) -> bool {
        let bottom = (top.0, top.1 - 1, top.2);
        let bs = self.st(bottom);
        if !passes_wall(self.st(top), bs) {
            return false;
        }
        fluid_of(bs).is_some_and(|b| b.kind == kind) || holds_any(bs)
    }

    /// `canBeReplacedWith` of whatever fluid is at `p`.
    fn replaceable_by(&self, p: Pos, by: Kind, down: bool) -> bool {
        let here = self.fluid_at(p);
        match here {
            None => true,
            Some(h) if h.kind == Kind::Water => down && by != Kind::Water,
            Some(h) => {
                let above = self.fluid_at((p.0, p.1 + 1, p.2));
                height(Some(h), above) >= 0.444_444_45 && by == Kind::Water
            }
        }
    }

    /// `FlowingFluid.spread`.
    fn spread(&mut self, p: Pos, f: FluidState) -> Result<Vec<Wire>, EncodeError> {
        let below = (p.0, p.1 - 1, p.2);
        if self.can_pass(f.kind, p, below)
            && let Some(nb) = self.new_liquid(f.kind, below)
            && self.replaceable_by(below, nb.kind, true)
        {
            let mut out = self.spread_to(below, nb, true)?;
            let sources = SIDES
                .iter()
                .filter(|d| {
                    self.fluid_at(side_of(p, **d))
                        .is_some_and(|n| n.kind == f.kind && n.source)
                })
                .count();
            if sources >= 3 {
                out.extend(self.spread_sides(p, f)?);
            }
            return Ok(out);
        }
        if f.source || !self.is_hole(f.kind, p) {
            return self.spread_sides(p, f);
        }
        Ok(Vec::new())
    }

    fn spread_sides(&mut self, p: Pos, f: FluidState) -> Result<Vec<Wire>, EncodeError> {
        let mut amount = f.amount - f.kind.drop_off(self.world.fast_lava());
        if f.falling {
            amount = 7;
        }
        if amount <= 0 {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for (d, nf) in self.get_spread(p, f.kind) {
            out.extend(self.spread_to(side_of(p, d), nf, false)?);
        }
        Ok(out)
    }

    /// `LavaFluid.spreadTo` (lava falling into water makes stone) and `FlowingFluid.spreadTo`.
    fn spread_to(&mut self, to: Pos, nf: FluidState, down: bool) -> Result<Vec<Wire>, EncodeError> {
        if down && nf.kind == Kind::Lava && self.fluid_at(to).is_some_and(|w| w.kind == Kind::Water)
        {
            let stone = BlockKind::Stone.info().map_or(0, |i| i.default_state);
            return self.set_and_update(to, stone);
        }
        match nf.state() {
            Some(s) => self.set_and_update(to, s),
            None => Ok(Vec::new()),
        }
    }

    /// `FlowingFluid.getSpread`: the sides the fluid goes to, toward the nearest hole.
    fn get_spread(&self, p: Pos, kind: Kind) -> Vec<((i32, i32), FluidState)> {
        let mut lowest = 1000;
        let mut result: Vec<((i32, i32), FluidState)> = Vec::new();
        for d in SIDES {
            let t = side_of(p, d);
            if !self.can_pass(kind, p, t) {
                continue;
            }
            let Some(nf) = self.new_liquid(kind, t) else {
                continue;
            };
            let distance = if self.is_hole(kind, t) {
                0
            } else {
                self.slope_distance(kind, t, 1, (-d.0, -d.1))
            };
            if distance < lowest {
                result.clear();
            }
            if distance > lowest {
                continue;
            }
            if self.replaceable_by(t, nf.kind, false) {
                result.push((d, nf));
            }
            lowest = distance;
        }
        result
    }

    /// `FlowingFluid.getSlopeDistance`.
    fn slope_distance(&self, kind: Kind, p: Pos, pass: i32, from: (i32, i32)) -> i32 {
        let mut lowest = 1000;
        for d in SIDES {
            if d == from {
                continue;
            }
            let t = side_of(p, d);
            if !self.can_pass(kind, p, t) {
                continue;
            }
            if self.is_hole(kind, t) {
                return pass;
            }
            if pass >= kind.slope_find(self.world.fast_lava()) {
                continue;
            }
            let v = self.slope_distance(kind, t, pass + 1, (-d.0, -d.1));
            if v < lowest {
                lowest = v;
            }
        }
        lowest
    }

    // ---------------------------------------------------------------- along the look

    /// The eye and the look's unit vector (`calculateViewVector`).
    fn look_ray(&self) -> ([f64; 3], [f64; 3]) {
        let eye = [self.pos[0], self.pos[1] + physics::EYE_HEIGHT, self.pos[2]];
        let (yaw, pitch) = self.look;
        let f = f64::from(pitch).to_radians();
        let g = -f64::from(yaw).to_radians();
        let (h, i, j, k) = (g.cos(), g.sin(), f.cos(), f.sin());
        (eye, [i * j, -k, h * j])
    }

    /// `getPlayerPOVHitResult`: the first cell the look meets within `USE_RANGE`, and the face
    /// entered (0 down .. 5 east). With `sources`, a fluid source's surface stops it.
    pub(crate) fn look_hit(&self, sources: bool) -> Option<(Pos, i32)> {
        let (eye, dir) = self.look_ray();
        let end = [
            eye[0] + dir[0] * USE_RANGE,
            eye[1] + dir[1] * USE_RANGE,
            eye[2] + dir[2] * USE_RANGE,
        ];
        let mut best: Option<(f64, Pos, i32)> = None;
        let cells = cells_along(eye, end);
        for c in cells {
            let s = self.st(c);
            let mut boxes: Vec<[f64; 6]> = Vec::new();
            if !is_air(s) && fluid_of(s).is_none() {
                let top = match collision(s) {
                    Collision::Partial { top } => top,
                    Collision::Full | Collision::Empty => 1.0,
                };
                boxes.push([0.0, 0.0, 0.0, 1.0, top, 1.0]);
            }
            if sources && let Some(f) = fluid_of(s).filter(|f| f.source) {
                let above = self.fluid_at((c.0, c.1 + 1, c.2));
                boxes.push([0.0, 0.0, 0.0, 1.0, f64::from(height(Some(f), above)), 1.0]);
            }
            for b in boxes {
                let abs = [
                    f64::from(c.0) + b[0],
                    f64::from(c.1) + b[1],
                    f64::from(c.2) + b[2],
                    f64::from(c.0) + b[3],
                    f64::from(c.1) + b[4],
                    f64::from(c.2) + b[5],
                ];
                if let Some((t, face)) = ray_box(eye, dir, USE_RANGE, abs)
                    && best.is_none_or(|(bt, _, _)| t < bt)
                {
                    best = Some((t, c, face));
                }
            }
            if best.is_some() {
                break;
            }
        }
        best.map(|(_, c, face)| (c, face))
    }

    /// `BucketItem.use` for the held bucket: `None` when the held item is no bucket.
    pub(crate) fn use_bucket(&mut self) -> Option<Result<Vec<Wire>, EncodeError>> {
        let (item, _) = self.survival.held_stack()?;
        let content = match item {
            data::ItemKind::Bucket => None,
            data::ItemKind::WaterBucket => Some(Kind::Water),
            data::ItemKind::LavaBucket => Some(Kind::Lava),
            _ => return None,
        };
        Some(self.bucket(content))
    }

    fn bucket(&mut self, content: Option<Kind>) -> Result<Vec<Wire>, EncodeError> {
        let Some((pos, face)) = self.look_hit(content.is_none()) else {
            return Ok(Vec::new());
        };
        match content {
            None => {
                // `LiquidBlock.pickupBlock`: a source becomes air and fills the bucket.
                let Some(f) = self.fluid_at(pos).filter(|f| f.source) else {
                    return Ok(Vec::new());
                };
                let air = BlockKind::Air.info().map_or(0, |i| i.default_state);
                let out = self.set_and_update(pos, air)?;
                self.survival.fill_held(f.kind.bucket());
                Ok(out)
            }
            Some(kind) => {
                let at = beside(pos, face);
                let s = self.st(at);
                let replaceable = is_air(s) || fluid_of(s).is_some() || !blocks_motion(s);
                if !replaceable {
                    return Ok(Vec::new());
                }
                let was_source = fluid_of(s).is_some_and(|f| f.source);
                let Some(new) = FluidState::source(kind).state() else {
                    return Ok(Vec::new());
                };
                let out = self.set_and_update(at, new)?;
                if !out.is_empty() || was_source {
                    self.survival.fill_held(data::ItemKind::Bucket);
                }
                Ok(out)
            }
        }
    }

    // ---------------------------------------------------------------- fire and the portal

    /// `FlintAndSteelItem.useOn`: fire beside the clicked face when it may stand there
    /// (`canBePlacedAt`: air, and a sturdy floor or a portal frame round it).
    pub(crate) fn strike(&mut self, clicked: Pos, face: i32) -> Result<Vec<Wire>, EncodeError> {
        let at = beside(clicked, face);
        if !is_air(self.st(at)) {
            return Ok(Vec::new());
        }
        let floor = matches!(collision(self.st((at.0, at.1 - 1, at.2))), Collision::Full);
        if !floor && self.portal_shape(at).is_none() {
            return Ok(Vec::new());
        }
        let fire = BlockKind::Fire.info().map_or(0, |i| i.default_state);
        self.set_and_update(at, fire)
    }

    /// `BaseFireBlock.onPlace`: fire inside an empty frame fills it with portal blocks.
    fn fire_placed(&mut self, p: Pos) -> Result<Vec<Wire>, EncodeError> {
        let Some(shape) = self.portal_shape(p) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        let base = BlockKind::NetherPortal
            .info()
            .map_or(0, |i| i.default_state);
        let state =
            with_property(base, "axis", if shape.x_axis { "x" } else { "z" }).unwrap_or(base);
        for up in 0..shape.height {
            for right in 0..shape.width {
                let (dx, dz) = if shape.x_axis {
                    (-right, 0)
                } else {
                    (0, right)
                };
                let c = (
                    shape.bottom_left.0 + dx,
                    shape.bottom_left.1 + up,
                    shape.bottom_left.2 + dz,
                );
                self.world.set_block(c.0, c.1, c.2, state)?;
                out.push(self.block_update_at(c)?);
            }
        }
        Ok(out)
    }

    /// `PortalShape.findEmptyPortalShape(pos, X)`: the X axis first (right is west), then Z
    /// (right is south).
    fn portal_shape(&self, p: Pos) -> Option<PortalFrame> {
        [true, false]
            .into_iter()
            .find_map(|x_axis| self.any_shape(p, x_axis).filter(|s| s.portal_blocks == 0))
    }

    fn any_shape(&self, p: Pos, x_axis: bool) -> Option<PortalFrame> {
        let right: (i32, i32) = if x_axis { (-1, 0) } else { (0, 1) };
        let left = (-right.0, -right.1);
        let empty = |c: Pos| {
            let s = self.st(c);
            is_air(s)
                || matches!(
                    kind_of(s),
                    Some(BlockKind::Fire | BlockKind::SoulFire | BlockKind::NetherPortal)
                )
        };
        let frame = |c: Pos| kind_of(self.st(c)) == Some(BlockKind::Obsidian);
        let at = |c: Pos, d: (i32, i32), n: i32| (c.0 + d.0 * n, c.1, c.2 + d.1 * n);
        // calculateBottomLeft
        let mut pos = p;
        let min_y = p.1 - 21;
        while pos.1 > min_y && empty((pos.0, pos.1 - 1, pos.2)) {
            pos.1 -= 1;
        }
        let until_edge = |c: Pos, d: (i32, i32)| -> i32 {
            for w in 0..=21 {
                let b = at(c, d, w);
                if !empty(b) {
                    return if frame(b) { w } else { 0 };
                }
                if !frame((b.0, b.1 - 1, b.2)) {
                    return 0;
                }
            }
            0
        };
        let edge = until_edge(pos, left) - 1;
        if edge < 0 {
            return None;
        }
        let bottom_left = at(pos, left, edge);
        let width = until_edge(bottom_left, right);
        if !(2..=21).contains(&width) {
            return None;
        }
        // calculateHeight
        let mut portal_blocks = 0;
        let mut height = 21;
        'rows: for h in 0..21 {
            let row = (bottom_left.0, bottom_left.1 + h, bottom_left.2);
            if !frame(at(row, right, -1)) || !frame(at(row, right, width)) {
                height = h;
                break;
            }
            for i in 0..width {
                let c = at(row, right, i);
                if !empty(c) {
                    height = h;
                    break 'rows;
                }
                if kind_of(self.st(c)) == Some(BlockKind::NetherPortal) {
                    portal_blocks += 1;
                }
            }
        }
        if !(3..=21).contains(&height) {
            return None;
        }
        let top = (bottom_left.0, bottom_left.1 + height, bottom_left.2);
        if !(0..width).all(|i| frame(at(top, right, i))) {
            return None;
        }
        Some(PortalFrame {
            x_axis,
            bottom_left,
            width,
            height,
            portal_blocks,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct PortalFrame {
    x_axis: bool,
    bottom_left: Pos,
    width: i32,
    height: i32,
    portal_blocks: i32,
}

/// The cell beside `p` across a face (0 down, 1 up, 2 north, 3 south, 4 west, 5 east).
pub fn beside(p: Pos, face: i32) -> Pos {
    match face {
        0 => (p.0, p.1 - 1, p.2),
        1 => (p.0, p.1 + 1, p.2),
        2 => (p.0, p.1, p.2 - 1),
        3 => (p.0, p.1, p.2 + 1),
        4 => (p.0 - 1, p.1, p.2),
        _ => (p.0 + 1, p.1, p.2),
    }
}

/// The cells a segment crosses, in order (`BlockGetter.traverseBlocks`).
fn cells_along(a: [f64; 3], b: [f64; 3]) -> Vec<Pos> {
    let mut out = Vec::new();
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let mut c = [
        a[0].floor() as i32,
        a[1].floor() as i32,
        a[2].floor() as i32,
    ];
    let end = [
        b[0].floor() as i32,
        b[1].floor() as i32,
        b[2].floor() as i32,
    ];
    let step = d.map(|v| {
        if v > 0.0 {
            1
        } else if v < 0.0 {
            -1
        } else {
            0
        }
    });
    let delta = d.map(|v| {
        if v == 0.0 {
            f64::INFINITY
        } else {
            1.0 / v.abs()
        }
    });
    let mut t_max = [0.0; 3];
    for i in 0..3 {
        t_max[i] = if step[i] > 0 {
            (f64::from(c[i]) + 1.0 - a[i]) * delta[i]
        } else if step[i] < 0 {
            (a[i] - f64::from(c[i])) * delta[i]
        } else {
            f64::INFINITY
        };
    }
    out.push((c[0], c[1], c[2]));
    for _ in 0..64 {
        if c == end {
            break;
        }
        let i = if t_max[0] < t_max[1] && t_max[0] < t_max[2] {
            0
        } else if t_max[1] < t_max[2] {
            1
        } else {
            2
        };
        if t_max[i] > 1.0 {
            break;
        }
        c[i] += step[i];
        t_max[i] += delta[i];
        out.push((c[0], c[1], c[2]));
    }
    out
}

/// Where a ray from `o` along unit `d` enters box `b` within `range`, and the face it enters.
fn ray_box(o: [f64; 3], d: [f64; 3], range: f64, b: [f64; 6]) -> Option<(f64, i32)> {
    let mut t_in = f64::NEG_INFINITY;
    let mut t_out = f64::INFINITY;
    let mut face = -1;
    for i in 0..3 {
        let (lo, hi) = (b[i], b[i + 3]);
        if d[i].abs() < 1e-12 {
            if o[i] < lo || o[i] > hi {
                return None;
            }
            continue;
        }
        let (t1, t2) = ((lo - o[i]) / d[i], (hi - o[i]) / d[i]);
        let (near, far, near_face) = if t1 < t2 {
            // Entering through the low face: down (0), north (2), west (4).
            (t1, t2, [4, 0, 2][i])
        } else {
            (t2, t1, [5, 1, 3][i])
        };
        if near > t_in {
            t_in = near;
            face = near_face;
        }
        t_out = t_out.min(far);
    }
    (t_in <= t_out && t_in >= 0.0 && t_in <= range && face >= 0).then_some((t_in, face))
}
