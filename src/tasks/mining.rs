//! Mining — gather a target count of a block, digging down to reach it when
//! there's none in reach. Port of the core of steve's `tasks/mining` (surface +
//! dig-down; deep-ore strip mining comes later).

use std::time::{Duration, Instant};

use rustcraft::bot::Bot;

use crate::bot_utils::{collect_drops, select_item};
use crate::types::{failure, success, StepResult};

/// Cobblestone (incl. deepslate) count — the drops from mining stone.
fn count_cobble(bot: &Bot) -> i32 {
    bot.inventory
        .slots
        .iter()
        .flatten()
        .filter(|i| i.name == "cobblestone" || i.name == "cobbled_deepslate")
        .map(|i| i.count)
        .sum()
}

fn is_stone(name: &str) -> bool {
    matches!(
        name,
        "stone" | "cobblestone" | "deepslate" | "cobbled_deepslate" | "andesite" | "diorite" | "granite" | "tuff"
    )
}

fn is_stone_at(bot: &Bot, x: i32, y: i32, z: i32) -> bool {
    bot.block_at(x, y, z).map(|b| is_stone(&b.name)).unwrap_or(false)
}

/// Nearest stone block within `r` of the bot (raw scan; no LOS needed).
fn find_stone(bot: &Bot, r: i32) -> Option<(i32, i32, i32)> {
    let p = bot.entity.position;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let mut best = None;
    let mut best_d = f64::MAX;
    for dx in -r..=r {
        for dy in -r..=r {
            for dz in -r..=r {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
                if is_stone_at(bot, x, y, z) {
                    let d = (dx * dx + dy * dy + dz * dz) as f64;
                    if d < best_d {
                        best_d = d;
                        best = Some((x, y, z));
                    }
                }
            }
        }
    }
    best
}

/// Mine the block under the bot's feet so it descends one level. Returns false
/// if it can't (lava below, or the dig failed).
async fn dig_down(bot: &mut Bot<'_>) -> bool {
    let p = bot.entity.position;
    let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let below = (x, y - 1, z);
    if bot.block_at(below.0, below.1, below.2).map(|b| b.name.contains("lava")).unwrap_or(false) {
        return false;
    }
    if y - 1 <= bot.game.min_y + 4 {
        return false;
    }
    if bot.block_state_at(below.0, below.1, below.2) == 0 {
        bot.wait_ticks(8).await.ok();
        return true;
    }
    if bot.dig(below.0, below.1, below.2).await.is_err() {
        return false;
    }
    bot.wait_ticks(8).await.ok(); // fall into the hole
    true
}

// ── Ore mining (coal / iron) ───────────────────────────────────────────────

/// Does block `name` match the requested ore family?
fn ore_block_matches(name: &str, ore: &str) -> bool {
    match ore {
        "coal" => name == "coal_ore" || name == "deepslate_coal_ore",
        "iron" => name == "iron_ore" || name == "deepslate_iron_ore",
        "gold" => name == "gold_ore" || name == "deepslate_gold_ore",
        _ => false,
    }
}

/// How much of the ore's RESOURCE the bot has (what the step counts).
fn count_ore_resource(bot: &Bot, ore: &str) -> i32 {
    let c = |name: &str| -> i32 {
        bot.inventory.slots.iter().flatten().filter(|i| i.name == name).map(|i| i.count).sum()
    };
    match ore {
        "coal" => c("coal"),
        "iron" => c("raw_iron") + c("iron_ingot"),
        _ => 0,
    }
}

/// Mine reachable ore + its connected vein. Returns how many resource units the
/// bot gained (0 if it couldn't actually get to the ore).
async fn mine_vein(bot: &mut Bot<'_>, ore: &str, tx: i32, ty: i32, tz: i32) -> i32 {
    let before = count_ore_resource(bot, ore);
    // Walk into reach (the pathfinder digs a horizontal/diagonal path to it).
    let _ = bot.goto_near(tx, ty, tz, 1.8).await;
    // Mine the ore and any directly-touching ore (a vein), finishing with
    // dig_toward which carves through the last block or two of stone.
    let mut frontier = vec![(tx, ty, tz)];
    let mut seen = std::collections::HashSet::new();
    while let Some((x, y, z)) = frontier.pop() {
        if !seen.insert((x, y, z)) {
            continue;
        }
        if !bot.block_at(x, y, z).map(|b| ore_block_matches(&b.name, ore)).unwrap_or(false) {
            continue;
        }
        let _ = bot.dig_toward(x, y, z).await;
        collect_drops(bot, x, z).await;
        if bot.block_state_at(x, y, z) == 0 {
            for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                frontier.push((x + dx, y + dy, z + dz));
            }
        }
    }
    count_ore_resource(bot, ore) - before
}

