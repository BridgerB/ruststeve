//! Wood gathering — find the nearest reachable log, walk to it with the
//! pathfinder, chop the tree, collect the drops, repeat. Blacklists trees it
//! can't reach and explores for new terrain. Port of steve's `tasks/gather-wood`
//! (clean navigate → mine → repeat; no ad-hoc terrain hacks).

use std::collections::HashSet;

use rustcraft::bot::{Bot, DriveStep};
use rustcraft::vec3::vec3;

use crate::types::{failure, success, StepResult};

pub fn count_logs(bot: &Bot) -> i32 {
    bot.inventory.slots.iter().flatten().filter(|i| i.name.ends_with("_log")).map(|i| i.count).sum()
}

fn is_log_at(bot: &Bot, x: i32, y: i32, z: i32) -> bool {
    bot.block_at(x, y, z).map(|b| b.name.ends_with("_log")).unwrap_or(false)
}

/// Nearest log near the bot (raw chunk scan — sees trunks through their own
/// leaves), skipping blacklisted positions. Prefers near + at-or-above the bot.
fn find_trunk_raw(bot: &Bot, blacklist: &HashSet<(i32, i32, i32)>) -> Option<(i32, i32, i32)> {
    let p = bot.entity.position;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let mut best = None;
    let mut best_key = f64::MAX;
    for dx in -4..=4 {
        for dz in -4..=4 {
            for dy in -3..=4 {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
                if blacklist.contains(&(x, y, z)) {
                    continue;
                }
                if is_log_at(bot, x, y, z) {
                    let below = if dy < 0 { 8.0 * (-dy) as f64 } else { 0.0 };
                    let key = (dx * dx + dy * dy + dz * dz) as f64 + below;
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

/// Nearest log near the bot's level (raw scan — finds reachable trunk bases, not
/// just visible treetops). Wide horizontal radius, narrow vertical band.
fn find_log(bot: &Bot, radius: i32, blacklist: &HashSet<(i32, i32)>) -> Option<(i32, i32, i32)> {
    let p = bot.entity.position;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let mut best = None;
    let mut best_d = f64::MAX;
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            if blacklist.contains(&(bx + dx, bz + dz)) {
                continue;
            }
            for dy in -4..=4 {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
                if is_log_at(bot, x, y, z) {
                    let d = (dx * dx + dz * dz) as f64 + 6.0 * (dy.abs() as f64);
                    if d < best_d {
                        best_d = d;
                        best = Some((x, y, z));
                    }
                    break;
                }
            }
        }
    }
    best
}

fn in_liquid(bot: &Bot) -> bool {
    let p = bot.entity.position;
    bot.block_at(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
        .map(|b| b.name.contains("water") || b.name.contains("lava"))
        .unwrap_or(false)
}

/// Any lava within `r` blocks (horizontally, ±1 vertically) of the bot — used to
/// stop the raw (non-pathfinder) walks from blindly stepping into a lava pool.
fn lava_near(bot: &Bot, r: i32) -> bool {
    let p = bot.entity.position;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    for dx in -r..=r {
        for dy in -1..=1 {
            for dz in -r..=r {
                if bot.block_at(bx + dx, by + dy, bz + dz).map(|b| b.name.contains("lava")).unwrap_or(false) {
                    return true;
                }
            }
        }
    }
    false
}

async fn escape_water(bot: &mut Bot<'_>) {
    for _ in 0..40 {
        if !in_liquid(bot) && bot.entity.on_ground {
            break;
        }
        bot.set_control_state("jump", true);
        bot.set_control_state("forward", true);
        bot.set_control_state("sprint", true);
        if bot.drive_tick().await.map(|s| matches!(s, DriveStep::Disconnected)).unwrap_or(true) {
            break;
        }
    }
    bot.clear_control_states();
}

/// Walk to the nearest dropped item near the dug block (fast raw step) to pick it
/// up. Item entities are tracked from add_entity packets.
async fn collect_at(bot: &mut Bot<'_>, x: i32, z: i32) {
    let item_type = bot.registry.entities_by_name.get("item").map(|d| d.id);
    let logs0 = count_logs(bot);
    for _ in 0..20 {
        if count_logs(bot) > logs0 || lava_near(bot, 2) {
            break;
        }
        let bp = bot.entity.position;
        let (mut tx, mut tz) = (x as f64 + 0.5, z as f64 + 0.5);
        let mut best = 6.0;
        for e in bot.entities.values() {
            if item_type.is_some() && e.entity_type != item_type {
                continue;
            }
            let d = ((e.position.x - (x as f64 + 0.5)).powi(2) + (e.position.z - (z as f64 + 0.5)).powi(2)).sqrt();
            if d < best {
                best = d;
                tx = e.position.x;
                tz = e.position.z;
            }
        }
        bot.look_at(vec3(tx, bp.y - 0.5, tz));
        bot.set_control_state("forward", true);
        if bot.drive_tick().await.map(|s| matches!(s, DriveStep::Disconnected)).unwrap_or(true) {
            break;
        }
    }
    bot.clear_control_states();
    bot.wait_ticks(3).await.ok();
}

/// Chop the tree the bot is standing at: repeatedly dig the nearest in-reach log
/// (clearing occluding leaves via dig_toward) and collect the drop, until no log
/// is reachable. Returns how many logs were gained.
async fn chop(bot: &mut Bot<'_>, target: i32) -> i32 {
    let start = count_logs(bot);
    let mut trunk_bl: HashSet<(i32, i32, i32)> = HashSet::new();
    let mut idle = 0;
    for _ in 0..30 {
        let Some((tx, ty, tz)) = find_trunk_raw(bot, &trunk_bl) else { break };
        let before = count_logs(bot);
        match bot.dig_toward(tx, ty, tz).await {
            Ok(true) => collect_at(bot, tx, tz).await,
            Ok(false) => {
                // cleared a leaf / out of reach — step toward the log (unless
                // that would walk us into lava).
                if lava_near(bot, 2) {
                    break;
                }
                let above = ty as f64 > bot.entity.position.y + 0.5;
                bot.look_at(vec3(tx as f64 + 0.5, ty as f64 + 0.5, tz as f64 + 0.5));
                bot.set_control_state("forward", true);
                bot.set_control_state("jump", above);
                bot.wait_ticks(4).await.ok();
                bot.clear_control_states();
            }
            Err(_) => break,
        }
        if count_logs(bot) > before {
            idle = 0;
        } else {
            idle += 1;
            if idle > 4 {
                trunk_bl.insert((tx, ty, tz));
                idle = 0;
            }
        }
        if count_logs(bot) >= target {
            break;
        }
    }
    count_logs(bot) - start
}

/// Sprint-walk a long way in a committed direction to leave a dead-end area and
/// load fresh terrain. Stops early if a reachable log turns up.
async fn explore(bot: &mut Bot<'_>, attempt: i32, ticks: u32) {
    let angle = attempt as f64 * 1.7;
    let empty = HashSet::new();
    for i in 0..ticks {
        // Never walk toward lava — stop and let the next attempt pick a new way.
        if lava_near(bot, 3) {
            break;
        }
        bot.look(angle, 0.0);
        bot.set_control_state("forward", true);
        bot.set_control_state("sprint", true);
        bot.set_control_state("jump", true);
        if in_liquid(bot) {
            escape_water(bot).await;
        }
        if bot.drive_tick().await.map(|s| matches!(s, DriveStep::Disconnected)).unwrap_or(true) {
            break;
        }
        if i % 10 == 0 && find_trunk_raw(bot, &empty).is_some() {
            break;
        }
    }
    bot.clear_control_states();
}

pub async fn gather_wood(bot: &mut Bot<'_>, target: i32) -> StepResult {
    bot.wait_ticks(4).await.ok();
    let mut blacklist: HashSet<(i32, i32)> = HashSet::new();
    let mut attempts = 0;
    let mut failed_trees = 0; // consecutive trees we couldn't get a log from

    while count_logs(bot) < target && attempts < 60 {
        attempts += 1;
        if in_liquid(bot) {
            escape_water(bot).await;
        }

        // Find the nearest reachable log.
        let mut found = None;
        for r in [12, 24, 40] {
            found = find_log(bot, r, &blacklist);
            if found.is_some() {
                break;
            }
        }

        // No tree nearby, or we've struck out on several nearby trees → this area
        // has only unreachable (elevated/cliff) trees; walk far to fresh terrain.
        if found.is_none() || failed_trees >= 4 {
            println!("    wood: leaving this area — exploring for accessible trees");
            blacklist.clear();
            explore(bot, attempts, 200).await;
            failed_trees = 0;
            continue;
        }

        let (x, y, z) = found.unwrap();
        let p0 = bot.entity.position;
        let d = ((x as f64 - p0.x).powi(2) + (z as f64 - p0.z).powi(2)).sqrt();
        println!("    wood: tree ({x},{y},{z}) d={d:.0} — walking");
        let arrived = bot.goto_near(x, y, z, 3.0).await.unwrap_or(false);

        let gained = chop(bot, target).await;
        if gained > 0 {
            println!("    wood: {} logs", count_logs(bot));
            failed_trees = 0;
        } else {
            blacklist.insert((x, z)); // couldn't get a log here — skip this tree
            // Only count it against the area if we couldn't even reach it.
            if !arrived {
                failed_trees += 1;
            }
        }
    }

    let logs = count_logs(bot);
    if logs >= target {
        success(format!("gathered {logs}/{target} logs"))
    } else {
        failure(format!("gathered {logs}/{target} logs"))
    }
}
