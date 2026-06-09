//! Wood gathering — find the nearest reachable log, walk to it, mine it, collect
//! the drop, repeat. Blacklists tree columns we fail to reach/mine and explores
//! when none are nearby. Port of steve's `tasks/gather-wood`.

use std::collections::HashSet;

use rustcraft::bot::Bot;

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
fn find_trunk_raw(bot: &Bot, blacklist: &HashSet<(i32, i32, i32)>) -> Option<(i32, i32, i32)> {
    let p = bot.entity.position;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let mut best: Option<(i32, i32, i32)> = None;
    let mut best_key = f64::MAX;
    for dx in -4..=4 {
        for dz in -4..=4 {
            for dy in -2..=4 {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
                if blacklist.contains(&(x, y, z)) {
                    continue;
                }
                if is_log_at(bot, x, y, z) {
                    // Nearest log, but never chase logs below us (that sinks the
                    // bot into pits) — heavily penalize lower Y.
                    let below = if dy < 0 { 20.0 * (-dy) as f64 } else { 0.0 };
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

/// Nearest log near the bot's OWN level, by raw chunk scan (no line-of-sight, so
/// it finds reachable trunk bases — not just visible treetops). Scans a wide
/// horizontal radius but a narrow vertical band so we only target logs the bot
/// can actually walk to. Skips blacklisted (x,z) columns; prefers near + level.
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
            for dy in -5..=5 {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
                if is_log_at(bot, x, y, z) {
                    // Weight vertical distance heavily — strongly prefer same-level
                    // logs (reachable) over higher/lower ones.
                    let d = (dx * dx + dz * dz) as f64 + 8.0 * (dy.abs() as f64);
                    if d < best_d {
                        best_d = d;
                        best = Some((x, y, z));
                    }
                    break; // one log per column is enough to mark the tree
                }
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

/// Collect the log dropped near the dug block. Pathfinds (drop-limited, so the
/// bot doesn't sink into pits chasing items) to the item entity nearest the dug
/// block, then waits for auto-pickup. Item entities are tracked from add_entity.
async fn collect_at(bot: &mut Bot<'_>, x: i32, z: i32) {
    let item_type = bot.registry.entities_by_name.get("item").map(|d| d.id);
    let logs0 = count_logs(bot);
    // Brief raw walk toward the nearest item near the dug block (fast — the drop
    // is right where we just chopped). Re-aims each tick at the live item.
    for _ in 0..22 {
        if count_logs(bot) > logs0 {
            break;
        }
        let bp = bot.entity.position;
        let mut tx = x as f64 + 0.5;
        let mut tz = z as f64 + 0.5;
        let mut best = 6.0;
        for e in bot.entities.values() {
            if item_type.is_some() && e.entity_type != item_type {
                continue;
            }
            let dxz = ((e.position.x - (x as f64 + 0.5)).powi(2) + (e.position.z - (z as f64 + 0.5)).powi(2)).sqrt();
            if dxz < best {
                best = dxz;
                tx = e.position.x;
                tz = e.position.z;
            }
        }
        bot.look_at(rustcraft::vec3::vec3(tx, bp.y - 0.5, tz));
        bot.set_control_state("forward", true);
        if bot.drive_tick().await.map(|s| matches!(s, rustcraft::bot::DriveStep::Disconnected)).unwrap_or(true) {
            break;
        }
    }
    bot.clear_control_states();
    bot.wait_ticks(3).await.ok();
}

const PLACEABLE: &[&str] = &[
    "dirt", "cobblestone", "cobbled_deepslate", "oak_planks", "spruce_planks", "birch_planks",
    "oak_log", "birch_log", "spruce_log", "jungle_log", "acacia_log", "dark_oak_log",
];

/// Pillar straight up `height` blocks by placing a block under the bot each jump
/// (classic tower-up) so it can reach logs on higher ground. Uses any placeable
/// block in the inventory. Returns true if it climbed most of the way.
/// Count placeable blocks the bot is holding.
fn placeable_count(bot: &Bot) -> i32 {
    PLACEABLE.iter().map(|n| crate::bot_utils::count_items(bot, n)).sum()
}

/// Hand-dig a few soft blocks around the bot (basin walls) to get pillar
/// material when we have none.
async fn dig_for_blocks(bot: &mut Bot<'_>) {
    let want = 4;
    // Prefer the FLOOR blocks beside the bot (dy=-1) — in an open basin the
    // feet-level neighbours are air. Digging a neighbouring floor block yields
    // dirt and leaves the bot standing on its own block.
    let offsets = [
        (1, -1, 0), (-1, -1, 0), (0, -1, 1), (0, -1, -1),
        (1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1),
        (1, 1, 0), (0, 1, 1),
    ];
    for (dx, dy, dz) in offsets {
        if placeable_count(bot) >= want {
            break;
        }
        let p = bot.entity.position;
        let (bx, by, bz) = (p.x.floor() as i32 + dx, p.y.floor() as i32 + dy, p.z.floor() as i32 + dz);
        let soft = bot
            .block_at(bx, by, bz)
            .map(|b| {
                let n = &b.name;
                n.contains("dirt") || n.contains("grass") || n.contains("sand") || n.contains("gravel")
            })
            .unwrap_or(false);
        if soft && bot.dig(bx, by, bz).await.is_ok() {
            collect_at(bot, bx, bz).await;
        }
    }
}

async fn pillar_up(bot: &mut Bot<'_>, height: i32) -> bool {
    if placeable_count(bot) < 2 {
        dig_for_blocks(bot).await;
    }
    let mut have = false;
    for name in PLACEABLE {
        if crate::bot_utils::select_item(bot, name).await.unwrap_or(false) {
            have = true;
            break;
        }
    }
    if !have {
        return false;
    }
    let overall_start = bot.entity.position.y;
    for _ in 0..height {
        if placeable_count(bot) == 0 {
            break;
        }
        let start_y = bot.entity.position.y;
        let floor_y = start_y.floor() as i32;
        let fx = bot.entity.position.x.floor() as i32;
        let fz = bot.entity.position.z.floor() as i32;
        // Look straight down at the block we're standing on.
        bot.look_at(rustcraft::vec3::vec3(fx as f64 + 0.5, (floor_y - 1) as f64 + 0.5, fz as f64 + 0.5));
        bot.set_control_state("jump", true);
        // Wait until we've actually risen a full block (so the new block won't
        // intersect us and the server accepts the placement).
        let mut rose = false;
        for _ in 0..14 {
            if matches!(bot.drive_tick().await, Ok(rustcraft::bot::DriveStep::Disconnected) | Err(_)) {
                break;
            }
            if bot.entity.position.y >= start_y + 1.0 {
                rose = true;
                break;
            }
        }
        if rose {
            // Place a block where our feet just were (against the top of the
            // block below), then drop onto it.
            let _ = bot.place_block(fx, floor_y - 1, fz, rustcraft::bot::Face::Top).await;
            bot.wait_ticks(1).await.ok();
        }
        bot.set_control_state("jump", false);
        for _ in 0..16 {
            if matches!(bot.drive_tick().await, Ok(rustcraft::bot::DriveStep::Disconnected) | Err(_)) {
                break;
            }
            if bot.entity.on_ground {
                break;
            }
        }
        // If we failed to gain height this step, stop trying.
        if bot.entity.position.y < start_y + 0.5 {
            break;
        }
    }
    bot.clear_control_states();
    bot.entity.position.y >= overall_start + 1.0
}

#[allow(dead_code)]
async fn dig_out(bot: &mut Bot<'_>, dir_attempt: i32) {
    let yaw = dir_attempt as f64 * 1.6;
    let fx = (-yaw.sin()).round() as i32;
    let fz = yaw.cos().round() as i32;
    for _ in 0..6 {
        let p = bot.entity.position;
        let (px, py, pz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        for (dx, dy, dz) in [(fx, 0, fz), (fx, 1, fz), (fx, 2, fz)] {
            let (bx, by, bz) = (px + dx, py + dy, pz + dz);
            // Only hand-dig soft blocks (dirt/sand/leaves/etc.) — never grind
            // stone by hand (minutes per block, no pickaxe yet).
            let soft = bot
                .block_at(bx, by, bz)
                .map(|b| {
                    let n = &b.name;
                    n.contains("dirt") || n.contains("grass") || n.contains("sand") || n.contains("gravel")
                        || n.contains("leaves") || n.contains("snow") || n.contains("mud") || n.contains("clay")
                })
                .unwrap_or(false);
            if soft {
                if bot.dig_toward(bx, by, bz).await.is_err() {
                    return;
                }
            }
        }
        bot.look(yaw, 0.0);
        bot.set_control_state("forward", true);
        bot.set_control_state("jump", true);
        bot.wait_ticks(6).await.ok();
        bot.clear_control_states();
        if find_trunk_raw(bot, &HashSet::new()).is_some() {
            break;
        }
    }
}

/// Walk a long way in a committed direction (sprinting, jumping obstacles) to
/// leave a hostile area and load new terrain. Stops early if a reachable trunk
/// shows up underfoot.
async fn explore(bot: &mut Bot<'_>, attempt: i32) -> std::io::Result<()> {
    let angle = attempt as f64 * 1.3; // rotate direction across calls
    for _ in 0..90 {
        bot.look(angle, 0.0);
        bot.set_control_state("forward", true);
        bot.set_control_state("sprint", true);
        // Jump to clear lips; dig/auto-step handles the rest.
        bot.set_control_state("jump", true);
        if in_liquid(bot) {
            escape_water(bot).await;
        }
        if matches!(bot.drive_tick().await, Ok(rustcraft::bot::DriveStep::Disconnected) | Err(_)) {
            break;
        }
        if find_trunk_raw(bot, &HashSet::new()).is_some() {
            break; // walked into a reachable tree
        }
    }
    bot.clear_control_states();
    Ok(())
}

pub async fn gather_wood(bot: &mut Bot<'_>, target: i32) -> StepResult {
    bot.wait_ticks(4).await.ok();
    let mut blacklist: HashSet<(i32, i32)> = HashSet::new();
    let mut trunk_bl: HashSet<(i32, i32, i32)> = HashSet::new();
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
            println!("    wood: stuck — pillaring out of the basin + exploring");
            consecutive_fail = 0;
            blacklist.clear();
            trunk_bl.clear();
            // Rise above low terrain to reach trees on the rim, then explore.
            pillar_up(bot, 4).await;
            let _ = explore(bot, attempts * 2).await;
            continue;
        }

        // Find a reachable log near the bot's level (raw scan), else explore.
        let mut found = None;
        for r in [10, 20, 32] {
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
        // Only walk if there's no reachable log right here (skip the slow
        // goto_near when we're already next to a tree we can chop).
        let arr = if find_trunk_raw(bot, &trunk_bl).is_some() {
            true
        } else {
            println!("    wood: log ({x},{y},{z}) d={:.0} — walking", ((x as f64 - p0.x).powi(2) + (z as f64 - p0.z).powi(2)).sqrt());
            match bot.goto_near(x, y, z, 3.0).await {
                Ok(a) => a,
                Err(_) => break,
            }
        };
        let p1 = bot.entity.position;
        println!(
            "    wood: arrived={arr} at ({:.0},{:.0},{:.0}) moved={:.0} trunk={:?}",
            p1.x, p1.y, p1.z, ((p1.x - p0.x).powi(2) + (p1.z - p0.z).powi(2)).sqrt(),
            find_trunk_raw(bot, &trunk_bl)
        );

        // If the log is up on higher ground roughly above us and we couldn't
        // reach it, pillar up to its level so the trunk comes into reach.
        if find_trunk_raw(bot, &trunk_bl).is_none() {
            let bp = bot.entity.position;
            let need = y - bp.y.floor() as i32;
            let horiz = ((x as f64 - bp.x).powi(2) + (z as f64 - bp.z).powi(2)).sqrt();
            if need >= 2 && horiz <= 6.0 {
                println!("    wood: pillaring up {need} to reach the log");
                pillar_up(bot, need.min(6)).await;
            }
        }

        // Tunnel into the tree toward the nearest log, digging whatever is in the
        // way (leaves + logs) and stepping forward, collecting after each log.
        let column_before = count_logs(bot);
        let mut idle = 0;
        let mut cur_trunk: Option<(i32, i32, i32)> = None;
        for _ in 0..28 {
            let Some((tx, ty, tz)) = find_trunk_raw(bot, &trunk_bl) else { break };
            cur_trunk = Some((tx, ty, tz));
            let before = count_logs(bot);
            match bot.dig_toward(tx, ty, tz).await {
                Ok(true) => {
                    collect_at(bot, tx, tz).await;
                }
                Ok(false) => {
                    let above = ty as f64 > bot.entity.position.y + 0.5;
                    bot.look_at(rustcraft::vec3::vec3(tx as f64 + 0.5, ty as f64 + 0.5, tz as f64 + 0.5));
                    bot.set_control_state("forward", true);
                    bot.set_control_state("jump", above);
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
                if idle > 5 {
                    // Give up this log; blacklist it so we don't loop on it.
                    if let Some(t) = cur_trunk {
                        trunk_bl.insert(t);
                    }
                    break;
                }
            }
            if count_logs(bot) >= target {
                break;
            }
        }

        if count_logs(bot) <= column_before {
            blacklist.insert((x, z)); // got nothing from this column
            if let Some(t) = cur_trunk {
                trunk_bl.insert(t); // and the specific log we couldn't break
            }
            consecutive_fail += 1; // made no progress — count it even if "arrived"
            let _ = arr;
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
