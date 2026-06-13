//! Mining — gather a target count of a block, digging down to reach it when
//! there's none in reach. Port of the core of steve's `tasks/mining` (surface +
//! dig-down; deep-ore strip mining comes later).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use rustcraft::bot::Bot;

use crate::bot_utils::{collect_drops, select_item};
use crate::memory::{PoiKind, PoiStatus, WorldMemory};
use crate::types::{failure, success, StepResult};

/// Remember where we entered the underground — the surface spot a mining run
/// starts from, so the bot can navigate back up to it later.
fn record_descent(bot: &Bot, mem: &mut WorldMemory) {
    let p = bot.entity.position;
    mem.record(
        PoiKind::DescentPoint,
        (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32),
        PoiStatus::Available,
    );
}

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
    let x = p.x.floor() as i32;
    let z = p.z.floor() as i32;
    // The block the bot is STANDING ON. Using floor(y)-1 is wrong when physics
    // jitter dips position.y just under the integer (e.g. 53.92 → floor-1 digs the
    // block one too low, leaving the real support intact). floor(y-0.5) is robust.
    let y = (p.y - 0.5).floor() as i32;
    let below = (x, y, z);
    // Death-AVOIDANCE: never break a block that is liquid, sits directly above
    // liquid, or has liquid beside it (which would flood the hole). Refusing here
    // makes the caller tunnel AROUND aquifers/lava instead of dropping into them.
    if is_liquid_at(bot, x, y, z)
        || is_liquid_at(bot, x, y - 1, z)
        || [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dz)| is_liquid_at(bot, x + dx, y, z + dz))
    {
        return false;
    }
    // Fall-avoidance: don't dig the floor out over a deep drop (open cavern /
    // ravine) — a 4+ block fall hurts and can strand the bot. The controlled
    // `descend_step` is how we go down; here we refuse the plunge.
    if bot.block_state_at(x, y - 1, z) == 0
        && bot.block_state_at(x, y - 2, z) == 0
        && bot.block_state_at(x, y - 3, z) == 0
    {
        return false;
    }
    if y <= bot.game.min_y + 4 {
        return false;
    }
    if bot.block_state_at(below.0, below.1, below.2) == 0 {
        bot.wait_ticks(8).await.ok(); // already open — just let physics drop us
        return true;
    }
    let y_before = bot.entity.position.y;
    if bot.dig(below.0, below.1, below.2).await.is_err() {
        return false;
    }
    bot.wait_ticks(8).await.ok(); // fall into the hole
    // Only report success if we ACTUALLY descended. Digging stone with no (or the
    // wrong) tool doesn't client-predict the break, so the block stays solid and
    // the bot never falls — returning true there spins forever on the same block.
    bot.entity.position.y < y_before - 0.5
}

/// Is there liquid (water or lava) at (x, y, z)?
fn is_liquid_at(bot: &Bot, x: i32, y: i32, z: i32) -> bool {
    bot.block_at(x, y, z)
        .map(|b| b.name.contains("water") || b.name.contains("lava"))
        .unwrap_or(false)
}

/// Holding a pickaxe right now?
fn held_is_pickaxe(bot: &Bot) -> bool {
    bot.held_item().map(|i| i.name.ends_with("_pickaxe")).unwrap_or(false)
}

