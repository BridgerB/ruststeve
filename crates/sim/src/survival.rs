//! Issue 10b: the server's side of digging, drops, pickup and window 0, each rule read from the
//! 26.1.2 jar with `javap` (brain/ruststeve/design/server-model.md):
//!
//! - digging: `ServerPlayerGameMode.handleBlockBreakAction` and `tick`, with the progress of
//!   `BlockBehaviour.getDestroyProgress` and `Player.getDestroySpeed`;
//! - drops: `Block.popResource` and the `ItemEntity` constructor, then `ItemEntity.tick`;
//! - pickup: `Player.aiStep`'s touch box and `ItemEntity.playerTouch`, `Inventory.add`;
//! - window 0: `ServerGamePacketListenerImpl.handleContainerClick`,
//!   `AbstractContainerMenu.doClick`, `InventoryMenu.quickMoveStack`, `ResultSlot.onTake` and
//!   `CraftingMenu.slotChangedCraftingGrid`.

use data::{BlockKind, ItemKind};
use protocol::metadata::MetaValue;
use protocol::slot::Slot;
use protocol::{EncodeError, S2c, State};

use crate::{PLAYER_ID, Sim, Wire, wire};

/// A stack: an item and how many (at least 1).
pub type Stack = (ItemKind, i32);

/// Window 0's menu slots: 0 the craft result, 1-4 the 2x2 grid, 5-8 armour, 9-35 the main
/// inventory, 36-44 the hotbar, 45 the offhand.
pub const MENU_SLOTS: usize = 46;
const RESULT: usize = 0;
const GRID: [usize; 4] = [1, 2, 3, 4];
const HOTBAR: usize = 36;

/// FINISH is accepted once the progress reaches this (vanilla's 0.7f in handleBlockBreakAction).
const FINISH_ACCEPT: f32 = 0.7;
/// The block interaction range (the attribute's default 4.5) and the 1.0 slack
/// `isWithinBlockInteractionRange(pos, 1.0)` adds.
const INTERACTION_RANGE: f64 = 4.5 + 1.0;
/// `setDefaultPickUpDelay`: ticks before a dropped item can be picked up.
pub const PICKUP_DELAY: u32 = 10;
/// A thrown item (the cursor dropped when the window closes): `setPickUpDelay(40)`.
const THROW_PICKUP_DELAY: u32 = 40;
/// The item entity's size (EntityType.ITEM: 0.25 by 0.25).
const ITEM_SIZE: f64 = 0.25;
/// Gravity and drag of an item entity (`ItemEntity.getDefaultGravity` 0.04, the 0.98 drag).
const ITEM_GRAVITY: f64 = 0.04;
const ITEM_DRAG: f64 = 0.98;
/// Friction under an item on the ground: the block's (0.6 for all but ice and slime) times 0.98.
const GROUND_FRICTION: f64 = 0.6 * 0.98;
/// An item entity despawns after 6000 ticks (five minutes).
const ITEM_LIFETIME: u32 = 6000;
/// How often the server tells the client where an item is (EntityType.ITEM's update interval).
const ITEM_UPDATE_EVERY: u32 = 20;
/// The player's touch box grows this much sideways and up (`Player.aiStep`).
const TOUCH_SIDEWAYS: f64 = 1.0;
const TOUCH_UP: f64 = 0.5;
/// The item entity's metadata index for its stack.
const ITEM_STACK_INDEX: u8 = 8;
/// An item's max stack size, from its default components (generated, issue 11a).
fn max_stack(item: ItemKind) -> i32 {
    data::generated::item_props::max_stack(item)
}

/// Where a menu slot is stored: the result, a grid cell, or one of the inventory's menu slots
/// (window 0's numbering).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Loc {
    Result,
    Grid(usize),
    Inv(usize),
}

/// A crafting table's open menu (`CraftingMenu`, issue 11a).
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub id: i32,
    pub pos: (i32, i32, i32),
    pub grid: [Option<Stack>; 9],
    pub result: Option<Stack>,
    pub state_id: i32,
    remote: [Option<Stack>; MENU_SLOTS],
}

/// The blocks report's definitions a placed block replaces (air and water aside): plants, snow.
const REPLACEABLE: &[&str] = &[
    "tall_grass",
    "double_plant",
    "short_dry_grass",
    "tall_dry_grass",
    "dry_vegetation",
    "leaf_litter",
    "flower_bed",
    "snow_layer",
    "vine",
];

/// A dig the server is timing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dig {
    pub pos: (i32, i32, i32),
    /// The game tick it started (`destroyProgressStart`).
    pub start: u64,
}

/// An item entity in the model's world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemEntity {
    pub stack: Stack,
    pub pos: [f64; 3],
    pub velocity: [f64; 3],
    pub on_ground: bool,
    pub pickup_delay: u32,
    pub age: u32,
}

/// The player's side of play the model keeps for issue 10.
#[derive(Clone, Debug, PartialEq)]
pub struct Survival {
    /// Window 0 as the server holds it, and the cursor.
    pub slots: [Option<Stack>; MENU_SLOTS],
    pub carried: Option<Stack>,
    pub state_id: i32,
    /// What the server believes the client shows (vanilla's remote slots and carried).
    remote: [Option<Stack>; MENU_SLOTS],
    remote_carried: Option<Stack>,
    /// The selected hotbar slot, 0-8.
    pub held: u8,
    pub on_ground: bool,
    pub dig: Option<Dig>,
    pub delayed: Option<Dig>,
    pub items: std::collections::BTreeMap<i32, ItemEntity>,
    next_entity: i32,
    rng: u64,
    /// Blocks broken, items picked up, clicks handled: the parity table's counts.
    pub broken: u64,
    pub picked: u64,
    pub clicks: u64,
    /// A crafting table's menu, when one is open (issue 11a), and the last container id given.
    pub table: Option<Table>,
    container_counter: i32,
    /// Blocks placed.
    pub placed: u64,
    /// Air, fire, hunger, eating and placed mobs (issue 12a, `life`).
    pub life: crate::life::Life,
}

impl Survival {
    pub fn new(seed: u64) -> Self {
        Self {
            slots: [None; MENU_SLOTS],
            carried: None,
            state_id: 0,
            remote: [None; MENU_SLOTS],
            remote_carried: None,
            held: 0,
            on_ground: false,
            dig: None,
            delayed: None,
            items: std::collections::BTreeMap::new(),
            next_entity: PLAYER_ID + 1,
            rng: seed ^ 0x9e37_79b9_7f4a_7c15,
            broken: 0,
            picked: 0,
            clicks: 0,
            table: None,
            container_counter: 0,
            placed: 0,
            life: crate::life::Life::default(),
        }
    }

