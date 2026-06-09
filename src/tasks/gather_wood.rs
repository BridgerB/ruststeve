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

        // Walk to the tree's COLUMN at our own level (not up to the canopy log,
        // which we can't climb). The trunk base becomes reachable once there.
        let by = bot.entity.position.y.floor() as i32;
        println!("    wood: tree at ({x},{y},{z}) — walking to column");
        match bot.goto(x, by, z).await {
            Ok(_) => {}
            Err(_) => break,
        }

        // Mine every reachable log in this column, bottom-up, until none in reach.
        let before = count_logs(bot);
        let mut mined_any = false;
        for _ in 0..6 {
            // lowest log within reach (trunk base first)
            let mut best: Option<(i32, i32, i32)> = None;
            for &log in LOG_TYPES {
                for (lx, ly, lz) in bot.find_blocks(log, 5, 16) {
                    if best.map(|(_, by2, _)| ly < by2).unwrap_or(true) {
                        best = Some((lx, ly, lz));
                    }
                }
            }
            let Some((tx, ty, tz)) = best else { break };
            if bot.dig(tx, ty, tz).await.is_err() {
                return failure(format!("disconnected; {} logs", count_logs(bot)));
            }
            let _ = bot.goto(tx, ty, tz).await; // step onto the drop
            bot.wait_ticks(8).await.ok();
            mined_any = true;
            if count_logs(bot) >= target {
                break;
            }
        }

        let now = count_logs(bot);
        if now > before {
            println!("    wood: {before} → {now} logs");
        } else if !mined_any {
            blacklist.insert((x, z)); // nothing reachable in this column
        }
    }

    let logs = count_logs(bot);
    if logs >= target {
        success(format!("gathered {logs}/{target} logs"))
    } else {
        failure(format!("gathered {logs}/{target} logs"))
    }
}