/// Dig one descending stair-step in direction (dx,dz): opens head+feet+floor so
/// the bot drops one level AND leaves a 1-high step it can later walk back up.
/// Returns false if blocked (lava/bedrock) — caller should turn.
async fn descend_step(bot: &mut Bot<'_>, dx: i32, dz: i32) -> bool {
    let p = bot.entity.position;
    let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    if y - 2 <= bot.game.min_y + 4 {
        return false;
    }
    let (nx, nz) = (x + dx, z + dz);
    for (cx, cy, cz) in [(nx, y + 1, nz), (nx, y, nz), (nx, y - 1, nz), (nx, y - 2, nz)] {
        if bot.block_at(cx, cy, cz).map(|b| b.name.contains("lava")).unwrap_or(false) {
            return false;
        }
    }
    // Open a 2-high niche ahead and the step below it, so there's a 1-drop forward.
    let _ = bot.dig(nx, y + 1, nz).await; // head clearance ahead
    let _ = bot.dig(nx, y, nz).await; // feet ahead
    let _ = bot.dig(nx, y - 1, nz).await; // the step down
    // Move onto the step. The pathfinder handles the 1-block forward drop; fall
    // back to a raw walk if it can't compute a path for such a short hop.
    let moved = bot.goto_near(nx, y - 1, nz, 0.7).await.unwrap_or(false);
    if !moved {
        bot.look_at(rustcraft::vec3::vec3(nx as f64 + 0.5, (y - 1) as f64, nz as f64 + 0.5));
        bot.set_control_state("forward", true);
        for _ in 0..10 {
            bot.drive_tick().await.ok();
        }
        bot.clear_control_states();
    }
    let descended = (bot.entity.position.y as i32) < y;
    if std::env::var("MINE_DEBUG").is_ok() {
        eprintln!(
            "descend dir=({dx},{dz}) from y={y} -> y={} dug({nx},{},{nz}) moved={moved} ok={descended}",
            bot.entity.position.y as i32,
            y - 1
        );
    }
    descended
}

/// Strip-tunnel forward ~6 blocks in direction (dx,dz); the pathfinder breaks
/// stone since blocks_cant_break is cleared. Returns whether it advanced.
async fn strip_tunnel(bot: &mut Bot<'_>, dx: i32, dz: i32) -> bool {
    let p = bot.entity.position;
    let (tx, tz) = (p.x.floor() as i32 + dx * 6, p.z.floor() as i32 + dz * 6);
    bot.goto_xz(tx, tz, 1.0).await.unwrap_or(false)
}

/// Mine `target` of an ore resource. Strategy: descend a staircase to ore depth,
/// then strip-tunnel, mining any reachable ore (and its vein) found nearby. The
/// loaded chunk data holds hidden ores so we can beeline toward them.
pub async fn mine_ore(bot: &mut Bot<'_>, ore: &str, target: i32) -> StepResult {
    for tier in ["diamond_pickaxe", "iron_pickaxe", "stone_pickaxe", "wooden_pickaxe"] {
        if select_item(bot, tier).await.unwrap_or(false) {
            break;
        }
    }
    bot.movement.blocks_cant_break.clear();
    // Productive depths: iron peaks low, coal is everywhere — go reasonably deep.
    let depth = if ore == "iron" { 35 } else { 50 };
    let dirs = [(1, 0), (-1, 0), (0, 1), (0, -1)];

    let deadline = Instant::now() + Duration::from_secs(300);
    let mut blacklist: std::collections::HashSet<(i32, i32, i32)> = std::collections::HashSet::new();
    let mut dir = 0usize;
    let mut idle = 0;
    while count_ore_resource(bot, ore) < target && Instant::now() < deadline {
        let by = bot.entity.position.y as i32;
        // 1) If we're well above ore depth, just descend a staircase — don't waste
        //    time chasing unreachable ore in steep high terrain (a mountain spawn).
        if by > depth + 2 {
            // Descend fast via straight dig-down (reliable); if it's blocked (lava
            // below), cut a sideways stair-step to move over and keep going down.
            if !dig_down(bot).await && !descend_step(bot, dirs[dir % 4].0, dirs[dir % 4].1).await {
                dir += 1;
                strip_tunnel(bot, dirs[dir % 4].0, dirs[dir % 4].1).await;
            }
            idle += 1;
            if idle % 8 == 0 {
                println!("    ore: descending to mine {ore} — y={}", bot.entity.position.y as i32);
            }
            continue;
        }
        // 2) At depth — grab any reachable ore nearby (skip failed ones).
        let found = find_ore_excluding(bot, ore, 12, &blacklist);
        if let Some((tx, ty, tz)) = found {
            let gained = mine_vein(bot, ore, tx, ty, tz).await;
            if gained > 0 {
                println!("    ore: {} {ore} (y={})", count_ore_resource(bot, ore), bot.entity.position.y as i32);
                idle = 0;
                continue;
            }
            blacklist.insert((tx, ty, tz)); // couldn't reach it — don't retry
        }
        // 3) No reachable ore here — strip-tunnel to expose more.
        idle += 1;
        if !strip_tunnel(bot, dirs[dir % 4].0, dirs[dir % 4].1).await {
            dir += 1;
        }
        if idle % 12 == 0 {
            println!("    ore: searching {ore} — y={by} have={}", count_ore_resource(bot, ore));
        }
    }

    let n = count_ore_resource(bot, ore);
    if n >= target {
        success(format!("mined {n}/{target} {ore}"))
    } else {
        failure(format!("mined {n}/{target} {ore}"))
    }
}