    /// splitmix64: the model's own seeded randomness (a function of the run's seed).
    fn next_u64(&mut self) -> u64 {
        self.rng = self.rng.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A double in [0, 1), as `RandomSource.nextDouble`.
    fn next_double(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// `Mth.nextDouble(random, lo, hi)`.
    fn between(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next_double() * (hi - lo)
    }

    /// The next entity id the model gives.
    pub(crate) fn next_id(&mut self) -> i32 {
        let id = self.next_entity;
        self.next_entity += 1;
        id
    }

    pub(crate) fn held_slot_index(&self) -> usize {
        self.held_menu_slot()
    }

    fn held_menu_slot(&self) -> usize {
        HOTBAR + usize::from(self.held)
    }

    /// The held stack.
    pub fn held_stack(&self) -> Option<Stack> {
        self.slots.get(self.held_menu_slot()).copied().flatten()
    }

    /// Give a stack (a checkpoint start's kit, a test): into the inventory as a pickup goes.
    pub fn give(&mut self, stack: Stack) -> i32 {
        self.add(stack)
    }

    /// `Inventory.add`: first into stacks of the same item with room (the selected slot, the
    /// offhand, then inventory order 0-35), then into the first free slot (0-35, the hotbar
    /// first). What did not fit is returned.
    fn add(&mut self, (item, mut count): Stack) -> i32 {
        let order: Vec<usize> = std::iter::once(self.held_menu_slot())
            .chain(std::iter::once(45))
            .chain((0..9).map(|i| HOTBAR + i))
            .chain(9..36)
            .collect();
        for &s in &order {
            if count == 0 {
                return 0;
            }
            if let Some(Some((k, n))) = self.slots.get_mut(s)
                && *k == item
                && *n < max_stack(item)
            {
                let moved = count.min(max_stack(item) - *n);
                *n += moved;
                count -= moved;
            }
        }
        let free: Vec<usize> = (0..9).map(|i| HOTBAR + i).chain(9..36).collect();
        for s in free {
            if count == 0 {
                return 0;
            }
            if let Some(slot) = self.slots.get_mut(s)
                && slot.is_none()
            {
                let moved = count.min(max_stack(item));
                *slot = Some((item, moved));
                count -= moved;
            }
        }
        count
    }
}

fn slot_of(s: Option<Stack>) -> Slot {
    match s {
        Some((item, count)) => Slot::Item {
            item: item.id(),
            count,
        },
        None => Slot::Empty,
    }
}

/// The block types (the blocks report's `definition`) that need the block under them.
const RESTS_ON_BELOW: &[&str] = &[
    "tall_grass",
    "double_plant",
    "tall_flower",
    "flower",
    "flower_bed",
    "leaf_litter",
    "bush",
    "firefly_bush",
    "short_dry_grass",
    "tall_dry_grass",
    "dry_vegetation",
    "sweet_berry_bush",
    "mushroom",
    "sapling",
    "snow_layer",
    "sugar_cane",
    "torch",
    "carpet",
    "wool_carpet",
];

/// Of those, the two-high ones (a lower and an upper half of the same block).
const TWO_HIGH: &[&str] = &["double_plant", "tall_flower"];

/// A state's value for one property: the state's offset from the block's first, with the last
/// property counting fastest (the blocks report's order).
pub fn property(state: u32, name: &str) -> Option<&'static str> {
    let info = data::block_of_state(state)?.info()?;
    let mut offset = state.checked_sub(info.first_state)?;
    for p in info.properties.iter().rev() {
        let n = u32::try_from(p.values.len()).ok()?.max(1);
        let value = p.values.get(usize::try_from(offset % n).ok()?).copied();
        if p.name == name {
            return value;
        }
        offset /= n;
    }
    None
}

fn block_kind(sim: &Sim, (x, y, z): (i32, i32, i32)) -> Option<BlockKind> {
    sim.state_at(x, y, z).and_then(data::block_of_state)
}

fn is_air(k: Option<BlockKind>) -> bool {
    matches!(
        k,
        None | Some(BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir)
    )
}

/// A grid as a recipe sees it: the cells trimmed to the smallest box holding every item.
fn trimmed(width: usize, grid: &[Option<ItemKind>]) -> (usize, usize, Vec<Option<ItemKind>>) {
    let at = |r: usize, c: usize| grid.get(r * width + c).copied().flatten();
    let rows: Vec<usize> = (0..width)
        .filter(|r| (0..width).any(|c| at(*r, c).is_some()))
        .collect();
    let cols: Vec<usize> = (0..width)
        .filter(|c| (0..width).any(|r| at(r, *c).is_some()))
        .collect();
    let (Some(r0), Some(r1), Some(c0), Some(c1)) =
        (rows.first(), rows.last(), cols.first(), cols.last())
    else {
        return (0, 0, Vec::new());
    };
    let cells = (*r0..=*r1)
        .flat_map(|r| (*c0..=*c1).map(move |c| (r, c)))
        .map(|(r, c)| at(r, c))
        .collect();
    (c1 - c0 + 1, r1 - r0 + 1, cells)
}

/// Whether a set of items fills each ingredient once, in any order.
fn shapeless_matches(items: &[ItemKind], ingredients: &[&[ItemKind]]) -> bool {
    if items.len() != ingredients.len() {
        return false;
    }
    fn assign(items: &[ItemKind], ingredients: &[&[ItemKind]], used: &mut Vec<bool>) -> bool {
        let Some((first, rest)) = items.split_first() else {
            return true;
        };
        for (i, ing) in ingredients.iter().enumerate() {
            if used.get(i).copied().unwrap_or(true) || !ing.contains(first) {
                continue;
            }
            if let Some(u) = used.get_mut(i) {
                *u = true;
            }
            if assign(rest, ingredients, used) {
                return true;
            }
            if let Some(u) = used.get_mut(i) {
                *u = false;
            }
        }
        false
    }
    assign(items, ingredients, &mut vec![false; ingredients.len()])
}

/// The crafting result of a square grid (2x2 or 3x3, `width` cells a row), from the generated
/// recipes (shaped, as laid or mirrored, and shapeless).
pub fn craft_result(width: usize, grid: &[Option<ItemKind>]) -> Option<Stack> {
    use data::types::Shape;
    let (w, h, cells) = trimmed(width, grid);
    if cells.is_empty() {
        return None;
    }
    let items: Vec<ItemKind> = cells.iter().flatten().copied().collect();
    data::generated::recipes::RECIPES
        .iter()
        .find(|r| match r.shape {
            Shape::Shaped {
                width,
                height,
                cells: pattern,
            } => {
                if usize::from(width) != w || usize::from(height) != h {
                    return false;
                }
                let fits = |mirror: bool| {
                    (0..h).all(|row| {
                        (0..w).all(|col| {
                            let pc = if mirror { w - 1 - col } else { col };
                            let want = pattern.get(row * w + pc).copied().flatten();
                            let have = cells.get(row * w + col).copied().flatten();
                            match (want, have) {
                                (None, None) => true,
                                (Some(ing), Some(item)) => ing.contains(&item),
                                (None, Some(_)) | (Some(_), None) => false,
                            }
                        })
                    })
                };
                fits(false) || fits(true)
            }
            Shape::Shapeless { ingredients } => shapeless_matches(&items, ingredients),
        })
        .map(|r| (r.result, i32::from(r.count)))
}

impl Sim {
    /// The game mode's dig progress for one tick (`BlockBehaviour.getDestroyProgress` with the
    /// held item, `data::dig`): the eyes in water and the feet off the ground slow it.
    pub fn destroy_progress(&self, kind: BlockKind) -> f32 {
        let eye = [self.pos[0], self.pos[1] + physics::EYE_HEIGHT, self.pos[2]];
        let eye_cell = (
            eye[0].floor() as i32,
            eye[1].floor() as i32,
            eye[2].floor() as i32,
        );
        let eye_in_water = self.eye_in_water(eye[1], eye_cell);
        let held = self.survival.held_stack().map(|(k, _)| k);
        data::dig::progress(kind, held, eye_in_water, self.survival.on_ground)
    }

