//! Wood gathering — find the nearest reachable log, walk to it, mine it, collect
//! the drop, repeat. Blacklists tree columns we fail to reach/mine and explores
//! when none are nearby. Port of steve's `tasks/gather-wood`.

use std::collections::HashSet;

use rustcraft::bot::Bot;

use crate::bot_utils::LOG_TYPES;
use crate::types::{failure, success, StepResult};

pub fn count_logs(bot: &Bot) -> i32 {
    bot.inventory.slots.iter().flatten().filter(|i| i.name.ends_with("_log")).map(|i| i.count).sum()
}

fn is_log_at(bot: &Bot, x: i32, y: i32, z: i32) -> bool {
    bot.block_at(x, y, z).map(|b| b.name.ends_with("_log")).unwrap_or(false)
}

/// Lowest log block in the 5x5 columns around (cx,cz), scanning raw chunk data
/// (no line-of-sight needed — finds trunks occluded by their own leaves) within
/// a Y window around the bot. Returns the block the bot can stand near + mine.
fn find_trunk_raw(bot: &Bot, cx: i32, cz: i32) -> Option<(i32, i32, i32)> {
    let by = bot.entity.position.y.floor() as i32;
    let mut best: Option<(i32, i32, i32)> = None;
    for dx in -2..=2 {
        for dz in -2..=2 {
            for y in (by - 4)..=(by + 8) {
                if is_log_at(bot, cx + dx, y, cz + dz)
                    && best.map(|(_, by2, _)| y < by2).unwrap_or(true)
                {
                    best = Some((cx + dx, y, cz + dz));
                }
            }
        }
    }
    best
}

/// Nearest log within `radius`, skipping blacklisted (x,z) columns.
fn find_log(bot: &Bot, radius: i32, blacklist: &HashSet<(i32, i32)>) -> Option<(i32, i32, i32)> {
    let p = bot.entity.position;
    let mut best = None;
    let mut best_d = f64::MAX;
    for &log in LOG_TYPES {
        for (x, y, z) in bot.find_blocks(log, radius, 24) {
            if blacklist.contains(&(x, z)) {
                continue;
            }
            let d = (x as f64 - p.x).powi(2) + (z as f64 - p.z).powi(2) + (y as f64 - p.y).powi(2);
            if d < best_d {
                best_d = d;
                best = Some((x, y, z));
            }
        }
    }
    best
}

