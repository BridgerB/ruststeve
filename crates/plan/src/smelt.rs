//! The smelt task (issue 13a, brain/ruststeve/design/smelting.md): cook a batch in a furnace,
//! believing only what the server says, a port of the core of clojurecraft's smelt.clj.
//!
//! - **Approach** within reach of the furnace and **Open** it (`use_item_on`, then the server's
//!   `open_screen`), as a table craft does.
//! - **Settle**: the cursor put down.
//! - **Fuel** first: what the batch still needs to burn, less what the furnace has burning and in
//!   its fuel slot, from the hottest fuel held (coal before wood), never more items than that
//!   (wood burned is wood the tools lack). One per right-click, the rest put back.
//! - **Input**: the batch, one per right-click, the rest put back.
//! - **Cook**: as results show in the result slot they are shift-clicked out; done when the
//!   batch's results are held. A load is judged by the window's slots, never its state id: the
//!   furnace's own ticks move that.
//!
//! One click per round trip. The cook has a deadline: the batch's cook time by the slowdown,
//! and a margin; past it the task fails `Slow`.

use data::ItemKind;
use world::{Act, BlockPos, World};

use crate::path::Cell;
use crate::walk::{TaskStatus, Walk};

/// The furnace window's own slots.
pub const INPUT: i16 = 0;
pub const FUEL: i16 = 1;
pub const RESULT: i16 = 2;
/// The inventory in the furnace's window: its main 27 then the hotbar.
pub const STORE: std::ops::RangeInclusive<i16> = 3..=38;
/// The furnace window's property for the lit time left.
const LIT_REMAINING: i16 = 0;
/// Ticks to wait for the window after opening, and opens tried.
pub const OPEN_TICKS: u64 = 40;
pub const MAX_OPENS: u32 = 3;
/// A click unanswered this long is missed, and misses before `StaleWindow`.
pub const CLICK_PATIENCE_TICKS: u64 = 60;
pub const MAX_MISSES: u32 = 3;
/// Past the batch's cook time, how long the results may take (clojurecraft's `cook-slack`, 5 s).
pub const COOK_SLACK_TICKS: u64 = 100;
/// The progress invariant: a load is a few clicks a stack; more than this is a loop.
pub const MAX_CLICKS: u32 = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SmeltStage {
    Approach,
    Open { since: u64 },
    Settle,
    Fuel,
    Input,
    Cook { since: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SmeltFailure {
    /// No way to the furnace, or it would not open.
    Unopened,
    /// The input or the fuel is not held.
    Missing,
    /// The fuel slot holds another fuel, or the input slot another item.
    Occupied,
    StaleWindow,
    TooManyClicks,
    /// The results did not come in the batch's cook time and the slack.
    Slow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmeltStatus {
    Running,
    Done,
    Failed(SmeltFailure),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Smelt {
    pub furnace: Cell,
    pub input: ItemKind,
    pub result: ItemKind,
    /// How many to smelt, and the result held when the task began.
    pub count: i32,
    pub before: i32,
    pub cook_ticks: u32,
    pub stage: SmeltStage,
    pub walk: Option<Walk>,
    pub opens: u32,
    pub clicks: u32,
    pub misses: u32,
    pub started: u64,
    /// Only take the results of a batch already loaded (issue 13b: a smelt the planner left).
    #[serde(default)]
    pub collect: bool,
}

/// The fuel the bot burns, hottest first: coal before wood. Logs, sticks and tools are kept for
/// making things (clojurecraft's burn order).
pub fn fuels_held(w: &World) -> Vec<(ItemKind, u32)> {
    let planks = data::generated::tags::item_tag("planks").unwrap_or(&[]);
    let mut v: Vec<(ItemKind, u32)> = crate::held(w)
        .into_iter()
        .filter(|(k, n)| {
            *n > 0
                && (matches!(k, ItemKind::Coal | ItemKind::Charcoal | ItemKind::CoalBlock)
                    || planks.contains(k))
        })
        .filter_map(|(k, _)| data::generated::fuel::fuel_ticks(k).map(|t| (k, t)))
        .collect();
    v.sort_by_key(|(k, t)| (std::cmp::Reverse(*t), k.id()));
    // Sticks over the reserve, last (make's `STICK_RESERVE`).
    if crate::craft::count(w, ItemKind::Stick) > crate::make::STICK_RESERVE {
        v.extend(data::generated::fuel::fuel_ticks(ItemKind::Stick).map(|t| (ItemKind::Stick, t)));
    }
    v
}

impl Smelt {
    pub fn new(
        w: &World,
        furnace: Cell,
        recipe: &data::types::Smelt,
        count: i32,
        tick: u64,
    ) -> Option<Self> {
        let input = *recipe
            .input
            .iter()
            .find(|i| crate::craft::count(w, **i) > 0)?;
        Some(Self {
            furnace,
            input,
            result: recipe.result,
            count,
            before: crate::craft::count(w, recipe.result),
            cook_ticks: recipe.ticks,
            stage: SmeltStage::Approach,
            walk: None,
            opens: 0,
            clicks: 0,
            misses: 0,
            started: tick,
            collect: false,
        })
    }

    /// Go back to a furnace loaded with `count` of a batch whose result is `result`, and take it.
    pub fn collect(w: &World, furnace: Cell, result: ItemKind, count: i32, tick: u64) -> Self {
        Self {
            furnace,
            input: result,
            result,
            count,
            before: crate::craft::count(w, result),
            cook_ticks: 200,
            stage: SmeltStage::Approach,
            walk: None,
            opens: 0,
            clicks: 0,
            misses: 0,
            started: tick,
            collect: true,
        }
    }

    /// The batch is in the furnace (the task is past its loads).
    pub fn loaded(&self) -> bool {
        matches!(self.stage, SmeltStage::Cook { .. })
    }

    fn slot(w: &World, s: i16) -> Option<(ItemKind, i32)> {
        w.window.as_ref().and_then(|win| win.slots.get(&s).copied())
    }

    fn click(&mut self, slot: i16, button: i8, mode: i32) -> (Option<Act>, SmeltStatus) {
        self.clicks += 1;
        if self.clicks > MAX_CLICKS {
            return (None, SmeltStatus::Failed(SmeltFailure::TooManyClicks));
        }
        (
            Some(Act::Click { slot, button, mode }),
            SmeltStatus::Running,
        )
    }

    /// Put the cursor down: on a stack of the same item with room, else an empty slot.
    fn put_down(&mut self, w: &World) -> (Option<Act>, SmeltStatus) {
        let Some((k, _)) = w.inventory.carried else {
            return (None, SmeltStatus::Running);
        };
        let max = data::generated::item_props::max_stack(k);
        let same = STORE
            .clone()
            .find(|s| Self::slot(w, *s).is_some_and(|(h, n)| h == k && n < max));
        let empty = STORE.clone().find(|s| Self::slot(w, *s).is_none());
        match same.or(empty) {
            Some(s) => self.click(s, 0, 0),
            None => (None, SmeltStatus::Failed(SmeltFailure::Missing)),
        }
    }

    /// Fill `slot` to `want` of `item`: pick a stack up from the inventory, one per right-click,
    /// then put the rest back. `None` when the slot holds enough.
    fn fill(
        &mut self,
        w: &World,
        slot: i16,
        item: ItemKind,
        want: i32,
    ) -> Option<(Option<Act>, SmeltStatus)> {
        let have = match Self::slot(w, slot) {
            Some((k, n)) if k == item => n,
            Some(_) => return Some((None, SmeltStatus::Failed(SmeltFailure::Occupied))),
            None => 0,
        };
        let carried = w.inventory.carried;
        if have >= want {
            return carried.map(|_| self.put_down(w));
        }
        match carried {
            Some((k, _)) if k == item => Some(self.click(slot, 1, 0)),
            Some(_) => Some(self.put_down(w)),
            None => match STORE
                .clone()
                .find(|s| Self::slot(w, *s).is_some_and(|(k, _)| k == item))
            {
                Some(s) => Some(self.click(s, 0, 0)),
                None => Some((None, SmeltStatus::Failed(SmeltFailure::Missing))),
            },
        }
    }

    /// The fuel to put in: (item, how many in the slot in all), or `None` when what burns now and
    /// sits in the slot covers the batch.
    fn fuel_plan(&self, w: &World) -> Result<Option<(ItemKind, i32)>, SmeltFailure> {
        let need = i64::from(self.cook_ticks) * i64::from(self.count);
        let burning = w
            .window
            .as_ref()
            .and_then(|win| win.properties.get(&LIT_REMAINING).copied())
            .map_or(0, i64::from);
        let in_slot = Self::slot(w, FUEL);
        let slot_ticks = in_slot.map_or(0, |(k, n)| {
            i64::from(data::generated::fuel::fuel_ticks(k).unwrap_or(0)) * i64::from(n)
        });
        let short = need - burning - slot_ticks;
        if short <= 0 {
            return Ok(None);
        }
        let candidates = fuels_held(w);
        let pick = match in_slot {
            // More of what is in the slot, or nothing.
            Some((k, _)) => candidates.iter().find(|(c, _)| *c == k).copied(),
            None => candidates.first().copied(),
        };
        let Some((fuel, ticks)) = pick else {
            return Err(SmeltFailure::Missing);
        };
        let n = (short + i64::from(ticks) - 1) / i64::from(ticks).max(1);
        let held = crate::craft::count(w, fuel)
            - if fuel == ItemKind::Stick {
                crate::make::STICK_RESERVE
            } else {
                0
            };
        let n = i32::try_from(n).unwrap_or(i32::MAX).min(held.max(0));
        Ok(Some((fuel, in_slot.map_or(0, |(_, h)| h) + n)))
    }

    fn approach(&mut self, w: &World, tick: u64) -> (physics::Controls, Option<Act>, SmeltStatus) {
        let still = physics::Controls::default();
        let Some(body) = w.body else {
            return (still, None, SmeltStatus::Failed(SmeltFailure::Unopened));
        };
        let t = self.furnace;
        let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
        let c = [
            f64::from(t.0) + 0.5,
            f64::from(t.1) + 0.5,
            f64::from(t.2) + 0.5,
        ];
        let d = (0..3).map(|i| (eye[i] - c[i]).powi(2)).sum::<f64>().sqrt();
        if d <= crate::walk::REACH {
            self.walk = None;
            self.stage = SmeltStage::Open { since: 0 };
            return (still, None, SmeltStatus::Running);
        }
        if self.walk.is_none() {
            let mut wk = Walk::to_block(t, tick);
            if !wk.route_now(w, tick) {
                return (still, None, SmeltStatus::Failed(SmeltFailure::Unopened));
            }
            self.walk = Some(wk);
        }
        let Some(wk) = self.walk.as_mut() else {
            return (still, None, SmeltStatus::Running);
        };
        match wk.step(w, tick) {
            (controls, TaskStatus::Running) => (controls, None, SmeltStatus::Running),
            (_, TaskStatus::Done) => {
                self.walk = None;
                self.stage = SmeltStage::Open { since: 0 };
                (still, None, SmeltStatus::Running)
            }
            (_, TaskStatus::Failed(_)) => {
                (still, None, SmeltStatus::Failed(SmeltFailure::Unopened))
            }
        }
    }

    /// One tick: the walk's controls, the act, how it stands. The window is closed as it ends.
    pub fn step(&mut self, w: &World, tick: u64) -> (physics::Controls, Option<Act>, SmeltStatus) {
        let (c, act, status) = self.step_inner(w, tick);
        match (status, act, w.window.is_some()) {
            (SmeltStatus::Done | SmeltStatus::Failed(_), None, true) => {
                (c, Some(Act::CloseWindow), status)
            }
            _ => (c, act, status),
        }
    }

    fn step_inner(
        &mut self,
        w: &World,
        tick: u64,
    ) -> (physics::Controls, Option<Act>, SmeltStatus) {
        let still = physics::Controls::default();
        if crate::craft::count(w, self.result) - self.before >= self.count {
            return (still, None, SmeltStatus::Done);
        }
        match self.stage {
            SmeltStage::Approach => return self.approach(w, tick),
            SmeltStage::Open { since } => {
                if w.window.is_some() && w.click.is_none() {
                    self.stage = SmeltStage::Settle;
                } else if since == 0 || tick.saturating_sub(since) > OPEN_TICKS {
                    if self.opens >= MAX_OPENS {
                        return (still, None, SmeltStatus::Failed(SmeltFailure::Unopened));
                    }
                    self.opens += 1;
                    self.stage = SmeltStage::Open { since: tick.max(1) };
                    let t = self.furnace;
                    let act = Act::UseOn {
                        pos: BlockPos::new(t.0, t.1, t.2),
                        face: 1,
                    };
                    return (still, Some(act), SmeltStatus::Running);
                } else {
                    return (still, None, SmeltStatus::Running);
                }
            }
            SmeltStage::Settle | SmeltStage::Fuel | SmeltStage::Input | SmeltStage::Cook { .. }
                if w.window.is_none() =>
            {
                // The window closed under the task: open it again.
                self.stage = SmeltStage::Open { since: 0 };
                return (still, None, SmeltStatus::Running);
            }
            SmeltStage::Settle | SmeltStage::Fuel | SmeltStage::Input | SmeltStage::Cook { .. } => {
            }
        }
        // One click per round trip.
        if let Some(c) = w.click {
            if tick.saturating_sub(c.at_tick) <= CLICK_PATIENCE_TICKS {
                return (still, None, SmeltStatus::Running);
            }
            self.misses += 1;
            if self.misses >= MAX_MISSES {
                return (still, None, SmeltStatus::Failed(SmeltFailure::StaleWindow));
            }
            return (still, Some(Act::Resync), SmeltStatus::Running);
        }
        let (act, status) = self.stages(w, tick);
        (still, act, status)
    }

    fn stages(&mut self, w: &World, tick: u64) -> (Option<Act>, SmeltStatus) {
        match self.stage {
            SmeltStage::Approach | SmeltStage::Open { .. } => (None, SmeltStatus::Running),
            SmeltStage::Settle => {
                if w.inventory.carried.is_some() {
                    return self.put_down(w);
                }
                self.stage = if self.collect {
                    SmeltStage::Cook { since: tick }
                } else {
                    SmeltStage::Fuel
                };
                self.stages(w, tick)
            }
            SmeltStage::Fuel
                if Self::slot(w, FUEL).is_some_and(|(k, _)| {
                    let held = fuels_held(w);
                    !held.iter().any(|(h, _)| *h == k) && !held.is_empty()
                }) && self.fuel_plan(w).is_err() =>
            {
                // A leftover of a fuel no longer held sits in the slot and the batch needs more:
                // take it out, so the fuel held can go in (a furnace with a plank left and only
                // sticks held: docs/problems/iron-kit.md wall 12).
                self.click(FUEL, 0, 1)
            }
            SmeltStage::Fuel => match self.fuel_plan(w) {
                Err(f) => (None, SmeltStatus::Failed(f)),
                Ok(None) => {
                    if w.inventory.carried.is_some() {
                        return self.put_down(w);
                    }
                    self.stage = SmeltStage::Input;
                    self.stages(w, tick)
                }
                Ok(Some((fuel, want))) => match self.fill(w, FUEL, fuel, want) {
                    Some(r) => r,
                    None => {
                        self.stage = SmeltStage::Input;
                        self.stages(w, tick)
                    }
                },
            },
            SmeltStage::Input => {
                let already = Self::slot(w, INPUT)
                    .filter(|(k, _)| *k == self.input)
                    .map_or(0, |(_, n)| n);
                let want = self.count.min(already + crate::craft::count(w, self.input));
                match self.fill(w, INPUT, self.input, want) {
                    Some(r) => r,
                    None => {
                        self.stage = SmeltStage::Cook { since: tick };
                        (None, SmeltStatus::Running)
                    }
                }
            }
            SmeltStage::Cook { since } => {
                if w.inventory.carried.is_some() {
                    return self.put_down(w);
                }
                if Self::slot(w, RESULT).is_some_and(|(k, _)| k == self.result) {
                    return self.click(RESULT, 0, 1);
                }
                let limit = (f64::from(self.cook_ticks)
                    * f64::from(self.count.max(1))
                    * crate::dig::slowdown(w)) as u64
                    + COOK_SLACK_TICKS;
                if tick.saturating_sub(since) > limit {
                    return (None, SmeltStatus::Failed(SmeltFailure::Slow));
                }
                (None, SmeltStatus::Running)
            }
        }
    }
}
