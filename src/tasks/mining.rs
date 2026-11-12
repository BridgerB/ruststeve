//! Mining — gather a target count of a block, digging down to reach it when
//! there's none in reach. Port of the core of steve's `tasks/mining` (surface +
//! dig-down; deep-ore strip mining comes later).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::bot::Bot;

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
    if std::env::var("MINE_DEBUG").is_ok() && best.is_none() {
        // Report what blocks ARE below the bot (up to 6 down) so we know why
        // stone isn't found — chunks may not be loaded at depth.
        let mut below: Vec<String> = Vec::new();
        for dy in 0..6 {
            let n = bot.block_at(bx, by - dy, bz).map(|b| b.name).unwrap_or_else(|| "?".into());
            below.push(format!("y={}={}", by - dy, n));
        }
        eprintln!("MINE find_stone: NONE at ({bx},{by},{bz}) below=[{}]", below.join(", "));
    }
    best
}

/// Mine the block under the bot's feet so it descends one level. Returns false
/// if it can't (lava below, or the dig failed).
pub(crate) async fn dig_down(bot: &mut Bot<'_>) -> bool {
    let p = bot.entity.position;
    let x = p.x.floor() as i32;
    let z = p.z.floor() as i32;
    // The block the bot is STANDING ON. Using floor(y)-1 is wrong when physics
    // jitter dips position.y just under the integer (e.g. 53.92 → floor-1 digs the
    // block one too low, leaving the real support intact). floor(y-0.5) is robust.
    let y = (p.y - 0.5).floor() as i32;
    // Support cells: the bot's 0.6-wide bounding box can STRADDLE up to 4 cells. Digging
    // only floor(x),floor(z) leaves an adjacent sub-cell holding the bot up → it never
    // falls and the caller loops "y->y via dig_down" forever (this capped every descent).
    // Dig every cell under the footprint so support is actually removed.
    let mut cells: Vec<(i32, i32)> = Vec::new();
    for dx in [-0.3, 0.3] {
        for dz in [-0.3, 0.3] {
            let c = ((p.x + dx).floor() as i32, (p.z + dz).floor() as i32);
            if !cells.contains(&c) {
                cells.push(c);
            }
        }
    }
    // Death-AVOIDANCE: never break a support cell that IS liquid or sits directly above
    // liquid (flooding into the shaft / dropping into lava). For a liquid BESIDE the cell,
    // only LAVA disqualifies — water beside is harmless (the bot punches through water and
    // a wet shaft is survivable), and refusing on water-beside WEDGED the descent in wet
    // biomes (217 relocates on surface grass with water beside every cell, never digging).
    for &(cx, cz) in &cells {
        let lava_beside = [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .iter()
            .any(|&(dx, dz)| is_lava_at(bot, cx + dx, y, cz + dz));
        if is_liquid_at(bot, cx, y, cz) || is_liquid_at(bot, cx, y - 1, cz) || lava_beside {
            return false;
        }
        // NEVER dig obsidian — it's the nether-portal frame. descend_to_y (used by the
        // portal to drop to base + open the interior after casting) was digging THROUGH a
        // frame block, dropping the count from 10/10 to <10 → 'only 9/10 present' → the whole
        // cast restarted forever. Refuse; the caller's descend_step steps aside instead.
        if bot.block_at(cx, y, cz).map(|b| b.name == "obsidian").unwrap_or(false) {
            return false;
        }
    }
    // Fall-avoidance: don't dig the floor out over a deep drop (open cavern / ravine) — a
    // 4+ block fall hurts. The controlled descend_step / drop_into_cavern handle those.
    if bot.block_state_at(x, y - 1, z) == 0
        && bot.block_state_at(x, y - 2, z) == 0
        && bot.block_state_at(x, y - 3, z) == 0
    {
        return false;
    }
    if y <= bot.game.min_y + 4 {
        return false;
    }
    let y_before = bot.entity.position.y;
    let mut dug_any = false;
    for &(cx, cz) in &cells {
        if bot.block_state_at(cx, y, cz) != 0 {
            if bot.dig(cx, y, cz).await.is_err() {
                continue;
            }
            dug_any = true;
        }
    }
    let _ = dug_any;
    bot.wait_ticks(8).await.ok(); // fall into the hole
    // Only report success if we ACTUALLY descended. Digging stone with no (or the wrong)
    // tool doesn't break the block, so the bot never falls — returning true there spins.
    bot.entity.position.y < y_before - 0.5
}

/// Is there liquid (water or lava) at (x, y, z)?
fn is_liquid_at(bot: &Bot, x: i32, y: i32, z: i32) -> bool {
    bot.block_at(x, y, z)
        .map(|b| b.name.contains("water") || b.name.contains("lava"))
        .unwrap_or(false)
}

fn is_water_at(bot: &Bot, x: i32, y: i32, z: i32) -> bool {
    bot.block_at(x, y, z).map(|b| b.name.contains("water")).unwrap_or(false)
}

/// The bot is standing in a real water BODY (a >=3-deep water column at its feet), not a
/// thin aquifer it could just punch through. Descending here only fights the survival
/// surfacing reflex forever — the miner should leave the water and mine on land instead.
fn over_deep_water(bot: &Bot, feet: (i32, i32, i32)) -> bool {
    let (x, y, z) = feet;
    (is_water_at(bot, x, y, z) || is_water_at(bot, x, y - 1, z)) && is_water_at(bot, x, y - 2, z)
}

/// Is there lava at (x, y, z)? Used for the descent's beside-check — only lava beside a
/// support cell disqualifies digging it (water beside is fine).
fn is_lava_at(bot: &Bot, x: i32, y: i32, z: i32) -> bool {
    bot.block_at(x, y, z).map(|b| b.name.contains("lava")).unwrap_or(false)
}

/// Holding a pickaxe right now?
fn held_is_pickaxe(bot: &Bot) -> bool {
    bot.held_item().map(|i| i.name.ends_with("_pickaxe")).unwrap_or(false)
}

/// Make sure a pickaxe is in hand; re-equips the best one if the current broke
/// (durability management). False if the bot has no pickaxe at all — the caller
/// should bail so the step machine crafts a replacement instead of mining
/// bare-handed (which on stone yields nothing).


pub(crate) async fn ensure_pickaxe(bot: &mut Bot<'_>) -> bool {
    if held_is_pickaxe(bot) {
        return true;
    }
    for tier in ["diamond_pickaxe", "iron_pickaxe", "stone_pickaxe", "wooden_pickaxe"] {
        if select_item(bot, tier).await.unwrap_or(false) {
            return true;
        }
    }
    // Loud, rate-limited: a natural portal run needs ~500 digs (551 in one rust-gym-001 trial) —
    // two iron pickaxes — and without one every stone dig is ~10 s by hand (and deepslate fails).
    static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    let mut last = LAST.lock().unwrap();
    if last.is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(60)) {
        *last = Some(std::time::Instant::now());
        crate::tasks::portal::cast_debug("NO PICKAXE in inventory — digging by hand");
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
    // Walk into reach (the pathfinder digs a horizontal/diagonal path to it). Fail-fast
    // (8s, not the default 18s): a bot at ore depth often has MANY exposed-but-unreachable
    // iron POIs in memory, and 18s each cycled the whole 250s budget mining nothing. If
    // the walk falls short, dig_toward below still carves toward the ore from here.
    let _ = bot.goto_near_timeout(tx, ty, tz, 1.8, Duration::from_secs(8)).await;
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
pub(crate) async fn descend_step(bot: &mut Bot<'_>, dx: i32, dz: i32) -> bool {
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
        bot.look_at(crate::vec3::vec3(nx as f64 + 0.5, (y - 1) as f64, nz as f64 + 0.5));
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
pub(crate) async fn strip_tunnel(bot: &mut Bot<'_>, dx: i32, dz: i32) -> bool {
    let p = bot.entity.position;
    let cy = p.y.floor() as i32;
    let (tx, tz) = (p.x.floor() as i32 + dx * 6, p.z.floor() as i32 + dz * 6);
    // Target the CURRENT y (a LEVEL corridor), not a free-y goto — goto_xz dives into any cave on
    // the way and SINKS the bot below the ore band (the iron search fell to y-9..-34 in the
    // iron-empty deepslate and thrashed there, climbing out and re-sinking dozens of times). A
    // y-pinned goto keeps the tunnel horizontal so it exposes ore in the walls without descending.
    bot.goto_near(tx, cy, tz, 1.0).await.unwrap_or(false)
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

/// One mining action. Chosen purely from a snapshot of state by `decide_mine_move`;
/// the loop below only senses (fills a `MineObs`) and acts (performs the move). All
/// the "where should I be, what should I do next" logic lives in that one pure
/// function instead of being smeared across interleaved branches + IO — so it reads
/// top-to-bottom as a priority ladder and can be reasoned about (and unit-tested)
/// without a live bot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MineMove {
    /// Iron, but we've sunk into the ore-empty deepslate void (y<0) — climb back up.
    ClimbFromVoid,
    /// Wedged (no gain / no movement for a while) — relocate to fresh ground, or bail.
    Relocate,
    /// The memory index knows a reachable vein — go dig it out.
    MineVein((i32, i32, i32)),
    /// Above the productive band — descend toward ore depth.
    Descend,
    /// At depth but this box is mined out — jump far to fresh terrain.
    RangeJump,
    /// At depth with ore still to find — strip-tunnel one leg to expose new stone.
    StripTunnel((i32, i32)),
}

/// A plain snapshot of everything the decision needs — no bot, no IO, no `await`.
struct MineObs {
    is_iron: bool,
    y: i32,
    depth: i32,
    stuck: u32,
    desc_fail: u32,
    dry_veins: u32,
    gain_iters: u32,
    nearest_ore: Option<(i32, i32, i32)>,
    strip_dir: (i32, i32),
    /// Standing in a real water body — descending here is futile (survival just surfaces us).
    over_water: bool,
}

/// The miner's brain: the whole priority ladder as one pure function of the snapshot.
/// Given "this is my state", it yields "this is my next move" — deterministically,
/// with no side effects and no dependence on wall-clock time.
fn decide_mine_move(o: &MineObs) -> MineMove {
    // 1. Fell through the iron band into the void — there is nothing to mine below y0.
    if o.is_iron && o.y < 0 {
        return MineMove::ClimbFromVoid;
    }
    // 2. Wedged in place — can't tunnel or descend from here.
    if o.stuck > 30 {
        return MineMove::Relocate;
    }
    // 3. We remember a reachable vein and haven't struck out chasing sightings too many
    //    times in a row — go mine it. (Past a few dry veins, fall through to change depth.)
    if o.dry_veins < 4 {
        if let Some(p) = o.nearest_ore {
            return MineMove::MineVein(p);
        }
    }
    // 3b. Standing in a water body — do NOT descend into it (that just oscillates against the
    //     survival surfacing reflex; a race bot burned ~2h + 238 surfacings this way). Range-
    //     relocate to dry land and mine there instead.
    if o.over_water {
        return MineMove::RangeJump;
    }
    // 4. Above the productive band (and either descending is still working, or we're high
    //    enough that pushing down beats searching here) — descend toward ore depth.
    if o.y > o.depth + 2 && (o.desc_fail < 5 || o.y > 55) {
        return MineMove::Descend;
    }
    // 5. At depth and nothing new for a while — this box is mined out; jump far.
    if o.gain_iters > 24 {
        return MineMove::RangeJump;
    }
    // 6. At depth with ore left to find — expose fresh stone.
    MineMove::StripTunnel(o.strip_dir)
}

/// Mine `target` of an ore resource. The bot's memory is the index: it OBSERVES
/// ores it sees into SQLite, then QUERIES the DB for the nearest usable one and
/// goes mines it. Only when the DB has nothing does it explore — searching SOUTH
/// (descending to ore depth, then tunnelling +Z to load fresh terrain) in a loop
/// until ore turns up, recording everything it finds along the way.
///
/// The loop is `sense → decide (pure) → act`: it snapshots state into a `MineObs`,
/// asks `decide_mine_move` for the one derived move, and performs it. No branch
/// picks the move; the pure function does.
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
    // Productive depths: iron's underground triangle peaks at y16 (dense y0..y32),
    // so y35 sits on the sparse upper slope — race bots stalled there with 0 iron.
    // Drop to y15 (the peak, still fast-mining stone above the deepslate transition)
    // where iron is abundant. Coal is everywhere — a moderate depth is fine.
    let depth = if ore == "iron" { 15 } else { 50 };

    let deadline = Instant::now() + Duration::from_secs(300);
    let mut iters = 0u32;
    let mut stuck = 0u32;
    let mut desc_fail = 0u32; // consecutive failures to descend (drives escalating relocation)
    // Consecutive mine_vein attempts that gained nothing (unreachable ore). At a shallow
    // depth the bot can SEE many exposed irons through walls it can't reach; cycling them
    // (goto drifts its position, so stuck>30 never fires) burned the whole budget after a
    // couple of easy irons. After a few dry veins, stop chasing sightings and DESCEND to
    // denser/deeper ore instead.
    let mut dry_veins = 0u32;
    // Consecutive "wedged" relocations (can't tunnel/descend from the current spot). We
    // relocate to fresh ground instead of bailing the whole step on the first wedge —
    // only give up after several relocations gain nothing.
    let mut wedge_relocates = 0u32;
    // Iterations since the ore count last INCREASED, and how many long-range jumps we've made.
    // The at-depth box-spiral strip search (S→E→N→W, 4 legs) has ~zero NET displacement — it
    // re-explores the same ~24-block box and, once the local ore is mined out, tunnels forever
    // finding nothing (race bots stalled 30+ min at 9/12 iron, oscillating y11↔y-45). When no
    // new ore turns up for a while, JUMP far to fresh terrain instead of re-combing the box.
    let mut gain_iters = 0u32;
    let mut range_relocates = 0u32;
    let mut overshoots = 0u32;
    let mut best_count = count_ore_resource(bot, ore);
    let mut last_count = count_ore_resource(bot, ore);
    let mut last_pos = {
        let p = bot.entity.position;
        (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
    };
    // Rotating headings (S→E→N→W) reused by both the descent's dry-ground sweep and the
    // at-depth strip search.
    const DIRS: [(i32, i32); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
    while count_ore_resource(bot, ore) < target && Instant::now() < deadline && !bot.breath_alarm {
        iters += 1;

        // ---- SENSE: fold this tick's world into the running counters ----
        let now_count = count_ore_resource(bot, ore);
        let now_pos = {
            let p = bot.entity.position;
            (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
        };
        // "stuck" = no ore gained AND no movement since last tick; "gain_iters" = ticks
        // since the ore count last rose (the box is mined out once this runs high).
        if now_count > last_count || now_pos != last_pos {
            stuck = 0;
        } else {
            stuck += 1;
        }
        last_count = now_count;
        last_pos = now_pos;
        if now_count > best_count {
            best_count = now_count;
            gain_iters = 0;
        } else {
            gain_iters += 1;
        }
        // Durability: re-equip a pickaxe if the held one broke; bail to re-craft if none.
        if !ensure_pickaxe(bot).await {
            mem.log("mine_ore", "no_pickaxe", "bailing to re-craft");
            break;
        }
        // Notice ores + exposed lava around us (cheap, throttled) into memory — the index
        // decide reads below. Recording lava now lets the later portal cast navigate
        // straight to a pool instead of blind-searching.
        if iters % 4 == 1 {
            observe_blocks(bot, mem, &ores);
            for (lx, ly, lz) in bot.find_exposed_blocks("lava", 12, 4) {
                mem.record(PoiKind::Lava, (lx, ly, lz), PoiStatus::Available);
            }
        }

        // ---- DECIDE: one pure move from the snapshot ----
        let from = now_pos;
        let tier = pickaxe_tier_rank(bot);
        // Consult the ore index only while we haven't struck out chasing sightings — a
        // `None` routes decide to Descend/StripTunnel instead of grinding the same cluster.
        // (No can-reach pre-check: mine_vein DIGS through stone to embedded ore, and a
        // walk-only test wrongly rejected 14 perfectly-mineable irons.)
        let nearest_ore = if dry_veins < 4 {
            mem.nearest(&[kind], from, tier).map(|p| p.pos)
        } else {
            None
        };
        let obs = MineObs {
            is_iron: ore == "iron",
            y: from.1,
            depth,
            stuck,
            desc_fail,
            dry_veins,
            gain_iters,
            nearest_ore,
            strip_dir: DIRS[((iters / 4) as usize + crate::learn::alternatives("mine_iron") as usize) % 4], // + stall alternative (learn.rs)
            over_water: over_deep_water(bot, from),
        };

        // ---- ACT: the only place with side effects ----
        match decide_mine_move(&obs) {
            MineMove::ClimbFromVoid => {
                overshoots += 1;
                mem.log("mine_ore", "overshoot", &format!("y={} — climbing out of the iron-empty deepslate void", from.1));
                println!("    ore: overshot to y={} (no iron in the deepslate void) — climbing back to the iron band", from.1);
                crate::tasks::portal::climb_out_of_pit(bot).await;
                // Repeated overshoots mean this column funnels through the band into the void
                // — re-descending the same spot just re-overshoots. Every few, jump away so
                // the next descent is a NEW column.
                if overshoots % 3 == 0 {
                    range_relocates += 1;
                    let (rdx, rdz) = DIRS[(range_relocates as usize + crate::learn::alternatives("mine_iron") as usize) % 4];
                    let dist = 48 + range_relocates.min(6) as i32 * 24;
                    let (tx, tz) = (from.0 + rdx * dist, from.2 + rdz * dist);
                    println!("    ore: repeated overshoot — ranging to fresh terrain ({tx},{tz})");
                    bot.movement.blocks_cant_break.clear();
                    let _ = bot.goto_xz(tx, tz, 4.0).await;
                }
                desc_fail = 0;
                dry_veins = 0;
            }
            MineMove::Relocate => {
                wedge_relocates += 1;
                if wedge_relocates > 6 {
                    mem.log("mine_ore", "stuck", &format!("y={} have={} bailed after {wedge_relocates}", from.1, now_count));
                    println!("    ore: stuck — bailing after {wedge_relocates} relocates");
                    break;
                }
                let (dx, dz) = DIRS[(wedge_relocates as usize) % 4];
                let dist = 16 + wedge_relocates as i32 * 8;
                let (tx, tz) = (from.0 + dx * dist, from.2 + dz * dist);
                mem.log("mine_ore", "wedged", &format!("y={} relocate#{wedge_relocates} to {tx},{tz}", from.1));
                println!("    ore: wedged — relocating #{wedge_relocates} to {tx},{tz}");
                bot.movement.blocks_cant_break.clear();
                let _ = bot.goto_xz(tx, tz, 3.0).await;
                stuck = 0;
            }
            MineMove::MineVein(tpos) => {
                mem.log("mine_ore", "target", &format!("{ore} {tpos:?}"));
                let gained = mine_vein(bot, ore, tpos.0, tpos.1, tpos.2).await;
                observe_blocks(bot, mem, &ores); // mined blocks are air now
                if gained > 0 {
                    mem.mark(tpos, PoiStatus::Gone);
                    dry_veins = 0;
                    println!("    ore: {} {ore} (y={})", count_ore_resource(bot, ore), bot.entity.position.y as i32);
                    mem.log("mine_ore", "mined", &format!("+{gained} {ore} total={}", count_ore_resource(bot, ore)));
                } else {
                    mem.mark(tpos, PoiStatus::Unreachable);
                    dry_veins += 1;
                }
            }
            MineMove::Descend => {
                // dig_down (straight) first; if it refuses (liquid/fall-avoidance), stair-step
                // in each compass direction — a watery lane blocks only some. Then try punching
                // through water / dropping into a cave. If all refuse, relocate to dry ground,
                // walking further the longer we've been stuck (a short hop won't clear a pond).
                let mut descended = dig_down(bot).await;
                if !descended {
                    for &(dx, dz) in &DIRS {
                        if descend_step(bot, dx, dz).await {
                            descended = true;
                            break;
                        }
                    }
                }
                if !descended {
                    descended = crate::tasks::portal::punch_through_water(bot).await
                        || crate::tasks::portal::drop_into_cavern(bot).await;
                }
                if descended {
                    desc_fail = 0;
                    dry_veins = 0; // new depth — allow chasing sightings again
                } else {
                    desc_fail += 1;
                    let (dx, dz) = DIRS[(desc_fail as usize / 2) % 4];
                    let dist = 6 + (desc_fail.min(8) as i32) * 4; // 6 → up to ~38 blocks
                    let (tx, tz) = (from.0 + dx * dist, from.2 + dz * dist);
                    let _ = bot.goto_xz(tx, tz, 2.0).await;
                }
                if iters % 8 == 0 {
                    println!("    ore: descending toward {ore} — y={} (relocate#{desc_fail})", bot.entity.position.y as i32);
                }
            }
            MineMove::RangeJump => {
                range_relocates += 1;
                let (rdx, rdz) = DIRS[(range_relocates as usize + crate::learn::alternatives("mine_iron") as usize) % 4];
                let dist = 48 + range_relocates.min(6) as i32 * 24; // 48 → up to ~192 blocks
                let (tx, tz) = (from.0 + rdx * dist, from.2 + rdz * dist);
                mem.log("mine_ore", "range", &format!("no gain {gain_iters} iters — jump#{range_relocates} to {tx},{tz}"));
                println!("    ore: local {ore} mined out — ranging to fresh terrain #{range_relocates} ({tx},{tz})");
                bot.movement.blocks_cant_break.clear();
                let _ = bot.goto_xz(tx, tz, 4.0).await;
                gain_iters = 0;
            }
            MineMove::StripTunnel((dx, dz)) => {
                let moved = strip_tunnel(bot, dx, dz).await;
                mem.log("mine_ore", "search", &format!("y={} dir=({dx},{dz}) moved={moved}", from.1));
                if iters % 8 == 0 {
                    println!("    ore: searching for {ore} — y={} dir=({dx},{dz}) have={}", from.1, count_ore_resource(bot, ore));
                }
            }
        }
    }

    let n = count_ore_resource(bot, ore);
    mem.log("mine_ore", "end", &format!("{n}/{target} {ore}"));
    if n >= target {
        // Surface before returning. The bot is dozens of blocks down its mine shaft
        // (iron mining ran to y-34); the NEXT steps — gather wood, water, food — are
        // SURFACE tasks that roam in place forever from down here (the lead race bot
        // stalled at Gather Wood at y-34 with 14 iron, oscillating 5 blocks from a
        // buried home). Climb out now, while we still hold the cobble we just mined.
        crate::tasks::portal::climb_out_of_pit(bot).await;
        success(format!("mined {n}/{target} {ore}"))
    } else {
        failure(format!("mined {n}/{target} {ore}"))
    }
}

/// Mine gravel until we have `target` flint (gravel drops flint ~10%).
pub async fn mine_gravel_for_flint(bot: &mut Bot<'_>, target: i32, mem: &mut WorldMemory) -> StepResult {
    record_descent(bot, mem);
    bot.movement.blocks_cant_break.clear();
    // Strip-mining to reach gravel digs stone via dig_down/strip_tunnel (bot.dig, whatever's
    // in hand). After the bucket/water steps the bot holds a bucket, not a pickaxe, so it
    // can't break stone and never reaches the gravel — the same bare-handed stall the portal
    // descent hit. Equip a pickaxe first.
    ensure_pickaxe(bot).await;
    let count = |bot: &Bot| -> i32 {
        bot.inventory.slots.iter().flatten().filter(|i| i.name == "flint").map(|i| i.count).sum()
    };
    // Gravel is sparse in forest/plains — a narrow ±20 scan often finds none, so the bot
    // strip-mines blind for many minutes (seen: 0 gravel in 13 min). Search a MUCH wider
    // radius with the efficient block index so we lock onto the nearest gravel blob and
    // walk to it instead of tunnelling past it.
    let find_gravel = |bot: &Bot| -> Option<(i32, i32, i32)> { bot.find_block("gravel", 40) };
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut blacklist = std::collections::HashSet::new();
    // Forest/plains SURFACE has almost no gravel — it's common underground (and near
    // water). The old code dug straight down and gave up (`break`) the instant a
    // liquid/cavern blocked the shaft, so a bot with everything-but-flint could spin
    // for many minutes. Instead: get below the surface band, then STRIP-MINE
    // horizontally — rotating heading whenever a tunnel/dig is blocked — so we keep
    // exposing fresh walls until a gravel pocket turns up. Never give up early.
    const STRIP_BELOW: i32 = 58;
    let dirs = [(1, 0), (0, 1), (-1, 0), (0, -1)];
    let mut dir = 0usize;
    while count(bot) < target && Instant::now() < deadline && !bot.breath_alarm {
        if let Some(pos) = find_gravel(bot).filter(|p| !blacklist.contains(p)) {
            let _ = bot.goto_near(pos.0, pos.1, pos.2, 2.0).await;
            let _ = bot.dig_toward(pos.0, pos.1, pos.2).await;
            collect_drops(bot, pos.0, pos.2).await;
            blacklist.insert(pos);
            continue;
        }
        // Nothing in view: descend toward the gravel band, else tunnel sideways.
        let feet = (bot.entity.position.y - 0.5).floor() as i32;
        let (dx, dz) = dirs[dir % dirs.len()];
        let moved = if feet > STRIP_BELOW {
            dig_down(bot).await || strip_tunnel(bot, dx, dz).await
        } else {
            strip_tunnel(bot, dx, dz).await || dig_down(bot).await
        };
        if !moved {
            dir += 1; // wedged (liquid/edge) — turn and try a fresh heading
            bot.wait_ticks(2).await.ok();
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
    // In water: swim out first. find_stone (radius 6) picks stone under the lake bed and the dig-down
    // pulls the bot back under, undoing every escape (water_wall_i8: escapes covered 6–12 blocks toward
    // the shore, then mine_stone dove again; 0/10 → 2/10 with the lily-pad fixes alone). Each call
    // resumes the swim from where the bot is.
    // Only when actually swimming: a bot standing in shallow water (on the ground, head out) mines
    // normally. Failing it too made a 726-call loop in one water_wall_i8 trial.
    if crate::bot_utils::feet_in_water(bot) && !bot.entity.on_ground {
        crate::bot_utils::leave_water(bot, 800).await;
        if crate::bot_utils::feet_in_water(bot) && !bot.entity.on_ground {
            return failure("stone: in water — swimming to shore first");
        }
    }
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

    let deadline = Instant::now() + Duration::from_secs(120);
    let mut no_progress = 0;
    // When digging down is blocked (liquid/cliff/sand-over-water below) we tunnel
    // sideways to a diggable spot instead of bailing — a bot at a bad lane spot used
    // to mine 0/16 cobblestone for the entire race, looping `dig_down → break → retry`.
    let dirs = [(1, 0), (0, 1), (-1, 0), (0, -1)];
    let mut dir = 0usize;
    while count_cobble(bot) < target && Instant::now() < deadline && !bot.breath_alarm {
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
        if let Some((tx, ty, tz)) = find_stone(bot, 6) {
            // If the stone is far below us, dig DOWN to it first so we reach the
            // same level and can collect drops. Mining remotely from the surface
            // loses every drop (the item falls 5+ blocks into the hole). But don't
            // `continue` — fall through to the dig so no_progress advances even
            // if dig_down fails (otherwise the loop burns 25 iterations in seconds).
            let feet = bot.entity.position.y.floor() as i32;
            if ty < feet - 2 {
                dig_down(bot).await;
            }
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
                // Reached but unminable (encased / can't path) — go down, else tunnel.
                if no_progress > 4 && !dig_down(bot).await {
                    let (dx, dz) = dirs[dir % dirs.len()];
                    dir += 1;
                    strip_tunnel(bot, dx, dz).await;
                }
            }
        } else {
            // No stone in reach — dig down toward the stone layer; if that's blocked
            // (liquid/cliff below), tunnel sideways to fresh, diggable ground.
            if !dig_down(bot).await {
                let (dx, dz) = dirs[dir % dirs.len()];
                dir += 1;
                strip_tunnel(bot, dx, dz).await;
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

#[cfg(test)]
mod tests {
    use super::{decide_mine_move, MineMove, MineObs};

    // A baseline "at depth, ore left to find, nothing wrong" observation. Each test
    // perturbs ONE field to prove the pure decision reacts to exactly that part of state.
    fn base() -> MineObs {
        MineObs {
            is_iron: true,
            y: 15,
            depth: 15,
            stuck: 0,
            desc_fail: 0,
            dry_veins: 0,
            gain_iters: 0,
            nearest_ore: None,
            strip_dir: (0, 1),
            over_water: false,
        }
    }

    #[test]
    fn over_water_relocates_instead_of_descending() {
        // Above the band AND in a water body → range-relocate to land, not Descend.
        let o = MineObs { y: 60, over_water: true, ..base() };
        assert_eq!(decide_mine_move(&o), MineMove::RangeJump);
    }

    #[test]
    fn over_water_still_mines_a_reachable_vein_first() {
        // A vein right here beats fleeing the water.
        let o = MineObs { over_water: true, nearest_ore: Some((1, 2, 3)), ..base() };
        assert_eq!(decide_mine_move(&o), MineMove::MineVein((1, 2, 3)));
    }

    #[test]
    fn void_is_climbed_before_anything_else() {
        let o = MineObs { y: -5, nearest_ore: Some((1, 2, 3)), stuck: 99, ..base() };
        assert_eq!(decide_mine_move(&o), MineMove::ClimbFromVoid);
    }

    #[test]
    fn void_only_applies_to_iron() {
        // Coal at y-5 is fine — no void climb, it just keeps searching at depth.
        let o = MineObs { is_iron: false, y: -5, ..base() };
        assert_eq!(decide_mine_move(&o), MineMove::StripTunnel((0, 1)));
    }

    #[test]
    fn wedged_relocates() {
        let o = MineObs { stuck: 31, ..base() };
        assert_eq!(decide_mine_move(&o), MineMove::Relocate);
    }

    #[test]
    fn known_reachable_vein_is_mined() {
        let o = MineObs { nearest_ore: Some((4, 5, 6)), ..base() };
        assert_eq!(decide_mine_move(&o), MineMove::MineVein((4, 5, 6)));
    }

    #[test]
    fn too_many_dry_veins_stops_chasing_sightings() {
        // Even with a sighting, past the dry-vein limit we change depth instead of grinding.
        let o = MineObs { nearest_ore: Some((4, 5, 6)), dry_veins: 4, y: 60, ..base() };
        assert_eq!(decide_mine_move(&o), MineMove::Descend);
    }

    #[test]
    fn above_the_band_descends() {
        let o = MineObs { y: 60, ..base() };
        assert_eq!(decide_mine_move(&o), MineMove::Descend);
    }

    #[test]
    fn stalled_descent_high_up_still_pushes_down() {
        // desc_fail high but still above y55 → keep descending (don't settle too shallow).
        let o = MineObs { y: 70, desc_fail: 9, ..base() };
        assert_eq!(decide_mine_move(&o), MineMove::Descend);
    }

    #[test]
    fn stalled_descent_near_band_searches_here() {
        // desc_fail high and only just above the band → stop insisting on depth, search here.
        let o = MineObs { y: 40, depth: 15, desc_fail: 9, ..base() };
        assert_eq!(decide_mine_move(&o), MineMove::StripTunnel((0, 1)));
    }

    #[test]
    fn mined_out_box_jumps_far() {
        let o = MineObs { gain_iters: 25, ..base() };
        assert_eq!(decide_mine_move(&o), MineMove::RangeJump);
    }

    #[test]
    fn otherwise_strip_tunnels_the_current_heading() {
        let o = MineObs { strip_dir: (1, 0), ..base() };
        assert_eq!(decide_mine_move(&o), MineMove::StripTunnel((1, 0)));
    }
}
