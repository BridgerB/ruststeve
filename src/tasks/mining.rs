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

/// Nearest matching ore block in the loaded world within `r` (the chunk data
/// holds hidden ores, so the pathfinder can tunnel to them).
fn find_ore(bot: &Bot, ore: &str, r: i32) -> Option<(i32, i32, i32)> {
    let p = bot.entity.position;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let mut best = None;
    let mut best_d = f64::MAX;
    for dx in -r..=r {
        for dy in -r..=r {
            for dz in -r..=r {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
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

/// Mine `target` of an ore resource. Finds ore in the loaded chunks and lets the
/// pathfinder dig through stone to it; descends to find more when none is near.
pub async fn mine_ore(bot: &mut Bot<'_>, ore: &str, target: i32) -> StepResult {
    for tier in ["diamond_pickaxe", "iron_pickaxe", "stone_pickaxe", "wooden_pickaxe"] {
        if select_item(bot, tier).await.unwrap_or(false) {
            break;
        }
    }
    bot.movement.blocks_cant_break.clear();

    let deadline = Instant::now() + Duration::from_secs(240);
    let mut no_progress = 0;
    while count_ore_resource(bot, ore) < target && Instant::now() < deadline {
        if let Some((tx, ty, tz)) = find_ore(bot, ore, 28) {
            let _ = bot.goto_near(tx, ty, tz, 2.0).await;
            let before = count_ore_resource(bot, ore);
            // Mine the ore and any ore neighbours (vein).
            for (dx, dy, dz) in [(0, 0, 0), (1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                let (x, y, z) = (tx + dx, ty + dy, tz + dz);
                if bot.block_at(x, y, z).map(|b| ore_block_matches(&b.name, ore)).unwrap_or(false) {
                    let _ = bot.dig_toward(x, y, z).await;
                    collect_drops(bot, x, z).await;
                }
            }
            if count_ore_resource(bot, ore) > before {
                println!("    ore: {} {ore}", count_ore_resource(bot, ore));
                no_progress = 0;
            } else {
                no_progress += 1;
            }
        } else {
            // No ore visible nearby — descend toward ore-bearing depths.
            if !dig_down(bot).await {
                break;
            }
        }
        if no_progress > 8 {
            no_progress = 0;
            if !dig_down(bot).await {
                break;
            }
        }
    }

    let n = count_ore_resource(bot, ore);
    if n >= target {
        success(format!("mined {n}/{target} {ore}"))
    } else {
        failure(format!("mined {n}/{target} {ore}"))
    }
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