/// Make sure a pickaxe is in hand; re-equips the best one if the current broke
/// (durability management). False if the bot has no pickaxe at all — the caller
/// should bail so the step machine crafts a replacement instead of mining
/// bare-handed (which on stone yields nothing).
async fn ensure_pickaxe(bot: &mut Bot<'_>) -> bool {
    if held_is_pickaxe(bot) {
        return true;
    }
    for tier in ["diamond_pickaxe", "iron_pickaxe", "stone_pickaxe", "wooden_pickaxe"] {
        if select_item(bot, tier).await.unwrap_or(false) {
            return true;
        }
    }
    false
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
        if is_liquid_at(bot, cx, cy, cz) {
            return false; // don't stair-step into water or lava
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

/// Classify an ore block name into a memory kind + the pickaxe tier it needs.
fn ore_kind_tier(name: &str) -> Option<(PoiKind, i32)> {
    if name.contains("iron_ore") {
        Some((PoiKind::IronOre, 2))
    } else if name.contains("coal_ore") {
        Some((PoiKind::CoalOre, 1))
    } else if name.contains("copper_ore") {
        Some((PoiKind::CopperOre, 2))
    } else if name.contains("gold_ore") {
        Some((PoiKind::GoldOre, 3))
    } else if name.contains("diamond_ore") {
        Some((PoiKind::DiamondOre, 3))
    } else if name.contains("redstone_ore") {
        Some((PoiKind::RedstoneOre, 3))
    } else if name.contains("lapis_ore") {
        Some((PoiKind::LapisOre, 2))
    } else {
        None
    }
}

/// State id → (memory kind, required tier) for every ore block, so observation
/// can classify by raw state id with no per-block allocation.
fn ore_states(bot: &Bot) -> HashMap<u32, (PoiKind, i32)> {
    let mut m = HashMap::new();
    for b in &bot.registry.blocks_array {
        if let Some((kind, tier)) = ore_kind_tier(&b.name) {
            for s in b.min_state_id..=b.max_state_id {
                m.insert(s, (kind, tier));
            }
        }
    }
    m
}

/// Best pickaxe tier in the inventory (0 none … 4 diamond/netherite).
fn pickaxe_tier_rank(bot: &Bot) -> i32 {
    let mut best = 0;
    for it in bot.inventory.slots.iter().flatten() {
        let r = match it.name.as_str() {
            "wooden_pickaxe" | "golden_pickaxe" => 1,
            "stone_pickaxe" => 2,
            "iron_pickaxe" => 3,
            "diamond_pickaxe" | "netherite_pickaxe" => 4,
            _ => 0,
        };
        best = best.max(r);
    }
    best
}

/// Record every ore the bot can currently see (through stone, via chunk data) in
/// a box around it — this is the bot's "I noticed ore over there" memory.
fn observe_blocks(bot: &Bot, mem: &mut WorldMemory, ores: &HashMap<u32, (PoiKind, i32)>) {
    let p = bot.entity.position;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    const R: i32 = 10;
    let mut seen = 0;
    for dx in -R..=R {
        for dy in -R..=R {
            for dz in -R..=R {
                let s = bot.block_state_at(bx + dx, by + dy, bz + dz);
                if let Some(&(kind, tier)) = ores.get(&s) {
                    mem.observe(kind, (bx + dx, by + dy, bz + dz), PoiStatus::NeedsTool(tier));
                    seen += 1;
                }
            }
        }
    }
    if seen > 0 {
        mem.log("observe", "ores", &format!("{seen} ore blocks near ({bx},{by},{bz})"));
    }
}

/// Mine `target` of an ore resource. The bot's memory is the index: it OBSERVES
/// ores it sees into SQLite, then QUERIES the DB for the nearest usable one and
/// goes mines it. Only when the DB has nothing does it explore — searching SOUTH
/// (descending to ore depth, then tunnelling +Z to load fresh terrain) in a loop
/// until ore turns up, recording everything it finds along the way.
pub async fn mine_ore(bot: &mut Bot<'_>, ore: &str, target: i32, mem: &mut WorldMemory) -> StepResult {
    record_descent(bot, mem);
    mem.log("mine_ore", "begin", &format!("{ore} target={target}"));
    for tier in ["diamond_pickaxe", "iron_pickaxe", "stone_pickaxe", "wooden_pickaxe"] {
        if select_item(bot, tier).await.unwrap_or(false) {
            break;
        }
    }
    bot.movement.blocks_cant_break.clear();
    let ores = ore_states(bot);
    let kind = match ore {
        "coal" => PoiKind::CoalOre,
        "gold" => PoiKind::GoldOre,
        "diamond" => PoiKind::DiamondOre,
        _ => PoiKind::IronOre,
    };
    // Productive depths: iron peaks low, coal is everywhere — go reasonably deep.
    let depth = if ore == "iron" { 35 } else { 50 };

    let deadline = Instant::now() + Duration::from_secs(300);
    let mut iters = 0u32;
    let mut stuck = 0u32;
    let mut last_count = count_ore_resource(bot, ore);
    let mut last_pos = {
        let p = bot.entity.position;
        (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
    };
    while count_ore_resource(bot, ore) < target && Instant::now() < deadline {
        iters += 1;
        // Progress / stuck tracking (vs the previous iteration).
        let now_count = count_ore_resource(bot, ore);
        let now_pos = {
            let p = bot.entity.position;
            (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
        };
        if now_count > last_count || now_pos != last_pos {
            stuck = 0;
        } else {
            stuck += 1;
        }
        last_count = now_count;
        last_pos = now_pos;
        if stuck > 30 {
            mem.log("mine_ore", "stuck", &format!("y={} have={}", now_pos.1, now_count));
            println!("    ore: stuck — bailing");
            break;
        }
        // Durability: re-equip a pickaxe if the held one broke; bail to re-craft
        // if we have none at all.
        if !ensure_pickaxe(bot).await {
            mem.log("mine_ore", "no_pickaxe", "bailing to re-craft");
            break;
        }
        // Notice ores around us (cheap, throttled) and write them to memory.
        if iters % 4 == 1 {
            observe_blocks(bot, mem, &ores);
        }
        let from = {
            let p = bot.entity.position;
            (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
        };
        let tier = pickaxe_tier_rank(bot);

        // 1) Ask the DB where the ore is. If it knows one, go mine it. NO can_reach
        // pre-check: ore is embedded in stone, and mine_vein DIGS through stone to
        // reach it — a walk-only reachability test wrongly rejects diggable ore (it
        // marked 14 perfectly-mineable irons "unreachable"). Let mine_vein try; only
        // if it actually gains nothing do we give up on that spot.
        if let Some(tpos) = mem.nearest(&[kind], from, tier).map(|p| p.pos) {
            mem.log("mine_ore", "target", &format!("{ore} {tpos:?}"));
            let gained = mine_vein(bot, ore, tpos.0, tpos.1, tpos.2).await;
            observe_blocks(bot, mem, &ores); // mined blocks are air now
            if gained > 0 {
                mem.mark(tpos, PoiStatus::Gone);
                println!("    ore: {} {ore} (y={})", count_ore_resource(bot, ore), bot.entity.position.y as i32);
                mem.log("mine_ore", "mined", &format!("+{gained} {ore} total={}", count_ore_resource(bot, ore)));
            } else {
                mem.mark(tpos, PoiStatus::Unreachable);
            }
            continue;
        }

        // 2) DB has nothing → search SOUTH until ore turns up.
        let by = bot.entity.position.y as i32;
        if by > depth + 2 {
            // Get down to ore depth first.
            if !dig_down(bot).await && !descend_step(bot, 0, 1).await {
                strip_tunnel(bot, 0, 1).await;
            }
            if iters % 8 == 0 {
                println!("    ore: descending toward {ore} — y={}", bot.entity.position.y as i32);
            }
        } else {
            // At depth — tunnel south to load + expose fresh terrain.
            let moved = strip_tunnel(bot, 0, 1).await;
            mem.log("mine_ore", "search_south", &format!("y={by} moved={moved}"));
            if iters % 8 == 0 {
                println!("    ore: searching south for {ore} — y={by} have={}", count_ore_resource(bot, ore));
            }
        }
    }

    let n = count_ore_resource(bot, ore);
    mem.log("mine_ore", "end", &format!("{n}/{target} {ore}"));
    if n >= target {
        success(format!("mined {n}/{target} {ore}"))
    } else {
        failure(format!("mined {n}/{target} {ore}"))
    }
}

/// Mine gravel until we have `target` flint (gravel drops flint ~10%).
pub async fn mine_gravel_for_flint(bot: &mut Bot<'_>, target: i32, mem: &mut WorldMemory) -> StepResult {
    record_descent(bot, mem);
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

pub async fn mine_stone(bot: &mut Bot<'_>, target: i32, mem: &mut WorldMemory) -> StepResult {
    record_descent(bot, mem);
    // Equip the best available pickaxe.
    for tier in ["diamond_pickaxe", "iron_pickaxe", "stone_pickaxe", "wooden_pickaxe"] {
        if select_item(bot, tier).await.unwrap_or(false) {
            break;
        }
    }
    // Now that we hold a pickaxe, allow the pathfinder to break stone again
    // (it's blocked by default so wood-gathering doesn't wedge on stone).
    bot.movement.blocks_cant_break.clear();
    let dbg = std::env::var("MINE_DEBUG").is_ok();
    if dbg {
        let p = bot.entity.position;
        eprintln!("MINE_STONE start: held={:?} at ({:.1},{:.1},{:.1})", bot.held_item().map(|i| i.name.clone()), p.x, p.y, p.z);
    }

    let deadline = Instant::now() + Duration::from_secs(100);
    let mut no_progress = 0;
    while count_cobble(bot) < target && Instant::now() < deadline {
        // Durability: re-equip a pickaxe if the held one broke; bail to re-craft
        // if we have none (don't mine stone bare-handed — it drops nothing).
        if !ensure_pickaxe(bot).await {
            println!("    stone: no pickaxe — stopping to re-craft");
            break;
        }
        // Stuck: bail after a long no-progress streak instead of grinding the
        // whole deadline against a wall.
        if no_progress > 25 {
            println!("    stone: stuck (no cobble progress) — bailing");
            break;
        }
        if let Some((tx, ty, tz)) = find_stone(bot, 4) {
            let _ = bot.goto_near(tx, ty, tz, 2.5).await;
            let before = count_cobble(bot);
            let held = bot.held_item().map(|i| i.name.clone());
            let bname = bot.block_at(tx, ty, tz).map(|b| b.name.clone());
            if bot.dig(tx, ty, tz).await.is_err() {
                break;
            }
            collect_drops(bot, tx, tz).await;
            if dbg {
                eprintln!("MINE_STONE dig ({tx},{ty},{tz}) block={bname:?} held={held:?} cobble {before}->{}", count_cobble(bot));
            }
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
