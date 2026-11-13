//! The craft task (brain/ruststeve/design/crafting-window.md): one run of a 2x2 recipe in
//! window 0, believing only what the server says. clojurecraft's stages as a state inside the
//! task:
//!
//! - **Settle**: the cursor put down in an empty slot, the grid emptied by shift-clicks;
//! - **Place**: the recipe laid at the grid's top left, one item a cell by right-clicks, the
//!   cursor put back when it holds what no cell still wants;
//! - **Verify**: slot 0 must hold exactly the recipe's result before it is taken;
//! - **Take**: shift-click it out; done when the inventory holds more of it.
//!
//! One click per round trip: the next goes only once the last moved window 0's state id (a
//! click that changes nothing draws no reply, so none is sent). A click unanswered for
//! `CLICK_PATIENCE_TICKS` is an expectation missed: the window is asked for again and the task
//! starts over from Settle; the third miss fails `StaleWindow`.
//!
//! A recipe that needs the 3x3 grid (issue 11b) is made at a remembered crafting table: walk
//! within reach of it, open it (`use_item_on`, then the server's `open_screen`), run the same
//! stages on its window (result 0, grid 1-9, the inventory 10-45), and close it.

use data::ItemKind;
use data::types::{Recipe, Shape};
use world::{Act, BlockPos, World};

use crate::path::Cell;
use crate::walk::{TaskStatus, Walk};

/// A click unanswered this long is missed (clojurecraft's 3 s stale-window deadline).
pub const CLICK_PATIENCE_TICKS: u64 = 60;
/// Misses before the task fails `StaleWindow`.
pub const MAX_MISSES: u32 = 3;
/// The progress invariant: a craft needs at most a click per cell, two per source stack and a
/// few to settle and take; more than this is a loop, and the task fails `TooManyClicks`.
pub const MAX_CLICKS: u32 = 40;
/// Ticks Verify waits for the result to show before it calls the result wrong.
pub const VERIFY_TICKS: u64 = 20;

/// The result slot.
const RESULT: i16 = 0;
/// Ticks to wait for the table's window after opening it, before trying again.
pub const OPEN_TICKS: u64 = 40;
/// Opens tried before the task fails `Unopened`.
pub const MAX_OPENS: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Stage {
    /// Walking within reach of the table (a table craft).
    Approach,
    /// The table clicked open at `since`; waiting for its window.
    Open {
        since: u64,
    },
    Settle,
    Place,
    Verify {
        since: u64,
    },
    Take {
        before: i32,
    },
}

/// Why a craft failed: data, for the planner and the case file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CraftFailure {
    /// Three clicks went unanswered.
    StaleWindow,
    /// Slot 0 did not hold the recipe's result.
    WrongResult,
    /// An ingredient the layout wants is not held.
    Missing,
    /// The progress invariant: too many clicks for one run.
    TooManyClicks,
    /// No way to the table, or it would not open.
    Unopened,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CraftStatus {
    Running,
    Done,
    Failed(CraftFailure),
}

/// A craft in progress.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Craft {
    pub recipe: String,
    pub stage: Stage,
    pub clicks: u32,
    pub misses: u32,
    pub started: u64,
    /// The crafting table it is made at, for a 3x3 recipe, the walk to it and the opens tried.
    #[serde(default)]
    pub table: Option<Cell>,
    #[serde(default)]
    pub walk: Option<Walk>,
    #[serde(default)]
    pub opens: u32,
}

/// The recipe's layout at the top left of a grid `side` cells wide (2: window 0, 3: a table):
/// each grid slot (1 + row × side + column) and the items it accepts.
pub fn layout(r: &Recipe, side: usize) -> Vec<(i16, &'static [ItemKind])> {
    let slot = |i: usize| i16::try_from(1 + i).unwrap_or(i16::MAX);
    match r.shape {
        Shape::Shaped { width, cells, .. } => cells
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let w = usize::from(width.max(1));
                let (row, col) = (i / w, i % w);
                c.map(|set| (slot(row * side + col), set))
            })
            .collect(),
        Shape::Shapeless { ingredients } => ingredients
            .iter()
            .enumerate()
            .map(|(i, set)| (slot(i), *set))
            .collect(),
    }
}

fn recipe_of(id: &str) -> Option<&'static Recipe> {
    data::generated::recipes::RECIPES
        .iter()
        .find(|r| r.id == id)
}

/// The grid's side, its slots and where stacks are taken from and put back, for the window the
/// craft works in.
fn frame(table: bool) -> (usize, Vec<i16>, std::ops::RangeInclusive<i16>) {
    if table {
        (3, (1..=9).collect(), 10..=45)
    } else {
        (2, (1..=4).collect(), 9..=44)
    }
}

