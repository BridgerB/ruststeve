//! The chest tasks (issue 77, brain/ruststeve/design/chests.md): `Stash` puts held stacks into a
//! chest, `Fetch` takes them out, believing only what the chest's window shows.
//!
//! - **Known first.** A task is made only for a chest memory holds (`memory::ChestSeen`: placed
//!   by the bot or seen with an exposed face). No X-ray: an unseen chest is never a target.
//! - **Approach** within reach and **Open** it (`use_item_on`, then the server's `open_screen`
//!   of a `generic_9x3` or `generic_9x6` window from that block). A chest with a full cube over
//!   it does not open (`ChestBlock.isChestBlockedAt`): the task fails `Blocked` before trying.
//! - **Settle**: the cursor put down.
//! - **Move**: whole stacks by shift-click (`ChestMenu.quickMoveStack`: inventory to the chest's
//!   start, the chest's to the inventory's end), one click per round trip. Each click is checked
//!   by the window's slots: the chest's count of the kind before and after. A click that moved
//!   nothing means the far side is full: that kind ends there.
//! - A stash never moves more than asked: a stack larger than what is left of the count stays
//!   held (said in the ledger). A fetch takes whole stacks until the count is reached; the last
//!   may bring more (a stack is not split).
//! - **Close** at the end, done or failed. Every open, move and close writes a ledger note
//!   (`notes`, drained by the planner).

use data::ItemKind;
use world::{Act, BlockPos, World};

use crate::path::Cell;
use crate::walk::{TaskStatus, Walk};

/// Ticks to wait for the window after opening, and opens tried (the smelt's numbers).
pub const OPEN_TICKS: u64 = crate::smelt::OPEN_TICKS;
pub const MAX_OPENS: u32 = crate::smelt::MAX_OPENS;
/// A click unanswered this long is missed, and misses before `StaleWindow` (the smelt's).
pub const CLICK_PATIENCE_TICKS: u64 = crate::smelt::CLICK_PATIENCE_TICKS;
pub const MAX_MISSES: u32 = crate::smelt::MAX_MISSES;
/// The progress invariant: a double chest is 54 slots and the inventory 36, so one click a
/// stack and a put-down each stays well under this; more is a loop.
pub const MAX_CLICKS: u32 = 120;
/// The inventory's slots at the end of every container window (main 27, hotbar 9).
const INVENTORY: i16 = 36;

/// Which way the stacks go.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Way {
    /// From the inventory into the chest.
    Stash,
    /// From the chest into the inventory.
    Fetch,
}

impl Way {
    fn word(self) -> &'static str {
        match self {
            Way::Stash => "stash",
            Way::Fetch => "fetch",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChestStage {
    Approach,
    Open { since: u64 },
    Settle,
    Move,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChestFailure {
    /// No way to the chest, or it would not open.
    Unopened,
    /// A full cube sits on the chest: the server never opens it.
    Blocked,
    /// The chest is gone from memory (dug, or seen to be something else).
    Gone,
    StaleWindow,
    TooManyClicks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChestStatus {
    Running,
    /// Ended: everything asked moved, or as much as there was and as fits (`moved` says).
    Done,
    Failed(ChestFailure),
}

/// A click in flight: the kind, the slot, and the chest's count of the kind before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Pending {
    kind: ItemKind,
    slot: i16,
    before: i32,
}

/// One kind to move: how many asked, how many moved, and whether it ended (none left, or the far
/// side full).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Want {
    pub kind: ItemKind,
    pub asked: i32,
    pub moved: i32,
    pub ended: bool,
}

/// Stacks moved between the inventory and one chest; `Stash` and `Fetch` are its two ways.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Transfer {
    pub chest: Cell,
    pub way: Way,
    pub wants: Vec<Want>,
    pub stage: ChestStage,
    pub walk: Option<Walk>,
    pub opens: u32,
    pub clicks: u32,
    pub misses: u32,
    pub started: u64,
    pending: Option<Pending>,
    /// Ledger lines made since the planner last took them.
    #[serde(default)]
    pub notes: Vec<String>,
}

fn menu_name(w: &world::Window) -> Option<&'static str> {
    usize::try_from(w.menu)
        .ok()
        .and_then(|i| data::generated::registries::MENU.get(i))
        .copied()
}

/// The chest's own slots in an open chest window: all but the inventory's last 36.
fn own_slots(win: &world::Window) -> std::ops::Range<i16> {
    0..(win.size - INVENTORY).max(0)
}

fn inventory_slots(win: &world::Window) -> std::ops::Range<i16> {
    (win.size - INVENTORY).max(0)..win.size
}

/// A kind's count over a range of the window's slots.
fn count_in(win: &world::Window, range: std::ops::Range<i16>, kind: ItemKind) -> i32 {
    win.slots
        .range(range)
        .filter(|(_, (k, _))| *k == kind)
        .map(|(_, (_, n))| n)
        .sum()
}

/// What a chest window shows, kind and count, for the ledger.
fn shown(win: &world::Window) -> String {
    let mut by: std::collections::BTreeMap<ItemKind, i32> = std::collections::BTreeMap::new();
    for (_, (k, n)) in win.slots.range(own_slots(win)) {
        *by.entry(*k).or_insert(0) += n;
    }
    if by.is_empty() {
        return "nothing".to_owned();
    }
    by.iter()
        .map(|(k, n)| format!("{n} {}", k.name().unwrap_or("?")))
        .collect::<Vec<_>>()
        .join(", ")
}

fn name(k: ItemKind) -> &'static str {
    k.name().unwrap_or("?")
}

impl Transfer {
    /// A transfer with a chest memory holds in the bot's dimension; `None` for any other cell
    /// (no X-ray) or with nothing asked.
    pub fn new(
        w: &World,
        chest: Cell,
        way: Way,
        kinds: &[(ItemKind, i32)],
        tick: u64,
    ) -> Option<Self> {
        let dim = w.dimension.as_ref()?.name.as_str();
        w.memory.chest(dim, chest)?;
        let wants: Vec<Want> = kinds
            .iter()
            .filter(|(_, n)| *n > 0)
            .map(|(kind, asked)| Want {
                kind: *kind,
                asked: *asked,
                moved: 0,
                ended: false,
            })
            .collect();
        if wants.is_empty() {
            return None;
        }
        Some(Self {
            chest,
            way,
            wants,
            stage: ChestStage::Approach,
            walk: None,
            opens: 0,
            clicks: 0,
            misses: 0,
            started: tick,
            pending: None,
            notes: Vec::new(),
        })
    }

    /// How many of a kind moved so far.
    pub fn moved(&self, kind: ItemKind) -> i32 {
        self.wants
            .iter()
            .filter(|x| x.kind == kind)
            .map(|x| x.moved)
            .sum()
    }

    fn known(&self, w: &World) -> bool {
        w.dimension
            .as_ref()
            .is_some_and(|d| w.memory.chest(&d.name, self.chest).is_some())
    }

    /// A full cube over the chest (`isChestBlockedAt`'s `isRedstoneConductor`, taken as a full
    /// collision cube: a slab or glass over it reads as open).
    fn blocked(&self, w: &World) -> bool {
        let (x, y, z) = self.chest;
        w.block_at(BlockPos::new(x, y + 1, z))
            .and_then(data::generated::shapes::collision)
            .is_some_and(|c| c == data::types::Collision::Full)
    }