    /// `isEyeInFluid(WATER)`: the eye below the water's surface in its cell. A water cell (or a
    /// waterlogged block, kelp, seagrass, a bubble column) holds `amount / 9` of a block, the
    /// amount 8 for a source or falling water, `8 - level` for a flow; a full block when water
    /// is above it (`FluidState.getHeight`).
    pub(crate) fn eye_in_water(&self, eye_y: f64, (x, y, z): (i32, i32, i32)) -> bool {
        let Some(state) = self.state_at(x, y, z) else {
            return false;
        };
        let Some(kind) = data::block_of_state(state) else {
            return false;
        };
        let watery = |s: u32, k: BlockKind| {
            matches!(
                k,
                BlockKind::Water
                    | BlockKind::BubbleColumn
                    | BlockKind::Kelp
                    | BlockKind::KelpPlant
                    | BlockKind::Seagrass
                    | BlockKind::TallSeagrass
            ) || property(s, "waterlogged") == Some("true")
        };
        if !watery(state, kind) {
            return false;
        }
        let above = self
            .state_at(x, y + 1, z)
            .and_then(|s| data::block_of_state(s).map(|k| (s, k)));
        let height = if above.is_some_and(|(s, k)| watery(s, k)) {
            1.0
        } else {
            let level = match kind {
                BlockKind::Water => property(state, "level")
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(0),
                _ => 0,
            };
            let amount = if level == 0 || level >= 8 {
                8
            } else {
                8 - level
            };
            f64::from(amount) / 9.0
        };
        eye_y < f64::from(y) + height
    }

    fn within_reach(&self, (x, y, z): (i32, i32, i32)) -> bool {
        let eye = [self.pos[0], self.pos[1] + physics::EYE_HEIGHT, self.pos[2]];
        // The distance from the eye to the block's box, as AABB.distanceToSqr.
        let axis = |e: f64, lo: i32| {
            let lo = f64::from(lo);
            (lo - e).max(0.0).max(e - (lo + 1.0))
        };
        let (dx, dy, dz) = (axis(eye[0], x), axis(eye[1], y), axis(eye[2], z));
        dx * dx + dy * dy + dz * dz < INTERACTION_RANGE * INTERACTION_RANGE
    }

    fn block_update(&self, (x, y, z): (i32, i32, i32)) -> Result<Wire, EncodeError> {
        wire(
            State::Play,
            &S2c::BlockUpdate {
                pos: (x, y, z),
                state: self.state_at(x, y, z).unwrap_or(0),
            },
        )
    }

    /// `handleBlockBreakAction` then `ackBlockChangesUpTo`.
    pub(crate) fn on_player_action(
        &mut self,
        status: i32,
        pos: (i32, i32, i32),
        sequence: i32,
    ) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        if self.within_reach(pos) {
            match status {
                0 => out.extend(self.start_destroy(pos)?),
                2 => out.extend(self.stop_destroy(pos)?),
                1 => {
                    self.survival.dig = None;
                }
                5 => self.on_release_use(),
                other => self.violations.push(format!("player action {other}")),
            }
        }
        out.push(wire(State::Play, &S2c::BlockChangedAck { sequence })?);
        Ok(out)
    }