/// The face of the table toward a position: the axis the position is farthest off the table's
/// centre along, on its side (down 0, up 1, north 2, south 3, west 4, east 5).
fn face_toward(pos: [f64; 3], (x, y, z): Cell) -> i8 {
    let d = [
        pos[0] - (f64::from(x) + 0.5),
        pos[1] + physics::EYE_HEIGHT - (f64::from(y) + 0.5),
        pos[2] - (f64::from(z) + 0.5),
    ];
    let (ax, ay, az) = (d[0].abs(), d[1].abs(), d[2].abs());
    if ax >= ay && ax >= az {
        if d[0] > 0.0 { 5 } else { 4 }
    } else if az >= ay {
        if d[2] > 0.0 { 3 } else { 2 }
    } else if d[1] > 0.0 {
        1
    } else {
        0
    }
}

/// How many of an item the player holds outside the grid and the result.
pub fn count(w: &World, item: ItemKind) -> i32 {
    w.inventory
        .slots
        .iter()
        .filter(|(s, _)| **s >= 5)
        .filter(|(_, (k, _))| *k == item)
        .map(|(_, (_, n))| *n)
        .sum::<i32>()
        + w.inventory
            .carried
            .filter(|(k, _)| *k == item)
            .map_or(0, |(_, n)| n)
}

impl Craft {
    pub fn new(recipe: &Recipe, tick: u64) -> Self {
        Self {
            recipe: recipe.id.to_owned(),
            stage: Stage::Settle,
            clicks: 0,
            misses: 0,
            started: tick,
            table: None,
            walk: None,
            opens: 0,
        }
    }

    /// A craft at the crafting table at `table` (a 3x3 recipe).
    pub fn at_table(recipe: &Recipe, table: Cell, tick: u64) -> Self {
        Self {
            stage: Stage::Approach,
            table: Some(table),
            ..Self::new(recipe, tick)
        }
    }

    /// A slot of the window the craft works in.
    fn held(&self, w: &World, slot: i16) -> Option<(ItemKind, i32)> {
        match (&self.table, &w.window) {
            (Some(_), Some(win)) => win.slots.get(&slot).copied(),
            (Some(_), None) => None,
            (None, _) => w.inventory.slots.get(&slot).copied(),
        }
    }

    /// Put the cursor down: on a stack of the same item with room, else an empty slot.
    fn put_down(&mut self, w: &World) -> (Option<Act>, CraftStatus) {
        let Some((k, _)) = w.inventory.carried else {
            return (None, CraftStatus::Running);
        };
        let (_, _, store) = frame(self.table.is_some());
        let max = data::generated::item_props::max_stack(k);
        let same = store
            .clone()
            .find(|s| self.held(w, *s).is_some_and(|(hk, n)| hk == k && n < max));
        let empty = store.clone().find(|s| self.held(w, *s).is_none());
        match same.or(empty) {
            Some(s) => self.click(s, 0, 0),
            None => (None, CraftStatus::Failed(CraftFailure::Missing)),
        }
    }

    fn click(&mut self, slot: i16, button: i8, mode: i32) -> (Option<Act>, CraftStatus) {
        self.clicks += 1;
        if self.clicks > MAX_CLICKS {
            return (None, CraftStatus::Failed(CraftFailure::TooManyClicks));
        }
        (
            Some(Act::Click { slot, button, mode }),
            CraftStatus::Running,
        )
    }

    /// One tick: the walk's controls (on the way to a table), the act to send, if any, and how
    /// the craft stands. A table craft closes the window as it ends, done or not.
    pub fn step(&mut self, w: &World, tick: u64) -> (physics::Controls, Option<Act>, CraftStatus) {
        let (controls, act, status) = self.step_inner(w, tick);
        let open = self.table.is_some() && w.window.is_some();
        match (status, act, open) {
            (CraftStatus::Done | CraftStatus::Failed(_), None, true) => {
                (controls, Some(Act::CloseWindow), status)
            }
            _ => (controls, act, status),
        }
    }

    fn approach(
        &mut self,
        w: &World,
        tick: u64,
        table: Cell,
    ) -> (physics::Controls, Option<Act>, CraftStatus) {
        let still = physics::Controls::default();
        let Some(body) = w.body else {
            return (still, None, CraftStatus::Failed(CraftFailure::Unopened));
        };
        let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
        let c = [
            f64::from(table.0) + 0.5,
            f64::from(table.1) + 0.5,
            f64::from(table.2) + 0.5,
        ];
        let d = (0..3).map(|i| (eye[i] - c[i]).powi(2)).sum::<f64>().sqrt();
        if d <= crate::walk::REACH {
            self.walk = None;
            self.stage = Stage::Open { since: 0 };
            return (still, None, CraftStatus::Running);
        }
        if self.walk.is_none() {
            let mut wk = Walk::to_block(table, tick);
            if !wk.route_now(w, tick) {
                return (still, None, CraftStatus::Failed(CraftFailure::Unopened));
            }
            self.walk = Some(wk);
        }
        let Some(wk) = self.walk.as_mut() else {
            return (still, None, CraftStatus::Running);
        };
        match wk.step(w, tick) {
            (controls, TaskStatus::Running) => (controls, None, CraftStatus::Running),
            (_, TaskStatus::Done) => {
                self.walk = None;
                self.stage = Stage::Open { since: 0 };
                (still, None, CraftStatus::Running)
            }
            (_, TaskStatus::Failed(_)) => {
                (still, None, CraftStatus::Failed(CraftFailure::Unopened))
            }
        }
    }

