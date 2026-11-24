//! Eating (issue 12c, brain/ruststeve/design/food-and-hunting.md): the `eat` always-row, a port
//! of the core of clojurecraft's eat.clj. Under `hungry_below` food with something edible held:
//! hold the best food, `use_item`, keep it in hand the food's eating time, done when the server
//! says the use finished (entity event 9) or the food rose. Food values come from the generated
//! item components; what is never eaten is data (`[survival] unsafe_foods`).

use data::ItemKind;
use world::{Act, World};

use crate::hold::{self, Hold};
use crate::make::SurvivalRules;

/// Past the eating time, how long the server's answer may take (clojurecraft's `answer-ms`, 3 s).
pub const ANSWER_TICKS: u64 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EatFailure {
    /// The food is no longer held or in the inventory.
    Missing,
    /// No answer from the server in the eating time and `ANSWER_TICKS`.
    NoAnswer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EatStatus {
    Running,
    Done,
    Failed(EatFailure),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Eat {
    pub item: ItemKind,
    /// The uses finished and the food when it began.
    pub uses_before: u64,
    pub food_before: Option<i32>,
    /// When `use_item` went.
    pub used_at: Option<u64>,
    pub started: u64,
}

/// Is the food the server last sent under the hungry line?
pub fn hungry(w: &World, rules: &SurvivalRules) -> bool {
    w.player.food.is_some_and(|f| f < rules.hungry_below)
}

/// Food at or over this lets health come back by itself (vanilla's natural regeneration).
pub const HEALS_FROM: i32 = 18;

/// Hurt under the engage line with the food bar too low to heal (issue 16: eat to heal between
/// blaze fights, not only when hungry).
pub fn eat_to_heal(w: &World, rules: &SurvivalRules) -> bool {
    w.player.health.is_some_and(|h| h < rules.engage_health)
        && w.player.food.is_some_and(|f| f < HEALS_FROM)
}

/// Hunger points of safe food held: what a reserve is counted in (issue 16a).
pub fn food_points(w: &World, rules: &SurvivalRules) -> i32 {
    crate::held(w)
        .into_iter()
        .filter(|(k, n)| *n > 0 && !rules.unsafe_foods.contains(k))
        .filter_map(|(k, n)| {
            data::generated::item_props::food_of(k).map(|f| i32::from(f.nutrition) * n)
        })
        .sum()
}

/// The furnace recipe that cooks `item` into food, if it is raw food (beef, porkchop, mutton,
/// chicken, rabbit, the fish, a potato): what the cook phase works (overworld-leg wall 4).
pub fn cooks(item: ItemKind) -> Option<&'static data::types::Smelt> {
    data::generated::item_props::food_of(item)?;
    data::generated::recipes::SMELTING.iter().find(|r| {
        r.input.contains(&item) && data::generated::item_props::food_of(r.result).is_some()
    })
}

/// Raw food held that cooks into safe food, by kind, with its recipe: raw chicken counts (it is
/// never eaten raw, and cooked it is safe).
pub fn raw_held(
    counts: &crate::make::Counts,
    rules: &SurvivalRules,
) -> Vec<(ItemKind, i32, &'static data::types::Smelt)> {
    counts
        .iter()
        .filter(|(_, n)| **n > 0)
        .filter_map(|(k, n)| cooks(*k).map(|r| (*k, *n, r)))
        .filter(|(_, _, r)| !rules.unsafe_foods.contains(&r.result))
        .collect()
}

/// Pieces of meat held: every safe food item, and raw food that cooks into one.
pub fn meat_pieces(counts: &crate::make::Counts, rules: &SurvivalRules) -> i32 {
    counts
        .iter()
        .filter(|(k, n)| {
            **n > 0
                && data::generated::item_props::food_of(**k).is_some()
                && (!rules.unsafe_foods.contains(k)
                    || cooks(**k).is_some_and(|r| !rules.unsafe_foods.contains(&r.result)))
        })
        .map(|(_, n)| *n)
        .sum()
}

/// The food to eat: safe food in the inventory (hotbar and main, window 0's slots 9-44), the
/// most nutritious first, ties by item id.
pub fn best_food(w: &World, rules: &SurvivalRules) -> Option<ItemKind> {
    crate::held(w)
        .into_iter()
        .filter(|(k, n)| *n > 0 && !rules.unsafe_foods.contains(k))
        .filter_map(|(k, _)| data::generated::item_props::food_of(k).map(|f| (k, f.nutrition)))
        .max_by_key(|(k, n)| (*n, std::cmp::Reverse(k.id())))
        .map(|(k, _)| k)
}

impl Eat {
    pub fn new(w: &World, item: ItemKind, tick: u64) -> Self {
        Self {
            item,
            uses_before: w.player.uses_finished,
            food_before: w.player.food,
            used_at: None,
            started: tick,
        }
    }

    pub fn step(&mut self, w: &World, tick: u64) -> (Option<Act>, EatStatus) {
        if w.player.uses_finished > self.uses_before
            || w.player
                .food
                .zip(self.food_before)
                .is_some_and(|(n, b)| n > b)
        {
            return (None, EatStatus::Done);
        }
        if let Some(at) = self.used_at {
            let ticks =
                data::generated::item_props::food_of(self.item).map_or(32, |f| u64::from(f.ticks));
            let slow = crate::dig::slowdown(w);
            if (tick.saturating_sub(at) as f64) > (ticks + ANSWER_TICKS) as f64 * slow {
                return (
                    Some(Act::ReleaseUse),
                    EatStatus::Failed(EatFailure::NoAnswer),
                );
            }
            // The held item must stay put while the server counts the use down.
            return (None, EatStatus::Running);
        }
        match hold::toward(w, self.item) {
            Hold::Held => {
                self.used_at = Some(tick);
                (Some(Act::UseItem), EatStatus::Running)
            }
            Hold::Act(a) => (Some(a), EatStatus::Running),
            Hold::Wait => (None, EatStatus::Running),
            Hold::Missing => (None, EatStatus::Failed(EatFailure::Missing)),
        }
    }
}