    /// The chest's window, open now: a chest menu opened from this block.
    fn window<'a>(&self, w: &'a World) -> Option<&'a world::Window> {
        let (x, y, z) = self.chest;
        w.window.as_ref().filter(|win| {
            win.block == Some(BlockPos::new(x, y, z))
                && matches!(menu_name(win), Some("generic_9x3" | "generic_9x6"))
        })
    }

    fn click(&mut self, slot: i16, button: i8, mode: i32) -> (Option<Act>, ChestStatus) {
        self.clicks += 1;
        if self.clicks > MAX_CLICKS {
            return (None, ChestStatus::Failed(ChestFailure::TooManyClicks));
        }
        (
            Some(Act::Click { slot, button, mode }),
            ChestStatus::Running,
        )
    }

    fn approach(&mut self, w: &World, tick: u64) -> (physics::Controls, Option<Act>, ChestStatus) {
        let still = physics::Controls::default();
        let Some(body) = w.body else {
            return (still, None, ChestStatus::Failed(ChestFailure::Unopened));
        };
        let t = self.chest;
        let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
        let c = [
            f64::from(t.0) + 0.5,
            f64::from(t.1) + 0.5,
            f64::from(t.2) + 0.5,
        ];
        let d = (0..3).map(|i| (eye[i] - c[i]).powi(2)).sum::<f64>().sqrt();
        if d <= crate::walk::REACH {
            self.walk = None;
            self.stage = ChestStage::Open { since: 0 };
            return (still, None, ChestStatus::Running);
        }
        if self.walk.is_none() {
            let mut wk = Walk::to_block(t, tick);
            if !wk.route_now(w, tick) {
                self.notes.push(format!(
                    "{} chest at {:?}: no way within reach",
                    self.way.word(),
                    t
                ));
                return (still, None, ChestStatus::Failed(ChestFailure::Unopened));
            }
            self.walk = Some(wk);
        }
        let Some(wk) = self.walk.as_mut() else {
            return (still, None, ChestStatus::Running);
        };
        match wk.step(w, tick) {
            (controls, TaskStatus::Running) => (controls, None, ChestStatus::Running),
            (_, TaskStatus::Done) => {
                self.walk = None;
                self.stage = ChestStage::Open { since: 0 };
                (still, None, ChestStatus::Running)
            }
            (_, TaskStatus::Failed(_)) => {
                self.notes.push(format!(
                    "{} chest at {:?}: the walk there failed",
                    self.way.word(),
                    t
                ));
                (still, None, ChestStatus::Failed(ChestFailure::Unopened))
            }
        }
    }

    /// One tick: the walk's controls, the act, how it stands. The window is closed as it ends,
    /// with a ledger note of what moved and what the chest shows.
    pub fn step(&mut self, w: &World, tick: u64) -> (physics::Controls, Option<Act>, ChestStatus) {
        let (c, act, status) = self.step_inner(w, tick);
        match (status, act) {
            (ChestStatus::Done | ChestStatus::Failed(_), None) => {
                let moved = self
                    .wants
                    .iter()
                    .map(|x| format!("{} of {} {}", x.moved, x.asked, name(x.kind)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let end = match status {
                    ChestStatus::Failed(f) => format!("failed {f:?}"),
                    ChestStatus::Done | ChestStatus::Running => "done".to_owned(),
                };
                match self.window(w) {
                    Some(win) => {
                        self.notes.push(format!(
                            "chest closed at {:?}: {} {end}, {moved}; it shows {}",
                            self.chest,
                            self.way.word(),
                            shown(win)
                        ));
                        (c, Some(Act::CloseWindow), status)
                    }
                    None => {
                        self.notes.push(format!(
                            "{} at {:?} {end}, {moved}",
                            self.way.word(),
                            self.chest
                        ));
                        (c, None, status)
                    }
                }
            }
            _ => (c, act, status),
        }
    }

    fn step_inner(
        &mut self,
        w: &World,
        tick: u64,
    ) -> (physics::Controls, Option<Act>, ChestStatus) {
        let still = physics::Controls::default();
        if !self.known(w) {
            return (still, None, ChestStatus::Failed(ChestFailure::Gone));
        }
        match self.stage {
            ChestStage::Approach => return self.approach(w, tick),
            ChestStage::Open { since } => {
                if let Some(win) = self.window(w)
                    && w.click.is_none()
                {
                    self.notes.push(format!(
                        "chest opened at {:?} for a {} (window {}, {} slots, open {}): it shows {}",
                        self.chest,
                        self.way.word(),
                        win.id,
                        win.size - INVENTORY,
                        self.opens,
                        shown(win)
                    ));
                    self.stage = ChestStage::Settle;
                } else if since == 0 || tick.saturating_sub(since) > OPEN_TICKS {
                    if self.blocked(w) {
                        self.notes.push(format!(
                            "chest at {:?} has a full block over it: it will not open",
                            self.chest
                        ));
                        return (still, None, ChestStatus::Failed(ChestFailure::Blocked));
                    }
                    if self.opens >= MAX_OPENS {
                        return (still, None, ChestStatus::Failed(ChestFailure::Unopened));
                    }
                    self.opens += 1;
                    self.stage = ChestStage::Open { since: tick.max(1) };
                    let t = self.chest;
                    let act = Act::UseOn {
                        pos: BlockPos::new(t.0, t.1, t.2),
                        face: 1,
                    };
                    return (still, Some(act), ChestStatus::Running);
                } else {
                    return (still, None, ChestStatus::Running);
                }
            }
            ChestStage::Settle | ChestStage::Move if self.window(w).is_none() => {
                // The window closed under the task: open it again.
                self.pending = None;
                self.stage = ChestStage::Open { since: 0 };
                return (still, None, ChestStatus::Running);
            }
            ChestStage::Settle | ChestStage::Move => {}
        }
        // One click per round trip.
        if let Some(c) = w.click {
            if tick.saturating_sub(c.at_tick) <= CLICK_PATIENCE_TICKS {
                return (still, None, ChestStatus::Running);
            }
            self.misses += 1;
            if self.misses >= MAX_MISSES {
                return (still, None, ChestStatus::Failed(ChestFailure::StaleWindow));
            }
            return (still, Some(Act::Resync), ChestStatus::Running);
        }
        let (act, status) = self.stages(w);
        (still, act, status)
    }

    /// Put the cursor down: on a stack of the same item with room, else an empty slot, in the
    /// inventory's part of the window.
    fn put_down(&mut self, w: &World, win: &world::Window) -> (Option<Act>, ChestStatus) {
        let Some((k, _)) = w.inventory.carried else {
            return (None, ChestStatus::Running);
        };
        let max = data::generated::item_props::max_stack(k);
        let inv = inventory_slots(win);
        let same = inv
            .clone()
            .find(|s| win.slots.get(s).is_some_and(|(h, n)| *h == k && *n < max));
        let empty = inv.clone().find(|s| !win.slots.contains_key(s));
        match same.or(empty) {
            Some(s) => {
                self.notes.push(format!(
                    "chest at {:?}: the cursor's {} put down in slot {s}",
                    self.chest,
                    name(k)
                ));
                self.click(s, 0, 0)
            }
            None => (None, ChestStatus::Failed(ChestFailure::TooManyClicks)),
        }
    }

    /// The click in flight answered: what it moved, by the chest's slots.
    fn verify(&mut self, win: &world::Window) {
        let Some(p) = self.pending.take() else {
            return;
        };
        let now = count_in(win, own_slots(win), p.kind);
        let delta = match self.way {
            Way::Stash => now - p.before,
            Way::Fetch => p.before - now,
        };
        let way = self.way;
        let chest = self.chest;
        let Some(want) = self.wants.iter_mut().find(|x| x.kind == p.kind) else {
            return;
        };
        want.moved += delta.max(0);
        let line = if delta > 0 {
            format!(
                "{} moved {delta} {} by slot {} at {chest:?}: {} of {}, the chest holds {now}",
                way.word(),
                name(p.kind),
                p.slot,
                want.moved,
                want.asked
            )
        } else {
            want.ended = true;
            let full = match way {
                Way::Stash => "the chest is full",
                Way::Fetch => "the inventory is full",
            };
            format!(
                "{} moved no {} by slot {} at {chest:?}: {full}",
                way.word(),
                name(p.kind),
                p.slot
            )
        };
        self.notes.push(line);
    }

    /// The next stack to shift-click for a want: its slot, or `None` (with a reason) when none
    /// is left to move.
    fn source(&self, win: &world::Window, want: &Want) -> Result<i16, String> {
        let left = want.asked - want.moved;
        let from = match self.way {
            Way::Stash => inventory_slots(win),
            Way::Fetch => own_slots(win),
        };
        let stacks: Vec<(i16, i32)> = win
            .slots
            .range(from)
            .filter(|(_, (k, _))| *k == want.kind)
            .map(|(s, (_, n))| (*s, *n))
            .collect();
        if stacks.is_empty() {
            return Err(format!(
                "no more {} to {}",
                name(want.kind),
                self.way.word()
            ));
        }
        // Room on the far side, read from the window: a click that moves nothing gets no answer
        // from the server (`broadcastChanges` sends no change), so it is never made.
        let (to, full) = match self.way {
            Way::Stash => (own_slots(win), "the chest is full"),
            Way::Fetch => (inventory_slots(win), "the inventory is full"),
        };
        let max = data::generated::item_props::max_stack(want.kind);
        let room = to.clone().any(|s| match win.slots.get(&s) {
            None => true,
            Some((k, n)) => *k == want.kind && *n < max,
        });
        if !room {
            return Err(format!("no room for {}: {full}", name(want.kind)));
        }
        match self.way {
            // The largest whole stack within what is left: never more than asked.
            Way::Stash => stacks
                .iter()
                .filter(|(_, n)| *n <= left)
                .max_by_key(|(s, n)| (*n, std::cmp::Reverse(*s)))
                .map(|(s, _)| *s)
                .ok_or_else(|| {
                    let least = stacks.iter().map(|(_, n)| *n).min().unwrap_or(0);
                    format!(
                        "{} {} left to stash, the smallest stack held is {least}: kept",
                        left,
                        name(want.kind)
                    )
                }),
            // The first stack: a fetch may bring more than asked.
            Way::Fetch => stacks.first().map(|(s, _)| *s).ok_or_else(String::new),
        }
    }

    fn stages(&mut self, w: &World) -> (Option<Act>, ChestStatus) {
        let Some(win) = self.window(w).cloned() else {
            return (None, ChestStatus::Running);
        };
        self.verify(&win);
        if w.inventory.carried.is_some() {
            return self.put_down(w, &win);
        }
        self.stage = ChestStage::Move;
        for i in 0..self.wants.len() {
            let Some(want) = self.wants.get(i).copied() else {
                continue;
            };
            if want.ended || want.moved >= want.asked {
                continue;
            }
            match self.source(&win, &want) {
                Ok(slot) => {
                    self.pending = Some(Pending {
                        kind: want.kind,
                        slot,
                        before: count_in(&win, own_slots(&win), want.kind),
                    });
                    return self.click(slot, 0, 1);
                }
                Err(why) => {
                    if let Some(x) = self.wants.get_mut(i) {
                        x.ended = true;
                    }
                    self.notes
                        .push(format!("{} at {:?}: {why}", self.way.word(), self.chest));
                }
            }
        }
        (None, ChestStatus::Done)
    }
}

/// Put held stacks into a known chest: each kind up to its count, in whole stacks.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stash(pub Transfer);

/// Take stacks out of a known chest: each kind until its count is reached.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fetch(pub Transfer);

impl Stash {
    pub fn new(w: &World, chest: Cell, kinds: &[(ItemKind, i32)], tick: u64) -> Option<Self> {
        Transfer::new(w, chest, Way::Stash, kinds, tick).map(Self)
    }

    pub fn step(&mut self, w: &World, tick: u64) -> (physics::Controls, Option<Act>, ChestStatus) {
        self.0.step(w, tick)
    }
}

impl Fetch {
    pub fn new(w: &World, chest: Cell, kinds: &[(ItemKind, i32)], tick: u64) -> Option<Self> {
        Transfer::new(w, chest, Way::Fetch, kinds, tick).map(Self)
    }

    pub fn step(&mut self, w: &World, tick: u64) -> (physics::Controls, Option<Act>, ChestStatus) {
        self.0.step(w, tick)
    }
}

// >>> stash (issue 77): the planner. When to stash, what, and where (the home chest); when to
// fetch, and what (brain/ruststeve/design/chests.md, "The planner").

use std::collections::BTreeMap;

use world::stage::Stage;

use crate::{Plan, Task};

/// The legs, compiled in (commit-pinned): an edit needs a rebuild.
pub const STASH_TOML: &str = include_str!("../../../config/stash.toml");
/// A chest is 8 planks at a table (the generated `chest` recipe: a ring of 8 `#planks`).
pub const CHEST_PLANKS: i32 = 8;
/// A table is 4 planks (the generated `crafting_table` recipe): wanted too when none is held or
/// at hand.
pub const TABLE_PLANKS: i32 = 4;
/// One log is 4 planks (the generated `*_planks` recipes).
pub const PLANKS_PER_LOG: i32 = 4;
/// Tasks begun for one home chest before it is given up: planks, a table put down, the chest
/// crafted and put down are 4; one failure each besides is 8.
pub const CHEST_TASKS: u32 = 8;
/// Early crafts tried that could not be begun (a trip, no table) before it is left to the portal:
/// each is `FAILURE_MEMORY_TICKS` apart, so 4 cover 2,400 ticks of the run before `ready`.
pub const EARLY_TRIES: u32 = 4;
/// Inventory slots of window 0 a chest window shows (main 9..=35, hotbar 36..=44): what can be
/// stashed. Armour and the off hand are not in a chest's window.
const PACK: std::ops::RangeInclusive<i16> = 9..=44;

/// What a stage span needs of a kind: the largest count any keep.toml row names for a stage in
/// `stages`, its `needs` and its `during` rows (`Keep::need_in`).
pub trait Needs {
    fn need_in(&self, kind: ItemKind, stages: &[Stage]) -> i32;
}

impl Needs for crate::keep::Keep {
    fn need_in(&self, kind: ItemKind, stages: &[Stage]) -> i32 {
        crate::keep::Keep::need_in(self, kind, stages)
    }
}

/// A dangerous leg: entered from `from` through a `portal` block, spanning `stages`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leg {
    pub name: String,
    pub from: String,
    pub portal: data::BlockKind,
    pub stages: Vec<Stage>,
}

