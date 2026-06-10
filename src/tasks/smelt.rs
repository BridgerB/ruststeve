//! Smelting — place a furnace, load ore + fuel, wait, collect ingots. Port of
//! steve's `tasks/smelt` (raw clickWindow approach; bot.transfer is a no-op here).

use std::time::{Duration, Instant};

use rustcraft::bot::{Bot, Face};

use crate::bot_utils::{count_items, select_item};
use crate::types::{failure, success, StepResult};

fn is_furnace(name: &str) -> bool {
    name == "furnace" || name == "lit_furnace"
}

/// Find a placed furnace nearby, or place one from inventory. Returns its pos.
async fn get_furnace(bot: &mut Bot<'_>) -> Option<(i32, i32, i32)> {
    let p = bot.entity.position;
    let (fx, fy, fz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    for (dx, dy, dz) in [(0, 0, 0), (1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (1, 1, 0), (-1, 1, 0), (0, 1, 1), (0, 1, -1)] {
        let (x, y, z) = (fx + dx, fy + dy, fz + dz);
        if bot.block_at(x, y, z).map(|b| is_furnace(&b.name)).unwrap_or(false) {
            return Some((x, y, z));
        }
    }
    if let Some(pos) = bot.find_block("furnace", 24) {
        return Some(pos);
    }
    if count_items(bot, "furnace") == 0 {
        return None;
    }
    if !select_item(bot, "furnace").await.unwrap_or(false) {
        return None;
    }
    // Place on a neighbouring floor (dig a niche if boxed in, like the table).
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let (tx, ty, tz) = (fx + dx, fy, fz + dz);
        if bot.block_state_at(tx, ty - 1, tz) != 0 {
            if bot.block_state_at(tx, ty, tz) != 0 && bot.dig(tx, ty, tz).await.is_err() {
                continue;
            }
            if bot.block_state_at(tx, ty, tz) != 0 {
                continue;
            }
            bot.look_at(rustcraft::vec3::vec3(tx as f64 + 0.5, ty as f64 - 0.5, tz as f64 + 0.5));
            bot.wait_ticks(2).await.ok();
            bot.place_block(tx, ty - 1, tz, Face::Top).await.ok();
            bot.wait_ticks(4).await.ok();
            return Some((tx, ty, tz));
        }
    }
    None
}

/// Move a stack of `name` from the open window's inventory into furnace `slot`.
async fn load_slot(bot: &mut Bot<'_>, name: &str, slot: i32) -> bool {
    let src = bot.current_window.as_ref().and_then(|w| {
        w.slots
            .iter()
            .enumerate()
            .find(|(i, s)| *i >= w.inventory_start && s.as_ref().map(|it| it.name == name).unwrap_or(false))
            .map(|(i, _)| i as i32)
    });
    if let Some(src) = src {
        let _ = bot.click_window(src, 0, 0).await; // pick up
        let _ = bot.click_window(slot, 0, 0).await; // place in furnace slot
        if bot.window_selected().is_some() {
            let _ = bot.click_window(src, 0, 0).await; // leftover back
        }
        bot.wait_ticks(2).await.ok();
        return true;
    }
    false
}

/// True if the open furnace `slot` is empty.
fn slot_empty(bot: &Bot, slot: i32) -> bool {
    bot.current_window
        .as_ref()
        .and_then(|w| w.slots.get(slot as usize))
        .map(|s| s.is_none())
        .unwrap_or(true)
}

pub async fn smelt_iron(bot: &mut Bot<'_>, target: i32) -> StepResult {
    let Some((fx, fy, fz)) = get_furnace(bot).await else {
        return failure("no furnace to smelt with");
    };
    let _ = bot.goto_near(fx, fy, fz, 2.0).await;
    if !bot.open_block(fx, fy, fz, Face::Top).await.unwrap_or(false) {
        return failure("could not open furnace");
    }
    bot.wait_ticks(10).await.ok(); // window contents

    // Load ore (slot 0) and fuel (slot 1).
    load_slot(bot, "raw_iron", 0).await;
    if !load_slot(bot, "coal", 1).await {
        load_slot(bot, "charcoal", 1).await;
    }

    let deadline = Instant::now() + Duration::from_secs(220);
    while count_items(bot, "iron_ingot") < target && Instant::now() < deadline {
        bot.wait_ticks(20).await.ok(); // ~1s

        // Take any finished ingots from the output (slot 2) into inventory.
        if !slot_empty(bot, 2) {
            let _ = bot.put_away(2).await;
            bot.wait_ticks(2).await.ok();
        }
        // Keep ore/fuel topped up.
        if slot_empty(bot, 0) && count_items(bot, "raw_iron") > 0 {
            load_slot(bot, "raw_iron", 0).await;
        }
        if slot_empty(bot, 1) && (count_items(bot, "coal") > 0 || count_items(bot, "charcoal") > 0) {
            if !load_slot(bot, "coal", 1).await {
                load_slot(bot, "charcoal", 1).await;
            }
        }
        // Done when nothing left to smelt.
        if slot_empty(bot, 0) && count_items(bot, "raw_iron") == 0 {
            // one more grace tick to let the last ingot finish
            bot.wait_ticks(20).await.ok();
            if !slot_empty(bot, 2) {
                let _ = bot.put_away(2).await;
            }
            break;
        }
    }
    let _ = bot.close_window().await;
    let n = count_items(bot, "iron_ingot");
    if n >= target {
        success(format!("smelted {n}/{target} iron"))
    } else if n > 0 {
        success(format!("smelted {n}/{target} iron (partial)"))
    } else {
        failure("smelted no iron")
    }
}
