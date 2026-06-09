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

/// Nearest reachable log near the BOT (raw chunk scan — finds trunks occluded by
/// their own leaves, no line-of-sight needed). Scans a radius around the bot's
/// current position so we only ever target logs we actually walked up to.
fn find_trunk_raw(bot: &Bot) -> Option<(i32, i32, i32)> {
    let p = bot.entity.position;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let mut best: Option<(i32, i32, i32)> = None;
    let mut best_key = (i32::MAX, f64::MAX); // prefer lower Y, then nearer
    for dx in -4..=4 {
        for dz in -4..=4 {
            for dy in -4..=4 {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
                if is_log_at(bot, x, y, z) {
                    let key = (y, (dx * dx + dz * dz) as f64);
                    if key < best_key {
                        best_key = key;
                        best = Some((x, y, z));
                    }
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
    let drop = rustcraft::vec3::vec3(x as f64 + 0.5, 0.0, z as f64 + 0.5);
    for _ in 0..40 {
        let bp = bot.entity.position;
        // Target the item entity nearest the DUG BLOCK (the fresh drop), not the
        // globally nearest item — busy servers have many unrelated drops.
        let mut target = rustcraft::vec3::vec3(x as f64 + 0.5, bp.y, z as f64 + 0.5);
        let mut best = 6.0; // within 6 blocks (xz) of the dug block
        for e in bot.entities.values() {
            if item_type.is_some() && e.entity_type != item_type {
                continue;
            }
            let dxz = ((e.position.x - drop.x).powi(2) + (e.position.z - drop.z).powi(2)).sqrt();
            if dxz < best {
                best = dxz;
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
            find_trunk_raw(bot)
        );

        // Tunnel into the tree toward the nearest log, digging whatever is in the
        // way (leaves + logs) and stepping forward, collecting after each log.
        let column_before = count_logs(bot);
        let mut idle = 0;
        for _ in 0..28 {
            let Some((tx, ty, tz)) = find_trunk_raw(bot) else { break };
            let before = count_logs(bot);
            match bot.dig_toward(tx, ty, tz).await {
                Ok(true) => {
                    // Broke a log — go pick up its drop.
                    collect_at(bot, tx, tz).await;
                }
                Ok(false) => {
                    // Cleared a leaf (or out of reach) — step toward the log to
                    // tunnel in and get within range.
                    bot.look_at(rustcraft::vec3::vec3(tx as f64 + 0.5, ty as f64 + 0.5, tz as f64 + 0.5));
                    bot.set_control_state("forward", true);
                    bot.wait_ticks(4).await.ok();
                    bot.clear_control_states();
                }
                Err(_) => return failure(format!("disconnected; {} logs", count_logs(bot))),
            }
            if count_logs(bot) > before {
                println!("    wood: {} logs", count_logs(bot));
                idle = 0;
            } else {
                idle += 1;
                if idle > 10 {
                    break; // no progress tunnelling — give up this tree
                }
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
