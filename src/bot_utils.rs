//! Action helpers built on rustcraft's bot — item counting, log finding,
//! generic crafting, and crafting-table placement. Port of the slice of steve's
//! `lib/bot-utils.ts` the early phases use.

use crate::bot::{Bot, DriveStep, Face};
use crate::path::{get_path_to, GoalNear, PathStatus};

use crate::memory::{PoiKind, PoiStatus, WorldMemory};
use crate::types::{failure, success, StepResult};

/// Is the bot's head submerged in water (i.e. it's drowning if it stays)?
pub fn head_in_water(bot: &Bot) -> bool {
    let p = bot.entity.position;
    let (x, hy, z) = (p.x.floor() as i32, (p.y + 1.0).floor() as i32, p.z.floor() as i32);
    bot.block_at(x, hy, z).map(|b| b.name.contains("water")).unwrap_or(false)
}

/// Are the bot's feet in water? Together with `head_in_water`, "fully out" means the
/// bot has actually left the pool, not just bobbed its head to the surface for a tick.
pub fn feet_in_water(bot: &Bot) -> bool {
    let p = bot.entity.position;
    let (x, fy, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    bot.block_at(x, fy, z).map(|b| b.name.contains("water")).unwrap_or(false)
}

/// Nearest horizontal direction toward an exit from open water: a column (within
/// `r`) whose head-height block is AIR — the bank/shallows. typecraft water has NO
/// swim-buoyancy (jump gives no lift; only a wall-collision impulse raises the bot),
/// so in an OPEN lake driving `forward` in an arbitrary heading swims DEEPER and the
/// bot drowns ("underwater — surfacing" spammed 7+ times → died at y55-61). Facing
/// the nearest air column first makes the swim go toward the bank, where the rising
/// floor + wall-collision lift carry it out. Returns (dx,dz) to face, or None if
/// fully enclosed by water (then the cap-dig / forward fallback handles it).
fn water_exit_dir(bot: &Bot, r: i32) -> Option<(f64, f64)> {
    let p = bot.entity.position;
    let (bx, hy, bz) = (p.x.floor() as i32, (p.y + 1.0).floor() as i32, p.z.floor() as i32);
    let mut best: Option<((i32, i32), i32)> = None;
    for dx in -r..=r {
        for dz in -r..=r {
            if dx == 0 && dz == 0 {
                continue;
            }
            let (x, z) = (bx + dx, bz + dz);
            let head_air = bot
                .block_at(x, hy, z)
                .map(|b| b.name == "air" || b.name == "cave_air")
                .unwrap_or(false);
            if !head_air {
                continue;
            }
            let d2 = dx * dx + dz * dz;
            if best.map_or(true, |(_, bd)| d2 < bd) {
                best = Some(((dx, dz), d2));
            }
        }
    }
    best.map(|((dx, dz), _)| (dx as f64, dz as f64))
}

/// Swim up/out to air before doing anything else. Returns true once the head is
/// no longer underwater. Used so the bot LEAVES water first instead of mining
/// while submerged (which drowns it) — mining underwater is only a last resort
/// when this can't surface it (boxed in).
/// Is `name` a block you can stand on / place against (not air, water, lava, or a
/// non-solid air variant)?
fn is_standable(name: &str) -> bool {
    !name.is_empty()
        && name != "air"
        && name != "cave_air"
        && name != "void_air"
        && !name.contains("water")
        && !name.contains("lava")
}

/// One pillar cycle from a solid footing: jump and place a throwaway block beneath the
/// feet so the bot rises a block. Returns false only when there's nothing to build with.
async fn pillar_step(bot: &mut Bot<'_>, t: &mut u32) -> bool {
    let block = ["cobblestone", "cobbled_deepslate", "dirt", "netherrack", "stone", "andesite", "diorite", "granite", "tuff", "deepslate"]
        .into_iter()
        .find(|b| count_items(bot, b) > 0);
    let Some(block) = block else { return false };
    if !select_item(bot, block).await.unwrap_or(false) {
        return false;
    }
    let cx = bot.entity.position.x.floor() as i32;
    let cz = bot.entity.position.z.floor() as i32;
    let fy = bot.entity.position.y.floor() as i32;
    bot.look_at(crate::vec3::vec3(cx as f64 + 0.5, (fy - 2) as f64, cz as f64 + 0.5));
    bot.set_control_state("jump", true);
    bot.wait_ticks(6).await.ok();
    // Place on top of our footing (the block one below the feet) → a new block at feet level
    // that we fall onto, one higher.
    if bot.block_at(cx, fy - 1, cz).map(|b| is_standable(&b.name)).unwrap_or(false) {
        let _ = bot.place_block(cx, fy - 1, cz, Face::Top).await;
    }
    bot.wait_ticks(5).await.ok();
    bot.set_control_state("jump", false);
    bot.wait_ticks(3).await.ok();
    *t += 14;
    true
}

/// Leave a body of water. The bot is buoyant — it floats to the surface on its own — so
/// the escape is to swim to the nearest LAND and climb out, not to rise in place (a jump
/// gives no lift once at the surface). Head toward the closest air-headed column, found
/// with a progressively wider scan (a 5-block scan misses a lake edge), commit that heading
/// for several ticks (sustained `forward` reaches the edge; the rising floor / wall carries
/// the bot up), and if pressed against a vertical bank without rising, DIG a foothold into
/// it and step up — a guaranteed exit from any walled pool. Returns true once fully out
/// (neither head nor feet submerged).
pub async fn leave_water(bot: &mut Bot<'_>, ticks: u32) -> bool {
    let mut t = 0;
    let mut best_y = bot.entity.position.y;
    let mut no_rise = 0u32;
    while t < ticks {
        if !head_in_water(bot) && !feet_in_water(bot) {
            bot.clear_control_states();
            return true;
        }
        // Solid ceiling overhead → dig straight up through it (a flooded stairwell's roof, or
        // a cap we've pillared up into: we bob under it and drown otherwise). Check the head
        // block itself AND the one above — digging the LOWEST solid opens the way up.
        let p = bot.entity.position;
        let (x, z) = (p.x.floor() as i32, p.z.floor() as i32);
        let feet_y = p.y.floor() as i32;
        let mut dug_cap = false;
        for cap_y in [feet_y + 1, feet_y + 2] {
            if bot.block_at(x, cap_y, z).map(|b| is_standable(&b.name)).unwrap_or(false) {
                bot.clear_control_states();
                let _ = bot.dig(x, cap_y, z).await;
                t += 4;
                dug_cap = true;
                break;
            }
        }
        if dug_cap {
            continue;
        }

        if head_in_water(bot) {
            // PHASE 1 — submerged and can't swim up (jump gives no lift mid-water). Rise by
            // PILLARING: when resting on a solid (the floor), jump and place a block beneath
            // the feet; when not yet on the floor, settle down onto it first (buoyancy only
            // lifts a block or two, so from depth we must build our way up). Look up so any
            // shallow buoyancy is captured too.
            let p = bot.entity.position;
            bot.look_at(crate::vec3::vec3(p.x, p.y + 3.0, p.z));
            if bot.entity.on_ground {
                if !pillar_step(bot, &mut t).await {
                    // Nothing to pillar with — hold jump and hope for a shallow float, then
                    // the surface phase can take over.
                    bot.set_control_state("jump", true);
                    for _ in 0..6 {
                        let _ = bot.drive_tick().await;
                        t += 1;
                    }
                    bot.clear_control_states();
                }
            } else {
                // Settle onto the floor (or catch a shallow buoyant rise to the surface).
                bot.set_control_state("jump", true);
                for _ in 0..6 {
                    if bot.drive_tick().await.map(|s| matches!(s, DriveStep::Disconnected)).unwrap_or(true) {
                        bot.clear_control_states();
                        return false;
                    }
                    t += 1;
                }
                bot.clear_control_states();
            }
        } else {
            // PHASE 2 — at the surface (head out, feet still in water). Swim to the nearest
            // bank and climb out. Face the closest air-headed column (progressively wider —
            // a 5-block scan misses a lake edge), committed for several ticks.
            let dir = water_exit_dir(bot, 6)
                .or_else(|| water_exit_dir(bot, 14))
                .or_else(|| water_exit_dir(bot, 24))
                .or_else(|| water_exit_dir(bot, 40));
            let (dx, dz) = dir.unwrap_or((1.0, 0.0));
            let p = bot.entity.position;
            bot.look_at(crate::vec3::vec3(p.x + dx * 4.0, p.y + 0.3, p.z + dz * 4.0));
            bot.set_control_state("jump", true);
            bot.set_control_state("forward", true);
            for _ in 0..8 {
                if bot.drive_tick().await.map(|s| matches!(s, DriveStep::Disconnected)).unwrap_or(true) {
                    bot.clear_control_states();
                    return !head_in_water(bot) && !feet_in_water(bot);
                }
                t += 1;
            }
            bot.clear_control_states();
            // Pressed against a vertical bank without climbing → carve a stair INTO it (dig
            // ahead at head then foot height) so the bot can move up and over. Guarantees an
            // exit where swim-climb stalls, and cheap (a couple of blocks).
            let p = bot.entity.position;
            if p.y > best_y + 0.3 {
                best_y = p.y;
                no_rise = 0;
            } else if {
                no_rise += 1;
                no_rise >= 3
            } {
                let (ax, az) = ((p.x + dx * 0.8).floor() as i32, (p.z + dz * 0.8).floor() as i32);
                let ay = p.y.floor() as i32;
                for dy in [1, 0] {
                    if bot.block_at(ax, ay + dy, az).map(|b| is_standable(&b.name)).unwrap_or(false) {
                        let _ = bot.dig(ax, ay + dy, az).await;
                        t += 4;
                    }
                }
                no_rise = 0;
            }
        }
    }
    bot.clear_control_states();
    !head_in_water(bot) && !feet_in_water(bot)
}

/// Pre-compute a route to within `range` of (x, y, z). True only if the
/// pathfinder can actually reach it — so callers never journey toward something
/// they can't get to.
pub fn can_reach(bot: &Bot, x: i32, y: i32, z: i32, range: f64) -> bool {
    let start = (
        bot.entity.position.x.floor() as i32,
        bot.entity.position.y.floor() as i32,
        bot.entity.position.z.floor() as i32,
    );
    let goal = GoalNear::new(x as f64, y as f64, z as f64, range);
    // BOUNDED: a short timeout + capped search radius. This is a synchronous,
    // CPU-bound A* that blocks the bot's async loop (and thus its keep-alive
    // responses) while it runs — an unbounded 3s version got bots kicked for
    // "Timed out" when many ran at once. Reachable-but-near targets resolve fast.
    let res = get_path_to(
        &bot.world,
        start,
        &goal,
        bot.movement.clone(),
        80.0,
        std::time::Duration::from_millis(800),
    );
    res.status == PathStatus::Success
}

pub const LOG_TYPES: &[&str] = &[
    "oak_log", "birch_log", "spruce_log", "jungle_log", "acacia_log", "dark_oak_log",
    "mangrove_log", "cherry_log", "pale_oak_log",
];

/// Total count of an item by name in the inventory.
pub fn count_items(bot: &Bot, name: &str) -> i32 {
    bot.inventory.slots.iter().flatten().filter(|i| i.name == name).map(|i| i.count).sum()
}

/// Total inventory item count (any item).
pub fn total_count(bot: &Bot) -> i32 {
    bot.inventory.slots.iter().flatten().map(|i| i.count).sum()
}

/// Worthless clutter a speedrun bot picks up while mining/chopping. Hoarding these
/// (plus over-mined cobble) fills the inventory, and a near-full inventory makes
/// CRAFTS silently fail — the result slot has nowhere to go. Dropping them keeps
/// crafting reliable. NOT junk: cobblestone (capped, used for tools+furnace), planks,
/// sticks, coal, iron, raw_iron, gravel+flint (flint&steel), tools, buckets, table.
const JUNK_ITEMS: &[&str] = &[
    "dirt", "coarse_dirt", "rooted_dirt", "mud", "clay_ball",
    "granite", "diorite", "andesite", "tuff", "calcite", "deepslate", "cobbled_deepslate",
    "leaf_litter", "oak_sapling", "birch_sapling", "spruce_sapling",
    "oak_leaves", "birch_leaves", "spruce_leaves", "azalea_leaves",
    "short_grass", "tall_grass", "fern", "dead_bush", "seeds", "wheat_seeds",
    "oak_button", "birch_button", "spruce_button", "stone_button",
    "raw_copper", "copper_ore",
];

/// Drop junk + excess cobblestone so the inventory keeps room for craft results.
/// Whole-stack drops (button 1 + mode 4). Keeps ≤64 cobblestone and ≤16 sticks.
pub async fn tidy_inventory(bot: &mut Bot<'_>) {
    let mut drop_slots: Vec<i32> = Vec::new();
    let mut cobble = 0i32;
    let mut sticks = 0i32;
    for (i, slot) in bot.inventory.slots.iter().enumerate() {
        let Some(it) = slot else { continue };
        let name = it.name.as_str();
        if name == "cobblestone" {
            cobble += it.count;
            // Generous cap — keep well more than enough for furnace (8) + tools, so a
            // craft is never starved of cobble; only dump genuine hoarding.
            if cobble > 128 {
                drop_slots.push(i as i32);
            }
        } else if name == "stick" {
            sticks += it.count;
            if sticks > 16 {
                drop_slots.push(i as i32);
            }
        } else if JUNK_ITEMS.contains(&name) {
            drop_slots.push(i as i32);
        }
    }
    for s in drop_slots {
        // Drop the WHOLE stack. Button-1 (drop-stack) mode-4 was NOT clearing the slot
        // in practice — junk like raw_copper/cobbled_deepslate persisted across every
        // craft (a lead race bot sat on 24 copper + 47 deepslate and its planks/table/
        // furnace crafts silently no-op'd from the bloated window, stalling the whole
        // run at Craft Furnace). Drop item-by-item with button 0 — the same primitive
        // `Bot::toss` uses successfully — re-reading the slot until it's empty.
        for _ in 0..64 {
            let occupied = bot
                .inventory
                .slots
                .get(s as usize)
                .and_then(|x| x.as_ref())
                .map(|it| it.count > 0)
                .unwrap_or(false);
            if !occupied {
                break;
            }
            let _ = bot.click_window(s, 0, 4).await; // button 0 + mode 4 = drop one
            bot.wait_ticks(1).await.ok();
        }
    }
}

/// Walk to the nearest dropped item entity (falling back to the dug block) to
/// pick it up. Stops early once total inventory count grows. ~2s budget.
pub async fn collect_drops(bot: &mut Bot<'_>, fx: i32, fz: i32) {
    let item_type = bot.registry.entities_by_name.get("item").map(|d| d.id);
    let before = total_count(bot);
    for _ in 0..40 {
        let bp = bot.entity.position;
        let mut target = crate::vec3::vec3(fx as f64 + 0.5, bp.y, fz as f64 + 0.5);
        let mut best = f64::MAX;
        for e in bot.entities.values() {
            if item_type.is_some() && e.entity_type != item_type {
                continue;
            }
            let d = e.position.distance(bp);
            if d < best && d < 10.0 {
                best = d;
                target = e.position;
            }
        }
        // Walk toward the item on XZ but also look DOWN toward it (items dropped
        // below the bot — e.g. stone mined underground — need the bot to descend).
        bot.look_at(crate::vec3::vec3(target.x, target.y - 0.5, target.z));
        bot.set_control_state("forward", true);
        // If the item is below us, dig down to reach it.
        if target.y < bp.y - 1.0 {
            let f = bp.y.floor() as i32;
            let cell = ((bp.x + 0.3).floor() as i32, (bp.z + 0.3).floor() as i32);
            for (dx, dz) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let (cx, cz) = (cell.0 + dx, cell.1 + dz);
                let n = bot.block_at(cx, f - 1, cz).map(|b| b.name.clone()).unwrap_or_default();
                if !n.is_empty() && n != "air" && !n.contains("water") && !n.contains("lava") {
                    bot.dig(cx, f - 1, cz).await.ok();
                    break;
                }
            }
        }
        match bot.drive_tick().await {
            Ok(crate::bot::DriveStep::Disconnected) | Err(_) => break,
            _ => {}
        }
        if total_count(bot) > before {
            break;
        }
    }
    bot.clear_control_states();
    bot.wait_ticks(4).await.ok();
}

