//! Crafting tasks. Port of steve's `tasks/craft` (early-phase subset).

use rustcraft::bot::Bot;

use crate::bot_utils::{craft_item, get_crafting_table};
use crate::memory::WorldMemory;
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

pub async fn craft_wooden_pickaxe(bot: &mut Bot<'_>, mem: &mut WorldMemory) -> StepResult {
    match get_crafting_table(bot, mem).await {
        Ok(Some(table)) => craft_item(bot, "wooden_pickaxe", 1, Some(table)).await,
        _ => failure("need a crafting table"),
    }
}

pub async fn craft_stone_pickaxe(bot: &mut Bot<'_>, mem: &mut WorldMemory) -> StepResult {
    // Ensure sticks first (2x2, no table).
    if crate::bot_utils::count_items(bot, "stick") < 2 {
        let r = craft_item(bot, "stick", 1, None).await;
        if !r.success {
            return r;
        }
    }
    match get_crafting_table(bot, mem).await {
        Ok(Some(table)) => craft_item(bot, "stone_pickaxe", 1, Some(table)).await,
        _ => failure("need a crafting table"),
    }
}

/// Craft a tool/item at a (placed/found) table after ensuring N sticks.
async fn craft_at_table(
    bot: &mut Bot<'_>,
    item: &str,
    sticks_needed: i32,
    mem: &mut WorldMemory,
) -> StepResult {
    if sticks_needed > 0 && crate::bot_utils::count_items(bot, "stick") < sticks_needed {
        let _ = craft_item(bot, "stick", 1, None).await;
    }
    match get_crafting_table(bot, mem).await {
        Ok(Some(table)) => craft_item(bot, item, 1, Some(table)).await,
        _ => failure("need a crafting table"),
    }
}

pub async fn craft_stone_sword(bot: &mut Bot<'_>, mem: &mut WorldMemory) -> StepResult {
    craft_at_table(bot, "stone_sword", 1, mem).await
}

pub async fn craft_furnace(bot: &mut Bot<'_>, mem: &mut WorldMemory) -> StepResult {
    craft_at_table(bot, "furnace", 0, mem).await
}

pub async fn craft_iron_pickaxe(bot: &mut Bot<'_>, mem: &mut WorldMemory) -> StepResult {
    craft_at_table(bot, "iron_pickaxe", 2, mem).await
}

pub async fn craft_buckets(bot: &mut Bot<'_>, count: i32, mem: &mut WorldMemory) -> StepResult {
    // Each bucket is 3 iron. Craft one at a time up to `count` while iron lasts.
    while crate::bot_utils::count_items(bot, "bucket") + crate::bot_utils::count_items(bot, "water_bucket") < count
        && crate::bot_utils::count_items(bot, "iron_ingot") >= 3
    {
        let r = craft_at_table(bot, "bucket", 0, mem).await;
        if !r.success {
            return r;
        }
    }
    success(format!("have {} buckets", crate::bot_utils::count_items(bot, "bucket")))
}

pub async fn get_flint_and_steel(bot: &mut Bot<'_>, mem: &mut WorldMemory) -> StepResult {
    // Need flint (from gravel) + 1 iron.
    if crate::bot_utils::count_items(bot, "flint") < 1 {
        let _ = crate::tasks::mining::mine_gravel_for_flint(bot, 1, mem).await;
        if crate::bot_utils::count_items(bot, "flint") < 1 {
            return failure("could not get flint from gravel");
        }
    }
    craft_at_table(bot, "flint_and_steel", 0, mem).await
}
