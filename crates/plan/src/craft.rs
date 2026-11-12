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

use data::ItemKind;
use data::types::{Recipe, Shape};
use world::{Act, World};

/// A click unanswered this long is missed (clojurecraft's 3 s stale-window deadline).
pub const CLICK_PATIENCE_TICKS: u64 = 60;
/// Misses before the task fails `StaleWindow`.
pub const MAX_MISSES: u32 = 3;
/// The progress invariant: a craft needs at most a click per cell, two per source stack and a
/// few to settle and take; more than this is a loop, and the task fails `TooManyClicks`.
pub const MAX_CLICKS: u32 = 40;
/// Ticks Verify waits for the result to show before it calls the result wrong.
pub const VERIFY_TICKS: u64 = 20;

/// The result slot and the grid's slots, top left first.
const RESULT: i16 = 0;
const GRID: [i16; 4] = [1, 2, 3, 4];
/// Where stacks may be taken from and put back: the main inventory and the hotbar.
const STORE: std::ops::RangeInclusive<i16> = 9..=44;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Stage {
    Settle,
    Place,
    Verify { since: u64 },
    Take { before: i32 },
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
}

/// The recipe's layout at the grid's top left: each grid slot and the items it accepts.
pub fn layout(r: &Recipe) -> Vec<(i16, &'static [ItemKind])> {
    match r.shape {
        Shape::Shaped { width, cells, .. } => cells
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let (row, col) = (i / usize::from(width.max(1)), i % usize::from(width.max(1)));
                let slot = GRID.get(row * 2 + col).copied()?;
                c.map(|set| (slot, set))
            })
            .collect(),
        Shape::Shapeless { ingredients } => GRID
            .iter()
            .zip(ingredients.iter())
            .map(|(s, set)| (*s, *set))
            .collect(),
    }
}

fn recipe_of(id: &str) -> Option<&'static Recipe> {
    data::generated::recipes::RECIPES
        .iter()
        .find(|r| r.id == id)
}

fn held(w: &World, slot: i16) -> Option<(ItemKind, i32)> {
    w.inventory.slots.get(&slot).copied()
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
        }
    }

    /// Put the cursor down: on a stack of the same item with room, else an empty slot.
    fn put_down(&mut self, w: &World) -> (Option<Act>, CraftStatus) {
        let Some((k, _)) = w.inventory.carried else {
            return (None, CraftStatus::Running);
        };
        let same = STORE
            .clone()
            .find(|s| held(w, *s).is_some_and(|(hk, n)| hk == k && n < 64));
        let empty = STORE.clone().find(|s| held(w, *s).is_none());
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

    /// One tick: the act to send, if any, and how the craft stands.
    pub fn step(&mut self, w: &World, tick: u64) -> (Option<Act>, CraftStatus) {
        let Some(recipe) = recipe_of(&self.recipe) else {
            return (None, CraftStatus::Failed(CraftFailure::Missing));
        };
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
            Stage::Settle => {
                if w.inventory.carried.is_some() {
                    return self.put_down(w);
                }
                if let Some(s) = GRID.iter().find(|s| held(w, **s).is_some()) {
                    return self.click(*s, 0, 1);
                }
                self.stage = Stage::Place;
                self.step(w, tick)
            }
            Stage::Place => {
                let cells = layout(recipe);
                let next = cells.iter().find(|(slot, _)| held(w, *slot).is_none());
                match (next, w.inventory.carried) {
                    (None, None) => {
                        self.stage = Stage::Verify { since: tick };
                        self.step(w, tick)
                    }
                    // Everything laid, or the cursor holds what the next cell does not take: put
                    // it back, the grid staying as laid.
                    (None, Some(_)) => self.put_down(w),
                    (Some((slot, set)), Some((k, _))) if set.contains(&k) => {
                        self.click(*slot, 1, 0)
                    }
                    (Some(_), Some(_)) => self.put_down(w),
                    (Some((_, set)), None) => {
                        match STORE
                            .clone()
                            .find(|s| held(w, *s).is_some_and(|(k, _)| set.contains(&k)))
                        {
                            Some(s) => self.click(s, 0, 0),
                            None => (None, CraftStatus::Failed(CraftFailure::Missing)),
                        }
                    }
                }
            }
            Stage::Verify { since } => {
                let want = (recipe.result, i32::from(recipe.count));
                if held(w, RESULT) == Some(want) {
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
            layout(recipe(id))
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