    fn step_inner(
        &mut self,
        w: &World,
        tick: u64,
    ) -> (physics::Controls, Option<Act>, CraftStatus) {
        let still = physics::Controls::default();
        match (self.stage, self.table) {
            (Stage::Approach, Some(t)) => return self.approach(w, tick, t),
            (Stage::Open { since }, Some(t)) => {
                if w.window.is_some() && w.click.is_none() {
                    self.stage = Stage::Settle;
                } else if since == 0 || tick.saturating_sub(since) > OPEN_TICKS {
                    if self.opens >= MAX_OPENS {
                        return (still, None, CraftStatus::Failed(CraftFailure::Unopened));
                    }
                    self.opens += 1;
                    self.stage = Stage::Open { since: tick.max(1) };
                    let face = w.body.map_or(1, |b| face_toward(b.pos, t));
                    let act = Act::UseOn {
                        pos: BlockPos::new(t.0, t.1, t.2),
                        face,
                    };
                    return (still, Some(act), CraftStatus::Running);
                } else {
                    return (still, None, CraftStatus::Running);
                }
            }
            (_, Some(_)) if w.window.is_none() => {
                // The table's window closed under the craft: open it again.
                self.stage = Stage::Open { since: 0 };
                return (still, None, CraftStatus::Running);
            }
            _ => {}
        }
        let (act, status) = self.stages(w, tick);
        (still, act, status)
    }

    /// The window stages, the same in window 0 and a table's window.
    fn stages(&mut self, w: &World, tick: u64) -> (Option<Act>, CraftStatus) {
        let Some(recipe) = recipe_of(&self.recipe) else {
            return (None, CraftStatus::Failed(CraftFailure::Missing));
        };
        let (side, grid, store) = frame(self.table.is_some());
        // One click per round trip.
        if let Some(c) = w.click {
            if tick.saturating_sub(c.at_tick) <= CLICK_PATIENCE_TICKS {
                return (None, CraftStatus::Running);
            }
            self.misses += 1;
            if self.misses >= MAX_MISSES {
                return (None, CraftStatus::Failed(CraftFailure::StaleWindow));
            }
            self.stage = Stage::Settle;
            return (Some(Act::Resync), CraftStatus::Running);
        }
        match self.stage {
            Stage::Approach | Stage::Open { .. } => {
                self.stage = Stage::Settle;
                (None, CraftStatus::Running)
            }
            Stage::Settle => {
                if w.inventory.carried.is_some() {
                    return self.put_down(w);
                }
                if let Some(s) = grid.iter().find(|s| self.held(w, **s).is_some()) {
                    return self.click(*s, 0, 1);
                }
                self.stage = Stage::Place;
                self.stages(w, tick)
            }
            Stage::Place => {
                let cells = layout(recipe, side);
                let next = cells.iter().find(|(slot, _)| self.held(w, *slot).is_none());
                match (next, w.inventory.carried) {
                    (None, None) => {
                        self.stage = Stage::Verify { since: tick };
                        self.stages(w, tick)
                    }
                    // Everything laid, or the cursor holds what the next cell does not take: put
                    // it back, the grid staying as laid.
                    (None, Some(_)) => self.put_down(w),
                    (Some((slot, set)), Some((k, _))) if set.contains(&k) => {
                        self.click(*slot, 1, 0)
                    }
                    (Some(_), Some(_)) => self.put_down(w),
                    (Some((_, set)), None) => {
                        match store
                            .clone()
                            .find(|s| self.held(w, *s).is_some_and(|(k, _)| set.contains(&k)))
                        {
                            Some(s) => self.click(s, 0, 0),
                            None => (None, CraftStatus::Failed(CraftFailure::Missing)),
                        }
                    }
                }
            }
            Stage::Verify { since } => {
                let want = (recipe.result, i32::from(recipe.count));
                if self.held(w, RESULT) == Some(want) {
                    self.stage = Stage::Take {
                        before: count(w, recipe.result),
                    };
                    return self.click(RESULT, 0, 1);
                }
                if tick.saturating_sub(since) > VERIFY_TICKS {
                    return (None, CraftStatus::Failed(CraftFailure::WrongResult));
                }
                (None, CraftStatus::Running)
            }
            Stage::Take { before } => {
                if count(w, recipe.result) > before {
                    return (None, CraftStatus::Done);
                }
                // The take was answered and nothing came: start over (the result is checked
                // again before it is taken).
                self.stage = Stage::Settle;
                (None, CraftStatus::Running)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe(id: &str) -> &'static Recipe {
        recipe_of(id).unwrap()
    }

    #[test]
    fn layouts_sit_at_the_grid_top_left() {
        let slots = |id| {
            layout(recipe(id), 2)
                .iter()
                .map(|(s, _)| *s)
                .collect::<Vec<_>>()
        };
        assert_eq!(slots("oak_planks"), vec![1]);
        // A column of two planks: the left cells of both rows.
        assert_eq!(slots("stick"), vec![1, 3]);
        assert_eq!(slots("crafting_table"), vec![1, 2, 3, 4]);
    }
}
