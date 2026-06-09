//! Speedrun step machine — ordered steps, each gated by `can_execute` and
//! satisfied by `is_complete`; `execute_step` dispatches by id. Port of steve's
//! `steps.ts` (early phases).

use rustcraft::bot::Bot;

use crate::tasks;
use crate::types::{failure, GameState, Step, StepResult};

pub const STEPS: &[Step] = &[
    Step {
        id: "gather_wood",
        name: "Gather Wood",
        priority: 1,
        can_execute: |s| s.world.dimension == "overworld" && s.alive,
        is_complete: |s| s.inventory.logs >= 5 || s.inventory.planks >= 12,
    },
    Step {
        id: "craft_planks",
        name: "Craft Planks",
        priority: 2,
        can_execute: |s| s.inventory.logs >= 2,
        is_complete: |s| s.inventory.planks >= 8,
    },
    Step {
        id: "craft_crafting_table",
        name: "Craft Crafting Table",
        priority: 3,
        can_execute: |s| s.inventory.planks >= 4,
        is_complete: |s| s.equipment.has_crafting_table,
    },
    Step {
        id: "craft_sticks",
        name: "Craft Sticks",
        priority: 4,
        can_execute: |s| s.inventory.planks >= 2,
        is_complete: |s| s.inventory.sticks >= 4,
    },
    Step {
        id: "craft_wooden_pickaxe",
        name: "Craft Wooden Pickaxe",
        priority: 5,
        can_execute: |s| s.inventory.planks >= 3 && s.inventory.sticks >= 2,
        is_complete: |s| s.equipment.pickaxe_tier().rank() >= 1,
    },
];

/// First step that can run and isn't already complete.
pub fn get_next_step(state: &GameState) -> Option<&'static Step> {
    STEPS.iter().find(|s| (s.can_execute)(state) && !(s.is_complete)(state))
}

/// How many steps are complete (progress reporting).
pub fn progress(state: &GameState) -> (usize, usize) {
    let done = STEPS.iter().filter(|s| (s.is_complete)(state)).count();
    (done, STEPS.len())
}

pub async fn execute_step(bot: &mut Bot<'_>, id: &str) -> StepResult {
    match id {
        "gather_wood" => tasks::gather_wood::gather_wood(bot, 5).await,
        "craft_planks" => tasks::craft::craft_planks(bot).await,
        "craft_crafting_table" => tasks::craft::craft_crafting_table(bot).await,
        "craft_sticks" => tasks::craft::craft_sticks(bot).await,
        "craft_wooden_pickaxe" => tasks::craft::craft_wooden_pickaxe(bot).await,
        other => failure(format!("no executor for step {other}")),
    }
}