impl Leg {
    /// The stages after the leg: every one past its last.
    pub fn after(&self) -> Vec<Stage> {
        let Some(last) = self.stages.iter().max().copied() else {
            return Vec::new();
        };
        Stage::ALL.into_iter().filter(|s| *s > last).collect()
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLegs {
    #[serde(default)]
    leg: Vec<RawLeg>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLeg {
    name: String,
    from: String,
    portal: String,
    stages: Vec<String>,
}

/// The legs from stash.toml's text. An unknown block or stage name is an error.
pub fn parse_legs(text: &str) -> Result<Vec<Leg>, String> {
    let raw: RawLegs = toml::from_str(text).map_err(|e| e.to_string())?;
    raw.leg
        .into_iter()
        .map(|l| {
            let portal = data::BlockKind::from_name(&l.portal)
                .ok_or_else(|| format!("stash.toml: {}: no block {}", l.name, l.portal))?;
            let stages = l
                .stages
                .iter()
                .map(|s| {
                    Stage::from_name(s)
                        .ok_or_else(|| format!("stash.toml: {}: no stage {s}", l.name))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(Leg {
                name: l.name,
                from: l.from,
                portal,
                stages,
            })
        })
        .collect()
}

/// What to stash before `leg`: each kind held that a stage after the leg needs, as many as that
/// need, less what the leg itself needs (its reserve: keep.toml's `during` and `needs` rows for
/// the leg's stages). A kind needed by no stage after the leg is never stashed (it is junk or
/// the leg's own). Kinds in their order.
pub fn stash_list(
    held: &BTreeMap<ItemKind, i32>,
    needs: &dyn Needs,
    leg: &Leg,
) -> Vec<(ItemKind, i32)> {
    let after = leg.after();
    held.iter()
        .filter_map(|(k, h)| {
            let later = needs.need_in(*k, &after);
            if later <= 0 {
                return None;
            }
            let during = needs.need_in(*k, &leg.stages);
            let n = later.min(h - during);
            (n > 0).then_some((*k, n))
        })
        .collect()
}

/// What to fetch from a chest's shown contents: each kind a stage still ahead (`reached` on)
/// needs past what is held, as many as the chest shows.
pub fn fetch_list(
    contents: &BTreeMap<ItemKind, i32>,
    held: &BTreeMap<ItemKind, i32>,
    needs: &dyn Needs,
    reached: Option<Stage>,
) -> Vec<(ItemKind, i32)> {
    let ahead: Vec<Stage> = Stage::ALL
        .into_iter()
        .filter(|s| reached.is_none_or(|r| *s >= r))
        .collect();
    contents
        .iter()
        .filter_map(|(k, n)| {
            let want = needs.need_in(*k, &ahead) - held.get(k).copied().unwrap_or(0);
            let take = want.min(*n);
            (take > 0).then_some((*k, take))
        })
        .collect()
}

/// The wood held as planks: planks, and 4 a log.
pub fn planks_held(held: &BTreeMap<ItemKind, i32>) -> i32 {
    let of = |tag: &str| -> i32 {
        data::generated::tags::item_tag(tag).map_or(0, |items| {
            items
                .iter()
                .map(|k| held.get(k).copied().unwrap_or(0))
                .sum()
        })
    };
    of("planks") + PLANKS_PER_LOG * of("logs")
}

/// The wood to spare for a chest: the planks held less the `ready` stage's logs (the wood the
/// Nether leg carries, `world::stage::READY_LOGS`), and whether it covers the chest (and a table
/// when `table` is false: none held or at hand).
pub fn wood_to_spare(held: &BTreeMap<ItemKind, i32>, table: bool) -> (i32, i32) {
    let ready = i32::try_from(world::stage::READY_LOGS).unwrap_or(i32::MAX);
    let spare = planks_held(held) - PLANKS_PER_LOG * ready;
    let want = CHEST_PLANKS + if table { 0 } else { TABLE_PLANKS };
    (spare, want)
}

/// What the development aim `chest-trip` stashes and fetches back: the parity row's numbers
/// (lead-77, 2026-11-28), those of `plan/tests/stash.rs`: of 138 cobblestone held (64, 64, 10)
/// the 64 and the 10, never the other 64 (a stack over what is left stays held), and 3 string.
pub const TRIP: [(ItemKind, i32); 2] = [(ItemKind::Cobblestone, 74), (ItemKind::String, 3)];

/// The development aim `chest-trip`'s steps, in order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TripStep {
    /// The held chest put down (it becomes the home chest).
    #[default]
    Place,
    Stash,
    Fetch,
    /// Fetched: the aim is met.
    Over,
}

/// The planner's chest state: the home chest, the legs passed, a fetch due.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StashState {
    /// The home chest: its dimension and cell (memory holds the chest itself).
    pub home: Option<(String, Cell)>,
    /// A chest is being put down: the chests memory held before, to tell the new one.
    pub placing: Option<Vec<Cell>>,
    /// A table was put down for the chest: taken along again after.
    pub table_placed: bool,
    /// Tasks begun for the home chest in hand (`CHEST_TASKS`).
    pub chest_tasks: u32,
    /// Legs whose trigger has run: stashed, or skipped with a reason in the ledger.
    pub legs: Vec<String>,
    /// A death and respawn happened: what is wanted is fetched from the home chest.
    pub fetch_due: bool,
    /// When a fetch waiting for safety may next say so.
    pub wait_noted_at: u64,
    /// Stashes and fetches ended, and how many items each way.
    pub stashes: u32,
    pub fetches: u32,
    /// The early craft (wall 5): the wood spare when it began, over (held, or given up), the
    /// tries that could not be begun and when the next may be.
    #[serde(default)]
    pub early_spare: Option<i32>,
    #[serde(default)]
    pub early_over: bool,
    #[serde(default)]
    pub early_tries: u32,
    #[serde(default)]
    pub early_retry_at: u64,
    /// The stocking's "+N logs for the home chest" note was made.
    #[serde(default)]
    pub logs_noted: bool,
    /// The development aim `chest-trip`'s next step.
    #[serde(default)]
    pub trip: TripStep,
}

/// Whether the eye is within `d` of a cell's centre.
fn within(w: &World, c: Cell, d: f64) -> bool {
    w.body.is_some_and(|b| {
        let eye = [b.pos[0], b.pos[1] + physics::EYE_HEIGHT, b.pos[2]];
        let m = [
            f64::from(c.0) + 0.5,
            f64::from(c.1) + 0.5,
            f64::from(c.2) + 0.5,
        ];
        (0..3).map(|i| (eye[i] - m[i]).powi(2)).sum::<f64>().sqrt() <= d
    })
}

/// The pack's stacks (what a chest window shows of the inventory), by kind.
fn pack(w: &World) -> BTreeMap<ItemKind, i32> {
    let mut c = BTreeMap::new();
    for (_, (k, n)) in w.inventory.slots.range(PACK) {
        *c.entry(*k).or_insert(0) += n;
    }
    c
}

fn list_text(l: &[(ItemKind, i32)]) -> String {
    l.iter()
        .map(|(k, n)| format!("{n} {}", name(*k)))
        .collect::<Vec<_>>()
        .join(", ")
}

impl Plan {
    /// The need table: the keep table (config/keep.toml), or none when it could not be read
    /// (nothing is stashed or fetched then, noted).
    fn stash_needs(&self) -> Option<std::sync::Arc<crate::keep::Keep>> {
        self.keep.table.as_ref().ok().cloned()
    }

    /// The danger that skips a chest trip: under the danger line, or a foe in range.
    fn stash_danger(&self, w: &World) -> Option<String> {
        let rules = &self.table.as_ref()?.survival;
        if crate::defend::in_danger(w, rules) {
            return Some(format!(
                "health {:.1} at or under the danger line {}",
                w.player.health.unwrap_or(0.0),
                rules.danger_health
            ));
        }
        crate::defend::foe(w, rules, &[]).map(|(id, k)| format!("a foe in range ({k:?} {id})"))
    }

    /// The one call site, as the planner chooses a task: a fetch due after a respawn, then a
    /// stash before a dangerous leg's portal; else the task chosen.
    pub fn stash_gate(&mut self, w: &World, chosen: Task, tick: u64) -> Task {
        // The chosen task's reason, the tick's last note before the gate's: written again after
        // a skip, so the task ledger's reason for it stays its own (`track` takes the tick's
        // last note).
        let (before, reason) = (self.event_count, self.events.last().cloned());
        if let Some(t) = self.stash_choice(w, &chosen, tick) {
            return t;
        }
        if self.event_count > before
            && !matches!(chosen, Task::Idle)
            && let Some(e) = reason.filter(|e| e.tick == tick)
        {
            self.note(tick, e.what, e.why);
        }
        chosen
    }

    /// The gate's own choice: a fetch due, or a leg's stash; `None` leaves the chosen task.
    fn stash_choice(&mut self, w: &World, chosen: &Task, tick: u64) -> Option<Task> {
        if let Some(t) = self.fetch_after_revive(w, tick) {
            return Some(t);
        }
        if !matches!(chosen, Task::Enter(_))
            && let Some(t) = self.early_chest(w, tick)
        {
            return Some(t);
        }
        let Task::Enter(e) = chosen else {
            return None;
        };
        let legs = parse_legs(STASH_TOML).ok()?;
        let portal = |c: Cell| {
            w.block_at(BlockPos::new(c.0, c.1, c.2))
                .or_else(|| w.block_at(BlockPos::new(c.0, c.1 - 1, c.2)))
        };
        let here = portal(e.cell);
        let from = e.from.as_deref();
        let leg = legs
            .into_iter()
            .find(|l| from == Some(l.from.as_str()) && here == Some(l.portal))?;
        self.stash_before(w, &leg, tick)
    }

    /// Before `leg`: the stash into the home chest of what is needed after it and not in it, the
    /// home chest made first when there is none near. `None` once the trigger has run (stashed,
    /// or skipped, each noted): the leg goes on.
    pub fn stash_before(&mut self, w: &World, leg: &Leg, tick: u64) -> Option<Task> {
        if self.stash.legs.contains(&leg.name) {
            return None;
        }
        let done = |p: &mut Plan, what: String, why: String| {
            p.stash.legs.push(leg.name.clone());
            p.note(tick, what, why);
            None
        };
        let Some(needs) = self.stash_needs() else {
            return done(
                self,
                format!("stash before the {} skipped", leg.name),
                "the keep table could not be read: nothing says what is needed after the leg"
                    .to_owned(),
            );
        };
        let list = stash_list(&pack(w), needs.as_ref(), leg);
        if list.is_empty() {
            return done(
                self,
                format!("stash before the {}: nothing to put away", leg.name),
                "nothing held is needed after the leg past what the leg needs".to_owned(),
            );
        }
        if let Some(why) = self.stash_danger(w) {
            return done(
                self,
                format!("stash before the {} skipped: danger", leg.name),
                format!("{why}; {} kept", list_text(&list)),
            );
        }
        let dim = w.dimension.as_ref()?.name.clone();
        self.home_placed(w, &dim, tick);
        let home = self
            .stash
            .home
            .clone()
            .filter(|(d, c)| *d == dim && within(w, *c, crate::TABLE_SEARCH));
        if let Some((_, c)) = home {
            // The table put down for the chest comes along first.
            if std::mem::take(&mut self.stash.table_placed)
                && let Some(t) = self.carry_table(w, tick)
            {
                return Some(t);
            }
            return match Stash::new(w, c, &list, tick) {
                Some(s) => {
                    self.stash.legs.push(leg.name.clone());
                    self.note(
                        tick,
                        format!(
                            "stash before the {} at {c:?}: {}",
                            leg.name,
                            list_text(&list)
                        ),
                        "needed after the leg and not in it; the leg's own kept".to_owned(),
                    );
                    Some(Task::Stash(Box::new(s)))
                }
                None => done(
                    self,
                    format!("stash before the {} skipped", leg.name),
                    format!("the home chest {c:?} is not in memory"),
                ),
            };
        }
        match self.home_chest_task(w, tick) {
            Ok(t) => Some(t),
            Err(why) => done(
                self,
                format!("stash before the {} skipped: no home chest", leg.name),
                format!("{why}; {} kept", list_text(&list)),
            ),
        }
    }

    /// The chest just put down becomes the home chest: the nearest chest memory holds that it
    /// did not hold before the place (no X-ray: memory has it from the block update).
    fn home_placed(&mut self, w: &World, dim: &str, tick: u64) {
        let Some(before) = self.stash.placing.clone() else {
            return;
        };
        if !matches!(self.task, Task::Idle) {
            return;
        }
        let new = w
            .memory
            .chests(dim)
            .into_iter()
            .map(|(c, _)| c)
            .filter(|c| !before.contains(c) && within(w, *c, crate::TABLE_SEARCH))
            .min_by_key(|c| {
                w.body.map_or(0, |b| {
                    let f = crate::path::feet_cell(b.pos);
                    (c.0 - f.0).abs() + (c.1 - f.1).abs() + (c.2 - f.2).abs()
                })
            });
        self.stash.placing = None;
        if let Some(c) = new {
            self.stash.home = Some((dim.to_owned(), c));
            self.stash.chest_tasks = 0;
            self.note(
                tick,
                format!("home chest at {c:?}"),
                "the chest put down for the stash".to_owned(),
            );
        }
    }

    /// The home chest crafted early (wall 5: bots reach the portal under `READY_LOGS`, so a chest
    /// begun there never had the wood): the first time the wood spares in the overworld before
    /// the `ready` stage, the `carry-chest` row where the bot stands, never a trip; the chest
    /// is carried and put down at the portal. Noted when held ("home chest crafted early").
    fn early_chest(&mut self, w: &World, tick: u64) -> Option<Task> {
        if self.stash.early_over || self.stash.home.is_some() || tick < self.stash.early_retry_at {
            return None;
        }
        let overworld = w
            .dimension
            .as_ref()
            .is_some_and(|d| d.name == crate::stronghold::OVERWORLD);
        let counts = crate::held(w);
        let held = counts.get(&ItemKind::Chest).copied().unwrap_or(0);
        if let Some(spare) = self.stash.early_spare
            && held > 0
        {
            self.stash.early_spare = None;
            self.stash.early_over = true;
            self.stash.chest_tasks = 0;
            self.note(
                tick,
                format!("home chest crafted early, {spare} planks spare"),
                "carried to the portal, put down there before the stash".to_owned(),
            );
            if std::mem::take(&mut self.stash.table_placed) {
                return self.carry_table(w, tick);
            }
            return None;
        }
        let begun = self.stash.early_spare.is_some();
        if !overworld || held > 0 || self.stash.legs.contains(&"nether".to_owned()) {
            return None;
        }
        if !begun {
            let at_table = self
                .at_hand(w, data::BlockKind::CraftingTable, tick)
                .is_some()
                || counts.get(&ItemKind::CraftingTable).is_some_and(|n| *n > 0);
            let (spare, want) = wood_to_spare(&counts, at_table);
            if spare < want || self.stash_danger(w).is_some() {
                return None;
            }
            self.stash.early_spare = Some(spare);
        }
        match self.chest_row_task(
            w,
            tick,
            "carry-chest",
            "the wood spares before the portal: the home chest crafted early, carried",
        ) {
            Ok(t) => Some(t),
            Err(why) => {
                self.stash.early_spare = None;
                self.stash.table_placed = false;
                self.stash.chest_tasks = 0;
                self.stash.early_tries += 1;
                self.stash.early_retry_at = tick + crate::FAILURE_MEMORY_TICKS;
                if self.stash.early_tries >= EARLY_TRIES {
                    self.stash.early_over = true;
                }
                self.note(
                    tick,
                    "home chest early craft skipped".to_owned(),
                    format!(
                        "{why}; try {} of {EARLY_TRIES}, the portal makes one if it can",
                        self.stash.early_tries
                    ),
                );
                None
            }
        }
    }

    /// The logs the home chest still wants on top of a stocking's (wall 5, race 46153646063:
    /// bots from `gear` gathered exactly `READY_LOGS` and never had wood to spare): the chest's
    /// 8 planks, 12 with no table held or at hand, less the planks held, in logs of 4 planks. 0
    /// with a home chest placed or carried, the early craft over, outside the overworld, or the
    /// Nether leg's trigger run. The stocking gathers them on its own trip (`clean_flow` adds
    /// them to its want); noted once, the first time the logs held reach `READY_LOGS`.
    pub fn chest_logs(&mut self, w: &World) -> i32 {
        let overworld = w
            .dimension
            .as_ref()
            .is_some_and(|d| d.name == crate::stronghold::OVERWORLD);
        let counts = crate::held(w);
        let carried = counts.get(&ItemKind::Chest).is_some_and(|n| *n > 0);
        if !overworld
            || carried
            || self.stash.home.is_some()
            || self.stash.early_over
            || self.stash.legs.contains(&"nether".to_owned())
        {
            return 0;
        }
        let table = self
            .at_hand(w, data::BlockKind::CraftingTable, w.ticks)
            .is_some()
            || counts.get(&ItemKind::CraftingTable).is_some_and(|n| *n > 0);
        let want = CHEST_PLANKS + if table { 0 } else { TABLE_PLANKS };
        let planks = data::generated::tags::item_tag("planks").map_or(0, |items| {
            items
                .iter()
                .map(|k| counts.get(k).copied().unwrap_or(0))
                .sum::<i32>()
        });
        let n = ((want - planks).max(0) + PLANKS_PER_LOG - 1) / PLANKS_PER_LOG;
        let ready = i32::try_from(world::stage::READY_LOGS).unwrap_or(i32::MAX);
        let held = crate::logs_held(w);
        if n > 0 && held >= ready && !self.stash.logs_noted {
            self.stash.logs_noted = true;
            self.note(
                w.ticks,
                format!(
                    "stock: +{n} logs for the home chest (held {held}, want {})",
                    ready + n
                ),
                format!(
                    "the chest's {want} planks past the leg's {ready} logs, {planks} planks held: gathered on this stocking's trip"
                ),
            );
        }
        n
    }

    /// The next step toward a home chest where the bot stands (the `home-chest` row), or why
    /// none is begun: no wood to spare, a trip, or the tasks' bound.
    fn home_chest_task(&mut self, w: &World, tick: u64) -> Result<Task, String> {
        self.chest_row_task(
            w,
            tick,
            "home-chest",
            "something to stash and no chest near",
        )
    }

    /// The next step of a chest row (`home-chest`: put down; `carry-chest`: crafted and
    /// carried) where the bot stands, or why none is begun.
    fn chest_row_task(
        &mut self,
        w: &World,
        tick: u64,
        row_id: &str,
        why: &str,
    ) -> Result<Task, String> {
        let table = self.table.clone().ok_or("no goal table")?;
        let goal = table
            .until(row_id)
            .ok_or(format!("no {row_id} row in the goal table"))?;
        if self.stash.chest_tasks >= CHEST_TASKS {
            return Err(format!("{CHEST_TASKS} tasks begun for it"));
        }
        let counts = crate::held(w);
        let at_table = self.at_hand(w, data::BlockKind::CraftingTable, tick);
        // The first step only: once begun, the wood is in planks, a table or the chest. A chest
        // carried (crafted early) needs no wood.
        let carried = counts.get(&ItemKind::Chest).is_some_and(|n| *n > 0);
        if self.stash.chest_tasks == 0 && !carried {
            let has_table =
                at_table.is_some() || counts.get(&ItemKind::CraftingTable).is_some_and(|n| *n > 0);
            let (spare, want) = wood_to_spare(&counts, has_table);
            if spare < want {
                return Err(format!(
                    "wood to spare {spare} planks of {want} (the leg's {} logs kept)",
                    world::stage::READY_LOGS
                ));
            }
        }
        let table_near = at_table.is_some();
        let near = move |b: data::BlockKind| b == data::BlockKind::CraftingTable && table_near;
        let choice = crate::make::make_with(&table, &counts, &near, false, goal);
        let row = match choice {
            crate::make::Choice::Act(r) => r,
            crate::make::Choice::Done => return Err("the needs planner has it held".to_owned()),
            crate::make::Choice::Stuck => return Err("not makeable from what is held".to_owned()),
        };
        let task = match row.act {
            Some(crate::make::Act::Craft(r)) if crate::make::fits_grid(r) => {
                Task::Craft(crate::craft::Craft::new(r, tick))
            }
            Some(crate::make::Act::Craft(r)) => match at_table {
                Some(t) => Task::Craft(crate::craft::Craft::at_table(r, t, tick)),
                None => return Err(format!("craft {} wants a table at hand", r.id)),
            },
            Some(crate::make::Act::Place(b)) => {
                let p = crate::place::Place::new(b, tick).ok_or(format!("{b:?} has no item"))?;
                if b == data::BlockKind::Chest {
                    let dim = w.dimension.as_ref().map_or("", |d| d.name.as_str());
                    self.stash.placing =
                        Some(w.memory.chests(dim).into_iter().map(|(c, _)| c).collect());
                } else if b == data::BlockKind::CraftingTable {
                    self.stash.table_placed = true;
                }
                Task::Place(p)
            }
            Some(
                crate::make::Act::Gather
                | crate::make::Act::Mine { .. }
                | crate::make::Act::Smelt(_)
                | crate::make::Act::Redig(_)
                | crate::make::Act::Scoop,
            )
            | None => return Err(format!("{} would be a trip", row.id)),
        };
        self.stash.chest_tasks += 1;
        self.note(
            tick,
            format!("home chest: {}", row.id),
            format!(
                "{why}; task {} of at most {CHEST_TASKS}",
                self.stash.chest_tasks
            ),
        );
        Ok(task)
    }

    /// After a death and a respawn: what the stages ahead want of the home chest's shown
    /// contents, fetched, unless danger holds (it waits, noted) or nothing is wanted (noted).
    fn fetch_after_revive(&mut self, w: &World, tick: u64) -> Option<Task> {
        if !self.stash.fetch_due {
            return None;
        }
        let dim = w.dimension.as_ref()?.name.clone();
        let over = |p: &mut Plan, what: &str, why: String| {
            p.stash.fetch_due = false;
            p.note(tick, format!("fetch after the respawn {what}"), why);
            None
        };
        let Some((hd, c)) = self.stash.home.clone() else {
            return over(self, "skipped", "no home chest".to_owned());
        };
        if hd != dim {
            return over(self, "skipped", format!("the home chest is in {hd}"));
        }
        let Some(contents) = w.memory.chest(&dim, c).and_then(|s| s.contents.clone()) else {
            return over(
                self,
                "skipped",
                format!("the home chest {c:?} never shown open"),
            );
        };
        let Some(needs) = self.stash_needs() else {
            return over(
                self,
                "skipped",
                "the keep table could not be read".to_owned(),
            );
        };
        let list = fetch_list(&contents, &pack(w), needs.as_ref(), w.stage_reached);
        if list.is_empty() {
            return over(
                self,
                "not needed",
                format!("nothing the chest {c:?} shows is wanted by a stage ahead"),
            );
        }
        if let Some(why) = self.stash_danger(w) {
            if tick >= self.stash.wait_noted_at {
                self.stash.wait_noted_at = tick + crate::FAILURE_MEMORY_TICKS;
                self.note(
                    tick,
                    "fetch after the respawn waits".to_owned(),
                    format!("{why}; {} wanted", list_text(&list)),
                );
            }
            return None;
        }
        let f = Fetch::new(w, c, &list, tick)?;
        self.stash.fetch_due = false;
        self.note(
            tick,
            format!("fetch after the respawn at {c:?}: {}", list_text(&list)),
            "wanted by a stage ahead and not held".to_owned(),
        );
        Some(Task::Fetch(Box::new(f)))
    }

    /// The development aim `chest-trip` (parity only; never a frontier start nor a race's
    /// evidence): the held chest put down, `TRIP` stashed in it, then fetched back, each a task
    /// of the planner's own and each noted; met when the fetch has ended.
    pub(crate) fn chest_trip_aim(&mut self, w: &World, tick: u64) -> Task {
        let Some(dim) = w.dimension.as_ref().map(|d| d.name.clone()) else {
            return Task::Idle;
        };
        self.home_placed(w, &dim, tick);
        let met = |p: &mut Plan, why: String| {
            p.stash.trip = TripStep::Over;
            p.met = true;
            p.note(tick, "chest trip met".to_owned(), why);
            Task::Idle
        };
        match self.stash.trip {
            TripStep::Place => {
                if let Some((_, c)) = self.stash.home.clone() {
                    self.stash.trip = TripStep::Stash;
                    self.note(
                        tick,
                        format!("chest trip: the chest at {c:?} is down"),
                        "stash next".to_owned(),
                    );
                    return Task::Idle;
                }
                if self.stash.placing.is_some() || self.stash.chest_tasks >= CHEST_TASKS {
                    return met(self, "the chest was not put down".to_owned());
                }
                let Some(pl) = crate::place::Place::new(data::BlockKind::Chest, tick)
                    .filter(|_| crate::craft::count(w, ItemKind::Chest) > 0)
                else {
                    return met(self, "no chest held".to_owned());
                };
                self.stash.chest_tasks += 1;
                self.stash.placing =
                    Some(w.memory.chests(&dim).into_iter().map(|(c, _)| c).collect());
                self.note(
                    tick,
                    "chest trip: place the chest".to_owned(),
                    "the development aim chest-trip, its first step".to_owned(),
                );
                Task::Place(pl)
            }
            TripStep::Stash | TripStep::Fetch => {
                let Some((_, c)) = self.stash.home.clone() else {
                    return met(self, "the chest is gone".to_owned());
                };
                let stash = self.stash.trip == TripStep::Stash;
                let t = if stash {
                    Stash::new(w, c, &TRIP, tick).map(|s| Task::Stash(Box::new(s)))
                } else {
                    Fetch::new(w, c, &TRIP, tick).map(|f| Task::Fetch(Box::new(f)))
                };
                let Some(t) = t else {
                    return met(self, format!("the chest at {c:?} is not in memory"));
                };
                self.stash.trip = if stash {
                    TripStep::Fetch
                } else {
                    TripStep::Over
                };
                self.note(
                    tick,
                    format!(
                        "chest trip: {} {} at {c:?}",
                        if stash { "stash" } else { "fetch" },
                        list_text(&TRIP)
                    ),
                    "the development aim chest-trip".to_owned(),
                );
                t
            }
            TripStep::Over => met(
                self,
                format!(
                    "{} stashes and {} fetches ended",
                    self.stash.stashes, self.stash.fetches
                ),
            ),
        }
    }

    /// The chest task in hand, one tick: its acts sent, its notes into the ledger, its end
    /// handled. The task loop's one call site for `Task::Stash` and `Task::Fetch`.
    pub(crate) fn chest_step(&mut self, w: &World, tick: u64) -> physics::Controls {
        let (c, act, status, notes, t) = if let Task::Stash(s) = &mut self.task {
            let (c, act, status) = s.step(w, tick);
            (c, act, status, std::mem::take(&mut s.0.notes), s.0.clone())
        } else if let Task::Fetch(f) = &mut self.task {
            let (c, act, status) = f.step(w, tick);
            (c, act, status, std::mem::take(&mut f.0.notes), f.0.clone())
        } else {
            return physics::Controls::default();
        };
        self.acts.extend(act);
        for n in notes {
            self.note(tick, n, format!("{} (issue 77)", t.way.word()));
        }
        let moved: i32 = t.wants.iter().map(|x| x.moved).sum();
        match status {
            ChestStatus::Running => return c,
            ChestStatus::Done => {
                self.failed_in_row = 0;
                match t.way {
                    Way::Stash => self.stash.stashes += 1,
                    Way::Fetch => self.stash.fetches += 1,
                }
                self.note(
                    tick,
                    format!("{} done: {moved} items at {:?}", t.way.word(), t.chest),
                    "back to the leg".to_owned(),
                );
            }
            ChestStatus::Failed(f) => {
                *self.failures.entry(format!("{f:?}")).or_default() += 1;
                self.retry_at = tick + crate::RETRY_TICKS;
                // A chest gone or covered is no home any more.
                if matches!(f, ChestFailure::Gone | ChestFailure::Blocked)
                    && self.stash.home.as_ref().is_some_and(|(_, h)| *h == t.chest)
                {
                    self.stash.home = None;
                }
                self.note(
                    tick,
                    format!("{} failed: {f:?}, {moved} items moved", t.way.word()),
                    "the leg goes on without it".to_owned(),
                );
            }
        }
        self.task = Task::Idle;
        physics::Controls::default()
    }
}
// <<< stash

// >>> stash (issue 77): the trigger at a real leg's portal, on the server model.
#[cfg(test)]
mod tests {
    use super::*;
    use protocol::decode;
    use sim::{Config, In, Sim, flat_world};
    use world::{Effect, Event};

    const CONFIG: Config = Config {
        view_distance: 2,
        simulation_distance: 2,
        peaceful: true,
    };

    /// The bot and the model joined on a flat world with a nether portal block at `PORTAL`.
    struct Pair {
        w: World,
        sim: Sim,
        tick: u64,
    }

    const PORTAL: Cell = (3, 64, 0);

    impl Pair {
        fn new(give: &[(ItemKind, i32)]) -> Self {
            let mut m = flat_world(3, 63).unwrap();
            let portal = data::BlockKind::NetherPortal.info().unwrap().default_state;
            m.set_block(PORTAL.0, PORTAL.1, PORTAL.2, portal).unwrap();
            Self::on(m, give)
        }

        fn on(m: sim::ModelWorld, give: &[(ItemKind, i32)]) -> Self {
            let mut sim = Sim::new(CONFIG, m).seeded(11);
            for g in give {
                assert_eq!(sim.survival.give(*g), 0);
            }
            let mut p = Self {
                w: World::new(),
                sim,
                tick: 0,
            };
            let protocol = i32::try_from(data::VERSION.protocol).unwrap();
            let mut first = Vec::new();
            for c in protocol::session::opening(protocol, "model", 0, "rs-gate") {
                first.extend(p.sim.step(&In::Packet(c)).unwrap());
            }
            p.deliver(first);
            for _ in 0..40 {
                p.step(Vec::new());
            }
            assert!(p.w.body.is_some(), "joined and placed");
            p
        }

        fn deliver(&mut self, mut to_bot: Vec<sim::Wire>) {
            for _ in 0..64 {
                if to_bot.is_empty() {
                    return;
                }
                let mut to_model = Vec::new();
                for wire in to_bot.drain(..) {
                    if let Ok(p) = decode(wire.state, wire.id, &wire.body) {
                        to_model.extend(self.w.step(&Event::Packet(p)));
                    }
                }
                for e in to_model {
                    if let Effect::Send(c) = e {
                        to_bot.extend(self.sim.step(&In::Packet(c)).unwrap());
                    }
                }
            }
        }

        fn step(&mut self, acts: Vec<Act>) {
            let mut to_bot = self.sim.step(&In::Tick).unwrap();
            for a in acts {
                for e in self.w.step(&Event::Act(a)) {
                    if let Effect::Send(c) = e {
                        to_bot.extend(self.sim.step(&In::Packet(c)).unwrap());
                    }
                }
            }
            for e in self.w.step(&Event::Tick {
                now_ms: self.tick * 50,
                seed: self.tick | 1,
            }) {
                if let Effect::Send(c) = e {
                    to_bot.extend(self.sim.step(&In::Packet(c)).unwrap());
                }
            }
            self.deliver(to_bot);
            self.tick += 1;
        }
    }

    fn planner() -> Plan {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
        let goals: crate::make::Goals =
            toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let table = crate::make::Table::new(&goals).unwrap();
        Plan::with_table(crate::Aim::Race, std::sync::Arc::new(table))
    }

    /// The walk into the portal, as `nether_task` makes it (its note first).
    fn enter(plan: &mut Plan, w: &World, tick: u64) -> Task {
        let mut walk = Walk::new(
            PORTAL,
            crate::path::Goal::Block { pos: PORTAL },
            crate::walk::Arrive::Goal,
            tick,
        );
        assert!(walk.route_now(w, tick), "a way to the portal");
        plan.note(
            tick,
            format!("enter the portal at {PORTAL:?}"),
            "a lit portal seen".to_owned(),
        );
        Task::Enter(Box::new(crate::Enter {
            walk,
            cell: PORTAL,
            inside_since: None,
            from: w.dimension.as_ref().map(|d| d.name.clone()),
        }))
    }

    fn whats(plan: &Plan) -> Vec<String> {
        plan.events.iter().map(|e| e.what.clone()).collect()
    }

    /// A bot at the Nether portal with the bow's feathers to stash and only the leg's 4 logs:
    /// no home chest was ever placed and none can be, so the stash is skipped with a note, the
    /// walk in goes ahead with its own reason, and the trigger never runs again for the leg.
    #[test]
    fn at_the_portal_without_a_home_chest_the_stash_is_skipped_and_the_run_goes_on() {
        let mut p = Pair::new(&[(ItemKind::OakLog, 4), (ItemKind::Feather, 16)]);
        let mut plan = planner();
        let t = enter(&mut plan, &p.w, p.tick);
        let out = plan.stash_gate(&p.w, t, p.tick);
        assert!(matches!(out, Task::Enter(_)), "the walk in goes ahead");
        let what = whats(&plan);
        assert!(
            what.iter()
                .any(|x| x == "stash before the nether skipped: no home chest"),
            "{what:?}"
        );
        let why = plan
            .events
            .iter()
            .map(|e| e.why.clone())
            .collect::<Vec<_>>();
        assert!(
            why.iter()
                .any(|x| x.contains("wood to spare 0 planks of 12") && x.contains("16 feather")),
            "{why:?}"
        );
        // The tick's last note is the walk's own again: the task ledger's reason for it.
        assert_eq!(
            what.last().map(String::as_str),
            Some("enter the portal at (3, 64, 0)")
        );
        assert_eq!(plan.stash.legs, vec!["nether".to_owned()]);
        assert!(plan.stash.home.is_none());
        // The run goes on: the walk runs, no chest task is begun, the gate is quiet after.
        plan.begin(out);
        let notes = plan.event_count;
        for _ in 0..20 {
            plan.step(&p.w);
            let acts = plan.take_acts();
            p.step(acts);
            assert!(
                !matches!(
                    plan.task,
                    Task::Craft(_) | Task::Place(_) | Task::Stash(_) | Task::Fetch(_)
                ),
                "{}",
                plan.task.name()
            );
        }
        let again = enter(&mut plan, &p.w, p.tick);
        let before = plan.event_count;
        assert!(matches!(
            plan.stash_gate(&p.w, again, p.tick),
            Task::Enter(_)
        ));
        assert_eq!(plan.event_count, before, "no note the second time");
        assert!(plan.event_count >= notes);
        assert_eq!(crate::craft::count(&p.w, ItemKind::Feather), 16);
    }

    /// The same portal with nothing to stash: noted as such, no chest begun.
    #[test]
    fn at_the_portal_with_nothing_to_stash_no_chest_is_begun() {
        let p = Pair::new(&[(ItemKind::OakPlanks, 64), (ItemKind::CraftingTable, 1)]);
        let mut plan = planner();
        let t = enter(&mut plan, &p.w, p.tick);
        assert!(matches!(plan.stash_gate(&p.w, t, p.tick), Task::Enter(_)));
        let what = whats(&plan);
        assert!(
            what.iter()
                .any(|x| x == "stash before the nether: nothing to put away"),
            "{what:?}"
        );
        assert!(plan.stash.home.is_none());
    }

    fn ledger(plan: &Plan) -> String {
        plan.events
            .iter()
            .map(|e| format!("{}: {} ({})", e.tick, e.what, e.why))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Wall 5 (race 46153646063): the ready stocking's log want grows by the chest's logs while
    /// no home chest is held, carried or placed, noted once in the director's words, and drops
    /// back to the stocking's own once the chest is held.
    #[test]
    fn the_stockings_log_want_grows_by_the_chests_logs_until_one_is_held() {
        let mut p = Pair::new(&[(ItemKind::OakLog, 4), (ItemKind::CraftingTable, 1)]);
        let mut plan = planner();
        assert_eq!(plan.chest_logs(&p.w), 2, "8 planks with a table held");
        assert_eq!(plan.chest_logs(&p.w), 2, "the same on every ask");
        let log = ledger(&plan);
        assert_eq!(
            log.matches("stock: +2 logs for the home chest (held 4, want 6)")
                .count(),
            1,
            "{log}"
        );
        // No table: the table's 4 planks too.
        let q = Pair::new(&[(ItemKind::OakLog, 4)]);
        assert_eq!(planner().chest_logs(&q.w), 3);
        // Planks held count toward it.
        let r = Pair::new(&[(ItemKind::OakPlanks, 4), (ItemKind::CraftingTable, 1)]);
        assert_eq!(planner().chest_logs(&r.w), 1);
        // A chest held: none.
        assert_eq!(p.sim.survival.give((ItemKind::Chest, 1)), 0);
        for _ in 0..5 {
            p.step(Vec::new());
        }
        assert_eq!(plan.chest_logs(&p.w), 0, "carried");
        // A home chest placed, or the Nether leg's trigger run: none.
        let mut placed = planner();
        placed.stash.home = Some(("minecraft:overworld".to_owned(), (0, 64, 0)));
        assert_eq!(placed.chest_logs(&q.w), 0);
        let mut gone = planner();
        gone.stash.legs.push("nether".to_owned());
        assert_eq!(gone.chest_logs(&q.w), 0);
    }

    /// Wall 5: a geared bot with the kit and the leg's 4 logs, a three-log tree beside it: the
    /// ready stocking gathers the chest's 2 logs on its own gather, the chest is crafted early
    /// and carried, and the logs left still make `ready`.
    #[test]
    fn a_bot_gathering_its_ready_logs_ends_with_the_chest_carried() {
        let mut m = flat_world(3, 63).unwrap();
        let log = data::BlockKind::OakLog.info().unwrap().default_state;
        for y in 64..67 {
            m.set_block(4, y, 3, log).unwrap();
        }
        let mut p = Pair::on(
            m,
            &[
                (ItemKind::IronHelmet, 1),
                (ItemKind::IronChestplate, 1),
                (ItemKind::IronLeggings, 1),
                (ItemKind::IronSword, 1),
                (ItemKind::WaterBucket, 1),
                (ItemKind::Bucket, 1),
                (ItemKind::FlintAndSteel, 1),
                (ItemKind::StonePickaxe, 1),
                (ItemKind::OakLog, 4),
                (ItemKind::CraftingTable, 1),
                (ItemKind::CookedBeef, 30),
            ],
        );
        let mut plan = planner();
        let mut done = false;
        for _ in 0..6000 {
            plan.step(&p.w);
            let acts = plan.take_acts();
            p.step(acts);
            if crate::craft::count(&p.w, ItemKind::Chest) > 0 && plan.stash.early_over {
                done = true;
                break;
            }
        }
        let text = ledger(&plan);
        assert!(done, "{text}");
        assert!(
            text.contains("stock: +2 logs for the home chest (held 4, want 6)"),
            "{text}"
        );
        assert!(text.contains("home chest crafted early"), "{text}");
        assert_eq!(crate::craft::count(&p.w, ItemKind::Chest), 1);
        assert!(crate::logs_held(&p.w) >= 4, "{text}");
        assert_eq!(plan.chest_logs(&p.w), 0);
    }
}
// <<< stash
