//! Smelting — place a furnace, load ore + fuel, wait, collect ingots. Port of
//! steve's `tasks/smelt` (raw clickWindow approach; bot.transfer is a no-op here).

use std::time::{Duration, Instant};

use crate::bot::{Bot, Face};

use crate::bot_utils::{count_items, select_item};
use crate::memory::WorldMemory;
use crate::types::{failure, success, StepResult};

fn is_furnace(name: &str) -> bool {
    name == "furnace" || name == "lit_furnace"
}

/// Find a placed furnace nearby, or place one from inventory. Returns its pos.
async fn get_furnace(bot: &mut Bot<'_>, mem: &mut WorldMemory) -> Option<(i32, i32, i32)> {
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
    // No furnace item and none placed within reach — the one we placed earlier is out of
    // range after a deep iron/coal mining trip. CRAFT a fresh one from cobble rather than
    // looping "no furnace to smelt with" forever (observed 44x live). The bot carries
    // hundreds of cobble; a furnace is 8, and it still keeps 30+ for the portal scaffold.
    if count_items(bot, "furnace") == 0 {
        if count_items(bot, "cobblestone") >= 8 {
            let _ = crate::tasks::craft::craft_furnace(bot, mem).await;
        }
        if count_items(bot, "furnace") == 0 {
            return None;
        }
    }
    if !select_item(bot, "furnace").await.unwrap_or(false) {
        return None;
    }
    // Place on a neighbouring floor (dig a niche if boxed in, like the table) and only
    // return a position we've VERIFIED holds a furnace. Returning an unverified spot is
    // what made smelt_iron loop "could not open furnace" 399x on a phantom furnace — the
    // placement silently failed (no floor / occupied) but get_furnace claimed success.
    // An honest total function: try each candidate, confirm the block landed, else move on.
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let (tx, ty, tz) = (fx + dx, fy, fz + dz);
        if bot.block_state_at(tx, ty - 1, tz) == 0 {
            // No floor — BUILD one from cobble rather than giving up. In a tight mining tunnel
            // all neighbours are air below, so every candidate was skipped and the bot looped
            // "no furnace to smelt with" forever (014568× live) despite holding a furnace + 346
            // cobble. Place a cobble against the bot's own feet-support, then set the furnace on it.
            if count_items(bot, "cobblestone") < 1 {
                continue;
            }
            let face = match (dx, dz) {
                (1, 0) => Face::East,
                (-1, 0) => Face::West,
                (0, 1) => Face::South,
                _ => Face::North,
            };
            if !select_item(bot, "cobblestone").await.unwrap_or(false) {
                continue;
            }
            bot.look_at(crate::vec3::vec3(tx as f64 + 0.5, (ty - 1) as f64 + 0.5, tz as f64 + 0.5));
            bot.wait_ticks(2).await.ok();
            let _ = bot.place_block(fx, fy - 1, fz, face).await; // cobble → (tx, ty-1, tz)
            bot.wait_ticks(3).await.ok();
            if bot.block_state_at(tx, ty - 1, tz) == 0 {
                continue; // couldn't build a floor here — try the next neighbour
            }
            select_item(bot, "furnace").await.ok();
        }
        if bot.block_state_at(tx, ty, tz) != 0 && bot.dig(tx, ty, tz).await.is_err() {
            continue;
        }
        if bot.block_state_at(tx, ty, tz) != 0 {
            continue; // still blocked after digging
        }
        bot.look_at(crate::vec3::vec3(tx as f64 + 0.5, ty as f64 - 0.5, tz as f64 + 0.5));
        bot.wait_ticks(2).await.ok();
        bot.place_block(tx, ty - 1, tz, Face::Top).await.ok();
        bot.wait_ticks(4).await.ok();
        if bot.block_at(tx, ty, tz).map(|b| is_furnace(&b.name)).unwrap_or(false) {
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

/// Count an item in the OPEN window's inventory section only (excludes furnace
/// slots 0-2). While a furnace window is open the live inventory lives in the
/// window — `bot.inventory` is stale until the window closes — so the smelt loop
/// must count HERE or it never sees the ingots it pulled out (count stays 0) and
/// always burns the full 220s deadline instead of finishing when the iron is done.
fn win_inv_count(bot: &Bot, name: &str) -> i32 {
    if let Some(w) = bot.current_window.as_ref() {
        let end = w.inventory_end.min(w.slots.len());
        if w.inventory_start <= end {
            return w.slots[w.inventory_start..end]
                .iter()
                .flatten()
                .filter(|i| i.name == name)
                .map(|i| i.count)
                .sum();
        }
    }
    count_items(bot, name)
}

/// True if the open furnace `slot` is empty.
fn slot_empty(bot: &Bot, slot: i32) -> bool {
    bot.current_window
        .as_ref()
        .and_then(|w| w.slots.get(slot as usize))
        .map(|s| s.is_none())
        .unwrap_or(true)
}

pub async fn smelt_iron(bot: &mut Bot<'_>, target: i32, mem: &mut WorldMemory) -> StepResult {
    let Some((fx, fy, fz)) = get_furnace(bot, mem).await else {
        return failure("no furnace to smelt with");
    };
    // Re-approach + retry the open a few times: a single open can miss if the bot hasn't
    // settled within reach yet. Bounded so a genuinely unreachable furnace still fails fast.
    let mut opened = false;
    for _ in 0..3 {
        let _ = bot.goto_near(fx, fy, fz, 2.0).await;
        if bot.open_block(fx, fy, fz, Face::Top).await.unwrap_or(false) {
            opened = true;
            break;
        }
        bot.wait_ticks(5).await.ok();
    }
    if !opened {
        return failure("could not open furnace");
    }
    bot.wait_ticks(10).await.ok(); // window contents

    // Load ore (slot 0) and fuel (slot 1).
    load_slot(bot, "raw_iron", 0).await;
    if !load_slot(bot, "coal", 1).await {
        load_slot(bot, "charcoal", 1).await;
    }

    let deadline = Instant::now() + Duration::from_secs(220);
    while win_inv_count(bot, "iron_ingot") < target && Instant::now() < deadline {
        bot.wait_ticks(20).await.ok(); // ~1s

        // Take any finished ingots from the output (slot 2) into inventory.
        if !slot_empty(bot, 2) {
            let _ = bot.put_away(2).await;
            bot.wait_ticks(2).await.ok();
        }
        // Keep ore/fuel topped up.
        if slot_empty(bot, 0) && win_inv_count(bot, "raw_iron") > 0 {
            load_slot(bot, "raw_iron", 0).await;
        }
        if slot_empty(bot, 1) && (win_inv_count(bot, "coal") > 0 || win_inv_count(bot, "charcoal") > 0) {
            if !load_slot(bot, "coal", 1).await {
                load_slot(bot, "charcoal", 1).await;
            }
        }
        // Done when nothing left to smelt.
        if slot_empty(bot, 0) && win_inv_count(bot, "raw_iron") == 0 {
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