/// Whether the bot is standing in liquid.
fn in_liquid(bot: &Bot) -> bool {
    let p = bot.entity.position;
    bot.block_at(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
        .map(|b| b.name.contains("water") || b.name.contains("lava"))
        .unwrap_or(false)
}

/// Swim up and out of water onto solid ground.
async fn escape_water(bot: &mut Bot<'_>) {
    for _ in 0..60 {
        if !in_liquid(bot) && bot.entity.on_ground {
            break;
        }
        bot.set_control_state("jump", true);
        bot.set_control_state("forward", true);
        bot.set_control_state("sprint", true);
        if bot.drive_tick().await.map(|s| matches!(s, rustcraft::bot::DriveStep::Disconnected)).unwrap_or(true) {
            break;
        }
    }
    bot.clear_control_states();
}

/// Walk to the nearest dropped item entity (falling back to the dug block) to
/// pick it up. Item entities are tracked from add_entity packets.
async fn collect_at(bot: &mut Bot<'_>, x: i32, z: i32) {
    let item_type = bot.registry.entities_by_name.get("item").map(|d| d.id);
    let logs0 = count_logs(bot);
    for _ in 0..40 {
        // nearest item entity within ~8 blocks, else the dug block
        let bp = bot.entity.position;
        let mut target = rustcraft::vec3::vec3(x as f64 + 0.5, bp.y, z as f64 + 0.5);
        let mut best = f64::MAX;
        for e in bot.entities.values() {
            if item_type.is_some() && e.entity_type != item_type {
                continue;
            }
            let d = e.position.distance(bp);
            if d < best && d < 8.0 {
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
        if count_logs(bot) > logs0 {
            break;
        }
    }
    bot.clear_control_states();
    bot.wait_ticks(4).await.ok();
}

async fn explore(bot: &mut Bot<'_>, attempt: i32) -> std::io::Result<()> {
    let angle = attempt as f64 * 0.95;
    bot.look(angle, 0.0);
    bot.set_control_state("forward", true);
    bot.set_control_state("sprint", true);
    bot.set_control_state("jump", true);
    bot.wait_ticks(40).await?;
    bot.clear_control_states();
    Ok(())
}

pub async fn gather_wood(bot: &mut Bot<'_>, target: i32) -> StepResult {
    bot.wait_ticks(4).await.ok();
    let mut blacklist: HashSet<(i32, i32)> = HashSet::new();
    let mut attempts = 0;
    let mut consecutive_fail = 0;

    while count_logs(bot) < target && attempts < 50 {
        attempts += 1;
        if in_liquid(bot) {
            escape_water(bot).await;
        }
        // Break out of stuck regions: after several failed approaches, stop
        // retrying across-barrier trees and explore in a fresh direction.
        if consecutive_fail >= 3 {
            println!("    wood: stuck — exploring to escape");
            consecutive_fail = 0;
            blacklist.clear();
            let _ = explore(bot, attempts * 2).await;
            continue;
        }

        // Find a tree (rings out to 64), else explore for new chunks.
        let mut found = None;
        for r in [16, 32, 48, 64] {
            found = find_log(bot, r, &blacklist);
            if found.is_some() {
                break;
            }
        }
        let Some((x, y, z)) = found else {
            println!("    wood: no tree nearby — exploring");
            if explore(bot, attempts).await.is_err() {
                break;
            }
            continue;
        };

        // Walk to the tree's COLUMN (horizontal goal — descends to the valley
        // floor if the tree is below us). The trunk base becomes reachable there.
        let p0 = bot.entity.position;
        println!("    wood: tree ({x},{y},{z}) d={:.0} — walking", ((x as f64 - p0.x).powi(2) + (z as f64 - p0.z).powi(2)).sqrt());
        let arr = match bot.goto_xz(x, z, 3.0).await {
            Ok(a) => a,
            Err(_) => break,
        };
        let p1 = bot.entity.position;
        println!(
            "    wood: arrived={arr} at ({:.0},{:.0},{:.0}) moved={:.0} trunk={:?}",
            p1.x, p1.y, p1.z, ((p1.x - p0.x).powi(2) + (p1.z - p0.z).powi(2)).sqrt(),
            find_trunk_raw(bot, x, z)
        );

        // Mine reachable logs in this column, bottom-up, until none in reach or a
        // dig yields nothing (occluded/too far → give up on this tree).
        let column_before = count_logs(bot);
        for _ in 0..10 {
            // Lowest trunk log near this column via raw chunk data (sees through
            // the tree's own leaf occlusion).
            let Some((tx, ty, tz)) = find_trunk_raw(bot, x, z) else { break };

            // Get adjacent to this specific log (clear LOS) before digging.
            let _ = bot.goto_near(tx, ty, tz, 2.5).await;
            let before = count_logs(bot);
            if bot.dig(tx, ty, tz).await.is_err() {
                return failure(format!("disconnected; {} logs", count_logs(bot)));
            }
            collect_at(bot, tx, tz).await;

            if count_logs(bot) > before {
                println!("    wood: {} → {} logs", before, count_logs(bot));
            } else {
                break; // dig yielded nothing — stop working this tree
            }
            if count_logs(bot) >= target {
                break;
            }
        }

        if count_logs(bot) <= column_before {
            blacklist.insert((x, z)); // got nothing from this column
            if !arr {
                consecutive_fail += 1; // couldn't even reach it — likely a barrier
            }
        } else {
            consecutive_fail = 0;
        }
    }

    let logs = count_logs(bot);
    if logs >= target {
        success(format!("gathered {logs}/{target} logs"))
    } else {
        failure(format!("gathered {logs}/{target} logs"))
    }
}