    fn start_destroy(&mut self, pos: (i32, i32, i32)) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let kind = block_kind(self, pos);
        let progress = match kind {
            Some(k) if !is_air(kind) => self.destroy_progress(k),
            Some(_) | None => 1.0,
        };
        if !is_air(kind) && progress >= 1.0 {
            out.extend(self.destroy_and_ack(pos)?);
            return Ok(out);
        }
        if let Some(old) = self.survival.dig
            && old.pos != pos
        {
            // "abort destroying since another started": the old block is told as it is.
            out.push(self.block_update(old.pos)?);
        }
        self.survival.dig = Some(Dig {
            pos,
            start: self.ticks,
        });
        Ok(out)
    }

    fn stop_destroy(&mut self, pos: (i32, i32, i32)) -> Result<Vec<Wire>, EncodeError> {
        let Some(dig) = self.survival.dig.filter(|d| d.pos == pos) else {
            return Ok(Vec::new());
        };
        let kind = block_kind(self, pos);
        let Some(k) = kind.filter(|_| !is_air(kind)) else {
            return Ok(Vec::new());
        };
        let ticks = self.ticks.saturating_sub(dig.start);
        let done = self.destroy_progress(k) * (ticks as f32 + 1.0);
        if done >= FINISH_ACCEPT {
            self.survival.dig = None;
            return self.destroy_and_ack(pos);
        }
        // An early FINISH is not an abort: the server keeps the dig and breaks the block itself
        // once the progress reaches 1 (`hasDelayedDestroy`), unless one is already pending.
        if self.survival.delayed.is_none() {
            self.survival.dig = None;
            self.survival.delayed = Some(dig);
        }
        Ok(Vec::new())
    }

    /// `destroyAndAck`: break it, or tell the client the block as it is.
    fn destroy_and_ack(&mut self, pos: (i32, i32, i32)) -> Result<Vec<Wire>, EncodeError> {
        match self.destroy_block(pos)? {
            Some(out) => Ok(out),
            None => Ok(vec![self.block_update(pos)?]),
        }
    }

    /// `destroyBlock`: air in its place, the update, and the block's drops.
    fn destroy_block(&mut self, pos: (i32, i32, i32)) -> Result<Option<Vec<Wire>>, EncodeError> {
        let kind = block_kind(self, pos);
        let Some(k) = kind.filter(|_| !is_air(kind)) else {
            return Ok(None);
        };
        if data::generated::hardness::hardness(k).unwrap_or(0.0) < 0.0 {
            return Ok(None);
        }
        let air = BlockKind::Air.info().map_or(0, |i| i.default_state);
        self.world.set_block(pos.0, pos.1, pos.2, air)?;
        self.survival.broken += 1;
        self.mined();
        let mut out = vec![self.block_update(pos)?];
        // setBlock's neighbour updates run before the broken block's own drops: what rested on
        // it pops off first.
        out.extend(self.pop_unsupported((pos.0, pos.1 + 1, pos.2))?);
        let held = self.survival.held_stack().map(|(k, _)| k);
        if data::dig::harvests(k, held) {
            for item in self.roll_drops(k) {
                out.extend(self.pop_resource(pos, (item, 1))?);
            }
        }
        Ok(Some(out))
    }

    /// A block that needs the one under it (`canSurvive`: plants, leaf litter, snow, torches)
    /// breaks when that one goes, dropping its loot (leaf litter and flower beds as many as
    /// their segments); a two-high plant loses its upper half too. Seagrass and kelp turn to
    /// water and are not modelled.
    fn pop_unsupported(&mut self, pos: (i32, i32, i32)) -> Result<Vec<Wire>, EncodeError> {
        let Some(state) = self.state_at(pos.0, pos.1, pos.2) else {
            return Ok(Vec::new());
        };
        let Some(kind) = data::block_of_state(state) else {
            return Ok(Vec::new());
        };
        let Some(info) = kind.info() else {
            return Ok(Vec::new());
        };
        if !RESTS_ON_BELOW.contains(&info.definition) {
            return Ok(Vec::new());
        }
        let air = BlockKind::Air.info().map_or(0, |i| i.default_state);
        self.world.set_block(pos.0, pos.1, pos.2, air)?;
        let mut out = vec![self.block_update(pos)?];
        let upper = (pos.0, pos.1 + 1, pos.2);
        if TWO_HIGH.contains(&info.definition) && block_kind(self, upper) == Some(kind) {
            self.world.set_block(upper.0, upper.1, upper.2, air)?;
            out.push(self.block_update(upper)?);
        }
        let count = property(state, "segment_amount")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(1);
        for item in self.roll_drops(kind) {
            out.extend(self.pop_resource(pos, (item, count))?);
        }
        Ok(out)
    }

    /// The block's first loot pool: tried in order, the first that rolls under its chance.
    fn roll_drops(&mut self, kind: BlockKind) -> Vec<ItemKind> {
        let tries = data::generated::loot::drops_of(kind);
        for d in tries {
            if d.chance >= 1.0 || self.survival.next_double() < d.chance {
                return vec![d.item];
            }
        }
        Vec::new()
    }

    /// `Block.popResource`: an item entity near the block's centre with the constructor's random
    /// velocity and the default pickup delay.
    fn pop_resource(
        &mut self,
        (x, y, z): (i32, i32, i32),
        stack: Stack,
    ) -> Result<Vec<Wire>, EncodeError> {
        let s = &mut self.survival;
        let px = f64::from(x) + 0.5 + s.between(-0.25, 0.25);
        let py = f64::from(y) + 0.5 + s.between(-0.25, 0.25) - ITEM_SIZE / 2.0;
        let pz = f64::from(z) + 0.5 + s.between(-0.25, 0.25);
        let vx = s.next_double() * 0.2 - 0.1;
        let vz = s.next_double() * 0.2 - 0.1;
        self.spawn_item(stack, [px, py, pz], [vx, 0.2, vz], PICKUP_DELAY)
    }

    fn spawn_item(
        &mut self,
        stack: Stack,
        pos: [f64; 3],
        velocity: [f64; 3],
        pickup_delay: u32,
    ) -> Result<Vec<Wire>, EncodeError> {
        let id = self.survival.next_entity;
        self.survival.next_entity += 1;
        let mut uuid = [0u8; 16];
        uuid[..8].copy_from_slice(&self.survival.next_u64().to_be_bytes());
        uuid[8..].copy_from_slice(&self.survival.next_u64().to_be_bytes());
        self.survival.items.insert(
            id,
            ItemEntity {
                stack,
                pos,
                velocity,
                on_ground: false,
                pickup_delay,
                age: 0,
            },
        );
        Ok(vec![
            wire(
                State::Play,
                &S2c::AddEntity {
                    id,
                    uuid,
                    kind: data::EntityKind::Item.id(),
                    x: pos[0],
                    y: pos[1],
                    z: pos[2],
                },
            )?,
            wire(
                State::Play,
                &S2c::SetEntityData {
                    id,
                    entries: vec![(ITEM_STACK_INDEX, MetaValue::Item(slot_of(Some(stack))))],
                },
            )?,
        ])
    }

    /// The game mode's tick: a delayed destroy breaks once its progress reaches 1.
    pub(crate) fn tick_digging(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let Some(d) = self.survival.delayed else {
            return Ok(Vec::new());
        };
        let kind = block_kind(self, d.pos);
        let Some(k) = kind.filter(|_| !is_air(kind)) else {
            self.survival.delayed = None;
            return Ok(Vec::new());
        };
        let ticks = self.ticks.saturating_sub(d.start);
        if self.destroy_progress(k) * (ticks as f32 + 1.0) >= 1.0 {
            self.survival.delayed = None;
            return Ok(self.destroy_block(d.pos)?.unwrap_or_default());
        }
        Ok(Vec::new())
    }

    /// Where an item resting at (x, z) stops falling: the top of what collides under `y`.
    fn floor_under(&self, x: f64, y: f64, z: f64) -> Option<f64> {
        let (cx, cz) = (x.floor() as i32, z.floor() as i32);
        let top = y.floor() as i32;
        (top - 64..=top).rev().find_map(|cy| {
            let h = self.solid(cx, cy, cz)?;
            let surface = f64::from(cy) + h;
            (surface <= y + 1e-9).then_some(surface)
        })
    }

    /// Whether an item's box at (x, y, z) would be inside something that collides.
    fn item_blocked(&self, x: f64, y: f64, z: f64) -> bool {
        let h = ITEM_SIZE / 2.0;
        let cells = |lo: f64, hi: f64| (lo.floor() as i32)..=(hi.floor() as i32);
        cells(x - h, x + h).any(|cx| {
            cells(z - h, z + h).any(|cz| {
                cells(y + 1e-3, y + ITEM_SIZE - 1e-3).any(|cy| {
                    self.solid(cx, cy, cz)
                        .is_some_and(|top| y + 1e-3 < f64::from(cy) + top)
                })
            })
        })
    }

    /// `ItemEntity.tick` for each item: gravity, the move (straight down onto what is under it;
    /// sideways the drift is small and not collided), drag and friction; the delay and the age.
    /// Then `Player.aiStep`'s touch: an item with no delay left inside the grown box is picked
    /// up as far as the inventory has room.
    pub(crate) fn tick_items(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let ids: Vec<i32> = self.survival.items.keys().copied().collect();
        let mut gone = Vec::new();
        for id in ids {
            let Some(mut it) = self.survival.items.get(&id).copied() else {
                continue;
            };
            it.age += 1;
            it.pickup_delay = it.pickup_delay.saturating_sub(1);
            if it.age >= ITEM_LIFETIME {
                gone.push(id);
                continue;
            }
            if !it.on_ground {
                it.velocity[1] -= ITEM_GRAVITY;
                // Sideways, a move into a block is stopped on that axis (Entity.collide, per
                // axis): a drop beside a trunk stays in the dug log's column rather than sinking
                // into the higher ground next to it (found by issue 10c's sim measure).
                let mut x = it.pos[0] + it.velocity[0];
                if self.item_blocked(x, it.pos[1], it.pos[2]) {
                    x = it.pos[0];
                    it.velocity[0] = 0.0;
                }
                let mut z = it.pos[2] + it.velocity[2];
                if self.item_blocked(x, it.pos[1], z) {
                    z = it.pos[2];
                    it.velocity[2] = 0.0;
                }
                let next = [x, it.pos[1] + it.velocity[1], z];
                let floor = self.floor_under(next[0], it.pos[1].max(next[1]), next[2]);
                match floor {
                    Some(f) if next[1] <= f => {
                        it.pos = [next[0], f, next[2]];
                        it.velocity[1] = 0.0;
                        it.on_ground = true;
                    }
                    Some(_) | None => it.pos = next,
                }
            }
            let f = if it.on_ground {
                GROUND_FRICTION
            } else {
                ITEM_DRAG
            };
            it.velocity = [
                it.velocity[0] * f,
                it.velocity[1] * ITEM_DRAG,
                it.velocity[2] * f,
            ];
            self.survival.items.insert(id, it);
            if it.age.is_multiple_of(ITEM_UPDATE_EVERY) {
                out.push(wire(
                    State::Play,
                    &S2c::EntityPositionSync {
                        id,
                        x: it.pos[0],
                        y: it.pos[1],
                        z: it.pos[2],
                    },
                )?);
            }
        }
        out.extend(self.touch_items()?);
        for id in &gone {
            self.survival.items.remove(id);
        }
        if !gone.is_empty() {
            out.push(wire(State::Play, &S2c::RemoveEntities { ids: gone })?);
        }
        Ok(out)
    }

    fn touch_items(&mut self) -> Result<Vec<Wire>, EncodeError> {
        if self.health <= 0.0 {
            return Ok(Vec::new());
        }
        let b = physics::aabb(self.pos);
        let reach = [
            b[0] - TOUCH_SIDEWAYS,
            b[1] - TOUCH_UP,
            b[2] - TOUCH_SIDEWAYS,
            b[3] + TOUCH_SIDEWAYS,
            b[4] + TOUCH_UP,
            b[5] + TOUCH_SIDEWAYS,
        ];
        let mut out = Vec::new();
        let mut taken = Vec::new();
        let touching: Vec<(i32, ItemEntity)> = self
            .survival
            .items
            .iter()
            .filter(|(_, it)| it.pickup_delay == 0)
            .filter(|(_, it)| {
                let h = ITEM_SIZE / 2.0;
                let ib = [
                    it.pos[0] - h,
                    it.pos[1],
                    it.pos[2] - h,
                    it.pos[0] + h,
                    it.pos[1] + ITEM_SIZE,
                    it.pos[2] + h,
                ];
                (0..3).all(|a| physics::overlaps(&reach, &ib, a))
            })
            .map(|(id, it)| (*id, *it))
            .collect();
        for (id, it) in touching {
            let left = self.survival.add(it.stack);
            let amount = it.stack.1 - left;
            if amount == 0 {
                continue;
            }
            self.survival.picked += 1;
            out.push(wire(
                State::Play,
                &S2c::TakeItemEntity {
                    item: id,
                    collector: PLAYER_ID,
                    amount,
                },
            )?);
            if left == 0 {
                self.survival.items.remove(&id);
                taken.push(id);
            } else if let Some(e) = self.survival.items.get_mut(&id) {
                e.stack.1 = left;
            }
        }
        if !taken.is_empty() {
            out.push(wire(State::Play, &S2c::RemoveEntities { ids: taken })?);
        }
        Ok(out)
    }

    /// The menu open now: a crafting table's, else the inventory's (window 0).
    fn menu_id(&self) -> i32 {
        self.survival.table.as_ref().map_or(0, |t| t.id)
    }

    /// Where a menu slot is stored. Window 0: 0 the result, 1-4 the 2x2 grid, 5-45 the inventory's
    /// menu slots. A crafting table's: 0 the result, 1-9 its 3x3 grid, 10-45 the inventory's main
    /// and hotbar slots (window 0's 9-44).
    fn loc(&self, slot: usize) -> Option<Loc> {
        match (self.survival.table.is_some(), slot) {
            (_, 0) => Some(Loc::Result),
            (false, 1..=4) => Some(Loc::Grid(slot - 1)),
            (false, 5..=45) => Some(Loc::Inv(slot)),
            (true, 1..=9) => Some(Loc::Grid(slot - 1)),
            (true, 10..=45) => Some(Loc::Inv(slot - 1)),
            (_, _) => None,
        }
    }

    fn get(&self, l: Loc) -> Option<Stack> {
        match (&self.survival.table, l) {
            (Some(t), Loc::Result) => t.result,
            (Some(t), Loc::Grid(i)) => t.grid.get(i).copied().flatten(),
            (None, Loc::Result) => self.survival.slots.get(RESULT).copied().flatten(),
            (None, Loc::Grid(i)) => self.survival.slots.get(1 + i).copied().flatten(),
            (_, Loc::Inv(i)) => self.survival.slots.get(i).copied().flatten(),
        }
    }

    fn put(&mut self, l: Loc, v: Option<Stack>) {
        let cell = match (&mut self.survival.table, l) {
            (Some(t), Loc::Result) => Some(&mut t.result),
            (Some(t), Loc::Grid(i)) => t.grid.get_mut(i),
            (None, Loc::Result) => self.survival.slots.get_mut(RESULT),
            (None, Loc::Grid(i)) => self.survival.slots.get_mut(1 + i),
            (_, Loc::Inv(i)) => self.survival.slots.get_mut(i),
        };
        if let Some(c) = cell {
            *c = v;
        }
    }

    fn view(&self, slot: usize) -> Option<Stack> {
        self.loc(slot).and_then(|l| self.get(l))
    }

    /// The open menu's grid: its width and items, top left first.
    fn grid(&self) -> (usize, Vec<Option<ItemKind>>) {
        match &self.survival.table {
            Some(t) => (3, t.grid.iter().map(|s| s.map(|(k, _)| k)).collect()),
            None => (
                2,
                GRID.iter()
                    .map(|i| {
                        self.survival
                            .slots
                            .get(*i)
                            .copied()
                            .flatten()
                            .map(|(k, _)| k)
                    })
                    .collect(),
            ),
        }
    }

    fn state_id_mut(&mut self) -> &mut i32 {
        match &mut self.survival.table {
            Some(t) => &mut t.state_id,
            None => &mut self.survival.state_id,
        }
    }

    fn next_state_id(&mut self) -> i32 {
        let s = self.state_id_mut();
        *s += 1;
        *s
    }

    /// The open menu's changes since the client was last told (`broadcastChanges`): one slot
    /// packet per changed slot, each with a new state id, and the cursor when it differs.
    pub(crate) fn broadcast_changes(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let window = self.menu_id();
        for i in 0..MENU_SLOTS {
            let now = self.view(i);
            let remote = match &self.survival.table {
                Some(t) => t.remote.get(i).copied().flatten(),
                None => self.survival.remote.get(i).copied().flatten(),
            };
            if remote == now {
                continue;
            }
            let r = match &mut self.survival.table {
                Some(t) => t.remote.get_mut(i),
                None => self.survival.remote.get_mut(i),
            };
            if let Some(r) = r {
                *r = now;
            }
            let state_id = self.next_state_id();
            out.push(wire(
                State::Play,
                &S2c::ContainerSetSlot {
                    window,
                    state_id,
                    slot: i16::try_from(i).unwrap_or(0),
                    item: slot_of(now),
                },
            )?);
        }
        if self.survival.remote_carried != self.survival.carried {
            self.survival.remote_carried = self.survival.carried;
            out.push(wire(
                State::Play,
                &S2c::SetCursorItem {
                    item: slot_of(self.survival.carried),
                },
            )?);
        }
        Ok(out)
    }

    /// `sendAllDataToRemote`/`broadcastFullState`: the whole open menu with a new state id.
    pub(crate) fn full_state(&mut self) -> Result<Wire, EncodeError> {
        let window = self.menu_id();
        let view: Vec<Option<Stack>> = (0..MENU_SLOTS).map(|i| self.view(i)).collect();
        match &mut self.survival.table {
            Some(t) => {
                for (r, v) in t.remote.iter_mut().zip(&view) {
                    *r = *v;
                }
            }
            None => self.survival.remote = self.survival.slots,
        }
        self.survival.remote_carried = self.survival.carried;
        let state_id = self.next_state_id();
        wire(
            State::Play,
            &S2c::ContainerSetContent {
                window,
                state_id,
                slots: view.iter().map(|s| slot_of(*s)).collect(),
                carried: Some(slot_of(self.survival.carried)),
            },
        )
    }

    /// `slotChangedCraftingGrid`: the result recomputed and sent at once with a new state id.
    fn grid_changed(&mut self) -> Result<Wire, EncodeError> {
        let (width, items) = self.grid();
        let result = craft_result(width, &items);
        self.put(Loc::Result, result);
        match &mut self.survival.table {
            Some(t) => t.remote[RESULT] = result,
            None => self.survival.remote[RESULT] = result,
        }
        let window = self.menu_id();
        let state_id = self.next_state_id();
        wire(
            State::Play,
            &S2c::ContainerSetSlot {
                window,
                state_id,
                slot: 0,
                item: slot_of(result),
            },
        )
    }

    /// `ResultSlot.onTake`: one of each grid ingredient is used up.
    fn take_result(&mut self) -> Result<Wire, EncodeError> {
        let (width, _) = self.grid();
        for i in 0..width * width {
            let v = match self.get(Loc::Grid(i)) {
                Some((k, n)) if n > 1 => Some((k, n - 1)),
                Some(_) | None => None,
            };
            self.put(Loc::Grid(i), v);
        }
        self.grid_changed()
    }

    /// `handleContainerClick` for the open menu: the click, then the changes (or the whole menu
    /// when the client's state id is not the server's). The client's claimed cursor (the bot
    /// claims nothing: empty) becomes the remote one first, as `setRemoteCarried` does.
    pub(crate) fn on_click(
        &mut self,
        window: i32,
        state_id: i32,
        slot: i16,
        button: i8,
        mode: i32,
    ) -> Result<Vec<Wire>, EncodeError> {
        if window != self.menu_id() {
            self.violations
                .push(format!("click in window {window}, open {}", self.menu_id()));
            return Ok(Vec::new());
        }
        self.survival.clicks += 1;
        let current = match &self.survival.table {
            Some(t) => t.state_id,
            None => self.survival.state_id,
        };
        let stale = state_id != current;
        let mut out = self.clicked(slot, button, mode)?;
        self.survival.remote_carried = None;
        if stale {
            out.push(self.full_state()?);
        } else {
            out.extend(self.broadcast_changes()?);
        }
        Ok(out)
    }

    fn clicked(&mut self, slot: i16, button: i8, mode: i32) -> Result<Vec<Wire>, EncodeError> {
        let Ok(i) = usize::try_from(slot) else {
            if slot == -999 {
                self.violations.push("drop from the cursor".to_owned());
            }
            return Ok(Vec::new());
        };
        let Some(l) = self.loc(i) else {
            self.violations.push(format!("click slot {slot}"));
            return Ok(Vec::new());
        };
        if !(button == 0 || button == 1) {
            self.violations.push(format!("click button {button}"));
            return Ok(Vec::new());
        }
        match mode {
            0 => self.pickup_click(l, button == 0),
            1 => self.quick_move(i, l),
            other => {
                self.violations.push(format!("click mode {other}"));
                Ok(Vec::new())
            }
        }
    }

    /// Whether a stack may be put in a slot: never the result; armour slots only armour (none of
    /// issue 11's items).
    fn may_place(l: Loc) -> bool {
        match l {
            Loc::Result => false,
            Loc::Inv(i) => !(5..=8).contains(&i),
            Loc::Grid(_) => true,
        }
    }

    /// Mode 0 (PICKUP), left (`all`) or right.
    fn pickup_click(&mut self, l: Loc, all: bool) -> Result<Vec<Wire>, EncodeError> {
        let s = self.get(l);
        let c = self.survival.carried;
        let mut out = Vec::new();
        let grid_before = self.grid();
        match (s, c) {
            (None, None) => {}
            (None, Some((ck, cn))) => {
                if Self::may_place(l) {
                    let put = if all { cn } else { 1 }.min(max_stack(ck));
                    self.put(l, Some((ck, put)));
                    self.survival.carried = (cn > put).then_some((ck, cn - put));
                }
            }
            (Some((sk, sn)), None) => {
                if l == Loc::Result {
                    self.survival.carried = Some((sk, sn));
                    self.put(l, None);
                    out.push(self.take_result()?);
                } else {
                    let take = if all { sn } else { (sn + 1) / 2 };
                    self.survival.carried = Some((sk, take));
                    self.put(l, (sn > take).then_some((sk, sn - take)));
                }
            }
            (Some((sk, sn)), Some((ck, cn))) => {
                if Self::may_place(l) {
                    if sk == ck {
                        let put = if all { cn } else { 1 }.min(max_stack(sk) - sn);
                        self.put(l, Some((sk, sn + put)));
                        self.survival.carried = (cn > put).then_some((ck, cn - put));
                    } else if cn <= max_stack(ck) {
                        self.put(l, Some((ck, cn)));
                        self.survival.carried = Some((sk, sn));
                    }
                } else if l == Loc::Result && sk == ck && cn + sn <= max_stack(sk) {
                    self.survival.carried = Some((ck, cn + sn));
                    self.put(l, None);
                    out.push(self.take_result()?);
                }
            }
        }
        if l != Loc::Result && self.grid() != grid_before {
            out.push(self.grid_changed()?);
        }
        Ok(out)
    }

    /// `moveItemStackTo(stack, from, to, reverse)` over the open menu's slots: into stacks of the
    /// same item, then empty slots, in order or reversed; what did not fit is returned.
    fn move_to(
        &mut self,
        (item, mut count): Stack,
        range: std::ops::Range<usize>,
        reverse: bool,
    ) -> i32 {
        let order: Vec<usize> = if reverse {
            range.clone().rev().collect()
        } else {
            range.clone().collect()
        };
        let max = max_stack(item);
        for &s in &order {
            if count == 0 {
                break;
            }
            let Some(l) = self.loc(s) else { continue };
            if let Some((k, n)) = self.get(l)
                && k == item
                && n < max
            {
                let moved = count.min(max - n);
                self.put(l, Some((k, n + moved)));
                count -= moved;
            }
        }
        for &s in &order {
            if count == 0 {
                break;
            }
            let Some(l) = self.loc(s) else { continue };
            if self.get(l).is_none() && Self::may_place(l) {
                let moved = count.min(max);
                self.put(l, Some((item, moved)));
                count -= moved;
            }
        }
        count
    }

    /// Mode 1 (QUICK_MOVE). The result crafts again and again into the inventory from the
    /// hotbar's end while it fits whole. Window 0 (`InventoryMenu.quickMoveStack`): the grid and
    /// armour to 9-44, the main inventory to the hotbar, the hotbar to the main inventory. A
    /// table (`CraftingMenu.quickMoveStack`): the grid to 10-45; the inventory into the grid
    /// first, then the main inventory to the hotbar or the hotbar to the main inventory.
    fn quick_move(&mut self, i: usize, l: Loc) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let table = self.survival.table.is_some();
        let inventory = if table { 10..46 } else { 9..45 };
        if l == Loc::Result {
            while let Some(stack) = self.get(Loc::Result) {
                let (saved_slots, saved_table) = (self.survival.slots, self.survival.table.clone());
                if self.move_to(stack, inventory.clone(), true) != 0 {
                    self.survival.slots = saved_slots;
                    self.survival.table = saved_table;
                    break;
                }
                self.put(Loc::Result, None);
                out.push(self.take_result()?);
                if self.get(Loc::Result).map(|s| s.0) != Some(stack.0) {
                    break;
                }
            }
            return Ok(out);
        }
        let Some(stack) = self.get(l) else {
            return Ok(out);
        };
        let grid_before = self.grid();
        self.put(l, None);
        let left = match (table, i) {
            (false, 1..=8) => self.move_to(stack, 9..45, false),
            (false, 9..=35) => self.move_to(stack, 36..45, false),
            (false, _) => self.move_to(stack, 9..36, false),
            (true, 1..=9) => self.move_to(stack, 10..46, false),
            (true, _) => {
                let left = self.move_to(stack, 1..10, false);
                match (left, i) {
                    (0, _) => 0,
                    (n, 10..=36) => self.move_to((stack.0, n), 37..46, false),
                    (n, _) => self.move_to((stack.0, n), 10..37, false),
                }
            }
        };
        if left > 0 {
            match self.get(l) {
                None => self.put(l, Some((stack.0, left))),
                // Moved into its own slot's place by a move into the grid: keep the rest beside it.
                Some(_) => {
                    let rest = self.survival.add((stack.0, left));
                    if rest > 0 {
                        self.violations.push("quick move lost items".to_owned());
                    }
                }
            }
        }
        if self.grid() != grid_before {
            out.push(self.grid_changed()?);
        }
        Ok(out)
    }

    /// The client closed a menu (`removed`): its grid goes back into the inventory, and so does
    /// the cursor (`dropOrPlaceInInventory`: a live player keeps it; only a full inventory drops
    /// what does not fit).
    pub(crate) fn on_close(&mut self, window: i32) -> Result<Vec<Wire>, EncodeError> {
        if window != self.menu_id() {
            return Ok(Vec::new());
        }
        let mut back = Vec::new();
        let (width, _) = self.grid();
        let window0_grid = self.survival.table.is_none();
        for i in 0..width * width {
            if let Some(stack) = self.get(Loc::Grid(i)) {
                self.put(Loc::Grid(i), None);
                back.push(stack);
            }
        }
        if let Some(c) = self.survival.carried.take() {
            back.push(c);
        }
        if window0_grid {
            self.put(Loc::Result, None);
        }
        self.survival.table = None;
        let mut out = Vec::new();
        for stack in back {
            let left = self.survival.add(stack);
            if left > 0 {
                let at = [self.pos[0], self.pos[1] + 1.32, self.pos[2]];
                out.extend(self.spawn_item(
                    (stack.0, left),
                    at,
                    [0.0, 0.1, 0.0],
                    THROW_PICKUP_DELAY,
                )?);
            }
        }
        if window0_grid {
            out.push(self.grid_changed()?);
        }
        Ok(out)
    }

    /// `ServerboundSetCarriedItemPacket`: a slot outside 0-8 is refused.
    pub(crate) fn on_hold(&mut self, slot: i16) {
        match u8::try_from(slot) {
            Ok(s) if s < 9 => self.survival.held = s,
            Ok(_) | Err(_) => self.violations.push(format!("held slot {slot}")),
        }
    }

    /// `handleUseItemOn` (issue 11a): out of reach it does nothing; else a crafting table opens
    /// (`CraftingTableBlock`, the player not sneaking), or a held block item is placed against the
    /// face (`BlockItem.place`: in the clicked cell when that is replaceable, else beside it,
    /// never where a player stands). Then, always, the block updates for the clicked cell and
    /// its neighbour on that face, and the ack.
    pub(crate) fn on_use_item_on(
        &mut self,
        pos: (i32, i32, i32),
        face: i32,
        sequence: i32,
    ) -> Result<Vec<Wire>, EncodeError> {
        let beside = match face {
            0 => (pos.0, pos.1 - 1, pos.2),
            1 => (pos.0, pos.1 + 1, pos.2),
            2 => (pos.0, pos.1, pos.2 - 1),
            3 => (pos.0, pos.1, pos.2 + 1),
            4 => (pos.0 - 1, pos.1, pos.2),
            _ => (pos.0 + 1, pos.1, pos.2),
        };
        let mut out = Vec::new();
        if self.within_reach(pos) {
            if block_kind(self, pos) == Some(BlockKind::CraftingTable) {
                out.extend(self.open_table(pos)?);
            } else {
                self.place_held(pos, beside)?;
            }
        }
        out.push(self.block_update(pos)?);
        out.push(self.block_update(beside)?);
        out.push(wire(State::Play, &S2c::BlockChangedAck { sequence })?);
        Ok(out)
    }

    /// `openMenu` for a crafting table: the next container id (1 to 100, round), `open_screen`,
    /// then the whole menu (`initMenu`).
    fn open_table(&mut self, pos: (i32, i32, i32)) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        if let Some(t) = &self.survival.table {
            // Another menu is open: the server closes it first.
            let id = t.id;
            out.extend(self.on_close(id)?);
            out.push(wire(State::Play, &S2c::ContainerClose { window: id })?);
        }
        let id = self.survival.container_counter % 100 + 1;
        self.survival.container_counter = id;
        self.survival.table = Some(Table {
            id,
            pos,
            grid: [None; 9],
            result: None,
            state_id: 0,
            remote: [None; MENU_SLOTS],
        });
        let menu = data::generated::registries::MENU
            .iter()
            .position(|m| *m == "crafting")
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1);
        out.push(wire(State::Play, &S2c::OpenScreen { window: id, menu })?);
        out.push(self.full_state()?);
        Ok(out)
    }

    /// `BlockItem.place` for the held stack.
    fn place_held(
        &mut self,
        clicked: (i32, i32, i32),
        beside: (i32, i32, i32),
    ) -> Result<(), EncodeError> {
        let held = self.survival.held_menu_slot();
        let Some((item, count)) = self.survival.slots.get(held).copied().flatten() else {
            return Ok(());
        };
        let Some(block) = item.name().and_then(BlockKind::from_name) else {
            return Ok(());
        };
        let replaceable = |k: Option<BlockKind>| {
            is_air(k)
                || k == Some(BlockKind::Water)
                || k.and_then(|k| k.info())
                    .is_some_and(|i| REPLACEABLE.contains(&i.definition))
        };
        let at = if replaceable(block_kind(self, clicked)) {
            clicked
        } else {
            beside
        };
        if !replaceable(block_kind(self, at)) || self.player_in(at) {
            return Ok(());
        }
        let state = block.info().map_or(0, |i| i.default_state);
        self.world.set_block(at.0, at.1, at.2, state)?;
        if let Some(s) = self.survival.slots.get_mut(held) {
            *s = (count > 1).then_some((item, count - 1));
        }
        self.survival.placed += 1;
        Ok(())
    }

    /// Whether the player's box overlaps a block cell (`isUnobstructed` for a full block).
    fn player_in(&self, (x, y, z): (i32, i32, i32)) -> bool {
        let b = physics::aabb(self.pos);
        let c = [
            f64::from(x),
            f64::from(y),
            f64::from(z),
            f64::from(x) + 1.0,
            f64::from(y) + 1.0,
            f64::from(z) + 1.0,
        ];
        (0..3).all(|a| physics::overlaps(&b, &c, a))
    }

    /// `stillValid`: a crafting table's menu closes when the player is more than 8 blocks from
    /// it (or it is gone), told by `container_close`.
    pub(crate) fn tick_menu(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let Some(t) = &self.survival.table else {
            return Ok(Vec::new());
        };
        let (id, (x, y, z)) = (t.id, t.pos);
        let c = [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5];
        let d2: f64 = (0..3).map(|i| (self.pos[i] - c[i]).powi(2)).sum();
        if d2 <= 64.0 && block_kind(self, (x, y, z)) == Some(BlockKind::CraftingTable) {
            return Ok(Vec::new());
        }
        let mut out = self.on_close(id)?;
        out.push(wire(State::Play, &S2c::ContainerClose { window: id })?);
        Ok(out)
    }
}
