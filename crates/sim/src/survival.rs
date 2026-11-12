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
/// Every stack the model makes stacks to 64. The generated tables do not carry
/// `max_stack_size` yet; wood, planks, sticks and tables all stack to 64 (a limit until tools,
/// which stack to 1, are crafted in issue 11).
const MAX_STACK: i32 = 64;

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
                && *n < MAX_STACK
            {
                let moved = count.min(MAX_STACK - *n);
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
                let moved = count.min(MAX_STACK);
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

/// The 2x2 grid as a recipe sees it: the cells trimmed to the smallest box holding every item.
fn trimmed(grid: [Option<ItemKind>; 4]) -> (usize, usize, Vec<Option<ItemKind>>) {
    let at = |r: usize, c: usize| grid.get(r * 2 + c).copied().flatten();
    let rows: Vec<usize> = (0..2)
        .filter(|r| (0..2).any(|c| at(*r, c).is_some()))
        .collect();
    let cols: Vec<usize> = (0..2)
        .filter(|c| (0..2).any(|r| at(r, *c).is_some()))
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

/// The crafting result of a 2x2 grid, from the generated recipes (shaped, as laid or mirrored,
/// and shapeless).
pub fn craft_result(grid: [Option<ItemKind>; 4]) -> Option<Stack> {
    use data::types::Shape;
    let (w, h, cells) = trimmed(grid);
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
    /// The game mode's dig progress for one tick (`BlockBehaviour.getDestroyProgress`): the
    /// destroy speed over the hardness, over 30 when the hand may harvest it, else 100.
    pub fn destroy_progress(&self, kind: BlockKind) -> f32 {
        let hardness = data::generated::hardness::hardness(kind).unwrap_or(0.0);
        if hardness < 0.0 {
            return 0.0;
        }
        if hardness == 0.0 {
            return f32::INFINITY;
        }
        let harvestable = !data::generated::hardness::needs_correct_tool(kind).unwrap_or(false);
        let divisor = if harvestable { 30.0 } else { 100.0 };
        self.destroy_speed() / hardness / divisor
    }

    /// `Player.getDestroySpeed` for the hand (and any item that is not a tool: speed 1.0; tools
    /// are issue 11): times the submerged mining speed 0.2 with the eyes in water, over 5 off
    /// the ground.
    fn destroy_speed(&self) -> f32 {
        let mut speed = 1.0_f32;
        let eye = [self.pos[0], self.pos[1] + physics::EYE_HEIGHT, self.pos[2]];
        let eye_cell = (
            eye[0].floor() as i32,
            eye[1].floor() as i32,
            eye[2].floor() as i32,
        );
        if block_kind(self, eye_cell) == Some(BlockKind::Water) {
            speed *= 0.2;
        }
        if !self.survival.on_ground {
            speed /= 5.0;
        }
        speed
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
        let mut out = vec![self.block_update(pos)?];
        // setBlock's neighbour updates run before the broken block's own drops: what rested on
        // it pops off first.
        out.extend(self.pop_unsupported((pos.0, pos.1 + 1, pos.2))?);
        let harvestable = !data::generated::hardness::needs_correct_tool(k).unwrap_or(false);
        if harvestable {
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
                let next = [
                    it.pos[0] + it.velocity[0],
                    it.pos[1] + it.velocity[1],
                    it.pos[2] + it.velocity[2],
                ];
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

    /// The menu's changes since the client was last told (`broadcastChanges`): one slot packet
    /// per changed slot, each with a new state id, and the cursor when it differs.
    pub(crate) fn broadcast_changes(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        for i in 0..MENU_SLOTS {
            let now = self.survival.slots.get(i).copied().flatten();
            if self.survival.remote.get(i).copied().flatten() == now {
                continue;
            }
            if let Some(r) = self.survival.remote.get_mut(i) {
                *r = now;
            }
            self.survival.state_id += 1;
            out.push(wire(
                State::Play,
                &S2c::ContainerSetSlot {
                    window: 0,
                    state_id: self.survival.state_id,
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

    /// `sendAllDataToRemote`/`broadcastFullState`: the whole window with a new state id.
    pub(crate) fn full_state(&mut self) -> Result<Wire, EncodeError> {
        self.survival.state_id += 1;
        self.survival.remote = self.survival.slots;
        self.survival.remote_carried = self.survival.carried;
        wire(
            State::Play,
            &S2c::ContainerSetContent {
                window: 0,
                state_id: self.survival.state_id,
                slots: self.survival.slots.iter().map(|s| slot_of(*s)).collect(),
                carried: Some(slot_of(self.survival.carried)),
            },
        )
    }

    fn grid(&self) -> [Option<ItemKind>; 4] {
        GRID.map(|i| self.survival.slots.get(i).copied().flatten().map(|s| s.0))
    }

    /// `slotChangedCraftingGrid`: the result recomputed and sent at once with a new state id.
    fn grid_changed(&mut self) -> Result<Wire, EncodeError> {
        let result = craft_result(self.grid());
        if let Some(r) = self.survival.slots.get_mut(RESULT) {
            *r = result;
        }
        if let Some(r) = self.survival.remote.get_mut(RESULT) {
            *r = result;
        }
        self.survival.state_id += 1;
        wire(
            State::Play,
            &S2c::ContainerSetSlot {
                window: 0,
                state_id: self.survival.state_id,
                slot: 0,
                item: slot_of(result),
            },
        )
    }

    /// `ResultSlot.onTake`: one of each grid ingredient is used up.
    fn take_result(&mut self) -> Result<Wire, EncodeError> {
        for i in GRID {
            if let Some(slot) = self.survival.slots.get_mut(i) {
                *slot = match *slot {
                    Some((k, n)) if n > 1 => Some((k, n - 1)),
                    Some(_) | None => None,
                };
            }
        }
        self.grid_changed()
    }

    /// `handleContainerClick` for window 0: the click, then the changes (or the whole window
    /// when the client's state id is not the server's). The client's claimed slots and cursor
    /// become the remote view first, as vanilla's `setRemoteSlotUnsafe`/`setRemoteCarried` do:
    /// the bot claims nothing, so an empty cursor.
    pub(crate) fn on_click(
        &mut self,
        window: i32,
        state_id: i32,
        slot: i16,
        button: i8,
        mode: i32,
    ) -> Result<Vec<Wire>, EncodeError> {
        if window != 0 {
            self.violations.push(format!("click in window {window}"));
            return Ok(Vec::new());
        }
        self.survival.clicks += 1;
        let stale = state_id != self.survival.state_id;
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
        if i >= MENU_SLOTS || !(button == 0 || button == 1) {
            self.violations
                .push(format!("click slot {slot} button {button}"));
            return Ok(Vec::new());
        }
        match mode {
            0 => self.pickup_click(i, button == 0),
            1 => self.quick_move(i),
            other => {
                self.violations.push(format!("click mode {other}"));
                Ok(Vec::new())
            }
        }
    }

    /// Whether a stack may be put in a slot: never the result; armour slots only armour (none
    /// of issue 10's items).
    fn may_place(i: usize) -> bool {
        !(i == RESULT || (5..=8).contains(&i))
    }

    /// Mode 0 (PICKUP), left (`all`) or right.
    fn pickup_click(&mut self, i: usize, all: bool) -> Result<Vec<Wire>, EncodeError> {
        let s = self.survival.slots.get(i).copied().flatten();
        let c = self.survival.carried;
        let mut out = Vec::new();
        let grid_before = self.grid();
        match (s, c) {
            (None, None) => {}
            (None, Some((ck, cn))) => {
                if Self::may_place(i) {
                    let put = if all { cn } else { 1 }.min(MAX_STACK);
                    self.set_slot(i, Some((ck, put)));
                    self.survival.carried = (cn > put).then_some((ck, cn - put));
                }
            }
            (Some((sk, sn)), None) => {
                if i == RESULT {
                    self.survival.carried = Some((sk, sn));
                    self.set_slot(i, None);
                    out.push(self.take_result()?);
                } else {
                    let take = if all { sn } else { (sn + 1) / 2 };
                    self.survival.carried = Some((sk, take));
                    self.set_slot(i, (sn > take).then_some((sk, sn - take)));
                }
            }
            (Some((sk, sn)), Some((ck, cn))) => {
                if Self::may_place(i) {
                    if sk == ck {
                        let put = if all { cn } else { 1 }.min(MAX_STACK - sn);
                        self.set_slot(i, Some((sk, sn + put)));
                        self.survival.carried = (cn > put).then_some((ck, cn - put));
                    } else if cn <= MAX_STACK {
                        self.set_slot(i, Some((ck, cn)));
                        self.survival.carried = Some((sk, sn));
                    }
                } else if i == RESULT && sk == ck && cn + sn <= MAX_STACK {
                    self.survival.carried = Some((ck, cn + sn));
                    self.set_slot(i, None);
                    out.push(self.take_result()?);
                }
            }
        }
        if i != RESULT && self.grid() != grid_before {
            out.push(self.grid_changed()?);
        }
        Ok(out)
    }

    fn set_slot(&mut self, i: usize, v: Option<Stack>) {
        if let Some(s) = self.survival.slots.get_mut(i) {
            *s = v;
        }
    }

    /// `moveItemStackTo(stack, from, to, reverse)`: into stacks of the same item, then empty
    /// slots, in order or reversed; what did not fit is returned.
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
        for &s in &order {
            if count == 0 {
                break;
            }
            if let Some(Some((k, n))) = self.survival.slots.get_mut(s)
                && *k == item
                && *n < MAX_STACK
            {
                let moved = count.min(MAX_STACK - *n);
                *n += moved;
                count -= moved;
            }
        }
        for &s in &order {
            if count == 0 {
                break;
            }
            if let Some(slot) = self.survival.slots.get_mut(s)
                && slot.is_none()
            {
                *slot = Some((item, count));
                count = 0;
            }
        }
        count
    }

    /// Mode 1 (QUICK_MOVE), `InventoryMenu.quickMoveStack`: the result crafts again and again
    /// into slots 9-44 (from the hotbar's end) while it fits whole; the grid and armour go to
    /// 9-44; the main inventory to the hotbar; the hotbar to the main inventory.
    fn quick_move(&mut self, i: usize) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        if i == RESULT {
            while let Some(stack) = self.survival.slots.get(RESULT).copied().flatten() {
                let saved = self.survival.slots;
                if self.move_to(stack, 9..45, true) != 0 {
                    self.survival.slots = saved;
                    break;
                }
                self.set_slot(RESULT, None);
                out.push(self.take_result()?);
                if self
                    .survival
                    .slots
                    .get(RESULT)
                    .copied()
                    .flatten()
                    .map(|s| s.0)
                    != Some(stack.0)
                {
                    break;
                }
            }
            return Ok(out);
        }
        let Some(stack) = self.survival.slots.get(i).copied().flatten() else {
            return Ok(out);
        };
        let grid_before = self.grid();
        let left = match i {
            1..=8 => self.move_to(stack, 9..45, false),
            9..=35 => self.move_to(stack, 36..45, false),
            36..=44 => self.move_to(stack, 9..36, false),
            _ => self.move_to(stack, 9..45, false),
        };
        self.set_slot(i, (left > 0).then_some((stack.0, left)));
        if self.grid() != grid_before {
            out.push(self.grid_changed()?);
        }
        Ok(out)
    }

    /// The client closed window 0 (`InventoryMenu.removed`): the grid goes back into the
    /// inventory and the cursor is thrown.
    pub(crate) fn on_close(&mut self, window: i32) -> Result<Vec<Wire>, EncodeError> {
        if window != 0 {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        let grid_before = self.grid();
        for i in GRID {
            if let Some(stack) = self.survival.slots.get(i).copied().flatten() {
                self.set_slot(i, None);
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
        }
        if self.grid() != grid_before {
            out.push(self.grid_changed()?);
        }
        if let Some(c) = self.survival.carried.take() {
            let at = [self.pos[0], self.pos[1] + 1.32, self.pos[2]];
            out.extend(self.spawn_item(c, at, [0.0, 0.1, 0.0], THROW_PICKUP_DELAY)?);
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
}
