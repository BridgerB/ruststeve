//! Action helpers built on rustcraft's bot — item counting, log finding,
//! generic crafting, and crafting-table placement. Port of the slice of steve's
//! `lib/bot-utils.ts` the early phases use.

use rustcraft::bot::{Bot, Face};

use crate::types::{failure, success, StepResult};

pub const LOG_TYPES: &[&str] = &[
    "oak_log", "birch_log", "spruce_log", "jungle_log", "acacia_log", "dark_oak_log",
    "mangrove_log", "cherry_log", "pale_oak_log",
];

/// Total count of an item by name in the inventory.
pub fn count_items(bot: &Bot, name: &str) -> i32 {
    bot.inventory.slots.iter().flatten().filter(|i| i.name == name).map(|i| i.count).sum()
}

/// Total inventory item count (any item).
pub fn total_count(bot: &Bot) -> i32 {
    bot.inventory.slots.iter().flatten().map(|i| i.count).sum()
}

/// Walk to the nearest dropped item entity (falling back to the dug block) to
/// pick it up. Stops early once total inventory count grows. ~2s budget.
pub async fn collect_drops(bot: &mut Bot<'_>, fx: i32, fz: i32) {
    let item_type = bot.registry.entities_by_name.get("item").map(|d| d.id);
    let before = total_count(bot);
    for _ in 0..40 {
        let bp = bot.entity.position;
        let mut target = rustcraft::vec3::vec3(fx as f64 + 0.5, bp.y, fz as f64 + 0.5);
        let mut best = f64::MAX;
        for e in bot.entities.values() {
            if item_type.is_some() && e.entity_type != item_type {
                continue;
            }
            let d = e.position.distance(bp);
            if d < best && d < 10.0 {
                best = d;
                target = e.position;
            }
        }
        bot.look_at(rustcraft::vec3::vec3(target.x, bp.y - 0.5, target.z));
        bot.set_control_state("forward", true);
        match bot.drive_tick().await {
            Ok(rustcraft::bot::DriveStep::Disconnected) | Err(_) => break,
            _ => {}
        }
        if total_count(bot) > before {
            break;
        }
    }
    bot.clear_control_states();
    bot.wait_ticks(4).await.ok();
}

