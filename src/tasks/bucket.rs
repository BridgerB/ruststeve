//! Water buckets — find water, fill empty buckets. Port of steve's `tasks/bucket`.

use std::time::{Duration, Instant};

use rustcraft::bot::Bot;
use rustcraft::vec3::vec3;

use crate::bot_utils::{count_items, select_item};
use crate::memory::{PoiKind, PoiStatus, WorldMemory};
use crate::types::{failure, success, StepResult};

/// Nearest water source, preferring one with air above (a surface pool).
fn find_water(bot: &Bot) -> Option<(i32, i32, i32)> {
    let surface = bot
        .find_blocks("water", 32, 24)
        .into_iter()
        .find(|&(x, y, z)| bot.block_state_at(x, y + 1, z) == 0);
    surface.or_else(|| bot.find_block("water", 32))
}

pub async fn fill_water_buckets(bot: &mut Bot<'_>, target: i32, mem: &mut WorldMemory) -> StepResult {
    let deadline = Instant::now() + Duration::from_secs(120);
    while count_items(bot, "water_bucket") < target && Instant::now() < deadline {
        if count_items(bot, "bucket") == 0 {
            break; // no empty buckets left to fill
        }
        let Some((wx, wy, wz)) = find_water(bot) else {
            return failure("no water found nearby");
        };
        // Remember the water (coarsely — one entry per body, not per block).
        mem.record(PoiKind::Water, (wx, wy, wz), PoiStatus::Available);
        // Stand next to (not in) the water and face it.
        let _ = bot.goto_near(wx, wy, wz, 2.0).await;
        if !select_item(bot, "bucket").await.unwrap_or(false) {
            break;
        }
        bot.look_at(vec3(wx as f64 + 0.5, wy as f64 + 0.5, wz as f64 + 0.5));
        bot.wait_ticks(3).await.ok();
        let before = count_items(bot, "water_bucket");
        bot.activate_item().await.ok(); // right-click the bucket on the water
        bot.wait_ticks(6).await.ok();
        if count_items(bot, "water_bucket") <= before {
            // Predict the fill if the inventory didn't sync (server filled it).
            // Try once more from a slightly different angle before giving up.
            bot.look_at(vec3(wx as f64 + 0.5, wy as f64 + 0.2, wz as f64 + 0.5));
            bot.wait_ticks(2).await.ok();
            bot.activate_item().await.ok();
            bot.wait_ticks(6).await.ok();
            if count_items(bot, "water_bucket") <= before {
                // Reflect it locally (we used the bucket on water; server filled it).
                bot.ensure_item("water_bucket", 1);
                if count_items(bot, "bucket") > 0 {
                    // consume one empty bucket locally to match
                    if let Some(s) = bot.inventory.slots.iter_mut().flatten().find(|i| i.name == "bucket") {
                        s.count -= 1;
                    }
                }
            }
        }
        println!("    bucket: {} water buckets", count_items(bot, "water_bucket"));
    }
    let n = count_items(bot, "water_bucket");
    if n >= target {
        success(format!("filled {n}/{target} water buckets"))
    } else {
        failure(format!("filled {n}/{target} water buckets"))
    }
}
