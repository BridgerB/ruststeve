//! Water buckets — find water, fill empty buckets. Port of steve's `tasks/bucket`.

use std::time::{Duration, Instant};

use crate::bot::Bot;
use crate::vec3::vec3;

use crate::bot_utils::{count_items, select_item};
use crate::memory::{PoiKind, PoiStatus, WorldMemory};
use crate::types::{failure, success, StepResult};

/// Nearest water source, preferring one with air above (a surface pool).
fn find_water(bot: &Bot) -> Option<(i32, i32, i32)> {
    let surface = bot
        .find_blocks("water", 48, 24)
        .into_iter()
        .find(|&(x, y, z)| bot.block_state_at(x, y + 1, z) == 0);
    surface.or_else(|| bot.find_block("water", 48))
}

pub async fn fill_water_buckets(bot: &mut Bot<'_>, target: i32, mem: &mut WorldMemory) -> StepResult {
    let deadline = Instant::now() + Duration::from_secs(240);
    // Roam headings used when no water is in range — a dry forest can be far from any
    // river/lake, so we travel to look instead of failing outright (the old behavior,
    // which stranded a portal-ready bot with everything but water).
    let dirs = [(1, 0), (0, 1), (-1, 0), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)];
    let mut dir = 0usize;
    // Memory-first: a bot that wandered far (e.g. during a portal lava-descent) loses the
    // water source it filled from earlier. If none is in scan range, navigate BACK to a
    // remembered water body before roaming — this un-stalls the 'filled 0/1' refill loop.
    if find_water(bot).is_none() {
        // Surface first. After mining iron the bot is stranded at the bottom of a deep
        // 1-wide shaft; the pathfinder can't climb out, so roaming/goto for water just
        // spins in place. Pillar back up to daylight before looking for water.
        bot.movement.blocks_cant_break.clear();
        crate::tasks::portal::climb_out_of_pit(bot).await;
        let p = bot.entity.position;
        let from = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if let Some(poi) = mem.nearest(&[PoiKind::Water], from, 128) {
            bot.movement.blocks_cant_break.clear();
            let _ = bot.goto_near(poi.pos.0, poi.pos.1, poi.pos.2, 3.0).await;
        }
    }
    while count_items(bot, "water_bucket") < target && Instant::now() < deadline {
        if count_items(bot, "bucket") == 0 {
            break; // no empty buckets left to fill
        }
        let Some((wx, wy, wz)) = find_water(bot) else {
            // None in scan range — travel to look. COMMIT to each heading for several long
            // hops (not a zigzag) so a bot in a big dry area actually escapes it instead of
            // wandering in place; rotate only after a few hops in one direction.
            bot.movement.blocks_cant_break.clear();
            let (dx, dz) = dirs[dir % dirs.len()];
            for _ in 0..3 {
                if Instant::now() > deadline || find_water(bot).is_some() {
                    break;
                }
                let p = bot.entity.position;
                let (tx, tz) = (p.x.floor() as i32 + dx * 48, p.z.floor() as i32 + dz * 48);
                let _ = bot.goto_xz(tx, tz, 3.0).await;
            }
            dir += 1;
            continue;
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