/// Find the nearest log block (any species), searching out in rings.
pub fn find_closest_log(bot: &Bot) -> Option<(String, (i32, i32, i32))> {
    let p = bot.entity.position;
    for radius in [16, 32, 48, 64] {
        let mut best: Option<(String, (i32, i32, i32))> = None;
        let mut best_d = f64::MAX;
        for &log in LOG_TYPES {
            for (x, y, z) in bot.find_blocks(log, radius, 16) {
                let d = (x as f64 - p.x).powi(2) + (z as f64 - p.z).powi(2) + (y as f64 - p.y).powi(2);
                if d < best_d {
                    best_d = d;
                    best = Some((log.to_string(), (x, y, z)));
                }
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

/// Ensure `name` is in the held hotbar slot (moving it there if needed).
pub async fn select_item(bot: &mut Bot<'_>, name: &str) -> std::io::Result<bool> {
    let pos = bot.inventory.slots.iter().position(|s| s.as_ref().map(|i| i.name.as_str()) == Some(name));
    let Some(idx) = pos else { return Ok(false) };
    if (36..45).contains(&idx) {
        bot.set_held_slot((idx - 36) as i32).await?;
    } else {
        bot.move_slot_item(idx as i32, 36).await?;
        bot.set_held_slot(0).await?;
    }
    Ok(true)
}

/// Craft `count` of an item by name. `table` is the position of an open-able
/// crafting table for 3x3 recipes (None → 2x2 inventory grid).
pub async fn craft_item(
    bot: &mut Bot<'_>,
    name: &str,
    count: i32,
    table: Option<(i32, i32, i32)>,
) -> StepResult {
    let Some(def) = bot.registry.items_by_name.get(name) else {
        return failure(format!("unknown item {name}"));
    };
    let id = def.id;
    let recipes = bot.recipes_for(id, Some(1), table.is_some());
    let Some(recipe) = recipes.into_iter().next() else {
        return failure(format!("no recipe for {name}"));
    };
    if recipe.requires_table && table.is_none() {
        return failure(format!("{name} needs a crafting table"));
    }

    if let Some((tx, ty, tz)) = table {
        let _ = bot.goto(tx, ty, tz).await;
        if bot.current_window.is_none() {
            let _ = bot.open_block(tx, ty, tz, Face::Top).await;
        }
    }

    let result = bot.craft(&recipe, count, table.is_some()).await;
    if table.is_some() {
        let _ = bot.close_window().await;
        // Let the server's post-close inventory update arrive so the crafted item
        // is reflected locally (otherwise the result looks lost and we re-craft).
        bot.wait_ticks(6).await.ok();
    }
    // Equip the result if it's a pickaxe — moves it to the hotbar (a stable slot)
    // and confirms it actually landed in the inventory.
    if name.ends_with("_pickaxe") {
        let _ = select_item(bot, name).await;
    }
    if std::env::var("CRAFT_DEBUG").is_ok() {
        let inv: Vec<String> = bot.inventory.slots.iter().flatten().filter(|i| i.count > 0).map(|i| format!("{}x{}", i.count, i.name)).collect();
        let win = bot.current_window.as_ref().map(|w| w.slots.iter().flatten().filter(|i| i.count > 0).map(|i| format!("{}x{}", i.count, i.name)).collect::<Vec<_>>());
        eprintln!("CRAFT {name}: result={result:?} inv={inv:?} win={win:?}");
    }
    match result {
        Ok(()) => success(format!("crafted {count}x {name}")),
        Err(e) => failure(format!("craft {name}: {e}")),
    }
}

/// Find an existing crafting table nearby, or place one from inventory.
/// Returns its position (the bot will be near it).
pub async fn get_crafting_table(bot: &mut Bot<'_>) -> std::io::Result<Option<(i32, i32, i32)>> {
    if let Some(pos) = bot.find_block("crafting_table", 24) {
        bot.goto(pos.0, pos.1, pos.2).await?;
        return Ok(Some(pos));
    }
    // No table in the world — craft one on demand if we don't have the item
    // (the step machine may have skipped the dedicated craft-table step).
    if count_items(bot, "crafting_table") == 0 {
        let _ = craft_item(bot, "crafting_table", 1, None).await;
        if count_items(bot, "crafting_table") == 0 {
            println!("    table: could not craft a crafting_table");
            return Ok(None);
        }
    }
    place_crafting_table(bot).await
}

/// Place a crafting table on an air block next to the bot with solid ground.
async fn place_crafting_table(bot: &mut Bot<'_>) -> std::io::Result<Option<(i32, i32, i32)>> {
    if !select_item(bot, "crafting_table").await? {
        println!("    table: could not select crafting_table item");
        return Ok(None);
    }
    let p = bot.entity.position;
    let (fx, fy, fz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    // Try feet-level and one-below neighbours: need an empty cell with a solid
    // block beneath it to place the table on.
    for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (1, -1, 0), (-1, -1, 0), (0, -1, 1), (0, -1, -1)] {
        let (tx, ty, tz) = (fx + dx, fy + dy, fz + dz);
        if bot.block_state_at(tx, ty, tz) == 0 && bot.block_state_at(tx, ty - 1, tz) != 0 {
            bot.look_at(rustcraft::vec3::vec3(tx as f64 + 0.5, ty as f64 - 0.5, tz as f64 + 0.5));
            bot.wait_ticks(2).await?;
            // Place against the top face of the ground block below the target.
            bot.place_block(tx, ty - 1, tz, Face::Top).await?;
            bot.wait_ticks(4).await?;
            select_item(bot, "crafting_table").await?;
            println!("    table: placed at ({tx},{ty},{tz})");
            return Ok(Some((tx, ty, tz)));
        }
    }
    // No open spot (e.g. mining in a tunnel) — DIG a side niche at feet level so a
    // cell opens up with solid ground below it, then place the table there.
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let (tx, ty, tz) = (fx + dx, fy, fz + dz);
        if bot.block_state_at(tx, ty - 1, tz) != 0 {
            if bot.block_state_at(tx, ty, tz) != 0 && bot.dig(tx, ty, tz).await.is_err() {
                continue;
            }
            if bot.block_state_at(tx, ty, tz) != 0 {
                continue; // couldn't break it (e.g. bedrock)
            }
            bot.look_at(rustcraft::vec3::vec3(tx as f64 + 0.5, ty as f64 - 0.5, tz as f64 + 0.5));
            bot.wait_ticks(2).await?;
            bot.place_block(tx, ty - 1, tz, Face::Top).await?;
            bot.wait_ticks(4).await?;
            select_item(bot, "crafting_table").await?;
            println!("    table: dug a niche + placed at ({tx},{ty},{tz})");
            return Ok(Some((tx, ty, tz)));
        }
    }
    println!("    table: no valid spot to place (bot at {fx},{fy},{fz})");
    Ok(None)
}
