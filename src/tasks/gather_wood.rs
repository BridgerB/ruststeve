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

/// Step toward a just-dug block for ~1.5s to walk over the dropped item.
async fn collect_at(bot: &mut Bot<'_>, x: i32, z: i32) {
    let logs0 = count_logs(bot);
    for _ in 0..30 {
        let p = bot.entity.position;
        bot.look_at(rustcraft::vec3::vec3(x as f64 + 0.5, p.y - 0.5, z as f64 + 0.5));
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

    while count_logs(bot) < target && attempts < 40 {
        attempts += 1;
        if in_liquid(bot) {
            escape_water(bot).await;
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
        let near = find_log(bot, 5, &HashSet::new());
        println!(
            "    wood: arrived={arr} at ({:.0},{:.0},{:.0}) moved={:.0} reachLog={:?}",
            p1.x, p1.y, p1.z, ((p1.x - p0.x).powi(2) + (p1.z - p0.z).powi(2)).sqrt(), near
        );

        // Mine reachable logs in this column, bottom-up, until none in reach or a
        // dig yields nothing (occluded/too far → give up on this tree).
        let column_before = count_logs(bot);
        for _ in 0..8 {
            // lowest log within range (trunk base first)
            let mut best: Option<(i32, i32, i32)> = None;
            for &log in LOG_TYPES {
                for (lx, ly, lz) in bot.find_blocks(log, 6, 24) {
                    if best.map(|(_, by2, _)| ly < by2).unwrap_or(true) {
                        best = Some((lx, ly, lz));
                    }
                }
            }
            let Some((tx, ty, tz)) = best else { break };

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
        }
    }

    let logs = count_logs(bot);
    if logs >= target {
        success(format!("gathered {logs}/{target} logs"))
    } else {
        failure(format!("gathered {logs}/{target} logs"))
    }
}