/// find_ore but skipping blacklisted positions.
fn find_ore_excluding(
    bot: &Bot,
    ore: &str,
    r: i32,
    blacklist: &std::collections::HashSet<(i32, i32, i32)>,
) -> Option<(i32, i32, i32)> {
    let p = bot.entity.position;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let mut best = None;
    let mut best_d = f64::MAX;
    for dx in -r..=r {
        for dy in -r..=r {
            for dz in -r..=r {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
                if blacklist.contains(&(x, y, z)) {
                    continue;
                }
                if bot.block_at(x, y, z).map(|b| ore_block_matches(&b.name, ore)).unwrap_or(false) {
                    let d = (dx * dx + dy * dy + dz * dz) as f64;
                    if d < best_d {
                        best_d = d;
                        best = Some((x, y, z));
                    }
                }
            }
        }
    }
    best
}

/// Mine gravel until we have `target` flint (gravel drops flint ~10%).
pub async fn mine_gravel_for_flint(bot: &mut Bot<'_>, target: i32) -> StepResult {
    bot.movement.blocks_cant_break.clear();
    let count = |bot: &Bot| -> i32 {
        bot.inventory.slots.iter().flatten().filter(|i| i.name == "flint").map(|i| i.count).sum()
    };
    let find_gravel = |bot: &Bot| -> Option<(i32, i32, i32)> {
        let p = bot.entity.position;
        let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let mut best = None;
        let mut best_d = f64::MAX;
        for dx in -20..=20 {
            for dy in -12..=6 {
                for dz in -20..=20 {
                    let (x, y, z) = (bx + dx, by + dy, bz + dz);
                    if bot.block_at(x, y, z).map(|b| b.name == "gravel").unwrap_or(false) {
                        let d = (dx * dx + dy * dy + dz * dz) as f64;
                        if d < best_d {
                            best_d = d;
                            best = Some((x, y, z));
                        }
                    }
                }
            }
        }
        best
    };
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut blacklist = std::collections::HashSet::new();
    while count(bot) < target && Instant::now() < deadline {
        match find_gravel(bot) {
            Some(pos) if !blacklist.contains(&pos) => {
                let _ = bot.goto_near(pos.0, pos.1, pos.2, 2.0).await;
                let _ = bot.dig_toward(pos.0, pos.1, pos.2).await;
                collect_drops(bot, pos.0, pos.2).await;
                blacklist.insert(pos);
            }
            _ => {
                if !dig_down(bot).await {
                    break;
                }
            }
        }
    }
    let n = count(bot);
    if n >= target {
        success(format!("got {n} flint"))
    } else {
        failure(format!("only {n} flint"))
    }
}

pub async fn mine_stone(bot: &mut Bot<'_>, target: i32) -> StepResult {
    // Equip the best available pickaxe.
    for tier in ["diamond_pickaxe", "iron_pickaxe", "stone_pickaxe", "wooden_pickaxe"] {
        if select_item(bot, tier).await.unwrap_or(false) {
            break;
        }
    }
    // Now that we hold a pickaxe, allow the pathfinder to break stone again
    // (it's blocked by default so wood-gathering doesn't wedge on stone).
    bot.movement.blocks_cant_break.clear();

    let deadline = Instant::now() + Duration::from_secs(100);
    let mut no_progress = 0;
    while count_cobble(bot) < target && Instant::now() < deadline {
        if let Some((tx, ty, tz)) = find_stone(bot, 4) {
            let _ = bot.goto_near(tx, ty, tz, 2.5).await;
            let before = count_cobble(bot);
            if bot.dig(tx, ty, tz).await.is_err() {
                break;
            }
            collect_drops(bot, tx, tz).await;
            if count_cobble(bot) > before {
                println!("    stone: {} cobblestone", count_cobble(bot));
                no_progress = 0;
            } else {
                no_progress += 1;
                if no_progress > 4 && !dig_down(bot).await {
                    break;
                }
            }
        } else {
            // No stone in reach — dig down toward the stone layer.
            if !dig_down(bot).await {
                break;
            }
        }
    }

    let n = count_cobble(bot);
    if n >= target {
        success(format!("mined {n}/{target} cobblestone"))
    } else {
        failure(format!("mined {n}/{target} cobblestone"))
    }
}
