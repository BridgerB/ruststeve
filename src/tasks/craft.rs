//! Crafting tasks. Port of steve's `tasks/craft` (early-phase subset).

use rustcraft::bot::Bot;

use crate::bot_utils::{craft_item, get_crafting_table};
use crate::types::{failure, success, StepResult};

/// Turn each log stack into planks (1 log → 4 planks, 2x2 grid).
pub async fn craft_planks(bot: &mut Bot<'_>) -> StepResult {
    let logs: Vec<(String, i32)> = bot
        .inventory
        .slots
        .iter()
        .flatten()
        .filter(|i| i.name.ends_with("_log"))
        .map(|i| (i.name.clone(), i.count))
        .collect();
    if logs.is_empty() {
        return failure("no logs in inventory");
    }
    for (log, count) in logs {
        let planks = log.replace("_log", "_planks");
        let r = craft_item(bot, &planks, count.min(8), None).await;
        if !r.success {
            // Fall back to oak_planks recipe family if species lookup missed.
            let _ = craft_item(bot, "oak_planks", count.min(8), None).await;
        }
    }
    success("crafted planks from logs")
}

pub async fn craft_crafting_table(bot: &mut Bot<'_>) -> StepResult {
    craft_item(bot, "crafting_table", 1, None).await
}

pub async fn craft_sticks(bot: &mut Bot<'_>) -> StepResult {
    // Two batches → 8 sticks, enough for the early tools.
    let r = craft_item(bot, "stick", 1, None).await;
    if !r.success {
        return r;
    }
    craft_item(bot, "stick", 1, None).await
}

pub async fn craft_wooden_pickaxe(bot: &mut Bot<'_>) -> StepResult {
    match get_crafting_table(bot).await {
        Ok(Some(table)) => craft_item(bot, "wooden_pickaxe", 1, Some(table)).await,
        _ => failure("need a crafting table"),
    }
}