/// Find the nearest log block (any species), searching out in rings.
pub fn find_closest_log(bot: &Bot) -> Option<(String, (i32, i32, i32))> {
    let p = bot.entity.position;
    for radius in [16, 32, 48, 64] {
        let mut best: Option<(String, (i32, i32, i32))> = None;
        let mut best_d = f64::MAX;
        for &log in LOG_TYPES {
            for (x, y, z) in bot.find_blocks(log, radius, 16) {
                let d = (x as f64 - p.x).powi(2) + (z as f64 - p.z).powi(2) + (y as f64 - p.y).powi(2);
                if d < best_d {
                    best_d = d;
                    best = Some((log.to_string(), (x, y, z)));
                }
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

/// Equip `name` to the hand, VALIDATED against the server. The optimistic swap
/// can be silently rejected under load (held hand stays empty), so we do the
/// swap, wait for the server's authoritative inventory update to land, then
/// confirm we actually hold it — re-doing the swap until the SERVER agrees.
/// Returns false only if the item isn't in the inventory at all.
pub async fn select_item(bot: &mut Bot<'_>, name: &str) -> std::io::Result<bool> {
    for _ in 0..5 {
        // Server-confirmed already holding it? Done.
        if bot.held_item().map(|i| i.name == name).unwrap_or(false) {
            return Ok(true);
        }
        let Some(idx) =
            bot.inventory.slots.iter().position(|s| s.as_ref().map(|i| i.name.as_str()) == Some(name))
        else {
            return Ok(false); // not in inventory — honest false, no item to equip
        };
        if (36..45).contains(&idx) {
            bot.set_held_slot((idx - 36) as i32).await?;
        } else {
            bot.click_window(idx as i32, 0, 2).await?; // atomic hotbar swap (mode 2)
            bot.set_held_slot(0).await?;
        }
        // Let the SERVER's authoritative inventory arrive (a rejected swap is
        // reverted here), then the loop re-checks the real held item.
        bot.wait_ticks(8).await.ok();
    }
    Ok(bot.held_item().map(|i| i.name == name).unwrap_or(false))
}

/// Craft `count` of an item by name. `table` is the position of an open-able
/// crafting table for 3x3 recipes (None → 2x2 inventory grid).
pub async fn craft_item(
    bot: &mut Bot<'_>,
    name: &str,
    count: i32,
    table: Option<(i32, i32, i32)>,
    mem: &mut WorldMemory,
) -> StepResult {
    let Some(def) = bot.registry.items_by_name.get(name) else {
        return failure(format!("unknown item {name}"));
    };
    let id = def.id;
    let recipes = bot.recipes_for(id, Some(1), table.is_some());
    let Some(recipe) = recipes.into_iter().next() else {
        return failure(format!("no recipe for {name}"));
    };
    if recipe.requires_table && table.is_none() {
        return failure(format!("{name} needs a crafting table"));
    }

    if let Some((tx, ty, tz)) = table {
        let _ = bot.goto(tx, ty, tz).await;
    }

    // Drop accumulated junk (dirt/granite/raw_copper/leaf_litter…) + excess cobble
    // before EVERY craft. A cluttered, fragmented inventory bloats the window state the
    // server has to keep in sync, and under load that desync is what makes the craft
    // result silently never appear. Keeping the inventory lean keeps crafting reliable.
    // (Cheap no-op when there's nothing to drop.)
    tidy_inventory(bot).await;

    let before = bot.item_count(name);
    let made = recipe.result.count.max(1) * count.max(1);
    // The container-craft is dropped ~half the time under server load. DON'T
    // fabricate the result — instead retry the craft IN PLACE (re-open the table
    // each attempt) and only count it done when the item REALLY appears in the
    // server-synced inventory. Fail-fast confirmation per attempt so a dropped
    // craft retries in ~1.5s instead of waiting on a slow step-machine re-run.
    let mut result: std::io::Result<()> = Ok(());
    for _ in 0..3 {
        if bot.item_count(name) >= before + made {
            break; // appeared (possibly late from a previous attempt) — don't re-craft
        }
        if table.is_some() && bot.current_window.is_none() {
            if let Some((tx, ty, tz)) = table {
                // Walk closer and re-approach: open_block fails silently when the
                // bot is too far or not facing the table (the #1 cause of the
                // "missing crafting ingredient" loop — the 3×3 recipe runs against
                // the 2×2 inventory grid). Retry the approach a few times.
                let _ = bot.goto_near(tx, ty, tz, 2.0).await;
                let mut opened = false;
                for _ in 0..3 {
                    if bot.open_block(tx, ty, tz, Face::Top).await.unwrap_or(false) {
                        opened = true;
                        break;
                    }
                    // Re-navigate: the bot may have drifted or the path dropped
                    // it a block away from the table.
                    let _ = bot.goto_near(tx, ty, tz, 1.5).await;
                    bot.wait_ticks(5).await.ok();
                }
                if !opened {
                    // The table won't open (commonly on a ledge/over a hole the bot
                    // mined itself — the block still EXISTS so get_crafting_table happily
                    // hands it back, and the bot loops "would not open" forever, ~1 bot
                    // lost per race). FORGET it so get_crafting_table places a fresh,
                    // reachable one next tick.
                    mem.mark((tx, ty, tz), PoiStatus::Gone);
                    result = Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "crafting table would not open",
                    ));
                    break;
                }
            }
        }
        result = bot.craft(&recipe, count, table.is_some()).await;
        if table.is_some() {
            let _ = bot.close_window().await;
        }
        if result.is_err() {
            break;
        }
        for _ in 0..8 {
            if bot.item_count(name) >= before + made {
                break;
            }
            bot.wait_ticks(4).await.ok();
        }
    }
    if std::env::var("CRAFT_DEBUG").is_ok() {
        let inv: Vec<String> = bot.inventory.slots.iter().flatten().filter(|i| i.count > 0).map(|i| format!("{}x{}", i.count, i.name)).collect();
        let win = bot.current_window.as_ref().map(|w| w.slots.iter().flatten().filter(|i| i.count > 0).map(|i| format!("{}x{}", i.count, i.name)).collect::<Vec<_>>());
        eprintln!("CRAFT {name}: result={result:?} inv={inv:?} win={win:?}");
    }
    match result {
        // Only call it a success if the item ACTUALLY appeared in our (server-
        // synced) inventory — never trust the click going through alone.
        Ok(()) if bot.item_count(name) > before => {
            success(format!("crafted {name} (have {})", bot.item_count(name)))
        }
        Ok(()) => failure(format!("craft {name}: result never appeared (server didn't make it)")),
        Err(e) => failure(format!("craft {name}: {e}")),
    }
}

/// Find an existing crafting table nearby, or place one from inventory.
/// Returns its position (the bot will be near it).
pub async fn get_crafting_table(
    bot: &mut Bot<'_>,
    mem: &mut WorldMemory,
) -> std::io::Result<Option<(i32, i32, i32)>> {
    let bpos = {
        let p = bot.entity.position;
        (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
    };
    // 1. A table we remember placing/seeing — walk back and reuse it instead of
    //    wasting a plank crafting + placing a fresh one.
    let remembered = mem.nearest(&[PoiKind::CraftingTable], bpos, 99).map(|p| p.pos);
    if let Some(tpos) = remembered {
        let _ = bot.goto(tpos.0, tpos.1, tpos.2).await;
        if bot.block_at(tpos.0, tpos.1, tpos.2).map(|b| b.name == "crafting_table").unwrap_or(false) {
            println!("    table: reusing remembered table at {tpos:?}");
            return Ok(Some(tpos));
        }
        // Loaded but no longer a table → it's gone; forget it. (Unloaded/unreachable
        // → leave it remembered and just place a new one for now.)
        if bot.block_at(tpos.0, tpos.1, tpos.2).is_some() {
            mem.mark(tpos, PoiStatus::Gone);
        }
    }
    // 2. An existing table in the loaded world — record it for next time.
    if let Some(pos) = bot.find_block("crafting_table", 24) {
        bot.goto(pos.0, pos.1, pos.2).await?;
        mem.record(PoiKind::CraftingTable, pos, PoiStatus::Available);
        return Ok(Some(pos));
    }
    // 3. None around — craft one on demand if we don't have the item, then place it.
    if count_items(bot, "crafting_table") == 0 {
        // A table costs 4 planks. A bot deep in the iron phase often has 0 planks but
        // still holds LOGS (it spent its planks on sticks/pickaxes) — without converting
        // them it can't craft a table and loops "need a crafting table" forever while
        // sitting on wood (whole race stalls here). Convert a log to planks first.
        let planks: i32 = bot
            .inventory
            .slots
            .iter()
            .flatten()
            .filter(|i| i.name.ends_with("_planks"))
            .map(|i| i.count)
            .sum();
        if planks < 4 {
            // No wood AT ALL — the bot spent every log and plank and is now stranded
            // needing a table it can't build (rust-race-002 looped "need a crafting table"
            // 1884x on zero wood and lost the whole race). Go cut a couple of logs first,
            // THEN convert to planks. This is the wood-reserve recovery: any table-craft
            // can top up wood instead of dead-ending.
            if !LOG_TYPES.iter().any(|l| count_items(bot, l) > 0) {
                println!("    table: out of wood — gathering a little before crafting a table");
                let _ = crate::tasks::gather_wood::gather_wood(bot, 2, mem).await;
            }
            for log in LOG_TYPES {
                if count_items(bot, log) > 0 {
                    let plank_name = log.replace("_log", "_planks");
                    let _ = craft_item(bot, &plank_name, 4, None, mem).await;
                    break;
                }
            }
        }
        let _ = craft_item(bot, "crafting_table", 1, None, mem).await;
        if count_items(bot, "crafting_table") == 0 {
            println!("    table: could not craft a crafting_table");
            return Ok(None);
        }
    }
    place_crafting_table(bot, mem).await
}

/// Place a crafting table at (tx,ty,tz) (against the top of the block below) and
/// VALIDATE it against the server: equip is confirmed first, then after placing
/// we wait for the server's block update and check the block is REALLY there —
/// the optimistic placement is reverted by the server on rejection, so this only
/// returns true when the table genuinely exists server-side.
async fn place_table_confirmed(bot: &mut Bot<'_>, tx: i32, ty: i32, tz: i32) -> std::io::Result<bool> {
    if !select_item(bot, "crafting_table").await? {
        return Ok(false); // couldn't confirm the item in hand — don't bother placing
    }
    bot.look_at(crate::vec3::vec3(tx as f64 + 0.5, ty as f64 - 0.5, tz as f64 + 0.5));
    bot.wait_ticks(2).await?;
    bot.place_block(tx, ty - 1, tz, Face::Top).await?;
    // Give the server time to confirm OR revert the optimistic placement (~1s) —
    // too short and we'd either trust a placement the server is about to revert,
    // or false-negative a real one and place a duplicate.
    bot.wait_ticks(20).await?;
    Ok(bot.block_at(tx, ty, tz).map(|b| b.name == "crafting_table").unwrap_or(false))
}

/// Place a crafting table on an air block next to the bot with solid ground.
/// Only returns a position once the server CONFIRMS the table is really there.
async fn place_crafting_table(
    bot: &mut Bot<'_>,
    mem: &mut WorldMemory,
) -> std::io::Result<Option<(i32, i32, i32)>> {
    if !select_item(bot, "crafting_table").await? {
        println!("    table: could not equip crafting_table (server didn't confirm it in hand)");
        return Ok(None);
    }
    let p = bot.entity.position;
    let (fx, fy, fz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    // Try feet-level and one-below neighbours: need an empty cell with a solid
    // block beneath it to place the table on.
    for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (1, -1, 0), (-1, -1, 0), (0, -1, 1), (0, -1, -1)] {
        let (tx, ty, tz) = (fx + dx, fy + dy, fz + dz);
        if bot.block_state_at(tx, ty, tz) == 0 && bot.block_state_at(tx, ty - 1, tz) != 0 {
            if place_table_confirmed(bot, tx, ty, tz).await? {
                println!("    table: placed + server-confirmed at ({tx},{ty},{tz})");
                mem.record(PoiKind::CraftingTable, (tx, ty, tz), PoiStatus::Available);
                return Ok(Some((tx, ty, tz)));
            }
            println!("    table: placement at ({tx},{ty},{tz}) NOT confirmed — trying elsewhere");
        }
    }
    // No open spot (e.g. mining in a tunnel) — DIG a side niche at feet level so a
    // cell opens up with solid ground below it, then place the table there.
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let (tx, ty, tz) = (fx + dx, fy, fz + dz);
        if bot.block_state_at(tx, ty - 1, tz) != 0 {
            if bot.block_state_at(tx, ty, tz) != 0 && bot.dig(tx, ty, tz).await.is_err() {
                continue;
            }
            if bot.block_state_at(tx, ty, tz) != 0 {
                continue; // couldn't break it (e.g. bedrock)
            }
            if place_table_confirmed(bot, tx, ty, tz).await? {
                println!("    table: dug a niche + confirmed at ({tx},{ty},{tz})");
                mem.record(PoiKind::CraftingTable, (tx, ty, tz), PoiStatus::Available);
                return Ok(Some((tx, ty, tz)));
            }
        }
    }
    println!("    table: could not place a server-confirmed table (bot at {fx},{fy},{fz})");
    Ok(None)
}
