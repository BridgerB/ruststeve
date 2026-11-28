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
